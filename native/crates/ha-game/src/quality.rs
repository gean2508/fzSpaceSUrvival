//! Platform quality tier — GLES mobile vs desktop Vulkan/Metal.

use ha_shared::CelestialBody;

/// Chunk ring around the player (radius → (2r+1)² chunks).
#[inline]
pub fn chunk_load_radius() -> i32 {
    #[cfg(target_os = "android")]
    {
        2
    }
    #[cfg(not(target_os = "android"))]
    {
        4
    }
}

/// Visible ground patch half-extent (metres).
#[inline]
pub fn surface_patch_half() -> f32 {
    #[cfg(target_os = "android")]
    {
        120.0
    }
    #[cfg(not(target_os = "android"))]
    {
        150.0
    }
}

/// Terrain lattice step for the sliding surface patch.
#[inline]
pub fn surface_patch_step(body: CelestialBody) -> f32 {
    if body == CelestialBody::Moon {
        #[cfg(target_os = "android")]
        {
            return 2.8;
        }
        #[cfg(not(target_os = "android"))]
        {
            return 2.0;
        }
    }
    #[cfg(target_os = "android")]
    {
        4.0
    }
    #[cfg(not(target_os = "android"))]
    {
        2.5
    }
}

/// Ghibli sky shader tier — stored in `GhibliSkyMaterial.sun_dir.w` (0 = lite, 1 = full).
#[inline]
pub fn sky_shader_quality() -> f32 {
    #[cfg(target_os = "android")]
    {
        0.0
    }
    #[cfg(not(target_os = "android"))]
    {
        1.0
    }
}

/// Parallax star shells in the overworld sky stack.
/// FLAG: disabled — the parallax factor (0.014) pins the shell near the world
/// origin, valid on the old 192×192 island but not on the 480 m walkable
/// sphere: the player walks OUT of the shell and sees its far wall as huge
/// tilted translucent sheets in the sky (inverted winding + AlphaMode::Blend).
/// It also costs a full-screen blend pass even at day (night=0 → alpha 0).
/// Re-enable only with the shell re-centred on the player (like the dome),
/// or bake stars into the dome shader instead.
/// FLAG: the outer shell (radius 445) sits OUTSIDE the opaque Ghibli dome
/// (420) so it was never visible — pure cost. Re-add a second layer only
/// inside the dome radius if the parallax depth is wanted again.
pub const ENABLE_OVERWORLD_STAR_SHELL: bool = false;

#[inline]
pub fn star_shell_layers() -> &'static [(f32, f32, f32)] {
    if ENABLE_OVERWORLD_STAR_SHELL {
        &[(420.0 * 0.86, 0.014, 1.15)]
    } else {
        &[]
    }
}

#[inline]
pub fn enable_sun_shadows(disk_visibility: f32, in_cave: bool, in_flight: bool) -> bool {
    #[cfg(target_os = "android")]
    {
        let _ = (disk_visibility, in_cave, in_flight);
        false
    }
    #[cfg(not(target_os = "android"))]
    {
        disk_visibility > 0.15 && !in_cave && !in_flight
    }
}

#[inline]
pub fn paint_uniform_rescan_frames() -> u32 {
    #[cfg(target_os = "android")]
    {
        96
    }
    #[cfg(not(target_os = "android"))]
    {
        48
    }
}

#[inline]
pub fn max_light_halos() -> usize {
    #[cfg(target_os = "android")]
    {
        4
    }
    #[cfg(not(target_os = "android"))]
    {
        8
    }
}
