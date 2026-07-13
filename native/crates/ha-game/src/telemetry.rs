//! Remote telemetry panel + camera uplink (Ellipse-style monitoring).

use bevy::prelude::*;

use crate::game_ui::GameUiRoot;
use crate::world_persist::{VesselPersistState, VesselRegistryRes};
use ha_shared::{ModuleKind, VesselId, VesselRegistry, VesselSimMode};

pub struct TelemetryPlugin;

impl Plugin for TelemetryPlugin {
    fn build(&self, app: &mut App) {
        // TX button removed from the HUD — base telemetry (remote camera) is a
        // future feature with no bases to monitor yet; it was just clutter in
        // the top-right. The systems stay (dormant) for when bases ship.
        app.init_resource::<TelemetryUiState>()
            .add_systems(
                Update,
                (
                    telemetry_toggle_input,
                    sync_telemetry_panel,
                    update_telemetry_status_text,
                    telemetry_button_click,
                    remote_camera_controls,
                ),
            );
    }
}

#[derive(Resource, Default)]
pub struct TelemetryUiState {
    pub open: bool,
    pub selected: VesselId,
}

#[derive(Component)]
struct TelemetryToggleButton;

#[derive(Component)]
struct TelemetryPanelRoot;

#[derive(Component)]
struct TelemetryCloseButton;

#[derive(Component)]
struct TelemetryVesselRow {
    vessel: VesselId,
}

#[derive(Component)]
struct TelemetryCameraButton;

#[derive(Component)]
struct TelemetryReturnButton;

#[derive(Component)]
struct TelemetryStatusText;

fn spawn_telemetry_button(mut commands: Commands) {
    commands.spawn((
        Button,
        Node {
            position_type: PositionType::Absolute,
            top: Val::Px(8.0),
            right: Val::Px(8.0),
            width: Val::Px(44.0),
            height: Val::Px(44.0),
            justify_content: JustifyContent::Center,
            align_items: AlignItems::Center,
            border: UiRect::all(Val::Px(2.0)),
            ..default()
        },
        BackgroundColor(Color::srgba(0.05, 0.12, 0.22, 0.85)),
        BorderColor(Color::srgb(0.2, 0.85, 1.0)),
        BorderRadius::all(Val::Px(4.0)),
        TelemetryToggleButton,
        Name::new("TelemetryToggle"),
    ))
    .with_children(|p| {
        p.spawn((
            Text::new("TX"),
            TextFont {
                font_size: 14.0,
                ..default()
            },
            TextColor(Color::srgb(0.2, 0.95, 1.0)),
        ));
    });
}

fn telemetry_toggle_input(
    keys: Res<ButtonInput<KeyCode>>,
    mut ui: ResMut<TelemetryUiState>,
) {
    if keys.just_pressed(KeyCode::KeyT) {
        ui.open = !ui.open;
    }
}

fn telemetry_button_click(
    mut ui: ResMut<TelemetryUiState>,
    mut interactions: Query<
        (&Interaction, &mut BackgroundColor),
        (Changed<Interaction>, With<TelemetryToggleButton>),
    >,
) {
    for (interaction, mut bg) in &mut interactions {
        if *interaction == Interaction::Pressed {
            ui.open = !ui.open;
            *bg = BackgroundColor(Color::srgba(0.1, 0.25, 0.4, 0.95));
        } else {
            *bg = BackgroundColor(Color::srgba(0.05, 0.12, 0.22, 0.85));
        }
    }
}

