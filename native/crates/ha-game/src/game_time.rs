//! Fixed 20 Hz game clock — movement, holds, and machines use this, not render FPS.

use bevy::prelude::*;
use ha_shared::{FIXED_MAX_FRAME_DELTA_MS, TICK_RATE};

/// Clamp virtual frame delta and ensure fixed timestep matches `TICK_RATE`.
pub fn configure_game_clock(mut time_virt: ResMut<Time<Virtual>>, mut time_fixed: ResMut<Time<Fixed>>) {
    time_fixed.set_timestep_hz(TICK_RATE as f64);
    time_virt.set_max_delta(std::time::Duration::from_millis(FIXED_MAX_FRAME_DELTA_MS));
}

/// Delta seconds for systems in `FixedUpdate` (always one sim tick).
#[inline]
pub fn fixed_delta_secs(time: &Time<Fixed>) -> f32 {
    time.delta_secs()
}
