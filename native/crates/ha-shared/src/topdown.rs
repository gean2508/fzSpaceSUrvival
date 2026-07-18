//! Top-Down 3/4 world model (LimeZu Modern) — successor to `flat2d.rs`.
//!
//! Migration to a Stardew/top-down view. The walkable ground is the XZ plane
//! (the same plane the solar system lives on): `body.x` = East(+)/West(−),
//! `body.z` = North(−)/South(+), `body.y` = height (0 on foot). Screen render
//! is `(x, -z)` — identical to `orbital2d.rs`, so surface and space share a
//! projection and launching is a zoom-out, not a coordinate change.
//!
//! Depth is a discrete **Z-Level** (`z_level: i32`): 0 = surface, negative =
//! underground mine floors, each its own deterministic top-down grid. Space is
//! NOT a z-level — it is the orbital mode. See `MIGRATION_PLAN.md` §1–§2.

use crate::celestial_bodies::{body_terrain_seed, CelestialBody};
use crate::constants::{FRICTION, SPRINT_MULT, STAMINA_SPRINT_DRAIN};
use crate::input::InputState;
use crate::movement::MovementBody;
use crate::types::{MOV_IDLE, MOV_RUNNING, MOV_SPRINTING};

pub const TILE_M: f32 = 1.0;
pub const PLAYER_RADIUS: f32 = 0.34;
/// Deepest mine floor reachable (z_level).
pub const MIN_Z_LEVEL: i32 = -8;

// ---------------------------------------------------------------------------
// Tiles
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Tile {
    // Surface (z=0)
    Grass,
    Dirt,
    Sand,
    Water,
    Regolith,
    RockRed,
    // Underground (z<0)
    Stone,
    RockDark,
    Bedrock,
    CaveFloor,
    // Player-placed / shared
    WoodFloor,
    Wall,
    // Build pieces (construction system) — placed only, never generated.
    WoodWall,
    StoneBrick,
    Door,
    Air,
}

impl Tile {
    /// Blocks walking (walls, water, solid rock — cave floors are walkable).
    /// Build walls block; wood floors and doors are walkable.
    pub fn solid(self) -> bool {
        matches!(
            self,
            Tile::Water
                | Tile::Wall
                | Tile::Bedrock
                | Tile::Stone
                | Tile::RockDark
                | Tile::RockRed
                | Tile::WoodWall
                | Tile::StoneBrick
        )
    }

    /// Diggable (removable) → yields this item id, if any. Build pieces refund
    /// their material so deconstruction is just "dig the piece".
    pub fn dug_item(self) -> Option<&'static str> {
        match self {
            Tile::Grass | Tile::Dirt => Some("dirt"),
            Tile::Sand => Some("sand"),
            Tile::Regolith => Some("regolith"),
            Tile::Stone | Tile::RockRed => Some("stone"),
            Tile::RockDark => Some("stone"),
            Tile::WoodFloor | Tile::Wall | Tile::WoodWall | Tile::Door => Some("wood"),
            Tile::StoneBrick => Some("stone"),
            _ => None,
        }
    }

    /// Flat sRGB colour (procedural fallback until LimeZu tilesets land).
    pub fn color(self) -> [f32; 3] {
        match self {
            Tile::Grass => [0.36, 0.62, 0.29],
            Tile::Dirt => [0.47, 0.33, 0.22],
            Tile::Sand => [0.78, 0.70, 0.45],
            Tile::Water => [0.25, 0.47, 0.70],
            Tile::Regolith => [0.62, 0.62, 0.64],
            Tile::RockRed => [0.60, 0.34, 0.24],
            Tile::Stone => [0.46, 0.47, 0.50],
            Tile::RockDark => [0.30, 0.30, 0.34],
            Tile::Bedrock => [0.16, 0.16, 0.18],
            Tile::CaveFloor => [0.24, 0.22, 0.26],
            Tile::WoodFloor => [0.58, 0.44, 0.26],
            Tile::Wall => [0.38, 0.34, 0.30],
            Tile::WoodWall => [0.45, 0.31, 0.17],
            Tile::StoneBrick => [0.55, 0.55, 0.58],
            Tile::Door => [0.30, 0.20, 0.10],
            Tile::Air => [0.0, 0.0, 0.0],
        }
    }
}

