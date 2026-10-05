//! **Stick derecho de puntería** — sólo existe con un arma de fuego en la mano.
//!
//! Receta de twin-stick (Brotato / Soul Knight / Call of Duty Mobile), con las perillas medidas para el pulgar:
//! * Pulgar fuera de la zona muerta → el personaje **apunta** hacia donde apunta el stick (sin disparar).
//! * Pulgar pasado el *umbral de disparo* → **dispara**, sin botón aparte. Tiene histéresis (se prende al 62 %,
//!   se apaga al 45 %) para que una ráfaga no parpadee cuando el pulgar tiembla en el borde.
//! * Pulgar de vuelta al centro (o suelto) → deja de disparar. Con el pulgar apoyado cerca del centro, el rumbo
//!   se **conserva**: el personaje sigue apuntando al último lado, que es lo que se pidió ("solo mantiene
//!   apuntando"). Al soltar, vuelve al control normal.
//! Sin armas en la mano el stick no se dibuja y la mitad derecha sigue siendo mundo tocable (`world_touch`).
//! La matemática (zona muerta radial escalada) es la de `apuntado::zona_muerta_radial`.

use bevy::input::touch::{TouchInput, TouchPhase};
use bevy::prelude::*;

use super::layout::TouchLayout;
use crate::game_ui::UiGameplayBlock;
use crate::input::EquippedItem;
use crate::ui_editor::UiEditorState;

/// Zona muerta del stick (fracción del radio): el temblor del pulgar apoyado no gira al personaje.
const ZONA_MUERTA: f32 = 0.18;
/// A partir de acá se dispara (histéresis: se prende arriba, se apaga abajo).
const DISPARA_ON: f32 = 0.62;
const DISPARA_OFF: f32 = 0.45;
/// El dedo puede arrancar un poco fuera del aro y igual lo toma (los pulgares no son precisos).
const MARGEN_TOQUE: f32 = 1.45;

/// Estado público del stick de puntería. `armas.rs` lo lee para apuntar y disparar.
#[derive(Resource, Default, Clone, Copy, Debug)]
pub struct AimStick {
    /// Hay un arma de fuego en la mano: el stick se dibuja y reclama toques.
    pub armado: bool,
    /// Pulgar apoyado.
    pub activo: bool,
    /// Último rumbo apuntado en el mundo `(x, z)`, unitario. Se conserva al volver al centro.
    pub dir: Vec2,
    /// Hay un rumbo válido en `dir`.
    pub tiene_dir: bool,
    /// Gatillo: el pulgar está lejos del centro.
    pub dispara: bool,
    pub(super) pointer: Option<u64>,
    pub(super) centro: Vec2,
    pub(super) radio: f32,
    pub(super) actual: Vec2,
}

impl AimStick {
    /// ¿El píxel cae sobre el stick? `world_touch` lo usa para no tomar ese dedo como mundo.
    pub fn cubre(&self, pos: Vec2) -> bool {
        self.armado && pos.distance(self.centro) <= self.radio * MARGEN_TOQUE
    }

    /// Fracción 0..1 de cuánto se desplazó el pulgar (para dibujar el pomo).
    pub fn desplazamiento(&self) -> Vec2 {
        if !self.activo {
            return Vec2::ZERO;
        }
        let d = self.actual - self.centro;
        if d.length() > self.radio { d.normalize() * self.radio } else { d }
    }
}

/// Núcleo puro y probable: del desplazamiento del pulgar (px, y hacia abajo) al rumbo/gatillo.
/// Devuelve `(rumbo_nuevo_si_hay, magnitud_0_1, dispara)`.
pub fn leer_stick(delta: Vec2, radio: f32, dispara_antes: bool) -> (Option<Vec2>, f32, bool) {
    let n = (delta / radio.max(1.0)).clamp_length_max(1.0);
    let mag = n.length();
    let escalado = crate::apuntado::zona_muerta_radial(n, ZONA_MUERTA);
    // Pantalla: +y = abajo = +z del mundo, así que el rumbo es el delta tal cual.
    let rumbo = (escalado != Vec2::ZERO).then(|| escalado.normalize());
    let dispara = if dispara_antes { mag > DISPARA_OFF } else { mag > DISPARA_ON };
    (rumbo, mag, dispara)
}

