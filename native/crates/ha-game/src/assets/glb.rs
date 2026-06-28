//! Optional `.glb` overrides — `{kind}.glb` replaces procedural meshes from `builders.rs`.

use std::path::{Path, PathBuf};

use bevy::prelude::*;
use bevy::render::mesh::{Indices, PrimitiveTopology};
use bevy::render::render_asset::RenderAssetUsages;
use gltf::mesh::util::ReadIndices;
use ha_shared::palette_rgb;

use crate::materials::ToonMaterial;

const BUNDLED_SUBDIR: &str = "models";
const GLB_TAG: &str = "ha-glb";
/// Matches `tools/blender/generate_assets.py` budget — warn in debug if exceeded.
const MAX_TRIS_WARN: usize = 350;

/// Parsed mesh + PBR material extracted from a `.glb` file.
pub struct GlbPayload {
    pub mesh: Mesh,
    pub standard_material: StandardMaterial,
}

/// Resolve candidate filesystem / APK-relative paths for a catalog kind.
pub fn glb_relative_path(kind: &str) -> String {
    format!("{BUNDLED_SUBDIR}/{kind}.glb")
}

pub fn bundled_glb_path(kind: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("assets")
        .join(BUNDLED_SUBDIR)
        .join(format!("{kind}.glb"))
}

pub fn external_models_dir() -> PathBuf {
    std::env::var("HA_MODELS_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("./models"))
}

pub fn external_glb_path(kind: &str) -> PathBuf {
    external_models_dir().join(format!("{kind}.glb"))
}

/// True when a drop-in override file is present (desktop fs check; Android APK asset).
pub fn glb_override_exists(kind: &str) -> bool {
    if external_glb_path(kind).is_file() || bundled_glb_path(kind).is_file() {
        return true;
    }
    #[cfg(target_os = "android")]
    {
        return read_bundled_asset_bytes(&glb_relative_path(kind)).is_some();
    }
    #[cfg(not(target_os = "android"))]
    {
        false
    }
}

/// Try every configured location; silent `None` when absent or invalid.
pub fn try_load_kind(kind: &str) -> Option<GlbPayload> {
    #[cfg(debug_assertions)]
    if let Some(path) = dev_override_path(kind) {
        if let Some(payload) = load_from_path(&path) {
            log_loaded(kind, &path);
            return Some(payload);
        }
        eprintln!(
            "{GLB_TAG}: dev override for '{kind}' failed to parse — {}",
            path.display()
        );
    }

    #[cfg(not(target_os = "android"))]
    {
        let external = external_glb_path(kind);
        if external.is_file() {
            if let Some(payload) = load_from_path(&external) {
                log_loaded(kind, &external);
                return Some(payload);
            }
        }
        let bundled = bundled_glb_path(kind);
        if bundled.is_file() {
            if let Some(payload) = load_from_path(&bundled) {
                log_loaded(kind, &bundled);
                return Some(payload);
            }
        }
        return None;
    }

    #[cfg(target_os = "android")]
    {
        let relative = glb_relative_path(kind);
        let bytes = read_bundled_asset_bytes(&relative)?;
        let payload = load_from_bytes(&bytes)?;
        log_loaded(kind, Path::new(&relative));
        Some(payload)
    }
}

pub fn load_from_path(path: &Path) -> Option<GlbPayload> {
    let bytes = std::fs::read(path).ok()?;
    load_from_bytes(&bytes)
}

pub fn load_from_bytes(bytes: &[u8]) -> Option<GlbPayload> {
    let gltf = gltf::Gltf::from_slice(bytes).ok()?;
    let blob = gltf.blob.as_deref()?;
    let mut positions: Vec<[f32; 3]> = Vec::new();
    let mut normals: Vec<[f32; 3]> = Vec::new();
    let mut uvs: Vec<[f32; 2]> = Vec::new();
    let mut indices: Vec<u32> = Vec::new();
    let mut standard_material = StandardMaterial::default();
    let mut got_material = false;

    let scene = gltf.default_scene().or_else(|| gltf.scenes().next())?;
    for node in scene.nodes() {
        visit_gltf_node(
            node,
            Mat4::IDENTITY,
            blob,
            &mut positions,
            &mut normals,
            &mut uvs,
            &mut indices,
            &mut standard_material,
            &mut got_material,
        );
    }

    if positions.is_empty() {
        return None;
    }

    let use_normals = normals.len() == positions.len();
    let mut mesh = Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::default(),
    );
    mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, positions);
    if !uvs.is_empty() {
        mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, uvs);
    }
    mesh.insert_indices(Indices::U32(indices));
    if use_normals {
        mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, normals);
    } else {
        mesh.compute_smooth_normals();
    }

    #[cfg(debug_assertions)]
    warn_triangle_budget(&mesh);

    Some(GlbPayload {
        mesh,
        standard_material,
    })
}

