//! Fauna VIVA con cerebro colmena frugal + **EVOLUCIÓN EN EL MUNDO** (FASE 8 / B4).
//!
//! No es una conducta congelada: es un ECOSISTEMA. Cada animal ejecuta un genoma
//! IAFZ-64; **come, se reproduce (hereda el genoma con mutación) y muere** según le
//! va en el mundo REAL → **selección natural in-game** = aprenden por su cuenta. El
//! sensor incluye **percepción SOCIAL** (vecinos), así pueden emerger dinámicas de
//! grupo/territorio/conflicto **sin enseñárselas** (memoria `ai-life-emergence-no-prune`).
//! NO se poda la conducta emergente: si surgen sociedades/guerras, bienvenidas.
//!
//! El gym (`./editor_assets gym`) SIEMBRA competencia base (forrajeo); la emergencia
//! social sale de la vida multi-agente acá. Arte = quad de color hasta `npc/fauna`.
//!
//! ## SE FUE EL ANILLO (no reintroducir)
//!
//! Hasta acá la fauna ambiental vivía en un **anillo alrededor del jugador**: se sembraba
//! a tu lado al pisar la superficie y el bicho que se alejaba más de `STREAM_R` se
//! teletransportaba al borde opuesto. Garantizaba tener vida al lado y por eso mismo
//! **mataba la exploración**: el mundo no tenía lugares, tenía un radio.
//!
//! Ahora la fuente de verdad es `ha_shared::ecology`: el mundo se parte en REGIONES con
//! capacidad de carga sacada del BIOMA y una cadena alimenticia (plantas → herbívoros →
//! carnívoros) que avanza **en todos los cuerpos, estés o no ahí**. Este módulo sólo
//! MATERIALIZA como entidades las regiones cercanas y devuelve los números al salir:
//!
//! - Un bosque hierve de vida y un desierto está vacío — hay que ir a buscarla.
//! - Nada se teletransporta. Un animal que se aleja vuelve a ser población en su región.
//! - Cazar un valle lo deja vacío de verdad, y tarda en recuperarse.
//!
//! El VIVARIO (residentes colocados en creativo) nunca tuvo anillo y sigue igual.

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;

use bevy::prelude::*;
use serde::{Deserialize, Serialize};

use ha_shared::ecology::{self, Capacity, Population, Trophic};
use ha_shared::fauna_brain::sense;
use ha_shared::sims::{self, Actividad, Anuncio, Clase, Motivo, Vida};
use ha_shared::CelestialBody;

use crate::platform::is_app_paused;
use crate::scene::{RegionRuntime, WorldContextRuntime};
use crate::sim::Player;
use crate::world2d::{
    active_surface_body, render_xy, sort_z, surface_region_active, ActivePlanet, TileWorld,
};

/// Genoma colmena SEMILLA (forrajeo, del gym). Cada nacimiento lo hereda mutado.
const GENOME: &[u8] = include_bytes!("../../iafz-sys/genomes/FAUNA.adn");

/// Tope de fauna ambiental VIVA como entidad (perf/batería). No es un tope de mundo: la
/// población de más allá sigue existiendo como número en `ha_shared::ecology`.
const MAX_LIVE: usize = 32;
const MAX_FAUNA_ID: i32 = 48; // ids de cerebro reservados a la fauna (0..48)
/// Ventana de MATERIALIZACIÓN, en regiones alrededor de la tuya (1 → 3×3 = 192 m de lado
/// con `ecology::REGION == 64`). Supera de sobra el radio de render, así ningún animal
/// "aparece" delante tuyo: entra caminando o ya estaba ahí.
const SIM_RINGS: i32 = 1;
/// Ventana que se MANTIENE EN MEMORIA (con su población) alrededor tuyo: da vecinos a la
/// migración, así la manada que se va de tu vista tiene adónde ir.
const AWARE_RINGS: i32 = 2;
/// Cada cuánto (s de mundo) corre el paso de ecología de TODAS las regiones conocidas.
const ECO_TICK: f32 = 4.0;
/// Tope de regiones guardadas en memoria/disco. Se podan primero las que volvieron a su
/// equilibrio (indistinguibles del valor perezoso), así el guardado no crece sin fin.
const MAX_KNOWN_REGIONS: usize = 3_000;
/// Cuántos puntos de forraje puede llegar a mostrar una región (el resto es biomasa que
/// existe pero no está dibujada).
const MAX_FORAGE_REGION: usize = 6;
/// Cuánta biomasa vale un punto de forraje dibujado.
const FORAGE_UNIT: f32 = 1.5;
const VEL: f32 = 2.6;
const TURN: f32 = 2.3;
const EAT_D2: f32 = 0.55;
const REPRO_ENERGY: f32 = 0.94; // se reproduce si está bien alimentado
const REPRO_COST: f32 = 0.55;
const MUTATE_RATE: f32 = 0.06;
// --- Ecosistema estable (inspiración Minecraft/Project Zomboid): el mundo vive sin el
// jugador y NADA se esfuma solo. El metabolismo NUNCA es fatal (piso de energía); la muerte
// viene de causas VISIBLES: PREDACIÓN (depredador caza presa) y el jugador. Eso da la
// selección para evolucionar sin "desaparecer de la nada". Diversión > realismo.
const STARVE_FLOOR: f32 = 0.12; // energía mínima — el hambre ralentiza, no mata
const HUNT_RANGE: f32 = 16.0; // a qué distancia el depredador detecta una presa
const PREDATION_D2: f32 = 2.56; // radio² (~1.6 m) para concretar la caza

// --- VIVARIO (F1): fauna RESIDENTE colocada en creativo. Ver PLAN_VIVARIO_*. ---
/// Offset de índice para los botones de spawn en la paleta de build (no colisiona
/// con piezas/prefabs/assets reales). `build.rs` lo detecta antes de `build_selection`.
pub const SPAWN_PALETTE_BASE: usize = 10_000;
/// Tope de residentes por sesión (perf/batería) — cuentan aparte del `MAX_POP` ambiental.
///
/// Subió de 24 a 64 con la GRANJA: los animales que alimentás se vuelven residentes
/// (tuyos, persisten) y las crías también; un corral de vacas + gallinero + chiquero
/// se come 24 enseguida.
const MAX_RESIDENTS: usize = 64;
/// Rango de ids de cerebro reservado a residentes (ambiental usa `0..MAX_FAUNA_ID`,
/// NPCs `72..120`, compañero `990/991`). Arranca en 200 para caber 64 sin pisar a nadie.
const RES_ID_BASE: i32 = 200;
/// Salvaguarda anti mínimos locales: cada N nacimientos, el hijo nace del SEED del gym
/// (inmigración) en vez del padre → reinyecta diversidad perdida (PLAN_VIVARIO §4.3).
const RES_SEED_EVERY: u32 = 6;
/// Hipermutación esporádica (§4.2): tasa ×4 para saltos exploratorios.
const RES_HYPERMUTATE_RATE: f32 = MUTATE_RATE * 4.0;
// --- GRANJA: cría a mano (Minecraft) ---------------------------------------------
/// Cuánto dura el celo después de darle de comer (Minecraft: 30 s).
pub const CELO_SECS: f32 = 30.0;
/// Descanso de un padre antes de volver a criar (Minecraft: 5 min).
pub const CRIA_CD_SECS: f32 = 300.0;
/// Lo que tarda una cría en hacerse adulta (Minecraft: 20 min). Cada comida le
/// descuenta un 10 %, igual que allá.
pub const CRECER_SECS: f32 = 1200.0;
/// A qué distancia un animal en celo ve a su pareja y camina hacia ella (m).
const PAREJA_R: f32 = 14.0;
/// Distancia² a la que la pareja se junta y nace la cría (~1,3 m).
const CITA_D2: f32 = 1.7;

/// Temperatura evolutiva (F3, §4.5): escala mutación + frecuencia de inmigración según la
/// DIVERSIDAD de conducta de los residentes vivos. Diversidad alta → temp≈`TEMP_MIN`
/// (estable); diversidad baja (estancamiento = mínimo local) → sube a `TEMP_MAX` → más
/// exploración. Es la salvaguarda central "prevenidos ante mínimos/máximos locales".
const TEMP_MIN: f32 = 1.0;
const TEMP_MAX: f32 = 3.0;

/// Slots FIJOS de la paleta creativa de spawn. El spawn resuelve el índice contra la fauna
/// del PLANETA donde estás (F5), así el mismo botón spawnea el bicho local; los índices
/// que ese cuerpo no tiene, no hacen nada.
const SPAWN_PALETTE_SLOTS: usize = 9;

/// Cuántos slots muestra la paleta creativa (para `hud.rs`). Genéricos y body-aware.
pub fn spawn_species_count() -> usize {
    SPAWN_PALETTE_SLOTS
}

/// Etiqueta ASCII del slot `i` (genérica: la especie real depende del planeta).
pub fn spawn_species_label(i: usize) -> &'static str {
    const L: [&str; SPAWN_PALETTE_SLOTS] =
        ["Fauna 1", "Fauna 2", "Fauna 3", "Fauna 4", "Fauna 5", "Fauna 6", "Fauna 7", "Fauna 8",
         "Fauna 9"];
    L.get(i).copied().unwrap_or("Fauna")
}


/// Slot de arte del hueco `i` de la paleta creativa — para que la TARJETA de la
/// mochila muestre el bicho en vez de un cuadrado. Son los slots terrestres (la
/// fauna inventada por planeta no tiene slot asignable); si el pack no los trae, la
/// tarjeta cae al icono de ítem que le pasa `hud.rs`.
pub fn spawn_species_art(i: usize) -> &'static str {
    const K: [&str; SPAWN_PALETTE_SLOTS] = [
        "npc/fauna_cow",
        "npc/fauna_chicken",
        "npc/fauna_sheep",
        "npc/fauna_pig",
        "npc/fauna_dog",
        "npc/fauna_goat",
        "npc/fauna_donkey",
        "npc/fauna_rabbit",
        "npc/fauna_duck",
    ];
    K.get(i).copied().unwrap_or("npc/fauna_cow")
}

/// Radio (m) de RENDER/simulación de residentes alrededor del jugador (F2). Más allá,
/// el residente se OCULTA y se CONGELA (no se despawnea): sigue como dato en su lugar y
/// vuelve a la vida cuando te acercás. "Guardados como datos, se renderizan cerca."
const RENDER_R: f32 = 64.0;
/// Cada cuántos segundos se persiste el vivario a disco (además del guardado al cambiar
/// de cuerpo y al pausar la app).
const VIV_SAVE_EVERY: f32 = 20.0;

/// Marca de fauna RESIDENTE: no se streamea, se activa por proximidad y PERSISTE. `uid`
/// = identidad estable (nombra su archivo de genoma `g_<uid>.adn`) entre sesiones.
#[derive(Component)]
pub struct Resident {
    uid: u64,
}

/// Pedido de spawn de un residente en `(x, z)` (celda apuntada). Lo emite `build.rs`
/// al completar el hold de colocación en creativo; `spawn_residents` lo consume.
#[derive(Event)]
pub struct SpawnResidentEvent {
    pub x: f32,
    pub z: f32,
    pub species: usize,
    /// F8: fuerza al residente a ser HOSTIL (depredador), sin importar la especie.
    pub hostile: bool,
}

/// F8 — sub-índice de la paleta creativa (dentro del rango de spawn) para el FEEDER de
/// comida persistente. `build.rs` lo separa del spawn de fauna.
pub const FEED_PALETTE_SUB: usize = 500;

/// F7 — sub-índice de la paleta creativa para COLOCAR una cámara de vigilancia.
pub const CAMERA_PALETTE_SUB: usize = 501;
/// F8 — sub-índice para spawnear un DEPREDADOR (residente forzado hostil, acecha).
pub const PREDATOR_PALETTE_SUB: usize = 502;
/// F8 — sub-índice para colocar AGUA (tile) — abrevadero para el hábitat.
pub const WATER_PALETTE_SUB: usize = 503;

/// F8 — pedido de colocar un feeder de comida persistente en `(x, z)`.
#[derive(Event)]
pub struct SpawnFoodEvent {
    pub x: f32,
    pub z: f32,
}

/// F8 — comida COLOCADA (feeder): no se streamea y no se reubica al ser comida → fuente
/// permanente para diseñar la presión de comida del hábitat (debug creativo).
#[derive(Component)]
struct PlacedFood;

/// Registro persistente de un residente (vive como DATO en TODOS los cuerpos a la vez; el
/// genoma evolucionado se guarda aparte por `iafz_sys::save` en `g_<uid>.adn`).
#[derive(Clone, Serialize, Deserialize)]
struct ResidentRecord {
    uid: u64,
    body: u8,
    x: f32,
    z: f32,
    heading: f32,
    species: usize,
    hunger: f32,
    thirst: f32,
    energy: f32,
    /// GRANJA: 0 = cría, 1 = adulto (los registros viejos son adultos).
    #[serde(default = "adulto")]
    edad: f32,
    #[serde(default)]
    cria_cd: f32,
    /// GRANJA: se cría a mano, no solo (ver `DeGranja`).
    #[serde(default)]
    granja: bool,
    /// Vida guardada (0 = sin dato → llena).
    #[serde(default)]
    vida: f32,
    #[serde(default)]
    domado: bool,
    #[serde(default)]
    sentado: bool,
    /// El nombre de la etiqueta.
    #[serde(default)]
    nombre: Option<String>,
}

fn adulto() -> f32 {
    1.0
}

#[derive(Serialize, Deserialize, Default)]
struct VivariumSave {
    residents: Vec<ResidentRecord>,
    next_uid: u64,
}

/// Estado del vivario: pool de ids/censo de residentes VIVOS (cuerpo actual) + los
/// registros persistentes de los OTROS cuerpos (`records`) + identidad monotónica.
#[derive(Resource)]
struct VivariumWorld {
    free_ids: Vec<i32>,
    alive: usize,
    births: u32,
    next_uid: u64,
    /// Residentes de cuerpos NO instanciados (el cuerpo activo vive como entidades).
    records: Vec<ResidentRecord>,
    loaded: bool,
    active_body: Option<u8>,
    /// Temperatura evolutiva adaptativa (F3): 1.0 = estable, sube si baja la diversidad.
    temperature: f32,
}

impl Default for VivariumWorld {
    fn default() -> Self {
        Self {
            free_ids: (RES_ID_BASE..RES_ID_BASE + MAX_RESIDENTS as i32).rev().collect(),
            alive: 0,
            births: 0,
            next_uid: 1,
            records: Vec::new(),
            loaded: false,
            active_body: None,
            temperature: TEMP_MIN,
        }
    }
}

fn vivarium_dir() -> PathBuf {
    crate::inventory_persist::persist_dir().join("vivarium")
}

fn vivarium_json() -> PathBuf {
    vivarium_dir().join("residents.json")
}

fn genome_path(uid: u64) -> PathBuf {
    vivarium_dir().join(format!("g_{uid}.adn"))
}

fn load_vivarium() -> (Vec<ResidentRecord>, u64) {
    let Ok(raw) = std::fs::read_to_string(vivarium_json()) else {
        return (Vec::new(), 1);
    };
    match serde_json::from_str::<VivariumSave>(&raw) {
        Ok(s) => (s.residents, s.next_uid.max(1)),
        Err(_) => (Vec::new(), 1),
    }
}

fn save_vivarium(records: &[ResidentRecord], next_uid: u64) {
    let _ = std::fs::create_dir_all(vivarium_dir());
    let save = VivariumSave { residents: records.to_vec(), next_uid };
    if let Ok(json) = serde_json::to_string(&save) {
        let _ = std::fs::write(vivarium_json(), json);
    }
}

/// Definición de una especie (visual + conducta base). El arte es un slot lógico
/// (`npc/...`) con fallback a quad tintado. F5: se genera por PLANETA (determinista).
#[derive(Clone)]
struct SpeciesDef {
    key: String,
    color: [f32; 3],
    size: Vec2,
    hostile: bool,      // sesga la conducta: acecha/acosa al jugador
    relays_alarm: bool, // como el perro terrestre: propaga la alarma a la manada
    label: String,      // ASCII (debug)
}

/// Temperamento del planeta → tiñe su ecosistema (F5). No sabés qué vas a encontrar; un
/// mundo hostil te obliga a ir preparado (rompe la monotonía, la exploración PESA).
#[derive(Clone, Copy, PartialEq)]
enum Temperament {
    Friendly,
    Neutral,
    Eerie,
    Hostile,
}

fn body_temperament(body: CelestialBody) -> Temperament {
    match body {
        CelestialBody::Earth => Temperament::Friendly,
        CelestialBody::Moon => Temperament::Eerie,
        _ => match (body as u32).wrapping_mul(2654435761) % 4 {
            0 => Temperament::Friendly,
            1 => Temperament::Neutral,
            2 => Temperament::Eerie,
            _ => Temperament::Hostile,
        },
    }
}

/// Fauna terrestre (amigable). El arte sale de `scripts/fauna3d.py`: cada animal es un
/// modelo 3D renderizado a pixel art, con poses `idle/walk/run/eat[/bark]_{dir}`.
/// **`size` es el CUADRO de esa hoja en metros** (lo imprime el script, a 33,7 px/m):
/// si no coincide, el sprite se estira y el pixel deja de ser cuadrado.
fn earth_species() -> Vec<SpeciesDef> {
    let raw: &[(&str, [f32; 3], Vec2, &str)] = &[
        ("npc/fauna_cow", [0.85, 0.82, 0.78], Vec2::new(1.96, 1.75), "Vaca"),
        ("npc/fauna_chicken", [0.90, 0.88, 0.72], Vec2::new(0.65, 0.71), "Gallina"),
        ("npc/fauna_sheep", [0.88, 0.86, 0.82], Vec2::new(1.37, 1.40), "Oveja"),
        ("npc/fauna_pig", [0.82, 0.60, 0.58], Vec2::new(1.48, 1.31), "Cerdo"),
        ("npc/fauna_dog", [0.60, 0.48, 0.34], Vec2::new(1.37, 1.28), "Perro"),
        ("npc/fauna_goat", [0.90, 0.88, 0.84], Vec2::new(1.37, 1.48), "Cabra"),
        ("npc/fauna_donkey", [0.55, 0.53, 0.55], Vec2::new(1.78, 1.84), "Burro"),
        ("npc/fauna_rabbit", [0.71, 0.59, 0.46], Vec2::new(1.13, 1.04), "Conejo"),
        ("npc/fauna_duck", [0.94, 0.93, 0.90], Vec2::new(0.71, 0.71), "Pato"),
    ];
    raw.iter()
        .enumerate()
        .map(|(i, (k, c, s, l))| SpeciesDef {
            key: k.to_string(),
            color: *c,
            size: *s,
            hostile: false,
            relays_alarm: i == 4, // el perro
            label: l.to_string(),
        })
        .collect()
}

/// Inventa la fauna de un cuerpo NO-terrestre según su temperamento (determinista → misma
/// fauna cada visita). Sin arte asignado → quad tintado; la agresividad sale del planeta.
fn planet_species(body: CelestialBody) -> Vec<SpeciesDef> {
    if body == CelestialBody::Earth {
        return earth_species();
    }
    let temp = body_temperament(body);
    let seed = (body as u32).wrapping_mul(2654435761).wrapping_add(0x9E37_79B9);
    let n = 3 + (seed >> 7) % 3; // 3..5 especies
    (0..n).map(|i| invent_species(body, temp, i, seed)).collect()
}

