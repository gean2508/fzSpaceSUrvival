//! GTA V-style full-screen menu with a Spaceflight-Simulator orbital minimap.
//!
//! Opens on the MENU button (shares `UiEditorState.menu_open`; the old vertical
//! editor panel is gated to edit-UI mode only). The hero is a top-down solar
//! map — Sun centre, orbit rings, live body positions from the shared
//! ephemeris, and your ship marker. A bottom bar keeps the big-button feel and
//! reuses the existing dev `UiEditorCommand`s (no logic duplication).

use bevy::prelude::*;
use bevy::ui::{FocusPolicy, Overflow, RepeatedGridTrack};
use ha_shared::{body_center_at, body_def, CelestialBody};

use crate::flight::Flight;
use crate::scene::RegionId;
use crate::sim::{Player, SimState};
use crate::solar_system_clock::SolarSystemClock;
use crate::ui_editor::UiEditorState;

/// Terrain minimap: NxN cells, each sampling one world tile every STEP units,
/// centred on the player. Cheap enough for the menu (recoloured only while open).
const MINI_N: usize = 24;
const MINI_STEP: f32 = 2.0;

const NEON_CYAN: Color = Color::srgb(0.16, 0.94, 1.0);
const NEON_MAGENTA: Color = Color::srgb(1.0, 0.18, 0.62);
// OPAQUE on purpose: the fullscreen menu covers the whole screen, so a
// semi-transparent panel forced the GPU to alpha-blend a fullscreen layer OVER
// the still-rendering 3D scene every frame — on the fill-rate-bound Redmi (§6)
// that alone dropped the map to ~4 fps. Opaque = no blend, no overdraw tax.
const PANEL_BG: Color = Color::srgb(0.03, 0.045, 0.08);

/// Map projection: solar X/Z (flight metres) → normalized panel space, centrado
/// en el Sol. Debe cubrir el radio orbital MÁS lejano (Neptuno ~630 km del Sol)
/// en ±0.5 → span ≈ 2·radio. Antes 520 km recortaba: los planetas exteriores se
/// salían de la ventana del mapa.
const MAP_SPAN_M: f32 = 1_400_000.0;

pub struct GameMenuPlugin;

impl Plugin for GameMenuPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<GameMenuState>()
            .add_systems(
                Update,
                (
                    sync_game_menu,
                    sync_menu_edit_mode.after(sync_game_menu),
                    update_map_markers,
                    update_terrain_minimap,
                    update_luna_base_monitor,
                )
                    .chain(),
            );
    }
}

#[derive(Resource, Default)]
struct GameMenuState {
    spawned: bool,
    root: Option<Entity>,
}

#[derive(Component)]
struct GameMenuRoot;
#[derive(Component)]
struct MapViewport;
/// A live body dot — repositioned each frame from the ephemeris.
#[derive(Component)]
struct MapBody(CelestialBody);
#[derive(Component)]
struct MapShipMarker;
/// Wraps the orbital dots/rings/sun/ship — shown in Space, hidden on a surface.
#[derive(Component)]
struct OrbitalLayer;
/// Wraps the terrain minimap grid — shown on a surface, hidden in Space.
#[derive(Component)]
struct TerrainLayer;
/// One terrain minimap cell; `.0` is its flat index (row * MINI_N + col).
#[derive(Component)]
struct MinimapCell(usize);
/// The map title text — swapped between "MAPA ORBITAL" and the body name.
#[derive(Component)]
struct MapTitle;
/// The UI-editing toolbar (element selectors + scale) — shown only in edit mode.
#[derive(Component)]
struct EditToolbar;
/// Remote camp monitor line — fed from `MachineWorld` while the menu is open.
#[derive(Component)]
struct LunaBaseMonitor;

fn sync_game_menu(
    editor: Res<UiEditorState>,
    mut state: ResMut<GameMenuState>,
    mut commands: Commands,
) {
    // ONE menu: shown whenever open. Edit-UI mode keeps it (goes see-through
    // via `sync_menu_edit_mode`); only the CALIBRATION sub-dialog replaces it.
    let want = editor.menu_open && !editor.settings_open;
    if want && !state.spawned {
        state.root = Some(spawn_menu(&mut commands));
        state.spawned = true;
    } else if !want && state.spawned {
        if let Some(root) = state.root.take() {
            commands.entity(root).despawn_recursive();
        }
        state.spawned = false;
    }
}

