use bevy::prelude::*;
use bevy::render::camera::{CameraOutputMode, ClearColorConfig};
use bevy::render::render_resource::BlendState;
use bevy::ui::IsDefaultUiCamera;

use bevy::ui::widget::ImageNode;

use crate::game_ui::{
    self, apply_bottom_center, ensure_game_ui_root, sync_game_ui_root, fullscreen_block_rect,
    GameUiRoot, GameUiRootReady, UiGameplayBlock,
};
use crate::debug::{span_guard, CpuSpanSlot, CpuSpanStats};
use crate::input::{CraftIntent, EquippedItem, HoldInteractState, AbilityCooldown, TouchActions, TouchLayout, TouchBlockRects};
use crate::inventory_persist::{InventoryPersistState, PlayerInventory};
use crate::launch_pad::LaunchPadWorld;
use crate::rocket_flight::RocketFlight;
use crate::machines::{MachineWorld, PendingMachinePlacement};
use crate::scene::{RegionId, RegionRuntime};
use crate::sim::{Player, SimState};
use crate::targeting::LookTarget;
use ha_shared::{
    can_assemble_module, can_craft, craft, get_craft_recipe, item_display_name, item_hotbar_label,
    require_machine, resolve_interaction, CraftRecipe, InteractActionKind,
    InteractTargetKind, STAMINA_MAX, CRAFT_RECIPES, STONE_EXTRACTOR, WOOD_EXTRACTOR, LAUNCH_PAD,
    LAUNCH_PAD_MODULE_COUNT,
};
use crate::ui_theme::{self, UiButtonState, UiButtonTone};
use crate::ghibli_atmosphere::PlayerLanternControl;
use crate::platform;

mod icons;
mod item_icons;
pub mod inventory_ui;
mod survival_bars;

use icons::{
    energy_image_for_units, heart_image_for_units, spawn_stat_energy, spawn_stat_heart, HudIconAtlas,
    HudIconPlugin, StatEnergyIcon, StatHeartIcon,
};
use item_icons::ItemIconCache;
use inventory_ui::{
    add_item_to_slots, consume_from_ui_slots, ensure_inventory_ui_state, flush_slots_to_inventory,
    grid_signature, inventory_drop_while_held_system, inventory_held_world_drop_system,
    inventory_long_press_system, inventory_signature, inventory_slot_press_system,
    inventory_slot_release_system, load_grid_from_inventory, merge_inv_gains_into_ui, InventoryGridRoot, InventoryGridSlot, InventorySlotIcon,
    InventorySlotQty, InventoryUiState, HotbarItemSlot, HotbarSlotIcon, HotbarSlotName, HotbarSlotQty, SlotId,
    InventorySlotName,
    HeldItemGhost, GRID_COLS, GRID_ROWS, HOTBAR_ITEM_SLOTS, HOTBAR_SLOTS_TOTAL,
};

#[derive(Component)]
struct LanternToggleRoot;

#[derive(Component)]
struct LanternToggleButton;

#[derive(Component)]
struct LanternToggleLabel;

pub struct PixelHudPlugin;

impl Plugin for PixelHudPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(HudIconPlugin)
            .init_resource::<GameUiRootReady>()
            .init_resource::<HudSpawned>()
            .init_resource::<HudTune>()
            .init_resource::<StaminaMeter>()
            .init_resource::<BagPanelState>()
            .init_resource::<MachinePanelState>()
            .init_resource::<ItemIconCache>()
            .init_resource::<InventoryUiState>()
            .init_resource::<UiGameplayBlock>()
            .init_resource::<HudTouchBlocks>()
            .init_resource::<HudDisplayCache>()
            .init_resource::<survival_bars::SurvivalBarsCache>()
            .add_systems(Startup, (spawn_hud_camera, ensure_game_ui_root))
            .add_systems(Update, (
                sync_game_ui_root,
                spawn_hud_once.after(sync_game_ui_root),
                update_hud_layout,
                update_floating_panels_layout,
                update_hud_state,
                update_survival_hud,
                machine_panel_system.after(update_survival_hud),
                sync_ui_gameplay_block,
                bag_hotbar_system,
                bag_panel_system.after(bag_hotbar_system),
                ensure_inventory_ui_state,
                inventory_slot_press_system,
                inventory_long_press_system,
                inventory_drop_while_held_system.after(inventory_long_press_system),
                inventory_slot_release_system,
                inventory_held_world_drop_system.after(inventory_drop_while_held_system),
                sync_inventory_from_player,
                update_inventory_slot_visuals,
                update_hotbar_item_visuals,
                sync_hotbar_chrome_icons,
            ))
            .add_systems(Update, (
                equip_from_hotbar_selection,
                equip_from_hotbar_click,
                equip_toast_system,
                hotbar_hammer_system,
                sync_lantern_equipped,
                lantern_toggle_system,
                update_lantern_toggle_ui,
                update_held_item_ghost,
                scroll_craft_list,
            ))
            .add_systems(Update, survival_bars::update_survival_bars)
            .add_systems(
                Update,
                (
                    append_hud_touch_blocks.after(crate::ui_editor::append_editor_touch_blocks),
                    update_interact_hud
                        .after(crate::machines::try_place_on_hold)
                        .after(crate::launch_pad::try_assemble_on_hold)
                        .after(crate::launch_pad::try_ignite_on_hold)
                        .after(crate::space_travel::try_lunar_return_to_space),
                ),
            );
    }
}

#[derive(Component)]
struct HudCamera;

#[derive(Component)]
struct HotbarRoot;

#[derive(Component)]
struct HotbarCraftButton;

#[derive(Component)]
struct HotbarSlot {
    disabled: bool,
}

#[derive(Component)]
struct BagPanelRoot;

#[derive(Component)]
struct ModalBackdrop;

#[derive(Component)]
struct ModalLayerRoot;

#[derive(Component)]
struct StatusPromptRoot;

#[derive(Resource, Default)]
struct HudTouchBlocks(pub Vec<[f32; 4]>);

#[derive(Component)]
struct BagPanelCloseButton;

#[derive(Clone, Copy, PartialEq, Eq, Default)]
pub(crate) enum BagTab {
    #[default]
    Inventory,
    Craft,
    /// Ship builder — part counters + live stats (`ship_builder.rs`).
    Ship,
}

#[derive(Component, Clone, Copy)]
struct BagTabButton {
    tab: BagTab,
}

#[derive(Component)]
struct BagInventorySection;

#[derive(Component)]
struct BagCraftSection;

#[derive(Component)]
struct CraftRecipeButton {
    recipe_id: u32,
    is_machine_select: bool,
}

#[derive(Component)]
struct CraftRecipeCostText {
    recipe_id: u32,
    is_machine_select: bool,
}

#[derive(Component)]
struct BagPanelBase {
    width: f32,
    height: f32,
}

#[derive(Component)]
pub(crate) struct HoldBarRoot;

#[derive(Component)]
pub(crate) struct HoldBarFill;

#[derive(Component)]
pub(crate) struct AbilityCooldownRing;

#[derive(Component)]
struct InteractPromptText;

#[derive(Component)]
struct MachinePanelRoot;

#[derive(Component)]
struct MachinePanelTitle;

#[derive(Component)]
struct MachinePanelBody;

#[derive(Component)]
struct MachinePanelCloseButton;

#[derive(Component)]
struct MachinePanelBase {
    width: f32,
    height: f32,
}

#[derive(Component)]
struct CraftStockText;

#[derive(Component)]
struct HotbarHammerSlot;

#[derive(Component)]
struct HotbarHammerIcon;

#[derive(Component)]
struct HotbarBagIcon;

#[derive(Resource, Default)]
struct HudSpawned(bool);

#[derive(Resource, Default)]
struct HudTune {
    heart_px: f32,
    energy_px: f32,
}

#[derive(Resource)]
struct StaminaMeter {
    value: f32,
    max: f32,
}

impl Default for StaminaMeter {
    fn default() -> Self {
        Self {
            value: STAMINA_MAX,
            max: STAMINA_MAX,
        }
    }
}


#[derive(Resource)]
pub(crate) struct BagPanelState {
    pub(crate) open: bool,
    pub(crate) tab: BagTab,
    pulse: Timer,
    modal: Option<Entity>,
}

#[derive(Resource, Default)]
struct MachinePanelState {
    panel: Option<Entity>,
}

impl Default for BagPanelState {
    fn default() -> Self {
        Self {
            open: false,
            tab: BagTab::Inventory,
            pulse: Timer::from_seconds(0.15, TimerMode::Once),
            modal: None,
        }
    }
}

#[derive(Resource, Default)]
pub(crate) struct HudDisplayCache {
    initialized: bool,
    heart_units: i32,
    energy_units: i32,
    hold_visible: bool,
    hold_pct: f32,
    cooldown_visible: bool,
    cooldown_alpha: f32,
    prompt_line: String,
    craft_stock_line: String,
    machine_panel_visible: bool,
    machine_panel_line: String,
    machine_panel_dismissed: bool,
    inv_signature: String,
    prompt_is_idle: bool,
    prompt_alpha: f32,
}

fn spawn_hud_camera(mut commands: Commands) {
    commands.spawn((
        Camera2d,
        Camera {
            order: 1,
            // 3D camera (order 0) clears the swapchain each frame; UI only blends widgets on top.
            // Never use a full-screen boot overlay without despawning — ghost text otherwise.
            clear_color: ClearColorConfig::None,
            output_mode: CameraOutputMode::Write {
                blend_state: Some(BlendState::ALPHA_BLENDING),
                clear_color: ClearColorConfig::None,
            },
            ..default()
        },
        IsDefaultUiCamera,
        HudCamera,
        Name::new("HudCamera"),
    ));
}

fn spawn_hud_once(
    mut commands: Commands,
    layout: Res<TouchLayout>,
    root: Query<Entity, With<GameUiRoot>>,
    mut spawned: ResMut<HudSpawned>,
    mut tune: ResMut<HudTune>,
    player: Query<&SimState, With<Player>>,
    atlas: Res<HudIconAtlas>,
) {
    if spawned.0 {
        return;
    }
    if player.get_single().is_err() || layout.width < 100.0 || !atlas.ready {
        return;
    }
    let Ok(root) = root.get_single() else {
        return;
    };
    spawned.0 = true;

    let short = layout.width.min(layout.height);
    let margin = (short * 0.02).clamp(10.0, 20.0);
    let heart = (short * 0.03).clamp(15.0, 24.0);
    let gap = (heart * 0.15).clamp(2.0, 4.0);
    tune.heart_px = heart;
    tune.energy_px = heart;

    commands.entity(root).with_children(|parent| {
        parent
            .spawn((
                Node {
                    position_type: PositionType::Absolute,
                    left: Val::Px(layout.insets.left as f32 + margin),
                    top: Val::Px(layout.insets.top as f32 + margin),
                    display: Display::Flex,
                    flex_direction: FlexDirection::Column,
                    row_gap: Val::Px(gap * 1.6),
                    ..default()
                },
                Name::new("HudTopLeft"),
            ))
            .with_children(|panel| {
                panel
                    .spawn((
                        Node {
                            display: Display::Flex,
                            column_gap: Val::Px(gap),
                            ..default()
                        },
                        Name::new("HeartRow"),
                    ))
                    .with_children(|row| {
                        for idx in 0..6 {
                            spawn_stat_heart(row, idx, heart, &atlas);
                        }
                    });
                survival_bars::spawn_survival_bars(panel, heart, gap);
                spawn_status_prompt_block(panel, heart, gap);
            });

        parent
            .spawn((
                Node {
                    position_type: PositionType::Absolute,
                    right: Val::Px(layout.insets.right as f32 + margin),
                    top: Val::Px(layout.insets.top as f32 + margin),
                    display: Display::Flex,
                    flex_direction: FlexDirection::Row,
                    align_items: AlignItems::FlexEnd,
                    column_gap: Val::Px(gap),
                    ..default()
                },
                Name::new("EnergyTopRight"),
            ))
            .with_children(|row| {
                for idx in 0..6 {
                    spawn_stat_energy(row, idx, heart, &atlas);
                }
            });

        spawn_hotbar(parent, &layout, short, heart);
        spawn_equip_toast(parent, &layout, heart);
        spawn_lantern_toggle(parent, &layout, short);
        spawn_interact_hud(parent, &layout, short);
    });
}

