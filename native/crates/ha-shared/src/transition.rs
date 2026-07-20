//! Surface ↔ space transition profile, per celestial body.
//!
//! ONE choreography for every ascent/descent (rocket or creative flight):
//! the camera zoom follows altitude, the sky fades to void, a cover layer
//! (clouds where there is an atmosphere, a brief dark fade where there is
//! none) fills the screen and hides the surface↔orbital frame swap, then
//! clears into space revealing the body as a circle (SFS). All altitudes
//! come from THIS profile — never hardcode transition bands elsewhere.
//!
//! Calibration: the factors reproduce EXACTLY the Earth numbers that were
//! tuned and verified by screenshot (cover 190→290 m, swap 300 m, clear by
//! 700 m, sky void by 900 m) — see biblia §11.

use crate::celestial_bodies::{body_atmo_top, CelestialBody};

/// Band factors relative to the body's atmosphere top (Earth atmo = 120 m).
/// Both swap altitudes sit INSIDE the fully-covered band
/// [cover_full, clear_start] so neither frame swap is ever visible, with a
/// hysteresis gap between them so hovering at the boundary (e.g. autopilot
/// hold) never ping-pongs the region.
const COVER_START_F: f32 = 190.0 / 120.0;
const COVER_FULL_F: f32 = 290.0 / 120.0;
const SWAP_DOWN_F: f32 = 300.0 / 120.0; // the screenshot-tuned SWAP_ALT
const SWAP_UP_F: f32 = 360.0 / 120.0;
const CLEAR_START_F: f32 = 400.0 / 120.0;
const CLEAR_END_F: f32 = 700.0 / 120.0;
const SKY_FADE_END_F: f32 = 900.0 / 120.0;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TransitionProfile {
    /// Ascending: swap surface→orbital above this (under full cover).
    pub swap_up: f32,
    /// Descending: swap orbital→surface below this (also under full cover;
    /// the gap up to `swap_up` is the hysteresis band).
    pub swap_down: f32,
    /// Cover starts rolling in.
    pub cover_start: f32,
    /// Cover fully opaque (≤ swap_down so no swap is ever seen).
    pub cover_full: f32,
    /// Cover starts clearing (already in the orbital view).
    pub clear_start: f32,
    /// Cover fully gone — open space.
    pub clear_end: f32,
    /// Sky 100 % void (stars); 0 = the surface sky is already void.
    pub sky_fade_end: f32,
    /// Atmosphere → white/tinted clouds; vacuum → brief dark fade.
    pub has_clouds: bool,
    /// Cover overlay tint (sRGB).
    pub cover_color: [f32; 3],
}

/// The transition profile for a body, derived from its atmosphere height.
pub fn transition_profile(body: CelestialBody) -> TransitionProfile {
    let atmo = body_atmo_top(body);
    if atmo <= 0.0 {
        // Airless: fixed low bands with a short fade around the swap (no clouds
        // to hide behind). The fade uses the BODY'S OWN colour (not black), so
        // the tile↔circle frame swap under it is invisible — the orbital disc
        // (same colour) and the surface both blend into it. Seamless landing on
        // airless bodies, matching the cloud-covered swap on bodies with air.
        return TransitionProfile {
            swap_up: 165.0,
            swap_down: 125.0,
            cover_start: 80.0,
            cover_full: 120.0,
            clear_start: 170.0,
            clear_end: 220.0,
            sky_fade_end: 0.0,
            has_clouds: false,
            cover_color: crate::celestial_bodies::body_def(body).color,
        };
    }
    TransitionProfile {
        swap_up: atmo * SWAP_UP_F,
        swap_down: atmo * SWAP_DOWN_F,
        cover_start: atmo * COVER_START_F,
        cover_full: atmo * COVER_FULL_F,
        clear_start: atmo * CLEAR_START_F,
        clear_end: atmo * CLEAR_END_F,
        sky_fade_end: atmo * SKY_FADE_END_F,
        has_clouds: true,
        cover_color: cover_color(body),
    }
}

