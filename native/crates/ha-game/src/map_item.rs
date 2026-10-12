//! **El mapa es un objeto, no una pantalla del menú.**
//!
//! Antes el minimapa vivía dentro del menú: en el teléfono quedaba en un recuadro
//! de 200 px que no se veía, y para mirarlo había que salir del juego. Ahora es
//! contenido: se **craftea** (`map_small` / `map_large`, ver `recipes.rs`), se
//! equipa desde la hotbar y se abre en la mano — con tu posición marcada, como el
//! mapa de Minecraft. Manteniendo USE lo **colgás** donde estés parado (pared,
//! mesa, lo que sea): el mapa colgado guarda el SECTOR donde lo pusiste, así que
//! recorrer el mundo y colgar uno por sector arma, pared mediante, el mapa grande
//! de todo el juego.
//!
//! El dibujo no son cientos de nodos: es **una textura** rasterizada del terreno
//! (un píxel por celda muestreada), la misma para la mano y para la pared. Por eso
//! cuesta lo mismo un mapa chico que uno grande y se puede refrescar mientras
//! caminás sin tocar el layout de la UI.

use std::collections::HashMap;

use bevy::prelude::*;
use bevy::render::render_asset::RenderAssetUsages;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};
use ha_shared::CelestialBody;
use serde::{Deserialize, Serialize};

use crate::game_ui::{GameUiRoot, GameplayUi};
use crate::input::{EquippedItem, HoldInteractState, TouchLayout};
use crate::inventory_persist::{persist_dir, InventoryPersistState, PlayerInventory};
use crate::sim::{Player, SimState};
use crate::ui_theme;
use crate::world2d::{render_xy, sort_z, TileWorld};

/// Ids de ítem. El chico cubre tu valle; el grande, la región.
pub const MAP_SMALL: &str = "map_small";
pub const MAP_LARGE: &str = "map_large";

/// ¿Este ítem es un mapa? (y si lo es, ¿es el grande?)
pub fn map_kind(item_id: &str) -> Option<bool> {
    match item_id {
        MAP_SMALL => Some(false),
        MAP_LARGE => Some(true),
        _ => None,
    }
}

/// Celdas de mundo que cubre un mapa de lado a lado. El chico es "lo que caminás
/// en un rato"; el grande, cuatro veces eso — y como los sectores del mapa colgado
/// se alinean a este número, dos mapas grandes vecinos encajan sin huecos.
pub fn map_span(large: bool) -> i64 {
    if large {
        256
    } else {
        64
    }
}

/// Lado en píxeles de la textura. 96 es más que suficiente: el mapa chico queda a
/// 1.5 px por celda (se ve el sendero entre los árboles) y el grande a 1 px cada
/// 2.7 celdas (se ve la costa, que es lo que un mapa grande tiene que contar).
const MAP_PX: u32 = 96;

/// Cada cuánto se vuelve a rasterizar el mapa que llevás en la mano (s). Es un
/// muestreo de 9216 celdas: a 3 Hz ni se nota, y el mapa "respira" mientras
/// caminás en vez de quedar congelado.
const HAND_REFRESH_S: f32 = 0.35;

pub struct MapItemPlugin;

impl Plugin for MapItemPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<HeldMapUi>()
            .init_resource::<PlacedMapWorld>()
            .add_systems(Startup, load_placed_maps)
            .add_systems(
                Update,
                (
                    held_map_system,
                    place_map_on_hold,
                    sync_placed_map_visuals.after(place_map_on_hold),
                    debounced_maps_save,
                ),
            );
    }
}

// ── El dibujo ────────────────────────────────────────────────────────────────

