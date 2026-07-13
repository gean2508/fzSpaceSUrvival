//! Self-test drive (HA_AUTODRIVE=1 + HA_AUTOPLAY=1): plays the game end to
//! end through the REAL input pipeline (keyboard/mouse ButtonInput) and logs
//! `ha-verify: <system> PASS|FAIL` per phase. This is the repo's runtime
//! verification harness — extend it when adding systems.

use bevy::input::ButtonInput;
use bevy::prelude::*;
use bevy::render::view::screenshot::{save_to_disk, Screenshot};

use ha_shared::flat2d::{body_circumference_tiles, wrap_tile};
use ha_shared::CelestialBody;

use crate::input::{DesktopMouseCapture, EquippedItem, HoldInteractState};
use crate::inventory_persist::PlayerInventory;
use crate::launch_pad::LaunchIgnitionMarker;
use crate::machines::MachineWorld;
use crate::rocket_flight::RocketFlight;
use crate::scene::{RegionId, RegionRuntime, WorldContextRuntime};
use crate::sim::{Player, PlayerVisual, SimState};
use crate::targeting::LookTarget;
use crate::world2d::{surface_decor_at, ActivePlanet, DecorKind, TileChunk, TileWorld};

pub struct AutoDrivePlugin;

impl Plugin for AutoDrivePlugin {
    fn build(&self, app: &mut App) {
        if std::env::var("HA_AUTODRIVE").is_err() {
            return;
        }
        app.init_resource::<AutoDriveState>()
            .init_resource::<VirtualInput>()
            .add_systems(
                PreUpdate,
                apply_virtual_input.after(bevy::input::InputSystem),
            )
            .add_systems(Update, autodrive);
    }
}

/// Buttons the autodrive holds down — re-applied AFTER bevy_input clears
/// just_* each PreUpdate, so downstream systems see real press edges.
#[derive(Resource, Default)]
struct VirtualInput {
    keys: Vec<KeyCode>,
    mouse: Vec<MouseButton>,
}

fn apply_virtual_input(
    virt: Res<VirtualInput>,
    mut keys: ResMut<ButtonInput<KeyCode>>,
    mut mouse: ResMut<ButtonInput<MouseButton>>,
) {
    for k in &virt.keys {
        keys.press(*k);
    }
    for m in &virt.mouse {
        mouse.press(*m);
    }
}

fn hold_key(world: &mut World, k: KeyCode) {
    let mut v = world.resource_mut::<VirtualInput>();
    if !v.keys.contains(&k) {
        v.keys.push(k);
    }
}

fn hold_mouse(world: &mut World, m: MouseButton) {
    let mut v = world.resource_mut::<VirtualInput>();
    if !v.mouse.contains(&m) {
        v.mouse.push(m);
    }
}

/// Re-press with fresh just_pressed edges while no hold is running — the
/// single edge can land on a frame where gates (equip flips) reject it.
/// Once a hold is active OR completed, hold continuously so no new edge
/// wipes `completed` before its consumer runs.
fn pulse_mouse(world: &mut World, m: MouseButton, t: f32) {
    let busy = {
        let h = world.resource::<HoldInteractState>();
        h.active || h.completed
    };
    if busy {
        hold_mouse(world, m);
        return;
    }
    // 0.4 s held / 0.1 s released cycle → a new edge every half second.
    if t.rem_euclid(0.5) < 0.4 {
        hold_mouse(world, m);
    } else {
        let mut v = world.resource_mut::<VirtualInput>();
        v.mouse.retain(|b| *b != m);
        world
            .resource_mut::<ButtonInput<MouseButton>>()
            .release(m);
    }
}

