//! Modular building — snap grid, placement validation, player-only destruction.

use crate::constants::LAUNCH_PAD_ZONE_RADIUS;
use crate::constants::{LAUNCH_PAD_ZONE_X, LAUNCH_PAD_ZONE_Z};

/// Grid module size (metres) — ARK / Rust / Valheim industry standard ~2 m.
pub const BUILD_GRID: f32 = 2.0;
pub const BUILD_WALL_HEIGHT: f32 = 2.0;
pub const BUILD_FLOOR_THICK: f32 = 0.25;
pub const BUILD_PIECE_HP: f32 = 100.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum BuildPieceKind {
    FloorWood,
    WallWood,
    WallStone,
    RoofWood,
    StairsWood,
}

impl BuildPieceKind {
    pub fn asset_kind(self) -> &'static str {
        match self {
            Self::FloorWood => "build_floor_wood",
            Self::WallWood => "build_wall_wood",
            Self::WallStone => "build_wall_stone",
            Self::RoofWood => "build_roof_wood",
            Self::StairsWood => "build_stairs_wood",
        }
    }

    pub fn display_name(self) -> &'static str {
        match self {
            Self::FloorWood => "Wood floor",
            Self::WallWood => "Wood wall",
            Self::WallStone => "Stone wall",
            Self::RoofWood => "Wood roof",
            Self::StairsWood => "Wood stairs",
        }
    }

    pub fn build_cost(self) -> &'static [(&'static str, u32)] {
        match self {
            Self::FloorWood => &[("wood", 4)],
            Self::WallWood => &[("wood", 6), ("sticks", 2)],
            Self::WallStone => &[("stone", 6), ("wood", 2)],
            Self::RoofWood => &[("wood", 5), ("sticks", 2)],
            Self::StairsWood => &[("wood", 8), ("sticks", 4)],
        }
    }

    pub fn all() -> &'static [BuildPieceKind] {
        &[
            BuildPieceKind::FloorWood,
            BuildPieceKind::WallWood,
            BuildPieceKind::WallStone,
            BuildPieceKind::RoofWood,
            BuildPieceKind::StairsWood,
        ]
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct PlacedBuildPiece {
    pub uid: u32,
    pub piece: BuildPieceKind,
    /// Snapped grid X (tangent metres).
    pub grid_x: f32,
    /// Snapped grid Z (tangent metres).
    pub grid_z: f32,
    /// Vertical stack level (0 = ground shell).
    pub level: i32,
    /// 0..3 quarter turns.
    pub yaw_quarter: u8,
    pub hp: f32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BuildPlacementPreview {
    pub grid_x: f32,
    pub grid_z: f32,
    pub level: i32,
    pub yaw_quarter: u8,
    pub valid: bool,
}

pub fn snap_scalar(v: f32) -> f32 {
    (v / BUILD_GRID).round() * BUILD_GRID
}

pub fn snap_yaw_quarter(yaw: f32) -> u8 {
    let q = (yaw / (std::f32::consts::FRAC_PI_2)).round() as i32;
    ((q % 4 + 4) % 4) as u8
}

/// Snap a world hit on the tangent plane to the build grid.
pub fn preview_placement(
    hit_tx: f32,
    hit_tz: f32,
    look_yaw: f32,
    piece: BuildPieceKind,
    existing: &[PlacedBuildPiece],
) -> BuildPlacementPreview {
    let mut gx = snap_scalar(hit_tx);
    let mut gz = snap_scalar(hit_tz);
    let mut level = 0i32;
    let yaw_q = snap_yaw_quarter(look_yaw);

    if let Some(adj) = snap_to_neighbor(hit_tx, hit_tz, piece, existing) {
        gx = adj.grid_x;
        gz = adj.grid_z;
        level = adj.level;
    }

    let preview = BuildPlacementPreview {
        grid_x: gx,
        grid_z: gz,
        level,
        yaw_quarter: yaw_q,
        valid: false,
    };
    let valid = can_place_piece(piece, &preview, existing)
        && !in_launch_pad_zone(gx, gz);
    BuildPlacementPreview { valid, ..preview }
}

fn snap_to_neighbor(
    hit_tx: f32,
    hit_tz: f32,
    piece: BuildPieceKind,
    existing: &[PlacedBuildPiece],
) -> Option<BuildPlacementPreview> {
    let mut best: Option<(f32, BuildPlacementPreview)> = None;
    for p in existing {
        let dx = hit_tx - p.grid_x;
        let dz = hit_tz - p.grid_z;
        let dist2 = dx * dx + dz * dz;
        if dist2 > BUILD_GRID * BUILD_GRID * 2.5 {
            continue;
        }
        let candidates = neighbor_snaps(p, piece);
        for c in candidates {
            let ddx = hit_tx - c.grid_x;
            let ddz = hit_tz - c.grid_z;
            let d = ddx * ddx + ddz * ddz;
            if d < BUILD_GRID * BUILD_GRID * 1.1 {
                if best.map(|(bd, _)| d < bd).unwrap_or(true) {
                    best = Some((d, c));
                }
            }
        }
    }
    best.map(|(_, p)| p)
}

fn neighbor_snaps(anchor: &PlacedBuildPiece, piece: BuildPieceKind) -> Vec<BuildPlacementPreview> {
    let mut out = Vec::new();
    let yq = anchor.yaw_quarter;
    let (fx, fz) = forward_offset(yq);
    let (rx, rz) = right_offset(yq);

    match (anchor.piece, piece) {
        (BuildPieceKind::FloorWood, BuildPieceKind::WallWood | BuildPieceKind::WallStone) => {
            for &(ox, oz) in &[(fx, fz), (-fx, -fz), (rx, rz), (-rx, -rz)] {
                out.push(BuildPlacementPreview {
                    grid_x: anchor.grid_x + ox * BUILD_GRID,
                    grid_z: anchor.grid_z + oz * BUILD_GRID,
                    level: anchor.level,
                    yaw_quarter: yaw_for_edge(ox, oz),
                    valid: false,
                });
            }
        }
        (BuildPieceKind::WallWood | BuildPieceKind::WallStone, BuildPieceKind::WallWood | BuildPieceKind::WallStone) => {
            for &(ox, oz) in &[(fx, fz), (-fx, -fz)] {
                out.push(BuildPlacementPreview {
                    grid_x: anchor.grid_x + ox * BUILD_GRID,
                    grid_z: anchor.grid_z + oz * BUILD_GRID,
                    level: anchor.level,
                    yaw_quarter: anchor.yaw_quarter,
                    valid: false,
                });
            }
            out.push(BuildPlacementPreview {
                grid_x: anchor.grid_x,
                grid_z: anchor.grid_z,
                level: anchor.level + 1,
                yaw_quarter: anchor.yaw_quarter,
                valid: false,
            });
        }
        (BuildPieceKind::WallWood | BuildPieceKind::WallStone, BuildPieceKind::RoofWood) => {
            out.push(BuildPlacementPreview {
                grid_x: anchor.grid_x,
                grid_z: anchor.grid_z,
                level: anchor.level + 1,
                yaw_quarter: anchor.yaw_quarter,
                valid: false,
            });
        }
        (BuildPieceKind::FloorWood, BuildPieceKind::FloorWood) => {
            out.push(BuildPlacementPreview {
                grid_x: anchor.grid_x + fx * BUILD_GRID,
                grid_z: anchor.grid_z + fz * BUILD_GRID,
                level: anchor.level,
                yaw_quarter: 0,
                valid: false,
            });
        }
        (BuildPieceKind::FloorWood, BuildPieceKind::StairsWood) => {
            out.push(BuildPlacementPreview {
                grid_x: anchor.grid_x + fx * BUILD_GRID,
                grid_z: anchor.grid_z + fz * BUILD_GRID,
                level: anchor.level,
                yaw_quarter: yq,
                valid: false,
            });
        }
        _ => {}
    }
    out
}

fn forward_offset(yaw_q: u8) -> (f32, f32) {
    match yaw_q % 4 {
        0 => (0.0, 1.0),
        1 => (1.0, 0.0),
        2 => (0.0, -1.0),
        _ => (-1.0, 0.0),
    }
}

fn right_offset(yaw_q: u8) -> (f32, f32) {
    let (fx, fz) = forward_offset(yaw_q);
    (fz, -fx)
}

fn yaw_for_edge(ox: f32, oz: f32) -> u8 {
    if ox.abs() > oz.abs() {
        if ox > 0.0 { 1 } else { 3 }
    } else if oz > 0.0 {
        0
    } else {
        2
    }
}

fn in_launch_pad_zone(tx: f32, tz: f32) -> bool {
    let dx = tx - LAUNCH_PAD_ZONE_X;
    let dz = tz - LAUNCH_PAD_ZONE_Z;
    dx * dx + dz * dz < LAUNCH_PAD_ZONE_RADIUS * LAUNCH_PAD_ZONE_RADIUS
}

pub fn can_place_piece(
    piece: BuildPieceKind,
    at: &BuildPlacementPreview,
    existing: &[PlacedBuildPiece],
) -> bool {
    if occupies(at, existing) {
        return false;
    }
    match piece {
        BuildPieceKind::FloorWood => has_ground_support(at, existing),
        BuildPieceKind::WallWood | BuildPieceKind::WallStone => has_wall_support(at, existing),
        BuildPieceKind::RoofWood => has_roof_support(at, existing),
        BuildPieceKind::StairsWood => has_stair_support(at, existing),
    }
}

fn occupies(at: &BuildPlacementPreview, existing: &[PlacedBuildPiece]) -> bool {
    existing.iter().any(|p| {
        p.grid_x == at.grid_x && p.grid_z == at.grid_z && p.level == at.level
    })
}

fn has_ground_support(at: &BuildPlacementPreview, existing: &[PlacedBuildPiece]) -> bool {
    if at.level == 0 {
        return true;
    }
    existing.iter().any(|p| {
        p.piece == BuildPieceKind::FloorWood
            && p.grid_x == at.grid_x
            && p.grid_z == at.grid_z
            && p.level == at.level - 1
    }) || existing.iter().any(|p| {
        matches!(p.piece, BuildPieceKind::WallWood | BuildPieceKind::WallStone)
            && p.grid_x == at.grid_x
            && p.grid_z == at.grid_z
            && p.level == at.level - 1
    })
}

fn has_wall_support(at: &BuildPlacementPreview, existing: &[PlacedBuildPiece]) -> bool {
    if at.level == 0
        && existing.iter().any(|p| {
            p.piece == BuildPieceKind::FloorWood && p.grid_x == at.grid_x && p.grid_z == at.grid_z
        })
    {
        return true;
    }
    if existing.iter().any(|p| {
        matches!(p.piece, BuildPieceKind::WallWood | BuildPieceKind::WallStone)
            && p.grid_x == at.grid_x
            && p.grid_z == at.grid_z
            && p.level == at.level - 1
    }) {
        return true;
    }
    false
}

fn has_roof_support(at: &BuildPlacementPreview, existing: &[PlacedBuildPiece]) -> bool {
    existing.iter().any(|p| {
        matches!(p.piece, BuildPieceKind::WallWood | BuildPieceKind::WallStone)
            && p.grid_x == at.grid_x
            && p.grid_z == at.grid_z
            && p.level == at.level - 1
    })
}

fn has_stair_support(at: &BuildPlacementPreview, existing: &[PlacedBuildPiece]) -> bool {
    existing.iter().any(|p| {
        p.piece == BuildPieceKind::FloorWood && p.grid_x == at.grid_x && p.grid_z == at.grid_z
    })
}

/// Damage a player-built piece — returns true if destroyed. World terrain is never passed here.
pub fn damage_build_piece(piece: &mut PlacedBuildPiece, amount: f32) -> bool {
    piece.hp -= amount;
    piece.hp <= 0.0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn floor_snaps_to_grid() {
        let prev = preview_placement(10.3, -8.7, 0.0, BuildPieceKind::FloorWood, &[]);
        assert_eq!(prev.grid_x, 10.0);
        assert_eq!(prev.grid_z, -8.0);
        assert!(prev.valid);
    }

    #[test]
    fn wall_requires_floor() {
        let prev = preview_placement(4.0, 4.0, 0.0, BuildPieceKind::WallWood, &[]);
        assert!(!prev.valid);
    }
}
