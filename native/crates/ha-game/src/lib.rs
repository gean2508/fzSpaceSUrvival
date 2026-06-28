//! Starship — native ECS client (Bevy + ha-shared). SFS-style 3D solar system.

mod game_time;
mod sky_clock;
mod assets;
mod camera;
mod celestial_cleanup;
mod celestial_sphere;
mod floating_origin;
mod view_target;
mod world_persist;
mod telemetry;
mod jetpack_transit;
mod flight_celestials;
mod world_travel;
mod solar_system_clock;
mod ghibli_atmosphere;
mod debug;
mod flight_beacon;
mod game_ui;
mod hud;
mod input;
mod inventory_persist;
mod launch_pad;
mod machines;
mod lunar_surface;
mod materials;
mod moon_world;
mod earth_world;
mod planet_surface_mesh;
mod planet_world;
mod solar_system;
mod platform;
mod render;
mod rocket_flight;
mod scene;
mod space_world;
mod space_travel;
mod sim;
mod solar_lighting;
mod targeting;
mod ui_editor;
#[cfg(debug_assertions)]
mod asset_dev;
mod ui_theme;
mod ui;
mod deposit_spawn;
mod surface_placement;
mod surface_patch;

use bevy::prelude::*;
use bevy::winit::WinitSettings;
#[cfg(not(target_os = "android"))]
use bevy::render::{
    settings::{Backends, RenderCreation, WgpuSettings, WgpuSettingsPriority},
    RenderPlugin,
};
use ha_shared::Ps1Retro;
use assets::AssetFactoryPlugin;
use camera::FollowCameraPlugin;
use celestial_cleanup::CelestialCleanupPlugin;
use floating_origin::FloatingOriginPlugin;
use view_target::ViewTargetPlugin;
use world_persist::WorldPersistPlugin;
use telemetry::TelemetryPlugin;
use jetpack_transit::JetpackTransitPlugin;
use ghibli_atmosphere::GhibliAtmospherePlugin;
use debug::{apply_android_frame_cap, install_crash_hook, CrashReportPlugin, CpuSpanPlugin, PerfDebugPlugin};
use flight_celestials::FlightCelestialsPlugin;
use world_travel::WorldTravelPlugin;
use solar_system_clock::SolarSystemClockPlugin;
use earth_world::EarthWorldPlugin;
use surface_patch::SurfacePatchPlugin;
use hud::PixelHudPlugin;
use input::{InputPlugin, LookInteractPlugin};
use materials::ToonMaterialPlugin;
use launch_pad::LaunchPadPlugin;
use machines::MachinesPlugin;
use moon_world::MoonWorldPlugin;
use planet_world::PlanetWorldPlugin;
use rocket_flight::RocketFlightPlugin;
use space_travel::SpaceTravelPlugin;
use space_world::SpaceWorldPlugin;
use inventory_persist::InventoryPersistPlugin;
use platform::PlatformPlugin;
use render::render_scale::RenderScalePlugin;
use render::particles::ParticlesPlugin;
use scene::ScenePlugin;
use sky_clock::SkyClockPlugin;
use sim::SimPlugin;
use solar_lighting::SolarLightingPlugin;
use targeting::TargetingPlugin;
use ui::IconFactoryPlugin;
use ui_editor::UiEditorPlugin;

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
    }

    let mut binding = App::new();
    let mut app = binding
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
                WinitSettings::game()
            }
        })
        .add_plugins((
            PlatformPlugin,
            FloatingOriginPlugin,
            ViewTargetPlugin,
            WorldPersistPlugin,
            SolarSystemClockPlugin,
            SkyClockPlugin,
            SolarLightingPlugin,
            AssetFactoryPlugin,
            IconFactoryPlugin,
            ToonMaterialPlugin,
            SimPlugin,
            EarthWorldPlugin,
            SurfacePatchPlugin,
            ScenePlugin,
        ))
        .add_plugins((
            RenderScalePlugin,
            InventoryPersistPlugin,
            MachinesPlugin,
            LaunchPadPlugin,
            WorldTravelPlugin,
            InputPlugin,
            TargetingPlugin,
            LookInteractPlugin,
        ))
        .add_plugins((
            GhibliAtmospherePlugin,
            RocketFlightPlugin,
            FlightCelestialsPlugin,
            MoonWorldPlugin,
            PlanetWorldPlugin,
            CelestialCleanupPlugin,
            SpaceWorldPlugin,
            SpaceTravelPlugin,
            JetpackTransitPlugin,
        ))
        .add_plugins((
            FollowCameraPlugin,
            TelemetryPlugin,
            PixelHudPlugin,
            ParticlesPlugin,
            PerfDebugPlugin,
            CpuSpanPlugin,
            CrashReportPlugin,
            UiEditorPlugin,
        ));
    #[cfg(debug_assertions)]
    app.add_plugins(asset_dev::AssetDevPlugin);
    app.add_systems(Startup, apply_android_frame_cap).run();
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
