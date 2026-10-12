//! **Pantalla de carga** — el único momento del juego donde se puede gastar de
//! más sin que se note.
//!
//! El problema que arregla: al tocar JUGAR, el mundo nacía **con vos adentro**.
//! El streaming de chunks construye 2 por frame (medido: 6-12 ms cada uno, ver
//! `world2d::CHUNK_BUILD_BUDGET_PER_FRAME`), así que los primeros segundos de
//! partida eran el suelo apareciendo a pedazos, los árboles cayendo del cielo y
//! el frame pegando saltos — justo cuando el jugador mira más.
//!
//! Cómo se arregla: se separa "el mundo existe" de "estás jugando".
//!
//! 1. JUGAR entra a [`GamePhase::Loading`]: `scene::spawn_player` y
//!    `universe::spawn_universe` corren ACÁ, y todo lo que construye mundo sigue
//!    andando (`game_phase::world_active`).
//! 2. Una cortina opaca tapa la pantalla — el mundo se está armando detrás, y de
//!    paso se compilan los pipelines y suben las texturas a la GPU, que es la
//!    otra mitad del tirón del primer frame.
//! 3. El presupuesto de chunks se sube a [`LOADING_CHUNK_BUDGET`]: un frame de 200
//!    ms detrás de la cortina no es un tirón, es trabajo adelantado.
//! 4. Se levanta la cortina cuando **las texturas terminaron de cargar**
//!    (`art2d::ArtLibrary::image_handles`), el anillo visible está armado
//!    (`world2d::ChunkBacklog` en 0), los personajes están compuestos
//!    (`chargen::ChargenJobs`) y hay [`STABLE_FRAMES`] frames seguidos rápidos.
//!    Con tope de [`MAX_SECS`]: nunca te deja encerrado.
//!
//! El paso de las texturas faltaba y era la mitad del problema: los PNG se
//! piden en `PreStartup` pero se decodifican en hilos de IO, así que la cortina
//! se levantaba con el mundo armado y el arte todavía viajando. Un sprite con
//! el handle sin cargar no se dibuja: el árbol aparecía de golpe unos frames
//! después, y la subida a GPU de cada textura caía **dentro** del gameplay.
//!
//! Esto NO contradice la regla 8 ("una transición es un cambio de marco, jamás
//! una cutscene"): esa regla habla de las transiciones EN JUEGO —despegue,
//! órbita, entrar a un planeta—, que siguen sin cortar nada. Esto es el arranque
//! de la partida, antes de que exista el jugador.

use bevy::asset::RecursiveDependencyLoadState;
use bevy::prelude::*;

use crate::art2d::ArtLibrary;
use crate::chargen::ChargenJobs;
use crate::game_phase::GamePhase;
use crate::universe::{DiscBakeQueue, DiscBakeRows};
use crate::world2d::{ChunkBacklog, ChunkBudget};

/// Chunks por frame mientras la cortina está abajo. Con el anillo de arranque
/// (7×7 = 49 chunks) esto lo arma en ~2 segundos en vez de en ~25.
const LOADING_CHUNK_BUDGET: usize = 12;

/// Filas del disco del planeta por frame mientras la cortina está abajo. Con las
/// 32 de siempre, las 512 filas son 16 frames que caen DENTRO del gameplay
/// costando el frame entero cada una (ver `universe::bake_body_textures`); acá
/// se hace de una y no se ve.
const LOADING_DISC_ROWS: usize = 128;

/// Frames seguidos SIN trabajo pendiente que hay que ver antes de levantar la
/// cortina. Uno solo no prueba nada: el frame siguiente puede ser el que compila
/// un pipeline.
const STABLE_FRAMES: u32 = 8;

/// Además se esperan [`SMOOTH_FRAMES`] frames rápidos: que el mundo esté armado
/// no alcanza, el primer frame jugable también tiene que salir barato.
const STABLE_MS: f32 = 34.0;
const SMOOTH_FRAMES: u32 = 3;

