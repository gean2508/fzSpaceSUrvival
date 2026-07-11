//! Despawn off-world decor when changing celestial region — prevents OOM on Redmi.

use bevy::prelude::*;

use crate::moon_world::{despawn_moon_entities, MoonWorldState};
use crate::planet_world::{despawn_planet_entities, PlanetWorldState};
use crate::scene::{RegionId, RegionRuntime};
use crate::view_target::ViewTarget;

#[derive(Resource, Default)]
pub struct CelestialCleanup {
    pub despawn_moon: bool,
    pub despawn_planet: bool,
}

/// Hysteresis before despawning hidden regions (seconds).
#[derive(Resource)]
pub struct RegionCleanupScheduler {
    pub last_region: Option<RegionId>,
    pub moon_timer: Timer,
    pub planet_timer: Timer,
    pub pending_moon: bool,
    pub pending_planet: bool,
}

impl Default for RegionCleanupScheduler {
    fn default() -> Self {
        Self {
            last_region: None,
            moon_timer: Timer::from_seconds(2.0, TimerMode::Once),
            planet_timer: Timer::from_seconds(2.0, TimerMode::Once),
            pending_moon: false,
            pending_planet: false,
        }
    }
}

pub struct CelestialCleanupPlugin;

impl Plugin for CelestialCleanupPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<CelestialCleanup>()
            .init_resource::<RegionCleanupScheduler>()
            .add_systems(PreUpdate, process_celestial_cleanup)
            .add_systems(Update, schedule_region_cleanup);
    }
}

fn schedule_region_cleanup(
    time: Res<Time>,
    region: Res<RegionRuntime>,
    view: Res<ViewTarget>,
    moon: Res<MoonWorldState>,
    planet: Res<PlanetWorldState>,
    mut sched: ResMut<RegionCleanupScheduler>,
    mut cleanup: ResMut<CelestialCleanup>,
) {
    if sched.last_region != Some(region.active) {
        sched.last_region = Some(region.active);
    }

    let on_moon = matches!(region.active, RegionId::Luna) || view.is_remote();
    let on_planet = matches!(region.active, RegionId::Planet(_));

    // Schedule despawn when region no longer needed.
    // Reset the timer only on the transition — resetting every frame would
    // keep it from ever finishing and the region would never despawn (OOM).
    if !on_moon && moon.spawned && !view.is_remote() && !sched.pending_moon {
        sched.pending_moon = true;
        sched.moon_timer.reset();
    }
    if !on_planet && planet.spawned && !sched.pending_planet {
        sched.pending_planet = true;
        sched.planet_timer.reset();
    }

    // Cancel pending if we return to region.
    if on_moon {
        sched.pending_moon = false;
    }
    if on_planet {
        sched.pending_planet = false;
    }

    // Don't despawn moon while remote camera active on Luna.
    if view.is_remote() {
        sched.pending_moon = false;
    }

    sched.moon_timer.tick(time.delta());
    sched.planet_timer.tick(time.delta());

    if sched.pending_moon && sched.moon_timer.finished() {
        cleanup.despawn_moon = true;
        sched.pending_moon = false;
    }
    if sched.pending_planet && sched.planet_timer.finished() {
        cleanup.despawn_planet = true;
        sched.pending_planet = false;
    }
}

fn process_celestial_cleanup(
    mut commands: Commands,
    mut cleanup: ResMut<CelestialCleanup>,
    mut moon: ResMut<MoonWorldState>,
    mut planet: ResMut<PlanetWorldState>,
    moon_entities: Query<Entity, With<crate::moon_world::MoonDecor>>,
    planet_entities: Query<Entity, With<crate::planet_world::PlanetDecor>>,
) {
    if cleanup.despawn_moon {
        despawn_moon_entities(&mut commands, &mut moon, &moon_entities);
        cleanup.despawn_moon = false;
        eprintln!("ha-celestial: despawned Luna surface");
    }
    if cleanup.despawn_planet {
        despawn_planet_entities(&mut commands, &mut planet, &planet_entities);
        cleanup.despawn_planet = false;
        eprintln!("ha-celestial: despawned planet surface");
    }
}
