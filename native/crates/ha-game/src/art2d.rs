//! 2D art library — drop-in PNG packs (Modern Exteriors/Interiors/UI, etc.)
//! over procedural fallbacks.
//!
//! Flow: put PNGs under `assets/art/<category>/<name>.png`, run
//! `npm run sync:art` (regenerates `assets/art/manifest.json`), rebuild.
//! The manifest is embedded at compile time (adding art requires a rebuild
//! anyway — the APK bundles `assets/`). Every consumer asks the library by
//! logical key ("decor/tree", "items/apple", "player/astronaut") and falls
//! back to the current procedural look when the key isn't shipped yet.
//!
//! Categories (see assets/art/README.md): tiles/ decor/ items/ machines/
//! player/ ship/ ui/ backgrounds/.

use std::collections::HashMap;
use std::sync::RwLock;

use bevy::prelude::*;
use bevy::asset::RenderAssetUsages;
use bevy::image::ImageLoaderSettings;

use crate::hud::item_icons::ItemIconCache;
use ha_assets::{ArtAnim, ArtManifest, ArtValue, CellRect, StripDir};

const MANIFEST: &str = include_str!("../assets/art/manifest.json");

/// **Cuántos tiles del juego mide una celda de 32 px del pack.**
///
/// Decía 1,5 con el argumento de que "LimeZu dibuja a 32 px por tile de ~0,66 m".
/// Ese 0,66 no se midió nunca contra nada del juego, y la escala del juego ya
/// estaba fijada por el único objeto cuyo tamaño real conocemos, el personaje:
/// su marco es de 32 × 64 px —1 × 2 celdas— y se dibuja de 0,95 × 1,90 m
/// (`scene::spawn_player`). Una celda son **0,95 m**.
///
/// Con 1,5 todo el pack se dibujaba un 58 % más grande que el personaje que
/// camina entre medio: bancos de tres metros, tachos más altos que el jugador,
/// latas de gaseosa de metro y medio. Se veía en cualquier captura de calle.
///
/// Es el mismo factor con el que `scripts/import-pack-masivo.py` calcula las
/// huellas (`ESCALA_TILE`), y tiene que seguir siéndolo — si se separan, el
/// objeto se dibuja de un tamaño y frena de otro. Lo cuida
/// `una_celda_del_pack_mide_lo_que_el_personaje_dice`.
pub const ESCALA_ARTE: f32 = 0.95;

/// **La hoja del SUELO** — la arma `scripts/atlas-suelo.py`.
///
/// El suelo no se dibuja con sprites sino con la malla del chunk, una sola por
/// chunk. Para que lleve el pixel del pack sin multiplicar los draw calls, TODO
/// el suelo vive en esta hoja y la malla mete las UVs de la celda que le toca a
/// cada tile.
pub const SUELO_PAGINA: &str = "art/pack/suelo.png";

/// Tamaño en píxeles de esa hoja. **Es fijo y compartido con el script** porque
/// la malla se arma antes de que Bevy termine de cargar la textura: no hay a
/// quién preguntarle el tamaño. Lo cuida `la_hoja_del_suelo_mide_lo_que_dice`.
pub const SUELO_PAGINA_PX: (u32, u32) = (1024, 256);

/// Celda BLANCA de la hoja (la 0,0). `ColorMaterial` multiplica textura × color
/// de vértice, así que un tile sin losa asignada apunta acá y se sigue viendo
/// con su color plano de siempre — que es lo que permitió texturizar el suelo
/// sin cambiarle el aspecto a lo que todavía no tiene arte.
pub fn suelo_uv_blanca() -> [f32; 4] {
    let (pw, ph) = (SUELO_PAGINA_PX.0 as f32, SUELO_PAGINA_PX.1 as f32);
    let (ix, iy) = (0.5 / pw, 0.5 / ph);
    [ix, iy, 32.0 / pw - ix, 32.0 / ph - iy]
}

/// **Las poses de las hojas de personaje, sin que nadie las recorte a mano.**
///
/// El generador de personajes hornea `art/npc/char_XX.png` y `art/player/
/// character.png` con una geometría FIJA que ya está medida celda por celda en
/// `ha_assets::chargen_index::CHAR_ANIMS` (20 animaciones: fila, cuadros,
/// direcciones, fps, si cicla) y `char_cell`. El juego, en cambio, pedía poses
/// (`walk_down`, `idle_left`, `hit_up`…) que **nadie había escrito en el
/// manifest**: `sim.rs` preguntaba `has_poses` y le decían que no, así que el
/// jugador y los 20 NPCs se dibujaban con un cuadro fijo. Las animaciones se
/// veían en el creador de personaje —que usa el índice— y en el juego no.
///
/// Acá se tiende ese puente: se registran las poses derivadas del MISMO índice
/// con el que se horneó la hoja, así que si mañana se rehornea siguen calzando.
/// No se inventa geometría: sale toda de `chargen_index`.
fn registrar_poses_de_personaje(art: &mut ArtLibrary) {
    use ha_assets::chargen_index::{char_cell, AnimDirs, CHAR_ANIMS, STRIP_COLS};

    // `R, U, L, D` — el orden de las direcciones en la hoja, con el nombre que
    // usa el juego (`sim::pose_direction`).
    const DIRS: [(u32, &str); 4] = [(0, "right"), (1, "up"), (2, "left"), (3, "down")];

    let claves: Vec<String> = art
        .rutas
        .keys()
        .filter(|k| *k == "player/character" || k.starts_with("npc/char_"))
        .cloned()
        .collect();
    if claves.is_empty() {
        return;
    }
    let [cw, ch] = ha_assets::chargen_index::CELL;
    let mut total = 0usize;
    for clave in claves {
        let mut poses: HashMap<String, PoseInfo> = HashMap::new();
        // Los aldeanos tienen la hoja COMPACTA (sólo sus filas, `NPC_ANIMS`); el jugador, la completa.
        let es_npc = clave.starts_with("npc/char_");
        for anim in CHAR_ANIMS {
            if es_npc && ha_assets::chargen_index::npc_cell(anim, 0, 0).is_none() {
                continue;
            }
            for (dir, nombre) in DIRS {
                // Una animación de una sola dirección se registra una vez: cuatro
                // poses idénticas sólo engordan el mapa.
                if anim.dirs == AnimDirs::Single && dir != 3 {
                    continue;
                }
                let (col, row) = if es_npc {
                    ha_assets::chargen_index::npc_cell(anim, dir, 0).unwrap_or((0, 0))
                } else {
                    let celda = char_cell(anim, dir, 0);
                    (celda % STRIP_COLS, celda / STRIP_COLS)
                };
                let origen = UVec2::new(col * cw, row * ch);
                let pose = format!("{}_{}", anim.key, nombre);
                poses.insert(
                    pose,
                    PoseInfo {
                        rect_px: [origen.x, origen.y, cw, ch],
                        // Un solo cuadro no es una animación: `still` se queda
                        // quieto y ahorra un reloj por personaje.
                        anim: (anim.frames > 1).then(|| StripInfo {
                            origin: origen,
                            cell: UVec2::new(cw, ch),
                            step: UVec2::new(cw, 0),
                            frames: anim.frames,
                            fps: anim.fps,
                        }),
                        layer: None,
                        image: None,
                    },
                );
                total += 1;
            }
        }
        // `idle_down` es el reposo al que cae todo lo demás (`sim.rs`): si la
        // hoja no trajera `idle`, el personaje se quedaría sin sprite.
        if !poses.contains_key("idle_down") {
            continue;
        }
        art.poses.insert(clave, poses);
    }
    eprintln!("ha-art: {total} poses de personaje derivadas del indice del chargen");
}

/// Una animación tira-de-frames dentro de una hoja: `frames` frames de tamaño
/// `cell` px, empezando en `origin` (px) y avanzando `step` px por frame.
/// `cell == ZERO` → derivar de la imagen completa (tira horizontal simple).
/// `step == ZERO` con `cell != ZERO` → avanzar horizontal por el ancho del frame.
#[derive(Clone, Copy)]
struct StripInfo {
    origin: UVec2,
    cell: UVec2,
    step: UVec2,
    frames: u32,
    fps: f32,
}

/// Resuelve un frame en celdas (`first_frame`) + cantidad + dirección a un
/// `StripInfo` en píxeles: cada frame ocupa `w×h` celdas y los siguientes se
/// pegan a la derecha o hacia abajo según `dir`.
fn strip_info(first_frame: CellRect, grid: (u32, u32), frames: u32, fps: f32, dir: StripDir) -> StripInfo {
    let (gw, gh) = grid;
    let (fw, fh) = (first_frame.w.max(1), first_frame.h.max(1));
    let cell = UVec2::new(fw * gw, fh * gh);
    let step = match dir {
        StripDir::Right => UVec2::new(cell.x, 0),
        StripDir::Down => UVec2::new(0, cell.y),
    };
    StripInfo {
        origin: UVec2::new(first_frame.col * gw, first_frame.row * gh),
        cell,
        step,
        frames: frames.max(1),
        fps,
    }
}

/// Un frame/rango de la hoja para una pose nombrada (idle_down, walk_left…):
/// rect px base + animación de sub-región si la pose es una tira.
#[derive(Clone)]
struct PoseInfo {
    rect_px: [u32; 4],
    anim: Option<StripInfo>,
    /// Capa de la pose (edificios: Floor/Wall/Object/Roof). None = Object.
    layer: Option<ha_assets::ArtLayer>,
    /// Hoja PROPIA de la pose (assets armados con varios PNG: uno por dirección
    /// o estado). `None` = la hoja del slot. Ver `ArtLibrary::pose_image`.
    image: Option<Handle<Image>>,
}

/// **De dónde sale el pixel de una clave, sin haberlo cargado todavía.**
///
/// Es la mitad barata del manifest: la ruta del PNG y con qué ajustes hay que
/// pedirlo. Declarar una clave cuesta esto —un `String` y un `bool`—, no una
/// textura.
#[derive(Clone)]
struct FuenteArte {
    png: String,
    /// El slot declara `grid`, así que su geometría sale del manifest y nadie
    /// necesita los píxeles en RAM: se sube a la GPU y se suelta la copia de CPU.
    solo_gpu: bool,
}

/// **Cuántas losas LISAS por cada losa con marca, según el material.** `None` =
/// el material no mezcla lisa (asfalto, vereda: sus variantes son la misma
/// baldosa con otro dibujo). Ver el armado de `suelo_variantes`.
///
/// El suelo se pinta LISO y la marca aparece salteada: dice que el suelo tiene
/// textura sin empedrarlo de puntitos. Historia: el 24 ago 2026 el pasto salía
/// con matas en todas las celdas (*"forman figuritas como charquitos"*) y se le
/// puso 7 lisas por mata; el 27 sep las variantes —que eran piezas de COSTURA
/// del pack— se sacaron y TODO quedó liso (*"la nieve no tiene texturas… así
/// con los suelos en general"*, 28 sep). Las marcas de hoy las dibuja
/// `scripts/dibujar-marcas-de-suelo.py`, limpias y sin costura: una cada cuatro
/// celdas, y una cada tres en la nieve, que no tiene el verde que le da vida al
/// pasto.
fn peso_liso(base: &str) -> Option<usize> {
    match base {
        "tiles/ice" => Some(2),
        "tiles/grass" | "tiles/sand" | "tiles/dirt" | "tiles/regolith" | "tiles/basalt" => Some(3),
        b if crate::world2d::PASTOS_DE_BIOMA.contains(&b) => Some(3),
        _ => None,
    }
}

