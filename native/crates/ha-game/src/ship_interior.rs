//! Ship interior — a bounded 2D room the player boards from the launch pad.
//!
//! Reuses the tile world at a dedicated positive Z-level: natural terrain there
//! is solid stone (see `topdown_tile`), so the carved `WoodFloor` room is
//! naturally enclosed without laying walls. Boarding and leaving are discrete
//! frame changes (region + Z-level + teleport), NEVER cutscenes (B8): input and
//! camera are never blocked.
//!
//! Phase 1 wires the interior scene + a stand-on boarding pad at the ship and a
//! stand-on exit door. The cabin (`CockpitSeat` + pre-flight HUD) lands in Fase 2
//! and the refuel station (`FuelSource`) in Fase 3.

use bevy::prelude::*;
use bevy::ui::FocusPolicy;
use ha_shared::topdown::{Tile, TILE_M};
use ha_shared::{CelestialBody, WorldContext};

use crate::art2d::ArtLibrary;
use crate::rocket_flight::RocketFlight;
use crate::scene::{RegionId, RegionRuntime, WorldContextRuntime};
use crate::sim::{Player, PlayerVisual, SimState};
use crate::solar_system_clock::SolarSystemClock;
use crate::world2d::{render_xy, sort_z, TileWorld};

/// Dedicated depth layer for the ship interior. Positive ⇒ natural terrain is
/// solid stone, so the room is enclosed. Far from `z=0` and the mine shafts.
pub const SHIP_INTERIOR_Z: i32 = 64;

/// Room rectangle in tile coords (inclusive), far from the mine-entrance portal
/// (which lives near the origin at any z).
const ROOM_X0: i64 = 300;
const ROOM_X1: i64 = 311;
const ROOM_Z0: i64 = 300;
const ROOM_Z1: i64 = 307;

/// Player stand point inside (world metres, cell centre) — near the top of the room.
pub const INTERIOR_SPAWN: (f32, f32) = (305.5, 302.5);
/// Exit cell (tile coords): stand on it to leave. Bottom-centre of the room.
pub const EXIT_CELL: (i64, i64) = (305, 307);
/// Boarding cell on the Earth surface, at the launch-pad ship (`LAUNCH_PAD_ZONE_X = 28`).
pub const ENTRANCE_CELL: (i64, i64) = (28, 6);
/// Cockpit seat cell (top-centre of the room). Stand on it to sit at the controls.
pub const COCKPIT_CELL: (i64, i64) = (305, 300);
/// Fuel station cell (engine-room corner). Walk up to it to top the tank.
pub const FUEL_SOURCE_CELL: (i64, i64) = (300, 300);
/// How close (metres) the player must be to the fuel station to refuel.
const FUEL_REACH_M: f32 = 1.8;

/// Runtime state for the (single, shared) ship interior.
#[derive(Resource, Default)]
pub struct ShipInteriorState {
    built: bool,
    /// Earth-surface point to drop the player back at when they leave.
    return_pos: Option<(f32, f32)>,
}

pub struct ShipInteriorPlugin;

impl Plugin for ShipInteriorPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<ShipInteriorState>()
            .init_resource::<CockpitState>()
            .add_event::<RefuelEvent>()
            .add_systems(Startup, (spawn_cockpit_marker, spawn_fuel_marker))
            .add_systems(
                Update,
                (
                    ship_transitions,
                    cockpit_seating,
                    toggle_preflight_panel,
                    update_preflight_fuel,
                    cockpit_button_system,
                    fuel_source_interact,
                    apply_refuel,
                ),
            );
    }
}

/// Cabin runtime: is the player sat at the controls (pre-flight)?
#[derive(Resource, Default)]
pub struct CockpitState {
    pub seated: bool,
}

/// Pre-flight actions offered by the seated cabin HUD.
#[derive(Clone, Copy)]
enum CockpitAction {
    /// Leave the interior and ignite manual surface flight.
    Launch,
    /// Same, but hand control to the autopilot (approach/hover — Fase 4).
    Autopilot,
    /// Stand up: unblock movement, step off the seat.
    StandUp,
}

