//! Sensor COMPARTIDO del cerebro de fauna (FASE 8 / B4). El gym (que evoluciona el
//! genoma) y el juego (que lo ejecuta) DEBEN codificar los sensores igual, o la
//! conducta aprendida no transfiere. Esta es la fuente única de verdad.
//!
//! Sensor de 32 bits: obstáculos relativos al rumbo + **BIOMA** (el entorno afecta
//! la conducta) + dirección a la comida más cercana + estado interno. Determinista.

use core::f32::consts::TAU;

use crate::celestial_bodies::CelestialBody;
use crate::topdown::{topdown_tile, Tile};

/// **Versión del código de sensores.** Hay que subirla cada vez que cambie el significado de los
/// bits de `sense()` (o de `Campo::sense_bits` en el juego). Un genoma entrenado con otros bits
/// lee basura y, como la locomoción es determinista, no se cae: sólo se mueve peor, en silencio.
/// Eso le pasó al dron (`~/Documents/mods/dron`, `SENSORES_VERSION`). El gym escribe esta cifra
/// junto al genoma (`FAUNA.adn.sensores`) y una prueba de `ha-game` exige que coincidan.
pub const SENSE_VERSION: u32 = 1;

pub fn solid_at(body: CelestialBody, x: f32, z: f32) -> bool {
    topdown_tile(body, 0, x.floor() as i64, z.floor() as i64).solid()
}

/// Bioma bajo las patas (2 bits): pasto=0, arena=1, agua=2, otro=3.
pub fn biome_at(body: CelestialBody, x: f32, z: f32) -> u32 {
    match topdown_tile(body, 0, x.floor() as i64, z.floor() as i64) {
        Tile::Grass => 0,
        Tile::Sand => 1,
        Tile::Water => 2,
        _ => 3,
    }
}

fn d2(ax: f32, az: f32, bx: f32, bz: f32) -> f32 {
    let (dx, dz) = (ax - bx, az - bz);
    dx * dx + dz * dz
}

/// Dirección al objeto más cercano de `pts`, en sector de 8 (relativa al rumbo),
/// y si está muy cerca. Devuelve `(sector, cerca)`.
fn nearest_dir(x: f32, z: f32, heading: f32, pts: &[(f32, f32)], near_d2: f32) -> Option<(u32, bool)> {
    pts.iter()
        .filter(|p| (p.0 - x).abs() + (p.1 - z).abs() > 1e-4) // excluir uno mismo
        .min_by(|a, b| d2(x, z, a.0, a.1).partial_cmp(&d2(x, z, b.0, b.1)).unwrap())
        .map(|&(px, pz)| {
            let ang = (pz - z).atan2(px - x) - heading;
            let sector = (((ang.rem_euclid(TAU)) / (TAU / 8.0)) as u32) & 7;
            (sector, d2(x, z, px, pz) < near_d2)
        })
}

/// Codifica la percepción del animal en 32 bits (idéntico en gym y juego). Incluye
/// **sensado SOCIAL** (vecinos, bits 10-13) para que puedan surgir dinámicas de
/// grupo/territorio/conflicto por sí solas — no se recompensa "hacé sociedad", se
/// da el canal y la evolución en el mundo decide qué hacer con él.
pub fn sense(
    body: CelestialBody,
    x: f32,
    z: f32,
    heading: f32,
    energy: f32,
    foods: &[(f32, f32)],
    neighbors: &[(f32, f32)],
) -> u32 {
    let (cx, sz) = (heading.cos(), heading.sin());
    let mut s = 0u32;
    if solid_at(body, x + cx, z + sz) {
        s |= 1; // pared al frente
    }
    if solid_at(body, x - sz, z + cx) {
        s |= 2; // izquierda
    }
    if solid_at(body, x + sz, z - cx) {
        s |= 4; // derecha
    }
    s |= biome_at(body, x, z) << 3; // bits 3-4: bioma
    if let Some((sector, near)) = nearest_dir(x, z, heading, foods, 2.5) {
        s |= sector << 5; // bits 5-7: dir a la comida
        if near {
            s |= 1 << 8; // comida muy cerca
        }
    }
    if energy < 0.3 {
        s |= 1 << 9; // poca energía
    }
    // SOCIAL: dirección al vecino más cercano + "hay alguien muy cerca".
    if let Some((sector, near)) = nearest_dir(x, z, heading, neighbors, 4.0) {
        s |= sector << 10; // bits 10-12: dir al vecino
        if near {
            s |= 1 << 13; // vecino muy cerca (aglomeración/contacto)
        }
    }
    s
}
