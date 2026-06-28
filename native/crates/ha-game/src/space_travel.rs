//! Orbital travel — dock assembly, re-entry, and transit to any solar body.

use bevy::prelude::*;
use ha_shared::{
    assemble_space_module, body_center, body_def, body_landable, body_transit_burn,
    planet_gameplay_world_pos, earth_center, luna_gameplay_world_pos, moon_center,
    CelestialBody, InteractActionKind, InteractTargetKind, EARTH_ATMO_TOP_M, FlightPhase,
    LAUNCH_PAD_ZONE_X, LAUNCH_PAD_ZONE_Z, WorldContext,
};

use crate::input::{EquippedItem, HoldInteractState};
use crate::moon_world::MoonWorldState;
use crate::planet_world::PlanetWorldState;
use crate::rocket_flight::RocketFlight;
use crate::scene::{RegionId, RegionRuntime, WorldContextRuntime};
use crate::space_world::{
    spawn_station_module_visual, SpaceStationModuleSlot, SpaceStationRes,
};
use crate::sim::{Player, SimState};
use crate::targeting::LookTarget;

/// Orbital start point above the launch pad — flight-scale metres (not dock walk mesh).
pub fn orbital_start_above_pad() -> Vec3 {
    let ec = Vec3::from_array(earth_center());
    ec + Vec3::new(
        LAUNCH_PAD_ZONE_X,
        EARTH_ATMO_TOP_M + 420.0,
        LAUNCH_PAD_ZONE_Z,
    )
}

/// Orbital dock — directly above launch pad at atmosphere edge (walk hub uses compressed coords).
pub fn space_dock_origin() -> Vec3 {
    Vec3::new(
        LAUNCH_PAD_ZONE_X,
        EARTH_ATMO_TOP_M + 8.0,
        LAUNCH_PAD_ZONE_Z,
    )
}

const REENTRY_BURN_SPEED: f32 = 55.0;
const LUNA_ESCAPE_BURN_SPEED: f32 = 48.0;

pub struct SpaceTravelPlugin;

impl Plugin for SpaceTravelPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            Update,
            (
                try_assemble_station_module,
                try_space_reentry,
                try_space_transit_luna,
                try_space_transit_body,
                try_lunar_return_to_space,
                try_planet_return_to_space,
            ),
        );
    }
}

pub(crate) fn try_assemble_station_module(
    mut commands: Commands,
    mut hold: ResMut<HoldInteractState>,
    target: Res<LookTarget>,
    equipped: Res<EquippedItem>,
    region: Res<RegionRuntime>,
    flight: Res<RocketFlight>,
    player: Query<&Transform, With<Player>>,
    mut station: ResMut<SpaceStationRes>,
    mut inv: ResMut<crate::inventory_persist::PlayerInventory>,
    mut persist: ResMut<crate::inventory_persist::InventoryPersistState>,
    slots: Query<(&SpaceStationModuleSlot, &GlobalTransform)>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut std_mats: ResMut<Assets<StandardMaterial>>,
) {
    if !hold.completed || hold.placing_machine {
        if hold.completed {
            hold.completed = false;
        }
        return;
    }
    if region.active != RegionId::Space {
        if hold.completed {
            hold.completed = false;
        }
        return;
    }
    if flight.active && flight.speed() > 2.5 {
        hold.completed = false;
        return;
    }
    let pilot_pos = if flight.active {
        flight.pos
    } else if let Ok(tf) = player.get_single() {
        tf.translation
    } else {
        hold.completed = false;
        return;
    };
    let Some(kind) = target.kind else {
        return;
    };
    if kind != InteractTargetKind::SpaceStationModule {
        return;
    }
    let Some(entity) = target.entity else {
        return;
    };
    let Ok((marker, gtf)) = slots.get(entity) else {
        return;
    };
    let Some(rule) = ha_shared::resolve_interaction(Some(equipped.item_id.as_str()), kind) else {
        return;
    };
    if rule.kind != InteractActionKind::Assemble {
        return;
    }
    hold.completed = false;
    let idx = marker.module_index;
    let pos = gtf.translation();
    if pilot_pos.distance(pos) > 9.0 {
        return;
    }
    if assemble_space_module(&mut station.0, idx, &mut inv) {
        persist.mark_immediate();
        spawn_station_module_visual(&mut commands, &mut meshes, &mut std_mats, idx, pos);
        eprintln!(
            "ha-space: station module {idx} ({}/3)",
            station.assembled_modules
        );
    }
}

fn begin_transit_burn(flight: &mut RocketFlight, target: CelestialBody, from: Vec3) {
    let to = Vec3::from_array(body_center(target));
    let vel = (to - from).normalize_or_zero() * body_transit_burn(target);
    flight.begin_coast_toward(from, vel, target);
}

pub(crate) fn try_space_reentry(
    mut hold: ResMut<HoldInteractState>,
    target: Res<LookTarget>,
    region: Res<RegionRuntime>,
    mut flight: ResMut<RocketFlight>,
    mut world_ctx: ResMut<WorldContextRuntime>,
) {
    if !hold.completed || region.active != RegionId::Space {
        return;
    }
    let Some(kind) = target.kind else {
        return;
    };
    if kind != InteractTargetKind::SpaceReentry {
        return;
    }
    hold.completed = false;

    let start = orbital_start_above_pad();
    let to_earth = (Vec3::from_array(earth_center()) - start).normalize_or_zero();
    let vel = to_earth * REENTRY_BURN_SPEED;

    if flight.active {
        flight.pos = start;
        flight.vel = vel;
        flight.phase = FlightPhase::Coast;
        flight.body = CelestialBody::Earth;
        flight.surface_flight = None;
    } else {
        flight.begin_coast_toward(start, vel, CelestialBody::Earth);
    }
    world_ctx.active = WorldContext::Overworld;
    eprintln!("ha-flight: reentry burn toward Earth");
}