/// Loaded drop-in sprites by logical key. Empty until the player ships art.
#[derive(Resource, Default)]
pub struct ArtLibrary {
    /// **El catálogo entero, sin cargar nada.** Clave lógica → PNG.
    ///
    /// Existe porque el manifest pasó de 296 sprites a 12.000 cuando entró el
    /// pack completo: cargarlos en boot como se hacía antes son 125 páginas de
    /// atlas de 4 MB = medio giga de textura, y el Redmi se muere por OOM mucho
    /// antes de llegar al menú. Ahora el boot sólo lee rutas y la textura se pide
    /// la primera vez que alguien dibuja esa clave (ver [`ArtLibrary::get`]).
    rutas: HashMap<String, FuenteArte>,
    /// Handles ya pedidos — la caché de [`ArtLibrary::get`]. Va detrás de un
    /// `RwLock` porque `get` se llama desde sistemas que sólo tienen `Res<_>`, y
    /// convertirlos todos a `ResMut` serializaría medio frame por un `insert`
    /// que ocurre una vez por clave y partida.
    sprites: RwLock<HashMap<String, Handle<Image>>>,
    /// El servidor de assets, para poder cargar tarde. Es un handle barato de
    /// clonar (`Arc` por dentro).
    servidor: Option<AssetServer>,
    /// Hojas (PNG) que ya se pidieron. Es la cuenta que importa para la memoria:
    /// las claves son miles y comparten página.
    paginas: RwLock<std::collections::HashSet<String>>,
    /// Animación de imagen completa (tira) — compat.
    anims: HashMap<String, ArtAnim>,
    /// Recorte estático px `[x, y, w, h]` (sheets con `rect`).
    rects: HashMap<String, [u32; 4]>,
    /// Tamaño del slot en TILES según su recorte (casa 5×4 → `(5,4)`). Es la
    /// grilla sobre la que se pintó la huella y la base de la colocación
    /// alineada al grid. Ver `slot_cells` / `anchor`.
    cells: HashMap<String, (u32, u32)>,
    /// Animación de sub-región (sheets con `strip`).
    strips: HashMap<String, StripInfo>,
    /// **Borde de 9 partes declarado** (px de la hoja), para los marcos del kit
    /// de interfaz. Lo publica `scripts/import-ui-kit.py`. Sin entrada acá,
    /// `ui_frame_node` cae a la heurística de "una celda de la grilla".
    pub(crate) slices: HashMap<String, [f32; 4]>,
    /// Poses nombradas por slot (aplica a CUALQUIER atlas, no sólo el jugador):
    /// clave del slot → nombre de pose ("idle_down", "walk_left"…) → frame.
    poses: HashMap<String, HashMap<String, PoseInfo>>,
    /// Override de colisión de caminado por slot (editor: casilla "bloquea el
    /// paso"). `None` en el mapa = sin override → el juego usa su default por
    /// tipo. Ver `collides_override`.
    collides: HashMap<String, bool>,
    /// Huella de colisión multi-celda por slot (A1). Vacío = ningún asset la
    /// declara → el juego salta el escaneo de vecindad (cero costo). Ver
    /// `collision_footprint` / `has_footprints`.
    footprints: HashMap<String, ha_assets::CollisionFootprint>,
    /// Lado de la huella más grande declarada (celdas). Lo consulta la colisión en
    /// cada paso para acotar el escaneo de vecindad — por eso está cacheado.
    footprint_reach: i64,
    /// Lo mismo, pero sólo de lo que se SIEMBRA solo (`decor/*`: árboles y rocas). La colisión
    /// procedural escanea con este alcance; el resto (casas autoradas) se busca por lista.
    decor_reach: i64,
    /// Luz que emite cada slot (editor: casilla "emite luz" / pincel de LUZ del
    /// Image Lab). Vacío = ningún asset ilumina → `light2d` no cuelga nada. Ver
    /// `emission` / `emission_first`.
    emissions: HashMap<String, ha_assets::Emission>,
    /// **Capa de dibujo por slot** (editor de assets: combo "capa"). Decide en
    /// qué banda z se planta el asset — `Floor` bajo los pies, `Roof` arriba con
    /// fade. Ausente = `Object` (Y-sort de siempre). Ver `crate::layers`.
    layers: HashMap<String, ha_assets::ArtLayer>,
    /// **Variantes** declaradas por objeto (`"floor/piso"` → madera, piedra…), en
    /// el orden del editor. Incluye las que todavía no tienen arte (para poder
    /// ofrecerlas en un menú y que caigan al genérico). Ver `variants`.
    objects: HashMap<String, Vec<ha_assets::VariantDef>>,
    /// Claves de sprite de las variantes que SÍ tienen arte, por objeto — lo que
    /// se puede dibujar de verdad. Ver `variant_key_seeded`.
    variant_art: HashMap<String, Vec<String>>,
    /// **UV de cada material de SUELO dentro de la hoja `art/pack/suelo.png`.**
    ///
    /// El suelo no se dibuja con sprites: lo dibuja la malla del chunk
    /// (`world2d::build_chunk_mesh`), una sola por chunk. Para que lleve el pixel
    /// del pack sin multiplicar los draw calls, TODO el suelo vive en una hoja y
    /// la malla mete las UVs de la celda que le toca a cada tile. Acá está esa
    /// tabla, ya normalizada y con el medio texel de margen que evita que se
    /// filtre la celda de al lado.
    ///
    /// La celda (0,0) de la hoja es BLANCA y es la que usa todo lo que no tiene
    /// losa asignada: `ColorMaterial` multiplica textura × color de vértice, así
    /// que blanco deja el color plano de siempre intacto. Por eso texturizar el
    /// suelo no cambió el aspecto de lo que todavía no tiene arte.
    suelo_uv: HashMap<String, [f32; 4]>,
    /// Las UVs de TODAS las variantes de cada material de suelo, ya resueltas.
    /// Es lo que deja elegir la losa de una celda sin construir su clave — ver
    /// `suelo_uv_seeded`, y por qué importa.
    suelo_variantes: HashMap<String, Vec<[f32; 4]>>,
    /// ¿Hay alguna losa de pasto POR BIOMA en la hoja?
    ///
    /// Es un atajo de rendimiento, no una decisión de arte. Preguntar "¿de qué
    /// bioma es esta celda?" cuesta media docena de fbm
    /// (`geography::biome_at`), y la malla del chunk lo preguntaría en CADA
    /// celda de pasto — que en un mundo templado son casi todas, 256 por chunk,
    /// cada vez que uno entra al streaming. Con esto en `false` la malla ni
    /// pregunta y el suelo cuesta exactamente lo que costaba antes.
    pasto_por_bioma: bool,
    /// ¿Hay alguna familia de vereda ALTERNATIVA cargada? Mismo atajo que
    /// `pasto_por_bioma`, para el otro lado de la ciudad: sin familias que
    /// repartir, preguntar qué asentamiento cubre una celda de acera son nueve
    /// consultas de sitio para obtener siempre la misma respuesta.
    veredas_de_ciudad: bool,
    /// La hoja del suelo, si el manifest la declara. La pide el material del
    /// chunk una vez (`world2d::sync_chunk_material`).
    suelo_pagina: Option<String>,
    /// Claves cuyo dibujo lo generó `procedural_art` porque el slot estaba vacío.
    /// Se comportan como arte normal (se dibujan, autotilean, colisionan igual)
    /// pero se distinguen para dos cosas: no pisar nunca arte real, y que
    /// `tile_has_art` diga que sí — si no, el mundo saltaría el material entero
    /// y el respaldo no se vería jamás.
    procedural: std::collections::HashSet<String>,
}

impl ArtLibrary {
    /// **Todos** los PNG que el manifest pidió cargar: los del slot y los de las
    /// poses con hoja propia. La pantalla de carga los recorre para no levantar
    /// la cortina con arte a medio llegar — si entra el gameplay antes, el
    /// sprite se dibuja sin textura y aparece de golpe cuando el PNG termina de
    /// decodificarse. Se llama unas pocas veces por partida, no por frame.
    pub fn image_handles(&self) -> Vec<Handle<Image>> {
        let mut out: Vec<Handle<Image>> = self.sprites.read().unwrap().values().cloned().collect();
        out.extend(
            self.poses
                .values()
                .flat_map(|m| m.values())
                .filter_map(|p| p.image.clone()),
        );
        out
    }

    /// Sprite for a logical key ("decor/tree") if the pack ships one.
    ///
    /// **Carga tarde**: la primera vez que se pide una clave se le pide el PNG al
    /// servidor de assets; después sale de la caché. Es lo que deja tener el pack
    /// entero declarado (12.000 claves, 125 páginas de atlas) pagando sólo las
    /// páginas que se están mirando — una calle de ciudad toca tres o cuatro.
    pub fn get(&self, key: &str) -> Option<Handle<Image>> {
        if let Some(h) = self.sprites.read().unwrap().get(key) {
            return Some(h.clone());
        }
        let fuente = self.rutas.get(key)?;
        let servidor = self.servidor.as_ref()?;
        let handle = cargar_png(servidor, fuente);
        self.sprites.write().unwrap().insert(key.to_string(), handle.clone());
        // Una línea por HOJA nueva (no por clave): es el contador que dice cuánta
        // textura está viva de verdad. Con el pack entero declarado, "12.000
        // sprites" no significa nada y "18 páginas" sí — 18 × 4 MB es lo que hay
        // que caber en el Redmi. Sale por el log, que es lo que se mira cuando el
        // teléfono muere por OOM.
        let mut cargadas = self.paginas.write().unwrap();
        if cargadas.insert(fuente.png.clone()) {
            eprintln!("ha-art: hoja {} (viven {})", fuente.png, cargadas.len());
        }
        Some(handle)
    }

