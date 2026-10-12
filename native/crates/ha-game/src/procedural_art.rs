//! **Arte de respaldo dibujado por código.**
//!
//! Sin arte curado, el mundo dibujaba cada material como un **cuadrado de color
//! plano**. Para el pasto o la arena eso se disimula; para un acantilado no: una
//! pared de 2.5 m se veía exactamente igual que el suelo, un tono más oscuro, y
//! el relieve entero quedaba invisible hasta que alguien recortara los sprites.
//!
//! Este módulo genera esas texturas por código al arrancar. No pretenden competir
//! con el arte real —son siluetas legibles, no pixel art— pero cumplen las dos
//! cosas que importan:
//!
//! 1. **El mundo nunca se ve vacío ni roto.** Si un slot no está curado todavía,
//!    o si su PNG falla al cargar, se ve algo que se entiende.
//! 2. **El arte real las reemplaza sin tocar código.** Se registran bajo la MISMA
//!    clave lógica y sólo si el slot está vacío, así que en cuanto asignás el
//!    pixel en el editor, el fallback desaparece solo (`art/manifest.json`
//!    manda, `biblia.md` §8).
//!
//! Se dibuja a 32×32 px, la grilla del pack LimeZu, para que al mezclarse con
//! arte real no se note un cambio de resolución.

use bevy::asset::RenderAssetUsages;
use bevy::prelude::*;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};

use crate::art2d::ArtLibrary;

/// Lado de las texturas generadas, en píxeles (la grilla de LimeZu).
const PX: usize = 32;

pub struct ProceduralArtPlugin;

impl Plugin for ProceduralArtPlugin {
    fn build(&self, app: &mut App) {
        // Después de `load_art_manifest` (PreStartup): así vemos qué slots quedaron
        // sin arte real y sólo rellenamos ésos.
        app.add_systems(Startup, generate_fallbacks);
    }
}

/// Lienzo RGBA sencillo con las cuatro o cinco primitivas que hacen falta.
struct Canvas {
    px: Vec<u8>,
}

impl Canvas {
    fn new() -> Self {
        Self { px: vec![0; PX * PX * 4] }
    }

    fn set(&mut self, x: usize, y: usize, c: [u8; 4]) {
        if x >= PX || y >= PX {
            return;
        }
        let i = (y * PX + x) * 4;
        self.px[i..i + 4].copy_from_slice(&c);
    }

    fn fill(&mut self, c: [u8; 4]) {
        for y in 0..PX {
            for x in 0..PX {
                self.set(x, y, c);
            }
        }
    }

    fn rect(&mut self, x0: usize, y0: usize, w: usize, h: usize, c: [u8; 4]) {
        for y in y0..(y0 + h) {
            for x in x0..(x0 + w) {
                self.set(x, y, c);
            }
        }
    }

    fn into_image(self) -> Image {
        Image::new(
            Extent3d { width: PX as u32, height: PX as u32, depth_or_array_layers: 1 },
            TextureDimension::D2,
            self.px,
            TextureFormat::Rgba8UnormSrgb,
            RenderAssetUsages::RENDER_WORLD,
        )
    }
}

/// Ruido determinista por píxel, 0..1. No hace falta Perlin: la textura sólo
/// tiene que romper el color plano.
fn grain(x: usize, y: usize, salt: u32) -> f32 {
    let h = ha_shared::worldgen::hash_cell(salt, x as i64, y as i64);
    (h & 0xffff) as f32 / 65535.0
}

fn shade(c: [f32; 3], k: f32, a: u8) -> [u8; 4] {
    [
        ((c[0] * k).clamp(0.0, 1.0) * 255.0) as u8,
        ((c[1] * k).clamp(0.0, 1.0) * 255.0) as u8,
        ((c[2] * k).clamp(0.0, 1.0) * 255.0) as u8,
        a,
    ]
}