/// Edit-UI mode: the SAME menu goes see-through (map hidden, background clear)
/// so the on-screen controls + their drag handles are visible/editable — no
/// second panel. Exits back to the normal opaque map menu.
#[allow(clippy::type_complexity)]
#[allow(clippy::type_complexity)]
fn sync_menu_edit_mode(
    editor: Res<UiEditorState>,
    mut roots: Query<(&mut BackgroundColor, &mut FocusPolicy), With<GameMenuRoot>>,
    mut sets: ParamSet<(
        Query<&mut Visibility, With<MapViewport>>,
        Query<&mut Visibility, With<EditToolbar>>,
    )>,
) {
    let Ok((mut bg, mut fp)) = roots.get_single_mut() else {
        return;
    };
    let (map_vis, toolbar_vis) = if editor.enabled {
        *bg = BackgroundColor(Color::NONE);
        *fp = FocusPolicy::Pass;
        (Visibility::Hidden, Visibility::Inherited)
    } else {
        *bg = BackgroundColor(PANEL_BG);
        *fp = FocusPolicy::Block;
        (Visibility::Inherited, Visibility::Hidden)
    };
    for mut v in &mut sets.p0() {
        *v = map_vis;
    }
    for mut v in &mut sets.p1() {
        *v = toolbar_vis;
    }
}

