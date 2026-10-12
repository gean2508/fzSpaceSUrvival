//! ECOLOGÍA DE MUNDO — la vida NO gira alrededor del jugador.
//!
//! Antes la fauna vivía en un **anillo**: se sembraba alrededor tuyo y el bicho que se
//! alejaba se teletransportaba al borde opuesto. Siempre había vida al lado, y por eso
//! explorar no valía nada: el mundo no tenía lugares, tenía un radio.
//!
//! Acá está el reemplazo. El mundo se divide en **regiones** de `REGION` tiles de lado y
//! cada región tiene:
//!
//! 1. una **capacidad de carga** determinista que sale del BIOMA (`carrying_capacity`) —
//!    un bosque sostiene manadas, un desierto casi nada;
//! 2. una **población** de tres niveles tróficos (plantas → herbívoros → carnívoros) que
//!    avanza con un paso Lotka–Volterra logístico (`step`) y se difunde a las regiones
//!    vecinas (`migrate`).
//!
//! El cliente (`fauna.rs`) sólo MATERIALIZA como entidades las regiones cercanas: la
//! población es la fuente de verdad, los animales son la vista. Nada se teletransporta y
//! nadie se esfuma — un animal que sale de la ventana de simulación vuelve a ser número en
//! la región donde estaba, y sigue vivo ahí.
//!
//! Reglas del mundo estable (biblia / memoria `ai-life-emergence-no-prune`):
//! - **Nada desaparece de la nada.** Las plantas rebrotan por semilla (`SEED_FLOOR`) y una
//!   especie extinta localmente vuelve MUY despacio por inmigración de fuera del mapa
//!   (`RECOLONISE`): el mundo es más grande que lo que simulamos.
//! - **La muerte es visible**: los herbívoros bajan por PREDACIÓN, no por metabolismo.
//! - Es determinista y sin `rand` → testeable, y una región no visitada vale exactamente
//!   su equilibrio (`equilibrium`), o sea "la vida estuvo ahí todo este tiempo" gratis.

use serde::{Deserialize, Serialize};

use crate::geography::{biome_at, hash2, Biome};
use crate::CelestialBody;

/// Lado de una región de ecología, en tiles (= metros, `TILE_M == 1`). 64 m es el orden
/// del radio de render: la ventana de simulación 3×3 cubre bastante más de lo que ves, así
/// que ningún animal "aparece" delante tuyo.
pub const REGION: i64 = 64;

/// Cuántos puntos se muestrean por región para leer el bioma (lattice 3×3). Barato y
/// suficiente: un borde bosque/desierto da una capacidad intermedia, que es lo correcto.
const SAMPLES: i64 = 3;

// --- Constantes del modelo (por segundo de mundo) ---------------------------------
/// Crecimiento logístico de la vegetación. Calibrado contra el CONSUMO real de una manada
/// materializada (ver `fauna.rs`): con 0.020 un prado bueno quedaba pelado en un minuto de
/// pastoreo y tardaba cinco en volver — el bosque se veía muerto justo cuando llegabas.
const PLANT_GROWTH: f32 = 0.050;
/// Piso de semilla: la vegetación de un bioma que la admite nunca llega a cero.
const SEED_FLOOR: f32 = 0.02;
/// Pastoreo (Holling II): cuánto come un herbívoro por segundo con comida de sobra.
const GRAZE_RATE: f32 = 0.010;
const PLANT_HALF: f32 = 2.0;
/// Caza: presas cobradas por carnívoro y segundo con presa de sobra.
const HUNT_RATE: f32 = 0.0030;
const PREY_HALF: f32 = 2.5;
/// Cuánta cría sale de lo pastoreado / cazado (con techo logístico).
const HERB_BIRTH: f32 = 0.55;
const CARN_BIRTH: f32 = 0.30;
/// Mortalidad base (vejez/accidentes). Chica: la muerte principal es la predación.
const HERB_DEATH: f32 = 0.0016;
const CARN_DEATH: f32 = 0.0020;
/// Recolonización desde fuera del mapa cuando una especie se extingue en la región.
const RECOLONISE: f32 = 0.00015;
/// Fracción de la diferencia de presión que migra por segundo entre regiones vecinas.
const MIGRATE_RATE: f32 = 0.010;
/// Techo duro sobre la capacidad (evita explosiones numéricas en pasos largos).
const OVERSHOOT: f32 = 1.5;

