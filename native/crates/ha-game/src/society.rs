//! F6 — Sociedad viva con MEMORIA. Colonos/NPCs con cerebro IAFZ + **familia** + una
//! **reputación del jugador POR FAMILIA que persiste**: si matás a un miembro, su familia
//! te lo recuerda (rencor) y su conducta reacciona. Reusa el patrón del vivario
//! (`fauna.rs`): activación por cuerpo + persistencia. Ver `PLAN_SOCIEDAD_MEMORIA.md`.
//! No podar lo emergente: la memoria abre reacciones, no impone guiones.

use std::collections::HashMap;
use std::path::PathBuf;

use bevy::prelude::*;
use serde::{Deserialize, Serialize};

use ha_shared::fauna_brain::sense;
use ha_shared::CelestialBody;

use crate::input::{AbilityTapPulse, AimCursor};
use crate::platform::is_app_paused;
use crate::scene::{RegionRuntime, WorldContextRuntime};
use crate::sim::Player;
use crate::world2d::{
    active_surface_body, body_from_u8, render_xy, sort_z, surface_region_active, ActivePlanet,
    TileWorld,
};

/// Cerebro SEMILLA (mismo que la fauna: base de movimiento/forrajeo; la conducta social
/// sale de la reputación + estigmergia, no de un genoma distinto — de momento).
const GENOME: &[u8] = include_bytes!("../../iafz-sys/genomes/FAUNA.adn");

const NPC_ID_BASE: i32 = 72; // fauna 0..48, residentes 48..72, NPCs 72..120
const MAX_NPCS: usize = 48;
const NPC_VEL: f32 = 2.2;
const NPC_TURN: f32 = 2.0;
const RENDER_R: f32 = 80.0; // render/sim sólo cerca (como los residentes)
const FAMILY_SIZE: u32 = 3; // NPCs por familia al sembrar la sociedad
const HARM_RADIUS: f32 = 2.6; // radio del "golpe" alrededor del cursor de apuntado
const WITNESS_RADIUS: f32 = 12.0; // testigos que también recuerdan
const REP_DEFAULT: f32 = 0.15; // disposición inicial (levemente cordial)
const REP_KILL_FAMILY: f32 = -0.7; // rencor por matar a un miembro
const REP_WITNESS: f32 = -0.2; // los testigos (otras familias) también recuerdan
const VIV_SAVE_EVERY: f32 = 20.0;
// F6.2 evolución
const NPC_REPRO_ENERGY: f32 = 0.97; // se reproduce bien alimentado
const NPC_REPRO_COST: f32 = 0.5;
const NPC_MUTATE: f32 = 0.05;
// F6.3 roles/hostilidad de grupo + recambio generacional
const AGGRO_THRESHOLD: f32 = -0.55; // rep por debajo → la familia te ENFRENTA (no huye)
const FRIENDLY_THRESHOLD: f32 = 0.15;
// --- F7 RELIGIÓN + propósito (role-play) ---
const NUM_SKINS: u64 = 20; // hojas horneadas por el Character Generator (npc/char_1..20)
/// Tamaño en mundo del aldeano: el MISMO que el jugador (`scene.rs`), porque
/// ahora comparten hoja y frame (16×32 = 1:2). Un humano de ~1,9 m.
const NPC_SIZE: Vec2 = Vec2::new(0.95, 1.9);
const HOME_WANDER_R: f32 = 5.0; // radio de deambular alrededor del hogar (pueblo)
const IDOL_RADIUS: f32 = 30.0; // "congregación": la fe se contagia por el pueblo
const IDOL_ORBIT: f32 = 2.8; // los fieles se juntan a ~este radio del ídolo
const FAITH_GAIN: f32 = 0.09; // fe/s cerca del ídolo (convierte en ~7 s)
/// D8: única definición — `npc_menu.rs` la importa en vez de duplicarla.
pub(crate) const CONVERT_AT: f32 = 0.6; // fe para convertirse en fiel
const FOUND_MIN_FLOCK: usize = 3; // NPCs cerca para fundar la religión
const FOUND_RADIUS: f32 = 13.0;
const SAY_COOLDOWN: f32 = 7.0; // charla ambiental por NPC
// --- S2: higiene de gameplay (auditoría PLAN_NPCS_Y_SOCIEDAD.md) ---
const ATTACK_DAMAGE: f32 = 0.5; // dos golpes para matar (C1)
const PLAYER_REACH: f32 = 3.2; // el golpe sólo cuenta si estás a esta distancia real
pub(crate) const CALM_COOLDOWN: f32 = 120.0; // s de mundo entre usos de CALMAR por NPC (C2)
pub(crate) const CALM_STEP: f32 = 0.15;
pub(crate) const CALM_CEILING: f32 = -0.1; // hablando no pasa de acá; para mejorar hay que HACER algo
pub(crate) const CONVERT_COOLDOWN: f32 = 60.0; // s de mundo entre intentos de CONVERTIR (C3)
/// C3: no se puede convertir a quien te enfrenta — sólo NPCs que no son hostiles.
pub(crate) const CONVERT_MIN_REP: f32 = AGGRO_THRESHOLD;
pub(crate) const TALK_RANGE: f32 = 6.0; // D6: el menú se cierra si te alejás más que esto

// --- S5/S6: economía de colonia (necesidades reales + roles por escasez) ---
/// Cada cuánto tiempo de MUNDO corre el tick abstracto de colonia (LOD 2 del plan: la
/// producción/consumo/roles avanzan aunque no estés parado en ese cuerpo — sólo el
/// render/cerebro por-entidad se congela lejos, la ECONOMÍA no).
const COLONY_TICK: f32 = 30.0;
const RATION: f32 = 0.05; // comida que consume un colono por tick
const PROD_FARMER: f32 = 0.30; // comida producida por un Farmer por tick
const PROD_MINER: f32 = 0.22; // repartido 60% madera / 40% piedra
/// Cuántos ticks de reserva de comida cuentan como colonia "sana" (satura la salud).
const HEALTH_RESERVE_TICKS: f32 = 5.0;
/// Salud mínima de la colonia para permitir un nacimiento (B4: antes era un timer de
/// energía individual que se disparaba para siempre sin importar si había con qué comer).
const BIRTH_MIN_HEALTH: f32 = 0.5;
const BUILD_WOOD_COST: f32 = 12.0; // madera que hace falta acumulada para tener un Builder
pub(crate) const TRADE_AMOUNT: f32 = 5.0;
pub(crate) const TRADE_MIN_RESERVE: f32 = 6.0; // el trato nunca deja el stock por debajo de esto
pub(crate) const TRADE_APPLE_COST: u32 = 2;

// --- S5: necesidades + rutina diaria (PLAN_NPCS_Y_SOCIEDAD.md §3.4) --------------
// Fase del día = `sky2d::DayClock::phase()` (0..1, 0=medianoche, 0.5=mediodía). El
// pueblo deja de ser un enjambre que deambula igual a toda hora: sale a trabajar, se
// junta a mediodía, y de noche vuelve a casa y no charla — B8 de la auditoría.
const ROUTINE_WORK_START: f32 = 0.25;
const ROUTINE_PLAZA_START: f32 = 0.45;
const ROUTINE_WORK2_START: f32 = 0.55;
const ROUTINE_DUSK_START: f32 = 0.78;
const ROUTINE_NIGHT_START: f32 = 0.86;
const WORK_DIST: f32 = 8.0; // qué tan lejos de casa está el "puesto de trabajo"
const PLAZA_ARRIVE_R: f32 = 4.0; // radio para contar "llegó a la plaza"
const NEEDS_FOOD_DECAY: f32 = 0.0025; // se vacía trabajando ~7 min sin volver
const NEEDS_REST_DECAY: f32 = 0.002; // se cansa despierto
const NEEDS_BELONGING_DECAY: f32 = 0.0015; // se siente solo lejos del pueblo
const NEEDS_PLAZA_GAIN: f32 = 0.05; // sube junto a otros en la plaza a mediodía
const NEEDS_HOME_REST_GAIN: f32 = 0.06; // descansa durmiendo en casa de noche
const NEEDS_WORK_FOOD_GAIN: f32 = 0.05; // "come" de lo que produce en su puesto

/// `[0,1]`: comida/descanso/pertenencia. No matan (regla del vivario: nadie se esfuma
/// por metabolismo) — sesgan la conducta (velocidad, charla) y son lo que la rutina
/// diaria satisface o no según el NPC cumpla su horario.
#[derive(Clone, Copy, Serialize, Deserialize, Debug)]
pub(crate) struct Needs {
    pub(crate) food: f32,
    pub(crate) rest: f32,
    pub(crate) belonging: f32,
}

