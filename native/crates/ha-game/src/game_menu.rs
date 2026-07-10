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
                    width: Val::Px(440.0),
                    height: Val::Px(440.0),
                    max_width: Val::Percent(92.0),
                    max_height: Val::Percent(66.0),
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
                    // Body dot.
                    map.spawn((
                        Node {
                            position_type: PositionType::Absolute,
                            width: Val::Px(9.0),
                            height: Val::Px(9.0),
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

/// Proyecta una posición del sistema solar en el viewport del mapa (Sol al
/// centro). Escala por `min(w,h)` → sin distorsión aunque el panel no sea
/// cuadrado; centra cada eje por su propia mitad → no se descentra.
fn project(pos: [f32; 3], w: f32, h: f32) -> (f32, f32) {
    let sun = ha_shared::sun_position();
    let scale = w.min(h);
    let rel_x = (pos[0] - sun[0]) / MAP_SPAN_M; // -0.5..0.5-ish
    let rel_z = (pos[2] - sun[2]) / MAP_SPAN_M;
    (w * 0.5 + rel_x * scale, h * 0.5 + rel_z * scale)
}

#[allow(clippy::type_complexity)]
fn update_map_markers(
    menu: Res<GameMenuState>,
    clock: Res<SolarSystemClock>,
    flight: Res<RocketFlight>,
    region: Res<crate::scene::RegionRuntime>,
    viewport: Query<&ComputedNode, With<MapViewport>>,
    mut sets: ParamSet<(
        Query<(&MapBody, &mut Node)>,
        Query<(&MapOrbitRing, &mut Node)>,
        Query<&mut Node, With<MapShipMarker>>,
    )>,
) {
    if !menu.spawned {
        return;
    }
    let Ok(vp) = viewport.get_single() else {
        return;
    };
    // Wait for the real computed size. On the first frame(s) the UI layout has
    // not run yet and `size()` is ~0 — positioning against an 80 px fallback is
    // exactly what made the map render tiny in the corner and then "jump" to
    // full size. Skip until Taffy has a real box; then it draws once, correctly.
    let raw = vp.size();
    if raw.x < 1.0 || raw.y < 1.0 {
        return;
    }
    let w = raw.x;
    let h = raw.y;
    let scale = w.min(h);
    let t = clock.t;

    for (body, mut node) in &mut sets.p0() {
        let (x, y) = project(body_center_at(body.0, t), w, h);
        let half = 6.0;
        // Clamp dentro del panel (nunca salir de la ventana del mapa).
        let x = x.clamp(half, w - half);
        let y = y.clamp(half, h - half);
        node.left = Val::Px(x - half);
        node.top = Val::Px(y - half);
    }
    // Orbit rings: radius = body's heliocentric distance projected. Clamp the
    // radius ONCE and derive both size and position from it — the old code
    // sized with a clamped diameter but positioned with the RAW radius, so a
    // far body's ring flew off to a negative offset and escaped the panel.
    for (ring, mut node) in &mut sets.p1() {
        let bc = body_center_at(ring.0, t);
        let sun = ha_shared::sun_position();
        let r_world = ((bc[0] - sun[0]).powi(2) + (bc[2] - sun[2]).powi(2)).sqrt();
        let rr = ((r_world / MAP_SPAN_M) * scale).clamp(1.0, scale * 0.5);
        node.width = Val::Px(rr * 2.0);
        node.height = Val::Px(rr * 2.0);
        node.left = Val::Px(w * 0.5 - rr);
        node.top = Val::Px(h * 0.5 - rr);
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
        let (x, y) = project(ship_world, w, h);
        node.left = Val::Px((x - 5.0).clamp(0.0, w - 10.0));
        node.top = Val::Px((y - 5.0).clamp(0.0, h - 10.0));
    }
}
