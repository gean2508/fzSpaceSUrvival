use bevy::prelude::*;
use ha_shared::palette::Palette;
use ha_shared::{get_class, CelestialBody, Ps1Retro, WorldContext, DEFAULT_CLASS_ID};
use ha_shared::{LAUNCH_PAD_ZONE_X, LAUNCH_PAD_ZONE_Z};

use crate::assets::AssetFactory;
use crate::debug::{span_guard, CpuSpanSlot, CpuSpanStats};
use crate::earth_world::{EarthDecor, EarthSurface};
use crate::moon_world::MoonDecor;
use crate::space_world::SpaceDecor;
use crate::celestial_sphere::{spawn_transit_planets, TransitPlanet};
use crate::ghibli_atmosphere::PlayerLantern;
use crate::materials::{
    outline_material, OutlineShell, PlanetSurfaceMaterial, ToonMaterial,
};
use crate::rocket_flight::RocketFlight;
use crate::camera::GameCamera;
use crate::sim::{Player, PlayerVisual, SimState, CAVE_INTERIOR_ORIGIN};
use crate::surface_placement::{player_sphere_transform, sphere_surface_transform};
use crate::input::TouchActions;
use crate::sky_clock::OverworldStarLayer;

pub struct ScenePlugin;

#[derive(Resource, Default)]
struct RegionVisibilityCache {
    last: Option<RegionId>,
}

impl Plugin for ScenePlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<ZoneAtmosphere>()
            .init_resource::<RegionRuntime>()
            .init_resource::<WorldContextRuntime>()
            .init_resource::<LaunchAtmosphere>()
            .init_resource::<RegionVisibilityCache>()
            .init_resource::<StarfieldAnimState>()
            .add_systems(Startup, spawn_world)
            .add_systems(PostStartup, (force_initial_biome_visibility, sync_biome_visibility))
            .add_systems(
                Update,
                (
                    update_region_runtime,
                    transition_zone_atmosphere,
                    update_cave_visual_overrides,
                    tick_starfield_time,
                ),
            )
            .add_systems(
                Last,
                (
                    guard_overworld_view,
                    sync_biome_visibility.after(guard_overworld_view),
                ),
            );
    }
}

/// Legacy marker — meadow voxel terrain removed; Earth uses `EarthDecor`.
#[derive(Component)]
pub struct Terrain;

/// Underground mission shell (visibility toggled per region).
#[derive(Component)]
pub struct CaveInterior;

/// Interior painted sky dome (follows player).
#[derive(Component)]
pub struct GhibliSkyDome;