impl Default for Needs {
    fn default() -> Self {
        Self { food: 1.0, rest: 1.0, belonging: 0.5 }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum RoutinePhase {
    GoToWork,
    Plaza,
    Work,
    GoHome,
    Sleep,
}

fn routine_phase(p: f32) -> RoutinePhase {
    if p < ROUTINE_WORK_START {
        RoutinePhase::Sleep
    } else if p < ROUTINE_PLAZA_START {
        RoutinePhase::GoToWork
    } else if p < ROUTINE_WORK2_START {
        RoutinePhase::Plaza
    } else if p < ROUTINE_DUSK_START {
        RoutinePhase::Work
    } else if p < ROUTINE_NIGHT_START {
        RoutinePhase::GoHome
    } else {
        RoutinePhase::Sleep
    }
}

/// Puesto de trabajo determinista (uid+family): sin edificios propios todavía, pero
/// estable — el mismo aldeano siempre "trabaja" en el mismo punto cerca de casa.
fn workplace_of(home_x: f32, home_z: f32, uid: u64, family: u32) -> (f32, f32) {
    let a = (uid as f32) * 2.399_963_2 + (family as f32) * 0.7;
    (home_x + a.cos() * WORK_DIST, home_z + a.sin() * WORK_DIST)
}

/// Oficio elegido por ESCASEZ (nunca a mano) — sólo se asigna a quien está `Wanderer`,
/// así un pueblo no rota de trabajo todo el tiempo. `ai-life-emergence-no-prune`: la
/// necesidad abre el rol, no un guion.
#[derive(Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Debug)]
pub(crate) enum Role {
    Wanderer,
    Farmer,
    Miner,
    Builder,
}

impl Default for Role {
    fn default() -> Self {
        Role::Wanderer
    }
}

pub(crate) fn role_label(role: Role) -> &'static str {
    match role {
        Role::Wanderer => "SIN OFICIO",
        Role::Farmer => "GRANJERO",
        Role::Miner => "MINERO",
        Role::Builder => "CONSTRUCTOR",
    }
}

/// Recursos acumulados de una colonia (por cuerpo). `S6`: reemplaza el contador
/// desconectado de `colony.rs` para los NPCs visuales — esto es lo que producen/consumen
/// de verdad los colonos con rol.
#[derive(Clone, Copy, Default, Serialize, Deserialize)]
pub(crate) struct Stock {
    pub(crate) food: f32,
    pub(crate) wood: f32,
    pub(crate) stone: f32,
}

/// `0..1`: cuánta reserva de comida tiene la colonia relativa a su población — la
/// natalidad y (más adelante) el humor dependen de esto, no de un timer fijo.
pub(crate) fn colony_health(stock: Stock, population: u32) -> f32 {
    if population == 0 {
        return 1.0;
    }
    let reserve_ticks = stock.food / (population as f32 * RATION).max(1e-4);
    (reserve_ticks / HEALTH_RESERVE_TICKS).clamp(0.0, 1.0)
}

/// Un paso de economía: producción/consumo por rol, y reasignación de los `Wanderer`
/// (sólo ellos) por escasez. Determinista — no usa `rand`, sólo el estado del stock y la
/// posición de cada uno en la lista. Reparte ~60/40 Farmer/Miner entre los recién
/// asignados (si tiráramos TODOS los Wanderer al déficit más grande, un pueblo nuevo se
/// vuelve 100% granjero y nunca junta madera para tener un Constructor); si la comida se
/// pone crítica, la emergencia manda y todos los libres van a granjear.
fn colony_step(stock: &mut Stock, roles: &mut [Role]) {
    let pop = roles.len() as f32;
    if pop <= 0.0 {
        return;
    }
    let farmers = roles.iter().filter(|r| **r == Role::Farmer).count() as f32;
    let miners = roles.iter().filter(|r| **r == Role::Miner).count() as f32;

    stock.food = (stock.food + farmers * PROD_FARMER - pop * RATION).max(0.0);
    stock.wood = (stock.wood + miners * PROD_MINER * 0.6).max(0.0);
    stock.stone = (stock.stone + miners * PROD_MINER * 0.4).max(0.0);

    let food_reserve = stock.food / (pop * RATION).max(1e-4);
    let food_deficit = 1.0 - (food_reserve / HEALTH_RESERVE_TICKS).clamp(0.0, 1.0);
    let famine = food_deficit > 0.6; // comida crítica: todo el que esté libre, a granjear

    let mut builder_assigned = roles.iter().any(|r| *r == Role::Builder);
    let mut wanderer_idx = 0u32;
    for r in roles.iter_mut() {
        if *r != Role::Wanderer {
            continue; // un colono con oficio no rota — pueblo estable, no ruleta de trabajos
        }
        *r = if famine {
            // Emergencia real, pero NUNCA 100% granja: si el hambre manda todo a Farmer
            // de una, la colonia queda sin Miner para siempre (nadie rota de oficio) y
            // jamás junta madera para un Constructor. 80/20 alcanza para salir del
            // hambre sin condenar al pueblo a no minar nunca.
            if wanderer_idx % 5 < 4 {
                Role::Farmer
            } else {
                Role::Miner
            }
        } else if !builder_assigned && stock.wood >= BUILD_WOOD_COST {
            builder_assigned = true;
            Role::Builder
        } else if wanderer_idx % 5 < 3 {
            Role::Farmer // ~60%
        } else {
            Role::Miner // ~40%
        };
        wanderer_idx += 1;
    }
}

#[derive(Component)]
pub(crate) struct Npc {
    brain: i32,
    pub(crate) x: f32,
    pub(crate) z: f32,
    heading: f32,
    pub(crate) family: u32,
    pub(crate) uid: u64,
    energy: f32,
    age: f32, // F6.3: edad (s de sim) → muerte por vejez = recambio generacional
    // --- Religión + propósito (runtime; se reconstruye cerca del ídolo) ---
    pub(crate) faith: f32,      // 0..1 devoción hacia el fundador; sube cerca del ídolo
    pub(crate) converted: bool, // fiel: sigue/venera al ídolo (fundador)
    home_x: f32,      // ancla del "pueblo" (pos de spawn) — deambula cerca, no en círculos
    home_z: f32,
    say_cd: f32,      // cooldown de charla ambiental
    /// Últimas frases dichas (máx 5) — para el menú del NPC (tocar → ver historial).
    pub(crate) history: Vec<String>,
    /// C1: dos golpes para matar (`ATTACK_DAMAGE`), en vez de un tap instantáneo.
    pub(crate) health: f32,
    /// C2/C3: cooldowns de mundo para CALMAR/CONVERTIR (evitan el spam del menú).
    pub(crate) calm_cd: f32,
    pub(crate) convert_cd: f32,
    /// S6: oficio elegido por escasez — así usa el mundo como el jugador (granjea/mina).
    pub(crate) role: Role,
    /// S5: comida/descanso/pertenencia — la rutina diaria las sube o las deja caer.
    pub(crate) needs: Needs,
}

/// Registro persistente de un NPC (posición/familia/edad); vive como dato en todos los
/// cuerpos. Su GENOMA evolucionado se guarda aparte en `g_<uid>.adn` (F6.2).
#[derive(Clone, Serialize, Deserialize)]
struct NpcRecord {
    uid: u64,
    family: u32,
    body: u8,
    x: f32,
    z: f32,
    heading: f32,
    energy: f32,
    #[serde(default)]
    age: f32,
    #[serde(default)]
    faith: f32,
    #[serde(default)]
    converted: bool,
    /// Ancla del "pueblo" (identidad, NO la posición actual — B1: si se deriva de `x/z`
    /// en cada respawn, el pueblo hace un random walk hasta difundirse). `None` en saves
    /// viejos → se ancla a `x/z` UNA vez al cargar (migración) y nunca más se mueve.
    #[serde(default)]
    home_x: Option<f32>,
    #[serde(default)]
    home_z: Option<f32>,
    /// E4: últimas frases dichas — sobrevive a cambios de cuerpo/reinicio.
    #[serde(default)]
    history: Vec<String>,
    /// C1: vida (dos golpes para matar). Los saves viejos no la traen → sanos.
    #[serde(default = "full_health")]
    health: f32,
    /// S6: oficio — persiste para que el pueblo no "olvide" quién hace qué al recargar.
    #[serde(default)]
    role: Role,
    /// S5: necesidades — persisten para que la rutina no reinicie al recargar.
    #[serde(default)]
    needs: Needs,
}

fn full_health() -> f32 {
    1.0
}

/// A3: la religión persiste (antes se perdía al reabrir y los fieles decían
/// "Alabado sea !" con el nombre vacío).
#[derive(Clone, Serialize, Deserialize, Default)]
struct ReligionSave {
    founded: bool,
    name: String,
}

#[derive(Serialize, Deserialize, Default)]
struct SocietySave {
    npcs: Vec<NpcRecord>,
    /// Reputación del jugador por familia (persistente = la MEMORIA).
    family_rep: Vec<(u32, f32)>,
    next_uid: u64,
    next_family: u32,
    #[serde(default)]
    religion: ReligionSave,
    /// S4/B2: centro fijo de asentamiento por cuerpo (dónde aterrizaste la primera vez) —
    /// nunca se recalcula, así los nativos no "caminan" ni aparecen en el origen del mundo.
    #[serde(default)]
    centers: Vec<(u8, f32, f32)>,
    /// S4/B3: reputación acumulada por CUERPO (sobrevive a la extinción de una familia) —
    /// matar a todos los miembros de una familia no lava la memoria del planeta.
    #[serde(default)]
    body_rep: Vec<(u8, f32)>,
    /// S6: recursos de la colonia por cuerpo.
    #[serde(default)]
    stock: Vec<(u8, Stock)>,
}

#[derive(Resource)]
pub(crate) struct Society {
    /// La MEMORIA: reputación del jugador ante cada familia `[-1 hostil .. +1 amable]`.
    family_rep: HashMap<u32, f32>,
    /// NPCs de cuerpos NO instanciados (el cuerpo activo vive como entidades).
    records: Vec<NpcRecord>,
    free_ids: Vec<i32>,
    next_uid: u64,
    next_family: u32,
    loaded: bool,
    active_body: Option<u8>,
    alive: usize,
    /// S4/B2: centro fijo de asentamiento por cuerpo — se escribe UNA vez.
    centers: HashMap<u8, (f32, f32)>,
    /// S4/B3: reputación por cuerpo — sobrevive a familias extintas.
    body_rep: HashMap<u8, f32>,
    /// S6: recursos de la colonia por cuerpo — lo que producen/consumen los roles.
    stock: HashMap<u8, Stock>,
    /// A1: se pone en `true` cuando algo cambió el estado persistente fuera del ciclo
    /// normal (p. ej. una muerte) y todavía no se volcó a disco. `save_society_periodic`
    /// es el ÚNICO camino de escritura; esto le pide un flush inmediato con `live` incluido.
    dirty: bool,
}

impl Default for Society {
    fn default() -> Self {
        Self {
            family_rep: HashMap::new(),
            records: Vec::new(),
            free_ids: (NPC_ID_BASE..NPC_ID_BASE + MAX_NPCS as i32).rev().collect(),
            next_uid: 1,
            next_family: 1,
            loaded: false,
            active_body: None,
            alive: 0,
            centers: HashMap::new(),
            body_rep: HashMap::new(),
            stock: HashMap::new(),
            dirty: false,
        }
    }
}

impl Society {
    pub(crate) fn rep(&self, family: u32) -> f32 {
        *self.family_rep.get(&family).unwrap_or(&REP_DEFAULT)
    }
    pub(crate) fn adjust(&mut self, family: u32, delta: f32) {
        let e = self.family_rep.entry(family).or_insert(REP_DEFAULT);
        *e = (*e + delta).clamp(-1.0, 1.0);
    }
    /// C2: CALMAR sube la reputación pero nunca por encima de `ceiling` — para volver a
    /// estar realmente bien hay que HACER algo, no sólo hablar.
    pub(crate) fn adjust_capped(&mut self, family: u32, delta: f32, ceiling: f32) {
        let e = self.family_rep.entry(family).or_insert(REP_DEFAULT);
        *e = (*e + delta).min(ceiling).clamp(-1.0, ceiling);
    }
    /// S4/B3: reputación acumulada de todo el CUERPO — no se resetea aunque una familia
    /// entera se extinga, así "matar a todos y volver" no vuelve con la pizarra limpia.
    pub(crate) fn body_rep(&self, body: u8) -> f32 {
        *self.body_rep.get(&body).unwrap_or(&0.0)
    }
    pub(crate) fn nudge_body_rep(&mut self, body: u8, delta: f32) {
        let e = self.body_rep.entry(body).or_insert(0.0);
        *e = (*e + delta).clamp(-1.0, 1.0);
    }
    /// S6: recursos de la colonia de `body` (0 si todavía no produjo nada).
    pub(crate) fn stock(&self, body: u8) -> Stock {
        *self.stock.get(&body).unwrap_or(&Stock::default())
    }
    /// S6/B4: reemplaza el timer fijo de reproducción — la natalidad depende de si la
    /// colonia tiene con qué alimentar a alguien más.
    pub(crate) fn body_health(&self, body: u8, population: u32) -> f32 {
        colony_health(self.stock(body), population)
    }
    pub(crate) fn active_body(&self) -> Option<u8> {
        self.active_body
    }
    /// Conecta el envío de víveres/colonos de `colony.rs` (uplink Tierra→Luna) con la
    /// economía REAL de los NPCs — antes alimentaba un contador (`ColonyState.food_stock`)
    /// que ningún NPC visible llegaba a usar.
    pub(crate) fn add_stock(&mut self, body: u8, food: f32, wood: f32, stone: f32) {
        let s = self.stock.entry(body).or_default();
        s.food += food;
        s.wood += wood;
        s.stone += stone;
    }
    /// C4/S6: comercio REAL — la colonia ofrece lo que tiene de SOBRA (madera o piedra,
    /// nunca comida: esa es la que la mantiene viva) sin dejar el stock por debajo de
    /// `TRADE_MIN_RESERVE`. `None` = no hay excedente todavía, no un stub fijo.
    pub(crate) fn try_trade(&mut self, body: u8) -> Option<(&'static str, f32)> {
        let mut s = self.stock(body);
        let offer = if s.wood >= s.stone && s.wood - TRADE_AMOUNT >= TRADE_MIN_RESERVE {
            Some("wood")
        } else if s.stone - TRADE_AMOUNT >= TRADE_MIN_RESERVE {
            Some("stone")
        } else {
            None
        };
        let item = offer?;
        if item == "wood" {
            s.wood -= TRADE_AMOUNT;
        } else {
            s.stone -= TRADE_AMOUNT;
        }
        self.stock.insert(body, s);
        Some((item, TRADE_AMOUNT))
    }
}

/// F7 — Estado de la RELIGIÓN que funda el jugador (+ su compañero como primer
/// discípulo). Un solo culto por ahora; los NPCs cercanos ganan fe y se convierten
/// en fieles que siguen/veneran al fundador (ídolo).
#[derive(Resource, Default)]
pub struct Religion {
    pub founded: bool,
    pub name: String,
    pub followers: u32,
}

/// Frase por CONTEXTO (fe/reputación/religión) — ASCII-only. `variant` desempata
/// para que no todos digan lo mismo. Es la base del "chat" con respuestas
/// predefinidas según lo que venimos construyendo (culto, pueblo, rencor).
pub(crate) fn npc_line(rep: f32, converted: bool, faith: f32, founded: bool, name: &str, variant: u64) -> String {
    if converted {
        let v = [
            format!("Alabado sea {name}!"),
            "El Profeta nos guia.".to_string(),
            "Uno con el Ki.".to_string(),
            format!("Somos {name}."),
            "Te seguire a las estrellas.".to_string(),
        ];
        return v[(variant as usize) % v.len()].clone();
    }
    if rep <= AGGRO_THRESHOLD {
        let v = ["No te queremos aqui.", "Pagaras por lo que hiciste.", "Vete!"];
        return v[(variant as usize) % v.len()].to_string();
    }
    if rep < -0.15 {
        let v = ["No me fio de ti.", "Mantente lejos.", "Te estoy vigilando."];
        return v[(variant as usize) % v.len()].to_string();
    }
    if founded && faith > 0.2 {
        let v = [
            format!("Que es {name}?"),
            "Ese forastero tiene algo...".to_string(),
            "Siento el Ki en el.".to_string(),
        ];
        return v[(variant as usize) % v.len()].clone();
    }
    let v = ["Buen dia, viajero.", "El pueblo crece.", "Hay trabajo que hacer.", "Lindo dia."];
    v[(variant as usize) % v.len()].to_string()
}

pub struct SocietyPlugin;

impl Plugin for SocietyPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Society>()
            .init_resource::<Religion>()
            .add_systems(
                Update,
                (manage_society, tick_society, harm_events, tick_colony_economy, save_society_periodic)
                    .chain(),
            )
            .add_systems(Startup, (spawn_society_overlay, force_creative_debug))
            .add_systems(Update, (update_society_overlay, update_npc_sightings));
    }
}

