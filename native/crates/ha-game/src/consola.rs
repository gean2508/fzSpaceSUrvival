//! **La consola escondida — `/creativo`, `/dar`, `/ir`.**
//!
//! Decisión del 15 ago 2026, literal: el modo creativo se elige al crear el
//! mundo, y en juego se cambia desde una consola escondida — **nunca un botón a
//! mano ni un chat**. Se destraba tocando **siete veces la versión** en AJUSTES,
//! que es el gesto que ya conoce cualquiera que haya activado las opciones de
//! desarrollador de un Android.
//!
//! ## Por qué tiene su propio teclado
//!
//! `character_create.rs` ya dibuja un teclado en pantalla (el del nombre), pero
//! está atado a su propio enum de acciones (`CcAction`) y a su pantalla. Sacarlo
//! a un módulo genérico tocaba dos sistemas y una pantalla que hoy funciona, y
//! encima el teclado del nombre no sirve tal cual: la consola necesita la barra
//! y la coma, y el del nombre los prohíbe a propósito. Son cuarenta líneas de
//! botones duplicadas contra el riesgo de romper el creador de personaje; se
//! eligió duplicar.
//!
//! ## Qué NO hace
//!
//! No es un chat ni un log del juego: la salida son las últimas respuestas a lo
//! que escribiste y nada más. Y no queda abierta: se cierra con LISTO.

use bevy::prelude::*;
use bevy::ui::FocusPolicy;

use crate::marca::Paleta;

/// Toques sobre la versión que hacen falta para destrabarla.
pub const TOQUES_PARA_DESTRABAR: u32 = 7;

/// Cuántas respuestas se recuerdan.
const LINEAS_SALIDA: usize = 5;

/// Las teclas, en filas. Va la barra (para el `/`), los dígitos y el punto y el
/// menos, que hacen falta para las coordenadas de `/ir`.
const FILAS: [&str; 4] = [
    "/ABCDEFGHIJ",
    "KLMNOPQRST",
    "UVWXYZ_ ",
    "0123456789-.",
];

/// **El estado de la consola.** Vive en un recurso y no en la pantalla porque
/// destrabarla es permanente dentro de la sesión: cerrás AJUSTES y sigue abierta
/// la puerta.
#[derive(Resource, Default)]
pub struct Consola {
    /// Toques acumulados sobre la versión.
    pub toques: u32,
    /// ¿Ya se destrabó?
    pub desbloqueada: bool,
    /// ¿Está el panel a la vista?
    pub abierta: bool,
    /// Lo que se está escribiendo.
    pub linea: String,
    /// Las últimas respuestas, la más nueva al final.
    pub salida: Vec<String>,
}

impl Consola {
    /// Un toque sobre la versión. Devuelve `true` si ESTE toque la destrabó.
    pub fn toque_en_version(&mut self) -> bool {
        if self.desbloqueada {
            return false;
        }
        self.toques += 1;
        if self.toques >= TOQUES_PARA_DESTRABAR {
            self.desbloqueada = true;
            self.responder("consola destrabada — escribí /ayuda");
            return true;
        }
        false
    }

    /// Cuántos toques faltan, para poder avisar en los últimos.
    pub fn faltan(&self) -> u32 {
        TOQUES_PARA_DESTRABAR.saturating_sub(self.toques)
    }

    fn responder(&mut self, texto: impl Into<String>) {
        self.salida.push(texto.into());
        while self.salida.len() > LINEAS_SALIDA {
            self.salida.remove(0);
        }
    }
}

/// Lo que un comando le pide al juego. Se devuelve en vez de ejecutarse en el
/// sitio para poder **probar el parseo sin levantar Bevy** — que es la mitad del
/// motivo de que esto sea un enum.
#[derive(Debug, PartialEq)]
pub enum Orden {
    /// Alternar el modo creativo.
    Creativo,
    /// Mostrar/ocultar los paneles de depuración (fauna, sociedad).
    Depurar,
    /// Poner `cantidad` de `item` en la bolsa.
    Dar { item: String, cantidad: u32 },
    /// Mover al jugador a esa celda de la superficie.
    Ir { x: f32, z: f32 },
    /// Los últimos eventos del mundo (`causal.rs`).
    Causal,
    /// Conteo de eventos y de tipos distintos (`causal.rs`).
    Actividad,
    /// Mostrar la ayuda.
    Ayuda,
    /// No era un comando: el texto es el motivo.
    Error(String),
}

