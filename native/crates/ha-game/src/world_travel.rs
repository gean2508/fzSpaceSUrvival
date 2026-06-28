//! Brief HUD lock after region jumps — prevents duplicate travel triggers.

use bevy::prelude::*;

pub struct WorldTravelPlugin;

impl Plugin for WorldTravelPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<FlightTransition>()
            .add_systems(Update, tick_flight_transition);
    }
}

#[derive(Resource, Default)]
pub struct FlightTransition {
    pub active: bool,
    pub elapsed: f32,
    pub label: String,
}

impl FlightTransition {
    pub fn begin(&mut self, label: &str) {
        self.active = true;
        self.elapsed = 0.0;
        self.label = label.to_string();
    }

    pub fn blocks_travel(&self) -> bool {
        self.active
    }
}

const FLIGHT_TRANSITION_SECS: f32 = 2.0;

fn tick_flight_transition(time: Res<Time>, mut transition: ResMut<FlightTransition>) {
    if !transition.active {
        return;
    }
    transition.elapsed += time.delta_secs();
    if transition.elapsed >= FLIGHT_TRANSITION_SECS {
        transition.active = false;
        transition.label.clear();
    }
}
