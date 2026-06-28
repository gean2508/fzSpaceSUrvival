use bevy::input::touch::{TouchInput, TouchPhase};
use bevy::prelude::*;

use crate::debug::PerfOverlayVisible;
use crate::celestial_cleanup::CelestialCleanup;
use crate::world_travel::FlightTransition;
use crate::rocket_flight::RocketFlight;
use crate::space_travel::space_dock_origin;
use ha_shared::{
    planet_gameplay_world_pos, planet_ground_y_at, CelestialBody, WorldContext,
    LAUNCH_PAD_ZONE_X, LAUNCH_PAD_ZONE_Z,
};
use crate::moon_world::MoonWorldState;
use crate::scene::{LaunchAtmosphere, RegionId, RegionRuntime, WorldContextRuntime};
use crate::sim::{Player, PlayerVisual, SimState};
use crate::game_ui::{self, apply_top_center, apply_center, GameUiRoot, MENU_BUTTON_WIDTH};
use crate::input::calibration::InputCalibration;
use crate::input::{TouchBlockRects, TouchLayout};
use crate::ui_theme::{self, UiButtonTone};

pub struct UiEditorPlugin;

impl Plugin for UiEditorPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(load_ui_layout().unwrap_or_default())
            .init_resource::<UiEditorState>()
            .init_resource::<EditorUiSpawned>()
            .init_resource::<EditorTouchBlocks>()
            .init_resource::<EditorPanelsState>()
            .add_systems(
                Update,
                (
                    ensure_editor_ui.after(crate::game_ui::sync_game_ui_root),
                    dedupe_menu_buttons,
                    sync_editor_panels.after(dedupe_menu_buttons),
                    editor_button_system,
                    editor_touch_drag_system,
                    update_editor_overlay.after(sync_editor_panels),
                    update_settings_labels,
                    append_editor_touch_blocks.after(crate::input::layout_action_buttons),
                    calibration_button_system,
                ),
            );
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UiEditable {
    MoveStick,
    UseButton,
    JumpButton,
}

#[derive(Clone, Copy, Debug)]
pub struct UiSlot {
    pub x: f32,
    pub y: f32,
    pub scale: f32,
}

impl UiSlot {
    const fn new(x: f32, y: f32, scale: f32) -> Self {
        Self { x, y, scale }
    }
}

#[derive(Resource, Clone, Debug)]
pub struct UiLayoutConfig {
    pub move_stick: UiSlot,
    pub use_button: UiSlot,
    pub jump_button: UiSlot,
    pub calibration: InputCalibration,
}

impl Default for UiLayoutConfig {
    fn default() -> Self {
        Self {
            move_stick: UiSlot::new(0.14, 0.78, 1.18),
            use_button: UiSlot::new(0.62, 0.78, 1.0),
            jump_button: UiSlot::new(0.48, 0.78, 1.0),
            calibration: InputCalibration::default(),
        }
    }
}

impl UiLayoutConfig {
    pub fn slot(&self, editable: UiEditable) -> UiSlot {
        match editable {
            UiEditable::MoveStick => self.move_stick,
            UiEditable::UseButton => self.use_button,
            UiEditable::JumpButton => self.jump_button,
        }
    }

    pub fn slot_mut(&mut self, editable: UiEditable) -> &mut UiSlot {
        match editable {
            UiEditable::MoveStick => &mut self.move_stick,
            UiEditable::UseButton => &mut self.use_button,
            UiEditable::JumpButton => &mut self.jump_button,
        }
    }

    pub fn center(&self, layout: &TouchLayout, editable: UiEditable, size: Vec2) -> Vec2 {
        let slot = self.slot(editable);
        let min_x = layout.insets.left as f32 + size.x * 0.5 + 6.0;
        let max_x = layout.width - layout.insets.right as f32 - size.x * 0.5 - 6.0;
        let min_y = layout.insets.top as f32 + size.y * 0.5 + 6.0;
        let max_y = layout.height - layout.insets.bottom as f32 - size.y * 0.5 - 6.0;
        Vec2::new(
            (slot.x * layout.width).clamp(min_x, max_x),
            (slot.y * layout.height).clamp(min_y, max_y),
        )
    }

