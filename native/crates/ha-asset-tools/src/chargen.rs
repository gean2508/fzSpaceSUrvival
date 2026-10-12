//! **Character Generator** — el generador por CAPAS de LimeZu, traído adentro de
//! nuestras herramientas.
//!
//! El pack (`PRUEBAS-CODIGOS/Character_Generator/`) trae hojas sueltas por capa
//! (cuerpos, ojos, ropa, pelo, accesorios) que hay que apilar **en un orden
//! exacto** para formar un personaje. Su `CHARACTER_GENERATOR.txt` dice que eso se
//! hace "abriendo Piskel y componiendo a mano". Eso acá no escala: necesitamos
//! decenas de aldeanos distintos y un creador de personaje jugable.
//!
//! Entonces el generador vive en el CÓDIGO:
//!
//! - [`Layers`] describe un personaje como 5 índices (cuerpo/ojos/ropa/pelo/
//!   accesorio) → un `u64` determinista lo genera. El aldeano `uid = 37` tiene
//!   siempre la misma cara, en esta máquina y en el teléfono.
//! - [`compose`] apila las capas en el orden del pack y devuelve la hoja.
//! - Se **hornea a un PNG por personaje** (media escala: celdas 16×32, que es el
//!   layout que ya usan `player/character` y el atlas del creador). El juego sigue
//!   cargando **UN PNG por slot** — la regla de `imglab-multipng-editor` intacta —
//!   y en el teléfono no se compone nada en runtime: cero costo.
//!
//! Uso (CLI):
//! ```text
//! ha-asset-tools chargen --root PRUEBAS-CODIGOS/Character_Generator \
//!                        --out native/crates/ha-game/assets/art \
//!                        --npcs 20 --seed 7 [--player] [--scale 2]
//! ```

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use image::{imageops, RgbaImage};

/// Orden de apilado que exige el pack (`CHARACTER_GENERATOR.txt`). Cambiarlo
/// dibuja el pelo debajo de la cabeza — no lo toques.
pub const LAYER_ORDER: [LayerKind; 5] = [
    LayerKind::Body,
    LayerKind::Eyes,
    LayerKind::Outfit,
    LayerKind::Hairstyle,
    LayerKind::Accessory,
];

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug)]
pub enum LayerKind {
    Body,
    Eyes,
    Outfit,
    Hairstyle,
    Accessory,
}

impl LayerKind {
    /// Carpeta del pack. Hay variante "kids" para los NIÑOS (§4.5 del plan de
    /// NPCs: los hijos se ven como hijos), con las mismas reglas de apilado.
    pub fn dir(self, kids: bool) -> &'static str {
        match (self, kids) {
            (LayerKind::Body, false) => "Bodies",
            (LayerKind::Body, true) => "Bodies_kids",
            (LayerKind::Eyes, false) => "Eyes",
            (LayerKind::Eyes, true) => "Eyes_kids",
            (LayerKind::Outfit, false) => "Outfits",
            (LayerKind::Outfit, true) => "Outfits_kids",
            (LayerKind::Hairstyle, false) => "Hairstyles",
            (LayerKind::Hairstyle, true) => "Hairstyles_kids",
            // Los accesorios son los mismos para chicos y grandes.
            (LayerKind::Accessory, _) => "Accessories",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            LayerKind::Body => "cuerpo",
            LayerKind::Eyes => "ojos",
            LayerKind::Outfit => "ropa",
            LayerKind::Hairstyle => "pelo",
            LayerKind::Accessory => "accesorio",
        }
    }
}

/// Un personaje = un índice por capa dentro del catálogo. `None` en accesorio =
/// sin accesorio (la mayoría de la gente no usa uno).
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct Layers {
    pub body: usize,
    pub eyes: usize,
    pub outfit: usize,
    pub hair: usize,
    /// `None` = sin accesorio.
    pub accessory: Option<usize>,
    pub kid: bool,
}

/// Catálogo de archivos por capa, leído del pack una sola vez.
pub struct Catalog {
    pub root: PathBuf,
    /// `(capa, kid)` → rutas ordenadas alfabéticamente (orden estable ⇒ los
    /// índices significan siempre lo mismo).
    files: BTreeMap<(LayerKind, bool), Vec<PathBuf>>,
    /// Tamaño de celda del pack a usar (`16x16`, `32x32`, `48x48`).
    pub cell: &'static str,
}

