//! **Los planetas, pintados.** Cada cuerpo del sistema tiene su textura de disco propia, generada una vez al entrar
//! al juego (96×96, ~10 ms): continentes, nubes y casquetes en la Tierra; cráteres y mares en la Luna; bandas y
//! tormentas en los gigantes; fisuras en Europa; volcanes en Ío. Antes el disco era un círculo de UN color hasta que
//! se acercaba lo suficiente como para hornear el terreno real (ver `universe::bake_body_textures`), y entonces
//! salía una plancha verde lisa.
//!
//! Se pinta sobre la ESFERA (cada píxel del disco es un punto de la esfera con su normal), no sobre el plano: el
//! ruido es 3D, así que no hay costuras ni estiramiento en los polos, y el oscurecimiento del limbo sale solo.
//! Estilo del juego: pocos tonos por canal (posterizado) y borde escalonado, como el resto del pixel.

use bevy::asset::RenderAssetUsages;
use bevy::image::ImageSampler;
use bevy::prelude::*;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};
use ha_shared::CelestialBody;

const N: usize = 96;

fn hash(ix: i32, iy: i32, iz: i32, seed: u32) -> f32 {
    let mut h = (ix as u32).wrapping_mul(0x8da6_b343) ^ (iy as u32).wrapping_mul(0xd816_3841) ^ (iz as u32).wrapping_mul(0xcb1a_b31f) ^ seed.wrapping_mul(0x9e37_79b1);
    h ^= h >> 15;
    h = h.wrapping_mul(0x2c1b_3c6d);
    h ^= h >> 12;
    h = h.wrapping_mul(0x297a_2d39);
    h ^= h >> 15;
    (h >> 8) as f32 / 16_777_216.0
}

fn vnoise(p: [f32; 3], seed: u32) -> f32 {
    let (x0, y0, z0) = (p[0].floor(), p[1].floor(), p[2].floor());
    let (fx, fy, fz) = (p[0] - x0, p[1] - y0, p[2] - z0);
    let s = |t: f32| t * t * (3.0 - 2.0 * t);
    let (u, v, w) = (s(fx), s(fy), s(fz));
    let (xi, yi, zi) = (x0 as i32, y0 as i32, z0 as i32);
    let mut acc = 0.0;
    for dz in 0..2 {
        for dy in 0..2 {
            for dx in 0..2 {
                let h = hash(xi + dx, yi + dy, zi + dz, seed);
                let wx = if dx == 1 { u } else { 1.0 - u };
                let wy = if dy == 1 { v } else { 1.0 - v };
                let wz = if dz == 1 { w } else { 1.0 - w };
                acc += h * wx * wy * wz;
            }
        }
    }
    acc
}

fn fbm(p: [f32; 3], octavas: u32, seed: u32) -> f32 {
    let (mut a, mut f, mut suma, mut norm) = (0.5, 1.0, 0.0, 0.0);
    for o in 0..octavas {
        suma += a * vnoise([p[0] * f, p[1] * f, p[2] * f], seed.wrapping_add(o * 101));
        norm += a;
        a *= 0.5;
        f *= 2.03;
    }
    suma / norm
}

fn mezcla(a: [f32; 3], b: [f32; 3], t: f32) -> [f32; 3] {
    let t = t.clamp(0.0, 1.0);
    [a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t, a[2] + (b[2] - a[2]) * t]
}

fn rgb(r: u8, g: u8, b: u8) -> [f32; 3] {
    [r as f32 / 255.0, g as f32 / 255.0, b as f32 / 255.0]
}

/// Cráteres sobre la esfera: (centro unitario, radio angular). Determinista por semilla.
fn crateres(n: usize, semilla: u32, rmin: f32, rmax: f32) -> Vec<([f32; 3], f32)> {
    (0..n)
        .map(|i| {
            let a = hash(i as i32, 1, 7, semilla) * std::f32::consts::TAU;
            let z = hash(i as i32, 2, 7, semilla) * 2.0 - 1.0;
            let rr = (1.0 - z * z).sqrt();
            let r = rmin + (rmax - rmin) * hash(i as i32, 3, 7, semilla).powi(2);
            ([rr * a.cos(), z, rr * a.sin()], r)
        })
        .collect()
}

/// Cuánto «cráter» hay en `p` (−1 = fondo oscuro, +1 = borde claro, 0 = nada).
fn relieve_crateres(p: [f32; 3], cs: &[([f32; 3], f32)]) -> f32 {
    let mut m = 0.0f32;
    for (c, r) in cs {
        let d = (p[0] * c[0] + p[1] * c[1] + p[2] * c[2]).clamp(-1.0, 1.0).acos();
        let k = d / r;
        if k < 0.8 {
            m = m.min(-0.6 * (1.0 - k));
        } else if k < 1.15 {
            m = m.max(0.7 * (1.0 - (k - 0.95).abs() / 0.2));
        }
    }
    m
}

