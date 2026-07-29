//! GEOGRAFÍA DEL MUNDO — biomas, ríos, carreteras y asentamientos.
//!
//! Con 60 km de lado (`cosmic_scale`), un mundo de puro ruido de período 22 es
//! papel pintado: todo se ve igual en todas partes y volar diez minutos no
//! significa nada. Este módulo le da al planeta la estructura que hace que
//! recorrerlo valga la pena, al estilo de un mapa de mundo abierto grande:
//!
//! - **Relieve en cuatro escalas** — continentes (9 km), cordilleras (1.4 km),
//!   colinas (110 m) y textura (22 m). Se cruzan mares, se rodean sierras.
//! - **Agua que se comporta** — océano bajo el nivel del mar, playas en la
//!   orilla, **ríos** que bajan de las cumbres al mar y **pantanos** donde el
//!   terreno es bajo y húmedo a la vez.
//! - **Civilización** — asentamientos por rangos (aldea, pueblo, ciudad) y una
//!   **red de carreteras** que los une, con **puentes** donde cruza agua. Es lo
//!   que convierte un paisaje en un país.
//! - **Arquetipos por cuerpo** — la Tierra es el mundo templado donde se vive;
//!   los demás son variaciones honestas (oceánico, desértico, helado, volcánico,
//!   yermo) con su propio nivel del mar y su propia paleta, no la Tierra pintada
//!   de otro color.
//!
//! Todo es **determinista y O(1) por celda**: no hay mapa en memoria, hay una
//! función. Por eso el mismo mundo lo pueden evaluar el streaming de tiles, el
//! mapa satelital de `universe.rs` y la textura del disco del planeta sin que
//! ninguno tenga que ponerse de acuerdo con los otros.
//!
//! `worldgen.rs` (decoración) y `topdown.rs` (tiles, colisión, capas) consumen
//! esto; no dupliques ninguna de estas reglas allí.

use crate::celestial_bodies::{body_terrain_seed, body_walk_radius, CelestialBody};
use crate::topdown::{Tile, TILE_M};

// ---------------------------------------------------------------------------
// Ruido base
// ---------------------------------------------------------------------------

fn hash_u32(mut x: u32) -> u32 {
    x ^= x >> 16;
    x = x.wrapping_mul(0x7feb_352d);
    x ^= x >> 15;
    x = x.wrapping_mul(0x846c_a68b);
    x ^= x >> 16;
    x
}

fn hash2(seed: u32, a: i64, b: i64) -> u32 {
    let a = (a as i32 as u32).wrapping_mul(0x9e37_79b9);
    let b = (b as i32 as u32).wrapping_mul(0x85eb_ca6b);
    hash_u32(seed ^ a ^ b.rotate_left(13))
}

fn unit(seed: u32, a: i64, b: i64) -> f32 {
    (hash2(seed, a, b) & 0x00ff_ffff) as f32 / 16_777_215.0
}

/// Ruido de valor bilineal suavizado; `period` en tiles.
fn value_noise(seed: u32, tx: i64, tz: i64, period: i64) -> f32 {
    let p = period.max(1);
    let (cx, cz) = (tx.div_euclid(p), tz.div_euclid(p));
    let fx = tx.rem_euclid(p) as f32 / p as f32;
    let fz = tz.rem_euclid(p) as f32 / p as f32;
    let sx = fx * fx * (3.0 - 2.0 * fx);
    let sz = fz * fz * (3.0 - 2.0 * fz);
    let n00 = unit(seed, cx, cz);
    let n10 = unit(seed, cx + 1, cz);
    let n01 = unit(seed, cx, cz + 1);
    let n11 = unit(seed, cx + 1, cz + 1);
    let a = n00 + (n10 - n00) * sx;
    let b = n01 + (n11 - n01) * sx;
    a + (b - a) * sz
}

fn fbm(seed: u32, tx: i64, tz: i64, period: i64) -> f32 {
    let a = value_noise(seed, tx, tz, period);
    let b = value_noise(seed ^ 0x51ed_2b37, tx, tz, (period / 3).max(2));
    (a * 0.68 + b * 0.32).clamp(0.0, 1.0)
}

// ---------------------------------------------------------------------------
// Arquetipos: qué CLASE de mundo es cada cuerpo
// ---------------------------------------------------------------------------

