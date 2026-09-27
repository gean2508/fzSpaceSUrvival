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
const MAX_RESIDENTS: usize = 24;
/// Rango de ids de cerebro reservado a residentes (ambiental usa `0..MAX_FAUNA_ID`).
const RES_ID_BASE: i32 = MAX_FAUNA_ID;
/// Salvaguarda anti mínimos locales: cada N nacimientos, el hijo nace del SEED del gym
/// (inmigración) en vez del padre → reinyecta diversidad perdida (PLAN_VIVARIO §4.3).
const RES_SEED_EVERY: u32 = 6;
/// Hipermutación esporádica (§4.2): tasa ×4 para saltos exploratorios.
const RES_HYPERMUTATE_RATE: f32 = MUTATE_RATE * 4.0;
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
        ("npc/fauna_cow", [0.85, 0.82, 0.78], Vec2::new(1.96, 1.78), "Vaca"),
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
struct Animal {
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
}

/// Psicología: necesidades que sesgan la conducta y gatillan reproducción/muerte.
#[derive(Component)]
struct Needs {
    hunger: f32,
    thirst: f32,
    energy: f32,
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
        app.init_resource::<FaunaWorld>()
            .init_resource::<VivariumWorld>()
            .init_resource::<FaunaCatalog>()
            .init_resource::<SignalField>()
            .init_resource::<EcologyWorld>()
            .add_event::<SpawnResidentEvent>()
            .add_event::<SpawnFoodEvent>()
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
        spawn_animal(commands, &art, brain, x, z, heading, 0.8, idx, &def, (rx, rz), trophic);
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
            },
            Needs { hunger: 0.3, thirst: 0.3, energy },
            FaunaPose::default(),
            sprite,
            Transform::from_xyz(x, -z, sort_z(z)),
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
    mut animals: Query<(Entity, &mut Animal, &mut Needs, &mut Transform, &mut Sprite, &mut FaunaPose, Option<&Resident>), (Without<FoodDot>, Without<Player>)>,
    foods: Query<(Entity, &FoodDot, Option<&PlacedFood>), (Without<Animal>, Without<Player>)>,
    mut acc: Local<f32>,
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

    for (e, mut a, mut n, mut tf, mut sprite, mut pose, resident) in &mut animals {
        let is_res = resident.is_some();
        // F2: un residente LEJOS del jugador queda CONGELADO (no piensa/mueve/gasta) — sigue
        // como dato en su lugar y `cull_fauna` lo oculta; revive al acercarte. Así "no
        // desaparece mi granja" pero tampoco cuesta cómputo/batería estando lejos.
        if is_res {
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
            for &(fx, fz) in &food_pos {
                let d2 = (fx - a.x) * (fx - a.x) + (fz - a.z) * (fz - a.z);
                if d2 < mejor {
                    mejor = d2;
                    rumbo = (fz - a.z).atan2(fx - a.x);
                }
            }
            rumbo
        };
        // El cerebro abre hasta +-120 grados respecto de esa referencia: puede
        // ir derecho a la comida, rodearla, o darle la espalda para explorar.
        // La decisión sigue siendo suya; lo que ya no puede es girar sin parar.
        let deseado = referencia + steer * 2.1;
        let diff = (deseado - a.heading + std::f32::consts::PI)
            .rem_euclid(std::f32::consts::TAU)
            - std::f32::consts::PI;
        a.heading += diff.clamp(-1.0, 1.0) * TURN * dt;

        // DEPREDADOR (hostil): CAZA. Persigue a la presa más cercana si hay una a la vista;
        // si no, acecha al jugador (F5). La caza es la fuente de muerte VISIBLE del
        // ecosistema (reemplaza a la inanición) → selección para evolucionar + gameplay.
        if a.hostile {
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

        // HUIDA: si siente ALARMA cerca (bit 17 de la estigmergia) → corre.
        a.running = (s & (1 << 17)) != 0;
        let speed = VEL
            * (0.4 + 0.6 * n.energy)
            * if a.running {
                1.9
            } else if n.hunger > 0.6 || n.thirst > 0.6 {
                1.2
            } else {
                1.0
            };
        let (nx, nz) = (a.x + a.heading.cos() * speed * dt, a.z + a.heading.sin() * speed * dt);
        // Las paredes construidas (bordes) también las encierran — es lo que
        // hace que el corral del vivario funcione.
        let hits_wall = ha_shared::build_grid::wall_capsule_hits(nx, nz, &|tx, tz, axis| {
            walls.blocks(body, tiles.z, tx, tz, axis)
        });
        if hits_wall || tiles.solid_at(body, nx.floor() as i64, nz.floor() as i64) {
            a.heading += 1.3;
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
        } else if n.energy >= REPRO_ENERGY {
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
        sprite.color = Color::srgb(c[0] * en, c[1] * en, c[2] * en);

        // Animación LimeZu-compatible: pose DIRECCIONAL según el estado (eat/drink/
        // run/idle/walk). Si aún no asignaste las poses del pack, se ve el quad de
        // color (fallback) orientado por flip.
        let key = a.art_key.clone();
        if art.has_poses(&key) {
            let verb = if a.eat_t > 0.0 {
                "eat"
            } else if a.drink_t > 0.0 {
                "drink"
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
            tf.scale.x = if a.heading.cos() < 0.0 { -1.0 } else { 1.0 };
        }
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
                ecology::region_of(cx, cz), trophic_of(def),
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
    live: Query<(&Animal, &Needs, &Resident)>,
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
        for (a, n, res) in &live {
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
                ecology::region_of(rec.x, rec.z), trophic_of(def),
            );
            commands.entity(e).try_insert((
                Resident { uid: rec.uid },
                Needs { hunger: rec.hunger, thirst: rec.thirst, energy: rec.energy },
            ));
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
    live: Query<(&Animal, &Needs, &Resident)>,
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
        for (a, n, res) in &live {
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
