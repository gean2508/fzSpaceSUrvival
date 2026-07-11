//! Day/night sky clock — painted procedural dome + parallax star layers.

use bevy::prelude::*;

use crate::materials::{GhibliSkyMaterial, StarfieldMaterial, StarfieldSettings};
use crate::quality::{sky_shader_quality, star_shell_layers};
use crate::scene::{GhibliSkyDome, RegionId, RegionRuntime};
use crate::sim::Player;

/// Legacy weather hook — meadow rain removed; neutral defaults for sky tint.
#[derive(Resource, Default, Clone)]
pub struct WeatherState {
    pub sample: WeatherSample,
}

#[derive(Clone, Copy, Default)]
pub struct WeatherSample {
    pub cloud_cover: f32,
    pub storm: f32,
    pub wind: f32,
    pub wind_angle: f32,
    pub sun_dim: f32,
    pub rain: f32,
    pub moon_phase: f32,
}

/// Real-time seconds for a full in-game day (12 min default).
pub const DAY_CYCLE_SECS: f32 = 720.0;

const SKY_DOME_RADIUS: f32 = 420.0;

#[derive(Clone, Copy, Debug)]
pub struct SkyStageDef {
    pub id: &'static str,
    pub zenith: [f32; 3],
    pub horizon: [f32; 3],
    pub warm: [f32; 3],
    pub sun_elevation: f32,
    pub night: f32,
    pub neon_band: f32,
}

/// Stages around the 0..1 day ring (Ghibli + synthwave dusk).
pub const SKY_STAGES: &[SkyStageDef] = &[
    SkyStageDef {
        id: "deep_night",
        zenith: [0.08, 0.06, 0.14],
        horizon: [0.14, 0.12, 0.22],
        warm: [0.18, 0.14, 0.28],
        sun_elevation: -0.35,
        night: 1.0,
        neon_band: 0.15,
    },
    SkyStageDef {
        id: "dawn",
        zenith: [0.32, 0.28, 0.48],
        horizon: [0.96, 0.66, 0.78],
        warm: [1.0, 0.72, 0.55],
        sun_elevation: 0.05,
        night: 0.55,
        neon_band: 0.2,
    },
    SkyStageDef {
        id: "morning",
        zenith: [0.43, 0.72, 0.91],
        horizon: [0.96, 0.78, 0.91],
        warm: [1.0, 0.86, 0.72],
        sun_elevation: 0.35,
        night: 0.12,
        neon_band: 0.05,
    },
    SkyStageDef {
        id: "noon",
        zenith: [0.53, 0.78, 0.94],
        horizon: [0.91, 0.94, 1.0],
        warm: [1.0, 0.95, 0.82],
        sun_elevation: 0.72,
        night: 0.0,
        neon_band: 0.0,
    },
    SkyStageDef {
        id: "afternoon",
        zenith: [0.42, 0.60, 0.78],
        horizon: [0.94, 0.72, 0.82],
        warm: [1.0, 0.78, 0.62],
        sun_elevation: 0.42,
        night: 0.08,
        neon_band: 0.08,
    },
    SkyStageDef {
        id: "dusk",
        zenith: [0.29, 0.18, 0.43],
        horizon: [0.96, 0.55, 0.78],
        warm: [1.0, 0.42, 0.55],
        sun_elevation: 0.08,
        night: 0.22,
        neon_band: 0.85,
    },
    SkyStageDef {
        id: "twilight",
        zenith: [0.22, 0.16, 0.34],
        horizon: [0.78, 0.40, 0.58],
        warm: [0.95, 0.45, 0.62],
        sun_elevation: -0.08,
        night: 0.65,
        neon_band: 0.45,
    },
    SkyStageDef {
        id: "night",
        zenith: [0.10, 0.12, 0.20],
        horizon: [0.18, 0.20, 0.32],
        warm: [0.22, 0.18, 0.30],
        sun_elevation: -0.42,
        night: 0.92,
        neon_band: 0.22,
    },
];

#[derive(Clone, Copy, Debug)]
pub struct SkySample {
    pub zenith: Vec3,
    pub horizon: Vec3,
    pub warm: Vec3,
    pub sun_dir: Vec3,
    pub night: f32,
    pub neon_band: f32,
    pub sun_visibility: f32,
    pub stage_a: usize,
    pub stage_b: usize,
    pub blend: f32,
}

#[derive(Resource)]
pub struct GameSkyClock {
    /// 0 = midnight, 0.25 = sunrise, 0.5 = noon, 0.75 = sunset.
    pub day_fraction: f32,
    pub paused: bool,
}

impl Default for GameSkyClock {
    fn default() -> Self {
        Self {
            day_fraction: 0.44,
            paused: false,
        }
    }
}

/// Parallax offset factor — farther layers drift more with player motion.
#[derive(Component, Clone, Copy)]
pub struct ParallaxSkyLayer {
    pub factor: f32,
}

/// Overworld star shell (distinct from orbital `StarfieldSphere`).
#[derive(Component)]
pub struct OverworldStarLayer;

pub struct SkyClockPlugin;

