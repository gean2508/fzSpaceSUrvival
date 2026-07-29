//! Avistamientos por cuerpo — lo que una CÁMARA (colocada en un punto) puede "ver" en
//! remoto: los seres vivos (animales/residentes, NPCs) con su posición real en el mundo.
//! Productores: `fauna.rs` y `society.rs` lo actualizan periódicamente (barato). Consumidor:
//! `camera_feed.rs`. Así la cámara muestra lo REAL cerca de su punto, sin renderizar un
//! segundo mundo. Ver `PLAN_CAMARA_SONDA.md`.

use std::collections::HashMap;

use bevy::prelude::*;

/// Un ser vivo avistado: posición mundo `(x, z)` + color + si es hostil (para el tinte).
#[derive(Clone, Copy)]
pub struct Sighting {
    pub x: f32,
    pub z: f32,
    pub color: [f32; 3],
    pub hostile: bool,
}

/// Avistamientos por cuerpo, en buckets separados por categoría para que fauna y sociedad
/// no se pisen. La cámara los fusiona al mostrar un punto.
#[derive(Resource, Default)]
pub struct WorldSightings {
    pub fauna: HashMap<u8, Vec<Sighting>>,
    pub npc: HashMap<u8, Vec<Sighting>>,
}

impl WorldSightings {
    /// Seres cerca de `(cx, cz)` en `body` (radio `r`), de ambas categorías.
    pub fn near(&self, body: u8, cx: f32, cz: f32, r: f32) -> Vec<Sighting> {
        let r2 = r * r;
        let mut out = Vec::new();
        for map in [&self.fauna, &self.npc] {
            if let Some(v) = map.get(&body) {
                for s in v {
                    if (s.x - cx) * (s.x - cx) + (s.z - cz) * (s.z - cz) <= r2 {
                        out.push(*s);
                    }
                }
            }
        }
        out
    }
}