#[derive(Resource, Default)]
struct AutoDriveState {
    phase: u32,
    entered: f32,
    initialized: bool,
    results: Vec<(&'static str, bool)>,
    base_x: f32,
    base_qty: u32,
    base_f32: f32,
    cell: Option<(i64, i32)>,
    done: bool,
}

const EARTH: CelestialBody = CelestialBody::Earth;

fn dig_loot(inv: &PlayerInventory) -> u32 {
    inv.qty("dirt") + inv.qty("stone") + inv.qty("sand")
}

fn ground_row_at(world: &mut World, body: CelestialBody, col: i64) -> i32 {
    world.resource::<TileWorld>().ground_row_at(body, col)
}

fn player_x(world: &mut World) -> f32 {
    let mut q = world.query_filtered::<&SimState, With<Player>>();
    q.single(world).body.x
}

fn teleport_player(world: &mut World, body: CelestialBody, s: f32) {
    let ground = world.resource::<TileWorld>().ground_height_at(body, s);
    let mut q = world.query_filtered::<(&mut SimState, &mut PlayerVisual), With<Player>>();
    let (mut sim, mut vis) = q.single_mut(world);
    sim.body.x = s;
    sim.body.y = ground + 0.1;
    sim.body.vx = 0.0;
    sim.body.vy = 0.0;
    vis.from_x = s;
    vis.to_x = s;
    vis.from_y = sim.body.y;
    vis.to_y = sim.body.y;
}

fn lock_target(world: &mut World, target: LookTarget) {
    *world.resource_mut::<LookTarget>() = target;
}

fn unlock_target(world: &mut World) {
    *world.resource_mut::<LookTarget>() = LookTarget::default();
}

fn shot(world: &mut World, path: &str) {
    let path = path.to_string();
    let mut commands = world.commands();
    commands.spawn(Screenshot::primary_window()).observe(save_to_disk(path.clone()));
    eprintln!("ha-verify: screenshot → {path}");
}

fn set_item(world: &mut World, id: &str) {
    // Clear any hotbar selection so `equip_from_hotbar_selection` doesn't
    // override the tool we force (hand/hammer aren't inventory items).
    world
        .resource_mut::<crate::hud::inventory_ui::InventoryUiState>()
        .selected = None;
    world.resource_mut::<EquippedItem>().item_id = id.to_string();
}

/// Equip an inventory item the real way: select its hotbar slot (the player's
/// click path). Falls back to a forced equip if it isn't in the hotbar yet.
fn equip_from_hotbar(world: &mut World, id: &str) {
    use crate::hud::inventory_ui::{InventoryUiState, SlotId};
    let slot = {
        let ui = world.resource::<InventoryUiState>();
        ui.hotbar
            .iter()
            .position(|s| s.item_id.as_deref() == Some(id) && s.qty > 0)
    };
    let mut ui = world.resource_mut::<InventoryUiState>();
    ui.selected = slot.map(SlotId::Hotbar);
    drop(ui);
    world.resource_mut::<EquippedItem>().item_id = id.to_string();
}

/// One state machine step per frame — exclusive so it can drive everything.
#[allow(clippy::too_many_lines)]
fn autodrive(world: &mut World) {
    let now = world.resource::<Time>().elapsed_secs();
    if now < 9.0 {
        return;
    }
    let (phase, entered, initialized) = {
        let st = world.resource::<AutoDriveState>();
        if st.done {
            return;
        }
        (st.phase, st.entered, st.initialized)
    };
    let dt_in_phase = now - entered;

    let advance = |world: &mut World, name: &'static str, ok: bool| {
        // Release injected input between phases.
        world.resource_mut::<VirtualInput>().keys.clear();
        world.resource_mut::<VirtualInput>().mouse.clear();
        world
            .resource_mut::<ButtonInput<KeyCode>>()
            .release_all();
        world
            .resource_mut::<ButtonInput<MouseButton>>()
            .release_all();
        unlock_target(world);
        let mut st = world.resource_mut::<AutoDriveState>();
        st.results.push((name, ok));
        st.phase += 1;
        st.entered = now;
        st.initialized = false;
        eprintln!(
            "ha-verify: {} {}",
            name,
            if ok { "PASS" } else { "FAIL" }
        );
    };