/// La familia a la que pertenece un mundo. Decide nivel del mar, paleta y si
/// hay civilización. La Tierra es el hogar; los demás son sitios donde SE PUEDE
/// vivir, cada uno con su propio problema.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Archetype {
    /// Templado: océanos, bosques, desiertos, sierras, gente. La Tierra.
    Terran,
    /// Casi todo mar, unas pocas islas. Vivir aquí es vivir en la costa.
    Oceanic,
    /// Roca y arena, sin mar; hielo en los extremos.
    Desert,
    /// Corteza helada con islas de roca asomando.
    Icy,
    /// Basalto, lava y ceniza. Hostil y espectacular.
    Volcanic,
    /// Regolito sin aire: la Luna, Mercurio, las lunitas.
    Barren,
}

pub fn archetype(body: CelestialBody) -> Archetype {
    use CelestialBody as B;
    match body {
        B::Earth => Archetype::Terran,
        B::Titan | B::Neptune => Archetype::Oceanic,
        B::Mars | B::Uranus => Archetype::Desert,
        B::Europa => Archetype::Icy,
        B::Venus | B::Io => Archetype::Volcanic,
        _ => Archetype::Barren,
    }
}

/// Nivel del mar del arquetipo, en unidades de elevación 0..1. Es EL parámetro
/// que cambia la sensación de un mundo: subirlo lo llena de agua, bajarlo lo
/// vuelve un continente único. 0 = mundo sin mar.
fn sea_level(a: Archetype) -> f32 {
    match a {
        Archetype::Terran => 0.440,
        // Un mundo oceánico tiene que ser HABITABLE, no un charco: con el mar a
        // 0.60 salía 94 % agua y aterrizar era caer al vacío. A 0.515 queda un
        // archipiélago de verdad — mucho más mar que la Tierra, pero con islas
        // donde plantar una base.
        Archetype::Oceanic => 0.495,
        Archetype::Icy => 0.500,
        Archetype::Desert | Archetype::Volcanic | Archetype::Barren => 0.0,
    }
}

/// ¿Este mundo genera civilización (casas, pueblos, carreteras)?
pub fn has_civilisation(body: CelestialBody) -> bool {
    matches!(archetype(body), Archetype::Terran)
}

// ---------------------------------------------------------------------------
// Relieve y clima
// ---------------------------------------------------------------------------

/// Sólo la escala CONTINENTAL del relieve (1 fbm en vez de 4). Es la prueba
/// barata de "¿esto es tierra firme?" que usan los asentamientos y las bocas de
/// cueva, que se consultan para las celdas vecinas de CADA tile: con la
/// elevación completa, generar un chunk costaba decenas de veces más.
pub fn continental_at(body: CelestialBody, tx: i64, tz: i64) -> f32 {
    let h = fbm(body_terrain_seed(body), tx, tz, 9_000);
    let r = body_walk_radius(body).max(1.0);
    let d = ((tx * tx + tz * tz) as f64).sqrt() as f32 * TILE_M / r;
    let shelf = 1.0 - ((d - 0.84) / 0.16).clamp(0.0, 1.0);
    (h * (0.32 + 0.68 * shelf)).clamp(0.0, 1.0)
}

/// Relieve compuesto, 0 (fosa) … 1 (cumbre). Cuatro escalas encajadas.
pub fn elevation_at(body: CelestialBody, tx: i64, tz: i64) -> f32 {
    let seed = body_terrain_seed(body);
    let continental = fbm(seed, tx, tz, 9_000);
    let regional = fbm(seed ^ 0x5EA1_1000, tx, tz, 1_400);
    let local = fbm(seed ^ 0x40C0_2200, tx, tz, 110);
    let detail = fbm(seed ^ 0x0DE7_3300, tx, tz, 22);
    let h = continental * 0.58 + regional * 0.24 + local * 0.13 + detail * 0.05;
    // El mundo es FINITO: los últimos kilómetros se hunden hacia el borde, así
    // el planeta termina en océano abierto en vez de cortarse a pico.
    let r = body_walk_radius(body).max(1.0);
    let d = ((tx * tx + tz * tz) as f64).sqrt() as f32 * TILE_M / r;
    let shelf = 1.0 - ((d - 0.84) / 0.16).clamp(0.0, 1.0);
    (h * (0.32 + 0.68 * shelf)).clamp(0.0, 1.0)
}

