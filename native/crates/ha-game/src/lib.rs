//! Starship — native ECS client (Bevy + ha-shared). 2D SFS-style solar system
//! + Terraria-style diggable surfaces (rewrite-2d).

mod art2d;
mod procedural_art;
mod audio;
mod boot_ui;
mod game_phase;
mod chargen;
mod character_create;
mod build;
mod building;
mod ceilings;
mod integrity;
mod structures;
mod walls;
mod weather;
mod world_edits;
mod companion;
mod dialog;
mod fauna;
mod ambient;
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
mod layers;
mod loading;
mod inventory_persist;
mod machines;
mod npc_menu;
mod space_fx;
mod universe;
mod platform;
mod render;
mod flight;
mod scene;
mod creative_fly;
mod nav_computer;
mod colony;
mod society;
mod harvest;
mod held_item;
mod chests;
mod sim;
mod survival_stats;
mod targeting;
pub mod map_item;
pub mod torches;
mod transition;
mod ui_editor;
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

    #[cfg(not(target_os = "android"))]
    {
        // `HA_RES=ancho x alto` — palanca de diagnóstico, no de juego. Sirve para
        // separar "cuesta la CPU" de "cuesta llenar píxeles": si el frame no baja
        // al achicar la ventana a 160×90, el gasto NO es fill-rate.
        let (w, h) = std::env::var("HA_RES")
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
    #[cfg_attr(target_os = "android", allow(unused_mut))]
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
                if std::env::var("HA_AUTOPLAY").is_ok() {
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
        .add_plugins(AblatePlugin)
        .add_plugins(debug::StageProbePlugin)
        .add_plugins(chargen::ChargenPlugin)
        .add_plugins(loading::LoadingPlugin)
        .add_plugins(vault::VaultPlugin)
        .add_plugins(torches::TorchesPlugin)
        .add_plugins(map_item::MapItemPlugin)
        .add_plugins(weather::WeatherPlugin)
        // Aparte: la tupla de `add_plugins` topa en 16 elementos.
        .add_plugins(RenderLodPlugin)
        .add_plugins(render::scale::RenderScalePlugin)
        .add_plugins(layers::LayersPlugin)
        .add_plugins((
            world_edits::WorldEditsPlugin,
            walls::WallsPlugin,
            ceilings::CeilingsPlugin,
            integrity::IntegrityPlugin,
            fauna::FaunaPlugin,
            companion::CompanionPlugin,
            dialog::DialogPlugin,
            society::SocietyPlugin,
            npc_menu::NpcMenuPlugin,
            camera_feed::CameraFeedPlugin,
            held_item::HeldItemPlugin,
            chests::ChestsPlugin,
        ))
        .add_systems(Startup, apply_android_frame_cap)
        // Android: SALIR mata el proceso (ver `platform::hard_exit_on_appexit`).
        .add_systems(bevy::app::Last, {
            #[cfg(target_os = "android")]
            {
                platform::hard_exit_on_appexit
            }
            #[cfg(not(target_os = "android"))]
            {
                || {}
            }
        })
        .run();
}

#[cfg(not(target_os = "android"))]
fn desktop_wgpu_backends() -> Backends {
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
