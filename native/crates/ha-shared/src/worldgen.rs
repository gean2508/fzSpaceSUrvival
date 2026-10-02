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
    // **Memoria por celda**, igual que `topdown_tile`: la colisión de decoración del que
    // camina pregunta por decenas de celdas por tick y cada respuesta recorre tile,
    // bioma, calles vecinas y retiro del canto (medido en el Redmi: `ob_decor` = 9,2 ms
    // por cuadro caminando). Pura dada la semilla, la época y las zonas autoradas.
    if crate::grueso::activo() {
        return surface_decor_at_calc(body, tx, tz);
    }
    const RANURAS: usize = 4096;
    type Clave = (u8, i64, i64, u32, u32);
    thread_local! {
        static MEMO: std::cell::RefCell<Vec<Option<(Clave, Option<DecorKind>)>>> =
            std::cell::RefCell::new(vec![None; RANURAS]);
    }
    let clave: Clave = (body as u8, tx, tz, crate::partida::epoca(), crate::zonas::generacion());
    let h = (tx as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15) ^ (tz as u64).wrapping_mul(0xC2B2_AE3D_27D4_EB4F);
    let ranura = (h >> 40) as usize % RANURAS;
    if let Some(v) = MEMO.with(|m| m.borrow()[ranura].as_ref().and_then(|(c, v)| (*c == clave).then_some(*v))) {
        return v;
    }
    let v = surface_decor_at_calc(body, tx, tz);
    MEMO.with(|m| m.borrow_mut()[ranura] = Some((clave, v)));
    v
}

