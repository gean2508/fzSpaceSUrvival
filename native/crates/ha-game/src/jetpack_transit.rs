//! Jetpack escape — Luna surface → coast toward Earth (arcade, no cutscene).

use bevy::prelude::*;
use ha_shared::{body_center_at, body_transit_burn, CelestialBody, WorldContext};

use crate::input::{EquippedItem, TouchActions};
use crate::moon_world::MoonWorldState;
use crate::rocket_flight::RocketFlight;
use crate::scene::{RegionId, RegionRuntime, WorldContextRuntime};
use crate::sim::{Player, SimState};
use crate::solar_system_clock::SolarSystemClock;
use crate::surface_placement::sphere_world_pos;
use crate::world_travel::FlightTransition;

pub const JETPACK_ITEM: &str = "jetpack";

pub struct JetpackTransitPlugin;

impl Plugin for JetpackTransitPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<JetpackTransit>()
            .add_systems(Update, try_jetpack_escape);
    }
}

#[derive(Resource, Default)]
pub struct JetpackTransit {
    pub active: bool,
}

fn try_jetpack_escape(
    actions: Res<TouchActions>,
    equipped: Res<EquippedItem>,
    region: Res<RegionRuntime>,
    transition: Res<FlightTransition>,
    clock: Res<SolarSystemClock>,
    mut flight: ResMut<RocketFlight>,
    mut moon: ResMut<MoonWorldState>,
    mut world_ctx: ResMut<WorldContextRuntime>,
    player: Query<&SimState, With<Player>>,
) {
    if transition.blocks_travel() || flight.active {
        return;
    }
    if region.active != RegionId::Luna || world_ctx.active != WorldContext::Luna {
        return;
    }
    if equipped.item_id != JETPACK_ITEM {
        return;
    }
    if !actions.jump || !actions.action {
        return;
    }
    let Ok(sim) = player.get_single() else {
        return;
    };

    let start = sphere_world_pos(CelestialBody::Moon, sim.body.x, sim.body.z, 2.2);
    let earth = body_center_at(CelestialBody::Earth, clock.t);
    let to_earth = Vec3::from_array(earth) - start;
    let vel = to_earth.normalize_or_zero() * body_transit_burn(CelestialBody::Earth);

    flight.surface_flight = None;
    flight.begin_coast_toward(start, vel, CelestialBody::Earth);
    world_ctx.active = WorldContext::Space;
    moon.needs_spawn = false;
    eprintln!("ha-jetpack: escape burn toward Earth");
}
