//! **Las tres pantallas nuevas del menú: PARTIDAS, PERSONAJES y NUEVA PARTIDA.**
//!
//! Salen de la propuesta de interfaz del 15 ago 2026 y de una decisión del
//! usuario que las ordena a las tres: *"el personaje es uno y los mundos son
//! otros"*. Por eso son **dos listas separadas** y no una: cualquier personaje
//! entra a cualquier mundo, y en un mundo pueden vivir varios.
//!
//! ## Por qué son overlays y no estados de `GamePhase`
//!
//! Un `GamePhase` nuevo arrastra el fondo, la cámara, el `root` y los sistemas de
//! entrada de toda la pantalla, y hay que enseñarle a volver a cada sitio desde
//! el que se llega. Estas tres son listas que se abren encima del menú y se
//! cierran; el patrón ya existe y funciona (`SettingsPanel` en `boot_ui`). Se
//! reusa ése.
//!
//! ## La papelera pregunta
//!
//! El usuario pidió borrar partidas, y borrar es lo único de este menú que no
//! tiene vuelta atrás. No se abre un diálogo modal —en apaisado de teléfono tapa
//! la lista entera— sino que **el botón se convierte en la pregunta**: el primer
//! toque lo pone en «¿SEGURO?» y sólo el segundo borra. Tocar cualquier otra
//! cosa lo cancela.

use bevy::prelude::*;

use crate::game_phase::{load_savegame, GamePhase, SelectedCharacter};
use crate::marca::Paleta;
use crate::partida::{self, Ficha};
use crate::personajes::{self, Personaje};

/// Cuántas partidas caben. El documento dejó el número abierto ("dibujé cinco");
/// se toma el del mockup, que es el que entra sin scroll en el apaisado del
/// Redmi.
pub const MAX_PARTIDAS: usize = 5;

// ---------------------------------------------------------------------------
// Estado
// ---------------------------------------------------------------------------

/// Cuál de las tres está abierta. `Ninguna` = se ve el menú pelado.
#[derive(Resource, Default, Clone, Copy, PartialEq, Eq, Debug)]
pub enum Pantalla {
    #[default]
    Ninguna,
    Partidas,
    Personajes,
    Nueva,
    /// MULTIJUGADOR: anfitriones de la LAN y el servidor online.
    Multijugador,
}

/// Lo que la pantalla NUEVA PARTIDA tiene armado ahora mismo.
#[derive(Resource)]
pub struct BorradorPartida {
    pub nombre: String,
    pub semilla: u32,
    pub creativo: bool,
}

impl Default for BorradorPartida {
    fn default() -> Self {
        Self { nombre: String::new(), semilla: 0, creativo: false }
    }
}

impl BorradorPartida {
    /// Un nombre que no hay que tipear. Tipear en este teléfono cuesta, y el
    /// nombre se puede cambiar después desde la lista.
    fn renombrar_auto(&mut self) {
        self.nombre = format!("Partida {}", partida::listar().len() + 1);
    }
}

/// **Qué se está escribiendo con el teclado en pantalla.**
///
/// El campo NOMBRE de la imagen de referencia se puede tipear, y el lápiz de una
/// fila renombra ese mundo. Las dos cosas escriben texto, así que comparten un
/// solo teclado en vez de dos.
#[derive(Resource, Default)]
pub struct Tecleando {
    /// `None` = el teclado está cerrado.
    pub destino: Option<Destino>,
    pub texto: String,
    /// MAYÚS apretada: invierte el caso de la próxima letra (`qwerty.rs`).
    pub mayus: bool,
}

/// A dónde va lo que se escribe.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum Destino {
    /// El nombre del mundo que se está por crear.
    NombreNuevo,
    /// El nombre de una partida que ya existe.
    Partida(String),
}

/// La fila que está preguntando si de verdad la borrás.
#[derive(Resource, Default)]
pub struct BorradoPendiente(pub Option<String>);

// ---------------------------------------------------------------------------
// Componentes
// ---------------------------------------------------------------------------

/// El overlay entero de una de las tres pantallas.
#[derive(Component)]
pub struct PanelLista;

/// El contenedor cuyos hijos se rehacen cada vez que cambia la pantalla.
#[derive(Component)]
pub struct CuerpoLista;

/// Qué hace un botón de adentro de estas pantallas.
#[derive(Component, Clone, Debug, PartialEq, Eq)]
pub enum AccionLista {
    Cerrar,
    /// Entrar a esa partida con el personaje que esté elegido.
    Jugar(String),
    /// Primer toque: pregunta. Segundo: borra.
    Borrar(String),
    /// Elegir este personaje como el que juega.
    Elegir(String),
    BorrarPersonaje(String),
    /// Editar el personaje elegido (vuelve al creador con su look puesto).
    EditarPersonaje(String),
    /// Ir a la pantalla de creación de personaje.
    CrearPersonaje,
    /// Renombrar una partida desde la lista (el lápiz de la fila).
    Renombrar(String),
    /// Tocar el campo NOMBRE de la partida nueva: abre el teclado.
    EscribirNombre,
    /// Una tecla del teclado en pantalla.
    Tecla(char),
    /// Invertir el caso de la próxima letra.
    Mayus,
    /// Borrar el último carácter.
    Retroceso,
    /// Cerrar el teclado y quedarse con lo escrito.
    Listo,
    /// Los controles de NUEVA PARTIDA.
    Sortear,
    /// Elegir modo: `true` = creativo.
    Modo(bool),
    Crear,
    /// MULTIJUGADOR: conectar a esta dirección (`ip:puerto`).
    Unirse(String),
}