pub(crate) fn try_space_transit_luna(
    mut hold: ResMut<HoldInteractState>,
    target: Res<LookTarget>,
    region: Res<RegionRuntime>,
    mut flight: ResMut<RocketFlight>,
    mut world_ctx: ResMut<WorldContextRuntime>,
    mut moon: ResMut<MoonWorldState>,
) {
    if !hold.completed || region.active != RegionId::Space {
        return;
    }
    let Some(kind) = target.kind else {
        return;
    };
    if kind != InteractTargetKind::SpaceTransitLuna {
        return;
    }
    hold.completed = false;

    let start = orbital_start_above_pad();
    begin_transit_burn(&mut flight, CelestialBody::Moon, start);
    world_ctx.active = WorldContext::Space;
    moon.needs_spawn = true;
    moon.spawn_after_frames = 0;
    eprintln!("ha-flight: transit burn toward Moon");
}

pub(crate) fn try_space_transit_body(
    mut hold: ResMut<HoldInteractState>,
    target: Res<LookTarget>,
    region: Res<RegionRuntime>,
    mut flight: ResMut<RocketFlight>,
    mut world_ctx: ResMut<WorldContextRuntime>,
    mut moon: ResMut<MoonWorldState>,
    mut planet: ResMut<PlanetWorldState>,
    interacts: Query<&crate::targeting::InteractTarget>,
) {
    if !hold.completed || region.active != RegionId::Space {
        return;
    }
    let Some(kind) = target.kind else {
        return;
    };
    if kind != InteractTargetKind::SpaceTransitBody {
        return;
    }
    let Some(entity) = target.entity else {
        hold.completed = false;
        return;
    };
    let Ok(marker) = interacts.get(entity) else {
        hold.completed = false;
        return;
    };
    let Some(body) = CelestialBody::from_u8(marker.id as u8) else {
        hold.completed = false;
        return;
    };
    if body == CelestialBody::Earth || body == CelestialBody::Sun || !body_landable(body) {
        hold.completed = false;
        return;
    }
    hold.completed = false;

    let start = orbital_start_above_pad();
    begin_transit_burn(&mut flight, body, start);
    world_ctx.active = WorldContext::Space;
    if body == CelestialBody::Moon {
        moon.needs_spawn = true;
        moon.spawn_after_frames = 0;
    } else {
        crate::planet_world::request_planet_spawn(&mut planet, body);
    }
    eprintln!("ha-flight: transit burn toward {}", body_def(body).label);
}

pub(crate) fn try_lunar_return_to_space(
    mut hold: ResMut<HoldInteractState>,
    target: Res<LookTarget>,
    mut region: ResMut<RegionRuntime>,
    mut flight: ResMut<RocketFlight>,
    mut world_ctx: ResMut<WorldContextRuntime>,
    mut space: ResMut<crate::space_world::SpaceWorldState>,
    player: Query<&SimState, With<Player>>,
) {
    if !hold.completed || region.active != RegionId::Luna {
        return;
    }
    let Some(kind) = target.kind else {
        return;
    };
    if kind != InteractTargetKind::LunarReturn {
        return;
    }
    hold.completed = false;
    let Ok(sim) = player.get_single() else {
        return;
    };

    let start = Vec3::from_array(luna_gameplay_world_pos(sim.body.x, sim.body.z, 3.0));
    let dir = ha_shared::planet_dir_from_tangent(CelestialBody::Moon, sim.body.x, sim.body.z);
    let vel = Vec3::from_array(dir) * LUNA_ESCAPE_BURN_SPEED;
    flight.begin_surface_gameplay(CelestialBody::Moon, start, vel);
    region.active = RegionId::Space;
    world_ctx.active = WorldContext::Space;
    space.needs_spawn = true;
    eprintln!("ha-flight: lunar escape burn toward orbital dock");
}

pub(crate) fn try_planet_return_to_space(
    mut hold: ResMut<HoldInteractState>,
    target: Res<LookTarget>,
    region: Res<RegionRuntime>,
    mut flight: ResMut<RocketFlight>,
    mut world_ctx: ResMut<WorldContextRuntime>,
    mut space: ResMut<crate::space_world::SpaceWorldState>,
    player: Query<&SimState, With<Player>>,
) {
    if !hold.completed {
        return;
    }
    let RegionId::Planet(body) = region.active else {
        return;
    };
    let Some(kind) = target.kind else {
        return;
    };
    if kind != InteractTargetKind::PlanetReturn {
        return;
    }
    hold.completed = false;
    let Ok(sim) = player.get_single() else {
        return;
    };

    let start = Vec3::from_array(planet_gameplay_world_pos(body, sim.body.x, sim.body.z, 3.0));
    let dir = ha_shared::planet_dir_from_tangent(body, sim.body.x, sim.body.z);
    let vel = Vec3::from_array(dir) * LUNA_ESCAPE_BURN_SPEED;
    flight.begin_surface_gameplay(body, start, vel);
    world_ctx.active = WorldContext::Space;
    space.needs_spawn = true;
    eprintln!("ha-flight: {} escape burn toward dock", body_def(body).label);
}