/// Rasteriza el terreno alrededor de `(cx, cz)` en una textura cuadrada: un píxel
/// por celda muestreada, con el color que el propio tile declara (el mismo que usa
/// el mundo, así el mapa y el suelo nunca se contradicen) y la decoración —
/// árboles, rocas— oscureciendo su celda, que es lo que hace que un claro se lea
/// como claro.
///
/// El borde se pinta como marco: sin él, un mapa colgado en una pared oscura no se
/// distingue del muro.
fn rasterize_map(tiles: &TileWorld, body: CelestialBody, cx: i64, cz: i64, span: i64) -> Image {
    let n = MAP_PX as i64;
    let step = (span / n).max(1);
    let half = span / 2;
    let mut data = vec![0u8; (MAP_PX * MAP_PX * 4) as usize];
    let surface = tiles.z == 0;
    for py in 0..n {
        for px in 0..n {
            // Pantalla arriba = −z, igual que la vista del juego.
            let tx = cx - half + px * step;
            let tz = cz - half + py * step;
            let mut rgb = tiles.tile_at(body, tx, tz).color();
            if surface && crate::world2d::surface_decor_at(body, tx, tz).is_some() {
                rgb = [rgb[0] * 0.62, rgb[1] * 0.62, rgb[2] * 0.62];
            }
            // Marco de pergamino en el borde.
            let edge = px < 2 || py < 2 || px >= n - 2 || py >= n - 2;
            if edge {
                rgb = [0.42, 0.32, 0.18];
            }
            let i = (((py * n + px) * 4) as usize).min(data.len().saturating_sub(4));
            data[i] = (rgb[0].clamp(0.0, 1.0) * 255.0) as u8;
            data[i + 1] = (rgb[1].clamp(0.0, 1.0) * 255.0) as u8;
            data[i + 2] = (rgb[2].clamp(0.0, 1.0) * 255.0) as u8;
            data[i + 3] = 255;
        }
    }
    Image::new(
        Extent3d { width: MAP_PX, height: MAP_PX, depth_or_array_layers: 1 },
        TextureDimension::D2,
        data,
        TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::default(),
    )
}

/// Sector (índice de celda dividido por el lado del mapa) de una posición. Es lo
/// que hace que dos mapas colgados encajen: cada uno cubre un sector entero y los
/// sectores son contiguos por construcción.
fn sector_of(x: f32, z: f32, span: i64) -> (i64, i64) {
    (
        (x.floor() as i64).div_euclid(span),
        (z.floor() as i64).div_euclid(span),
    )
}

/// Centro (en celdas) de un sector.
fn sector_center(sector: (i64, i64), span: i64) -> (i64, i64) {
    (sector.0 * span + span / 2, sector.1 * span + span / 2)
}

// ── El mapa en la mano ───────────────────────────────────────────────────────

#[derive(Component)]
struct HeldMapRoot;
#[derive(Component)]
struct HeldMapImage;
#[derive(Component)]
struct HeldMapLabel;

#[derive(Resource, Default)]
struct HeldMapUi {
    root: Option<Entity>,
    image: Option<Handle<Image>>,
    timer: f32,
    /// Última celda central dibujada: si no te moviste, no se rasteriza de nuevo.
    last: Option<(i64, i64, bool)>,
}

