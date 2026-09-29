//! **La huerta**: sembrar, ver crecer con el reloj del juego y cosechar.
//!
//! El modelo es el de Minecraft, adaptado al toque:
//!
//! - **De dónde sale la semilla.** Arrancar una planta del mundo (una mata de
//!   pasto, un arbusto, un brote) a veces deja semillas — casi siempre de trigo,
//!   a veces una zanahoria o semillas de calabaza (ver [`semillas_de_planta`]).
//!   Una calabaza cosechada se parte en cuatro semillas en la mesa de crafteo, y
//!   la zanahoria se planta entera, como en Minecraft.
//! - **Sembrar.** Con la semilla en la mano, mantener sobre pasto o tierra: la
//!   celda queda arada y sembrada. No hace falta azada — en el teléfono cada
//!   herramienta extra es un viaje a la mochila y el surco ya dice "huerta".
//! - **Crecer.** Cuatro etapas a la vista: recién sembrado → brote → creciendo →
//!   listo. El crecimiento corre con el RELOJ DEL JUEGO (`sky2d::DayClock`, un día
//!   = [`crate::sky2d::DAY_SECS`]): de día a pleno, de noche a un cuarto — como
//!   Minecraft, donde el cultivo necesita luz —, y más rápido si hay agua a
//!   [`RADIO_AGUA`] celdas (la tierra se ve húmeda). Media jornada de juego al sol
//!   y con riego alcanza; sin agua, casi una entera.
//! - **Cosechar.** Mantener sobre la planta: si está lista da la cosecha y
//!   semillas de más (la huerta crece sola si la cuidás); si no, se arranca y
//!   devuelve la semilla, igual que romper trigo verde en Minecraft.
//!
//! Todo persiste por partida en `cultivos.json`, en TODOS los cuerpos a la vez:
//! la huerta de la Tierra sigue creciendo mientras estás en Marte.

use std::collections::HashMap;
use std::path::PathBuf;

use bevy::prelude::*;
use ha_shared::topdown::Tile;
use ha_shared::{CelestialBody, InteractTargetKind};
use serde::{Deserialize, Serialize};

use crate::hud::inventory_ui::{
    add_item_to_slots, consume_from_ui_slots, flush_slots_to_inventory, InventoryUiState,
};
use crate::input::{EquippedItem, HoldInteractState};
use crate::inventory_persist::{persist_dir, InventoryPersistState, PlayerInventory};
use crate::platform::is_app_paused;
use crate::sim::Player;
use crate::sky2d::{DayClock, DAY_SECS};
use crate::targeting::{InteractTarget, LookTarget};
use crate::world2d::{active_surface_body, render_xy, sort_z, ActivePlanet, TileWorld};

/// Segundos de JUEGO que tarda un cultivo en madurar a pleno sol y sin riego:
/// casi una jornada. El trigo de Minecraft tarda 1-3 días de 20 min; acá el día
/// dura una hora real, así que una jornada de sol es la misma sensación de
/// "sembré ayer, cosecho hoy".
pub const MADURAR_SECS: f32 = DAY_SECS * 0.8;
/// El riego multiplica la velocidad (la tierra húmeda de Minecraft rinde ~2×).
pub const RIEGO_X: f32 = 1.6;
/// De noche el cultivo no se detiene del todo, pero casi (Minecraft pide luz 9).
pub const NOCHE_X: f32 = 0.25;
/// A cuántas celdas tiene que haber agua para que la tierra quede húmeda (el
/// mismo 4 de Minecraft).
pub const RADIO_AGUA: i64 = 4;
/// Hasta dónde se materializan los cultivos alrededor del jugador (m).
const RADIO_VISTA: f32 = 60.0;

// ─── Las plantas ─────────────────────────────────────────────────────────────

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Planta {
    Trigo,
    Zanahoria,
    Calabaza,
}

impl Planta {
    pub const TODAS: [Planta; 3] = [Planta::Trigo, Planta::Zanahoria, Planta::Calabaza];