/// Nivel trófico de un animal — la cadena alimenticia del mundo.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Trophic {
    /// Herbívoro: pasta la vegetación de su región.
    Herbivore,
    /// Carnívoro: caza herbívoros. Es la fuente de muerte VISIBLE del ecosistema.
    Carnivore,
}

/// Cuánta vida sostiene una región. Sale del bioma y **no cambia nunca** (es el terreno).
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Capacity {
    pub plants: f32,
    pub herbivores: f32,
    pub carnivores: f32,
}

impl Capacity {
    /// Una región sin nada que sostener (océano, roca pelada, regolito).
    pub fn is_barren(&self) -> bool {
        self.plants <= 0.01
    }
}

/// Población VIVA de una región. Es la fuente de verdad: los animales-entidad son sólo la
/// vista que se materializa cerca del jugador.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Population {
    /// Biomasa de forraje disponible (unidades = puntos de comida materializables).
    pub plants: f32,
    pub herbivores: f32,
    pub carnivores: f32,
}

// --- Geometría de región -----------------------------------------------------------

/// Región que contiene la posición de mundo `(x, z)`.
pub fn region_of(x: f32, z: f32) -> (i32, i32) {
    (
        (x / REGION as f32).floor() as i32,
        (z / REGION as f32).floor() as i32,
    )
}

/// Centro (en metros) de la región `(rx, rz)`.
pub fn region_center(rx: i32, rz: i32) -> (f32, f32) {
    let h = REGION as f32 * 0.5;
    (
        rx as f32 * REGION as f32 + h,
        rz as f32 * REGION as f32 + h,
    )
}

/// Punto determinista `i` dentro de la región — dónde se planta el forraje y dónde se
/// junta una manada. Estable entre sesiones: la misma región siempre tiene sus mismos
/// claros. Devuelve coordenadas de MUNDO.
pub fn region_point(body: CelestialBody, rx: i32, rz: i32, i: u32) -> (f32, f32) {
    let seed = 0x00ec_0100 ^ (body as u32).wrapping_mul(2_654_435_761);
    let h = hash2(seed.wrapping_add(i.wrapping_mul(0x9E37_79B9)), rx as i64, rz as i64);
    let margin = 3.0;
    let span = REGION as f32 - margin * 2.0;
    let ox = margin + ((h & 0xFFFF) as f32 / 65_535.0) * span;
    let oz = margin + (((h >> 16) & 0xFFFF) as f32 / 65_535.0) * span;
    (rx as f32 * REGION as f32 + ox, rz as f32 * REGION as f32 + oz)
}

// --- Capacidad de carga (el bioma manda) -------------------------------------------

/// Cuánto forraje da un bioma, 0..1. Es más fino que `geography::vegetation_density`
/// (que mide ÁRBOLES para la decoración): acá interesa lo COMESTIBLE, así que el pastizal
/// puntúa alto aunque no tenga un solo árbol.
fn forage_of(b: Biome) -> f32 {
    match b {
        Biome::Forest => 0.90,
        Biome::Grassland => 1.00,
        Biome::Swamp => 0.70,
        Biome::River => 0.75,
        Biome::Savanna => 0.55,
        Biome::Highland => 0.30,
        Biome::Beach => 0.10,
        Biome::Snow => 0.06,
        Biome::Desert => 0.05,
        Biome::Mountain => 0.04,
        // Océano, hielo, basalto y regolito no dan pasto. Ahí la vida hay que llevarla.
        Biome::Ocean | Biome::Ice | Biome::Basalt | Biome::Regolith => 0.0,
    }
}

/// Cuánto ESCONDITE da un bioma al depredador (emboscada). Un bosque sostiene más
/// carnívoros que un pastizal abierto con la misma cantidad de presas.
fn cover_of(b: Biome) -> f32 {
    match b {
        Biome::Forest => 1.0,
        Biome::Swamp => 0.85,
        Biome::Highland | Biome::Mountain => 0.70,
        Biome::Savanna => 0.55,
        Biome::Grassland => 0.45,
        _ => 0.30,
    }
}

