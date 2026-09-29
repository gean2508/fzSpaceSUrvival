use std::collections::VecDeque;

use bevy::prelude::*;
use bevy::window::PrimaryWindow;

use crate::camera::GameCamera;
use crate::debug::{CpuSpanSlot, CpuSpanStats};
use crate::targeting::InteractTarget;
use crate::ui_theme;

pub struct PerfDebugPlugin;

impl Plugin for PerfDebugPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<PerfStats>()
            .init_resource::<PerfOverlayVisible>()
            .init_resource::<PerfBreakdownTimer>()
            .init_resource::<PerfRingBuffer>();
        // **En el teléfono, nada de esto.** Son herramientas de la compu: el
        // overlay, el log periódico (ordenaba todos los sprites visibles cada
        // 5 s — un tirón fijo), el log de tirones y el censo. Los recursos
        // quedan porque otros sistemas los leen (el reporte de cuelgues).
        #[cfg(target_os = "android")]
        return;
        #[cfg(not(target_os = "android"))]
        app
            .add_systems(Startup, spawn_perf_overlay)
            .add_systems(
                Update,
                (
                    toggle_perf_overlay,
                    sample_frame_timing,
                    sample_perf_breakdown,
                    update_perf_overlay,
                    log_perf_periodic,
                    log_hitches,
                )
                    .chain(),
            )
            .add_systems(Update, censo_de_entidades);
        #[cfg(not(target_os = "android"))]
        app.add_systems(Update, debug_screenshot);
    }
}

/// **Nunca en el teléfono.** Una captura es una lectura del framebuffer entero
/// más un PNG comprimido a disco: dos tirones garantizados a los 7 s y 11 s de
/// CADA partida, en el dispositivo donde menos margen hay. Es una herramienta
/// para las corridas automáticas de escritorio (`HA_AUTOPLAY`, ver `swarm-sim`),
/// así que ahí es donde vive.
#[cfg(not(target_os = "android"))]
fn debug_screenshot(
    mut commands: Commands,
    time: Res<Time>,
    keys: Res<ButtonInput<KeyCode>>,
    mut state: Local<(bool, u32)>,
) {
    use bevy::render::view::screenshot::{save_to_disk, Screenshot};
    // El disparo automático es sólo para los drivers de QA; jugando en
    // escritorio, F12 sigue sacando una a pedido.
    let autoplay = crate::flags::var("HA_AUTOPLAY").is_ok();
    let auto_due = autoplay
        && ((!state.0 && time.elapsed_secs() > 7.0)
            || (state.0 && state.1 == 1 && time.elapsed_secs() > 11.0));
    let manual = keys.just_pressed(KeyCode::F12);
    if !auto_due && !manual {
        return;
    }
    if auto_due {
        state.0 = true;
    }
    let n = state.1;
    state.1 += 1;
    let path = format!("ha-shot-{n}.png");
    commands.spawn(Screenshot::primary_window()).observe(save_to_disk(path.clone()));
    eprintln!("ha-debug: screenshot → {path}");
}

#[derive(Resource, Default)]
pub struct PerfOverlayVisible(pub bool);

#[derive(Resource, Default)]
struct PerfBreakdownTimer {
    elapsed: f32,
    overlay_elapsed: f32,
}

#[derive(Resource, Default, Clone, Copy, Debug)]
pub struct PerfRenderBreakdown {
    pub ocean: u32,
    pub island_ground: u32,
    pub static_world: u32,
    pub chunk_decor: u32,
    pub obstacles: u32,
    pub interactables: u32,
    pub outline: u32,
    pub other_3d: u32,
    pub ui_nodes: u32,
    /// Sprites que el motor va a dibujar este frame (los visibles de verdad).
    pub sprite_visible: u32,
    /// Lotes que salen de esos sprites ≈ **draw calls de sprite**. Es el número
    /// que importa: ver `sprite_visible` sin esto no dice nada.
    pub sprite_batches: u32,
    /// Texturas distintas entre los visibles = el piso al que podrían bajar los
    /// lotes si estuvieran agrupados por textura (o 1, con atlas común).
    pub sprite_textures: u32,
    /// De los visibles, cuántos son **arte de tile** (`world2d::TileArt`): el
    /// suelo dibujado como sprite ENCIMA de la malla de color que ya lo pintó.
    /// Separarlo importa porque es la capa duplicada, no decoración de verdad.
    pub sprite_tileart: u32,
}