    pub fn rect(&self, layout: &TouchLayout, editable: UiEditable, size: Vec2) -> [f32; 4] {
        let c = self.center(layout, editable, size);
        [
            c.x - size.x * 0.5,
            c.y - size.y * 0.5,
            c.x + size.x * 0.5,
            c.y + size.y * 0.5,
        ]
    }

    fn set_center_norm(&mut self, layout: &TouchLayout, editable: UiEditable, pos: Vec2, size: Vec2) {
        let min_x = layout.insets.left as f32 + size.x * 0.5 + 6.0;
        let max_x = layout.width - layout.insets.right as f32 - size.x * 0.5 - 6.0;
        let min_y = layout.insets.top as f32 + size.y * 0.5 + 6.0;
        let max_y = layout.height - layout.insets.bottom as f32 - size.y * 0.5 - 6.0;
        let x = pos.x.clamp(min_x, max_x) / layout.width.max(1.0);
        let y = pos.y.clamp(min_y, max_y) / layout.height.max(1.0);
        let slot = self.slot_mut(editable);
        slot.x = x;
        slot.y = y;
    }
}

#[derive(Resource, Debug)]
pub struct UiEditorState {
    pub enabled: bool,
    pub menu_open: bool,
    pub settings_open: bool,
    pub selected: UiEditable,
    active_pointer: Option<u64>,
}

impl Default for UiEditorState {
    fn default() -> Self {
        Self {
            enabled: false,
            menu_open: false,
            settings_open: false,
            selected: UiEditable::UseButton,
            active_pointer: None,
        }
    }
}

#[derive(Resource, Default)]
struct EditorUiSpawned(bool);

/// Touch rects owned by the in-game menu / editor panel (merged after action buttons).
#[derive(Resource, Default, Clone)]
pub struct EditorTouchBlocks(pub Vec<[f32; 4]>);

#[derive(Resource, Default)]
struct EditorPanelsState {
    menu_panel: Option<Entity>,
    settings_panel: Option<Entity>,
}

#[derive(Component)]
struct UiEditorPanel;

#[derive(Component, Clone, Copy)]
enum CalAdjust {
    CamSensDown,
    CamSensUp,
    MoveSensDown,
    MoveSensUp,
    CurveDown,
    CurveUp,
    RenderDown,
    RenderUp,
}

#[derive(Component)]
struct SettingsPanel;

#[derive(Component)]
struct CalLabel(CalAdjust);

#[derive(Component)]
struct UiEditorLabel;

#[derive(Component)]
struct UiMenuButton;

#[derive(Component, Clone, Copy)]
enum UiEditorCommand {
    Menu,
    CloseMenu,
    CloseSettings,
    Toggle,
    SelectMove,
    SelectUse,
    SelectJump,
    OpenSettings,
    TogglePerf,
    GoLuna,
    GoSpace,
    GoEarth,
    ScaleDown,
    ScaleUp,
    Reset,
    Save,
    #[cfg(debug_assertions)]
    OpenAssets,
}

const EDITOR_PANEL_W: f32 = 116.0;
const EDITOR_PANEL_H: f32 = 436.0;
const SETTINGS_PANEL_W: f32 = 168.0;
const SETTINGS_PANEL_H: f32 = 168.0;

fn ensure_editor_ui(
    mut commands: Commands,
    layout: Res<TouchLayout>,
    root: Query<Entity, With<GameUiRoot>>,
    mut spawned: ResMut<EditorUiSpawned>,
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
                    width: Val::Px(MENU_BUTTON_WIDTH),
                    height: Val::Px(26.0),
                    border: UiRect::all(Val::Px(2.0)),
                    justify_content: JustifyContent::Center,
                    align_items: AlignItems::Center,
                    ..default()
                },
                BorderColor(ui_theme::button_border(UiButtonTone::Primary)),
                BackgroundColor(ui_theme::button_bg(
                    UiButtonTone::Primary,
                    ui_theme::interaction_state(Interaction::None, false),
                )),
                UiEditorCommand::Menu,
                UiMenuButton,
                Name::new("InGameMenuButton"),
            ))
            .with_children(|button| {
                button.spawn((
                    Text::new("MENU"),
                    TextFont {
                        font_size: 12.0,
                        ..default()
                    },
                    TextColor(ui_theme::text_primary()),
                ));
            });
    });
}