fn spawn_status_prompt_block(parent: &mut ChildBuilder, heart: f32, gap: f32) {
    let border = ui_theme::panel_border_width();
    parent
        .spawn((
            Node {
                display: Display::Flex,
                flex_direction: FlexDirection::Column,
                row_gap: Val::Px(gap * 0.6),
                padding: UiRect::axes(Val::Px(8.0), Val::Px(6.0)),
                border: UiRect::all(Val::Px(border)),
                min_width: Val::Px(180.0),
                max_width: Val::Px(320.0),
                ..default()
            },
            BorderColor(ui_theme::accent_ink()),
            BackgroundColor(ui_theme::panel_bg(0.88)),
            StatusPromptRoot,
            Name::new("StatusPromptBlock"),
        ))
        .with_children(|block| {
            block.spawn((
                Text::new(platform::gameplay_controls_hint()),
                TextFont {
                    font_size: (heart * 0.58).clamp(10.0, 14.0),
                    ..default()
                },
                TextColor(ui_theme::text_primary()),
                InteractPromptText,
            ));
            block
                .spawn((
                    Node {
                        width: Val::Percent(100.0),
                        height: Val::Px(10.0),
                        display: Display::None,
                        border: UiRect::all(Val::Px(2.0)),
                        ..default()
                    },
                    BorderColor(ui_theme::ink_outline()),
                    BackgroundColor(ui_theme::panel_bg(0.55)),
                    HoldBarRoot,
                    Name::new("HoldInteractBar"),
                ))
                .with_children(|bar| {
                    bar.spawn((
                        Node {
                            width: Val::Percent(0.0),
                            height: Val::Percent(100.0),
                            ..default()
                        },
                        BackgroundColor(ui_theme::accent_cyan()),
                        HoldBarFill,
                    ));
                });
        });
}

fn spawn_machine_panel(commands: &mut Commands, root: Entity, layout: &TouchLayout, heart: f32) -> Entity {
    let short = layout.width.min(layout.height);
    let panel_w = (short * 0.48).clamp(240.0, 360.0);
    let panel_h = (short * 0.28).clamp(140.0, 220.0);
    let left = game_ui::ui_center_left(layout, panel_w);
    let top = ((layout.height - panel_h) * 0.5).max(layout.insets.top as f32);
    let mut panel_entity = Entity::PLACEHOLDER;
    commands.entity(root).with_children(|parent| {
        panel_entity = parent
            .spawn((
                Node {
                    position_type: PositionType::Absolute,
                    left: Val::Px(left),
                    top: Val::Px(top),
                    width: Val::Px(panel_w),
                    height: Val::Px(panel_h),
                    padding: UiRect::all(Val::Px(10.0)),
                    border: UiRect::all(Val::Px(ui_theme::panel_border_width())),
                    flex_direction: FlexDirection::Column,
                    row_gap: Val::Px(6.0),
                    overflow: Overflow::clip(),
                    ..default()
                },
                BorderColor(ui_theme::accent_ink()),
                BackgroundColor(ui_theme::panel_bg_lit(0.0, 0.92)),
                MachinePanelRoot,
                MachinePanelBase {
                    width: panel_w,
                    height: panel_h,
                },
                Name::new("MachineControlPanel"),
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
                        Name::new("MachinePanelHeader"),
                    ))
                    .with_children(|header| {
                        header.spawn((
                            Text::new("CONTROL PANEL"),
                            TextFont {
                                font_size: (heart * 0.62).clamp(11.0, 16.0),
                                ..default()
                            },
                            TextColor(ui_theme::accent_ink()),
                            MachinePanelTitle,
                        ));
                        spawn_panel_close_button(header, heart, MachinePanelCloseButton);
                    });
                panel.spawn((
                    Text::new("Approach a machine"),
                    TextFont {
                        font_size: (heart * 0.54).clamp(10.0, 14.0),
                        ..default()
                    },
                    TextColor(ui_theme::text_muted()),
                    MachinePanelBody,
                ));
            })
            .id();
    });
    panel_entity
}

fn hotbar_layout_metrics(layout: &TouchLayout) -> (f32, f32, f32, f32) {
    let short = layout.width.min(layout.height);
    let margin_h = layout.insets.left as f32 + layout.insets.right as f32 + 12.0;
    let usable = (layout.width - margin_h).max(220.0);
    let gap = (short * 0.004).clamp(2.0, 4.0);
    let slot_ideal = (short * 0.068).clamp(34.0, 58.0);
    let total_ideal =
        slot_ideal * HOTBAR_SLOTS_TOTAL as f32 + gap * (HOTBAR_SLOTS_TOTAL as f32 - 1.0);
    let slot = if total_ideal > usable {
        ((usable - gap * (HOTBAR_SLOTS_TOTAL as f32 - 1.0)) / HOTBAR_SLOTS_TOTAL as f32)
            .clamp(30.0, 58.0)
    } else {
        slot_ideal
    };
    let total = slot * HOTBAR_SLOTS_TOTAL as f32 + gap * (HOTBAR_SLOTS_TOTAL as f32 - 1.0);
    let bottom = layout.insets.bottom as f32 + (short * 0.022).clamp(8.0, 18.0);
    (slot, gap, total, bottom)
}

fn spawn_hotbar(parent: &mut ChildBuilder, layout: &TouchLayout, _short: f32, heart: f32) {
    let (slot, gap, total, bottom) = hotbar_layout_metrics(layout);
    parent
        .spawn((
            Node {
                position_type: PositionType::Absolute,
                left: Val::Px(game_ui::ui_center_left(layout, total)),
                bottom: Val::Px(bottom),
                width: Val::Px(total),
                height: Val::Px(slot),
                display: Display::Flex,
                flex_direction: FlexDirection::Row,
                align_items: AlignItems::Center,
                justify_content: JustifyContent::Center,
                column_gap: Val::Px(gap),
                ..default()
            },
            HotbarRoot,
            Name::new("Hotbar"),
        ))
        .with_children(|bar| {
            for i in 0..HOTBAR_SLOTS_TOTAL {
                let is_bag = i == HOTBAR_SLOTS_TOTAL - 1;
                let is_hammer = i == 0;
                let is_item = (1..HOTBAR_SLOTS_TOTAL - 1).contains(&i);
                let enabled = is_bag || is_hammer || is_item;
                let mut e = bar.spawn((
                    Button,
                    Node {
                        width: Val::Px(slot),
                        height: Val::Px(slot),
                        min_width: Val::Px(slot),
                        min_height: Val::Px(slot),
                        flex_shrink: 0.0,
                        border: UiRect::all(Val::Px(ui_theme::panel_border_width() - 1.0)),
                        justify_content: JustifyContent::Center,
                        align_items: AlignItems::Center,
                        overflow: Overflow::clip(),
                        ..default()
                    },
                    BorderColor(if is_hammer {
                        ui_theme::accent_cyan()
                    } else {
                        ui_theme::button_border(UiButtonTone::Inventory)
                    }),
                    BackgroundColor(ui_theme::button_bg(
                        UiButtonTone::Inventory,
                        if enabled {
                            UiButtonState::Normal
                        } else {
                            UiButtonState::Disabled
                        },
                    )),
                    HotbarSlot { disabled: !enabled },
                ));
                if is_hammer {
                    e.insert(HotbarHammerSlot);
                    e.with_children(|slot_node| {
                        slot_node.spawn((
                            Node {
                                width: Val::Percent(86.0),
                                height: Val::Percent(86.0),
                                ..default()
                            },
                            HotbarHammerIcon,
                        ));
                    });
                } else if is_bag {
                    e.insert(HotbarCraftButton);
                    e.with_children(|slot_node| {
                        slot_node.spawn((
                            Node {
                                width: Val::Percent(86.0),
                                height: Val::Percent(86.0),
                                ..default()
                            },
                            HotbarBagIcon,
                        ));
                    });
                } else if is_item {
                    let idx = i - 1;
                    e.insert(HotbarItemSlot { index: idx });
                    e.with_children(|slot_node| {
                        slot_node.spawn((
                            Node {
                                width: Val::Percent(100.0),
                                height: Val::Percent(100.0),
                                flex_direction: FlexDirection::Column,
                                justify_content: JustifyContent::SpaceBetween,
                                align_items: AlignItems::Center,
                                padding: UiRect::vertical(Val::Px(1.0)),
                                ..default()
                            },
                        ))
                        .with_children(|col| {
                            col.spawn((
                                Node {
                                    width: Val::Percent(78.0),
                                    height: Val::Percent(68.0),
                                    ..default()
                                },
                                HotbarSlotIcon,
                            ));
                            col.spawn((
                                Text::new(""),
                                TextFont {
                                    font_size: (heart * 0.28).clamp(6.0, 8.0),
                                    ..default()
                                },
                                TextColor(ui_theme::text_muted()),
                                HotbarSlotName,
                            ));
                        });
                        slot_node.spawn((
                            Text::new(""),
                            TextFont {
                                font_size: (heart * 0.34).clamp(7.0, 10.0),
                                ..default()
                            },
                            TextColor(ui_theme::text_primary()),
                            HotbarSlotQty,
                        ));
                    });
                }
            }
        });
}

fn spawn_lantern_toggle(parent: &mut ChildBuilder, layout: &TouchLayout, short: f32) {
    let (_, _, total, bottom) = hotbar_layout_metrics(layout);
    let btn_h = (short * 0.052).clamp(28.0, 40.0);
    let btn_w = btn_h * 2.1;
    parent
        .spawn((
            Node {
                position_type: PositionType::Absolute,
                left: Val::Px(game_ui::ui_center_left(layout, total) + total + 8.0),
                bottom: Val::Px(bottom),
                width: Val::Px(btn_w),
                height: Val::Px(btn_h),
                display: Display::None,
                ..default()
            },
            LanternToggleRoot,
            Name::new("LanternToggle"),
        ))
        .with_children(|root| {
            root.spawn((
                Button,
                Node {
                    width: Val::Percent(100.0),
                    height: Val::Percent(100.0),
                    border: UiRect::all(Val::Px(2.0)),
                    justify_content: JustifyContent::Center,
                    align_items: AlignItems::Center,
                    ..default()
                },
                BorderColor(ui_theme::accent_cyan()),
                BackgroundColor(ui_theme::button_bg(
                    UiButtonTone::Use,
                    UiButtonState::Normal,
                )),
                LanternToggleButton,
            ))
            .with_children(|btn| {
                btn.spawn((
                    Text::new("LAMP ON"),
                    TextFont {
                        font_size: (btn_h * 0.28).clamp(9.0, 12.0),
                        ..default()
                    },
                    TextColor(ui_theme::text_primary()),
                    LanternToggleLabel,
                ));
            });
        });
}

