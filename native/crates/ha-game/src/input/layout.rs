use bevy::prelude::*;
use bevy::window::PrimaryWindow;

use crate::platform::{self, DisplayInsets};

/// Computed touch / HUD layout from real window size + optional display cutout.
#[derive(Resource, Clone, Debug)]
pub struct TouchLayout {
    pub width: f32,
    pub height: f32,
    pub scale_factor: f32,
    pub landscape: bool,
    pub insets: DisplayInsets,
    /// Full logical screen (touch coordinates use this space).
    pub screen_left: f32,
    pub screen_top: f32,
    pub screen_width: f32,
    pub screen_height: f32,
    pub radius: f32,
    pub knob_radius: f32,
    pub move_anchor: Vec2,
    pub aim_anchor: Vec2,
    pub zone_top: f32,
    /// Borde derecho del stick de mover: a partir de acá el toque es MUNDO.
    pub look_min_x: f32,
    pub mid_x: f32,
}

/// UI regions that steal touches from joysticks (action buttons).
#[derive(Resource, Default, Clone)]
pub struct TouchBlockRects(pub Vec<[f32; 4]>);

impl Default for TouchLayout {
    fn default() -> Self {
        Self {
            width: 1280.0,
            height: 720.0,
            scale_factor: 1.0,
            landscape: true,
            insets: DisplayInsets::default(),
            screen_left: 0.0,
            screen_top: 0.0,
            screen_width: 1280.0,
            screen_height: 720.0,
            radius: 56.0,
            knob_radius: 20.0,
            move_anchor: Vec2::new(180.0, 560.0),
            aim_anchor: Vec2::new(1100.0, 560.0),
            zone_top: 324.0,
            look_min_x: 486.0,
            mid_x: 640.0,
        }
    }
}

const BOTTOM_ZONE_FRAC: f32 = 0.46;
const EDGE_PAD_FRAC: f32 = 0.035;
const STICK_Y_FROM_BOTTOM_FRAC: f32 = 0.13;
const RADIUS_FRAC: f32 = 0.132;
const KNOB_RADIUS_FRAC: f32 = 0.044;
/// MC Bedrock: ~38% para el stick de mover; el resto de la pantalla es mundo
/// (división invisible, no se dibuja ninguna línea).
const LOOK_ZONE_FRAC: f32 = 0.38;

pub fn sync_touch_layout(window: Query<&Window, With<PrimaryWindow>>, mut layout: ResMut<TouchLayout>) {
    let Ok(win) = window.get_single() else {
        return;
    };
    let w = win.width();
    let h = win.height();
    if w < 1.0 || h < 1.0 {
        return;
    }

    let scale = win.scale_factor();
    let insets = platform::display_insets(scale);
    let inset_l = insets.left as f32;
    let _inset_t = insets.top as f32;
    let inset_r = insets.right as f32;
    let inset_b = insets.bottom as f32;

    if (layout.width - w).abs() < 0.5
        && (layout.height - h).abs() < 0.5
        && (layout.scale_factor - scale).abs() < 0.01
        && layout.insets.left == insets.left
        && layout.insets.top == insets.top
        && layout.insets.right == insets.right
        && layout.insets.bottom == insets.bottom
    {
        return;
    }

    let landscape = w >= h;
    let short = w.min(h);
    let radius = short * RADIUS_FRAC;
    let knob_radius = short * KNOB_RADIUS_FRAC;

    // Use the full window for touch; insets only nudge default stick anchors inward.
    let pad = short * EDGE_PAD_FRAC;
    let y_from_bottom = h * STICK_Y_FROM_BOTTOM_FRAC + inset_b;
    let anchor_y = h - y_from_bottom;

    let move_x = inset_l + pad + radius;
    let aim_x = w - inset_r - pad - radius;

    layout.width = w;
    layout.height = h;
    layout.scale_factor = scale;
    layout.landscape = landscape;
    layout.insets = insets;
    layout.screen_left = 0.0;
    layout.screen_top = 0.0;
    layout.screen_width = w;
    layout.screen_height = h;
    layout.radius = radius;
    layout.knob_radius = knob_radius;
    layout.move_anchor = Vec2::new(move_x, anchor_y);
    layout.aim_anchor = Vec2::new(aim_x, anchor_y);
    layout.zone_top = h * (1.0 - BOTTOM_ZONE_FRAC);
    layout.look_min_x = inset_l + w * LOOK_ZONE_FRAC;
    layout.mid_x = layout.look_min_x;
}

/// Zona del stick de mover: cuña inferior izquierda. Todo lo demás es mundo
/// tocable (ver `input/world_touch.rs`).
pub fn touch_in_move_zone(pos: Vec2, layout: &TouchLayout) -> bool {
    pos.x < layout.look_min_x && pos.y >= layout.zone_top
}

pub fn point_in_rect(x: f32, y: f32, rect: [f32; 4]) -> bool {
    let [left, top, right, bottom] = rect;
    x >= left && x <= right && y >= top && y <= bottom
}

pub fn touch_blocked(pos: Vec2, rects: &[ [f32; 4] ]) -> bool {
    rects.iter().any(|r| point_in_rect(pos.x, pos.y, *r))
}

/// Convert touch-space Y (top-origin) to UI `bottom` offset for a widget of `height`.
pub fn touch_y_to_ui_bottom(y_top: f32, win_h: f32, height: f32) -> f32 {
    win_h - y_top - height * 0.5
}

/// Position a square base ring centered on `anchor` (touch coords).
pub fn base_ui_pos(anchor: Vec2, radius: f32, win_h: f32) -> (f32, f32) {
    let size = radius * 2.0;
    (
        anchor.x - radius,
        touch_y_to_ui_bottom(anchor.y, win_h, size),
    )
}

/// Knob position **relative to base node** (parent coords, bottom-left origin).
pub fn knob_offset_in_base(delta: Vec2, radius: f32, knob_radius: f32) -> (f32, f32) {
    (
        radius + delta.x - knob_radius,
        radius - delta.y - knob_radius,
    )
}
