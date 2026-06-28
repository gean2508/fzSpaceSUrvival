//! World transforms for walkable sphere surfaces (Earth, Moon, planets).

use bevy::prelude::*;
use ha_shared::{
    body_walk_radius, capsule_center_above_feet, planet_dir_from_tangent,
    planet_displacement_at_dir, planet_gameplay_center, planet_gameplay_world_pos,
    planet_ground_y_at, surface_relief_at, CelestialBody, WorldContext,
};

pub fn on_sphere_context(ctx: WorldContext) -> bool {
    matches!(
        ctx,
        WorldContext::Overworld | WorldContext::Luna | WorldContext::Planet
    )
}

pub fn surface_body_for_context(ctx: WorldContext, planet: CelestialBody) -> CelestialBody {
    match ctx {
        WorldContext::Luna => CelestialBody::Moon,
        WorldContext::Planet => planet,
        _ => CelestialBody::Earth,
    }
}

/// Radial meters above the displaced shell — NOT `planet_ground_y_at` / sim `body.y`.
pub const PAD_SURFACE_LIFT: f32 = 0.18;
pub const MACHINE_SURFACE_LIFT: f32 = 0.35;

pub fn sphere_world_pos(body: CelestialBody, tx: f32, tz: f32, lift_m: f32) -> Vec3 {
    Vec3::from_array(planet_gameplay_world_pos(body, tx, tz, lift_m))
}

pub fn sphere_surface_normal(body: CelestialBody, tx: f32, tz: f32) -> Vec3 {
    let dir = planet_dir_from_tangent(body, tx, tz);
    Vec3::new(dir[0], dir[1], dir[2]).normalize_or_zero()
}

pub fn sphere_surface_transform(
    body: CelestialBody,
    tx: f32,
    tz: f32,
    lift_above_surface: f32,
    yaw: f32,
) -> Transform {
    let normal = sphere_surface_normal(body, tx, tz);
    let rot = Quat::from_rotation_arc(Vec3::Y, normal) * Quat::from_rotation_y(yaw);
    Transform {
        translation: sphere_world_pos(body, tx, tz, lift_above_surface),
        rotation: rot,
        scale: Vec3::ONE,
    }
}

pub fn prop_surface_transform(
    body: CelestialBody,
    tx: f32,
    tz: f32,
    lift_above_surface: f32,
    yaw: f32,
) -> Transform {
    sphere_surface_transform(body, tx, tz, lift_above_surface, yaw)
}

/// Player capsule on a sphere — matches `interpolate_player_visual`.
pub fn player_sphere_transform(
    body: CelestialBody,
    tx: f32,
    tz: f32,
    feet_y: f32,
    radius: f32,
    rot_y: f32,
) -> Transform {
    let r = body_walk_radius(body);
    let dir = planet_dir_from_tangent(body, tx, tz);
    let disp = surface_relief_at(body, tx, tz);
    let center = Vec3::from_array(planet_gameplay_center(body));
    let normal = Vec3::new(dir[0], dir[1], dir[2]).normalize_or_zero();
    let surf = center + normal * (r + disp);
    let ground = planet_ground_y_at(body, tx, tz);
    let hop = (feet_y - ground).max(0.0);
    let align = Quat::from_rotation_arc(Vec3::Y, normal);
    Transform {
        translation: surf + normal * (radius * 1.1 + hop),
        rotation: align * Quat::from_rotation_y(rot_y),
        scale: Vec3::ONE,
    }
}

pub fn tangent_at_world(body: CelestialBody, world: Vec3) -> (f32, f32) {
    let center = Vec3::from_array(planet_gameplay_center(body));
    let dir = (world - center).normalize_or_zero();
    ha_shared::planet_tangent_from_dir(body, dir.to_array())
}

pub fn world_pos_from_tangent(body: CelestialBody, tx: f32, tz: f32, lift_m: f32) -> Vec3 {
    sphere_world_pos(body, tx, tz, lift_m)
}

pub fn surface_normal_at_tangent(body: CelestialBody, tx: f32, tz: f32) -> Vec3 {
    sphere_surface_normal(body, tx, tz)
}

pub fn flat_player_transform(tx: f32, feet_y: f32, tz: f32, radius: f32, rot_y: f32) -> Transform {
    Transform {
        translation: Vec3::new(
            tx,
            feet_y + capsule_center_above_feet(radius),
            tz,
        ),
        rotation: Quat::from_rotation_y(rot_y),
        scale: Vec3::ONE,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ha_shared::{LAUNCH_PAD_ZONE_X, LAUNCH_PAD_ZONE_Z};

    fn dist(a: Vec3, b: Vec3) -> f32 {
        (a - b).length()
    }

    #[test]
    fn launch_pad_props_sit_on_shell() {
        let body = CelestialBody::Earth;
        let tx = LAUNCH_PAD_ZONE_X;
        let tz = LAUNCH_PAD_ZONE_Z;
        let center = Vec3::from_array(planet_gameplay_center(body));
        let dir = planet_dir_from_tangent(body, tx, tz);
        let disp = surface_relief_at(body, tx, tz);
        let r = body_walk_radius(body);
        let pad = sphere_world_pos(body, tx, tz, PAD_SURFACE_LIFT);
        let expected = r + disp + PAD_SURFACE_LIFT;
        assert!((dist(pad, center) - expected).abs() < 0.05);
    }
}
