//! **ARMAS DE FUEGO** (3 oct 2026): pistola, cinco subfusiles de la Segunda Guerra, un fusil de asalto, una escopeta y una
//! pistola de rayos, cada una en siete aspectos (base + seis camuflajes tipo Black Ops).
//!
//! * **Cómo se juega.** Con un arma en la mano, apoyar el dedo en el mundo apunta y dispara hacia ese punto (el
//!   mantenido es ráfaga en las automáticas; las semiautomáticas disparan una vez por toque). El botón USAR dice
//!   RECARGAR; cargador vacío + disparo recarga solo. En escritorio: clic izquierdo y la tecla R.
//! * **Apuntado asistido** (`apuntado.rs`, skill `disparo-y-apuntado`): el dedo dice a quién, un cono angosto elige el
//!   infectado y la bala se dobla hasta 9° hacia él; la mira se dibuja encima del blanco (el dedo lo tapa), roja si
//!   pega de lleno. Nunca ayuda contra aldeanos ni animales.
//! * **Balas de verdad**: entidades con velocidad, alcance, dispersión y caída del daño; se frenan en paredes y bloques
//!   (la misma geometría que decide el paso del jugador) y le pegan a **cualquiera** — infectados, animales o
//!   aldeanos. Cada módulo aplica el daño con SU propia lógica de muerte (`infectados`, `fauna`, `society`): acá sólo
//!   vuela la bala.
//! * **Munición**: cinco calibres (`ammo_45`, `ammo_9mm`, `ammo_762`, `ammo_cartucho`, `ammo_energia`), que se sacan de la
//!   mochila al recargar. Cada arma lleva su cargador; se encuentra en los infectados, los alijos y los cofres.
//! * **Ruido**: un disparo atrae a los infectados desde mucho más lejos (`RuidoDeArma`).
//! * **Consecuencias**: dispararle a un aldeano tiene los mismos costos sociales que pegarle (lo maneja `society`).
//!
//! Todo es de cliente y barato: nada se sintetiza (los sonidos vienen pre-renderizados) y las balas son quads.

use std::collections::HashMap;
use std::sync::OnceLock;

use bevy::prelude::*;
use bevy::window::PrimaryWindow;

use crate::audio::{PlaySfx, PlaySfxAt, Sfx};
use crate::camera::{CameraShakeEvent, GameCamera};
use crate::game_phase::GamePhase;
use crate::input::{AimCursor, EquippedItem, TouchActions};
use crate::inventory_persist::{InventoryPersistState, PlayerInventory};
use crate::scene::WorldContextRuntime;
use crate::sim::{Player, PlayerVisual};
use crate::walls::BuiltWalls;
use crate::world2d::{active_surface_body, ActivePlanet, TileWorld};
use ha_shared::build_grid::WallAxis;
use ha_shared::topdown::TILE_M;

// ---------------------------------------------------------------------------------------------------------------------
// Catálogo
// ---------------------------------------------------------------------------------------------------------------------

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Calibre {
    C45,
    C9,
    C762,
    Cartucho,
    Energia,
}

impl Calibre {
    /// El ítem de munición de este calibre.
    pub fn item(self) -> &'static str {
        match self {
            Calibre::C45 => "ammo_45",
            Calibre::C9 => "ammo_9mm",
            Calibre::C762 => "ammo_762",
            Calibre::Cartucho => "ammo_cartucho",
            Calibre::Energia => "ammo_energia",
        }
    }
    pub fn etiqueta(self) -> &'static str {
        match self {
            Calibre::C45 => "Balas .45",
            Calibre::C9 => "Balas 9 mm",
            Calibre::C762 => "Balas 7,62",
            Calibre::Cartucho => "Cartuchos",
            Calibre::Energia => "Celdas de energía",
        }
    }
    pub const TODOS: [Calibre; 5] = [Calibre::C45, Calibre::C9, Calibre::C762, Calibre::Cartucho, Calibre::Energia];
}

/// La ficha de un arma. El **daño** está en la escala de los golpes cuerpo a cuerpo (un infectado tiene 12 de vida, una
/// espada pega 5-7): la pistola mata de dos tiros, el fusil también, un subfusil de tres o cuatro.
pub struct ArmaDef {
    /// `gun_<id>` es el ítem.
    pub id: &'static str,
    pub nombre: &'static str,
    pub calibre: Calibre,
    /// Daño por proyectil.
    pub dano: f32,
    /// Disparos por segundo.
    pub cadencia: f32,
    /// Mantener dispara en ráfaga (si no, un disparo por toque).
    pub auto: bool,
    pub cargador: u32,
    /// Segundos de recarga completa.
    pub recarga_s: f32,
    /// Dispersión (grados, ±).
    pub dispersion: f32,
    /// Proyectiles por disparo (la escopeta: perdigones).
    pub perdigones: u32,
    /// Velocidad de la bala (m/s).
    pub vel: f32,
    /// Alcance (m); más allá la bala desaparece y el daño cae linealmente hasta el 55 %.
    pub alcance: f32,
    /// A cuántos objetivos atraviesa.
    pub atraviesa: u8,
    /// Sacudida de cámara al disparar (0..1).
    pub retroceso: f32,
    pub sfx: Sfx,
    pub impacto: Sfx,
    /// Color de la estela de la bala.
    pub estela: [f32; 3],
}