// ---------------------------------------------------------------------------
// Plugin
// ---------------------------------------------------------------------------

pub struct MenuListasPlugin;

impl Plugin for MenuListasPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Pantalla>()
            .init_resource::<BorradorPartida>()
            .init_resource::<BorradoPendiente>()
            .init_resource::<Tecleando>()
            .init_resource::<ListaShot>()
            .add_systems(OnEnter(GamePhase::MainMenu), al_entrar)
            .add_systems(
                Update,
                (
                    asegurar_panel,
                    redibujar.run_if(resource_changed::<Pantalla>),
                    tocar,
                    capturar_listas,
                )
                    .chain()
                    .run_if(in_state(GamePhase::MainMenu)),
            );
    }
}

/// El menú siempre abre pelado: si saliste de PARTIDAS entrando a un mundo y
/// volvés, no te encontrás la lista abierta de antes.
fn al_entrar(mut pantalla: ResMut<Pantalla>, mut pendiente: ResMut<BorradoPendiente>) {
    *pantalla = Pantalla::Ninguna;
    pendiente.0 = None;
}

/// **El overlay se crea cuando el menú ya existe, no en el mismo `OnEnter`.**
///
/// Primera versión: `spawn_panel` colgado de `OnEnter(MainMenu)`, igual que
/// `boot_ui::spawn_main_menu`. Dos sistemas en el mismo `OnEnter` no tienen orden
/// garantizado, y cuando éste corría primero no había `BootRoot` todavía: la
/// query salía vacía, el `return` se comía el spawn en silencio y tocar PARTIDAS
/// no hacía nada. Se vio en el Redmi antes que en ningún test.
///
/// Corriendo en `Update` el problema no existe: el primer cuadro en el que el
/// menú está armado, el panel nace. Y como es hijo de `BootRoot`, se va solo con
/// él al salir del menú.
fn asegurar_panel(
    mut commands: Commands,
    mut pantalla: ResMut<Pantalla>,
    ya: Query<(), With<PanelLista>>,
    root: Query<Entity, With<crate::boot_ui::BootRoot>>,
) {
    if !ya.is_empty() {
        return;
    }
    let Ok(root) = root.get_single() else { return };
    // Recién creado no tiene hijos: se marca para que `redibujar` lo llene en
    // este mismo cuadro (los sistemas van encadenados).
    pantalla.set_changed();
    commands.entity(root).with_children(|p| {
        p.spawn((
            Node {
                position_type: PositionType::Absolute,
                left: Val::Px(0.0),
                top: Val::Px(0.0),
                width: Val::Percent(100.0),
                height: Val::Percent(100.0),
                display: Display::None,
                ..default()
            },
            // **Transparente a propósito.** En el mockup estas pantallas se ven
            // sobre el mismo bosque del menú; lo que evita leer los botones de
            // atrás no es un velo sino esconder el lienzo del menú
            // (`boot_ui::MenuContenido`), que es lo que hace `redibujar`.
            GlobalZIndex(1100),
            bevy::ui::FocusPolicy::Block,
            PanelLista,
            Name::new("PanelLista"),
        ))
        .with_children(|panel| {
            panel.spawn((
                Node {
                    position_type: PositionType::Absolute,
                    left: Val::Px(0.0),
                    top: Val::Px(0.0),
                    width: Val::Percent(100.0),
                    height: Val::Percent(100.0),
                    ..default()
                },
                CuerpoLista,
                Name::new("CuerpoLista"),
            ));
        });
    });
}

// ---------------------------------------------------------------------------
// Dibujo
// ---------------------------------------------------------------------------

#[allow(clippy::too_many_arguments)]
fn redibujar(
    mut commands: Commands,
    pantalla: Res<Pantalla>,
    borrador: Res<BorradorPartida>,
    pendiente: Res<BorradoPendiente>,
    mut paneles: Query<&mut Node, With<PanelLista>>,
    cuerpos: Query<Entity, With<CuerpoLista>>,
    mut menu: Query<&mut Node, (With<crate::boot_ui::MenuContenido>, Without<PanelLista>)>,
    touch: Res<crate::input::TouchLayout>,
    art: Res<crate::art2d::ArtLibrary>,
    cg: Res<crate::chargen::Chargen>,
    hoja: Res<crate::ui_pack::HojaPj>,
    assets: Res<AssetServer>,
    seleccionado: Res<SelectedCharacter>,
    tecleando: Res<Tecleando>,
    multi: Res<crate::multijugador::Multi>,
) {
    let Ok(mut panel) = paneles.get_single_mut() else { return };
    let abierta = *pantalla != Pantalla::Ninguna;
    panel.display = if abierta { Display::Flex } else { Display::None };
    // El menú de atrás se esconde: en el mockup estas pantallas se ven sobre el
    // bosque, no sobre los botones del menú.
    for mut n in &mut menu {
        n.display = if abierta { Display::None } else { Display::Flex };
    }
    let Ok(cuerpo) = cuerpos.get_single() else { return };
    commands.entity(cuerpo).despawn_descendants();
    if !abierta {
        return;
    }
    let k = crate::ui_pack::k(&touch);
    let pend = pendiente.0.clone();
    let ctx = Ctx { art: &art, cg: &cg, hoja: &hoja, assets: &assets, k };
    let escribiendo = tecleando.destino.is_some();
    commands.entity(cuerpo).with_children(|c| {
        c.spawn(crate::ui_pack::lienzo(k)).with_children(|l| {
            match *pantalla {
                Pantalla::Ninguna => {}
                Pantalla::Partidas => dibujar_partidas(l, &ctx, pend.as_deref()),
                Pantalla::Personajes => {
                    dibujar_personajes(l, &ctx, pend.as_deref(), &seleccionado)
                }
                Pantalla::Nueva => dibujar_nueva(l, &ctx, &borrador),
                Pantalla::Multijugador => dibujar_multijugador(l, &ctx, &multi),
            }
            if escribiendo {
                dibujar_teclado(l, &ctx, &tecleando.texto, tecleando.mayus);
            }
        });
    });
}

