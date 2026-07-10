//! Local player sim @ 20 Hz — sphere surface locomotion (SFS worlds).

use bevy::prelude::*;
use ha_shared::{
    capsule_center_above_feet, collision_obstacles, generate_world, ground_height_for_locomotion,
    parkour_button_hint, step_surface_movement, CelestialBody, InputState, ParkourButtonHint,
    WorldContext,
};

use crate::game_time::fixed_delta_secs;
use crate::game_time::configure_game_clock;
use crate::input::{EquippedItem, HoldInteractState, MovementIntent, TouchActions};
use crate::jetpack_transit::{JetpackTransit, JETPACK_ITEM};
use crate::rocket_flight::RocketFlight;
use crate::planet_world::ActivePlanet;
use crate::scene::WorldContextRuntime;

/// Underground mission floor when cave interiors return.
pub const CAVE_INTERIOR_ORIGIN: Vec3 = Vec3::new(0.0, -22.0, 0.0);

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

pub fn earth_surface_y(tx: f32, tz: f32) -> f32 {
    ground_height_for_locomotion(tx, tz, 0.35)
}

#[derive(Component)]
pub struct SimState {
    pub body: ha_shared::MovementBody,
    pub hp: f32,
    pub max_hp: f32,
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

fn surface_body(ctx: &WorldContext, planet: &ActivePlanet) -> CelestialBody {
    match ctx {
        WorldContext::Luna => CelestialBody::Moon,
        WorldContext::Planet => planet.body.unwrap_or(CelestialBody::Mars),
        _ => CelestialBody::Earth,
    }
}

fn tick_sim(
    time: Res<Time<Fixed>>,
    intent: Res<MovementIntent>,
    actions: Res<TouchActions>,
    hold: Res<HoldInteractState>,
    equipped: Res<EquippedItem>,
    transit: Res<JetpackTransit>,
    flight: Res<RocketFlight>,
    world_ctx: Res<WorldContextRuntime>,
    planet_ctx: Res<ActivePlanet>,
    placed_blocks: Res<crate::block_world::PlacedBlocks>,
    editor: Res<crate::ui_editor::UiEditorState>,
    mut query: Query<(&mut SimState, &mut PlayerVisual), With<Player>>,
) {
    if transit.active || flight.blocks_walking() {
        return;
    }
    let dt = fixed_delta_secs(&time);
    let gravity_scale = ha_shared::gravity_scale_for_context(world_ctx.active);
    let body_surface = surface_body(&world_ctx.active, &planet_ctx);

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

        let parkour_action = if climb_mode && actions.jump {
            1
        } else if action_now {
            1
        } else {
            0
        };

        let input = InputState {
            seq: 0,
            move_x: intent.move_x,
            move_z: intent.move_z,
            aim_x: intent.aim_x,
            aim_z: intent.aim_z,
            abilities: actions.abilities,
            sprint: if intent.sprint { 1 } else { 0 },
            action: parkour_action,
        };

        match world_ctx.active {
            WorldContext::Mission => {
                sim.body.x = sim.body.x.clamp(-12.0, 12.0);
                sim.body.z = sim.body.z.clamp(-12.0, 12.0);
                let floor = CAVE_INTERIOR_ORIGIN.y + 1.1;
                if sim.body.y < floor {
                    sim.body.y = floor;
                    sim.body.vy = 0.0;
                }
            }
            WorldContext::Space => {
                // Walk zone centred on the dock — the old ±6 around the world
                // origin was ~130 m away from the SpaceDecor spawned at
                // `space_dock_origin()` (28, 128, -28).
                let dock = crate::space_travel::space_dock_origin();
                sim.body.x = sim.body.x.clamp(dock.x - 6.0, dock.x + 6.0);
                sim.body.z = sim.body.z.clamp(dock.z - 6.0, dock.z + 6.0);
                let floor = dock.y + 0.12;
                if sim.body.y < floor {
                    sim.body.y = floor;
                    sim.body.vy = 0.0;
                }
            }
            WorldContext::Luna | WorldContext::Planet | WorldContext::Overworld
                if editor.creative =>
            {
                // Creative fly: JUMP ascends, sprint descends, no gravity —
                // fly straight up to reach space without building a rocket.
                ha_shared::step_surface_creative_fly(
                    body_surface,
                    &mut sim.body,
                    &input,
                    dt,
                    actions.jump,
                    intent.sprint,
                );
            }
            WorldContext::Luna | WorldContext::Planet | WorldContext::Overworld => {
                ha_shared::step_surface_movement_with_blocks(
                    body_surface,
                    &mut sim.body,
                    &input,
                    dt,
                    jump_pressed,
                    gravity_scale,
                    placed_blocks.for_body(body_surface),
                );
                // Jetpack is a universal movement tool — usable on ANY walkable
                // body (Tierra, Luna, planetas), not just the overworld. Same
                // hop-and-sustain feel everywhere; the lower gravity on airless
                // bodies just makes each burn carry farther.
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

        visual.to_x = sim.body.x;
        visual.to_z = sim.body.z;
        visual.to_y = sim.body.y;
        visual.to_rot = sim.body.rot;
        visual.radius = sim.body.radius;
    }
}

fn interpolate_player_visual(
    time: Res<Time<Fixed>>,
    world_ctx: Res<WorldContextRuntime>,
    planet_ctx: Res<ActivePlanet>,
    flight: Res<RocketFlight>,
    origin: Res<crate::floating_origin::FloatingOrigin>,
    mut query: Query<(&PlayerVisual, &mut Transform), With<Player>>,
) {
    let t = time.overstep_fraction() as f32;
    if flight.active && flight.phase != ha_shared::FlightPhase::Surface {
        let (pos, rot) = crate::rocket_flight::flight_world_transform(&flight, &origin);
        for (_, mut transform) in &mut query {
            transform.translation = pos;
            transform.rotation = rot;
        }
        return;
    }

    let on_sphere = matches!(
        world_ctx.active,
        WorldContext::Overworld | WorldContext::Luna | WorldContext::Planet
    );
    let sphere_body = surface_body(&world_ctx.active, &planet_ctx);

    for (visual, mut transform) in &mut query {
        let x = visual.from_x.lerp(visual.to_x, t);
        let z = visual.from_z.lerp(visual.to_z, t);
        let y = visual.from_y.lerp(visual.to_y, t);
        let rot = lerp_angle(visual.from_rot, visual.to_rot, t);

        if on_sphere {
            let r = ha_shared::body_walk_radius(sphere_body);
            let dir = ha_shared::planet_dir_from_tangent(sphere_body, x, z);
            let disp = ha_shared::planet_displacement_at_dir(sphere_body, dir);
            let center = Vec3::from_array(ha_shared::planet_gameplay_center(sphere_body));
            let normal = Vec3::new(dir[0], dir[1], dir[2]).normalize_or_zero();
            let surf = center + normal * (r + disp);
            let ground = ha_shared::planet_ground_y_at(sphere_body, x, z);
            let hop = (y - ground).max(0.0);
            transform.translation = surf + normal * (visual.radius * 1.1 + hop);
            let align = Quat::from_rotation_arc(Vec3::Y, normal);
            transform.rotation = align * Quat::from_rotation_y(rot);
        } else {
            let center_y = capsule_center_above_feet(visual.radius);
            transform.translation = Vec3::new(x, y + center_y, z);
            transform.rotation = Quat::from_rotation_y(rot);
        }
    }
}

fn lerp_angle(from: f32, to: f32, t: f32) -> f32 {
    let mut delta = to - from;
    while delta > std::f32::consts::PI {
        delta -= std::f32::consts::TAU;
    }
    while delta < -std::f32::consts::PI {
        delta += std::f32::consts::TAU;
    }
    from + delta * t
}
