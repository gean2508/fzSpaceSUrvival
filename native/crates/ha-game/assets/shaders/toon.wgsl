#import bevy_pbr::{
    mesh_functions,
    forward_io::{Vertex, VertexOutput},
    view_transformations::position_world_to_clip,
}

struct ToonMaterial {
    base_color: vec4<f32>,
    bands: f32,
    ambient: f32,
    emissive_glow: vec4<f32>,
    sun_dir: vec4<f32>,
    world_curve: vec4<f32>,
    atmos_sky: vec4<f32>,
    atmos_ground: vec4<f32>,
    local_light: vec4<f32>,
    light_color: vec4<f32>,
    ps1_settings: vec4<f32>,
    ps1_fog_color: vec4<f32>,
    camera_pos: vec4<f32>,
}

@group(2) @binding(0)
var<uniform> material: ToonMaterial;

// FLAGS — minimal stable shading base. These additive terms are view-dependent
// and popped on merged low-poly silhouettes at some camera angles. Flip back on
// when polishing atmosphere effects (dead code is eliminated at compile time).
const ENABLE_RIM_LIGHT: bool = false;
const ENABLE_BLOOM_APPROX: bool = false;

fn ps1_snap_clip(clip: vec4<f32>) -> vec4<f32> {
    let res = material.ps1_settings.x;
    if res < 4.0 {
        return clip;
    }
    let w = max(clip.w, 1e-6);
    var ndc = clip.xyz / w;
    let grid = res * 0.5;
    ndc = vec3<f32>(
        floor(ndc.x * grid) / grid,
        floor(ndc.y * grid) / grid,
        ndc.z,
    );
    return vec4<f32>(ndc * w, w);
}

// world_curve: x=origin.x, y=origin.z, z=curvature, w=enabled

@vertex
fn vertex(vertex_no_morph: Vertex) -> VertexOutput {
    var out: VertexOutput;
    var vertex = vertex_no_morph;
    var world_from_local = mesh_functions::get_world_from_local(vertex_no_morph.instance_index);

#ifdef VERTEX_NORMALS
    out.world_normal = mesh_functions::mesh_normal_local_to_world(
        vertex.normal,
        vertex_no_morph.instance_index,
    );
#endif

#ifdef VERTEX_POSITIONS
    var world_position = mesh_functions::mesh_position_local_to_world(
        world_from_local,
        vec4<f32>(vertex.position, 1.0),
    );
    let k = material.world_curve.z * material.world_curve.w;
    if k > 0.0 {
        let dx = world_position.x - material.world_curve.x;
        let dz = world_position.z - material.world_curve.y;
        world_position.y -= k * (dx * dx + dz * dz);
    }
    out.world_position = world_position;
    out.position = ps1_snap_clip(position_world_to_clip(world_position.xyz));
#endif

#ifdef VERTEX_UVS_A
    out.uv = vertex.uv;
#endif

#ifdef VERTEX_COLORS
    out.color = vertex.color;
#endif

    return out;
}

fn bayer4(x: u32, y: u32) -> f32 {
    let m = array<f32, 16>(
        0.0 / 16.0, 8.0 / 16.0, 2.0 / 16.0, 10.0 / 16.0,
        12.0 / 16.0, 4.0 / 16.0, 14.0 / 16.0, 6.0 / 16.0,
        3.0 / 16.0, 11.0 / 16.0, 1.0 / 16.0, 9.0 / 16.0,
        15.0 / 16.0, 7.0 / 16.0, 13.0 / 16.0, 5.0 / 16.0,
    );
    return m[y * 4u + x];
}

fn ps1_dither(col: vec3<f32>, frag_xy: vec2<f32>, strength: f32) -> vec3<f32> {
    if strength < 0.01 {
        return col;
    }
    let ix = u32(floor(frag_xy.x)) % 4u;
    let iy = u32(floor(frag_xy.y)) % 4u;
    let t = bayer4(ix, iy) - 0.5;
    let levels = 8.0;
    return floor(col * levels + t * strength) / levels;
}

// Crafted lights (lantern, torch) — quadratic falloff; solar stays infinite.
fn light_attenuation(dist: f32, radius: f32) -> f32 {
    let x = clamp(dist / max(radius, 0.001), 0.0, 1.0);
    let atten = 1.0 - x * x;
    return atten * atten;
}

