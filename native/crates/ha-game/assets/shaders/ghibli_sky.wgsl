#import bevy_pbr::{
    mesh_functions,
    forward_io::{Vertex, VertexOutput},
    view_transformations::position_world_to_clip,
}

struct GhibliSkyUniforms {
    zenith: vec4<f32>,
    horizon: vec4<f32>,
    warm: vec4<f32>,
    /// x = night, y = neon dusk band, z = sun visibility, w = time (twinkle)
    paint: vec4<f32>,
    /// x = cloud cover, y = rain, z = moon phase, w = storm
    weather: vec4<f32>,
    sun_dir: vec4<f32>,
}

@group(2) @binding(0)
var<uniform> sky: GhibliSkyUniforms;

fn hash21(p: vec2<f32>) -> f32 {
    var p3 = fract(vec3<f32>(p.x, p.y, p.x) * 0.1031);
    p3 += dot(p3, p3.yzx + 33.33);
    return fract((p3.x + p3.y) * p3.z);
}

fn hash31(p: vec3<f32>) -> f32 {
    var q = fract(p * 0.1031);
    q += dot(q, q.yzx + 33.33);
    return fract((q.x + q.y) * q.z);
}

fn noise3(p: vec3<f32>) -> f32 {
    let i = floor(p);
    let f = fract(p);
    let u = f * f * (3.0 - 2.0 * f);
    let a = hash31(i);
    let b = hash31(i + vec3<f32>(1.0, 0.0, 0.0));
    let c = hash31(i + vec3<f32>(0.0, 1.0, 0.0));
    let d = hash31(i + vec3<f32>(1.0, 1.0, 0.0));
    let e = hash31(i + vec3<f32>(0.0, 0.0, 1.0));
    let f2 = hash31(i + vec3<f32>(1.0, 0.0, 1.0));
    let g = hash31(i + vec3<f32>(0.0, 1.0, 1.0));
    let h = hash31(i + vec3<f32>(1.0, 1.0, 1.0));
    return mix(
        mix(mix(a, b, u.x), mix(c, d, u.x), u.y),
        mix(mix(e, f2, u.x), mix(g, h, u.x), u.y),
        u.z,
    );
}

fn fbm3(p: vec3<f32>, octaves: i32) -> f32 {
    var v = 0.0;
    var a = 0.5;
    var pp = p;
    for (var i = 0; i < octaves; i++) {
        v += a * noise3(pp);
        pp = pp * 2.02 + vec3<f32>(1.7, 2.3, 0.9);
        a *= 0.5;
    }
    return v;
}

@vertex
fn vertex(vertex_no_morph: Vertex) -> VertexOutput {
    var out: VertexOutput;
    var vertex = vertex_no_morph;
    var world_from_local = mesh_functions::get_world_from_local(vertex_no_morph.instance_index);
#ifdef VERTEX_POSITIONS
    var world_position = mesh_functions::mesh_position_local_to_world(
        world_from_local,
        vec4<f32>(vertex.position, 1.0),
    );
    out.world_position = world_position;
    out.position = position_world_to_clip(world_position.xyz);
#endif
    return out;
}