    /// El ítem que se planta.
    pub fn semilla(self) -> &'static str {
        match self {
            Planta::Trigo => "wheat_seeds",
            Planta::Zanahoria => "carrot",
            Planta::Calabaza => "pumpkin_seeds",
        }
    }

    /// Qué se planta con este ítem en la mano.
    pub fn de_semilla(item: &str) -> Option<Planta> {
        Planta::TODAS.into_iter().find(|p| p.semilla() == item)
    }

    pub fn nombre(self) -> &'static str {
        match self {
            Planta::Trigo => "Trigo",
            Planta::Zanahoria => "Zanahoria",
            Planta::Calabaza => "Calabaza",
        }
    }

    fn clave(self) -> &'static str {
        match self {
            Planta::Trigo => "trigo",
            Planta::Zanahoria => "zanahoria",
            Planta::Calabaza => "calabaza",
        }
    }

    /// Color del quad de respaldo si el arte no cargó (nunca debería).
    fn color(self) -> Color {
        match self {
            Planta::Trigo => Color::srgb(0.84, 0.70, 0.36),
            Planta::Zanahoria => Color::srgb(0.87, 0.49, 0.19),
            Planta::Calabaza => Color::srgb(0.86, 0.52, 0.20),
        }
    }

    /// El slot de arte de cada etapa. La 0 (recién sembrado) es común a todas.
    pub fn arte(self, etapa: u8) -> String {
        match etapa {
            0 => "cultivo/sembrado".to_string(),
            e => format!("cultivo/{}_{}", self.clave(), e.min(3)),
        }
    }

    /// Lo que da una planta LISTA: `(ítem, cantidad)` + las semillas de vuelta.
    /// `r` es un número 0..1 determinista de la cosecha.
    pub fn cosecha(self, r: f32) -> Vec<(&'static str, u32)> {
        match self {
            // Minecraft: 1 trigo + 0-3 semillas. Acá 1-3, para que la huerta
            // siempre pueda crecer: una cosecha que no devuelve semilla es una
            // parcela que se pierde por mala suerte.
            Planta::Trigo => vec![("wheat", 1), ("wheat_seeds", 1 + (r * 2.99) as u32)],
            // La zanahoria ES la semilla: 2-4 por planta (Minecraft: 2-5).
            Planta::Zanahoria => vec![("carrot", 2 + (r * 2.99) as u32)],
            // La calabaza no devuelve semilla sola: se parte en la mesa (4).
            Planta::Calabaza => vec![("pumpkin", 1)],
        }
    }
}

/// Etapa visible (0..=3) de un crecimiento 0..1. Lo MADURO es exactamente 1: la
/// última etapa no se ve hasta que la planta está lista para cosechar.
pub fn etapa(crecimiento: f32) -> u8 {
    if crecimiento >= 1.0 {
        3
    } else if crecimiento >= 0.5 {
        2
    } else if crecimiento >= 0.15 {
        1
    } else {
        0
    }
}

/// Cuánto crece (fracción de 0..1) en `dt` segundos de juego.
/// `luz` es `DayClock::daylight` (0 noche, 1 día).
pub fn crecer(dt: f32, luz: f32, regado: bool) -> f32 {
    let sol = NOCHE_X + (1.0 - NOCHE_X) * luz.clamp(0.0, 1.0);
    let agua = if regado { RIEGO_X } else { 1.0 };
    dt * sol * agua / MADURAR_SECS
}

/// ¿Se puede sembrar sobre este suelo? Pasto y tierra, como el `farmland` de
/// Minecraft. La arena no (salvo que un día haya caña de azúcar).
pub fn suelo_sembrable(t: Tile) -> bool {
    matches!(t, Tile::Grass | Tile::Dirt)
}

/// **Qué deja una planta del mundo al arrancarla.** `clave` es su slot de arte y
/// `h` un hash estable del objeto. Minecraft suelta semillas de trigo del pasto
/// alto (12,5 %); acá las plantas sueltas son menos y están más lejos, así que
/// la chance es mayor: 55 % trigo, 8 % zanahoria, 6 % calabaza.
pub fn semillas_de_planta(clave: &str, h: u32) -> Option<(&'static str, u32)> {
    if !es_planta_silvestre(clave) {
        return None;
    }
    let r = (mezclar(h) % 1000) as f32 / 1000.0;
    if r < 0.06 {
        Some(("pumpkin_seeds", 1))
    } else if r < 0.14 {
        Some(("carrot", 1))
    } else if r < 0.69 {
        Some(("wheat_seeds", 1 + (mezclar(h ^ 0x5eed) % 2)))
    } else {
        None
    }
}

/// Plantas del mundo (no muebles con planta: una maceta es un objeto).
pub fn es_planta_silvestre(clave: &str) -> bool {
    let Some(nombre) = clave.strip_prefix("nature/") else {
        return false;
    };
    if nombre.contains("maceta") || nombre.contains("florero") || nombre.contains("banco") {
        return false;
    }
    ["pasto", "arbusto", "brote", "flor", "mata", "hierba", "helecho", "yuyo", "planta"]
        .iter()
        .any(|p| nombre.contains(p))
}

