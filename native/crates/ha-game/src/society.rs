//! F6 — Sociedad viva con MEMORIA. Colonos/NPCs con cerebro IAFZ + **familia** + una
//! **reputación del jugador POR FAMILIA que persiste**: si matás a un miembro, su familia
//! te lo recuerda (rencor) y su conducta reacciona. Reusa el patrón del vivario
//! (`fauna.rs`): activación por cuerpo + persistencia. Ver `PLAN_SOCIEDAD_MEMORIA.md`.
//! No podar lo emergente: la memoria abre reacciones, no impone guiones.
//!
//! ## LOS PUEBLOS ESTÁN DONDE DICE LA GEOGRAFÍA (no alrededor tuyo)
//!
//! Antes la sociedad se sembraba en tu posición al pisar el cuerpo: aterrizabas en mitad
//! de la nada y te brotaban nueve aldeanos al lado. Se acabó. Ahora la gente vive en los
//! ASENTAMIENTOS de `ha_shared::geography` (aldea / pueblo / ciudad, deterministas y con
//! su sitio fijo en el planeta) y se siembra cuando el jugador **entra** en uno
//! (`discover_settlements`). Si nunca caminás hasta un pueblo, no ves un alma — encontrar
//! gente es parte de explorar, igual que encontrar fauna.
//!
//! Y el pueblo NO se congela cuando te vas: los NPCs lejos siguen su rutina en modo barato
//! (`advance_far`) y la economía corre para todos los cuerpos (`tick_colony_economy`), así
//! que al volver de noche los encontrás en sus casas y con el stock que les tocaba.

use std::collections::HashMap;
use std::path::PathBuf;

use bevy::prelude::*;
use serde::{Deserialize, Serialize};

use ha_shared::credos::{Credos, CREDO_KI};
use ha_shared::hogares::{self, Huella};
use ha_shared::linaje::{self, Etapa, Parentesco};
use ha_shared::sims::{self, Actividad, Clase, Motivo, Vida};
use ha_shared::sociedad::{self, Sitio, Vecino};
use ha_shared::CelestialBody;

use crate::input::{AbilityTapPulse, AimCursor};
use crate::platform::is_app_paused;
use crate::scene::{RegionRuntime, WorldContextRuntime};
use crate::sim::Player;
use crate::world2d::{
    active_surface_body, render_xy, sort_z, surface_region_active, ActivePlanet,
    TileWorld,
};

const NPC_ID_BASE: i32 = 72; // fauna 0..48, residentes 48..72, NPCs 72..120
const MAX_NPCS: usize = 48;
/// Cada cuántos segundos se mira si el jugador entró en un asentamiento sin sembrar.
const DISCOVER_EVERY: f32 = 2.0;
/// Cuánta gente tiene cada rango de asentamiento al fundarse. La ciudad no llega a los 48
/// del tope de golpe: crece sola con los nacimientos si la colonia da de comer.
fn tier_population(tier: ha_shared::geography::SettlementTier) -> u32 {
    use ha_shared::geography::SettlementTier as T;
    match tier {
        T::Hamlet => 5,
        T::Town => 11,
        T::City => 18,
    }
}
const NPC_VEL: f32 = 2.2;
/// Radio del cuerpo de un vecino al chocar con el mundo (el del jugador mide 0,34).
const NPC_RADIO: f32 = 0.3;
const NPC_TURN: f32 = 2.0;
const RENDER_R: f32 = 80.0; // render/sim sólo cerca (como los residentes)
/// STREAMING de gente. Los 48 cerebros de `MAX_NPCS` son un presupuesto GLOBAL, y antes se
/// los quedaba para siempre el primer pueblo que pisabas: al llegar después a una ciudad a
/// 2 km, `discover_settlements` veía `alive == MAX_NPCS` y cortaba, así que la ciudad
/// quedaba desierta para siempre (invariante `ciudad poblada` de `HA_ECOTEST`).
///
/// Ahora el presupuesto es LOCAL: quien queda lejos vuelve a ser `NpcRecord` y sigue
/// viviendo su rutina en `tick_society_remote` (`advance_far`), sin entidad ni cerebro.
/// Nadie desaparece — deja de estar renderizado, que es distinto. Con histéresis para que
/// caminar sobre el borde no haga spawn/despawn cada frame.
const VIRT_OUT_R: f32 = 300.0; // más lejos que esto → se virtualiza
const VIRT_IN_R: f32 = 220.0; // más cerca que esto → se materializa
/// Cada cuánto corre el streaming. No hace falta por frame: a `NPC_VEL` (2.2 m/s) el borde
/// de 80 m de histéresis tarda decenas de segundos en cruzarse.
const STREAM_EVERY: f32 = 1.0;
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
// F6.3 roles/hostilidad de grupo + recambio generacional
const AGGRO_THRESHOLD: f32 = -0.55; // rep por debajo → la familia te ENFRENTA (no huye)
// --- F7 RELIGIÓN + propósito (role-play) ---
const NUM_SKINS: u64 = 20; // hojas horneadas por el Character Generator (npc/char_1..20)
/// Tamaño en mundo del aldeano: el MISMO que el jugador (`scene.rs`), porque
/// ahora comparten hoja y frame (16×32 = 1:2). Un humano de ~1,9 m.
const NPC_SIZE: Vec2 = Vec2::new(0.95, 1.9);
const HOME_WANDER_R: f32 = 5.0; // radio de deambular alrededor del hogar (pueblo)
/// **NINGÚN NPC CAMINA HACIA EL JUGADOR POR FE.** La secta existe, pero es del
/// pueblo, no tuya: los fieles hacen su vida y se juntan a rezar en el santuario
/// de SU pueblo (`shrine_of`). Antes el converso te tomaba de ídolo y te seguía
/// —o te orbitaba a 2,8 m— así que el pueblo entero terminaba caminando atrás
/// del jugador. Eso se fue entero; si volvés a ver un `toward_p` con `converted`
/// al lado, es el bug de vuelta.
const SHRINE_OFFSET: f32 = 7.5; // el santuario está a un costado de la plaza
const PREACH_RADIUS: f32 = 6.0; // un fiel contagia a los vecinos de este radio
const FAITH_SPREAD: f32 = 0.03; // fe/s de boca en boca (~20 s de charla al lado)
const FAITH_COOL: f32 = 0.01; // fe/s que se pierde sin nadie que predique
/// D8: única definición — `npc_menu.rs` la importa en vez de duplicarla.
pub(crate) const CONVERT_AT: f32 = 0.6; // fe para convertirse en fiel
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

/// **De noche, las casas con alguien despierto adentro tienen la luz encendida.** Una sola luz
/// cálida en la puerta por casa; se apaga sola cuando todos se van o se duermen. Sólo las de
/// alrededor del jugador: más lejos no se ven y no valen el costo.
fn luces_de_casas(
    time: Res<Time>,
    day: Res<crate::sky2d::DayClock>,
    mut commands: Commands,
    npcs: Query<&Npc>,
    player: Query<&Transform, With<Player>>,
    luces: Query<(Entity, &LuzDeCasa)>,
    mut acc: Local<f32>,
) {
    *acc += time.delta_secs();
    if *acc < 1.0 {
        return;
    }
    *acc = 0.0;
    let Ok(pt) = player.get_single() else { return };
    let (px, pz) = (pt.translation.x, -pt.translation.y);
    let noche = matches!(day.phase_name(), "NOCHE" | "ATARDECER");
    let mut deben: Vec<Huella> = Vec::new();
    if noche {
        for n in &npcs {
            let Some(c) = n.vivienda.casa else { continue };
            let dormido = n.vida.actividad() == Actividad::Dormir;
            if !dormido && c.dentro(n.x, n.z) && !deben.contains(&c) && (c.puerta.0 as f32 - px).abs() < 60.0 && (c.puerta.1 as f32 - pz).abs() < 60.0 {
                deben.push(c);
            }
        }
    }
    for (e, l) in &luces {
        if !deben.iter().any(|c| c.min == l.min) {
            commands.entity(e).despawn_recursive();
        }
    }
    for c in deben {
        if luces.iter().any(|(_, l)| l.min == c.min) {
            continue;
        }
        let u = c.umbral();
        let r = render_xy(u.0, u.1);
        commands.spawn((
            LuzDeCasa { min: c.min },
            crate::light2d::Light2d {
                color: Color::srgb(1.0, 0.82, 0.48),
                radius: 3.6,
                intensity: 0.9,
                flicker: 0.08,
                ..default()
            },
            Transform::from_xyz(r.x, r.y, sort_z(u.1)),
            Visibility::default(),
        ));
    }
}

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

// --- Vida de los aldeanos: motivos + objetos que anuncian (ha_shared::sims) --------------
// El horario ya no es una tabla de fases: cada aldeano tiene hambre, sed, sueño, ganas de
// gente, de rezar y de plata, y elige entre lo que el pueblo le ofrece (casa, puesto, plaza,
// santuario) y lo que le ofrecen sus vecinos (charla, ayuda, hurto, prédica). La hora del
// día (`sky2d::DayClock`) sólo modula el sueño y el trabajo. Ver biblia §6.
const WORK_DIST: f32 = 8.0; // qué tan lejos de casa está el "puesto de trabajo"
const FOOD_MIN_STOCK: f32 = 0.3; // sin esta reserva en la despensa del cuerpo, no hay qué comer

/// Puesto de trabajo determinista (uid+family): sin edificios propios todavía, pero
/// estable — el mismo aldeano siempre "trabaja" en el mismo punto cerca de casa.
fn workplace_of(home_x: f32, home_z: f32, uid: u64, family: u32) -> (f32, f32) {
    let a = (uid as f32) * 2.399_963_2 + (family as f32) * 0.7;
    (home_x + a.cos() * WORK_DIST, home_z + a.sin() * WORK_DIST)
}

/// Los EDIFICIOS de una vida: la casa donde duerme, el puesto donde trabaja y el parque donde
/// juega. Salen de la ciudad de verdad (`ha_shared::hogares`): con puerta, con pared, con calle.
/// Un vecino sin casa (colono lunar, partida vieja sin migrar) cae a puntos sueltos.
#[derive(Clone, Copy, Default, Debug, PartialEq, Serialize, Deserialize)]
pub(crate) struct Vivienda {
    pub casa: Option<Huella>,
    pub puesto: Option<Huella>,
    pub parque: Option<(f32, f32)>,
}

impl Vivienda {
    /// Los edificios propios (0, 1 o 2), en un arreglo fijo: se consulta cada tick.
    fn huellas(&self) -> ([Huella; 2], usize) {
        let vacia = Huella { min: (0, 0), w: 0, h: 0, puerta: (0, 0) };
        let mut out = [vacia; 2];
        let mut n = 0;
        for h in [self.casa, self.puesto].into_iter().flatten() {
            out[n] = h;
            n += 1;
        }
        (out, n)
    }
}

/// Lo que una ciudad ofrece para poblarla: casas, puestos y un parque, todo con puerta real.
/// Se arma en un hilo aparte (`discover_settlements`): barrer lotes cuesta decenas de ms.
#[derive(Clone, Default)]
pub(crate) struct Barrio {
    casas: Vec<Huella>,
    puestos: Vec<Huella>,
    parque: Option<(f32, f32)>,
}

fn barrio_de(body: CelestialBody, s: ha_shared::geography::Settlement, hogares_n: usize) -> Barrio {
    let radio = s.radius.clamp(24, 160);
    let edificios = hogares::edificios_alrededor(body, &s, radio, hogares_n, 4);
    Barrio {
        casas: edificios.iter().filter(|e| e.es_hogar()).map(|e| e.huella).collect(),
        puestos: edificios.iter().filter(|e| e.es_puesto()).map(|e| e.huella).collect(),
        parque: hogares::parque_cerca(body, &s, radio.min(60)),
    }
}

/// Los lugares de una vida: casa, puesto, plaza, fuente (a un costado de la plaza) y santuario.
fn sitio_de(home: (f32, f32), town: (f32, f32), uid: u64, family: u32, viv: &Vivienda) -> Sitio {
    let plaza = town;
    let pozo = (town.0 - 4.0, town.1 + 3.0);
    let altar = shrine_of(town.0, town.1);
    let parque = viv.parque.unwrap_or((town.0 + 3.0, town.1 + 4.0));
    match viv.casa {
        Some(c) => Sitio {
            casa: c.adentro(uid),
            casa_uso: 1.0,
            trabajo: viv.puesto.map_or_else(|| workplace_of(c.escalon().0, c.escalon().1, uid, family), |p| p.escalon()),
            plaza,
            pozo,
            altar,
            parque,
        },
        None => Sitio { casa: home, casa_uso: 0.0, trabajo: workplace_of(home.0, home.1, uid, family), plaza, pozo, altar, parque },
    }
}

