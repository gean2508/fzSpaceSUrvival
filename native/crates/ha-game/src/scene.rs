//! Region/context runtime + player spawn for the 2D world.
//!
//! `WorldContext` (gravity/locomotion) vs `RegionId` (render/visibility) keep
//! their old meanings; transitions still only happen in `flight.rs`
//! (B8) or via deliberate MENU teleports (`teleport_to_body`).

use bevy::prelude::*;
use ha_shared::topdown::{topdown_tile, TILE_M};
use ha_shared::{CelestialBody, WorldContext};

use crate::sim::{Player, PlayerVisual, SimState};
use crate::world2d::{render_xy, sort_z};

/// Find a walkable (non-solid, non-water) surface cell near a preferred world
/// point, spiralling outward. Returns world `(x, z)` metres (cell centre).
pub fn walkable_spawn(body: CelestialBody, near_x: f32, near_z: f32) -> (f32, f32) {
    let bx = (near_x / TILE_M).floor() as i64;
    let bz = (near_z / TILE_M).floor() as i64;
    for r in 0i64..64 {
        for dz in -r..=r {
            for dx in -r..=r {
                if dx.abs() != r && dz.abs() != r {
                    continue; // only the ring at radius r
                }
                let (tx, tz) = (bx + dx, bz + dz);
                let t = topdown_tile(body, 0, tx, tz);
                // Aparecer nadando en mitad del lago no es aparecer: `buildable_ground`
                // pide tierra firme (ni líquido, ni rampa, ni pared).
                if t.buildable_ground() {
                    return (tx as f32 * TILE_M + 0.5, tz as f32 * TILE_M + 0.5);
                }
            }
        }
    }
    (near_x, near_z)
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum RegionId {
    Overworld { gx: i8, gz: i8 },
    /// Legacy cave instance — superseded in 2D (digging IS the cave); kept so
    /// old persistence/UI code keeps compiling until content moves over.
    Cave(u8),
    Space,
    Luna,
    Planet(CelestialBody),
    Transit,
    /// Bounded room inside the player's ship (a carved Z-level of the Earth grid;
    /// see `ship_interior.rs`). Walkable, safe, no flight.
    ShipInterior,
}

#[derive(Resource, Default)]
pub struct WorldContextRuntime {
    pub active: WorldContext,
}

#[derive(Resource)]
pub struct RegionRuntime {
    pub active: RegionId,
    pub cave_portal: Vec3,
    pub edge_pressed: bool,
    pub cave_surface_return: Option<Vec3>,
    pub cave_return_body: CelestialBody,
}

impl Default for RegionRuntime {
    fn default() -> Self {
        Self {
            active: RegionId::Overworld { gx: 0, gz: 0 },
            cave_portal: Vec3::new(11.0, 0.0, -9.5),
            edge_pressed: false,
            cave_surface_return: None,
            cave_return_body: CelestialBody::Earth,
        }
    }
}

/// Arc position of the agency launch pad on Earth (spawn/respawn anchor).
pub const EARTH_PAD_S: f32 = ha_shared::LAUNCH_PAD_ZONE_X;

pub struct ScenePlugin;

impl Plugin for ScenePlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<WorldContextRuntime>()
            .init_resource::<RegionRuntime>()
            // El jugador (y por lo tanto el mundo, que se streamea a su alrededor)
            // NO existe hasta tocar JUGAR: se spawnea al entrar a `Loading`, o sea
            // DETRÁS de la cortina. Sin Player, `world2d::stream_tile_chunks`
            // retorna temprano → cero worldgen durante splash/menú/creación.
            .add_systems(OnEnter(crate::game_phase::GamePhase::Loading), spawn_player);
    }
}

/// Grilla de la hoja de personaje de LimeZu: 56 columnas × 20 filas de celdas
/// [`ha_assets::chargen_index::CELL`] (32×64 — el pack a resolución nativa).
/// Fila 0, direcciones base: el orden de la hoja es `R, U, L, D`.
pub const CHAR_COLS: u32 = 56;
pub const CHAR_ROWS: u32 = 20;

/// **El tamaño al que se dibuja una persona, y con él la escala de todo el arte.**
///
/// El marco del personaje de LimeZu es de 32 × 64 px = 1 × 2 celdas del pack, y
/// se dibuja de 0,95 × 1,90 m para que una persona se lea chica contra un árbol
/// (4–8 m) y contra una casa. De esa división —1,90 m entre 2 celdas— sale
/// [`crate::art2d::ESCALA_ARTE`], que es lo que mide una celda del pack en el
/// mundo. Es constante y no literal justamente para que esa cuenta se pueda
/// escribir en un test en vez de quedar en un comentario.
pub const TAMANO_JUGADOR: Vec2 = Vec2::new(0.95, 1.9);