/// Abre/cierra el mapa según lo que lleves en la mano y lo mantiene al día.
#[allow(clippy::too_many_arguments)]
fn held_map_system(
    mut commands: Commands,
    time: Res<Time>,
    equipped: Res<EquippedItem>,
    layout: Res<TouchLayout>,
    tiles: Res<TileWorld>,
    world_ctx: Res<crate::scene::WorldContextRuntime>,
    planet: Res<crate::world2d::ActivePlanet>,
    region: Res<crate::scene::RegionRuntime>,
    bag: Res<crate::hud::BagPanelState>,
    mut images: ResMut<Assets<Image>>,
    mut ui: ResMut<HeldMapUi>,
    root: Query<Entity, With<GameUiRoot>>,
    player: Query<&SimState, With<Player>>,
    mut vis: Query<&mut Visibility, With<HeldMapRoot>>,
    mut image_nodes: Query<&mut ImageNode, With<HeldMapImage>>,
    mut labels: Query<&mut Text, With<HeldMapLabel>>,
) {
    let large = map_kind(&equipped.item_id);
    // Con la mochila abierta el mapa estorba (y ya estás mirando otra cosa).
    let show = large.is_some()
        && !bag.open
        && crate::world2d::surface_region_active(&region).is_some();
    if !show {
        for mut v in &mut vis {
            if *v != Visibility::Hidden {
                *v = Visibility::Hidden;
            }
        }
        return;
    }
    let Some(large) = large else { return };
    let Ok(sim) = player.get_single() else { return };
    let body = crate::world2d::active_surface_body(&world_ctx, &planet);

    // Panel: cuadrado, arriba a la izquierda del jugador y por encima de la
    // hotbar. El tamaño sale del lado corto para que en el Redmi ocupe lo que
    // ocupa un mapa en la mano y no media pantalla.
    let short = layout.width.min(layout.height);
    let size = (short * 0.40).clamp(140.0, 300.0);
    if ui.root.is_none() {
        let Ok(root_e) = root.get_single() else { return };
        let handle = images.add(rasterize_map(&tiles, body, 0, 0, map_span(large)));
        ui.image = Some(handle.clone());
        let mut spawned = Entity::PLACEHOLDER;
        commands.entity(root_e).with_children(|p| {
            spawned = p
                .spawn((
                    Node {
                        position_type: PositionType::Absolute,
                        // **Arriba al centro**, que es el único hueco libre del
                        // HUD: a la izquierda están los corazones, a la derecha el
                        // reloj, abajo los joysticks y USE/JUMP — puesto ahí, el
                        // mapa tapaba el botón de saltar. Y el centro quedó libre
                        // justo porque el MENU se mudó adentro de la mochila.
                        left: Val::Px(layout.width * 0.5 - size * 0.5),
                        top: Val::Px(layout.insets.top as f32 + 8.0),
                        width: Val::Px(size),
                        height: Val::Px(size),
                        padding: UiRect::all(Val::Px(5.0)),
                        border: UiRect::all(Val::Px(2.0)),
                        flex_direction: FlexDirection::Column,
                        align_items: AlignItems::Center,
                        row_gap: Val::Px(3.0),
                        ..default()
                    },
                    BorderRadius::all(Val::Px(10.0)),
                    BorderColor(ui_theme::accent_ink()),
                    BackgroundColor(ui_theme::panel_bg(0.92)),
                    HeldMapRoot,
                    GameplayUi,
                    Name::new("HeldMap"),
                ))
                .with_children(|panel| {
                    panel
                        .spawn((
                            Node {
                                width: Val::Percent(100.0),
                                flex_grow: 1.0,
                                justify_content: JustifyContent::Center,
                                align_items: AlignItems::Center,
                                overflow: Overflow::clip(),
                                ..default()
                            },
                            ImageNode::new(handle.clone()),
                            HeldMapImage,
                        ))
                        .with_children(|map| {
                            // **Estás vos, en el centro.** El mapa se re-dibuja
                            // centrado en el jugador, así que el marcador no se
                            // mueve: lo que se mueve es el mundo debajo, que es
                            // exactamente cómo se lee un mapa en la mano.
                            map.spawn((
                                Node {
                                    position_type: PositionType::Absolute,
                                    left: Val::Percent(50.0),
                                    top: Val::Percent(50.0),
                                    width: Val::Px(9.0),
                                    height: Val::Px(9.0),
                                    margin: UiRect::all(Val::Px(-4.5)),
                                    border: UiRect::all(Val::Px(2.0)),
                                    ..default()
                                },
                                BorderRadius::MAX,
                                BorderColor(Color::WHITE),
                                BackgroundColor(Color::srgb(1.0, 0.18, 0.62)),
                            ));
                        });
                    panel.spawn((
                        Text::new(""),
                        TextFont { font_size: 10.0, ..default() },
                        TextColor(ui_theme::text_muted()),
                        HeldMapLabel,
                    ));
                })
                .id();
        });
        ui.root = Some(spawned);
        ui.timer = 0.0;
        ui.last = None;
        return;
    }
    for mut v in &mut vis {
        if *v != Visibility::Inherited {
            *v = Visibility::Inherited;
        }
    }

    ui.timer -= time.delta_secs();
    let cx = sim.body.x.floor() as i64;
    let cz = sim.body.z.floor() as i64;
    let span = map_span(large);
    let step = (span / MAP_PX as i64).max(1);
    let moved = ui
        .last
        .map(|(lx, lz, l)| l != large || (cx - lx).abs() >= step || (cz - lz).abs() >= step)
        .unwrap_or(true);
    if ui.timer <= 0.0 && moved {
        ui.timer = HAND_REFRESH_S;
        ui.last = Some((cx, cz, large));
        let handle = images.add(rasterize_map(&tiles, body, cx, cz, span));
        ui.image = Some(handle.clone());
        for mut node in &mut image_nodes {
            node.image = handle.clone();
        }
        let sector = sector_of(sim.body.x, sim.body.z, span);
        for mut text in &mut labels {
            // Corto a propósito: el renglón entero tiene que entrar bajo el mapa
            // sin cortarse (antes decía "X … Z … sector …" y se comía el borde).
            text.0 = format!("{cx},{cz} - sector {},{}", sector.0, sector.1);
        }
    }
}

