use std::collections::VecDeque;

use bevy::prelude::*;
use bevy::window::PrimaryWindow;

use crate::camera::{CameraViewMode, GameCamera};
use crate::earth_world::EarthDecor;
use crate::materials::OutlineShell;
use crate::scene::Terrain;
use crate::debug::CpuSpanStats;
use crate::targeting::InteractTarget;
use crate::ui_theme;

pub struct PerfDebugPlugin;

impl Plugin for PerfDebugPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<PerfStats>()
            .init_resource::<PerfOverlayVisible>()
            .init_resource::<PerfBreakdownTimer>()
            .init_resource::<PerfRingBuffer>()
            .add_systems(Startup, spawn_perf_overlay)
            .add_systems(
                Update,
                (
                    toggle_perf_overlay,
                    sample_frame_timing,
                    sample_perf_breakdown,
                    update_perf_overlay,
                    log_perf_periodic,
                )
                    .chain(),
            );
    }
}

#[derive(Resource, Default)]
pub struct PerfOverlayVisible(pub bool);

#[derive(Resource, Default)]
struct PerfBreakdownTimer {
    elapsed: f32,
    overlay_elapsed: f32,
}

#[derive(Resource, Default, Clone, Copy, Debug)]
pub struct PerfRenderBreakdown {
    pub ocean: u32,
    pub island_ground: u32,
    pub static_world: u32,
    pub chunk_decor: u32,
    pub obstacles: u32,
    pub interactables: u32,
    pub outline: u32,
    pub other_3d: u32,
    pub ui_nodes: u32,
}

#[derive(Resource, Default)]
pub struct PerfStats {
    pub fps: f32,
    pub frame_ms: f32,
    pub frame_jitter_ms: f32,
    pub draw_calls: u32,
    pub visible_meshes: u32,
    pub active_entities: u32,
    pub breakdown: PerfRenderBreakdown,
    pub window_w: u32,
    pub window_h: u32,
    last_frame_ms: f32,
}

#[derive(Clone, Debug, Default, serde::Serialize)]
pub struct PerfSampleLine {
    pub fps: f32,
    pub frame_ms: f32,
    pub draw3d: u32,
    pub ui: u32,
    pub ents: u32,
}

#[derive(Resource, Default, Clone)]
pub struct PerfRingBuffer {
    pub samples: VecDeque<PerfSampleLine>,
}

impl PerfRingBuffer {
    pub const CAP: usize = 5;

    pub fn push(&mut self, sample: PerfSampleLine) {
        if self.samples.len() >= Self::CAP {
            self.samples.pop_front();
        }
        self.samples.push_back(sample);
    }
}

#[derive(Component)]
struct PerfOverlayRoot;

#[derive(Component)]
struct PerfOverlayText;

fn spawn_perf_overlay(mut commands: Commands) {
    commands
        .spawn((
            Node {
                position_type: PositionType::Absolute,
                left: Val::Px(8.0),
                bottom: Val::Px(88.0),
                display: Display::None,
                padding: UiRect::all(Val::Px(6.0)),
                border: UiRect::all(Val::Px(2.0)),
                ..default()
            },
            BorderColor(ui_theme::button_border(ui_theme::UiButtonTone::Primary)),
            BackgroundColor(ui_theme::panel_bg(0.72)),
            PerfOverlayRoot,
            Name::new("PerfOverlay"),
        ))
        .with_children(|panel| {
            panel.spawn((
                Text::new("PERF"),
                TextFont {
                    font_size: 11.0,
                    ..default()
                },
                TextColor(ui_theme::text_primary()),
                PerfOverlayText,
            ));
        });
}

/// Desktop: F3. Android/touch: MENU → PERF.
fn toggle_perf_overlay(
    keys: Res<ButtonInput<KeyCode>>,
    mut visible: ResMut<PerfOverlayVisible>,
    mut root: Query<&mut Node, With<PerfOverlayRoot>>,
) {
    if keys.just_pressed(KeyCode::F3) {
        visible.0 = !visible.0;
    }
    for mut node in &mut root {
        node.display = if visible.0 {
            Display::Flex
        } else {
            Display::None
        };
    }
}

/// Cheap every frame — no entity scans.
fn sample_frame_timing(
    time: Res<Time>,
    window: Query<&Window, With<PrimaryWindow>>,
    mut stats: ResMut<PerfStats>,
) {
    let dt_ms = time.delta_secs() * 1000.0;
    stats.frame_ms = dt_ms;
    stats.frame_jitter_ms = (dt_ms - stats.last_frame_ms).abs();
    stats.last_frame_ms = dt_ms;
    if dt_ms > 0.0 {
        stats.fps = 1000.0 / dt_ms;
    }
    if let Ok(win) = window.get_single() {
        stats.window_w = win.physical_width().max(1);
        stats.window_h = win.physical_height().max(1);
    }
}

