//! **El barrio aprende.** Lo que pasa en un pueblo lo cambia, y se nota.
//!
//! Hasta acá los infectados sólo te perseguían a vos y las casas de los pueblos eran todas iguales de
//! buenas. Ahora el pueblo vive consecuencias, y de ellas aprende:
//!
//! * **Los infectados atacan a los vecinos** (`infectados::mover_y_morder` persigue al más cercano).
//!   Un vecino mordido grita; si muere, sus testigos lo viven (`Seguridad` a cero) y el barrio
//!   lo anota (`Lecciones::muertes`).
//! * **Cada uno reacciona según quién es y qué vivió el barrio**: los valientes salen a pelear con
//!   los infectados (se juntan: ver pelear a otros anima a más), el resto corre a su casa. Cuanto más
//!   sufrió el barrio (muertes, ataques), más valientes salen — de ahí que un barrio golpeado se
//!   organice solo, sin que nadie lo ordene.
//! * **Las casas que levantan los pueblos pueden salir mal** (`calidad_de_obra`: depende de si el
//!   pueblo sabe techar —cerámica, mortero, metal, vidrio— y de lo que ya aprendió). Una casa floja
//!   **gotea** cuando llueve fuerte: el que está adentro se queja, el barrio lo anota, y cuando
//!   para de llover la remiendan. La próxima obra sale mejor.
//! * **Lo que sufrieron cambia lo que construyen**: tras ataques se levantan más torres y residencias
//!   de varios pisos (`Lecciones::defensivo` → `casas_npc::plano_para`).
//! * La memoria del barrio se guarda (`barrio.json`) y es por pueblo.
//!
//! Costo: una pasada cada 0.4 s sobre lo que está a la vista; nada se materializa de más.

use bevy::prelude::*;
use ha_shared::sims::{Actividad, Anuncio, Motivo};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

use crate::civilizacion::Civ;
use crate::dialog::{say, say_from, SpeechQueue};
use crate::edificios::Edificios;
use crate::infectados::{Infectado, ATAQUE_CD, ATAQUE_RADIO};
use crate::sim::Player;
use crate::society::{despedir_npc, npc_name, push_history, Npc, Society};
use crate::weather::Weather;
use crate::world2d::{active_surface_body, ActivePlanet};

const PASADA_S: f32 = 0.4;
/// Hasta dónde un vecino se da cuenta de que hay un infectado.
const ALERTA: f32 = 18.0;
/// Alcance de un golpe de vecino.
const ALCANCE_VECINO: f32 = 1.7;
const DANO_VECINO: f32 = 4.0;
/// Cuánto le saca una mordida a la vida de un vecino (1.0 = llena).
const MORDIDA: f32 = 0.15;
const TESTIGOS: f32 = 25.0;
/// Una casa por debajo de esto gotea con lluvia fuerte.
pub const GOTEA_BAJO: u8 = 55;
const LLUVIA_FUERTE: f32 = 0.45;
const LLUVIA_REFUGIO: f32 = 0.6;
const REMIENDO: u8 = 30;

/// Lo que un pueblo vivió y aprendió.
#[derive(Default, Clone, Copy, Serialize, Deserialize, Debug, PartialEq)]
pub struct Lecciones {
    pub ataques: u16,
    pub muertes: u16,
    pub gotean: u16,
    pub defendidas: u16,
    pub remendadas: u16,
    /// Infectados que mató el JUGADOR delante de los vecinos de este pueblo (biblia §20: los
    /// vecinos te recuerdan por lo que hiciste).
    #[serde(default)]
    pub por_vos: u16,
}

impl Lecciones {
    /// 0..6: cuánto empuja a construir en alto y con paredes (de lo sufrido, no de un sorteo).
    pub fn defensivo(&self) -> u32 {
        (self.muertes as u32 * 2 + self.ataques as u32 / 3).min(6)
    }

    /// 0..90: probabilidad (%) de que un vecino adulto salga a pelear en vez de correr. Sube con
    /// lo que el barrio sufrió, con las que ya defendió con éxito y con los que ya están peleando.
    pub fn valentia(&self, peleando: usize) -> u32 {
        (10 + 12 * self.muertes.min(5) as u32 + 4 * self.ataques.min(8) as u32 + 3 * self.defendidas.min(5) as u32 + 14 * peleando.min(3) as u32).min(90)
    }
}

