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
        #[cfg(target_os = "android")]
        app.add_systems(Update, android_res::apply_surface_scale);
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

// --- EXPERIMENTAL: Android surface-resolution scale (bible §6 lever #1) --------
//
// `ANativeWindow_setBuffersGeometry` shrinks the window buffers; SurfaceFlinger
// upscales them to the display for free — the canonical GLES trick, NO blit
// (the offscreen blit regressed on this device). Toggled by marker files so
// adb can flip/revert it without touching the UI (debug build: `run-as`):
//
//   adb shell "run-as com.hungerarena.app touch files/res-scale-60"   # 60 %
//   adb shell "run-as com.hungerarena.app rm files/res-scale-60"     # revert
//   adb shell am force-stop com.hungerarena.app                       # restart
//
// Verify on device: image fills the screen (not cropped), touch lands where
// you tap, ha-res log line shows the applied size.

#[cfg(target_os = "android")]
mod android_res {
    use bevy::prelude::*;
    use bevy::window::{PrimaryWindow, RawHandleWrapper};
    use raw_window_handle::RawWindowHandle;

    #[link(name = "android")]
    extern "C" {
        fn ANativeWindow_setBuffersGeometry(
            window: *mut std::ffi::c_void,
            width: i32,
            height: i32,
            format: i32,
        ) -> i32;
    }

    fn requested_scale() -> f32 {
        let dir = crate::inventory_persist::persist_dir();
        for (name, s) in [
            ("res-scale-50", 0.5_f32),
            ("res-scale-60", 0.6),
            ("res-scale-75", 0.75),
        ] {
            if dir.join(name).exists() {
                return s;
            }
        }
        1.0
    }

    /// Applies once as soon as the winit surface exists.
    pub fn apply_surface_scale(
        mut done: Local<bool>,
        windows: Query<(&Window, &RawHandleWrapper), With<PrimaryWindow>>,
    ) {
        if *done {
            return;
        }
        let Ok((win, handle)) = windows.get_single() else {
            return;
        };
        let scale = requested_scale();
        if scale >= 0.999 {
            *done = true;
            return;
        }
        let (w, h) = (win.physical_width(), win.physical_height());
        if w < 64 || h < 64 {
            return;
        }
        let (sw, sh) = (
            ((w as f32 * scale) as i32).max(64),
            ((h as f32 * scale) as i32).max(64),
        );
        // SAFETY: handle is the live ANativeWindow provided by winit; the call
        // only resizes its buffer geometry (format 0 = keep current).
        let raw = handle.window_handle;
        if let RawWindowHandle::AndroidNdk(ndk) = raw {
            let ret = unsafe {
                ANativeWindow_setBuffersGeometry(ndk.a_native_window.as_ptr(), sw, sh, 0)
            };
            eprintln!("ha-res: setBuffersGeometry {w}x{h} -> {sw}x{sh} (scale {scale}) ret={ret}");
        } else {
            eprintln!("ha-res: no AndroidNdk window handle — skip");
        }
        *done = true;
    }
}

#[cfg(target_os = "android")]
pub use android_res::apply_surface_scale;