fn sync_telemetry_panel(
    mut commands: Commands,
    ui: Res<TelemetryUiState>,
    reg: Res<VesselRegistryRes>,
    root: Query<Entity, With<GameUiRoot>>,
    existing: Query<Entity, With<TelemetryPanelRoot>>,
) {
    let want = ui.open;
    let has = existing.iter().next();
    if !want {
        if let Some(e) = has {
            commands.entity(e).despawn_recursive();
        }
        return;
    }
    if has.is_some() {
        return;
    }
    let Ok(game_root) = root.get_single() else {
        return;
    };

    let panel = commands
        .spawn((
            TelemetryPanelRoot,
            Node {
                position_type: PositionType::Absolute,
                left: Val::Percent(5.0),
                top: Val::Percent(8.0),
                width: Val::Percent(90.0),
                max_height: Val::Percent(70.0),
                flex_direction: FlexDirection::Column,
                padding: UiRect::all(Val::Px(12.0)),
                row_gap: Val::Px(8.0),
                border: UiRect::all(Val::Px(2.0)),
                ..default()
            },
            BackgroundColor(Color::srgba(0.02, 0.06, 0.12, 0.92)),
            BorderColor(Color::srgb(0.2, 0.85, 1.0)),
            BorderRadius::all(Val::Px(6.0)),
        ))
        .id();

    commands.entity(game_root).add_child(panel);

    commands.entity(panel).with_children(|p| {
        p.spawn((
            Text::new("AGENCY TELEMETRY"),
            TextFont {
                font_size: 18.0,
                ..default()
            },
            TextColor(Color::srgb(0.2, 0.95, 1.0)),
        ));
        p.spawn((
            TelemetryCloseButton,
            Button,
            Node {
                position_type: PositionType::Absolute,
                top: Val::Px(4.0),
                right: Val::Px(4.0),
                width: Val::Px(32.0),
                height: Val::Px(32.0),
                justify_content: JustifyContent::Center,
                align_items: AlignItems::Center,
                ..default()
            },
            BackgroundColor(Color::srgba(0.3, 0.1, 0.1, 0.8)),
        ))
        .with_children(|c| {
            c.spawn((
                Text::new("X"),
                TextFont {
                    font_size: 16.0,
                    ..default()
                },
                TextColor(Color::WHITE),
            ));
        });

        for &vid in VesselId::all() {
            let Some(v) = reg.get(vid) else {
                continue;
            };
            let mode = match v.mode {
                VesselSimMode::ActiveFlight => "ACTIVE",
                VesselSimMode::ParkedOrbit => "ORBIT",
                VesselSimMode::ParkedSurface => "SURFACE",
                VesselSimMode::Inactive => "STANDBY",
            };
            let pwr = format!(
                "PWR {:.0}W {:.0}% BAT",
                v.power.net_w(),
                v.power.storage_pct()
            );
            let sig = format!(
                "SIG {:.0}% {}",
                v.connectivity.signal_strength * 100.0,
                if v.connectivity.earth_los { "EARTH" } else { "NO LINK" }
            );
            let line = format!("{} | {} | {} | {}", v.label, mode, pwr, sig);
            p.spawn((
                TelemetryVesselRow { vessel: vid },
                Button,
                Node {
                    width: Val::Percent(100.0),
                    padding: UiRect::all(Val::Px(6.0)),
                    ..default()
                },
                BackgroundColor(if ui.selected == vid {
                    Color::srgba(0.15, 0.35, 0.5, 0.9)
                } else {
                    Color::srgba(0.08, 0.14, 0.22, 0.85)
                }),
            ))
            .with_children(|row| {
                row.spawn((
                    Text::new(line),
                    TextFont {
                        font_size: 13.0,
                        ..default()
                    },
                    TextColor(Color::srgb(0.75, 0.9, 1.0)),
                ));
            });
        }

        p.spawn((
            TelemetryStatusText,
            Text::new(status_line(&reg, &ui)),
            TextFont {
                font_size: 12.0,
                ..default()
            },
            TextColor(Color::srgb(0.55, 0.75, 0.85)),
        ));

        p.spawn((
            TelemetryCameraButton,
            Button,
            Node {
                padding: UiRect::axes(Val::Px(12.0), Val::Px(8.0)),
                margin: UiRect::top(Val::Px(8.0)),
                ..default()
            },
            BackgroundColor(Color::srgba(0.1, 0.4, 0.55, 0.9)),
        ))
        .with_children(|b| {
            b.spawn((
                Text::new("REMOTE CAM — LUNA MODULE"),
                TextFont {
                    font_size: 14.0,
                    ..default()
                },
                TextColor(Color::srgb(0.2, 1.0, 0.85)),
            ));
        });

        if false {
            p.spawn((
                TelemetryReturnButton,
                Button,
                Node {
                    padding: UiRect::axes(Val::Px(12.0), Val::Px(8.0)),
                    ..default()
                },
                BackgroundColor(Color::srgba(0.45, 0.15, 0.15, 0.9)),
            ))
            .with_children(|b| {
                b.spawn((
                    Text::new("RETURN TO PLAYER"),
                    TextFont {
                        font_size: 14.0,
                        ..default()
                    },
                    TextColor(Color::WHITE),
                ));
            });
        }
    });
}

fn update_telemetry_status_text(
    ui: Res<TelemetryUiState>,
    reg: Res<VesselRegistryRes>,
    mut text_q: Query<&mut Text, With<TelemetryStatusText>>,
) {
    if !ui.open {
        return;
    }
    let line = status_line(&reg, &ui);
    for mut text in &mut text_q {
        **text = line.clone();
    }
}

fn status_line(reg: &VesselRegistry, ui: &TelemetryUiState) -> String {
    let Some(v) = reg.get(ui.selected) else {
        return String::new();
    };
    // Remote camera view is pending in the 2D client — always local for now.
    let cam = "CAM: LOCAL";
    format!(
        "{} | {} | SOL {:.0}% | {}",
        cam,
        ModuleKind::SolarPanel.label(),
        v.power.solar_efficiency * 100.0,
        v.body.label()
    )
}

fn remote_camera_controls(
    mut ui: ResMut<TelemetryUiState>,
    mut persist: ResMut<VesselPersistState>,
    mut reg: ResMut<VesselRegistryRes>,
    close_q: Query<
        &Interaction,
        (Changed<Interaction>, With<TelemetryCloseButton>),
    >,
    row_q: Query<
        (&Interaction, &TelemetryVesselRow),
        (Changed<Interaction>, Without<TelemetryCloseButton>),
    >,
    cam_q: Query<
        &Interaction,
        (Changed<Interaction>, With<TelemetryCameraButton>),
    >,
    ret_q: Query<
        &Interaction,
        (Changed<Interaction>, With<TelemetryReturnButton>),
    >,
) {
    for interaction in &close_q {
        if *interaction == Interaction::Pressed {
            ui.open = false;
        }
    }
    for (interaction, row) in &row_q {
        if *interaction == Interaction::Pressed {
            ui.selected = row.vessel;
        }
    }
    for interaction in &cam_q {
        if *interaction == Interaction::Pressed {
            reg.ensure(VesselId::LunaOutpost, ha_shared::CelestialBody::Moon);
            reg.park_surface(VesselId::LunaOutpost, ha_shared::CelestialBody::Moon, 0.0, -18.0);
            ui.open = false;
            persist.mark_immediate();
            // 2D: remote body camera is pending — vessel is parked/monitored only.
            eprintln!("ha-telemetry: remote camera pending in 2D client");
        }
    }
    for interaction in &ret_q {
        if *interaction == Interaction::Pressed {
            eprintln!("ha-telemetry: camera already follows the player in 2D");
        }
    }
}