fn update_hud_layout(
    layout: Res<TouchLayout>,
    mut hotbar: Query<&mut Node, (With<HotbarRoot>, Without<HotbarSlot>)>,
    mut slots: Query<&mut Node, (With<HotbarSlot>, Without<HotbarRoot>)>,
    mut hud_blocks: ResMut<HudTouchBlocks>,
    mut span: ResMut<CpuSpanStats>,
    mut last: Local<(f32, f32, f32, f32)>,
) {
    let _guard = span_guard(&mut span, CpuSpanSlot::Hud);
    if layout.width < 100.0 {
        return;
    }
    let (slot, gap, total, bottom) = hotbar_layout_metrics(&layout);
    let key = (layout.width, layout.height, total, bottom);
    if *last == key {
        return;
    }
    *last = key;
    if let Ok(mut node) = hotbar.get_single_mut() {
        apply_bottom_center(&mut node, &layout, total, bottom);
        node.width = Val::Px(total);
        node.height = Val::Px(slot);
        node.column_gap = Val::Px(gap);
        let left = game_ui::ui_center_left(&layout, total);
        let top = layout.height - bottom - slot;
        hud_blocks.0 = vec![[left, top, left + total, top + slot]];
    }
    for mut slot_node in &mut slots {
        slot_node.width = Val::Px(slot);
        slot_node.height = Val::Px(slot);
        slot_node.min_width = Val::Px(slot);
        slot_node.min_height = Val::Px(slot);
    }
}

fn update_hud_state(
    player: Query<&SimState, With<Player>>,
    atlas: Res<HudIconAtlas>,
    mut stamina: ResMut<StaminaMeter>,
    mut cache: ResMut<HudDisplayCache>,
    mut hearts: Query<(&StatHeartIcon, &mut ImageNode), Without<StatEnergyIcon>>,
    mut energy: Query<(&StatEnergyIcon, &mut ImageNode), Without<StatHeartIcon>>,
    mut span: ResMut<CpuSpanStats>,
) {
    let _guard = span_guard(&mut span, CpuSpanSlot::Hud);
    let Ok(sim) = player.get_single() else {
        return;
    };
    if !atlas.ready {
        return;
    }

    stamina.value = sim.body.stamina;
    stamina.max = sim.body.max_stamina;

    let hp_ratio = if sim.max_hp > 0.0 {
        (sim.hp / sim.max_hp).clamp(0.0, 1.0)
    } else {
        0.0
    };
    let heart_units = (hp_ratio * 12.0).round() as i32;
    let energy_units = if stamina.max > 0.0 {
        ((stamina.value / stamina.max).clamp(0.0, 1.0) * 12.0).round() as i32
    } else {
        0
    };

    if !cache.initialized || cache.heart_units != heart_units {
        cache.heart_units = heart_units;
        for (icon, mut image) in &mut hearts {
            let units = (heart_units - icon.idx * 2).clamp(0, 2);
            image.image = heart_image_for_units(units, &atlas);
        }
    }
    if !cache.initialized || cache.energy_units != energy_units {
        cache.energy_units = energy_units;
        for (icon, mut image) in &mut energy {
            let units = (energy_units - icon.idx * 2).clamp(0, 2);
            image.image = energy_image_for_units(units, &atlas);
        }
    }
    cache.initialized = true;
}

fn update_survival_hud(
    player: Query<&Transform, With<Player>>,
    equipped: Res<EquippedItem>,
    hold: Res<HoldInteractState>,
    target: Res<LookTarget>,
    region: Res<RegionRuntime>,
    pending: Res<PendingMachinePlacement>,
    inv: Res<PlayerInventory>,
    pads: Res<LaunchPadWorld>,
    flight: Res<RocketFlight>,
    loadout: Res<crate::rocket_flight::ShipLoadout>,
    mut cache: ResMut<HudDisplayCache>,
    mut prompts: Query<(&mut Text, &mut TextColor), With<InteractPromptText>>,
    mut prompt_roots: Query<(&mut BackgroundColor, &mut BorderColor), With<StatusPromptRoot>>,
) {
    let player_pos = player.get_single().ok().map(|tf| tf.translation);
    let prompt_line = build_interact_prompt(
        &equipped,
        &hold,
        &target,
        &region,
        &pending,
        &inv,
        &pads,
        &flight,
        &loadout,
        player_pos,
    );
    if cache.prompt_line != prompt_line {
        cache.prompt_line = prompt_line.clone();
        for (mut text, _) in &mut prompts {
            text.0 = prompt_line.clone();
        }
    }

    let idle_hint = prompt_line.starts_with('←')
        || prompt_line.starts_with("WASD")
        || prompt_line.contains("Move · Look");
    let target_alpha = if hold.active || !idle_hint { 0.92 } else { 0.34 };
    if cache.prompt_is_idle != idle_hint || (cache.prompt_alpha - target_alpha).abs() > 0.02 {
        cache.prompt_is_idle = idle_hint;
        cache.prompt_alpha = target_alpha;
        for (mut bg, mut border) in &mut prompt_roots {
            *bg = BackgroundColor(ui_theme::panel_bg(target_alpha * 0.82));
            *border = BorderColor(ui_theme::accent_ink().with_alpha(target_alpha));
        }
        for (_, mut color) in &mut prompts {
            *color = TextColor(ui_theme::text_primary().with_alpha(if idle_hint && !hold.active {
                0.55
            } else {
                0.95
            }));
        }
    }
}

fn machine_panel_system(
    layout: Res<TouchLayout>,
    tune: Res<HudTune>,
    root: Query<Entity, With<GameUiRoot>>,
    player: Query<&Transform, With<Player>>,
    inv: Res<PlayerInventory>,
    machines: Res<MachineWorld>,
    mut cache: ResMut<HudDisplayCache>,
    mut panel_state: ResMut<MachinePanelState>,
    mut commands: Commands,
    mut panel_body: Query<&mut Text, With<MachinePanelBody>>,
    mut close_btn: Query<
        (&Interaction, &mut BackgroundColor),
        (With<MachinePanelCloseButton>, With<Button>, Changed<Interaction>),
    >,
) {
    let player_pos = player.get_single().ok().map(|tf| tf.translation);
    let machine_panel = build_machine_panel_text(player_pos, &inv, &machines);
    let near_machine = machine_panel.is_some();
    if !near_machine {
        cache.machine_panel_dismissed = false;
    }
    let panel_visible = near_machine && !cache.machine_panel_dismissed;

    if !panel_visible {
        if let Some(entity) = panel_state.panel.take() {
            commands.entity(entity).despawn_recursive();
        }
        cache.machine_panel_visible = false;
        return;
    }

    if panel_state.panel.is_none() {
        let Ok(root) = root.get_single() else {
            return;
        };
        panel_state.panel = Some(spawn_machine_panel(
            &mut commands,
            root,
            &layout,
            tune.heart_px,
        ));
        cache.machine_panel_visible = true;
    }

    if let Some(line) = machine_panel {
        if cache.machine_panel_line != line {
            cache.machine_panel_line = line.clone();
            for mut text in &mut panel_body {
                text.0 = line.clone();
            }
        }
    }

    for (interaction, mut bg) in &mut close_btn {
        if *interaction == Interaction::Pressed {
            cache.machine_panel_dismissed = true;
        }
        *bg = BackgroundColor(ui_theme::button_bg(
            UiButtonTone::Primary,
            ui_theme::interaction_state(*interaction, false),
        ));
    }
}

fn update_floating_panels_layout(
    layout: Res<TouchLayout>,
    mut machine: Query<(&mut Node, &MachinePanelBase), With<MachinePanelRoot>>,
) {
    if layout.width < 100.0 {
        return;
    }
    if let Ok((mut node, base)) = machine.get_single_mut() {
        game_ui::apply_center(&mut node, &layout, base.width, base.height);
    }
}

const MACHINE_PANEL_RANGE: f32 = 4.2;

fn format_inventory_line(inv: &PlayerInventory) -> String {
    let entries = inv.sorted_entries();
    if entries.is_empty() {
        return "Stock: (empty)".to_string();
    }
    let parts: Vec<String> = entries
        .iter()
        .map(|(id, qty)| format!("{}×{qty}", item_label(id)))
        .collect();
    format!("Stock: {}", parts.join(" · "))
}

fn item_label(id: &str) -> &str {
    match id {
        "wood" => "W",
        "stone" => "S",
        "sticks" => "T",
        _ => id,
    }
}

fn format_ingredients(ingredients: &[(&str, u32)]) -> String {
    ingredients
        .iter()
        .map(|(id, qty)| format!("{}×{qty}", item_label(id)))
        .collect::<Vec<_>>()
        .join(" ")
}

fn recipe_status(inv: &PlayerInventory, recipe: &CraftRecipe) -> (bool, String) {
    if can_craft(inv, recipe) {
        return (true, format_ingredients(recipe.ingredients));
    }
    let missing: Vec<String> = recipe
        .ingredients
        .iter()
        .filter_map(|(id, need)| {
            let have = inv.qty(id);
            if have < *need {
                Some(format!("{}×{}", item_label(id), need - have))
            } else {
                None
            }
        })
        .collect();
    (false, format!("Need {}", missing.join(", ")))
}

/// Touch-drag + mouse-wheel scroll for the BAG panels (CRAFT recipe list AND
/// the inventory grid). Both had `Overflow::scroll_y()` + `ScrollPosition` but
/// no input driver, so content below the fold was unreachable on mobile.
/// BAG-open gates gameplay input, so any drag while open scrolls. The hidden
/// tab's ScrollPosition is harmless to touch.
fn scroll_craft_list(
    bag: Res<BagPanelState>,
    touches: Res<Touches>,
    mut wheel: EventReader<bevy::input::mouse::MouseWheel>,
    mut sections: Query<
        &mut ScrollPosition,
        Or<(With<BagCraftSection>, With<BagInventorySection>)>,
    >,
) {
    if !bag.open {
        wheel.clear();
        return;
    }
    let mut dy = 0.0f32;
    for ev in wheel.read() {
        dy -= ev.y * 26.0;
    }
    for t in touches.iter() {
        dy -= t.delta().y;
    }
    if dy.abs() < 0.5 {
        return;
    }
    for mut sp in &mut sections {
        // Generous content bound (12 recipes / 8 inventory rows + machines).
        sp.offset_y = (sp.offset_y + dy).clamp(0.0, 900.0);
    }
}

fn build_machine_panel_text(
    player_pos: Option<Vec3>,
    inv: &PlayerInventory,
    world: &MachineWorld,
) -> Option<String> {
    let pos = player_pos?;
    let mut nearby: Vec<_> = world
        .placed
        .iter()
        .filter(|m| {
            let dx = m.x - pos.x;
            let dz = m.z - pos.z;
            (dx * dx + dz * dz).sqrt() <= MACHINE_PANEL_RANGE
        })
        .collect();
    if nearby.is_empty() {
        return None;
    }
    nearby.sort_by(|a, b| {
        let da = (a.x - pos.x).powi(2) + (a.z - pos.z).powi(2);
        let db = (b.x - pos.x).powi(2) + (b.z - pos.z).powi(2);
        da.partial_cmp(&db).unwrap_or(std::cmp::Ordering::Equal)
    });
    let mut lines = vec![format!(
        "Your stock: W{} S{} T{}",
        inv.qty("wood"),
        inv.qty("stone"),
        inv.qty("sticks")
    )];
    for m in nearby.iter().take(3) {
        let def = require_machine(m.machine_id);
        lines.push(format!(
            "{} — stored {} {} ({}/s)",
            def.display_name,
            m.stored_output,
            def.output_item_id,
            def.output_per_second
        ));
    }
    Some(lines.join("\n"))
}

