//! Pantalla **CREA TU PERSONAJE** — se muestra en `GamePhase::CharacterCreate`,
//! entre "Nueva partida" y el gameplay. El mundo todavía NO existe (no hay Player
//! → no hay worldgen); recién al tocar JUGAR se pasa a `GamePhase::Loading`, que
//! arma el mundo detrás de la cortina y compone tu personaje.
//!
//! Es el **generador por capas** del pack de LimeZu metido adentro del juego: piel,
//! ojos, ropa (+color), pelo (+color) y gorro/accesorio (+color) se eligen acá y
//! salen de `crate::chargen`. La vista previa NO compone nada: son las cinco capas
//! apiladas como cinco `ImageNode`, así tocar `>` es instantáneo y gratis. La
//! composición a UNA textura ocurre una sola vez, en la pantalla de carga.
//!
//! Al lado del muñeco va tu **retrato**: la misma cara con la que vas a aparecer en
//! los diálogos. Elegís mirándote como te van a ver.
//!
//! Texto ASCII (la fuente del juego no dibuja acentos ni flechas — regla 6).

use bevy::input::keyboard::{Key, KeyboardInput};
use bevy::input::ButtonState;
use bevy::prelude::*;
use ha_shared::Ps1Retro;
use ha_assets::chargen_index::{char_anim, char_cell};

use crate::chargen::{portrait_for, Chargen, ChargenJobs, Layer, LookDef};
use crate::game_phase::{save_savegame, GamePhase, SelectedCharacter};

const NEON_CYAN: Color = Color::srgb(Ps1Retro::TEXT[0], Ps1Retro::TEXT[1], Ps1Retro::TEXT[2]);
const NEON_MAGENTA: Color = Color::srgb(Ps1Retro::SELECT[0], Ps1Retro::SELECT[1], Ps1Retro::SELECT[2]);
const DIM: Color = Color::srgb(0.7, 0.75, 0.85);
const VOID: Color = Color::srgb(Ps1Retro::VOID[0], Ps1Retro::VOID[1], Ps1Retro::VOID[2]);
/// Fondo de las tarjetas que agrupan la pantalla. Un escalón por encima del
/// fondo: lo que agrupa se tiene que VER agrupado, y un borde solo no alcanza.
const CARD_BG: Color = Color::srgb(0.07, 0.09, 0.14);
const CARD_EDGE: Color = Color::srgba(Ps1Retro::PANEL_HI[0], Ps1Retro::PANEL_HI[1], Ps1Retro::PANEL_HI[2], 0.55);
const NAME_MAX: usize = 16;

/// Capas que se pueden elegir, en el orden en que aparecen las filas.
const ROWS: [Layer; 5] = [Layer::Body, Layer::Eyes, Layer::Outfit, Layer::Hair, Layer::Accessory];

/// **La vitrina: las cuatro direcciones, caminando en el lugar.**
///
/// Antes se veía UNA pose quieta de perfil derecho (la celda 0 de la hoja) y era
/// engañoso: elegías un personaje mirándolo de costado y en el juego lo ves casi
/// siempre de frente o de espaldas, que es donde se nota el pelo, el gorro y la
/// cara. Ahora se ven las cuatro al mismo tiempo y en movimiento — que es como
/// vas a verlo jugando.
///
/// El número es el índice de dirección de la hoja del pack, que va **`R, U, L, D`**
/// (ver `ha_assets::chargen_index::AnimDirs`), y el orden de la fila es un giro
/// completo: de frente, a la derecha, de espaldas, a la izquierda.
const SHOWCASE: [(u32, &str); 4] =
    [(3, "DE FRENTE"), (0, "DERECHA"), (1, "DE ESPALDAS"), (2, "IZQUIERDA")];

/// La animación que se muestra en la vitrina. `walk` y no `idle` porque caminar
/// es lo que hace el personaje el 90 % del tiempo de juego.
const SHOWCASE_ANIM: &str = "walk";

pub struct CharacterCreatePlugin;

impl Plugin for CharacterCreatePlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<CcShot>()
            .init_resource::<KeyboardOpen>()
            .init_resource::<GridLayer>()
            .init_resource::<Toque>()
            .init_resource::<Miniaturas>()
            .init_resource::<CcSheetLayout>()
            .add_systems(Update, cc_autoshot)
            .add_systems(OnEnter(GamePhase::CharacterCreate), spawn_screen)
            .add_systems(OnExit(GamePhase::CharacterCreate), despawn_screen)
            .add_systems(
                Update,
                (name_input, button_system, resolver_toque, seguir_pestana, refresh, sync_grid, sincronizar_colores, animate_showcase, sync_keyboard, scroll_grid)
                    .chain()
                    .run_if(in_state(GamePhase::CharacterCreate)),
            )
            // DESPUES DEL LAYOUT, no en `Update`. Los `GlobalTransform` de la UI
            // los escribe el layout de `PostUpdate`: mirandolos en `Update` todas
            // las celdas recien creadas valen y=0, o sea que TODAS parecen
            // visibles y se cargaban las 132 hojas igual que antes — 1,6 GB y
            // SIGKILL. Este orden es la mitad del arreglo, no un detalle.
            .add_systems(
                PostUpdate,
                (recortar_miniaturas, stream_thumbs, mover_pulgar)
                    .chain()
                    .after(bevy::ui::UiSystem::Layout)
                    .run_if(in_state(GamePhase::CharacterCreate)),
            );
    }
}

/// QA de dev: `HA_CCSHOT=<prefijo>` entra al creador, lo captura, toca JUGAR y
/// captura también la pantalla de carga; después sale. Es la forma de mirar las
/// dos pantallas sin el teléfono (ver skill `swarm-sim`).
#[derive(Resource, Default)]
struct CcShot {
    frames: u32,
    done: bool,
}

fn cc_autoshot(
    mut shot: ResMut<CcShot>,
    mut commands: Commands,
    mut next: ResMut<NextState<GamePhase>>,
    mut exit: EventWriter<bevy::app::AppExit>,
    mut keyboard: ResMut<KeyboardOpen>,
    mut capa: ResMut<GridLayer>,
) {
    let Ok(prefix) = crate::flags::var("HA_CCSHOT") else { return };
    if shot.done {
        return;
    }
    shot.frames += 1;
    let mut foto = |nombre: &str| {
        commands
            .spawn(bevy::render::view::screenshot::Screenshot::primary_window())
            .observe(bevy::render::view::screenshot::save_to_disk(format!("{prefix}_{nombre}.png")));
    };
    // Los tiempos son GENEROSOS a propósito: las cinco capas del muñeco son PNG
    // que se decodifican en hilos de IO, y disparando a los 30 frames de entrar
    // la captura salía con la vitrina vacía — parecía un bug de layout y era la
    // foto sacada antes de que llegara el arte.
    //
    // Cada pestaña de la cuadrícula sale en su propia foto: los ojos, el pelo
    // y sus colores son los que más se rompían al importar.
    match shot.frames {
        10 => next.set(GamePhase::CharacterCreate),
        100 => foto("creacion"),
        105 => capa.0 = Layer::Body,
        135 => foto("piel"),
        140 => capa.0 = Layer::Eyes,
        170 => foto("ojos"),
        175 => capa.0 = Layer::Hair,
        205 => foto("pelo"),
        210 => capa.0 = Layer::Accessory,
        240 => foto("accesorios"),
        245 => capa.0 = Layer::Outfit,
        // El teclado en pantalla: es la única forma de escribir el nombre en el
        // teléfono, así que entra en la corrida de QA como una pantalla más.
        260 => keyboard.0 = true,
        280 => foto("teclado"),
        300 => keyboard.0 = false,
        320 => next.set(GamePhase::Loading),
        345 => foto("carga"),
        f if f >= 380 => {
            shot.done = true;
            exit.send(bevy::app::AppExit::Success);
        }
        _ => {}
    }
}

#[derive(Component)]
struct CcRoot;
#[derive(Component)]
struct NameText;
/// Una de las cinco capas apiladas de un muñeco de la vitrina. `dir` es la
/// dirección de la hoja (`R,U,L,D`) que le toca a ese muñeco.
#[derive(Component, Clone, Copy)]
struct PreviewLayer {
    layer: Layer,
    dir: u32,
}
#[derive(Component)]
struct PortraitPreview;
/// El globo donde el personaje opina sobre lo que le estas haciendo.
#[derive(Component)]
struct PortraitQuip;
/// La raiz del teclado en pantalla (existe solo mientras esta abierto).
#[derive(Component)]
struct CcKeyboard;
/// El contenedor de la cuadricula de opciones: se vacia y se rellena cada vez
/// que cambias de capa o de eleccion.
#[derive(Component)]
struct CcGrid;
/// Una celda de la cuadricula. Guarda a que opcion corresponde para poder
/// marcar la elegida sin reconstruir.
#[derive(Component, Clone, Copy)]
struct CcCell(Layer, Option<u32>);

/// Miniaturas ya recortadas, por ruta de hoja.
///
/// **El desperdicio que esto arregla:** para dibujar una prenda en la
/// cuadricula hace falta UNA celda de 32x64 px (8 KB), y se estaba cargando la
/// hoja entera de 1792x1280 (9,2 MB) para leer esa celda. Mil veces mas memoria
/// de la necesaria, multiplicado por cuantas prendas se vean. Ese es el grueso
/// de por que el juego inflaba tanto la RAM del telefono y Android terminaba
/// matando lo que tuviera al lado — Tailscale incluido.
///
/// Ahora la hoja se carga UNA vez, se le copia la celda a una imagen chica, y la
/// hoja se suelta. Lo que queda vivo son los 8 KB.
#[derive(Resource, Default)]
struct Miniaturas {
    hechas: std::collections::HashMap<String, Handle<Image>>,
    /// Hojas pedidas y todavia sin recortar (se sueltan al recortarlas).
    pendientes: std::collections::HashMap<String, Handle<Image>>,
}

/// Recorta a miniatura las hojas que ya terminaron de cargar.
fn recortar_miniaturas(
    cg: Res<Chargen>,
    mut mini: ResMut<Miniaturas>,
    mut imagenes: ResMut<Assets<Image>>,
) {
    if mini.pendientes.is_empty() {
        return;
    }
    let (cw, ch) = (cg.index.cell[0], cg.index.cell[1]);
    let cols = cg.index.cols.max(1);
    let idx = grid_cell_index() as u32;
    let (col, fila) = (idx % cols, idx / cols);
    let listas: Vec<String> = mini
        .pendientes
        .iter()
        .filter(|(_, h)| imagenes.get(*h).is_some())
        .map(|(k, _)| k.clone())
        .collect();
    for ruta in listas {
        let Some(h) = mini.pendientes.remove(&ruta) else { continue };
        let Some(hoja) = imagenes.get(&h) else { continue };
        if let Some(chica) = recortar(hoja, col * cw, fila * ch, cw, ch) {
            let handle = imagenes.add(chica);
            mini.hechas.insert(ruta, handle);
        }
        // Soltar la hoja grande: es la mitad que importa.
        drop(h);
    }
}

/// Copia un rectangulo de una imagen RGBA a una imagen nueva.
fn recortar(src: &Image, x: u32, y: u32, w: u32, h: u32) -> Option<Image> {
    let ancho = src.texture_descriptor.size.width;
    let alto = src.texture_descriptor.size.height;
    if x + w > ancho || y + h > alto {
        return None;
    }
    let datos = src.data.as_slice();
    let bpp = 4usize;
    let mut out = vec![0u8; (w * h) as usize * bpp];
    for fila in 0..h {
        let o_src = (((y + fila) * ancho + x) as usize) * bpp;
        let o_dst = (fila * w) as usize * bpp;
        let n = w as usize * bpp;
        if o_src + n > datos.len() {
            return None;
        }
        out[o_dst..o_dst + n].copy_from_slice(&datos[o_src..o_src + n]);
    }
    Some(Image::new(
        bevy::render::render_resource::Extent3d { width: w, height: h, depth_or_array_layers: 1 },
        bevy::render::render_resource::TextureDimension::D2,
        out,
        src.texture_descriptor.format,
        bevy::asset::RenderAssetUsages::RENDER_WORLD | bevy::asset::RenderAssetUsages::MAIN_WORLD,
    ))
}

/// El hueco de la celda donde `stream_thumbs` mete y saca el `ImageNode`.
#[derive(Component)]
struct CcThumbSlot;