#[derive(Component)]
struct CockpitButton(CockpitAction);

#[derive(Component)]
struct CockpitSeatMarker;

#[derive(Component)]
struct PreFlightPanel;

#[derive(Component)]
struct PreFlightFuelText;

/// Carve the WoodFloor room over the solid-rock Z-level. Assumes `tiles.z` is
/// already `SHIP_INTERIOR_Z`.
fn carve_room(tiles: &mut TileWorld) {
    for tz in ROOM_Z0..=ROOM_Z1 {
        for tx in ROOM_X0..=ROOM_X1 {
            tiles.force_tile(CelestialBody::Earth, tx, tz, Tile::WoodFloor);
        }
    }
    tiles.force_tile(CelestialBody::Earth, EXIT_CELL.0, EXIT_CELL.1, Tile::Door);
}

fn teleport(sim: &mut SimState, visual: &mut PlayerVisual, x: f32, z: f32) {
    sim.body.x = x;
    sim.body.z = z;
    sim.body.y = 0.0;
    sim.body.vx = 0.0;
    sim.body.vz = 0.0;
    sim.body.vy = 0.0;
    visual.from_x = x;
    visual.to_x = x;
    visual.from_z = z;
    visual.to_z = z;
    visual.from_y = 0.0;
    visual.to_y = 0.0;
}

/// Enter the ship interior: switch region/context/Z and teleport the player in.
pub fn board_ship(
    tiles: &mut TileWorld,
    region: &mut RegionRuntime,
    ctx: &mut WorldContextRuntime,
    state: &mut ShipInteriorState,
    sim: &mut SimState,
    visual: &mut PlayerVisual,
) {
    state.return_pos = Some((sim.body.x, sim.body.z));
    tiles.z = SHIP_INTERIOR_Z;
    tiles.version = tiles.version.wrapping_add(1);
    if !state.built {
        carve_room(tiles);
        state.built = true;
    }
    region.active = RegionId::ShipInterior;
    ctx.active = WorldContext::Mission;
    teleport(sim, visual, INTERIOR_SPAWN.0, INTERIOR_SPAWN.1);
    eprintln!("ha-ship: boarded → interior (z={SHIP_INTERIOR_Z})");
}

/// Leave the ship interior: back to the Earth overworld at the boarding point.
pub fn leave_ship(
    tiles: &mut TileWorld,
    region: &mut RegionRuntime,
    ctx: &mut WorldContextRuntime,
    state: &mut ShipInteriorState,
    sim: &mut SimState,
    visual: &mut PlayerVisual,
) {
    tiles.z = 0;
    tiles.version = tiles.version.wrapping_add(1);
    region.active = RegionId::Overworld { gx: 0, gz: 0 };
    ctx.active = WorldContext::Overworld;
    let (rx, rz) = state
        .return_pos
        .take()
        .unwrap_or((ENTRANCE_CELL.0 as f32 + 0.5, ENTRANCE_CELL.1 as f32 + 0.5));
    teleport(sim, visual, rx, rz);
    eprintln!("ha-ship: left interior → Earth");
}

/// Stand-on boarding/exit, mirroring the mine-shaft `layer_transition`: the
/// `armed` latch requires stepping off the trigger cell before it fires again,
/// so you can't ping-pong. USE-driven boarding + the cockpit come in Fase 2.
#[allow(clippy::too_many_arguments)]
fn ship_transitions(
    mut region: ResMut<RegionRuntime>,
    mut ctx: ResMut<WorldContextRuntime>,
    mut tiles: ResMut<TileWorld>,
    mut state: ResMut<ShipInteriorState>,
    mut armed: Local<bool>,
    mut started: Local<bool>,
    mut player: Query<(&mut SimState, &mut PlayerVisual), With<Player>>,
) {
    if !*started {
        *armed = true;
        *started = true;
    }
    let Ok((mut sim, mut visual)) = player.get_single_mut() else {
        return;
    };
    let tx = (sim.body.x / TILE_M).floor() as i64;
    let tz = (sim.body.z / TILE_M).floor() as i64;

    match region.active {
        RegionId::Overworld { .. } if ctx.active == WorldContext::Overworld => {
            if (tx, tz) == ENTRANCE_CELL {
                if *armed {
                    board_ship(
                        &mut tiles, &mut region, &mut ctx, &mut state, &mut sim, &mut visual,
                    );
                    *armed = false;
                }
            } else {
                *armed = true;
            }
        }
        RegionId::ShipInterior => {
            if (tx, tz) == EXIT_CELL {
                if *armed {
                    leave_ship(
                        &mut tiles, &mut region, &mut ctx, &mut state, &mut sim, &mut visual,
                    );
                    *armed = false;
                }
            } else {
                *armed = true;
            }
        }
        _ => {}
    }
}