fn invent_species(body: CelestialBody, temp: Temperament, i: u32, seed: u32) -> SpeciesDef {
    let h = seed.wrapping_add(i.wrapping_mul(0x85EB_CA6B));
    let f = |shift: u32| ((h >> shift) & 0xFF) as f32 / 255.0;
    let (color, hostile, relays) = match temp {
        Temperament::Friendly => (
            [0.55 + f(0) * 0.4, 0.60 + f(4) * 0.35, 0.40 + f(8) * 0.4],
            false,
            false,
        ),
        Temperament::Neutral => (
            [0.50 + f(0) * 0.3, 0.50 + f(4) * 0.3, 0.50 + f(8) * 0.3],
            i % 3 == 0,
            false,
        ),
        Temperament::Eerie => (
            [0.25 + f(0) * 0.25, 0.20 + f(4) * 0.25, 0.35 + f(8) * 0.35],
            i % 2 == 0,
            i % 2 == 1,
        ),
        Temperament::Hostile => (
            [0.50 + f(0) * 0.4, 0.12 + f(4) * 0.2, 0.12 + f(8) * 0.2],
            true,
            true,
        ),
    };
    let sz = 0.7 + f(12) * 1.6;
    let asp = 0.8 + f(16) * 0.5;
    let prefix = match temp {
        Temperament::Friendly => "Criatura",
        Temperament::Neutral => "Bestia",
        Temperament::Eerie => "Ente",
        Temperament::Hostile => "Depredador",
    };
    SpeciesDef {
        key: format!("npc/fauna_b{}_{i}", body as u8),
        color,
        size: Vec2::new(sz * asp, sz),
        hostile,
        relays_alarm: relays,
        label: format!("{prefix} {}", i + 1),
    }
}

/// Catálogo de fauna por cuerpo (cache perezoso). El ambiental y el spawn creativo lo
/// consultan según el planeta donde estás.
#[derive(Resource, Default)]
struct FaunaCatalog {
    by_body: HashMap<u8, Vec<SpeciesDef>>,
}

impl FaunaCatalog {
    fn get(&mut self, body: CelestialBody) -> &[SpeciesDef] {
        self.by_body
            .entry(body as u8)
            .or_insert_with(|| planet_species(body))
    }
}

#[derive(Component)]
pub(crate) struct Animal {
    brain: i32,
    heading: f32,
    x: f32,
    z: f32,
    species: usize,
    /// Región de ecología donde está contado. Al cruzar el borde, el censo se MUEVE de
    /// región (no se teletransporta el animal): así una manada emigra de verdad.
    region: (i32, i32),
    /// Su lugar en la cadena alimenticia. Sale de la especie del planeta (`hostile`).
    trophic: Trophic,
    /// Visual + conducta resueltos de la `SpeciesDef` del planeta al spawnear (evita
    /// consultar el catálogo en el loop caliente).
    color: [f32; 3],
    art_key: String,
    hostile: bool,
    relays_alarm: bool,
    /// Timers (s) para elegir la animación del pack: comer / beber.
    eat_t: f32,
    drink_t: f32,
    running: bool, // huyendo (alarma) → animación "run"
    /// GRANJA: segundos de celo que le quedan (alimentado con su comida).
    celo: f32,
    /// GRANJA: descanso tras criar; mientras dure no entra en celo.
    cria_cd: f32,
    /// 0 = recién nacido, 1 = adulto. Las crías se dibujan chicas y no crían.
    edad: f32,
    /// Nombre humano de la especie ("Vaca").
    nombre: String,
    /// GRANJA — vida y golpes.
    vida: f32,
    vida_max: f32,
    /// Destello rojo tras un golpe (s).
    herido_t: f32,
    /// Huye del jugador tras un golpe (s).
    panico_t: f32,
    /// Perro que golpeaste: te persigue y te muerde (el lobo de Minecraft).
    enojado: bool,
    mordida_cd: f32,
    /// Perro domado con un hueso: te sigue; `sentado` lo deja quieto.
    domado: bool,
    sentado: bool,
    /// El nombre que le pusiste con la etiqueta.
    nombre_propio: Option<String>,
    /// Corazones sin celo (domado, bautizado).
    corazones_t: f32,
    /// Cuenta regresiva al próximo huevo (gallina/pato de granja).
    huevo_t: f32,
}

/// Psicología: necesidades que sesgan la conducta y gatillan reproducción/muerte.
#[derive(Component)]
struct Needs {
    hunger: f32,
    thirst: f32,
    energy: f32,
    /// La cabeza tipo Sims: decide QUÉ hacer (comer, beber, dormir, manada, explorar) entre lo
    /// que el mundo ofrece según cuál barra aprieta. El IAFZ sólo le da textura al andar.
    vida: Vida,
    /// Último charco visto y cuándo se lo volvió a buscar (buscar agua no es gratis).
    agua: Option<(f32, f32)>,
    agua_cd: f32,
    bump: f32,
}

impl Needs {
    fn nuevo(hunger: f32, thirst: f32, energy: f32, semilla: u64) -> Self {
        Self { hunger, thirst, energy, vida: Vida::nueva(semilla), agua: None, agua_cd: 0.0, bump: 0.0 }
    }
}

/// Caché de la pose aplicada (evita reconstruir el sprite cada frame).
#[derive(Component, Default)]
struct FaunaPose(String);

#[derive(Component)]
struct FoodDot {
    x: f32,
    z: f32,
    /// Región que lo sostiene (`None` = feeder colocado a mano, que no sale de ninguna
    /// biomasa y por eso no se agota).
    region: Option<(i32, i32)>,
    /// Índice del ancla determinista dentro de la región (para no duplicar el mismo mato).
    slot: u32,
}

/// Pool de ids de cerebro (C global) + censo. Reciclamos ids al morir.
#[derive(Resource)]
struct FaunaWorld {
    free_ids: Vec<i32>,
    alive: usize,
}

impl Default for FaunaWorld {
    fn default() -> Self {
        Self { free_ids: (0..MAX_FAUNA_ID).rev().collect(), alive: 0 }
    }
}

// --- ECOLOGÍA DE MUNDO (reemplazo del anillo) --------------------------------------
//
// La población vive como NÚMERO por región y por cuerpo; las entidades son sólo la vista
// cercana. `ha_shared::ecology` tiene el modelo (determinista y testeado); acá está el
// pegamento con Bevy: persistencia, paso periódico y materialización.

/// Una región conocida: su capacidad (fija, sale del bioma) y su población (lo que pasó).
#[derive(Clone, Copy, Serialize, Deserialize)]
struct EcoCell {
    pop: Population,
    #[serde(skip)]
    cap: Option<Capacity>,
}

#[derive(Resource, Default)]
struct EcologyWorld {
    /// `(cuerpo, rx, rz)` → estado. Lo que NO está acá vale su equilibrio: una región no
    /// visitada ya estaba viva, con la vida que su bioma sostiene.
    cells: HashMap<(u8, i32, i32), EcoCell>,
    /// Regiones MATERIALIZADAS ahora mismo (ahí manda la entidad, no la aritmética).
    live: HashSet<(u8, i32, i32)>,
    loaded: bool,
    save_acc: f32,
    /// Regiones cuya capacidad todavía no se calculó. **La capacidad es cara** (9
    /// muestras de bioma, y cada una pasa por relieve y ríos) y el guardado trae hasta
    /// `MAX_KNOWN_REGIONS` regiones SIN ella: calcularlas todas en el primer tick
    /// congelaba el juego a los 4 s de empezar a jugar (medido el 27 sep 2026 con gdb:
    /// `EcologyWorld::cap → carrying_capacity → biome_calc → rios::trazar_brazo`, 3.001
    /// regiones de golpe). Ahora se calculan en esta cola, con plazo por frame
    /// ([`PLAZO_CAPACIDAD`]); lo que no está listo espera, no traba.
    pendientes: std::collections::VecDeque<(u8, i32, i32)>,
    en_cola: HashSet<(u8, i32, i32)>,
}

/// Tiempo por frame para calcular capacidades pendientes. La unidad de trabajo es UNA
/// región (9 muestras), así que el frame paga a lo sumo esto más una región — la
/// lección de §10: un plazo sólo acota si la unidad entre miradas al reloj es chica.
const PLAZO_CAPACIDAD: std::time::Duration = std::time::Duration::from_millis(2);

impl EcologyWorld {
    fn cap(&mut self, body: CelestialBody, rx: i32, rz: i32) -> Capacity {
        let key = (body as u8, rx, rz);
        if let Some(c) = self.cells.get(&key).and_then(|c| c.cap) {
            return c;
        }
        // En grueso: 9 muestras de bioma por región no necesitan el trazado fino de
        // los ríos (ver `ha_shared::grueso`), y con él una región costaba varios ms.
        let cap = ha_shared::grueso::con(|| ecology::carrying_capacity(body, rx, rz));
        let e = self.cells.entry(key).or_insert(EcoCell {
            pop: ecology::equilibrium(cap),
            cap: Some(cap),
        });
        e.cap = Some(cap);
        cap
    }

    /// La capacidad YA calculada, sin calcular nada.
    fn cap_hecha(&self, body: CelestialBody, rx: i32, rz: i32) -> Option<Capacity> {
        self.cells.get(&(body as u8, rx, rz)).and_then(|c| c.cap)
    }

    /// Encola el cálculo de una región (adelante si es urgente: la ventana del jugador).
    fn pedir(&mut self, key: (u8, i32, i32), urgente: bool) {
        if self.cells.get(&key).is_some_and(|c| c.cap.is_some()) {
            return;
        }
        if self.en_cola.insert(key) {
            if urgente {
                self.pendientes.push_front(key);
            } else {
                self.pendientes.push_back(key);
            }
        } else if urgente {
            // ya estaba al fondo: se adelanta
            if let Some(i) = self.pendientes.iter().position(|k| *k == key) {
                self.pendientes.remove(i);
                self.pendientes.push_front(key);
            }
        }
    }

    /// Calcula capacidades pendientes hasta agotar el plazo.
    fn trabajar_cola(&mut self, plazo: std::time::Duration) {
        let reloj = std::time::Instant::now();
        while let Some(key) = self.pendientes.pop_front() {
            self.en_cola.remove(&key);
            if let Some(body) = crate::world2d::body_from_u8(key.0) {
                self.cap(body, key.1, key.2);
            }
            if reloj.elapsed() >= plazo {
                break;
            }
        }
    }

    /// Población de la región, creándola perezosamente en su equilibrio.
    fn pop(&mut self, body: CelestialBody, rx: i32, rz: i32) -> Population {
        self.cap(body, rx, rz);
        self.cells.get(&(body as u8, rx, rz)).map(|c| c.pop).unwrap_or_default()
    }

    fn set_pop(&mut self, body: CelestialBody, rx: i32, rz: i32, pop: Population) {
        self.cap(body, rx, rz);
        if let Some(c) = self.cells.get_mut(&(body as u8, rx, rz)) {
            c.pop = pop;
        }
    }

    /// Suma `d` cabezas al nivel trófico de una región (negativo = muertes). Es el ÚNICO
    /// camino por el que una entidad afecta al censo del mundo — así nunca se pierde ni se
    /// inventa un animal al materializar/desmaterializar.
    fn add_head(&mut self, body: CelestialBody, rx: i32, rz: i32, trophic: Trophic, d: f32) {
        let mut p = self.pop(body, rx, rz);
        match trophic {
            Trophic::Herbivore => p.herbivores = (p.herbivores + d).max(0.0),
            Trophic::Carnivore => p.carnivores = (p.carnivores + d).max(0.0),
        }
        self.set_pop(body, rx, rz, p);
    }

    fn eat_forage(&mut self, body: CelestialBody, rx: i32, rz: i32) {
        let mut p = self.pop(body, rx, rz);
        p.plants = (p.plants - FORAGE_UNIT).max(0.0);
        self.set_pop(body, rx, rz, p);
    }
}

fn ecology_json() -> PathBuf {
    vivarium_dir().join("ecology.json")
}

#[derive(Serialize, Deserialize, Default)]
struct EcologySave {
    /// `(cuerpo, rx, rz, población)` — la capacidad no se guarda: sale del terreno.
    cells: Vec<(u8, i32, i32, Population)>,
}

// --- ESTIGMERGIA (B4.4): campo de feromonas para comunicación EMERGENTE. ---
// Cada animal deposita señales (alarma / presencia) en su celda; los demás sienten
// el GRADIENTE (bits altos del sensor). Así emergen manadas, huidas colectivas,
// territorios y conflictos SIN scriptearlos — se da el canal, la evolución decide.
// El jugador se comunica con el mismo campo (comida atrae; futuro: amenaza = alarma).
const SIG_CELL: f32 = 2.0; // tamaño de celda de señal (m)
const SIG_DECAY: f32 = 0.75; // por segundo (feromona se evapora)
const SIG_SENSE_R: i32 = 4; // radio de sensado (celdas)

#[derive(Clone, Copy, Default)]
struct Signal {
    alarm: f32,
    presence: f32,
}

/// Grid disperso de feromonas (se poda al evaporarse). Barato: sólo celdas activas.
#[derive(Resource, Default)]
struct SignalField {
    grid: HashMap<(i32, i32), Signal>,
}

impl SignalField {
    fn cell(x: f32, z: f32) -> (i32, i32) {
        ((x / SIG_CELL).floor() as i32, (z / SIG_CELL).floor() as i32)
    }
    fn decay(&mut self, dt: f32) {
        let k = SIG_DECAY.powf(dt);
        self.grid.retain(|_, s| {
            s.alarm *= k;
            s.presence *= k;
            s.alarm + s.presence > 0.02
        });
    }
    fn deposit(&mut self, x: f32, z: f32, alarm: f32, presence: f32) {
        let e = self.grid.entry(Self::cell(x, z)).or_default();
        e.alarm += alarm;
        e.presence += presence;
    }
    /// Bits de sensor (14-18): dirección a la ALARMA más fuerte + alarma/aglomeración
    /// local. Se OR-ean al `sense()` sin tocar su firma (el gym no los ve).
    fn sense_bits(&self, x: f32, z: f32, heading: f32) -> u32 {
        let (cx, cz) = Self::cell(x, z);
        let mut best = (0.0f32, 0.0f32, 0.0f32); // alarm, dx, dz
        let mut local_alarm = 0.0;
        let mut local_presence = 0.0;
        for dz in -SIG_SENSE_R..=SIG_SENSE_R {
            for dx in -SIG_SENSE_R..=SIG_SENSE_R {
                if let Some(s) = self.grid.get(&(cx + dx, cz + dz)) {
                    if dx == 0 && dz == 0 {
                        local_alarm = s.alarm;
                        local_presence = s.presence;
                    }
                    if s.alarm > best.0 {
                        best = (s.alarm, dx as f32, dz as f32);
                    }
                }
            }
        }
        let mut bits = 0u32;
        if best.0 > 0.1 {
            let ang = best.2.atan2(best.1) - heading;
            let sector = (((ang.rem_euclid(std::f32::consts::TAU)) / (std::f32::consts::TAU / 8.0))
                as u32)
                & 7;
            bits |= sector << 14; // bits 14-16: dir a la alarma
        }
        if local_alarm > 0.4 {
            bits |= 1 << 17; // peligro aquí
        }
        if local_presence > 1.5 {
            bits |= 1 << 18; // aglomeración (manada/territorio)
        }
        bits
    }
}

pub struct FaunaPlugin;

impl Plugin for FaunaPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(bevy::prelude::OnExit(crate::game_phase::GamePhase::Playing), (crate::salir::reiniciar::<FaunaWorld>, crate::salir::reiniciar::<VivariumWorld>, crate::salir::reiniciar::<EcologyWorld>, crate::salir::reiniciar::<SignalField>));
        app.init_resource::<FaunaWorld>()
            .init_resource::<VivariumWorld>()
            .init_resource::<FaunaCatalog>()
            .init_resource::<SignalField>()
            .init_resource::<EcologyWorld>()
            .add_event::<SpawnResidentEvent>()
            .add_event::<SpawnFoodEvent>()
            .add_plugins(GranjaAnimalesPlugin)
            .add_systems(
                Update,
                (
                    manage_vivarium,
                    tick_ecology,
                    stream_regions,
                    spawn_residents,
                    spawn_placed_food,
                    tick_fauna,
                    cull_fauna,
                    vivarium_temperature,
                    save_vivarium_periodic,
                    save_ecology_periodic,
                )
                    .chain()
                    .run_if(crate::game_phase::world_active),
            )
            .add_systems(Startup, spawn_vivarium_overlay)
            .add_systems(Update, (update_vivarium_overlay, update_fauna_sightings));
    }
}

fn scatter(seed: u32, i: usize, radius: f32) -> (f32, f32) {
    let a = seed.wrapping_add(i as u32).wrapping_mul(2654435761);
    let ang = ((a >> 8) % 6283) as f32 / 1000.0;
    let r = radius * (0.35 + 0.65 * ((a % 997) as f32 / 997.0));
    (ang.cos() * r, ang.sin() * r)
}

