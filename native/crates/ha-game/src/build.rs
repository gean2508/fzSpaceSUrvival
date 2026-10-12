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
    ceiling_piece, ceiling_piece_count, ceiling_supported, edge_piece, edge_piece_count,
    is_build_floor, nearest_edge, prefab_edges, wall_supported, CeilingPiece, EdgePiece, WallAxis,
    WallKind, WALL_THICK,
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
    /// TAPA (tumbado o techo modular): ocupa la celda, pero por encima.
    Ceiling(&'static CeilingPiece),
}

/// Índice de paleta donde arrancan las piezas de borde.
pub fn edge_palette_base() -> usize {
    build_piece_count() + prefab_count() + large_asset_count()
}

/// Índice de paleta donde arrancan las tapas (techos y tumbados).
pub fn ceiling_palette_base() -> usize {
    edge_palette_base() + edge_piece_count()
}

/// Combined palette lookup: cell pieces, prefabs, large assets (A2), edges, tapas.
pub fn build_selection(idx: usize) -> Option<BuildSelection> {
    let pc = build_piece_count();
    let prc = prefab_count();
    let lc = large_asset_count();
    let ec = edge_piece_count();
    if idx < pc {
        return build_piece(idx).map(BuildSelection::Piece);
    }
    if idx < pc + prc {
        return prefab(idx - pc).map(BuildSelection::Prefab);
    }
    if idx < pc + prc + lc {
        return large_asset(idx - pc - prc).map(BuildSelection::Large);
    }
    if idx < pc + prc + lc + ec {
        return edge_piece(idx - pc - prc - lc).map(BuildSelection::Edge);
    }
    ceiling_piece(idx - pc - prc - lc - ec).map(BuildSelection::Ceiling)
}

/// **¿Esta celda del nivel activo admite fundar algo?** En el suelo (`z <= 0`)
/// siempre: hay terreno. En altura sólo sobre lo construido —tu casa, o la casa
/// generada del pueblo que estás ampliando— o enganchándose a una losa vecina del
/// mismo piso (`build_grid::floor_supported`, regla de Project Zomboid).
///
/// Lo que aguanta desde abajo son las TRES capas que existen: el tile (piso o
/// pared de asentamiento), la pared de BORDE que rodea la celda, y la estructura
/// colocada. Si faltara alguna, techar tu propia casa fallaría según con qué la
/// hayas levantado, que es la peor clase de regla: la que depende de por dónde
/// entraste.
fn floor_supported_here(
    tiles: &TileWorld,
    walls: &crate::walls::BuiltWalls,
    structures: &crate::structures::BuiltStructures,
    body: ha_shared::CelestialBody,
    z: i32,
    tx: i64,
    tz: i64,
) -> bool {
    ha_shared::build_grid::floor_supported(
        z,
        &|cx, cz| {
            tiles.supports_upper_floor(body, z - 1, cx, cz)
                || walls.any_edge_of_cell(body, z - 1, cx, cz)
                || structures.blocks(body, z - 1, cx, cz)
        },
        &|cx, cz| is_build_floor(tiles.tile_at(body, cx, cz)),
        tx,
        tz,
    )
}

/// Footprint min corner for a prefab aimed at cell `(tx, tz)` (centered).
fn prefab_min(p: &Prefab, tx: i64, tz: i64) -> (i64, i64) {
    (tx - p.w / 2, tz - p.h / 2)
}

/// **Hacia dónde mira lo que estás por colocar.**
///
/// `None` = AUTOMÁTICO: el objeto se planta de espaldas a la pared que tenga al
/// lado. Es lo que uno espera al amoblar —la cabecera contra el muro, la
/// heladera pegada al rincón— y evita tener que girar a mano cada mueble.
/// `Some(r)` = lo fijaste vos con GIRAR y manda sobre el automático, para el caso
/// en que quieras la cama en medio del cuarto.
#[derive(Resource, Default)]
pub struct BuildFacing(pub Option<u8>);

impl BuildFacing {
    /// El botón cicla AUTO → sur → oeste → norte → este → AUTO.
    fn cycle(&mut self) {
        self.0 = match self.0 {
            None => Some(0),
            Some(3) => None,
            Some(r) => Some(r + 1),
        };
    }
    fn label(&self) -> &'static str {
        match self.0 {
            None => "AUTO",
            Some(0) => "SUR",
            Some(1) => "OESTE",
            Some(2) => "NORTE",
            _ => "ESTE",
        }
    }
}