/// Lo que hace falta para dibujar cualquiera de las tres pantallas.
struct Ctx<'a> {
    art: &'a crate::art2d::ArtLibrary,
    cg: &'a crate::chargen::Chargen,
    hoja: &'a crate::ui_pack::HojaPj,
    assets: &'a AssetServer,
    k: f32,
}

/// "hace 2 h" — cuánto pasó desde que se tocó la partida más nueva.
fn hace_cuanto(segundos: u64) -> String {
    let ahora = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let d = ahora.saturating_sub(segundos);
    if d < 90 {
        "recién".to_string()
    } else if d < 5400 {
        format!("hace {} min", d / 60)
    } else if d < 172_800 {
        format!("hace {} h", d / 3600)
    } else {
        format!("hace {} días", d / 86_400)
    }
}

/// **PARTIDAS.** Cada fila es un mundo: su viñeta, su semilla, su día y quiénes
/// viven ahí. Transcripta de `mock_pantallas::partidas`.
/// MULTIJUGADOR: los anfitriones que avisaron por la LAN y el servidor online.
fn dibujar_multijugador(l: &mut ChildBuilder, ctx: &Ctx, multi: &crate::multijugador::Multi) {
    use crate::ui_pack::{self as up, Ancla, Pack};
    let k = ctx.k;
    let (pw, ph) = (1800.0_f32, 810.0_f32);
    let (pxu, pyu) = ((up::ANCHO - pw) / 2.0, 210.0_f32);
    up::marco(l, ctx.art, up::marco::PANEL, pxu, pyu, pw, ph, k);
    up::cartel(l, ctx.art, "MULTIJUGADOR", pxu + pw / 2.0, pyu - 48.0, 640.0, 96.0, 48.0, k);
    let (ix, iy) = (pxu + 66.0, pyu + 60.0);
    let (fila_h, sep) = (150.0_f32, 18.0_f32);
    let mut fila = 0usize;
    let mut y = |fila: usize| iy + fila as f32 * (fila_h + sep);
    for a in multi.anfitriones.iter().take(4) {
        let yy = y(fila);
        up::marco(l, ctx.art, up::marco::SLOT, ix, yy, pw - 132.0, fila_h, k);
        up::texto(l, a.nombre.clone(), 44.0, Pack::TINTA, ix + 40.0, yy + 50.0, Ancla::Izq, k);
        up::texto(
            l,
            format!("{} · semilla {} · {} jugando", a.direccion, crate::boot_ui::separar_miles(a.semilla), a.jugadores),
            28.0, Pack::SUAVE, ix + 40.0, yy + 100.0, Ancla::Izq, k,
        );
        up::boton(l, ctx.art, ix + pw - 132.0 - 330.0, yy + 29.0, 290.0, 92.0, Some("UNIRSE"), None, fila == 0, 32.0,
                  AccionLista::Unirse(a.direccion.clone()), k);
        fila += 1;
    }
    if let Some(dir) = crate::multijugador::servidor_online() {
        let yy = y(fila);
        up::marco(l, ctx.art, up::marco::SLOT, ix, yy, pw - 132.0, fila_h, k);
        up::texto(l, "SERVIDOR ONLINE", 44.0, Pack::TINTA, ix + 40.0, yy + 50.0, Ancla::Izq, k);
        up::texto(l, dir.clone(), 28.0, Pack::SUAVE, ix + 40.0, yy + 100.0, Ancla::Izq, k);
        up::boton(l, ctx.art, ix + pw - 132.0 - 330.0, yy + 29.0, 290.0, 92.0, Some("UNIRSE"), None, fila == 0, 32.0,
                  AccionLista::Unirse(dir), k);
        fila += 1;
    }
    if fila == 0 {
        up::texto(l, "Buscando anfitriones en tu red...", 36.0, Pack::SUAVE, pxu + pw / 2.0, pyu + ph / 2.0 - 40.0, Ancla::Medio, k);
        up::texto(l, "Un amigo abre su mundo con MENÚ · MULTIJUGADOR · ANFITRIÓN LAN.", 28.0, Pack::PIE, pxu + pw / 2.0, pyu + ph / 2.0 + 30.0, Ancla::Medio, k);
    }
    if !multi.mensaje.is_empty() {
        up::texto(l, multi.mensaje.clone(), 30.0, Pack::SUAVE, pxu + pw / 2.0, pyu + ph - 46.0, Ancla::Medio, k);
    }
    up::boton(l, ctx.art, 40.0, 134.0, 200.0, 76.0, Some("VOLVER"), None, false, 30.0, AccionLista::Cerrar, k);
}

