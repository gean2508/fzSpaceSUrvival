use bevy::prelude::*;

use crate::camera::CameraShakeEvent;
use crate::debug::{span_guard, CpuSpanSlot, CpuSpanStats};
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
    duration: f32,
    strength: f32,
    base_scale: Vec3,
}

#[derive(Resource, Default)]
struct ActionEdge {
    was_pressed: bool,
}

#[derive(Resource)]
pub struct HarvestParticlePool {
    pub mesh: Handle<Mesh>,
    pub materials: [Handle<StandardMaterial>; 5],
}

pub struct ParticlesPlugin;

impl Plugin for ParticlesPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<ActionEdge>()
            .add_systems(Startup, init_harvest_particle_pool)
            .add_systems(
                Update,
                (
                    trigger_impact_squash,
                    tick_particles,
                    tick_squash_stretch,
                ),
            );
    }
}

fn init_harvest_particle_pool(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    const COLORS: [Vec4; 5] = [
        Vec4::new(0.62, 1.7, 1.6, 1.0),
        Vec4::new(2.1, 0.75, 1.65, 1.0),
        Vec4::new(1.8, 1.2, 0.45, 1.0),
        Vec4::new(0.6, 2.2, 1.0, 1.0),
        Vec4::new(2.2, 1.0, 0.95, 1.0),
    ];
    let mesh = meshes.add(Cuboid::new(0.12, 0.12, 0.12));
    let materials_arr = COLORS.map(|c| {
        materials.add(StandardMaterial {
            base_color: Color::srgb(0.82, 0.93, 0.74),
            unlit: true,
            emissive: LinearRgba::new(c.x, c.y, c.z, c.w),
            ..default()
        })
    });
    commands.insert_resource(HarvestParticlePool {
        mesh,
        materials: materials_arr,
    });
}

pub fn spawn_harvest_particles(
    commands: &mut Commands,
    pool: &HarvestParticlePool,
    position: Vec3,
    seed_offset: usize,
) {
    // TODO(perf-android): conservar mallas puras (cubos/esferas) sin billboards texturizados.
    // Evita overdraw y mantiene 60fps en móviles al escalar partículas "juicy".
    const OFFSETS: [Vec3; 5] = [
        Vec3::new(-0.48, 0.0, 0.21),
        Vec3::new(0.42, 0.0, -0.08),
        Vec3::new(0.1, 0.0, 0.46),
        Vec3::new(-0.2, 0.0, -0.4),
        Vec3::new(0.28, 0.0, 0.34),
    ];
    const KICKS: [Vec3; 5] = [
        Vec3::new(-0.82, 2.1, 0.45),
        Vec3::new(0.74, 2.35, -0.32),
        Vec3::new(0.3, 2.45, 0.7),
        Vec3::new(-0.55, 1.9, -0.66),
        Vec3::new(0.62, 2.2, 0.58),
    ];

    for i in 0..5 {
        let idx = (i + seed_offset) % 5;
        commands.spawn((
            Mesh3d(pool.mesh.clone()),
            MeshMaterial3d(pool.materials[idx].clone()),
            Transform::from_translation(position + OFFSETS[idx] * 0.55),
            ProceduralParticle {
                velocity: KICKS[idx],
                lifetime: Timer::from_seconds(0.42, TimerMode::Once),
                scale_decay: 2.7,
            },
            Name::new("HarvestParticle"),
        ));
    }
}

fn trigger_impact_squash(
    actions: Res<TouchActions>,
    mut edge: ResMut<ActionEdge>,
    mut commands: Commands,
    player: Query<&Transform, With<Player>>,
    mut targets: Query<(Entity, &Transform, Option<&SquashStretch>), With<HarvestReactive>>,
    mut shake: EventWriter<CameraShakeEvent>,
) {
    let just_pressed = actions.action && !edge.was_pressed;
    edge.was_pressed = actions.action;
    if !just_pressed || !actions.action {
        return;
    }
    let Ok(player_tf) = player.get_single() else {
        return;
    };
    let mut closest: Option<(Entity, Vec3)> = None;
    let mut best_d2 = f32::MAX;
    for (entity, tf, _) in &mut targets {
        let d2 = tf.translation.distance_squared(player_tf.translation);
        if d2 < best_d2 {
            best_d2 = d2;
            closest = Some((entity, tf.scale));
        }
    }
    if let Some((entity, base_scale)) = closest {
        if best_d2 <= 7.0 * 7.0 {
            commands.entity(entity).insert(SquashStretch {
                timer: Timer::from_seconds(0.15, TimerMode::Once),
                duration: 0.15,
                strength: 0.22,
                base_scale,
            });
            shake.send(CameraShakeEvent { intensity: 0.12 });
        }
    }
}

fn tick_particles(
    mut commands: Commands,
    mut particles: Query<(Entity, &mut ProceduralParticle, &mut Transform)>,
    time: Res<Time>,
    mut span: ResMut<CpuSpanStats>,
) {
    let _guard = span_guard(&mut span, CpuSpanSlot::Particles);
    let dt = time.delta_secs().min(0.05);
    for (e, mut p, mut tf) in &mut particles {
        p.lifetime.tick(time.delta());
        if p.lifetime.finished() {
            commands.entity(e).despawn();
            continue;
        }

        p.velocity.y -= 9.8 * dt * 0.45;
        tf.translation += p.velocity * dt;

        if tf.translation.y < 0.12 {
            tf.translation.y = 0.12;
            p.velocity.y = p.velocity.y.abs() * 0.45;
            p.velocity.x *= 0.82;
            p.velocity.z *= 0.82;
        }

        let decay = (1.0 - dt * p.scale_decay).clamp(0.75, 1.0);
        tf.scale *= Vec3::splat(decay);
    }
}

fn tick_squash_stretch(
    mut commands: Commands,
    time: Res<Time>,
    mut query: Query<(Entity, &mut Transform, &mut SquashStretch)>,
) {
    for (entity, mut tf, mut squash) in &mut query {
        squash.timer.tick(time.delta());
        let t = (squash.timer.elapsed_secs() / squash.duration).clamp(0.0, 1.0);
        let e = elastic_out(t);
        let y = 1.0 + (1.0 - e) * squash.strength;
        let xz = 1.0 - (1.0 - e) * squash.strength * 0.52;
        tf.scale = Vec3::new(
            squash.base_scale.x * xz,
            squash.base_scale.y * y,
            squash.base_scale.z * xz,
        );
        if squash.timer.finished() {
            tf.scale = squash.base_scale;
            commands.entity(entity).remove::<SquashStretch>();
        }
    }
}

fn elastic_out(t: f32) -> f32 {
    if t <= 0.0 {
        return 0.0;
    }
    if t >= 1.0 {
        return 1.0;
    }
    let c4 = (2.0 * std::f32::consts::PI) / 3.0;
    (2.0_f32).powf(-10.0 * t) * ((t * 10.0 - 0.75) * c4).sin() + 1.0
}
