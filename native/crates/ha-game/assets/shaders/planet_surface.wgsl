#import bevy_pbr::forward_io::VertexOutput

struct PlanetMaterial {
    tint: vec4<f32>,
    crater_strength: f32,
    body_kind: f32,
    _pad1: vec2<f32>,
}

@group(2) @binding(0) var<uniform> material: PlanetMaterial;

fn hash3(p: vec3<f32>) -> f32 {
    var p3 = fract(p * 0.1031);
    p3 += dot(p3, p3.yzx + 33.33);
    return fract((p3.x + p3.y) * p3.z);
}

fn earth_surface(n: vec3<f32>) -> vec3<f32> {
    let land_h = hash3(n * vec3<f32>(4.0, 6.0, 4.0));
    let detail = hash3(n * 12.0);
    let ocean = vec3<f32>(0.06, 0.18, 0.52);
    let lowland = vec3<f32>(0.18, 0.42, 0.16);
    let highland = vec3<f32>(0.38, 0.32, 0.22);
    let ice = vec3<f32>(0.88, 0.92, 0.96);
    let is_land = smoothstep(0.46, 0.64, land_h);
    var rgb = mix(ocean, mix(lowland, highland, detail), is_land);
    if n.y > 0.72 {
        rgb = mix(rgb, ice, smoothstep(0.72, 0.9, n.y));
    }
    let shade = 0.62 + 0.38 * max(n.y, 0.0);
    rgb *= shade;
    let rim = pow(1.0 - abs(n.y), 2.5) * 0.18;
    rgb += vec3<f32>(0.15, 0.35, 0.75) * rim;
    return rgb;
}

fn moon_surface(n: vec3<f32>) -> vec3<f32> {
    let h = hash3(n * 14.0);
    let h2 = hash3(n * 28.0 + vec3<f32>(2.1, 4.3, 1.7));
    let mare = smoothstep(0.38, 0.62, hash3(n * 2.2));
    var base = mix(vec3<f32>(0.30, 0.31, 0.34), vec3<f32>(0.48, 0.49, 0.52), mare);
    // 3D crater bowls — angular distance to seeded centers on sphere.
    var disp = 0.0;
    for (var i = 0; i < 6; i++) {
        let fi = f32(i);
        let c = normalize(vec3<f32>(
            hash3(vec3<f32>(fi, 1.0, 2.0)) - 0.5,
            hash3(vec3<f32>(fi, 3.0, 4.0)),
            hash3(vec3<f32>(fi, 5.0, 6.0)) - 0.5,
        ));
        let ang = acos(clamp(dot(n, c), -1.0, 1.0));
        let r = 0.06 + hash3(vec3<f32>(fi, 7.0, 8.0)) * 0.12;
        if ang < r {
            let t = 1.0 - ang / r;
            disp -= t * t * (0.35 + hash3(vec3<f32>(fi, 9.0, 1.0)) * 0.4);
        }
    }
    base *= 1.0 + disp * 0.55;
    base += vec3<f32>(0.02) * h2;
    let shade = 0.55 + 0.45 * max(dot(n, normalize(vec3<f32>(0.35, 0.88, 0.32))), 0.0);
    return base * shade;
}

@fragment
fn fragment(input: VertexOutput) -> @location(0) vec4<f32> {
    let n = normalize(input.world_normal);

    if material.body_kind > 0.5 {
        return vec4<f32>(earth_surface(n), 1.0);
    }

    if material.crater_strength > 0.5 {
        return vec4<f32>(moon_surface(n), 1.0);
    }

    let h = hash3(n * 18.0);
    let crater = smoothstep(0.93, 0.97, h) * material.crater_strength;
    let shade = 0.72 + 0.28 * n.y;
    var rgb = material.tint.rgb * shade * (1.0 - crater * 0.55);
    rgb += vec3<f32>(0.04, 0.05, 0.08) * crater;
    return vec4<f32>(rgb, material.tint.a);
}