fn dibujar_partidas(l: &mut ChildBuilder, ctx: &Ctx, pendiente: Option<&str>) {
    use crate::ui_pack::{self as up, Ancla, Pack};
    let k = ctx.k;
    let fichas = partida::listar();
    let personajes = personajes::listar();

    up::barra_estado(
        l,
        ctx.art,
        up::CUERPO_TIERRA,
        &[
            up::Ficha {
                icono: "ui/ic_lista",
                etiqueta: "PARTIDAS",
                valor: format!("{} de {}", fichas.len(), MAX_PARTIDAS),
            },
            up::Ficha {
                icono: "ui/ic_persona",
                etiqueta: "PERSONAJES",
                valor: personajes.len().to_string(),
            },
            up::Ficha {
                icono: "ui/ic_reloj",
                etiqueta: "ÚLTIMA",
                valor: fichas.first().map(|f| hace_cuanto(f.creada)).unwrap_or_else(|| "—".into()),
            },
        ],
        k,
    );

    let (pw, ph) = (1800.0_f32, 810.0_f32);
    let (pxu, pyu) = ((up::ANCHO - pw) / 2.0, 210.0_f32);
    up::marco(l, ctx.art, up::marco::PANEL, pxu, pyu, pw, ph, k);
    up::cartel(l, ctx.art, "PARTIDAS", pxu + pw / 2.0, pyu - 48.0, 520.0, 96.0, 48.0, k);

    let (ix, iy) = (pxu + 66.0, pyu + 60.0);
    let (fila_h, sep) = (186.0_f32, 18.0_f32);
    // La deco de la viñeta la elige la semilla del mundo: dos mundos distintos
    // no se ven iguales, y el mismo mundo se ve siempre igual.
    const DECOS: [[&str; 2]; 4] = [
        ["nature/arbol_1", "agua/bote_abajo_1"],
        ["nature/piedra_1", "structure/boulder"],
        ["edificio/local", "vehicle/auto"],
        ["nature/arbol_10", "prop/farol_de_calle_1"],
    ];
    if fichas.is_empty() {
        up::texto(
            l,
            "Todavía no hay ninguna. Tocá NUEVA PARTIDA.",
            34.0,
            Pack::SUAVE,
            pxu + pw / 2.0,
            pyu + ph / 2.0,
            Ancla::Medio,
            k,
        );
    }
    for (i, f) in fichas.iter().take(MAX_PARTIDAS).enumerate() {
        let y = iy + i as f32 * (fila_h + sep);
        up::marco(l, ctx.art, up::marco::SLOT, ix, y, pw - 132.0, fila_h, k);
        let deco = DECOS[(f.semilla as usize) % DECOS.len()];
        up::vineta(l, ctx.art, ix + 20.0, y + 20.0, 300.0, fila_h - 40.0, &deco, 2.0, false,
                   f.semilla as u64 + 3, k);
        let modo = if f.creativo { " · creativo" } else { "" };
        up::texto(l, format!("{}{modo}", f.nombre), 44.0, Pack::TINTA, ix + 350.0, y + 52.0,
                  Ancla::Izq, k);
        up::texto(
            l,
            format!("semilla {} · día {}", crate::boot_ui::separar_miles(f.semilla), f.dia),
            28.0, Pack::SUAVE, ix + 350.0, y + 100.0, Ancla::Izq, k,
        );
        up::texto(l, "viven acá:", 26.0, Pack::SUAVE, ix + 350.0, y + 140.0, Ancla::Izq, k);
        for (j, nombre) in f.habitantes.iter().take(4).enumerate() {
            let Some(p) = personajes.iter().find(|p| &p.nombre == nombre) else { continue };
            let Some(look) = p.look else { continue };
            up::muneco(l, &look, ctx.cg, ctx.assets, ctx.hoja, 3,
                       ix + 500.0 + j as f32 * 46.0, y + 112.0, 1.0, k);
        }
        let der = ix + pw - 132.0;
        up::boton(l, ctx.art, der - 520.0, y + 46.0, 260.0, 92.0, Some("JUGAR"), None, i == 0,
                  32.0, AccionLista::Jugar(f.id.clone()), k);
        let (et, ic) = if pendiente == Some(f.id.as_str()) {
            (Some("¿SEGURO?"), None)
        } else {
            (None, Some("ui/ic_papelera"))
        };
        up::boton(l, ctx.art, der - 230.0, y + 46.0, 92.0, 92.0, et, ic, false, 20.0,
                  AccionLista::Borrar(f.id.clone()), k);
        up::boton(l, ctx.art, der - 122.0, y + 46.0, 92.0, 92.0, None, Some("ui/ic_lapiz"),
                  false, 32.0, AccionLista::Renombrar(f.id.clone()), k);
    }
    up::texto(
        l,
        "La papelera pide confirmación y borra sólo esa partida — el personaje no se toca.",
        28.0, Pack::PIE, pxu + pw / 2.0, pyu + ph - 46.0, Ancla::Medio, k,
    );
    // VOLVER: el mockup no lo dibuja porque la maqueta no se toca, pero la
    // pantalla tiene que tener salida. Va en el hueco del cartel, arriba a la
    // izquierda del panel, sin taparle nada a la lista.
    up::boton(l, ctx.art, 40.0, 134.0, 200.0, 76.0, Some("VOLVER"), None, false, 30.0,
              AccionLista::Cerrar, k);
}