    /// Pide de una las texturas de estas claves. La pantalla de carga la usa para
    /// que el arte que el mundo va a sembrar SÍ esté cuando se levanta la
    /// cortina: sin esto la carga perezosa deja aparecer los props de golpe unos
    /// frames después de empezar a jugar.
    pub fn precargar<'a>(&self, claves: impl IntoIterator<Item = &'a str>) {
        for k in claves {
            let _ = self.get(k);
        }
    }

    /// ¿Este slot tiene arte ASIGNADA (real o de respaldo)?
    ///
    /// Mira el CATÁLOGO, no la caché: una clave del pack tiene arte desde que se
    /// importó, la haya dibujado alguien o no. Preguntarle a la caché haría que
    /// el mundo se saltara todo el material que todavía no se cargó.
    pub fn has_art(&self, key: &str) -> bool {
        self.rutas.contains_key(key) || self.sprites.read().unwrap().contains_key(key)
    }

    /// Registra un dibujo generado por código para un slot vacío. No hace nada si
    /// el slot ya tiene arte: **lo curado siempre gana**.
    pub fn set_procedural(&mut self, key: &str, image: Handle<Image>) {
        if self.has_art(key) {
            return;
        }
        self.sprites.write().unwrap().insert(key.to_string(), image);
        self.procedural.insert(key.to_string());
    }

    /// Registra una POSE generada por código (una forma de autotile: `center`,
    /// `edge_s`, `corner_se`…). Misma regla que [`set_procedural`]: si el slot ya
    /// tiene arte de verdad, no se toca.
    ///
    /// Hace falta porque un respaldo de una sola imagen no puede orientarse: el
    /// acantilado se dibujaba idéntico mirara hacia donde mirara. Cada pose trae
    /// su propia hoja de 32×32, así que el rect es la imagen entera.
    pub fn set_procedural_pose(&mut self, key: &str, pose: &str, image: Handle<Image>, px: u32) {
        // `has_art` mira el catálogo (el manifest) y la caché de sprites; las
        // poses de respaldo no entran en ninguno de los dos, así que llamar a
        // esto una vez por forma no se pisa a sí mismo.
        if self.has_art(key) {
            return;
        }
        self.poses.entry(key.to_string()).or_default().insert(
            pose.to_string(),
            PoseInfo { rect_px: [0, 0, px, px], anim: None, layer: None, image: Some(image) },
        );
        self.procedural.insert(key.to_string());
    }

    /// Animation for a key if its PNG is a frame-strip.
    pub fn anim(&self, key: &str) -> Option<ArtAnim> {
        self.anims.get(key).copied()
    }

    /// First key that exists from a fallback chain (most→least specific,
    /// e.g. `["decor/rock_moon", "decor/rock"]`).
    pub fn get_first(&self, keys: &[&str]) -> Option<Handle<Image>> {
        keys.iter().find_map(|k| self.get(k))
    }

    // ── variantes (un objeto, varios materiales/skins) ──────────────────────

    /// Variantes declaradas del objeto, en el orden del editor. Es la lista que un
    /// menú (materiales de construcción, skins) debe ofrecer. Vacío = objeto simple.
    pub fn variants(&self, base: &str) -> &[ha_assets::VariantDef] {
        self.objects.get(base).map(|v| v.as_slice()).unwrap_or(&[])
    }

    /// Clave de sprite para `(objeto, variante)`: la de la variante si tiene arte,
    /// si no la del objeto genérico. Con la clave que devuelve pedís sprite, poses,
    /// colisión, huella y luz por los caminos de siempre — nada más cambia.
    pub fn resolved_key(&self, base: &str, variant: Option<&str>) -> String {
        match variant {
            Some(v) => {
                let k = ha_assets::variant_key(base, v);
                if self.has_art(&k) {
                    k
                } else {
                    base.to_string()
                }
            }
            None => base.to_string(),
        }
    }

    /// Las claves con arte de un objeto con variantes (`objeto#variante`), o la
    /// clave base sola si no tiene. Es la lista entre la que sortea
    /// [`Self::variant_key_seeded`], para quien necesita filtrarla antes.
    pub fn variantes_con_arte(&self, base: &str) -> Vec<String> {
        match self.variant_art.get(base) {
            Some(keys) if !keys.is_empty() => keys.clone(),
            _ => vec![base.to_string()],
        }
    }

    /// Variante elegida **determinísticamente** por una semilla (la celda del
    /// mundo): es lo que hace que un objeto con 3 variantes de arte se vea variado
    /// en el mundo sin guardar nada por instancia — la misma celda elige siempre la
    /// misma. Sin variantes con arte devuelve la clave base tal cual.
    pub fn variant_key_seeded(&self, base: &str, seed: u64) -> String {
        match self.variant_art.get(base) {
            Some(keys) if !keys.is_empty() => {
                // Mezcla barata (splitmix64) para que celdas vecinas no caigan
                // siempre en la misma variante.
                let mut h = seed.wrapping_mul(0x9E37_79B9_7F4A_7C15);
                h ^= h >> 29;
                h = h.wrapping_mul(0xBF58_476D_1CE4_E5B9);
                h ^= h >> 32;
                keys[(h % keys.len() as u64) as usize].clone()
            }
            _ => base.to_string(),
        }
    }

    /// **UV de este material de suelo dentro de [`SUELO_PAGINA`]**, si la tiene.
    /// `None` = ese material no está en la hoja y lo dibuja su color plano.
    pub fn suelo_uv(&self, key: &str) -> Option<[f32; 4]> {
        self.suelo_uv.get(key).copied()
    }

    /// ¿Vale la pena preguntar de qué bioma es una celda de pasto? Ver el campo.
    pub fn pasto_por_bioma(&self) -> bool {
        self.pasto_por_bioma
    }

    /// ¿Vale la pena preguntar de qué ciudad es una celda de vereda? Ver el campo.
    pub fn veredas_de_ciudad(&self) -> bool {
        self.veredas_de_ciudad
    }

    /// La UV de la LOSA que le toca a una celda: la variante elegida por semilla,
    /// **sin construir la clave**.
    ///
    /// `variant_key_seeded` devuelve un `String`, y la malla del chunk pregunta
    /// una vez por tile: 256 asignaciones de memoria por chunk, cada vez que se
    /// reconstruye. Medido en el Redmi eso puso `build_chunk_mesh` en 4 ms por
    /// frame (fps 30 con la regla en 60). Acá las variantes ya están resueltas a
    /// UV en un vector y se elige por índice — cero asignaciones.
    pub fn suelo_uv_seeded(&self, key: &str, seed: u64) -> Option<[f32; 4]> {
        let uvs = self.suelo_variantes.get(key)?;
        if uvs.is_empty() {
            return None;
        }
        // Misma mezcla que `variant_key_seeded`, para que la celda elija SIEMPRE
        // la misma losa que elegiría por el camino de las claves.
        let mut h = seed.wrapping_mul(0x9E37_79B9_7F4A_7C15);
        h ^= h >> 29;
        h = h.wrapping_mul(0xBF58_476D_1CE4_E5B9);
        h ^= h >> 32;
        Some(uvs[(h % uvs.len() as u64) as usize])
    }

    /// La hoja del suelo, pedida al asset server (la carga perezosa la trae la
    /// primera vez). `None` = el manifest no declara ningún suelo en hoja.
    pub fn suelo_pagina(&self) -> Option<Handle<Image>> {
        let png = self.suelo_pagina.as_ref()?;
        let servidor = self.servidor.as_ref()?;
        self.paginas.write().ok()?.insert(png.clone());
        Some(servidor.load(png.clone()))
    }

    /// Como `get_first`, pero por cada clave de la cadena prueba primero una de sus
    /// variantes (elegida por `seed`). Así el decor procedural (rocas, árboles) usa
    /// las variantes que cargaste en el editor sin tocar la generación del mundo.
    pub fn get_first_seeded(&self, keys: &[&str], seed: u64) -> Option<Handle<Image>> {
        keys.iter().find_map(|k| {
            let vk = self.variant_key_seeded(k, seed);
            self.get(&vk).or_else(|| self.get(k))
        })
    }

    /// Sprite de la primera clave con arte, **con su recorte aplicado**.
    ///
    /// Es el hermano correcto de `get_first_seeded`: ése devuelve sólo el
    /// `Handle<Image>`, y quien lo usaba armaba el sprite a mano con
    /// `Sprite::from_image` — o sea, SIN el `rect`. Mientras cada slot tenía su PNG
    /// propio eso funcionaba de casualidad (la imagen ERA el sprite). Al empaquetar
    /// la decoración en una hoja (`scripts/pack-world-atlas.py`) dejó de serlo y el
    /// árbol pasó a dibujar el atlas entero encogido: fragmentos de basura.
    pub fn sprite_first_seeded(&self, keys: &[&str], seed: u64, size: Vec2) -> Option<Sprite> {
        keys.iter().find_map(|k| {
            let vk = self.variant_key_seeded(k, seed);
            for cand in [vk.as_str(), k] {
                if let Some(image) = self.get(cand) {
                    let mut s = Sprite::from_image(image);
                    s.custom_size = Some(size);
                    if let Some([x, y, w, h]) = self.rects.get(cand).copied() {
                        s.rect = Some(Rect::from_corners(
                            Vec2::new(x as f32, y as f32),
                            Vec2::new((x + w) as f32, (y + h) as f32),
                        ));
                    }
                    return Some(s);
                }
            }
            None
        })
    }

    /// `world_sprite` de una variante concreta (o del genérico si esa variante no
    /// tiene arte).
    pub fn world_sprite_variant(
        &self,
        base: &str,
        variant: Option<&str>,
        size: Vec2,
    ) -> Option<(Sprite, Option<ArtAnimated>)> {
        self.world_sprite(&self.resolved_key(base, variant), size)
    }

    /// `world_sprite` con la variante elegida por la celda (variedad ambiental).
    pub fn world_sprite_seeded(
        &self,
        base: &str,
        seed: u64,
        size: Vec2,
    ) -> Option<(Sprite, Option<ArtAnimated>)> {
        self.world_sprite(&self.variant_key_seeded(base, seed), size)
    }

    /// Override de colisión que el usuario marcó en el editor para este slot:
    /// `Some(true/false)` si lo fijó, `None` si no → el que llama usa su default
    /// por tipo. Prueba las claves en orden (específica → genérica).
    pub fn collides_override(&self, keys: &[&str]) -> Option<bool> {
        keys.iter().find_map(|k| self.collides.get(*k).copied())
    }

    /// Huella de colisión multi-celda del primer slot de la cadena que la tenga
    /// (A1). `None` = ninguno la declara → colisión de 1 celda por defecto.
    pub fn collision_footprint(&self, keys: &[&str]) -> Option<&ha_assets::CollisionFootprint> {
        keys.iter().find_map(|k| self.footprints.get(*k))
    }

    /// ¿Algún slot declara huella multi-celda? Si no, el juego evita el escaneo
    /// de vecindad en la colisión de decor (cero costo hasta que se pinte una).
    pub fn has_footprints(&self) -> bool {
        !self.footprints.is_empty()
    }

    /// Cuántas celdas alrededor hay que mirar para no perderse una huella: la más
    /// grande declarada. 0 = ninguna huella → la colisión sólo mira la celda de al
    /// lado (lo barato, que es lo que corre en el Redmi).
    pub fn footprint_reach(&self) -> i64 {
        self.footprint_reach
    }

    pub fn decor_reach(&self) -> i64 {
        self.decor_reach
    }

    /// Tamaño del slot en TILES según su recorte (una casa de 5×4 celdas mide
    /// 5×4). `None` = PNG suelto sin recorte: el que llama lo deduce del tamaño
    /// con el que lo dibuja.
    pub fn slot_cells(&self, key: &str) -> Option<(u32, u32)> {
        self.cells.get(key).copied()
    }

    /// **Tamaño con el que hay que DIBUJAR este arte, en tiles** — el del pixel,
    /// no el de la huella.
    ///
    /// La huella de un colocable es un rectángulo de tiles ENTEROS (es lo que
    /// ocupa y lo que frena), y hasta acá el sprite se estiraba a ella. Como el
    /// arte de LimeZu está dibujado a 32 px por celda y una celda son
    /// [`ESCALA_ARTE`] tiles, redondear a entero deforma: una silla de 1×2 celdas
    /// (huella 2×2) se dibujaba con **el doble de ancho**. Medido sobre la tabla
    /// entera: 2.713 de 7.710 colocables salían deformados, algunos al doble.
    ///
    /// Devuelve el tamaño de la POSE que se va a dibujar si existe (una cama de
    /// costado tiene su propio recorte), si no el del slot. `None` = arte sin
    /// recorte declarado (un PNG suelto curado a mano): ahí el dibujo ES la
    /// imagen y quien llama se queda con su tamaño de siempre.
    pub fn draw_tiles(&self, key: &str, pose: Option<&str>) -> Option<Vec2> {
        Some(self.celdas_de(key, pose)? * ESCALA_ARTE)
    }

    /// **Cuántas CELDAS DEL PACK mide el recorte** (la pose si existe, si no el
    /// slot). Es [`Self::draw_tiles`] antes de aplicar ninguna escala: el dato
    /// crudo del dibujo, para quien tenga que convertirlo con otra regla.
    pub fn celdas_de(&self, key: &str, pose: Option<&str>) -> Option<Vec2> {
        let px = pose
            .and_then(|p| self.poses.get(key)?.get(p))
            .map(|p| (p.rect_px[2], p.rect_px[3]))
            .or_else(|| self.cells.get(key).map(|&(w, h)| (w * 32, h * 32)))?;
        if px.0 == 0 || px.1 == 0 {
            return None;
        }
        Some(Vec2::new(px.0 as f32, px.1 as f32) / 32.0)
    }

    /// **La medida de una PIEZA DE OBRA: una celda del pack = un tile de la
    /// grilla.**
    ///
    /// Una pared, un vano o una ventana no son props sueltos: son piezas que
    /// **embaldosan a lo largo de un muro**, y una pieza que embaldosa tiene que
    /// medir lo que mide su ranura de la grilla o no encaja con la de al lado.
    /// Con [`Self::tamano_real`] (que multiplica por [`ESCALA_ARTE`] = 1,5) una
    /// pared de una celda salía de 1,5 m sobre un borde de 1 m, así que **cada
    /// pared nueva se montaba medio metro encima de la anterior**: ponías tres en
    /// fila y la segunda tapaba a la primera y la tercera a la segunda.
    ///
    /// Es la misma excepción que ya declara `tamano_real` para las superficies
    /// (un piso mide la celda "porque si no, no embaldosa"); lo que faltaba era
    /// aplicarla también a lo que embaldosa en VERTICAL.
    ///
    /// La proporción del dibujo se respeta entera: el recorte de 1×2 celdas de
    /// `wall/wood` sale de 1×2 tiles, o sea más alto que el jugador (1,9 m) y sin
    /// un pixel estirado. Y una pieza de 2 celdas de ancho —el vano, que trae sus
    /// dos jambas— ocupa 2 tiles y está bien: lo que no puede es solaparse con la
    /// que sigue.
    pub fn medida_de_obra(&self, key: &str, pose: Option<&str>, respaldo: Vec2) -> Vec2 {
        self.celdas_de(key, pose)
            .or_else(|| self.celdas_de(key, None))
            .map(|c| c * ha_shared::topdown::TILE_M)
            .unwrap_or(respaldo)
    }

    /// **El tamaño con el que se dibuja un OBJETO del mundo: el que mide su
    /// recorte.** Regla del usuario, sin matices: *"los assets no los podés
    /// escalar a pequeño, usás el tamaño real"*.
    ///
    /// Es [`Self::draw_tiles`] con un respaldo, y existe para que la regla tenga
    /// UNA casa. La forma en que se rompía era siempre la misma: cada sitio que
    /// dibujaba algo elegía un tamaño "que quedara bien" —`Vec2::splat(1.1)` para
    /// un cofre, `TILE_M × WALL_RENDER_H` para una puerta— y el resultado era un
    /// mundo donde cada cosa estaba a una escala distinta y casi todas más chicas
    /// que el personaje. El respaldo sólo entra cuando el slot **no tiene recorte
    /// declarado**, que es el único caso en el que no hay tamaño real que respetar.
    ///
    /// ⚠ **No vale para SUPERFICIES.** Un piso, un techo o un tumbado no son
    /// objetos: son la textura de una celda, y su tamaño lo manda la celda por
    /// definición (si no, no embaldosan). Esos siguen midiendo `TILE_M`, igual que
    /// los quads del suelo en `world2d::build_chunk_mesh`.
    pub fn tamano_real(&self, key: &str, pose: Option<&str>, respaldo: Vec2) -> Vec2 {
        self.draw_tiles(key, pose)
            .or_else(|| self.draw_tiles(key, None))
            .unwrap_or(respaldo)
    }

    /// Celda ANCLA del slot dentro de su grilla: la que queda sobre la celda que
    /// apuntaste al colocarlo. La de su huella si el usuario la pintó; si no, la
    /// columna del medio y la fila del pie.
    pub fn anchor(&self, key: &str, cells: (u32, u32)) -> (u32, u32) {
        match self.footprints.get(key) {
            Some(f) => (f.ax.min(cells.0.saturating_sub(1)), f.az.min(cells.1.saturating_sub(1))),
            None => ha_assets::CollisionFootprint::default_anchor(cells.0, cells.1),
        }
    }

    /// **Capa de dibujo del slot** (editor de assets). `None` = sin etiquetar →
    /// quien la use debe tratarlo como `Object` (ver `crate::layers::depth`).
    /// La variante hereda la capa del objeto base: `floor/piso#madera` y
    /// `floor/piso#piedra` son el mismo mueble, no dos capas distintas.
    pub fn slot_layer(&self, key: &str) -> Option<ha_assets::ArtLayer> {
        if let Some(l) = self.layers.get(key) {
            return Some(*l);
        }
        let (base, _) = ha_assets::split_variant(key);
        self.layers
            .get(base)
            .copied()
            // Convención de namespace: un `floor/*` es piso aunque nadie lo haya
            // etiquetado. Ver `ha_assets::namespace_layer`.
            .or_else(|| ha_assets::namespace_layer(base))
    }

    /// Luz declarada por el slot, si el usuario la activó en el editor.
    pub fn emission(&self, key: &str) -> Option<&ha_assets::Emission> {
        self.emissions.get(key)
    }

    /// Luz del primer slot de la cadena de fallback que la declare (misma cadena
    /// que usás para el sprite). `None` = ese asset no ilumina.
    pub fn emission_first(&self, keys: &[&str]) -> Option<&ha_assets::Emission> {
        if self.emissions.is_empty() {
            return None; // atajo: ningún asset del pack ilumina
        }
        keys.iter().find_map(|k| self.emissions.get(*k))
    }

    /// Sprite component for a key with a fixed world size, or the plain
    /// colored-quad fallback. Aplica el recorte `rect` si el slot es un sheet.
    pub fn sprite_or_color(&self, key: &str, color: Color, size: Vec2) -> Sprite {
        match self.get(key) {
            Some(image) => {
                let mut s = Sprite::from_image(image);
                s.custom_size = Some(size);
                if let Some([x, y, w, h]) = self.rects.get(key).copied() {
                    s.rect = Some(Rect::from_corners(
                        Vec2::new(x as f32, y as f32),
                        Vec2::new((x + w) as f32, (y + h) as f32),
                    ));
                }
                s
            }
            None => Sprite::from_color(color, size),
        }
    }

    /// Componente de animación para un sprite del slot, si trae animación (tira
    /// de sub-región `strip`, o imagen completa). Insertalo junto al sprite.
    pub fn animation(&self, key: &str) -> Option<ArtAnimated> {
        if let Some(s) = self.strips.get(key) {
            return Some(ArtAnimated::from_strip(*s));
        }
        self.anims.get(key).map(|a| ArtAnimated::new(*a))
    }

    /// `true` si el slot trae poses guardadas (personaje CON direcciones, o
    /// cualquier otro atlas con estados). Genérico — no sólo el jugador.
    pub fn has_poses(&self, key: &str) -> bool {
        self.poses.get(key).map(|m| !m.is_empty()).unwrap_or(false)
    }

    pub fn has_pose(&self, key: &str, name: &str) -> bool {
        self.poses.get(key).map(|m| m.contains_key(name)).unwrap_or(false)
    }

    /// `(frames, fps)` de una pose animada. Lo usa quien necesita saber CUÁNTO
    /// dura la animación para sincronizar el estado del juego con ella — la
    /// transición de una puerta, por ejemplo: el juego la da por abierta cuando
    /// termina de verse abriéndose, no un número fijo de segundos después.
    pub fn pose_frames(&self, key: &str, name: &str) -> Option<(u32, f32)> {
        let s = self.poses.get(key)?.get(name)?.anim?;
        Some((s.frames.max(1), s.fps))
    }

    /// Rect px de la pose + animación lista para insertar si es una tira.
    /// `None` si el slot/pose no existe.
    pub fn pose(&self, key: &str, name: &str) -> Option<([u32; 4], Option<ArtAnimated>)> {
        let p = self.poses.get(key)?.get(name)?;
        Some((p.rect_px, p.anim.map(ArtAnimated::from_strip)))
    }

    /// Imagen que le corresponde a una pose: la hoja PROPIA de la pose si el
    /// asset se armó con varios PNG (uno por dirección/estado), si no la del
    /// slot. Quien cambia de pose debe asignar esto a `sprite.image` — si no, se
    /// vería el recorte de la pose nueva sobre la hoja vieja.
    pub fn pose_image(&self, key: &str, name: &str) -> Option<Handle<Image>> {
        match self.poses.get(key).and_then(|m| m.get(name)) {
            Some(PoseInfo { image: Some(img), .. }) => Some(img.clone()),
            _ => self.get(key),
        }
    }

    /// Capa etiquetada de una pose (editor F2). `None` = sin etiqueta (Object).
    pub fn pose_layer(&self, key: &str, name: &str) -> Option<ha_assets::ArtLayer> {
        self.poses.get(key)?.get(name)?.layer
    }

    /// Primera pose del slot etiquetada con `layer` (p.ej. el techo de un
    /// edificio, sin importar cómo la nombró el usuario): su nombre y su rect px.
    /// El nombre hace falta para pedirle su hoja a `pose_image` cuando el asset
    /// está repartido en varios PNG.
    pub fn pose_with_layer_named(
        &self,
        key: &str,
        layer: ha_assets::ArtLayer,
    ) -> Option<(&str, [u32; 4])> {
        let m = self.poses.get(key)?;
        // Nombre canónico primero ("roof"/"base"), después cualquier etiquetada.
        let canonical = match layer {
            ha_assets::ArtLayer::Roof => "roof",
            ha_assets::ArtLayer::Floor => "floor",
            ha_assets::ArtLayer::Basement => "basement",
            ha_assets::ArtLayer::Wall => "base",
            ha_assets::ArtLayer::Object => "object",
            ha_assets::ArtLayer::Light => "light",
        };
        if let Some(p) = m.get(canonical) {
            if p.layer.map(|l| l == layer).unwrap_or(true) {
                return Some((canonical, p.rect_px));
            }
        }
        m.iter()
            .find(|(_, p)| p.layer == Some(layer))
            .map(|(n, p)| (n.as_str(), p.rect_px))
    }

    /// **Una TIRA de una pose, estirada al tamaño pedido.**
    ///
    /// `franja` es `[u0, v0, u1, v1]` en fracciones del recorte de la pose (0..1,
    /// origen arriba-izquierda). Sirve para sacar material de una textura sin
    /// heredar el dibujo que la orienta: la columna central de una pared de
    /// tablones es tablón y nada más —sin el marco ni los remates que la hacen
    /// leerse "de frente"—, así que estirada da la cara LATERAL del mismo muro,
    /// con el color y la veta que el usuario le asignó.
    ///
    /// Es el camino que evita las dos salidas malas: inventar un PNG (regla 7) y
    /// pedirle al usuario que recorte cuatro poses por cada material que exista.
    /// Si el usuario recorta la pose de verdad, gana la suya — esto es el
    /// respaldo, no el reemplazo.
    pub fn pose_sprite_slice(
        &self,
        key: &str,
        name: &str,
        size: Vec2,
        franja: [f32; 4],
    ) -> Option<Sprite> {
        let (rect_px, _) = self.pose(key, name)?;
        let image = self.pose_image(key, name)?;
        let [x, y, w, h] = rect_px;
        let (w, h) = (w as f32, h as f32);
        let [u0, v0, u1, v1] = franja;
        // Al menos un texel: una franja que se redondea a cero deja el sprite sin
        // recorte y dibujaría el ATLAS ENTERO — el bug clásico de este camino.
        let x0 = x as f32 + w * u0.clamp(0.0, 1.0);
        let y0 = y as f32 + h * v0.clamp(0.0, 1.0);
        let x1 = (x as f32 + w * u1.clamp(0.0, 1.0)).max(x0 + 1.0);
        let y1 = (y as f32 + h * v1.clamp(0.0, 1.0)).max(y0 + 1.0);
        let mut s = Sprite::from_image(image);
        s.custom_size = Some(size);
        s.rect = Some(Rect::from_corners(Vec2::new(x0, y0), Vec2::new(x1, y1)));
        Some(s)
    }

    /// Sprite recortado de una pose concreta con tamaño en mundo, si existe.
    pub fn pose_sprite(&self, key: &str, name: &str, size: Vec2) -> Option<Sprite> {
        let (rect_px, _) = self.pose(key, name)?;
        let image = self.pose_image(key, name)?;
        let mut s = Sprite::from_image(image);
        s.custom_size = Some(size);
        let [x, y, w, h] = rect_px;
        s.rect = Some(Rect::from_corners(
            Vec2::new(x as f32, y as f32),
            Vec2::new((x + w) as f32, (y + h) as f32),
        ));
        Some(s)
    }

    /// Sprite "tal como se ve este asset" para plantarlo en el mundo: aplica su
    /// recorte de atlas, o su pose representativa (`idle_down`/`center`/la 1ª, con
    /// la hoja que le corresponda), y devuelve la animación si la tiene. Sin esto,
    /// un slot recortado de un atlas se dibujaría con el ATLAS ENTERO. `None` =
    /// el slot no tiene arte.
    pub fn world_sprite(&self, key: &str, size: Vec2) -> Option<(Sprite, Option<ArtAnimated>)> {
        // Pose representativa primero (personajes/NPC colocados a mano).
        if let Some(name) = ["idle_down", "center", "base"]
            .into_iter()
            .find(|n| self.has_pose(key, n))
            .or_else(|| self.poses.get(key).and_then(|m| m.keys().next().map(|s| s.as_str())))
        {
            if let Some((rect_px, anim)) = self.pose(key, name) {
                let image = self.pose_image(key, name)?;
                let mut s = Sprite::from_image(image);
                s.custom_size = Some(size);
                if anim.is_none() {
                    let [x, y, w, h] = rect_px;
                    s.rect = Some(Rect::from_corners(
                        Vec2::new(x as f32, y as f32),
                        Vec2::new((x + w) as f32, (y + h) as f32),
                    ));
                }
                return Some((s, anim));
            }
        }
        let image = self.get(key)?;
        let mut s = Sprite::from_image(image);
        s.custom_size = Some(size);
        let anim = self.animation(key);
        // El recorte estático sólo aplica si no hay tira (la tira maneja su rect).
        if anim.is_none() {
            if let Some([x, y, w, h]) = self.rects.get(key).copied() {
                s.rect = Some(Rect::from_corners(
                    Vec2::new(x as f32, y as f32),
                    Vec2::new((x + w) as f32, (y + h) as f32),
                ));
            }
        }
        Some((s, anim))
    }

    /// `ImageNode` de UI para un slot `ui/*` (respeta el recorte de sheet si el
    /// usuario lo recortó de un atlas en el editor). `None` = sin arte → el HUD
    /// mantiene su dibujo procedural (cero regresión).
    /// **El recorte de un slot de UI, venga de donde venga.**
    ///
    /// `self.rects` sólo se llena con el `rect` PLANO del manifest. Los marcos de
    /// UI del pack están curados como POSES (`poses.center.rect`) porque son
    /// autotile de 9 partes, así que para ellos ese mapa está vacío — y un
    /// `ImageNode` sin `rect` dibuja **la hoja entera** escalada al widget.
    ///
    /// Eso es exactamente lo que se veía en el Redmi el 15 ago 2026: las diez
    /// casillas de la hotbar con un tilesheet adentro, el joystick con la lámina
    /// de gamepads del pack y el botón SALTAR encima de la misma lámina. Tres
    /// "bugs" distintos que eran uno solo, y no se veía en el escritorio porque
    /// ahí el arte del pack ni se cargaba.
    fn rect_de_ui(&self, key: &str) -> Option<[u32; 4]> {
        if let Some(r) = self.rects.get(key).copied() {
            return Some(r);
        }
        let poses = self.poses.get(key)?;
        // `center` es la pose del medio de un autotile, que es la que vale como
        // imagen suelta. Si el slot tiene una sola pose, ésa.
        poses
            .get("center")
            .or_else(|| poses.values().next().filter(|_| poses.len() == 1))
            .map(|p| p.rect_px)
    }

    pub fn ui_image_node(&self, key: &str) -> Option<bevy::ui::widget::ImageNode> {
        let image = self.get(key)?;
        let mut node = bevy::ui::widget::ImageNode::new(image);
        if let Some([x, y, w, h]) = self.rect_de_ui(key) {
            node.rect = Some(Rect::from_corners(
                Vec2::new(x as f32, y as f32),
                Vec2::new((x + w) as f32, (y + h) as f32),
            ));
        }
        Some(node)
    }

    /// **Un marco de UI en 9 PARTES** (`ui/panel`, `ui/button`, `ui/slot_empty`).
    ///
    /// Un `ImageNode` a secas ESTIRA la imagen: un panel de 96×64 px estirado a
    /// 800×400 sale con el borde deformado y las esquinas hechas puré. Por eso el
    /// arte de marco recortado en el editor no se estaba usando y todo el HUD
    /// seguía dibujándose con los colores planos de `ui_theme` — el arte estaba,
    /// faltaba la forma de estirarlo bien.
    ///
    /// El corte sale del recorte que hizo el usuario: se reserva **una celda de
    /// la grilla** (típicamente 32 px) para el borde y el resto es el centro, que
    /// es lo que se repite. Si el recorte mide menos de tres celdas de lado, el
    /// borde se achica para que quede centro: un marco 1×1 no tiene 9 partes.
    pub fn ui_frame_node(&self, key: &str) -> Option<bevy::ui::widget::ImageNode> {
        use bevy::ui::widget::NodeImageMode;
        use bevy::sprite::{BorderRect, SliceScaleMode, TextureSlicer};
        let mut node = self.ui_image_node(key)?;
        // Mismo respaldo que `ui_image_node`: los marcos del pack guardan su
        // recorte como pose, no como `rect` plano. Con `self.rects` a secas esto
        // devolvía `None` y el botón se quedaba con la hoja entera estirada.
        let [_, _, w, h] = self.rect_de_ui(key)?;
        // La celda de la grilla es el borde natural del marco. Sin recorte
        // declarado no hay nada que cortar: se devuelve el nodo estirado.
        // **El borde declarado gana.** Las piezas del kit de interfaz
        // (`ui/marco_*`) no están alineadas a la grilla de 32 y su borde va de 2
        // a 11 px: adivinarlo con la celda dejaba el panel todo marco. Lo
        // publica el importador, medido.
        let declarado = self.slices.get(key).copied();
        let [bl, br, bt, bb] = declarado.unwrap_or_else(|| {
            let cell = self.cells.get(key).copied().unwrap_or((1, 1));
            let (cw, ch) = (w / cell.0.max(1), h / cell.1.max(1));
            let bx = (cw as f32).min(w as f32 / 3.0).max(1.0);
            let by = (ch as f32).min(h as f32 / 3.0).max(1.0);
            [bx, bx, by, by]
        });
        node.image_mode = NodeImageMode::Sliced(TextureSlicer {
            border: BorderRect { left: bl, right: br, top: bt, bottom: bb },
            // El centro y los bordes se ESTIRAN, no se azulejan: con pixel art a
            // escalas no enteras, repetir deja costuras visibles entre copias.
            center_scale_mode: SliceScaleMode::Stretch,
            sides_scale_mode: SliceScaleMode::Stretch,
            // 4.0 dejaba una esquina de 128 px de arte a 32: el marco se comía
            // el panel. 2.0 sigue el escalado de UI del teléfono sin engordar.
            //
            // Los marcos del KIT son otra cosa: se piden a la escala de pixel-art
            // de la pantalla (x3 en el Redmi apaisado) y su borde ya es chico —
            // 8 px de botón a 2x sale de 16 y el mockup los tiene de 24. Con el
            // borde declarado, el tope sube para que la esquina llegue a su
            // escala real.
            max_corner_scale: if declarado.is_some() { crate::ui_pack::ESQUINA_SIN_AJUSTAR } else { 2.0 },
        });
        Some(node)
    }

    /// **El icono de UI de un asset, tal como se ve.** Es el gemelo de
    /// [`Self::world_sprite`] para un `ImageNode`: resuelve el PRIMER frame si el
    /// slot es una tira, la pose representativa si tiene poses (el `center` de un
    /// autotile, el `idle_down` de un personaje) y el recorte de atlas si se curó
    /// desde una hoja. Sin esto, una tarjeta de craft o de construir mostraba la
    /// HOJA ENTERA escalada a 40 px, que es justo el bug de "usa mal los assets".
    pub fn ui_icon(&self, key: &str) -> Option<bevy::ui::widget::ImageNode> {
        let px = |a: UVec2, b: UVec2| Some(Rect::from_corners(a.as_vec2(), b.as_vec2()));
        // 1) Tira de animación: un solo frame, no la fila completa.
        if let Some(s) = self.strips.get(key) {
            if s.cell.x > 0 && s.cell.y > 0 {
                let mut node = bevy::ui::widget::ImageNode::new(self.get(key)?);
                node.rect = px(s.origin, s.origin + s.cell);
                return Some(node);
            }
        }
        // 2) Pose representativa. El orden es el mismo criterio que en el mundo, y
        //    si el usuario nombró sus poses de otra forma se toma la primera.
        let pose_name = ["center", "idle_down", "base", "closed"]
            .into_iter()
            .find(|n| self.has_pose(key, n))
            .map(|n| n.to_string())
            .or_else(|| self.poses.get(key).and_then(|m| m.keys().next().cloned()));
        if let Some(name) = pose_name {
            if let Some(p) = self.poses.get(key).and_then(|m| m.get(&name)) {
                let mut node = bevy::ui::widget::ImageNode::new(self.pose_image(key, &name)?);
                node.rect = match p.anim {
                    Some(s) if s.cell.x > 0 && s.cell.y > 0 => px(s.origin, s.origin + s.cell),
                    _ => {
                        let [x, y, w, h] = p.rect_px;
                        px(UVec2::new(x, y), UVec2::new(x + w, y + h))
                    }
                };
                return Some(node);
            }
        }
        // 3) Slot plano (con su recorte si lo tiene).
        self.ui_image_node(key)
    }

    /// `true` si el material de tile trae arte "vivo" (autotile por poses o una
    /// animación): sólo entonces el mundo lo dibuja como sprite en vez de dejar
    /// el quad vertex-color barato. Un PNG estático plano NO califica (evita
    /// spawnear miles de sprites por el suelo base). Ver `world2d::spawn_chunk_tile_art`.
    /// **Cuánto mide, en píxeles de la hoja, el icono de este slot.**
    ///
    /// Es lo que hace falta para dibujar un sprite del catálogo a escala entera
    /// (x2, x3, x5) sin deformarlo: el nodo de UI se pide del tamaño del recorte
    /// por la escala, no de un cuadrado inventado. Lo usa el parallax del menú.
    pub fn ui_icon_px(&self, key: &str) -> Option<Vec2> {
        self.ui_icon(key)?.rect.map(|r| r.size())
    }

    pub fn tile_has_art(&self, key: &str) -> bool {
        self.has_poses(key)
            || self.anims.contains_key(key)
            || self.strips.contains_key(key)
            // Los respaldos dibujados por código no tienen poses ni tira, pero SÍ
            // hay que dibujarlos: sin esta rama el mundo se saltaba el material y
            // un acantilado seguía siendo el cuadrado marrón de siempre.
            || self.procedural.contains(key)
    }

    /// Sprite (+ animación) para un tile de material `key` con la forma autotile
    /// `shape` (center/edge_n/corner_ne/…). Cae a la pose "center", y si no hay
    /// poses, al sprite plano del slot. `None` = sin arte (usar vertex-color).
    pub fn tile_sprite(&self, key: &str, shape: &str, size: Vec2) -> Option<(Sprite, Option<ArtAnimated>)> {
        // La forma pedida puede venir de su propia hoja (autotile repartido en
        // varios PNG) → la imagen se elige DESPUÉS de saber qué pose se usa.
        let used = [shape, "center"].into_iter().find(|n| self.has_pose(key, n));
        let image = match used {
            Some(name) => self.pose_image(key, name)?,
            None => self.get(key)?,
        };
        let mut s = Sprite::from_image(image);
        s.custom_size = Some(size);
        // Preferí la forma pedida; si falta, probá "center"; si no hay poses, plano.
        if let Some((rect_px, anim)) = self.pose(key, shape).or_else(|| self.pose(key, "center")) {
            if anim.is_none() {
                let [x, y, w, h] = rect_px;
                s.rect = Some(Rect::from_corners(
                    Vec2::new(x as f32, y as f32),
                    Vec2::new((x + w) as f32, (y + h) as f32),
                ));
            }
            return Some((s, anim));
        }
        // Sin poses: recorte estático del slot (si tiene) + animación de tira.
        if let Some([x, y, w, h]) = self.rects.get(key).copied() {
            s.rect = Some(Rect::from_corners(
                Vec2::new(x as f32, y as f32),
                Vec2::new((x + w) as f32, (y + h) as f32),
            ));
        }
        Some((s, self.animation(key)))
    }
}