fn surface_decor_at_calc(body: CelestialBody, tx: i64, tz: i64) -> Option<DecorKind> {
    let seed = crate::body_terrain_seed(body) ^ 0x1234_abcd;
    let tile = topdown_tile(body, 0, tx, tz);
    // `buildable_ground` y no `!solid()`: desde que el bajío se nada, `!solid()`
    // dice que sí a plantar un roble en medio del lago. Tampoco crece nada sobre
    // una rampa (es un paso, no un terreno).
    if !tile.buildable_ground() {
        return None;
    }
    // **NADA SE SIEMBRA PEGADO AL FILO.** El canto ya es `Tile::Cliff` y no
    // recibe nada, pero la celda de ADENTRO era terreno llano y edificable, así
    // que el sorteo le plantaba el árbol ahí y medio tronco quedaba colgando
    // sobre la pared. El retiro es de la meseta hacia adentro
    // (`relief::retiro_del_canto`), no del canto hacia afuera.
    if crate::relief::retiro_del_canto(body, tx, tz) {
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
    // El sorteo va POR MIL, no por ciento: la densidad que hace falta es más fina
    // que un punto porcentual (ver `TREE_PERMILLE_MAX`).
    let tree_permille = (vegetation_density(biome) * TREE_PERMILLE_MAX) as u32;
    let roll = h % 1000;
    // La roca es aparte y aparece en cualquier terreno pelado.
    let rock_permille = match biome {
        Biome::Mountain | Biome::Highland | Biome::Regolith | Biome::Basalt => 60,
        Biome::Desert => 30,
        // **EN LA NIEVE NO ASOMA PIEDRA, y no es por diseño: es por el dibujo.**
        //
        // Una piedra asomando entre la nieve sería exactamente lo que tiene que
        // verse en una cumbre, y así estaba puesto (30 ‰). Pero `decor/rock` es
        // el único que tiene pixel —`rock_moon` y `rock_mars` son slots vacíos— y
        // **viene con su óvalo de pasto dibujado debajo**, que es lo correcto
        // sobre pradera y lo que hace que sobre nieve cada piedra sea una mancha
        // VERDE. Se vio en `relieve-3-cumbre.png` el 14 ago 2026: la cumbre con
        // lunares de césped.
        //
        // No se le recorta la base al asset (memoria: el asset se usa como está
        // dibujado). El día que `decor/rock_moon` tenga pixel —una piedra con
        // base neutra— la cumbre lo pide y esto vuelve a 30.
        Biome::Snow | Biome::Ice => 0,
        _ => 20,
    };
    let kind = if roll < tree_permille {
        DecorKind::Tree
    } else if roll < tree_permille + rock_permille {
        DecorKind::Rock
    } else {
        return None;
    };
    // Dentro de un pueblo el monte NO crece solo — lo último que se comprueba
    // porque es lo más caro y sólo hace falta para la celda que ya sacó decor.
    if urban_decor_blocked(body, tx, tz) {
        return None;
    }
    // **LA COPA NO CUELGA SOBRE LA RUTA** (28 sep 2026). Un árbol plantado al lado
    // del asfalto tapaba media calzada con su copa. Retiro de dos celdas a los
    // cuatro lados (tres celdas: la copa grande mide cuatro de ancho); sólo se
    // paga en la celda que ya sacó árbol.
    if kind == DecorKind::Tree {
        for (dx, dz) in [(3, 0), (-3, 0), (0, 3), (0, -3), (2, 0), (-2, 0), (0, 2), (0, -2), (1, 0), (-1, 0), (0, 1), (0, -1)] {
            if matches!(
                crate::geography::civil_at(body, tx + dx, tz + dz),
                Some(crate::geography::Civil::Road | crate::geography::Civil::Bridge)
            ) {
                return None;
            }
        }
        // **Y NI SOBRE LA VEREDA** (30 sep 2026). La copa se dibuja con el pie en
        // la celda del tronco y sube hacia el norte —hasta 8 celdas un pino—, así
        // que un árbol en el pasto del parque, pegado a la acera, le tendía la
        // copa por encima de la vereda y de la calle de atrás. Se mira la columna
        // del tronco hacia el norte (donde está la copa) y un costado de una
        // celda a cada lado: ni acera ni calzada bajo la copa.
        for dz in -8..=1 {
            for dx in -2..=2 {
                if dx == 0 && dz == 0 {
                    continue;
                }
                if crate::geography::civil_at(body, tx + dx, tz + dz).is_some() {
                    return None;
                }
            }
        }
    }
    Some(kind)
}

/// Techo de densidad de árboles, POR MIL de celdas, en el bioma que más admite
/// (`vegetation_density == 1.0`, o sea el bosque).
///
/// **Sale de la geometría, no del gusto.** La copa media de `TREE_SPECIES` mide
/// ~34 m² y la celda mide 1 m²: con el 14 % de antes, cada metro cuadrado de
/// bosque estaba cubierto por CINCO copas superpuestas. Eso no es un bosque, es
/// una plancha verde — de ahí "los árboles son muy invasivos", y de ahí también
/// que taparan las casas que tenían al lado. A 30 ‰ la cobertura queda en ~1.0:
/// las copas se tocan (sigue leyéndose como bosque desde el aire) pero se ve el
/// suelo, se camina entre los troncos y lo que hay detrás se sigue viendo.
/// Barato de paso: son ~5× menos entidades de decoración por chunk.
const TREE_PERMILLE_MAX: f32 = 30.0;

/// ¿Le toca a esta celda quedarse SIN decoración natural por estar en un pueblo?
///
/// Una parcela es de su casa: el jardín, el patio, la entrada y la vereda son
/// parte del programa del lote aunque no pinten tile. Plantar ahí un árbol de 6 m
/// —que se dibuja con el pie en su celda y la copa hacia el norte— le comía la
/// fachada a la casa. La única excepción es la arboleda: su programa a propósito
/// no pinta ningún tile porque los árboles los siembra esta misma función.
///
/// En lo que queda del casco (manzanas vacías, descampados) crece un cuarto de lo
/// normal: un pueblo tiene árboles, pero no es un claro dentro del bosque.
fn urban_decor_blocked(body: CelestialBody, tx: i64, tz: i64) -> bool {
    let Some(s) = crate::geography::settlement_covering(body, tx, tz) else {
        return false;
    };
    match crate::city::lot_at(body, &s, tx, tz) {
        Some(lot) => lot.program().key != "lot/arboleda",
        // Hash propio: reusar los bits del sorteo de arriba correlaciona las dos
        // decisiones y deja franjas enteras del pueblo peladas o llenas.
        None => hash_cell(0x5aab_0110, tx, tz) % 4 != 0,
    }
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
#[derive(Debug)]
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
    ExtraDecor { key: "nature/hongo", size: [0.7, 0.7], weight: 4 },
    ExtraDecor { key: "nature/cactus", size: [1.0, 1.4], weight: 3 },
    // El cuervo del pack ESTÁ ANIMADO (6 cuadros de aleteo) y no se dibujaba en
    // ninguna parte: su clave había quedado sin slot. Peso bajo — un cuervo cada
    // tanto es un campo con bichos, uno cada diez metros es una plaga.
    ExtraDecor { key: "npc/fauna_crow", size: [0.8, 1.5], weight: 2 },
];

pub const PROP_SPECIES: &[ExtraDecor] = &[
    ExtraDecor { key: "prop/banco", size: [1.6, 1.6], weight: 10 },
    ExtraDecor { key: "prop/farol", size: [1.1, 2.2], weight: 10 },
    ExtraDecor { key: "prop/cartel", size: [0.7, 1.4], weight: 8 },
    ExtraDecor { key: "prop/barril", size: [1.1, 1.1], weight: 8 },
    ExtraDecor { key: "prop/caja", size: [1.0, 1.0], weight: 8 },
    ExtraDecor { key: "prop/pozo", size: [1.6, 1.8], weight: 3 },
];

/// Flora extra en celdas de pasto/tierra (Tierra): ~4% de celdas candidatas.
///
/// Reparte entre tres tablas según DÓNDE esté la celda, que es lo que hace que
/// el campo, el bosque y el jardín de un pueblo no se vean iguales:
/// [`NATURE_SPECIES`] es la flora curada de siempre, y las otras dos salen del
/// pack (`pack_siembra`), que trae 570 sprites de jardín y 693 de bosque que
/// hasta ahora no se dibujaban en ninguna parte.
pub fn nature_decor_at(body: CelestialBody, tx: i64, tz: i64) -> Option<&'static ExtraDecor> {
    if body != CelestialBody::Earth {
        return None;
    }
    // **LO QUE CRECE SOLO NO CRECE EN EL ASFALTO.** Esta tabla no miraba el suelo
    // urbanizado, así que la flora de jardín del pack —girasoles, macetas,
    // arbustos podados— salía sorteada encima de la calzada y de la acera, en
    // mitad del cruce. Lo urbano tiene sus propias tablas y van antes que ésta en
    // la cadena (`vereda_decor_at`, `calzada_decor_at`); acá sólo hace falta no
    // pisarlas.
    if crate::geography::civil_at(body, tx, tz).is_some() {
        return None;
    }
    // **Y NO CRECE EN LA NIEVE.**
    //
    // Esta tabla sorteaba el 4 % de TODAS las celdas de la Tierra sin mirar el
    // bioma —sólo esquivaba el asfalto—, así que el arbusto de jardín, el
    // girasol y la mata de bosque salían igual en la pradera que en la cumbre.
    // No se notaba mientras la nieve no existía casi; desde que la cumbre se
    // decide por escalón (`geography::NIVEL_NIEVE`, 14 ago 2026) se ve en la
    // primera captura: `relieve-3-cumbre.png` era un pico nevado FLORECIDO.
    //
    // La ROCA no entra en este veto y está bien que no entre: la pone
    // `surface_decor_at` con su propia densidad, y un pico con piedra asomando
    // entre la nieve es exactamente lo que tiene que verse. Lo que no puede
    // haber ahí es hoja verde.
    if matches!(biome_at(body, tx, tz), Biome::Snow | Biome::Ice) {
        return None;
    }
    // **NADA SE SIEMBRA PEGADO AL FILO.** El canto ya es `Tile::Cliff` y no
    // recibe nada, pero la celda de ADENTRO era terreno llano y edificable, así
    // que el sorteo le plantaba el árbol ahí y medio tronco quedaba colgando
    // sobre la pared. El retiro es de la meseta hacia adentro
    // (`relief::retiro_del_canto`), no del canto hacia afuera.
    if crate::relief::retiro_del_canto(body, tx, tz) {
        return None;
    }
    let h = hash_cell(0xf10a_0001, tx, tz);
    if h % 1000 >= 40 {
        return None;
    }
    let roll = h >> 10;
    // **"Cerca de gente" es DENTRO del pueblo, no en su provincia.**
    //
    // Esto preguntaba si había un asentamiento en la celda de `SITE_PERIOD`, que
    // mide **2.600 tiles de lado**: 6,7 km² de campo abierto declarados jardín
    // porque a cuatro kilómetros hay un pueblo. Por eso el descampado salía
    // sembrado de regaderas, macetas, rastrillos y bancos de parque sin una casa
    // a la vista — el sitio no era el jardín de nadie, sólo estaba en el mismo
    // cuadrante. `settlement_covering` sí mide el casco (su radio), así que el
    // jardín termina donde termina el pueblo y afuera vuelve a haber monte.
    let cerca_de_gente = crate::geography::settlement_covering(body, tx, tz);
    let tabla = match (&cerca_de_gente, roll % 3) {
        // Alrededor de un asentamiento la flora es de JARDÍN: lo plantado se
        // distingue de lo que creció solo, y esa diferencia es la que dice "acá
        // vive alguien" antes de ver la primera casa.
        (Some(s), 0 | 1) => return jardin_del_pueblo(body, s, tx, tz, roll),
        (None, 0) => crate::pack_siembra::BOSQUE,
        _ => NATURE_SPECIES,
    };
    pick_weighted(tabla, roll)
}

