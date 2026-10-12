//! **Dónde se va el frame, por ETAPA.** `HA_STAGES=1`.
//!
//! Existía un agujero en el diagnóstico de §10: `ha-perf` imprime seis spans
//! (`chk/terrain/lit/atm/ptc/hud`) pero **sólo tres están cableados de verdad**
//! (`Chunk`, `Lights`, `Hud`). `terrain`, `atmo` y `part` salen 0 porque nadie
//! los escribe, no porque cuesten cero — y de ahí salió la conclusión "spans ≈ 0
//! y frame enorme ⇒ es la GPU", que no se sostiene: un sistema sin guard es
//! invisible exactamente igual que uno barato.
//!
//! Esta sonda no depende de que alguien se acuerde de poner un guard. Mete
//! schedules-marca ENTRE las etapas del `Main` (`MainScheduleOrder`), cada una
//! con un único sistema que anota `Instant::now()`; el costo de una etapa es la
//! resta de dos marcas consecutivas. Lo que pasa entre la última marca de un
//! frame y la primera del siguiente es **`resto`**: extracción, render, present
//! y la espera del vsync — o sea, todo lo que el `Main` schedule no ve.
//!
//! Con eso una sola corrida separa las tres respuestas posibles: CPU de gameplay
//! (`update`), CPU de motor (`post`: transform, visibilidad, layout de UI) o
//! GPU/driver (`resto`).

use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Instant;

use bevy::app::MainScheduleOrder;
use bevy::ecs::schedule::ScheduleLabel;
use bevy::prelude::*;

/// Nombres de los tramos que separan las marcas, en orden.
const SPANS: [&str; 7] =
    ["first", "pre", "fixed", "update", "post", "last", "resto"];

pub fn enabled() -> bool {
    std::env::var("HA_STAGES").is_ok()
}

macro_rules! mark_schedule {
    ($name:ident, $idx:expr) => {
        #[derive(ScheduleLabel, Debug, Clone, PartialEq, Eq, Hash)]
        struct $name;

        impl $name {
            fn system(mut p: ResMut<StageProbe>) {
                p.stamp($idx);
            }
        }
    };
}

mark_schedule!(Mark0, 0);
mark_schedule!(Mark1, 1);
mark_schedule!(Mark2, 2);
mark_schedule!(Mark3, 3);
mark_schedule!(Mark4, 4);
mark_schedule!(Mark5, 5);
mark_schedule!(Mark6, 6);

#[derive(Resource, Default)]
struct StageProbe {
    /// Última marca vista (índice, instante).
    prev: Option<(usize, Instant)>,
    /// Acumulado del segundo en curso, en microsegundos, por tramo.
    acc: [u64; 7],
    frames: u32,
    window: Option<Instant>,
}

impl StageProbe {
    /// Anota la marca `idx` y le carga a su tramo el tiempo desde la anterior.
    /// El tramo `resto` (6) es el que va de la última marca del frame a la
    /// primera del siguiente, así que se cierra solo con esta misma regla.
    fn stamp(&mut self, idx: usize) {
        let now = Instant::now();
        if let Some((prev_idx, t0)) = self.prev {
            self.acc[prev_idx] += now.duration_since(t0).as_micros() as u64;
        }
        self.prev = Some((idx, now));
        if idx == 0 {
            self.frames += 1;
        }
    }
}

pub struct StageProbePlugin;

