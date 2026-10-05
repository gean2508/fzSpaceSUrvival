//! **Interiores: entrar a un edificio es pasar a un espacio aparte** (pedido del usuario, 4 oct 2026,
//! estilo GTA San Andreas). El interior ya no comparte coordenadas con la calle: vive en su propia
//! **banda de pisos** (`z >= Z0`), una por edificio, así que no puede quedar «por fuera de la casa» y
//! puede medir hasta 1,5 veces la huella exterior.
//!
//! * [`origen`] es el sitio fijo (borde del mundo) donde viven todos los interiores.
//! * [`registrar`] le da a cada edificio (lote) su `slot`; `z_de(slot, piso)` es el nivel de cada planta.
//! * [`plano_de`] genera, de forma pura y determinista, las plantas del edificio: paredes, ventanas,
//!   la puerta del frente, tabiques con sus vanos y el hall con la escalera que se camina.
//! * [`tile`] responde `topdown_tile` para esa banda. Los muebles, las camas y los cofres los pone el
//!   amueblado de siempre (`amueblado`), que ahora sabe de pisos.
//! * [`ocupacion`] decide quién está despierto y qué ventanas están prendidas según la hora: de noche, la
//!   fachada deja ver cuartos ocupados y desocupados.
//!
//! Nada de esto se guarda: sale del lote. Sólo el registro de slots vive en memoria.

use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::{Arc, Mutex, OnceLock};

use crate::celestial_bodies::CelestialBody;
use crate::escaleras::{Tramo, PASOS};
use crate::topdown::Tile;
use crate::worldgen::hash_cell;

/// Primer nivel de la banda de interiores. Muy por encima de cualquier planta real.
pub const Z0: i32 = 20_000;
/// Plantas máximas por edificio (y ancho de la banda de cada uno).
pub const PISOS_MAX: i32 = 8;
/// El interior mide como mucho esto veces la huella exterior (pedido del usuario).
pub const FACTOR_MAX: f32 = 1.5;
/// Lado máximo de un interior, en tiles.
pub const LADO_MAX: i64 = 56;

/// **Dónde viven los interiores en el plano (x, z)**: en la franja de agua del borde del mundo, al este,
/// donde no hay ciudades, ni vecinos, ni fauna, ni autos. Todos comparten ese origen —los separa la banda
/// de pisos—, y lo de afuera que pasa a esas coordenadas queda a decenas de kilómetros de cualquier
/// lote: nada del exterior se cuela por una ventana.
pub fn origen(body: CelestialBody) -> (i64, i64) {
    let r = crate::celestial_bodies::body_walk_radius(body);
    let r = if r > 0.0 { r } else { 30_000.0 };
    ((r * 0.992) as i64, 0)
}

pub fn es_interior(z: i32) -> bool {
    z >= Z0
}

/// Nivel absoluto de la planta `piso` (0 = baja) del edificio `slot`.
pub fn z_de(slot: u32, piso: i32) -> i32 {
    Z0 + slot as i32 * PISOS_MAX + piso
}

/// `(slot, piso)` de un nivel de la banda.
pub fn de_z(z: i32) -> Option<(u32, i32)> {
    if z < Z0 {
        return None;
    }
    let d = z - Z0;
    Some(((d / PISOS_MAX) as u32, d % PISOS_MAX))
}

/// Un edificio del mundo exterior: su rectángulo en tiles y su programa (clave de arte).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Lote {
    pub body: u8,
    pub x: i64,
    pub z: i64,
    pub w: i64,
    pub h: i64,
    pub clave: &'static str,
}

/// Qué clase de interior le toca. Sale del nombre de la clave: cubre TODOS los edificios del pack.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Arq {
    Vivienda,
    Comercio,
    Gasolinera,
    Oficina,
    Hotel,
    Hospital,
    Escuela,
    Comisaria,
    Bomberos,
    Correo,
    Estacion,
    Industrial,
    Granero,
    Bunker,
    Iglesia,
    Restaurante,
    Generico,
}

