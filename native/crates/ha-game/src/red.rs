//! **La red del multijugador: LAN y online, un solo protocolo.**
//!
//! Sin dependencias nuevas: `std::net` + `serde_json`, una línea de JSON por
//! mensaje. La decisión de diseño (biblia §15) es que **cada cliente corre su
//! mundo determinista** —la semilla alcanza para que todos vean el mismo
//! terreno— y la red sólo transporta lo que NO se puede deducir de la semilla:
//! quién está, dónde, hacia dónde mira y lo que dice.
//!
//! Hay dos formas de tener un `Hub` (el que reparte los mensajes):
//! * **LAN**: el anfitrión lo levanta dentro de su propio juego (`Hub::levantar`)
//!   y avisa por UDP broadcast (`Aviso`); los demás lo ven en MULTIJUGADOR.
//! * **Online**: `server/src/relay.ts` es el mismo hub en Node, siempre prendido.
//!
//! El hub NO simula nada: asigna ids, guarda el último estado de cada uno para
//! el que llega tarde y reenvía. Desde la v2 (3 oct 2026) además:
//! * guarda el **estado del mundo** de la sala como un mapa clave → valor
//!   (`Ed`): la última pared, losa, cofre o árbol talado de cada clave. El que
//!   entra tarde lo recibe entero y después un `Listo`;
//! * reparte **eventos** (`Ev`) a todos o a un id (`para`): golpes a un
//!   infectado, la foto de los infectados, la hora, una puerta;
//! * nombra **árbitro** al id más viejo de la sala (`Arb`): él decide lo que
//!   no es determinista (aparición de infectados, la hora). Si se va, el
//!   siguiente toma la posta — no hay un "servidor" del que dependa la partida;
//! * rechaza clientes de otra versión del protocolo (`Error`) en vez de dejar
//!   que se desincronicen en silencio.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{SocketAddr, TcpListener, TcpStream, ToSocketAddrs, UdpSocket};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc, Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

pub const PUERTO: u16 = 2568;
pub const PUERTO_AVISO: u16 = 2569;
pub const MAX_JUGADORES: usize = 8;
/// Versión del protocolo. Sube cuando un mensaje cambia de forma: un cliente de
/// otra versión recibe `Error` y no entra (biblia §15).
pub const PROTOCOLO: u32 = 2;
/// Una línea más larga que esto es basura o un ataque: se corta al que la manda.
/// 8 KB: un cofre lleno con sus 24 pilas entra holgado.
const MAX_LINEA: u64 = 8192;
/// Tope de claves del mundo de una sala: un cliente roto no llena la memoria del hub.
pub const MAX_CLAVES_MUNDO: usize = 200_000;
const MAX_NOMBRE: usize = 24;
const MAX_CHAT: usize = 140;

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(tag = "t")]
pub enum Msg {
    /// Cliente → hub, lo primero que dice. `semilla` 0 = "no tengo mundo, dame el
    /// de la sala".
    #[serde(rename = "hola")]
    Hola {
        nombre: String,
        piel: String,
        semilla: u32,
        /// Versión del protocolo; 0 = cliente v1 (se rechaza).
        #[serde(default)]
        ver: u32,
        /// Sala del relay online ("" = la pública). El hub de LAN la ignora.
        #[serde(default)]
        sala: String,
    },
    /// Hub → cliente: tu id, el mundo de la sala y quién arbitra.
    #[serde(rename = "id")]
    Id {
        id: u32,
        semilla: u32,
        #[serde(default)]
        arbitro: u32,
    },
    /// Cambió el árbitro (el anterior se fue).
    #[serde(rename = "arb")]
    Arb { id: u32 },
    /// Estado del mundo: la clave `k` vale `v` (`null` = ya no existe). El hub
    /// guarda el último de cada clave y se lo pasa al que llega tarde.
    #[serde(rename = "ed")]
    Ed { k: String, v: serde_json::Value },
    /// Ya llegó todo el estado guardado de la sala.
    #[serde(rename = "listo")]
    Listo,
    /// Evento pasajero (no se guarda). `para` 0 = todos los demás; si no, sólo a
    /// ese id. `de` lo pone el hub.
    #[serde(rename = "ev")]
    Ev {
        #[serde(default)]
        de: u32,
        #[serde(default)]
        para: u32,
        k: String,
        #[serde(default)]
        v: serde_json::Value,
    },
    /// El hub no te deja entrar (versión distinta, sala llena…).
    #[serde(rename = "error")]
    Error { msg: String },
    /// Cliente → hub: ¿qué salas hay? (sin entrar).
    #[serde(rename = "lista")]
    Lista,
    /// Hub → cliente: las salas abiertas.
    #[serde(rename = "salas")]
    Salas { salas: Vec<SalaInfo> },
    /// Alguien entró (hub → los demás, y los ya presentes → el recién llegado).
    #[serde(rename = "ent")]
    Ent { id: u32, nombre: String, piel: String },
    /// Dónde está. El cliente no manda `id`: lo pone el hub.
    #[serde(rename = "est")]
    Est {
        #[serde(default)]
        id: u32,
        x: f32,
        z: f32,
        cuerpo: u8,
        rumbo: f32,
        #[serde(default)]
        vuelo: bool,
        /// Puntos de aventura (metas) y bajas de infectados: el ranking de la sala.
        #[serde(default)]
        pts: u32,
        #[serde(default)]
        baj: u32,
        /// Clave del vehículo que maneja (`vehicle/coche_3`), o vacío si va a pie.
        /// Un cliente viejo no la manda y los demás lo ven caminar: compatible.
        #[serde(default)]
        veh: String,
    },
    #[serde(rename = "chat")]
    Chat {
        #[serde(default)]
        id: u32,
        msg: String,
    },
    #[serde(rename = "chau")]
    Chau { id: u32 },
}