/// **Cara de acantilado.** Lo que la hace leerse como una pared y no como suelo:
/// una cornisa clara arriba (el canto que recibe la luz), estratos horizontales
/// que dan altura, y una base oscura donde la pared se hunde en la terraza de
/// abajo. Es el mismo truco que usa cualquier tileset de acantilado.
fn cliff(base: [f32; 3], salt: u32) -> Image {
    let mut c = Canvas::new();
    for y in 0..PX {
        // Gradiente vertical: la cara se oscurece hacia el pie.
        let t = y as f32 / PX as f32;
        let mut k = 1.02 - t * 0.42;
        // Estratos: bandas horizontales de espesor irregular.
        let band = (y / 5) as u32;
        k *= 0.94 + grain(0, band as usize, salt ^ 0xB1) * 0.13;
        for x in 0..PX {
            let g = grain(x, y, salt) * 0.14 - 0.07;
            c.set(x, y, shade(base, k + g, 255));
        }
    }
    // Cornisa iluminada arriba (3 px) — el canto de la terraza superior.
    for y in 0..3 {
        for x in 0..PX {
            let k = 1.30 - y as f32 * 0.10 + grain(x, y, salt ^ 0x5E) * 0.10;
            c.set(x, y, shade(base, k, 255));
        }
    }
    // Sombra del pie (4 px): es lo que asienta la pared sobre el suelo de abajo.
    for y in (PX - 4)..PX {
        for x in 0..PX {
            let k = 0.46 - (y - (PX - 4)) as f32 * 0.05;
            c.set(x, y, shade(base, k, 255));
        }
    }
    c.into_image()
}

/// **Rampa.** Tiene que leerse como "por acá SÍ se sube": más clara que la pared,
/// con galones diagonales que apuntan hacia arriba, como la señalización de una
/// pendiente.
fn slope(base: [f32; 3], salt: u32) -> Image {
    let mut c = Canvas::new();
    for y in 0..PX {
        for x in 0..PX {
            let t = y as f32 / PX as f32;
            let mut k = 1.12 - t * 0.20;
            // Galones: bandas diagonales cada 6 px.
            if (x + y) % 8 < 3 {
                k *= 1.10;
            }
            k += grain(x, y, salt) * 0.10 - 0.05;
            c.set(x, y, shade(base, k, 255));
        }
    }
    // Bordes laterales oscuros: encajan la rampa entre las dos paredes vecinas.
    for y in 0..PX {
        for x in [0, 1, PX - 2, PX - 1] {
            c.set(x, y, shade(base, 0.62, 255));
        }
    }
    c.into_image()
}

/// **Cerca.** Postes verticales y dos travesaños, con el fondo TRANSPARENTE: una
/// cerca no tapa el jardín que hay detrás, se ve a través.
fn fence(base: [f32; 3], salt: u32) -> Image {
    let mut c = Canvas::new(); // arranca transparente
    // Travesaños.
    for y in [10usize, 20] {
        for x in 0..PX {
            let k = 0.95 + grain(x, y, salt) * 0.12;
            c.rect(x, y, 1, 3, shade(base, k, 255));
        }
    }
    // Postes cada 8 px.
    for px0 in (2..PX).step_by(8) {
        for y in 4..PX - 2 {
            let k = 1.05 + grain(px0, y, salt ^ 0x77) * 0.10;
            c.rect(px0, y, 3, 1, shade(base, k, 255));
        }
        // Punta más clara.
        c.rect(px0, 4, 3, 2, shade(base, 1.25, 255));
    }
    c.into_image()
}

/// Registra los respaldos en los slots que hayan quedado SIN arte real.
fn generate_fallbacks(mut art: ResMut<ArtLibrary>, mut images: ResMut<Assets<Image>>) {
    // (clave, color base, generador). Los colores son los del material, no los del
    // tile: un acantilado de arena es arena oscurecida, no gris.
    let rock = [0.46, 0.42, 0.38];
    let entries: &[(&str, [f32; 3], fn([f32; 3], u32) -> Image)] = &[
        ("cliff/roca", rock, cliff),
        ("cliff/pasto", [0.40, 0.36, 0.26], cliff),
        ("cliff/arena", [0.72, 0.62, 0.42], cliff),
        ("cliff/nieve", [0.78, 0.83, 0.89], cliff),
        ("cliff/regolito", [0.55, 0.55, 0.57], cliff),
        ("cliff/basalto", [0.27, 0.26, 0.29], cliff),
        ("slope/roca", rock, slope),
        ("slope/pasto", [0.44, 0.58, 0.32], slope),
        ("slope/arena", [0.76, 0.67, 0.46], slope),
        ("slope/nieve", [0.84, 0.88, 0.93], slope),
        ("fence/madera", [0.52, 0.36, 0.20], fence),
        ("fence/alambre", [0.60, 0.62, 0.64], fence),
        ("fence/piedra", [0.55, 0.54, 0.52], fence),
    ];

    let mut made = 0;
    for (i, (key, base, gen)) in entries.iter().enumerate() {
        if art.has_art(key) {
            continue; // ya lo curaste: el arte real manda siempre
        }
        let handle = images.add(gen(*base, 0x9E37_0000 ^ i as u32));
        art.set_procedural(key, handle);
        made += 1;
    }
    if made > 0 {
        eprintln!("ha-art: {made} respaldos dibujados por código (slots sin arte asignada)");
    }
}
