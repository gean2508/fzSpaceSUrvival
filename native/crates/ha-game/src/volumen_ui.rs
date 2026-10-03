//! Sliders de volumen — **Efectos** y **Música** — para los dos paneles de
//! AJUSTES (el del menú principal y el del menú de juego).
//!
//! Efectos es TODO lo que no es música: golpes a animales y enemigos, talar,
//! picar, pasos, la lluvia, el aura del Ki, los clics. Música son las pistas del
//! director (`audio::music_director`). Llevar un slider a la izquierda del todo
//! lo silencia; el botón de la derecha silencia/devuelve el volumen de un toque.
//!
//! La verdad es una sola: `audio::AudioSettings`. El slider la escribe, y
//! `audio` la aplica en vivo (también a lo que YA está sonando) y la guarda en
//! disco, así que el volumen elegido sobrevive a cerrar el juego.

use bevy::prelude::*;
use bevy::ui::{FocusPolicy, RelativeCursorPosition};

use crate::audio::AudioSettings;

/// Qué volumen mueve un slider.
#[derive(Component, Clone, Copy, PartialEq, Eq, Debug)]
pub enum CanalVolumen {
    Efectos,
    Musica,
}

impl CanalVolumen {
    fn nombre(self) -> &'static str {
        match self {
            CanalVolumen::Efectos => "Efectos",
            CanalVolumen::Musica => "Música",
        }
    }

    fn leer(self, s: &AudioSettings) -> f32 {
        match self {
            CanalVolumen::Efectos => s.sfx,
            CanalVolumen::Musica => s.music,
        }
    }

    fn escribir(self, s: &mut AudioSettings, v: f32) {
        let v = v.clamp(0.0, 1.0);
        match self {
            CanalVolumen::Efectos => s.sfx = v,
            CanalVolumen::Musica => s.music = v,
        }
    }
}

/// La pista que se arrastra.
#[derive(Component)]
struct SliderPista(CanalVolumen);

/// El tramo lleno (ancho = volumen).
#[derive(Component)]
struct SliderRelleno(CanalVolumen);

/// La perilla.
#[derive(Component)]
struct SliderPerilla(CanalVolumen);

/// El "80 %" / "Silencio".
#[derive(Component)]
struct SliderValor(CanalVolumen);

/// El botón de silenciar; recuerda el volumen que había para devolverlo.
#[derive(Component)]
struct SliderMudo {
    canal: CanalVolumen,
    antes: f32,
}

/// Texto del botón de silenciar.
#[derive(Component)]
struct SliderMudoTexto(CanalVolumen);

/// Volumen al que vuelve "Activar" si se silenció arrastrando (no hay "antes").
const VOLUMEN_AL_REACTIVAR: f32 = 0.6;

/// Paso del slider: 5 %. Más fino no se oye; así el número se lee redondo.
const PASO: f32 = 0.05;

pub struct VolumenUiPlugin;

impl Plugin for VolumenUiPlugin {
    fn build(&self, app: &mut App) {
        // Sin `run_if(Playing)`: el panel del menú principal también los usa.
        app.add_systems(Update, (arrastrar_sliders, boton_silenciar, pintar_sliders).chain());
    }
}

/// Posición del dedo sobre la pista (0..1, puede salirse) → volumen con paso.
pub fn volumen_desde_posicion(x: f32) -> f32 {
    ((x.clamp(0.0, 1.0) / PASO).round() * PASO).clamp(0.0, 1.0)
}

/// Texto del valor: "Silencio" en cero, si no el porcentaje.
pub fn texto_volumen(v: f32) -> String {
    if v <= 0.001 {
        "Silencio".to_string()
    } else {
        format!("{:.0} %", v * 100.0)
    }
}

