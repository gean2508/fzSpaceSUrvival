//! **PANDEMIA** (5 oct 2026): la mordida de un infectado contagia. Un vecino mordido puede quedar **contagiado**: unos
//! segundos de incubación (se queja, el cartel lo avisa) y se convierte en infectado, con lo que llevaba encima, y ahora
//! persigue a los suyos. El que muere de una mordida también puede levantarse. Mientras haya noche y vecinos, el brote crece;
//! de día los infectados se queman (`infectados::amanecer`), así que la pandemia da la vuelta con el sol, no sin fin.
//!
//! * **Sin estado nuevo en el vecino**: el contagio es un componente (`Contagiado`) pegado a la entidad del `Npc`; quitar la
//!   entidad lo borra. Los contadores (`Pandemia`) son sólo estadística para la consola/causal.
//! * **El jugador también**: una mordida tiene su chance de dejarte con **fiebre** (`Infeccion`): pierde un poco de vida por
//!   segundo hasta que se le pasa (75 s) — nunca lo mata (se detiene en 1), igual que el sangrado de `estados`.
//! * Todo corre sobre lo que ya había: la mordida (`barrio::morder_vecinos`, `infectados::mover_y_morder`), la baja del vecino
//!   (`society::despedir_npc`) y el alta del infectado (`infectados::levantar`, el mismo cuerpo que el que sale de noche).

use bevy::prelude::*;

use crate::civilizacion::Civ;
use crate::dialog::{say, SpeechQueue};
use crate::game_phase::GamePhase;
use crate::sim::{Player, SimState};
use crate::society::{despedir_npc, Npc, Society};

/// Segundos de incubación de un vecino contagiado.
pub const INCUBACION: f32 = 25.0;
/// Probabilidad (%) de que una mordida que no mata contagie a un vecino.
pub const PROB_CONTAGIO_VECINO: u32 = 55;
/// Probabilidad (%) de que el vecino muerto de una mordida se levante.
pub const PROB_LEVANTARSE: u32 = 60;
/// Probabilidad (%) de que una mordida le dé fiebre al jugador, y cuánto dura.
pub const PROB_FIEBRE: u32 = 30;
pub const FIEBRE_S: f32 = 75.0;
const FIEBRE_POR_SEG: f32 = 0.22;

#[derive(Component)]
pub struct Contagiado {
    pub t: f32,
    avisado: bool,
}

impl Contagiado {
    pub fn nuevo() -> Self {
        Self { t: INCUBACION, avisado: false }
    }
}

#[derive(Resource, Default, Debug)]
pub struct Pandemia {
    pub contagios: u32,
    pub convertidos: u32,
    pub levantados: u32,
}

/// La fiebre del jugador: segundos que le quedan.
#[derive(Resource, Default)]
pub struct Infeccion {
    pub t: f32,
    avisada: bool,
}

impl Infeccion {
    /// Una mordida: con [`PROB_FIEBRE`] de chance empieza (o renueva) la fiebre.
    pub fn mordida(&mut self, tirada: u32) {
        if tirada % 100 < PROB_FIEBRE && self.t <= 0.0 {
            self.t = FIEBRE_S;
            self.avisada = false;
        }
    }
}

pub struct PandemiaPlugin;

impl Plugin for PandemiaPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Pandemia>()
            .init_resource::<Infeccion>()
            .add_systems(Update, (avanzar_contagio, fiebre).run_if(crate::game_phase::world_active))
            .add_systems(OnExit(GamePhase::Playing), (crate::salir::reiniciar::<Pandemia>, crate::salir::reiniciar::<Infeccion>));
    }
}

#[allow(clippy::too_many_arguments)]
fn avanzar_contagio(
    mut commands: Commands,
    time: Res<Time>,
    art: Res<crate::art2d::ArtLibrary>,
    mut pandemia: ResMut<Pandemia>,
    mut soc: ResMut<Society>,
    mut civ: ResMut<Civ>,
    mut speech: ResMut<SpeechQueue>,
    mut q: Query<(Entity, &Npc, &mut Contagiado)>,
) {
    let dt = time.delta_secs().min(0.2);
    for (e, n, mut c) in &mut q {
        c.t -= dt;
        if !c.avisado && c.t < INCUBACION * 0.55 {
            c.avisado = true;
            say(&mut speech, n.x, n.z, "No me siento bien...", Color::srgb(0.7, 0.95, 0.6));
        }
        if c.t > 0.0 {
            continue;
        }
        let (x, z, uid) = (n.x, n.z, n.uid);
        civ.muerte_de_npc(uid, x, z);
        despedir_npc(&mut commands, &mut soc, e, n);
        crate::infectados::levantar(&mut commands, &art, x, z, uid as u32);
        pandemia.convertidos += 1;
        crate::causal::registrar("pandemia", format!("un vecino contagiado se convirtió en infectado en ({x:.0},{z:.0}); van {}", pandemia.convertidos));
    }
}

fn fiebre(
    time: Res<Time>,
    mut inf: ResMut<Infeccion>,
    mut speech: ResMut<SpeechQueue>,
    mut player: Query<(&Transform, &mut SimState), With<Player>>,
) {
    if inf.t <= 0.0 {
        return;
    }
    let Ok((tf, mut sim)) = player.get_single_mut() else { return };
    let dt = time.delta_secs().min(0.2);
    if !inf.avisada {
        inf.avisada = true;
        say(&mut speech, tf.translation.x, -tf.translation.y, "Me mordieron... tengo fiebre.", Color::srgb(0.7, 0.95, 0.6));
    }
    inf.t -= dt;
    sim.hp = (sim.hp - FIEBRE_POR_SEG * dt).max(1.0);
    if inf.t <= 0.0 {
        say(&mut speech, tf.translation.x, -tf.translation.y, "Se me pasó la fiebre.", Color::srgb(0.7, 0.95, 0.6));
    }
}
