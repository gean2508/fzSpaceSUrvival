//! **Los textos del mundo miden lo mismo en pantalla, pase lo que pase con la
//! cámara.**
//!
//! Las burbujas de los NPC, el cartel de lo que tocás y los globitos de la granja
//! son `Text2d` en el MUNDO: se escalaban una sola vez a "un metro de letra". Con
//! la cámara a pie (30 m de alto) eso da ~20 px, pero la cámara se mueve: al
//! acercarte con el pellizco o al pasar volando, el metro de letra se volvía media
//! pantalla y las frases cruzaban la vista enormes (lo reportó el usuario el 28 sep
//! 2026). El cartel del objetivo ni siquiera tenía escala: 15 *metros* de letra.
//!
//! Ahora cada texto de mundo declara cuántos PÍXELES de pantalla quiere medir
//! ([`TextoEnMundo`]) y este sistema le recalcula la escala cada cuadro contra el
//! encuadre vivo de la cámara. Se sigue anclando a su dueño en el mundo; lo que no
//! cambia es su tamaño en pantalla.

use bevy::prelude::*;
use bevy::window::PrimaryWindow;

/// Un `Text2d` del mundo que debe medir `px` píxeles de alto de letra en pantalla.
/// `font_size` es el del `TextFont` (la rasterización); la escala hace el resto.
#[derive(Component, Clone, Copy)]
pub struct TextoEnMundo {
    pub px: f32,
    pub font_size: f32,
}

pub struct TextosMundoPlugin;

impl Plugin for TextosMundoPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            PostUpdate,
            escalar.before(bevy::transform::TransformSystem::TransformPropagate),
        );
    }
}

/// Metros de mundo por píxel de pantalla, con el encuadre que se ve este cuadro.
pub fn metros_por_px(view_h: f32, alto_ventana: f32) -> f32 {
    view_h / alto_ventana.max(1.0)
}

/// La escala que hace que un texto rasterizado a `font_size` mida `px` en pantalla.
pub fn escala_para(px: f32, font_size: f32, m_por_px: f32) -> f32 {
    px * m_por_px / font_size.max(1.0)
}

fn escalar(
    zoom: Option<Res<crate::camera::CameraZoom>>,
    ventana: Query<&Window, With<PrimaryWindow>>,
    mut textos: Query<(&TextoEnMundo, &mut Transform)>,
) {
    let Some(zoom) = zoom else { return };
    let Ok(win) = ventana.get_single() else { return };
    let m = metros_por_px(zoom.live_view_h, win.height());
    for (t, mut tf) in &mut textos {
        let s = escala_para(t.px, t.font_size, m);
        if (tf.scale.x - s).abs() > s * 1e-3 {
            tf.scale = Vec3::new(s, s, 1.0);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A pie (30 m sobre 612 px) una letra de 18 px mide ~0,9 m, que es lo que
    /// medía la burbuja de siempre; acercando la cámara al doble, la letra en
    /// METROS se achica a la mitad para seguir midiendo 18 px.
    #[test]
    fn el_texto_mide_lo_mismo_en_pantalla_con_cualquier_zoom() {
        let a_pie = escala_para(18.0, 22.0, metros_por_px(30.0, 612.0));
        assert!((a_pie * 22.0 - 0.88).abs() < 0.05, "{}", a_pie * 22.0);
        let cerca = escala_para(18.0, 22.0, metros_por_px(15.0, 612.0));
        assert!((cerca * 2.0 - a_pie).abs() < 1e-6);
        // Y en pantalla: metros de letra / metros por px = 18 px en los dos.
        for vh in [8.0, 30.0, 400.0] {
            let m = metros_por_px(vh, 612.0);
            let px = escala_para(18.0, 22.0, m) * 22.0 / m;
            assert!((px - 18.0).abs() < 1e-3);
        }
    }
}