/// Los objetos de la tabla `PARQUE` que son **equipamiento**: se colocan en un
/// sitio pensado para ellos, no se siembran a los dados.
///
/// La fuente, la estatua de seto, el arco de seto y los bancos son un OBJETO con
/// lugar (la plaza), no flora. Sorteados por celda salían plantados en cualquier
/// pradera del casco —"¿qué hace un arco de seto ahí?"— y la fuente, que es el
/// más pesado de la tabla, se repetía cada pocos pasos.
fn es_equipamiento_de_parque(key: &str) -> bool {
    ["fuente", "pasto_arco", "pasto_estatua", "banco", "flores_banco"].iter().any(|m| key.contains(m))
}

/// Un florero o una maceta es un objeto de interior/terraza: sobre el pasto
/// pelado de un descampado no tiene qué hacer.
fn es_maceta(key: &str) -> bool {
    key.contains("florero") || key.contains("maceta")
}

/// Qué crece en el jardín de un pueblo: flores y matas sueltas en cualquier
/// parte; el equipamiento (fuente, bancos) sólo dentro de una plaza, y nunca el
/// arco ni la estatua de seto, que son piezas de un jardín compuesto a mano.
fn jardin_del_pueblo(
    body: CelestialBody,
    s: &crate::geography::Settlement,
    tx: i64,
    tz: i64,
    roll: u32,
) -> Option<&'static ExtraDecor> {
    let en_plaza = crate::city::lot_at(body, s, tx, tz)
        .is_some_and(|l| l.program().key == "lot/plaza");
    for i in 0..12u32 {
        let r = roll.wrapping_add(i.wrapping_mul(0x9e37_79b1));
        let e = pick_weighted(crate::pack_siembra::PARQUE, r)?;
        if es_equipamiento_de_parque(e.key) {
            if en_plaza && !e.key.contains("pasto_") {
                return Some(e);
            }
            continue;
        }
        if es_maceta(e.key) {
            continue;
        }
        return Some(e);
    }
    None
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
    // El patio del pueblo es el suelo NO urbanizado que rodea al solar. Sobre la
    // calzada y la acera manda lo urbano — sin esta guarda la reposera, la
    // parrilla y la escalera de pileta se sorteaban en medio de la calle, que es
    // exactamente cómo se veía.
    if crate::geography::civil_at(body, tx, tz).is_some() {
        return None;
    }
    // **NADA SE SIEMBRA PEGADO AL FILO.** El canto ya es `Tile::Cliff` y no
    // recibe nada, pero la celda de ADENTRO era terreno llano y edificable, así
    // que el sorteo le plantaba el árbol ahí y medio tronco quedaba colgando
    // sobre la pared. El retiro es de la meseta hacia adentro
    // (`relief::retiro_del_canto`), no del canto hacia afuera.
    if crate::relief::retiro_del_canto(body, tx, tz) {
        return None;
    }
    let h = hash_cell(0x9209_0002, tx, tz);
    if h % 1000 >= 30 {
        return None;
    }
    // El patio del pueblo mezcla los seis props curados con los del pack: sin
    // esto, todo asentamiento del mundo tenía el mismo banco y el mismo barril.
    let roll = h >> 10;
    let tabla = if roll % 4 == 0 { PROP_SPECIES } else { crate::pack_siembra::PATIO };
    pick_weighted(tabla, roll)
}

// ---------------------------------------------------------------------------
// El agua no está vacía: flota, se pesca y se acampa en la orilla
// ---------------------------------------------------------------------------

// **La tabla única de vida del agua se fue** (`WATER_DECOR` + `water_decor_at`).
// Era una sola bolsa de pesos para TODO el líquido del planeta: el mismo dado
// decidía si en esta celda había un nenúfar, un salvavidas o un cardumen, sin
// mirar si el agua estaba quieta, si corría o si tocaba la orilla — por eso el
// flotador de pileta aparecía en un charco de la sierra. La reemplazaron las
// cuatro tablas de `crate::agua` (RIBERA / REMANSO / CORRIENTE / ABIERTO), que
// eligen por SITIO; sembrar es `agua::decor_at`. Quedó marcada `#[deprecated]` y
// llamándose a sí misma desde `water_decor_at`, o sea viva para el compilador y
// muerta para el juego.

/// Lo que hay en la ORILLA de una laguna: el campamento y el puesto de pesca.
/// Los `Fishing_Spot_1..4` del pack son cuatro variantes del MISMO objeto, así
/// que van por una sola clave (`fishing/spot`) y el juego elige la variante por
/// celda — ver `ArtLibrary::variant_key_seeded`.
pub const SHORE_DECOR: &[ExtraDecor] = &[
    ExtraDecor { key: "fishing/spot", size: [1.6, 1.4], weight: 30 },
    ExtraDecor { key: "fishing/caña", size: [1.0, 1.8], weight: 10 },
    ExtraDecor { key: "fishing/balde", size: [0.9, 0.9], weight: 8 },
    ExtraDecor { key: "prop/carpa", size: [2.6, 2.6], weight: 12 },
    ExtraDecor { key: "prop/barril", size: [1.1, 1.1], weight: 6 },
    ExtraDecor { key: "nature/tronco", size: [1.5, 0.8], weight: 14 },
    ExtraDecor { key: "prop/mesa_picnic", size: [2.0, 1.3], weight: 8 },
    ExtraDecor { key: "agua/muelle", size: [2.0, 3.0], weight: 6 },
];

/// **El semáforo de una esquina, uno por RUMBO.**
///
/// Indexado por `rumbo / 2` con los octantes de `roads::semaforo_at`: `0` = mira
/// al este, `1` = al norte, `2` = al oeste, `3` = al sur. **No es una tabla de
/// sorteo** y por eso no lleva pesos que signifiquen algo: cuál va en cada
/// esquina lo decide la geometría del cruce, no un hash.
///
/// **Las cuatro caras salen de `scripts/import-semaforos.py`, y por qué.**
///
/// El semáforo del pack no es un sprite: es un KIT. Las cuatro caras del poste
/// (`Off_Frontal_2`, `Back_2`, `Left_2`, `Right_2`) van solas, pero la frontal
/// viene APAGADA; lo que la prende son las piezas `*_Mod_*` —`Mod` de MODULAR—,
/// que son calcomanías de las tres lámparas para pegar encima del poste.
///
/// Antes esta tabla apuntaba directo a los recortes crudos del pack y de ahí
/// salían los tres males que se vieron en la esquina: `de_colores_2` es una de
/// esas calcomanías, así que era una caja de luces flotando sin poste; `atras_1`
/// es la versión CORTA (1×4) y desentonaba con las otras; y quedaba una sola
/// esquina prendida de las cuatro. (Más atrás todavía se sorteaba entre `_6` y
/// `_16`, que son pórticos de 4×6 tiles aplastados en una caja de 1,5×3 m.)
///
/// Ahora el importador compone poste + calcomanía y deja las cuatro caras del
/// MISMO poste alto, todas de 1×5 tiles — de ahí que las cuatro midan igual:
/// 3 m de alto y 0,6 de ancho es exactamente la proporción 1:5 del recorte.
pub const SEMAFOROS: &[ExtraDecor] = &[
    // Lámparas a la derecha del poste: se lee desde el este.
    ExtraDecor { key: "road/semaforo_derecha", size: [0.6, 3.0], weight: 1 },
    // La espalda de la caja: el que la ve viene del norte.
    ExtraDecor { key: "road/semaforo_atras", size: [0.6, 3.0], weight: 1 },
    // Lámparas a la izquierda: se lee desde el oeste.
    ExtraDecor { key: "road/semaforo_izquierda", size: [0.6, 3.0], weight: 1 },
    // De frente, las tres luces PRENDIDAS: es la cara que mira al sur, y la que
    // más se ve en 3/4 — por eso es la del brazo que sube desde abajo.
    ExtraDecor { key: "road/semaforo_frente", size: [0.6, 3.0], weight: 1 },
];