/// Humedad, 0 (desierto) … 1 (selva): franjas climáticas grandes + variación
/// regional, para que los biomas sean REGIONES que se atraviesan.
pub fn moisture_at(body: CelestialBody, tx: i64, tz: i64) -> f32 {
    let seed = body_terrain_seed(body);
    let belt = fbm(seed ^ 0x1234_abcd, tx + 900, tz - 400, 5_200);
    let regional = fbm(seed ^ 0x77A5_4400, tx - 300, tz + 700, 700);
    (belt * 0.62 + regional * 0.38).clamp(0.0, 1.0)
}

/// Frío del lugar, 0 (ecuador) … 1 (polo). En un mundo-disco la "latitud" es la
/// distancia al centro, así que los casquetes quedan en el borde… y en las
/// cumbres, que es donde uno espera encontrar nieve.
fn coldness_at(body: CelestialBody, tx: i64, tz: i64, h: f32) -> f32 {
    let r = body_walk_radius(body).max(1.0);
    let d = ((tx * tx + tz * tz) as f64).sqrt() as f32 * TILE_M / r;
    let polar = ((d - 0.66) / 0.26).clamp(0.0, 1.0);
    let altitude = ((h - 0.74) / 0.20).clamp(0.0, 1.0);
    polar.max(altitude)
}

/// Cauce de río: una cresta de ruido sobre un dominio deformado. El truco es que
/// el cauce se ENSANCHA a poca altura, así los ríos nacen finos en la sierra y
/// desembocan anchos — corren cuesta abajo sin simular ninguna hidrología.
fn river_here(body: CelestialBody, tx: i64, tz: i64, h: f32, sea: f32) -> bool {
    if sea <= 0.0 || h <= sea {
        return false;
    }
    let seed = body_terrain_seed(body) ^ 0x21ce_2000;
    // Deformación del dominio: sin esto los ríos son rayas rectas.
    let wx = tx + ((fbm(seed ^ 0xAA, tx, tz, 700) - 0.5) * 420.0) as i64;
    let wz = tz + ((fbm(seed ^ 0xBB, tx, tz, 700) - 0.5) * 420.0) as i64;
    let ridge = (fbm(seed, wx, wz, 2_600) - 0.5).abs();
    let lowland = 1.0 - ((h - sea) / 0.22).clamp(0.0, 1.0);
    ridge < 0.005 + 0.017 * lowland
}

// ---------------------------------------------------------------------------
// Biomas
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Biome {
    Ocean,
    Beach,
    River,
    Swamp,
    Grassland,
    Forest,
    Savanna,
    Desert,
    Highland,
    Mountain,
    Snow,
    Basalt,
    Ice,
    Regolith,
}

/// El bioma de una celda. Es la decisión de diseño del mundo; el tile concreto
/// sale después (`biome_tile`), así el mismo bioma puede pintarse distinto en
/// cuerpos distintos.
pub fn biome_at(body: CelestialBody, tx: i64, tz: i64) -> Biome {
    let arch = archetype(body);
    let sea = sea_level(arch);
    let h = elevation_at(body, tx, tz);
    let m = moisture_at(body, tx, tz);
    let cold = coldness_at(body, tx, tz, h);

    if sea > 0.0 && h < sea {
        return if cold > 0.75 { Biome::Ice } else { Biome::Ocean };
    }
    if sea > 0.0 && h < sea + 0.020 {
        return Biome::Beach;
    }
    if river_here(body, tx, tz, h, sea) {
        return Biome::River;
    }
    if cold > 0.70 {
        return Biome::Snow;
    }
    if h > 0.780 {
        return Biome::Mountain;
    }
    if h > 0.660 {
        return Biome::Highland;
    }

    match arch {
        Archetype::Terran | Archetype::Oceanic => {
            // Pantano: tierra baja Y húmeda. Es el bioma que más se echa de menos
            // en un mapa grande — da variedad sin inventar tiles.
            if sea > 0.0 && h < sea + 0.070 && m > 0.60 {
                Biome::Swamp
            } else if m > 0.64 {
                Biome::Forest
            } else if m > 0.44 {
                Biome::Grassland
            } else if m > 0.28 {
                Biome::Savanna
            } else {
                Biome::Desert
            }
        }
        Archetype::Desert => {
            if m > 0.72 {
                Biome::Savanna
            } else {
                Biome::Desert
            }
        }
        Archetype::Icy => Biome::Ice,
        Archetype::Volcanic => {
            if h < 0.46 {
                Biome::Basalt
            } else {
                Biome::Desert
            }
        }
        Archetype::Barren => {
            if h < 0.455 {
                Biome::Basalt // mares lunares
            } else {
                Biome::Regolith
            }
        }
    }
}

