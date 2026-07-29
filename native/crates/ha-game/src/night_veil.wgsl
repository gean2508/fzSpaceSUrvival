// Velo nocturno con agujeros: la α cae cerca de cada luz → se ve el albedo del mundo.
//
// Tres cosas hacen que el borde sea SUAVE y no un disco recortado:
//   1. caída gaussiana renormalizada (llega a 0 exacto en d = r, sin escalón),
//   2. unión multiplicativa entre luces (dos focos se funden, no se pisan),
//   3. dither de ±1 LSB: sin él, un degradé de α 0.88→0 sobre 8 bits deja
//      anillos concéntricos que en el móvil se leen como "el borde de la luz".
#import bevy_sprite::mesh2d_vertex_output::VertexOutput

const MAX_VEIL_LIGHTS: u32 = 24u;

/// Dureza de la gaussiana. Bajo = pozo ancho y muy difuso.
const FALLOFF_K: f32 = 3.2;
/// exp(-FALLOFF_K): se resta y renormaliza para que f(r) == 0 sin corte visible.
const FALLOFF_EDGE: f32 = 0.040762;

struct NightVeilMaterial {
    // rgba del velo (a = opacidad base de noche)
    color: vec4<f32>,
    // count + padding (16-byte align antes de los arrays)
    count: u32,
    _p0: u32,
    _p1: u32,
    _p2: u32,
    // xy = world pos (m), z = radio (m), w = fuerza 0..1
    lights: array<vec4<f32>, 24>,
    // rgb = color de esa luz (para teñir el velo que queda alrededor)
    tints: array<vec4<f32>, 24>,
}

@group(2) @binding(0)
var<uniform> material: NightVeilMaterial;

/// Ruido hash barato por píxel (Dave Hoskins). Solo se usa para el dither.
fn hash12(p: vec2<f32>) -> f32 {
    var p3 = fract(vec3<f32>(p.x, p.y, p.x) * 0.1031);
    p3 += dot(p3, vec3<f32>(p3.y, p3.z, p3.x) + 33.33);
    return fract((p3.x + p3.y) * p3.z);
}

@fragment
fn fragment(mesh: VertexOutput) -> @location(0) vec4<f32> {
    let p = mesh.world_position.xy;
    let n = min(material.count, MAX_VEIL_LIGHTS);

    var cover = 1.0; // 1 = velo entero, 0 = mundo a la vista
    var tint = vec3<f32>(0.0);
    var tint_w = 0.0;

    for (var i = 0u; i < n; i = i + 1u) {
        let l = material.lights[i];
        let r = max(l.z, 0.05);
        let x = distance(p, l.xy) / r;
        if x >= 1.0 {
            continue;
        }
        let g = exp(-FALLOFF_K * x * x);
        let f = max(g - FALLOFF_EDGE, 0.0) / (1.0 - FALLOFF_EDGE);
        let s = f * clamp(l.w, 0.0, 1.0);
        // Unión suave: N luces débiles suman sin producir un escalón donde
        // una "gana" sobre otra (que es lo que hacía `min`).
        cover *= 1.0 - s;
        tint += material.tints[i].rgb * s;
        tint_w += s;
    }

    // El velo que queda cerca de un foco toma SU color: la transición pasa por
    // el tono cálido de la lámpara antes de caer a la noche, en vez de saltar
    // de "iluminado" a "negro" en un par de píxeles.
    //
    // El coeficiente es CHICO. Este término pinta color sólido sobre el mundo:
    // subido (probamos 0.28) el pozo se vuelve un disco lechoso del color de la
    // luz que tapa el suelo, en vez de dejarlo ver. Quien ilumina de verdad es
    // `cover` (destapa el albedo) — esto es sólo el aire teñido de la caída.
    var rgb = material.color.rgb;
    if tint_w > 0.001 {
        rgb += (tint / tint_w) * clamp(tint_w, 0.0, 1.0) * 0.07;
    }

    let dither = (hash12(mesh.position.xy) - 0.5) * (1.6 / 255.0);
    let a = clamp(material.color.a * cover + dither, 0.0, 1.0);
    return vec4<f32>(rgb, a);
}
