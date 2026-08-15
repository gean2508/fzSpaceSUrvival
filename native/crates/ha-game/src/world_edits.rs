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
    art: Res<crate::art2d::ArtLibrary>,
) {
    let edits = load_embedded_world_edits();
    // **LAS ZONAS, ANTES QUE NADA.** No estampan nada: le cambian el carácter a
    // las manzanas que cubren, y el generador de ciudad las consulta desde dentro
    // (`city::district_at`). Tienen que estar puestas antes de que se genere el
    // primer chunk, o el pedazo de mundo que ya se generó saldría con el reparto
    // viejo y el resto con el nuevo.
    if !edits.zonas.is_empty() {
        eprintln!("ha-zonas: {} zonas autoradas", edits.zonas.len());
    }
    ha_shared::zonas::instalar(ha_shared::zonas::Zonas::new(edits.zonas.clone()));
    let (mut nt, mut ns, mut ng) = (0usize, 0usize, 0usize);
    // SOLAR DE LOS ASSETS COLOCADOS, PRIMERO. Un asset con `ground` reemplaza el
    // terreno de toda su huella, así que plantar una casa deja de ser "dibujar un
    // sprite encima del bosque": el suelo se adapta al tamaño del asset. Va por la
    // misma vía que los tiles pintados (`author_tile`), de modo que un solar
    // autorado y uno pintado son la misma cosa para la colisión, el decor y la
    // persistencia — y va ANTES para que un camino que pintaste a mano sobre el
    // solar gane sobre él, que es el orden en que lo dibujaste.
    for t in edits.stamped_ground() {
        if let Some(body) = crate::world2d::body_from_u8(t.body) {
            tiles.author_tile(body, t.z, t.x, t.zc, t.tile);
            ng += 1;
        }
    }
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
        // Lo que se quedó sin pixel no entra: dibujaría un quad gris (ver el
        // mismo filtro en `structures::load_structures_on_startup`).
        if !art.has_art(&s.art) {
            continue;
        }
        structures.add(s.clone());
        ns += 1;
    }
    // Celdas donde el decor procedural no debe generarse: las que pintaste
    // (tu terreno reemplaza al mundo) + las que borraste con la goma de decor.
    let free: std::collections::HashSet<(u8, i32, i64, i64)> = edits.decor_free_cells().collect();
    let nf = free.len();
    decor_free.set(free);
    // Decor autorado (árboles/props colocados en el editor) → recurso que el streaming pinta.
    let decor: Vec<_> = edits
        .decor
        .into_iter()
        .filter(|d| art.has_art(&d.key))
        .collect();
    let nd = decor.len();
    authored.set(decor);
    if nt > 0 || ns > 0 || nd > 0 {
        eprintln!(
            "ha-world: autorado aplicado — {nt} tiles, {ng} celdas de solar, {ns} estructuras, \
             {nd} decor, {nf} celdas sin decor procedural"
        );
    }
}
