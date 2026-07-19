//! Top-down smoke test (HA_AUTODRIVE=1 + HA_AUTOPLAY=1): drives the player
//! through the REAL input pipeline to confirm the game boots, renders the
//! top-down world, and the player walks. Screenshots at key beats.
//!
//! NOTE: the full 11-system verification (dig/chop/craft/machine/ignition/…)
//! from the side-scroller is being re-ported to top-down in a later phase.
//! This lean version keeps the build green and gives a visual check for Fase 2.

use bevy::input::ButtonInput;
use bevy::prelude::*;
use bevy::render::view::screenshot::{save_to_disk, Screenshot};

use crate::input::HoldInteractState;
use crate::inventory_persist::PlayerInventory;
use crate::sim::{Player, PlayerVisual, SimState};
use crate::targeting::LookTarget;
use crate::world2d::TileWorld;

pub struct AutoDrivePlugin;

impl Plugin for AutoDrivePlugin {
    fn build(&self, app: &mut App) {
        // Creative-fly visual test: enable creative, hold JUMP, screenshot the
        // whole ascent so we can SEE it (HA_FLYTEST=1 HA_AUTOPLAY=1).
        if std::env::var("HA_FLYTEST").is_ok() {
            app.init_resource::<FlyTestState>()
                .add_systems(Update, fly_test_drive);
            return;
        }
        if std::env::var("HA_AUTODRIVE").is_err() {
            return;
        }
        app.init_resource::<SmokeState>()
            .add_systems(Update, smoke_drive);
    }
}

#[derive(Resource, Default)]
struct FlyTestState {
    triggered: bool,
    shots: u32,
}

const FLYTEST_DIR: &str =
    "/tmp/claude-1000/-home-fzion-Documents-mods-phaser-arena/501aea2c-9fc5-4caa-9230-ef1403004e02/scratchpad";

fn fly_test_drive(world: &mut World) {
    let now = world.resource::<Time>().elapsed_secs();
    if now < 1.0 {
        return;
    }
    // Enable creative every frame (so nothing turns it back off).
    world.resource_mut::<crate::ui_editor::UiEditorState>().creative = true;

    // From t=3s: hold JUMP (Space) through the REAL input pipeline — first frame
    // gives the jump_edge that fires the fly, then it stays held to keep rising.
    if now > 3.0 {
        press(world, KeyCode::Space);
    }

    // Screenshot the ascent at a sequence of times.
    let want = [3.2f32, 3.5, 3.9, 4.3, 4.9, 6.0, 8.0, 11.0];
    let shots = world.resource::<FlyTestState>().shots as usize;
    if shots < want.len() && now >= want[shots] {
        let (alt, cloud, orbit) = {
            let f = world.resource::<crate::creative_fly::CreativeFlight>();
            (f.alt, f.cloud, f.in_orbit())
        };
        let region = format!("{:?}", world.resource::<crate::scene::RegionRuntime>().active);
        eprintln!(
            "ha-flytest[{shots}] t={now:.1} alt={alt:.0} cloud={cloud:.2} orbit={orbit} region={region}"
        );
        shot(world, &format!("{FLYTEST_DIR}/flytest-{shots}.png"));
        world.resource_mut::<FlyTestState>().shots += 1;
    } else if shots >= want.len() {
        eprintln!("ha-flytest: DONE");
        std::process::exit(0);
    }
}

#[derive(Resource, Default)]
struct SmokeState {
    phase: u32,
    entered: f32,
    start_x: f32,
    cell: (i64, i64),
    done: bool,
}


fn player_cell(world: &mut World) -> (i64, i64) {
    let mut q = world.query_filtered::<&SimState, With<Player>>();
    let s = q.single(world);
    (s.body.x.floor() as i64, s.body.z.floor() as i64)
}

fn dig_loot(world: &mut World) -> u32 {
    let inv = world.resource::<PlayerInventory>();
    inv.qty("dirt") + inv.qty("stone") + inv.qty("sand")
}

fn equip(world: &mut World, id: &str) {
    // Clear any hotbar selection so `equip_from_hotbar_selection` doesn't
    // override the forced tool next frame.
    world
        .resource_mut::<crate::hud::inventory_ui::InventoryUiState>()
        .selected = None;
    world.resource_mut::<crate::input::EquippedItem>().item_id = id.to_string();
}

