//! **FERROCARRIL** (5 oct 2026): cada ciudad tiene su estación y su línea de tren, con DOS vías.
//!
//! Sólo la geometría y las reglas: el cliente (`ha-game::trenes`) dibuja las vías y mueve los trenes.
//!
//! ## Qué es una línea
//! Un tramo recto **este-oeste** que pasa junto a una ciudad (`City`), por fuera de su circunvalación, con TRES paradas:
//! el apeadero del oeste, la **estación** (frente al centro de la ciudad) y el apeadero del este. Las dos puntas se
//! pasan [`EXTENSION`] tiles del casco, así que el tren hace de 3 a 8 km de recorrido y se puede viajar de verdad.
//!
//! ## Por qué recta y este-oeste
//! El tren del pack (`vehicle/tren_*`) es una vista lateral de 10 m por coche: no existe un dibujo de tren yendo hacia
//! el norte, y el riel modular no tiene curvas que cierren con un tren de 29 m. Y **no hay línea entre ciudades**: se
//! midió (8 ciudades en ±78 km, a 8-14 km unas de otras y casi nunca a la misma latitud) y una recta entre dos
//! ciudades queda a kilómetros de ambas. Las ciudades se conectan por carretera (`geography::link_clearance`).
//!
//! ## Cómo se elige la fila
//! Al sur del casco (y si no sirve, al norte) se prueban filas cada [`PASO_FILA`] tiles hasta que el corredor
//! ([`ancho_total`] filas) cumple, en TODAS sus celdas:
//!
//! * suelo firme (`Tile::buildable_ground`, o calzada: un paso a nivel): **ni agua, ni acantilado, ni rampa** — una vía
//!   sobre un desnivel sale rota, y el pedido fue explícito;
//! * ningún casco ajeno (`settlement_covering`): el tren no atraviesa una aldea.
//!
//! Si ninguna fila sirve, esa ciudad NO tiene tren (y no hay vía a medias).
//!
//! ## Las dos vías
//! Vía 0 (`ida`, hacia el este) en la fila `z`, vía 1 (`vuelta`, hacia el oeste) en `z + SEPARACION_VIAS`. Entre las dos
//! queda el andén. Cada tren va y vuelve por SU vía; el cambio de una a otra se hace en el apeadero de la punta.

use crate::celestial_bodies::CelestialBody;
use crate::geography::{settlement_covering, settlement_in, Settlement, SettlementTier, SITE_PERIOD};
use crate::topdown::{topdown_tile, Tile};

/// Cuánto más allá del casco (tiles) queda la fila de las vías: afuera de la circunvalación y de sus veredas.
pub const HOLGURA_ESTACION: i64 = 70;
/// Cuánto se pasa la línea del casco hacia cada punta (tiles): ahí están los apeaderos.
pub const EXTENSION: i64 = 500;
/// Separación entre la fila de la vía de ida y la de vuelta. Un coche del pack mide 4 tiles de alto: más junto, el de
/// una vía taparía al de la otra.
pub const SEPARACION_VIAS: i64 = 5;
/// De a cuántas filas se corre la línea al buscar un corredor llano.
pub const PASO_FILA: i64 = 6;
/// Cuántos corrimientos (a cada lado) se prueban.
pub const CORRIMIENTOS: i64 = 30;
/// Mitad del largo del andén (tiles) a cada lado del centro de la estación.
pub const MEDIO_ANDEN: i64 = 24;
/// Franja de seguridad (tiles) a cada lado de las vías que tiene que ser llana: el andén y los coches (4 de alto).
pub const MARGEN_NORTE: i64 = 4;
pub const MARGEN_SUR: i64 = 1;

/// Una línea de tren: tres paradas (oeste, estación, este) sobre un tramo recto junto a una ciudad.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Linea {
    /// Identificador estable (sale del centro de la ciudad).
    pub id: u32,
    /// Centro y radio de la ciudad a la que sirve.
    pub ciudad: (i64, i64),
    pub radio: i64,
    /// Fila de la vía de ida.
    pub z: i64,
    /// Columna de cada parada: oeste, estación (frente al centro de la ciudad), este.
    pub paradas: [i64; 3],
}

impl Linea {
    /// Columna de la punta oeste y de la punta este.
    pub fn x_oeste(&self) -> i64 {
        self.paradas[0]
    }
    pub fn x_este(&self) -> i64 {
        self.paradas[2]
    }

