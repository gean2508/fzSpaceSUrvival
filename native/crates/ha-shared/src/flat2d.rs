//! Flat-2D world model — Terraria-style surface + SFS-style orbital plane.
//!
//! The 2D game is the z=0 great-circle slice of each body: the surface is a
//! wrapped strip of tiles (`s` = arc metres along the circumference, `h` =
//! height above the walk shell) and the orbital frame is the SAME XZ plane
//! the ephemerides already live on. `flat_surface_to_orbital` /
//! `orbital_to_flat_surface` are exact inverses, so takeoff/landing is a
//! continuous frame change (biblia §1 continuity, guard B8).

use crate::celestial_bodies::{body_terrain_seed, body_walk_radius, CelestialBody};
use crate::constants::{FRICTION, GRAVITY, JUMP_STAMINA_COST, JUMP_VELOCITY, SPRINT_MULT, STAMINA_SPRINT_DRAIN};
use crate::input::InputState;
use crate::movement::MovementBody;
use crate::solar_ephemeris::body_center_at;
use crate::types::{MOV_IDLE, MOV_RUNNING, MOV_SPRINTING};

pub const TILE_M: f32 = 1.0;
/// Deepest diggable depth below the local surface line (tiles).
pub const DIG_DEPTH_TILES: i32 = 48;
/// Highest tile row that terrain generation can produce.
pub const SKY_TOP_TILES: i32 = 96;
pub const PLAYER_RADIUS: f32 = 0.35;
pub const PLAYER_HEIGHT: f32 = 1.7;
pub const COYOTE_TIME_SECS: f32 = 0.12;

/// Tiles around the full circumference of a body's walk shell.
pub fn body_circumference_tiles(body: CelestialBody) -> u32 {
    (body_walk_radius(body) * std::f32::consts::TAU / TILE_M).round() as u32
}

pub fn body_circumference_m(body: CelestialBody) -> f32 {
    body_circumference_tiles(body) as f32 * TILE_M
}

/// Wrap a tile column index onto the circumference.
pub fn wrap_tile(i: i64, circumference: u32) -> u32 {
    let c = circumference as i64;
    (((i % c) + c) % c) as u32
}

/// Shortest signed arc from `from` to `to` on a circumference of `c` metres.
pub fn wrap_delta_m(from: f32, to: f32, c: f32) -> f32 {
    let mut d = (to - from) % c;
    if d > c * 0.5 {
        d -= c;
    }
    if d < -c * 0.5 {
        d += c;
    }
    d
}

// ---------------------------------------------------------------------------
// Terrain generation (deterministic per body seed, wraps seamlessly)
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TileKind {
    Air,
    Grass,
    Dirt,
    Stone,
    Sand,
    Regolith,
    RockDark,
    RockRed,
    Water,
    Bedrock,
    /// Player-placed wood planks.
    Wood,
}

impl TileKind {
    pub fn solid(self) -> bool {
        !matches!(self, TileKind::Air | TileKind::Water)
    }

    /// Item granted when the tile is dug (None = not collectible).
    pub fn dug_item(self) -> Option<&'static str> {
        match self {
            TileKind::Grass | TileKind::Dirt => Some("dirt"),
            TileKind::Stone | TileKind::RockDark | TileKind::RockRed => Some("stone"),
            TileKind::Sand => Some("sand"),
            TileKind::Regolith => Some("regolith"),
            TileKind::Wood => Some("wood"),
            _ => None,
        }
    }

    /// Flat sRGB colour for chunk vertex tinting.
    pub fn color(self) -> [f32; 3] {
        match self {
            TileKind::Air => [0.0, 0.0, 0.0],
            TileKind::Grass => [0.36, 0.62, 0.29],
            TileKind::Dirt => [0.47, 0.33, 0.22],
            TileKind::Stone => [0.46, 0.47, 0.50],
            TileKind::Sand => [0.78, 0.70, 0.45],
            TileKind::Regolith => [0.62, 0.62, 0.64],
            TileKind::RockDark => [0.34, 0.34, 0.38],
            TileKind::RockRed => [0.60, 0.34, 0.24],
            TileKind::Water => [0.25, 0.47, 0.70],
            TileKind::Bedrock => [0.16, 0.16, 0.18],
            TileKind::Wood => [0.58, 0.44, 0.26],
        }
    }
}

fn hash_u32(mut x: u32) -> u32 {
    x ^= x >> 16;
    x = x.wrapping_mul(0x7feb_352d);
    x ^= x >> 15;
    x = x.wrapping_mul(0x846c_a68b);
    x ^= x >> 16;
    x
}