/// Capacidad de carga de una región: muestreo 3×3 de biomas → forraje medio → herbívoros
/// → carnívoros. Determinista y sin estado; el cliente la cachea junto a la población.
pub fn carrying_capacity(body: CelestialBody, rx: i32, rz: i32) -> Capacity {
    let step = REGION / SAMPLES;
    let (mut forage, mut cover) = (0.0f32, 0.0f32);
    for i in 0..SAMPLES {
        for j in 0..SAMPLES {
            let tx = rx as i64 * REGION + step / 2 + i * step;
            let tz = rz as i64 * REGION + step / 2 + j * step;
            let b = biome_at(body, tx, tz);
            forage += forage_of(b);
            cover += cover_of(b);
        }
    }
    let n = (SAMPLES * SAMPLES) as f32;
    let forage = forage / n;
    let cover = cover / n;
    if forage < 0.06 {
        // Yermo real: ni forraje ni cadena. Que un desierto esté vacío es la mitad del
        // sentido de explorar — la otra mitad es el oasis de al lado.
        return Capacity::default();
    }
    let plants = forage * 20.0;
    let herbivores = forage * 8.0;
    // El depredador necesita presa Y escondite: sin las dos cosas, no se sostiene.
    //
    // La proporción es BAJA a propósito (un depredador por cada ~20 presas): una región mide
    // 64 m y el territorio de un carnívoro es mucho mayor, así que su capacidad se cuenta
    // por región pero se MATERIALIZA por ventana (`fauna.rs`). Es una cifra MEDIDA, no
    // supuesta: con 0.35 salían 8 depredadores contra 15 presas y el valle se vaciaba en
    // minuto y medio con el jugador delante (`HA_ECOTEST HA_ECOSETTLE=90`). Ojo al tocarla:
    // los nacimientos empujan la población hasta la CAPACIDAD, no hasta el equilibrio.
    let carnivores = herbivores * 0.05 * cover;
    Capacity { plants, herbivores, carnivores }
}

/// La población de una región que NUNCA se visitó: su equilibrio. Así el mundo no
/// "empieza a vivir" cuando llegás — ya estaba vivo, y su estado es el que le corresponde
/// a su bioma. (Lo que se aparta del equilibrio es lo que HICISTE ahí.)
pub fn equilibrium(cap: Capacity) -> Population {
    if cap.is_barren() {
        return Population::default();
    }
    Population {
        plants: cap.plants * 0.70,
        herbivores: cap.herbivores * 0.55,
        carnivores: cap.carnivores * 0.50,
    }
}

// --- El paso de simulación ---------------------------------------------------------

/// Sólo la VEGETACIÓN. Es el paso que corre en una región **materializada** (con animales
/// de verdad caminando): ahí los herbívoros y carnívoros los simulan las entidades, no la
/// aritmética, así que aplicar también los términos de fauna contaría cada bicho dos veces.
/// El pasto, en cambio, rebrota igual estés mirando o no.
pub fn step_flora(pop: Population, cap: Capacity, dt: f32) -> Population {
    let dt = dt.clamp(0.0, 60.0);
    if cap.is_barren() {
        return Population { plants: 0.0, ..pop };
    }
    let mut plants = pop.plants.max(cap.plants * SEED_FLOOR);
    plants += PLANT_GROWTH * plants * (1.0 - plants / cap.plants) * dt;
    Population { plants: plants.clamp(0.0, cap.plants * OVERSHOOT), ..pop }
}

