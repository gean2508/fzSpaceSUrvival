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

use std::collections::HashMap;
use std::path::PathBuf;

use bevy::prelude::*;
use serde::{Deserialize, Serialize};

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

const START_HERD: i32 = 12;
const MAX_POP: usize = 40; // tope de población (perf)
/// Radio (m) de "mundo vivo": la fauna que se aleja se REUBICA al borde opuesto →
/// siempre hay vida alrededor del jugador, población acotada (no drena FPS/batería).
const STREAM_R: f32 = 42.0;
const MAX_FAUNA_ID: i32 = 48; // ids de cerebro reservados a la fauna (0..48)
const FOOD_N: usize = 24;
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
const SPAWN_PALETTE_SLOTS: usize = 6;

/// Cuántos slots muestra la paleta creativa (para `hud.rs`). Genéricos y body-aware.
pub fn spawn_species_count() -> usize {
    SPAWN_PALETTE_SLOTS
}

/// Etiqueta ASCII del slot `i` (genérica: la especie real depende del planeta).
pub fn spawn_species_label(i: usize) -> &'static str {
    const L: [&str; SPAWN_PALETTE_SLOTS] =
        ["Fauna 1", "Fauna 2", "Fauna 3", "Fauna 4", "Fauna 5", "Fauna 6"];
    L.get(i).copied().unwrap_or("Fauna")
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

