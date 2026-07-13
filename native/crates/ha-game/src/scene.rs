//! Region/context runtime + player spawn for the 2D world.
//!
//! `WorldContext` (gravity/locomotion) vs `RegionId` (render/visibility) keep
//! their old meanings; transitions still only happen in `rocket_flight.rs`
//! (B8) or via deliberate MENU teleports (`teleport_to_body`).

use bevy::prelude::*;
use ha_shared::topdown::{topdown_tile, Tile, TILE_M};
use ha_shared::{CelestialBody, WorldContext};

use crate::sim::{Player, PlayerVisual, SimState};
use crate::world2d::{render_xy, sort_z};

/// Find a walkable (non-solid, non-water) surface cell near a preferred world
/// point, spiralling outward. Returns world `(x, z)` metres (cell centre).
pub fn walkable_spawn(body: CelestialBody, near_x: f32, near_z: f32) -> (f32, f32) {
    let bx = (near_x / TILE_M).floor() as i64;
    let bz = (near_z / TILE_M).floor() as i64;
    for r in 0i64..64 {
        for dz in -r..=r {
            for dx in -r..=r {
                if dx.abs() != r && dz.abs() != r {
                    continue; // only the ring at radius r
                }
                let (tx, tz) = (bx + dx, bz + dz);
                let t = topdown_tile(body, 0, tx, tz);
                if !t.solid() && t != Tile::Water {
                    return (tx as f32 * TILE_M + 0.5, tz as f32 * TILE_M + 0.5);
                }
            }
        }
    }
    (near_x, near_z)
}

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

fn spawn_player(mut commands: Commands, art: Res<crate::art2d::ArtLibrary>) {
    let (sx, sz) = walkable_spawn(CelestialBody::Earth, EARTH_PAD_S + 4.0, 6.0);
    let mut body = ha_shared::MovementBody::new(5.2, ha_shared::topdown::PLAYER_RADIUS);
    body.x = sx;
    body.z = sz;
    body.y = 0.0;

    let sim = SimState {
        body,
        hp: 100.0,
        max_hp: 100.0,
        survival: ha_shared::SurvivalStats::default(),
        prev_action: false,
        prev_jump: false,
    };
    let mut visual = PlayerVisual {
        radius: ha_shared::topdown::PLAYER_RADIUS,
        ..default()
    };
    visual.from_x = sx;
    visual.to_x = sx;
    visual.from_z = sz;
    visual.to_z = sz;
    let r = render_xy(sx, sz);
    let tf = Transform::from_xyz(r.x, r.y, sort_z(sz) + 1.0);

    // Drop-in LimeZu character (assets/art/player/astronaut.png) or a
    // procedural top-down avatar (body + head).
    if let Some(image) = art.get("player/astronaut") {
        let mut sprite = Sprite::from_image(image);
        sprite.custom_size = Some(Vec2::new(1.4, 2.0));
        commands.spawn((Player, sim, visual, sprite, tf));
        return;
    }
    commands
        .spawn((
            Player,
            sim,
            visual,
            Sprite::from_color(Color::srgb(0.92, 0.55, 0.25), Vec2::new(0.8, 1.1)),
            tf,
        ))
        .with_children(|p| {
            // Head (top of the 3/4 body).
            p.spawn((
                Sprite::from_color(Color::srgb(0.98, 0.85, 0.7), Vec2::new(0.6, 0.6)),
                Transform::from_xyz(0.0, 0.7, 0.05),
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
    let near_x = if body == CelestialBody::Earth {
        EARTH_PAD_S + 4.0
    } else {
        0.0
    };
    let (sx, sz) = walkable_spawn(body, near_x, 6.0);
    sim.body.x = sx;
    sim.body.z = sz;
    sim.body.y = 0.0;
    sim.body.vx = 0.0;
    sim.body.vz = 0.0;
    sim.body.vy = 0.0;
    visual.from_x = sx;
    visual.to_x = sx;
    visual.from_z = sz;
    visual.to_z = sz;
    visual.from_y = 0.0;
    visual.to_y = 0.0;
}