fn spawn_editor_menu_panel(commands: &mut Commands, root: Entity) -> Entity {
    let mut panel_entity = Entity::PLACEHOLDER;
    commands.entity(root).with_children(|parent| {
        panel_entity = parent
            .spawn((
                Node {
                    position_type: PositionType::Absolute,
                    width: Val::Px(EDITOR_PANEL_W),
                    flex_direction: FlexDirection::Column,
                    row_gap: Val::Px(5.0),
                    padding: UiRect::all(Val::Px(6.0)),
                    border: UiRect::all(Val::Px(2.0)),
                    ..default()
                },
                BorderColor(ui_theme::button_border(UiButtonTone::Primary)),
                BackgroundColor(ui_theme::panel_bg(0.66)),
                UiEditorPanel,
                Name::new("UiEditorPanel"),
            ))
            .with_children(|panel| {
                spawn_editor_button(panel, "CLOSE", UiEditorCommand::CloseMenu);
                spawn_editor_button(panel, "SETTINGS", UiEditorCommand::OpenSettings);
                spawn_editor_button(panel, "PERF", UiEditorCommand::TogglePerf);
                spawn_editor_button(panel, "GO MOON", UiEditorCommand::GoLuna);
                spawn_editor_button(panel, "GO SPACE", UiEditorCommand::GoSpace);
                spawn_editor_button(panel, "GO EARTH", UiEditorCommand::GoEarth);
                spawn_editor_button(panel, "EDIT UI", UiEditorCommand::Toggle);
                spawn_editor_button(panel, "MOVE", UiEditorCommand::SelectMove);
                spawn_editor_button(panel, "USE", UiEditorCommand::SelectUse);
                spawn_editor_button(panel, "JUMP", UiEditorCommand::SelectJump);
                spawn_editor_button(panel, "-", UiEditorCommand::ScaleDown);
                spawn_editor_button(panel, "+", UiEditorCommand::ScaleUp);
                spawn_editor_button(panel, "RESET", UiEditorCommand::Reset);
                spawn_editor_button(panel, "SAVE", UiEditorCommand::Save);
                #[cfg(debug_assertions)]
                spawn_editor_button(panel, "ASSETS", UiEditorCommand::OpenAssets);
                panel.spawn((
                    Text::new("UI: OFF"),
                    TextFont {
                        font_size: 11.0,
                        ..default()
                    },
                    TextColor(ui_theme::text_primary()),
                    UiEditorLabel,
                ));
            })
            .id();
    });
    panel_entity
}

fn spawn_settings_panel(commands: &mut Commands, root: Entity) -> Entity {
    let mut panel_entity = Entity::PLACEHOLDER;
    commands.entity(root).with_children(|parent| {
        panel_entity = parent
            .spawn((
                Node {
                    position_type: PositionType::Absolute,
                    width: Val::Px(SETTINGS_PANEL_W),
                    flex_direction: FlexDirection::Column,
                    row_gap: Val::Px(4.0),
                    padding: UiRect::all(Val::Px(8.0)),
                    border: UiRect::all(Val::Px(ui_theme::panel_border_width())),
                    ..default()
                },
                BorderColor(ui_theme::accent_ink()),
                BackgroundColor(ui_theme::panel_bg(0.88)),
                SettingsPanel,
                Name::new("SettingsPanel"),
            ))
            .with_children(|panel| {
                panel
                    .spawn((
                        Node {
                            width: Val::Percent(100.0),
                            justify_content: JustifyContent::SpaceBetween,
                            align_items: AlignItems::Center,
                            ..default()
                        },
                        Name::new("SettingsHeader"),
                    ))
                    .with_children(|header| {
                        header.spawn((
                            Text::new("CALIBRATION"),
                            TextFont {
                                font_size: 11.0,
                                ..default()
                            },
                            TextColor(ui_theme::accent_ink()),
                        ));
                        spawn_editor_button(header, "X", UiEditorCommand::CloseSettings);
                    });
                spawn_cal_row(panel, "Camera", CalAdjust::CamSensDown, CalAdjust::CamSensUp);
                spawn_cal_row(panel, "Move", CalAdjust::MoveSensDown, CalAdjust::MoveSensUp);
                spawn_cal_row(panel, "Curve", CalAdjust::CurveDown, CalAdjust::CurveUp);
                spawn_cal_row(panel, "Render", CalAdjust::RenderDown, CalAdjust::RenderUp);
            })
            .id();
    });
    panel_entity
}

