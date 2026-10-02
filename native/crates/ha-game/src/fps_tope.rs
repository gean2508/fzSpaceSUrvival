//! **Tope de cuadros por segundo** (AJUSTES → FPS).
//!
//! Con `UpdateMode::Continuous` + `Fifo`, el juego corre a lo que dé el vsync de
//! la pantalla: 90 Hz en el Redmi. Como en una aldea no llega ni a 60, lo que
//! salía era un ritmo desparejo (15, 32, 24…), la batería en caída libre y el
//! teléfono caliente hasta colgarse (reporte del 29 sep 2026). Un tope parejo a
//! 30 se ve más suave que 24-45 saltando y deja al procesador respirar.
//!
//! Se duerme al final del frame lo que falte para completar el período. No es
//! un reloj de precisión y no hace falta: el vsync redondea el resto.

use std::time::{Duration, Instant};

use bevy::prelude::*;

pub struct FpsTopePlugin;

impl Plugin for FpsTopePlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Last, dormir_hasta_el_tope);
    }
}

fn dormir_hasta_el_tope(config: Res<crate::ui_editor::UiLayoutConfig>, mut ultimo: Local<Option<Instant>>) {
    let tope = config.calibration.fps_max;
    let ahora = Instant::now();
    if tope > 0 && !crate::flags::driver_active() && !sin_tope() {
        if let Some(antes) = *ultimo {
            let periodo = Duration::from_secs_f64(1.0 / tope as f64);
            let gastado = ahora.duration_since(antes);
            if gastado < periodo {
                std::thread::sleep(periodo - gastado);
            }
        }
    }
    *ultimo = Some(Instant::now());
}

/// `setprop debug.ha.sin_tope 1`: sin dormir al final del cuadro (aislar el parpadeo).
fn sin_tope() -> bool {
    static N: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *N.get_or_init(|| crate::flags::var("HA_SIN_TOPE").is_ok())
}
