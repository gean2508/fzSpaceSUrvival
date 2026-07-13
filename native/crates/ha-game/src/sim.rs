//! Local player sim @ 20 Hz — top-down 3/4 locomotion on the XZ ground plane.
//! `body.x`/`body.z` are the ground plane; render maps to screen `(x, -z)`.

use bevy::prelude::*;
use ha_shared::topdown::step_topdown_movement;
use ha_shared::{collision_obstacles, generate_world, InputState, WorldContext};

use crate::game_time::configure_game_clock;
use crate::game_time::fixed_delta_secs;
use crate::input::{MovementIntent, TouchActions};
use crate::rocket_flight::RocketFlight;
use crate::scene::WorldContextRuntime;
use crate::world2d::{active_surface_body, render_xy, sort_z, ActivePlanet, TileWorld};

/// Legacy anchor kept for compat.
pub const CAVE_INTERIOR_ORIGIN: Vec3 = Vec3::new(0.0, -22.0, 0.0);
pub const JETPACK_ITEM: &str = "jetpack";

#[derive(Component)]
pub struct Player;

#[derive(Resource, Clone)]
pub struct WorldCollision(pub Vec<ha_shared::types::Obstacle>);

pub struct SimPlugin;

impl Plugin for SimPlugin {
    fn build(&self, app: &mut App) {
        let world = generate_world(42);
        app.insert_resource(Time::<Fixed>::from_hz(ha_shared::TICK_RATE as f64))
            .insert_resource(WorldLayout(world.clone()))
            .insert_resource(WorldCollision(collision_obstacles(&world)))
            .add_systems(Startup, configure_game_clock)
            .add_systems(FixedUpdate, tick_sim)
            .add_systems(Update, interpolate_player_visual);
    }
}

#[derive(Resource, Clone)]
pub struct WorldLayout(pub ha_shared::GeneratedWorld);

#[derive(Component)]
pub struct SimState {
    pub body: ha_shared::MovementBody,
    pub hp: f32,
    pub max_hp: f32,
    pub survival: ha_shared::SurvivalStats,
    pub prev_action: bool,
    pub prev_jump: bool,
}

#[derive(Component, Default)]
pub struct PlayerVisual {
    pub from_x: f32,
    pub from_z: f32,
    pub from_y: f32,
    pub from_rot: f32,
    pub to_x: f32,
    pub to_z: f32,
    pub to_y: f32,
    pub to_rot: f32,
    pub radius: f32,
}

fn tick_sim(
    time: Res<Time<Fixed>>,
    intent: Res<MovementIntent>,
    actions: Res<TouchActions>,
    flight: Res<RocketFlight>,
    world_ctx: Res<WorldContextRuntime>,
    planet_ctx: Res<ActivePlanet>,
    tiles: Res<TileWorld>,
    editor: Res<crate::ui_editor::UiEditorState>,
    mut query: Query<(&mut SimState, &mut PlayerVisual), With<Player>>,
) {
    if flight.blocks_walking() {
        return;
    }
    let dt = fixed_delta_secs(&time);
    let body_surface = active_surface_body(&world_ctx, &planet_ctx);

    for (mut sim, mut visual) in &mut query {
        visual.from_x = visual.to_x;
        visual.from_z = visual.to_z;
        visual.from_y = visual.to_y;
        visual.from_rot = visual.to_rot;

        sim.prev_action = actions.action;

        let input = InputState {
            seq: 0,
            move_x: intent.move_x,
            move_z: intent.move_z,
            aim_x: intent.aim_x,
            aim_z: intent.aim_z,
            abilities: actions.abilities,
            sprint: if intent.sprint { 1 } else { 0 },
            action: if actions.action { 1 } else { 0 },
        };

        match world_ctx.active {
            WorldContext::Space => {
                // In vacuum the rocket owns movement.
            }
            _ if editor.creative => {
                // Creative: free 8-dir move with no collision (fly over water/rock).
                let no_solid = |_tx: i64, _tz: i64| false;
                step_topdown_movement(&mut sim.body, &input, dt, &no_solid);
            }
            _ => {
                let solid = |tx: i64, tz: i64| tiles.solid_at(body_surface, tx, tz);
                step_topdown_movement(&mut sim.body, &input, dt, &solid);
            }
        }
        sim.body.y = 0.0;

        visual.to_x = sim.body.x;
        visual.to_z = sim.body.z;
        visual.to_y = 0.0;
        visual.to_rot = sim.body.rot;
        visual.radius = sim.body.radius;
    }
}

fn interpolate_player_visual(
    time: Res<Time<Fixed>>,
    flight: Res<RocketFlight>,
    mut query: Query<(&PlayerVisual, &mut Transform, &mut Visibility), With<Player>>,
) {
    let t = time.overstep_fraction() as f32;
    if flight.active && flight.phase != ha_shared::FlightPhase::Surface {
        for (_, _, mut vis) in &mut query {
            *vis = Visibility::Hidden;
        }
        return;
    }

    for (visual, mut transform, mut vis) in &mut query {
        *vis = Visibility::Visible;
        let x = visual.from_x.lerp(visual.to_x, t);
        let z = visual.from_z.lerp(visual.to_z, t);
        let r = render_xy(x, z);
        // Player sorts by its own z (+1 so it draws above same-cell ground decor).
        transform.translation = Vec3::new(r.x, r.y, sort_z(z) + 1.0);
        // Facing: flip horizontally when moving west (left half of the circle).
        let facing_left = visual.to_rot.abs() > std::f32::consts::FRAC_PI_2;
        transform.rotation = Quat::IDENTITY;
        transform.scale = Vec3::new(if facing_left { -1.0 } else { 1.0 }, 1.0, 1.0);
    }
}

/// Compat shim: top-down ground is flat (y=0).
pub fn earth_surface_y(_tx: f32, _tz: f32) -> f32 {
    0.0
}
