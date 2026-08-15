//! **Antorchas colocables** — suelo y pared, tipo Minecraft.
//!
//! Es la mecánica que convierte la iluminación en juego en vez de decorado: la
//! noche y las cuevas son oscuras de verdad (`sky2d::NIGHT_VEIL`), así que
//! avanzar por ellas es ir dejando luz detrás tuyo. Cada antorcha colocada es
//! un `Light2d` normal, o sea que pasa por exactamente el mismo camino que un
//! farol del pueblo o la llama en tu mano: agujerea el velo, pinta el suelo de
//! su color y tiñe los sprites de alrededor. **No hay iluminación "de antorcha"
//! aparte** — si algo declara que emite, emite.
//!
//! Dónde se pega la decide el mundo, no un modo aparte: si tenés un bloque
//! sólido delante, la antorcha se monta en su cara; si no, se planta en el
//! suelo. Igual que en Minecraft, y sin un botón extra que aprender.
//!
//! Persistencia y streaming copian el patrón de `structures.rs` (JSON en
//! `persist_dir()`, flush al pausar, entidades sólo cerca del jugador).

use bevy::prelude::*;
use serde::{Deserialize, Serialize};

use ha_shared::CelestialBody;

use crate::art2d::ArtLibrary;
use crate::input::{EquippedItem, HoldInteractState};
use crate::inventory_persist::persist_dir;
use crate::light2d::Light2d;
use crate::platform::is_app_paused;
use crate::scene::{RegionRuntime, WorldContextRuntime};
use crate::sim::{Player, SimState};
use crate::world2d::{
    active_surface_body, render_xy, sort_z, surface_region_active, ActivePlanet, TileWorld,
};

/// Radio (m) alrededor del jugador donde las antorchas existen como entidad.
const TORCH_STREAM_R: f32 = 70.0;

/// Alto del sprite de la antorcha en metros.
const TORCH_H: f32 = 0.85;

/// Cuánto se separa de la pared una antorcha montada (m). Sale de la CARA del
/// bloque hacia vos: pegada al centro del bloque quedaría media enterrada.
const WALL_OUT: f32 = 0.34;

/// Una clase de antorcha. El `id` es también el id de ítem del inventario, así
/// que agregar un color nuevo es una línea acá y nada más.
pub struct TorchVariant {
    pub id: &'static str,
    pub label: &'static str,
    /// Color de la llama Y de la luz que emite: son lo mismo a propósito. Si el
    /// halo y el charco no coincidieran, la fuente se leería de un color y su
    /// luz de otro — el error más fácil de cometer y el más feo de ver.
    pub color: Color,
    /// Radio del emisor (m). El pozo real mide `radius * light2d::LIGHT_REACH`.
    pub radius: f32,
    /// 0 = llama quieta (cristal, hongo); 1 = fuego vivo.
    pub flicker: f32,
    /// Slot de arte opcional (editor de assets). Si no existe, se dibuja
    /// procedural: palo + llama. Así las cuevas se pueden iluminar HOY y el arte
    /// entra después sin tocar código.
    pub art: &'static str,
}

/// El catálogo de antorchas. Los colores fríos son los que mejor funcionan bajo
/// tierra: la sombra de la cueva tira a azul, así que una llama naranja da el
/// contraste complementario y una de cristal azul da la vibra de gruta profunda.
pub static TORCH_VARIANTS: &[TorchVariant] = &[
    TorchVariant {
        id: "torch",
        label: "Antorcha",
        color: Color::srgb(1.0, 0.62, 0.24),
        radius: 4.2,
        flicker: 0.75,
        art: "decor/torch",
    },
    TorchVariant {
        id: "torch_blue",
        label: "Antorcha azul",
        color: Color::srgb(0.36, 0.70, 1.0),
        radius: 4.0,
        flicker: 0.35,
        art: "decor/torch_blue",
    },
    TorchVariant {
        id: "torch_green",
        label: "Antorcha verde",
        color: Color::srgb(0.40, 1.0, 0.48),
        radius: 3.8,
        flicker: 0.30,
        art: "decor/torch_green",
    },
    TorchVariant {
        id: "torch_violet",
        label: "Antorcha violeta",
        color: Color::srgb(0.72, 0.42, 1.0),
        radius: 3.8,
        flicker: 0.28,
        art: "decor/torch_violet",
    },
    TorchVariant {
        id: "torch_ember",
        label: "Brasa",
        color: Color::srgb(1.0, 0.30, 0.12),
        radius: 3.0,
        flicker: 0.9,
        art: "decor/torch_ember",
    },
];

/// `Some` si el ítem equipado es una antorcha colocable.
pub fn torch_variant(item_id: &str) -> Option<&'static TorchVariant> {
    TORCH_VARIANTS.iter().find(|v| v.id == item_id)
}

fn variant_by_index(i: u8) -> &'static TorchVariant {
    TORCH_VARIANTS.get(i as usize).unwrap_or(&TORCH_VARIANTS[0])
}

