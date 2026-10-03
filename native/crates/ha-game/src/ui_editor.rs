use bevy::input::touch::{TouchInput, TouchPhase};
use bevy::prelude::*;

use crate::debug::PerfOverlayVisible;
use crate::flight::Flight;
use ha_shared::{body_radius, CelestialBody};
use crate::scene::{teleport_to_body, RegionRuntime, WorldContextRuntime};
use crate::sim::{Player, PlayerVisual, SimState};
use crate::game_ui::{self, apply_center, GameUiRoot};
use crate::input::calibration::InputCalibration;
use crate::input::{TouchBlockRects, TouchLayout};
use crate::ui_theme::{self, UiButtonTone};

pub struct UiEditorPlugin;

impl Plugin for UiEditorPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(load_ui_layout().unwrap_or_default())
            .init_resource::<UiEditorState>()
            .init_resource::<EditorTouchBlocks>()
            .init_resource::<EditorPanelsState>()
            .add_systems(
                Update,
                (
                    sync_editor_panels,
                    editor_button_system,
                    creative_topup_system,
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
            // The bottom-right corner is now the aim stick, so USE/JUMP arc
            // ABOVE it — within the right thumb's reach when it lifts off the
            // stick, clear of both the stick and the centre hotbar.
            use_button: UiSlot::new(0.72, 0.60, 1.0),
            jump_button: UiSlot::new(0.88, 0.50, 1.0),
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
    /// Creative/dev mode: keeps the inventory stocked with the full kit so
    /// every system can be tested without grinding (Minecraft gamemode 1).
    pub creative: bool,
    active_pointer: Option<u64>,
}

impl Default for UiEditorState {
    fn default() -> Self {
        Self {
            enabled: false,
            menu_open: false,
            settings_open: false,
            selected: UiEditable::UseButton,
            creative: false,
            active_pointer: None,
        }
    }
}

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
    FpsDown,
    FpsUp,
    IdiomaDown,
    IdiomaUp,
}

#[derive(Component)]
struct SettingsPanel;

#[derive(Component)]
struct CalLabel(CalAdjust);

#[derive(Component)]
struct UiEditorLabel;

#[derive(Component, Clone, Copy)]
pub enum UiEditorCommand {
    Menu,
    CloseMenu,
    /// Guardar la partida y volver al menú principal (`crate::salir`).
    SalirAlMenu,
    CloseSettings,
    Toggle,
    SelectMove,
    SelectUse,
    SelectJump,
    OpenSettings,
    TogglePerf,
    ToggleProbe,
    ToggleCreative,
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
const SETTINGS_PANEL_W: f32 = 220.0;
const SETTINGS_PANEL_H: f32 = 308.0;
/// Ancho útil adentro del panel (menos el relleno de 8 px por lado y el borde).
const SETTINGS_INNER_W: f32 = SETTINGS_PANEL_W - 20.0;

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
                            Text::new("AJUSTES"),
                            TextFont {
                                font_size: 11.0,
                                ..default()
                            },
                            TextColor(ui_theme::accent_ink()),
                        ));
                        spawn_editor_button(header, "X", UiEditorCommand::CloseSettings);
                    });
                // Volumen arriba: es lo que más se busca en AJUSTES.
                crate::volumen_ui::spawn_slider_volumen(
                    panel,
                    crate::volumen_ui::CanalVolumen::Efectos,
                    SETTINGS_INNER_W,
                    10.0,
                    ui_theme::accent_ink(),
                );
                crate::volumen_ui::spawn_slider_volumen(
                    panel,
                    crate::volumen_ui::CanalVolumen::Musica,
                    SETTINGS_INNER_W,
                    10.0,
                    ui_theme::accent_ink(),
                );
                spawn_cal_row(panel, "Cámara", CalAdjust::CamSensDown, CalAdjust::CamSensUp);
                spawn_cal_row(panel, "Mover", CalAdjust::MoveSensDown, CalAdjust::MoveSensUp);
                spawn_cal_row(panel, "Curva", CalAdjust::CurveDown, CalAdjust::CurveUp);
                spawn_cal_row(panel, "Detalle", CalAdjust::RenderDown, CalAdjust::RenderUp);
                spawn_cal_row(panel, "FPS", CalAdjust::FpsDown, CalAdjust::FpsUp);
                spawn_cal_row(panel, "Idioma", CalAdjust::IdiomaDown, CalAdjust::IdiomaUp);
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
    // The editor menu is UNIFIED into `game_menu.rs` (one menu). We no longer
    // spawn a separate editor panel — edit-UI mode just toggles drag handles.
    if let Some(entity) = panels.menu_panel.take() {
        commands.entity(entity).despawn_recursive();
    }
    let need_settings = state.menu_open && state.settings_open;

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
    mut idioma: ResMut<crate::i18n::Idioma>,
    mut buttons: Query<(&Interaction, &CalAdjust, &mut BackgroundColor), Changed<Interaction>>,
) {
    for (interaction, adjust, mut bg) in &mut buttons {
        if *interaction == Interaction::Pressed {
            match adjust {
                CalAdjust::IdiomaDown | CalAdjust::IdiomaUp => *idioma = idioma.otro(),
                other => {
                    let cal = &mut config.calibration;
                    match other {
                        CalAdjust::CamSensDown => cal.camera_sensitivity -= 0.1,
                        CalAdjust::CamSensUp => cal.camera_sensitivity += 0.1,
                        CalAdjust::MoveSensDown => cal.movement_sensitivity -= 0.1,
                        CalAdjust::MoveSensUp => cal.movement_sensitivity += 0.1,
                        CalAdjust::CurveDown => cal.stick_curve_exponent -= 0.1,
                        CalAdjust::CurveUp => cal.stick_curve_exponent += 0.1,
                        CalAdjust::RenderDown => cal.render_scale -= 0.05,
                        CalAdjust::RenderUp => cal.render_scale += 0.05,
                        CalAdjust::FpsDown | CalAdjust::FpsUp => {
                            use crate::input::calibration::FPS_MAX_OPCIONES as O;
                            let i = O.iter().position(|v| *v == cal.fps_max).unwrap_or(0);
                            let n = O.len();
                            let j = if matches!(other, CalAdjust::FpsUp) { (i + 1) % n } else { (i + n - 1) % n };
                            cal.fps_max = O[j];
                        }
                        _ => {}
                    }
                    config.calibration = cal.clamp();
                    save_ui_layout(&config);
                }
            }
        }
        *bg = BackgroundColor(ui_theme::button_bg(
            UiButtonTone::Primary,
            ui_theme::interaction_state(*interaction, false),
        ));
    }
}

