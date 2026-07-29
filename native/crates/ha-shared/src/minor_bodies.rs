//! Cuerpos MENORES del sistema: cinturón de asteroides y cometas.
//!
//! No son `CelestialBody` (no se aterriza en ellos, no tienen terreno ni
//! atmósfera), así que viven aparte y son 100 % procedurales: no hay una lista,
//! hay una FUNCIÓN de `(índice, tiempo)` a posición. Eso los hace gratis en
//! memoria y coherentes entre cliente y cualquier otro consumidor.
//!
//! Orbitan igual que todo lo demás — alrededor del Sol, en el mismo plano y con
//! el mismo reloj (`solar_ephemeris`) — porque la regla del universo nuevo es
//! que **todo orbita a algo**: lunas a sus planetas, planetas y cinturón al Sol,
//! cometas en elipses que cruzan el sistema entero.

use crate::solar_ephemeris::sun_position;

const TAU: f32 = std::f32::consts::TAU;

/// Cuántas rocas tiene el cinturón. Se dibujan como puntos; sólo las que caen
/// en pantalla cuestan algo.
pub const ASTEROID_COUNT: usize = 220;
/// Cuántos cometas recorren el sistema.
pub const COMET_COUNT: usize = 5;

/// Radio interior del cinturón (m). Entre Marte (~5.6 M del Sol) y los mundos
/// jovianos (~6.2 M), que es donde lo pondría cualquiera que mire el mapa.
const BELT_INNER_M: f32 = 5_850_000.0;
const BELT_OUTER_M: f32 = 6_150_000.0;

fn hash(i: u32, salt: u32) -> f32 {
    let mut h = i.wrapping_mul(0x9E37_79B9) ^ salt.wrapping_mul(0x85EB_CA6B);
    h ^= h >> 15;
    h = h.wrapping_mul(0xC2B2_AE35);
    h ^= h >> 13;
    (h & 0x00FF_FFFF) as f32 / 16_777_215.0
}

/// Una roca del cinturón: dónde está y cuánto mide.
#[derive(Clone, Copy, Debug)]
pub struct Asteroid {
    pub pos: [f32; 3],
    pub radius_m: f32,
    /// Giro propio (rad) — para que no todas se dibujen iguales.
    pub spin: f32,
}

pub fn asteroid_at(i: usize, t_secs: f32) -> Asteroid {
    let idx = i as u32;
    let sun = sun_position();
    // Órbita casi circular, con excentricidad e inclinación de plano fingidas
    // por un pequeño achatamiento — leen como un cinturón, no como un aro.
    let f = hash(idx, 0x51);
    let r = BELT_INNER_M + (BELT_OUTER_M - BELT_INNER_M) * f;
    let squash = 0.94 + hash(idx, 0x77) * 0.12;
    // Más lejos, más lento (como los planetas). Vueltas de 1.5 a 2.5 h.
    let period = 5_400.0 + f * 3_600.0 + hash(idx, 0x13) * 900.0;
    let a = hash(idx, 0x2b) * TAU + t_secs / period * TAU;
    Asteroid {
        pos: [
            sun[0] + a.cos() * r,
            sun[1],
            sun[2] + a.sin() * r * squash,
        ],
        // De 400 m a 2.6 km: se ven como rocas al pasar entre ellas y como polvo
        // desde lejos.
        radius_m: 400.0 + hash(idx, 0x9d).powi(2) * 2_200.0,
        spin: hash(idx, 0xa1) * TAU + t_secs * (0.05 + hash(idx, 0xc3) * 0.2),
    }
}

/// Un cometa: cabeza, tamaño y hacia dónde apunta su cola (siempre en contra
/// del Sol, como en la realidad).
#[derive(Clone, Copy, Debug)]
pub struct Comet {
    pub pos: [f32; 3],
    pub radius_m: f32,
    /// Unitario Sol→cometa: la cola se dibuja en esta dirección.
    pub tail_dir: [f32; 3],
    /// 0 lejos del Sol (apagado) … 1 en el perihelio (cola larga y brillante).
    pub activity: f32,
}

