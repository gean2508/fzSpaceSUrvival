//! **Trenes bala entre ciudades** (cliente). La red y el horario son de `ha_shared::trenes` (función pura del reloj);
//! acá sólo se dibuja lo que está cerca del jugador: el balasto con sus dos rieles, las plataformas de las estaciones y
//! el tren (cabeza, tres coches y cola del pack, `vehicle/tren_exterior_blanco_*`).
//!
//! Los dibujos son vistas laterales, así que NO se rotan: cada coche se coloca upright sobre el riel, separado lo justo
//! para que en una diagonal no se pisen ni queden huecos. Cupo: sólo las líneas a menos de 450 tiles, 5 sprites de
//! tren por línea. El tren pasa por encima de quien esté en la vía y lo lastima (sin matarlo: deja 1 de vida).

use bevy::prelude::*;
use ha_shared::trenes as red;
use std::collections::HashMap;

use crate::game_phase::GamePhase;
use crate::scene::WorldContextRuntime;
use crate::sim::{Player, SimState};
use crate::world2d::{active_surface_body, render_xy, sort_z, ActivePlanet, TileWorld};

const RADIO_VER: f32 = 450.0;
const RADIO_OLVIDAR: f32 = 650.0;
/// Alto de un coche en tiles.
const ALTO: f32 = 4.0;
const CABEZA_D: &str = "vehicle/tren_exterior_blanco_derecha";
const CABEZA_I: &str = "vehicle/tren_exterior_blanco_izquierda";
const MEDIO: &str = "vehicle/tren_exterior_blanco_medio";
/// (clave, largo en tiles) de los cinco coches, de atrás hacia adelante.
const COCHES: [(&str, f32); 5] = [(CABEZA_I, 7.0), (MEDIO, 9.0), (MEDIO, 9.0), (MEDIO, 9.0), (CABEZA_D, 7.0)];

#[derive(Component)]
struct LineaViva(red::Linea);

#[derive(Component)]
struct Coche(usize);

#[derive(Resource, Default)]
struct Vivas(HashMap<u32, Entity>);

pub struct TrenesPlugin;

impl Plugin for TrenesPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Vivas>()
            .add_systems(Update, (aparecer, mover, atropellar).chain().run_if(in_state(GamePhase::Playing)))
            .add_systems(OnExit(GamePhase::Playing), limpiar);
    }
}

fn limpiar(mut commands: Commands, mut vivas: ResMut<Vivas>) {
    for (_, e) in vivas.0.drain() {
        commands.entity(e).despawn_recursive();
    }
}

/// Segundos del reloj de pared del día: todos los clientes ven el tren en el mismo lugar.
fn reloj() -> f32 {
    let s = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs_f64()).unwrap_or(0.0);
    (s % 86_400.0) as f32
}

/// Largo total del tren: la suma de sus coches.
pub fn largo_del_tren() -> f32 {
    COCHES.iter().map(|c| c.1).sum()
}

fn aparecer(
    mut commands: Commands,
    art: Res<crate::art2d::ArtLibrary>,
    ctx: Res<WorldContextRuntime>,
    planet: Res<ActivePlanet>,
    tiles: Res<TileWorld>,
    player: Query<&SimState, With<Player>>,
    mut vivas: ResMut<Vivas>,
    mut cd: Local<f32>,
    time: Res<Time>,
) {
    *cd -= time.delta_secs();
    if *cd > 0.0 {
        return;
    }
    *cd = 1.0;
    let Ok(sim) = player.get_single() else { return };
    let (px, pz) = (sim.body.x, sim.body.z);
    let body = active_surface_body(&ctx, &planet);
    // Lo lejano se olvida.
    let lejanas: Vec<u32> = vivas
        .0
        .keys()
        .copied()
        .filter(|id| red::red(body).iter().find(|l| l.id == *id).map_or(true, |l| l.distancia_a(px, pz) > RADIO_OLVIDAR))
        .collect();
    for id in lejanas {
        if let Some(e) = vivas.0.remove(&id) {
            commands.entity(e).despawn_recursive();
        }
    }
    if tiles.z != 0 {
        return;
    }
    for l in red::lineas_cerca(body, px, pz, RADIO_VER) {
        if vivas.0.contains_key(&l.id) {
            continue;
        }
        let e = crear(&mut commands, &art, l);
        vivas.0.insert(l.id, e);
    }
}

