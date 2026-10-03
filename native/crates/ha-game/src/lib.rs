//! Starship — native ECS client (Bevy + ha-shared). 2D SFS-style solar system
//! + Terraria-style diggable surfaces (rewrite-2d).

mod art2d;
mod manos;
mod flags;
mod fuente;
mod prueba_overlay;
mod procedural_art;
mod audio;
mod boot_ui;
mod game_phase;
mod chargen;
mod character_create;
mod build;
mod building;
mod ceilings;
mod cuartos;
mod edificios;
mod integrity;
mod structures;
mod walls;
mod weather;
mod partida;
mod personajes;
mod menu_listas;
mod objetivo;
mod world_edits;
mod companion;
mod dialog;
mod fauna;
mod granja;
mod textos_mundo;
mod tirados;
mod bautizo;
mod salir;
mod cubo;
mod fps_tope;
mod equipo;
mod frutales;
pub mod i18n;
pub mod red;
pub mod metas;
pub mod habilidades;
pub mod infectados;
pub mod armas;
mod pose_jugador;
mod descanso;
pub mod estados;
pub mod alijos;
pub mod causal;
pub mod materias;
pub mod experimentos;
pub mod casas_npc;
pub mod saber_vivo;
pub mod planet_art;
pub mod barrio;
pub mod multijugador;
mod ambient;
mod consola;
mod marca;
mod light2d;
mod game_menu;
mod game_time;
mod sky2d;
mod camera;
mod sightings;
mod camera_feed;
mod world_persist;
mod telemetry;
mod solar_system_clock;
mod debug;
mod game_ui;
mod hud;
mod input;
mod escaleras;
mod interior;
mod layers;
mod loading;
mod inventory_persist;
mod machines;
mod npc_menu;
mod puertas;
mod obstaculos;
mod space_fx;
mod universe;
mod platform;
mod render;
mod flight;
mod scene;
mod creative_fly;
mod nav_computer;
mod colony;
mod persistent_agents;
mod society;
mod civ_vida;
mod civilizacion;
mod harvest;
mod held_item;
mod chests;
mod sim;
mod survival_stats;
mod targeting;
pub mod map_item;
pub mod torches;
/// Apuntado asistido (puro): a quién le tirás y hacia dónde sale la bala. Lo usa `armas`; receta en la skill
/// `disparo-y-apuntado`.
pub mod apuntado;
mod transition;
mod ui_editor;
mod ui_pack;
mod ui_theme;
mod vault;
mod ui;
mod world2d;

use bevy::prelude::*;
use bevy::winit::WinitSettings;
#[cfg(not(target_os = "android"))]
use bevy::render::{
    settings::{Backends, RenderCreation, WgpuSettings, WgpuSettingsPriority},
    RenderPlugin,
};
use ha_shared::Ps1Retro;
use art2d::Art2dPlugin;
use camera::FollowCameraPlugin;
use world_persist::WorldPersistPlugin;
use telemetry::TelemetryPlugin;
use debug::{apply_android_frame_cap, install_crash_hook, AblatePlugin, AutoDrivePlugin, CrashReportPlugin, CpuSpanPlugin, PerfDebugPlugin};
use solar_system_clock::SolarSystemClockPlugin;
use hud::PixelHudPlugin;
use boot_ui::BootUiPlugin;
use game_menu::GameMenuPlugin;
use input::{InputPlugin, LookInteractPlugin, WorldTouchPlugin};
use machines::MachinesPlugin;
use space_fx::SpaceFxPlugin;
use universe::UniversePlugin;
use flight::FlightPlugin;
use nav_computer::NavComputerPlugin;
use colony::ColonyPlugin;
use harvest::HarvestPlugin;
use survival_stats::SurvivalStatsPlugin;
use inventory_persist::InventoryPersistPlugin;
use platform::PlatformPlugin;
use render::lod::RenderLodPlugin;
use render::particles::ParticlesPlugin;
use scene::ScenePlugin;
use creative_fly::CreativeFlyPlugin;
use sky2d::Sky2dPlugin;
use sim::SimPlugin;
use targeting::TargetingPlugin;
use transition::TransitionPlugin;
use ui::IconFactoryPlugin;
use ui_editor::UiEditorPlugin;
use world2d::World2dPlugin;

#[bevy_main]
pub fn main() {
    run_app();
}

pub use input::DEFAULT_RENDER_SCALE;

#[cfg(target_os = "android")]
static ANDROID_APP_ONCE: std::sync::Once = std::sync::Once::new();

pub fn run_app() {
    install_crash_hook();

    #[cfg(target_os = "android")]
    {
        ANDROID_APP_ONCE.call_once(run_app_inner);
        return;
    }

    #[cfg(not(target_os = "android"))]
    run_app_inner();
}