/// Cloud/haze tint per body — sells each atmosphere's identity.
fn cover_color(body: CelestialBody) -> [f32; 3] {
    match body {
        CelestialBody::Earth => [0.86, 0.89, 0.95],
        CelestialBody::Venus => [0.90, 0.78, 0.55],
        CelestialBody::Mars => [0.85, 0.62, 0.48],
        CelestialBody::Titan => [0.82, 0.68, 0.42],
        CelestialBody::Uranus => [0.68, 0.82, 0.88],
        CelestialBody::Neptune => [0.62, 0.72, 0.92],
        _ => [0.80, 0.82, 0.88],
    }
}

impl TransitionProfile {
    /// Cover opacity 0..1 at an altitude: none on the ground, full through
    /// the swap, clearing into space.
    pub fn cover_at(&self, alt: f32) -> f32 {
        let rising =
            ((alt - self.cover_start) / (self.cover_full - self.cover_start)).clamp(0.0, 1.0);
        let clearing =
            ((self.clear_end - alt) / (self.clear_end - self.clear_start)).clamp(0.0, 1.0);
        rising.min(clearing)
    }

    /// Sky fade 0 (surface sky) → 1 (void) at an altitude.
    pub fn sky_blend_at(&self, alt: f32) -> f32 {
        if self.sky_fade_end <= 0.0 {
            return 1.0;
        }
        (alt / self.sky_fade_end).clamp(0.0, 1.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn earth_matches_tuned_constants() {
        // The screenshot-verified creative-ascent numbers (biblia §11).
        let p = transition_profile(CelestialBody::Earth);
        assert_eq!(p.cover_start, 190.0);
        assert_eq!(p.cover_full, 290.0);
        assert_eq!(p.swap_down, 300.0);
        assert_eq!(p.swap_up, 360.0);
        assert_eq!(p.clear_start, 400.0);
        assert_eq!(p.clear_end, 700.0);
        assert_eq!(p.sky_fade_end, 900.0);
        assert!(p.has_clouds);
    }

    #[test]
    fn earth_cover_curve_matches_old_cloud_from_alt() {
        let p = transition_profile(CelestialBody::Earth);
        let old = |alt: f32| -> f32 {
            let rising = ((alt - 190.0) / 100.0).clamp(0.0, 1.0);
            let clearing = ((700.0 - alt) / 300.0).clamp(0.0, 1.0);
            rising.min(clearing)
        };
        for alt in [0.0, 150.0, 220.0, 290.0, 300.0, 380.0, 500.0, 690.0, 900.0] {
            assert!((p.cover_at(alt) - old(alt)).abs() < 1e-5, "alt {alt}");
        }
    }

    #[test]
    fn bands_monotonic_for_all_bodies() {
        for &body in CelestialBody::all() {
            let p = transition_profile(body);
            assert!(p.cover_start < p.cover_full, "{body:?}");
            assert!(p.cover_full <= p.swap_down, "{body:?}");
            assert!(p.swap_down < p.swap_up, "{body:?}");
            assert!(p.swap_up <= p.clear_start, "{body:?}");
            assert!(p.clear_start < p.clear_end, "{body:?}");
        }
    }

    #[test]
    fn swap_is_always_fully_covered() {
        // The frame swap must never be visible: full cover at both swap
        // altitudes and everywhere in the hysteresis band between them.
        for &body in CelestialBody::all() {
            let p = transition_profile(body);
            assert_eq!(p.cover_at(p.swap_up), 1.0, "{body:?}");
            assert_eq!(p.cover_at(p.swap_down), 1.0, "{body:?}");
            let mid = (p.swap_up + p.swap_down) * 0.5;
            assert_eq!(p.cover_at(mid), 1.0, "{body:?}");
        }
    }

    #[test]
    fn airless_bodies_fade_without_clouds() {
        let p = transition_profile(CelestialBody::Moon);
        assert!(!p.has_clouds);
        // Surface sky already void.
        assert_eq!(p.sky_blend_at(0.0), 1.0);
        // Ground and open space are clear of cover.
        assert_eq!(p.cover_at(0.0), 0.0);
        assert_eq!(p.cover_at(p.clear_end + 1.0), 0.0);
    }

    #[test]
    fn cover_clears_on_both_ends() {
        for &body in CelestialBody::all() {
            let p = transition_profile(body);
            assert_eq!(p.cover_at(0.0), 0.0, "{body:?}");
            assert_eq!(p.cover_at(p.clear_end), 0.0, "{body:?}");
        }
    }
}
