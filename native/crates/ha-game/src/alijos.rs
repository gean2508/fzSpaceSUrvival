//! **Alijos: tesoros escondidos que hay que ir a buscar** (el "ir a algún lado" que le faltaba
//! al mundo abierto — las llaves y los dungeons del OASIS, en versión caminable).
//!
//! El mundo se divide en sectores de `SECTOR` m. Cada uno tiene, determinista por semilla, un
//! alijo en un punto fijo. Cuando te acercás a menos de `CREAR_A` m de ese punto (y no es
//! pared/agua) se materializa: un cofre con botín rico (ropa, gemas, lingotes, comida) y 2-3
//! infectados que lo custodian de día y de noche. Nada se genera lejos de vos: cupo de un
//! alijo por revisión (cada 2 s) y sólo en la Tierra.
//!
//! Una brújula de texto (`ALIJO a 120 m · NE`) apunta al más cercano que ya conocés y no
//! abriste. El estado (sectores creados, uids, abiertos) vive en `alijos.json` de la partida,
//! y el cofre en `chests.json` como cualquier otro.

use bevy::prelude::*;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

use crate::chests::{ChestPanelState, ChestWorld};
use crate::game_phase::GamePhase;
use crate::i18n::{idioma_actual, Idioma};
use crate::obstaculos::Obstaculos;
use crate::scene::WorldContextRuntime;
use crate::sim::Player;
use crate::world2d::{active_surface_body, ActivePlanet};

const SECTOR: i64 = 200;
const CREAR_A: f32 = 140.0;
const REVISAR_CADA: f32 = 2.0;

#[derive(Serialize, Deserialize, Default, Clone)]
struct Alijo {
    uid: u32,
    x: f32,
    z: f32,
    abierto: bool,
}

#[derive(Resource, Default)]
pub struct Alijos {
    por_sector: BTreeMap<(i64, i64), Alijo>,
    cargado: bool,
    acc: f32,
}

impl Alijos {
    pub fn abiertos(&self) -> u32 {
        self.por_sector.values().filter(|a| a.abierto).count() as u32
    }
}

/// El punto del alijo de un sector: siempre el mismo para la misma semilla.
pub fn punto_del_sector(semilla: u32, sx: i64, sz: i64) -> (f32, f32) {
    let h = ha_shared::worldgen::hash_cell(semilla ^ 0xA11_705, sx, sz);
    let ox = 20 + (h % 160) as i64;
    let oz = 20 + ((h >> 12) % 160) as i64;
    ((sx * SECTOR + ox) as f32, (sz * SECTOR + oz) as f32)
}

/// El botín de un alijo: seguro hay algo bueno. `n` sale del sector.
pub fn botin_alijo(n: u32) -> Vec<(&'static str, u32)> {
    let mut s = n | 1;
    let mut azar = move |m: u32| {
        s ^= s << 13;
        s ^= s >> 17;
        s ^= s << 5;
        s % m.max(1)
    };
    let mut v: Vec<(&'static str, u32)> = vec![
        (ha_shared::armadura::prenda_al_azar(azar(1000)), 1),
        (["gem_blue", "gem_red", "gold_ingot", "iron_ingot"][azar(4) as usize], 1 + azar(2)),
        (["bread", "pie", "soup", "apple"][azar(4) as usize], 2 + azar(3)),
    ];
    if azar(100) < 50 {
        v.push((ha_shared::armadura::prenda_al_azar(azar(1000)), 1));
    }
    if azar(100) < 40 {
        v.push((["torch", "lantern", "potion_green", "potion_blue"][azar(4) as usize], 1 + azar(2)));
    }
    v
}

fn archivo() -> Option<std::path::PathBuf> {
    crate::partida::activa().map(|id| crate::partida::carpeta(&id).join("alijos.json"))
}

fn guardar(a: &Alijos) {
    let Some(p) = archivo() else { return };
    let lista: Vec<(i64, i64, Alijo)> = a.por_sector.iter().map(|((x, z), v)| (*x, *z, v.clone())).collect();
    if let Ok(t) = serde_json::to_string(&lista) {
        let _ = std::fs::write(p, t);
    }
}

fn cargar() -> BTreeMap<(i64, i64), Alijo> {
    archivo()
        .and_then(|p| std::fs::read_to_string(p).ok())
        .and_then(|t| serde_json::from_str::<Vec<(i64, i64, Alijo)>>(&t).ok())
        .map(|l| l.into_iter().map(|(x, z, a)| ((x, z), a)).collect())
        .unwrap_or_default()
}

