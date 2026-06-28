use bevy::asset::load_internal_asset;
use bevy::prelude::*;
use bevy::render::render_resource::{AsBindGroup, Face, Shader, ShaderRef, ShaderType};
use ha_shared::palette::{HybridLights, Palette};
use ha_shared::Ps1Retro;

/// Embedded at compile time — Android APK asset I/O cannot resolve `shaders/toon.wgsl`.
const TOON_SHADER_HANDLE: Handle<Shader> = Handle::weak_from_u128(0x8a3f_2c91_b704_4e6d);
const STARS_SHADER_HANDLE: Handle<Shader> = Handle::weak_from_u128(0x7c2e_91a4_b603_5f8e);
const PLANET_SHADER_HANDLE: Handle<Shader> = Handle::weak_from_u128(0x6b1d_82c5_a704_6e9f);
const GHIBLI_SKY_SHADER_HANDLE: Handle<Shader> = Handle::weak_from_u128(0x5a0c_73d6_9805_7fae);

pub struct ToonMaterialPlugin;

impl Plugin for ToonMaterialPlugin {
    fn build(&self, app: &mut App) {
        load_internal_asset!(
            app,
            TOON_SHADER_HANDLE,
            "../assets/shaders/toon.wgsl",
            Shader::from_wgsl
        );
        load_internal_asset!(
            app,
            STARS_SHADER_HANDLE,
            "../assets/shaders/stars.wgsl",
            Shader::from_wgsl
        );
        load_internal_asset!(
            app,
            PLANET_SHADER_HANDLE,
            "../assets/shaders/planet_surface.wgsl",
            Shader::from_wgsl
        );
        load_internal_asset!(
            app,
            GHIBLI_SKY_SHADER_HANDLE,
            "../assets/shaders/ghibli_sky.wgsl",
            Shader::from_wgsl
        );
        app.add_plugins(MaterialPlugin::<ToonMaterial>::default())
            .add_plugins(MaterialPlugin::<StarfieldMaterial>::default())
            .add_plugins(MaterialPlugin::<PlanetSurfaceMaterial>::default())
            .add_plugins(MaterialPlugin::<GhibliSkyMaterial>::default());
    }
}

/// 3-band cel material — mirrors `makeCelMaterial` in client-gfx.
#[derive(Asset, TypePath, AsBindGroup, Debug, Clone)]
pub struct ToonMaterial {
    #[uniform(0)]
    pub base_color: LinearRgba,
    #[uniform(0)]
    pub bands: f32,
    #[uniform(0)]
    pub ambient: f32,
    #[uniform(0)]
    pub emissive_glow: Vec4,
    /// Direction toward sun (xyz, w unused) — matches scene `DirectionalLight` euler.
    #[uniform(0)]
    pub sun_dir: Vec4,
    /// Curved-world vertex bend: origin.xz + curvature * enabled (w).
    #[uniform(0)]
    pub world_curve: Vec4,
    /// Sky hemi rgb + night factor (0 day → 1 deep night).
    #[uniform(0)]
    pub atmos_sky: Vec4,
    /// Ground hemi rgb + sun strength multiplier.
    #[uniform(0)]
    pub atmos_ground: Vec4,
    /// Local warm light world xyz + falloff radius (metres).
    #[uniform(0)]
    pub local_light: Vec4,
    /// Local light rgb + intensity.
    #[uniform(0)]
    pub light_color: Vec4,
    /// x=snap res, y=fog_start, z=fog_end, w=dither strength.
    #[uniform(0)]
    pub ps1_settings: Vec4,
    #[uniform(0)]
    pub ps1_fog_color: Vec4,
    #[uniform(0)]
    pub camera_pos: Vec4,
}

impl Default for ToonMaterial {
    fn default() -> Self {
        ps1_base_material(LinearRgba::new(
            Ps1Retro::GRASS[0],
            Ps1Retro::GRASS[1],
            Ps1Retro::GRASS[2],
            1.0,
        ))
    }
}