fn crear(commands: &mut Commands, art: &crate::art2d::ArtLibrary, l: red::Linea) -> Entity {
    let (dx, dz) = l.dir();
    let medio = render_xy((l.a.0 + l.b.0) * 0.5, (l.a.1 + l.b.1) * 0.5);
    let ang = (-dz).atan2(dx);
    let z_vía = crate::layers::SOBRE_EL_PISO;
    commands
        .spawn((LineaViva(l), Transform::default(), Visibility::Inherited, Name::new("LineaDeTren")))
        .with_children(|c| {
            // Balasto y rieles (rotados: son lisos, no tienen "lado").
            c.spawn((
                Sprite::from_color(Color::srgb(0.30, 0.28, 0.26), Vec2::new(l.largo(), 3.2)),
                Transform::from_xyz(medio.x, medio.y, z_vía).with_rotation(Quat::from_rotation_z(ang)),
            ));
            for lado in [-0.9f32, 0.9] {
                let off = Vec2::new(-ang.sin(), ang.cos()) * lado;
                c.spawn((
                    Sprite::from_color(Color::srgb(0.78, 0.78, 0.82), Vec2::new(l.largo(), 0.18)),
                    Transform::from_xyz(medio.x + off.x, medio.y + off.y, z_vía + 0.001).with_rotation(Quat::from_rotation_z(ang)),
                ));
            }
            // Plataformas de las dos estaciones.
            for p in [l.a, l.b] {
                let r = render_xy(p.0, p.1);
                c.spawn((
                    Sprite::from_color(Color::srgb(0.55, 0.55, 0.58), Vec2::new(46.0, 9.0)),
                    Transform::from_xyz(r.x, r.y, z_vía + 0.002),
                ));
            }
            // El tren: cinco coches (las posiciones las pone `mover`).
            for (i, (clave, largo)) in COCHES.iter().enumerate() {
                let sprite = art
                    .world_sprite(clave, Vec2::new(*largo, ALTO))
                    .map(|(s, _)| s)
                    .unwrap_or_else(|| Sprite::from_color(Color::srgb(0.9, 0.9, 0.95), Vec2::new(*largo, ALTO)));
                c.spawn((sprite, Transform::default(), Coche(i)));
            }
        })
        .id()
}

fn mover(
    lineas: Query<(&LineaViva, &Children)>,
    mut coches: Query<(&Coche, &mut Transform)>,
) {
    let t = reloj();
    for (viva, hijos) in &lineas {
        let l = &viva.0;
        let ((cx, cz), _) = red::posicion(l, t);
        let (dx, dz) = l.dir();
        // Separación entre centros de coches contiguos, medida en la caja de cada uno (largo en x, alto en z).
        let sep = |a: f32, b: f32| (a + b) * 0.5 * dx.abs() + ALTO * dz.abs();
        let total: f32 = (0..COCHES.len() - 1).map(|i| sep(COCHES[i].1, COCHES[i + 1].1)).sum();
        let mut acum = -total * 0.5;
        let mut offs = [0.0f32; 5];
        for i in 0..COCHES.len() {
            offs[i] = acum;
            if i + 1 < COCHES.len() {
                acum += sep(COCHES[i].1, COCHES[i + 1].1);
            }
        }
        for &h in hijos.iter() {
            let Ok((Coche(i), mut tf)) = coches.get_mut(h) else { continue };
            let (x, z) = (cx + dx * offs[*i] * 1.0, cz + dz * offs[*i]);
            let r = render_xy(x, z);
            tf.translation = Vec3::new(r.x, r.y, sort_z(z) + 0.3);
        }
    }
}

/// El tren se lleva por delante a quien esté en la vía: lo deja en 1 de vida.
fn atropellar(
    time: Res<Time>,
    lineas: Query<&LineaViva>,
    tiles: Res<TileWorld>,
    mut player: Query<&mut SimState, With<Player>>,
) {
    if tiles.z != 0 {
        return;
    }
    let Ok(mut sim) = player.get_single_mut() else { return };
    let t = reloj();
    let largo = largo_del_tren();
    for viva in &lineas {
        if red::bajo_el_tren(&viva.0, t, largo, sim.body.x, sim.body.z) {
            sim.hp = (sim.hp - 90.0 * time.delta_secs()).max(1.0);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn el_tren_mide_la_suma_de_sus_coches() {
        assert_eq!(largo_del_tren(), 41.0);
    }
}