/// **Anclaje a la pared**: de espaldas al muro que tenga la celda al lado. Se
/// mira en orden norte, este, sur, oeste y se toma el primero; sin paredes
/// alrededor queda mirando al sur, que es el reposo de todos los sprites.
fn auto_facing(walls: &crate::walls::BuiltWalls, body: ha_shared::CelestialBody, z: i32, tx: i64, tz: i64) -> u8 {
    use ha_shared::build_grid::WallAxis::{H, V};
    if walls.at(body, z, tx, tz, H).is_some() {
        return 0; // muro al norte → mira al sur
    }
    if walls.at(body, z, tx + 1, tz, V).is_some() {
        return 1; // muro al este → mira al oeste
    }
    if walls.at(body, z, tx, tz + 1, H).is_some() {
        return 2; // muro al sur → mira al norte
    }
    if walls.at(body, z, tx, tz, V).is_some() {
        return 3; // muro al oeste → mira al este
    }
    0
}

/// **¿Esta celda está ADENTRO?** Un mueble de interior (`PlaceCat::Mueble`) no va
/// en mitad de la calle: pide estar bajo techo —el que pusiste o la losa de la
/// planta de arriba— o en un rincón con al menos dos paredes.
///
/// Las dos condiciones existen porque las dos son "adentro" para el jugador: un
/// cuarto terminado tiene tapa, y uno a medio hacer todavía no, pero ya tiene
/// esquinas. Exigir sólo techo obligaría a techar antes de poder amoblar.
fn cell_is_indoors(
    ceilings: &crate::ceilings::BuiltCeilings,
    walls: &crate::walls::BuiltWalls,
    tiles: &TileWorld,
    body: ha_shared::CelestialBody,
    z: i32,
    tx: i64,
    tz: i64,
) -> bool {
    if crate::ceilings::covered(ceilings, tiles, body, z, tx, tz) {
        return true;
    }
    use ha_shared::build_grid::WallAxis::{H, V};
    let sides = [
        walls.at(body, z, tx, tz, H).is_some(),
        walls.at(body, z, tx, tz + 1, H).is_some(),
        walls.at(body, z, tx, tz, V).is_some(),
        walls.at(body, z, tx + 1, tz, V).is_some(),
    ];
    sides.iter().filter(|s| **s).count() >= 2
}

#[derive(Resource, Default)]
struct BuildIndicatorSpawned(bool);

#[derive(Component)]
struct BuildIndicator;
#[derive(Component)]
struct BuildIndicatorLabel;
#[derive(Component)]
struct BuildGhost;
/// Chapa "en qué piso estoy". Ver `sync_floor_indicator`.
#[derive(Component)]
struct FloorIndicator;
/// Chapa GIRAR: toca y el objeto que estás por colocar cambia de orientación.
#[derive(Component)]
struct FacingChip;
#[derive(Component)]
struct FacingChipLabel;

pub struct BuildPlugin;

