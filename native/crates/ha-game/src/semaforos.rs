//! **Semáforos con fase** (5 oct 2026): cada poste de esquina apaga las lámparas que no corresponden a la luz de SU eje
//! (`ha_shared::semaforos::luz`). El poste del pack trae las tres prendidas; encima se le cuelgan tres tapas oscuras que
//! se muestran o no según la fase. Los coches frenan ante el rojo (`vehiculos::pensar_ia`) con la misma función.

use bevy::prelude::*;
use ha_shared::semaforos::{luz, Luz};

/// Un semáforo de esquina: de qué eje es (este-oeste o norte-sur).
#[derive(Component)]
pub struct Semaforo {
    pub ew: bool,
}

/// Tapa oscura sobre una lámpara: se muestra cuando esa lámpara NO está prendida.
#[derive(Component)]
struct Tapa(Luz);

/// Posición y tamaño de las tres lámparas en el recorte de 32×160 px de cada cara: `(x, y, w, h)` del centro en px, rojo/amarillo/verde.
fn lamparas(key: &str) -> Option<[(f32, f32, f32, f32); 3]> {
    match key {
        "road/semaforo_frente" => Some([(15.0, 47.5, 14.0, 11.0), (15.0, 61.5, 14.0, 11.0), (15.0, 75.0, 14.0, 11.0)]),
        "road/semaforo_izquierda" => Some([(5.0, 39.5, 10.0, 9.0), (5.0, 54.0, 10.0, 9.0), (5.0, 68.0, 10.0, 9.0)]),
        "road/semaforo_derecha" => Some([(27.0, 39.5, 10.0, 9.0), (27.0, 54.0, 10.0, 9.0), (27.0, 68.0, 10.0, 9.0)]),
        _ => None,
    }
}

/// Convierte un poste recién sembrado en semáforo con fase. `size` es el tamaño con el que se dibuja el sprite.
pub fn equipar(ent: &mut EntityCommands, key: &str, ew: bool, size: Vec2) {
    let Some(l) = lamparas(key) else { return };
    ent.insert(Semaforo { ew });
    let colores = [Luz::Roja, Luz::Amarilla, Luz::Verde];
    ent.with_children(|p| {
        for (i, (x, y, w, h)) in l.iter().enumerate() {
            p.spawn((
                Tapa(colores[i]),
                crate::sim::UnlitSprite,
                Sprite { color: Color::srgba(0.13, 0.13, 0.18, 0.92), custom_size: Some(Vec2::new(w / 32.0 * size.x, h / 160.0 * size.y)), ..default() },
                Transform::from_xyz((x / 32.0 - 0.5) * size.x, (0.5 - y / 160.0) * size.y, 0.001),
                Visibility::Hidden,
            ));
        }
    });
}

pub struct SemaforosPlugin;

impl Plugin for SemaforosPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, fase_de_los_semaforos.run_if(crate::game_phase::world_active));
    }
}

fn fase_de_los_semaforos(
    time: Res<Time>,
    postes: Query<(&Semaforo, &Children)>,
    mut tapas: Query<(&Tapa, &mut Visibility)>,
    mut acc: Local<f32>,
) {
    *acc += time.delta_secs();
    if *acc < 0.2 {
        return;
    }
    *acc = 0.0;
    let t = time.elapsed_secs();
    for (s, hijos) in &postes {
        let ahora = luz(t, s.ew);
        for h in hijos.iter() {
            if let Ok((tapa, mut vis)) = tapas.get_mut(*h) {
                let quiero = if tapa.0 == ahora { Visibility::Hidden } else { Visibility::Inherited };
                if *vis != quiero {
                    *vis = quiero;
                }
            }
        }
    }
}