fn hash2(seed: u32, a: u32, b: u32) -> u32 {
    hash_u32(seed ^ hash_u32(a).wrapping_add(hash_u32(b).wrapping_mul(0x9e37_79b9)))
}

fn hash01(seed: u32, a: u32, b: u32) -> f32 {
    (hash2(seed, a, b) & 0x00ff_ffff) as f32 / 16_777_215.0
}

/// Wrapped 1D value noise: lattice points every `period` tiles, seamless
/// around a circumference of `c` tiles.
fn wrapped_value_noise(seed: u32, i: i64, c: u32, period: u32, octave: u32) -> f32 {
    let period = period.max(2);
    let cells = (c / period).max(2);
    let fi = (wrap_tile(i, c) as f32) / period as f32;
    let cell = fi.floor() as u32 % cells;
    let next = (cell + 1) % cells;
    let t = fi.fract();
    let smooth = t * t * (3.0 - 2.0 * t);
    let a = hash01(seed.wrapping_add(octave.wrapping_mul(0x51ed)), cell, octave);
    let b = hash01(seed.wrapping_add(octave.wrapping_mul(0x51ed)), next, octave);
    a + (b - a) * smooth
}

struct BodyTerrain {
    amp: f32,
    base_period: u32,
    sea_level: Option<i32>,
    top: TileKind,
    shallow: TileKind,
    deep: TileKind,
    shore: TileKind,
}

fn body_terrain(body: CelestialBody) -> BodyTerrain {
    match body {
        CelestialBody::Earth => BodyTerrain {
            amp: 14.0,
            base_period: 48,
            sea_level: Some(-3),
            top: TileKind::Grass,
            shallow: TileKind::Dirt,
            deep: TileKind::Stone,
            shore: TileKind::Sand,
        },
        CelestialBody::Moon => BodyTerrain {
            amp: 9.0,
            base_period: 32,
            sea_level: None,
            top: TileKind::Regolith,
            shallow: TileKind::Regolith,
            deep: TileKind::RockDark,
            shore: TileKind::Regolith,
        },
        _ => BodyTerrain {
            amp: 11.0,
            base_period: 40,
            sea_level: None,
            top: TileKind::Sand,
            shallow: TileKind::RockRed,
            deep: TileKind::RockDark,
            shore: TileKind::Sand,
        },
    }
}

/// Ground surface row (tile j of the topmost solid tile) for column `i`.
pub fn ground_row(body: CelestialBody, i: i64) -> i32 {
    let seed = body_terrain_seed(body);
    let c = body_circumference_tiles(body);
    let t = body_terrain(body);
    let n0 = wrapped_value_noise(seed, i, c, t.base_period, 0) - 0.5;
    let n1 = wrapped_value_noise(seed, i, c, t.base_period / 3, 1) - 0.5;
    let n2 = wrapped_value_noise(seed, i, c, (t.base_period / 9).max(2), 2) - 0.5;
    (t.amp * (n0 + n1 * 0.4 + n2 * 0.15)).round() as i32
}

/// Ground height in metres (top face of the surface tile) for arc position `s`.
pub fn ground_height_m(body: CelestialBody, s: f32) -> f32 {
    (ground_row(body, (s / TILE_M).floor() as i64) + 1) as f32 * TILE_M
}

/// Natural (un-edited) tile at column `i`, row `j` (row 0 straddles h=0).
pub fn natural_tile(body: CelestialBody, i: i64, j: i32) -> TileKind {
    let g = ground_row(body, i);
    if j < g - DIG_DEPTH_TILES {
        return TileKind::Bedrock;
    }
    let t = body_terrain(body);
    if j > g {
        if let Some(sea) = t.sea_level {
            if j <= sea {
                return TileKind::Water;
            }
        }
        return TileKind::Air;
    }
    let near_water = t
        .sea_level
        .map(|sea| g <= sea + 1)
        .unwrap_or(false);
    if j == g {
        return if near_water { t.shore } else { t.top };
    }
    if j > g - 4 {
        return if near_water { t.shore } else { t.shallow };
    }
    t.deep
}

// ---------------------------------------------------------------------------
// Flat platformer locomotion (capsule vs tile grid, axis-separated)
// ---------------------------------------------------------------------------