/// Fila completa: nombre + valor arriba, pista + botón de silenciar abajo.
///
/// `ancho` es el ancho total de la fila en px lógicos, `fuente` el tamaño del
/// texto y `acento` el color del panel que la aloja.
pub fn spawn_slider_volumen(parent: &mut ChildBuilder, canal: CanalVolumen, ancho: f32, fuente: f32, acento: Color) {
    let alto_pista = (fuente * 1.9).max(22.0);
    let ancho_mudo = (fuente * 6.2).max(56.0);
    parent
        .spawn((
            Node {
                width: Val::Px(ancho),
                flex_direction: FlexDirection::Column,
                row_gap: Val::Px(3.0),
                ..default()
            },
            Name::new(format!("Volumen_{}", canal.nombre())),
        ))
        .with_children(|col| {
            col.spawn(Node {
                width: Val::Percent(100.0),
                justify_content: JustifyContent::SpaceBetween,
                ..default()
            })
            .with_children(|fila| {
                fila.spawn((
                    Text::new(canal.nombre()),
                    TextFont { font_size: fuente, ..default() },
                    TextColor(acento),
                ));
                fila.spawn((
                    Text::new(""),
                    TextFont { font_size: fuente, ..default() },
                    TextColor(Color::srgb(0.92, 0.92, 0.95)),
                    SliderValor(canal),
                ));
            });
            col.spawn(Node {
                width: Val::Percent(100.0),
                column_gap: Val::Px(6.0),
                align_items: AlignItems::Center,
                ..default()
            })
            .with_children(|fila| {
                // La pista entera es el área táctil (alta, para el pulgar); la
                // barra visible va centrada adentro.
                fila.spawn((
                    Button,
                    Node {
                        flex_grow: 1.0,
                        height: Val::Px(alto_pista),
                        justify_content: JustifyContent::Center,
                        flex_direction: FlexDirection::Column,
                        ..default()
                    },
                    BackgroundColor(Color::NONE),
                    RelativeCursorPosition::default(),
                    SliderPista(canal),
                ))
                .with_children(|pista| {
                    pista
                        .spawn((
                            Node {
                                width: Val::Percent(100.0),
                                height: Val::Px((alto_pista * 0.32).max(6.0)),
                                border: UiRect::all(Val::Px(1.0)),
                                ..default()
                            },
                            BorderColor(acento),
                            BackgroundColor(Color::srgb(0.08, 0.10, 0.15)),
                            FocusPolicy::Pass,
                        ))
                        .with_children(|barra| {
                            barra.spawn((
                                Node {
                                    width: Val::Percent(0.0),
                                    height: Val::Percent(100.0),
                                    ..default()
                                },
                                BackgroundColor(acento),
                                FocusPolicy::Pass,
                                SliderRelleno(canal),
                            ));
                        });
                    // La perilla: absoluta sobre la pista, centrada en el valor.
                    let lado = (alto_pista * 0.8).max(14.0);
                    pista.spawn((
                        Node {
                            position_type: PositionType::Absolute,
                            width: Val::Px(lado),
                            height: Val::Px(lado),
                            top: Val::Px((alto_pista - lado) * 0.5),
                            left: Val::Percent(0.0),
                            margin: UiRect::left(Val::Px(-lado * 0.5)),
                            border: UiRect::all(Val::Px(2.0)),
                            ..default()
                        },
                        BorderColor(acento),
                        BackgroundColor(Color::srgb(0.92, 0.94, 0.98)),
                        BorderRadius::all(Val::Px(lado * 0.5)),
                        FocusPolicy::Pass,
                        SliderPerilla(canal),
                    ));
                });
                fila.spawn((
                    Button,
                    Node {
                        width: Val::Px(ancho_mudo),
                        height: Val::Px(alto_pista),
                        border: UiRect::all(Val::Px(1.5)),
                        justify_content: JustifyContent::Center,
                        align_items: AlignItems::Center,
                        ..default()
                    },
                    BorderColor(acento),
                    BackgroundColor(Color::srgb(0.06, 0.08, 0.12)),
                    SliderMudo { canal, antes: VOLUMEN_AL_REACTIVAR },
                ))
                .with_children(|b| {
                    b.spawn((
                        Text::new("Silenciar"),
                        TextFont { font_size: (fuente * 0.85).max(8.0), ..default() },
                        TextColor(acento),
                        SliderMudoTexto(canal),
                    ));
                });
            });
        });
}