/// Un paso de `dt` segundos de mundo para UNA región. Plantas logísticas, pastoreo y caza
/// tipo Holling II, natalidad por comida con techo, y recolonización desde fuera del mapa.
///
/// Se llama para TODAS las regiones conocidas, estés o no parado ahí: es aritmética, no
/// entidades. Eso es lo que hace que el mundo siga vivo sin vos.
pub fn step(pop: Population, cap: Capacity, dt: f32) -> Population {
    let dt = dt.clamp(0.0, 60.0);
    if cap.is_barren() {
        return Population::default();
    }
    let Population { plants: _, mut herbivores, mut carnivores } = pop;
    let mut plants = step_flora(pop, cap, dt).plants;

    // 2) Pastoreo (Holling II): saturado por lo que hay, y nunca más de la mitad del pasto
    //    en un solo paso (si no, un tick largo arrasa la región).
    let graze = (GRAZE_RATE * herbivores * plants / (plants + PLANT_HALF) * dt).min(plants * 0.5);
    plants -= graze;

    // 3) Caza: la muerte VISIBLE del ecosistema.
    let hunt =
        (HUNT_RATE * carnivores * herbivores / (herbivores + PREY_HALF) * dt).min(herbivores * 0.5);

    // 4) Herbívoros: nacen de lo que pastaron (con techo de la región), mueren por caza
    //    y por una mortalidad base chica.
    let room_h = (1.0 - herbivores / cap.herbivores.max(1e-3)).max(0.0);
    herbivores += HERB_BIRTH * graze * room_h - hunt - HERB_DEATH * herbivores * dt;

    // 5) Carnívoros: igual, pero comiendo herbívoros.
    let room_c = (1.0 - carnivores / cap.carnivores.max(1e-3)).max(0.0);
    carnivores += CARN_BIRTH * hunt * room_c - CARN_DEATH * carnivores * dt;

    // 6) Recolonización: el mapa simulado es un recorte del mundo. Si la región quedó sin
    //    una especie, vuelve MUY despacio desde fuera — nunca de golpe, y nunca donde el
    //    bioma no la sostiene.
    if cap.herbivores > 0.05 {
        herbivores += RECOLONISE * cap.herbivores * dt;
    }
    if cap.carnivores > 0.05 && herbivores > cap.herbivores * 0.25 {
        carnivores += RECOLONISE * 0.5 * cap.carnivores * dt;
    }

    Population {
        plants: plants.clamp(0.0, cap.plants * OVERSHOOT),
        herbivores: herbivores.clamp(0.0, cap.herbivores * OVERSHOOT),
        carnivores: carnivores.clamp(0.0, cap.carnivores * OVERSHOOT),
    }
}

/// Difusión entre dos regiones vecinas: los animales se van de donde hay presión (mucha
/// población para poca capacidad) hacia donde sobra sitio. Es lo que reparte una manada por
/// un valle y lo que hace que un depredador aparezca donde hay presa — sin scriptearlo.
///
/// Devuelve `(a, b)` actualizadas. Conserva la suma: nadie se pierde en el camino.
pub fn migrate(a: Population, a_cap: Capacity, b: Population, b_cap: Capacity, dt: f32) -> (Population, Population) {
    let (mut a, mut b) = (a, b);
    let dt = dt.clamp(0.0, 60.0);
    let flow = |from: f32, from_cap: f32, to: f32, to_cap: f32| -> f32 {
        if from <= 0.0 {
            return 0.0;
        }
        let pf = from / from_cap.max(1e-3);
        let pt = to / to_cap.max(1e-3);
        if pf <= pt {
            return 0.0;
        }
        // Sólo emigra a donde hay con qué vivir: nadie camina hacia el yermo.
        if to_cap <= 0.01 {
            return 0.0;
        }
        (MIGRATE_RATE * (pf - pt) * from * dt).min(from * 0.25)
    };
    let h = flow(a.herbivores, a_cap.herbivores, b.herbivores, b_cap.herbivores)
        - flow(b.herbivores, b_cap.herbivores, a.herbivores, a_cap.herbivores);
    let c = flow(a.carnivores, a_cap.carnivores, b.carnivores, b_cap.carnivores)
        - flow(b.carnivores, b_cap.carnivores, a.carnivores, a_cap.carnivores);
    a.herbivores -= h;
    b.herbivores += h;
    a.carnivores -= c;
    b.carnivores += c;
    (a, b)
}

