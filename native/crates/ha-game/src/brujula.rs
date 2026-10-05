//! **Brújula de ciudades** (pedido del usuario, 4 oct 2026: «no encuentro las ciudades»). Una línea fija en el HUD:
//! la ciudad o pueblo más cercano, a cuántos metros y hacia dónde queda (rumbo en palabras: la fuente no tiene
//! flechas). Se recalcula cada medio segundo y sólo en la superficie.

use bevy::prelude::*;
use ha_shared::trenes;

use crate::game_phase::GamePhase;
use crate::scene::WorldContextRuntime;
use crate::sim::{Player, SimState};
use crate::world2d::{active_surface_body, ActivePlanet, TileWorld};

#[derive(Component)]
struct BrujulaHud;

pub struct BrujulaPlugin;

impl Plugin for BrujulaPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, actualizar.run_if(in_state(GamePhase::Playing)))
            .add_systems(OnExit(GamePhase::Playing), limpiar);
    }
}

fn limpiar(mut commands: Commands, q: Query<Entity, With<BrujulaHud>>) {
    for e in &q {
        commands.entity(e).despawn_recursive();
    }
}

/// Rumbo en palabras. `dx` crece hacia el este y `dz` hacia el sur (norte = z negativo).
pub fn rumbo(dx: f32, dz: f32) -> &'static str {
    let ang = (-dz).atan2(dx).to_degrees(); // 0 = este, 90 = norte
    const N: [&str; 8] = ["este", "noreste", "norte", "noroeste", "oeste", "suroeste", "sur", "sureste"];
    N[(((ang + 360.0 + 22.5) % 360.0) / 45.0) as usize % 8]
}

fn distancia_en_texto(m: f32) -> String {
    if m < 1000.0 {
        format!("{:.0} m", m)
    } else {
        format!("{:.1} km", m / 1000.0)
    }
}

pub fn linea(body: ha_shared::CelestialBody, x: f32, z: f32) -> Option<String> {
    let (nombre, (cx, cz), _) = trenes::ciudades(body)
        .into_iter()
        .min_by(|a, b| {
            let da = (a.1 .0 - x).hypot(a.1 .1 - z);
            let db = (b.1 .0 - x).hypot(b.1 .1 - z);
            da.total_cmp(&db)
        })?;
    let (dx, dz) = (cx - x, cz - z);
    let d = dx.hypot(dz);
    if d < 60.0 {
        Some(format!("Estás en {nombre}"))
    } else {
        Some(format!("{nombre}: {} al {}", distancia_en_texto(d), rumbo(dx, dz)))
    }
}

#[allow(clippy::too_many_arguments)]
fn actualizar(
    mut commands: Commands,
    time: Res<Time>,
    layout: Res<crate::input::TouchLayout>,
    ctx: Res<WorldContextRuntime>,
    planet: Res<ActivePlanet>,
    tiles: Res<TileWorld>,
    player: Query<&SimState, With<Player>>,
    mut q: Query<(Entity, &mut Text), With<BrujulaHud>>,
    mut cd: Local<f32>,
) {
    *cd -= time.delta_secs();
    if *cd > 0.0 {
        return;
    }
    *cd = 0.5;
    let Ok(sim) = player.get_single() else { return };
    let texto = if tiles.z == 0 && matches!(ctx.active, ha_shared::WorldContext::Overworld) {
        linea(active_surface_body(&ctx, &planet), sim.body.x, sim.body.z)
    } else {
        None
    };
    let Some(texto) = texto else {
        for (e, _) in &q {
            commands.entity(e).despawn_recursive();
        }
        return;
    };
    if let Ok((_, mut t)) = q.get_single_mut() {
        if t.0 != texto {
            t.0 = texto;
        }
        return;
    }
    let k = crate::ui_pack::k(&layout);
    commands.spawn((
        Node {
            position_type: PositionType::Absolute,
            right: Val::Px(layout.insets.right as f32 + 12.0),
            top: Val::Px(layout.insets.top as f32 + 150.0 * k),
            max_width: Val::Px(layout.width * 0.3),
            ..default()
        },
        GlobalZIndex(60),
        BrujulaHud,
        Text::new(texto),
        TextFont { font_size: (16.0 * k).max(11.0), ..default() },
        TextColor(Color::srgb(1.0, 0.92, 0.6)),
        Name::new("BrujulaHud"),
    ));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn el_rumbo_sale_en_palabras_con_el_norte_arriba() {
        assert_eq!(rumbo(10.0, 0.0), "este");
        assert_eq!(rumbo(0.0, -10.0), "norte");
        assert_eq!(rumbo(0.0, 10.0), "sur");
        assert_eq!(rumbo(-10.0, -10.0), "noroeste");
    }

    #[test]
    fn siempre_hay_una_ciudad_a_la_que_apuntar_en_la_tierra() {
        let l = linea(ha_shared::CelestialBody::Earth, 0.0, 0.0).expect("sin ciudades");
        assert!(l.contains(':') || l.starts_with("Estás en"));
    }
}