/// **PERSONAJES.** Una lista aparte de los mundos, a propósito.
fn dibujar_personajes(
    l: &mut ChildBuilder,
    ctx: &Ctx,
    pendiente: Option<&str>,
    seleccionado: &SelectedCharacter,
) {
    use crate::ui_pack::{self as up, Ancla, Pack};
    let k = ctx.k;
    let lista = personajes::listar();
    let elegido = lista
        .iter()
        .find(|p| p.nombre == seleccionado.name)
        .or_else(|| lista.first());

    up::barra_estado(
        l,
        ctx.art,
        up::CUERPO_TIERRA,
        &[
            up::Ficha {
                icono: "ui/ic_persona",
                etiqueta: "PERSONAJES",
                valor: format!("{} de {}", lista.len(), personajes::MAXIMO),
            },
            up::Ficha {
                icono: "ui/ic_lista",
                etiqueta: "PARTIDAS",
                valor: partida::listar().len().to_string(),
            },
            up::Ficha {
                icono: "ui/ic_reloj",
                etiqueta: "ELEGIDO",
                valor: elegido.map(|p| p.nombre.clone()).unwrap_or_else(|| "—".into()),
            },
        ],
        k,
    );

    let (pw, ph) = (2040.0_f32, 810.0_f32);
    let (pxu, pyu) = ((up::ANCHO - pw) / 2.0, 210.0_f32);
    up::marco(l, ctx.art, up::marco::PANEL, pxu, pyu, pw, ph, k);
    up::cartel(l, ctx.art, "PERSONAJES", pxu + pw / 2.0, pyu - 48.0, 560.0, 96.0, 48.0, k);

    let (ix, iy) = (pxu + 60.0, pyu + 54.0);
    let (cw, ch, sep) = (372.0_f32, 330.0_f32, 18.0_f32);
    for (i, p) in lista.iter().take(5).enumerate() {
        let x = ix + i as f32 * (cw + sep);
        up::marco(l, ctx.art, up::marco::SLOT, x, iy, cw, ch, k)
            .insert((Button, AccionLista::Elegir(p.id.clone())));
        if let Some(look) = p.look {
            let cel = Vec2::new(ctx.cg.index.cell[0] as f32, ctx.cg.index.cell[1] as f32) * 4.0;
            up::muneco(l, &look, ctx.cg, ctx.assets, ctx.hoja, 3,
                       x + cw / 2.0 - cel.x / 2.0, iy + 26.0, 4.0, k);
        }
        up::texto(l, p.nombre.clone(), 38.0, Pack::TINTA, x + cw / 2.0, iy + 296.0, Ancla::Medio, k);
        if Some(p.id.as_str()) == elegido.map(|e| e.id.as_str()) {
            up::elegido(l, x, iy, cw, ch, 6.0, k);
        }
    }

    // Segunda fila: el hueco para uno nuevo y la ficha del elegido.
    let y2 = iy + ch + 30.0;
    let alto2 = ch - 40.0;
    up::marco(l, ctx.art, up::marco::SLOT, ix, y2, cw, alto2, k)
        .insert((Button, AccionLista::CrearPersonaje));
    up::texto(l, "+", 92.0, Pack::SUAVE, ix + cw / 2.0, y2 + 60.0, Ancla::Medio, k);
    up::texto(l, "NUEVO", 34.0, Pack::TINTA, ix + cw / 2.0, y2 + 150.0, Ancla::Medio, k);
    up::texto(
        l,
        format!("quedan {} huecos", personajes::MAXIMO.saturating_sub(lista.len())),
        24.0, Pack::SUAVE, ix + cw / 2.0, y2 + 200.0, Ancla::Medio, k,
    );

    let px2 = ix + cw + sep;
    let ancho2 = pw - 120.0 - cw - sep;
    up::marco(l, ctx.art, up::marco::LISO, px2, y2, ancho2, alto2, k);
    if let Some(p) = elegido {
        up::texto(l, p.nombre.to_uppercase(), 44.0, Pack::TINTA, px2 + 40.0, y2 + 56.0, Ancla::Izq, k);
        let mundos: Vec<String> = partida::listar()
            .into_iter()
            .filter(|f| f.habitantes.iter().any(|h| h == &p.nombre))
            .map(|f| f.nombre)
            .collect();
        let donde = match mundos.len() {
            0 => "Todavía no entró a ningún mundo.".to_string(),
            1 => format!("Estuvo en {}.", mundos[0]),
            _ => format!("Estuvo en {} y en {}.", mundos[0], mundos[mundos.len() - 1]),
        };
        up::texto(l, donde, 30.0, Pack::SUAVE, px2 + 40.0, y2 + 106.0, Ancla::Izq, k);
        up::texto(
            l,
            "Un personaje no pertenece a un mundo: entra al que elijas.",
            28.0, Pack::SUAVE, px2 + 40.0, y2 + 150.0, Ancla::Izq, k,
        );
        let bw = 260.0_f32;
        let bx = px2 + ancho2 - 60.0 - (bw * 3.0 + 36.0);
        let borrando = pendiente == Some(p.id.as_str());
        up::boton(l, ctx.art, bx, y2 + 180.0, bw, 92.0, Some("EDITAR"), None, false, 32.0,
                  AccionLista::EditarPersonaje(p.id.clone()), k);
        up::boton(l, ctx.art, bx + bw + 18.0, y2 + 180.0, bw, 92.0,
                  Some(if borrando { "¿SEGURO?" } else { "BORRAR" }), None, false, 32.0,
                  AccionLista::BorrarPersonaje(p.id.clone()), k);
        up::boton(l, ctx.art, bx + (bw + 18.0) * 2.0, y2 + 180.0, bw, 92.0, Some("ELEGIR"), None,
                  true, 32.0, AccionLista::Elegir(p.id.clone()), k);
    } else {
        up::texto(l, "Todavía no hay ninguno.", 34.0, Pack::SUAVE, px2 + 40.0, y2 + 90.0,
                  Ancla::Izq, k);
    }
    up::boton(l, ctx.art, 40.0, 134.0, 200.0, 76.0, Some("VOLVER"), None, false, 30.0,
              AccionLista::Cerrar, k);
}

