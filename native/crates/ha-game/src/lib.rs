//! Starship — native ECS client (Bevy + ha-shared). 2D SFS-style solar system
//! + Terraria-style diggable surfaces (rewrite-2d).

mod art2d;
mod boot_ui;
mod build;
mod game_menu;
mod game_time;
mod sky2d;
mod camera;
mod world_persist;
mod telemetry;
mod solar_system_clock;
mod debug;
mod game_ui;
mod hud;
mod input;
mod inventory_persist;
mod launch_pad;
mod machines;
mod orbital2d;
mod platform;
mod render;
mod rocket_flight;
mod scene;
mod creative_fly;
mod ship_interior;
mod nav_computer;
mod colony;
mod harvest;
mod ship_builder;
mod sim;
mod survival_stats;
mod targeting;
mod ui_editor;
mod ui_theme;
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
use debug::{apply_android_frame_cap, install_crash_hook, AutoDrivePlugin, CrashReportPlugin, CpuSpanPlugin, PerfDebugPlugin};
use solar_system_clock::SolarSystemClockPlugin;
use hud::PixelHudPlugin;
use boot_ui::BootUiPlugin;
use game_menu::GameMenuPlugin;
use input::{InputPlugin, LookInteractPlugin};
use launch_pad::LaunchPadPlugin;
use machines::MachinesPlugin;
use orbital2d::Orbital2dPlugin;
use rocket_flight::RocketFlightPlugin;
use nav_computer::NavComputerPlugin;
use colony::ColonyPlugin;
use harvest::HarvestPlugin;
use ship_builder::ShipBuilderPlugin;
use survival_stats::SurvivalStatsPlugin;
use inventory_persist::InventoryPersistPlugin;
use platform::PlatformPlugin;
use render::particles::ParticlesPlugin;
use scene::ScenePlugin;
use creative_fly::CreativeFlyPlugin;
use ship_interior::ShipInteriorPlugin;
use sky2d::Sky2dPlugin;
use sim::SimPlugin;
use targeting::TargetingPlugin;
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
        title: "Starship".into(),
        #[cfg(target_os = "android")]
        present_mode: bevy::window::PresentMode::Fifo,
        #[cfg(not(target_os = "android"))]
        present_mode: bevy::window::PresentMode::AutoVsync,
        ..default()
    };

    #[cfg(not(target_os = "android"))]
    {
        window.resolution = (1280.0, 720.0).into();
    }

    #[cfg(target_os = "android")]
    {
        window.resizable = false;
        window.mode = bevy::window::WindowMode::BorderlessFullscreen(
            bevy::window::MonitorSelection::Primary,
        );
    }

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
            #[cfg(target_os = "android")]
            {
                WinitSettings::mobile()
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
            PlatformPlugin,
            Art2dPlugin,
            WorldPersistPlugin,
            SolarSystemClockPlugin,
            IconFactoryPlugin,
            SimPlugin,
            ScenePlugin,
            World2dPlugin,
            Orbital2dPlugin,
            Sky2dPlugin,
        ))
        .add_plugins((
            InventoryPersistPlugin,
            MachinesPlugin,
            LaunchPadPlugin,
            InputPlugin,
            TargetingPlugin,
            LookInteractPlugin,
            RocketFlightPlugin,
            NavComputerPlugin,
            ShipInteriorPlugin,
            CreativeFlyPlugin,
            HarvestPlugin,
            SurvivalStatsPlugin,
            ShipBuilderPlugin,
            ColonyPlugin,
        ))
        .add_plugins((
            FollowCameraPlugin,
            TelemetryPlugin,
            PixelHudPlugin,
            BootUiPlugin,
            GameMenuPlugin,
            build::BuildPlugin,
            ParticlesPlugin,
            PerfDebugPlugin,
            CpuSpanPlugin,
            CrashReportPlugin,
            UiEditorPlugin,
            AutoDrivePlugin,
        ))
        .add_systems(Startup, apply_android_frame_cap)
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