// --- Persistencia ------------------------------------------------------------

fn society_dir() -> PathBuf {
    crate::inventory_persist::persist_dir().join("society")
}

fn society_json() -> PathBuf {
    society_dir().join("society.json")
}

/// Genoma evolucionado de un NPC (F6.2), por identidad estable.
fn npc_genome_path(uid: u64) -> PathBuf {
    society_dir().join(format!("g_{uid}.adn"))
}

fn load_society() -> SocietySave {
    match std::fs::read_to_string(society_json()) {
        Ok(raw) => serde_json::from_str(&raw).unwrap_or_default(),
        Err(_) => SocietySave::default(),
    }
}

/// Guarda TODO: registros de otros cuerpos + snapshot de los NPCs vivos del cuerpo actual
/// + la memoria (reputación) + la religión (A3). Barato (≤ `MAX_NPCS`). Único camino de
/// escritura (A1) — quien quiera persistir marca `soc.dirty` y espera este flush.
fn save_society(soc: &Society, live: &[NpcRecord], religion: &Religion) {
    let _ = std::fs::create_dir_all(society_dir());
    let mut npcs = soc.records.clone();
    npcs.extend_from_slice(live);
    // E6: podar del archivo la reputación de familias que ya no tienen ni un registro
    // (ni vivo ni en otro cuerpo) — si no, `family_rep` crece para siempre.
    let living: std::collections::HashSet<u32> = npcs.iter().map(|r| r.family).collect();
    let save = SocietySave {
        npcs,
        family_rep: soc
            .family_rep
            .iter()
            .filter(|(f, _)| living.contains(f))
            .map(|(k, v)| (*k, *v))
            .collect(),
        next_uid: soc.next_uid,
        next_family: soc.next_family,
        religion: ReligionSave { founded: religion.founded, name: religion.name.clone() },
        centers: soc.centers.iter().map(|(&b, &(x, z))| (b, x, z)).collect(),
        body_rep: soc.body_rep.iter().map(|(&b, &r)| (b, r)).collect(),
        stock: soc.stock.iter().map(|(&b, &s)| (b, s)).collect(),
    };
    if let Ok(json) = serde_json::to_string(&save) {
        let _ = std::fs::write(society_json(), json);
    }
}