pub fn ps1_settings_default() -> Vec4 {
    Vec4::new(
        Ps1Retro::SNAP_RES * 0.75,
        Ps1Retro::FOG_START,
        Ps1Retro::FOG_END,
        Ps1Retro::DITHER * 0.32,
    )
}

/// Block terrain — no vertex snap (keeps cuboid edges stable on mobile GPUs).
pub fn ps1_settings_flat_terrain() -> Vec4 {
    Vec4::new(0.0, Ps1Retro::FOG_START, Ps1Retro::FOG_END, Ps1Retro::DITHER * 0.1)
}

/// Extended fog for orbital / lunar vistas (planets sit hundreds of metres away).
pub fn ps1_settings_celestial() -> Vec4 {
    Vec4::new(Ps1Retro::SNAP_RES, 600.0, 2_400.0, Ps1Retro::DITHER * 0.65)
}

pub fn ps1_settings_for_region(region: &crate::scene::RegionId) -> Vec4 {
    use crate::scene::RegionId;
    match region {
        RegionId::Space | RegionId::Luna | RegionId::Transit => ps1_settings_celestial(),
        _ => ps1_settings_default(),
    }
}

pub fn ps1_fog_default() -> Vec4 {
    Vec4::new(
        Ps1Retro::FOG[0],
        Ps1Retro::FOG[1],
        Ps1Retro::FOG[2],
        1.0,
    )
}

/// Shader fog tint + distance band matched to zone atmosphere (ToonMaterial path).
pub fn ps1_fog_for_zone(zone: &str, region: &crate::scene::RegionId) -> (Vec4, f32, f32) {
    use crate::scene::RegionId;
    if matches!(
        region,
        RegionId::Luna | RegionId::Space | RegionId::Transit | RegionId::Planet(_)
    ) {
        return (Vec4::new(0.0, 0.0, 0.0, 1.0), 900.0, 2_400.0);
    }
    match zone {
        "cave" => (
            Vec4::new(0.05, 0.04, 0.06, 1.0),
            4.0,
            22.0,
        ),
        "mystic" => (
            Vec4::new(0.55, 0.35, 0.75, 1.0),
            18.0,
            72.0,
        ),
        "launch" | "meadow" => (
            Vec4::new(0.75, 0.85, 0.92, 1.0),
            Ps1Retro::FOG_START,
            Ps1Retro::FOG_END,
        ),
        _ => (ps1_fog_default(), Ps1Retro::FOG_START, Ps1Retro::FOG_END),
    }
}

/// Additive billboard halo for crafted lights — GLES-safe, no postprocess.
pub fn light_halo_material(color: Color) -> StandardMaterial {
    StandardMaterial {
        base_color: Color::srgba(0.0, 0.0, 0.0, 0.0),
        emissive: LinearRgba::from(color) * 2.5,
        alpha_mode: AlphaMode::Add,
        unlit: true,
        cull_mode: None,
        ..default()
    }
}

fn ps1_base_material(base: LinearRgba) -> ToonMaterial {
    ToonMaterial {
        base_color: base,
        bands: 4.0,
        ambient: 0.26,
        emissive_glow: Vec4::ZERO,
        sun_dir: Vec4::new(0.35, 0.88, 0.32, 0.0),
        world_curve: Vec4::new(0.0, 0.0, ha_shared::CURVED_WORLD_K, 0.0),
        atmos_sky: Vec4::new(
            Ps1Retro::SKY_ZENITH[0],
            Ps1Retro::SKY_ZENITH[1],
            Ps1Retro::SKY_ZENITH[2],
            0.0,
        ),
        atmos_ground: Vec4::new(
            Ps1Retro::SKY_GROUND[0],
            Ps1Retro::SKY_GROUND[1],
            Ps1Retro::SKY_GROUND[2],
            1.0,
        ),
        local_light: Vec4::ZERO,
        light_color: Vec4::ZERO,
        ps1_settings: ps1_settings_default(),
        ps1_fog_color: ps1_fog_default(),
        camera_pos: Vec4::ZERO,
    }
}

