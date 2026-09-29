//! **Las puertas se abren y se cierran cuando VOS querés.**
//!
//! Hasta el 28 sep 2026 toda puerta del juego —la de una celda de obra
//! (`building::DoorArt`), la de un borde construido (`walls::WallDoor`) y la
//! pintada en una fachada (`building::PuertaDeFachada`)— se abría sola al
//! acercarte y se cerraba al irte. El usuario pidió lo de Project Zomboid: la
//! puerta cambia de estado al interactuar con ella, y **queda como la dejaste**.
//!
//! Este módulo sólo decide A QUÉ puerta le hablaste; el ciclo cerrada →
//! abriendo → abierta → cerrando sigue viviendo en el sistema de cada tipo, que
//! lee [`Abrible::pedido`] en vez de medir la distancia.
//!
//! Cómo se pide:
//! - **Tocar / clickear la puerta** (o a menos de [`HOLGURA_M`] de ella).
//! - **Tecla E** en la compu: la puerta más cercana al alcance.
//!
//! En los dos casos el jugador tiene que estar a menos de [`ALCANCE_M`]: tocar una
//! puerta del otro lado de la calle no la abre.

use bevy::input::touch::{TouchInput, TouchPhase};
use bevy::prelude::*;
use bevy::window::PrimaryWindow;

use crate::camera::GameCamera;
use crate::input::{touch_blocked, touch_in_move_zone, TouchBlockRects, TouchLayout};
use crate::sim::Player;

/// Hasta dónde llega la mano, en metros de render desde el pie del jugador al
/// centro del batiente. Una puerta de fachada mide hasta 3 m de alto y su centro
/// queda a 1,5 m del piso: con menos de esto no se alcanzaba la propia puerta
/// estando parado en el umbral.
const ALCANCE_M: f32 = 3.2;
/// Cuánto se puede errar el dedo alrededor del dibujo de la puerta.
const HOLGURA_M: f32 = 0.6;

/// Una puerta que el jugador puede abrir o cerrar. La agregan solas las tres
/// clases de puerta (`#[require(Abrible)]`).
#[derive(Component, Default)]
pub struct Abrible {
    /// El jugador pidió cambiarla de estado. El sistema de la puerta lo consume
    /// (y lo ignora si está a mitad de una transición: no se cambia de idea a
    /// mitad de camino).
    pub pedido: bool,
}

pub struct PuertasPlugin;

impl Plugin for PuertasPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, pedir_puerta);
    }
}

#[allow(clippy::too_many_arguments)]
fn pedir_puerta(
    mouse: Res<ButtonInput<MouseButton>>,
    keys: Res<ButtonInput<KeyCode>>,
    mut touches: EventReader<TouchInput>,
    windows: Query<&Window, With<PrimaryWindow>>,
    cam: Query<(&Camera, &GlobalTransform), With<GameCamera>>,
    touch_layout: Res<TouchLayout>,
    blocks: Res<TouchBlockRects>,
    player: Query<&Transform, With<Player>>,
    mut puertas: Query<(&GlobalTransform, &Sprite, &mut Abrible)>,
) {
    let Ok(pj) = player.get_single() else {
        touches.clear();
        return;
    };
    let pie = pj.translation.truncate();
    let tecla = keys.just_pressed(KeyCode::KeyE);
    let pantalla = if mouse.just_pressed(MouseButton::Left) {
        windows.get_single().ok().and_then(|w| w.cursor_position())
    } else {
        touches
            .read()
            .find(|t| {
                t.phase == TouchPhase::Started
                    && !touch_in_move_zone(t.position, &touch_layout)
                    && !touch_blocked(t.position, &blocks.0)
            })
            .map(|t| t.position)
    };
    touches.clear();
    let punto = pantalla.and_then(|s| {
        let (camera, cam_tf) = cam.get_single().ok()?;
        let win = windows.get_single().ok()?;
        crate::render::scale::window_to_world_2d(camera, cam_tf, win, s)
    });
    if punto.is_none() && !tecla {
        return;
    }
    let mut mejor: Option<(Mut<Abrible>, f32)> = None;
    for (tf, sprite, abrible) in &mut puertas {
        let c = tf.translation().truncate();
        let d_pj = c.distance(pie);
        if d_pj > ALCANCE_M {
            continue;
        }
        let puntaje = match punto {
            Some(p) => {
                let medio = sprite.custom_size.unwrap_or(Vec2::splat(1.0)) * 0.5 + Vec2::splat(HOLGURA_M);
                let d = (p - c).abs();
                if d.x > medio.x || d.y > medio.y {
                    continue;
                }
                d.length()
            }
            None => d_pj,
        };
        if mejor.as_ref().map_or(true, |(_, m)| puntaje < *m) {
            mejor = Some((abrible, puntaje));
        }
    }
    if let Some((mut a, _)) = mejor {
        a.pedido = true;
    }
}