/// Una sala abierta, como la lista el relay online (o el hub de LAN, que tiene una).
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct SalaInfo {
    pub sala: String,
    pub semilla: u32,
    pub jugadores: u32,
    /// Nombre del árbitro (el que la abrió, mientras siga).
    #[serde(default)]
    pub anfitrion: String,
}

/// El saludo de un cliente de esta versión.
pub fn hola(nombre: &str, piel: &str, semilla: u32, sala: &str) -> Msg {
    Msg::Hola { nombre: nombre.to_string(), piel: piel.to_string(), semilla, ver: PROTOCOLO, sala: sala.to_string() }
}

fn limpiar(s: &str, max: usize) -> String {
    s.chars().filter(|c| !c.is_control()).take(max).collect()
}

// ---------------------------------------------------------------------------
// Hub
// ---------------------------------------------------------------------------

struct Par {
    salida: TcpStream,
    nombre: String,
    piel: String,
    ultimo: Option<Msg>,
}

struct EstadoHub {
    siguiente: u32,
    semilla: u32,
    pares: HashMap<u32, Par>,
    /// El estado del mundo de la sala (clave → último `Ed`).
    mundo: HashMap<String, Msg>,
    arbitro: u32,
}

pub struct Hub {
    vivo: Arc<AtomicBool>,
    estado: Arc<Mutex<EstadoHub>>,
    pub puerto: u16,
}

fn enviar(s: &mut TcpStream, m: &Msg) -> bool {
    let Ok(mut l) = serde_json::to_string(m) else { return false };
    l.push('\n');
    s.write_all(l.as_bytes()).is_ok()
}

impl EstadoHub {
    /// Manda `m` a todos menos a `salvo`; los que no responden se descartan.
    fn difundir(&mut self, m: &Msg, salvo: Option<u32>) -> Vec<u32> {
        let mut caidos = Vec::new();
        for (id, p) in self.pares.iter_mut() {
            if Some(*id) == salvo {
                continue;
            }
            if !enviar(&mut p.salida, m) {
                caidos.push(*id);
            }
        }
        caidos
    }

