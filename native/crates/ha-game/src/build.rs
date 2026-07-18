//! 2D construction system — build mode, piece palette, ghost preview, place.
//!
//! Enter build mode with the CONSTRUIR toggle (or the `B` key on desktop). While
//! active, the aim cursor's highlighted cell (`CursorHighlight`) becomes a
//! placement slot for the selected `BuildPiece`: hold USE / right-click to place
//! it (consuming its resource cost). A translucent ghost previews validity
//! (green = placeable, red = occupied / not enough materials). Removing a piece
//! is just digging it — `Tile::dug_item` refunds the material (see ha-shared).

use bevy::prelude::*;

use ha_shared::topdown::{build_piece, BUILD_PIECES};

use crate::game_ui::GameUiRoot;
use crate::input::{BuildModeActive, HoldInteractState};
use crate::inventory_persist::{InventoryPersistState, PlayerInventory};
use crate::scene::{RegionRuntime, WorldContextRuntime};
use crate::targeting::LookTarget;
use crate::ui_theme::{self, UiButtonState, UiButtonTone};
use crate::world2d::{
    active_surface_body, render_xy, surface_region_active, ActivePlanet, TileWorld,
};

/// Currently selected palette piece (index into `BUILD_PIECES`).
#[derive(Resource, Default)]
pub struct SelectedBuildPiece(pub usize);

#[derive(Resource, Default)]
struct BuildUiSpawned(bool);

#[derive(Component)]
struct BuildToggleButton;
#[derive(Component)]
struct BuildToggleLabel;
#[derive(Component)]
struct BuildPaletteRoot;
#[derive(Component)]
struct BuildPaletteButton(usize);
#[derive(Component)]
struct BuildGhost;

pub struct BuildPlugin;

impl Plugin for BuildPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<SelectedBuildPiece>()
            .init_resource::<BuildUiSpawned>()
            .add_systems(Startup, spawn_build_ghost)
            .add_systems(
                Update,
                (
                    ensure_build_ui,
                    desktop_build_hotkey,
                    build_toggle_click,
                    build_palette_click,
                    sync_build_ui,
                    apply_build_hold.before(crate::hud::update_interact_hud),
                    sync_build_ghost,
                ),
            );
    }
}

/// `B` toggles build mode on desktop (mobile uses the CONSTRUIR button).
fn desktop_build_hotkey(
    keys: Res<ButtonInput<KeyCode>>,
    mut build_mode: ResMut<BuildModeActive>,
) {
    if crate::platform::uses_touch_gameplay_ui() {
        return;
    }
    if keys.just_pressed(KeyCode::KeyB) {
        build_mode.active = !build_mode.active;
    }
}

fn ensure_build_ui(
    mut commands: Commands,
    layout: Res<crate::input::TouchLayout>,
    root: Query<Entity, With<GameUiRoot>>,
    mut spawned: ResMut<BuildUiSpawned>,
) {
    if spawned.0 || layout.width < 100.0 {
        return;
    }
    let Ok(root) = root.get_single() else {
        return;
    };
    spawned.0 = true;

    commands.entity(root).with_children(|parent| {
        // Toggle — upper-left, clear of the joystick zones (right half = look,
        // lower-left = move) so touches reach the button, not a stick.
        parent
            .spawn((
                Button,
                Node {
                    position_type: PositionType::Absolute,
                    left: Val::Px(8.0),
                    top: Val::Px(100.0),
                    width: Val::Px(124.0),
                    height: Val::Px(30.0),
                    border: UiRect::all(Val::Px(2.0)),
                    justify_content: JustifyContent::Center,
                    align_items: AlignItems::Center,
                    ..default()
                },
                BorderColor(ui_theme::button_border(UiButtonTone::Primary)),
                BackgroundColor(ui_theme::button_bg(
                    UiButtonTone::Primary,
                    UiButtonState::Normal,
                )),
                BuildToggleButton,
                Name::new("BuildToggleButton"),
            ))
            .with_children(|b| {
                b.spawn((
                    Text::new("CONSTRUIR"),
                    TextFont {
                        font_size: 13.0,
                        ..default()
                    },
                    TextColor(ui_theme::text_primary()),
                    BuildToggleLabel,
                ));
            });

        // Palette — vertical stack below the toggle, hidden until build mode on.
        parent
            .spawn((
                Node {
                    position_type: PositionType::Absolute,
                    left: Val::Px(8.0),
                    top: Val::Px(136.0),
                    flex_direction: FlexDirection::Column,
                    row_gap: Val::Px(4.0),
                    ..default()
                },
                Visibility::Hidden,
                BuildPaletteRoot,
                Name::new("BuildPalette"),
            ))
            .with_children(|col| {
                for (idx, piece) in BUILD_PIECES.iter().enumerate() {
                    col.spawn((
                        Button,
                        Node {
                            width: Val::Px(150.0),
                            height: Val::Px(28.0),
                            border: UiRect::all(Val::Px(2.0)),
                            justify_content: JustifyContent::Center,
                            align_items: AlignItems::Center,
                            ..default()
                        },
                        BorderColor(ui_theme::button_border(UiButtonTone::Inventory)),
                        BackgroundColor(ui_theme::button_bg(
                            UiButtonTone::Inventory,
                            UiButtonState::Normal,
                        )),
                        BuildPaletteButton(idx),
                        Name::new(format!("BuildPiece_{}", piece.id)),
                    ))
                    .with_children(|b| {
                        b.spawn((
                            Text::new(format!(
                                "{} ({}×{})",
                                piece.label, piece.cost_qty, piece.cost_item
                            )),
                            TextFont {
                                font_size: 12.0,
                                ..default()
                            },
                            TextColor(ui_theme::text_primary()),
                        ));
                    });
                }
            });
    });
}