fn sync_editor_panels(
    mut commands: Commands,
    state: Res<UiEditorState>,
    root: Query<Entity, With<GameUiRoot>>,
    mut panels: ResMut<EditorPanelsState>,
) {
    let need_editor = (state.menu_open && !state.settings_open) || state.enabled;
    let need_settings = state.menu_open && state.settings_open;

    if !need_editor {
        if let Some(entity) = panels.menu_panel.take() {
            commands.entity(entity).despawn_recursive();
        }
    } else if panels.menu_panel.is_none() {
        let Ok(root) = root.get_single() else {
            return;
        };
        panels.menu_panel = Some(spawn_editor_menu_panel(&mut commands, root));
    }

    if !need_settings {
        if let Some(entity) = panels.settings_panel.take() {
            commands.entity(entity).despawn_recursive();
        }
    } else if panels.settings_panel.is_none() {
        let Ok(root) = root.get_single() else {
            return;
        };
        panels.settings_panel = Some(spawn_settings_panel(&mut commands, root));
    }
}

/// Safety net: only one MENU button may exist (Startup race / hot reload).
fn dedupe_menu_buttons(mut commands: Commands, menus: Query<Entity, With<UiMenuButton>>) {
    let mut iter = menus.iter();
    let Some(keep) = iter.next() else {
        return;
    };
    for extra in iter {
        if extra != keep {
            commands.entity(extra).despawn_recursive();
        }
    }
}

fn spawn_editor_button(parent: &mut ChildBuilder, label: &str, command: UiEditorCommand) {
    parent
        .spawn((
            Button,
            Node {
                width: Val::Px(58.0),
                height: Val::Px(22.0),
                border: UiRect::all(Val::Px(1.5)),
                justify_content: JustifyContent::Center,
                align_items: AlignItems::Center,
                ..default()
            },
            BorderColor(ui_theme::button_border(UiButtonTone::Primary)),
            BackgroundColor(ui_theme::button_bg(
                UiButtonTone::Primary,
                ui_theme::interaction_state(Interaction::None, false),
            )),
            command,
        ))
        .with_children(|button| {
            button.spawn((
                Text::new(label),
                TextFont {
                    font_size: 10.0,
                    ..default()
                },
                TextColor(ui_theme::text_primary()),
            ));
        });
}

fn spawn_cal_row(
    parent: &mut ChildBuilder,
    title: &str,
    down: CalAdjust,
    up: CalAdjust,
) {
    parent
        .spawn((
            Node {
                width: Val::Percent(100.0),
                display: Display::Flex,
                flex_direction: FlexDirection::Row,
                column_gap: Val::Px(4.0),
                align_items: AlignItems::Center,
                ..default()
            },
        ))
        .with_children(|row| {
            row.spawn((
                Text::new(title),
                TextFont {
                    font_size: 9.0,
                    ..default()
                },
                TextColor(ui_theme::text_muted()),
                Node {
                    width: Val::Px(42.0),
                    ..default()
                },
            ));
            spawn_cal_button(row, "-", down);
            row.spawn((
                Text::new("1.00"),
                TextFont {
                    font_size: 9.0,
                    ..default()
                },
                TextColor(ui_theme::text_primary()),
                CalLabel(up),
                Node {
                    width: Val::Px(52.0),
                    justify_content: JustifyContent::Center,
                    ..default()
                },
            ));
            spawn_cal_button(row, "+", up);
        });
}

fn spawn_cal_button(parent: &mut ChildBuilder, label: &str, adjust: CalAdjust) {
    parent.spawn((
        Button,
        Node {
            width: Val::Px(24.0),
            height: Val::Px(20.0),
            border: UiRect::all(Val::Px(1.5)),
            justify_content: JustifyContent::Center,
            align_items: AlignItems::Center,
            ..default()
        },
        BorderColor(ui_theme::button_border(UiButtonTone::Primary)),
        BackgroundColor(ui_theme::button_bg(
            UiButtonTone::Primary,
            ui_theme::interaction_state(Interaction::None, false),
        )),
        adjust,
    ))
    .with_children(|b| {
        b.spawn((
            Text::new(label),
            TextFont {
                font_size: 10.0,
                ..default()
            },
            TextColor(ui_theme::text_primary()),
        ));
    });
}