pub const ARMAS: [ArmaDef; 9] = [
    ArmaDef { id: "pistola", nombre: "Pistola M1911", calibre: Calibre::C45, dano: 6.0, cadencia: 3.6, auto: false, cargador: 8, recarga_s: 1.5, dispersion: 1.8, perdigones: 1, vel: 75.0, alcance: 30.0, atraviesa: 1, retroceso: 0.10, sfx: Sfx::ShotPistol, impacto: Sfx::BulletHit, estela: [1.0, 0.86, 0.45] },
    ArmaDef { id: "thompson", nombre: "Thompson M1A1", calibre: Calibre::C45, dano: 4.0, cadencia: 11.0, auto: true, cargador: 30, recarga_s: 2.2, dispersion: 4.2, perdigones: 1, vel: 70.0, alcance: 26.0, atraviesa: 1, retroceso: 0.08, sfx: Sfx::ShotSmg, impacto: Sfx::BulletHit, estela: [1.0, 0.82, 0.4] },
    ArmaDef { id: "ppsh", nombre: "PPSh-41", calibre: Calibre::C762, dano: 3.4, cadencia: 15.0, auto: true, cargador: 71, recarga_s: 3.0, dispersion: 5.2, perdigones: 1, vel: 68.0, alcance: 24.0, atraviesa: 1, retroceso: 0.07, sfx: Sfx::ShotSmg, impacto: Sfx::BulletHit, estela: [1.0, 0.78, 0.38] },
    ArmaDef { id: "mp40", nombre: "MP40", calibre: Calibre::C9, dano: 3.4, cadencia: 9.5, auto: true, cargador: 32, recarga_s: 2.0, dispersion: 3.6, perdigones: 1, vel: 66.0, alcance: 25.0, atraviesa: 1, retroceso: 0.07, sfx: Sfx::ShotSmg, impacto: Sfx::BulletHit, estela: [1.0, 0.88, 0.5] },
    ArmaDef { id: "suomi", nombre: "Suomi KP/-31", calibre: Calibre::C9, dano: 3.2, cadencia: 14.0, auto: true, cargador: 50, recarga_s: 2.8, dispersion: 4.6, perdigones: 1, vel: 68.0, alcance: 25.0, atraviesa: 1, retroceso: 0.07, sfx: Sfx::ShotSmg, impacto: Sfx::BulletHit, estela: [1.0, 0.84, 0.46] },
    ArmaDef { id: "pps43", nombre: "PPS-43", calibre: Calibre::C762, dano: 3.6, cadencia: 11.0, auto: true, cargador: 35, recarga_s: 2.1, dispersion: 4.0, perdigones: 1, vel: 66.0, alcance: 24.0, atraviesa: 1, retroceso: 0.07, sfx: Sfx::ShotSmg, impacto: Sfx::BulletHit, estela: [1.0, 0.8, 0.42] },
    ArmaDef { id: "ak", nombre: "Fusil de asalto AK", calibre: Calibre::C762, dano: 6.2, cadencia: 9.0, auto: true, cargador: 30, recarga_s: 2.4, dispersion: 2.8, perdigones: 1, vel: 95.0, alcance: 42.0, atraviesa: 2, retroceso: 0.14, sfx: Sfx::ShotRifle, impacto: Sfx::BulletHit, estela: [1.0, 0.9, 0.55] },
    ArmaDef { id: "escopeta", nombre: "Escopeta de ánima lisa", calibre: Calibre::Cartucho, dano: 2.3, cadencia: 1.15, auto: false, cargador: 6, recarga_s: 3.2, dispersion: 7.5, perdigones: 8, vel: 60.0, alcance: 15.0, atraviesa: 1, retroceso: 0.30, sfx: Sfx::ShotShotgun, impacto: Sfx::BulletHit, estela: [1.0, 0.8, 0.4] },
    ArmaDef { id: "raygun", nombre: "Pistola de rayos", calibre: Calibre::Energia, dano: 13.0, cadencia: 2.6, auto: false, cargador: 20, recarga_s: 1.9, dispersion: 0.6, perdigones: 1, vel: 42.0, alcance: 34.0, atraviesa: 3, retroceso: 0.06, sfx: Sfx::ShotRay, impacto: Sfx::RayHit, estela: [0.35, 1.0, 0.5] },
];

/// Los aspectos (camuflajes) y cómo se llaman. El orden y los ids salen de `scripts/lib/armas.py::CAMOS`.
pub const CAMUFLAJES: [(&str, &str); 6] = [
    ("115", "Weaponized 115"),
    ("singularidad", "Singularidad"),
    ("grafiti", "Grafiti"),
    ("bacon", "Bacon"),
    ("ciclon", "Ciclón"),
    ("oscura", "Materia oscura"),
];

/// Separa `gun_<arma>[__<camo>]`.
pub fn parsear(item: &str) -> Option<(&'static ArmaDef, Option<&'static str>)> {
    let resto = item.strip_prefix("gun_")?;
    let (arma, camo) = match resto.split_once("__") {
        Some((a, c)) => (a, Some(c)),
        None => (resto, None),
    };
    let def = ARMAS.iter().find(|d| d.id == arma)?;
    let camo = match camo {
        Some(c) => Some(CAMUFLAJES.iter().find(|(id, _)| *id == c)?.0),
        None => None,
    };
    Some((def, camo))
}

pub fn es_arma(item: &str) -> bool {
    parsear(item).is_some()
}

pub fn es_municion(item: &str) -> bool {
    Calibre::TODOS.iter().any(|c| c.item() == item)
}

/// Nombre en castellano para mochila y avisos.
pub fn nombre_es(item: &str) -> Option<String> {
    if let Some((def, camo)) = parsear(item) {
        return Some(match camo {
            Some(c) => format!("{} · {}", def.nombre, CAMUFLAJES.iter().find(|(id, _)| *id == c).map(|(_, n)| *n).unwrap_or(c)),
            None => def.nombre.to_string(),
        });
    }
    Calibre::TODOS.iter().find(|c| c.item() == item).map(|c| c.etiqueta().to_string())
}

/// Nombre en castellano como `&'static str` (para las listas de la mochila, que piden una referencia estable).
pub fn etiqueta_estatica(item: &str) -> Option<&'static str> {
    static NOMBRES: OnceLock<HashMap<String, &'static str>> = OnceLock::new();
    NOMBRES
        .get_or_init(|| {
            let mut m = HashMap::new();
            for it in items_de_arma() {
                if let Some(n) = nombre_es(it) {
                    m.insert((*it).to_string(), &*Box::leak(n.into_boxed_str()));
                }
            }
            for c in Calibre::TODOS {
                m.insert(c.item().to_string(), c.etiqueta());
            }
            m
        })
        .get(item)
        .copied()
}

/// Todos los ítems de arma (9 × 7), con `&'static str` para poder meterlos en el botín.
pub fn items_de_arma() -> &'static [&'static str] {
    static TODOS: OnceLock<Vec<&'static str>> = OnceLock::new();
    TODOS.get_or_init(|| {
        let mut v: Vec<&'static str> = Vec::new();
        for d in &ARMAS {
            v.push(Box::leak(format!("gun_{}", d.id).into_boxed_str()));
            for (c, _) in &CAMUFLAJES {
                v.push(Box::leak(format!("gun_{}__{}", d.id, c).into_boxed_str()));
            }
        }
        v
    })
}

/// **Un arma al azar para el botín**, con `r` de 0..1: las bases abundan y cada camuflaje es raro (el 115 y la materia
/// oscura, los más raros). Determinista por `r` para que el botín de un infectado sea el mismo siempre.
pub fn arma_al_azar(r1: f32, r2: f32, r3: f32) -> &'static str {
    let armas = items_de_arma();
    let por_arma = 1 + CAMUFLAJES.len();
    // las armas fuertes (fusil, escopeta, rayos) salen menos
    let pesos = [10.0f32, 8.0, 5.0, 6.0, 4.0, 5.0, 5.0, 4.0, 1.5];
    let total: f32 = pesos.iter().sum();
    let mut acc = 0.0;
    let mut a = pesos.len() - 1;
    for (i, p) in pesos.iter().enumerate() {
        acc += p / total;
        if r1 <= acc {
            a = i;
            break;
        }
    }
    // 62 % base; el resto se reparte con los camuflajes raros al final
    let c = if r2 < 0.62 {
        0
    } else {
        let t = (r3 * CAMUFLAJES.len() as f32) as usize;
        1 + t.min(CAMUFLAJES.len() - 1)
    };
    armas[a * por_arma + c]
}