    fn quitar(&mut self, id: u32) {
        if self.pares.remove(&id).is_some() {
            let mut caidos = self.difundir(&Msg::Chau { id }, None);
            if self.arbitro == id {
                // La posta pasa al más viejo que queda.
                self.arbitro = self.pares.keys().min().copied().unwrap_or(0);
                if self.arbitro != 0 {
                    let arb = Msg::Arb { id: self.arbitro };
                    caidos.extend(self.difundir(&arb, None));
                }
            }
            for c in caidos {
                self.quitar(c);
            }
        }
    }

    fn mandar_a(&mut self, id: u32, m: &Msg) -> bool {
        match self.pares.get_mut(&id) {
            Some(p) => enviar(&mut p.salida, m),
            None => true,
        }
    }

    fn info(&self) -> SalaInfo {
        SalaInfo {
            sala: String::new(),
            semilla: self.semilla,
            jugadores: self.pares.len() as u32,
            anfitrion: self.pares.get(&self.arbitro).map(|p| p.nombre.clone()).unwrap_or_default(),
        }
    }
}

impl Hub {
    /// Levanta el hub en `puerto` (0 = el que dé el sistema) para el mundo `semilla`.
    pub fn levantar(puerto: u16, semilla: u32) -> std::io::Result<Hub> {
        let l = TcpListener::bind(("0.0.0.0", puerto))?;
        l.set_nonblocking(true)?;
        let puerto = l.local_addr()?.port();
        let vivo = Arc::new(AtomicBool::new(true));
        let estado = Arc::new(Mutex::new(EstadoHub {
            siguiente: 1,
            semilla,
            pares: HashMap::new(),
            mundo: HashMap::new(),
            arbitro: 0,
        }));
        {
            let (vivo, estado) = (vivo.clone(), estado.clone());
            thread::spawn(move || {
                while vivo.load(Ordering::Relaxed) {
                    match l.accept() {
                        Ok((s, _)) => {
                            let (vivo, estado) = (vivo.clone(), estado.clone());
                            thread::spawn(move || atender(s, vivo, estado));
                        }
                        Err(_) => thread::sleep(Duration::from_millis(60)),
                    }
                }
            });
        }
        Ok(Hub { vivo, estado, puerto })
    }

    pub fn jugadores(&self) -> usize {
        self.estado.lock().map(|e| e.pares.len()).unwrap_or(0)
    }

    /// Un contador compartido de jugadores, para el aviso de LAN.
    pub fn contador(&self) -> impl Fn() -> usize + Send + 'static {
        let e = self.estado.clone();
        move || e.lock().map(|e| e.pares.len()).unwrap_or(0)
    }

    /// Cuántas claves del mundo guarda la sala (pruebas y consola).
    pub fn claves_mundo(&self) -> usize {
        self.estado.lock().map(|e| e.mundo.len()).unwrap_or(0)
    }
}

impl Drop for Hub {
    fn drop(&mut self) {
        self.vivo.store(false, Ordering::Relaxed);
        // Soltar los sockets cierra los hilos lectores.
        if let Ok(mut e) = self.estado.lock() {
            e.pares.clear();
        }
    }
}