impl Material for ToonMaterial {
    fn vertex_shader() -> ShaderRef {
        ShaderRef::Handle(TOON_SHADER_HANDLE)
    }

    fn fragment_shader() -> ShaderRef {
        ShaderRef::Handle(TOON_SHADER_HANDLE)
    }
}

/// GLES-safe unlit surface — always visible without PBR lighting.
pub fn unlit_srgb(rgb: [f32; 3]) -> StandardMaterial {
    StandardMaterial {
        base_color: Color::srgb(rgb[0], rgb[1], rgb[2]),
        unlit: true,
        ..default()
    }
}

/// Cel-shaded voxel surface — flat terrain settings (no vertex snap on cuboids).
pub fn block_toon(rgb: [f32; 3]) -> ToonMaterial {
    let mut m = ps1_base_material(LinearRgba::new(rgb[0], rgb[1], rgb[2], 1.0));
    m.ps1_settings = ps1_settings_flat_terrain();
    m
}

/// GLES-safe block voxels — lit PBR + custom toon both fail on some Android GPUs.
pub fn block_std_material(rgb: [f32; 3]) -> StandardMaterial {
    unlit_srgb(rgb)
}

pub fn luna_regolith_std() -> StandardMaterial {
    StandardMaterial {
        base_color: Color::srgb(0.62, 0.63, 0.67),
        perceptual_roughness: 0.92,
        metallic: 0.0,
        unlit: false,
        ..default()
    }
}

/// Interior star shell — visible from inside inverted sphere.
pub fn luna_starfield_shell_std() -> StandardMaterial {
    StandardMaterial {
        base_color: Color::srgb(0.02, 0.025, 0.06),
        emissive: LinearRgba::new(0.04, 0.05, 0.12, 1.0),
        unlit: true,
        cull_mode: None,
        ..default()
    }
}

/// Tiny emissive points on the interior sky shell.
pub fn luna_star_point_std() -> StandardMaterial {
    StandardMaterial {
        base_color: Color::srgb(0.92, 0.95, 1.0),
        emissive: LinearRgba::new(0.85, 0.9, 1.0, 1.0),
        unlit: true,
        ..default()
    }
}

pub fn sky_dome_std() -> StandardMaterial {
    StandardMaterial {
        base_color: Color::srgb(
            Ps1Retro::SKY_ZENITH[0],
            Ps1Retro::SKY_ZENITH[1],
            Ps1Retro::SKY_ZENITH[2],
        ),
        unlit: true,
        cull_mode: Some(Face::Back),
        ..default()
    }
}

/// Painted equirectangular sky — GLES-safe (Android + desktop).
pub fn sky_dome_textured(texture: Handle<Image>) -> StandardMaterial {
    StandardMaterial {
        base_color_texture: Some(texture),
        base_color: Color::WHITE,
        unlit: true,
        cull_mode: Some(Face::Back),
        ..default()
    }
}

pub fn horizon_water_material() -> StandardMaterial {
    use ha_shared::palette::Palette;
    StandardMaterial {
        base_color: Color::srgba(Palette::WATER[0], Palette::WATER[1], Palette::WATER[2], 0.88),
        unlit: true,
        alpha_mode: AlphaMode::Blend,
        perceptual_roughness: 1.0,
        ..default()
    }
}

/// Inverted-hull outline shell color `#2b2f3a`.
#[derive(Component)]
pub struct OutlineShell;

pub fn outline_material() -> StandardMaterial {
    StandardMaterial {
        base_color: Color::srgb(Palette::BARK[0], Palette::BARK[1], Palette::BARK[2]),
        unlit: true,
        cull_mode: Some(Face::Front),
        ..default()
    }
}

pub fn hero_toon() -> ToonMaterial {
    ps1_base_material(LinearRgba::new(
        Palette::HERO[0],
        Palette::HERO[1],
        Palette::HERO[2],
        1.0,
    ))
}

