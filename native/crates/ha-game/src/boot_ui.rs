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
use crate::marca::Paleta;
use ha_shared::Ps1Retro;
use bevy::render::view::screenshot::{save_to_disk, Screenshot};
use bevy::ui::FocusPolicy;
use bevy::window::{MonitorSelection, PrimaryWindow, WindowMode};

use crate::game_phase::{load_savegame, GamePhase, SelectedCharacter};
use crate::input::TouchLayout;

/// **Dura lo que dura el sonido de la marca.** El WAV de `audio/splash.wav` mide
/// 2,60 s medidos; cortar la splash antes deja el golpe de sonido colgado, que es
/// peor que no tenerlo. Los 0,25 de más son el aire para que el logo respire
/// antes de irse.
const SPLASH_SECS: f32 = 2.85;
const BG_FPS: f32 = 8.0;
const NEON_CYAN: Color = Color::srgb(Ps1Retro::TEXT[0], Ps1Retro::TEXT[1], Ps1Retro::TEXT[2]);
const NEON_MAGENTA: Color = Color::srgb(Ps1Retro::SELECT[0], Ps1Retro::SELECT[1], Ps1Retro::SELECT[2]);
const VOID: Color = Color::srgb(Ps1Retro::VOID[0], Ps1Retro::VOID[1], Ps1Retro::VOID[2]);
const BTN_IDLE: Color = Color::srgb(0.10, 0.14, 0.22);

pub struct BootUiPlugin;

