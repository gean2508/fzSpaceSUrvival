//! Assets grandes colocados por el jugador (`biblia.md` §5).
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
    active_surface_body, render_xy, surface_region_active, ActivePlanet, TileWorld,
};

/// Radio (tiles) alrededor del jugador donde las estructuras existen como entidad.
const STRUCT_STREAM_RADIUS: i64 = 60;

/// Una estructura colocada (datos compartidos con el editor de mundo). `(cx,cz)` =
/// esquina MIN de la base `w×h`; `solid` = celdas reales de colisión (huella A1),
/// vacío = base completa → colisión DINÁMICA por asset.
pub use ha_shared::world_edits::PlacedStructure;

/// Registro persistente de estructuras colocadas por el jugador.
///
/// **Las celdas sólidas viven en un set, no se recalculan.** `blocks()` la
/// consultan la colisión del jugador, la de la fauna y el ghost de construcción
/// varias veces por frame; barrer la lista entera y dentro de cada estructura su
/// huella (`solid`, hasta `w×h` pares) era un bucle anidado por consulta. La
/// huella no cambia nunca después de colocada, así que se expande UNA vez.
#[derive(Resource, Default)]
pub struct BuiltStructures {
    pub list: Vec<PlacedStructure>,
    /// (cuerpo, piso, tx, tz) de cada celda que bloquea.
    solid_cells: std::collections::HashSet<(u8, i32, i64, i64)>,
    dirty: bool,
    was_paused: bool,
    debounce: f32,
}

impl BuiltStructures {
    pub fn add(&mut self, s: PlacedStructure) {
        if self.list.contains(&s) {
            return;
        }
        self.index_cells(&s);
        self.list.push(s);
        self.dirty = true;
    }

    /// Saca la estructura anclada en `(cx,cz)` del cuerpo/piso y la devuelve.
    ///
    /// Reindexa el set de celdas sólidas ENTERO en vez de restar las suyas: dos
    /// estructuras pueden compartir una celda (huellas finas que se solapan en
    /// un borde) y restar a ciegas abriría un agujero de colisión en la vecina.
    /// Pasa una vez por objeto levantado, no por frame.
    pub fn remove_at(&mut self, body: CelestialBody, z: i32, cx: i64, cz: i64) -> Option<PlacedStructure> {
        let idx = self
            .list
            .iter()
            .position(|s| s.body == body as u8 && s.z == z && s.cx == cx && s.cz == cz)?;
        let removed = self.list.remove(idx);
        self.solid_cells.clear();
        let list = std::mem::take(&mut self.list);
        for s in &list {
            self.index_cells(s);
        }
        self.list = list;
        self.dirty = true;
        Some(removed)
    }

    /// Vuelca al set las celdas que bloquea una estructura: las de su huella
    /// pintada (A1) o, si no tiene, su base `w×h` entera.
    fn index_cells(&mut self, s: &PlacedStructure) {
        // La huella GIRA con el objeto: una cama de 1×2 puesta de costado ocupa
        // 2×1. Si no, el mueble se ve apoyado contra la pared y bloquea el paso
        // en la celda equivocada — colisión que no coincide con lo que se ve,
        // que es el bug que biblia §7 llama "pared invisible".
        let (w, h) = ha_shared::world_edits::footprint_rotated(s.w, s.h, s.rot);
        if s.solid.is_empty() {
            for dz in 0..h {
                for dx in 0..w {
                    self.solid_cells.insert((s.body, s.z, s.cx + dx, s.cz + dz));
                }
            }
        } else {
            for &(c, r) in &s.solid {
                let (dx, dz) = rotate_cell(c as i64, r as i64, s.w, s.h, s.rot);
                self.solid_cells.insert((s.body, s.z, s.cx + dx, s.cz + dz));
            }
        }
    }

    /// ¿La celda `(tx,tz)` cae en una celda SÓLIDA de alguna estructura del cuerpo/z?
    /// Usa las celdas reales de la huella (`solid`); si está vacía, la base completa
    /// `w×h`. Así la colisión es dinámica por asset.
    pub fn blocks(&self, body: CelestialBody, z: i32, tx: i64, tz: i64) -> bool {
        self.solid_cells.contains(&(body as u8, z, tx, tz))
    }
}

