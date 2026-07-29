//! Assets grandes colocados por el jugador (A2, `PLAN_MAESTRO §3.A2`).
//!
//! Un "asset grande" (catálogo `ha_shared::topdown::LARGE_ASSETS`) es un sprite
//! ÚNICO con footprint de colisión que el jugador coloca como una casa y RODEA
//! (Y-sort), a diferencia de un edificio (en el que entra). Este módulo:
//! - persiste lo colocado en `structures.json` (flush-on-pause, como roofs),
//! - lo streamea como sprites cerca del jugador (base al suelo, Y-sorted),
//! - expone la colisión de su BASE (`BuiltStructures::blocks`) para que el sim y
//!   la construcción no dejen caminar/edificar encima.
//!
//! Colisión = base completa `w×h`. La huella fina per-celda (arcos caminables,
//! etc.) es un refinamiento futuro sobre la sección "🗺 huella" del editor.

use bevy::prelude::*;
use serde::{Deserialize, Serialize};

use ha_shared::topdown::TILE_M;
use ha_shared::CelestialBody;

use crate::art2d::ArtLibrary;
use crate::inventory_persist::persist_dir;
use crate::platform::is_app_paused;
use crate::scene::{RegionRuntime, WorldContextRuntime};
use crate::sim::Player;
use crate::world2d::{
    active_surface_body, render_xy, sort_z, surface_region_active, ActivePlanet, TileWorld,
};

/// Radio (tiles) alrededor del jugador donde las estructuras existen como entidad.
const STRUCT_STREAM_RADIUS: i64 = 60;

/// Una estructura colocada (datos compartidos con el editor de mundo). `(cx,cz)` =
/// esquina MIN de la base `w×h`; `solid` = celdas reales de colisión (huella A1),
/// vacío = base completa → colisión DINÁMICA por asset.
pub use ha_shared::world_edits::PlacedStructure;

/// Registro persistente de estructuras colocadas por el jugador.
#[derive(Resource, Default)]
pub struct BuiltStructures {
    pub list: Vec<PlacedStructure>,
    dirty: bool,
    was_paused: bool,
    debounce: f32,
}

impl BuiltStructures {
    pub fn add(&mut self, s: PlacedStructure) {
        if !self.list.contains(&s) {
            self.list.push(s);
            self.dirty = true;
        }
    }

    /// ¿La celda `(tx,tz)` cae en una celda SÓLIDA de alguna estructura del cuerpo/z?
    /// Usa las celdas reales de la huella (`solid`); si está vacía, la base completa
    /// `w×h`. Así la colisión es dinámica por asset. Barato (pocas estructuras).
    pub fn blocks(&self, body: CelestialBody, z: i32, tx: i64, tz: i64) -> bool {
        self.list.iter().any(|s| {
            if s.body != body as u8 || s.z != z {
                return false;
            }
            let (dx, dz) = (tx - s.cx, tz - s.cz);
            if dx < 0 || dz < 0 || dx >= s.w || dz >= s.h {
                return false; // fuera de la base
            }
            if s.solid.is_empty() {
                true // sin huella pintada → base completa
            } else {
                s.solid.iter().any(|&(c, r)| c as i64 == dx && r as i64 == dz)
            }
        })
    }
}

/// Entidad de estructura viva (para el despawn por streaming).
#[derive(Component)]
struct StructureSprite {
    cx: i64,
    cz: i64,
    z: i32,
    body: CelestialBody,
}

pub struct StructuresPlugin;

impl Plugin for StructuresPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<BuiltStructures>()
            .add_systems(Startup, load_structures_on_startup)
            .add_systems(Update, (stream_structures, debounced_structures_save));
    }
}

/// Sprite de la estructura: arte del slot a tamaño `w×h` tiles, o quad de color
/// (gris piedra) si el usuario aún no le asignó textura.
fn structure_sprite(art: &ArtLibrary, key: &str, size: Vec2) -> Sprite {
    let fallback = Color::srgb(0.42, 0.40, 0.38);
    art.sprite_or_color(key, fallback, size)
}

