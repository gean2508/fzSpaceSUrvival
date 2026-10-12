//! Generación DETERMINISTA de decoración de superficie (árboles, rocas, flora, props) —
//! **fuente única** para el juego (`ha-game`) Y el editor de mundo (`ha-asset-tools`). Sin
//! Bevy: tamaños como `[f32;2]` y colores fallback como `[f32;3]` (el juego los convierte a
//! Vec2/Color al spawnear; el editor los usa para previsualizar). Ver `PLAN_EDITOR_MUNDO_CAPAS.md`.

use crate::celestial_bodies::CelestialBody;
use crate::geography::{biome_at, vegetation_density, Biome};
use crate::topdown::{building_cell, building_site_in, topdown_tile, SITE_PERIOD};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum DecorKind {
    Tree,
    Rock,
}

pub fn hash_cell(seed: u32, tx: i64, tz: i64) -> u32 {
    let a = (tx as i32 as u32).wrapping_mul(0x9e37_79b9);
    let b = (tz as i32 as u32).wrapping_mul(0x85eb_ca6b);
    let mut x = seed ^ a ^ b;
    x ^= x >> 16;
    x = x.wrapping_mul(0x7feb_352d);
    x ^= x >> 15;
    x = x.wrapping_mul(0x846c_a68b);
    x ^= x >> 16;
    x
}

/// Decor cosechable determinista (árbol/roca) en una celda de superficie
/// caminable. La densidad la manda el BIOMA (`geography::vegetation_density`),
/// no el tile: por eso un bosque se ve bosque y una sabana tiene cuatro árboles
/// contados, aunque los dos pisen césped.
pub fn surface_decor_at(body: CelestialBody, tx: i64, tz: i64) -> Option<DecorKind> {
    let seed = crate::body_terrain_seed(body) ^ 0x1234_abcd;
    let tile = topdown_tile(body, 0, tx, tz);
    // `buildable_ground` y no `!solid()`: desde que el bajío se nada, `!solid()`
    // dice que sí a plantar un roble en medio del lago. Tampoco crece nada sobre
    // una rampa (es un paso, no un terreno).
    if !tile.buildable_ground() {
        return None;
    }
    if building_cell(body, tx, tz).is_some() {
        return None;
    }
    // Nada de árboles en mitad de la calzada.
    if crate::geography::civil_at(body, tx, tz).is_some() {
        return None;
    }
    let biome = biome_at(body, tx, tz);
    let h = hash_cell(seed, tx, tz);
    // Hasta el 14 % de las celdas del bosque llevan árbol; la roca es aparte y
    // aparece en cualquier terreno pelado.
    let tree_pct = (vegetation_density(biome) * 14.0) as u32;
    let roll = h % 100;
    if roll < tree_pct {
        return Some(DecorKind::Tree);
    }
    let rock_pct = match biome {
        Biome::Mountain | Biome::Highland | Biome::Regolith | Biome::Basalt => 6,
        Biome::Desert | Biome::Snow | Biome::Ice => 3,
        _ => 2,
    };
    if roll >= tree_pct && roll < tree_pct + rock_pct {
        return Some(DecorKind::Rock);
    }
    None
}

/// La especie de árbol adecuada al bioma — palmeras en la playa, pinos en la
/// sierra, sauces en el pantano. Es lo que hace que un sitio se reconozca de un
/// vistazo mientras lo sobrevolás.
pub fn biome_tree_key(biome: Biome) -> Option<&'static str> {
    Some(match biome {
        Biome::Beach => "decor/tree_palmera",
        Biome::Swamp => "decor/tree_sauce",
        Biome::Highland | Biome::Snow => "decor/tree_pino",
        Biome::Savanna => "decor/tree_seco",
        Biome::Desert => "decor/tree_arbusto",
        _ => return None,
    })
}

/// Una especie de árbol (variedad VISUAL). `size`=[ancho,alto] en metros; `fallback`=[r,g,b].
pub struct TreeSpecies {
    pub key: &'static str,
    pub size: [f32; 2],
    pub fallback: [f32; 3],
    pub weight: u32,
}

pub const TREE_SPECIES: &[TreeSpecies] = &[
    TreeSpecies { key: "decor/tree_roble",    size: [6.0, 6.5], fallback: [0.24, 0.50, 0.22], weight: 24 },
    TreeSpecies { key: "decor/tree_pino",     size: [4.8, 8.0], fallback: [0.16, 0.42, 0.28], weight: 22 },
    TreeSpecies { key: "decor/tree_abedul",   size: [4.4, 7.0], fallback: [0.48, 0.68, 0.36], weight: 15 },
    TreeSpecies { key: "decor/tree_arce",     size: [5.8, 6.2], fallback: [0.80, 0.48, 0.16], weight: 12 },
    TreeSpecies { key: "decor/tree_sauce",    size: [6.2, 6.0], fallback: [0.42, 0.58, 0.24], weight: 9 },
    TreeSpecies { key: "decor/tree_palmera",  size: [5.4, 7.2], fallback: [0.30, 0.56, 0.34], weight: 6 },
    TreeSpecies { key: "decor/tree_arbusto",  size: [2.4, 1.6], fallback: [0.31, 0.54, 0.26], weight: 7 },
    TreeSpecies { key: "decor/tree_seco",     size: [4.2, 5.6], fallback: [0.44, 0.34, 0.22], weight: 5 },
];