fn spawn_npc(
    commands: &mut Commands,
    art: &crate::art2d::ArtLibrary,
    rec: &NpcRecord,
    brain: i32,
) -> Entity {
    // Skin del Character Generator por uid (variedad estable — ver
    // `ha-asset-tools::chargen`, hoja 16×32 igual a la del jugador). Mismo tamaño
    // humano que el jugador (0.95×1.9): el frame es 1:2, forzarlo a cuadrado
    // achataba al aldeano.
    let skin = format!("npc/char_{}", (rec.uid % NUM_SKINS) + 1);
    let sprite = art.sprite_or_color(&skin, Color::srgb(0.85, 0.82, 0.75), NPC_SIZE);
    let r = render_xy(rec.x, rec.z);
    let mut ent = commands
        .spawn((
            Npc {
                brain,
                x: rec.x,
                z: rec.z,
                heading: rec.heading,
                family: rec.family,
                uid: rec.uid,
                energy: rec.energy,
                age: rec.age,
                faith: rec.faith,
                converted: rec.converted,
                // B1: el hogar es la identidad del NPC, no su posición actual — sólo cae a
                // `rec.x/z` la primera vez que un registro viejo no lo trae (migración).
                home_x: rec.home_x.unwrap_or(rec.x),
                home_z: rec.home_z.unwrap_or(rec.z),
                say_cd: (rec.uid % 7) as f32, // desfasa el chatter
                history: rec.history.clone(),
                health: rec.health,
                calm_cd: 0.0,
                convert_cd: 0.0,
                role: rec.role,
                needs: rec.needs,
            },
            sprite,
            Transform::from_xyz(r.x, r.y, sort_z(rec.z)),
        ));
    // Un aldeano con antorcha: si la skin declara luz, la lleva encima.
    crate::light2d::attach(&mut ent, art, &[&skin, "npc/aldeano"], NPC_SIZE);
    ent.id()
}

// --- Activación por cuerpo (como el vivario) ---------------------------------

#[allow(clippy::too_many_arguments)]
fn manage_society(
    mut commands: Commands,
    art: Res<crate::art2d::ArtLibrary>,
    region: Res<RegionRuntime>,
    ctx: Res<WorldContextRuntime>,
    planet: Res<ActivePlanet>,
    colony: Res<crate::colony::ColonyRuntime>,
    machines: Res<crate::machines::MachineWorld>,
    mut soc: ResMut<Society>,
    mut religion: ResMut<Religion>,
    live: Query<&Npc>,
    live_ents: Query<Entity, With<Npc>>,
    player: Query<&Transform, With<Player>>,
) {
    if !soc.loaded {
        let s = load_society();
        soc.records = s.npcs;
        soc.family_rep = s.family_rep.into_iter().collect();
        soc.next_uid = s.next_uid.max(soc.next_uid);
        soc.next_family = s.next_family.max(soc.next_family);
        religion.founded = s.religion.founded; // A3
        religion.name = s.religion.name;
        soc.centers = s.centers.into_iter().map(|(b, x, z)| (b, (x, z))).collect();
        soc.body_rep = s.body_rep.into_iter().collect();
        soc.stock = s.stock.into_iter().collect();
        soc.loaded = true;
    }
    let current = if surface_region_active(&region).is_some() {
        Some(active_surface_body(&ctx, &planet) as u8)
    } else {
        None
    };
    if current == soc.active_body {
        return;
    }
    // Snapshot + despawn del cuerpo viejo (guarda también el GENOMA evolucionado, F6.2).
    if let Some(old) = soc.active_body {
        soc.records.retain(|r| r.body != old);
        let mut snap: Vec<NpcRecord> = Vec::new();
        for n in &live {
            iafz_sys::save(n.brain, npc_genome_path(n.uid).to_str().unwrap_or(""));
            snap.push(NpcRecord {
                uid: n.uid,
                family: n.family,
                body: old,
                x: n.x,
                z: n.z,
                heading: n.heading,
                energy: n.energy,
                age: n.age,
                faith: n.faith,
                converted: n.converted,
                home_x: Some(n.home_x),
                home_z: Some(n.home_z),
                history: n.history.clone(),
                health: n.health,
                role: n.role,
                needs: n.needs,
            });
        }
        soc.records.extend(snap);
        for e in &live_ents {
            commands.entity(e).despawn_recursive();
        }
        soc.free_ids = (NPC_ID_BASE..NPC_ID_BASE + MAX_NPCS as i32).rev().collect();
        soc.alive = 0;
        save_society(&soc, &[], &religion);
    }
    // Instancia el cuerpo nuevo.
    if let Some(nb) = current {
        let mut recs: Vec<NpcRecord> =
            soc.records.iter().filter(|r| r.body == nb).cloned().collect();
        soc.records.retain(|r| r.body != nb);
        // Semilla si el cuerpo aún no tiene sociedad:
        //  - Luna: TUS colonos (`colony.state.colonists`), amigables (cerca de hábitats).
        //  - Otros planetas (F6.4): NATIVOS deterministas con disposición base según el
        //    TEMPERAMENTO del planeta → culturas distintas (un mundo hostil = nativos que
        //    ya te reciben con recelo).
        if recs.is_empty() {
            if nb == CelestialBody::Earth as u8 {
                // Pueblo inicial en la Tierra (role-play): 9 aldeanos cordiales
                // alrededor del JUGADOR → coexisten, forman familias, y son el
                // rebaño que puede convertir a su religión.
                let origin = fixed_center(&mut soc, nb, &player);
                recs = seed_society(nb, 9, None, 0.4, origin, &mut soc);
            } else if nb == CelestialBody::Moon as u8 && colony.state.colonists > 0 {
                recs = seed_society(nb, colony.state.colonists, Some(&machines), 0.0, (0.0, 0.0), &mut soc);
            } else if nb != CelestialBody::Earth as u8 && nb != CelestialBody::Moon as u8 {
                if let Some(body) = body_from_u8(nb) {
                    // B2/S4: se siembra en tu punto de aterrizaje REAL, fijado para
                    // siempre — antes era `(0.0, 0.0)`, típicamente a kilómetros de
                    // donde estás, así que la feature multi-planeta era invisible.
                    let origin = fixed_center(&mut soc, nb, &player);
                    // B3/S4: genocidiar una familia no lava la memoria del planeta — la
                    // disposición cultural se combina con `body_rep`, que sobrevive.
                    let disp = (planet_temperament_disposition(body) + soc.body_rep(nb))
                        .clamp(-1.0, 1.0);
                    let seed = (nb as u32).wrapping_mul(2_654_435_761);
                    let count = 3 + (seed >> 5) % 7; // 3..9 nativos
                    recs = seed_society(nb, count, None, disp, origin, &mut soc);
                }
            }
        }
        for rec in recs {
            let Some(brain) = soc.free_ids.pop() else { break };
            // F6.2: cargá el GENOMA evolucionado si existe; si no, el seed del gym.
            let loaded = npc_genome_path(rec.uid)
                .to_str()
                .map(|p| iafz_sys::load(brain, p))
                .unwrap_or(false);
            if !loaded {
                iafz_sys::load_bytes(brain, GENOME);
            }
            iafz_sys::reset_echo(brain);
            soc.alive += 1;
            spawn_npc(&mut commands, &art, &rec, brain);
        }
    }
    soc.active_body = current;
}

