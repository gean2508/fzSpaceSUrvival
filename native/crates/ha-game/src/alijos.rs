//! **Alijos: tesoros escondidos que hay que ir a buscar** (el "ir a algún lado" que le faltaba
//! al mundo abierto — las llaves y los dungeons del OASIS, en versión caminable).
//!
//! El mundo se divide en sectores de `SECTOR` m. Cada uno tiene, determinista por semilla, un
//! alijo en un punto fijo. Cuando te acercás a menos de `CREAR_A` m de ese punto (y no es
//! pared/agua) se materializa: un cofre con botín rico (ropa, gemas, lingotes, comida) y 2-3
//! infectados que lo custodian de día y de noche. Nada se genera lejos de vos: cupo de un
//! alijo por revisión (cada 2 s) y sólo en la Tierra.
//!
//! **La guía sale del mapa** (3 oct 2026). Un alijo se puede encontrar de casualidad, pero la
//! brújula de texto (`ALIJO a 120 m · NE`) sólo aparece si llevás el **mapa del tesoro** de ese
//! alijo (`mapa_tesoro`: lo sueltan los infectados, aparece en cómodas y, a veces, un alijo trae
//! el mapa del siguiente). Cada mapa que entra a la mochila marca un alijo lejano (400-900 m);
//! si el mapa sale de la mochila (lo guardás, lo tirás) la guía se apaga y vuelve, al mismo
//! alijo, cuando lo recuperás. Abrir el alijo gasta su mapa.
//!
//! El estado (sectores creados, uids, abiertos, mapas leídos) vive en `alijos.json` de la
//! partida, y el cofre en `chests.json` como cualquier otro.

use bevy::prelude::*;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

use crate::chests::{ChestPanelState, ChestWorld};
use crate::game_phase::GamePhase;
use crate::i18n::{idioma_actual, Idioma};
use crate::inventory_persist::{InventoryPersistState, PlayerInventory};
use crate::obstaculos::Obstaculos;
use crate::scene::WorldContextRuntime;
use crate::sim::Player;
use crate::world2d::{active_surface_body, ActivePlanet};

const SECTOR: i64 = 200;
const CREAR_A: f32 = 140.0;
const REVISAR_CADA: f32 = 2.0;
/// El ítem que da la guía.
pub const MAPA_TESORO: &str = "mapa_tesoro";
/// Un mapa marca un alijo a esta distancia (en sectores, chebyshev): ni a la vuelta de la
/// esquina ni del otro lado del mundo.
const MAPA_MIN: i64 = 2;
const MAPA_MAX: i64 = 4;
/// Cuánto queda en pantalla el "¡Mapa del tesoro!" al leer uno nuevo.
const AVISO_S: f32 = 4.0;

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
    /// Sectores marcados por mapas, en el orden en que se leyeron, todavía sin abrir. Llevar `n`
    /// mapas encima guía a los primeros `n`.
    mapas: Vec<(i64, i64)>,
    cargado: bool,
    acc: f32,
    /// Segundos que le quedan al aviso de mapa nuevo.
    aviso: f32,
}

/// Lo que se escribe en `alijos.json`. Las partidas viejas guardaban sólo la lista de sectores.
#[derive(Serialize, Deserialize, Default)]
struct Guardado {
    sectores: Vec<(i64, i64, Alijo)>,
    #[serde(default)]
    mapas: Vec<(i64, i64)>,
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
    // Un alijo es lo que alguien escondió para el fin del mundo: munición casi siempre y, a veces, un arma.
    if azar(100) < 70 {
        v.push(crate::armas::municion_al_azar(azar(1000) as f32 / 1000.0, azar(1000) as f32 / 1000.0));
    }
    if azar(100) < 15 {
        v.push((crate::armas::arma_al_azar(azar(1000) as f32 / 1000.0, azar(1000) as f32 / 1000.0, azar(1000) as f32 / 1000.0), 1));
    }
    // Un alijo de cada tres guarda el mapa del siguiente: la cadena de llaves del OASIS.
    if azar(100) < 35 {
        v.push((MAPA_TESORO, 1));
    }
    v
}

fn archivo() -> Option<std::path::PathBuf> {
    crate::partida::activa().map(|id| crate::partida::carpeta(&id).join("alijos.json"))
}

fn guardar(a: &Alijos) {
    let Some(p) = archivo() else { return };
    let g = Guardado {
        sectores: a.por_sector.iter().map(|((x, z), v)| (*x, *z, v.clone())).collect(),
        mapas: a.mapas.clone(),
    };
    if let Ok(t) = serde_json::to_string(&g) {
        let _ = std::fs::write(p, t);
    }
}

