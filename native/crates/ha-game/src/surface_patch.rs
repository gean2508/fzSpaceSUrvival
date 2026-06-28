//! Streaming tangent patches for all landable celestial surfaces (SFS-style local ground).

use bevy::prelude::*;
use ha_shared::{
    body_def, body_landable, body_walk_radius, CelestialBody, LAUNCH_PAD_ZONE_X, LAUNCH_PAD_ZONE_Z,
    WorldContext,
};

use crate::lunar_surface::build_planet_patch_mesh;
use crate::materials::{block_std_material, luna_regolith_std};
use crate::planet_world::ActivePlanet;
use crate::rocket_flight::RocketFlight;
use crate::scene::{RegionId, RegionRuntime, WorldContextRuntime};
use crate::sim::{Player, SimState};

/// Visible ground radius around the player (tangent metres).
pub const SURFACE_PATCH_HALF: f32 = 150.0;
pub const SURFACE_PATCH_STEP: f32 = 2.5;
/// Finer lattice on Luna so crater bowls read on the patch.
pub const LUNA_PATCH_STEP: f32 = 2.0;
pub const SURFACE_PATCH_REBUILD: f32 = 28.0;

pub struct SurfacePatchPlugin;

impl Plugin for SurfacePatchPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<SurfacePatchStreamState>()
            .add_systems(Update, stream_active_surface_patch);
    }
}

#[derive(Resource, Default)]
pub struct SurfacePatchStreamState {
    pub tracked_body: Option<CelestialBody>,
    pub center_tx: f32,
    pub center_tz: f32,
}

/// Walkable displaced shell — one entity per landable body (only one active at a time).
#[derive(Component, Clone, Copy)]
pub struct CelestialSurfacePatch {
    pub body: CelestialBody,
}

pub fn default_spawn_tangent(body: CelestialBody) -> (f32, f32) {
    match body {
        CelestialBody::Earth => (LAUNCH_PAD_ZONE_X, LAUNCH_PAD_ZONE_Z),
        CelestialBody::Moon => (0.0, -18.0),
        other => (0.0, -body_walk_radius(other) * 0.04),
    }
}

pub fn patch_mesh_step(body: CelestialBody) -> f32 {
    if body == CelestialBody::Moon {
        LUNA_PATCH_STEP
    } else {
        SURFACE_PATCH_STEP
    }
}

pub fn spawn_celestial_surface_patch<D: Component + Clone>(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    std_mats: &mut Assets<StandardMaterial>,
    body: CelestialBody,
    cx: f32,
    cz: f32,
    decor: D,
    name: impl Into<String>,
    extra: impl Bundle,
) {
    let step = patch_mesh_step(body);
    let mesh = meshes.add(build_planet_patch_mesh(body, cx, cz, SURFACE_PATCH_HALF, step));
    let material = patch_material(body, std_mats);

    commands.spawn((
        Mesh3d(mesh),
        MeshMaterial3d(material),
        Transform::IDENTITY,
        CelestialSurfacePatch { body },
        decor,
        Name::new(name.into()),
        extra,
    ));
}

fn patch_material(
    body: CelestialBody,
    std_mats: &mut Assets<StandardMaterial>,
) -> Handle<StandardMaterial> {
    if body == CelestialBody::Moon {
        std_mats.add(luna_regolith_std())
    } else {
        std_mats.add(block_std_material(body_def(body).color))
    }
}

pub fn active_landable_body(
    region: &RegionRuntime,
    world_ctx: &WorldContextRuntime,
    flight: &RocketFlight,
    _planet: &ActivePlanet,
) -> Option<CelestialBody> {
    if flight.active {
        return None;
    }
    match (&region.active, world_ctx.active) {
        (RegionId::Overworld { .. }, WorldContext::Overworld) => Some(CelestialBody::Earth),
        (RegionId::Luna, WorldContext::Luna) => Some(CelestialBody::Moon),
        (RegionId::Planet(body), WorldContext::Planet) if body_landable(*body) => Some(*body),
        _ => None,
    }
}

fn stream_active_surface_patch(
    region: Res<RegionRuntime>,
    world_ctx: Res<WorldContextRuntime>,
    flight: Res<RocketFlight>,
    planet: Res<ActivePlanet>,
    player: Query<&SimState, With<Player>>,
    mut stream: ResMut<SurfacePatchStreamState>,
    mut patches: Query<(&CelestialSurfacePatch, &mut Mesh3d)>,
    mut meshes: ResMut<Assets<Mesh>>,
) {
    let Some(body) = active_landable_body(&region, &world_ctx, &flight, &planet) else {
        return;
    };
    let Ok(sim) = player.get_single() else {
        return;
    };

    let body_changed = stream.tracked_body != Some(body);
    if body_changed {
        stream.tracked_body = Some(body);
        stream.center_tx = sim.body.x;
        stream.center_tz = sim.body.z;
    }

    let dx = sim.body.x - stream.center_tx;
    let dz = sim.body.z - stream.center_tz;
    let needs_rebuild =
        body_changed || dx * dx + dz * dz >= SURFACE_PATCH_REBUILD * SURFACE_PATCH_REBUILD;
    if !needs_rebuild {
        return;
    }

    if !body_changed {
        stream.center_tx = sim.body.x;
        stream.center_tz = sim.body.z;
    }

    let step = patch_mesh_step(body);
    let new_mesh = meshes.add(build_planet_patch_mesh(
        body,
        stream.center_tx,
        stream.center_tz,
        SURFACE_PATCH_HALF,
        step,
    ));

    for (patch, mut mesh3d) in &mut patches {
        if patch.body == body {
            *mesh3d = Mesh3d(new_mesh);
            return;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn moon_default_spawn_is_return_zone() {
        let (x, z) = default_spawn_tangent(CelestialBody::Moon);
        assert!((x - 0.0).abs() < 1e-6);
        assert!((z + 18.0).abs() < 1e-6);
    }
}