/// Paso de ECOLOGÍA: avanza la cadena alimenticia de TODAS las regiones conocidas, de
/// todos los cuerpos, estés o no parado ahí. Es aritmética sobre un `HashMap` chico
/// (`MAX_KNOWN_REGIONS`), no entidades: por eso el mundo puede seguir vivo sin costar FPS.
///
/// Las regiones MATERIALIZADAS sólo avanzan su vegetación — su fauna la están simulando
/// las entidades de verdad, y contarla dos veces sería inventar animales.
fn tick_ecology(
    time: Res<Time>,
    region: Res<RegionRuntime>,
    ctx: Res<WorldContextRuntime>,
    planet: Res<ActivePlanet>,
    mut eco: ResMut<EcologyWorld>,
    player: Query<&Transform, With<Player>>,
    mut acc: Local<f32>,
) {
    if !eco.loaded {
        load_ecology(&mut eco);
    }
    eco.trabajar_cola(PLAZO_CAPACIDAD);
    *acc += time.delta_secs();
    if *acc < ECO_TICK {
        return;
    }
    let dt = *acc;
    *acc = 0.0;

    // Las regiones alrededor tuyo se traen a memoria (con su equilibrio si es la primera
    // vez): la migración necesita vecinos para que la manada tenga adónde irse.
    let mut here = None;
    if surface_region_active(&region).is_some() {
        if let Ok(pt) = player.get_single() {
            let body = active_surface_body(&ctx, &planet);
            let (rx, rz) = ecology::region_of(pt.translation.x, -pt.translation.y);
            for dz in -AWARE_RINGS..=AWARE_RINGS {
                for dx in -AWARE_RINGS..=AWARE_RINGS {
                    eco.pedir((body as u8, rx + dx, rz + dz), dx.abs().max(dz.abs()) <= SIM_RINGS);
                }
            }
            here = Some((body as u8, rx, rz));
        }
    }

    // 1) Paso propio de cada región.
    let keys: Vec<(u8, i32, i32)> = eco.cells.keys().copied().collect();
    for key in &keys {
        let Some(body) = crate::world2d::body_from_u8(key.0) else { continue };
        // sin capacidad calculada (recién cargada del guardado): a la cola, y este
        // paso se la saltea — lejos del jugador, esperar un tick no se nota
        let Some(cap) = eco.cap_hecha(body, key.1, key.2) else {
            eco.pedir(*key, false);
            continue;
        };
        let live = eco.live.contains(key);
        if let Some(c) = eco.cells.get_mut(key) {
            c.pop = if live {
                ecology::step_flora(c.pop, cap, dt)
            } else {
                ecology::step(c.pop, cap, dt)
            };
        }
    }

    // 2) Difusión hacia el vecino de la derecha y el de abajo (cada par, una vez). Las
    //    regiones materializadas se saltan: ahí la migración la hacen las patas.
    for key in &keys {
        let Some(body) = crate::world2d::body_from_u8(key.0) else { continue };
        for (dx, dz) in [(1, 0), (0, 1)] {
            let other = (key.0, key.1 + dx, key.2 + dz);
            if !eco.cells.contains_key(&other) {
                continue;
            }
            // Entre DOS regiones materializadas no se difunde: ahí la migración la hacen
            // las patas (`tick_fauna` muda el censo al cruzar el borde) y sumar las dos
            // cosas contaría el mismo viaje dos veces. Pero de fuera HACIA DENTRO sí: si
            // no, el prado donde te plantás se vacía solo según los bichos se dispersan y
            // nada vuelve a entrar — el jugador quieto veía morir el bosque.
            if eco.live.contains(key) && eco.live.contains(&other) {
                continue;
            }
            let (Some(ca), Some(cb)) = (eco.cap_hecha(body, key.1, key.2), eco.cap_hecha(body, other.1, other.2))
            else {
                continue;
            };
            let (pa, pb) = (eco.cells[key].pop, eco.cells[&other].pop);
            let (na, nb) = ecology::migrate(pa, ca, pb, cb, dt);
            eco.cells.get_mut(key).unwrap().pop = na;
            eco.cells.get_mut(&other).unwrap().pop = nb;
        }
    }

    // 3) Poda: lo que volvió a su equilibrio y está lejos vale exactamente el valor
    //    perezoso, así que guardarlo no aporta nada. Memoria acotada para siempre.
    if eco.cells.len() > MAX_KNOWN_REGIONS {
        let live: HashSet<(u8, i32, i32)> = eco.live.clone();
        let mut drop_keys: Vec<(u8, i32, i32)> = Vec::new();
        for key in &keys {
            if live.contains(key) {
                continue;
            }
            if let Some((hb, hx, hz)) = here {
                if key.0 == hb && (key.1 - hx).abs() <= AWARE_RINGS && (key.2 - hz).abs() <= AWARE_RINGS {
                    continue;
                }
            }
            let Some(body) = crate::world2d::body_from_u8(key.0) else { continue };
            let Some(cap) = eco.cap_hecha(body, key.1, key.2) else { continue };
            if ecology::is_settled(eco.cells[key].pop, cap) {
                drop_keys.push(*key);
            }
        }
        for k in drop_keys {
            eco.cells.remove(&k);
            if eco.cells.len() <= MAX_KNOWN_REGIONS {
                break;
            }
        }
    }
}

/// MATERIALIZACIÓN (el reemplazo del anillo): las regiones de la ventana `SIM_RINGS` se
/// convierten en entidades reales — tantos animales como dice su población y tantos matos
/// de forraje como biomasa dibujable tenga. Las que salen de la ventana se devuelven a
/// número (sin morir: siguen vivas en su región).
///
/// Ningún animal se coloca "alrededor tuyo": van a los CLAROS deterministas de su región
/// (`ecology::region_point`), que son los mismos siempre. Si la región cercana no sostiene
/// fauna, no hay fauna — eso es un desierto, y está bien que lo sea.
#[allow(clippy::too_many_arguments, clippy::type_complexity)]
fn stream_regions(
    mut commands: Commands,
    region: Res<RegionRuntime>,
    ctx: Res<WorldContextRuntime>,
    planet: Res<ActivePlanet>,
    tiles: Res<TileWorld>,
    art: Res<crate::art2d::ArtLibrary>,
    mut world: ResMut<FaunaWorld>,
    mut eco: ResMut<EcologyWorld>,
    mut catalog: ResMut<FaunaCatalog>,
    player: Query<&Transform, With<Player>>,
    animals: Query<(Entity, &Animal), Without<Resident>>,
    foods: Query<(Entity, &FoodDot), Without<PlacedFood>>,
) {
    if surface_region_active(&region).is_none() {
        for (e, _) in &animals {
            commands.entity(e).despawn_recursive();
        }
        for (e, _) in &foods {
            commands.entity(e).despawn_recursive();
        }
        *world = FaunaWorld::default(); // pool ambiental (los residentes los maneja manage_vivarium)
        eco.live.clear();
        return;
    }
    let Ok(pt) = player.get_single() else {
        return;
    };
    let body = active_surface_body(&ctx, &planet);
    let defs = catalog.get(body).to_vec();
    if defs.is_empty() {
        return;
    }
    let (prx, prz) = ecology::region_of(pt.translation.x, -pt.translation.y);

    // Ventana deseada, de la más cercana a la más lejana (el tope de entidades se gasta
    // primero donde el jugador realmente mira).
    let mut want: Vec<(i32, i32)> = Vec::new();
    for dz in -SIM_RINGS..=SIM_RINGS {
        for dx in -SIM_RINGS..=SIM_RINGS {
            want.push((prx + dx, prz + dz));
        }
    }
    want.sort_by_key(|&(x, z)| (x - prx).abs() + (z - prz).abs());
    // La ventana se materializa cuando TODAS sus regiones tienen capacidad: si alguna
    // falta, se pide al frente de la cola y se espera (los animales que ya están
    // siguen; nada se congela calculando nueve regiones de golpe).
    let mut falta = false;
    for &(x, z) in &want {
        if eco.cap_hecha(body, x, z).is_none() {
            eco.pedir((body as u8, x, z), true);
            falta = true;
        }
    }
    if falta {
        eco.trabajar_cola(PLAZO_CAPACIDAD);
        return;
    }
    let want_set: HashSet<(i32, i32)> = want.iter().copied().collect();

    // --- Desmaterializar lo que quedó fuera (o de otro cuerpo): vuelve a ser número. ---
    let mut live_count: HashMap<(i32, i32), (usize, usize)> = HashMap::new(); // (herb, carn)
    for (e, a) in &animals {
        if want_set.contains(&a.region) {
            let c = live_count.entry(a.region).or_default();
            match a.trophic {
                Trophic::Herbivore => c.0 += 1,
                Trophic::Carnivore => c.1 += 1,
            }
        } else {
            commands.entity(e).despawn_recursive();
            world.free_ids.push(a.brain);
            world.alive = world.alive.saturating_sub(1);
        }
    }
    let mut live_forage: HashMap<(i32, i32), HashSet<u32>> = HashMap::new();
    for (e, f) in &foods {
        match f.region {
            Some(r) if want_set.contains(&r) => {
                live_forage.entry(r).or_default().insert(f.slot);
            }
            _ => commands.entity(e).despawn_recursive(),
        }
    }
    eco.live = want.iter().map(|&(x, z)| (body as u8, x, z)).collect();

    // El DEPREDADOR tiene TERRITORIO: su capacidad por región es una fracción (un carnívoro
    // no cabe en 64 m de bosque) y redondearla región a región lo borraría del mundo. Se
    // cuenta sobre la VENTANA entera y se coloca uno por región, en las que su bioma lo
    // admite. El herbívoro sí se cuenta por región: pasta donde hay pasto.
    let carn_total: f32 = want.iter().map(|&(x, z)| eco.pop(body, x, z).carnivores).sum();
    let carn_live: usize = live_count.values().map(|c| c.1).sum();
    let mut carn_budget = (carn_total.floor() as usize).saturating_sub(carn_live);

    // --- Materializar lo que falta, respetando el tope global de entidades. ---
    //
    // Dos pasadas, y el orden importa: los DEPREDADORES primero, en TODA la ventana. Son
    // pocos y son la mitad interesante de la cadena; si van detrás, las decenas de
    // herbívoros de la región más cercana se comen el presupuesto de entidades y no se ve
    // un solo cazador aunque el mundo tenga cinco (pasó, medido con `HA_ECOTEST`).
    let spawn_one = |commands: &mut Commands,
                         world: &mut FaunaWorld,
                         rx: i32,
                         rz: i32,
                         i: usize,
                         trophic: Trophic| {
        if world.alive >= MAX_LIVE {
            return false;
        }
        let hostile = trophic == Trophic::Carnivore;
        // La especie sale del planeta (F5) y del nivel trófico: los depredadores del
        // cuerpo si los hay, si no el catálogo entero teñido de hostil.
        let idx = pick_species(&defs, hostile, rx, rz, i as u32);
        let mut def = defs[idx].clone();
        def.hostile = hostile;
        let slot = herd_slot(trophic, i as u32);
        let Some((x, z)) = free_spot(&tiles, body, rx, rz, slot) else { return false };
        let Some(brain) = world.free_ids.pop() else { return false };
        iafz_sys::load_bytes(brain, GENOME);
        iafz_sys::reset_echo(brain);
        world.alive += 1;
        let heading = (brain as f32) * 2.399963;
        spawn_animal(commands, &art, brain, x, z, heading, 0.8, idx, &def, (rx, rz), trophic, 1.0);
        true
    };

    for &(rx, rz) in &want {
        if carn_budget == 0 {
            break;
        }
        let have = live_count.get(&(rx, rz)).copied().unwrap_or((0, 0));
        // Un depredador por región como mucho, y sólo donde su bioma lo sostiene.
        if have.1 > 0 || eco.pop(body, rx, rz).carnivores <= 0.03 {
            continue;
        }
        if spawn_one(&mut commands, &mut world, rx, rz, 0, Trophic::Carnivore) {
            carn_budget -= 1;
        }
    }

    for (rx, rz) in want {
        let pop = eco.pop(body, rx, rz);
        let have = live_count.get(&(rx, rz)).copied().unwrap_or((0, 0));
        // Tantas cabezas como dice el censo (la parte fraccionaria es la cría que todavía
        // no nació — nunca se redondea hacia arriba, o el mundo inventaría vida).
        for i in have.0..pop.herbivores.floor() as usize {
            if !spawn_one(&mut commands, &mut world, rx, rz, i, Trophic::Herbivore)
                && world.alive >= MAX_LIVE
            {
                break;
            }
        }

        // Forraje: la biomasa de la región, dibujada. Se come → baja la biomasa → tarda en
        // rebrotar (y rebrota en el MISMO claro, que es cómo se aprende un terreno).
        let shown = (pop.plants / FORAGE_UNIT).floor() as usize;
        let target = shown.min(MAX_FORAGE_REGION);
        let live = live_forage.entry((rx, rz)).or_default();
        for slot in 0..target as u32 {
            if live.contains(&slot) {
                continue;
            }
            let Some((x, z)) = free_spot(&tiles, body, rx, rz, 100 + slot) else { continue };
            commands.spawn((
                FoodDot { x, z, region: Some((rx, rz)), slot },
                Sprite::from_color(Color::srgb(0.85, 0.25, 0.35), Vec2::splat(0.5)),
                // Marca pintada en el suelo: banda de piso, nunca tapa a un animal.
                Transform::from_xyz(x, -z, crate::layers::FLOOR),
            ));
        }
    }
}

/// El nivel trófico de una especie: `hostile` es el depredador de la cadena.
fn trophic_of(def: &SpeciesDef) -> Trophic {
    if def.hostile {
        Trophic::Carnivore
    } else {
        Trophic::Herbivore
    }
}

/// Ancla determinista del individuo `i` de un nivel trófico dentro de su región. Los
/// herbívoros comparten pocos claros (salen en MANADA); el depredador va aparte.
fn herd_slot(trophic: Trophic, i: u32) -> u32 {
    match trophic {
        Trophic::Herbivore => 200 + i,
        Trophic::Carnivore => 300 + i,
    }
}

/// Primer punto CAMINABLE cerca del ancla `slot` de la región (prueba unas pocas
/// alternativas deterministas). `None` = la región está tapada ahí; se materializa menos
/// fauna, que es preferible a incrustar un animal dentro de una roca.
fn free_spot(
    tiles: &TileWorld,
    body: CelestialBody,
    rx: i32,
    rz: i32,
    slot: u32,
) -> Option<(f32, f32)> {
    for k in 0..4u32 {
        let (x, z) = ecology::region_point(body, rx, rz, slot.wrapping_add(k * 7919));
        if !tiles.solid_at(body, x.floor() as i64, z.floor() as i64) {
            return Some((x, z));
        }
    }
    None
}

/// Elige la especie del planeta que toca a este individuo. Si el cuerpo tiene especies
/// depredadoras propias, el carnívoro sale de ésas; si no, se tiñe una de hostil (los
/// mundos amables también tienen quien cace — la cadena no puede quedar coja).
fn pick_species(defs: &[SpeciesDef], hostile: bool, rx: i32, rz: i32, i: u32) -> usize {
    let pool: Vec<usize> = defs
        .iter()
        .enumerate()
        .filter(|(_, d)| d.hostile == hostile)
        .map(|(i, _)| i)
        .collect();
    let h = (rx as u32)
        .wrapping_mul(0x9E37_79B9)
        .wrapping_add((rz as u32).wrapping_mul(0x85EB_CA6B))
        .wrapping_add(i);
    if pool.is_empty() {
        (h as usize) % defs.len()
    } else {
        pool[(h as usize) % pool.len()]
    }
}

// --- Persistencia de la ecología ---------------------------------------------------

fn load_ecology(eco: &mut EcologyWorld) {
    eco.loaded = true;
    let Ok(raw) = std::fs::read_to_string(ecology_json()) else {
        return;
    };
    let Ok(save) = serde_json::from_str::<EcologySave>(&raw) else {
        return;
    };
    for (b, rx, rz, pop) in save.cells {
        eco.cells.insert((b, rx, rz), EcoCell { pop, cap: None });
    }
}

/// Vuelca la ecología a disco cada `VIV_SAVE_EVERY` y al pausar la app — lo que cazaste (y
/// lo que se recuperó solo) sobrevive a cerrar el juego.
fn save_ecology_periodic(
    time: Res<Time>,
    mut eco: ResMut<EcologyWorld>,
    mut was_paused: Local<bool>,
) {
    if !eco.loaded {
        return;
    }
    let paused = is_app_paused();
    let flush_now = paused && !*was_paused;
    *was_paused = paused;
    eco.save_acc += time.delta_secs();
    if !flush_now && eco.save_acc < VIV_SAVE_EVERY {
        return;
    }
    eco.save_acc = 0.0;
    let save = EcologySave {
        cells: eco.cells.iter().map(|(&(b, x, z), c)| (b, x, z, c.pop)).collect(),
    };
    let _ = std::fs::create_dir_all(vivarium_dir());
    if let Ok(json) = serde_json::to_string(&save) {
        let _ = std::fs::write(ecology_json(), json);
    }
}

/// VIVARIO (F1): consume los pedidos de spawn en creativo → coloca un residente por
/// evento con un id de cerebro del pool propio, cargado con el SEED del gym (las BASES
/// que se enseñaron). No se streamea (`Resident`); confinado por paredes/puertas del
/// terreno. Respeta `MAX_RESIDENTS`. Ver `PLAN_VIVARIO_VIDA_EMERGENTE.md`.
#[allow(clippy::too_many_arguments)]
fn spawn_residents(
    mut commands: Commands,
    art: Res<crate::art2d::ArtLibrary>,
    region: Res<RegionRuntime>,
    ctx: Res<WorldContextRuntime>,
    planet: Res<ActivePlanet>,
    mut vivarium: ResMut<VivariumWorld>,
    mut catalog: ResMut<FaunaCatalog>,
    mut events: EventReader<SpawnResidentEvent>,
) {
    if surface_region_active(&region).is_none() {
        events.clear();
        return;
    }
    // F5: el slot de la paleta resuelve contra la fauna del planeta donde estás.
    let body = active_surface_body(&ctx, &planet);
    let defs = catalog.get(body).to_vec();
    for ev in events.read() {
        if vivarium.alive >= MAX_RESIDENTS || ev.species >= defs.len() {
            continue;
        }
        let Some(brain) = vivarium.free_ids.pop() else { break };
        iafz_sys::load_bytes(brain, GENOME);
        iafz_sys::reset_echo(brain);
        vivarium.alive += 1;
        let uid = vivarium.next_uid;
        vivarium.next_uid = vivarium.next_uid.wrapping_add(1);
        let heading = (brain as f32) * 2.399963;
        // F8: depredador = residente forzado hostil (acecha al jugador/fauna).
        let mut def = defs[ev.species].clone();
        if ev.hostile {
            def.hostile = true;
        }
        let e = spawn_animal(
            &mut commands,
            &art,
            brain,
            ev.x,
            ev.z,
            heading,
            0.85,
            ev.species,
            &def,
            ecology::region_of(ev.x, ev.z),
            trophic_of(&def),
            1.0,
        );
        commands.entity(e).try_insert(Resident { uid });
    }
}

/// F8 — coloca un FEEDER de comida persistente (`PlacedFood`) en la celda apuntada. La
/// fauna lo come (baja el hambre) pero NO se agota ni se reubica → fuente permanente para
/// diseñar la presión de comida del hábitat (debug creativo). Ver PLAN_VIVARIO §F8.
fn spawn_placed_food(
    mut commands: Commands,
    region: Res<RegionRuntime>,
    mut events: EventReader<SpawnFoodEvent>,
) {
    if surface_region_active(&region).is_none() {
        events.clear();
        return;
    }
    for ev in events.read() {
        let r = render_xy(ev.x, ev.z);
        commands.spawn((
            FoodDot { x: ev.x, z: ev.z, region: None, slot: 0 },
            PlacedFood,
            Sprite::from_color(Color::srgb(0.35, 0.85, 0.35), Vec2::splat(0.6)),
            // Comida tirada al piso: banda de piso (antes era `sort_z − 0.1`, o
            // sea 100 m de desempate — funcionaba de casualidad cerca del origen).
            Transform::from_xyz(r.x, r.y, crate::layers::FLOOR),
        ));
    }
}

/// Dirección cardinal (4) del rumbo — mismo esquema que el jugador/colonos.
fn dir4(heading: f32) -> &'static str {
    let (dx, dz) = (heading.cos(), heading.sin());
    if dz.abs() >= dx.abs() {
        if dz > 0.0 {
            "down"
        } else {
            "up"
        }
    } else if dx < 0.0 {
        "left"
    } else {
        "right"
    }
}

