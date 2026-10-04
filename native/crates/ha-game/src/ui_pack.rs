//! **El kit de interfaz del pack — la interfaz de Ki, tal cual el mockup.**
//!
//! Es el gemelo en Bevy de `uikit.py`, el módulo con el que se compuso la
//! propuesta que el usuario aprobó el 15 ago 2026 (*Interfaz de Ki*). Todo lo
//! que dibuja sale de arte REAL: los marcos y los botones-icono son recortes de
//! **Modern UI Style 1** publicados por `scripts/import-ui-kit.py`, y el texto va
//! con la fuente que embarca el APK.
//!
//! ## La regla de oro de este módulo: las coordenadas son las del mockup
//!
//! El mockup se compuso a **2400 × 1080** —el Redmi apaisado— y cada número de
//! esas pantallas (el panel de 1800 × 810, la fila de 186 de alto, el texto de
//! 44 px) está transcripto literal. Para que eso valga en cualquier pantalla, un
//! solo factor [`k`] multiplica todo: `k = ancho_de_la_ventana / 2400`. En el
//! teléfono da 1,0 y sale idéntico al PNG; en el escritorio encoge parejo.
//!
//! **No se corrigen números "a ojo" acá.** Si algo no entra, el número está mal
//! en el mockup y se cambia en el mockup.
//!
//! ## Por qué todo es absoluto y no flex
//!
//! Un layout de flex "parecido" a un dibujo se parece pero no es. La consigna
//! del usuario fue *"no quiero variaciones"*, así que cada pieza se planta en la
//! coordenada exacta que tiene en la imagen de referencia.

use bevy::prelude::*;
use bevy::ecs::system::EntityCommands;

use crate::art2d::ArtLibrary;
use crate::input::TouchLayout;

// ---------------------------------------------------------------------------
// La paleta, medida sobre el pack (uikit.py)
// ---------------------------------------------------------------------------

/// Los colores del kit. Medidos sobre Modern UI Style 1, no elegidos.
pub struct Pack;

impl Pack {
    /// Texto sobre panel: la tinta azulada del pack.
    pub const TINTA: Color = Color::srgb(0.227, 0.227, 0.314);
    /// Texto claro sobre el mundo (fuera de panel).
    pub const TEXTO: Color = Color::srgb(0.969, 0.941, 0.886);
    /// El amarillo de "esto está elegido".
    pub const ACENTO: Color = Color::srgb(0.933, 0.769, 0.329);
    /// Texto de etiqueta chiquita dentro de un panel.
    pub const ETIQUETA: Color = Color::srgb(0.588, 0.486, 0.384);
    /// Texto secundario dentro de un panel.
    pub const SUAVE: Color = Color::srgb(0.549, 0.455, 0.361);
    /// El pie de página de un panel, sobre el marco.
    pub const PIE: Color = Color::srgb(0.808, 0.729, 0.627);
    /// El filo de una viñeta (el recuadro de la miniatura de mundo).
    pub const FILO: Color = Color::srgb(0.471, 0.361, 0.259);
    /// El separador vertical de la barra de estado.
    pub const SEPARADOR: Color = Color::srgb(0.769, 0.659, 0.525);
    /// El clavo del cartel y el filo interno.
    pub const CLAVO: Color = Color::srgb(0.588, 0.424, 0.275);
    /// Tinta sobre el amarillo del botón fuerte.
    pub const SOBRE_ACENTO: Color = Color::srgb(0.220, 0.165, 0.078);
    /// La sombra bajo el personaje.
    pub const SOMBRA: Color = Color::srgba(0.078, 0.063, 0.102, 0.43);
}

/// Claves del kit (las publica `scripts/import-ui-kit.py`).
pub mod marco {
    pub const PANEL: &str = "ui/marco_panel";
    pub const LISO: &str = "ui/marco_liso";
    pub const SLOT: &str = "ui/marco_slot";
    pub const SLOT_CHICO: &str = "ui/marco_slot_chico";
    pub const BOTON: &str = "ui/marco_boton";
    pub const BOTON_ANCHO: &str = "ui/marco_boton_ancho";
    pub const HUECO: &str = "ui/marco_hueco";
}

// ---------------------------------------------------------------------------
// La escala
// ---------------------------------------------------------------------------

/// **El único factor de escala de la interfaz.** El mockup mide 2400 × 1080:
/// manda el lado que primero se queda corto, así una pantalla más ancha que
/// 20:9 no se come la parte de abajo.
pub fn k(touch: &TouchLayout) -> f32 {
    let por_ancho = touch.width.max(320.0) / ANCHO;
    let por_alto = touch.height.max(180.0) / ALTO;
    por_ancho.min(por_alto).clamp(0.18, 3.0)
}

/// **Las pantallas de menú se rehacen si la escala cambió después de armarlas.**
///
/// En el teléfono, el primer cuadro todavía no conoce la ventana: `TouchLayout`
/// vale su default (1280 de ancho) y el splash y el menú se armaban con una
/// escala más grande que la pantalla real — "desde el splash ya se sale de la
/// pantalla, no está centrada" (29 sep 2026). Cada pantalla se arma una sola vez
/// en su `OnEnter`, así que nadie la corregía. Esto re-entra al mismo estado
/// (sale y vuelve a entrar) cuando la escala se mueve, sólo en las pantallas de
/// menú: en `Playing` la ventana ya está medida hace rato.
fn rehacer_pantalla_si_cambia_k(
    layout: Res<TouchLayout>,
    fase: Res<State<crate::game_phase::GamePhase>>,
    mut next: ResMut<NextState<crate::game_phase::GamePhase>>,
    mut armada_con: Local<Option<(crate::game_phase::GamePhase, f32)>>,
) {
    use crate::game_phase::GamePhase as F;
    let actual = fase.get().clone();
    if !matches!(actual, F::Splash | F::MainMenu | F::CharacterCreate) {
        *armada_con = None;
        return;
    }
    let kk = k(&layout);
    match armada_con.as_ref() {
        Some((f, viejo)) if *f == actual => {
            if (kk - viejo).abs() > 0.002 {
                *armada_con = Some((actual.clone(), kk));
                next.set(actual);
            }
        }
        _ => *armada_con = Some((actual, kk)),
    }
}