/// Especie de árbol para una celda, sesgada por su bioma: si el bioma tiene una
/// especie característica sale casi siempre, y si no, sorteo ponderado normal.
pub fn pick_tree_species_in(body: CelestialBody, tx: i64, tz: i64) -> &'static TreeSpecies {
    if let Some(key) = biome_tree_key(biome_at(body, tx, tz)) {
        // 3 de cada 4: la especie del bioma domina pero no monopoliza.
        if hash_cell(0x8bee_2f01, tx, tz) % 4 != 0 {
            if let Some(s) = TREE_SPECIES.iter().find(|s| s.key == key) {
                return s;
            }
        }
    }
    pick_tree_species(tx, tz)
}

pub fn pick_tree_species(tx: i64, tz: i64) -> &'static TreeSpecies {
    let total: u32 = TREE_SPECIES.iter().map(|s| s.weight).sum();
    let mut roll = hash_cell(0x7bee_1eaf, tx, tz) % total.max(1);
    for s in TREE_SPECIES {
        if roll < s.weight {
            return s;
        }
        roll -= s.weight;
    }
    &TREE_SPECIES[0]
}

/// Flora/props sólo visuales (sin cosecha ni colisión). `size`=[ancho,alto] en metros.
pub struct ExtraDecor {
    pub key: &'static str,
    pub size: [f32; 2],
    pub weight: u32,
}

pub const NATURE_SPECIES: &[ExtraDecor] = &[
    ExtraDecor { key: "nature/arbusto", size: [1.2, 1.1], weight: 24 },
    ExtraDecor { key: "nature/flores_rojas", size: [0.9, 0.8], weight: 18 },
    ExtraDecor { key: "nature/flores_amarillas", size: [0.9, 0.8], weight: 18 },
    ExtraDecor { key: "nature/pasto", size: [1.0, 0.7], weight: 22 },
    ExtraDecor { key: "nature/tocon", size: [1.0, 0.8], weight: 6 },
    ExtraDecor { key: "nature/tronco", size: [1.5, 0.8], weight: 5 },
    ExtraDecor { key: "nature/hongo", size: [0.7, 0.6], weight: 4 },
    ExtraDecor { key: "nature/cactus", size: [1.0, 1.4], weight: 3 },
];

pub const PROP_SPECIES: &[ExtraDecor] = &[
    ExtraDecor { key: "prop/banco", size: [1.6, 1.0], weight: 10 },
    ExtraDecor { key: "prop/farol", size: [0.8, 2.2], weight: 10 },
    ExtraDecor { key: "prop/cartel", size: [1.0, 1.4], weight: 8 },
    ExtraDecor { key: "prop/barril", size: [0.9, 1.1], weight: 8 },
    ExtraDecor { key: "prop/caja", size: [1.0, 1.0], weight: 8 },
    ExtraDecor { key: "prop/pozo", size: [1.6, 1.8], weight: 3 },
];

/// Flora extra en celdas de pasto/tierra (Tierra): ~4% de celdas candidatas.
pub fn nature_decor_at(body: CelestialBody, tx: i64, tz: i64) -> Option<&'static ExtraDecor> {
    if body != CelestialBody::Earth {
        return None;
    }
    let h = hash_cell(0xf10a_0001, tx, tz);
    if h % 1000 >= 40 {
        return None;
    }
    pick_weighted(NATURE_SPECIES, h >> 10)
}

/// Props cerca de asentamientos (el "patio" del pueblo): ~3% de esas celdas.
pub fn prop_decor_at(body: CelestialBody, tx: i64, tz: i64) -> Option<&'static ExtraDecor> {
    let site = building_site_in(body, tx.div_euclid(SITE_PERIOD), tz.div_euclid(SITE_PERIOD))?;
    let (ox, oz) = site.min;
    let near = tx >= ox - 6 && tx < ox + site.w + 6 && tz >= oz - 6 && tz < oz + site.h + 6;
    let inside = tx >= ox && tx < ox + site.w && tz >= oz && tz < oz + site.h;
    if !near || inside {
        return None;
    }
    let h = hash_cell(0x9209_0002, tx, tz);
    if h % 1000 >= 30 {
        return None;
    }
    pick_weighted(PROP_SPECIES, h >> 10)
}

// ---------------------------------------------------------------------------
// El agua no está vacía: flota, se pesca y se acampa en la orilla
// ---------------------------------------------------------------------------