/// Aplica una pose `verb_dir` al sprite (con espejo left/right y fallbacks verbo→
/// walk→idle→idle_down), cacheando para no reconstruir cada frame. Réplica del
/// patrón de `sim::face_player_sprite`, así el pack LimeZu (poses por dirección)
/// anima la fauna igual que al jugador.
fn apply_pose(
    commands: &mut Commands,
    entity: Entity,
    art: &crate::art2d::ArtLibrary,
    key: &str,
    wanted: &str,
    sprite: &mut Sprite,
    cache: &mut String,
) {
    let mut name = wanted.to_string();
    let mut flip = false;
    // Espejo left<->right si sólo se sliceó un lado.
    if !art.has_pose(key, &name) {
        if let Some(m) = name.strip_suffix("_left") {
            let alt = format!("{m}_right");
            if art.has_pose(key, &alt) {
                name = alt;
                flip = true;
            }
        } else if let Some(m) = name.strip_suffix("_right") {
            let alt = format!("{m}_left");
            if art.has_pose(key, &alt) {
                name = alt;
                flip = true;
            }
        }
    }
    // Fallback de verbo (eat/drink/run) → walk → idle en la misma dirección.
    if !art.has_pose(key, &name) {
        let dir = name.rsplit('_').next().unwrap_or("down").to_string();
        for v in ["walk", "idle"] {
            let alt = format!("{v}_{dir}");
            if art.has_pose(key, &alt) {
                name = alt;
                break;
            }
        }
    }
    if !art.has_pose(key, &name) {
        name = "idle_down".to_string();
    }
    let state = format!("{name}#{flip}");
    if *cache == state {
        return;
    }
    *cache = state;
    sprite.flip_x = flip;
    // Hoja propia de la pose (animal armado con un PNG por dirección/estado).
    if let Some(img) = art.pose_image(key, &name) {
        if sprite.image != img {
            sprite.image = img;
            sprite.texture_atlas = None;
        }
    }
    if let Some(([x, y, w, h], anim)) = art.pose(key, &name) {
        match anim {
            Some(anim) => {
                sprite.rect = None;
                crate::render::try_insert(commands, entity, "pose/fauna", anim);
            }
            None => {
                commands.entity(entity).remove::<crate::art2d::ArtAnimated>();
                sprite.texture_atlas = None;
                sprite.rect = Some(Rect::from_corners(
                    Vec2::new(x as f32, y as f32),
                    Vec2::new((x + w) as f32, (y + h) as f32),
                ));
            }
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn spawn_animal(
    commands: &mut Commands,
    art: &crate::art2d::ArtLibrary,
    brain: i32,
    x: f32,
    z: f32,
    heading: f32,
    energy: f32,
    species: usize,
    def: &SpeciesDef,
    region: (i32, i32),
    trophic: Trophic,
    edad: f32,
) -> Entity {
    let sprite = art.sprite_or_color(
        &def.key,
        Color::srgb(def.color[0], def.color[1], def.color[2]),
        def.size,
    );
    commands
        .spawn((
            Animal {
                brain,
                heading,
                x,
                z,
                species,
                region,
                trophic,
                color: def.color,
                art_key: def.key.clone(),
                hostile: def.hostile,
                relays_alarm: def.relays_alarm,
                eat_t: 0.0,
                drink_t: 0.0,
                running: false,
                celo: 0.0,
                cria_cd: 0.0,
                edad: edad.clamp(0.0, 1.0),
                nombre: def.label.clone(),
                vida: vida_de(&def.key),
                vida_max: vida_de(&def.key),
                herido_t: 0.0,
                panico_t: 0.0,
                enojado: false,
                mordida_cd: 0.0,
                domado: false,
                sentado: false,
                nombre_propio: None,
                corazones_t: 0.0,
                // Desfasado por cerebro: no ponen todas a la vez.
                huevo_t: HUEVO_MIN + (brain as f32 * 37.0) % (HUEVO_MAX - HUEVO_MIN),
            },
            Needs::nuevo(0.3, 0.3, energy, brain as u64 ^ (x.to_bits() as u64) << 20),
            FaunaPose::default(),
            sprite,
            Transform::from_xyz(x, -z, sort_z(z)).with_scale(Vec3::splat(escala_de_edad(edad))),
            // Se toca: con su comida en la mano se alimenta (cría), con espada se
            // faena. Lo resuelve `alimentar_animales`.
            crate::targeting::InteractTarget {
                kind: ha_shared::InteractTargetKind::Animal,
                id: brain as u32,
            },
            crate::structures::StructureName(def.label.clone()),
        ))
        .id()
}

#[allow(clippy::type_complexity, clippy::too_many_arguments)]
fn tick_fauna(
    time: Res<Time>,
    ctx: Res<WorldContextRuntime>,
    planet: Res<ActivePlanet>,
    tiles: Res<TileWorld>,
    walls: Res<crate::walls::BuiltWalls>,
    art: Res<crate::art2d::ArtLibrary>,
    mut commands: Commands,
    mut world: ResMut<FaunaWorld>,
    mut vivarium: ResMut<VivariumWorld>,
    mut catalog: ResMut<FaunaCatalog>,
    mut field: ResMut<SignalField>,
    mut eco: ResMut<EcologyWorld>,
    player: Query<&Transform, (With<Player>, Without<Animal>, Without<FoodDot>)>,
    mut animals: Query<(Entity, &mut Animal, &mut Needs, &mut Transform, &mut Sprite, &mut FaunaPose, Option<&Resident>, Has<DeGranja>), (Without<FoodDot>, Without<Player>)>,
    foods: Query<(Entity, &FoodDot, Option<&PlacedFood>), (Without<Animal>, Without<Player>)>,
    (mut tirados, mut jugador_sim, mut acc, day, mut ads): (
        ResMut<crate::tirados::Tirados>,
        Query<&mut crate::sim::SimState, With<Player>>,
        Local<f32>,
        Res<crate::sky2d::DayClock>,
        Local<Vec<Anuncio>>,
    ),
) {
    let _probe = crate::debug::probe("tick_fauna");
    // BATERÍA: el cerebro de la fauna corre a ~15 Hz (no 60), acumulando el dt →
    // misma velocidad, 4× menos `think`/movimiento. En pausa el dt es 0 (Time
    // Virtual pausado) → no acumula → no corre → cero drenaje en segundo plano.
    const AI_HZ: f32 = 15.0;
    *acc += time.delta_secs();
    if *acc < 1.0 / AI_HZ {
        return;
    }
    let dt = (*acc).min(0.2);
    *acc = 0.0;
    field.decay(dt); // las feromonas se evaporan
    let body = active_surface_body(&ctx, &planet);
    let defs = catalog.get(body).to_vec(); // F5: fauna del planeta (para los nacimientos)
    // El jugador ya NO es el centro del mundo vivo: sólo marca qué residentes se congelan
    // (LOD) y a quién acecha un depredador. La fauna ambiental vive en su región.
    let (px, pz) = player
        .get_single()
        .map(|t| (t.translation.x, -t.translation.y))
        .unwrap_or((0.0, 0.0));
    let fase = day.phase();
    let ahora = time.elapsed_secs_f64();
    let food_pos: Vec<(f32, f32)> = foods.iter().map(|(_, f, _)| (f.x, f.z)).collect();
    let neigh: Vec<(f32, f32)> = animals.iter().map(|(_, a, ..)| (a.x, a.z)).collect();
    // Presas (no hostiles) y depredadores (hostiles) para caza/predación.
    let prey_pos: Vec<(f32, f32)> =
        animals.iter().filter(|(_, a, ..)| !a.hostile).map(|(_, a, ..)| (a.x, a.z)).collect();
    // Sólo los depredadores HAMBRIENTOS cazan → menos muertes "sin sentido" (un
    // depredador saciado no mata por matar; la presa puede pasar cerca y vivir).
    let predator_pos: Vec<(Entity, f32, f32)> = animals
        .iter()
        .filter(|(_, a, n, ..)| a.hostile && n.hunger > 0.45)
        .map(|(e, a, ..)| (e, a.x, a.z))
        .collect();

    // Muertes/nacimientos llevan región y nivel trófico: el censo del mundo tiene que
    // enterarse de cada cabeza que entra o sale (si no, materializar volvería a inventarla).
    let mut deaths: Vec<(Entity, i32, bool, (i32, i32), Trophic)> = Vec::new();
    // x, z, brain del padre, rumbo, especie, es_residente, nivel trófico.
    let mut births: Vec<(f32, f32, i32, f32, usize, bool, Trophic)> = Vec::new();
    // Censo para la predación post-loop: (entity, x, z, hostil, es_residente, brain, region, trofico).
    let mut census: Vec<(Entity, f32, f32, bool, bool, i32, (i32, i32), Trophic)> = Vec::new();
    // Forraje comido este tick: se despawnea y se le descuenta biomasa a su región.
    let mut eaten: Vec<(Entity, (i32, i32))> = Vec::new();
    // GRANJA: los adultos en celo (entity, x, z, especie). Se buscan entre sí.
    // GRANJA: mordidas de perros enojados y huevos puestos en este paso.
    let mut mordidas = 0u32;
    let mut huevos: Vec<(f32, f32)> = Vec::new();
    let enamorados: Vec<(Entity, f32, f32, usize)> = animals
        .iter()
        .filter(|(_, a, ..)| a.celo > 0.0 && a.edad >= 1.0)
        .map(|(e, a, ..)| (e, a.x, a.z, a.species))
        .collect();

    for (e, mut a, mut n, mut tf, mut sprite, mut pose, resident, de_granja) in &mut animals {
        let n = &mut *n;
        let is_res = resident.is_some();
        // F2: un residente LEJOS del jugador queda CONGELADO (no piensa/mueve/gasta) — sigue
        // como dato en su lugar y `cull_fauna` lo oculta; revive al acercarte. Así "no
        // desaparece mi granja" pero tampoco cuesta cómputo/batería estando lejos.
        // (El perro domado no se congela: te sigue a donde vayas.)
        if is_res && !a.domado {
            let dr2 = (a.x - px) * (a.x - px) + (a.z - pz) * (a.z - pz);
            if dr2 > RENDER_R * RENDER_R {
                continue;
            }
        }
        // Psicología / metabolismo: hambre y SED suben; la sed acelera el desgaste
        // (deshidratación) → el animal necesita beber.
        // El carnívoro tiene metabolismo LENTO: una presa lo sacia mucho tiempo. Con el
        // mismo ritmo que el herbívoro cazaba cada 20 s y, entre pocos, vaciaban el valle.
        let metab = if a.hostile { 0.006 } else { 0.02 };
        n.hunger = (n.hunger + dt * metab).min(1.0);
        n.thirst = (n.thirst + dt * 0.018).min(1.0);
        // El hambre baja la energía pero NUNCA a cero (piso): sin muerte por inanición.
        n.energy = (n.energy - dt * 0.02 * (1.0 + n.thirst * 0.5)).clamp(STARVE_FLOOR, 1.0);
        a.eat_t = (a.eat_t - dt).max(0.0);
        a.drink_t = (a.drink_t - dt).max(0.0);
        a.celo = (a.celo - dt).max(0.0);
        a.cria_cd = (a.cria_cd - dt).max(0.0);

        // LA CABEZA: las barras que el metabolismo ya lleva se copian, el resto decae solo, y
        // cuando toca elegir se decide entre lo que el mundo ofrece (comida, agua, cama,
        // manada, un rincón por explorar). Una necesidad crítica manda sobre el gusto.
        let clase = if a.hostile { Clase::Carnivoro } else { Clase::Herbivoro };
        let pos = (a.x, a.z);
        n.vida.m.set(Motivo::Hambre, 1.0 - n.hunger);
        n.vida.m.set(Motivo::Sed, 1.0 - n.thirst);
        n.vida.tick(clase, fase, dt, pos);
        n.bump = (n.bump - dt).max(0.0);
        n.agua_cd -= dt;
        // Si el bocado al que iba ya no está (otro se lo comió), se replantea.
        if n.vida.actividad() == Actividad::Comer {
            if let Some((tx, tz, _)) = n.vida.destino() {
                if !food_pos.iter().any(|f| (f.0 - tx).powi(2) + (f.1 - tz).powi(2) < 1.0) {
                    n.vida.abandonar();
                }
            }
        }
        if n.vida.necesita_decidir(clase, fase) {
            ads.clear();
            let rnd = |k: u64| {
                let h = sims::hash((ahora * 1000.0) as u64 ^ (a.brain as u64).wrapping_mul(0x9E37_79B9) ^ k);
                (h >> 40) as f32 / (1u64 << 24) as f32
            };
            if n.hunger > 0.4 && !a.hostile {
                let mut mejor = 40.0f32 * 40.0;
                let mut donde = None;
                for &(fx, fz) in &food_pos {
                    let d2 = (fx - a.x).powi(2) + (fz - a.z).powi(2);
                    if d2 < mejor {
                        mejor = d2;
                        donde = Some((fx, fz));
                    }
                }
                if let Some((fx, fz)) = donde {
                    ads.push(Anuncio::nuevo(Actividad::Comer, fx, fz).alcance(40.0).uso(0.5).dur(4.0).gana(Motivo::Hambre, 0.2));
                }
            }
            if n.thirst > 0.25 {
                if n.agua_cd <= 0.0 {
                    n.agua_cd = 20.0;
                    n.agua = None;
                    'buscar: for r in [3.0f32, 6.0, 9.0, 13.0, 18.0] {
                        for k in 0..8 {
                            let ang = k as f32 * std::f32::consts::FRAC_PI_4;
                            let (wx, wz) = (a.x + ang.cos() * r, a.z + ang.sin() * r);
                            if tiles.tile_at(body, wx.floor() as i64, wz.floor() as i64) == ha_shared::topdown::Tile::Water {
                                n.agua = Some((wx, wz));
                                break 'buscar;
                            }
                        }
                    }
                }
                if let Some((wx, wz)) = n.agua {
                    ads.push(Anuncio::nuevo(Actividad::Beber, wx, wz).alcance(40.0).uso(2.2).dur(4.0).gana(Motivo::Sed, 0.25));
                }
            }
            if sims::noche(fase) > 0.5 {
                ads.push(Anuncio::nuevo(Actividad::Dormir, pos.0, pos.1).uso(3.0).dur(1800.0).hasta_dia().gana(Motivo::Sueno, 0.02));
            } else {
                ads.push(Anuncio::nuevo(Actividad::Descansar, pos.0, pos.1).uso(3.0).dur(12.0).gana(Motivo::Sueno, 0.03));
            }
            if !a.hostile {
                let mut mejor = 30.0f32 * 30.0;
                let mut donde = None;
                for &(qx, qz) in &neigh {
                    let d2 = (qx - a.x).powi(2) + (qz - a.z).powi(2);
                    if d2 > 9.0 && d2 < mejor {
                        mejor = d2;
                        donde = Some((qx, qz));
                    }
                }
                if let Some((qx, qz)) = donde {
                    ads.push(Anuncio::nuevo(Actividad::Charlar, qx, qz).alcance(30.0).uso(3.0).dur(10.0).gana(Motivo::Social, 0.05));
                }
            }
            let ang = a.heading + (rnd(7) - 0.5) * 2.4;
            let dist = 8.0 + 10.0 * rnd(11);
            ads.push(
                Anuncio::nuevo(Actividad::Jugar, a.x + ang.cos() * dist, a.z + ang.sin() * dist)
                    .uso(1.5)
                    .dur(8.0)
                    .gana(Motivo::Ocio, 0.05),
            );
            n.vida.decidir(clase, pos, fase, &ads);
        }
        let act_vida = n.vida.actividad();
        // El depredador sólo persigue con hambre: saciado, deja pasar (antes cazaba por cazar).
        let cazando = a.hostile && n.hunger > 0.45;
        let mut vida_parado = false;
        let mut vida_lento = 1.0f32;
        let mut amplitud = 2.1f32;
        let mut destino_rumbo: Option<f32> = None;
        match n.vida.destino() {
            Some((tx, tz, false)) => {
                destino_rumbo = Some((tz - a.z).atan2(tx - a.x));
                amplitud = 0.6; // el IAFZ deja de mandar: sólo queda su textura
            }
            Some((_, _, true)) => match act_vida {
                Actividad::Comer | Actividad::Beber | Actividad::Dormir | Actividad::Descansar => vida_parado = true,
                Actividad::Charlar | Actividad::Jugar => vida_lento = 0.5,
                _ => {}
            },
            None => {}
        }

        // Sensor compartido (bioma + comida + social) + ESTIGMERGIA (bits 14-18:
        // gradiente de alarma/aglomeración del campo de feromonas) → IAFZ → giro.
        let s = sense(body, a.x, a.z, a.heading, n.energy, &food_pos, &neigh)
            | field.sense_bits(a.x, a.z, a.heading);
        let steer = iafz_sys::think(a.brain, s).tanh();

        // CAPA DE LOCOMOCIÓN. Antes la salida del cerebro se sumaba al rumbo:
        // `heading += steer * TURN * dt`. Eso es un integrador, y con velocidad
        // constante un `steer` constante da un CÍRCULO PERFECTO — no era un bug,
        // era el óptimo degenerado de esa acción. Bajarle la autoridad de giro a
        // la mitad sólo agrandaba el círculo.
        //
        // Ahora el cerebro no dice "cuánto giro" sino "hacia dónde", relativo a
        // un rumbo de referencia, y una capa determinista persigue ese rumbo.
        // Un `steer` constante pasa a ser un rumbo constante: una recta, no un
        // círculo. Es el mismo reparto que el PID del dron — el aprendizaje
        // elige la intención, el código garantiza la locomoción.
        //
        // Esto no poda conducta emergente: al contrario, saca a la evolución de
        // un atractor degenerado del que no podía escapar.
        let referencia = {
            let mut mejor = f32::MAX;
            let mut rumbo = a.heading; // sin comida a la vista, sigue derecho
            for &(fx, fz) in food_pos.iter().filter(|_| destino_rumbo.is_none() && (n.hunger > 0.3 || a.hostile)) {
                let d2 = (fx - a.x) * (fx - a.x) + (fz - a.z) * (fz - a.z);
                if d2 < mejor {
                    mejor = d2;
                    rumbo = (fz - a.z).atan2(fx - a.x);
                }
            }
            destino_rumbo.unwrap_or(rumbo)
        };
        // El cerebro abre hasta +-120 grados respecto de esa referencia: puede
        // ir derecho a la comida, rodearla, o darle la espalda para explorar.
        // La decisión sigue siendo suya; lo que ya no puede es girar sin parar.
        let deseado = referencia + steer * amplitud;
        let diff = (deseado - a.heading + std::f32::consts::PI)
            .rem_euclid(std::f32::consts::TAU)
            - std::f32::consts::PI;
        a.heading += diff.clamp(-1.0, 1.0) * TURN * dt;

        // DEPREDADOR (hostil): CAZA. Persigue a la presa más cercana si hay una a la vista;
        // si no, acecha al jugador (F5). La caza es la fuente de muerte VISIBLE del
        // ecosistema (reemplaza a la inanición) → selección para evolucionar + gameplay.
        if cazando {
            let mut target: Option<(f32, f32)> = None;
            let mut best = HUNT_RANGE * HUNT_RANGE;
            for &(qx, qz) in &prey_pos {
                let d2 = (qx - a.x) * (qx - a.x) + (qz - a.z) * (qz - a.z);
                if d2 > 0.01 && d2 < best {
                    best = d2;
                    target = Some((qx, qz));
                }
            }
            if target.is_none() {
                let d2 = (px - a.x) * (px - a.x) + (pz - a.z) * (pz - a.z);
                if d2 > 0.01 && d2 < 30.0 * 30.0 {
                    target = Some((px, pz));
                }
            }
            if let Some((tx, tz)) = target {
                let want = (tz - a.z).atan2(tx - a.x);
                let diff = (want - a.heading + std::f32::consts::PI)
                    .rem_euclid(std::f32::consts::TAU)
                    - std::f32::consts::PI;
                a.heading += diff.clamp(-1.0, 1.0) * TURN * dt * 0.9;
            }
        }

        // GRANJA: en celo, el animal va hacia la pareja más cercana de su especie
        // (Minecraft). Pisa la decisión del cerebro sólo mientras dura el celo —
        // 30 s—, así que no poda nada: es el jugador eligiendo quién cría.
        if a.celo > 0.0 && a.edad >= 1.0 {
            let mut mejor = PAREJA_R * PAREJA_R;
            let mut hacia: Option<(f32, f32)> = None;
            for &(oe, ox, oz, sp) in &enamorados {
                if oe == e || sp != a.species {
                    continue;
                }
                let d2 = (ox - a.x) * (ox - a.x) + (oz - a.z) * (oz - a.z);
                if d2 < mejor {
                    mejor = d2;
                    hacia = Some((ox, oz));
                }
            }
            if let Some((tx, tz)) = hacia {
                a.heading = (tz - a.z).atan2(tx - a.x);
            }
        }

        // HUIDA: si siente ALARMA cerca (bit 17 de la estigmergia) → corre.
        a.running = (s & (1 << 17)) != 0;
        // GRANJA: pánico tras un golpe, perro enojado, perro domado que te sigue.
        let conducta = conducta_granja(&mut a, px, pz, dt);
        if let Some((sx, sz)) = conducta.saltar_a {
            a.x = sx;
            a.z = sz;
        }
        if conducta.muerde {
            mordidas += 1;
        }
        // Huevos: la gallina (y el pato) de granja adulta pone cada 5-10 min.
        if de_granja && a.edad >= 1.0 && pone_huevos(&a.art_key) {
            a.huevo_t -= dt;
            if a.huevo_t <= 0.0 {
                huevos.push((a.x, a.z));
                let r = ((a.brain as u32).wrapping_mul(2654435761) ^ (a.x.to_bits())) % 1000;
                a.huevo_t = HUEVO_MIN + (HUEVO_MAX - HUEVO_MIN) * r as f32 / 1000.0;
            }
        }
        let vida_parado = vida_parado && !cazando && !a.running && conducta.prisa <= 1.0;
        let speed = if conducta.quieto || vida_parado {
            0.0
        } else if conducta.prisa > 1.0 {
            VEL * conducta.prisa
        } else {
            VEL
                * (0.4 + 0.6 * n.energy)
                * if a.running {
                    1.9
                } else if n.hunger > 0.6 || n.thirst > 0.6 {
                    1.2
                } else {
                    1.0
                }
                * vida_lento
        };
        let (nx, nz) = (a.x + a.heading.cos() * speed * dt, a.z + a.heading.sin() * speed * dt);
        // Las paredes construidas (bordes) también las encierran — es lo que
        // hace que el corral del vivario funcione.
        let hits_wall = ha_shared::build_grid::wall_capsule_hits(nx, nz, &|tx, tz, axis| {
            walls.blocks(body, tiles.z, tx, tz, axis)
        });
        if hits_wall || tiles.solid_at(body, nx.floor() as i64, nz.floor() as i64) {
            a.heading += 1.3;
            if n.bump <= 0.0 {
                n.vida.trabar();
                n.bump = 2.5;
            }
        } else {
            a.x = nx;
            a.z = nz;
        }

        // Comer. El forraje del mundo se CONSUME: desaparece del suelo y le descuenta
        // biomasa a su región, que la irá rebrotando (`ecology::step_flora`) en el mismo
        // claro. Antes se teletransportaba a 20 m del animal — otra pieza del anillo.
        // Sólo come si TIENE hambre: pasar por encima del pasto no lo arranca. Con la
        // manada entera materializada, comer por contacto dejaba el prado pelado en un
        // minuto y no había rebrote que aguantara.
        for (fe, f, placed) in foods.iter().filter(|_| n.hunger > 0.4) {
            let (dx, dz) = (a.x - f.x, a.z - f.z);
            if dx * dx + dz * dz < EAT_D2 {
                // Una comida tiene que RENDIR más de lo que gasta el metabolismo entre
                // comidas (el hambre sólo deja comer cada ~20 s). Con +0.3 el balance era
                // negativo: ningún herbívoro llegaba nunca a `REPRO_ENERGY`, así que la
                // manada no se reponía y los depredadores la extinguían. Medido con
                // `HA_ECOTEST HA_ECOSETTLE=90`.
                n.hunger = (n.hunger - 0.7).max(0.0);
                n.energy = (n.energy + 0.5).min(1.0);
                a.eat_t = 0.5; // animación "eat"
                // F8: el feeder COLOCADO (creativo) no se agota — fuente permanente.
                if placed.is_none() {
                    if let Some(r) = f.region {
                        if !eaten.iter().any(|(e, _)| *e == fe) {
                            eaten.push((fe, r));
                        }
                    }
                }
                break;
            }
        }

        // Llegó al charco que había elegido: bebe aunque la orilla le quede a un par de pasos.
        if act_vida == Actividad::Beber && n.vida.usando() && a.drink_t <= 0.0 && n.thirst > 0.05 {
            n.thirst = (n.thirst - 0.5).max(0.0);
            a.drink_t = 0.5;
        }
        // BEBER: si tiene sed y hay AGUA (Tile::Water) en una celda vecina.
        if n.thirst > 0.15 {
            let (cx, cz) = (a.x.floor() as i64, a.z.floor() as i64);
            let near_water = [(1, 0), (-1, 0), (0, 1), (0, -1)]
                .iter()
                .any(|&(dx, dz)| tiles.tile_at(body, cx + dx, cz + dz) == ha_shared::topdown::Tile::Water);
            if near_water {
                n.thirst = (n.thirst - 0.5).max(0.0);
                a.drink_t = 0.5; // animación "drink"
            }
        }

        // MIGRACIÓN REAL: si cruzó el borde de su región, el CENSO se muda con él. Nadie
        // se teletransporta y nadie se pierde — la manada se va a otro valle caminando.
        if !is_res {
            let now = ecology::region_of(a.x, a.z);
            if now != a.region {
                eco.add_head(body, a.region.0, a.region.1, a.trophic, -1.0);
                eco.add_head(body, now.0, now.1, a.trophic, 1.0);
                a.region = now;
            }
        }

        // Censo para la predación post-loop (posición ya movida).
        census.push((e, a.x, a.z, a.hostile, is_res, a.brain, a.region, a.trophic));

        // --- Ciclo de vida: reproducirse → EVOLUCIÓN. La muerte NO es por inanición (hay
        // piso de energía); llega por PREDACIÓN (post-loop) o el jugador. Esta rama sólo
        // dispara si algo externo baja la energía a 0 (futuro combate). ---
        if n.energy <= 0.0 {
            deaths.push((e, a.brain, is_res, a.region, a.trophic));
        } else if n.energy >= REPRO_ENERGY && a.edad >= 1.0 && !de_granja {
            // Cada categoría se reproduce contra SU pool/censo (ambiental vs. residente),
            // reservando espacio por los nacimientos ya encolados de esa misma categoría.
            let can = if is_res {
                let pending = births.iter().filter(|b| b.5).count();
                vivarium.alive + pending < MAX_RESIDENTS && vivarium.free_ids.len() > pending
            } else {
                // Lo que decide si nace una cría es la CAPACIDAD DE CARGA de la región, no
                // el tope de entidades: ése es un presupuesto de dibujo. Mezclarlos era un
                // bug real — el tope frenaba los nacimientos pero no la predación, así que
                // una manada saturada se extinguía por un límite de render.
                let cap = eco.cap(body, a.region.0, a.region.1);
                let pop = eco.pop(body, a.region.0, a.region.1);
                match a.trophic {
                    Trophic::Herbivore => pop.herbivores < cap.herbivores,
                    Trophic::Carnivore => pop.carnivores < cap.carnivores,
                }
            };
            if can {
                n.energy -= REPRO_COST;
                births.push((a.x, a.z, a.brain, a.heading, a.species, is_res, a.trophic));
            }
        }

        // Deposita feromona: PRESENCIA siempre (deja rastro/manada) + ALARMA si
        // está agonizando (poca energía) → los demás sienten el peligro y pueden
        // huir/agruparse por su cuenta. El jugador puede leer/alterar este campo.
        let alarm = if n.energy < 0.25 {
            1.5 // agonizando
        } else if a.relays_alarm && a.running {
            1.0 // especie "centinela" (perro terrestre / ente): propaga la alarma
        } else {
            0.0
        };
        field.deposit(a.x, a.z, alarm, 0.5);

        // (Acá vivía el ANILLO: el animal que se alejaba de `STREAM_R` se teletransportaba
        // al borde opuesto para que siempre hubiera fauna al lado tuyo. Se fue a propósito
        // — ver la cabecera del módulo. Ahora el que se aleja simplemente se aleja, y
        // `stream_regions` lo devuelve al censo de su región cuando sale de la ventana.)

        // Render: posición + tinte por energía (vivo→apagado).
        let r = render_xy(a.x, a.z);
        tf.translation = Vec3::new(r.x, r.y, sort_z(a.z));
        let en = 0.45 + 0.55 * n.energy;
        // El color de la especie es para el quad SIN arte. Con arte, multiplicarlo
        // ensuciaba el dibujo (la vaca salía gris, el perro barroso): ahí sólo se
        // apaga con la energía.
        let c = if art.has_poses(&a.art_key) { [1.0; 3] } else { a.color };
        // El golpe se ve: destello rojo, como en Minecraft.
        let rojo = if a.herido_t > 0.0 { 0.35 } else { 1.0 };
        sprite.color = Color::srgb(c[0] * en, c[1] * en * rojo, c[2] * en * rojo);

        // Animación LimeZu-compatible: pose DIRECCIONAL según el estado (eat/drink/
        // run/idle/walk). Si aún no asignaste las poses del pack, se ve el quad de
        // color (fallback) orientado por flip.
        let esc = escala_de_edad(a.edad);
        let key = a.art_key.clone();
        if art.has_poses(&key) {
            tf.scale = Vec3::new(esc, esc, 1.0);
            let verb = if conducta.quieto && !conducta.muerde {
                "idle"
            } else if a.eat_t > 0.0 {
                "eat"
            } else if a.drink_t > 0.0 {
                "drink"
            } else if vida_parado && !a.running {
                "idle"
            } else if a.running {
                // Centinela (perro terrestre) ladra ante el peligro; el resto huye.
                if a.relays_alarm {
                    "bark"
                } else {
                    "run"
                }
            } else if n.energy < 0.15 {
                "idle"
            } else {
                "walk"
            };
            let wanted = format!("{verb}_{}", dir4(a.heading));
            apply_pose(&mut commands, e, &art, &key, &wanted, &mut sprite, &mut pose.0);
        } else {
            tf.scale = Vec3::new(if a.heading.cos() < 0.0 { -esc } else { esc }, esc, 1.0);
        }
    }

    // GRANJA: las mordidas duelen de verdad y los huevos caen al piso.
    if mordidas > 0 {
        if let Ok(mut sim) = jugador_sim.get_single_mut() {
            sim.hp = (sim.hp - MORDIDA_DANO * mordidas as f32).max(0.0);
        }
    }
    for (hx, hz) in huevos {
        crate::tirados::tirar(&mut tirados, hx, hz + 0.2, "egg", 1);
    }

    // GRANJA — CITA: dos adultos en celo de la misma especie que se tocan tienen una
    // cría. Los dos quedan descansando `CRIA_CD_SECS`; la cría es residente (tuya) y
    // hereda el genoma de AMBOS padres (cruza + mutación), no de uno solo.
    let mut crias: Vec<(f32, f32, i32, i32, usize, f32)> = Vec::new();
    let mut usados: Vec<Entity> = Vec::new();
    for (i, &(e1, x1, z1, sp1)) in enamorados.iter().enumerate() {
        if usados.contains(&e1) {
            continue;
        }
        for &(e2, x2, z2, sp2) in &enamorados[i + 1..] {
            if sp1 != sp2 || usados.contains(&e2) {
                continue;
            }
            if (x1 - x2) * (x1 - x2) + (z1 - z2) * (z1 - z2) > CITA_D2 {
                continue;
            }
            let (Ok(b1), Ok(b2)) = (animals.get(e1).map(|q| q.1.brain), animals.get(e2).map(|q| q.1.brain))
            else {
                continue;
            };
            let heading = animals.get(e1).map(|q| q.1.heading).unwrap_or(0.0);
            usados.push(e1);
            usados.push(e2);
            crias.push(((x1 + x2) * 0.5, (z1 + z2) * 0.5, b1, b2, sp1, heading));
            break;
        }
    }
    for &e in &usados {
        if let Ok((_, mut a, ..)) = animals.get_mut(e) {
            a.celo = 0.0;
            a.cria_cd = CRIA_CD_SECS;
        }
    }
    for (x, z, b1, b2, species, heading) in crias {
        let Some(child) = vivarium.free_ids.pop() else { continue };
        iafz_sys::crossover(b1, b2, child);
        iafz_sys::mutate(child, child, MUTATE_RATE);
        iafz_sys::reset_echo(child);
        vivarium.alive += 1;
        let uid = vivarium.next_uid;
        vivarium.next_uid = vivarium.next_uid.wrapping_add(1);
        let def = &defs[species % defs.len()];
        let e = spawn_animal(
            &mut commands, &art, child, x, z + 0.4, heading + 2.0, 0.7, species, def,
            ecology::region_of(x, z), trophic_of(def), 0.0,
        );
        commands.entity(e).try_insert((Resident { uid }, DeGranja));
        crate::granja::globo(&mut commands, x, z, format!("¡Nació una cría de {}!", def.label));
    }

    // PREDACIÓN: un depredador MUY cerca de una presa la caza (muerte VISIBLE). Es la fuente
    // de muerte del ecosistema (en vez de la inanición) → mundo estable pero con selección
    // para evolucionar. O(presas × depredadores) con poblaciones chicas = barato.
    let mut feasts: Vec<Entity> = Vec::new();
    if !predator_pos.is_empty() {
        for &(e, x, z, hostile, is_res, brain, reg, trophic) in &census {
            if hostile {
                continue;
            }
            let hunter = predator_pos
                .iter()
                .find(|&&(_, qx, qz)| (qx - x) * (qx - x) + (qz - z) * (qz - z) < PREDATION_D2);
            if let Some(&(pe, ..)) = hunter {
                deaths.push((e, brain, is_res, reg, trophic));
                feasts.push(pe); // el que caza, COME (abajo)
            }
        }
    }

    // Forraje comido: se lo lleva del suelo y se le descuenta a la biomasa de la región.
    // Rebrota solo con el tiempo (`ecology::step_flora`) — pastar de más deja el valle
    // pelado, y eso se nota al volver.
    for (fe, reg) in eaten {
        commands.entity(fe).despawn_recursive();
        eco.eat_forage(body, reg.0, reg.1);
    }

    // Muertes: liberan su id de cerebro al pool correcto Y bajan el censo de su región (la
    // fauna ambiental es una VISTA de la población: si muere una cabeza, muere en el mundo).
    for (e, brain, is_res, reg, trophic) in deaths {
        commands.entity(e).despawn_recursive();
        if is_res {
            vivarium.free_ids.push(brain);
            vivarium.alive = vivarium.alive.saturating_sub(1);
        } else {
            world.free_ids.push(brain);
            world.alive = world.alive.saturating_sub(1);
            eco.add_head(body, reg.0, reg.1, trophic, -1.0);
        }
    }
    // El depredador COME de lo que caza: se le va el hambre y sube la energía. Sin esto
    // (era el caso) un carnívoro nunca se saciaba y mataba sin parar — con el anillo casi
    // no se materializaban depredadores y no se notaba; con la cadena alimenticia real
    // vaciaba la región en un minuto.
    for pe in feasts {
        if let Ok((_, _, mut n, ..)) = animals.get_mut(pe) {
            n.hunger = 0.0; // una presa entera SACIA: no vuelve a cazar hasta tener hambre
            n.energy = (n.energy + 0.45).min(1.0);
        }
    }

    // Nacimientos: el hijo HEREDA el genoma del padre con mutación (evolución).
    for (x, z, parent, heading, species, is_res, trophic) in births {
        let (ox, oz) = scatter((parent as u32) ^ x.to_bits(), 0, 1.2);
        if is_res {
            let Some(child) = vivarium.free_ids.pop() else { continue };
            // Salvaguardas anti mínimos/máximos locales (PLAN_VIVARIO §4): cada
            // `RES_SEED_EVERY` nacimientos, INMIGRANTE del seed del gym (reinyecta
            // diversidad); si no, herencia con mutación normal o hipermutación
            // esporádica (salto exploratorio). El piso de mutación nunca es 0.
            vivarium.births = vivarium.births.wrapping_add(1);
            // F3: la temperatura (poca diversidad → alta) inmigra más seguido y muta más
            // fuerte para escapar de mínimos/máximos locales.
            let temp = vivarium.temperature.clamp(TEMP_MIN, TEMP_MAX);
            let seed_every = (RES_SEED_EVERY as f32 / temp).round().max(2.0) as u32;
            if vivarium.births % seed_every == 0 {
                iafz_sys::load_bytes(child, GENOME); // inmigrante del seed del gym
            } else {
                let base = if (child as u32).wrapping_mul(2654435761) % 5 == 0 {
                    RES_HYPERMUTATE_RATE
                } else {
                    MUTATE_RATE
                };
                iafz_sys::mutate(parent, child, (base * temp).min(0.5));
            }
            iafz_sys::reset_echo(child);
            vivarium.alive += 1;
            let uid = vivarium.next_uid;
            vivarium.next_uid = vivarium.next_uid.wrapping_add(1);
            let def = &defs[species % defs.len()];
            let (cx, cz) = (x + ox, z + oz);
            let e = spawn_animal(
                &mut commands, &art, child, cx, cz, heading + 1.0, 0.55, species, def,
                ecology::region_of(cx, cz), trophic_of(def), 1.0,
            );
            commands.entity(e).try_insert(Resident { uid });
        } else {
            let (cx, cz) = (x + ox, z + oz);
            let reg = ecology::region_of(cx, cz);
            // El nacimiento SUMA una cabeza al MUNDO, siempre. Que además se dibuje
            // depende del presupuesto de entidades: si no hay hueco, la cría existe como
            // población y se materializa cuando lo haya (o cuando vuelvas).
            eco.add_head(body, reg.0, reg.1, trophic, 1.0);
            if world.alive >= MAX_LIVE {
                continue;
            }
            let Some(child) = world.free_ids.pop() else { continue };
            iafz_sys::mutate(parent, child, MUTATE_RATE); // copia padre→hijo + muta
            iafz_sys::reset_echo(child);
            world.alive += 1;
            // La cría hereda el nivel trófico del padre (la especie puede venir teñida por
            // `pick_species`, así que no se re-deriva de `def`).
            let mut def = defs[species % defs.len()].clone();
            def.hostile = trophic == Trophic::Carnivore;
            spawn_animal(
                &mut commands, &art, child, cx, cz, heading + 1.0, 0.55, species, &def, reg, trophic,
                1.0,
            );
        }
    }
}

/// F2 — Activación por CUERPO: instancia como entidades sólo los residentes del cuerpo
/// donde estás. Al SALIR de un cuerpo, snapshotea los vivos → `records` (con su genoma
/// evolucionado en `g_<uid>.adn`) y despawnea; al ENTRAR, re-spawnea los guardados. Los
/// residentes NUNCA se pierden: viven como datos (otros cuerpos) o entidades (el actual).
/// Ver `PLAN_VIVARIO_VIDA_EMERGENTE.md` §2/§5.
#[allow(clippy::too_many_arguments)]
fn manage_vivarium(
    mut commands: Commands,
    art: Res<crate::art2d::ArtLibrary>,
    region: Res<RegionRuntime>,
    ctx: Res<WorldContextRuntime>,
    planet: Res<ActivePlanet>,
    mut vivarium: ResMut<VivariumWorld>,
    mut catalog: ResMut<FaunaCatalog>,
    live: Query<(&Animal, &Needs, &Resident, Has<DeGranja>)>,
    live_ents: Query<Entity, With<Resident>>,
) {
    if !vivarium.loaded {
        let (recs, next) = load_vivarium();
        vivarium.records = recs;
        vivarium.next_uid = next.max(vivarium.next_uid);
        vivarium.loaded = true;
    }
    let current = if surface_region_active(&region).is_some() {
        Some(active_surface_body(&ctx, &planet) as u8)
    } else {
        None
    };
    if current == vivarium.active_body {
        return;
    }
    // Snapshot + despawn del cuerpo VIEJO → registros + genomas a disco.
    if let Some(old) = vivarium.active_body {
        vivarium.records.retain(|r| r.body != old);
        for (a, n, res, granja) in &live {
            iafz_sys::save(a.brain, genome_path(res.uid).to_str().unwrap_or(""));
            vivarium.records.push(ResidentRecord {
                uid: res.uid,
                body: old,
                x: a.x,
                z: a.z,
                heading: a.heading,
                species: a.species,
                hunger: n.hunger,
                thirst: n.thirst,
                energy: n.energy,
                edad: a.edad,
                cria_cd: a.cria_cd,
                granja,
                vida: a.vida,
                domado: a.domado,
                sentado: a.sentado,
                nombre: a.nombre_propio.clone(),
            });
        }
        for e in &live_ents {
            commands.entity(e).despawn_recursive();
        }
        vivarium.free_ids = (RES_ID_BASE..RES_ID_BASE + MAX_RESIDENTS as i32).rev().collect();
        vivarium.alive = 0;
        let next = vivarium.next_uid;
        save_vivarium(&vivarium.records, next);
    }
    // Spawn del cuerpo NUEVO desde sus registros guardados.
    if let Some(nb) = current {
        let body = active_surface_body(&ctx, &planet);
        let defs = catalog.get(body).to_vec(); // F5: fauna del planeta para el visual
        let recs: Vec<ResidentRecord> =
            vivarium.records.iter().filter(|r| r.body == nb).cloned().collect();
        vivarium.records.retain(|r| r.body != nb);
        for rec in recs {
            if defs.is_empty() {
                break;
            }
            let Some(brain) = vivarium.free_ids.pop() else { break };
            let loaded = genome_path(rec.uid)
                .to_str()
                .map(|p| iafz_sys::load(brain, p))
                .unwrap_or(false);
            if !loaded {
                iafz_sys::load_bytes(brain, GENOME); // sin archivo → seed del gym
            }
            iafz_sys::reset_echo(brain);
            vivarium.alive += 1;
            let def = defs.get(rec.species).unwrap_or(&defs[0]);
            let e = spawn_animal(
                &mut commands, &art, brain, rec.x, rec.z, rec.heading, rec.energy, rec.species, def,
                ecology::region_of(rec.x, rec.z), trophic_of(def), rec.edad,
            );
            commands.entity(e).try_insert((
                Resident { uid: rec.uid },
                Needs::nuevo(rec.hunger, rec.thirst, rec.energy, rec.uid),
                Restaurar {
                    cria_cd: rec.cria_cd,
                    vida: rec.vida,
                    domado: rec.domado,
                    sentado: rec.sentado,
                    nombre: rec.nombre.clone(),
                },
            ));
            if rec.granja {
                commands.entity(e).try_insert(DeGranja);
            }
        }
    }
    vivarium.active_body = current;
}

// --- Sonda de QA (HA_ECOTEST) ------------------------------------------------------

/// Foto del estado ecológico de donde está el jugador. La usa el driver `HA_ECOTEST`
/// (`debug/autodrive.rs`) para afirmar lo que el usuario pidió: **yermo vacío, bosque
/// lleno**. Sin esto, "se fue el anillo" sería una afirmación sin comprobar.
pub(crate) struct EcoProbe {
    pub(crate) region: (i32, i32),
    pub(crate) cap: Capacity,
    pub(crate) pop: Population,
    pub(crate) live_herb: usize,
    pub(crate) live_carn: usize,
    pub(crate) forage: usize,
    pub(crate) known_regions: usize,
    /// Totales de la VENTANA 3×3 (lo que de verdad se está simulando): una región sola
    /// engaña, porque los animales cruzan de una a otra todo el rato.
    pub(crate) win: Population,
    /// Totales de TODAS las regiones conocidas del cuerpo. Es la única medida honesta de
    /// "el mundo se está vaciando": la ventana baja sola cuando la manada se dispersa, y
    /// eso es vida normal, no un colapso.
    pub(crate) total: Population,
}

pub(crate) fn eco_probe(world: &mut World, body: CelestialBody, x: f32, z: f32) -> EcoProbe {
    let (rx, rz) = ecology::region_of(x, z);
    let (cap, pop, known_regions, win) = {
        let mut eco = world.resource_mut::<EcologyWorld>();
        let mut win = Population::default();
        for dz in -SIM_RINGS..=SIM_RINGS {
            for dx in -SIM_RINGS..=SIM_RINGS {
                let p = eco.pop(body, rx + dx, rz + dz);
                win.plants += p.plants;
                win.herbivores += p.herbivores;
                win.carnivores += p.carnivores;
            }
        }
        (eco.cap(body, rx, rz), eco.pop(body, rx, rz), eco.cells.len(), win)
    };
    let total = {
        let eco = world.resource::<EcologyWorld>();
        let mut t = Population::default();
        for (&(b, ..), c) in eco.cells.iter() {
            if b == body as u8 {
                t.plants += c.pop.plants;
                t.herbivores += c.pop.herbivores;
                t.carnivores += c.pop.carnivores;
            }
        }
        t
    };
    let mut q = world.query_filtered::<&Animal, Without<Resident>>();
    let (mut live_herb, mut live_carn) = (0, 0);
    for a in q.iter(world) {
        match a.trophic {
            Trophic::Herbivore => live_herb += 1,
            Trophic::Carnivore => live_carn += 1,
        }
    }
    let mut qf = world.query_filtered::<&FoodDot, Without<PlacedFood>>();
    let forage = qf.iter(world).count();
    EcoProbe { region: (rx, rz), cap, pop, live_herb, live_carn, forage, known_regions, win, total }
}

/// F4 — Overlay de observación (DEBUG, sólo creativo): arriba-izquierda, ASCII-only, con
/// censo/nacimientos/temperatura/diversidad/energía de los residentes. Para "verles" y
/// depurar la evolución. Oculto fuera de creativo.
#[derive(Component)]
struct VivariumOverlay;

fn spawn_vivarium_overlay(mut commands: Commands) {
    commands.spawn((
        VivariumOverlay,
        Text::new(""),
        TextFont { font_size: 14.0, ..default() },
        TextColor(Color::srgba(0.70, 1.0, 0.80, 0.92)),
        Node {
            position_type: PositionType::Absolute,
            top: Val::Px(8.0),
            left: Val::Px(8.0),
            ..default()
        },
        Visibility::Hidden,
    ));
}

#[allow(clippy::too_many_arguments)]
fn update_vivarium_overlay(
    editor: Res<crate::ui_editor::UiEditorState>,
    vivarium: Res<VivariumWorld>,
    ctx: Res<WorldContextRuntime>,
    planet: Res<ActivePlanet>,
    mut eco: ResMut<EcologyWorld>,
    world: Res<FaunaWorld>,
    player: Query<&Transform, With<Player>>,
    residents: Query<&Needs, With<Resident>>,
    mut overlay: Query<(&mut Text, &mut Visibility), With<VivariumOverlay>>,
) {
    let Ok((mut text, mut vis)) = overlay.get_single_mut() else {
        return;
    };
    if !editor.creative || !crate::debug::paneles_visibles() {
        if *vis != Visibility::Hidden {
            *vis = Visibility::Hidden;
        }
        return;
    }
    if *vis != Visibility::Visible {
        *vis = Visibility::Visible;
    }
    let n = residents.iter().count();
    let avg_e = if n > 0 {
        residents.iter().map(|nd| nd.energy).sum::<f32>() / n as f32
    } else {
        0.0
    };
    let div = ((TEMP_MAX - vivarium.temperature) / (TEMP_MAX - TEMP_MIN)).clamp(0.0, 1.0);
    // ECOLOGÍA: lo que hay que poder leer para saber si el mundo vive sin el anillo — en
    // qué región estás, cuánto sostiene su bioma, cuánto hay de verdad, y cuántas regiones
    // se están simulando de fondo. Si la fila `aqui` dice `cap h 0.0` estás en un yermo y
    // que no haya fauna es CORRECTO: hay que ir a buscarla a otro lado.
    let eco_line = match player.get_single() {
        Ok(pt) => {
            let body = active_surface_body(&ctx, &planet);
            let (rx, rz) = ecology::region_of(pt.translation.x, -pt.translation.y);
            let cap = eco.cap(body, rx, rz);
            let pop = eco.pop(body, rx, rz);
            format!(
                "\nECO  region ({rx},{rz})  cap p{:.0} h{:.1} c{:.1}  vive p{:.1} h{:.1} c{:.1}  \
                 entidades {}  regiones {}",
                cap.plants,
                cap.herbivores,
                cap.carnivores,
                pop.plants,
                pop.herbivores,
                pop.carnivores,
                world.alive,
                eco.cells.len(),
            )
        }
        Err(_) => String::new(),
    };
    let line = format!(
        "VIVARIO  vivos {n}  nac {}  temp {:.1}  div {:.0}%  energia {:.0}%{eco_line}",
        vivarium.births,
        vivarium.temperature,
        div * 100.0,
        avg_e * 100.0
    );
    if text.0 != line {
        text.0 = line;
    }
}

/// F3 — Termostato de diversidad: mide la variedad de CONDUCTA de los residentes vivos
/// (histograma acción×octante de rumbo → entropía de Shannon normalizada) y ajusta
/// `temperature`. Poca diversidad (todos hacen lo mismo = mínimo local) → temperatura alta
/// → más mutación/inmigración en los próximos nacimientos. Barato (≤ `MAX_RESIDENTS`).
fn vivarium_temperature(
    time: Res<Time>,
    mut vivarium: ResMut<VivariumWorld>,
    residents: Query<(&Animal, &Needs), With<Resident>>,
    mut acc: Local<f32>,
) {
    *acc += time.delta_secs();
    if *acc < 3.0 {
        return;
    }
    *acc = 0.0;
    // Firma de conducta: verbo (0..4) × octante de rumbo (0..7) = 40 clases.
    let mut hist = [0u32; 40];
    let mut n = 0u32;
    for (a, needs) in &residents {
        let verb = if a.eat_t > 0.0 {
            0
        } else if a.drink_t > 0.0 {
            1
        } else if a.running {
            2
        } else if needs.energy < 0.15 {
            3
        } else {
            4
        };
        let oct = (((a.heading.rem_euclid(std::f32::consts::TAU))
            / (std::f32::consts::TAU / 8.0)) as usize)
            & 7;
        hist[verb * 8 + oct] += 1;
        n += 1;
    }
    if n < 2 {
        return; // muy pocos para medir; dejar la temperatura como está
    }
    let nf = n as f32;
    let mut ent = 0.0f32;
    for &c in &hist {
        if c > 0 {
            let p = c as f32 / nf;
            ent -= p * p.ln();
        }
    }
    let max_ent = (n.min(40) as f32).ln().max(1e-3);
    let diversity = (ent / max_ent).clamp(0.0, 1.0);
    let target = TEMP_MIN + (1.0 - diversity) * (TEMP_MAX - TEMP_MIN);
    vivarium.temperature += (target - vivarium.temperature) * 0.5; // suavizado
}

/// F2 — Qué fauna se DIBUJA. Dos reglas, un solo dueño del `Visibility`:
///
/// 1. **Otra capa, no se ve.** Los animales viven en la superficie; desde el
///    segundo piso que construiste o desde una cueva no se los ve, igual que no
///    se ven los tiles ni el decor de abajo. Sin esto, subir a tu segundo piso te
///    dejaba las vacas pastando sobre la losa.
/// 2. **Residente lejos, no se ve** (se congela en `tick_fauna`): "se renderizan
///    sólo cuando estás cerca". La fauna ambiental no necesita esta regla porque
///    ya se desmaterializa por región.
///
/// Barato: una pasada por animal vivo, sin raíces cuadradas.
fn cull_fauna(
    tiles: Res<TileWorld>,
    player: Query<&Transform, (With<Player>, Without<Animal>)>,
    mut fauna: Query<(&Animal, &mut Visibility, Has<Resident>)>,
) {
    let Ok(pt) = player.get_single() else {
        return;
    };
    // **Cualquier planta que no sea la superficie oculta la vida.** Los animales
    // viven arriba: sobre tu losa no pastan, y bajo tierra tampoco. Hasta el 7 ago
    // esto era `z > 0` —el subsuelo se consideraba "el mismo mundo, visto desde
    // abajo"—, pero desde que el subsuelo se DIBUJA como subsuelo
    // (`world2d::render_tile`) eso dejaba mariposas y vacas revoloteando sobre la
    // tierra maciza de una galería. El subsuelo es su propio sitio; lo que viva
    // ahí lo tiene que poner el subsuelo.
    let off_layer = tiles.z != 0;
    let (px, pz) = (pt.translation.x, -pt.translation.y);
    for (a, mut vis, resident) in &mut fauna {
        let dr2 = (a.x - px) * (a.x - px) + (a.z - pz) * (a.z - pz);
        let want = if off_layer || (resident && dr2 > RENDER_R * RENDER_R) {
            Visibility::Hidden
        } else {
            Visibility::Visible
        };
        if *vis != want {
            *vis = want;
        }
    }
}

/// F2 — Persistencia periódica (y al pausar la app): combina los registros de otros
/// cuerpos con un snapshot de los residentes VIVOS del cuerpo actual → disco, para que la
/// evolución no se pierda aunque no cambies de cuerpo ni cierres el juego "prolijo".
#[allow(clippy::too_many_arguments)]
fn save_vivarium_periodic(
    time: Res<Time>,
    region: Res<RegionRuntime>,
    ctx: Res<WorldContextRuntime>,
    planet: Res<ActivePlanet>,
    vivarium: Res<VivariumWorld>,
    live: Query<(&Animal, &Needs, &Resident, Has<DeGranja>)>,
    mut acc: Local<f32>,
    mut was_paused: Local<bool>,
) {
    if !vivarium.loaded {
        return;
    }
    let paused = is_app_paused();
    let flush_now = paused && !*was_paused;
    *was_paused = paused;
    *acc += time.delta_secs();
    if !flush_now && *acc < VIV_SAVE_EVERY {
        return;
    }
    *acc = 0.0;
    let current = if surface_region_active(&region).is_some() {
        Some(active_surface_body(&ctx, &planet) as u8)
    } else {
        None
    };
    let mut all = vivarium.records.clone();
    if let Some(b) = current {
        all.retain(|r| r.body != b);
        for (a, n, res, granja) in &live {
            iafz_sys::save(a.brain, genome_path(res.uid).to_str().unwrap_or(""));
            all.push(ResidentRecord {
                uid: res.uid,
                body: b,
                x: a.x,
                z: a.z,
                heading: a.heading,
                species: a.species,
                hunger: n.hunger,
                thirst: n.thirst,
                energy: n.energy,
                edad: a.edad,
                cria_cd: a.cria_cd,
                granja,
                vida: a.vida,
                domado: a.domado,
                sentado: a.sentado,
                nombre: a.nombre_propio.clone(),
            });
        }
    }
    save_vivarium(&all, vivarium.next_uid);
}

/// F7 — publica los avistamientos de FAUNA/residentes por cuerpo (para la cámara). Cuerpos
/// remotos: desde `records` (posiciones persistidas); cuerpo actual: desde los residentes
/// vivos. Throttle 2 s → barato. Sólo residentes/persistentes (la fauna ambiental se
/// streamea y no existe en remoto).
#[allow(clippy::too_many_arguments)]
fn update_fauna_sightings(
    time: Res<Time>,
    region: Res<RegionRuntime>,
    ctx: Res<WorldContextRuntime>,
    planet: Res<ActivePlanet>,
    vivarium: Res<VivariumWorld>,
    mut catalog: ResMut<FaunaCatalog>,
    mut sightings: ResMut<crate::sightings::WorldSightings>,
    live: Query<&Animal, With<Resident>>,
    ambient: Query<&Animal, Without<Resident>>,
    mut acc: Local<f32>,
) {
    *acc += time.delta_secs();
    if *acc < 2.0 {
        return;
    }
    *acc = 0.0;
    use crate::sightings::Sighting;
    sightings.fauna.clear();
    // Cuerpos remotos: desde los registros persistidos.
    for rec in &vivarium.records {
        if let Some(body) = crate::world2d::body_from_u8(rec.body) {
            let (color, hostile) = {
                let defs = catalog.get(body);
                match defs.get(rec.species) {
                    Some(d) => (d.color, d.hostile),
                    None => continue,
                }
            };
            sightings
                .fauna
                .entry(rec.body)
                .or_default()
                .push(Sighting { x: rec.x, z: rec.z, color, hostile });
        }
    }
    // Cuerpo actual: residentes vivos + la fauna AMBIENTAL materializada. La ambiental ya
    // tiene sitio propio en el mundo (antes se reubicaba en un anillo y publicarla no
    // significaba nada), así que la cámara puede enseñar la manada que hay de verdad.
    if surface_region_active(&region).is_some() {
        let b = active_surface_body(&ctx, &planet) as u8;
        for a in live.iter().chain(ambient.iter()) {
            sightings
                .fauna
                .entry(b)
                .or_default()
                .push(Sighting { x: a.x, z: a.z, color: a.color, hostile: a.hostile });
        }
    }
}

// ─── GRANJA: alimentar, criar, golpear, domar, bautizar ─────────────────────────
//
// El modelo es el de Minecraft: cada especie come algo; dárselo a un adulto lo pone
// en celo 30 s; dos en celo de la misma especie se buscan y tienen una cría que
// tarda 20 min en crecer (cada comida le ahorra un 10 %). Los padres descansan 5 min.
//
// El GESTO es uno solo (tocar o mantener sobre el animal) y lo que hace depende de
// lo que llevás en la mano, como el clic derecho/izquierdo de Minecraft fundidos:
//
// | en la mano              | hace                                             |
// |-------------------------|--------------------------------------------------|
// | su comida               | celo (adulto) o +10 % de crecimiento (cría)      |
// | etiqueta                | abre el panel para ponerle nombre (`bautizo.rs`) |
// | hueso (perro salvaje)   | intenta domarlo (1 de 3, como el lobo)           |
// | lo que sea (perro tuyo) | sentado / te sigue                               |
// | cualquier otra cosa     | GOLPE: daño según lo que tengas (mano 1, espada 5-7), retroceso, huye; el perro se ENOJA y te muerde |
//
// Al morir suelta su botín en el SUELO (`tirados.rs`): res → carne y cuero, cerdo →
// carne de cerdo, gallina → pollo y plumas, oveja → cordero y lana… Las crías no
// sueltan nada.
//
// Lo propio de este juego: la fauna del mundo es una VISTA de la población de su
// región (ver la cabecera). Un animal que alimentás, domás o bautizás deja de ser
// del monte y pasa a ser TUYO — residente de granja: persiste, no se desmaterializa
// al alejarte, no se reproduce solo y viaja con la partida.

/// Qué come cada especie para criar. La clave es la del arte (`npc/fauna_*`).
pub fn comida_de_cria(art_key: &str, hostil: bool) -> &'static [&'static str] {
    if hostil {
        return &[];
    }
    match art_key {
        "npc/fauna_cow" | "npc/fauna_sheep" | "npc/fauna_goat" => &["wheat"],
        "npc/fauna_chicken" | "npc/fauna_duck" => &["wheat_seeds", "pumpkin_seeds"],
        "npc/fauna_pig" | "npc/fauna_rabbit" => &["carrot"],
        "npc/fauna_donkey" => &["apple", "carrot"],
        // El perro, como el lobo: cualquier carne.
        "npc/fauna_dog" => &["meat", "beef", "pork", "mutton", "chicken", "rabbit_meat"],
        // Fauna de otros planetas: come de la huerta como cualquier herbívoro.
        _ => &["wheat", "carrot"],
    }
}

/// Vida de cada especie (Minecraft: vaca/cerdo 10, oveja 8, gallina 4, conejo 3,
/// lobo 8, burro ~15).
pub fn vida_de(art_key: &str) -> f32 {
    match art_key {
        "npc/fauna_cow" | "npc/fauna_pig" | "npc/fauna_goat" => 10.0,
        "npc/fauna_sheep" | "npc/fauna_dog" => 8.0,
        "npc/fauna_donkey" => 15.0,
        "npc/fauna_chicken" | "npc/fauna_duck" => 4.0,
        "npc/fauna_rabbit" => 3.0,
        _ => 8.0,
    }
}

/// Daño de un golpe según lo que llevás en la mano. Las espadas usan el de su
/// regla de interacción; el resto, lo que pegaría esa herramienta.
pub fn dano_de(item: &str) -> f32 {
    if let Some(d) = ha_shared::resolve_interaction(Some(item), ha_shared::InteractTargetKind::Animal)
        .and_then(|r| r.damage)
    {
        return d / 3.6; // 18 → 5, 26 → 7: la escala de vida de los animales
    }
    match item {
        "axe" | "stone_axe" => 4.0,
        "wood_axe" | "pickaxe" | "stone_pickaxe" => 3.0,
        "wood_pickaxe" => 2.0,
        _ => 1.0,
    }
}

/// **El botín de cada especie** (Minecraft), con `r` una fuente de azar 0..1.
/// Adultos sólo. Los huesos no los da Minecraft, pero acá son la única forma de
/// conseguir con qué domar un perro hasta que haya enemigos que los suelten.
pub fn botin(art_key: &str, mut r: impl FnMut() -> f32) -> Vec<(&'static str, u32)> {
    let mut rango = |a: u32, b: u32| a + ((r() * (b - a + 1) as f32) as u32).min(b - a);
    let mut v = match art_key {
        "npc/fauna_cow" => vec![("beef", rango(1, 3)), ("leather", rango(0, 2))],
        "npc/fauna_pig" => vec![("pork", rango(1, 3))],
        "npc/fauna_chicken" | "npc/fauna_duck" => vec![("chicken", 1), ("feather", rango(0, 2))],
        "npc/fauna_sheep" => vec![("mutton", rango(1, 2)), ("wool", 1)],
        "npc/fauna_goat" => vec![("mutton", rango(1, 2)), ("leather", rango(0, 1))],
        "npc/fauna_donkey" => vec![("leather", rango(0, 2))],
        "npc/fauna_rabbit" => vec![("rabbit_meat", 1), ("leather", rango(0, 1))],
        "npc/fauna_dog" => vec![],
        _ => vec![("meat", 1)],
    };
    if matches!(
        art_key,
        "npc/fauna_cow" | "npc/fauna_pig" | "npc/fauna_sheep" | "npc/fauna_goat" | "npc/fauna_donkey"
    ) {
        v.push(("bone", rango(0, 1)));
    }
    v.retain(|(_, n)| *n > 0);
    v
}

/// ¿Pone huevos? (gallina y pato de granja, adultos).
fn pone_huevos(art_key: &str) -> bool {
    matches!(art_key, "npc/fauna_chicken" | "npc/fauna_duck")
}

/// Tamaño de dibujo por edad: una cría mide la mitad.
pub fn escala_de_edad(edad: f32) -> f32 {
    0.5 + 0.5 * edad.clamp(0.0, 1.0)
}

/// Cada cuánto pone un huevo una gallina (s). Minecraft: 5-10 min.
const HUEVO_MIN: f32 = 300.0;
const HUEVO_MAX: f32 = 600.0;
/// Un perro enojado muerde cada tanto y a esta distancia.
const MORDIDA_D: f32 = 1.3;
const MORDIDA_CD: f32 = 1.0;
const MORDIDA_DANO: f32 = 2.0;
/// El perro domado te sigue si te alejás más que esto, y aparece a tu lado si te
/// alejás muchísimo (volaste, te teletransportaste).
const PERRO_SIGUE_D: f32 = 4.5;
const PERRO_SALTA_D: f32 = 30.0;

/// Azar barato y reproducible (xorshift sobre una semilla).
fn azar(semilla: &mut u32) -> f32 {
    let mut x = *semilla | 1;
    x ^= x << 13;
    x ^= x >> 17;
    x ^= x << 5;
    *semilla = x;
    (x % 10_000) as f32 / 10_000.0
}

/// Lo que la conducta de granja le pide al movimiento este paso.
#[derive(Default, Debug, PartialEq)]
pub(crate) struct Conducta {
    /// No se mueve (perro sentado).
    pub(crate) quieto: bool,
    /// Multiplicador de velocidad (huye, persigue, te alcanza).
    pub(crate) prisa: f32,
    /// Aparecer al lado del jugador (perro domado que quedó muy lejos).
    pub(crate) saltar_a: Option<(f32, f32)>,
    /// Muerde al jugador este paso.
    pub(crate) muerde: bool,
}

/// **La conducta de granja**, encima de lo que decide el cerebro: pánico tras un
/// golpe, el perro enojado que persigue y muerde, el domado que te sigue. Pisa el
/// rumbo sólo mientras dura cada estado, así que la conducta emergente del resto
/// del tiempo queda intacta. Función pura para poder probarla sin Bevy.
pub(crate) fn conducta_granja(a: &mut Animal, px: f32, pz: f32, dt: f32) -> Conducta {
    let mut c = Conducta { prisa: 1.0, ..default() };
    a.herido_t = (a.herido_t - dt).max(0.0);
    a.panico_t = (a.panico_t - dt).max(0.0);
    a.mordida_cd = (a.mordida_cd - dt).max(0.0);
    a.corazones_t = (a.corazones_t - dt).max(0.0);
    let (dx, dz) = (px - a.x, pz - a.z);
    let d = (dx * dx + dz * dz).sqrt();
    if a.domado {
        a.enojado = false;
        if a.sentado {
            c.quieto = true;
        } else if d > PERRO_SALTA_D {
            c.saltar_a = Some((px - 1.2, pz + 0.6));
        } else if d > PERRO_SIGUE_D {
            a.heading = dz.atan2(dx);
            c.prisa = 1.7;
            a.running = true;
        }
        return c;
    }
    if a.enojado {
        if d > 30.0 {
            a.enojado = false; // lo perdiste de vista
        } else {
            a.heading = dz.atan2(dx);
            c.prisa = 1.9;
            a.running = true;
            if d < MORDIDA_D && a.mordida_cd <= 0.0 {
                a.mordida_cd = MORDIDA_CD;
                c.muerde = true;
            }
            if d < 0.8 {
                c.quieto = true; // encima tuyo no empuja: muerde
            }
        }
        return c;
    }
    if a.panico_t > 0.0 {
        a.heading = (-dz).atan2(-dx);
        c.prisa = 1.9;
        a.running = true;
    }
    c
}

/// Estado de un animal en palabras, para el cartel y la ayuda.
fn estado_animal(a: &Animal, tuyo: bool) -> String {
    let mut s = match &a.nombre_propio {
        Some(n) => format!("{n} ({})", a.nombre),
        None => a.nombre.clone(),
    };
    if a.edad < 1.0 {
        s.push_str(&format!(" · cría {} %", (a.edad * 100.0).floor() as u32));
    } else if a.celo > 0.0 {
        s.push_str(" · en celo");
    } else if a.cria_cd > 0.0 {
        s.push_str(" · descansando");
    }
    if a.enojado {
        s.push_str(" · ¡enojado!");
    }
    if a.domado {
        s.push_str(if a.sentado { " · sentado" } else { " · te sigue" });
    } else if tuyo {
        s.push_str(" · tuyo");
    }
    if a.vida < a.vida_max {
        s.push_str(&format!(" · vida {}/{}", a.vida.ceil() as u32, a.vida_max as u32));
    }
    s
}

/// El cartel de acción del animal apuntado (lo lee el HUD). `None` sin animal.
#[derive(Resource, Default)]
pub struct AyudaAnimal(pub Option<String>);

/// **Animal de GRANJA**: lo alimentaste, lo domaste o lo bautizaste vos, o nació de
/// una cría tuya. A diferencia del residente del vivario (creativo), no se
/// reproduce solo cuando está bien comido: se cría a mano, como en Minecraft. Sin
/// esto la reproducción emergente del vivario convertía dos vacas en treinta y dos
/// en un minuto (medido con `HA_GRANJATEST`), y la granja dejaba de ser una
/// decisión del jugador. La fauna salvaje y el vivario siguen exactamente igual.
#[derive(Component)]
pub struct DeGranja;

/// Estado guardado en disco que hay que devolverle al animal al re-spawnearlo.
#[derive(Component)]
struct Restaurar {
    cria_cd: f32,
    vida: f32,
    domado: bool,
    sentado: bool,
    nombre: Option<String>,
}

/// Corazón que sube sobre un animal en celo.
#[derive(Component)]
struct Corazon {
    vida: f32,
}

/// El nombre flotando sobre un animal bautizado. Sigue a su dueño (no es hijo:
/// el animal se espeja y se escala, y el texto no tiene que hacerlo).
#[derive(Component)]
struct EtiquetaNombre(Entity);

/// Marca en el animal: ya tiene su etiqueta dibujada.
#[derive(Component)]
struct ConEtiqueta(Entity);

pub struct GranjaAnimalesPlugin;

impl Plugin for GranjaAnimalesPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<AyudaAnimal>().add_systems(
            Update,
            (
                aplicar_restauracion,
                crecer_crias,
                interactuar_animal.before(crate::hud::update_interact_hud),
                aplicar_nombres,
                rotular_animal_apuntado,
                corazones,
                etiquetas_de_nombre,
            )
                .chain()
                .run_if(crate::game_phase::world_active),
        );
    }
}

