//! Active camera observer — player, rocket, or remote module.

use bevy::prelude::*;
use ha_shared::{
    body_center_at, planet_gameplay_world_pos, CelestialBody, VesselId,
};

use crate::floating_origin::earth_relative_anchor;
use crate::rocket_flight::RocketFlight;
use crate::sim::{Player, SimState};
use ha_shared::FlightPhase;

pub struct ViewTargetPlugin;

impl Plugin for ViewTargetPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<ViewTarget>()
            .add_systems(Update, (sync_view_target_observer, hide_player_during_remote));
    }
}

#[derive(Resource, Clone, Debug)]
pub struct ViewTarget {
    pub mode: ViewMode,
    /// Last resolved observer in flight/world metres (for floating origin).
    pub observer_world: Vec3,
}

#[derive(Clone, Debug)]
pub enum ViewMode {
    Player,
    Rocket,
    RemoteCamera {
        vessel: VesselId,
        body: CelestialBody,
        tx: f32,
        tz: f32,
    },
}

impl Default for ViewTarget {
    fn default() -> Self {
        Self {
            mode: ViewMode::Player,
            observer_world: Vec3::ZERO,
        }
    }
}

impl ViewTarget {
    pub fn is_remote(&self) -> bool {
        matches!(self.mode, ViewMode::RemoteCamera { .. })
    }

    pub fn set_player(&mut self) {
        self.mode = ViewMode::Player;
    }

    pub fn set_remote_luna(&mut self, tx: f32, tz: f32) {
        self.mode = ViewMode::RemoteCamera {
            vessel: VesselId::LunaOutpost,
            body: CelestialBody::Moon,
            tx,
            tz,
        };
    }

    pub fn observer_world_pos(&self) -> Vec3 {
        self.observer_world
    }

    pub fn origin_anchor(&self, t: f32) -> Vec3 {
        match &self.mode {
            ViewMode::RemoteCamera { body, .. } => Vec3::from_array(body_center_at(*body, t)),
            _ => earth_relative_anchor(self.observer_world, t),
        }
    }
}

fn sync_view_target_observer(
    flight: Res<RocketFlight>,
    mut view: ResMut<ViewTarget>,
    player: Query<&SimState, With<Player>>,
    player_tf: Query<&Transform, With<Player>>,
) {
    if matches!(view.mode, ViewMode::RemoteCamera { .. }) {
        if let ViewMode::RemoteCamera { body, tx, tz, .. } = &view.mode {
            view.observer_world =
                Vec3::from_array(planet_gameplay_world_pos(*body, *tx, *tz, 1.63));
        }
        return;
    }

    if flight.active && flight.phase != FlightPhase::Surface {
        view.mode = ViewMode::Rocket;
        view.observer_world = flight.pos;
        return;
    }

    view.mode = ViewMode::Player;
    if let Ok(sim) = player.get_single() {
        if let Ok(tf) = player_tf.get_single() {
            view.observer_world = tf.translation;
        } else {
            view.observer_world = Vec3::new(sim.body.x, sim.body.y, sim.body.z);
        }
    }
}

fn hide_player_during_remote(
    view: Res<ViewTarget>,
    mut body: Query<&mut Visibility, (With<Player>, With<Mesh3d>)>,
) {
    let vis = if view.is_remote() {
        Visibility::Hidden
    } else {
        Visibility::Visible
    };
    for mut v in &mut body {
        *v = vis;
    }
}