// ---------------------------------------------------------------------------
// Construction system — build pieces catalogue (deterministic, shared)
// ---------------------------------------------------------------------------

/// A placeable structural piece. Placed on an empty surface cell via the build
/// mode (hammer + `BuildModeActive`), snapping to the aimed cell. Removing a
/// piece is just digging it — `Tile::dug_item` refunds the material.
#[derive(Clone, Copy, Debug)]
pub struct BuildPiece {
    pub id: &'static str,
    pub label: &'static str,
    /// The tile this piece places.
    pub tile: Tile,
    /// Resource consumed to place one (item id + quantity).
    pub cost_item: &'static str,
    pub cost_qty: u32,
}

/// Ordered palette of build pieces (index = palette slot).
pub static BUILD_PIECES: &[BuildPiece] = &[
    BuildPiece {
        id: "build_wood_floor",
        label: "Piso",
        tile: Tile::WoodFloor,
        cost_item: "wood",
        cost_qty: 1,
    },
    BuildPiece {
        id: "build_wood_wall",
        label: "Muro madera",
        tile: Tile::WoodWall,
        cost_item: "wood",
        cost_qty: 2,
    },
    BuildPiece {
        id: "build_stone_brick",
        label: "Muro piedra",
        tile: Tile::StoneBrick,
        cost_item: "stone",
        cost_qty: 2,
    },
    BuildPiece {
        id: "build_door",
        label: "Puerta",
        tile: Tile::Door,
        cost_item: "wood",
        cost_qty: 3,
    },
];

/// Number of pieces in the build palette.
pub fn build_piece_count() -> usize {
    BUILD_PIECES.len()
}

/// Palette piece by index (clamped-safe: returns None if out of range).
pub fn build_piece(idx: usize) -> Option<&'static BuildPiece> {
    BUILD_PIECES.get(idx)
}

// ---------------------------------------------------------------------------
// Deterministic hash noise (2D)
// ---------------------------------------------------------------------------

fn hash_u32(mut x: u32) -> u32 {
    x ^= x >> 16;
    x = x.wrapping_mul(0x7feb_352d);
    x ^= x >> 15;
    x = x.wrapping_mul(0x846c_a68b);
    x ^= x >> 16;
    x
}

fn hash2(seed: u32, a: i64, b: i64) -> u32 {
    let a = (a as i32 as u32).wrapping_mul(0x9e37_79b9);
    let b = (b as i32 as u32).wrapping_mul(0x85eb_ca6b);
    hash_u32(seed ^ a ^ hash_u32(b))
}

fn hash01(seed: u32, a: i64, b: i64) -> f32 {
    (hash2(seed, a, b) & 0x00ff_ffff) as f32 / 16_777_215.0
}

/// 2D value noise with smooth interpolation, lattice every `period` tiles.
fn value_noise_2d(seed: u32, tx: i64, tz: i64, period: i64) -> f32 {
    let period = period.max(2);
    let cx = tx.div_euclid(period);
    let cz = tz.div_euclid(period);
    let fx = (tx.rem_euclid(period)) as f32 / period as f32;
    let fz = (tz.rem_euclid(period)) as f32 / period as f32;
    let sx = fx * fx * (3.0 - 2.0 * fx);
    let sz = fz * fz * (3.0 - 2.0 * fz);
    let n00 = hash01(seed, cx, cz);
    let n10 = hash01(seed, cx + 1, cz);
    let n01 = hash01(seed, cx, cz + 1);
    let n11 = hash01(seed, cx + 1, cz + 1);
    let a = n00 + (n10 - n00) * sx;
    let b = n01 + (n11 - n01) * sx;
    a + (b - a) * sz
}