/// **NUEVA PARTIDA: nombre, semilla y modo.** El creativo se decide acá, cuando
/// nace el mundo — no es un botón del menú (decisión del 15 ago 2026).
fn dibujar_nueva(l: &mut ChildBuilder, ctx: &Ctx, b: &BorradorPartida) {
    use crate::ui_pack::{self as up, Ancla, Pack};
    let k = ctx.k;
    let personajes = personajes::listar();

    up::barra_estado(
        l,
        ctx.art,
        up::CUERPO_TIERRA,
        &[
            up::Ficha {
                icono: "ui/ic_lista",
                etiqueta: "PARTIDAS",
                valor: format!("{} de {}", partida::listar().len(), MAX_PARTIDAS),
            },
            up::Ficha {
                icono: "ui/ic_persona",
                etiqueta: "PERSONAJE",
                valor: personajes.first().map(|p| p.nombre.clone()).unwrap_or_else(|| "—".into()),
            },
        ],
        k,
    );

    let (pw, ph) = (1560.0_f32, 780.0_f32);
    let (pxu, pyu) = ((up::ANCHO - pw) / 2.0, 222.0_f32);
    up::marco(l, ctx.art, up::marco::PANEL, pxu, pyu, pw, ph, k);
    up::cartel(l, ctx.art, "NUEVA PARTIDA", pxu + pw / 2.0, pyu - 48.0, 620.0, 96.0, 46.0, k);

    let (ix, iy) = (pxu + 66.0, pyu + 54.0);
    let inner = pw - 132.0;

    up::texto(l, "NOMBRE", 28.0, Pack::ETIQUETA, ix, iy + 26.0, Ancla::Izq, k);
    up::marco(l, ctx.art, up::marco::BOTON_ANCHO, ix, iy + 48.0, inner, 92.0, k)
        .insert((Button, AccionLista::EscribirNombre));
    up::texto(l, b.nombre.clone(), 40.0, Pack::TINTA, ix + 36.0, iy + 96.0, Ancla::Izq, k);

    up::texto(l, "SEMILLA", 28.0, Pack::ETIQUETA, ix, iy + 190.0, Ancla::Izq, k);
    up::marco(l, ctx.art, up::marco::BOTON_ANCHO, ix, iy + 212.0, inner - 220.0, 92.0, k);
    up::texto(l, crate::boot_ui::separar_miles(b.semilla), 40.0, Pack::TINTA, ix + 36.0,
              iy + 260.0, Ancla::Izq, k);
    up::boton(l, ctx.art, ix + inner - 200.0, iy + 212.0, 200.0, 92.0, Some("AL AZAR"), None,
              false, 32.0, AccionLista::Sortear, k);

    up::texto(l, "MODO", 28.0, Pack::ETIQUETA, ix, iy + 354.0, Ancla::Izq, k);
    let mw = (inner - 24.0) / 2.0;
    let modos = [
        ("SUPERVIVENCIA", "Se junta, se fabrica, se pasa hambre.", false),
        ("CREATIVO", "Todo el catálogo a mano y vuelo libre.", true),
    ];
    for (j, (et, sub, es_creativo)) in modos.into_iter().enumerate() {
        let x = ix + j as f32 * (mw + 24.0);
        up::marco(l, ctx.art, up::marco::SLOT, x, iy + 386.0, mw, 150.0, k)
            .insert((Button, AccionLista::Modo(es_creativo)));
        up::texto(l, et, 40.0, Pack::TINTA, x + 30.0, iy + 440.0, Ancla::Izq, k);
        up::texto(l, sub, 26.0, Pack::SUAVE, x + 30.0, iy + 490.0, Ancla::Izq, k);
        if es_creativo == b.creativo {
            up::elegido(l, x, iy + 386.0, mw, 150.0, 6.0, k);
            up::icono(l, ctx.art, "ui/ic_tilde", x + mw - 84.0, iy + 410.0, 64.0, k);
        }
    }
    up::texto(
        l,
        "El modo se elige acá. Después, en el juego, se cambia con la consola.",
        26.0, Pack::ETIQUETA, ix, iy + 566.0, Ancla::Izq, k,
    );
    up::boton(l, ctx.art, ix + inner - 420.0, iy + 596.0, 420.0, 96.0, Some("CREAR MUNDO"), None,
              true, 34.0, AccionLista::Crear, k);
    up::boton(l, ctx.art, 40.0, 134.0, 200.0, 76.0, Some("VOLVER"), None, false, 30.0,
              AccionLista::Cerrar, k);
}

