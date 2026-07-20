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
        // whole ascent so we can SEE it (HA_FLYTEST=1 HA_AUTOPLAY=1). Ordered
        // BEFORE the desktop key-reading system: as an exclusive `&mut World`
        // system, `fly_test_drive` is otherwise an unordered sync point
        // relative to it, so a same-frame `press()` could land AFTER that
        // system already read this frame's (stale) key state — the
        // `just_pressed` edge silently missed and never recurring since the
        // key stays "held". Explicit ordering removes that race entirely.
        if std::env::var("HA_FLYTEST").is_ok() {
            app.init_resource::<FlyTestState>();
            #[cfg(not(target_os = "android"))]
            app.add_systems(
                Update,
                fly_test_drive.before(crate::input::desktop_input::desktop_keyboard_actions),
            );
            #[cfg(target_os = "android")]
            app.add_systems(Update, fly_test_drive);
            return;
        }
        // Rocket transition visual test: ignite from the pad, thrust to space,
        // release and fall back to touchdown — screenshots at altitude beats on
        // BOTH legs so the surface↔space choreography can be SEEN
        // (HA_ROCKETTEST=1 HA_AUTOPLAY=1).
        if std::env::var("HA_ROCKETTEST").is_ok() {
            app.init_resource::<RocketTestState>();
            #[cfg(not(target_os = "android"))]
            app.add_systems(
                Update,
                rocket_test_drive.before(crate::input::desktop_input::desktop_keyboard_actions),
            );
            #[cfg(target_os = "android")]
            app.add_systems(Update, rocket_test_drive);
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
    shots: u32,
    /// Previous frame's camera `view_h`, to catch any jump automatically —
    /// this is exactly the bug that caused the "clouds cut, tiny blue circle
    /// appears" report: a discontinuous zoom at the surface↔orbit swap.
    prev_view_h: f32,
    max_jump_ratio: f32,
    /// Simple stage machine driving JUMP/USE by OBSERVED phase, not fixed
    /// timers, so it actually exercises the climb→orbit→descend→(abort back
    /// to orbit)→descend→touchdown round trip.
    stage: u8,
    stage_entered: f32,
    /// While `now < this`, skip zoom-jump comparisons — set right before OUR
    /// OWN synthetic state injections (stage 1/4 force `CreativeFlight`
    /// directly, which is NOT a real gameplay transition). A single-frame
    /// flag isn't reliable here: which frame the camera actually reacts to
    /// the forced state in depends on cross-plugin system ordering, which
    /// isn't guaranteed — a short TIME WINDOW covers it regardless.
    suppress_jump_until: f32,
    last_press_pulse: f32,
}

/// Keep `key` held (so the game reads it as a continuous hold) while
/// periodically releasing-then-repressing it to force a FRESH `just_pressed`
/// edge every `period` seconds. A single press-at-time-T can silently miss
/// its edge if that exact frame is gameplay-blocked (boot/loading screen) —
/// once "stuck held" the edge never recurs since the key never re-transitions.
/// Retrying removes the need to guess how long boot takes (it varies with
/// system load) instead of gating on a fixed timer.
fn hold_with_fresh_edges(world: &mut World, key: KeyCode, now: f32, period: f32) {
    let pulse = {
        let mut st = world.resource_mut::<FlyTestState>();
        let due = now - st.last_press_pulse > period;
        if due {
            st.last_press_pulse = now;
        }
        due
    };
    if pulse {
        release_all(world);
    }
    press(world, key);
}

const FLYTEST_DIR: &str =
    "/tmp/claude-1000/-home-fzion-Documents-mods-phaser-arena/6399287e-c6d0-477a-ada7-4482fa0e8175/scratchpad";