impl Plugin for StageProbePlugin {
    fn build(&self, app: &mut App) {
        if !enabled() {
            return;
        }
        *NAMED.lock().unwrap() = Some(HashMap::new());
        ON.store(true, Ordering::Relaxed);
        app.init_resource::<StageProbe>();
        app.add_systems(Mark0, Mark0::system);
        app.add_systems(Mark1, Mark1::system);
        app.add_systems(Mark2, Mark2::system);
        app.add_systems(Mark3, Mark3::system);
        app.add_systems(Mark4, Mark4::system);
        app.add_systems(Mark5, Mark5::system);
        app.add_systems(Mark6, (Mark6::system, report).chain());

        let mut order = app.world_mut().resource_mut::<MainScheduleOrder>();
        order.insert_before(First, Mark0);
        order.insert_after(First, Mark1);
        order.insert_after(PreUpdate, Mark2);
        order.insert_after(RunFixedMainLoop, Mark3);
        order.insert_after(Update, Mark4);
        order.insert_after(PostUpdate, Mark5);
        order.insert_after(Last, Mark6);
    }
}

// ── Sonda por SISTEMA (`HA_STAGES=1` también) ───────────────────────────────
//
// La de etapas dice "el frame se va en `Update`"; ésta dice en QUÉ de `Update`.
// Va por nombre (`&'static str`) y no por un enum como `CpuSpanSlot`, para poder
// tocar veinte sistemas sin tocar el enum veinte veces. Es de diagnóstico: un
// `Mutex` por sistema y por frame no se paga en una build de release del juego
// porque sin `HA_STAGES` ni se registra.

use std::collections::HashMap;
use std::sync::Mutex;

static NAMED: Mutex<Option<HashMap<&'static str, u64>>> = Mutex::new(None);

/// Interruptor leído por `probe`. **Con la sonda apagada, `probe` no toma el
/// `Mutex` ni mira el reloj**: es un `load` relajado y un `None`. Importa porque
/// estas líneas quedan en el código de producción y el presupuesto del Redmi
/// manda (regla 2) — un lock por sistema instrumentado y por frame sería un
/// costo permanente a cambio de nada.
static ON: AtomicBool = AtomicBool::new(false);

/// Guard que le carga a `name` lo que dure el scope. Uso:
/// `let _probe = crate::debug::probe("tick_fauna");`
pub fn probe(name: &'static str) -> Option<NamedGuard> {
    ON.load(Ordering::Relaxed).then(|| NamedGuard { name, start: Instant::now() })
}

pub struct NamedGuard {
    name: &'static str,
    start: Instant,
}

impl Drop for NamedGuard {
    fn drop(&mut self) {
        let us = self.start.elapsed().as_micros() as u64;
        if let Ok(mut g) = NAMED.lock() {
            if let Some(map) = g.as_mut() {
                *map.entry(self.name).or_insert(0) += us;
            }
        }
    }
}

fn report_named(frames: f64) {
    let Ok(mut g) = NAMED.lock() else { return };
    let Some(map) = g.as_mut() else { return };
    if map.is_empty() {
        return;
    }
    let mut rows: Vec<(&str, u64)> = map.iter().map(|(k, v)| (*k, *v)).collect();
    rows.sort_by_key(|&(_, v)| std::cmp::Reverse(v));
    let line: Vec<String> = rows
        .iter()
        .take(12)
        .map(|(k, v)| format!("{k}={:.1}", *v as f64 / frames / 1000.0))
        .collect();
    eprintln!("ha-sys: {} (ms/frame)", line.join(" "));
    map.clear();
}

fn report(mut p: ResMut<StageProbe>) {
    let now = Instant::now();
    let start = *p.window.get_or_insert(now);
    if now.duration_since(start).as_secs_f32() < 1.0 {
        return;
    }
    let n = p.frames.max(1) as f64;
    let total: u64 = p.acc.iter().sum();
    let parts: Vec<String> = SPANS
        .iter()
        .enumerate()
        .map(|(i, name)| format!("{name}={:.1}", p.acc[i] as f64 / n / 1000.0))
        .collect();
    eprintln!(
        "ha-stages: frames={} frame_ms={:.1} | {} (ms/frame)",
        p.frames,
        total as f64 / n / 1000.0,
        parts.join(" "),
    );
    report_named(n);
    p.acc = [0; 7];
    p.frames = 0;
    p.window = Some(now);
}
