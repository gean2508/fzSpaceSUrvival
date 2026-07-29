//! Identidad atmosférica de cada cuerpo — colores y techo del cielo.
//!
//! ⚠ Este módulo ERA la coreografía del "swap" superficie↔órbita: bandas de
//! altitud (`swap_up`, `cover_full`, `clear_end`…) que decían cuándo tapar la
//! pantalla con nubes opacas para esconder un teletransporte. **Ese salto ya no
//! existe** (29 jul 2026): el juego tiene un solo marco continuo y el terreno se
//! funde por LOD (ver `ha-game/src/universe.rs` y biblia §13-septendecies).
//!
//! Lo que quedó es lo único que seguía siendo verdad: de qué color es el aire de
//! cada mundo y hasta dónde llega. La opacidad de la bruma la calcula el cliente
//! (`ha-game/src/transition.rs`), con techo — nunca ciega al jugador.

use crate::celestial_bodies::{body_atmo_top, CelestialBody};

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TransitionProfile {
    /// Hay aire (nubes/bruma) en vez de vacío.
    pub has_clouds: bool,
    /// Tinte de la atmósfera (sRGB) — bruma, halo orbital y cielo del cuerpo.
    pub cover_color: [f32; 3],
    /// Altura a la que el cielo ya es vacío estrellado; 0 = ya lo es en el suelo.
    pub sky_fade_end: f32,
}

/// El perfil atmosférico de un cuerpo, derivado de su techo de atmósfera.
pub fn transition_profile(body: CelestialBody) -> TransitionProfile {
    let atmo = body_atmo_top(body);
    TransitionProfile {
        has_clouds: atmo > 0.0,
        // Los cuerpos sin aire usan su propio color: su halo y su cielo son ellos
        // mismos, no un gris genérico.
        cover_color: if atmo > 0.0 {
            cover_color(body)
        } else {
            crate::celestial_bodies::body_def(body).color
        },
        // El vacío llega al doble del techo atmosférico (misma curva que
        // `ha-game/src/transition.rs::sky_blend_at`).
        sky_fade_end: atmo * 2.0,
    }
}

/// Tinte del aire por cuerpo — vende la identidad de cada atmósfera.
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn airless_bodies_have_no_sky_and_wear_their_own_colour() {
        let p = transition_profile(CelestialBody::Moon);
        assert!(!p.has_clouds);
        assert_eq!(p.sky_fade_end, 0.0);
        assert_eq!(p.cover_color, crate::celestial_bodies::body_def(CelestialBody::Moon).color);
    }

    #[test]
    fn air_worlds_fade_to_void_above_their_atmosphere() {
        for &b in CelestialBody::all() {
            let p = transition_profile(b);
            if p.has_clouds {
                assert!(p.sky_fade_end > body_atmo_top(b), "{b:?}");
            }
        }
    }
}