fn mezclar(mut x: u32) -> u32 {
    x ^= x >> 16;
    x = x.wrapping_mul(0x7feb_352d);
    x ^= x >> 15;
    x = x.wrapping_mul(0x846c_a68b);
    x ^ (x >> 16)
}

// ─── Estado persistente ──────────────────────────────────────────────────────

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Cultivo {
    pub body: CelestialBody,
    pub x: i64,
    pub z: i64,
    pub planta: Planta,
    /// 0..1; 1 = listo.
    pub crecimiento: f32,
    /// Hay agua cerca: se recalcula cuando el cultivo está a la vista (el mapa de
    /// tiles sólo sabe de lo que está cargado) y se recuerda para cuando no.
    #[serde(default)]
    pub regado: bool,
}

#[derive(Resource, Default)]
pub struct Huerta {
    pub cultivos: Vec<Cultivo>,
    /// Cuántas cosechas van — alimenta el azar determinista de la cosecha.
    cosechas: u32,
    dirty: bool,
    was_paused: bool,
    debounce: f32,
    /// Reloj de juego visto la última vez (para crecer con su delta).
    ultimo_t: Option<f32>,
}

impl Huerta {
    pub fn en(&self, body: CelestialBody, x: i64, z: i64) -> Option<usize> {
        self.cultivos
            .iter()
            .position(|c| c.body == body && c.x == x && c.z == z)
    }

    pub fn sembrar(&mut self, body: CelestialBody, x: i64, z: i64, planta: Planta) -> bool {
        if self.en(body, x, z).is_some() {
            return false;
        }
        self.cultivos.push(Cultivo { body, x, z, planta, crecimiento: 0.0, regado: false });
        self.dirty = true;
        true
    }

    /// Avanza todo lo sembrado `dt` segundos de juego.
    pub fn avanzar(&mut self, dt: f32, luz: f32) {
        let mut cambio = false;
        for c in &mut self.cultivos {
            if c.crecimiento >= 1.0 {
                continue;
            }
            c.crecimiento = (c.crecimiento + crecer(dt, luz, c.regado)).min(1.0);
            cambio = true;
        }
        if cambio {
            self.dirty = true;
        }
    }

    /// Saca el cultivo y devuelve lo que da: la cosecha si estaba listo, o la
    /// semilla si se arrancó verde.
    pub fn cosechar(&mut self, i: usize) -> Vec<(&'static str, u32)> {
        let c = self.cultivos.swap_remove(i);
        self.dirty = true;
        if c.crecimiento >= 1.0 {
            self.cosechas = self.cosechas.wrapping_add(1);
            let h = mezclar((c.x as u32) ^ (c.z as u32).rotate_left(16) ^ self.cosechas);
            c.planta.cosecha((h % 1000) as f32 / 1000.0)
        } else {
            vec![(c.planta.semilla(), 1)]
        }
    }
}

#[derive(Serialize, Deserialize, Default)]
struct HuertaSave {
    cultivos: Vec<Cultivo>,
    #[serde(default)]
    cosechas: u32,
}

fn huerta_path() -> PathBuf {
    persist_dir().join("cultivos.json")
}

fn cargar_huerta(mut huerta: ResMut<Huerta>) {
    let save = std::fs::read_to_string(huerta_path())
        .ok()
        .and_then(|t| serde_json::from_str::<HuertaSave>(&t).ok())
        .unwrap_or_default();
    *huerta = Huerta { cultivos: save.cultivos, cosechas: save.cosechas, ..default() };
}

fn guardar_huerta(huerta: &Huerta) {
    let save = HuertaSave { cultivos: huerta.cultivos.clone(), cosechas: huerta.cosechas };
    if let Ok(json) = serde_json::to_string(&save) {
        if let Err(e) = std::fs::write(huerta_path(), json) {
            eprintln!("ha-granja: no se pudo guardar: {e}");
        }
    }
}

fn guardado_diferido(time: Res<Time>, mut huerta: ResMut<Huerta>) {
    let paused = is_app_paused();
    if paused && !huerta.was_paused && huerta.dirty {
        guardar_huerta(&huerta);
        huerta.dirty = false;
    }
    huerta.was_paused = paused;
    if !huerta.dirty {
        return;
    }
    huerta.debounce += time.delta_secs();
    // Crecer ensucia cada cuadro: se guarda a lo sumo cada 10 s. Sembrar y
    // cosechar guardan en el acto (ver `forzar_guardado`).
    if huerta.debounce >= 10.0 {
        guardar_huerta(&huerta);
        huerta.dirty = false;
        huerta.debounce = 0.0;
    }
}

