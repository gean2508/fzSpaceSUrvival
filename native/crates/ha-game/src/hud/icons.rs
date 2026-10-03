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
    /// Ancho del corazón entero, en px (el medio corazón usa la mitad).
    pub ancho: f32,
}

// `StatEnergyIcon` y su fila de muslos (`drumstick_*`) se borraron el 15 ago
// 2026: la energía se mostraba con seis patas de pollo al lado de una barra de
// hambre, o sea comida dos veces. Ahora es una barra con un rayo, en
// `survival_bars.rs`. Los handles `drumstick_*` siguen en el atlas porque el
// slot `ui/energy_full` es del catálogo y lo puede asignar el editor.

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

/// **El corazón del PACK, no el dibujado por código.**
///
/// El slot `ui/heart_full` existe en el manifest y es el que usa la imagen de
/// referencia; el icono procedural era el respaldo de cuando no había arte
/// asignada, y en la captura se leía como una mancha naranja. Si el slot no
/// tiene arte, sigue cayendo al procedural (regla de los fallbacks).
pub fn spawn_stat_heart(
    parent: &mut ChildBuilder,
    idx: i32,
    size: f32,
    atlas: &HudIconAtlas,
    art: &crate::art2d::ArtLibrary,
) {
    let img = art
        .ui_icon("ui/heart_full")
        .unwrap_or_else(|| ImageNode::new(handles(atlas).heart_full.clone()));
    // **El corazón no es cuadrado.** El recorte del pack mide 64 × 32 con el
    // dibujo en el medio; metido a la fuerza en un cuadrado sale como una gota
    // estirada, que es como se veía en la captura. `size` es el ALTO, y el ancho
    // sale de la proporción del recorte — igual que en la maqueta, donde los
    // corazones se pegan cada 46 px aunque el recorte mida 83.
    let (w, h) = match art.ui_icon_px("ui/heart_full") {
        Some(px) if px.x > 0.0 && px.y > 0.0 => (size * px.x / px.y, size),
        _ => (size, size),
    };
    parent.spawn((
        Node {
            position_type: PositionType::Absolute,
            left: Val::Px(idx as f32 * size * 1.1),
            top: Val::Px(0.0),
            width: Val::Px(w),
            height: Val::Px(h),
            ..default()
        },
        img,
        StatHeartIcon { idx, ancho: w },
        Name::new("StatHeart"),
    ));
}

/// El dibujo que le toca a un corazón según cuánta vida le queda.
pub fn heart_node_for_units(
    units: i32,
    atlas: &HudIconAtlas,
    art: &crate::art2d::ArtLibrary,
) -> ImageNode {
    let clave = match units {
        0 => "ui/heart_empty",
        1 => "ui/heart_half",
        _ => "ui/heart_full",
    };
    art.ui_icon(clave)
        .unwrap_or_else(|| ImageNode::new(heart_image_for_units(units, atlas)))
}

pub fn heart_image_for_units(units: i32, atlas: &HudIconAtlas) -> Handle<Image> {
    let h = handles(atlas);
    match units {
        0 => h.heart_empty.clone(),
        1 => h.heart_half.clone(),
        _ => h.heart_full.clone(),
    }
}

