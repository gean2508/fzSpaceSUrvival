//! Índice del **generador de personaje por capas** — el contrato entre la
//! herramienta que recorta el pack (`ha-asset-tools chargen --capas`) y el juego,
//! que arma personajes en vivo (creador de personaje y retratos de diálogo).
//!
//! El pack de LimeZu trae una hoja ENTERA por opción de capa (56×20 celdas: idle,
//! caminar, dormir, sentarse, teléfono, libro…). El juego usa la fila 0 (poses
//! quietas) y las filas 1–2 (idle y caminar), así que la herramienta recorta a
//! [`STRIP_ROWS`] filas: el APK no engorda con animaciones de sillón y componer
//! una hoja en el teléfono cuesta 86 k píxeles en vez de 590 k.
//!
//! **Los índices son la identidad.** `pelo[7]` es el mismo peinado acá, en el
//! retrato (`retrato/pelo/007.png`) y en el savegame. Reordenar las listas
//! cambiaría la cara de todos los personajes guardados — por eso la herramienta
//! ordena alfabéticamente y nunca filtra.

use serde::{Deserialize, Serialize};

/// Ruta del índice dentro de `assets/` (el juego lo incrusta con `include_str!`;
/// es texto chico y así no hay carga asíncrona en Android para arrancar).
pub const CHARGEN_INDEX_PATH: &str = "art/chargen/index.json";

/// Filas de la hoja que se exportan: **las 20**, o sea el pack entero (dormir,
/// sentarse, teléfono, levantar, empujar, pelear…). Antes se exportaban 3 y el
/// resto se tiraba; el peso del APK no es un problema y esas animaciones son el
/// gameplay (ver [`CHAR_ANIMS`]).
pub const STRIP_ROWS: u32 = 20;

/// Columnas que se exportan. La hoja del pack mide 927 px = 56 celdas de 16 px
/// más 31 de sobra; la fila más larga (`lift`/`throw`) usa las 56. Medido sobre
/// las hojas, no supuesto.
pub const STRIP_COLS: u32 = 56;

/// Cuántas direcciones tiene una fila de la hoja.
///
/// **El orden de las direcciones es `R, U, L, D`** (derecha, arriba, izquierda,
/// abajo) — verificado por template-match contra la fila 0. El comentario viejo
/// de este archivo decía "abajo/arriba/derecha/izquierda" y era falso.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
pub enum AnimDirs {
    /// Las cuatro: `R, U, L, D`, una tras otra en la fila.
    Four,
    /// Sólo perfil derecho e izquierdo (sentarse: de frente no se lee la pose).
    RightLeft,
    /// Una sola pose para todas las direcciones (dormir, usar el celular).
    Single,
}

impl AnimDirs {
    pub const fn count(self) -> u32 {
        match self {
            AnimDirs::Four => 4,
            AnimDirs::RightLeft => 2,
            AnimDirs::Single => 1,
        }
    }
}

/// Una animación de la hoja del personaje.
#[derive(Serialize, Deserialize, Clone, Copy, Debug)]
pub struct CharAnim {
    /// ID lógico ASCII (`"walk"`, `"sit"`, `"phone"`…).
    pub key: &'static str,
    /// Nombre humano en castellano para los editores (memoria
    /// `nombres-humanos-en-editores`).
    pub label: &'static str,
    /// Fila de la hoja.
    pub row: u32,
    /// Cuadros POR DIRECCIÓN (no celdas de la fila).
    pub frames: u32,
    pub dirs: AnimDirs,
    pub fps: f32,
    /// `false` = se reproduce una vez y se queda en el último cuadro.
    pub looping: bool,
    /// Primer cuadro dentro del bloque de la dirección. Casi siempre 0; lo usan los tramos de una
    /// fila que el pack marca aparte («4-9 loop» del celular: se saca una vez y después se repite
    /// sólo el tramo del medio).
    pub first: u32,
}