fn forzar_guardado(huerta: &mut Huerta) {
    guardar_huerta(huerta);
    huerta.dirty = false;
    huerta.debounce = 0.0;
}

// ─── Plugin ──────────────────────────────────────────────────────────────────

pub struct GranjaPlugin;

impl Plugin for GranjaPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Huerta>()
            .init_resource::<CultivosVivos>()
            .add_systems(OnEnter(crate::game_phase::GamePhase::Loading), cargar_huerta)
            .add_systems(
                Update,
                (
                    crecer_con_el_reloj,
                    sembrar_al_completar.before(crate::hud::update_interact_hud),
                    cosechar_al_completar.before(crate::hud::update_interact_hud),
                    sincronizar_cultivos,
                    animar_globos,
                    guardado_diferido,
                )
                    .chain()
                    .run_if(crate::game_phase::world_active),
            );
    }
}

/// Un cultivo materializado: la tierra arada (plana) y la planta (Y-sorted, es
/// la que se toca).
#[derive(Component)]
pub struct CultivoEnt {
    pub body: CelestialBody,
    pub x: i64,
    pub z: i64,
}

#[derive(Component)]
struct TierraArada;

#[derive(Resource, Default)]
struct CultivosVivos {
    /// (body, x, z) → (tierra, planta, etapa dibujada, regado dibujado, % del cartel)
    mapa: HashMap<(CelestialBody, i64, i64), (Entity, Entity, u8, bool, u32)>,
    acc: f32,
    /// Versión del mapa de tiles con la que se miró el agua por última vez: el
    /// barrido de 9×9 celdas por cultivo sólo se repite si el mundo cambió (se
    /// cavó un canal, se tapó un charco) o entró un cultivo nuevo a la vista.
    agua_version: u64,
}

fn crecer_con_el_reloj(clock: Res<DayClock>, mut huerta: ResMut<Huerta>) {
    let t = clock.t;
    let dt = match huerta.ultimo_t {
        // Un salto del reloj (dormir, un driver que adelanta el día) cuenta
        // entero hasta un día; más que eso es un reloj reiniciado, no tiempo.
        Some(prev) => (t - prev).clamp(0.0, DAY_SECS),
        None => 0.0,
    };
    huerta.ultimo_t = Some(t);
    if dt > 0.0 && !huerta.cultivos.is_empty() {
        huerta.avanzar(dt, clock.daylight());
    }
}

/// ¿Hay agua a [`RADIO_AGUA`] celdas? Mira el mapa de tiles, así que sólo vale
/// para lo que está cargado — por eso se recuerda en el cultivo.
fn hay_agua_cerca(tiles: &TileWorld, body: CelestialBody, x: i64, z: i64) -> bool {
    for dz in -RADIO_AGUA..=RADIO_AGUA {
        for dx in -RADIO_AGUA..=RADIO_AGUA {
            if matches!(tiles.tile_at(body, x + dx, z + dz), Tile::Water | Tile::WaterDeep) {
                return true;
            }
        }
    }
    false
}

