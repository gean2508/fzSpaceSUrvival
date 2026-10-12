//! El ítem equipado, EN LA MANO del personaje (biblia: la hotbar no es sólo un
//! menú — lo que seleccionás se ve).
//!
//! Un único hijo del jugador ([`HeldItemVisual`]) lleva el sprite del ítem
//! seleccionado en la hotbar. Se coloca según hacia dónde mira el personaje
//! (delante en down/left/right, detrás en up) y, si el ítem emite luz (antorcha,
//! linterna), se le cuelga un [`Light2d`] real: ilumina el mundo, agujerea el
//! velo de noche y tiñe los sprites cercanos como cualquier otra antorcha.
//!
//! El visual se re-crea (despawn + spawn) sólo cuando cambia el ítem equipado:
//! así el `Light2d` y sus quads de halo nacen y mueren juntos (quitar el
//! componente dejaría los quads huérfanos encendidos).

use bevy::prelude::*;

use crate::art2d::ArtLibrary;
use crate::hud::item_icons::ItemIconCache;
use crate::input::EquippedItem;
use crate::light2d::Light2d;
use crate::sim::{Player, PlayerVisual};
use crate::sky2d::PlayerLanternControl;

/// Tamaño del ítem en la mano (metros). El personaje mide 1.9 m.
const HELD_SIZE: f32 = 0.62;

pub struct HeldItemPlugin;

impl Plugin for HeldItemPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<HeldItemVisualState>().add_systems(
            Update,
            (
                sync_held_item_visual,
                place_held_item_in_hand.after(sync_held_item_visual),
                sync_light_item_equipped.before(sync_held_item_visual),
            ),
        );
    }
}

/// Sprite del ítem que el personaje sostiene (hijo del jugador).
#[derive(Component)]
pub struct HeldItemVisual;

/// Qué ítem está representado ahora mismo — evita re-crear el visual por frame.
#[derive(Resource, Default)]
struct HeldItemVisualState {
    item_id: String,
    /// El visual existe; si el jugador se despawnea (cambio de escena) hay que
    /// volver a crearlo aunque el ítem no haya cambiado.
    spawned: bool,
    /// Estado del interruptor LAMP con el que se construyó el visual: apagar la
    /// antorcha re-crea la entidad SIN `Light2d` (quitar el componente dejaría
    /// los quads de halo huérfanos, encendidos para siempre).
    lit: bool,
}

/// Luz que emite un ítem al llevarlo en la mano. `None` = no ilumina.
///
/// Es la tabla que hace que "ponerse la antorcha" sea una mecánica y no un
/// icono: mismo `Light2d` que usan las antorchas colocadas en el mundo.
pub fn item_light(item_id: &str) -> Option<Light2d> {
    match item_id {
        // Las antorchas salen del MISMO catálogo que las colocables
        // (`torches::TORCH_VARIANTS`), no de una tabla paralela: si el color de
        // la azul en la mano no fuera el de la azul clavada en la pared, el
        // jugador vería cambiar la luz al soltarla. Una sola fuente de verdad.
        //
        // Los radios son en metros y el pozo real mide `radius * LIGHT_REACH`
        // (×1.5), así que el 7.0 de antes daba 18 m de charco: a zoom de
        // caminata ocupaba la pantalla ENTERA y no quedaba noche contra la cual
        // notar la antorcha.
        id if crate::torches::torch_variant(id).is_some() => {
            let v = crate::torches::torch_variant(id)?;
            Some(Light2d {
                color: v.color,
                radius: v.radius,
                intensity: 1.15,
                flicker: v.flicker,
                ..default()
            })
        }
        // La linterna es la luz MÁS generosa del juego (por eso se craftea), pero
        // "generosa" son ~7 m de pozo, no media pantalla: su gracia es que ves
        // dónde pisás, no que desaparezca la noche.
        "lantern" => Some(Light2d {
            color: Color::srgb(1.0, 0.86, 0.55),
            radius: 4.6,
            intensity: 1.15,
            flicker: 0.25,
            ..default()
        }),
        "potion_blue" | "gem_blue" => Some(Light2d {
            color: Color::srgb(0.45, 0.78, 1.0),
            radius: 4.5,
            intensity: 0.85,
            flicker: 0.15,
            ..default()
        }),
        _ => None,
    }
}

