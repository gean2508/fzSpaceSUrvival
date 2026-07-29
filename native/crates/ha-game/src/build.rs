//! 2D construction system — build mode, ghost preview, placement.
//!
//! **Modelo estilo Last Day on Earth (dos capas):**
//! - *Pisos* (y escaleras) ocupan la CELDA: se colocan libres en cualquier parte
//!   del mapa, la grilla es global (1 tile = 1 m).
//! - *Paredes / puertas / ventanas* ocupan el BORDE entre celdas: el dedo/cursor
//!   se engancha (`build_grid::nearest_edge`) a la arista más cercana de la
//!   celda apuntada, y la pieza sólo entra si hay piso construido de al menos un
//!   lado (`wall_supported`). Apuntar a un borde YA ocupado convierte el gesto
//!   en DEMOLER (el ghost se pone naranja) y devuelve el material.
//!
//! Las reglas puras viven en `ha_shared::build_grid`; el estado y los sprites de
//! las paredes en `crate::walls`.
//!
//! Pieces live in the bag's **CONSTRUIR** tab (`hud.rs`) — the screen stays
//! clean. Picking a piece there enters build mode and closes the bag. While
//! active, the aimed cell (`CursorHighlight`) becomes a placement slot: hold USE
//! / right-click to place the piece (spending its resource cost). A translucent
//! ghost shows validity (green = placeable, red = not enough materials). A small
//! on-screen chip is the ONLY persistent build UI — tap it (or pick a hotbar
//! item) to leave build mode. Removing a piece is just digging it (ha-shared
//! `Tile::dug_item` refunds the material). Desktop: `B` toggles build mode.

use bevy::prelude::*;

use ha_shared::build_grid::{
    edge_piece, edge_piece_count, is_build_floor, nearest_edge, prefab_edges, wall_supported,
    EdgePiece, WallAxis, WallKind, WALL_THICK,
};
use ha_shared::topdown::{
    build_piece, build_piece_count, large_asset, large_asset_count, prefab, prefab_cost,
    prefab_count, BuildPiece, LargeAsset, Prefab, Tile, TILE_M,
};

use crate::game_ui::GameUiRoot;
use crate::hud::{BagBuildPieceButton, BagBuildSection, BagPanelState, BagTab};
use crate::input::{BuildModeActive, HoldInteractState};
use crate::inventory_persist::{InventoryPersistState, PlayerInventory};
use crate::scene::{RegionRuntime, WorldContextRuntime};
use crate::targeting::LookTarget;
use crate::ui_theme::{self, UiButtonState, UiButtonTone};
use crate::world2d::{
    active_surface_body, render_xy, surface_region_active, ActivePlanet, TileWorld,
};

/// Currently selected build entry — combined palette: indices below
/// `build_piece_count()` are single pieces (`BUILD_PIECES`), the rest are
/// multi-tile prefabs (`PREFABS`). Body-agnostic: places wherever building is
/// allowed (Earth, Moon, Mars, stations).
#[derive(Resource, Default)]
pub struct SelectedBuildPiece(pub usize);