/// Cómo está puesta. En pared guarda hacia dónde MIRA la llama (la dirección en
/// la que sale del bloque), que es lo que decide el offset del sprite.
#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Debug)]
pub enum TorchMount {
    Floor,
    /// Montada en la cara de un bloque: `(dx, dz)` es la normal de esa cara.
    Wall(i8, i8),
}

/// Una antorcha colocada. Se persiste tal cual.
#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Debug)]
pub struct PlacedTorch {
    pub body: u8,
    /// Nivel Z (subsuelo negativo): una antorcha en la cueva no alumbra arriba.
    pub z: i32,
    pub x: f32,
    pub zz: f32,
    pub mount: TorchMount,
    pub variant: u8,
}

#[derive(Resource, Default)]
pub struct PlacedTorches {
    pub list: Vec<PlacedTorch>,
    dirty: bool,
    was_paused: bool,
    debounce: f32,
}

/// Entidad viva de una antorcha (para el despawn por streaming).
#[derive(Component)]
struct TorchEntity {
    idx: usize,
}

pub struct TorchesPlugin;

impl Plugin for TorchesPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<PlacedTorches>()
            .add_systems(Startup, load_torches)
            .add_systems(Update, (place_torch_on_hold, stream_torches, save_torches));
    }
}

// ── Colocar ──────────────────────────────────────────────────────────────────

/// ¿Hay un bloque sólido en la celda `(tx,tz)` del nivel actual?
fn solid(tiles: &TileWorld, body: CelestialBody, tx: i64, tz: i64) -> bool {
    tiles.solid_at(body, tx, tz)
}

/// Decide dónde va la antorcha: **el mundo elige el modo**.
///
/// `at` es el punto que tocaste; `from`, dónde estás parado. Si la celda tocada
/// es sólida, la antorcha se pega a su cara (montada en pared) y si no, se planta
/// en el suelo ahí mismo. No hay modo "pared" que activar: es la misma acción, y
/// ahora la elige el dedo — tocás la pared de la cueva y queda en la pared.
fn resolve_mount(
    tiles: &TileWorld,
    body: CelestialBody,
    at: Vec2,
    from: Vec2,
) -> (f32, f32, TorchMount) {
    let (tx, tz) = (at.x.floor() as i64, at.y.floor() as i64);
    if solid(tiles, body, tx, tz) {
        // Normal de la cara = el eje dominante de la línea vos→antorcha,
        // invertido (la llama sale del bloque HACIA vos). Con el eje dominante en
        // vez del vector crudo la antorcha queda recta contra la pared y no en
        // diagonal.
        let d = at - from;
        let (nx, nz) = if d.x.abs() >= d.y.abs() {
            (-d.x.signum() as i8, 0i8)
        } else {
            (0i8, -d.y.signum() as i8)
        };
        let cx = tx as f32 + 0.5 + nx as f32 * (0.5 + WALL_OUT);
        let cz = tz as f32 + 0.5 + nz as f32 * (0.5 + WALL_OUT);
        return (cx, cz, TorchMount::Wall(nx, nz));
    }
    (at.x, at.y, TorchMount::Floor)
}

#[allow(clippy::too_many_arguments)]
fn place_torch_on_hold(
    mut hold: ResMut<HoldInteractState>,
    equipped: Res<EquippedItem>,
    region: Res<RegionRuntime>,
    world_ctx: Res<WorldContextRuntime>,
    planet: Res<ActivePlanet>,
    tiles: Res<TileWorld>,
    cursor: Res<crate::input::AimCursor>,
    player: Query<&SimState, With<Player>>,
    mut torches: ResMut<PlacedTorches>,
) {
    if !hold.completed || !hold.placing_torch {
        return;
    }
    hold.completed = false;
    hold.placing_torch = false;

    let Some(v) = torch_variant(&equipped.item_id) else {
        return;
    };
    if surface_region_active(&region).is_none() {
        return;
    }
    let Ok(sim) = player.get_single() else {
        return;
    };
    let body = active_surface_body(&world_ctx, &planet);
    let idx = TORCH_VARIANTS.iter().position(|t| t.id == v.id).unwrap_or(0) as u8;
    // Al punto TOCADO (o, sin toque, un metro largo al frente: lo justo para
    // agarrar la pared que estás tocando sin pescar la de más allá).
    let (px, pz) = crate::input::place_point(&cursor, sim, 1.1);
    let from = Vec2::new(sim.body.x, sim.body.z);
    let (x, zz, mount) = resolve_mount(&tiles, body, Vec2::new(px, pz), from);
    torches.list.push(PlacedTorch {
        body: body as u8,
        z: tiles.z,
        x,
        zz,
        mount,
        variant: idx,
    });
    torches.dirty = true;
}

// ── Streaming ────────────────────────────────────────────────────────────────