impl Plugin for BootUiPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<MenuBg>()
            .init_resource::<SplashTimer>()
            .init_resource::<MenuShot>()
            .init_resource::<SonoSplash>()
            .init_resource::<RehacerMenu>()
            .add_systems(Startup, load_menu_bg)
            .add_systems(OnEnter(GamePhase::Splash), spawn_splash)
            .add_systems(OnExit(GamePhase::Splash), despawn_boot)
            .add_systems(OnEnter(GamePhase::MainMenu), spawn_main_menu)
            .add_systems(OnExit(GamePhase::MainMenu), despawn_boot)
            .add_systems(
                Update,
                (tick_splash, animar_splash, sonar_splash, splash_screenshot).run_if(in_state(GamePhase::Splash)),
            )
            .add_systems(
                Update,
                (menu_button_system, settings_button_system, rotular_idioma, menu_screenshot, rehacer_menu)
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
pub struct BootRoot;
#[derive(Component)]
struct BootPulse;
#[derive(Component, Clone, Copy)]
enum MenuAction {
    /// **Lo que se toca casi siempre**: entra a la última partida jugada. Por eso
    /// se lleva el ancho entero en el menú.
    Continuar,
    /// Abre la lista de mundos.
    Partidas,
    /// Abre la lista de personajes, que es OTRA lista: un personaje no pertenece
    /// a un mundo.
    Personajes,
    NewGame,
    Settings,
    Quit,
    /// El riel de la izquierda del mockup.
    Controles,
    Actualizar,
    Creditos,
    /// Partidas en red (LAN / online).
    Multijugador,
    /// Las flechas al lado del personaje: cambian cuál está elegido.
    PjAnterior,
    PjSiguiente,
}
#[derive(Component)]
struct SettingsPanel;

/// El texto de la versión, que cambia mientras se lo toca.
#[derive(Component)]
struct VersionLabel;
#[derive(Component, Clone, Copy, PartialEq)]
enum SettingsAction {
    ToggleFullscreen,
    /// Español ⇄ English (`crate::i18n`).
    Idioma,
    Close,
    /// **La versión, que es un botón sin parecerlo.** Siete toques destraban la
    /// consola (`crate::consola`). Es el gesto de las opciones de desarrollador
    /// de Android, así que no hay que explicárselo a nadie — y quien no lo
    /// conozca sólo ve el número de build, que es para lo que está ahí.
    Version,
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
    // **El fondo es PARALLAX, no un video de PNG.**
    //
    // Antes esto pegaba una secuencia de imágenes de pantalla completa
    // (`art/ui/menu_bg/*.png`) reproducida a 8 fps. En Android ni siquiera
    // existía —no hay readdir del APK— así que el menú quedaba en un color
    // sólido, que es exactamente como se veía en el celu. Y donde sí existía
    // costaba N texturas de pantalla completa residentes para algo que se mira
    // diez segundos. Tres nodos de color que derivan cuestan cero textura.
    let _ = bg;
    ec.with_children(|root| {
        crate::marca::fondo(root, crate::marca::Ambiente::Menu, s);
    });
    ec
}

/// El título del menú, con la paleta de la marca. El "KI" es del juego; el
/// renglón de abajo va en el cartel, que es la regla: **un rótulo nunca es texto
/// pelado**.
fn title(root: &mut ChildBuilder, s: f32) {
    use crate::marca::Paleta;
    root.spawn((
        Text::new("KI"),
        TextFont { font_size: (58.0 * s).clamp(30.0, 66.0), ..default() },
        TextColor(Paleta::TEXTO),
    ));
    crate::marca::cartel(root, "Mundo abierto", s);
    root.spawn((
        Node {
            width: Val::Px(220.0 * s),
            height: Val::Px(3.0),
            margin: UiRect::top(Val::Px(6.0 * s)),
            ..default()
        },
        BackgroundColor(Paleta::TURQUESA),
        BorderRadius::all(Val::Px(2.0)),
        BootPulse,
    ));
}

/// Una mancha de pintura de la splash.
#[derive(Component)]
struct Salpicon {
    /// Índice: decide cuándo entra (`i * 0,075` de `paso`).
    i: f32,
    /// Centro en coordenadas del mockup (2400 × 1080).
    centro: Vec2,
    /// Lado final del dibujo, en coordenadas del mockup.
    lado: f32,
    k: f32,
}

/// El texto de la marca, que entra cuando ya hay pintura debajo.
#[derive(Component)]
struct MarcaTexto {
    /// Valor de `paso` a partir del cual aparece.
    desde: f32,
    /// Cuánto crece la tipografía mientras entra (0 = no crece).
    crece: f32,
    base: f32,
    k: f32,
}

/// Marca al reproductor de sonido para no spawnearlo dos veces.
#[derive(Resource, Default)]
struct SonoSplash(bool);

/// **El reparto de manchas del mockup**, en su espacio de 400 × 180 (la splash
/// se dibuja en baja resolución y se amplía con vecino más cercano, que es lo que
/// le da el borde escalonado). `(cx, cy, r, color)`.
const MANCHAS: [(f32, f32, f32, Color); 8] = [
    (132.0, 76.0, 34.0, MORADO),
    (268.0, 118.0, 32.0, TURQUESA),
    (238.0, 54.0, 19.0, NARANJA),
    (152.0, 146.0, 20.0, FUCSIA),
    (316.0, 64.0, 14.0, MORADO_HI),
    (78.0, 132.0, 15.0, TURQUESA_HI),
    (200.0, 156.0, 11.0, NARANJA),
    (296.0, 26.0, 10.0, FUCSIA),
];

/// La paleta de FZ STUDIOS, tal cual el mockup.
const MORADO: Color = Color::srgb(0.486, 0.243, 0.839);
const MORADO_HI: Color = Color::srgb(0.635, 0.416, 0.941);
const TURQUESA: Color = Color::srgb(0.149, 0.878, 0.773);
const TURQUESA_HI: Color = Color::srgb(0.494, 0.965, 0.886);
const NARANJA: Color = Color::srgb(1.0, 0.541, 0.165);
const FUCSIA: Color = Color::srgb(0.941, 0.235, 0.627);
const NEGRO: Color = Color::srgb(0.071, 0.063, 0.102);
const CREMA: Color = Color::srgb(0.941, 0.957, 0.949);

/// Cuánto dura la parte animada, en segundos. Sale de atar la animación al
/// sonido: la mancha `i` aterriza en `0,288 + i · 0,135` s y el WAV pega su pop
/// en `0,06 + i · 0,135` — con el audio arrancando `0,228` s después del dibujo,
/// los ocho golpes caen exactamente sobre las ocho manchas.
const PASO_SEGS: f32 = 1.8;
/// Cuánto se atrasa el audio para que los pops caigan sobre las manchas.
const RETARDO_AUDIO: f32 = 0.228;

/// **La mancha, dibujada pixel a pixel.** Es el port de `mock_menu::salpicon`:
/// un núcleo deforme de once gotas, siete chorros que se afinan y un rocío de
/// gotitas. Va PLANA — un degradado la convierte en una pelota.
///
/// Se dibuja en una textura chica (el radio por ocho) y se amplía con vecino más
/// cercano, así el borde queda escalonado como el pixel del juego. Es la misma
/// razón por la que el mockup dibuja a un sexto y agranda.
fn pintar_mancha(r: f32, color: Color, semilla: u64) -> Image {
    use bevy::asset::RenderAssetUsages;
    use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};

    let lado = ((r * 8.0).ceil() as usize).max(8);
    let mut px = vec![0u8; lado * lado * 4];
    let c = color.to_srgba();
    let rgba = [
        (c.red * 255.0) as u8,
        (c.green * 255.0) as u8,
        (c.blue * 255.0) as u8,
        255u8,
    ];
    let centro = lado as f32 / 2.0;

    let mut estado = semilla.wrapping_mul(6364136223846793005).wrapping_add(1);
    let mut az = move || {
        estado = estado.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        ((estado >> 11) % 1_000_000) as f32 / 1_000_000.0
    };

    let mut gota = |px: &mut Vec<u8>, cx: f32, cy: f32, rr: f32| {
        let r2 = rr * rr;
        let x0 = ((cx - rr).floor().max(0.0)) as usize;
        let x1 = ((cx + rr).ceil().min(lado as f32 - 1.0)) as usize;
        let y0 = ((cy - rr).floor().max(0.0)) as usize;
        let y1 = ((cy + rr).ceil().min(lado as f32 - 1.0)) as usize;
        for y in y0..=y1 {
            for x in x0..=x1 {
                let dx = x as f32 + 0.5 - cx;
                let dy = y as f32 + 0.5 - cy;
                if dx * dx + dy * dy <= r2 {
                    let o = (y * lado + x) * 4;
                    px[o..o + 4].copy_from_slice(&rgba);
                }
            }
        }
    };

    const TAU: f32 = std::f32::consts::TAU;
    // El núcleo: once gotas desparramadas dentro del radio.
    for _ in 0..11 {
        let a = az() * TAU;
        let d = az() * r * 0.72;
        gota(&mut px, centro + d * a.cos(), centro + d * a.sin() * 0.8, r * (0.38 + az() * 0.34));
    }
    // Los chorros, que se afinan hacia la punta, con su gota al final.
    for _ in 0..7 {
        let a = az() * TAU;
        let largo = r * (1.3 + az() * 1.5);
        for i in 0..9 {
            let t = (i + 1) as f32 / 9.0;
            let rr = (r * 0.42 * (1.0 - t).powf(1.5)).max(0.6);
            gota(&mut px, centro + largo * t * a.cos(), centro + largo * t * a.sin() * 0.82, rr);
        }
        if az() < 0.7 {
            let gr = r * (0.10 + az() * 0.10);
            gota(
                &mut px,
                centro + (largo + gr * 2.4) * a.cos(),
                centro + (largo + gr * 2.4) * a.sin() * 0.82,
                gr,
            );
        }
    }
    // El rocío.
    for _ in 0..21 {
        let a = az() * TAU;
        let largo = r * (1.6 + az() * 2.0);
        let rr = (r * (0.04 + az() * 0.09)).max(0.6);
        gota(&mut px, centro + largo * a.cos(), centro + largo * a.sin() * 0.82, rr);
    }

    sombrear_mancha(&mut px, lado, rgba);

    Image::new(
        Extent3d { width: lado as u32, height: lado as u32, depth_or_array_layers: 1 },
        TextureDimension::D2,
        px,
        TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::RENDER_WORLD,
    )
}