/// Alto del lienzo del mockup, en sus propias unidades.
pub const ALTO: f32 = 1080.0;
/// Ancho del lienzo del mockup.
pub const ANCHO: f32 = 2400.0;

/// **El lienzo: 2400 × 1080 escalados, pegado ARRIBA.**
///
/// Todo lo demás se cuelga de acá con coordenadas del mockup. Si la ventana es
/// más alta que 20:9, lo que sobra queda ABAJO y lo llena el fondo — nunca se
/// estira el contenido para tapar el hueco, que es lo que deformaría el pixel.
///
/// **Por qué arriba y no centrado.** Centrado dejaba una franja vacía entre el
/// borde de la pantalla y la barra de estado, y esa barra es justamente lo que
/// tiene que estar pegado al borde: se ve como si la interfaz estuviera
/// flotando. Lo dijo el usuario mirando la primera captura.
pub fn lienzo(k: f32) -> Node {
    Node {
        position_type: PositionType::Absolute,
        left: Val::Percent(50.0),
        top: Val::Px(0.0),
        width: Val::Px(ANCHO * k),
        height: Val::Px(ALTO * k),
        margin: UiRect { left: Val::Px(-ANCHO * k / 2.0), ..default() },
        ..default()
    }
}

/// Un nodo absoluto en coordenadas del mockup.
pub fn caja(x: f32, y: f32, w: f32, h: f32, k: f32) -> Node {
    Node {
        position_type: PositionType::Absolute,
        left: Val::Px(x * k),
        top: Val::Px(y * k),
        width: Val::Px(w * k),
        height: Val::Px(h * k),
        ..default()
    }
}

// ---------------------------------------------------------------------------
// Las piezas
// ---------------------------------------------------------------------------

/// **Un marco de 9 partes en su coordenada.** Devuelve el `EntityCommands` para
/// que quien llame le cuelgue el contenido adentro.
///
/// Sin arte (el manifest todavía sin importar) cae a un rectángulo del color del
/// panel: la pantalla sigue siendo legible y no queda un agujero — es la regla
/// de los fallbacks dibujados por código.
pub fn marco<'a>(
    c: &'a mut ChildBuilder,
    art: &ArtLibrary,
    clave: &str,
    x: f32,
    y: f32,
    w: f32,
    h: f32,
    k: f32,
) -> EntityCommands<'a> {
    let nodo = caja(x, y, w, h, k);
    match art.ui_frame_node(clave) {
        Some(img) => c.spawn((nodo, img)),
        None => c.spawn((nodo, BackgroundColor(Color::srgb(0.796, 0.706, 0.580)))),
    }
}

/// **Un icono encajado en un cuadrado, sin deformarlo.**
///
/// Es el `icono_en` del mockup: el dibujo entra entero en el cuadrado y se
/// centra, conservando su proporción. Estirarlo a la fuerza es lo que hace que
/// una lupa de 26 × 20 salga como un huevo.
pub fn icono(c: &mut ChildBuilder, art: &ArtLibrary, clave: &str, x: f32, y: f32, lado: f32, k: f32) {
    let Some(img) = art.ui_icon(clave) else { return };
    let (w, h) = match art.ui_icon_px(clave) {
        Some(px) if px.x > 0.0 && px.y > 0.0 => {
            let f = (lado / px.x).min(lado / px.y);
            (px.x * f, px.y * f)
        }
        _ => (lado, lado),
    };
    c.spawn((caja(x + (lado - w) / 2.0, y + (lado - h) / 2.0, w, h, k), img, ZIndex(ICONO_Z)));
}

/// **Este botón no lleva marco.** El reskin del HUD (`hud::skin_ui_from_art`)
/// le pone `ui/button` a todo botón que no traiga imagen; un área táctil
/// invisible —el nombre del personaje, el fondo de un modal— quedaba con una
/// tabla de madera detrás. Con esta marca se lo saltea.
#[derive(Component)]
pub struct SinMarco;

/// Dónde cae el texto respecto de la `x` que se le pasa.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Ancla {
    /// `x` es el borde izquierdo (el `lm` del mockup).
    Izq,
    /// `x` es el centro (`mm`).
    Medio,
    /// `x` es el borde derecho (`rm`).
    Der,
}

/// **Texto anclado como en el mockup**: `y` es siempre el MEDIO de la línea.
///
/// Se planta dentro de una caja de alto conocido y centrado vertical, que es la
/// única forma de que el texto de Bevy caiga donde el `anchor="lm"` de PIL.
pub fn texto(
    c: &mut ChildBuilder,
    txt: impl Into<String>,
    px: f32,
    color: Color,
    x: f32,
    y: f32,
    ancla: Ancla,
    k: f32,
) {
    let alto = px * 1.6;
    // Ancho generoso: el texto no se corta y el centrado usa la caja, no el glifo.
    let ancho = ANCHO;
    let izq = match ancla {
        Ancla::Izq => x,
        Ancla::Medio => x - ancho / 2.0,
        Ancla::Der => x - ancho,
    };
    let justificado = match ancla {
        Ancla::Izq => JustifyContent::FlexStart,
        Ancla::Medio => JustifyContent::Center,
        Ancla::Der => JustifyContent::FlexEnd,
    };
    let mut nodo = caja(izq, y - alto / 2.0, ancho, alto, k);
    nodo.justify_content = justificado;
    nodo.align_items = AlignItems::Center;
    nodo.overflow = Overflow::visible();
    // **El texto va SIEMPRE por encima del marco.** Los rótulos chiquitos
    // ("NOMBRE", "SEMILLA") caen justo arriba de su campo y el campo se dibuja
    // después: sin esto la mitad de abajo de la palabra desaparecía.
    c.spawn((nodo, ZIndex(TEXTO_Z))).with_children(|t| {
        t.spawn((
            Text::new(txt),
            TextFont { font_size: px * k, ..default() },
            TextColor(color),
            TextLayout::new_with_no_wrap(),
        ));
    });
}

