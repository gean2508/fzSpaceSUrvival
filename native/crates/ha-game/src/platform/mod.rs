//! Platform-specific hooks (Android display cutout insets, lifecycle pause, etc.).

#[cfg(target_os = "android")]
mod android;

use bevy::prelude::*;
use std::sync::atomic::{AtomicBool, AtomicI32, Ordering};

#[derive(Clone, Copy, Debug, Default)]
pub struct DisplayInsets {
    /// Physical pixels from JNI — convert with `scale_factor`.
    pub left: i32,
    pub top: i32,
    pub right: i32,
    pub bottom: i32,
}

static INSETS: AtomicI32 = AtomicI32::new(0);
static INSET_TOP: AtomicI32 = AtomicI32::new(0);
static INSET_RIGHT: AtomicI32 = AtomicI32::new(0);
static INSET_BOTTOM: AtomicI32 = AtomicI32::new(0);

static APP_PAUSED: AtomicBool = AtomicBool::new(false);

pub fn set_display_insets(left: i32, top: i32, right: i32, bottom: i32) {
    INSETS.store(left, Ordering::Relaxed);
    INSET_TOP.store(top, Ordering::Relaxed);
    INSET_RIGHT.store(right, Ordering::Relaxed);
    INSET_BOTTOM.store(bottom, Ordering::Relaxed);
}

pub fn display_insets(scale_factor: f32) -> DisplayInsets {
    let s = scale_factor.max(1.0);
    DisplayInsets {
        left: (INSETS.load(Ordering::Relaxed) as f32 / s).round() as i32,
        top: (INSET_TOP.load(Ordering::Relaxed) as f32 / s).round() as i32,
        right: (INSET_RIGHT.load(Ordering::Relaxed) as f32 / s).round() as i32,
        bottom: (INSET_BOTTOM.load(Ordering::Relaxed) as f32 / s).round() as i32,
    }
}

pub fn set_app_paused(paused: bool) {
    APP_PAUSED.store(paused, Ordering::Relaxed);
    eprintln!("ha-life paused={paused}");
}

pub fn is_app_paused() -> bool {
    APP_PAUSED.load(Ordering::Relaxed)
}

/// Touch sticks + on-screen USE/JUMP — Android only; desktop uses keyboard/mouse.
#[inline]
pub fn uses_touch_gameplay_ui() -> bool {
    // Desktop testing of the touch UI: HA_FORCE_TOUCH=1.
    cfg!(target_os = "android") || std::env::var("HA_FORCE_TOUCH").is_ok()
}

/// Idle HUD hint when no interact target is active.
pub fn gameplay_controls_hint() -> &'static str {
    if uses_touch_gameplay_ui() {
        "← Move · Look → · USE · JUMP · BAG"
    } else {
        "WASD · Mouse look · LMB break · RMB/F place · Space jump · Shift sprint · F5 view · F3 debug · BAG"
    }
}

#[cfg(target_os = "android")]
pub use android::jni_on_load_insets;

pub struct PlatformPlugin;

impl Plugin for PlatformPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(PreUpdate, sync_lifecycle_pause);
    }
}

/// Backgrounded (`onPause`/`onStop`): freeze the sim clock AND idle the winit
/// loop so we stop redrawing at 60 FPS. Persistence already flushes on the
/// pause edge (see `*_persist.rs`), so a swipe-kill keeps its state.
#[cfg(target_os = "android")]
fn sync_lifecycle_pause(
    mut time: ResMut<Time<Virtual>>,
    mut winit: ResMut<bevy::winit::WinitSettings>,
    mut last_paused: Local<Option<bool>>,
) {
    let paused = is_app_paused();
    if paused {
        if !time.is_paused() {
            time.pause();
        }
    } else if time.is_paused() {
        time.unpause();
    }

    // Only rewrite WinitSettings on the pause/resume edge.
    if *last_paused != Some(paused) {
        *last_paused = Some(paused);
        *winit = if paused {
            // Park the loop: wake only on a window/user event (a resume), not
            // on a fixed timer — effectively zero draws while backgrounded.
            let idle = bevy::winit::UpdateMode::reactive_low_power(
                std::time::Duration::from_secs(3600),
            );
            bevy::winit::WinitSettings {
                focused_mode: idle,
                unfocused_mode: idle,
            }
        } else {
            bevy::winit::WinitSettings::mobile()
        };
    }
}

#[cfg(not(target_os = "android"))]
fn sync_lifecycle_pause(mut time: ResMut<Time<Virtual>>) {
    if is_app_paused() {
        if !time.is_paused() {
            time.pause();
        }
    } else if time.is_paused() {
        time.unpause();
    }
}