/// **Volumen de pintura gorda, sin degradado** (2 oct 2026, el listón de la skill `pixel-art`): la mancha
/// sigue siendo plana por BANDAS —cara de luz, base y cara de sombra, como pinta LimeZu— pero ya no es un
/// recorte de un solo color: la luz entra desde arriba-izquierda, el canto de abajo-derecha se oscurece y
/// las manchas grandes llevan un brillo húmedo de dos píxeles. El volumen sale de la distancia al borde
/// (una cúpula) y de su gradiente, igual que `scripts/lib/volumen.py::esculpir`.
fn sombrear_mancha(px: &mut [u8], lado: usize, base: [u8; 4]) {
    let dentro = |px: &[u8], x: usize, y: usize| px[(y * lado + x) * 4 + 3] > 0;
    // Distancia al borde con chamfer de 8 vecinos (1 y 1,414): casi isótropa, así las facetas son redondas
    // y no rectángulos. Fuera = 0; el borde de la imagen cuenta como "fuera".
    let inf = 1.0e6f32;
    let mut dist = vec![0.0f32; lado * lado];
    for y in 0..lado {
        for x in 0..lado {
            if dentro(px, x, y) {
                dist[y * lado + x] = inf;
            }
        }
    }
    const D: f32 = std::f32::consts::SQRT_2;
    let get = |dist: &Vec<f32>, x: isize, y: isize| -> f32 {
        if x < 0 || y < 0 || x >= lado as isize || y >= lado as isize {
            0.0
        } else {
            dist[y as usize * lado + x as usize]
        }
    };
    for y in 0..lado as isize {
        for x in 0..lado as isize {
            let i = y as usize * lado + x as usize;
            if dist[i] == 0.0 {
                continue;
            }
            let m = (get(&dist, x - 1, y) + 1.0)
                .min(get(&dist, x, y - 1) + 1.0)
                .min(get(&dist, x - 1, y - 1) + D)
                .min(get(&dist, x + 1, y - 1) + D);
            dist[i] = dist[i].min(m);
        }
    }
    for y in (0..lado as isize).rev() {
        for x in (0..lado as isize).rev() {
            let i = y as usize * lado + x as usize;
            if dist[i] == 0.0 {
                continue;
            }
            let m = (get(&dist, x + 1, y) + 1.0)
                .min(get(&dist, x, y + 1) + 1.0)
                .min(get(&dist, x + 1, y + 1) + D)
                .min(get(&dist, x - 1, y + 1) + D);
            dist[i] = dist[i].min(m);
        }
    }
    let dmax = dist.iter().cloned().filter(|v| *v < inf).fold(1.0f32, f32::max);
    let at = |x: isize, y: isize| -> f32 {
        let (x, y) = (x.clamp(0, lado as isize - 1) as usize, y.clamp(0, lado as isize - 1) as usize);
        dist[y * lado + x].min(dmax)
    };
    let mult = |c: [u8; 4], k: f32| -> [u8; 4] {
        let f = |v: u8| ((v as f32 * k).clamp(0.0, 255.0)) as u8;
        [f(c[0]), f(c[1]), f(c[2]), c[3]]
    };
    let mut mejor_brillo: Option<(usize, usize, f32)> = None;
    let mut salida = px.to_vec();
    for y in 0..lado {
        for x in 0..lado {
            let o = (y * lado + x) * 4;
            if px[o + 3] == 0 {
                continue;
            }
            let (xi, yi) = (x as isize, y as isize);
            // gradiente sobre una ventana de 3 px: facetas anchas y suaves, no escalones de a un píxel
            let gx = at(xi + 3, yi) - at(xi - 3, yi);
            let gy = at(xi, yi + 3) - at(xi, yi - 3);
            // normal hacia afuera = -grad; luz desde arriba-izquierda
            let luz = 0.55 * gx + 0.75 * gy;
            let d = dist[y * lado + x];
            let c = if d <= 1.5 && luz < -0.6 {
                mult(base, 0.52) // el canto en sombra
            } else if luz > 2.4 {
                mult(base, 1.24) // la cara de luz
            } else if luz < -2.4 {
                mult(base, 0.74) // la cara de sombra
            } else {
                base
            };
            salida[o..o + 4].copy_from_slice(&c);
            // candidato a brillo húmedo: adentro, en el cuarto de arriba-izquierda de la cúpula
            let relieve = d / dmax;
            if relieve > 0.4 && luz > 1.2 && mejor_brillo.map(|m| relieve > m.2).unwrap_or(true) {
                mejor_brillo = Some((x, y, relieve));
            }
        }
    }
    // El brillo: sólo en las manchas con cuerpo (dmax grande), dos píxeles y un acompañante.
    if dmax >= 6.0 {
        if let Some((bx, by, _)) = mejor_brillo {
            for (dx, dy, k) in [(0usize, 0usize, 0.78f32), (1, 0, 0.78), (0, 1, 0.5)] {
                let (x, y) = (bx + dx, by + dy);
                if x < lado && y < lado {
                    let o = (y * lado + x) * 4;
                    if salida[o + 3] > 0 {
                        for ch in 0..3 {
                            salida[o + ch] = (salida[o + ch] as f32 + (255.0 - salida[o + ch] as f32) * k) as u8;
                        }
                    }
                }
            }
        }
    }
    px.copy_from_slice(&salida);
}

/// **La splash: FZ STUDIOS con manchas de pintura.**
///
/// Es la pantalla de la imagen de referencia, no una versión parecida: las ocho
/// manchas están en sus coordenadas, entran escalonadas con el mismo rebote
/// corto al aterrizar, y el logo aparece cuando ya hay pintura debajo. El sonido
/// —los ocho pops y el acorde final— es el WAV que ya viene en el APK, y arranca
/// atrasado [`RETARDO_AUDIO`] para que cada golpe caiga sobre su mancha.
fn spawn_splash(
    mut commands: Commands,
    touch: Res<TouchLayout>,
    mut timer: ResMut<SplashTimer>,
    mut sono: ResMut<SonoSplash>,
    mut images: ResMut<Assets<Image>>,
) {
    timer.0 = 0.0;
    sono.0 = false;
    let k = crate::ui_pack::k(&touch);

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
            BackgroundColor(NEGRO),
            GlobalZIndex(1000),
            FocusPolicy::Block,
            BootRoot,
            Name::new("Splash"),
        ))
        .with_children(|root| {
            root.spawn(crate::ui_pack::lienzo(k)).with_children(|l| {
                for (i, (cx, cy, r, color)) in MANCHAS.into_iter().enumerate() {
                    // El espacio del mockup es 400 × 180; la pantalla, 2400 × 1080.
                    let escala = 6.0;
                    let img = images.add(pintar_mancha(r, color, 20 + i as u64));
                    let lado_dibujo = (r * 8.0).ceil() * escala;
                    let centro = Vec2::new(cx * escala, cy * escala);
                    l.spawn((
                        crate::ui_pack::caja(centro.x, centro.y, 0.0, 0.0, k),
                        ImageNode::new(img),
                        Salpicon { i: i as f32, centro, lado: lado_dibujo, k },
                        Name::new("Mancha"),
                    ));
                }
                // El logo. La sombra va primero, corrida como en el mockup.
                // El logo EXTRUIDO: ocho capas de volumen que bajan del morado al negro, y la cara crema arriba.
                let mut capas: Vec<(f32, Color)> = (1..=12)
                    .rev()
                    .map(|n| (n as f32 * 1.9, Color::srgb((0.50 - 0.036 * n as f32).max(0.05), (0.27 - 0.02 * n as f32).max(0.03), (0.86 - 0.06 * n as f32).max(0.10))))
                    .collect();
                capas.push((0.0, CREMA));
                for (d, color) in capas {
                    l.spawn(Node {
                        position_type: PositionType::Absolute,
                        left: Val::Px(d * k),
                        top: Val::Px((534.0 - 120.0 + d) * k),
                        width: Val::Px(crate::ui_pack::ANCHO * k),
                        height: Val::Px(240.0 * k),
                        justify_content: JustifyContent::Center,
                        align_items: AlignItems::Center,
                        ..default()
                    })
                    .with_children(|t| {
                        t.spawn((
                            Text::new("FZ STUDIOS"),
                            TextFont { font_size: 120.0 * k, ..default() },
                            TextColor(color.with_alpha(0.0)),
                            TextLayout::new_with_no_wrap(),
                            MarcaTexto { desde: 0.55, crece: 84.0, base: 120.0, k },
                        ));
                    });
                }
                l.spawn(Node {
                    position_type: PositionType::Absolute,
                    left: Val::Px(0.0),
                    top: Val::Px((672.0 - 40.0) * k),
                    width: Val::Px(crate::ui_pack::ANCHO * k),
                    height: Val::Px(80.0 * k),
                    justify_content: JustifyContent::Center,
                    align_items: AlignItems::Center,
                    ..default()
                })
                .with_children(|t| {
                    t.spawn((
                        Text::new("presenta"),
                        TextFont { font_size: 50.0 * k, ..default() },
                        TextColor(TURQUESA_HI.with_alpha(0.0)),
                        TextLayout::new_with_no_wrap(),
                        MarcaTexto { desde: 0.85, crece: 0.0, base: 50.0, k },
                    ));
                });
            });
        });
}

