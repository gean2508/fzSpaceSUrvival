//! **Ponerle nombre a un animal** — la etiqueta de Minecraft.
//!
//! Con la etiqueta (`name_tag`) en la mano se mantiene sobre un animal y se abre
//! este panel: un renglón y un teclado en pantalla (en la compu también se escribe
//! con el teclado de verdad). LISTO emite [`NombreElegido`], que `fauna.rs` aplica:
//! el nombre queda flotando sobre el animal, persiste con él y lo vuelve TUYO para
//! siempre. CANCELAR no gasta la etiqueta.
//!
//! El teclado es una copia del de la consola (`consola.rs`), por la misma razón
//! que la consola tiene el suyo: cada uno necesita teclas distintas (acá acentos y
//! eñe, allá la barra y los dígitos) y compartirlo tocaba pantallas que andan.

use bevy::input::keyboard::{Key, KeyboardInput};
use bevy::prelude::*;
use bevy::ui::FocusPolicy;

/// Largo máximo del nombre (entra encima de una vaca sin tapar a la de al lado).
pub const MAX_NOMBRE: usize = 16;

const FILAS: [&str; 4] = ["ABCDEFGHIJKLMN", "ÑOPQRSTUVWXYZ", "abcdefghijklmn", "ñopqrstuvwxyzá"];

/// El panel abierto: a quién se bautiza y lo escrito.
#[derive(Resource, Default)]
pub struct Bautizo {
    pub objetivo: Option<Entity>,
    pub texto: String,
}

impl Bautizo {
    pub fn abierto(&self) -> bool {
        self.objetivo.is_some()
    }

    pub fn abrir(&mut self, animal: Entity, actual: Option<&str>) {
        self.objetivo = Some(animal);
        self.texto = actual.unwrap_or("").to_string();
    }

    fn escribir(&mut self, c: char) {
        if self.texto.chars().count() < MAX_NOMBRE {
            self.texto.push(c);
        }
    }
}

/// El jugador confirmó un nombre. Lo consume `fauna.rs`.
#[derive(Event)]
pub struct NombreElegido {
    pub animal: Entity,
    pub nombre: String,
}

/// Limpia lo que se escribió: sin espacios de más y con largo acotado. `None` si
/// quedó vacío (un nombre en blanco no es un nombre).
pub fn nombre_valido(crudo: &str) -> Option<String> {
    let limpio: String = crudo.split_whitespace().collect::<Vec<_>>().join(" ");
    let limpio: String = limpio.chars().take(MAX_NOMBRE).collect();
    (!limpio.is_empty()).then_some(limpio)
}

#[derive(Component)]
struct BautizoRoot;

#[derive(Component)]
struct BautizoLinea;

#[derive(Component, Clone, PartialEq)]
enum Tecla {
    Char(char),
    Borrar,
    Listo,
    Cancelar,
}

pub struct BautizoPlugin;

impl Plugin for BautizoPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Bautizo>()
            .add_event::<NombreElegido>()
            .add_systems(Update, (sincronizar_panel, teclas, teclado_fisico, refrescar).chain());
    }
}

fn sincronizar_panel(
    bautizo: Res<Bautizo>,
    mut commands: Commands,
    touch: Res<crate::input::TouchLayout>,
    art: Res<crate::art2d::ArtLibrary>,
    vivos: Query<Entity, With<BautizoRoot>>,
) {
    let quiere = bautizo.abierto();
    if quiere == !vivos.is_empty() {
        return;
    }
    if !quiere {
        for e in &vivos {
            commands.entity(e).despawn_recursive();
        }
        return;
    }
    let k = crate::ui_pack::k(&touch);
    use crate::ui_pack::{self as up, Ancla, Pack};
    let (pw, ph) = (1500.0_f32, 250.0_f32);
    let (px, py) = ((up::ANCHO - pw) / 2.0, 150.0_f32);
    commands
        .spawn((
            Node {
                position_type: PositionType::Absolute,
                left: Val::Px(0.0),
                top: Val::Px(0.0),
                width: Val::Percent(100.0),
                height: Val::Percent(100.0),
                ..default()
            },
            BackgroundColor(Color::srgba(0.047, 0.039, 0.078, 0.55)),
            GlobalZIndex(1150),
            FocusPolicy::Block,
            BautizoRoot,
            Name::new("Bautizo"),
        ))
        .with_children(|root| {
            root.spawn(up::lienzo(k)).with_children(|l| {
                up::marco(l, &art, up::marco::PANEL, px, py, pw, ph, k);
                up::cartel(l, &art, "PONERLE NOMBRE", px + pw / 2.0, py - 48.0, 620.0, 92.0, 40.0, k);
                let ix = px + 60.0;
                up::marco(l, &art, up::marco::BOTON_ANCHO, ix, py + 60.0, pw - 120.0, 96.0, k);
                up::texto_marcado(
                    l,
                    "|",
                    44.0,
                    Pack::TINTA,
                    ix + 36.0,
                    py + 108.0,
                    Ancla::Izq,
                    k,
                    BautizoLinea,
                );
                up::texto(
                    l,
                    "El nombre queda sobre el animal y lo hace tuyo.",
                    26.0,
                    Pack::SUAVE,
                    px + pw / 2.0,
                    py + ph - 40.0,
                    Ancla::Medio,
                    k,
                );
                let ty0 = py + ph + 30.0;
                let (ancho_t, alto_t, sep) = (104.0_f32, 74.0_f32, 6.0_f32);
                for (f, fila) in FILAS.iter().enumerate() {
                    let n = fila.chars().count() as f32;
                    let total = n * ancho_t + (n - 1.0) * sep;
                    let x0 = (up::ANCHO - total) / 2.0;
                    let y = ty0 + f as f32 * (alto_t + sep);
                    for (i, c) in fila.chars().enumerate() {
                        up::boton(
                            l, &art, x0 + i as f32 * (ancho_t + sep), y, ancho_t, alto_t,
                            Some(&c.to_string()), None, false, 30.0, Tecla::Char(c), k,
                        );
                    }
                }
                let y = ty0 + FILAS.len() as f32 * (alto_t + sep);
                let fila = [
                    ("ESPACIO", Tecla::Char(' '), 300.0_f32, false),
                    ("BORRAR", Tecla::Borrar, 260.0, false),
                    ("CANCELAR", Tecla::Cancelar, 300.0, false),
                    ("LISTO", Tecla::Listo, 260.0, true),
                ];
                let total: f32 = fila.iter().map(|a| a.2).sum::<f32>() + sep * 3.0;
                let mut x = (up::ANCHO - total) / 2.0;
                for (et, accion, w, fuerte) in fila {
                    up::boton(l, &art, x, y, w, alto_t, Some(et), None, fuerte, 28.0, accion, k);
                    x += w + sep;
                }
            });
        });
}