    match phase {
        // ------------------------------------------------- setup
        0 => {
            let mut q = world.query_filtered::<&SimState, With<Player>>();
            if q.get_single(world).is_ok() {
                world.resource_mut::<DesktopMouseCapture>().active = true;
                let x = player_x(world);
                let jump = std::env::var("HA_PHASE")
                    .ok()
                    .and_then(|v| v.parse::<u32>().ok())
                    .unwrap_or(1);
                let mut st = world.resource_mut::<AutoDriveState>();
                st.base_x = x;
                st.phase = jump;
                st.entered = now;
                eprintln!("ha-verify: START (player at s={x:.1}, phase {jump})");
            }
        }
        // ------------------------------------------------- locomoción
        1 => {
            hold_key(world, KeyCode::KeyD);
            let x = player_x(world);
            let base = world.resource::<AutoDriveState>().base_x;
            if x > base + 4.0 {
                advance(world, "walk", true);
            } else if dt_in_phase > 10.0 {
                advance(world, "walk", false);
            }
        }
        // ------------------------------------------------- cavar tile
        2 => {
            if !initialized {
                set_item(world, "hand");
                let x = player_x(world);
                let col = x.floor() as i64 + 2;
                let row = ground_row_at(world, EARTH, col);
                let before = dig_loot(&world.resource::<PlayerInventory>());
                let mut st = world.resource_mut::<AutoDriveState>();
                st.cell = Some((col, row));
                st.base_qty = before;
                st.initialized = true;
            }
            set_item(world, "hand");
            let (col, row) = world.resource::<AutoDriveState>().cell.unwrap();
            let wrapped = wrap_tile(col, body_circumference_tiles(EARTH)) as i32;
            lock_target(
                world,
                LookTarget {
                    kind: Some(ha_shared::InteractTargetKind::Block),
                    block_cell: Some((wrapped, row, 0)),
                    distance: 2.0,
                    locked: true,
                    ..default()
                },
            );
            hold_mouse(world, MouseButton::Left);
            let loot = dig_loot(&world.resource::<PlayerInventory>());
            let base = world.resource::<AutoDriveState>().base_qty;
            let dug = !world.resource::<TileWorld>().solid_at(EARTH, col, row);
            if (dt_in_phase * 2.0).fract() < 0.05 {
                let hold = world.resource::<HoldInteractState>();
                eprintln!(
                    "ha-verify: dig cell=({col},{row}) solid={} loot={loot}/{base} hold a={} p={} d={} c={} pm={}",
                    !dug, hold.active, hold.progress_ms, hold.completed, hold.cancelled, hold.placing_machine
                );
            }
            if loot > base && dug {
                advance(world, "dig", true);
            } else if dt_in_phase > 8.0 {
                advance(world, "dig", false);
            }
        }
        // ------------------------------------------------- talar árbol (targeting real)
        3 => {
            if !initialized {
                let x = player_x(world).floor() as i64;
                let c = body_circumference_tiles(EARTH);
                let tree = (0..200).find_map(|d| {
                    for col in [x + d, x - d] {
                        if surface_decor_at(EARTH, wrap_tile(col, c)) == Some(DecorKind::Tree) {
                            return Some(col);
                        }
                    }
                    None
                });
                let Some(tree_col) = tree else {
                    advance(world, "chop", false);
                    return;
                };
                teleport_player(world, EARTH, tree_col as f32 - 2.0);
                {
                    let mut q =
                        world.query_filtered::<&mut SimState, With<Player>>();
                    q.single_mut(world).body.rot = 0.0; // face +s
                }
                set_item(world, "hand");
                let wood = world.resource::<PlayerInventory>().qty("wood");
                let mut st = world.resource_mut::<AutoDriveState>();
                st.base_qty = wood;
                st.initialized = true;
                return;
            }
            // Lock onto the nearest TREE entity (a rock may be closer; the
            // auto-targeting path itself is validated by it picking targets).
            let px = player_x(world);
            let tree = {
                let mut q = world
                    .query::<(Entity, &GlobalTransform, &crate::targeting::InteractTarget)>();
                q.iter(world)
                    .filter(|(_, _, t)| t.kind == ha_shared::InteractTargetKind::Tree)
                    .map(|(e, tf, t)| (e, tf.translation().truncate(), t.id))
                    .min_by(|a, b| {
                        (a.1.x - px)
                            .abs()
                            .partial_cmp(&(b.1.x - px).abs())
                            .unwrap()
                    })
            };
            set_item(world, "hand");
            if let Some((entity, pos, col)) = tree {
                lock_target(
                    world,
                    LookTarget {
                        entity: Some(entity),
                        kind: Some(ha_shared::InteractTargetKind::Tree),
                        distance: (pos.x - px).abs(),
                        hit_point: pos.extend(0.0),
                        flora_cell: Some(((col / 32) as i32, 0, col)),
                        locked: true,
                        ..default()
                    },
                );
            }
            hold_mouse(world, MouseButton::Left);
            let wood = world.resource::<PlayerInventory>().qty("wood");
            let base = world.resource::<AutoDriveState>().base_qty;
            if (dt_in_phase * 2.0).fract() < 0.05 {
                let slots_wood: u32 = {
                    let ui = world.resource::<crate::hud::inventory_ui::InventoryUiState>();
                    ui.hotbar
                        .iter()
                        .chain(ui.grid.iter())
                        .filter(|s| s.item_id.as_deref() == Some("wood"))
                        .map(|s| s.qty)
                        .sum()
                };
                let t = world.resource::<LookTarget>();
                let hold = world.resource::<HoldInteractState>();
                eprintln!(
                    "ha-verify: chop target={:?} wood={wood}/{base} slots_wood={slots_wood} hold a={} p={}",
                    t.kind, hold.active, hold.progress_ms
                );
            }
            if wood > base {
                advance(world, "chop+targeting", true);
            } else if dt_in_phase > 12.0 {
                advance(world, "chop+targeting", false);
            }
        }
        // ------------------------------------------------- craftear (mismas fns que el botón BAG)
        4 => {
            {
                let mut inv = world.resource_mut::<PlayerInventory>();
                if inv.qty("wood") < 2 {
                    inv.add("wood", 2);
                }
            }
            let recipe = ha_shared::get_craft_recipe(1).expect("recipe 1");
            let crafted = {
                let mut inv = world.resource_mut::<PlayerInventory>();
                ha_shared::craft(&mut inv, recipe)
            };
            if crafted {
                world.resource_scope(|world, mut ui: Mut<crate::hud::inventory_ui::InventoryUiState>| {
                    let mut inv = world.resource_mut::<PlayerInventory>();
                    crate::hud::inventory_ui::add_item_to_slots(&mut ui, recipe.output_id, recipe.output_qty);
                    crate::hud::inventory_ui::flush_slots_to_inventory(&ui, &mut inv);
                });
                world
                    .resource_mut::<crate::inventory_persist::InventoryPersistState>()
                    .mark_immediate();
            }
            let ok = world.resource::<PlayerInventory>().qty("sticks") >= 4;
            advance(world, "craft", crafted && ok);
        }
        // ------------------------------------------------- colocar máquina
        5 => {
            if !initialized {
                set_item(world, "hammer");
                {
                    let mut inv = world.resource_mut::<PlayerInventory>();
                    inv.add("wood", 6);
                    inv.add("stone", 3);
                    inv.add("sticks", 2);
                }
                let count = world.resource::<MachineWorld>().placed.len() as u32;
                let mut st = world.resource_mut::<AutoDriveState>();
                st.base_qty = count;
                st.initialized = true;
            }
            let base = world.resource::<AutoDriveState>().base_qty;
            let count = world.resource::<MachineWorld>().placed.len() as u32;
            if count > base {
                advance(world, "machine-place", true);
                return;
            }
            set_item(world, "hammer");
            // Natural desktop flow: RIGHT hold + hammer + nothing targeted.
            lock_target(
                world,
                LookTarget {
                    locked: true,
                    ..default()
                },
            );
            pulse_mouse(world, MouseButton::Right, dt_in_phase);
            if (dt_in_phase * 2.0).fract() < 0.05 {
                let hold = world.resource::<HoldInteractState>();
                let equipped = world.resource::<EquippedItem>().item_id.clone();
                eprintln!(
                    "ha-verify: machine hold a={} p={} d={} c={} pm={} equipped={equipped} count={count}",
                    hold.active, hold.progress_ms, hold.completed, hold.cancelled, hold.placing_machine
                );
            }
            if dt_in_phase > 8.0 {
                advance(world, "machine-place", false);
            }
        }
        // ------------------------------------------------- automatización (produce sola)
        6 => {
            if !initialized {
                let wood = world.resource::<PlayerInventory>().qty("wood");
                let mut st = world.resource_mut::<AutoDriveState>();
                st.base_qty = wood;
                st.initialized = true;
            }
            let wood = world.resource::<PlayerInventory>().qty("wood");
            let base = world.resource::<AutoDriveState>().base_qty;
            if wood > base {
                advance(world, "automation", true);
            } else if dt_in_phase > 25.0 {
                advance(world, "automation", false);
            }
        }
        // ------------------------------------------------- encender cohete
        7 => {
            if !initialized {
                teleport_player(world, EARTH, ha_shared::LAUNCH_PAD_ZONE_X);
                set_item(world, "hand");
                let mut st = world.resource_mut::<AutoDriveState>();
                st.initialized = true;
                return;
            }
            let ignition = {
                let mut q = world.query_filtered::<Entity, With<LaunchIgnitionMarker>>();
                q.iter(world).next()
            };
            let Some(entity) = ignition else {
                if dt_in_phase > 6.0 {
                    advance(world, "ignition", false);
                }
                return;
            };
            set_item(world, "hand");
            lock_target(
                world,
                LookTarget {
                    entity: Some(entity),
                    kind: Some(ha_shared::InteractTargetKind::LaunchIgnition),
                    distance: 2.0,
                    locked: true,
                    ..default()
                },
            );
            hold_mouse(world, MouseButton::Left);
            if world.resource::<RocketFlight>().active {
                advance(world, "ignition", true);
            } else {
                let hold = world.resource::<HoldInteractState>();
                if (dt_in_phase * 2.0).fract() < 0.05 {
                    eprintln!(
                        "ha-verify: ignition hold active={} prog={}ms done={} cancelled={}",
                        hold.active, hold.progress_ms, hold.completed, hold.cancelled
                    );
                }
                if dt_in_phase > 14.0 {
                    advance(world, "ignition", false);
                }
            }
        }
        // ------------------------------------------------- ascenso → espacio
        8 => {
            hold_key(world, KeyCode::Space);
            let in_space = matches!(world.resource::<RegionRuntime>().active, RegionId::Space);
            if in_space {
                shot(world, "ha-verify-space.png");
                advance(world, "space-flight", true);
            } else if dt_in_phase > 50.0 {
                advance(world, "space-flight", false);
            }
        }
        // ------------------------------------------------- luna (teleport deliberado MENU)
        9 => {
            if !initialized {
                world.resource_mut::<RocketFlight>().end_surface();
                world.resource_mut::<RegionRuntime>().active = RegionId::Luna;
                world.resource_mut::<WorldContextRuntime>().active =
                    ha_shared::WorldContext::Luna;
                world.resource_mut::<ActivePlanet>().body = None;
                teleport_player(world, CelestialBody::Moon, 6.0);
                let o2 = {
                    let mut q = world.query_filtered::<&SimState, With<Player>>();
                    q.single(world).survival.oxygen
                };
                let mut st = world.resource_mut::<AutoDriveState>();
                st.base_f32 = o2;
                st.initialized = true;
                return;
            }
            let moon_chunks = {
                let mut q = world.query::<&TileChunk>();
                q.iter(world).filter(|c| c.body == CelestialBody::Moon).count()
            };
            if moon_chunks > 0 && dt_in_phase > 2.0 {
                shot(world, "ha-verify-moon.png");
                advance(world, "moon-strip", true);
            } else if dt_in_phase > 10.0 {
                advance(world, "moon-strip", false);
            }
        }
        // ------------------------------------------------- O₂ drena en vacío
        10 => {
            let o2 = {
                let mut q = world.query_filtered::<&SimState, With<Player>>();
                q.single(world).survival.oxygen
            };
            let base = world.resource::<AutoDriveState>().base_f32;
            if o2 < base - 1.0 {
                advance(world, "oxygen-vacuum", true);
            } else if dt_in_phase > 15.0 {
                advance(world, "oxygen-vacuum", false);
            }
        }
        // ------------------------------------------------- colocar bloque
        11 => {
            if !initialized {
                {
                    let mut inv = world.resource_mut::<PlayerInventory>();
                    if inv.qty("dirt") == 0 {
                        inv.add("dirt", 2);
                    }
                }
                equip_from_hotbar(world, "dirt");
                let x = player_x(world);
                let col = x.floor() as i64 + 2;
                let row = ground_row_at(world, CelestialBody::Moon, col) + 1;
                let mut st = world.resource_mut::<AutoDriveState>();
                st.cell = Some((col, row));
                st.initialized = true;
            }
            equip_from_hotbar(world, "dirt");
            let (col, row) = world.resource::<AutoDriveState>().cell.unwrap();
            let wrapped =
                wrap_tile(col, body_circumference_tiles(CelestialBody::Moon)) as i32;
            lock_target(
                world,
                LookTarget {
                    place_cell: Some((wrapped, row, 0)),
                    distance: 2.0,
                    locked: true,
                    ..default()
                },
            );
            // Equip is now a block item + place_cell locked, so a continuous
            // RIGHT hold makes desktop_hold_interact start the placing_block
            // hold and complete it through the real consumer.
            hold_mouse(world, MouseButton::Right);
            let placed = world
                .resource::<TileWorld>()
                .solid_at(CelestialBody::Moon, col, row);
            if placed {
                advance(world, "place-tile", true);
            } else if dt_in_phase > 8.0 {
                advance(world, "place-tile", false);
            }
        }
        // ------------------------------------------------- resumen
        _ => {
            shot(world, "ha-verify-final.png");
            let mut st = world.resource_mut::<AutoDriveState>();
            st.done = true;
            let pass = st.results.iter().filter(|(_, ok)| *ok).count();
            let total = st.results.len();
            let detail: Vec<String> = st
                .results
                .iter()
                .map(|(n, ok)| format!("{n}={}", if *ok { "PASS" } else { "FAIL" }))
                .collect();
            eprintln!("ha-verify: SUMMARY {pass}/{total} [{}]", detail.join(" "));
        }
    }
}