fn calibration_button_system(
    mut config: ResMut<UiLayoutConfig>,
    mut buttons: Query<(&Interaction, &CalAdjust, &mut BackgroundColor), Changed<Interaction>>,
) {
    for (interaction, adjust, mut bg) in &mut buttons {
        if *interaction == Interaction::Pressed {
            let cal = &mut config.calibration;
            match adjust {
                CalAdjust::CamSensDown => cal.camera_sensitivity -= 0.1,
                CalAdjust::CamSensUp => cal.camera_sensitivity += 0.1,
                CalAdjust::MoveSensDown => cal.movement_sensitivity -= 0.1,
                CalAdjust::MoveSensUp => cal.movement_sensitivity += 0.1,
                CalAdjust::CurveDown => cal.stick_curve_exponent -= 0.1,
                CalAdjust::CurveUp => cal.stick_curve_exponent += 0.1,
                CalAdjust::RenderDown => cal.render_scale -= 0.05,
                CalAdjust::RenderUp => cal.render_scale += 0.05,
            }
            config.calibration = cal.clamp();
            save_ui_layout(&config);
        }
        *bg = BackgroundColor(ui_theme::button_bg(
            UiButtonTone::Primary,
            ui_theme::interaction_state(*interaction, false),
        ));
    }
}

fn cal_label_for(adjust: CalAdjust, cal: &InputCalibration) -> String {
    match adjust {
        CalAdjust::CamSensUp | CalAdjust::CamSensDown => {
            format!("{:.2}", cal.camera_sensitivity)
        }
        CalAdjust::MoveSensUp | CalAdjust::MoveSensDown => {
            format!("{:.2}", cal.movement_sensitivity)
        }
        CalAdjust::CurveUp | CalAdjust::CurveDown => {
            format!("{:.2}", cal.stick_curve_exponent)
        }
        CalAdjust::RenderUp | CalAdjust::RenderDown => {
            format!("{:.0}%", cal.render_scale * 100.0)
        }
    }
}

fn update_settings_labels(
    state: Res<UiEditorState>,
    config: Res<UiLayoutConfig>,
    mut labels: Query<(&CalLabel, &mut Text)>,
) {
    if !state.menu_open {
        return;
    }
    for (label, mut text) in &mut labels {
        let line = cal_label_for(label.0, &config.calibration);
        if text.0 != line {
            text.0 = line;
        }
    }
}