/// Sprite de la antorcha. Si el pack trae arte para el slot lo usa; si no,
/// dibuja el palo. La llama va SIEMPRE aparte, porque tiene que ser del color
/// de la variante aunque el arte del palo sea el mismo para todas.
fn torch_sprite(art: &ArtLibrary, v: &'static TorchVariant) -> Sprite {
    if let Some((s, _)) = art.world_sprite(v.art, Vec2::new(TORCH_H * 0.42, TORCH_H)) {
        return s;
    }
    // Palo procedural: madera oscura. Cero dependencia del pack — las cuevas se
    // pueden iluminar hoy y el arte entra después desde el editor.
    Sprite::from_color(Color::srgb(0.32, 0.21, 0.12), Vec2::new(TORCH_H * 0.16, TORCH_H))
}

#[allow(clippy::too_many_arguments)]
fn stream_torches(
    mut commands: Commands,
    torches: Res<PlacedTorches>,
    art: Res<ArtLibrary>,
    glow: Option<Res<crate::light2d::GlowTexture>>,
    tiles: Res<TileWorld>,
    world_ctx: Res<WorldContextRuntime>,
    planet: Res<ActivePlanet>,
    region: Res<RegionRuntime>,
    player: Query<&SimState, With<Player>>,
    live: Query<(Entity, &TorchEntity)>,
) {
    let Some(glow) = glow else { return };
    let Ok(sim) = player.get_single() else { return };
    let on_surface = surface_region_active(&region).is_some();
    let body = active_surface_body(&world_ctx, &planet);
    let here = Vec2::new(sim.body.x, sim.body.z);

    // ¿Cuáles DEBERÍAN estar vivas ahora?
    let want = |t: &PlacedTorch| {
        on_surface
            && t.body == body as u8
            && t.z == tiles.z
            && Vec2::new(t.x, t.zz).distance(here) <= TORCH_STREAM_R
    };

    let mut alive = vec![false; torches.list.len()];
    for (e, tag) in &live {
        match torches.list.get(tag.idx) {
            Some(t) if want(t) => alive[tag.idx] = true,
            // Fuera de rango, otro nivel u otro planeta: se va. La luz se apaga
            // con ella porque el `Light2d` es hijo de esta misma entidad.
            _ => commands.entity(e).despawn_recursive(),
        }
    }

    for (idx, t) in torches.list.iter().enumerate() {
        if alive[idx] || !want(t) {
            continue;
        }
        let v = variant_by_index(t.variant);
        let r = render_xy(t.x, t.zz);
        let sprite = torch_sprite(&art, v);
        // La llama arriba del palo; en pared, un pelo hacia afuera para que se
        // vea salir de la cara del bloque y no del interior.
        let (ox, oy) = match t.mount {
            TorchMount::Floor => (0.0, TORCH_H * 0.42),
            TorchMount::Wall(nx, nz) => (nx as f32 * 0.10, TORCH_H * 0.30 - nz as f32 * 0.04),
        };
        commands
            .spawn((
                TorchEntity { idx },
                sprite,
                Transform::from_xyz(r.x, r.y, sort_z(t.zz)),
                // La luz vive en la MISMA entidad que el sprite: es el contrato de
                // `light2d`, y es lo que hace que una antorcha del suelo pase por
                // el mismo código que un farol o la llama en la mano.
                Light2d {
                    color: v.color,
                    radius: v.radius,
                    intensity: 1.1,
                    offset: Vec2::new(ox, oy),
                    flicker: v.flicker,
                    // De día también: una antorcha en una cueva no sabe qué hora es
                    // arriba, y `light2d::darkness` ya devuelve 1 bajo tierra.
                    day: false,
                    draw_glow: true,
                },
            ))
            .with_children(|p| {
                // La llama: el mismo degradé radial que todo el sistema de luz,
                // teñido con el color de la variante.
                p.spawn((
                    crate::sim::UnlitSprite,
                    Sprite {
                        image: glow.0.clone(),
                        custom_size: Some(Vec2::splat(TORCH_H * 0.55)),
                        color: v.color.with_alpha(0.95),
                        ..default()
                    },
                    Transform::from_xyz(ox, oy, 0.05),
                ));
            });
    }
}

// ── Persistencia ─────────────────────────────────────────────────────────────

fn torches_path() -> std::path::PathBuf {
    persist_dir().join("torches.json")
}

fn load_torches(mut torches: ResMut<PlacedTorches>) {
    let Ok(raw) = std::fs::read_to_string(torches_path()) else {
        return;
    };
    if let Ok(list) = serde_json::from_str::<Vec<PlacedTorch>>(&raw) {
        eprintln!("ha-torch: {} antorchas cargadas", list.len());
        torches.list = list;
    }
}

/// Flush con debounce + al pausar, igual que estructuras: colocar veinte
/// antorchas seguidas no puede ser veinte escrituras a disco en el Redmi.
fn save_torches(time: Res<Time>, mut torches: ResMut<PlacedTorches>) {
    let paused = is_app_paused();
    let just_paused = paused && !torches.was_paused;
    torches.was_paused = paused;
    if !torches.dirty {
        return;
    }
    torches.debounce += time.delta_secs();
    if !just_paused && torches.debounce < 2.0 {
        return;
    }
    torches.debounce = 0.0;
    torches.dirty = false;
    let path = torches_path();
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    if let Ok(json) = serde_json::to_string(&torches.list) {
        let _ = std::fs::write(path, json);
    }
}