fn build_interact_prompt(
    equipped: &EquippedItem,
    hold: &HoldInteractState,
    target: &LookTarget,
    region: &RegionRuntime,
    pending: &PendingMachinePlacement,
    inv: &PlayerInventory,
    pads: &LaunchPadWorld,
    flight: &RocketFlight,
    loadout: &crate::rocket_flight::ShipLoadout,
    player_pos: Option<Vec3>,
) -> String {
    if flight.active {
        let alt = flight.altitude_m();
        let spd = flight.speed();
        let fuel = flight.fuel_pct();
        let body = flight.body.label();
        let vac = if flight.in_vacuum() { "VAC" } else { "ATM" };
        let chute = if flight.parachute_open { " · CHUTE" } else { "" };
        let retro = if flight.phase == ha_shared::FlightPhase::Retro {
            " · RC RETRO"
        } else {
            ""
        };
        let ap = if flight.autopilot { " · AUTOPILOT" } else { "" };
        // Autonomy = seconds of continuous thrust left at this ship's mass.
        let autonomy =
            flight.fuel / (ha_shared::flight_physics::ROCKET_FUEL_BURN_PER_S * flight.burn_rate_factor()).max(0.01);
        return format!(
            "FLIGHT — {body} · alt {alt:.0}m · {spd:.0} m/s · fuel {fuel:.0}% · masa {:.1}t · ~{autonomy:.0}s thrust · {vac}{chute}{retro}{ap} · ACTION thrust · SHIFT/jump retro",
            loadout.mass_t
        );
    }

    if hold.active && hold.placing_machine {
        let def = require_machine(pending.machine_id);
        let pct = if hold.duration_ms > 0 {
            (hold.progress_ms as f32 / hold.duration_ms as f32 * 100.0) as u32
        } else {
            0
        };
        return format!("BUILD {} — {}%", def.display_name, pct);
    }

    if hold.active && hold.eating {
        let pct = if hold.duration_ms > 0 {
            (hold.progress_ms as f32 / hold.duration_ms as f32 * 100.0) as u32
        } else {
            0
        };
        return format!(
            "EATING {} — {}%",
            ha_shared::item_display_name(&equipped.item_id),
            pct
        );
    }
    if target.kind.is_none()
        && !hold.active
        && ha_shared::food_restore(&equipped.item_id).is_some()
        && inv.qty(&equipped.item_id) > 0
    {
        return format!(
            "HOLD → Eat {}",
            ha_shared::item_display_name(&equipped.item_id)
        );
    }

    if let Some(pos) = player_pos {
        if matches!(region.active, RegionId::Cave(_)) {
            return "USE → Exit cave".to_string();
        }
        if pos.distance(region.cave_portal) <= 3.2 {
            return "USE → Descend into the core".to_string();
        }
    }

    if let Some(kind) = target.kind {
        if kind == InteractTargetKind::LaunchIgnition {
            if let Some(pad) = nearest_launch_pad(pads, target.hit_point) {
                if !pad.ready {
                    return format!(
                        "Need starship modules {}/{}",
                        pad.assembled_modules, LAUNCH_PAD_MODULE_COUNT
                    );
                }
            }
        }
        if kind == InteractTargetKind::LaunchModule {
            if let Some((pad, idx)) = nearest_launch_module_slot(pads, target.hit_point) {
                if pad.assembled_modules > idx {
                    return format!("Module {} installed", idx + 1);
                }
                if !can_assemble_module(pad, idx, inv) {
                    return format!("Need materials for module {}", idx + 1);
                }
            }
        }
        if kind == InteractTargetKind::Block {
            if ha_shared::is_block_item(&equipped.item_id) {
                return format!(
                    "HOLD → Place {}",
                    ha_shared::item_display_name(&equipped.item_id)
                );
            }
            return "HOLD → Break block".to_string();
        }
        if let Some(rule) = resolve_interaction(Some(equipped.item_id.as_str()), kind) {
            return match rule.kind {
                InteractActionKind::Gather => {
                    format!("HOLD → Gather {}", rule.item_id.unwrap_or("resource"))
                }
                InteractActionKind::Mine => {
                    format!("HOLD → Mine {}", rule.item_id.unwrap_or("stone"))
                }
                InteractActionKind::Collect => "HOLD → Collect".to_string(),
                InteractActionKind::Assemble if kind == InteractTargetKind::SpaceStationModule => {
                    "HOLD → Install station module".to_string()
                }
                InteractActionKind::Assemble => "HOLD → Install starship module".to_string(),
                InteractActionKind::Launch => "HOLD → LAUNCH to orbit".to_string(),
                InteractActionKind::ReturnEarth if kind == InteractTargetKind::LunarReturn => {
                    "HOLD → Return to orbital hub".to_string()
                }
                InteractActionKind::ReturnEarth => "HOLD → Re-enter Earth".to_string(),
                InteractActionKind::TravelLuna => {
                    // Earth→Moon distance is effectively fixed (~52 km flight
                    // metres) — cost preview without needing the clock.
                    let secs = ha_shared::transit_cruise_secs(52_000.0);
                    let cost = ha_shared::transit_fuel_cost(secs, flight.burn_rate_factor());
                    format!(
                        "HOLD → AUTOPILOT Luna · ~{secs:.0}s · {cost:.0} fuel (tenés {:.0}) · requiere nav data",
                        flight.fuel
                    )
                }
                InteractActionKind::TravelBody => {
                    "HOLD → AUTOPILOT a planeta · coste según distancia · requiere nav data".to_string()
                }
                InteractActionKind::LaunchProbe => {
                    if inv.qty("probe") > 0 {
                        "HOLD → Launch nav probe".to_string()
                    } else {
                        "Need Nav Probe — BAG → CRAFT".to_string()
                    }
                }
                InteractActionKind::Attack => "TAP → Ability".to_string(),
                InteractActionKind::None => match kind {
                    InteractTargetKind::Rock
                    | InteractTargetKind::OreIron
                    | InteractTargetKind::OreGold => {
                        "Need pickaxe — BAG → CRAFT".to_string()
                    }
                    _ => "No action".to_string(),
                },
            };
        }
    }

    if ha_shared::is_block_item(&equipped.item_id) && target.place_cell.is_some() {
        return format!(
            "HOLD → Place {}",
            ha_shared::item_display_name(&equipped.item_id)
        );
    }

    if equipped.item_id == "hammer" {
        let def = require_machine(pending.machine_id);
        let can = inv.can_afford_machine(def);
        if can {
            return format!(
                "HOLD (empty) → Place {} · W{} S{} T{}",
                def.display_name, def.cost_wood, def.cost_stone, def.cost_sticks
            );
        }
        return format!(
            "Need W{} S{} T{} for {}",
            def.cost_wood, def.cost_stone, def.cost_sticks, def.display_name
        );
    }

    if region.active == RegionId::Space {
        return "Vacío orbital · ACTION thrust · quemar hacia Tierra/Luna · muelle sobre pad".to_string();
    }
    if region.active == RegionId::Luna {
        return "Superficie lunar · ACTION thrust → despegar hacia el espacio".to_string();
    }

    platform::gameplay_controls_hint().to_string()
}

fn nearest_launch_pad<'a>(
    pads: &'a LaunchPadWorld,
    hit: Vec3,
) -> Option<&'a ha_shared::LaunchPadState> {
    pads.pads.iter().min_by(|a, b| {
        let da = hit.distance(Vec3::new(a.x, a.y, a.z));
        let db = hit.distance(Vec3::new(b.x, b.y, b.z));
        da.partial_cmp(&db).unwrap_or(std::cmp::Ordering::Equal)
    })
}

fn nearest_launch_module_slot<'a>(
    pads: &'a LaunchPadWorld,
    hit: Vec3,
) -> Option<(&'a ha_shared::LaunchPadState, u8)> {
    let offsets = [
        (0u8, -8.0_f32, 8.0_f32),
        (1u8, 8.0_f32, 8.0_f32),
        (2u8, 0.0_f32, -8.0_f32),
    ];
    let slot_lift = crate::surface_placement::PAD_SURFACE_LIFT + 0.55;
    let mut best: Option<(&ha_shared::LaunchPadState, u8, f32)> = None;
    for pad in &pads.pads {
        for (idx, ox, oz) in offsets {
            let slot = crate::surface_placement::sphere_world_pos(
                ha_shared::CelestialBody::Earth,
                pad.x + ox,
                pad.z + oz,
                slot_lift,
            );
            let d = hit.distance(slot);
            if d > 2.5 {
                continue;
            }
            if best.map(|(_, _, bd)| d < bd).unwrap_or(true) {
                best = Some((pad, idx, d));
            }
        }
    }
    best.map(|(pad, idx, _)| (pad, idx))
}

fn spawn_interact_hud(parent: &mut ChildBuilder, layout: &TouchLayout, short: f32) {
    let ring = (short * 0.055).clamp(28.0, 44.0);
    parent.spawn((
        Node {
            position_type: PositionType::Absolute,
            left: Val::Px(layout.width * 0.58),
            top: Val::Px(layout.insets.top as f32 + (short * 0.22).clamp(72.0, 120.0)),
            width: Val::Px(ring),
            height: Val::Px(ring),
            border: UiRect::all(Val::Px(ui_theme::panel_border_width())),
            // Hidden until an ability is actually on cooldown (the update
            // system reveals it) — it was showing as a stray magenta square.
            display: Display::None,
            ..default()
        },
        BorderColor(ui_theme::accent_magenta()),
        BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.35)),
        AbilityCooldownRing,
        Name::new("AbilityCooldownRing"),
    ));
}

fn spawn_panel_close_button(parent: &mut ChildBuilder, heart: f32, marker: impl Component) {
    parent
        .spawn((
            Button,
            Node {
                width: Val::Px((heart * 1.1).clamp(24.0, 32.0)),
                height: Val::Px((heart * 1.1).clamp(24.0, 32.0)),
                border: UiRect::all(Val::Px(2.0)),
                justify_content: JustifyContent::Center,
                align_items: AlignItems::Center,
                ..default()
            },
            BorderColor(ui_theme::button_border(UiButtonTone::Primary)),
            BackgroundColor(ui_theme::button_bg(UiButtonTone::Primary, UiButtonState::Normal)),
            marker,
        ))
        .with_children(|btn| {
            btn.spawn((
                Text::new("X"),
                TextFont {
                    font_size: (heart * 0.62).clamp(11.0, 16.0),
                    ..default()
                },
                TextColor(ui_theme::text_primary()),
            ));
        });
}

fn spawn_bag_tab_button(parent: &mut ChildBuilder, label: &str, tab: BagTab, heart: f32) {
    parent
        .spawn((
            Button,
            Node {
                flex_grow: 1.0,
                height: Val::Px((heart * 0.95).clamp(22.0, 30.0)),
                border: UiRect::all(Val::Px(2.0)),
                justify_content: JustifyContent::Center,
                align_items: AlignItems::Center,
                ..default()
            },
            BorderColor(ui_theme::button_border(UiButtonTone::Inventory)),
            BackgroundColor(ui_theme::button_bg(UiButtonTone::Inventory, UiButtonState::Normal)),
            BagTabButton { tab },
        ))
        .with_children(|btn| {
            btn.spawn((
                Text::new(label),
                TextFont {
                    font_size: (heart * 0.52).clamp(10.0, 14.0),
                    ..default()
                },
                TextColor(ui_theme::text_primary()),
            ));
        });
}