/// Munición para el botín: calibre al azar (el de energía es raro).
pub fn municion_al_azar(r1: f32, r2: f32) -> (&'static str, u32) {
    let cal = if r1 < 0.30 {
        Calibre::C45
    } else if r1 < 0.58 {
        Calibre::C9
    } else if r1 < 0.84 {
        Calibre::C762
    } else if r1 < 0.97 {
        Calibre::Cartucho
    } else {
        Calibre::Energia
    };
    let n = match cal {
        Calibre::C45 | Calibre::C9 => 8 + (r2 * 16.0) as u32,
        Calibre::C762 => 10 + (r2 * 20.0) as u32,
        Calibre::Cartucho => 3 + (r2 * 6.0) as u32,
        Calibre::Energia => 6 + (r2 * 10.0) as u32,
    };
    (cal.item(), n)
}

/// Daño de una bala a esa distancia: pleno hasta el 40 % del alcance y cae hasta el 55 % al final.
pub fn dano_a_distancia(dano: f32, recorrido: f32, alcance: f32) -> f32 {
    let t = ((recorrido / alcance.max(0.1)) - 0.4).max(0.0) / 0.6;
    dano * (1.0 - 0.45 * t.min(1.0))
}

/// Segundos entre disparos.
pub fn intervalo(def: &ArmaDef) -> f32 {
    1.0 / def.cadencia.max(0.1)
}

/// Segundos de la recarga: la escopeta carga cartucho por cartucho (proporcional a lo que le falta).
pub fn duracion_recarga(def: &ArmaDef, faltan: u32) -> f32 {
    if def.calibre == Calibre::Cartucho {
        0.5 + def.recarga_s * (faltan as f32 / def.cargador as f32)
    } else {
        def.recarga_s
    }
}

// ---------------------------------------------------------------------------------------------------------------------
// Estado
// ---------------------------------------------------------------------------------------------------------------------

struct Recarga {
    item: String,
    t: f32,
    total: f32,
    /// Qué ruido ya sonó: 0 nada, 1 sacó el cargador, 2 lo metió.
    fase: u8,
}

#[derive(Resource, Default)]
pub struct EstadoArmas {
    /// Balas en el arma, por ítem.
    cargadas: HashMap<String, u32>,
    cd: f32,
    recarga: Option<Recarga>,
    apretado_antes: bool,
    /// Retroceso visual (m) que decae.
    kick: f32,
    aviso: Option<(String, f32)>,
    /// El último ítem visto en la mano, para cancelar la recarga si lo cambiás.
    en_mano: String,
    /// Blanco del apuntado asistido (bits de la `Entity`), para la histéresis entre cuadros.
    blanco: Option<u64>,
    /// Dónde está ese blanco y si la bala le va a pegar de lleno («mira en rojo»): la mira se dibuja ENCIMA de él,
    /// que el dedo no tapa.
    mira_en: Option<(Vec2, bool)>,
    /// Segundos de gatillo sostenido: la primera bala va derecha y la ráfaga abre (`apuntado::dispersion_efectiva`).
    rafaga_s: f32,
}

impl EstadoArmas {
    pub fn cargadas_de(&self, item: &str) -> Option<u32> {
        self.cargadas.get(item).copied()
    }

    /// Retroceso visual (m): salta en cada disparo y decae. `pose_jugador` lee el salto como «disparó».
    pub fn retroceso(&self) -> f32 {
        self.kick
    }

    /// `true` mientras se recarga (el cuerpo hace el gesto de manipular el arma).
    pub fn recargando(&self) -> bool {
        self.recarga.is_some()
    }
}

/// Segundos que quedan de «se oyó un tiro»: los infectados lo oyen desde más lejos.
#[derive(Resource, Default)]
pub struct RuidoDeArma(pub f32);

/// Multiplicador del olfato de los infectados mientras suena un tiro.
pub const OIDO_DE_TIRO: f32 = 2.6;

/// Una bala en vuelo. Los módulos que tienen blancos (`infectados`, `fauna`, `society`) leen estas entidades y aplican el
/// daño a los suyos.
#[derive(Component)]
pub struct Bala {
    /// Posición (x, z) del mundo y la del cuadro anterior.
    pub pos: Vec2,
    pub prev: Vec2,
    pub vel: Vec2,
    pub dano: f32,
    pub origen: Vec2,
    pub alcance: f32,
    pub atraviesa: u8,
    pub golpeados: Vec<Entity>,
    pub viva: bool,
    pub impacto: Sfx,
    pub rayo: bool,
}

impl Bala {
    /// ¿Ya le pegó a este blanco? (una bala no le pega dos veces al mismo).
    pub fn ya_golpeo(&self, e: Entity) -> bool {
        self.golpeados.contains(&e)
    }

    /// Daño efectivo con la distancia recorrida.
    pub fn dano_ahora(&self) -> f32 {
        dano_a_distancia(self.dano, self.pos.distance(self.origen), self.alcance)
    }

    /// Registra un impacto en `e`: si ya no atraviesa más, la bala muere.
    pub fn registrar(&mut self, e: Entity) {
        self.golpeados.push(e);
        if self.golpeados.len() as u8 >= self.atraviesa {
            self.viva = false;
        }
    }
}

/// Distancia de un punto a un segmento.
pub fn distancia_a_segmento(p: Vec2, a: Vec2, b: Vec2) -> f32 {
    let ab = b - a;
    let l2 = ab.length_squared();
    if l2 < 1e-9 {
        return p.distance(a);
    }
    let t = ((p - a).dot(ab) / l2).clamp(0.0, 1.0);
    p.distance(a + ab * t)
}

#[derive(Component)]
struct EstelaDeBala;

#[derive(Component)]
struct ArmaVisual;

#[derive(Component)]
struct Chispa {
    vel: Vec2,
    vida: f32,
}

#[derive(Component)]
struct Fogonazo(f32);

#[derive(Component)]
struct MiraDeArma;

#[derive(Component)]
struct HudArmas;

/// Fases de la bala: primero vuela (`Mover`), después cada módulo con blancos la prueba contra los suyos (`Impactos`) y
/// al final se limpian las que murieron.
#[derive(SystemSet, Clone, Debug, PartialEq, Eq, Hash)]
pub enum ArmasSet {
    Mover,
    Impactos,
}

pub struct ArmasPlugin;

impl Plugin for ArmasPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<EstadoArmas>()
            .init_resource::<RuidoDeArma>()
            .add_systems(Update, (apuntar_y_disparar, recargar, mover_balas.in_set(ArmasSet::Mover)).chain().run_if(in_state(GamePhase::Playing)))
            .add_systems(
                Update,
                limpiar_balas.after(ArmasSet::Impactos).run_if(in_state(GamePhase::Playing)),
            )
            .add_systems(Update, (visual_del_arma, efectos, hud_armas, regalo_inicial).run_if(in_state(GamePhase::Playing)))
            .add_systems(OnExit(GamePhase::Playing), limpiar_todo);
    }
}

