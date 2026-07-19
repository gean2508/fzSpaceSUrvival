//! Shared surface↔space transition state — ONE choreography for rocket and
//! creative flight, on every celestial body.
//!
//! The active flight (rocket XOR creative — they are mutually exclusive)
//! writes this resource each frame from `ha_shared::transition_profile`;
//! passive readers consume it: `sky2d` (cover overlay + sky fade), `camera`
//! (zoom while transitioning), `world2d` (wide streaming ring),
//! `rocket_flight::sync_flight_world_state` (the altitude-gated region swap).
//! B8 holds by construction: the swap is pure render/region under full cover,
//! physics and input never notice.

use bevy::prelude::*;
use ha_shared::{transition_profile, CelestialBody};

/// Viewport height (m) standing on the ground (matches the surface camera).
pub const GROUND_VIEW_H: f32 = 30.0;
/// Viewport height (m) at the frame swap — small enough that the streamed
/// terrain ring always fills it; the cover hides whatever lies beyond.
pub const SWAP_VIEW_H: f32 = 180.0;

#[derive(Clone, Copy, PartialEq, Eq, Default, Debug)]
pub enum TransitionMode {
    #[default]
    None,
    /// Below the swap band, surface frame, climbing out.
    Ascent,
    /// Below the swap band, surface frame, coming down.
    Descent,
    /// Orbital frame — cover/zoom still tracked so approaching any body
    /// rolls its atmosphere back in.
    Space,
}

#[derive(Resource)]
pub struct FlightTransition {
    pub mode: TransitionMode,
    /// Body whose transition profile is in effect.
    pub body: CelestialBody,
    /// Altitude above that body's surface (m).
    pub alt: f32,
    /// Cover overlay opacity 0..1 (clouds / airless fade) → `sky2d`.
    pub cover: f32,
    /// Cover overlay tint (sRGB) → `sky2d`.
    pub cover_color: [f32; 3],
    /// Sky fade 0 (surface sky) → 1 (void) → `sky2d`.
    pub sky_blend: f32,
    /// Zoom the transition wants while it owns the camera (`None` = user's).
    pub view_h: Option<f32>,
    /// Surface-frame render anchor: launch pad on ascent, landing zone on
    /// descent (render x,y — the rocket sprite climbs/descends "over" it).
    pub surface_anchor: Vec2,
}

impl Default for FlightTransition {
    fn default() -> Self {
        Self {
            mode: TransitionMode::None,
            body: CelestialBody::Earth,
            alt: 0.0,
            cover: 0.0,
            cover_color: [1.0, 1.0, 1.0],
            sky_blend: 0.0,
            view_h: None,
            surface_anchor: Vec2::ZERO,
        }
    }
}

impl FlightTransition {
    pub fn active(&self) -> bool {
        self.mode != TransitionMode::None
    }

    /// In the surface frame below the swap (rocket sprite renders at the
    /// anchor, terrain visible under the closing cover).
    pub fn in_surface_frame(&self) -> bool {
        matches!(self.mode, TransitionMode::Ascent | TransitionMode::Descent)
    }

    /// Drive the choreography from the current altitude. Computes cover,
    /// sky fade and (below the swap) the camera zoom from the body profile.
    pub fn drive(&mut self, mode: TransitionMode, body: CelestialBody, alt: f32) {
        let p = transition_profile(body);
        self.mode = mode;
        self.body = body;
        self.alt = alt;
        self.cover = p.cover_at(alt);
        self.cover_color = p.cover_color;
        self.sky_blend = p.sky_blend_at(alt);
        self.view_h = match mode {
            TransitionMode::Ascent | TransitionMode::Descent => {
                let zf = (alt / p.swap_up).clamp(0.0, 1.0);
                Some(GROUND_VIEW_H + (SWAP_VIEW_H - GROUND_VIEW_H) * zf)
            }
            _ => None,
        };
    }

    /// Back on the ground / flight over: release sky, cover and camera.
    pub fn clear(&mut self) {
        *self = Self { body: self.body, surface_anchor: self.surface_anchor, ..default() };
    }
}

pub struct TransitionPlugin;

impl Plugin for TransitionPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<FlightTransition>();
    }
}