pub fn meadow_toon() -> ToonMaterial {
    ps1_base_material(LinearRgba::new(
        Ps1Retro::GRASS[0],
        Ps1Retro::GRASS[1],
        Ps1Retro::GRASS[2],
        1.0,
    ))
}

pub fn earth_toon() -> ToonMaterial {
    ps1_base_material(LinearRgba::new(
        Ps1Retro::DIRT[0],
        Ps1Retro::DIRT[1],
        Ps1Retro::DIRT[2],
        1.0,
    ))
}

pub fn terrain_toon() -> ToonMaterial {
    ps1_base_material(LinearRgba::new(
        Ps1Retro::GRASS[0],
        Ps1Retro::GRASS[1],
        Ps1Retro::GRASS[2],
        1.0,
    ))
}

pub fn water_toon() -> ToonMaterial {
    ps1_base_material(LinearRgba::new(
        Ps1Retro::WATER[0],
        Ps1Retro::WATER[1],
        Ps1Retro::WATER[2],
        0.92,
    ))
}

pub fn sand_toon() -> ToonMaterial {
    ps1_base_material(LinearRgba::new(
        Ps1Retro::SAND[0],
        Ps1Retro::SAND[1],
        Ps1Retro::SAND[2],
        1.0,
    ))
}

pub fn stone_toon() -> ToonMaterial {
    ps1_base_material(LinearRgba::new(
        Ps1Retro::STONE[0],
        Ps1Retro::STONE[1],
        Ps1Retro::STONE[2],
        1.0,
    ))
}

pub fn snow_toon() -> ToonMaterial {
    ps1_base_material(LinearRgba::new(
        Ps1Retro::SNOW[0],
        Ps1Retro::SNOW[1],
        Ps1Retro::SNOW[2],
        1.0,
    ))
}

pub fn firefly_toon() -> ToonMaterial {
    ToonMaterial {
        base_color: LinearRgba::new(0.65, 1.0, 0.72, 1.0),
        ambient: 0.5,
        // NUNCA mover esto a textura o bloom obligatorio: glow procedural por uniform.
        emissive_glow: Vec4::new(1.1, 2.6, 1.2, 1.0),
        ..default()
    }
}

/// Must match `stars.wgsl` `StarfieldSettings` (16-byte std140 block).
#[derive(Clone, Copy, Debug, Default, ShaderType)]
pub struct StarfieldSettings {
    pub time: f32,
    pub density: f32,
    pub night: f32,
    pub _pad: f32,
}

/// Inverted sky dome — procedural star hash (embedded WGSL for Android).
#[derive(Asset, TypePath, AsBindGroup, Debug, Clone)]
pub struct StarfieldMaterial {
    #[uniform(0)]
    pub settings: StarfieldSettings,
}

impl Material for StarfieldMaterial {
    fn fragment_shader() -> ShaderRef {
        ShaderRef::Handle(STARS_SHADER_HANDLE)
    }

    fn alpha_mode(&self) -> AlphaMode {
        AlphaMode::Blend
    }
}

pub fn starfield_material() -> StarfieldMaterial {
    StarfieldMaterial {
        settings: StarfieldSettings {
            time: 0.0,
            density: 1.0,
            night: 1.0,
            _pad: 0.0,
        },
    }
}

/// Cratered planet shell — flattened uniforms like `ToonMaterial` (Mali-safe layout).
#[derive(Asset, TypePath, AsBindGroup, Debug, Clone)]
pub struct PlanetSurfaceMaterial {
    #[uniform(0)]
    pub tint: LinearRgba,
    #[uniform(0)]
    pub crater_strength: f32,
    #[uniform(0)]
    pub body_kind: f32,
    #[uniform(0)]
    pub _pad1: Vec2,
}

impl Material for PlanetSurfaceMaterial {
    fn fragment_shader() -> ShaderRef {
        ShaderRef::Handle(PLANET_SHADER_HANDLE)
    }
}

