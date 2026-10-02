//! **El descubrimiento se VE: los que estudian trabajan en el mundo.**
//!
//! La civilización (`ha_shared::civ::descubrimiento`) anota en el cuaderno de cada pueblo qué se
//! probó y qué salió. Hasta acá eso eran números. Acá cada erudito o artesano que vive cerca de
//! vos —encarnado en un aldeano de verdad— se pone a probar, con su cuaderno a la vista: lo dice
//! en voz alta, hace una fogata chica donde prueba, y sale lo que de verdad le sale a su pueblo
//! (la receta con sus aciertos y fallos; si cree en un mito, lo cuenta).
//!
//! * No inventa nada: cada experimento que se ve es una entrada real del cuaderno de su pueblo.
//! * Si te acercás, te cuenta lo que hace (menú del aldeano → PREGUNTAR) y, si te aprecia, te
//!   enseña lo que sabe (`NpcAction::Aprender` en `npc_menu.rs`).
//! * Cupo: se miran como mucho `MIRAR` aldeanos por pasada y sólo los que están a la vista.

use bevy::prelude::*;
use ha_shared::civ::persona::Oficio;
use std::collections::HashMap;

use crate::civilizacion::{Civ, Receta};
use crate::dialog::{say_from, SpeechQueue};
use crate::sim::Player;
use crate::society::Npc;
use ha_shared::sims::Actividad;

/// Hasta dónde (celdas) se ven estudiar.
const VISTA: f32 = 38.0;
const PASADA_S: f32 = 1.0;
const MIRAR: usize = 3;
const ENTRE_PRUEBAS: (f32, f32) = (18.0, 40.0);
const ESPERA_RESULTADO: f32 = 5.5;
const FOGATA_S: f32 = 7.0;
const REFRESCO_S: f32 = 150.0;

/// Las circunstancias con las que un crédulo ata un éxito (el índice es `Entrada::ctx_creido`).
pub const CIRCUNSTANCIAS: [&str; 8] = [
    "de noche",
    "con luna llena",
    "de mañana temprano",
    "con viento del norte",
    "bajo la lluvia",
    "en ayunas",
    "con frío",
    "un día de fiesta",
];

struct Estudioso {
    nombre: String,
    oficio: Oficio,
    recetas: Vec<Receta>,
    proxima: f32,
    refresco: f32,
    /// Lo que va a decir cuando termine de probar, y cuándo.
    pendiente: Option<(f32, String, Color)>,
}

#[derive(Resource, Default)]
pub struct Taller {
    gente: HashMap<u64, Option<Estudioso>>,
}

#[derive(Component)]
struct Fogata {
    ttl: f32,
    fase: f32,
}

pub struct SaberVivoPlugin;

impl Plugin for SaberVivoPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Taller>().add_systems(Update, (estudiar, fogatas).run_if(crate::game_phase::world_active));
    }
}

/// La frase de la ley, en la boca de quien la prueba.
fn que_espera(ley: &str) -> &'static str {
    match ley {
        "cuece" => "a ver si se endurece al fuego",
        "carboniza" => "tapado y despacito, a ver si se vuelve carbón",
        "reduce" => "con mucho fuego y carbón, a ver si suelta metal",
        "alea" => "fundidos juntos, a ver qué metal sale",
        "calcina" => "calcinado, a ver si se vuelve polvo que liga",
        "vitrifica" => "con fundente, a ver si se vuelve vidrio",
        _ => "mezclados, a ver qué pasa",
    }
}

/// Lo que dice al ponerse a probar `r`.
pub fn frase_de_prueba(r: &Receta) -> String {
    let ab = match &r.nombre_b {
        Some(nb) => format!("{} con {nb}", r.nombre_a),
        None => r.nombre_a.clone(),
    };
    let mut t = format!("Pruebo {ab}: {}.", que_espera(&r.entrada.ley));
    if r.entrada.ritual {
        t += &format!(" Siempre me sale {}.", CIRCUNSTANCIAS[(r.entrada.ctx_creido as usize) % 8]);
    }
    t
}

/// Lo que dice cuando termina: sale (con la frecuencia que de verdad le sale) o no.
pub fn frase_de_resultado(r: &Receta, azar: u64) -> (String, bool) {
    let n = r.entrada.n.max(1) as u64;
    let sale = (azar % n) < r.entrada.ok as u64;
    if sale {
        (format!("¡Salió {}!", r.resultado.nombre), true)
    } else {
        ("Nada... se me echó a perder. Otra vez.".to_string(), false)
    }
}