/// Las 20 filas del pack (21 animaciones: la del celular son dos tramos), medidas celda por celda (ver la tabla del scratchpad).
/// Las columnas que sobran en cada fila son **props** (la cama de `sleep`, el
/// celular de `phone_intro`, la caja de `gift`, el fogonazo de `shoot`): no son
/// cuadros del personaje y por eso no entran en `frames`.
pub const CHAR_ANIMS: &[CharAnim] = &[
    a("still", "QUIETO", 0, 1, AnimDirs::Four, 1.0, false),
    a("idle", "RESPIRANDO", 1, 6, AnimDirs::Four, 6.0, true),
    a("walk", "CAMINAR", 2, 6, AnimDirs::Four, 10.0, true),
    a("sleep", "DORMIR", 3, 6, AnimDirs::Single, 4.0, true),
    a("sit", "SENTARSE", 4, 6, AnimDirs::RightLeft, 6.0, true),
    a("sit_floor", "SENTARSE EN EL PISO", 5, 6, AnimDirs::RightLeft, 6.0, true),
    // Fila 6, según la guía del pack (`Spritesheet_animations_GUIDE.png`): cuadros 0-2 lo saca,
    // 3-8 lo usa («4-9 loop», contado desde 1), 9-11 lo guarda. El celular viene DIBUJADO en el
    // cuerpo, así que sale con cualquier personaje del chargen.
    sub("phone_intro", "SACAR EL CELULAR", 6, 0, 3, 8.0, false),
    sub("phone", "USAR EL CELULAR", 6, 3, 6, 8.0, true),
    // Fila 7: LEER. Hasta el 3 oct 2026 se llamaba «phone_intro / SACAR EL CELULAR» y era falso —
    // la guía la marca con los libros y la capa `Books/` del pack sólo tiene esta fila. El cuerpo
    // trae las manos en el pecho; el libro lo pone encima `player/libro`. 0-5 lee, 6-11 da vuelta
    // la página.
    a("read", "LEER UN LIBRO", 7, 12, AnimDirs::Single, 6.0, true),
    // Las manos en el pecho (los 6 primeros cuadros de LEER, sin libro) a 8 fps: el cuerpo de comer y
    // beber. Lo que se lleva a la boca lo dibuja `player/en_mano`, 12 cuadros = dos vueltas de éstos.
    sub("hands_chest", "MANOS EN EL PECHO", 7, 0, 6, 8.0, true),
    a("push_cart", "EMPUJAR CARRITO", 8, 12, AnimDirs::Four, 10.0, true),
    a("pick_up", "AGACHARSE A JUNTAR", 9, 12, AnimDirs::Four, 10.0, false),
    a("gift", "DAR UN REGALO", 10, 10, AnimDirs::Four, 10.0, false),
    a("lift", "LEVANTAR", 11, 14, AnimDirs::Four, 10.0, false),
    a("throw", "TIRAR", 12, 14, AnimDirs::Four, 12.0, false),
    a("hit", "GOLPEAR", 13, 6, AnimDirs::Four, 12.0, false),
    a("punch", "PUNETAZO", 14, 6, AnimDirs::Four, 12.0, false),
    a("stab", "APUNALAR", 15, 12, AnimDirs::Four, 12.0, false),
    a("grab_gun", "SACAR EL ARMA", 16, 4, AnimDirs::Four, 10.0, false),
    a("gun_idle", "APUNTAR", 17, 6, AnimDirs::Four, 6.0, true),
    a("shoot", "DISPARAR", 18, 3, AnimDirs::Four, 14.0, false),
    a("hurt", "RECIBIR DANO", 19, 3, AnimDirs::Four, 10.0, false),
    // No es del pack: el torso de `gun_idle` sobre las piernas de `walk`, empalmado en las celdas libres de la
    // fila 17 (cols 24..47: vacías en las 432 capas, lo verifica `scripts/medir-manos.py`). Lo arma
    // `chargen::empalmar_walk_gun` al componer la hoja y el script en la hoja por defecto.
    CharAnim { first: WALK_GUN_COL0, ..a("walk_gun", "CAMINAR CON EL ARMA", 17, 6, AnimDirs::Four, 10.0, true) },
];

