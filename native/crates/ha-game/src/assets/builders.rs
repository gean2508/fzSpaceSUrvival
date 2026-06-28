use bevy::prelude::*;
use bevy::render::mesh::{Indices, VertexAttributeValues};
use ha_shared::{AssetClass, AssetDef};

fn mesh<M: Into<Mesh>>(builder: M) -> Mesh {
    builder.into()
}

/// Build a prototype mesh for an asset definition (cached by `AssetFactory`).
pub fn build_mesh(def: &AssetDef) -> Mesh {
    let s = def.hints.scale;
    match def.class {
        AssetClass::Terrain => build_terrain(def.kind, s),
        AssetClass::Flora360 => build_tree(def.kind, s),
        AssetClass::FloraFlat => build_flat_flora(def.kind, s),
        AssetClass::Prop => build_prop(def.kind, s),
        AssetClass::Item => build_item(def.kind, s),
        AssetClass::Character | AssetClass::Creature => build_character(def.kind, s),
        AssetClass::Ui => build_ui_icon(s),
    }
}

fn build_terrain(kind: &str, scale: f32) -> Mesh {
    let (w, h, d) = if kind.contains("water") {
        (1.8, 0.08, 1.8)
    } else if kind.contains("cliff") {
        (1.4, 1.6, 0.6)
    } else if kind.starts_with("rock_") {
        (0.9, 0.7, 0.9)
    } else {
        (1.0, 0.25, 1.0)
    };
    mesh(Cuboid::new(w * scale, h * scale, d * scale))
}

fn build_tree(kind: &str, scale: f32) -> Mesh {
    let canopy_r = if kind.contains("pine") {
        0.55
    } else if kind.contains("cherry") || kind.contains("red") {
        0.75
    } else {
        0.65
    };
    mesh(Sphere::new(canopy_r * scale))
}

fn build_flat_flora(_kind: &str, scale: f32) -> Mesh {
    mesh(Sphere::new(0.22 * scale))
}

fn build_prop(kind: &str, scale: f32) -> Mesh {
    if kind == "machine_launch_pad" {
        return build_launch_pad_mesh();
    }
    let (w, h, d) = match kind {
        k if k.starts_with("building_barn") => (3.2, 2.0, 2.4),
        k if k.starts_with("building_") => (2.4, 1.6, 2.0),
        k if k.contains("bridge") => (2.8, 0.35, 1.0),
        k if k.contains("well") => (0.9, 1.1, 0.9),
        k if k.starts_with("machine_") => (1.4, 1.2, 1.0),
        k if k.contains("fence") => (1.2, 0.55, 0.15),
        k if k.contains("crate_large") => (1.1, 1.0, 1.1),
        k if k.contains("crate") || k.contains("barrel") => (0.7, 0.7, 0.7),
        k if k.contains("lantern") => (0.25, 0.35, 0.25),
        k if k.contains("bed") => (1.4, 0.35, 0.9),
        k if k.contains("table") => (1.2, 0.45, 0.8),
        k if k.contains("chair") => (0.45, 0.55, 0.45),
        _ => (1.0, 1.0, 1.0),
    };
    mesh(Cuboid::new(w * scale, h * scale, d * scale))
}

fn build_item(kind: &str, scale: f32) -> Mesh {
    if kind.contains("potion") {
        return mesh(Cylinder::new(0.12 * scale, 0.35 * scale));
    }
    if kind.contains("sword") || kind.contains("spear") || kind.contains("staff") {
        return mesh(Cuboid::new(0.08 * scale, 0.7 * scale, 0.08 * scale));
    }
    if kind.contains("bow") {
        return mesh(Torus {
            minor_radius: 0.08 * scale,
            major_radius: 0.35 * scale,
        });
    }
    if kind.contains("shield") {
        return mesh(Cylinder::new(0.35 * scale, 0.06 * scale));
    }
    if kind.contains("coin") {
        return mesh(Cylinder::new(0.18 * scale, 0.04 * scale));
    }
    if kind.contains("food") {
        return mesh(Sphere::new(0.2 * scale));
    }
    if kind.contains("ingot") {
        return mesh(Cuboid::new(0.35 * scale, 0.12 * scale, 0.2 * scale));
    }
    if kind.contains("log") {
        return mesh(Cylinder::new(0.15 * scale, 0.55 * scale));
    }
    if kind.contains("gem") || kind.contains("crystal") {
        return mesh(Sphere::new(0.18 * scale));
    }
    mesh(Cuboid::new(0.3 * scale, 0.3 * scale, 0.3 * scale))
}

