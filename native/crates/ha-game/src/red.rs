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
//! el que llega tarde y reenvía. Una trampa (o un cliente roto) a lo sumo se ve
//! mal a sí mismo; no toca el mundo de nadie.

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
/// Una línea más larga que esto es basura o un ataque: se corta al que la manda.
const MAX_LINEA: u64 = 2048;
const MAX_NOMBRE: usize = 24;
const MAX_CHAT: usize = 140;

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(tag = "t")]
pub enum Msg {
    /// Cliente → hub, lo primero que dice. `semilla` 0 = "no tengo mundo, dame el
    /// de la sala".
    #[serde(rename = "hola")]
    Hola { nombre: String, piel: String, semilla: u32 },
    /// Hub → cliente: tu id y el mundo de la sala.
    #[serde(rename = "id")]
    Id { id: u32, semilla: u32 },
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
            let caidos = self.difundir(&Msg::Chau { id }, None);
            for c in caidos {
                self.quitar(c);
            }
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
        let estado = Arc::new(Mutex::new(EstadoHub { siguiente: 1, semilla, pares: HashMap::new() }));
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
            (Msg::Hola { nombre, piel, .. }, None) => {
                if e.pares.len() >= MAX_JUGADORES {
                    break;
                }
                let id = e.siguiente;
                e.siguiente += 1;
                let (nombre, piel) = (limpiar(&nombre, MAX_NOMBRE), limpiar(&piel, 40));
                let Ok(mut mia) = salida.try_clone() else { break };
                let semilla = e.semilla;
                if !enviar(&mut mia, &Msg::Id { id, semilla }) {
                    break;
                }
                // Los presentes se presentan al recién llegado, con su último estado.
                for (pid, p) in e.pares.iter() {
                    enviar(&mut mia, &Msg::Ent { id: *pid, nombre: p.nombre.clone(), piel: p.piel.clone() });
                    if let Some(u) = &p.ultimo {
                        enviar(&mut mia, u);
                    }
                }
                let ent = Msg::Ent { id, nombre: nombre.clone(), piel: piel.clone() };
                let caidos = e.difundir(&ent, None);
                e.pares.insert(id, Par { salida: mia, nombre, piel, ultimo: None });
                for c in caidos {
                    e.quitar(c);
                }
                mi_id = Some(id);
            }
            (Msg::Est { x, z, cuerpo, rumbo, vuelo, pts, baj, .. }, Some(id)) => {
                if !(x.is_finite() && z.is_finite() && rumbo.is_finite()) {
                    continue;
                }
                let m = Msg::Est { id, x, z, cuerpo, rumbo, vuelo, pts, baj };
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
        let a = Cliente::conectar(dir.clone(), Msg::Hola { nombre: "Ana".into(), piel: "p1".into(), semilla: 0 });
        let id_a = esperar(&a.rx, |m| if let Msg::Id { id, semilla } = m { assert_eq!(*semilla, 4242); Some(*id) } else { None });
        a.enviar(Msg::Est { id: 0, x: 3.0, z: 4.0, cuerpo: 0, rumbo: 1.0, vuelo: false, pts: 0, baj: 0 });
        std::thread::sleep(Duration::from_millis(150));

        let b = Cliente::conectar(dir, Msg::Hola { nombre: "Beto\n".into(), piel: "p2".into(), semilla: 4242 });
        let id_b = esperar(&b.rx, |m| if let Msg::Id { id, .. } = m { Some(*id) } else { None });
        assert_ne!(id_a, id_b);
        // Beto recibe a Ana y su último estado, aunque llegó tarde.
        esperar(&b.rx, |m| matches!(m, Msg::Ent { id, nombre, .. } if *id == id_a && nombre == "Ana").then_some(()));
        esperar(&b.rx, |m| matches!(m, Msg::Est { id, x, .. } if *id == id_a && *x == 3.0).then_some(()));
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
}