impl Catalog {
    /// Lee el pack. `cell` es la subcarpeta de resolución ("32x32" es la que casa
    /// con nuestro arte a media escala).
    pub fn load(root: impl Into<PathBuf>, cell: &'static str) -> Result<Self, String> {
        let root = root.into();
        if !root.is_dir() {
            return Err(format!("no existe el pack: {}", root.display()));
        }
        let mut files = BTreeMap::new();
        for kind in LAYER_ORDER {
            for kids in [false, true] {
                let dir = root.join(kind.dir(kids)).join(cell);
                let mut v: Vec<PathBuf> = std::fs::read_dir(&dir)
                    .map(|rd| {
                        rd.flatten()
                            .map(|e| e.path())
                            .filter(|p| p.extension().and_then(|e| e.to_str()) == Some("png"))
                            .collect()
                    })
                    .unwrap_or_default();
                v.sort();
                files.insert((kind, kids), v);
            }
        }
        if files.get(&(LayerKind::Body, false)).map(|v| v.is_empty()).unwrap_or(true) {
            return Err(format!(
                "el pack no tiene cuerpos en {}/Bodies/{cell} — ¿--root correcto?",
                root.display()
            ));
        }
        Ok(Self { root, files, cell })
    }

    pub fn list(&self, kind: LayerKind, kids: bool) -> &[PathBuf] {
        self.files.get(&(kind, kids)).map(|v| v.as_slice()).unwrap_or(&[])
    }

    pub fn count(&self, kind: LayerKind, kids: bool) -> usize {
        self.list(kind, kids).len()
    }

    /// Nombre legible de una opción (sin la resolución ni la extensión).
    pub fn option_label(&self, kind: LayerKind, kids: bool, idx: usize) -> String {
        self.list(kind, kids)
            .get(idx)
            .and_then(|p| p.file_stem())
            .map(|s| s.to_string_lossy().replace(&format!("_{}", self.cell), ""))
            .unwrap_or_else(|| "-".into())
    }

    /// Personaje determinista a partir de una semilla (uid del NPC, por ejemplo).
    /// Mismo número ⇒ mismo personaje, siempre y en todas las plataformas.
    ///
    /// El sorteo en sí vive en `ha-assets` porque el JUEGO lo repite para componer
    /// aldeanos y sus retratos en vivo: una sola implementación, sin paridad que
    /// mantener a mano.
    pub fn layers_from_seed(&self, seed: u64, kid: bool) -> Layers {
        let counts = ha_assets::chargen_index::LayerCounts {
            bodies: self.count(LayerKind::Body, kid),
            eyes: self.count(LayerKind::Eyes, kid),
            outfits: self.count(LayerKind::Outfit, kid),
            hair: self.count(LayerKind::Hairstyle, kid),
            accessories: self.count(LayerKind::Accessory, kid),
        };
        let look = ha_assets::chargen_index::look_from_seed(counts, seed);
        Layers {
            body: look.body as usize,
            eyes: look.eyes as usize,
            outfit: look.outfit as usize,
            hair: look.hair as usize,
            accessory: look.accessory.map(|a| a as usize),
            kid,
        }
    }

    fn path_of(&self, kind: LayerKind, l: &Layers) -> Option<&PathBuf> {
        let idx = match kind {
            LayerKind::Body => l.body,
            LayerKind::Eyes => l.eyes,
            LayerKind::Outfit => l.outfit,
            LayerKind::Hairstyle => l.hair,
            LayerKind::Accessory => l.accessory?,
        };
        self.list(kind, l.kid).get(idx)
    }
}

/// Ancho canónico de una hoja de personaje del pack, a escala completa (56
/// columnas de 32 px). Las hojas de CUERPO vienen más anchas (1854) que las de
/// ropa (1792) porque traen frames extra al final; si no se recorta, cada
/// personaje sale de un ancho distinto y **los rects de pose dejan de servir para
/// todos**. Recortamos a lo que comparten → toda hoja horneada queda idéntica a
/// `art/player/character.png` (896×656 a media escala) y una sola tanda de poses
/// en el editor vale para el jugador y para los 20 aldeanos.
const CANON_W: u32 = 1792;