fn fly_test_drive(world: &mut World) {
    let now = world.resource::<Time>().elapsed_secs();
    if now < 1.0 {
        return;
    }
    // Enable creative every frame (so nothing turns it back off).
    world.resource_mut::<crate::ui_editor::UiEditorState>().creative = true;

    // Track the camera zoom every frame and flag any discontinuous jump
    // (ratio vs previous frame) — this is the exact bug class reported.
    let view_h = world.resource::<crate::camera::CameraZoom>().view_h;
    {
        let mut st = world.resource_mut::<FlyTestState>();
        if now < st.suppress_jump_until {
            // Still inside the post-injection grace window — don't compare,
            // but DO track this reading so comparisons resume cleanly once
            // the window closes (instead of comparing across the gap).
        } else if st.prev_view_h > 0.0 {
            let ratio = (view_h / st.prev_view_h).max(st.prev_view_h / view_h);
            if ratio > st.max_jump_ratio {
                st.max_jump_ratio = ratio;
            }
            if ratio > 1.5 {
                eprintln!(
                    "ha-flytest: ZOOM JUMP t={now:.2} {:.0} -> {:.0} (x{ratio:.2})",
                    st.prev_view_h, view_h
                );
            }
        }
        st.prev_view_h = view_h;
    }

    let phase = world.resource::<crate::creative_fly::CreativeFlight>().phase;
    let stage = world.resource::<FlyTestState>().stage;
    match stage {
        // 0: climb from the ground until Orbit (this leg already proves the
        // zoom-continuity fix via the jump detector above). The boot/loading
        // screen blocks gameplay input for a few seconds of VARIABLE length
        // (depends on system load) — `hold_with_fresh_edges` retries instead
        // of guessing a fixed delay.
        0 => {
            hold_with_fresh_edges(world, KeyCode::Space, now, 0.4);
            if phase == crate::creative_fly::Phase::Orbit {
                advance_stage(world, now);
            }
        }
        // 1: test setup — jump straight to "mid-descent" (the gameplay path to
        // get here is flying around in orbit for a while and choosing to land
        // far from any body; not worth scripting here). Mirrors exactly what
        // `creative_fly_orbit`'s landing branch does (region/ctx/phase/alt),
        // just at a deliberately mid-band altitude instead of whatever alt the
        // player happened to be at.
        1 => {
            release_all(world);
            {
                let mut fly = world.resource_mut::<crate::creative_fly::CreativeFlight>();
                fly.phase = crate::creative_fly::Phase::Descend;
                fly.alt = 150.0;
                fly.body = ha_shared::CelestialBody::Earth;
            }
            {
                let mut region = world.resource_mut::<crate::scene::RegionRuntime>();
                region.active = crate::scene::RegionId::Overworld { gx: 0, gz: 0 };
            }
            world.resource_mut::<crate::scene::WorldContextRuntime>().active =
                ha_shared::WorldContext::Overworld;
            // This is OUR synthetic teleport (not a real gameplay transition)
            // — don't let it register as a "zoom jump" regression.
            world.resource_mut::<FlyTestState>().suppress_jump_until = now + 0.5;
            eprintln!("ha-flytest: forced mid-descent (alt=150) for the abort test");
            advance_stage(world, now);
        }
        // 2: let the DEFAULT auto-fall run a beat first (bonus coverage: also
        // confirms the "release everything, it still lands on its own" path).
        2 => {
            release_all(world);
            if now - world.resource::<FlyTestState>().stage_entered > 1.5 {
                advance_stage(world, now);
            }
        }
        // 3: THE regression test — hold JUMP mid-descent and confirm it
        // actually climbs back to Orbit (used to just arrest the fall at
        // vy=0 — "los controles dejan de servir, no puedo subir").
        3 => {
            hold_with_fresh_edges(world, KeyCode::Space, now, 0.4);
            if phase == crate::creative_fly::Phase::Orbit {
                eprintln!("ha-flytest: mid-descent abort -> Orbit CONFIRMED");
                advance_stage(world, now);
            } else if now - world.resource::<FlyTestState>().stage_entered > 14.0 {
                // Budget: worst case climbs from ~120 m (after stage 2's 1.5 s
                // default fall from the forced 150 m) to swap_up (360 m on
                // Earth) at VERT_SPEED=32 m/s ≈ 7.5 s — 14 s leaves margin.
                eprintln!("ha-flytest: mid-descent abort FAILED (stuck in Descend)");
                std::process::exit(1);
            }
        }
        // 4: force mid-descent again (one-shot) and this time let it land for
        // real (natural default fall, no input), covering full touchdown.
        4 => {
            release_all(world);
            {
                let mut fly = world.resource_mut::<crate::creative_fly::CreativeFlight>();
                fly.phase = crate::creative_fly::Phase::Descend;
                fly.alt = 60.0;
            }
            world.resource_mut::<FlyTestState>().suppress_jump_until = now + 0.5; // synthetic, not a real jump
            advance_stage(world, now); // one-shot: don't re-force every frame
        }
        // 5: just wait for the natural fall to touch down.
        _ => {
            release_all(world);
        }
    }

    // Screenshot beats by observed phase change + a few time-based ones so we
    // still SEE the mid-climb/mid-descent frames, not just the transitions.
    let want = [4.0f32, 6.0, 8.0, 10.0, 11.5, 12.8, 14.5, 17.0, 19.0, 22.0, 26.0, 30.0];
    let shots = world.resource::<FlyTestState>().shots as usize;
    if shots < want.len() && now >= want[shots] {
        let (alt, orbit, phase) = {
            let f = world.resource::<crate::creative_fly::CreativeFlight>();
            (f.alt, f.in_orbit(), f.phase)
        };
        let cloud = world.resource::<crate::transition::FlightTransition>().cover;
        let region = format!("{:?}", world.resource::<crate::scene::RegionRuntime>().active);
        let view_h = world.resource::<crate::camera::CameraZoom>().view_h;
        eprintln!(
            "ha-flytest[{shots}] t={now:.1} alt={alt:.0} cloud={cloud:.2} orbit={orbit} phase={phase:?} region={region} view_h={view_h:.0}"
        );
        shot(world, &format!("{FLYTEST_DIR}/flytest-{shots}.png"));
        world.resource_mut::<FlyTestState>().shots += 1;
    }
    if stage >= 5 && world.resource::<crate::creative_fly::CreativeFlight>().phase
        == crate::creative_fly::Phase::Ground
    {
        let max_jump = world.resource::<FlyTestState>().max_jump_ratio;
        eprintln!("ha-flytest: max zoom jump ratio observed = x{max_jump:.2}");
        eprintln!("ha-flytest: {}", if max_jump > 1.5 { "ZOOM CONTINUITY FAIL" } else { "ZOOM CONTINUITY PASS" });
        eprintln!("ha-flytest: DONE");
        std::process::exit(if max_jump > 1.5 { 1 } else { 0 });
    }
    if now > 60.0 {
        eprintln!("ha-flytest: TIMEOUT (FAIL, stage={stage})");
        std::process::exit(1);
    }
}