fn atender(s: TcpStream, vivo: Arc<AtomicBool>, estado: Arc<Mutex<EstadoHub>>) {
    let _ = s.set_nodelay(true);
    let _ = s.set_read_timeout(Some(Duration::from_secs(2)));
    let _ = s.set_write_timeout(Some(Duration::from_millis(300)));
    let Ok(salida) = s.try_clone() else { return };
    let mut lector = BufReader::new(s);
    let mut mi_id: Option<u32> = None;
    let mut linea = String::new();
    let mut ultimo_dato = Instant::now();
    while vivo.load(Ordering::Relaxed) {
        linea.clear();
        match (&mut lector).take(MAX_LINEA).read_line(&mut linea) {
            Ok(0) => break,
            Ok(_) => {
                if !linea.ends_with('\n') {
                    break; // línea desmedida
                }
                ultimo_dato = Instant::now();
            }
            Err(e) if matches!(e.kind(), std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut) => {
                // Sin pulso en 30 s = caído sin avisar.
                if ultimo_dato.elapsed() > Duration::from_secs(30) {
                    break;
                }
                continue;
            }
            Err(_) => break,
        }
        let Ok(m) = serde_json::from_str::<Msg>(linea.trim()) else { continue };
        let Ok(mut e) = estado.lock() else { break };
        match (m, mi_id) {
            (Msg::Lista, None) => {
                let Ok(mut mia) = salida.try_clone() else { break };
                enviar(&mut mia, &Msg::Salas { salas: vec![e.info()] });
            }
            (Msg::Hola { nombre, piel, ver, .. }, None) => {
                let Ok(mut mia) = salida.try_clone() else { break };
                if ver != PROTOCOLO {
                    enviar(&mut mia, &Msg::Error { msg: format!("versión distinta (sala v{PROTOCOLO}, tu juego v{ver}): actualizá") });
                    break;
                }
                if e.pares.len() >= MAX_JUGADORES {
                    enviar(&mut mia, &Msg::Error { msg: format!("sala llena ({MAX_JUGADORES})") });
                    break;
                }
                let id = e.siguiente;
                e.siguiente += 1;
                if e.arbitro == 0 {
                    e.arbitro = id;
                }
                let (nombre, piel) = (limpiar(&nombre, MAX_NOMBRE), limpiar(&piel, 40));
                let (semilla, arbitro) = (e.semilla, e.arbitro);
                if !enviar(&mut mia, &Msg::Id { id, semilla, arbitro }) {
                    break;
                }
                // Los presentes se presentan al recién llegado, con su último estado.
                for (pid, p) in e.pares.iter() {
                    enviar(&mut mia, &Msg::Ent { id: *pid, nombre: p.nombre.clone(), piel: p.piel.clone() });
                    if let Some(u) = &p.ultimo {
                        enviar(&mut mia, u);
                    }
                }
                // El mundo de la sala tal como quedó, y la marca de que terminó.
                for m in e.mundo.values() {
                    if !enviar(&mut mia, m) {
                        break;
                    }
                }
                enviar(&mut mia, &Msg::Listo);
                let ent = Msg::Ent { id, nombre: nombre.clone(), piel: piel.clone() };
                let caidos = e.difundir(&ent, None);
                e.pares.insert(id, Par { salida: mia, nombre, piel, ultimo: None });
                for c in caidos {
                    e.quitar(c);
                }
                mi_id = Some(id);
            }
            (Msg::Est { x, z, cuerpo, rumbo, vuelo, pts, baj, veh, .. }, Some(id)) => {
                if !(x.is_finite() && z.is_finite() && rumbo.is_finite()) {
                    continue;
                }
                let m = Msg::Est { id, x, z, cuerpo, rumbo, vuelo, pts, baj, veh: limpiar(&veh, 40) };
                if let Some(p) = e.pares.get_mut(&id) {
                    p.ultimo = Some(m.clone());
                }
                let caidos = e.difundir(&m, Some(id));
                for c in caidos {
                    e.quitar(c);
                }
            }
            (Msg::Chat { msg, .. }, Some(id)) => {
                let m = Msg::Chat { id, msg: limpiar(&msg, MAX_CHAT) };
                let caidos = e.difundir(&m, None);
                for c in caidos {
                    e.quitar(c);
                }
            }
            (Msg::Ed { k, v }, Some(id)) => {
                if k.is_empty() || k.len() > 96 {
                    continue;
                }
                if !e.mundo.contains_key(&k) && e.mundo.len() >= MAX_CLAVES_MUNDO {
                    continue;
                }
                let m = Msg::Ed { k: k.clone(), v };
                e.mundo.insert(k, m.clone());
                let caidos = e.difundir(&m, Some(id));
                for c in caidos {
                    e.quitar(c);
                }
            }
            (Msg::Ev { para, k, v, .. }, Some(id)) => {
                let m = Msg::Ev { de: id, para, k: limpiar(&k, 24), v };
                let caidos = if para == 0 {
                    e.difundir(&m, Some(id))
                } else if e.mandar_a(para, &m) {
                    Vec::new()
                } else {
                    vec![para]
                };
                for c in caidos {
                    e.quitar(c);
                }
            }
            _ => {}
        }
    }
    if let Some(id) = mi_id {
        if let Ok(mut e) = estado.lock() {
            e.quitar(id);
        }
    }
}

