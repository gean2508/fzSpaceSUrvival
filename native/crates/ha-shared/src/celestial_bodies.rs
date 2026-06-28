//! Arcade solar-system catalog — all bodies visitable via rocket flight (SFS-style scale).

use crate::constants::{GRAVITY, GRAVITY_LUNA_SCALE};

/// Shared Y for all body centres (Earth flight shell).
pub const FLIGHT_PLANE_Y: f32 = -8_000.0;

#[repr(u8)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default, serde::Serialize, serde::Deserialize)]
pub enum CelestialBody {
    #[default]
    Earth = 0,
    Moon = 1,
    Mercury = 2,
    Venus = 3,
    Mars = 4,
    Phobos = 5,
    Deimos = 6,
    Io = 7,
    Europa = 8,
    Titan = 9,
    Uranus = 10,
    Neptune = 11,
    Sun = 12,
}

impl CelestialBody {
    pub fn label(self) -> &'static str {
        body_def(self).label
    }

    pub fn from_u8(v: u8) -> Option<Self> {
        match v {
            0 => Some(Self::Earth),
            1 => Some(Self::Moon),
            2 => Some(Self::Mercury),
            3 => Some(Self::Venus),
            4 => Some(Self::Mars),
            5 => Some(Self::Phobos),
            6 => Some(Self::Deimos),
            7 => Some(Self::Io),
            8 => Some(Self::Europa),
            9 => Some(Self::Titan),
            10 => Some(Self::Uranus),
            11 => Some(Self::Neptune),
            12 => Some(Self::Sun),
            _ => None,
        }
    }

    pub fn all() -> &'static [CelestialBody] {
        &[
            Self::Sun,
            Self::Mercury,
            Self::Venus,
            Self::Earth,
            Self::Moon,
            Self::Mars,
            Self::Phobos,
            Self::Deimos,
            Self::Io,
            Self::Europa,
            Self::Titan,
            Self::Uranus,
            Self::Neptune,
        ]
    }

    pub fn visitable_surfaces() -> &'static [CelestialBody] {
        &[
            Self::Earth,
            Self::Moon,
            Self::Mercury,
            Self::Venus,
            Self::Mars,
            Self::Phobos,
            Self::Deimos,
            Self::Io,
            Self::Europa,
            Self::Titan,
            Self::Uranus,
            Self::Neptune,
        ]
    }
}

#[derive(Clone, Copy, Debug)]
pub struct BodyDef {
    pub id: CelestialBody,
    pub label: &'static str,
    pub center: [f32; 3],
    /// Flight collision sphere (metres).
    pub radius_m: f32,
    /// Walkable displaced shell radius (metres).
    pub walk_radius_m: f32,
    pub soi_m: f32,
    pub gravity_surface: f32,
    /// Atmosphere top altitude; 0 = vacuum only.
    pub atmo_top_m: f32,
    pub landable: bool,
    pub color: [f32; 3],
    /// Initial coast speed from dock toward this body (m/s).
    pub transit_burn: f32,
    pub terrain_seed: u32,
}