fn cal_label_for(adjust: CalAdjust, cal: &InputCalibration, idioma: crate::i18n::Idioma) -> String {
    match adjust {
        CalAdjust::IdiomaUp | CalAdjust::IdiomaDown => idioma.nombre().to_string(),
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
        CalAdjust::FpsUp | CalAdjust::FpsDown => match cal.fps_max {
            0 => "libre".into(),
            n => format!("{n}"),
        },
    }
}

fn update_settings_labels(
    state: Res<UiEditorState>,
    config: Res<UiLayoutConfig>,
    idioma: Res<crate::i18n::Idioma>,
    mut labels: Query<(&CalLabel, &mut Text)>,
) {
    if !state.menu_open {
        return;
    }
    for (label, mut text) in &mut labels {
        let line = cal_label_for(label.0, &config.calibration, *idioma);
        if text.0 != line {
            text.0 = line;
        }
    }
}

/// Creative mode (gamemode 1): while ON, keep the inventory topped up with the
/// full kit so every system — crafting, building, machines, flight — can be
/// tested instantly without grinding. Runs every ~2 s (cheap, gated OFF).
fn creative_topup_system(
    time: Res<Time>,
    state: Res<UiEditorState>,
    mut inv_ui: ResMut<crate::hud::inventory_ui::InventoryUiState>,
    mut inv: ResMut<crate::inventory_persist::PlayerInventory>,
    mut persist: ResMut<crate::inventory_persist::InventoryPersistState>,
    mut timer: Local<f32>,
) {
    if !state.creative {
        return;
    }
    *timer += time.delta_secs();
    if *timer < 2.0 {
        return;
    }
    *timer = 0.0;

    // Materials kept near a target so slots stay stocked but not overflowing.
    const MATERIALS: &[(&str, u32)] = &[
        ("wood", 64), ("stone", 64), ("sticks", 64), ("dirt", 64),
        ("iron_ingot", 64), ("gold_ingot", 32), ("gem_blue", 16), ("gem_red", 16),
    ];
    // Tools + special items given once (stack 1 — only if missing).
    const TOOLS: &[&str] = &[
        "wood_pickaxe", "stone_pickaxe", "pickaxe", "wood_axe", "stone_axe",
        "hammer", "lantern", "jetpack", "probe", "shield_wood",
        // Ítems "en la mano" (`held_item.rs`) + almacenamiento: en creativo se
        // prueban al instante sin craftear.
        "sword_iron", "sword_steel",
        // Armas de fuego (una de cada, con camuflaje 115 y Bacon para ver los aspectos)
        "gun_pistola", "gun_thompson__115", "gun_ppsh__bacon", "gun_mp40__singularidad", "gun_ak__grafiti",
        "gun_escopeta__ciclon", "gun_raygun__oscura",
    ];
    // Consumibles/colocables que conviene tener en pila, no de a uno.
    const KIT_STACKS: &[(&str, u32)] = &[("torch", 16), ("chest", 4), ("ammo_45", 60), ("ammo_9mm", 60), ("ammo_762", 60), ("ammo_cartucho", 24), ("ammo_energia", 40)];

    let mut changed = false;
    for &(id, target) in MATERIALS {
        if inv.qty(id) < target {
            let need = target - inv.qty(id);
            crate::hud::inventory_ui::add_item_to_slots(&mut inv_ui, id, need);
            changed = true;
        }
    }
    for &id in TOOLS {
        if inv.qty(id) == 0 {
            crate::hud::inventory_ui::add_item_to_slots(&mut inv_ui, id, 1);
            changed = true;
        }
    }
    for &(id, target) in KIT_STACKS {
        if inv.qty(id) < target {
            let need = target - inv.qty(id);
            crate::hud::inventory_ui::add_item_to_slots(&mut inv_ui, id, need);
            changed = true;
        }
    }
    if changed {
        crate::hud::inventory_ui::flush_slots_to_inventory(&inv_ui, &mut inv);
        persist.mark_immediate();
    }
}