// ── El mapa colgado ──────────────────────────────────────────────────────────

/// Un mapa puesto en el mundo. Guarda el SECTOR que retrata (no la posición del
/// jugador): por eso dos mapas colgados uno al lado del otro muestran regiones
/// contiguas y no la misma vista dos veces.
#[derive(Clone, Serialize, Deserialize)]
pub struct PlacedMap {
    pub uid: u32,
    pub body: CelestialBody,
    pub x: f32,
    pub z: f32,
    pub large: bool,
    pub sector: (i64, i64),
}

#[derive(Resource, Default)]
pub struct PlacedMapWorld {
    next_uid: u32,
    pub placed: Vec<PlacedMap>,
    dirty: bool,
    debounce: f32,
    /// Texturas ya rasterizadas por (cuerpo, sector, tamaño): un mapa colgado se
    /// dibuja UNA vez — lo que retrata no cambia aunque camines.
    cache: HashMap<(i32, i64, i64, bool), Handle<Image>>,
}

/// El sprite de un mapa colgado. `pub` porque el driver de QA
/// (`debug/autodrive.rs`) cuenta estas entidades: "está en la lista" y "se
/// dibuja" son dos cosas distintas, y sin contar sprites el test sólo probaba la
/// primera.
#[derive(Component)]
pub struct PlacedMapEntity {
    pub uid: u32,
}

/// Colgar el mapa: mantenerlo tocado con uno equipado. Como el cofre o la
/// antorcha, se cuelga en el punto que tocaste.
#[allow(clippy::too_many_arguments)]
fn place_map_on_hold(
    mut hold: ResMut<HoldInteractState>,
    equipped: Res<EquippedItem>,
    world_ctx: Res<crate::scene::WorldContextRuntime>,
    planet: Res<crate::world2d::ActivePlanet>,
    region: Res<crate::scene::RegionRuntime>,
    editor: Res<crate::ui_editor::UiEditorState>,
    cursor: Res<crate::input::AimCursor>,
    player: Query<&SimState, With<Player>>,
    mut world: ResMut<PlacedMapWorld>,
    mut inv: ResMut<PlayerInventory>,
    mut inv_ui: ResMut<crate::hud::inventory_ui::InventoryUiState>,
    mut persist: ResMut<InventoryPersistState>,
) {
    if !hold.completed || !hold.placing_map {
        return;
    }
    hold.completed = false;
    hold.placing_map = false;
    let Some(large) = map_kind(&equipped.item_id) else { return };
    if crate::world2d::surface_region_active(&region).is_none() {
        return;
    }
    let Ok(sim) = player.get_single() else { return };
    if !editor.creative && inv.qty(&equipped.item_id) == 0 {
        return;
    }
    let body = crate::world2d::active_surface_body(&world_ctx, &planet);
    let span = map_span(large);
    // Se cuelga donde TOCASTE (la pared/mesa que elegiste con el dedo) y retrata
    // el sector donde estás parado.
    let (x, z) = crate::input::place_point(&cursor, sim, 1.2);
    let uid = world.next_uid;
    world.next_uid += 1;
    world.placed.push(PlacedMap {
        uid,
        body,
        x,
        z,
        large,
        sector: sector_of(sim.body.x, sim.body.z, span),
    });
    world.dirty = true;

    if !editor.creative {
        crate::hud::inventory_ui::remove_from_slots(&mut inv_ui, &equipped.item_id, 1);
        crate::hud::inventory_ui::flush_slots_to_inventory(&inv_ui, &mut inv);
        persist.mark_immediate();
    }
}

