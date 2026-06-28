//! Ghibli painted atmosphere — sky dome, cel lighting uniforms, local light pools.

use std::collections::HashSet;

use bevy::asset::AssetId;
use bevy::pbr::{DistanceFog, FogFalloff};
use bevy::prelude::*;
use ha_shared::Ps1Retro;
use crate::camera::GameCamera;

use crate::materials::{GhibliSkyMaterial, ToonMaterial, ps1_fog_for_zone};
use crate::sky_clock::{GameSkyClock, sample_sky};
use crate::debug::{span_guard, CpuSpanSlot, CpuSpanStats};
use crate::rocket_flight::RocketFlight;
use crate::scene::{CaveTorch, GhibliSkyDome, LaunchAtmosphere, RegionId, RegionRuntime, ZoneAtmosphere};
use crate::sim::Player;

/// Warm pool light attached to the player (lantern / firefly glow).
#[derive(Component)]
pub struct PlayerLantern;

pub struct GhibliAtmospherePlugin;

impl Plugin for GhibliAtmospherePlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<PaintState>().add_systems(
            Update,
            (
                tick_paint_state.after(crate::solar_lighting::sync_solar_lighting),
                sync_fog_with_zone.after(tick_paint_state),
                sync_ghibli_sky,
                sync_player_lantern,
                apply_paint_uniforms,
            )
                .chain(),
        );
    }
}

#[derive(Resource, Default)]
pub struct PaintState {
    pub night_factor: f32,
    pub sun_dir: Vec3,
    pub sun_strength: f32,
    pub sky_hemi: Vec3,
    pub ground_hemi: Vec3,
    pub zenith: Vec3,
    pub horizon: Vec3,
    pub warm: Vec3,
    pub neon_band: f32,
    pub local_pos: Vec3,
    pub local_radius: f32,
    pub local_color: Vec3,
    pub local_intensity: f32,
}

const LANTERN_DAY_RADIUS: f32 = 0.0;
const LANTERN_NIGHT_RADIUS: f32 = 16.0;
const LANTERN_CAVE_RADIUS: f32 = 11.0;

pub fn spawn_ghibli_sky(commands: &mut Commands, meshes: &mut Assets<Mesh>, mat: Handle<GhibliSkyMaterial>) {
    commands.spawn((
        Mesh3d(meshes.add(Sphere::new(420.0))),
        MeshMaterial3d(mat),
        Transform::from_scale(Vec3::new(-1.0, 1.0, -1.0)),
        GhibliSkyDome,
        Name::new("GhibliSky"),
    ));
}

fn tick_paint_state(
    region: Res<RegionRuntime>,
    zone: Res<ZoneAtmosphere>,
    launch_atmo: Res<LaunchAtmosphere>,
    clock: Res<GameSkyClock>,
    solar: Option<Res<crate::solar_lighting::SolarLightingState>>,
    player: Query<&Transform, With<Player>>,
    cave_torch: Query<&GlobalTransform, With<CaveTorch>>,
    mut paint: ResMut<PaintState>,
) {
    let sky = sample_sky(clock.day_fraction);
    let in_cave = matches!(region.active, RegionId::Cave(_));
    let on_space = matches!(region.active, RegionId::Space);
    let on_luna = matches!(region.active, RegionId::Luna);
    let on_transit = matches!(region.active, RegionId::Transit);

    let mut night = sky.night;
    if zone.current_zone == "mystic" {
        night = night.max(0.88);
    } else if zone.current_zone == "cave" {
        night = 0.95;
    } else if zone.current_zone == "meadow" {
        night = night.min(0.18);
    } else if launch_atmo.active {
        night = night.max(launch_atmo.blend * 0.4);
    }

    paint.night_factor = night;
    paint.neon_band = sky.neon_band;
    let solar_vis = solar
        .as_ref()
        .map(|s| s.sample.disk_visibility)
        .unwrap_or(1.0 - night * 0.5);
    paint.sun_strength = if in_cave {
        0.42
    } else if on_luna || on_space || on_transit {
        (0.35 + solar_vis * 0.75).clamp(0.2, 1.0)
    } else {
        (0.25 + solar_vis * 0.85).max(0.15)
    };

    paint.sky_hemi = sky.zenith;
    paint.ground_hemi = Vec3::from_array(Ps1Retro::SKY_GROUND);
    paint.zenith = sky.zenith;
    paint.horizon = sky.horizon;
    paint.warm = sky.warm;
    paint.sun_dir = solar
        .as_ref()
        .map(|s| Vec3::from_array(s.sample.lit_to_sun))
        .unwrap_or(sky.sun_dir);

    let Ok(player_tf) = player.get_single() else {
        return;
    };
    let mut light_pos = player_tf.translation + Vec3::new(0.0, 1.6, 0.0);
    let mut radius = if in_cave {
        LANTERN_CAVE_RADIUS
    } else if night > 0.45 {
        LANTERN_NIGHT_RADIUS
    } else {
        LANTERN_DAY_RADIUS
    };
    let mut intensity = if in_cave {
        1.35
    } else if night > 0.45 {
        1.1
    } else {
        0.0
    };

    if in_cave {
        if let Ok(torch_tf) = cave_torch.get_single() {
            light_pos = torch_tf.translation();
            radius = LANTERN_CAVE_RADIUS;
            intensity = 1.5;
        }
    }

    paint.local_pos = light_pos;
    paint.local_radius = radius;
    paint.local_color = Vec3::new(1.0, 0.76, 0.42);
    paint.local_intensity = intensity;
}