fn editor_button_system(
    mut state: ResMut<UiEditorState>,
    mut config: ResMut<UiLayoutConfig>,
    mut perf_visible: ResMut<PerfOverlayVisible>,
    mut region: ResMut<RegionRuntime>,
    mut world_ctx: ResMut<WorldContextRuntime>,
    mut planet: ResMut<crate::world2d::ActivePlanet>,
    mut trans: ResMut<crate::transition::FlightTransition>,
    mut flight: ResMut<Flight>,
    mut fly: ResMut<crate::creative_fly::CreativeFlight>,
    (mut frame, mut salida): (ResMut<crate::universe::UniverseFrame>, ResMut<crate::salir::SalidaAlMenu>),
    mut player: Query<(&mut SimState, &mut PlayerVisual), With<Player>>,
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
                // La sonda escribe a logcat, no a pantalla: el menú se cierra
                // igual que con PERF para poder jugar el escenario que se mide.
                UiEditorCommand::ToggleProbe => {
                    crate::debug::set_stage_probe(!crate::debug::stage_probe_on());
                    state.menu_open = false;
                    state.settings_open = false;
                    state.enabled = false;
                    state.active_pointer = None;
                }
                UiEditorCommand::ToggleCreative => {
                    // Flag only — `creative_topup_system` does the stocking so
                    // this system stays under Bevy's 16-param limit.
                    state.creative = !state.creative;
                    state.menu_open = false;
                    state.settings_open = false;
                    state.enabled = false;
                    state.active_pointer = None;
                    eprintln!("ha-creative: {}", if state.creative { "ON" } else { "OFF" });
                }
                UiEditorCommand::GoLuna => {
                    flight.end_surface();
                    trans.clear();
                    if let Ok((mut sim, mut visual)) = player.get_single_mut() {
                        teleport_to_body(
                            CelestialBody::Moon,
                            &mut frame,
                            &mut region,
                            &mut world_ctx,
                            &mut planet,
                            &mut sim,
                            &mut visual,
                        );
                    }
                    state.menu_open = false;
                    state.settings_open = false;
                    state.enabled = false;
                    eprintln!("ha-luna: dev teleport → Luna strip");
                }
                UiEditorCommand::GoSpace => {
                    // Marco único: "ir al espacio" es SÓLO subir. El jugador se
                    // queda donde está (encima de la Tierra), y la región la
                    // deriva `creative_fly::sync_frame_region` de la altura.
                    flight.end_surface();
                    fly.body = CelestialBody::Earth;
                    fly.alt = body_radius(CelestialBody::Earth) * 1.4;
                    fly.vy = 0.0;
                    fly.phase = crate::creative_fly::Phase::Orbit;
                    fly.land_armed = true;
                    state.menu_open = false;
                    state.settings_open = false;
                    state.enabled = false;
                    eprintln!("ha-space: dev teleport → orbita creativa sobre la Tierra");
                }
                UiEditorCommand::GoEarth => {
                    flight.end_surface();
                    trans.clear();
                    if let Ok((mut sim, mut visual)) = player.get_single_mut() {
                        teleport_to_body(
                            CelestialBody::Earth,
                            &mut frame,
                            &mut region,
                            &mut world_ctx,
                            &mut planet,
                            &mut sim,
                            &mut visual,
                        );
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
                UiEditorCommand::SalirAlMenu => {
                    state.menu_open = false;
                    state.settings_open = false;
                    state.enabled = false;
                    state.active_pointer = None;
                    salida.pedida = true;
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
                    // Asset dev viewer retired with the 3D asset factory.
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
    mut label: Query<&mut Text, With<UiEditorLabel>>,
    mut editor_blocks: ResMut<EditorTouchBlocks>,
) {
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
        "move,{:.5},{:.5},{:.3}\nuse,{:.5},{:.5},{:.3}\njump,{:.5},{:.5},{:.3}\ncal,{:.3},{:.3},{:.3},{:.3},{}\n",
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
        cal.fps_max,
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
            // Campo nuevo (29 sep 2026): los archivos viejos no lo traen.
            let fps_max = parts
                .next()
                .and_then(|v| v.trim().parse::<u32>().ok())
                .unwrap_or(crate::input::calibration::DEFAULT_FPS_MAX);
            config.calibration = InputCalibration {
                camera_sensitivity: cam,
                movement_sensitivity: mv,
                stick_curve_exponent: curve,
                render_scale: render,
                fps_max,
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
    crate::flags::var("HA_UI_LAYOUT_FILE")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|_| std::path::PathBuf::from("ui-layout.cfg"))
}