/// …pero no para siempre. En un build de debug (o en un teléfono muy justo) NINGÚN
/// frame baja de 34 ms, y exigirlo dejaba la cortina puesta los 20 s enteros con
/// el mundo ya listo — medido en la corrida de QA. Pasado esto se entra igual.
const SMOOTH_GIVEUP_SECS: f32 = 6.0;

/// Tope duro. Si el mundo no termina de armarse (teléfono lento, planeta raro),
/// se juega igual: una pantalla de carga eterna es peor que un par de tirones.
const MAX_SECS: f32 = 20.0;

/// Mínimo en pantalla. Sin esto, en una máquina rápida la cortina aparece y
/// desaparece en dos frames y queda un parpadeo negro feo.
const MIN_SECS: f32 = 0.6;

const NEON_CYAN: Color = Color::srgb(0.16, 0.94, 1.0);
const NEON_MAGENTA: Color = Color::srgb(1.0, 0.18, 0.62);
const VOID: Color = Color::srgb(0.04, 0.05, 0.09);

/// Lo que se lee mientras carga. Son las cosas que el juego NO te explica en
/// ningún lado; la pantalla de carga es el único rato en que alguien las lee.
/// ASCII-only (regla 6).
const CONSEJOS: [&str; 8] = [
    "Mantene el dedo en el suelo para volar. Soltar aterriza.",
    "Los aldeanos se acuerdan de lo que hiciste la ultima vez.",
    "De noche la luz sale de las antorchas: llevate una.",
    "Cada planeta tiene relieve propio. Ninguno es una bola lisa.",
    "La fauna aprende: lo que te funciono ayer hoy puede fallar.",
    "Subi derecho y el mundo se vuelve mapa. Nunca hay corte.",
    "Podes editar el mundo y queda guardado en tu partida.",
    "Toca a un aldeano para hablarle. Tiene cara y humor propios.",
];

#[cfg(test)]
mod tests {
    use super::*;

    /// Regla 6: la fuente del juego no dibuja acentos — un consejo con tilde sale
    /// con una caja en el medio (y así salio el primero que escribi).
    #[test]
    fn los_consejos_son_ascii() {
        for tip in CONSEJOS {
            assert!(tip.is_ascii(), "consejo con caracteres que la fuente no dibuja: {tip}");
        }
    }

    /// La cortina tiene que poder levantarse. El bug que esto cierra: exigir
    /// frames de menos de 34 ms en un build donde NINGUNO baja de eso dejaba la
    /// pantalla puesta los 20 s con el mundo ya armado.
    #[test]
    fn el_tope_de_tiempo_deja_salir() {
        assert!(MIN_SECS < MAX_SECS);
        assert!(
            SMOOTH_GIVEUP_SECS < MAX_SECS,
            "si nunca hay un frame rapido, se entra por el giveup, no por el tope"
        );
        assert!(LOADING_CHUNK_BUDGET > ChunkBudget::normal());
    }

    /// El horneado del disco del planeta cuesta el frame entero mientras dura
    /// (medido: 125-384 ms por frame, `ha-sys`). Detrás de la cortina tiene que
    /// ir MÁS rápido que en juego, o la cortina se levanta con la cola a medias
    /// y el jugador paga los frames que faltan — que es el bug de "se laggea dos
    /// minutos y despues levanta solo".
    #[test]
    fn el_disco_se_cocina_mas_rapido_detras_de_la_cortina() {
        assert!(LOADING_DISC_ROWS > DiscBakeRows::normal());
    }
}

pub struct LoadingPlugin;

impl Plugin for LoadingPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<LoadingState>()
            .add_systems(OnEnter(GamePhase::Loading), (spawn_curtain, boost_budget))
            .add_systems(OnExit(GamePhase::Loading), (despawn_curtain, restore_budget))
            .add_systems(Update, tick.run_if(in_state(GamePhase::Loading)));
    }
}

#[derive(Resource, Default)]
struct LoadingState {
    elapsed: f32,
    stable: u32,
    smooth: u32,
    /// Progreso mostrado (0..1). Sube y NUNCA baja: una barra que retrocede
    /// porque el anillo creció se lee como que algo se rompió.
    shown: f32,
    backlog0: f32,
    /// Texturas que el manifest pidió, snapshot al entrar. Se recorren cada
    /// frame para ver cuántas terminaron; se vacía cuando ya están todas para
    /// no seguir preguntando por nada.
    art: Vec<Handle<Image>>,
    art_total: usize,
}