fn editor_button_system(
    mut state: ResMut<UiEditorState>,
    mut config: ResMut<UiLayoutConfig>,
    mut perf_visible: ResMut<PerfOverlayVisible>,
    #[cfg(debug_assertions)] mut asset_dev: ResMut<crate::asset_dev::AssetDevState>,
    mut region: ResMut<RegionRuntime>,
    mut world_ctx: ResMut<WorldContextRuntime>,
    mut moon: ResMut<MoonWorldState>,
    mut space: ResMut<crate::space_world::SpaceWorldState>,
    mut launch_atmo: ResMut<LaunchAtmosphere>,
    mut flight: ResMut<RocketFlight>,
    mut transition: ResMut<FlightTransition>,
    mut cleanup: ResMut<CelestialCleanup>,
    layout: Res<crate::sim::WorldLayout>,
    collision: Res<crate::sim::WorldCollision>,
    mut player: Query<(&mut Transform, &mut SimState, &mut PlayerVisual), With<Player>>,
    mut buttons: Query<(&Interaction, &UiEditorCommand, &mut BackgroundColor), Changed<Interaction>>,
) {
    for (interaction, command, mut bg) in &mut buttons {
        if *interaction == Interaction::Pressed {
            match command {
                UiEditorCommand::TogglePerf => {
                    perf_visible.0 = !perf_visible.0;
                    state.menu_open = false;
                    state.settings_open = false;
                    state.enabled = false;
                    state.active_pointer = None;
                }
                UiEditorCommand::GoLuna => {
                    flight.end_surface();
                    flight.active = false;
                    cleanup.despawn_space = true;
                    cleanup.despawn_moon = false;
                    cleanup.despawn_planet = false;
                    region.active = RegionId::Luna;
                    world_ctx.active = WorldContext::Luna;
                    moon.needs_spawn = true;
                    moon.spawn_after_frames = 0;
                    launch_atmo.active = false;
                    launch_atmo.blend = 0.0;
                    const SPAWN_TX: f32 = 0.0;
                    const SPAWN_TZ: f32 = 0.0;
                    if let Ok((mut tf, mut sim, mut visual)) = player.get_single_mut() {
                        let ground =
                            planet_ground_y_at(CelestialBody::Moon, SPAWN_TX, SPAWN_TZ);
                        sim.body.x = SPAWN_TX;
                        sim.body.z = SPAWN_TZ;
                        sim.body.y = ground;
                        sim.body.vx = 0.0;
                        sim.body.vy = 0.0;
                        sim.body.vz = 0.0;
                        visual.from_x = SPAWN_TX;
                        visual.from_z = SPAWN_TZ;
                        visual.from_y = ground;
                        visual.to_x = SPAWN_TX;
                        visual.to_z = SPAWN_TZ;
                        visual.to_y = ground;
                        let lift = sim.body.radius * 1.1;
                        let wp = planet_gameplay_world_pos(
                            CelestialBody::Moon,
                            SPAWN_TX,
                            SPAWN_TZ,
                            lift,
                        );
                        tf.translation = Vec3::from_array(wp);
                        let dir = ha_shared::luna_dir_from_tangent(SPAWN_TX, SPAWN_TZ);
                        let normal = Vec3::new(dir[0], dir[1], dir[2]).normalize_or_zero();
                        tf.rotation =
                            Quat::from_rotation_arc(Vec3::Y, normal)
                                * Quat::from_rotation_y(sim.body.rot);
                    }
                    transition.begin("Dev → Luna surface");
                    state.menu_open = false;
                    state.settings_open = false;
                    state.enabled = false;
                    eprintln!("ha-luna: dev teleport → surface txz=({SPAWN_TX},{SPAWN_TZ})");
                }
                UiEditorCommand::GoSpace => {
                    let dock = space_dock_origin();
                    flight.begin_coast_toward(dock, Vec3::ZERO, ha_shared::CelestialBody::Earth);
                    region.active = RegionId::Space;
                    world_ctx.active = ha_shared::WorldContext::Space;
                    space.needs_spawn = true;
                    cleanup.despawn_moon = false;
                    launch_atmo.active = false;
                    transition.begin("Dev → orbital dock");
                    state.menu_open = false;
                    state.settings_open = false;
                    state.enabled = false;
                }
                UiEditorCommand::GoEarth => {
                    flight.end_surface();
                    flight.active = false;
                    cleanup.despawn_space = true;
                    cleanup.despawn_moon = true;
                    transition.begin("Dev → Earth pad");
                    launch_atmo.active = false;
                    launch_atmo.blend = 0.0;
                    region.active = RegionId::Overworld { gx: 0, gz: 0 };
                    world_ctx.active = WorldContext::Overworld;
                    if let Ok((mut tf, mut sim, mut visual)) = player.get_single_mut() {
                        let feet = crate::sim::earth_surface_y(LAUNCH_PAD_ZONE_X, LAUNCH_PAD_ZONE_Z);
                        let placed = crate::surface_placement::player_sphere_transform(
                            ha_shared::CelestialBody::Earth,
                            LAUNCH_PAD_ZONE_X,
                            LAUNCH_PAD_ZONE_Z,
                            feet,
                            sim.body.radius,
                            sim.body.rot,
                        );
                        tf.translation = placed.translation;
                        tf.rotation = placed.rotation;
                        sim.body.x = LAUNCH_PAD_ZONE_X;
                        sim.body.y = feet;
                        sim.body.z = LAUNCH_PAD_ZONE_Z;
                        sim.body.vy = 0.0;
                        visual.from_x = sim.body.x;
                        visual.from_z = sim.body.z;
                        visual.from_y = feet;
                        visual.to_x = sim.body.x;
                        visual.to_z = sim.body.z;
                        visual.to_y = feet;
                    }
                    state.menu_open = false;
                    state.settings_open = false;
                    state.enabled = false;
                }
                UiEditorCommand::Menu => {
                    state.menu_open = !state.menu_open;
                    if !state.menu_open {
                        state.enabled = false;
                        state.settings_open = false;
                        state.active_pointer = None;
                    }
                }
                UiEditorCommand::CloseMenu => {
                    state.menu_open = false;
                    state.settings_open = false;
                    state.enabled = false;
                    state.active_pointer = None;
                }
                UiEditorCommand::CloseSettings => {
                    state.settings_open = false;
                }
                UiEditorCommand::OpenSettings => {
                    state.settings_open = !state.settings_open;
                    if state.settings_open {
                        state.enabled = false;
                        state.active_pointer = None;
                    }
                }
                UiEditorCommand::Toggle => state.enabled = !state.enabled,
                UiEditorCommand::SelectMove => {
                    state.enabled = true;
                    state.selected = UiEditable::MoveStick;
                }
                UiEditorCommand::SelectUse => {
                    state.enabled = true;
                    state.selected = UiEditable::UseButton;
                }
                UiEditorCommand::SelectJump => {
                    state.enabled = true;
                    state.selected = UiEditable::JumpButton;
                }
                UiEditorCommand::ScaleDown => {
                    let slot = config.slot_mut(state.selected);
                    slot.scale = (slot.scale - 0.08).clamp(0.72, 1.4);
                    save_ui_layout(&config);
                }
                UiEditorCommand::ScaleUp => {
                    let slot = config.slot_mut(state.selected);
                    slot.scale = (slot.scale + 0.08).clamp(0.72, 1.4);
                    save_ui_layout(&config);
                }
                UiEditorCommand::Reset => {
                    *config = UiLayoutConfig::default();
                    save_ui_layout(&config);
                }
                UiEditorCommand::Save => save_ui_layout(&config),
                #[cfg(debug_assertions)]
                UiEditorCommand::OpenAssets => {
                    asset_dev.panel_open = !asset_dev.panel_open;
                    if asset_dev.panel_open {
                        state.settings_open = false;
                        state.enabled = false;
                    }
                }
            }
        }
        *bg = BackgroundColor(ui_theme::button_bg(
            UiButtonTone::Primary,
            ui_theme::interaction_state(*interaction, false),
        ));
    }
}