/// La calidad (0..100) con la que sale una casa de pueblo: el saber de techar del pueblo civil
/// (0..1), las goteras que ya sufrió el barrio (cada una enseña) y la suerte de esa obra.
pub fn calidad_de_obra(saber: f32, l: &Lecciones, k: u32) -> u8 {
    let ruido = (ha_shared::worldgen::hash_cell(0xCA11, k as i64, 3) % 21) as f32 - 10.0;
    let aprendido = 8.0 * l.gotean.min(6) as f32;
    (30.0 + 40.0 * saber.clamp(0.0, 1.0) + aprendido + ruido).clamp(5.0, 100.0) as u8
}

/// Lo que un vecino cuenta del barrio cuando le preguntás cómo anda todo: lo que de verdad pasó.
pub fn noticias(l: &Lecciones) -> Option<String> {
    let mut v: Vec<String> = Vec::new();
    if l.muertes > 0 {
        v.push(format!("Perdimos a {} vecino{} en las noches", l.muertes, if l.muertes == 1 { "" } else { "s" }));
    }
    if l.defendidas > 0 {
        v.push(format!("entre todos acabamos con {} infectado{}", l.defendidas, if l.defendidas == 1 { "" } else { "s" }));
    }
    if l.por_vos > 0 {
        v.push(format!("vos solo acabaste con {} acá, y eso no se olvida", l.por_vos));
    }
    if l.gotean > 0 {
        v.push(if l.remendadas >= l.gotean { "aprendimos a techar mejor después de las goteras".to_string() } else { "algunas casas gotean cuando llueve fuerte".to_string() });
    }
    if v.is_empty() {
        return None;
    }
    let mut t = v.join("; ");
    t.push('.');
    Some(t)
}

type Clave = (i32, i32);

fn clave(x: f32, z: f32) -> Clave {
    (x.round() as i32, z.round() as i32)
}

#[derive(Resource, Default)]
pub struct Memoria {
    pueblos: HashMap<Clave, Lecciones>,
    cargado: bool,
    sucio: bool,
    guardar_en: f32,
}

impl Memoria {
    pub fn de(&self, x: f32, z: f32) -> Lecciones {
        self.pueblos.get(&clave(x, z)).copied().unwrap_or_default()
    }
    fn tocar(&mut self, x: f32, z: f32) -> &mut Lecciones {
        self.sucio = true;
        self.pueblos.entry(clave(x, z)).or_default()
    }
    /// Total de lo vivido en todos los pueblos (para `consola` y QA).
    pub fn total(&self) -> Lecciones {
        self.pueblos.values().fold(Lecciones::default(), |a, b| Lecciones {
            ataques: a.ataques + b.ataques,
            muertes: a.muertes + b.muertes,
            gotean: a.gotean + b.gotean,
            defendidas: a.defendidas + b.defendidas,
            remendadas: a.remendadas + b.remendadas,
            por_vos: a.por_vos + b.por_vos,
        })
    }
}

fn ruta() -> std::path::PathBuf {
    crate::inventory_persist::persist_dir().join("barrio.json")
}

fn cargar_y_guardar(mut m: ResMut<Memoria>, time: Res<Time>) {
    if !m.cargado {
        m.cargado = true;
        if let Some(l) = std::fs::read_to_string(ruta()).ok().and_then(|s| serde_json::from_str::<Vec<(Clave, Lecciones)>>(&s).ok()) {
            m.pueblos = l.into_iter().collect();
        }
    }
    m.guardar_en += time.delta_secs();
    if m.sucio && m.guardar_en > 20.0 {
        m.sucio = false;
        m.guardar_en = 0.0;
        let v: Vec<(Clave, Lecciones)> = m.pueblos.iter().map(|(k, l)| (*k, *l)).collect();
        crate::persistent_agents::guardar_json(&ruta(), &v);
    }
}

pub struct BarrioPlugin;

impl Plugin for BarrioPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Memoria>()
            .add_systems(bevy::prelude::OnExit(crate::game_phase::GamePhase::Playing), crate::salir::reiniciar::<Memoria>)
            .add_systems(
                Update,
                (cargar_y_guardar, morder_vecinos, vecinos_reaccionan, goteras, gemidos, el_barrio_te_ve, te_reconocen).run_if(crate::game_phase::world_active),
            );
    }
}