/// ¿La región está tan cerca de su equilibrio que guardarla no aporta nada? El cliente usa
/// esto para PODAR el mapa de regiones conocidas: lo que volvió a su estado natural es
/// indistinguible del valor perezoso, así que la memoria queda acotada para siempre.
pub fn is_settled(pop: Population, cap: Capacity) -> bool {
    if cap.is_barren() {
        return true;
    }
    let eq = equilibrium(cap);
    let near = |a: f32, b: f32, scale: f32| (a - b).abs() <= scale.max(1e-3) * 0.12;
    near(pop.plants, eq.plants, cap.plants)
        && near(pop.herbivores, eq.herbivores, cap.herbivores)
        && near(pop.carnivores, eq.carnivores, cap.carnivores)
}

#[cfg(test)]
mod tests {
    use super::*;

    const FOREST: Capacity = Capacity { plants: 10.0, herbivores: 5.0, carnivores: 1.0 };

    fn run(mut pop: Population, cap: Capacity, secs: f32) -> Population {
        let mut t = 0.0;
        while t < secs {
            pop = step(pop, cap, 5.0);
            t += 5.0;
        }
        pop
    }

    #[test]
    fn una_region_yerma_no_sostiene_nada() {
        let cap = Capacity::default();
        let pop = run(Population { plants: 5.0, herbivores: 5.0, carnivores: 2.0 }, cap, 600.0);
        assert_eq!(pop, Population::default());
    }

    #[test]
    fn el_desierto_esta_casi_vacio_y_el_bosque_lleno() {
        // Mismo modelo, distinto bioma: la diferencia la hace el terreno, no el jugador.
        let desert = Capacity { plants: 0.6, herbivores: 0.3, carnivores: 0.03 };
        let d = equilibrium(desert);
        let f = equilibrium(FOREST);
        assert!(d.herbivores < 1.0, "un desierto no puede tener manadas: {d:?}");
        assert!(f.herbivores > 2.0, "un bosque tiene que tener vida: {f:?}");
    }

    #[test]
    fn la_vegetacion_rebrota_desde_cero() {
        // Nada se esfuma para siempre: el pasto arrasado vuelve solo.
        let pop = run(Population { plants: 0.0, herbivores: 0.0, carnivores: 0.0 }, FOREST, 4_000.0);
        assert!(pop.plants > FOREST.plants * 0.5, "el pasto no rebrotó: {pop:?}");
    }

    #[test]
    fn una_region_cazada_a_cero_se_recoloniza_pero_lento() {
        let start = Population { plants: 8.0, herbivores: 0.0, carnivores: 0.0 };
        let corto = run(start, FOREST, 120.0);
        assert!(corto.herbivores < 0.6, "volvieron demasiado rapido: {corto:?}");
        let largo = run(start, FOREST, 20_000.0);
        assert!(largo.herbivores > 1.0, "el mundo no se recupero nunca: {largo:?}");
    }

    #[test]
    fn la_poblacion_queda_acotada_por_la_capacidad() {
        // Sin techo, un modelo así explota y el teléfono se queda sin fauna que dibujar.
        let pop = run(Population { plants: 10.0, herbivores: 5.0, carnivores: 1.0 }, FOREST, 100_000.0);
        assert!(pop.herbivores <= FOREST.herbivores * OVERSHOOT + 1e-3, "{pop:?}");
        assert!(pop.carnivores <= FOREST.carnivores * OVERSHOOT + 1e-3, "{pop:?}");
        assert!(pop.plants <= FOREST.plants * OVERSHOOT + 1e-3, "{pop:?}");
    }

    #[test]
    fn los_depredadores_bajan_a_las_presas() {
        let sin = run(Population { plants: 10.0, herbivores: 4.0, carnivores: 0.0 }, FOREST, 3_000.0);
        let con = run(Population { plants: 10.0, herbivores: 4.0, carnivores: 1.0 }, FOREST, 3_000.0);
        assert!(con.herbivores < sin.herbivores, "la caza no hace mella: {con:?} vs {sin:?}");
    }