/// La hoja que le toca a esta celda, SIN cargar todavia.
///
/// Cada opcion del chargen es una hoja de 1792x1280 = **9,2 MB de textura**, y
/// hay capas de 200 (PELO) y 132 (ROPA). Cargarlas todas al abrir la pantalla
/// pedia ~1,2 GB de golpe en un telefono de 3,8 GB con 700 MB libres: el
/// `lowmemorykiller` de Android mataba el juego a los pocos segundos de tocar
/// NUEVA PARTIDA, sin panico ni segfault (SIGKILL pelado, por eso no dejaba
/// rastro en `ha-crash`).
///
/// Asi que la celda guarda la RUTA y `stream_thumbs` carga la textura sólo
/// mientras la celda esta a la vista, y la suelta cuando sale. Con 3 columnas se
/// sostienen ~12-20 hojas en vez de 132.
#[derive(Component)]
struct CcThumb(String);
/// La ficha de una capa en la fila de arriba de la cuadricula.
#[derive(Component, Clone, Copy)]
struct CcLayerTab(Layer);

/// El recuadro de "elegida" de una pestaña de capa.
#[derive(Component, Clone, Copy)]
struct ElegidoDeTab(Layer);

/// **Las pestañas y los rótulos siguen a la capa activa.** El recuadro, el
/// marco de la ficha, "CAPA: …" arriba y "ROPA 24" debajo del nombre se armaban
/// una vez: al tocar OJOS la grilla cambiaba pero todo lo demás seguía diciendo
/// ROPA (auditoría del 29 sep 2026).
fn seguir_pestana(
    mut commands: Commands,
    capa: Res<GridLayer>,
    art: Res<crate::art2d::ArtLibrary>,
    mut recuadros: Query<(&ElegidoDeTab, &mut Visibility)>,
    mut fichas: Query<(&CcLayerTab, &mut ImageNode)>,
    raiz: Query<Entity, With<CcRoot>>,
) {
    if !capa.is_changed() {
        return;
    }
    for (e, mut v) in &mut recuadros {
        *v = if e.0 == capa.0 { Visibility::Inherited } else { Visibility::Hidden };
    }
    for (t, mut img) in &mut fichas {
        let clave = if t.0 == capa.0 { crate::ui_pack::marco::SLOT } else { crate::ui_pack::marco::SLOT_CHICO };
        if let Some(nuevo) = art.ui_frame_node(clave) {
            img.image = nuevo.image;
            img.rect = nuevo.rect;
            img.image_mode = nuevo.image_mode;
        }
    }
    for r in &raiz {
        commands.entity(r).try_insert(Dirty);
    }
}

/// Que capa se esta eligiendo en la cuadricula. Es lo unico que la pantalla
/// recuerda aparte del look.
#[derive(Resource)]
struct GridLayer(Layer);

impl Default for GridLayer {
    fn default() -> Self {
        // Arranca en ROPA: es lo que mas se mira al armar un personaje.
        Self(Layer::Outfit)
    }
}
#[derive(Component, Clone, Copy)]
struct StyleLabel(Layer);
#[derive(Component, Clone, Copy)]
struct ColorLabel(Layer);
/// Marca que el look cambió y hay que repintar la vista previa.
#[derive(Component)]
struct Dirty;

#[derive(Component, Clone, Copy)]
enum CcAction {
    Play,
    Back,
    /// Sorteo completo — el botón que más se usa en un generador.
    Random,
    /// Cambia el ESTILO de una capa (`-1` / `+1`).
    Style(Layer, i32),
    /// Cambia el COLOR dentro del estilo.
    Color(Layer, i32),
    /// Cambia la capa que muestra la cuadricula (PIEL/OJOS/ROPA/PELO/GORRO).
    PickLayer(Layer),
    /// Elige una opcion de la cuadricula por su indice en el catalogo de la capa.
    /// `None` = NINGUNO (solo accesorio).
    PickOption(Layer, Option<u32>),
    /// Abre el teclado en pantalla para escribir el nombre.
    OpenKeyboard,
    /// Una tecla del teclado en pantalla.
    Key(char),
    /// Borrar el ultimo caracter.
    KeyBack,
    /// Cerrar el teclado.
    KeyDone,
    /// Volver al look con el que se abrió la pantalla (el botón REINICIAR).
    Reset,
    /// Elegir un color del estilo actual, por su posición dentro del estilo.
    PickColor(Layer, usize),
}

// ── el look como (estilo, color) ────────────────────────────────────────────
//
// El índice plano del catálogo (`pelo[137]`) es lo que se guarda, pero elegir se
// hace con dos ruedas: el corte de pelo y su color. Estas funciones traducen entre
// las dos vistas; la agrupación la da `Chargen::styles_of`.

fn index_of(look: &LookDef, layer: Layer) -> Option<u32> {
    match layer {
        Layer::Body => Some(look.body),
        Layer::Eyes => Some(look.eyes),
        Layer::Outfit => Some(look.outfit),
        Layer::Hair => Some(look.hair),
        Layer::Accessory => look.accessory,
    }
}

fn set_index(look: &mut LookDef, layer: Layer, idx: Option<u32>) {
    match layer {
        Layer::Body => look.body = idx.unwrap_or(0),
        Layer::Eyes => look.eyes = idx.unwrap_or(0),
        Layer::Outfit => look.outfit = idx.unwrap_or(0),
        Layer::Hair => look.hair = idx.unwrap_or(0),
        Layer::Accessory => look.accessory = idx,
    }
}

/// `(estilo, color)` actuales de una capa. En accesorio, estilo `0` = NINGUNO.
fn style_color(cg: &Chargen, look: &LookDef, layer: Layer) -> (usize, usize) {
    let groups = cg.styles_of(layer);
    let Some(idx) = index_of(look, layer).map(|i| i as usize) else {
        return (0, 0);
    };
    if groups.is_empty() {
        return (idx, 0);
    }
    for (g, group) in groups.iter().enumerate() {
        if let Some(pos) = group.iter().position(|&i| i == idx) {
            let g = if layer == Layer::Accessory { g + 1 } else { g };
            return (g, pos);
        }
    }
    (0, 0)
}

/// Mueve el estilo (o el índice plano, en piel/ojos) `delta` pasos, con vuelta.
fn cycle_style(cg: &Chargen, look: &mut LookDef, layer: Layer, delta: i32) {
    let groups = cg.styles_of(layer);
    if groups.is_empty() {
        let n = cg.options(layer).len();
        if n == 0 {
            return;
        }
        let cur = index_of(look, layer).unwrap_or(0) as i32;
        let next = (cur + delta).rem_euclid(n as i32) as u32;
        set_index(look, layer, Some(next));
        return;
    }
    // El accesorio suma NINGUNO al principio: "sin gorro" es una opción, no un
    // hueco (la mayoría de la gente no usa uno).
    let extra = if layer == Layer::Accessory { 1 } else { 0 };
    let (style, color) = style_color(cg, look, layer);
    let total = groups.len() as i32 + extra;
    let next = (style as i32 + delta).rem_euclid(total);
    if extra == 1 && next == 0 {
        set_index(look, layer, None);
        return;
    }
    let group = &groups[(next - extra) as usize];
    set_index(look, layer, Some(group[color.min(group.len() - 1)] as u32));
}

/// Mueve el color dentro del estilo actual.
fn cycle_color(cg: &Chargen, look: &mut LookDef, layer: Layer, delta: i32) {
    let groups = cg.styles_of(layer);
    if groups.is_empty() {
        return;
    }
    let extra = if layer == Layer::Accessory { 1 } else { 0 };
    let (style, color) = style_color(cg, look, layer);
    if extra == 1 && style == 0 {
        return; // NINGUNO no tiene color
    }
    let Some(group) = groups.get(style - extra) else { return };
    if group.is_empty() {
        return;
    }
    let next = (color as i32 + delta).rem_euclid(group.len() as i32) as usize;
    set_index(look, layer, Some(group[next] as u32));
}

fn style_text(cg: &Chargen, look: &LookDef, layer: Layer) -> String {
    match index_of(look, layer).and_then(|i| cg.options(layer).get(i as usize)) {
        Some(opt) => opt.label.clone(),
        None => "NINGUNO".into(),
    }
}

fn color_text(cg: &Chargen, look: &LookDef, layer: Layer) -> String {
    let (_, color) = style_color(cg, look, layer);
    match index_of(look, layer) {
        Some(_) if !cg.styles_of(layer).is_empty() => format!("{}", color + 1),
        _ => "-".into(),
    }
}

// ── pantalla ────────────────────────────────────────────────────────────────