#[derive(Resource, Clone)]
pub struct ZoneAtmosphere {
    pub current_zone: &'static str,
    pub target_sky_color: Color,
    pub target_light_intensity: f32,
    pub firefly_density: f32,
    pub target_firefly_density: f32,
    pub transition_speed: f32,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum RegionId {
    Overworld { gx: i8, gz: i8 },
    Cave(u8),
    Space,
    Luna,
    /// Mercury, Mars, Titan, … — walkable displaced shell.
    Planet(CelestialBody),
    /// Jetpack arc Luna → Earth (void cinematic).
    Transit,
}

#[derive(Resource, Default)]
pub struct WorldContextRuntime {
    pub active: WorldContext,
}

#[derive(Resource, Default)]
pub struct LaunchAtmosphere {
    pub active: bool,
    pub blend: f32,
}

impl LaunchAtmosphere {
    /// 1.0 on pad → 0.0 at orbital void (inverse of ascent blend).
    pub fn atmosphere_alpha(&self) -> f32 {
        if !self.active {
            return 1.0;
        }
        (1.0 - self.blend).clamp(0.0, 1.0)
    }
}

#[derive(Resource)]
pub struct RegionRuntime {
    pub active: RegionId,
    pub cave_portal: Vec3,
    pub edge_pressed: bool,
    /// Surface position restored when leaving cave.
    pub cave_surface_return: Option<Vec3>,
}

impl Default for RegionRuntime {
    fn default() -> Self {
        Self {
            active: RegionId::Overworld { gx: 0, gz: 0 },
            cave_portal: Vec3::new(11.0, 0.0, -9.5),
            edge_pressed: false,
            cave_surface_return: None,
        }
    }
}

#[derive(Component)]
pub struct SolarLight;

#[derive(Component)]
struct CaveCrystal;

#[derive(Component)]
pub(crate) struct CaveTorch;

impl Default for ZoneAtmosphere {
    fn default() -> Self {
        Self {
            current_zone: "meadow",
            target_sky_color: Color::srgb(
                Ps1Retro::SKY_ZENITH[0],
                Ps1Retro::SKY_ZENITH[1],
                Ps1Retro::SKY_ZENITH[2],
            ),
            target_light_intensity: 28_000.0,
            firefly_density: 0.2,
            target_firefly_density: 0.2,
            transition_speed: 2.2,
        }
    }
}

pub(crate) fn spawn_world(
    mut commands: Commands,
    world_layout: Res<crate::sim::WorldLayout>,
    mut factory: ResMut<AssetFactory>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut toon_mats: ResMut<Assets<ToonMaterial>>,
    mut std_mats: ResMut<Assets<StandardMaterial>>,
    mut planet_mats: ResMut<Assets<PlanetSurfaceMaterial>>,
) {
    let _ = (&mut factory, &mut toon_mats, &world_layout);
    let sun_color = Ps1Retro::SUN;
    let sun_shadows = false;
    commands.spawn((
        DirectionalLight {
            illuminance: 32_000.0,
            shadows_enabled: sun_shadows,
            color: Color::srgb(sun_color[0], sun_color[1], sun_color[2]),
            ..default()
        },
        Transform::from_rotation(Quat::from_euler(EulerRot::XYZ, -1.15, 0.55, 0.0)),
        SolarLight,
        Name::new("Sun"),
    ));

    commands.insert_resource(AmbientLight {
        color: Color::srgb(
            Ps1Retro::AMBIENT[0],
            Ps1Retro::AMBIENT[1],
            Ps1Retro::AMBIENT[2],
        ),
        brightness: 220.0,
    });

    // Earth sphere + deposits: `EarthWorldPlugin`. Sky: `sky_clock`.
    spawn_transit_planets(&mut commands, &mut meshes, &mut planet_mats, &mut std_mats);

    let class = get_class(DEFAULT_CLASS_ID);
    let spawn_x = LAUNCH_PAD_ZONE_X;
    let spawn_z = LAUNCH_PAD_ZONE_Z;
    let spawn_y = ha_shared::ground_height_for_locomotion(spawn_x, spawn_z, class.radius);
    let capsule = meshes.add(Capsule3d::new(class.radius, class.radius * 2.2));
    let outline_scale = 1.0 + 0.06;
    let player_tf = player_sphere_transform(CelestialBody::Earth, spawn_x, spawn_z, spawn_y, class.radius, 0.0);

    let body_mat = MeshMaterial3d(std_mats.add(crate::materials::unlit_srgb(Palette::HERO)));

    commands
        .spawn((
            Mesh3d(capsule.clone()),
            body_mat,
            player_tf,
            Player,
            SimState {
                body: {
                    let mut body = ha_shared::MovementBody::new(class.move_speed, class.radius);
                    body.x = spawn_x;
                    body.z = spawn_z;
                    body.y = spawn_y;
                    body
                },
                hp: class.max_hp,
                max_hp: class.max_hp,
                prev_action: false,
                prev_jump: false,
            },
            PlayerVisual {
                from_x: spawn_x,
                to_x: spawn_x,
                from_z: spawn_z,
                to_z: spawn_z,
                from_y: spawn_y,
                to_y: spawn_y,
                radius: class.radius,
                ..default()
            },
            Visibility::Visible,
            Name::new("Player"),
        ))
        .with_children(|parent| {
            parent.spawn((
                Mesh3d(capsule),
                MeshMaterial3d(std_mats.add(outline_material())),
                Transform::from_scale(Vec3::splat(outline_scale)),
                OutlineShell,
            ));
            parent.spawn((
                PointLight {
                    intensity: 0.0,
                    range: 16.0,
                    shadows_enabled: false,
                    color: Color::srgb(1.0, 0.76, 0.42),
                    ..default()
                },
                Transform::from_xyz(0.0, 1.6, 0.0),
                PlayerLantern,
                crate::solar_lighting::CraftedLight,
                Name::new("PlayerLantern"),
            ));
        });

    let portal_tx = 11.0_f32;
    let portal_tz = -9.5_f32;
    let portal_tf = sphere_surface_transform(CelestialBody::Earth, portal_tx, portal_tz, 0.12, 0.0);
    commands.spawn((
        Mesh3d(meshes.add(Cylinder::new(1.15, 0.12))),
        MeshMaterial3d(std_mats.add(crate::materials::unlit_srgb([0.35, 0.22, 0.55]))),
        portal_tf,
        Name::new("CavePortal"),
    ));

    for (lx, lz) in [(-4.0, -3.5), (3.2, -2.0), (5.5, 4.0)] {
        commands.spawn((
            Mesh3d(meshes.add(Capsule3d::new(0.26, 0.95))),
            MeshMaterial3d(std_mats.add(crate::materials::unlit_srgb([0.38, 0.5, 0.62]))),
            Transform::from_translation(CAVE_INTERIOR_ORIGIN + Vec3::new(lx, 0.58, lz)),
            Visibility::Hidden,
            CaveCrystal,
            CaveInterior,
            Name::new("CaveCrystal"),
        ));
    }

    spawn_cave_interior(&mut commands, &mut meshes, &mut std_mats);
}

/// Lightweight underground room — region toggle only, no voxel dig.
fn spawn_cave_interior(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    std_mats: &mut Assets<StandardMaterial>,
) {
    let origin = CAVE_INTERIOR_ORIGIN;
    let room_half = 12.0_f32;
    let wall_h = 6.0_f32;
    let floor_mat = std_mats.add(crate::materials::unlit_srgb([0.10, 0.09, 0.13]));
    let wall_mat = std_mats.add(crate::materials::unlit_srgb([0.16, 0.13, 0.19]));
    let ceil_mat = std_mats.add(crate::materials::unlit_srgb([0.08, 0.07, 0.11]));

    commands.spawn((
        Mesh3d(meshes.add(Cuboid::new(room_half * 2.0, 0.35, room_half * 2.0))),
        MeshMaterial3d(floor_mat.clone()),
        Transform::from_translation(origin + Vec3::new(0.0, -0.18, 0.0)),
        CaveInterior,
        Visibility::Hidden,
        Name::new("CaveFloor"),
    ));
    commands.spawn((
        Mesh3d(meshes.add(Cuboid::new(room_half * 2.0, 0.25, room_half * 2.0))),
        MeshMaterial3d(ceil_mat),
        Transform::from_translation(origin + Vec3::new(0.0, wall_h + 0.12, 0.0)),
        CaveInterior,
        Visibility::Hidden,
        Name::new("CaveCeiling"),
    ));
    for (ox, oz) in [
        (room_half, 0.0),
        (-room_half, 0.0),
        (0.0, room_half),
        (0.0, -room_half),
    ] {
        let (sx, sz) = if ox.abs() > 0.1 {
            (0.35, room_half * 2.0)
        } else {
            (room_half * 2.0, 0.35)
        };
        commands.spawn((
            Mesh3d(meshes.add(Cuboid::new(sx, wall_h, sz))),
            MeshMaterial3d(wall_mat.clone()),
            Transform::from_translation(origin + Vec3::new(ox, wall_h * 0.5, oz)),
            CaveInterior,
            Visibility::Hidden,
            Name::new("CaveWall"),
        ));
    }

    commands.spawn((
        PointLight {
            intensity: 0.0,
            range: 10.0,
            shadows_enabled: false,
            color: Color::srgb(1.0, 0.76, 0.34),
            ..default()
        },
        Transform::from_translation(origin + Vec3::new(0.0, 2.8, 0.0)),
        CaveTorch,
        CaveInterior,
        Visibility::Hidden,
        crate::solar_lighting::CraftedLight,
        Name::new("CaveTorch"),
    ));
}

fn transition_zone_atmosphere(
    time: Res<Time>,
    region: Res<RegionRuntime>,
    launch_atmo: Res<LaunchAtmosphere>,
    player: Query<&SimState, With<Player>>,
    mut clear_color: ResMut<ClearColor>,
    mut zone: ResMut<ZoneAtmosphere>,
    mut span: ResMut<CpuSpanStats>,
) {
    let _guard = span_guard(&mut span, CpuSpanSlot::Atmo);
    let Ok(sim) = player.get_single() else {
        return;
    };
    // TODO(arq): atmósfera es CLIENTE render-only, jamás tocar estado lógico de shared/server.
    let tx = sim.body.x;
    let tz = sim.body.z;
    let in_cave = matches!(region.active, RegionId::Cave(_));
    let on_space = matches!(region.active, RegionId::Space);
    let on_luna = matches!(region.active, RegionId::Luna);
    let night_biome = !in_cave
        && !on_space
        && !on_luna
        && matches!(region.active, RegionId::Overworld { .. })
        && (tx > 14.0 || tz > 14.0 || tx + tz > 20.0);

    if launch_atmo.active {
        zone.current_zone = "launch";
        let b = launch_atmo.blend;
        zone.target_sky_color = Color::srgb(
            Ps1Retro::VOID[0] * b + Ps1Retro::SKY_ZENITH[0] * (1.0 - b),
            Ps1Retro::VOID[1] * b + Ps1Retro::SKY_ZENITH[1] * (1.0 - b),
            Ps1Retro::VOID[2] * b + Ps1Retro::SKY_ZENITH[2] * (1.0 - b),
        );
        zone.target_light_intensity = 30_000.0 * (1.0 - b * 0.8);
        zone.target_firefly_density = 0.0;
    } else if on_luna {
        zone.current_zone = "luna";
        zone.target_sky_color = Color::srgb(0.02, 0.02, 0.04);
        zone.target_light_intensity = 80_000.0;
        zone.target_firefly_density = 0.0;
    } else if matches!(region.active, RegionId::Transit) {
        zone.current_zone = "transit";
        zone.target_sky_color = Color::srgb(0.01, 0.01, 0.03);
        zone.target_light_intensity = 4_200.0;
        zone.target_firefly_density = 0.0;
    } else if on_space {
        zone.current_zone = "space";
        zone.target_sky_color = Color::srgb(0.02, 0.02, 0.05);
        zone.target_light_intensity = 5_800.0;
        zone.target_firefly_density = 0.0;
    } else if in_cave {
        zone.current_zone = "cave";
        zone.target_sky_color = Color::srgb(0.04, 0.05, 0.08);
        zone.target_light_intensity = 2_800.0;
        zone.target_firefly_density = 0.45;
    } else if night_biome {
        zone.current_zone = "mystic";
        zone.target_sky_color = Color::srgb(0.12, 0.14, 0.22);
        zone.target_light_intensity = 15_000.0;
        zone.target_firefly_density = 0.55;
    } else {
        zone.current_zone = "meadow";
        zone.target_sky_color = Color::srgb(
            Ps1Retro::SKY_ZENITH[0],
            Ps1Retro::SKY_ZENITH[1],
            Ps1Retro::SKY_HORIZON[2],
        );
        zone.target_light_intensity = 30_000.0;
        zone.target_firefly_density = 0.12;
    }

    let cur = clear_color.0.to_srgba();
    let tgt = zone.target_sky_color.to_srgba();
    #[cfg(not(target_os = "android"))]
    let alpha = if matches!(region.active, RegionId::Overworld { .. })
        && !launch_atmo.active
        && !in_cave
        && !on_space
        && !on_luna
    {
        1.0
    } else {
        (time.delta_secs() * zone.transition_speed).clamp(0.0, 1.0)
    };
    #[cfg(target_os = "android")]
    let alpha = (time.delta_secs() * zone.transition_speed).clamp(0.0, 1.0);
    clear_color.0 = Color::srgba(
        cur.red + (tgt.red - cur.red) * alpha,
        cur.green + (tgt.green - cur.green) * alpha,
        cur.blue + (tgt.blue - cur.blue) * alpha,
        1.0,
    );
    zone.firefly_density += (zone.target_firefly_density - zone.firefly_density) * alpha;
}

fn update_region_runtime(
    actions: Res<TouchActions>,
    mut region: ResMut<RegionRuntime>,
    mut world_ctx: ResMut<WorldContextRuntime>,
    mut player: Query<(&mut Transform, &mut SimState), With<Player>>,
) {
    let Ok((mut player_tf, mut sim)) = player.get_single_mut() else {
        return;
    };
    let just_pressed = actions.action && !region.edge_pressed;
    region.edge_pressed = actions.action;
    if !just_pressed {
        return;
    }
    if matches!(region.active, RegionId::Cave(_)) {
        region.active = RegionId::Overworld { gx: 0, gz: 0 };
        world_ctx.active = WorldContext::Overworld;
        let return_pos = region.cave_surface_return.take().unwrap_or(region.cave_portal);
        let return_tx = return_pos.x;
        let return_tz = return_pos.z;
        let feet = ha_shared::ground_height_for_locomotion(return_tx, return_tz, sim.body.radius);
        let placed = player_sphere_transform(
            CelestialBody::Earth,
            return_tx,
            return_tz,
            feet,
            sim.body.radius,
            sim.body.rot,
        );
        player_tf.translation = placed.translation;
        player_tf.rotation = placed.rotation;
        sim.body.x = return_tx;
        sim.body.z = return_tz;
        sim.body.y = feet;
        sim.body.vy = 0.0;
        return;
    }
    if matches!(region.active, RegionId::Luna) || matches!(region.active, RegionId::Space) {
        return;
    }
    let near_portal = {
        let dx = sim.body.x - region.cave_portal.x;
        let dz = sim.body.z - region.cave_portal.z;
        (dx * dx + dz * dz).sqrt() <= 2.8
    };
    if near_portal {
        region.cave_surface_return = Some(Vec3::new(sim.body.x, sim.body.y, sim.body.z));
        region.active = RegionId::Cave(1);
        world_ctx.active = WorldContext::Mission;
        player_tf.translation = CAVE_INTERIOR_ORIGIN + Vec3::new(0.0, 1.1, 0.0);
        player_tf.rotation = Quat::IDENTITY;
        sim.body.x = CAVE_INTERIOR_ORIGIN.x;
        sim.body.y = CAVE_INTERIOR_ORIGIN.y + 1.1;
        sim.body.z = CAVE_INTERIOR_ORIGIN.z;
        sim.body.vy = 0.0;
    }
}

fn update_cave_visual_overrides(
    region: Res<RegionRuntime>,
    mut player_torch: ParamSet<(
        Query<&Transform, With<Player>>,
        Query<(&mut PointLight, &mut Transform), With<CaveTorch>>,
    )>,
    mut sun: Query<&mut DirectionalLight>,
    mut crystals: Query<&mut Visibility, With<CaveCrystal>>,
) {
    let in_cave = matches!(region.active, RegionId::Cave(_));
    if let Ok(mut sun_light) = sun.get_single_mut() {
        sun_light.shadows_enabled = false;
    }
    let player_pos = player_torch
        .p0()
        .get_single()
        .ok()
        .map(|tf| tf.translation);
    if let Ok((mut light, mut tf)) = player_torch.p1().get_single_mut() {
        if in_cave {
            light.intensity = 55_000.0;
            light.range = 11.0;
            if let Some(pos) = player_pos {
                tf.translation = pos + Vec3::new(0.0, 2.2, 0.0);
            }
        } else {
            light.intensity = 0.0;
        }
    }
    // CRITICO: RECORDATORIO DE RENDIMIENTO EN MOVILES.
    // La "cueva" es cambio lógico de región + overrides de materiales procedurales,
    // sin texturas pesadas ni geometría subterránea compleja.
    for mut visibility in &mut crystals {
        *visibility = if in_cave {
            Visibility::Visible
        } else {
            Visibility::Hidden
        };
    }
}

/// Region visibility for Earth / moon / space shells.
fn force_initial_biome_visibility(mut cache: ResMut<RegionVisibilityCache>) {
    cache.last = None;
}

/// Overworld on the ground: visibility + camera handled in `sync_biome_visibility`.
fn guard_overworld_view(
    flight: Res<RocketFlight>,
    mut region: ResMut<RegionRuntime>,
    mut world_ctx: ResMut<WorldContextRuntime>,
    mut launch_atmo: ResMut<LaunchAtmosphere>,
    mut clear_color: ResMut<ClearColor>,
    mut cache: ResMut<RegionVisibilityCache>,
) {
    if !matches!(region.active, RegionId::Overworld { .. }) {
        return;
    }

    if world_ctx.active != WorldContext::Overworld {
        return;
    }

    let on_surface_earth = !flight.active
        && !matches!(region.active, RegionId::Cave(_));

    if !on_surface_earth {
        return;
    }

    let mut corrected = false;
    if matches!(
        region.active,
        RegionId::Space | RegionId::Luna | RegionId::Planet(_) | RegionId::Transit
    ) {
        region.active = RegionId::Overworld { gx: 0, gz: 0 };
        corrected = true;
        eprintln!("ha-guard: corrected stale region → Overworld");
    }
    if world_ctx.active != WorldContext::Overworld {
        world_ctx.active = WorldContext::Overworld;
        corrected = true;
        eprintln!("ha-guard: corrected stale world_ctx → Overworld");
    }
    if corrected {
        cache.last = None;
    }

    launch_atmo.active = false;
    launch_atmo.blend = 0.0;
    clear_color.0 = Color::srgb(
        Ps1Retro::SKY_ZENITH[0],
        Ps1Retro::SKY_ZENITH[1],
        Ps1Retro::SKY_ZENITH[2],
    );
}

fn sync_biome_visibility(
    region: Res<RegionRuntime>,
    world_ctx: Res<WorldContextRuntime>,
    flight: Res<RocketFlight>,
    mut launch_atmo: ResMut<LaunchAtmosphere>,
    mut cache: ResMut<RegionVisibilityCache>,
    mut game_cam: Query<&mut Camera, With<GameCamera>>,
    mut sets: ParamSet<(
        Query<
            &mut Visibility,
            (
                With<EarthDecor>,
                Without<EarthSurface>,
                Without<MoonDecor>,
                Without<SpaceDecor>,
                Without<crate::flight_celestials::SkyMoon>,
            ),
        >,
        Query<&mut Visibility, With<MoonDecor>>,
        Query<&mut Visibility, With<SpaceDecor>>,
        Query<&mut Visibility, With<CaveInterior>>,
        Query<&mut Visibility, With<TransitPlanet>>,
        Query<&mut Visibility, Or<(With<GhibliSkyDome>, With<OverworldStarLayer>)>>,
        Query<&mut Visibility, With<EarthSurface>>,
    )>,
) {
    let on_ground_overworld = matches!(region.active, RegionId::Overworld { .. })
        && world_ctx.active == WorldContext::Overworld
        && !flight.active;

    if cache.last != Some(region.active) {
        cache.last = Some(region.active);

        let on_luna = matches!(region.active, RegionId::Luna);
        let on_planet = matches!(region.active, RegionId::Planet(_));
        let on_space = matches!(region.active, RegionId::Space);
        let on_transit = matches!(region.active, RegionId::Transit);
        let in_cave = matches!(region.active, RegionId::Cave(_));
        let on_surface = on_luna || on_planet;
        let hide_earth = on_surface || on_space || on_transit || in_cave;
        let earth_vis = if hide_earth {
            Visibility::Hidden
        } else {
            Visibility::Visible
        };
        let moon_vis = if on_surface {
            Visibility::Visible
        } else {
            Visibility::Hidden
        };
        let space_vis = if on_space {
            Visibility::Visible
        } else {
            Visibility::Hidden
        };

        for mut v in sets.p0().iter_mut() {
            *v = earth_vis;
        }
        for mut v in sets.p6().iter_mut() {
            *v = earth_vis;
        }
        for mut v in sets.p1().iter_mut() {
            *v = moon_vis;
        }
        for mut v in sets.p2().iter_mut() {
            *v = space_vis;
        }
        let cave_vis = if in_cave {
            Visibility::Visible
        } else {
            Visibility::Hidden
        };
        for mut v in sets.p3().iter_mut() {
            *v = cave_vis;
        }
        let transit_vis = if on_transit {
            Visibility::Visible
        } else {
            Visibility::Hidden
        };
        for mut v in sets.p4().iter_mut() {
            *v = transit_vis;
        }

        let sky_vis = if on_space || on_transit || in_cave {
            Visibility::Hidden
        } else {
            Visibility::Visible
        };
        for mut v in sets.p5().iter_mut() {
            *v = sky_vis;
        }
    }

    if on_ground_overworld {
        launch_atmo.active = false;
        launch_atmo.blend = 0.0;
        for mut vis in sets.p0().iter_mut() {
            *vis = Visibility::Visible;
        }
        for mut vis in sets.p6().iter_mut() {
            *vis = Visibility::Visible;
        }
        for mut vis in sets.p5().iter_mut() {
            *vis = Visibility::Visible;
        }
        if let Ok(mut cam) = game_cam.get_single_mut() {
            cam.is_active = true;
        }
    }

    // Rocket cruise — per-frame overrides (avoid duplicate Visibility queries elsewhere).
    if flight.active {
        let vacuum = flight.in_vacuum();
        for mut vis in sets.p6().iter_mut() {
            *vis = if vacuum {
                Visibility::Hidden
            } else {
                Visibility::Visible
            };
        }
        for mut vis in sets.p5().iter_mut() {
            *vis = if vacuum {
                Visibility::Hidden
            } else {
                Visibility::Visible
            };
        }
        for mut vis in sets.p2().iter_mut() {
            *vis = Visibility::Hidden;
        }
    }
}

#[derive(Resource, Default)]
struct StarfieldAnimState {
    last_region: Option<RegionId>,
    frame: u8,
}

fn tick_starfield_time(
    time: Res<Time>,
    region: Res<RegionRuntime>,
    mut anim: ResMut<StarfieldAnimState>,
    mut materials: ResMut<Assets<crate::materials::StarfieldMaterial>>,
    stars: Query<
        &MeshMaterial3d<crate::materials::StarfieldMaterial>,
        Without<OverworldStarLayer>,
    >,
) {
    if !matches!(region.active, RegionId::Luna)
        && !matches!(region.active, RegionId::Planet(_))
        && !matches!(region.active, RegionId::Space)
        && !matches!(region.active, RegionId::Transit)
    {
        anim.last_region = None;
        return;
    }
    anim.frame = anim.frame.wrapping_add(1);
    if anim.frame & 1 != 0 {
        return;
    }
    let t = time.elapsed_secs();
    for mat in &stars {
        if let Some(m) = materials.get_mut(&mat.0) {
            m.settings.time = t;
            m.settings.night = 1.0;
        }
    }
}