/// Cómo se reproduce una tira de frames.
#[derive(Clone, Copy, PartialEq, Eq, Default)]
pub enum PlayMode {
    /// Cicla para siempre (agua, fuego, caminar): el default de todo el arte.
    #[default]
    Loop,
    /// Corre UNA vez y se queda clavada en el último frame. Es lo que hace que
    /// un cofre/puerta "termine de abrirse" y se mantenga abierto mientras lo
    /// usás: el estado lo decide el juego, la animación sólo es la transición.
    OnceHold,
}

/// Sprite animado por tira horizontal de N frames a F fps. El atlas se arma solo
/// cuando la imagen termina de cargar (`setup_art_atlases`).
#[derive(Component)]
pub struct ArtAnimated {
    frames: u32,
    timer: Timer,
    origin: UVec2,
    cell: UVec2, // ZERO → derivar de la imagen completa
    step: UVec2, // desplazamiento px por frame (ZERO → derivar horizontal)
    ready: bool,
    mode: PlayMode,
    /// Sólo con `OnceHold`: ya llegó al último frame y ahí se queda.
    done: bool,
    /// Quieta en el cuadro 0 hasta que alguien la toque (muebles: ver
    /// `harvest::animar_mueble_al_tocar`).
    pausada: bool,
}

impl ArtAnimated {
    pub fn new(a: ArtAnim) -> Self {
        Self {
            frames: a.frames.max(1),
            timer: Timer::from_seconds(1.0 / a.fps.max(0.1), TimerMode::Repeating),
            origin: UVec2::ZERO,
            cell: UVec2::ZERO,
            step: UVec2::ZERO,
            ready: false,
            mode: PlayMode::Loop,
            done: false,
            pausada: false,
        }
    }