#[allow(clippy::too_many_arguments)]
/// **El creador de personaje, tal cual la imagen de referencia.**
///
/// A la izquierda el personaje grande con su nombre, sus tres poses y su claro
/// de luz; a la derecha el panel con las cinco pestañas, la grilla de ESTILOS,
/// la fila de COLORES y los tres botones. El fondo es el mismo bosque del menú
/// pero de noche, con faroles y luciérnagas — el creador es otro sitio.
///
/// Cada número sale de `mock_menu::crear_personaje`.
#[allow(clippy::too_many_arguments)]
fn spawn_screen(
    mut commands: Commands,
    asset_server: Res<AssetServer>,
    mut layouts: ResMut<Assets<TextureAtlasLayout>>,
    cg: Res<Chargen>,
    mut selected: ResMut<SelectedCharacter>,
    touch: Res<crate::input::TouchLayout>,
    art: Res<crate::art2d::ArtLibrary>,
    halo_tex: Res<crate::ui_pack::HaloTex>,
    grid_layer: Res<GridLayer>,
    mut sheet_res: ResMut<CcSheetLayout>,
) {
    use crate::ui_pack::{self as up, Ancla, Pack};

    // Sin look previo (partida nueva) se arranca con uno sorteado: la pantalla
    // nunca abre con un maniquí vacío.
    let look = *selected.look.get_or_insert_with(|| cg.index.look_from_seed(rand_seed()));
    if selected.name.trim().is_empty() {
        selected.name = "Viajero".into();
    }
    let nombre = selected.name.clone();

    let cell = UVec2::new(cg.index.cell[0], cg.index.cell[1]);
    let sheet_layout = layouts.add(TextureAtlasLayout::from_grid(
        cell,
        cg.index.cols.max(1),
        cg.index.rows.max(1),
        None,
        None,
    ));
    sheet_res.0 = sheet_layout.clone();

    let k = up::k(&touch);
    let capa = grid_layer.0;
    let cel = Vec2::new(cell.x as f32, cell.y as f32);

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
            BackgroundColor(VOID),
            GlobalZIndex(1000),
            bevy::ui::FocusPolicy::Block,
            CcRoot,
            Name::new("CharacterCreate"),
        ))
        .with_children(|root| {
            crate::marca::bosque::fondo(root, &art, crate::marca::Ambiente::Creador, k);
            root.spawn(up::lienzo(k)).with_children(|l| {
                // ── la barra de arriba ────────────────────────────────────
                up::barra_estado(
                    l,
                    &art,
                    up::CUERPO_TIERRA,
                    &[
                        up::Ficha {
                            icono: "ui/ic_lista",
                            etiqueta: "HUECOS",
                            valor: format!(
                                "{} de {}",
                                crate::personajes::listar().len(),
                                crate::personajes::MAXIMO
                            ),
                        },
                        up::Ficha {
                            icono: "ui/ic_grilla",
                            etiqueta: "CAPA",
                            valor: etiqueta_capa(capa).to_string(),
                        },
                        up::Ficha {
                            icono: "ui/ic_persona",
                            etiqueta: "NOMBRE",
                            valor: nombre.clone(),
                        },
                    ],
                    k,
                );

                // ── el personaje ──────────────────────────────────────────
                let cx = 560.0_f32;
                up::halo(l, &halo_tex, cx, 650.0, 300.0, k);
                up::sombra(l, cx, 838.0, 108.0, 18.0, k);
                muneco_animado(l, &look, &cg, &asset_server, &sheet_layout, 3,
                               cx - cel.x * 7.0 / 2.0, 860.0 - cel.y * 7.0, 7.0, k);
                for (i, (dir, et)) in
                    [(0u32, "DERECHA"), (1, "DE ESPALDAS"), (2, "IZQUIERDA")].into_iter().enumerate()
                {
                    let x = cx - 216.0 + i as f32 * 156.0;
                    up::marco(l, &art, up::marco::SLOT, x, 890.0, 132.0, 144.0, k);
                    muneco_animado(l, &look, &cg, &asset_server, &sheet_layout, dir,
                                   x + 66.0 - cel.x, 898.0, 2.0, k);
                    up::texto(l, et, 22.0, Color::srgb(0.839, 0.792, 0.729), x + 66.0, 1056.0,
                              Ancla::Medio, k);
                }
                // El nombre se toca para cambiarlo.
                // El nombre se toca para cambiarlo. `SinMarco` porque este botón
                // es invisible a propósito: es el área del texto, y sin la marca
                // el reskin del HUD le pinta un botón de madera detrás.
                l.spawn((
                    up::caja(cx - 300.0, 158.0, 600.0, 56.0, k),
                    Button,
                    crate::ui_pack::SinMarco,
                    CcAction::OpenKeyboard,
                ));
                nombre_grande(l, &nombre, cx, 186.0, k);
                up::texto(l, "tocá el nombre para cambiarlo", 26.0,
                          Color::srgb(0.776, 0.729, 0.667), cx, 238.0, Ancla::Medio, k);
                etiqueta_estilo(l, &cg, &look, capa, cx, 292.0, k);

                // ── el panel ──────────────────────────────────────────────
                let (pxu, pyu, pw, ph) = (1170.0_f32, 186.0_f32, 1194.0_f32, 828.0_f32);
                up::marco(l, &art, up::marco::PANEL, pxu, pyu, pw, ph, k);
                let (ix, iy) = (pxu + 66.0, pyu + 54.0);
                let inner_w = pw - 132.0;

                // Las cinco pestañas.
                let tab_w = (inner_w - 4.0 * 9.0) / 5.0;
                for (i, cap) in ROWS.into_iter().enumerate() {
                    let tx = ix + i as f32 * (tab_w + 9.0);
                    let act = cap == capa;
                    let marco = if act { up::marco::SLOT } else { up::marco::SLOT_CHICO };
                    up::marco(l, &art, marco, tx, iy, tab_w, 84.0, k)
                        .insert((Button, CcAction::PickLayer(cap), CcLayerTab(cap)));
                    up::texto(l, etiqueta_capa(cap), 28.0, Pack::TINTA, tx + tab_w / 2.0,
                              iy + 44.0, Ancla::Medio, k);
                    // El recuadro de la pestaña elegida existe en las cinco y se
                    // prende en la activa (`seguir_pestana`): antes se dibujaba al
                    // armar la pantalla y al cambiar de capa se quedaba en ROPA.
                    l.spawn((
                        Node {
                            position_type: PositionType::Absolute,
                            left: Val::Px((tx - 3.0) * k),
                            top: Val::Px((iy - 3.0) * k),
                            width: Val::Px(tab_w * k),
                            height: Val::Px(84.0 * k),
                            border: UiRect::all(Val::Px(6.0 * k)),
                            ..default()
                        },
                        BorderColor(Pack::ACENTO),
                        ZIndex(8),
                        if act { Visibility::Inherited } else { Visibility::Hidden },
                        ElegidoDeTab(cap),
                    ));
                }

                // La grilla de estilos: siete por tres, con su canaleta.
                let gy = iy + 84.0 + 21.0;
                let grid_w = COLS_GRID as f32 * LADO_GRID
                    + (COLS_GRID as f32 - 1.0) * SEP_GRID;
                let grid_h = FILAS_GRID as f32 * LADO_GRID
                    + (FILAS_GRID as f32 - 1.0) * SEP_GRID;
                let mut vista = up::caja(ix, gy, grid_w, grid_h, k);
                vista.overflow = Overflow::scroll_y();
                l.spawn((vista, CcScroll, Name::new("CcVista"))).with_children(|v| {
                    v.spawn((
                        Node {
                            // **Siete columnas EXACTAS.** Con `flex_wrap` la
                            // séptima casilla se caía a la fila de abajo: 7 × 130
                            // + 6 × 12 da 982 clavados y basta que taffy redondee
                            // un pixel para que no entre. Una grilla declarada no
                            // depende del redondeo.
                            display: Display::Grid,
                            width: Val::Px(grid_w * k + 2.0),
                            grid_template_columns: vec![RepeatedGridTrack::px(
                                COLS_GRID as u16,
                                LADO_GRID * k,
                            )],
                            column_gap: Val::Px(SEP_GRID * k),
                            row_gap: Val::Px(SEP_GRID * k),
                            ..default()
                        },
                        crate::hud::ScrollContent,
                        CcGrid,
                        Name::new("CcGrid"),
                    ));
                });
                // La canaleta. El pulgar lo mueve `mover_pulgar`.
                let sx = ix + grid_w + 14.0;
                up::marco(l, &art, up::marco::HUECO, sx, gy, 24.0, grid_h, k);
                l.spawn((
                    up::caja(sx + 6.0, gy, 12.0, 90.0, k),
                    BackgroundColor(Color::srgb(0.690, 0.463, 0.322)),
                    PulgarScroll { x: sx + 6.0, y0: gy, alto: grid_h, k },
                ));

                // La fila de COLORES y el contador.
                let cy = gy + grid_h + 20.0;
                l.spawn((up::caja(ix, cy, inner_w, 62.0, k), FilaColores { ix, cy, k },
                         Name::new("FilaColores")));
                up::texto(l, "COLOR", 26.0, Color::srgb(0.839, 0.769, 0.659), ix, cy + 30.0,
                          Ancla::Izq, k);
                contador_estilos(l, &cg, &look, capa, ix + inner_w, cy + 30.0, k);

                // Los tres botones.
                let by = pyu + ph - 54.0 - 84.0;
                let anchos: [(f32, &str, bool, CcAction); 3] = [
                    (300.0, "ALEATORIO", false, CcAction::Random),
                    (300.0, "REINICIAR", false, CcAction::Reset),
                    (390.0, "CONFIRMAR", true, CcAction::Play),
                ];
                let tot: f32 = anchos.iter().map(|a| a.0).sum::<f32>() + 24.0 * 2.0;
                let mut bx = ix + inner_w - tot;
                for (ancho, et, fuerte, accion) in anchos {
                    up::boton(l, &art, bx, by, ancho, 84.0, Some(et), None, fuerte, 34.0,
                              accion, k);
                    bx += ancho + 24.0;
                }
                up::cartel(l, &art, "CREAR PERSONAJE", pxu + pw / 2.0, pyu - 48.0, 640.0, 96.0,
                           44.0, k);
                // VOLVER, en la franja libre entre la barra y el panel.
                up::boton(l, &art, 40.0, 134.0, 200.0, 76.0, Some("VOLVER"), None, false,
                          30.0, CcAction::Back, k);
            });
        });
}

/// **El nombre de una capa en la interfaz.** `Layer::label` dice "GORRO" y las
/// imágenes de referencia dicen ACCESORIOS: la pestaña incluye vinchas, barbas y
/// máscaras, no sólo gorros.
fn etiqueta_capa(capa: Layer) -> &'static str {
    match capa {
        Layer::Accessory => "ACCESORIOS",
        otra => otra.label(),
    }
}

/// Cuántas casillas entran de ancho y de alto en la grilla del mockup.
const COLS_GRID: usize = 7;
const FILAS_GRID: usize = 3;
/// El lado de una casilla y la separación, en coordenadas del mockup.
const LADO_GRID: f32 = 130.0;

/// **Dónde está la cabeza en la celda de 32×64, medido** (29 sep 2026). Se medía
/// "de la fila 4 a la 30" y estaba mal: en el cuadro que usa la grilla
/// (`walk`, de frente) el pelo empieza en la fila 18 y el mentón termina en la 44
/// (bbox de `cuerpo/000` + `pelo/000`), así que el centro es la 31. Con 17 el
/// retrato encuadraba el aire sobre la cabeza: sólo se veía la punta del pelo y
/// los ojos quedaban cortados abajo.
const CENTRO_CABEZA: f32 = 31.0;

/// El centro vertical de lo que muestra cada capa en su muestra de COLOR, en
/// filas de la celda. Medido igual: pelo 18-36, ropa 44-62, accesorio 12-46.
fn centro_de_muestra(capa: Layer) -> f32 {
    match capa {
        Layer::Hair => 27.0,
        Layer::Outfit => 53.0,
        Layer::Accessory => 29.0,
        _ => CENTRO_CABEZA,
    }
}
const SEP_GRID: f32 = 12.0;

/// El pulgar de la canaleta: se mueve siguiendo el scroll de la grilla.
#[derive(Component)]
struct PulgarScroll {
    x: f32,
    y0: f32,
    alto: f32,
    k: f32,
}

/// El nodo que contiene la fila de colores; se rehace al cambiar de estilo.
#[derive(Component)]
struct FilaColores {
    ix: f32,
    cy: f32,
    k: f32,
}

/// El nombre grande de arriba a la izquierda.
fn nombre_grande(l: &mut ChildBuilder, nombre: &str, cx: f32, y: f32, k: f32) {
    l.spawn(Node {
        position_type: PositionType::Absolute,
        left: Val::Px((cx - 600.0) * k),
        top: Val::Px((y - 45.0) * k),
        width: Val::Px(1200.0 * k),
        height: Val::Px(90.0 * k),
        justify_content: JustifyContent::Center,
        align_items: AlignItems::Center,
        ..default()
    })
    .with_children(|t| {
        t.spawn((
            Text::new(nombre.to_uppercase()),
            TextFont { font_size: 56.0 * k, ..default() },
            TextColor(crate::ui_pack::Pack::TEXTO),
            TextLayout::new_with_no_wrap(),
            NameText,
        ));
    });
}

/// La etiqueta de lo elegido, debajo del nombre ("PELO 10").
fn etiqueta_estilo(
    l: &mut ChildBuilder,
    cg: &Chargen,
    look: &LookDef,
    capa: Layer,
    cx: f32,
    y: f32,
    k: f32,
) {
    let (estilo, _) = style_color(cg, look, capa);
    let txt = if cg.styles_of(capa).is_empty() {
        format!("{} {}", etiqueta_capa(capa), index_of(look, capa).map(|i| i + 1).unwrap_or(0))
    } else {
        format!("{} {}", etiqueta_capa(capa), estilo + 1)
    };
    l.spawn(Node {
        position_type: PositionType::Absolute,
        left: Val::Px((cx - 600.0) * k),
        top: Val::Px((y - 28.0) * k),
        width: Val::Px(1200.0 * k),
        height: Val::Px(56.0 * k),
        justify_content: JustifyContent::Center,
        align_items: AlignItems::Center,
        ..default()
    })
    .with_children(|t| {
        t.spawn((
            Text::new(txt),
            TextFont { font_size: 34.0 * k, ..default() },
            TextColor(crate::ui_pack::Pack::ACENTO),
            TextLayout::new_with_no_wrap(),
            StyleLabel(capa),
        ));
    });
}

/// El contador "10/29" de la esquina de la grilla.
fn contador_estilos(
    l: &mut ChildBuilder,
    cg: &Chargen,
    look: &LookDef,
    capa: Layer,
    x: f32,
    y: f32,
    k: f32,
) {
    let (estilo, _) = style_color(cg, look, capa);
    let total = if cg.styles_of(capa).is_empty() {
        cg.options(capa).len()
    } else {
        cg.styles_of(capa).len()
    };
    let pos = if cg.styles_of(capa).is_empty() {
        index_of(look, capa).map(|i| i as usize + 1).unwrap_or(0)
    } else {
        estilo + 1
    };
    l.spawn(Node {
        position_type: PositionType::Absolute,
        left: Val::Px((x - 600.0) * k),
        top: Val::Px((y - 24.0) * k),
        width: Val::Px(600.0 * k),
        height: Val::Px(48.0 * k),
        justify_content: JustifyContent::FlexEnd,
        align_items: AlignItems::Center,
        ..default()
    })
    .with_children(|t| {
        t.spawn((
            Text::new(format!("{pos}/{total}")),
            TextFont { font_size: 30.0 * k, ..default() },
            TextColor(Color::srgb(0.839, 0.769, 0.659)),
            TextLayout::new_with_no_wrap(),
            ColorLabel(capa),
        ));
    });
}

