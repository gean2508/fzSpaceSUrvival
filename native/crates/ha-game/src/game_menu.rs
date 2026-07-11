//! GTA V-style full-screen menu with a Spaceflight-Simulator orbital minimap.
//!
//! Opens on the MENU button (shares `UiEditorState.menu_open`; the old vertical
//! editor panel is gated to edit-UI mode only). The hero is a top-down solar
//! map — Sun centre, orbit rings, live body positions from the shared
//! ephemeris, and your ship marker. A bottom bar keeps the big-button feel and
//! reuses the existing dev `UiEditorCommand`s (no logic duplication).

use bevy::prelude::*;
use bevy::ui::{FocusPolicy, Overflow};
use ha_shared::{body_center_at, body_def, CelestialBody};

use crate::rocket_flight::RocketFlight;
use crate::scene::RegionId;
use crate::solar_system_clock::SolarSystemClock;
use crate::ui_editor::UiEditorState;

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
            .add_systems(Update, (sync_game_menu, update_map_markers).chain());
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

fn sync_game_menu(
    editor: Res<UiEditorState>,
    mut state: ResMut<GameMenuState>,
    mut commands: Commands,
) {
    // Show only when the menu is open and NOT in the edit-UI sub-mode.
    let want = editor.menu_open && !editor.enabled && !editor.settings_open;
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
                    width: Val::VMin(70.0),
                    height: Val::VMin(70.0),
                    border: UiRect::all(Val::Px(2.0)),
                    // Clip orbit rings / body dots to the panel — nothing the
                    // marker system positions can bleed outside the map window.
                    overflow: Overflow::clip(),
                    ..default()
                },
                BorderColor(NEON_CYAN.with_alpha(0.6)),
                BackgroundColor(Color::srgb(0.02, 0.03, 0.06)),
                MapViewport,
            ))
            .with_children(|map| {
                // Orbit rings + body dots (positions set by update_map_markers).
                for &body in CelestialBody::all() {
                    if body == CelestialBody::Sun {
                        continue;
                    }
                    let rgb = body_def(body).color;
                    let color = Color::srgb(rgb[0], rgb[1], rgb[2]);
                    // Orbit ring (thin bordered circle centred on the Sun).
                    map.spawn((
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
                    map.spawn((
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
                map.spawn((
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
                map.spawn((
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

            root.spawn((
                Text::new("● tu nave    ○ órbitas    ☀ Sol"),
                TextFont {
                    font_size: 12.0,
                    ..default()
                },
                TextColor(Color::srgb(0.7, 0.75, 0.85)),
            ));

            // Big-button bar (reuses the dev UiEditorCommands via editor_button_system).
            root.spawn((
                Node {
                    margin: UiRect::top(Val::Px(10.0)),
                    flex_direction: FlexDirection::Row,
                    flex_wrap: FlexWrap::Wrap,
                    justify_content: JustifyContent::Center,
                    column_gap: Val::Px(10.0),
                    row_gap: Val::Px(8.0),
                    ..default()
                },
            ))
            .with_children(|bar| {
                use crate::ui_editor::UiEditorCommand as C;
                menu_button(bar, "CERRAR", C::CloseMenu);
                menu_button(bar, "CREATIVO", C::ToggleCreative);
                menu_button(bar, "PERF", C::TogglePerf);
                menu_button(bar, "TIERRA", C::GoEarth);
                menu_button(bar, "LUNA", C::GoLuna);
                menu_button(bar, "ESPACIO", C::GoSpace);
                menu_button(bar, "EDITAR UI", C::Toggle);
            });
        })
        .id()
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
    flight: Res<RocketFlight>,
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