/// Las manchas caen escalonadas con un rebote corto, y el logo entra al final.
///
/// La curva es la del mockup, literal: `crecer = k · (1,12 − 0,12 k)`, o sea que
/// pasa de largo un 12 % y se asienta. Sin ese sobrepaso la mancha se infla como
/// un globo.
fn animar_splash(
    timer: Res<SplashTimer>,
    mut manchas: Query<(&mut Node, &Salpicon)>,
    mut textos: Query<(&mut TextColor, &mut TextFont, &MarcaTexto)>,
) {
    let paso = (timer.0 / PASO_SEGS).clamp(0.0, 1.0);
    for (mut nodo, m) in &mut manchas {
        let t0 = m.i * 0.075;
        let k = ((paso - t0) / 0.16).clamp(0.0, 1.0);
        if k <= 0.0 {
            continue;
        }
        let crecer = k * (1.12 - 0.12 * k);
        let lado = m.lado * crecer;
        // Aplastón al caer: ancha y baja al tocar, y se recupera (squash & stretch de dibujo animado).
        let aplasta = (1.0 - k).powi(2) * 0.16;
        let (w, h) = (lado * (1.0 + aplasta), lado * (1.0 - aplasta));
        nodo.width = Val::Px(w * m.k);
        nodo.height = Val::Px(h * m.k);
        nodo.left = Val::Px((m.centro.x - w / 2.0) * m.k);
        nodo.top = Val::Px((m.centro.y - h / 2.0 + (lado - h) * 0.5) * m.k);
    }
    for (mut color, mut fuente, t) in &mut textos {
        if paso <= t.desde {
            continue;
        }
        let k = ((paso - t.desde) / 0.2).clamp(0.0, 1.0);
        color.0 = color.0.with_alpha(1.0);
        fuente.font_size = (t.base + t.crece * k) * t.k;
    }
}

/// El sonido de la marca, atrasado para que los pops caigan sobre las manchas.
fn sonar_splash(
    timer: Res<SplashTimer>,
    mut sono: ResMut<SonoSplash>,
    mut commands: Commands,
    asset_server: Res<AssetServer>,
) {
    if sono.0 || timer.0 < RETARDO_AUDIO {
        return;
    }
    sono.0 = true;
    commands.spawn((
        AudioPlayer::<AudioSource>::new(asset_server.load("audio/splash.wav")),
        PlaybackSettings::DESPAWN,
        Name::new("SplashSonido"),
    ));
}

fn tick_splash(
    time: Res<Time<bevy::time::Real>>,
    mut timer: ResMut<SplashTimer>,
    mut next: ResMut<NextState<GamePhase>>,
) {
    // CI/agente: HA_AUTOPLAY salta el menú y arranca el gameplay directo (por
    // la pantalla de carga, igual que un jugador: así los drivers de QA miden un
    // mundo YA armado y no el suelo apareciendo a pedazos).
    if crate::flags::var("HA_AUTOPLAY").is_ok() {
        // `HA_PARTIDA_NUEVA=1`: el driver juega una partida recién creada, como
        // quien toca NUEVA PARTIDA. Es la prueba de que una partida nueva viene
        // limpia (nada de las paredes ni los pozos de corridas anteriores).
        if crate::flags::var("HA_PARTIDA_NUEVA").is_ok() && crate::partida::activa().is_none() {
            // `HA_SEMILLA=n` reproduce el mundo de una partida concreta (la del
            // usuario sale en la barra de PARTIDAS) para mirar lo que él ve.
            let semilla = crate::flags::var("HA_SEMILLA").ok().and_then(|v| v.parse().ok());
            crate::partida::crear("QA", semilla, false);
        }
        next.set(GamePhase::Loading);
        return;
    }
    timer.0 += time.delta_secs();
    if timer.0 >= SPLASH_SECS {
        next.set(GamePhase::MainMenu);
    }
}

/// **El menú principal, tal cual la imagen de referencia.**
///
/// La barra de estado arriba, el riel de opciones a la izquierda, el personaje
/// elegido en el medio con sus flechas y su nombre, y el panel MENÚ a la derecha
/// con cinco fichas: CONTINUAR se lleva el ancho entero porque es lo que se toca
/// casi siempre, y debajo van PARTIDAS, PERSONAJES, NUEVA PARTIDA y SALIR.
///
/// Cada número de acá está transcripto del mockup (`mock_menu::menu`): las
/// coordenadas son las de 2400 × 1080 y las escala [`crate::ui_pack::k`].
#[allow(clippy::too_many_arguments)]
fn spawn_main_menu(
    mut commands: Commands,
    touch: Res<TouchLayout>,
    art: Res<crate::art2d::ArtLibrary>,
    cg: Res<crate::chargen::Chargen>,
    hoja: Res<crate::ui_pack::HojaPj>,
    halo_tex: Res<crate::ui_pack::HaloTex>,
    assets: Res<AssetServer>,
    selected: Res<SelectedCharacter>,
) {
    construir_menu(&mut commands, &touch, &art, &cg, &hoja, &halo_tex, &assets, &selected);
}