fn advance_stage(world: &mut World, now: f32) {
    let mut st = world.resource_mut::<FlyTestState>();
    st.stage += 1;
    st.stage_entered = now;
}

#[derive(Resource, Default)]
struct RocketTestState {
    ignited: bool,
    released: bool,
    was_active: bool,
    up_idx: usize,
    down_idx: usize,
    landed_at: Option<f32>,
}

/// Altitude beats to capture going up / coming down (m). The 330 beat sits in
/// the fully-covered swap band; 500/800 show the orbital reveal.
const ROCKET_UP_ALTS: [f32; 6] = [30.0, 150.0, 250.0, 330.0, 500.0, 800.0];
const ROCKET_DOWN_ALTS: [f32; 4] = [500.0, 300.0, 150.0, 50.0];

fn rocket_test_drive(world: &mut World) {
    let now = world.resource::<Time>().elapsed_secs();
    // Wait out the boot screen so the first shots show the world.
    if now < 4.0 {
        return;
    }
    if now > 120.0 {
        eprintln!("ha-rockettest: TIMEOUT (FAIL)");
        std::process::exit(1);
    }
    if !world.resource::<RocketTestState>().ignited {
        let clk = world.resource::<crate::solar_system_clock::SolarSystemClock>().t;
        let (pos, vel) =
            crate::rocket_flight::launch_pos_vel(ha_shared::CelestialBody::Earth, clk, 60.0);
        let mut flight = world.resource_mut::<crate::rocket_flight::RocketFlight>();
        flight.clock_secs = clk;
        flight.surface_flight = Some(ha_shared::CelestialBody::Earth);
        flight.begin_at(pos);
        flight.fuel = flight.capacity();
        flight.vel = vel;
        world.resource_mut::<RocketTestState>().ignited = true;
        eprintln!("ha-rockettest: ignition");
        return;
    }

    let (active, alt) = {
        let f = world.resource::<crate::rocket_flight::RocketFlight>();
        (f.active && f.phase != ha_shared::FlightPhase::Surface, f.altitude_m())
    };
    let (cover, mode) = {
        let t = world.resource::<crate::transition::FlightTransition>();
        (t.cover, t.mode)
    };
    let region = format!("{:?}", world.resource::<crate::scene::RegionRuntime>().active);
    let st = world.resource::<RocketTestState>();
    let (released, up_idx, down_idx, was_active, landed_at) =
        (st.released, st.up_idx, st.down_idx, st.was_active, st.landed_at);

    if active {
        world.resource_mut::<RocketTestState>().was_active = true;
    }

    if !released {
        press(world, KeyCode::Space); // thrust
        if up_idx < ROCKET_UP_ALTS.len() && alt >= ROCKET_UP_ALTS[up_idx] {
            eprintln!(
                "ha-rockettest[up-{up_idx}] t={now:.1} alt={alt:.0} cover={cover:.2} mode={mode:?} region={region}"
            );
            shot(world, &format!("{FLYTEST_DIR}/rockettest-up-{up_idx}.png"));
            world.resource_mut::<RocketTestState>().up_idx += 1;
        }
        if alt >= 900.0 {
            release_all(world);
            world.resource_mut::<RocketTestState>().released = true;
            eprintln!("ha-rockettest: engine off at {alt:.0} m — falling back");
        }
    } else if active {
        if down_idx < ROCKET_DOWN_ALTS.len() && alt <= ROCKET_DOWN_ALTS[down_idx] {
            eprintln!(
                "ha-rockettest[down-{down_idx}] t={now:.1} alt={alt:.0} cover={cover:.2} mode={mode:?} region={region}"
            );
            shot(world, &format!("{FLYTEST_DIR}/rockettest-down-{down_idx}.png"));
            world.resource_mut::<RocketTestState>().down_idx += 1;
        }
    }

    if was_active && !active {
        match landed_at {
            None => {
                eprintln!(
                    "ha-rockettest: touchdown t={now:.1} region={region} (shots up={up_idx} down={down_idx})"
                );
                shot(world, &format!("{FLYTEST_DIR}/rockettest-landed.png"));
                world.resource_mut::<RocketTestState>().landed_at = Some(now);
            }
            Some(t0) if now - t0 > 1.5 => {
                eprintln!("ha-rockettest: DONE");
                std::process::exit(0);
            }
            _ => {}
        }
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
            // Hold thrust. The ascent is GRADUAL now (shared transition layer):
            // below the swap band the surface region stays live under the
            // closing cover, so the pass condition is "flying with a coherent
            // frame", not "instantly in Space".
            press(world, KeyCode::Space);
            if dt > 4.0 {
                let in_space = matches!(
                    world.resource::<crate::scene::RegionRuntime>().active,
                    crate::scene::RegionId::Space
                );
                let in_surface_frame = world
                    .resource::<crate::transition::FlightTransition>()
                    .in_surface_frame();
                let alt = world.resource::<crate::rocket_flight::RocketFlight>().altitude_m();
                let ok = in_space || in_surface_frame;
                eprintln!(
                    "ha-verify: space-flight {} (region_space={in_space}, surface_frame={in_surface_frame}, alt={alt:.0})",
                    if ok { "PASS" } else { "FAIL" }
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
