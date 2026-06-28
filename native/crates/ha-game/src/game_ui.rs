//! Full-screen UI root — all gameplay HUD is parented here so center = screen center,
//! independent of the 38/62 touch split (that split is input-only, not layout).

use bevy::prelude::*;
use bevy::ui::FocusPolicy;

use crate::input::TouchLayout;

#[derive(Component)]
pub struct GameUiRoot;

#[derive(Resource, Default)]
pub struct GameUiRootReady(bool);

/// When true, raw touch gameplay (move stick, look, USE/JUMP) is suppressed.
#[derive(Resource, Default, Clone, Copy)]
pub struct UiGameplayBlock {
    pub blocked: bool,
}

const MENU_W: f32 = 96.0;

/// Horizontal offset from parent left so a widget of `width` is screen-centered.
pub fn ui_center_left(layout: &TouchLayout, width: f32) -> f32 {
    layout.width * 0.5 - width * 0.5
}

pub fn menu_screen_rect(layout: &TouchLayout) -> [f32; 4] {
    let left = ui_center_left(layout, MENU_W);
    let top = layout.insets.top as f32 + 8.0;
    [left, top, left + MENU_W, top + 26.0]
}

/// Full-screen touch block rect (top-left origin, same space as `TouchInput`).
pub fn fullscreen_block_rect(layout: &TouchLayout) -> [f32; 4] {
    [0.0, 0.0, layout.width, layout.height]
}

pub fn ensure_game_ui_root(mut commands: Commands, mut ready: ResMut<GameUiRootReady>) {
    if ready.0 {
        return;
    }
    ready.0 = true;
    commands.spawn((
        Node {
            position_type: PositionType::Absolute,
            left: Val::Px(0.0),
            top: Val::Px(0.0),
            width: Val::Percent(100.0),
            height: Val::Percent(100.0),
            ..default()
        },
        FocusPolicy::Pass,
        GameUiRoot,
        Name::new("GameUiRoot"),
    ));
}

pub fn sync_game_ui_root(
    layout: Res<TouchLayout>,
    mut q: Query<&mut Node, With<GameUiRoot>>,
    mut last: Local<(f32, f32)>,
) {
    if layout.width < 1.0 {
        return;
    }
    let key = (layout.width, layout.height);
    if *last == key {
        return;
    }
    *last = key;
    if let Ok(mut node) = q.get_single_mut() {
        node.width = Val::Px(layout.width);
        node.height = Val::Px(layout.height);
    }
}

/// Apply true screen-center positioning to a top-anchored bar/button.
pub fn apply_top_center(node: &mut Node, layout: &TouchLayout, width: f32, top: f32) {
    node.left = Val::Px(ui_center_left(layout, width));
    node.right = Val::Auto;
    node.top = Val::Px(top);
    node.bottom = Val::Auto;
    node.width = Val::Px(width);
    node.margin = UiRect::ZERO;
}

/// Apply true screen-center positioning to a bottom-anchored bar (hotbar).
pub fn apply_bottom_center(node: &mut Node, layout: &TouchLayout, width: f32, bottom: f32) {
    node.left = Val::Px(ui_center_left(layout, width));
    node.right = Val::Auto;
    node.bottom = Val::Px(bottom);
    node.top = Val::Auto;
    node.width = Val::Px(width);
    node.margin = UiRect::ZERO;
}

pub const MENU_BUTTON_WIDTH: f32 = MENU_W;

/// Center a floating panel on the full screen (mobile + desktop).
pub fn apply_center(node: &mut Node, layout: &TouchLayout, width: f32, height: f32) {
    node.left = Val::Px(ui_center_left(layout, width));
    node.top = Val::Px(((layout.height - height) * 0.5).max(layout.insets.top as f32));
    node.right = Val::Auto;
    node.bottom = Val::Auto;
    node.width = Val::Px(width);
    node.height = Val::Px(height);
    node.margin = UiRect::ZERO;
}