/// **Rehacer el menú sin salir de él.** Las flechas del personaje cambian lo que
/// se ve en media pantalla; volver a entrar al estado no dispara `OnEnter`
/// cuando el estado no cambió, así que el redibujo se pide por bandera.
#[derive(Resource, Default)]
pub struct RehacerMenu(pub bool);

#[allow(clippy::too_many_arguments)]
fn rehacer_menu(
    mut flag: ResMut<RehacerMenu>,
    mut commands: Commands,
    raices: Query<Entity, With<BootRoot>>,
    touch: Res<TouchLayout>,
    art: Res<crate::art2d::ArtLibrary>,
    cg: Res<crate::chargen::Chargen>,
    hoja: Res<crate::ui_pack::HojaPj>,
    halo_tex: Res<crate::ui_pack::HaloTex>,
    assets: Res<AssetServer>,
    selected: Res<SelectedCharacter>,
) {
    if !flag.0 {
        return;
    }
    flag.0 = false;
    for e in &raices {
        commands.entity(e).despawn_recursive();
    }
    construir_menu(&mut commands, &touch, &art, &cg, &hoja, &halo_tex, &assets, &selected);
}

#[allow(clippy::too_many_arguments)]
fn construir_menu(
    commands: &mut Commands,
    touch: &TouchLayout,
    art: &crate::art2d::ArtLibrary,
    cg: &crate::chargen::Chargen,
    hoja: &crate::ui_pack::HojaPj,
    halo_tex: &crate::ui_pack::HaloTex,
    assets: &AssetServer,
    selected: &SelectedCharacter,
) {
    use crate::ui_pack::{self as up, Ancla, Pack};

    let k = up::k(&touch);
    let s = ui_scale(&touch);
    let fichas_partida = crate::partida::listar();
    let ultima = fichas_partida.first().cloned();
    let hay_partidas = !fichas_partida.is_empty();
    let personajes = crate::personajes::listar();
    let look = selected.look.or_else(|| personajes.first().and_then(|p| p.look));
    let nombre_pj = personajes
        .first()
        .map(|p| p.nombre.to_uppercase())
        .unwrap_or_else(|| "SIN PERSONAJE".to_string());

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
            FocusPolicy::Block,
            BootRoot,
            Name::new("BootOverlay"),
        ))
        .with_children(|root| {
            // El bosque del mockup, con el arte del pack.
            crate::marca::bosque::fondo(root, art, crate::marca::Ambiente::Menu, k);

            root.spawn((up::lienzo(k), MenuContenido, Name::new("MenuLienzo")))
                .with_children(|l| {
                    // ── la barra de arriba ────────────────────────────────
                    up::barra_estado(
                        l,
                        art,
                        up::CUERPO_TIERRA,
                        &[
                            up::Ficha {
                                icono: "ui/ic_grilla",
                                etiqueta: "SEMILLA",
                                valor: ultima
                                    .as_ref()
                                    .map(|f| separar_miles(f.semilla))
                                    .unwrap_or_else(|| "—".into()),
                            },
                            up::Ficha {
                                icono: "ui/ic_casa",
                                etiqueta: "PARTIDA",
                                valor: ultima
                                    .as_ref()
                                    .map(|f| f.nombre.clone())
                                    .unwrap_or_else(|| "—".into()),
                            },
                            up::Ficha {
                                icono: "ui/ic_reloj",
                                etiqueta: "DÍA",
                                valor: ultima
                                    .as_ref()
                                    .map(|f| f.dia.to_string())
                                    .unwrap_or_else(|| "—".into()),
                            },
                            up::Ficha {
                                icono: "ui/ic_persona",
                                etiqueta: "PERSONAJES",
                                valor: personajes.len().to_string(),
                            },
                        ],
                        k,
                    );

                    // ── el riel de la izquierda ───────────────────────────
                    let riel: [(&str, &str, MenuAction); 5] = [
                        ("ui/ic_persona", "MULTIJUGADOR", MenuAction::Multijugador),
                        ("ui/ic_engranaje", "AJUSTES", MenuAction::Settings),
                        ("ui/ic_controles", "CONTROLES", MenuAction::Controles),
                        ("ui/ic_bajar", "ACTUALIZAR", MenuAction::Actualizar),
                        ("ui/ic_interrogacion", "CRÉDITOS", MenuAction::Creditos),
                    ];
                    for (i, (ic, et, accion)) in riel.into_iter().enumerate() {
                        let yy = 200.0 + i as f32 * 126.0;
                        up::marco(l, art, up::marco::SLOT, 40.0, yy, 96.0, 96.0, k)
                            .insert((Button, accion));
                        up::icono(l, art, ic, 56.0, yy + 16.0, 64.0, k);
                        up::texto(l, et, 32.0, Pack::TEXTO, 156.0, yy + 50.0, Ancla::Izq, k);
                    }

                    // ── el personaje ──────────────────────────────────────
                    let cx = 900.0;
                    up::halo(l, halo_tex, cx, 690.0, 300.0, k);
                    up::sombra(l, cx, 878.0, 108.0, 18.0, k);
                    if let Some(look) = look {
                        // El mockup lo apoya en y = 900 y lo centra en cx.
                        let cel = Vec2::new(cg.index.cell[0] as f32, cg.index.cell[1] as f32) * 7.0;
                        up::muneco(
                            l, &look, &cg, &assets, &hoja, 3,
                            cx - cel.x / 2.0, 900.0 - cel.y, 7.0, k,
                        );
                    }
                    for (dx, ic, accion) in [
                        (-330.0_f32, "ui/ic_flecha_izq", MenuAction::PjAnterior),
                        (330.0, "ui/ic_flecha_der", MenuAction::PjSiguiente),
                    ] {
                        let bx = cx + dx - 48.0;
                        up::marco(l, art, up::marco::SLOT, bx, 640.0, 96.0, 96.0, k)
                            .insert((Button, accion));
                        up::icono(l, art, ic, bx + 16.0, 656.0, 64.0, k);
                    }
                    up::texto(l, nombre_pj, 52.0, Pack::TEXTO, cx, 960.0, Ancla::Medio, k);
                    for i in 0..3 {
                        let col = if i == 0 { Pack::ACENTO } else { Color::srgb(0.588, 0.510, 0.471) };
                        let px0 = cx - 34.0 + i as f32 * 34.0;
                        l.spawn((
                            up::caja(px0 - 8.0, 1000.0, 16.0, 16.0, k),
                            BackgroundColor(col),
                            BorderRadius::all(Val::Percent(50.0)),
                        ));
                    }

                    // ── el panel MENÚ ─────────────────────────────────────
                    let (pxu, pyu, pw, ph) = (1344.0_f32, 186.0_f32, 1020.0_f32, 828.0_f32);
                    up::marco(l, art, up::marco::PANEL, pxu, pyu, pw, ph, k);
                    let (ox, oy, tw, th, gap) = (pxu + 42.0, pyu + 84.0, 462.0_f32, 216.0_f32, 24.0_f32);

                    // CONTINUAR: el ancho entero, y elegido.
                    ficha_menu(
                        l, art, cg, assets, hoja, ox, oy, tw * 2.0 + gap, 228.0, "CONTINUAR",
                        &["nature/arbol_1", "room/casa_1", "prop/farol_de_calle_1"], None,
                        hay_partidas, MenuAction::Continuar, k,
                    );
                    let resto: [(&str, &[&str], Option<&[LookDeMuestra]>, MenuAction); 4] = [
                        ("PARTIDAS", &["edificio/local", "nature/piedra_1"], None, MenuAction::Partidas),
                        ("PERSONAJES", &[], Some(&[]), MenuAction::Personajes),
                        ("NUEVA PARTIDA", &["agua/bote_abajo_1", "nature/arbol_10"], None, MenuAction::NewGame),
                        ("SALIR", &["door/wood", "prop/farol_de_calle_1"], None, MenuAction::Quit),
                    ];
                    for (i, (et, deco, caras, accion)) in resto.into_iter().enumerate() {
                        let tx = ox + (i % 2) as f32 * (tw + gap);
                        let ty = oy + 228.0 + gap + (i / 2) as f32 * (th + gap);
                        let looks: Vec<crate::chargen::LookDef> = if caras.is_some() {
                            personajes.iter().filter_map(|p| p.look).take(3).collect()
                        } else {
                            Vec::new()
                        };
                        ficha_menu_caras(
                            l, art, cg, assets, hoja, tx, ty, tw, th, et, deco, &looks, false,
                            accion, k,
                        );
                    }
                    up::cartel(l, art, "MENÚ", pxu + pw / 2.0, pyu - 48.0, 420.0, 96.0, 54.0, k);
                });

            // Los paneles que se abren desde el riel. Nacen ocultos.
            paneles_del_riel(root, art, k, s);
        });
}