fn limpiar_todo(
    mut commands: Commands,
    mut estado: ResMut<EstadoArmas>,
    q: Query<Entity, Or<(With<Bala>, With<ArmaVisual>, With<Chispa>, With<Fogonazo>, With<MiraDeArma>, With<HudArmas>)>>,
) {
    for e in &q {
        commands.entity(e).despawn_recursive();
    }
    *estado = EstadoArmas::default();
}

/// Quien empieza una partida con las manos vacías recibe una pistola y dos cargadores: sin esto el contenido de armas
/// quedaría detrás de encontrar un infectado con suerte. Es un regalo, no una receta: se da una vez.
fn regalo_inicial(
    mut dado: Local<bool>,
    mut inv: ResMut<PlayerInventory>,
    mut inv_ui: ResMut<crate::hud::inventory_ui::InventoryUiState>,
    mut persist: ResMut<InventoryPersistState>,
    time: Res<Time>,
) {
    if *dado || time.elapsed_secs() < 3.0 {
        return;
    }
    *dado = true;
    if inv.items.values().any(|&q| q > 0) {
        return;
    }
    crate::hud::inventory_ui::add_item_to_slots(&mut inv_ui, "gun_pistola", 1);
    crate::hud::inventory_ui::add_item_to_slots(&mut inv_ui, "ammo_45", 24);
    crate::hud::inventory_ui::flush_slots_to_inventory(&inv_ui, &mut inv);
    persist.mark_dirty();
}

// ---------------------------------------------------------------------------------------------------------------------
// Disparo
// ---------------------------------------------------------------------------------------------------------------------

/// Hacia dónde mira el jugador cuando no hay un punto apuntado.
fn rumbo_del_jugador(visual: &PlayerVisual) -> Vec2 {
    Vec2::new(visual.to_rot.cos(), visual.to_rot.sin())
}