    fn from_strip(s: StripInfo) -> Self {
        Self {
            frames: s.frames.max(1),
            timer: Timer::from_seconds(1.0 / s.fps.max(0.1), TimerMode::Repeating),
            origin: s.origin,
            cell: s.cell,
            step: s.step,
            ready: false,
            mode: PlayMode::Loop,
            done: false,
            pausada: false,
        }
    }

    /// Reproducir una sola vez y quedarse en el último frame (abrir/cerrar).
    pub fn once(mut self) -> Self {
        self.mode = PlayMode::OnceHold;
        self
    }

    /// `true` cuando una `OnceHold` ya terminó su recorrido.
    pub fn done(&self) -> bool {
        self.done
    }

    /// Arranca quieta en el cuadro 0: se anima sólo cuando se la reanuda.
    pub fn quieta(mut self) -> Self {
        self.pausada = true;
        self
    }

    pub fn pausada(&self) -> bool {
        self.pausada
    }

    /// Alterna quieta/animada. Al pararla vuelve al cuadro 0 (el mueble "cerrado",
    /// "apagado"), que es como estaba antes de tocarlo.
    pub fn alternar(&mut self, sprite: &mut Sprite) {
        self.pausada = !self.pausada;
        if self.pausada {
            if let Some(atlas) = sprite.texture_atlas.as_mut() {
                atlas.index = 0;
            }
        }
    }
}

pub struct Art2dPlugin;

impl Plugin for Art2dPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<ArtLibrary>()
            .add_systems(PreStartup, load_art_manifest)
            .add_systems(Startup, preload_item_icons)
            .add_systems(Update, (setup_art_atlases, tick_art_animations).chain())
            .add_systems(Update, medir_ram_de_imagenes);
    }
}

/// Cuánta RAM tienen las imágenes **del lado de la CPU**, que es la mitad de la cuenta que
/// no se ve. Bevy carga con `MAIN_WORLD | RENDER_WORLD` por defecto: sube la textura a la
/// GPU **y** se queda con los píxeles en RAM. Si nadie los lee, esa copia es puro peso.
///
/// Vale la pena medirlo porque el Redmi murió por OOM (biblia §12) con ~492 MB de heap
/// nativo, y el arte del manifest son ~222 MB decodificados: si aparece dos veces, ahí está
/// casi todo el presupuesto.
///
/// Se imprime cada 30 s con `HA_RAMART=1`, no en el arranque: las texturas entran de a
/// poco y una sola foto temprana miente.
fn medir_ram_de_imagenes(
    images: Res<Assets<Image>>,
    time: Res<Time>,
    mut acc: Local<f32>,
    mut primera: Local<bool>,
) {
    if crate::flags::var("HA_RAMART").is_err() {
        return;
    }
    *acc += time.delta_secs();
    if *primera && *acc < 30.0 {
        return;
    }
    *primera = true;
    *acc = 0.0;
    let mut total = 0usize;
    let mut n = 0usize;
    let mut mayor = 0usize;
    for (_, img) in images.iter() {
        let b = img.data.len();
        total += b;
        mayor = mayor.max(b);
        n += 1;
    }
    eprintln!(
        "ha-ramart: {n} imagenes en Assets<Image> = {:.1} MB de CPU (la mas grande {:.1} MB)",
        total as f32 / 1024.0 / 1024.0,
        mayor as f32 / 1024.0 / 1024.0,
    );
}

/// Una vez que la imagen del sprite cargó, arma el `TextureAtlasLayout` (ancho de
/// frame = ancho/frames) y engancha el atlas. Corre hasta que `ready`.
fn setup_art_atlases(
    images: Res<Assets<Image>>,
    asset_server: Res<AssetServer>,
    mut layouts: ResMut<Assets<TextureAtlasLayout>>,
    mut q: Query<(&mut Sprite, &mut ArtAnimated)>,
) {
    for (mut sprite, mut anim) in &mut q {
        if anim.ready {
            continue;
        }
        // Dos motivos distintos para que la imagen no esté en `Assets<Image>`, y hay que
        // separarlos: o todavía está cargando (esperar), o se cargó sólo-GPU y ya se soltó
        // la copia de CPU (seguir con la geometría del manifest). Confundirlos deja al
        // sprite sin atlas para siempre, congelado en el frame 0.
        let size = match images.get(&sprite.image) {
            Some(img) => img.size(),
            None => {
                if !asset_server.is_loaded_with_dependencies(&sprite.image) {
                    continue; // aún cargando de verdad
                }
                // Sólo-GPU: `anim.cell/step/origin` vienen del `grid` del manifest, que es
                // justo la condición con la que se eligió no guardar los píxeles. El
                // "tamaño del atlas" se acota por lo que la tira realmente usa; el render
                // dibuja por RECT, así que este número no cambia lo que se ve.
                if anim.cell == UVec2::ZERO {
                    // No debería pasar (sin grid se carga con copia de CPU), pero si pasa
                    // es mejor un aviso que un sprite congelado sin explicación.
                    warn!(
                        "art: {:?} se cargó sólo-GPU pero no trae grilla; sin atlas",
                        sprite.image.path()
                    );
                    anim.ready = true;
                    continue;
                }
                let step = if anim.step == UVec2::ZERO { UVec2::new(anim.cell.x, 0) } else { anim.step };
                anim.origin + anim.cell + step * anim.frames.saturating_sub(1)
            }
        };
        let frames = anim.frames.max(1);
        // Imagen completa → derivar celda y paso horizontal; sub-región → usar
        // origin/cell/step del strip (frame multi-celda + dirección).
        let (cell_w, cell_h, origin, step) = if anim.cell == UVec2::ZERO {
            let cw = (size.x / frames).max(1);
            (cw, size.y, UVec2::ZERO, UVec2::new(cw, 0))
        } else {
            let cell = UVec2::new(anim.cell.x.max(1), anim.cell.y.max(1));
            // step ZERO (compat) → avanzar horizontal por el ancho del frame.
            let step = if anim.step == UVec2::ZERO { UVec2::new(cell.x, 0) } else { anim.step };
            (cell.x, cell.y, anim.origin, step)
        };
        let mut layout = TextureAtlasLayout::new_empty(size);
        for i in 0..frames {
            let x = origin.x + i * step.x;
            let y = origin.y + i * step.y;
            layout.add_texture(URect::from_corners(
                UVec2::new(x, y),
                UVec2::new(x + cell_w, y + cell_h),
            ));
        }
        sprite.texture_atlas = Some(TextureAtlas { layout: layouts.add(layout), index: 0 });
        anim.ready = true;
    }
}

/// Avanza el frame del atlas a `fps` (wrap en `Loop`; en `OnceHold` frena y se
/// queda en el último frame — de ahí sale el cofre que "queda abierto").
fn tick_art_animations(time: Res<Time>, mut q: Query<(&mut Sprite, &mut ArtAnimated)>) {
    for (mut sprite, mut anim) in &mut q {
        if !anim.ready || anim.done || anim.pausada {
            continue;
        }
        anim.timer.tick(time.delta());
        if anim.timer.just_finished() {
            let frames = anim.frames.max(1) as usize;
            if let Some(atlas) = sprite.texture_atlas.as_mut() {
                match anim.mode {
                    PlayMode::Loop => atlas.index = (atlas.index + 1) % frames,
                    PlayMode::OnceHold => {
                        if atlas.index + 1 >= frames {
                            anim.done = true;
                        } else {
                            atlas.index += 1;
                        }
                    }
                }
            }
        }
    }
}