#[allow(clippy::too_many_arguments)]
fn sembrar_al_completar(
    mut commands: Commands,
    mut hold: ResMut<HoldInteractState>,
    target: Res<LookTarget>,
    ctx: Res<crate::scene::WorldContextRuntime>,
    planet: Res<ActivePlanet>,
    tiles: Res<TileWorld>,
    mut equipped: ResMut<EquippedItem>,
    mut huerta: ResMut<Huerta>,
    mut inv: ResMut<PlayerInventory>,
    mut inv_ui: ResMut<InventoryUiState>,
    mut persist: ResMut<InventoryPersistState>,
    mut flora: ResMut<crate::harvest::FloraHarvest>,
    matas: Query<(Entity, &InteractTarget, &crate::harvest::PropArt)>,
) {
    if !hold.completed || !hold.placing_crop {
        return;
    }
    hold.completed = false;
    hold.placing_crop = false;
    let Some((tx, tz, _)) = target.place_cell else { return };
    let Some(planta) = Planta::de_semilla(&equipped.item_id) else { return };
    if inv.qty(planta.semilla()) == 0 {
        return;
    }
    let body = active_surface_body(&ctx, &planet);
    let (x, z) = (tx as i64, tz as i64);
    if !suelo_sembrable(tiles.tile_at(body, x, z)) || tiles.solid_at(body, x, z) {
        return;
    }
    if !huerta.sembrar(body, x, z, planta) {
        return;
    }
    if let Some(i) = huerta.en(body, x, z) {
        huerta.cultivos[i].regado = hay_agua_cerca(&tiles, body, x, z);
    }
    // **Desmalezar**: la mata silvestre que había en la celda se va (y queda ida:
    // se anota como cosechada, si no el chunk la re-siembra encima del surco).
    // Los props sembrados llevan de id el hash de su celda (`world2d::pack_cell`).
    let id_celda = crate::world2d::pack_cell(x, z);
    for (e, t, arte) in &matas {
        if t.kind == InteractTargetKind::Prop && t.id == id_celda && es_planta_silvestre(&arte.0) {
            let (kx, kz) = crate::targeting::removed_key(InteractTargetKind::Prop);
            flora.harvest(body, kx, kz, t.id);
            commands.entity(e).despawn_recursive();
        }
    }
    forzar_guardado(&mut huerta);
    consume_from_ui_slots(&mut inv_ui, &[(planta.semilla(), 1)]);
    flush_slots_to_inventory(&inv_ui, &mut inv);
    persist.mark_immediate();
    if inv.qty(planta.semilla()) == 0 {
        equipped.item_id = "hand".to_string();
    }
    globo(&mut commands, x as f32 + 0.5, z as f32, format!("{} sembrado", planta.nombre()));
}

#[allow(clippy::too_many_arguments)]
fn cosechar_al_completar(
    mut commands: Commands,
    mut hold: ResMut<HoldInteractState>,
    target: Res<LookTarget>,
    ents: Query<&CultivoEnt>,
    mut huerta: ResMut<Huerta>,
    mut inv: ResMut<PlayerInventory>,
    mut inv_ui: ResMut<InventoryUiState>,
    mut persist: ResMut<InventoryPersistState>,
) {
    if !hold.completed || target.kind != Some(InteractTargetKind::Crop) {
        return;
    }
    hold.completed = false;
    let Some(c) = target.entity.and_then(|e| ents.get(e).ok()) else { return };
    let Some(i) = huerta.en(c.body, c.x, c.z) else { return };
    let listo = huerta.cultivos[i].crecimiento >= 1.0;
    let nombre = huerta.cultivos[i].planta.nombre();
    let botin = huerta.cosechar(i);
    forzar_guardado(&mut huerta);
    let mut texto = Vec::new();
    for (item, n) in &botin {
        add_item_to_slots(&mut inv_ui, item, *n);
        texto.push(format!("+{n} {}", nombre_item(item)));
    }
    flush_slots_to_inventory(&inv_ui, &mut inv);
    persist.mark_immediate();
    let msg = if listo {
        texto.join("  ")
    } else {
        format!("{nombre} arrancado verde: {}", texto.join(" "))
    };
    globo(&mut commands, c.x as f32 + 0.5, c.z as f32, msg);
}

/// Nombre en castellano de un ítem (el catálogo de slots lo tiene; el de
/// `ha_shared::items` está en inglés).
pub fn nombre_item(item: &str) -> String {
    ha_assets::slots::slot_label(&format!("items/{item}"))
        .map(str::to_string)
        .unwrap_or_else(|| ha_shared::item_display_name(item).to_string())
}

/// El cartel del cultivo: "Trigo · creciendo 62 %" o "Trigo · ¡listo!".
pub fn rotulo(planta: Planta, crecimiento: f32, regado: bool) -> String {
    let e = etapa(crecimiento);
    let pct = (crecimiento * 100.0).floor() as u32;
    let agua = if regado { " · regado" } else { "" };
    match e {
        3 => format!("{} · ¡listo para cosechar!", planta.nombre()),
        0 => format!("{} · sembrado {pct} %{agua}", planta.nombre()),
        1 => format!("{} · brote {pct} %{agua}", planta.nombre()),
        _ => format!("{} · creciendo {pct} %{agua}", planta.nombre()),
    }
}

