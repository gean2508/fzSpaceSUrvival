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
    pub fn layers_from_seed(&self, seed: u64, kid: bool) -> Layers {
        let pick = |salt: u64, n: usize| -> usize {
            if n == 0 {
                return 0;
            }
            (splitmix64(seed ^ salt) % n as u64) as usize
        };
        // 1 de cada 4 lleva accesorio: si todos llevan, el pueblo parece un desfile.
        let acc_n = self.count(LayerKind::Accessory, kid);
        let accessory = (splitmix64(seed ^ 0xACCE) % 4 == 0 && acc_n > 0)
            .then(|| (splitmix64(seed ^ 0xA11) % acc_n as u64) as usize);
        Layers {
            body: pick(0xB0D1, self.count(LayerKind::Body, kid)),
            eyes: pick(0xEE55, self.count(LayerKind::Eyes, kid)),
            outfit: pick(0x0F17, self.count(LayerKind::Outfit, kid)),
            hair: pick(0x4A18, self.count(LayerKind::Hairstyle, kid)),
            accessory,
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

/// Mezclador de bits (SplitMix64): barato y bien distribuido. Determinista.
fn splitmix64(mut x: u64) -> u64 {
    x = x.wrapping_add(0x9E37_79B9_7F4A_7C15);
    let mut z = x;
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
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

/// Celda de la hoja horneada, en píxeles (media escala). Es la MISMA que usa el
/// creador de personaje (`character_create::CHAR_COLS/ROWS`) y `player/character`.
pub const CELL: (u32, u32) = (16, 32);

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
        for i in 0..npcs {
            // El aldeano `i` sale del uid que el juego le va a dar: el mismo
            // número produce la misma cara acá y allá (`society::spawn_npc`).
            let layers = cat.layers_from_seed(seed ^ (i as u64 + 1).wrapping_mul(0x100_0193), false);
            let img = compose(cat, &layers, scale_div)?;
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
        let v: Vec<u64> = (0..8).map(|i| splitmix64(i) % 100).collect();
        assert!(v.windows(2).any(|w| w[0] != w[1]), "no puede ser constante");
    }
}