/// Rumbo en 8 vientos (castellano o inglés).
pub fn viento(dx: f32, dz: f32, en: bool) -> &'static str {
    // z crece hacia el sur.
    let ang = (-dz).atan2(dx).to_degrees(); // 0 = este, 90 = norte
    let i = (((ang + 360.0 + 22.5) % 360.0) / 45.0) as usize % 8;
    const ES: [&str; 8] = ["E", "NE", "N", "NO", "O", "SO", "S", "SE"];
    const EN: [&str; 8] = ["E", "NE", "N", "NW", "W", "SW", "S", "SE"];
    if en { EN[i] } else { ES[i] }
}

#[derive(Component)]
struct BrujulaAlijo;

pub struct AlijosPlugin;

impl Plugin for AlijosPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Alijos>()
            .add_systems(Update, (crear_alijos, marcar_abiertos, brujula).chain().run_if(in_state(GamePhase::Playing)))
            .add_systems(OnExit(GamePhase::Playing), salir);
    }
}

fn salir(mut commands: Commands, mut a: ResMut<Alijos>, q: Query<Entity, With<BrujulaAlijo>>) {
    *a = Alijos::default();
    for e in &q {
        commands.entity(e).despawn_recursive();
    }
}

#[allow(clippy::too_many_arguments)]
fn crear_alijos(
    mut commands: Commands,
    time: Res<Time>,
    mut a: ResMut<Alijos>,
    mut chests: ResMut<ChestWorld>,
    art: Res<crate::art2d::ArtLibrary>,
    ctx: Res<WorldContextRuntime>,
    planet: Res<ActivePlanet>,
    obst: Obstaculos,
    player: Query<&Transform, With<Player>>,
) {
    if !a.cargado {
        a.por_sector = cargar();
        a.cargado = true;
    }
    a.acc += time.delta_secs();
    if a.acc < REVISAR_CADA {
        return;
    }
    a.acc = 0.0;
    if !matches!(ctx.active, ha_shared::WorldContext::Overworld) {
        return;
    }
    let Ok(pt) = player.get_single() else { return };
    let (px, pz) = (pt.translation.x, -pt.translation.y);
    let body = active_surface_body(&ctx, &planet);
    let semilla = ha_shared::partida::semilla();
    let (sx0, sz0) = ((px / SECTOR as f32).floor() as i64, (pz / SECTOR as f32).floor() as i64);
    for sx in sx0 - 1..=sx0 + 1 {
        for sz in sz0 - 1..=sz0 + 1 {
            if a.por_sector.contains_key(&(sx, sz)) {
                continue;
            }
            let (x, z) = punto_del_sector(semilla, sx, sz);
            if ((x - px).powi(2) + (z - pz).powi(2)).sqrt() > CREAR_A {
                continue;
            }
            // Si cae en pared o agua se corre un poco; si no hay lugar, el sector queda sin alijo.
            let mut sitio = None;
            'busca: for r in 0..6i64 {
                for (dx, dz) in [(0, 0), (r, 0), (-r, 0), (0, r), (0, -r), (r, r), (-r, -r)] {
                    let (cx, cz) = (x + dx as f32, z + dz as f32);
                    if !obst.caja_frena(body, cx, cz, 0.6) && !obst.solido(body, cx.floor() as i64, cz.floor() as i64) {
                        sitio = Some((cx, cz));
                        break 'busca;
                    }
                }
            }
            let Some((cx, cz)) = sitio else {
                a.por_sector.insert((sx, sz), Alijo { uid: u32::MAX, x, z, abierto: true });
                guardar(&a);
                continue;
            };
            let n = ha_shared::worldgen::hash_cell(semilla ^ 0xB0071, sx, sz);
            let uid = chests.crear_alijo(body, cx, cz, &botin_alijo(n));
            for k in 0..(2 + n % 2) {
                let ang = (k as f32 / 3.0) * std::f32::consts::TAU + (n % 7) as f32;
                crate::infectados::spawn_guardia(&mut commands, &art, cx + ang.cos() * 3.0, cz + ang.sin() * 3.0, n.wrapping_add(k));
            }
            a.por_sector.insert((sx, sz), Alijo { uid, x: cx, z: cz, abierto: false });
            guardar(&a);
            crate::causal::registrar("alijo", format!("creado en el sector ({sx},{sz}), ({cx:.0},{cz:.0})"));
            eprintln!("ha-alijos: sector ({sx},{sz}) en ({cx:.0},{cz:.0})");
            return; // un alijo por revisión
        }
    }
}