#[allow(clippy::too_many_arguments, clippy::type_complexity)]
fn sincronizar_cultivos(
    mut commands: Commands,
    time: Res<Time>,
    art: Res<crate::art2d::ArtLibrary>,
    ctx: Res<crate::scene::WorldContextRuntime>,
    planet: Res<ActivePlanet>,
    tiles: Res<TileWorld>,
    player: Query<&Transform, With<Player>>,
    mut huerta: ResMut<Huerta>,
    mut vivos: ResMut<CultivosVivos>,
    mut sprites: Query<&mut Sprite>,
    mut names: Query<&mut crate::structures::StructureName>,
) {
    vivos.acc += time.delta_secs();
    // Cuatro veces por segundo alcanza: un cultivo tarda media hora en crecer.
    if vivos.acc < 0.25 {
        return;
    }
    vivos.acc = 0.0;
    let body = active_surface_body(&ctx, &planet);
    let Ok(ptf) = player.get_single() else { return };
    let (px, pz) = (ptf.translation.x, -ptf.translation.y);
    let r2 = RADIO_VISTA * RADIO_VISTA;
    let en_superficie = tiles.z == 0;

    // Lo que tiene que estar a la vista.
    let mut quiero: HashMap<(CelestialBody, i64, i64), usize> = HashMap::new();
    if en_superficie {
        for (i, c) in huerta.cultivos.iter().enumerate() {
            if c.body != body {
                continue;
            }
            let (dx, dz) = (c.x as f32 + 0.5 - px, c.z as f32 + 0.5 - pz);
            if dx * dx + dz * dz <= r2 {
                quiero.insert((c.body, c.x, c.z), i);
            }
        }
    }
    // Sale de la vista (o se cosechó): se despawnea.
    let fuera: Vec<_> = vivos.mapa.keys().filter(|k| !quiero.contains_key(k)).copied().collect();
    for k in fuera {
        if let Some((t, p, ..)) = vivos.mapa.remove(&k) {
            commands.entity(t).despawn_recursive();
            commands.entity(p).despawn_recursive();
        }
    }
    let mirar_agua_todo = vivos.agua_version != tiles.version;
    vivos.agua_version = tiles.version;
    for (k, i) in quiero {
        // El agua se mira con el mapa cargado; el cultivo lo recuerda.
        if mirar_agua_todo || !vivos.mapa.contains_key(&k) {
            let regado = hay_agua_cerca(&tiles, k.0, k.1, k.2);
            if huerta.cultivos[i].regado != regado {
                huerta.cultivos[i].regado = regado;
                huerta.dirty = true;
            }
        }
        let regado = huerta.cultivos[i].regado;
        let c = huerta.cultivos[i].clone();
        let e = etapa(c.crecimiento);
        let pct = (c.crecimiento * 100.0).floor() as u32;
        let base = render_xy(c.x as f32 + 0.5, c.z as f32 + 0.5);
        match vivos.mapa.get_mut(&k) {
            Some((tierra, planta, etapa_vista, regado_visto, pct_visto)) => {
                if *etapa_vista != e {
                    if let Ok(mut s) = sprites.get_mut(*planta) {
                        *s = art.sprite_or_color(&c.planta.arte(e), c.planta.color(), Vec2::ONE);
                    }
                    *etapa_vista = e;
                }
                if *regado_visto != regado {
                    if let Ok(mut s) = sprites.get_mut(*tierra) {
                        *s = sprite_tierra(&art, regado);
                    }
                    *regado_visto = regado;
                }
                if *pct_visto != pct {
                    if let Ok(mut n) = names.get_mut(*planta) {
                        n.0 = rotulo(c.planta, c.crecimiento, regado);
                    }
                    *pct_visto = pct;
                }
            }
            None => {
                let tierra = commands
                    .spawn((
                        sprite_tierra(&art, regado),
                        Transform::from_xyz(
                            base.x,
                            base.y,
                            crate::layers::FLOOR + crate::layers::SOBRE_EL_PISO * 2.0,
                        ),
                        TierraArada,
                        Name::new("TierraArada"),
                    ))
                    .id();
                let planta = commands
                    .spawn((
                        art.sprite_or_color(&c.planta.arte(e), c.planta.color(), Vec2::ONE),
                        // Pie al borde sur de la celda: se ordena con el jugador
                        // como cualquier cosa que se levanta del suelo.
                        Transform::from_xyz(base.x, base.y, sort_z(c.z as f32 + 1.0)),
                        CultivoEnt { body: c.body, x: c.x, z: c.z },
                        InteractTarget { kind: InteractTargetKind::Crop, id: 0 },
                        crate::structures::StructureName(rotulo(c.planta, c.crecimiento, regado)),
                        Name::new("Cultivo"),
                    ))
                    .id();
                vivos.mapa.insert(k, (tierra, planta, e, regado, pct));
            }
        }
    }
}

