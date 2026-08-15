//! Lightweight CPU span profiler — logs `ha-span` every 60 frames (Android logcat).

use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Instant;

use bevy::prelude::*;

pub struct CpuSpanPlugin;

impl Plugin for CpuSpanPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<CpuSpanStats>()
            .init_resource::<SpanAccumulator>()
            .init_resource::<TotalUpdateStart>()
            .add_systems(First, record_total_update_span)
            .add_systems(Last, log_cpu_spans);
    }
}

/// **Contadores atómicos, no `u64` a secas.** Con campos normales, medir un
/// sistema exige `ResMut<CpuSpanStats>`, que es acceso EXCLUSIVO: dos sistemas
/// instrumentados nunca pueden correr en paralelo. Con `multi_threaded` activo
/// eso convierte el profiler en la causa de lo que viene a medir — y cuanto más
/// se instrumenta, más miente. Con atómicos alcanza `Res<_>` (acceso compartido)
/// y el planificador los sigue repartiendo entre hilos; el coste por span es un
/// `fetch_add` relajado, o sea nada.
#[derive(Resource, Default)]
pub struct CpuSpanStats {
    pub camera_us: AtomicU64,
    pub chunk_us: AtomicU64,
    pub target_us: AtomicU64,
    pub hud_us: AtomicU64,
    pub particles_us: AtomicU64,
    pub atmo_us: AtomicU64,
    pub flight_us: AtomicU64,
    pub terrain_us: AtomicU64,
    pub lights_us: AtomicU64,
    pub frames: u32,
}

impl CpuSpanStats {
    fn take(&self, slot: &AtomicU64) -> u64 {
        slot.load(Ordering::Relaxed)
    }

    /// Microsegundos acumulados en un slot desde el último reinicio.
    pub fn us(&self, slot: CpuSpanSlot) -> u64 {
        let counter = match slot {
            CpuSpanSlot::Camera => &self.camera_us,
            CpuSpanSlot::Chunk => &self.chunk_us,
            CpuSpanSlot::Target => &self.target_us,
            CpuSpanSlot::Hud => &self.hud_us,
            CpuSpanSlot::Particles => &self.particles_us,
            CpuSpanSlot::Atmo => &self.atmo_us,
            CpuSpanSlot::Flight => &self.flight_us,
            CpuSpanSlot::Terrain => &self.terrain_us,
            CpuSpanSlot::Lights => &self.lights_us,
            CpuSpanSlot::TotalUpdate => return 0,
        };
        counter.load(Ordering::Relaxed)
    }

    fn reset(&mut self) {
        for slot in [
            &self.camera_us,
            &self.chunk_us,
            &self.target_us,
            &self.hud_us,
            &self.particles_us,
            &self.atmo_us,
            &self.flight_us,
            &self.terrain_us,
            &self.lights_us,
        ] {
            slot.store(0, Ordering::Relaxed);
        }
        self.frames = 0;
    }
}

/// Ring buffer of 60 averaged samples — pre-allocated (B9).
#[derive(Resource)]
pub struct SpanAccumulator {
    pub samples: [[u64; 10]; 60],
    pub head: usize,
    pub count: usize,
}

impl Default for SpanAccumulator {
    fn default() -> Self {
        Self {
            samples: [[0; 10]; 60],
            head: 0,
            count: 0,
        }
    }
}

#[derive(Clone, Copy)]
pub enum CpuSpanSlot {
    Camera,
    Chunk,
    Target,
    Hud,
    Particles,
    Atmo,
    Flight,
    Terrain,
    Lights,
    TotalUpdate,
}

pub fn span_start() -> Instant {
    Instant::now()
}

pub fn span_record(stats: &CpuSpanStats, slot: CpuSpanSlot, start: Instant) {
    let us = start.elapsed().as_micros() as u64;
    let counter = match slot {
        CpuSpanSlot::Camera => &stats.camera_us,
        CpuSpanSlot::Chunk => &stats.chunk_us,
        CpuSpanSlot::Target => &stats.target_us,
        CpuSpanSlot::Hud => &stats.hud_us,
        CpuSpanSlot::Particles => &stats.particles_us,
        CpuSpanSlot::Atmo => &stats.atmo_us,
        CpuSpanSlot::Flight => &stats.flight_us,
        CpuSpanSlot::Terrain => &stats.terrain_us,
        CpuSpanSlot::Lights => &stats.lights_us,
        CpuSpanSlot::TotalUpdate => return,
    };
    counter.fetch_add(us, Ordering::Relaxed);
}

/// Records elapsed time on drop — use for early-return systems.
pub struct SpanGuard<'a> {
    stats: &'a CpuSpanStats,
    slot: CpuSpanSlot,
    start: Instant,
}

impl Drop for SpanGuard<'_> {
    fn drop(&mut self) {
        span_record(self.stats, self.slot, self.start);
    }
}

pub fn span_guard(stats: &CpuSpanStats, slot: CpuSpanSlot) -> SpanGuard<'_> {
    SpanGuard {
        stats,
        slot,
        start: span_start(),
    }
}

#[derive(Resource, Default)]
struct TotalUpdateStart {
    frame_start: Option<Instant>,
    frames: u32,
}

fn record_total_update_span(mut start: ResMut<TotalUpdateStart>) {
    if start.frame_start.take().is_some() {
        start.frames += 1;
    }
    start.frame_start = Some(Instant::now());
}

fn log_cpu_spans(
    mut stats: ResMut<CpuSpanStats>,
    mut acc: ResMut<SpanAccumulator>,
    mut start: ResMut<TotalUpdateStart>,
) {
    stats.frames = start.frames;
    if stats.frames < 60 {
        return;
    }
    start.frames = 0;
    let n = stats.frames as u64;
    let each = [
        stats.take(&stats.camera_us),
        stats.take(&stats.chunk_us),
        stats.take(&stats.target_us),
        stats.take(&stats.hud_us),
        stats.take(&stats.particles_us),
        stats.take(&stats.atmo_us),
        stats.take(&stats.flight_us),
        stats.take(&stats.terrain_us),
        stats.take(&stats.lights_us),
    ];
    let total: u64 = each.iter().sum();
    let mut row = [0u64; 10];
    for (dst, src) in row.iter_mut().zip(each.iter()) {
        *dst = src / n;
    }
    row[9] = total / n;
    let head = acc.head;
    acc.samples[head] = row;
    acc.head = (head + 1) % 60;
    acc.count = (acc.count + 1).min(60);

    eprintln!(
        "ha-span camera={} chunk={} target={} hud={} particles={} atmo={} flight={} terrain={} lights={} measured_total={} us_avg",
        row[0], row[1], row[2], row[3], row[4], row[5], row[6], row[7], row[8], row[9],
    );
    stats.reset();
}