fn run_app_inner() {
    let mut window = Window {
        title: "Ki - Mundo Abierto".into(),
        #[cfg(target_os = "android")]
        present_mode: bevy::window::PresentMode::Fifo,
        #[cfg(not(target_os = "android"))]
        present_mode: bevy::window::PresentMode::AutoVsync,
        ..default()
    };

    // **Interruptores para cazar el parpadeo** (1 oct 2026): de a ratos, un cuadro sale SIN
    // ningún sprite (sólo suelo + UI) cada ~6 s, en el Redmi. Se aíslan sin recompilar:
    // `setprop debug.ha.present mailbox|immediate|fifo|relaxed`, `debug.ha.latencia 1|2|3`.
    if let Ok(v) = crate::flags::var("HA_PRESENT") {
        window.present_mode = match v.as_str() {
            "mailbox" => bevy::window::PresentMode::Mailbox,
            "immediate" => bevy::window::PresentMode::Immediate,
            "relaxed" => bevy::window::PresentMode::FifoRelaxed,
            "auto" => bevy::window::PresentMode::AutoVsync,
            _ => bevy::window::PresentMode::Fifo,
        };
    }
    if let Some(n) = crate::flags::var("HA_LATENCIA").ok().and_then(|v| v.parse::<u32>().ok()).and_then(std::num::NonZeroU32::new) {
        window.desired_maximum_frame_latency = Some(n);
    }

    #[cfg(not(target_os = "android"))]
    {
        // `HA_RES=ancho x alto` — palanca de diagnóstico, no de juego. Sirve para
        // separar "cuesta la CPU" de "cuesta llenar píxeles": si el frame no baja
        // al achicar la ventana a 160×90, el gasto NO es fill-rate.
        let (w, h) = crate::flags::var("HA_RES")
            .ok()
            .and_then(|v| {
                let (a, b) = v.split_once('x')?;
                Some((a.trim().parse().ok()?, b.trim().parse().ok()?))
            })
            .unwrap_or((1280.0, 720.0));
        window.resolution = (w, h).into();
    }

    #[cfg(target_os = "android")]
    {
        window.resizable = false;
        window.mode = bevy::window::WindowMode::BorderlessFullscreen(
            bevy::window::MonitorSelection::Primary,
        );
// LA UI VA A RESOLUCIÓN NATIVA. Se probó bajar la escala de la surface a 0.7
        // para ganar fill-rate y el reporte fue inmediato: "la UI en mi celu es
        // demasiado pequeña, ni siquiera puedo leer el texto del reloj". Y encima no
        // dio la ganancia buscada (seguía en 10-13 fps). El ahorro de píxeles tiene
        // que salir del render del MUNDO, nunca de la legibilidad de la interfaz.
    }

    // `mut` sólo hace falta en el camino no-android (abajo se le encadena más).
    let mut plugins = DefaultPlugins
        .set(WindowPlugin {
            primary_window: Some(window),
            ..default()
        })
        .set(ImagePlugin {
            default_sampler: bevy::image::ImageSamplerDescriptor::nearest(),
        });

    #[cfg(not(target_os = "android"))]
    {
        plugins = plugins.set(RenderPlugin {
            render_creation: RenderCreation::Automatic(WgpuSettings {
                backends: Some(desktop_wgpu_backends()),
                priority: WgpuSettingsPriority::Functionality,
                ..default()
            }),
            ..default()
        });
        // Desktop: assets live in the crate (crates/ha-game/assets), not the
        // cwd — point Bevy at the absolute crate dir so drop-in art loads.
        // Android bundles them in the APK, so its default path is correct.
        plugins = plugins.set(bevy::asset::AssetPlugin {
            file_path: concat!(env!("CARGO_MANIFEST_DIR"), "/assets").to_string(),
            ..default()
        });
    }

    // `setprop debug.ha.backend gl|vulkan` / `debug.ha.sync_pipe 1` (celu): aislar si el parpadeo
    // es del driver o de pipelines que se compilan y mientras tanto no se dibujan.
    #[cfg(target_os = "android")]
    if crate::flags::var("HA_BACKEND").is_ok() || crate::flags::var("HA_SYNC_PIPE").is_ok() {
        use bevy::render::settings::{Backends, RenderCreation, WgpuSettings};
        let backends = match crate::flags::var("HA_BACKEND").as_deref() {
            Ok("gl") => Some(Backends::GL),
            Ok(_) => Some(Backends::VULKAN),
            Err(_) => None,
        };
        plugins = plugins.set(bevy::render::RenderPlugin {
            render_creation: RenderCreation::Automatic(WgpuSettings { backends, ..default() }),
            synchronous_pipeline_compilation: crate::flags::var("HA_SYNC_PIPE").is_ok(),
            ..default()
        });
    }
    if crate::flags::var("HA_SIN_PIPELINE").is_ok() {
        plugins = plugins.disable::<bevy::render::pipelined_rendering::PipelinedRenderingPlugin>();
    }

    App::new()
        .add_plugins(plugins)
        .insert_resource(ClearColor(Color::srgb(
            Ps1Retro::SKY_ZENITH[0],
            Ps1Retro::SKY_ZENITH[1],
            Ps1Retro::SKY_ZENITH[2],
        )))
        .insert_resource({
            // **`Continuous`, NO `mobile()`.** `WinitSettings::mobile()` es
            // `UpdateMode::reactive(1/60)`: actualiza cuando pasan 16.6 ms *o
            // cuando llega un evento de entrada*. Una pantalla táctil reporta a
            // 120-240 Hz, así que con el dedo apoyado en el joystick el juego
            // corría 2-4× más seguido de lo que presenta (`Fifo` = vsync a 60):
            // `Time::delta()` saltaba frame a frame, el movimiento se escalonaba
            // y la UI vibraba — y empeoraba justo mientras tocabas la pantalla.
            //
            // `reactive` es para una app que espera al usuario. Un juego de
            // acción quiere el paso que marca el vsync, y nada más. En segundo
            // plano sí se baja el ritmo: ahí no hay nada que animar.
            #[cfg(target_os = "android")]
            {
                WinitSettings {
                    focused_mode: bevy::winit::UpdateMode::Continuous,
                    unfocused_mode: bevy::winit::UpdateMode::reactive_low_power(
                        std::time::Duration::from_secs(1),
                    ),
                }
            }
            #[cfg(not(target_os = "android"))]
            {
                if crate::flags::var("HA_AUTOPLAY").is_ok() {
                    // Agent test drives run unfocused — never throttle.
                    WinitSettings {
                        focused_mode: bevy::winit::UpdateMode::Continuous,
                        unfocused_mode: bevy::winit::UpdateMode::Continuous,
                    }
                } else {
                    WinitSettings::game()
                }
            }
        })
        // Aparte y primero: la fuente de la UI. En su propia llamada porque el
        // tuple de abajo ya esta en el maximo de 16 que soporta `add_plugins`.
        .add_plugins((fuente::FuentePlugin, prueba_overlay::PruebaOverlayPlugin))
        .add_plugins((
            game_phase::GamePhasePlugin,
            character_create::CharacterCreatePlugin,
            PlatformPlugin,
            Art2dPlugin,
            WorldPersistPlugin,
            SolarSystemClockPlugin,
            IconFactoryPlugin,
            SimPlugin,
            ScenePlugin,
            World2dPlugin,
            UniversePlugin,
            SpaceFxPlugin,
            ambient::AmbientPlugin,
            light2d::Light2dPlugin,
            Sky2dPlugin,
        ))
        .add_plugins((
            procedural_art::ProceduralArtPlugin,
            InventoryPersistPlugin,
            MachinesPlugin,
            InputPlugin,
            WorldTouchPlugin,
            TargetingPlugin,
            LookInteractPlugin,
            FlightPlugin,
            NavComputerPlugin,
            TransitionPlugin,
            CreativeFlyPlugin,
            HarvestPlugin,
            SurvivalStatsPlugin,
            ColonyPlugin,
        ))
        // El fondo parallax de las pantallas de menú. En su propia llamada
        // porque el tuple de al lado ya llegó al tope de 16 que admite `Plugins`.
        .add_plugins((ui_pack::UiPackPlugin, marca::ParallaxPlugin, consola::ConsolaPlugin, menu_listas::MenuListasPlugin, objetivo::ObjetivoPlugin, puertas::PuertasPlugin))
        .add_plugins((
            FollowCameraPlugin,
            TelemetryPlugin,
            PixelHudPlugin,
            BootUiPlugin,
            GameMenuPlugin,
            audio::GameAudioPlugin,
            build::BuildPlugin,
            building::BuildingPlugin,
            structures::StructuresPlugin,
            ParticlesPlugin,
            PerfDebugPlugin,
            CpuSpanPlugin,
            CrashReportPlugin,
            UiEditorPlugin,
            AutoDrivePlugin,
        ))
        // Los fuegos artificiales, en su propia llamada: el tuple de arriba ya
        // está en el tope de 16 que admite `Plugins` (el mismo motivo por el que
        // el parallax de los menús está aparte).
        .add_plugins(render::fireworks::FireworksPlugin)
        // **Herramientas de la compu, no del teléfono** (reporte del 29 sep 2026:
        // "las pruebas que hagas, que se queden para la compu"). La sonda de
        // etapas mete siete schedules extra en CADA frame; la ablación y el
        // overlay de colisión sólo sirven con variables de entorno.
        .add_plugins(herramientas_de_la_compu)
        .add_plugins(chargen::ChargenPlugin)
        .add_plugins(loading::LoadingPlugin)
        .add_plugins(vault::VaultPlugin)
        .add_plugins(escaleras::EscalerasPlugin)
        .add_plugins(interior::InteriorPlugin)
        .add_plugins(torches::TorchesPlugin)
        .add_plugins(map_item::MapItemPlugin)
        .add_plugins(weather::WeatherPlugin)
        .add_plugins(granja::GranjaPlugin)
        .add_plugins(textos_mundo::TextosMundoPlugin)
        .add_plugins(armas::ArmasPlugin)
        .add_plugins((pose_jugador::PoseJugadorPlugin, descanso::DescansoPlugin))
        .add_plugins((tirados::TiradosPlugin, bautizo::BautizoPlugin, salir::SalirPlugin, cubo::CuboPlugin, frutales::FrutalesPlugin, i18n::IdiomaPlugin, multijugador::MultijugadorPlugin, metas::MetasPlugin, habilidades::HabilidadesPlugin, infectados::InfectadosPlugin, estados::EstadosPlugin, alijos::AlijosPlugin, causal::CausalPlugin, equipo::EquipoPlugin, fps_tope::FpsTopePlugin))
        .add_plugins((experimentos::ExperimentosPlugin, casas_npc::CasasNpcPlugin, saber_vivo::SaberVivoPlugin, barrio::BarrioPlugin))
        // Aparte: la tupla de `add_plugins` topa en 16 elementos.
        .add_plugins(RenderLodPlugin)
        .add_plugins(render::scale::RenderScalePlugin)
        .add_plugins(layers::LayersPlugin)
        .add_plugins((
            world_edits::WorldEditsPlugin,
            walls::WallsPlugin,
            cuartos::CuartosPlugin,
            edificios::EdificiosPlugin,
            ceilings::CeilingsPlugin,
            integrity::IntegrityPlugin,
            fauna::FaunaPlugin,
            companion::CompanionPlugin,
            dialog::DialogPlugin,
            society::SocietyPlugin,
            civilizacion::CivPlugin,
            npc_menu::NpcMenuPlugin,
            camera_feed::CameraFeedPlugin,
            held_item::HeldItemPlugin,
            chests::ChestsPlugin,
        ))
        .add_plugins(manos::ManosPlugin)
        .add_plugins(civ_vida::CivVidaPlugin)
        .add_systems(Startup, apply_android_frame_cap)
        .run();
}