fn sprite_tierra(art: &crate::art2d::ArtLibrary, regado: bool) -> Sprite {
    let key = if regado { "cultivo/tierra_regada" } else { "cultivo/tierra_arada" };
    art.sprite_or_color(key, Color::srgb(0.45, 0.32, 0.22), Vec2::ONE)
}

// ─── Globitos de texto ("+2 Trigo") ──────────────────────────────────────────

/// Texto que sube y se apaga sobre algo del mundo. Es la respuesta de la granja:
/// el jugador tiene que VER que sembró, que cosechó o que la vaca quedó en celo.
#[derive(Component)]
pub struct Globo {
    vida: f32,
}

pub const GLOBO_VIDA: f32 = 1.8;

pub fn globo(commands: &mut Commands, x: f32, z: f32, texto: String) {
    let p = render_xy(x, z);
    commands.spawn((
        Text2d::new(texto),
        TextFont { font_size: 22.0, ..default() },
        TextColor(Color::srgb(1.0, 0.97, 0.80)),
        Transform::from_xyz(p.x, p.y + 1.2, crate::layers::GLOBO).with_scale(Vec3::splat(0.045)),
        Globo { vida: GLOBO_VIDA },
        crate::textos_mundo::TextoEnMundo { px: 16.0, font_size: 22.0 },
        Name::new("GloboGranja"),
    ));
}