/// Apila las capas y devuelve la hoja compuesta, recortada a [`CANON_W`].
/// `scale_div` la reduce después (2 = media escala, que es lo que usa el juego).
pub fn compose(cat: &Catalog, l: &Layers, scale_div: u32) -> Result<RgbaImage, String> {
    let mut out: Option<RgbaImage> = None;
    for kind in LAYER_ORDER {
        let Some(path) = cat.path_of(kind, l) else { continue };
        let img = image::open(path)
            .map_err(|e| format!("{}: {e}", path.display()))?
            .to_rgba8();
        match out.as_mut() {
            None => out = Some(img),
            Some(base) => {
                if base.dimensions() != img.dimensions() {
                    // Las hojas del pack comparten alto pero el ancho varía por
                    // capa (los accesorios traen menos animaciones). Se pega
                    // arriba-izquierda, que es como están alineadas.
                    imageops::overlay(base, &img, 0, 0);
                } else {
                    imageops::overlay(base, &img, 0, 0);
                }
            }
        }
    }
    let mut img = out.ok_or_else(|| "no pude componer: faltan capas".to_string())?;
    // Recorte al ancho canónico (ver `CANON_W`): todas las hojas iguales.
    if img.width() > CANON_W {
        let h = img.height();
        img = imageops::crop_imm(&img, 0, 0, CANON_W, h).to_image();
    }
    if scale_div <= 1 {
        return Ok(img);
    }
    let (w, h) = img.dimensions();
    // `Nearest`: es pixel art — cualquier filtro lo emborrona.
    Ok(imageops::resize(
        &img,
        (w / scale_div).max(1),
        (h / scale_div).max(1),
        imageops::FilterType::Nearest,
    ))
}

/// Celda de la hoja horneada, en píxeles. Es la MISMA que usa el creador de
/// personaje (`scene::CHAR_COLS/ROWS`) y `player/character` — y la misma que
/// `ha_assets::chargen_index::CELL`, que es quien manda.
///
/// **32×64 = la resolución nativa del pack.** Todo el arte del juego sale de las
/// carpetas `32x32`; no queda nada leído de `16x16`.
pub const CELL: (u32, u32) = (32, 64);

/// Registra en el `manifest.json` del juego las hojas recién horneadas: la grilla
/// (16×32) y un `rect` de 1 celda en (0,0) = **el frame de reposo mirando al
/// jugador**.
///
/// Sin esto el juego dibujaría la HOJA ENTERA dentro del quad del aldeano (un
/// mosaico de miniaturas), porque `ArtLibrary::sprite_or_color` usa la imagen
/// completa salvo que el slot declare un recorte. Las poses direccionales se
/// recortan después en el editor sobre esta misma grilla — y como todas las
/// hojas son idénticas (ver `CANON_W`), sirven para todos los personajes.
pub fn register_in_manifest(manifest_path: &Path, keys: &[(String, String)]) -> Result<(), String> {
    use ha_assets::{ArtManifest, CellRect};
    let mut manifest = match std::fs::read_to_string(manifest_path) {
        Ok(raw) => ArtManifest::from_json(&raw).map_err(|e| format!("{}: {e}", manifest_path.display()))?,
        Err(_) => ArtManifest::default(),
    };
    for (key, png) in keys {
        // `set_rect` conserva la entrada si el PNG no cambió; si el usuario ya le
        // había recortado poses a mano, esto sólo fija grilla + frame de reposo.
        let already_posed = manifest.poses(key).map(|p| !p.is_empty()).unwrap_or(false);
        if already_posed {
            continue; // no pisar el trabajo de slicing del usuario
        }
        manifest.set_rect(key, png.clone(), CELL, CellRect { col: 0, row: 0, w: 1, h: 1 });
    }
    let json = manifest.to_json().map_err(|e| e.to_string())?;
    std::fs::write(manifest_path, json + "\n").map_err(|e| format!("{}: {e}", manifest_path.display()))
}