pub fn comet_at(i: usize, t_secs: f32) -> Comet {
    let idx = i as u32 ^ 0xC0_4E70;
    let sun = sun_position();
    // Elipse de verdad: perihelio dentro de la órbita terrestre, afelio más allá
    // de Neptuno. Por eso pasan, se encienden y se van.
    let peri = 1_200_000.0 + hash(idx, 0x31) * 2_000_000.0;
    let aphe = 8_000_000.0 + hash(idx, 0x59) * 6_000_000.0;
    let a = (peri + aphe) * 0.5;
    let b = (peri * aphe).sqrt();
    let period = 3_600.0 + hash(idx, 0x71) * 5_400.0;
    let phase = hash(idx, 0x17) * TAU + t_secs / period * TAU;
    // Rotación del eje mayor, para que no compartan todos la misma línea.
    let rot = hash(idx, 0x8f) * TAU;
    let (ex, ez) = ((phase.cos() * a) - (a - peri), phase.sin() * b);
    let (cr, sr) = (rot.cos(), rot.sin());
    let x = sun[0] + ex * cr - ez * sr;
    let z = sun[2] + ex * sr + ez * cr;
    let d = ((x - sun[0]).powi(2) + (z - sun[2]).powi(2)).sqrt().max(1.0);
    let inv = |v: f32| 1.0 / v.max(1e-6);
    Comet {
        pos: [x, sun[1], z],
        radius_m: 900.0 + hash(idx, 0xb7) * 1_400.0,
        tail_dir: [(x - sun[0]) * inv(d), 0.0, (z - sun[2]) * inv(d)],
        // Se enciende al acercarse: 1 en el perihelio, ~0 pasado el doble.
        activity: (1.0 - (d - peri) / (peri * 2.0)).clamp(0.0, 1.0),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dist_to_sun(p: [f32; 3]) -> f32 {
        let s = sun_position();
        ((p[0] - s[0]).powi(2) + (p[2] - s[2]).powi(2)).sqrt()
    }

    #[test]
    fn belt_stays_in_its_lane_and_moves() {
        for i in 0..ASTEROID_COUNT {
            let a0 = asteroid_at(i, 0.0);
            let d = dist_to_sun(a0.pos);
            assert!(
                d > BELT_INNER_M * 0.85 && d < BELT_OUTER_M * 1.15,
                "asteroide {i} fuera del cinturon: {d}"
            );
            assert!(a0.radius_m > 0.0);
            let a1 = asteroid_at(i, 600.0);
            let moved = ((a1.pos[0] - a0.pos[0]).powi(2) + (a1.pos[2] - a0.pos[2]).powi(2)).sqrt();
            assert!(moved > 1_000.0, "asteroide {i} congelado");
        }
    }

    #[test]
    fn comet_tail_points_away_from_the_sun() {
        for i in 0..COMET_COUNT {
            for step in 0..12 {
                let c = comet_at(i, step as f32 * 300.0);
                let s = sun_position();
                let to = [c.pos[0] - s[0], c.pos[2] - s[2]];
                let len = (to[0] * to[0] + to[1] * to[1]).sqrt().max(1e-6);
                let dot = (to[0] / len) * c.tail_dir[0] + (to[1] / len) * c.tail_dir[2];
                assert!(dot > 0.99, "cometa {i} con la cola torcida: {dot}");
                assert!((0.0..=1.0).contains(&c.activity));
            }
        }
    }

    #[test]
    fn comets_actually_swing_in_and_out() {
        // Una elipse de verdad: la distancia al Sol tiene que variar mucho a lo
        // largo de una vuelta, si no es un círculo disfrazado.
        for i in 0..COMET_COUNT {
            let mut lo = f32::MAX;
            let mut hi: f32 = 0.0;
            for step in 0..200 {
                let d = dist_to_sun(comet_at(i, step as f32 * 60.0).pos);
                lo = lo.min(d);
                hi = hi.max(d);
            }
            assert!(hi > lo * 2.0, "cometa {i} sin elipse: {lo}..{hi}");
        }
    }
}