fn spawn_bag_panel(
    commands: &mut Commands,
    root: Entity,
    layout: &TouchLayout,
    tune: &HudTune,
    tab: BagTab,
    ship_state: &crate::ship_builder::ShipBuilderState,
    inv: &PlayerInventory,
) -> Entity {
    let short = layout.width.min(layout.height);
    let heart = tune.heart_px;
    let panel_w = (short * 0.74).clamp(340.0, 540.0);
    let panel_h = (short * 0.72).clamp(340.0, 520.0);
    let cell = (panel_w * 0.19).clamp(52.0, 72.0);
    let cell_gap = (cell * 0.08).clamp(4.0, 8.0);

    let mut modal_entity = Entity::PLACEHOLDER;
    commands.entity(root).with_children(|parent| {
        modal_entity = parent
            .spawn((
                Node {
                    position_type: PositionType::Absolute,
                    left: Val::Px(0.0),
                    top: Val::Px(0.0),
                    width: Val::Px(layout.width),
                    height: Val::Px(layout.height),
                    ..default()
                },
                ModalLayerRoot,
                Name::new("BagModalLayer"),
            ))
            .with_children(|layer| {
                layer.spawn((
                    Button,
                    Node {
                        width: Val::Percent(100.0),
                        height: Val::Percent(100.0),
                        ..default()
                    },
                    BackgroundColor(Color::srgba(0.02, 0.03, 0.08, 0.52)),
                    ModalBackdrop,
                    Name::new("ModalBackdrop"),
                ));

                layer
                    .spawn((
                        Node {
                            position_type: PositionType::Absolute,
                            left: Val::Px(game_ui::ui_center_left(layout, panel_w)),
                            top: Val::Px(((layout.height - panel_h) * 0.5).max(layout.insets.top as f32)),
                            width: Val::Px(panel_w),
                            height: Val::Px(panel_h),
                            border: UiRect::all(Val::Px(ui_theme::panel_border_width())),
                            padding: UiRect::all(Val::Px((heart * 0.4).clamp(6.0, 14.0))),
                            flex_direction: FlexDirection::Column,
                            row_gap: Val::Px((heart * 0.35).clamp(5.0, 10.0)),
                            overflow: Overflow::clip(),
                            ..default()
                        },
                        BorderColor(ui_theme::accent_ink()),
                        BackgroundColor(ui_theme::panel_bg_lit(0.0, 0.90)),
                        BagPanelRoot,
                        BagPanelBase {
                            width: panel_w,
                            height: panel_h,
                        },
                        Name::new("BagPanel"),
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
                                Name::new("BagPanelHeader"),
                            ))
                            .with_children(|header| {
                                header.spawn((
                                    Text::new("BAG"),
                                    TextFont {
                                        font_size: (heart * 0.72).clamp(12.0, 20.0),
                                        ..default()
                                    },
                                    TextColor(ui_theme::text_primary()),
                                ));
                                spawn_panel_close_button(header, heart, BagPanelCloseButton);
                            });

                        panel
                            .spawn((
                                Node {
                                    width: Val::Percent(100.0),
                                    flex_direction: FlexDirection::Row,
                                    column_gap: Val::Px(6.0),
                                    ..default()
                                },
                                Name::new("BagTabs"),
                            ))
                            .with_children(|tabs| {
                                spawn_bag_tab_button(tabs, "INVENTORY", BagTab::Inventory, heart);
                                spawn_bag_tab_button(tabs, "CRAFT", BagTab::Craft, heart);
                                spawn_bag_tab_button(tabs, "SHIP", BagTab::Ship, heart);
                            });

                        panel
                            .spawn((
                                Node {
                                    width: Val::Percent(100.0),
                                    flex_grow: 1.0,
                                    flex_direction: FlexDirection::Column,
                                    row_gap: Val::Px(cell_gap),
                                    display: if tab == BagTab::Inventory {
                                        Display::Flex
                                    } else {
                                        Display::None
                                    },
                                    overflow: Overflow::scroll_y(),
                                    ..default()
                                },
                                BagInventorySection,
                                ScrollPosition::default(),
                            ))
                            .with_children(|section| {
                                section.spawn((
                                    Text::new("Tap move · Tap again split · Hold drag"),
                                    TextFont {
                                        font_size: (heart * 0.46).clamp(9.0, 12.0),
                                        ..default()
                                    },
                                    TextColor(ui_theme::text_muted()),
                                ));
                                section
                                    .spawn((
                                        Node {
                                            width: Val::Percent(100.0),
                                            flex_direction: FlexDirection::Column,
                                            row_gap: Val::Px(cell_gap),
                                            ..default()
                                        },
                                        InventoryGridRoot,
                                    ))
                                    .with_children(|grid| {
                                        for row in 0..GRID_ROWS {
                                            grid.spawn(Node {
                                                width: Val::Percent(100.0),
                                                flex_direction: FlexDirection::Row,
                                                column_gap: Val::Px(cell_gap),
                                                ..default()
                                            })
                                            .with_children(|row_node| {
                                                for col in 0..GRID_COLS {
                                                    let index = row * GRID_COLS + col;
                                                    spawn_inventory_grid_cell(
                                                        row_node,
                                                        index,
                                                        cell,
                                                        heart,
                                                    );
                                                }
                                            });
                                        }
                                    });
                            });

                        panel
                            .spawn((
                                Node {
                                    width: Val::Percent(100.0),
                                    flex_grow: 1.0,
                                    flex_direction: FlexDirection::Column,
                                    row_gap: Val::Px((heart * 0.28).clamp(4.0, 8.0)),
                                    display: if tab == BagTab::Craft {
                                        Display::Flex
                                    } else {
                                        Display::None
                                    },
                                    overflow: Overflow::scroll_y(),
                                    ..default()
                                },
                                BagCraftSection,
                                ScrollPosition::default(),
                            ))
                            .with_children(|section| {
                                // All recipes are craftable from obtainable
                                // materials now (the old id 2/3 skip hid the
                                // wood pickaxe/axe after the recipe renumber).
                                for recipe in CRAFT_RECIPES {
                                    spawn_recipe_button(section, recipe, heart);
                                }
                                spawn_machine_select_button(
                                    section,
                                    "Wood Extractor",
                                    10,
                                    machine_cost_label(&WOOD_EXTRACTOR),
                                    heart,
                                );
                                spawn_machine_select_button(
                                    section,
                                    "Stone Extractor",
                                    11,
                                    machine_cost_label(&STONE_EXTRACTOR),
                                    heart,
                                );
                                spawn_machine_select_button(
                                    section,
                                    "Launch Pad",
                                    12,
                                    machine_cost_label(&LAUNCH_PAD),
                                    heart,
                                );
                                spawn_machine_select_button(
                                    section,
                                    "Regolith Drill (Luna)",
                                    13,
                                    machine_cost_label(&ha_shared::REGOLITH_DRILL),
                                    heart,
                                );
                                spawn_machine_select_button(
                                    section,
                                    "Antenna (Luna)",
                                    14,
                                    machine_cost_label(&ha_shared::ANTENNA),
                                    heart,
                                );
                            });

                        crate::ship_builder::spawn_ship_section(
                            panel,
                            heart,
                            tab == BagTab::Ship,
                            ship_state,
                            inv,
                        );
                    });
            })
            .id();
    });
    modal_entity
}

fn spawn_inventory_grid_cell(
    parent: &mut ChildBuilder,
    index: usize,
    cell: f32,
    heart: f32,
) {
    parent
        .spawn((
            Button,
            Node {
                width: Val::Px(cell),
                height: Val::Px(cell),
                border: UiRect::all(Val::Px(2.0)),
                justify_content: JustifyContent::Center,
                align_items: AlignItems::Center,
                overflow: Overflow::clip(),
                ..default()
            },
            BorderColor(ui_theme::button_border(UiButtonTone::Inventory)),
            BackgroundColor(ui_theme::button_bg(UiButtonTone::Inventory, UiButtonState::Normal)),
            InventoryGridSlot { index },
        ))
        .with_children(|slot| {
            slot.spawn((
                Node {
                    width: Val::Percent(100.0),
                    height: Val::Percent(100.0),
                    flex_direction: FlexDirection::Column,
                    justify_content: JustifyContent::SpaceBetween,
                    align_items: AlignItems::Center,
                    padding: UiRect::vertical(Val::Px(2.0)),
                    ..default()
                },
            ))
            .with_children(|col| {
                col.spawn((
                    Node {
                        width: Val::Percent(82.0),
                        height: Val::Percent(72.0),
                        ..default()
                    },
                    InventorySlotIcon,
                ));
                col.spawn((
                    Text::new(""),
                    TextFont {
                        font_size: (heart * 0.32).clamp(7.0, 10.0),
                        ..default()
                    },
                    TextColor(ui_theme::text_muted()),
                    InventorySlotName,
                ));
            });
            slot.spawn((
                Text::new(""),
                TextFont {
                    font_size: (heart * 0.42).clamp(9.0, 12.0),
                    ..default()
                },
                TextColor(ui_theme::text_primary()),
                InventorySlotQty,
            ));
        });
}

fn despawn_bag_panel(commands: &mut Commands, bag_state: &mut BagPanelState) {
    if let Some(entity) = bag_state.modal.take() {
        commands.entity(entity).despawn_recursive();
    }
}

fn spawn_recipe_button(panel: &mut ChildBuilder, recipe: &CraftRecipe, heart: f32) {
    panel.spawn((
        Button,
        Node {
            width: Val::Percent(100.0),
            min_height: Val::Px((heart * 1.0).clamp(22.0, 36.0)),
            border: UiRect::all(Val::Px(2.0)),
            justify_content: JustifyContent::SpaceBetween,
            align_items: AlignItems::Center,
            padding: UiRect::axes(Val::Px(8.0), Val::Px(4.0)),
            overflow: Overflow::clip(),
            ..default()
        },
        BorderColor(ui_theme::button_border(UiButtonTone::Craft)),
        BackgroundColor(ui_theme::button_bg(UiButtonTone::Craft, UiButtonState::Normal)),
        CraftRecipeButton {
            recipe_id: recipe.id,
            is_machine_select: false,
        },
    ))
    .with_children(|btn| {
        btn.spawn((
            Node {
                max_width: Val::Percent(48.0),
                overflow: Overflow::clip(),
                ..default()
            },
            Name::new("RecipeTitleWrap"),
        ))
        .with_children(|wrap| {
            wrap.spawn((
                Text::new(recipe.display_name),
                TextFont {
                    font_size: (heart * 0.54).clamp(10.0, 15.0),
                    ..default()
                },
                TextColor(ui_theme::text_primary()),
            ));
        });
        btn.spawn((
            Node {
                max_width: Val::Percent(48.0),
                overflow: Overflow::clip(),
                ..default()
            },
            Name::new("RecipeCostWrap"),
        ))
        .with_children(|wrap| {
            wrap.spawn((
                Text::new(format_ingredients(recipe.ingredients)),
                TextFont {
                    font_size: (heart * 0.48).clamp(9.0, 13.0),
                    ..default()
                },
                TextColor(ui_theme::text_muted()),
                CraftRecipeCostText {
                    recipe_id: recipe.id,
                    is_machine_select: false,
                },
            ));
        });
    });
}

/// "W5 S2 T1 I4" — only nonzero costs, so lunar kit shows its iron.
fn machine_cost_label(def: &ha_shared::MachineDef) -> String {
    let mut parts: Vec<String> = Vec::new();
    for (tag, qty) in [
        ("W", def.cost_wood),
        ("S", def.cost_stone),
        ("T", def.cost_sticks),
        ("I", def.cost_iron),
    ] {
        if qty > 0 {
            parts.push(format!("{tag}{qty}"));
        }
    }
    parts.join(" ")
}