fn spawn_menu(commands: &mut Commands) -> Entity {
    commands
        .spawn((
            Node {
                position_type: PositionType::Absolute,
                left: Val::Px(0.0),
                top: Val::Px(0.0),
                width: Val::Percent(100.0),
                height: Val::Percent(100.0),
                flex_direction: FlexDirection::Column,
                align_items: AlignItems::Center,
                padding: UiRect::all(Val::Px(16.0)),
                row_gap: Val::Px(12.0),
                ..default()
            },
            BackgroundColor(PANEL_BG),
            GlobalZIndex(900),
            FocusPolicy::Block,
            GameMenuRoot,
            Name::new("GameMenu"),
        ))
        .with_children(|root| {
            root.spawn((
                Text::new("MAPA ORBITAL"),
                TextFont {
                    font_size: 26.0,
                    ..default()
                },
                TextColor(NEON_CYAN),
                MapTitle,
            ));

            // Square map viewport (keeps the top-down projection undistorted).
            root.spawn((
                Node {
                    // Square sized to the SMALLER screen dimension (VMin) so it
                    // always fits both orientations and stays square → orbit
                    // circles never distort. This also sidesteps the bug that
                    // pushed the map off-panel: `ComputedNode.size()` is in
                    // PHYSICAL px, but `Node` offsets are LOGICAL px, so on a
                    // high-DPI phone (UI scale ~2.8) the old pixel-measured
                    // layout multiplied every marker position and threw the
                    // whole system to the right. Markers are now positioned in
                    // PERCENT (see `update_map_markers`), no measurement needed.
                    // Square viewport sized off the viewport HEIGHT (Vh) so it is
                    // always laid out with an explicit size — `VMin` was collapsing
                    // to ~0 here, leaving the map blank. Kept square so orbit circles
                    // don't distort; markers are positioned in PERCENT.
                    width: Val::Vh(52.0),
                    height: Val::Vh(52.0),
                    border: UiRect::all(Val::Px(2.0)),
                    // Clip orbit rings / body dots to the panel — nothing the
                    // marker system positions can bleed outside the map window.
                    overflow: Overflow::clip(),
                    ..default()
                },
                BorderColor(NEON_CYAN),
                BackgroundColor(Color::srgb(0.05, 0.07, 0.12)),
                MapViewport,
            ))
            .with_children(|map| {
                // ── Orbital layer (shown in Space): rings + body dots + ship.
                map.spawn((
                    Node {
                        position_type: PositionType::Absolute,
                        left: Val::Px(0.0),
                        top: Val::Px(0.0),
                        width: Val::Percent(100.0),
                        height: Val::Percent(100.0),
                        ..default()
                    },
                    OrbitalLayer,
                ))
                .with_children(|orb| {
                    // Orbit rings + body dots (positions set by update_map_markers).
                    for &body in CelestialBody::all() {
                        if body == CelestialBody::Sun {
                            continue;
                        }
                        let rgb = body_def(body).color;
                        let color = Color::srgb(rgb[0], rgb[1], rgb[2]);
                        // Orbit ring (thin bordered circle centred on the Sun).
                        orb.spawn((
                            Node {
                                position_type: PositionType::Absolute,
                                border: UiRect::all(Val::Px(1.0)),
                                ..default()
                            },
                            BorderColor(color.with_alpha(0.20)),
                            BorderRadius::all(Val::Percent(50.0)),
                            MapOrbitRing(body),
                        ));
                        // Body dot. Negative margin = half its size so the percent
                        // position (its top-left) lands the dot's CENTRE on the point.
                        orb.spawn((
                            Node {
                                position_type: PositionType::Absolute,
                                width: Val::Px(9.0),
                                height: Val::Px(9.0),
                                margin: UiRect::all(Val::Px(-4.5)),
                                ..default()
                            },
                            BackgroundColor(color),
                            BorderRadius::all(Val::Percent(50.0)),
                            MapBody(body),
                        ));
                    }
                    // Sun at centre.
                    orb.spawn((
                        Node {
                            position_type: PositionType::Absolute,
                            width: Val::Px(16.0),
                            height: Val::Px(16.0),
                            margin: UiRect::all(Val::Px(-8.0)),
                            ..default()
                        },
                        BackgroundColor(Color::srgb(1.0, 0.86, 0.35)),
                        BorderRadius::all(Val::Percent(50.0)),
                        MapBody(CelestialBody::Sun),
                    ));
                    // Your ship marker (magenta).
                    orb.spawn((
                        Node {
                            position_type: PositionType::Absolute,
                            width: Val::Px(10.0),
                            height: Val::Px(10.0),
                            border: UiRect::all(Val::Px(2.0)),
                            margin: UiRect::all(Val::Px(-5.0)),
                            ..default()
                        },
                        BorderColor(Color::WHITE),
                        BackgroundColor(NEON_MAGENTA),
                        BorderRadius::all(Val::Percent(50.0)),
                        MapShipMarker,
                    ));
                });

                // ── Terrain layer (shown on a surface): NxN grid of tile cells,
                // recoloured each frame from the tiles around the player.
                map.spawn((
                    Node {
                        position_type: PositionType::Absolute,
                        left: Val::Px(0.0),
                        top: Val::Px(0.0),
                        width: Val::Percent(100.0),
                        height: Val::Percent(100.0),
                        display: Display::Grid,
                        grid_template_columns: vec![RepeatedGridTrack::flex(
                            MINI_N as u16,
                            1.0,
                        )],
                        grid_template_rows: vec![RepeatedGridTrack::flex(MINI_N as u16, 1.0)],
                        ..default()
                    },
                    Visibility::Hidden,
                    TerrainLayer,
                ))
                .with_children(|grid| {
                    for i in 0..MINI_N * MINI_N {
                        grid.spawn((
                            Node::default(),
                            BackgroundColor(Color::srgb(0.02, 0.03, 0.06)),
                            MinimapCell(i),
                        ));
                    }
                    // Player marker: absolute overlay at the grid centre.
                    grid.spawn((
                        Node {
                            position_type: PositionType::Absolute,
                            left: Val::Percent(50.0),
                            top: Val::Percent(50.0),
                            width: Val::Px(11.0),
                            height: Val::Px(11.0),
                            border: UiRect::all(Val::Px(2.0)),
                            margin: UiRect::all(Val::Px(-5.5)),
                            ..default()
                        },
                        BorderColor(Color::WHITE),
                        BackgroundColor(NEON_MAGENTA),
                        BorderRadius::all(Val::Percent(50.0)),
                    ));
                });
            });

            root.spawn((
                Text::new("+ tu     o orbitas    * Sol"),
                TextFont {
                    font_size: 12.0,
                    ..default()
                },
                TextColor(Color::srgb(0.7, 0.75, 0.85)),
            ));

            // Remote base monitor — the antenna's uplink (Act II "agencia").
            // Text stays empty until an antenna exists on the Moon.
            root.spawn((
                Text::new(""),
                TextFont {
                    font_size: 13.0,
                    ..default()
                },
                TextColor(NEON_CYAN),
                LunaBaseMonitor,
                Name::new("LunaBaseMonitor"),
            ));

            // Colony uplink actions — hidden until the antenna is up
            // (`colony::colony_button_system` owns visibility + costs).
            root.spawn((
                Node {
                    flex_direction: FlexDirection::Row,
                    column_gap: Val::Px(10.0),
                    ..default()
                },
                Name::new("ColonyUplinkRow"),
            ))
            .with_children(|row| {
                use crate::colony::{ColonyAction, ColonyButton};
                menu_button(row, "COLONO (5 manz)", ColonyButton(ColonyAction::SendColonist));
                menu_button(row, "VIVERES (3 manz)", ColonyButton(ColonyAction::SendSupplies));
            });

            // ── Menú principal: SOLO funciones de menú (viaje, modos, opciones).
            // Los controles del jugador (mover/saltar/usar) viven en el HUD, no
            // acá; los selectores del editor salen a la barra de edición (abajo),
            // visible sólo en "EDITAR UI".
            menu_row(root, |bar| {
                use crate::ui_editor::UiEditorCommand as C;
                menu_button(bar, "TIERRA", C::GoEarth);
                menu_button(bar, "LUNA", C::GoLuna);
                menu_button(bar, "ESPACIO", C::GoSpace);
                menu_button(bar, "CREATIVO", C::ToggleCreative);
                menu_button(bar, "PERF", C::TogglePerf);
            });
            menu_row(root, |bar| {
                use crate::ui_editor::UiEditorCommand as C;
                menu_button(bar, "EDITAR UI", C::Toggle);
                menu_button(bar, "AJUSTES", C::OpenSettings);
                menu_button(bar, "CERRAR", C::CloseMenu);
            });

            // ── Barra de EDICIÓN de UI (oculta salvo en modo EDITAR UI). Estos
            // botones eligen QUÉ control reposicionar y su escala — no controlan
            // al jugador. Se muestran/ocultan en `sync_menu_edit_mode`.
            root.spawn((
                Node {
                    margin: UiRect::top(Val::Px(8.0)),
                    flex_direction: FlexDirection::Column,
                    align_items: AlignItems::Center,
                    row_gap: Val::Px(6.0),
                    ..default()
                },
                Visibility::Hidden,
                EditToolbar,
            ))
            .with_children(|col| {
                col.spawn((
                    Text::new("EDITAR UI - elegi un control y movelo/escalalo:"),
                    TextFont {
                        font_size: 13.0,
                        ..default()
                    },
                    TextColor(NEON_CYAN),
                ));
                menu_row(col, |bar| {
                    use crate::ui_editor::UiEditorCommand as C;
                    menu_button(bar, "> Stick", C::SelectMove);
                    menu_button(bar, "> Usar", C::SelectUse);
                    menu_button(bar, "> Saltar", C::SelectJump);
                    menu_button(bar, "-", C::ScaleDown);
                    menu_button(bar, "+", C::ScaleUp);
                    menu_button(bar, "RESET", C::Reset);
                    menu_button(bar, "GUARDAR", C::Save);
                });
            });
        })
        .id()
}