    #[test]
    fn la_migracion_conserva_la_poblacion() {
        let lleno = Population { plants: 8.0, herbivores: 5.0, carnivores: 1.0 };
        let vacio = Population { plants: 8.0, herbivores: 0.2, carnivores: 0.0 };
        let antes = lleno.herbivores + vacio.herbivores;
        let (a, b) = migrate(lleno, FOREST, vacio, FOREST, 30.0);
        assert!((a.herbivores + b.herbivores - antes).abs() < 1e-4);
        assert!(b.herbivores > vacio.herbivores, "no emigro nadie al valle vacio");
        assert!(a.herbivores < lleno.herbivores);
    }

    #[test]
    fn nadie_emigra_al_yermo() {
        let lleno = Population { plants: 8.0, herbivores: 5.0, carnivores: 1.0 };
        let (a, b) = migrate(lleno, FOREST, Population::default(), Capacity::default(), 30.0);
        assert_eq!(a.herbivores, lleno.herbivores);
        assert_eq!(b.herbivores, 0.0);
    }

    #[test]
    fn las_regiones_no_se_solapan_y_el_punto_cae_dentro() {
        assert_eq!(region_of(0.5, 0.5), (0, 0));
        assert_eq!(region_of(-0.5, -0.5), (-1, -1));
        assert_eq!(region_of(REGION as f32 + 1.0, 0.0), (1, 0));
        for i in 0..8 {
            let (x, z) = region_point(CelestialBody::Earth, 3, -2, i);
            assert_eq!(region_of(x, z), (3, -2), "punto fuera de su region: {x},{z}");
        }
    }

    /// CALIBRACIÓN (no borrar): la fauna se materializa por CABEZAS ENTERAS, así que una
    /// capacidad cuyo equilibrio no llega a 1.0 es una especie que el jugador **no ve
    /// nunca**. El herbívoro tiene que dar manada en UNA región; el depredador, que tiene
    /// territorio grande, se cuenta sobre la ventana de simulación (3×3 regiones). Si esto
    /// se rompe, la cadena alimenticia queda coja en pantalla aunque los números cuadren.
    const SIM_WINDOW: f32 = 9.0;

    #[test]
    fn un_bosque_de_verdad_sostiene_un_depredador_entero() {
        let mut best = Capacity::default();
        // Barrido de regiones reales de la Tierra: alcanza con que EXISTA el bioma bueno.
        for rx in -40..40 {
            for rz in -40..40 {
                let cap = carrying_capacity(CelestialBody::Earth, rx, rz);
                if cap.carnivores > best.carnivores {
                    best = cap;
                }
            }
        }
        let eq = equilibrium(best);
        assert!(
            eq.carnivores * SIM_WINDOW >= 1.0,
            "ningun bioma terrestre llega a un depredador materializable: cap {best:?} eq {eq:?}"
        );
        // …y tampoco a una plaga de ellos: más de uno por cada cuatro presas de la ventana
        // es un ecosistema que se come a sí mismo (fue el bug que midió `HA_ECOTEST`).
        assert!(
            eq.carnivores <= eq.herbivores * 0.25,
            "demasiados depredadores por presa: eq {eq:?}"
        );
        assert!(
            eq.herbivores >= 2.0,
            "el mejor bioma no da ni una manada: eq {eq:?}"
        );
    }

    /// La otra mitad: que exista terreno REALMENTE vacío. Si todo el planeta sostiene
    /// fauna, volvimos al anillo por la puerta de atrás — explorar no distinguiría nada.
    #[test]
    fn el_planeta_tiene_zonas_de_verdad_vacias() {
        let vacias = (-40..40)
            .flat_map(|rx| (-40..40).map(move |rz| (rx, rz)))
            .filter(|&(rx, rz)| carrying_capacity(CelestialBody::Earth, rx, rz).is_barren())
            .count();
        assert!(vacias > 0, "no hay una sola region yerma en la Tierra");
    }

    #[test]
    fn el_equilibrio_esta_asentado_por_definicion() {
        assert!(is_settled(equilibrium(FOREST), FOREST));
        let arrasada = Population { plants: 0.0, herbivores: 0.0, carnivores: 0.0 };
        assert!(!is_settled(arrasada, FOREST), "una region arrasada no esta asentada");
    }
}