/// **El parseo, aparte de todo lo demás y por eso testeable.**
///
/// Acepta con barra y sin barra (`/dar` y `dar`): en un teclado en pantalla la
/// barra es un toque más y no aporta nada.
pub fn parsear(linea: &str) -> Orden {
    let limpio = linea.trim().trim_start_matches('/').trim();
    if limpio.is_empty() {
        return Orden::Error("escribí algo".into());
    }
    let mut partes = limpio.split_whitespace();
    let verbo = partes.next().unwrap_or("").to_ascii_lowercase();
    match verbo.as_str() {
        "ayuda" => Orden::Ayuda,
        "creativo" => Orden::Creativo,
        "depurar" => Orden::Depurar,
        "causal" => Orden::Causal,
        "actividad" => Orden::Actividad,
        "dar" => {
            let Some(item) = partes.next() else {
                return Orden::Error("dar qué. probá /dar madera 10".into());
            };
            // La cantidad es opcional: `/dar madera` da uno.
            let cantidad = match partes.next() {
                None => 1,
                Some(n) => match n.parse::<u32>() {
                    Ok(v) if v > 0 => v,
                    _ => return Orden::Error(format!("`{n}` no es una cantidad")),
                },
            };
            Orden::Dar { item: item.to_ascii_lowercase(), cantidad }
        }
        "ir" => {
            let (Some(sx), Some(sz)) = (partes.next(), partes.next()) else {
                return Orden::Error("ir adónde. probá /ir 100 -40".into());
            };
            match (sx.parse::<f32>(), sz.parse::<f32>()) {
                (Ok(x), Ok(z)) => Orden::Ir { x, z },
                _ => Orden::Error(format!("`{sx} {sz}` no son coordenadas")),
            }
        }
        otro => Orden::Error(format!("no conozco `{otro}`. probá /ayuda")),
    }
}

// ---------------------------------------------------------------------------
// El panel
// ---------------------------------------------------------------------------

#[derive(Component)]
struct ConsolaRoot;

#[derive(Component)]
struct ConsolaLinea;

#[derive(Component)]
struct ConsolaSalida;

/// Qué hace cada botón del panel.
#[derive(Component, Clone, PartialEq)]
enum Tecla {
    Char(char),
    Borrar,
    Enviar,
    Cerrar,
}

pub struct ConsolaPlugin;

impl Plugin for ConsolaPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Consola>().add_systems(
            Update,
            (sincronizar_panel, teclas, refrescar_texto).chain(),
        );
    }
}