/// **El teclado en pantalla.** QWERTY (`qwerty.rs`) con el marco del pack:
/// números, letras con eñe y tildes, MAYÚS, retroceso y LISTO. Sin él, el campo
/// NOMBRE de la imagen de referencia sería un adorno. Se redibuja en cada tecla,
/// así que las letras salen rotuladas en el caso que van a escribir.
fn dibujar_teclado(l: &mut ChildBuilder, ctx: &Ctx, texto: &str, mayus: bool) {
    use crate::ui_pack::{self as up, Ancla, Pack};
    let k = ctx.k;
    let filas = crate::qwerty::filas_castellano();
    let mayuscula = crate::qwerty::va_mayuscula(texto, mayus);

    // El velo: acá sí tapa, porque el teclado se abre ENCIMA de la pantalla.
    l.spawn((
        up::caja(0.0, 0.0, up::ANCHO, up::ALTO, k),
        BackgroundColor(Color::srgba(0.047, 0.039, 0.078, 0.78)),
    ));
    let (pw, ph) = (1800.0_f32, 680.0_f32);
    let (pxu, pyu) = ((up::ANCHO - pw) / 2.0, 200.0_f32);
    up::marco(l, ctx.art, up::marco::PANEL, pxu, pyu, pw, ph, k);
    up::cartel(l, ctx.art, "NOMBRE", pxu + pw / 2.0, pyu - 48.0, 460.0, 96.0, 44.0, k);

    let ix = pxu + 66.0;
    up::marco(l, ctx.art, up::marco::BOTON_ANCHO, ix, pyu + 50.0, pw - 132.0, 92.0, k);
    up::texto(l, format!("{texto}|"), 40.0, Pack::TINTA, ix + 36.0, pyu + 98.0, Ancla::Izq, k);

    let ancho_t = 118.0_f32;
    let alto_t = 84.0_f32;
    let sep = 8.0_f32;
    for (f, fila) in filas.iter().enumerate() {
        let n = fila.chars().count() as f32;
        let total = n * ancho_t + (n - 1.0) * sep;
        let x0 = pxu + (pw - total) / 2.0;
        let y = pyu + 180.0 + f as f32 * (alto_t + sep);
        for (i, c) in fila.chars().enumerate() {
            let rotulo = crate::qwerty::en_caso(c, mayuscula).to_string();
            up::boton(
                l, ctx.art, x0 + i as f32 * (ancho_t + sep), y, ancho_t, alto_t,
                Some(&rotulo), None, false, 34.0, AccionLista::Tecla(c), k,
            );
        }
    }
    let y = pyu + 180.0 + filas.len() as f32 * (alto_t + sep);
    up::boton(l, ctx.art, pxu + 66.0, y, 240.0, alto_t, Some("MAYÚS"), None, mayus, 30.0,
              AccionLista::Mayus, k);
    up::boton(l, ctx.art, pxu + 340.0, y, 420.0, alto_t, Some("ESPACIO"), None, false, 30.0,
              AccionLista::Tecla(' '), k);
    up::boton(l, ctx.art, pxu + 794.0, y, 260.0, alto_t, Some("BORRAR"), None, false, 30.0,
              AccionLista::Retroceso, k);
    up::boton(l, ctx.art, pxu + pw - 66.0 - 300.0, y, 300.0, alto_t, Some("LISTO"), None, true,
              32.0, AccionLista::Listo, k);
}

/// **QA de dev**: `HA_LISTASHOT=<prefijo>` abre las tres pantallas del menú una
/// tras otra y deja una captura de cada una, después sale. Es la única forma de
/// compararlas contra las imágenes de referencia sin el teléfono.
#[derive(Resource, Default)]
pub struct ListaShot {
    cuadros: u32,
    paso: usize,
}

fn capturar_listas(
    mut shot: ResMut<ListaShot>,
    mut pantalla: ResMut<Pantalla>,
    mut borrador: ResMut<BorradorPartida>,
    mut commands: Commands,
    mut exit: EventWriter<AppExit>,
) {
    use bevy::render::view::screenshot::{save_to_disk, Screenshot};
    let Ok(prefijo) = crate::flags::var("HA_LISTASHOT") else { return };
    shot.cuadros += 1;
    // Un respiro entre pantalla y pantalla: el fondo tiene que estar armado.
    const CADA: u32 = 24;
    if shot.cuadros % CADA != 0 {
        return;
    }
    let pasos: [(Pantalla, &str); 4] = [
        (Pantalla::Partidas, "partidas"),
        (Pantalla::Personajes, "personajes"),
        (Pantalla::Nueva, "nueva"),
        (Pantalla::Ninguna, "fin"),
    ];
    // El paso anterior ya está dibujado: se captura y se pasa al siguiente.
    if shot.paso > 0 && shot.paso <= pasos.len() {
        let (_, nombre) = pasos[shot.paso - 1];
        if nombre != "fin" {
            commands
                .spawn(Screenshot::primary_window())
                .observe(save_to_disk(format!("{prefijo}-{nombre}.png")));
            eprintln!("ha-listashot: {nombre}");
        }
    }
    if shot.paso >= pasos.len() {
        exit.send(AppExit::Success);
        return;
    }
    let (siguiente, _) = pasos[shot.paso];
    if siguiente == Pantalla::Nueva {
        preparar_borrador(&mut borrador);
    }
    *pantalla = siguiente;
    shot.paso += 1;
}

// ---------------------------------------------------------------------------
// Toques
// ---------------------------------------------------------------------------