/// Fractal (2-octave) 2D noise in 0..1.
fn fbm2(seed: u32, tx: i64, tz: i64, period: i64) -> f32 {
    let a = value_noise_2d(seed, tx, tz, period);
    let b = value_noise_2d(seed ^ 0x51ed_2b37, tx, tz, (period / 3).max(2));
    (a * 0.68 + b * 0.32).clamp(0.0, 1.0)
}

// ---------------------------------------------------------------------------
// Terrain generation per (body, z_level)
// ---------------------------------------------------------------------------

fn surface_tile(body: CelestialBody, tx: i64, tz: i64) -> Tile {
    let seed = body_terrain_seed(body);
    let elevation = fbm2(seed, tx, tz, 22);
    let moisture = fbm2(seed ^ 0x1234_abcd, tx + 900, tz - 400, 30);
    match body {
        CelestialBody::Earth => {
            if elevation < 0.30 {
                Tile::Water
            } else if elevation < 0.36 {
                Tile::Sand
            } else if moisture > 0.55 {
                Tile::Grass
            } else if moisture > 0.40 {
                Tile::Dirt
            } else {
                Tile::Sand
            }
        }
        CelestialBody::Moon => Tile::Regolith,
        _ => {
            if elevation < 0.34 {
                Tile::RockRed
            } else {
                Tile::Sand
            }
        }
    }
}

/// True where an underground floor has an open (walkable) cave cell.
fn cave_open(body: CelestialBody, z: i32, tx: i64, tz: i64) -> bool {
    let seed = body_terrain_seed(body) ^ 0x0cae_0000u32 ^ (z as u32).wrapping_mul(0x9e37);
    // Deeper floors are more open (worm-like tunnels via thresholded noise):
    // lower threshold ⇒ more cells pass ⇒ more open cave.
    let depth = (-z as f32) / (-MIN_Z_LEVEL as f32);
    let threshold = 0.52 - depth * 0.12;
    fbm2(seed, tx, tz, 9) > threshold
}

/// The natural (un-edited) tile at a grid cell of a given z_level.
pub fn topdown_tile(body: CelestialBody, z: i32, tx: i64, tz: i64) -> Tile {
    // Portals + a 1-ring around them are always open so you never arrive stuck.
    if portal_here(body, z, tx, tz).is_some() || portal_adjacent(body, z, tx, tz) {
        return if z == 0 { Tile::Dirt } else { Tile::CaveFloor };
    }
    if z == 0 {
        return surface_tile(body, tx, tz);
    }
    if z <= MIN_Z_LEVEL {
        return Tile::Bedrock;
    }
    if cave_open(body, z, tx, tz) {
        Tile::CaveFloor
    } else {
        match body {
            CelestialBody::Moon => Tile::RockDark,
            _ => Tile::Stone,
        }
    }
}

// ---------------------------------------------------------------------------
// Z-Level portals (mine entrance / ladders) — deterministic vertical shafts
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Portal {
    /// Hole down to floor `z-1`.
    Descend,
    /// Ladder up to floor `z+1`.
    Ascend,
}

/// Anchor cell of the surface mine entrance (findable, near the origin/pad).
fn mine_entrance(body: CelestialBody) -> (i64, i64) {
    let seed = body_terrain_seed(body);
    let ox = 6 + (hash2(seed, 1, 1) % 6) as i64;
    let oz = 4 + (hash2(seed, 2, 2) % 6) as i64;
    (ox, oz)
}