fn load_art_manifest(asset_server: Res<AssetServer>, mut art: ResMut<ArtLibrary>) {
    // Formato del editor (ha-asset-tools): compatible con el viejo `{"key":"png"}`
    // y con el nuevo `{"key":{"png":"...","anim":{...}}}`.
    let manifest = match ArtManifest::from_json(MANIFEST) {
        Ok(m) => m,
        Err(err) => {
            eprintln!("ha-art: manifest.json unreadable ({err}) — procedural art only");
            return;
        }
    };
    // Variantes declaradas (`objects` del manifest): la lista que el juego puede
    // ofrecer (materiales de un piso, skins) y, de esas, las que tienen arte.
    for (base, obj) in &manifest.objects {
        let with_art: Vec<String> = obj
            .variants
            .iter()
            .map(|v| ha_assets::variant_key(base, &v.id))
            .filter(|k| manifest.sprites.contains_key(k))
            .collect();
        if !with_art.is_empty() {
            art.variant_art.insert(base.clone(), with_art);
        }
        if !obj.variants.is_empty() {
            art.objects.insert(base.clone(), obj.variants.clone());
        }
    }
    for (key, value) in manifest.sprites {
        // SÓLO GPU cuando se puede. Por defecto Bevy carga con `MAIN_WORLD | RENDER_WORLD`:
        // sube la textura **y** se queda con los píxeles en RAM. Medido con `HA_RAMART=1`:
        // 306 imágenes = **238 MB de CPU**, además de las mismas texturas en la GPU. En un
        // Redmi de 3.8 GB eso es lo que lo mata por OOM (biblia §12).
        //
        // Nadie lee esos píxeles: el único que tocaba `Assets<Image>` era
        // `setup_art_atlases`, y sólo para `img.size()`. Cuando el slot declara `grid`, la
        // geometría ya viene del manifest y la imagen en RAM no hace falta para nada — se
        // suelta apenas se sube a la GPU. Ahí están los pesados: `npc/char_*` son 9 MB cada
        // uno (hoja de 1792×1312 para usar UNA celda de 32×64) y los atlas de UI 10 MB.
        //
        // Los slots SIN `grid` siguen con la copia de CPU: su tamaño de celda se deriva del
        // ancho de la imagen y sin ella no se puede armar el atlas. Son los livianos.
        // `HA_ART_CPU=1` vuelve al comportamiento viejo para todos, por si algo se ve mal.
        let geometria_conocida = matches!(&value, ArtValue::Full(e) if e.grid.is_some());
        let forzar_cpu = crate::flags::var("HA_ART_CPU").is_ok();
        let png = value.png().to_string();
        // El PNG **no se carga acá**: se anota de dónde sale y se pide la primera
        // vez que alguien dibuja la clave (`ArtLibrary::get`). Ver `rutas`.
        let solo_gpu = geometria_conocida && !forzar_cpu;
        let es_suelo = png == SUELO_PAGINA;
        art.rutas.insert(key.clone(), FuenteArte { png, solo_gpu });
        // Recortes de sheet (rect / strip) → celdas a px con la grilla del entry.
        if let ArtValue::Full(e) = &value {
            let (gw, gh) = e.grid.unwrap_or((32, 32));
            if let Some(c) = e.collides {
                art.collides.insert(key.clone(), c);
            }
            if let Some(fp) = &e.footprint {
                if !fp.is_empty() {
                    art.footprint_reach = art.footprint_reach.max(fp.w.max(fp.h) as i64);
                    if key.starts_with("decor/") {
                        art.decor_reach = art.decor_reach.max(fp.w.max(fp.h) as i64);
                    }
                    art.footprints.insert(key.clone(), fp.clone());
                }
            }
            if let Some(em) = e.emission.filter(|em| !em.is_off()) {
                art.emissions.insert(key.clone(), em);
            }
            // Capa del slot: la explícita del combo del editor; si no la tiene,
            // la de su ÚNICA pose etiquetada (assets viejos que declararon
            // "floor"/"roof" como pose suelta). Con varias poses no se asume: ahí
            // la capa es de cada parte y la elige quien las instancia.
            if let Some(l) = e.layer.or_else(|| {
                e.poses
                    .as_ref()
                    .filter(|m| m.len() == 1)
                    .and_then(|m| m.values().next()?.layer)
            }) {
                art.layers.insert(key.clone(), l);
            }
            if let Some(r) = e.rect {
                art.rects.insert(key.clone(), [r.col * gw, r.row * gh, r.w * gw, r.h * gh]);
                // **Suelo**: si el recorte vive en la hoja del suelo, se guarda su
                // UV normalizada para que la malla del chunk lo dibuje sin
                // spawnear un sprite por tile (ver `suelo_uv`).
                if es_suelo {
                    art.suelo_pagina = Some(SUELO_PAGINA.to_string());
                    let (pw, ph) = (SUELO_PAGINA_PX.0 as f32, SUELO_PAGINA_PX.1 as f32);
                    // Medio texel adentro: sin esto el filtrado toma el borde de
                    // la celda vecina y aparece una costura de otro material.
                    let (ix, iy) = (0.5 / pw, 0.5 / ph);
                    art.suelo_uv.insert(
                        key.clone(),
                        [
                            (r.col * gw) as f32 / pw + ix,
                            (r.row * gh) as f32 / ph + iy,
                            ((r.col + r.w) * gw) as f32 / pw - ix,
                            ((r.row + r.h) * gh) as f32 / ph - iy,
                        ],
                    );
                }
            }
            // Tamaño en tiles del slot (recorte, pose representativa o tira).
            let cells = e
                .rect
                .map(|r| (r.w, r.h))
                .or_else(|| {
                    e.poses
                        .as_ref()
                        .and_then(|m| {
                            m.get("idle_down").or_else(|| m.get("center")).or_else(|| m.values().next())
                        })
                        .map(|p| (p.rect.w, p.rect.h))
                })
                .or_else(|| e.strip.map(|s| (s.w, s.h)));
            if let Some((cw, ch)) = cells {
                art.cells.insert(key.clone(), (cw.max(1), ch.max(1)));
            }
            if let Some(sl) = e.slice {
                art.slices
                    .insert(key.clone(), [sl.left, sl.right, sl.top, sl.bottom]);
            }
            if let Some(strip) = e.strip {
                // `strip` es el rect (celdas) del PRIMER frame → su w×h es el
                // tamaño de cada frame. La cantidad de frames viene de `anim`
                // (default: 1 si falta). Los frames avanzan en `strip_dir`.
                let fps = e.anim.map(|a| a.fps).unwrap_or(6.0);
                let frames = e.anim.map(|a| a.frames).unwrap_or(1).max(1);
                let dir = e.strip_dir.unwrap_or_default();
                art.strips.insert(
                    key.clone(),
                    strip_info(strip, (gw, gh), frames, fps, dir),
                );
            }
            if let Some(poses) = &e.poses {
                // Autotile ANIMADO repartido en BLOQUES: cuando el pack guarda
                // cada frame como un bloque entero (`strip` = 3×3) y cada pose es
                // una SUBCELDA de ese bloque, los frames de una pose están
                // separados por el ancho/alto del BLOQUE, no por el de su
                // subcelda — sin eso la animación pisa las celdas vecinas
                // (bordes/esquinas) y el agua se ve como rayas.
                //
                // **Y sólo cuando el entry DECLARA el bloque.** El default de
                // `(1,1)` no era neutro: pisaba el paso que ya había calculado
                // `strip_info` (el ancho del frame) por UNA celda. Para las poses
                // de un tile daba lo mismo, así que el error vivió escondido en
                // lo único que mide más: la CASCADA. Sus cuatro poses miden 3
                // celdas de ancho, y avanzaban de a 32 px en vez de 96 — o sea
                // que el chorro no se animaba, se arrastraba por encima de sus
                // propios frames y se leía como una lámina quieta con los seis
                // dibujos encimados. Es exactamente la queja del usuario.
                let bloque = e.strip.map(|s| (s.w.max(1), s.h.max(1)));
                let mut m = HashMap::new();
                for (name, pose) in poses {
                    // Hoja propia de la pose (asset armado con varios PNG: uno
                    // por dirección/estado) → se carga aparte y con SU grilla.
                    let own = pose.png.as_ref().map(|p| asset_server.load(p.to_string()));
                    let (pw, ph) = pose.grid_or((gw, gh));
                    let rect_px = [pose.rect.col * pw, pose.rect.row * ph, pose.rect.w * pw, pose.rect.h * ph];
                    let dir = pose.dir.unwrap_or_default();
                    let anim = pose.anim.map(|a| {
                        let mut si = strip_info(pose.rect, (pw, ph), a.frames.max(1), a.fps, dir);
                        // El paso de BLOQUE sólo aplica cuando la pose comparte la
                        // hoja del slot; con hoja propia los frames van pegados
                        // como en cualquier tira. Sin bloque declarado manda el
                        // paso natural: el ancho (o alto) del propio frame.
                        if let Some((bw, bh)) = bloque.filter(|_| pose.png.is_none()) {
                            si.step = match dir {
                                StripDir::Right => UVec2::new(bw * pw, 0),
                                StripDir::Down => UVec2::new(0, bh * ph),
                            };
                        }
                        si
                    });
                    m.insert(name.clone(), PoseInfo { rect_px, anim, layer: pose.layer, image: own });
                }
                if !m.is_empty() {
                    art.poses.insert(key.clone(), m);
                }
            }
        }
        // Animación de imagen completa sólo si no hay strip explícito.
        if !art.strips.contains_key(&key) {
            if let Some(anim) = value.anim() {
                art.anims.insert(key, anim);
            }
        }
    }
    art.servidor = Some(asset_server.clone());
    if !art.rutas.is_empty() {
        eprintln!(
            "ha-art: {} sprites ({} anim, {} recortes, {} strips, {} con luz)",
            art.rutas.len(),
            art.anims.len(),
            art.rects.len(),
            art.strips.len(),
            art.emissions.len()
        );
        if !art.objects.is_empty() {
            eprintln!(
                "ha-art: {} objetos con variantes ({} con arte)",
                art.objects.len(),
                art.variant_art.values().map(|v| v.len()).sum::<usize>()
            );
        }
    }

    registrar_poses_de_personaje(&mut art);

    // **Las losas de cada material, ya resueltas a UV.** Se arma UNA vez acá para
    // que la malla del chunk no tenga que construir la clave de la variante por
    // cada tile (ver `suelo_uv_seeded`: eran 256 asignaciones por chunk).
    let bases: Vec<String> = art.suelo_uv.keys().filter(|k| !k.contains('#')).cloned().collect();
    for base in bases {
        let mut uvs = Vec::new();
        if let Some(vs) = art.variant_art.get(&base) {
            // **En el PASTO, la losa lisa entra al sorteo y MANDA.**
            //
            // Un material con variantes sorteaba sólo entre las variantes: la losa
            // lisa quedaba afuera. Para el asfalto o la vereda da igual —sus
            // variantes son la misma baldosa con otro dibujo— pero en el pasto se
            // ve de lejos y es un desastre: la única variante de `pasto_bosque` es
            // `#s1`, la losa CON MATAS, así que el 100 % del campo salía con matas
            // repetidas en grilla. El reporte fue literal: *"le pones un pasto con
            // tanta mierda que parecen puntos… forman figuritas como charquitos"*
            // (24 ago 2026).
            //
            // El suelo se pinta LISO. Las matas son una marca ocasional de textura,
            // así que la lisa va repetida `PESO_PASTO_LISO` veces contra una sola de
            // cada variante — con la única variante de hoy, una mata cada ocho
            // celdas. No se toca ningún otro material.
            if let Some(peso) = peso_liso(&base) {
                if let Some(uv) = art.suelo_uv.get(&base) {
                    uvs.extend(std::iter::repeat(*uv).take(peso * vs.len()));
                }
            }
            uvs.extend(vs.iter().filter_map(|v| art.suelo_uv.get(v).copied()));
        }
        // Sin variantes con arte, la única losa es la del propio material.
        if uvs.is_empty() {
            if let Some(uv) = art.suelo_uv.get(&base) {
                uvs.push(*uv);
            }
        }
        if !uvs.is_empty() {
            art.suelo_variantes.insert(base, uvs);
        }
    }
    art.pasto_por_bioma = crate::world2d::PASTOS_DE_BIOMA
        .iter()
        .any(|k| art.suelo_variantes.contains_key(*k));
    // La familia 0 es la de siempre y no cuenta: lo que se pregunta es si hay
    // ALTERNATIVAS que repartir entre las ciudades.
    art.veredas_de_ciudad = crate::world2d::VEREDA_BALDOSA[1..]
        .iter()
        .any(|k| art.suelo_variantes.contains_key(*k));
}

/// Item/UI icons: any `items/<item_id>.png` in the pack replaces the
/// procedural icon EVERYWHERE (hotbar, bag, craft list, machine sprites) by
/// pre-seeding the icon cache before the HUD first asks for it.
/// **Con su recorte.** Un slot `items/*` curado desde un ATLAS guarda el rect en
/// `art/manifest.json`; sin pasarlo acá, la mochila y la mano dibujaban la hoja entera
/// escalada a 40 px (era el "a veces el juego usa mal los assets"). Un PNG suelto
/// no tiene rect y se comporta como siempre.
fn preload_item_icons(art: Res<ArtLibrary>, mut icons: ResMut<ItemIconCache>) {
    // Los íconos de la mochila SÍ se cargan de una: son tres docenas, se dibujan
    // en el primer HUD y una barra rápida con huecos que se llenan de a poco se
    // lee como que el juego perdió el inventario.
    let claves: Vec<String> = art
        .rutas
        .keys()
        .filter(|k| k.starts_with("items/"))
        .cloned()
        .collect();
    for key in claves {
        let Some(handle) = art.get(&key) else { continue };
        let Some(item_id) = key.strip_prefix("items/") else { continue };
        let rect = art
            .rects
            .get(&key)
            .copied()
            .map(|[x, y, w, h]| {
                Rect::from_corners(
                    Vec2::new(x as f32, y as f32),
                    Vec2::new((x + w) as f32, (y + h) as f32),
                )
            })
            // **Un ítem animado es su PRIMER cuadro.** La linterna es una tira de
            // cuatro (`strip`, sin `rect`): sin esto la mochila, la hotbar y la
            // receta dibujaban las cuatro llamas en fila (29 sep 2026).
            .or_else(|| {
                art.strips.get(&key).map(|s| {
                    Rect::from_corners(s.origin.as_vec2(), (s.origin + s.cell).as_vec2())
                })
            });
        icons.preload(item_id, handle.clone(), rect);
    }
}

/// Pide el PNG de una fuente con los ajustes que el slot declaró.
///
/// **Sólo GPU cuando se puede.** Por defecto Bevy carga con `MAIN_WORLD |
/// RENDER_WORLD`: sube la textura **y** se queda con los píxeles en RAM. Medido
/// con `HA_RAMART=1`: 306 imágenes = 238 MB de CPU, además de las mismas
/// texturas en la GPU. En un Redmi de 3,8 GB eso es lo que lo mata por OOM
/// (biblia §12). Cuando el slot declara `grid` la geometría ya viene del
/// manifest y nadie lee esos píxeles, así que se sueltan apenas se suben.
/// `HA_ART_CPU=1` vuelve al comportamiento viejo, por si algo se ve mal.
fn cargar_png(servidor: &AssetServer, fuente: &FuenteArte) -> Handle<Image> {
    if fuente.solo_gpu {
        servidor.load_with_settings(fuente.png.clone(), |s: &mut ImageLoaderSettings| {
            s.asset_usage = RenderAssetUsages::RENDER_WORLD;
        })
    } else {
        servidor.load(fuente.png.clone())
    }
}

#[cfg(test)]
impl ArtLibrary {
    /// Biblioteca mínima con la huella de un slot — para probar la colisión de
    /// decor sin levantar Bevy ni cargar PNGs.
    pub(crate) fn test_footprint(key: &str, fp: ha_assets::CollisionFootprint) -> Self {
        let mut a = Self::default();
        a.footprint_reach = fp.w.max(fp.h) as i64;
        a.decor_reach = a.footprint_reach;
        a.footprints.insert(key.to_string(), fp);
        a
    }

