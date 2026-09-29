//! **Ver la colisión** — `HA_VER_COLISION=1`.
//!
//! Pinta en rojo translúcido todo lo que FRENA alrededor del jugador (tiles,
//! objetos colocados y huellas del decor), muestreado cada 1/8 de metro, y en
//! verde la caja del jugador. Es la única forma de comparar al milímetro lo que
//! se ve contra lo que choca: una foto sola no dice dónde está la pared de
//! verdad. Una textura de 160×96 que se rehace cuatro veces por segundo: no es
//! para jugar, es para mirar.

use bevy::prelude::*;
use bevy::render::render_asset::RenderAssetUsages;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};

use crate::sim::{Player, SimState};
use crate::world2d::{active_surface_body, ActivePlanet, TileWorld};

const PASO: f32 = 0.125;
const W: usize = 160; // 20 m
const H: usize = 96; // 12 m

#[derive(Component)]
struct Capa;

pub struct VerColisionPlugin;

impl Plugin for VerColisionPlugin {
    fn build(&self, app: &mut App) {
        if crate::flags::var("HA_VER_COLISION").is_err() {
            return;
        }
        app.add_systems(Update, pintar);
    }
}

#[allow(clippy::too_many_arguments)]
fn pintar(
    mut commands: Commands,
    time: Res<Time>,
    mut acc: Local<f32>,
    tiles: Res<TileWorld>,
    structures: Res<crate::structures::BuiltStructures>,
    walls: Res<crate::walls::BuiltWalls>,
    art: Res<crate::art2d::ArtLibrary>,
    authored: Res<crate::world2d::AuthoredDecor>,
    flora: Res<crate::harvest::FloraHarvest>,
    ctx: Res<crate::scene::WorldContextRuntime>,
    planet: Res<ActivePlanet>,
    player: Query<&SimState, With<Player>>,
    fachadas: Query<&crate::building::SolidoDeFachada>,
    mut images: ResMut<Assets<Image>>,
    mut capa: Query<(&mut Sprite, &mut Transform), With<Capa>>,
) {
    *acc += time.delta_secs();
    if *acc < 0.25 {
        return;
    }
    *acc = 0.0;
    let Ok(sim) = player.get_single() else { return };
    let body = active_surface_body(&ctx, &planet);
    let (cx, cz) = (sim.body.x.floor(), sim.body.z.floor());
    let (x0, z0) = (cx - (W as f32 * PASO) / 2.0, cz - (H as f32 * PASO) / 2.0);
    let r = ha_shared::topdown::PLAYER_RADIUS;
    let mut data = vec![0u8; W * H * 4];
    // Primero por CELDA (barato): sólo las celdas donde algo del decor toca se
    // subdividen al octavo de metro. Muestrear todo al octavo congelaba el juego.
    let celdas_x = (W as f32 * PASO) as i64;
    let celdas_z = (H as f32 * PASO) as i64;
    let mut decor_celda = std::collections::HashSet::new();
    for dz in 0..celdas_z {
        for dx in 0..celdas_x {
            let (tx, tz) = (x0 as i64 + dx, z0 as i64 + dz);
            let caja = [tx as f32 + 0.01, tz as f32 + 0.01, tx as f32 + 0.99, tz as f32 + 0.99];
            if crate::world2d::decor_hits_box(&art, Some(&authored), Some(&flora), body, tiles.z, caja) {
                decor_celda.insert((tx, tz));
            }
        }
    }
    for j in 0..H {
        for i in 0..W {
            let x = x0 + (i as f32 + 0.5) * PASO;
            let z = z0 + (j as f32 + 0.5) * PASO;
            let (tx, tz) = (x.floor() as i64, z.floor() as i64);
            let e = PASO * 0.5;
            let es_cerca = |tx: i64, tz: i64| {
                tiles.z == 0 && tiles.tile_at(body, tx, tz) == ha_shared::topdown::Tile::Fence
            };
            let caja = [x - PASO * 0.5, z - PASO * 0.5, x + PASO * 0.5, z + PASO * 0.5];
            let frena = (tiles.walk_blocked(body, tx, tz) && !es_cerca(tx, tz))
                || crate::sim::cerca_frena(&es_cerca, caja)
                || crate::building::fachada_frena(&fachadas, caja)
                || structures.blocks(body, tiles.z, tx, tz)
                || crate::walls::punto_bloqueado(&walls, body, tiles.z, x, z)
                || (decor_celda.contains(&(tx, tz))
                    && crate::world2d::decor_hits_box(
                        &art,
                        Some(&authored),
                        Some(&flora),
                        body,
                        tiles.z,
                        [x - e, z - e, x + e, z + e],
                    ));
            let jugador = (x - sim.body.x).abs() <= r && (z - sim.body.z).abs() <= r;
            let k = (j * W + i) * 4;
            let c: [u8; 4] = if jugador {
                [40, 240, 60, 150]
            } else if frena {
                [240, 30, 40, 110]
            } else {
                [0, 0, 0, 0]
            };
            data[k..k + 4].copy_from_slice(&c);
        }
    }
    let img = Image::new(
        Extent3d { width: W as u32, height: H as u32, depth_or_array_layers: 1 },
        TextureDimension::D2,
        data,
        TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::RENDER_WORLD | RenderAssetUsages::MAIN_WORLD,
    );
    let h = images.add(img);
    let tam = Vec2::new(W as f32 * PASO, H as f32 * PASO);
    // Centro de la textura en pantalla: x igual, y = −z.
    let centro = Vec3::new(x0 + tam.x / 2.0, -(z0 + tam.y / 2.0), crate::layers::GLOBO - 0.5);
    if let Ok((mut s, mut tf)) = capa.get_single_mut() {
        s.image = h;
        tf.translation = centro;
    } else {
        let mut s = Sprite::from_image(h);
        s.custom_size = Some(tam);
        commands.spawn((s, Transform::from_translation(centro), Capa, Name::new("VerColision")));
    }
}