/// LOD LEJANO: sin cerebro, sin colisión y sin trato con vecinos, pero la misma cabeza — sigue
/// eligiendo entre lo que el pueblo ofrece y camina en línea recta. Si te vas al mediodía y
/// volvés de noche, están en sus casas con las barras que les tocaban. Devuelve la posición.
fn avanzar_lejos(v: &mut Vida, pos: (f32, f32), sitio: &Sitio, viv: &Vivienda, fase: f32, comida: bool, dt: f32) -> (f32, f32) {
    v.tick(Clase::Aldeano, fase, dt, pos);
    if v.necesita_decidir(Clase::Aldeano, fase) {
        let mut ads = Vec::with_capacity(8);
        sociedad::anuncios_sitio(sitio, fase, comida, v.credo != 0, &mut ads);
        v.decidir(Clase::Aldeano, pos, fase, &ads);
    }
    match v.destino() {
        Some((tx, tz, false)) => {
            let (hs, nh) = viv.huellas();
            let (tx, tz) = hogares::siguiente_punto(pos, (tx, tz), &hs[..nh]);
            let (dx, dz) = (tx - pos.0, tz - pos.1);
            let d = (dx * dx + dz * dz).sqrt().max(1e-3);
            let step = (NPC_VEL * 0.75 * dt).min(d);
            (pos.0 + dx / d * step, pos.1 + dz / d * step)
        }
        _ => pos,
    }
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
    /// Piezas que los constructores del pueblo llevan colocadas (mobiliario real del parque).
    #[serde(default)]
    pub(crate) obras: u16,
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
    pub(crate) age: f32, // edad (s de sim): define la etapa (niño / adulto / anciano), ver `ha_shared::linaje`
    /// Uid de su pareja (0 = soltero) y de sus padres (0 = fundador: sembrado, no nacido aquí).
    pub(crate) pareja: u64,
    pub(crate) padre: u64,
    pub(crate) madre: u64,
    // --- Religión + propósito (runtime; se reconstruye cerca del ídolo) ---
    pub(crate) faith: f32,      // 0..1 devoción hacia el fundador; sube cerca del ídolo
    pub(crate) converted: bool, // fiel: sigue/venera al ídolo (fundador)
    pub(crate) home_x: f32,      // ancla de su CASA (pos de spawn) — deambula cerca, no en círculos
    pub(crate) home_z: f32,
    /// Centro del PUEBLO al que pertenece (la plaza). Es el asentamiento de `geography`
    /// donde nació; ya no hay "un centro por cuerpo", puede haber varios pueblos.
    pub(crate) town_x: f32,
    pub(crate) town_z: f32,
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
    /// La cabeza: hambre, sed, sueño, ganas de gente, fe, plata, carácter, lazos y credo.
    pub(crate) vida: Vida,
    /// Segundos hasta poder avisar otra vez de que se topó con algo (evita rendirse al toque).
    bump: f32,
    /// Dónde duerme, dónde trabaja y dónde juega: edificios con puerta de la ciudad.
    pub(crate) vivienda: Vivienda,
    /// Camino A* vigente (puntos de mundo, en orden) cuando un edificio ajeno le cortó el paso.
    ruta: Vec<(f32, f32)>,
    /// El destino del plan para el que se calculó `ruta`: si el plan cambia, la ruta se tira.
    ruta_meta: (f32, f32),
    /// Chocó y espera que el buscador le arme un camino (no camina mientras tanto).
    pide_ruta: bool,
    /// Ticks seguidos sin poder avanzar ni por búsqueda: pasado el tope, se lo saca del apuro.
    atasco: u16,
}

/// El resultado de mirar a todos los vecinos de golpe (lo usa el driver `HA_ECOTEST`).
#[derive(Debug, Default, Clone, Copy)]
pub(crate) struct AuditoriaNpc {
    pub total: usize,
    pub con_casa: usize,
    pub dentro_de_casa: usize,
    pub con_ruta: usize,
    pub atascados: usize,
}

/// Cuenta, entre los vecinos a menos de `radio` de `(px, pz)`, cuántos tienen casa, cuántos
/// están adentro de ella, cuántos siguen una ruta A* y cuántos están trabados.
pub(crate) fn auditar(world: &mut World, px: f32, pz: f32, radio: f32) -> AuditoriaNpc {
    let mut a = AuditoriaNpc::default();
    let mut q = world.query::<&Npc>();
    for n in q.iter(world) {
        if (n.x - px).powi(2) + (n.z - pz).powi(2) > radio * radio {
            continue;
        }
        a.total += 1;
        if let Some(c) = n.vivienda.casa {
            a.con_casa += 1;
            if c.contiene(n.x, n.z, 0.0) {
                a.dentro_de_casa += 1;
            }
        }
        if !n.ruta.is_empty() {
            a.con_ruta += 1;
        }
        if n.atasco >= 30 {
            a.atascados += 1;
        }
    }
    a
}

/// Para el QA: le da madera de sobra al pueblo activo y nombra Constructor a un vecino.
pub(crate) fn forzar_constructor(world: &mut World) {
    let body = {
        let ctx = world.resource::<WorldContextRuntime>();
        let planet = world.resource::<ActivePlanet>();
        active_surface_body(ctx, planet) as u8
    };
    let mut stock = world.resource::<Society>().stock(body);
    stock.wood = 500.0;
    world.resource_mut::<Society>().stock.insert(body, stock);
    let mut q = world.query::<&mut Npc>();
    if let Some(mut n) = q.iter_mut(world).next() {
        n.role = Role::Builder;
    }
}

/// Qué pose lleva puesta un vecino (caché para no rearmar el sprite cada tick).
#[derive(Component, Default)]
pub(crate) struct NpcPose {
    pedida: String,
    aplicada: String,
}

/// La luz encendida de una casa donde hay alguien despierto de noche.
#[derive(Component)]
pub(crate) struct LuzDeCasa {
    min: (i64, i64),
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
    pareja: u64,
    #[serde(default)]
    padre: u64,
    #[serde(default)]
    madre: u64,
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
    /// Plaza de SU pueblo. `None` en saves viejos (había un solo centro por cuerpo) → cae
    /// a su casa, que es el comportamiento razonable para un aldeano sin pueblo.
    #[serde(default)]
    town_x: Option<f32>,
    #[serde(default)]
    town_z: Option<f32>,
    /// E4: últimas frases dichas — sobrevive a cambios de cuerpo/reinicio.
    #[serde(default)]
    history: Vec<String>,
    /// C1: vida (dos golpes para matar). Los saves viejos no la traen → sanos.
    #[serde(default = "full_health")]
    health: f32,
    /// S6: oficio — persiste para que el pueblo no "olvide" quién hace qué al recargar.
    #[serde(default)]
    role: Role,
    /// La cabeza persiste: carácter, barras, lazos y credo. Los saves viejos no la traen →
    /// se siembra de su uid al despertarlos.
    #[serde(default)]
    vida: Option<Vida>,
    /// Su casa, su puesto y su parque (edificios reales). Los saves viejos no lo traen: se
    /// les asigna al pisar su pueblo (`asignar_viviendas`).
    #[serde(default)]
    vivienda: Vivienda,
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
    /// Asentamientos YA SEMBRADOS `(cuerpo, centro_x, centro_z)`. El sitio sale de
    /// `geography::settlement_*` (fijo en el planeta), no de dónde aterrizaste; esta lista
    /// sólo recuerda a cuáles ya les pusimos gente para no duplicarla al volver.
    #[serde(default)]
    centers: Vec<(u8, f32, f32)>,
    /// S4/B3: reputación acumulada por CUERPO (sobrevive a la extinción de una familia) —
    /// matar a todos los miembros de una familia no lava la memoria del planeta.
    #[serde(default)]
    body_rep: Vec<(u8, f32)>,
    /// S6: recursos de la colonia por cuerpo.
    #[serde(default)]
    stock: Vec<(u8, Stock)>,
    /// Las sectas nacidas de la sociedad (la del jugador vive aparte, en `Religion`).
    #[serde(default)]
    pub(crate) credos: Credos,
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
    /// Asentamientos ya sembrados (un cuerpo puede tener varios pueblos). Ver el campo
    /// homónimo de `SocietySave`.
    centers: Vec<(u8, f32, f32)>,
    /// S4/B3: reputación por cuerpo — sobrevive a familias extintas.
    body_rep: HashMap<u8, f32>,
    /// S6: recursos de la colonia por cuerpo — lo que producen/consumen los roles.
    pub(crate) stock: HashMap<u8, Stock>,
    /// Sectas de la sociedad: nacen de un carismático, reclutan y se odian entre sí.
    pub(crate) credos: Credos,
    /// Contadores de la vida social, para QA (no persisten).
    pub(crate) stats: SocStats,
    /// A1: se pone en `true` cuando algo cambió el estado persistente fuera del ciclo
    /// normal (p. ej. una muerte) y todavía no se volcó a disco. `save_society_periodic`
    /// es el ÚNICO camino de escritura; esto le pide un flush inmediato con `live` incluido.
    dirty: bool,
    /// Muertes sin repartir: `(uid, pareja, vivienda)`. Las procesa `repartir_herencias`.
    herencias: Vec<(u64, u64, Vivienda)>,
    /// Casas, puestos y parque de cada asentamiento `(cuerpo, cx, cz)`, ya calculados.
    barrios: HashMap<(u8, i64, i64), Barrio>,
    /// Barrios que se están calculando en un hilo aparte.
    barrios_en_curso: HashMap<(u8, i64, i64), bevy::tasks::Task<Barrio>>,
    /// La búsqueda de camino en curso (una a la vez, por tramos): de quién es, el buscador,
    /// desde dónde salió y el destino del plan.
    busqueda: Option<(u64, ha_shared::ruta::Buscador, (f32, f32), (f32, f32))>,
}

/// Lo que pasó en la sociedad desde que abriste el juego.
#[derive(Default, Clone, Copy)]
pub(crate) struct SocStats {
    hurtos: u32,
    agresiones: u32,
    muertes: u32,
    detenciones: u32,
    ayudas: u32,
    conversiones: u32,
    fundados: u32,
    pub(crate) parejas: u32,
    nacimientos: u32,
    herencias: u32,
}