fn aplicar_restauracion(mut commands: Commands, mut q: Query<(Entity, &mut Animal, &Restaurar)>) {
    for (e, mut a, r) in &mut q {
        a.cria_cd = r.cria_cd;
        if r.vida > 0.0 {
            a.vida = r.vida.min(a.vida_max);
        }
        a.domado = r.domado;
        a.sentado = r.sentado;
        a.nombre_propio = r.nombre.clone();
        commands.entity(e).remove::<Restaurar>();
    }
}

/// Las crías crecen con el reloj, estén o no a la vista (un residente lejano
/// está congelado para el cerebro, no para el calendario). Y los heridos se
/// curan despacio (Minecraft: medio corazón cada tanto).
fn crecer_crias(time: Res<Time>, mut q: Query<&mut Animal>) {
    let dt = time.delta_secs();
    for mut a in &mut q {
        if a.edad < 1.0 {
            a.edad = (a.edad + dt / CRECER_SECS).min(1.0);
        }
        if a.vida < a.vida_max && a.herido_t <= 0.0 {
            a.vida = (a.vida + dt / 20.0).min(a.vida_max);
        }
    }
}

/// **Pasa un animal del monte a tu granja**: cerebro nuevo del rango de
/// residentes (copia exacta del que tenía), baja del censo de su región, y desde
/// ahora persiste. Si el vivario está lleno queda como estaba (igual es tuyo en la
/// sesión: lleva `DeGranja`).
#[allow(clippy::too_many_arguments)]
fn domesticar(
    commands: &mut Commands,
    entity: Entity,
    a: &mut Animal,
    residente: bool,
    body: CelestialBody,
    world: &mut FaunaWorld,
    vivarium: &mut VivariumWorld,
    eco: &mut EcologyWorld,
) {
    if !residente && !a.hostile {
        if let Some(nuevo) = vivarium.free_ids.pop() {
            iafz_sys::mutate(a.brain, nuevo, 0.0);
            world.free_ids.push(a.brain);
            world.alive = world.alive.saturating_sub(1);
            eco.add_head(body, a.region.0, a.region.1, a.trophic, -1.0);
            a.brain = nuevo;
            vivarium.alive += 1;
            let uid = vivarium.next_uid;
            vivarium.next_uid = vivarium.next_uid.wrapping_add(1);
            commands.entity(entity).try_insert(Resident { uid });
        }
    }
    commands.entity(entity).try_insert(DeGranja);
}