/// Mantiene un sprite por mapa colgado del cuerpo actual (y despawnea los del
/// resto). Su textura sale del caché: se rasteriza una vez por sector.
fn sync_placed_map_visuals(
    mut commands: Commands,
    tiles: Res<TileWorld>,
    world_ctx: Res<crate::scene::WorldContextRuntime>,
    planet: Res<crate::world2d::ActivePlanet>,
    region: Res<crate::scene::RegionRuntime>,
    mut images: ResMut<Assets<Image>>,
    mut world: ResMut<PlacedMapWorld>,
    existing: Query<(Entity, &PlacedMapEntity)>,
) {
    let on_surface = crate::world2d::surface_region_active(&region).is_some() && tiles.z == 0;
    let body = crate::world2d::active_surface_body(&world_ctx, &planet);
    let mut alive: Vec<u32> = Vec::new();
    if on_surface {
        alive = world
            .placed
            .iter()
            .filter(|m| m.body == body)
            .map(|m| m.uid)
            .collect();
    }
    for (entity, marker) in &existing {
        if !alive.contains(&marker.uid) {
            commands.entity(entity).despawn_recursive();
        }
    }
    if !on_surface {
        return;
    }
    let have: Vec<u32> = existing.iter().map(|(_, m)| m.uid).collect();
    let todo: Vec<PlacedMap> = world
        .placed
        .iter()
        .filter(|m| m.body == body && !have.contains(&m.uid))
        .cloned()
        .collect();
    for m in todo {
        let key = (m.body as i32, m.sector.0, m.sector.1, m.large);
        let handle = match world.cache.get(&key) {
            Some(h) => h.clone(),
            None => {
                let span = map_span(m.large);
                let (cx, cz) = sector_center(m.sector, span);
                let h = images.add(rasterize_map(&tiles, m.body, cx, cz, span));
                world.cache.insert(key, h.clone());
                h
            }
        };
        // El grande se ve grande: colgado en la pared ocupa el doble.
        let side = if m.large { 2.8 } else { 1.6 };
        let mut sprite = Sprite::from_image(handle);
        sprite.custom_size = Some(Vec2::splat(side));
        let r = render_xy(m.x, m.z);
        commands.spawn((
            sprite,
            crate::sim::UnlitSprite,
            Transform::from_xyz(r.x, r.y + side * 0.35, sort_z(m.z)),
            PlacedMapEntity { uid: m.uid },
            Name::new(format!("PlacedMap_{}", m.uid)),
        ));
    }
}

// ── Persistencia ─────────────────────────────────────────────────────────────

fn maps_path() -> std::path::PathBuf {
    persist_dir().join("maps.json")
}

fn load_placed_maps(mut world: ResMut<PlacedMapWorld>) {
    let Ok(text) = std::fs::read_to_string(maps_path()) else { return };
    let Ok(list) = serde_json::from_str::<Vec<PlacedMap>>(&text) else { return };
    world.next_uid = list.iter().map(|m| m.uid + 1).max().unwrap_or(0);
    world.placed = list;
}

/// Mismo patrón que cofres y máquinas: se guarda con retardo tras un cambio (y no
/// una vez por frame), así colgar cinco mapas seguidos escribe una sola vez.
fn debounced_maps_save(time: Res<Time>, mut world: ResMut<PlacedMapWorld>) {
    if !world.dirty {
        return;
    }
    world.debounce += time.delta_secs();
    if world.debounce < 1.5 && !crate::platform::is_app_paused() {
        return;
    }
    world.debounce = 0.0;
    world.dirty = false;
    if let Ok(text) = serde_json::to_string(&world.placed) {
        let _ = std::fs::create_dir_all(persist_dir());
        let _ = std::fs::write(maps_path(), text);
    }
}
