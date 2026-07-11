//! Walkable sphere locomotion — Earth, Moon, and visitable planets (SFS-style surfaces).

use crate::blocks::{PlacedBlockWorld, BLOCK_STEP_UP};
use crate::celestial_bodies::CelestialBody;
use crate::constants::{
    COYOTE_TIME_SECS, FRICTION, GRAVITY, JUMP_STAMINA_COST, JUMP_VELOCITY, SLOPE_SAMPLE, SPRINT_MULT,
    STAMINA_MAX, STAMINA_REGEN, STAMINA_SPRINT_DRAIN,
};
use crate::input::InputState;
use crate::movement::MovementBody;
use crate::planet_surface::{
    block_surface_steep_step, block_surface_unwalkable_ascent, follow_surface_ground,
    planet_ground_y_at, sphere_tangent_step, surface_slope_at,
};
use crate::types::{MOV_IDLE, MOV_RUNNING, MOV_SPRINTING};

/// Terrain height plus any player-placed block top the body can stand on.
pub fn effective_ground_y(
    surface: CelestialBody,
    tx: f32,
    tz: f32,
    body_y: f32,
    blocks: Option<&PlacedBlockWorld>,
) -> f32 {
    let mut g = planet_ground_y_at(surface, tx, tz);
    if let Some(bw) = blocks {
        if let Some(top) = bw.ground_at(tx, tz, body_y + BLOCK_STEP_UP) {
            g = g.max(top);
        }
    }
    g
}

pub fn step_surface_movement(
    surface: CelestialBody,
    body: &mut MovementBody,
    input: &InputState,
    dt: f32,
    jump_pressed: bool,
    gravity_scale: f32,
) {
    step_surface_movement_with_blocks(surface, body, input, dt, jump_pressed, gravity_scale, None)
}

/// Full surface step including collision with player-placed voxel blocks.
/// Creative/dev free-flight: same horizontal sphere locomotion, but no gravity
/// and manual vertical control (ascend/descend). Lets the player fly straight
/// up to test the space transition without building a rocket.
pub fn step_surface_creative_fly(
    surface: CelestialBody,
    body: &mut MovementBody,
    input: &InputState,
    dt: f32,
    ascend: bool,
    descend: bool,
) {
    const FLY_SPEED: f32 = 16.0;
    const FLY_HORIZ_MULT: f32 = 1.6;

    let mut mx = input.move_x;
    let mut mz = input.move_z;
    let move_len = (mx * mx + mz * mz).sqrt();
    if move_len > 1.0 {
        mx /= move_len;
        mz /= move_len;
    }
    if input.aim_x != 0.0 || input.aim_z != 0.0 {
        body.rot = input.aim_x.atan2(input.aim_z);
    } else if move_len > 0.01 {
        body.rot = mx.atan2(mz);
    }
    sphere_ground_locomotion(body, surface, mx, mz, FLY_HORIZ_MULT, dt);

    let mut vy = 0.0;
    if ascend {
        vy += FLY_SPEED;
    }
    if descend {
        vy -= FLY_SPEED;
    }
    body.vy = vy;
    body.y += vy * dt;
    let ground = planet_ground_y_at(surface, body.x, body.z);
    if body.y < ground {
        body.y = ground;
        body.vy = 0.0;
    }
    body.state = MOV_IDLE;
    body.stamina = STAMINA_MAX; // no stamina drain in creative
}