#[allow(clippy::too_many_arguments, clippy::type_complexity)]
fn interactuar_animal(
    mut commands: Commands,
    mut hold: ResMut<crate::input::HoldInteractState>,
    target: Res<crate::targeting::LookTarget>,
    time: Res<Time>,
    ctx: Res<WorldContextRuntime>,
    planet: Res<ActivePlanet>,
    mut equipped: ResMut<crate::input::EquippedItem>,
    (mut inv, mut inv_ui, mut persist): (
        ResMut<crate::inventory_persist::PlayerInventory>,
        ResMut<crate::hud::inventory_ui::InventoryUiState>,
        ResMut<crate::inventory_persist::InventoryPersistState>,
    ),
    (mut world, mut vivarium, mut eco): (ResMut<FaunaWorld>, ResMut<VivariumWorld>, ResMut<EcologyWorld>),
    (mut bautizo, mut tirados): (ResMut<crate::bautizo::Bautizo>, ResMut<crate::tirados::Tirados>),
    player: Query<&Transform, (With<Player>, Without<Animal>)>,
    mut animals: Query<(&mut Animal, &mut Needs, Option<&Resident>)>,
) {
    use crate::hud::inventory_ui::{consume_from_ui_slots, flush_slots_to_inventory};
    if target.kind != Some(ha_shared::InteractTargetKind::Animal) {
        return;
    }
    // Un TOQUE (soltar antes de llenar la barra) vale lo mismo que el mantenido:
    // golpear un pollo a toques es como se juega en Minecraft.
    let tocado = std::mem::take(&mut hold.tapped);
    if !hold.completed && !tocado {
        return;
    }
    hold.completed = false;
    let Some(entity) = target.entity else { return };
    let body = active_surface_body(&ctx, &planet);
    let (px, pz) = player
        .get_single()
        .map(|t| (t.translation.x, -t.translation.y))
        .unwrap_or_default();
    let item = equipped.item_id.clone();
    let mut semilla = (time.elapsed_secs() * 1000.0) as u32 ^ entity.index().wrapping_mul(2654435761);
    let mut gastar = |item: &str,
                      inv: &mut crate::inventory_persist::PlayerInventory,
                      inv_ui: &mut crate::hud::inventory_ui::InventoryUiState,
                      persist: &mut crate::inventory_persist::InventoryPersistState,
                      equipped: &mut crate::input::EquippedItem| {
        consume_from_ui_slots(inv_ui, &[(item, 1)]);
        flush_slots_to_inventory(inv_ui, inv);
        persist.mark_immediate();
        if inv.qty(item) == 0 {
            equipped.item_id = "hand".to_string();
        }
    };

    // Los perros salvajes de alrededor, por si hay que enojarlos a todos (la
    // jauría de Minecraft). Se juntan antes del préstamo mutable.
    let jauria: Vec<(f32, f32)> = animals
        .iter()
        .filter(|(a, ..)| a.art_key == "npc/fauna_dog" && !a.domado)
        .map(|(a, ..)| (a.x, a.z))
        .collect();

    let Ok((mut a, mut n, resident)) = animals.get_mut(entity) else { return };
    let (x, z) = (a.x, a.z);
    let es_perro = a.art_key == "npc/fauna_dog";
    let come = comida_de_cria(&a.art_key, a.hostile);

    // 0) Cubo vacío a una vaca o cabra adulta: leche (Minecraft).
    if item == "bucket"
        && inv.qty("bucket") > 0
        && a.edad >= 1.0
        && matches!(a.art_key.as_str(), "npc/fauna_cow" | "npc/fauna_goat")
    {
        use crate::hud::inventory_ui::add_item_to_slots;
        consume_from_ui_slots(&mut inv_ui, &[("bucket", 1)]);
        add_item_to_slots(&mut inv_ui, "milk_bucket", 1);
        flush_slots_to_inventory(&inv_ui, &mut inv);
        persist.mark_immediate();
        if inv.qty("bucket") == 0 {
            equipped.item_id = "milk_bucket".to_string();
        }
        crate::granja::globo(&mut commands, x, z, format!("Ordeñaste a {}", a.nombre_propio.clone().unwrap_or(a.nombre.to_lowercase())));
        return;
    }
    // 1) Etiqueta: abrir el panel del nombre (la etiqueta se gasta al confirmar).
    if item == "name_tag" && inv.qty("name_tag") > 0 {
        bautizo.abrir(entity, a.nombre_propio.as_deref());
        return;
    }
    // 2) Hueso a un perro salvaje: domarlo, uno de cada tres (el lobo).
    if es_perro && !a.domado && item == "bone" && inv.qty("bone") > 0 {
        gastar("bone", &mut inv, &mut inv_ui, &mut persist, &mut equipped);
        if azar(&mut semilla) < 1.0 / 3.0 {
            a.domado = true;
            a.enojado = false;
            a.sentado = false;
            a.corazones_t = 2.5;
            domesticar(&mut commands, entity, &mut a, resident.is_some(), body, &mut world, &mut vivarium, &mut eco);
            crate::granja::globo(&mut commands, x, z, format!("¡{} domado! Ahora te sigue", a.nombre));
        } else {
            crate::granja::globo(&mut commands, x, z, "…lo olfatea y no se decide".to_string());
        }
        return;
    }
    // 3) Su comida: celo (adulto) o crecer (cría). Minecraft: un adulto que ya está
    //    en celo o descansando no la acepta (y no se gasta).
    if come.contains(&item.as_str()) && inv.qty(&item) > 0 {
        let msg = if a.edad < 1.0 {
            a.edad = (a.edad + 0.1).min(1.0);
            format!("{} crece ({} %)", a.nombre, (a.edad * 100.0).floor() as u32)
        } else if a.celo > 0.0 {
            crate::granja::globo(&mut commands, x, z, format!("{} ya está en celo", a.nombre));
            return;
        } else if a.cria_cd > 0.0 {
            crate::granja::globo(
                &mut commands,
                x,
                z,
                format!("{} descansa ({} s)", a.nombre, a.cria_cd.ceil() as u32),
            );
            return;
        } else {
            a.celo = CELO_SECS;
            format!("¡{} en celo!", a.nombre)
        };
        n.hunger = 0.0;
        a.vida = (a.vida + 4.0).min(a.vida_max);
        gastar(&item, &mut inv, &mut inv_ui, &mut persist, &mut equipped);
        domesticar(&mut commands, entity, &mut a, resident.is_some(), body, &mut world, &mut vivarium, &mut eco);
        crate::granja::globo(&mut commands, x, z, msg);
        return;
    }
    // 4) Tu perro: sentarse / seguirte. Nunca se le pega por error.
    if a.domado {
        a.sentado = !a.sentado;
        let msg = if a.sentado { "se sienta y espera" } else { "te sigue" };
        crate::granja::globo(&mut commands, x, z, format!("{} {msg}", a.nombre_propio.clone().unwrap_or(a.nombre.clone())));
        return;
    }
    // 5) El golpe.
    let dano = dano_de(&item);
    a.vida -= dano;
    a.herido_t = 0.35;
    let (dx, dz) = (a.x - px, a.z - pz);
    let d = (dx * dx + dz * dz).sqrt().max(0.1);
    // Retroceso: medio metro largo, lejos del golpe (lo frena el movimiento normal
    // si hay pared, que corre en el paso siguiente).
    a.x += dx / d * 0.7;
    a.z += dz / d * 0.7;
    if es_perro {
        a.enojado = true;
    } else {
        a.panico_t = 5.0;
    }
    if a.vida > 0.0 {
        if es_perro {
            // La jauría: los perros salvajes cerca también se enojan (abajo).
            drop(a);
            for (mut o, ..) in &mut animals {
                if o.art_key == "npc/fauna_dog"
                    && !o.domado
                    && jauria.iter().any(|&(jx, jz)| (jx - o.x).abs() < 0.01 && (jz - o.z).abs() < 0.01)
                    && (o.x - x).powi(2) + (o.z - z).powi(2) < 100.0
                {
                    o.enojado = true;
                }
            }
        }
        return;
    }
    // Murió: baja del censo (o del vivario) y suelta su botín en el piso.
    if resident.is_some() {
        vivarium.free_ids.push(a.brain);
        vivarium.alive = vivarium.alive.saturating_sub(1);
    } else {
        world.free_ids.push(a.brain);
        world.alive = world.alive.saturating_sub(1);
        eco.add_head(body, a.region.0, a.region.1, a.trophic, -1.0);
    }
    if a.edad >= 1.0 {
        for (it, q) in botin(&a.art_key, || azar(&mut semilla)) {
            crate::tirados::tirar(&mut tirados, a.x, a.z, it, q);
        }
    }
    if let Some(nombre) = &a.nombre_propio {
        crate::granja::globo(&mut commands, a.x, a.z, format!("{nombre} murió"));
    }
    commands.entity(entity).despawn_recursive();
}