    /// Largo entre las dos puntas (m).
    pub fn largo(&self) -> f32 {
        (self.x_este() - self.x_oeste()) as f32
    }

    /// Fila de la vía `via` (0 = ida/este, 1 = vuelta/oeste).
    pub fn fila_via(&self, via: u8) -> i64 {
        self.z + if via == 0 { 0 } else { SEPARACION_VIAS }
    }

    /// ¿La celda cae sobre alguno de los andenes? Devuelve el índice de parada (0 = oeste, 1 = estación, 2 = este).
    pub fn anden_en(&self, tx: i64, tz: i64) -> Option<u8> {
        if tz <= self.z || tz >= self.z + SEPARACION_VIAS {
            return None;
        }
        self.paradas.iter().position(|&x| (tx - x).abs() <= MEDIO_ANDEN).map(|i| i as u8)
    }

    /// Caja de la línea (con margen) para descartar rápido.
    pub fn caja(&self) -> [i64; 4] {
        [self.x_oeste() - MEDIO_ANDEN, self.z - MARGEN_NORTE, self.x_este() + MEDIO_ANDEN, self.z + SEPARACION_VIAS + MARGEN_SUR]
    }

    pub fn toca(&self, tx: i64, tz: i64) -> bool {
        let [x0, z0, x1, z1] = self.caja();
        tx >= x0 && tx <= x1 && tz >= z0 && tz <= z1
    }
}

/// Qué hay en una celda de vía.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Riel {
    /// 0 = ida (este), 1 = vuelta (oeste).
    pub via: u8,
    /// La vía cruza una calzada: paso a nivel.
    pub paso_a_nivel: bool,
}

/// Ancho total (filas) del corredor que tiene que ser llano.
pub fn ancho_total() -> i64 {
    MARGEN_NORTE + SEPARACION_VIAS + 1 + MARGEN_SUR
}

/// ¿Esta celda aguanta una vía? Suelo firme ni agua, ni acantilado, ni rampa. La calzada cuenta (paso a nivel).
pub fn celda_firme(body: CelestialBody, tx: i64, tz: i64) -> bool {
    let t = topdown_tile(body, 0, tx, tz);
    t.buildable_ground() && !matches!(t, Tile::Cliff | Tile::Slope | Tile::Fence | Tile::Air)
}

/// **Qué columnas de la fila `z` (entre `x_lo` y `x_hi`) tienen un corredor llano**: para cada columna, las dos filas de
/// vía y el andén con su margen sobre suelo firme (ni agua, ni acantilado, ni rampa) y fuera de todo casco.
///
/// Un solo desnivel en medio de la línea la dejaba entera sin tren (medido: casi todas las filas de la Tierra cruzan
/// alguna terraza), así que en vez de exigir el corredor completo se calcula DÓNDE es llano y [`mejor_tramo`] elige el
/// tramo más largo.
pub fn columnas_llanas(body: CelestialBody, z: i64, x_lo: i64, x_hi: i64) -> Vec<bool> {
    let n = (x_hi - x_lo + 1).max(0) as usize;
    let mut ok = vec![true; n];
    let filas_via = [z, z + SEPARACION_VIAS];
    for i in 0..n {
        let x = x_lo + i as i64;
        // Las filas de las vías, en cada columna.
        if !filas_via.iter().all(|&f| celda_firme(body, x, f)) {
            ok[i] = false;
        }
    }
    // El andén y el margen: en columnas pares (las terrazas son bandas, no puntos), y se propaga a la vecina.
    for f in (z - MARGEN_NORTE)..=(z + SEPARACION_VIAS + MARGEN_SUR) {
        if filas_via.contains(&f) {
            continue;
        }
        for i in (0..n).step_by(2) {
            if ok[i] && !celda_firme(body, x_lo + i as i64, f) {
                ok[i] = false;
                if i + 1 < n {
                    ok[i + 1] = false;
                }
            }
        }
    }
    // Cascos ajenos: cada 8 columnas (un casco mide cientos).
    for i in (0..n).step_by(8) {
        let x = x_lo + i as i64;
        if settlement_covering(body, x, z).is_some() || settlement_covering(body, x, z + SEPARACION_VIAS).is_some() {
            for j in i..(i + 8).min(n) {
                ok[j] = false;
            }
        }
    }
    ok
}