/// Tipo de relleno para la firma de la tabla de fichas (sólo marca "acá van
/// caras"): las caras reales salen de los personajes guardados.
type LookDeMuestra = ();

/// Un número de semilla con el punto de los miles, como en el mockup ("48 213").
pub fn separar_miles(n: u32) -> String {
    let s = n.to_string();
    let mut out = String::new();
    for (i, c) in s.chars().rev().enumerate() {
        if i > 0 && i % 3 == 0 {
            out.push(' ');
        }
        out.push(c);
    }
    out.chars().rev().collect()
}

/// Una ficha del panel MENÚ: casilla + viñeta + rótulo abajo.
#[allow(clippy::too_many_arguments)]
fn ficha_menu(
    l: &mut ChildBuilder,
    art: &crate::art2d::ArtLibrary,
    cg: &crate::chargen::Chargen,
    assets: &AssetServer,
    hoja: &crate::ui_pack::HojaPj,
    tx: f32,
    ty: f32,
    tw: f32,
    th: f32,
    etiqueta: &str,
    deco: &[&str],
    _caras: Option<&[LookDeMuestra]>,
    activa: bool,
    accion: MenuAction,
    k: f32,
) {
    ficha_menu_caras(l, art, cg, assets, hoja, tx, ty, tw, th, etiqueta, deco, &[], activa, accion, k)
}

/// La ficha de verdad. Si `looks` no está vacío, la viñeta muestra personajes en
/// vez de props — que es lo que hace la ficha PERSONAJES en el mockup.
#[allow(clippy::too_many_arguments)]
fn ficha_menu_caras(
    l: &mut ChildBuilder,
    art: &crate::art2d::ArtLibrary,
    cg: &crate::chargen::Chargen,
    assets: &AssetServer,
    hoja: &crate::ui_pack::HojaPj,
    tx: f32,
    ty: f32,
    tw: f32,
    th: f32,
    etiqueta: &str,
    deco: &[&str],
    looks: &[crate::chargen::LookDef],
    activa: bool,
    accion: MenuAction,
    k: f32,
) {
    use crate::ui_pack::{self as up, Ancla, Pack};
    up::marco(l, art, up::marco::SLOT, tx, ty, tw, th, k).insert((Button, accion));
    let (vw, vh) = (tw - 48.0, th - 108.0);
    if looks.is_empty() {
        up::vineta(l, art, tx + 24.0, ty + 22.0, vw, vh, deco, 2.0, true, 1, k);
    } else {
        // La viñeta con caras: mismo pasto, y los personajes apoyados.
        up::vineta(l, art, tx + 24.0, ty + 22.0, vw, vh, &[], 2.0, true, 1, k);
        let cel = cg.index.cell[1] as f32 * 2.0;
        for (j, look) in looks.iter().enumerate() {
            up::muneco(
                l, look, cg, assets, hoja, 3,
                tx + 24.0 + 60.0 + j as f32 * 90.0,
                ty + 22.0 + vh - cel - 4.0,
                2.0, k,
            );
        }
    }
    up::texto(l, etiqueta, 40.0, Pack::TINTA, tx + tw / 2.0, ty + th - 30.0, Ancla::Medio, k);
    if activa {
        up::elegido(l, tx, ty, tw, th, 6.0, k);
    }
}

/// Marca el lienzo del menú: las listas lo esconden para abrirse encima sin que
/// se lean los botones detrás.
#[derive(Component)]
pub struct MenuContenido;

/// Cuál de los cuatro paneles del riel es éste.
#[derive(Component, Clone, Copy, PartialEq, Eq)]
pub enum PanelRiel {
    Ajustes,
    Controles,
    Actualizar,
    Creditos,
}

