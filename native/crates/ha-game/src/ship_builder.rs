//! Ship Builder Fase 1 — SFS-style part counters (see SHIP_BUILDER_DESIGN.md).
//!
//! The pad's 3 assembled modules ARE the baseline hull+tank+engine; here the
//! player spends crafted parts (`part_tank` / `part_engine` / `part_lander`)
//! to grow the ship, watching mass/fuel/burn/thrust trade off live. Rules in
//! `ha-shared/ship_mass.rs` (mirror of TS `shipMass.ts`); this module owns the
//! BAG→SHIP tab UI, the config resource and `ship.json` persistence.

use bevy::prelude::*;
use ha_shared::{ship_config_stats, ShipConfig, SHIP_MAX_ENGINES, SHIP_MAX_EXTRA_TANKS, SHIP_MAX_LANDERS};
use serde::{Deserialize, Serialize};

use crate::hud::inventory_ui::{
    add_item_to_slots, consume_from_ui_slots, flush_slots_to_inventory, InventoryUiState,
};
use crate::inventory_persist::{persist_dir, InventoryPersistState, PlayerInventory};
use crate::platform::is_app_paused;
use crate::ui_theme;
use crate::ui_theme::UiButtonTone;

pub struct ShipBuilderPlugin;

impl Plugin for ShipBuilderPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<ShipBuilderState>()
            .add_systems(Startup, load_ship_config_on_startup)
            .add_systems(Update, (ship_builder_panel_system, debounced_ship_save));
    }
}

/// Player's persistent ship configuration (extends the pad baseline).
#[derive(Resource, Default)]
pub struct ShipBuilderState {
    pub config: ShipConfig,
    dirty: bool,
    was_paused: bool,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum ShipPart {
    Tank,
    Engine,
    Lander,
}

impl ShipPart {
    fn item_id(self) -> &'static str {
        match self {
            ShipPart::Tank => "part_tank",
            ShipPart::Engine => "part_engine",
            ShipPart::Lander => "part_lander",
        }
    }

    fn label(self) -> &'static str {
        match self {
            ShipPart::Tank => "FUEL TANKS",
            ShipPart::Engine => "ENGINES",
            ShipPart::Lander => "LANDER",
        }
    }
}

#[derive(Component)]
pub struct ShipPartButton {
    part: ShipPart,
    delta: i32,
}

#[derive(Component)]
pub struct ShipPartQtyText(ShipPart);

#[derive(Component)]
pub struct ShipStatsText;

#[derive(Component)]
pub struct BagShipSection;

fn part_count(config: &ShipConfig, part: ShipPart) -> u32 {
    match part {
        ShipPart::Tank => config.tanks,
        ShipPart::Engine => config.engines,
        ShipPart::Lander => config.landers,
    }
}

fn set_part_count(config: &mut ShipConfig, part: ShipPart, value: u32) {
    match part {
        ShipPart::Tank => config.tanks = value,
        ShipPart::Engine => config.engines = value,
        ShipPart::Lander => config.landers = value,
    }
}

/// (min, max) the counter may reach — engines start at the baseline 1.
fn part_range(part: ShipPart) -> (u32, u32) {
    match part {
        ShipPart::Tank => (0, SHIP_MAX_EXTRA_TANKS),
        ShipPart::Engine => (1, SHIP_MAX_ENGINES),
        ShipPart::Lander => (0, SHIP_MAX_LANDERS),
    }
}

/// "config count (stock N)" line for a part row.
fn qty_line(config: &ShipConfig, inv: &PlayerInventory, part: ShipPart) -> String {
    format!(
        "{}  (stock {})",
        part_count(config, part),
        inv.qty(part.item_id())
    )
}

fn stats_line(config: ShipConfig) -> String {
    let stats = ship_config_stats(config);
    let autonomy = stats.fuel_max
        / (ha_shared::flight_physics::ROCKET_FUEL_BURN_PER_S * stats.burn_factor).max(0.01);
    format!(
        "MASS {:.1} t · FUEL {:.0} · BURN x{:.2} · THRUST x{:.2} · ~{:.0}s",
        stats.mass_t, stats.fuel_max, stats.burn_factor, stats.thrust_factor, autonomy
    )
}