/// **Un muñeco animado**, apilando las cinco capas con el atlas de la hoja.
/// Es el mismo mecanismo de la vitrina de siempre; lo que cambia es dónde se
/// planta.
#[allow(clippy::too_many_arguments)]
fn muneco_animado(
    l: &mut ChildBuilder,
    look: &LookDef,
    cg: &Chargen,
    assets: &AssetServer,
    hoja: &Handle<TextureAtlasLayout>,
    dir: u32,
    x: f32,
    y: f32,
    escala: f32,
    k: f32,
) {
    let cel = Vec2::new(cg.index.cell[0] as f32, cg.index.cell[1] as f32) * escala;
    l.spawn(crate::ui_pack::caja(x, y, cel.x, cel.y, k)).with_children(|pila| {
        for capa in ROWS {
            let opt = index_of(look, capa).and_then(|i| cg.options(capa).get(i as usize));
            pila.spawn((
                ImageNode::from_atlas_image(
                    opt.map(|o| assets.load(&o.png)).unwrap_or_default(),
                    TextureAtlas { layout: hoja.clone(), index: showcase_cell(dir, 0) },
                ),
                Node {
                    position_type: PositionType::Absolute,
                    left: Val::Px(0.0),
                    top: Val::Px(0.0),
                    width: Val::Percent(100.0),
                    height: Val::Percent(100.0),
                    ..default()
                },
                if opt.is_some() { Visibility::Inherited } else { Visibility::Hidden },
                PreviewLayer { dir, layer: capa },
            ));
        }
    });
}

/// Semilla de arranque: la hora. No hace falta determinismo acá — es el sorteo de
/// la primera cara que ves, y `AL AZAR` la cambia.
fn rand_seed() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos() as u64)
        .unwrap_or(0x5EED)
}

/// Celda de la hoja que le toca a un muñeco de la vitrina. Si el pack no trae la
/// animación cae en la celda 0, que es la pose quieta: nunca un cuadro en blanco.
fn showcase_cell(dir: u32, frame: u32) -> usize {
    match char_anim(SHOWCASE_ANIM) {
        Some(anim) => char_cell(anim, dir, frame) as usize,
        None => 0,
    }
}

/// Ancho de la columna de rótulos. Fijo a propósito: es lo que hace que las cinco
/// filas (y la del nombre) compartan una sola grilla en vez de quedar escalonadas
/// según lo largo que sea cada palabra — que era lo que se veía antes.
fn label_w(s: f32) -> f32 {
    (78.0 * s).clamp(54.0, 88.0)
}

/// Una tarjeta que agrupa una sección de la pantalla. Agrupar por proximidad y
/// fondo es lo que convierte una lista de controles sueltos en tres bloques que
/// se leen de un vistazo: quién sos, cómo te ves, cómo te llamás.
fn card<'a>(parent: &'a mut ChildBuilder, s: f32) -> EntityCommands<'a> {
    parent.spawn((
        Node {
            flex_direction: FlexDirection::Column,
            align_items: AlignItems::Center,
            row_gap: Val::Px(5.0 * s),
            padding: UiRect::all(Val::Px((10.0 * s).clamp(6.0, 14.0))),
            border: UiRect::all(Val::Px(1.0)),
            ..default()
        },
        BorderRadius::all(Val::Px((10.0 * s).clamp(6.0, 12.0))),
        BorderColor(CARD_EDGE),
        BackgroundColor(CARD_BG),
    ))
}

/// Fila `CAPA  < NOMBRE >   COLOR < n >`. La rueda de color sólo aparece donde el
/// pack la tiene (ropa, pelo, accesorio), pero el hueco se reserva igual: sin eso
/// las filas con color y las sin color arrancan en sitios distintos y la columna
/// de flechas queda serrucho.
fn selector_row(parent: &mut ChildBuilder, cg: &Chargen, look: &LookDef, layer: Layer, s: f32) {
    let has_color = !cg.styles_of(layer).is_empty();
    let value_w = (140.0 * s).clamp(98.0, 156.0);
    let color_w = (118.0 * s).clamp(84.0, 128.0);
    parent
        .spawn(Node {
            flex_direction: FlexDirection::Row,
            align_items: AlignItems::Center,
            justify_content: JustifyContent::Center,
            column_gap: Val::Px(7.0 * s),
            ..default()
        })
        .with_children(|row| {
            row.spawn(Node {
                width: Val::Px(label_w(s)),
                justify_content: JustifyContent::FlexEnd,
                ..default()
            })
            .with_children(|slot| {
                slot.spawn((
                    Text::new(format!("{}:", layer.label())),
                    TextFont { font_size: (14.0 * s).clamp(10.0, 16.0), ..default() },
                    TextColor(DIM),
                ));
            });
            small_button(row, "<", CcAction::Style(layer, -1), s);
            row.spawn((
                Node {
                    width: Val::Px(value_w),
                    justify_content: JustifyContent::Center,
                    ..default()
                },
                Name::new("CcStyle"),
            ))
            .with_children(|slot| {
                slot.spawn((
                    Text::new(style_text(cg, look, layer)),
                    TextFont { font_size: (15.0 * s).clamp(11.0, 17.0), ..default() },
                    TextColor(NEON_MAGENTA),
                    StyleLabel(layer),
                ));
            });
            small_button(row, ">", CcAction::Style(layer, 1), s);

            // El grupo del color ocupa siempre el mismo ancho: vacío en piel y
            // ojos, con sus tres controles en el resto.
            row.spawn(Node {
                width: Val::Px(color_w),
                flex_direction: FlexDirection::Row,
                align_items: AlignItems::Center,
                justify_content: JustifyContent::FlexStart,
                column_gap: Val::Px(5.0 * s),
                ..default()
            })
            .with_children(|group| {
                if !has_color {
                    return;
                }
                group.spawn((
                    Text::new("COLOR"),
                    TextFont { font_size: (11.0 * s).clamp(8.0, 13.0), ..default() },
                    TextColor(DIM),
                ));
                small_button(group, "<", CcAction::Color(layer, -1), s);
                group
                    .spawn((
                        Node {
                            min_width: Val::Px((24.0 * s).clamp(18.0, 28.0)),
                            justify_content: JustifyContent::Center,
                            ..default()
                        },
                        Name::new("CcColor"),
                    ))
                    .with_children(|slot| {
                        slot.spawn((
                            Text::new(color_text(cg, look, layer)),
                            TextFont { font_size: (15.0 * s).clamp(11.0, 17.0), ..default() },
                            TextColor(NEON_MAGENTA),
                            ColorLabel(layer),
                        ));
                    });
                small_button(group, ">", CcAction::Color(layer, 1), s);
            });
        });
}

/// Las flechas de las ruedas. **Redondas**: son el control que más se toca de la
/// pantalla y una diana redonda de 36 px se acierta con el pulgar sin mirar.
fn small_button(parent: &mut ChildBuilder, label: &str, action: CcAction, s: f32) {
    let d = (36.0 * s).clamp(30.0, 40.0);
    parent
        .spawn((
            Button,
            Node {
                width: Val::Px(d),
                height: Val::Px(d),
                border: UiRect::all(Val::Px(2.0)),
                justify_content: JustifyContent::Center,
                align_items: AlignItems::Center,
                ..default()
            },
            BorderRadius::MAX,
            BorderColor(Color::srgba(0.16, 0.94, 1.0, 0.55)),
            BackgroundColor(idle_bg(action)),
            action,
        ))
        .with_children(|b| {
            b.spawn((
                Text::new(label),
                TextFont { font_size: (19.0 * s).clamp(13.0, 21.0), ..default() },
                TextColor(NEON_CYAN),
            ));
        });
}

/// Los botones de la barra de abajo. **JUGAR se ve distinto a propósito**: fondo
/// lleno contra los otros dos delineados. Con los tres iguales (que es como
/// estaban) los tres pesan lo mismo y la acción principal de la pantalla no se
/// distingue de "volver".
fn action_button(
    parent: &mut ChildBuilder,
    label: &str,
    action: CcAction,
    color: Color,
    w: f32,
    s: f32,
    filled: bool,
) {
    let bg = if filled {
        Color::srgb(0.09, 0.30, 0.40)
    } else {
        Color::srgb(0.09, 0.12, 0.19)
    };
    parent
        .spawn((
            Button,
            Node {
                width: Val::Px(w),
                height: Val::Px((52.0 * s).clamp(40.0, 56.0)),
                border: UiRect::all(Val::Px(if filled { 3.0 } else { 2.0 })),
                justify_content: JustifyContent::Center,
                align_items: AlignItems::Center,
                ..default()
            },
            BorderRadius::all(Val::Px((10.0 * s).clamp(6.0, 12.0))),
            BorderColor(color),
            BackgroundColor(bg),
            action,
        ))
        .with_children(|b| {
            b.spawn((
                Text::new(label),
                TextFont { font_size: (20.0 * s).clamp(14.0, 23.0), ..default() },
                TextColor(if filled { Color::WHITE } else { color }),
            ));
        });
}

/// Fondo en reposo de cada botón (ver [`action_button`]).
fn idle_bg(action: CcAction) -> Color {
    match action {
        CcAction::Play | CcAction::KeyDone => Color::srgb(0.09, 0.30, 0.40),
        _ => Color::srgb(0.09, 0.12, 0.19),
    }
}

/// Camina a los cuatro muñecos de la vitrina. Sólo escribe cuando el cuadro
/// cambió: un `ImageNode` tocado se vuelve a extraer, y a 10 fps de animación
/// eso son 6 escrituras por segundo en vez de 60.
fn animate_showcase(
    time: Res<Time<bevy::time::Real>>,
    mut clock: Local<f32>,
    mut shown: Local<u32>,
    mut q: Query<(&mut ImageNode, &PreviewLayer)>,
) {
    let Some(anim) = char_anim(SHOWCASE_ANIM) else { return };
    *clock += time.delta_secs();
    let frame = (*clock * anim.fps) as u32 % anim.frames.max(1);
    if frame == *shown {
        return;
    }
    *shown = frame;
    for (mut node, preview) in &mut q {
        if let Some(atlas) = node.texture_atlas.as_mut() {
            atlas.index = showcase_cell(preview.dir, frame);
        }
    }
}

fn despawn_screen(
    mut commands: Commands,
    q: Query<Entity, Or<(With<CcRoot>, With<CcKeyboard>)>>,
    mut keyboard: ResMut<KeyboardOpen>,
) {
    keyboard.0 = false;
    for e in &q {
        commands.entity(e).despawn_recursive();
    }
}

/// Escribe el nombre con el teclado (desktop). Enter = JUGAR.
fn name_input(
    mut events: EventReader<KeyboardInput>,
    mut selected: ResMut<SelectedCharacter>,
    mut text: Query<&mut Text, With<NameText>>,
    mut next: ResMut<NextState<GamePhase>>,
) {
    let Ok(mut label) = text.get_single_mut() else {
        return;
    };
    let mut changed = false;
    for ev in events.read() {
        if ev.state != ButtonState::Pressed {
            continue;
        }
        match &ev.logical_key {
            Key::Backspace => {
                selected.name.pop();
                changed = true;
            }
            Key::Space => {
                if selected.name.len() < NAME_MAX {
                    selected.name.push(' ');
                    changed = true;
                }
            }
            Key::Enter => {
                start_game(&selected);
                next.set(GamePhase::Loading);
                return;
            }
            Key::Character(s) => {
                for c in s.chars() {
                    if (c.is_ascii_alphanumeric() || c == '_' || c == '-') && selected.name.len() < NAME_MAX {
                        selected.name.push(c);
                        changed = true;
                    }
                }
            }
            _ => {}
        }
    }
    if changed {
        label.0 = selected.name.clone();
    }
}

