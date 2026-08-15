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

/// **Cara de acantilado, orientada.**
///
/// `caras` dice por qué lados de la celda se CAE (N, E, S, W en ese orden), que
/// es lo que `world2d` deduce del relieve. Importa porque una pared de terraza es
/// una banda de un tile y el precipicio puede estar de cualquier lado: dibujar
/// siempre la misma cara frontal hacía que el borde norte de una meseta —el que
/// desde arriba se ve como una cornisa que se aleja— se viera igual que el borde
/// sur, que es el que se te viene encima.
///
/// Lo que hace que se lea como pared y no como suelo es siempre lo mismo: la
/// **meseta** conserva el color del terreno, el **canto** que da al vacío se
/// ilumina, y de ahí para abajo cae la cara oscura con sus estratos. La cara sólo
/// se dibuja hacia el SUR (y una franja fina hacia los costados), porque en 3/4
/// es la única que el jugador ve de frente.
fn cliff(suelo: [f32; 3], base: [f32; 3], salt: u32, caras: [bool; 4]) -> Image {
    let [n, e, s, w] = caras;
    let mut c = Canvas::new();
    // **La meseta se pinta del color del SUELO de arriba, no del de la pared.**
    // Es media celda de terraza que sigue siendo terreno pisable, y pintarla del
    // marrón de la roca dejaba una banda oliva atravesando el prado — se ve en
    // `agua-2-cascada.png` de la corrida vieja, un rectángulo de otro color
    // sobre el pasto.
    for y in 0..PX {
        for x in 0..PX {
            let g = grain(x, y, salt ^ 0x3E) * 0.12 - 0.06;
            c.set(x, y, shade(suelo, 1.0 + g, 255));
        }
    }
    // Dónde empieza la cara: si se cae al sur, ocupa casi toda la celda; si no,
    // la meseta llega hasta abajo y sólo hay canto en los costados.
    let alto_cara = if s { PX - 8 } else { 0 };
    let y0 = PX - alto_cara;
    for y in y0..PX {
        // Gradiente vertical: la cara se oscurece hacia el pie.
        let t = (y - y0) as f32 / alto_cara.max(1) as f32;
        let mut k = 0.86 - t * 0.34;
        // Estratos: bandas horizontales de espesor irregular.
        k *= 0.94 + grain(0, y / 5, salt ^ 0xB1) * 0.13;
        for x in 0..PX {
            let g = grain(x, y, salt) * 0.14 - 0.07;
            c.set(x, y, shade(base, k + g, 255));
        }
    }
    // El canto: la línea iluminada donde la meseta se asoma al vacío. Es el
    // detalle que hace que un desnivel de 2,5 m se lea a simple vista.
    if s {
        for y in y0.saturating_sub(3)..y0 {
            for x in 0..PX {
                let k = 1.24 - (y0 - y) as f32 * 0.08 + grain(x, y, salt ^ 0x5E) * 0.10;
                c.set(x, y, shade(suelo, k, 255));
            }
        }
        // Sombra del pie: asienta la pared sobre el suelo de abajo.
        for y in (PX - 3)..PX {
            for x in 0..PX {
                c.set(x, y, shade(base, 0.42, 255));
            }
        }
    }
    // Los costados: una franja de cara vista casi de perfil. Estrecha a
    // propósito — de canto, una pared no ocupa media celda.
    for (lado, izquierda) in [(w, true), (e, false)] {
        if !lado {
            continue;
        }
        for y in 0..PX {
            for i in 0..6 {
                let x = if izquierda { i } else { PX - 1 - i };
                let k = 0.70 - i as f32 * 0.04 + grain(x, y, salt ^ 0xC3) * 0.10;
                c.set(x, y, shade(base, k, 255));
            }
        }
    }
    // El borde norte da a la terraza de ARRIBA: no hay cara que ver, sólo la
    // sombra que esa terraza proyecta sobre ésta.
    if n {
        for y in 0..3 {
            for x in 0..PX {
                c.set(x, y, shade(suelo, 0.55, 255));
            }
        }
    }
    c.into_image()
}

