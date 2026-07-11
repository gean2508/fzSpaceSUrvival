//! Lightweight CPU span profiler — logs `ha-span` every 60 frames (Android logcat).

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

#[derive(Resource, Default)]
pub struct CpuSpanStats {
    pub camera_us: u64,
    pub chunk_us: u64,
    pub target_us: u64,
    pub hud_us: u64,
    pub particles_us: u64,
    pub atmo_us: u64,
    pub flight_us: u64,
    pub terrain_us: u64,
    pub frames: u32,
}

/// Ring buffer of 60 averaged samples — pre-allocated (B9).
#[derive(Resource)]
pub struct SpanAccumulator {
    pub samples: [[u64; 9]; 60],
    pub head: usize,
    pub count: usize,
}

impl Default for SpanAccumulator {
    fn default() -> Self {
        Self {
            samples: [[0; 9]; 60],
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
    TotalUpdate,
}

pub fn span_start() -> Instant {
    Instant::now()
}

pub fn span_record(stats: &mut CpuSpanStats, slot: CpuSpanSlot, start: Instant) {
    let us = start.elapsed().as_micros() as u64;
    match slot {
        CpuSpanSlot::Camera => stats.camera_us += us,
        CpuSpanSlot::Chunk => stats.chunk_us += us,
        CpuSpanSlot::Target => stats.target_us += us,
        CpuSpanSlot::Hud => stats.hud_us += us,
        CpuSpanSlot::Particles => stats.particles_us += us,
        CpuSpanSlot::Atmo => stats.atmo_us += us,
        CpuSpanSlot::Flight => stats.flight_us += us,
        CpuSpanSlot::Terrain => stats.terrain_us += us,
        CpuSpanSlot::TotalUpdate => {}
    }
}

/// Records elapsed time on drop — use for early-return systems.
pub struct SpanGuard<'a> {
    stats: &'a mut CpuSpanStats,
    slot: CpuSpanSlot,
    start: Instant,
}

impl Drop for SpanGuard<'_> {
    fn drop(&mut self) {
        span_record(self.stats, self.slot, self.start);
    }
}

pub fn span_guard<'a>(stats: &'a mut CpuSpanStats, slot: CpuSpanSlot) -> SpanGuard<'a> {
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
    let row = [
        stats.camera_us / n,
        stats.chunk_us / n,
        stats.target_us / n,
        stats.hud_us / n,
        stats.particles_us / n,
        stats.atmo_us / n,
        stats.flight_us / n,
        stats.terrain_us / n,
        (stats.camera_us
            + stats.chunk_us
            + stats.target_us
            + stats.hud_us
            + stats.particles_us
            + stats.atmo_us
            + stats.flight_us
            + stats.terrain_us)
            / n,
    ];
    let head = acc.head;
    acc.samples[head] = row;
    acc.head = (head + 1) % 60;
    acc.count = (acc.count + 1).min(60);

    eprintln!(
        "ha-span camera={} chunk={} target={} hud={} particles={} atmo={} flight={} terrain={} total_update={} us_avg",
        row[0], row[1], row[2], row[3], row[4], row[5], row[6], row[7], row[8],
    );
    *stats = CpuSpanStats::default();
}