/// El tramo llano contiguo más largo que contiene (o roza) `x_ref`: `(desde, hasta)` en columnas absolutas.
pub fn mejor_tramo(ok: &[bool], x_lo: i64, x_ref: i64) -> Option<(i64, i64)> {
    let mut mejor: Option<(i64, i64)> = None;
    let mut i = 0;
    while i < ok.len() {
        if !ok[i] {
            i += 1;
            continue;
        }
        let ini = i;
        while i < ok.len() && ok[i] {
            i += 1;
        }
        let (d, h) = (x_lo + ini as i64, x_lo + i as i64 - 1);
        // Preferencia: el que contiene a x_ref; entre los demás, el más largo.
        let pt = |(a, b): (i64, i64)| ((a <= x_ref && x_ref <= b) as i64, b - a);
        if mejor.map_or(true, |m| pt((d, h)) > pt(m)) {
            mejor = Some((d, h));
        }
    }
    mejor
}

fn es_ciudad(s: &Settlement) -> bool {
    s.tier == SettlementTier::City
}

/// Identificador estable de una línea.
fn id_de(a: (i64, i64), b: (i64, i64)) -> u32 {
    let h = (a.0.wrapping_mul(73_856_093) ^ a.1.wrapping_mul(19_349_663) ^ b.0.wrapping_mul(83_492_791) ^ b.1.wrapping_mul(2_654_435_761)) as u64;
    (h ^ (h >> 32)) as u32
}

/// Largo mínimo de una línea (tiles entre las dos puntas): menos que esto no vale un tren.
pub const LARGO_MIN: i64 = 260;
/// Cada cuántas columnas se sondea una fila candidata.
const SONDEO: i64 = 6;
/// Cuántas filas candidatas se miden exactas.
const FINALISTAS: usize = 5;

/// La línea de la ciudad `a`: la fila (al sur o al norte del casco) con el tramo llano más largo cerca de la ciudad,
/// o `None` si ninguna da [`LARGO_MIN`].
pub fn linea_desde(body: CelestialBody, a: &Settlement) -> Option<Linea> {
    if !es_ciudad(a) {
        return None;
    }
    let (cx, cz) = a.center;
    let (x_lo, x_hi) = (cx - a.radius - EXTENSION - MEDIO_ANDEN, cx + a.radius + EXTENSION + MEDIO_ANDEN);
    let completo = x_hi - x_lo + 1;
    // **Dos pasadas** (una fila exacta cuesta ~15 mil consultas de terreno y hay 62 candidatas por ciudad): primero un
    // sondeo barato —las dos filas de vía cada [`SONDEO`] columnas— que ordena las candidatas por su tramo llano más
    // largo, y recién después la medición exacta de las [`FINALISTAS`] mejores.
    let filas: Vec<i64> = [1i64, -1]
        .into_iter()
        .flat_map(|lado| {
            (0..=CORRIMIENTOS).map(move |k| {
                if lado > 0 {
                    cz + a.radius + HOLGURA_ESTACION + k * PASO_FILA
                } else {
                    // al norte la vía de ida queda arriba y el andén (y la vuelta) entre ella y el casco
                    cz - a.radius - HOLGURA_ESTACION - k * PASO_FILA - SEPARACION_VIAS - MARGEN_SUR
                }
            })
        })
        .collect();
    let mut sondeadas: Vec<(i64, usize, i64)> = filas
        .iter()
        .enumerate()
        .map(|(orden, &z)| {
            let (mut run, mut mejor_run) = (0i64, 0i64);
            let mut x = x_lo;
            while x <= x_hi {
                if celda_firme(body, x, z) && celda_firme(body, x, z + SEPARACION_VIAS) {
                    run += SONDEO;
                    mejor_run = mejor_run.max(run);
                } else {
                    run = 0;
                }
                x += SONDEO;
            }
            (-mejor_run, orden, z)
        })
        .collect();
    sondeadas.sort_unstable();
    let mut mejor: Option<(i64, usize)> = None;
    let mut fila_mejor = None;
    for &(_, orden, z) in sondeadas.iter().take(FINALISTAS) {
        let ok = columnas_llanas(body, z, x_lo, x_hi);
        let Some((d, h)) = mejor_tramo(&ok, x_lo, cx) else { continue };
        // Las puntas son apeaderos: se retiran un andén del borde llano.
        let (d, h) = (d + MEDIO_ANDEN, h - MEDIO_ANDEN);
        let largo = h - d;
        if largo < LARGO_MIN {
            continue;
        }
        // Gana el más largo; a igual largo, el más cercano al casco (menor `orden`).
        if mejor.map_or(true, |m| (largo, std::cmp::Reverse(orden)) > (m.0, std::cmp::Reverse(m.1))) {
            mejor = Some((largo, orden));
            fila_mejor = Some((z, d, h));
        }
        if largo >= completo - 2 * MEDIO_ANDEN - 2 {
            break; // tramo completo: no se mejora
        }
    }
    let (z, d, h) = fila_mejor?;
    // La estación, frente al centro de la ciudad si el tramo llega; si no, en el punto del tramo más cercano a él.
    let est = cx.clamp(d + 80, h - 80);
    if (est - cx).abs() > a.radius + EXTENSION || h - d < 2 * 80 + 40 {
        return None;
    }
    Some(Linea { id: id_de((cx, cz), (0, 0)), ciudad: (cx, cz), radio: a.radius, z, paradas: [d, est, h] })
}

