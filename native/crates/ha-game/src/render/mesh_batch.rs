use bevy::prelude::*;
use bevy::render::mesh::{Indices, VertexAttributeValues};
use bevy::render::render_asset::RenderAssetUsages;
use bevy::render::render_resource::PrimitiveTopology as Topo;

/// Per-kind instance data from one streamed chunk — the world streamer merges
/// entries across all loaded chunks into a single mesh per kind.
pub struct ChunkBatchEntry {
    pub kind: String,
    pub material: crate::assets::WorldMaterial,
    pub mesh: Handle<Mesh>,
    pub transforms: Vec<Transform>,
}

/// Smooth terrain patch — vertex heights from `height_at` (matches `terrain_height_at` collision).
/// Vertices sit on a world-aligned lattice so adjacent chunks share watertight edges.
pub fn build_terrain_heightmap(
    x0: f32,
    z0: f32,
    x1: f32,
    z1: f32,
    step: f32,
    mut height_at: impl FnMut(f32, f32) -> f32,
) -> Mesh {
    build_terrain_heightmap_colored(x0, z0, x1, z1, step, height_at, |_, _, _| [1.0, 1.0, 1.0, 1.0])
}

/// Earth-tinted terrain with per-vertex albedo (multiplied in toon shader).
pub fn build_terrain_heightmap_colored(
    x0: f32,
    z0: f32,
    x1: f32,
    z1: f32,
    step: f32,
    mut height_at: impl FnMut(f32, f32) -> f32,
    mut color_at: impl FnMut(f32, f32, f32) -> [f32; 4],
) -> Mesh {
    build_terrain_heightmap_colored_with_slope(x0, z0, x1, z1, step, height_at, color_at, None::<fn(f32, f32) -> f32>)
}

/// Heightmap mesh — vertex Y must match `height_at` exactly (same fn as collision).
pub fn build_terrain_heightmap_colored_with_slope(
    x0: f32,
    z0: f32,
    x1: f32,
    z1: f32,
    step: f32,
    mut height_at: impl FnMut(f32, f32) -> f32,
    mut color_at: impl FnMut(f32, f32, f32) -> [f32; 4],
    _slope_at: Option<impl FnMut(f32, f32) -> f32>,
) -> Mesh {
    let step = step.max(0.5);
    // Shared global grid — chunk neighbours reuse identical edge vertices (no see-through seams).
    let ix0 = (x0 / step).floor() as i32;
    let iz0 = (z0 / step).floor() as i32;
    let ix1 = (x1 / step).ceil() as i32 + 1;
    let iz1 = (z1 / step).ceil() as i32 + 1;
    let cols = (ix1 - ix0 + 1) as usize;
    let rows = (iz1 - iz0 + 1) as usize;

    let mut positions: Vec<[f32; 3]> = Vec::with_capacity(cols * rows);
    let mut normals: Vec<[f32; 3]> = Vec::with_capacity(cols * rows);
    let mut uvs: Vec<[f32; 2]> = Vec::with_capacity(cols * rows);
    let mut colors: Vec<[f32; 4]> = Vec::with_capacity(cols * rows);
    let mut indices: Vec<u32> = Vec::with_capacity((cols.saturating_sub(1)) * (rows.saturating_sub(1)) * 6);

    for row in 0..rows {
        let z = (iz0 + row as i32) as f32 * step;
        for col in 0..cols {
            let x = (ix0 + col as i32) as f32 * step;
            let y = height_at(x, z);
            positions.push([x, y, z]);
            // Analytic central differences sample PAST the chunk edge — per-chunk
            // `compute_smooth_normals` left hard normal seams along borders.
            let hl = height_at(x - step, z);
            let hr = height_at(x + step, z);
            let hd = height_at(x, z - step);
            let hu = height_at(x, z + step);
            let n = Vec3::new(hl - hr, 2.0 * step, hd - hu).normalize_or_zero();
            normals.push(n.to_array());
            colors.push(color_at(x, z, y));
            let u = if cols > 1 {
                col as f32 / (cols - 1) as f32
            } else {
                0.0
            };
            let v = if rows > 1 {
                row as f32 / (rows - 1) as f32
            } else {
                0.0
            };
            uvs.push([u, v]);
        }
    }

    for row in 0..(rows.saturating_sub(1)) {
        for col in 0..(cols.saturating_sub(1)) {
            let i00 = (row * cols + col) as u32;
            let i10 = i00 + 1;
            let i01 = i00 + cols as u32;
            let i11 = i01 + 1;
            indices.extend([i00, i01, i10, i10, i01, i11]);
        }
    }

    let mut mesh = Mesh::new(Topo::TriangleList, RenderAssetUsages::RENDER_WORLD);
    mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, positions);
    mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, normals);
    mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, uvs);
    mesh.insert_attribute(Mesh::ATTRIBUTE_COLOR, colors);
    mesh.insert_indices(Indices::U32(indices));
    mesh
}