fn dig_setup(world: &mut World, cell: (i64, i64)) {
    equip(world, "hand");
    world.resource_mut::<SmokeState>().cell = cell;
}

fn place_setup(world: &mut World, cell: (i64, i64)) {
    world.resource_mut::<PlayerInventory>().add("stone", 5);
    equip(world, "stone");
    world.resource_mut::<SmokeState>().cell = cell;
}

fn lock_block_target(world: &mut World, tx: i64, tz: i64) {
    *world.resource_mut::<LookTarget>() = LookTarget {
        kind: Some(ha_shared::InteractTargetKind::Block),
        block_cell: Some((tx as i32, tz as i32, 0)),
        locked: true,
        ..default()
    };
}

fn lock_place_target(world: &mut World, tx: i64, tz: i64) {
    *world.resource_mut::<LookTarget>() = LookTarget {
        place_cell: Some((tx as i32, tz as i32, 0)),
        locked: true,
        ..default()
    };
}

fn complete_hold(world: &mut World, placing_block: bool) {
    let mut h = world.resource_mut::<HoldInteractState>();
    h.active = false;
    h.completed = true;
    h.placing_block = placing_block;
    h.placing_machine = false;
}

fn unlock(world: &mut World) {
    *world.resource_mut::<LookTarget>() = LookTarget::default();
    let mut h = world.resource_mut::<HoldInteractState>();
    h.completed = false;
    h.placing_block = false;
}

fn player_x(world: &mut World) -> f32 {
    let mut q = world.query_filtered::<&SimState, With<Player>>();
    q.get_single(world).map(|s| s.body.x).unwrap_or(0.0)
}

fn shot(world: &mut World, path: &str) {
    let path = path.to_string();
    let mut commands = world.commands();
    commands
        .spawn(Screenshot::primary_window())
        .observe(save_to_disk(path.clone()));
    eprintln!("ha-verify: screenshot → {path}");
}

fn press(world: &mut World, k: KeyCode) {
    world.resource_mut::<ButtonInput<KeyCode>>().press(k);
}

fn release_all(world: &mut World) {
    world.resource_mut::<ButtonInput<KeyCode>>().release_all();
}