#[allow(clippy::too_many_arguments)]
fn spawn_player(
    mut commands: Commands,
    asset_server: Res<AssetServer>,
    mut layouts: ResMut<Assets<TextureAtlasLayout>>,
    mut images: ResMut<Assets<Image>>,
    mut jobs: ResMut<crate::chargen::ChargenJobs>,
    cg: Res<crate::chargen::Chargen>,
    selected: Res<crate::game_phase::SelectedCharacter>,
    // El degradado radial compartido (`PreStartup`): es lo que convierte el aura
    // en LUZ en vez de un rectángulo de color. Ver `crate::sim::PlayerAura`.
    glow: Option<Res<crate::light2d::GlowTexture>>,
) {
    let (sx, sz) = walkable_spawn(CelestialBody::Earth, EARTH_PAD_S + 4.0, 6.0);
    let mut body = ha_shared::MovementBody::new(5.2, ha_shared::topdown::PLAYER_RADIUS);
    body.x = sx;
    body.z = sz;
    body.y = 0.0;

    let sim = SimState {
        body,
        hp: 100.0,
        max_hp: 100.0,
        survival: ha_shared::SurvivalStats::default(),
        prev_action: false,
        prev_jump: false,
    };
    let mut visual = PlayerVisual {
        radius: ha_shared::topdown::PLAYER_RADIUS,
        ..default()
    };
    visual.from_x = sx;
    visual.to_x = sx;
    visual.from_z = sz;
    visual.to_z = sz;
    let r = render_xy(sx, sz);
    // z de arranque: el Y-sort real lo pisa `sim::apply_visual` en el primer
    // frame. El `+1.0` que había acá valía UN KILÓMETRO de desempate.
    let tf = Transform::from_xyz(r.x, r.y, sort_z(sz) + crate::layers::HAIR);

    // LimeZu directional character. Frame is 32×64 (1:2); drawn at a HUMAN ~1.9 m
    // tall (0.95×1.9) so a person reads small against trees (4–8 m) and props —
    // real proportions, not a giant.
    //
    // El personaje que armaste en la pantalla de creación se compone acá a UNA
    // textura (`crate::chargen`): a partir de este punto es un sprite normal, con
    // el mismo costo que la hoja horneada de siempre. Sin `look` (partida vieja o
    // build sin capas) se carga esa hoja y la grilla es la del pack entero.
    let composed = selected
        .look
        .and_then(|look| crate::chargen::sheet_for(look, &cg, &mut jobs, &mut images, &asset_server));
    let (image, cols, rows) = match composed {
        Some(handle) => (handle, cg.index.cols.max(1), cg.index.rows.max(1)),
        None => (asset_server.load(selected.sheet.clone()), CHAR_COLS, CHAR_ROWS),
    };
    // La celda sale de `chargen_index::CELL`, no de un número escrito acá: si se
    // vuelve a cambiar la resolución del pack, el atlas del jugador tiene que
    // seguirla o se dibuja media cara.
    let cell = ha_assets::chargen_index::CELL;
    let layout = layouts.add(TextureAtlasLayout::from_grid(
        UVec2::new(cell[0], cell[1]),
        cols,
        rows,
        None,
        None,
    ));
    let mut sprite = Sprite::from_atlas_image(image, TextureAtlas { layout, index: 0 });
    sprite.custom_size = Some(TAMANO_JUGADOR);
    sprite.color = selected.tint_color();
    commands
        .spawn((Player, sim, visual, sprite, tf, crate::sim::CurrentPose::default()))
        .with_children(|p| {
            // Flight-only ground shadow (biblia §11-bis) — hidden until
            // `sync_player_shadow` turns it on for Climb/Descend.
            p.spawn((
                crate::sim::PlayerFlightShadow,
                crate::sim::UnlitSprite,
                Sprite::from_color(Color::srgba(0.0, 0.0, 0.0, 0.5), Vec2::new(0.9, 0.35)),
                // Just below the character's feet (custom_size 0.95x1.9,
                // centre-anchored → bottom at y=-0.95) so it reads as cast ON
                // the ground, not smudged into the sprite's legs.
                Transform::from_xyz(0.0, -1.0, -0.05),
                Visibility::Hidden,
            ));

            // AURA DE KI — tres halos radiales concéntricos, no un cuadro.
            //
            // El aura vieja era UN `Sprite::from_color` amarillo: un quad de color
            // plano, o sea un rectángulo con bordes rectos y duros. De ahí el
            // "cuadros amarillos cutres". Estas tres capas usan el degradado radial
            // compartido (`light2d::GlowTexture`, alfa gaussiana que llega a 0 en el
            // borde), así que no hay borde que ver, y al apilarse de ancho-tenue a
            // chico-intenso el alfa acumulado crece hacia el centro: ESO es el
            // volumen. Los tonos van de naranja profundo afuera a dorado casi blanco
            // en el núcleo, que es lo que lee como fuego y no como pintura.
            //
            // Sin la textura (build sin `GlowTexture`) se cae al quad de color de
            // antes: fea pero visible, nunca invisible.
            let aura = |size: Vec2| -> Sprite {
                match &glow {
                    Some(g) => Sprite {
                        image: g.0.clone(),
                        custom_size: Some(size),
                        color: Color::NONE,
                        ..default()
                    },
                    None => Sprite::from_color(Color::NONE, size),
                }
            };
            // Externa: el derrame naranja ancho — la "sombra de color" que separa
            // al personaje del fondo y hace que el aura ocupe aire.
            p.spawn((
                crate::sim::PlayerAura,
                crate::sim::UnlitSprite,
                aura(Vec2::new(3.4, 4.2)),
                Transform::from_xyz(0.0, 0.15, -0.05),
                Visibility::Hidden,
            ));
            // Media: el cuerpo ámbar de la llama.
            p.spawn((
                crate::sim::PlayerAuraMid,
                crate::sim::UnlitSprite,
                aura(Vec2::new(2.1, 2.9)),
                Transform::from_xyz(0.0, 0.12, -0.04),
                Visibility::Hidden,
            ));
            // Núcleo: el punto caliente pegado al cuerpo.
            p.spawn((
                crate::sim::PlayerAuraCore,
                crate::sim::UnlitSprite,
                aura(Vec2::new(1.25, 1.95)),
                Transform::from_xyz(0.0, 0.08, -0.03),
                Visibility::Hidden,
            ));
        });
}