pub fn arquetipo_de(clave: &str) -> Arq {
    let c = clave.to_ascii_lowercase();
    let tiene = |ws: &[&str]| ws.iter().any(|w| c.contains(w));
    if tiene(&["bunker", "búnker", "militar", "refugio_antiaereo", "base_militar"]) {
        Arq::Bunker
    } else if tiene(&["hospital", "clinica", "clínica", "ambulancia_edificio", "farmacia"]) {
        Arq::Hospital
    } else if tiene(&["universidad", "escuela", "colegio", "biblioteca", "palace"]) {
        Arq::Escuela
    } else if tiene(&["comisaria", "comisaría", "policia", "carcel", "cárcel"]) {
        Arq::Comisaria
    } else if tiene(&["bomberos", "bombero"]) {
        Arq::Bomberos
    } else if tiene(&["correo", "banco", "municipal", "ayuntamiento"]) {
        Arq::Correo
    } else if tiene(&["estacion", "estación", "terminal", "reloj_torre"]) {
        Arq::Estacion
    } else if tiene(&["gasolinera", "estacion_de_servicio", "surtidor"]) {
        Arq::Gasolinera
    } else if tiene(&["hotel", "motel", "hostal", "posada"]) {
        Arq::Hotel
    } else if tiene(&["torre", "oficina", "edificios_condominio", "condominio", "rascacielos"]) {
        Arq::Oficina
    } else if tiene(&["iglesia", "capilla", "templo", "catedral"]) {
        Arq::Iglesia
    } else if tiene(&["restaurante", "bar_", "cafe", "café", "pizzeria", "pizzería", "panaderia", "heladeria", "carniceria"]) {
        Arq::Restaurante
    } else if tiene(&["tienda", "mercado", "shopping", "local", "puesto", "almacen", "kiosco", "vidriera", "discos", "juguetes", "mascotas", "gimnasio", "pesca", "galletas"]) {
        Arq::Comercio
    } else if tiene(&["nave_industrial", "deposito", "depósito", "hangar", "fabrica", "fábrica", "energia", "taller", "galpon", "galpón"]) {
        Arq::Industrial
    } else if tiene(&["granero", "establo", "granja", "silo"]) {
        Arq::Granero
    } else if tiene(&["casa", "villa", "duplex", "dúplex", "hilera", "cabaña", "cabana", "rodante", "chalet", "vivienda", "departamento"]) {
        Arq::Vivienda
    } else {
        Arq::Generico
    }
}

/// Cuántas plantas tiene el edificio por dentro.
pub fn pisos_de(arq: Arq, clave: &str, w: i64, h: i64) -> i32 {
    let grande = w * h >= 400;
    let p = match arq {
        Arq::Vivienda => {
            if clave.contains("chica") || clave.contains("rodante") {
                1
            } else if clave.contains("villa") || clave.contains("grande") || grande {
                3
            } else {
                2
            }
        }
        Arq::Oficina => {
            if clave.contains("torre") {
                5
            } else {
                3
            }
        }
        Arq::Hotel => 4,
        Arq::Hospital => 3,
        Arq::Escuela => {
            if clave.contains("universidad") {
                3
            } else {
                2
            }
        }
        Arq::Comercio => {
            if clave.contains("shopping") {
                2
            } else {
                1
            }
        }
        Arq::Comisaria | Arq::Correo | Arq::Estacion => 2,
        Arq::Restaurante | Arq::Iglesia | Arq::Bomberos => {
            if grande {
                2
            } else {
                1
            }
        }
        _ => 1,
    };
    p.clamp(1, PISOS_MAX)
}

/// Medidas del interior: hasta 1,5× la huella, con un mínimo habitable y un tope de lado.
pub fn medidas(w_ext: i64, h_ext: i64) -> (i64, i64) {
    let f = |v: i64, min: i64| (((v as f32) * FACTOR_MAX).floor() as i64).max(min).min(LADO_MAX);
    (f(w_ext, 12), f(h_ext, 10))
}

// ---------------------------------------------------------------------------
// Registro de slots (global: lo consultan sistemas en distintos hilos)
// ---------------------------------------------------------------------------

#[derive(Default)]
struct Registro {
    lotes: Vec<Lote>,
    por_lote: HashMap<(u8, i64, i64), u32>,
    planos: HashMap<u32, Arc<Plano>>,
}

fn registro() -> &'static Mutex<Registro> {
    static R: OnceLock<Mutex<Registro>> = OnceLock::new();
    R.get_or_init(|| Mutex::new(Registro::default()))
}

/// Le da a un edificio su slot (el mismo siempre, mientras viva el proceso).
pub fn registrar(l: Lote) -> u32 {
    let mut r = registro().lock().expect("registro de interiores");
    if let Some(&s) = r.por_lote.get(&(l.body, l.x, l.z)) {
        return s;
    }
    let s = r.lotes.len() as u32;
    r.lotes.push(l);
    r.por_lote.insert((l.body, l.x, l.z), s);
    s
}

pub fn lote_de_slot(slot: u32) -> Option<Lote> {
    registro().lock().ok()?.lotes.get(slot as usize).copied()
}