/// Columna de la fila 17 donde empieza `walk_gun`.
pub const WALK_GUN_COL0: u32 = 24;
/// Desde esta altura (px de celda) las piernas de `walk_gun` son las de `walk`; arriba, las de `gun_idle`.
pub const WALK_GUN_CORTE: u32 = 56;

const fn a(
    key: &'static str,
    label: &'static str,
    row: u32,
    frames: u32,
    dirs: AnimDirs,
    fps: f32,
    looping: bool,
) -> CharAnim {
    CharAnim { key, label, row, frames, dirs, fps, looping, first: 0 }
}

/// Un tramo de una fila de UNA sola dirección que arranca en el cuadro `first`.
const fn sub(key: &'static str, label: &'static str, row: u32, first: u32, frames: u32, fps: f32, looping: bool) -> CharAnim {
    CharAnim { key, label, row, frames, dirs: AnimDirs::Single, fps, looping, first }
}

/// **Las filas que usan los aldeanos** (`society::tick_society`: respirar, caminar, dormir, rezar
/// sentado, pelear, juntar). Las hojas de `art/npc/char_XX.png` se COMPACTAN a sólo estas filas, en
/// este orden (`scripts/compactar-npcs.py`): 1536×448 en vez de 1792×1312, o sea 2,7 MB de GPU por
/// hoja en lugar de 9,4 — con 20 hojas son ~54 MB en vez de ~188 MB (el Redmi muere por OOM, §12).
/// El jugador y el creador de personajes siguen con la hoja completa.
pub const NPC_ANIMS: &[&str] = &["still", "idle", "walk", "sleep", "sit_floor", "pick_up", "punch"];

/// Columnas de la hoja compacta: la fila más ancha (`pick_up`: 12 cuadros × 4 direcciones).
pub const NPC_COLS: u32 = 48;

/// `(col, fila)` de una celda en la hoja COMPACTA del aldeano, o `None` si esa animación no está.
pub fn npc_cell(anim: &CharAnim, dir: u32, frame: u32) -> Option<(u32, u32)> {
    let fila = NPC_ANIMS.iter().position(|k| *k == anim.key)? as u32;
    let col = char_cell(anim, dir, frame) % STRIP_COLS;
    Some((col, fila))
}

/// Busca una animación por su ID lógico.
pub fn char_anim(key: &str) -> Option<&'static CharAnim> {
    CHAR_ANIMS.iter().find(|a| a.key == key)
}

/// Índice de celda dentro de la hoja para `(animación, dirección, cuadro)`.
///
/// `dir` viene en el orden `R, U, L, D` (0..4). Cuando la animación tiene menos
/// direcciones se pliega: `RightLeft` manda arriba/abajo al perfil derecho, y
/// `Single` ignora la dirección.
pub fn char_cell(anim: &CharAnim, dir: u32, frame: u32) -> u32 {
    let slot = match anim.dirs {
        AnimDirs::Four => dir % 4,
        // R y U -> perfil derecho (0); L y D -> perfil izquierdo (1).
        AnimDirs::RightLeft => u32::from(matches!(dir, 2 | 3)),
        AnimDirs::Single => 0,
    };
    let col = slot * anim.frames + anim.first + frame % anim.frames.max(1);
    anim.row * STRIP_COLS + col
}

/// Celda del personaje — la misma que `art/player/character.png` y que el atlas
/// del jugador (`scene::CHAR_COLS/CHAR_ROWS`).
///
/// **32×64: la resolución nativa del pack, sin downscale.** Antes era 16×32,
/// que salía de la carpeta `16x16` (el downscale a media escala que hizo el
/// autor). El personaje se dibuja igual de grande —su tamaño en el mundo lo fija
/// `custom_size` en metros, no la textura—, así que lo único que cambia es
/// cuántos píxeles tiene adentro: el doble por lado. En la pantalla del teléfono
/// eso es la diferencia entre una cara de 8 px y una de 16.
pub const CELL: [u32; 2] = [32, 64];