impl Plugin for SkyClockPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<GameSkyClock>()
            .init_resource::<WeatherState>()
            .add_systems(PostStartup, spawn_sky_stack_once)
            .add_systems(
                Update,
                (
                    tick_sky_clock,
                    crate::solar_lighting::sync_solar_lighting.after(tick_sky_clock),
                    sync_parallax_sky.after(crate::solar_lighting::sync_solar_lighting),
                    sync_sky_projection.after(sync_parallax_sky),
                ),
            );
    }
}

fn spawn_sky_stack_once(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut ghibli_mats: ResMut<Assets<GhibliSkyMaterial>>,
    mut star_mats: ResMut<Assets<StarfieldMaterial>>,
    existing: Query<Entity, With<GhibliSkyDome>>,
) {
    if !existing.is_empty() {
        return;
    }
    spawn_sky_stack(&mut commands, &mut meshes, &mut ghibli_mats, &mut star_mats);
}

fn sync_parallax_sky(
    player: Query<&Transform, (With<Player>, Without<ParallaxSkyLayer>)>,
    mut layers: Query<(&mut Transform, &ParallaxSkyLayer), Without<Player>>,
) {
    let Ok(player_tf) = player.get_single() else {
        return;
    };
    let p = player_tf.translation;
    for (mut tf, layer) in &mut layers {
        tf.translation = p * layer.factor;
    }
}

/// Creative-ascent sky fade band (metres above Earth's surface): below START
/// the sky is normal; by END it reads as full space. Tune on device.
const CREATIVE_SKY_FADE_START_M: f32 = 150.0;
const CREATIVE_SKY_FADE_END_M: f32 = 1200.0;

fn sync_sky_projection(
    time: Res<Time>,
    clock: Res<GameSkyClock>,
    region: Res<RegionRuntime>,
    launch_atmo: Res<crate::scene::LaunchAtmosphere>,
    editor: Res<crate::ui_editor::UiEditorState>,
    weather: Option<Res<WeatherState>>,
    solar: Option<Res<crate::solar_lighting::SolarLightingState>>,
    mut clear: ResMut<ClearColor>,
    mut ghibli_mats: ResMut<Assets<GhibliSkyMaterial>>,
    mut star_mats: ResMut<Assets<StarfieldMaterial>>,
    sky_mats: Query<&MeshMaterial3d<GhibliSkyMaterial>, With<GhibliSkyDome>>,
    star_handles: Query<&MeshMaterial3d<StarfieldMaterial>, With<OverworldStarLayer>>,
    player: Query<&crate::sim::SimState, With<Player>>,
) {
    // Airless/thin-atmosphere bodies keep a dark starry dome regardless of day clock.
    let dark_sky_body = matches!(region.active, RegionId::Luna | RegionId::Planet(_));
    let mut sample = sample_sky(clock.day_fraction);
    if dark_sky_body {
        sample.night = 0.96;
        sample.neon_band = 0.08;
        sample.sun_visibility = 0.12;
        sample.zenith = Vec3::new(0.04, 0.05, 0.09);
        sample.horizon = Vec3::new(0.08, 0.09, 0.14);
    }

    // Creative straight-up flight is the only way to fly today (no buildable
    // rocket yet), and it must reach space CONTINUOUSLY — no region flip, no
    // teleport, no forced transition. Reuse the day/night machinery: gaining
    // altitude on Earth ramps an extra "night" toward 1 and pulls the painted
    // gradient toward the airless dark-sky values, so the dome (which fully
    // encloses the camera → the clear colour is never seen, so no guard fight)
    // darkens and the stars fade in. Airless bodies already force dark, and the
    // rocket launch owns its own atmosphere (`launch_atmo`), so exclude both.
    if editor.creative
        && !dark_sky_body
        && matches!(region.active, RegionId::Overworld { .. })
        && !launch_atmo.active
    {
        if let Ok(sim) = player.get_single() {
            let ground = ha_shared::planet_ground_y_at(
                ha_shared::CelestialBody::Earth,
                sim.body.x,
                sim.body.z,
            );
            let alt = sim.body.y - ground;
            let f = ((alt - CREATIVE_SKY_FADE_START_M)
                / (CREATIVE_SKY_FADE_END_M - CREATIVE_SKY_FADE_START_M))
                .clamp(0.0, 1.0);
            if f > 0.0 {
                sample.night = sample.night.max(f);
                sample.zenith = sample.zenith.lerp(Vec3::new(0.04, 0.05, 0.09), f);
                sample.horizon = sample.horizon.lerp(Vec3::new(0.08, 0.09, 0.14), f);
            }
        }
    }
    let w = if dark_sky_body {
        WeatherSample::default()
    } else {
        weather.map(|w| w.sample).unwrap_or_default()
    };
    let mut zenith = sample.zenith;
    let mut horizon = sample.horizon;
    zenith *= 1.0 - w.sun_dim * 0.35;
    horizon *= 1.0 - w.sun_dim * 0.25;
    // ClearColor ownership: overworld surface belongs to the day/night clock;
    // launch/space/luna/planet/cave belong to `transition_zone_atmosphere`.
    if matches!(region.active, RegionId::Overworld { .. }) && !launch_atmo.active {
        clear.0 = Color::srgb(zenith.x, zenith.y, zenith.z);
    }

    let paint = Vec4::new(
        sample.night,
        sample.neon_band * (1.0 - w.storm * 0.8),
        sample.sun_visibility * (1.0 - w.sun_dim),
        time.elapsed_secs(),
    );
    let weather_v4 = Vec4::new(w.cloud_cover, w.rain, w.moon_phase, w.storm);
    let sun_dir = solar
        .as_ref()
        .map(|s| Vec3::from_array(s.sample.lit_to_sun))
        .unwrap_or(sample.sun_dir);
    let sun_v4 = Vec4::new(
        sun_dir.x,
        sun_dir.y,
        sun_dir.z,
        sky_shader_quality(),
    );
    let zenith_v4 = Vec4::new(zenith.x, zenith.y, zenith.z, 0.0);
    let horizon_v4 = Vec4::new(horizon.x, horizon.y, horizon.z, 0.0);
    let warm_v4 = Vec4::new(sample.warm.x, sample.warm.y, sample.warm.z, 0.0);

    for mat_handle in &sky_mats {
        if let Some(mat) = ghibli_mats.get_mut(&mat_handle.0) {
            mat.zenith = zenith_v4;
            mat.horizon = horizon_v4;
            mat.warm = warm_v4;
            mat.paint = paint;
            mat.weather = weather_v4;
            mat.sun_dir = sun_v4;
        }
    }

    let star_night = (sample.night * 1.15).clamp(0.0, 1.0);
    for mat_handle in &star_handles {
        if let Some(mat) = star_mats.get_mut(&mat_handle.0) {
            mat.settings.time = time.elapsed_secs();
            mat.settings.night = star_night;
        }
    }
}

