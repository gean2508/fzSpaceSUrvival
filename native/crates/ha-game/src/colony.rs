//! Colony runtime v1 — colonists live at the lunar base as abstract sim
//! (rules in `ha-shared/colony.rs` ← TS `colony.ts`). This module owns the
//! 20 Hz food tick, the Earth-side uplink buttons (send colonist / send
//! supplies, gated by antenna + habitat), `colony.json` persistence, and
//! feeds the drill boost that `machines::tick_machines` applies.

use bevy::prelude::*;
use ha_shared::{
    can_request_colonist, step_colony, CelestialBody, ColonyState, COLONIST_SEND_COST_APPLES,
    SUPPLY_SEND_APPLES,
};
use serde::{Deserialize, Serialize};

use crate::game_time::fixed_delta_secs;
use crate::hud::inventory_ui::{consume_from_ui_slots, flush_slots_to_inventory, InventoryUiState};
use crate::inventory_persist::{persist_dir, InventoryPersistState, PlayerInventory};
use crate::machines::MachineWorld;
use crate::platform::is_app_paused;

pub struct ColonyPlugin;

impl Plugin for ColonyPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<ColonyRuntime>()
            .add_systems(OnEnter(crate::game_phase::GamePhase::Loading), load_colony_on_startup)
            .add_systems(FixedUpdate, tick_colony)
            .add_systems(Update, (colony_button_system, debounced_colony_save));
    }
}

#[derive(Resource, Default)]
pub struct ColonyRuntime {
    pub state: ColonyState,
    dirty: bool,
    was_paused: bool,
    debounce: f32,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum ColonyAction {
    SendColonist,
    SendSupplies,
}

/// Uplink buttons in the orbital map menu (spawned by `game_menu`).
#[derive(Component)]
pub struct ColonyButton(pub ColonyAction);

fn habitats_on_moon(machines: &MachineWorld) -> u32 {
    machines
        .on_body(CelestialBody::Moon)
        .filter(|m| m.machine_id == ha_shared::HABITAT.id)
        .count() as u32
}

fn tick_colony(
    time: Res<Time<Fixed>>,
    mut colony: ResMut<ColonyRuntime>,
) {
    let _p = crate::debug::probe("fx_tick_colony");
    if is_app_paused() || colony.state.colonists == 0 {
        return;
    }
    let dt = fixed_delta_secs(&time);
    step_colony(&mut colony.state, dt);
    colony.dirty = true;
}

/// Earth-side uplink: send a colonist (5 apples, needs antenna + free bunk)
/// or a supply crate (3 apples → food stock). Works from anywhere the map
/// opens — the antenna is the fiction, the gate is real.
#[allow(clippy::too_many_arguments)]
fn colony_button_system(
    machines: Res<MachineWorld>,
    mut colony: ResMut<ColonyRuntime>,
    mut inv: ResMut<PlayerInventory>,
    mut inv_ui: ResMut<InventoryUiState>,
    mut inv_persist: ResMut<InventoryPersistState>,
    mut soc: ResMut<crate::society::Society>,
    mut buttons: Query<
        (&Interaction, &ColonyButton, &mut BackgroundColor, &mut Visibility),
        With<Button>,
    >,
) {
    let antenna = machines.has_antenna_on(CelestialBody::Moon);
    let habitats = habitats_on_moon(&machines);

    for (interaction, btn, mut bg, mut vis) in &mut buttons {
        // Buttons only exist as options once the uplink exists.
        *vis = if antenna {
            Visibility::Visible
        } else {
            Visibility::Hidden
        };
        if !antenna {
            continue;
        }

        let (cost, can) = match btn.0 {
            ColonyAction::SendColonist => (
                COLONIST_SEND_COST_APPLES,
                can_request_colonist(&colony.state, habitats),
            ),
            ColonyAction::SendSupplies => (SUPPLY_SEND_APPLES, colony.state.colonists > 0),
        };
        let affordable = inv.qty("apple") >= cost;

        if *interaction == Interaction::Pressed && can && affordable {
            consume_from_ui_slots(&mut inv_ui, &[("apple", cost)]);
            flush_slots_to_inventory(&inv_ui, &mut inv);
            inv_persist.mark_immediate();
            match btn.0 {
                ColonyAction::SendColonist => {
                    colony.state.colonists += 1;
                    eprintln!("ha-colony: colonist sent ({} total)", colony.state.colonists);
                }
                ColonyAction::SendSupplies => {
                    colony.state.food_stock += SUPPLY_SEND_APPLES as f32;
                    // Alimenta la economía REAL de los NPCs lunares (`society.rs`), no
                    // sólo el contador abstracto — si no, los colonos que se VEN nunca
                    // reciben lo que mandás.
                    soc.add_stock(CelestialBody::Moon as u8, SUPPLY_SEND_APPLES as f32, 0.0, 0.0);
                    eprintln!("ha-colony: supplies sent (stock {:.0})", colony.state.food_stock);
                }
            }
            colony.dirty = true;
        }

        crate::ui_theme::pintar_boton(&mut bg, if can && affordable {
            crate::ui_theme::button_bg(
                crate::ui_theme::UiButtonTone::Primary,
                crate::ui_theme::interaction_state(*interaction, false),
            )
        } else {
            crate::ui_theme::button_bg(
                crate::ui_theme::UiButtonTone::Primary,
                crate::ui_theme::UiButtonState::Disabled,
            )
        });
    }
}

// --- Persistence (colony.json, flush-on-pause — B10) ----------------------------

#[derive(Serialize, Deserialize)]
struct ColonySave {
    colonists: u32,
    food_stock: f32,
}

fn colony_path() -> std::path::PathBuf {
    persist_dir().join("colony.json")
}

fn load_colony_on_startup(mut colony: ResMut<ColonyRuntime>) {
    // **Cada partida arranca con lo SUYO.** Esto corría en `Startup`, antes de
    // elegir partida: leía la carpeta base y lo de las pruebas viejas aparecía en
    // cualquier partida nueva. Ahora corre al entrar a la partida (`Loading`) y
    // parte de cero; ver `partida.rs`.
    *colony = ColonyRuntime::default();
    let Ok(text) = std::fs::read_to_string(colony_path()) else {
        return;
    };
    match serde_json::from_str::<ColonySave>(&text) {
        Ok(save) => {
            colony.state.colonists = save.colonists;
            colony.state.food_stock = save.food_stock.max(0.0);
        }
        Err(_) => eprintln!("ha-colony: colony.json unreadable — starting empty"),
    }
}

fn debounced_colony_save(time: Res<Time>, mut colony: ResMut<ColonyRuntime>) {
    let paused = is_app_paused();
    let pause_edge = paused && !colony.was_paused;
    colony.was_paused = paused;
    if !colony.dirty {
        return;
    }
    colony.debounce += time.delta_secs();
    if !pause_edge && colony.debounce < 10.0 {
        return;
    }
    let save = ColonySave {
        colonists: colony.state.colonists,
        food_stock: colony.state.food_stock,
    };
    match serde_json::to_string(&save) {
        Ok(json) => {
            if let Err(err) = std::fs::write(colony_path(), json) {
                eprintln!("ha-colony: save failed: {err}");
            }
        }
        Err(err) => eprintln!("ha-colony: serialize failed: {err}"),
    }
    colony.dirty = false;
    colony.debounce = 0.0;
}