#[derive(Resource, Default)]
pub struct PerfStats {
    pub fps: f32,
    pub frame_ms: f32,
    pub frame_jitter_ms: f32,
    pub draw_calls: u32,
    pub visible_meshes: u32,
    pub active_entities: u32,
    pub breakdown: PerfRenderBreakdown,
    pub window_w: u32,
    pub window_h: u32,
    last_frame_ms: f32,
}

#[derive(Clone, Debug, Default, serde::Serialize)]
pub struct PerfSampleLine {
    pub fps: f32,
    pub frame_ms: f32,
    pub draw3d: u32,
    pub ui: u32,
    pub ents: u32,
}

#[derive(Resource, Default, Clone)]
pub struct PerfRingBuffer {
    pub samples: VecDeque<PerfSampleLine>,
}

impl PerfRingBuffer {
    pub const CAP: usize = 5;

    pub fn push(&mut self, sample: PerfSampleLine) {
        if self.samples.len() >= Self::CAP {
            self.samples.pop_front();
        }
        self.samples.push_back(sample);
    }
}

/// Umbral por defecto del log de tirones. **En el teléfono va encendido**: es el
/// único sitio donde el perf importa de verdad (regla 2) y no hay forma de
/// pasarle variables de entorno a un APK. En escritorio queda apagado salvo que
/// pidas `HA_HITCH=ms`, porque acá el fps no significa nada
/// (940M + NVK: frames de 170 ms con 0,35 ms de CPU).
#[cfg(target_os = "android")]
const HITCH_DEFAULT_MS: f32 = 50.0;
#[cfg(not(target_os = "android"))]
const HITCH_DEFAULT_MS: f32 = f32::INFINITY;

/// Tope de líneas por segundo. A 15 fps TODOS los frames pasan el umbral: sin
/// tope el log se convierte en un chorro que además cuesta lo suyo escribir, y
/// terminás midiendo el logger.
const HITCH_MAX_PER_SEC: u32 = 8;

/// **Los primeros segundos de juego se loguean enteros, frame por frame.** El
/// bajón que se reporta ("después de la pantalla de carga va a 4 fps y después
/// levanta") vive exactamente acá y no lo ve ninguna de las otras dos sondas: el
/// log periódico promedia 5 s y `ha-span` promedia 60 frames — a 4 fps eso son
/// 15 s, o sea que el bajón entero cae DENTRO de una sola muestra promediada.
/// Con el mundo ya armado detrás de la cortina, cada frame de estos debería
/// costar lo mismo que el resto de la partida; el que no, dice quién es.
const STARTUP_PROBE_SECS: f32 = 12.0;