/// BAG panel SHIP tab content — called from `hud::spawn_bag_panel`.
pub fn spawn_ship_section(
    parent: &mut ChildBuilder,
    heart: f32,
    visible: bool,
    state: &ShipBuilderState,
    inv: &PlayerInventory,
) {
    parent
        .spawn((
            Node {
                width: Val::Percent(100.0),
                flex_grow: 1.0,
                flex_direction: FlexDirection::Column,
                row_gap: Val::Px((heart * 0.4).clamp(6.0, 11.0)),
                display: if visible { Display::Flex } else { Display::None },
                overflow: Overflow::scroll_y(),
                ..default()
            },
            BagShipSection,
            ScrollPosition::default(),
        ))
        .with_children(|section| {
            section.spawn((
                Text::new("Craft parts in CRAFT, add them here — stats trade off live"),
                TextFont {
                    font_size: (heart * 0.46).clamp(9.0, 12.0),
                    ..default()
                },
                TextColor(ui_theme::text_muted()),
            ));
            for part in [ShipPart::Tank, ShipPart::Engine, ShipPart::Lander] {
                spawn_part_row(section, part, heart, state, inv);
            }
            section.spawn((
                Text::new(stats_line(state.config)),
                TextFont {
                    font_size: (heart * 0.5).clamp(10.0, 13.0),
                    ..default()
                },
                TextColor(ui_theme::text_primary()),
                ShipStatsText,
            ));
        });
}

fn spawn_part_row(
    parent: &mut ChildBuilder,
    part: ShipPart,
    heart: f32,
    state: &ShipBuilderState,
    inv: &PlayerInventory,
) {
    let btn_px = (heart * 1.3).clamp(24.0, 40.0);
    parent
        .spawn((
            Node {
                width: Val::Percent(100.0),
                flex_direction: FlexDirection::Row,
                align_items: AlignItems::Center,
                column_gap: Val::Px(8.0),
                ..default()
            },
            Name::new(part.label()),
        ))
        .with_children(|row| {
            row.spawn((
                Node {
                    width: Val::Px(heart * 5.2),
                    ..default()
                },
                Text::new(part.label()),
                TextFont {
                    font_size: (heart * 0.5).clamp(10.0, 13.0),
                    ..default()
                },
                TextColor(ui_theme::text_primary()),
            ));
            for (label, delta) in [("-", -1i32), ("+", 1i32)] {
                row.spawn((
                    Button,
                    Node {
                        width: Val::Px(btn_px),
                        height: Val::Px(btn_px),
                        justify_content: JustifyContent::Center,
                        align_items: AlignItems::Center,
                        border: UiRect::all(Val::Px(ui_theme::panel_border_width())),
                        ..default()
                    },
                    BorderColor(ui_theme::accent_ink()),
                    BackgroundColor(ui_theme::button_bg(
                        UiButtonTone::Primary,
                        ui_theme::UiButtonState::Normal,
                    )),
                    ShipPartButton { part, delta },
                ))
                .with_children(|btn| {
                    btn.spawn((
                        Text::new(label),
                        TextFont {
                            font_size: (heart * 0.62).clamp(12.0, 16.0),
                            ..default()
                        },
                        TextColor(ui_theme::text_primary()),
                    ));
                });
                if delta < 0 {
                    row.spawn((
                        Text::new(qty_line(&state.config, inv, part)),
                        TextFont {
                            font_size: (heart * 0.52).clamp(10.0, 13.0),
                            ..default()
                        },
                        TextColor(ui_theme::text_primary()),
                        ShipPartQtyText(part),
                    ));
                }
            }
        });
}