/// Step the player on a flat 2D slice. `body.x` = arc metres `s` (unwrapped),
/// `body.y` = height `h`; `body.z` stays 0. `solid_at(i, j)` must answer for
/// ANY column (it wraps internally) and include player-placed tiles.
pub fn step_flat_movement(
    surface: CelestialBody,
    body: &mut MovementBody,
    input: &InputState,
    dt: f32,
    jump_pressed: bool,
    gravity_scale: f32,
    solid_at: &dyn Fn(i64, i32) -> bool,
) {
    // `gravity_scale` (per WorldContext) already encodes Moon/planet gravity.
    let g = GRAVITY * gravity_scale.max(0.02);
    let move_x = input.move_x.clamp(-1.0, 1.0);
    let wants_sprint = input.sprint == 1 && move_x.abs() > 0.1 && body.stamina > 0.0;
    let speed_mult = if wants_sprint { SPRINT_MULT } else { 1.0 };
    let target_vx = move_x * body.speed * speed_mult;
    let blend = (FRICTION * dt).clamp(0.0, 1.0);
    body.vx += (target_vx - body.vx) * blend;

    if wants_sprint {
        body.stamina = (body.stamina - STAMINA_SPRINT_DRAIN * dt).max(0.0);
    } else {
        body.stamina = (body.stamina + STAMINA_SPRINT_DRAIN * 0.6 * dt).min(body.max_stamina);
    }

    if jump_pressed && body.coyote_sec > 0.0 && body.stamina >= JUMP_STAMINA_COST {
        body.vy = JUMP_VELOCITY * (gravity_scale.max(0.15)).sqrt().max(0.55);
        body.stamina -= JUMP_STAMINA_COST;
        body.coyote_sec = 0.0;
    }
    body.vy -= g * dt;
    body.vy = body.vy.clamp(-60.0, 60.0);

    // X axis, then Y axis, resolving against solid tiles.
    let new_x = body.x + body.vx * dt;
    if !capsule_hits(new_x, body.y, solid_at) {
        body.x = new_x;
    } else if step_up_ok(new_x, body.y, solid_at) {
        // 1-tile auto step-up keeps walking over gentle terrain.
        body.x = new_x;
        body.y = (body.y / TILE_M).floor() * TILE_M + TILE_M;
    } else {
        body.vx = 0.0;
    }

    let new_y = body.y + body.vy * dt;
    let mut grounded = false;
    if !capsule_hits(body.x, new_y, solid_at) {
        body.y = new_y;
    } else if body.vy <= 0.0 {
        // Landing: snap feet to the tile top below.
        body.y = (new_y / TILE_M).ceil() * TILE_M;
        while capsule_hits(body.x, body.y, solid_at) {
            body.y += TILE_M;
        }
        body.vy = 0.0;
        grounded = true;
    } else {
        // Head bump.
        body.vy = 0.0;
    }
    if !grounded && body.vy == 0.0 && capsule_hits(body.x, body.y - 0.05, solid_at) {
        grounded = true;
    }

    body.coyote_sec = if grounded {
        COYOTE_TIME_SECS
    } else {
        (body.coyote_sec - dt).max(0.0)
    };

    if move_x.abs() > 0.05 {
        // rot encodes 2D facing: 0 = +s (right), PI = -s (left).
        body.rot = if move_x >= 0.0 { 0.0 } else { std::f32::consts::PI };
        body.state = if wants_sprint { MOV_SPRINTING } else { MOV_RUNNING };
    } else {
        body.state = MOV_IDLE;
    }
    let _ = surface;
}

/// Creative fly on the flat slice: JUMP ascends, sprint descends, no gravity.
pub fn step_flat_creative_fly(
    body: &mut MovementBody,
    input: &InputState,
    dt: f32,
    jump_held: bool,
    sprint_held: bool,
) {
    const FLY_SPEED: f32 = 16.0;
    let target_vx = input.move_x.clamp(-1.0, 1.0) * FLY_SPEED * 1.6;
    let blend = (FRICTION * dt).clamp(0.0, 1.0);
    body.vx += (target_vx - body.vx) * blend;
    body.x += body.vx * dt;
    let vy = if jump_held {
        FLY_SPEED
    } else if sprint_held {
        -FLY_SPEED
    } else {
        0.0
    };
    body.vy = vy;
    body.y += vy * dt;
    if input.move_x.abs() > 0.05 {
        body.rot = if input.move_x >= 0.0 { 0.0 } else { std::f32::consts::PI };
    }
    body.coyote_sec = COYOTE_TIME_SECS;
}