/// Interna una clave de arte en un `&'static str` (para edificios que el jugador o los vecinos colocan).
pub fn clave_estatica(s: &str) -> &'static str {
    static I: OnceLock<Mutex<HashSet<&'static str>>> = OnceLock::new();
    let m = I.get_or_init(|| Mutex::new(HashSet::new()));
    let mut g = m.lock().expect("internador");
    if let Some(&k) = g.get(s) {
        return k;
    }
    let k: &'static str = Box::leak(s.to_string().into_boxed_str());
    g.insert(k);
    k
}

/// Vacía el registro (cambio de partida).
pub fn reiniciar() {
    if let Ok(mut r) = registro().lock() {
        *r = Registro::default();
    }
}

// ---------------------------------------------------------------------------
// El plano
// ---------------------------------------------------------------------------

/// Un edificio por dentro. Coordenadas del mundo (la esquina `(x0, z0)` es la del lote).
#[derive(Debug)]
pub struct Plano {
    pub x0: i64,
    pub z0: i64,
    pub w: i64,
    pub h: i64,
    pub pisos: i32,
    pub arq: Arq,
    pub clave: &'static str,
    /// `plantas[piso][lz * w + lx]`.
    pub plantas: Vec<Vec<Tile>>,
    /// La celda de la puerta del frente (planta baja), en mundo.
    pub puerta: (i64, i64),
    /// Dónde aparecés al entrar (centro de la celda de adentro de la puerta).
    pub entrada: (f32, f32),
    /// Los tramos de escalera, con su `z` absoluto de partida ya resuelto por [`tramos_de`].
    pub tramos: Vec<TramoLocal>,
    /// Ventanas de las paredes de afuera: `(piso, x, z)` en mundo. La fachada las enciende de noche.
    pub ventanas: Vec<(i32, i64, i64)>,
    /// Rectángulos de los ambientes (sin paredes), por planta: `(piso, x, z, w, h)` en mundo.
    pub ambientes: Vec<(i32, i64, i64, i64, i64)>,
}

#[derive(Clone, Copy, Debug)]
pub struct TramoLocal {
    /// Planta de abajo que une.
    pub piso: i32,
    pub tx: i64,
    pub tz: i64,
    pub dir: u8,
}

impl Plano {
    pub fn en_rect(&self, tx: i64, tz: i64) -> bool {
        tx >= self.x0 && tz >= self.z0 && tx < self.x0 + self.w && tz < self.z0 + self.h
    }

    pub fn celda(&self, piso: i32, tx: i64, tz: i64) -> Tile {
        if piso < 0 || piso >= self.pisos || !self.en_rect(tx, tz) {
            return Tile::Bedrock;
        }
        self.plantas[piso as usize][((tz - self.z0) * self.w + (tx - self.x0)) as usize]
    }
}

struct Rng(u64);

impl Rng {
    fn new(seed: u32, x: i64, z: i64) -> Self {
        Rng(((hash_cell(seed, x, z) as u64) << 32 | hash_cell(seed ^ 0xA5A5, z, x) as u64) | 1)
    }
    fn next(&mut self) -> u32 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.0 = x;
        (x >> 16) as u32
    }
    fn rango(&mut self, n: i64) -> i64 {
        if n <= 0 {
            0
        } else {
            (self.next() as i64).rem_euclid(n)
        }
    }
}

#[derive(Clone, Copy, Debug)]
struct R {
    x: i64,
    z: i64,
    w: i64,
    h: i64,
}

/// Ancho del hall de la escalera (adentro de sus paredes) y alto mínimo.
const HALL_W: i64 = 9;
const HALL_H: i64 = 9;

fn pared_de(arq: Arq) -> Tile {
    match arq {
        Arq::Bunker | Arq::Industrial | Arq::Comisaria | Arq::Bomberos | Arq::Granero | Arq::Estacion => Tile::StoneBrick,
        _ => Tile::WoodWall,
    }
}

/// Lado mínimo de un ambiente según el tipo de edificio.
fn minimo_de(arq: Arq) -> i64 {
    match arq {
        Arq::Industrial | Arq::Granero | Arq::Bunker | Arq::Iglesia => 9,
        Arq::Comercio | Arq::Gasolinera | Arq::Restaurante | Arq::Estacion => 6,
        Arq::Hospital | Arq::Escuela | Arq::Comisaria | Arq::Bomberos | Arq::Correo | Arq::Oficina | Arq::Hotel => 5,
        _ => 4,
    }
}

