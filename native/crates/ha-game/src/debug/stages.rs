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
//!
//! ## Por qué se registra SIEMPRE y se prende en caliente
//!
//! `HA_STAGES` es una variable de entorno, y **a un APK lanzado por el sistema no
//! se le pasan variables de entorno**. O sea que la sonda que cazó los dos
//! últimos culpables de §10 sólo existía en la compu — justo donde los FPS no
//! significan nada (940M + NVK). El teléfono, que es el único sitio donde el
//! perf importa (regla 2), se quedaba con `ha-perf` y `ha-hitch`: cuánto duele,
//! nunca quién.
//!
//! Por eso el plugin se registra siempre y el interruptor es un atómico que
//! `MENU -> SONDA` mueve en caliente. Apagada no cuesta: los siete sistemas-marca
//! son un `load` relajado y un `return`, y `probe()` ni toma el `Mutex` ni mira
//! el reloj (ver `ON`, abajo).

use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Instant;

use bevy::app::MainScheduleOrder;
use bevy::ecs::schedule::ScheduleLabel;
use bevy::prelude::*;

/// Nombres de los tramos que separan las marcas, en orden.
const SPANS: [&str; 7] =
    ["first", "pre", "fixed", "update", "post", "last", "resto"];

/// Estado de arranque: en escritorio `HA_STAGES=1` la deja prendida de entrada,
/// como siempre. En el teléfono no hay entorno, así que arranca apagada y se
/// prende desde `MENU -> SONDA`.
fn enabled_at_boot() -> bool {
    crate::flags::var("HA_STAGES").is_ok()
}

/// La prende/apaga en caliente. `MENU -> SONDA` la llama, y también el arranque.
///
/// El orden importa: al PRENDER, el mapa de `ha-sys` tiene que existir antes de
/// que `probe()` vea el interruptor en alto, o los primeros guards escriben en
/// un `None` y esa fracción de segundo sale sin atribuir. Al APAGAR, al revés.
pub fn set_enabled(on: bool) {
    if on {
        *lock_named() = Some(HashMap::new());
        ON.store(true, Ordering::Relaxed);
    } else {
        ON.store(false, Ordering::Relaxed);
        *lock_named() = None;
    }
    eprintln!("ha-stages: sonda {}", if on { "ON" } else { "OFF" });
}

pub fn enabled() -> bool {
    ON.load(Ordering::Relaxed)
}

/// Un `Mutex` envenenado (un `probe` que murió en un panic mientras lo tenía)
/// no puede tumbar al interruptor: el mapa es de diagnóstico y se reemplaza
/// entero en cada `set_enabled`, así que los datos de adentro son descartables.
fn lock_named() -> std::sync::MutexGuard<'static, Option<HashMap<&'static str, u64>>> {
    NAMED.lock().unwrap_or_else(|e| e.into_inner())
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
    ///
    /// Con la sonda apagada se limpia el acumulador en vez de acumular. Eso
    /// además arregla el arranque en caliente: al prender, `prev` está en `None`,
    /// así que la primera marca sólo deja la referencia y **no le carga a nadie
    /// el rato que la sonda estuvo apagada** — que si no sale como un tramo de
    /// varios minutos y tapa la tabla entera.
    fn stamp(&mut self, idx: usize) {
        if !enabled() {
            self.prev = None;
            self.acc = [0; 7];
            self.frames = 0;
            self.window = None;
            return;
        }
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
        set_enabled(enabled_at_boot());
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
    let mut g = lock_named();
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
    if !enabled() {
        return;
    }
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