/// Hornea `n` aldeanos deterministas a `art/npc/char_XX.png` + el jugador default
/// a `art/player/character.png`. Devuelve `(clave lógica, ruta relativa del PNG)`.
pub fn bake(
    cat: &Catalog,
    art_root: &Path,
    npcs: usize,
    seed: u64,
    scale_div: u32,
    player: bool,
) -> Result<Vec<(String, String)>, String> {
    let mut keys = Vec::new();
    if npcs > 0 {
        let dir = art_root.join("npc");
        std::fs::create_dir_all(&dir).map_err(|e| format!("{}: {e}", dir.display()))?;
        // El aldeano `i` sale del uid que el juego le va a dar: el mismo número
        // produce la misma cara acá y allá (`society::spawn_npc`). La fórmula vive
        // en `npc_looks` — también la usa el índice de capas, y si se bifurcaran
        // los retratos dejarían de coincidir con los cuerpos.
        for (i, layers) in npc_looks(cat, npcs, seed).iter().enumerate() {
            let img = compose(cat, layers, scale_div)?;
            let rel = format!("art/npc/char_{:02}.png", i + 1);
            let path = art_root.join("npc").join(format!("char_{:02}.png", i + 1));
            img.save(&path).map_err(|e| format!("{}: {e}", path.display()))?;
            keys.push((format!("npc/char_{}", i + 1), rel));
        }
    }
    if player {
        let dir = art_root.join("player");
        std::fs::create_dir_all(&dir).map_err(|e| format!("{}: {e}", dir.display()))?;
        let layers = cat.layers_from_seed(seed ^ 0x50_1AA7, false);
        let img = compose(cat, &layers, scale_div)?;
        let path = dir.join("character.png");
        img.save(&path).map_err(|e| format!("{}: {e}", path.display()))?;
        keys.push(("player/character".to_string(), "art/player/character.png".to_string()));
    }
    Ok(keys)
}

// ── exportar las CAPAS al juego ─────────────────────────────────────────────
//
// `bake` (arriba) hornea personajes CERRADOS: 20 aldeanos y el jugador default.
// Sirve para la variedad del pueblo, pero no para que VOS te armes la cara: eso
// necesita las capas sueltas adentro del APK.
//
// Meterlas enteras costaría ~13 MB (200 peinados × una hoja de 56×20 celdas con
// animaciones de sillón, teléfono y libro que nadie usa). Entonces se exporta una
// TIRA: las filas 0–2 (poses quietas, idle, caminar) a media escala. Mismo
// contrato, ~15% del peso, y componer en el teléfono pasa a costar 86 k píxeles.
//
// Los retratos salen del otro pack (Modern UI · Portrait Generator) y se exportan
// ALINEADOS por índice: `retrato/pelo/007.png` es la cara del mismo peinado que
// `chargen/pelo/007.png`. Por eso el aldeano del menú de diálogo tiene SU cara y
// no una al azar.

use ha_assets::chargen_index::{
    ChargenIndex, LayerOption, LookDef, CELL as IDX_CELL, PORTRAIT_CELL, PORTRAIT_EXPRESSIONS,
    STRIP_COLS, STRIP_ROWS,
};

/// Carpeta destino (relativa a `assets/`) de cada capa del cuerpo.
fn body_dir(kind: LayerKind) -> &'static str {
    match kind {
        LayerKind::Body => "chargen/cuerpo",
        LayerKind::Eyes => "chargen/ojos",
        LayerKind::Outfit => "chargen/ropa",
        LayerKind::Hairstyle => "chargen/pelo",
        LayerKind::Accessory => "chargen/accesorio",
    }
}

/// Carpeta destino del retrato equivalente.
fn portrait_dir(kind: LayerKind) -> Option<&'static str> {
    match kind {
        LayerKind::Body => Some("retrato/piel"),
        LayerKind::Eyes => Some("retrato/ojos"),
        LayerKind::Hairstyle => Some("retrato/pelo"),
        LayerKind::Accessory => Some("retrato/accesorio"),
        // La ropa no entra en un primer plano de la cara: el pack de UI ni la trae.
        LayerKind::Outfit => None,
    }
}

/// Subcarpeta del pack de retratos (Modern UI).
fn portrait_pack_dir(kind: LayerKind) -> Option<&'static str> {
    match kind {
        LayerKind::Body => Some("Skins_32x32"),
        LayerKind::Eyes => Some("Eyes_32x32"),
        LayerKind::Hairstyle => Some("Hairstyles_32x32"),
        LayerKind::Accessory => Some("Accessories_32x32"),
        LayerKind::Outfit => None,
    }
}