fn spawn_machine_select_button(
    panel: &mut ChildBuilder,
    label: &str,
    recipe_id: u32,
    cost_label: String,
    heart: f32,
) {
    panel.spawn((
        Button,
        Node {
            width: Val::Percent(100.0),
            height: Val::Px((heart * 1.0).clamp(20.0, 34.0)),
            border: UiRect::all(Val::Px(2.0)),
            justify_content: JustifyContent::SpaceBetween,
            align_items: AlignItems::Center,
            padding: UiRect::axes(Val::Px(8.0), Val::Px(0.0)),
            ..default()
        },
        BorderColor(ui_theme::button_border(UiButtonTone::Craft)),
        BackgroundColor(ui_theme::button_bg(UiButtonTone::Craft, UiButtonState::Normal)),
        CraftRecipeButton {
            recipe_id,
            is_machine_select: true,
        },
    ))
    .with_children(|btn| {
        btn.spawn((
            Text::new(label),
            TextFont {
                font_size: (heart * 0.58).clamp(11.0, 17.0),
                ..default()
            },
            TextColor(ui_theme::text_primary()),
        ));
        btn.spawn((
            Text::new(cost_label),
            TextFont {
                font_size: (heart * 0.52).clamp(10.0, 15.0),
                ..default()
            },
            TextColor(ui_theme::text_muted()),
            CraftRecipeCostText {
                recipe_id,
                is_machine_select: true,
            },
        ));
    });
}

fn bag_hotbar_system(
    keys: Res<ButtonInput<KeyCode>>,
    editor: Res<crate::ui_editor::UiEditorState>,
    mut bag_state: ResMut<BagPanelState>,
    mut ui: ResMut<InventoryUiState>,
    mut equipped: ResMut<EquippedItem>,
    mut hotbar: Query<
        (&Interaction, &HotbarSlot, &mut BackgroundColor),
        (With<HotbarCraftButton>, Changed<Interaction>),
    >,
) {
    let desktop = !platform::uses_touch_gameplay_ui();
    if desktop && !editor.enabled && !editor.menu_open {
        if keys.just_pressed(KeyCode::KeyI) {
            bag_state.open = !bag_state.open;
            if bag_state.open {
                bag_state.tab = BagTab::Inventory;
            }
        }
        for (key, idx) in [
            (KeyCode::Digit1, 0usize),
            (KeyCode::Digit2, 1),
            (KeyCode::Digit3, 2),
            (KeyCode::Digit4, 3),
            (KeyCode::Digit5, 4),
            (KeyCode::Digit6, 5),
            (KeyCode::Digit7, 6),
            (KeyCode::Digit8, 7),
        ] {
            if keys.just_pressed(key) {
                select_hotbar_slot(&mut ui, &mut equipped, idx);
            }
        }
    }

    for (interaction, slot, mut bg) in &mut hotbar {
        if *interaction == Interaction::Pressed {
            let opening = !bag_state.open;
            bag_state.open = opening;
            if opening {
                bag_state.tab = BagTab::Inventory;
            }
        }
        *bg = BackgroundColor(ui_theme::button_bg(
            UiButtonTone::Inventory,
            ui_theme::interaction_state(*interaction, slot.disabled),
        ));
    }

    if keys.just_pressed(KeyCode::Escape) && bag_state.open {
        bag_state.open = false;
    }
}

fn select_hotbar_slot(
    ui: &mut InventoryUiState,
    equipped: &mut EquippedItem,
    idx: usize,
) {
    if idx >= HOTBAR_ITEM_SLOTS {
        return;
    }
    ui.selected = Some(SlotId::Hotbar(idx));
    if let Some(ref id) = ui.hotbar[idx].item_id {
        if ui.hotbar[idx].qty > 0 && equipped.item_id != *id {
            equipped.item_id = id.clone();
        }
    }
}

fn bag_panel_system(
    time: Res<Time>,
    layout: Res<TouchLayout>,
    tune: Res<HudTune>,
    root: Query<Entity, With<GameUiRoot>>,
    ship_state: Res<crate::ship_builder::ShipBuilderState>,
    mut commands: Commands,
    mut bag_state: ResMut<BagPanelState>,
    mut craft_intent: ResMut<CraftIntent>,
    mut inv: ResMut<PlayerInventory>,
    mut ui: ResMut<InventoryUiState>,
    mut persist: ResMut<InventoryPersistState>,
    mut pending: ResMut<crate::machines::PendingMachinePlacement>,
    mut cache: ResMut<HudDisplayCache>,
    mut inventory_sections: Query<
        &mut Node,
        (
            With<BagInventorySection>,
            Without<BagCraftSection>,
            Without<BagPanelRoot>,
        ),
    >,
    mut craft_sections: Query<
        &mut Node,
        (
            With<BagCraftSection>,
            Without<BagInventorySection>,
            Without<BagPanelRoot>,
        ),
    >,
    mut ui_queries: ParamSet<(
        Query<
            (&mut Node, &mut BackgroundColor, &BagPanelBase),
            (
                With<BagPanelRoot>,
                Without<CraftRecipeButton>,
                Without<BagInventorySection>,
                Without<BagCraftSection>,
            ),
        >,
        Query<
            (&Interaction, &CraftRecipeButton, &mut BackgroundColor),
            (With<Button>, Without<BagPanelCloseButton>, Without<BagTabButton>),
        >,
        Query<
            (&CraftRecipeCostText, &mut Text),
            (With<CraftRecipeCostText>, Without<CraftStockText>),
        >,
        Query<
            (&Interaction, &mut BackgroundColor),
            (With<BagPanelCloseButton>, With<Button>, Changed<Interaction>),
        >,
        Query<
            (&Interaction, &BagTabButton, &mut BackgroundColor),
            (With<Button>, Changed<Interaction>),
        >,
    )>,
) {
    if !bag_state.open {
        despawn_bag_panel(&mut commands, &mut bag_state);
        return;
    }

    if bag_state.modal.is_none() {
        let Ok(root) = root.get_single() else {
            return;
        };
        bag_state.modal = Some(spawn_bag_panel(
            &mut commands,
            root,
            &layout,
            &tune,
            bag_state.tab,
            &ship_state,
            &inv,
        ));
    }

    for (meta, mut text) in &mut ui_queries.p2() {
        if meta.is_machine_select {
            continue;
        }
        if let Some(recipe) = get_craft_recipe(meta.recipe_id) {
            let (_, line) = recipe_status(&inv, recipe);
            text.0 = line;
        }
    }

    for (interaction, mut bg) in &mut ui_queries.p3() {
        if *interaction == Interaction::Pressed {
            bag_state.open = false;
        }
        *bg = BackgroundColor(ui_theme::button_bg(
            UiButtonTone::Primary,
            ui_theme::interaction_state(*interaction, false),
        ));
    }

    if !bag_state.open {
        despawn_bag_panel(&mut commands, &mut bag_state);
        return;
    }

    for (interaction, tab_btn, mut bg) in &mut ui_queries.p4() {
        if *interaction == Interaction::Pressed {
            bag_state.tab = tab_btn.tab;
        }
        let active = bag_state.tab == tab_btn.tab;
        *bg = BackgroundColor(ui_theme::button_bg(
            UiButtonTone::Inventory,
            if active {
                UiButtonState::Pressed
            } else {
                ui_theme::interaction_state(*interaction, false)
            },
        ));
    }

    // SHIP section visibility is owned by `ship_builder_panel_system`.
    for mut node in &mut inventory_sections {
        node.display = if bag_state.tab == BagTab::Inventory {
            Display::Flex
        } else {
            Display::None
        };
    }
    for mut node in &mut craft_sections {
        node.display = if bag_state.tab == BagTab::Craft {
            Display::Flex
        } else {
            Display::None
        };
    }

    for (interaction, recipe_btn, mut bg) in &mut ui_queries.p1() {
        let can = if recipe_btn.is_machine_select {
            true
        } else if let Some(recipe) = get_craft_recipe(recipe_btn.recipe_id) {
            can_craft(&inv, recipe)
        } else {
            false
        };

        if *interaction == Interaction::Pressed && can {
            if recipe_btn.is_machine_select {
                craft_intent.recipe_id = Some(recipe_btn.recipe_id);
                if recipe_btn.recipe_id == 10 {
                    pending.machine_id = WOOD_EXTRACTOR.id;
                } else if recipe_btn.recipe_id == 11 {
                    pending.machine_id = STONE_EXTRACTOR.id;
                } else if recipe_btn.recipe_id == 12 {
                    pending.machine_id = LAUNCH_PAD.id;
                } else if recipe_btn.recipe_id == 13 {
                    pending.machine_id = ha_shared::REGOLITH_DRILL.id;
                } else if recipe_btn.recipe_id == 14 {
                    pending.machine_id = ha_shared::ANTENNA.id;
                }
                bag_state.open = false;
            } else if let Some(recipe) = get_craft_recipe(recipe_btn.recipe_id) {
                flush_slots_to_inventory(&ui, &mut inv);
                if craft(&mut inv, recipe) {
                    consume_from_ui_slots(&mut ui, recipe.ingredients);
                    if !add_item_to_slots(&mut ui, recipe.output_id, recipe.output_qty) {
                        merge_inv_gains_into_ui(&mut ui, &inv);
                    }
                    flush_slots_to_inventory(&ui, &mut inv);
                    cache.inv_signature = inventory_signature(&inv);
                    persist.mark_immediate();
                }
            }
            bag_state.pulse.reset();
            *bg = BackgroundColor(ui_theme::button_bg(UiButtonTone::Craft, UiButtonState::Pressed));
        } else if can {
            *bg = BackgroundColor(ui_theme::button_bg(
                UiButtonTone::Craft,
                ui_theme::interaction_state(*interaction, false),
            ));
        } else {
            *bg = BackgroundColor(ui_theme::button_bg(UiButtonTone::Craft, UiButtonState::Disabled));
        }
    }

    if !bag_state.open {
        despawn_bag_panel(&mut commands, &mut bag_state);
        return;
    }

    bag_state.pulse.tick(time.delta());
    let progress = if bag_state.pulse.duration().as_secs_f32() > 0.0 {
        (bag_state.pulse.elapsed_secs() / bag_state.pulse.duration().as_secs_f32()).clamp(0.0, 1.0)
    } else {
        1.0
    };
    let e = elastic_out(progress);
    let k = 1.0 + (1.0 - e) * 0.06;
    if let Ok((mut node, mut bg, base)) = ui_queries.p0().get_single_mut() {
        node.width = Val::Px(base.width * k);
        node.height = Val::Px(base.height * k);
        let glow = (1.0 - e) * 0.18;
        *bg = BackgroundColor(ui_theme::panel_bg_lit(glow, 0.88));
    }
}

fn elastic_out(t: f32) -> f32 {
    if t <= 0.0 {
        return 0.0;
    }
    if t >= 1.0 {
        return 1.0;
    }
    let c4 = (2.0 * std::f32::consts::PI) / 3.0;
    (2.0_f32).powf(-10.0 * t) * ((t * 10.0 - 0.75) * c4).sin() + 1.0
}

