//! Flujo de arranque: **splash → menú principal**. Maneja `GamePhase`:
//! `Splash` (logo + fondo animado, auto-avanza) → `MainMenu` (botones REALES:
//! Nueva partida, Cargar partida, Ajustes, Salir). "Nueva" pasa a
//! `CharacterCreate` (ver `character_create.rs`); recién al JUGAR se entra a
//! `Playing` y se genera el mundo.
//!
//! Fondo: secuencia de imágenes (`assets/art/ui/menu_bg/*.png`) reproducida como
//! un video de fondo — editable en el editor de assets. Sin frames, cae al fondo
//! neón sólido. Texto ASCII (la fuente del juego no dibuja acentos/flechas).

use bevy::app::AppExit;
use bevy::prelude::*;
use bevy::render::view::screenshot::{save_to_disk, Screenshot};
use bevy::ui::FocusPolicy;
use bevy::window::{MonitorSelection, PrimaryWindow, WindowMode};

use crate::game_phase::{has_savegame, load_savegame, GamePhase, SelectedCharacter};
use crate::input::TouchLayout;

const SPLASH_SECS: f32 = 1.8;
const BG_FPS: f32 = 8.0;
const NEON_CYAN: Color = Color::srgb(0.16, 0.94, 1.0);
const NEON_MAGENTA: Color = Color::srgb(1.0, 0.18, 0.62);
const VOID: Color = Color::srgb(0.04, 0.05, 0.09);
const BTN_IDLE: Color = Color::srgb(0.10, 0.14, 0.22);

pub struct BootUiPlugin;

impl Plugin for BootUiPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<MenuBg>()
            .init_resource::<SplashTimer>()
            .init_resource::<MenuShot>()
            .add_systems(Startup, load_menu_bg)
            .add_systems(OnEnter(GamePhase::Splash), spawn_splash)
            .add_systems(OnExit(GamePhase::Splash), despawn_boot)
            .add_systems(OnEnter(GamePhase::MainMenu), spawn_main_menu)
            .add_systems(OnExit(GamePhase::MainMenu), despawn_boot)
            .add_systems(Update, tick_splash.run_if(in_state(GamePhase::Splash)))
            .add_systems(
                Update,
                (menu_button_system, settings_button_system, menu_screenshot)
                    .run_if(in_state(GamePhase::MainMenu)),
            )
            .add_systems(Update, (animate_bg, boot_pulse))
            .add_systems(
                Update,
                (
                    void_clear_outside_playing,
                    crate::game_ui::hide_gameplay_ui_outside_playing,
                ),
            );
    }
}

// ── fondo animado ─────────────────────────────────────────────────────────────

#[derive(Resource, Default)]
struct MenuBg {
    frames: Vec<Handle<Image>>,
    idx: usize,
    t: f32,
}

#[derive(Resource, Default)]
struct SplashTimer(f32);

/// Carga los frames del fondo animado. En desktop lee `art/ui/menu_bg/` ordenado
/// por nombre; en Android (sin readdir del APK) queda vacío → fondo sólido.
#[cfg_attr(target_os = "android", allow(unused_mut, unused_variables))]
fn load_menu_bg(mut bg: ResMut<MenuBg>, asset_server: Res<AssetServer>) {
    #[cfg(not(target_os = "android"))]
    {
        let dir = concat!(env!("CARGO_MANIFEST_DIR"), "/assets/art/ui/menu_bg");
        if let Ok(rd) = std::fs::read_dir(dir) {
            let mut files: Vec<String> = rd
                .flatten()
                .filter_map(|e| {
                    let p = e.path();
                    (p.extension().and_then(|x| x.to_str()) == Some("png"))
                        .then(|| p.file_name().unwrap().to_string_lossy().to_string())
                })
                .collect();
            files.sort();
            bg.frames = files.iter().map(|f| asset_server.load(format!("art/ui/menu_bg/{f}"))).collect();
        }
    }
    let _ = &asset_server;
}

#[derive(Component)]
struct MenuBgImage;