/// Cajas (con margen) de las líneas ya calculadas, por `(época, cuerpo)`: lo que consulta el sembrado de decoración para no
/// plantar árboles ni piedras sobre las vías. **Nunca calcula** (cuesta segundos): sólo mira lo que el hilo de fondo ya midió.
fn cajas() -> &'static std::sync::RwLock<Vec<(u32, u8, [i64; 4])>> {
    static C: std::sync::OnceLock<std::sync::RwLock<Vec<(u32, u8, [i64; 4])>>> = std::sync::OnceLock::new();
    C.get_or_init(|| std::sync::RwLock::new(Vec::new()))
}

static GENERACION: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);

/// Sube cada vez que se mide una línea nueva: entra en la clave de la memoria de decoración (`worldgen`).
pub fn generacion() -> u32 {
    GENERACION.load(std::sync::atomic::Ordering::Relaxed)
}

/// ¿La celda cae dentro del corredor de una línea YA calculada (vías, andenes y su margen, +1 tile)? Es lo que impide
/// sembrar sobre las vías. No calcula nada: una línea todavía no medida no excluye nada (y `ha-game` limpia la
/// decoración ya plantada cuando la línea llega).
pub fn corredor_conocido(body: CelestialBody, tx: i64, tz: i64) -> bool {
    // Camino rápido: sin ninguna línea medida (lo normal en el campo y en las pruebas) no se toma el candado.
    if GENERACION.load(std::sync::atomic::Ordering::Relaxed) == 0 {
        return false;
    }
    let epoca = crate::partida::epoca();
    cajas().read().unwrap().iter().any(|&(e, b, [x0, z0, x1, z1])| e == epoca && b == body as u8 && tx >= x0 - 1 && tx <= x1 + 1 && tz >= z0 - 1 && tz <= z1 + 1)
}

/// Memo global de la línea de cada ciudad (calcularla cuesta segundos): `(época, cuerpo, centro) → línea`.
fn memo() -> &'static std::sync::Mutex<std::collections::HashMap<(u32, u8, i64, i64), Option<Linea>>> {
    static M: std::sync::OnceLock<std::sync::Mutex<std::collections::HashMap<(u32, u8, i64, i64), Option<Linea>>>> = std::sync::OnceLock::new();
    M.get_or_init(|| std::sync::Mutex::new(std::collections::HashMap::new()))
}

/// La línea de una ciudad, con memo (la época de la partida entra en la clave: otra semilla, otras ciudades).
pub fn linea_de_ciudad(body: CelestialBody, a: &Settlement) -> Option<Linea> {
    let clave = (crate::partida::epoca(), body as u8, a.center.0, a.center.1);
    if let Some(v) = memo().lock().unwrap().get(&clave) {
        return *v;
    }
    let v = linea_desde(body, a);
    if let Some(l) = &v {
        cajas().write().unwrap().push((clave.0, clave.1, l.caja()));
        GENERACION.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    }
    memo().lock().unwrap().insert(clave, v);
    v
}