/// Nombre en castellano de cada accesorio, por número de estilo. ASCII a
/// propósito: la fuente del juego no dibuja acentos ni eñes (regla 6).
const ACCESORIOS_ES: [(u32, &str); 19] = [
    (1, "MARIQUITA"),
    (2, "ABEJA"),
    (3, "MOCHILA"),
    (4, "GORRA"),
    (5, "GORRA DINO"),
    (6, "GORRA DE POLICIA"),
    (7, "PASAMONTANAS"),
    (8, "SOMBRERO DETECTIVE"),
    (9, "CEREBRO ZOMBI"),
    (10, "TORNILLO"),
    (11, "GORRO DE LANA"),
    (12, "BIGOTE"),
    (13, "BARBA"),
    (14, "GUANTES"),
    (15, "ANTEOJOS"),
    (16, "MONOCULO"),
    (17, "BARBIJO"),
    (18, "GORRO DE CHEF"),
    (19, "GORRO DE FIESTA"),
];

/// Trocea el nombre de archivo en palabras, tirando los tokens de resolución y el
/// prefijo `PG_` y normalizando los números (`01` y `1` son el mismo). Es lo que
/// permite casar `Hairstyle_01_01` (cuerpo) con `PG_Hairstyle_01_32x32_1`
/// (retrato) sin una tabla escrita a mano de 200 entradas.
fn tokens(stem: &str) -> Vec<String> {
    stem.split('_')
        .filter(|t| !t.is_empty() && *t != "PG")
        .filter(|t| !(t.contains('x') && t.chars().all(|c| c.is_ascii_digit() || c == 'x')))
        .map(|t| match t.parse::<u32>() {
            Ok(n) => n.to_string(),
            Err(_) => t.to_string(),
        })
        .collect()
}

fn stem_of(p: &Path) -> String {
    p.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default()
}

/// `(estilo, color, nombre)` de una opción. El pack numera `Cosa_<estilo>_<color>`
/// y a veces mete el nombre en el medio (`Accessory_06_Policeman_Hat_01`).
fn style_color(kind: LayerKind, stem: &str) -> (u32, u32, String) {
    let tk = tokens(stem);
    let nums: Vec<u32> = tk.iter().filter_map(|t| t.parse::<u32>().ok()).collect();
    let name: String = tk
        .iter()
        .skip(1)
        .filter(|t| t.parse::<u32>().is_err())
        .cloned()
        .collect::<Vec<_>>()
        .join(" ");
    match kind {
        // Piel y ojos son una sola serie: no tienen "color" aparte.
        LayerKind::Body | LayerKind::Eyes => (nums.first().copied().unwrap_or(1), 0, name),
        _ => (
            nums.first().copied().unwrap_or(1),
            nums.get(1).copied().unwrap_or(1),
            name,
        ),
    }
}

fn label_of(kind: LayerKind, style: u32, name: &str) -> String {
    match kind {
        LayerKind::Body => format!("PIEL {style}"),
        LayerKind::Eyes => format!("OJOS {style}"),
        LayerKind::Outfit => format!("ROPA {style:02}"),
        LayerKind::Hairstyle => format!("PELO {style:02}"),
        LayerKind::Accessory => ACCESORIOS_ES
            .iter()
            .find(|(n, _)| *n == style)
            .map(|(_, es)| es.to_string())
            // Sin traducción: el nombre del pack en mayúsculas ASCII, nunca un
            // número pelado (memoria `nombres-humanos-en-editores`).
            .unwrap_or_else(|| name.to_uppercase()),
    }
}

/// Recorta la tira útil de una hoja del pack ([`STRIP_COLS`] × [`STRIP_ROWS`]
/// celdas) y le borra los carteles de la guía. La hoja viene de la carpeta
/// `32x32`, o sea a resolución nativa (celda 32×64).
fn strip(img: &RgbaImage) -> RgbaImage {
    let w = img.width().min(IDX_CELL[0] * STRIP_COLS);
    let h = img.height().min(IDX_CELL[1] * STRIP_ROWS);
    let mut out = imageops::crop_imm(img, 0, 0, w, h).to_image();
    erase_guide_labels(&mut out);
    out
}

