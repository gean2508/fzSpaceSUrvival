//! Structured crash reporting — `ha-crash` log tag + optional cache file on Android.

use bevy::prelude::*;
use std::panic::PanicHookInfo;
use std::sync::{Mutex, OnceLock};

use crate::debug::perf::{PerfRingBuffer, PerfSampleLine, PerfStats};
use crate::input::{HoldInteractState, MovementIntent, TouchActions};
use crate::platform::is_app_paused;
use crate::scene::RegionRuntime;

static CRASH_CTX: OnceLock<Mutex<CrashContext>> = OnceLock::new();

#[derive(Clone, Debug, Default)]
pub struct CrashContext {
    pub frame: u64,
    pub region: String,
    pub paused: bool,
    pub move_x: f32,
    pub move_z: f32,
    pub action: u8,
    pub hold_active: bool,
    pub hold_progress_ms: u32,
    pub hold_duration_ms: u32,
    pub hold_placing_machine: bool,
    pub perf_samples: Vec<PerfSampleLine>,
}

#[derive(Resource, Default)]
pub struct CrashFrameCounter(pub u64);

pub struct CrashReportPlugin;

impl Plugin for CrashReportPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<CrashFrameCounter>()
            .add_systems(PostUpdate, update_crash_context);
    }
}

pub fn install_crash_hook() {
    CRASH_CTX.get_or_init(|| Mutex::new(CrashContext::default()));
    std::panic::set_hook(Box::new(crash_panic_hook));
}

fn crash_panic_hook(info: &PanicHookInfo) {
    let ctx = CRASH_CTX
        .get()
        .and_then(|m| m.lock().ok().map(|g| g.clone()))
        .unwrap_or_default();

    let location = info
        .location()
        .map(|l| format!("{}:{}:{}", l.file(), l.line(), l.column()))
        .unwrap_or_else(|| "unknown".into());

    let message = info
        .payload()
        .downcast_ref::<&str>()
        .map(|s| (*s).to_string())
        .or_else(|| info.payload().downcast_ref::<String>().cloned())
        .unwrap_or_else(|| "panic".into());

    let line = build_crash_report(&message, &location, &ctx);
    eprintln!("ha-crash {line}");
    persist_crash_report(&line);
    #[cfg(target_os = "android")]
    std::process::exit(1);
}

pub(crate) fn build_crash_report(message: &str, location: &str, ctx: &CrashContext) -> String {
    let report = serde_json::json!({
        "tag": "ha-crash",
        "message": message,
        "location": location,
        "frame": ctx.frame,
        "region": ctx.region,
        "paused": ctx.paused,
        "input": {
            "moveX": ctx.move_x,
            "moveZ": ctx.move_z,
            "action": ctx.action,
        },
        "hold": {
            "active": ctx.hold_active,
            "progressMs": ctx.hold_progress_ms,
            "durationMs": ctx.hold_duration_ms,
            "placingMachine": ctx.hold_placing_machine,
        },
        "perf": ctx.perf_samples,
    });
    report.to_string()
}

/// Dónde queda el último reporte. En Android va al cache de la app (lo único
/// escribible sin permisos); en escritorio, al lado del binario.
fn crash_report_paths() -> &'static [&'static str] {
    #[cfg(target_os = "android")]
    {
        &["/data/data/com.hungerarena.app/cache/ha-last-crash.json", "./ha-last-crash.json"]
    }
    #[cfg(not(target_os = "android"))]
    {
        &["./ha-last-crash.json"]
    }
}

fn persist_crash_report(payload: &str) {
    for p in crash_report_paths() {
        if std::fs::write(p, payload).is_ok() {
            eprintln!("ha-crash persisted={p}");
            return;
        }
    }
}

/// Una línea legible del último cierre, o `None` si la partida anterior terminó
/// bien. La lee el menú principal.
///
/// **Por qué se muestra en pantalla y no sólo en logcat.** En Android el hook de
/// pánico termina en `std::process::exit(1)`: la app se cierra **de golpe y sin
/// diálogo**, que es exactamente el reporte "se cierra solito". Con el flujo de
/// este proyecto (APK por URL, sin cable) el motivo era invisible. Ahora al
/// volver a abrir el juego el menú lo dice.
pub fn last_crash_line() -> Option<String> {
    let raw = crash_report_paths().iter().find_map(|p| std::fs::read_to_string(p).ok())?;
    let v: serde_json::Value = serde_json::from_str(&raw).ok()?;
    let msg = v.get("message")?.as_str()?;
    let loc = v.get("location").and_then(|l| l.as_str()).unwrap_or("?");
    // El archivo sólo dice "la vez pasada": ASCII y recortado, la fuente del
    // juego no dibuja acentos (regla 6) y el menú no es un visor de JSON.
    let short = loc.rsplit('/').next().unwrap_or(loc);
    let line = format!("ULTIMO CIERRE: {msg} ({short})");
    Some(line.chars().filter(|c| c.is_ascii()).take(120).collect())
}

/// Borra el reporte. Lo llama el menú tras mostrarlo: la línea vale para el
/// cierre que acabás de sufrir, no para siempre.
pub fn clear_last_crash() {
    for p in crash_report_paths() {
        let _ = std::fs::remove_file(p);
    }
}

fn update_crash_context(
    mut counter: ResMut<CrashFrameCounter>,
    region: Option<Res<RegionRuntime>>,
    intent: Option<Res<MovementIntent>>,
    actions: Option<Res<TouchActions>>,
    hold: Option<Res<HoldInteractState>>,
    perf: Option<Res<PerfStats>>,
    ring: Option<Res<PerfRingBuffer>>,
) {
    counter.0 += 1;
    let Some(lock) = CRASH_CTX.get() else {
        return;
    };
    let Ok(mut ctx) = lock.lock() else {
        return;
    };

    ctx.frame = counter.0;
    ctx.region = region
        .as_ref()
        .map(|r| format!("{:?}", r.active))
        .unwrap_or_else(|| "unknown".into());
    ctx.paused = is_app_paused();

    if let Some(i) = intent.as_ref() {
        ctx.move_x = i.move_x;
        ctx.move_z = i.move_z;
    }
    if let Some(a) = actions.as_ref() {
        ctx.action = u8::from(a.action);
    }

    if let Some(h) = hold.as_ref() {
        ctx.hold_active = h.active;
        ctx.hold_progress_ms = h.progress_ms;
        ctx.hold_duration_ms = h.duration_ms;
        ctx.hold_placing_machine = h.placing_machine;
    }

    if let (Some(stats), Some(ring)) = (perf.as_ref(), ring.as_ref()) {
        ctx.perf_samples = ring.samples.iter().cloned().collect();
        if ctx.perf_samples.is_empty() {
            ctx.perf_samples.push(PerfSampleLine {
                fps: stats.fps,
                frame_ms: stats.frame_ms,
                draw3d: stats.draw_calls,
                ui: stats.breakdown.ui_nodes,
                ents: stats.active_entities,
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn crash_report_includes_ha_crash_fields() {
        let json = build_crash_report(
            "test panic",
            "crash.rs:1:1",
            &CrashContext {
                frame: 99,
                region: "Overworld { gx: 0, gz: 0 }".into(),
                paused: false,
                move_x: 0.5,
                move_z: 1.0,
                action: 1,
                hold_active: true,
                hold_progress_ms: 500,
                hold_duration_ms: 2000,
                hold_placing_machine: true,
                perf_samples: vec![],
            },
        );
        assert!(json.contains("\"tag\":\"ha-crash\""));
        assert!(json.contains("\"frame\":99"));
        assert!(json.contains("\"placingMachine\":true"));
    }
}