#[allow(clippy::type_complexity)]
fn button_system(
    mut commands: Commands,
    mut interactions: Query<
        (
            &Interaction,
            &CcAction,
            Option<&mut BackgroundColor>,
            Option<&mut ImageNode>,
        ),
        Changed<Interaction>,
    >,
    cg: Res<Chargen>,
    mut selected: ResMut<SelectedCharacter>,
    mut next: ResMut<NextState<GamePhase>>,
    root: Query<Entity, With<CcRoot>>,
    mut keyboard: ResMut<KeyboardOpen>,
    mut grid_layer: ResMut<GridLayer>,
    mut name_text: Query<&mut Text, (With<NameText>, Without<PortraitQuip>)>,
    mut quip: Query<&mut Text, With<PortraitQuip>>,
    mut toque: ResMut<Toque>,
    mut inicial: Local<LookDef>,
    mut hubo_inicial: Local<bool>,
) {
    if !*hubo_inicial {
        *inicial = selected.look.unwrap_or_default();
        *hubo_inicial = true;
    }
    let mut look = selected.look.unwrap_or_default();
    let mut dirty = false;
    // Lo que el retrato va a decir este frame, si es que dice algo.
    let mut say: Option<String> = None;
    for (interaction, action, bg, img) in &mut interactions {
        // El realce: los botones del kit son marcos del pack (una imagen), así
        // que se tiñen; los que quedan del layout viejo siguen con su fondo.
        if let Some(mut img) = img {
            img.color = match interaction {
                Interaction::Pressed => Color::srgb(0.82, 0.82, 0.82),
                Interaction::Hovered => Color::srgb(0.93, 0.93, 0.93),
                Interaction::None => Color::WHITE,
            };
        }
        if let Some(mut bg) = bg {
            *bg = BackgroundColor(match interaction {
                Interaction::Pressed => Color::srgb(0.16, 0.24, 0.36),
                Interaction::Hovered => Color::srgb(0.14, 0.20, 0.30),
                Interaction::None => idle_bg(*action),
            });
        }
        match interaction {
            Interaction::Pressed => {
                match *action {
                    CcAction::Play => {
                        start_game(&selected);
                        next.set(GamePhase::Loading);
                        return;
                    }
                    CcAction::Back => {
                        next.set(GamePhase::MainMenu);
                        return;
                    }
                    CcAction::Random => {
                        look = cg.index.look_from_seed(rand_seed());
                        dirty = true;
                        say = Some(quip_from(QUIP_AZAR, rand_seed()));
                    }
                    CcAction::Style(layer, d) => {
                        cycle_style(&cg, &mut look, layer, d);
                        dirty = true;
                        say = Some(quip_for(Some(layer), rand_seed()));
                    }
                    CcAction::Color(layer, d) => {
                        cycle_color(&cg, &mut look, layer, d);
                        dirty = true;
                        say = Some(quip_for(Some(layer), rand_seed()));
                    }
                    CcAction::PickLayer(layer) => grid_layer.0 = layer,
                    // REINICIAR: vuelve al look con el que se abrió la pantalla.
                    CcAction::Reset => {
                        look = *inicial;
                        dirty = true;
                    }
                    // Un color del estilo actual, elegido tocando su muestra.
                    CcAction::PickColor(layer, pos) => {
                        let grupos = cg.styles_of(layer);
                        let (estilo, _) = style_color(&cg, &look, layer);
                        let extra = if layer == Layer::Accessory { 1 } else { 0 };
                        if let Some(grupo) = grupos.get(estilo.saturating_sub(extra)) {
                            if let Some(&idx) = grupo.get(pos) {
                                set_index(&mut look, layer, Some(idx as u32));
                                dirty = true;
                                say = Some(quip_for(Some(layer), rand_seed()));
                            }
                        }
                    }
                    // NO se elige al apoyar el dedo: se anota como candidata y
                    // `resolver_toque` la aplica al soltar, si no hubo arrastre.
                    CcAction::PickOption(layer, idx) => {
                        toque.candidata = Some((layer, idx));
                        toque.recorrido = 0.0;
                    }
                    CcAction::OpenKeyboard => {
                        keyboard.0 = true;
                        // El nombre sorteado se va apenas tocas para escribir:
                        // antes habia que borrarlo letra por letra con el
                        // teclado en pantalla antes de poder poner el tuyo.
                        selected.name.clear();
                        if let Ok(mut label) = name_text.get_single_mut() {
                            label.0.clear();
                        }
                    }
                    CcAction::KeyDone => {
                        keyboard.0 = false;
                        say = Some(quip_from(QUIP_NOMBRE, rand_seed()));
                    }
                    CcAction::KeyBack => {
                        selected.name.pop();
                        if let Ok(mut label) = name_text.get_single_mut() {
                            label.0 = selected.name.clone();
                        }
                    }
                    CcAction::Key(c) => {
                        if selected.name.len() < NAME_MAX {
                            selected.name.push(c);
                            if let Ok(mut label) = name_text.get_single_mut() {
                                label.0 = selected.name.clone();
                            }
                        }
                    }
                }
            }
            _ => {}
        }
    }
    if let (Some(line), Ok(mut bubble)) = (say, quip.get_single_mut()) {
        bubble.0 = line;
    }
    if dirty {
        selected.look = Some(look);
        // `refresh` repinta en el mismo frame; se marca la raíz en vez de repintar
        // acá para no pedir media docena de queries más en este sistema.
        if let Ok(e) = root.get_single() {
            commands.entity(e).try_insert(Dirty);
        }
    }
}

/// Repinta la vista previa y las etiquetas cuando el look cambió.
#[allow(clippy::too_many_arguments)]
fn refresh(
    mut commands: Commands,
    dirty: Query<Entity, (With<CcRoot>, With<Dirty>)>,
    cg: Res<Chargen>,
    selected: Res<SelectedCharacter>,
    asset_server: Res<AssetServer>,
    mut images: ResMut<Assets<Image>>,
    mut jobs: ResMut<ChargenJobs>,
    mut sets: ParamSet<(
        Query<(&mut ImageNode, &mut Visibility, &PreviewLayer)>,
        Query<&mut ImageNode, With<PortraitPreview>>,
        Query<(&mut Text, &StyleLabel)>,
        Query<(&mut Text, &ColorLabel)>,
        Query<(&mut Text, &crate::ui_pack::FichaValor)>,
    )>,
    grid_layer: Res<GridLayer>,
) {
    let Ok(root) = dirty.get_single() else { return };
    commands.entity(root).remove::<Dirty>();
    let Some(look) = selected.look else { return };

    for (mut node, mut vis, preview) in &mut sets.p0() {
        let layer = &preview.layer;
        match index_of(&look, *layer).and_then(|i| cg.options(*layer).get(i as usize)) {
            Some(opt) => {
                node.image = asset_server.load(&opt.png);
                *vis = Visibility::Inherited;
            }
            // Sin gorro: la capa se apaga, no se dibuja un hueco.
            None => *vis = Visibility::Hidden,
        }
    }
    if let (Ok(mut node), Some(h)) = (
        sets.p1().get_single_mut(),
        portrait_for(look, &cg, &mut jobs, &mut images, &asset_server),
    ) {
        node.image = h;
    }
    // La etiqueta de abajo del nombre y el contador siguen a la capa ACTIVA,
    // no a la que tenían cuando se armó la pantalla.
    let capa = grid_layer.0;
    let (estilo, _) = style_color(&cg, &look, capa);
    let sin_estilos = cg.styles_of(capa).is_empty();
    let pos = if sin_estilos {
        index_of(&look, capa).map(|i| i as usize + 1).unwrap_or(0)
    } else {
        estilo + 1
    };
    let total = if sin_estilos { cg.options(capa).len() } else { cg.styles_of(capa).len() };
    for (mut text, _) in &mut sets.p2() {
        text.0 = format!("{} {}", etiqueta_capa(capa), pos);
    }
    for (mut text, _) in &mut sets.p3() {
        text.0 = format!("{pos}/{total}");
    }
    for (mut text, marca) in &mut sets.p4() {
        if marca.0 == 1 {
            text.0 = capa.label().to_string();
        } else if marca.0 == 2 {
            text.0 = selected.name.clone();
        }
    }
}