/// Borra los carteles blancos que el pack trae DIBUJADOS ENCIMA de las hojas
/// ("idle", "walk", "sleep", "phone"…).
///
/// No es paranoia: con el recorte viejo de 28 columnas, `chargen/cuerpo/000.png`
/// ya se shippeaba con los carteles "idle" y "walk" adentro (`Body_01.png` es la
/// hoja anotada del pack). No se veía porque el índice del atlas nunca pasaba de
/// 2; al animar de verdad habría aparecido un cartel blanco al lado del
/// personaje. Las filas 3, 6 y 7 tienen cartel en TODAS las hojas.
///
/// Detección: componentes conexos de blanco puro razonablemente grandes. Se
/// borra el bounding box entero porque el texto oscuro del cartel vive adentro
/// del rectángulo blanco.
fn erase_guide_labels(img: &mut RgbaImage) {
    /// Un cartel es blanco puro; el arte del pack no tiene blancos así de planos
    /// salvo la sábana de la cama y el fogonazo, que son más chicos que esto.
    fn is_label_white(p: &image::Rgba<u8>) -> bool {
        p[3] > 200 && p[0] > 240 && p[1] > 240 && p[2] > 240
    }
    const MIN_W: u32 = 10;
    const MIN_H: u32 = 7;

    let (w, h) = (img.width(), img.height());
    let mut seen = vec![false; (w * h) as usize];
    let mut boxes: Vec<(u32, u32, u32, u32)> = Vec::new();

    for y in 0..h {
        for x in 0..w {
            let start = (y * w + x) as usize;
            if seen[start] || !is_label_white(img.get_pixel(x, y)) {
                continue;
            }
            // Flood fill 8-conexo para agarrar el rectángulo del cartel entero.
            let (mut x0, mut y0, mut x1, mut y1) = (x, y, x, y);
            let mut stack = vec![(x, y)];
            seen[start] = true;
            while let Some((cx, cy)) = stack.pop() {
                x0 = x0.min(cx);
                y0 = y0.min(cy);
                x1 = x1.max(cx);
                y1 = y1.max(cy);
                for dy in -1i64..=1 {
                    for dx in -1i64..=1 {
                        let (nx, ny) = (cx as i64 + dx, cy as i64 + dy);
                        if nx < 0 || ny < 0 || nx >= w as i64 || ny >= h as i64 {
                            continue;
                        }
                        let (nx, ny) = (nx as u32, ny as u32);
                        let i = (ny * w + nx) as usize;
                        if !seen[i] && is_label_white(img.get_pixel(nx, ny)) {
                            seen[i] = true;
                            stack.push((nx, ny));
                        }
                    }
                }
            }
            if x1 - x0 + 1 >= MIN_W && y1 - y0 + 1 >= MIN_H {
                boxes.push((x0, y0, x1, y1));
            }
        }
    }

    for (x0, y0, x1, y1) in boxes {
        for y in y0..=y1 {
            for x in x0..=x1 {
                img.put_pixel(x, y, image::Rgba([0, 0, 0, 0]));
            }
        }
    }
}

/// Arma la tira de expresiones: una fila con las caras de
/// [`PORTRAIT_EXPRESSIONS`], en ese orden, sacadas de la hoja 10×3 del pack.
fn expression_strip(sheet: &RgbaImage) -> RgbaImage {
    let (cw, ch) = (PORTRAIT_CELL[0], PORTRAIT_CELL[1]);
    let mut out = RgbaImage::new(cw * PORTRAIT_EXPRESSIONS.len() as u32, ch);
    for (i, (_, col, row)) in PORTRAIT_EXPRESSIONS.iter().enumerate() {
        let (x, y) = (col * cw, row * ch);
        if x + cw > sheet.width() || y + ch > sheet.height() {
            continue; // hoja más chica de lo esperado: esa cara queda vacía
        }
        let cell = imageops::crop_imm(sheet, x, y, cw, ch).to_image();
        imageops::overlay(&mut out, &cell, (i as u32 * cw) as i64, 0);
    }
    out
}

/// Retratos del pack Modern UI, indexados por nombre normalizado.
pub struct PortraitPack {
    files: BTreeMap<(LayerKind, Vec<String>), PathBuf>,
}