/// **El semáforo va en la ESQUINA, no donde caiga.**
///
/// El resto del mobiliario urbano se siembra por hash sobre la vereda, que para
/// un banco o un tacho está bien: van "por ahí". Un semáforo no — un semáforo en
/// mitad de la cuadra no es decoración, es un error que se ve de lejos. Éste sale
/// de la GEOMETRÍA: una celda de vereda que tiene calzada en dos lados
/// perpendiculares es, por definición, la esquina de un cruce.
///
/// **Y MIRA a donde tiene que mirar.** Cuál de las cuatro caras le toca sale del
/// cruce (`roads::semaforo_at`), no de un hash: un semáforo que mira a la vereda
/// mientras el tránsito le pasa por detrás es peor que no tener semáforo. Ahí
/// están la tabla de esquinas y el reparto de un poste por brazo.
pub fn esquina_decor_at(body: CelestialBody, tx: i64, tz: i64) -> Option<&'static ExtraDecor> {
    let rumbo = crate::roads::semaforo_at(body, tx, tz)?;
    SEMAFOROS.get((rumbo / 2) as usize)
}

/// **El alumbrado público: el farol va en el CORDÓN y con ritmo, no por sorteo.**
///
/// El farol estaba en [`PROP_SPECIES`] con peso 10 sobre un 3 % de celdas de
/// vereda, o sea que salía uno cada varias cuadras y en cualquier parte de la
/// acera. De noche eso no es alumbrado: es un charco de luz suelto en el medio
/// de la nada, con el resto de la calle a oscuras. Y una calle a oscuras con un
/// solo farol se lee peor que una calle sin faroles, porque el ojo compara.
///
/// Un alumbrado de verdad tiene dos propiedades, y las dos son GEOMETRÍA, no
/// azar — el mismo criterio que el semáforo de [`esquina_decor_at`]:
///
/// 1. **Va contra el cordón**, o sea en la celda de vereda que toca la calzada.
///    Un farol en el fondo de la acera alumbra la pared, no la calle.
/// 2. **Va con ritmo**, cada [`PASO_ALUMBRADO`] tiles, y el ritmo corre A LO
///    LARGO de la vereda: si la calzada está al este o al oeste, la acera va
///    norte-sur y el compás lo lleva `tz`; si está al norte o al sur, lo lleva
///    `tx`. Sin eso el paso se mide en diagonal y la hilera sale despareja.
///
/// El resultado es una hilera de charcos parejos a lo largo de cada cuadra, que
/// es lo que hace que una ciudad de noche se lea como una ciudad. Cuesta una
/// entidad cada ocho tiles de cordón — menos que el 3 % de siembra al azar que
/// ya había, porque el cordón es una fracción chica de la vereda.
///
/// Va ANTES que [`vereda_decor_at`] en la cadena (`world2d`) por lo mismo que el
/// semáforo: tiene sitio fijo y no puede perderlo contra un tacho sorteado.
pub fn alumbrado_at(body: CelestialBody, tx: i64, tz: i64) -> Option<&'static ExtraDecor> {
    use crate::geography::Civil;
    if crate::geography::civil_at(body, tx, tz) != Some(Civil::Pavement) {
        return None;
    }
    if building_cell(body, tx, tz).is_some() {
        return None;
    }
    let calzada = |dx: i64, dz: i64| {
        crate::geography::civil_at(body, tx + dx, tz + dz) == Some(Civil::Road)
    };
    // Cordón vertical (calzada al este u oeste) → la hilera corre en z.
    // Cordón horizontal (calzada al norte o al sur) → corre en x.
    let paso = if calzada(1, 0) || calzada(-1, 0) {
        tz
    } else if calzada(0, 1) || calzada(0, -1) {
        tx
    } else {
        return None; // vereda de fondo: no es cordón
    };
    if paso.rem_euclid(PASO_ALUMBRADO) != 0 {
        return None;
    }
    Some(&FAROL_DE_CALLE)
}

/// **Las GUIRNALDAS de la plaza.**
///
/// Es el elemento que más se repite en las capturas de referencia y el que le da
/// escala a la noche: las hileras de puntos cálidos cruzando por encima son la
/// única luz que está EN EL AIRE. Un pueblo alumbrado sólo desde el piso se ve
/// plano por más faroles que tenga.
///
/// Van sobre la acera y **dentro del casco** —la regla de "cerca de gente" del
/// proyecto— en una retícula de [`PASO_GUIRNALDA_X`] × [`PASO_GUIRNALDA_Z`]. La
/// retícula existe por dos motivos, y ninguno es estético: el paso en x es más
/// ancho que la guirnalda más larga (6 celdas), así que **dos nunca se pisan**; y
/// al ser determinista, la guirnalda no parpadea al re-streamear el chunk.
///
/// No compite con el alumbrado: el farol vive en el CORDÓN (la acera que toca la
/// calzada) y esto se planta en la acera de adentro, que es donde de verdad se
/// cuelga algo entre dos postes.
pub fn guirnalda_at(body: CelestialBody, tx: i64, tz: i64) -> Option<&'static ExtraDecor> {
    use crate::geography::Civil;
    if crate::geography::civil_at(body, tx, tz) != Some(Civil::Pavement) {
        return None;
    }
    if building_cell(body, tx, tz).is_some() {
        return None;
    }
    // El cordón es del farol. Una guirnalda ahí quedaría colgada sobre la calle.
    let cordon = [(1, 0), (-1, 0), (0, 1), (0, -1)]
        .into_iter()
        .any(|(dx, dz)| crate::geography::civil_at(body, tx + dx, tz + dz) == Some(Civil::Road));
    if cordon {
        return None;
    }
    if crate::geography::settlement_covering(body, tx, tz).is_none() {
        return None;
    }
    if tx.rem_euclid(PASO_GUIRNALDA_X) != 0 || tz.rem_euclid(PASO_GUIRNALDA_Z) != 0 {
        return None;
    }
    // Cuál de las cuatro sale de la celda, no de un contador: así el mismo sitio
    // tiene siempre la misma guirnalda y la plaza no se ve como una copia.
    let h = hash_cell(0x6c11_0004, tx, tz);
    GUIRNALDAS.get((h >> 9) as usize % GUIRNALDAS.len())
}