/// Celda del retrato (pack Modern UI, `Portrait_Generator`). Ojo: la carpeta del
/// pack se llama `32x32` por el TILE, pero una cara ocupa 2×2 tiles — medido
/// abriendo la hoja (640×192 = 10 × 3 caras), no supuesto por el nombre.
pub const PORTRAIT_CELL: [u32; 2] = [64, 64];

/// Las expresiones que el juego usa, en el orden en que quedan en la tira
/// exportada, y de qué celda `(col, fila)` de la hoja del pack salen.
///
/// **Las 30 que trae el pack** (hoja 10×3). Antes se exportaban 6 para ahorrar
/// 4 MB de APK; el peso no es un problema y las burbujas de diálogo ahora
/// muestran retrato, así que conviene tener todo el repertorio de caras.
///
/// Sobre los nombres: el pack **no** viene con las caras etiquetadas, y mirando
/// la hoja al 10× la mayoría son variaciones finas de hacia dónde miran los ojos
/// y cómo está la boca, no emociones distintas. Sólo seis se leen sin ambigüedad
/// y ésas conservan su nombre; a las otras las dejo como `CARA NN` en vez de
/// inventarles una emoción que no se puede verificar. Renombralas desde HA
/// Studio cuando decidas para qué usás cada una.
pub const PORTRAIT_EXPRESSIONS: [(&str, u32, u32); 30] = [
    ("NEUTRA", 0, 0),
    ("CARA 02", 1, 0),
    ("DORMIDA", 2, 0),
    ("CARA 04", 3, 0),
    ("CONTENTA", 4, 0),
    ("CARA 06", 5, 0),
    ("CARA 07", 6, 0),
    ("CARA 08", 7, 0),
    ("CARA 09", 8, 0),
    ("CARA 10", 9, 0),
    ("CARA 11", 0, 1),
    ("CARA 12", 1, 1),
    ("ENOJADA", 2, 1),
    ("CARA 14", 3, 1),
    ("CARA 15", 4, 1),
    ("TRISTE", 5, 1),
    ("CARA 17", 6, 1),
    ("CARA 18", 7, 1),
    ("CARA 19", 8, 1),
    ("CARA 20", 9, 1),
    ("CARA 21", 0, 2),
    ("CARA 22", 1, 2),
    ("CARA 23", 2, 2),
    ("CARA 24", 3, 2),
    ("DE ESPALDAS", 4, 2),
    ("CARA 26", 5, 2),
    ("CARA 27", 6, 2),
    ("CARA 28", 7, 2),
    ("CARA 29", 8, 2),
    ("CARA 30", 9, 2),
];

/// Una opción de una capa: un PNG recortado + cómo se llama en castellano.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq, Eq)]
pub struct LayerOption {
    /// Ruta relativa a `assets/` — `"art/chargen/pelo/007.png"`.
    pub png: String,
    /// Nombre humano ASCII del ESTILO ("PELO 02", "GORRA POLICIA"). El color va
    /// aparte: en la pantalla son dos selectores, no 200 entradas en fila.
    pub label: String,
    /// Estilo (1-based). Las opciones con el mismo `style` son el mismo corte de
    /// pelo / la misma prenda en distinto color.
    pub style: u32,
    /// Color/variante dentro del estilo (1-based). 0 = la capa no tiene colores.
    pub color: u32,
    /// Retrato equivalente, si el pack de UI lo trae (`None` = mochila, guantes…
    /// cosas que no se ven en un primer plano de la cara).
    pub portrait: Option<String>,
}

/// Un personaje = un índice por capa. Es lo que se guarda en el savegame y lo
/// que define un NPC; `None` en accesorio = sin accesorio.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct LookDef {
    pub body: u32,
    pub eyes: u32,
    pub outfit: u32,
    pub hair: u32,
    pub accessory: Option<u32>,
}