/// Abre y cierra el panel siguiendo a `Consola::abierta`, sin recrearlo cada
/// frame.
fn sincronizar_panel(
    consola: Res<Consola>,
    mut commands: Commands,
    touch: Res<crate::input::TouchLayout>,
    art: Res<crate::art2d::ArtLibrary>,
    vivos: Query<Entity, With<ConsolaRoot>>,
) {
    let quiere = consola.abierta && consola.desbloqueada;
    let hay = !vivos.is_empty();
    if quiere == hay {
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

    // Las coordenadas son las de `mock_pantallas::consola`, literales.
    let (cw, chh) = (1500.0_f32, 476.0_f32);
    let (cx, cy) = ((up::ANCHO - cw) / 2.0, 300.0_f32);

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
            // El velo del mockup: el juego se sigue viendo detrás.
            BackgroundColor(Color::srgba(0.047, 0.039, 0.078, 0.47)),
            // Por encima del teclado del creador de personaje (1100).
            GlobalZIndex(1200),
            FocusPolicy::Block,
            ConsolaRoot,
            Name::new("Consola"),
        ))
        .with_children(|root| {
            root.spawn(up::lienzo(k)).with_children(|l| {
                up::marco(l, &art, up::marco::PANEL, cx, cy, cw, chh, k);
                up::cartel(l, &art, "CONSOLA", cx + cw / 2.0, cy - 48.0, 480.0, 92.0, 44.0, k);

                let ix = cx + 60.0;
                up::marco(l, &art, up::marco::BOTON_ANCHO, ix, cy + 56.0, cw - 120.0, 92.0, k);
                // El renglón que se escribe. El cursor es parte del texto: un
                // nodo aparte tendría que perseguir el ancho del glifo.
                l.spawn(up::caja(ix + 34.0, cy + 104.0 - 32.0, cw - 160.0, 64.0, k))
                    .with_children(|t| {
                        t.spawn(Node {
                            position_type: PositionType::Absolute,
                            left: Val::Px(0.0),
                            top: Val::Px(0.0),
                            width: Val::Percent(100.0),
                            height: Val::Percent(100.0),
                            justify_content: JustifyContent::FlexStart,
                            align_items: AlignItems::Center,
                            ..default()
                        })
                        .with_children(|t| {
                            t.spawn((
                                Text::new("/"),
                                TextFont { font_size: 44.0 * k, ..default() },
                                TextColor(Pack::TINTA),
                                TextLayout::new_with_no_wrap(),
                                ConsolaLinea,
                            ));
                        });
                    });

                // La ayuda: los tres comandos y qué hacen.
                for (j, (cmd, expl)) in [
                    ("/creativo", "prende o apaga el modo creativo"),
                    ("/dar <ítem> <n>", "te pone algo en la mochila"),
                    ("/ir <x> <z>", "te lleva a esa coordenada"),
                ]
                .into_iter()
                .enumerate()
                {
                    let y = cy + 176.0 + j as f32 * 68.0;
                    up::texto(l, cmd, 34.0, Pack::TINTA, ix + 12.0, y + 26.0, Ancla::Izq, k);
                    up::texto(l, expl, 28.0, Pack::SUAVE, ix + 460.0, y + 28.0, Ancla::Izq, k);
                }
                // Las respuestas de lo que corriste, encima del pie.
                l.spawn(up::caja(ix, cy + chh - 110.0, cw - 120.0, 46.0, k)).with_children(|t| {
                    t.spawn((
                        Text::new(String::new()),
                        TextFont { font_size: 26.0 * k, ..default() },
                        TextColor(Pack::SUAVE),
                        ConsolaSalida,
                    ));
                });
                up::texto(
                    l,
                    "Se destraba tocando siete veces el número de versión, en AJUSTES.",
                    28.0, Pack::PIE, cx + cw / 2.0, cy + chh - 42.0, Ancla::Medio, k,
                );

                // **El teclado va DEBAJO del panel.** La imagen de referencia
                // muestra la consola con `/creativo` ya escrito y no dibuja las
                // teclas; escribirlo hace falta igual, así que las teclas se
                // ponen en el espacio que la referencia deja vacío, sin tocar
                // nada de lo que sí dibuja.
                let ty0 = cy + chh + 24.0;
                let ancho_t = 100.0_f32;
                let alto_t = 62.0_f32;
                let sep = 6.0_f32;
                for (f, fila) in FILAS.iter().enumerate() {
                    let n = fila.chars().count() as f32;
                    let total = n * ancho_t + (n - 1.0) * sep;
                    let x0 = (up::ANCHO - total) / 2.0;
                    let y = ty0 + f as f32 * (alto_t + sep);
                    for (i, c) in fila.chars().enumerate() {
                        up::boton(
                            l, &art, x0 + i as f32 * (ancho_t + sep), y, ancho_t, alto_t,
                            Some(&etiqueta(c)), None, false, 28.0, Tecla::Char(c), k,
                        );
                    }
                }
                let y = ty0 + FILAS.len() as f32 * (alto_t + sep);
                let anchos = [("BORRAR", Tecla::Borrar, 260.0_f32, false),
                              ("CORRER", Tecla::Enviar, 260.0, true),
                              ("LISTO", Tecla::Cerrar, 220.0, false)];
                let total: f32 = anchos.iter().map(|a| a.2).sum::<f32>() + sep * 2.0;
                let mut x = (up::ANCHO - total) / 2.0;
                for (et, accion, w, fuerte) in anchos {
                    up::boton(l, &art, x, y, w, alto_t, Some(et), None, fuerte, 28.0, accion, k);
                    x += w + sep;
                }
            });
        });
}