/// The surface region + world-context a body walks in (Earth = overworld,
/// Moon = Luna, everything else = its planet patch). Single source of truth so
/// landing paths never disagree on where "the ground" is.
pub fn surface_region_for(body: CelestialBody) -> (RegionId, WorldContext) {
    match body {
        CelestialBody::Earth => (RegionId::Overworld { gx: 0, gz: 0 }, WorldContext::Overworld),
        CelestialBody::Moon => (RegionId::Luna, WorldContext::Luna),
        other => (RegionId::Planet(other), WorldContext::Planet),
    }
}

/// Teletransporte DELIBERADO desde el menú (creativo/dev): deja al caminante en
/// la superficie de un cuerpo.
///
/// Es el único salto de coordenadas que queda en el juego, y es explícito: el
/// vuelo normal (`creative_fly`) nunca teletransporta. Mueve también el ORIGEN
/// del marco (`UniverseFrame`), porque las coordenadas del jugador se leen
/// siempre respecto del centro del cuerpo de referencia (`universe.rs`).
pub fn teleport_to_body(
    body: CelestialBody,
    frame: &mut crate::universe::UniverseFrame,
    region: &mut RegionRuntime,
    ctx: &mut WorldContextRuntime,
    planet: &mut crate::world2d::ActivePlanet,
    sim: &mut SimState,
    visual: &mut PlayerVisual,
) {
    let (target_region, target_ctx) = surface_region_for(body);
    frame.ref_body = body;
    region.active = target_region;
    ctx.active = target_ctx;
    if !matches!(body, CelestialBody::Earth | CelestialBody::Moon) {
        planet.body = Some(body);
    }
    let near_x = if body == CelestialBody::Earth {
        EARTH_PAD_S + 4.0
    } else {
        0.0
    };
    let (sx, sz) = walkable_spawn(body, near_x, 6.0);
    sim.body.x = sx;
    sim.body.z = sz;
    sim.body.y = 0.0;
    sim.body.vx = 0.0;
    sim.body.vz = 0.0;
    sim.body.vy = 0.0;
    visual.from_x = sx;
    visual.to_x = sx;
    visual.from_z = sz;
    visual.to_z = sz;
    visual.from_y = 0.0;
    visual.to_y = 0.0;
}
