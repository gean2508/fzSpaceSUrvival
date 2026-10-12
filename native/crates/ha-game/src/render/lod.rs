//! LOD por TAMAÑO EN PANTALLA — el recorte que Bevy no hace por vos.
//!
//! Bevy ya descarta lo que queda FUERA de cámara (frustum culling), pero no lo
//! que queda dentro midiendo menos de un píxel. Y eso es justo lo que pasa en
//! este juego cuando subís: el encuadre va de 30 m a millones (`universe.rs`), así
//! que a media altura todo el contenido de la escena —NPCs, fauna, colonos,
//! máquinas, estructuras, el compañero— sigue extrayéndose y dibujándose entero
//! para pintar manchas de medio píxel que nadie distingue.
//!
//! Este sistema apaga esos sprites. No es un recorte por distancia (que borraría
//! cosas que sí se ven al hacer zoom): es por **tamaño angular**, o sea la única
//! medida que corresponde — si no llega a un píxel, no hay nada que mirar.
//!
//! ## Por qué hay histéresis
//!
//! Dos umbrales, no uno. Con un umbral único, quedarse con el zoom justo en el
//! corte enciende y apaga cientos de sprites cada frame — es exactamente el
//! parpadeo que se corrigió en `space_fx` y `sky2d`. Se enciende a
//! [`SHOW_PX`] y no se apaga hasta bajar de [`HIDE_PX`].
//!
//! ## Qué NO toca
//!
//! - El **jugador** y sus efectos: tiene que verse siempre, aunque en vista de
//!   sistema sea un punto (hay invariantes de QA que lo exigen — ver
//!   `debug/autodrive.rs`).
//! - Lo que vive en la **banda z de la cámara**: estrellas (−400), polvo (−350),
//!   discos y halos planetarios (−25 … −17). Esos ya se escalan solos con el
//!   encuadre, así que su tamaño en píxeles es constante y el criterio no aplica.
//! - Los **halos de luz** (z 700), las luciérnagas (900) y la estela (260), que
//!   tienen su propio presupuesto en `light2d` / `creative_fly`.

use bevy::prelude::*;

use crate::camera::CameraZoom;
use crate::sim::Player;

/// Por debajo de esto (píxeles de pantalla) un sprite se apaga.
const HIDE_PX: f32 = 1.1;
/// Y no vuelve hasta superar esto. La brecha es la histéresis.
const SHOW_PX: f32 = 2.0;

/// Banda de z que se considera "contenido del mundo". Deja fuera el cielo y los
/// cuerpos celestes (z ≤ −200 y −25…−17 se escalan con el encuadre) y los
/// efectos de luz (z ≥ 100).
const WORLD_Z_MIN: f32 = -10.0;
const WORLD_Z_MAX: f32 = 100.0;

/// Marca que este sprite ya está apagado por LOD, para poder distinguir
/// "lo apagué yo" de "lo apagó su dueño" y no pisarnos.
#[derive(Component)]
struct LodHidden;

pub struct RenderLodPlugin;

impl Plugin for RenderLodPlugin {
    fn build(&self, app: &mut App) {
        // En `PostUpdate`, antes de que Bevy propague visibilidad: así el frame en
        // el que se apaga ya no paga extracción.
        app.add_systems(
            PostUpdate,
            cull_subpixel_sprites.before(bevy::render::view::VisibilitySystems::CalculateBounds),
        );
    }
}

#[allow(clippy::type_complexity)]
fn cull_subpixel_sprites(
    mut commands: Commands,
    zoom: Res<CameraZoom>,
    windows: Query<&Window>,
    mut sprites: Query<
        (Entity, &GlobalTransform, &Sprite, &mut Visibility, Option<&LodHidden>),
        (Without<Player>, Without<crate::sim::UnlitSprite>),
    >,
) {
    let Ok(win) = windows.get_single() else { return };
    let win_h = win.resolution.physical_height() as f32;
    if win_h < 1.0 {
        return;
    }
    // Píxeles de pantalla por metro de mundo. Es LA conversión: el encuadre vivo
    // (`live_view_h`) es cuántos metros de mundo entran en el alto de la ventana.
    let px_per_m = win_h / zoom.live_view_h.max(0.001);
    // Con la cámara cerca (a pie) nada puede ser sub-píxel: ni se recorre la query.
    if px_per_m > SHOW_PX / 0.05 {
        return;
    }

    for (e, gt, sprite, mut vis, hidden) in &mut sprites {
        let z = gt.translation().z;
        if !(WORLD_Z_MIN..WORLD_Z_MAX).contains(&z) {
            continue;
        }
        // Sin `custom_size` el tamaño lo dicta la textura y no lo sabemos acá; esos
        // sprites se dejan en paz (son pocos y suelen ser efectos).
        let Some(size) = sprite.custom_size else { continue };
        let px = size.max_element() * px_per_m;
        let was_hidden = hidden.is_some();
        let hide = if was_hidden { px < SHOW_PX } else { px < HIDE_PX };
        if hide == was_hidden {
            continue;
        }
        if hide {
            *vis = Visibility::Hidden;
            commands.entity(e).insert(LodHidden);
        } else {
            // Se devuelve a `Inherited`, NO a `Visible`: si su dueño lo tenía
            // apagado (un prop de una capa que no toca, una puerta cerrada), forzar
            // `Visible` lo encendería de más.
            *vis = Visibility::Inherited;
            commands.entity(e).remove::<LodHidden>();
        }
    }
}
