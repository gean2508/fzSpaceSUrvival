//! CIVILIZACIÓN — una sociedad que se simula sola, en abstracto, y en la que el
//! jugador vive como una persona más.
//!
//! No hay guion. Hay una **física del saber** (`saber`, `ciencia`): un grafo de
//! ideas con prerrequisitos, sustrato físico que las hace posibles y gente que las
//! aprende, enseña, archiva u olvida. Encima hay pueblos con economía (`economia`),
//! gente con rasgos, familia y muerte (`persona`, `gente`), poder con varias fuentes
//! de legitimidad (`gobierno`), diplomacia y guerra (`guerra`), obras, naves y
//! colonias (`obras`), creencias (`cultos`) y la crónica de todo eso (`cronica`).
//! Que alguien encuentre una pieza de nave, la estudie y llegue a la Luna no está
//! escrito en ningún lado: sale de esas reglas.
//!
//! Es solo Rust (`ha-shared`) y no tiene espejo TS: no es sim de servidor sino
//! del cliente. Sólo se ve lo que está cerca del jugador; el resto es aritmética.

pub mod azar;
pub mod ciencia;
pub mod cronica;
pub mod cultos;
pub mod catastrofe;
pub mod economia;
pub mod fisica;
pub mod gente;
pub mod gobierno;
pub mod guerra;
pub mod jugador;
pub mod mundo;
pub mod nombres;
pub mod obras;
pub mod persona;
pub mod pueblo;
pub mod saber;

#[cfg(test)]
mod pruebas;

pub use cronica::{Cronica, Hecho, Tipo};
pub use jugador::{Baja, Decreto};
pub use mundo::{Jugador, Mundo, Sitio, JUGADOR_ID};
pub use persona::{Oficio, Persona};
pub use pueblo::{Convencion, Pueblo};
pub use saber::{Saberes, N_SABERES};

/// Días de civilización que dura un año. ~30 s reales = 1 día.
pub const DIAS_ANIO: u64 = 20;

/// Los pueblos de la Tierra según la geografía del juego (el mismo sorteo que ve el jugador).
pub fn sitios_tierra() -> Vec<Sitio> {
    use crate::celestial_bodies::CelestialBody;
    use crate::geography::{settlement_in, SettlementTier, SITE_PERIOD};
    let mut v = Vec::new();
    for sz in -12..=12i64 {
        for sx in -12..=12i64 {
            let Some(s) = settlement_in(CelestialBody::Earth, sx, sz) else { continue };
            let rango = match s.tier {
                SettlementTier::Hamlet => 0,
                SettlementTier::Town => 1,
                SettlementTier::City => 2,
            };
            let _ = SITE_PERIOD;
            v.push(Sitio { x: s.center.0 as f32, z: s.center.1 as f32, rango, nombre: s.name.map(|n| n.to_string()) });
        }
    }
    v
}