fn editor_touch_drag_system(
    mut touches: EventReader<TouchInput>,
    layout: Res<TouchLayout>,
    mut state: ResMut<UiEditorState>,
    mut config: ResMut<UiLayoutConfig>,
) {
    if !state.enabled {
        return;
    }
    let size = editable_size(&layout, state.selected, &config);
    for ev in touches.read() {
        let pos = ev.position;
        match ev.phase {
            TouchPhase::Started => {
                let selected_rect = config.rect(&layout, state.selected, size);
                if point_in_rect(pos, selected_rect) {
                    state.active_pointer = Some(ev.id);
                    continue;
                }
                for editable in [UiEditable::MoveStick, UiEditable::UseButton, UiEditable::JumpButton] {
                    let s = editable_size(&layout, editable, &config);
                    if point_in_rect(pos, config.rect(&layout, editable, s)) {
                        state.selected = editable;
                        state.active_pointer = Some(ev.id);
                        break;
                    }
                }
            }
            TouchPhase::Moved => {
                if state.active_pointer == Some(ev.id) {
                    let s = editable_size(&layout, state.selected, &config);
                    config.set_center_norm(&layout, state.selected, pos, s);
                }
            }
            TouchPhase::Ended | TouchPhase::Canceled => {
                if state.active_pointer == Some(ev.id) {
                    state.active_pointer = None;
                    save_ui_layout(&config);
                }
            }
        }
    }
}

fn update_editor_overlay(
    state: Res<UiEditorState>,
    config: Res<UiLayoutConfig>,
    layout: Res<TouchLayout>,
    mut panel: Query<&mut Node, With<UiEditorPanel>>,
    mut settings: Query<&mut Node, (With<SettingsPanel>, Without<UiEditorPanel>)>,
    mut menu_button: Query<&mut Node, (With<UiMenuButton>, Without<UiEditorPanel>, Without<SettingsPanel>)>,
    mut label: Query<&mut Text, With<UiEditorLabel>>,
    mut editor_blocks: ResMut<EditorTouchBlocks>,
) {
    let menu_top = layout.insets.top as f32 + 8.0;
    if let Ok(mut node) = menu_button.get_single_mut() {
        apply_top_center(&mut node, &layout, MENU_BUTTON_WIDTH, menu_top);
    }
    if let Ok(mut node) = panel.get_single_mut() {
        apply_center(&mut node, &layout, EDITOR_PANEL_W, EDITOR_PANEL_H);
    }
    if let Ok(mut node) = settings.get_single_mut() {
        apply_center(&mut node, &layout, SETTINGS_PANEL_W, SETTINGS_PANEL_H);
    }
    if let Ok(mut text) = label.get_single_mut() {
        text.0 = format!(
            "UI: {}\n{:?} {:.0}%",
            if state.enabled { "EDIT" } else { "OFF" },
            state.selected,
            config.slot(state.selected).scale * 100.0
        );
    }

    editor_blocks.0.clear();
    editor_blocks.0.push(game_ui::menu_screen_rect(&layout));
    if state.menu_open || state.enabled {
        let (w, h) = if state.settings_open {
            (SETTINGS_PANEL_W, SETTINGS_PANEL_H)
        } else {
            (EDITOR_PANEL_W, EDITOR_PANEL_H)
        };
        let left = game_ui::ui_center_left(&layout, w);
        let top = ((layout.height - h) * 0.5).max(layout.insets.top as f32);
        editor_blocks.0.push([left, top, left + w, top + h]);
    }
}

