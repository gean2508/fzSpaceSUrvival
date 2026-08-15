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
                (name_input, button_system, resolver_toque, refresh, sync_grid, marcar_seleccion, animate_showcase, sync_keyboard, scroll_grid)
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
                (recortar_miniaturas, stream_thumbs)
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
) {
    let Ok(prefix) = crate::flags::var("HA_CCSHOT") else { return };
    if shot.done {
        return;
    }
    shot.frames += 1;
    // Los tiempos son GENEROSOS a propósito: las cinco capas del muñeco son PNG
    // que se decodifican en hilos de IO, y disparando a los 30 frames de entrar
    // la captura salía con la vitrina vacía — parecía un bug de layout y era la
    // foto sacada antes de que llegara el arte.
    match shot.frames {
        10 => next.set(GamePhase::CharacterCreate),
        100 => {
            commands
                .spawn(bevy::render::view::screenshot::Screenshot::primary_window())
                .observe(bevy::render::view::screenshot::save_to_disk(format!(
                    "{prefix}_creacion.png"
                )));
        }
        // El teclado en pantalla: es la única forma de escribir el nombre en el
        // teléfono, así que entra en la corrida de QA como una pantalla más.
        120 => keyboard.0 = true,
        140 => {
            commands
                .spawn(bevy::render::view::screenshot::Screenshot::primary_window())
                .observe(bevy::render::view::screenshot::save_to_disk(format!(
                    "{prefix}_teclado.png"
                )));
        }
        160 => keyboard.0 = false,
        180 => next.set(GamePhase::Loading),
        205 => {
            commands
                .spawn(bevy::render::view::screenshot::Screenshot::primary_window())
                .observe(bevy::render::view::screenshot::save_to_disk(format!(
                    "{prefix}_carga.png"
                )));
        }
        f if f >= 240 => {
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
fn spawn_screen(
    mut commands: Commands,
    asset_server: Res<AssetServer>,
    mut layouts: ResMut<Assets<TextureAtlasLayout>>,
    mut images: ResMut<Assets<Image>>,
    mut jobs: ResMut<ChargenJobs>,
    cg: Res<Chargen>,
    mut selected: ResMut<SelectedCharacter>,
    touch: Res<crate::input::TouchLayout>,
    grid_layer: Res<GridLayer>,
    mut sheet_res: ResMut<CcSheetLayout>,
) {
    // Sin look previo (partida nueva) se arranca con uno sorteado: la pantalla
    // nunca abre con un maniquí vacío.
    let look = *selected.look.get_or_insert_with(|| cg.index.look_from_seed(rand_seed()));

    let cell = UVec2::new(cg.index.cell[0], cg.index.cell[1]);
    let sheet_layout = layouts.add(TextureAtlasLayout::from_grid(
        cell,
        cg.index.cols.max(1),
        cg.index.rows.max(1),
        None,
        None,
    ));
    let portrait_layout = layouts.add(TextureAtlasLayout::from_grid(
        UVec2::new(cg.index.portrait_cell[0], cg.index.portrait_cell[1]),
        cg.index.portrait_cols.max(1),
        1,
        None,
        None,
    ));
    let portrait = portrait_for(look, &cg, &mut jobs, &mut images, &asset_server);
    // La cuadricula reusa este layout (una sola vez, no uno por celda).
    sheet_res.0 = sheet_layout.clone();

    // ── Escalado responsive ────────────────────────────────────────────────
    // **TODO ENTRA EN UNA PANTALLA. NO HAY SCROLL.** Antes esto era una columna
    // con `Overflow::scroll_y()`: en apaisado de teléfono (~400 px de alto
    // lógico) la vitrina se comía la mitad y las últimas ruedas, el nombre y
    // hasta JUGAR quedaban abajo del pliegue. Ponerle scroll tapaba el síntoma;
    // el pedido fue que entre todo.
    //
    // Lo que sobra en apaisado es ANCHO, así que la pantalla es una FILA:
    // vitrina + retrato a la izquierda, ruedas + nombre a la derecha. Las
    // medidas salen del alto disponible (`s`) y las dos columnas se reparten el
    // ancho con `flex_grow`, así que se acomoda sola en vez de depender de que
    // los números elegidos a mano den justo.
    let vh = touch.height.max(240.0);
    let vw = touch.width.max(320.0);
    let s = (vh / 720.0).clamp(0.45, 1.15);
    let bar_h = (72.0 * s).clamp(44.0, 84.0);
    // La vitrina son 4 muñecos (ancho = alto/2) + el retrato (0.62) + huecos
    // ≈ 2.8 alturas. Ahora la columna izquierda se lleva ~45 % del ancho, no el
    // 80 % de antes, así que el límite por ancho es más estricto.
    let by_h = (vh * 0.26).clamp(52.0, 170.0);
    let by_w = (vw * 0.45) / 2.8;
    let preview_h = by_h.min(by_w).max(48.0);

    commands
        .spawn((
            Node {
                position_type: PositionType::Absolute,
                left: Val::Px(0.0),
                top: Val::Px(0.0),
                width: Val::Percent(100.0),
                height: Val::Percent(100.0),
                flex_direction: FlexDirection::Column,
                justify_content: JustifyContent::FlexStart,
                align_items: AlignItems::Stretch,
                ..default()
            },
            BackgroundColor(VOID),
            GlobalZIndex(1000),
            bevy::ui::FocusPolicy::Block,
            CcRoot,
            Name::new("CharacterCreate"),
        ))
        .with_children(|root| {
            root.spawn((
                Node {
                    width: Val::Percent(100.0),
                    justify_content: JustifyContent::Center,
                    padding: UiRect::top(Val::Px(touch.insets.top as f32 + 4.0 * s)),
                    flex_shrink: 0.0,
                    ..default()
                },
                Name::new("CcTitle"),
            ))
            .with_children(|slot| {
                slot.spawn((
                    Text::new("CREA TU PERSONAJE"),
                    TextFont { font_size: (20.0 * s).clamp(13.0, 26.0), ..default() },
                    TextColor(NEON_CYAN),
                ));
            });

            // ── El cuerpo: dos columnas que se reparten lo que queda de alto ──
            root.spawn((
                Node {
                    width: Val::Percent(100.0),
                    flex_grow: 1.0,
                    flex_shrink: 1.0,
                    flex_basis: Val::Px(0.0),
                    min_height: Val::Px(0.0),
                    flex_direction: FlexDirection::Row,
                    justify_content: JustifyContent::Center,
                    align_items: AlignItems::Center,
                    column_gap: Val::Px(10.0 * s),
                    padding: UiRect::horizontal(Val::Px(
                        touch.insets.left.max(touch.insets.right) as f32 + 10.0,
                    )),
                    ..default()
                },
                Name::new("CcBody"),
            ))
            .with_children(|body| {
                // ── IZQUIERDA: vitrina, retrato y lo que el retrato opina ──
                card(body, s).with_children(|card| {
                    card.spawn(Node {
                        flex_direction: FlexDirection::Row,
                        align_items: AlignItems::FlexEnd,
                        justify_content: JustifyContent::Center,
                        column_gap: Val::Px(8.0 * s),
                        ..default()
                    })
                    .with_children(|row| {
                        for (dir, caption) in SHOWCASE {
                            row.spawn(Node {
                                flex_direction: FlexDirection::Column,
                                align_items: AlignItems::Center,
                                row_gap: Val::Px(2.0 * s),
                                ..default()
                            })
                            .with_children(|slot| {
                                // Las cinco capas apiladas: un contenedor del
                                // tamaño del muñeco y cinco hijos absolutos.
                                slot.spawn(Node {
                                    width: Val::Px(preview_h * 0.5),
                                    height: Val::Px(preview_h),
                                    ..default()
                                })
                                .with_children(|stack| {
                                    for layer in ROWS {
                                        let opt = index_of(&look, layer)
                                            .and_then(|i| cg.options(layer).get(i as usize));
                                        stack.spawn((
                                            ImageNode::from_atlas_image(
                                                opt.map(|o| asset_server.load(&o.png))
                                                    .unwrap_or_default(),
                                                TextureAtlas {
                                                    layout: sheet_layout.clone(),
                                                    index: showcase_cell(dir, 0),
                                                },
                                            ),
                                            Node {
                                                position_type: PositionType::Absolute,
                                                left: Val::Px(0.0),
                                                top: Val::Px(0.0),
                                                width: Val::Percent(100.0),
                                                height: Val::Percent(100.0),
                                                ..default()
                                            },
                                            if opt.is_some() {
                                                Visibility::Inherited
                                            } else {
                                                Visibility::Hidden
                                            },
                                            PreviewLayer { layer, dir },
                                        ));
                                    }
                                });
                                slot.spawn((
                                    Text::new(caption),
                                    TextFont {
                                        font_size: (9.0 * s).clamp(7.0, 11.0),
                                        ..default()
                                    },
                                    TextColor(DIM),
                                ));
                            });
                        }

                        // Separador: la vitrina y el retrato son dos cosas
                        // distintas (cómo te ves andando / cómo te ven al hablar).
                        row.spawn((
                            Node {
                                width: Val::Px(1.0),
                                height: Val::Px(preview_h * 0.8),
                                margin: UiRect::horizontal(Val::Px(5.0 * s)),
                                ..default()
                            },
                            BackgroundColor(CARD_EDGE),
                        ));

                        // El retrato con el que vas a hablar con la gente.
                        row.spawn(Node {
                            flex_direction: FlexDirection::Column,
                            align_items: AlignItems::Center,
                            row_gap: Val::Px(2.0 * s),
                            ..default()
                        })
                        .with_children(|col| {
                            let d = preview_h * 0.62;
                            col.spawn((
                                ImageNode::from_atlas_image(
                                    portrait.clone().unwrap_or_default(),
                                    TextureAtlas { layout: portrait_layout.clone(), index: 0 },
                                ),
                                Node { width: Val::Px(d), height: Val::Px(d), ..default() },
                                PortraitPreview,
                            ));
                            col.spawn((
                                Text::new("TU RETRATO"),
                                TextFont {
                                    font_size: (9.0 * s).clamp(7.0, 11.0),
                                    ..default()
                                },
                                TextColor(DIM),
                            ));
                        });
                    });

                    // **Lo que el retrato opina.** Un globo de diálogo debajo de
                    // la vitrina: cada vez que le cambiás algo, el personaje
                    // comenta. Rompe la cuarta pared a propósito — todavía no
                    // empezó la partida, así que acá te puede hablar a VOS.
                    card.spawn((
                        Node {
                            width: Val::Percent(100.0),
                            min_height: Val::Px((26.0 * s).clamp(20.0, 32.0)),
                            justify_content: JustifyContent::Center,
                            align_items: AlignItems::Center,
                            padding: UiRect::axes(Val::Px(8.0), Val::Px(3.0)),
                            border: UiRect::all(Val::Px(1.0)),
                            ..default()
                        },
                        BorderRadius::all(Val::Px(6.0)),
                        BorderColor(Color::srgba(1.0, 0.18, 0.62, 0.35)),
                        BackgroundColor(Color::srgb(0.06, 0.05, 0.09)),
                    ))
                    .with_children(|bubble| {
                        bubble.spawn((
                            Text::new(quip_for(None, rand_seed())),
                            TextFont {
                                font_size: (12.0 * s).clamp(9.0, 14.0),
                                ..default()
                            },
                            TextColor(NEON_MAGENTA),
                            TextLayout::new_with_justify(JustifyText::Center),
                            PortraitQuip,
                        ));
                    });
                });

                // ── DERECHA: las ruedas y el nombre ──
                body.spawn(Node {
                    flex_grow: 1.0,
                    flex_shrink: 1.0,
                    min_width: Val::Px(0.0),
                    flex_direction: FlexDirection::Column,
                    align_items: AlignItems::Center,
                    justify_content: JustifyContent::Center,
                    row_gap: Val::Px(6.0 * s),
                    ..default()
                })
                .with_children(|col| {
                    // ── CUADRICULA DE ICONOS ──
                    // Antes esto eran cinco ruedas `< NOMBRE >`: para ver un
                    // gorro habia que pasarlos de a uno y leer "GORRA 14".
                    // Ahora se ve la PRENDA y se elige tocandola. La fila de
                    // arriba cambia de capa; abajo va la cuadricula de esa capa.
                    card(col, s).with_children(|card| {
                        card.spawn(Node {
                            flex_direction: FlexDirection::Row,
                            column_gap: Val::Px(4.0 * s),
                            flex_wrap: FlexWrap::Wrap,
                            justify_content: JustifyContent::Center,
                            ..default()
                        })
                        .with_children(|row| {
                            for layer in ROWS {
                                layer_tab(row, layer, layer == grid_layer.0, s);
                            }
                        });
                        // El marco de la cuadricula: alto fijo y `scroll_y`. Es
                        // una LISTA (hay ~200 prendas), no un menu: aca el
                        // scroll si va, igual que en la mochila.
                        card.spawn((
                            Node {
                                width: Val::Percent(100.0),
                                flex_grow: 1.0,
                                // Que se vean CUATRO filas, no una. Con
                                // `min_height: 0` el marco se quedaba en el alto
                                // de su contenido y la cuadricula era una franja
                                // de una fila sola.
                                min_height: Val::Px(celda_px(touch.width) * 4.0),
                                // ARRIBA, no estirado. El marco es una FILA y en
                                // una fila el eje cruzado es el alto: con el
                                // `Stretch` por defecto la cuadricula se estiraba
                                // al alto del marco, o sea `contenido == vista`, y
                                // `max_scroll` daba 0. Mismo bug que tenian las
                                // tres pestanas de la mochila.
                                align_items: AlignItems::FlexStart,
                                overflow: Overflow::scroll_y(),
                                ..default()
                            },
                            ScrollPosition::default(),
                            CcScroll,
                            Name::new("CcGridFrame"),
                        ))
                        .with_children(|frame| {
                            frame.spawn((
                                Node {
                                    width: Val::Percent(100.0),
                                    // CUADRICULA DE 3, no un `flex_wrap` que
                                    // mete las que entren. Con el wrap el ancho
                                    // de la celda mandaba y en el Redmi entraban
                                    // seis prendas de 52 px: no se veia cual era
                                    // cual. Con 3 columnas fraccionarias la
                                    // celda mide un tercio del panel — grande,
                                    // siempre alineada, y la fila no cambia de
                                    // largo al rotar el telefono.
                                    // TRES POR FILA con flex-wrap, no con
                                    // `Display::Grid`. Con grilla, una celda
                                    // cuadrada por `aspect_ratio` inflaba la
                                    // PISTA a ~3 veces su alto: el panel crecia
                                    // tanto que empujaba las pestanas y el campo
                                    // NOMBRE fuera de la pantalla. En flex el
                                    // alto de la linea ES el de la celda, asi que
                                    // no hay nada que inflar; y con 31% de ancho
                                    // entran exactamente tres.
                                    flex_direction: FlexDirection::Row,
                                    flex_wrap: FlexWrap::Wrap,
                                    justify_content: JustifyContent::Center,
                                    align_content: AlignContent::FlexStart,
                                    max_width: Val::Px(grilla_px(touch.width, s)),
                                    column_gap: Val::Px(6.0 * s),
                                    row_gap: Val::Px(6.0 * s),
                                    padding: UiRect::all(Val::Px(4.0 * s)),
                                    // Sin esto no hay scroll: ver hud.rs.
                                    flex_shrink: 0.0,
                                    ..default()
                                },
                                crate::hud::ScrollContent,
                                CcGrid,
                            ));
                        });
                    });

                    // ── NOMBRE ──
                    // El recuadro es un BOTÓN: tocarlo abre el teclado en
                    // pantalla (`CcAction::OpenKeyboard`). En el teléfono no hay
                    // teclado físico, así que sin esto el nombre no se podía
                    // escribir — quedaba el que vino sorteado. En escritorio se
                    // sigue tecleando directo (`name_input`).
                    card(col, s).with_children(|card| {
                        card.spawn(Node {
                            flex_direction: FlexDirection::Row,
                            align_items: AlignItems::Center,
                            justify_content: JustifyContent::Center,
                            column_gap: Val::Px(8.0 * s),
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
                                    Text::new("NOMBRE:"),
                                    TextFont {
                                        font_size: (13.0 * s).clamp(10.0, 15.0),
                                        ..default()
                                    },
                                    TextColor(DIM),
                                ));
                            });
                            row.spawn((
                                Button,
                                Node {
                                    min_width: Val::Px((220.0 * s).clamp(140.0, 240.0)),
                                    height: Val::Px((36.0 * s).clamp(28.0, 40.0)),
                                    border: UiRect::all(Val::Px(2.0)),
                                    justify_content: JustifyContent::Center,
                                    align_items: AlignItems::Center,
                                    padding: UiRect::horizontal(Val::Px(10.0)),
                                    ..default()
                                },
                                BorderRadius::all(Val::Px(6.0)),
                                BorderColor(NEON_CYAN),
                                BackgroundColor(idle_bg(CcAction::OpenKeyboard)),
                                CcAction::OpenKeyboard,
                            ))
                            .with_children(|b| {
                                b.spawn((
                                    Text::new(selected.name.clone()),
                                    TextFont {
                                        font_size: (18.0 * s).clamp(13.0, 22.0),
                                        ..default()
                                    },
                                    TextColor(Color::WHITE),
                                    NameText,
                                ));
                            });
                        });
                        card.spawn((
                            Text::new("(tocalo para escribir)"),
                            TextFont { font_size: (10.0 * s).clamp(8.0, 12.0), ..default() },
                            TextColor(DIM),
                        ));
                    });
                });
            });

            // Barra de acción al pie: hermana del contenido (no hija), con alto
            // propio → JUGAR/VOLVER siempre visibles y a un toque.
            root.spawn((
                Node {
                    width: Val::Percent(100.0),
                    height: Val::Px(bar_h + touch.insets.bottom as f32),
                    flex_shrink: 0.0,
                    flex_direction: FlexDirection::Row,
                    justify_content: JustifyContent::Center,
                    align_items: AlignItems::Center,
                    column_gap: Val::Px(10.0),
                    padding: UiRect::bottom(Val::Px(touch.insets.bottom as f32)),
                    border: UiRect::top(Val::Px(2.0)),
                    ..default()
                },
                BorderColor(Color::srgba(0.16, 0.94, 1.0, 0.35)),
                BackgroundColor(Color::srgb(0.03, 0.04, 0.08)),
                Name::new("CcButtons"),
            ))
            .with_children(|row| {
                let w_play = (180.0 * s).clamp(110.0, 200.0);
                let w_rand = (140.0 * s).clamp(96.0, 150.0);
                let w_back = (120.0 * s).clamp(84.0, 140.0);
                // VOLVER primero, JUGAR último y relleno: se lee de izquierda a
                // derecha y la acción que avanza queda del lado del pulgar.
                action_button(row, "VOLVER", CcAction::Back, Color::srgb(0.6, 0.6, 0.7), w_back, s, false);
                action_button(row, "AL AZAR", CcAction::Random, NEON_MAGENTA, w_rand, s, false);
                action_button(row, "JUGAR >", CcAction::Play, NEON_CYAN, w_play, s, true);
            });
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
    mut interactions: Query<(&Interaction, &CcAction, &mut BackgroundColor), Changed<Interaction>>,
    cg: Res<Chargen>,
    mut selected: ResMut<SelectedCharacter>,
    mut next: ResMut<NextState<GamePhase>>,
    root: Query<Entity, With<CcRoot>>,
    mut keyboard: ResMut<KeyboardOpen>,
    mut grid_layer: ResMut<GridLayer>,
    mut name_text: Query<&mut Text, (With<NameText>, Without<PortraitQuip>)>,
    mut quip: Query<&mut Text, With<PortraitQuip>>,
    mut toque: ResMut<Toque>,
) {
    let mut look = selected.look.unwrap_or_default();
    let mut dirty = false;
    // Lo que el retrato va a decir este frame, si es que dice algo.
    let mut say: Option<String> = None;
    for (interaction, action, mut bg) in &mut interactions {
        match interaction {
            Interaction::Pressed => {
                *bg = BackgroundColor(Color::srgb(0.16, 0.24, 0.36));
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
            // El reposo vuelve al fondo QUE LE TOCA a ese botón: JUGAR es el
            // único relleno, y pintarlo del mismo gris que los demás al soltarlo
            // le borraba la jerarquía en cuanto lo tocabas una vez.
            Interaction::Hovered => *bg = BackgroundColor(Color::srgb(0.14, 0.20, 0.30)),
            Interaction::None => *bg = BackgroundColor(idle_bg(*action)),
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
    )>,
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
    for (mut text, StyleLabel(layer)) in &mut sets.p2() {
        text.0 = style_text(&cg, &look, *layer);
    }
    for (mut text, ColorLabel(layer)) in &mut sets.p3() {
        text.0 = color_text(&cg, &look, *layer);
    }
}

fn start_game(selected: &SelectedCharacter) {
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
    "Otra piel? Dale, total no me duele.",
    "Con esta me veo bien hasta de noche.",
    "Ojo que despues no te la cambio.",
    "Esa me gusta. Pero vos sabras.",
];
const QUIP_OJOS: &[&str] = &[
    "Con estos ojos veo el mismo mundo, eh.",
    "Me estas mirando o me estas eligiendo?",
    "Estos dejalos. Con estos veo bien.",
];
const QUIP_ROPA: &[&str] = &[
    "Linda ropa. Igual me la voy a ensuciar.",
    "Esto no abriga nada. Sabes que hay noche?",
    "Me vestis vos y despues se rien de mi.",
    "Con esto puesto no me toma nadie en serio.",
];
const QUIP_PELO: &[&str] = &[
    "El pelo asi? Bueno. Vos sabras.",
    "Peluquero de mundo abierto, tenemos.",
    "Ese corte se lo vi a un aldeano. Lo copiaste.",
];
const QUIP_GORRO: &[&str] = &[
    "El gorro es lo unico que me representa.",
    "Sin gorro me siento desnudo.",
    "Con este gorro me van a ver de lejos. Eso es bueno?",
];
const QUIP_AZAR: &[&str] = &[
    "Al azar? Gracias por el esfuerzo.",
    "Y asi nace un heroe. De casualidad.",
    "Tira de nuevo, dale. No soy yo el que sufre.",
];
const QUIP_NOMBRE: &[&str] = &[
    "Con ese nombre me voy a tener que hacer respetar.",
    "Ese nombre me lo van a gritar los aldeanos. Pensalo.",
    "Listo. Ya soy alguien.",
];
const QUIP_HOLA: &[&str] = &[
    "Bueno, aca estoy. Hace lo que quieras.",
    "Antes de empezar: gorro si, o gorro no?",
    "Tocame algo. No me dejes asi.",
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
fn sync_keyboard(
    open: Res<KeyboardOpen>,
    mut commands: Commands,
    touch: Res<crate::input::TouchLayout>,
    live: Query<Entity, With<CcKeyboard>>,
) {
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
    let vh = touch.height.max(240.0);
    let s = (vh / 720.0).clamp(0.45, 1.15);
    let key = (34.0 * s).clamp(26.0, 40.0);
    commands
        .spawn((
            Node {
                position_type: PositionType::Absolute,
                left: Val::Px(0.0),
                top: Val::Px(0.0),
                width: Val::Percent(100.0),
                height: Val::Percent(100.0),
                flex_direction: FlexDirection::Column,
                justify_content: JustifyContent::Center,
                align_items: AlignItems::Center,
                row_gap: Val::Px(4.0 * s),
                ..default()
            },
            // Encima de la pantalla de creacion (que va en 1000).
            GlobalZIndex(1100),
            BackgroundColor(Color::srgba(0.02, 0.03, 0.06, 0.94)),
            bevy::ui::FocusPolicy::Block,
            CcKeyboard,
            Name::new("CcKeyboard"),
        ))
        .with_children(|kb| {
            kb.spawn((
                Text::new("ESCRIBI TU NOMBRE"),
                TextFont { font_size: (15.0 * s).clamp(11.0, 18.0), ..default() },
                TextColor(NEON_CYAN),
                Node { margin: UiRect::bottom(Val::Px(6.0 * s)), ..default() },
            ));
            for row in KEY_ROWS {
                kb.spawn(Node {
                    flex_direction: FlexDirection::Row,
                    column_gap: Val::Px(4.0 * s),
                    ..default()
                })
                .with_children(|line| {
                    for c in row.chars() {
                        key_button(line, c, key, s);
                    }
                });
            }
            kb.spawn(Node {
                flex_direction: FlexDirection::Row,
                column_gap: Val::Px(8.0 * s),
                margin: UiRect::top(Val::Px(6.0 * s)),
                ..default()
            })
            .with_children(|line| {
                wide_key(line, "BORRAR", CcAction::KeyBack, key, s);
                wide_key(line, "LISTO", CcAction::KeyDone, key, s);
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
fn fill_grid(
    commands: &mut Commands,
    grid: Entity,
    cg: &Chargen,
    look: &LookDef,
    layer: Layer,
    ancho_pantalla: f32,
    s: f32,
) {
    let d = celda_px(ancho_pantalla);
    let current = index_of(look, layer);
    commands.entity(grid).despawn_descendants();
    commands.entity(grid).with_children(|grid| {
        // El accesorio puede NO estar: "sin gorro" es una eleccion, no un hueco.
        if layer == Layer::Accessory {
            grid_cell(grid, layer, None, current.is_none(), d, s, None);
        }
        for (i, opt) in cg.options(layer).iter().enumerate() {
            let idx = i as u32;
            grid_cell(grid, layer, Some(idx), current == Some(idx), d, s, Some(opt.png.clone()));
        }
    });
}

#[allow(clippy::too_many_arguments)]
fn grid_cell(
    parent: &mut ChildBuilder,
    layer: Layer,
    idx: Option<u32>,
    selected: bool,
    d: f32,
    s: f32,
    png: Option<String>,
) {
    let mut cell = parent.spawn((
        Button,
        Node {
            // La celda llena SU COLUMNA (un tercio del panel) y se mantiene
            // cuadrada. Antes era un cuadrado fijo de 52 px como mucho: en una
            // pantalla de 2400 px de ancho sobraba panel a los costados y la
            // prenda se veia como una estampilla.
            // Cuadrada por `aspect_ratio` sobre el ancho de la columna.
            //
            // Probe el truco de CSS de `height: 0` + `padding-top: 100%` creyendo
            // que taffy resolvia el padding porcentual contra el ANCHO como el
            // navegador: NO lo hace. Las celdas colapsaban a alto ~0 y la
            // cuadricula quedaba en tres tiras rayadas. `aspect_ratio` es lo que
            // funciona aca.
            // PIXELES FIJOS. Con el ancho en porcentaje la celda se resolvia
            // contra un contenedor que no podia encogerse (`flex_shrink: 0`) y
            // el resultado eran barras anchisimas que se salian de la pantalla.
            // Fijo no puede explotar, y el tamano sale del ancho REAL del panel
            // (medido en el telefono: ~21% del ancho de pantalla) dividido en 3.
            width: Val::Px(d),
            height: Val::Px(d),
            justify_content: JustifyContent::Center,
            align_items: AlignItems::Center,
            border: UiRect::all(Val::Px(if selected { 3.0 } else { 1.0 })),
            overflow: Overflow::clip(),
            ..default()
        },
        BorderRadius::all(Val::Px(10.0 * s)),
        // La elegida se marca con BORDE y FONDO, no solo con el borde: en el
        // telefono a plena luz un borde de 2 px no se distingue de uno de 1.
        BorderColor(if selected { NEON_MAGENTA } else { CARD_EDGE }),
        // La celda VACIA tiene que leerse como celda. El gris de antes
        // (0.08,0.10,0.16) era casi el mismo azul del panel: mientras todas las
        // prendas estaban cargadas no se notaba, pero al cargar solo las
        // visibles (`stream_thumbs`) las demas quedaban como agujeros y la
        // cuadricula parecia rota, con dos filas sueltas flotando. Con un fondo
        // que contrasta se ve la grilla completa y las que faltan se leen como
        // "todavia no cargo", que es la verdad.
        BackgroundColor(if selected {
            Color::srgb(0.24, 0.07, 0.15)
        } else {
            Color::srgb(0.15, 0.17, 0.23)
        }),
        CcAction::PickOption(layer, idx),
        CcCell(layer, idx),
    ));
    match png {
        Some(ruta) => {
            // OJO: acá NO se carga la textura. Se anota la ruta y el hueco donde
            // va, y `stream_thumbs` la trae cuando la celda entra en pantalla.
            // Cargarlas todas era 1,2 GB y el SIGKILL de Android (ver `CcThumb`).
            cell.insert(CcThumb(ruta));
            cell.with_children(|c| {
                c.spawn((
                    // El muneco es 1:2; la celda es cuadrada. Se dibuja alto y
                    // angosto y centrado, que es como se ve la prenda puesta.
                    // En PORCENTAJE de la celda, no en pixeles fijos: asi la
                    // prenda crece con la celda en vez de quedar chiquita en el
                    // medio de un recuadro grande.
                    Node {
                        // ABSOLUTO a proposito. La celda saca su alto de su
                        // `aspect_ratio` sobre el ancho de la columna; si el
                        // sprite queda EN EL FLUJO, taffy intenta a la vez sacar
                        // el alto de la celda de su hijo y el alto del hijo de la
                        // celda, y resuelve el ciclo inflando la fila a ~3 veces
                        // lo que mide la celda (se veian tres huecos enormes con
                        // una prenda chiquita en el medio). Fuera del flujo, la
                        // celda manda y el sprite la sigue.
                        // Centrado por las CUATRO inserciones, no por
                        // `aspect_ratio` + centrado del padre: 44% de ancho y 88%
                        // de alto sobre una celda cuadrada dan justo el 1:2 del
                        // muneco, y quedan centradas por construccion. Como la
                        // celda saca su alto del padding, el sprite ya no puede
                        // inflar la fila.
                        // ACERCAMIENTO. La celda del atlas trae la prenda sola
                        // sobre transparente y ocupando una parte chica de sus
                        // 32x64: dibujada "entera" se veia como una estampilla
                        // en el medio de un recuadro grande. Se dibuja MAS
                        // GRANDE que la celda y la celda recorta (`overflow:
                        // clip`), que es un zoom hecho desde el juego — sin
                        // tocar el recorte del manifest, que lo pinta el
                        // usuario en el editor (regla 7).
                        // La imagen YA es la celda recortada (`Miniaturas`), no
                        // la hoja entera: dibujarla agrandada y recortada dejaba
                        // ver el medio del muneco, que en una capa de ropa es
                        // transparente. Se dibuja entera y centrada.
                        position_type: PositionType::Absolute,
                        left: Val::Percent(24.0),
                        right: Val::Percent(24.0),
                        top: Val::Percent(4.0),
                        bottom: Val::Percent(4.0),
                        ..default()
                    },
                    CcThumbSlot,
                ));
            });
        }
        None => {
            cell.with_children(|c| {
                c.spawn((
                    Text::new("NINGUNO"),
                    TextFont { font_size: (13.0 * s).clamp(10.0, 18.0), ..default() },
                    TextColor(DIM),
                ));
            });
        }
    }
}

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
    mut tabs: Query<(&CcLayerTab, &mut BorderColor, &mut BackgroundColor)>,
    mut last: Local<Option<(Layer, Option<u32>)>>,
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
    let key = (layer, None);
    if *last == Some(key) {
        return;
    }
    *last = Some(key);
    let s = (touch.height.max(240.0) / 720.0).clamp(0.45, 1.15);
    fill_grid(&mut commands, grid_e, &cg, &look, layer, touch.width, s);
    for (tab, mut border, mut bg) in &mut tabs {
        let active = tab.0 == layer;
        *border = BorderColor(if active { NEON_CYAN } else { CARD_EDGE });
        *bg = BackgroundColor(if active {
            Color::srgb(0.10, 0.22, 0.30)
        } else {
            Color::srgb(0.08, 0.10, 0.16)
        });
    }
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