fn start_game(selected: &SelectedCharacter) {
    // **El personaje también entra a la lista, no sólo al savegame.** Desde el
    // 15 ago 2026 los personajes viven aparte de los mundos (`personajes.rs`):
    // sin esto, el que acabás de crear jugaría bien pero no aparecería en la
    // pantalla PERSONAJES, que es donde se lo elige para la próxima partida.
    let p = crate::personajes::de_seleccionado("", selected);
    if !crate::personajes::guardar(&p) {
        eprintln!(
            "character-create: la lista de personajes está llena ({}), no se guardó «{}»",
            crate::personajes::MAXIMO,
            selected.name
        );
    }
    // Y si se llegó acá sin partida abierta (NUEVA PARTIDA la crea antes, pero
    // se puede entrar desde CREAR PERSONAJE), se le hace una para que el mundo
    // tenga dónde guardarse en vez de escribir suelto en la raíz.
    if crate::partida::activa().is_none() {
        crate::partida::crear("Partida", None, false);
    }
    save_savegame(selected);
    eprintln!(
        "character-create: JUGAR name='{}' look={:?}",
        selected.name, selected.look
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Las ruedas dan la vuelta y nunca se salen del catálogo: en el teléfono se
    /// toca `>` treinta veces seguidas sin mirar.
    #[test]
    fn las_ruedas_dan_la_vuelta_sin_salirse() {
        let cg = Chargen::default();
        let mut look = LookDef::default();
        for layer in ROWS {
            for _ in 0..(cg.options(layer).len() + 5) {
                cycle_style(&cg, &mut look, layer, 1);
                cycle_color(&cg, &mut look, layer, 1);
                if let Some(i) = index_of(&look, layer) {
                    assert!(
                        (i as usize) < cg.options(layer).len(),
                        "{layer:?} se salio del catalogo"
                    );
                }
            }
            for _ in 0..(cg.options(layer).len() + 5) {
                cycle_style(&cg, &mut look, layer, -1);
                cycle_color(&cg, &mut look, layer, -1);
            }
        }
    }

    /// El color no cambia el estilo: cambiar de color de pelo deja el mismo corte.
    #[test]
    fn el_color_no_cambia_el_corte() {
        let cg = Chargen::default();
        let mut look = LookDef::default();
        cycle_style(&cg, &mut look, Layer::Hair, 3);
        let (style, _) = style_color(&cg, &look, Layer::Hair);
        for _ in 0..4 {
            cycle_color(&cg, &mut look, Layer::Hair, 1);
            assert_eq!(style_color(&cg, &look, Layer::Hair).0, style);
        }
    }

    /// El gorro se puede sacar: la primera posicion de la rueda es NINGUNO.
    #[test]
    fn el_gorro_se_puede_sacar() {
        let cg = Chargen::default();
        let mut look = LookDef { accessory: Some(0), ..Default::default() };
        let styles = cg.styles_of(Layer::Accessory).len();
        let mut sin_gorro = false;
        for _ in 0..=styles {
            cycle_style(&cg, &mut look, Layer::Accessory, 1);
            if look.accessory.is_none() {
                sin_gorro = true;
                assert_eq!(style_text(&cg, &look, Layer::Accessory), "NINGUNO");
            }
        }
        assert!(sin_gorro, "no hay forma de quedarse sin accesorio");
    }
}

// ── Lo que el retrato opina ─────────────────────────────────────────────────
//
// Rompe la cuarta pared A PROPOSITO: la partida todavia no empezo, asi que este
// es el unico momento en que el personaje puede hablarle al que lo esta
// armando en vez de al mundo. Una vez que tocas JUGAR, deja de existir.
//
// ASCII-only (regla 6): la fuente del juego no dibuja acentos ni signos de
// apertura, salen cajas. Lo clava `los_comentarios_son_ascii`.

const QUIP_PIEL: &[&str] = &[
    "¿Otra piel? Dale, total no me duele.",
    "Con esta me veo bien hasta de noche.",
    "Ojo que después no te la cambio.",
    "Esa me gusta. Pero vos sabrás.",
];
const QUIP_OJOS: &[&str] = &[
    "Con estos ojos veo el mismo mundo, eh.",
    "¿Me estás mirando o me estás eligiendo?",
    "Estos dejalos. Con estos veo bien.",
];
const QUIP_ROPA: &[&str] = &[
    "Linda ropa. Igual me la voy a ensuciar.",
    "Esto no abriga nada. ¿Sabés que hay noche?",
    "Me vestís vos y después se ríen de mí.",
    "Con esto puesto no me toma nadie en serio.",
];
const QUIP_PELO: &[&str] = &[
    "El pelo asi? Bueno. Vos sabras.",
    "Peluquero de mundo abierto, tenemos.",
    "Ese corte se lo vi a un aldeano. Lo copiaste.",
];
const QUIP_GORRO: &[&str] = &[
    "El gorro es lo único que me representa.",
    "Sin gorro me siento desnudo.",
    "Con este gorro me van a ver de lejos. ¿Eso es bueno?",
];
const QUIP_AZAR: &[&str] = &[
    "¿Al azar? Gracias por el esfuerzo.",
    "Y así nace un héroe. De casualidad.",
    "Tirá de nuevo, dale. No soy yo el que sufre.",
];
const QUIP_NOMBRE: &[&str] = &[
    "Con ese nombre me voy a tener que hacer respetar.",
    "Ese nombre me lo van a gritar los aldeanos. Pensalo.",
    "Listo. Ya soy alguien.",
];
const QUIP_HOLA: &[&str] = &[
    "Bueno, acá estoy. Hacé lo que quieras.",
    "Antes de empezar: ¿gorro sí, o gorro no?",
    "Tocame algo. No me dejes así.",
];

/// La linea que le toca a un cambio. `None` = el saludo de bienvenida.
fn quip_for(layer: Option<Layer>, seed: u64) -> String {
    let pool = match layer {
        None => QUIP_HOLA,
        Some(Layer::Body) => QUIP_PIEL,
        Some(Layer::Eyes) => QUIP_OJOS,
        Some(Layer::Outfit) => QUIP_ROPA,
        Some(Layer::Hair) => QUIP_PELO,
        Some(Layer::Accessory) => QUIP_GORRO,
    };
    pool[(seed as usize) % pool.len()].to_string()
}

fn quip_from(pool: &[&str], seed: u64) -> String {
    pool[(seed as usize) % pool.len()].to_string()
}

// ── Teclado en pantalla ─────────────────────────────────────────────────────
//
// **Por que existe.** El nombre se escribia solo con `KeyboardInput`, o sea solo
// en escritorio: en el telefono no hay teclado fisico y el campo era decorativo
// — te quedabas con el nombre que vino sorteado. Se evaluo pedirle el teclado
// del sistema a Android (`show_soft_input`), pero eso tapa media pantalla y
// depende de que los eventos del IME lleguen como teclas, que en
// `android-activity` no es seguro. Dibujarlo nosotros es predecible y entra en
// la misma pantalla apaisada.

/// Las filas del teclado. ASCII y en mayusculas: es lo unico que la fuente
/// dibuja y lo unico que `name_input` acepta.
const KEY_ROWS: [&str; 4] = ["ABCDEFGHIJ", "KLMNOPQRST", "UVWXYZ0123", "456789 -_"];

#[derive(Resource, Default)]
struct KeyboardOpen(bool);

/// Abre/cierra el teclado sin recrear la pantalla entera.
/// **El teclado para el nombre, con el marco del pack.**
///
/// La maqueta no lo dibuja —el nombre ya aparece escrito— pero escribirlo hace
/// falta, así que se arma con las mismas piezas que el resto: panel, cartel y
/// botones del kit. Va sobre un velo, que es lo único que tapa la pantalla de
/// atrás mientras se escribe.
fn sync_keyboard(
    open: Res<KeyboardOpen>,
    mut commands: Commands,
    touch: Res<crate::input::TouchLayout>,
    art: Res<crate::art2d::ArtLibrary>,
    live: Query<Entity, With<CcKeyboard>>,
) {
    use crate::ui_pack::{self as up, Ancla, Pack};
    let want = open.0;
    let has = !live.is_empty();
    if want == has {
        return;
    }
    if !want {
        for e in &live {
            commands.entity(e).despawn_recursive();
        }
        return;
    }
    let k = up::k(&touch);
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
            // Encima de la pantalla de creacion (que va en 1000).
            GlobalZIndex(1100),
            BackgroundColor(Color::srgba(0.047, 0.039, 0.078, 0.78)),
            bevy::ui::FocusPolicy::Block,
            CcKeyboard,
            Name::new("CcKeyboard"),
        ))
        .with_children(|root| {
            root.spawn(up::lienzo(k)).with_children(|l| {
                let (pw, ph) = (1800.0_f32, 680.0_f32);
                let (pxu, pyu) = ((up::ANCHO - pw) / 2.0, 200.0_f32);
                up::marco(l, &art, up::marco::PANEL, pxu, pyu, pw, ph, k);
                up::cartel(l, &art, "TU NOMBRE", pxu + pw / 2.0, pyu - 48.0, 520.0, 96.0, 44.0, k);

                let ix = pxu + 66.0;
                up::marco(l, &art, up::marco::BOTON_ANCHO, ix, pyu + 50.0, pw - 132.0, 92.0, k);
                l.spawn((
                    up::caja(ix + 36.0, pyu + 96.0 - 32.0, pw - 200.0, 64.0, k),
                    Name::new("CcNombreCaja"),
                ))
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
                            Text::new(String::new()),
                            TextFont { font_size: 44.0 * k, ..default() },
                            TextColor(Pack::TINTA),
                            TextLayout::new_with_no_wrap(),
                            NameText,
                        ));
                    });
                });

                let ancho_t = 118.0_f32;
                let alto_t = 84.0_f32;
                let sep = 8.0_f32;
                for (f, fila) in KEY_ROWS.iter().enumerate() {
                    let n = fila.chars().count() as f32;
                    let total = n * ancho_t + (n - 1.0) * sep;
                    let x0 = pxu + (pw - total) / 2.0;
                    let y = pyu + 180.0 + f as f32 * (alto_t + sep);
                    for (i, c) in fila.chars().enumerate() {
                        up::boton(
                            l, &art, x0 + i as f32 * (ancho_t + sep), y, ancho_t, alto_t,
                            Some(&c.to_string()), None, false, 34.0, CcAction::Key(c), k,
                        );
                    }
                }
                let y = pyu + 180.0 + KEY_ROWS.len() as f32 * (alto_t + sep);
                up::boton(l, &art, pxu + 66.0, y, 320.0, alto_t, Some("BORRAR"), None, false,
                          32.0, CcAction::KeyBack, k);
                up::boton(l, &art, pxu + pw - 66.0 - 320.0, y, 320.0, alto_t, Some("LISTO"),
                          None, true, 32.0, CcAction::KeyDone, k);
                let _ = Ancla::Izq;
            });
        });
}

fn key_button(parent: &mut ChildBuilder, c: char, key: f32, s: f32) {
    let label = if c == ' ' { "ESP".to_string() } else { c.to_string() };
    parent
        .spawn((
            Button,
            Node {
                width: Val::Px(key),
                height: Val::Px(key),
                justify_content: JustifyContent::Center,
                align_items: AlignItems::Center,
                border: UiRect::all(Val::Px(1.0)),
                ..default()
            },
            BorderRadius::all(Val::Px(5.0)),
            BorderColor(Color::srgba(0.16, 0.94, 1.0, 0.5)),
            BackgroundColor(idle_bg(CcAction::Key(c))),
            CcAction::Key(c),
        ))
        .with_children(|b| {
            b.spawn((
                Text::new(label),
                TextFont { font_size: (14.0 * s).clamp(9.0, 16.0), ..default() },
                TextColor(Color::WHITE),
            ));
        });
}

fn wide_key(parent: &mut ChildBuilder, label: &str, action: CcAction, key: f32, s: f32) {
    parent
        .spawn((
            Button,
            Node {
                width: Val::Px(key * 3.0),
                height: Val::Px(key),
                justify_content: JustifyContent::Center,
                align_items: AlignItems::Center,
                border: UiRect::all(Val::Px(2.0)),
                ..default()
            },
            BorderRadius::all(Val::Px(5.0)),
            BorderColor(NEON_CYAN),
            BackgroundColor(idle_bg(action)),
            action,
        ))
        .with_children(|b| {
            b.spawn((
                Text::new(label.to_string()),
                TextFont { font_size: (13.0 * s).clamp(9.0, 15.0), ..default() },
                TextColor(NEON_CYAN),
            ));
        });
}

#[cfg(test)]
mod quip_tests {
    use super::*;

    /// La fuente embarcada tiene que saber dibujar cada comentario del
    /// creador de personaje — acentos incluidos, que ahora si entran.
    #[test]
    fn la_fuente_dibuja_los_comentarios() {
        for pool in [
            QUIP_PIEL, QUIP_OJOS, QUIP_ROPA, QUIP_PELO, QUIP_GORRO, QUIP_AZAR, QUIP_NOMBRE,
            QUIP_HOLA,
        ] {
            for line in pool {
                let rotos: String =
                    line.chars().filter(|c| !crate::fuente::dibujable(*c)).collect();
                assert!(rotos.is_empty(), "la fuente no dibuja [{rotos}] en: {line}");
            }
        }
    }

    /// El teclado tiene que poder escribir todo lo que `name_input` acepta
    /// (alfanumerico + `_` + `-` + espacio); si no, hay nombres que el teclado
    /// fisico permite y el del telefono no.
    #[test]
    fn el_teclado_cubre_lo_que_el_nombre_acepta() {
        let keys: String = KEY_ROWS.concat();
        for c in "ABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789 -_".chars() {
            assert!(keys.contains(c), "al teclado en pantalla le falta '{c}'");
        }
    }
}

// ── La cuadricula de opciones ───────────────────────────────────────────────
//
// **Por que reemplazo a las ruedas.** Con `< ROPA 20 >` habia que pasar las
// prendas de a una y leer un numero para saber cual era: elegir ropa mirando
// texto. Ahora cada opcion es su propio dibujo y se elige tocandola, que es lo
// que uno espera de un creador de personaje.
//
// La celda muestra el frame DE FRENTE de la capa sola (sin cuerpo debajo): para
// ropa, pelo y gorro eso es exactamente la prenda. Es el mismo `ImageNode` con
// atlas que ya usa la vitrina, asi que no compone ni hornea nada — dibujar 200
// celdas cuesta lo mismo que dibujar 200 sprites.

/// Celda de la hoja que se muestra en la cuadricula: quieto, de frente.
fn grid_cell_index() -> usize {
    showcase_cell(3, 0)
}

/// Una ficha de capa (la fila de arriba de la cuadricula).
fn layer_tab(parent: &mut ChildBuilder, layer: Layer, active: bool, s: f32) {
    parent
        .spawn((
            Button,
            Node {
                height: Val::Px((26.0 * s).clamp(20.0, 30.0)),
                padding: UiRect::horizontal(Val::Px(8.0 * s)),
                justify_content: JustifyContent::Center,
                align_items: AlignItems::Center,
                border: UiRect::all(Val::Px(if active { 2.0 } else { 1.0 })),
                ..default()
            },
            BorderRadius::all(Val::Px(5.0)),
            BorderColor(if active { NEON_CYAN } else { CARD_EDGE }),
            BackgroundColor(if active {
                Color::srgb(0.10, 0.22, 0.30)
            } else {
                Color::srgb(0.08, 0.10, 0.16)
            }),
            CcAction::PickLayer(layer),
            CcLayerTab(layer),
        ))
        .with_children(|b| {
            b.spawn((
                Text::new(layer.label().to_string()),
                TextFont { font_size: (11.0 * s).clamp(8.0, 13.0), ..default() },
                TextColor(if active { NEON_CYAN } else { DIM }),
            ));
        });
}

/// Rellena la cuadricula con las opciones de `layer`, marcando la elegida.
/// Se llama al entrar y cada vez que cambia la capa o la eleccion.
#[allow(clippy::too_many_arguments)]
/// **La grilla de ESTILOS del mockup.**
///
/// Lo que se muestra no es el catálogo plano sino los ESTILOS de la capa, que es
/// como el creador ya modela el look (`Chargen::styles_of`): una casilla por
/// corte de pelo, y los colores de ese corte en la fila de abajo. En PIEL y
/// OJOS, que no tienen color aparte, la grilla ES el catálogo — por eso el
/// mockup muestra 9 y 7 casillas ahí.
#[allow(clippy::too_many_arguments)]
fn fill_grid(
    commands: &mut Commands,
    art: &crate::art2d::ArtLibrary,
    cg: &Chargen,
    look: &LookDef,
    layer: Layer,
    hoja: &Handle<TextureAtlasLayout>,
    assets: &AssetServer,
    grid: Entity,
    k: f32,
) {
    let grupos = cg.styles_of(layer);
    let (estilo_actual, color_actual) = style_color(cg, look, layer);
    commands.entity(grid).despawn_descendants();
    commands.entity(grid).with_children(|grid| {
        // El accesorio suma NINGUNO al principio: "sin gorro" es una elección.
        if layer == Layer::Accessory {
            celda_estilo(grid, art, cg, look, layer, None, index_of(look, layer).is_none(),
                         hoja, assets, k);
        }
        if grupos.is_empty() {
            for (i, _) in cg.options(layer).iter().enumerate() {
                let idx = i as u32;
                celda_estilo(grid, art, cg, look, layer, Some(idx),
                             index_of(look, layer) == Some(idx), hoja, assets, k);
            }
        } else {
            let extra = if layer == Layer::Accessory { 1 } else { 0 };
            for (g, grupo) in grupos.iter().enumerate() {
                // La casilla muestra el estilo EN EL COLOR ELEGIDO: cambiar de
                // color repinta la grilla entera, que es lo que hace el mockup.
                let idx = grupo[color_actual.min(grupo.len() - 1)] as u32;
                let elegida = g + extra == estilo_actual;
                celda_estilo(grid, art, cg, look, layer, Some(idx), elegida, hoja, assets, k);
            }
        }
    });
}

