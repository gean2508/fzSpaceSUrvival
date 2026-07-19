//! 2D sky — day/night clear color, night overlay, starfield in vacuum.
//!
//! Replaces the Ghibli dome + fog + starfield shells: on Earth the clear
//! color runs a 720 s day cycle; airless bodies and space render near-black
//! with camera-following star sprites (never a crossable shell — biblia §1).

use bevy::prelude::*;

use crate::camera::GameCamera;
use crate::scene::{LaunchAtmosphere, RegionId, RegionRuntime};
use crate::sim::{Player, SimState};

/// Full day length in seconds (matches the old sky_clock cycle).
pub const DAY_SECS: f32 = 720.0;

/// Creative ascent fade band (biblia §11): sky darkens into space.
const SKY_FADE_START_M: f32 = 150.0;
const SKY_FADE_END_M: f32 = 1200.0;

#[derive(Resource)]
pub struct DayClock {
    pub t: f32,
}

impl Default for DayClock {
    fn default() -> Self {
        // Start mid-morning.
        Self { t: DAY_SECS * 0.28 }
    }
}

impl DayClock {
    /// 0..1 phase of the day (0 = midnight).
    pub fn phase(&self) -> f32 {
        (self.t / DAY_SECS).fract()
    }

    /// 0 = full night, 1 = full day.
    pub fn daylight(&self) -> f32 {
        // Day between 0.2 and 0.8 phase with smooth dawn/dusk ramps.
        let p = self.phase();
        let up = ((p - 0.17) / 0.1).clamp(0.0, 1.0);
        let down = ((0.83 - p) / 0.1).clamp(0.0, 1.0);
        up.min(down)
    }
}

/// Kept for HUD compat: lantern state (crafted light) — dims the night overlay.
#[derive(Resource, Default)]
pub struct PlayerLanternControl {
    pub equipped: bool,
    pub lit: bool,
}

#[derive(Component)]
struct NightOverlay;

#[derive(Component)]
struct StarSprite;

/// Camera-following cloud layer for the creative ascent/descent — hides the
/// ground (and the surface↔orbit swap) mid-atmosphere, clears into space.
/// Alpha is driven by `CreativeFlight::cloud`. Colour = white fallback; drop a
/// `decor/clouds` texture later for real clouds.
#[derive(Component)]
struct CloudOverlay;

pub struct Sky2dPlugin;

impl Plugin for Sky2dPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<DayClock>()
            .init_resource::<PlayerLanternControl>()
            .add_systems(Startup, spawn_sky_attachments)
            .add_systems(Update, (advance_day_clock, sync_sky));
    }
}

fn advance_day_clock(time: Res<Time>, mut clock: ResMut<DayClock>) {
    clock.t += time.delta_secs();
}

fn spawn_sky_attachments(mut commands: Commands, camera: Query<Entity, With<GameCamera>>) {
    let Ok(cam) = camera.get_single() else {
        return;
    };
    commands.entity(cam).with_children(|p| {
        // Night dimmer in front of the world, behind UI.
        p.spawn((
            NightOverlay,
            Sprite::from_color(Color::srgba(0.02, 0.02, 0.08, 0.0), Vec2::new(4.0, 2.4)),
            Transform::from_xyz(0.0, 0.0, 40.0),
        ));
        // Cloud layer for the creative ascent (in front of night, behind UI).
        p.spawn((
            CloudOverlay,
            Sprite::from_color(Color::srgba(0.86, 0.89, 0.95, 0.0), Vec2::new(4.0, 2.4)),
            Transform::from_xyz(0.0, 0.0, 45.0),
        ));
        // Static star pattern that follows the camera (skybox, never crossed).
        let mut seed = 0x1357_9bdfu32;
        for _ in 0..90 {
            seed ^= seed << 13;
            seed ^= seed >> 17;
            seed ^= seed << 5;
            let x = ((seed & 0xffff) as f32 / 65535.0 - 0.5) * 2.2;
            let y = (((seed >> 16) & 0xffff) as f32 / 65535.0 - 0.5) * 1.4;
            let s = 0.004 + (seed % 5) as f32 * 0.0015;
            p.spawn((
                StarSprite,
                Sprite::from_color(Color::srgba(0.95, 0.95, 1.0, 0.9), Vec2::splat(s)),
                Transform::from_xyz(x, y, 30.0),
                Visibility::Hidden,
            ));
        }
    });
}

