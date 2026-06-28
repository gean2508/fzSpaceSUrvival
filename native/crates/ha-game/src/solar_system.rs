//! Solar-system body markers in the orbital void — flight-scale positions, hub-compressed visuals.

use bevy::prelude::*;

use ha_shared::{body_center, body_def, body_radius, CelestialBody};

use crate::materials::{moon_globe_std, unlit_srgb};
use crate::space_world::SpaceDecor;

#[derive(Component)]
pub struct SolarBodyMarker {
    pub body: CelestialBody,
}

/// Hub visual scale: flight metres → dock neighbourhood metres.
const HUB_VISUAL_SCALE: f32 = 0.0042;

pub fn spawn_solar_system_bodies(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    std_mats: &mut Assets<StandardMaterial>,
    dock: Vec3,
) {
    for &body in CelestialBody::all() {
        if body == CelestialBody::Earth {
            continue;
        }
        let def = body_def(body);
        let flight_c = Vec3::from_array(body_center(body));
        let rel = (flight_c - dock) * HUB_VISUAL_SCALE;
        let vis_pos = dock + rel;
        let vis_r = (body_radius(body) * HUB_VISUAL_SCALE).clamp(1.2, 28.0);
        if body == CelestialBody::Sun {
            let mut mat = unlit_srgb(def.color);
            mat.emissive = LinearRgba::new(def.color[0], def.color[1], def.color[2], 1.0) * 4.0;
            commands.spawn((
                Mesh3d(meshes.add(Sphere::new(vis_r))),
                MeshMaterial3d(std_mats.add(mat)),
                Transform::from_translation(vis_pos),
                SolarBodyMarker { body },
                SpaceDecor,
                Name::new("SunMarker"),
            ));
            continue;
        }
        let mat = if body == CelestialBody::Moon {
            moon_globe_std()
        } else {
            unlit_srgb(def.color)
        };
        commands.spawn((
            Mesh3d(meshes.add(Sphere::new(vis_r))),
            MeshMaterial3d(std_mats.add(mat)),
            Transform::from_translation(vis_pos),
            SolarBodyMarker { body },
            SpaceDecor,
            Name::new(format!("Solar_{}", def.label)),
        ));
    }
}
