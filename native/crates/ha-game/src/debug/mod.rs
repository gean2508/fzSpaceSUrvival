mod ablate;
mod colision;
pub use colision::VerColisionPlugin;
mod autodrive;
mod crash;
mod cpu_span;
mod perf;
mod stages;
pub use ablate::AblatePlugin;
pub use stages::{
    enabled as stage_probe_on, probe, set_enabled as set_stage_probe, StageProbePlugin,
};
pub use autodrive::AutoDrivePlugin;

pub use crash::{
    clear_last_crash, crash_history_lines, install_crash_hook, last_crash_line, CrashReportPlugin,
};
pub use cpu_span::{CpuSpanPlugin, CpuSpanSlot, CpuSpanStats, span_guard};
pub use perf::{apply_android_frame_cap, PerfDebugPlugin, PerfOverlayVisible};

/// **Los paneles de depuración arrancan APAGADOS.** Estaban atados al modo
/// creativo, y como el juego es un sandbox creativo, cualquiera que jugara veía
/// líneas de "VIVARIO vivos…" y "ECO region…" encima de los corazones (capturas del
/// vuelo, 27 sep 2026). Se prenden con `/depurar` en la consola o `HA_DEBUG_HUD=1`.
static PANELES: std::sync::atomic::AtomicU8 = std::sync::atomic::AtomicU8::new(2);

/// ¿Se muestran los paneles de depuración?
pub fn paneles_visibles() -> bool {
    use std::sync::atomic::Ordering;
    match PANELES.load(Ordering::Relaxed) {
        0 => false,
        1 => true,
        _ => {
            let on = crate::flags::var("HA_DEBUG_HUD").is_ok();
            PANELES.store(on as u8, Ordering::Relaxed);
            on
        }
    }
}

/// Alterna los paneles; devuelve el estado nuevo.
pub fn alternar_paneles() -> bool {
    let nuevo = !paneles_visibles();
    PANELES.store(nuevo as u8, std::sync::atomic::Ordering::Relaxed);
    nuevo
}