/// Capa de dibujo del texto dentro de una pantalla del kit.
const TEXTO_Z: i32 = 3;
/// Capa de los iconos: por encima del marco, por debajo del texto.
const ICONO_Z: i32 = 2;
/// Capa del recuadro de "esto está elegido": arriba de todo.
const ELEGIDO_Z: i32 = 4;

/// **El cartel de un panel.** Un título nunca es texto pelado: va en su chapa,
/// colgada del borde de arriba, con el filo interno y los dos clavos.
pub fn cartel(
    c: &mut ChildBuilder,
    art: &ArtLibrary,
    etiqueta: &str,
    cx: f32,
    y: f32,
    ancho: f32,
    alto: f32,
    px: f32,
    k: f32,
) {
    let x = cx - ancho / 2.0;
    marco(c, art, marco::BOTON, x, y, ancho, alto, k);
    // El filo interno, 15 px adentro (mock_menu::cartel).
    let mut filo = caja(x + 15.0, y + 15.0, ancho - 30.0, alto - 30.0, k);
    filo.border = UiRect::all(Val::Px(3.0 * k));
    c.spawn((filo, BorderColor(Pack::CLAVO)));
    texto(c, etiqueta, px, Pack::TINTA, cx, y + alto / 2.0, Ancla::Medio, k);
    for lado in [-1.0_f32, 1.0] {
        let rx = cx + lado * (ancho / 2.0 - 30.0);
        c.spawn((
            Node {
                position_type: PositionType::Absolute,
                left: Val::Px((rx - 7.0) * k),
                top: Val::Px((y + alto / 2.0 - 7.0) * k),
                width: Val::Px(14.0 * k),
                height: Val::Px(14.0 * k),
                ..default()
            },
            BackgroundColor(Pack::CLAVO),
            BorderRadius::all(Val::Percent(50.0)),
        ));
    }
}

/// **El recuadro amarillo de "esto está elegido".**
///
/// En el mockup es un rectángulo de 6 px de filo corrido 3 px hacia arriba y a
/// la izquierda respecto de la pieza. Se copia igual.
pub fn elegido(c: &mut ChildBuilder, x: f32, y: f32, w: f32, h: f32, grosor: f32, k: f32) {
    c.spawn((
        Node {
            position_type: PositionType::Absolute,
            left: Val::Px((x - 3.0) * k),
            top: Val::Px((y - 3.0) * k),
            width: Val::Px(w * k),
            height: Val::Px(h * k),
            border: UiRect::all(Val::Px(grosor * k)),
            ..default()
        },
        BorderColor(Pack::ACENTO),
        ZIndex(ELEGIDO_Z),
    ));
}

/// El relleno amarillo de un botón fuerte (el que llama a la acción).
pub fn relleno_fuerte(c: &mut ChildBuilder, x: f32, y: f32, w: f32, h: f32, k: f32) {
    c.spawn((caja(x, y, w, h, k), BackgroundColor(Pack::ACENTO)));
}

/// **Un botón del kit**: marco + (relleno si es fuerte) + etiqueta y/o icono.
///
/// La `accion` es el componente que identifica al botón para el sistema de
/// toques de cada pantalla; se le agrega `Button` para que Bevy le siga la
/// `Interaction`.
#[allow(clippy::too_many_arguments)]
pub fn boton<A: Bundle>(
    c: &mut ChildBuilder,
    art: &ArtLibrary,
    x: f32,
    y: f32,
    w: f32,
    h: f32,
    etiqueta: Option<&str>,
    icono_clave: Option<&str>,
    fuerte: bool,
    px: f32,
    accion: A,
    k: f32,
) {
    let nodo = caja(x, y, w, h, k);
    let mut ec = match art.ui_frame_node(marco::BOTON) {
        Some(img) => c.spawn((nodo, img, Button, accion)),
        None => c.spawn((nodo, BackgroundColor(Color::srgb(0.796, 0.706, 0.580)), Button, accion)),
    };
    ec.with_children(|b| {
        if fuerte {
            b.spawn((
                Node {
                    position_type: PositionType::Absolute,
                    left: Val::Px(18.0 * k),
                    top: Val::Px(16.0 * k),
                    width: Val::Px((w - 36.0) * k),
                    height: Val::Px((h - 32.0) * k),
                    ..default()
                },
                BackgroundColor(Pack::ACENTO),
            ));
        }
        if let Some(clave) = icono_clave {
            if let Some(img) = art.ui_icon(clave) {
                b.spawn((
                    Node {
                        position_type: PositionType::Absolute,
                        left: Val::Px((w / 2.0 - 32.0) * k),
                        top: Val::Px((h / 2.0 - 32.0) * k),
                        width: Val::Px(64.0 * k),
                        height: Val::Px(64.0 * k),
                        ..default()
                    },
                    img,
                ));
            }
        }
        if let Some(et) = etiqueta {
            let color = if fuerte { Pack::SOBRE_ACENTO } else { Pack::TINTA };
            let alto = px * 1.6;
            b.spawn(Node {
                position_type: PositionType::Absolute,
                left: Val::Px(0.0),
                top: Val::Px((h / 2.0 - alto / 2.0) * k),
                width: Val::Px(w * k),
                height: Val::Px(alto * k),
                justify_content: JustifyContent::Center,
                align_items: AlignItems::Center,
                ..default()
            })
            .with_children(|t| {
                t.spawn((
                    Text::new(et.to_string()),
                    TextFont { font_size: px * k, ..default() },
                    TextColor(color),
                    TextLayout::new_with_no_wrap(),
                ));
            });
        }
    });
}