/// El tile que pinta un bioma. Los sólidos (`Water`, `Stone`, `RockRed`) quedan
/// para lo que de verdad corta el paso: mar, cumbres y macizos.
pub fn biome_tile(body: CelestialBody, biome: Biome, tx: i64, tz: i64) -> Tile {
    let seed = body_terrain_seed(body);
    match biome {
        Biome::Ocean | Biome::River => Tile::Water,
        Biome::Beach => Tile::Sand,
        // El pantano es una mezcla: charcos entre matas. Se cruza bordeando.
        Biome::Swamp => {
            if fbm(seed ^ 0x5A11_0001, tx, tz, 9) < 0.42 {
                Tile::Water
            } else {
                Tile::Dirt
            }
        }
        Biome::Grassland | Biome::Forest => Tile::Grass, // los ÁRBOLES los pone la decoración
        Biome::Savanna => Tile::Dirt,
        Biome::Desert => Tile::Sand,
        Biome::Highland => Tile::Dirt,
        Biome::Mountain => Tile::Stone,
        Biome::Snow | Biome::Ice => Tile::Ice,
        Biome::Basalt => Tile::Basalt,
        Biome::Regolith => Tile::Regolith,
    }
}

/// Cuánta vegetación admite un bioma, 0..1. Lo lee la decoración del cliente:
/// así el bosque es bosque y la sabana tiene cuatro árboles contados.
pub fn vegetation_density(biome: Biome) -> f32 {
    match biome {
        Biome::Forest => 1.0,
        Biome::Swamp => 0.55,
        Biome::Grassland => 0.28,
        Biome::Savanna => 0.10,
        Biome::Highland => 0.08,
        Biome::Beach => 0.03,
        _ => 0.0,
    }
}

// ---------------------------------------------------------------------------
// Civilización: asentamientos por rango y carreteras
// ---------------------------------------------------------------------------

/// Lado de la celda de asentamiento, en tiles. Uno cada 2.6 km reparte ~530
/// núcleos por un planeta de 60 km: te cruzás con uno cada tanto volando, y deja
/// sitio para que una ciudad sea de verdad grande.
pub const SITE_PERIOD: i64 = 2_600;

/// Rango del núcleo: su tamaño, su trama y el ancho de la carretera que llega.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SettlementTier {
    /// Un puñado de casas junto al camino.
    Hamlet,
    /// Calle principal y varias manzanas.
    Town,
    /// Trama urbana con avenidas — adonde llevan las carreteras.
    City,
}