/// Las líneas de las ciudades a ≤ `radio_celdas` celdas de asentamiento de `(tx, tz)`. La primera vez cuesta segundos
/// (corre en un hilo de fondo, ver `ha-game::trenes`); después sale del memo.
pub fn lineas_cerca(body: CelestialBody, tx: i64, tz: i64, radio_celdas: i64) -> Vec<Linea> {
    let (cx, cz) = (tx.div_euclid(SITE_PERIOD), tz.div_euclid(SITE_PERIOD));
    let mut v: Vec<Linea> = Vec::new();
    for dx in -radio_celdas..=radio_celdas {
        for dz in -radio_celdas..=radio_celdas {
            let Some(a) = settlement_in(body, cx + dx, cz + dz) else { continue };
            if let Some(l) = linea_de_ciudad(body, &a) {
                if !v.iter().any(|o| o.id == l.id) {
                    v.push(l);
                }
            }
        }
    }
    v
}

/// ¿Hay vía en esta celda? Puro sobre las líneas ya calculadas.
pub fn riel_en(body: CelestialBody, lineas: &[Linea], tx: i64, tz: i64) -> Option<Riel> {
    for l in lineas {
        if tx < l.x_oeste() - MEDIO_ANDEN || tx > l.x_este() + MEDIO_ANDEN {
            continue;
        }
        let via = if tz == l.z {
            0
        } else if tz == l.z + SEPARACION_VIAS {
            1
        } else {
            continue;
        };
        let paso_a_nivel = matches!(crate::geography::civil_kind_at(body, tx, tz), Some(crate::geography::Civil::Road));
        return Some(Riel { via, paso_a_nivel });
    }
    None
}

// ── Horario (determinista, sin estado) ───────────────────────────────────────────────────────────────────────────

/// Velocidad de crucero de los trenes (m/s): ~100 km/h.
pub const V_TREN: f32 = 28.0;
/// Aceleración y frenado (m/s²).
pub const A_TREN: f32 = 1.6;
/// Segundos que el tren espera en cada estación con las puertas abiertas.
pub const ESPERA_S: f32 = 20.0;
/// Segundos de «cambio de vía» al final de cada recorrido (el tren se desvanece en un andén y sale por el otro).
pub const CAMBIO_S: f32 = 1.2;

/// Estado de un tren en un instante.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct EstadoTren {
    /// Vía por la que va (0 = ida, hacia el este; 1 = vuelta, hacia el oeste).
    pub via: u8,
    /// Posición del centro del tren (tiles, x absoluta).
    pub x: f32,
    /// Velocidad con signo (m/s; positiva = este).
    pub vel: f32,
    /// Parado en una parada con las puertas abiertas (índice 0 = oeste, 1 = estación, 2 = este).
    pub en_estacion: Option<u8>,
    /// 0..1: opacidad (se desvanece en el cambio de vía de las puntas).
    pub opacidad: f32,
}

/// Tiempo (s) que tarda un recorrido entero: acelerar, crucero, frenar. `largo` en m.
pub fn tiempo_de_recorrido(largo: f32) -> f32 {
    let d_acel = V_TREN * V_TREN / (2.0 * A_TREN);
    if largo >= 2.0 * d_acel {
        2.0 * V_TREN / A_TREN + (largo - 2.0 * d_acel) / V_TREN
    } else {
        2.0 * (largo / A_TREN).sqrt()
    }
}

/// Avance (m desde la salida) tras `t` s de un recorrido de `largo` m, y la velocidad.
fn avance(largo: f32, t: f32) -> (f32, f32) {
    let d_acel = V_TREN * V_TREN / (2.0 * A_TREN);
    let total = tiempo_de_recorrido(largo);
    let t = t.clamp(0.0, total);
    let (vmax, t_acel) = if largo >= 2.0 * d_acel { (V_TREN, V_TREN / A_TREN) } else { ((largo * A_TREN).sqrt(), (largo / A_TREN).sqrt()) };
    if t < t_acel {
        (0.5 * A_TREN * t * t, A_TREN * t)
    } else if t > total - t_acel {
        let r = total - t;
        (largo - 0.5 * A_TREN * r * r, A_TREN * r)
    } else {
        (0.5 * A_TREN * t_acel * t_acel + vmax * (t - t_acel), vmax)
    }
}

/// Los cuatro tramos del ciclo: `(vía, parada de salida, parada de llegada)`.
const TRAMOS: [(u8, usize, usize); 4] = [(0, 0, 1), (0, 1, 2), (1, 2, 1), (1, 1, 0)];