// ---------------------------------------------------------------------------
// La barra de estado
// ---------------------------------------------------------------------------

/// Una ficha de la barra de arriba: icono, etiqueta chiquita y valor.
pub struct Ficha<'a> {
    pub icono: &'a str,
    pub etiqueta: &'a str,
    pub valor: String,
}

/// **La barra de arriba**: la chapa del juego + las fichas con su separador.
///
/// Es la misma en las cinco pantallas de menú; lo único que cambia son las
/// fichas. Sale literal de `mock_menu::barra_estado`.
pub fn barra_estado(c: &mut ChildBuilder, art: &ArtLibrary, cuerpo: &str, fichas: &[Ficha], k: f32) {
    barra_estado_en(c, art, cuerpo, "TIERRA", fichas, k);
}

/// Como [`barra_estado`], diciendo el nombre del cuerpo donde estás (el menú de
/// pausa se abre también en la Luna o en otro planeta).
pub fn barra_estado_en(
    c: &mut ChildBuilder,
    art: &ArtLibrary,
    cuerpo: &str,
    nombre_cuerpo: &str,
    fichas: &[Ficha],
    k: f32,
) {
    marco(c, art, marco::LISO, 18.0, 12.0, ANCHO - 36.0, 114.0, k);
    marco(c, art, marco::BOTON, 36.0, 24.0, 570.0, 90.0, k);
    texto(c, "KI", 56.0, Pack::TINTA, 66.0, 66.0, Ancla::Izq, k);
    texto(c, "MUNDO", 26.0, Pack::ETIQUETA, 146.0, 50.0, Ancla::Izq, k);
    texto(c, "ABIERTO", 26.0, Pack::ETIQUETA, 146.0, 82.0, Ancla::Izq, k);
    c.spawn((caja(300.0, 40.0, 4.0, 58.0, k), BackgroundColor(Pack::SEPARADOR)));
    icono(c, art, cuerpo, 356.0 - 29.0, 68.0 - 29.0, 58.0, k);
    texto(c, nombre_cuerpo, 30.0, Pack::TINTA, 400.0, 68.0, Ancla::Izq, k);
    let mut x = 700.0;
    for (i, f) in fichas.iter().enumerate() {
        c.spawn((caja(x - 30.0, 32.0, 4.0, 62.0, k), BackgroundColor(Pack::SEPARADOR)));
        icono(c, art, f.icono, x, 42.0, 64.0, k);
        texto(c, f.etiqueta, 24.0, Pack::ETIQUETA, x + 76.0, 50.0, Ancla::Izq, k);
        // El VALOR lleva marca con su posición: así una pantalla puede
        // refrescar "CAPA: PELO" sin rehacer la barra entera.
        texto_marcado(c, f.valor.clone(), 34.0, Pack::TINTA, x + 76.0, 84.0, Ancla::Izq, k,
                      FichaValor(i));
        x += 424.0;
    }
}

/// El valor de la ficha número `.0` de la barra de estado.
#[derive(Component, Clone, Copy, PartialEq, Eq)]
pub struct FichaValor(pub usize);

/// Como [`texto`], pero le cuelga un componente al nodo del texto para poder
/// refrescarlo después.
#[allow(clippy::too_many_arguments)]
pub fn texto_marcado<M: Component>(
    c: &mut ChildBuilder,
    txt: impl Into<String>,
    px: f32,
    color: Color,
    x: f32,
    y: f32,
    ancla: Ancla,
    k: f32,
    marca: M,
) {
    let alto = px * 1.6;
    let ancho = ANCHO;
    let izq = match ancla {
        Ancla::Izq => x,
        Ancla::Medio => x - ancho / 2.0,
        Ancla::Der => x - ancho,
    };
    let justificado = match ancla {
        Ancla::Izq => JustifyContent::FlexStart,
        Ancla::Medio => JustifyContent::Center,
        Ancla::Der => JustifyContent::FlexEnd,
    };
    let mut nodo = caja(izq, y - alto / 2.0, ancho, alto, k);
    nodo.justify_content = justificado;
    nodo.align_items = AlignItems::Center;
    nodo.overflow = Overflow::visible();
    c.spawn((nodo, ZIndex(TEXTO_Z))).with_children(|t| {
        t.spawn((
            Text::new(txt),
            TextFont { font_size: px * k, ..default() },
            TextColor(color),
            TextLayout::new_with_no_wrap(),
            marca,
        ));
    });
}

/// El nombre del cuerpo celeste que muestra la barra (siempre la Tierra por
/// ahora: los planetas son el gameplay de adentro de una partida, no una
/// pantalla de menú).
pub const CUERPO_TIERRA: &str = "ui/menu_tierra";

// ---------------------------------------------------------------------------
// El muñeco
// ---------------------------------------------------------------------------

use crate::chargen::{Chargen, Layer, LookDef};