/// Cuántos cortes como máximo (profundidad del BSP).
fn profundidad_de(arq: Arq) -> u32 {
    match arq {
        Arq::Industrial | Arq::Granero | Arq::Iglesia => 1,
        Arq::Comercio | Arq::Gasolinera | Arq::Restaurante | Arq::Estacion | Arq::Bunker => 2,
        _ => 4,
    }
}

pub fn generar(l: &Lote) -> Plano {
    let (ox, oz) = origen(CelestialBody::from_u8(l.body).unwrap_or(CelestialBody::Earth));
    let arq = arquetipo_de(l.clave);
    let (w, h) = medidas(l.w, l.h);
    let mut pisos = pisos_de(arq, l.clave, w, h);
    // Sin lugar para el hall de la escalera no hay segunda planta (un piso al que no se llega no existe).
    if pisos > 1 && !(w - 2 >= HALL_W + 2 + minimo_de(arq) && h - 2 >= HALL_H) {
        pisos = 1;
    }
    let mut rng = Rng::new(0x1_47e7, l.x, l.z);
    let pared = pared_de(arq);
    let n = (w * h) as usize;
    let xf = w / 2; // columna de la puerta del frente
    let mut plantas: Vec<Vec<Tile>> = Vec::with_capacity(pisos as usize);

    // Una sola distribución para todas las plantas (misma estructura portante), con la puerta sólo abajo.
    let mut base = vec![Tile::WoodFloor; n];
    let idx = |x: i64, z: i64| (z * w + x) as usize;
    for x in 0..w {
        base[idx(x, 0)] = pared;
        base[idx(x, h - 1)] = pared;
    }
    for z in 0..h {
        base[idx(0, z)] = pared;
        base[idx(w - 1, z)] = pared;
    }

    // Puertas de los tabiques ya puestas (para que un corte nunca las tape).
    let mut puertas: Vec<(i64, i64)> = vec![(xf, h - 1)];
    let mut ambientes: Vec<R> = Vec::new();
    let raiz = R { x: 1, z: 1, w: w - 2, h: h - 2 };
    let multi = pisos > 1;
    let min = minimo_de(arq);

    let mut resto = raiz;
    let mut hall: Option<R> = None;
    if multi && raiz.w >= HALL_W + 2 + min && raiz.h >= HALL_H {
        // El hall de la escalera: una franja a la izquierda, con su tabique y su vano.
        let corte = raiz.x + HALL_W;
        for z in raiz.z..raiz.z + raiz.h {
            base[idx(corte, z)] = pared;
        }
        let vz = raiz.z + raiz.h - 3 - rng.rango((raiz.h - 6).max(1));
        base[idx(corte, vz)] = Tile::Door;
        puertas.push((corte, vz));
        hall = Some(R { x: raiz.x, z: raiz.z, w: HALL_W, h: raiz.h });
        resto = R { x: corte + 1, z: raiz.z, w: raiz.w - HALL_W - 1, h: raiz.h };
    }

    dividir(&mut base, w, resto, min, profundidad_de(arq), 0, pared, &mut rng, &mut puertas, &mut ambientes);
    if let Some(hl) = hall {
        ambientes.push(hl);
    }

    // Ventanas en las paredes de afuera (sólo donde la celda de adentro es suelo).
    let mut ventanas: Vec<(i32, i64, i64)> = Vec::new();
    let ventana_en = |base: &mut Vec<Tile>, x: i64, z: i64, adentro: (i64, i64)| -> bool {
        if base[idx(adentro.0, adentro.1)] == Tile::WoodFloor && base[idx(x, z)] == pared {
            base[idx(x, z)] = Tile::Window;
            true
        } else {
            false
        }
    };
    let paso = if matches!(arq, Arq::Industrial | Arq::Granero | Arq::Bunker) { 7 } else { 4 };
    let mut ventanas_ptos: Vec<(i64, i64, (i64, i64))> = Vec::new();
    let mut x = 3;
    while x < w - 2 {
        ventanas_ptos.push((x, 0, (x, 1)));
        if (x - xf).abs() > 2 {
            ventanas_ptos.push((x, h - 1, (x, h - 2)));
        }
        x += paso;
    }
    let mut z = 3;
    while z < h - 2 {
        ventanas_ptos.push((0, z, (1, z)));
        ventanas_ptos.push((w - 1, z, (w - 2, z)));
        z += paso;
    }
    if arq == Arq::Bunker {
        ventanas_ptos.clear(); // un búnker no tiene ventanas
    }
    let mut ventanas_planta0: Vec<(i64, i64)> = Vec::new();
    for &(vx, vz, adentro) in &ventanas_ptos {
        if ventana_en(&mut base, vx, vz, adentro) {
            ventanas_planta0.push((vx, vz));
        }
    }

    // Plantas: la baja lleva la puerta; las altas, una ventana donde estaba la puerta.
    for p in 0..pisos {
        let mut planta = base.clone();
        planta[idx(xf, h - 1)] = if p == 0 { Tile::Door } else { Tile::Window };
        for &(vx, vz) in &ventanas_planta0 {
            ventanas.push((p, ox + vx, oz + vz));
        }
        plantas.push(planta);
    }
    // El vano de la puerta del frente también deja ver el cuarto desde afuera: ya está.

    // Escaleras: como en cualquier juego visto desde arriba, SUBEN HACIA ARRIBA DE LA PANTALLA (norte): los
    // escalones quedan apilados en una columna, con el pie al sur y el rellano al norte. Uno por cada par de
    // plantas, cada uno en su columna del hall (así el que llega de abajo no tapa al que sigue subiendo).
    let mut tramos: Vec<TramoLocal> = Vec::new();
    if let Some(hl) = hall {
        for p in 0..pisos - 1 {
            // Dos columnas de ancho (un tramo cada una): una escalera de una sola celda no se lee como escalera.
            let col = hl.x + 2 + (p as i64 % 2) * 3;
            let pie_z = hl.z + hl.h - 2;
            for dc in 0..2i64 {
                for i in 0..PASOS as i64 {
                    plantas[p as usize][idx(col + dc, pie_z - i)] = Tile::Step;
                    plantas[p as usize + 1][idx(col + dc, pie_z - i)] = Tile::Stairwell;
                }
                tramos.push(TramoLocal { piso: p, tx: ox + col + dc, tz: oz + pie_z, dir: 0 });
            }
        }
    }

    let puerta = (ox + xf, oz + h - 1);
    let entrada = ((ox + xf) as f32 + 0.5, (oz + h - 2) as f32 + 0.5);
    let mut amb_mundo: Vec<(i32, i64, i64, i64, i64)> = Vec::new();
    for p in 0..pisos {
        for a in &ambientes {
            amb_mundo.push((p, ox + a.x, oz + a.z, a.w, a.h));
        }
    }
    Plano {
        x0: ox,
        z0: oz,
        w,
        h,
        pisos,
        arq,
        clave: l.clave,
        plantas,
        puerta,
        entrada,
        tramos,
        ventanas,
        ambientes: amb_mundo,
    }
}