fn stream_structures(
    mut commands: Commands,
    region: Res<RegionRuntime>,
    ctx: Res<WorldContextRuntime>,
    planet: Res<ActivePlanet>,
    tiles: Res<TileWorld>,
    art: Res<ArtLibrary>,
    built: Res<BuiltStructures>,
    player: Query<&Transform, With<Player>>,
    live: Query<(Entity, &StructureSprite)>,
) {
    if surface_region_active(&region).is_none() {
        for (e, _) in &live {
            commands.entity(e).despawn_recursive();
        }
        return;
    }
    let Ok(player_tf) = player.get_single() else {
        return;
    };
    let body = active_surface_body(&ctx, &planet);
    let z = tiles.z;
    let ptx = (player_tf.translation.x / TILE_M).floor() as i64;
    let ptz = (-player_tf.translation.y / TILE_M).floor() as i64;

    // Estructuras deseadas (cerca, mismo cuerpo/capa).
    let want: Vec<&PlacedStructure> = built
        .list
        .iter()
        .filter(|s| {
            s.body == body as u8
                && s.z == z
                && (s.cx + s.w / 2 - ptx).abs() <= STRUCT_STREAM_RADIUS
                && (s.cz + s.h / 2 - ptz).abs() <= STRUCT_STREAM_RADIUS
        })
        .collect();

    // Despawn de las que ya no aplican.
    let mut alive: Vec<(i64, i64)> = Vec::new();
    for (e, st) in &live {
        let keep = st.body == body
            && st.z == z
            && want.iter().any(|w| w.cx == st.cx && w.cz == st.cz);
        if keep {
            alive.push((st.cx, st.cz));
        } else {
            commands.entity(e).despawn_recursive();
        }
    }
    // Spawn de las que faltan.
    for s in want {
        if alive.contains(&(s.cx, s.cz)) {
            continue;
        }
        let size = Vec2::new(s.w as f32 * TILE_M, s.h as f32 * TILE_M);
        // Base al SUR (borde de mayor z = frente): el sprite se ancla al pie y se
        // eleva medio alto, Y-sorted por la fila del pie (como los árboles).
        let base_x = s.cx as f32 + s.w as f32 * 0.5;
        let base_z = (s.cz + s.h) as f32;
        let r = render_xy(base_x, base_z);
        let mut ent = commands.spawn((
            StructureSprite { cx: s.cx, cz: s.cz, z, body },
            structure_sprite(&art, &s.art, size),
            Transform::from_xyz(r.x, r.y + size.y * 0.5, sort_z(base_z)),
        ));
        // Casas/faroles/hogueras construidas: su luz (y su pose `Luces` — las
        // ventanas encendidas) sale del slot, no de código por tipo.
        crate::light2d::attach(&mut ent, &art, &[s.art.as_str()], size);
    }
}

// --- Persistencia (mirror de roofs.json) -------------------------------------

#[derive(Serialize, Deserialize, Default)]
struct StructuresSave {
    structures: Vec<PlacedStructure>,
}

fn structures_path() -> std::path::PathBuf {
    persist_dir().join("structures.json")
}

fn load_structures_on_startup(mut built: ResMut<BuiltStructures>) {
    let Ok(raw) = std::fs::read_to_string(structures_path()) else {
        return;
    };
    let Ok(save) = serde_json::from_str::<StructuresSave>(&raw) else {
        return;
    };
    built.list = save.structures;
}

fn debounced_structures_save(time: Res<Time>, mut built: ResMut<BuiltStructures>) {
    let paused = is_app_paused();
    let flush_now = paused && !built.was_paused && built.dirty;
    built.was_paused = paused;
    if !built.dirty {
        return;
    }
    built.debounce += time.delta_secs();
    if !flush_now && built.debounce < 20.0 {
        return;
    }
    built.debounce = 0.0;
    built.dirty = false;
    let save = StructuresSave { structures: built.list.clone() };
    if let Ok(json) = serde_json::to_string(&save) {
        let _ = std::fs::write(structures_path(), json);
    }
}
