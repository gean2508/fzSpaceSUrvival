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

pub(crate) fn hash2(seed: u32, a: i64, b: i64) -> u32 {
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

pub(crate) fn fbm(seed: u32, tx: i64, tz: i64, period: i64) -> f32 {
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

// El nivel del mar de cada cuerpo se mudó a `relief::sea_level`: ahí es donde
// vive la escalera de terrazas, y el nivel del mar no es más que su escalón cero.
// Se reexporta para no tocar los call sites históricos.
pub use crate::relief::sea_level;

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
    // **Tiene que coincidir con `relief_field` en el SIGNO** (mar o tierra),
    // aunque no en el detalle: si no, los pueblos se fundan donde esta función
    // cree que hay costa y el terreno de verdad pone océano. Por eso comparte la
    // plataforma de la plantilla y sólo se ahorra tres de las cuatro octavas.
    let h = fbm(body_terrain_seed(body), tx, tz, 9_000);
    let amp = crate::world_map::RELIEF_AMPLITUDE * 0.7;
    (crate::world_map::platform(body, tx, tz) + (h - 0.5) * amp).clamp(0.0, 1.0)
}

/// Relieve compuesto, 0 (fosa) … 1 (cumbre).
///
/// Es exactamente [`crate::relief::relief_field`]: la altura del terreno y el
/// campo que se escalona en terrazas son **la misma cosa**, y tenerlos separados
/// era pedir que se fueran desincronizando (el mapa satelital pintando una loma
/// donde el suelo tenía un acantilado). La octava fina de 22 m que esta función
/// tenía se quitó a propósito — ver la nota en `relief_field`.
pub fn elevation_at(body: CelestialBody, tx: i64, tz: i64) -> f32 {
    crate::relief::relief_field(body, tx, tz)
}

/// Humedad, 0 (desierto) … 1 (selva): franjas climáticas grandes + variación
/// regional, para que los biomas sean REGIONES que se atraviesan.
pub fn moisture_at(body: CelestialBody, tx: i64, tz: i64) -> f32 {
    let seed = body_terrain_seed(body);
    let belt = fbm(seed ^ 0x1234_abcd, tx + 900, tz - 400, 5_200);
    let regional = fbm(seed ^ 0x77A5_4400, tx - 300, tz + 700, 700);
    // Los parches de la plantilla mandan sobre el ruido climático: el Bosque
    // Denso está donde dice el mapa, no donde cayó el fbm. Sin esto, poner un
    // bosque con nombre en el mapa y que salga una estepa era cuestión de suerte.
    let authored = crate::world_map::moisture_bias(body, tx, tz);
    (belt * 0.62 + regional * 0.38 + authored).clamp(0.0, 1.0)
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
    let sea = sea_level(body);
    let h = elevation_at(body, tx, tz);
    let m = moisture_at(body, tx, tz);
    let cold = coldness_at(body, tx, tz, h);

    // EL BIOMA SIGUE AL ESCALÓN, no a la altura cruda: así el bioma y la geometría
    // no pueden discrepar (nunca más una playa colgada en mitad de un acantilado).
    let level = crate::relief::relief_level(body, tx, tz);
    if level < 0 {
        return if cold > 0.75 { Biome::Ice } else { Biome::Ocean };
    }
    // Playa: el pie de la primera terraza de tierra. La franja mide un cuarto de
    // escalón, así que en una costa tendida sale una playa ancha y en una brava
    // apenas una orilla — que es como se comportan las playas.
    if sea > 0.0 && level == 0 {
        let step = (1.0 - sea).max(1e-3) / crate::relief::LEVELS_UP as f32;
        if h < sea + step * 0.25 {
            return Biome::Beach;
        }
    }
    if river_here(body, tx, tz, h, sea) {
        return Biome::River;
    }
    if cold > 0.70 {
        return Biome::Snow;
    }
    // Montaña y sierra por ESCALÓN: 8 de 12 escalones arriba es alta montaña.
    if level >= 8 {
        return Biome::Mountain;
    }
    if level >= 5 {
        return Biome::Highland;
    }

    match arch {
        Archetype::Terran | Archetype::Oceanic => {
            // Pantano: tierra baja Y húmeda. Es el bioma que más se echa de menos
            // en un mapa grande — da variedad sin inventar tiles.
            if sea > 0.0 && level <= 1 && m > 0.60 {
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
            // Las planicies bajas son basalto; a partir del tercer escalón, ceniza.
            if level <= 2 {
                Biome::Basalt
            } else {
                Biome::Desert
            }
        }
        Archetype::Barren => {
            // Los MARES lunares son las cuencas: los dos escalones más bajos.
            if level <= 1 {
                Biome::Basalt
            } else {
                Biome::Regolith
            }
        }
    }
}

/// El tile que pinta un bioma. Los sólidos (`WaterDeep`, `Stone`, `RockRed`)
/// quedan para lo que de verdad corta el paso: mar abierto, cumbres y macizos.
pub fn biome_tile(body: CelestialBody, biome: Biome, tx: i64, tz: i64) -> Tile {
    let seed = body_terrain_seed(body);
    match biome {
        // El océano se divide en bajío (se nada) y fondo (bloquea) según el
        // escalón: por eso se puede entrar al mar caminando desde la playa y a la
        // vez el océano abierto sigue siendo un límite.
        Biome::Ocean => crate::relief::water_tile(crate::relief::relief_level(body, tx, tz))
            .unwrap_or(Tile::Water),
        // Los ríos son SIEMPRE vadeables: un río que bloquea es un muro que parte
        // el mapa en dos sin avisar.
        Biome::River => Tile::Water,
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
        // La sierra y la montaña son ALTURA, no material: de qué están hechas lo
        // decide el cuerpo. Sin esto, las tierras altas de la Luna salían de
        // TIERRA VEGETAL — un altiplano de humus en un mundo sin aire.
        Biome::Highland => match archetype(body) {
            Archetype::Barren => Tile::Regolith,
            Archetype::Volcanic => Tile::Basalt,
            Archetype::Icy => Tile::Ice,
            Archetype::Desert => Tile::RockRed,
            _ => Tile::Dirt,
        },
        // `Rock` y no `Stone`: la cumbre SE PISA. La roca maciza que bloquea es
        // la del subsuelo; en superficie, lo que corta el paso es el acantilado.
        Biome::Mountain => match archetype(body) {
            Archetype::Icy => Tile::Ice,
            _ => Tile::Rock,
        },
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
    /// Trama urbana. **Es lo que rompe el damero**: hasta que existió, las cuatro
    /// ciudades del mundo eran la misma ciudad con otro hash y el planeta entero
    /// se veía desde el aire como un tablero de ajedrez.
    pub style: CityStyle,
    /// Nombre, si es una ciudad AUTORADA en `world_map`. Las procedurales no
    /// tienen: son "un pueblo", y está bien que lo sean.
    pub name: Option<&'static str>,
    /// Radio del casco en tiles. Las autoradas lo declaran (una metrópolis mide
    /// lo que mide); las procedurales lo heredan de su rango.
    pub radius: i64,
}

pub use crate::world_map::CityStyle;

/// El núcleo de una celda de asentamiento, si el sorteo y el terreno lo
/// permiten. Es barata a propósito: la consultan las carreteras para las cuatro
/// celdas vecinas de CADA tile.
pub fn settlement_in(body: CelestialBody, sx: i64, sz: i64) -> Option<Settlement> {
    if !has_civilisation(body) {
        return None;
    }
    // LAS CIUDADES AUTORADAS MANDAN. Si el mapa del cuerpo declara una ciudad con
    // nombre en esta celda, ésa es la ciudad: sitio, tamaño y trama fijos. El
    // sorteo por hash sólo puebla lo que queda entre ellas — pueblos y aldeas,
    // que es exactamente el papel que les toca.
    if let Some(town) = crate::world_map::town_in_site(body, sx, sz, SITE_PERIOD) {
        return Some(Settlement {
            tier: SettlementTier::City,
            center: crate::world_map::town_center(body, town),
            style: town.style,
            name: Some(town.name),
            radius: town.radius,
        });
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
    let sea = sea_level(body);
    if he < sea + 0.02 || he > 0.72 {
        return None;
    }
    // No se funda un pueblo pegado a una ciudad autorada: quedaría un suburbio
    // fantasma pisándole las calles.
    if crate::world_map::town_covering(body, cx, cz).is_some() {
        return None;
    }
    Some(Settlement {
        tier,
        center: (cx, cz),
        // Los núcleos procedurales son rurales salvo los que llegan a ciudad, que
        // se ganan una retícula.
        style: match tier {
            SettlementTier::City => CityStyle::Grid,
            _ => CityStyle::Rural,
        },
        name: None,
        radius: tier.radius(),
    })
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
    match civil_kind_at(body, tx, tz)? {
        // La acera no cambia por lo que tenga debajo.
        Civil::Pavement => Some(Civil::Pavement),
        // Sobre líquido la calzada es puente; sobre tierra, carretera. Cuenta el
        // bajío igual que el fondo: un puente que sólo cubre la parte honda te
        // deja vadeando el último tramo, que se lee como un puente roto.
        _ => Some(match natural_tile(body, tx, tz) {
            Tile::Water | Tile::WaterDeep => Civil::Bridge,
            _ => Civil::Road,
        }),
    }
}

/// Obra civil **sin mirar el terreno**: la geometría pura de la red (dónde hay
/// calzada y dónde acera). Nunca devuelve `Bridge`, porque eso depende de si
/// debajo hay agua.
///
/// Existe separada de [`civil_at`] por una razón concreta: el RELIEVE la consulta
/// para abrir rampas donde pasa un camino, y `civil_at` consulta el relieve para
/// decidir si es puente. Llamarse entre sí era una recursión infinita — desbordó
/// la pila en el primer test que la tocó. Partir la función en "geometría" y
/// "geometría + terreno" rompe el ciclo sin duplicar ninguna regla.
pub fn civil_kind_at(body: CelestialBody, tx: i64, tz: i64) -> Option<Civil> {
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
            let r = from.radius;
            // Dentro del núcleo: trama urbana (calles + solares).
            if (tx - fx).abs() <= r && (tz - fz).abs() <= r {
                // TRAMA URBANA CON JERARQUÍA. El tamaño de manzana sale de la
                // TRAMA de la ciudad (`roads::block_size`), no de su rango, y cada
                // línea de la retícula es avenida, calle o callejón según dónde
                // cae. Antes todas las calles medían 3 tiles y todas las manzanas
                // lo mismo: desde el aire, un tablero de ajedrez idéntico en las
                // cuatro ciudades del mundo. Con avenidas cada pocas manzanas la
                // ciudad se puede recorrer y —lo que importa— se puede conducir.
                let b = crate::roads::block_size(from.style, from.tier);
                let (bix, biz) = ((tx - fx).div_euclid(b), (tz - fz).div_euclid(b));
                let lx = (tx - fx).rem_euclid(b);
                let lz = (tz - fz).rem_euclid(b);
                let class = crate::roads::street_class(from.style, bix, biz);
                let road = class.half_width().ceil() as i64;
                let walk = class.half_shoulder().ceil() as i64;
                // Calzada · acera · y el INTERIOR de la manzana sin pavimentar.
                // Asfaltar la manzana entera dejaba la ciudad como una explanada
                // de hormigón: con el interior en terreno natural quedan patios
                // y jardines entre las casas, que es como se lee una ciudad.
                return if lx < road || lz < road {
                    Some(Civil::Road)
                } else if lx < walk || lz < walk {
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
                // El ancho del enlace sale del RANGO de los dos extremos: dos
                // ciudades se unen por autopista, a una aldea le llega una ruta.
                let half = crate::roads::link_class(from.tier, to.tier).half_width();
                let d = dist_to_segment(p, a, mid).min(dist_to_segment(p, mid, b));
                if d <= half {
                    on_road = true;
                }
            }
        }
    }
    on_road.then_some(Civil::Road)
}

/// El tile de una pieza de obra civil. Reutiliza tiles caminables — el basalto
/// oscuro hace de asfalto y la piedra de acera — en vez de inventar más.
pub fn civil_tile(c: Civil) -> Tile {
    match c {
        Civil::Road | Civil::Bridge => Tile::Basalt,
        // `Pavement` y no `Stone`: la roca del subsuelo es SÓLIDA, así que las
        // aceras de todas las ciudades eran muros. Ver `Tile::Pavement`.
        Civil::Pavement => Tile::Pavement,
    }
}

/// El tile natural (sin obra civil ni edificios) de una celda de superficie.
///
/// **El relieve manda sobre el bioma**: un acantilado es un acantilado en el
/// desierto, en el bosque y en la Luna — es geometría, no clima. Lo que el bioma
/// decide es su ARTE (`cliff/arena` vs `cliff/pasto`), que elige el cliente.
pub fn natural_tile(body: CelestialBody, tx: i64, tz: i64) -> Tile {
    if let Some(t) = crate::relief::relief_tile(body, tx, tz) {
        return t;
    }
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
    // El tamaño de manzana y el ancho de la calle salen de la TRAMA de la ciudad,
    // los mismos que dibuja `civil_at`. Antes eran `s.tier.block()` y un `+3`
    // hardcodeado que suponía calles de 3 tiles: en cuanto las avenidas pasaron a
    // medir 8, las casas empezaron a plantarse ENCIMA DE LA CALZADA — un muro de
    // madera cruzado en mitad de la autopista. Lo cazó `no_road_is_ever_blocked_by_a_cliff`.
    let block = crate::roads::block_size(s.style, s.tier);
    let street = crate::roads::street_class(s.style, bx, bz);
    let margin = street.half_shoulder().ceil() as i64;
    let (cx, cz) = s.center;
    // Origen de la manzana: justo después de la acera de su calle.
    let ox = cx + bx * block + margin;
    let oz = cz + bz * block + margin;
    // ¿Cae la manzana dentro del núcleo?
    let r = s.radius;
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
    // La casa deja un patio dentro de la manzana. El interior ÚTIL es la manzana
    // menos las dos aceras (la de esta calle y la de la siguiente), no un `-4`
    // fijo: con avenidas de 8 tiles ese margen se quedaba corto por el lado sur y
    // la casa invadía la calle de la manzana de abajo.
    let inner = (block - margin * 2).max(4);
    let w = (5 + (h >> 7) as i64 % (inner - 5).max(1)).min(inner);
    let hh = (4 + (h >> 13) as i64 % (inner - 4).max(1)).min(inner);
    let min = (ox + ((h >> 19) as i64).rem_euclid((inner - w).max(1)), oz);
    // Puerta al norte, mirando a la calle de la que cuelga la manzana.
    let door = (min.0 + w / 2, min.1);
    // No se construye sobre agua, sobre roca ni colgando de un acantilado.
    // `buildable_ground` y no `!solid()`: desde que el bajío se nada, `!solid()`
    // decía que sí a fundar una casa dentro del lago.
    if !natural_tile(body, min.0, min.1).buildable_ground() {
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
            let r = s.radius;
            if (tx - s.center.0).abs() <= r && (tz - s.center.1).abs() <= r {
                return Some(s);
            }
        }
    }
    None
}

/// El edificio que ocupa este tile, si lo hay.
///
/// Devuelve la huella **techada** (la casa, no el jardín ni la pileta): es lo que
/// el cliente necesita para poner el techo y hacerlo transparente al entrar.
pub fn building_at(body: CelestialBody, tx: i64, tz: i64) -> Option<BuildingSite> {
    let s = settlement_covering(body, tx, tz)?;
    let (x, z, w, h) = crate::city::roofed_rect(body, &s, tx, tz)?;
    if tx < x || tz < z || tx >= x + w || tz >= z + h {
        return None;
    }
    Some(BuildingSite { min: (x, z), w, h, door: (x + w / 2, z + h - 1) })
}

/// El tile de un edificio generado en esta celda.
///
/// **Delega en `city::lot_tile`**, que es quien sabe de parcelas: casa + garaje +
/// entrada + pileta + cerca, con el programa y el tamaño que le toque al lote. La
/// versión vieja pintaba un rectángulo de madera por manzana y ya — de ahí salía
/// el damero de cajas idénticas.
pub fn building_tile(body: CelestialBody, tx: i64, tz: i64) -> Option<Tile> {
    let s = settlement_covering(body, tx, tz)?;
    crate::city::lot_tile(body, &s, tx, tz)
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
            let r = body_walk_radius(b).max(1.0) as i64;
            for i in -12..12 {
                for j in -12..12 {
                    // `buildable_ground`: tierra donde PLANTARSE. Con `!solid()`
                    // el bajío contaría como suelo y un mundo hecho de charcos
                    // pasaría el test sin tener dónde poner una base.
                    if natural_tile(b, i * r / 12, j * r / 12).buildable_ground() {
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
        // La red vial existe, une núcleos y **cruza el agua con puente** en vez de
        // cortarse en la orilla.
        //
        // Se barre una franja del mundo en vez de caminar el segmento recto entre
        // dos núcleos, que es lo que hacía antes: los tramos se COMBAN por un punto
        // medio desplazado hasta un 22 % de su longitud (`civil_at`), o sea hasta
        // ~570 tiles en un tramo de 2.6 km. Muestrear la recta era muestrear campo
        // vacío — encontraba calzada sólo cerca de los extremos y no veía un puente
        // en su vida. Este barrido mira dónde ESTÁ la carretera, no dónde debería.
        let body = CelestialBody::Earth;
        let (mut roads, mut bridges, mut pavement) = (0, 0, 0);
        let r = body_walk_radius(body) as i64;
        for tz in (-r / 2..r / 2).step_by(7) {
            for tx in (-r / 2..r / 2).step_by(7) {
                match civil_at(body, tx, tz) {
                    Some(Civil::Road) => roads += 1,
                    Some(Civil::Bridge) => bridges += 1,
                    Some(Civil::Pavement) => pavement += 1,
                    None => {}
                }
            }
        }
        assert!(roads > 200, "apenas {roads} celdas de calzada en medio planeta");
        assert!(pavement > 50, "ciudades sin acera: {pavement}");
        assert!(bridges > 0, "ninguna carretera cruza agua con puente");
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
        // Cada arquetipo tiene su propia cantidad de líquido. TODOS los cuerpos
        // con atmósfera tienen algo (el juego apunta a divertido, no a realista:
        // un lago de metano en Titán y una salmuera marciana son sitios adonde ir),
        // pero un mundo oceánico tiene que ser MUCHO más líquido que la Tierra y
        // un desierto, mucho menos. Si no, son la Tierra repintada.
        // El muestreo se escala al RADIO del cuerpo y cubre el planeta entero. La
        // rejilla fija de ±3.6 km que había sólo veía el continente central, así
        // que medía "cuánta agua hay tierra adentro" — que en la Tierra es casi
        // nada — y comparaba eso con el océano de Marte. Comparación sin sentido.
        let water_frac = |b: CelestialBody| {
            let r = body_walk_radius(b).max(1.0) as i64;
            let mut w = 0;
            for i in -14..14 {
                for j in -14..14 {
                    if matches!(
                        natural_tile(b, i * r / 14, j * r / 14),
                        Tile::Water | Tile::WaterDeep
                    ) {
                        w += 1;
                    }
                }
            }
            w as f32 / (28.0 * 28.0)
        };
        let earth = water_frac(CelestialBody::Earth);
        let titan = water_frac(CelestialBody::Titan);
        let mars = water_frac(CelestialBody::Mars);
        let moon = water_frac(CelestialBody::Moon);
        assert!(titan > earth, "oceanico {titan} <= terrestre {earth}");
        // Medido: tierra 0.32 · titán 0.41 · marte 0.17 · luna 0.00. Lo que el
        // test defiende es la RELACIÓN entre arquetipos, no un número exacto: un
        // desierto tiene claramente menos mar que un mundo templado, pero tiene.
        assert!(mars < earth * 0.7, "el desierto tiene tanto mar como la Tierra: {mars}");
        assert!(mars > 0.0, "un desierto sin un solo lago adonde ir: {mars}");
        assert_eq!(moon, 0.0, "la Luna con mares líquidos: {moon}");
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
        let r = s.radius;
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
