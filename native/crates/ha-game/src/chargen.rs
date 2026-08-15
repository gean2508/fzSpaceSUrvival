//! **Generador de personaje en vivo** — apila las capas del pack (piel, ojos,
//! ropa, pelo, accesorio) dentro del juego y devuelve UNA textura.
//!
//! Por qué existe, si `ha-asset-tools chargen` ya hornea personajes: porque un
//! personaje horneado es una cara fija. Vos querés elegir gorro, piel y camiseta
//! en la pantalla de creación, y cada aldeano querés que sea distinto — eso son
//! miles de combinaciones y no se hornean todas.
//!
//! Reglas que respeta (biblia §2, §10):
//!
//! - **Se compone UNA vez, no por frame.** El resultado se cachea por `LookDef` y
//!   el sprite del personaje sigue siendo un solo quad con una sola textura: en el
//!   Redmi cuesta exactamente lo mismo que antes.
//! - **Se compone en la pantalla de carga**, no en medio del gameplay: por eso
//!   cada trabajo entra en una cola con presupuesto por frame ([`JOBS_PER_FRAME`]).
//! - Las capas son TIRAS recortadas (56×20 celdas de 32×64 — ver
//!   `ha_assets::chargen_index`): componer cuesta 43 k píxeles, no 590 k, y la
//!   textura del aldeano ocupa 172 kB de VRAM en vez de 2,3 MB.
//!
//! El índice (`art/chargen/index.json`) va **incrustado en el binario**: es texto
//! chico y así el arranque no depende de una carga asíncrona en Android.

use std::collections::HashMap;

use bevy::asset::RenderAssetUsages;
use bevy::prelude::*;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};

pub use ha_assets::chargen_index::{ChargenIndex, LayerOption, LookDef};

/// El índice que dejó `ha-asset-tools chargen --capas`.
const INDEX_JSON: &str = include_str!("../assets/art/chargen/index.json");

/// Cuántos personajes se componen por frame. La pantalla de carga tiene todo el
/// tiempo del mundo; lo que no puede pasar es un tirón si un aldeano nace tarde.
const JOBS_PER_FRAME: usize = 2;

/// La semilla con la que se hornearon los aldeanos del pack
/// (`ha-asset-tools chargen --seed 7`). Vive acá porque el juego rehace el mismo
/// sorteo para componerlos en vivo; cambiarla le cambia la cara a todo el pueblo.
const BAKE_SEED: u64 = 7;

/// Capas en el orden EXACTO de apilado del pack. Cambiarlo dibuja el pelo debajo
/// de la cabeza.
const LAYER_ORDER: [Layer; 5] = [Layer::Body, Layer::Eyes, Layer::Outfit, Layer::Hair, Layer::Accessory];

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Layer {
    Body,
    Eyes,
    Outfit,
    Hair,
    Accessory,
}

impl Layer {
    /// Nombre humano de la fila en la pantalla de creación (ASCII: regla 6).
    pub fn label(self) -> &'static str {
        match self {
            Layer::Body => "PIEL",
            Layer::Eyes => "OJOS",
            Layer::Outfit => "ROPA",
            Layer::Hair => "PELO",
            Layer::Accessory => "GORRO",
        }
    }
}

/// Cara con la que se dibuja un retrato. El número ES la columna del atlas, así
/// que estos valores tienen que coincidir con la posición que ocupa cada nombre
/// en `ha_assets::chargen_index::PORTRAIT_EXPRESSIONS` (la tira exporta las 30
/// caras del pack). Sólo se nombran las seis que se leen sin ambigüedad; el
/// resto de la tira existe igual y se puede indexar por número.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum Expression {
    #[default]
    Neutral = 0,
    Dormida = 2,
    Contenta = 4,
    Enojada = 12,
    Triste = 15,
    DeEspaldas = 24,
}

impl Expression {
    pub fn index(self) -> usize {
        self as usize
    }

    /// La cara que le toca a un aldeano por cómo te trata y cómo está. Es lo que
    /// convierte el retrato en información en vez de un adorno: si el pueblo te
    /// odia, o si ese vecino se está muriendo de hambre, se te nota en la cara
    /// antes de que abra la boca.
    pub fn for_npc(role: &str, food: f32, rest: f32) -> Self {
        match role {
            "HOSTIL" => Expression::Enojada,
            _ if rest < 0.2 => Expression::Dormida,
            _ if food < 0.3 => Expression::Triste,
            "FIEL" => Expression::Contenta,
            _ => Expression::Neutral,
        }
    }
}