fn menu_row(parent: &mut ChildBuilder, build: impl FnOnce(&mut ChildBuilder)) {
    parent
        .spawn(Node {
            margin: UiRect::top(Val::Px(8.0)),
            flex_direction: FlexDirection::Row,
            flex_wrap: FlexWrap::Wrap,
            justify_content: JustifyContent::Center,
            column_gap: Val::Px(8.0),
            row_gap: Val::Px(8.0),
            ..default()
        })
        .with_children(build);
}

fn menu_button(parent: &mut ChildBuilder, label: &str, cmd: impl Bundle) {
    parent
        .spawn((
            Button,
            Node {
                width: Val::Px(112.0),
                height: Val::Px(44.0),
                border: UiRect::all(Val::Px(2.0)),
                justify_content: JustifyContent::Center,
                align_items: AlignItems::Center,
                ..default()
            },
            BorderColor(NEON_CYAN),
            BackgroundColor(Color::srgb(0.10, 0.14, 0.22)),
            cmd,
            Name::new(format!("GameMenuBtn_{label}")),
        ))
        .with_children(|b| {
            b.spawn((
                Text::new(label),
                TextFont {
                    font_size: 15.0,
                    ..default()
                },
                TextColor(NEON_CYAN),
            ));
        });
}

#[derive(Component)]
struct MapOrbitRing(CelestialBody);