// ---------------------------------------------------------------------------
// Cliente
// ---------------------------------------------------------------------------

#[derive(Debug)]
pub enum Evento {
    Mensaje(Msg),
    /// La conexión se cortó (o no se pudo abrir). El texto es para mostrar.
    Caido(String),
}

pub struct Cliente {
    pub rx: mpsc::Receiver<Evento>,
    tx: mpsc::Sender<Msg>,
    vivo: Arc<AtomicBool>,
}

impl Cliente {
    /// Conecta en segundo plano: `rx` avisa si salió bien (llega `Id`) o mal.
    pub fn conectar(destino: String, hola: Msg) -> Cliente {
        let (etx, rx) = mpsc::channel();
        let (tx, srx) = mpsc::channel::<Msg>();
        let vivo = Arc::new(AtomicBool::new(true));
        let v = vivo.clone();
        thread::spawn(move || {
            let addr = destino.to_socket_addrs().ok().and_then(|mut a| a.next());
            let Some(addr) = addr else {
                let _ = etx.send(Evento::Caido(format!("dirección inválida: {destino}")));
                return;
            };
            let s = match TcpStream::connect_timeout(&addr, Duration::from_secs(4)) {
                Ok(s) => s,
                Err(e) => {
                    let _ = etx.send(Evento::Caido(format!("no se pudo conectar: {e}")));
                    return;
                }
            };
            let _ = s.set_nodelay(true);
            let _ = s.set_read_timeout(Some(Duration::from_millis(500)));
            let _ = s.set_write_timeout(Some(Duration::from_secs(2)));
            let Ok(mut salida) = s.try_clone() else { return };
            if !enviar(&mut salida, &hola) {
                let _ = etx.send(Evento::Caido("se cortó al saludar".into()));
                return;
            }
            // Escritor.
            let v2 = v.clone();
            thread::spawn(move || {
                while v2.load(Ordering::Relaxed) {
                    match srx.recv_timeout(Duration::from_millis(500)) {
                        Ok(m) => {
                            if !enviar(&mut salida, &m) {
                                break;
                            }
                        }
                        Err(mpsc::RecvTimeoutError::Timeout) => {
                            // Pulso para que el hub no nos dé por caídos.
                            if !enviar(&mut salida, &Msg::Chau { id: 0 }) {
                                break;
                            }
                        }
                        Err(_) => break,
                    }
                }
            });
            let mut lector = BufReader::new(s);
            let mut linea = String::new();
            while v.load(Ordering::Relaxed) {
                linea.clear();
                match (&mut lector).take(MAX_LINEA * 4).read_line(&mut linea) {
                    Ok(0) => {
                        let _ = etx.send(Evento::Caido("el anfitrión cerró".into()));
                        return;
                    }
                    Ok(_) => {
                        if let Ok(m) = serde_json::from_str::<Msg>(linea.trim()) {
                            if etx.send(Evento::Mensaje(m)).is_err() {
                                return;
                            }
                        }
                    }
                    Err(e) if matches!(e.kind(), std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut) => {}
                    Err(e) => {
                        let _ = etx.send(Evento::Caido(format!("se cortó: {e}")));
                        return;
                    }
                }
            }
        });
        Cliente { rx, tx, vivo }
    }

    pub fn enviar(&self, m: Msg) {
        let _ = self.tx.send(m);
    }
}

impl Drop for Cliente {
    fn drop(&mut self) {
        self.vivo.store(false, Ordering::Relaxed);
    }
}

