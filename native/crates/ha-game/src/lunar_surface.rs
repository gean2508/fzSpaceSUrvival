//! Displaced moon sphere — walkable regolith mesh aligned with `luna_terrain` collision.

use bevy::prelude::*;
use bevy::render::mesh::{Indices, PrimitiveTopology};
use bevy::render::render_asset::RenderAssetUsages;
use ha_shared::{
    luna_albedo_at_dir, luna_displacement_at_dir, luna_dir_from_tangent, planet_displacement_at_dir,
    body_def, CelestialBody, MOON_GAMEPLAY_RADIUS_M,
};

use crate::assets::WorldMaterial;
use crate::materials::ToonMaterial;
use crate::moon_world::{LunarSurface, MoonDecor};
#[cfg(target_os = "android")]
const LUNA_LAT_SEGS: u32 = 40;
#[cfg(target_os = "android")]
const LUNA_LON_SEGS: u32 = 64;
#[cfg(not(target_os = "android"))]
const LUNA_LAT_SEGS: u32 = 48;
#[cfg(not(target_os = "android"))]
const LUNA_LON_SEGS: u32 = 80;

/// Full gameplay moon — legacy UV sphere (use `spawn_celestial_surface_patch` instead).
#[allow(dead_code)]
pub fn spawn_lunar_sphere(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    toon_mats: &mut Assets<ToonMaterial>,
    std_mats: &mut Assets<StandardMaterial>,
) {
    let r = MOON_GAMEPLAY_RADIUS_M;
    let center = Vec3::new(0.0, -r, 0.0);
    let mesh = meshes.add(build_displaced_moon_sphere(r, LUNA_LAT_SEGS, LUNA_LON_SEGS));
    #[cfg(target_os = "android")]
    {
        commands.spawn((
            Mesh3d(mesh),
            MeshMaterial3d(std_mats.add(crate::materials::luna_regolith_std())),
            Transform::from_translation(center),
            MoonDecor,
            LunarSurface,
            Name::new("LunarSphere"),
        ));
    }
    #[cfg(not(target_os = "android"))]
    {
        let _ = toon_mats;
        commands.spawn((
            Mesh3d(mesh),
            MeshMaterial3d(std_mats.add(crate::materials::luna_regolith_std())),
            Transform::from_translation(center),
            MoonDecor,
            LunarSurface,
            Name::new("LunarSphere"),
        ));
    }
}

pub fn build_displaced_moon_sphere(radius: f32, lat_segments: u32, lon_segments: u32) -> Mesh {
    build_displaced_sphere(CelestialBody::Moon, radius, lat_segments, lon_segments)
}

pub fn build_displaced_sphere(
    body: CelestialBody,
    radius: f32,
    lat_segments: u32,
    lon_segments: u32,
) -> Mesh {
    let lat = lat_segments.max(3) as usize;
    let lon = lon_segments.max(3) as usize;
    let rgb = body_def(body).color;
    let mut positions: Vec<[f32; 3]> = Vec::with_capacity((lat + 1) * (lon + 1));
    let mut normals: Vec<[f32; 3]> = Vec::with_capacity((lat + 1) * (lon + 1));
    let mut uvs: Vec<[f32; 2]> = Vec::with_capacity((lat + 1) * (lon + 1));
    let mut colors: Vec<[f32; 4]> = Vec::with_capacity((lat + 1) * (lon + 1));
    let mut indices: Vec<u32> = Vec::with_capacity(lat * lon * 6);

    for iy in 0..=lat {
        let v = iy as f32 / lat as f32;
        let phi = v * std::f32::consts::PI;
        let sin_phi = phi.sin();
        let cos_phi = phi.cos();
        for ix in 0..=lon {
            let u = ix as f32 / lon as f32;
            let theta = u * std::f32::consts::TAU;
            let dir = [
                sin_phi * theta.cos(),
                cos_phi,
                sin_phi * theta.sin(),
            ];
            let disp = if body == CelestialBody::Moon {
                luna_displacement_at_dir(dir)
            } else {
                planet_displacement_at_dir(body, dir)
            };
            let n = normalize_dir(dir);
            positions.push([
                n[0] * (radius + disp),
                n[1] * (radius + disp),
                n[2] * (radius + disp),
            ]);
            normals.push(n);
            uvs.push([u, v]);
            let albedo = if body == CelestialBody::Moon {
                luna_albedo_at_dir(dir)
            } else {
                let shade = 0.85 + dir[1] * 0.12;
                [rgb[0] * shade, rgb[1] * shade, rgb[2] * shade, 1.0]
            };
            colors.push(albedo);
        }
    }

    let row = lon + 1;
    for iy in 0..lat {
        for ix in 0..lon {
            let i0 = (iy * row + ix) as u32;
            let i1 = i0 + 1;
            let i2 = i0 + row as u32;
            let i3 = i2 + 1;
            indices.extend([i0, i2, i1, i1, i2, i3]);
        }
    }

    // Normals from displaced geometry — sphere normals hide crater relief under lighting.
    recompute_grid_normals(&mut normals, &positions, lat, lon, row);

    let mut mesh = Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::default());
    mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, positions);
    mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, normals);
    mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, uvs);
    mesh.insert_attribute(Mesh::ATTRIBUTE_COLOR, colors);
    mesh.insert_indices(Indices::U32(indices));
    mesh
}