/// Cuántas opciones tiene cada capa. Es lo único que necesita el sorteo, así que
/// sirve tanto para el catálogo del pack (la herramienta) como para el índice
/// exportado (el juego).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct LayerCounts {
    pub bodies: usize,
    pub eyes: usize,
    pub outfits: usize,
    pub hair: usize,
    pub accessories: usize,
}

/// Mezclador de bits (SplitMix64): barato, bien distribuido y **determinista en
/// toda plataforma**. El aldeano `uid = 37` tiene la misma cara en tu máquina y
/// en el teléfono.
pub fn splitmix64(mut x: u64) -> u64 {
    x = x.wrapping_add(0x9E37_79B9_7F4A_7C15);
    let mut z = x;
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
}

/// Personaje sorteado a partir de una semilla (el uid del NPC, por ejemplo).
///
/// **Vive acá y no en cada lado a propósito.** La herramienta lo usa para hornear
/// los aldeanos y el juego para componerlos y para dibujar su retrato; si hubiera
/// dos implementaciones, el aldeano hablaría con la cara de otro en cuanto una se
/// tocara.
pub fn look_from_seed(counts: LayerCounts, seed: u64) -> LookDef {
    let pick = |salt: u64, n: usize| -> u32 {
        if n == 0 {
            return 0;
        }
        (splitmix64(seed ^ salt) % n as u64) as u32
    };
    // 1 de cada 4 lleva accesorio: si todos llevan, el pueblo parece un desfile.
    let accessory = (splitmix64(seed ^ 0xACCE) % 4 == 0 && counts.accessories > 0)
        .then(|| (splitmix64(seed ^ 0xA11) % counts.accessories as u64) as u32);
    LookDef {
        body: pick(0xB0D1, counts.bodies),
        eyes: pick(0xEE55, counts.eyes),
        outfit: pick(0x0F17, counts.outfits),
        hair: pick(0x4A18, counts.hair),
        accessory,
    }
}

/// El índice completo. Lo escribe la herramienta y lo lee el juego.
#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct ChargenIndex {
    /// Celda del personaje (16×32) — redundante con [`CELL`] pero explícito en el
    /// archivo para que se entienda sin leer código.
    pub cell: [u32; 2],
    /// Columnas de la tira exportada (56).
    pub cols: u32,
    /// Filas de la tira exportada ([`STRIP_ROWS`]).
    pub rows: u32,
    pub portrait_cell: [u32; 2],
    /// Grilla de EXPRESIONES del retrato (10×3 = 30 caras: neutra, contenta,
    /// dormida, de espaldas…). Ver `Expression` en el juego.
    pub portrait_cols: u32,
    #[serde(default)]
    pub portrait_rows: u32,
    pub bodies: Vec<LayerOption>,
    pub eyes: Vec<LayerOption>,
    pub outfits: Vec<LayerOption>,
    pub hair: Vec<LayerOption>,
    pub accessories: Vec<LayerOption>,
    /// El look EXACTO con el que se horneó cada `art/npc/char_XX.png`, en orden.
    /// Sin esto el retrato de un aldeano sería una cara al azar en vez de LA suya:
    /// el juego no puede recalcular la semilla del horneado, así que la guardamos.
    pub npc_looks: Vec<LookDef>,
}

impl ChargenIndex {
    pub fn from_json(raw: &str) -> Result<Self, serde_json::Error> {
        serde_json::from_str(raw)
    }

    pub fn to_json(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string_pretty(self)
    }

    pub fn counts(&self) -> LayerCounts {
        LayerCounts {
            bodies: self.bodies.len(),
            eyes: self.eyes.len(),
            outfits: self.outfits.len(),
            hair: self.hair.len(),
            accessories: self.accessories.len(),
        }
    }