// ---------------------------------------------------------------------------
// Fase 2 — cabina (CockpitSeat) + HUD de pre-vuelo (PreFlightUI)
// ---------------------------------------------------------------------------

/// A one-time marker sprite over the cockpit seat cell (logical art slot
/// `decor/cockpit_seat`; colour fallback until textured). Lives at the interior
/// world coords, so it's only on-camera while the player is inside.
fn spawn_cockpit_marker(mut commands: Commands, art: Res<ArtLibrary>) {
    let wx = COCKPIT_CELL.0 as f32 + 0.5;
    let wz = COCKPIT_CELL.1 as f32 + 0.5;
    let r = render_xy(wx, wz);
    commands.spawn((
        CockpitSeatMarker,
        art.sprite_or_color(
            "decor/cockpit_seat",
            Color::srgb(0.20, 0.55, 0.72),
            Vec2::new(1.1, 1.3),
        ),
        Transform::from_xyz(r.x, r.y + 0.3, sort_z(wz) + 0.2),
    ));
}

/// Sit down when the player stands on the cockpit cell; movement is blocked
/// while seated (see `sync_ui_gameplay_block` in hud.rs, which reads
/// `CockpitState::seated`). Standing up is the panel's `LEVANTARSE` button.
fn cockpit_seating(
    region: Res<RegionRuntime>,
    mut cockpit: ResMut<CockpitState>,
    mut player: Query<(&mut SimState, &mut PlayerVisual), With<Player>>,
) {
    // Any non-interior region (e.g. we just launched) clears the seated latch.
    if region.active != RegionId::ShipInterior {
        cockpit.seated = false;
        return;
    }
    if cockpit.seated {
        return;
    }
    let Ok((mut sim, mut visual)) = player.get_single_mut() else {
        return;
    };
    let tx = (sim.body.x / TILE_M).floor() as i64;
    let tz = (sim.body.z / TILE_M).floor() as i64;
    if (tx, tz) == COCKPIT_CELL {
        cockpit.seated = true;
        teleport(
            &mut sim,
            &mut visual,
            COCKPIT_CELL.0 as f32 + 0.5,
            COCKPIT_CELL.1 as f32 + 0.5,
        );
    }
}

/// Spawn the pre-flight panel on sitting, despawn on standing (guarded by the
/// panel's presence, so this is idempotent every frame).
fn toggle_preflight_panel(
    mut commands: Commands,
    cockpit: Res<CockpitState>,
    panel: Query<Entity, With<PreFlightPanel>>,
) {
    match (cockpit.seated, panel.get_single()) {
        (true, Err(_)) => spawn_preflight_panel(&mut commands),
        (false, Ok(e)) => commands.entity(e).despawn_recursive(),
        _ => {}
    }
}

fn update_preflight_fuel(
    cockpit: Res<CockpitState>,
    flight: Res<RocketFlight>,
    mut q: Query<&mut Text, With<PreFlightFuelText>>,
) {
    if !cockpit.seated {
        return;
    }
    let cap = flight.capacity();
    let pct = if cap > 0.0 {
        (flight.fuel / cap * 100.0).clamp(0.0, 100.0)
    } else {
        0.0
    };
    for mut t in &mut q {
        *t = Text::new(format!("Combustible: {pct:.0}%"));
    }
}

