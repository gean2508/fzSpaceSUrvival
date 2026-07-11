//! Brawl-style app boot flow: splash logo → main menu (PLAY) → loading → game.
//!
//! Renders as a full-screen overlay ABOVE the HUD (`GlobalZIndex`), so the
//! world loads/streams underneath while the splash shows (that IS the loading).
//! Pressing PLAY fades the overlay out and reveals gameplay. No system-gating
//! refactor — low risk.

use bevy::prelude::*;
use bevy::ui::FocusPolicy;

use crate::ui_theme;

const SPLASH_SECS: f32 = 1.8;
const LOADING_SECS: f32 = 1.1;
/// Neon-arcade essence (Brawl chunk + Blood-Dragon glow), not a literal palette.
const NEON_CYAN: Color = Color::srgb(0.16, 0.94, 1.0);
const NEON_MAGENTA: Color = Color::srgb(1.0, 0.18, 0.62);
const VOID: Color = Color::srgb(0.04, 0.05, 0.09);

pub struct BootUiPlugin;

impl Plugin for BootUiPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<BootFlow>()
            .add_systems(Startup, spawn_boot_overlay)
            .add_systems(
                Update,
                (tick_boot_flow, play_button_system, boot_pulse_system),
            );
    }
}

#[derive(PartialEq, Clone, Copy)]
enum BootPhase {
    Splash,
    Menu,
    Loading,
    Done,
}

#[derive(Resource)]
struct BootFlow {
    phase: BootPhase,
    timer: f32,
    root: Option<Entity>,
}

impl Default for BootFlow {
    fn default() -> Self {
        Self {
            phase: BootPhase::Splash,
            timer: 0.0,
            root: None,
        }
    }
}

#[derive(Component)]
struct BootRoot;
#[derive(Component)]
struct BootTitle;
#[derive(Component)]
struct BootSubtitle;
#[derive(Component)]
struct PlayButton;
#[derive(Component)]
struct PlayButtonLabel;
#[derive(Component)]
struct LoadingBarFill;
#[derive(Component)]
struct LoadingGroup;
#[derive(Component)]
struct MenuGroup;
/// Pulsing glow accent (title underline / play button breathing).
#[derive(Component)]
struct BootPulse {
    base: f32,
}

fn spawn_boot_overlay(mut commands: Commands, mut flow: ResMut<BootFlow>) {
    let root = commands
        .spawn((
            Node {
                position_type: PositionType::Absolute,
                left: Val::Px(0.0),
                top: Val::Px(0.0),
                width: Val::Percent(100.0),
                height: Val::Percent(100.0),
                flex_direction: FlexDirection::Column,
                justify_content: JustifyContent::Center,
                align_items: AlignItems::Center,
                row_gap: Val::Px(18.0),
                ..default()
            },
            BackgroundColor(VOID),
            GlobalZIndex(1000),
            // Swallow clicks so the PLAY tap can't leak through to the HUD
            // behind (it was opening the BAG on start).
            FocusPolicy::Block,
            BootRoot,
            Name::new("BootOverlay"),
        ))
        .with_children(|root| {
            // Title logo (text-based, neon arcade — no image asset needed).
            root.spawn((
                Text::new("STARSHIP"),
                TextFont {
                    font_size: 58.0,
                    ..default()
                },
                TextColor(NEON_CYAN),
                BootTitle,
            ));
            root.spawn((
                Text::new("AGENCIA ESPACIAL"),
                TextFont {
                    font_size: 18.0,
                    ..default()
                },
                TextColor(NEON_MAGENTA),
                BootSubtitle,
            ));
            // Neon underline bar (pulses).
            root.spawn((
                Node {
                    width: Val::Px(220.0),
                    height: Val::Px(4.0),
                    margin: UiRect::top(Val::Px(6.0)),
                    ..default()
                },
                BackgroundColor(NEON_MAGENTA),
                BootPulse { base: 1.0 },
            ));

            // --- MAIN MENU group (hidden until Menu phase) -------------------
            root.spawn((
                Node {
                    margin: UiRect::top(Val::Px(40.0)),
                    flex_direction: FlexDirection::Column,
                    align_items: AlignItems::Center,
                    row_gap: Val::Px(14.0),
                    display: Display::None,
                    ..default()
                },
                MenuGroup,
            ))
            .with_children(|menu| {
                // Big chunky PLAY button (Brawl-style).
                menu.spawn((
                    Button,
                    Node {
                        width: Val::Px(240.0),
                        height: Val::Px(74.0),
                        border: UiRect::all(Val::Px(4.0)),
                        justify_content: JustifyContent::Center,
                        align_items: AlignItems::Center,
                        ..default()
                    },
                    BorderColor(NEON_CYAN),
                    BackgroundColor(Color::srgb(0.10, 0.14, 0.22)),
                    BootPulse { base: 1.0 },
                    PlayButton,
                    Name::new("PlayButton"),
                ))
                .with_children(|b| {
                    b.spawn((
                        Text::new("▶  PLAY"),
                        TextFont {
                            font_size: 30.0,
                            ..default()
                        },
                        TextColor(NEON_CYAN),
                        PlayButtonLabel,
                    ));
                });
                menu.spawn((
                    Text::new("tocá para lanzar la misión"),
                    TextFont {
                        font_size: 13.0,
                        ..default()
                    },
                    TextColor(ui_theme::text_primary().with_alpha(0.6)),
                ));
            });

            // --- LOADING group (hidden until Loading phase) ------------------
            root.spawn((
                Node {
                    margin: UiRect::top(Val::Px(40.0)),
                    width: Val::Px(260.0),
                    flex_direction: FlexDirection::Column,
                    align_items: AlignItems::Center,
                    row_gap: Val::Px(8.0),
                    display: Display::None,
                    ..default()
                },
                LoadingGroup,
            ))
            .with_children(|group| {
                group.spawn((
                    Text::new("CARGANDO SISTEMA SOLAR…"),
                    TextFont {
                        font_size: 13.0,
                        ..default()
                    },
                    TextColor(NEON_CYAN),
                ));
                // Loading bar track.
                group
                    .spawn((
                        Node {
                            width: Val::Px(240.0),
                            height: Val::Px(12.0),
                            border: UiRect::all(Val::Px(2.0)),
                            ..default()
                        },
                        BorderColor(NEON_CYAN),
                        BackgroundColor(Color::srgb(0.06, 0.08, 0.12)),
                    ))
                    .with_children(|track| {
                        track.spawn((
                            Node {
                                width: Val::Percent(0.0),
                                height: Val::Percent(100.0),
                                ..default()
                            },
                            BackgroundColor(NEON_MAGENTA),
                            LoadingBarFill,
                        ));
                    });
            });
        })
        .id();
    flow.root = Some(root);
}