fn azar(uid: u64, sal: u64) -> u32 {
    (ha_shared::sims::hash(uid ^ sal.wrapping_mul(0x9E37_79B9_7F4A_7C15)) >> 20) as u32 % 100
}

/// Los infectados muerden a los vecinos que tienen más cerca (y no a vos, si estás más cerca de ellos).
#[allow(clippy::too_many_arguments)]
fn morder_vecinos(
    mut commands: Commands,
    mut infectados: Query<&mut Infectado>,
    mut npcs: Query<(Entity, &mut Npc)>,
    player: Query<&Transform, With<Player>>,
    mut soc: ResMut<Society>,
    mut civ: ResMut<Civ>,
    mut memoria: ResMut<Memoria>,
    mut speech: ResMut<SpeechQueue>,
    mut sfx: EventWriter<crate::audio::PlaySfxAt>,
) {
    let (px, pz) = player.get_single().map(|t| (t.translation.x, -t.translation.y)).unwrap_or((1.0e9, 1.0e9));
    let mut muertos: Vec<Entity> = Vec::new();
    for mut i in &mut infectados {
        if i.ataque_cd > 0.0 {
            continue;
        }
        if ((px - i.x).powi(2) + (pz - i.z).powi(2)).sqrt() < ATAQUE_RADIO {
            continue; // te muerde a vos
        }
        let mut mejor: Option<(Entity, f32)> = None;
        for (e, n) in npcs.iter() {
            if muertos.contains(&e) {
                continue;
            }
            let d = ((n.x - i.x).powi(2) + (n.z - i.z).powi(2)).sqrt();
            if d < ATAQUE_RADIO && mejor.map(|m| d < m.1).unwrap_or(true) {
                mejor = Some((e, d));
            }
        }
        let Some((e, _)) = mejor else { continue };
        i.ataque_cd = ATAQUE_CD;
        sfx.send(crate::audio::PlaySfxAt(crate::audio::Sfx::Bite, crate::world2d::render_xy(i.x, i.z)));
        let Ok((_, mut n)) = npcs.get_mut(e) else { continue };
        n.health -= MORDIDA;
        let (tx, tz) = (n.town_x, n.town_z);
        memoria.tocar(tx, tz).ataques += 1;
        if n.health <= 0.0 {
            muertos.push(e);
        } else {
            say_from(&mut speech, e, n.x, n.z, "¡Aaay! ¡Me mordió!", Color::srgb(1.0, 0.55, 0.5), true);
        }
    }
    for e in muertos {
        let Ok((_, n)) = npcs.get(e) else { continue };
        let (x, z, uid, family, tx, tz) = (n.x, n.z, n.uid, n.family, n.town_x, n.town_z);
        let nombre = npc_name(uid, family);
        civ.muerte_de_npc(uid, x, z);
        despedir_npc(&mut commands, &mut soc, e, &n);
        memoria.tocar(tx, tz).muertes += 1;
        crate::causal::registrar("muerte", format!("{nombre} murió a manos de los infectados en ({x:.0},{z:.0})"));
        say(&mut speech, x, z, format!("¡Mataron a {nombre}!"), Color::srgb(1.0, 0.45, 0.4));
        // Los testigos lo viven: quedan sin seguridad, y se acuerdan.
        for (we, mut w) in &mut npcs {
            if we != e && (w.x - x).powi(2) + (w.z - z).powi(2) < TESTIGOS * TESTIGOS {
                w.vida.m.set(Motivo::Seguridad, 0.0);
                push_history(&mut w.history, &format!("Mataron a {nombre}."));
            }
        }
    }
}