impl PortraitPack {
    /// `root` = `PRUEBAS-CODIGOS/modernuserinterface-win/32x32/Portrait_Generator_32x32`.
    pub fn load(root: &Path) -> Result<Self, String> {
        if !root.is_dir() {
            return Err(format!("no existe el pack de retratos: {}", root.display()));
        }
        let mut files = BTreeMap::new();
        for kind in LAYER_ORDER {
            let Some(sub) = portrait_pack_dir(kind) else { continue };
            let dir = root.join(sub);
            let Ok(rd) = std::fs::read_dir(&dir) else { continue };
            for entry in rd.flatten() {
                let p = entry.path();
                if p.extension().and_then(|e| e.to_str()) != Some("png") {
                    continue;
                }
                // `Skin` (retrato) y `Body` (cuerpo) son la misma serie con otro
                // nombre: se casan por el número, así que se guarda sin la palabra.
                let mut tk = tokens(&stem_of(&p));
                if kind == LayerKind::Body || kind == LayerKind::Eyes {
                    tk.retain(|t| t.parse::<u32>().is_ok());
                }
                files.insert((kind, tk), p);
            }
        }
        Ok(Self { files })
    }

    fn find(&self, kind: LayerKind, body_stem: &str) -> Option<&PathBuf> {
        let mut tk = tokens(body_stem);
        if kind == LayerKind::Body || kind == LayerKind::Eyes {
            tk.retain(|t| t.parse::<u32>().is_ok());
        }
        self.files.get(&(kind, tk))
    }
}

/// Exporta las capas recortadas + los retratos alineados y devuelve el índice.
///
/// `cat` tiene que venir de la carpeta `32x32` del pack: es la resolución
/// nativa, y es la única que usa el juego (no queda arte de `16x16`).
pub fn export_layers(
    cat: &Catalog,
    portraits: &PortraitPack,
    art_root: &Path,
    npc_looks: &[Layers],
) -> Result<ChargenIndex, String> {
    let mut index = ChargenIndex {
        cell: IDX_CELL,
        cols: STRIP_COLS,
        rows: STRIP_ROWS,
        portrait_cell: PORTRAIT_CELL,
        portrait_cols: 0,
        portrait_rows: 0,
        ..Default::default()
    };

    for kind in LAYER_ORDER {
        let out_dir = art_root.join(body_dir(kind));
        std::fs::create_dir_all(&out_dir).map_err(|e| format!("{}: {e}", out_dir.display()))?;
        let pdir = portrait_dir(kind);
        if let Some(pd) = pdir {
            let d = art_root.join(pd);
            std::fs::create_dir_all(&d).map_err(|e| format!("{}: {e}", d.display()))?;
        }

        let mut options = Vec::new();
        for (i, src) in cat.list(kind, false).iter().enumerate() {
            let stem = stem_of(src);
            let img = image::open(src).map_err(|e| format!("{}: {e}", src.display()))?.to_rgba8();
            let out_name = format!("{i:03}.png");
            let out_path = out_dir.join(&out_name);
            strip(&img)
                .save(&out_path)
                .map_err(|e| format!("{}: {e}", out_path.display()))?;

            // Retrato del MISMO índice, si el pack de UI lo trae.
            let mut portrait_rel = None;
            if let (Some(pd), Some(psrc)) = (pdir, portraits.find(kind, &stem)) {
                let pimg = image::open(psrc)
                    .map_err(|e| format!("{}: {e}", psrc.display()))?
                    .to_rgba8();
                let strip = expression_strip(&pimg);
                index.portrait_cols = PORTRAIT_EXPRESSIONS.len() as u32;
                index.portrait_rows = 1;
                let pout = art_root.join(pd).join(&out_name);
                strip.save(&pout).map_err(|e| format!("{}: {e}", pout.display()))?;
                portrait_rel = Some(format!("art/{pd}/{out_name}"));
            }

            let (style, color, name) = style_color(kind, &stem);
            options.push(LayerOption {
                png: format!("art/{}/{}", body_dir(kind), out_name),
                label: label_of(kind, style, &name),
                style,
                color,
                portrait: portrait_rel,
            });
        }
        match kind {
            LayerKind::Body => index.bodies = options,
            LayerKind::Eyes => index.eyes = options,
            LayerKind::Outfit => index.outfits = options,
            LayerKind::Hairstyle => index.hair = options,
            LayerKind::Accessory => index.accessories = options,
        }
    }

    index.npc_looks = npc_looks
        .iter()
        .map(|l| LookDef {
            body: l.body as u32,
            eyes: l.eyes as u32,
            outfit: l.outfit as u32,
            hair: l.hair as u32,
            accessory: l.accessory.map(|a| a as u32),
        })
        .collect();
    Ok(index)
}