/// `HA_HITCH=ms` → una línea por cada frame que pase de ese umbral, con el
/// segundo (contado desde que se levantó la cortina de carga) y el desglose de
/// CPU del frame. El log periódico promedia cada 5 s, así que un tirón de 200 ms
/// se le desaparece adentro; para cazar "los lagazos al empezar a jugar" hace
/// falta ver el frame suelto.
///
/// **Los spans van en DELTA de este frame, no acumulados.** `CpuSpanStats` se
/// reinicia cada 60 frames, así que leerlo crudo mezcla este frame con los N
/// anteriores: durante un bajón (donde 60 frames son 15 s) el número que salía
/// era la suma de medio minuto y no señalaba a nadie. Restando la lectura
/// anterior, la línea dice qué costó ESTE frame.
fn log_hitches(
    time: Res<Time<bevy::time::Real>>,
    phase: Res<State<crate::game_phase::GamePhase>>,
    span: Res<CpuSpanStats>,
    stats: Res<PerfStats>,
    mut playing_t: Local<f32>,
    mut threshold: Local<Option<f32>>,
    mut budget: Local<(f32, u32)>,
    mut last: Local<[u64; 5]>,
) {
    let thr = *threshold.get_or_insert_with(|| {
        crate::flags::var("HA_HITCH")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(HITCH_DEFAULT_MS)
    });
    if *phase.get() != crate::game_phase::GamePhase::Playing {
        return;
    }
    let dt_ms = time.delta_secs() * 1000.0;
    *playing_t += time.delta_secs();

    // Delta por frame de cada span. El acumulador se reinicia cada 60 frames: al
    // hacerlo, la resta daría negativo — ahí se toma el valor crudo.
    let now = [
        span.us(CpuSpanSlot::Chunk),
        span.us(CpuSpanSlot::Terrain),
        span.us(CpuSpanSlot::Lights),
        span.us(CpuSpanSlot::Atmo),
        span.us(CpuSpanSlot::Particles),
    ];
    // Primer frame jugable: sólo se anota la marca. Restar contra cero le
    // atribuiría a este frame TODO lo que el acumulador junto detrás de la
    // cortina (medido: 4136 ms de `chunk` en un frame de 996 ms — un número
    // imposible que apuntaba al sistema equivocado).
    let first = *playing_t <= time.delta_secs() * 1.5;
    let mut d = [0.0f32; 5];
    for i in 0..5 {
        d[i] = now[i].saturating_sub(last[i]) as f32 / 1000.0;
    }
    *last = now;
    if first {
        return;
    }

    // Ventana de arranque: todo frame se loguea. Después, sólo los tirones.
    let startup = *playing_t <= STARTUP_PROBE_SECS;
    if !startup && (!thr.is_finite() || dt_ms < thr) {
        return;
    }
    // Ventana de 1 s para el tope de líneas.
    if *playing_t - budget.0 >= 1.0 {
        *budget = (*playing_t, 0);
    }
    budget.1 += 1;
    // En el arranque el tope sube: con 8 por segundo, en un tramo que corre a 60
    // fps se loguean los primeros 8 frames y se pierde justo el pico del medio.
    if budget.1 > if startup { 30 } else { HITCH_MAX_PER_SEC } {
        return;
    }
    eprintln!(
        "ha-hitch: t={:.2}s frame={dt_ms:.1}ms chunk={:.1} terrain={:.1} lights={:.1} atmo={:.1} part={:.1} ents={} draws={}",
        *playing_t, d[0], d[1], d[2], d[3], d[4], stats.active_entities, stats.draw_calls,
    );
}

#[derive(Component)]
struct PerfOverlayRoot;

#[derive(Component)]
struct PerfOverlayText;

fn spawn_perf_overlay(mut commands: Commands) {
    commands
        .spawn((
            Node {
                position_type: PositionType::Absolute,
                left: Val::Px(8.0),
                bottom: Val::Px(88.0),
                display: Display::None,
                padding: UiRect::all(Val::Px(6.0)),
                border: UiRect::all(Val::Px(2.0)),
                ..default()
            },
            BorderColor(ui_theme::button_border(ui_theme::UiButtonTone::Primary)),
            BackgroundColor(ui_theme::panel_bg(0.72)),
            PerfOverlayRoot,
            Name::new("PerfOverlay"),
        ))
        .with_children(|panel| {
            panel.spawn((
                Text::new("PERF"),
                TextFont {
                    font_size: 11.0,
                    ..default()
                },
                TextColor(ui_theme::text_primary()),
                PerfOverlayText,
            ));
        });
}

/// Desktop: F3. Android/touch: MENU → PERF.
fn toggle_perf_overlay(
    keys: Res<ButtonInput<KeyCode>>,
    mut visible: ResMut<PerfOverlayVisible>,
    mut root: Query<&mut Node, With<PerfOverlayRoot>>,
) {
    if keys.just_pressed(KeyCode::F3) {
        visible.0 = !visible.0;
    }
    for mut node in &mut root {
        node.display = if visible.0 {
            Display::Flex
        } else {
            Display::None
        };
    }
}

