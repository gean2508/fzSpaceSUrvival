//! Editor de Mundo — H0: capa de OVERRIDES autorados (`world_edits.json`).
//!
//! El mundo sigue siendo procedural y determinista; este archivo es la capa que
//! el usuario AUTORA (en el futuro editor de mundo, fases H1+) y que el juego
//! aplica ENCIMA de la generación en boot. Es la tercera herramienta data-driven
//! (junto a `manifest.json` de arte y `audio.json` de sonido). Formatos:
//! - `tiles`: overrides de tile puntuales (una cascada entre dos lagos, etc.).
//! - `structures`: assets grandes autorados (reusa `PlacedStructure` de A2).
//! Ideas futuras (H3): `decor`, `buildings`, `spawns`, `regions`, `city_templates`.
//!
//! Embebido al compilar (`include_str!`) → cero assets nuevos en el APK. Si falta
//! o está roto → **mundo 100 % procedural** (nunca rompe). Lo autorado GANA sobre
//! la generación donde exista.

use bevy::prelude::*;

use ha_shared::world_edits::WorldEdits;

use crate::structures::BuiltStructures;
use crate::world2d::TileWorld;

/// Contenido embebido (lo escribe el editor de mundo). Default vacío = sin overrides.
const WORLD_EDITS_JSON: &str = include_str!("../assets/world_edits.json");

/// Parsea la capa autorada embebida; si está rota → vacía (100% procedural).
fn load_embedded_world_edits() -> WorldEdits {
    match serde_json::from_str::<WorldEdits>(WORLD_EDITS_JSON) {
        Ok(w) => w,
        Err(e) => {
            eprintln!("ha-world: world_edits.json invalido ({e}) — mundo 100% procedural");
            WorldEdits::default()
        }
    }
}

pub struct WorldEditsPlugin;

impl Plugin for WorldEditsPlugin {
    fn build(&self, app: &mut App) {
        // Se aplica una vez, después de que TileWorld/BuiltStructures existan.
        app.add_systems(Startup, apply_world_edits);
    }
}

fn apply_world_edits(
    mut tiles: ResMut<TileWorld>,
    mut structures: ResMut<BuiltStructures>,
    mut authored: ResMut<crate::world2d::AuthoredDecor>,
    mut decor_free: ResMut<crate::world2d::DecorFreeCells>,
) {
    let edits = load_embedded_world_edits();
    let (mut nt, mut ns) = (0usize, 0usize);
    for t in &edits.tiles {
        if let Some(body) = crate::world2d::body_from_u8(t.body) {
            // `author_tile` y no `place_tile`: respeta la CAPA del edit, pisa
            // celdas sólidas (reemplazar roca por pasto es válido al diseñar) y
            // no programa rebrote de césped, que deshacía el diseño al minuto.
            tiles.author_tile(body, t.z, t.x, t.zc, t.tile);
            nt += 1;
        }
    }
    for s in &edits.structures {
        structures.add(s.clone());
        ns += 1;
    }
    // Celdas donde el decor procedural no debe generarse: las que pintaste
    // (tu terreno reemplaza al mundo) + las que borraste con la goma de decor.
    let free: std::collections::HashSet<(u8, i32, i64, i64)> = edits.decor_free_cells().collect();
    let nf = free.len();
    decor_free.set(free);
    // Decor autorado (árboles/props colocados en el editor) → recurso que el streaming pinta.
    let nd = edits.decor.len();
    authored.0 = edits.decor;
    if nt > 0 || ns > 0 || nd > 0 {
        eprintln!(
            "ha-world: autorado aplicado — {nt} tiles, {ns} estructuras, {nd} decor, \
             {nf} celdas sin decor procedural"
        );
    }
}
