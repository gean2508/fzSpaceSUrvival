//! Local player sim @ 20 Hz — top-down 3/4 locomotion on the XZ ground plane.
//! `body.x`/`body.z` are the ground plane; render maps to screen `(x, -z)`.

use bevy::prelude::*;
use ha_shared::topdown::{step_topdown_movement, step_topdown_movement_ex};
use ha_shared::{collision_obstacles, generate_world, InputState, WorldContext};

use crate::game_time::configure_game_clock;
use crate::game_time::fixed_delta_secs;
use crate::input::{MovementIntent, TouchActions};
use crate::flight::Flight;
use crate::scene::WorldContextRuntime;
use crate::world2d::{active_surface_body, render_xy, sort_z, ActivePlanet, TileWorld};

/// Legacy anchor kept for compat.
pub const CAVE_INTERIOR_ORIGIN: Vec3 = Vec3::new(0.0, -22.0, 0.0);
pub const JETPACK_ITEM: &str = "jetpack";

#[derive(Component)]
pub struct Player;

/// Ground shadow shown ONLY while flying above visible terrain (creative
/// Climb/Descend — biblia §11-bis) to sell the illusion of altitude. Hidden
/// on foot (unchanged baseline look) and in Orbit (no ground in view to fall
/// on). Synced in `creative_fly.rs::sync_player_shadow`.
#[derive(Component)]
pub struct PlayerFlightShadow;

/// Saiyan Ki aura — a golden glow behind the character that flares while
/// charging (power-up) and stays lit while flying. Hidden on foot at rest.
/// Procedural pulse today; the user can texture logical slot `fx/aura_saiyan`.
/// Synced in `creative_fly.rs::sync_player_aura`.
#[derive(Component)]
pub struct PlayerAura;

#[derive(Resource, Clone)]
pub struct WorldCollision(pub Vec<ha_shared::types::Obstacle>);

pub struct SimPlugin;

impl Plugin for SimPlugin {
    fn build(&self, app: &mut App) {
        let world = generate_world(42);
        app.insert_resource(Time::<Fixed>::from_hz(ha_shared::TICK_RATE as f64))
            .insert_resource(WorldLayout(world.clone()))
            .insert_resource(WorldCollision(collision_obstacles(&world)))
            .add_systems(Startup, configure_game_clock)
            .add_systems(FixedUpdate, tick_sim)
            .add_systems(Update, (interpolate_player_visual, face_player_sprite));
    }
}

#[derive(Resource, Clone)]
pub struct WorldLayout(pub ha_shared::GeneratedWorld);

#[derive(Component)]
pub struct SimState {
    pub body: ha_shared::MovementBody,
    pub hp: f32,
    pub max_hp: f32,
    pub survival: ha_shared::SurvivalStats,
    pub prev_action: bool,
    pub prev_jump: bool,
}

#[derive(Component, Default)]
pub struct PlayerVisual {
    pub from_x: f32,
    pub from_z: f32,
    pub from_y: f32,
    pub from_rot: f32,
    pub to_x: f32,
    pub to_z: f32,
    pub to_y: f32,
    pub to_rot: f32,
    pub radius: f32,
}

