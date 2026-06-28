#import bevy_pbr::forward_io::VertexOutput

struct StarfieldSettings {
    time: f32,
    density: f32,
    night: f32,
    _pad: f32,
}

@group(2) @binding(0) var<uniform> settings: StarfieldSettings;

fn hash21(p: vec2<f32>) -> f32 {
    var p3 = fract(vec3<f32>(p.x, p.y, p.x) * 0.1031);
    p3 += dot(p3, p3.yzx + 33.33);
    return fract((p3.x + p3.y) * p3.z);
}

@fragment
fn fragment(input: VertexOutput) -> @location(0) vec4<f32> {
    let dir = normalize(input.world_position);
    let uv = vec2<f32>(atan2(dir.z, dir.x), dir.y);
    let scale = 110.0 * settings.density;
    let cell = floor(uv * scale);
    let h = hash21(cell);
    let twinkle = 0.55 + 0.45 * sin(settings.time * 2.4 + h * 40.0);
    let threshold = 0.992 - settings.density * 0.004;
    let star = step(threshold, h) * twinkle * settings.night;
    let color = vec3<f32>(0.92, 0.95, 1.0) * star;
    return vec4<f32>(color, star * 0.85);
}
