//! Slim survival bars under the heart row — hunger / thirst / oxygen.
//! Oxygen only shows when it matters (in vacuum or not full).

use bevy::prelude::*;
use ha_shared::{HUNGER_MAX, OXYGEN_MAX, THIRST_MAX};

use crate::sim::{Player, SimState};
use crate::ui_theme;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum SurvivalBarKind {
    Hunger,
    Thirst,
    Oxygen,
    /// Ki reserve — only shown while flying/charging (Saiyan flight).
    Ki,
}

#[derive(Component)]
pub struct SurvivalBarRow(pub SurvivalBarKind);

#[derive(Component)]
pub struct SurvivalBarFill(pub SurvivalBarKind);

#[derive(Resource, Default)]
pub struct SurvivalBarsCache {
    hunger_pct: f32,
    thirst_pct: f32,
    oxygen_pct: f32,
    ki_pct: f32,
    oxygen_visible: bool,
    ki_visible: bool,
    initialized: bool,
}

fn bar_color(kind: SurvivalBarKind) -> Color {
    match kind {
        SurvivalBarKind::Hunger => Color::srgb(0.94, 0.63, 0.25), // amber — food
        SurvivalBarKind::Thirst => Color::srgb(0.30, 0.65, 0.95), // water blue
        SurvivalBarKind::Oxygen => Color::srgb(0.78, 0.93, 0.98), // suit white-cyan
        SurvivalBarKind::Ki => Color::srgb(1.0, 0.82, 0.22),      // Saiyan gold
    }
}

/// Spawned inside the HudTopLeft column, right below the heart row.
pub fn spawn_survival_bars(parent: &mut ChildBuilder, heart: f32, gap: f32) {
    let width = heart * 6.0 + gap * 5.0;
    let height = (heart * 0.28).clamp(4.0, 7.0);
    for kind in [
        SurvivalBarKind::Hunger,
        SurvivalBarKind::Thirst,
        SurvivalBarKind::Oxygen,
        SurvivalBarKind::Ki,
    ] {
        parent
            .spawn((
                Node {
                    width: Val::Px(width),
                    height: Val::Px(height),
                    border: UiRect::all(Val::Px(1.0)),
                    ..default()
                },
                BorderColor(ui_theme::accent_ink().with_alpha(0.65)),
                BackgroundColor(ui_theme::panel_bg(0.72)),
                SurvivalBarRow(kind),
                Name::new(match kind {
                    SurvivalBarKind::Hunger => "HungerBar",
                    SurvivalBarKind::Thirst => "ThirstBar",
                    SurvivalBarKind::Oxygen => "OxygenBar",
                    SurvivalBarKind::Ki => "KiBar",
                }),
            ))
            .with_children(|bar| {
                bar.spawn((
                    Node {
                        width: Val::Percent(100.0),
                        height: Val::Percent(100.0),
                        ..default()
                    },
                    BackgroundColor(bar_color(kind)),
                    SurvivalBarFill(kind),
                ));
            });
    }
}

pub fn update_survival_bars(
    player: Query<&SimState, With<Player>>,
    world_ctx: Res<crate::scene::WorldContextRuntime>,
    flight: Res<crate::flight::Flight>,
    mut cache: ResMut<SurvivalBarsCache>,
    mut fills: Query<(&SurvivalBarFill, &mut Node), Without<SurvivalBarRow>>,
    mut rows: Query<(&SurvivalBarRow, &mut Node), Without<SurvivalBarFill>>,
) {
    let Ok(sim) = player.get_single() else {
        return;
    };
    let hunger_pct = (sim.survival.hunger / HUNGER_MAX * 100.0).clamp(0.0, 100.0);
    let thirst_pct = (sim.survival.thirst / THIRST_MAX * 100.0).clamp(0.0, 100.0);
    let oxygen_pct = (sim.survival.oxygen / OXYGEN_MAX * 100.0).clamp(0.0, 100.0);
    let ki_pct = flight.ki_pct();
    // O₂ row earns its pixels only when the suit is in play.
    let oxygen_visible = !matches!(
        world_ctx.active,
        ha_shared::WorldContext::Overworld | ha_shared::WorldContext::Mission
    ) || oxygen_pct < 99.5;
    // Ki row shows while flying/charging, or when not fully recovered.
    let ki_visible = flight.active || flight.charging || ki_pct < 99.5;

    let changed = !cache.initialized
        || (cache.hunger_pct - hunger_pct).abs() > 0.4
        || (cache.thirst_pct - thirst_pct).abs() > 0.4
        || (cache.oxygen_pct - oxygen_pct).abs() > 0.4
        || (cache.ki_pct - ki_pct).abs() > 0.4
        || cache.oxygen_visible != oxygen_visible
        || cache.ki_visible != ki_visible;
    if !changed {
        return;
    }
    cache.initialized = true;
    cache.hunger_pct = hunger_pct;
    cache.thirst_pct = thirst_pct;
    cache.oxygen_pct = oxygen_pct;
    cache.ki_pct = ki_pct;
    cache.oxygen_visible = oxygen_visible;
    cache.ki_visible = ki_visible;

    for (fill, mut node) in &mut fills {
        node.width = Val::Percent(match fill.0 {
            SurvivalBarKind::Hunger => hunger_pct,
            SurvivalBarKind::Thirst => thirst_pct,
            SurvivalBarKind::Oxygen => oxygen_pct,
            SurvivalBarKind::Ki => ki_pct,
        });
    }
    for (row, mut node) in &mut rows {
        match row.0 {
            SurvivalBarKind::Oxygen => {
                node.display = if oxygen_visible { Display::Flex } else { Display::None };
            }
            SurvivalBarKind::Ki => {
                node.display = if ki_visible { Display::Flex } else { Display::None };
            }
            _ => {}
        }
    }
}
