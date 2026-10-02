//! **Heridas** (Project Zomboid): un golpe fuerte puede abrirte una herida que
//! sangra. Es una consecuencia, no un castigo: se cura con lo que llevás.
//!
//! * Un golpe de ≥ `GOLPE_FUERTE` de vida (mordida de infectado, de un depredador)
//!   tiene `PROB_SANGRADO` de dejarte **sangrando** `DURACION` segundos: `-0,6`
//!   de vida por segundo, sin matarte (se detiene en 1).
//! * Si llevás **lana**, a los `TIEMPO_VENDAJE` segundos te vendás sola y gastás
//!   una. Sin lana, esperás a que cierre — o corrés a buscar una oveja.
//! * Se avisa con un cartel de una línea; no hay botón: no le roba toques al mundo
//!   ni hay que aprenderse un menú para sobrevivir.
//!
//! Lee y escribe sólo `SimState.hp` y la mochila: no toca los sistemas que dañan.

use bevy::prelude::*;

use crate::game_phase::GamePhase;
use crate::i18n::{idioma_actual, Idioma};
use crate::inventory_persist::{InventoryPersistState, PlayerInventory};
use crate::sim::{Player, SimState};

const GOLPE_FUERTE: f32 = 5.0;
const PROB_SANGRADO: f32 = 0.35;
const DURACION: f32 = 20.0;
const PERDIDA_POR_SEG: f32 = 0.6;
const TIEMPO_VENDAJE: f32 = 3.0;

const FATIGA_POR_SEG: f32 = 100.0 / 1200.0; // 20 min despierto = agotado
const DESCANSO_POR_SEG: f32 = 100.0 / 150.0; // 2,5 min quieto = descansado
const QUIETO_PARA_DESCANSAR: f32 = 4.0;
const CANSADO: f32 = 80.0;
const AGOTADO: f32 = 100.0;
const AGOTADO_PERDIDA: f32 = 0.1;
const CANSADO_HAMBRE_EXTRA: f32 = 0.02;

/// Un paso de fatiga: devuelve la nueva (0..100) según si te moviste.
pub fn paso_fatiga(f: f32, quieto_seg: f32, dt: f32) -> f32 {
    let d = if quieto_seg >= QUIETO_PARA_DESCANSAR { -DESCANSO_POR_SEG } else { FATIGA_POR_SEG };
    (f + d * dt).clamp(0.0, 100.0)
}

const FRIO_LUZ: f32 = 0.25;
const FRIO_ARMADURA: f32 = 0.15;
const FRIO_PERDIDA: f32 = 0.12;
const FRIO_PISO_HP: f32 = 25.0;

/// ¿Pasás frío? De noche, a la intemperie (superficie de la Tierra), sin abrigo ni luz.
pub fn pasa_frio(luz_del_dia: f32, armadura: f32, tiene_luz: bool, en_tierra: bool) -> bool {
    en_tierra && luz_del_dia < FRIO_LUZ && armadura < FRIO_ARMADURA && !tiene_luz
}

#[derive(Resource, Default)]
pub struct Heridas {
    pub frio: bool,
    /// Cansancio 0..100: sube despierto, baja descansando quieto.
    pub fatiga: f32,
    quieto: f32,
    px: f32,
    pz: f32,
    prev_hp: Option<f32>,
    /// Segundos de sangrado que quedan (0 = sana).
    pub sangrado: f32,
    desde: f32,
    azar: u64,
}

#[derive(Component)]
struct CartelHeridas;

pub struct EstadosPlugin;

impl Plugin for EstadosPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Heridas>()
            .add_systems(Update, (tick_heridas, pintar).chain().run_if(in_state(GamePhase::Playing)))
            .add_systems(OnExit(GamePhase::Playing), salir);
    }
}

fn salir(mut commands: Commands, mut h: ResMut<Heridas>, q: Query<Entity, With<CartelHeridas>>) {
    *h = Heridas::default();
    for e in &q {
        commands.entity(e).despawn_recursive();
    }
}