/// Paso de la retícula de guirnaldas, en tiles. **El de x tiene que ser mayor
/// que la guirnalda más larga** (6 celdas) o dos vecinas se solaparían.
const PASO_GUIRNALDA_X: i64 = 9;
/// …y el de z decide cuántas hileras cruzan la plaza. Siete deja una cada dos
/// pasos y medio de caminata: se ven varias a la vez sin que tapen el cielo.
const PASO_GUIRNALDA_Z: i64 = 7;

/// Las cuatro guirnaldas dibujadas (`scripts/dibujar-guirnaldas.py`). El `size`
/// es el vano real de cada una — 4, 5, 3 y 6 celdas de ancho por 2 de alto — en
/// la misma escala de 0,95 m por celda que usa el resto de las tablas.
const GUIRNALDAS: &[ExtraDecor] = &[
    ExtraDecor { key: "prop/luces_guirnalda_ambar", size: [3.80, 1.90], weight: 1 },
    ExtraDecor { key: "prop/luces_guirnalda_papel", size: [4.75, 1.90], weight: 1 },
    ExtraDecor { key: "prop/luces_guirnalda_rojo", size: [2.85, 1.90], weight: 1 },
    ExtraDecor { key: "prop/luces_guirnalda_verde", size: [5.70, 1.90], weight: 1 },
];

/// Cada cuántos tiles hay un farol a lo largo del cordón.
///
/// **Catorce, y el número salió de una captura.** Con ocho, la primera versión
/// dejaba diez pozos superpuestos sobre cada celda de vereda y la calle de
/// medianoche se veía como un mediodía nublado (`HA_NIGHTTEST` night-1-block):
/// el charco de un farol sólo se lee como charco si a los costados hay
/// oscuridad. Con el alcance medido del farol (4,5 × `LIGHT_REACH` 1,5 ≈ 6,8 m
/// de pozo) y el paso en catorce, dos pozos vecinos se rozan por el borde
/// —donde la caída ya vale casi cero— y entre medio queda calle a oscuras. Eso
/// es una hilera de faroles; lo otro era subir el brillo.
const PASO_ALUMBRADO: i64 = 14;

/// El farol del alumbrado, y la elección tiene dos descartes detrás:
///
/// - **No `anim/street_lamp`**, aunque esté animado y sea el más vistoso:
///   LimeZu le dibujó el charco de luz ADENTRO del sprite, y un charco pintado
///   se ve igual a las tres de la tarde. El pozo lo pone el motor
///   (`ha-game::light2d`), que es el que sabe qué hora es.
/// - **No `prop/farol`**, que era el candidato obvio por nombre: medido, está
///   dibujado APAGADO (0,50 de luminancia máxima — el poste gris con el vidrio
///   sin encender), así que `scripts/import-emisiones.py` no le declara emisión.
///   Un farol que no ilumina no puede ser el alumbrado público.
///
/// `prop/farol_de_calle_5` sí lo dibujó el pack ENCENDIDO: 36 píxeles de vidrio
/// blanco medidos, un alcance de 4,5 m y la silueta de farol clásico de una
/// celda de ancho por cuatro de alto, que es la proporción de un poste de calle.
const FAROL_DE_CALLE: ExtraDecor =
    ExtraDecor { key: "prop/farol_de_calle_5", size: [0.95, 3.80], weight: 1 };

/// **La tabla de muebles que le toca a un edificio, por su programa de lote.**
///
/// Es la regla que hace que las cosas peguen entre sí: el tema del lote decide la
/// fachada (`import-edificios.py`) Y los muebles. Una casa no puede recibir una
/// camilla de hospital porque su tabla no la tiene.
///
/// Las casas alternan entre cocina, dormitorio, baño y living **por celda**, así
/// que una casa no es un solo ambiente repetido: tiene cuartos distintos.
fn tabla_interior(lote: &str, roll: u32) -> Option<&'static [ExtraDecor]> {
    use crate::interior_siembra as m;
    let casa = [m::LIVING, m::COCINA, m::DORMITORIO, m::BANO];
    Some(match lote {
        "building/casa_chica" | "building/casa_mediana" | "building/casa_grande"
        | "building/casa_country" | "building/villa" | "building/duplex" => {
            casa[(roll % 4) as usize]
        }
        "building/tienda" | "building/mercado" | "building/shopping" => m::COCINA,
        "building/oficina" | "building/torre" | "building/correo" => m::ESTUDIO,
        "building/escuela" | "building/universidad" => m::AULA,
        "building/hospital" | "building/hotel" => m::HOSPITAL,
        "building/comisaria" => m::CARCEL,
        "building/deposito" | "building/nave_industrial" | "building/granero"
        | "building/base_militar" | "building/bomberos" => m::SOTANO,
        "building/estacion_tren" => m::SALA,
        _ => return None,
    })
}

/// **Lo que hay ADENTRO de un edificio generado.**
///
/// Entrar a una casa y encontrarla vacía es peor que no poder entrar: el techo se
/// transparenta, ves el piso y las paredes, y nada más. Esto la amuebla con el
/// tema de su lote.
///
/// Dos reglas de colocación, y las dos salen de mirar cómo se amuebla de verdad
/// (y de las capturas del pack): el mueble va **contra una pared** —el centro del
/// cuarto queda libre para caminar— y **nunca en la celda de la puerta ni en su
/// paso**, que si no te quedás afuera de tu propia casa.
pub fn interior_decor_at(body: CelestialBody, tx: i64, tz: i64) -> Option<&'static ExtraDecor> {
    // **El reparto lo decide `amueblado`**, cuarto por cuarto y con paso
    // garantizado (29 sep 2026). Acá sólo se contesta si en esta celda está
    // ANCLADO un mueble; el resto de sus celdas no siembra nada.
    crate::amueblado::mueble_anclado(body, tx, tz).map(|m| m.decor)
}

/// La tabla de muebles de un edificio que NO es casa (la casa reparte sus
/// ambientes por cuarto en `amueblado`).
pub fn tabla_interior_de(lote: &str) -> Option<&'static [ExtraDecor]> {
    tabla_interior(lote, 0)
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
    // La orilla de una laguna es el campamento: el puesto de pesca curado más
    // los 693 sprites de camping del pack, que traen la fogata, la mesa, la
    // conservadora y la ropa tendida.
    let roll = h >> 10;
    let tabla = if roll % 3 == 0 { SHORE_DECOR } else { crate::pack_siembra::CAMPAMENTO };
    pick_weighted(tabla, roll)
}