pub(super) fn touch_aim_stick(
    layout: Res<TouchLayout>,
    equipped: Res<EquippedItem>,
    editor: Res<UiEditorState>,
    gameplay_block: Res<UiGameplayBlock>,
    mut touches: EventReader<TouchInput>,
    mut stick: ResMut<AimStick>,
) {
    if !crate::platform::uses_touch_gameplay_ui() {
        return;
    }
    let armado = crate::armas::es_arma(&equipped.item_id);
    stick.armado = armado;
    stick.centro = layout.aim_anchor;
    stick.radio = (layout.radius * 0.95).max(1.0);
    let bloqueado = editor.enabled || editor.menu_open || gameplay_block.blocked;
    if (!armado || bloqueado) && stick.pointer.is_some() {
        soltar(&mut stick);
    }
    if !armado || bloqueado || crate::flags::driver_active() {
        // Los eventos se dejan para el resto de sistemas (cada lector tiene su cursor).
        return;
    }
    for ev in touches.read() {
        match ev.phase {
            TouchPhase::Started => {
                if stick.pointer.is_none() && stick.cubre(ev.position) {
                    stick.pointer = Some(ev.id);
                    stick.activo = true;
                    stick.actual = ev.position;
                }
            }
            TouchPhase::Moved => {
                if stick.pointer == Some(ev.id) {
                    stick.actual = ev.position;
                }
            }
            TouchPhase::Ended | TouchPhase::Canceled => {
                if stick.pointer == Some(ev.id) {
                    soltar(&mut stick);
                }
            }
        }
    }
    if stick.activo {
        let (rumbo, _mag, dispara) = leer_stick(stick.actual - stick.centro, stick.radio, stick.dispara);
        if let Some(r) = rumbo {
            stick.dir = r;
            stick.tiene_dir = true;
        }
        stick.dispara = dispara;
    }
}

fn soltar(stick: &mut AimStick) {
    stick.pointer = None;
    stick.activo = false;
    stick.dispara = false;
    stick.actual = stick.centro;
}

#[derive(Component)]
pub(super) struct AimBase;
#[derive(Component)]
pub(super) struct AimKnob;

pub(super) fn spawn_aim_stick_ui(mut commands: Commands, art: Res<crate::art2d::ArtLibrary>) {
    if !crate::platform::uses_touch_gameplay_ui() {
        return;
    }
    let base_img = art.ui_image_node("ui/joystick_base");
    let knob_img = art.ui_image_node("ui/joystick_knob");
    let con_arte = base_img.is_some();
    let mut base = commands.spawn((
        Node {
            position_type: PositionType::Absolute,
            display: Display::None,
            border: UiRect::all(Val::Px(if con_arte { 0.0 } else { 3.2 })),
            ..default()
        },
        BorderColor(if con_arte { Color::NONE } else { crate::ui_theme::joystick_border(false) }),
        BackgroundColor(if con_arte { Color::NONE } else { crate::ui_theme::joystick_base() }),
        BorderRadius::MAX,
        AimBase,
        crate::game_ui::GameplayUi,
        Name::new("StickApuntar"),
    ));
    if let Some(img) = base_img {
        base.insert(img.with_color(Color::srgba(1.0, 0.8, 0.75, 0.6)));
    }
    base.with_children(|p| {
        let mut knob = p.spawn((Node { position_type: PositionType::Absolute, ..default() }, BorderRadius::MAX, AimKnob));
        match knob_img {
            Some(img) => {
                knob.insert((img, BackgroundColor(Color::NONE)));
            }
            None => {
                knob.insert(BackgroundColor(crate::ui_theme::joystick_knob()));
            }
        }
    });
}