fn capsule_hits(x: f32, feet_y: f32, solid_at: &dyn Fn(i64, i32) -> bool) -> bool {
    let x0 = ((x - PLAYER_RADIUS) / TILE_M).floor() as i64;
    let x1 = ((x + PLAYER_RADIUS - 1e-4) / TILE_M).floor() as i64;
    let y0 = (feet_y / TILE_M + 1e-4).floor() as i32;
    let y1 = ((feet_y + PLAYER_HEIGHT - 1e-3) / TILE_M).floor() as i32;
    for i in x0..=x1 {
        for j in y0..=y1 {
            if solid_at(i, j) {
                return true;
            }
        }
    }
    false
}

fn step_up_ok(x: f32, feet_y: f32, solid_at: &dyn Fn(i64, i32) -> bool) -> bool {
    let stepped = (feet_y / TILE_M).floor() * TILE_M + TILE_M;
    (stepped - feet_y) <= 1.05 * TILE_M && !capsule_hits(x, stepped, solid_at)
}

// ---------------------------------------------------------------------------
// Surface ↔ orbital frame remap (exact, continuous — biblia §1 in 2D)
// ---------------------------------------------------------------------------

/// Angle on the body's circle for arc position `s`.
pub fn arc_to_angle(body: CelestialBody, s: f32) -> f32 {
    s / body_walk_radius(body)
}

/// Map (s, h) on a body's surface to a world-plane position [x, y, z]
/// (y is the shared flight plane; all bodies are coplanar).
pub fn flat_surface_to_orbital(body: CelestialBody, s: f32, h: f32, t_secs: f32) -> [f32; 3] {
    let c = body_center_at(body, t_secs);
    let r = body_walk_radius(body) + h;
    let th = arc_to_angle(body, s);
    [c[0] + th.cos() * r, c[1], c[2] + th.sin() * r]
}

/// Inverse of `flat_surface_to_orbital`: world-plane position → (s, h).
pub fn orbital_to_flat_surface(body: CelestialBody, pos: [f32; 3], t_secs: f32) -> (f32, f32) {
    let c = body_center_at(body, t_secs);
    let dx = pos[0] - c[0];
    let dz = pos[2] - c[2];
    let dist = (dx * dx + dz * dz).sqrt();
    let th = dz.atan2(dx);
    let r = body_walk_radius(body);
    let s = th.rem_euclid(std::f32::consts::TAU) * r;
    (s, dist - r)
}

/// Remap a surface-frame velocity (vs = along-surface, vh = radial) into the
/// world plane at arc position `s`.
pub fn flat_velocity_to_orbital(body: CelestialBody, s: f32, vs: f32, vh: f32) -> [f32; 3] {
    let th = arc_to_angle(body, s);
    // radial = (cos th, sin th); tangent (+s direction) = (-sin th, cos th)
    [
        -th.sin() * vs + th.cos() * vh,
        0.0,
        th.cos() * vs + th.sin() * vh,
    ]
}

