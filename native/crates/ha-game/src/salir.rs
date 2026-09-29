//! **Volver al menú principal** desde una partida (pedido del usuario, 29 sep 2026).
//!
//! El juego nunca había salido de `Playing`: todo el mundo se armaba una vez y
//! se quedaba. Salir bien es tres pasos, en orden:
//!
//! 1. **Guardar todo.** Cada módulo ya vuelca su estado al disco cuando la app
//!    se PAUSA (Android puede matar el proceso en segundo plano); se simula una
//!    pausa de unos cuadros y todos guardan por el mismo camino de siempre.
//! 2. **Despejar el mundo.** Se despawnean las entidades del MUNDO —las que
//!    tienen `Transform` y no son interfaz, cámara ni ventana— que no existían
//!    antes de entrar a la partida. La interfaz se queda (se esconde sola fuera
//!    de `Playing`).
//! 3. **Reiniciar lo que recuerda el mundo.** Cada módulo con estado atado a lo
//!    que spawneó (banderas de "ya lo armé", mapas de entidades, cargas perezosas)
//!    se reinicia con [`reiniciar`] al salir de `Playing`. Lo persistente ya se
//!    carga al entrar a la próxima partida (`OnEnter(Loading)`).

use std::collections::HashSet;

use bevy::prelude::*;

use crate::game_phase::GamePhase;

/// Pedido pendiente de salir (lo pone el botón del menú).
#[derive(Resource, Default)]
pub struct SalidaAlMenu {
    pub pedida: bool,
    cuadros: u8,
}

/// Las entidades que existían ANTES de entrar a la partida: ésas no se tocan.
#[derive(Resource, Default)]
struct DeAntes(HashSet<Entity>);

/// Reinicia un recurso a su valor por defecto. Se registra por módulo:
/// `.add_systems(OnExit(GamePhase::Playing), crate::salir::reiniciar::<X>)`.
pub fn reiniciar<R: Resource + Default>(mut r: ResMut<R>) {
    *r = R::default();
}

pub struct SalirPlugin;

impl Plugin for SalirPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<SalidaAlMenu>()
            .init_resource::<DeAntes>()
            .add_systems(OnExit(GamePhase::MainMenu), anotar_lo_de_antes)
            .add_systems(OnExit(GamePhase::CharacterCreate), anotar_lo_de_antes)
            .add_systems(OnExit(GamePhase::Splash), anotar_lo_de_antes)
            .add_systems(Update, salir.run_if(in_state(GamePhase::Playing)))
            .add_systems(OnExit(GamePhase::Playing), despejar_el_mundo);
    }
}

fn anotar_lo_de_antes(todas: Query<Entity>, mut antes: ResMut<DeAntes>) {
    antes.0 = todas.iter().collect();
}

fn salir(mut pedido: ResMut<SalidaAlMenu>, mut next: ResMut<NextState<GamePhase>>) {
    if !pedido.pedida {
        return;
    }
    // Unos cuadros de "pausa" para que todos guarden por su camino de siempre.
    if pedido.cuadros == 0 {
        crate::platform::set_app_paused(true);
    }
    pedido.cuadros += 1;
    if pedido.cuadros < 4 {
        return;
    }
    crate::platform::set_app_paused(false);
    pedido.pedida = false;
    pedido.cuadros = 0;
    eprintln!("ha-salir: partida guardada, volviendo al menú principal");
    next.set(GamePhase::MainMenu);
}

#[allow(clippy::type_complexity)]
fn despejar_el_mundo(
    mut commands: Commands,
    antes: Res<DeAntes>,
    mundo: Query<
        Entity,
        (
            With<Transform>,
            Without<Node>,
            Without<Camera>,
            Without<Window>,
            Without<Parent>,
        ),
    >,
) {
    let mut n = 0;
    for e in &mundo {
        if !antes.0.contains(&e) {
            commands.entity(e).despawn_recursive();
            n += 1;
        }
    }
    eprintln!("ha-salir: {n} entidades del mundo despejadas");
}