#[allow(clippy::too_many_arguments)]
fn apuntar_y_disparar(
    mut commands: Commands,
    time: Res<Time>,
    equipped: Res<EquippedItem>,
    cursor: Res<AimCursor>,
    (stick, mut accion): (Res<crate::input::AimStick>, ResMut<crate::pose_jugador::AccionJugador>),
    (mouse, windows, cam): (
        Res<ButtonInput<MouseButton>>,
        Query<&Window, With<PrimaryWindow>>,
        Query<(&Camera, &GlobalTransform), With<GameCamera>>,
    ),
    (gameplay_block, editor, fly): (
        Res<crate::game_ui::UiGameplayBlock>,
        Res<crate::ui_editor::UiEditorState>,
        Res<crate::creative_fly::CreativeFlight>,
    ),
    (mut estado, mut ruido): (ResMut<EstadoArmas>, ResMut<RuidoDeArma>),
    player: Query<(&Transform, &PlayerVisual), With<Player>>,
    (mut sfx, mut sfx_ui, mut shake): (EventWriter<PlaySfxAt>, EventWriter<PlaySfx>, EventWriter<CameraShakeEvent>),
    glow: Option<Res<crate::light2d::GlowTexture>>,
    inv: Res<PlayerInventory>,
    (ctx, planet, tiles, walls): (Res<WorldContextRuntime>, Res<ActivePlanet>, Res<TileWorld>, Res<BuiltWalls>),
    infectados: Query<(Entity, &crate::infectados::Infectado)>,
    mut pos_antes: Local<Option<Vec2>>,
) {
    let dt = time.delta_secs();
    estado.cd = (estado.cd - dt).max(0.0);
    estado.kick = (estado.kick - dt * 3.2).max(0.0);
    ruido.0 = (ruido.0 - dt).max(0.0);
    if let Some((_, t)) = estado.aviso.as_mut() {
        *t -= dt;
    }
    if estado.aviso.as_ref().map_or(false, |(_, t)| *t <= 0.0) {
        estado.aviso = None;
    }

    // Cambiar de ítem corta la recarga.
    if estado.en_mano != equipped.item_id {
        estado.recarga = None;
        estado.en_mano = equipped.item_id.clone();
        estado.apretado_antes = true; // el toque que equipó no dispara
    }
    let Some((def, _camo)) = parsear(&equipped.item_id) else {
        estado.blanco = None;
        estado.mira_en = None;
        return;
    };
    let Ok((ptf, visual)) = player.get_single() else { return };
    let pos = Vec2::new(ptf.translation.x, -ptf.translation.y);
    // ¿Corre? (la dispersión abre un poco): por lo que se movió desde el cuadro anterior.
    let corriendo = pos_antes.map_or(false, |a| dt > 0.0 && a.distance(pos) / dt > 2.5);
    *pos_antes = Some(pos);

    // ¿Se puede disparar ahora? (nada de paneles, editor ni vuelo)
    let bloqueado = gameplay_block.blocked || editor.enabled || editor.menu_open || fly.flying();

    // El punto apuntado: el dedo en el mundo, o el mouse en escritorio.
    let mut punto: Option<Vec2> = None;
    let mut apretado = false;
    if stick.armado && stick.activo && stick.tiene_dir {
        // Stick de puntería: manda sobre el dedo en el mundo. Dispara sólo lejos del centro.
        punto = Some(pos + stick.dir * 8.0);
        apretado = stick.dispara;
        accion.apuntar(stick.dir.y.atan2(stick.dir.x));
    } else if cursor.targeting() && cursor.active {
        punto = Some(cursor.world);
        apretado = true;
    } else if !crate::platform::uses_touch_gameplay_ui() {
        if let (Ok(win), Ok((camera, cam_tf))) = (windows.get_single(), cam.get_single()) {
            if let Some(c) = win.cursor_position() {
                if let Some(r) = crate::render::scale::window_to_world_2d(camera, cam_tf, win, c) {
                    punto = Some(Vec2::new(r.x, -r.y));
                    apretado = mouse.pressed(MouseButton::Left);
                }
            }
        }
    }
    let dir = punto.map(|p| (p - pos).normalize_or_zero()).filter(|d| *d != Vec2::ZERO).unwrap_or_else(|| rumbo_del_jugador(visual));

    // **Apuntado asistido** (`apuntado.rs`, skill `disparo-y-apuntado`): el dedo dice a quién, el cono elige el
    // infectado y la bala se dobla hacia él lo justo. Sólo infectados vivos y a la vista: nunca aldeanos ni animales,
    // la ayuda no puede convertir un tiro errado en un crimen.
    let cfg = crate::apuntado::Asistencia::default();
    let elegido = if punto.is_some() && !bloqueado {
        let body = active_surface_body(&ctx, &planet);
        let blancos: Vec<crate::apuntado::Blanco> = infectados
            .iter()
            .filter(|(_, i)| i.vida > 0.0)
            .map(|(e, i)| crate::apuntado::Blanco { id: e.to_bits(), pos: Vec2::new(i.x, i.z), vel: Vec2::ZERO, radio: 0.55 })
            .filter(|b| b.pos.distance_squared(pos) <= (def.alcance + 1.0) * (def.alcance + 1.0))
            .filter(|b| a_la_vista(pos, b.pos, body, tiles.z, &tiles, &walls))
            .collect();
        crate::apuntado::elegir_blanco(pos, dir, punto, def.alcance, &blancos, estado.blanco, &cfg).map(|i| blancos[i])
    } else {
        None
    };
    estado.blanco = elegido.map(|b| b.id);
    estado.mira_en = elegido.map(|b| (b.pos, crate::apuntado::en_rojo(pos, dir, &b, def.vel, &cfg)));
    let dir = crate::apuntado::rumbo_asistido(pos, dir, elegido.as_ref(), def.vel, &cfg);

    let flanco = apretado && !estado.apretado_antes;
    estado.apretado_antes = apretado;
    estado.rafaga_s = if apretado { estado.rafaga_s + dt } else { 0.0 };
    if bloqueado || !apretado || estado.recarga.is_some() || estado.cd > 0.0 {
        return;
    }
    if !def.auto && !flanco {
        return;
    }

    let cargadas = estado.cargadas.entry(equipped.item_id.clone()).or_insert(def.cargador);
    if *cargadas == 0 {
        // Gatillo en seco; si hay munición en la mochila, recarga sola.
        estado.cd = 0.35;
        if flanco || def.auto {
            sfx_ui.send(PlaySfx(Sfx::DryFire));
        }
        if inv.qty(def.calibre.item()) > 0 {
            iniciar_recarga(&mut estado, def, &equipped.item_id, &inv);
        } else {
            estado.aviso = Some((format!("SIN {}", def.calibre.etiqueta().to_uppercase()), 1.6));
        }
        return;
    }
    *cargadas -= 1;
    estado.cd = intervalo(def);
    estado.kick = 0.35 + def.retroceso;
    ruido.0 = 5.0;

    // Los proyectiles salen de la boca del arma.
    let boca = pos + dir * 1.15;
    let base = dir.y.atan2(dir.x);
    let mut sembrar = ((time.elapsed_secs() * 7919.0) as u32).wrapping_mul(2654435761) ^ (estado.cd.to_bits());
    let mut azar = move || {
        sembrar ^= sembrar << 13;
        sembrar ^= sembrar >> 17;
        sembrar ^= sembrar << 5;
        (sembrar % 10_000) as f32 / 10_000.0
    };
    // La escopeta conserva su abanico; el resto: primera bala derecha, la ráfaga abre, correr abre más.
    let dispersion = if def.perdigones > 1 { def.dispersion } else { crate::apuntado::dispersion_efectiva(def.dispersion, estado.rafaga_s, corriendo) };
    for _ in 0..def.perdigones {
        let desvio = (azar() * 2.0 - 1.0) * dispersion.to_radians();
        let a = base + desvio;
        let v = Vec2::new(a.cos(), a.sin()) * def.vel;
        let rayo = def.calibre == Calibre::Energia;
        let (largo, grueso) = if rayo { (0.9, 0.34) } else if def.perdigones > 1 { (0.35, 0.09) } else { (0.8, 0.07) };
        let color = Color::srgb(def.estela[0], def.estela[1], def.estela[2]);
        commands.spawn((
            Bala {
                pos: boca,
                prev: boca,
                vel: v,
                dano: def.dano,
                origen: boca,
                alcance: def.alcance,
                atraviesa: def.atraviesa,
                golpeados: Vec::new(),
                viva: true,
                impacto: def.impacto,
                rayo,
            },
            crate::sim::UnlitSprite,
            Sprite { color, custom_size: Some(Vec2::new(largo, grueso)), ..default() },
            Transform {
                translation: Vec3::new(boca.x, -boca.y, 18.0),
                rotation: Quat::from_rotation_z(-a),
                ..default()
            },
            Name::new("Bala"),
        ));
    }
    // Fogonazo en la boca + sonido + sacudida.
    if let Some(g) = glow.as_ref() {
        commands.spawn((
            Fogonazo(0.06),
            // Un fogonazo ES luz: sin esto `light2d::attach_ground_shadows` le colgaba una sombra proyectada
            // (el sprite mide 2 m y está sobre la capa Y-sort) — las manchas negras en el suelo al disparar.
            crate::sim::UnlitSprite,
            Sprite {
                image: g.0.clone(),
                color: Color::srgba(def.estela[0], def.estela[1], (def.estela[2] + 0.2).min(1.0), 0.9),
                custom_size: Some(Vec2::splat(if def.perdigones > 1 { 3.0 } else { 2.0 })),
                ..default()
            },
            Transform::from_xyz(boca.x, -boca.y, 18.5),
            Name::new("Fogonazo"),
        ));
    }
    sfx.send(PlaySfxAt(def.sfx, Vec2::new(pos.x, -pos.y)));
    shake.send(CameraShakeEvent { trauma: def.retroceso });
}

fn iniciar_recarga(estado: &mut EstadoArmas, def: &ArmaDef, item: &str, inv: &PlayerInventory) {
    if estado.recarga.is_some() {
        return;
    }
    let dentro = estado.cargadas.get(item).copied().unwrap_or(def.cargador);
    if dentro >= def.cargador {
        return;
    }
    if inv.qty(def.calibre.item()) == 0 {
        estado.aviso = Some((format!("SIN {}", def.calibre.etiqueta().to_uppercase()), 1.6));
        return;
    }
    let total = duracion_recarga(def, def.cargador - dentro);
    estado.recarga = Some(Recarga { item: item.to_string(), t: 0.0, total, fase: 0 });
}