fn normalize_dir(v: [f32; 3]) -> [f32; 3] {
    let len = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt().max(1e-6);
    [v[0] / len, v[1] / len, v[2] / len]
}

fn recompute_grid_normals(
    normals: &mut [[f32; 3]],
    positions: &[[f32; 3]],
    lat: usize,
    lon: usize,
    row: usize,
) {
    for iy in 0..=lat {
        for ix in 0..=lon {
            let idx = iy * row + ix;
            let pc = Vec3::from_array(positions[idx]);
            let pr = Vec3::from_array(positions[iy * row + (ix + 1).min(lon)]);
            let pu = Vec3::from_array(positions[(iy + 1).min(lat) * row + ix]);
            let pl = Vec3::from_array(positions[iy * row + ix.saturating_sub(1)]);
            let pd = Vec3::from_array(positions[iy.saturating_sub(1) * row + ix]);
            let mut n = (pr - pl).cross(pu - pd);
            if n.length_squared() < 1e-8 {
                n = pc.normalize_or_zero();
            } else {
                n = n.normalize();
            }
            if n.dot(pc.normalize_or_zero()) < 0.0 {
                n = -n;
            }
            normals[idx] = n.to_array();
        }
    }
}

/// Walkable tangent patch in world space — flat SFS feel on small-radius spheres.
pub fn build_planet_patch_mesh(
    body: CelestialBody,
    cx: f32,
    cz: f32,
    half: f32,
    step: f32,
) -> Mesh {
    use ha_shared::{
        body_def, body_walk_radius, planet_dir_from_tangent, planet_displacement_at_dir,
        planet_gameplay_center,
    };

    let step = step.max(0.5);
    let r = body_walk_radius(body);
    let center = Vec3::from_array(planet_gameplay_center(body));
    let rgb = body_def(body).color;
    let cols = ((half * 2.0) / step).ceil() as usize + 1;
    let rows = cols;
    let mut positions = Vec::with_capacity(cols * rows);
    let mut normals = Vec::with_capacity(cols * rows);
    let mut colors = Vec::with_capacity(cols * rows);
    let mut indices = Vec::new();

    for iz in 0..rows {
        let tz = cz - half + iz as f32 * step;
        for ix in 0..cols {
            let tx = cx - half + ix as f32 * step;
            let dir = if body == CelestialBody::Moon {
                luna_dir_from_tangent(tx, tz)
            } else {
                planet_dir_from_tangent(body, tx, tz)
            };
            let disp = ha_shared::surface_relief_at(body, tx, tz);
            let n = normalize_dir(dir);
            let radial = r + disp;
            let world = center + Vec3::from_array([
                n[0] * radial,
                n[1] * radial,
                n[2] * radial,
            ]);
            positions.push(world.to_array());
            normals.push(n);
            let albedo = if body == CelestialBody::Moon {
                luna_albedo_at_dir(dir)
            } else {
                let shade = 0.85 + dir[1] * 0.12;
                [rgb[0] * shade, rgb[1] * shade, rgb[2] * shade, 1.0]
            };
            colors.push(albedo);
        }
    }

    recompute_grid_normals(&mut normals, &positions, rows - 1, cols - 1, cols);

    for iz in 0..(rows - 1) {
        for ix in 0..(cols - 1) {
            let i0 = (iz * cols + ix) as u32;
            let i1 = i0 + 1;
            let i2 = i0 + cols as u32;
            let i3 = i2 + 1;
            indices.extend([i0, i2, i1, i1, i2, i3]);
        }
    }

    let mut mesh = Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::default());
    mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, positions);
    mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, normals);
    mesh.insert_attribute(Mesh::ATTRIBUTE_COLOR, colors);
    mesh.insert_indices(Indices::U32(indices));
    mesh
}

#[allow(dead_code)]
pub fn build_lunar_patch_mesh(cx: f32, cz: f32, half: f32, step: f32, _radius: f32) -> Mesh {
    build_planet_patch_mesh(CelestialBody::Moon, cx, cz, half, step)
}
