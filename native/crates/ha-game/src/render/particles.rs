//! 2D harvest particles — tiny colored sprites with a kick, plus impact
//! squash on harvestable sprites. Same public API as the old 3D pool.

use bevy::prelude::*;

use crate::camera::CameraShakeEvent;
use crate::input::TouchActions;
use crate::sim::Player;

#[derive(Component)]
pub struct ProceduralParticle {
    pub velocity: Vec3,
    pub lifetime: Timer,
    pub scale_decay: f32,
}

#[derive(Component)]
pub struct HarvestReactive;

#[derive(Component)]
struct SquashStretch {
    timer: Timer,
    base_scale: Vec3,
}

#[derive(Resource, Default)]
struct ActionEdge {
    was_pressed: bool,
}

/// Kept for API compat: particle tint pool.
#[derive(Resource, Default)]
pub struct HarvestParticlePool {
    pub colors: [Color; 5],
}

pub struct ParticlesPlugin;

impl Plugin for ParticlesPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<ActionEdge>()
            .insert_resource(HarvestParticlePool {
                colors: [
                    Color::srgb(0.78, 0.62, 0.36),
                    Color::srgb(0.55, 0.55, 0.58),
                    Color::srgb(0.45, 0.62, 0.34),
                    Color::srgb(0.68, 0.5, 0.3),
                    Color::srgb(0.6, 0.6, 0.66),
                ],
            })
            .add_systems(Update, (tick_particles, trigger_impact_squash, tick_squash));
    }
}

pub fn spawn_harvest_particles(
    commands: &mut Commands,
    pool: &HarvestParticlePool,
    position: Vec3,
    seed_offset: usize,
) {
    const OFFSETS: [Vec2; 5] = [
        Vec2::new(-0.48, 0.21),
        Vec2::new(0.42, -0.08),
        Vec2::new(0.1, 0.46),
        Vec2::new(-0.2, -0.4),
        Vec2::new(0.28, 0.34),
    ];
    const KICKS: [Vec2; 5] = [
        Vec2::new(-0.82, 2.6),
        Vec2::new(0.74, 2.9),
        Vec2::new(0.3, 3.1),
        Vec2::new(-0.55, 2.4),
        Vec2::new(0.62, 2.7),
    ];
    for i in 0..5 {
        let idx = (i + seed_offset) % 5;
        commands.spawn((
            Sprite::from_color(pool.colors[idx], Vec2::splat(0.28)),
            Transform::from_translation(
                (position.truncate() + OFFSETS[idx] * 0.55).extend(5.0),
            ),
            ProceduralParticle {
                velocity: KICKS[idx].extend(0.0),
                lifetime: Timer::from_seconds(0.42, TimerMode::Once),
                scale_decay: 2.7,
            },
            Name::new("HarvestParticle"),
        ));
    }
}

fn tick_particles(
    time: Res<Time>,
    mut commands: Commands,
    mut q: Query<(Entity, &mut Transform, &mut Sprite, &mut ProceduralParticle)>,
) {
    let dt = time.delta_secs();
    for (e, mut tf, mut sprite, mut p) in &mut q {
        p.lifetime.tick(time.delta());
        if p.lifetime.finished() {
            commands.entity(e).despawn_recursive();
            continue;
        }
        p.velocity.y -= 9.0 * dt;
        tf.translation += p.velocity * dt;
        let s = (tf.scale.x - p.scale_decay * dt * 0.4).max(0.05);
        tf.scale = Vec3::splat(s);
        // Y se APAGA, no desaparece de golpe. Encogerse solo alcanzaba para una
        // astilla, pero el polvo de `vault.rs` es una nube: sin el fundido, la
        // ráfaga al caer termina en un corte seco que se lee como un glitch.
        // Al cuadrado, para que la mota viva casi entera y se vaya rápido al final.
        let k = 1.0 - p.lifetime.fraction();
        sprite.color.set_alpha(k * k);
    }
}

/// Nudge the closest harvestable sprite when the action button lands a hit.
fn trigger_impact_squash(
    actions: Res<TouchActions>,
    mut edge: ResMut<ActionEdge>,
    mut shake: EventWriter<CameraShakeEvent>,
    mut commands: Commands,
    player: Query<&Transform, With<Player>>,
    targets: Query<(Entity, &Transform, Option<&SquashStretch>), With<HarvestReactive>>,
) {
    let pressed = actions.action;
    let rising = pressed && !edge.was_pressed;
    edge.was_pressed = pressed;
    if !rising {
        return;
    }
    let Ok(p) = player.get_single() else {
        return;
    };
    let mut best: Option<(Entity, Vec3, f32)> = None;
    for (e, tf, squash) in &targets {
        if squash.is_some() {
            continue;
        }
        let d = tf.translation.truncate().distance(p.translation.truncate());
        if d < 3.2 && best.map(|(_, _, bd)| d < bd).unwrap_or(true) {
            best = Some((e, tf.scale, d));
        }
    }
    if let Some((e, base_scale, _)) = best {
        commands.entity(e).insert(SquashStretch {
            timer: Timer::from_seconds(0.22, TimerMode::Once),
            base_scale,
        });
        shake.send(CameraShakeEvent { trauma: 0.12 });
    }
}

fn tick_squash(
    time: Res<Time>,
    mut commands: Commands,
    mut q: Query<(Entity, &mut Transform, &mut SquashStretch)>,
) {
    for (e, mut tf, mut s) in &mut q {
        s.timer.tick(time.delta());
        let t = (s.timer.elapsed_secs() / 0.22).clamp(0.0, 1.0);
        let wobble = 1.0 + (1.0 - t) * 0.18 * (t * 14.0).sin();
        tf.scale = s.base_scale * Vec3::new(wobble, 2.0 - wobble, 1.0);
        if s.timer.finished() {
            tf.scale = s.base_scale;
            commands.entity(e).remove::<SquashStretch>();
        }
    }
}