pub fn body_def(body: CelestialBody) -> &'static BodyDef {
    static DEFS: &[BodyDef] = &[
        BodyDef {
            id: CelestialBody::Earth,
            label: "EARTH",
            center: [0.0, FLIGHT_PLANE_Y, 0.0],
            radius_m: 8_000.0,
            walk_radius_m: crate::cosmic_scale::EARTH_GAMEPLAY_RADIUS_M,
            soi_m: 22_000.0,
            gravity_surface: GRAVITY,
            atmo_top_m: 120.0,
            landable: true,
            color: [0.28, 0.52, 0.38],
            transit_burn: 55.0,
            terrain_seed: 0xE4_47_0001,
        },
        BodyDef {
            id: CelestialBody::Moon,
            label: "MOON",
            center: [52_000.0, FLIGHT_PLANE_Y, 0.0],
            radius_m: 420.0,
            walk_radius_m: 420.0,
            soi_m: 4_800.0,
            gravity_surface: GRAVITY * GRAVITY_LUNA_SCALE,
            atmo_top_m: 0.0,
            landable: true,
            color: [0.62, 0.64, 0.68],
            transit_burn: 130.0,
            terrain_seed: 0x1A6A_6001,
        },
        BodyDef {
            id: CelestialBody::Mercury,
            label: "MERCURY",
            center: [88_000.0, FLIGHT_PLANE_Y, 14_000.0],
            radius_m: 280.0,
            walk_radius_m: 280.0,
            soi_m: 3_200.0,
            gravity_surface: GRAVITY * 0.38,
            atmo_top_m: 0.0,
            landable: true,
            color: [0.55, 0.48, 0.42],
            transit_burn: 145.0,
            terrain_seed: 0x4D45_5201,
        },
        BodyDef {
            id: CelestialBody::Venus,
            label: "VENUS",
            center: [128_000.0, FLIGHT_PLANE_Y, -10_000.0],
            radius_m: 520.0,
            walk_radius_m: 520.0,
            soi_m: 5_500.0,
            gravity_surface: GRAVITY * 0.91,
            atmo_top_m: 80.0,
            landable: true,
            color: [0.82, 0.62, 0.35],
            transit_burn: 155.0,
            terrain_seed: 0x5645_4E01,
        },
        BodyDef {
            id: CelestialBody::Mars,
            label: "MARS",
            center: [198_000.0, FLIGHT_PLANE_Y, 0.0],
            radius_m: 380.0,
            walk_radius_m: 380.0,
            soi_m: 5_000.0,
            gravity_surface: GRAVITY * 0.38,
            atmo_top_m: 45.0,
            landable: true,
            color: [0.72, 0.38, 0.28],
            transit_burn: 175.0,
            terrain_seed: 0x4D41_5253,
        },
        BodyDef {
            id: CelestialBody::Phobos,
            label: "PHOBOS",
            center: [199_200.0, FLIGHT_PLANE_Y, 600.0],
            radius_m: 90.0,
            walk_radius_m: 90.0,
            soi_m: 900.0,
            gravity_surface: GRAVITY * 0.06,
            atmo_top_m: 0.0,
            landable: true,
            color: [0.48, 0.44, 0.40],
            transit_burn: 185.0,
            terrain_seed: 0x5048_4F01,
        },
        BodyDef {
            id: CelestialBody::Deimos,
            label: "DEIMOS",
            center: [196_800.0, FLIGHT_PLANE_Y, -900.0],
            radius_m: 70.0,
            walk_radius_m: 70.0,
            soi_m: 750.0,
            gravity_surface: GRAVITY * 0.04,
            atmo_top_m: 0.0,
            landable: true,
            color: [0.50, 0.46, 0.42],
            transit_burn: 185.0,
            terrain_seed: 0x4445_4901,
        },
        BodyDef {
            id: CelestialBody::Io,
            label: "IO",
            center: [278_000.0, FLIGHT_PLANE_Y, 2_400.0],
            radius_m: 200.0,
            walk_radius_m: 200.0,
            soi_m: 2_200.0,
            gravity_surface: GRAVITY * 0.18,
            atmo_top_m: 0.0,
            landable: true,
            color: [0.78, 0.58, 0.22],
            transit_burn: 210.0,
            terrain_seed: 0x494F_0001,
        },
        BodyDef {
            id: CelestialBody::Europa,
            label: "EUROPA",
            center: [274_000.0, FLIGHT_PLANE_Y, 4_200.0],
            radius_m: 190.0,
            walk_radius_m: 190.0,
            soi_m: 2_000.0,
            gravity_surface: GRAVITY * 0.13,
            atmo_top_m: 0.0,
            landable: true,
            color: [0.72, 0.78, 0.85],
            transit_burn: 210.0,
            terrain_seed: 0x4555_5201,
        },
        BodyDef {
            id: CelestialBody::Titan,
            label: "TITAN",
            center: [348_000.0, FLIGHT_PLANE_Y, 1_800.0],
            radius_m: 250.0,
            walk_radius_m: 250.0,
            soi_m: 2_800.0,
            gravity_surface: GRAVITY * 0.14,
            atmo_top_m: 60.0,
            landable: true,
            color: [0.68, 0.52, 0.28],
            transit_burn: 235.0,
            terrain_seed: 0x5449_5401,
        },
        BodyDef {
            id: CelestialBody::Uranus,
            label: "URANUS",
            center: [398_000.0, FLIGHT_PLANE_Y, 0.0],
            radius_m: 420.0,
            walk_radius_m: 420.0,
            soi_m: 4_500.0,
            gravity_surface: GRAVITY * 0.89,
            atmo_top_m: 55.0,
            landable: true,
            color: [0.45, 0.72, 0.78],
            transit_burn: 250.0,
            terrain_seed: 0x5552_414E,
        },
        BodyDef {
            id: CelestialBody::Neptune,
            label: "NEPTUNE",
            center: [448_000.0, FLIGHT_PLANE_Y, -8_000.0],
            radius_m: 400.0,
            walk_radius_m: 400.0,
            soi_m: 4_200.0,
            gravity_surface: GRAVITY * 1.12,
            atmo_top_m: 50.0,
            landable: true,
            color: [0.28, 0.42, 0.88],
            transit_burn: 265.0,
            terrain_seed: 0x4E45_5001,
        },
        BodyDef {
            id: CelestialBody::Sun,
            label: "SUN",
            center: [-180_000.0, FLIGHT_PLANE_Y, 0.0],
            radius_m: 12_000.0,
            walk_radius_m: 0.0,
            soi_m: 35_000.0,
            gravity_surface: GRAVITY * 28.0,
            atmo_top_m: 0.0,
            landable: false,
            color: [1.0, 0.82, 0.35],
            transit_burn: 280.0,
            terrain_seed: 0,
        },
    ];
    DEFS.iter()
        .find(|d| d.id == body)
        .unwrap_or(&DEFS[0])
}

