//! Estado ATMOSFÉRICO del vuelo — lo que el resto del motor lee mientras volás.
//!
//! Antes esto era la coreografía que TAPABA el salto superficie↔órbita con una
//! capa opaca de nubes. Ese salto ya no existe (`universe.rs`: un solo marco,
//! altura = zoom, terreno con LOD continuo), así que este módulo se quedó con lo
//! único que sigue teniendo sentido: **cuánta atmósfera hay entre la cámara y el
//! suelo**, y qué encuadre pide la altura actual.
//!
//! Reglas nuevas:
//! - `cover` es BRUMA, no una persiana: nunca llega a opaca (`MAX_HAZE`), así que
//!   el jugador SIEMPRE ve a dónde va. Si algo necesita esconderse detrás de una
//!   pantalla opaca, es un bug de continuidad, no algo para tapar.
//! - Los cuerpos sin aire no tienen bruma: su disco ya está pintado con el mismo
//!   terreno que pisás, así que no hay nada que disimular.
//! - `view_h` sale de `universe::view_h_for_alt` — una sola función para todos.

use bevy::prelude::*;
use ha_shared::{body_atmo_top, CelestialBody};

use crate::universe::{space_alt, view_h_for_alt};


/// Opacidad máxima de la bruma atmosférica. Deliberadamente < 1: la continuidad
/// se logra con LOD, no cegando al jugador.
const MAX_HAZE: f32 = 0.28;

#[derive(Clone, Copy, PartialEq, Eq, Default, Debug)]
pub enum TransitionMode {
    #[default]
    None,
    /// Subiendo dentro de la atmósfera / cerca del suelo.
    Ascent,
    /// Bajando hacia la superficie.
    Descent,
    /// Por encima del techo atmosférico del cuerpo.
    Space,
}

#[derive(Resource)]
pub struct FlightTransition {
    pub mode: TransitionMode,
    /// Cuerpo cuya atmósfera/superficie manda ahora.
    pub body: CelestialBody,
    /// Altura sobre esa superficie (m).
    pub alt: f32,
    /// Bruma atmosférica 0..MAX_HAZE → `sky2d`.
    pub cover: f32,
    /// Tinte de la bruma (sRGB) → `sky2d`.
    pub cover_color: [f32; 3],
    /// Desvanecido del cielo 0 (cielo de superficie) → 1 (vacío) → `sky2d`.
    pub sky_blend: f32,
    /// Encuadre que pide la altura (`None` = manda el usuario, a pie).
    pub view_h: Option<f32>,
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
        }
    }
}

impl FlightTransition {
    pub fn active(&self) -> bool {
        self.mode != TransitionMode::None
    }

    /// Todavía por debajo del techo atmosférico (el HUD y el audio lo usan para
    /// distinguir "volando sobre el mundo" de "en el espacio").
    pub fn in_surface_frame(&self) -> bool {
        matches!(self.mode, TransitionMode::Ascent | TransitionMode::Descent)
    }

    /// Actualiza el estado desde la altura actual. Todo lo de acá es una función
    /// PURA y continua de `alt` — no hay bandas de conmutación ni histéresis
    /// porque ya no hay nada que conmutar.
    pub fn drive(&mut self, mode: TransitionMode, body: CelestialBody, alt: f32) {
        self.mode = mode;
        self.body = body;
        self.alt = alt;
        self.cover = haze_at(body, alt);
        self.cover_color = ha_shared::transition_profile(body).cover_color;
        self.sky_blend = sky_blend_at(body, alt);
        self.view_h = Some(view_h_for_alt(body, alt));
    }

    /// De vuelta en el suelo: el cielo, la bruma y la cámara vuelven al jugador.
    pub fn clear(&mut self) {
        *self = Self { body: self.body, ..default() };
    }
}

/// Bruma atmosférica a una altura: nula en el suelo (ves tu mundo nítido), pico
/// suave cruzando la atmósfera, nula otra vez arriba (ves el planeta entero).
/// Los cuerpos sin aire no tienen ninguna.
pub fn haze_at(body: CelestialBody, alt: f32) -> f32 {
    let atmo = body_atmo_top(body);
    if atmo <= 0.0 {
        return 0.0;
    }
    // Campana sobre [0, 2·atmo] con el pico en el techo de la atmósfera.
    let x = (alt / atmo.max(1.0)).clamp(0.0, 2.0);
    let bell = 1.0 - (x - 1.0).abs();
    MAX_HAZE * bell.clamp(0.0, 1.0)
}

/// Cielo de superficie → vacío estrellado. Completo un poco por encima del
/// techo atmosférico; instantáneo en cuerpos sin aire (allí ya es vacío).
pub fn sky_blend_at(body: CelestialBody, alt: f32) -> f32 {
    let atmo = body_atmo_top(body);
    if atmo <= 0.0 {
        return 1.0;
    }
    (alt / (atmo * 2.0)).clamp(0.0, 1.0)
}

/// Modo que corresponde a una altura y una velocidad vertical.
pub fn mode_for(body: CelestialBody, alt: f32, vy: f32) -> TransitionMode {
    if alt >= space_alt(body) {
        TransitionMode::Space
    } else if vy < 0.0 {
        TransitionMode::Descent
    } else {
        TransitionMode::Ascent
    }
}

pub struct TransitionPlugin;

impl Plugin for TransitionPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<FlightTransition>();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn haze_never_blinds_and_clears_on_both_ends() {
        for &b in CelestialBody::all() {
            assert_eq!(haze_at(b, 0.0), 0.0, "{b:?} en el suelo");
            let atmo = body_atmo_top(b);
            let high = if atmo > 0.0 { atmo * 4.0 } else { 500.0 };
            assert_eq!(haze_at(b, high), 0.0, "{b:?} arriba");
            for step in 0..200 {
                let alt = step as f32 * 10.0;
                assert!(haze_at(b, alt) <= MAX_HAZE, "{b:?} alt {alt}");
            }
        }
    }

    #[test]
    fn airless_bodies_have_no_haze_at_all() {
        assert_eq!(haze_at(CelestialBody::Moon, 50.0), 0.0);
        assert_eq!(sky_blend_at(CelestialBody::Moon, 0.0), 1.0);
    }
}