impl SettlementTier {
    /// Radio del núcleo, en tiles.
    pub fn radius(self) -> i64 {
        match self {
            SettlementTier::Hamlet => 40,
            SettlementTier::Town => 130,
            SettlementTier::City => 620, // 1.24 km de punta a punta: un centro urbano
        }
    }
    /// Separación entre calles dentro del núcleo.
    fn block(self) -> i64 {
        match self {
            SettlementTier::Hamlet => 22,
            SettlementTier::Town => 26,
            SettlementTier::City => 34,
        }
    }
    /// Semiancho de la calzada que sale de él.
    fn road_half(self) -> f32 {
        match self {
            SettlementTier::Hamlet => 2.0,
            SettlementTier::Town => 3.0,
            SettlementTier::City => 4.5,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Settlement {
    pub tier: SettlementTier,
    /// Centro del núcleo, en tiles absolutos.
    pub center: (i64, i64),
}

/// El núcleo de una celda de asentamiento, si el sorteo y el terreno lo
/// permiten. Es barata a propósito: la consultan las carreteras para las cuatro
/// celdas vecinas de CADA tile.
pub fn settlement_in(body: CelestialBody, sx: i64, sz: i64) -> Option<Settlement> {
    if !has_civilisation(body) {
        return None;
    }
    let seed = body_terrain_seed(body) ^ 0xb01d_0000;
    let h = hash2(seed, sx, sz);
    // Reparto de rangos: la mayoría son aldeas; una ciudad es un hito.
    let tier = match h % 100 {
        0..=3 => SettlementTier::City,
        4..=20 => SettlementTier::Town,
        21..=52 => SettlementTier::Hamlet,
        _ => return None,
    };
    let margin = tier.radius() + 20;
    let span = (SITE_PERIOD - margin * 2).max(1);
    let cx = sx * SITE_PERIOD + margin + ((h >> 9) as i64).rem_euclid(span);
    let cz = sz * SITE_PERIOD + margin + ((h >> 19) as i64).rem_euclid(span);
    // Sólo se funda en tierra firme y no en la alta montaña. Prueba CONTINENTAL
    // (1 octava): esto se evalúa para 4 celdas vecinas de cada tile del mundo.
    let he = continental_at(body, cx, cz);
    let sea = sea_level(archetype(body));
    if he < sea + 0.02 || he > 0.72 {
        return None;
    }
    Some(Settlement { tier, center: (cx, cz) })
}

/// Qué hay de obra civil en una celda.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Civil {
    /// Calzada sobre tierra.
    Road,
    /// Calzada sobre agua — un puente. Es lo que une dos continentes.
    Bridge,
    /// Suelo urbanizado (acera/solar) dentro de un núcleo.
    Pavement,
}

fn dist_to_segment(p: (f32, f32), a: (f32, f32), b: (f32, f32)) -> f32 {
    let (vx, vz) = (b.0 - a.0, b.1 - a.1);
    let (wx, wz) = (p.0 - a.0, p.1 - a.1);
    let len2 = vx * vx + vz * vz;
    let t = if len2 <= 1e-6 { 0.0 } else { ((wx * vx + wz * vz) / len2).clamp(0.0, 1.0) };
    let (dx, dz) = (wx - vx * t, wz - vz * t);
    (dx * dx + dz * dz).sqrt()
}

/// Obra civil en una celda: carretera, puente o suelo urbano.
///
/// La red une cada núcleo con su vecino al este y al sur. Cada tramo se comba
/// por un punto medio desplazado con hash, así las carreteras serpentean como
/// carreteras y no como una cuadrícula. Donde el tramo cruza agua sale un
/// PUENTE — que es exactamente cómo terminan conectados dos continentes sin que
/// nadie los haya colocado a mano.
pub fn civil_at(body: CelestialBody, tx: i64, tz: i64) -> Option<Civil> {
    if !has_civilisation(body) {
        return None;
    }
    let p = (tx as f32, tz as f32);
    let cx = tx.div_euclid(SITE_PERIOD);
    let cz = tz.div_euclid(SITE_PERIOD);
    let seed = body_terrain_seed(body) ^ 0x20ad_5000;

    let mut on_road = false;
    for dx in -1..=0 {
        for dz in -1..=0 {
            let (ax, az) = (cx + dx, cz + dz);
            let Some(from) = settlement_in(body, ax, az) else { continue };
            let (fx, fz) = from.center;
            let r = from.tier.radius();
            // Dentro del núcleo: trama urbana (calles + solares).
            if (tx - fx).abs() <= r && (tz - fz).abs() <= r {
                let b = from.tier.block();
                let lx = (tx - fx).rem_euclid(b);
                let lz = (tz - fz).rem_euclid(b);
                // Calzada · acera · y el INTERIOR de la manzana sin pavimentar.
                // Asfaltar la manzana entera dejaba la ciudad como una explanada
                // de hormigón: con el interior en terreno natural quedan patios
                // y jardines entre las casas, que es como se lee una ciudad.
                return if lx < 3 || lz < 3 {
                    Some(Civil::Road)
                } else if lx < 5 || lz < 5 {
                    Some(Civil::Pavement)
                } else {
                    None
                };
            }
            for (nx, nz) in [(ax + 1, az), (ax, az + 1)] {
                let Some(to) = settlement_in(body, nx, nz) else { continue };
                let a = (fx as f32, fz as f32);
                let b = (to.center.0 as f32, to.center.1 as f32);
                let hh = hash2(seed, ax + nx * 7, az + nz * 13);
                let bend = ((hh & 0xffff) as f32 / 65535.0 - 0.5) * 0.22;
                // Punto medio desplazado en perpendicular: la curva del tramo.
                let mid = (
                    (a.0 + b.0) * 0.5 - (b.1 - a.1) * bend,
                    (a.1 + b.1) * 0.5 + (b.0 - a.0) * bend,
                );
                let half = from.tier.road_half().max(to.tier.road_half());
                let d = dist_to_segment(p, a, mid).min(dist_to_segment(p, mid, b));
                if d <= half {
                    on_road = true;
                }
            }
        }
    }
    if !on_road {
        return None;
    }
    // Sobre agua la calzada es puente; sobre tierra, carretera. Se decide con el
    // tile natural, que a esta altura del cálculo ya suele estar en caché de CPU.
    Some(match natural_tile(body, tx, tz) {
        Tile::Water => Civil::Bridge,
        _ => Civil::Road,
    })
}

/// El tile de una pieza de obra civil. Reutiliza tiles caminables — el basalto
/// oscuro hace de asfalto y la piedra de acera — en vez de inventar más.
pub fn civil_tile(c: Civil) -> Tile {
    match c {
        Civil::Road | Civil::Bridge => Tile::Basalt,
        Civil::Pavement => Tile::Stone,
    }
}

/// El tile natural (sin obra civil ni edificios) de una celda de superficie.
pub fn natural_tile(body: CelestialBody, tx: i64, tz: i64) -> Tile {
    biome_tile(body, biome_at(body, tx, tz), tx, tz)
}


// ---------------------------------------------------------------------------
// Edificios: manzanas dentro de los núcleos
// ---------------------------------------------------------------------------

/// La huella de un edificio generado: rectángulo + celda de puerta. Las paredes
/// y el suelo salen como TILES normales (`building_tile`), así colisionan, se
/// excavan y se persisten igual que todo lo demás; el cliente sólo añade el
/// techo a partir de estos mismos datos.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BuildingSite {
    pub min: (i64, i64),
    pub w: i64,
    pub h: i64,
    /// Celda absoluta de la puerta.
    pub door: (i64, i64),
}

/// El edificio de una MANZANA del núcleo `s`, si esa manzana está edificada.
///
/// Las manzanas son la retícula que dejan las calles (`civil_at`), y la casa se
/// planta dentro con su puerta hacia la calle. Por eso una ciudad se ve como
/// una ciudad —fachadas alineadas a la avenida— en vez de casas sueltas tiradas
/// por el campo, que es lo que salía cuando cada celda de 1.8 km ponía una casa
/// y ya.
pub fn building_in_block(
    body: CelestialBody,
    s: Settlement,
    bx: i64,
    bz: i64,
) -> Option<BuildingSite> {
    let block = s.tier.block();
    let (cx, cz) = s.center;
    // Origen de la manzana, justo después de la calle (3 tiles de calzada).
    let ox = cx + bx * block + 3;
    let oz = cz + bz * block + 3;
    // ¿Cae la manzana dentro del núcleo?
    let r = s.tier.radius();
    if (ox - cx).abs() > r || (oz - cz).abs() > r {
        return None;
    }
    let seed = body_terrain_seed(body) ^ 0xb1_0c_0000;
    let h = hash2(seed, ox, oz);
    // Densidad por rango: la ciudad tiene casi todas las manzanas edificadas;
    // la aldea, cuatro casas contadas.
    let filled = match s.tier {
        SettlementTier::City => 82,
        SettlementTier::Town => 58,
        SettlementTier::Hamlet => 30,
    };
    if h % 100 >= filled {
        return None;
    }
    // La casa deja un patio dentro de la manzana.
    let inner = block - 4;
    let w = (5 + (h >> 7) as i64 % (inner - 5).max(1)).min(inner);
    let hh = (4 + (h >> 13) as i64 % (inner - 4).max(1)).min(inner);
    let min = (ox + ((h >> 19) as i64).rem_euclid((inner - w).max(1)), oz);
    // Puerta al norte, mirando a la calle de la que cuelga la manzana.
    let door = (min.0 + w / 2, min.1);
    // No se construye sobre agua ni en la roca.
    if natural_tile(body, min.0, min.1).solid() {
        return None;
    }
    Some(BuildingSite { min, w, h: hh, door })
}

/// El núcleo cuyo casco contiene este tile, si alguno.
pub fn settlement_covering(body: CelestialBody, tx: i64, tz: i64) -> Option<Settlement> {
    let cx = tx.div_euclid(SITE_PERIOD);
    let cz = tz.div_euclid(SITE_PERIOD);
    for dx in -1..=1 {
        for dz in -1..=1 {
            // `continue`, no `?`: una celda vecina vacía no puede cancelar la
            // búsqueda entera — ese `?` dejaba TODAS las ciudades sin edificios.
            let Some(s) = settlement_in(body, cx + dx, cz + dz) else { continue };
            let r = s.tier.radius();
            if (tx - s.center.0).abs() <= r && (tz - s.center.1).abs() <= r {
                return Some(s);
            }
        }
    }
    None
}

/// El edificio que ocupa este tile, si lo hay.
pub fn building_at(body: CelestialBody, tx: i64, tz: i64) -> Option<BuildingSite> {
    let s = settlement_covering(body, tx, tz)?;
    let block = s.tier.block();
    let bx = (tx - s.center.0).div_euclid(block);
    let bz = (tz - s.center.1).div_euclid(block);
    let b = building_in_block(body, s, bx, bz)?;
    let (dx, dz) = (tx - b.min.0, tz - b.min.1);
    if dx < 0 || dz < 0 || dx >= b.w || dz >= b.h {
        return None;
    }
    Some(b)
}

/// El tile de un edificio generado en esta celda.
pub fn building_tile(body: CelestialBody, tx: i64, tz: i64) -> Option<Tile> {
    let b = building_at(body, tx, tz)?;
    if (tx, tz) == b.door {
        return Some(Tile::Door);
    }
    let (dx, dz) = (tx - b.min.0, tz - b.min.1);
    let perimeter = dx == 0 || dz == 0 || dx == b.w - 1 || dz == b.h - 1;
    Some(if perimeter { Tile::WoodWall } else { Tile::WoodFloor })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn earth_has_a_real_geography() {
        let mut seen = std::collections::HashSet::new();
        for i in -20..20 {
            for j in -20..20 {
                seen.insert(biome_at(CelestialBody::Earth, i * 700, j * 700));
            }
        }
        for want in [Biome::Ocean, Biome::Beach, Biome::Grassland, Biome::Forest] {
            assert!(seen.contains(&want), "falta {want:?} en la Tierra: {seen:?}");
        }
        assert!(seen.len() >= 6, "geografia pobre: {seen:?}");
    }

    #[test]
    fn rivers_exist() {
        let mut rivers = 0;
        for i in -70..70 {
            for j in -70..70 {
                if biome_at(CelestialBody::Earth, i * 80, j * 80) == Biome::River {
                    rivers += 1;
                }
            }
        }
        assert!(rivers > 0, "un planeta sin un solo rio");
    }

    #[test]
    fn every_world_is_walkable_somewhere() {
        // Ningún arquetipo puede salir casi todo sólido: aterrizar ahí sería
        // quedar enterrado.
        for &b in CelestialBody::all() {
            if !crate::celestial_bodies::body_landable(b) {
                continue;
            }
            let mut walkable = 0;
            for i in -12..12 {
                for j in -12..12 {
                    if !natural_tile(b, i * 300, j * 300).solid() {
                        walkable += 1;
                    }
                }
            }
            // 15 % es el mínimo: un mundo oceánico ES mayormente agua a
            // propósito, pero tiene que haber isla donde posarse y vivir.
            assert!(walkable > 90, "{b:?} casi sin suelo pisable: {walkable}/576");
        }
    }

    #[test]
    fn roads_connect_settlements_and_bridge_water() {
        // Recorriendo el segmento entre dos núcleos vecinos tiene que haber
        // calzada; y si cruza agua, tiene que ser puente, no un salto de fe.
        let body = CelestialBody::Earth;
        let mut found_road = false;
        let mut found_bridge = false;
        'outer: for sx in -22..22 {
            for sz in -22..22 {
                let (Some(a), Some(b)) =
                    (settlement_in(body, sx, sz), settlement_in(body, sx + 1, sz))
                else {
                    continue;
                };
                for k in 0..=90 {
                    let t = k as f32 / 90.0;
                    let x = a.center.0 + ((b.center.0 - a.center.0) as f32 * t) as i64;
                    let z = a.center.1 + ((b.center.1 - a.center.1) as f32 * t) as i64;
                    match civil_at(body, x, z) {
                        Some(Civil::Bridge) => {
                            found_bridge = true;
                            found_road = true;
                        }
                        Some(_) => found_road = true,
                        None => {}
                    }
                }
                if found_road && found_bridge {
                    break 'outer;
                }
            }
        }
        assert!(found_road, "no hay carreteras entre nucleos vecinos");
        assert!(found_bridge, "ninguna carretera cruza agua con puente");
    }