#[allow(clippy::too_many_arguments)]
fn cockpit_button_system(
    mut buttons: Query<(&Interaction, &CockpitButton, &mut BackgroundColor), Changed<Interaction>>,
    mut cockpit: ResMut<CockpitState>,
    mut tiles: ResMut<TileWorld>,
    mut region: ResMut<RegionRuntime>,
    mut ctx: ResMut<WorldContextRuntime>,
    mut state: ResMut<ShipInteriorState>,
    mut flight: ResMut<RocketFlight>,
    clock: Res<SolarSystemClock>,
    mut player: Query<(&mut SimState, &mut PlayerVisual), With<Player>>,
) {
    for (interaction, btn, mut bg) in &mut buttons {
        match *interaction {
            Interaction::Pressed => {
                let Ok((mut sim, mut visual)) = player.get_single_mut() else {
                    return;
                };
                match btn.0 {
                    CockpitAction::StandUp => {
                        cockpit.seated = false;
                        teleport(
                            &mut sim,
                            &mut visual,
                            COCKPIT_CELL.0 as f32 + 0.5,
                            COCKPIT_CELL.1 as f32 + 1.5,
                        );
                    }
                    CockpitAction::Launch | CockpitAction::Autopilot => {
                        cockpit.seated = false;
                        leave_ship(
                            &mut tiles, &mut region, &mut ctx, &mut state, &mut sim, &mut visual,
                        );
                        // Reuse the pad ignition path (see launch_pad.rs) and top
                        // the tank so the flight is actually playable (the FuelSource
                        // stays useful for off-world top-ups). `leave_ship` already
                        // put the walker back at the boarding spot — launch from it.
                        flight.clock_secs = clock.t;
                        let (pos, vel) = crate::rocket_flight::launch_pos_vel_from(
                            CelestialBody::Earth,
                            clock.t,
                            18.0,
                            sim.body.x,
                        );
                        flight.surface_flight = Some(CelestialBody::Earth);
                        flight.begin_at(pos);
                        flight.fuel = flight.capacity();
                        flight.vel = vel;
                        flight.autopilot = matches!(btn.0, CockpitAction::Autopilot);
                        eprintln!("ha-ship: cockpit launch (autopilot={})", flight.autopilot);
                    }
                }
                *bg = BackgroundColor(Color::srgb(0.18, 0.26, 0.40));
            }
            Interaction::Hovered => *bg = BackgroundColor(Color::srgb(0.14, 0.20, 0.30)),
            Interaction::None => *bg = BackgroundColor(Color::srgb(0.10, 0.14, 0.22)),
        }
    }
}

const CABIN_CYAN: Color = Color::srgb(0.16, 0.94, 1.0);

fn spawn_preflight_panel(commands: &mut Commands) {
    commands
        .spawn((
            PreFlightPanel,
            Node {
                position_type: PositionType::Absolute,
                left: Val::Px(0.0),
                top: Val::Px(0.0),
                width: Val::Percent(100.0),
                height: Val::Percent(100.0),
                justify_content: JustifyContent::Center,
                align_items: AlignItems::Center,
                ..default()
            },
            FocusPolicy::Pass,
            Name::new("PreFlightPanel"),
        ))
        .with_children(|root| {
            root.spawn((
                Node {
                    width: Val::Px(300.0),
                    flex_direction: FlexDirection::Column,
                    align_items: AlignItems::Center,
                    padding: UiRect::all(Val::Px(16.0)),
                    row_gap: Val::Px(10.0),
                    border: UiRect::all(Val::Px(2.0)),
                    ..default()
                },
                BackgroundColor(Color::srgb(0.03, 0.045, 0.08)),
                BorderColor(CABIN_CYAN.with_alpha(0.7)),
            ))
            .with_children(|panel| {
                panel.spawn((
                    Text::new("CABINA - PRE-VUELO"),
                    TextFont {
                        font_size: 20.0,
                        ..default()
                    },
                    TextColor(CABIN_CYAN),
                ));
                panel.spawn((
                    PreFlightFuelText,
                    Text::new("Combustible: —"),
                    TextFont {
                        font_size: 14.0,
                        ..default()
                    },
                    TextColor(Color::srgb(0.85, 0.9, 0.95)),
                ));
                cockpit_button(panel, "DESPEGAR", CockpitAction::Launch);
                cockpit_button(panel, "AUTOPILOTO", CockpitAction::Autopilot);
                cockpit_button(panel, "LEVANTARSE", CockpitAction::StandUp);
            });
        });
}