/// The "go down" hole on floor `z` (surface 0 → deepest+1). Deterministic;
/// each floor's hole is a short walk from the ladder you arrived at.
pub fn descend_portal(body: CelestialBody, z: i32) -> (i64, i64) {
    let mut cell = mine_entrance(body);
    let seed = body_terrain_seed(body) ^ 0x5afe_0001u32;
    // Walk the shaft downward, offsetting a few tiles per floor.
    let mut f = 0;
    while f > z {
        f -= 1;
        let h = hash2(seed, f as i64, 77);
        let dx = (h % 13) as i64 - 6;
        let dz = ((h >> 8) % 13) as i64 - 6;
        cell = (cell.0 + dx, cell.1 + dz);
    }
    cell
}

/// The "go up" ladder on floor `z` (only floors below the surface). Placed
/// where you arrive from `z+1` — i.e. the descend hole of the floor above.
pub fn ascend_portal(body: CelestialBody, z: i32) -> Option<(i64, i64)> {
    if z >= 0 {
        return None;
    }
    Some(descend_portal(body, z + 1))
}

/// Portal at this exact cell, if any.
pub fn portal_here(body: CelestialBody, z: i32, tx: i64, tz: i64) -> Option<Portal> {
    if z > MIN_Z_LEVEL && descend_portal(body, z) == (tx, tz) {
        return Some(Portal::Descend);
    }
    if let Some(a) = ascend_portal(body, z) {
        if a == (tx, tz) {
            return Some(Portal::Ascend);
        }
    }
    None
}

fn portal_adjacent(body: CelestialBody, z: i32, tx: i64, tz: i64) -> bool {
    for dx in -1..=1 {
        for dz in -1..=1 {
            if (dx != 0 || dz != 0) && portal_here(body, z, tx + dx, tz + dz).is_some() {
                return true;
            }
        }
    }
    false
}

pub fn solid_at(body: CelestialBody, z: i32, tx: i64, tz: i64) -> bool {
    topdown_tile(body, z, tx, tz).solid()
}

// ---------------------------------------------------------------------------
// Deposits (ore/gem veins) — deterministic, denser underground
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Deposit {
    OreIron,
    OreGold,
    GemBlue,
    GemRed,
}

/// Deterministic deposit at a walkable cell, if any.
pub fn deposit_at(body: CelestialBody, z: i32, tx: i64, tz: i64) -> Option<Deposit> {
    // Surface: sparse rocks/ore. Underground: richer, deeper = rarer gems.
    let seed = body_terrain_seed(body) ^ 0xde90_0000u32 ^ (z as u32).wrapping_mul(0x2545);
    let h = hash2(seed, tx, tz);
    let roll = h % 1000;
    let deep = z < 0;
    match body {
        CelestialBody::Moon => match roll {
            0..=3 if deep => Some(Deposit::GemRed),
            4..=14 if deep => Some(Deposit::OreIron),
            0..=1 => Some(Deposit::OreIron),
            _ => None,
        },
        CelestialBody::Earth => match roll {
            0..=6 if deep => Some(Deposit::OreIron),
            7..=10 if deep => Some(Deposit::OreGold),
            11..=13 if deep => Some(Deposit::GemBlue),
            0..=2 => Some(Deposit::OreIron),
            _ => None,
        },
        _ => match roll {
            0..=8 if deep => Some(Deposit::OreIron),
            9..=12 if deep => Some(Deposit::GemBlue),
            0..=1 => Some(Deposit::OreIron),
            _ => None,
        },
    }
}

// ---------------------------------------------------------------------------
// 8-direction locomotion (no gravity, capsule vs tile grid, axis-separated)
// ---------------------------------------------------------------------------

/// Nearest of 8 facing directions (0..8, 0 = East, CCW) for animation.
pub fn dir8(rot: f32) -> u8 {
    let tau = std::f32::consts::TAU;
    (((rot.rem_euclid(tau) / tau) * 8.0).round() as u8) % 8
}

