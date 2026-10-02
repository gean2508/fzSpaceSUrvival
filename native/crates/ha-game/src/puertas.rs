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
use bevy::ecs::system::SystemParam;
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
    /// Segundos hasta que se cierre sola: sólo cuando la abrió un vecino (`npc_mundo`) al
    /// pasar. Una puerta que abriste vos queda como la dejaste (0 = no se cierra sola).
    pub por_npc: f32,
}

pub struct PuertasPlugin;

impl Plugin for PuertasPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<PedidosDePuerta>()
            .add_systems(Update, (pedir_puerta, atender_pedidos_de_vecinos));
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

/// **Una puerta CERRADA frena; una abierta se cruza.** Para eso existe abrirla.
///
/// El sim compartido no conoce el estado de las puertas (vive en el cliente, en
/// cada `open`), así que el jugador las respeta acá, por metros, como al decor.
/// Una puerta a mitad de transición cuenta como la del estado al que va.
#[derive(SystemParam)]
pub struct PuertasQueFrenan<'w, 's> {
    fachada: Query<'w, 's, &'static crate::building::PuertaDeFachada>,
    celda: Query<'w, 's, &'static crate::building::DoorArt>,
    borde: Query<'w, 's, &'static crate::walls::WallDoor>,
}

/// El rectángulo `[x0, z0, x1, z1]` (metros de mundo) de una puerta que ocupa una celda.
pub fn rect_de_celda(tx: i64, tz: i64) -> [f32; 4] {
    use ha_shared::topdown::TILE_M;
    [tx as f32 * TILE_M, tz as f32 * TILE_M, (tx + 1) as f32 * TILE_M, (tz + 1) as f32 * TILE_M]
}

/// El rectángulo de una puerta que vive en el borde entre dos celdas.
pub fn rect_de_borde(d: &crate::walls::WallDoor) -> [f32; 4] {
    use ha_shared::build_grid::{WallAxis, WALL_THICK};
    use ha_shared::topdown::TILE_M;
    let (mitad_largo, mitad_ancho) = (TILE_M * 0.5, WALL_THICK * 0.5);
    match d.axis {
        WallAxis::H => [d.cx - mitad_largo, d.cz - mitad_ancho, d.cx + mitad_largo, d.cz + mitad_ancho],
        WallAxis::V => [d.cx - mitad_ancho, d.cz - mitad_largo, d.cx + mitad_ancho, d.cz + mitad_largo],
    }
}

impl PuertasQueFrenan<'_, '_> {
    /// ¿La caja `[x0, z0, x1, z1]` (metros de mundo) pisa una puerta cerrada?
    pub fn frena(&self, caja: [f32; 4]) -> bool {
        let toca = |r: [f32; 4]| r[0] < caja[2] && caja[0] < r[2] && r[1] < caja[3] && caja[1] < r[3];
        self.fachada.iter().any(|p| !p.open && toca(rect_de_celda(p.tx, p.tz)))
            || self.celda.iter().any(|d| !d.open && toca(rect_de_celda(d.tx, d.tz)))
            || self.borde.iter().any(|d| !d.open && toca(rect_de_borde(d)))
    }
}

/// Cuánto espera abierta una puerta que abrió un vecino antes de cerrarse sola.
const CIERRE_S: f32 = 4.0;
/// A cuánto de la puerta (m) alguien la mantiene abierta.
const RADIO_PASO: f32 = 1.8;

/// Los puntos (metros de mundo) donde un vecino se topó con una puerta cerrada o está por
/// cruzarla. Los escribe `society::tick_society`; los atiende [`atender_pedidos_de_vecinos`].
#[derive(Resource, Default)]
pub struct PedidosDePuerta(pub Vec<(f32, f32)>);

/// **Los vecinos abren las puertas que necesitan y las cierran al pasar.** Una puerta que
/// abrió un vecino se cierra sola a los [`CIERRE_S`] de que nadie esté cerca; una que abriste
/// vos (`por_npc == 0`) queda como la dejaste.
#[allow(clippy::type_complexity)]
fn atender_pedidos_de_vecinos(
    time: Res<Time>,
    mut pedidos: ResMut<PedidosDePuerta>,
    npcs: Query<&crate::society::Npc>,
    player: Query<&Transform, With<Player>>,
    mut fachada: Query<(&crate::building::PuertaDeFachada, &mut Abrible)>,
    mut celda: Query<(&crate::building::DoorArt, &mut Abrible), Without<crate::building::PuertaDeFachada>>,
    mut borde: Query<
        (&crate::walls::WallDoor, &mut Abrible),
        (Without<crate::building::PuertaDeFachada>, Without<crate::building::DoorArt>),
    >,
) {
    let dt = time.delta_secs();
    let hay_pedidos = !pedidos.0.is_empty();
    // Sin pedidos y sin puertas abiertas por vecinos no hay nada que hacer: lo normal.
    let mut gente: Option<Vec<(f32, f32)>> = None;
    let mut tratar = |open: bool, rect: [f32; 4], ab: &mut Abrible| {
        let pide = hay_pedidos
            && pedidos.0.iter().any(|&(x, z)| x > rect[0] - 0.4 && x < rect[2] + 0.4 && z > rect[1] - 0.4 && z < rect[3] + 0.4);
        if !open {
            if pide {
                ab.pedido = true;
                ab.por_npc = CIERRE_S;
            } else if !ab.pedido {
                ab.por_npc = 0.0;
            }
            return;
        }
        if ab.por_npc <= 0.0 {
            return;
        }
        let g = gente.get_or_insert_with(|| {
            let mut v: Vec<(f32, f32)> = npcs.iter().map(|n| (n.x, n.z)).collect();
            if let Ok(p) = player.get_single() {
                v.push((p.translation.x, -p.translation.y));
            }
            v
        });
        let (cx, cz) = ((rect[0] + rect[2]) * 0.5, (rect[1] + rect[3]) * 0.5);
        if pide || g.iter().any(|&(x, z)| (x - cx).powi(2) + (z - cz).powi(2) < RADIO_PASO * RADIO_PASO) {
            ab.por_npc = CIERRE_S;
        } else {
            ab.por_npc -= dt;
            if ab.por_npc <= 0.0 {
                ab.por_npc = 0.0;
                ab.pedido = true;
            }
        }
    };
    for (p, mut ab) in &mut fachada {
        tratar(p.open, rect_de_celda(p.tx, p.tz), &mut ab);
    }
    for (d, mut ab) in &mut celda {
        tratar(d.open, rect_de_celda(d.tx, d.tz), &mut ab);
    }
    for (d, mut ab) in &mut borde {
        tratar(d.open, rect_de_borde(d), &mut ab);
    }
    pedidos.0.clear();
}
