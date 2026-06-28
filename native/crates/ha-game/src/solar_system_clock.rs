//! Shared arcade time for heliocentric orbits — one universe, all bodies.

use bevy::prelude::*;

#[derive(Resource)]
pub struct SolarSystemClock {
    pub t: f32,
}

impl Default for SolarSystemClock {
    fn default() -> Self {
        Self { t: 0.0 }
    }
}

pub struct SolarSystemClockPlugin;

impl Plugin for SolarSystemClockPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<SolarSystemClock>()
            .add_systems(Update, advance_solar_clock);
    }
}

fn advance_solar_clock(time: Res<Time>, mut clock: ResMut<SolarSystemClock>) {
    clock.t += time.delta_secs();
}