impl Default for Society {
    fn default() -> Self {
        Self {
            family_rep: HashMap::new(),
            records: Vec::new(),
            free_ids: (NPC_ID_BASE..NPC_ID_BASE + MAX_NPCS as i32).rev().collect(),
            herencias: Vec::new(),
            next_uid: 1,
            next_family: 1,
            loaded: false,
            active_body: None,
            alive: 0,
            centers: Vec::new(),
            body_rep: HashMap::new(),
            stock: HashMap::new(),
            credos: Credos::default(),
            stats: SocStats::default(),
            dirty: false,
            barrios: HashMap::new(),
            barrios_en_curso: HashMap::new(),
            busqueda: None,
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
/// discípulo). Un solo culto por ahora.
///
/// Se funda PREDICANDO: el primer aldeano que convencés (menú del NPC) funda el
/// culto. De ahí en más crece solo, de vecino a vecino, y los fieles siguen
/// haciendo su vida — **no te veneran caminándote atrás**. Ver `SHRINE_OFFSET`.
#[derive(Resource, Default)]
pub struct Religion {
    pub founded: bool,
    pub name: String,
    pub followers: u32,
}

impl Religion {
    /// Nace el culto. Idempotente: llamarla dos veces no lo re-funda ni le
    /// cambia el nombre.
    pub fn found(&mut self) {
        if self.founded {
            return;
        }
        self.founded = true;
        self.name = "Los Hijos del Ki".to_string();
        eprintln!("ha-religion: FUNDADA '{}' (primer converso)", self.name);
    }
}

/// El santuario del pueblo: un punto fijo a un costado de su plaza. No es un
/// edificio (no hay slot de arte todavía) — es el lugar al que van a rezar los
/// fieles a la hora de la plaza. Determinista a partir del centro del pueblo,
/// así que dos pueblos distintos tienen su propio santuario y el corro siempre
/// se arma en el mismo sitio.
pub(crate) fn shrine_of(town_x: f32, town_z: f32) -> (f32, f32) {
    (town_x + SHRINE_OFFSET, town_z - SHRINE_OFFSET)
}

/// Frase por CONTEXTO (fe/reputación/religión) — ASCII-only. `variant` desempata
/// para que no todos digan lo mismo. Es la base del "chat" con respuestas
/// predefinidas según lo que venimos construyendo (culto, pueblo, rencor).
/// `name` es el nombre de la RELIGION; `player` es como te llamaste vos en la
/// pantalla de creacion. El saludo neutro te nombra: es lo que hace que el
/// nombre que elegiste siga existiendo despues del menu y no sea un campo
/// decorativo. Vacio o por defecto, se cae al "viajero" de siempre.
pub(crate) fn npc_line(
    rep: f32,
    converted: bool,
    faith: f32,
    founded: bool,
    name: &str,
    player: &str,
    variant: u64,
) -> String {
    if converted {
        // Nada de "te sigo": el fiel cree, no te escolta. Habla de SU culto y de
        // SU santuario, que es donde va a estar cuando lo busques.
        let v = [
            format!("Alabado sea {name}!"),
            "El Profeta nos enseñó, el resto es nuestro.".to_string(),
            "Uno con el Ki.".to_string(),
            format!("Somos {name}."),
            "Al mediodía rezamos en el santuario.".to_string(),
        ];
        return v[(variant as usize) % v.len()].clone();
    }
    if rep <= AGGRO_THRESHOLD {
        let v = ["No te queremos aquí.", "Pagarás por lo que hiciste.", "Vete!"];
        return v[(variant as usize) % v.len()].to_string();
    }
    if rep < -0.15 {
        let v = ["No me fío de ti.", "Mantente lejos.", "Te estoy vigilando."];
        return v[(variant as usize) % v.len()].to_string();
    }
    if founded && faith > 0.2 {
        let v = [
            format!("Que es {name}?"),
            "Ese forastero tiene algo...".to_string(),
            "Siento el Ki en él.".to_string(),
        ];
        return v[(variant as usize) % v.len()].clone();
    }
    let who = if player.trim().is_empty() { "viajero" } else { player.trim() };
    let v = [
        format!("Buen dia, {who}."),
        "El pueblo crece.".to_string(),
        format!("{who}. Que andas buscando?"),
        "Lindo dia.".to_string(),
    ];
    v[(variant as usize) % v.len()].clone()
}

impl Society {
    /// Parejas formadas en el pueblo (lo lee `metas.rs`).
    pub fn stats_parejas(&self) -> u32 {
        self.stats.parejas as u32
    }
}

pub struct SocietyPlugin;

impl Plugin for SocietyPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(bevy::prelude::OnExit(crate::game_phase::GamePhase::Playing), (crate::salir::reiniciar::<Society>, crate::salir::reiniciar::<Religion>,));
        app.init_resource::<Society>()
            .init_resource::<Religion>()
            // La sociedad NPC vive SÓLO en partida. Antes se poblaba en el primer
            // frame: había aldeanos caminando (y hablando) detrás del splash y del
            // menú — el "gameplay" que asomaba al abrir el juego.
            .add_systems(
                Update,
                (
                    manage_society,
                    // Antes que `discover_settlements`: libera los cerebros de los que
                    // quedaron lejos para que el asentamiento donde estás parado tenga
                    // lugar en el MISMO tick en que lo descubrís.
                    stream_npcs,
                    discover_settlements,
                    tick_society,
                    parejas_del_pueblo,
                    tick_society_remote,
                    harm_events,
                    repartir_herencias,
                    tick_colony_economy,
                    constructor_obra,
                    save_society_periodic,
                )
                    .chain()
                    .run_if(crate::game_phase::world_active),
            )
            .add_systems(Startup, (spawn_society_overlay, force_creative_debug))
            .add_systems(
                Update,
                (update_society_overlay, update_npc_sightings, luces_de_casas)
                    .run_if(crate::game_phase::world_active),
            );
    }
}

// --- Persistencia ------------------------------------------------------------

fn society_dir() -> PathBuf {
    crate::inventory_persist::persist_dir().join("society")
}

fn society_json() -> PathBuf {
    society_dir().join("society.json")
}

fn load_society() -> SocietySave {
    crate::persistent_agents::cargar_json(&society_json()).unwrap_or_default()
}

/// Guarda TODO: registros de otros cuerpos + snapshot de los NPCs vivos del cuerpo actual
/// + la memoria (reputación) + la religión (A3). Barato (≤ `MAX_NPCS`). Único camino de
/// escritura (A1) — quien quiera persistir marca `soc.dirty` y espera este flush.
fn save_society(soc: &Society, live: &[NpcRecord], religion: &Religion) {
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
        centers: soc.centers.clone(),
        body_rep: soc.body_rep.iter().map(|(&b, &r)| (b, r)).collect(),
        stock: soc.stock.iter().map(|(&b, &s)| (b, s)).collect(),
        credos: soc.credos.clone(),
    };
    crate::persistent_agents::guardar_json(&society_json(), &save);
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
                pareja: rec.pareja,
                padre: rec.padre,
                madre: rec.madre,
                faith: rec.faith,
                converted: rec.converted,
                // B1: el hogar es la identidad del NPC, no su posición actual — sólo cae a
                // `rec.x/z` la primera vez que un registro viejo no lo trae (migración).
                home_x: rec.home_x.unwrap_or(rec.x),
                home_z: rec.home_z.unwrap_or(rec.z),
                town_x: rec.town_x.unwrap_or_else(|| rec.home_x.unwrap_or(rec.x)),
                town_z: rec.town_z.unwrap_or_else(|| rec.home_z.unwrap_or(rec.z)),
                say_cd: (rec.uid % 7) as f32, // desfasa el chatter
                history: rec.history.clone(),
                health: rec.health,
                calm_cd: 0.0,
                convert_cd: 0.0,
                role: rec.role,
                vida: rec.vida.unwrap_or_else(|| Vida::nueva(rec.uid)),
                bump: 0.0,
                vivienda: rec.vivienda,
                ruta: Vec::new(),
                ruta_meta: (0.0, 0.0),
                pide_ruta: false,
                atasco: 0,
            },
            NpcPose::default(),
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
) {
    if !soc.loaded {
        let s = load_society();
        soc.records = s.npcs;
        soc.family_rep = s.family_rep.into_iter().collect();
        soc.next_uid = s.next_uid.max(soc.next_uid);
        soc.next_family = s.next_family.max(soc.next_family);
        religion.founded = s.religion.founded; // A3
        religion.name = s.religion.name;
        soc.centers = s.centers;
        soc.body_rep = s.body_rep.into_iter().collect();
        soc.stock = s.stock.into_iter().collect();
        soc.credos = s.credos;
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
        // OJO: acá había un `retain(|r| r.body != old)` a secas. Valía cuando en el cuerpo
        // activo TODO el mundo estaba vivo, así que un registro suyo sólo podía ser basura
        // vieja. Con el streaming (`stream_npcs`) los lejanos son registros legítimos, y
        // barrerlos borraba pueblos enteros al despegar. Ahora sólo se van los que están a
        // punto de re-snapshotearse desde su entidad viva — o sea, los duplicados.
        let vivos: std::collections::HashSet<u64> = live.iter().map(|n| n.uid).collect();
        soc.records.retain(|r| r.body != old || !vivos.contains(&r.uid));
        let mut snap: Vec<NpcRecord> = Vec::new();
        for n in &live {
            snap.push(record_from(n, old));
        }
        soc.records.extend(snap);
        for e in &live_ents {
            commands.entity(e).despawn_recursive();
        }
        soc.free_ids = (NPC_ID_BASE..NPC_ID_BASE + MAX_NPCS as i32).rev().collect();
        soc.alive = 0;
        // NO se escribe a disco AQUÍ. Este bloque corre en el frame exacto en que
        // cruzás a `RegionId::Space`, que es el frame en el que ya se están
        // despawneando chunks, decoración, fauna, muros, cofres y máquinas: meterle
        // encima un `write` síncrono es parte del tirón medido al despegar (picos de
        // 200-490 ms en el Redmi). Se marca sucio y lo vuelca `save_society_periodic`,
        // que además ya es el ÚNICO camino de escritura (invariante A1).
        soc.dirty = true;
    }
    // Instancia el cuerpo nuevo.
    if let Some(nb) = current {
        let mut recs: Vec<NpcRecord> =
            soc.records.iter().filter(|r| r.body == nb).cloned().collect();
        soc.records.retain(|r| r.body != nb);
        // Semilla al ENTRAR al cuerpo: SÓLO la base lunar, que es TUYA (los colonos que
        // mandaste viven pegados a tus hábitats, no en un asentamiento procedural).
        //
        // Todo el resto de la gente del sistema solar la siembra `discover_settlements`
        // cuando llegás a un pueblo de `geography`. Acá NO se mira dónde está el jugador:
        // ése era el anillo de NPCs y ya no existe.
        if recs.is_empty()
            && nb == CelestialBody::Moon as u8
            && colony.state.colonists > 0
            && !already_seeded(&soc, nb, 0.0, 0.0)
        {
            recs = seed_society(nb, colony.state.colonists, Some(&machines), 0.0, planet_temperament_disposition(CelestialBody::Moon), (0.0, 0.0), None, &mut soc);
            soc.centers.push((nb, 0.0, 0.0));
        }
        // Los registros del cuerpo ya se sacaron de `soc.records` (arriba). Si se acaban los
        // cerebros, el que no entra tiene que VOLVER al pozo de registros: antes había un
        // `break` que lo dejaba caer, y como ya no estaba en `records` desaparecía del
        // guardado. Llegar a un cuerpo con más de `MAX_NPCS` habitantes te borraba el
        // excedente para siempre — el "se pierde el pueblo". Ahora sigue vivo como dato y
        // `stream_npcs` lo materializa cuando te le acercás.
        for rec in recs {
            let Some(brain) = soc.free_ids.pop() else {
                soc.records.push(rec);
                continue;
            };
            soc.alive += 1;
            spawn_npc(&mut commands, &art, &rec, brain);
        }
    }
    soc.active_body = current;
}

/// Congela un NPC vivo en su registro persistente. Un solo camino para las dos vías que
/// lo hacen: cambiar de cuerpo y el streaming por distancia.
fn record_from(n: &Npc, body: u8) -> NpcRecord {
    NpcRecord {
        uid: n.uid,
        family: n.family,
        body,
        x: n.x,
        z: n.z,
        heading: n.heading,
        energy: n.energy,
        age: n.age,
        pareja: n.pareja,
        padre: n.padre,
        madre: n.madre,
        faith: n.faith,
        converted: n.converted,
        home_x: Some(n.home_x),
        home_z: Some(n.home_z),
        town_x: Some(n.town_x),
        town_z: Some(n.town_z),
        history: n.history.clone(),
        health: n.health,
        role: n.role,
        vida: Some(n.vida),
        vivienda: n.vivienda,
    }
}

/// STREAMING de gente en el cuerpo activo (ver `VIRT_OUT_R`). Devuelve al pozo los cerebros
/// de quienes quedaron lejos y los vuelve a sacar cuando te acercás. Es lo que hace que el
/// presupuesto de 48 sea LOCAL y que entrar a una ciudad nueva siempre tenga lugar.
///
/// Invariante que sostiene: `soc.records` = gente que NO tiene entidad. Vale para todos los
/// cuerpos, incluido el activo — por eso `tick_society_remote` ya no saltea el activo.
#[allow(clippy::too_many_arguments)]
fn stream_npcs(
    time: Res<Time>,
    mut commands: Commands,
    art: Res<crate::art2d::ArtLibrary>,
    region: Res<RegionRuntime>,
    mut soc: ResMut<Society>,
    player: Query<&Transform, With<Player>>,
    live: Query<(Entity, &Npc)>,
    mut acc: Local<f32>,
) {
    *acc += time.delta_secs();
    if *acc < STREAM_EVERY {
        return;
    }
    *acc = 0.0;
    if !soc.loaded || surface_region_active(&region).is_none() {
        return;
    }
    let (Ok(pt), Some(nb)) = (player.get_single(), soc.active_body) else {
        return;
    };
    let (px, pz) = (pt.translation.x, -pt.translation.y);

    // Lejos → registro. Se guarda el genoma evolucionado igual que al cambiar de cuerpo:
    // si no, virtualizar borraría lo aprendido (F6.2).
    let out2 = VIRT_OUT_R * VIRT_OUT_R;
    for (e, n) in live.iter() {
        if (n.x - px).powi(2) + (n.z - pz).powi(2) <= out2 {
            continue;
        }
        soc.records.push(record_from(n, nb));
        soc.free_ids.push(n.brain);
        soc.alive = soc.alive.saturating_sub(1);
        commands.entity(e).despawn_recursive();
        soc.dirty = true;
    }

    // Cerca → entidad otra vez. Un solo `retain` saca a todos los que vuelven en una
    // pasada, en vez de un `remove` por vecino (que movería el vector entero cada vez).
    let in2 = VIRT_IN_R * VIRT_IN_R;
    let mut vuelven: Vec<NpcRecord> = Vec::new();
    soc.records.retain(|r| {
        if r.body != nb || (r.x - px).powi(2) + (r.z - pz).powi(2) > in2 {
            return true;
        }
        vuelven.push(r.clone());
        false
    });
    for rec in vuelven {
        let Some(brain) = soc.free_ids.pop() else {
            // Sin cerebro libre: se queda de registro y lo reintenta el próximo tick.
            soc.records.push(rec);
            continue;
        };
        soc.alive += 1;
        spawn_npc(&mut commands, &art, &rec, brain);
        soc.dirty = true;
    }
}

/// ¿Ya sembramos el asentamiento centrado en `(x, z)` de `body`? Se compara con holgura
/// de un tile: el centro sale siempre del mismo cálculo determinista.
fn already_seeded(soc: &Society, body: u8, x: f32, z: f32) -> bool {
    soc.centers
        .iter()
        .any(|&(b, cx, cz)| b == body && (cx - x).abs() < 1.0 && (cz - z).abs() < 1.0)
}

/// DESCUBRIMIENTO: si el jugador entró en un asentamiento de `geography` que todavía no
/// tiene gente, se funda ahí — en SU sitio, no en el tuyo. Es lo que reemplazó al "pueblo
/// inicial de 9 aldeanos alrededor del jugador": ahora hay que llegar caminando (o
/// volando) hasta un pueblo para conocer a alguien.
#[allow(clippy::too_many_arguments)]
fn discover_settlements(
    time: Res<Time>,
    mut commands: Commands,
    art: Res<crate::art2d::ArtLibrary>,
    region: Res<RegionRuntime>,
    ctx: Res<WorldContextRuntime>,
    planet: Res<ActivePlanet>,
    tiles: Res<TileWorld>,
    mut soc: ResMut<Society>,
    player: Query<&Transform, With<Player>>,
    mut live: Query<&mut Npc>,
    mut acc: Local<f32>,
) {
    *acc += time.delta_secs();
    if *acc < DISCOVER_EVERY {
        return;
    }
    *acc = 0.0;
    if !soc.loaded || surface_region_active(&region).is_none() {
        return;
    }
    let Ok(pt) = player.get_single() else {
        return;
    };
    let body = active_surface_body(&ctx, &planet);
    let nb = body as u8;
    if soc.active_body != Some(nb) || soc.alive >= MAX_NPCS {
        return;
    }
    let (ptx, ptz) = (pt.translation.x.floor() as i64, (-pt.translation.y).floor() as i64);
    let Some(s) = ha_shared::geography::settlement_covering(body, ptx, ptz) else {
        return; // fuera de todo pueblo: el mundo salvaje está vacío de gente, y así debe ser
    };
    let (cx, cz) = (s.center.0 as f32, s.center.1 as f32);
    // Las casas de la ciudad se calculan en un hilo aparte (barrer lotes cuesta decenas de ms);
    // hasta que vuelvan no se siembra ni se migra a nadie: unos segundos, sin tirón.
    let key = (nb, s.center.0, s.center.1);
    if !soc.barrios.contains_key(&key) {
        if let Some(tarea) = soc.barrios_en_curso.get_mut(&key) {
            let Some(b) = bevy::tasks::block_on(bevy::tasks::futures_lite::future::poll_once(tarea)) else {
                return;
            };
            soc.barrios_en_curso.remove(&key);
            soc.barrios.insert(key, b);
        } else {
            let pool = bevy::tasks::AsyncComputeTaskPool::get_or_init(bevy::tasks::TaskPool::default);
            let hogares_n = (tier_population(s.tier) as usize).div_ceil(FAMILY_SIZE as usize) + 2;
            soc.barrios_en_curso.insert(key, pool.spawn(async move { barrio_de(body, s, hogares_n) }));
            return;
        }
    }
    // Sólo se usan las casas cuyo escalón se puede pisar: una puerta clavada contra un muro o
    // un árbol dejaría a la familia entera encerrada afuera.
    let pisable = |h: &Huella| {
        let (ex, ez) = h.escalon();
        !tiles.walk_blocked(body, ex.floor() as i64, ez.floor() as i64)
    };
    let barrio = {
        let b = &soc.barrios[&key];
        Barrio {
            casas: b.casas.iter().copied().filter(|h| pisable(h)).collect(),
            puestos: b.puestos.iter().copied().filter(|h| pisable(h)).collect(),
            parque: b.parque,
        }
    };
    asignar_viviendas(&mut live, &barrio, (cx, cz));
    if already_seeded(&soc, nb, cx, cz) {
        return;
    }
    // B3/S4: la disposición cultural del planeta se combina con la memoria del CUERPO —
    // llegar a un pueblo nuevo del planeta donde masacraste al anterior no es empezar de cero.
    let disp = (planet_temperament_disposition(body) + soc.body_rep(nb)).clamp(-1.0, 1.0);
    let room = (MAX_NPCS - soc.alive) as u32;
    let count = tier_population(s.tier).min(room);
    if count == 0 {
        return;
    }
    let recs = seed_society(nb, count, None, disp, planet_temperament_disposition(body), (cx, cz), Some(&barrio), &mut soc);
    soc.centers.push((nb, cx, cz));
    let name = s.name.unwrap_or(match s.tier {
        ha_shared::geography::SettlementTier::City => "ciudad",
        ha_shared::geography::SettlementTier::Town => "pueblo",
        ha_shared::geography::SettlementTier::Hamlet => "aldea",
    });
    eprintln!("ha-society: asentamiento '{name}' poblado en ({cx}, {cz}) con {count} vecinos");
    for rec in recs {
        let Some(brain) = soc.free_ids.pop() else { break };
        soc.alive += 1;
        spawn_npc(&mut commands, &art, &rec, brain);
    }
    soc.dirty = true;
}