/// Cheap every frame — no entity scans.
///
/// **`Time<Real>`, no `Time` a secas.** El reloj virtual —el que sirve para la
/// lógica de juego— RECORTA el delta a 250 ms
/// (`Time::<Virtual>::DEFAULT_MAX_DELTA`) para que un tirón no teletransporte al
/// jugador. Perfecto para simular, veneno para medir: con el reloj virtual este
/// contador no podía reportar nunca menos de 4 FPS ni un `frame_ms` mayor a 250,
/// así que en cuanto el frame se iba de verdad al carajo el overlay se clavaba en
/// "4 fps / 250 ms / jitter 0" y ocultaba CUÁNTO peor estaba — justo el momento
/// en que uno mira el número. `Time<Real>` es el reloj de pared, sin recortes.
fn sample_frame_timing(
    time: Res<Time<Real>>,
    window: Query<&Window, With<PrimaryWindow>>,
    mut stats: ResMut<PerfStats>,
) {
    let dt_ms = time.delta_secs() * 1000.0;
    stats.frame_ms = dt_ms;
    stats.frame_jitter_ms = (dt_ms - stats.last_frame_ms).abs();
    stats.last_frame_ms = dt_ms;
    if dt_ms > 0.0 {
        stats.fps = 1000.0 / dt_ms;
    }
    if let Ok(win) = window.get_single() {
        stats.window_w = win.physical_width().max(1);
        stats.window_h = win.physical_height().max(1);
    }
}

/// Expensive mesh/UI/entity scan — ha-perf log every 5s; overlay refresh every 0.5s (not every frame).
fn sample_perf_breakdown(
    time: Res<Time>,
    visible: Res<PerfOverlayVisible>,
    mut timer: ResMut<PerfBreakdownTimer>,
    mut stats: ResMut<PerfStats>,
    meshes: Query<(
        &Mesh2d,
        Option<&Visibility>,
        Option<&Name>,
        Option<&crate::world2d::TileChunk>,
        Option<&InteractTarget>,
    )>,
    sprites: Query<(&Sprite, &GlobalTransform, &ViewVisibility, Option<&crate::world2d::TileArt>)>,
    ui_nodes: Query<&Node>,
    all_entities: Query<Entity>,
) {
    timer.elapsed += time.delta_secs();
    timer.overlay_elapsed += time.delta_secs();
    let due_log = timer.elapsed >= 5.0;
    let due_overlay = visible.0 && timer.overlay_elapsed >= 0.5;
    if !due_log && !due_overlay {
        return;
    }
    if due_log {
        timer.elapsed = 0.0;
    }
    if due_overlay {
        timer.overlay_elapsed = 0.0;
    }

    let mut breakdown = PerfRenderBreakdown::default();
    let mut visible_count = 0u32;
    for (_, vis, _name, chunk, interactable) in &meshes {
        if vis.map(|v| *v != Visibility::Hidden).unwrap_or(true) {
            visible_count += 1;
            if chunk.is_some() {
                breakdown.island_ground += 1;
            } else if interactable.is_some() {
                breakdown.interactables += 1;
            } else {
                breakdown.other_3d += 1;
            }
        }
    }
    breakdown.chunk_decor = sprites.iter().count() as u32;
    breakdown.ui_nodes = ui_nodes.iter().count() as u32;
    stats.visible_meshes = visible_count;
    stats.draw_calls = visible_count;
    stats.active_entities = all_entities.iter().count() as u32;

    // --- ROTURAS DE LOTE: la métrica que faltaba ---
    //
    // `draw_calls` de arriba cuenta MALLAS (el suelo), no sprites. Los miles de
    // sprites de decoración se dibujaban sin que nada midiera cuántos lotes
    // salían, y ahí está el gasto: Bevy 2D agrupa sprites que comparten textura
    // **y son contiguos en el orden de dibujo**. Con orden Y y una textura
    // distinta por slot, cada vecino en Y rompe el lote y cada sprite termina
    // siendo su propio draw call.
    //
    // Se mide igual que dibuja el motor: ordenar los sprites VISIBLES por z y
    // contar cuántas veces cambia la textura. `lotes` ≈ draw calls de sprite;
    // `texturas` es el piso teórico (lo que darías si estuvieran agrupados).
    // A 0,2 Hz — no está en el camino del frame.
    if due_log {
        let mut vis: Vec<(f32, u128)> = Vec::new();
        let mut tileart = 0u32;
        for (s, gt, v, ta) in &sprites {
            if !v.get() {
                continue;
            }
            if ta.is_some() {
                tileart += 1;
            }
            vis.push((gt.translation().z, image_id(&s.image)));
        }
        vis.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));
        let mut batches = 0u32;
        let mut last = u128::MAX;
        for (_, id) in &vis {
            if *id != last {
                batches += 1;
                last = *id;
            }
        }
        let uniq: std::collections::HashSet<u128> = vis.iter().map(|(_, id)| *id).collect();
        breakdown.sprite_visible = vis.len() as u32;
        breakdown.sprite_batches = batches;
        breakdown.sprite_textures = uniq.len() as u32;
        breakdown.sprite_tileart = tileart;
    } else {
        breakdown.sprite_visible = stats.breakdown.sprite_visible;
        breakdown.sprite_batches = stats.breakdown.sprite_batches;
        breakdown.sprite_textures = stats.breakdown.sprite_textures;
        breakdown.sprite_tileart = stats.breakdown.sprite_tileart;
    }
    stats.breakdown = breakdown;
}