/// El nombre confirmado en el panel: queda puesto, gasta la etiqueta y el animal
/// pasa a ser tuyo.
#[allow(clippy::too_many_arguments)]
fn aplicar_nombres(
    mut commands: Commands,
    mut eventos: EventReader<crate::bautizo::NombreElegido>,
    ctx: Res<WorldContextRuntime>,
    planet: Res<ActivePlanet>,
    mut equipped: ResMut<crate::input::EquippedItem>,
    (mut inv, mut inv_ui, mut persist): (
        ResMut<crate::inventory_persist::PlayerInventory>,
        ResMut<crate::hud::inventory_ui::InventoryUiState>,
        ResMut<crate::inventory_persist::InventoryPersistState>,
    ),
    (mut world, mut vivarium, mut eco): (ResMut<FaunaWorld>, ResMut<VivariumWorld>, ResMut<EcologyWorld>),
    mut animals: Query<(&mut Animal, Option<&Resident>)>,
) {
    use crate::hud::inventory_ui::{consume_from_ui_slots, flush_slots_to_inventory};
    let body = active_surface_body(&ctx, &planet);
    for ev in eventos.read() {
        let Ok((mut a, res)) = animals.get_mut(ev.animal) else { continue };
        if inv.qty("name_tag") == 0 {
            continue;
        }
        a.nombre_propio = Some(ev.nombre.clone());
        consume_from_ui_slots(&mut inv_ui, &[("name_tag", 1)]);
        flush_slots_to_inventory(&inv_ui, &mut inv);
        persist.mark_immediate();
        if inv.qty("name_tag") == 0 {
            equipped.item_id = "hand".to_string();
        }
        domesticar(&mut commands, ev.animal, &mut a, res.is_some(), body, &mut world, &mut vivarium, &mut eco);
        a.corazones_t = 1.5;
        crate::granja::globo(&mut commands, a.x, a.z, format!("Se llama {}", ev.nombre));
    }
}