pub(crate) fn update_interact_hud(
    mut hold: ResMut<HoldInteractState>,
    cooldown: Res<AbilityCooldown>,
    equipped: Res<EquippedItem>,
    target: Res<crate::targeting::LookTarget>,
    mut inv: ResMut<PlayerInventory>,
    mut inv_ui: ResMut<InventoryUiState>,
    mut persist: ResMut<InventoryPersistState>,
    mut flora: ResMut<crate::harvest::FloraHarvest>,
    mut cache: ResMut<HudDisplayCache>,
    mut ui: ParamSet<(
        Query<&mut Node, With<HoldBarRoot>>,
        Query<&mut Node, With<HoldBarFill>>,
        Query<(&mut Node, &mut BorderColor), With<AbilityCooldownRing>>,
    )>,
) {
    if hold.completed {
        if hold.placing_machine {
            // Machine placement consumes hold in try_place_on_hold (runs before this system).
        } else if let Some(kind) = target.kind {
            if hold_target_handled_elsewhere(kind) {
                // Launch pad / orbital travel systems consume hold.completed first.
            } else if let Some(rule) =
                ha_shared::resolve_interaction(Some(equipped.item_id.as_str()), kind)
            {
                match rule.kind {
                    ha_shared::InteractActionKind::Gather
                    | ha_shared::InteractActionKind::Mine
                    | ha_shared::InteractActionKind::Collect => {
                        hold.completed = false;
                        if let Some(item_id) = rule.item_id {
                            add_item_to_slots(&mut inv_ui, item_id, 1);
                            if rule.kind == ha_shared::InteractActionKind::Gather
                                && kind == InteractTargetKind::Tree
                                && equipped.item_id == "hand"
                            {
                                add_item_to_slots(&mut inv_ui, "sticks", 1);
                            }
                            // Chopping a streamed tree removes it from the world
                            // (persisted, re-merges the batch without it).
                            if kind == InteractTargetKind::Tree {
                                if let Some((cx, cz, pid)) = target.flora_cell {
                                    // Deterministic fruit tree: every 3rd tree
                                    // by cell hash also drops an apple (food).
                                    let hash =
                                        cx as i64 * 7 + cz as i64 * 13 + pid as i64;
                                    if hash.rem_euclid(3) == 0 {
                                        add_item_to_slots(&mut inv_ui, "apple", 1);
                                    }
                                    flora.harvest(ha_shared::CelestialBody::Earth, cx, cz, pid);
                                }
                            }
                            flush_slots_to_inventory(&inv_ui, &mut inv);
                            cache.inv_signature = inventory_signature(&inv);
                            persist.mark_immediate();
                        }
                    }
                    _ => {}
                }
            } else {
                hold.completed = false;
            }
        } else {
            hold.completed = false;
        }
    }

    let hold_visible = hold.active;
    let hold_pct = if hold.duration_ms > 0 {
        (hold.progress_ms as f32 / hold.duration_ms as f32).clamp(0.0, 1.0) * 100.0
    } else {
        0.0
    };
    if cache.hold_visible != hold_visible {
        cache.hold_visible = hold_visible;
        if let Ok(mut root) = ui.p0().get_single_mut() {
            root.display = if hold_visible {
                Display::Flex
            } else {
                Display::None
            };
        }
    }
    if hold_visible && (cache.hold_pct - hold_pct).abs() > 0.5 {
        cache.hold_pct = hold_pct;
        if let Ok(mut fill) = ui.p1().get_single_mut() {
            fill.width = Val::Percent(hold_pct);
        }
    }

    let ready = cooldown.remaining_ms <= 0.0;
    let cooldown_visible = cooldown.total_ms > 0.0 && !ready;
    let cooldown_alpha = if cooldown.total_ms > 0.0 {
        let pct = 1.0 - (cooldown.remaining_ms / cooldown.total_ms).clamp(0.0, 1.0);
        if ready { 0.35 } else { 0.55 + pct * 0.35 }
    } else {
        0.35
    };
    if cache.cooldown_visible != cooldown_visible {
        cache.cooldown_visible = cooldown_visible;
        if let Ok((mut node, _)) = ui.p2().get_single_mut() {
            node.display = if cooldown_visible {
                Display::Flex
            } else {
                Display::None
            };
        }
    }
    if cooldown_visible && (cache.cooldown_alpha - cooldown_alpha).abs() > 0.02 {
        cache.cooldown_alpha = cooldown_alpha;
        if let Ok((_, mut border)) = ui.p2().get_single_mut() {
            *border = BorderColor(ui_theme::accent_magenta().with_alpha(cooldown_alpha));
        }
    }
}

fn sync_lantern_equipped(
    equipped: Res<EquippedItem>,
    mut ctrl: ResMut<PlayerLanternControl>,
) {
    ctrl.equipped = equipped.item_id == "lantern";
}

fn lantern_toggle_system(
    keys: Res<ButtonInput<KeyCode>>,
    equipped: Res<EquippedItem>,
    editor: Res<crate::ui_editor::UiEditorState>,
    gameplay_block: Res<UiGameplayBlock>,
    mut ctrl: ResMut<PlayerLanternControl>,
    mut buttons: Query<&Interaction, (With<LanternToggleButton>, Changed<Interaction>)>,
) {
    if equipped.item_id != "lantern" {
        return;
    }
    if !editor.menu_open && !editor.enabled && !gameplay_block.blocked {
        if keys.just_pressed(KeyCode::KeyL) {
            ctrl.lit = !ctrl.lit;
        }
    }
    for interaction in &buttons {
        if *interaction == Interaction::Pressed {
            ctrl.lit = !ctrl.lit;
        }
    }
}

fn update_lantern_toggle_ui(
    equipped: Res<EquippedItem>,
    ctrl: Res<PlayerLanternControl>,
    layout: Res<TouchLayout>,
    mut root: Query<(&mut Node, &mut Visibility), With<LanternToggleRoot>>,
    mut labels: Query<&mut Text, With<LanternToggleLabel>>,
    mut buttons: Query<&mut BackgroundColor, With<LanternToggleButton>>,
) {
    let show = equipped.item_id == "lantern";
    let short = layout.width.min(layout.height);
    let (_, _, total, bottom) = hotbar_layout_metrics(&layout);
    let btn_h = (short * 0.052).clamp(28.0, 40.0);
    let btn_w = btn_h * 2.1;

    for (mut node, mut vis) in &mut root {
        *vis = if show {
            Visibility::Visible
        } else {
            Visibility::Hidden
        };
        node.left = Val::Px(game_ui::ui_center_left(&layout, total) + total + 8.0);
        node.bottom = Val::Px(bottom);
        node.width = Val::Px(btn_w);
        node.height = Val::Px(btn_h);
    }
    let label = if ctrl.lit { "LAMP ON" } else { "LAMP OFF" };
    for mut text in &mut labels {
        if text.0 != label {
            text.0 = label.to_string();
        }
    }
    let tone = if ctrl.lit {
        UiButtonState::Pressed
    } else {
        UiButtonState::Normal
    };
    for mut bg in &mut buttons {
        *bg = BackgroundColor(ui_theme::button_bg(UiButtonTone::Use, tone));
    }
}

fn hotbar_hammer_system(
    mut equipped: ResMut<EquippedItem>,
    mut ui: ResMut<InventoryUiState>,
    hammer: Query<&Interaction, (With<HotbarHammerSlot>, Changed<Interaction>)>,
) {
    for interaction in &hammer {
        if *interaction == Interaction::Pressed && equipped.item_id != "hammer" {
            equipped.item_id = "hammer".to_string();
            ui.selected = None;
        }
    }
}

/// Floating icon while long-press dragging a stack.
fn update_held_item_ghost(
    mut commands: Commands,
    ui: Res<InventoryUiState>,
    layout: Res<TouchLayout>,
    root: Query<Entity, With<GameUiRoot>>,
    mut ghost: Query<(Entity, &mut Node, &mut Visibility), With<HeldItemGhost>>,
    mut images: ResMut<Assets<Image>>,
    mut icons: ResMut<ItemIconCache>,
    mut image_nodes: Query<&mut ImageNode, With<HeldItemGhost>>,
) {
    let Ok(root_e) = root.get_single() else {
        return;
    };
    let held = ui.held.as_ref();
    if held.is_none() {
        for (_, _, mut vis) in &mut ghost {
            *vis = Visibility::Hidden;
        }
        return;
    }
    let held = held.unwrap();
    let short = layout.width.min(layout.height);
    let size = (short * 0.06).clamp(32.0, 48.0);
    let bottom = layout.insets.bottom as f32 + (short * 0.14).clamp(56.0, 90.0);

    if ghost.is_empty() {
        let icon = icons.get(&held.item_id, &mut images);
        commands.entity(root_e).with_children(|p| {
            p.spawn((
                Node {
                    position_type: PositionType::Absolute,
                    left: Val::Px(layout.width * 0.5 - size * 0.5),
                    bottom: Val::Px(bottom),
                    width: Val::Px(size),
                    height: Val::Px(size),
                    ..default()
                },
                ImageNode::new(icon),
                Visibility::Visible,
                HeldItemGhost,
                Name::new("HeldItemGhost"),
            ));
        });
        return;
    }

    for (e, mut node, mut vis) in &mut ghost {
        *vis = Visibility::Visible;
        node.left = Val::Px(layout.width * 0.5 - size * 0.5);
        node.bottom = Val::Px(bottom);
        node.width = Val::Px(size);
        node.height = Val::Px(size);
        let icon = icons.get(&held.item_id, &mut images);
        if let Ok(mut img) = image_nodes.get_mut(e) {
            img.image = icon;
        }
    }
}

fn hold_target_handled_elsewhere(kind: InteractTargetKind) -> bool {
    matches!(
        kind,
        InteractTargetKind::Block
            | InteractTargetKind::LaunchModule
            | InteractTargetKind::LaunchIgnition
            | InteractTargetKind::SpaceReentry
            | InteractTargetKind::SpaceTransitLuna
            | InteractTargetKind::SpaceStationModule
            | InteractTargetKind::LunarReturn
    )
}

fn sync_ui_gameplay_block(
    bag: Res<BagPanelState>,
    editor: Res<crate::ui_editor::UiEditorState>,
    flight: Res<RocketFlight>,
    mut block: ResMut<UiGameplayBlock>,
) {
    block.blocked = bag.open || editor.menu_open || editor.enabled || flight.blocks_walking();
}

fn append_hud_touch_blocks(
    layout: Res<TouchLayout>,
    bag: Res<BagPanelState>,
    hud_blocks: Res<HudTouchBlocks>,
    mut blocks: ResMut<TouchBlockRects>,
) {
    blocks.0.extend(hud_blocks.0.iter().copied());
    if bag.open {
        blocks.0.push(fullscreen_block_rect(&layout));
    }
}

fn sync_inventory_from_player(
    inv: Res<PlayerInventory>,
    mut ui: ResMut<InventoryUiState>,
    mut equipped: ResMut<EquippedItem>,
    mut cache: ResMut<HudDisplayCache>,
) {
    let sig = inventory_signature(&inv);
    if !ui.loaded_from_disk {
        load_grid_from_inventory(&mut ui, &inv);
        ui.loaded_from_disk = true;
        if ui.hotbar[0].item_id.as_deref() == Some("lantern") && ui.hotbar[0].qty > 0 {
            ui.selected = Some(SlotId::Hotbar(0));
            equipped.item_id = "lantern".to_string();
        }
        cache.inv_signature = sig;
        return;
    }
    if ui.held.is_some() {
        return;
    }
    let grid_sig = grid_signature(&ui);
    if sig == cache.inv_signature && !grid_sig.is_empty() {
        return;
    }
    if flush_matches_inventory(&ui, &inv) {
        cache.inv_signature = sig;
        return;
    }
    if merge_inv_gains_into_ui(&mut ui, &inv) {
        cache.inv_signature = sig;
        return;
    }
    load_grid_from_inventory(&mut ui, &inv);
    cache.inv_signature = sig;
}

fn flush_matches_inventory(ui: &InventoryUiState, inv: &PlayerInventory) -> bool {
    let mut tmp = PlayerInventory::default();
    flush_slots_to_inventory(ui, &mut tmp);
    tmp.items == inv.items
}