#[allow(clippy::too_many_arguments)]
fn dividir(
    base: &mut Vec<Tile>,
    w: i64,
    r: R,
    min: i64,
    prof_max: u32,
    prof: u32,
    pared: Tile,
    rng: &mut Rng,
    puertas: &mut Vec<(i64, i64)>,
    salida: &mut Vec<R>,
) {
    let idx = |x: i64, z: i64| (z * w + x) as usize;
    if prof >= prof_max || (r.w < 2 * min + 1 && r.h < 2 * min + 1) {
        salida.push(r);
        return;
    }
    // Se corta el lado más largo (con azar si son parecidos).
    let vertical = if r.w > r.h + 2 {
        true
    } else if r.h > r.w + 2 {
        false
    } else {
        rng.next() & 1 == 0
    };
    if vertical && r.w >= 2 * min + 1 {
        for _ in 0..6 {
            let c = r.x + min + rng.rango(r.w - 2 * min);
            // No tapar una puerta que desemboca en la fila de arriba o de abajo de este rectángulo.
            if puertas.iter().any(|&(dx, dz)| dx == c && (dz == r.z - 1 || dz == r.z + r.h)) {
                continue;
            }
            for z in r.z..r.z + r.h {
                base[idx(c, z)] = pared;
            }
            let vz = r.z + rng.rango(r.h);
            base[idx(c, vz)] = Tile::Door;
            puertas.push((c, vz));
            let a = R { x: r.x, z: r.z, w: c - r.x, h: r.h };
            let b = R { x: c + 1, z: r.z, w: r.x + r.w - c - 1, h: r.h };
            dividir(base, w, a, min, prof_max, prof + 1, pared, rng, puertas, salida);
            dividir(base, w, b, min, prof_max, prof + 1, pared, rng, puertas, salida);
            return;
        }
    } else if !vertical && r.h >= 2 * min + 1 {
        for _ in 0..6 {
            let c = r.z + min + rng.rango(r.h - 2 * min);
            if puertas.iter().any(|&(dx, dz)| dz == c && (dx == r.x - 1 || dx == r.x + r.w)) {
                continue;
            }
            for x in r.x..r.x + r.w {
                base[idx(x, c)] = pared;
            }
            let vx = r.x + rng.rango(r.w);
            base[idx(vx, c)] = Tile::Door;
            puertas.push((vx, c));
            let a = R { x: r.x, z: r.z, w: r.w, h: c - r.z };
            let b = R { x: r.x, z: c + 1, w: r.w, h: r.z + r.h - c - 1 };
            dividir(base, w, a, min, prof_max, prof + 1, pared, rng, puertas, salida);
            dividir(base, w, b, min, prof_max, prof + 1, pared, rng, puertas, salida);
            return;
        }
    }
    salida.push(r);
}

