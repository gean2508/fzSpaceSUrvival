//! **Las materias en el juego: de dónde salen, qué ítem es cada una y cómo se llaman.**
//!
//! `ha_shared::civ::materia` define la materia oculta del mundo. Este módulo la conecta con la
//! mochila: los CRUDOS son ítems fijos (`clay`, `coal`, `copper_ore`…), repartidos por el mundo en
//! VETAS (una región de 48 celdas tiene una veta de algo, y picar ahí a veces da una pizca); los
//! DERIVADOS son ítems con id `mat_<clase>_<hex>` cuyo nombre y propiedades viven en el cuaderno del
//! jugador y se registran acá para que cualquier lista de la UI los muestre bien.
//!
//! Las vetas salen de la semilla (`partida::semilla`), así que dos mundos reparten distinto y un
//! pueblo sin cobre tiene que comerciarlo: la interdependencia que la civilización necesita para
//! especializarse (§17 regla 7) también existe para el jugador.

use ha_shared::civ::materia::*;
use ha_shared::topdown::Tile;
use std::collections::HashMap;
use std::sync::RwLock;

/// (ítem, material crudo) — los ítems que ya existían y los nuevos.
pub const CRUDOS_ITEMS: &[(&str, u64)] = &[
    ("wood", MADERA),
    ("sticks", MADERA),
    ("stone", PIEDRA),
    ("clay", ARCILLA),
    ("sand", ARENA),
    ("copper_ore", MENA_COBRE),
    ("tin_ore", MENA_ESTANO),
    ("iron_ore", MENA_HIERRO),
    ("coal", CARBON),
    ("wool", FIBRA),
    ("bone", HUESO),
    ("limestone", CALIZA),
    ("resin", RESINA),
    ("salt", SAL),
];

/// El ítem canónico de cada crudo (para devolverle al jugador lo que no se consumió).
pub fn item_de_crudo(id: u64) -> &'static str {
    match id {
        MADERA => "wood",
        PIEDRA => "stone",
        ARCILLA => "clay",
        ARENA => "sand",
        MENA_COBRE => "copper_ore",
        MENA_ESTANO => "tin_ore",
        MENA_HIERRO => "iron_ore",
        CARBON => "coal",
        FIBRA => "wool",
        HUESO => "bone",
        CALIZA => "limestone",
        RESINA => "resin",
        SAL => "salt",
        _ => "stone",
    }
}

pub fn crudo_de_item(item: &str) -> Option<u64> {
    CRUDOS_ITEMS.iter().find(|(i, _)| *i == item).map(|(_, m)| *m)
}

/// El id de ítem de un material derivado: `mat_ceramica_4000000000abcdef`.
pub fn item_de_derivado(m: &Material) -> String {
    format!("mat_{}_{:016x}", slug(m.clase), m.id)
}

fn slug(c: Clase) -> &'static str {
    match c {
        Clase::Metal => "metal",
        Clase::Aleacion => "aleacion",
        Clase::Ceramica => "ceramica",
        Clase::Vidrio => "vidrio",
        Clase::Aglutinante => "aglutinante",
        Clase::Compuesto => "compuesto",
        Clase::Combustible => "combustible",
        _ => "compuesto",
    }
}

/// `(clase en el ícono, id del material)` de un ítem `mat_<clase>_<hex>`.
pub fn derivado_de_item(item: &str) -> Option<(&str, u64)> {
    let r = item.strip_prefix("mat_")?;
    let (clase, hex) = r.rsplit_once('_')?;
    Some((clase, u64::from_str_radix(hex, 16).ok()?))
}

/// El material (crudo o derivado) que representa un ítem, o `None` si no es una materia.
pub fn material_de_item(item: &str) -> Option<u64> {
    crudo_de_item(item).or_else(|| derivado_de_item(item).map(|(_, id)| id))
}

/// El ítem del ícono genérico de una clase (`mat_metal`…): lo que dibuja la mochila.
pub fn icono_generico(item: &str) -> Option<String> {
    derivado_de_item(item).map(|(c, _)| format!("mat_{c}"))
}

// ── nombres y propiedades de los derivados que el jugador conoce ─────────────────────────────

static REGISTRO: RwLock<Option<HashMap<String, (String, Props)>>> = RwLock::new(None);

/// Registra un derivado para que la UI pueda mostrar su nombre.
pub fn registrar(m: &Material) {
    if let Ok(mut g) = REGISTRO.write() {
        g.get_or_insert_with(HashMap::new).insert(item_de_derivado(m), (m.nombre.clone(), m.props));
    }
}

pub fn nombre_derivado(item: &str) -> Option<String> {
    REGISTRO.read().ok()?.as_ref()?.get(item).map(|(n, _)| n.clone())
}

pub fn vaciar_registro() {
    if let Ok(mut g) = REGISTRO.write() {
        *g = None;
    }
}