fn leer_guardado(t: &str) -> Guardado {
    serde_json::from_str::<Guardado>(t)
        .or_else(|_| serde_json::from_str::<Vec<(i64, i64, Alijo)>>(t).map(|sectores| Guardado { sectores, mapas: Vec::new() }))
        .unwrap_or_default()
}

fn cargar() -> Guardado {
    archivo().and_then(|p| std::fs::read_to_string(p).ok()).map(|t| leer_guardado(&t)).unwrap_or_default()
}

/// El sector que marca un mapa nuevo leído parado en `(sx0, sz0)`: uno del anillo
/// `MAPA_MIN..=MAPA_MAX` que no se haya creado ni esté ya marcado. `n` desempata (cuántos
/// mapas se leyeron), así dos mapas seguidos no marcan el mismo.
fn sector_para_mapa(semilla: u32, sx0: i64, sz0: i64, n: u32, ocupado: impl Fn((i64, i64)) -> bool) -> Option<(i64, i64)> {
    let mut libres = Vec::new();
    for sx in sx0 - MAPA_MAX..=sx0 + MAPA_MAX {
        for sz in sz0 - MAPA_MAX..=sz0 + MAPA_MAX {
            let d = (sx - sx0).abs().max((sz - sz0).abs());
            if d >= MAPA_MIN && !ocupado((sx, sz)) {
                libres.push((sx, sz));
            }
        }
    }
    if libres.is_empty() {
        return None;
    }
    let h = ha_shared::worldgen::hash_cell(semilla ^ 0x3A9A, sx0.wrapping_mul(31) + n as i64, sz0);
    Some(libres[h as usize % libres.len()])
}