// ---------------------------------------------------------------------------
// El pack, en el suelo: lo que hay en la vereda, el parque, la playa y la obra
// ---------------------------------------------------------------------------

/// **Lo que hay tirado en la VEREDA de una ciudad.**
///
/// Es el agujero que hacía que las ciudades del juego se leyeran como maquetas:
/// la trama urbana pintaba calzada y acera y ahí se acababa. Ni un banco, ni un
/// farol, ni un tacho — kilómetros de gris. Los props existían en el pack desde
/// el primer día (716 sprites sólo en `3_City_Props`) y ninguno estaba sembrado.
///
/// Va sobre `Pavement` y **nunca sobre la calzada**: un banco en mitad de la
/// avenida no es decoración, es un obstáculo que nadie puso ahí.
pub fn vereda_decor_at(body: CelestialBody, tx: i64, tz: i64) -> Option<&'static ExtraDecor> {
    if crate::geography::civil_at(body, tx, tz) != Some(crate::geography::Civil::Pavement) {
        return None;
    }
    if building_cell(body, tx, tz).is_some() {
        return None;
    }
    let h = hash_cell(0x5ede_0001, tx, tz);
    // 3 %: una vereda con algo cada seis o siete pasos. Empezó en 6 % y se bajó
    // midiendo en el Redmi: cada prop es una entidad con su sprite, y la cuenta
    // de entidades es lo que la CPU paga por frame aunque no se mueva ninguna.
    // Con el 6 % la aldea llegaba a 5.500 entidades; el ojo no nota la mitad y
    // los frames sí.
    if h % 1000 >= 30 {
        return None;
    }
    pick_weighted(crate::pack_siembra::VEREDA, h >> 10)
}

/// **Lo que hay TIRADO en el asfalto**: tapas de alcantarilla, rejillas, latas,
/// cartones, marcas de frenada.
///
/// La tabla `CALLE` existía desde el import masivo —se generó con la regla del
/// ALTO justamente para que sólo entrara lo que no se levanta del suelo— y no la
/// consumía nadie: la calzada quedaba pelada mientras la basura de la vereda se
/// sorteaba encima de ella por la puerta de atrás (`prop_decor_at`, que no
/// miraba el suelo urbano). Ahora la calzada tiene su propia siembra y las otras
/// tienen prohibido pisarla.
///
/// Densidad baja a propósito: la calle es para circular. Una tapa de alcantarilla
/// cada tanto la vuelve un sitio; una lata cada dos pasos la vuelve un basural, y
/// además es una entidad por celda que el Redmi paga por frame.
pub fn calzada_decor_at(body: CelestialBody, tx: i64, tz: i64) -> Option<&'static ExtraDecor> {
    if crate::geography::civil_at(body, tx, tz) != Some(crate::geography::Civil::Road) {
        return None;
    }
    let h = hash_cell(0xca11_0003, tx, tz);
    if h % 1000 >= 8 {
        return None;
    }
    // Alcantarillas y rejillas son de la CALLE del pueblo, no de la ruta de campo.
    // Va después del sorteo: sólo lo paga la celda que ya sacó algo.
    if crate::geography::settlement_covering(body, tx, tz).is_none() {
        return None;
    }
    pick_weighted(crate::pack_siembra::CALLE, h >> 10)
}

/// Lo que hay en la ARENA de la costa: sombrillas, reposeras, castillos, conchas.
///
/// La playa era arena pelada aunque el pack trae 317 sprites de playa. Se siembra
/// sobre arena con agua cerca — o sea la orilla del mar, no un desierto interior.
pub fn playa_decor_at(body: CelestialBody, tx: i64, tz: i64) -> Option<&'static ExtraDecor> {
    if topdown_tile(body, 0, tx, tz) != crate::topdown::Tile::Sand {
        return None;
    }
    let cerca_del_agua = [(4, 0), (-4, 0), (0, 4), (0, -4)]
        .into_iter()
        .any(|(dx, dz)| crate::relief::is_liquid(body, tx + dx, tz + dz));
    if !cerca_del_agua || crate::geography::civil_at(body, tx, tz).is_some() {
        return None;
    }
    let h = hash_cell(0x91a4_0002, tx, tz);
    // 2 %: la playa es grande y una sombrilla cada siete pasos ya la puebla.
    if h % 1000 >= 20 {
        return None;
    }
    pick_weighted(crate::pack_siembra::PLAYA, h >> 10)
}