// ---------------------------------------------------------------------------
// Aviso por LAN (UDP broadcast)
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, PartialEq)]
pub struct Anfitrion {
    pub direccion: String,
    pub nombre: String,
    pub semilla: u32,
    pub jugadores: u32,
    pub visto: Instant,
}

fn paquete(puerto: u16, nombre: &str, semilla: u32, jugadores: usize) -> String {
    format!("HAMUNDO|{puerto}|{semilla}|{jugadores}|{}", limpiar(&nombre.replace('|', " "), MAX_NOMBRE))
}

fn leer_paquete(txt: &str, ip: std::net::IpAddr) -> Option<Anfitrion> {
    let mut p = txt.trim().splitn(5, '|');
    if p.next()? != "HAMUNDO" {
        return None;
    }
    let puerto: u16 = p.next()?.parse().ok()?;
    let semilla: u32 = p.next()?.parse().ok()?;
    let jugadores: u32 = p.next()?.parse().ok()?;
    let nombre = p.next()?.to_string();
    Some(Anfitrion { direccion: format!("{ip}:{puerto}"), nombre, semilla, jugadores, visto: Instant::now() })
}

/// Grita cada 2 s "acá hay un mundo" mientras viva.
pub struct Aviso {
    vivo: Arc<AtomicBool>,
}

impl Aviso {
    pub fn emitir(hub_puerto: u16, nombre: String, semilla: u32, cuenta: impl Fn() -> usize + Send + 'static) -> Option<Aviso> {
        let s = UdpSocket::bind("0.0.0.0:0").ok()?;
        s.set_broadcast(true).ok()?;
        let vivo = Arc::new(AtomicBool::new(true));
        let v = vivo.clone();
        thread::spawn(move || {
            while v.load(Ordering::Relaxed) {
                let p = paquete(hub_puerto, &nombre, semilla, cuenta());
                let _ = s.send_to(p.as_bytes(), ("255.255.255.255", PUERTO_AVISO));
                thread::sleep(Duration::from_secs(2));
            }
        });
        Some(Aviso { vivo })
    }
}

impl Drop for Aviso {
    fn drop(&mut self) {
        self.vivo.store(false, Ordering::Relaxed);
    }
}

/// Escucha los avisos de la LAN.
pub struct Buscador {
    pub rx: mpsc::Receiver<Anfitrion>,
    vivo: Arc<AtomicBool>,
}

impl Buscador {
    pub fn escuchar() -> Option<Buscador> {
        Self::escuchar_en(PUERTO_AVISO)
    }

    pub fn escuchar_en(puerto: u16) -> Option<Buscador> {
        let s = UdpSocket::bind(("0.0.0.0", puerto)).ok()?;
        s.set_read_timeout(Some(Duration::from_millis(500))).ok()?;
        let (tx, rx) = mpsc::channel();
        let vivo = Arc::new(AtomicBool::new(true));
        let v = vivo.clone();
        thread::spawn(move || {
            let mut buf = [0u8; 256];
            while v.load(Ordering::Relaxed) {
                if let Ok((n, de)) = s.recv_from(&mut buf) {
                    if let Some(a) = std::str::from_utf8(&buf[..n]).ok().and_then(|t| leer_paquete(t, de.ip())) {
                        if tx.send(a).is_err() {
                            return;
                        }
                    }
                }
            }
        });
        Some(Buscador { rx, vivo })
    }
}

impl Drop for Buscador {
    fn drop(&mut self) {
        self.vivo.store(false, Ordering::Relaxed);
    }
}