/// Dónde está el alijo de un sector marcado: el cofre si ya se creó, el punto del sector si no.
fn posicion(a: &Alijos, semilla: u32, s: (i64, i64)) -> (f32, f32) {
    match a.por_sector.get(&s) {
        Some(al) => (al.x, al.z),
        None => punto_del_sector(semilla, s.0, s.1),
    }
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
            .add_systems(Update, (crear_alijos, leer_mapas, marcar_abiertos, brujula).chain().run_if(in_state(GamePhase::Playing)))
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
        let g = cargar();
        a.por_sector = g.sectores.into_iter().map(|(x, z, al)| ((x, z), al)).collect();
        a.mapas = g.mapas;
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
            // Al que marca un mapa se lo busca más lejos: el mapa no puede llevar a la nada.
            let radio: i64 = if a.mapas.contains(&(sx, sz)) { 40 } else { 6 };
            let mut sitio = None;
            'busca: for r in 0..radio {
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
                // Si un mapa lo marcaba, el mapa se queda y `leer_mapas` le busca otro alijo.
                a.mapas.retain(|s| *s != (sx, sz));
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

/// Cada mapa del tesoro que entra a la mochila marca un alijo. Los mapas son ítems comunes (un
/// contador), así que la cuenta manda: si hay más mapas que alijos marcados, se marca otro.
fn leer_mapas(
    mut a: ResMut<Alijos>,
    time: Res<Time>,
    inv: Res<PlayerInventory>,
    ctx: Res<WorldContextRuntime>,
    player: Query<&Transform, With<Player>>,
) {
    if a.aviso > 0.0 {
        a.aviso = (a.aviso - time.delta_secs()).max(0.0);
    }
    if !a.cargado || inv.qty(MAPA_TESORO) as usize <= a.mapas.len() {
        return;
    }
    if !matches!(ctx.active, ha_shared::WorldContext::Overworld) {
        return;
    }
    let Ok(pt) = player.get_single() else { return };
    let (px, pz) = (pt.translation.x, -pt.translation.y);
    let (sx0, sz0) = ((px / SECTOR as f32).floor() as i64, (pz / SECTOR as f32).floor() as i64);
    let n = a.mapas.len() as u32 + a.abiertos();
    let elegido = sector_para_mapa(ha_shared::partida::semilla(), sx0, sz0, n, |s| {
        a.por_sector.contains_key(&s) || a.mapas.contains(&s)
    });
    let Some(s) = elegido else { return };
    a.mapas.push(s);
    a.aviso = AVISO_S;
    guardar(&a);
    crate::causal::registrar("alijo", format!("mapa leído: marca el sector ({},{})", s.0, s.1));
}

fn marcar_abiertos(
    mut a: ResMut<Alijos>,
    panel: Res<ChestPanelState>,
    mut inv: ResMut<PlayerInventory>,
    mut persist: ResMut<InventoryPersistState>,
) {
    let Some(uid) = panel.open_uid else { return };
    let mut abierto = None;
    for (s, al) in a.por_sector.iter_mut() {
        if al.uid == uid && !al.abierto {
            al.abierto = true;
            abierto = Some(*s);
            crate::causal::registrar("alijo", format!("abierto en ({:.0},{:.0})", al.x, al.z));
        }
    }
    let Some(s) = abierto else { return };
    // Encontrado: su mapa ya no sirve. Sólo se gasta si lo llevabas (si no, la guía ni se veía).
    if let Some(i) = a.mapas.iter().position(|m| *m == s) {
        a.mapas.remove(i);
        if i < inv.qty(MAPA_TESORO) as usize && inv.remove(MAPA_TESORO, 1) {
            persist.mark_dirty();
        }
    }
    guardar(&a);
}

fn brujula(
    mut commands: Commands,
    a: Res<Alijos>,
    inv: Res<PlayerInventory>,
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
                top: Val::Px(148.0),
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
    let semilla = ha_shared::partida::semilla();
    // Sólo guían los alijos cuyo mapa llevás encima: los primeros `n` marcados.
    let n = inv.qty(MAPA_TESORO) as usize;
    let cercano = a
        .mapas
        .iter()
        .take(n)
        .map(|s| posicion(&a, semilla, *s))
        .map(|(x, z)| ((x, z), ((x - px).powi(2) + (z - pz).powi(2)).sqrt()))
        .min_by(|x, y| x.1.total_cmp(&y.1));
    let nuevo = a.aviso > 0.0;
    for (mut t, mut v) in &mut q {
        match cercano {
            Some(((x, z), d)) => {
                let rumbo = viento(x - px, z - pz, en);
                let s = match (en, nuevo) {
                    (true, true) => format!("Treasure map! CACHE {d:.0} m · {rumbo}"),
                    (true, false) => format!("CACHE {d:.0} m · {rumbo}"),
                    (false, true) => format!("¡Mapa del tesoro! ALIJO a {d:.0} m · {rumbo}"),
                    (false, false) => format!("ALIJO a {d:.0} m · {rumbo}"),
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
        for t in ["ALIJO a 120 m · NO", "CACHE 90 m · NW", "¡Mapa del tesoro! ALIJO a 640 m · NE", "Treasure map! CACHE 640 m · NE"] {
            assert!(t.chars().all(crate::fuente::dibujable), "{t}");
        }
    }

    #[test]
    fn el_mapa_marca_un_sector_lejano_libre_y_distinto() {
        let mut marcados: Vec<(i64, i64)> = Vec::new();
        for n in 0..10 {
            let s = sector_para_mapa(4242, 3, -2, n, |s| marcados.contains(&s)).unwrap();
            let d = (s.0 - 3).abs().max((s.1 + 2).abs());
            assert!((MAPA_MIN..=MAPA_MAX).contains(&d), "{s:?} a {d} sectores");
            assert!(!marcados.contains(&s));
            marcados.push(s);
        }
        // Con todo el anillo ocupado no inventa nada.
        assert_eq!(sector_para_mapa(1, 0, 0, 0, |_| true), None);
    }

    #[test]
    fn las_partidas_viejas_cargan_sin_mapas_y_las_nuevas_con_mapas() {
        let viejo = r#"[[1,2,{"uid":7,"x":10.0,"z":20.0,"abierto":false}]]"#;
        let g = leer_guardado(viejo);
        assert_eq!(g.sectores.len(), 1);
        assert!(g.mapas.is_empty());
        let nuevo = Guardado { sectores: g.sectores, mapas: vec![(5, -3)] };
        let g2 = leer_guardado(&serde_json::to_string(&nuevo).unwrap());
        assert_eq!(g2.mapas, vec![(5, -3)]);
        assert_eq!(g2.sectores[0].2.uid, 7);
    }

    #[test]
    fn algunos_alijos_traen_el_mapa_del_siguiente() {
        let con = (0..200u32)
            .filter(|n| botin_alijo(n.wrapping_mul(2654435761)).iter().any(|(i, _)| *i == MAPA_TESORO))
            .count();
        assert!((30..=110).contains(&con), "{con} de 200");
    }
}