pub fn body_center(body: CelestialBody) -> [f32; 3] {
    body_def(body).center
}

pub fn body_radius(body: CelestialBody) -> f32 {
    body_def(body).radius_m
}

pub fn body_walk_radius(body: CelestialBody) -> f32 {
    let d = body_def(body);
    if d.walk_radius_m > 0.0 {
        d.walk_radius_m
    } else {
        d.radius_m
    }
}

pub fn body_soi(body: CelestialBody) -> f32 {
    body_def(body).soi_m
}

pub fn body_gravity_surface(body: CelestialBody) -> f32 {
    body_def(body).gravity_surface
}

pub fn body_atmo_top(body: CelestialBody) -> f32 {
    body_def(body).atmo_top_m
}

pub fn body_landable(body: CelestialBody) -> bool {
    body_def(body).landable
}

pub fn body_transit_burn(body: CelestialBody) -> f32 {
    body_def(body).transit_burn
}

pub fn body_terrain_seed(body: CelestialBody) -> u32 {
    body_def(body).terrain_seed
}

pub fn earth_center() -> [f32; 3] {
    body_center(CelestialBody::Earth)
}

pub fn moon_center() -> [f32; 3] {
    body_center(CelestialBody::Moon)
}

pub const EARTH_FLIGHT_RADIUS_M: f32 = 8_000.0;
pub const MOON_ORBIT_X_M: f32 = 52_000.0;
pub const MOON_FLIGHT_RADIUS_M: f32 = 420.0;
pub const EARTH_SOI_M: f32 = 22_000.0;
pub const MOON_SOI_M: f32 = 4_800.0;
pub const EARTH_ATMO_TOP_M: f32 = 120.0;
pub const EARTH_CENTER_Y: f32 = FLIGHT_PLANE_Y;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_bodies_have_positive_radius() {
        for &b in CelestialBody::all() {
            assert!(body_radius(b) > 0.0, "{b:?}");
            assert!(body_soi(b) > body_radius(b), "{b:?}");
        }
    }

    #[test]
    fn moon_orbit_distance_matches_catalog() {
        let c = body_center(CelestialBody::Moon);
        assert!((c[0] - MOON_ORBIT_X_M).abs() < 1.0);
    }

    #[test]
    fn sun_is_not_landable() {
        assert!(!body_landable(CelestialBody::Sun));
    }
}