/// Expensive mesh/UI/entity scan — ha-perf log every 5s; overlay refresh every 0.5s (not every frame).
fn sample_perf_breakdown(
    time: Res<Time>,
    visible: Res<PerfOverlayVisible>,
    mut timer: ResMut<PerfBreakdownTimer>,
    mut stats: ResMut<PerfStats>,
    meshes: Query<(
        &Mesh3d,
        Option<&Visibility>,
        Option<&Name>,
        Option<&Terrain>,
        Option<&EarthDecor>,
        Option<&InteractTarget>,
        Option<&OutlineShell>,
        Option<&crate::moon_world::MoonDecor>,
    )>,
    ui_nodes: Query<&Node>,
    all_entities: Query<Entity>,
) {
    timer.elapsed += time.delta_secs();
    timer.overlay_elapsed += time.delta_secs();
    let due_log = timer.elapsed >= 5.0;
    let due_overlay = visible.0 && timer.overlay_elapsed >= 0.5;
    if !due_log && !due_overlay {
        return;
    }
    if due_log {
        timer.elapsed = 0.0;
    }
    if due_overlay {
        timer.overlay_elapsed = 0.0;
    }

    let mut breakdown = PerfRenderBreakdown::default();
    let mut visible_count = 0u32;
    for (_, vis, name, terrain, earth_decor, interactable, outline, moon_decor) in &meshes {
        if vis.map(|v| *v != Visibility::Hidden).unwrap_or(true) {
            visible_count += 1;
            if outline.is_some() {
                breakdown.outline += 1;
            } else if interactable.is_some() {
                breakdown.interactables += 1;
            } else if terrain.is_some() || earth_decor.is_some() {
                breakdown.island_ground += 1;
            } else if moon_decor.is_some() {
                breakdown.chunk_decor += 1;
            } else if name
                .map(|n| n.as_str().contains("FloraBatch"))
                .unwrap_or(false)
            {
                breakdown.chunk_decor += 1;
            } else {
                breakdown.other_3d += 1;
            }
        }
    }
    breakdown.ui_nodes = ui_nodes.iter().count() as u32;
    stats.visible_meshes = visible_count;
    stats.draw_calls = visible_count;
    stats.active_entities = all_entities.iter().count() as u32;
    stats.breakdown = breakdown;
}

fn update_perf_overlay(
    visible: Res<PerfOverlayVisible>,
    stats: Res<PerfStats>,
    view: Res<CameraViewMode>,
    clear: Res<ClearColor>,
    game_cam: Query<&GlobalTransform, With<GameCamera>>,
    mut text: Query<&mut Text, With<PerfOverlayText>>,
) {
    if !visible.0 {
        return;
    }
    if let Ok(mut t) = text.get_single_mut() {
        let b = stats.breakdown;
        let cc = clear.0.to_srgba();
        let cam_pos = game_cam
            .get_single()
            .map(|tf| tf.translation())
            .unwrap_or(Vec3::ZERO);
        t.0 = format!(
            "FPS {:.0}  {:.1}ms  jitter {:.1}ms  {}x{}\n\
             cam={} pos=({:.1},{:.1},{:.1})\n\
             clear=({:.2},{:.2},{:.2})\n\
             3D~ {}  ui {}  ents {}\n\
             ocean {} ground {} static {} chunk {} obs {} nodes {} out {} other {}",
            stats.fps,
            stats.frame_ms,
            stats.frame_jitter_ms,
            stats.window_w,
            stats.window_h,
            view.label(),
            cam_pos.x,
            cam_pos.y,
            cam_pos.z,
            cc.red,
            cc.green,
            cc.blue,
            stats.draw_calls,
            b.ui_nodes,
            stats.active_entities,
            b.ocean,
            b.island_ground,
            b.static_world,
            b.chunk_decor,
            b.obstacles,
            b.interactables,
            b.outline,
            b.other_3d,
        );
    }
}

fn log_perf_periodic(
    time: Res<Time>,
    stats: Res<PerfStats>,
    span: Res<CpuSpanStats>,
    region: Res<crate::scene::RegionRuntime>,
    world_ctx: Res<crate::scene::WorldContextRuntime>,
    flight: Res<crate::rocket_flight::RocketFlight>,
    mut ring: ResMut<PerfRingBuffer>,
    mut log_timer: Local<f32>,
) {
    *log_timer += time.delta_secs();
    if *log_timer < 5.0 {
        return;
    }
    *log_timer = 0.0;
    // Breakdown refreshed by sample_perf_breakdown on same 5s cadence.
    let b = stats.breakdown;
    let sf = span.frames.max(1) as u64;
    ring.push(PerfSampleLine {
        fps: stats.fps,
        frame_ms: stats.frame_ms,
        draw3d: stats.draw_calls,
        ui: b.ui_nodes,
        ents: stats.active_entities,
    });
    eprintln!(
        "ha-perf fps={:.0} frame_ms={:.1} jitter={:.1} res={}x{} \
         region={:?} ctx={:?} flight={} \
         draw3d~={} ui={} ents={} \
         ocean={} ground={} chunk={} obs={} nodes={} outline={} other3d={} \
         span[cam={} chk={} tgt={} hud={} ptc={} atm={}]us",
        stats.fps,
        stats.frame_ms,
        stats.frame_jitter_ms,
        stats.window_w,
        stats.window_h,
        region.active,
        world_ctx.active,
        flight.active,
        stats.draw_calls,
        b.ui_nodes,
        stats.active_entities,
        b.ocean,
        b.island_ground,
        b.chunk_decor,
        b.obstacles,
        b.interactables,
        b.outline,
        b.other_3d,
        span.camera_us / sf,
        span.chunk_us / sf,
        span.target_us / sf,
        span.hud_us / sf,
        span.particles_us / sf,
        span.atmo_us / sf,
    );
}

/// Cap target frame rate to display refresh when available.
pub fn apply_android_frame_cap(mut window: Query<&mut Window, With<PrimaryWindow>>) {
    let Ok(mut win) = window.get_single_mut() else {
        return;
    };
    #[cfg(target_os = "android")]
    {
        win.present_mode = bevy::window::PresentMode::Fifo;
    }
    #[cfg(not(target_os = "android"))]
    {
        win.present_mode = bevy::window::PresentMode::AutoVsync;
    }
}