/// Recorre los frames del fondo como un loop de video.
fn animate_bg(time: Res<Time<bevy::time::Real>>, mut bg: ResMut<MenuBg>, mut q: Query<&mut ImageNode, With<MenuBgImage>>) {
    if bg.frames.len() < 2 {
        return;
    }
    bg.t += time.delta_secs() * BG_FPS;
    if bg.t >= 1.0 {
        let steps = bg.t.floor() as usize;
        bg.idx = (bg.idx + steps) % bg.frames.len();
        bg.t = bg.t.fract();
    }
    let frame = bg.frames[bg.idx].clone();
    for mut img in &mut q {
        img.image = frame.clone();
    }
}

// ── marcadores ─────────────────────────────────────────────────────────────

#[derive(Component)]
struct BootRoot;
#[derive(Component)]
struct BootPulse;
#[derive(Component, Clone, Copy)]
enum MenuAction {
    NewGame,
    Load,
    Settings,
    Quit,
}
#[derive(Component)]
struct SettingsPanel;
#[derive(Component, Clone, Copy)]
enum SettingsAction {
    ToggleFullscreen,
    Close,
}

fn despawn_boot(mut commands: Commands, q: Query<Entity, With<BootRoot>>) {
    for e in &q {
        commands.entity(e).despawn_recursive();
    }
}

/// Factor de escala del menú según el alto REAL de la ventana. En el teléfono el
/// alto lógico puede ser ~400 px (scale factor alto): con tamaños fijos el título
/// + los 4 botones medían más que la pantalla y, al estar centrados, se salían por
/// arriba y por abajo. Todo el menú se dibuja multiplicado por esto.
fn ui_scale(touch: &TouchLayout) -> f32 {
    (touch.height.max(240.0) / 720.0).clamp(0.45, 1.15)
}

/// Nodo raíz full-screen (con fondo animado detrás) compartido por splash y menú.
/// El contenido se centra DENTRO del área segura (insets del notch/barras).
fn root_with_bg<'a>(commands: &'a mut Commands, bg: &MenuBg, touch: &TouchLayout, s: f32) -> EntityCommands<'a> {
    let mut ec = commands.spawn((
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
            padding: UiRect {
                top: Val::Px(touch.insets.top as f32 + 8.0),
                bottom: Val::Px(touch.insets.bottom as f32 + 8.0),
                left: Val::Px(touch.insets.left as f32 + 12.0),
                right: Val::Px(touch.insets.right as f32 + 12.0),
            },
            ..default()
        },
        BackgroundColor(VOID),
        GlobalZIndex(1000),
        FocusPolicy::Block,
        BootRoot,
        Name::new("BootOverlay"),
    ));
    if !bg.frames.is_empty() {
        ec.with_children(|root| {
            root.spawn((
                ImageNode::new(bg.frames[0].clone()),
                Node {
                    position_type: PositionType::Absolute,
                    left: Val::Px(0.0),
                    top: Val::Px(0.0),
                    width: Val::Percent(100.0),
                    height: Val::Percent(100.0),
                    ..default()
                },
                MenuBgImage,
            ));
        });
    }
    ec
}

fn title(root: &mut ChildBuilder, s: f32) {
    root.spawn((
        Text::new("KI"),
        TextFont { font_size: (58.0 * s).clamp(30.0, 66.0), ..default() },
        TextColor(NEON_CYAN),
    ));
    root.spawn((
        Text::new("MUNDO ABIERTO"),
        TextFont { font_size: (18.0 * s).clamp(12.0, 20.0), ..default() },
        TextColor(NEON_MAGENTA),
    ));
    root.spawn((
        Node {
            width: Val::Px(220.0 * s),
            height: Val::Px(4.0),
            margin: UiRect::top(Val::Px(6.0 * s)),
            ..default()
        },
        BackgroundColor(NEON_MAGENTA),
        BootPulse,
    ));
}

