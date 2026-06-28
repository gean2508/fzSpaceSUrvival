use bevy::prelude::*;
use bevy::ui::FocusPolicy;

use crate::game_ui::GameUiRoot;
use crate::input::TouchLayout;
use crate::targeting::LookTarget;
use crate::ui_theme;

#[derive(Component)]
pub(crate) struct FpCrosshairRoot;

#[derive(Component)]
pub(crate) struct FpCrosshairH;

#[derive(Component)]
pub(crate) struct FpCrosshairV;

#[derive(Resource, Default)]
pub struct FpCrosshairSpawned(bool);

pub fn ensure_fp_crosshair(
    mut commands: Commands,
    layout: Res<TouchLayout>,
    root: Query<Entity, With<GameUiRoot>>,
    mut spawned: ResMut<FpCrosshairSpawned>,
) {
    if spawned.0 || layout.width < 100.0 {
        return;
    }
    let Ok(root) = root.get_single() else {
        return;
    };
    spawned.0 = true;

    let cx = layout.width * 0.5;
    let cy = layout.height * 0.5;

    commands.entity(root).with_children(|parent| {
        parent
            .spawn((
                Node {
                    position_type: PositionType::Absolute,
                    left: Val::Px(cx - 1.0),
                    top: Val::Px(cy - 1.0),
                    width: Val::Px(0.0),
                    height: Val::Px(0.0),
                    ..default()
                },
                FocusPolicy::Pass,
                FpCrosshairRoot,
                Name::new("FpCrosshair"),
            ))
            .with_children(|cross| {
                cross.spawn((
                    Node {
                        position_type: PositionType::Absolute,
                        left: Val::Px(-7.0),
                        top: Val::Px(-1.0),
                        width: Val::Px(14.0),
                        height: Val::Px(2.0),
                        ..default()
                    },
                    FocusPolicy::Pass,
                    BackgroundColor(ui_theme::crosshair_color(false)),
                    FpCrosshairH,
                ));
                cross.spawn((
                    Node {
                        position_type: PositionType::Absolute,
                        left: Val::Px(-1.0),
                        top: Val::Px(-7.0),
                        width: Val::Px(2.0),
                        height: Val::Px(14.0),
                        ..default()
                    },
                    FocusPolicy::Pass,
                    BackgroundColor(ui_theme::crosshair_color(false)),
                    FpCrosshairV,
                ));
            });
    });
}

pub fn sync_fp_crosshair(
    layout: Res<TouchLayout>,
    mut cross: Query<&mut Node, With<FpCrosshairRoot>>,
) {
    if layout.width < 100.0 {
        return;
    }
    if let Ok(mut node) = cross.get_single_mut() {
        let cx = layout.width * 0.5;
        let cy = layout.height * 0.5;
        node.left = Val::Px(cx - 1.0);
        node.top = Val::Px(cy - 1.0);
    }
}

pub fn update_fp_crosshair(
    target: Res<LookTarget>,
    mut cross: Query<&mut BackgroundColor, Or<(With<FpCrosshairH>, With<FpCrosshairV>)>>,
) {
    let has_target = target.entity.is_some();
    let color = ui_theme::crosshair_color(has_target);
    for mut bg in &mut cross {
        *bg = BackgroundColor(color);
    }
}