/// La grilla de la hoja de personaje, compartida por todas las pantallas que
/// dibujan un muñeco (el menú, la lista de personajes, las miniaturas de "viven
/// acá"). Se arma una vez.
#[derive(Resource, Default)]
pub struct HojaPj(pub Handle<TextureAtlasLayout>);

pub struct UiPackPlugin;

impl Plugin for UiPackPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<HojaPj>()
            .init_resource::<HaloTex>()
            .add_systems(Startup, preparar_halo)
            .add_systems(Update, preparar_hoja)
            // En PreUpdate, no en Update: `OnEnter` arma la pantalla antes de cualquier Update,
            // con el layout que hubiera entonces. Si este sistema corriera después del sync
            // del layout, vería la escala YA corregida, la guardaría como "la de siempre" y
            // nunca reharía la splash — el celu la dejaba armada con la escala de 1280×720
            // (logo 45 % más grande y corrido hacia abajo, 3 oct 2026).
            .add_systems(PreUpdate, rehacer_pantalla_si_cambia_k)
            .add_systems(Update, (enmarcar_paneles, realce_botones))
            .add_systems(PostUpdate, escala_de_los_marcos.before(bevy::ui::UiSystem::Layout));
    }
}

// ---------------------------------------------------------------------------
// El toque se ve
// ---------------------------------------------------------------------------

/// Lo que el realce le cambió a un botón: el color que tenía y el que le puso.
#[derive(Component)]
pub struct Realce {
    base: Color,
    puesto: Color,
}

/// Cuánto se aclara un botón con el puntero encima (escritorio).
pub const REALCE_ENCIMA: f32 = 1.12;
/// Cuánto se oscurece mientras se aprieta: la madera "se hunde".
pub const REALCE_APRETADO: f32 = 0.74;

/// El color de un botón según su estado, partiendo del que tenía en reposo.
pub fn color_realzado(base: Color, i: Interaction) -> Color {
    let f = match i {
        Interaction::Pressed => REALCE_APRETADO,
        Interaction::Hovered => REALCE_ENCIMA,
        Interaction::None => return base,
    };
    let l = base.to_linear();
    Color::LinearRgba(LinearRgba::new(l.red * f, l.green * f, l.blue * f, l.alpha))
}

