//! Batched interactable deposits on sphere surfaces (Luna + planets).

use std::collections::HashMap;

use bevy::prelude::*;
use ha_shared::{
    asset_kind_for_interactable, body_terrain_seed, interact_target_for_kind, CelestialBody,
    InteractableKind, SurfaceDeposit,
};
use ha_shared::{
    luna_dir_from_tangent, luna_displacement_at_dir, luna_gameplay_center, luna_gameplay_world_pos,
    planet_dir_from_tangent, planet_displacement_at_dir, planet_gameplay_center,
    planet_gameplay_world_pos, MOON_GAMEPLAY_RADIUS_M,
};

use crate::assets::AssetFactory;
use crate::materials::ToonMaterial;
use crate::render::mesh_batch::build_merged_instances;
use crate::render::particles::HarvestReactive;
use crate::targeting::InteractTarget;

fn surface_transform(body: CelestialBody, tx: f32, tz: f32, lift: f32, yaw: f32, scale: Vec3) -> Transform {
    let pos = deposit_world_pos(body, tx, tz, lift);
    let dir = deposit_surface_dir(body, tx, tz);
    let normal = Vec3::from_array(dir).normalize_or_zero();
    let rot = Quat::from_rotation_arc(Vec3::Y, normal) * Quat::from_rotation_y(yaw);
    Transform::from_translation(pos)
        .with_rotation(rot)
        .with_scale(scale)
}

fn deposit_world_pos(body: CelestialBody, tx: f32, tz: f32, lift: f32) -> Vec3 {
    match body {
        CelestialBody::Moon => Vec3::from_array(luna_gameplay_world_pos(tx, tz, lift)),
        _ => Vec3::from_array(planet_gameplay_world_pos(body, tx, tz, lift)),
    }
}

fn deposit_surface_dir(body: CelestialBody, tx: f32, tz: f32) -> [f32; 3] {
    match body {
        CelestialBody::Moon => luna_dir_from_tangent(tx, tz),
        _ => planet_dir_from_tangent(body, tx, tz),
    }
}

fn deposit_lift(body: CelestialBody, kind: InteractableKind, scale: f32) -> f32 {
    let base = match kind {
        InteractableKind::Tree => 1.1,
        InteractableKind::Rock => 0.45,
        InteractableKind::Container => 0.35,
        InteractableKind::OreIron => 0.5,
        InteractableKind::OreGold => 0.55,
        InteractableKind::GemBlue | InteractableKind::GemRed => 0.4,
    };
    base * scale
}

fn harvest_reactive(kind: InteractableKind) -> bool {
    matches!(
        kind,
        InteractableKind::Rock
            | InteractableKind::OreIron
            | InteractableKind::OreGold
            | InteractableKind::GemBlue
            | InteractableKind::GemRed
    )
}

/// Spawn mineral/resource nodes on a displaced sphere shell.
pub fn spawn_sphere_deposits<D: Component + Clone>(
    commands: &mut Commands,
    factory: &mut AssetFactory,
    meshes: &mut Assets<Mesh>,
    toon_mats: &mut Assets<ToonMaterial>,
    std_mats: &mut Assets<StandardMaterial>,
    body: CelestialBody,
    deposits: &[SurfaceDeposit],
    decor: D,
) {
    struct BatchAcc {
        material: crate::assets::WorldMaterial,
        mesh: Handle<Mesh>,
        transforms: Vec<Transform>,
    }
    let mut by_kind: HashMap<String, BatchAcc> = HashMap::new();

    for node in deposits {
        let kind_str = node.kind.tag();
        let target_kind = interact_target_for_kind(node.kind);
        let asset = factory.require(
            asset_kind_for_interactable(kind_str, node.id),
            meshes,
            toon_mats,
            std_mats,
        );
        let scale_hint = asset.def.hints.scale;
        let h = hash_u32(node.id ^ body_terrain_seed(body));
        let h2 = hash_u32(h);
        let yaw = (h2 as f32 / u32::MAX as f32) * std::f32::consts::TAU;
        let size = 0.55 + (h & 0xff) as f32 / 255.0 * 0.9;
        let lift = deposit_lift(body, node.kind, scale_hint * size);
        let tf = surface_transform(
            body,
            node.tangent_x,
            node.tangent_z,
            lift,
            yaw,
            Vec3::splat(scale_hint * size),
        );

        by_kind
            .entry(kind_str.to_string())
            .or_insert_with(|| BatchAcc {
                material: asset.material.clone(),
                mesh: asset.mesh.clone(),
                transforms: Vec::new(),
            })
            .transforms
            .push(tf);

        let mut marker = commands.spawn((
            tf,
            InteractTarget {
                kind: target_kind,
                id: node.id,
            },
            decor.clone(),
            Visibility::Visible,
            Name::new(format!("Deposit_{kind_str}_{}", node.id)),
        ));
        if harvest_reactive(node.kind) {
            marker.insert(HarvestReactive);
        }
    }

    for (kind_str, acc) in by_kind {
        if acc.transforms.is_empty() {
            continue;
        }
        let base_mesh = meshes.get(&acc.mesh).expect("deposit mesh").clone();
        let mesh = meshes.add(build_merged_instances(&base_mesh, &acc.transforms));
        let mut entity = commands.spawn((
            Mesh3d(mesh),
            Transform::IDENTITY,
            decor.clone(),
            Visibility::Visible,
            Name::new(format!("DepositBatch_{kind_str}_{}", body.label())),
        ));
        acc.material.insert_on(&mut entity);
    }
}

fn hash_u32(mut x: u32) -> u32 {
    x ^= x >> 16;
    x = x.wrapping_mul(0x7feb_352d);
    x ^= x >> 15;
    x = x.wrapping_mul(0x846c_a68b);
    x ^= x >> 16;
    x
}

/// Lift for return beacon clearance — deposits avoid spawn pad.
pub fn deposit_spawn_clear_radius(body: CelestialBody) -> f32 {
    match body {
        CelestialBody::Moon => 6.0,
        CelestialBody::Phobos | CelestialBody::Deimos => 4.0,
        _ => 5.0,
    }
}

#[allow(dead_code)]
fn deposit_shell_radius(body: CelestialBody) -> f32 {
    match body {
        CelestialBody::Moon => MOON_GAMEPLAY_RADIUS_M,
        b => ha_shared::body_walk_radius(b),
    }
}

#[allow(dead_code)]
fn deposit_center(body: CelestialBody) -> Vec3 {
    match body {
        CelestialBody::Moon => Vec3::from_array(luna_gameplay_center()),
        b => Vec3::from_array(planet_gameplay_center(b)),
    }
}

#[allow(dead_code)]
fn deposit_displacement(body: CelestialBody, dir: [f32; 3]) -> f32 {
    match body {
        CelestialBody::Moon => luna_displacement_at_dir(dir),
        b => planet_displacement_at_dir(b, dir),
    }
}
