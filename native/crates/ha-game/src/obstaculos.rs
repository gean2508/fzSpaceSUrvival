//! **Lo que frena a quien camina**, en un solo lugar: el jugador (`sim::tick_sim`) y los
//! vecinos (`society::tick_society`) chocan con el mismo mundo — celdas sólidas, estructuras
//! construidas, paredes de borde, decoración por metros, fachadas, cercas y puertas cerradas.
//!
//! Antes los NPC sólo miraban `TileWorld::solid_at`: atravesaban los muebles, las cercas, los
//! árboles y las puertas. Ahora una puerta cerrada los frena igual que a vos (y ellos la abren).

use bevy::ecs::system::SystemParam;
use bevy::prelude::*;
use ha_shared::CelestialBody;

use crate::world2d::TileWorld;

#[derive(SystemParam)]
pub struct Obstaculos<'w, 's> {
    tiles: Res<'w, TileWorld>,
    art: Res<'w, crate::art2d::ArtLibrary>,
    flora: Res<'w, crate::harvest::FloraHarvest>,
    authored: Res<'w, crate::world2d::AuthoredDecor>,
    structures: Res<'w, crate::structures::BuiltStructures>,
    walls: Res<'w, crate::walls::BuiltWalls>,
    fachadas: Query<'w, 's, &'static crate::building::SolidoDeFachada>,
    puertas: crate::puertas::PuertasQueFrenan<'w, 's>,
    autos: Res<'w, crate::vehiculos::AutosCaja>,
}

impl Obstaculos<'_, '_> {
    /// La capa vertical en la que se camina (0 = superficie).
    pub fn capa(&self) -> i32 {
        self.tiles.z
    }

    pub fn estructuras(&self) -> &crate::structures::BuiltStructures {
        &self.structures
    }

    pub fn tiles(&self) -> &TileWorld {
        &self.tiles
    }

    /// ¿La celda es una cerca? (frena por su pie, no por la celda entera)
    pub fn es_cerca(&self, body: CelestialBody, tx: i64, tz: i64) -> bool {
        self.tiles.z == 0 && self.tiles.tile_at(body, tx, tz) == ha_shared::topdown::Tile::Fence
    }

    /// Celda que frena por entera (pared, agua profunda, desnivel alto, estructura construida).
    pub fn solido(&self, body: CelestialBody, tx: i64, tz: i64) -> bool {
        let _p = crate::debug::probe("ob_solido");
        (self.tiles.walk_blocked(body, tx, tz) && !self.es_cerca(body, tx, tz))
            || self.structures.blocks(body, self.tiles.z, tx, tz)
    }

    /// Lo que frena por metros: paredes de borde, decoración, fachada, puertas cerradas, cercas.
    pub fn caja_frena(&self, body: CelestialBody, x: f32, z: f32, r: f32) -> bool {
        // Un auto es una pared con ruedas: los peatones no lo atraviesan (el jugador ya lo tenía).
        let choca_auto = self.autos.0.iter().any(|&(px, pz, rv)| (px - x).powi(2) + (pz - z).powi(2) < (rv + r).powi(2));
        if choca_auto {
            return true;
        }
        let caja = [x - r, z - r, x + r, z + r];
        let a = {
            let _p = crate::debug::probe("ob_paredes");
            ha_shared::build_grid::wall_capsule_hits(x, z, &|tx, tz, axis| self.walls.blocks(body, self.tiles.z, tx, tz, axis))
        };
        a || {
            let _p = crate::debug::probe("ob_decor");
            crate::world2d::decor_hits_box(&self.art, Some(&self.authored), Some(&self.flora), body, self.tiles.z, caja)
        } || {
            let _p = crate::debug::probe("ob_fachada");
            self.tiles.z == 0 && crate::building::fachada_frena(&self.fachadas, caja)
        } || {
            let _p = crate::debug::probe("ob_puertas");
            self.puertas.frena(caja)
        } || {
            let _p = crate::debug::probe("ob_cerca");
            crate::sim::cerca_frena(&|tx, tz| self.es_cerca(body, tx, tz), caja)
        }
    }

    /// ¿Se puede estar parado en `(x, z)` con un cuerpo de radio `r`? Es la pregunta de un
    /// vecino: no hay física de empuje, sólo sí o no.
    pub fn libre(&self, body: CelestialBody, x: f32, z: f32, r: f32) -> bool {
        for (cx, cz) in [(x - r, z - r), (x + r, z - r), (x - r, z + r), (x + r, z + r)] {
            if self.solido(body, cx.floor() as i64, cz.floor() as i64) {
                return false;
            }
        }
        !self.caja_frena(body, x, z, r)
    }

    /// ¿Lo que frena en `(x, z)` es una PUERTA cerrada? (para pedirle que se abra)
    pub fn puerta_cerrada_en(&self, x: f32, z: f32, r: f32) -> bool {
        self.puertas.frena([x - r, z - r, x + r, z + r])
    }
}