/// Los looks con los que `bake` hornea los `art/npc/char_XX.png`, en orden. Se
/// calcula igual que allá (misma semilla, misma mezcla) para que el índice diga la
/// verdad sobre lo que hay en disco.
pub fn npc_looks(cat: &Catalog, npcs: usize, seed: u64) -> Vec<Layers> {
    (0..npcs)
        .map(|i| cat.layers_from_seed(seed ^ (i as u64 + 1).wrapping_mul(0x100_0193), false))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn seed_is_deterministic() {
        // Sin pack en disco no se puede componer, pero el sorteo sí se prueba:
        // un catálogo vacío devuelve índices 0 y nunca panickea.
        let cat = Catalog {
            root: PathBuf::from("."),
            files: BTreeMap::new(),
            cell: "32x32",
        };
        let a = cat.layers_from_seed(42, false);
        let b = cat.layers_from_seed(42, false);
        assert_eq!(a, b, "misma semilla ⇒ mismo personaje");
        assert_eq!(a.body, 0, "catálogo vacío no debe salirse de rango");
    }

    #[test]
    fn splitmix_spreads() {
        let v: Vec<u64> = (0..8)
            .map(|i| ha_assets::chargen_index::splitmix64(i) % 100)
            .collect();
        assert!(v.windows(2).any(|w| w[0] != w[1]), "no puede ser constante");
    }

    /// El cuerpo y el retrato del MISMO peinado tienen que caer en la misma clave,
    /// aunque los dos packs escriban el número distinto (`_01` vs `_1`) y uno
    /// meta la resolución en el medio. Si esto se rompe, el aldeano habla con la
    /// cara de otro.
    #[test]
    fn cuerpo_y_retrato_casan_por_nombre() {
        assert_eq!(tokens("Hairstyle_01_01"), tokens("PG_Hairstyle_01_32x32_1"));
        assert_eq!(
            tokens("Accessory_06_Policeman_Hat_01"),
            tokens("PG_Accessory_06_Policeman_Hat_32x32_1")
        );
        // Las variantes "_Small" del pack de UI son OTRA cosa: no deben casar.
        assert_ne!(
            tokens("Accessory_11_Beanie_01"),
            tokens("PG_Accessory_11_Beanie_Small_32x32_1")
        );
    }

    #[test]
    fn estilo_y_color_salen_del_nombre() {
        assert_eq!(style_color(LayerKind::Hairstyle, "Hairstyle_07_03").0, 7);
        assert_eq!(style_color(LayerKind::Hairstyle, "Hairstyle_07_03").1, 3);
        // Piel y ojos son una serie sola: sin color aparte.
        assert_eq!(style_color(LayerKind::Body, "Body_04"), (4, 0, String::new()));
        let (style, color, name) = style_color(LayerKind::Accessory, "Accessory_06_Policeman_Hat_02");
        assert_eq!((style, color), (6, 2));
        assert_eq!(name, "Policeman Hat");
    }

    #[test]
    fn los_accesorios_tienen_nombre_humano() {
        assert_eq!(label_of(LayerKind::Accessory, 13, "Beard"), "BARBA");
        assert_eq!(label_of(LayerKind::Hairstyle, 7, ""), "PELO 07");
        // Sin traducción cae al nombre del pack, nunca a un número pelado.
        assert_eq!(label_of(LayerKind::Accessory, 99, "Space Helmet"), "SPACE HELMET");
    }

    /// Regla 6: la fuente del juego no dibuja acentos ni eñes.
    #[test]
    fn las_etiquetas_son_ascii() {
        for (_, es) in ACCESORIOS_ES {
            assert!(es.is_ascii(), "{es} tiene caracteres que la fuente no dibuja");
        }
    }
}
