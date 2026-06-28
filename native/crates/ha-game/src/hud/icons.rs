//! HUD stat icons — consume `IconFactory` + catalog (no hardcoded patterns here).

use bevy::prelude::*;
use bevy::ui::widget::ImageNode;

use crate::ui::{HudIconHandles, IconFactory};

pub struct HudIconPlugin;

impl Plugin for HudIconPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<HudIconAtlas>()
            .add_systems(Startup, ensure_hud_icon_atlas);
    }
}

#[derive(Resource, Default, Clone)]
pub struct HudIconAtlas {
    pub ready: bool,
    pub handles: Option<HudIconHandles>,
}

#[derive(Component)]
pub struct StatHeartIcon {
    pub idx: i32,
}

#[derive(Component)]
pub struct StatEnergyIcon {
    pub idx: i32,
}

fn ensure_hud_icon_atlas(
    mut images: ResMut<Assets<Image>>,
    mut factory: ResMut<IconFactory>,
    mut atlas: ResMut<HudIconAtlas>,
) {
    if atlas.ready {
        return;
    }
    atlas.handles = Some(factory.preload_hud_atlas(&mut images));
    atlas.ready = true;
}

fn handles(atlas: &HudIconAtlas) -> &HudIconHandles {
    atlas.handles.as_ref().expect("HudIconAtlas not ready")
}

pub fn spawn_stat_heart(
    parent: &mut ChildBuilder,
    idx: i32,
    size: f32,
    atlas: &HudIconAtlas,
) {
    parent.spawn((
        Node {
            width: Val::Px(size),
            height: Val::Px(size),
            ..default()
        },
        ImageNode::new(handles(atlas).heart_full.clone()),
        StatHeartIcon { idx },
        Name::new("StatHeart"),
    ));
}

pub fn spawn_stat_energy(
    parent: &mut ChildBuilder,
    idx: i32,
    size: f32,
    atlas: &HudIconAtlas,
) {
    parent.spawn((
        Node {
            width: Val::Px(size),
            height: Val::Px(size),
            ..default()
        },
        ImageNode::new(handles(atlas).drumstick_full.clone()),
        StatEnergyIcon { idx },
        Name::new("StatEnergy"),
    ));
}

pub fn heart_image_for_units(units: i32, atlas: &HudIconAtlas) -> Handle<Image> {
    let h = handles(atlas);
    match units {
        0 => h.heart_empty.clone(),
        1 => h.heart_half.clone(),
        _ => h.heart_full.clone(),
    }
}

pub fn energy_image_for_units(units: i32, atlas: &HudIconAtlas) -> Handle<Image> {
    let h = handles(atlas);
    match units {
        0 => h.drumstick_empty.clone(),
        1 => h.drumstick_half.clone(),
        _ => h.drumstick_full.clone(),
    }
}

pub fn energy_image_for(filled: bool, atlas: &HudIconAtlas) -> Handle<Image> {
    energy_image_for_units(if filled { 2 } else { 0 }, atlas)
}
