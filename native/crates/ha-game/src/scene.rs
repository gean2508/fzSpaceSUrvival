//! Region/context runtime + player spawn for the 2D world.
//!
//! `WorldContext` (gravity/locomotion) vs `RegionId` (render/visibility) keep
//! their old meanings; transitions still only happen in `rocket_flight.rs`
//! (B8) or via deliberate MENU teleports (`teleport_to_body`).

use bevy::prelude::*;
use ha_shared::flat2d::ground_height_m;
use ha_shared::{CelestialBody, WorldContext};

use crate::sim::{Player, PlayerVisual, SimState};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum RegionId {
    Overworld { gx: i8, gz: i8 },
    /// Legacy cave instance — superseded in 2D (digging IS the cave); kept so
    /// old persistence/UI code keeps compiling until content moves over.
    Cave(u8),
    Space,
    Luna,
    Planet(CelestialBody),
    Transit,
}

#[derive(Resource, Default)]
pub struct WorldContextRuntime {
    pub active: WorldContext,
}

/// Launch ascent blend consumed by the sky tint (0 pad → 1 void).
#[derive(Resource, Default)]
pub struct LaunchAtmosphere {
    pub active: bool,
    pub blend: f32,
}

impl LaunchAtmosphere {
    pub fn atmosphere_alpha(&self) -> f32 {
        if !self.active {
            return 1.0;
        }
        (1.0 - self.blend).clamp(0.0, 1.0)
    }
}

#[derive(Resource)]
pub struct RegionRuntime {
    pub active: RegionId,
    pub cave_portal: Vec3,
    pub edge_pressed: bool,
    pub cave_surface_return: Option<Vec3>,
    pub cave_return_body: CelestialBody,
}

impl Default for RegionRuntime {
    fn default() -> Self {
        Self {
            active: RegionId::Overworld { gx: 0, gz: 0 },
            cave_portal: Vec3::new(11.0, 0.0, -9.5),
            edge_pressed: false,
            cave_surface_return: None,
            cave_return_body: CelestialBody::Earth,
        }
    }
}

/// Arc position of the agency launch pad on Earth (spawn/respawn anchor).
pub const EARTH_PAD_S: f32 = ha_shared::LAUNCH_PAD_ZONE_X;

pub struct ScenePlugin;

impl Plugin for ScenePlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<WorldContextRuntime>()
            .init_resource::<RegionRuntime>()
            .init_resource::<LaunchAtmosphere>()
            .add_systems(Startup, spawn_player);
    }
}

fn spawn_player(mut commands: Commands) {
    let spawn_s = EARTH_PAD_S + 12.0;
    let mut body = ha_shared::MovementBody::new(5.2, 0.35);
    body.x = spawn_s;
    body.y = ground_height_m(CelestialBody::Earth, spawn_s) + 0.1;

    commands
        .spawn((
            Player,
            SimState {
                body,
                hp: 100.0,
                max_hp: 100.0,
                survival: ha_shared::SurvivalStats::default(),
                prev_action: false,
                prev_jump: false,
            },
            PlayerVisual::default(),
            // Astronaut suit: torso sprite + helmet child.
            Sprite::from_color(Color::srgb(0.92, 0.55, 0.25), Vec2::new(0.8, 1.7)),
            Transform::from_xyz(spawn_s, 4.0, 2.0),
        ))
        .with_children(|p| {
            p.spawn((
                Sprite::from_color(Color::srgb(0.85, 0.9, 0.95), Vec2::new(0.62, 0.5)),
                Transform::from_xyz(0.08, 0.62, 0.05),
            ));
            p.spawn((
                Sprite::from_color(Color::srgb(0.25, 0.28, 0.34), Vec2::new(0.5, 0.35)),
                Transform::from_xyz(0.0, -0.15, 0.04),
            ));
        });
}

/// Deliberate MENU teleport (creative/dev): put the walker on a body surface.
pub fn teleport_to_body(
    body: CelestialBody,
    region: &mut RegionRuntime,
    ctx: &mut WorldContextRuntime,
    planet: &mut crate::world2d::ActivePlanet,
    sim: &mut SimState,
    visual: &mut PlayerVisual,
) {
    let (target_region, target_ctx) = match body {
        CelestialBody::Earth => (RegionId::Overworld { gx: 0, gz: 0 }, WorldContext::Overworld),
        CelestialBody::Moon => (RegionId::Luna, WorldContext::Luna),
        other => (RegionId::Planet(other), WorldContext::Planet),
    };
    region.active = target_region;
    ctx.active = target_ctx;
    if !matches!(body, CelestialBody::Earth | CelestialBody::Moon) {
        planet.body = Some(body);
    }
    let s = if body == CelestialBody::Earth {
        EARTH_PAD_S + 12.0
    } else {
        6.0
    };
    sim.body.x = s;
    sim.body.y = ground_height_m(body, s) + 0.2;
    sim.body.vx = 0.0;
    sim.body.vy = 0.0;
    visual.from_x = s;
    visual.to_x = s;
    visual.from_y = sim.body.y;
    visual.to_y = sim.body.y;
}