/// ¿Esta caída de vida fue un golpe que puede abrir una herida?
pub fn abre_herida(antes: f32, ahora: f32, tirada: f32) -> bool {
    antes - ahora >= GOLPE_FUERTE && tirada < PROB_SANGRADO
}

fn tick_heridas(
    time: Res<Time>,
    mut h: ResMut<Heridas>,
    mut sim: Query<&mut SimState, With<Player>>,
    player_tf: Query<&Transform, With<Player>>,
    mut inv: ResMut<PlayerInventory>,
    mut persist: ResMut<InventoryPersistState>,
    day: Res<crate::sky2d::DayClock>,
    ctx: Res<crate::scene::WorldContextRuntime>,
) {
    let Ok(mut s) = sim.get_single_mut() else { return };
    let Ok(pt) = player_tf.get_single() else { return };
    let dt = time.delta_secs().min(0.1);
    h.azar = h.azar.wrapping_mul(6364136223846793005).wrapping_add(time.elapsed().as_micros() as u64 | 1);
    let tirada = ((h.azar >> 40) as f32) / (1u64 << 24) as f32;
    if let Some(antes) = h.prev_hp {
        if h.sangrado <= 0.0 && abre_herida(antes, s.hp, tirada) {
            h.sangrado = DURACION;
            h.desde = 0.0;
        }
    }
    if h.sangrado > 0.0 {
        h.sangrado = (h.sangrado - dt).max(0.0);
        h.desde += dt;
        s.hp = (s.hp - PERDIDA_POR_SEG * dt).max(1.0_f32.min(s.hp));
        if h.desde >= TIEMPO_VENDAJE && inv.qty("wool") > 0 {
            inv.remove("wool", 1);
            persist.mark_dirty();
            h.sangrado = 0.0;
        }
    }
    // FATIGA: sube despierto; quedarte quieto unos segundos te hace descansar. Cansado, tenés
    // más hambre; agotado, perdés vida despacio (piso 25), como el frío.
    let (px, pz) = (pt.translation.x, -pt.translation.y);
    let vel = ((px - h.px).abs() + (pz - h.pz).abs()) / dt.max(1e-3);
    h.px = px;
    h.pz = pz;
    h.quieto = if vel < 0.3 { h.quieto + dt } else { 0.0 };
    h.fatiga = paso_fatiga(h.fatiga, h.quieto, dt);
    if h.fatiga >= CANSADO {
        s.survival.hunger = (s.survival.hunger - CANSADO_HAMBRE_EXTRA * dt).max(0.0);
    }
    if h.fatiga >= AGOTADO && s.hp > FRIO_PISO_HP {
        s.hp = (s.hp - AGOTADO_PERDIDA * dt).max(FRIO_PISO_HP);
    }
    // FRÍO: una molestia, no una sentencia — baja la vida despacio y nunca bajo el piso.
    let tiene_luz = inv.qty("torch") > 0 || inv.qty("lantern") > 0;
    h.frio = pasa_frio(day.daylight(), s.armadura, tiene_luz, matches!(ctx.active, ha_shared::WorldContext::Overworld));
    if h.frio && s.hp > FRIO_PISO_HP {
        s.hp = (s.hp - FRIO_PERDIDA * dt).max(FRIO_PISO_HP);
    }
    h.prev_hp = Some(s.hp);
}