/// El plano de un edificio (memorizado por slot).
pub fn plano_de(slot: u32) -> Option<Arc<Plano>> {
    let mut r = registro().lock().ok()?;
    if let Some(p) = r.planos.get(&slot) {
        return Some(p.clone());
    }
    let l = *r.lotes.get(slot as usize)?;
    let p = Arc::new(generar(&l));
    r.planos.insert(slot, p.clone());
    Some(p)
}

/// `topdown_tile` para la banda de interiores.
pub fn tile(_body: CelestialBody, z: i32, tx: i64, tz: i64) -> Tile {
    let Some((slot, piso)) = de_z(z) else { return Tile::Bedrock };
    match plano_de(slot) {
        Some(p) => p.celda(piso, tx, tz),
        None => Tile::Bedrock,
    }
}

/// El rectángulo del interior que corresponde a un nivel de la banda: `(x, z, w, h, clave)`.
pub fn rect_de_z(z: i32) -> Option<(i64, i64, i64, i64, &'static str)> {
    let (slot, _) = de_z(z)?;
    let p = plano_de(slot)?;
    Some((p.x0, p.z0, p.w, p.h, p.clave))
}

/// El tramo de escalera del interior cuyo pie está en el nivel `z` (la planta de abajo del par).
pub fn tramo_en(body: CelestialBody, z: i32, tx: i64, tz: i64) -> Option<Tramo> {
    let (slot, piso) = de_z(z)?;
    let p = plano_de(slot)?;
    for t in &p.tramos {
        // Igual que los tramos del jugador, se indexa por la planta de ABAJO (donde están los escalones).
        if piso != t.piso {
            continue;
        }
        let tr = Tramo { body: body as u8, z: z_de(slot, t.piso), tx: t.tx, tz: t.tz, dir: t.dir, pasos: PASOS };
        if tr.paso_en(tx, tz).is_some() {
            return Some(tr);
        }
    }
    None
}

// ---------------------------------------------------------------------------
// Quién vive y qué ventanas se prenden
// ---------------------------------------------------------------------------

/// Cuántas personas hay en el edificio (residentes o personal).
pub fn residentes(l: &Lote) -> u32 {
    let arq = arquetipo_de(l.clave);
    let h = hash_cell(0x5e51d, l.x, l.z);
    match arq {
        Arq::Vivienda => 1 + h % 4,
        Arq::Comercio | Arq::Gasolinera | Arq::Restaurante | Arq::Correo => 1 + h % 2,
        Arq::Hospital => 5,
        Arq::Escuela => 5,
        Arq::Comisaria => 4,
        Arq::Bomberos => 3,
        Arq::Oficina | Arq::Hotel => 4,
        Arq::Industrial | Arq::Estacion => 2,
        Arq::Bunker => 2 + h % 2,
        Arq::Iglesia => 1,
        Arq::Granero | Arq::Generico => 1,
    }
}

/// ¿Está despierta la persona `k` del edificio a la hora `hora` (0..24)?
pub fn despierto(arq: Arq, l: &Lote, k: u32, hora: f32) -> bool {
    let h = hash_cell(0xd0d0_0077, l.x + k as i64 * 7, l.z) as f32 / u32::MAX as f32;
    let desfase = (h - 0.5) * 3.0; // ±1,5 h
    let duerme = |ini: f32, fin: f32| {
        let t = (hora - desfase).rem_euclid(24.0);
        t >= ini || t < fin
    };
    match arq {
        // Las casas, los hoteles y los búnkeres duermen de noche.
        Arq::Vivienda | Arq::Hotel | Arq::Bunker | Arq::Granero => !duerme(23.0, 6.5),
        // Servicios de guardia: la mitad vela toda la noche.
        Arq::Hospital | Arq::Comisaria | Arq::Bomberos | Arq::Gasolinera => k % 2 == 0 || !duerme(23.0, 6.5),
        // Comercios, oficinas, escuelas, fábricas: abren de día.
        _ => (7.5..20.5).contains(&hora),
    }
}