fn tierra(p: [f32; 3]) -> [f32; 3] {
    let h = fbm([p[0] * 2.3 + 3.0, p[1] * 2.3, p[2] * 2.3], 5, 11);
    let lat = p[1].abs();
    let hielo = lat > 0.86 + 0.05 * fbm([p[0] * 6.0, p[1] * 6.0, p[2] * 6.0], 3, 5);
    let mut c = if h > 0.52 {
        // tierra: pradera, bosque, montaña
        let alt = ((h - 0.52) / 0.3).clamp(0.0, 1.0);
        let seca = fbm([p[0] * 3.0 + 9.0, p[1] * 3.0, p[2] * 3.0], 3, 21);
        let base = mezcla(rgb(70, 140, 68), rgb(196, 170, 100), (seca - 0.5) * 2.2 + (1.0 - lat) * 0.0);
        mezcla(base, rgb(120, 112, 100), (alt - 0.55).max(0.0) * 2.2)
    } else {
        let prof = ((0.52 - h) / 0.3).clamp(0.0, 1.0);
        mezcla(rgb(54, 150, 196), rgb(22, 52, 128), prof.powf(0.7))
    };
    if hielo {
        c = rgb(236, 244, 250);
    }
    // nubes: vetas largas
    let n = fbm([p[0] * 3.4 + 40.0, p[1] * 7.0, p[2] * 3.4], 4, 33);
    let nube = ((n - 0.56) * 4.5).clamp(0.0, 0.88);
    mezcla(c, rgb(250, 252, 255), nube)
}

fn rocoso(p: [f32; 3], base: [f32; 3], mares: [f32; 3], semilla: u32, ncrat: usize) -> [f32; 3] {
    let m = fbm([p[0] * 1.8 + 5.0, p[1] * 1.8, p[2] * 1.8], 4, semilla);
    let mut c = mezcla(base, mares, ((m - 0.5) * 3.0).clamp(0.0, 1.0));
    let grano = 0.88 + 0.24 * fbm([p[0] * 7.5, p[1] * 7.5, p[2] * 7.5], 3, semilla + 3);
    c = [c[0] * grano, c[1] * grano, c[2] * grano];
    let cs = crateres(ncrat, semilla, 0.06, 0.22);
    let k = relieve_crateres(p, &cs);
    c = if k < 0.0 {
        mezcla(c, [c[0] * 0.55, c[1] * 0.55, c[2] * 0.6], -k)
    } else {
        mezcla(c, [c[0] * 1.16 + 0.02, c[1] * 1.16 + 0.02, c[2] * 1.16 + 0.02], k)
    };
    c
}

fn marte(p: [f32; 3]) -> [f32; 3] {
    let mut c = rocoso(p, rgb(188, 98, 62), rgb(112, 58, 44), 71, 12);
    if p[1].abs() > 0.9 + 0.04 * fbm([p[0] * 7.0, p[1] * 7.0, p[2] * 7.0], 3, 9) {
        c = rgb(240, 236, 232);
    }
    c
}

fn bandas(p: [f32; 3], colores: &[[f32; 3]], turbulencia: f32, frecuencia: f32, semilla: u32) -> [f32; 3] {
    let t = fbm([p[0] * 2.5, p[1] * 5.0, p[2] * 2.5], 4, semilla) - 0.5;
    let v = (p[1] * frecuencia + t * turbulencia).sin() * 0.5 + 0.5;
    let n = colores.len() as f32 - 1.0;
    let x = v * n;
    let i = (x.floor() as usize).min(colores.len() - 2);
    mezcla(colores[i], colores[i + 1], x - i as f32)
}

fn europa(p: [f32; 3]) -> [f32; 3] {
    let base = mezcla(rgb(230, 226, 214), rgb(206, 214, 226), fbm([p[0] * 2.0, p[1] * 2.0, p[2] * 2.0], 3, 3));
    // grietas: crestas del ruido (|n − 0.5| chico)
    let g = (fbm([p[0] * 4.5 + 11.0, p[1] * 4.5, p[2] * 4.5], 4, 17) - 0.5).abs();
    let grieta = (1.0 - g * 28.0).clamp(0.0, 1.0);
    mezcla(base, rgb(150, 98, 70), grieta * 0.85)
}

fn io(p: [f32; 3]) -> [f32; 3] {
    let base = mezcla(rgb(228, 206, 90), rgb(214, 150, 54), fbm([p[0] * 2.4, p[1] * 2.4, p[2] * 2.4], 4, 8));
    let v = fbm([p[0] * 5.0 + 20.0, p[1] * 5.0, p[2] * 5.0], 3, 41);
    let volcan = ((v - 0.66) * 7.0).clamp(0.0, 1.0);
    mezcla(base, rgb(60, 40, 36), volcan)
}