    /// Biblioteca mínima con el flag "bloquea el paso" de un slot.
    pub(crate) fn test_collides(key: &str, collides: bool) -> Self {
        let mut a = Self::default();
        a.collides.insert(key.to_string(), collides);
        a
    }

    /// Biblioteca mínima con una pose recortada de `celdas` celdas del pack —
    /// para probar las dos reglas de medida sin cargar un PNG.
    pub(crate) fn test_pose(key: &str, pose: &str, celdas: (u32, u32)) -> Self {
        Self::test_poses(key, &[(pose, celdas)])
    }

    /// Ídem, con VARIAS poses de tamaños distintos. Es lo que hace falta para
    /// probar una fachada, donde la cara de afuera (`back`, elevación de
    /// exteriores) mide más que la de adentro (`front`, Room Builder).
    pub(crate) fn test_poses(key: &str, poses: &[(&str, (u32, u32))]) -> Self {
        let mut a = Self::default();
        let entrada = a.poses.entry(key.to_string()).or_default();
        for (pose, celdas) in poses {
            entrada.insert(
                pose.to_string(),
                PoseInfo {
                    rect_px: [0, 0, celdas.0 * 32, celdas.1 * 32],
                    anim: None,
                    layer: None,
                    image: None,
                },
            );
        }
        a
    }
}

#[cfg(test)]
mod tests {
    /// Las hojas de aldeano tienen que estar COMPACTAS (`scripts/compactar-npcs.py`): con la
    /// hoja completa las poses derivadas apuntarían a filas que no son las suyas.
    #[test]
    fn las_hojas_de_aldeano_estan_compactas() {
        let dir = concat!(env!("CARGO_MANIFEST_DIR"), "/assets/art/npc");
        let (w, h) = (ha_assets::chargen_index::NPC_COLS * 32, ha_assets::chargen_index::NPC_ANIMS.len() as u32 * 64);
        let mut n = 0;
        for e in std::fs::read_dir(dir).unwrap().flatten() {
            let p = e.path();
            if p.extension().map(|x| x == "png").unwrap_or(false) && p.file_name().unwrap().to_string_lossy().starts_with("char_") {
                let b = std::fs::read(&p).unwrap();
                let (pw, ph) = (u32::from_be_bytes(b[16..20].try_into().unwrap()), u32::from_be_bytes(b[20..24].try_into().unwrap()));
                assert_eq!((pw, ph), (w, h), "{} no está compacta: correr scripts/compactar-npcs.py", p.display());
                n += 1;
            }
        }
        // 20 aldeanos adultos (`char_1..20`) + 6 skins de niño (`char_21..26`, `scripts/hornear-ninos.py`).
        assert_eq!(n, 26);
    }

    use super::*;

    /// **Un slot de UI curado como POSE tiene que traer su recorte.**
    ///
    /// Sin esto el `ImageNode` sale con `rect: None` y Bevy estira **la hoja
    /// entera** dentro del widget. Es el bug que el 15 ago 2026 se vio en el
    /// Redmi como tres cosas distintas —las diez casillas de la hotbar con un
    /// tilesheet adentro, el joystick hecho un collage de gamepads y el botón
    /// SALTAR encima de esa misma lámina— y que era uno solo: los marcos de UI
    /// del pack guardan su recorte en `poses.center`, no en el `rect` plano que
    /// era lo único que se leía.
    ///
    /// No se puede probar `ui_image_node` sin `AssetServer`, así que se prueba la
    /// pieza que fallaba: la resolución del recorte.
    #[test]
    fn un_slot_de_ui_recortado_como_pose_no_pierde_su_recorte() {
        let art = ArtLibrary::test_pose("ui/hotbar_slot", "center", (1, 1));
        assert_eq!(
            art.rect_de_ui("ui/hotbar_slot"),
            Some([0, 0, 32, 32]),
            "el recorte de la pose `center` no llegó: el widget dibujaría la hoja entera"
        );
    }

    /// Con una sola pose, sea cual sea su nombre, ésa es la imagen del slot: no
    /// hay ambigüedad que resolver y quedarse sin recorte sería peor.
    #[test]
    fn con_una_sola_pose_esa_es_el_recorte() {
        let art = ArtLibrary::test_pose("ui/close_button", "normal", (2, 1));
        assert_eq!(art.rect_de_ui("ui/close_button"), Some([0, 0, 64, 32]));
    }

    /// Y un slot sin recorte declarado sigue sin recorte — el respaldo no puede
    /// inventar uno, porque un PNG suelto ES la imagen entera y recortarlo lo
    /// rompería.
    #[test]
    fn un_png_suelto_se_queda_sin_recorte() {
        let art = ArtLibrary::default();
        assert_eq!(art.rect_de_ui("ui/loquesea"), None);
    }

    /// **La escala del arte sale de una división, no de la intuición.**
    ///
    /// [`ESCALA_ARTE`] valió 1,5 durante toda la importación del pack con el
    /// argumento de que "LimeZu dibuja a 32 px por tile de ~0,66 m" — un número
    /// que no se midió contra nada del juego. El resultado fue que 12.000 objetos
    /// entraron un 58 % más grandes que el personaje que camina entre medio.
    ///
    /// El único tamaño real que el juego conoce es el del personaje, y el marco
    /// del personaje son 2 celdas del pack de alto. Esa división es la escala. Si
    /// alguien vuelve a cambiar una de las dos sin la otra, esto lo caza.
    #[test]
    fn una_celda_del_pack_mide_lo_que_el_personaje_dice() {
        // El marco del personaje de LimeZu es de 32 × 64 px = 1 × 2 celdas.
        const CELDAS_DE_ALTO: f32 = 2.0;
        let por_celda = crate::scene::TAMANO_JUGADOR.y / CELDAS_DE_ALTO;
        assert!(
            (ESCALA_ARTE - por_celda).abs() < 1e-4,
            "ESCALA_ARTE = {ESCALA_ARTE} pero el personaje dice {por_celda} m por celda: \
             el pack se dibuja a otra escala que la gente que camina entre medio"
        );
    }

    /// La escala del dibujo y la de la huella son **el mismo número**. Viven en
    /// dos idiomas (Rust y el importador de Python), y si se separan el objeto se
    /// dibuja de un tamaño y frena de otro.
    /// **Y TODOS los importadores, no sólo el masivo.** Este test miraba
    /// únicamente `import-pack-masivo.py`, así que `import-edificios.py` se quedó
    /// meses con el 1,5 viejo sin que nadie lo cazara: medía cada fachada un 58 %
    /// más grande de lo que el juego la dibuja y por eso elegía siempre edificios
    /// demasiado chicos para su parcela.
    #[test]
    fn el_importador_usa_la_misma_escala() {
        for (nombre, py) in [
            ("import-pack-masivo.py", include_str!("../../../../scripts/import-pack-masivo.py")),
            ("import-edificios.py", include_str!("../../../../scripts/import-edificios.py")),
        ] {
            let linea = py
                .lines()
                .find(|l| l.starts_with("ESCALA_TILE"))
                .unwrap_or_else(|| panic!("{nombre} declara ESCALA_TILE"));
            let valor: f32 = linea
                .split('=')
                .nth(1)
                .and_then(|v| v.trim().parse().ok())
                .expect("ESCALA_TILE es un número");
            assert!(
                (valor - ESCALA_ARTE).abs() < 1e-4,
                "{nombre} escribe las huellas con {valor} y el juego dibuja con {ESCALA_ARTE}"
            );
        }
    }

    /// **Una animación avanza un frame entero, nunca un pedazo de frame.**
    ///
    /// Es la regla que se rompía con las cascadas: sus poses miden 3 celdas de
    /// ancho y el paso salía de UNA, así que cada "frame" mostraba dos tercios
    /// del anterior y un tercio del siguiente — seis dibujos encimados que en
    /// pantalla se leen como una lámina quieta. Con arte de 1×1 el error era
    /// invisible, y por eso vivió tanto.
    ///
    /// Se comprueba contra el manifest de verdad: si mañana alguien importa otra
    /// pose animada de varias celdas, la regla ya está puesta.
    #[test]
    fn los_frames_de_una_pose_no_se_pisan() {
        let m = ArtManifest::from_json(MANIFEST).expect("manifest legible");
        let mut animadas = 0;
        for (clave, valor) in &m.sprites {
            let ArtValue::Full(e) = valor else { continue };
            let (gw, gh) = e.grid.unwrap_or((32, 32));
            let bloque = e.strip.map(|s| (s.w.max(1), s.h.max(1)));
            let Some(poses) = &e.poses else { continue };
            for (nombre, pose) in poses {
                let Some(a) = pose.anim.filter(|a| a.frames > 1) else { continue };
                animadas += 1;
                let (pw, ph) = pose.grid_or((gw, gh));
                let dir = pose.dir.unwrap_or_default();
                let mut si = strip_info(pose.rect, (pw, ph), a.frames.max(1), a.fps, dir);
                if let Some((bw, bh)) = bloque.filter(|_| pose.png.is_none()) {
                    si.step = match dir {
                        StripDir::Right => UVec2::new(bw * pw, 0),
                        StripDir::Down => UVec2::new(0, bh * ph),
                    };
                }
                let (paso, mide) = match dir {
                    StripDir::Right => (si.step.x, si.cell.x),
                    StripDir::Down => (si.step.y, si.cell.y),
                };
                assert!(
                    paso >= mide,
                    "{clave}/{nombre}: el frame mide {mide} px y la tira avanza {paso} — \
                     los frames se pisan y la animación se ve quieta"
                );
            }
        }
        assert!(animadas > 0, "el manifest no trae ni una pose animada: la regla no probó nada");
    }

    /// **LA POZA MIDE LO QUE LA SIM CREE QUE MIDE.**
    ///
    /// `agua::POZA_FILAS` es la altura del reventón en la simulación: de ahí
    /// salen `filas_de_dibujo`, `bajo_el_chorro` (para no sortear un nenúfar
    /// dentro del chorro) y `poza_de_cascada` (para que el autotile del agua no
    /// dibuje su ribete de orilla debajo del salto). El dibujo, en cambio, lee la
    /// altura real del recorte, y **había un `unwrap_or(1)` justificado con "las
    /// variantes derivadas de la hoja entera miden uno"**: medido sobre el
    /// manifest, las 30 miden dos. La suposición era falsa y nadie se enteraba
    /// porque el caso no se daba; el día que se diera, la sim habría tapado una
    /// fila de agua de más y el chorro habría reventado un cuadro antes.
    ///
    /// Las dos alturas son un contrato entre `scripts/import-agua.py` y la sim,
    /// así que se comprueba en vez de suponerse.
    #[test]
    fn la_poza_del_pack_mide_lo_que_dice_la_sim() {
        let m = ArtManifest::from_json(MANIFEST).expect("manifest legible");
        let esperado = ha_shared::agua::POZA_FILAS as u32;
        let mut vistas = 0;
        for (clave, valor) in &m.sprites {
            if !clave.starts_with("agua/cascada") {
                continue;
            }
            let ArtValue::Full(e) = valor else { continue };
            let Some(poses) = &e.poses else { continue };
            let Some(poza) = poses.get("poza") else { continue };
            vistas += 1;
            assert_eq!(
                poza.rect.h, esperado,
                "{clave}/poza mide {} celdas de alto y la sim usa POZA_FILAS = {esperado}",
                poza.rect.h
            );
        }
        assert!(vistas > 0, "no hay una sola pose de poza en el manifest: la regla no probó nada");
    }

    /// **UN MURO ALTO NO PUEDE TENER UNA SOLA BALDOSA DE CUERPO.**
    ///
    /// `relief::SALTO_DE_TERRAZA` llega a diez escalones, así que
    /// `world2d::pared_apilada` apila hasta nueve filas de `cuerpo`. Con una sola
    /// celda eso no se lee como roca: se lee como una cuadrícula de gofre, que es
    /// lo que el usuario marcó en la captura. `import-acantilado.py` publica las
    /// tres celdas de la fila repetible del kit como `cuerpo_2..cuerpo_4`; si
    /// alguien vuelve a dejar el muro con una sola, esto lo caza acá y no en el
    /// teléfono.
    #[test]
    fn el_cuerpo_de_una_pared_tiene_variantes() {
        let m = ArtManifest::from_json(MANIFEST).expect("manifest legible");
        let mut vistas = 0;
        for (clave, valor) in &m.sprites {
            if !clave.starts_with("cliff/") {
                continue;
            }
            let ArtValue::Full(e) = valor else { continue };
            let Some(poses) = &e.poses else { continue };
            let Some(cuerpo) = poses.get("cuerpo") else { continue };
            vistas += 1;
            let variantes: Vec<_> = ["cuerpo_2", "cuerpo_3", "cuerpo_4"]
                .iter()
                .filter_map(|n| poses.get(*n))
                .collect();
            assert!(
                !variantes.is_empty(),
                "{clave} apila `cuerpo` sin una sola variante: una pared de diez pisos \
                 saldría con la misma celda nueve veces"
            );
            // Y que sean recortes DISTINTOS: tres claves apuntando al mismo rect
            // serían la misma baldosa con otro nombre.
            for v in variantes {
                assert!(
                    (v.rect.col, v.rect.row) != (cuerpo.rect.col, cuerpo.rect.row),
                    "{clave}: una variante de cuerpo apunta al mismo recorte que `cuerpo`"
                );
            }
        }
        assert!(vistas > 0, "ninguna clave `cliff/*` tiene cuerpo: la regla no probó nada");
    }
}

#[cfg(test)]
mod inventario_de_arte {
    use super::*;