/// What the combined palette index resolves to.
pub enum BuildSelection {
    Piece(&'static BuildPiece),
    Prefab(&'static Prefab),
    /// Asset grande (A2): sprite único + footprint, colocado como estructura.
    Large(&'static LargeAsset),
    /// Pieza de BORDE (pared/puerta/ventana): se pega a la arista entre celdas.
    Edge(&'static EdgePiece),
}

/// Índice de paleta donde arrancan las piezas de borde.
pub fn edge_palette_base() -> usize {
    build_piece_count() + prefab_count() + large_asset_count()
}

/// Combined palette lookup: cell pieces, prefabs, large assets (A2), edges.
pub fn build_selection(idx: usize) -> Option<BuildSelection> {
    let pc = build_piece_count();
    let prc = prefab_count();
    let lc = large_asset_count();
    if idx < pc {
        return build_piece(idx).map(BuildSelection::Piece);
    }
    if idx < pc + prc {
        return prefab(idx - pc).map(BuildSelection::Prefab);
    }
    if idx < pc + prc + lc {
        return large_asset(idx - pc - prc).map(BuildSelection::Large);
    }
    edge_piece(idx - pc - prc - lc).map(BuildSelection::Edge)
}

/// Footprint min corner for a prefab aimed at cell `(tx, tz)` (centered).
fn prefab_min(p: &Prefab, tx: i64, tz: i64) -> (i64, i64) {
    (tx - p.w / 2, tz - p.h / 2)
}

#[derive(Resource, Default)]
struct BuildIndicatorSpawned(bool);

#[derive(Component)]
struct BuildIndicator;
#[derive(Component)]
struct BuildIndicatorLabel;
#[derive(Component)]
struct BuildGhost;

pub struct BuildPlugin;

impl Plugin for BuildPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<SelectedBuildPiece>()
            .init_resource::<BuildIndicatorSpawned>()
            .add_systems(Startup, spawn_build_ghost)
            .add_systems(
                Update,
                (
                    ensure_build_indicator,
                    desktop_build_hotkey,
                    build_tab_select,
                    sync_bag_build_section,
                    sync_build_indicator,
                    apply_build_hold.before(crate::hud::update_interact_hud),
                    sync_build_ghost,
                ),
            );
    }
}

/// `B` toggles build mode on desktop (mobile enters via the bag's CONSTRUIR tab).
fn desktop_build_hotkey(keys: Res<ButtonInput<KeyCode>>, mut build_mode: ResMut<BuildModeActive>) {
    if crate::platform::uses_touch_gameplay_ui() {
        return;
    }
    if keys.just_pressed(KeyCode::KeyB) {
        build_mode.active = !build_mode.active;
    }
}

/// Picking a piece in the bag's CONSTRUIR tab selects it, enters build mode and
/// closes the bag so you can place it right away.
fn build_tab_select(
    mut buttons: Query<(&Interaction, &BagBuildPieceButton, &mut BackgroundColor), Changed<Interaction>>,
    mut selected: ResMut<SelectedBuildPiece>,
    mut build_mode: ResMut<BuildModeActive>,
    mut bag: ResMut<BagPanelState>,
) {
    for (interaction, button, mut bg) in &mut buttons {
        if *interaction == Interaction::Pressed {
            selected.0 = button.0;
            build_mode.active = true;
            bag.open = false;
            *bg = BackgroundColor(ui_theme::button_bg(UiButtonTone::Craft, UiButtonState::Pressed));
        } else {
            *bg = BackgroundColor(ui_theme::button_bg(
                UiButtonTone::Craft,
                ui_theme::interaction_state(*interaction, false),
            ));
        }
    }
}

/// Show/hide the CONSTRUIR section with its tab (display toggle owned here).
fn sync_bag_build_section(
    bag: Res<BagPanelState>,
    mut sections: Query<&mut Node, With<BagBuildSection>>,
) {
    for mut node in &mut sections {
        node.display = if bag.tab == BagTab::Build {
            Display::Flex
        } else {
            Display::None
        };
    }
}

/// The single persistent build-mode chip — upper-left (clear of the joystick
/// zones), hidden unless building. Tapping it leaves build mode.
fn ensure_build_indicator(
    mut commands: Commands,
    layout: Res<crate::input::TouchLayout>,
    root: Query<Entity, With<GameUiRoot>>,
    mut spawned: ResMut<BuildIndicatorSpawned>,
) {
    if spawned.0 || layout.width < 100.0 {
        return;
    }
    let Ok(root) = root.get_single() else {
        return;
    };
    spawned.0 = true;
    commands.entity(root).with_children(|parent| {
        parent
            .spawn((
                Button,
                Node {
                    position_type: PositionType::Absolute,
                    left: Val::Px(8.0),
                    top: Val::Px(100.0),
                    padding: UiRect::axes(Val::Px(8.0), Val::Px(4.0)),
                    border: UiRect::all(Val::Px(2.0)),
                    justify_content: JustifyContent::Center,
                    align_items: AlignItems::Center,
                    ..default()
                },
                BorderColor(ui_theme::accent_cyan()),
                BackgroundColor(ui_theme::button_bg(UiButtonTone::Primary, UiButtonState::Pressed)),
                Visibility::Hidden,
                BuildIndicator,
                Name::new("BuildIndicator"),
            ))
            .with_children(|b| {
                b.spawn((
                    Text::new("CONSTRUIR (tocar: salir)"),
                    TextFont {
                        font_size: 13.0,
                        ..default()
                    },
                    TextColor(ui_theme::text_primary()),
                    BuildIndicatorLabel,
                ));
            });
    });
}

fn sync_build_indicator(
    mut build_mode: ResMut<BuildModeActive>,
    selected: Res<SelectedBuildPiece>,
    taps: Query<&Interaction, (With<BuildIndicator>, Changed<Interaction>)>,
    mut chips: Query<&mut Visibility, With<BuildIndicator>>,
    mut label: Query<&mut Text, With<BuildIndicatorLabel>>,
) {
    // Tap the chip → leave build mode.
    for interaction in &taps {
        if *interaction == Interaction::Pressed {
            build_mode.active = false;
        }
    }
    let want = if build_mode.active {
        Visibility::Inherited
    } else {
        Visibility::Hidden
    };
    for mut v in &mut chips {
        if *v != want {
            *v = want;
        }
    }
    if build_mode.active {
        if let Ok(mut text) = label.get_single_mut() {
            let is_spawn = selected.0 >= crate::fauna::SPAWN_PALETTE_BASE;
            let name = if is_spawn {
                crate::fauna::spawn_species_label(selected.0 - crate::fauna::SPAWN_PALETTE_BASE)
            } else {
                match build_selection(selected.0) {
                    Some(BuildSelection::Piece(p)) => p.label,
                    Some(BuildSelection::Prefab(p)) => p.label,
                    Some(BuildSelection::Large(a)) => a.label,
                    Some(BuildSelection::Edge(p)) => p.label,
                    None => "",
                }
            };
            let verb = if is_spawn { "SPAWN" } else { "CONSTRUIR" };
            let line = format!("{verb}: {name}  (tocar: salir)");
            if text.0 != line {
                text.0 = line;
            }
        }
    }
}

/// Escritores de eventos de colocación creativa (fauna/comida/cámara) agrupados en un
/// `SystemParam` para no pasar del límite de 16 parámetros de sistema de Bevy.
#[derive(bevy::ecs::system::SystemParam)]
struct CreativeSpawnWriters<'w> {
    resident: EventWriter<'w, crate::fauna::SpawnResidentEvent>,
    food: EventWriter<'w, crate::fauna::SpawnFoodEvent>,
    camera: EventWriter<'w, crate::camera_feed::PlaceCameraEvent>,
}

/// Todo lo que la colocación ESCRIBE, junto (mismo motivo: límite de params).
#[derive(bevy::ecs::system::SystemParam)]
struct BuildTargets<'w> {
    tiles: ResMut<'w, TileWorld>,
    roofs: ResMut<'w, crate::building::BuiltRoofs>,
    structures: ResMut<'w, crate::structures::BuiltStructures>,
    walls: ResMut<'w, crate::walls::BuiltWalls>,
}