/// Celda `(dx, dz)` de una huella `w×h` después de girar el objeto `rot` cuartos
/// de vuelta. Es la rotación de toda la vida sobre la caja, y deja el resultado
/// otra vez con la esquina mínima en el origen.
fn rotate_cell(dx: i64, dz: i64, w: i64, h: i64, rot: u8) -> (i64, i64) {
    match rot % 4 {
        1 => (dz, w - 1 - dx),
        2 => (w - 1 - dx, h - 1 - dz),
        3 => (h - 1 - dz, dx),
        _ => (dx, dz),
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

/// Nombre humano del objeto colocado, para que el HUD diga "Manten para levantar
/// **Heladera**" y no "objeto". Sale del catálogo de colocables por su clave de
/// arte; si el arte no está en el catálogo (una estructura vieja, o una del
/// editor de mundo) queda la clave, que igual es más útil que nada.
#[derive(Component)]
pub struct StructureName(pub String);

/// **Todo lo colocado se puede tocar.** Empaqueta `(cx,cz)` en el `id` del
/// `InteractTarget`: es la clave con la que `pick_up_structure` vuelve a
/// encontrar la estructura en la lista. 16 bits por eje alcanzan de sobra —el
/// anillo de streaming son 60 tiles— y el desempate final lo hace el cuerpo/piso
/// activo, así que no hay ambigüedad.
fn pack_cell(cx: i64, cz: i64) -> u32 {
    (((cx as i32 as u32) & 0xFFFF) << 16) | ((cz as i32 as u32) & 0xFFFF)
}

fn unpack_cell(id: u32) -> (i64, i64) {
    let cx = ((id >> 16) & 0xFFFF) as u16 as i16 as i64;
    let cz = (id & 0xFFFF) as u16 as i16 as i64;
    (cx, cz)
}

pub struct StructuresPlugin;

impl Plugin for StructuresPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<BuiltStructures>()
            .add_systems(Startup, load_structures_on_startup)
            .add_systems(
                Update,
                (stream_structures, debounced_structures_save, pick_up_structure),
            );
    }
}

/// Nombre humano de un slot de arte, buscándolo en el catálogo de colocables.
fn art_label(art: &str) -> String {
    ha_shared::placeables::PLACEABLES
        .iter()
        .find(|a| a.art == art)
        .map(|a| a.label.to_string())
        .unwrap_or_else(|| art.to_string())
}

/// **Levantar lo que colocaste.** Mantener el dedo sobre un objeto colocado —el
/// mismo gesto que picar un bloque o levantar un cofre— lo saca del mundo y te
/// devuelve su material.
///
/// Por ahora vale para TODOS los colocables, que es lo pedido: más adelante el
/// editor va a decir por objeto si es interactuable y con qué acción, y este
/// sistema pasa a ser el caso "levantar" de esa tabla.
#[allow(clippy::too_many_arguments)]
fn pick_up_structure(
    mut hold: ResMut<crate::input::HoldInteractState>,
    target: Res<crate::targeting::LookTarget>,
    targets: Query<&crate::targeting::InteractTarget>,
    ctx: Res<WorldContextRuntime>,
    planet: Res<ActivePlanet>,
    tiles: Res<TileWorld>,
    editor: Res<crate::ui_editor::UiEditorState>,
    mut built: ResMut<BuiltStructures>,
    mut inv_ui: ResMut<crate::hud::inventory_ui::InventoryUiState>,
    mut inv: ResMut<crate::inventory_persist::PlayerInventory>,
    mut persist: ResMut<crate::inventory_persist::InventoryPersistState>,
) {
    if target.kind != Some(ha_shared::InteractTargetKind::Entity) || !hold.completed {
        return;
    }
    let Some(entity) = target.entity else { return };
    let Ok(t) = targets.get(entity) else { return };
    hold.completed = false;

    let body = active_surface_body(&ctx, &planet);
    let (cx, cz) = unpack_cell(t.id);
    let Some(s) = built.remove_at(body, tiles.z, cx, cz) else {
        return;
    };
    // Devolver el material. En creativo no se devuelve nada porque tampoco costó
    // nada: si no, colocar y levantar sería una fábrica de recursos.
    if !editor.creative {
        if let Some(a) = ha_shared::placeables::PLACEABLES.iter().find(|a| a.art == s.art) {
            crate::hud::inventory_ui::add_item_to_slots(&mut inv_ui, a.cost_item, a.cost_qty);
            crate::hud::inventory_ui::flush_slots_to_inventory(&inv_ui, &mut inv);
        }
    }
    persist.mark_dirty();
}