/// Tab visibility + [-]/[+] presses + live texts. Runs every frame the BAG
/// panel exists; cheap when the SHIP tab is hidden.
#[allow(clippy::too_many_arguments)]
fn ship_builder_panel_system(
    bag: Res<crate::hud::BagPanelState>,
    mut state: ResMut<ShipBuilderState>,
    mut inv: ResMut<PlayerInventory>,
    mut inv_ui: ResMut<InventoryUiState>,
    mut inv_persist: ResMut<InventoryPersistState>,
    mut sections: Query<&mut Node, With<BagShipSection>>,
    mut buttons: Query<
        (&Interaction, &ShipPartButton, &mut BackgroundColor),
        (With<Button>, Changed<Interaction>),
    >,
    mut qty_texts: Query<(&ShipPartQtyText, &mut Text), Without<ShipStatsText>>,
    mut stats_texts: Query<&mut Text, With<ShipStatsText>>,
) {
    if !bag.open {
        return;
    }
    let show = bag.tab == crate::hud::BagTab::Ship;
    for mut node in &mut sections {
        node.display = if show { Display::Flex } else { Display::None };
    }
    if !show {
        return;
    }

    let mut changed = false;
    for (interaction, btn, mut bg) in &mut buttons {
        if *interaction == Interaction::Pressed {
            let (min, max) = part_range(btn.part);
            let count = part_count(&state.config, btn.part);
            if btn.delta > 0 && count < max && inv.qty(btn.part.item_id()) > 0 {
                consume_from_ui_slots(&mut inv_ui, &[(btn.part.item_id(), 1)]);
                flush_slots_to_inventory(&inv_ui, &mut inv);
                set_part_count(&mut state.config, btn.part, count + 1);
                changed = true;
            } else if btn.delta < 0 && count > min {
                add_item_to_slots(&mut inv_ui, btn.part.item_id(), 1);
                flush_slots_to_inventory(&inv_ui, &mut inv);
                set_part_count(&mut state.config, btn.part, count - 1);
                changed = true;
            }
        }
        *bg = BackgroundColor(ui_theme::button_bg(
            UiButtonTone::Primary,
            ui_theme::interaction_state(*interaction, false),
        ));
    }

    if changed {
        state.dirty = true;
        inv_persist.mark_immediate();
    }

    // Texts are cheap (4 nodes) — refresh while the tab is visible so crafted
    // stock counts stay live even without config changes.
    for (meta, mut text) in &mut qty_texts {
        let line = qty_line(&state.config, &inv, meta.0);
        if text.0 != line {
            text.0 = line;
        }
    }
    for mut text in &mut stats_texts {
        let line = stats_line(state.config);
        if text.0 != line {
            text.0 = line;
        }
    }
}

// --- Persistence (ship.json, flush-on-pause — B10) -----------------------------

#[derive(Serialize, Deserialize)]
struct ShipSave {
    tanks: u32,
    engines: u32,
    landers: u32,
}

fn ship_path() -> std::path::PathBuf {
    persist_dir().join("ship.json")
}

fn load_ship_config_on_startup(mut state: ResMut<ShipBuilderState>) {
    let Ok(text) = std::fs::read_to_string(ship_path()) else {
        return;
    };
    match serde_json::from_str::<ShipSave>(&text) {
        Ok(save) => {
            state.config = ha_shared::clamp_ship_config(ShipConfig {
                tanks: save.tanks,
                engines: save.engines,
                landers: save.landers,
            });
        }
        Err(_) => eprintln!("ha-ship: ship.json unreadable — baseline config"),
    }
}

fn debounced_ship_save(mut state: ResMut<ShipBuilderState>) {
    state.was_paused = is_app_paused();
    // Config changes are rare (panel taps) — save immediately when dirty.
    if !state.dirty {
        return;
    }
    let save = ShipSave {
        tanks: state.config.tanks,
        engines: state.config.engines,
        landers: state.config.landers,
    };
    match serde_json::to_string(&save) {
        Ok(json) => {
            if let Err(err) = std::fs::write(ship_path(), json) {
                eprintln!("ha-ship: save failed: {err}");
            }
        }
        Err(err) => eprintln!("ha-ship: serialize failed: {err}"),
    }
    state.dirty = false;
}
