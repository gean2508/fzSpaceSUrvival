//! Player-placed voxel blocks — Minecraft-style break/place on a 1 m tangent grid.
//!
//! Cells are integers in the active body's tangent frame: cell `(ix, iy, iz)`
//! occupies `x ∈ [ix, ix+1)`, `y ∈ [iy, iy+1)`, `z ∈ [iz, iz+1)` where `y` is
//! the same height axis the movement sim uses (`planet_ground_y_at` == ground).

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

pub const BLOCK_SIZE: f32 = 1.0;
/// Max auto step onto a lower block without jumping (Minecraft: none for 1 m).
pub const BLOCK_STEP_UP: f32 = 0.55;
/// Player capsule height used for side collision.
pub const BLOCK_COLLIDE_HEIGHT: f32 = 1.7;
pub const BLOCK_COLLIDE_RADIUS: f32 = 0.32;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum PlacedBlockKind {
    Dirt,
    Wood,
    Stone,
}

impl PlacedBlockKind {
    pub fn item_id(self) -> &'static str {
        match self {
            Self::Dirt => "dirt",
            Self::Wood => "wood",
            Self::Stone => "stone",
        }
    }

    pub fn from_item(item_id: &str) -> Option<Self> {
        match item_id {
            "dirt" => Some(Self::Dirt),
            "wood" => Some(Self::Wood),
            "stone" => Some(Self::Stone),
            _ => None,
        }
    }

    pub fn all() -> &'static [PlacedBlockKind] {
        &[Self::Dirt, Self::Wood, Self::Stone]
    }
}

pub fn is_block_item(item_id: &str) -> bool {
    PlacedBlockKind::from_item(item_id).is_some()
}

pub type BlockCell = (i32, i32, i32);

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct PlacedBlockWorld {
    pub cells: HashMap<BlockCell, PlacedBlockKind>,
}

pub fn cell_of(x: f32, y: f32, z: f32) -> BlockCell {
    (x.floor() as i32, y.floor() as i32, z.floor() as i32)
}

pub fn cell_center(c: BlockCell) -> [f32; 3] {
    [c.0 as f32 + 0.5, c.1 as f32 + 0.5, c.2 as f32 + 0.5]
}

impl PlacedBlockWorld {
    pub fn get(&self, c: BlockCell) -> Option<PlacedBlockKind> {
        self.cells.get(&c).copied()
    }

    pub fn insert(&mut self, c: BlockCell, kind: PlacedBlockKind) -> bool {
        if self.cells.contains_key(&c) {
            return false;
        }
        self.cells.insert(c, kind);
        true
    }

    pub fn remove(&mut self, c: BlockCell) -> Option<PlacedBlockKind> {
        self.cells.remove(&c)
    }

    pub fn len(&self) -> usize {
        self.cells.len()
    }

    pub fn is_empty(&self) -> bool {
        self.cells.is_empty()
    }

    /// Highest block top at the XZ column whose top is at or below `max_top`
    /// — the walkable ground contribution for a body at that height.
    pub fn ground_at(&self, tx: f32, tz: f32, max_top: f32) -> Option<f32> {
        let ix = tx.floor() as i32;
        let iz = tz.floor() as i32;
        let mut best: Option<f32> = None;
        for (&(cx, cy, cz), _) in &self.cells {
            if cx != ix || cz != iz {
                continue;
            }
            let top = (cy + 1) as f32;
            if top <= max_top && best.map(|b| top > b).unwrap_or(true) {
                best = Some(top);
            }
        }
        best
    }

    /// True when a capsule at `(tx, tz)` feet `y` overlaps a block it cannot
    /// step onto — the caller should cancel the horizontal move.
    pub fn blocks_capsule(&self, tx: f32, tz: f32, feet_y: f32) -> bool {
        let lo = (feet_y + 0.05).floor() as i32;
        let hi = (feet_y + BLOCK_COLLIDE_HEIGHT).floor() as i32;
        for level in lo..=hi {
            for (dx, dz) in [
                (0.0, 0.0),
                (BLOCK_COLLIDE_RADIUS, 0.0),
                (-BLOCK_COLLIDE_RADIUS, 0.0),
                (0.0, BLOCK_COLLIDE_RADIUS),
                (0.0, -BLOCK_COLLIDE_RADIUS),
            ] {
                let c = (
                    (tx + dx).floor() as i32,
                    level,
                    (tz + dz).floor() as i32,
                );
                if let Some(_) = self.cells.get(&c) {
                    let top = (level + 1) as f32;
                    if top - feet_y > BLOCK_STEP_UP {
                        return true;
                    }
                }
            }
        }
        false
    }

