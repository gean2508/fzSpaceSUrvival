//! Orbital travel helpers — escape burns from a surface back to continuous flight.
//!
//! El espacio es VUELO CONTINUO (§1). Se llega a los cuerpos VOLANDO; el viejo
//! walk-hub orbital (dock, ensamblar estación, tránsito por beacon) fue purgado.
//! Quedan: los puntos de referencia orbitales (usados por nav/sim) y el escape
//! desde Luna/planeta al espacio (mismo `begin_surface_gameplay` que el thrust).

use bevy::prelude::*;
use ha_shared::{
    body_def, planet_gameplay_world_pos, luna_gameplay_world_pos, CelestialBody,
    InteractTargetKind, EARTH_ATMO_TOP_M, LAUNCH_PAD_ZONE_X, LAUNCH_PAD_ZONE_Z, WorldContext,
};

use crate::input::HoldInteractState;
use crate::rocket_flight::RocketFlight;
use crate::scene::{RegionId, RegionRuntime, WorldContextRuntime};
use crate::sim::{Player, SimState};
use crate::targeting::LookTarget;

/// Orbital start point above the launch pad — flight-scale metres. Lo usan las
/// sondas de `nav_computer` y referencias de HUD.
pub fn orbital_start_above_pad() -> Vec3 {
    let ec = Vec3::from_array(ha_shared::earth_center());
    ec + Vec3::new(
        LAUNCH_PAD_ZONE_X,
        EARTH_ATMO_TOP_M + 420.0,
        LAUNCH_PAD_ZONE_Z,
    )
}

/// Punto de referencia sobre el pad al borde de atmósfera (lo usa `sim.rs`).
pub fn space_dock_origin() -> Vec3 {
    Vec3::new(
        LAUNCH_PAD_ZONE_X,
        EARTH_ATMO_TOP_M + 8.0,
        LAUNCH_PAD_ZONE_Z,
    )
}

const LUNA_ESCAPE_BURN_SPEED: f32 = 48.0;

pub struct SpaceTravelPlugin;

impl Plugin for SpaceTravelPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            Update,
            (try_lunar_return_to_space, try_planet_return_to_space),
        );
    }
}

pub(crate) fn try_lunar_return_to_space(
    mut hold: ResMut<HoldInteractState>,
    target: Res<LookTarget>,
    mut region: ResMut<RegionRuntime>,
    mut flight: ResMut<RocketFlight>,
    mut world_ctx: ResMut<WorldContextRuntime>,
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
    eprintln!("ha-flight: lunar escape burn toward space");
}

pub(crate) fn try_planet_return_to_space(
    mut hold: ResMut<HoldInteractState>,
    target: Res<LookTarget>,
    region: Res<RegionRuntime>,
    mut flight: ResMut<RocketFlight>,
    mut world_ctx: ResMut<WorldContextRuntime>,
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
    eprintln!("ha-flight: {} escape burn toward space", body_def(body).label);
}