fn spawn_splash(
    mut commands: Commands,
    bg: Res<MenuBg>,
    touch: Res<TouchLayout>,
    mut timer: ResMut<SplashTimer>,
) {
    timer.0 = 0.0;
    let s = ui_scale(&touch);
    root_with_bg(&mut commands, &bg, &touch, s).with_children(|root| {
        title(root, s);
        root.spawn((
            Text::new("cargando..."),
            TextFont { font_size: (13.0 * s).clamp(10.0, 15.0), ..default() },
            TextColor(Color::srgb(0.7, 0.75, 0.85)),
        ));
    });
}

fn tick_splash(
    time: Res<Time<bevy::time::Real>>,
    mut timer: ResMut<SplashTimer>,
    mut next: ResMut<NextState<GamePhase>>,
) {
    // CI/agente: HA_AUTOPLAY salta el menú y arranca el gameplay directo (por
    // la pantalla de carga, igual que un jugador: así los drivers de QA miden un
    // mundo YA armado y no el suelo apareciendo a pedazos).
    if std::env::var("HA_AUTOPLAY").is_ok() {
        next.set(GamePhase::Loading);
        return;
    }
    timer.0 += time.delta_secs();
    if timer.0 >= SPLASH_SECS {
        next.set(GamePhase::MainMenu);
    }
}

fn spawn_main_menu(mut commands: Commands, bg: Res<MenuBg>, touch: Res<TouchLayout>) {
    let load_label = if has_savegame() { "CARGAR PARTIDA" } else { "CARGAR (vacio)" };
    let s = ui_scale(&touch);
    // Ancho de botón acotado también por el ancho útil: en pantallas angostas
    // (o con notch lateral) 260 px fijos se comían el margen.
    let usable_w = (touch.width - touch.insets.left as f32 - touch.insets.right as f32 - 32.0).max(160.0);
    let btn_w = (260.0 * s).clamp(150.0, 300.0).min(usable_w);
    root_with_bg(&mut commands, &bg, &touch, s).with_children(|root| {
        title(root, s);
        root.spawn((
            Node {
                margin: UiRect::top(Val::Px(20.0 * s)),
                flex_direction: FlexDirection::Column,
                align_items: AlignItems::Center,
                row_gap: Val::Px(10.0 * s),
                ..default()
            },
            Name::new("MainMenuButtons"),
        ))
        .with_children(|col| {
            menu_button(col, "NUEVA PARTIDA", MenuAction::NewGame, NEON_CYAN, btn_w, s);
            menu_button(col, load_label, MenuAction::Load, NEON_CYAN, btn_w, s);
            menu_button(col, "AJUSTES", MenuAction::Settings, NEON_CYAN, btn_w, s);
            menu_button(col, "SALIR", MenuAction::Quit, NEON_MAGENTA, btn_w, s);
        });
        // Si la partida anterior se cerró sola, acá se lee POR QUÉ. En Android el
        // hook de pánico hace `process::exit(1)`: se cierra sin diálogo y sin
        // rastro visible, y con el APK por URL nadie tiene un cable a mano para
        // mirar `adb logcat -s ha-crash`.
        if let Some(line) = crate::debug::last_crash_line() {
            eprintln!("ha-crash previo: {line}");
            root.spawn((
                Text::new(line),
                TextFont { font_size: (11.0 * s).clamp(9.0, 13.0), ..default() },
                TextColor(NEON_MAGENTA),
                Node {
                    margin: UiRect::top(Val::Px(14.0 * s)),
                    max_width: Val::Percent(90.0),
                    ..default()
                },
                Name::new("LastCrashLine"),
            ));
            crate::debug::clear_last_crash();
        }
    })
    // Panel de ajustes: overlay CENTRADO propio (oculto hasta tocar AJUSTES).
    // Antes era un hermano más de la columna: al abrirlo empujaba los botones
    // fuera de la pantalla en teléfonos bajos.
    .with_children(|root| {
        root.spawn((
            Node {
                position_type: PositionType::Absolute,
                left: Val::Px(0.0),
                top: Val::Px(0.0),
                width: Val::Percent(100.0),
                height: Val::Percent(100.0),
                flex_direction: FlexDirection::Column,
                justify_content: JustifyContent::Center,
                align_items: AlignItems::Center,
                display: Display::None,
                ..default()
            },
            BackgroundColor(Color::srgba(0.02, 0.03, 0.06, 0.88)),
            FocusPolicy::Block,
            SettingsPanel,
        ))
        .with_children(|wrap| {
            wrap.spawn((
                Node {
                    flex_direction: FlexDirection::Column,
                    align_items: AlignItems::Center,
                    row_gap: Val::Px(10.0 * s),
                    padding: UiRect::all(Val::Px(14.0 * s)),
                    border: UiRect::all(Val::Px(2.0)),
                    ..default()
                },
                BorderColor(NEON_CYAN),
                BackgroundColor(Color::srgb(0.06, 0.08, 0.12)),
            ))
            .with_children(|panel| {
                panel.spawn((
                    Text::new("AJUSTES"),
                    TextFont { font_size: (20.0 * s).clamp(14.0, 22.0), ..default() },
                    TextColor(NEON_CYAN),
                ));
                settings_button(panel, "PANTALLA COMPLETA", SettingsAction::ToggleFullscreen, s);
                settings_button(panel, "VOLVER", SettingsAction::Close, s);
            });
        });
    });
}