/// Los vecinos de este pueblo que todavía no tienen casa (partidas viejas, o nacidos antes de
/// que hubiera barrio) la reciben: una por familia, un puesto por cabeza.
fn asignar_viviendas(live: &mut Query<&mut Npc>, barrio: &Barrio, centro: (f32, f32)) {
    if barrio.casas.is_empty() {
        return;
    }
    for mut n in live.iter_mut() {
        if n.vivienda.casa.is_some() || (n.town_x - centro.0).abs() > 1.0 || (n.town_z - centro.1).abs() > 1.0 {
            continue;
        }
        let casa = barrio.casas[n.family as usize % barrio.casas.len()];
        let puesto = (!barrio.puestos.is_empty()).then(|| barrio.puestos[n.uid as usize % barrio.puestos.len()]);
        n.vivienda = Vivienda { casa: Some(casa), puesto, parque: barrio.parque };
        let e = casa.escalon();
        n.home_x = e.0;
        n.home_z = e.1;
    }
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

/// **El humor de la colonia** (`colony_health`, 0..1) empuja el ánimo de cada vecino en cada
/// tick de economía: con la despensa vacía la gente se siente insegura y agobiada; con la
/// despensa llena, tranquila. No manda en nadie — sólo mueve motivos, y son los motivos los
/// que deciden qué hace cada uno.
fn aplicar_humor(v: &mut Vida, humor: f32) {
    let delta = if humor < 0.35 {
        -0.08 * (0.35 - humor) / 0.35
    } else if humor > 0.8 {
        0.03 * (humor - 0.8) / 0.2
    } else {
        0.0
    };
    if delta != 0.0 {
        for m in [Motivo::Seguridad, Motivo::Ocio] {
            v.m.set(m, (v.m.get(m) + delta).clamp(0.0, 1.0));
        }
    }
}

/// Cada cuánto (s) un constructor mira si puede colocar otra pieza.
const OBRA_CADA_S: f32 = 8.0;
/// Tope de piezas por planeta: el mundo no se llena de mobiliario sin fin.
const OBRAS_MAX: u16 = 24;
/// Lo que un constructor sabe hacer: mobiliario de parque, por id del catálogo de colocables.
const OBRAS_ID: [&str; 4] = ["calle_banco", "calle_mesa_picnic", "calle_barril", "calle_farol"];

/// **El Constructor construye de verdad.** Cuando el pueblo juntó madera y hay un Constructor
/// despierto en la zona, gasta [`BUILD_WOOD_COST`] y coloca una pieza real (`BuiltStructures`,
/// la misma que coloca el jugador: colisiona, persiste y se dibuja) alrededor del parque de su
/// pueblo, en una posición libre. El sitio sale de una espiral por número de obra: determinista.
#[allow(clippy::too_many_arguments)]
fn constructor_obra(
    time: Res<Time>,
    ctx: Res<WorldContextRuntime>,
    planet: Res<ActivePlanet>,
    obs: crate::obstaculos::Obstaculos,
    mut soc: ResMut<Society>,
    npcs: Query<&Npc>,
    mut commands: Commands,
    mut acc: Local<f32>,
) {
    *acc += time.delta_secs();
    if *acc < OBRA_CADA_S {
        return;
    }
    *acc = 0.0;
    let body = active_surface_body(&ctx, &planet);
    let body_u8 = body as u8;
    let mut stock = soc.stock(body_u8);
    if stock.wood < BUILD_WOOD_COST || stock.obras >= OBRAS_MAX || obs.capa() != 0 {
        return;
    }
    let Some(constructor) = npcs.iter().find(|n| n.role == Role::Builder && n.vida.plan.map_or(true, |p| p.act != Actividad::Dormir)) else {
        return;
    };
    let centro = constructor.vivienda.parque.unwrap_or((constructor.town_x + 3.0, constructor.town_z + 4.0));
    let k = stock.obras as u32 + constructor.uid as u32;
    let id = OBRAS_ID[k as usize % OBRAS_ID.len()];
    let Some(a) = ha_shared::topdown::LARGE_ASSETS.iter().find(|a| a.id == id) else {
        return;
    };
    // Espiral alrededor del centro: se prueban 16 sitios y se toma el primero libre.
    for i in 0..16u32 {
        let ang = (k + i) as f32 * 2.399_963_2;
        let r = 2.5 + ((k + i) % 5) as f32 * 1.6;
        let min = ((centro.0 + ang.cos() * r).floor() as i64, (centro.1 + ang.sin() * r).floor() as i64);
        let celdas = || (0..a.w).flat_map(|dx| (0..a.h).map(move |dz| (min.0 + dx, min.1 + dz)));
        if celdas().any(|(tx, tz)| obs.solido(body, tx, tz)) {
            continue;
        }
        let (mx, mz) = (min.0 as f32 + a.w as f32 * 0.5, min.1 as f32 + a.h as f32 * 0.5);
        if !obs.libre(body, mx, mz, 0.4) {
            continue;
        }
        let ya = obs.estructuras().list.iter().any(|s| s.body == body_u8 && s.z == 0 && (s.cx - min.0).abs() < 3 && (s.cz - min.1).abs() < 3);
        if ya {
            continue;
        }
        let pieza = crate::structures::PlacedStructure {
            body: body_u8,
            z: 0,
            cx: min.0,
            cz: min.1,
            w: a.w,
            h: a.h,
            art: a.art.to_string(),
            solid: Vec::new(),
            rot: 0,
        };
        commands.queue(move |w: &mut World| w.resource_mut::<crate::structures::BuiltStructures>().add(pieza));
        stock.wood -= BUILD_WOOD_COST;
        stock.obras += 1;
        soc.stock.insert(body_u8, stock);
        return;
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
    tendency: f32,
    origin: (f32, f32),
    barrio: Option<&Barrio>,
    soc: &mut Society,
) -> Vec<NpcRecord> {
    let barrio = barrio.filter(|b| !b.casas.is_empty());
    // La primera despensa: una colonia nueva llega con víveres para no arrancar en hambruna.
    soc.add_stock(body, count as f32 * RATION * 8.0, 0.0, 0.0);
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
        let mut wx = bx + ang.cos() * rad;
        let mut wz = bz + 1.0 + ang.sin() * rad;
        // Con barrio: cada familia nace en el escalón de SU casa, una casa por familia.
        let mut vivienda = Vivienda::default();
        if let Some(b) = barrio {
            let casa = b.casas[(i / FAMILY_SIZE) as usize % b.casas.len()];
            let e = casa.escalon();
            wx = e.0 + ((i % FAMILY_SIZE) as f32 - 1.0) * 0.35;
            wz = e.1;
            let puesto = (!b.puestos.is_empty()).then(|| b.puestos[i as usize % b.puestos.len()]);
            vivienda = Vivienda { casa: Some(casa), puesto, parque: b.parque };
        }
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
            pareja: 0,
            padre: 0,
            madre: 0,
            faith: 0.0,
            converted: false,
            home_x: Some(wx),
            home_z: Some(wz),
            // La plaza es el centro de SU asentamiento (o su hábitat, en la Luna): a eso
            // vuelve a mediodía, aunque en el cuerpo haya media docena de pueblos más.
            town_x: Some(bx),
            town_z: Some(bz),
            history: Vec::new(),
            health: 1.0,
            role: Role::Wanderer,
            vida: Some(Vida::nueva_con(uid, tendency)),
            vivienda,
        });
    }
    out
}

// --- Conducta (sesgada por la reputación) ------------------------------------

/// Tinte suave por secta: se reconoce el corro a simple vista sin tapar la skin.
/// Un matiz propio de cada vecino (hasta −12 % por canal, estable por uid): con 20 hojas de aldeano,
/// esto evita que dos vecinos al lado se vean idénticos sin gastar una textura más.
pub(crate) fn tinte_individual(uid: u64) -> Color {
    let h = ha_shared::sims::hash(uid ^ 0x71E7_0001);
    let c = |k: u32| 1.0 - 0.12 * (((h >> (k * 8)) & 0xFF) as f32 / 255.0);
    Color::srgb(c(0), c(1), c(2))
}

fn tinte_credo(id: u16) -> Color {
    match id % 6 {
        0 => Color::srgb(0.80, 0.90, 1.00),
        1 => Color::srgb(0.85, 1.00, 0.82),
        2 => Color::srgb(1.00, 0.86, 0.92),
        3 => Color::srgb(0.90, 0.85, 1.00),
        4 => Color::srgb(1.00, 0.95, 0.80),
        _ => Color::srgb(0.82, 0.98, 0.98),
    }
}

/// La frase de un aldeano: lo que le falta manda; después su fe; después el saludo de siempre.
#[allow(clippy::too_many_arguments)]
fn frase_de(n: &Npc, soc: &Society, religion: &Religion, player_name: &str, rep: f32) -> String {
    let variante = n.uid.wrapping_add(n.age as u64);
    if let Some(q) = n.vida.queja(Clase::Aldeano) {
        if variante % 3 != 0 {
            return q.to_string();
        }
    }
    if n.vida.mala_fama > 0.6 && variante % 4 == 0 {
        return ["No mires.", "Yo no fui.", "Cada uno se rebusca."][(variante / 4) as usize % 3].to_string();
    }
    if n.vida.credo != 0 && n.vida.credo != CREDO_KI && variante % 2 == 0 {
        if let Some(c) = soc.credos.get(n.vida.credo) {
            return match variante / 2 % 3 {
                0 => format!("Gloria {}.", c.nombre),
                1 => format!("Somos {} miembros de {}.", c.miembros, c.nombre),
                _ => format!("Seguimos a {}.", c.nombre),
            };
        }
    }
    npc_line(rep, n.converted, n.faith, religion.founded, &religion.name, player_name, variante)
}