fn build_toggle_click(
    buttons: Query<&Interaction, (With<BuildToggleButton>, Changed<Interaction>)>,
    mut build_mode: ResMut<BuildModeActive>,
) {
    for interaction in &buttons {
        if *interaction == Interaction::Pressed {
            build_mode.active = !build_mode.active;
        }
    }
}

fn build_palette_click(
    buttons: Query<(&Interaction, &BuildPaletteButton), Changed<Interaction>>,
    mut selected: ResMut<SelectedBuildPiece>,
) {
    for (interaction, button) in &buttons {
        if *interaction == Interaction::Pressed {
            selected.0 = button.0;
        }
    }
}

#[allow(clippy::type_complexity)]
fn sync_build_ui(
    build_mode: Res<BuildModeActive>,
    selected: Res<SelectedBuildPiece>,
    mut sets: ParamSet<(
        Query<&mut Visibility, With<BuildPaletteRoot>>,
        Query<(&BuildPaletteButton, &mut BorderColor)>,
        Query<(&mut BackgroundColor, &mut BorderColor), With<BuildToggleButton>>,
        Query<&mut Text, With<BuildToggleLabel>>,
    )>,
) {
    let active = build_mode.active;
    for mut v in &mut sets.p0() {
        *v = if active {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        };
    }
    for (button, mut border) in &mut sets.p1() {
        *border = BorderColor(if button.0 == selected.0 {
            ui_theme::accent_cyan()
        } else {
            ui_theme::button_border(UiButtonTone::Inventory)
        });
    }
    if let Ok((mut bg, mut border)) = sets.p2().get_single_mut() {
        let tone = if active {
            UiButtonState::Pressed
        } else {
            UiButtonState::Normal
        };
        *bg = BackgroundColor(ui_theme::button_bg(UiButtonTone::Primary, tone));
        *border = BorderColor(if active {
            ui_theme::accent_cyan()
        } else {
            ui_theme::button_border(UiButtonTone::Primary)
        });
    }
    if let Ok(mut text) = sets.p3().get_single_mut() {
        let want = if active { "CONSTRUIR ✓" } else { "CONSTRUIR" };
        if text.0 != want {
            text.0 = want.to_string();
        }
    }
}

/// Consumes a completed build hold: places the selected piece at the aimed cell,
/// spending its resource cost. Mirrors `try_place_tile_on_hold` for blocks.
fn apply_build_hold(
    mut hold: ResMut<HoldInteractState>,
    target: Res<LookTarget>,
    selected: Res<SelectedBuildPiece>,
    ctx: Res<WorldContextRuntime>,
    planet: Res<ActivePlanet>,
    mut tiles: ResMut<TileWorld>,
    mut inv: ResMut<PlayerInventory>,
    mut persist: ResMut<InventoryPersistState>,
) {
    if !hold.completed || !hold.placing_build {
        return;
    }
    hold.completed = false;
    hold.placing_build = false;
    let Some((tx, tz, _)) = target.place_cell else {
        return;
    };
    let Some(piece) = build_piece(selected.0) else {
        return;
    };
    if inv.qty(piece.cost_item) < piece.cost_qty {
        return;
    }
    let body = active_surface_body(&ctx, &planet);
    if tiles.place_tile(body, tx as i64, tz as i64, piece.tile) {
        inv.remove(piece.cost_item, piece.cost_qty);
        persist.mark_immediate();
    }
}

fn spawn_build_ghost(mut commands: Commands) {
    commands.spawn((
        BuildGhost,
        Sprite::from_color(Color::srgba(0.3, 1.0, 0.3, 0.4), Vec2::splat(1.0)),
        Transform::from_xyz(0.0, 0.0, 5.7),
        Visibility::Hidden,
    ));
}

fn sync_build_ghost(
    build_mode: Res<BuildModeActive>,
    region: Res<RegionRuntime>,
    target: Res<LookTarget>,
    selected: Res<SelectedBuildPiece>,
    inv: Res<PlayerInventory>,
    mut ghost: Query<(&mut Transform, &mut Visibility, &mut Sprite), With<BuildGhost>>,
) {
    let Ok((mut tf, mut vis, mut sprite)) = ghost.get_single_mut() else {
        return;
    };
    let placeable = build_mode.active && surface_region_active(&region).is_some();
    let Some((tx, tz, _)) = target.place_cell.filter(|_| placeable) else {
        *vis = Visibility::Hidden;
        return;
    };
    let Some(piece) = build_piece(selected.0) else {
        *vis = Visibility::Hidden;
        return;
    };
    *vis = Visibility::Visible;
    let enough = inv.qty(piece.cost_item) >= piece.cost_qty;
    // Bright green = placeable, red = not enough materials. High contrast so the
    // preview reads on any terrain (the palette already shows which piece).
    sprite.color = if enough {
        Color::srgba(0.25, 1.0, 0.35, 0.6)
    } else {
        Color::srgba(1.0, 0.25, 0.2, 0.55)
    };
    let r = render_xy(tx as f32 + 0.5, tz as f32 + 0.5);
    tf.translation = Vec3::new(r.x, r.y, 5.7);
}
