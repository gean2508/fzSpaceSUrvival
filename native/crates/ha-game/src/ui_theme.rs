use bevy::prelude::*;
use ha_shared::Ps1Retro;

#[derive(Clone, Copy, Debug)]
pub enum UiButtonTone {
    Primary,
    Use,
    Jump,
    Attack,
    Inventory,
    Craft,
}

#[derive(Clone, Copy, Debug)]
pub enum UiButtonState {
    Normal,
    Pressed,
    Disabled,
}

pub fn boot_void() -> Color {
    rgb(Ps1Retro::VOID)
}

pub fn ink() -> Color {
    rgb(Ps1Retro::PANEL_LO)
}

pub fn ink_outline() -> Color {
    rgb(Ps1Retro::PANEL_LO)
}

pub fn accent_ink() -> Color {
    rgb(Ps1Retro::SELECT)
}

pub fn panel_bevel_hi() -> Color {
    rgb(Ps1Retro::PANEL_HI)
}

pub fn panel_bevel_lo() -> Color {
    rgb(Ps1Retro::PANEL_LO)
}

pub fn panel_bg(alpha: f32) -> Color {
    Color::srgba(Ps1Retro::PANEL[0], Ps1Retro::PANEL[1], Ps1Retro::PANEL[2], alpha)
}

pub fn panel_bg_lit(_glow: f32, alpha: f32) -> Color {
    Color::srgba(
        Ps1Retro::PANEL[0] * 1.08,
        Ps1Retro::PANEL[1] * 1.08,
        Ps1Retro::PANEL[2] * 1.08,
        alpha,
    )
}

pub fn panel_border_width() -> f32 {
    3.0
}

pub fn action_border_width() -> f32 {
    3.5
}

pub fn text_primary() -> Color {
    rgb(Ps1Retro::TEXT)
}

pub fn text_muted() -> Color {
    rgb(Ps1Retro::TEXT_DIM)
}

pub fn text_dim() -> Color {
    rgb(Ps1Retro::TEXT_DIM)
}

pub fn text_warm() -> Color {
    rgb(Ps1Retro::SELECT)
}

pub fn heart_full() -> Color {
    Color::srgb(0.88, 0.28, 0.32)
}

pub fn heart_half() -> Color {
    Color::srgb(0.88, 0.48, 0.52)
}

pub fn heart_empty() -> Color {
    Color::srgba(0.88, 0.28, 0.32, 0.0)
}

pub fn energy_full() -> Color {
    rgb(Ps1Retro::ACCENT)
}

pub fn energy_empty() -> Color {
    Color::srgba(Ps1Retro::PANEL_LO[0], Ps1Retro::PANEL_LO[1], Ps1Retro::PANEL_LO[2], 0.55)
}

pub fn joystick_base() -> Color {
    Color::srgba(Ps1Retro::PANEL_LO[0], Ps1Retro::PANEL_LO[1], Ps1Retro::PANEL_LO[2], 0.82)
}

pub fn joystick_knob() -> Color {
    Color::srgba(Ps1Retro::TEXT[0], Ps1Retro::TEXT[1], Ps1Retro::TEXT[2], 0.95)
}

pub fn joystick_border(active: bool) -> Color {
    if active {
        accent_ink()
    } else {
        panel_bevel_hi()
    }
}

pub fn accent_cyan() -> Color {
    rgb(Ps1Retro::ACCENT)
}

pub fn accent_magenta() -> Color {
    Color::srgb(0.72, 0.48, 0.78)
}

pub fn look_zone_fill(has_target: bool) -> Color {
    if has_target {
        Color::srgba(Ps1Retro::ACCENT[0], Ps1Retro::ACCENT[1], Ps1Retro::ACCENT[2], 0.18)
    } else {
        Color::srgba(Ps1Retro::PANEL_HI[0], Ps1Retro::PANEL_HI[1], Ps1Retro::PANEL_HI[2], 0.08)
    }
}

pub fn look_zone_edge() -> Color {
    Color::srgba(Ps1Retro::PANEL_HI[0], Ps1Retro::PANEL_HI[1], Ps1Retro::PANEL_HI[2], 0.72)
}

pub fn look_zone_label() -> Color {
    text_muted().with_alpha(0.72)
}

pub fn crosshair_color(has_target: bool) -> Color {
    if has_target {
        accent_ink()
    } else {
        text_primary().with_alpha(0.82)
    }
}

pub fn look_zone_reticle(has_target: bool) -> Color {
    crosshair_color(has_target)
}

pub fn button_bg(tone: UiButtonTone, state: UiButtonState) -> Color {
    match (tone, state) {
        (_, UiButtonState::Disabled) => Color::srgba(
            Ps1Retro::PANEL_LO[0],
            Ps1Retro::PANEL_LO[1],
            Ps1Retro::PANEL_LO[2],
            0.65,
        ),
        (_, UiButtonState::Pressed) => Color::srgba(
            Ps1Retro::SELECT[0],
            Ps1Retro::SELECT[1],
            Ps1Retro::SELECT[2],
            0.96,
        ),
        (UiButtonTone::Use, UiButtonState::Normal) => panel_bg(0.94),
        (UiButtonTone::Jump, UiButtonState::Normal) => Color::srgba(
            Ps1Retro::ACCENT[0],
            Ps1Retro::ACCENT[1],
            Ps1Retro::ACCENT[2],
            0.88,
        ),
        (UiButtonTone::Attack, UiButtonState::Normal) => Color::srgba(0.62, 0.42, 0.72, 0.9),
        (UiButtonTone::Inventory, UiButtonState::Normal) => Color::srgba(
            Ps1Retro::PANEL_LO[0],
            Ps1Retro::PANEL_LO[1],
            Ps1Retro::PANEL_LO[2],
            0.92,
        ),
        (UiButtonTone::Craft, UiButtonState::Normal) => panel_bg(0.9),
        (_, UiButtonState::Normal) => panel_bg(0.92),
    }
}

pub fn button_border(tone: UiButtonTone) -> Color {
    let _ = tone;
    panel_bevel_hi()
}

pub fn button_border_pressed(tone: UiButtonTone, interaction: Interaction) -> Color {
    if interaction == Interaction::Pressed {
        accent_ink()
    } else {
        button_border(tone)
    }
}

pub fn interaction_state(interaction: Interaction, disabled: bool) -> UiButtonState {
    if disabled {
        UiButtonState::Disabled
    } else if interaction == Interaction::Pressed {
        UiButtonState::Pressed
    } else {
        UiButtonState::Normal
    }
}

fn rgb(rgb: [f32; 3]) -> Color {
    Color::srgb(rgb[0], rgb[1], rgb[2])
}

/// **Pinta el fondo de un botón sólo si tiene fondo.** Los botones con marco del
/// kit (o reenmarcados por `hud::skin_ui_from_art`) llevan el fondo
/// transparente: la madera es su imagen. Si un handler viejo les escribía su
/// color plano al tocarlos, el rectángulo oscuro asomaba por las esquinas del
/// marco. El realce del toque de esos botones lo hace `ui_pack::realce_botones`.
pub fn pintar_boton(bg: &mut BackgroundColor, color: Color) {
    if bg.0.alpha() > 0.0 {
        bg.0 = color;
    }
}