    /// Voxel DDA raycast in tangent space. Returns the first solid cell hit
    /// and the empty cell adjacent to the face that was entered (place slot).
    pub fn raycast(
        &self,
        origin: [f32; 3],
        dir: [f32; 3],
        max_dist: f32,
    ) -> Option<(BlockCell, BlockCell, f32)> {
        let len = (dir[0] * dir[0] + dir[1] * dir[1] + dir[2] * dir[2]).sqrt();
        if len < 1e-6 || self.cells.is_empty() {
            return None;
        }
        let d = [dir[0] / len, dir[1] / len, dir[2] / len];
        let mut cell = cell_of(origin[0], origin[1], origin[2]);
        let step = [sign_i(d[0]), sign_i(d[1]), sign_i(d[2])];
        let mut t_max = [
            axis_t_max(origin[0], d[0]),
            axis_t_max(origin[1], d[1]),
            axis_t_max(origin[2], d[2]),
        ];
        let t_delta = [axis_t_delta(d[0]), axis_t_delta(d[1]), axis_t_delta(d[2])];
        let mut prev = cell;
        let mut t = 0.0f32;

        for _ in 0..96 {
            if self.cells.contains_key(&cell) {
                return Some((cell, prev, t));
            }
            let axis = if t_max[0] <= t_max[1] && t_max[0] <= t_max[2] {
                0
            } else if t_max[1] <= t_max[2] {
                1
            } else {
                2
            };
            t = t_max[axis];
            if t > max_dist {
                return None;
            }
            prev = cell;
            match axis {
                0 => {
                    cell.0 += step[0];
                    t_max[0] += t_delta[0];
                }
                1 => {
                    cell.1 += step[1];
                    t_max[1] += t_delta[1];
                }
                _ => {
                    cell.2 += step[2];
                    t_max[2] += t_delta[2];
                }
            }
        }
        None
    }
}

fn sign_i(v: f32) -> i32 {
    if v > 0.0 {
        1
    } else if v < 0.0 {
        -1
    } else {
        0
    }
}

fn axis_t_max(origin: f32, d: f32) -> f32 {
    if d > 0.0 {
        (origin.floor() + 1.0 - origin) / d
    } else if d < 0.0 {
        (origin - origin.floor()) / -d
    } else {
        f32::INFINITY
    }
}

fn axis_t_delta(d: f32) -> f32 {
    if d.abs() < 1e-9 {
        f32::INFINITY
    } else {
        1.0 / d.abs()
    }
}

/// March a tangent-space ray against analytic terrain — for placing the first
/// block on open ground. Returns the cell sitting on the terrain at the hit.
pub fn raycast_terrain_place_cell(
    surface: crate::celestial_bodies::CelestialBody,
    origin: [f32; 3],
    dir: [f32; 3],
    max_dist: f32,
) -> Option<BlockCell> {
    let len = (dir[0] * dir[0] + dir[1] * dir[1] + dir[2] * dir[2]).sqrt();
    if len < 1e-6 {
        return None;
    }
    let d = [dir[0] / len, dir[1] / len, dir[2] / len];
    let step = 0.25f32;
    let mut t = 0.6f32;
    while t <= max_dist {
        let x = origin[0] + d[0] * t;
        let y = origin[1] + d[1] * t;
        let z = origin[2] + d[2] * t;
        let ground = crate::planet_surface::planet_ground_y_at(surface, x, z);
        if y <= ground {
            let iy = ground.floor() as i32;
            return Some((x.floor() as i32, iy, z.floor() as i32));
        }
        t += step;
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn insert_remove_roundtrip() {
        let mut w = PlacedBlockWorld::default();
        assert!(w.insert((1, 0, 2), PlacedBlockKind::Dirt));
        assert!(!w.insert((1, 0, 2), PlacedBlockKind::Stone));
        assert_eq!(w.remove((1, 0, 2)), Some(PlacedBlockKind::Dirt));
        assert!(w.is_empty());
    }

    #[test]
    fn raycast_hits_block_and_reports_place_face() {
        let mut w = PlacedBlockWorld::default();
        w.insert((3, 0, 0), PlacedBlockKind::Stone);
        let hit = w.raycast([0.5, 0.5, 0.5], [1.0, 0.0, 0.0], 8.0);
        let (cell, place, _t) = hit.expect("ray should hit");
        assert_eq!(cell, (3, 0, 0));
        assert_eq!(place, (2, 0, 0));
    }

    #[test]
    fn ground_at_returns_stack_top_below_limit() {
        let mut w = PlacedBlockWorld::default();
        w.insert((0, 0, 0), PlacedBlockKind::Dirt);
        w.insert((0, 1, 0), PlacedBlockKind::Dirt);
        assert_eq!(w.ground_at(0.4, 0.4, 2.5), Some(2.0));
        assert_eq!(w.ground_at(0.4, 0.4, 1.5), Some(1.0));
        assert_eq!(w.ground_at(5.0, 5.0, 3.0), None);
    }

    #[test]
    fn capsule_blocked_by_tall_wall_not_by_step() {
        let mut w = PlacedBlockWorld::default();
        w.insert((2, 0, 0), PlacedBlockKind::Stone);
        w.insert((2, 1, 0), PlacedBlockKind::Stone);
        // Feet at ground level next to a 2-high wall → blocked.
        assert!(w.blocks_capsule(2.5, 0.5, 0.0));
        // Standing on top of the wall → not blocked.
        assert!(!w.blocks_capsule(2.5, 0.5, 2.0));
    }
}