impl Plugin for BuildPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<SelectedBuildPiece>()
            .init_resource::<BuildFacing>()
            .init_resource::<BuildIndicatorSpawned>()
            .add_systems(Startup, spawn_build_ghost)
            .add_systems(
                Update,
                (
                    ensure_build_indicator,
                    desktop_build_hotkey,
                    desktop_rotate_hotkey,
                    build_tab_select,
                    sync_bag_build_section,
                    sync_build_indicator,
                    sync_floor_indicator,
                    sync_facing_chip,
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

/// `R` gira lo que estás por colocar (escritorio). En el celular lo hace la
/// chapa GIRAR, que es táctil y vive arriba-izquierda, lejos de los joysticks.
fn desktop_rotate_hotkey(keys: Res<ButtonInput<KeyCode>>, mut facing: ResMut<BuildFacing>) {
    if crate::platform::uses_touch_gameplay_ui() {
        return;
    }
    if keys.just_pressed(KeyCode::KeyR) {
        facing.cycle();
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
    // Sólo al cambiar de pestaña: escribir `display` cada frame marca el `Node`
    // sucio y taffy relayoutea toda la UI, aunque el valor sea el mismo.
    let want = if bag.tab == BagTab::Build {
        Display::Flex
    } else {
        Display::None
    };
    for mut node in &mut sections {
        if node.display != want {
            node.display = want;
        }
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
    // Chapa de PISO: nace oculta y sólo aparece fuera de la planta baja. Sin
    // esto la verticalidad es adivinanza — el mundo de abajo se ve atenuado por
    // el hueco, pero "estoy en el 2º" y "estoy en una cueva" no se distinguen, y
    // colocar en el piso equivocado es el error más fácil de cometer.
    commands.entity(root).with_children(|parent| {
        parent
            .spawn((
                Node {
                    position_type: PositionType::Absolute,
                    left: Val::Px(8.0),
                    top: Val::Px(128.0),
                    padding: UiRect::axes(Val::Px(8.0), Val::Px(4.0)),
                    border: UiRect::all(Val::Px(2.0)),
                    ..default()
                },
                BorderColor(ui_theme::accent_cyan()),
                BackgroundColor(ui_theme::button_bg(UiButtonTone::Primary, UiButtonState::Pressed)),
                Visibility::Hidden,
                FloorIndicator,
                Name::new("FloorIndicator"),
            ))
            .with_children(|b| {
                b.spawn((
                    Text::new(""),
                    TextFont { font_size: 13.0, ..default() },
                    TextColor(ui_theme::text_primary()),
                ));
            });
    });
    // Chapa GIRAR: el celular no tiene tecla `R`, y sin ella amoblar de espaldas
    // a la pared sería imposible de corregir a mano. Arriba-izquierda como el
    // resto (biblia §7: ahí no la roban los joysticks).
    commands.entity(root).with_children(|parent| {
        parent
            .spawn((
                Button,
                Node {
                    position_type: PositionType::Absolute,
                    left: Val::Px(8.0),
                    top: Val::Px(156.0),
                    padding: UiRect::axes(Val::Px(8.0), Val::Px(4.0)),
                    border: UiRect::all(Val::Px(2.0)),
                    ..default()
                },
                BorderColor(ui_theme::accent_cyan()),
                BackgroundColor(ui_theme::button_bg(UiButtonTone::Craft, UiButtonState::Pressed)),
                Visibility::Hidden,
                FacingChip,
                Name::new("FacingChip"),
            ))
            .with_children(|b| {
                b.spawn((
                    Text::new("GIRAR: AUTO"),
                    TextFont { font_size: 13.0, ..default() },
                    TextColor(ui_theme::text_primary()),
                    FacingChipLabel,
                ));
            });
    });
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
                    Some(BuildSelection::Ceiling(p)) => p.label,
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

/// **En qué piso estás**, en una línea y sólo cuando no es obvio. Planta baja =
/// sin chapa: el 99 % del juego pasa ahí y una etiqueta permanente sería ruido.
///
/// La verticalidad ya no es sólo la mina: `z > 0` son los pisos que construís
/// (Project Zomboid) y `z < 0` el subsuelo. Que la chapa sea la MISMA para los
/// dos es a propósito — es un solo eje, no dos sistemas.
fn sync_floor_indicator(
    tiles: Res<TileWorld>,
    mut chips: Query<(&mut Visibility, &Children), With<FloorIndicator>>,
    mut labels: Query<&mut Text>,
) {
    let z = tiles.z;
    for (mut vis, children) in &mut chips {
        let want = if z == 0 { Visibility::Hidden } else { Visibility::Inherited };
        if *vis != want {
            *vis = want;
        }
        if z == 0 {
            continue;
        }
        // ASCII-only: la fuente del juego no dibuja acentos ni flechas.
        let line = if z > 0 {
            format!("PISO {}", z + 1)
        } else {
            format!("SUBSUELO {}", -z)
        };
        for child in children.iter() {
            if let Ok(mut text) = labels.get_mut(*child) {
                if text.0 != line {
                    text.0 = line.clone();
                }
            }
        }
    }
}

/// La chapa GIRAR: sólo aparece con algo ORIENTABLE en la mano (un colocable),
/// porque girar un piso o una pared no significa nada y una chapa que no hace
/// nada es peor que ninguna.
fn sync_facing_chip(
    build_mode: Res<BuildModeActive>,
    selected: Res<SelectedBuildPiece>,
    mut facing: ResMut<BuildFacing>,
    taps: Query<&Interaction, (With<FacingChip>, Changed<Interaction>)>,
    mut chips: Query<&mut Visibility, With<FacingChip>>,
    mut label: Query<&mut Text, With<FacingChipLabel>>,
) {
    for interaction in &taps {
        if *interaction == Interaction::Pressed {
            facing.cycle();
        }
    }
    let orientable = build_mode.active
        && matches!(build_selection(selected.0), Some(BuildSelection::Large(_)));
    let want = if orientable { Visibility::Inherited } else { Visibility::Hidden };
    for mut v in &mut chips {
        if *v != want {
            *v = want;
        }
    }
    if orientable {
        if let Ok(mut text) = label.get_single_mut() {
            let line = format!("GIRAR: {}", facing.label());
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
    ceilings: ResMut<'w, crate::ceilings::BuiltCeilings>,
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
    facing: Res<BuildFacing>,
    mut inv: ResMut<PlayerInventory>,
    mut persist: ResMut<InventoryPersistState>,
    mut sfx: EventWriter<crate::audio::PlaySfxAt>,
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
    // TAPAS: ocupan la celda apuntada, pero por encima. Mismas dos reglas que el
    // borde —tocar una ocupada DEMUELE y devuelve el material, si no se coloca
    // exigiendo apoyo—, así techar se aprende sabiendo levantar una pared.
    if let Some(BuildSelection::Ceiling(piece)) = build_selection(selected.0) {
        apply_ceiling_build(
            piece,
            selected.0 - ceiling_palette_base(),
            tx as i64,
            tz as i64,
            &ctx,
            &planet,
            editor.creative,
            &mut world,
            &mut inv,
            &mut persist,
            &mut sfx,
            &cursor,
        );
        return;
    }
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
            sfx.send(crate::audio::PlaySfxAt(
                crate::audio::Sfx::Place,
                render_xy(cursor.world.x, cursor.world.y),
            ));
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
            // En altura, la losa necesita apoyo (la casa de abajo o una losa vecina).
            if !floor_supported_here(
                &world.tiles,
                &world.walls,
                &world.structures,
                body,
                z,
                tx as i64,
                tz as i64,
            ) {
                return;
            }
            if world.tiles.place_tile(body, tx as i64, tz as i64, piece.tile) {
                sfx.send(crate::audio::PlaySfxAt(
                    crate::audio::Sfx::Place,
                    render_xy(cursor.world.x, cursor.world.y),
                ));
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
                        || !floor_supported_here(
                            &world.tiles,
                            &world.walls,
                            &world.structures,
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
            sfx.send(crate::audio::PlaySfxAt(
                crate::audio::Sfx::Stamp,
                render_xy(cursor.world.x, cursor.world.y),
            ));
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
            // Orientación: la que fijaste con GIRAR, o de espaldas a la pared.
            let rot = facing
                .0
                .unwrap_or_else(|| auto_facing(&world.walls, body, z, tx as i64, tz as i64));
            let (fw, fh) = ha_shared::world_edits::footprint_rotated(a.w, a.h, rot);
            // Un mueble de interior no se planta en la calle.
            if !creative
                && a.cat == ha_shared::placeables::PlaceCat::Mueble
                && !cell_is_indoors(
                    &world.ceilings, &world.walls, &world.tiles, body, z, tx as i64, tz as i64,
                )
            {
                return;
            }
            let (min_x, min_z) = (tx as i64 - fw / 2, tz as i64 - fh / 2);
            // Footprint despejado: nada sólido, ni decor cosechable, ni otra
            // estructura (no se apilan).
            for dz in 0..fh {
                for dx in 0..fw {
                    let (cx, cz) = (min_x + dx, min_z + dz);
                    if world.tiles.solid_at(body, cx, cz)
                        || crate::world2d::cell_build_blocked(&flora, body, z, cx, cz)
                        || world.structures.blocks(body, z, cx, cz)
                        || !floor_supported_here(
                            &world.tiles,
                            &world.walls,
                            &world.structures,
                            body,
                            z,
                            cx,
                            cz,
                        )
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
            // `w`/`h` se guardan SIN girar: son la medida del asset. Quien dibuja
            // y quien colisiona aplican `rot` (`footprint_rotated`), así que la
            // orientación es un dato y no una copia deformada del tamaño.
            world.structures.add(crate::structures::PlacedStructure {
                body: body as u8,
                z,
                cx: min_x,
                cz: min_z,
                w: a.w,
                h: a.h,
                art: a.art.to_string(),
                solid,
                rot,
            });
            sfx.send(crate::audio::PlaySfxAt(
                crate::audio::Sfx::Stamp,
                render_xy(cursor.world.x, cursor.world.y),
            ));
            if !creative {
                inv.remove(a.cost_item, a.cost_qty);
            }
            persist.mark_immediate();
        }
        // Ya resueltos arriba (borde y tapa tienen su propio camino).
        BuildSelection::Edge(_) | BuildSelection::Ceiling(_) => {}
    }
}

/// **Colocación / demolición de una TAPA.** Las mismas dos reglas que el borde:
/// 1. Si la celda ya tiene tapa colocada → la saca y devuelve el material.
/// 2. Si no, exige apoyo (`ceiling_supported`: piso construido o una pared en
///    algún borde de la celda) y material; en creativo se saltea todo.
///
/// La tapa DERIVADA de la losa de arriba no se toca acá: ésa no es una pieza, es
/// el piso del vecino de arriba, y se quita quitando esa losa.
#[allow(clippy::too_many_arguments)]
fn apply_ceiling_build(
    piece: &'static CeilingPiece,
    piece_idx: usize,
    tx: i64,
    tz: i64,
    ctx: &WorldContextRuntime,
    planet: &ActivePlanet,
    creative: bool,
    world: &mut BuildTargets,
    inv: &mut PlayerInventory,
    persist: &mut InventoryPersistState,
    sfx: &mut EventWriter<crate::audio::PlaySfxAt>,
    cursor: &crate::input::AimCursor,
) {
    let body = active_surface_body(ctx, planet);
    let z = world.tiles.z;
    if let Some(old) = world.ceilings.remove(body, z, tx, tz) {
        if !creative {
            if let Some(def) = ceiling_piece(old.piece) {
                inv.add(def.cost_item, def.cost_qty);
            }
        }
        sfx.send(crate::audio::PlaySfxAt(
            crate::audio::Sfx::Dig,
            render_xy(cursor.world.x, cursor.world.y),
        ));
        persist.mark_immediate();
        return;
    }
    if !creative {
        if inv.qty(piece.cost_item) < piece.cost_qty {
            return;
        }
        let has_floor = |cx: i64, cz: i64| is_build_floor(world.tiles.tile_at(body, cx, cz));
        let has_wall = |cx: i64, cz: i64| world.walls.any_edge_of_cell(body, z, cx, cz);
        if !ceiling_supported(&has_floor, &has_wall, tx, tz) {
            return;
        }
    }
    world.ceilings.add(crate::ceilings::PlacedCeiling {
        body: body as u8,
        z,
        tx,
        tz,
        piece: piece_idx,
    });
    sfx.send(crate::audio::PlaySfxAt(
        crate::audio::Sfx::Place,
        render_xy(cursor.world.x, cursor.world.y),
    ));
    if !creative {
        inv.remove(piece.cost_item, piece.cost_qty);
    }
    persist.mark_immediate();
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
    sfx: &mut EventWriter<crate::audio::PlaySfxAt>,
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
        sfx.send(crate::audio::PlaySfxAt(
            crate::audio::Sfx::Dig,
            render_xy(cursor.world.x, cursor.world.y),
        ));
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
    sfx.send(crate::audio::PlaySfxAt(
        crate::audio::Sfx::Place,
        render_xy(cursor.world.x, cursor.world.y),
    ));
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
    ceilings: Res<crate::ceilings::BuiltCeilings>,
    facing: Res<BuildFacing>,
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
    // TAPA: ghost de la celda entera. Naranja si ahí ya hay una (el gesto pasa a
    // DEMOLER), verde si entra, rojo si falta apoyo o material.
    if let BuildSelection::Ceiling(piece) = selection {
        let (cx, cz) = (tx as i64, tz as i64);
        let demolish = ceilings.at(body, tiles.z, cx, cz).is_some();
        let has_floor = |x: i64, z: i64| is_build_floor(tiles.tile_at(body, x, z));
        let has_wall = |x: i64, z: i64| walls.any_edge_of_cell(body, tiles.z, x, z);
        let ok = inv.qty(piece.cost_item) >= piece.cost_qty
            && ceiling_supported(&has_floor, &has_wall, cx, cz);
        sprite.color = if demolish {
            Color::srgba(1.0, 0.62, 0.12, 0.7)
        } else if ok {
            Color::srgba(0.25, 1.0, 0.35, 0.6)
        } else {
            Color::srgba(1.0, 0.25, 0.2, 0.55)
        };
        sprite.custom_size = Some(Vec2::splat(TILE_M));
        let r = render_xy((tx as f32 + 0.5) * TILE_M, (tz as f32 + 0.5) * TILE_M);
        tf.translation = Vec3::new(r.x, r.y, 5.7);
        return;
    }
    // Una celda bloquea si es sólida, tiene decor cosechable (talar/picar antes)
    // o —en un piso alto— no tiene nada debajo que la aguante.
    let blocked = |cx: i64, cz: i64| {
        tiles.solid_at(body, cx, cz)
            || crate::world2d::cell_build_blocked(&flora, body, tiles.z, cx, cz)
            || structures.blocks(body, tiles.z, cx, cz)
            || !floor_supported_here(&tiles, &walls, &structures, body, tiles.z, cx, cz)
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
            // El ghost muestra la huella YA GIRADA y aplica la regla de interior:
            // el rojo tiene que significar "asi no entra", incluido "esto va
            // adentro de una casa".
            let rot = facing
                .0
                .unwrap_or_else(|| auto_facing(&walls, body, tiles.z, tx as i64, tz as i64));
            let (fw, fh) = ha_shared::world_edits::footprint_rotated(a.w, a.h, rot);
            let (min_x, min_z) = (tx as i64 - fw / 2, tz as i64 - fh / 2);
            let clear = (0..fh).all(|dz| (0..fw).all(|dx| !blocked(min_x + dx, min_z + dz)));
            let indoors = a.cat != ha_shared::placeables::PlaceCat::Mueble
                || cell_is_indoors(&ceilings, &walls, &tiles, body, tiles.z, tx as i64, tz as i64);
            (
                (inv.qty(a.cost_item) >= a.cost_qty) && clear && indoors,
                Vec2::new(fw as f32 * TILE_M, fh as f32 * TILE_M),
                (min_x as f32 + fw as f32 * 0.5, min_z as f32 + fh as f32 * 0.5),
            )
        }
        // Resueltos arriba (borde y tapa tienen su propio ghost).
        BuildSelection::Edge(_) | BuildSelection::Ceiling(_) => return,
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

#[cfg(test)]
mod tests {
    use super::*;

    /// **La tarjeta que tocás pone EXACTAMENTE lo que dice.**
    ///
    /// El menú dibuja los colocables agrupados por categoría (`hud.rs`) pero el
    /// índice de paleta sigue siendo el de la tabla. Es la clase de acoplamiento
    /// que se rompe en silencio: reordenás una categoría, el menú se ve perfecto y
    /// tocar "Heladera" planta un colectivo. Acá se recorre la paleta igual que la
    /// dibuja el HUD y se exige que el índice devuelva el mismo objeto.
    #[test]
    fn cada_tarjeta_del_menu_coloca_su_propio_objeto() {
        let lbase = build_piece_count() + prefab_count();
        let mut vistas = 0;
        for &cat in ha_shared::placeables::PlaceCat::all() {
            for (i, esperado) in ha_shared::placeables::placeables_in(cat) {
                let Some(BuildSelection::Large(puesto)) = build_selection(lbase + i) else {
                    panic!("la tarjeta {} ({}) no resuelve a un colocable", lbase + i, esperado.label);
                };
                assert_eq!(
                    puesto.art, esperado.art,
                    "la tarjeta de '{}' coloca '{}'",
                    esperado.label, puesto.label
                );
                vistas += 1;
            }
        }
        assert_eq!(vistas, large_asset_count(), "el menu no muestra la tabla entera");
    }

    /// Las cuatro familias de la paleta no pueden pisarse: si los rangos se
    /// solaparan, una pieza de borde se colocaría como estructura (o al revés).
    #[test]
    fn los_rangos_de_la_paleta_no_se_solapan() {
        let pc = build_piece_count();
        let prc = prefab_count();
        let lc = large_asset_count();
        for i in 0..pc {
            assert!(matches!(build_selection(i), Some(BuildSelection::Piece(_))), "{i} no es pieza");
        }
        for i in pc..pc + prc {
            assert!(matches!(build_selection(i), Some(BuildSelection::Prefab(_))), "{i} no es prefab");
        }
        for i in pc + prc..pc + prc + lc {
            assert!(matches!(build_selection(i), Some(BuildSelection::Large(_))), "{i} no es colocable");
        }
        let ebase = edge_palette_base();
        for i in 0..ha_shared::build_grid::edge_piece_count() {
            assert!(
                matches!(build_selection(ebase + i), Some(BuildSelection::Edge(_))),
                "{} no es borde",
                ebase + i
            );
        }
        // El spawn de fauna vive MUY por encima de la paleta: si algún día la
        // tabla creciera hasta ahí, tocar un colocable spawnearía una vaca.
        assert!(
            ebase + ha_shared::build_grid::edge_piece_count() < crate::fauna::SPAWN_PALETTE_BASE,
            "la paleta alcanzo al rango de spawn de fauna"
        );
    }
}