#[allow(clippy::too_many_arguments, clippy::type_complexity)]
fn sync_sky(
    clock: Res<DayClock>,
    region: Res<RegionRuntime>,
    launch: Res<LaunchAtmosphere>,
    lantern: Res<PlayerLanternControl>,
    tiles: Res<crate::world2d::TileWorld>,
    fly: Res<crate::creative_fly::CreativeFlight>,
    mut clear: ResMut<ClearColor>,
    player: Query<&SimState, With<Player>>,
    camera: Query<&OrthographicProjection, With<GameCamera>>,
    mut overlay: Query<
        (&mut Sprite, &mut Transform),
        (With<NightOverlay>, Without<StarSprite>, Without<CloudOverlay>),
    >,
    mut stars: Query<
        (&mut Visibility, &mut Transform),
        (With<StarSprite>, Without<NightOverlay>, Without<CloudOverlay>),
    >,
    mut clouds: Query<
        (&mut Sprite, &mut Transform),
        (With<CloudOverlay>, Without<NightOverlay>, Without<StarSprite>),
    >,
) {
    let view_h = camera
        .get_single()
        .ok()
        .map(|p| match p.scaling_mode {
            bevy::render::camera::ScalingMode::FixedVertical { viewport_height } => viewport_height,
            _ => 30.0,
        })
        .unwrap_or(30.0);

    // How "spacey" is the view? 0 = Earth ground by day, 1 = void.
    let mut space_f: f32;
    let day = clock.daylight();
    match region.active {
        RegionId::Overworld { .. } | RegionId::Cave(_) => {
            space_f = 0.0;
            // Walking/flying high on Earth fades the atmosphere out (§11).
            if let Ok(sim) = player.get_single() {
                let h = sim.body.y;
                space_f = ((h - SKY_FADE_START_M) / (SKY_FADE_END_M - SKY_FADE_START_M))
                    .clamp(0.0, 1.0);
            }
            if launch.active {
                space_f = space_f.max(launch.blend.clamp(0.0, 1.0));
            }
        }
        _ => space_f = 1.0,
    }

    // Earth sky palette by daylight.
    let day_sky = Vec3::new(0.45, 0.68, 0.85);
    let dusk_sky = Vec3::new(0.13, 0.12, 0.22);
    let sky = dusk_sky.lerp(day_sky, day);
    let void = Vec3::new(0.01, 0.01, 0.03);
    let rgb = sky.lerp(void, space_f);
    *clear = ClearColor(Color::srgb(rgb.x, rgb.y, rgb.z));

    // Underground (Z-Level < 0): near-black cave, lantern cuts the dark.
    let underground = tiles.z < 0;
    if underground {
        let cave = Vec3::new(0.03, 0.03, 0.05);
        *clear = ClearColor(Color::srgb(cave.x, cave.y, cave.z));
    }

    // Night dimmer over the world (skipped in void where it's already dark).
    let mut night_alpha = (1.0 - day) * 0.55 * (1.0 - space_f);
    if underground {
        night_alpha = 0.82;
    }
    if lantern.lit {
        night_alpha *= 0.45;
    }
    for (mut sprite, mut tf) in &mut overlay {
        sprite.color = Color::srgba(0.02, 0.02, 0.08, night_alpha);
        // Cover the viewport at any zoom.
        tf.scale = Vec3::splat(view_h * 1.6);
    }

    // Stars visible in the void / at night high up.
    let stars_on = space_f > 0.55 || (day < 0.25 && space_f > 0.2);
    for (mut vis, mut tf) in &mut stars {
        *vis = if stars_on {
            Visibility::Visible
        } else {
            Visibility::Hidden
        };
        tf.scale = Vec3::splat(view_h);
    }

    // Creative-ascent clouds — hide the ground + the frame swap mid-atmosphere.
    for (mut sprite, mut tf) in &mut clouds {
        sprite.color = Color::srgba(0.86, 0.89, 0.95, fly.cloud.clamp(0.0, 1.0));
        tf.scale = Vec3::splat(view_h * 1.7);
    }
}
