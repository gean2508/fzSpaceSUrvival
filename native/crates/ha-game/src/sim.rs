//! Local player sim @ 20 Hz — flat-2D slice locomotion (Terraria-style strip
//! around each body; biblia §1 continuity in 2D).

use bevy::prelude::*;
use ha_shared::flat2d::{step_flat_creative_fly, step_flat_movement, PLAYER_HEIGHT};
use ha_shared::{
    collision_obstacles, generate_world, parkour_button_hint, CelestialBody, InputState,
    ParkourButtonHint, WorldContext,
};

use crate::game_time::configure_game_clock;
use crate::game_time::fixed_delta_secs;
use crate::input::{EquippedItem, HoldInteractState, MovementIntent, TouchActions};
use crate::rocket_flight::RocketFlight;
use crate::scene::WorldContextRuntime;
use crate::world2d::{active_surface_body, ActivePlanet, TileWorld};

/// Legacy anchor kept for compat (old cave interior origin).
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
    /// Hunger / thirst / oxygen bars — ticked by `survival_stats.rs`.
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
    hold: Res<HoldInteractState>,
    equipped: Res<EquippedItem>,
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
    let gravity_scale = ha_shared::gravity_scale_for_context(world_ctx.active);
    let body_surface = active_surface_body(&world_ctx, &planet_ctx);

    for (mut sim, mut visual) in &mut query {
        visual.from_x = visual.to_x;
        visual.from_z = visual.to_z;
        visual.from_y = visual.to_y;
        visual.from_rot = visual.to_rot;

        let action_now = actions.action;
        let hint = parkour_button_hint(&sim.body);
        let climb_mode = matches!(hint, ParkourButtonHint::Climb | ParkourButtonHint::Mantle);
        let jump_pressed = (actions.jump || actions.jump_edge)
            && !climb_mode
            && (!hold.active || hold.flight_hold);
        sim.prev_action = action_now;

        let input = InputState {
            seq: 0,
            move_x: intent.move_x,
            move_z: intent.move_z,
            aim_x: intent.aim_x,
            aim_z: intent.aim_z,
            abilities: actions.abilities,
            sprint: if intent.sprint { 1 } else { 0 },
            action: if action_now { 1 } else { 0 },
        };

        match world_ctx.active {
            WorldContext::Space => {
                // In vacuum outside a rocket there is nothing to walk on —
                // the rocket flight owns movement (context only reachable
                // mid-flight).
            }
            _ if editor.creative => {
                step_flat_creative_fly(&mut sim.body, &input, dt, actions.jump, intent.sprint);
            }
            _ => {
                let solid = |i: i64, j: i32| tiles.solid_at(body_surface, i, j);
                step_flat_movement(
                    body_surface,
                    &mut sim.body,
                    &input,
                    dt,
                    jump_pressed,
                    gravity_scale,
                    &solid,
                );
                // Jetpack is a universal movement tool — usable on ANY body.
                if equipped.item_id == JETPACK_ITEM {
                    if jump_pressed && sim.body.coyote_sec > 0.0 {
                        sim.body.vy = sim.body.vy.max(9.5);
                    }
                    if (actions.action || actions.jump) && sim.body.vy > 0.0 {
                        sim.body.vy += 14.0 * dt;
                    }
                }
            }
        }
        sim.body.z = 0.0;

        visual.to_x = sim.body.x;
        visual.to_z = 0.0;
        visual.to_y = sim.body.y;
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
        // In flight the rocket sprite is the avatar; hide the walker.
        for (_, _, mut vis) in &mut query {
            *vis = Visibility::Hidden;
        }
        return;
    }

    for (visual, mut transform, mut vis) in &mut query {
        *vis = Visibility::Visible;
        let x = visual.from_x.lerp(visual.to_x, t);
        let y = visual.from_y.lerp(visual.to_y, t);
        transform.translation = Vec3::new(x, y + PLAYER_HEIGHT * 0.5, 2.0);
        // rot encodes facing: flip the sprite for -s.
        let facing_left = visual.to_rot.abs() > std::f32::consts::FRAC_PI_2;
        transform.rotation = Quat::IDENTITY;
        transform.scale = Vec3::new(if facing_left { -1.0 } else { 1.0 }, 1.0, 1.0);
    }
}

/// Ground height (m) on Earth at arc position `s` — natural terrain only.
pub fn earth_surface_y(tx: f32, _tz: f32) -> f32 {
    ha_shared::flat2d::ground_height_m(CelestialBody::Earth, tx)
}