/// ¿Está prendida la ventana `i` del edificio a la hora `hora` (0..24)? Los cuartos ocupados prenden;
/// los desocupados quedan a oscuras. Dos edificios nunca prenden igual.
pub fn ventana_prendida(l: &Lote, i: usize, hora: f32) -> bool {
    let arq = arquetipo_de(l.clave);
    let oscuro = !(7.0..18.0).contains(&hora);
    if !oscuro {
        return false;
    }
    let n = residentes(l);
    let h = hash_cell(0x817d, l.x + i as i64, l.z) % 100;
    // Cada ventana pertenece a un habitante; si ese habitante está despierto, hay luz (con algo de azar).
    let dueno = (hash_cell(0x5ec7, l.x, l.z + i as i64) % n.max(1)) as u32;
    let activo = despierto(arq, l, dueno, hora);
    match arq {
        Arq::Vivienda | Arq::Hotel => activo && h < 75,
        Arq::Hospital | Arq::Comisaria | Arq::Bomberos | Arq::Gasolinera => h < 80,
        Arq::Industrial | Arq::Estacion | Arq::Bunker => h < 25,
        _ => (hora < 21.5 || hora >= 18.0) && activo && h < 60,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lote(clave: &'static str, w: i64, h: i64, n: i64) -> Lote {
        Lote { body: CelestialBody::Earth as u8, x: 1000 + n * 100, z: 2000 + n * 77, w, h, clave }
    }

    const CLAVES: &[&str] = &[
        "building/casa_chica",
        "building/casa_mediana",
        "building/casa_grande",
        "building/casa_country",
        "building/villa",
        "building/duplex",
        "building/casa_hilera",
        "edificio/tienda_panaderia",
        "building/tienda",
        "building/mercado_mediano_1",
        "building/gasolinera",
        "building/shopping",
        "building/oficina",
        "building/torre",
        "building/hotel",
        "building/reloj_torre",
        "building/nave_industrial",
        "building/deposito",
        "building/granero",
        "building/hangar",
        "building/energia_casa",
        "building/hospital",
        "building/universidad",
        "building/escuela",
        "building/comisaria",
        "building/bomberos",
        "building/correo_edificio",
        "building/estacion_tren",
        "building/bunker_1",
        "building/iglesia_1",
    ];

    #[test]
    fn la_banda_de_pisos_ida_y_vuelta() {
        for slot in [0u32, 1, 7, 1000] {
            for piso in 0..PISOS_MAX {
                assert_eq!(de_z(z_de(slot, piso)), Some((slot, piso)));
            }
        }
        assert_eq!(de_z(0), None);
        assert!(!es_interior(5) && es_interior(Z0));
    }

    #[test]
    fn todo_edificio_tiene_un_arquetipo_y_el_interior_respeta_el_tope() {
        for (i, c) in CLAVES.iter().enumerate() {
            let l = lote(c, 10 + (i as i64 % 20), 9 + (i as i64 % 15), i as i64);
            let p = generar(&l);
            assert!(p.w as f32 <= (l.w as f32 * FACTOR_MAX).max(12.0) + 0.01, "{c}: ancho {} de {}", p.w, l.w);
            assert!(p.h as f32 <= (l.h as f32 * FACTOR_MAX).max(10.0) + 0.01, "{c}: alto {} de {}", p.h, l.h);
            assert!(p.w <= LADO_MAX && p.h <= LADO_MAX);
            assert!(p.pisos >= 1);
        }
    }

    #[test]
    fn los_arquetipos_cubren_los_nombres_del_pack() {
        assert_eq!(arquetipo_de("building/hospital"), Arq::Hospital);
        assert_eq!(arquetipo_de("building/bomberos"), Arq::Bomberos);
        assert_eq!(arquetipo_de("building/gasolinera"), Arq::Gasolinera);
        assert_eq!(arquetipo_de("building/bunker_2"), Arq::Bunker);
        assert_eq!(arquetipo_de("building/casa_chica"), Arq::Vivienda);
        assert_eq!(arquetipo_de("edificio/tienda_panaderia"), Arq::Restaurante);
        assert_eq!(arquetipo_de("building/universidad"), Arq::Escuela);
    }

    /// Desde la puerta del frente se llega a todas las celdas de piso de la planta baja y a cada escalera.
    #[test]
    fn el_interior_se_recorre_de_punta_a_punta() {
        for (i, c) in CLAVES.iter().enumerate() {
            let l = lote(c, 12 + (i as i64 % 20), 10 + (i as i64 % 15), i as i64);
            let p = generar(&l);
            // Flood fill por piso 0 + escaleras a las demás plantas (las celdas Step/Stairwell son transitables).
            let transitable = |t: Tile| matches!(t, Tile::WoodFloor | Tile::Door | Tile::Step | Tile::Stairwell);
            let mut vistas: HashSet<(i32, i64, i64)> = HashSet::new();
            let mut cola: VecDeque<(i32, i64, i64)> = VecDeque::new();
            let e = (0, p.entrada.0.floor() as i64, p.entrada.1.floor() as i64);
            vistas.insert(e);
            cola.push_back(e);
            while let Some((pz, x, z)) = cola.pop_front() {
                for (dx, dz) in [(1, 0), (-1, 0), (0, 1), (0, -1)] {
                    let n = (pz, x + dx, z + dz);
                    if !vistas.contains(&n) && transitable(p.celda(pz, n.1, n.2)) {
                        vistas.insert(n);
                        cola.push_back(n);
                    }
                }
                // Escalera: desde un escalón de la planta pz se pasa a la misma celda de pz+1.
                if p.celda(pz, x, z) == Tile::Step && !vistas.contains(&(pz + 1, x, z)) {
                    vistas.insert((pz + 1, x, z));
                    cola.push_back((pz + 1, x, z));
                }
                if p.celda(pz, x, z) == Tile::Stairwell && pz > 0 && !vistas.contains(&(pz - 1, x, z)) {
                    vistas.insert((pz - 1, x, z));
                    cola.push_back((pz - 1, x, z));
                }
            }
            for piso in 0..p.pisos {
                for z in 0..p.h {
                    for x in 0..p.w {
                        let t = p.celda(piso, p.x0 + x, p.z0 + z);
                        if transitable(t) {
                            assert!(vistas.contains(&(piso, p.x0 + x, p.z0 + z)), "{c}: celda aislada piso {piso} ({x},{z}) {t:?}");
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn la_puerta_del_frente_existe_y_arriba_es_ventana() {
        let l = lote("building/casa_mediana", 20, 14, 3);
        let p = generar(&l);
        assert_eq!(p.celda(0, p.puerta.0, p.puerta.1), Tile::Door);
        if p.pisos > 1 {
            assert_eq!(p.celda(1, p.puerta.0, p.puerta.1), Tile::Window);
        }
        assert!(!p.ventanas.is_empty(), "una casa lleva ventanas");
    }

    #[test]
    fn el_tramo_une_dos_plantas_y_se_indexa_por_la_de_abajo() {
        let l = lote("building/casa_grande", 24, 16, 5);
        let slot = registrar(l);
        let p = plano_de(slot).expect("plano");
        assert!(p.pisos >= 2 && !p.tramos.is_empty());
        let t = p.tramos[0];
        let abajo = tramo_en(CelestialBody::Earth, z_de(slot, t.piso), t.tx, t.tz - 1).expect("tramo abajo");
        assert_eq!(abajo.z, z_de(slot, t.piso));
        assert!(tramo_en(CelestialBody::Earth, z_de(slot, t.piso + 1), t.tx, t.tz - 1).is_none(), "arriba no se indexa");
        assert_eq!(tile(CelestialBody::Earth, z_de(slot, t.piso), t.tx, t.tz - 1), Tile::Step);
        assert_eq!(tile(CelestialBody::Earth, z_de(slot, t.piso + 1), t.tx, t.tz - 1), Tile::Stairwell);
    }

    #[test]
    fn de_noche_hay_ventanas_prendidas_y_apagadas_y_de_dia_ninguna() {
        let l = lote("building/duplex", 22, 16, 9);
        let n = 40;
        let noche = (0..n).filter(|&i| ventana_prendida(&l, i, 21.0)).count();
        let dia = (0..n).filter(|&i| ventana_prendida(&l, i, 12.0)).count();
        assert_eq!(dia, 0);
        assert!(noche > 0 && noche < n, "prendidas de noche: {noche}/{n}");
    }

    #[test]
    fn las_casas_duermen_de_madrugada_y_el_hospital_vela() {
        let casa = lote("building/casa_chica", 14, 12, 1);
        let hosp = lote("building/hospital", 28, 22, 2);
        let durmiendo = (0..residentes(&casa)).filter(|&k| !despierto(Arq::Vivienda, &casa, k, 3.0)).count();
        assert_eq!(durmiendo as u32, residentes(&casa));
        assert!((0..residentes(&hosp)).any(|k| despierto(Arq::Hospital, &hosp, k, 3.0)));
    }
}