fn tick_boot_flow(
    // REAL clock, not the virtual one: `Time<Virtual>.delta` is clamped to
    // ~0.25 s/frame, so at 2-3 fps (heavy first-load streaming) the splash +
    // loading timers took tens of real seconds to advance and the "STARSHIP"
    // overlay looked frozen over live gameplay. Real wall-time advances the
    // boot in 1-2 frames regardless of frame rate, so gameplay is revealed.
    time: Res<Time<bevy::time::Real>>,
    mut flow: ResMut<BootFlow>,
    mut commands: Commands,
    mut menu: Query<&mut Node, (With<MenuGroup>, Without<LoadingGroup>, Without<LoadingBarFill>)>,
    mut loading: Query<&mut Node, (With<LoadingGroup>, Without<MenuGroup>, Without<LoadingBarFill>)>,
    mut fill: Query<&mut Node, (With<LoadingBarFill>, Without<MenuGroup>, Without<LoadingGroup>)>,
) {
    if flow.phase == BootPhase::Done {
        return;
    }
    flow.timer += time.delta_secs();

    match flow.phase {
        BootPhase::Splash => {
            if flow.timer >= SPLASH_SECS {
                flow.phase = BootPhase::Menu;
                flow.timer = 0.0;
                if let Ok(mut node) = menu.get_single_mut() {
                    node.display = Display::Flex;
                }
            }
        }
        BootPhase::Menu => {} // waits for PLAY (play_button_system)
        BootPhase::Loading => {
            let pct = (flow.timer / LOADING_SECS).clamp(0.0, 1.0);
            if let Ok(mut node) = fill.get_single_mut() {
                node.width = Val::Percent(pct * 100.0);
            }
            if flow.timer >= LOADING_SECS {
                flow.phase = BootPhase::Done;
                if let Some(root) = flow.root.take() {
                    commands.entity(root).despawn_recursive();
                }
                eprintln!("ha-boot: menu → gameplay");
            }
        }
        BootPhase::Done => {}
    }

    // Reveal loading group when entering Loading.
    if flow.phase == BootPhase::Loading {
        if let Ok(mut node) = menu.get_single_mut() {
            node.display = Display::None;
        }
        if let Ok(mut node) = loading.get_single_mut() {
            node.display = Display::Flex;
        }
    }
}

fn play_button_system(
    mut flow: ResMut<BootFlow>,
    mut buttons: Query<
        (&Interaction, &mut BackgroundColor),
        (Changed<Interaction>, With<PlayButton>),
    >,
) {
    if flow.phase != BootPhase::Menu {
        return;
    }
    for (interaction, mut bg) in &mut buttons {
        match interaction {
            Interaction::Pressed => {
                *bg = BackgroundColor(Color::srgb(0.16, 0.24, 0.36));
                flow.phase = BootPhase::Loading;
                flow.timer = 0.0;
                eprintln!("ha-boot: PLAY pressed");
            }
            Interaction::Hovered => *bg = BackgroundColor(Color::srgb(0.14, 0.20, 0.30)),
            Interaction::None => *bg = BackgroundColor(Color::srgb(0.10, 0.14, 0.22)),
        }
    }
}

/// Breathing glow on pulse-tagged accents (title bar + play button).
fn boot_pulse_system(
    time: Res<Time>,
    flow: Res<BootFlow>,
    mut q: Query<(&BootPulse, &mut BorderColor), With<PlayButton>>,
    mut bars: Query<(&BootPulse, &mut BackgroundColor), Without<PlayButton>>,
) {
    if flow.phase == BootPhase::Done {
        return;
    }
    let t = time.elapsed_secs();
    let pulse = 0.55 + 0.45 * (t * 3.2).sin().abs();
    for (_p, mut border) in &mut q {
        *border = BorderColor(NEON_CYAN.with_alpha(pulse));
    }
    for (_p, mut bg) in &mut bars {
        *bg = BackgroundColor(NEON_MAGENTA.with_alpha(0.5 + 0.5 * pulse));
    }
}