/// **Todo botón con arte responde al dedo.** Los marcos del kit no traen pieza
/// de "apretado", y un botón que no cambia al tocarlo se siente roto aunque
/// funcione (el clic suena, pero el ojo no se entera). Esto tiñe la imagen del
/// botón: más clara con el puntero encima, más oscura mientras se aprieta.
///
/// Sólo corre con `Changed<Interaction>`, o sea casi nunca. Y si otro sistema le
/// cambió el color al botón mientras tanto (una casilla deshabilitada, la
/// elegida de la hotbar), al soltar se respeta lo suyo en vez de pisarlo.
fn realce_botones(
    mut commands: Commands,
    mut botones: Query<
        (Entity, &Interaction, &mut ImageNode, Option<&mut Realce>),
        (Changed<Interaction>, With<Button>),
    >,
) {
    for (e, i, mut img, realce) in &mut botones {
        match realce {
            Some(mut r) => {
                // Alguien más tocó el color: ése pasa a ser el de reposo.
                if img.color != r.puesto {
                    r.base = img.color;
                }
                let c = color_realzado(r.base, *i);
                r.puesto = c;
                img.color = c;
            }
            None => {
                let base = img.color;
                let c = color_realzado(base, *i);
                img.color = c;
                commands.entity(e).try_insert(Realce { base, puesto: c });
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Velo y barra
// ---------------------------------------------------------------------------

/// El oscurecido de un menú que se abre sobre el juego.
pub const VELO: Color = Color::srgba(0.051, 0.039, 0.071, 0.64);

/// **El velo de un menú en partida**: el mundo sigue ahí, atenuado. Tapar todo
/// con un color opaco hacía que pausar se sintiera como salir del juego.
pub fn velo() -> (Node, BackgroundColor) {
    (
        Node {
            position_type: PositionType::Absolute,
            left: Val::Px(0.0),
            top: Val::Px(0.0),
            width: Val::Percent(100.0),
            height: Val::Percent(100.0),
            ..default()
        },
        BackgroundColor(VELO),
    )
}

/// El color de cada barra de estado (hambre, sed, ki): el mismo de la mochila.
pub const BARRA_HAMBRE: Color = Color::srgb(0.851, 0.627, 0.278);
pub const BARRA_SED: Color = Color::srgb(0.294, 0.525, 0.851);
pub const BARRA_KI: Color = Color::srgb(0.353, 0.784, 0.886);

/// **Una barra con marco**: canaleta hundida del kit y relleno con brillo
/// arriba. Devuelve nada; el relleno lleva `marca` para que un sistema le
/// cambie el ancho (`Val::Percent`).
#[allow(clippy::too_many_arguments)]
pub fn barra<M: Component>(
    c: &mut ChildBuilder,
    art: &ArtLibrary,
    x: f32,
    y: f32,
    w: f32,
    h: f32,
    color: Color,
    lleno: f32,
    marca: M,
    k: f32,
) {
    marco(c, art, marco::HUECO, x, y, w, h, k).with_children(|b| {
        let pad = 6.0;
        b.spawn(Node {
            position_type: PositionType::Absolute,
            left: Val::Px(pad * k),
            top: Val::Px(pad * k),
            width: Val::Px((w - pad * 2.0) * k),
            height: Val::Px((h - pad * 2.0) * k),
            overflow: Overflow::clip(),
            ..default()
        })
        .with_children(|canal| {
            canal
                .spawn((
                    Node {
                        width: Val::Percent((lleno * 100.0).clamp(0.0, 100.0)),
                        height: Val::Percent(100.0),
                        ..default()
                    },
                    BackgroundColor(color),
                    marca,
                ))
                .with_children(|r| {
                    // El brillo de arriba: lo que separa una barra del pack de
                    // un rectángulo de color.
                    r.spawn((
                        Node { width: Val::Percent(100.0), height: Val::Percent(30.0), ..default() },
                        BackgroundColor(Color::srgba(1.0, 1.0, 1.0, 0.22)),
                    ));
                });
        });
    });
}

/// Esquina de un marco del kit que todavía no se ajustó (la pone
/// `ArtLibrary::ui_frame_node` para los marcos con borde declarado).
pub const ESQUINA_SIN_AJUSTAR: f32 = 6.0;

/// **Un panel del HUD con el marco del pack** (en vez de un color plano): se le pone a la
/// entidad que hoy lleva `BackgroundColor` + `BorderColor`, con la clave del marco
/// (`marco::PANEL`, `marco::SLOT_CHICO`…). `enmarcar_paneles` la cambia por el 9-slice del kit y
/// subraya el relleno interior para que el contenido no caiga sobre la moldura. Si el pack no
/// tiene la clave, el panel queda como estaba (color plano): nunca se queda sin fondo.
#[derive(Component, Clone, Copy)]
pub struct MarcoDeHud(pub &'static str);

fn enmarcar_paneles(
    mut commands: Commands,
    art: Res<ArtLibrary>,
    touch: Res<TouchLayout>,
    mut nuevos: Query<(Entity, &MarcoDeHud, &mut Node), Added<MarcoDeHud>>,
) {
    for (e, m, mut node) in &mut nuevos {
        let Some(img) = art.ui_frame_node(m.0) else { continue };
        // La moldura se dibuja a `3·k` puntos por píxel del kit (ver `escala_de_los_marcos`): el
        // contenido tiene que empezar después. Se toma el borde declarado del marco.
        let borde = art.slices.get(m.0).map(|b| b[0]).unwrap_or(8.0);
        let moldura = (borde * 3.0 * k(&touch)).ceil() + 3.0;
        let (pl, pt) = (node.padding.left, node.padding.top);
        let mas = |v: Val| match v {
            Val::Px(x) => Val::Px(x.max(moldura)),
            _ => Val::Px(moldura),
        };
        node.padding = UiRect { left: mas(pl), right: mas(pl), top: mas(pt), bottom: mas(pt) };
        node.border = UiRect::ZERO;
        commands.entity(e).remove::<BackgroundColor>().remove::<BorderColor>().insert(img);
    }
}

/// Un marco del kit ya llevado a la escala de la pantalla.
#[derive(Component)]
pub struct MarcoDelKit;

/// **El borde de madera mide lo que mide en la maqueta.** Bevy agranda las
/// esquinas de un 9-slice hasta `max_corner_scale` según el tamaño del nodo:
/// con el tope en 6, un panel grande dibujaba su moldura de 11 px a 66 px
/// físicos, cuando la maqueta (x3 por píxel del kit) le da 33. Todo lo que el
/// panel pone "adentro" —pestañas, casillas, barras— caía ENCIMA de la madera y
/// la interfaz se veía rota (captura del usuario, 29 sep 2026). La escala del
/// kit en pantalla es `3 × k` puntos lógicos por píxel, o sea `3·k·factor` en
/// físicos, que es la unidad del `TextureSlicer`.
fn escala_de_los_marcos(
    mut commands: Commands,
    layout: Res<TouchLayout>,
    mut nuevos: Query<(Entity, &mut ImageNode), (Without<MarcoDelKit>, Changed<ImageNode>)>,
    mut viejos: Query<&mut ImageNode, With<MarcoDelKit>>,
    mut ultima: Local<f32>,
) {
    use bevy::ui::widget::NodeImageMode;
    let escala = (3.0 * k(&layout) * layout.scale_factor.max(0.5)).max(1.0);
    for (e, mut img) in &mut nuevos {
        let NodeImageMode::Sliced(s) = &img.image_mode else { continue };
        if s.max_corner_scale != ESQUINA_SIN_AJUSTAR {
            continue;
        }
        if let NodeImageMode::Sliced(s) = &mut img.image_mode {
            s.max_corner_scale = escala;
        }
        commands.entity(e).try_insert(MarcoDelKit);
    }
    if (escala - *ultima).abs() > 1e-3 {
        *ultima = escala;
        for mut img in &mut viejos {
            if let NodeImageMode::Sliced(s) = &mut img.image_mode {
                s.max_corner_scale = escala;
            }
        }
    }
}

fn preparar_halo(mut tex: ResMut<HaloTex>, mut images: ResMut<Assets<Image>>) {
    tex.0 = pintar_halo(&mut images);
}

fn preparar_hoja(
    mut hoja: ResMut<HojaPj>,
    mut layouts: ResMut<Assets<TextureAtlasLayout>>,
    cg: Option<Res<Chargen>>,
    mut listo: Local<bool>,
) {
    if *listo {
        return;
    }
    let Some(cg) = cg else { return };
    if !cg.available() {
        return;
    }
    hoja.0 = layouts.add(TextureAtlasLayout::from_grid(
        UVec2::new(cg.index.cell[0], cg.index.cell[1]),
        cg.index.cols.max(1),
        cg.index.rows.max(1),
        None,
        None,
    ));
    *listo = true;
}

/// Las capas de un personaje, en el orden en que se apilan.
const CAPAS: [Layer; 5] =
    [Layer::Body, Layer::Eyes, Layer::Outfit, Layer::Hair, Layer::Accessory];

fn indice_de(look: &LookDef, capa: Layer) -> Option<u32> {
    match capa {
        Layer::Body => Some(look.body),
        Layer::Eyes => Some(look.eyes),
        Layer::Outfit => Some(look.outfit),
        Layer::Hair => Some(look.hair),
        Layer::Accessory => look.accessory,
    }
}

/// La celda de la hoja: quieto, mirando a donde diga `dir` (3 = de frente, en el
/// orden `R, U, L, D` del pack).
pub fn celda(dir: u32) -> usize {
    match ha_assets::chargen_index::char_anim("walk") {
        Some(a) => ha_assets::chargen_index::char_cell(a, dir, 0) as usize,
        None => 0,
    }
}

/// **Un personaje dibujado con sus capas**, en coordenadas del mockup.
///
/// `escala` es la del mockup (`PJ.personaje(look, FRENTE, escala=7)`): multiplica
/// la celda de 32 × 64 del pack. Se apilan cinco `ImageNode` con atlas — lo
/// mismo que hace la vitrina del creador, o sea que no compone ni hornea nada.
///
/// `x`, `y` es la esquina superior izquierda; devuelve el alto que ocupó para
/// que quien lo llame lo pueda apoyar en el piso.
#[allow(clippy::too_many_arguments)]
pub fn muneco(
    c: &mut ChildBuilder,
    look: &LookDef,
    cg: &Chargen,
    assets: &AssetServer,
    hoja: &HojaPj,
    dir: u32,
    x: f32,
    y: f32,
    escala: f32,
    k: f32,
) -> Vec2 {
    let cel = Vec2::new(cg.index.cell[0] as f32, cg.index.cell[1] as f32);
    let medida = cel * escala;
    let idx = celda(dir);
    c.spawn(caja(x, y, medida.x, medida.y, k)).with_children(|pila| {
        for capa in CAPAS {
            let opt = indice_de(look, capa).and_then(|i| cg.options(capa).get(i as usize));
            let Some(o) = opt else { continue };
            pila.spawn((
                ImageNode::from_atlas_image(
                    assets.load(&o.png),
                    TextureAtlas { layout: hoja.0.clone(), index: idx },
                ),
                Node {
                    position_type: PositionType::Absolute,
                    left: Val::Px(0.0),
                    top: Val::Px(0.0),
                    width: Val::Percent(100.0),
                    height: Val::Percent(100.0),
                    ..default()
                },
            ));
        }
    });
    medida
}

/// La sombra ovalada bajo un personaje (el mockup la dibuja siempre).
pub fn sombra(c: &mut ChildBuilder, cx: f32, y: f32, rx: f32, ry: f32, k: f32) {
    c.spawn((
        caja(cx - rx, y, rx * 2.0, ry * 2.0, k),
        BackgroundColor(Pack::SOMBRA),
        BorderRadius::all(Val::Percent(50.0)),
    ));
}

/// La textura del claro de luz: un degradado radial suave, dibujado una vez.
#[derive(Resource, Default)]
pub struct HaloTex(pub Handle<Image>);

/// **El claro de luz detrás del personaje.** Sin esto, con el parallax encima, un
/// árbol del primer plano se lo come (nota del mockup, `mock_menu::halo`).
///
/// Va con TEXTURA y no con nodos redondeados apilados: cuatro elipses
/// translúcidas una encima de otra se leen como un disco gris con el borde
/// marcado —se vio en la primera captura—, mientras que el degradado del mockup
/// no tiene borde en ningún lado.
pub fn halo(c: &mut ChildBuilder, tex: &HaloTex, cx: f32, cy: f32, r: f32, k: f32) {
    if tex.0 == Handle::default() {
        return;
    }
    c.spawn((
        caja(cx - r, cy - r * 0.92, r * 2.0, r * 1.84, k),
        ImageNode::new(tex.0.clone()),
    ));
}

/// Dibuja el degradado del halo. 96 × 96 alcanza: se ve a 600 px y es un
/// degradado, o sea que ampliarlo no muestra ningún pixel.
fn pintar_halo(images: &mut Assets<Image>) -> Handle<Image> {
    use bevy::asset::RenderAssetUsages;
    use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};
    const N: usize = 96;
    let mut px = vec![0u8; N * N * 4];
    let c = (N as f32 - 1.0) / 2.0;
    for y in 0..N {
        for x in 0..N {
            let d = (((x as f32 - c) / c).powi(2) + ((y as f32 - c) / c).powi(2)).sqrt();
            // La misma curva del mockup: `52 · (1 − d)^1.5` de alfa.
            let a = if d >= 1.0 { 0.0 } else { 52.0 * (1.0 - d).powf(1.5) };
            let o = (y * N + x) * 4;
            px[o] = 255;
            px[o + 1] = 244;
            px[o + 2] = 214;
            px[o + 3] = a as u8;
        }
    }
    images.add(Image::new(
        Extent3d { width: N as u32, height: N as u32, depth_or_array_layers: 1 },
        TextureDimension::D2,
        px,
        TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::RENDER_WORLD,
    ))
}

// ---------------------------------------------------------------------------
// La viñeta
// ---------------------------------------------------------------------------

/// El azulejo de pasto de una viñeta, en coordenadas del mockup.
const AZULEJO_VINETA: f32 = 64.0;

/// **Una viñeta: un pedacito de mundo dentro de un recuadro.**
///
/// Es lo que hace que una fila de PARTIDAS diga de qué mundo se trata y que las
/// fichas del menú no sean cinco rectángulos iguales. Pasto azulejado, los props
/// que se le pasen apoyados en el piso, y el filo del mockup alrededor.
///
/// `separados` reparte los props a intervalos fijos (las fichas del menú);
/// si no, los desparrama con la semilla (las filas de partidas).
#[allow(clippy::too_many_arguments)]
pub fn vineta(
    c: &mut ChildBuilder,
    art: &ArtLibrary,
    x: f32,
    y: f32,
    w: f32,
    h: f32,
    props: &[&str],
    escala: f32,
    separados: bool,
    semilla: u64,
    k: f32,
) {
    let mut caja_vin = caja(x, y, w, h, k);
    caja_vin.overflow = Overflow::clip();
    caja_vin.border = UiRect::all(Val::Px(3.0 * k));
    c.spawn((caja_vin, BorderColor(Pack::FILO))).with_children(|v| {
        match art.ui_icon("tiles/grass") {
            Some(mut img) => {
                img.image_mode = bevy::ui::widget::NodeImageMode::Tiled {
                    tile_x: true,
                    tile_y: true,
                    stretch_value: AZULEJO_VINETA * k / 32.0,
                };
                v.spawn((
                    Node {
                        position_type: PositionType::Absolute,
                        left: Val::Px(0.0),
                        top: Val::Px(0.0),
                        width: Val::Percent(100.0),
                        height: Val::Percent(100.0),
                        ..default()
                    },
                    img,
                ));
            }
            None => {
                v.spawn((
                    Node {
                        position_type: PositionType::Absolute,
                        left: Val::Px(0.0),
                        top: Val::Px(0.0),
                        width: Val::Percent(100.0),
                        height: Val::Percent(100.0),
                        ..default()
                    },
                    BackgroundColor(Color::srgb(0.278, 0.592, 0.341)),
                ));
            }
        }
        let mut estado = semilla.wrapping_mul(6364136223846793005).wrapping_add(7);
        let mut azar = move || {
            estado = estado.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
            (estado >> 33) as u32
        };

        // **El pasto del pack es UN color plano** (71,151,87 en `suelo.png`): solo,
        // la viñeta era un rectángulo verde «de Paint» con un poste encima. El
        // mundo lo salva sembrando las variantes con marca (`#m1..#m4`); acá se
        // hace lo mismo, una de cada tres celdas, con la semilla de la viñeta.
        let celda = AZULEJO_VINETA;
        let (cols, filas) = ((w / celda).ceil() as i32, (h / celda).ceil() as i32);
        for cy in 0..filas {
            for cx in 0..cols {
                let r = azar();
                if r % 3 != 0 {
                    continue;
                }
                let clave = MARCAS_PASTO[(r / 3) as usize % MARCAS_PASTO.len()];
                let Some(img) = art.ui_icon(clave) else { continue };
                v.spawn((caja(cx as f32 * celda, cy as f32 * celda, celda, celda, k), img));
            }
        }

        // La luz del cuadrito: la moldura de arriba hace sombra sobre el pasto.
        // Dos escalones (no un degradado): es como sombrea el propio pack.
        for (alto, alfa) in [(h * 0.30, 0.10), (h * 0.14, 0.10)] {
            v.spawn((caja(0.0, 0.0, w, alto, k), BackgroundColor(Color::srgba(0.06, 0.09, 0.05, alfa))));
        }

        // **Cada prop entra entero.** Antes iban a x2 fijo: un árbol de 128 px
        // medía 256 en una viñeta de 120 y sólo asomaba la base del tronco.
        let medidas: Vec<(ImageNode, f32, f32)> = props
            .iter()
            .filter_map(|clave| {
                let px = art.ui_icon_px(clave)?;
                let img = art.ui_icon(clave)?;
                let e = escala_que_entra(px, escala, w, h, props.len());
                Some((img, px.x * e, px.y * e))
            })
            .collect();
        let total: f32 = medidas.iter().map(|m| m.1).sum();
        let hueco = ((w - total) / (medidas.len() as f32 + 1.0)).max(4.0);
        let mut cursor = hueco;
        for (img, pw, ph) in medidas {
            let (px_x, px_y) = if separados {
                (cursor, h - ph - 8.0)
            } else {
                let rx = (azar() % 1000) as f32 / 1000.0;
                let ry = (azar() % 14) as f32;
                (((w - pw).max(1.0) * rx).min(w - pw), h - ph - 6.0 - ry)
            };
            cursor += pw + hueco;
            // Sombra de contacto: la misma elipse translúcida que trae el pack
            // debajo de cada objeto; sin ella el prop flota sobre el pasto.
            sombra(v, px_x + pw / 2.0, px_y + ph - 7.0, pw * 0.36, 5.0, k);
            v.spawn((caja(px_x, px_y, pw, ph, k), img));
        }
    });
}

/// Las variantes de pasto con marca que siembra el mundo (ver `vineta`).
const MARCAS_PASTO: [&str; 4] = ["tiles/grass#m1", "tiles/grass#m2", "tiles/grass#m3", "tiles/grass#m4"];

/// **La escala a la que un prop entra en su viñeta.** Nunca más grande que la
/// pedida, nunca más alto que el cuadro (con aire para la sombra) ni más ancho
/// que su parte del ancho. Se prefieren escalas enteras —el pixel queda
/// parejo— y sólo si ni x1 entra se achica lo justo.
pub fn escala_que_entra(px: Vec2, pedida: f32, w: f32, h: f32, cuantos: usize) -> f32 {
    if px.x <= 0.0 || px.y <= 0.0 {
        return pedida;
    }
    let tope_alto = (h - 12.0).max(8.0) / px.y;
    let tope_ancho = (w * 0.9 / cuantos.max(1) as f32) / px.x;
    let tope = tope_alto.min(tope_ancho).min(pedida);
    if tope >= 1.0 {
        tope.floor()
    } else {
        tope
    }
}