/// Cartel del animal que estás tocando + la ayuda de qué hacer con él.
fn rotular_animal_apuntado(
    target: Res<crate::targeting::LookTarget>,
    equipped: Res<crate::input::EquippedItem>,
    mut ayuda: ResMut<AyudaAnimal>,
    mut q: Query<(&Animal, Option<&Resident>, &mut crate::structures::StructureName, Option<&Needs>)>,
) {
    let apuntado = target
        .entity
        .filter(|_| target.kind == Some(ha_shared::InteractTargetKind::Animal));
    let Some((a, res, mut nombre, needs)) = apuntado.and_then(|e| q.get_mut(e).ok()) else {
        if ayuda.0.is_some() {
            ayuda.0 = None;
        }
        return;
    };
    let mut rotulo = estado_animal(a, res.is_some());
    if let Some(nd) = needs {
        let clase = if a.hostile { Clase::Carnivoro } else { Clase::Herbivoro };
        if !a.enojado && a.celo <= 0.0 && !a.domado {
            rotulo.push_str(&format!(" · {}", nd.vida.actividad().verbo(clase).to_lowercase()));
        }
    }
    if nombre.0 != rotulo {
        nombre.0 = rotulo;
    }
    let item = equipped.item_id.as_str();
    let come = comida_de_cria(&a.art_key, a.hostile);
    let quien = a.nombre_propio.clone().unwrap_or_else(|| a.nombre.to_lowercase());
    let pide = come.iter().map(|i| crate::granja::nombre_item(i)).collect::<Vec<_>>().join(" o ");
    let texto = if item == "bucket" && matches!(a.art_key.as_str(), "npc/fauna_cow" | "npc/fauna_goat") && a.edad >= 1.0 {
        format!("Tocá para ordeñar a {quien}")
    } else if item == "name_tag" {
        format!("Mantené para ponerle nombre a {quien}")
    } else if a.art_key == "npc/fauna_dog" && !a.domado && item == "bone" {
        "Mantené para darle el hueso (domarlo)".to_string()
    } else if come.contains(&item) {
        if a.edad < 1.0 {
            format!("Mantené para dar {} a la cría", crate::granja::nombre_item(item))
        } else {
            format!("Mantené para dar {} (celo)", crate::granja::nombre_item(item))
        }
    } else if a.domado {
        if a.sentado {
            format!("Tocá a {quien} para que te siga")
        } else {
            format!("Tocá a {quien} para que se siente")
        }
    } else if come.is_empty() {
        format!("Tocá para golpear a {quien}")
    } else {
        format!("Tocá para golpear · come {pide}")
    };
    if ayuda.0.as_deref() != Some(texto.as_str()) {
        ayuda.0 = Some(texto);
    }
}

/// Corazones sobre los animales en celo o recién domados (uno cada ~0,7 s).
fn corazones(
    mut commands: Commands,
    time: Res<Time>,
    art: Res<crate::art2d::ArtLibrary>,
    animals: Query<(&Animal, &Visibility)>,
    mut vivos: Query<(Entity, &mut Corazon, &mut Transform, &mut Sprite)>,
    mut acc: Local<f32>,
) {
    let dt = time.delta_secs();
    for (e, mut c, mut tf, mut s) in &mut vivos {
        c.vida -= dt;
        if c.vida <= 0.0 {
            commands.entity(e).despawn_recursive();
            continue;
        }
        tf.translation.y += dt * 0.7;
        s.color.set_alpha(c.vida.min(1.0));
    }
    *acc += dt;
    if *acc < 0.7 {
        return;
    }
    *acc = 0.0;
    for (a, vis) in &animals {
        if (a.celo <= 0.0 && a.corazones_t <= 0.0) || *vis == Visibility::Hidden {
            continue;
        }
        let r = render_xy(a.x, a.z);
        commands.spawn((
            art.sprite_or_color("ui/corazon_celo", Color::srgb(0.85, 0.3, 0.35), Vec2::splat(0.55)),
            Transform::from_xyz(r.x + (a.brain as f32 * 0.37).sin() * 0.3, r.y + 1.3, crate::layers::GLOBO),
            Corazon { vida: 1.3 },
        ));
    }
}

/// El nombre flotando sobre cada animal bautizado.
#[allow(clippy::type_complexity)]
fn etiquetas_de_nombre(
    mut commands: Commands,
    nuevos: Query<(Entity, &Animal), Without<ConEtiqueta>>,
    duenos: Query<(&Animal, &Visibility, &Sprite), With<ConEtiqueta>>,
    mut etiquetas: Query<
        (Entity, &EtiquetaNombre, &mut Transform, &mut Text2d, &mut Visibility),
        (Without<Animal>, Without<Corazon>),
    >,
) {
    for (e, a) in &nuevos {
        let Some(n) = &a.nombre_propio else { continue };
        let et = commands
            .spawn((
                Text2d::new(n.clone()),
                TextFont { font_size: 22.0, ..default() },
                TextColor(Color::srgb(1.0, 0.95, 0.70)),
                Transform::from_xyz(a.x, -a.z, crate::layers::GLOBO),
                crate::textos_mundo::TextoEnMundo { px: 14.0, font_size: 22.0 },
                EtiquetaNombre(e),
                Name::new("EtiquetaNombre"),
            ))
            .id();
        commands.entity(e).try_insert(ConEtiqueta(et));
    }
    for (et, dueno, mut tf, mut texto, mut vis) in &mut etiquetas {
        let Ok((a, v, s)) = duenos.get(dueno.0) else {
            commands.entity(et).despawn_recursive();
            continue;
        };
        let Some(n) = &a.nombre_propio else {
            commands.entity(et).despawn_recursive();
            commands.entity(dueno.0).remove::<ConEtiqueta>();
            continue;
        };
        if texto.0 != *n {
            texto.0 = n.clone();
        }
        let alto = s.custom_size.map_or(1.0, |c| c.y) * escala_de_edad(a.edad);
        let r = render_xy(a.x, a.z);
        tf.translation.x = r.x;
        tf.translation.y = r.y + alto * 0.5 + 0.25;
        if *vis != *v {
            *vis = *v;
        }
    }
}

#[cfg(test)]
mod granja_tests {
    use super::*;

    /// Cada animal de granja de la Tierra come algo que la huerta produce (o que
    /// ya existe): si no, no hay forma de criarlo.
    #[test]
    fn cada_animal_de_la_tierra_tiene_su_comida() {
        let producibles = [
            "wheat", "wheat_seeds", "pumpkin_seeds", "carrot", "apple", "meat", "beef", "pork", "mutton",
            "chicken", "rabbit_meat",
        ];
        for def in earth_species() {
            let come = comida_de_cria(&def.key, false);
            assert!(!come.is_empty(), "{} no come nada", def.label);
            for c in come {
                assert!(producibles.contains(c), "{} come {c}, que no se consigue", def.label);
            }
        }
        assert!(comida_de_cria("npc/fauna_cow", true).is_empty(), "un depredador no se cría");
    }

    #[test]
    fn la_cria_nace_chica_y_crece_a_tamano_adulto() {
        assert_eq!(escala_de_edad(0.0), 0.5);
        assert_eq!(escala_de_edad(1.0), 1.0);
        // Diez comidas (10 % cada una) la hacen adulta, como en Minecraft.
        let mut edad = 0.0f32;
        for _ in 0..10 {
            edad = (edad + 0.1).min(1.0);
        }
        assert!(edad >= 0.999);
    }

    fn animal(key: &str) -> Animal {
        Animal {
            brain: 1,
            heading: 0.0,
            x: 0.0,
            z: 0.0,
            species: 0,
            region: (0, 0),
            trophic: Trophic::Herbivore,
            color: [1.0; 3],
            art_key: key.to_string(),
            hostile: false,
            relays_alarm: false,
            eat_t: 0.0,
            drink_t: 0.0,
            running: false,
            celo: 0.0,
            cria_cd: 0.0,
            edad: 1.0,
            nombre: "Bicho".into(),
            vida: vida_de(key),
            vida_max: vida_de(key),
            herido_t: 0.0,
            panico_t: 0.0,
            enojado: false,
            mordida_cd: 0.0,
            domado: false,
            sentado: false,
            nombre_propio: None,
            corazones_t: 0.0,
            huevo_t: HUEVO_MIN,
        }
    }

    /// **El botín de Minecraft**: la vaca da carne de res y cuero, el cerdo carne de
    /// cerdo, la gallina pollo y plumas, la oveja cordero y lana. El perro nada.
    #[test]
    fn cada_animal_suelta_lo_suyo() {
        let mut k = 0u32;
        let mut r = || {
            k = k.wrapping_add(7919);
            (k % 1000) as f32 / 1000.0
        };
        let mut junta = |key: &str, r: &mut dyn FnMut() -> f32| {
            let mut vistos = std::collections::HashSet::new();
            for _ in 0..200 {
                for (it, n) in botin(key, &mut *r) {
                    assert!(n >= 1);
                    vistos.insert(it);
                }
            }
            vistos
        };
        let vaca = junta("npc/fauna_cow", &mut r);
        assert!(vaca.contains("beef") && vaca.contains("leather") && vaca.contains("bone"));
        assert!(junta("npc/fauna_pig", &mut r).contains("pork"));
        let gallina = junta("npc/fauna_chicken", &mut r);
        assert!(gallina.contains("chicken") && gallina.contains("feather"));
        let oveja = junta("npc/fauna_sheep", &mut r);
        assert!(oveja.contains("mutton") && oveja.contains("wool"));
        assert!(junta("npc/fauna_dog", &mut r).is_empty(), "el perro no se come");
        // Y todo lo que suelta tiene ícono declarado.
        for key in ["npc/fauna_cow", "npc/fauna_pig", "npc/fauna_chicken", "npc/fauna_sheep", "npc/fauna_rabbit"] {
            for it in junta(key, &mut r) {
                assert!(ha_assets::slots::slot_label(&format!("items/{it}")).is_some(), "sin slot: {it}");
            }
        }
    }

    /// Una vaca muere de dos espadazos de acero o de diez puñetazos, como allá.
    #[test]
    fn el_dano_depende_de_lo_que_tenes_en_la_mano() {
        assert_eq!(dano_de("hand"), 1.0);
        assert!(dano_de("sword_steel") > dano_de("sword_iron"));
        assert!(dano_de("sword_iron") > dano_de("axe"));
        let vaca = vida_de("npc/fauna_cow");
        assert!((vaca / dano_de("sword_steel")).ceil() <= 2.0);
        assert_eq!((vaca / dano_de("hand")).ceil(), 10.0);
    }

    /// El perro enojado te persigue y muerde con pausa; el domado te sigue y,
    /// sentado, se queda; el golpeado huye.
    #[test]
    fn perros_y_panico() {
        let mut p = animal("npc/fauna_dog");
        p.enojado = true;
        p.x = 5.0;
        let c = conducta_granja(&mut p, 0.0, 0.0, 0.1);
        assert!(c.prisa > 1.0 && !c.muerde);
        assert!((p.heading - std::f32::consts::PI).abs() < 1e-3, "corre hacia vos");
        p.x = 1.0;
        assert!(conducta_granja(&mut p, 0.0, 0.0, 0.1).muerde);
        assert!(!conducta_granja(&mut p, 0.0, 0.0, 0.1).muerde, "no muerde en cada paso");

        let mut d = animal("npc/fauna_dog");
        d.domado = true;
        d.x = 10.0;
        assert!(conducta_granja(&mut d, 0.0, 0.0, 0.1).prisa > 1.0, "te alcanza");
        d.x = 100.0;
        assert!(conducta_granja(&mut d, 0.0, 0.0, 0.1).saltar_a.is_some(), "aparece a tu lado");
        d.sentado = true;
        assert!(conducta_granja(&mut d, 0.0, 0.0, 0.1).quieto);

        let mut v = animal("npc/fauna_cow");
        v.panico_t = 3.0;
        v.x = 2.0;
        conducta_granja(&mut v, 0.0, 0.0, 0.1);
        assert!(v.heading.abs() < 1e-3, "huye para el otro lado");
    }

    /// Los rangos de cerebros no se pisan: fauna 0..48, NPCs 72..120, residentes
    /// (granja) 200..264, compañero 990.
    #[test]
    fn los_cerebros_de_la_granja_no_pisan_a_nadie() {
        let res = RES_ID_BASE..RES_ID_BASE + MAX_RESIDENTS as i32;
        assert!(res.start >= 120 && res.end <= 990);
        assert!(res.end <= iafz_sys::MAX_AGENTS);
    }
}

/// Foto de un animal para los drivers de QA (`HA_GRANJATEST`).
pub(crate) struct AnimalProbe {
    pub(crate) entity: Entity,
    pub(crate) x: f32,
    pub(crate) z: f32,
    pub(crate) edad: f32,
    pub(crate) celo: f32,
    pub(crate) cria_cd: f32,
    pub(crate) residente: bool,
    pub(crate) especie: usize,
    pub(crate) granja: bool,
    pub(crate) vida: f32,
    pub(crate) enojado: bool,
    pub(crate) domado: bool,
    pub(crate) nombre: Option<String>,
    pub(crate) clave: String,
}

pub(crate) fn probe_animales(world: &mut World) -> Vec<AnimalProbe> {
    let mut q = world.query::<(Entity, &Animal, Has<Resident>, Has<DeGranja>)>();
    q.iter(world)
        .map(|(e, a, r, g)| AnimalProbe {
            granja: g,
            vida: a.vida,
            enojado: a.enojado,
            domado: a.domado,
            nombre: a.nombre_propio.clone(),
            clave: a.art_key.clone(),
            entity: e,
            x: a.x,
            z: a.z,
            edad: a.edad,
            celo: a.celo,
            cria_cd: a.cria_cd,
            residente: r,
            especie: a.species,
        })
        .collect()
}