/// Testigos de lo que hiciste: cada infectado que matás con vecinos cerca queda en la memoria de SU pueblo,
/// en la historia de cada testigo y en la fama de su familia (biblia §20, ítem 2).
fn el_barrio_te_ve(
    mut bajas: ResMut<crate::infectados::Bajas>,
    mut npcs: Query<(Entity, &mut Npc)>,
    mut soc: ResMut<Society>,
    mut memoria: ResMut<Memoria>,
    mut speech: ResMut<SpeechQueue>,
) {
    if bajas.recientes.is_empty() {
        return;
    }
    for (x, z) in std::mem::take(&mut bajas.recientes) {
        let mut pueblo: Option<(f32, f32)> = None;
        let mut grito: Option<(Entity, f32, f32)> = None;
        for (e, mut n) in &mut npcs {
            if (n.x - x).powi(2) + (n.z - z).powi(2) >= TESTIGOS * TESTIGOS {
                continue;
            }
            pueblo.get_or_insert((n.town_x, n.town_z));
            push_history(&mut n.history, "El forastero mató a un infectado delante mío.");
            soc.adjust_capped(n.family, TE_VEN_REP, TE_VEN_TOPE);
            grito.get_or_insert((e, n.x, n.z));
        }
        let Some((tx, tz)) = pueblo else { continue };
        let l = memoria.tocar(tx, tz);
        l.por_vos = l.por_vos.saturating_add(1);
        let n = l.por_vos;
        if let Some((e, nx, nz)) = grito {
            // Lo celebran la primera vez y en los números redondos, no a cada golpe.
            if let Some(t) = festejo(n) {
                say_from(&mut speech, e, nx, nz, t, Color::srgb(0.75, 1.0, 0.7), false);
            }
        }
        crate::causal::registrar("testigos", format!("el barrio de ({tx:.0},{tz:.0}) te vio matar al infectado n.º {n}"));
    }
}

const TE_VEN_REP: f32 = 0.04;
const TE_VEN_TOPE: f32 = 0.6;

/// Qué grita un testigo cuando el forastero lleva `n` infectados muertos en su barrio.
pub fn festejo(n: u16) -> Option<String> {
    match n {
        1 => Some("¡Bien ahí, forastero!".into()),
        5 => Some("¡Ya van cinco! ¡Ése está de nuestro lado!".into()),
        n if n >= 10 && n % 10 == 0 => Some(format!("¡{n}! ¡El forastero es una leyenda!")),
        _ => None,
    }
}

/// Lo que un vecino te dice al abrir su menú si su barrio te vio pelear por él. Una vez por vecino y partida.
pub fn reconocimiento(por_vos: u16, uid: u64) -> Option<String> {
    let v = [
        format!("¡Vos sos el que mató {por_vos} infectados en el barrio! Acá siempre vas a tener un plato."),
        format!("Te vi pelear. {por_vos} menos que nos muerdan de noche. Gracias."),
        format!("Mi familia habla de vos: {por_vos} infectados, dicen. ¿Es cierto?"),
    ];
    match por_vos {
        0 => None,
        1 | 2 => Some("Me contaron que nos diste una mano con los infectados. Gracias, forastero.".into()),
        _ => Some(v[(uid % 3) as usize].clone()),
    }
}

fn te_reconocen(
    menu: Res<crate::npc_menu::NpcMenu>,
    mut npcs: Query<(Entity, &mut Npc)>,
    memoria: Res<Memoria>,
    mut speech: ResMut<SpeechQueue>,
    mut ultimo: Local<Option<u64>>,
    mut ya: Local<HashSet<u64>>,
) {
    if menu.open_uid == *ultimo {
        return;
    }
    *ultimo = menu.open_uid;
    let Some(uid) = menu.open_uid else { return };
    if ya.contains(&uid) {
        return;
    }
    let Some((e, mut n)) = npcs.iter_mut().find(|(_, n)| n.uid == uid) else { return };
    let Some(t) = reconocimiento(memoria.de(n.town_x, n.town_z).por_vos, uid) else { return };
    ya.insert(uid);
    push_history(&mut n.history, &t);
    say_from(&mut speech, e, n.x, n.z, t, Color::srgb(0.75, 1.0, 0.7), true);
}

/// Los infectados gimen: cada tanto, el más cercano a vos (a menos de 30 celdas) suelta un gemido en su lugar.
/// Es lo que avisa que algo se acerca ANTES de verlo.
fn gemidos(
    time: Res<Time>,
    infectados: Query<&Infectado>,
    player: Query<&Transform, With<Player>>,
    mut sfx: EventWriter<crate::audio::PlaySfxAt>,
    mut resto: Local<f32>,
) {
    *resto -= time.delta_secs();
    if *resto > 0.0 {
        return;
    }
    let Ok(pt) = player.get_single() else { return };
    let (px, pz) = (pt.translation.x, -pt.translation.y);
    let cerca = infectados
        .iter()
        .map(|i| (((i.x - px).powi(2) + (i.z - pz).powi(2)).sqrt(), i.x, i.z))
        .filter(|(d, _, _)| *d < 30.0)
        .min_by(|a, b| a.0.total_cmp(&b.0));
    if let Some((_, x, z)) = cerca {
        sfx.send(crate::audio::PlaySfxAt(crate::audio::Sfx::Groan, crate::world2d::render_xy(x, z)));
        *resto = 6.0 + (x.abs() as u32 % 7) as f32;
    } else {
        *resto = 1.5;
    }
}