fn build_character(kind: &str, scale: f32) -> Mesh {
    let (r, h) = if kind.contains("slime") {
        (0.45, 0.35)
    } else if kind.contains("wolf") || kind.contains("golem") {
        (0.4, 0.9)
    } else {
        (0.28, 1.05)
    };
    mesh(Capsule3d::new(r * scale, h * scale))
}

fn build_ui_icon(scale: f32) -> Mesh {
    mesh(Plane3d::new(Vec3::Y, Vec2::splat(0.45 * scale)))
}

/// Platform + central column — single draw call (merged cuboids).
/// ~32 m pad aligned with `LAUNCH_PAD_ZONE_RADIUS` (18 m).
fn build_launch_pad_mesh() -> Mesh {
    use crate::render::mesh_batch::build_merged_instances;

    let platform = Mesh::from(Cuboid::new(32.0, 0.5, 32.0));
    let column = Mesh::from(Cuboid::new(3.0, 4.0, 3.0));
    let plat = build_merged_instances(&platform, &[Transform::IDENTITY]);
    let col = build_merged_instances(&column, &[Transform::from_xyz(0.0, 2.25, 0.0)]);
    append_meshes(plat, col)
}

/// Pre-assembled starship on the pad — hull + tank + engine in one draw call.
pub fn build_demo_ship_mesh() -> Mesh {
    use crate::render::mesh_batch::build_merged_instances;

    let hull = Mesh::from(Cuboid::new(1.4, 2.8, 1.4));
    let fuel = Mesh::from(Capsule3d::new(0.65, 1.6));
    let engine = Mesh::from(Cylinder::new(0.55, 1.5));
    let slots = [
        Transform::from_xyz(-8.0, 0.75 + 1.4, 8.0),
        Transform::from_xyz(8.0, 0.75 + 1.1, 8.0),
        Transform::from_xyz(0.0, 0.75 + 0.9, -8.0),
    ];
    let m0 = build_merged_instances(&hull, &[slots[0]]);
    let m1 = build_merged_instances(&fuel, &[slots[1]]);
    let m2 = build_merged_instances(&engine, &[slots[2]]);
    append_meshes(append_meshes(m0, m1), m2)
}

pub(crate) fn append_meshes(a: Mesh, b: Mesh) -> Mesh {
    use bevy::render::mesh::{Indices, VertexAttributeValues};
    use bevy::render::render_asset::RenderAssetUsages;
    use bevy::render::render_resource::PrimitiveTopology as Topo;

    let pos_a = mesh_vec3(&a);
    let norm_a = mesh_normals(&a);
    let uv_a = mesh_vec2(&a);
    let idx_a = mesh_indices(&a);

    let pos_b = mesh_vec3(&b);
    let norm_b = mesh_normals(&b);
    let uv_b = mesh_vec2(&b);
    let idx_b = mesh_indices(&b);

    let offset = pos_a.len() as u32;
    let mut positions = pos_a;
    positions.extend(pos_b);
    let mut normals = norm_a;
    normals.extend(norm_b);
    let mut uvs = uv_a;
    uvs.extend(uv_b);
    let mut indices = idx_a;
    indices.extend(idx_b.into_iter().map(|i| i + offset));

    let mut mesh = Mesh::new(Topo::TriangleList, RenderAssetUsages::default());
    mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, positions);
    mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, normals);
    mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, uvs);
    mesh.insert_indices(Indices::U32(indices));
    mesh
}

fn mesh_vec3(m: &Mesh) -> Vec<[f32; 3]> {
    m.attribute(Mesh::ATTRIBUTE_POSITION)
        .and_then(|a| match a {
            VertexAttributeValues::Float32x3(v) => Some(v.clone()),
            _ => None,
        })
        .unwrap_or_default()
}

fn mesh_normals(m: &Mesh) -> Vec<[f32; 3]> {
    m.attribute(Mesh::ATTRIBUTE_NORMAL)
        .and_then(|a| match a {
            VertexAttributeValues::Float32x3(v) => Some(v.clone()),
            _ => None,
        })
        .unwrap_or_default()
}

fn mesh_vec2(m: &Mesh) -> Vec<[f32; 2]> {
    m.attribute(Mesh::ATTRIBUTE_UV_0)
        .and_then(|a| match a {
            VertexAttributeValues::Float32x2(v) => Some(v.clone()),
            _ => None,
        })
        .unwrap_or_default()
}

fn mesh_indices(m: &Mesh) -> Vec<u32> {
    m.indices()
        .map(|i| match i {
            Indices::U32(v) => v.clone(),
            Indices::U16(v) => v.iter().map(|&x| x as u32).collect(),
        })
        .unwrap_or_default()
}
