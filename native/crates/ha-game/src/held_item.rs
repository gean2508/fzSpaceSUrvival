//! El ítem equipado, EN LA MANO del personaje (biblia: la hotbar no es sólo un
//! menú — lo que seleccionás se ve).
//!
//! Desde el 3 oct 2026 el objeto no flota al costado: lo AGARRA la mano que dibuja la hoja
//! (`manos.rs`: punto de agarre medido por objeto, mano medida por cuadro, y los dedos encima).
//! Acá sólo se decide QUÉ lleva el jugador (`Sostiene`) y, si el ítem emite luz (antorcha,
//! linterna), qué `Light2d` real se le cuelga: ilumina el mundo, agujerea el velo de noche y tiñe
//! los sprites cercanos como cualquier otra antorcha.
//!
//! **La ropa NO se lleva en la mano**: una prenda se viste (`equipo.rs`) y reemplaza la ropa que
//! trae el personaje; seleccionarla en la hotbar no dibuja nada en la mano.
//!
//! El objeto se re-crea (despawn + spawn) sólo cuando cambia el ítem o el interruptor de la luz:
//! así el `Light2d` y sus quads de halo nacen y mueren juntos (quitar el componente dejaría
//! los quads huérfanos encendidos).

use bevy::prelude::*;

use crate::input::EquippedItem;
use crate::light2d::Light2d;
use crate::manos::Sostiene;
use crate::sim::Player;
use crate::sky2d::PlayerLanternControl;

pub struct HeldItemPlugin;

impl Plugin for HeldItemPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            Update,
            (
                sync_light_item_equipped.before(sync_held_item),
                sync_held_item,
                herramienta_de_aldeanos,
            ),
        );
    }
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

/// ¿El ítem se muestra en la mano? "hand" (puño), los materiales sueltos (madera, piedra…) y la ROPA no:
/// sólo herramientas, armas, luces y colocables. La ropa se viste, no se empuña.
fn shows_in_hand(item_id: &str) -> bool {
    if item_id.is_empty() || item_id == "hand" {
        return false;
    }
    if ha_shared::armadura::prenda(item_id).is_some() {
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

/// Lo que sostiene el jugador: el ítem seleccionado (si se empuña) y la piel de sus manos.
fn sync_held_item(
    mut commands: Commands,
    equipped: Res<EquippedItem>,
    lantern: Res<PlayerLanternControl>,
    selected: Res<crate::game_phase::SelectedCharacter>,
    player: Query<(Entity, Option<&Sostiene>), With<Player>>,
) {
    let Ok((e, actual)) = player.get_single() else { return };
    let want = if shows_in_hand(&equipped.item_id) { equipped.item_id.as_str() } else { "" };
    let nuevo = Sostiene {
        item: want.to_string(),
        piel: selected.look.map(|l| l.body),
        // Apagar la antorcha re-crea el objeto SIN `Light2d`.
        luz: lantern.lit,
    };
    if actual != Some(&nuevo) {
        commands.entity(e).insert(nuevo);
    }
}

/// Qué herramienta lleva cada aldeano según su oficio: el granjero el balde, el minero el pico, el constructor el
/// martillo. Los niños y los sin oficio van con las manos libres.
fn herramienta_de_aldeanos(
    mut commands: Commands,
    cg: Res<crate::chargen::Chargen>,
    aldeanos: Query<(Entity, &crate::society::Npc, Option<&Sostiene>)>,
) {
    for (e, n, actual) in &aldeanos {
        let item = crate::society::herramienta_de(n.role);
        let piel = crate::society::skin_n(n).and_then(|i| cg.index.npc_looks.get(i)).map(|l| l.body);
        // Sin lámina de manos (niños) no hay agarre: mejor sin objeto que flotando.
        let nuevo = Sostiene { item: if piel.is_some() { item.to_string() } else { String::new() }, piel, luz: true };
        if actual != Some(&nuevo) {
            commands.entity(e).insert(nuevo);
        }
    }
}