/// Exponential distance fog on the game camera — GLES3-safe (StandardMaterial path).
fn distance_fog_for_zone(zone: &str, region: &RegionId) -> DistanceFog {
    if matches!(
        region,
        RegionId::Luna | RegionId::Space | RegionId::Transit | RegionId::Planet(_)
    ) {
        return DistanceFog {
            color: Color::srgba(0.0, 0.0, 0.0, 1.0),
            falloff: FogFalloff::Exponential { density: 0.0 },
            ..default()
        };
    }
    match zone {
        "cave" => DistanceFog {
            color: Color::srgba(0.05, 0.04, 0.06, 1.0),
            falloff: FogFalloff::Exponential { density: 0.18 },
            ..default()
        },
        "mystic" => DistanceFog {
            color: Color::srgba(0.55, 0.35, 0.75, 1.0),
            falloff: FogFalloff::ExponentialSquared { density: 0.025 },
            ..default()
        },
        "launch" | "meadow" => DistanceFog {
            color: Color::srgba(0.75, 0.85, 0.92, 1.0),
            falloff: FogFalloff::Exponential { density: 0.008 },
            ..default()
        },
        _ => DistanceFog {
            color: Color::srgba(0.0, 0.0, 0.0, 0.0),
            falloff: FogFalloff::Exponential { density: 0.0 },
            ..default()
        },
    }
}

fn fog_zone_key(zone: &str, region: &RegionId) -> u64 {
    let mut s = zone.len() as u64;
    for b in zone.bytes() {
        s = s.wrapping_mul(31).wrapping_add(b as u64);
    }
    s.wrapping_mul(31).wrapping_add(match region {
        RegionId::Overworld { .. } => 0,
        RegionId::Cave(_) => 1,
        RegionId::Space => 2,
        RegionId::Luna | RegionId::Planet(_) => 3,
        RegionId::Transit => 4,
    })
}

fn sync_fog_with_zone(
    zone: Res<ZoneAtmosphere>,
    region: Res<RegionRuntime>,
    mut cache: Local<u64>,
    mut camera: Query<(Entity, Option<&mut DistanceFog>), With<GameCamera>>,
    mut commands: Commands,
) {
    let key = fog_zone_key(&zone.current_zone, &region.active);
    if *cache == key {
        return;
    }
    *cache = key;
    let fog = distance_fog_for_zone(&zone.current_zone, &region.active);
    if let Ok((entity, existing)) = camera.get_single_mut() {
        match existing {
            Some(mut current) => *current = fog,
            None => {
                commands.entity(entity).insert(fog);
            }
        }
    }
}

fn sync_ghibli_sky(
    player: Query<&Transform, (With<Player>, Without<GhibliSkyDome>)>,
    mut sky: Query<&mut Transform, (With<GhibliSkyDome>, Without<Player>)>,
) {
    if let Ok(mut sky_tf) = sky.get_single_mut() {
        if let Ok(player_tf) = player.get_single() {
            sky_tf.translation = player_tf.translation;
        }
    }
}

fn sync_player_lantern(
    paint: Res<PaintState>,
    region: Res<RegionRuntime>,
    player: Query<&Transform, (With<Player>, Without<PlayerLantern>)>,
    mut lantern: Query<
        (&mut PointLight, &mut Transform),
        (With<PlayerLantern>, Without<Player>),
    >,
) {
    let in_cave = matches!(region.active, RegionId::Cave(_));
    let Ok(player_tf) = player.get_single() else {
        return;
    };
    let Ok((mut light, mut tf)) = lantern.get_single_mut() else {
        return;
    };

    tf.translation = paint.local_pos;
    let active = paint.local_intensity > 0.01;
    light.intensity = if active {
        if in_cave {
            95_000.0
        } else {
            72_000.0 * paint.local_intensity
        }
    } else {
        0.0
    };
    light.range = if active { paint.local_radius } else { 0.0 };
    light.color = Color::srgb(paint.local_color.x, paint.local_color.y, paint.local_color.z);
}

const PAINT_PROP_RESCAN_FRAMES: u32 = 48;

#[derive(Default)]
struct PaintUniformCache {
    sig: u64,
    prop_materials: Vec<AssetId<ToonMaterial>>,
    frame: u32,
}