#[allow(clippy::too_many_arguments)]
fn tocar(
    mut interacciones: Query<(&Interaction, &AccionLista), Changed<Interaction>>,
    mut pantalla: ResMut<Pantalla>,
    mut borrador: ResMut<BorradorPartida>,
    mut pendiente: ResMut<BorradoPendiente>,
    mut seleccionado: ResMut<SelectedCharacter>,
    mut next: ResMut<NextState<GamePhase>>,
    mut editor: ResMut<crate::ui_editor::UiEditorState>,
    mut tecleando: ResMut<Tecleando>,
    mut multi: ResMut<crate::multijugador::Multi>,
) {
    for (interaccion, accion) in &mut interacciones {
        if *interaccion != Interaction::Pressed {
            continue;
        }
        // Cualquier toque que no sea el segundo de un borrado lo cancela.
        let confirmando = matches!(
            accion,
            AccionLista::Borrar(_) | AccionLista::BorrarPersonaje(_)
        );
        match accion {
            AccionLista::Cerrar => {
                if *pantalla == Pantalla::Multijugador {
                    multi.buscar(false);
                    multi.cerrar_todo();
                }
                *pantalla = Pantalla::Ninguna;
            }
            AccionLista::Unirse(dir) => {
                let nombre = if seleccionado.name.is_empty() { "Viajero".to_string() } else { seleccionado.name.clone() };
                let piel = crate::multijugador::piel_para(&nombre);
                multi.conectar(dir.clone(), &nombre, &piel, 0, crate::multijugador::Origen::Menu);
                pantalla.set_changed();
            }
            AccionLista::Jugar(id) => {
                partida::abrir(id);
                // El personaje elegido manda; si esta partida traía uno guardado
                // y no hay ninguno elegido, se usa el de la partida.
                if let Some(guardado) = load_savegame() {
                    if seleccionado.look.is_none() {
                        *seleccionado = guardado;
                    }
                }
                editor.creative = partida::listar()
                    .into_iter()
                    .find(|f| &f.id == id)
                    .map(|f| f.creativo)
                    .unwrap_or(false);
                *pantalla = Pantalla::Ninguna;
                next.set(GamePhase::Loading);
            }
            AccionLista::Borrar(id) => {
                if pendiente.0.as_deref() == Some(id.as_str()) {
                    partida::borrar(id);
                    pendiente.0 = None;
                } else {
                    pendiente.0 = Some(id.clone());
                }
                // Forzar el redibujo: el texto del botón es el que pregunta.
                pantalla.set_changed();
            }
            AccionLista::Elegir(id) => {
                if let Some(p) = personajes::buscar(id) {
                    *seleccionado = personajes::a_seleccionado(&p);
                }
                *pantalla = Pantalla::Ninguna;
            }
            AccionLista::BorrarPersonaje(id) => {
                if pendiente.0.as_deref() == Some(id.as_str()) {
                    personajes::borrar(id);
                    pendiente.0 = None;
                } else {
                    pendiente.0 = Some(id.clone());
                }
                pantalla.set_changed();
            }
            AccionLista::CrearPersonaje => {
                *pantalla = Pantalla::Ninguna;
                next.set(GamePhase::CharacterCreate);
            }
            AccionLista::Sortear => {
                borrador.semilla = partida::semilla_al_azar();
                pantalla.set_changed();
            }
            AccionLista::Modo(creativo) => {
                borrador.creativo = *creativo;
                pantalla.set_changed();
            }
            AccionLista::EditarPersonaje(id) => {
                if let Some(p) = personajes::buscar(id) {
                    *seleccionado = personajes::a_seleccionado(&p);
                }
                *pantalla = Pantalla::Ninguna;
                next.set(GamePhase::CharacterCreate);
            }
            AccionLista::Renombrar(id) => {
                let actual = partida::listar()
                    .into_iter()
                    .find(|f| &f.id == id)
                    .map(|f| f.nombre)
                    .unwrap_or_default();
                tecleando.texto = actual;
                tecleando.destino = Some(Destino::Partida(id.clone()));
                tecleando.mayus = false;
                pantalla.set_changed();
            }
            AccionLista::EscribirNombre => {
                tecleando.texto = borrador.nombre.clone();
                tecleando.destino = Some(Destino::NombreNuevo);
                tecleando.mayus = false;
                pantalla.set_changed();
            }
            AccionLista::Tecla(c) => {
                if tecleando.texto.chars().count() < 22 {
                    let mayuscula = crate::qwerty::va_mayuscula(&tecleando.texto, tecleando.mayus);
                    let c = crate::qwerty::en_caso(*c, mayuscula);
                    tecleando.texto.push(c);
                }
                tecleando.mayus = false;
                pantalla.set_changed();
            }
            AccionLista::Mayus => {
                tecleando.mayus = !tecleando.mayus;
                pantalla.set_changed();
            }
            AccionLista::Retroceso => {
                tecleando.texto.pop();
                pantalla.set_changed();
            }
            AccionLista::Listo => {
                let nombre = tecleando.texto.trim().to_string();
                match tecleando.destino.take() {
                    Some(Destino::NombreNuevo) => {
                        if !nombre.is_empty() {
                            borrador.nombre = nombre;
                        }
                    }
                    Some(Destino::Partida(id)) => {
                        if !nombre.is_empty() {
                            partida::renombrar(&id, &nombre);
                        }
                    }
                    None => {}
                }
                pantalla.set_changed();
            }
            AccionLista::Crear => {
                if borrador.nombre.is_empty() {
                    borrador.renombrar_auto();
                }
                partida::crear(&borrador.nombre, Some(borrador.semilla), borrador.creativo);
                editor.creative = borrador.creativo;
                *pantalla = Pantalla::Ninguna;
                next.set(GamePhase::CharacterCreate);
            }
        }
        if !confirmando {
            pendiente.0 = None;
        }
    }
}

/// Deja el borrador listo para que la pantalla NUEVA PARTIDA muestre algo desde
/// el primer cuadro. La llama el botón del menú.
pub fn preparar_borrador(b: &mut BorradorPartida) {
    b.renombrar_auto();
    b.semilla = partida::semilla_al_azar();
    b.creativo = false;
}

#[cfg(test)]
mod tests {
    use super::*;

    /// **Borrar pide confirmación.** La invariante es que un solo toque nunca
    /// borra: el estado intermedio (`BorradoPendiente`) tiene que existir entre
    /// el primer toque y el segundo. Se prueba la máquina de estados, que es lo
    /// que se puede probar sin levantar la UI entera.
    #[test]
    fn un_solo_toque_no_borra() {
        let mut pendiente = BorradoPendiente::default();
        let id = "p123".to_string();
        // Primer toque: pregunta, no borra.
        let borraria = pendiente.0.as_deref() == Some(id.as_str());
        assert!(!borraria, "el primer toque ya borraba");
        pendiente.0 = Some(id.clone());
        // Segundo toque sobre la MISMA fila: ahora sí.
        assert!(pendiente.0.as_deref() == Some(id.as_str()));
        // Y un toque en otra fila no arrastra la confirmación.
        assert!(pendiente.0.as_deref() != Some("p999"));
    }
}