/// B2/S4: el centro de asentamiento de un cuerpo se fija la PRIMERA vez que se siembra
/// (tu posición al aterrizar) y nunca se recalcula — se conserva en `Society.centers` y
/// en `society.json`, así el pueblo no "camina" entre visitas ni reaparece en `(0,0)`.
fn fixed_center(soc: &mut Society, body: u8, player: &Query<&Transform, With<Player>>) -> (f32, f32) {
    if let Some(&c) = soc.centers.get(&body) {
        return c;
    }
    let c = player
        .get_single()
        .map(|t| (t.translation.x, -t.translation.y))
        .unwrap_or((0.0, 0.0));
    soc.centers.insert(body, c);
    c
}

/// Disposición base de los nativos de un planeta según su carácter (espejo del
/// `Temperament` de la fauna F5 → mismas culturas): amigable ↑, hostil ↓.
fn planet_temperament_disposition(body: CelestialBody) -> f32 {
    match body {
        CelestialBody::Earth => 0.4,
        CelestialBody::Moon => 0.2,
        _ => match (body as u32).wrapping_mul(2_654_435_761) % 4 {
            0 => 0.35,  // amigable
            1 => 0.05,  // neutral
            2 => -0.30, // tenebroso
            _ => -0.60, // hostil
        },
    }
}

/// Funda una sociedad: `count` NPCs en familias de `FAMILY_SIZE`. Con `machines` = cerca de
/// los hábitats (colonos lunares); sin `machines` = nativos dispersos alrededor del origen.
/// `base_rep != 0` fija la disposición inicial de cada familia (culturas de F6.4).
fn seed_society(
    body: u8,
    count: u32,
    machines: Option<&crate::machines::MachineWorld>,
    base_rep: f32,
    origin: (f32, f32),
    soc: &mut Society,
) -> Vec<NpcRecord> {
    let anchors: Vec<(f32, f32)> = machines
        .map(|m| {
            m.on_body(CelestialBody::Moon)
                .filter(|mm| mm.machine_id == ha_shared::HABITAT.id)
                .map(|mm| (mm.x, mm.z))
                .collect()
        })
        .unwrap_or_default();
    let n_anchor = anchors.len().max(1) as u32;
    let mut out = Vec::new();
    for i in 0..count {
        let (bx, bz) = anchors.get((i % n_anchor) as usize).copied().unwrap_or(origin);
        let ring = (i / n_anchor) as f32;
        let ang = i as f32 * 2.399_963_2;
        let rad = if machines.is_some() { 1.5 + ring * 1.2 } else { 6.0 + ring * 2.5 };
        let wx = bx + ang.cos() * rad;
        let wz = bz + 1.0 + ang.sin() * rad;
        // Familias: grupos contiguos de `FAMILY_SIZE`; fijá su disposición cultural.
        if i % FAMILY_SIZE == 0 {
            soc.next_family = soc.next_family.wrapping_add(1);
            if base_rep != 0.0 {
                let f = soc.next_family;
                soc.family_rep.insert(f, base_rep.clamp(-1.0, 1.0));
            }
        }
        let family = soc.next_family;
        let uid = soc.next_uid;
        soc.next_uid = soc.next_uid.wrapping_add(1);
        out.push(NpcRecord {
            uid,
            family,
            body,
            x: wx,
            z: wz,
            heading: ang,
            energy: 1.0,
            age: 0.0,
            faith: 0.0,
            converted: false,
            home_x: Some(wx),
            home_z: Some(wz),
            history: Vec::new(),
            health: 1.0,
            role: Role::Wanderer,
            needs: Needs::default(),
        });
    }
    out
}

// --- Conducta (sesgada por la reputación) ------------------------------------