pub fn step_surface_movement_with_blocks(
    surface: CelestialBody,
    body: &mut MovementBody,
    input: &InputState,
    dt: f32,
    jump_pressed: bool,
    gravity_scale: f32,
    blocks: Option<&PlacedBlockWorld>,
) {
    if body.max_stamina <= 0.0 {
        body.max_stamina = STAMINA_MAX;
    }
    if body.stamina <= 0.0 && body.state == MOV_IDLE {
        body.stamina = STAMINA_MAX;
    }

    let mut mx = input.move_x;
    let mut mz = input.move_z;
    let move_len = (mx * mx + mz * mz).sqrt();
    if move_len > 1.0 {
        mx /= move_len;
        mz /= move_len;
    }

    if input.aim_x != 0.0 || input.aim_z != 0.0 {
        body.rot = input.aim_x.atan2(input.aim_z);
    } else if move_len > 0.01 {
        body.rot = mx.atan2(mz);
    }

    let wants_sprint = input.sprint == 1 && move_len > 0.1 && body.stamina > 0.0;
    let speed_mult = if wants_sprint { SPRINT_MULT } else { 1.0 };
    let prev_x = body.x;
    let prev_z = body.z;

    sphere_ground_locomotion(body, surface, mx, mz, speed_mult, dt);
    block_surface_steep_step(body, surface, prev_x, prev_z);
    block_surface_unwalkable_ascent(body, surface, prev_x, prev_z);

    // Placed voxel blocks: solid side collision (a step over BLOCK_STEP_UP
    // cancels the move — jump to climb, exactly like terrain steep-steps).
    if let Some(bw) = blocks {
        if bw.blocks_capsule(body.x, body.z, body.y) {
            body.x = prev_x;
            body.z = prev_z;
            body.vx = 0.0;
            body.vz = 0.0;
        }
    }

    if jump_pressed && body.coyote_sec > 0.0 && body.stamina >= JUMP_STAMINA_COST {
        let ground = effective_ground_y(surface, body.x, body.z, body.y, blocks);
        if body.y <= ground + 0.12 {
            body.vy = JUMP_VELOCITY;
            body.stamina = (body.stamina - JUMP_STAMINA_COST).max(0.0);
            body.coyote_sec = 0.0;
        }
    }

    apply_surface_gravity_ex(surface, body, dt, gravity_scale, blocks);

    // Stick to ground only when descending near the surface — not while rising.
    // Standing on a block keeps the body above terrain snap range on its own.
    if body.vy <= 0.0 {
        let block_ground = blocks
            .and_then(|bw| bw.ground_at(body.x, body.z, body.y + BLOCK_STEP_UP));
        match block_ground {
            Some(top) if body.y <= top + 0.14 => {
                body.y = top;
                body.vy = 0.0;
            }
            _ => follow_surface_ground(surface, body, prev_x, prev_z),
        }
    }

    if wants_sprint {
        body.stamina = (body.stamina - STAMINA_SPRINT_DRAIN * dt).max(0.0);
    } else {
        body.stamina = (body.stamina + STAMINA_REGEN * dt).min(body.max_stamina);
    }

    body.state = if move_len > 0.1 {
        if wants_sprint {
            MOV_SPRINTING
        } else {
            MOV_RUNNING
        }
    } else {
        MOV_IDLE
    };
}

fn sphere_ground_locomotion(
    b: &mut MovementBody,
    surface: CelestialBody,
    mx: f32,
    mz: f32,
    mult: f32,
    dt: f32,
) {
    let target_vx = mx * b.speed * mult;
    let target_vz = mz * b.speed * mult;
    let moving = mx.abs() + mz.abs() > 0.05;
    let rate = if moving { 20.0 } else { FRICTION };
    let k = (1.0f32).min(rate * dt);
    b.vx += (target_vx - b.vx) * k;
    b.vz += (target_vz - b.vz) * k;

    let (tx, tz) = sphere_tangent_step(surface, b.x, b.z, b.vx * dt, b.vz * dt);
    b.x = tx;
    b.z = tz;
}

fn apply_surface_gravity_ex(
    surface: CelestialBody,
    b: &mut MovementBody,
    dt: f32,
    gravity_scale: f32,
    blocks: Option<&PlacedBlockWorld>,
) {
    let ground = effective_ground_y(surface, b.x, b.z, b.y, blocks);
    let slope = surface_slope_at(surface, b.x, b.z, SLOPE_SAMPLE);
    let snap_eps = 0.08 + slope.min(1.0) * 0.1;
    let grounded = b.y <= ground + snap_eps && b.vy <= 0.05;
    if grounded {
        b.coyote_sec = COYOTE_TIME_SECS;
    } else {
        b.coyote_sec = (b.coyote_sec - dt).max(0.0);
    }

    if b.y > ground + 0.001 || b.vy > 0.001 {
        b.vy -= GRAVITY * gravity_scale * dt;
        b.y += b.vy * dt;
        if b.y <= ground {
            b.y = ground;
            b.vy = 0.0;
        }
    } else {
        b.y = ground;
        b.vy = 0.0;
    }
}