/// Fauna terrestre = pack **LimeZu Modern Farm** (amigable). El arte lo asigna el usuario
/// (poses del pack) → se anima sola; sin arte, quad de color.
fn earth_species() -> Vec<SpeciesDef> {
    let raw: &[(&str, [f32; 3], Vec2, &str)] = &[
        ("npc/fauna_cow", [0.85, 0.82, 0.78], Vec2::new(1.4, 1.1), "Vaca"),
        ("npc/fauna_chicken", [0.90, 0.88, 0.72], Vec2::new(0.6, 0.6), "Gallina"),
        ("npc/fauna_sheep", [0.88, 0.86, 0.82], Vec2::new(1.0, 0.9), "Oveja"),
        ("npc/fauna_pig", [0.82, 0.60, 0.58], Vec2::new(1.1, 0.9), "Cerdo"),
        ("npc/fauna_dog", [0.60, 0.48, 0.34], Vec2::new(0.9, 0.7), "Perro"),
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
}

/// Pool de ids de cerebro (C global) + censo. Reciclamos ids al morir.
#[derive(Resource)]
struct FaunaWorld {
    free_ids: Vec<i32>,
    alive: usize,
    ready: bool,
}

impl Default for FaunaWorld {
    fn default() -> Self {
        Self { free_ids: (0..MAX_FAUNA_ID).rev().collect(), alive: 0, ready: false }
    }
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
            .add_event::<SpawnResidentEvent>()
            .add_event::<SpawnFoodEvent>()
            .add_systems(
                Update,
                (
                    manage_vivarium,
                    spawn_fauna,
                    spawn_residents,
                    spawn_placed_food,
                    tick_fauna,
                    cull_residents,
                    vivarium_temperature,
                    save_vivarium_periodic,
                )
                    .chain(),
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

#[allow(clippy::too_many_arguments)]
fn spawn_fauna(
    mut commands: Commands,
    region: Res<RegionRuntime>,
    ctx: Res<WorldContextRuntime>,
    planet: Res<ActivePlanet>,
    art: Res<crate::art2d::ArtLibrary>,
    mut world: ResMut<FaunaWorld>,
    mut catalog: ResMut<FaunaCatalog>,
    player: Query<&Transform, With<Player>>,
    entities: Query<Entity, (Or<(With<Animal>, With<FoodDot>)>, Without<Resident>)>,
) {
    if surface_region_active(&region).is_none() {
        for e in &entities {
            commands.entity(e).despawn_recursive();
        }
        *world = FaunaWorld::default(); // pool ambiental (los residentes los maneja manage_vivarium)
        return;
    }
    if world.ready {
        return;
    }
    let Ok(pt) = player.get_single() else {
        return;
    };
    world.ready = true;
    let (px, pz) = (pt.translation.x, -pt.translation.y);
    // F5: fauna del PLANETA donde estás (no vacas en Marte).
    let body = active_surface_body(&ctx, &planet);
    let defs = catalog.get(body).to_vec();
    if defs.is_empty() {
        return;
    }

    for i in 0..FOOD_N {
        let (dx, dz) = scatter(0xF00D, i, 24.0);
        let (x, z) = (px + dx, pz + dz);
        commands.spawn((
            FoodDot { x, z },
            Sprite::from_color(Color::srgb(0.85, 0.25, 0.35), Vec2::splat(0.5)),
            Transform::from_xyz(x, -z, sort_z(z) - 0.1),
        ));
    }
    for i in 0..START_HERD {
        let Some(brain) = world.free_ids.pop() else { break };
        iafz_sys::load_bytes(brain, GENOME);
        iafz_sys::reset_echo(brain);
        world.alive += 1;
        let (dx, dz) = scatter(0xA417, i as usize, 8.0);
        let (x, z) = (px + dx, pz + dz);
        let idx = i as usize % defs.len();
        spawn_animal(&mut commands, &art, brain, x, z, dz.atan2(dx), 1.0, idx, &defs[idx]);
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
        let e = spawn_animal(&mut commands, &art, brain, ev.x, ev.z, heading, 0.85, ev.species, &def);
        commands.entity(e).insert(Resident { uid });
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
            FoodDot { x: ev.x, z: ev.z },
            PlacedFood,
            Sprite::from_color(Color::srgb(0.35, 0.85, 0.35), Vec2::splat(0.6)),
            Transform::from_xyz(r.x, r.y, sort_z(ev.z) - 0.1),
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
                commands.entity(entity).insert(anim);
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
    player: Query<&Transform, (With<Player>, Without<Animal>, Without<FoodDot>)>,
    mut animals: Query<(Entity, &mut Animal, &mut Needs, &mut Transform, &mut Sprite, &mut FaunaPose, Option<&Resident>), (Without<FoodDot>, Without<Player>)>,
    mut foods: Query<(&mut FoodDot, &mut Transform, Option<&PlacedFood>), (Without<Animal>, Without<Player>)>,
    mut acc: Local<f32>,
) {
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
    // Centro del "mundo vivo" = el jugador (para el streaming de fauna/comida).
    let (px, pz) = player
        .get_single()
        .map(|t| (t.translation.x, -t.translation.y))
        .unwrap_or((0.0, 0.0));
    let food_pos: Vec<(f32, f32)> = foods.iter().map(|(f, ..)| (f.x, f.z)).collect();
    let neigh: Vec<(f32, f32)> = animals.iter().map(|(_, a, ..)| (a.x, a.z)).collect();
    // Presas (no hostiles) y depredadores (hostiles) para caza/predación.
    let prey_pos: Vec<(f32, f32)> =
        animals.iter().filter(|(_, a, ..)| !a.hostile).map(|(_, a, ..)| (a.x, a.z)).collect();
    // Sólo los depredadores HAMBRIENTOS cazan → menos muertes "sin sentido" (un
    // depredador saciado no mata por matar; la presa puede pasar cerca y vivir).
    let predator_pos: Vec<(f32, f32)> = animals
        .iter()
        .filter(|(_, a, n, ..)| a.hostile && n.hunger > 0.45)
        .map(|(_, a, ..)| (a.x, a.z))
        .collect();

    let mut deaths: Vec<(Entity, i32, bool)> = Vec::new(); // entity, brain, is_resident
    let mut births: Vec<(f32, f32, i32, f32, usize, bool)> = Vec::new(); // x,z,parent,heading,species,is_res
    // Censo para la predación post-loop: (entity, x, z, hostil, es_residente, brain).
    let mut census: Vec<(Entity, f32, f32, bool, bool, i32)> = Vec::new();

    for (e, mut a, mut n, mut tf, mut sprite, mut pose, resident) in &mut animals {
        let is_res = resident.is_some();
        // F2: un residente LEJOS del jugador queda CONGELADO (no piensa/mueve/gasta) — sigue
        // como dato en su lugar y `cull_residents` lo oculta; revive al acercarte. Así "no
        // desaparece mi granja" pero tampoco cuesta cómputo/batería estando lejos.
        if is_res {
            let dr2 = (a.x - px) * (a.x - px) + (a.z - pz) * (a.z - pz);
            if dr2 > RENDER_R * RENDER_R {
                continue;
            }
        }
        // Psicología / metabolismo: hambre y SED suben; la sed acelera el desgaste
        // (deshidratación) → el animal necesita beber.
        n.hunger = (n.hunger + dt * 0.02).min(1.0);
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
        // Autoridad de giro REDUCIDA: con el steer completo daban vueltas en el
        // sitio cuando no había comida cerca. A la mitad hacen arcos amplios y
        // recorren el mundo (siguen buscando comida por el sensor de dirección).
        a.heading += steer * TURN * dt * 0.5;

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

        // Comer.
        for (mut f, mut ftf, placed) in &mut foods {
            let (dx, dz) = (a.x - f.x, a.z - f.z);
            if dx * dx + dz * dz < EAT_D2 {
                n.hunger = (n.hunger - 0.5).max(0.0);
                n.energy = (n.energy + 0.3).min(1.0);
                a.eat_t = 0.5; // animación "eat"
                // F8: el feeder COLOCADO no se agota ni se mueve (fuente permanente); la
                // comida ambiental se reubica al ser comida.
                if placed.is_none() {
                    let (ox, oz) = scatter(a.x.to_bits() ^ f.z.to_bits(), a.brain as usize, 20.0);
                    f.x = a.x + ox;
                    f.z = a.z + oz;
                    let r = render_xy(f.x, f.z);
                    ftf.translation = Vec3::new(r.x, r.y, sort_z(f.z) - 0.1);
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

        // Censo para la predación post-loop (posición ya movida).
        census.push((e, a.x, a.z, a.hostile, is_res, a.brain));

        // --- Ciclo de vida: reproducirse → EVOLUCIÓN. La muerte NO es por inanición (hay
        // piso de energía); llega por PREDACIÓN (post-loop) o el jugador. Esta rama sólo
        // dispara si algo externo baja la energía a 0 (futuro combate). ---
        if n.energy <= 0.0 {
            deaths.push((e, a.brain, is_res));
        } else if n.energy >= REPRO_ENERGY {
            // Cada categoría se reproduce contra SU pool/censo (ambiental vs. residente),
            // reservando espacio por los nacimientos ya encolados de esa misma categoría.
            let can = if is_res {
                let pending = births.iter().filter(|b| b.5).count();
                vivarium.alive + pending < MAX_RESIDENTS && vivarium.free_ids.len() > pending
            } else {
                let pending = births.iter().filter(|b| !b.5).count();
                world.alive + pending < MAX_POP && world.free_ids.len() > pending
            };
            if can {
                n.energy -= REPRO_COST;
                births.push((a.x, a.z, a.brain, a.heading, a.species, is_res));
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

        // MUNDO VIVO (sólo ambiental): si se alejó del radio, reubicarlo al borde
        // (siempre hay vida alrededor sin spawnear de más). Los RESIDENTES no se
        // streamean — viven donde los pusiste, confinados por paredes/puertas.
        if !is_res {
            let (rx, rz) = (a.x - px, a.z - pz);
            if rx * rx + rz * rz > STREAM_R * STREAM_R {
                let ang = (a.brain as f32) * 2.399963 + time.elapsed_secs() * 0.7;
                a.x = px + ang.cos() * STREAM_R * 0.9;
                a.z = pz + ang.sin() * STREAM_R * 0.9;
                a.heading = ang + std::f32::consts::PI; // mirando hacia adentro
            }
        }

        // Render: posición + tinte por energía (vivo→apagado).
        let r = render_xy(a.x, a.z);
        tf.translation = Vec3::new(r.x, r.y, sort_z(a.z));
        let en = 0.45 + 0.55 * n.energy;
        let c = a.color;
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
    if !predator_pos.is_empty() {
        for &(e, x, z, hostile, is_res, brain) in &census {
            if hostile {
                continue;
            }
            let caught = predator_pos
                .iter()
                .any(|&(qx, qz)| (qx - x) * (qx - x) + (qz - z) * (qz - z) < PREDATION_D2);
            if caught {
                deaths.push((e, brain, is_res));
            }
        }
    }

    // La comida ambiental se streamea: las bayas lejanas reaparecen cerca del jugador.
    // El feeder COLOCADO (F8) queda fijo donde lo pusiste.
    for (mut f, mut ftf, placed) in &mut foods {
        if placed.is_some() {
            continue;
        }
        let (rx, rz) = (f.x - px, f.z - pz);
        if rx * rx + rz * rz > STREAM_R * STREAM_R {
            let ang = (f.x.to_bits() ^ f.z.to_bits()) as f32 * 0.001 + time.elapsed_secs();
            f.x = px + ang.cos() * STREAM_R * 0.8;
            f.z = pz + ang.sin() * STREAM_R * 0.8;
            let r = render_xy(f.x, f.z);
            ftf.translation = Vec3::new(r.x, r.y, sort_z(f.z) - 0.1);
        }
    }

    // Muertes: liberan su id de cerebro al pool correcto.
    for (e, brain, is_res) in deaths {
        commands.entity(e).despawn_recursive();
        if is_res {
            vivarium.free_ids.push(brain);
            vivarium.alive = vivarium.alive.saturating_sub(1);
        } else {
            world.free_ids.push(brain);
            world.alive = world.alive.saturating_sub(1);
        }
    }
    // Nacimientos: el hijo HEREDA el genoma del padre con mutación (evolución).
    for (x, z, parent, heading, species, is_res) in births {
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
            let e = spawn_animal(&mut commands, &art, child, x + ox, z + oz, heading + 1.0, 0.55, species, def);
            commands.entity(e).insert(Resident { uid });
        } else {
            let Some(child) = world.free_ids.pop() else { continue };
            iafz_sys::mutate(parent, child, MUTATE_RATE); // copia padre→hijo + muta
            iafz_sys::reset_echo(child);
            world.alive += 1;
            let def = &defs[species % defs.len()];
            spawn_animal(&mut commands, &art, child, x + ox, z + oz, heading + 1.0, 0.55, species, def);
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
            );
            commands.entity(e).insert((
                Resident { uid: rec.uid },
                Needs { hunger: rec.hunger, thirst: rec.thirst, energy: rec.energy },
            ));
        }
    }
    vivarium.active_body = current;
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

fn update_vivarium_overlay(
    editor: Res<crate::ui_editor::UiEditorState>,
    vivarium: Res<VivariumWorld>,
    residents: Query<&Needs, With<Resident>>,
    mut overlay: Query<(&mut Text, &mut Visibility), With<VivariumOverlay>>,
) {
    let Ok((mut text, mut vis)) = overlay.get_single_mut() else {
        return;
    };
    if !editor.creative {
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
    let line = format!(
        "VIVARIO  vivos {n}  nac {}  temp {:.1}  div {:.0}%  energia {:.0}%",
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

/// F2 — Oculta los residentes lejos del jugador (se congelan en `tick_fauna`); visibles
/// cerca. "Se renderizan sólo cuando estás cerca." Barato (≤ `MAX_RESIDENTS`).
fn cull_residents(
    player: Query<&Transform, (With<Player>, Without<Resident>)>,
    mut residents: Query<(&Animal, &mut Visibility), With<Resident>>,
) {
    let Ok(pt) = player.get_single() else {
        return;
    };
    let (px, pz) = (pt.translation.x, -pt.translation.y);
    for (a, mut vis) in &mut residents {
        let dr2 = (a.x - px) * (a.x - px) + (a.z - pz) * (a.z - pz);
        let want = if dr2 > RENDER_R * RENDER_R {
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
    // Cuerpo actual: residentes vivos.
    if surface_region_active(&region).is_some() {
        let b = active_surface_body(&ctx, &planet) as u8;
        for a in &live {
            sightings
                .fauna
                .entry(b)
                .or_default()
                .push(Sighting { x: a.x, z: a.z, color: a.color, hostile: a.hostile });
        }
    }
}