/// Identidad estable de la textura de un sprite, para contar roturas de lote.
fn image_id(h: &Handle<Image>) -> u128 {
    match h.id() {
        AssetId::Index { index, .. } => index.to_bits() as u128,
        AssetId::Uuid { uuid } => uuid.as_u128(),
    }
}

fn update_perf_overlay(
    visible: Res<PerfOverlayVisible>,
    stats: Res<PerfStats>,
    clear: Res<ClearColor>,
    rs: Res<crate::render::scale::RenderScaleState>,
    game_cam: Query<&GlobalTransform, With<GameCamera>>,
    mut text: Query<&mut Text, With<PerfOverlayText>>,
    time: Res<Time<Real>>,
    mut acc: Local<f32>,
) {
    if !visible.0 {
        return;
    }
    // **El medidor se estaba midiendo a sí mismo.** Esto reescribía el texto del
    // overlay CADA FRAME, y cambiar un `Text` obliga a re-medir la tipografía y a
    // relayoutear el árbol de UI entero (~120 nodos). O sea: tener el overlay
    // abierto —justo lo que uno hace para diagnosticar un bajón— empeoraba el
    // bajón, y el número que leías incluía ese costo. A 4 Hz sigue siendo
    // perfectamente legible y deja de contaminar la medición.
    *acc += time.delta_secs();
    if *acc < 0.25 {
        return;
    }
    *acc = 0.0;
    if let Ok(mut t) = text.get_single_mut() {
        let b = stats.breakdown;
        let cc = clear.0.to_srgba();
        let cam_pos = game_cam
            .get_single()
            .map(|tf| tf.translation())
            .unwrap_or(Vec3::ZERO);
        // `blit` es lo primero que hay que mirar si el mundo se ve negro: dice si
        // la escala de render está desviando la escena a una imagen y de qué
        // tamaño. `blit off` = la cámara dibuja directo a la ventana.
        let blit = if rs.scale >= 0.995 {
            "off".to_string()
        } else {
            format!("{:.2} {}x{}", rs.scale, rs.size.x, rs.size.y)
        };
        t.0 = format!(
            "FPS {:.0}  {:.1}ms  jitter {:.1}ms  {}x{}\n\
             blit {blit}\n\
             cam pos=({:.1},{:.1})\n\
             clear=({:.2},{:.2},{:.2})\n\
             3D~ {}  ui {}  ents {}\n\
             ocean {} ground {} static {} chunk {} obs {} nodes {} out {} other {}",
            stats.fps,
            stats.frame_ms,
            stats.frame_jitter_ms,
            stats.window_w,
            stats.window_h,
            cam_pos.x,
            cam_pos.y,
            cc.red,
            cc.green,
            cc.blue,
            stats.draw_calls,
            b.ui_nodes,
            stats.active_entities,
            b.ocean,
            b.island_ground,
            b.static_world,
            b.chunk_decor,
            b.obstacles,
            b.interactables,
            b.outline,
            b.other_3d,
        );
    }
}