pub struct ChargenPlugin;

impl Plugin for ChargenPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Chargen>()
            .init_resource::<ChargenJobs>()
            .add_systems(Update, tick_jobs);
    }
}

/// El catálogo de capas, ya parseado. Se lee en cualquier lado con `Res<Chargen>`.
#[derive(Resource)]
pub struct Chargen {
    pub index: ChargenIndex,
    /// Índices agrupados por estilo, por capa: lo que necesitan los selectores
    /// "estilo" y "color" de la pantalla de creación.
    pub styles: HashMap<&'static str, Vec<Vec<usize>>>,
}

impl Default for Chargen {
    fn default() -> Self {
        let index = ChargenIndex::from_json(INDEX_JSON).unwrap_or_else(|e| {
            // Sin índice el juego sigue andando con la hoja horneada de siempre.
            eprintln!("chargen: index.json invalido ({e}) — sin generador de personaje");
            ChargenIndex::default()
        });
        let mut styles = HashMap::new();
        styles.insert("ropa", ChargenIndex::by_style(&index.outfits));
        styles.insert("pelo", ChargenIndex::by_style(&index.hair));
        styles.insert("accesorio", ChargenIndex::by_style(&index.accessories));
        Self { index, styles }
    }
}

impl Chargen {
    pub fn options(&self, layer: Layer) -> &[LayerOption] {
        match layer {
            Layer::Body => &self.index.bodies,
            Layer::Eyes => &self.index.eyes,
            Layer::Outfit => &self.index.outfits,
            Layer::Hair => &self.index.hair,
            Layer::Accessory => &self.index.accessories,
        }
    }

    /// Estilos de una capa (cada uno con sus colores). Vacío en piel/ojos, que no
    /// tienen color aparte.
    pub fn styles_of(&self, layer: Layer) -> &[Vec<usize>] {
        let key = match layer {
            Layer::Outfit => "ropa",
            Layer::Hair => "pelo",
            Layer::Accessory => "accesorio",
            _ => return &[],
        };
        self.styles.get(key).map(|v| v.as_slice()).unwrap_or(&[])
    }

    /// `true` si el pack está adentro del APK. Si es `false` (build sin exportar
    /// las capas) todo lo demás cae solo al camino viejo.
    pub fn available(&self) -> bool {
        !self.index.bodies.is_empty() && !self.index.outfits.is_empty()
    }

    /// Tamaño en píxeles de una hoja compuesta.
    pub fn sheet_size(&self) -> UVec2 {
        UVec2::new(self.index.cell[0] * self.index.cols, self.index.cell[1] * self.index.rows)
    }

    pub fn portrait_size(&self) -> UVec2 {
        UVec2::new(
            self.index.portrait_cell[0] * self.index.portrait_cols.max(1),
            self.index.portrait_cell[1] * self.index.portrait_rows.max(1),
        )
    }

    /// El look de un aldeano por `uid` — **el que se ve en el mundo**.
    ///
    /// El cuerpo del aldeano es una hoja YA horneada (`npc/char_XX.png`, que
    /// `society::spawn_npc` elige con `uid % 20`), así que el retrato no puede
    /// sortear una cara: tiene que ser LA de esa hoja. El índice guarda con qué
    /// look se horneó cada una, y de ahí sale. Sin eso el aldeano de barba
    /// hablaría con la cara de otro.
    pub fn look_for_uid(&self, uid: u64) -> LookDef {
        let looks = &self.index.npc_looks;
        if looks.is_empty() {
            return self.look_from_bake_seed(uid);
        }
        looks[(uid % looks.len() as u64) as usize]
    }

    /// El sorteo con el que la herramienta hornea al aldeano `i` (`bake`). Es la
    /// misma cuenta de los dos lados — lo verifica
    /// `los_looks_horneados_coinciden_con_los_del_juego`.
    pub fn look_from_bake_seed(&self, i: u64) -> LookDef {
        self.index
            .look_from_seed(BAKE_SEED ^ i.wrapping_add(1).wrapping_mul(0x100_0193))
    }