pub fn planet_surface_material(tint: LinearRgba, crater_strength: f32) -> PlanetSurfaceMaterial {
    PlanetSurfaceMaterial {
        tint,
        crater_strength,
        body_kind: 0.0,
        _pad1: Vec2::ZERO,
    }
}

/// Earth as seen from orbit / Luna — procedural oceans + continents (body_kind=1).
pub fn earth_planet_material() -> PlanetSurfaceMaterial {
    PlanetSurfaceMaterial {
        tint: LinearRgba::new(1.0, 1.0, 1.0, 1.0),
        crater_strength: 0.0,
        body_kind: 1.0,
        _pad1: Vec2::ZERO,
    }
}

/// Grey cratered regolith shell.
pub fn moon_planet_material() -> PlanetSurfaceMaterial {
    PlanetSurfaceMaterial {
        tint: LinearRgba::new(0.42, 0.44, 0.48, 1.0),
        crater_strength: 0.85,
        body_kind: 0.0,
        _pad1: Vec2::ZERO,
    }
}

pub fn earth_globe_material() -> StandardMaterial {
    StandardMaterial {
        base_color: Color::srgb(0.55, 0.72, 0.95),
        unlit: true,
        emissive: LinearRgba::new(0.25, 0.35, 0.55, 1.0),
        ..default()
    }
}

/// Android-safe moon globe (custom planet WGSL fails on many GLES GPUs).
pub fn moon_globe_std() -> StandardMaterial {
    unlit_srgb([0.55, 0.57, 0.62])
}

/// Dark void shell when starfield shader is unavailable.
pub fn starfield_void_std() -> StandardMaterial {
    StandardMaterial {
        base_color: Color::srgb(0.02, 0.025, 0.06),
        emissive: LinearRgba::new(0.03, 0.04, 0.1, 1.0),
        unlit: true,
        cull_mode: None,
        ..default()
    }
}

/// Painted Ghibli + synthwave dusk sky dome (interior of large sphere).
#[derive(Asset, TypePath, AsBindGroup, Debug, Clone)]
pub struct GhibliSkyMaterial {
    #[uniform(0)]
    pub zenith: Vec4,
    #[uniform(0)]
    pub horizon: Vec4,
    #[uniform(0)]
    pub warm: Vec4,
    /// x = night factor, y = neon horizon band, z = sun visibility, w = time.
    #[uniform(0)]
    pub paint: Vec4,
    /// x = cloud cover, y = rain, z = moon phase (0..1), w = storm.
    #[uniform(0)]
    pub weather: Vec4,
    #[uniform(0)]
    pub sun_dir: Vec4,
}

impl Default for GhibliSkyMaterial {
    fn default() -> Self {
        Self {
            zenith: Vec4::new(
                Ps1Retro::SKY_ZENITH[0],
                Ps1Retro::SKY_ZENITH[1],
                Ps1Retro::SKY_ZENITH[2],
                0.0,
            ),
            horizon: Vec4::new(
                Ps1Retro::SKY_HORIZON[0],
                Ps1Retro::SKY_HORIZON[1],
                Ps1Retro::SKY_HORIZON[2],
                0.0,
            ),
            warm: Vec4::new(Ps1Retro::FOG[0], Ps1Retro::FOG[1], Ps1Retro::FOG[2], 0.0),
            paint: Vec4::new(0.0, 0.0, 1.0, 0.0),
            weather: Vec4::ZERO,
            sun_dir: Vec4::new(0.35, 0.88, 0.32, 0.0),
        }
    }
}

impl Material for GhibliSkyMaterial {
    fn vertex_shader() -> ShaderRef {
        ShaderRef::Handle(GHIBLI_SKY_SHADER_HANDLE)
    }

    fn fragment_shader() -> ShaderRef {
        ShaderRef::Handle(GHIBLI_SKY_SHADER_HANDLE)
    }

    fn alpha_mode(&self) -> AlphaMode {
        AlphaMode::Opaque
    }
}