/// Las formas de autotile que `world2d` puede pedir, con los lados por los que
/// se cae en cada una. El borde "mira" hacia el terreno más bajo, así que el
/// nombre de la forma ya dice dónde va la cara.
const FORMAS_CLIFF: &[(&str, [bool; 4])] = &[
    // (forma, [se cae al N, al E, al S, al W])
    ("center", [false, false, true, false]),
    ("edge_n", [true, false, false, false]),
    ("edge_e", [false, true, false, false]),
    ("edge_s", [false, false, true, false]),
    ("edge_w", [false, false, false, true]),
    ("corner_ne", [true, true, false, false]),
    ("corner_nw", [true, false, false, true]),
    ("corner_se", [false, true, true, false]),
    ("corner_sw", [false, false, true, true]),
    ("inner_ne", [false, false, true, false]),
    ("inner_nw", [false, false, true, false]),
    ("inner_se", [false, true, true, false]),
    ("inner_sw", [false, false, true, true]),
];

/// **Bordillo: el desnivel que SÍ se trepa.**
///
/// Es la misma cara que el acantilado pero baja y clara — un escalón de prado, no
/// un farallón. La diferencia tiene que verse **antes** de caminar hacia él: si
/// un desnivel que se salta se dibujara igual que un muro que hay que rodear, la
/// única forma de saber cuál es cuál sería chocarse (ver `climbable_edge`).
fn ledge(suelo: [f32; 3], base: [f32; 3], salt: u32, caras: [bool; 4]) -> Image {
    let [_, e, s, w] = caras;
    let mut c = Canvas::new();
    for y in 0..PX {
        for x in 0..PX {
            let g = grain(x, y, salt ^ 0x3E) * 0.12 - 0.06;
            c.set(x, y, shade(suelo, 1.0 + g, 255));
        }
    }
    if s {
        // Un canto de nueve píxeles: se ve el escalón, no una pared.
        for y in (PX - 9)..PX {
            let t = (y - (PX - 9)) as f32 / 9.0;
            for x in 0..PX {
                let g = grain(x, y, salt) * 0.10 - 0.05;
                c.set(x, y, shade(base, 1.16 - t * 0.50 + g, 255));
            }
        }
    }
    for (lado, izquierda) in [(w, true), (e, false)] {
        if !lado {
            continue;
        }
        for y in 0..PX {
            for i in 0..3 {
                let x = if izquierda { i } else { PX - 1 - i };
                c.set(x, y, shade(base, 0.78, 255));
            }
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
/// **La cerca del respaldo, ORIENTADA.**
///
/// Dibujaba los travesaños siempre en horizontal, así que los lados este y oeste
/// de una parcela —que son tramos verticales— salían con una valla acostada. Es
/// el bug que reportó el usuario, y arreglarlo sólo en el arte del pack no
/// alcanza: un slot sin curar (el seto) sigue cayendo acá.
///
/// `horizontal` decide el eje. Los travesaños corren a lo largo del tramo y los
/// postes lo cruzan, que es lo que hace que una cerca se lea como cerca; visto de
/// canto (el tramo vertical) el pack dibuja menos postes y más juntos, y acá se
/// imita achicando el paso.
fn fence_eje(base: [f32; 3], salt: u32, horizontal: bool) -> Image {
    let mut c = Canvas::new(); // arranca transparente
    // Travesaños: dos líneas paralelas al tramo.
    for t in [10usize, 20] {
        for u in 0..PX {
            let k = 0.95 + grain(u, t, salt) * 0.12;
            let (x, y, w, h) = if horizontal { (u, t, 1, 3) } else { (t, u, 3, 1) };
            c.rect(x, y, w, h, shade(base, k, 255));
        }
    }
    // Postes: cruzan el tramo, cada 8 px.
    let paso = if horizontal { 8 } else { 6 };
    for p0 in (2..PX).step_by(paso) {
        for u in 4..PX - 2 {
            let k = 1.05 + grain(p0, u, salt ^ 0x77) * 0.10;
            let (x, y, w, h) = if horizontal { (p0, u, 3, 1) } else { (u, p0, 1, 3) };
            c.rect(x, y, w, h, shade(base, k, 255));
        }
        // Punta más clara: el remate del poste, del lado por el que se ve.
        let (x, y, w, h) = if horizontal { (p0, 4, 3, 2) } else { (4, p0, 2, 3) };
        c.rect(x, y, w, h, shade(base, 1.25, 255));
    }
    c.into_image()
}

/// Registra los respaldos en los slots que hayan quedado SIN arte real.
/// **La senda peatonal**: las cebras, paralelas al tránsito que cruzan.
///
/// `vertical` = las barras corren norte-sur, que es como se pintan sobre una
/// calle norte-sur. Cuatro barras de cinco píxeles con tres de aire es la
/// proporción de una senda real y a 32 px se sigue leyendo desde el aire.
fn zebra(base: [f32; 3], salt: u32) -> Image {
    let vertical = salt & 1 == 0;
    let mut c = Canvas::new();
    for i in 0..4 {
        let p = 2 + i * 8;
        // Un poco de desgaste por barra: una senda recién pintada se ve de
        // plástico, y el asfalto de al lado tiene grano.
        let k = 0.88 + grain(p, i * 7, salt) * 0.12;
        let col = shade(base, k, 235);
        if vertical {
            c.rect(p, 1, 5, PX - 2, col);
        } else {
            c.rect(1, p, PX - 2, 5, col);
        }
    }
    c.into_image()
}

/// **Los materiales de relieve, cada uno con sus trece formas.**
///
/// El acantilado y el bordillo son lo único que se dibuja ORIENTADO: son bandas
/// de un tile cuyo sentido lo da el terreno, no la celda. El resto de los
/// respaldos son una imagen y basta.
/// (clave, color del SUELO de arriba, color de la CARA, ¿es bordillo?). Los dos
/// colores son distintos a propósito: la meseta tiene que confundirse con el
/// terreno que la rodea y la cara tiene que despegarse de él.
/// **Las cercas que pueden quedarse sin pixel, y su color de material.**
///
/// `import-cercas.py` cura las tres primeras del pack; están igual acá porque el
/// respaldo tiene que existir para el día que alguien borre una asignación desde
/// HA Studio. El seto todavía no tiene familia curada: el pack lo dibuja como
/// autotile de ÁREA (`Grass_Fenced_1`, pasto ya cercado), que es otra cosa que una
/// línea, y hasta que se importe como corresponde se dibuja acá.
const CERCAS: &[(&str, [f32; 3])] = &[
    ("fence/madera", [0.52, 0.36, 0.20]),
    ("fence/alambre", [0.60, 0.62, 0.64]),
    ("fence/piedra", [0.55, 0.54, 0.52]),
    ("fence/seto", [0.30, 0.46, 0.26]),
];

/// Las formas de cerca que el respaldo sabe dibujar, y en qué eje va cada una.
///
/// Son las mínimas de [`ha_shared::cercas::cadena`]: los dos tramos y el poste
/// suelto. Los codos caen al tramo de su eje dominante — un codo dibujado de
/// tramo se nota poco; un tramo dibujado en el eje equivocado se nota siempre.
const FORMAS_CERCA: &[(&str, bool)] = &[("ew", true), ("ns", false), ("solo", false)];

const RELIEVE: &[(&str, [f32; 3], [f32; 3], bool)] = &[
    ("cliff/roca", [0.55, 0.52, 0.48], [0.46, 0.42, 0.38], false),
    ("cliff/pasto", [0.36, 0.52, 0.28], [0.42, 0.34, 0.24], false),
    ("cliff/arena", [0.86, 0.78, 0.56], [0.70, 0.58, 0.38], false),
    ("cliff/nieve", [0.92, 0.95, 0.98], [0.70, 0.77, 0.86], false),
    ("cliff/regolito", [0.62, 0.62, 0.64], [0.48, 0.48, 0.50], false),
    ("cliff/basalto", [0.34, 0.33, 0.36], [0.22, 0.21, 0.24], false),
    // El bordillo se TREPA, y hay que poder verlo antes de caminar hacia él:
    // mismo suelo, cara baja y clara. Ver `world2d::terrain_art_key`.
    ("ledge/roca", [0.55, 0.52, 0.48], [0.66, 0.62, 0.57], true),
    ("ledge/pasto", [0.36, 0.52, 0.28], [0.52, 0.58, 0.34], true),
    ("ledge/arena", [0.86, 0.78, 0.56], [0.92, 0.84, 0.62], true),
    ("ledge/nieve", [0.92, 0.95, 0.98], [0.99, 1.00, 1.00], true),
    ("ledge/regolito", [0.62, 0.62, 0.64], [0.72, 0.72, 0.74], true),
    ("ledge/basalto", [0.34, 0.33, 0.36], [0.44, 0.43, 0.46], true),
];

fn generate_fallbacks(mut art: ResMut<ArtLibrary>, mut images: ResMut<Assets<Image>>) {
    // (clave, color base, generador). Los colores son los del material, no los del
    // tile: un acantilado de arena es arena oscurecida, no gris.
    let rock = [0.46, 0.42, 0.38];
    let entries: &[(&str, [f32; 3], fn([f32; 3], u32) -> Image)] = &[
        ("slope/roca", rock, slope),
        ("slope/pasto", [0.44, 0.58, 0.32], slope),
        ("slope/arena", [0.76, 0.67, 0.46], slope),
        ("slope/nieve", [0.84, 0.88, 0.93], slope),
        // La senda no tiene arte recortado del pack todavía: hasta que alguien se
        // lo asigne desde el editor, se pinta acá. El `salt` par/impar es el que
        // decide si las barras van en vertical o en horizontal (ver `zebra`).
        ("road/senda_ns", [0.93, 0.93, 0.90], zebra),
        ("road/senda_ew", [0.93, 0.93, 0.90], zebra),
    ];

    let mut made = 0;
    // El relieve primero: una hoja por FORMA, para que la cara mire hacia donde
    // de verdad se cae.
    for (i, (key, suelo, base, bordillo)) in RELIEVE.iter().enumerate() {
        if art.has_art(key) {
            continue; // ya lo curaste: el arte real manda siempre
        }
        for (forma, caras) in FORMAS_CLIFF {
            let salt = 0x9E37_0000 ^ ((i as u32) << 8) ^ forma.len() as u32;
            let img = if *bordillo {
                ledge(*suelo, *base, salt, *caras)
            } else {
                cliff(*suelo, *base, salt, *caras)
            };
            let handle = images.add(img);
            art.set_procedural_pose(key, forma, handle, PX as u32);
        }
        made += 1;
    }
    // **LAS CERCAS, UNA HOJA POR EJE.** Mismo trato que el relieve de arriba: el
    // respaldo no puede ser una sola imagen porque la orientación del tramo es
    // justo lo que hay que respetar. Las poses se llaman como las que publica
    // `scripts/import-cercas.py` y pide `ha_shared::cercas::cadena`, así que un
    // slot con arte real y uno sin curar se piden exactamente igual.
    for (i, (key, base)) in CERCAS.iter().enumerate() {
        if art.has_art(key) {
            continue; // ya lo curaste: el arte real manda siempre
        }
        for (forma, horizontal) in FORMAS_CERCA {
            let salt = 0x5ce0_0000 ^ ((i as u32) << 8) ^ forma.len() as u32;
            let handle = images.add(fence_eje(*base, salt, *horizontal));
            art.set_procedural_pose(key, forma, handle, PX as u32);
        }
        made += 1;
    }
    for (i, (key, base, gen)) in entries.iter().enumerate() {
        if art.has_art(key) {
            continue; // ya lo curaste: el arte real manda siempre
        }
        // La sal lleva la paridad de la clave, no la del índice de la tabla: la
        // senda norte-sur tiene que salir vertical aunque alguien reordene la
        // lista de arriba.
        let par = if key.ends_with("_ns") { 0 } else { 1 };
        let handle = images.add(gen(*base, (0x9E37_0000 ^ (i as u32) << 1) | par));
        art.set_procedural(key, handle);
        made += 1;
    }
    if made > 0 {
        eprintln!("ha-art: {made} respaldos dibujados por código (slots sin arte asignada)");
    }
}