    /// Rutas de las capas de un look, en orden de apilado.
    fn layer_paths(&self, look: &LookDef, portrait: bool) -> Vec<String> {
        let mut out = Vec::with_capacity(5);
        for layer in LAYER_ORDER {
            let (list, idx) = match layer {
                Layer::Body => (&self.index.bodies, Some(look.body)),
                Layer::Eyes => (&self.index.eyes, Some(look.eyes)),
                Layer::Outfit => (&self.index.outfits, Some(look.outfit)),
                Layer::Hair => (&self.index.hair, Some(look.hair)),
                Layer::Accessory => (&self.index.accessories, look.accessory),
            };
            let Some(opt) = idx.and_then(|i| list.get(i as usize)) else { continue };
            if portrait {
                // La ropa no entra en un primer plano; lo que no tiene retrato se
                // saltea y la cara igual queda completa.
                if let Some(p) = &opt.portrait {
                    out.push(p.clone());
                }
            } else {
                out.push(opt.png.clone());
            }
        }
        out
    }
}

/// Un trabajo de composición pendiente. El handle de salida se entrega al que lo
/// pide EN EL ACTO (con la textura todavía en blanco) y se rellena cuando las
/// capas terminan de cargar: así ningún sistema tiene que esperar.
struct Job {
    layers: Vec<Handle<Image>>,
    out: Handle<Image>,
    size: UVec2,
    /// Para el mensaje de error: qué se estaba componiendo.
    what: String,
}

#[derive(Resource, Default)]
pub struct ChargenJobs {
    queue: Vec<Job>,
    sheets: HashMap<LookDef, Handle<Image>>,
    portraits: HashMap<LookDef, Handle<Image>>,
}

impl ChargenJobs {
    /// `true` cuando no queda nada por componer — la pantalla de carga espera esto.
    pub fn idle(&self) -> bool {
        self.queue.is_empty()
    }

    pub fn pending(&self) -> usize {
        self.queue.len()
    }
}

/// Hoja de personaje (28×3 celdas) de un look. Cacheada: pedirla dos veces
/// devuelve la misma textura.
pub fn sheet_for(
    look: LookDef,
    cg: &Chargen,
    jobs: &mut ChargenJobs,
    images: &mut Assets<Image>,
    assets: &AssetServer,
) -> Option<Handle<Image>> {
    request(look, false, cg, jobs, images, assets)
}

/// Retrato (tira de expresiones de 32×32) de un look.
pub fn portrait_for(
    look: LookDef,
    cg: &Chargen,
    jobs: &mut ChargenJobs,
    images: &mut Assets<Image>,
    assets: &AssetServer,
) -> Option<Handle<Image>> {
    request(look, true, cg, jobs, images, assets)
}

fn request(
    look: LookDef,
    portrait: bool,
    cg: &Chargen,
    jobs: &mut ChargenJobs,
    images: &mut Assets<Image>,
    assets: &AssetServer,
) -> Option<Handle<Image>> {
    if !cg.available() {
        return None;
    }
    let cache = if portrait { &jobs.portraits } else { &jobs.sheets };
    if let Some(h) = cache.get(&look) {
        return Some(h.clone());
    }
    // TECHO DEL CACHE. Cada hoja compuesta pesa lo que la hoja del pack (~9 MB)
    // y hasta ahora se guardaban TODAS las combinaciones vistas: apretar AL AZAR
    // en el creador estrenaba un look por vez y el cache crecia sin freno hasta
    // que Android mataba el juego. Al pasarse, se tira lo viejo — recomponer
    // cuesta un instante y no hay forma de quedarse sin memoria.
    {
        let cache_mut = if portrait { &mut jobs.portraits } else { &mut jobs.sheets };
        let techo = if portrait { 24 } else { 6 };
        if cache_mut.len() >= techo {
            cache_mut.clear();
        }
    }
    let paths = cg.layer_paths(&look, portrait);
    if paths.is_empty() {
        return None;
    }
    let size = if portrait { cg.portrait_size() } else { cg.sheet_size() };
    // Textura vacía YA: el que pide dibuja algo (transparente) mientras las capas
    // cargan, en vez de tener que preguntar si está lista.
    let out = images.add(blank(size));
    jobs.queue.push(Job {
        layers: paths.iter().map(|p| assets.load(p)).collect(),
        out: out.clone(),
        size,
        what: format!("{}{:?}", if portrait { "retrato " } else { "" }, look),
    });
    if portrait {
        jobs.portraits.insert(look, out.clone());
    } else {
        jobs.sheets.insert(look, out.clone());
    }
    Some(out)
}