/// **Los cuatro paneles del riel de la izquierda.** Nacen ocultos y se muestran
/// al tocar su ficha. Van con el marco del pack, como todo el resto.
fn paneles_del_riel(root: &mut ChildBuilder, art: &crate::art2d::ArtLibrary, k: f32, s: f32) {
    use crate::ui_pack::{self as up, Ancla, Pack};

    let contenido: [(PanelRiel, &str, f32, Vec<(String, bool)>); 4] = [
        (
            PanelRiel::Ajustes,
            "AJUSTES",
            520.0,
            vec![],
        ),
        (
            PanelRiel::Controles,
            "CONTROLES",
            560.0,
            vec![
                ("Mover · el stick de la izquierda".to_string(), false),
                ("Mirar y tocar · el dedo sobre el mundo".to_string(), false),
                ("Volar · mantené SALTAR en el aire".to_string(), false),
                ("Mochila · el botón de abajo a la derecha".to_string(), false),
                ("Acercar y alejar · pellizcá la pantalla".to_string(), false),
            ],
        ),
        (
            PanelRiel::Actualizar,
            "ACTUALIZAR",
            560.0,
            vec![
                (format!("Estás en la versión {}.", env!("CARGO_PKG_VERSION")), false),
                ("El APK nuevo se baja de la misma dirección".to_string(), false),
                ("desde la que instalaste éste.".to_string(), false),
            ],
        ),
        (
            PanelRiel::Creditos,
            "CRÉDITOS",
            560.0,
            vec![
                ("Ki · Mundo Abierto — FZ STUDIOS".to_string(), false),
                ("Pixel art · LimeZu (Modern Interiors,".to_string(), false),
                ("Modern Exteriors, Modern UI)".to_string(), false),
                ("Motor · Bevy".to_string(), false),
            ],
        ),
    ];

    for (cual, titulo, pw, lineas) in contenido {
        let ph = 560.0_f32;
        let pxu = (up::ANCHO - pw) / 2.0;
        let pyu = (up::ALTO - ph) / 2.0;
        root.spawn((
            Node {
                position_type: PositionType::Absolute,
                left: Val::Px(0.0),
                top: Val::Px(0.0),
                width: Val::Percent(100.0),
                height: Val::Percent(100.0),
                display: Display::None,
                ..default()
            },
            BackgroundColor(Color::srgba(0.047, 0.039, 0.078, 0.72)),
            FocusPolicy::Block,
            GlobalZIndex(1200),
            cual,
            SettingsPanel,
            Name::new("PanelRiel"),
        ))
        .with_children(|wrap| {
            wrap.spawn(up::lienzo(k)).with_children(|l| {
                up::marco(l, art, up::marco::PANEL, pxu, pyu, pw, ph, k);
                up::cartel(l, art, titulo, pxu + pw / 2.0, pyu - 48.0, 460.0, 96.0, 44.0, k);
                let ix = pxu + 66.0;
                let mut y = pyu + 110.0;
                for (linea, _) in &lineas {
                    up::texto(l, linea.clone(), 30.0, Pack::SUAVE, ix, y, Ancla::Izq, k);
                    y += 56.0;
                }
                if cual == PanelRiel::Ajustes {
                    // La única con controles de verdad.
                    up::boton(
                        l, art, ix, pyu + 110.0, pw - 132.0, 92.0,
                        Some("PANTALLA COMPLETA"), None, false, 34.0,
                        SettingsAction::ToggleFullscreen, k,
                    );
                    // El idioma: el botón muestra el vigente, siempre en su propia lengua.
                    up::boton(
                        l, art, ix, pyu + 216.0, pw - 132.0, 80.0,
                        Some(crate::i18n::idioma_actual().nombre()), None, false, 34.0,
                        SettingsAction::Idioma, k,
                    );
                    // La versión: siete toques y aparece la consola.
                    let vy = pyu + ph - 100.0;
                    up::marco(l, art, up::marco::BOTON, ix, vy, 360.0, 76.0, k)
                        .insert((Button, SettingsAction::Version));
                    l.spawn(up::caja(ix, vy, 360.0, 76.0, k)).with_children(|t| {
                        t.spawn((
                            Node {
                                position_type: PositionType::Absolute,
                                left: Val::Px(0.0),
                                top: Val::Px(22.0 * k),
                                width: Val::Px(360.0 * k),
                                height: Val::Px(40.0 * k),
                                justify_content: JustifyContent::Center,
                                align_items: AlignItems::Center,
                                ..default()
                            },
                            Name::new("VersionCaja"),
                        ))
                        .with_children(|t| {
                            t.spawn((
                                Text::new(format!("v{}", env!("CARGO_PKG_VERSION"))),
                                TextFont { font_size: 30.0 * k, ..default() },
                                TextColor(Pack::TINTA),
                                TextLayout::new_with_no_wrap(),
                                VersionLabel,
                            ));
                        });
                    });
                    // Historial de cierres: con el APK por URL nadie tiene un
                    // cable para hacer `adb pull`.
                    let historial = crate::debug::crash_history_lines(3);
                    let mut hy = pyu + 312.0;
                    for linea in historial {
                        up::texto(l, linea, 22.0, Pack::SUAVE, ix, hy, Ancla::Izq, k);
                        hy += 40.0;
                    }
                }
                up::boton(
                    l, art, pxu + pw - 66.0 - 260.0, pyu + ph - 66.0 - 92.0, 260.0, 92.0,
                    Some("VOLVER"), None, true, 34.0, SettingsAction::Close, k,
                );
            });
        });
    }
    let _ = s;
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
/// El sistema de toques del menú. El realce de "tocado" ya no es un cambio de
/// color de fondo —los botones son marcos del pack, no rectángulos— sino un
/// tinte sobre la imagen, que es lo que respeta el pixel del marco.
#[allow(clippy::too_many_arguments, clippy::type_complexity)]
fn menu_button_system(
    mut interactions: Query<
        (&Interaction, &MenuAction, Option<&mut ImageNode>),
        Changed<Interaction>,
    >,
    mut next: ResMut<NextState<GamePhase>>,
    mut selected: ResMut<SelectedCharacter>,
    mut exit: EventWriter<AppExit>,
    mut panels: Query<(&mut Node, &PanelRiel), With<SettingsPanel>>,
    mut pantalla: ResMut<crate::menu_listas::Pantalla>,
    mut borrador: ResMut<crate::menu_listas::BorradorPartida>,
    mut cual: Local<usize>,
    mut rehacer: ResMut<RehacerMenu>,
    mut multi: ResMut<crate::multijugador::Multi>,
) {
    for (interaction, action, img) in &mut interactions {
        if let Some(mut img) = img {
            img.color = match interaction {
                Interaction::Pressed => Color::srgb(0.82, 0.82, 0.82),
                Interaction::Hovered => Color::srgb(0.93, 0.93, 0.93),
                Interaction::None => Color::WHITE,
            };
        }
        if *interaction != Interaction::Pressed {
            continue;
        }
        let mut abrir = |quien: PanelRiel| {
            for (mut node, p) in &mut panels {
                node.display = if *p == quien && node.display == Display::None {
                    Display::Flex
                } else {
                    Display::None
                };
            }
        };
        match action {
            // **CONTINUAR = la última partida que jugaste.** Abrirla es lo
            // primero que pasa: la semilla tiene que estar puesta ANTES de que
            // exista un solo tile, o el mundo se genera con la del mundo
            // anterior (ver `crate::partida`).
            MenuAction::Continuar => {
                if let Some(f) = crate::partida::ultima() {
                    crate::partida::abrir(&f.id);
                    if let Some(saved) = load_savegame() {
                        *selected = saved;
                    }
                    next.set(GamePhase::Loading);
                }
            }
            MenuAction::Partidas => *pantalla = crate::menu_listas::Pantalla::Partidas,
            MenuAction::Personajes => *pantalla = crate::menu_listas::Pantalla::Personajes,
            // **NUEVA PARTIDA ya no crea nada acá**: abre la pantalla donde se
            // eligen nombre, semilla y modo. El creativo se decide cuando nace
            // el mundo (decisión del 15 ago 2026, y por eso ya no hay ficha
            // CREATIVO).
            MenuAction::NewGame => {
                crate::menu_listas::preparar_borrador(&mut borrador);
                *pantalla = crate::menu_listas::Pantalla::Nueva;
            }
            MenuAction::Multijugador => {
                multi.buscar(true);
                multi.mensaje.clear();
                *pantalla = crate::menu_listas::Pantalla::Multijugador;
            }
            MenuAction::Settings => abrir(PanelRiel::Ajustes),
            MenuAction::Controles => abrir(PanelRiel::Controles),
            MenuAction::Actualizar => abrir(PanelRiel::Actualizar),
            MenuAction::Creditos => abrir(PanelRiel::Creditos),
            // Las flechas del mockup: pasan de personaje sin salir del menú.
            MenuAction::PjAnterior | MenuAction::PjSiguiente => {
                let lista = crate::personajes::listar();
                if lista.is_empty() {
                    continue;
                }
                let n = lista.len();
                *cual = if matches!(action, MenuAction::PjSiguiente) {
                    (*cual + 1) % n
                } else {
                    (*cual + n - 1) % n
                };
                *selected = crate::personajes::a_seleccionado(&lista[*cual]);
                // Redibujar el menú es lo que muestra al nuevo elegido.
                rehacer.0 = true;
            }
            MenuAction::Quit => {
                exit.send(AppExit::Success);
            }
        }
    }
}

#[allow(clippy::type_complexity)]
fn settings_button_system(
    mut interactions: Query<(&Interaction, &SettingsAction, &mut BackgroundColor), Changed<Interaction>>,
    mut panels: Query<&mut Node, With<SettingsPanel>>,
    mut windows: Query<&mut Window, With<PrimaryWindow>>,
    mut consola: ResMut<crate::consola::Consola>,
    mut etiqueta: Query<&mut Text, With<VersionLabel>>,
    mut idioma: ResMut<crate::i18n::Idioma>,
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
                    SettingsAction::Idioma => {
                        *idioma = idioma.otro();
                    }
                    SettingsAction::Close => {
                        if let Ok(mut node) = panels.get_single_mut() {
                            node.display = Display::None;
                        }
                    }
                    SettingsAction::Version => {
                        // Ya destrabada, el toque la ABRE: no hay otra puerta.
                        if consola.desbloqueada {
                            consola.abierta = true;
                        } else {
                            consola.toque_en_version();
                            // Se avisa sólo en los últimos tres, para que quien
                            // llegó por accidente no descubra que hay algo.
                            let faltan = consola.faltan();
                            let destrabada = consola.desbloqueada;
                            if let Ok(mut txt) = etiqueta.get_single_mut() {
                                txt.0 = if destrabada {
                                    "CONSOLA DESTRABADA".to_string()
                                } else if faltan <= 3 {
                                    format!("v{} — faltan {faltan}", env!("CARGO_PKG_VERSION"))
                                } else {
                                    format!("v{}", env!("CARGO_PKG_VERSION"))
                                };
                            }
                        }
                    }
                }
            }
            Interaction::Hovered if *action != SettingsAction::Version => {
                *bg = BackgroundColor(Color::srgb(0.14, 0.20, 0.30))
            }
            Interaction::None if *action != SettingsAction::Version => {
                *bg = BackgroundColor(BTN_IDLE)
            }
            _ => {}
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

