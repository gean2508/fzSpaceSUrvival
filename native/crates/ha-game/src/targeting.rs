use bevy::prelude::*;
use ha_shared::{interact_radius_for, interact_reach_for, InteractTargetKind};

use crate::camera::look_forward;
use crate::debug::{span_guard, CpuSpanSlot, CpuSpanStats};
use crate::input::CameraLookIntent;
use crate::celestial_sphere::{moon_sphere_center, moon_sphere_radius, ray_sphere_t};
use crate::scene::{RegionId, RegionRuntime, WorldContextRuntime};
use crate::sim::Player;

#[derive(Component, Clone, Copy, Debug)]
pub struct InteractTarget {
    pub kind: InteractTargetKind,
    pub id: u32,
}

#[derive(Resource, Default, Clone, Copy, Debug)]
pub struct LookTarget {
    pub entity: Option<Entity>,
    pub kind: Option<InteractTargetKind>,
    pub distance: f32,
    pub hit_point: Vec3,
    pub block_cell: Option<(i32, i32, i32)>,
    pub place_cell: Option<(i32, i32, i32)>,
}

#[derive(Resource, Default)]
struct TargetingCache {
    player_hash: u32,
    yaw_bits: u32,
    pitch_bits: u32,
    region_tag: u8,
}

pub struct TargetingPlugin;

impl Plugin for TargetingPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<LookTarget>()
            .init_resource::<TargetingCache>()
            .add_systems(Update, update_look_target);
    }
}

fn target_allowed_in_region(kind: InteractTargetKind, region: RegionId) -> bool {
    match region {
        RegionId::Overworld { .. } => matches!(
            kind,
            InteractTargetKind::LaunchModule
                | InteractTargetKind::LaunchIgnition
                | InteractTargetKind::Tree
                | InteractTargetKind::Rock
                | InteractTargetKind::Container
                | InteractTargetKind::OreIron
                | InteractTargetKind::OreGold
                | InteractTargetKind::GemBlue
                | InteractTargetKind::GemRed
                | InteractTargetKind::Water
                | InteractTargetKind::Entity
        ),
        RegionId::Space => matches!(
            kind,
            InteractTargetKind::SpaceReentry
                | InteractTargetKind::SpaceTransitLuna
                | InteractTargetKind::SpaceTransitBody
                | InteractTargetKind::SpaceStationModule
        ),
        RegionId::Luna => matches!(
            kind,
            InteractTargetKind::LunarReturn
                | InteractTargetKind::Rock
                | InteractTargetKind::Container
                | InteractTargetKind::OreIron
                | InteractTargetKind::OreGold
                | InteractTargetKind::GemBlue
                | InteractTargetKind::GemRed
        ),
        RegionId::Planet(_) => matches!(
            kind,
            InteractTargetKind::PlanetReturn
                | InteractTargetKind::Rock
                | InteractTargetKind::Container
                | InteractTargetKind::OreIron
                | InteractTargetKind::OreGold
                | InteractTargetKind::GemBlue
                | InteractTargetKind::GemRed
        ),
        _ => false,
    }
}

fn update_look_target(
    look: Res<CameraLookIntent>,
    region: Res<RegionRuntime>,
    player: Query<&Transform, With<Player>>,
    targets: Query<(Entity, &GlobalTransform, &InteractTarget)>,
    mut current: ResMut<LookTarget>,
    mut cache: ResMut<TargetingCache>,
    mut span: ResMut<CpuSpanStats>,
) {
    let _guard = span_guard(&mut span, CpuSpanSlot::Target);

    let Ok(player_tf) = player.get_single() else {
        *current = LookTarget::default();
        return;
    };
    let origin = player_tf.translation + Vec3::Y * 1.55;
    let dir = look_forward(&look);
    if dir.length_squared() < 0.001 {
        *current = LookTarget::default();
        return;
    }

    let region_tag = region_tag(&region.active);
    let pos_hash = (player_tf.translation.x.to_bits() ^ player_tf.translation.z.to_bits()) as u32;
    let yaw_bits = look.yaw.to_bits();
    let pitch_bits = look.pitch.to_bits();
    if cache.player_hash == pos_hash
        && cache.yaw_bits == yaw_bits
        && cache.pitch_bits == pitch_bits
        && cache.region_tag == region_tag
    {
        return;
    }
    cache.player_hash = pos_hash;
    cache.yaw_bits = yaw_bits;
    cache.pitch_bits = pitch_bits;
    cache.region_tag = region_tag;

    let mut best: Option<(Entity, InteractTargetKind, f32, Vec3)> = None;
    for (entity, gtf, target) in &targets {
        if !target_allowed_in_region(target.kind, region.active) {
            continue;
        }
        let center = gtf.translation();
        let to = center - origin;
        let along = to.dot(dir);
        let reach = interact_reach_for(target.kind);
        if along < 0.2 || along > reach {
            continue;
        }
        let closest = origin + dir * along;
        let lateral = (center - closest).length();
        let radius = interact_radius_for(target.kind);
        if lateral > radius {
            continue;
        }
        if best.map(|(_, _, d, _)| along < d).unwrap_or(true) {
            best = Some((entity, target.kind, along, center));
        }
    }

    if let Some((entity, kind, distance, hit)) = best {
        *current = LookTarget {
            entity: Some(entity),
            kind: Some(kind),
            distance,
            hit_point: hit,
            block_cell: None,
            place_cell: None,
        };
        return;
    }

    *current = LookTarget::default();
}

fn region_tag(region: &RegionId) -> u8 {
    match region {
        RegionId::Overworld { .. } => 0,
        RegionId::Cave(_) => 1,
        RegionId::Space => 2,
        RegionId::Luna => 3,
        RegionId::Transit => 4,
        RegionId::Planet(_) => 5,
    }
}

fn update_space_moon_target(
    look: &CameraLookIntent,
    player: &Query<&Transform, With<Player>>,
    current: &mut LookTarget,
    cache: &mut TargetingCache,
    region: &RegionRuntime,
) {
    let Ok(player_tf) = player.get_single() else {
        *current = LookTarget::default();
        return;
    };
    let origin = player_tf.translation + Vec3::Y * 1.55;
    let dir = look_forward(look);
    if dir.length_squared() < 0.001 {
        *current = LookTarget::default();
        return;
    }
    let region_tag = region_tag(&region.active);
    let pos_hash = (player_tf.translation.x.to_bits() ^ player_tf.translation.z.to_bits()) as u32;
    let yaw_bits = look.yaw.to_bits();
    let pitch_bits = look.pitch.to_bits();
    if cache.player_hash == pos_hash
        && cache.yaw_bits == yaw_bits
        && cache.pitch_bits == pitch_bits
        && cache.region_tag == region_tag
    {
        return;
    }
    cache.player_hash = pos_hash;
    cache.yaw_bits = yaw_bits;
    cache.pitch_bits = pitch_bits;
    cache.region_tag = region_tag;

    let center = moon_sphere_center();
    let radius = moon_sphere_radius();
    if let Some(along) = ray_sphere_t(origin, dir, center, radius) {
        if along > 0.2 && along < interact_reach_for(InteractTargetKind::SpaceTransitLuna) {
            *current = LookTarget {
                entity: None,
                kind: Some(InteractTargetKind::SpaceTransitLuna),
                distance: along,
                hit_point: origin + dir * along,
                block_cell: None,
                place_cell: None,
            };
            return;
        }
    }
    *current = LookTarget::default();
}