    #[test]
    fn cities_are_rarer_than_hamlets() {
        let (mut cities, mut hamlets) = (0, 0);
        for sx in -30..30 {
            for sz in -30..30 {
                match settlement_in(CelestialBody::Earth, sx, sz).map(|s| s.tier) {
                    Some(SettlementTier::City) => cities += 1,
                    Some(SettlementTier::Hamlet) => hamlets += 1,
                    _ => {}
                }
            }
        }
        assert!(hamlets > cities * 3, "ciudades {cities} vs aldeas {hamlets}");
        assert!(cities > 0, "un mundo sin una sola ciudad");
    }

    #[test]
    fn archetypes_are_actually_different_worlds() {
        // Un mundo oceánico tiene que tener MUCHA más agua que la Tierra, y uno
        // desértico, ninguna. Si no, son la Tierra repintada.
        let water_frac = |b: CelestialBody| {
            let mut w = 0;
            for i in -14..14 {
                for j in -14..14 {
                    if natural_tile(b, i * 260, j * 260) == Tile::Water {
                        w += 1;
                    }
                }
            }
            w as f32 / (28.0 * 28.0)
        };
        let earth = water_frac(CelestialBody::Earth);
        let titan = water_frac(CelestialBody::Titan);
        let mars = water_frac(CelestialBody::Mars);
        assert!(titan > earth, "oceanico {titan} <= terrestre {earth}");
        assert!(mars < 0.02, "un desierto con mar: {mars}");
    }