/// Sprite de la estructura: arte del slot a tamaño `w×h` tiles, o quad de color
/// (gris piedra) si el usuario aún no le asignó textura.
/// Sprite de la estructura **mirando hacia donde la pusiste**.
///
/// La orientación NO rota píxeles: en pixel art una cama girada 90° queda
/// tumbada de costado, con la perspectiva 3/4 rota. Se busca la pose del slot
/// (`sur`/`oeste`/`norte`/`este`, las que el usuario recorta en el editor igual
/// que las de las paredes) y, si no está, se dibuja el sprite plano de siempre.
/// Así girar un mueble mejora cuando hay arte y nunca empeora cuando no lo hay.
fn structure_sprite(art: &ArtLibrary, key: &str, rot: u8, size: Vec2) -> Sprite {
    let fallback = Color::srgb(0.42, 0.40, 0.38);
    let pose = ha_shared::world_edits::FACING_POSES[(rot % 4) as usize];
    if let Some(s) = art.pose_sprite(key, pose, size) {
        return s;
    }
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
        // El tamaño DIBUJADO sigue a la huella girada: un mueble de costado ocupa
        // el rectángulo transpuesto, y su pose ya viene dibujada así.
        let (fw, fh) = ha_shared::world_edits::footprint_rotated(s.w, s.h, s.rot);
        let size = Vec2::new(fw as f32 * TILE_M, fh as f32 * TILE_M);
        // **Se dibuja con la proporción del PIXEL, no con la de la huella.**
        // La huella es de tiles enteros —es lo que ocupa y lo que frena— y el
        // arte del pack viene en celdas de 32 px que valen 1,5 tiles: estirar el
        // sprite hasta la huella deformaba 2.713 de los 7.710 colocables (una
        // silla de 1×2 celdas, huella 2×2, salía con el doble de ancho). El
        // sprite se sigue anclando al PIE y centrado en la huella, así que lo que
        // frena y lo que se ve siguen coincidiendo por abajo.
        let pose_rot = ha_shared::world_edits::FACING_POSES[(s.rot % 4) as usize];
        let dibujo = art
            .draw_tiles(&s.art, Some(pose_rot))
            .map(|t| t * TILE_M)
            .unwrap_or(size);
        // Base al SUR (borde de mayor z = frente): el sprite se ancla al pie y se
        // eleva medio alto, Y-sorted por la fila del pie (como los árboles).
        let base_x = s.cx as f32 + fw as f32 * 0.5;
        let base_z = (s.cz + fh) as f32;
        let r = render_xy(base_x, base_z);
        // La CAPA del slot elige la banda (ver `crate::layers`): una plataforma o
        // un piso marcado `Floor` queda bajo los pies, un techo `Roof` se va
        // arriba y se transparenta al meterte debajo. Sin capa, Y-sort por la
        // fila del pie — el comportamiento de siempre.
        let layer = art.slot_layer(&s.art);
        let mut ent = commands.spawn((
            StructureSprite { cx: s.cx, cz: s.cz, z, body },
            structure_sprite(&art, &s.art, s.rot, dibujo),
            Transform::from_xyz(
                r.x,
                r.y + dibujo.y * 0.5,
                crate::layers::depth(layer, base_z * TILE_M),
            ),
            // Tocable: sin esto el objeto colocado no existe para el dedo — el
            // targeting sólo mira entidades con `InteractTarget`.
            crate::targeting::InteractTarget {
                kind: ha_shared::InteractTargetKind::Entity,
                id: pack_cell(s.cx, s.cz),
            },
            StructureName(art_label(&s.art)),
        ));
        if layer == Some(ha_assets::ArtLayer::Roof) {
            ent.insert(crate::layers::Overhead::techo((s.cx, s.cz), (fw, fh)));
        }
        // Casas/faroles/hogueras construidas: su luz (y su pose `Luces` — las
        // ventanas encendidas) sale del slot, no de código por tipo.
        crate::light2d::attach(&mut ent, &art, &[s.art.as_str()], dibujo);
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

fn load_structures_on_startup(mut built: ResMut<BuiltStructures>, art: Res<ArtLibrary>) {
    let Ok(raw) = std::fs::read_to_string(structures_path()) else {
        return;
    };
    let Ok(save) = serde_json::from_str::<StructuresSave>(&raw) else {
        return;
    };
    // **Lo que ya no tiene arte, no entra.** El catálogo se limpia de tanto en
    // tanto (`scripts/slots-colgados.py` sacó 168 claves que eran el mismo dibujo
    // con otro nombre), y una partida vieja puede tener colocada una de ésas. Sin
    // este filtro esa estructura se dibuja con el quad gris de respaldo: en el
    // Redmi aparecían rectángulos grises sueltos por todo el mundo, que es
    // exactamente lo que se ve cuando una clave se quedó sin pixel.
    //
    // Se DESCARTA en vez de dibujarse gris a propósito: un cuadrado gris en medio
    // del campo es peor que no tener nada, y el jugador puede volver a colocar la
    // pieza con la clave que sí existe.
    let mut perdidas = 0;
    for s in save.structures {
        if !art.has_art(&s.art) {
            perdidas += 1;
            continue;
        }
        // Por `add`, no asignando la lista: es lo único que llena el set de celdas
        // sólidas. Asignar `list` a secas dejaría todo lo colocado en partidas
        // anteriores dibujándose pero atravesable.
        built.add(s);
    }
    if perdidas > 0 {
        eprintln!("ha-mundo: {perdidas} estructuras de la partida usaban arte que ya no existe — se descartaron");
    }
    built.dirty = false; // cargar no es colocar
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

#[cfg(test)]
mod tests {
    use super::*;

    /// El `id` del `InteractTarget` tiene que devolver EXACTAMENTE la celda que
    /// se empaquetó, también al oeste/norte del origen: con coordenadas
    /// negativas un empaquetado ingenuo devuelve un número enorme y positivo, y
    /// entonces `remove_at` no encuentra nada — el objeto se ve tocable y no se
    /// levanta nunca. Media Tierra tiene coordenadas negativas.
    #[test]
    fn la_celda_vuelve_igual_de_como_se_empaqueto() {
        for &(cx, cz) in &[
            (0i64, 0i64),
            (5, 9),
            (-1, -1),
            (-320, 148),
            (2048, -2048),
            (i16::MAX as i64, i16::MIN as i64),
        ] {
            assert_eq!(unpack_cell(pack_cell(cx, cz)), (cx, cz), "celda ({cx},{cz})");
        }
    }

    /// Levantar un objeto lo saca de la lista Y de la colisión: si la celda
    /// siguiera marcada como sólida, quedaría un muro invisible donde estaba.
    #[test]
    fn levantar_libera_la_colision() {
        let mut built = BuiltStructures::default();
        built.add(PlacedStructure {
            body: CelestialBody::Earth as u8,
            z: 0,
            cx: -4,
            cz: 7,
            w: 2,
            h: 2,
            art: "furniture/cama".into(),
            solid: Vec::new(),
            rot: 0,
        });
        assert!(built.blocks(CelestialBody::Earth, 0, -3, 8));
        let sacada = built.remove_at(CelestialBody::Earth, 0, -4, 7);
        assert!(sacada.is_some(), "no encontro la estructura para levantarla");
        assert!(built.list.is_empty());
        assert!(
            !built.blocks(CelestialBody::Earth, 0, -3, 8),
            "quedo un muro invisible donde estaba el objeto"
        );
        assert!(built.remove_at(CelestialBody::Earth, 0, -4, 7).is_none());
    }
}