/// **Una casilla de la grilla.** Es la casilla del pack con el personaje entero
/// adentro: las cuatro capas que NO cambian se dibujan con la hoja que ya está
/// en memoria (la del look actual) y la que sí cambia se pide en miniatura
/// recortada, que es lo que evita cargar doscientas hojas de 9 MB.
#[allow(clippy::too_many_arguments)]
fn celda_estilo(
    parent: &mut ChildBuilder,
    art: &crate::art2d::ArtLibrary,
    cg: &Chargen,
    look: &LookDef,
    layer: Layer,
    idx: Option<u32>,
    elegida: bool,
    hoja: &Handle<TextureAtlasLayout>,
    assets: &AssetServer,
    k: f32,
) {
    use crate::ui_pack as up;
    // ¿Se ve el cuerpo entero o sólo la cabeza? El mockup muestra el muñeco
    // completo en ROPA y ACCESORIOS, y un retrato en PIEL, OJOS y PELO.
    let cuerpo_entero = matches!(layer, Layer::Outfit | Layer::Accessory);
    // **El retrato va al DOBLE de escala que el cuerpo entero.** La cabeza son
    // 26 de los 64 píxeles del muñeco: dibujada a la misma escala que el cuerpo
    // ocupa un tercio de la casilla y la casilla se ve vacía. En la imagen de
    // referencia el retrato llena la casilla.
    let escala = if cuerpo_entero { 2.0_f32 } else { 4.0 };
    let cel = Vec2::new(cg.index.cell[0] as f32, cg.index.cell[1] as f32) * escala;
    // El encuadre: el cuerpo entero se apoya arriba; el retrato se centra en la
    // cabeza (ver [`CENTRO_CABEZA`]).
    let (off_x, off_y) = if cuerpo_entero {
        (LADO_GRID / 2.0 - cel.x / 2.0, 2.0)
    } else {
        (LADO_GRID / 2.0 - cel.x / 2.0, LADO_GRID / 2.0 - CENTRO_CABEZA * escala)
    };

    let mut nodo = Node {
        width: Val::Px(LADO_GRID * k),
        height: Val::Px(LADO_GRID * k),
        overflow: Overflow::clip(),
        ..default()
    };
    nodo.position_type = PositionType::Relative;
    // **La miniatura la pide `stream_thumbs`**, y para encontrarla necesita el
    // `CcThumb` en la CASILLA y el `CcThumbSlot` en un hijo. Ponerlos en el
    // mismo nodo (primer intento) dejaba la grilla entera con la misma prenda.
    let png_variable = idx
        .and_then(|i| cg.options(layer).get(i as usize))
        .map(|o| o.png.clone());
    let mut celda = match art.ui_frame_node(up::marco::SLOT) {
        Some(img) => parent.spawn((nodo, img, Button, CcAction::PickOption(layer, idx), CcCell(layer, idx))),
        None => parent.spawn((
            nodo,
            BackgroundColor(Color::srgb(0.796, 0.706, 0.580)),
            Button,
            CcAction::PickOption(layer, idx),
            CcCell(layer, idx),
        )),
    };
    if let Some(png) = png_variable {
        celda.insert(CcThumb(png));
    }
    celda.with_children(|c| {
        let Some(elegido) = idx else {
            c.spawn((
                Node {
                    position_type: PositionType::Absolute,
                    left: Val::Px(0.0),
                    top: Val::Px(0.0),
                    width: Val::Percent(100.0),
                    height: Val::Percent(100.0),
                    justify_content: JustifyContent::Center,
                    align_items: AlignItems::Center,
                    ..default()
                },
                Name::new("SinAccesorio"),
            ))
            .with_children(|t| {
                t.spawn((
                    Text::new("NINGUNO"),
                    TextFont { font_size: 22.0 * k, ..default() },
                    TextColor(up::Pack::SUAVE),
                ));
            });
            return;
        };
        // Las cinco capas del retrato. La que se está eligiendo va con SU
        // opción; las otras cuatro, con las del look actual.
        for capa in ROWS {
            let propia = capa == layer;
            let opt = if propia {
                cg.options(capa).get(elegido as usize)
            } else {
                index_of(look, capa).and_then(|i| cg.options(capa).get(i as usize))
            };
            let Some(o) = opt else { continue };
            let hueco = Node {
                position_type: PositionType::Absolute,
                left: Val::Px(off_x * k),
                top: Val::Px(off_y * k),
                width: Val::Px(cel.x * k),
                height: Val::Px(cel.y * k),
                ..default()
            };
            if propia {
                // El hueco de la miniatura: lo llena `stream_thumbs` cuando la
                // casilla entra en pantalla (ver `CcThumb`).
                c.spawn((hueco, CcThumbSlot));
            } else {
                c.spawn((
                    hueco,
                    ImageNode::from_atlas_image(
                        assets.load(&o.png),
                        TextureAtlas { layout: hoja.clone(), index: grid_cell_index() },
                    ),
                ));
            }
        }
    });
    if elegida {
        // El recuadro del mockup: 6 px, corrido 3 hacia arriba y a la izquierda.
        celda.with_children(|c| {
            c.spawn((
                Node {
                    position_type: PositionType::Absolute,
                    left: Val::Px(-3.0 * k),
                    top: Val::Px(-3.0 * k),
                    width: Val::Px(LADO_GRID * k),
                    height: Val::Px(LADO_GRID * k),
                    border: UiRect::all(Val::Px(6.0 * k)),
                    ..default()
                },
                BorderColor(up::Pack::ACENTO),
                MarcaElegida,
            ));
        });
    }
}

/// **La fila de COLORES.** Es la otra mitad del creador: la grilla muestra los
/// cortes y esta fila, los colores del corte elegido. Se rehace cuando cambia la
/// capa o el estilo, no cada frame.
#[allow(clippy::too_many_arguments)]
fn sincronizar_colores(
    mut commands: Commands,
    art: Res<crate::art2d::ArtLibrary>,
    cg: Res<Chargen>,
    selected: Res<SelectedCharacter>,
    grid_layer: Res<GridLayer>,
    assets: Res<AssetServer>,
    sheet: Res<CcSheetLayout>,
    fila: Query<(Entity, &FilaColores)>,
    mut ultimo: Local<Option<(Layer, usize)>>,
) {
    let Ok((e, f)) = fila.get_single() else {
        *ultimo = None;
        return;
    };
    let capa = grid_layer.0;
    let look = selected.look.unwrap_or_default();
    let (estilo, color_actual) = style_color(&cg, &look, capa);
    if *ultimo == Some((capa, estilo)) {
        return;
    }
    *ultimo = Some((capa, estilo));

    let extra = if capa == Layer::Accessory { 1 } else { 0 };
    let grupo: Vec<usize> = cg
        .styles_of(capa)
        .get(estilo.saturating_sub(extra))
        .cloned()
        .unwrap_or_default();
    let k = f.k;
    commands.entity(e).despawn_descendants();
    if grupo.len() < 2 {
        // Piel y ojos no tienen color aparte: la fila queda vacía, como en el
        // mockup (las pestañas PIEL y OJOS no la dibujan).
        return;
    }
    commands.entity(e).with_children(|c| {
        for (pos, &idx) in grupo.iter().enumerate() {
            let x = 130.0 + pos as f32 * 74.0;
            let mut cuadro = crate::ui_pack::caja(x, 0.0, 62.0, 62.0, k);
            cuadro.overflow = Overflow::clip();
            let mut ec = match art.ui_frame_node(crate::ui_pack::marco::SLOT_CHICO) {
                Some(img) => c.spawn((cuadro, img, Button, CcAction::PickColor(capa, pos))),
                None => c.spawn((
                    cuadro,
                    BackgroundColor(Color::srgb(0.796, 0.706, 0.580)),
                    Button,
                    CcAction::PickColor(capa, pos),
                )),
            };
            // La muestra: el propio dibujo de esa opción, recortado a un
            // cuadradito. Es más honesto que inventar un color plano — el color
            // de una prenda del pack no es un número, es su pixel.
            ec.with_children(|m| {
                if let Some(o) = cg.options(capa).get(idx) {
                    // La muestra encuadra el TORSO: la cabeza no dice nada del
                    // color de una remera, y para el pelo el encuadre igual cae
                    // sobre la melena.
                    // Escala 62/32: la celda de 32 llena la muestra de ancho y
                    // se corre para que quede centrada la parte de la capa.
                    let esc = 62.0 / 32.0;
                    let top = 31.0 - centro_de_muestra(capa) * esc;
                    m.spawn((
                        Node {
                            position_type: PositionType::Absolute,
                            left: Val::Px(0.0),
                            top: Val::Px(top * k),
                            width: Val::Px(62.0 * k),
                            height: Val::Px(124.0 * k),
                            ..default()
                        },
                        ImageNode::from_atlas_image(
                            assets.load(&o.png),
                            TextureAtlas { layout: sheet.0.clone(), index: grid_cell_index() },
                        ),
                    ));
                }
            });
            if pos == color_actual {
                ec.with_children(|m| {
                    m.spawn((
                        Node {
                            position_type: PositionType::Absolute,
                            left: Val::Px(-3.0 * k),
                            top: Val::Px(-3.0 * k),
                            width: Val::Px(62.0 * k),
                            height: Val::Px(62.0 * k),
                            border: UiRect::all(Val::Px(5.0 * k)),
                            ..default()
                        },
                        BorderColor(crate::ui_pack::Pack::ACENTO),
                    ));
                });
            }
        }
    });
}

/// El pulgar de la canaleta sigue al scroll de la grilla. Sin esto la canaleta
/// sería un adorno: el mockup la dibuja con el pulgar proporcional.
fn mover_pulgar(
    vistas: Query<(&ScrollPosition, &ComputedNode, &Children), With<CcScroll>>,
    contenidos: Query<&ComputedNode, With<crate::hud::ScrollContent>>,
    mut pulgares: Query<(&mut Node, &PulgarScroll)>,
) {
    let Ok((sp, vista, hijos)) = vistas.get_single() else { return };
    let alto_vista = vista.size().y * vista.inverse_scale_factor();
    let alto_contenido = hijos
        .iter()
        .filter_map(|c| contenidos.get(*c).ok())
        .map(|c| c.size().y * c.inverse_scale_factor())
        .fold(0.0f32, f32::max);
    if alto_contenido <= 0.0 {
        return;
    }
    let frac = (alto_vista / alto_contenido).clamp(0.08, 1.0);
    let tope = crate::hud::max_scroll(alto_vista, alto_contenido);
    let avance = if tope > 0.0 { (sp.offset_y / tope).clamp(0.0, 1.0) } else { 0.0 };
    for (mut nodo, p) in &mut pulgares {
        let alto = (p.alto * frac).max(90.0);
        nodo.height = Val::Px(alto * p.k);
        nodo.top = Val::Px((p.y0 + (p.alto - alto) * avance) * p.k);
        nodo.display = if frac >= 1.0 { Display::None } else { Display::Flex };
    }
}

/// El recuadro amarillo de la casilla elegida.
#[derive(Component)]
struct MarcaElegida;

/// Rehace la cuadricula cuando cambio la capa o la eleccion. Va aparte de
/// `refresh` porque esa repinta lo que YA existe y esto crea/destruye celdas.
#[allow(clippy::too_many_arguments)]
fn sync_grid(
    mut commands: Commands,
    grid_layer: Res<GridLayer>,
    selected: Res<SelectedCharacter>,
    cg: Res<Chargen>,
    touch: Res<crate::input::TouchLayout>,
    grid: Query<Entity, With<CcGrid>>,
    mut tabs: Query<(&CcLayerTab, &mut BorderColor)>,
    mut last: Local<Option<(Layer, Option<u32>)>>,
    art: Res<crate::art2d::ArtLibrary>,
    sheet: Res<CcSheetLayout>,
    assets: Res<AssetServer>,
) {
    let Ok(grid_e) = grid.get_single() else {
        *last = None;
        return;
    };
    let layer = grid_layer.0;
    let look = selected.look.unwrap_or_default();
    // SOLO la capa. Antes la clave incluia la prenda elegida, asi que cada
    // eleccion (y cada AL AZAR, que cambia las cinco) destruia y recreaba las
    // 132 celdas: de ahi el titileo, y el pico de trabajo que dejaba al juego al
    // borde de que Android lo matara. Lo que cambia al elegir es un BORDE, y eso
    // lo pinta `marcar_seleccion` sin tocar la jerarquia.
    let key = (layer, index_of(&look, layer));
    if *last == Some(key) {
        return;
    }
    *last = Some(key);
    let k = crate::ui_pack::k(&touch);
    fill_grid(&mut commands, &art, &cg, &look, layer, &sheet.0, &assets, grid_e, k);
    let _ = &mut tabs;
}