@fragment
fn fragment(in: VertexOutput) -> @location(0) vec4<f32> {
    let dir = normalize(in.world_position.xyz);
    let elev = clamp(dir.y * 0.5 + 0.5, 0.0, 1.0);

    let zenith = sky.zenith.rgb;
    let horizon = sky.horizon.rgb;
    let warm = sky.warm.rgb;
    let night = sky.paint.x;
    let neon = sky.paint.y;
    let sun_vis = sky.paint.z;
    let time = sky.paint.w;
    let cloud_cover = sky.weather.x;
    let rain = sky.weather.y;
    let moon_phase = sky.weather.z;
    let storm = sky.weather.w;

    // Smooth multi-band gradient (less flat than hard cel steps).
    let t1 = smoothstep(0.0, 0.22, elev);
    let t2 = smoothstep(0.22, 0.55, elev);
    let t3 = smoothstep(0.55, 1.0, elev);
    var col = mix(horizon, mix(horizon, zenith, t2), t1);
    col = mix(col, zenith, t3 * 0.85);
    col = mix(col, warm, (1.0 - elev) * (1.0 - night) * 0.38);

    // Atmospheric depth haze toward horizon.
    let haze = exp(-abs(dir.y) * 5.5) * (0.12 + cloud_cover * 0.18);
    col = mix(col, horizon * 1.08, haze);

    // Procedural sky clouds — FULL quality only. The "lite" tier still ran a
    // 2-octave fbm3 (~16 hash31 + mixes) per pixel: a fullscreen noise pass at
    // 2400×1080 on Mali — measured as the frame-time floor on the Redmi
    // (looking at pure sky cost ~66 ms with game CPU at 0.1 ms).
    let quality = sky.sun_dir.w;
    if quality > 0.5 {
        let octaves = 4;
        let wind = vec3<f32>(time * 0.018, time * 0.004, time * 0.014);
        let cloud_p = dir * 3.2 + wind;
        let n = fbm3(cloud_p, octaves);
        let n2 = fbm3(cloud_p * 1.7 + vec3<f32>(4.1, 1.2, 2.8), octaves);
        let cloud_noise = n * 0.65 + n2 * 0.35;
        let thresh = 0.52 - cloud_cover * 0.38;
        let cloud_mask = smoothstep(thresh, thresh + 0.22, cloud_noise);
        let cloud_bright = mix(vec3<f32>(0.92, 0.95, 1.0), vec3<f32>(0.55, 0.58, 0.62), storm);
        col = mix(col, cloud_bright, cloud_mask * cloud_cover * (0.35 + (1.0 - elev) * 0.45));
    }

    // Sun disc.
    let sun_dir = normalize(sky.sun_dir.xyz);
    let sun_dot = dot(dir, sun_dir);
    let sun_core = smoothstep(0.9992, 0.9999, sun_dot);
    let sun_halo = smoothstep(0.992, 0.9992, sun_dot);
    col += warm * sun_core * 2.8 * sun_vis * (1.0 - storm * 0.7);
    col += vec3<f32>(1.0, 0.92, 0.78) * sun_halo * 0.55 * sun_vis * (1.0 - storm * 0.5);

    // Moon — opposite sun, phase masks crescent.
    if night > 0.15 {
        let moon_dir = normalize(vec3<f32>(-sun_dir.x * 0.65, abs(sun_dir.y) * 0.25 + 0.72, -sun_dir.z * 0.65));
        let moon_dot = dot(dir, moon_dir);
        let disc = smoothstep(0.9983, 0.9993, moon_dot);
        let phase_offset = (moon_phase - 0.5) * 0.35;
        let phase_mask = smoothstep(-0.02 + phase_offset, 0.08 + phase_offset, dot(dir, vec3<f32>(1.0, 0.0, 0.0)));
        let moon_glow = disc * phase_mask * smoothstep(0.2, 0.55, night);
        let full_boost = smoothstep(0.42, 0.58, moon_phase) * 1.35;
        col += vec3<f32>(0.88, 0.92, 1.0) * moon_glow * (0.55 + full_boost * 0.45);
    }

    // Blood Dragon neon horizon at dusk.
    let horizon_mask = exp(-abs(dir.y) * 9.0) * (1.0 - elev * 0.6);
    col += vec3<f32>(1.0, 0.28, 0.62) * horizon_mask * neon * 0.55 * (1.0 - storm);
    col += vec3<f32>(0.2, 0.85, 1.0) * horizon_mask * neon * 0.22 * (1.0 - storm);

    // Stars — shader + parallax shells (sparse on mobile).
    if night > 0.08 {
        let star_scale = select(90.0, 130.0, quality > 0.5);
        let uv = vec2<f32>(atan2(dir.z, dir.x), dir.y);
        let cell = floor(uv * star_scale);
        let h = hash21(cell);
        let twinkle = 0.6 + 0.4 * sin(time * 2.2 + h * 50.0);
        let star_vis = (1.0 - cloud_cover * 0.85) * (1.0 - rain * 0.6);
        let star_thresh = select(0.996, 0.993, quality > 0.5);
        let star = step(star_thresh, h) * twinkle * smoothstep(0.08, 0.55, night) * star_vis;
        col += vec3<f32>(0.92, 0.96, 1.0) * star;
        let pin_thresh = select(0.9992, 0.9985, quality > 0.5);
        let pin = step(pin_thresh, h) * twinkle * smoothstep(0.25, 0.7, night) * star_vis;
        col += vec3<f32>(1.0, 0.98, 0.9) * pin * 0.8;
    }

    // Rain / storm mood.
    col *= 1.0 - rain * 0.32 - storm * 0.28;
    col = mix(col, col * vec3<f32>(0.55, 0.58, 0.68), storm * 0.35);

    col = mix(col, col * vec3<f32>(0.45, 0.52, 0.72), night * 0.35);
    col = floor(col * 8.0 + vec3<f32>(0.015)) / 8.0;
    return vec4<f32>(col, 1.0);
}