/// Cada vecino decide, ante un infectado cerca, si pelea o corre; los que pelean lo golpean.
#[allow(clippy::too_many_arguments)]
fn vecinos_reaccionan(
    mut commands: Commands,
    time: Res<Time>,
    mut infectados: Query<(Entity, &mut Infectado)>,
    mut npcs: Query<(Entity, &mut Npc)>,
    mut memoria: ResMut<Memoria>,
    mut speech: ResMut<SpeechQueue>,
    mut acc: Local<f32>,
    mut dicho: Local<HashMap<u64, f32>>,
) {
    *acc += time.delta_secs();
    if *acc < PASADA_S {
        return;
    }
    *acc = 0.0;
    let ahora = time.elapsed_secs();
    let infs: Vec<(Entity, f32, f32)> = infectados.iter().map(|(e, i)| (e, i.x, i.z)).collect();
    if infs.is_empty() {
        return;
    }
    // Quién ya está peleando contra cada infectado (para que el ejemplo contagie).
    let peleando: Vec<usize> = infs
        .iter()
        .map(|(_, ix, iz)| npcs.iter().filter(|(_, n)| n.vida.actividad() == Actividad::Atacar && (n.x - ix).powi(2) + (n.z - iz).powi(2) < ALERTA * ALERTA).count())
        .collect();
    let mut golpes: Vec<(Entity, f32)> = Vec::new();
    for (e, mut n) in &mut npcs {
        let mut cerca: Option<(usize, f32)> = None;
        for (k, (_, ix, iz)) in infs.iter().enumerate() {
            let d = ((n.x - ix).powi(2) + (n.z - iz).powi(2)).sqrt();
            if d < ALERTA && cerca.map(|c| d < c.1).unwrap_or(true) {
                cerca = Some((k, d));
            }
        }
        let Some((k, d)) = cerca else { continue };
        let (_, ix, iz) = infs[k];
        let l = memoria.de(n.town_x, n.town_z);
        let adulto = !crate::society::es_nino(n.age, n.padre, n.madre);
        let valiente = adulto && n.health > 0.5 && n.vida.pers.moral > -0.2 && azar(n.uid, 0xB4B) < l.valentia(peleando[k]);
        let act = n.vida.actividad();
        let callado = dicho.get(&n.uid).copied().unwrap_or(0.0) > ahora;
        if valiente {
            if act == Actividad::Atacar {
                n.vida.apuntar(ix, iz);
            } else {
                n.vida.imponer(&Anuncio::nuevo(Actividad::Atacar, ix, iz).uso(1.2).dur(12.0));
                if !callado {
                    let frases = if l.muertes > 0 {
                        ["¡Por los que perdimos!", "¡Ya basta, defendamos el barrio!", "¡No pasan de acá!"]
                    } else {
                        ["¡A ellos!", "¡Defendamos el barrio!", "¡Vengan, que somos más!"]
                    };
                    say_from(&mut speech, e, n.x, n.z, frases[(n.uid % 3) as usize], Color::srgb(1.0, 0.8, 0.4), false);
                    dicho.insert(n.uid, ahora + 25.0);
                }
            }
            if d < ALCANCE_VECINO {
                golpes.push((infs[k].0, DANO_VECINO));
            }
        } else if act != Actividad::Huir && act != Actividad::Dormir {
            let (hx, hz) = n.vivienda.casa.map(|h| h.umbral()).unwrap_or((n.home_x, n.home_z));
            n.vida.imponer(&Anuncio::nuevo(Actividad::Huir, hx, hz).uso(1.5).dur(15.0));
            if !callado {
                say_from(&mut speech, e, n.x, n.z, "¡Infectados! ¡A casa!", Color::srgb(1.0, 0.65, 0.5), false);
                dicho.insert(n.uid, ahora + 25.0);
            }
        }
    }
    let mut caidos: HashSet<Entity> = HashSet::new();
    for (ie, dano) in golpes {
        if caidos.contains(&ie) {
            continue;
        }
        let Ok((_, mut i)) = infectados.get_mut(ie) else { continue };
        i.vida -= dano;
        i.destello = 0.25;
        if i.vida <= 0.0 {
            caidos.insert(ie);
            let (x, z) = (i.x, i.z);
            commands.entity(ie).despawn_recursive();
            // La victoria es del pueblo más cercano al infectado.
            if let Some((_, n)) = npcs.iter().min_by(|a, b| ((a.1.x - x).powi(2) + (a.1.z - z).powi(2)).total_cmp(&((b.1.x - x).powi(2) + (b.1.z - z).powi(2)))) {
                let (tx, tz) = (n.town_x, n.town_z);
                memoria.tocar(tx, tz).defendidas += 1;
            }
            crate::causal::registrar("defensa", format!("los vecinos acabaron con un infectado en ({x:.0},{z:.0})"));
            say(&mut speech, x, z, "¡Uno menos!", Color::srgb(1.0, 0.9, 0.5));
        }
    }
}