pub(crate) fn pick_weighted(table: &'static [ExtraDecor], roll: u32) -> Option<&'static ExtraDecor> {
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

/// **Todas las tablas que el mundo SIEMBRA a ciegas**, para poder auditarlas de
/// una vez. Lo que entra acá se sortea por hash y aterriza donde caiga, así que
/// todo lo que hay adentro tiene que ser un objeto ENTERO — ver el test
/// `ninguna_tabla_siembra_piezas_de_kit`.
///
/// `SEMAFOROS` a propósito NO está: no se sortea, se reparte por la geometría del
/// cruce, y sus cuatro caras son piezas del mismo poste compuestas a mano.
pub const TABLAS_SEMBRADAS: &[(&str, &[ExtraDecor])] = &[
    ("NATURE_SPECIES", NATURE_SPECIES),
    ("PROP_SPECIES", PROP_SPECIES),
    // WATER_DECOR salió: la reemplazan RIBERA/REMANSO/CORRIENTE/ABIERTO de
    // `crate::agua`, que se auditan igual acá abajo.
    ("AGUA_RIBERA", crate::agua::RIBERA),
    ("AGUA_REMANSO", crate::agua::REMANSO),
    ("AGUA_CORRIENTE", crate::agua::CORRIENTE),
    ("AGUA_ABIERTO", crate::agua::ABIERTO),
    ("SHORE_DECOR", SHORE_DECOR),
    ("VEREDA", crate::pack_siembra::VEREDA),
    ("CALLE", crate::pack_siembra::CALLE),
    ("PARQUE", crate::pack_siembra::PARQUE),
    ("PLAYA", crate::pack_siembra::PLAYA),
    ("CEMENTERIO", crate::pack_siembra::CEMENTERIO),
    ("BOSQUE", crate::pack_siembra::BOSQUE),
    ("CAMPAMENTO", crate::pack_siembra::CAMPAMENTO),
    ("OBRA", crate::pack_siembra::OBRA),
    ("PATIO", crate::pack_siembra::PATIO),
];

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geography::{settlement_in, SettlementTier};

    /// **EN LA NIEVE NO CRECE NADA VERDE.**
    ///
    /// `nature_decor_at` sorteaba el 4 % de todas las celdas de la Tierra sin
    /// mirar el bioma —sólo esquivaba el asfalto—, así que el arbusto de jardín y
    /// la mata de bosque salían igual en la pradera que en la cumbre. Se vio en
    /// la primera captura del driver de relieve el 14 ago 2026: un pico nevado
    /// florecido. La roca sí puede asomar; la hoja verde no.
    #[test]
    fn en_la_nieve_no_crece_nada() {
        let b = CelestialBody::Earth;
        // Alrededor de las cumbres declaradas, que es donde hay nieve de verdad.
        let mut nieve = 0u32;
        for (_, (cx, cz)) in crate::world_map::cumbres(b) {
            for dz in -160..160 {
                for dx in -160..160 {
                    let (tx, tz) = (cx + dx, cz + dz);
                    if biome_at(b, tx, tz) != Biome::Snow {
                        continue;
                    }
                    nieve += 1;
                    assert!(
                        nature_decor_at(b, tx, tz).is_none(),
                        "flora sembrada sobre la nieve en ({tx},{tz})"
                    );
                }
            }
        }
        assert!(nieve > 500, "sólo {nieve} celdas de nieve: la regla no probó nada");
    }

    /// **Una PIEZA no se siembra: se coloca.**
    ///
    /// Medio pack es modular —el semáforo es poste + calcomanía de lámparas, la
    /// cerca son doce losas de autotile, el galpón es cuerpo + puerta + techo, el
    /// seto son tramos de pared—, y una pieza suelta sorteada sobre el pasto no
    /// decora: deja media cosa tirada. Es lo que se veía y lo que motivó la regla:
    /// postes sin lámpara en las cuatro esquinas de un cruce, un tramo de cerca
    /// solo en mitad de un patio, arcos de seto plantados a campo abierto.
    ///
    /// El filtro de verdad está en el importador
    /// (`scripts/import-pack-masivo.py::MARCAS_MODULARES`), que es quien ve el
    /// nombre de archivo del pack. Esto es el alambre de púa del lado del juego:
    /// caza que alguien edite una tabla a mano o que el filtro del importador se
    /// afloje. Las marcas de acá son las del nombre YA traducido al castellano.
    ///
    /// Si algo cae acá y de verdad va solo, no se agrega una excepción: se le
    /// cambia el nombre en el importador. Un objeto entero que se llama
    /// "esquina_abajo_izquierda" va a volver a confundir al que lo lea.
    #[test]
    fn ninguna_tabla_siembra_piezas_de_kit() {
        const MARCAS: &[&str] = &[
            "modular",
            "_pared_",
            "_diagonal_",
            "esquina_interior",
            "abajo_izquierda",
            "abajo_medio",
            "abajo_derecha",
            "arriba_izquierda",
            "arriba_medio",
            "arriba_derecha",
            "medio_izquierda",
            "medio_derecha",
            "semaforo",
            // **El cerco de piedra del pack.** `Grass_Fenced_1` son 22 piezas de
            // un muro con pasto, y las seis que llegaron al juego como
            // `pasto_cercado*` son tramos y esquinas de ese muro. Tres estaban
            // sembradas en el BOSQUE: un pedazo de cerca tirado en mitad del
            // monte. Los props de esa familia son otros (`Props_Grass_Fenced`,
            // que entran como `terreno_y_cercas_*`) y no llevan esta marca.
            "pasto_cercado",
        ];
        // **Y las que el nombre en castellano NO delata.**
        //
        // Una marca sólo sirve si el nombre dice algo. `pasto_13` y `monticulo_4`
        // no dicen nada: suenan a prop y son `Grass_2_13` (una pieza del blob de
        // pasto sobre transparente) y `Mound_2_4` (un quinto de montículo, con el
        // corte vertical a la vista). Entraron las dieciocho al BOSQUE y no había
        // red que las cazara.
        //
        // Lo que sí es medible acá es la FORMA de la clave: en este pack, **toda**
        // `nature/pasto_<n>` sale de `Grass_2_<n>` y **toda** `nature/monticulo_<n>`
        // de `Mound_1`/`Mound_2` — comprobado clave por clave contra el pixel del
        // pack. Los props de esas mismas cosas tienen otro nombre y no terminan en
        // número pelado: `pasto_matas`, `monticulo_de_tierra`, `tierra_10`.
        const FAMILIAS_DE_BLOB: &[&str] = &["nature/pasto_", "nature/monticulo_"];
        let es_pieza_de_blob = |k: &str| {
            FAMILIAS_DE_BLOB.iter().any(|f| {
                k.strip_prefix(*f)
                    .is_some_and(|resto| !resto.is_empty() && resto.bytes().all(|b| b.is_ascii_digit()))
            })
        };
        let mut malas = Vec::new();
        for (nombre, tabla) in TABLAS_SEMBRADAS {
            for d in *tabla {
                if let Some(m) = MARCAS.iter().find(|m| d.key.contains(**m)) {
                    malas.push(format!("{nombre}: {} (marca «{m}»)", d.key));
                } else if es_pieza_de_blob(d.key) {
                    malas.push(format!("{nombre}: {} (pieza de blob de terreno)", d.key));
                }
            }
        }
        assert!(malas.is_empty(), "piezas de kit sembradas a ciegas:\n  {}", malas.join("\n  "));
    }

    /// **Lo que la revisión VISUAL descartó no se siembra.**
    ///
    /// El 29 sep 2026 se miró el recorte de cada clave del manifest
    /// (`scripts/revision-visual.json`), porque el nombre no alcanzaba: Modern
    /// Interiors viene numerado y el importador bautiza por el archivo. Así
    /// entraban espejos, termotanques y cuadros sembrados de pie en mitad del
    /// cuarto —"muebles montados sobre las paredes"—, frazadas sin cama y
    /// parlantes flotando en el agua. Los importadores ya filtran; esto caza que
    /// alguien edite una tabla a mano o que un importador deje de leer la revisión.
    ///
    /// Afuera no entra `X`, `M` ni `P`; adentro tampoco `S` (una alfombra plantada
    /// como mueble frena el paso).
    #[test]
    fn ninguna_tabla_siembra_lo_que_la_revision_visual_descarto() {
        let rev: serde_json::Value =
            serde_json::from_str(include_str!("../../../../scripts/revision-visual.json")).unwrap();
        let flags = |k: &str| rev["claves"][k]["flags"].as_str().unwrap_or("").to_string();
        let mut malas = Vec::new();
        for (nombre, tabla) in TABLAS_SEMBRADAS {
            for d in *tabla {
                let f = flags(d.key);
                if f.contains(['X', 'M', 'P']) {
                    malas.push(format!("{nombre}: {} [{f}]", d.key));
                }
            }
        }
        use crate::interior_siembra as i;
        let adentro: &[(&str, &[ExtraDecor])] = &[
            ("COCINA", i::COCINA),
            ("DORMITORIO", i::DORMITORIO),
            ("BANO", i::BANO),
            ("LIVING", i::LIVING),
            ("SALA", i::SALA),
            ("AULA", i::AULA),
            ("HOSPITAL", i::HOSPITAL),
            ("CARCEL", i::CARCEL),
            ("OFICINA", i::OFICINA),
            ("SOTANO", i::SOTANO),
            ("MUSEO", i::MUSEO),
            ("GIMNASIO", i::GIMNASIO),
            ("ESTUDIO", i::ESTUDIO),
        ];
        for (nombre, tabla) in adentro {
            for d in *tabla {
                let f = flags(d.key);
                if !f.is_empty() {
                    malas.push(format!("{nombre}: {} [{f}]", d.key));
                }
            }
        }
        assert!(malas.is_empty(), "sembrado lo que la revisión visual descartó:\n  {}", malas.join("\n  "));
    }

    /// Sembrar la misma clave dos veces en la misma tabla es peso duplicado sin
    /// querer: el objeto sale el doble de seguido que lo que dice su `weight`.
    #[test]
    fn ninguna_tabla_repite_una_clave() {
        for (nombre, tabla) in TABLAS_SEMBRADAS {
            let mut vistas = std::collections::BTreeSet::new();
            for d in *tabla {
                assert!(vistas.insert(d.key), "{nombre} repite {}", d.key);
            }
        }
    }

    /// **La regla de oro de la vegetación.** La densidad de árboles no se elige a
    /// ojo: se calcula contra la COBERTURA de copas (densidad × área media de
    /// copa). Con el 14 % de antes daba ~5.0 — cinco copas apiladas por metro
    /// cuadrado, o sea una plancha verde que tapaba las casas. Alrededor de 1.0
    /// las copas se tocan (sigue siendo bosque desde el aire) y se ve el suelo.
    #[test]
    fn el_bosque_no_es_una_plancha_verde() {
        let peso: f32 = TREE_SPECIES.iter().map(|s| s.weight as f32).sum();
        let area: f32 = TREE_SPECIES
            .iter()
            .map(|s| s.size[0] * s.size[1] * s.weight as f32)
            .sum::<f32>()
            / peso;
        // `vegetation_density(Forest) == 1.0`, así que el bosque usa el techo.
        let cobertura = (TREE_PERMILLE_MAX / 1000.0) * area;
        assert!(
            (0.6..=1.5).contains(&cobertura),
            "cobertura de copas {cobertura:.2} (densidad {TREE_PERMILLE_MAX} por mil x {area:.1} m2 de copa)"
        );
    }

    fn una_ciudad() -> crate::geography::Settlement {
        for sx in -30..30 {
            for sz in -30..30 {
                if let Some(s) = settlement_in(CelestialBody::Earth, sx, sz) {
                    if s.tier == SettlementTier::City {
                        return s;
                    }
                }
            }
        }
        panic!("ninguna ciudad en 60x60 celdas de asentamiento");
    }

    /// El reporte literal: "hasta las casas están con árboles". El jardín, el
    /// patio y la entrada son parte de la parcela aunque no pinten tile, y un
    /// árbol plantado ahí le come la fachada a la casa. Ninguna parcela genera
    /// árbol — salvo la arboleda, que existe justo para eso.
    #[test]
    fn ninguna_parcela_tiene_arboles_encima() {
        let s = una_ciudad();
        let body = CelestialBody::Earth;
        let mut en_parcela = 0;
        let mut arbolada = 0;
        for dz in -70..70 {
            for dx in -70..70 {
                let (x, z) = (s.center.0 + dx, s.center.1 + dz);
                let Some(lot) = crate::city::lot_at(body, &s, x, z) else { continue };
                if lot.program().key == "lot/arboleda" {
                    continue;
                }
                en_parcela += 1;
                if surface_decor_at(body, x, z).is_some() {
                    arbolada += 1;
                }
            }
        }
        assert!(en_parcela > 500, "la ventana no cayó sobre ninguna parcela ({en_parcela} celdas)");
        assert_eq!(arbolada, 0, "{arbolada} de {en_parcela} celdas de parcela con decor encima");
    }

    /// El otro lado de la misma moneda: podar el pueblo no puede dejar el campo
    /// pelado. Un bosque tiene que seguir siendo un bosque.
    #[test]
    fn el_campo_sigue_teniendo_bosque() {
        let body = CelestialBody::Earth;
        // Primero se BUSCA un bosque —muestreo ralo y ancho, que el bosque no cae
        // necesariamente al lado del origen— y recién después se mide DENTRO de
        // él. Medir muestreando el planeta entero daba 466 celdas de bosque: con
        // tan pocas, el ruido del muestreo (±24 ‰) es mayor que lo que se quiere
        // medir, y el test no distinguiría 30 ‰ de 140 ‰.
        let mut centro = None;
        'buscar: for iz in -190..190i64 {
            for ix in -190..190i64 {
                let (tx, tz) = (ix * 313, iz * 313);
                if crate::geography::biome_at(body, tx, tz) == crate::geography::Biome::Forest {
                    centro = Some((tx, tz));
                    break 'buscar;
                }
            }
        }
        let (cx, cz) = centro.expect("ningún bosque en toda la Tierra");
        let mut arboles = 0;
        let mut celdas = 0;
        for dz in -250..250i64 {
            for dx in -250..250i64 {
                let (tx, tz) = (cx + dx, cz + dz);
                if crate::geography::biome_at(body, tx, tz) != crate::geography::Biome::Forest {
                    continue;
                }
                // Sólo cuentan las celdas donde podría crecer algo: una celda de
                // bosque que es roca o agua nunca lleva árbol y bajaría la medida
                // sin que nadie haya tocado la densidad.
                if !crate::topdown::topdown_tile(body, 0, tx, tz).buildable_ground() {
                    continue;
                }
                celdas += 1;
                if surface_decor_at(body, tx, tz) == Some(DecorKind::Tree) {
                    arboles += 1;
                }
            }
        }
        assert!(celdas > 10_000, "no se encontró bosque para medir ({celdas} celdas)");
        let permille = arboles as f32 / celdas as f32 * 1000.0;
        assert!(
            (12.0..48.0).contains(&permille),
            "{permille:.1} por mil de celdas de bosque con árbol"
        );
    }
}

#[cfg(test)]
mod memo_decor_tests {
    use super::*;

    #[test]
    fn la_memoria_del_decor_no_cambia_ninguna_respuesta() {
        for (cx, cz) in [(0i64, 0i64), (-754, -1818), (3000, -900)] {
            for dx in -24..24 {
                for dz in -24..24 {
                    let a = surface_decor_at_calc(CelestialBody::Earth, cx + dx, cz + dz);
                    assert_eq!(a, surface_decor_at(CelestialBody::Earth, cx + dx, cz + dz));
                    assert_eq!(a, surface_decor_at(CelestialBody::Earth, cx + dx, cz + dz));
                }
            }
        }
    }
}