pub fn append_editor_touch_blocks(
    mut blocks: ResMut<TouchBlockRects>,
    editor_blocks: Res<EditorTouchBlocks>,
) {
    blocks.0.extend(editor_blocks.0.iter().copied());
}

fn editable_size(layout: &TouchLayout, editable: UiEditable, config: &UiLayoutConfig) -> Vec2 {
    let r = layout.radius;
    let scale = config.slot(editable).scale;
    match editable {
        UiEditable::MoveStick => Vec2::splat(r * 2.0 * scale),
        UiEditable::UseButton => Vec2::new(r * 1.38 * scale, r * 0.56 * scale),
        UiEditable::JumpButton => Vec2::new(r * 1.08 * scale, r * 0.56 * scale),
    }
}

fn point_in_rect(pos: Vec2, rect: [f32; 4]) -> bool {
    let [left, top, right, bottom] = rect;
    pos.x >= left && pos.x <= right && pos.y >= top && pos.y <= bottom
}

fn save_ui_layout(config: &UiLayoutConfig) {
    let cal = config.calibration.clamp();
    let body = format!(
        "move,{:.5},{:.5},{:.3}\nuse,{:.5},{:.5},{:.3}\njump,{:.5},{:.5},{:.3}\ncal,{:.3},{:.3},{:.3},{:.3}\n",
        config.move_stick.x,
        config.move_stick.y,
        config.move_stick.scale,
        config.use_button.x,
        config.use_button.y,
        config.use_button.scale,
        config.jump_button.x,
        config.jump_button.y,
        config.jump_button.scale,
        cal.camera_sensitivity,
        cal.movement_sensitivity,
        cal.stick_curve_exponent,
        cal.render_scale,
    );
    if let Err(err) = std::fs::write(layout_path(), body) {
        eprintln!("ui layout save failed: {err}");
    }
}

fn load_ui_layout() -> Option<UiLayoutConfig> {
    let text = std::fs::read_to_string(layout_path()).ok()?;
    let mut config = UiLayoutConfig::default();
    for line in text.lines() {
        let mut parts = line.split(',');
        let key = parts.next()?;
        if key == "cal" {
            let cam = parts.next()?.parse::<f32>().ok()?;
            let mv = parts.next()?.parse::<f32>().ok()?;
            let curve = parts.next()?.parse::<f32>().ok()?;
            let render = parts.next()?.parse::<f32>().ok()?;
            config.calibration = InputCalibration {
                camera_sensitivity: cam,
                movement_sensitivity: mv,
                stick_curve_exponent: curve,
                render_scale: render,
            }
            .clamp();
            continue;
        }
        let x = parts.next()?.parse::<f32>().ok()?.clamp(0.02, 0.98);
        let y = parts.next()?.parse::<f32>().ok()?.clamp(0.02, 0.98);
        let scale = parts.next()?.parse::<f32>().ok()?.clamp(0.72, 1.4);
        let slot = UiSlot { x, y, scale };
        match key {
            "move" => config.move_stick = slot,
            "use" => config.use_button = slot,
            "jump" => config.jump_button = slot,
            "atk" => {}
            _ => {}
        }
    }
    Some(config)
}

fn layout_path() -> std::path::PathBuf {
    std::env::var("HA_UI_LAYOUT_FILE")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|_| std::path::PathBuf::from("ui-layout.cfg"))
}
