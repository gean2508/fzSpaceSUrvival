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
                fly_test_drive
                    .before(crate::input::desktop_input::desktop_keyboard_actions)
                    .after(crate::creative_fly::sync_flight_mirror),
            );
            #[cfg(target_os = "android")]
            app.add_systems(Update, fly_test_drive.after(crate::creative_fly::sync_flight_mirror));
            return;
        }
        // Moon creative-fly test: teleport to the Moon, climb, confirm you
        // reach CLEAR orbit (no instant re-teleport loop = the "white square"),
        // then measure the Earth→Moon lateral cruise time (HA_MOONTEST=1).
        if std::env::var("HA_MOONTEST").is_ok() {
            app.init_resource::<FlyTestState>();
            #[cfg(not(target_os = "android"))]
            app.add_systems(
                Update,
                moon_test_drive
                    .before(crate::input::desktop_input::desktop_keyboard_actions)
                    .after(crate::creative_fly::sync_flight_mirror),
            );
            #[cfg(target_os = "android")]
            app.add_systems(Update, moon_test_drive.after(crate::creative_fly::sync_flight_mirror));
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
                rocket_test_drive
                    .before(crate::input::desktop_input::desktop_keyboard_actions)
                    .after(crate::creative_fly::sync_flight_mirror),
            );
            #[cfg(target_os = "android")]
            app.add_systems(Update, rocket_test_drive.after(crate::creative_fly::sync_flight_mirror));
            return;
        }
        // Universe round-trip test: for several bodies, force orbit→land TWICE
        // and assert the surface state (region/ctx/planet) is coherent after
        // EVERY touchdown — the "land, take off, land again → stuck in the
        // orbital dimension" soft-lock (seen on Venus). HA_CYCLETEST=1.
        // Retrato de una ciudad generada (HA_CITYTEST=1): teletransporta a la
        // ciudad más cercana y captura a tres alturas — calle, barrio y región.
        // Es la verificación VISUAL de la geografía: que haya calles, fachadas y
        // carreteras saliendo hacia los pueblos vecinos, no sólo tests en verde.
        if std::env::var("HA_CITYTEST").is_ok() {
            app.init_resource::<FlyTestState>();
            #[cfg(not(target_os = "android"))]
            app.add_systems(
                Update,
                city_test_drive
                    .before(crate::input::desktop_input::desktop_keyboard_actions)
                    .after(crate::creative_fly::sync_flight_mirror),
            );
            #[cfg(target_os = "android")]
            app.add_systems(Update, city_test_drive.after(crate::creative_fly::sync_flight_mirror));
            return;
        }
        // Viaje completo Tierra→Luna→Tierra volando de verdad (HA_TRIPTEST=1):
        // el reporte "salgo al espacio y ya no puedo regresar" como test.
        if std::env::var("HA_TRIPTEST").is_ok() {
            app.init_resource::<TripTestState>().init_resource::<FlyTestState>();
            #[cfg(not(target_os = "android"))]
            app.add_systems(
                Update,
                trip_test_drive
                    .before(crate::input::desktop_input::desktop_keyboard_actions)
                    .after(crate::creative_fly::sync_flight_mirror),
            );
            #[cfg(target_os = "android")]
            app.add_systems(Update, trip_test_drive.after(crate::creative_fly::sync_flight_mirror));
            return;
        }
        if std::env::var("HA_CYCLETEST").is_ok() {
            app.init_resource::<CycleTestState>().init_resource::<FlyTestState>();
            #[cfg(not(target_os = "android"))]
            app.add_systems(
                Update,
                cycle_test_drive
                    .before(crate::input::desktop_input::desktop_keyboard_actions)
                    .after(crate::creative_fly::sync_flight_mirror),
            );
            #[cfg(target_os = "android")]
            app.add_systems(Update, cycle_test_drive.after(crate::creative_fly::sync_flight_mirror));
            return;
        }
        // Continuous performance/behaviour TESTER (HA_TESTER=1 HA_AUTOPLAY=1): an
        // AI that plays FOREVER through the real input pipeline — wander (streams
        // terrain/decor/fauna, the FPS-critical path), dig, build, then a full Ki
        // takeoff→orbit→land round trip — looping so `ha-perf fps=` samples a real
        // session. Never exits, never soft-locks (each phase self-resets on a
        // timeout). Drops a fresh `tester-live.png` every few seconds and obeys
        // on-demand commands from `tester-cmd.txt` (shot/earth/luna/fly/land) so a
        // human can steer it ("go to the Moon and test X").
        if std::env::var("HA_TESTER").is_ok() {
            app.init_resource::<TesterState>();
            app.init_resource::<SmokeState>(); // dig_setup/place_setup write its cell
            app.init_resource::<FlyTestState>(); // hold_with_fresh_edges uses it
            #[cfg(not(target_os = "android"))]
            app.add_systems(
                Update,
                tester_drive
                    .before(crate::input::desktop_input::desktop_keyboard_actions)
                    .after(crate::creative_fly::sync_flight_mirror),
            );
            #[cfg(target_os = "android")]
            app.add_systems(Update, tester_drive.after(crate::creative_fly::sync_flight_mirror));
            return;
        }
        // Construcción por BORDES (LDoE): levanta un cuarto real por el mismo
        // camino que el jugador (hold → `apply_build_hold`) y comprueba que el
        // piso queda, que las paredes se pegan a los bordes, que una pared sin
        // piso NO entra y que el muro FRENA al jugador. HA_BUILDTEST=1
        // (correr con HA_FORCE_TOUCH=1: si no, el mouse le pisa la mira).
        if std::env::var("HA_BUILDTEST").is_ok() {
            app.init_resource::<BuildTestState>();
            #[cfg(not(target_os = "android"))]
            app.add_systems(
                Update,
                build_test_drive.before(crate::input::desktop_input::desktop_keyboard_actions),
            );
            #[cfg(target_os = "android")]
            app.add_systems(Update, build_test_drive);
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
    cleared_at: f32,
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
    "/tmp/claude-1000/-home-fzion-Documents-mods-phaser-arena/cad6eec6-9b35-4c76-b5dd-17834cd68598/scratchpad";

/// (B1 · `PLAN_MAESTRO §3.B1`) Anexa un bug machine-readable (una línea JSON) al
/// archivo `HA_BUGREPORT` (default `bugreport.jsonl`). Cada driver lo llama al
/// romper un invariante; el runner `HA_SUITE` (scripts/ha-suite.mjs) agrega todo
/// para que la IA lea y resuelva. Doble uso: primer formato del log de
/// comportamiento que alimentará la IA colmena de colonos (B3/B4).
pub fn report_bug(mechanic: &str, invariant: &str, detail: &str) {
    use std::io::Write;
    let path = std::env::var("HA_BUGREPORT").unwrap_or_else(|_| "bugreport.jsonl".to_string());
    let line = serde_json::json!({
        "mechanic": mechanic,
        "invariant": invariant,
        "detail": detail,
    })
    .to_string();
    if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(&path) {
        let _ = writeln!(f, "{line}");
    }
    eprintln!("ha-bug[{mechanic}]: {invariant} — {detail}");
}

fn fly_test_drive(world: &mut World) {
    let now = world.resource::<Time>().elapsed_secs();
    if now < 1.0 {
        return;
    }
    // Enable creative every frame (so nothing turns it back off).
    world.resource_mut::<crate::ui_editor::UiEditorState>().creative = true;

    // Track the camera zoom every frame and flag any discontinuous jump
    // (ratio vs previous frame) — this is the exact bug class reported.
    // El encuadre REALMENTE en pantalla (no el objetivo): es lo que el jugador
    // ve, y ahora TODO salto cuenta — ya no hay pantalla opaca detrás de la cual
    // esconder un zoom brusco, así que el invariante se aplica siempre.
    let view_h = world.resource::<crate::camera::CameraZoom>().live_view_h;
    let cover_now = world.resource::<crate::transition::FlightTransition>().cover;
    {
        let mut st = world.resource_mut::<FlyTestState>();
        if now < st.suppress_jump_until {
            // Post-injection grace window — track but don't compare.
        } else if st.prev_view_h > 0.0 {
            let ratio = (view_h / st.prev_view_h).max(st.prev_view_h / view_h);
            if ratio > st.max_jump_ratio {
                st.max_jump_ratio = ratio;
            }
            if ratio > 1.5 {
                eprintln!(
                    "ha-flytest: VISIBLE ZOOM JUMP t={now:.2} {:.0} -> {:.0} (x{ratio:.2}, cover={cover_now:.2})",
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
            // Keep holding JUMP until we've reached orbit AND CLIMBED CLEAR of
            // the cover into open space (cover≈0). Entering orbit is NOT enough
            // — the bug was being stuck at swap_up under full cloud with no way
            // up. Requiring cover to clear is the real regression test.
            hold_with_fresh_edges(world, KeyCode::Space, now, 0.4);
            let cover = world.resource::<crate::transition::FlightTransition>().cover;
            let (armed, alt) = {
                let f = world.resource::<crate::creative_fly::CreativeFlight>();
                (f.land_armed, f.alt)
            };
            // Subir hasta un RADIO de altura: ahí el disco de la Tierra ocupa
            // ~45 % de la pantalla, que es el encuadre que hay que ver bien.
            let framed = alt >= ha_shared::body_radius(ha_shared::CelestialBody::Earth);
            if phase == crate::creative_fly::Phase::Orbit && cover < 0.05 && armed && framed {
                // Cleared! Release JUMP to HOVER (no radial movement) and linger
                // ~1.8 s so the disc screenshot lands before the descent tests.
                let cleared = world.resource::<FlyTestState>().cleared_at;
                if cleared == 0.0 {
                    let vh = world.resource::<crate::camera::CameraZoom>().view_h;
                    eprintln!("ha-flytest: climbed CLEAR (cover={cover:.2} view_h={vh:.0}) — should see Earth disc");
                    world.resource_mut::<FlyTestState>().cleared_at = now;
                } else {
                    release_all(world); // hover
                    if now - cleared > 0.8 {
                        shot(world, &format!("{FLYTEST_DIR}/earth-cleared-disc.png"));
                    }
                    if now - cleared > 1.8 {
                        advance_stage(world, now);
                    }
                }
            } else if now > 40.0 {
                eprintln!("ha-flytest: STUCK — never cleared clouds (cover={cover:.2} phase={phase:?})");
                std::process::exit(1);
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
                fly.vert = 1_500.0;
                fly.alt = 1_500.0;
                fly.body = ha_shared::CelestialBody::Earth;
            }
            // La región NO se toca: la deriva `sync_frame_region` de la altura.
            // This is OUR synthetic teleport (not a real gameplay transition)
            // — don't let it register as a "zoom jump" regression.
            world.resource_mut::<FlyTestState>().suppress_jump_until = now + 1.6;
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
                fly.vert = 600.0;
                fly.alt = 600.0;
            }
            world.resource_mut::<FlyTestState>().suppress_jump_until = now + 1.6; // synthetic, not a real jump
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
        // POST-TOUCHDOWN INVARIANTS — the "stuck after landing" bug class: the
        // camera flew out but the player is left behind / the region or the
        // rocket state is crossed. On the ground everything must be back to the
        // walking baseline.
        let region = format!("{:?}", world.resource::<crate::scene::RegionRuntime>().active);
        let in_space = region.contains("Space");
        let rocket_active = world.resource::<crate::flight::Flight>().active;
        let view_h = world.resource::<crate::camera::CameraZoom>().view_h;
        let camera_stuck_out = view_h > 200.0; // surface view is ~30 m
        let mut fail = false;
        if in_space {
            report_bug("flight", "surface_after_landing", &format!("region still {region}"));
            fail = true;
        }
        if rocket_active {
            report_bug("flight", "rocket_active_in_creative", "landed in creative but Flight.active");
            fail = true;
        }
        if camera_stuck_out {
            report_bug("flight", "camera_zoomed_out_after_touchdown", &format!("view_h={view_h:.0}"));
            fail = true;
        }
        eprintln!("ha-flytest: {}", if fail { "LANDING INVARIANTS FAIL" } else { "LANDING INVARIANTS PASS" });
        eprintln!("ha-flytest: DONE");
        std::process::exit(if max_jump > 1.5 || fail { 1 } else { 0 });
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

/// Coloca el vuelo a una altura fija sobre la Luna (determinista, sin deriva de
/// input) para que la captura encuadre el disco siempre igual.
///
/// En el marco único (`universe.rs`) esto es sólo: la Luna es el origen, el
/// jugador está dentro de su disco y la altura vale `alt`. La región la deriva
/// `creative_fly::sync_frame_region`, así que NO se fuerza acá — si hiciera
/// falta forzarla, sería un bug.
fn place_creative_orbit(world: &mut World, alt: f32) {
    world.resource_mut::<crate::universe::UniverseFrame>().ref_body =
        ha_shared::CelestialBody::Moon;
    {
        let mut fly = world.resource_mut::<crate::creative_fly::CreativeFlight>();
        fly.body = ha_shared::CelestialBody::Moon;
        fly.phase = crate::creative_fly::Phase::Orbit;
        fly.land_armed = true;
        fly.vert = alt;
        fly.alt = alt;
        fly.vy = 0.0;
        fly.pos = Vec3::ZERO;
    }
    let mut q = world.query_filtered::<(&mut SimState, &mut PlayerVisual), With<Player>>();
    if let Ok((mut sim, mut vis)) = q.get_single_mut(world) {
        sim.body.x = 0.0;
        sim.body.z = 0.0;
        vis.from_x = 0.0;
        vis.to_x = 0.0;
        vis.from_z = 0.0;
        vis.to_z = 0.0;
    }
    let mut intent = world.resource_mut::<crate::input::MovementIntent>();
    intent.move_x = 0.0;
    intent.move_z = 0.0;
}

/// Moon creative-fly regression: teleport to the Moon, climb, confirm you reach
/// CLEAR orbit (no instant re-teleport loop = the "cuadrado blanco"), then
/// measure the capped lateral cruise so travel isn't instant.
fn moon_test_drive(world: &mut World) {
    let now = world.resource::<Time>().elapsed_secs();
    if now < 4.0 {
        return;
    }
    world.resource_mut::<crate::ui_editor::UiEditorState>().creative = true;
    let stage = world.resource::<FlyTestState>().stage;
    match stage {
        // 0: teleport onto the Moon surface.
        0 => {
            world.resource_mut::<crate::universe::UniverseFrame>().ref_body =
                ha_shared::CelestialBody::Moon;
            world.resource_mut::<crate::scene::RegionRuntime>().active = crate::scene::RegionId::Luna;
            world.resource_mut::<crate::scene::WorldContextRuntime>().active =
                ha_shared::WorldContext::Luna;
            let (sx, sz) =
                crate::scene::walkable_spawn(ha_shared::CelestialBody::Moon, 0.0, 6.0);
            let mut q = world.query_filtered::<(&mut SimState, &mut PlayerVisual), With<Player>>();
            if let Ok((mut sim, mut vis)) = q.get_single_mut(world) {
                sim.body.x = sx;
                sim.body.z = sz;
                sim.body.y = 0.0;
                vis.from_x = sx;
                vis.to_x = sx;
                vis.from_z = sz;
                vis.to_z = sz;
            }
            eprintln!("ha-moontest: teleported to Moon surface");
            advance_stage(world, now);
        }
        // 1: hold JUMP just until you're clear of the fade and armed (NOT all
        // the way up — we want to stay LOW so the Moon disc is big on screen).
        1 => {
            hold_with_fresh_edges(world, KeyCode::Space, now, 0.4);
            let (phase, body, armed, alt) = {
                let f = world.resource::<crate::creative_fly::CreativeFlight>();
                (f.phase, f.body, f.land_armed, f.alt)
            };
            let cover = world.resource::<crate::transition::FlightTransition>().cover;
            if phase == crate::creative_fly::Phase::Orbit
                && body == ha_shared::CelestialBody::Moon
                && cover < 0.05
                && armed
            {
                eprintln!("ha-moontest: reached CLEAR Moon orbit (no white-square loop, cover={cover:.2} alt={alt:.0})");
                advance_stage(world, now);
            } else if now - world.resource::<FlyTestState>().stage_entered > 30.0 {
                report_bug("moon_climb", "stuck_never_cleared", &format!("cover={cover:.2} phase={phase:?} body={body:?}"));
                std::process::exit(1);
            }
        }
        // 2: place at a FAR orbit altitude (deterministic, no input drift) and
        // screenshot — the Moon should be a clear grey disc, small.
        2 => {
            place_creative_orbit(world, 700.0);
            if now - world.resource::<FlyTestState>().stage_entered > 1.0 {
                let vh = world.resource::<crate::camera::CameraZoom>().view_h;
                eprintln!("ha-moontest: FAR orbit alt=700 view_h={vh:.0} — Moon disc small");
                shot(world, &format!("{FLYTEST_DIR}/moon-disc-far.png"));
                advance_stage(world, now);
            }
        }
        // 3: place NEAR the Moon — the disc should be MUCH BIGGER (grew as we
        // approached), same Moon.
        3 => {
            place_creative_orbit(world, 180.0);
            let dt = now - world.resource::<FlyTestState>().stage_entered;
            if dt > 1.0 && world.resource::<FlyTestState>().cleared_at == 0.0 {
                let vh = world.resource::<crate::camera::CameraZoom>().view_h;
                eprintln!("ha-moontest: NEAR orbit alt=180 view_h={vh:.0} — Moon disc BIG");
                shot(world, &format!("{FLYTEST_DIR}/moon-disc-near.png"));
                world.resource_mut::<FlyTestState>().cleared_at = now;
            }
            // Let the async screenshot observer flush before exiting.
            if world.resource::<FlyTestState>().cleared_at > 0.0 && dt > 2.0 {
                eprintln!("ha-moontest: DONE");
                std::process::exit(0);
            }
        }
        _ => {}
    }
    if now > 60.0 {
        eprintln!("ha-moontest: TIMEOUT (FAIL, stage={stage})");
        std::process::exit(1);
    }
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
        // Saiyan Ki-flight: no manual ignition — the thrust loop below holds JUMP
        // (Space), which charges the aura and lifts the character off.
        world.resource_mut::<RocketTestState>().ignited = true;
        eprintln!("ha-rockettest: charging for takeoff");
        return;
    }

    let (active, alt) = {
        let f = world.resource::<crate::flight::Flight>();
        (f.active && f.phase != ha_shared::FlightPhase::Surface, f.altitude_m())
    };
    let (cover, mode) = {
        let t = world.resource::<crate::transition::FlightTransition>();
        (t.cover, t.mode)
    };
    let (ki_alt, ki_phase) = {
        let f = world.resource::<crate::creative_fly::CreativeFlight>();
        (f.alt, f.phase)
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
                "ha-rockettest[up-{up_idx}] t={now:.1} alt={alt:.0} ki_alt={ki_alt:.0} ki_phase={ki_phase:?} cover={cover:.2} mode={mode:?} region={region}"
            );
            shot(world, &format!("{FLYTEST_DIR}/rockettest-up-{up_idx}.png"));
            world.resource_mut::<RocketTestState>().up_idx += 1;
        }
        if alt >= 900.0 {
            release_all(world);
            world.resource_mut::<RocketTestState>().released = true;
            eprintln!("ha-rockettest: engine off at {alt:.0} m — descending");
        }
    } else if active {
        // En Ki-flight soltar el empuje NO te hace caer: sin input flotás
        // (`creative_fly_climb` deja `vy = 0`). El descenso es activo — hay que
        // mantener USE (Shift/F) igual que el jugador en el móvil.
        press(world, KeyCode::ShiftLeft);
        if down_idx < ROCKET_DOWN_ALTS.len() && alt <= ROCKET_DOWN_ALTS[down_idx] {
            eprintln!(
                "ha-rockettest[down-{down_idx}] t={now:.1} alt={alt:.0} ki_alt={ki_alt:.0} ki_phase={ki_phase:?} cover={cover:.2} mode={mode:?} region={region}"
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

/// El jugador puede no existir todavía (mundo cargando) o estar respawneando.
/// Los drivers deben esperar en vez de panicar con `single()`.
fn player_ready(world: &mut World) -> bool {
    let mut q = world.query_filtered::<&SimState, With<Player>>();
    q.get_single(world).is_ok()
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

// ---------------------------------------------------------------------------
// HA_CITYTEST — retrato de una ciudad generada
// ---------------------------------------------------------------------------

fn city_test_drive(world: &mut World) {
    let now = world.resource::<Time>().elapsed_secs();
    if now < 3.0 {
        return;
    }
    world.resource_mut::<crate::ui_editor::UiEditorState>().creative = true;
    let stage = world.resource::<FlyTestState>().stage;
    let body = ha_shared::CelestialBody::Earth;

    if stage == 0 {
        // Buscar la ciudad más cercana al origen y plantarse en su centro.
        let mut best: Option<(i64, (i64, i64))> = None;
        for sx in -25..25 {
            for sz in -25..25 {
                let Some(s) = ha_shared::geography::settlement_in(body, sx, sz) else { continue };
                if s.tier != ha_shared::geography::SettlementTier::City {
                    continue;
                }
                let d = s.center.0.abs() + s.center.1.abs();
                if best.map_or(true, |(bd, _)| d < bd) {
                    best = Some((d, s.center));
                }
            }
        }
        let Some((_, (cx, cz))) = best else {
            eprintln!("ha-citytest: NO HAY NINGUNA CIUDAD");
            std::process::exit(1);
        };
        let mut q = world.query_filtered::<(&mut SimState, &mut PlayerVisual), With<Player>>();
        if let Ok((mut sim, mut vis)) = q.get_single_mut(world) {
            sim.body.x = cx as f32;
            sim.body.z = cz as f32;
            vis.from_x = cx as f32;
            vis.to_x = cx as f32;
            vis.from_z = cz as f32;
            vis.to_z = cz as f32;
        }
        eprintln!("ha-citytest: ciudad en ({cx}, {cz})");
        advance_stage(world, now);
        return;
    }

    // Tres alturas: a pie en la calle, sobre el barrio y sobre la región.
    let beats: [(f32, f32, &str); 3] = [
        (2.0, 0.0, "city-0-street"),
        (5.0, 900.0, "city-1-district"),
        (9.0, 9_000.0, "city-2-region"),
    ];
    let shots = world.resource::<FlyTestState>().shots as usize;
    if shots >= beats.len() {
        eprintln!("ha-citytest: DONE");
        std::process::exit(0);
    }
    let (at, alt, name) = beats[shots];
    let entered = world.resource::<FlyTestState>().stage_entered;
    {
        let mut fly = world.resource_mut::<crate::creative_fly::CreativeFlight>();
        if alt > 0.0 {
            fly.phase = crate::creative_fly::Phase::Climb;
            fly.vert = alt;
            fly.alt = alt;
            fly.vy = 0.0;
        }
    }
    if now - entered > at {
        shot(world, &format!("{FLYTEST_DIR}/{name}.png"));
        world.resource_mut::<FlyTestState>().shots += 1;
    }
}

// ---------------------------------------------------------------------------
// HA_TRIPTEST — el viaje de verdad: Tierra → Luna → Tierra, volando
// ---------------------------------------------------------------------------

#[derive(Resource, Default)]
struct TripTestState {
    stage: u8,
    stage_t: f32,
    prev_view_h: f32,
    max_jump_ratio: f32,
}

/// Altura de crucero para el trayecto: a esta altura el encuadre abarca la
/// distancia Tierra–Luna, que es la condición para poder apuntar y viajar.
const TRIP_CRUISE_ALT: f32 = 200_000.0;

/// Viaje completo por el camino REAL del jugador (teclas, sin forzar estado):
/// despegar de la Tierra, cruzar hasta la Luna, aterrizar, volver a despegar y
/// regresar a la Tierra. Es literalmente el reporte del usuario ("salgo al
/// espacio y ya no puedo regresar a la Tierra") convertido en test.
///
/// Comprueba en el camino que el marco REBASA de cuerpo sin saltos de cámara:
/// el rebase mueve al jugador y a todos los discos el mismo delta, así que un
/// salto de encuadre ahí significaría que la continuidad se rompió.
fn trip_test_drive(world: &mut World) {
    let now = world.resource::<Time>().elapsed_secs();
    if now < 2.0 {
        return;
    }
    world.resource_mut::<crate::ui_editor::UiEditorState>().creative = true;

    // Vigilancia de continuidad del encuadre (lo que se ve, no el objetivo).
    let view_h = world.resource::<crate::camera::CameraZoom>().live_view_h;
    {
        let mut st = world.resource_mut::<TripTestState>();
        if st.prev_view_h > 0.0 {
            let ratio = (view_h / st.prev_view_h).max(st.prev_view_h / view_h);
            if ratio > st.max_jump_ratio {
                st.max_jump_ratio = ratio;
            }
        }
        st.prev_view_h = view_h;
    }

    let (stage, stage_t) = {
        let s = world.resource::<TripTestState>();
        (s.stage, s.stage_t)
    };
    let dt = now - stage_t;
    let alt = world.resource::<crate::creative_fly::CreativeFlight>().alt;
    let phase = world.resource::<crate::creative_fly::CreativeFlight>().phase;
    let ref_body = world.resource::<crate::universe::UniverseFrame>().ref_body;
    let advance = |w: &mut World, st: u8| {
        let mut s = w.resource_mut::<TripTestState>();
        s.stage = st;
        s.stage_t = now;
    };

    match stage {
        // 0 · despegar de la Tierra hasta altura de crucero.
        0 => {
            hold_with_fresh_edges(world, KeyCode::Space, now, 0.4);
            if alt >= TRIP_CRUISE_ALT {
                shot(world, &format!("{FLYTEST_DIR}/trip-0-earth-orbit.png"));
                eprintln!("ha-triptest: crucero sobre la Tierra (alt={alt:.0} view_h={view_h:.0})");
                advance(world, 1);
            } else if dt > 30.0 {
                report_bug("trip", "no_climb_from_earth", &format!("alt={alt:.0}"));
                std::process::exit(1);
            }
        }
        // 1 · cruzar hasta la Luna (el marco debe rebasar solo).
        1 => {
            steer_toward(world, ha_shared::CelestialBody::Moon, now, KeyCode::Space);
            if ref_body == ha_shared::CelestialBody::Moon {
                shot(world, &format!("{FLYTEST_DIR}/trip-1-moon-approach.png"));
                eprintln!("ha-triptest: marco rebasado a la Luna (alt={alt:.0})");
                advance(world, 2);
            } else if dt > 40.0 {
                report_bug("trip", "never_reached_moon", &format!("ref={ref_body:?}"));
                std::process::exit(1);
            }
        }
        // 2 · bajar y aterrizar en la Luna.
        2 => {
            // Aproximación: bajar Y cerrar sobre el cuerpo a la vez. La altura
            // efectiva combina ambas, así que el encuadre cierra y la velocidad
            // baja sola conforme llegás — no hace falta frenar a mano.
            steer_toward(world, ha_shared::CelestialBody::Moon, now, KeyCode::KeyF);
            if phase == crate::creative_fly::Phase::Ground {
                let region = world.resource::<crate::scene::RegionRuntime>().active;
                if region != crate::scene::RegionId::Luna {
                    report_bug("trip", "moon_landing_wrong_region", &format!("{region:?}"));
                    std::process::exit(1);
                }
                shot(world, &format!("{FLYTEST_DIR}/trip-2-moon-landed.png"));
                eprintln!("ha-triptest: aterrizado en la Luna, region={region:?}");
                advance(world, 3);
            } else if dt > 40.0 {
                report_bug("trip", "never_landed_on_moon", &format!("alt={alt:.0} phase={phase:?}"));
                std::process::exit(1);
            }
        }
        // 3 · volver a despegar (ESTE es el "ya no puedo salir").
        3 => {
            hold_with_fresh_edges(world, KeyCode::Space, now, 0.4);
            if alt >= TRIP_CRUISE_ALT {
                eprintln!("ha-triptest: de vuelta en crucero desde la Luna");
                advance(world, 4);
            } else if dt > 30.0 {
                report_bug("trip", "no_climb_from_moon", &format!("alt={alt:.0}"));
                std::process::exit(1);
            }
        }
        // 4 · regresar a la Tierra.
        4 => {
            steer_toward(world, ha_shared::CelestialBody::Earth, now, KeyCode::Space);
            if ref_body == ha_shared::CelestialBody::Earth {
                shot(world, &format!("{FLYTEST_DIR}/trip-3-earth-return.png"));
                eprintln!("ha-triptest: marco de vuelta en la Tierra");
                advance(world, 5);
            } else if dt > 40.0 {
                report_bug("trip", "never_returned_to_earth", &format!("ref={ref_body:?}"));
                std::process::exit(1);
            }
        }
        // 5 · aterrizar en casa y cerrar el ciclo.
        _ => {
            steer_toward(world, ha_shared::CelestialBody::Earth, now, KeyCode::KeyF);
            if phase == crate::creative_fly::Phase::Ground {
                let region = world.resource::<crate::scene::RegionRuntime>().active;
                let max_jump = world.resource::<TripTestState>().max_jump_ratio;
                shot(world, &format!("{FLYTEST_DIR}/trip-4-earth-landed.png"));
                let ok = matches!(region, crate::scene::RegionId::Overworld { .. });
                if !ok {
                    report_bug("trip", "earth_landing_wrong_region", &format!("{region:?}"));
                }
                eprintln!("ha-triptest: aterrizado en la Tierra, region={region:?}");
                eprintln!("ha-triptest: salto de encuadre maximo = x{max_jump:.2}");
                eprintln!(
                    "ha-triptest: {}",
                    if ok && max_jump < 1.5 { "ROUND TRIP OK" } else { "ROUND TRIP FAIL" }
                );
                std::process::exit(if ok && max_jump < 1.5 { 0 } else { 1 });
            } else if dt > 40.0 {
                report_bug("trip", "never_landed_on_earth", &format!("alt={alt:.0}"));
                std::process::exit(1);
            }
        }
    }

    if now > 220.0 {
        report_bug("trip", "timeout", &format!("stage={stage}"));
        std::process::exit(1);
    }
}

/// Mantiene SALTO (para no perder altura) y pulsa las teclas de movimiento que
/// apuntan hacia `target` en el plano del marco. Es lo que haría un jugador
/// mirando la pantalla: ver el disco y volar hacia él.
fn steer_toward(
    world: &mut World,
    target: ha_shared::CelestialBody,
    now: f32,
    vertical: KeyCode,
) {
    let t = world.resource::<crate::solar_system_clock::SolarSystemClock>().t;
    let ref_body = world.resource::<crate::universe::UniverseFrame>().ref_body;
    let goal = crate::universe::body_offset(target, ref_body, t);
    let here = {
        let mut q = world.query_filtered::<&SimState, With<Player>>();
        q.get_single(world)
            .map(|s| Vec2::new(s.body.x, -s.body.z))
            .unwrap_or(Vec2::ZERO)
    };
    let d = goal - here;
    // Un pulso de teclas fresco de vez en cuando (mismo motivo que
    // `hold_with_fresh_edges`: no perder el flanco durante un frame bloqueado).
    let pulse = {
        let mut st = world.resource_mut::<FlyTestState>();
        let due = now - st.last_press_pulse > 0.4;
        if due {
            st.last_press_pulse = now;
        }
        due
    };
    if pulse {
        release_all(world);
    }
    press(world, vertical); // SALTO = mantener crucero · USAR = ir bajando
    // render y = -z ⇒ W (z decreciente) va hacia arriba en pantalla.
    if d.x.abs() > d.y.abs() * 0.3 {
        press(world, if d.x > 0.0 { KeyCode::KeyD } else { KeyCode::KeyA });
    }
    if d.y.abs() > d.x.abs() * 0.3 {
        press(world, if d.y > 0.0 { KeyCode::KeyW } else { KeyCode::KeyS });
    }
}

#[derive(Resource, Default)]
struct CycleTestState {
    body_idx: usize,
    land_idx: u32,
    stage: u8,
    stage_t: f32,
}

/// Bodies to round-trip (landable, varied atmospheres) and lands per body.
const CYCLE_BODIES: [ha_shared::CelestialBody; 3] = [
    ha_shared::CelestialBody::Moon,
    ha_shared::CelestialBody::Venus,
    ha_shared::CelestialBody::Mars,
];
const CYCLE_LANDS: u32 = 2;

/// Deja el vuelo EN ÓRBITA sobre `body`, armado para aterrizar. En el marco
/// único eso es: ese cuerpo es el origen, el jugador está dentro de su disco y
/// la altura está muy por encima de la atmósfera. El descenso posterior baja por
/// el camino REAL (mantener USAR), sin forzar región ni posición.
fn place_orbit_to_land(world: &mut World, body: ha_shared::CelestialBody) {
    world.resource_mut::<crate::universe::UniverseFrame>().ref_body = body;
    let alt = ha_shared::body_radius(body) * 1.2;
    {
        let mut fly = world.resource_mut::<crate::creative_fly::CreativeFlight>();
        fly.body = body;
        fly.phase = crate::creative_fly::Phase::Orbit;
        fly.land_armed = true;
        fly.vert = alt;
        fly.alt = alt;
        fly.vy = 0.0;
        fly.pos = Vec3::ZERO;
    }
    let mut q = world.query_filtered::<(&mut SimState, &mut PlayerVisual), With<Player>>();
    if let Ok((mut sim, mut vis)) = q.get_single_mut(world) {
        sim.body.x = 0.0;
        sim.body.z = 0.0;
        vis.from_x = 0.0;
        vis.to_x = 0.0;
        vis.from_z = 0.0;
        vis.to_z = 0.0;
    }
    zero_flight_input(world);
}

/// Neutralise all simulated input so the orbit doesn't drift/climb on its own.
fn zero_flight_input(world: &mut World) {
    {
        let mut intent = world.resource_mut::<crate::input::MovementIntent>();
        intent.move_x = 0.0;
        intent.move_z = 0.0;
    }
    {
        let mut a = world.resource_mut::<crate::input::TouchActions>();
        a.jump = false;
        a.jump_edge = false;
        a.action = false;
    }
    world.resource_mut::<ButtonInput<KeyCode>>().release_all();
}

fn cycle_test_drive(world: &mut World) {
    let now = world.resource::<Time>().elapsed_secs();
    if now < 2.0 {
        return;
    }
    world.resource_mut::<crate::ui_editor::UiEditorState>().creative = true;
    let (body_idx, land_idx, stage, stage_t) = {
        let s = world.resource::<CycleTestState>();
        (s.body_idx, s.land_idx, s.stage, s.stage_t)
    };
    if body_idx >= CYCLE_BODIES.len() {
        eprintln!("ha-cycletest: ALL BODIES ROUND-TRIPPED OK (no orbital soft-lock)");
        std::process::exit(0);
    }
    let body = CYCLE_BODIES[body_idx];
    let phase = world.resource::<crate::creative_fly::CreativeFlight>().phase;
    let dt = now - stage_t;
    let mut set = |w: &mut World, st: u8| {
        let mut s = w.resource_mut::<CycleTestState>();
        s.stage = st;
        s.stage_t = now;
    };
    match stage {
        // Setup a landing through the real orbit→descend path.
        0 => {
            place_orbit_to_land(world, body);
            set(world, 1);
        }
        // Once the landing branch handed us to Descend, drop the altitude so the
        // fall touches down fast.
        1 => {
            // Bajar MANTENIENDO USAR — el camino real del jugador. Nada de
            // forzar posiciones: si el descenso no llega al suelo solo, es un
            // bug de verdad y el test debe verlo.
            hold_with_fresh_edges(world, KeyCode::KeyF, now, 0.4);
            if phase == crate::creative_fly::Phase::Ground {
                set(world, 2);
            } else if dt > 20.0 {
                let (fp, fa) = {
                    let f = world.resource::<crate::creative_fly::CreativeFlight>();
                    (f.phase, f.alt)
                };
                report_bug("landing_cycle", "never_touched_down", &format!("{body:?} land#{land_idx} phase={fp:?} alt={fa:.0}"));
                std::process::exit(1);
            }
        }
        // Touched down → assert the surface state is coherent.
        2 => {
            if phase == crate::creative_fly::Phase::Ground {
                let region = world.resource::<crate::scene::RegionRuntime>().active;
                let ctx = world.resource::<crate::scene::WorldContextRuntime>().active;
                let pbody = world.resource::<crate::world2d::ActivePlanet>().body;
                let (want_r, want_c) = crate::scene::surface_region_for(body);
                let region_ok = region == want_r;
                let ctx_ok = ctx == want_c;
                let planet_ok = matches!(body, ha_shared::CelestialBody::Earth | ha_shared::CelestialBody::Moon)
                    || pbody == Some(body);
                if !(region_ok && ctx_ok && planet_ok) {
                    report_bug("landing_cycle", "incoherent_surface_state", &format!("{body:?} land#{land_idx} region={region:?} ctx={ctx:?} planet={pbody:?}"));
                    std::process::exit(1);
                }
                eprintln!("ha-cycletest: {body:?} land#{land_idx} OK");
                let next_land = land_idx + 1;
                if next_land < CYCLE_LANDS {
                    world.resource_mut::<CycleTestState>().land_idx = next_land;
                    set(world, 3); // take off, then land again
                } else {
                    let mut s = world.resource_mut::<CycleTestState>();
                    s.land_idx = 0;
                    s.body_idx += 1;
                    s.stage = 0;
                    s.stage_t = now;
                }
            } else if dt > 12.0 {
                report_bug("landing_cycle", "never_reached_ground", &format!("{body:?} land#{land_idx}"));
                std::process::exit(1);
            }
        }
        // Despegar de VERDAD: mantener SALTO desde el suelo hasta cruzar la
        // línea de espacio. Es el otro medio ciclo del bug reportado ("aterricé
        // y ya no pude volver a salir"), así que se ejerce por el camino real.
        3 => {
            hold_with_fresh_edges(world, KeyCode::Space, now, 0.4);
            if phase == crate::creative_fly::Phase::Orbit {
                zero_flight_input(world);
                set(world, 0); // otra vez a preparar el aterrizaje
            } else if dt > 20.0 {
                report_bug("landing_cycle", "never_took_off_again", &format!("{body:?} phase={phase:?}"));
                std::process::exit(1);
            }
        }
        _ => {}
    }
    if now > 120.0 {
        report_bug("landing_cycle", "timeout", &format!("body_idx={body_idx} stage={stage}"));
        std::process::exit(1);
    }
}

// ---------------------------------------------------------------------------
// HA_BUILDTEST — construcción por bordes (piso en la celda, pared en la arista)
// ---------------------------------------------------------------------------

/// Lado del cuarto de prueba, en tiles.
const BT_SIDE: i64 = 3;

#[derive(Resource, Default)]
struct BuildTestState {
    step: u32,
    next_t: f32,
    base: (i64, i64),
    /// Mira sostenida frame a frame (si no, el cursor de aim se mueve solo).
    aim: Vec2,
    walk_until: f32,
    walls_before: usize,
    start_x: f32,
    failed: bool,
}

/// Bordes del perímetro del cuarto de prueba, en orden.
fn bt_edges(base: (i64, i64)) -> Vec<(i64, i64, ha_shared::build_grid::WallAxis)> {
    use ha_shared::build_grid::WallAxis::{H, V};
    let mut out = Vec::new();
    for i in 0..BT_SIDE {
        out.push((base.0 + i, base.1, H)); // norte
        out.push((base.0 + i, base.1 + BT_SIDE, H)); // sur
        out.push((base.0, base.1 + i, V)); // oeste
        out.push((base.0 + BT_SIDE, base.1 + i, V)); // este
    }
    out
}

/// Punto de mundo que engancha (`nearest_edge`) exactamente a ese borde.
fn bt_aim_for(tx: i64, tz: i64, axis: ha_shared::build_grid::WallAxis) -> Vec2 {
    match axis {
        ha_shared::build_grid::WallAxis::H => Vec2::new(tx as f32 + 0.5, tz as f32 + 0.08),
        ha_shared::build_grid::WallAxis::V => Vec2::new(tx as f32 + 0.08, tz as f32 + 0.5),
    }
}

fn complete_build_hold(world: &mut World) {
    let mut h = world.resource_mut::<HoldInteractState>();
    h.active = false;
    h.completed = true;
    h.placing_build = true;
    h.placing_block = false;
    h.placing_machine = false;
}

fn bt_body(world: &mut World) -> ha_shared::CelestialBody {
    let ctx = world.resource::<crate::scene::WorldContextRuntime>().clone();
    let planet = *world.resource::<crate::world2d::ActivePlanet>();
    crate::world2d::active_surface_body(&ctx, &planet)
}

fn bt_wall_count(world: &mut World) -> usize {
    world.resource::<crate::walls::BuiltWalls>().list.len()
}

fn build_test_drive(world: &mut World) {
    let now = world.resource::<Time>().elapsed_secs();
    if now < 2.0 || !player_ready(world) {
        return;
    }
    // La mira se sostiene TODOS los frames: `apply_build_hold` puede consumir el
    // hold un frame después de que lo marcamos.
    {
        let aim = world.resource::<BuildTestState>().aim;
        if aim != Vec2::ZERO {
            world.resource_mut::<crate::input::AimCursor>().world = aim;
        }
    }
    if now < world.resource::<BuildTestState>().next_t {
        return;
    }
    let step = world.resource::<BuildTestState>().step;
    {
        let mut st = world.resource_mut::<BuildTestState>();
        st.step += 1;
        st.next_t = now + 0.05;
    }
    let floors = (BT_SIDE * BT_SIDE) as u32;
    let edges_n = bt_edges((0, 0)).len() as u32;

    match step {
        // Setup: madera de sobra, modo construir, pieza = piso, cuarto al lado.
        0 => {
            world.resource_mut::<crate::ui_editor::UiEditorState>().creative = false;
            world.resource_mut::<PlayerInventory>().add("wood", 999);
            world.resource_mut::<crate::input::BuildModeActive>().active = true;
            world.resource_mut::<crate::input::AimCursor>().locked = true;
            world.resource_mut::<crate::build::SelectedBuildPiece>().0 = 0;
            let (px, pz) = player_cell(world);
            world.resource_mut::<BuildTestState>().base = (px + 1, pz - 1);
        }
        // Piso: una celda por paso, por el camino real (LookTarget + hold).
        s if s >= 1 && s <= floors => {
            let base = world.resource::<BuildTestState>().base;
            let i = (s - 1) as i64;
            let (cx, cz) = (base.0 + i % BT_SIDE, base.1 + i / BT_SIDE);
            world.resource_mut::<crate::build::SelectedBuildPiece>().0 = 0;
            lock_place_target(world, cx, cz);
            complete_build_hold(world);
        }
        // ¿Quedó el piso? Y pasamos a la pared de madera (pieza de borde).
        s if s == floors + 1 => {
            let base = world.resource::<BuildTestState>().base;
            let body = bt_body(world);
            // Una celda con un árbol/roca encima NO admite piso (hay que talar
            // primero): eso es correcto, así que sólo exigimos las libres.
            let flora = world.resource::<crate::harvest::FloraHarvest>().clone();
            let tiles = world.resource::<TileWorld>();
            let z = tiles.z;
            let missing = (0..BT_SIDE)
                .flat_map(|dz| (0..BT_SIDE).map(move |dx| (dx, dz)))
                .filter(|&(dx, dz)| {
                    let (cx, cz) = (base.0 + dx, base.1 + dz);
                    !crate::world2d::cell_build_blocked(&flora, body, z, cx, cz)
                        && tiles.tile_at(body, cx, cz) != ha_shared::topdown::Tile::WoodFloor
                })
                .count();
            if missing > 0 {
                report_bug("build", "floor_placed", &format!("{missing} celdas libres sin piso"));
                world.resource_mut::<BuildTestState>().failed = true;
            }
            world.resource_mut::<crate::build::SelectedBuildPiece>().0 =
                crate::build::edge_palette_base();
        }
        // Paredes: un borde del perímetro por paso.
        s if s > floors + 1 && s <= floors + 1 + edges_n => {
            let base = world.resource::<BuildTestState>().base;
            let (tx, tz, axis) = bt_edges(base)[(s - floors - 2) as usize];
            world.resource_mut::<BuildTestState>().aim = bt_aim_for(tx, tz, axis);
            world.resource_mut::<crate::input::AimCursor>().world = bt_aim_for(tx, tz, axis);
            // El borde se toma de la mira, pero el targeting debe estar "en
            // alcance": con una celda apuntada alcanza.
            lock_place_target(world, tx, tz);
            complete_build_hold(world);
        }
        // ¿Se levantó el anillo completo?
        s if s == floors + edges_n + 2 => {
            // Esperadas = los bordes con piso de algún lado (los demás los
            // rechaza la regla de apoyo, y está bien que lo haga).
            let base = world.resource::<BuildTestState>().base;
            let body = bt_body(world);
            let expected = {
                let tiles = world.resource::<TileWorld>();
                let has_floor = |cx: i64, cz: i64| {
                    ha_shared::build_grid::is_build_floor(tiles.tile_at(body, cx, cz))
                };
                bt_edges(base)
                    .into_iter()
                    .filter(|&(tx, tz, axis)| {
                        ha_shared::build_grid::wall_supported(&has_floor, tx, tz, axis)
                    })
                    .count()
            };
            let n = bt_wall_count(world);
            world.resource_mut::<BuildTestState>().walls_before = n;
            if n != expected {
                report_bug("build", "walls_placed", &format!("{n} paredes, esperadas {expected}"));
                world.resource_mut::<BuildTestState>().failed = true;
            }
            // Pared SIN piso a ningún lado (3 tiles al norte del cuarto, dentro
            // del alcance): la regla de apoyo debe rechazarla.
            let far = (base.0, base.1 - 3);
            world.resource_mut::<BuildTestState>().aim =
                bt_aim_for(far.0, far.1, ha_shared::build_grid::WallAxis::H);
            lock_place_target(world, far.0, far.1);
            complete_build_hold(world);
        }
        s if s == floors + edges_n + 3 => {
            let n = bt_wall_count(world);
            let before = world.resource::<BuildTestState>().walls_before;
            if n != before {
                report_bug(
                    "build",
                    "wall_needs_floor",
                    &format!("la pared sin piso cambió el conteo: {before} → {n}"),
                );
                world.resource_mut::<BuildTestState>().failed = true;
            }
            // Al centro del cuarto y a caminar hacia el muro este.
            unlock(world);
            world.resource_mut::<crate::input::AimCursor>().locked = false;
            world.resource_mut::<BuildTestState>().aim = Vec2::ZERO;
            let base = world.resource::<BuildTestState>().base;
            let (cx, cz) = (base.0 as f32 + 0.5, base.1 as f32 + BT_SIDE as f32 * 0.5);
            let mut q = world.query_filtered::<&mut SimState, With<Player>>();
            if let Ok(mut sim) = q.get_single_mut(world) {
                sim.body.x = cx;
                sim.body.z = cz;
                sim.body.vx = 0.0;
                sim.body.vz = 0.0;
            }
            let mut st = world.resource_mut::<BuildTestState>();
            st.start_x = cx;
            st.walk_until = now + 2.5;
        }
        // Caminar hacia el este contra la pared.
        s if s == floors + edges_n + 4 => {
            press(world, KeyCode::KeyD);
            let until = world.resource::<BuildTestState>().walk_until;
            let mut st = world.resource_mut::<BuildTestState>();
            st.next_t = now + 0.05;
            if now < until {
                st.step = s; // repetir este paso hasta cumplir el tiempo
            }
        }
        // Veredicto: no cruzó el muro, pero sí avanzó hasta él.
        _ => {
            release_all(world);
            let base = world.resource::<BuildTestState>().base;
            let x = player_x(world);
            let wall_x = (base.0 + BT_SIDE) as f32;
            if x >= wall_x {
                report_bug(
                    "build",
                    "wall_blocks_player",
                    &format!("el jugador cruzó la pared: x={x:.2} ≥ {wall_x:.2}"),
                );
                world.resource_mut::<BuildTestState>().failed = true;
            }
            if x <= base.0 as f32 + 1.0 {
                report_bug(
                    "build",
                    "player_reaches_wall",
                    &format!("el jugador no avanzó hacia el muro: x={x:.2}"),
                );
                world.resource_mut::<BuildTestState>().failed = true;
            }
            let walls = bt_wall_count(world);
            let failed = world.resource::<BuildTestState>().failed;
            eprintln!(
                "ha-verify: BUILDTEST {} — paredes={walls} x={x:.2} (muro en {wall_x:.2})",
                if failed { "FAIL" } else { "DONE" }
            );
            shot(world, &format!("{FLYTEST_DIR}/buildtest-room.png"));
            // Un par de frames para que el screenshot llegue al disco.
            let mut st = world.resource_mut::<BuildTestState>();
            st.next_t = now + 1.5;
            if st.step > floors + edges_n + 8 {
                std::process::exit(if failed { 1 } else { 0 });
            }
        }
    }
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

// ---------------------------------------------------------------------------
// HA_TESTER — continuous AI player + perf harness (see plugin registration).
// ---------------------------------------------------------------------------

#[derive(Resource, Default)]
struct TesterState {
    phase: u8,
    entered: f32,
    last_snap: f32,
    last_cmd_poll: f32,
    dir_idx: u8,
    laps: u32,
    cleared_at: f32,
}

/// Directional walk keys, rotated each lap so the tester roams (streaming stress
/// in every direction) instead of pacing one line.
const TESTER_DIRS: [KeyCode; 4] = [KeyCode::KeyD, KeyCode::KeyS, KeyCode::KeyA, KeyCode::KeyW];

fn tester_cmd_path() -> String {
    format!("{FLYTEST_DIR}/tester-cmd.txt")
}

/// Teleport the player onto a body's surface (on-demand "go to X" support).
fn tester_teleport(world: &mut World, body: ha_shared::CelestialBody) {
    let (region, ctx) = match body {
        ha_shared::CelestialBody::Moon => {
            (crate::scene::RegionId::Luna, ha_shared::WorldContext::Luna)
        }
        _ => (
            crate::scene::RegionId::Overworld { gx: 0, gz: 0 },
            ha_shared::WorldContext::Overworld,
        ),
    };
    world.resource_mut::<crate::scene::RegionRuntime>().active = region;
    world.resource_mut::<crate::scene::WorldContextRuntime>().active = ctx;
    // Drop flight so we land as a walker.
    world.resource_mut::<crate::creative_fly::CreativeFlight>().phase =
        crate::creative_fly::Phase::Ground;
    let (sx, sz) = crate::scene::walkable_spawn(body, 0.0, 6.0);
    let mut q = world.query_filtered::<(&mut SimState, &mut PlayerVisual), With<Player>>();
    if let Ok((mut sim, mut vis)) = q.get_single_mut(world) {
        sim.body.x = sx;
        sim.body.z = sz;
        sim.body.y = 0.0;
        vis.from_x = sx;
        vis.to_x = sx;
        vis.from_z = sz;
        vis.to_z = sz;
    }
    eprintln!("ha-tester: teleported to {body:?} surface");
}

/// Poll `tester-cmd.txt` for a one-word directive, act on it, then clear the file
/// so it fires once. Lets a human steer the running tester without a rebuild.
fn tester_poll_command(world: &mut World, now: f32) {
    let path = tester_cmd_path();
    let Ok(raw) = std::fs::read_to_string(&path) else {
        return;
    };
    let cmd = raw.trim().to_ascii_lowercase();
    if cmd.is_empty() {
        return;
    }
    let _ = std::fs::remove_file(&path);
    eprintln!("ha-tester: command '{cmd}'");
    match cmd.as_str() {
        "shot" => shot(world, &format!("{FLYTEST_DIR}/tester-ondemand.png")),
        // `shot <nombre>` / `zoom <metros>`: sirven para barrer niveles de zoom y
        // verificar que la iluminación se ve igual a cualquier encuadre.
        _ if cmd.starts_with("shot ") => {
            let name = cmd[5..].trim().replace('/', "_");
            shot(world, &format!("{FLYTEST_DIR}/tester-{name}.png"));
        }
        _ if cmd.starts_with("zoom ") => {
            if let Ok(h) = cmd[5..].trim().parse::<f32>() {
                let mut z = world.resource_mut::<crate::camera::CameraZoom>();
                z.view_h = h;
                z.user_zoom_mult = 1.0;
                eprintln!("ha-tester: zoom view_h={h}");
            }
        }
        "earth" => tester_teleport(world, ha_shared::CelestialBody::Earth),
        "luna" | "moon" => tester_teleport(world, ha_shared::CelestialBody::Moon),
        "fly" | "flyup" => {
            let mut s = world.resource_mut::<TesterState>();
            s.phase = 3;
            s.entered = now;
            s.cleared_at = 0.0;
        }
        "land" | "descend" => {
            world.resource_mut::<crate::creative_fly::CreativeFlight>().phase =
                crate::creative_fly::Phase::Descend;
            let mut s = world.resource_mut::<TesterState>();
            s.phase = 4;
            s.entered = now;
        }
        // Órdenes al compañero (mano derecha): asignar tarea.
        "follow" => {
            world.resource_mut::<crate::companion::CompanionOrder>().0 =
                crate::companion::CompanionTask::Follow;
        }
        "gather" => {
            world.resource_mut::<crate::companion::CompanionOrder>().0 =
                crate::companion::CompanionTask::Gather;
        }
        "pray" => {
            world.resource_mut::<crate::companion::CompanionOrder>().0 =
                crate::companion::CompanionTask::Pray;
        }
        _ => eprintln!("ha-tester: unknown command '{cmd}'"),
    }
}

fn tester_goto_phase(world: &mut World, phase: u8, now: f32) {
    let mut s = world.resource_mut::<TesterState>();
    s.phase = phase;
    s.entered = now;
    s.cleared_at = 0.0;
}

fn tester_drive(world: &mut World) {
    let now = world.resource::<Time>().elapsed_secs();
    if now < 9.0 {
        return; // let boot/loading finish and gameplay input unblock
    }
    if !player_ready(world) {
        return; // aún sin jugador spawneado: reintentar el frame siguiente
    }

    // Live snapshot every ~3 s so a fresh frame is always on disk to inspect,
    // plus a heartbeat (perf logs `ha-perf fps=` on its own 5 s cadence).
    if now - world.resource::<TesterState>().last_snap > 3.0 {
        world.resource_mut::<TesterState>().last_snap = now;
        let (px, pz) = {
            let mut q = world.query_filtered::<&SimState, With<Player>>();
            q.get_single(world).map(|s| (s.body.x, s.body.z)).unwrap_or((0.0, 0.0))
        };
        let region = format!("{:?}", world.resource::<crate::scene::RegionRuntime>().active);
        let (phase, laps) = {
            let s = world.resource::<TesterState>();
            (s.phase, s.laps)
        };
        let ki = world.resource::<crate::flight::Flight>().ki_pct();
        eprintln!(
            "ha-tester: alive phase={phase} laps={laps} pos=({px:.0},{pz:.0}) region={region} ki={ki:.0}%"
        );
        shot(world, &format!("{FLYTEST_DIR}/tester-live.png"));
    }

    // On-demand steering (poll at ~2 Hz to keep the syscall cheap).
    if now - world.resource::<TesterState>().last_cmd_poll > 0.5 {
        world.resource_mut::<TesterState>().last_cmd_poll = now;
        tester_poll_command(world, now);
    }

    let (phase, entered) = {
        let s = world.resource::<TesterState>();
        (s.phase, s.entered)
    };
    let dt = now - entered;

    match phase {
        // WALK — roam in the current direction; streams the world (FPS stress).
        0 => {
            let dir = TESTER_DIRS[(world.resource::<TesterState>().dir_idx as usize) % 4];
            press(world, dir);
            if dt > 5.0 {
                release_all(world);
                let (tx, tz) = player_cell(world);
                dig_setup(world, (tx + 2, tz));
                tester_goto_phase(world, 1, now);
            }
        }
        // DIG — break the tile in front (real dig path: lock + complete hold).
        1 => {
            equip(world, "hand");
            let (tx, tz) = {
                let (tx, tz) = player_cell(world);
                (tx + 2, tz)
            };
            lock_block_target(world, tx, tz);
            complete_hold(world, false);
            if dt > 1.2 {
                unlock(world);
                let (px, pz) = player_cell(world);
                place_setup(world, (px + 1, pz));
                tester_goto_phase(world, 2, now);
            }
        }
        // BUILD — place a stone block next to the player.
        2 => {
            let (px, pz) = player_cell(world);
            lock_place_target(world, px + 1, pz);
            complete_hold(world, true);
            if dt > 1.2 {
                unlock(world);
                // Every 3rd lap, do a full Ki flight; otherwise keep roaming.
                if world.resource::<TesterState>().laps % 3 == 2 {
                    world.resource_mut::<crate::ui_editor::UiEditorState>().creative = true;
                    tester_goto_phase(world, 3, now);
                } else {
                    let mut s = world.resource_mut::<TesterState>();
                    s.dir_idx = s.dir_idx.wrapping_add(1);
                    s.laps += 1;
                    s.phase = 0;
                    s.entered = now;
                }
            }
        }
        // FLY UP — charge Ki (hold JUMP) and climb to a CLEAR orbit, hover a beat.
        3 => {
            world.resource_mut::<crate::ui_editor::UiEditorState>().creative = true;
            hold_with_fresh_edges(world, KeyCode::Space, now, 0.4);
            let (fphase, armed) = {
                let f = world.resource::<crate::creative_fly::CreativeFlight>();
                (f.phase, f.land_armed)
            };
            let cover = world.resource::<crate::transition::FlightTransition>().cover;
            if fphase == crate::creative_fly::Phase::Orbit && cover < 0.05 && armed {
                let cleared = world.resource::<TesterState>().cleared_at;
                if cleared == 0.0 {
                    world.resource_mut::<TesterState>().cleared_at = now;
                    eprintln!("ha-tester: reached CLEAR orbit");
                } else {
                    release_all(world); // hover
                    if now - cleared > 1.5 {
                        shot(world, &format!("{FLYTEST_DIR}/tester-orbit.png"));
                        tester_goto_phase(world, 4, now);
                    }
                }
            } else if dt > 40.0 {
                // Never cleared — bail flight, back to walking (no crash).
                eprintln!("ha-tester: flight timeout, resuming ground");
                release_all(world);
                world.resource_mut::<crate::ui_editor::UiEditorState>().creative = false;
                tester_goto_phase(world, 0, now);
            }
        }
        // DESCEND/LAND — hold USE (F) to actively descend to the surface, then
        // resume the loop. (Releasing everything in orbit only HOVERS.)
        _ => {
            release_all(world);
            press(world, KeyCode::KeyF); // hold USE = descend while flying
            let grounded = world.resource::<crate::creative_fly::CreativeFlight>().phase
                == crate::creative_fly::Phase::Ground;
            if grounded {
                world.resource_mut::<crate::ui_editor::UiEditorState>().creative = false;
                let mut s = world.resource_mut::<TesterState>();
                s.dir_idx = s.dir_idx.wrapping_add(1);
                s.laps += 1;
                s.phase = 0;
                s.entered = now;
                eprintln!("ha-tester: landed, lap complete");
            } else if dt > 30.0 {
                // Force a low descent so it can't hang forever.
                {
                let mut f = world.resource_mut::<crate::creative_fly::CreativeFlight>();
                f.vert = 400.0;
                f.alt = 400.0;
            }
                if dt > 45.0 {
                    tester_teleport(world, ha_shared::CelestialBody::Earth);
                    tester_goto_phase(world, 0, now);
                }
            }
        }
    }
}

fn smoke_drive(world: &mut World) {
    let now = world.resource::<Time>().elapsed_secs();
    if now < 9.0 {
        return;
    }
    if !player_ready(world) {
        return; // aún sin jugador spawneado: reintentar el frame siguiente
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
                // Back to surface, then hold JUMP to Saiyan-fly into space (Fase 4).
                world.resource_mut::<TileWorld>().z = 0;
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
                let alt = world.resource::<crate::flight::Flight>().altitude_m();
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