fn pintar(
    mut commands: Commands,
    h: Res<Heridas>,
    inv: Res<PlayerInventory>,
    mut q: Query<(&mut Text, &mut Visibility), With<CartelHeridas>>,
) {
    if q.is_empty() {
        commands.spawn((
            Text::new(""),
            TextFont { font_size: 13.0, ..default() },
            TextColor(Color::srgb(1.0, 0.4, 0.4)),
            Node {
                position_type: PositionType::Absolute,
                top: Val::Px(130.0),
                left: Val::Percent(50.0),
                margin: UiRect::left(Val::Px(-170.0)),
                width: Val::Px(340.0),
                justify_content: JustifyContent::Center,
                ..default()
            },
            TextLayout::new_with_justify(JustifyText::Center),
            Visibility::Hidden,
            GlobalZIndex(40),
            CartelHeridas,
            Name::new("CartelHeridas"),
        ));
        return;
    }
    let en = idioma_actual() == Idioma::En;
    for (mut t, mut v) in &mut q {
        let mut partes: Vec<String> = Vec::new();
        if h.sangrado > 0.0 {
            let tiene = inv.qty("wool") > 0;
            partes.push(match (en, tiene) {
                (false, true) => format!("SANGRANDO · te vendás con lana ({:.0}s)", h.sangrado),
                (false, false) => format!("SANGRANDO {:.0}s · buscá lana", h.sangrado),
                (true, true) => format!("BLEEDING · bandaging with wool ({:.0}s)", h.sangrado),
                (true, false) => format!("BLEEDING {:.0}s · find wool", h.sangrado),
            });
        }
        if h.frio {
            partes.push(if en { "COLD · coat or torch".into() } else { "FRÍO · abrigo o antorcha".into() });
        }
        if h.fatiga >= AGOTADO {
            partes.push(if en { "EXHAUSTED · stand still to rest".into() } else { "AGOTADO · quedate quieto a descansar".into() });
        } else if h.fatiga >= CANSADO {
            partes.push(if en { "TIRED · rest soon".into() } else { "CANSADO · descansá pronto".into() });
        }
        if partes.is_empty() {
            *v = Visibility::Hidden;
        } else {
            let txt = partes.join("  ·  ");
            if t.0 != txt {
                t.0 = txt;
            }
            *v = Visibility::Inherited;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn solo_los_golpes_fuertes_y_con_suerte_abren_herida() {
        assert!(abre_herida(100.0, 93.0, 0.1));
        assert!(!abre_herida(100.0, 93.0, 0.9), "sin suerte no sangra");
        assert!(!abre_herida(100.0, 99.0, 0.0), "un rasguño no sangra");
        // El hambre le saca poca vida por cuadro: nunca llega al umbral.
        assert!(!abre_herida(100.0, 99.99, 0.0));
    }

    #[test]
    fn el_frio_pide_noche_intemperie_sin_abrigo_ni_luz() {
        assert!(pasa_frio(0.0, 0.0, false, true));
        assert!(!pasa_frio(0.9, 0.0, false, true), "de día no");
        assert!(!pasa_frio(0.0, 0.3, false, true), "con abrigo no");
        assert!(!pasa_frio(0.0, 0.0, true, true), "con antorcha no");
        assert!(!pasa_frio(0.0, 0.0, false, false), "fuera de la Tierra no");
    }

    #[test]
    fn la_fatiga_sube_despierto_y_baja_quieto() {
        let f = paso_fatiga(0.0, 0.0, 600.0);
        assert!((f - 50.0).abs() < 0.01, "{f}");
        assert!(paso_fatiga(f, 10.0, 60.0) < f, "quieto descansa");
        assert!(paso_fatiga(f, 1.0, 60.0) > f, "recién parado todavía no");
        assert_eq!(paso_fatiga(99.0, 0.0, 1e6), 100.0);
        assert_eq!(paso_fatiga(1.0, 9.0, 1e6), 0.0);
    }

    #[test]
    fn el_cartel_dibuja() {
        for t in ["AGOTADO · quedate quieto a descansar", "CANSADO · descansá pronto", "FRÍO · abrigo o antorcha", "EXHAUSTED · stand still to rest", "FRÍO · abrigate o llevá una antorcha", "COLD · wear a coat or carry a torch", "SANGRANDO · te vendás con lana (20s)", "SANGRANDO 20s · buscá lana", "BLEEDING · bandaging with wool (20s)"] {
            assert!(t.chars().all(crate::fuente::dibujable), "{t}");
        }
    }
}