/// El layout de atlas de las hojas del pack. Lo arma `spawn_screen` y lo reusa
/// la cuadricula: recrearlo por celda seria un `TextureAtlasLayout` nuevo por
/// cada una de las ~200 opciones.
#[derive(Resource, Default)]
struct CcSheetLayout(Handle<TextureAtlasLayout>);

/// Arrastre tactil + rueda en la cuadricula. Es una LISTA de ~200 prendas, asi
/// que aca el scroll SI corresponde — lo que no lleva scroll es el menu.
fn scroll_grid(
    touches: Res<Touches>,
    mut wheel: EventReader<bevy::input::mouse::MouseWheel>,
    mut views: Query<(&mut ScrollPosition, &ComputedNode, &Children), With<CcScroll>>,
    contents: Query<&ComputedNode, With<crate::hud::ScrollContent>>,
) {
    let mut dy = 0.0f32;
    for ev in wheel.read() {
        dy -= ev.y * 26.0;
    }
    for t in touches.iter() {
        dy -= t.delta().y;
    }
    if dy.abs() < 0.5 {
        return;
    }
    for (mut sp, view, children) in &mut views {
        let view_h = view.size().y * view.inverse_scale_factor();
        let content_h = children
            .iter()
            .filter_map(|c| contents.get(*c).ok())
            .map(|c| c.size().y * c.inverse_scale_factor())
            .fold(0.0f32, f32::max);
        let tope = crate::hud::max_scroll(view_h, content_h);
        sp.offset_y = (sp.offset_y + dy).clamp(0.0, tope);
        // Mismo diagnostico que la mochila: si `tope` sale 0 con 132 prendas, el
        // problema es la MEDIDA del contenido, no el gesto.
        eprintln!(
            "ha-scroll-ropa: dy={dy:.0} vista={view_h:.0} contenido={content_h:.0} tope={tope:.0}"
        );
    }
}

/// El marco que scrollea de la cuadricula.
#[derive(Component)]
struct CcScroll;

/// Repinta cual celda esta elegida, sin rehacer la cuadricula.
///
/// Es la contraparte de que `sync_grid` ya no se dispare con cada eleccion:
/// cambiar de prenda mueve un borde, no reconstruye 132 entidades.
fn marcar_seleccion(
    selected: Res<SelectedCharacter>,
    grid_layer: Res<GridLayer>,
    mut celdas: Query<(&CcCell, &mut BorderColor, &mut BackgroundColor)>,
) {
    let look = selected.look.unwrap_or_default();
    let actual = index_of(&look, grid_layer.0);
    for (celda, mut borde, mut fondo) in &mut celdas {
        let elegida = celda.0 == grid_layer.0 && celda.1 == actual;
        *borde = BorderColor(if elegida { NEON_MAGENTA } else { CARD_EDGE });
        *fondo = BackgroundColor(if elegida {
            Color::srgb(0.24, 0.07, 0.15)
        } else {
            Color::srgb(0.15, 0.17, 0.23)
        });
    }
}

/// Aplica la prenda candidata cuando SOLTAS, y solo si el dedo casi no se movio.
///
/// Es la mitad que hace usable la cuadricula en el telefono: con 200 prendas hay
/// que arrastrar mucho, y antes cada arrastre cambiaba la ropa con la primera
/// celda que tocabas.
fn resolver_toque(
    mut commands: Commands,
    touches: Res<Touches>,
    cg: Res<Chargen>,
    mut toque: ResMut<Toque>,
    mut selected: ResMut<SelectedCharacter>,
    root: Query<Entity, With<CcRoot>>,
    mut quip: Query<&mut Text, With<PortraitQuip>>,
) {
    if toque.candidata.is_none() {
        return;
    }
    // Mientras haya dedo apoyado, sumamos lo que se movio.
    let mut hay_dedo = false;
    for t in touches.iter() {
        hay_dedo = true;
        toque.recorrido += t.delta().length();
    }
    if hay_dedo {
        return;
    }
    // Se solto: decide.
    let Some((layer, idx)) = toque.candidata.take() else { return };
    if toque.recorrido > UMBRAL_TOQUE {
        return; // era un scroll
    }
    let mut look = selected.look.unwrap_or_default();
    set_index(&mut look, layer, idx);
    selected.look = Some(look);
    // Marcar la raiz SUCIA es lo que dispara `refresh`: sin esto se elegia la
    // prenda pero el muneco, el retrato y las etiquetas se quedaban con la
    // anterior. Lo rompi al mover la seleccion de "al apoyar" a "al soltar".
    if let Ok(e) = root.get_single() {
        commands.entity(e).try_insert(Dirty);
    }
    let _ = &cg;
    if let Ok(mut t) = quip.get_single_mut() {
        t.0 = quip_for(Some(layer), rand_seed());
    }
}

/// Lado de una celda de la cuadricula, en px de UI.
///
/// El panel ocupa ~21% del ancho de pantalla (medido en el Redmi), y queremos
/// TRES por fila: la celda es ese tercio menos el hueco. Antes era `44*s`
/// topeado en 52 px, que con `s=0.52` daba 32 px y las prendas se veian como
/// estampillas.
fn celda_px(ancho_pantalla: f32) -> f32 {
    (ancho_pantalla * 0.21 / COLUMNAS as f32 - 5.0).clamp(34.0, 96.0)
}

/// Prendas por fila en la cuadricula del creador de personaje.
const COLUMNAS: usize = 4;

/// Cuanto se movio el dedo desde que toco la pantalla, y que prenda estaba
/// debajo cuando empezo.
///
/// Sin esto, elegir ropa y scrollear eran la misma cosa: `Interaction::Pressed`
/// dispara al APOYAR el dedo, asi que arrastrar para ver mas prendas te cambiaba
/// la ropa con la primera que tocabas. Ahora la eleccion espera a que SUELTES, y
/// solo se aplica si el dedo casi no se movio.
#[derive(Resource, Default)]
struct Toque {
    recorrido: f32,
    candidata: Option<(Layer, Option<u32>)>,
}

/// Cuanto puede moverse el dedo y seguir contando como TOQUE y no arrastre, en
/// px de UI. Por debajo de esto la seleccion se aplica al soltar; por encima se
/// entiende que estabas scrolleando y no se elige nada.
const UMBRAL_TOQUE: f32 = 12.0;

/// Ancho del contenedor de la cuadricula: EXACTAMENTE tres celdas mas sus dos
/// huecos. Sin esto el contenedor se dimensionaba al contenido (`flex_shrink:
/// 0`) y el flex-wrap envolvia a los 2400 px de la pantalla en vez de a los del
/// panel: entraban quince prendas en una sola fila que se salia por los costados.
fn grilla_px(ancho_pantalla: f32, s: f32) -> f32 {
    // Tres celdas + los DOS huecos + el padding de los dos lados + un par de px
    // de borde. La primera version sumaba solo los huecos y quedaba ~4 px corta:
    // entraban dos prendas por fila en vez de tres, que es exactamente el tipo
    // de error que no se ve leyendo el codigo y si en la captura.
    let n = COLUMNAS as f32;
    celda_px(ancho_pantalla) * n + (6.0 * s) * (n - 1.0) + (4.0 * s) * 2.0 + 4.0
}

/// Cuántas alturas de celda se precargan por arriba y por abajo de la vista.
/// Con 0 la prenda aparecería recién al entrar; con 1 ya está lista cuando la
/// fila asoma, y aun asi el pico de memoria queda en ~5 filas.
const MARGEN_FILAS: f32 = 1.0;

/// **Techo duro de hojas cargadas a la vez.** Cada una son 9,2 MB, asi que 18
/// son ~165 MB: entra holgado en el Redmi, que es lo que manda (regla 2). Con 3
/// columnas alcanza para las 6 filas alrededor de la vista, o sea que scrolleando
/// normal la prenda ya esta cuando la fila asoma.
const TECHO_HOJAS: usize = 18;

/// **Carga las hojas que se ven y suelta las que no.**
///
/// Es lo que hace que el creador de personaje no muera: cada hoja son 9,2 MB y
/// hay capas de 200 opciones (ver `CcThumb`). Se mira la posicion REAL de cada
/// celda —via `GlobalTransform`, o sea sin re-derivar el layout a mano— contra
/// el rectangulo del marco que scrollea.
fn stream_thumbs(
    mut commands: Commands,
    server: Res<AssetServer>,
    layouts: Res<CcSheetLayout>,
    marco: Query<(&ComputedNode, &GlobalTransform), With<CcScroll>>,
    celdas: Query<(&CcThumb, &ComputedNode, &GlobalTransform, &Children)>,
    huecos: Query<(Entity, Option<&ImageNode>), With<CcThumbSlot>>,
    mut mini: ResMut<Miniaturas>,
    mut ultimo: Local<usize>,
) {
    let Ok((marco_node, marco_tf)) = marco.get_single() else { return };
    let centro_marco = marco_tf.translation().y;

    // PRESUPUESTO DURO, no "las que se vean".
    //
    // La primera version confiaba en el calculo de visibilidad, y cuando ese
    // calculo se equivocaba —celdas todavia sin layout, todas en y=0— cargaba
    // las 132 hojas igual que antes y Android mataba el juego. Un limite por
    // construccion no depende de que la geometria este bien: pase lo que pase,
    // nunca hay mas de `TECHO` hojas vivas. La visibilidad ahora sólo decide
    // CUALES son, ordenando por cercania al centro del marco.
    let mut candidatas: Vec<(f32, Entity, &str)> = Vec::new();
    for (thumb, celda_node, celda_tf, hijos) in &celdas {
        let Some(hueco) = hijos.iter().find(|h| huecos.contains(**h)) else { continue };
        // Celda sin medir todavia: al fondo de la cola, no adelante.
        let dist = if celda_node.size().y <= 0.0 {
            f32::MAX
        } else {
            (celda_tf.translation().y - centro_marco).abs()
        };
        candidatas.push((dist, *hueco, thumb.0.as_str()));
    }
    candidatas.sort_by(|a, b| a.0.total_cmp(&b.0));

    let mut cargadas = 0usize;
    for (i, (dist, hueco, ruta)) in candidatas.iter().enumerate() {
        let Ok((_, tiene)) = huecos.get(*hueco) else { continue };
        let quiero = i < TECHO_HOJAS && *dist < f32::MAX;
        match (quiero, tiene.is_some()) {
            (true, false) => match mini.hechas.get(*ruta) {
                // Ya recortada: 8 KB, sin atlas.
                Some(h) => {
                    commands.entity(*hueco).try_insert(ImageNode::new(h.clone()));
                }
                // Todavia no: se pide la hoja una vez y `recortar_miniaturas`
                // se encarga de quedarse solo con la celda.
                None => {
                    if !mini.pendientes.contains_key(*ruta) {
                        let h = server.load(*ruta);
                        mini.pendientes.insert(ruta.to_string(), h);
                    }
                }
            },
            // Sacar el `ImageNode` suelta la ultima referencia al handle, y ahi
            // el `AssetServer` descarga la textura. Es la mitad que importa: sin
            // esto el pico seria el mismo, solo que mas tarde.
            (false, true) => {
                commands.entity(*hueco).remove::<ImageNode>();
            }
            _ => {}
        }
        if quiero {
            cargadas += 1;
        }
    }

    // Se imprime cuando CAMBIA, no por frame: es la unica forma de ver desde
    // logcat que el techo se esta respetando en el telefono.
    if *ultimo != cargadas {
        *ultimo = cargadas;
        eprintln!("ha-chargen: {cargadas} hojas cargadas (techo {TECHO_HOJAS}) de {} celdas", candidatas.len());
    }
}
