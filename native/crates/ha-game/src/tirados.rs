//! **Ítems tirados en el suelo** — el botín a lo Minecraft.
//!
//! Lo que suelta un animal al morir, el huevo que pone la gallina: no va directo a
//! la mochila, cae al piso, rebota un poco y se junta **pasando por encima**
//! (el imán de Minecraft, a [`RADIO_IMAN`]). Si nadie lo junta desaparece a los
//! [`VIDA_SECS`], como allá: el suelo no se llena de basura.
//!
//! Cualquier sistema tira algo con [`tirar`] (cola en un recurso, así no necesita
//! las texturas de los íconos); [`materializar`] lo dibuja con el ícono del ítem.
//! No se persiste: un ítem en el piso es un estado de minutos.

use bevy::prelude::*;

use crate::hud::inventory_ui::{add_item_to_slots_rest, flush_slots_to_inventory, InventoryUiState};
use crate::hud::item_icons::ItemIconCache;
use crate::inventory_persist::{InventoryPersistState, PlayerInventory};
use crate::sim::Player;
use crate::world2d::render_xy;

/// A qué distancia el ítem vuela hacia el jugador (m).
pub const RADIO_IMAN: f32 = 1.6;
/// A qué distancia se junta.
const RADIO_JUNTAR: f32 = 0.55;
/// Cuánto vive en el piso (s). Minecraft: 5 min.
pub const VIDA_SECS: f32 = 300.0;
/// Recién tirado no se junta enseguida (se ve caer).
const DEMORA_SECS: f32 = 0.5;
const LADO: f32 = 0.55;

#[derive(Component)]
pub struct Tirado {
    pub item: String,
    pub qty: u32,
    x: f32,
    z: f32,
    t: f32,
}

#[derive(Resource, Default)]
pub struct Tirados(Vec<(f32, f32, String, u32)>);

/// Tira `qty` de `item` en el suelo `(x, z)`.
pub fn tirar(cola: &mut Tirados, x: f32, z: f32, item: &str, qty: u32) {
    if qty > 0 {
        cola.0.push((x, z, item.to_string(), qty));
    }
}

pub struct TiradosPlugin;

impl Plugin for TiradosPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Tirados>().add_systems(
            Update,
            (materializar, juntar)
                .chain()
                .run_if(crate::game_phase::world_active),
        );
    }
}

fn materializar(
    mut commands: Commands,
    mut cola: ResMut<Tirados>,
    mut icons: ResMut<ItemIconCache>,
    mut images: ResMut<Assets<Image>>,
) {
    for (i, (x, z, item, qty)) in cola.0.drain(..).enumerate() {
        // Desparramo determinista: varias piezas no caen una encima de otra.
        let a = (i as f32 * 2.39996) + x * 1.7;
        let (x, z) = (x + a.cos() * 0.35, z + a.sin() * 0.35);
        let p = render_xy(x, z);
        commands.spawn((
            icons.icon(&item, &mut images).sprite(Vec2::splat(LADO)),
            Transform::from_xyz(p.x, p.y, crate::layers::ysort(z)),
            Tirado { item, qty, x, z, t: 0.0 },
            Name::new("ItemTirado"),
        ));
    }
}

#[allow(clippy::too_many_arguments)]
fn juntar(
    mut commands: Commands,
    time: Res<Time>,
    player: Query<&Transform, (With<Player>, Without<Tirado>)>,
    mut tirados: Query<(Entity, &mut Tirado, &mut Transform)>,
    mut inv_ui: ResMut<InventoryUiState>,
    mut inv: ResMut<PlayerInventory>,
    mut persist: ResMut<InventoryPersistState>,
) {
    let dt = time.delta_secs();
    let jug = player.get_single().ok().map(|t| Vec2::new(t.translation.x, -t.translation.y));
    let mut junto = false;
    for (e, mut it, mut tf) in &mut tirados {
        it.t += dt;
        if it.t > VIDA_SECS {
            commands.entity(e).despawn_recursive();
            continue;
        }
        if let Some(j) = jug.filter(|_| it.t > DEMORA_SECS) {
            let d = j - Vec2::new(it.x, it.z);
            let dist = d.length();
            if dist < RADIO_JUNTAR {
                let resto = add_item_to_slots_rest(&mut inv_ui, &it.item, it.qty);
                if resto < it.qty {
                    junto = true;
                    crate::granja::globo(
                        &mut commands,
                        it.x,
                        it.z,
                        format!("+{} {}", it.qty - resto, crate::granja::nombre_item(&it.item)),
                    );
                }
                if resto == 0 {
                    commands.entity(e).despawn_recursive();
                    continue;
                }
                // Mochila llena: lo que no entró se queda en el piso.
                it.qty = resto;
            } else if dist < RADIO_IMAN {
                let paso = (6.0 * dt).min(dist);
                let dir = d / dist.max(1e-4);
                it.x += dir.x * paso;
                it.z += dir.y * paso;
            }
        }
        // Rebote suave (flota), y cae un poco al aparecer.
        let p = render_xy(it.x, it.z);
        let caida = (1.0 - (it.t / 0.35).min(1.0)) * 0.6;
        tf.translation.x = p.x;
        tf.translation.y = p.y + 0.12 + (it.t * 3.0).sin() * 0.06 + caida;
        tf.translation.z = crate::layers::ysort(it.z);
    }
    if junto {
        flush_slots_to_inventory(&inv_ui, &mut inv);
        persist.mark_immediate();
    }
}