fn marcar_abiertos(mut a: ResMut<Alijos>, panel: Res<ChestPanelState>) {
    let Some(uid) = panel.open_uid else { return };
    let mut cambio = false;
    for al in a.por_sector.values_mut() {
        if al.uid == uid && !al.abierto {
            al.abierto = true;
            cambio = true;
            crate::causal::registrar("alijo", format!("abierto en ({:.0},{:.0})", al.x, al.z));
        }
    }
    if cambio {
        guardar(&a);
    }
}

fn brujula(
    mut commands: Commands,
    a: Res<Alijos>,
    player: Query<&Transform, With<Player>>,
    mut q: Query<(&mut Text, &mut Visibility), With<BrujulaAlijo>>,
) {
    if q.is_empty() {
        commands.spawn((
            Text::new(""),
            TextFont { font_size: 12.0, ..default() },
            TextColor(Color::srgb(1.0, 0.85, 0.4)),
            Node {
                position_type: PositionType::Absolute,
                top: Val::Px(108.0),
                left: Val::Percent(50.0),
                margin: UiRect::left(Val::Px(-170.0)),
                width: Val::Px(340.0),
                justify_content: JustifyContent::Center,
                ..default()
            },
            TextLayout::new_with_justify(JustifyText::Center),
            Visibility::Hidden,
            GlobalZIndex(40),
            BrujulaAlijo,
            Name::new("BrujulaAlijo"),
        ));
        return;
    }
    let Ok(pt) = player.get_single() else { return };
    let (px, pz) = (pt.translation.x, -pt.translation.y);
    let en = idioma_actual() == Idioma::En;
    let cercano = a
        .por_sector
        .values()
        .filter(|al| !al.abierto)
        .map(|al| (al, ((al.x - px).powi(2) + (al.z - pz).powi(2)).sqrt()))
        .min_by(|x, y| x.1.total_cmp(&y.1));
    for (mut t, mut v) in &mut q {
        match cercano {
            Some((al, d)) => {
                let s = if en {
                    format!("CACHE {d:.0} m · {}", viento(al.x - px, al.z - pz, true))
                } else {
                    format!("ALIJO a {d:.0} m · {}", viento(al.x - px, al.z - pz, false))
                };
                if t.0 != s {
                    t.0 = s;
                }
                *v = Visibility::Inherited;
            }
            None => *v = Visibility::Hidden,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn el_punto_es_fijo_y_cae_dentro_de_su_sector() {
        for (sx, sz) in [(0, 0), (-3, 5), (12, -7)] {
            let (x, z) = punto_del_sector(777, sx, sz);
            assert_eq!((x, z), punto_del_sector(777, sx, sz));
            assert!(x >= (sx * SECTOR + 20) as f32 && x < (sx * SECTOR + 180) as f32);
            assert!(z >= (sz * SECTOR + 20) as f32 && z < (sz * SECTOR + 180) as f32);
        }
        assert_ne!(punto_del_sector(1, 0, 0), punto_del_sector(2, 0, 0));
    }

    #[test]
    fn el_botin_siempre_trae_ropa_gema_y_comida_con_nombre() {
        for n in 0..200u32 {
            let b = botin_alijo(n.wrapping_mul(2654435761));
            assert!(b.len() >= 3);
            assert!(b.iter().any(|(i, _)| ha_shared::armadura::es_hallable(i)));
            for (it, q) in &b {
                assert!(*q >= 1);
                assert!(ha_assets::slots::slot_label(&format!("items/{it}")).is_some(), "sin slot: {it}");
            }
        }
    }

    #[test]
    fn la_brujula_apunta_bien_y_dibuja() {
        assert_eq!(viento(10.0, 0.0, false), "E");
        assert_eq!(viento(0.0, -10.0, false), "N"); // z negativo = norte
        assert_eq!(viento(0.0, 10.0, false), "S");
        assert_eq!(viento(-10.0, 0.0, true), "W");
        for t in ["ALIJO a 120 m · NO", "CACHE 90 m · NW"] {
            assert!(t.chars().all(crate::fuente::dibujable), "{t}");
        }
    }
}