#[allow(clippy::type_complexity, clippy::too_many_arguments)]
fn tick_society(
    time: Res<Time>,
    ctx: Res<WorldContextRuntime>,
    planet: Res<ActivePlanet>,
    obs: crate::obstaculos::Obstaculos,
    mut pedidos: ResMut<crate::puertas::PedidosDePuerta>,
    mut commands: Commands,
    art: Res<crate::art2d::ArtLibrary>,
    mut soc: ResMut<Society>,
    mut religion: ResMut<Religion>,
    day: Res<crate::sky2d::DayClock>,
    mut speech: ResMut<crate::dialog::SpeechQueue>,
    player: Query<&Transform, (With<Player>, Without<Npc>)>,
    mut npcs: Query<(Entity, &mut Npc, &mut Transform, &mut Sprite, &mut Visibility, &mut NpcPose)>,
    selected: Res<crate::game_phase::SelectedCharacter>,
    mut acc: Local<f32>,
    mut ads: Local<Vec<sims::Anuncio>>,
) {
    let _probe = crate::debug::probe("tick_society");
    // El nombre que elegiste en la creacion: los aldeanos te saludan con el.
    let player_name = selected.name.clone();
    const AI_HZ: f32 = 15.0;
    *acc += time.delta_secs();
    if *acc < 1.0 / AI_HZ {
        return;
    }
    let dt = (*acc).min(0.2);
    *acc = 0.0;
    let body = active_surface_body(&ctx, &planet);
    let body_u8 = body as u8;
    let has_player = player.get_single().is_ok();
    let (px, pz) = player
        .get_single()
        .map(|t| (t.translation.x, -t.translation.y))
        .unwrap_or((0.0, 0.0));
    let fase = day.phase();
    let comida = soc.stock(body_u8).food > FOOD_MIN_STOCK;
    // Foto de los vecinos de este tick: es lo que cada uno ve de los demás.
    let mut vecinos: Vec<Vecino> = Vec::with_capacity(MAX_NPCS);
    let mut ents: Vec<(u64, Entity)> = Vec::with_capacity(MAX_NPCS);
    // Lo que hace falta de cada uno para saber si una pareja puede tener un hijo.
    let mut padres_posibles: HashMap<u64, (f32, f32, Vida, f32, bool)> = HashMap::with_capacity(MAX_NPCS);
    for (e, n, ..) in npcs.iter() {
        vecinos.push(Vecino::de(n.uid, n.x, n.z, &n.vida, n.health));
        ents.push((n.uid, e));
        padres_posibles.insert(n.uid, (n.x, n.z, n.vida, n.age, n.padre != 0 || n.madre != 0));
    }
    // Los que ya creen en el Ki predican a sus vecinos: la fe del jugador crece de boca en
    // boca dentro del pueblo, sin que el jugador esté presente.
    let preachers: Vec<(f32, f32)> = npcs
        .iter()
        .filter(|(_, n, ..)| n.converted)
        .map(|(_, n, ..)| (n.x, n.z))
        .collect();

    let mut followers = 0u32;
    // Tratos que se cerraron este tick: (actor, actividad, otro).
    let mut eventos: Vec<(u64, Actividad, u64)> = Vec::new();
    // x, z, familia, plaza del pueblo, la cabeza de la madre, la del padre, uids (madre, padre), vivienda.
    let mut births: Vec<(f32, f32, u32, (f32, f32), Vida, Vida, (u64, u64), Vivienda)> = Vec::new();

    // ¿Está el jugador fuera de la superficie? Entonces no ve al pueblo: arriba caminarían
    // por encima de tu losa, y abajo sobre la tierra maciza de la galería. La vida sigue
    // corriendo en barato, así que el pueblo no se congela por bajar a la mina.
    let off_layer = obs.capa() != 0;
    for (e, mut n, mut tf, mut sprite, mut vis, mut pose) in &mut npcs {
        let n = &mut *n;
        let dr2 = (n.x - px) * (n.x - px) + (n.z - pz) * (n.z - pz);
        n.vida.mala_fama = n.vida.mala_fama.clamp(0.0, 1.0);
        if dr2 > RENDER_R * RENDER_R || off_layer {
            if *vis != Visibility::Hidden {
                *vis = Visibility::Hidden;
            }
            n.age += dt;
            let sitio = sitio_de((n.home_x, n.home_z), (n.town_x, n.town_z), n.uid, n.family, &n.vivienda);
            sociedad::vivir_credo(&mut n.vida, &soc.credos, dt);
            let viv = n.vivienda;
            let (nx, nz) = avanzar_lejos(&mut n.vida, (n.x, n.z), &sitio, &viv, fase, comida, dt);
            n.ruta.clear();
            n.pide_ruta = false;
            n.x = nx;
            n.z = nz;
            if n.converted {
                followers += 1;
            }
            continue;
        }
        // Adentro de su casa o de su puesto sólo se lo ve si vos también estás adentro: la casa
        // cerrada no deja ver lo que pasa en el interior.
        let (hs, nh) = n.vivienda.huellas();
        let hs = &hs[..nh];
        let oculto = hs.iter().any(|h| h.dentro(n.x, n.z) && !h.dentro(px, pz));
        let quiero = if oculto { Visibility::Hidden } else { Visibility::Visible };
        if *vis != quiero {
            *vis = quiero;
        }
        // Los NPCs NO se esfuman de viejos: sólo mueren por una causa VISIBLE (el jugador, una
        // pelea del pueblo, el hambre de verdad). La edad se cuenta por si luego querés ancianos.
        n.age += dt;
        n.calm_cd = (n.calm_cd - dt).max(0.0);
        n.convert_cd = (n.convert_cd - dt).max(0.0);
        n.bump = (n.bump - dt).max(0.0);

        use std::f32::consts::{PI, TAU};
        let rep = soc.rep(n.family);

        // --- FE en el Ki + conversión (religión del jugador) ---
        // La fe se contagia entre VECINOS, no por estar cerca del jugador. Vos fundás predicando
        // (menú del NPC → PREDICAR) y a partir del primer creyente el culto se propaga solo.
        if !n.converted {
            let preached = religion.founded
                && rep > AGGRO_THRESHOLD
                && preachers.iter().any(|&(fx, fz)| {
                    (n.x - fx) * (n.x - fx) + (n.z - fz) * (n.z - fz) < PREACH_RADIUS * PREACH_RADIUS
                });
            if preached {
                n.faith = (n.faith + FAITH_SPREAD * dt).min(1.0);
                if n.faith >= CONVERT_AT {
                    n.converted = true;
                    crate::dialog::say_from(
                        &mut speech,
                        e,
                        n.x,
                        n.z,
                        format!("¡Creo en {}!", religion.name),
                        Color::srgb(1.0, 0.85, 0.3),
                        false,
                    );
                }
            } else {
                n.faith = (n.faith - FAITH_COOL * dt).max(0.0);
            }
        }
        // El Ki es un credo más para la sociedad: el converso reza en su santuario y su moral
        // se acerca al dogma; y si la sociedad lo convirtió a él, figura como fiel.
        if n.converted && n.vida.credo != CREDO_KI {
            n.vida.credo = CREDO_KI;
            n.vida.m.set(Motivo::Fe, 1.0);
        } else if n.vida.credo == CREDO_KI && !n.converted {
            n.converted = true;
        }
        if n.converted {
            followers += 1;
        }

        // --- LA CABEZA: barras, carácter, y elegir entre lo que se ofrece ---
        n.vida.tick(Clase::Aldeano, fase, dt, (n.x, n.z));
        sociedad::vivir_credo(&mut n.vida, &soc.credos, dt);
        let objetivo = n.vida.objetivo();
        if objetivo != 0 {
            match vecinos.iter().find(|v| v.uid == objetivo) {
                Some(v) => n.vida.apuntar(v.x, v.z),
                None => n.vida.abandonar(),
            }
        }
        if n.vida.necesita_decidir(Clase::Aldeano, fase) {
            ads.clear();
            let sitio = sitio_de((n.home_x, n.home_z), (n.town_x, n.town_z), n.uid, n.family, &n.vivienda);
            sociedad::anuncios_sitio(&sitio, fase, comida, n.vida.credo != 0, &mut ads);
            sociedad::anuncios_vecinos(&n.vida, n.uid, (n.x, n.z), &vecinos, &soc.credos, &mut ads);
            n.vida.decidir(Clase::Aldeano, (n.x, n.z), fase, &ads);
        }
        if let Some((act, otro)) = n.vida.por_resolver() {
            eventos.push((n.uid, act, otro));
        }
        let act = n.vida.actividad();

        // --- MOVIMIENTO: hacia donde apunta el plan; el trato con el jugador manda ---
        let toward_p = (pz - n.z).atan2(px - n.x);
        // Ojo con lo que NO está acá: no hay rama de "fiel". Lo único que camina hacia el
        // jugador es quien te ODIA, y eso es una consecuencia de matarle a la familia.
        let aggro = has_player && rep <= AGGRO_THRESHOLD && dr2 < 24.0 * 24.0;
        let flee = has_player && rep < -0.15 && rep > AGGRO_THRESHOLD && dr2 < 20.0 * 20.0;
        let (goal, gain, speed): (Option<f32>, f32, f32) = if aggro {
            (Some(toward_p), 1.0, NPC_VEL * 1.9) // te encara
        } else if flee {
            (Some(toward_p + PI), 0.9, NPC_VEL * 1.6) // huye
        } else {
            match n.vida.destino() {
                Some((tx, tz, false)) => {
                    let v = match act {
                        Actividad::Huir | Actividad::Atacar | Actividad::Detener | Actividad::Robar => 1.25,
                        _ => 0.9,
                    };
                    (Some((tz - n.z).atan2(tx - n.x)), 0.9, NPC_VEL * v)
                }
                Some((_, _, true)) => match act {
                    // Trabajando, juntándose o jugando se mueve un poco en el lugar.
                    Actividad::Trabajar | Actividad::Reunirse | Actividad::Jugar => {
                        let drift = (n.uid as f32 * 1.7 + n.age * 0.25).sin() * 0.3;
                        (Some(n.heading + drift), 0.15, NPC_VEL * 0.3)
                    }
                    // Todo lo demás se hace quieto: duerme, come, bebe, reza, charla, se pelea.
                    _ => (None, 0.0, 0.0),
                },
                None => {
                    // Sin plan concreto: deambula cerca de casa, sin círculos.
                    let hd2 = (n.x - n.home_x).powi(2) + (n.z - n.home_z).powi(2);
                    if hd2 > HOME_WANDER_R * HOME_WANDER_R {
                        (Some((n.home_z - n.z).atan2(n.home_x - n.x)), 0.9, NPC_VEL * 0.6)
                    } else {
                        let drift = (n.uid as f32 * 1.7 + n.age * 0.25).sin() * 0.35;
                        (Some(n.heading + drift), 0.12, NPC_VEL * 0.35)
                    }
                }
            }
        };
        // Un plan con destino camina por la calle de verdad: por la puerta de su edificio, rodeando
        // los ajenos. Todo lo demás (deambular, encarar, huir) sigue el rumbo con giro suave.
        let destino = if aggro || flee {
            None
        } else {
            match n.vida.destino() {
                Some((tx, tz, false)) => Some((tx, tz)),
                _ => None,
            }
        };
        let mut rumbo_directo = None;
        if let Some((tx, tz)) = destino {
            let mut wp = hogares::siguiente_punto((n.x, n.z), (tx, tz), hs);
            if !n.ruta.is_empty() {
                let (mx, mz) = (n.ruta_meta.0 - tx, n.ruta_meta.1 - tz);
                if mx * mx + mz * mz > 4.0 {
                    n.ruta.clear();
                } else {
                    while n.ruta.first().is_some_and(|p| (p.0 - n.x).powi(2) + (p.1 - n.z).powi(2) < 0.2) {
                        n.ruta.remove(0);
                    }
                    if let Some(&p) = n.ruta.first() {
                        wp = p;
                    }
                }
            }
            // A punto de cruzar una puerta propia: se la pide antes de llegar, así no se frena.
            for h in hs {
                let u = h.umbral();
                if (u.0 - n.x).powi(2) + (u.1 - n.z).powi(2) < 2.6 && h.dentro(n.x, n.z) != h.dentro(tx, tz) {
                    pedidos.0.push(u);
                }
            }
            rumbo_directo = Some(((wp.1 - n.z).atan2(wp.0 - n.x), wp));
        }
        if let Some((a, _)) = rumbo_directo {
            n.heading = a;
        } else if let Some(g) = goal {
            let diff = (g - n.heading + PI).rem_euclid(TAU) - PI;
            n.heading += diff.clamp(-1.0, 1.0) * NPC_TURN * dt * gain.max(0.5);
        }
        let mut se_movio = false;
        if speed > 0.0 && !n.pide_ruta {
            let etapa = linaje::etapa(n.age, n.padre != 0 || n.madre != 0);
            let paso = speed * dt * if etapa == Etapa::Anciano { 0.7 } else { 1.0 };
            let (nx, nz) = (n.x + n.heading.cos() * paso, n.z + n.heading.sin() * paso);
            let libre = |x: f32, z: f32| obs.libre(body, x, z, NPC_RADIO);
            if libre(nx, nz) {
                (n.x, n.z, se_movio) = (nx, nz, true);
                n.atasco = 0;
            } else if libre(nx, n.z) && (nx - n.x).abs() > 1e-4 {
                (n.x, se_movio) = (nx, true);
            } else if libre(n.x, nz) && (nz - n.z).abs() > 1e-4 {
                (n.z, se_movio) = (nz, true);
            } else if obs.puerta_cerrada_en(nx, nz, NPC_RADIO) {
                // Una puerta cerrada no es un muro: se la pide y se espera a que abra.
                pedidos.0.push((nx, nz));
            } else if let Some((tx, tz)) = destino {
                // Un edificio ajeno o un obstáculo en la calle: que el buscador arme un camino.
                n.ruta.clear();
                n.pide_ruta = true;
                n.ruta_meta = (tx, tz);
            } else {
                n.heading += 1.3;
                n.atasco = n.atasco.saturating_add(1);
                if n.bump <= 0.0 {
                    n.vida.trabar();
                    n.bump = 2.5;
                }
            }
        }
        if n.atasco >= 45 {
            // Se lo saca del apuro: el punto libre más cercano, o el escalón de su casa.
            n.atasco = 0;
            n.ruta.clear();
            let salida = (1..=8).flat_map(|r| (0..8).map(move |k| (r as f32, k as f32 * TAU / 8.0))).find_map(|(r, a)| {
                let (x, z) = (n.x + a.cos() * r, n.z + a.sin() * r);
                obs.libre(body, x, z, NPC_RADIO).then_some((x, z))
            });
            if let Some((x, z)) = salida.or_else(|| n.vivienda.casa.map(|c| c.escalon())) {
                (n.x, n.z) = (x, z);
            }
        }
        n.energy = (n.energy + dt * 0.006).clamp(0.0, 1.0);

        // Reproducción: la tiene la MUJER adulta que está con su PAREJA (los dos adultos, cerca).
        // La cría cruza el carácter de los dos, hereda la familia de la madre y su credo. Hace
        // falta estar bien de ánimo Y que la colonia tenga con qué alimentar a alguien más (B4).
        let nacido = n.padre != 0 || n.madre != 0;
        if linaje::es_mujer(n.uid)
            && n.pareja != 0
            && linaje::etapa(n.age, nacido) == Etapa::Adulto
            && n.energy >= NPC_REPRO_ENERGY
            && n.vida.m.bienestar(Clase::Aldeano) > 0.45
            && soc.alive + births.len() < MAX_NPCS
            && soc.free_ids.len() > births.len()
            && soc.body_health(body_u8, soc.alive as u32) >= BIRTH_MIN_HEALTH
        {
            // Con el JUGADOR de pareja: el hijo nace si estás cerca; no hay edad que comprobar.
            let otro = if n.pareja == JUGADOR_UID {
                has_player.then(|| (px, pz, Vida::nueva(JUGADOR_UID), 0.0, false))
            } else {
                padres_posibles.get(&n.pareja).copied()
            };
            if let Some((px_, pz_, pv, page, pnacido)) = otro {
                let cerca = (px_ - n.x).powi(2) + (pz_ - n.z).powi(2) <= linaje::RADIO_PAREJA * linaje::RADIO_PAREJA;
                if cerca && linaje::etapa(page, pnacido) == Etapa::Adulto {
                    n.energy -= NPC_REPRO_COST;
                    births.push((n.x, n.z, n.family, (n.town_x, n.town_z), n.vida, pv, (n.uid, n.pareja), n.vivienda));
                }
            }
        }

        // --- CHAT ambiental por CONTEXTO ---
        n.say_cd -= dt;
        if n.say_cd <= 0.0 && dr2 < 30.0 * 30.0 && act != Actividad::Dormir && !oculto {
            n.say_cd = SAY_COOLDOWN + (n.uid % 5) as f32;
            let line = frase_de(n, &soc, &religion, &player_name, rep);
            let col = if n.converted {
                Color::srgb(1.0, 0.9, 0.4)
            } else if rep < 0.0 || n.vida.mala_fama > 0.5 {
                Color::srgb(1.0, 0.55, 0.45)
            } else {
                Color::srgb(0.92, 0.96, 1.0)
            };
            push_history(&mut n.history, &line);
            crate::dialog::say_from(&mut speech, e, n.x, n.z, line, col, false);
        }

        // Render: la SKIN se ve natural; un tinte SUAVE marca disposición (fiel → dorado,
        // secta → su color, malhechor conocido → rojizo, rencor → rojizo).
        let r = render_xy(n.x, n.z);
        // La edad se ve: el niño es más bajo (escala desde el centro → se baja para que los pies
        // queden donde estaban) y el anciano se nota en el color más abajo.
        let k = linaje::escala_por_edad(n.age, n.padre != 0 || n.madre != 0);
        if (tf.scale.x - k).abs() > 0.005 {
            tf.scale = Vec3::new(k, k, 1.0);
        }
        tf.translation = Vec3::new(r.x, r.y - NPC_SIZE.y * 0.5 * (1.0 - k), sort_z(n.z));
        // La pose sale de lo que hace: llegó al lugar → duerme / reza / pelea; si no, camina o respira.
        let llego = matches!(n.vida.destino(), Some((_, _, true)));
        let dir = crate::fauna::dir4(n.heading);
        let pedida = match (llego, act) {
            (true, Actividad::Dormir) => "sleep_down".to_string(),
            (true, Actividad::Rezar) => format!("sit_floor_{dir}"),
            (true, Actividad::Atacar) => format!("punch_{dir}"),
            (true, Actividad::Robar) => format!("pick_up_{dir}"),
            _ if se_movio => format!("walk_{dir}"),
            _ => format!("idle_{dir}"),
        };
        if pose.pedida != pedida {
            let clave = format!("npc/char_{}", (n.uid % NUM_SKINS) + 1);
            crate::fauna::apply_pose(&mut commands, e, &art, &clave, &pedida, &mut sprite, &mut pose.aplicada);
            pose.pedida = pedida;
        }
        sprite.color = if n.converted {
            Color::srgb(1.0, 0.94, 0.7)
        } else if n.vida.mala_fama > 0.5 {
            Color::srgb(1.0, 0.72, 0.68)
        } else if n.vida.credo != 0 {
            tinte_credo(n.vida.credo)
        } else if rep < 0.0 {
            Color::srgb(1.0, 0.7 + 0.3 * (1.0 + rep), 0.65 + 0.3 * (1.0 + rep))
        } else if linaje::etapa(n.age, n.padre != 0 || n.madre != 0) == Etapa::Anciano {
            Color::srgb(0.86, 0.86, 0.92)
        } else {
            tinte_individual(n.uid)
        };
    }
    religion.followers = followers;

    // --- CAMINOS: una búsqueda a la vez, unos cuantos nodos por tick (sin tirones) ---
    if soc.busqueda.is_none() {
        if let Some((_, n, ..)) = npcs.iter().find(|(_, n, ..)| n.pide_ruta) {
            let desde = (n.x, n.z);
            let (hs, nh) = n.vivienda.huellas();
            let meta = hogares::siguiente_punto(desde, n.ruta_meta, &hs[..nh]);
            soc.busqueda = Some((n.uid, ha_shared::ruta::Buscador::nuevo(desde, meta), desde, n.ruta_meta));
        }
    }
    if let Some((uid, mut b, desde, plan)) = soc.busqueda.take() {
        let mut libre = |tx: i64, tz: i64| obs.libre(body, tx as f32 + 0.5, tz as f32 + 0.5, NPC_RADIO);
        match b.paso(60, &mut libre) {
            ha_shared::ruta::Estado::Buscando => soc.busqueda = Some((uid, b, desde, plan)),
            fin => {
                let camino = b.camino(desde, &mut libre);
                if let Some((_, mut n, ..)) = npcs.iter_mut().find(|(_, n, ..)| n.uid == uid) {
                    n.pide_ruta = false;
                    n.ruta_meta = plan;
                    if fin == ha_shared::ruta::Estado::Encontrada && !camino.is_empty() {
                        n.ruta = camino;
                    } else {
                        // Sin camino: que se busque otra cosa que hacer.
                        n.vida.trabar();
                        n.bump = 2.5;
                        n.atasco = n.atasco.saturating_add(15);
                    }
                }
            }
        }
    }

    // --- TRATOS entre vecinos: robo, pelea, ayuda, prédica, detención ---
    let mut muertos: Vec<Entity> = Vec::new();
    for (auid, act, ouid) in eventos {
        let (Some(&(_, ea)), Some(&(_, eo))) =
            (ents.iter().find(|x| x.0 == auid), ents.iter().find(|x| x.0 == ouid))
        else {
            continue;
        };
        if ea == eo || muertos.contains(&ea) || muertos.contains(&eo) {
            continue;
        }
        let (Ok((_, na, ..)), Ok((_, nb, ..))) = (npcs.get(ea), npcs.get(eo)) else { continue };
        let (mut va, mut vb, apos, bpos) = (na.vida, nb.vida, (na.x, na.z), (nb.x, nb.z));
        // El trato sólo se cierra si siguen cerca (si el otro se fue, no hubo nada).
        if (apos.0 - bpos.0).powi(2) + (apos.1 - bpos.1).powi(2) > (sociedad::RADIO_TRATO * 1.5).powi(2) {
            continue;
        }
        let h = sims::hash(time.elapsed().as_micros() as u64 ^ auid.rotate_left(21) ^ ouid);
        let azar = (h >> 40) as f32 / (1u64 << 24) as f32;
        let r = sociedad::resolver(act, &mut va, auid, &mut vb, ouid, &mut soc.credos, body_u8, azar);
        // Contadores + frases.
        let mut dicho = |ent: Entity, x: f32, z: f32, txt: String, col: Color| {
            crate::dialog::say_from(&mut speech, ent, x, z, txt, col, true);
        };
        match act {
            Actividad::Robar => {
                soc.stats.hurtos += 1;
                dicho(eo, bpos.0, bpos.1, "Eh! Mi bolsa!".into(), Color::srgb(1.0, 0.6, 0.5));
            }
            Actividad::Atacar => {
                soc.stats.agresiones += 1;
                dicho(eo, bpos.0, bpos.1, "Ay!".into(), Color::srgb(1.0, 0.6, 0.5));
            }
            Actividad::Detener => {
                soc.stats.detenciones += 1;
                dicho(ea, apos.0, apos.1, "Alto ahi!".into(), Color::srgb(0.7, 0.85, 1.0));
            }
            Actividad::Ayudar => {
                soc.stats.ayudas += 1;
                dicho(ea, apos.0, apos.1, "Toma, comparte.".into(), Color::srgb(0.7, 1.0, 0.75));
            }
            _ => {}
        }
        if r.fundo != 0 {
            soc.stats.fundados += 1;
            let nombre = soc.credos.get(r.fundo).map(|c| c.nombre.clone()).unwrap_or_default();
            eprintln!("ha-society: NACE la secta '{nombre}' (fundador uid {auid})");
            dicho(ea, apos.0, apos.1, format!("Fundo {nombre}!"), Color::srgb(1.0, 0.9, 0.5));
        }
        if r.convirtio {
            soc.stats.conversiones += 1;
            let nombre = soc.credos.get(va.credo).map(|c| c.nombre.clone()).unwrap_or_else(|| religion.name.clone());
            dicho(eo, bpos.0, bpos.1, format!("¡Creo en {nombre}!"), Color::srgb(1.0, 0.9, 0.5));
        }
        // Escribir de vuelta las dos cabezas (+ daño) y avisar a los testigos.
        let mut murio = false;
        if let Ok((_, mut b, ..)) = npcs.get_mut(eo) {
            b.vida = vb;
            b.health -= r.dano;
            murio = b.health <= 0.0;
        }
        if murio {
            va.mala_fama = 1.0;
        }
        if let Ok((_, mut a, ..)) = npcs.get_mut(ea) {
            a.vida = va;
        }
        for (_, mut w, ..) in npcs.iter_mut() {
            let d2 = (w.x - apos.0).powi(2) + (w.z - apos.1).powi(2);
            if d2 <= sociedad::RADIO_TESTIGO * sociedad::RADIO_TESTIGO {
                let wuid = w.uid;
                sociedad::testigo(&mut w.vida, wuid, auid, ouid, &r);
            }
        }
        if murio {
            muertos.push(eo);
        }
    }
    for e in muertos {
        if let Ok((_, n, ..)) = npcs.get(e) {
            soc.herencias.push((n.uid, n.pareja, n.vivienda));
            soc.free_ids.push(n.brain);
            eprintln!("ha-society: {} murio en una pelea del pueblo", npc_name(n.uid, n.family));
        }
        commands.entity(e).despawn_recursive();
        soc.alive = soc.alive.saturating_sub(1);
        soc.stats.muertes += 1;
        soc.dirty = true;
    }

    for (x, z, family, town, mvida, pvida, (madre, padre), vivienda) in births {
        let Some(child) = soc.free_ids.pop() else { break };
        let uid = soc.next_uid;
        soc.next_uid = soc.next_uid.wrapping_add(1);
        soc.alive += 1;
        let (ox, oz) = ((child as f32).sin() * 0.8, (child as f32).cos() * 0.8);
        let rec = NpcRecord {
            uid,
            family,
            body: soc.active_body.unwrap_or(0),
            x: x + ox,
            z: z + oz,
            heading: 0.0,
            energy: 0.5,
            age: 0.0,
            pareja: 0,
            padre,
            madre,
            faith: 0.0,
            converted: false,
            home_x: Some(x + ox),
            home_z: Some(z + oz),
            // El recién nacido hereda el PUEBLO de su padre: la plaza a la que irá es la de su familia.
            town_x: Some(town.0),
            town_z: Some(town.1),
            history: Vec::new(),
            health: 1.0,
            role: Role::Wanderer, // S6: el recién nacido elige oficio en el próximo tick
            vida: Some(Vida::hija_de_pareja(&mvida, &pvida, uid)),
            // La cría vive en la casa de la familia y trabaja donde le toque en el pueblo.
            vivienda,
        };
        soc.stats.nacimientos += 1;
        eprintln!(
            "ha-society: nacio {} (hija/o de {} y {})",
            npc_name(uid, family),
            npc_name(madre, family),
            npc_name(padre, family)
        );
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
    // Censo de credos sobre TODOS (vivos + registros): una secta sin nadie se disuelve, y si
    // muere su líder sube el más carismático. El Ki (id propio) no se censa: es del jugador.
    {
        let mut cuenta: std::collections::HashMap<u16, u16> = std::collections::HashMap::new();
        let mut miembros: std::collections::HashMap<u16, Vec<(u64, f32)>> = std::collections::HashMap::new();
        let mut ver = |uid: u64, v: &Vida| {
            if v.credo != 0 && v.credo != CREDO_KI {
                *cuenta.entry(v.credo).or_default() += 1;
                miembros.entry(v.credo).or_default().push((uid, v.pers.carisma));
            }
        };
        for n in live.iter() {
            ver(n.uid, &n.vida);
        }
        for r in soc.records.iter() {
            if let Some(v) = &r.vida {
                ver(r.uid, v);
            }
        }
        let muertos = soc.credos.censar(|id| cuenta.get(&id).copied().unwrap_or(0));
        for id in muertos {
            eprintln!("ha-society: la secta {id} se disolvio");
        }
        for (id, m) in &miembros {
            soc.credos.sucesion(*id, m);
        }
    }
    let mut bodies: std::collections::HashSet<u8> = soc.records.iter().map(|r| r.body).collect();
    if let Some(b) = soc.active_body {
        bodies.insert(b);
    }
    for body in bodies {
        if Some(body) == soc.active_body {
            // Los niños no trabajan: quedan fuera del reparto de oficios (pero comen, y cuentan
            // para el humor de la colonia).
            let mut roles: Vec<Role> = live.iter().filter(|n| !es_nino(n.age, n.padre, n.madre)).map(|n| n.role).collect();
            if roles.is_empty() {
                continue;
            }
            let mut stock = soc.stock(body);
            colony_step(&mut stock, &mut roles);
            soc.stock.insert(body, stock);
            let humor = colony_health(stock, live.iter().count() as u32);
            let mut it = roles.into_iter();
            for mut n in live.iter_mut() {
                n.role = if es_nino(n.age, n.padre, n.madre) { Role::Wanderer } else { it.next().unwrap_or(n.role) };
                aplicar_humor(&mut n.vida, humor);
            }
        } else {
            let mut roles: Vec<Role> = soc
                .records
                .iter()
                .filter(|r| r.body == body && !es_nino(r.age, r.padre, r.madre))
                .map(|r| r.role)
                .collect();
            if roles.is_empty() {
                continue;
            }
            let mut stock = soc.stock(body);
            colony_step(&mut stock, &mut roles);
            soc.stock.insert(body, stock);
            let mut it = roles.into_iter();
            for r in soc.records.iter_mut().filter(|r| r.body == body) {
                if es_nino(r.age, r.padre, r.madre) {
                    r.role = Role::Wanderer;
                } else if let Some(nr) = it.next() {
                    r.role = nr;
                }
            }
        }
    }
}

/// Los pueblos de los OTROS cuerpos también cumplen su horario. Corre a 1 Hz sobre los
/// `NpcRecord` guardados (aritmética pura, sin entidades ni render): un pueblo de Marte
/// sigue yendo a trabajar mientras vos estás en la Tierra, y al aterrizar los encontrás
/// donde les toca — no clavados donde los dejaste hace una semana.
fn tick_society_remote(
    time: Res<Time>,
    day: Res<crate::sky2d::DayClock>,
    mut soc: ResMut<Society>,
    mut acc: Local<f32>,
) {
    *acc += time.delta_secs();
    if *acc < 1.0 {
        return;
    }
    let dt = *acc;
    *acc = 0.0;
    if !soc.loaded {
        return;
    }
    let fase = day.phase();
    // Antes se salteaba el cuerpo activo porque ahí TODOS estaban vivos. Con el streaming
    // (`stream_npcs`) el cuerpo activo también tiene registros — los de lejos —, y esos son
    // justamente los que necesitan seguir su rutina sin entidad. La regla ahora es la del
    // invariante: si está en `records` no tiene entidad, así que le toca `avanzar_lejos`.
    let soc = &mut *soc;
    for r in soc.records.iter_mut() {
        let home = (r.home_x.unwrap_or(r.x), r.home_z.unwrap_or(r.z));
        let town = (r.town_x.unwrap_or(home.0), r.town_z.unwrap_or(home.1));
        let sitio = sitio_de(home, town, r.uid, r.family, &r.vivienda);
        let comida = soc.stock.get(&r.body).map(|s| s.food > FOOD_MIN_STOCK).unwrap_or(false);
        let v = r.vida.get_or_insert_with(|| Vida::nueva(r.uid));
        sociedad::vivir_credo(v, &soc.credos, dt);
        let (nx, nz) = avanzar_lejos(v, (r.x, r.z), &sitio, &r.vivienda, fase, comida, dt);
        r.x = nx;
        r.z = nz;
        r.age += dt;
    }
}

// --- Evento de DAÑO (crea el rencor) -----------------------------------------

/// Un golpe de ATAQUE cerca del cursor daña al NPC más próximo (`ATTACK_DAMAGE`, dos
/// golpes para matar) — C1: sólo cuenta si el jugador está a distancia de combate real
/// (`PLAYER_REACH`), así usar cualquier habilidad lejos del pueblo ya no es un homicidio
/// accidental. Muerte → su familia lo recuerda (rencor fuerte) y los testigos (otras
/// familias) también. Es la semilla del "le maté al papá y esa familia me odia".
#[allow(clippy::too_many_arguments)]
/// Saca del mundo a un NPC que murió por decisión de la civilización (ejecución, vejez del
/// linaje, tragedia): mismo libro de cuentas que una muerte a golpes.
pub(crate) fn despedir_npc(commands: &mut Commands, soc: &mut Society, e: Entity, npc: &Npc) {
    soc.herencias.push((npc.uid, npc.pareja, npc.vivienda));
    commands.entity(e).despawn_recursive();
    soc.alive = soc.alive.saturating_sub(1);
    soc.free_ids.push(npc.brain);
    soc.dirty = true;
}

fn harm_events(
    mut commands: Commands,
    mut civ: ResMut<crate::civilizacion::Civ>,
    tap: Res<AbilityTapPulse>,
    cursor: Res<AimCursor>,
    mut soc: ResMut<Society>,
    mut npcs: Query<(Entity, &mut Npc)>,
    player: Query<&Transform, With<Player>>,
    mut speech: ResMut<crate::dialog::SpeechQueue>,
    mut cooldown: Local<f32>,
    time: Res<Time>,
    mut sfx: EventWriter<crate::audio::PlaySfxAt>,
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
    let brain_id = npc.brain;
    let herencia = (npc.uid, npc.pareja, npc.vivienda);
    drop(npc);
    sfx.send(crate::audio::PlaySfxAt(
        crate::audio::Sfx::Dig,
        crate::world2d::render_xy(vx, vz),
    ));
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
    civ.matado_por_el_jugador(uid, vx, vz);
    soc.herencias.push(herencia);
    commands.entity(e).despawn_recursive();
    soc.alive = soc.alive.saturating_sub(1);
    soc.free_ids.push(brain_id);
    // A1: NO guardar acá (no tenemos `live` a mano → escribiría el pueblo incompleto).
    // Un solo camino de escritura: pedir flush inmediato al próximo tick del guardado
    // periódico, que sí junta `live`.
    soc.dirty = true;
}

/// **Herencia de la casa.** Cuando muere alguien con casa, la toma su pareja o, si no hay, un
/// hijo ya adulto — sólo si ese heredero no tiene casa propia: nadie queda en la calle
/// mientras una vivienda del pueblo está vacía. Los hijos chicos siguen bajo el techo de
/// su madre (heredan la vivienda al nacer), así que a ellos no hace falta repartirles nada.
fn repartir_herencias(mut soc: ResMut<Society>, mut npcs: Query<&mut Npc>) {
    if soc.herencias.is_empty() {
        return;
    }
    for (uid, pareja, viv) in std::mem::take(&mut soc.herencias) {
        let Some(casa) = viv.casa else { continue };
        let mut heredero: Option<Mut<Npc>> = None;
        for n in npcs.iter_mut() {
            if n.uid == uid || n.vivienda.casa.is_some() {
                continue;
            }
            let es_pareja = pareja != 0 && n.uid == pareja;
            let es_hijo = (n.madre == uid || n.padre == uid) && !es_nino(n.age, n.padre, n.madre);
            let mejor = match &heredero {
                None => es_pareja || es_hijo,
                Some(h) => es_pareja && h.uid != pareja,
            };
            if mejor {
                heredero = Some(n);
            }
        }
        if let Some(mut h) = heredero {
            h.vivienda.casa = Some(casa);
            soc.stats.herencias += 1;
            soc.dirty = true;
            eprintln!("ha-society: {} hereda la casa de uid {uid}", npc_name(h.uid, h.family));
        }
    }
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
    let live: Vec<NpcRecord> = npcs.iter().map(|n| record_from(n, body)).collect();
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
    if crate::flags::var("HA_FORCE_CREATIVE").is_ok() {
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
            // Debajo del overlay del VIVARIO, que ahora lleva dos líneas (censo + ECO).
            top: Val::Px(46.0),
            left: Val::Px(8.0),
            ..default()
        },
        Visibility::Hidden,
    ));
}