    #[test]
    fn cities_have_streets_and_buildings_along_them() {
        // Una ciudad tiene que tener las dos cosas: calzada y fachadas. Si sólo
        // hay una, es un descampado con rayas o un bloque macizo.
        let body = CelestialBody::Earth;
        let mut city = None;
        'find: for sx in -30..30 {
            for sz in -30..30 {
                if let Some(s) = settlement_in(body, sx, sz) {
                    if s.tier == SettlementTier::City {
                        city = Some(s);
                        break 'find;
                    }
                }
            }
        }
        let s = city.expect("ninguna ciudad en 60x60 celdas");
        let (mut roads, mut walls, mut doors) = (0, 0, 0);
        let r = s.tier.radius();
        for dx in -r..r {
            for dz in -r..r {
                let (x, z) = (s.center.0 + dx, s.center.1 + dz);
                if civil_at(body, x, z) == Some(Civil::Road) {
                    roads += 1;
                }
                match building_tile(body, x, z) {
                    Some(Tile::WoodWall) => walls += 1,
                    Some(Tile::Door) => doors += 1,
                    _ => {}
                }
            }
        }
        assert!(roads > 500, "ciudad sin calles: {roads}");
        assert!(walls > 500, "ciudad sin edificios: {walls}");
        assert!(doors > 20, "edificios sin puertas: {doors}");
    }

    #[test]
    fn only_earth_is_civilised_for_now() {
        assert!(has_civilisation(CelestialBody::Earth));
        for &b in CelestialBody::all() {
            if b != CelestialBody::Earth {
                assert!(civil_at(b, 100, 100).is_none(), "{b:?} con carreteras");
            }
        }
    }
}