#[allow(clippy::type_complexity, clippy::too_many_arguments)]
fn tick_society(
    time: Res<Time>,
    ctx: Res<WorldContextRuntime>,
    planet: Res<ActivePlanet>,
    tiles: Res<TileWorld>,
    mut commands: Commands,
    art: Res<crate::art2d::ArtLibrary>,
    mut soc: ResMut<Society>,
    mut religion: ResMut<Religion>,
    day: Res<crate::sky2d::DayClock>,
    mut speech: ResMut<crate::dialog::SpeechQueue>,
    player: Query<&Transform, (With<Player>, Without<Npc>)>,
    mut npcs: Query<(Entity, &mut Npc, &mut Transform, &mut Sprite, &mut Visibility)>,
    mut acc: Local<f32>,
) {
    const AI_HZ: f32 = 15.0;
    *acc += time.delta_secs();
    if *acc < 1.0 / AI_HZ {
        return;
    }
    let dt = (*acc).min(0.2);
    *acc = 0.0;
    let body = active_surface_body(&ctx, &planet);
    let has_player = player.get_single().is_ok();
    let (px, pz) = player
        .get_single()
        .map(|t| (t.translation.x, -t.translation.y))
        .unwrap_or((0.0, 0.0));
    let neigh: Vec<(f32, f32)> = npcs.iter().map(|(_, n, ..)| (n.x, n.z)).collect();

    // FUNDAR la religión: si el fundador (jugador) reúne un rebaño de NPCs cordiales
    // a su alrededor, nace el culto (su compañero es el primer discípulo). Una vez.
    if has_player && !religion.founded {
        let flock = npcs
            .iter()
            .filter(|(_, n, ..)| {
                let d2 = (n.x - px) * (n.x - px) + (n.z - pz) * (n.z - pz);
                d2 < FOUND_RADIUS * FOUND_RADIUS && soc.rep(n.family) > AGGRO_THRESHOLD
            })
            .count();
        if flock >= FOUND_MIN_FLOCK {
            religion.founded = true;
            religion.name = "Los Hijos del Ki".to_string();
            eprintln!("ha-religion: FUNDADA '{}' (rebaño {flock})", religion.name);
            crate::dialog::say(
                &mut speech,
                px,
                pz,
                format!("Fundo {}!", religion.name),
                Color::srgb(1.0, 0.9, 0.3),
            );
        }
    }
    let mut followers = 0u32;

    // B6: la muerte por causa VISIBLE vive entera en `harm_events` (el único lugar que
    // puede matar hoy); acá no hay canal muerto que iterar.
    let mut births: Vec<(f32, f32, i32, u32)> = Vec::new(); // x, z, brain padre, familia

    for (e, mut n, mut tf, mut sprite, mut vis) in &mut npcs {
        let dr2 = (n.x - px) * (n.x - px) + (n.z - pz) * (n.z - pz);
        // Cull + freeze lejos del jugador (barato; sigue como dato).
        if dr2 > RENDER_R * RENDER_R {
            if *vis != Visibility::Hidden {
                *vis = Visibility::Hidden;
            }
            continue;
        }
        if *vis != Visibility::Visible {
            *vis = Visibility::Visible;
        }
        // Los NPCs NO se esfuman de viejos (mundo estable tipo GTA/Minecraft): sólo mueren
        // por causa VISIBLE (el jugador los mata → rencor de la familia). Se sigue contando
        // la edad por si luego querés "ancianos"/roles, pero no gatilla muerte.
        n.age += dt;
        n.calm_cd = (n.calm_cd - dt).max(0.0);
        n.convert_cd = (n.convert_cd - dt).max(0.0);

        use std::f32::consts::{FRAC_PI_2, PI, TAU};
        let rep = soc.rep(n.family);

        // --- FE + conversión (religión) ---
        let near_idol =
            religion.founded && has_player && dr2 < IDOL_RADIUS * IDOL_RADIUS && rep > AGGRO_THRESHOLD;
        if near_idol {
            n.faith = (n.faith + FAITH_GAIN * dt).min(1.0);
            if !n.converted && n.faith >= CONVERT_AT {
                n.converted = true;
                crate::dialog::say_from(
                    &mut speech,
                    e,
                    n.x,
                    n.z,
                    format!("Creo en {}!", religion.name),
                    Color::srgb(1.0, 0.85, 0.3),
                    false,
                );
            }
        } else if !n.converted {
            n.faith = (n.faith - 0.01 * dt).max(0.0); // se enfría lejos del ídolo
        }
        if n.converted {
            followers += 1;
        }

        // Cerebro base (IA frugal): obstáculos/textura, con AUTORIDAD REDUCIDA para
        // que NO gire en círculos (era el bug: sin comida el steer lo hacía girar).
        let s = sense(body, n.x, n.z, n.heading, n.energy, &[], &neigh);
        let steer = iafz_sys::think(n.brain, s).tanh();
        n.heading += steer * NPC_TURN * dt * 0.35;

        // --- S5: necesidades — decaen siempre; la rutina las repone si cumple el horario ---
        n.needs.food = (n.needs.food - NEEDS_FOOD_DECAY * dt).max(0.0);
        n.needs.rest = (n.needs.rest - NEEDS_REST_DECAY * dt).max(0.0);
        n.needs.belonging = (n.needs.belonging - NEEDS_BELONGING_DECAY * dt).max(0.0);
        let phase = routine_phase(day.phase());

        // --- PROPÓSITO: SIEMPRE hay un rumbo → nada de círculos sin sentido ---
        let toward_p = (pz - n.z).atan2(px - n.x);
        let mut resting = false;
        let (goal, gain): (Option<f32>, f32) = if n.converted && has_player {
            // Fiel: venera al ídolo — acércate si lejos, rodéalo si cerca.
            let d = dr2.sqrt();
            if d > IDOL_ORBIT + 0.6 {
                (Some(toward_p), 1.1)
            } else {
                (Some(toward_p + FRAC_PI_2), 0.7) // orbita al ídolo
            }
        } else if rep <= AGGRO_THRESHOLD && dr2 < 24.0 * 24.0 {
            (Some(toward_p), 1.0) // te encara
        } else if rep < -0.15 && dr2 < 20.0 * 20.0 {
            (Some(toward_p + PI), 0.9) // huye
        } else {
            // S5: rutina diaria — reemplaza el "deambula cerca de casa" fijo por un
            // horario real (trabajo/plaza/casa) atado a la MISMA hora que ves en el
            // cielo/HUD (`sky2d::DayClock`). Arregla B8: de noche ya no siguen paseando.
            match phase {
                RoutinePhase::Sleep => {
                    let hd2 = (n.x - n.home_x) * (n.x - n.home_x) + (n.z - n.home_z) * (n.z - n.home_z);
                    if hd2 > HOME_WANDER_R * HOME_WANDER_R {
                        (Some((n.home_z - n.z).atan2(n.home_x - n.x)), 0.9) // vuelve a dormir
                    } else {
                        n.needs.rest = (n.needs.rest + NEEDS_HOME_REST_GAIN * dt).min(1.0);
                        resting = true;
                        (None, 0.0) // quieto en casa de noche
                    }
                }
                RoutinePhase::GoToWork | RoutinePhase::Work => {
                    let (wx, wz) = workplace_of(n.home_x, n.home_z, n.uid, n.family);
                    let wd2 = (n.x - wx) * (n.x - wx) + (n.z - wz) * (n.z - wz);
                    if wd2 > 2.0 * 2.0 {
                        (Some((wz - n.z).atan2(wx - n.x)), 0.9) // va/está en el puesto
                    } else {
                        n.needs.food = (n.needs.food + NEEDS_WORK_FOOD_GAIN * dt).min(1.0);
                        let drift = (n.uid as f32 * 1.7 + n.age * 0.25).sin() * 0.3;
                        (Some(n.heading + drift), 0.15) // trabaja en el lugar
                    }
                }
                RoutinePhase::Plaza => {
                    if let Some(&(cx, cz)) = soc.centers.get(&(body as u8)) {
                        let cd2 = (n.x - cx) * (n.x - cx) + (n.z - cz) * (n.z - cz);
                        if cd2 > PLAZA_ARRIVE_R * PLAZA_ARRIVE_R {
                            (Some((cz - n.z).atan2(cx - n.x)), 0.85) // va a la plaza
                        } else {
                            n.needs.belonging = (n.needs.belonging + NEEDS_PLAZA_GAIN * dt).min(1.0);
                            let drift = (n.uid as f32 * 1.7 + n.age * 0.25).sin() * 0.3;
                            (Some(n.heading + drift), 0.15) // se junta con el pueblo
                        }
                    } else {
                        (None, 0.0)
                    }
                }
                RoutinePhase::GoHome => {
                    let hd2 = (n.x - n.home_x) * (n.x - n.home_x) + (n.z - n.home_z) * (n.z - n.home_z);
                    if hd2 > HOME_WANDER_R * HOME_WANDER_R {
                        (Some((n.home_z - n.z).atan2(n.home_x - n.x)), 0.85) // vuelve a casa
                    } else {
                        let drift = (n.uid as f32 * 1.7 + n.age * 0.25).sin() * 0.35;
                        (Some(n.heading + drift), 0.12)
                    }
                }
            }
        };
        if let Some(g) = goal {
            let diff = (g - n.heading + PI).rem_euclid(TAU) - PI;
            n.heading += diff.clamp(-1.0, 1.0) * NPC_TURN * dt * gain;
        }

        let aggro = rep <= AGGRO_THRESHOLD && dr2 < 24.0 * 24.0;
        let flee = rep < -0.2 && rep > AGGRO_THRESHOLD && dr2 < 20.0 * 20.0;
        let speed = if resting {
            0.0 // S5: duerme adentro, quieto — no camina en el sitio
        } else {
            NPC_VEL
                * if aggro {
                    1.9
                } else if flee {
                    1.6
                } else if n.converted {
                    1.3
                } else {
                    0.9
                }
        };
        let (nx, nz) = (n.x + n.heading.cos() * speed * dt, n.z + n.heading.sin() * speed * dt);
        if tiles.solid_at(body, nx.floor() as i64, nz.floor() as i64) {
            n.heading += 1.3;
        } else {
            n.x = nx;
            n.z = nz;
        }
        n.energy = (n.energy + dt * 0.006).clamp(0.0, 1.0);

        // F6.2/S6(B4): reproducción → el hijo hereda la FAMILIA (memoria) + el GENOMA
        // mutado. La energía individual sigue siendo la señal de "está bien" del NPC, pero
        // ya NO alcanza sola — la colonia tiene que tener con qué alimentar a alguien más
        // (antes era un timer que disparaba nacimientos para siempre sin importar la
        // comida real, así que "evolución" no tenía con qué competir).
        if n.energy >= NPC_REPRO_ENERGY
            && soc.alive + births.len() < MAX_NPCS
            && soc.free_ids.len() > births.len()
            && soc.body_health(body as u8, soc.alive as u32) >= BIRTH_MIN_HEALTH
        {
            n.energy -= NPC_REPRO_COST;
            births.push((n.x, n.z, n.brain, n.family));
        }

        // --- CHAT ambiental por CONTEXTO (el mundo se siente vivo) ---
        n.say_cd -= dt;
        // S5/B8: de noche el pueblo duerme — ya no charla a las 3 AM.
        if n.say_cd <= 0.0 && dr2 < 30.0 * 30.0 && phase != RoutinePhase::Sleep {
            n.say_cd = SAY_COOLDOWN + (n.uid % 5) as f32;
            let line = npc_line(
                rep,
                n.converted,
                n.faith,
                religion.founded,
                &religion.name,
                n.uid.wrapping_add(n.age as u64),
            );
            let col = if n.converted {
                Color::srgb(1.0, 0.9, 0.4)
            } else if rep < 0.0 {
                Color::srgb(1.0, 0.55, 0.45)
            } else {
                Color::srgb(0.92, 0.96, 1.0)
            };
            push_history(&mut n.history, &line);
            // E2: la burbuja sigue al NPC — antes quedaba clavada y el chatter lo dejaba
            // atrás al caminar.
            crate::dialog::say_from(&mut speech, e, n.x, n.z, line, col, false);
        }

        // Render: la SKIN se ve natural; sólo un tinte SUAVE marca disposición
        // (fiel → dorado, rencor → rojizo, cordial → blanco).
        let r = render_xy(n.x, n.z);
        tf.translation = Vec3::new(r.x, r.y, sort_z(n.z));
        let tint = if n.converted {
            Color::srgb(1.0, 0.94, 0.7)
        } else if rep < 0.0 {
            Color::srgb(1.0, 0.7 + 0.3 * (1.0 + rep), 0.65 + 0.3 * (1.0 + rep))
        } else {
            Color::WHITE
        };
        sprite.color = tint;
    }
    religion.followers = followers;
    if religion.founded && followers > 0 {
        // (log barato para el tester/capturas)
    }

    let body_u8 = soc.active_body.unwrap_or(0);
    for (x, z, parent, family) in births {
        let Some(child) = soc.free_ids.pop() else { break };
        iafz_sys::mutate(parent, child, NPC_MUTATE);
        iafz_sys::reset_echo(child);
        let uid = soc.next_uid;
        soc.next_uid = soc.next_uid.wrapping_add(1);
        soc.alive += 1;
        let (ox, oz) = ((child as f32).sin() * 0.8, (child as f32).cos() * 0.8);
        let rec = NpcRecord {
            uid,
            family,
            body: body_u8,
            x: x + ox,
            z: z + oz,
            heading: 0.0,
            energy: 0.5,
            age: 0.0,
            faith: 0.0,
            converted: false,
            home_x: Some(x + ox),
            home_z: Some(z + oz),
            history: Vec::new(),
            health: 1.0,
            role: Role::Wanderer, // S6: el recién nacido elige oficio en el próximo tick
            needs: Needs::default(),
        };
        spawn_npc(&mut commands, &art, &rec, child);
    }
}