fn log_perf_periodic(
    time: Res<Time>,
    stats: Res<PerfStats>,
    span: Res<CpuSpanStats>,
    region: Res<crate::scene::RegionRuntime>,
    world_ctx: Res<crate::scene::WorldContextRuntime>,
    flight: Res<crate::flight::Flight>,
    mut ring: ResMut<PerfRingBuffer>,
    mut log_timer: Local<f32>,
) {
    *log_timer += time.delta_secs();
    if *log_timer < 5.0 {
        return;
    }
    *log_timer = 0.0;
    // Breakdown refreshed by sample_perf_breakdown on same 5s cadence.
    let b = stats.breakdown;
    let sf = span.frames.max(1) as u64;
    ring.push(PerfSampleLine {
        fps: stats.fps,
        frame_ms: stats.frame_ms,
        draw3d: stats.draw_calls,
        ui: b.ui_nodes,
        ents: stats.active_entities,
    });
    eprintln!(
        "ha-perf fps={:.0} frame_ms={:.1} jitter={:.1} res={}x{} \
         region={:?} ctx={:?} flight={} \
         draw3d~={} ui={} ents={} \
         spr[vis={} lotes={} tex={} tileart={}] \
         ocean={} ground={} chunk={} obs={} nodes={} outline={} other3d={} \
         span[cam={} chk={} tgt={} hud={} ptc={} atm={} lit={}]us",
        stats.fps,
        stats.frame_ms,
        stats.frame_jitter_ms,
        stats.window_w,
        stats.window_h,
        region.active,
        world_ctx.active,
        flight.active,
        stats.draw_calls,
        b.ui_nodes,
        stats.active_entities,
        b.sprite_visible,
        b.sprite_batches,
        b.sprite_textures,
        b.sprite_tileart,
        b.ocean,
        b.island_ground,
        b.chunk_decor,
        b.obstacles,
        b.interactables,
        b.outline,
        b.other_3d,
        span.us(CpuSpanSlot::Camera) / sf,
        span.us(CpuSpanSlot::Chunk) / sf,
        span.us(CpuSpanSlot::Target) / sf,
        span.us(CpuSpanSlot::Hud) / sf,
        span.us(CpuSpanSlot::Particles) / sf,
        span.us(CpuSpanSlot::Atmo) / sf,
        span.us(CpuSpanSlot::Lights) / sf,
    );
}

/// Cap target frame rate to display refresh when available.
pub fn apply_android_frame_cap(mut window: Query<&mut Window, With<PrimaryWindow>>) {
    let Ok(mut win) = window.get_single_mut() else {
        return;
    };
    #[cfg(target_os = "android")]
    {
        win.present_mode = bevy::window::PresentMode::Fifo;
    }
    #[cfg(not(target_os = "android"))]
    {
        win.present_mode = bevy::window::PresentMode::AutoVsync;
    }
}

/// **Censo de entidades por arquetipo.** `HA_CENSO=1` → cada 10 s, los 20
/// arquetipos más poblados con sus componentes propios del juego. El `ents=` del
/// log dice CUÁNTAS hay; esto dice QUÉ son, que es lo que hace falta para bajar
/// el número sin adivinar.
pub fn censo_de_entidades(world: &mut World, mut t: Local<f32>) {
    if crate::flags::var("HA_CENSO").is_err() {
        return;
    }
    *t += world.resource::<Time<bevy::time::Real>>().delta_secs();
    if *t < 10.0 {
        return;
    }
    *t = 0.0;
    let comps = world.components();
    let mut v: Vec<(usize, String)> = world
        .archetypes()
        .iter()
        .filter(|a| !a.is_empty())
        .map(|a| {
            let nombres: Vec<String> = a
                .components()
                .filter_map(|c| comps.get_name(c))
                .filter(|n| n.starts_with("ha_game") || n.contains("Sprite") || n.contains("Parent"))
                .map(|n| n.rsplit("::").next().unwrap_or(n).to_string())
                .collect();
            (a.len(), nombres.join("+"))
        })
        .collect();
    v.sort_by(|a, b| b.0.cmp(&a.0));
    let total: usize = v.iter().map(|x| x.0).sum();
    eprintln!("ha-censo total={total}");
    for (n, k) in v.iter().take(20) {
        eprintln!("ha-censo   {n:5} {k}");
    }
}