fn menu_button(parent: &mut ChildBuilder, label: &str, action: MenuAction, color: Color, w: f32, s: f32) {
    parent
        .spawn((
            Button,
            Node {
                width: Val::Px(w),
                height: Val::Px((56.0 * s).clamp(38.0, 60.0)),
                border: UiRect::all(Val::Px(3.0)),
                justify_content: JustifyContent::Center,
                align_items: AlignItems::Center,
                ..default()
            },
            BorderColor(color),
            BackgroundColor(BTN_IDLE),
            BootPulse,
            action,
            Name::new(format!("MenuBtn_{label}")),
        ))
        .with_children(|b| {
            b.spawn((
                Text::new(label),
                TextFont { font_size: (24.0 * s).clamp(15.0, 26.0), ..default() },
                TextColor(color),
            ));
        });
}

fn settings_button(parent: &mut ChildBuilder, label: &str, action: SettingsAction, s: f32) {
    parent
        .spawn((
            Button,
            Node {
                width: Val::Px((240.0 * s).clamp(150.0, 260.0)),
                height: Val::Px((42.0 * s).clamp(34.0, 46.0)),
                border: UiRect::all(Val::Px(2.0)),
                justify_content: JustifyContent::Center,
                align_items: AlignItems::Center,
                ..default()
            },
            BorderColor(NEON_CYAN),
            BackgroundColor(BTN_IDLE),
            action,
        ))
        .with_children(|b| {
            b.spawn((
                Text::new(label),
                TextFont { font_size: (16.0 * s).clamp(12.0, 18.0), ..default() },
                TextColor(NEON_CYAN),
            ));
        });
}

#[allow(clippy::type_complexity)]
fn menu_button_system(
    mut interactions: Query<(&Interaction, &MenuAction, &mut BackgroundColor), Changed<Interaction>>,
    mut next: ResMut<NextState<GamePhase>>,
    mut selected: ResMut<SelectedCharacter>,
    mut exit: EventWriter<AppExit>,
    mut panels: Query<&mut Node, With<SettingsPanel>>,
) {
    for (interaction, action, mut bg) in &mut interactions {
        match interaction {
            Interaction::Pressed => {
                *bg = BackgroundColor(Color::srgb(0.16, 0.24, 0.36));
                match action {
                    MenuAction::NewGame => next.set(GamePhase::CharacterCreate),
                    MenuAction::Load => {
                        if let Some(saved) = load_savegame() {
                            *selected = saved;
                            next.set(GamePhase::Loading);
                        } else {
                            // Sin partida guardada → arranca creación de personaje.
                            next.set(GamePhase::CharacterCreate);
                        }
                    }
                    MenuAction::Settings => {
                        if let Ok(mut node) = panels.get_single_mut() {
                            node.display = if node.display == Display::None {
                                Display::Flex
                            } else {
                                Display::None
                            };
                        }
                    }
                    MenuAction::Quit => {
                        exit.send(AppExit::Success);
                    }
                }
            }
            Interaction::Hovered => *bg = BackgroundColor(Color::srgb(0.14, 0.20, 0.30)),
            Interaction::None => *bg = BackgroundColor(BTN_IDLE),
        }
    }
}