/// **Snapping de pared**: borde de la cuadrícula al que se engancha la mira.
/// Sólo dentro del alcance de construcción — lo garantiza el propio targeting,
/// que sólo publica `place_cell`/`block_cell` dentro de `TILE_REACH`.
fn aimed_edge(
    cursor: &crate::input::AimCursor,
    target: &LookTarget,
) -> Option<(i64, i64, WallAxis)> {
    if target.place_cell.is_none() && target.block_cell.is_none() {
        return None;
    }
    Some(nearest_edge(cursor.world.x, cursor.world.y))
}

/// Consumes a completed build hold: places the selected piece at the aimed cell,
/// spending its resource cost. Mirrors `try_place_tile_on_hold` for blocks.
fn apply_build_hold(
    mut hold: ResMut<HoldInteractState>,
    target: Res<LookTarget>,
    selected: Res<SelectedBuildPiece>,
    ctx: Res<WorldContextRuntime>,
    planet: Res<ActivePlanet>,
    editor: Res<crate::ui_editor::UiEditorState>,
    flora: Res<crate::harvest::FloraHarvest>,
    art: Res<crate::art2d::ArtLibrary>,
    cursor: Res<crate::input::AimCursor>,
    mut world: BuildTargets,
    mut inv: ResMut<PlayerInventory>,
    mut persist: ResMut<InventoryPersistState>,
    mut sfx: EventWriter<crate::audio::PlaySfx>,
    mut spawners: CreativeSpawnWriters,
) {
    if !hold.completed || !hold.placing_build {
        return;
    }
    hold.completed = false;
    hold.placing_build = false;
    // Las piezas de BORDE no usan la celda: se enganchan a la arista apuntada.
    if let Some(BuildSelection::Edge(piece)) = build_selection(selected.0) {
        apply_edge_build(
            piece,
            selected.0 - edge_palette_base(),
            &cursor,
            &target,
            &ctx,
            &planet,
            editor.creative,
            &mut world,
            &mut inv,
            &mut persist,
            &mut sfx,
        );
        return;
    }
    let Some((tx, tz, _)) = target.place_cell else {
        return;
    };
    // VIVARIO (F1): índices ≥ SPAWN_PALETTE_BASE son fauna residente (creativo), no
    // piezas — spawnea en el centro de la celda apuntada. Ver PLAN_VIVARIO_*.
    if selected.0 >= crate::fauna::SPAWN_PALETTE_BASE {
        if editor.creative {
            let sub = selected.0 - crate::fauna::SPAWN_PALETTE_BASE;
            let (x, z) = (tx as f32 + 0.5, tz as f32 + 0.5);
            if sub == crate::fauna::FEED_PALETTE_SUB {
                spawners.food.send(crate::fauna::SpawnFoodEvent { x, z }); // F8: feeder
            } else if sub == crate::fauna::CAMERA_PALETTE_SUB {
                // F7: dejar una cámara de vigilancia en este punto del cuerpo actual.
                let cbody = active_surface_body(&ctx, &planet) as u8;
                spawners
                    .camera
                    .send(crate::camera_feed::PlaceCameraEvent { body: cbody, x, z });
            } else if sub == crate::fauna::WATER_PALETTE_SUB {
                // F8: abrevadero — coloca un tile de agua.
                let body = active_surface_body(&ctx, &planet);
                world.tiles.place_tile(body, tx as i64, tz as i64, Tile::Water);
            } else if sub == crate::fauna::PREDATOR_PALETTE_SUB {
                // F8: depredador — residente forzado hostil (acecha).
                spawners.resident.send(crate::fauna::SpawnResidentEvent {
                    x,
                    z,
                    species: 0,
                    hostile: true,
                });
            } else {
                spawners.resident.send(crate::fauna::SpawnResidentEvent {
                    x,
                    z,
                    species: sub,
                    hostile: false,
                });
            }
            sfx.send(crate::audio::PlaySfx(crate::audio::Sfx::Place));
        }
        return;
    }
    let Some(selection) = build_selection(selected.0) else {
        return;
    };
    // Creative = Minecraft-style: build free, no resource cost.
    let creative = editor.creative;
    let body = active_surface_body(&ctx, &planet);
    let z = world.tiles.z;
    match selection {
        BuildSelection::Piece(piece) => {
            if !creative && inv.qty(piece.cost_item) < piece.cost_qty {
                return;
            }
            // No se construye sobre decor cosechable (árbol/roca/mineral): talá o
            // picá primero.
            if crate::world2d::cell_build_blocked(&flora, body, z, tx as i64, tz as i64) {
                return;
            }
            if world.tiles.place_tile(body, tx as i64, tz as i64, piece.tile) {
                sfx.send(crate::audio::PlaySfx(crate::audio::Sfx::Place));
                if !creative {
                    inv.remove(piece.cost_item, piece.cost_qty);
                }
                persist.mark_immediate();
            }
        }
        // Prefab: stamp del área COMPLETA de una (ghost ya mostró el footprint).
        // Piso en TODAS las celdas + paredes en el perímetro (bordes), con su
        // puerta: la habitación queda caminable de pared a pared, como en LDoE.
        BuildSelection::Prefab(p) => {
            let qty = prefab_cost(p);
            if !creative && inv.qty(p.cost_item) < qty {
                return;
            }
            let (min_x, min_z) = prefab_min(p, tx as i64, tz as i64);
            // Validar TODO el footprint antes de gastar: ninguna celda sólida NI
            // con decor cosechable (no se pone un cuarto sobre un árbol/roca).
            for dz in 0..p.h {
                for dx in 0..p.w {
                    if world.tiles.solid_at(body, min_x + dx, min_z + dz)
                        || crate::world2d::cell_build_blocked(
                            &flora,
                            body,
                            z,
                            min_x + dx,
                            min_z + dz,
                        )
                    {
                        return;
                    }
                }
            }
            for dz in 0..p.h {
                for dx in 0..p.w {
                    world.tiles.place_tile(body, min_x + dx, min_z + dz, Tile::WoodFloor);
                }
            }
            let wall_piece = edge_piece_of_kind(WallKind::Wall, p.cost_item);
            let door_piece = edge_piece_of_kind(WallKind::Door, p.cost_item);
            for (ex, ez, axis, kind) in prefab_edges(p, min_x, min_z) {
                let piece = if kind == WallKind::Door { door_piece } else { wall_piece };
                world.walls.add(crate::walls::PlacedWall {
                    body: body as u8,
                    z,
                    tx: ex,
                    tz: ez,
                    axis,
                    piece,
                });
            }
            sfx.send(crate::audio::PlaySfx(crate::audio::Sfx::Stamp));
            if !creative {
                inv.remove(p.cost_item, qty);
            }
            // Techo del prefab → sistema de techo translúcido (F4).
            world.roofs.add(crate::building::RoofRect {
                body: body as u8,
                z,
                min: (min_x, min_z),
                w: p.w,
                h: p.h,
                art: Some(room_art_key(p)),
            });
            persist.mark_immediate();
        }
        // Asset grande (A2): registra una estructura-sprite + su base sólida.
        // Centrado en la celda apuntada, como un prefab.
        BuildSelection::Large(a) => {
            if !creative && inv.qty(a.cost_item) < a.cost_qty {
                return;
            }
            let (min_x, min_z) = (tx as i64 - a.w / 2, tz as i64 - a.h / 2);
            // Footprint despejado: nada sólido, ni decor cosechable, ni otra
            // estructura (no se apilan).
            for dz in 0..a.h {
                for dx in 0..a.w {
                    let (cx, cz) = (min_x + dx, min_z + dz);
                    if world.tiles.solid_at(body, cx, cz)
                        || crate::world2d::cell_build_blocked(&flora, body, z, cx, cz)
                        || world.structures.blocks(body, z, cx, cz)
                    {
                        return;
                    }
                }
            }
            // Colisión DINÁMICA: si el asset tiene huella pintada (A1), guardamos sus
            // celdas reales (sólo eso bloquea); si no, base completa (`solid` vacío).
            let solid = art
                .collision_footprint(&[a.art])
                .map(|fp| {
                    fp.solid
                        .iter()
                        .copied()
                        .filter(|&(c, r)| (c as i64) < a.w && (r as i64) < a.h)
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default();
            world.structures.add(crate::structures::PlacedStructure {
                body: body as u8,
                z,
                cx: min_x,
                cz: min_z,
                w: a.w,
                h: a.h,
                art: a.art.to_string(),
                solid,
            });
            sfx.send(crate::audio::PlaySfx(crate::audio::Sfx::Stamp));
            if !creative {
                inv.remove(a.cost_item, a.cost_qty);
            }
            persist.mark_immediate();
        }
        // Ya resuelto arriba (usa borde, no celda).
        BuildSelection::Edge(_) => {}
    }
}

/// Índice de paleta de la pieza de borde de ese tipo hecha del material pedido
/// (madera/piedra); cae a la primera de ese tipo. Lo usan los prefabs para
/// levantar sus paredes con el mismo material que cobran.
fn edge_piece_of_kind(kind: WallKind, cost_item: &str) -> usize {
    (0..edge_piece_count())
        .find(|&i| {
            let p = edge_piece(i).unwrap();
            p.kind == kind && p.cost_item == cost_item
        })
        .or_else(|| (0..edge_piece_count()).find(|&i| edge_piece(i).unwrap().kind == kind))
        .unwrap_or(0)
}

/// **Colocación / demolición de una pieza de BORDE.** Reglas (LDoE):
/// 1. Si el borde apuntado ya tiene algo → lo demuele y devuelve el material.
/// 2. Si no, exige piso construido de al menos un lado (`wall_supported`) y
///    material suficiente; en creativo se saltea ambas cosas.
#[allow(clippy::too_many_arguments)]
fn apply_edge_build(
    piece: &'static EdgePiece,
    piece_idx: usize,
    cursor: &crate::input::AimCursor,
    target: &LookTarget,
    ctx: &WorldContextRuntime,
    planet: &ActivePlanet,
    creative: bool,
    world: &mut BuildTargets,
    inv: &mut PlayerInventory,
    persist: &mut InventoryPersistState,
    sfx: &mut EventWriter<crate::audio::PlaySfx>,
) {
    let Some((tx, tz, axis)) = aimed_edge(cursor, target) else {
        return;
    };
    let body = active_surface_body(ctx, planet);
    let z = world.tiles.z;
    // 1. Demoler lo que haya en ese borde (devuelve el material).
    if let Some(old) = world.walls.remove(body, z, tx, tz, axis) {
        if !creative {
            let def = old.def();
            inv.add(def.cost_item, def.cost_qty);
        }
        sfx.send(crate::audio::PlaySfx(crate::audio::Sfx::Dig));
        persist.mark_immediate();
        return;
    }
    // 2. Colocar.
    if !creative {
        if inv.qty(piece.cost_item) < piece.cost_qty {
            return;
        }
        let has_floor = |cx: i64, cz: i64| is_build_floor(world.tiles.tile_at(body, cx, cz));
        if !wall_supported(&has_floor, tx, tz, axis) {
            return;
        }
    }
    world.walls.add(crate::walls::PlacedWall {
        body: body as u8,
        z,
        tx,
        tz,
        axis,
        piece: piece_idx,
    });
    sfx.send(crate::audio::PlaySfx(crate::audio::Sfx::Place));
    if !creative {
        inv.remove(piece.cost_item, piece.cost_qty);
    }
    persist.mark_immediate();
}

/// Slot de arte `room/*` del prefab (el usuario recorta la pose Techo ahí).
fn room_art_key(p: &Prefab) -> &'static str {
    match p.id {
        "room_small" => "room/room_small",
        "room_medium" => "room/room_medium",
        "room_large" => "room/room_large",
        _ => "room/casa_1",
    }
}

fn spawn_build_ghost(mut commands: Commands) {
    commands.spawn((
        BuildGhost,
        Sprite::from_color(Color::srgba(0.25, 1.0, 0.35, 0.6), Vec2::splat(1.0)),
        Transform::from_xyz(0.0, 0.0, 5.7),
        Visibility::Hidden,
    ));
}

#[allow(clippy::too_many_arguments)]
fn sync_build_ghost(
    build_mode: Res<BuildModeActive>,
    region: Res<RegionRuntime>,
    ctx: Res<WorldContextRuntime>,
    planet: Res<ActivePlanet>,
    target: Res<LookTarget>,
    cursor: Res<crate::input::AimCursor>,
    selected: Res<SelectedBuildPiece>,
    inv: Res<PlayerInventory>,
    tiles: Res<TileWorld>,
    flora: Res<crate::harvest::FloraHarvest>,
    structures: Res<crate::structures::BuiltStructures>,
    walls: Res<crate::walls::BuiltWalls>,
    mut ghost: Query<(&mut Transform, &mut Visibility, &mut Sprite), With<BuildGhost>>,
) {
    let Ok((mut tf, mut vis, mut sprite)) = ghost.get_single_mut() else {
        return;
    };
    let placeable = build_mode.active && surface_region_active(&region).is_some();
    let Some(selection) = build_selection(selected.0).filter(|_| placeable) else {
        *vis = Visibility::Hidden;
        return;
    };
    let body = active_surface_body(&ctx, &planet);
    // Pieza de BORDE: el ghost es la propia arista (fina), verde si entra,
    // naranja si ahí ya hay algo (el gesto pasa a DEMOLER), roja si no se puede.
    if let BuildSelection::Edge(piece) = selection {
        let Some((ex, ez, axis)) = aimed_edge(&cursor, &target) else {
            *vis = Visibility::Hidden;
            return;
        };
        *vis = Visibility::Visible;
        let demolish = walls.at(body, tiles.z, ex, ez, axis).is_some();
        let has_floor = |cx: i64, cz: i64| is_build_floor(tiles.tile_at(body, cx, cz));
        let ok = inv.qty(piece.cost_item) >= piece.cost_qty
            && wall_supported(&has_floor, ex, ez, axis);
        sprite.color = if demolish {
            Color::srgba(1.0, 0.62, 0.12, 0.7)
        } else if ok {
            Color::srgba(0.25, 1.0, 0.35, 0.7)
        } else {
            Color::srgba(1.0, 0.25, 0.2, 0.6)
        };
        sprite.custom_size = Some(match axis {
            WallAxis::H => Vec2::new(TILE_M, WALL_THICK),
            WallAxis::V => Vec2::new(WALL_THICK, TILE_M),
        });
        let (cx, cz) = ha_shared::build_grid::edge_center(ex, ez, axis);
        let r = render_xy(cx, cz);
        tf.translation = Vec3::new(r.x, r.y, 5.7);
        return;
    }
    let Some((tx, tz, _)) = target.place_cell else {
        *vis = Visibility::Hidden;
        return;
    };
    *vis = Visibility::Visible;
    // Una celda bloquea si es sólida o tiene decor cosechable (talar/picar antes).
    let blocked = |cx: i64, cz: i64| {
        tiles.solid_at(body, cx, cz)
            || crate::world2d::cell_build_blocked(&flora, body, tiles.z, cx, cz)
            || structures.blocks(body, tiles.z, cx, cz)
    };
    // Pieza: celda 1×1. Prefab: ghost del footprint COMPLETO w×h, centrado en la
    // celda apuntada. `ok` = materiales suficientes Y todo el footprint despejado.
    let (ok, size, center) = match selection {
        BuildSelection::Piece(piece) => (
            (inv.qty(piece.cost_item) >= piece.cost_qty) && !blocked(tx as i64, tz as i64),
            Vec2::splat(1.0),
            (tx as f32 + 0.5, tz as f32 + 0.5),
        ),
        BuildSelection::Prefab(p) => {
            let (min_x, min_z) = prefab_min(p, tx as i64, tz as i64);
            let clear = (0..p.h).all(|dz| (0..p.w).all(|dx| !blocked(min_x + dx, min_z + dz)));
            (
                (inv.qty(p.cost_item) >= prefab_cost(p)) && clear,
                Vec2::new(p.w as f32 * TILE_M, p.h as f32 * TILE_M),
                (
                    min_x as f32 + p.w as f32 * 0.5,
                    min_z as f32 + p.h as f32 * 0.5,
                ),
            )
        }
        BuildSelection::Large(a) => {
            let (min_x, min_z) = (tx as i64 - a.w / 2, tz as i64 - a.h / 2);
            let clear = (0..a.h).all(|dz| (0..a.w).all(|dx| !blocked(min_x + dx, min_z + dz)));
            (
                (inv.qty(a.cost_item) >= a.cost_qty) && clear,
                Vec2::new(a.w as f32 * TILE_M, a.h as f32 * TILE_M),
                (min_x as f32 + a.w as f32 * 0.5, min_z as f32 + a.h as f32 * 0.5),
            )
        }
        // Resuelto arriba (usa borde, no celda).
        BuildSelection::Edge(_) => return,
    };
    // Bright green = placeable, red = not enough materials OR footprint obstructed.
    sprite.color = if ok {
        Color::srgba(0.25, 1.0, 0.35, 0.6)
    } else {
        Color::srgba(1.0, 0.25, 0.2, 0.55)
    };
    sprite.custom_size = Some(size);
    let r = render_xy(center.0, center.1);
    tf.translation = Vec3::new(r.x, r.y, 5.7);
}