/// Inverse of `flat_velocity_to_orbital` at arc position `s`.
pub fn orbital_velocity_to_flat(body: CelestialBody, s: f32, v: [f32; 3]) -> (f32, f32) {
    let th = arc_to_angle(body, s);
    let vs = -th.sin() * v[0] + th.cos() * v[2];
    let vh = th.cos() * v[0] + th.sin() * v[2];
    (vs, vh)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::input::empty_input;

    fn natural_solid(body: CelestialBody) -> impl Fn(i64, i32) -> bool {
        move |i, j| natural_tile(body, i, j).solid()
    }

    #[test]
    fn terrain_wraps_seamlessly() {
        let c = body_circumference_tiles(CelestialBody::Earth) as i64;
        for i in [0i64, 7, 133, 4000] {
            assert_eq!(
                ground_row(CelestialBody::Earth, i),
                ground_row(CelestialBody::Earth, i + c),
                "column {i} must match one wrap later"
            );
        }
    }

    #[test]
    fn terrain_is_deterministic_and_layered() {
        let g = ground_row(CelestialBody::Earth, 100);
        assert_eq!(g, ground_row(CelestialBody::Earth, 100));
        assert_eq!(natural_tile(CelestialBody::Earth, 100, g + 1), TileKind::Air);
        assert!(natural_tile(CelestialBody::Earth, 100, g).solid());
        assert_eq!(
            natural_tile(CelestialBody::Earth, 100, g - DIG_DEPTH_TILES - 1),
            TileKind::Bedrock
        );
        assert_eq!(natural_tile(CelestialBody::Moon, 50, ground_row(CelestialBody::Moon, 50)), TileKind::Regolith);
    }

    #[test]
    fn walking_moves_and_gravity_lands() {
        let solid = natural_solid(CelestialBody::Earth);
        let mut b = MovementBody::new(5.0, PLAYER_RADIUS);
        b.x = 40.5;
        b.y = ground_height_m(CelestialBody::Earth, b.x) + 6.0;
        let mut input = empty_input(1);
        input.move_x = 1.0;
        let x0 = b.x;
        for _ in 0..120 {
            step_flat_movement(CelestialBody::Earth, &mut b, &input, 0.05, false, 1.0, &solid);
        }
        assert!(b.x > x0 + 2.0, "player should walk forward, moved {}", b.x - x0);
        // Stop and settle: mid-walk the player may be falling off a ledge.
        input.move_x = 0.0;
        for _ in 0..60 {
            step_flat_movement(CelestialBody::Earth, &mut b, &input, 0.05, false, 1.0, &solid);
        }
        assert!(b.vy.abs() < 0.01, "player should have landed, vy {}", b.vy);
        assert!(b.coyote_sec > 0.0, "grounded player has coyote time");
    }

    #[test]
    fn jump_rises_and_returns() {
        let solid = natural_solid(CelestialBody::Earth);
        let mut b = MovementBody::new(5.0, PLAYER_RADIUS);
        b.x = 10.5;
        b.y = ground_height_m(CelestialBody::Earth, b.x) + 4.0;
        let input = empty_input(1);
        for _ in 0..80 {
            step_flat_movement(CelestialBody::Earth, &mut b, &input, 0.05, false, 1.0, &solid);
        }
        let ground = b.y;
        step_flat_movement(CelestialBody::Earth, &mut b, &input, 0.05, true, 1.0, &solid);
        assert!(b.vy > 0.0, "jump must set upward velocity");
        let mut peak = b.y;
        for _ in 0..80 {
            step_flat_movement(CelestialBody::Earth, &mut b, &input, 0.05, false, 1.0, &solid);
            peak = peak.max(b.y);
        }
        assert!(peak > ground + 0.8, "jump should gain height, peak {peak} vs {ground}");
        assert!((b.y - ground).abs() < 1.2, "player should land near start");
    }

    #[test]
    fn orbital_remap_round_trips() {
        for (s, h) in [(0.0f32, 0.0f32), (1200.0, 35.0), (8000.0, 400.0)] {
            let pos = flat_surface_to_orbital(CelestialBody::Earth, s, h, 0.0);
            let (s2, h2) = orbital_to_flat_surface(CelestialBody::Earth, pos, 0.0);
            let c = body_circumference_m(CelestialBody::Earth);
            assert!(wrap_delta_m(s.rem_euclid(c), s2, c).abs() < 0.5, "s {s} vs {s2}");
            assert!((h - h2).abs() < 0.5, "h {h} vs {h2}");
        }
    }

    #[test]
    fn velocity_remap_round_trips() {
        let v = flat_velocity_to_orbital(CelestialBody::Earth, 3000.0, 12.0, -4.0);
        let (vs, vh) = orbital_velocity_to_flat(CelestialBody::Earth, 3000.0, v);
        assert!((vs - 12.0).abs() < 1e-3);
        assert!((vh + 4.0).abs() < 1e-3);
    }

    #[test]
    fn moon_jump_flies_higher_than_earth() {
        // Same jump on the Moon (gravity_scale 0.17) must peak higher.
        let peak_on = |body: CelestialBody, scale: f32| {
            let solid = natural_solid(body);
            let mut b = MovementBody::new(5.0, PLAYER_RADIUS);
            b.x = 20.5;
            b.y = ground_height_m(body, b.x) + 3.0;
            let input = empty_input(1);
            for _ in 0..100 {
                step_flat_movement(body, &mut b, &input, 0.05, false, scale, &solid);
            }
            let ground = b.y;
            step_flat_movement(body, &mut b, &input, 0.05, true, scale, &solid);
            let mut peak = b.y;
            for _ in 0..160 {
                step_flat_movement(body, &mut b, &input, 0.05, false, scale, &solid);
                peak = peak.max(b.y);
            }
            peak - ground
        };
        let earth = peak_on(CelestialBody::Earth, 1.0);
        let moon = peak_on(CelestialBody::Moon, crate::constants::GRAVITY_LUNA_SCALE);
        assert!(moon > earth * 1.5, "moon jump {moon} vs earth {earth}");
    }
}