fn blank(size: UVec2) -> Image {
    Image::new(
        Extent3d { width: size.x, height: size.y, depth_or_array_layers: 1 },
        TextureDimension::D2,
        vec![0; (size.x * size.y * 4) as usize],
        TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::RENDER_WORLD,
    )
}

/// Compone los trabajos cuyas capas ya cargaron, con presupuesto por frame.
fn tick_jobs(
    mut jobs: ResMut<ChargenJobs>,
    mut images: ResMut<Assets<Image>>,
    assets: Res<AssetServer>,
) {
    if jobs.queue.is_empty() {
        return;
    }
    let mut done = 0usize;
    let mut i = 0usize;
    while i < jobs.queue.len() && done < JOBS_PER_FRAME {
        let ready = jobs.queue[i].layers.iter().all(|h| images.get(h).is_some());
        if !ready {
            // Una capa que falla al cargar dejaría el trabajo colgado para siempre
            // (y la pantalla de carga esperando): se descarta con aviso.
            if jobs.queue[i]
                .layers
                .iter()
                .any(|h| matches!(assets.get_load_state(h.id()), Some(bevy::asset::LoadState::Failed(_))))
            {
                let job = jobs.queue.remove(i);
                eprintln!("chargen: no pude componer {} (falta una capa)", job.what);
                continue;
            }
            i += 1;
            continue;
        }
        let job = jobs.queue.remove(i);
        match compose(&job, &images) {
            Some(px) => {
                let mut img = blank(job.size);
                img.data = px;
                images.insert(&job.out, img);
            }
            None => eprintln!("chargen: no pude componer {}", job.what),
        }
        done += 1;
    }
}