#[cfg(not(target_os = "android"))]
fn desktop_wgpu_backends() -> Backends {
    // `WGPU_BACKEND=gl` manda (lo documentan los drivers de QA y la skill
    // `swarm-sim`, pero estaba fijado a Vulkan y se ignoraba). Hace falta para
    // correr el juego en una pantalla virtual (Xvfb), donde Vulkan no tiene
    // superficie: así se capturan pantallas con la compu bloqueada.
    match std::env::var("WGPU_BACKEND").map(|v| v.to_ascii_lowercase()) {
        Ok(v) if v == "gl" || v == "opengl" || v == "gles" => return Backends::GL,
        Ok(v) if v == "vulkan" || v == "vk" => return Backends::VULKAN,
        _ => {}
    }
    #[cfg(target_os = "linux")]
    {
        Backends::VULKAN
    }
    #[cfg(target_os = "windows")]
    {
        Backends::DX12 | Backends::VULKAN
    }
    #[cfg(target_os = "macos")]
    {
        Backends::METAL
    }
    #[cfg(not(any(target_os = "linux", target_os = "windows", target_os = "macos")))]
    {
        Backends::VULKAN
    }
}

/// Los plugins de diagnóstico que sólo existen en la compu. En el APK no se
/// registran: nada de lo que se mide o se prueba viaja al teléfono.
fn herramientas_de_la_compu(app: &mut App) {
    #[cfg(not(target_os = "android"))]
    app.add_plugins((AblatePlugin, debug::StageProbePlugin, debug::VerColisionPlugin));
    // En el celu sólo con `setprop debug.ha.perflog 1` (ver `debug::perf`); sin la
    // propiedad no se registra nada.
    #[cfg(target_os = "android")]
    if flags::var("HA_PERFLOG").is_ok() {
        app.add_plugins((AblatePlugin, debug::StageProbePlugin));
    }
}