// Edge halo — cheap substitute for volumetric scatter on GLES3.
fn rim_light(normal: vec3<f32>, view_dir: vec3<f32>, light_dir: vec3<f32>) -> f32 {
    let ndotv = max(dot(normal, view_dir), 0.0);
    let rim = 1.0 - ndotv;
    let rim_pow = rim * rim * rim;
    let ndotl = max(dot(normal, light_dir), 0.0);
    return rim_pow * ndotl * 0.35;
}

// Single-pass screen-space glow — no extra render targets (GLES-safe).
fn apply_bloom_approx(color: vec3<f32>, threshold: f32, strength: f32) -> vec3<f32> {
    let brightness = dot(color, vec3<f32>(0.2126, 0.7152, 0.0722));
    let bloom_amount = max(brightness - threshold, 0.0);
    let glow = color * bloom_amount * strength;
    return color + glow;
}

@fragment
fn fragment(in: VertexOutput) -> @location(0) vec4<f32> {
#ifdef VERTEX_NORMALS
    let n = normalize(in.world_normal);
#else
    let n = vec3<f32>(0.0, 1.0, 0.0);
#endif

    let light_dir = normalize(material.sun_dir.xyz);
    let ndotl = max(dot(n, light_dir), 0.0);

    let night = material.atmos_sky.w;
    let sky_amb = material.atmos_sky.xyz;
    let ground_amb = material.atmos_ground.xyz;
    let sun_strength = material.atmos_ground.w;

    let hemi = mix(ground_amb, sky_amb, clamp(n.y * 0.5 + 0.5, 0.0, 1.0));
    let cel_raw = ndotl * material.bands;
    let cel = (floor(cel_raw) + smoothstep(0.0, 0.38, fract(cel_raw))) / material.bands;
    let direct = mix(material.ambient * 0.42, 1.0, cel) * sun_strength;

    let cam = material.camera_pos.xyz;
    let view_dir = normalize(cam - in.world_position.xyz);

    var color = material.base_color.rgb * (hemi * 0.48 + direct * 0.92);
    if ENABLE_RIM_LIGHT {
        let solar_rim = rim_light(n, view_dir, light_dir);
        color += material.base_color.rgb * solar_rim * sun_strength * 0.28;
    }
    color = mix(color, color * vec3<f32>(0.55, 0.62, 0.82), night * 0.55);

#ifdef VERTEX_COLORS
    color *= mix(vec3<f32>(1.0), in.color.rgb, 0.88);
#endif

    let lp = material.local_light.xyz;
    let lr = material.local_light.w;
    if lr > 0.01 && material.light_color.a > 0.01 {
        let to_light = lp - in.world_position.xyz;
        let dist_l = length(to_light);
        let crafted_dir = to_light / max(dist_l, 0.001);
        let atten = light_attenuation(dist_l, lr);
        let crafted_ndotl = max(dot(n, crafted_dir), 0.0);
        let crafted_cel = floor(crafted_ndotl * 2.0) / 2.0;
        // Stable fill instead of the view-dependent rim while flag is off.
        var crafted_rim = 0.12;
        if ENABLE_RIM_LIGHT {
            crafted_rim = rim_light(n, view_dir, crafted_dir);
        }
        let crafted = material.light_color.rgb
            * material.light_color.a
            * atten
            * (crafted_cel * 0.62 + crafted_rim);
        color += crafted;
    }

    color += material.emissive_glow.rgb;

    let dist = distance(in.world_position.xyz, cam);
    let fog_t = smoothstep(material.ps1_settings.y, material.ps1_settings.z, dist);
    color = mix(color, material.ps1_fog_color.rgb, fog_t);

    let dither_amt = material.ps1_settings.w * material.ps1_settings.w * 0.85;
    color = ps1_dither(color, in.position.xy, dither_amt);

    // Bloom after cel + fog — crafted lights punch through navy mist.
    if ENABLE_BLOOM_APPROX {
        let in_cave_glow = material.atmos_sky.w > 0.9 && material.light_color.a > 1.0;
        let night_glow = material.atmos_sky.w > 0.45 && material.light_color.a > 0.01;
        var bloom_threshold = select(0.75, 0.6, in_cave_glow);
        bloom_threshold = select(bloom_threshold, 0.48, night_glow);
        var bloom_strength = select(0.4, 0.6, in_cave_glow);
        bloom_strength = select(bloom_strength, 0.82, night_glow);
        color = apply_bloom_approx(color, bloom_threshold, bloom_strength);
    }

    return vec4<f32>(color, material.base_color.a);
}