/// Duración (s) de un ciclo completo (ida y vuelta, con todas las esperas).
pub fn ciclo(l: &Linea) -> f32 {
    TRAMOS
        .iter()
        .map(|&(_, a, b)| tiempo_de_recorrido((l.paradas[b] - l.paradas[a]).abs() as f32) + ESPERA_S)
        .sum()
}

/// **El horario**: un tren por línea que va y vuelve, cada uno por su vía. Oeste → estación → este por la vía 0 (con
/// espera en cada parada), cambio de vía en la punta, este → estación → oeste por la vía 1, cambio de vía. Función pura
/// del reloj `t_s`: el mundo no guarda nada del tren, reaparece donde el horario dice que está.
pub fn estado_del_tren(l: &Linea, t_s: f32) -> EstadoTren {
    let total = ciclo(l);
    let desfase = (l.id % 97) as f32 * 3.1;
    let mut t = (t_s + desfase).rem_euclid(total);
    for (i, &(via, a, b)) in TRAMOS.iter().enumerate() {
        let (xa, xb) = (l.paradas[a] as f32, l.paradas[b] as f32);
        let largo = (xb - xa).abs();
        let rec = tiempo_de_recorrido(largo);
        let dur = rec + ESPERA_S;
        if t < dur || i == TRAMOS.len() - 1 {
            let signo = if xb >= xa { 1.0 } else { -1.0 };
            if t >= rec {
                // Parado en la parada de llegada; en las puntas, al final de la espera se desvanece (cambio de vía).
                let punta = b == 0 || b == 2;
                let opacidad = if punta { ((dur - t) / CAMBIO_S).clamp(0.0, 1.0) } else { 1.0 };
                return EstadoTren { via, x: xb, vel: 0.0, en_estacion: Some(b as u8), opacidad };
            }
            let (d, v) = avance(largo, t);
            // Saliendo de una punta se «materializa» (viene del cambio de vía).
            let opacidad = if a == 0 || a == 2 { (t / CAMBIO_S).clamp(0.0, 1.0) } else { 1.0 };
            return EstadoTren { via, x: xa + signo * d, vel: signo * v, en_estacion: None, opacidad };
        }
        t -= dur;
    }
    unreachable!()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn l() -> Linea {
        Linea { id: 7, ciudad: (1500, 0), radio: 620, z: 800, paradas: [380, 1500, 2620] }
    }

    #[test]
    fn el_tren_recorre_las_tres_paradas_por_su_via_y_vuelve_por_la_otra() {
        let l = l();
        let desfase = (l.id % 97) as f32 * 3.1;
        let en = |t: f32| estado_del_tren(&l, t - desfase);
        let rec = tiempo_de_recorrido(1120.0);
        // Primer tramo: oeste → estación por la vía 0, hacia el este.
        let e = en(rec * 0.5);
        assert_eq!(e.via, 0);
        assert!(e.vel > 20.0 && e.x > 380.0 && e.x < 1500.0, "{e:?}");
        // Parado en la estación del medio.
        let e = en(rec + 5.0);
        assert_eq!((e.via, e.en_estacion, e.vel), (0, Some(1), 0.0));
        assert!((e.x - 1500.0).abs() < 0.5);
        // Segundo tramo hasta la punta este.
        let e = en(rec + ESPERA_S + rec * 0.5);
        assert_eq!(e.via, 0);
        assert!(e.vel > 20.0 && e.x > 1500.0 && e.x < 2620.0);
        // Esperando en la punta este.
        let e = en(2.0 * rec + ESPERA_S + 5.0);
        assert_eq!((e.via, e.en_estacion), (0, Some(2)));
        // Vuelta por la vía 1, hacia el oeste.
        let e = en(2.0 * (rec + ESPERA_S) + rec * 0.5);
        assert_eq!(e.via, 1);
        assert!(e.vel < -20.0 && e.x < 2620.0 && e.x > 1500.0);
        // Esperando en la estación de vuelta y, al final, en la punta oeste.
        let e = en(3.0 * rec + 2.0 * ESPERA_S + 5.0);
        assert_eq!((e.via, e.en_estacion), (1, Some(1)));
        let e = en(4.0 * rec + 3.0 * ESPERA_S + 5.0);
        assert_eq!((e.via, e.en_estacion), (1, Some(0)));
        assert!((e.x - 380.0).abs() < 0.5);
    }

    #[test]
    fn el_recorrido_es_continuo_acelera_y_frena_sin_saltos() {
        let l = l();
        let mut previo: Option<(f32, f32, u8)> = None;
        let mut t = 0.0;
        let mut max_salto = 0.0f32;
        while t < 2.0 * ciclo(&l) {
            let e = estado_del_tren(&l, t);
            if let Some((xp, vp, via_p)) = previo {
                // En el cambio de vía de las puntas el tren «salta» a la otra punta: sólo se miden los continuos.
                if via_p == e.via && (e.x - xp).abs() < 100.0 {
                    max_salto = max_salto.max((e.x - xp).abs());
                    assert!((e.vel - vp).abs() < 0.5, "la velocidad salta en t={t}: {vp} → {}", e.vel);
                }
            }
            previo = Some((e.x, e.vel, e.via));
            t += 0.1;
        }
        assert!(max_salto <= V_TREN * 0.1 + 0.01, "salto de {max_salto} m en 0,1 s");
    }

    #[test]
    fn el_tren_nunca_se_pasa_de_las_puntas_ni_cambia_de_via_andando() {
        let l = l();
        let mut t = 0.0;
        let mut via_ant: Option<(u8, bool)> = None;
        while t < 2.0 * ciclo(&l) {
            let e = estado_del_tren(&l, t);
            assert!(e.x >= 379.9 && e.x <= 2620.1, "se pasó de la punta: {e:?}");
            if let Some((v, parado)) = via_ant {
                if v != e.via {
                    assert!(parado, "cambió de vía en marcha en t={t}");
                }
            }
            via_ant = Some((e.via, e.en_estacion.is_some()));
            t += 0.1;
        }
    }

    #[test]
    fn las_vias_estan_separadas_y_el_anden_queda_entre_ellas() {
        let l = l();
        assert_eq!(l.fila_via(1) - l.fila_via(0), SEPARACION_VIAS);
        assert_eq!(l.anden_en(1500, l.z + 2), Some(1));
        assert_eq!(l.anden_en(380 - 10, l.z + 1), Some(0));
        assert_eq!(l.anden_en(2620 + 10, l.z + 1), Some(2));
        assert_eq!(l.anden_en(1500, l.z), None, "la vía no es andén");
        assert_eq!(l.anden_en(1000, l.z + 2), None, "entre paradas no hay andén");
    }
}