/// Solar position → percent of the (square) viewport, Sun at centre. All map
/// layout is in PERCENT so it is resolution- and UI-scale-independent: no
/// `ComputedNode` pixel measurement (which returns PHYSICAL px while `Node`
/// offsets are LOGICAL px — the mismatch that threw the whole map off-panel to
/// the right on a high-DPI phone). Markers carry a negative margin of half
/// their size, so the percent (their top-left) lands their centre on the point.
fn map_percent(pos: [f32; 3], sun: [f32; 3]) -> (f32, f32) {
    let nx = (0.5 + (pos[0] - sun[0]) / MAP_SPAN_M).clamp(0.0, 1.0);
    let nz = (0.5 + (pos[2] - sun[2]) / MAP_SPAN_M).clamp(0.0, 1.0);
    (nx * 100.0, nz * 100.0)
}

#[allow(clippy::type_complexity)]
fn update_map_markers(
    menu: Res<GameMenuState>,
    clock: Res<SolarSystemClock>,
    flight: Res<Flight>,
    region: Res<crate::scene::RegionRuntime>,
    mut sets: ParamSet<(
        Query<(&MapBody, &mut Node)>,
        Query<(&MapOrbitRing, &mut Node)>,
        Query<&mut Node, With<MapShipMarker>>,
    )>,
) {
    if !menu.spawned {
        return;
    }
    let t = clock.t;
    let sun = ha_shared::sun_position();

    for (body, mut node) in &mut sets.p0() {
        let (px, pz) = map_percent(body_center_at(body.0, t), sun);
        node.left = Val::Percent(px);
        node.top = Val::Percent(pz);
    }
    // Orbit rings: diameter = 2·(heliocentric distance / span), centred on the
    // panel. The viewport is square, so a percent width == percent height →
    // circles stay circular. Radius capped to half so the ring can't exceed it.
    for (ring, mut node) in &mut sets.p1() {
        let bc = body_center_at(ring.0, t);
        let r_frac = (((bc[0] - sun[0]).powi(2) + (bc[2] - sun[2]).powi(2)).sqrt() / MAP_SPAN_M)
            .clamp(0.004, 0.5);
        node.width = Val::Percent(r_frac * 200.0);
        node.height = Val::Percent(r_frac * 200.0);
        node.left = Val::Percent(50.0 - r_frac * 100.0);
        node.top = Val::Percent(50.0 - r_frac * 100.0);
    }
    // Ship marker: use the flight position, else the body you stand on.
    if let Ok(mut node) = sets.p2().get_single_mut() {
        let ship_world = if flight.active {
            flight.pos.to_array()
        } else {
            let body = match region.active {
                RegionId::Luna => CelestialBody::Moon,
                RegionId::Planet(b) => b,
                _ => CelestialBody::Earth,
            };
            body_center_at(body, t)
        };
        let (px, pz) = map_percent(ship_world, sun);
        node.left = Val::Percent(px);
        node.top = Val::Percent(pz);
    }
}

/// The surface body you're standing on, or None when in Space/Transit (the
/// orbital map). Digging only changes the tile layer (`TileWorld.z`), not the
/// region, so a cave still reports its body — the minimap shows that depth.
fn surface_body(region: &RegionId) -> Option<CelestialBody> {
    match region {
        RegionId::Overworld { .. } => Some(CelestialBody::Earth),
        RegionId::Luna => Some(CelestialBody::Moon),
        RegionId::Planet(b) => Some(*b),
        _ => None,
    }
}

fn body_menu_name(b: CelestialBody) -> String {
    match b {
        CelestialBody::Earth => "TIERRA".to_string(),
        CelestialBody::Moon => "LUNA".to_string(),
        other => body_def(other).label.to_string(),
    }
}