pub(super) fn update_aim_stick_ui(
    layout: Res<TouchLayout>,
    stick: Res<AimStick>,
    mut bases: Query<(&mut Node, Option<&mut ImageNode>), (With<AimBase>, Without<AimKnob>)>,
    mut knobs: Query<&mut Node, (With<AimKnob>, Without<AimBase>)>,
) {
    if !crate::platform::uses_touch_gameplay_ui() {
        return;
    }
    for (mut node, img) in &mut bases {
        let want = if stick.armado { Display::Flex } else { Display::None };
        let n = node.bypass_change_detection();
        let mut dirty = n.display != want;
        n.display = want;
        let (left, bottom) = super::layout::base_ui_pos(stick.centro, stick.radio, layout.height);
        dirty |= crate::input::set_px(&mut n.left, left);
        dirty |= crate::input::set_px(&mut n.bottom, bottom);
        dirty |= crate::input::set_px(&mut n.width, stick.radio * 2.0);
        dirty |= crate::input::set_px(&mut n.height, stick.radio * 2.0);
        if dirty {
            node.set_changed();
        }
        if let Some(mut img) = img {
            // Se enciende rojizo cuando dispara: el pulgar sabe sin mirar que ya está tirando.
            let a = if stick.activo { 0.95 } else { 0.6 };
            let want = if stick.dispara { Color::srgba(1.0, 0.45, 0.4, a) } else { Color::srgba(1.0, 0.85, 0.8, a) };
            if img.color != want {
                img.color = want;
            }
        }
    }
    let knob_r = (layout.knob_radius * 1.1).max(8.0);
    for mut node in &mut knobs {
        let (left, bottom) = super::layout::knob_offset_in_base(stick.desplazamiento(), stick.radio, knob_r);
        let n = node.bypass_change_detection();
        let mut dirty = crate::input::set_px(&mut n.width, knob_r * 2.0);
        dirty |= crate::input::set_px(&mut n.height, knob_r * 2.0);
        dirty |= crate::input::set_px(&mut n.left, left);
        dirty |= crate::input::set_px(&mut n.bottom, bottom);
        if dirty {
            node.set_changed();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn centro_no_gira_ni_dispara() {
        let (r, m, d) = leer_stick(Vec2::new(3.0, 2.0), 60.0, false);
        assert!(r.is_none() && m < 0.1 && !d);
    }

    #[test]
    fn apunta_sin_disparar_a_media_carrera() {
        let (r, _, d) = leer_stick(Vec2::new(0.0, -60.0 * 0.4), 60.0, false);
        assert_eq!(r.map(|v| v.round()), Some(Vec2::new(0.0, -1.0)));
        assert!(!d, "40 % apunta, no dispara");
    }

    #[test]
    fn lejos_dispara_y_la_histeresis_no_parpadea() {
        let (_, _, d) = leer_stick(Vec2::new(60.0 * 0.7, 0.0), 60.0, false);
        assert!(d);
        // Bajó a 50 %: sigue disparando (apaga recién bajo 45 %).
        assert!(leer_stick(Vec2::new(60.0 * 0.5, 0.0), 60.0, true).2);
        assert!(!leer_stick(Vec2::new(60.0 * 0.4, 0.0), 60.0, true).2);
        // Desde apagado, 50 % no prende (prende sobre 62 %).
        assert!(!leer_stick(Vec2::new(60.0 * 0.5, 0.0), 60.0, false).2);
    }

    #[test]
    fn el_rumbo_sigue_la_pantalla_abajo_es_mas_z() {
        let (r, ..) = leer_stick(Vec2::new(0.0, 50.0), 60.0, false);
        assert_eq!(r.map(|v| v.round()), Some(Vec2::new(0.0, 1.0)));
    }
}