fn tick_sim(
    time: Res<Time<Fixed>>,
    intent: Res<MovementIntent>,
    actions: Res<TouchActions>,
    flight: Res<Flight>,
    world_ctx: Res<WorldContextRuntime>,
    planet_ctx: Res<ActivePlanet>,
    tiles: Res<TileWorld>,
    art: Res<crate::art2d::ArtLibrary>,
    flora: Res<crate::harvest::FloraHarvest>,
    structures: Res<crate::structures::BuiltStructures>,
    walls: Res<crate::walls::BuiltWalls>,
    editor: Res<crate::ui_editor::UiEditorState>,
    mut query: Query<(&mut SimState, &mut PlayerVisual), With<Player>>,
) {
    if flight.blocks_walking() {
        return;
    }
    let dt = fixed_delta_secs(&time);
    let body_surface = active_surface_body(&world_ctx, &planet_ctx);

    for (mut sim, mut visual) in &mut query {
        visual.from_x = visual.to_x;
        visual.from_z = visual.to_z;
        visual.from_y = visual.to_y;
        visual.from_rot = visual.to_rot;

        sim.prev_action = actions.action;

        let input = InputState {
            seq: 0,
            move_x: intent.move_x,
            move_z: intent.move_z,
            aim_x: intent.aim_x,
            aim_z: intent.aim_z,
            abilities: actions.abilities,
            sprint: if intent.sprint { 1 } else { 0 },
            action: if actions.action { 1 } else { 0 },
        };

        match world_ctx.active {
            WorldContext::Space => {
                // In vacuum the rocket owns movement.
            }
            _ if editor.creative => {
                // Creative: free 8-dir move with no collision (fly over water/rock).
                let no_solid = |_tx: i64, _tz: i64| false;
                step_topdown_movement(&mut sim.body, &input, dt, &no_solid);
            }
            _ => {
                // Solid tiles PLUS decor collision: a tree blocks its own cell so
                // you can't walk through the trunk (keeps the 3/4 illusion), while
                // the canopy stays visual — you pass "behind" it. Per-asset the
                // user can flip this in the editor ("bloquea el paso"), so e.g.
                // rocks/boulders can be made solid too.
                let solid = |tx: i64, tz: i64| {
                    tiles.solid_at(body_surface, tx, tz)
                        || structures.blocks(body_surface, tiles.z, tx, tz)
                        || (crate::world2d::decor_blocks_walk(&art, body_surface, tx, tz)
                            && !flora.is_removed(
                                body_surface,
                                0,
                                0,
                                crate::world2d::pack_cell(tx, tz),
                            ))
                };
                // Paredes de BORDE (construcción LDoE): viven en la arista
                // entre celdas, así que van por fuera del grid de sólidos.
                let wall_hit = |x: f32, z: f32| {
                    ha_shared::build_grid::wall_capsule_hits(x, z, &|tx, tz, axis| {
                        walls.blocks(body_surface, tiles.z, tx, tz, axis)
                    })
                };
                step_topdown_movement_ex(&mut sim.body, &input, dt, &solid, &wall_hit);
            }
        }
        sim.body.y = 0.0;

        visual.to_x = sim.body.x;
        visual.to_z = sim.body.z;
        visual.to_y = 0.0;
        visual.to_rot = sim.body.rot;
        visual.radius = sim.body.radius;
    }
}

