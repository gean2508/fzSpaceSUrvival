//! Walkable sphere locomotion — Earth, Moon, and visitable planets (SFS-style surfaces).

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

pub fn step_surface_movement(
    surface: CelestialBody,
    body: &mut MovementBody,
    input: &InputState,
    dt: f32,
    jump_pressed: bool,
    gravity_scale: f32,
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

    if jump_pressed && body.coyote_sec > 0.0 && body.stamina >= JUMP_STAMINA_COST {
        let ground = planet_ground_y_at(surface, body.x, body.z);
        if body.y <= ground + 0.12 {
            body.vy = JUMP_VELOCITY;
            body.stamina = (body.stamina - JUMP_STAMINA_COST).max(0.0);
            body.coyote_sec = 0.0;
        }
    }

    apply_surface_gravity(surface, body, dt, gravity_scale);

    // Stick to ground only when descending near the surface — not while rising.
    if body.vy <= 0.0 {
        follow_surface_ground(surface, body, prev_x, prev_z);
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

fn apply_surface_gravity(
    surface: CelestialBody,
    b: &mut MovementBody,
    dt: f32,
    gravity_scale: f32,
) {
    let ground = planet_ground_y_at(surface, b.x, b.z);
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