fn smoke_drive(world: &mut World) {
    let now = world.resource::<Time>().elapsed_secs();
    if now < 9.0 {
        return;
    }
    let (phase, entered, done) = {
        let s = world.resource::<SmokeState>();
        (s.phase, s.entered, s.done)
    };
    if done {
        return;
    }
    let dt = now - entered;

    match phase {
        0 => {
            // Place the aim cursor a couple tiles from the player so the
            // yellow cell highlight is visible in the boot shot.
            let (px, pz) = {
                let mut q = world.query_filtered::<&SimState, With<Player>>();
                let s = q.single(world);
                (s.body.x, s.body.z)
            };
            {
                let mut c = world.resource_mut::<crate::input::AimCursor>();
                c.world = bevy::prelude::Vec2::new(px + 2.5, pz - 1.5);
                c.initialized = true;
            }
            let x = player_x(world);
            shot(world, "ha-verify-topdown-boot.png");
            // Open the unified menu for a capture.
            world.resource_mut::<crate::ui_editor::UiEditorState>().menu_open = true;
            let mut s = world.resource_mut::<SmokeState>();
            s.start_x = x;
            s.phase = 20;
            s.entered = now;
            eprintln!("ha-verify: boot (player x={x:.1})");
        }
        20 => {
            if dt > 1.0 {
                shot(world, "ha-verify-topdown-menu.png");
                eprintln!("ha-verify: menu captured");
                world.resource_mut::<crate::ui_editor::UiEditorState>().menu_open = false;
                world.resource_mut::<SmokeState>().phase = 1;
                world.resource_mut::<SmokeState>().entered = now;
            }
        }
        1 => {
            // Walk east (D) for ~3 s, then check we moved.
            press(world, KeyCode::KeyD);
            if dt > 3.0 {
                let x = player_x(world);
                let start = world.resource::<SmokeState>().start_x;
                let moved = x > start + 2.0;
                eprintln!(
                    "ha-verify: walk-east {} (Δx={:.1})",
                    if moved { "PASS" } else { "FAIL" },
                    x - start
                );
                release_all(world);
                shot(world, "ha-verify-topdown-walk.png");
                // Dig the tile in front (inject the completed break hold →
                // real consumer apply_tile_hold digs + grants).
                let (tx, tz) = player_cell(world);
                let cell = (tx + 2, tz);
                dig_setup(world, cell);
                world.resource_mut::<SmokeState>().phase = 10;
                world.resource_mut::<SmokeState>().entered = now;
                world.resource_mut::<SmokeState>().start_x = x;
            }
        }
        10 => {
            // Digging: keep the break hold locked/completed until the tile opens.
            equip(world, "hand");
            let (tx, tz) = world.resource::<SmokeState>().cell;
            lock_block_target(world, tx, tz);
            complete_hold(world, false);
            let dug = !world
                .resource::<TileWorld>()
                .solid_at(ha_shared::CelestialBody::Earth, tx, tz);
            let loot = dig_loot(world);
            if dug || loot > 0 {
                eprintln!("ha-verify: dig PASS (dug={dug}, loot={loot})");
                unlock(world);
                // Now place a block on that freshly-dug cell's neighbour.
                place_setup(world, (tx, tz));
                world.resource_mut::<SmokeState>().phase = 11;
                world.resource_mut::<SmokeState>().entered = now;
            } else if dt > 4.0 {
                eprintln!("ha-verify: dig FAIL");
                unlock(world);
                world.resource_mut::<SmokeState>().phase = 2;
                world.resource_mut::<SmokeState>().entered = now;
            }
        }
        11 => {
            equip(world, "stone");
            let (tx, tz) = world.resource::<SmokeState>().cell;
            lock_place_target(world, tx, tz);
            complete_hold(world, true);
            let placed = world
                .resource::<TileWorld>()
                .solid_at(ha_shared::CelestialBody::Earth, tx, tz);
            if placed {
                eprintln!("ha-verify: place PASS");
                unlock(world);
                world.resource_mut::<SmokeState>().phase = 12;
                world.resource_mut::<SmokeState>().entered = now;
            } else if dt > 4.0 {
                eprintln!("ha-verify: place FAIL");
                unlock(world);
                world.resource_mut::<SmokeState>().phase = 12;
                world.resource_mut::<SmokeState>().entered = now;
            }
        }
        12 => {
            // Build system: place a wood floor via the real build-hold consumer.
            world.resource_mut::<crate::input::BuildModeActive>().active = true;
            world.resource_mut::<crate::build::SelectedBuildPiece>().0 = 0;
            if world.resource::<PlayerInventory>().qty("wood") < 3 {
                world.resource_mut::<PlayerInventory>().add("wood", 5);
            }
            let (bx, bz) = player_cell(world);
            let (tx, tz) = (bx + 1, bz + 2);
            lock_place_target(world, tx, tz);
            // Point the aim cursor at the build cell so highlight + ghost align.
            {
                let mut c = world.resource_mut::<crate::input::AimCursor>();
                c.world = bevy::prelude::Vec2::new(tx as f32 + 0.5, tz as f32 + 0.5);
                c.initialized = true;
            }
            // Open the bag's CONSTRUIR tab for a capture, then close it.
            if dt < 0.5 {
                let mut bag = world.resource_mut::<crate::hud::BagPanelState>();
                bag.open = true;
                bag.tab = crate::hud::BagTab::Build;
            }
            if dt > 0.3 && dt < 0.5 {
                shot(world, "ha-verify-bag-build.png");
            }
            if dt >= 0.5 {
                world.resource_mut::<crate::hud::BagPanelState>().open = false;
            }
            if dt > 0.9 && dt < 1.1 {
                shot(world, "ha-verify-topdown-build.png");
            }
            // Place only after the captures, so the ghost shows on empty ground.
            if dt > 1.2 {
                let mut h = world.resource_mut::<HoldInteractState>();
                h.active = false;
                h.completed = true;
                h.placing_build = true;
            }
            let built = world
                .resource::<TileWorld>()
                .tile_at(ha_shared::CelestialBody::Earth, tx, tz)
                == ha_shared::topdown::Tile::WoodFloor;
            if built && dt > 1.3 || dt > 4.0 {
                eprintln!("ha-verify: build {}", if built { "PASS" } else { "FAIL" });
                unlock(world);
                world.resource_mut::<crate::input::BuildModeActive>().active = false;
                world.resource_mut::<SmokeState>().phase = 2;
                world.resource_mut::<SmokeState>().entered = now;
            }
        }
        2 => {
            // Walk south (S) briefly to confirm the vertical axis works.
            press(world, KeyCode::KeyS);
            if dt > 2.5 {
                release_all(world);
                shot(world, "ha-verify-topdown-walk2.png");
                // Teleport onto the mine entrance to test the Z-Level descent.
                let (px, pz) = ha_shared::topdown::descend_portal(
                    ha_shared::CelestialBody::Earth,
                    0,
                );
                let wx = px as f32 + 0.5;
                let wz = pz as f32 + 0.5;
                {
                    let mut q = world
                        .query_filtered::<(&mut SimState, &mut PlayerVisual), With<Player>>();
                    if let Ok((mut sim, mut vis)) = q.get_single_mut(world) {
                        sim.body.x = wx;
                        sim.body.z = wz;
                        vis.from_x = wx;
                        vis.to_x = wx;
                        vis.from_z = wz;
                        vis.to_z = wz;
                    }
                }
                world.resource_mut::<SmokeState>().phase = 3;
                world.resource_mut::<SmokeState>().entered = now;
            }
        }
        3 => {
            // The layer_transition should have dropped us to z=-1.
            if dt > 1.5 {
                let z = world.resource::<TileWorld>().z;
                eprintln!(
                    "ha-verify: descend {} (z={z})",
                    if z < 0 { "PASS" } else { "FAIL" }
                );
                shot(world, "ha-verify-topdown-cave.png");
                // Back to surface, then ignite a rocket to test space (Fase 4).
                world.resource_mut::<TileWorld>().z = 0;
                {
                    let clk = world.resource::<crate::solar_system_clock::SolarSystemClock>().t;
                    let (pos, vel) = crate::rocket_flight::launch_pos_vel(
                        ha_shared::CelestialBody::Earth,
                        clk,
                        60.0,
                    );
                    let mut flight = world.resource_mut::<crate::rocket_flight::RocketFlight>();
                    flight.clock_secs = clk;
                    flight.surface_flight = Some(ha_shared::CelestialBody::Earth);
                    flight.begin_at(pos);
                    flight.fuel = flight.capacity();
                    flight.vel = vel;
                }
                world.resource_mut::<SmokeState>().phase = 4;
                world.resource_mut::<SmokeState>().entered = now;
            }
        }
        4 => {
            // Hold thrust; after a few seconds we should be in Space (orbital).
            press(world, KeyCode::Space);
            if dt > 4.0 {
                let in_space = matches!(
                    world.resource::<crate::scene::RegionRuntime>().active,
                    crate::scene::RegionId::Space
                );
                let alt = world.resource::<crate::rocket_flight::RocketFlight>().altitude_m();
                eprintln!(
                    "ha-verify: space-flight {} (region_space={in_space}, alt={alt:.0})",
                    if in_space { "PASS" } else { "FAIL" }
                );
                release_all(world);
                shot(world, "ha-verify-topdown-space.png");
                // Open the menu in Space to confirm the orbital map still shows.
                world.resource_mut::<crate::ui_editor::UiEditorState>().menu_open = true;
                world.resource_mut::<SmokeState>().phase = 21;
                world.resource_mut::<SmokeState>().entered = now;
            }
        }
        21 => {
            if dt > 1.0 {
                shot(world, "ha-verify-topdown-space-menu.png");
                eprintln!("ha-verify: space menu captured");
                eprintln!("ha-verify: SMOKE done");
                world.resource_mut::<SmokeState>().done = true;
            }
        }
        _ => {
            world.resource_mut::<SmokeState>().done = true;
        }
    }
}