#[allow(clippy::type_complexity)]
fn settings_button_system(
    mut interactions: Query<(&Interaction, &SettingsAction, &mut BackgroundColor), Changed<Interaction>>,
    mut panels: Query<&mut Node, With<SettingsPanel>>,
    mut windows: Query<&mut Window, With<PrimaryWindow>>,
) {
    for (interaction, action, mut bg) in &mut interactions {
        match interaction {
            Interaction::Pressed => {
                *bg = BackgroundColor(Color::srgb(0.16, 0.24, 0.36));
                match action {
                    SettingsAction::ToggleFullscreen => {
                        if let Ok(mut win) = windows.get_single_mut() {
                            win.mode = match win.mode {
                                WindowMode::Windowed => {
                                    WindowMode::BorderlessFullscreen(MonitorSelection::Primary)
                                }
                                _ => WindowMode::Windowed,
                            };
                        }
                    }
                    SettingsAction::Close => {
                        if let Ok(mut node) = panels.get_single_mut() {
                            node.display = Display::None;
                        }
                    }
                }
            }
            Interaction::Hovered => *bg = BackgroundColor(Color::srgb(0.14, 0.20, 0.30)),
            Interaction::None => *bg = BackgroundColor(BTN_IDLE),
        }
    }
}

/// QA de dev: si `HA_MENUSHOT=<ruta.png>` está seteado, captura el menú tras
/// unos frames (para que el fondo animado ya haya arrancado) y sale. Usa la
/// captura in-engine de Bevy → funciona sin herramienta externa del SO.
#[derive(Resource, Default)]
struct MenuShot {
    frames: u32,
    done: bool,
}

fn menu_screenshot(mut shot: ResMut<MenuShot>, mut commands: Commands, mut exit: EventWriter<AppExit>) {
    let Ok(path) = std::env::var("HA_MENUSHOT") else {
        return;
    };
    if shot.done {
        return;
    }
    shot.frames += 1;
    if shot.frames == 20 {
        commands.spawn(Screenshot::primary_window()).observe(save_to_disk(path));
        eprintln!("ha-menushot: capturando menu");
    }
    if shot.frames >= 45 {
        shot.done = true;
        exit.send(AppExit::Success);
    }
}

/// Fuera de `Playing`, el color de fondo del render es el void del menú y no el
/// cielo del mundo. Tercera capa del mismo arreglo: la cámara no mira la capa del
/// mundo (`camera::WORLD_LAYER`), la UI de gameplay está apagada
/// ([`crate::game_ui::GameplayUi`]) y lo único que queda debajo del menú es negro
/// — ni un frame de cielo azul asomando en el arranque.
fn void_clear_outside_playing(phase: Res<State<GamePhase>>, mut clear: ResMut<ClearColor>) {
    // En `Loading` el mundo ya existe y el cielo maneja el color: forzar el void
    // acá sería pelearse con `sky2d` todos los frames debajo de la cortina.
    if crate::game_phase::world_active(phase) || clear.0 == VOID {
        return;
    }
    clear.0 = VOID;
}

/// Latido de los acentos neón (subrayado + botones).
fn boot_pulse(
    time: Res<Time>,
    mut borders: Query<&mut BorderColor, With<BootPulse>>,
    mut bars: Query<&mut BackgroundColor, (With<BootPulse>, Without<Button>)>,
) {
    let t = time.elapsed_secs();
    let pulse = 0.55 + 0.45 * (t * 3.0).sin().abs();
    for mut border in &mut borders {
        *border = BorderColor(NEON_CYAN.with_alpha(pulse));
    }
    for mut bg in &mut bars {
        *bg = BackgroundColor(NEON_MAGENTA.with_alpha(0.5 + 0.5 * pulse));
    }
}