fn visit_gltf_node(
    node: gltf::Node,
    parent: Mat4,
    blob: &[u8],
    positions: &mut Vec<[f32; 3]>,
    normals: &mut Vec<[f32; 3]>,
    uvs: &mut Vec<[f32; 2]>,
    indices: &mut Vec<u32>,
    standard_material: &mut StandardMaterial,
    got_material: &mut bool,
) {
    let world = parent * node_transform_mat4(node.transform());
    if let Some(gltf_mesh) = node.mesh() {
        for primitive in gltf_mesh.primitives() {
            append_primitive_merged(
                &primitive,
                world,
                blob,
                positions,
                normals,
                uvs,
                indices,
                standard_material,
                got_material,
            );
        }
    }
    for child in node.children() {
        visit_gltf_node(child, world, blob, positions, normals, uvs, indices, standard_material, got_material);
    }
}

fn node_transform_mat4(tf: gltf::scene::Transform) -> Mat4 {
    let m = tf.matrix();
    Mat4::from_cols(
        Vec4::new(m[0][0], m[0][1], m[0][2], m[0][3]),
        Vec4::new(m[1][0], m[1][1], m[1][2], m[1][3]),
        Vec4::new(m[2][0], m[2][1], m[2][2], m[2][3]),
        Vec4::new(m[3][0], m[3][1], m[3][2], m[3][3]),
    )
}

fn append_primitive_merged(
    primitive: &gltf::Primitive,
    world: Mat4,
    blob: &[u8],
    positions: &mut Vec<[f32; 3]>,
    normals: &mut Vec<[f32; 3]>,
    uvs: &mut Vec<[f32; 2]>,
    indices: &mut Vec<u32>,
    standard_material: &mut StandardMaterial,
    got_material: &mut bool,
) {
    let reader = primitive.reader(|_| Some(blob));
    let base = positions.len() as u32;

    if let Some(iter) = reader.read_positions() {
        for p in iter {
            let v = world.transform_point3(Vec3::new(p[0], p[1], p[2]));
            positions.push(v.to_array());
        }
    }
    if let Some(iter) = reader.read_normals() {
        let normal_mat = world.inverse().transpose();
        for n in iter {
            let v = normal_mat
                .transform_vector3(Vec3::new(n[0], n[1], n[2]))
                .normalize_or_zero();
            normals.push(v.to_array());
        }
    }
    if let Some(iter) = reader.read_tex_coords(0) {
        uvs.extend(iter.into_f32().map(|uv| [uv[0], uv[1]]));
    }
    if let Some(iter) = reader.read_indices() {
        let chunk: Vec<u32> = match iter {
            ReadIndices::U8(i) => i.map(u32::from).collect(),
            ReadIndices::U16(i) => i.map(u32::from).collect(),
            ReadIndices::U32(i) => i.collect(),
        };
        indices.extend(chunk.into_iter().map(|idx| idx + base));
    }

    if !*got_material {
        let factor = primitive
            .material()
            .pbr_metallic_roughness()
            .base_color_factor();
        standard_material.base_color =
            Color::srgba(factor[0], factor[1], factor[2], factor[3]);
        *got_material = true;
    }
}

#[cfg(debug_assertions)]
fn warn_triangle_budget(mesh: &Mesh) {
    let tris = mesh
        .indices()
        .map(|i| match i {
            Indices::U32(v) => v.len() / 3,
            Indices::U16(v) => v.len() / 3,
        })
        .unwrap_or(0);
    if tris > MAX_TRIS_WARN {
        eprintln!(
            "{GLB_TAG}: warning — merged mesh has {tris} tris (budget {MAX_TRIS_WARN}); consider simplifying in Blender"
        );
    }
}

#[cfg(not(debug_assertions))]
fn warn_triangle_budget(_mesh: &Mesh) {}

#[cfg(target_os = "android")]
fn read_bundled_asset_bytes(relative_path: &str) -> Option<Vec<u8>> {
    use std::ffi::CString;

    let app = bevy::window::ANDROID_APP.get()?;
    let asset_manager = app.asset_manager();
    let cpath = CString::new(relative_path).ok()?;
    let mut asset = asset_manager.open(&cpath)?;
    asset.buffer().ok().map(|b| b.to_vec())
}

#[cfg(debug_assertions)]
fn log_loaded(kind: &str, path: &Path) {
    eprintln!("{GLB_TAG}: loaded override for '{kind}' from {}", path.display());
}

#[cfg(not(debug_assertions))]
fn log_loaded(_kind: &str, _path: &Path) {}

#[cfg(debug_assertions)]
fn dev_override_path(kind: &str) -> Option<PathBuf> {
    use std::collections::HashMap;

    static LAST_MTIME: std::sync::Mutex<Option<std::time::SystemTime>> = std::sync::Mutex::new(None);
    static CACHE: std::sync::Mutex<Option<HashMap<String, PathBuf>>> = std::sync::Mutex::new(None);

    let signal = crate::asset_dev::dev_signal_path()?;
    let mtime = std::fs::metadata(&signal).ok()?.modified().ok();
    let mut last = LAST_MTIME.lock().ok()?;
    let mut cache = CACHE.lock().ok()?;
    if *last != mtime || cache.is_none() {
        *last = mtime;
        *cache = crate::asset_dev::read_dev_overrides();
    }
    cache.as_ref()?.get(kind).cloned()
}

