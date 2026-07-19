//! 2D sky — day/night clear color, night overlay, starfield in vacuum.
//!
//! Replaces the Ghibli dome + fog + starfield shells: on Earth the clear
//! color runs a 720 s day cycle; airless bodies and space render near-black
//! with camera-following star sprites (never a crossable shell — biblia §1).

use bevy::prelude::*;

use crate::camera::GameCamera;
use crate::scene::{RegionId, RegionRuntime};
use crate::sim::{Player, SimState};
use crate::transition::FlightTransition;

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

/// Camera-following cover layer for every ascent/descent (rocket AND creative)
/// — hides the ground (and the surface↔orbit swap) mid-atmosphere, clears into
/// space. Alpha + tint are driven by `FlightTransition` (clouds on bodies with
/// an atmosphere, a brief dark fade on airless ones). Drop a `decor/clouds`
/// texture later for real clouds.
#[derive(Component)]
struct CloudOverlay;

pub struct Sky2dPlugin;

impl Plugin for Sky2dPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<DayClock>()
            .init_resource::<PlayerLanternControl>()
            .add_systems(Startup, (spawn_sky_attachments, spawn_cloud_overlay))
            .add_systems(Update, (advance_day_clock, sync_sky, sync_cloud_overlay));
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
    trans: Res<FlightTransition>,
    lantern: Res<PlayerLanternControl>,
    tiles: Res<crate::world2d::TileWorld>,
    mut clear: ResMut<ClearColor>,
    player: Query<&SimState, With<Player>>,
    camera: Query<&OrthographicProjection, With<GameCamera>>,
    mut overlay: Query<(&mut Sprite, &mut Transform), (With<NightOverlay>, Without<StarSprite>)>,
    mut stars: Query<
        (&mut Visibility, &mut Transform),
        (With<StarSprite>, Without<NightOverlay>),
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

    // How "spacey" is the view? 0 = surface sky by day, 1 = void. Bodies with
    // an atmosphere get their own tinted sky (from the transition profile);
    // airless surfaces are void + stars, as always.
    let mut space_f: f32;
    let mut body_sky: Option<Vec3> = None;
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
            if trans.in_surface_frame() {
                space_f = space_f.max(trans.sky_blend.clamp(0.0, 1.0));
            }
        }
        RegionId::Luna | RegionId::Planet(_) => {
            let body = match region.active {
                RegionId::Planet(b) => b,
                _ => ha_shared::CelestialBody::Moon,
            };
            let p = ha_shared::transition_profile(body);
            if p.sky_fade_end > 0.0 {
                // Same on-foot fade band as Earth, scaled to this atmosphere,
                // so the sky you land under matches the descent you flew.
                let start = p.sky_fade_end * (SKY_FADE_START_M / 900.0);
                let end = p.sky_fade_end * (SKY_FADE_END_M / 900.0);
                space_f = player
                    .get_single()
                    .map(|sim| ((sim.body.y - start) / (end - start)).clamp(0.0, 1.0))
                    .unwrap_or(0.0);
                if trans.in_surface_frame() {
                    space_f = space_f.max(trans.sky_blend.clamp(0.0, 1.0));
                }
                body_sky = Some(Vec3::from_array(p.cover_color) * 0.85);
            } else {
                space_f = 1.0;
            }
        }
        _ => space_f = 1.0,
    }

    // Sky palette: Earth runs its day/night cycle; other atmospheres use the
    // body's haze tint (no local day cycle yet).
    let day_sky = Vec3::new(0.45, 0.68, 0.85);
    let dusk_sky = Vec3::new(0.13, 0.12, 0.22);
    let sky = body_sky.unwrap_or_else(|| dusk_sky.lerp(day_sky, day));
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
}

/// Full-screen UI overlay for the ascent/descent cover. A UI Node (renders
/// reliably on top, unlike the old camera-child sprite which never drew).
fn spawn_cloud_overlay(mut commands: Commands) {
    commands.spawn((
        CloudOverlay,
        Node {
            position_type: PositionType::Absolute,
            left: Val::Px(0.0),
            top: Val::Px(0.0),
            width: Val::Percent(100.0),
            height: Val::Percent(100.0),
            ..default()
        },
        BackgroundColor(Color::srgba(0.86, 0.89, 0.95, 0.0)),
        // Negative z: above the 2D world (UI pass always is) but BELOW the HUD so
        // the clouds cover the ground, never the hearts/hotbar/buttons.
        GlobalZIndex(-50),
        bevy::ui::FocusPolicy::Pass,
        Name::new("CloudOverlay"),
    ));
}

fn sync_cloud_overlay(
    trans: Res<FlightTransition>,
    mut q: Query<&mut BackgroundColor, With<CloudOverlay>>,
) {
    let [r, g, b] = trans.cover_color;
    for mut bg in &mut q {
        *bg = BackgroundColor(Color::srgba(r, g, b, trans.cover.clamp(0.0, 1.0)));
    }
}