fn tick_sky_clock(time: Res<Time>, mut clock: ResMut<GameSkyClock>) {
    if clock.paused {
        return;
    }
    clock.day_fraction =
        (clock.day_fraction + time.delta_secs() / DAY_CYCLE_SECS) % 1.0;
}

pub fn sample_sky(day_fraction: f32) -> SkySample {
    let n = SKY_STAGES.len();
    let t = (day_fraction.fract() + 1.0).fract();
    let pos = t * n as f32;
    let a = pos.floor() as usize % n;
    let b = (a + 1) % n;
    let blend = pos.fract();
    let sa = &SKY_STAGES[a];
    let sb = &SKY_STAGES[b];
    let lerp3 = |x: [f32; 3], y: [f32; 3]| {
        Vec3::new(
            x[0] + (y[0] - x[0]) * blend,
            x[1] + (y[1] - x[1]) * blend,
            x[2] + (y[2] - x[2]) * blend,
        )
    };
    let elev = sa.sun_elevation + (sb.sun_elevation - sa.sun_elevation) * blend;
    let yaw = t * std::f32::consts::TAU + 0.55;
    let sun_dir = Vec3::new(
        yaw.cos() * elev.abs().max(0.05),
        elev,
        yaw.sin() * elev.abs().max(0.05),
    )
    .normalize_or_zero();
    let sun_visibility = (elev * 4.0 + 0.15).clamp(0.0, 1.0);
    SkySample {
        zenith: lerp3(sa.zenith, sb.zenith),
        horizon: lerp3(sa.horizon, sb.horizon),
        warm: lerp3(sa.warm, sb.warm),
        sun_dir: if sun_dir.length_squared() > 1e-6 {
            sun_dir
        } else {
            Vec3::new(0.35, 0.88, 0.32)
        },
        night: sa.night + (sb.night - sa.night) * blend,
        neon_band: sa.neon_band + (sb.neon_band - sa.neon_band) * blend,
        sun_visibility,
        stage_a: a,
        stage_b: b,
        blend,
    }
}

/// Interior sky — procedural Ghibli dome + two parallax star shells.
pub fn spawn_sky_stack(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    ghibli_mats: &mut Assets<GhibliSkyMaterial>,
    star_mats: &mut Assets<StarfieldMaterial>,
) {
    let invert = Vec3::new(-1.0, 1.0, -1.0);

    for &(radius, factor, density) in star_shell_layers() {
        let mut star = crate::materials::starfield_material();
        star.settings.density = density;
        star.settings.night = 0.0;
        commands.spawn((
            Mesh3d(meshes.add(Sphere::new(radius))),
            MeshMaterial3d(star_mats.add(star)),
            Transform::from_scale(invert),
            ParallaxSkyLayer { factor },
            OverworldStarLayer,
            Name::new("OverworldStars"),
        ));
    }

    let sky_mat = ghibli_mats.add(GhibliSkyMaterial::default());
    commands.spawn((
        Mesh3d(meshes.add(Sphere::new(SKY_DOME_RADIUS))),
        MeshMaterial3d(sky_mat),
        Transform::from_scale(invert),
        GhibliSkyDome,
        Name::new("SkyDome"),
    ));
}