#[allow(clippy::too_many_arguments)]
fn recargar(
    time: Res<Time>,
    equipped: Res<EquippedItem>,
    keys: Res<ButtonInput<KeyCode>>,
    acciones: Res<TouchActions>,
    gameplay_block: Res<crate::game_ui::UiGameplayBlock>,
    mut estado: ResMut<EstadoArmas>,
    mut inv: ResMut<PlayerInventory>,
    mut inv_ui: ResMut<crate::hud::inventory_ui::InventoryUiState>,
    mut persist: ResMut<InventoryPersistState>,
    player: Query<&Transform, With<Player>>,
    mut sfx: EventWriter<PlaySfxAt>,
    conduccion: Res<crate::vehiculos::Conduccion>,
) {
    let Some((def, _)) = parsear(&equipped.item_id) else { return };
    let Ok(ptf) = player.get_single() else { return };
    let ppos = Vec2::new(ptf.translation.x, ptf.translation.y);
    // Pedido de recarga: tecla R o el botón USAR (que dice RECARGAR).
    if !gameplay_block.blocked && (keys.just_pressed(KeyCode::KeyR) || (acciones.use_edge && !conduccion.boton_activo())) {
        iniciar_recarga(&mut estado, def, &equipped.item_id, &inv);
    }
    let dt = time.delta_secs();
    let Some(r) = estado.recarga.as_mut() else { return };
    if r.item != equipped.item_id {
        estado.recarga = None;
        return;
    }
    let antes = r.t;
    r.t += dt;
    let frac = r.t / r.total.max(0.01);
    if r.fase == 0 {
        sfx.send(PlaySfxAt(Sfx::ReloadOut, ppos));
        r.fase = 1;
    }
    if r.fase == 1 && frac >= 0.55 {
        sfx.send(PlaySfxAt(Sfx::ReloadIn, ppos));
        r.fase = 2;
    }
    let _ = antes;
    if r.t < r.total {
        return;
    }
    // Listo: se mueven las balas de la mochila al arma.
    let item = r.item.clone();
    let ammo = def.calibre.item();
    let dentro = estado.cargadas.get(&item).copied().unwrap_or(0);
    let pasan = (def.cargador - dentro.min(def.cargador)).min(inv.qty(ammo));
    if pasan > 0 {
        crate::hud::inventory_ui::remove_from_slots(&mut inv_ui, ammo, pasan);
        crate::hud::inventory_ui::flush_slots_to_inventory(&inv_ui, &mut inv);
        persist.mark_dirty();
        estado.cargadas.insert(item, dentro + pasan);
    }
    estado.recarga = None;
    sfx.send(PlaySfxAt(Sfx::ReloadBolt, ppos));
}

// ---------------------------------------------------------------------------------------------------------------------
// Balas
// ---------------------------------------------------------------------------------------------------------------------

#[allow(clippy::too_many_arguments)]
fn mover_balas(
    mut commands: Commands,
    time: Res<Time>,
    ctx: Res<WorldContextRuntime>,
    planet: Res<ActivePlanet>,
    tiles: Res<TileWorld>,
    walls: Res<BuiltWalls>,
    mut sfx: EventWriter<PlaySfxAt>,
    mut q: Query<(&mut Bala, &mut Transform)>,
) {
    let dt = time.delta_secs().min(0.05);
    let body = active_surface_body(&ctx, &planet);
    for (mut b, mut tf) in &mut q {
        if !b.viva {
            continue;
        }
        let paso = b.vel * dt;
        b.prev = b.pos;
        let destino = b.pos + paso;
        // ¿Choca con una pared o un bloque? Se marcha el tramo a media celda.
        if let Some(punto) = choque_con_el_mundo(b.pos, destino, body, tiles.z, &tiles, &walls) {
            b.pos = punto;
            b.viva = false;
            sfx.send(PlaySfxAt(Sfx::BulletWall, Vec2::new(punto.x, -punto.y)));
            chispa_de_impacto(&mut commands, punto, Color::srgb(1.0, 0.85, 0.5));
            tf.translation = Vec3::new(punto.x, -punto.y, 18.0);
            continue;
        }
        b.pos = destino;
        if b.pos.distance(b.origen) >= b.alcance {
            b.viva = false;
        }
        tf.translation = Vec3::new(b.pos.x, -b.pos.y, 18.0);
    }
}

/// Primer punto del tramo `a → b` donde la bala se topa con algo macizo; `None` si el camino está limpio.
fn choque_con_el_mundo(a: Vec2, b: Vec2, body: ha_shared::CelestialBody, z: i32, tiles: &TileWorld, walls: &BuiltWalls) -> Option<Vec2> {
    let delta = b - a;
    let dist = delta.length();
    if dist < 1e-4 {
        return None;
    }
    let pasos = ((dist / (TILE_M * 0.5)).ceil() as i32).clamp(1, 12);
    let celda = |p: Vec2| ((p.x / TILE_M).floor() as i64, (p.y / TILE_M).floor() as i64);
    let mut prev = celda(a);
    for i in 1..=pasos {
        let p = a + delta * (i as f32 / pasos as f32);
        let c = celda(p);
        if c == prev {
            continue;
        }
        if c.0 != prev.0 && walls.blocks(body, z, c.0.max(prev.0), prev.1, WallAxis::V) {
            return Some(p);
        }
        if c.1 != prev.1 && walls.blocks(body, z, c.0, c.1.max(prev.1), WallAxis::H) {
            return Some(p);
        }
        let t = tiles.tile_at(body, c.0, c.1);
        if t.solid() && t != ha_shared::topdown::Tile::WaterDeep {
            return Some(p);
        }
        prev = c;
    }
    None
}

/// ¿Hay camino limpio de `a` a `b`? `choque_con_el_mundo` marcha a lo sumo 12 pasos (está hecho para el tramo de un
/// cuadro); para una línea de vista de 30 m se parte en tramos de 5 m, así ningún muro queda entre dos muestras.
fn a_la_vista(a: Vec2, b: Vec2, body: ha_shared::CelestialBody, z: i32, tiles: &TileWorld, walls: &BuiltWalls) -> bool {
    let tramos = ((a.distance(b) / 5.0).ceil() as i32).max(1);
    (0..tramos).all(|k| {
        let p = a.lerp(b, k as f32 / tramos as f32);
        let q = a.lerp(b, (k + 1) as f32 / tramos as f32);
        choque_con_el_mundo(p, q, body, z, tiles, walls).is_none()
    })
}

/// Chispa de sangre/polvo en el punto de impacto (la usan los módulos con blancos).
pub fn chispa_de_impacto(commands: &mut Commands, en: Vec2, color: Color) {
    for k in 0..4 {
        let a = k as f32 * 1.7 + en.x * 3.1;
        commands.spawn((
            Chispa { vel: Vec2::new(a.cos(), a.sin()) * 2.2, vida: 0.22 },
            crate::sim::UnlitSprite,
            Sprite { color, custom_size: Some(Vec2::splat(0.14)), ..default() },
            Transform::from_xyz(en.x, -en.y, 18.2),
        ));
    }
}

fn limpiar_balas(mut commands: Commands, q: Query<(Entity, &Bala)>) {
    for (e, b) in &q {
        if !b.viva {
            commands.entity(e).despawn();
        }
    }
}

/// Fogonazos y chispas: viven una fracción de segundo.
fn efectos(
    mut commands: Commands,
    time: Res<Time>,
    mut chispas_q: Query<(Entity, &mut Chispa, &mut Transform, &mut Sprite), Without<Fogonazo>>,
    mut fogonazos: Query<(Entity, &mut Fogonazo, &mut Sprite), Without<Chispa>>,
) {
    let dt = time.delta_secs();
    for (e, mut c, mut tf, mut s) in &mut chispas_q {
        c.vida -= dt;
        if c.vida <= 0.0 {
            commands.entity(e).despawn();
            continue;
        }
        tf.translation.x += c.vel.x * dt;
        tf.translation.y -= c.vel.y * dt;
        s.color = s.color.with_alpha((c.vida / 0.22).clamp(0.0, 1.0));
    }
    for (e, mut f, mut s) in &mut fogonazos {
        f.0 -= dt;
        if f.0 <= 0.0 {
            commands.entity(e).despawn();
        } else {
            s.color = s.color.with_alpha((f.0 / 0.06).clamp(0.0, 1.0) * 0.9);
        }
    }
}