/// Step the player on a top-down floor. `body.x`/`body.z` are the ground
/// plane; `move_x`→x, `move_z`→z. `solid(tx,tz)` answers for the ACTIVE floor
/// including player edits.
pub fn step_topdown_movement(
    body: &mut MovementBody,
    input: &InputState,
    dt: f32,
    solid: &dyn Fn(i64, i64) -> bool,
) {
    let mut mx = input.move_x;
    let mut mz = input.move_z;
    let len = (mx * mx + mz * mz).sqrt();
    if len > 1.0 {
        mx /= len;
        mz /= len;
    }
    let moving = len > 0.1;
    let wants_sprint = input.sprint == 1 && moving && body.stamina > 0.0;
    let speed_mult = if wants_sprint { SPRINT_MULT } else { 1.0 };
    let target_vx = mx * body.speed * speed_mult;
    let target_vz = mz * body.speed * speed_mult;
    let blend = (FRICTION * dt).clamp(0.0, 1.0);
    body.vx += (target_vx - body.vx) * blend;
    body.vz += (target_vz - body.vz) * blend;

    if wants_sprint {
        body.stamina = (body.stamina - STAMINA_SPRINT_DRAIN * dt).max(0.0);
    } else {
        body.stamina = (body.stamina + STAMINA_SPRINT_DRAIN * 0.6 * dt).min(body.max_stamina);
    }

    // X axis then Z axis, resolving against solid tiles (slide along walls).
    let new_x = body.x + body.vx * dt;
    if !capsule_hits(new_x, body.z, solid) {
        body.x = new_x;
    } else {
        body.vx = 0.0;
    }
    let new_z = body.z + body.vz * dt;
    if !capsule_hits(body.x, new_z, solid) {
        body.z = new_z;
    } else {
        body.vz = 0.0;
    }

    if moving {
        body.rot = mz.atan2(mx);
        body.state = if wants_sprint { MOV_SPRINTING } else { MOV_RUNNING };
    } else {
        body.state = MOV_IDLE;
    }
    body.y = 0.0;
}