    /// Sortea un personaje con las opciones que trae este índice.
    pub fn look_from_seed(&self, seed: u64) -> LookDef {
        look_from_seed(self.counts(), seed)
    }

    /// Índices de `list` agrupados por estilo, en orden. `[[0,1,2],[3,4,5]…]` —
    /// es lo que necesitan los dos selectores (estilo y color) de la pantalla.
    pub fn by_style(list: &[LayerOption]) -> Vec<Vec<usize>> {
        let mut out: Vec<Vec<usize>> = Vec::new();
        let mut last: Option<u32> = None;
        for (i, opt) in list.iter().enumerate() {
            if last != Some(opt.style) || out.is_empty() {
                out.push(Vec::new());
                last = Some(opt.style);
            }
            out.last_mut().expect("recién empujado").push(i);
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn opt(style: u32, color: u32) -> LayerOption {
        LayerOption { style, color, ..Default::default() }
    }

    #[test]
    fn agrupa_por_estilo_en_orden() {
        let list = vec![opt(1, 1), opt(1, 2), opt(2, 1), opt(3, 1), opt(3, 2), opt(3, 3)];
        let groups = ChargenIndex::by_style(&list);
        assert_eq!(groups, vec![vec![0, 1], vec![2], vec![3, 4, 5]]);
    }

    #[test]
    fn lista_vacia_no_agrupa_nada() {
        assert!(ChargenIndex::by_style(&[]).is_empty());
    }

    #[test]
    fn el_indice_va_y_vuelve_de_json() {
        let mut idx = ChargenIndex { cell: CELL, cols: 56, rows: STRIP_ROWS, ..Default::default() };
        idx.hair.push(LayerOption {
            png: "art/chargen/pelo/000.png".into(),
            label: "PELO 01".into(),
            style: 1,
            color: 1,
            portrait: Some("art/retrato/pelo/000.png".into()),
        });
        idx.npc_looks.push(LookDef { body: 1, eyes: 2, outfit: 3, hair: 4, accessory: Some(5) });
        let back = ChargenIndex::from_json(&idx.to_json().unwrap()).unwrap();
        assert_eq!(back.hair, idx.hair);
        assert_eq!(back.npc_looks, idx.npc_looks);
    }

    #[test]
    fn la_hoja_compacta_del_aldeano_alcanza_para_todas_sus_animaciones() {
        for k in NPC_ANIMS {
            let a = char_anim(k).expect(k);
            for dir in 0..4 {
                for f in 0..a.frames {
                    let (col, fila) = npc_cell(a, dir, f).unwrap();
                    assert!(col < NPC_COLS, "{k} se sale de la hoja compacta");
                    assert!((fila as usize) < NPC_ANIMS.len());
                }
            }
        }
        assert!(npc_cell(char_anim("hurt").unwrap(), 0, 0).is_none());
    }

    #[test]
    fn las_animaciones_caben_en_su_fila_y_respetan_la_guia_del_pack() {
        for a in CHAR_ANIMS {
            let ultimo = char_cell(a, 3, a.frames - 1);
            assert_eq!(ultimo / STRIP_COLS, a.row, "{} se sale de su fila", a.key);
            assert!(ultimo % STRIP_COLS < STRIP_COLS);
        }
        // Celular: lo saca en 0-2 y repite 3-8 («4-9 loop» de la guía, contado desde 1).
        let intro = char_anim("phone_intro").unwrap();
        let uso = char_anim("phone").unwrap();
        assert_eq!(char_cell(intro, 0, 0) % STRIP_COLS, 0);
        assert_eq!(char_cell(uso, 0, 0) % STRIP_COLS, intro.frames);
        assert_eq!(char_cell(uso, 0, uso.frames - 1) % STRIP_COLS, 8);
        // La fila 7 es LEER (la capa `Books/` del pack sólo tiene esa fila), no el celular.
        assert_eq!(char_anim("read").unwrap().row, 7);
        assert!(CHAR_ANIMS.iter().all(|a| a.key != "phone_intro" || a.row == 6));
    }
}