fn interpolate_player_visual(
    time: Res<Time<Fixed>>,
    flight: Res<Flight>,
    fly: Res<crate::creative_fly::CreativeFlight>,
    zoom: Res<crate::camera::CameraZoom>,
    mut query: Query<(&PlayerVisual, &mut Transform, &mut Visibility), With<Player>>,
) {
    // Volando: el personaje NUNCA cambia de sitio (marco único — `universe.rs`),
    // sólo deja de encoger. Se dibuja con TAMAÑO DE PANTALLA CONSTANTE, así que
    // a cualquier altura seguís viéndote: la sensación de subir la da el mundo
    // que se aleja (el zoom por altura), no el sprite.
    //
    // Que el personaje se volviera un punto sub-píxel al alejarse era la mitad
    // del bug "no veo nada y los controles no responden" — respondían, pero no
    // había forma de saber dónde estabas.
    if fly.flying() {
        let t = time.overstep_fraction() as f32;
        // 1 = igual de grande que pisando el suelo. En vista de mapa se achica
        // un poco (marcador, no protagonista) pero nunca por debajo de ~2/3.
        let screen_f =
            1.0 - 0.35 * crate::universe::smootherstep(200.0, 4_000.0, zoom.view_h);
        let s = (zoom.view_h / crate::universe::GROUND_VIEW_H * screen_f).max(1.0);
        for (visual, mut transform, mut vis) in &mut query {
            *vis = Visibility::Visible;
            let wx = visual.from_x.lerp(visual.to_x, t);
            let wz = visual.from_z.lerp(visual.to_z, t);
            let r = render_xy(wx, wz);
            // Siempre por delante del terreno y de los discos planetarios.
            transform.translation = Vec3::new(r.x, r.y, 900.0);
            transform.rotation = Quat::IDENTITY;
            transform.scale = Vec3::new(s, s, 1.0);
        }
        return;
    }
    let t = time.overstep_fraction() as f32;
    if flight.active && flight.phase != ha_shared::FlightPhase::Surface {
        for (_, _, mut vis) in &mut query {
            *vis = Visibility::Hidden;
        }
        return;
    }

    for (visual, mut transform, mut vis) in &mut query {
        *vis = Visibility::Visible;
        let x = visual.from_x.lerp(visual.to_x, t);
        let z = visual.from_z.lerp(visual.to_z, t);
        let r = render_xy(x, z);
        // Y-sort by the player's own base z, with a TINY epsilon (< one tile
        // step) so it draws above same-cell flat ground art but still sorts
        // fairly against trees/walls: a tree to the SOUTH (larger z) now draws
        // OVER the player — real 3/4 depth, canopies can hide you.
        transform.translation = Vec3::new(r.x, r.y, sort_z(z) + 0.0005);
        // Facing: flip horizontally when moving west (left half of the circle).
        let facing_left = visual.to_rot.abs() > std::f32::consts::FRAC_PI_2;
        transform.rotation = Quat::IDENTITY;
        transform.scale = Vec3::new(if facing_left { -1.0 } else { 1.0 }, 1.0, 1.0);
    }
}

/// Clave del slot de arte del jugador (coincide con `art/player/character.png`
/// hardcodeado en `spawn_player` — el editor escribe sus poses ahí mismo).
const PLAYER_ART_KEY: &str = "player/character";

/// Última pose aplicada (evita reconstruir el atlas/rect todos los frames si
/// no cambió). Vacía en el spawn.
#[derive(Component, Default)]
pub struct CurrentPose(String);

fn pose_direction(to_rot: f32) -> &'static str {
    let (dx, dz) = (to_rot.cos(), to_rot.sin());
    if dz.abs() >= dx.abs() {
        if dz > 0.0 { "down" } else { "up" }
    } else if dx < 0.0 {
        "left"
    } else {
        "right"
    }
}

/// Direcciones candidatas para el rumbo, de la más específica a la más general:
/// la **diagonal** (octante) y después la **cardinal** (cuadrante). Si el atlas
/// trae `walk_down_left` se usa; si sólo trae `walk_left`, cae ahí sola. Así un
/// pack de 8 direcciones se aprovecha entero sin romper los de 4.
fn pose_directions(to_rot: f32) -> [&'static str; 2] {
    let (dx, dz) = (to_rot.cos(), to_rot.sin());
    let card = pose_direction(to_rot);
    // Octante: diagonal sólo si ambas componentes son comparables (±22.5°..67.5°).
    let (ax, az) = (dx.abs(), dz.abs());
    let diagonal = ax > 0.383 && az > 0.383;
    if !diagonal {
        return [card, card];
    }
    let vert = if dz > 0.0 { "down" } else { "up" };
    let horiz = if dx < 0.0 { "left" } else { "right" };
    [
        match (vert, horiz) {
            ("down", "left") => "down_left",
            ("down", _) => "down_right",
            (_, "left") => "up_left",
            _ => "up_right",
        },
        card,
    ]
}