/// Apila las capas con "source over" en RGBA8. Pixel art: casi todo el alfa es 0
/// o 255, pero el pack tiene bordes suavizados y mezclarlos mal deja un halo.
fn compose(job: &Job, images: &Assets<Image>) -> Option<Vec<u8>> {
    let (w, h) = (job.size.x as usize, job.size.y as usize);
    let mut out = vec![0u8; w * h * 4];
    let mut any = false;
    for handle in &job.layers {
        let Some(img) = images.get(handle) else { continue };
        if img.texture_descriptor.format != TextureFormat::Rgba8UnormSrgb {
            eprintln!(
                "chargen: capa en formato {:?} (esperaba Rgba8UnormSrgb) — se saltea",
                img.texture_descriptor.format
            );
            continue;
        }
        let lw = img.texture_descriptor.size.width as usize;
        let lh = img.texture_descriptor.size.height as usize;
        let src = &img.data;
        for y in 0..h.min(lh) {
            for x in 0..w.min(lw) {
                let s = (y * lw + x) * 4;
                let a = src[s + 3] as u32;
                if a == 0 {
                    continue;
                }
                let d = (y * w + x) * 4;
                if a == 255 {
                    out[d..d + 4].copy_from_slice(&src[s..s + 4]);
                    continue;
                }
                let inv = 255 - a;
                for c in 0..3 {
                    let sv = src[s + c] as u32 * a;
                    let dv = out[d + c] as u32 * inv;
                    out[d + c] = ((sv + dv) / 255) as u8;
                }
                out[d + 3] = (a + out[d + 3] as u32 * inv / 255).min(255) as u8;
            }
        }
        any = true;
    }
    any.then_some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// El índice tiene que estar exportado y bien formado: es lo que come el
    /// creador de personaje y el retrato de cada aldeano.
    #[test]
    fn el_indice_esta_exportado() {
        let cg = Chargen::default();
        assert!(cg.available(), "faltan las capas — corre `chargen --capas`");
        assert_eq!(cg.index.cell, ha_assets::chargen_index::CELL);
        assert!(cg.index.hair.len() > 20, "muy pocos peinados: {}", cg.index.hair.len());
        assert!(cg.sheet_size().x > 0 && cg.sheet_size().y > 0);
    }

    /// Todas las opciones tienen nombre humano en castellano — la lista se lee
    /// "GORRA DE POLICIA", no `Accessory_06`.
    #[test]
    fn todas_las_opciones_tienen_nombre_ascii() {
        let cg = Chargen::default();
        for layer in LAYER_ORDER {
            for opt in cg.options(layer) {
                assert!(!opt.label.is_empty(), "{layer:?} sin etiqueta");
                let rotos: String =
                    opt.label.chars().filter(|c| !crate::fuente::dibujable(*c)).collect();
                assert!(rotos.is_empty(), "la fuente no dibuja [{rotos}] en {}", opt.label);
            }
        }
    }

    /// Un aldeano compone SU cara y SU retrato desde el mismo look: el sorteo
    /// tiene que ser estable entre llamadas y quedar dentro del catálogo.
    #[test]
    fn el_look_de_un_aldeano_es_estable_y_valido() {
        let cg = Chargen::default();
        for uid in 0..50u64 {
            let a = cg.look_for_uid(uid);
            assert_eq!(a, cg.look_for_uid(uid), "el uid {uid} cambio de cara");
            assert!((a.body as usize) < cg.index.bodies.len());
            assert!((a.eyes as usize) < cg.index.eyes.len());
            assert!((a.outfit as usize) < cg.index.outfits.len());
            assert!((a.hair as usize) < cg.index.hair.len());
            if let Some(acc) = a.accessory {
                assert!((acc as usize) < cg.index.accessories.len());
            }
        }
        // Tantas caras distintas como hojas horneadas hay (el cuerpo manda).
        let distintos: std::collections::HashSet<_> = (0..50u64).map(|u| cg.look_for_uid(u)).collect();
        assert_eq!(
            distintos.len(),
            cg.index.npc_looks.len(),
            "el pueblo sale clonado: {} caras",
            distintos.len()
        );
    }

    /// Paridad con la herramienta: los looks que el juego arma en vivo son los
    /// MISMOS que los que se hornearon a `art/npc/char_XX.png`. Si esto se rompe,
    /// el aldeano tendría una cara y su retrato otra.
    #[test]
    fn los_looks_horneados_coinciden_con_los_del_juego() {
        let cg = Chargen::default();
        assert!(!cg.index.npc_looks.is_empty(), "el indice no trae los looks horneados");
        for (i, baked) in cg.index.npc_looks.iter().enumerate() {
            assert_eq!(*baked, cg.look_from_bake_seed(i as u64), "aldeano {i}");
        }
        // Y el retrato de un uid cualquiera sale de la MISMA hoja que su cuerpo:
        // `society::spawn_npc` usa `uid % 20` para elegir `npc/char_XX.png`.
        let n = cg.index.npc_looks.len() as u64;
        assert_eq!(cg.look_for_uid(n + 3), cg.look_for_uid(3), "el retrato no sigue al cuerpo");
    }

    /// El valor de cada `Expression` ES la columna del atlas. Si alguien reordena
    /// `PORTRAIT_EXPRESSIONS` (o vuelve a exportar menos caras), el NPC enojado
    /// pasaría a mostrar una cara al azar sin que nada falle en runtime.
    #[test]
    fn las_expresiones_apuntan_a_su_columna() {
        use ha_assets::chargen_index::PORTRAIT_EXPRESSIONS as EXPR;
        for (e, nombre) in [
            (Expression::Neutral, "NEUTRA"),
            (Expression::Dormida, "DORMIDA"),
            (Expression::Contenta, "CONTENTA"),
            (Expression::Enojada, "ENOJADA"),
            (Expression::Triste, "TRISTE"),
            (Expression::DeEspaldas, "DE ESPALDAS"),
        ] {
            assert_eq!(EXPR[e.index()].0, nombre, "{e:?} no cae en su columna");
        }
        let cg = Chargen::default();
        assert_eq!(
            cg.index.portrait_cols as usize,
            EXPR.len(),
            "la tira exportada no trae todas las caras del catalogo"
        );
    }
}