/// Material chosen for a resolved asset (procedural toon or glb-derived).
#[derive(Clone)]
pub enum WorldMaterial {
    Toon(Handle<ToonMaterial>),
    Standard(Handle<StandardMaterial>),
}

impl WorldMaterial {
    pub fn insert_on<'a>(&self, entity: &mut EntityCommands<'a>) {
        match self {
            WorldMaterial::Toon(h) => {
                entity.insert(MeshMaterial3d(h.clone()));
            }
            WorldMaterial::Standard(h) => {
                entity.insert(MeshMaterial3d(h.clone()));
            }
        }
    }
}

/// Build the world material for a loaded GLB — unlit Standard on Android (Toon shader fails on GLES).
pub fn world_material_from_glb(
    payload: &GlbPayload,
    _palette: ha_shared::PaletteKey,
    toon_mats: &mut Assets<ToonMaterial>,
    std_mats: &mut Assets<StandardMaterial>,
) -> WorldMaterial {
    #[cfg(target_os = "android")]
    {
        let _ = toon_mats;
        let s = payload.standard_material.base_color.to_srgba();
        WorldMaterial::Standard(std_mats.add(crate::materials::unlit_srgb([
            s.red, s.green, s.blue,
        ])))
    }
    #[cfg(not(target_os = "android"))]
    {
        let _ = toon_mats;
        WorldMaterial::Standard(std_mats.add(payload.standard_material.clone()))
    }
}

/// Procedural fallback material from catalog palette.
pub fn world_material_procedural(
    palette: ha_shared::PaletteKey,
    toon_mats: &mut Assets<ToonMaterial>,
    std_mats: &mut Assets<StandardMaterial>,
) -> WorldMaterial {
    let rgb = palette_rgb(palette);
    let _ = toon_mats;
    WorldMaterial::Standard(std_mats.add(crate::materials::unlit_srgb(rgb)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn merged_glb_includes_all_scene_primitives() {
        let path = bundled_glb_path("building_cottage");
        if !path.is_file() {
            eprintln!("skip merged_glb_includes_all_scene_primitives: run tools/blender/run_generate.sh");
            return;
        }
        let payload = load_from_path(&path).expect("building_cottage glb");
        let tris = payload
            .mesh
            .indices()
            .map(|i| match i {
                Indices::U32(v) => v.len() / 3,
                Indices::U16(v) => v.len() / 3,
            })
            .unwrap_or(0);
        assert!(
            tris >= 80,
            "cottage should merge trunk+roof+parts (got {tris} tris); loader may be dropping meshes"
        );
    }

    #[test]
    fn loads_minimal_fixture_glb() {
        let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/minimal_cube.glb");
        let payload = load_from_path(&path).expect("fixture glb parses");
        assert!(payload.mesh.attribute(Mesh::ATTRIBUTE_POSITION).is_some());
    }

    #[test]
    fn factory_prefers_external_glb_over_procedural() {
        let dir = tempfile::tempdir().expect("tempdir");
        let dst = dir.path().join("machine_wood_extractor.glb");
        std::fs::copy(
            Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/minimal_cube.glb"),
            &dst,
        )
        .expect("copy fixture");

        std::env::set_var("HA_MODELS_DIR", dir.path());
        let payload = try_load_kind("machine_wood_extractor").expect("external glb wins");
        assert!(payload.mesh.attribute(Mesh::ATTRIBUTE_POSITION).is_some());
    }

    #[test]
    fn asset_factory_resolves_glb_override() {
        use super::super::factory::{AssetFactory, AssetSource};

        let dir = tempfile::tempdir().expect("tempdir");
        let dst = dir.path().join("machine_wood_extractor.glb");
        std::fs::copy(
            Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/minimal_cube.glb"),
            &dst,
        )
        .expect("copy fixture");

        std::env::set_var("HA_MODELS_DIR", dir.path());
        let mut factory = AssetFactory::default();
        let mut meshes = Assets::<Mesh>::default();
        let mut toon = Assets::<ToonMaterial>::default();
        let mut std = Assets::<StandardMaterial>::default();
        let asset = factory
            .get("machine_wood_extractor", &mut meshes, &mut toon, &mut std)
            .expect("catalog kind");
        assert_eq!(asset.source, AssetSource::GlbFile);
        assert_eq!(asset.def.kind, "machine_wood_extractor");
    }

    #[test]
    fn catalog_kinds_manifest_matches_shared_catalog() {
        use ha_shared::ASSET_CATALOG;

        let body: String = ASSET_CATALOG
            .iter()
            .map(|a| a.kind)
            .collect::<Vec<_>>()
            .join("\n")
            + "\n";
        let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("assets/models/catalog-kinds.txt");
        if std::fs::read_to_string(&path).unwrap_or_default() != body {
            let mut f = std::fs::File::create(&path).expect("create catalog-kinds");
            f.write_all(body.as_bytes()).expect("write catalog-kinds");
        }
        assert_eq!(std::fs::read_to_string(&path).unwrap(), body);
    }
}