/// Orient the character sprite to the facing direction. Si el slot
/// `player/character` trae **poses** del editor (idle/walk × 4 direcciones),
/// las usa (con mirror left↔right si sólo se sliceó un lado). Si no hay poses
/// definidas, cae al atlas LimeZu de 3 frames hardcodeado (row-0: 0=down,
/// 1=up, 2=side+flip) — comportamiento previo intacto.
fn face_player_sprite(
    art: Res<crate::art2d::ArtLibrary>,
    mut commands: Commands,
    mut query: Query<(Entity, &PlayerVisual, &mut Sprite, &mut CurrentPose), With<Player>>,
) {
    let use_poses = art.has_poses(PLAYER_ART_KEY);
    for (entity, visual, mut sprite, mut current) in &mut query {
        if !use_poses {
            if sprite.texture_atlas.is_none() {
                continue;
            }
            let (dx, dz) = (visual.to_rot.cos(), visual.to_rot.sin());
            let (index, flip) = if dz.abs() >= dx.abs() {
                if dz > 0.0 { (0, false) } else { (1, false) }
            } else {
                (2, dx < 0.0)
            };
            if let Some(atlas) = &mut sprite.texture_atlas {
                atlas.index = index;
            }
            sprite.flip_x = flip;
            continue;
        }

        let moving = (visual.to_x - visual.from_x).abs() > 1e-4 || (visual.to_z - visual.from_z).abs() > 1e-4;
        let verb = if moving { "walk" } else { "idle" };
        let [diag, card] = pose_directions(visual.to_rot);

        // Orden de preferencia: diagonal → cardinal → mismo verbo espejado →
        // idle en esa dirección → idle_down. Lo que el usuario sliceó manda; lo
        // que no sliceó nunca deja al personaje sin sprite.
        let mut flip = false;
        let mut wanted = format!("{verb}_{diag}");
        if !art.has_pose(PLAYER_ART_KEY, &wanted) {
            wanted = format!("{verb}_{card}");
        }
        if !art.has_pose(PLAYER_ART_KEY, &wanted) {
            // Sólo se sliceó un lado (ej. "right") — espejar para el opuesto.
            let mirror = match card {
                "left" => Some("right"),
                "right" => Some("left"),
                _ => None,
            };
            if let Some(m) = mirror {
                let alt = format!("{verb}_{m}");
                if art.has_pose(PLAYER_ART_KEY, &alt) {
                    wanted = alt;
                    flip = true;
                }
            }
        }
        if !art.has_pose(PLAYER_ART_KEY, &wanted) {
            wanted = format!("idle_{card}");
        }
        if !art.has_pose(PLAYER_ART_KEY, &wanted) {
            wanted = "idle_down".to_string();
        }

        let state_key = format!("{wanted}#{flip}");
        if current.0 == state_key {
            continue;
        }
        current.0 = state_key;
        sprite.flip_x = flip;

        // Personaje armado con VARIAS hojas (un PNG por dirección/estado): la
        // pose trae su propia imagen. Con una sola hoja esto devuelve la misma
        // y no cambia nada.
        if let Some(img) = art.pose_image(PLAYER_ART_KEY, &wanted) {
            if sprite.image != img {
                sprite.image = img;
                sprite.texture_atlas = None; // el layout se rehace para la hoja nueva
            }
        }
        if let Some(([x, y, w, h], anim)) = art.pose(PLAYER_ART_KEY, &wanted) {
            match anim {
                Some(anim) => {
                    sprite.rect = None;
                    commands.entity(entity).insert(anim);
                }
                None => {
                    commands.entity(entity).remove::<crate::art2d::ArtAnimated>();
                    sprite.texture_atlas = None;
                    sprite.rect = Some(Rect::from_corners(
                        Vec2::new(x as f32, y as f32),
                        Vec2::new((x + w) as f32, (y + h) as f32),
                    ));
                }
            }
        }
    }
}

/// Compat shim: top-down ground is flat (y=0).
pub fn earth_surface_y(_tx: f32, _tz: f32) -> f32 {
    0.0
}