fn color_de(body: CelestialBody, p: [f32; 3]) -> [f32; 3] {
    use CelestialBody::*;
    match body {
        Earth => tierra(p),
        Moon => rocoso(p, rgb(176, 174, 172), rgb(104, 104, 112), 5, 26),
        Mercury => rocoso(p, rgb(150, 142, 136), rgb(108, 102, 100), 17, 34),
        Phobos => rocoso(p, rgb(124, 108, 98), rgb(80, 70, 66), 29, 18),
        Deimos => rocoso(p, rgb(136, 122, 108), rgb(92, 82, 74), 43, 12),
        Mars => marte(p),
        Venus => bandas(p, &[rgb(244, 224, 170), rgb(226, 188, 120), rgb(250, 238, 200), rgb(214, 170, 104)], 6.0, 7.0, 13),
        Titan => bandas(p, &[rgb(214, 156, 66), rgb(236, 184, 92), rgb(196, 134, 56)], 3.0, 5.0, 19),
        Io => io(p),
        Europa => europa(p),
        Uranus => bandas(p, &[rgb(170, 226, 232), rgb(138, 210, 222), rgb(190, 238, 240)], 1.2, 6.0, 23),
        Neptune => {
            let mut c = bandas(p, &[rgb(54, 96, 214), rgb(40, 70, 176), rgb(88, 132, 236)], 2.4, 7.0, 31);
            // la Gran Mancha Oscura
            let d = ((p[0] - 0.35).powi(2) * 3.0 + (p[1] + 0.25).powi(2) * 9.0 + (p[2] - 0.85).powi(2) * 1.0).sqrt();
            if d < 0.3 {
                c = mezcla(c, rgb(24, 40, 120), (0.3 - d) * 5.0);
            }
            c
        }
        Sun => rgb(255, 224, 120),
    }
}

/// La textura de disco de `body`: RGBA, borde transparente, limbo oscurecido y posterizado.
pub fn textura_de_cuerpo(body: CelestialBody) -> Image {
    let mut px = vec![0u8; N * N * 4];
    for y in 0..N {
        for x in 0..N {
            let nx = (x as f32 + 0.5) / N as f32 * 2.0 - 1.0;
            let ny = -((y as f32 + 0.5) / N as f32 * 2.0 - 1.0);
            let r2 = nx * nx + ny * ny;
            if r2 > 1.0 {
                continue;
            }
            let z = (1.0 - r2).sqrt();
            let p = [nx, ny, z];
            let mut c = color_de(body, p);
            // limbo: la esfera se oscurece hacia el borde (luz de frente, la dirección real la pone `BodyNightSide`)
            let luz = 0.58 + 0.42 * z.powf(0.55);
            c = [c[0] * luz, c[1] * luz, c[2] * luz];
            // posterizado: once niveles por canal, como el resto del pixel del juego
            let q = |v: f32| ((v.clamp(0.0, 1.0) * 11.0).round() / 11.0 * 255.0) as u8;
            let o = (y * N + x) * 4;
            px[o] = q(c[0]);
            px[o + 1] = q(c[1]);
            px[o + 2] = q(c[2]);
            px[o + 3] = 255;
        }
    }
    let mut img = Image::new(
        Extent3d { width: N as u32, height: N as u32, depth_or_array_layers: 1 },
        TextureDimension::D2,
        px,
        TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::RENDER_WORLD | RenderAssetUsages::MAIN_WORLD,
    );
    img.sampler = ImageSampler::nearest();
    img
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Vuelca los discos a `/tmp/planetas.rgba` (12 cuerpos de 96×96 en fila) para mirarlos con `scripts/lamina-planetas.py`.
    #[test]
    #[ignore]
    fn volcar_planetas() {
        let mut todo = Vec::new();
        for &b in CelestialBody::all().iter().filter(|b| **b != CelestialBody::Sun) {
            todo.extend_from_slice(&textura_de_cuerpo(b).data);
        }
        std::fs::write("/tmp/planetas.rgba", todo).unwrap();
    }

    /// Cada cuerpo tiene un disco con relieve de color de verdad (no un círculo de un solo tono) y borde transparente.
    #[test]
    fn cada_planeta_tiene_textura_con_variedad() {
        for &b in CelestialBody::all().iter() {
            if b == CelestialBody::Sun {
                continue;
            }
            let img = textura_de_cuerpo(b);
            let d = &img.data;
            let opacos: Vec<&[u8]> = d.chunks(4).filter(|p| p[3] == 255).collect();
            assert!(opacos.len() > N * N / 2, "{b:?}: el disco casi no tiene píxeles");
            assert_eq!(d[3], 0, "{b:?}: la esquina tiene que ser transparente");
            let mut colores = std::collections::BTreeSet::new();
            for p in &opacos {
                colores.insert((p[0], p[1], p[2]));
            }
            assert!(colores.len() >= 8, "{b:?}: sólo {} colores, es un círculo plano", colores.len());
        }
    }
}