/// Asentamiento más cercano al jugador (nombre + distancia) — la brújula de QA para
/// comprobar que la gente está donde dice la geografía y no alrededor tuyo. Barra las 3×3
/// celdas de sitio vecinas; se llama con throttle, nunca por frame.
fn nearest_settlement_line(body: CelestialBody, px: f32, pz: f32) -> String {
    use ha_shared::geography::{settlement_in, SettlementTier, SITE_PERIOD};
    let (sx, sz) = (
        (px as i64).div_euclid(SITE_PERIOD),
        (pz as i64).div_euclid(SITE_PERIOD),
    );
    let mut best: Option<(f32, &'static str, i64, i64)> = None;
    for dz in -1..=1 {
        for dx in -1..=1 {
            let Some(s) = settlement_in(body, sx + dx, sz + dz) else { continue };
            let (cx, cz) = (s.center.0 as f32, s.center.1 as f32);
            let d = ((cx - px) * (cx - px) + (cz - pz) * (cz - pz)).sqrt();
            let label = s.name.unwrap_or(match s.tier {
                SettlementTier::City => "CIUDAD",
                SettlementTier::Town => "PUEBLO",
                SettlementTier::Hamlet => "ALDEA",
            });
            if best.map_or(true, |(bd, ..)| d < bd) {
                best = Some((d, label, s.center.0, s.center.1));
            }
        }
    }
    match best {
        Some((d, label, cx, cz)) => format!("{label} en ({cx},{cz}) a {d:.0} m"),
        None => "ninguno en 2.6 km - sigue explorando".to_string(),
    }
}

#[allow(clippy::too_many_arguments)]
fn update_society_overlay(
    editor: Res<crate::ui_editor::UiEditorState>,
    soc: Res<Society>,
    day: Res<crate::sky2d::DayClock>,
    time: Res<Time>,
    region: Res<RegionRuntime>,
    ctx: Res<WorldContextRuntime>,
    planet: Res<ActivePlanet>,
    player: Query<&Transform, With<Player>>,
    npcs: Query<&Npc>,
    mut overlay: Query<(&mut Text, &mut Visibility), With<SocietyOverlay>>,
    mut acc: Local<f32>,
    mut nearest: Local<String>,
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
    // Promedio de las barras del pueblo + qué está haciendo cada quien: se ve de un vistazo
    // si la escasez empuja al crimen o si el pueblo está en paz.
    let (mut food_avg, mut rest_avg, mut belong_avg, mut moral_avg) = (0.0f32, 0.0f32, 0.0f32, 0.0f32);
    let mut hist: std::collections::BTreeMap<&'static str, u32> = std::collections::BTreeMap::new();
    let mut malos = 0u32;
    for np in &npcs {
        food_avg += np.vida.m.get(Motivo::Hambre);
        rest_avg += np.vida.m.get(Motivo::Sueno);
        belong_avg += np.vida.m.get(Motivo::Social);
        moral_avg += np.vida.pers.moral;
        if np.vida.mala_fama > 0.5 {
            malos += 1;
        }
        *hist.entry(np.vida.actividad().verbo(Clase::Aldeano)).or_default() += 1;
    }
    if n > 0 {
        food_avg /= n as f32;
        rest_avg /= n as f32;
        belong_avg /= n as f32;
        moral_avg /= n as f32;
    }
    let mut acts: Vec<(&&str, &u32)> = hist.iter().collect();
    acts.sort_by(|a, b| b.1.cmp(a.1));
    let acts_txt = acts.iter().take(4).map(|(k, v)| format!("{k} {v}")).collect::<Vec<_>>().join(", ");
    let credos_txt = if soc.credos.lista.is_empty() {
        "ninguna".to_string()
    } else {
        soc.credos
            .lista
            .iter()
            .map(|c| format!("{} ({}, dogma {:+.1})", c.nombre, c.miembros, c.dogma))
            .collect::<Vec<_>>()
            .join("; ")
    };
    let (mut ninos, mut ancianos, mut con_pareja) = (0u32, 0u32, 0u32);
    for np in &npcs {
        match linaje::etapa(np.age, np.padre != 0 || np.madre != 0) {
            Etapa::Nino => ninos += 1,
            Etapa::Anciano => ancianos += 1,
            Etapa::Adulto => {}
        }
        if np.pareja != 0 {
            con_pareja += 1;
        }
    }
    let st = &soc.stats;
    // Brújula de asentamientos (throttle 2 s: `settlement_in` no es gratis).
    *acc += time.delta_secs();
    if *acc >= 2.0 || nearest.is_empty() {
        *acc = 0.0;
        *nearest = match (surface_region_active(&region), player.get_single()) {
            (Some(()), Ok(pt)) => nearest_settlement_line(
                active_surface_body(&ctx, &planet),
                pt.translation.x,
                -pt.translation.y,
            ),
            _ => "-".to_string(),
        };
    }
    let nearest_txt: &str = nearest.as_str();
    let seeded_here = soc.active_body().map_or(0, |b| soc.centers.iter().filter(|c| c.0 == b).count());
    let (hh, mm) = day.clock_hm();
    let line = format!(
        "SOCIEDAD  npcs {n}  familias {}  reput [{:+.2} .. {:+.2}]\n\
         COLONIA  salud {:.0}%  comida {:.1}  madera {:.1}  piedra {:.1}  \
         granj {farmers} min {miners} constr {builders} libre {wanderers}\n\
         ANIMO  {hh:02}:{mm:02}  comida {:.0}% descanso {:.0}% compania {:.0}%  moral media {:+.2}  malhechores {malos}\n\
         HACEN  {acts_txt}\n\
         CREDOS  {credos_txt}\n\
         LINAJE  ninos {ninos}  ancianos {ancianos}  en pareja {con_pareja}  parejas formadas {}  nacimientos {}  herencias {}\n\
         HISTORIA  hurtos {}  agresiones {}  detenciones {}  ayudas {}  muertes {}  conversiones {}  sectas {}\n\
         PUEBLOS  sembrados aqui {seeded_here}  mas cercano: {nearest_txt}",
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
        moral_avg,
        st.parejas,
        st.nacimientos,
        st.herencias,
        st.hurtos,
        st.agresiones,
        st.detenciones,
        st.ayudas,
        st.muertes,
        st.conversiones,
        st.fundados,
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

/// **Las parejas del pueblo.** Cada ~2 s los solteros adultos de un mismo pueblo que están
/// cerca se pueden enamorar (más si ya se quieren: `lazos`); al formarse se quieren más aún. Una
/// pareja se deshace sola cuando uno de los dos deja de existir (viudez). Sin parientes cercanos
/// ni niños ni ancianos (ver `ha_shared::linaje::pueden_ser_pareja`).
fn parejas_del_pueblo(time: Res<Time>, mut soc: ResMut<Society>, mut npcs: Query<&mut Npc>, mut acc: Local<(f32, u64)>) {
    acc.0 += time.delta_secs();
    if acc.0 < 2.0 {
        return;
    }
    acc.0 = 0.0;
    acc.1 = acc.1.wrapping_add(1);
    let vivos: std::collections::HashSet<u64> = npcs.iter().map(|n| n.uid).collect();
    for mut n in &mut npcs {
        if n.pareja != 0
            && n.pareja != JUGADOR_UID
            && !vivos.contains(&n.pareja)
            && !soc.records.iter().any(|r| r.uid == n.pareja)
        {
            n.pareja = 0;
        }
    }
    struct Soltero {
        uid: u64,
        x: f32,
        z: f32,
        town: (f32, f32),
        age: f32,
        nacido: bool,
        par: Parentesco,
        vida: Vida,
    }
    let solteros: Vec<Soltero> = npcs
        .iter()
        .filter(|n| n.pareja == 0 && n.health > 0.0)
        .map(|n| Soltero {
            uid: n.uid,
            x: n.x,
            z: n.z,
            town: (n.town_x, n.town_z),
            age: n.age,
            nacido: n.padre != 0 || n.madre != 0,
            par: Parentesco { padre: n.padre, madre: n.madre },
            vida: n.vida,
        })
        .collect();
    let mut formadas: Vec<(u64, u64)> = Vec::new();
    let ronda = acc.1;
    for (i, a) in solteros.iter().enumerate() {
        if formadas.iter().any(|&(x, y)| x == a.uid || y == a.uid) {
            continue;
        }
        for b in &solteros[i + 1..] {
            if formadas.iter().any(|&(x, y)| x == b.uid || y == b.uid) {
                continue;
            }
            if (a.town.0 - b.town.0).abs() > 2.0 || (a.town.1 - b.town.1).abs() > 2.0 {
                continue;
            }
            if (a.x - b.x).powi(2) + (a.z - b.z).powi(2) > linaje::RADIO_CORTEJO * linaje::RADIO_CORTEJO {
                continue;
            }
            let afin = (a.vida.lazos.get(b.uid) + b.vida.lazos.get(a.uid)) * 0.5;
            if !linaje::pueden_ser_pareja(a.uid, a.age, a.nacido, a.par, b.uid, b.age, b.nacido, b.par, afin) {
                continue;
            }
            let h = sims::hash(a.uid.wrapping_mul(0x9E37_79B9) ^ b.uid ^ ronda.wrapping_mul(0x85EB_CA6B));
            let suerte = (h >> 11) as f32 / (1u64 << 53) as f32;
            if suerte < linaje::chance_de_cortejo(afin) {
                formadas.push((a.uid, b.uid));
                break;
            }
        }
    }
    for (ua, ub) in formadas {
        for mut n in &mut npcs {
            let otro = if n.uid == ua {
                ub
            } else if n.uid == ub {
                ua
            } else {
                continue;
            };
            n.pareja = otro;
            n.vida.lazos.sumar(otro, 0.5);
            let nombre = npc_name(otro, n.family);
            push_history(&mut n.history, &format!("Me junté con {nombre}."));
        }
        soc.stats.parejas += 1;
        let familia = |uid: u64| npcs.iter().find(|n| n.uid == uid).map(|n| n.family).unwrap_or(0);
        eprintln!("ha-society: {} y {} son pareja", npc_name(ua, familia(ua)), npc_name(ub, familia(ub)));
    }
}

/// El jugador como pareja o padre en el linaje: no es un NPC, así que tiene un uid propio que
/// no choca con ninguno (los NPC se numeran desde abajo).
pub(crate) const JUGADOR_UID: u64 = u64::MAX;

/// Niño = nacido en el juego (tiene padres) y todavía de poca edad. Los fundadores no lo son.
pub(crate) fn es_nino(age: f32, padre: u64, madre: u64) -> bool {
    linaje::etapa(age, padre != 0 || madre != 0) == Etapa::Nino
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

#[cfg(test)]
mod religion_tests {
    #[test]
    fn cada_vecino_tiene_su_matiz_y_ninguno_se_ve_apagado() {
        use bevy::color::ColorToComponents;
        let mut distintos = std::collections::BTreeSet::new();
        for uid in 1..200u64 {
            let c = tinte_individual(uid).to_srgba();
            assert!(c.red >= 0.87 && c.green >= 0.87 && c.blue >= 0.87, "demasiado oscuro: {c:?}");
            distintos.insert(((c.red * 100.0) as u32, (c.green * 100.0) as u32, (c.blue * 100.0) as u32));
        }
        assert!(distintos.len() > 100, "pocos matices: {}", distintos.len());
        assert_eq!(tinte_individual(7), tinte_individual(7));
    }

    use super::*;

    /// **Los fieles no te siguen.** El pedido es explícito y se rompió una vez:
    /// el converso tomaba al jugador de ídolo y le caminaba atrás. La guardia es
    /// sobre el fuente porque la conducta vive dentro de `tick_society`, que
    /// necesita medio ECS para correr: si alguien vuelve a poner `toward_p`
    /// (el rumbo hacia el jugador) en la misma rama que `converted`, esto salta.
    #[test]
    fn ningun_fiel_apunta_al_jugador() {
        let src = include_str!("society.rs");
        // Sólo el código del módulo, no este test (que nombra las dos cosas).
        let code_only = src.split("mod religion_tests").next().unwrap_or(src);
        for (i, line) in code_only.lines().enumerate() {
            let code = line.split("//").next().unwrap_or("");
            assert!(
                !(code.contains("toward_p") && code.contains("converted")),
                "society.rs:{}: el fiel vuelve a caminar hacia el jugador — la secta \
                 se junta en su santuario, no atras tuyo:\n  {line}",
                i + 1
            );
        }
    }

    /// El santuario está al lado de la plaza de SU pueblo: lo bastante lejos
    /// para que el corro de fieles se lea aparte, lo bastante cerca para seguir
    /// siendo el mismo pueblo.
    #[test]
    fn el_santuario_esta_junto_a_la_plaza() {
        let (sx, sz) = shrine_of(100.0, -40.0);
        let d = ((sx - 100.0).powi(2) + (sz + 40.0).powi(2)).sqrt();
        assert!(d > 4.0, "el santuario cae dentro de la plaza (d={d})");
        assert!(d < 20.0, "el santuario quedo fuera del pueblo (d={d})");
    }

    /// Fundar es idempotente: predicarle a un segundo aldeano no re-funda el
    /// culto ni le cambia el nombre.
    #[test]
    fn fundar_dos_veces_no_cambia_nada() {
        let mut r = Religion::default();
        r.found();
        let name = r.name.clone();
        r.found();
        assert!(r.founded);
        assert_eq!(r.name, name);
        assert!(!name.is_empty());
    }
}

#[cfg(test)]
mod vivienda_tests {
    use super::*;

    fn casa() -> Huella {
        Huella { min: (10, 10), w: 4, h: 3, puerta: (12, 13) }
    }

    /// Con casa, el sitio de dormir queda ADENTRO del edificio y usa la casa entera.
    #[test]
    fn con_casa_el_sitio_esta_adentro() {
        let viv = Vivienda { casa: Some(casa()), puesto: None, parque: Some((50.0, 50.0)) };
        let s = sitio_de((0.0, 0.0), (5.0, 5.0), 7, 1, &viv);
        assert!(s.casa_uso > 0.0);
        assert!(casa().dentro(s.casa.0, s.casa.1), "el lugar de dormir cae fuera de la casa: {:?}", s.casa);
        assert_eq!(s.parque, (50.0, 50.0));
    }

    /// Sin casa (colono, partida vieja) cae a puntos sueltos y `casa_uso` no promete de más.
    #[test]
    fn sin_casa_cae_a_puntos_sueltos() {
        let s = sitio_de((3.0, 4.0), (5.0, 5.0), 7, 1, &Vivienda::default());
        assert_eq!(s.casa_uso, 0.0);
    }

    #[test]
    fn huellas_lista_solo_lo_que_hay() {
        assert_eq!(Vivienda::default().huellas().1, 0);
        let v = Vivienda { casa: Some(casa()), puesto: Some(casa()), parque: None };
        assert_eq!(v.huellas().1, 2);
        let v = Vivienda { casa: None, puesto: Some(casa()), parque: None };
        assert_eq!(v.huellas().1, 1);
    }

    /// Una partida guardada sin `Vivienda` sigue leyéndose (migración por `Default`).
    #[test]
    fn vivienda_serde_ida_y_vuelta() {
        let v = Vivienda { casa: Some(casa()), puesto: None, parque: Some((1.0, 2.0)) };
        let j = serde_json::to_string(&v).unwrap();
        assert_eq!(serde_json::from_str::<Vivienda>(&j).unwrap(), v);
    }
}

#[cfg(test)]
mod humor_tests {
    use super::*;

    #[test]
    fn hambre_baja_el_animo_y_abundancia_lo_sube() {
        let mut v = Vida::nueva(3);
        v.m.set(Motivo::Seguridad, 0.5);
        aplicar_humor(&mut v, 0.0);
        assert!(v.m.get(Motivo::Seguridad) < 0.5);
        let mut w = Vida::nueva(3);
        w.m.set(Motivo::Seguridad, 0.5);
        aplicar_humor(&mut w, 1.0);
        assert!(w.m.get(Motivo::Seguridad) > 0.5);
        let mut x = Vida::nueva(3);
        x.m.set(Motivo::Seguridad, 0.5);
        aplicar_humor(&mut x, 0.6);
        assert_eq!(x.m.get(Motivo::Seguridad), 0.5);
    }

    #[test]
    fn el_stock_viejo_sin_obras_se_lee() {
        let s: Stock = serde_json::from_str(r#"{"food":1.0,"wood":2.0,"stone":3.0}"#).unwrap();
        assert_eq!(s.obras, 0);
    }
}

#[cfg(test)]
mod linaje_tests {
    use super::*;
    use std::time::Duration;

    fn npc(uid: u64, x: f32, age: f32, padre: u64, madre: u64) -> Npc {
        Npc {
            brain: 0,
            x,
            z: 0.0,
            heading: 0.0,
            family: 0,
            uid,
            energy: 1.0,
            age,
            pareja: 0,
            padre,
            madre,
            faith: 0.0,
            converted: false,
            home_x: x,
            home_z: 0.0,
            town_x: 0.0,
            town_z: 0.0,
            say_cd: 0.0,
            history: Vec::new(),
            health: 1.0,
            calm_cd: 0.0,
            convert_cd: 0.0,
            role: Role::Wanderer,
            vida: Vida::nueva(uid),
            bump: 0.0,
            vivienda: Vivienda::default(),
            ruta: Vec::new(),
            ruta_meta: (0.0, 0.0),
            pide_ruta: false,
            atasco: 0,
        }
    }

    fn mundo(npcs: Vec<Npc>) -> App {
        let mut app = App::new();
        app.insert_resource(Time::<()>::default());
        app.insert_resource(Society::default());
        app.add_systems(Update, parejas_del_pueblo);
        for n in npcs {
            app.world_mut().spawn(n);
        }
        app
    }

    fn rondas(app: &mut App, n: usize) {
        for _ in 0..n {
            app.world_mut().resource_mut::<Time>().advance_by(Duration::from_secs(3));
            app.update();
        }
    }

    fn parejas(app: &mut App) -> Vec<(u64, u64)> {
        let mut q = app.world_mut().query::<&Npc>();
        q.iter(app.world()).map(|n| (n.uid, n.pareja)).collect()
    }

    /// Un hombre y una mujer adultos del mismo pueblo, cerca, terminan juntos — y de a dos.
    #[test]
    fn dos_adultos_compatibles_se_emparejan() {
        let (mut h, mut m) = (0u64, 0u64);
        for u in 1..200 {
            if linaje::es_mujer(u) && m == 0 {
                m = u;
            }
            if !linaje::es_mujer(u) && h == 0 {
                h = u;
            }
        }
        let mut app = mundo(vec![npc(h, 0.0, 0.0, 0, 0), npc(m, 3.0, 0.0, 0, 0)]);
        rondas(&mut app, 60);
        let p = parejas(&mut app);
        assert!(p.iter().all(|&(u, par)| par == if u == h { m } else { h }), "no se emparejaron: {p:?}");
        assert!(app.world().resource::<Society>().stats.parejas >= 1);
    }

    /// Un niño nunca forma pareja, ni hermanos entre sí.
    #[test]
    fn ni_ninos_ni_hermanos() {
        let (mut h, mut m) = (0u64, 0u64);
        for u in 1..200 {
            if linaje::es_mujer(u) && m == 0 {
                m = u;
            }
            if !linaje::es_mujer(u) && h == 0 {
                h = u;
            }
        }
        let mut app = mundo(vec![npc(h, 0.0, 10.0, 900, 901), npc(m, 3.0, 10.0, 900, 901)]);
        rondas(&mut app, 80);
        assert!(parejas(&mut app).iter().all(|&(_, p)| p == 0), "niños emparejados");
        let adultos = 2.0 * linaje::NINO_S;
        let mut app = mundo(vec![npc(h, 0.0, adultos, 900, 901), npc(m, 3.0, adultos, 900, 901)]);
        rondas(&mut app, 80);
        assert!(parejas(&mut app).iter().all(|&(_, p)| p == 0), "hermanos emparejados");
    }

    /// Si uno de la pareja ya no existe, el que queda vuelve a estar soltero (viudez).
    #[test]
    fn la_viudez_suelta_a_la_pareja() {
        let mut v = npc(5, 0.0, 0.0, 0, 0);
        v.pareja = 77;
        let mut app = mundo(vec![v]);
        rondas(&mut app, 2);
        assert_eq!(parejas(&mut app), vec![(5, 0)]);
    }

    /// Quien se juntó con el jugador no enviuda por no encontrarlo entre los NPC.
    #[test]
    fn la_pareja_del_jugador_no_enviuda() {
        let mut v = npc(6, 0.0, 0.0, 0, 0);
        v.pareja = JUGADOR_UID;
        let mut app = mundo(vec![v]);
        rondas(&mut app, 3);
        assert_eq!(parejas(&mut app), vec![(6, JUGADOR_UID)]);
    }

    #[test]
    fn el_nino_no_trabaja_y_el_fundador_si() {
        assert!(es_nino(10.0, 1, 2));
        assert!(!es_nino(10.0, 0, 0));
        assert!(!es_nino(linaje::NINO_S * 2.0, 1, 2));
    }

    /// Muere el dueño de una casa: la toma la pareja que no tiene; el que ya tiene casa no.
    #[test]
    fn la_pareja_hereda_la_casa_y_quien_ya_tiene_no() {
        let casa = Huella { min: (0, 0), w: 4, h: 4, puerta: (1, 3) };
        let mut viuda = npc(8, 0.0, 0.0, 0, 0);
        viuda.pareja = 7;
        let mut con_casa = npc(9, 0.0, 0.0, 0, 0);
        con_casa.pareja = 7;
        con_casa.vivienda.casa = Some(casa);
        let mut app = App::new();
        app.insert_resource(Society::default());
        app.add_systems(Update, repartir_herencias);
        app.world_mut().spawn(viuda);
        app.world_mut().spawn(con_casa);
        let viv = Vivienda { casa: Some(casa), ..Default::default() };
        app.world_mut().resource_mut::<Society>().herencias.push((7, 8, viv));
        app.update();
        let mut q = app.world_mut().query::<&Npc>();
        let r: Vec<(u64, bool)> = q.iter(app.world()).map(|n| (n.uid, n.vivienda.casa.is_some())).collect();
        assert!(r.contains(&(8, true)), "la viuda no heredó: {r:?}");
        assert_eq!(app.world().resource::<Society>().stats.herencias, 1);
    }
}