// --- S5/S6: economía de colonia (LOD 2 — avanza aunque no estés parado ahí) --------

/// Corre cada `COLONY_TICK` de mundo para TODOS los cuerpos con sociedad (no sólo el
/// activo): produce/consume por rol y reasigna oficios por escasez. El cuerpo activo usa
/// las entidades vivas; los demás mutan directamente sus `NpcRecord` guardados — así el
/// pueblo de otro planeta sigue produciendo mientras estás en otro lado (arregla B7 para
/// la parte económica; el steering/cerebro por-entidad sigue congelado lejos, que es lo
/// caro, no la aritmética).
fn tick_colony_economy(time: Res<Time>, mut soc: ResMut<Society>, mut live: Query<&mut Npc>, mut acc: Local<f32>) {
    *acc += time.delta_secs();
    if *acc < COLONY_TICK {
        return;
    }
    *acc = 0.0;
    let mut bodies: std::collections::HashSet<u8> = soc.records.iter().map(|r| r.body).collect();
    if let Some(b) = soc.active_body {
        bodies.insert(b);
    }
    for body in bodies {
        if Some(body) == soc.active_body {
            let mut roles: Vec<Role> = live.iter().map(|n| n.role).collect();
            if roles.is_empty() {
                continue;
            }
            let mut stock = soc.stock(body);
            colony_step(&mut stock, &mut roles);
            soc.stock.insert(body, stock);
            for (mut n, r) in live.iter_mut().zip(roles) {
                n.role = r;
            }
        } else {
            let mut roles: Vec<Role> =
                soc.records.iter().filter(|r| r.body == body).map(|r| r.role).collect();
            if roles.is_empty() {
                continue;
            }
            let mut stock = soc.stock(body);
            colony_step(&mut stock, &mut roles);
            soc.stock.insert(body, stock);
            let mut it = roles.into_iter();
            for r in soc.records.iter_mut().filter(|r| r.body == body) {
                if let Some(nr) = it.next() {
                    r.role = nr;
                }
            }
        }
    }
}

// --- Evento de DAÑO (crea el rencor) -----------------------------------------

/// Un golpe de ATAQUE cerca del cursor daña al NPC más próximo (`ATTACK_DAMAGE`, dos
/// golpes para matar) — C1: sólo cuenta si el jugador está a distancia de combate real
/// (`PLAYER_REACH`), así usar cualquier habilidad lejos del pueblo ya no es un homicidio
/// accidental. Muerte → su familia lo recuerda (rencor fuerte) y los testigos (otras
/// familias) también. Es la semilla del "le maté al papá y esa familia me odia".
#[allow(clippy::too_many_arguments)]
fn harm_events(
    mut commands: Commands,
    tap: Res<AbilityTapPulse>,
    cursor: Res<AimCursor>,
    mut soc: ResMut<Society>,
    mut npcs: Query<(Entity, &mut Npc)>,
    player: Query<&Transform, With<Player>>,
    mut speech: ResMut<crate::dialog::SpeechQueue>,
    mut cooldown: Local<f32>,
    time: Res<Time>,
    mut sfx: EventWriter<crate::audio::PlaySfx>,
) {
    *cooldown = (*cooldown - time.delta_secs()).max(0.0);
    if tap.slot.is_none() || *cooldown > 0.0 || !cursor.initialized {
        return;
    }
    let Ok(pt) = player.get_single() else {
        return;
    };
    let (px, pz) = (pt.translation.x, -pt.translation.y);
    // NPC más cercano al cursor dentro del radio de golpe.
    let c = cursor.world;
    let mut victim: Option<(Entity, u32, u64, f32, f32)> = None; // entity, family, uid, x, z
    for (e, n) in npcs.iter() {
        let d2 = (n.x - c.x) * (n.x - c.x) + (n.z - c.y) * (n.z - c.y);
        if d2 <= HARM_RADIUS * HARM_RADIUS
            && victim.map(|(_, _, _, vx, vz)| {
                (n.x - c.x).powi(2) + (n.z - c.y).powi(2)
                    < (vx - c.x).powi(2) + (vz - c.y).powi(2)
            }).unwrap_or(true)
        {
            victim = Some((e, n.family, n.uid, n.x, n.z));
        }
    }
    let Some((e, family, uid, vx, vz)) = victim else {
        return;
    };
    // C1: fuera de alcance real del jugador → no cuenta como golpe (no consume cooldown,
    // podés acercarte e intentarlo de nuevo).
    if (vx - px) * (vx - px) + (vz - pz) * (vz - pz) > PLAYER_REACH * PLAYER_REACH {
        return;
    }
    *cooldown = 0.5; // un golpe por medio segundo
    let Ok((_, mut npc)) = npcs.get_mut(e) else {
        return;
    };
    npc.health = (npc.health - ATTACK_DAMAGE).max(0.0);
    let dead = npc.health <= 0.0;
    drop(npc);
    sfx.send(crate::audio::PlaySfx(crate::audio::Sfx::Dig));
    let body = soc.active_body.unwrap_or(0);
    if !dead {
        // Herido y no muerto: agresión atenuada (no el rencor completo de una muerte) y
        // huye — la reconciliación (curar) queda para cuando exista sanación real.
        soc.adjust(family, REP_KILL_FAMILY * 0.35);
        soc.nudge_body_rep(body, REP_KILL_FAMILY * 0.1); // S4/B3: mancha leve el planeta
        crate::dialog::say_from(&mut speech, e, vx, vz, "Ay!", Color::srgb(1.0, 0.7, 0.6), true);
        return;
    }
    // Rencor de la familia de la víctima + testigos cercanos (OTRAS familias lo recuerdan;
    // B5: la propia familia de la víctima no se cuenta dos veces como "testigo").
    soc.adjust(family, REP_KILL_FAMILY);
    // B3/S4: la mancha del CUERPO entero persiste aunque la familia se extinga — es lo
    // que evita que "matar a todos y volver" resetee la reputación del planeta.
    soc.nudge_body_rep(body, REP_KILL_FAMILY * 0.2);
    let witnesses: Vec<u32> = npcs
        .iter()
        .filter(|(we, n)| {
            *we != e
                && n.family != family
                && (n.x - vx) * (n.x - vx) + (n.z - vz) * (n.z - vz) < WITNESS_RADIUS * WITNESS_RADIUS
        })
        .map(|(_, n)| n.family)
        .collect();
    for f in witnesses {
        soc.adjust(f, REP_WITNESS);
    }
    commands.entity(e).despawn_recursive();
    soc.alive = soc.alive.saturating_sub(1);
    // A2: el genoma evolucionado del muerto no sirve de nada archivado — borrarlo evita
    // acumular cientos de `.adn` huérfanos en el guardado del teléfono.
    let _ = std::fs::remove_file(npc_genome_path(uid));
    // A1: NO guardar acá (no tenemos `live` a mano → escribiría el pueblo incompleto).
    // Un solo camino de escritura: pedir flush inmediato al próximo tick del guardado
    // periódico, que sí junta `live`.
    soc.dirty = true;
}

fn save_society_periodic(
    time: Res<Time>,
    mut soc: ResMut<Society>,
    religion: Res<Religion>,
    npcs: Query<&Npc>,
    mut acc: Local<f32>,
    mut was_paused: Local<bool>,
) {
    if !soc.loaded {
        return;
    }
    let paused = is_app_paused();
    let flush_now = paused && !*was_paused;
    *was_paused = paused;
    *acc += time.delta_secs();
    // A1: además del timer y la pausa, un `dirty` pedido por `harm_events` (o cualquier
    // otro evento futuro que toque la memoria) fuerza un flush YA — es el único camino
    // de escritura, así que siempre incluye `live`.
    if !flush_now && !soc.dirty && *acc < VIV_SAVE_EVERY {
        return;
    }
    *acc = 0.0;
    soc.dirty = false;
    let body = soc.active_body.unwrap_or(0);
    let live: Vec<NpcRecord> = npcs
        .iter()
        .map(|n| {
            // F6.2: persistí también el genoma evolucionado de cada NPC vivo.
            iafz_sys::save(n.brain, npc_genome_path(n.uid).to_str().unwrap_or(""));
            NpcRecord {
                uid: n.uid,
                family: n.family,
                body,
                x: n.x,
                z: n.z,
                heading: n.heading,
                energy: n.energy,
                age: n.age,
                faith: n.faith,
                converted: n.converted,
                home_x: Some(n.home_x),
                home_z: Some(n.home_z),
                history: n.history.clone(),
                health: n.health,
                role: n.role,
                needs: n.needs,
            }
        })
        .collect();
    // Si estamos en un cuerpo activo, esos NPCs viven; si no, `live` va vacío.
    if soc.active_body.is_some() {
        save_society(&soc, &live, &religion);
    } else {
        save_society(&soc, &[], &religion);
    }
}