// ---------------------------------------------------------------------------
// Fase 3 — estación de recarga (FuelSource) + bucle de combustible por eventos
// ---------------------------------------------------------------------------

/// Emitted when the player triggers the engine-room fuel station; the tank is
/// filled by `apply_refuel` (EventWriter → EventReader loop, as designed).
#[derive(Event)]
struct RefuelEvent;

#[derive(Component)]
struct FuelSourceMarker;

/// One-time marker sprite over the fuel station (logical slot `decor/fuel_source`;
/// colour fallback until textured).
fn spawn_fuel_marker(mut commands: Commands, art: Res<ArtLibrary>) {
    let wx = FUEL_SOURCE_CELL.0 as f32 + 0.5;
    let wz = FUEL_SOURCE_CELL.1 as f32 + 0.5;
    let r = render_xy(wx, wz);
    commands.spawn((
        FuelSourceMarker,
        art.sprite_or_color(
            "decor/fuel_source",
            Color::srgb(0.85, 0.55, 0.16),
            Vec2::new(1.0, 1.4),
        ),
        Transform::from_xyz(r.x, r.y + 0.3, sort_z(wz) + 0.2),
    ));
}

/// Inside the interior, emit a `RefuelEvent` when the player steps up to the fuel
/// station with a tank that isn't full. Edge-triggered via `armed` (step away to
/// refuel again), so one approach = one refuel, never a per-frame spam.
fn fuel_source_interact(
    region: Res<RegionRuntime>,
    flight: Res<RocketFlight>,
    mut armed: Local<bool>,
    mut writer: EventWriter<RefuelEvent>,
    player: Query<&SimState, With<Player>>,
) {
    if region.active != RegionId::ShipInterior {
        *armed = true;
        return;
    }
    let Ok(sim) = player.get_single() else {
        return;
    };
    let dx = sim.body.x - (FUEL_SOURCE_CELL.0 as f32 + 0.5);
    let dz = sim.body.z - (FUEL_SOURCE_CELL.1 as f32 + 0.5);
    let near = (dx * dx + dz * dz).sqrt() <= FUEL_REACH_M;
    if !near {
        *armed = true;
        return;
    }
    if *armed && flight.fuel < flight.capacity() - 0.01 {
        writer.send(RefuelEvent);
        *armed = false;
    }
}

/// Fill the tank to capacity on a `RefuelEvent`.
fn apply_refuel(mut events: EventReader<RefuelEvent>, mut flight: ResMut<RocketFlight>) {
    let mut refuelled = false;
    for _ in events.read() {
        refuelled = true;
    }
    if refuelled {
        flight.fuel = flight.capacity();
        eprintln!("ha-ship: refuelled at engine-room station → {:.0}", flight.fuel);
    }
}

fn cockpit_button(parent: &mut ChildBuilder, label: &str, action: CockpitAction) {
    parent
        .spawn((
            Button,
            CockpitButton(action),
            Node {
                width: Val::Px(220.0),
                height: Val::Px(40.0),
                border: UiRect::all(Val::Px(2.0)),
                justify_content: JustifyContent::Center,
                align_items: AlignItems::Center,
                ..default()
            },
            BorderColor(CABIN_CYAN.with_alpha(0.6)),
            BackgroundColor(Color::srgb(0.10, 0.14, 0.22)),
            Name::new(format!("CockpitBtn_{label}")),
        ))
        .with_children(|b| {
            b.spawn((
                Text::new(label),
                TextFont {
                    font_size: 16.0,
                    ..default()
                },
                TextColor(CABIN_CYAN),
            ));
        });
}