// ---------------------------------------------------------------------------------------------------------------------
// Lo que se ve: el arma en la mano, la mira y el contador de balas
// ---------------------------------------------------------------------------------------------------------------------

#[allow(clippy::too_many_arguments)]
fn visual_del_arma(
    mut commands: Commands,
    equipped: Res<EquippedItem>,
    art: Res<crate::art2d::ArtLibrary>,
    cursor: Res<AimCursor>,
    stick: Res<crate::input::AimStick>,
    conduccion: Res<crate::vehiculos::Conduccion>,
    estado: Res<EstadoArmas>,
    player: Query<(&Transform, &PlayerVisual), (With<Player>, Without<ArmaVisual>, Without<MiraDeArma>)>,
    mut visual: Query<(Entity, &mut Transform, &mut Sprite, &mut Visibility, &Name), (With<ArmaVisual>, Without<MiraDeArma>)>,
    mano: Res<crate::manos::ManoDelJugador>,
    mut mira: Query<(Entity, &mut Transform, &mut Sprite), (With<MiraDeArma>, Without<ArmaVisual>)>,
    mut ultimo: Local<String>,
) {
    let armado = parsear(&equipped.item_id).is_some();
    let Ok((ptf, pv)) = player.get_single() else { return };
    if !armado {
        for (e, ..) in &visual {
            commands.entity(e).despawn_recursive();
        }
        for (e, ..) in &mira {
            commands.entity(e).despawn_recursive();
        }
        ultimo.clear();
        return;
    }
    let pos = Vec2::new(ptf.translation.x, -ptf.translation.y);
    let apuntando = cursor.targeting() && cursor.active;
    // El arma apunta al blanco del apuntado asistido si lo hay; si no, al dedo; si no, al frente.
    let dir = match estado.mira_en {
        _ if stick.activo && stick.tiene_dir => stick.dir,
        Some((p, _)) => (p - pos).normalize_or_zero(),
        None if apuntando => (cursor.world - pos).normalize_or_zero(),
        None => Vec2::ZERO,
    };
    let dir = if dir == Vec2::ZERO { rumbo_del_jugador(pv) } else { dir };
    // Si cambió el arma (o no existe todavía) se vuelve a crear con su sprite.
    if *ultimo != equipped.item_id || visual.is_empty() {
        for (e, ..) in &visual {
            commands.entity(e).despawn_recursive();
        }
        *ultimo = equipped.item_id.clone();
        let clave = format!("items/{}", equipped.item_id);
        // El tamaño y el punto de agarre salen del dibujo (`manos.json`): el arma se empuña por su
        // empuñadura, no por el centro.
        if let (Some(ag), Some(_)) = (crate::manos::objeto(&clave), art.get(&clave)) {
            let tam = ag.tam_en_mundo();
            if let Some((mut sprite, _)) = art.world_sprite(&clave, tam) {
                sprite.custom_size = Some(tam);
                sprite.anchor = bevy::sprite::Anchor::Custom(Vec2::new(ag.agarre[0] / ag.tam[0] - 0.5, 0.5 - ag.agarre[1] / ag.tam[1]));
                commands.spawn((ArmaVisual, crate::light2d::ShadowChecked, sprite, Transform::from_xyz(pos.x, -pos.y, 5.0), Visibility::Hidden, Name::new("ArmaVisual")));
            }
        }
    }
    let ang = dir.y.atan2(dir.x); // en el mundo (x, z)
    // En pantalla el eje y es -z: el ángulo de render es -ang.
    let izquierda = dir.x < 0.0;
    let agarre = crate::manos::objeto(&format!("items/{}", equipped.item_id));
    for (_, mut tf, mut s, mut vis, _) in &mut visual {
        // El arma se agarra con la mano que dibuja la hoja (`manos.rs`): sin mano a la vista (nadando, comiendo,
        // sentado) no se dibuja, y mirando hacia arriba queda detrás del cuerpo.
        if !mano.vigente || conduccion.activa() {
            *vis = Visibility::Hidden;
            continue;
        }
        *vis = Visibility::Inherited;
        let retro = dir * -(estado.kick * 0.35);
        tf.translation = Vec3::new(
            mano.pos.x + retro.x,
            mano.pos.y - retro.y,
            if mano.detras { mano.z - crate::manos::DZ } else { mano.z + crate::manos::DZ },
        );
        // Mirando a la izquierda el sprite se espeja (en vertical) para no quedar patas arriba; el agarre
        // se espeja con él.
        tf.rotation = Quat::from_rotation_z(-ang);
        s.flip_y = izquierda;
        if let Some(ag) = agarre {
            let y = 0.5 - ag.agarre[1] / ag.tam[1];
            s.anchor = bevy::sprite::Anchor::Custom(Vec2::new(ag.agarre[0] / ag.tam[0] - 0.5, if izquierda { -y } else { y }));
        }
    }
    // La mira: ENCIMA del infectado elegido por el apuntado asistido (el dedo lo tapa; la mira no), roja si la bala
    // le va a pegar de lleno. Sin blanco, un aro blanco donde está el dedo.
    let (donde, color, lado) = match estado.mira_en {
        Some((p, true)) => (p, Color::srgba(1.0, 0.25, 0.2, 0.95), 1.05),
        Some((p, false)) => (p, Color::srgba(1.0, 0.85, 0.75, 0.9), 1.05),
        None => (cursor.world, Color::srgba(1.0, 1.0, 1.0, 0.85), 0.8),
    };
    if apuntando || estado.mira_en.is_some() {
        if mira.is_empty() {
            if let Some((sprite, _)) = art.world_sprite("ui/crosshair", Vec2::splat(0.8)) {
                commands.spawn((MiraDeArma, crate::sim::UnlitSprite, sprite, Transform::from_xyz(donde.x, -donde.y, 17.0), Name::new("MiraDeArma")));
            }
        }
        for (_, mut tf, mut s) in &mut mira {
            tf.translation = Vec3::new(donde.x, -donde.y, 17.0);
            tf.scale = Vec3::splat(lado / 0.8);
            s.color = color;
        }
    } else {
        for (e, ..) in &mira {
            commands.entity(e).despawn_recursive();
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn hud_armas(
    mut commands: Commands,
    equipped: Res<EquippedItem>,
    estado: Res<EstadoArmas>,
    inv: Res<PlayerInventory>,
    layout: Res<crate::input::TouchLayout>,
    mut q: Query<(&mut Text, &mut Visibility, &mut TextColor), With<HudArmas>>,
) {
    if q.is_empty() {
        let k = crate::ui_pack::k(&layout);
        commands.spawn((
            Text::new(""),
            TextFont { font_size: (22.0 * k).clamp(15.0, 26.0), ..default() },
            TextColor(Color::srgb(1.0, 0.92, 0.7)),
            Node {
                position_type: PositionType::Absolute,
                bottom: Val::Px((118.0 * k).max(96.0)),
                left: Val::Percent(50.0),
                margin: UiRect::left(Val::Px(-190.0)),
                width: Val::Px(380.0),
                justify_content: JustifyContent::Center,
                ..default()
            },
            TextLayout::new_with_justify(JustifyText::Center),
            Visibility::Hidden,
            GlobalZIndex(41),
            HudArmas,
            Name::new("HudArmas"),
        ));
        return;
    }
    let Some((def, _)) = parsear(&equipped.item_id) else {
        for (_, mut v, _) in &mut q {
            *v = Visibility::Hidden;
        }
        return;
    };
    let dentro = estado.cargadas_de(&equipped.item_id).unwrap_or(def.cargador);
    let reserva = inv.qty(def.calibre.item());
    let texto = if let Some(r) = &estado.recarga {
        format!("{} · RECARGANDO {}%", def.nombre, ((r.t / r.total.max(0.01)) * 100.0).min(100.0) as u32)
    } else if let Some((m, _)) = &estado.aviso {
        format!("{} · {m}", def.nombre)
    } else {
        format!("{} · {dentro}/{reserva}", def.nombre)
    };
    let alerta = estado.recarga.is_some() || dentro == 0 || estado.aviso.is_some();
    for (mut t, mut v, mut c) in &mut q {
        if t.0 != texto {
            t.0 = texto.clone();
        }
        *v = Visibility::Inherited;
        c.0 = if alerta { Color::srgb(1.0, 0.55, 0.45) } else { Color::srgb(1.0, 0.92, 0.7) };
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn los_ids_se_parsean_y_los_63_items_existen() {
        assert_eq!(items_de_arma().len(), 9 * 7);
        for it in items_de_arma() {
            assert!(es_arma(it), "{it}");
            assert!(nombre_es(it).is_some());
        }
        assert!(parsear("gun_thompson__115").is_some());
        assert!(parsear("gun_thompson__nada").is_none());
        assert!(parsear("gun_inexistente").is_none());
        assert!(!es_arma("sword_iron"));
    }

    #[test]
    fn cada_calibre_tiene_item_y_las_armas_lo_usan() {
        for c in Calibre::TODOS {
            assert!(es_municion(c.item()));
            assert!(ARMAS.iter().any(|a| a.calibre == c), "{c:?} sin arma");
        }
    }

    #[test]
    fn las_armas_tienen_numeros_razonables() {
        for a in &ARMAS {
            assert!(a.cargador >= 6 && a.cadencia > 0.5 && a.vel > 30.0 && a.alcance > 10.0, "{}", a.id);
            // matar a un infectado (12 de vida) lleva entre 2 y 8 disparos de pleno daño de un arma de un proyectil
            if a.perdigones == 1 {
                let tiros = (12.0 / a.dano).ceil();
                // (la pistola de rayos, como la de los zombis, mata de uno: se paga con una munición escasa)
                let minimo = if a.calibre == Calibre::Energia { 1.0 } else { 2.0 };
                assert!((minimo..=8.0).contains(&tiros), "{}: {tiros} tiros", a.id);
            }
            // y todas las armas automáticas vacían el cargador en menos de 8 s
            if a.auto {
                assert!(a.cargador as f32 / a.cadencia < 8.0, "{}", a.id);
            }
        }
        // la escopeta a quemarropa mata de un cartucho
        let esc = ARMAS.iter().find(|a| a.id == "escopeta").unwrap();
        assert!(esc.dano * esc.perdigones as f32 >= 12.0);
    }

    #[test]
    fn el_dano_cae_con_la_distancia_hasta_el_55_por_ciento() {
        assert!((dano_a_distancia(10.0, 5.0, 30.0) - 10.0).abs() < 1e-4);
        assert!((dano_a_distancia(10.0, 30.0, 30.0) - 5.5).abs() < 1e-3);
        assert!(dano_a_distancia(10.0, 20.0, 30.0) < 10.0);
    }

    #[test]
    fn la_escopeta_recarga_por_cartucho() {
        let esc = ARMAS.iter().find(|a| a.id == "escopeta").unwrap();
        assert!(duracion_recarga(esc, 1) < duracion_recarga(esc, 6));
        let pis = ARMAS.iter().find(|a| a.id == "pistola").unwrap();
        assert_eq!(duracion_recarga(pis, 1), duracion_recarga(pis, 8));
    }

    #[test]
    fn el_botin_de_armas_es_estable_y_variado() {
        assert_eq!(arma_al_azar(0.3, 0.9, 0.4), arma_al_azar(0.3, 0.9, 0.4));
        let mut vistas = std::collections::BTreeSet::new();
        for i in 0..200 {
            let r = i as f32 / 200.0;
            vistas.insert(arma_al_azar(r, (r * 7.3) % 1.0, (r * 3.1) % 1.0));
        }
        assert!(vistas.len() > 20, "{}", vistas.len());
        for i in 0..50 {
            let (it, n) = municion_al_azar(i as f32 / 50.0, 0.5);
            assert!(es_municion(it) && n > 0);
        }
    }

    #[test]
    fn la_bala_no_le_pega_dos_veces_al_mismo_y_muere_al_agotar_lo_que_atraviesa() {
        let mut b = Bala {
            pos: Vec2::ZERO, prev: Vec2::ZERO, vel: Vec2::X, dano: 5.0, origen: Vec2::ZERO, alcance: 20.0,
            atraviesa: 2, golpeados: vec![], viva: true, impacto: Sfx::BulletHit, rayo: false,
        };
        let (e1, e2) = (Entity::from_raw(1), Entity::from_raw(2));
        b.registrar(e1);
        assert!(b.viva && b.ya_golpeo(e1) && !b.ya_golpeo(e2));
        b.registrar(e2);
        assert!(!b.viva);
    }

    #[test]
    fn distancia_punto_segmento() {
        let a = Vec2::new(0.0, 0.0);
        let b = Vec2::new(10.0, 0.0);
        assert!((distancia_a_segmento(Vec2::new(5.0, 3.0), a, b) - 3.0).abs() < 1e-4);
        assert!((distancia_a_segmento(Vec2::new(-4.0, 3.0), a, b) - 5.0).abs() < 1e-4);
        assert!((distancia_a_segmento(Vec2::new(1.0, 1.0), a, a) - 2f32.sqrt()).abs() < 1e-4);
    }
}