// ── vetas: dónde hay qué ─────────────────────────────────────────────────────────────────────

const REGION: i64 = 48;

fn h(semilla: u32, x: i64, z: i64, sal: u32) -> u32 {
    ha_shared::worldgen::hash_cell(semilla ^ sal, x, z)
}

/// Qué veta tiene la región de la celda `(tx, tz)`: 0 nada, 1 carbón, 2 cobre y estaño, 3 hierro, 4 caliza.
fn veta_de(semilla: u32, tx: i64, tz: i64) -> u8 {
    let (rx, rz) = (tx.div_euclid(REGION), tz.div_euclid(REGION));
    match h(semilla, rx, rz, 0xA7E7A) % 100 {
        0..=29 => 0,
        30..=44 => 1,
        45..=62 => 2,
        63..=77 => 3,
        _ => 4,
    }
}

/// Lo que da de más picar la celda `(tx, tz)` de tipo `tile`, además de lo de siempre.
pub fn extra_al_picar(semilla: u32, tile: Tile, tx: i64, tz: i64) -> Option<&'static str> {
    let azar = h(semilla, tx, tz, 0x91C0) % 100;
    let piedra = matches!(tile, Tile::Stone | Tile::Rock | Tile::RockRed | Tile::RockDark | Tile::Basalt | Tile::Cliff);
    if piedra {
        if azar >= 22 {
            return None;
        }
        return match veta_de(semilla, tx, tz) {
            1 => Some("coal"),
            2 => Some(if azar % 3 == 0 { "tin_ore" } else { "copper_ore" }),
            3 => Some("iron_ore"),
            4 => Some("limestone"),
            _ => None,
        };
    }
    match tile {
        // La arcilla se junta en los bajos: una región de cada cuatro la tiene.
        Tile::Dirt | Tile::Grass | Tile::Subsoil if h(semilla, tx.div_euclid(REGION), tz.div_euclid(REGION), 0xC1A4) % 4 == 0 && azar < 25 => Some("clay"),
        // La sal está donde hubo mar: una región de cada tres, en la arena.
        Tile::Sand if h(semilla, tx.div_euclid(REGION), tz.div_euclid(REGION), 0x5A17) % 3 == 0 && azar < 12 => Some("salt"),
        _ => None,
    }
}

/// Lo que da de más talar el árbol `id` del chunk `(cx, cz)`: a veces, resina.
pub fn extra_al_talar(semilla: u32, cx: i32, cz: i32, id: u32) -> Option<&'static str> {
    (h(semilla, cx as i64 * 31 + id as i64, cz as i64, 0x4E51) % 100 < 14).then_some("resin")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cada_crudo_tiene_item_y_viceversa() {
        for id in 0..N_CRUDOS {
            if id == AGUA {
                continue; // el agua no es un ítem de mochila
            }
            let it = item_de_crudo(id);
            assert_eq!(crudo_de_item(it), Some(id), "{it}");
        }
    }

    #[test]
    fn el_id_de_un_derivado_va_y_vuelve() {
        let mundo = Materia::nueva(9);
        let Resultado::Obtiene(m, _) = mundo.reaccionar(MENA_COBRE, Some(CARBON), 1.0, false) else { panic!() };
        let item = item_de_derivado(&m);
        assert_eq!(derivado_de_item(&item), Some(("metal", m.id)));
        assert_eq!(material_de_item(&item), Some(m.id));
        assert_eq!(icono_generico(&item).as_deref(), Some("mat_metal"));
        registrar(&m);
        assert_eq!(nombre_derivado(&item), Some(m.nombre.clone()));
    }

    #[test]
    fn las_vetas_son_de_la_semilla_y_hay_de_todo_en_algun_mundo() {
        let mut vistos = std::collections::BTreeSet::new();
        for semilla in 1..=30u32 {
            for rx in -3..3 {
                for rz in -3..3 {
                    vistos.insert(veta_de(semilla, rx * REGION, rz * REGION));
                }
            }
        }
        assert_eq!(vistos.len(), 5, "faltan tipos de veta: {vistos:?}");
        // Determinista.
        assert_eq!(extra_al_picar(7, Tile::Stone, 100, 100), extra_al_picar(7, Tile::Stone, 100, 100));
    }

    #[test]
    fn picar_piedra_da_menas_y_la_tierra_da_arcilla_a_veces() {
        let (mut ore, mut arc) = (0, 0);
        for x in 0..400 {
            for z in 0..40 {
                if matches!(extra_al_picar(3, Tile::Stone, x, z), Some("coal" | "copper_ore" | "tin_ore" | "iron_ore" | "limestone")) {
                    ore += 1;
                }
                if extra_al_picar(3, Tile::Dirt, x, z) == Some("clay") {
                    arc += 1;
                }
            }
        }
        assert!(ore > 100, "casi no hay menas: {ore}");
        assert!(arc > 0, "no hay arcilla en ninguna región de ese mundo");
    }
}