/// Mientras el dedo (o el mouse) aprieta la pista, el volumen sigue a la
/// posición — aunque se salga de la pista: `Interaction::Pressed` dura hasta
/// soltar, y fuera de rango se recorta a 0 o 100 %.
fn arrastrar_sliders(
    pistas: Query<(&Interaction, &RelativeCursorPosition, &SliderPista)>,
    mut ajustes: ResMut<AudioSettings>,
) {
    for (interaccion, cursor, pista) in &pistas {
        if *interaccion != Interaction::Pressed {
            continue;
        }
        let Some(pos) = cursor.normalized else { continue };
        let v = volumen_desde_posicion(pos.x);
        if (pista.0.leer(&ajustes) - v).abs() > 1e-4 {
            pista.0.escribir(&mut ajustes, v);
        }
    }
}

fn boton_silenciar(
    mut botones: Query<(&Interaction, &mut SliderMudo), Changed<Interaction>>,
    mut ajustes: ResMut<AudioSettings>,
) {
    for (interaccion, mut mudo) in &mut botones {
        if *interaccion != Interaction::Pressed {
            continue;
        }
        let actual = mudo.canal.leer(&ajustes);
        if actual > 0.001 {
            mudo.antes = actual;
            mudo.canal.escribir(&mut ajustes, 0.0);
        } else {
            let v = if mudo.antes > 0.001 { mudo.antes } else { VOLUMEN_AL_REACTIVAR };
            mudo.canal.escribir(&mut ajustes, v);
        }
    }
}

/// Refleja `AudioSettings` en los sliders: al abrir el panel (recién creado) y
/// cada vez que el volumen cambia.
#[allow(clippy::type_complexity)]
fn pintar_sliders(
    ajustes: Res<AudioSettings>,
    nuevos: Query<(), Added<SliderPista>>,
    mut rellenos: Query<(&SliderRelleno, &mut Node), Without<SliderPerilla>>,
    mut perillas: Query<(&SliderPerilla, &mut Node), Without<SliderRelleno>>,
    mut valores: Query<(&SliderValor, &mut Text), Without<SliderMudoTexto>>,
    mut mudos: Query<(&SliderMudoTexto, &mut Text), Without<SliderValor>>,
) {
    if !ajustes.is_changed() && nuevos.is_empty() {
        return;
    }
    for (r, mut node) in &mut rellenos {
        node.width = Val::Percent(r.0.leer(&ajustes) * 100.0);
    }
    for (p, mut node) in &mut perillas {
        node.left = Val::Percent(p.0.leer(&ajustes) * 100.0);
    }
    for (v, mut text) in &mut valores {
        let linea = texto_volumen(v.0.leer(&ajustes));
        if text.0 != linea {
            text.0 = linea;
        }
    }
    for (m, mut text) in &mut mudos {
        let linea = if m.0.leer(&ajustes) > 0.001 { "Silenciar" } else { "Activar" };
        if text.0 != linea {
            text.0 = linea.to_string();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn el_slider_va_de_silencio_a_tope_con_paso_de_cinco() {
        assert_eq!(volumen_desde_posicion(-0.4), 0.0, "a la izquierda de la pista = silencio");
        assert_eq!(volumen_desde_posicion(1.7), 1.0, "a la derecha = tope");
        assert!((volumen_desde_posicion(0.52) - 0.5).abs() < 1e-6);
        assert!((volumen_desde_posicion(0.53) - 0.55).abs() < 1e-6);
    }

    #[test]
    fn cero_se_lee_silencio() {
        assert_eq!(texto_volumen(0.0), "Silencio");
        assert_eq!(texto_volumen(0.8), "80 %");
    }

    #[test]
    fn cada_canal_toca_solo_su_volumen() {
        let mut s = AudioSettings { sfx: 0.8, music: 0.5 };
        CanalVolumen::Musica.escribir(&mut s, 0.0);
        assert_eq!((s.sfx, s.music), (0.8, 0.0), "silenciar la música no apaga los efectos");
        CanalVolumen::Efectos.escribir(&mut s, 2.0);
        assert_eq!(s.sfx, 1.0, "se recorta a 0..1");
    }
}