/// Merge axis-aligned cuboids at transforms into one mesh (1 draw call).
/// Merge axis-aligned cuboids of arbitrary sizes (world-space centres).
pub fn build_merged_sized_cuboids(cuboids: &[(Vec3, f32, f32, f32)]) -> Mesh {
    let mut positions: Vec<[f32; 3]> = Vec::new();
    let mut normals: Vec<[f32; 3]> = Vec::new();
    let mut uvs: Vec<[f32; 2]> = Vec::new();
    let mut indices: Vec<u32> = Vec::new();

    for &(center, w, h, d) in cuboids {
        let base = Mesh::from(Cuboid::new(w, h, d));
        let base_pos = base
            .attribute(Mesh::ATTRIBUTE_POSITION)
            .and_then(|a| match a {
                VertexAttributeValues::Float32x3(v) => Some(v.clone()),
                _ => None,
            })
            .unwrap_or_default();
        let base_norm = base
            .attribute(Mesh::ATTRIBUTE_NORMAL)
            .and_then(|a| match a {
                VertexAttributeValues::Float32x3(v) => Some(v.clone()),
                _ => None,
            })
            .unwrap_or_default();
        let base_uv = base
            .attribute(Mesh::ATTRIBUTE_UV_0)
            .and_then(|a| match a {
                VertexAttributeValues::Float32x2(v) => Some(v.clone()),
                _ => None,
            })
            .unwrap_or_default();
        let base_idx = base
            .indices()
            .map(|i| match i {
                Indices::U32(v) => v.clone(),
                Indices::U16(v) => v.iter().map(|&x| x as u32).collect(),
            })
            .unwrap_or_default();

        let offset = positions.len() as u32;
        for p in &base_pos {
            positions.push((center + Vec3::from_array(*p)).to_array());
        }
        normals.extend(base_norm.iter().copied());
        uvs.extend(base_uv.iter().copied());
        for idx in &base_idx {
            indices.push(idx + offset);
        }
    }

    let mut mesh = Mesh::new(Topo::TriangleList, RenderAssetUsages::RENDER_WORLD);
    mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, positions);
    mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, normals);
    mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, uvs);
    mesh.insert_indices(Indices::U32(indices));
    mesh
}