fn confirmar(bautizo: &mut Bautizo, eventos: &mut EventWriter<NombreElegido>) {
    if let (Some(animal), Some(nombre)) = (bautizo.objetivo, nombre_valido(&bautizo.texto)) {
        eventos.send(NombreElegido { animal, nombre });
    }
    bautizo.objetivo = None;
    bautizo.texto.clear();
}

fn teclas(
    mut bautizo: ResMut<Bautizo>,
    botones: Query<(&Interaction, &Tecla), Changed<Interaction>>,
    mut eventos: EventWriter<NombreElegido>,
) {
    for (interaccion, tecla) in &botones {
        if *interaccion != Interaction::Pressed {
            continue;
        }
        match tecla {
            Tecla::Char(c) => bautizo.escribir(*c),
            Tecla::Borrar => {
                bautizo.texto.pop();
            }
            Tecla::Cancelar => {
                bautizo.objetivo = None;
                bautizo.texto.clear();
            }
            Tecla::Listo => confirmar(&mut bautizo, &mut eventos),
        }
    }
}

/// En la compu se escribe con el teclado: Enter confirma, Escape cancela.
fn teclado_fisico(
    mut bautizo: ResMut<Bautizo>,
    mut teclas: EventReader<KeyboardInput>,
    mut eventos: EventWriter<NombreElegido>,
) {
    if !bautizo.abierto() {
        teclas.clear();
        return;
    }
    for ev in teclas.read() {
        if !ev.state.is_pressed() {
            continue;
        }
        match &ev.logical_key {
            Key::Enter => confirmar(&mut bautizo, &mut eventos),
            Key::Escape => {
                bautizo.objetivo = None;
                bautizo.texto.clear();
            }
            Key::Backspace => {
                bautizo.texto.pop();
            }
            Key::Space => bautizo.escribir(' '),
            Key::Character(s) => {
                for c in s.chars().filter(|c| c.is_alphanumeric() || *c == '-' || *c == '\'') {
                    bautizo.escribir(c);
                }
            }
            _ => {}
        }
        if !bautizo.abierto() {
            break;
        }
    }
}

fn refrescar(bautizo: Res<Bautizo>, mut linea: Query<&mut Text, With<BautizoLinea>>) {
    if !bautizo.is_changed() {
        return;
    }
    for mut t in &mut linea {
        t.0 = format!("{}|", bautizo.texto);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn el_nombre_se_limpia_y_se_acota() {
        assert_eq!(nombre_valido("  Manchita  "), Some("Manchita".into()));
        assert_eq!(nombre_valido("Doña   Rosa"), Some("Doña Rosa".into()));
        assert_eq!(nombre_valido("   "), None);
        let largo = "a".repeat(40);
        assert_eq!(nombre_valido(&largo).unwrap().chars().count(), MAX_NOMBRE);
    }

    /// Todo lo que el teclado ofrece lo dibuja la fuente del juego (regla 6).
    #[test]
    fn el_teclado_escribe_castellano() {
        let todas: String = FILAS.concat();
        assert!(todas.contains('Ñ') && todas.contains('ñ') && todas.contains('á'));
    }
}