/// Swaps the map between the orbital view (Space) and a top-down terrain
/// minimap (any surface), and — on a surface — recolours the grid from the
/// tiles around the player. Screen-up = −z, matching the main game view.
#[allow(clippy::type_complexity)]
fn update_terrain_minimap(
    menu: Res<GameMenuState>,
    region: Res<crate::scene::RegionRuntime>,
    tiles: Res<crate::world2d::TileWorld>,
    player: Query<&SimState, With<Player>>,
    mut sets: ParamSet<(
        Query<&mut Visibility, With<OrbitalLayer>>,
        Query<&mut Visibility, With<TerrainLayer>>,
        Query<(&MinimapCell, &mut BackgroundColor)>,
        Query<&mut Text, With<MapTitle>>,
    )>,
) {
    if !menu.spawned {
        return;
    }
    let body = surface_body(&region.active);
    let surface = body.is_some();

    for mut v in &mut sets.p0() {
        *v = if surface { Visibility::Hidden } else { Visibility::Inherited };
    }
    for mut v in &mut sets.p1() {
        *v = if surface { Visibility::Inherited } else { Visibility::Hidden };
    }
    let title = match body {
        None => "MAPA ORBITAL".to_string(),
        Some(b) => body_menu_name(b),
    };
    if let Ok(mut t) = sets.p3().get_single_mut() {
        if t.0 != title {
            t.0 = title;
        }
    }

    let Some(body) = body else {
        return;
    };
    let (px, pz) = player
        .get_single()
        .map(|s| (s.body.x, s.body.z))
        .unwrap_or((0.0, 0.0));
    let cx = px.floor() as i64;
    let cz = pz.floor() as i64;
    let half = MINI_N as f32 * 0.5;
    let surface_layer = tiles.z == 0;
    for (cell, mut bg) in &mut sets.p2() {
        let col = (cell.0 % MINI_N) as f32;
        let row = (cell.0 / MINI_N) as f32;
        let tx = cx + ((col - half) * MINI_STEP).round() as i64;
        let tz = cz + ((row - half) * MINI_STEP).round() as i64;
        let mut rgb = tiles.tile_at(body, tx, tz).color();
        // Trees/rocks darken their cell so clearings read lighter (surface only).
        if surface_layer && crate::world2d::surface_decor_at(body, tx, tz).is_some() {
            rgb = [rgb[0] * 0.6, rgb[1] * 0.6, rgb[2] * 0.6];
        }
        *bg = BackgroundColor(Color::srgb(rgb[0], rgb[1], rgb[2]));
    }
}

/// Antenna uplink — while the menu is open, show the lunar camp status line:
/// drills placed, iron banked while you were away, and the O₂ tether.
fn update_luna_base_monitor(
    menu: Res<GameMenuState>,
    machines: Res<crate::machines::MachineWorld>,
    colony: Res<crate::colony::ColonyRuntime>,
    mut monitor: Query<&mut Text, With<LunaBaseMonitor>>,
) {
    if !menu.spawned {
        return;
    }
    let Ok(mut text) = monitor.get_single_mut() else {
        return;
    };
    let line = if machines.has_antenna_on(CelestialBody::Moon) {
        let drills = machines
            .on_body(CelestialBody::Moon)
            .filter(|m| m.machine_id == ha_shared::REGOLITH_DRILL.id)
            .count();
        let habitats = machines
            .on_body(CelestialBody::Moon)
            .filter(|m| m.machine_id == ha_shared::HABITAT.id)
            .count() as u32;
        let banked: u32 = machines
            .on_body(CelestialBody::Moon)
            .map(|m| m.stored_output)
            .sum();
        let crew = colony.state.colonists;
        let cap = ha_shared::colony_capacity(habitats);
        let fed = if crew == 0 {
            ""
        } else if ha_shared::colony_fed(&colony.state) {
            " (trabajando x1.5)"
        } else {
            " (SIN VIVERES)"
        };
        format!(
            "BASE LUNA - antena OK | taladros {drills} | hierro {banked} | colonos {crew}/{cap}{fed} | viveres {:.0}",
            colony.state.food_stock
        )
    } else {
        String::new()
    };
    if text.0 != line {
        text.0 = line;
    }
}
