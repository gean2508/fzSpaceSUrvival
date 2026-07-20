//! Orbital view — bodies as real 2D circles on the shared plane (SFS map).
//!
//! Only spawned while RegionId::Space; positions track the live ephemerides
//! every frame (render x = world x, render y = -world z). B6: everything
//! despawns when returning to a surface.

use bevy::prelude::*;
use ha_shared::{body_atmo_top, body_radius, CelestialBody};

use crate::scene::{RegionId, RegionRuntime};
use crate::solar_system_clock::SolarSystemClock;

#[derive(Component)]
struct OrbitalBody {
    body: CelestialBody,
}

#[derive(Component)]
struct OrbitalSun;

pub struct Orbital2dPlugin;

impl Plugin for Orbital2dPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, sync_orbital_bodies);
    }
}

fn body_color(body: CelestialBody) -> Color {
    match body {
        // Earth keeps its bright ocean-blue disc (its own catalog colour is a
        // greener land tone). Every other body uses its catalog colour so the
        // orbital disc matches the airless landing fade (same colour → the
        // frame swap under the fade is invisible).
        CelestialBody::Earth => Color::srgb(0.30, 0.55, 0.70),
        other => {
            let [r, g, b] = ha_shared::body_def(other).color;
            Color::srgb(r, g, b)
        }
    }
}

fn sync_orbital_bodies(
    mut commands: Commands,
    region: Res<RegionRuntime>,
    clock: Res<SolarSystemClock>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<ColorMaterial>>,
    mut bodies: Query<(Entity, &OrbitalBody, &mut Transform)>,
    sun: Query<Entity, With<OrbitalSun>>,
) {
    let in_space = matches!(region.active, RegionId::Space);
    if !in_space {
        for (e, _, _) in &bodies {
            commands.entity(e).despawn_recursive();
        }
        for e in &sun {
            commands.entity(e).despawn_recursive();
        }
        return;
    }
    let t = clock.t;

    if bodies.is_empty() {
        for &body in CelestialBody::all() {
            let r = body_radius(body);
            let entity = commands
                .spawn((
                    OrbitalBody { body },
                    Mesh2d(meshes.add(Circle::new(r))),
                    MeshMaterial2d(materials.add(ColorMaterial::from_color(body_color(body)))),
                    Transform::from_xyz(0.0, 0.0, -5.0),
                ))
                .id();
            // Thin atmosphere halo for bodies that have one.
            let atmo = body_atmo_top(body);
            if atmo > 0.0 {
                let halo = commands
                    .spawn((
                        Mesh2d(meshes.add(Circle::new(r + atmo))),
                        MeshMaterial2d(materials.add(ColorMaterial::from_color(
                            Color::srgba(0.45, 0.65, 0.9, 0.25),
                        ))),
                        Transform::from_xyz(0.0, 0.0, -0.5),
                    ))
                    .id();
                commands.entity(entity).add_child(halo);
            }
        }
        // The Sun, far on the plane.
        let sun_pos = ha_shared::solar_ephemeris::sun_position();
        commands.spawn((
            OrbitalSun,
            Mesh2d(meshes.add(Circle::new(2200.0))),
            MeshMaterial2d(materials.add(ColorMaterial::from_color(Color::srgb(1.0, 0.85, 0.4)))),
            Transform::from_xyz(sun_pos[0], -sun_pos[2], -6.0),
        ));
        return;
    }

    for (_, ob, mut tf) in &mut bodies {
        let c = ha_shared::solar_ephemeris::body_center_at(ob.body, t);
        tf.translation = Vec3::new(c[0], -c[2], -5.0);
    }
}