fn etiqueta(c: char) -> String {
    if c == ' ' {
        "ESP".into()
    } else {
        c.to_string()
    }
}

fn tecla(padre: &mut ChildBuilder, accion: Tecla, texto: &str, w: f32, h: f32, s: f32) {
    let ancha = !matches!(accion, Tecla::Char(_));
    padre
        .spawn((
            Button,
            Node {
                width: Val::Px(w),
                height: Val::Px(h),
                justify_content: JustifyContent::Center,
                align_items: AlignItems::Center,
                border: UiRect::all(Val::Px(if ancha { 2.0 } else { 1.0 })),
                ..default()
            },
            BorderRadius::all(Val::Px(4.0 * s)),
            BorderColor(if ancha { Paleta::TURQUESA } else { Paleta::MORADO }),
            BackgroundColor(Paleta::MORADO_HONDO),
            accion,
        ))
        .with_children(|b| {
            b.spawn((
                Text::new(texto.to_string()),
                TextFont { font_size: (12.0 * s).clamp(8.0, 14.0), ..default() },
                TextColor(Paleta::TEXTO),
            ));
        });
}

/// Los toques del panel, y la ejecución de lo que se escribió.
#[allow(clippy::too_many_arguments)]
fn teclas(
    mut consola: ResMut<Consola>,
    botones: Query<(&Interaction, &Tecla), Changed<Interaction>>,
    mut editor: ResMut<crate::ui_editor::UiEditorState>,
    mut bolsa: ResMut<crate::hud::inventory_ui::InventoryUiState>,
    mut jugador: Query<(&mut crate::sim::SimState, &mut crate::sim::PlayerVisual), With<crate::sim::Player>>,
) {
    for (interaccion, tecla) in &botones {
        if *interaccion != Interaction::Pressed {
            continue;
        }
        match tecla {
            Tecla::Char(c) => {
                if consola.linea.chars().count() < 48 {
                    consola.linea.push(*c);
                }
            }
            Tecla::Borrar => {
                consola.linea.pop();
            }
            Tecla::Cerrar => {
                consola.abierta = false;
            }
            Tecla::Enviar => {
                let linea = std::mem::take(&mut consola.linea);
                let orden = parsear(&linea);
                let respuesta = ejecutar(orden, &mut editor, &mut bolsa, &mut jugador);
                consola.responder(respuesta);
            }
        }
    }
}

/// Aplica una orden ya parseada. Devuelve lo que se le contesta al jugador.
fn ejecutar(
    orden: Orden,
    editor: &mut crate::ui_editor::UiEditorState,
    bolsa: &mut crate::hud::inventory_ui::InventoryUiState,
    jugador: &mut Query<(&mut crate::sim::SimState, &mut crate::sim::PlayerVisual), With<crate::sim::Player>>,
) -> String {
    match orden {
        Orden::Ayuda => "/creativo · /dar <item> <n> · /ir <x> <z> · /depurar · /causal · /actividad".into(),
        Orden::Causal => {
            let u = crate::causal::ultimos(4);
            if u.is_empty() { "todavía no pasó nada anotable".into() } else { u.join(" | ") }
        }
        Orden::Actividad => crate::causal::resumen(),
        Orden::Depurar => {
            if crate::debug::alternar_paneles() {
                "paneles de depuración a la vista".into()
            } else {
                "paneles de depuración ocultos".into()
            }
        }
        Orden::Error(por_que) => por_que,
        Orden::Creativo => {
            editor.creative = !editor.creative;
            if editor.creative { "creativo ACTIVADO".into() } else { "creativo apagado".into() }
        }
        Orden::Dar { item, cantidad } => {
            // **Se valida contra el catálogo**: `/dar asdf 10` tiene que decirlo,
            // no meter una pila fantasma en la bolsa.
            //
            // No hay un `item_def` que devuelva `Option`; lo que hay es
            // `item_display_name`, que para un id desconocido cae en `"Item"`.
            // Ese es el predicado disponible y por eso se usa éste y no otro —
            // si algún día aparece un catálogo de verdad, esto se cambia por él.
            if ha_shared::items::item_display_name(&item) == "Item" {
                return format!("no existe el ítem `{item}`");
            }
            if crate::hud::inventory_ui::add_item_to_slots(bolsa, &item, cantidad) {
                format!("{cantidad} × {item}")
            } else {
                "no entra: la bolsa está llena".into()
            }
        }
        Orden::Ir { x, z } => {
            let Ok((mut sim, mut vis)) = jugador.get_single_mut() else {
                return "no hay jugador al que mover".into();
            };
            sim.body.x = x;
            sim.body.z = z;
            vis.from_x = x;
            vis.to_x = x;
            vis.from_z = z;
            vis.to_z = z;
            format!("estás en {x:.0}, {z:.0}")
        }
    }
}