/// Cosas que FLOTAN. Van sobre celdas de líquido, en capa de piso (no tapan al
/// que nada). Un lago liso de un solo azul se lee como un agujero en el mapa;
/// cuatro troncos y un nenúfar lo convierten en un sitio.
pub const WATER_DECOR: &[ExtraDecor] = &[
    ExtraDecor { key: "agua/tronco_flotante", size: [2.2, 1.0], weight: 20 },
    ExtraDecor { key: "agua/nenufar", size: [1.1, 1.0], weight: 26 },
    ExtraDecor { key: "agua/junco", size: [1.0, 1.6], weight: 22 },
    ExtraDecor { key: "agua/boya", size: [0.8, 1.1], weight: 8 },
    ExtraDecor { key: "agua/flotador", size: [1.2, 1.1], weight: 6 },
    ExtraDecor { key: "agua/estrella_mar", size: [0.8, 0.7], weight: 10 },
];

/// Lo que hay en la ORILLA de una laguna: el campamento y el puesto de pesca.
/// Los `Fishing_Spot_1..4` del pack son cuatro variantes del MISMO objeto, así
/// que van por una sola clave (`fishing/spot`) y el juego elige la variante por
/// celda — ver `ArtLibrary::variant_key_seeded`.
pub const SHORE_DECOR: &[ExtraDecor] = &[
    ExtraDecor { key: "fishing/spot", size: [1.6, 1.4], weight: 30 },
    ExtraDecor { key: "fishing/caña", size: [1.0, 1.8], weight: 10 },
    ExtraDecor { key: "fishing/balde", size: [0.9, 0.9], weight: 8 },
    ExtraDecor { key: "prop/carpa", size: [2.6, 2.2], weight: 12 },
    ExtraDecor { key: "prop/barril", size: [0.9, 1.1], weight: 6 },
    ExtraDecor { key: "nature/tronco", size: [1.5, 0.8], weight: 14 },
    ExtraDecor { key: "prop/mesa_picnic", size: [2.0, 1.6], weight: 8 },
    ExtraDecor { key: "agua/muelle", size: [2.0, 3.0], weight: 6 },
];

/// Decoración de una celda de LÍQUIDO. Densidad baja (~2 %): lo justo para que el
/// agua tenga cosas y no para convertir el lago en un vertedero.
pub fn water_decor_at(body: CelestialBody, tx: i64, tz: i64) -> Option<&'static ExtraDecor> {
    if !crate::relief::is_liquid(body, tx, tz) {
        return None;
    }
    // Sólo en el BAJÍO: el fondo profundo se ve desde arriba y nada flota ahí
    // (además de que un tronco a 8 m de profundidad sería un tronco hundido).
    if crate::relief::relief_level(body, tx, tz) < -1 {
        return None;
    }
    let h = hash_cell(0x1a6e_1001, tx, tz);
    if h % 1000 >= 20 {
        return None;
    }
    pick_weighted(WATER_DECOR, h >> 10)
}

/// Campamento y puesto de pesca en la ORILLA de una laguna: la franja de tierra
/// firme que rodea el agua, hasta 5 tiles.
///
/// Es lo que convierte una charca en un destino: llegás, hay una carpa, una mesa
/// y sitio para pescar. Sin esto un lago es un obstáculo; con esto es un sitio
/// adonde ir.
pub fn shore_decor_at(body: CelestialBody, tx: i64, tz: i64) -> Option<&'static ExtraDecor> {
    // Tiene que ser tierra firme: nada de plantar una carpa dentro del agua.
    if !topdown_tile(body, 0, tx, tz).buildable_ground() {
        return None;
    }
    // ¿Hay agua a menos de 5 tiles? Se prueba en cruz, que basta para una orilla.
    let near_water = [(5, 0), (-5, 0), (0, 5), (0, -5), (3, 3), (-3, -3), (3, -3), (-3, 3)]
        .into_iter()
        .any(|(dx, dz)| crate::relief::is_liquid(body, tx + dx, tz + dz));
    if !near_water {
        return None;
    }
    // Y tiene que ser la orilla de una LAGUNA, no la del océano: el mar abierto
    // tiene su propia costa (playa) y no se acampa en ella por defecto.
    crate::world_map::pond_covering(body, tx, tz)?;
    // Nada de campamentos en mitad de la calzada ni dentro de un pueblo.
    if crate::geography::civil_at(body, tx, tz).is_some() {
        return None;
    }
    let h = hash_cell(0x1a6e_2002, tx, tz);
    if h % 1000 >= 45 {
        return None;
    }
    pick_weighted(SHORE_DECOR, h >> 10)
}

fn pick_weighted(table: &'static [ExtraDecor], roll: u32) -> Option<&'static ExtraDecor> {
    let total: u32 = table.iter().map(|s| s.weight).sum();
    let mut r = roll % total.max(1);
    for s in table {
        if r < s.weight {
            return Some(s);
        }
        r -= s.weight;
    }
    table.first()
}