fn estudiar(
    mut commands: Commands,
    time: Res<Time>,
    mut civ: ResMut<Civ>,
    mut taller: ResMut<Taller>,
    player: Query<&Transform, With<Player>>,
    mut npcs: Query<(Entity, &mut Npc)>,
    mut speech: ResMut<SpeechQueue>,
    glow: Option<Res<crate::light2d::GlowTexture>>,
    mut acc: Local<f32>,
) {
    *acc += time.delta_secs();
    if *acc < PASADA_S {
        return;
    }
    *acc = 0.0;
    let Ok(pt) = player.get_single() else { return };
    let (px, pz) = (pt.translation.x, -pt.translation.y);
    let ahora = time.elapsed_secs();
    let mut mirados = 0;
    for (e, mut n) in &mut npcs {
        let d2 = (n.x - px).powi(2) + (n.z - pz).powi(2);
        if d2 > VISTA * VISTA {
            continue;
        }
        let uid = n.uid;
        let entrada = taller.gente.entry(uid);
        let est = match entrada {
            std::collections::hash_map::Entry::Vacant(v) => {
                // Sólo uno de cada cinco aldeanos se anota para estudiar; los demás no gastan nada.
                if uid % 5 != 0 || mirados >= MIRAR {
                    v.insert(None);
                    continue;
                }
                mirados += 1;
                let s = civ.saber_de_npc(uid, n.x, n.z, true).filter(|s| matches!(s.oficio, Oficio::Erudito | Oficio::Artesano) && !s.recetas.is_empty());
                let nuevo = s.map(|s| Estudioso {
                    nombre: s.nombre,
                    oficio: s.oficio,
                    recetas: s.recetas,
                    proxima: ahora + ENTRE_PRUEBAS.0 * 0.3 + (uid % 11) as f32,
                    refresco: ahora + REFRESCO_S,
                    pendiente: None,
                });
                v.insert(nuevo)
            }
            std::collections::hash_map::Entry::Occupied(o) => o.into_mut(),
        };
        let Some(est) = est else { continue };
        // El cuaderno del pueblo crece: de tanto en tanto se vuelve a leer.
        if ahora >= est.refresco && mirados < MIRAR {
            mirados += 1;
            est.refresco = ahora + REFRESCO_S;
            if let Some(s) = civ.saber_de_npc(uid, n.x, n.z, false) {
                if !s.recetas.is_empty() {
                    est.recetas = s.recetas;
                }
            }
        }
        // ¿Terminó la prueba que estaba haciendo?
        if let Some((t, texto, color)) = est.pendiente.clone() {
            if ahora >= t {
                est.pendiente = None;
                crate::society::push_history(&mut n.history, &texto);
                say_from(&mut speech, e, n.x, n.z, texto, color, false);
            }
            continue;
        }
        if ahora < est.proxima {
            continue;
        }
        // Estudia cuando está en lo suyo: trabajando o sin nada urgente.
        if !matches!(n.vida.actividad(), Actividad::Trabajar | Actividad::Vagar) {
            continue;
        }
        let azar = ha_shared::worldgen::hash_cell(0x57D1, uid as i64, (ahora as i64) / 7) as u64;
        let r = &est.recetas[(azar as usize >> 3) % est.recetas.len()];
        let dicho = frase_de_prueba(r);
        let (res, sale) = frase_de_resultado(r, azar >> 11);
        crate::society::push_history(&mut n.history, &dicho);
        say_from(&mut speech, e, n.x, n.z, dicho, Color::srgb(0.85, 0.95, 1.0), false);
        let col = if sale { Color::srgb(1.0, 0.9, 0.5) } else { Color::srgb(0.8, 0.75, 0.7) };
        est.pendiente = Some((ahora + ESPERA_RESULTADO, res, col));
        est.proxima = ahora + ENTRE_PRUEBAS.0 + (azar % 1000) as f32 / 1000.0 * (ENTRE_PRUEBAS.1 - ENTRE_PRUEBAS.0);
        if let Some(g) = glow.as_ref() {
            let r = crate::world2d::render_xy(n.x + 0.9, n.z + 0.3);
            commands.spawn((
                Sprite { image: g.0.clone(), color: Color::srgba(1.0, 0.62, 0.25, 0.55), custom_size: Some(Vec2::splat(3.2)), ..default() },
                Transform::from_xyz(r.x, r.y - 0.2, crate::layers::OVERHEAD - 0.5),
                Fogata { ttl: FOGATA_S, fase: (uid % 97) as f32 },
            ));
        }
        let _ = (&est.nombre, est.oficio);
    }
}

/// La fogata del experimento titila y se apaga.
fn fogatas(mut commands: Commands, time: Res<Time>, mut q: Query<(Entity, &mut Fogata, &mut Sprite)>) {
    let dt = time.delta_secs();
    for (e, mut f, mut s) in &mut q {
        f.ttl -= dt;
        if f.ttl <= 0.0 {
            commands.entity(e).despawn();
            continue;
        }
        let t = time.elapsed_secs() * 9.0 + f.fase;
        let parpadeo = 0.78 + 0.22 * (t.sin() * 0.6 + (t * 2.3).sin() * 0.4);
        let apagado = (f.ttl / 1.5).clamp(0.0, 1.0);
        s.color = Color::srgba(1.0, 0.62, 0.25, 0.55 * parpadeo * apagado);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ha_shared::civ::descubrimiento::Entrada;
    use ha_shared::civ::materia::*;

    fn receta(ritual: bool, n: u16, ok: u16) -> Receta {
        let mundo = Materia::nueva(5);
        let Resultado::Obtiene(m, ley) = mundo.reaccionar(ARCILLA, None, 0.5, false) else { panic!("la arcilla al fuego debería dar algo") };
        Receta {
            entrada: Entrada { a: ARCILLA, b: u64::MAX, calor: 10, resultado: Some(m.id), ley: ley.into(), n, ok, ctx_vistos: 1, ritual, ctx_creido: 0, de_oido: false },
            resultado: m,
            nombre_a: "arcilla".into(),
            nombre_b: None,
        }
    }

    #[test]
    fn el_mito_se_dice_en_voz_alta() {
        let t = frase_de_prueba(&receta(true, 3, 3));
        assert!(t.contains("de noche"), "{t}");
        assert!(!frase_de_prueba(&receta(false, 3, 3)).contains("Siempre"));
    }

    #[test]
    fn sale_con_la_frecuencia_que_de_verdad_le_sale() {
        let r = receta(false, 10, 7);
        let sale = (0..1000u64).filter(|a| frase_de_resultado(&r, *a).1).count();
        assert!((650..=750).contains(&sale), "salió {sale} de 1000");
    }
}