/// QA de dev: `HA_SPLASHSHOT=<ruta.png>` captura la splash ya asentada (todas las manchas y el logo) y sale.
/// `HA_SPLASHSHOT_T=<s>` elige el instante (por defecto 2,3 s) para mirar la animación a mitad de camino.
fn splash_screenshot(timer: Res<SplashTimer>, mut shot: ResMut<MenuShot>, mut commands: Commands, mut exit: EventWriter<AppExit>) {
    let Ok(path) = crate::flags::var("HA_SPLASHSHOT") else { return };
    let t: f32 = crate::flags::var("HA_SPLASHSHOT_T").ok().and_then(|v| v.parse().ok()).unwrap_or(2.3);
    if shot.done {
        return;
    }
    if shot.frames == 0 && timer.0 >= t {
        shot.frames = 1;
        commands.spawn(Screenshot::primary_window()).observe(save_to_disk(path));
        eprintln!("ha-splashshot: capturando splash en t={:.2}", timer.0);
    } else if shot.frames > 0 {
        shot.frames += 1;
        if shot.frames > 20 {
            shot.done = true;
            exit.send(AppExit::Success);
        }
    }
}

fn menu_screenshot(mut shot: ResMut<MenuShot>, mut commands: Commands, mut exit: EventWriter<AppExit>) {
    let Ok(path) = crate::flags::var("HA_MENUSHOT") else {
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


/// El botón de idioma dice el idioma vigente, en su propia lengua (no se traduce).
fn rotular_idioma(
    idioma: Res<crate::i18n::Idioma>,
    botones: Query<(Entity, &SettingsAction)>,
    hijos: Query<&Children>,
    mut textos: Query<&mut Text>,
) {
    if !idioma.is_changed() {
        return;
    }
    for (e, a) in &botones {
        if *a != SettingsAction::Idioma {
            continue;
        }
        let mut pila = vec![e];
        while let Some(n) = pila.pop() {
            if let Ok(mut t) = textos.get_mut(n) {
                t.0 = idioma.nombre().to_string();
            }
            if let Ok(h) = hijos.get(n) {
                pila.extend(h.iter().copied());
            }
        }
    }
}