/// ¿El ítem se muestra en la mano? "hand" (puño) y los materiales sueltos
/// (madera, piedra…) no: sólo herramientas, armas, luces y colocables.
fn shows_in_hand(item_id: &str) -> bool {
    if item_id.is_empty() || item_id == "hand" {
        return false;
    }
    true
}

/// Cualquier ítem que ilumine cuenta como "linterna equipada" para el velo de
/// noche, y se enciende solo al equiparlo (el botón LAMP sigue pudiendo
/// apagarlo).
fn sync_light_item_equipped(
    equipped: Res<EquippedItem>,
    mut ctrl: ResMut<PlayerLanternControl>,
) {
    if !equipped.is_changed() {
        return;
    }
    let emits = item_light(&equipped.item_id).is_some();
    ctrl.equipped = emits;
    // `lit` se apaga al guardar la luz. Antes sólo se encendía: equipar una
    // antorcha una vez y volver al puño dejaba el flag en `true` para siempre, y
    // el HUD mostraba el botón LAMP encendido sin nada en la mano.
    ctrl.lit = emits;
}

fn sync_held_item_visual(
    mut commands: Commands,
    equipped: Res<EquippedItem>,
    art: Res<ArtLibrary>,
    mut icons: ResMut<ItemIconCache>,
    mut images: ResMut<Assets<Image>>,
    mut state: ResMut<HeldItemVisualState>,
    lantern: Res<PlayerLanternControl>,
    player: Query<Entity, With<Player>>,
    existing: Query<Entity, With<HeldItemVisual>>,
) {
    let Ok(player_e) = player.get_single() else {
        state.spawned = false;
        return;
    };
    let want = if shows_in_hand(&equipped.item_id) {
        equipped.item_id.as_str()
    } else {
        ""
    };
    if state.spawned && state.item_id == want && state.lit == lantern.lit && !existing.is_empty() {
        return;
    }
    for e in &existing {
        commands.entity(e).despawn_recursive();
    }
    state.item_id = want.to_string();
    state.spawned = true;
    state.lit = lantern.lit;
    if want.is_empty() {
        return;
    }

    // Arte del pack primero (`assets/art/items/<id>.png`, ya transparente); si
    // no hay, el icono procedural SIN el cuadrado de fondo.
    let size = Vec2::splat(HELD_SIZE);
    let mut sprite = art
        .world_sprite(&format!("items/{want}"), size)
        .map(|(s, _)| s)
        .unwrap_or_else(|| icons.hand_icon(want, &mut images).sprite(size));
    sprite.custom_size = Some(size);

    let mut entity = commands.spawn((
        HeldItemVisual,
        sprite,
        Transform::from_xyz(0.0, 0.0, 0.02),
        Name::new(format!("HeldItem_{want}")),
    ));
    if let Some(light) = item_light(want) {
        entity.insert(light);
    }
    let held = entity.id();
    commands.entity(player_e).add_child(held);
}

/// Coloca el ítem en la mano correcta según el rumbo: a la derecha mirando
/// down/right, a la izquierda mirando left (con el sprite espejado) y DETRÁS
/// del personaje mirando up — que es donde queda la mano desde esa cámara.
fn place_held_item_in_hand(
    player: Query<&PlayerVisual, With<Player>>,
    mut held: Query<(&mut Transform, &mut Sprite), With<HeldItemVisual>>,
) {
    let Ok(visual) = player.get_single() else {
        return;
    };
    let (dx, dz) = (visual.to_rot.cos(), visual.to_rot.sin());
    // Mismo criterio de cuadrante que `sim::pose_direction`.
    let (offset, flip, front) = if dz.abs() >= dx.abs() {
        if dz > 0.0 {
            (Vec2::new(0.34, -0.30), false, true) // de frente: mano derecha
        } else {
            (Vec2::new(-0.34, -0.30), true, false) // de espaldas: detrás
        }
    } else if dx < 0.0 {
        (Vec2::new(-0.42, -0.26), true, true)
    } else {
        (Vec2::new(0.42, -0.26), false, true)
    };
    for (mut tf, mut sprite) in &mut held {
        tf.translation.x = offset.x;
        tf.translation.y = offset.y;
        tf.translation.z = if front { 0.02 } else { -0.02 };
        sprite.flip_x = flip;
    }
}