fn animar_globos(
    mut commands: Commands,
    time: Res<Time>,
    mut q: Query<(Entity, &mut Globo, &mut Transform, &mut TextColor)>,
) {
    let dt = time.delta_secs();
    for (e, mut g, mut tf, mut color) in &mut q {
        g.vida -= dt;
        if g.vida <= 0.0 {
            commands.entity(e).despawn_recursive();
            continue;
        }
        tf.translation.y += dt * 0.6;
        color.0.set_alpha((g.vida / GLOBO_VIDA * 1.5).min(1.0));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cada_semilla_planta_lo_suyo_y_vuelve() {
        for p in Planta::TODAS {
            assert_eq!(Planta::de_semilla(p.semilla()), Some(p));
        }
        assert_eq!(Planta::de_semilla("wood"), None);
    }

    /// Las cuatro etapas que pidió el usuario: sembrado, chiquita, mediana, lista.
    #[test]
    fn las_etapas_van_en_orden_y_lo_maduro_es_exacto() {
        assert_eq!(etapa(0.0), 0);
        assert_eq!(etapa(0.2), 1);
        assert_eq!(etapa(0.6), 2);
        assert_eq!(etapa(0.999), 2, "casi listo no es listo");
        assert_eq!(etapa(1.0), 3);
    }

    /// Ligado al reloj: a pleno sol y sin agua tarda [`MADURAR_SECS`] de juego;
    /// con riego menos; de noche mucho más. Es la promesa del módulo.
    #[test]
    fn crece_con_el_reloj_del_juego() {
        let mut h = Huerta::default();
        h.sembrar(CelestialBody::Earth, 0, 0, Planta::Trigo);
        h.sembrar(CelestialBody::Earth, 5, 0, Planta::Trigo);
        h.cultivos[1].regado = true;
        let paso = 1.0;
        let mut t = 0.0;
        while h.cultivos[0].crecimiento < 1.0 {
            h.avanzar(paso, 1.0);
            t += paso;
            assert!(t < DAY_SECS * 2.0, "no madura nunca");
        }
        assert!((t - MADURAR_SECS).abs() <= 2.0, "a pleno sol madura en {t} s, no {MADURAR_SECS}");
        assert!(h.cultivos[1].crecimiento >= 1.0, "el regado madura antes");
        // De noche avanza a un cuarto.
        assert!((crecer(100.0, 0.0, false) - crecer(100.0, 1.0, false) * NOCHE_X).abs() < 1e-6);
    }

    #[test]
    fn un_dia_de_juego_entero_madura_cualquier_cultivo_regado() {
        // Un día real de juego: ~1/3 de noche. Con riego alcanza y sobra.
        let noche = DAY_SECS * 0.35;
        let dia = DAY_SECS - noche;
        let total = crecer(dia, 1.0, true) + crecer(noche, 0.0, true);
        assert!(total >= 1.0, "con riego, un día entero da {total}");
    }

    #[test]
    fn no_se_siembra_dos_veces_la_misma_celda() {
        let mut h = Huerta::default();
        assert!(h.sembrar(CelestialBody::Earth, 3, 4, Planta::Zanahoria));
        assert!(!h.sembrar(CelestialBody::Earth, 3, 4, Planta::Trigo));
        // Otra celda, u otro cuerpo, sí.
        assert!(h.sembrar(CelestialBody::Mars, 3, 4, Planta::Trigo));
    }

    /// Minecraft: el trigo verde devuelve su semilla; el maduro da trigo Y
    /// semillas (acá siempre al menos una, para que la huerta no se pierda).
    #[test]
    fn cosechar_verde_devuelve_la_semilla_y_maduro_da_cosecha() {
        let mut h = Huerta::default();
        h.sembrar(CelestialBody::Earth, 0, 0, Planta::Trigo);
        assert_eq!(h.cosechar(0), vec![("wheat_seeds", 1)]);
        for _ in 0..20 {
            h.sembrar(CelestialBody::Earth, 0, 0, Planta::Trigo);
            h.cultivos[0].crecimiento = 1.0;
            let botin = h.cosechar(0);
            assert!(botin.contains(&("wheat", 1)));
            let semillas = botin.iter().find(|(i, _)| *i == "wheat_seeds").unwrap().1;
            assert!((1..=3).contains(&semillas));
        }
        h.sembrar(CelestialBody::Earth, 0, 0, Planta::Zanahoria);
        h.cultivos[0].crecimiento = 1.0;
        let z = h.cosechar(0);
        assert!((2..=4).contains(&z[0].1) && z[0].0 == "carrot");
        assert!(h.cultivos.is_empty());
    }

    #[test]
    fn se_siembra_en_pasto_y_tierra_no_en_agua_ni_roca() {
        assert!(suelo_sembrable(Tile::Grass));
        assert!(suelo_sembrable(Tile::Dirt));
        assert!(!suelo_sembrable(Tile::Water));
        assert!(!suelo_sembrable(Tile::Sand));
        assert!(!suelo_sembrable(Tile::Stone));
    }

    /// Arrancar pasto a veces da semillas, y de las tres plantas; una maceta o
    /// un banco no dan nada.
    #[test]
    fn las_plantas_del_mundo_sueltan_semillas() {
        let mut vistas = std::collections::HashSet::new();
        let mut algo = 0;
        for h in 0..2000u32 {
            if let Some((item, n)) = semillas_de_planta("nature/pasto_5", h) {
                assert!(n >= 1);
                vistas.insert(item);
                algo += 1;
            }
        }
        assert_eq!(vistas.len(), 3, "salen las tres semillas: {vistas:?}");
        assert!((1000..1600).contains(&algo), "~69 % de las veces: {algo}");
        assert!(es_planta_silvestre("nature/arbusto_con_flores_1"));
        assert!(!es_planta_silvestre("nature/arbusto_en_maceta_3"));
        assert!(!es_planta_silvestre("prop/banco"));
        assert!(semillas_de_planta("prop/farol", 7).is_none());
    }

    /// Cada etapa de cada planta tiene su slot de arte declarado (si no, sale un
    /// cuadrado de color en el mundo).
    #[test]
    fn cada_etapa_tiene_slot_de_arte() {
        for p in Planta::TODAS {
            for e in 0..=3 {
                let k = p.arte(e);
                assert!(ha_assets::slots::slot_label(&k).is_some(), "falta el slot {k}");
            }
        }
        assert!(ha_assets::slots::slot_label("cultivo/tierra_arada").is_some());
        assert!(ha_assets::slots::slot_label("cultivo/tierra_regada").is_some());
    }

    /// Y el manifest tiene PIXEL para cada uno (lo publica `dibujar-granja.py`).
    #[test]
    fn el_manifest_tiene_arte_para_la_huerta() {
        let raw = std::fs::read_to_string(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("assets/art/manifest.json"),
        )
        .unwrap();
        let man: serde_json::Value = serde_json::from_str(&raw).unwrap();
        let sp = &man["sprites"];
        for p in Planta::TODAS {
            for e in 0..=3 {
                assert!(sp.get(p.arte(e)).is_some(), "sin arte: {}", p.arte(e));
            }
        }
        for item in ["wheat_seeds", "wheat", "carrot", "pumpkin_seeds", "pumpkin"] {
            assert!(sp.get(format!("items/{item}")).is_some(), "sin icono: {item}");
        }
    }
}