pub fn build_merged_cuboids(transforms: &[Transform], w: f32, h: f32, d: f32) -> Mesh {
    let mut positions: Vec<[f32; 3]> = Vec::new();
    let mut normals: Vec<[f32; 3]> = Vec::new();
    let mut uvs: Vec<[f32; 2]> = Vec::new();
    let mut indices: Vec<u32> = Vec::new();
    let base = Cuboid::new(w, h, d);
    let base_mesh = Mesh::from(base);
    let base_pos = base_mesh
        .attribute(Mesh::ATTRIBUTE_POSITION)
        .and_then(|a| match a {
            VertexAttributeValues::Float32x3(v) => Some(v.clone()),
            _ => None,
        })
        .unwrap_or_default();
    let base_norm = base_mesh
        .attribute(Mesh::ATTRIBUTE_NORMAL)
        .and_then(|a| match a {
            VertexAttributeValues::Float32x3(v) => Some(v.clone()),
            _ => None,
        })
        .unwrap_or_default();
    let base_uv = base_mesh
        .attribute(Mesh::ATTRIBUTE_UV_0)
        .and_then(|a| match a {
            VertexAttributeValues::Float32x2(v) => Some(v.clone()),
            _ => None,
        })
        .unwrap_or_default();
    let base_idx = base_mesh
        .indices()
        .map(|i| match i {
            Indices::U32(v) => v.clone(),
            Indices::U16(v) => v.iter().map(|&x| x as u32).collect(),
        })
        .unwrap_or_default();

    for tf in transforms {
        let offset = positions.len() as u32;
        for p in &base_pos {
            positions.push(tf.transform_point(Vec3::from_array(*p)).to_array());
        }
        for n in &base_norm {
            let v = (tf.rotation * Vec3::from_array(*n)).normalize_or_zero();
            normals.push(v.to_array());
        }
        uvs.extend(base_uv.iter().copied());
        for idx in &base_idx {
            indices.push(idx + offset);
        }
    }

    let mut mesh = Mesh::new(Topo::TriangleList, RenderAssetUsages::RENDER_WORLD);
    mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, positions);
    mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, normals);
    mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, uvs);
    mesh.insert_indices(Indices::U32(indices));
    mesh
}

/// Merge copies of an arbitrary mesh at each transform (1 draw call).
pub fn build_merged_instances(base: &Mesh, transforms: &[Transform]) -> Mesh {
    let base_pos = base
        .attribute(Mesh::ATTRIBUTE_POSITION)
        .and_then(|a| match a {
            VertexAttributeValues::Float32x3(v) => Some(v.clone()),
            _ => None,
        })
        .unwrap_or_default();
    let base_norm = base
        .attribute(Mesh::ATTRIBUTE_NORMAL)
        .and_then(|a| match a {
            VertexAttributeValues::Float32x3(v) => Some(v.clone()),
            _ => None,
        })
        .unwrap_or_default();
    let base_uv = base
        .attribute(Mesh::ATTRIBUTE_UV_0)
        .and_then(|a| match a {
            VertexAttributeValues::Float32x2(v) => Some(v.clone()),
            _ => None,
        })
        .unwrap_or_default();
    let base_idx = base
        .indices()
        .map(|i| match i {
            Indices::U32(v) => v.clone(),
            Indices::U16(v) => v.iter().map(|&x| x as u32).collect(),
        })
        .unwrap_or_default();

    let mut positions: Vec<[f32; 3]> = Vec::new();
    let mut normals: Vec<[f32; 3]> = Vec::new();
    let mut uvs: Vec<[f32; 2]> = Vec::new();
    let mut indices: Vec<u32> = Vec::new();

    for tf in transforms {
        let offset = positions.len() as u32;
        // Every attribute padded to EXACTLY base_pos.len() per instance — a
        // base mesh with short/missing UVs or normals must not desync the
        // merged buffers (truncated vertex buffer + out-of-range indices =
        // garbage stretched triangles).
        for (i, p) in base_pos.iter().enumerate() {
            positions.push(tf.transform_point(Vec3::from_array(*p)).to_array());
            let n = base_norm.get(i).copied().unwrap_or([0.0, 1.0, 0.0]);
            let v = (tf.rotation * Vec3::from_array(n)).normalize_or_zero();
            normals.push(v.to_array());
            uvs.push(base_uv.get(i).copied().unwrap_or([0.0, 0.0]));
        }
        for idx in &base_idx {
            indices.push(idx + offset);
        }
    }

    let mut mesh = Mesh::new(Topo::TriangleList, RenderAssetUsages::RENDER_WORLD);
    mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, positions);
    mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, normals);
    mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, uvs);
    mesh.insert_indices(Indices::U32(indices));
    mesh
}