// --- Overlay debug (creativo) ------------------------------------------------

#[derive(Component)]
struct SocietyOverlay;

/// QA de dev: `HA_FORCE_CREATIVE=1` fuerza el overlay de sociedad sin tener que
/// tocar el botón de creativo — mismo patrón que `HA_DAYPHASE`/`HA_NO_LIGHTS`.
fn force_creative_debug(mut editor: ResMut<crate::ui_editor::UiEditorState>) {
    if std::env::var("HA_FORCE_CREATIVE").is_ok() {
        editor.creative = true;
    }
}

fn spawn_society_overlay(mut commands: Commands) {
    commands.spawn((
        SocietyOverlay,
        Text::new(""),
        TextFont { font_size: 14.0, ..default() },
        TextColor(Color::srgba(1.0, 0.85, 0.6, 0.92)),
        Node {
            position_type: PositionType::Absolute,
            top: Val::Px(28.0),
            left: Val::Px(8.0),
            ..default()
        },
        Visibility::Hidden,
    ));
}

fn update_society_overlay(
    editor: Res<crate::ui_editor::UiEditorState>,
    soc: Res<Society>,
    day: Res<crate::sky2d::DayClock>,
    npcs: Query<&Npc>,
    mut overlay: Query<(&mut Text, &mut Visibility), With<SocietyOverlay>>,
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
    let n = npcs.iter().count();
    let (mut worst, mut best) = (1.0f32, -1.0f32);
    let mut fams: std::collections::HashSet<u32> = std::collections::HashSet::new();
    for np in &npcs {
        fams.insert(np.family);
        let r = soc.rep(np.family);
        worst = worst.min(r);
        best = best.max(r);
    }
    if n == 0 {
        worst = 0.0;
        best = 0.0;
    }
    // S6: stock/salud/roles de la colonia del cuerpo activo — verificar la economía sin
    // herramientas aparte.
    let (farmers, miners, builders, wanderers) = npcs.iter().fold((0, 0, 0, 0), |(f, m, b, w), np| {
        match np.role {
            Role::Farmer => (f + 1, m, b, w),
            Role::Miner => (f, m + 1, b, w),
            Role::Builder => (f, m, b + 1, w),
            Role::Wanderer => (f, m, b, w + 1),
        }
    });
    let body = soc.active_body().unwrap_or(0);
    let stock = soc.stock(body);
    let health = soc.body_health(body, n as u32);
    // S5: promedio de necesidades del pueblo + fase de rutina — verificar el horario
    // (trabajo/plaza/casa/duerme) sin herramientas aparte.
    let (mut food_avg, mut rest_avg, mut belong_avg) = (0.0f32, 0.0f32, 0.0f32);
    for np in &npcs {
        food_avg += np.needs.food;
        rest_avg += np.needs.rest;
        belong_avg += np.needs.belonging;
    }
    if n > 0 {
        food_avg /= n as f32;
        rest_avg /= n as f32;
        belong_avg /= n as f32;
    }
    let (hh, mm) = day.clock_hm();
    let routine = match routine_phase(day.phase()) {
        RoutinePhase::Sleep => "DUERME",
        RoutinePhase::GoToWork => "VA A TRABAJAR",
        RoutinePhase::Work => "TRABAJA",
        RoutinePhase::Plaza => "PLAZA",
        RoutinePhase::GoHome => "VUELVE A CASA",
    };
    let line = format!(
        "SOCIEDAD  npcs {n}  familias {}  reput [{:+.2} .. {:+.2}]\n\
         COLONIA  salud {:.0}%  comida {:.1}  madera {:.1}  piedra {:.1}  \
         granj {farmers} min {miners} constr {builders} libre {wanderers}\n\
         RUTINA  {hh:02}:{mm:02} {routine}  necesidad comida {:.0}% descanso {:.0}% pertenencia {:.0}%",
        fams.len(),
        worst,
        best,
        health * 100.0,
        stock.food,
        stock.wood,
        stock.stone,
        food_avg * 100.0,
        rest_avg * 100.0,
        belong_avg * 100.0,
    );
    if text.0 != line {
        text.0 = line;
    }
}

/// F7 — publica los avistamientos de NPCs por cuerpo (para la cámara). Remotos desde
/// `records`, cuerpo actual desde los NPCs vivos. Color por disposición hacia el jugador
/// (rencor → rojizo; cordial → azulado). Throttle 2 s.
#[allow(clippy::too_many_arguments)]
fn update_npc_sightings(
    time: Res<Time>,
    region: Res<RegionRuntime>,
    ctx: Res<WorldContextRuntime>,
    planet: Res<ActivePlanet>,
    soc: Res<Society>,
    mut sightings: ResMut<crate::sightings::WorldSightings>,
    live: Query<&Npc>,
    mut acc: Local<f32>,
) {
    *acc += time.delta_secs();
    if *acc < 2.0 {
        return;
    }
    *acc = 0.0;
    use crate::sightings::Sighting;
    let npc_color = |rep: f32| -> [f32; 3] {
        if rep < 0.0 {
            [0.9, 0.5 + 0.4 * (1.0 + rep).max(0.0), 0.5]
        } else {
            [0.55, 0.75, 0.95]
        }
    };
    sightings.npc.clear();
    for rec in &soc.records {
        let rep = soc.rep(rec.family);
        sightings.npc.entry(rec.body).or_default().push(Sighting {
            x: rec.x,
            z: rec.z,
            color: npc_color(rep),
            hostile: rep <= AGGRO_THRESHOLD,
        });
    }
    if surface_region_active(&region).is_some() {
        let b = active_surface_body(&ctx, &planet) as u8;
        for n in &live {
            let rep = soc.rep(n.family);
            sightings.npc.entry(b).or_default().push(Sighting {
                x: n.x,
                z: n.z,
                color: npc_color(rep),
                hostile: rep <= AGGRO_THRESHOLD,
            });
        }
    }
}

// ── Helpers para el menú de NPC (`npc_menu.rs`) ──────────────────────────────

/// Empuja una frase al historial del NPC, conservando sólo las últimas 5.
pub(crate) fn push_history(history: &mut Vec<String>, line: &str) {
    history.push(line.to_string());
    let n = history.len();
    if n > 5 {
        history.drain(0..n - 5);
    }
}

/// Nombre estable (ASCII) derivado del uid — para el título del menú del NPC.
/// E5: nombre + APELLIDO de familia (determinista) — antes era `Ana 0`, sufijo numérico
/// que colisionaba cada 2400 uids y encima se leía como inventario, no como pueblo. Ahora
/// "Ana Vero", "Beto Vero" se leen como parientes.
pub(crate) fn npc_name(uid: u64, family: u32) -> String {
    const NAMES: [&str; 24] = [
        "Ana", "Beto", "Cira", "Dario", "Ema", "Fito", "Gala", "Hugo", "Ines", "Juno", "Kira",
        "Leo", "Mara", "Nilo", "Ona", "Pipo", "Rea", "Sami", "Tavo", "Uma", "Vito", "Wara",
        "Yago", "Zoe",
    ];
    const SURNAMES: [&str; 20] = [
        "Vero", "Cato", "Nix", "Solar", "Duna", "Reyes", "Cielo", "Ferro", "Nova", "Cruz",
        "Marmol", "Rocio", "Alba", "Torres", "Viento", "Rios", "Selva", "Brava", "Luz", "Piedra",
    ];
    let first = NAMES[(uid as usize) % NAMES.len()];
    let last = SURNAMES[(family as usize) % SURNAMES.len()];
    format!("{first} {last}")
}

/// Rol del NPC (ASCII) según su disposición — decide qué botones muestra el menú.
pub(crate) fn npc_role(faith: f32, converted: bool, rep: f32) -> &'static str {
    if converted {
        "FIEL"
    } else if rep <= AGGRO_THRESHOLD {
        "HOSTIL"
    } else if faith > 0.3 {
        "CREYENTE"
    } else {
        "ALDEANO"
    }
}
