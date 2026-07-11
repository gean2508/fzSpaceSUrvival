//! Keep the 3D camera rendering directly to the window (no offscreen blit).
//! Internal scale blit regressed on Redmi — geometry + shader tier cuts are preferred.

use bevy::prelude::*;
use bevy::render::camera::RenderTarget;
use bevy::window::{PrimaryWindow, WindowRef};

use crate::camera::GameCamera;
use crate::input::calibration::DEFAULT_RENDER_SCALE;
use crate::ui_editor::UiLayoutConfig;

pub struct RenderScalePlugin;

impl Plugin for RenderScalePlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<RenderScaleState>()
            .add_systems(
                Update,
                sync_render_target.after(bevy::transform::TransformSystem::TransformPropagate),
            );
    }
}

#[derive(Resource, Default)]
pub struct RenderScaleState {
    pub scale: f32,
    pub size: UVec2,
    pub image: Option<Handle<Image>>,
}

fn sync_render_target(
    window: Query<&Window, With<PrimaryWindow>>,
    config: Res<UiLayoutConfig>,
    mut state: ResMut<RenderScaleState>,
    mut game_cam: Query<&mut Camera, With<GameCamera>>,
) {
    let Ok(win) = window.get_single() else {
        return;
    };

    let Ok(mut game_camera) = game_cam.get_single_mut() else {
        return;
    };

    let _ = config;
    state.scale = 1.0;
    state.size = UVec2::new(win.physical_width().max(64), win.physical_height().max(64));
    state.image = None;
    game_camera.is_active = true;
    game_camera.order = 0;
    game_camera.target = RenderTarget::Window(WindowRef::Primary);
    // Usar el `ClearColor` GLOBAL (lo maneja `sky_clock` en la Tierra y
    // `transition_zone_atmosphere` en Luna/espacio/cueva → oscuro fuera de la
    // Tierra). Antes se forzaba a SKY_ZENITH (azul Tierra) cada frame → la Luna
    // tenía fondo azul en vez de espacio negro.
    game_camera.clear_color = ClearColorConfig::Default;
}

/// Used at startup before UiLayoutConfig is loaded.
pub fn initial_render_scale() -> f32 {
    #[cfg(target_os = "android")]
    {
        DEFAULT_RENDER_SCALE
    }
    #[cfg(not(target_os = "android"))]
    {
        1.0
    }
}