/// La primera IPv4 propia que no sea loopback: lo que el anfitrión le dicta a un
/// amigo que está fuera de la LAN (o cuando el broadcast no pasa).
pub fn ip_local() -> Option<String> {
    // Conectar un UDP no manda nada: sólo elige la interfaz de salida.
    let s = UdpSocket::bind("0.0.0.0:0").ok()?;
    s.connect("192.0.2.1:9").ok()?;
    match s.local_addr().ok()? {
        SocketAddr::V4(a) if !a.ip().is_loopback() && !a.ip().is_unspecified() => Some(a.ip().to_string()),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn esperar<T>(rx: &mpsc::Receiver<Evento>, mut f: impl FnMut(&Msg) -> Option<T>) -> T {
        let fin = Instant::now() + Duration::from_secs(5);
        while Instant::now() < fin {
            if let Ok(Evento::Mensaje(m)) = rx.recv_timeout(Duration::from_millis(200)) {
                if let Some(r) = f(&m) {
                    return r;
                }
            }
        }
        panic!("no llegó el mensaje esperado");
    }

    #[test]
    fn dos_clientes_se_ven_y_se_hablan_por_el_hub() {
        let hub = Hub::levantar(0, 4242).expect("hub");
        let dir = format!("127.0.0.1:{}", hub.puerto);
        let a = Cliente::conectar(dir.clone(), hola("Ana", "p1", 0, ""));
        let id_a = esperar(&a.rx, |m| if let Msg::Id { id, semilla, arbitro } = m { assert_eq!(*semilla, 4242); assert_eq!(*arbitro, *id); Some(*id) } else { None });
        a.enviar(Msg::Est { id: 0, x: 3.0, z: 4.0, cuerpo: 0, rumbo: 1.0, vuelo: false, pts: 0, baj: 0, veh: "vehicle/coche_3".into() });
        std::thread::sleep(Duration::from_millis(150));

        let b = Cliente::conectar(dir, hola("Beto\n", "p2", 4242, ""));
        let id_b = esperar(&b.rx, |m| if let Msg::Id { id, .. } = m { Some(*id) } else { None });
        assert_ne!(id_a, id_b);
        // Beto recibe a Ana y su último estado, aunque llegó tarde.
        esperar(&b.rx, |m| matches!(m, Msg::Ent { id, nombre, .. } if *id == id_a && nombre == "Ana").then_some(()));
        esperar(&b.rx, |m| matches!(m, Msg::Est { id, x, veh, .. } if *id == id_a && *x == 3.0 && veh == "vehicle/coche_3").then_some(()));
        // Ana ve entrar a Beto (con el nombre limpio) y le llega su chat.
        esperar(&a.rx, |m| matches!(m, Msg::Ent { id, nombre, .. } if *id == id_b && nombre == "Beto").then_some(()));
        b.enviar(Msg::Chat { id: 999, msg: "hola".into() });
        // El hub pone el id verdadero: el 999 que mandó Beto no se respeta.
        esperar(&a.rx, |m| matches!(m, Msg::Chat { id, msg } if *id == id_b && msg == "hola").then_some(()));
        assert_eq!(hub.jugadores(), 2);

        drop(b);
        esperar(&a.rx, |m| matches!(m, Msg::Chau { id } if *id == id_b).then_some(()));
    }

    #[test]
    fn el_aviso_de_lan_se_lee_y_lo_raro_se_ignora() {
        let p = paquete(2568, "Mundo|raro", 777, 2);
        let a = leer_paquete(&p, "192.168.1.9".parse().unwrap()).expect("aviso");
        assert_eq!(a.direccion, "192.168.1.9:2568");
        assert_eq!((a.semilla, a.jugadores), (777, 2));
        assert_eq!(a.nombre, "Mundo raro");
        assert!(leer_paquete("hola mundo", "10.0.0.1".parse().unwrap()).is_none());
        assert!(leer_paquete("HAMUNDO|x|1|1|n", "10.0.0.1".parse().unwrap()).is_none());
    }

    #[test]
    fn un_hub_lleno_rechaza_y_las_lineas_desmedidas_cortan() {
        let hub = Hub::levantar(0, 1).expect("hub");
        let dir = format!("127.0.0.1:{}", hub.puerto);
        let mut s = TcpStream::connect(&dir).unwrap();
        s.write_all(&vec![b'a'; 5000]).unwrap();
        s.write_all(b"\n").unwrap();
        std::thread::sleep(Duration::from_millis(300));
        assert_eq!(hub.jugadores(), 0);
    }

    #[test]
    fn un_cliente_de_otra_version_no_entra_y_se_entera() {
        let hub = Hub::levantar(0, 5).expect("hub");
        let viejo = Cliente::conectar(
            format!("127.0.0.1:{}", hub.puerto),
            Msg::Hola { nombre: "Viejo".into(), piel: "p".into(), semilla: 0, ver: 1, sala: String::new() },
        );
        esperar(&viejo.rx, |m| matches!(m, Msg::Error { msg } if msg.contains("versión")).then_some(()));
        assert_eq!(hub.jugadores(), 0);
    }

    #[test]
    fn el_que_llega_tarde_recibe_el_mundo_y_los_eventos_van_a_quien_deben() {
        let hub = Hub::levantar(0, 9).expect("hub");
        let dir = format!("127.0.0.1:{}", hub.puerto);
        let a = Cliente::conectar(dir.clone(), hola("Ana", "p", 9, ""));
        let id_a = esperar(&a.rx, |m| if let Msg::Id { id, .. } = m { Some(*id) } else { None });
        // Ana edita: una pared puesta, una losa puesta y después sacada (la última gana).
        a.enviar(Msg::Ed { k: "w|0|0|1|2|0".into(), v: serde_json::json!(3) });
        a.enviar(Msg::Ed { k: "t|0|0|5|5".into(), v: serde_json::json!("Stone") });
        a.enviar(Msg::Ed { k: "t|0|0|5|5".into(), v: serde_json::Value::Null });
        std::thread::sleep(Duration::from_millis(200));
        assert_eq!(hub.claves_mundo(), 2);

        let b = Cliente::conectar(dir.clone(), hola("Beto", "p", 9, ""));
        let (id_b, arb) = esperar(&b.rx, |m| if let Msg::Id { id, arbitro, .. } = m { Some((*id, *arbitro)) } else { None });
        assert_eq!(arb, id_a, "el más viejo arbitra");
        let mut visto = HashMap::new();
        esperar(&b.rx, |m| match m {
            Msg::Ed { k, v } => {
                visto.insert(k.clone(), v.clone());
                None
            }
            Msg::Listo => Some(()),
            _ => None,
        });
        assert_eq!(visto.get("w|0|0|1|2|0"), Some(&serde_json::json!(3)));
        assert_eq!(visto.get("t|0|0|5|5"), Some(&serde_json::Value::Null));

        // Un evento dirigido sólo le llega a su destinatario, con el remitente que pone el hub.
        let c = Cliente::conectar(dir, hola("Caro", "p", 9, ""));
        let id_c = esperar(&c.rx, |m| if let Msg::Id { id, .. } = m { Some(*id) } else { None });
        b.enviar(Msg::Ev { de: 777, para: id_a, k: "golpe".into(), v: serde_json::json!({"n": 4}) });
        esperar(&a.rx, |m| matches!(m, Msg::Ev { de, k, .. } if *de == id_b && k == "golpe").then_some(()));
        let fin = Instant::now() + Duration::from_millis(400);
        while Instant::now() < fin {
            if let Ok(Evento::Mensaje(Msg::Ev { .. })) = c.rx.recv_timeout(Duration::from_millis(50)) {
                panic!("Caro recibió un evento que no era para ella");
            }
        }

        // Ana se va: arbitra el que sigue en antigüedad.
        drop(a);
        let nuevo = esperar(&c.rx, |m| if let Msg::Arb { id } = m { Some(*id) } else { None });
        assert_eq!(nuevo, id_b);
        assert!(id_c > id_b);
    }

    #[test]
    fn el_hub_de_lan_dice_su_sala_sin_entrar() {
        let hub = Hub::levantar(0, 31).expect("hub");
        let c = Cliente::conectar(format!("127.0.0.1:{}", hub.puerto), Msg::Lista);
        let salas = esperar(&c.rx, |m| if let Msg::Salas { salas } = m { Some(salas.clone()) } else { None });
        assert_eq!(salas.len(), 1);
        assert_eq!(salas[0].semilla, 31);
        assert_eq!(hub.jugadores(), 0);
    }
}