fn capsule_hits(x: f32, z: f32, solid: &dyn Fn(i64, i64) -> bool) -> bool {
    let x0 = ((x - PLAYER_RADIUS) / TILE_M).floor() as i64;
    let x1 = ((x + PLAYER_RADIUS - 1e-4) / TILE_M).floor() as i64;
    let z0 = ((z - PLAYER_RADIUS) / TILE_M).floor() as i64;
    let z1 = ((z + PLAYER_RADIUS - 1e-4) / TILE_M).floor() as i64;
    for tx in x0..=x1 {
        for tz in z0..=z1 {
            if solid(tx, tz) {
                return true;
            }
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::input::empty_input;

    fn natural_solid(body: CelestialBody, z: i32) -> impl Fn(i64, i64) -> bool {
        move |tx, tz| solid_at(body, z, tx, tz)
    }

    #[test]
    fn terrain_is_deterministic() {
        let a = topdown_tile(CelestialBody::Earth, 0, 12, -7);
        let b = topdown_tile(CelestialBody::Earth, 0, 12, -7);
        assert_eq!(a, b);
        assert_eq!(topdown_tile(CelestialBody::Moon, 0, 3, 3), Tile::Regolith);
        assert_eq!(topdown_tile(CelestialBody::Earth, -20, 0, 0), Tile::Bedrock);
    }

    #[test]
    fn surface_has_variety() {
        // Across a patch there should be more than one tile kind on Earth.
        let mut kinds = std::collections::HashSet::new();
        for tx in 0..40 {
            for tz in 0..40 {
                kinds.insert(topdown_tile(CelestialBody::Earth, 0, tx, tz));
            }
        }
        assert!(kinds.len() >= 3, "expected biome variety, got {kinds:?}");
    }

    #[test]
    fn caves_open_up_with_depth() {
        let count_open = |z: i32| {
            let mut n = 0;
            for tx in 0..60 {
                for tz in 0..60 {
                    if !solid_at(CelestialBody::Earth, z, tx, tz) {
                        n += 1;
                    }
                }
            }
            n
        };
        // Deeper floors have at least as much open space (looser threshold).
        assert!(count_open(-6) >= count_open(-1));
    }

    #[test]
    fn dir8_maps_cardinals() {
        use std::f32::consts::PI;
        assert_eq!(dir8(0.0), 0); // East
        assert_eq!(dir8(PI / 2.0), 2); // South (+z)
        assert_eq!(dir8(PI), 4); // West
        assert_eq!(dir8(-PI / 2.0), 6); // North (-z)
    }

    #[test]
    fn walks_in_8_directions_no_gravity() {
        let solid = natural_solid(CelestialBody::Moon, 0); // regolith = all walkable
        let mut b = MovementBody::new(5.0, PLAYER_RADIUS);
        b.x = 100.0;
        b.z = 100.0;
        let mut input = empty_input(1);
        // Move north-east (x+, z-).
        input.move_x = 1.0;
        input.move_z = -1.0;
        let (x0, z0, y0) = (b.x, b.z, b.y);
        for _ in 0..60 {
            step_topdown_movement(&mut b, &input, 0.05, &solid);
        }
        assert!(b.x > x0 + 1.0, "should move east");
        assert!(b.z < z0 - 1.0, "should move north");
        assert_eq!(b.y, y0, "no vertical drift in top-down");
    }

    #[test]
    fn collision_blocks_into_wall() {
        // A solid column at tx=105: walking east must stop before it.
        let solid = |tx: i64, _tz: i64| tx >= 105;
        let mut b = MovementBody::new(5.0, PLAYER_RADIUS);
        b.x = 100.0;
        b.z = 0.0;
        let mut input = empty_input(1);
        input.move_x = 1.0;
        for _ in 0..120 {
            step_topdown_movement(&mut b, &input, 0.05, &solid);
        }
        assert!(b.x < 105.0, "player must not pass into solid tiles, x={}", b.x);
        assert!(b.x > 103.0, "player should reach near the wall, x={}", b.x);
    }

    #[test]
    fn portals_round_trip_between_floors() {
        let body = CelestialBody::Earth;
        // The ladder up on floor z is exactly the hole down on floor z+1
        // (a coherent vertical shaft): descend then ascend returns you.
        for z in -6..0 {
            assert_eq!(
                ascend_portal(body, z),
                Some(descend_portal(body, z + 1)),
                "floor {z}: ladder up must match the hole above"
            );
        }
        // Surface has a descend hole but no ascend ladder.
        assert!(ascend_portal(body, 0).is_none());
        assert!(matches!(
            portal_here(body, 0, descend_portal(body, 0).0, descend_portal(body, 0).1),
            Some(Portal::Descend)
        ));
    }

    #[test]
    fn portal_cells_are_walkable() {
        let body = CelestialBody::Earth;
        for z in [-1, -3, -6] {
            let (px, pz) = descend_portal(body, z);
            assert!(
                !solid_at(body, z, px, pz),
                "descend portal on floor {z} must be walkable"
            );
        }
    }

    #[test]
    fn deposits_are_deterministic_and_deeper_is_richer() {
        let a = deposit_at(CelestialBody::Earth, -5, 42, 17);
        assert_eq!(a, deposit_at(CelestialBody::Earth, -5, 42, 17));
        let deep = (0..80)
            .flat_map(|tx| (0..80).map(move |tz| (tx, tz)))
            .filter(|(tx, tz)| deposit_at(CelestialBody::Earth, -5, *tx, *tz).is_some())
            .count();
        let surf = (0..80)
            .flat_map(|tx| (0..80).map(move |tz| (tx, tz)))
            .filter(|(tx, tz)| deposit_at(CelestialBody::Earth, 0, *tx, *tz).is_some())
            .count();
        assert!(deep > surf, "underground should have more deposits ({deep} vs {surf})");
    }
}