fn paint_uniform_signature(
    paint: &PaintState,
    region: &RegionId,
    zone: &str,
    cam_pos: Vec3,
    player_xz: Option<Vec2>,
) -> u64 {
    let q = |f: f32| (f * 4.0).round() as i32;
    let mut s = paint.night_factor.to_bits() as u64;
    s = s.wrapping_mul(31).wrapping_add(paint.sun_strength.to_bits() as u64);
    s = s.wrapping_mul(31).wrapping_add(paint.local_intensity.to_bits() as u64);
    s = s.wrapping_mul(31).wrapping_add(q(cam_pos.x) as u64);
    s = s.wrapping_mul(31).wrapping_add(q(cam_pos.y) as u64);
    s = s.wrapping_mul(31).wrapping_add(q(cam_pos.z) as u64);
    if let Some(p) = player_xz {
        s = s.wrapping_mul(31).wrapping_add(q(p.x) as u64);
        s = s.wrapping_mul(31).wrapping_add(q(p.y) as u64);
    }
    for b in zone.bytes() {
        s = s.wrapping_mul(31).wrapping_add(b as u64);
    }
    s = s.wrapping_mul(31).wrapping_add(match region {
        RegionId::Overworld { .. } => 0u64,
        RegionId::Cave(_) => 1,
        RegionId::Space => 2,
        RegionId::Luna | RegionId::Planet(_) => 3,
        RegionId::Transit => 4,
    });
    s
}

fn apply_paint_uniforms(
    paint: Res<PaintState>,
    zone: Res<ZoneAtmosphere>,
    region: Res<crate::scene::RegionRuntime>,
    sun: Query<&GlobalTransform, With<DirectionalLight>>,
    player: Query<&Transform, With<Player>>,
    camera: Query<&GlobalTransform, With<GameCamera>>,
    toon_meshes: Query<&MeshMaterial3d<ToonMaterial>>,
    mut materials: ResMut<Assets<ToonMaterial>>,
    mut cache: Local<PaintUniformCache>,
    mut span: ResMut<CpuSpanStats>,
) {
    let _guard = span_guard(&mut span, CpuSpanSlot::Atmo);

    let cam_pos = camera
        .get_single()
        .map(|tf| tf.translation())
        .or_else(|_| player.get_single().map(|tf| tf.translation + Vec3::Y * 1.6))
        .unwrap_or(Vec3::ZERO);
    let player_xz = player
        .get_single()
        .ok()
        .map(|tf| Vec2::new(tf.translation.x, tf.translation.z));

    let sig = paint_uniform_signature(&paint, &region.active, &zone.current_zone, cam_pos, player_xz);
    cache.frame = cache.frame.wrapping_add(1);
    let rescan_props = cache.frame % PAINT_PROP_RESCAN_FRAMES == 0 || cache.prop_materials.is_empty();
    if !rescan_props && sig == cache.sig {
        return;
    }
    cache.sig = sig;

    if rescan_props {
        cache.prop_materials.clear();
        for mat in &toon_meshes {
            cache.prop_materials.push(mat.id());
        }
        cache.prop_materials.sort_unstable();
        cache.prop_materials.dedup();
    }

    let sun_dir = sun
        .get_single()
        .map(|tf| -(tf.forward().as_vec3()))
        .unwrap_or(paint.sun_dir);

    let flat = Vec4::new(0.0, 0.0, 0.0, 0.0);

    let atmos_sky = Vec4::new(
        paint.sky_hemi.x,
        paint.sky_hemi.y,
        paint.sky_hemi.z,
        paint.night_factor,
    );
    let atmos_ground = Vec4::new(
        paint.ground_hemi.x,
        paint.ground_hemi.y,
        paint.ground_hemi.z,
        paint.sun_strength,
    );
    let local_light = Vec4::new(
        paint.local_pos.x,
        paint.local_pos.y,
        paint.local_pos.z,
        paint.local_radius,
    );
    let light_color = Vec4::new(
        paint.local_color.x,
        paint.local_color.y,
        paint.local_color.z,
        paint.local_intensity,
    );
    let ps1_settings = {
        let mut s = crate::materials::ps1_settings_for_region(&region.active);
        let (_, fog_start, fog_end) =
            ps1_fog_for_zone(&zone.current_zone, &region.active);
        s.y = fog_start;
        s.z = fog_end;
        s
    };
    let ps1_fog = {
        let (rgb, _, _) = ps1_fog_for_zone(&zone.current_zone, &region.active);
        rgb
    };
    let sun_v4 = Vec4::new(sun_dir.x, sun_dir.y, sun_dir.z, 0.0);
    let cam_v4 = Vec4::new(cam_pos.x, cam_pos.y, cam_pos.z, 0.0);

    let mut apply_one = |id: AssetId<ToonMaterial>| {
        if let Some(slot) = materials.get_mut(id) {
            slot.sun_dir = sun_v4;
            slot.world_curve = flat;
            slot.atmos_sky = atmos_sky;
            slot.atmos_ground = atmos_ground;
            slot.local_light = local_light;
            slot.light_color = light_color;
            slot.ps1_settings = ps1_settings;
            slot.ps1_fog_color = ps1_fog;
            slot.camera_pos = cam_v4;
        }
    };

    for id in &cache.prop_materials {
        apply_one(*id);
    }
}
