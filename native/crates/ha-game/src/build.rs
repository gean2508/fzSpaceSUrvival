//! 2D construction system — build mode, ghost preview, placement.
//!
//! Pieces live in the bag's **CONSTRUIR** tab (`hud.rs`) — the screen stays
//! clean. Picking a piece there enters build mode and closes the bag. While
//! active, the aimed cell (`CursorHighlight`) becomes a placement slot: hold USE
//! / right-click to place the piece (spending its resource cost). A translucent
//! ghost shows validity (green = placeable, red = not enough materials). A small
//! on-screen chip is the ONLY persistent build UI — tap it (or pick a hotbar
//! item) to leave build mode. Removing a piece is just digging it (ha-shared
//! `Tile::dug_item` refunds the material). Desktop: `B` toggles build mode.

use bevy::prelude::*;

use ha_shared::topdown::build_piece;

use crate::game_ui::GameUiRoot;
use crate::hud::{BagBuildPieceButton, BagBuildSection, BagPanelState, BagTab};
use crate::input::{BuildModeActive, HoldInteractState};
use crate::inventory_persist::{InventoryPersistState, PlayerInventory};
use crate::scene::{RegionRuntime, WorldContextRuntime};
use crate::targeting::LookTarget;
use crate::ui_theme::{self, UiButtonState, UiButtonTone};
use crate::world2d::{
    active_surface_body, render_xy, surface_region_active, ActivePlanet, TileWorld,
};

/// Currently selected build piece (index into `BUILD_PIECES`).
#[derive(Resource, Default)]
pub struct SelectedBuildPiece(pub usize);

#[derive(Resource, Default)]
struct BuildIndicatorSpawned(bool);

#[derive(Component)]
struct BuildIndicator;
#[derive(Component)]
struct BuildIndicatorLabel;
#[derive(Component)]
struct BuildGhost;

pub struct BuildPlugin;

impl Plugin for BuildPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<SelectedBuildPiece>()
            .init_resource::<BuildIndicatorSpawned>()
            .add_systems(Startup, spawn_build_ghost)
            .add_systems(
                Update,
                (
                    ensure_build_indicator,
                    desktop_build_hotkey,
                    build_tab_select,
                    sync_bag_build_section,
                    sync_build_indicator,
                    apply_build_hold.before(crate::hud::update_interact_hud),
                    sync_build_ghost,
                ),
            );
    }
}

/// `B` toggles build mode on desktop (mobile enters via the bag's CONSTRUIR tab).
fn desktop_build_hotkey(keys: Res<ButtonInput<KeyCode>>, mut build_mode: ResMut<BuildModeActive>) {
    if crate::platform::uses_touch_gameplay_ui() {
        return;
    }
    if keys.just_pressed(KeyCode::KeyB) {
        build_mode.active = !build_mode.active;
    }
}

/// Picking a piece in the bag's CONSTRUIR tab selects it, enters build mode and
/// closes the bag so you can place it right away.
fn build_tab_select(
    mut buttons: Query<(&Interaction, &BagBuildPieceButton, &mut BackgroundColor), Changed<Interaction>>,
    mut selected: ResMut<SelectedBuildPiece>,
    mut build_mode: ResMut<BuildModeActive>,
    mut bag: ResMut<BagPanelState>,
) {
    for (interaction, button, mut bg) in &mut buttons {
        if *interaction == Interaction::Pressed {
            selected.0 = button.0;
            build_mode.active = true;
            bag.open = false;
            *bg = BackgroundColor(ui_theme::button_bg(UiButtonTone::Craft, UiButtonState::Pressed));
        } else {
            *bg = BackgroundColor(ui_theme::button_bg(
                UiButtonTone::Craft,
                ui_theme::interaction_state(*interaction, false),
            ));
        }
    }
}

/// Show/hide the CONSTRUIR section with its tab (display toggle owned here).
fn sync_bag_build_section(
    bag: Res<BagPanelState>,
    mut sections: Query<&mut Node, With<BagBuildSection>>,
) {
    for mut node in &mut sections {
        node.display = if bag.tab == BagTab::Build {
            Display::Flex
        } else {
            Display::None
        };
    }
}

/// The single persistent build-mode chip — upper-left (clear of the joystick
/// zones), hidden unless building. Tapping it leaves build mode.
fn ensure_build_indicator(
    mut commands: Commands,
    layout: Res<crate::input::TouchLayout>,
    root: Query<Entity, With<GameUiRoot>>,
    mut spawned: ResMut<BuildIndicatorSpawned>,
) {
    if spawned.0 || layout.width < 100.0 {
        return;
    }
    let Ok(root) = root.get_single() else {
        return;
    };
    spawned.0 = true;
    commands.entity(root).with_children(|parent| {
        parent
            .spawn((
                Button,
                Node {
                    position_type: PositionType::Absolute,
                    left: Val::Px(8.0),
                    top: Val::Px(100.0),
                    padding: UiRect::axes(Val::Px(8.0), Val::Px(4.0)),
                    border: UiRect::all(Val::Px(2.0)),
                    justify_content: JustifyContent::Center,
                    align_items: AlignItems::Center,
                    ..default()
                },
                BorderColor(ui_theme::accent_cyan()),
                BackgroundColor(ui_theme::button_bg(UiButtonTone::Primary, UiButtonState::Pressed)),
                Visibility::Hidden,
                BuildIndicator,
                Name::new("BuildIndicator"),
            ))
            .with_children(|b| {
                b.spawn((
                    Text::new("CONSTRUIR (tocar: salir)"),
                    TextFont {
                        font_size: 13.0,
                        ..default()
                    },
                    TextColor(ui_theme::text_primary()),
                    BuildIndicatorLabel,
                ));
            });
    });
}

fn sync_build_indicator(
    mut build_mode: ResMut<BuildModeActive>,
    selected: Res<SelectedBuildPiece>,
    taps: Query<&Interaction, (With<BuildIndicator>, Changed<Interaction>)>,
    mut chips: Query<&mut Visibility, With<BuildIndicator>>,
    mut label: Query<&mut Text, With<BuildIndicatorLabel>>,
) {
    // Tap the chip → leave build mode.
    for interaction in &taps {
        if *interaction == Interaction::Pressed {
            build_mode.active = false;
        }
    }
    let want = if build_mode.active {
        Visibility::Inherited
    } else {
        Visibility::Hidden
    };
    for mut v in &mut chips {
        if *v != want {
            *v = want;
        }
    }
    if build_mode.active {
        if let Ok(mut text) = label.get_single_mut() {
            let name = build_piece(selected.0).map(|p| p.label).unwrap_or("");
            let line = format!("CONSTRUIR: {name}  (tocar: salir)");
            if text.0 != line {
                text.0 = line;
            }
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
        Sprite::from_color(Color::srgba(0.25, 1.0, 0.35, 0.6), Vec2::splat(1.0)),
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
    // Bright green = placeable, red = not enough materials.
    sprite.color = if enough {
        Color::srgba(0.25, 1.0, 0.35, 0.6)
    } else {
        Color::srgba(1.0, 0.25, 0.2, 0.55)
    };
    let r = render_xy(tx as f32 + 0.5, tz as f32 + 0.5);
    tf.translation = Vec3::new(r.x, r.y, 5.7);
}