#[derive(Component)]
struct CurtainRoot;
#[derive(Component)]
struct ProgressBar;
#[derive(Component)]
struct ProgressText;

fn boost_budget(mut budget: ResMut<ChunkBudget>, mut rows: ResMut<DiscBakeRows>) {
    budget.0 = LOADING_CHUNK_BUDGET;
    rows.0 = LOADING_DISC_ROWS;
}

fn restore_budget(mut budget: ResMut<ChunkBudget>, mut rows: ResMut<DiscBakeRows>) {
    budget.0 = ChunkBudget::normal();
    rows.0 = DiscBakeRows::normal();
}

fn spawn_curtain(
    mut commands: Commands,
    mut state: ResMut<LoadingState>,
    art: Res<ArtLibrary>,
    touch: Res<crate::input::TouchLayout>,
) {
    *state = LoadingState::default();
    state.art = art.image_handles();
    state.art_total = state.art.len();
    let vh = touch.height.max(240.0);
    let s = (vh / 720.0).clamp(0.45, 1.15);
    // Un consejo distinto por partida (la hora alcanza: no hace falta que sea
    // determinista).
    let tip = CONSEJOS[(std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as usize)
        .unwrap_or(0))
        % CONSEJOS.len()];

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
                row_gap: Val::Px(14.0 * s),
                padding: UiRect::horizontal(Val::Px(24.0)),
                ..default()
            },
            BackgroundColor(VOID),
            // Por encima de TODO: del HUD, del menú y de cualquier cosa que se
            // encienda antes de tiempo. La cortina no puede tener rendijas.
            GlobalZIndex(2000),
            bevy::ui::FocusPolicy::Block,
            CurtainRoot,
            Name::new("LoadingCurtain"),
        ))
        .with_children(|root| {
            root.spawn((
                Text::new("PREPARANDO EL MUNDO"),
                TextFont { font_size: (26.0 * s).clamp(16.0, 34.0), ..default() },
                TextColor(NEON_CYAN),
            ));
            // Barra de progreso: marco fijo + relleno que crece.
            root.spawn((
                Node {
                    width: Val::Percent(70.0),
                    max_width: Val::Px(420.0),
                    height: Val::Px((16.0 * s).clamp(10.0, 18.0)),
                    border: UiRect::all(Val::Px(2.0)),
                    ..default()
                },
                BorderColor(NEON_CYAN),
                BackgroundColor(Color::srgb(0.06, 0.08, 0.12)),
            ))
            .with_children(|bar| {
                bar.spawn((
                    Node { width: Val::Percent(0.0), height: Val::Percent(100.0), ..default() },
                    BackgroundColor(NEON_MAGENTA),
                    ProgressBar,
                ));
            });
            root.spawn((
                Text::new("generando terreno..."),
                TextFont { font_size: (13.0 * s).clamp(10.0, 15.0), ..default() },
                TextColor(Color::srgb(0.7, 0.75, 0.85)),
                ProgressText,
            ));
            root.spawn((
                Text::new(format!("CONSEJO: {tip}")),
                TextFont { font_size: (13.0 * s).clamp(10.0, 15.0), ..default() },
                TextColor(Color::srgb(0.55, 0.6, 0.72)),
                Node { margin: UiRect::top(Val::Px(18.0 * s)), ..default() },
            ));
        });
}

fn despawn_curtain(mut commands: Commands, q: Query<Entity, With<CurtainRoot>>) {
    for e in &q {
        commands.entity(e).despawn_recursive();
    }
}