fn refrescar_texto(
    consola: Res<Consola>,
    mut linea: Query<&mut Text, (With<ConsolaLinea>, Without<ConsolaSalida>)>,
    mut salida: Query<&mut Text, (With<ConsolaSalida>, Without<ConsolaLinea>)>,
) {
    if !consola.is_changed() {
        return;
    }
    for mut t in &mut linea {
        t.0 = format!("/{}", consola.linea);
    }
    for mut t in &mut salida {
        t.0 = consola.salida.join("\n");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// **Los tres comandos que se pidieron, y sus formas.**
    #[test]
    fn los_tres_comandos_se_entienden() {
        assert_eq!(parsear("/creativo"), Orden::Creativo);
        // Sin barra también: en un teclado en pantalla es un toque de más.
        assert_eq!(parsear("creativo"), Orden::Creativo);
        assert_eq!(parsear("/depurar"), Orden::Depurar);
        assert_eq!(
            parsear("/dar madera 10"),
            Orden::Dar { item: "madera".into(), cantidad: 10 }
        );
        // La cantidad es opcional.
        assert_eq!(parsear("/dar madera"), Orden::Dar { item: "madera".into(), cantidad: 1 });
        assert_eq!(parsear("/ir 100 -40"), Orden::Ir { x: 100.0, z: -40.0 });
    }

    /// Un comando mal escrito CONTESTA. Quedarse callado en una consola es lo
    /// peor que puede pasar: no sabés si no entendió o si no hizo nada.
    #[test]
    fn lo_que_no_entiende_lo_dice() {
        for malo in ["/", "  ", "/volar", "/dar", "/dar madera dos", "/ir 1", "/ir a b"] {
            assert!(
                matches!(parsear(malo), Orden::Error(_)),
                "`{malo}` tendría que dar error y no lo dio"
            );
        }
    }

    /// **Siete toques, ni seis ni ocho**, y sólo cuentan hasta destrabar.
    #[test]
    fn la_versión_se_toca_siete_veces() {
        let mut c = Consola::default();
        for i in 1..TOQUES_PARA_DESTRABAR {
            assert!(!c.toque_en_version(), "se destrabó en el toque {i}");
            assert!(!c.desbloqueada);
            assert_eq!(c.faltan(), TOQUES_PARA_DESTRABAR - i);
        }
        assert!(c.toque_en_version(), "el séptimo toque no la destrabó");
        assert!(c.desbloqueada);
        // Y ya destrabada, tocar de nuevo no vuelve a "destrabar".
        assert!(!c.toque_en_version());
    }

    /// La salida no crece sin freno.
    #[test]
    fn la_salida_tiene_techo() {
        let mut c = Consola::default();
        for i in 0..20 {
            c.responder(format!("linea {i}"));
        }
        assert_eq!(c.salida.len(), LINEAS_SALIDA);
        assert_eq!(c.salida.last().unwrap(), "linea 19");
    }
}