    /// **NINGÚN SLOT DEL JUEGO SE QUEDA SIN PIXEL.**
    ///
    /// El 14 ago 2026 el catálogo tenía **142 claves de 8.792 sin una sola
    /// entrada** en el manifest: los 36 iconos del inventario, 18 de interfaz, la
    /// fauna, las máquinas, el cohete… todo eso se dibujaba como un cuadrado de
    /// color. Se cerraron con `scripts/dibujar-*.py` (dibujadas con el estilo
    /// medido de la skill `pixel-art`), con asignaciones del pack y con dos
    /// derivaciones (`cliff/roca`, `tiles/regolith`).
    ///
    /// Esto es lo que impide que vuelva a abrirse: agregar un slot a
    /// `ha_assets::game_slots` sin darle arte falla acá, no en el teléfono.
    /// Si estás agregando uno, el mensaje te dice qué hacer.
    #[test]
    fn ningun_slot_se_queda_sin_pixel() {
        let m = ArtManifest::from_json(MANIFEST).expect("manifest legible");
        let sin: Vec<&str> = ha_assets::game_slots()
            .into_iter()
            .filter(|s| {
                !m.sprites.contains_key(s.key)
                    && !m.sprites.keys().any(|k| k.starts_with(&format!("{}#", s.key)))
            })
            .map(|s| s.key)
            .collect();
        assert!(
            sin.is_empty(),
            "{} slots sin arte: {:?}\n\
             Buscalo en el pack, derivalo de lo que ya está en uso, o dibujalo con \
             la skill `pixel-art` (scripts/lib/pixel.py). Detalle del hueco:\n  \
             cargo test -p ha-game inventario_de_arte -- --ignored --nocapture",
            sin.len(),
            &sin[..sin.len().min(12)]
        );
    }

    /// **NINGUNA PARCELA DE LA CIUDAD SE DIBUJA COMO UNA CAJA DE COLOR.**
    ///
    /// El guardián que faltaba, y se notó: `ningun_slot_se_queda_sin_pixel` mira
    /// el catálogo de slots (`ha_assets::game_slots`), pero los lotes de la ciudad
    /// apuntan a claves del PACK que no están en ese catálogo — y ahí nadie
    /// miraba. Con eso, la parcela más grande del barrio estuvo apuntando a
    /// `building/villa_6`, que el importador masivo había bautizado sobre un
    /// recorte de UNA CELDA, y cuatro programas residenciales a una clave que
    /// existía pero era la misma para los cuatro. El barrio —el 60 % de la
    /// ciudad— se veía como cajas.
    ///
    /// Se comprueban las dos mitades:
    ///
    /// 1. **Que la clave tenga pixel.** Sin entrada en el manifest, el lote se
    ///    dibuja con el cuadrado marrón de respaldo.
    /// 2. **Que el pixel LLENE la parcela.** Y esto no se mide contra un número
    ///    inventado, sino contra lo que hace el render: `building::roof_layout`
    ///    escala la fachada por el ancho de la huella y **no la agranda más de
    ///    `AGRANDE_MAX`**, para no ampliar el pixel. O sea que un sprite de una
    ///    celda en una parcela de doce tiles no sale "estirado": sale como una
    ///    mota en un solar vacío. Se exige que, ya agrandado todo lo que se
    ///    permite, el edificio cubra al menos la mitad del frente del lote.
    #[test]
    fn ningun_lote_de_la_ciudad_se_dibuja_como_una_caja() {
        let m = ArtManifest::from_json(MANIFEST).expect("manifest legible");
        let mut mal = Vec::new();
        for p in ha_shared::city::LOT_PROGRAMS {
            // `lot/*` no lleva edificio a propósito (la plaza, el estacionamiento,
            // el natatorio): lo dibujan sus tiles, no un sprite.
            if p.key.starts_with("lot/") {
                continue;
            }
            let variantes: Vec<&String> = m
                .sprites
                .keys()
                .filter(|k| k.as_str() == p.key || k.starts_with(&format!("{}#", p.key)))
                .collect();
            if variantes.is_empty() {
                mal.push(format!("{}: sin una sola entrada en el manifest", p.key));
                continue;
            }
            // Los animados no traen `rect` (traen tira) y se aceptan tal cual.
            let Some(celdas) = variantes
                .iter()
                .filter_map(|k| m.sprites.get(k.as_str()))
                .filter_map(|e| e.rect())
                .map(|r| r.w)
                .max()
            else {
                continue;
            };
            let huella = p.huella();
            let dibujado = celdas as f32 * ESCALA_ARTE * crate::building::AGRANDE_MAX;
            if dibujado < huella.0 as f32 * 0.5 {
                mal.push(format!(
                    "{}: su arte mide {celdas} celdas de frente ({dibujado:.0} tiles como mucho) \
                     y la parcela pide {}: queda una mota en un solar vacío",
                    p.key, huella.0
                ));
            }
        }
        assert!(
            mal.is_empty(),
            "{} lotes de ciudad cuyo arte no llena la parcela:\n  {}\n\
             O se apunta el lote a una clave del pack que SÍ sea un edificio, o se \
             achica `cells` hasta el talle del dibujo (ver `ha_shared::city::LOT_PROGRAMS`).",
            mal.len(),
            mal.join("\n  ")
        );
    }

    /// **CADA LOTE RECIBE EDIFICIOS COMPLETOS DE SU CLASE.**
    ///
    /// El pack trae casas enteras, casas por piezas (kits modulares) y, en la
    /// misma página, edificios enteros de clases distintas. Elegir por tamaño los
    /// mezclaba: el hospital salía con el cartel HOTEL, una ferretería caía en el
    /// lote de viviendas y la casa embrujada de Halloween en una cuadra de
    /// chalets. Qué dibujo es de qué parcela se decide en
    /// `scripts/import-edificios.py::FILTRO_POR_NOMBRE`; esto lo clava.
    #[test]
    fn cada_lote_solo_recibe_edificios_de_su_clase() {
        let m = ArtManifest::from_json(MANIFEST).expect("manifest legible");
        let firma = |k: &str| {
            m.sprites.get(k).and_then(|e| {
                let r = e.rect()?;
                Some((e.png().to_string(), r.col, r.row, r.w, r.h))
            })
        };
        let variantes = |slot: &str| -> Vec<String> {
            m.sprites
                .keys()
                .filter(|k| k.as_str() == slot || k.starts_with(&format!("{slot}#")))
                .cloned()
                .collect()
        };
        let hotel = firma("building/hotel_y_hospital_hotel_y_hospital_1").expect("el hotel del pack");
        let ferreteria: Vec<_> = ["building/edificios_ferreteria_tienda", "building/ferreteria_tienda"]
            .iter()
            .filter_map(|k| firma(k))
            .collect();
        let embrujadas: Vec<_> = ["building/casa_embrujada", "building/casa_embrujada_sin_farol"]
            .iter()
            .filter_map(|k| firma(k))
            .collect();
        assert!(!ferreteria.is_empty() && !embrujadas.is_empty(), "faltan claves de referencia");
        let mut mal = Vec::new();
        for slot in ["building/hotel", "building/hospital"] {
            for k in variantes(slot) {
                let es_hotel = firma(&k).as_ref() == Some(&hotel);
                if es_hotel != (slot == "building/hotel") {
                    mal.push(format!("{k}: {} recibió el edificio del otro", slot));
                }
            }
        }
        for k in variantes("building/duplex") {
            if firma(&k).is_some_and(|f| ferreteria.contains(&f)) {
                mal.push(format!("{k}: una ferretería en el lote de viviendas"));
            }
        }
        for slot in [
            "building/casa_chica",
            "building/casa_mediana",
            "building/casa_grande",
            "building/casa_country",
            "building/villa",
        ] {
            for k in variantes(slot) {
                if firma(&k).is_some_and(|f| embrujadas.contains(&f)) {
                    mal.push(format!("{k}: la casa embrujada en un barrio de chalets"));
                }
            }
        }
        assert!(mal.is_empty(), "lotes con edificio de otra clase:\n  {}", mal.join("\n  "));
    }

    /// **El kit modular tiene todas sus piezas, en los tres temas.**
    ///
    /// La ciudad compone las torres del centro y los locales del comercio banda
    /// por banda (`building::spawn_fachada_apilada`), pidiendo cada pieza por
    /// `objeto#tema`. Una pieza sin arte no se nota como un hueco: se dibuja con
    /// el color de respaldo (`edificios::color_de_respaldo`), o sea un paño de
    /// color plano en medio de la torre, que es justo lo que se estaba tratando
    /// de sacar del mapa. Y como el tema se elige por hash, faltar UNO significa
    /// que un tercio de los edificios sale así.
    #[test]
    fn el_kit_modular_no_tiene_piezas_sin_pixel() {
        let m = ArtManifest::from_json(MANIFEST).expect("manifest legible");
        let mut mal = Vec::new();
        let tablas = [
            crate::edificios::PLANTAS_BAJAS,
            crate::edificios::PISOS,
            crate::edificios::AZOTEAS,
            crate::building::LOCALES,
        ];
        for tabla in tablas {
            for clave in tabla {
                for tema in ["ladrillo", "azul", "hormigon"] {
                    let k = format!("{clave}#{tema}");
                    if !m.sprites.contains_key(&k) {
                        mal.push(k);
                    }
                }
            }
        }
        assert!(
            mal.is_empty(),
            "{} piezas del kit modular sin arte (salen como un pano de color \
             plano en medio del edificio):\n  {}\n\
             Corre `python3 scripts/import-modulares.py`.",
            mal.len(),
            mal.join("\n  ")
        );
    }

    /// **Lo de adentro se queda adentro.**
    ///
    /// El juego mezcla dos packs de LimeZu que dibujan cosas parecidas con los
    /// mismos nombres: `Modern Exteriors` (la calle, las casas, los edificios) y
    /// `Modern Interiors` (los muebles, las góndolas, los mostradores). El
    /// importador masivo clasificaba por PALABRA del nombre de archivo antes que
    /// por pack, y las palabras se repiten entre los dos:
    ///
    /// ```text
    ///   Clothing_Store  → "store" → building/tienda_de_ropa_11  (una PERCHA)
    ///   Grocery_Store   → "store" → building/almacen_1          (una GÓNDOLA)
    ///   Ice_Cream_Shop  → "shop"  → building/heladeria_1        (un MOSTRADOR)
    /// ```
    ///
    /// 661 muebles de interior quedaron en `building/*` —o sea, en la categoría
    /// "Edificio" del menú de construir— y uno de ellos, `building/heladeria_1`,
    /// llegó a ser una PARCELA de la ciudad: el generador plantaba un mostrador de
    /// 2×3 celdas en un solar de la avenida.
    ///
    /// Ahora el importador declara de qué pack salió cada página
    /// (`scripts/lib/origen.py` → `manifest.origen`) y esto se planta si algo que
    /// se dibuja A LA INTEMPERIE apunta a un dibujo de adentro. Se miran las tres
    /// bocas por las que el arte sale al mundo:
    ///
    /// 1. los lotes de la ciudad (`city::LOT_PROGRAMS`),
    /// 2. las tablas de siembra (`pack_siembra`, vía `worldgen`),
    /// 3. la carpeta `building/` entera, que es lo que el menú de construir
    ///    ofrece bajo "Edificio".
    #[test]
    fn la_ciudad_no_se_construye_con_interiores() {
        let m = ArtManifest::from_json(MANIFEST).expect("manifest legible");
        assert!(
            !m.origen.is_empty(),
            "el manifest no declara el origen de ninguna página: corré \
             `python3 scripts/import-pack-masivo.py`"
        );
        let mut mal = Vec::new();
        let mut revisar = |que: &str, clave: &str| {
            if !m.se_planta_afuera(clave) {
                let png = m.sprites.get(clave).map(|e| e.png()).unwrap_or("?").to_string();
                mal.push(format!("{que} {clave} sale de {png} (Modern Interiors)"));
            }
        };
        for p in ha_shared::city::LOT_PROGRAMS {
            revisar("lote de ciudad", p.key);
        }
        for (nombre, tabla) in ha_shared::worldgen::TABLAS_SEMBRADAS {
            for d in *tabla {
                revisar(nombre, d.key);
            }
        }
        for clave in m.sprites.keys().filter(|k| k.starts_with("building/")) {
            revisar("slot", clave);
        }
        assert!(
            mal.is_empty(),
            "{} cosas de Modern Interiors plantadas a la intemperie:\n  {}\n\
             Los interiores no pueden caer en las carpetas de afuera: ver \
             `import-pack-masivo.py::CARPETAS_DE_AFUERA` y volvé a correr el importador.",
            mal.len(),
            mal.iter().take(20).cloned().collect::<Vec<_>>().join("\n  ")
        );
    }

    /// **QUÉ LE FALTA PIXEL AL JUEGO** — el inventario que hay que mirar antes de
    /// dibujar nada. Recorre el catálogo canónico (`ha_assets::game_slots`) y
    /// dice, por categoría, qué claves no tienen una sola entrada en el manifest.
    ///
    /// `cargo test -p ha-game inventario_de_arte -- --ignored --nocapture`
    #[test]
    #[ignore = "herramienta de medición, no un guardián"]
    fn que_le_falta_pixel_al_juego() {
        let m = ArtManifest::from_json(MANIFEST).expect("manifest legible");
        let mut por_cat: std::collections::BTreeMap<&str, (Vec<&str>, Vec<&str>)> =
            Default::default();
        for s in ha_assets::game_slots() {
            let tiene = m.sprites.contains_key(s.key)
                || m.sprites.keys().any(|k| k.starts_with(&format!("{}#", s.key)));
            let e = por_cat.entry(s.category).or_default();
            if tiene {
                e.0.push(s.key);
            } else {
                e.1.push(s.key);
            }
        }
        let (mut con, mut sin) = (0usize, 0usize);
        println!("\ninventario de arte por categoría:");
        for (cat, (c, s)) in &por_cat {
            con += c.len();
            sin += s.len();
            println!(
                "  {cat:14} {:4} con pixel · {:4} SIN  ({:.0}%)",
                c.len(),
                s.len(),
                c.len() as f32 * 100.0 / (c.len() + s.len()).max(1) as f32
            );
        }
        println!("\nTOTAL: {con} con pixel, {sin} sin — {} slots", con + sin);
        println!("\nlos que faltan, por categoría:");
        for (cat, (_, s)) in &por_cat {
            if s.is_empty() {
                continue;
            }
            println!("\n  == {cat} ({}) ==", s.len());
            for chunk in s.chunks(4) {
                println!("     {}", chunk.join("  "));
            }
        }
    }
}