fn visit_slot_entities(
    children: &[Entity],
    children_q: &Query<&Children>,
    visit: &mut impl FnMut(Entity),
) {
    let mut stack: Vec<Entity> = children.to_vec();
    while let Some(entity) = stack.pop() {
        visit(entity);
        if let Ok(kids) = children_q.get(entity) {
            stack.extend(kids.iter().copied());
        }
    }
}

fn update_inventory_slot_visuals(
    bag: Res<BagPanelState>,
    mut commands: Commands,
    mut images: ResMut<Assets<Image>>,
    mut icons: ResMut<ItemIconCache>,
    ui: Res<InventoryUiState>,
    mut span: ResMut<CpuSpanStats>,
    children_q: Query<&Children>,
    mut slots: Query<
        (&InventoryGridSlot, &mut BorderColor, &Children),
        Without<HotbarItemSlot>,
    >,
    mut icon_markers: Query<
        (Entity, &mut Visibility),
        (With<InventorySlotIcon>, Without<HotbarSlotIcon>),
    >,
    mut image_nodes: Query<&mut ImageNode, With<InventorySlotIcon>>,
    mut slot_texts: ParamSet<(
        Query<&mut Text, (With<InventorySlotQty>, Without<HotbarSlotQty>)>,
        Query<&mut Text, (With<InventorySlotName>, Without<HotbarSlotName>)>,
    )>,
) {
    if !bag.open {
        return;
    }
    let _guard = span_guard(&mut span, CpuSpanSlot::Hud);
    for (slot, mut border, children) in &mut slots {
        let stack = &ui.grid[slot.index];
        let selected = ui.selected == Some(SlotId::Grid(slot.index));
        *border = BorderColor(if selected {
            ui_theme::accent_cyan()
        } else if ui.held.as_ref().is_some_and(|h| h.from == SlotId::Grid(slot.index)) {
            ui_theme::accent_magenta()
        } else {
            ui_theme::button_border(UiButtonTone::Inventory)
        });

        visit_slot_entities(children, &children_q, &mut |entity| {
            if let Ok((icon_e, mut vis)) = icon_markers.get_mut(entity) {
                let handle = if stack.is_empty() {
                    icons.empty_slot(&mut images)
                } else {
                    icons.get(stack.item_id.as_deref().unwrap_or("wood"), &mut images)
                };
                *vis = Visibility::Visible;
                if let Ok(mut node) = image_nodes.get_mut(entity) {
                    node.image = handle;
                } else {
                    commands.entity(icon_e).insert(ImageNode::new(handle));
                }
            }
        });
        {
            let mut qty_labels = slot_texts.p0();
            visit_slot_entities(children, &children_q, &mut |entity| {
                if let Ok(mut text) = qty_labels.get_mut(entity) {
                    let label = if stack.qty > 1 {
                        format!("{}", stack.qty)
                    } else {
                        String::new()
                    };
                    if text.0 != label {
                        text.0 = label;
                    }
                }
            });
        }
        {
            let mut name_labels = slot_texts.p1();
            visit_slot_entities(children, &children_q, &mut |entity| {
                if let Ok(mut text) = name_labels.get_mut(entity) {
                    let label = if stack.is_empty() {
                        String::new()
                    } else {
                        item_display_name(stack.item_id.as_deref().unwrap_or("wood")).to_string()
                    };
                    if text.0 != label {
                        text.0 = label;
                    }
                }
            });
        }
    }
}

fn update_hotbar_item_visuals(
    mut commands: Commands,
    mut images: ResMut<Assets<Image>>,
    mut icons: ResMut<ItemIconCache>,
    ui: Res<InventoryUiState>,
    children_q: Query<&Children>,
    mut slots: Query<
        (&HotbarItemSlot, &mut BorderColor, &Children),
        Without<InventoryGridSlot>,
    >,
    mut icon_markers: Query<
        (Entity, &mut Visibility),
        (With<HotbarSlotIcon>, Without<InventorySlotIcon>),
    >,
    mut image_nodes: Query<
        &mut ImageNode,
        (
            With<HotbarSlotIcon>,
            Without<HotbarHammerIcon>,
            Without<HotbarBagIcon>,
        ),
    >,
    mut slot_texts: ParamSet<(
        Query<&mut Text, (With<HotbarSlotQty>, Without<InventorySlotQty>)>,
        Query<&mut Text, (With<HotbarSlotName>, Without<InventorySlotName>)>,
    )>,
    mut span: ResMut<CpuSpanStats>,
) {
    let _guard = span_guard(&mut span, CpuSpanSlot::Hud);
    for (slot, mut border, children) in &mut slots {
        let stack = &ui.hotbar[slot.index];
        let selected = ui.selected == Some(SlotId::Hotbar(slot.index));
        *border = BorderColor(if selected {
            ui_theme::accent_cyan()
        } else if ui.held.as_ref().is_some_and(|h| h.from == SlotId::Hotbar(slot.index)) {
            ui_theme::accent_magenta()
        } else {
            ui_theme::button_border(UiButtonTone::Inventory)
        });
        visit_slot_entities(children, &children_q, &mut |entity| {
            if let Ok((icon_e, mut vis)) = icon_markers.get_mut(entity) {
                let handle = if stack.is_empty() {
                    icons.empty_slot(&mut images)
                } else {
                    icons.get(stack.item_id.as_deref().unwrap_or("wood"), &mut images)
                };
                *vis = Visibility::Visible;
                if let Ok(mut node) = image_nodes.get_mut(entity) {
                    node.image = handle;
                } else {
                    commands.entity(icon_e).insert(ImageNode::new(handle));
                }
            }
        });
        {
            let mut qty_labels = slot_texts.p0();
            visit_slot_entities(children, &children_q, &mut |entity| {
                if let Ok(mut text) = qty_labels.get_mut(entity) {
                    let label = if stack.qty > 1 {
                        format!("{}", stack.qty)
                    } else {
                        String::new()
                    };
                    if text.0 != label {
                        text.0 = label;
                    }
                }
            });
        }
        {
            let mut name_labels = slot_texts.p1();
            visit_slot_entities(children, &children_q, &mut |entity| {
                if let Ok(mut text) = name_labels.get_mut(entity) {
                    let label = if stack.is_empty() {
                        String::new()
                    } else {
                        item_hotbar_label(stack.item_id.as_deref().unwrap_or("wood"))
                    };
                    if text.0 != label {
                        text.0 = label;
                    }
                }
            });
        }
    }
}

fn sync_hotbar_chrome_icons(
    mut commands: Commands,
    mut images: ResMut<Assets<Image>>,
    mut icons: ResMut<ItemIconCache>,
    mut hammer: Query<
        (Entity, &mut ImageNode),
        (With<HotbarHammerIcon>, Without<HotbarBagIcon>, Without<HotbarSlotIcon>),
    >,
    mut bag: Query<
        (Entity, &mut ImageNode),
        (With<HotbarBagIcon>, Without<HotbarHammerIcon>, Without<HotbarSlotIcon>),
    >,
    hammer_missing: Query<
        Entity,
        (
            With<HotbarHammerIcon>,
            Without<ImageNode>,
            Without<HotbarBagIcon>,
        ),
    >,
    bag_missing: Query<
        Entity,
        (
            With<HotbarBagIcon>,
            Without<ImageNode>,
            Without<HotbarHammerIcon>,
        ),
    >,
) {
    let hammer_icon = icons.get("hammer", &mut images);
    for e in &hammer_missing {
        commands.entity(e).insert(ImageNode::new(hammer_icon.clone()));
    }
    for (_, mut node) in &mut hammer {
        node.image = hammer_icon.clone();
    }

    let bag_icon = icons.bag_slot(&mut images);
    for e in &bag_missing {
        commands.entity(e).insert(ImageNode::new(bag_icon.clone()));
    }
    for (_, mut node) in &mut bag {
        node.image = bag_icon.clone();
    }
}

fn equip_from_hotbar_selection(
    ui: Res<InventoryUiState>,
    mut equipped: ResMut<EquippedItem>,
) {
    let Some(SlotId::Hotbar(idx)) = ui.selected else {
        return;
    };
    let stack = &ui.hotbar[idx];
    let Some(ref id) = stack.item_id else {
        return;
    };
    if stack.qty == 0 {
        return;
    }
    if equipped.item_id != *id {
        equipped.item_id = id.clone();
    }
}

/// Tapping/clicking a hotbar item cell selects + equips it — works for both
/// touch (mobile) and mouse (desktop), so the hotbar is usable without the bag.
fn equip_from_hotbar_click(
    mut ui: ResMut<InventoryUiState>,
    mut equipped: ResMut<EquippedItem>,
    slots: Query<(&Interaction, &HotbarItemSlot), Changed<Interaction>>,
) {
    for (interaction, slot) in &slots {
        if *interaction == Interaction::Pressed {
            select_hotbar_slot(&mut ui, &mut equipped, slot.index);
        }
    }
}

/// How long the "en mano: X" toast stays before it starts vanishing, and the
/// fade-out window (alpha ramps down over the final `FADE` seconds).
const EQUIP_TOAST_HOLD: f32 = 1.1;
const EQUIP_TOAST_FADE: f32 = 0.8;

/// The floating "en mano: <item>" label that appears when the equipped item
/// changes and then fades away (only the text fades — nothing stays on screen).
#[derive(Component)]
struct EquipToast {
    /// Seconds of visible life remaining; 0 = hidden.
    life: f32,
}

fn equip_toast_system(
    time: Res<Time>,
    equipped: Res<EquippedItem>,
    mut last: Local<Option<String>>,
    mut q: Query<(&mut EquipToast, &mut Text, &mut TextColor, &mut Node)>,
) {
    // Seed on first run so we don't flash a toast on boot.
    if last.is_none() {
        *last = Some(equipped.item_id.clone());
        return;
    }
    let changed = last.as_deref() != Some(equipped.item_id.as_str());
    if changed {
        *last = Some(equipped.item_id.clone());
    }
    for (mut toast, mut text, mut color, mut node) in &mut q {
        if changed {
            let name = item_display_name(&equipped.item_id);
            text.0 = format!("en mano: {name}");
            toast.life = EQUIP_TOAST_HOLD + EQUIP_TOAST_FADE;
            node.display = Display::Flex;
        }
        if toast.life > 0.0 {
            toast.life = (toast.life - time.delta_secs()).max(0.0);
            let alpha = (toast.life / EQUIP_TOAST_FADE).clamp(0.0, 1.0);
            *color = TextColor(ui_theme::accent_cyan().with_alpha(alpha));
            if toast.life <= 0.0 {
                node.display = Display::None;
            }
        }
    }
}

fn spawn_equip_toast(parent: &mut ChildBuilder, layout: &TouchLayout, heart: f32) {
    let (slot, gap, _, bottom) = hotbar_layout_metrics(layout);
    parent
        .spawn((
            Node {
                position_type: PositionType::Absolute,
                left: Val::Px(0.0),
                bottom: Val::Px(bottom + slot + gap + 6.0),
                width: Val::Percent(100.0),
                justify_content: JustifyContent::Center,
                align_items: AlignItems::Center,
                ..default()
            },
            Name::new("EquipToastRow"),
        ))
        .with_children(|row| {
            row.spawn((
                Text::new(""),
                TextFont {
                    font_size: (heart * 0.5).clamp(12.0, 18.0),
                    ..default()
                },
                TextColor(ui_theme::accent_cyan().with_alpha(0.0)),
                Node {
                    display: Display::None,
                    ..default()
                },
                EquipToast { life: 0.0 },
            ));
        });
}