#[cfg(test)]
mod con_mapa {
    use super::*;

    /// **Sobre el mapa real**: la ciudad más cercana al origen tiene línea, las celdas de las dos vías y del andén son
    /// suelo firme (nada de agua, acantilado ni rampa) y la estación queda junto a la ciudad.
    #[test]
    fn la_linea_de_una_ciudad_real_es_llana_y_queda_junto_a_ella() {
        let body = CelestialBody::Earth;
        let mut ciudad: Option<Settlement> = None;
        for sx in -3..=3 {
            for sz in -3..=3 {
                if let Some(s) = settlement_in(body, sx, sz) {
                    if es_ciudad(&s) && ciudad.as_ref().map_or(true, |c| s.center.0.abs() + s.center.1.abs() < c.center.0.abs() + c.center.1.abs()) {
                        ciudad = Some(s);
                    }
                }
            }
        }
        let c = ciudad.expect("hay una ciudad cerca del origen");
        let l = linea_de_ciudad(body, &c).expect("la ciudad tiene línea");
        assert!(l.largo() >= LARGO_MIN as f32);
        assert!((l.paradas[1] - c.center.0).abs() <= c.radius + EXTENSION, "la estación queda frente a la ciudad");
        assert!(l.paradas[0] < l.paradas[1] && l.paradas[1] < l.paradas[2]);
        let lineas = [l];
        for x in (l.x_oeste() - MEDIO_ANDEN)..=(l.x_este() + MEDIO_ANDEN) {
            for via in 0..2u8 {
                let z = l.fila_via(via);
                assert!(riel_en(body, &lineas, x, z).is_some(), "falta riel en ({x},{z})");
                assert!(celda_firme(body, x, z), "la vía {via} pisa un desnivel/agua en ({x},{z})");
            }
        }
        for &est in &l.paradas {
            for x in (est - MEDIO_ANDEN)..=(est + MEDIO_ANDEN) {
                for z in (l.z + 1)..(l.z + SEPARACION_VIAS) {
                    assert!(celda_firme(body, x, z), "el andén pisa un desnivel/agua en ({x},{z})");
                }
            }
        }
    }
}