/// Lluvia fuerte: algunos se refugian en su casa; las casas flojas gotean; al parar, las remiendan.
#[allow(clippy::too_many_arguments)]
fn goteras(
    time: Res<Time>,
    ctx: Res<crate::scene::WorldContextRuntime>,
    planet: Res<ActivePlanet>,
    weather: Res<Weather>,
    mut eds: ResMut<Edificios>,
    mut npcs: Query<(Entity, &mut Npc)>,
    mut memoria: ResMut<Memoria>,
    mut speech: ResMut<SpeechQueue>,
    mut acc: Local<f32>,
    mut mojadas: Local<HashMap<usize, Clave>>,
    mut refugiados: Local<HashSet<u64>>,
    mut llovia: Local<bool>,
) {
    *acc += time.delta_secs();
    if *acc < 2.0 {
        return;
    }
    *acc = 0.0;
    let lluvia = if weather.snowing { 0.0 } else { weather.rain };
    let body = active_surface_body(&ctx, &planet) as u8;
    if lluvia >= LLUVIA_FUERTE {
        *llovia = true;
        for (e, mut n) in &mut npcs {
            let uid = n.uid;
            let Some(casa) = n.vivienda.casa else { continue };
            let adentro = casa.dentro(n.x, n.z);
            if lluvia >= LLUVIA_REFUGIO && !adentro && !refugiados.contains(&uid) && azar(uid, 0xA1) < 45 {
                let ocupado = matches!(n.vida.actividad(), Actividad::Atacar | Actividad::Huir | Actividad::Detener | Actividad::Dormir);
                if !ocupado {
                    refugiados.insert(uid);
                    let (hx, hz) = casa.umbral();
                    n.vida.imponer(&Anuncio::nuevo(Actividad::Descansar, hx, hz).uso(1.5).dur(30.0));
                    say_from(&mut speech, e, n.x, n.z, "¡Llueve fuerte! A casa.", Color::srgb(0.75, 0.85, 1.0), false);
                }
            }
            if !adentro {
                continue;
            }
            let Some(idx) = eds.en(body, 0, n.x.floor() as i64, n.z.floor() as i64) else { continue };
            if eds.list[idx].calidad >= GOTEA_BAJO || mojadas.contains_key(&idx) {
                continue;
            }
            mojadas.insert(idx, clave(n.town_x, n.town_z));
            memoria.tocar(n.town_x, n.town_z).gotean += 1;
            let nombre = npc_name(uid, n.family);
            say_from(&mut speech, e, n.x, n.z, "¡Se me mojó todo! Esta casa gotea.", Color::srgb(0.7, 0.85, 1.0), true);
            push_history(&mut n.history, "Esta casa gotea: se me mojó todo.");
            crate::causal::registrar("gotera", format!("{nombre}: la casa (calidad {}) gotea con la lluvia", eds.list[idx].calidad));
        }
    } else if lluvia < 0.1 && *llovia {
        *llovia = false;
        refugiados.clear();
        // Paró: remiendan lo que goteó.
        let hechos: Vec<(usize, Clave)> = mojadas.drain().collect();
        for (idx, k) in hechos {
            if let Some(ed) = eds.list.get_mut(idx) {
                ed.calidad = ed.calidad.saturating_add(REMIENDO).min(100);
                memoria.sucio = true;
                memoria.pueblos.entry(k).or_default().remendadas += 1;
                crate::causal::registrar("gotera", format!("remiendan un techo: ahora calidad {}", ed.calidad));
            }
        }
        // Una voz del barrio lo dice, si hay alguien cerca.
        if let Some((e, n)) = npcs.iter().next() {
            say_from(&mut speech, e, n.x, n.z, "Hay que techar mejor. Remendamos lo que goteó.", Color::srgb(0.9, 0.95, 0.8), false);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn un_barrio_golpeado_se_vuelve_mas_valiente_y_construye_defensa() {
        let tranquilo = Lecciones::default();
        let golpeado = Lecciones { ataques: 8, muertes: 3, defendidas: 2, ..Default::default() };
        assert!(golpeado.valentia(0) > tranquilo.valentia(0) + 40, "{} vs {}", golpeado.valentia(0), tranquilo.valentia(0));
        assert!(golpeado.defensivo() > tranquilo.defensivo());
        assert!(tranquilo.valentia(0) < 20, "un barrio que nunca sufrió casi no pelea");
        assert!(tranquilo.valentia(3) > tranquilo.valentia(0) + 30, "ver pelear a otros contagia");
        assert!(golpeado.valentia(9) <= 90);
    }

    #[test]
    fn el_pueblo_que_sabe_techar_hace_casas_firmes_y_el_que_no_aprende_a_las_malas() {
        let nada = Lecciones::default();
        let ignorante: Vec<u8> = (0..60).map(|k| calidad_de_obra(0.0, &nada, k)).collect();
        let sabio: Vec<u8> = (0..60).map(|k| calidad_de_obra(1.0, &nada, k)).collect();
        assert!(ignorante.iter().all(|c| *c < GOTEA_BAJO), "sin saber, todas gotean: {ignorante:?}");
        assert!(sabio.iter().all(|c| *c >= GOTEA_BAJO), "con saber, ninguna gotea: {sabio:?}");
        // Cada gotera enseña: tras unas cuantas, hasta el que no sabe hace casas que aguantan.
        let escarmentado = Lecciones { gotean: 5, ..Default::default() };
        let mejoran = (0..60).filter(|k| calidad_de_obra(0.0, &escarmentado, *k) >= GOTEA_BAJO).count();
        assert!(mejoran > 30, "sólo {mejoran}/60 aprendieron");
    }

    #[test]
    fn las_noticias_cuentan_lo_que_paso() {
        assert_eq!(noticias(&Lecciones::default()), None);
        let t = noticias(&Lecciones { muertes: 2, defendidas: 1, gotean: 1, ..Default::default() }).unwrap();
        assert!(t.contains("2 vecinos") && t.contains("1 infectado") && t.contains("gotean"), "{t}");
    }

    #[test]
    fn el_barrio_te_recuerda_y_no_repite_el_festejo() {
        assert!(festejo(1).is_some());
        assert!(festejo(2).is_none() && festejo(7).is_none());
        assert!(festejo(5).is_some() && festejo(30).is_some());
        assert_eq!(reconocimiento(0, 1), None);
        assert!(reconocimiento(4, 9).unwrap().contains('4'));
        let t = noticias(&Lecciones { por_vos: 3, ..Default::default() }).unwrap();
        assert!(t.contains("vos solo acabaste con 3"));
        // Las partidas viejas (sin `por_vos` en barrio.json) siguen cargando.
        let l: Lecciones = serde_json::from_str(r#"{"ataques":1,"muertes":0,"gotean":0,"defendidas":0,"remendadas":0}"#).unwrap();
        assert_eq!(l.por_vos, 0);
    }

    #[test]
    fn la_memoria_es_por_pueblo() {
        let mut m = Memoria::default();
        m.tocar(100.0, 200.0).muertes += 2;
        assert_eq!(m.de(100.2, 199.8).muertes, 2);
        assert_eq!(m.de(900.0, 900.0).muertes, 0);
        assert_eq!(m.total().muertes, 2);
    }
}
