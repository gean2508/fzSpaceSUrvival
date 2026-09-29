//! Platform-specific hooks (Android display cutout insets, lifecycle pause, etc.).

#[cfg(target_os = "android")]
mod android;

use bevy::audio::{AudioSink, AudioSinkPlayback};
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
    cfg!(target_os = "android") || crate::flags::var("HA_FORCE_TOUCH").is_ok()
}

/// Idle HUD hint when no interact target is active.
/// Chuleta de controles. **Sólo se muestra en modo CREATIVO** (`hud.rs`): en
/// partida normal el juego se explica solo — tocás la cosa y pasa.
pub fn gameplay_controls_hint() -> &'static str {
    if uses_touch_gameplay_ui() {
        "Stick izq: mover - toca el mundo: actuar - SALTAR - MOCHILA"
    } else {
        "WASD | Mouse look | LMB break | RMB/F place | Space jump | Shift sprint | F5 view | F3 debug | BAG"
    }
}

pub struct PlatformPlugin;

impl Plugin for PlatformPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(PreUpdate, (escuchar_ciclo_de_vida, sync_lifecycle_pause).chain())
            .add_systems(PostUpdate, mute_late_sinks);
    }
}

/// **Enterarse de que Android nos mandó a segundo plano.**
///
/// `set_app_paused` existía desde el principio y **nadie la llamaba**: la bandera
/// se quedaba en `false` para siempre, así que todo lo que cuelga de ella era
/// código muerto — el reloj no se pausaba, el audio seguía sonando con la
/// pantalla apagada, el bucle de winit seguía redibujando a 60 fps contra una
/// superficie que Android estaba por llevarse, y las partidas sólo se guardaban
/// por el debounce de 20 s (o sea: si el sistema mataba la app en segundo plano,
/// se perdían hasta 20 segundos de juego).
///
/// El evento lo emite el propio backend de winit; sólo faltaba escucharlo.
/// `WillSuspend` es el aviso ANTES de que la superficie se vaya: ahí es donde
/// hay que frenar, no en `Suspended`, que ya llega tarde.
fn escuchar_ciclo_de_vida(mut eventos: EventReader<bevy::window::AppLifecycle>) {
    for e in eventos.read() {
        use bevy::window::AppLifecycle::*;
        match e {
            WillSuspend | Suspended | Idle => set_app_paused(true),
            WillResume | Running => set_app_paused(false),
        }
    }
}

/// Backgrounded (`onPause`/`onStop`): silence every sink, freeze the sim clock
/// AND idle the winit loop so we stop redrawing at 60 FPS.
///
/// Audio has to be stopped *explicitly*: rodio mixes on its own OS thread, so
/// parking the frame loop leaves the music playing with the screen off (and the
/// speaker + audio HW awake) — that was the battery drain. Pausing the sinks
/// keeps their position, so a resume picks the track back up: nothing is lost.
///
/// Persistence already flushes on the pause edge (see `*_persist.rs`), so a
/// swipe-kill keeps its state.
fn sync_lifecycle_pause(
    mut time: ResMut<Time<Virtual>>,
    sinks: Query<&AudioSink>,
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

    // Everything below is edge-triggered — only on the pause/resume flip.
    if *last_paused == Some(paused) {
        return;
    }
    *last_paused = Some(paused);

    for sink in &sinks {
        if paused {
            sink.pause();
        } else {
            sink.play();
        }
    }

    // Desktop keeps whatever `run_app_inner` chose (HA_AUTOPLAY needs
    // `Continuous`), and never backgrounds anyway.
    #[cfg(not(target_os = "android"))]
    let _ = &mut winit;

    #[cfg(target_os = "android")]
    {
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
            // **`Continuous`, igual que al arrancar (`lib.rs`), NUNCA `mobile()`.**
            // `mobile()` es `reactive(1/60)` + cada evento de entrada: con el dedo
            // en el joystick la pantalla reporta a 120-240 Hz y el juego corría
            // 2-4 veces más seguido de lo que presentaba. Lo peor es que sólo
            // pasaba DESPUÉS de la primera pausa (bajar la cortina, una llamada,
            // apagar la pantalla): desde ahí la partida entera quemaba batería y
            // calentaba el teléfono al doble. Reporte del 29 sep 2026.
            bevy::winit::WinitSettings {
                focused_mode: bevy::winit::UpdateMode::Continuous,
                unfocused_mode: bevy::winit::UpdateMode::reactive_low_power(
                    std::time::Duration::from_secs(1),
                ),
            }
        };
    }
}

/// Bevy creates the `AudioSink` in `PostUpdate`, after the spawn — so a sound
/// queued on the very frame we backgrounded would start playing *after*
/// `sync_lifecycle_pause` parked the loop, and never be paused. Catch those.
/// Free when running (one atomic read, then out).
fn mute_late_sinks(sinks: Query<&AudioSink>) {
    if !is_app_paused() {
        return;
    }
    for sink in &sinks {
        if !sink.is_paused() {
            sink.pause();
        }
    }
}