#[allow(clippy::too_many_arguments)]
fn tick(
    time: Res<Time<bevy::time::Real>>,
    assets: Res<AssetServer>,
    mut state: ResMut<LoadingState>,
    backlog: Res<ChunkBacklog>,
    discs: Res<DiscBakeQueue>,
    jobs: Res<ChargenJobs>,
    player: Query<(), With<crate::sim::Player>>,
    mut next: ResMut<NextState<GamePhase>>,
    mut bar: Query<&mut Node, With<ProgressBar>>,
    mut text: Query<&mut Text, With<ProgressText>>,
) {
    let dt = time.delta_secs();
    state.elapsed += dt;

    // --- 1. TEXTURAS: se van tachando las que terminaron ---
    // Un PNG que falta (slot sin arte asignada) cuenta como terminado: el juego
    // ya sabe caer al respaldo procedural, esperarlo sería esperar para siempre.
    state.art.retain(|h| {
        !matches!(
            assets.get_recursive_dependency_load_state(h),
            Some(RecursiveDependencyLoadState::Loaded) | Some(RecursiveDependencyLoadState::Failed(_)) | None
        )
    });
    let art_left = state.art.len();
    let art_ready = art_left == 0;

    // --- 2. MUNDO y 3. PERSONAJES ---
    // El backlog del primer pase es el 100% del trabajo; de ahí sale el progreso.
    let pending = backlog.0 as f32;
    if state.backlog0 <= 0.0 {
        state.backlog0 = pending;
    }
    let world_ready = backlog.0 == 0 && !player.is_empty();
    let chars_ready = jobs.idle();
    // El mapa del planeta que estás pisando. Es lo que faltaba esperar: cocinarlo
    // cuesta el frame entero durante 16 frames seguidos, así que si la cortina se
    // levanta antes, esos 16 frames los paga el jugador (§10).
    let discs_ready = discs.pending() == 0;
    let all_ready = art_ready && world_ready && chars_ready && discs_ready;

    // La barra reparte el recorrido entre las tres etapas para que avance
    // durante las tres y no se clave en 30% esperando texturas.
    let art_ratio = if state.art_total > 0 {
        1.0 - art_left as f32 / state.art_total as f32
    } else {
        1.0
    };
    let world_ratio = if state.backlog0 > 0.0 {
        1.0 - (pending / state.backlog0).clamp(0.0, 1.0)
    } else {
        f32::from(u8::from(world_ready))
    };
    // Mezcla con el tiempo para que la barra siempre se mueva un poco: una barra
    // clavada parece colgada aunque esté trabajando.
    let target = (art_ratio * 0.35 + world_ratio * 0.45 + (state.elapsed / MAX_SECS).min(1.0) * 0.2)
        .clamp(0.0, 1.0);
    let target = if all_ready { 1.0 } else { target.min(0.97) };
    state.shown = state.shown.max(target);

    if let Ok(mut node) = bar.get_single_mut() {
        node.width = Val::Percent(state.shown * 100.0);
    }
    if let Ok(mut t) = text.get_single_mut() {
        let msg = if !art_ready {
            "cargando texturas..."
        } else if !world_ready {
            "generando terreno..."
        } else if !discs_ready {
            "pintando el planeta..."
        } else if !chars_ready {
            "vistiendo a la gente..."
        } else {
            "listo"
        };
        if t.0 != msg {
            t.0 = msg.into();
        }
    }

    // Frames sin trabajo pendiente (se reinicia si vuelve a aparecer trabajo) y,
    // aparte, frames rápidos.
    if all_ready {
        state.stable += 1;
        if dt * 1000.0 <= STABLE_MS {
            state.smooth += 1;
        }
    } else {
        state.stable = 0;
    }

    let smooth_enough = state.smooth >= SMOOTH_FRAMES || state.elapsed >= SMOOTH_GIVEUP_SECS;
    let done = state.elapsed >= MIN_SECS && state.stable >= STABLE_FRAMES && smooth_enough;
    if done || state.elapsed >= MAX_SECS {
        if !done {
            eprintln!(
                "loading: se acabo el tiempo ({MAX_SECS} s) con {} chunks, {art_left} texturas y {} discos pendientes — entro igual",
                backlog.0,
                discs.pending()
            );
        } else {
            eprintln!(
                "loading: listo en {:.1} s ({} texturas, anillo armado)",
                state.elapsed, state.art_total
            );
        }
        next.set(GamePhase::Playing);
    }
}
