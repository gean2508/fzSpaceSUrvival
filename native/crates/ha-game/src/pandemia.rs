//! **La pandemia: un virus que se descubre, se propaga y se puede detener.**
//!
//! Sin rondas (es un mundo abierto): la presión sube con los días mientras no se cure.
//!
//! ## La historia (lore) y los pasos de la cura
//! 0. **Rumor.** Los primeros días no hay infectados. Desde el día 2, de noche, aparece el
//!    **paciente cero**, un infectado rojizo que merodea a 60-90 m: es el origen.
//! 1. **El virus se descubre.** Al abatir al paciente cero suelta la **muestra del virus**
//!    (`virus_sample`, queda en la mochila). A partir de ahí hay pandemia.
//! 2. **Analizarla.** Llevala a una **comisaría** (el laboratorio de campo, `city::comisaria_cercana`)
//!    y quedate 4 s a menos de 14 m: sale la pista de que el virus nace de **tres alfas**
//!    y aparecen en tres focos.
//! 3. **Los tres alfas** (3× de vida) caen: el virus pierde su fuente.
//! 4. **El antídoto.** Volvé a la comisaría con los alfas abatidos: te dan el `antidoto`.
//! 5. **Curar el foco principal.** Llevá el antídoto al lugar del paciente cero y quedate 5 s: la
//!    pandemia termina, los infectados se desvanecen. Final secreto: si cortás el brote antes del
//!    día 7 de pandemia, además llevás un tesoro de gemas.
//!
//! ## Cómo se propaga
//! El avance (0-100) sube cada día. Cada día nace un **foco** nuevo cerca del anterior (se corre
//! 150-300 m), y los infectados salen alrededor de los focos (`infectados::aparecer`). En cada foco
//! de la pandemia avanzada vive un **jefe** (tipo RPG): mucha vida, que **crece con los jugadores
//! conectados** (coop: solo no podés con él), y suelta botín raro. Cerca de un foco el terreno se
//! ve enfermo: los árboles y las plantas se tiñen y se dibuja una mancha en el suelo.
//!
//! Cupo (checklist §2): cupo de infectados `2 + avance/8` (tope 12 + 2 por jugador), un solo
//! sistema de teñido a 1 Hz, y el estado entero cabe en un `Resource` guardado en la partida.

use bevy::prelude::*;
use serde::{Deserialize, Serialize};

use crate::game_phase::GamePhase;
use crate::inventory_persist::{persist_dir, InventoryPersistState, PlayerInventory};
use crate::infectados::{Bajas, Infectado};
use crate::scene::WorldContextRuntime;
use crate::sim::Player;
use crate::sky2d::DayClock;
use crate::world2d::{active_surface_body, sort_z, ActivePlanet, SurfaceDecor};

const AVANCE_POR_DIA: f32 = 10.0;
const RADIO_ENFERMO: f32 = 55.0;
const SEGUNDOS_ANALISIS: f32 = 4.0;
const SEGUNDOS_CURA: f32 = 5.0;
const RADIO_LAB: f32 = 14.0;
const TOPE_CUPO: usize = 12;

/// Etapas de la historia. Guardadas como número en la partida.
pub mod etapa {
    pub const RUMOR: u8 = 0;
    pub const BROTE: u8 = 1;
    pub const MUESTRA: u8 = 2;
    pub const ANALIZADA: u8 = 3;
    pub const ALFAS_CAIDOS: u8 = 4;
    pub const ANTIDOTO: u8 = 5;
    pub const CURADA: u8 = 6;
}

#[derive(Resource, Default, Serialize, Deserialize, Clone, Debug)]
#[serde(default)]
pub struct Pandemia {
    pub etapa: u8,
    pub dia_inicio: f32,
    /// 0-100.
    pub avance: f32,
    pub focos: Vec<(f32, f32)>,
    pub alfas_muertos: u32,
    pub alfas_sembrados: bool,
    /// El día en que creció el avance por última vez.
    ultimo_dia: f32,
    #[serde(skip)]
    pub aviso: Option<(String, f32)>,
    #[serde(skip)]
    espera_lab: f32,
    #[serde(skip)]
    espera_cura: f32,
    #[serde(skip)]
    sucio: bool,
    #[serde(skip)]
    ultimo_guardado: f32,
    #[serde(skip)]
    sondeo_lab: f32,
    #[serde(skip)]
    lab: Option<(i64, i64)>,
    #[serde(skip)]
    jefes_sembrados: u32,
}

impl Pandemia {
    /// Hay infectados sueltos por el mundo: desde que el virus se descubre hasta que se cura.
    pub fn activa(&self) -> bool {
        self.etapa >= etapa::MUESTRA && self.etapa < etapa::CURADA || (self.etapa == etapa::BROTE && self.avance > 0.0)
    }

    /// Cuántos infectados sueltos aguanta el mundo (más con más jugadores).
    pub fn cupo(&self, otros_jugadores: usize) -> usize {
        let base = 2 + (self.avance / 8.0) as usize;
        base.min(TOPE_CUPO) + otros_jugadores * 2
    }

    /// El objetivo en una línea, para el HUD.
    pub fn objetivo(&self) -> &'static str {
        match self.etapa {
            etapa::RUMOR => "",
            etapa::BROTE => "Hay un infectado raro de noche: abatilo (paciente cero)",
            etapa::MUESTRA => "Llevá la muestra del virus a una comisaría y analizala",
            etapa::ANALIZADA => "Abatí a los 3 alfas rojizos de los focos",
            etapa::ALFAS_CAIDOS => "Volvé a la comisaría por el antídoto",
            etapa::ANTIDOTO => "Llevá el antídoto al foco principal (donde cayó el paciente cero)",
            _ => "",
        }
    }
}

#[derive(Component)]
struct Mancha;
#[derive(Component)]
pub struct Jefe;
#[derive(Component)]
struct PandemiaHud;

pub struct PandemiaPlugin;

impl Plugin for PandemiaPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Pandemia>()
            .add_systems(OnEnter(GamePhase::Loading), cargar)
            .add_systems(
                Update,
                (avanzar, laboratorio, curar, manchas, tenir_el_mundo, hud, guardar, volcar_ui_esquina)
                    .chain()
                    .run_if(in_state(GamePhase::Playing)),
            )
            .add_systems(OnExit(GamePhase::Playing), limpiar);
    }
}

fn ruta() -> std::path::PathBuf {
    persist_dir().join("pandemia.json")
}

fn cargar(mut p: ResMut<Pandemia>) {
    *p = std::fs::read_to_string(ruta()).ok().and_then(|raw| serde_json::from_str(&raw).ok()).unwrap_or_default();
}

fn guardar(time: Res<Time>, mut p: ResMut<Pandemia>) {
    if !p.sucio || time.elapsed_secs() - p.ultimo_guardado < 3.0 {
        return;
    }
    p.sucio = false;
    p.ultimo_guardado = time.elapsed_secs();
    if let Some(dir) = ruta().parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    if let Ok(json) = serde_json::to_string(&*p) {
        let _ = std::fs::write(ruta(), json);
    }
}

fn limpiar(mut commands: Commands, q: Query<Entity, Or<(With<Mancha>, With<PandemiaHud>)>>, mut p: ResMut<Pandemia>) {
    for e in &q {
        commands.entity(e).despawn_recursive();
    }
    *p = Pandemia::default();
}

fn hash(a: u32) -> u32 {
    let mut x = a.wrapping_mul(0x9E37_79B9) ^ 0x85EB_CA6B;
    x ^= x >> 15;
    x = x.wrapping_mul(0x2C1B_3C6D);
    x ^ (x >> 12)
}

/// Una orden del jefe/foco para un infectado nuevo: la cola de spawn la hace `avanzar`.
#[allow(clippy::too_many_arguments)]
fn avanzar(
    mut commands: Commands,
    time: Res<Time>,
    day: Res<DayClock>,
    art: Res<crate::art2d::ArtLibrary>,
    (ctx, planet): (Res<WorldContextRuntime>, Res<ActivePlanet>),
    obs: crate::obstaculos::Obstaculos,
    multi: Res<crate::multijugador::Multi>,
    mut p: ResMut<Pandemia>,
    mut bajas: ResMut<Bajas>,
    mut inv: ResMut<PlayerInventory>,
    mut persist: ResMut<InventoryPersistState>,
    player: Query<&Transform, With<Player>>,
    infectados: Query<&Infectado>,
) {
    let dt = time.delta_secs().min(0.1);
    if let Some((_, t)) = p.aviso.as_mut() {
        *t -= dt;
    }
    if p.aviso.as_ref().is_some_and(|(_, t)| *t <= 0.0) {
        p.aviso = None;
    }
    bajas.avance = p.avance;
    let Ok(pt) = player.get_single() else { return };
    if (time.elapsed_secs() as u32) % 10 == 0 && (time.elapsed_secs().fract() < dt) {
        eprintln!("ha-pandemia: etapa={} avance={:.0} focos={} infectados={} cupo={}", p.etapa, p.avance, p.focos.len(), infectados.iter().filter(|i| !i.guardia).count(), p.cupo(0));
    }
    let (px, pz) = (pt.translation.x, -pt.translation.y);
    // QA: `HA_PANDEMIA=1` arranca con el virus ya descubierto, 50 % de avance y dos focos cerca.
    if p.etapa == etapa::RUMOR && crate::flags::var("HA_PANDEMIA").is_ok() {
        p.etapa = etapa::MUESTRA;
        p.avance = 50.0;
        p.dia_inicio = day.day_index();
        p.ultimo_dia = day.day_index();
        p.focos = vec![(px + 12.0, pz + 4.0), (px - 40.0, pz + 35.0)];
        p.sucio = true;
    }
    let dia = day.day_index();
    let noche = day.daylight() <= 0.30;
    let body = active_surface_body(&ctx, &planet);
    let s = time.elapsed().as_micros() as u64;
    let h = |k: u64| {
        let mut x = s ^ k.wrapping_mul(0x9E37_79B9_7F4A_7C15);
        x ^= x >> 31;
        x = x.wrapping_mul(0xBF58_476D_1CE4_E5B9);
        ((x >> 40) as f32) / (1u64 << 24) as f32
    };
    let en_superficie = matches!(ctx.active, ha_shared::WorldContext::Overworld) && obs.capa() == 0;

    // 0 → 1: el primer brote. Desde el día 2, de noche, aparece el paciente cero.
    if p.etapa == etapa::RUMOR && dia >= 2.0 && noche && en_superficie {
        let ang = h(1) * std::f32::consts::TAU;
        let d = 60.0 + h(2) * 30.0;
        let (x, z) = (px + ang.cos() * d, pz + ang.sin() * d);
        if obs.libre(body, x, z, 0.4) {
            crate::infectados::crear_infectado_con_rol(&mut commands, &art, x, z, crate::oleadas::Clase::Tanque, (h(3), h(4), h(5)), 1);
            p.focos.push((x, z));
            p.etapa = etapa::BROTE;
            p.dia_inicio = dia;
            p.ultimo_dia = dia;
            p.avance = 1.0;
            p.aviso = Some(("Algo anda mal: un infectado raro merodea de noche".into(), 6.0));
            p.sucio = true;
        }
    }

    // 1 → 2: el paciente cero cae y suelta la muestra (queda en el cadáver; la mochila la recoge al abrirlo).
    if bajas.paciente_cero_muerto {
        bajas.paciente_cero_muerto = false;
        if p.etapa == etapa::BROTE {
            p.etapa = etapa::MUESTRA;
            p.aviso = Some(("El paciente cero soltó una muestra del virus: recogela del cuerpo".into(), 6.0));
            p.sucio = true;
        }
    }
    // Si ya hay muestra en la mochila y seguía en BROTE (la recogió por otro lado), también avanza.
    if p.etapa == etapa::BROTE && inv.qty("virus_sample") > 0 {
        p.etapa = etapa::MUESTRA;
        p.sucio = true;
    }

    if p.etapa >= etapa::BROTE && p.etapa < etapa::CURADA {
        // La pandemia avanza un poco cada día y nace un foco nuevo cerca del último.
        if dia > p.ultimo_dia {
            p.ultimo_dia = dia;
            p.avance = (p.avance + AVANCE_POR_DIA).min(100.0);
            if let Some(&(lx, lz)) = p.focos.last() {
                let ang = h(7) * std::f32::consts::TAU;
                let d = 150.0 + h(8) * 150.0;
                if p.focos.len() < 8 {
                    p.focos.push((lx + ang.cos() * d, lz + ang.sin() * d));
                }
            }
            p.aviso = Some(("La infección avanza: hay un foco nuevo".into(), 5.0));
            p.sucio = true;
        }
        // Un jefe por foco, cuando el avance lo permite. Sale al acercarte (no gasta nada lejos).
        let cerca: Vec<(f32, f32)> = p.focos.iter().copied().skip(1).filter(|&(fx, fz)| (fx - px).hypot(fz - pz) < 90.0).collect();
        let jefes_vivos = infectados.iter().filter(|i| i.rol == 3).count() as u32;
        if p.avance >= 20.0 && jefes_vivos < 2 && p.jefes_sembrados < p.focos.len() as u32 {
            if let Some(&(fx, fz)) = cerca.first() {
                // Coop: más vida con más jugadores (se aplica en `crear_infectado_con_rol` por rol 3).
                let n = 1 + multi.remotos.len() as u32;
                crate::infectados::crear_jefe(&mut commands, &art, fx, fz, n, (h(11), h(12), h(13)));
                p.jefes_sembrados += 1;
                p.aviso = Some(("Un jefe infectado custodia este foco: mejor ir acompañado".into(), 5.0));
            }
        }
        // 3 → alfas: se siembran en tres focos al analizar (ver `laboratorio`).
        if bajas.alfas_muertos > 0 {
            p.alfas_muertos += bajas.alfas_muertos;
            bajas.alfas_muertos = 0;
            p.sucio = true;
            if p.etapa == etapa::ANALIZADA && p.alfas_muertos >= 3 {
                p.etapa = etapa::ALFAS_CAIDOS;
                p.aviso = Some(("Los tres alfas cayeron: volvé a la comisaría por el antídoto".into(), 6.0));
            } else if p.etapa == etapa::ANALIZADA {
                p.aviso = Some((format!("Alfa abatido ({}/3)", p.alfas_muertos), 4.0));
            }
        }
    }
    let _ = (&mut inv, &mut persist);
}

/// La comisaría hace de laboratorio: analiza la muestra y entrega el antídoto.
#[allow(clippy::too_many_arguments)]
fn laboratorio(
    mut commands: Commands,
    time: Res<Time>,
    art: Res<crate::art2d::ArtLibrary>,
    (ctx, planet): (Res<WorldContextRuntime>, Res<ActivePlanet>),
    mut p: ResMut<Pandemia>,
    mut inv: ResMut<PlayerInventory>,
    mut persist: ResMut<InventoryPersistState>,
    player: Query<(&Transform, &crate::sim::SimState), With<Player>>,
) {
    let dt = time.delta_secs().min(0.1);
    let Ok((pt, sim)) = player.get_single() else { return };
    let (px, pz) = (pt.translation.x, -pt.translation.y);
    let body = active_surface_body(&ctx, &planet);
    p.sondeo_lab -= dt;
    if p.sondeo_lab <= 0.0 {
        p.sondeo_lab = 4.0;
        p.lab = ha_shared::city::comisaria_cercana(body, px.floor() as i64, pz.floor() as i64, 60);
    }
    let Some((lx, lz)) = p.lab else {
        p.espera_lab = 0.0;
        return;
    };
    let cerca = (lx as f32 - px).hypot(lz as f32 - pz) < RADIO_LAB;
    let quieto = (sim.body.vx * sim.body.vx + sim.body.vz * sim.body.vz).sqrt() < 1.0;
    let toca = match p.etapa {
        etapa::MUESTRA => inv.qty("virus_sample") > 0,
        etapa::ALFAS_CAIDOS => true,
        _ => false,
    };
    if !(cerca && quieto && toca) {
        p.espera_lab = 0.0;
        return;
    }
    p.espera_lab += dt;
    if p.espera_lab < SEGUNDOS_ANALISIS {
        return;
    }
    p.espera_lab = 0.0;
    if p.etapa == etapa::MUESTRA {
        inv.remove("virus_sample", 1);
        persist.mark_dirty();
        p.etapa = etapa::ANALIZADA;
        // Los tres alfas nacen en tres focos (se crean focos si faltan).
        let mut i = 0u32;
        while p.focos.len() < 4 {
            let (bx, bz) = p.focos.last().copied().unwrap_or((px, pz));
            let a = (hash(i.wrapping_add(p.focos.len() as u32 * 31)) % 360) as f32 * 0.01745;
            p.focos.push((bx + a.cos() * 220.0, bz + a.sin() * 220.0));
            i += 1;
        }
        if !p.alfas_sembrados {
            p.alfas_sembrados = true;
            let objetivos: Vec<(f32, f32)> = p.focos.iter().copied().skip(1).take(3).collect();
            for (k, (fx, fz)) in objetivos.into_iter().enumerate() {
                crate::infectados::crear_infectado_con_rol(&mut commands, &art, fx, fz, crate::oleadas::Clase::Tanque, (0.2 + k as f32 * 0.2, 0.5, 0.5), 2);
            }
        }
        p.aviso = Some(("Análisis listo: el virus nace de 3 alfas, marcados en los focos".into(), 7.0));
    } else if p.etapa == etapa::ALFAS_CAIDOS {
        inv.add("antidoto", 1);
        persist.mark_dirty();
        p.etapa = etapa::ANTIDOTO;
        p.aviso = Some(("Antídoto listo: llevalo al foco principal".into(), 7.0));
    }
    p.sucio = true;
}

/// El antídoto en el foco principal termina la pandemia.
#[allow(clippy::too_many_arguments)]
fn curar(
    mut commands: Commands,
    time: Res<Time>,
    mut p: ResMut<Pandemia>,
    mut inv: ResMut<PlayerInventory>,
    mut persist: ResMut<InventoryPersistState>,
    player: Query<(&Transform, &crate::sim::SimState), With<Player>>,
    infectados: Query<(Entity, &Infectado)>,
    manchas: Query<Entity, With<Mancha>>,
    mut sfx: EventWriter<crate::audio::PlaySfxAt>,
    day: Res<DayClock>,
) {
    if p.etapa != etapa::ANTIDOTO || inv.qty("antidoto") == 0 {
        p.espera_cura = 0.0;
        return;
    }
    let Ok((pt, sim)) = player.get_single() else { return };
    let (px, pz) = (pt.translation.x, -pt.translation.y);
    let Some(&(fx, fz)) = p.focos.first() else { return };
    let quieto = (sim.body.vx * sim.body.vx + sim.body.vz * sim.body.vz).sqrt() < 1.0;
    if (fx - px).hypot(fz - pz) > 7.0 || !quieto {
        p.espera_cura = 0.0;
        return;
    }
    p.espera_cura += time.delta_secs().min(0.1);
    if p.espera_cura < SEGUNDOS_CURA {
        return;
    }
    inv.remove("antidoto", 1);
    inv.add("gold_ingot", 15);
    let rapido = day.day_index() - p.dia_inicio < 7.0;
    if rapido {
        // Final secreto: cortar el brote en la primera semana.
        inv.add("gem_blue", 3);
    }
    persist.mark_dirty();
    p.etapa = etapa::CURADA;
    p.avance = 0.0;
    p.focos.clear();
    p.aviso = Some((
        if rapido { "Pandemia detenida en la primera semana. Final secreto: tesoro de gemas".into() } else { "Pandemia detenida. El mundo se recupera".to_string() },
        9.0,
    ));
    p.sucio = true;
    for (e, i) in &infectados {
        if !i.guardia {
            commands.entity(e).despawn_recursive();
        }
    }
    for e in &manchas {
        commands.entity(e).despawn_recursive();
    }
    sfx.send(crate::audio::PlaySfxAt(crate::audio::Sfx::Pickup, Vec2::new(pt.translation.x, pt.translation.y)));
}

/// Una mancha enferma en el suelo de cada foco (un sprite por foco, con el degradado compartido).
fn manchas(
    mut commands: Commands,
    p: Res<Pandemia>,
    glow: Option<Res<crate::light2d::GlowTexture>>,
    q: Query<(Entity, &Transform), With<Mancha>>,
) {
    if !p.is_changed() && !q.is_empty() {
        return;
    }
    let Some(glow) = glow else { return };
    let vivos = q.iter().count();
    if vivos == p.focos.len() && p.etapa < etapa::CURADA {
        return;
    }
    for (e, _) in &q {
        commands.entity(e).despawn_recursive();
    }
    if p.etapa >= etapa::CURADA {
        return;
    }
    for &(fx, fz) in &p.focos {
        let lado = RADIO_ENFERMO * 2.2;
        let mut sprite = Sprite::from_image(glow.0.clone());
        sprite.custom_size = Some(Vec2::splat(lado));
        sprite.color = Color::srgba(0.62, 0.12, 0.58, 0.55);
        commands.spawn((sprite, Transform::from_xyz(fx, -fz, crate::layers::FLOOR + crate::layers::SOBRE_EL_PISO), Mancha, Name::new("ManchaInfectada")));
    }
}

/// El mundo enfermo: árboles y plantas cerca de un foco se tiñen de verde-violáceo, más cuanto
/// más avanzada la pandemia. A 1 Hz y sólo con focos a la vista.
fn tenir_el_mundo(
    time: Res<Time>,
    p: Res<Pandemia>,
    mut acc: Local<f32>,
    mut decor: Query<(&Transform, &mut Sprite), With<SurfaceDecor>>,
    player: Query<&Transform, (With<Player>, Without<SurfaceDecor>)>,
) {
    *acc += time.delta_secs();
    if *acc < 1.0 {
        return;
    }
    *acc = 0.0;
    let Ok(pt) = player.get_single() else { return };
    let (px, pz) = (pt.translation.x, -pt.translation.y);
    let cerca: Vec<(f32, f32)> = p.focos.iter().copied().filter(|&(fx, fz)| (fx - px).hypot(fz - pz) < RADIO_ENFERMO + 120.0).collect();
    let curada = p.etapa >= etapa::CURADA;
    if cerca.is_empty() && !curada && p.focos.is_empty() {
        return;
    }
    let fuerza = (0.35 + p.avance / 100.0 * 0.5).min(0.85);
    for (tf, mut sprite) in &mut decor {
        let (x, z) = (tf.translation.x, -tf.translation.y);
        let d = cerca.iter().map(|&(fx, fz)| (fx - x).hypot(fz - z)).fold(f32::MAX, f32::min);
        let objetivo = if d < RADIO_ENFERMO && !curada { (1.0 - d / RADIO_ENFERMO) * fuerza } else { 0.0 };
        let c = sprite.color.to_srgba();
        // Mezcla el RGB hacia un verde-violeta enfermo; el alfa es de otro sistema y no se toca.
        let (tr, tg, tb) = (0.72, 0.50, 0.78);
        let nr = 1.0 + (tr - 1.0) * objetivo;
        let ng = 1.0 + (tg - 1.0) * objetivo;
        let nb = 1.0 + (tb - 1.0) * objetivo;
        if (c.red - nr).abs() > 0.02 || (c.green - ng).abs() > 0.02 || (c.blue - nb).abs() > 0.02 {
            sprite.color = Color::srgba(nr, ng, nb, c.alpha);
        }
    }
}

/// La pista del objetivo, arriba a la derecha, bajo el reloj.
fn hud(
    mut commands: Commands,
    layout: Res<crate::input::TouchLayout>,
    p: Res<Pandemia>,
    bajas: Res<Bajas>,
    mut q: Query<(Entity, &mut Text), With<PandemiaHud>>,
) {
    let k = crate::ui_pack::k(&layout);
    let mut texto = String::new();
    if p.etapa >= etapa::BROTE && p.etapa < etapa::CURADA {
        texto = format!("PANDEMIA {:.0} %  ·  {} pts\n{}", p.avance, bajas.puntos, p.objetivo());
    }
    if let Some((a, _)) = &p.aviso {
        texto = if texto.is_empty() { a.clone() } else { format!("{texto}\n{a}") };
    }
    if texto.is_empty() {
        for (e, _) in &q {
            commands.entity(e).despawn_recursive();
        }
        return;
    }
    if let Ok((_, mut t)) = q.get_single_mut() {
        if t.0 != texto {
            t.0 = texto;
        }
        return;
    }
    commands.spawn((
        Node {
            position_type: PositionType::Absolute,
            right: Val::Px(layout.insets.right as f32 + 12.0),
            top: Val::Px(layout.insets.top as f32 + 190.0 * k),
            max_width: Val::Px(layout.width * 0.27),
            ..default()
        },
        GlobalZIndex(60),
        PandemiaHud,
        Text::new(texto),
        TextFont { font_size: (17.0 * k).max(11.0), ..default() },
        TextColor(Color::srgb(0.85, 0.98, 0.78)),
        Name::new("PandemiaHud"),
    ));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sin_virus_descubierto_no_hay_infectados_sueltos() {
        let p = Pandemia::default();
        assert!(!p.activa());
        let curada = Pandemia { etapa: etapa::CURADA, ..Default::default() };
        assert!(!curada.activa());
        let en_marcha = Pandemia { etapa: etapa::MUESTRA, avance: 10.0, ..Default::default() };
        assert!(en_marcha.activa());
    }

    #[test]
    fn el_cupo_crece_con_el_avance_con_tope_y_suma_jugadores() {
        let mut p = Pandemia { etapa: etapa::MUESTRA, ..Default::default() };
        let c0 = p.cupo(0);
        p.avance = 60.0;
        assert!(p.cupo(0) > c0);
        p.avance = 100.0;
        assert!(p.cupo(0) <= TOPE_CUPO);
        assert_eq!(p.cupo(2), p.cupo(0) + 4);
    }

    #[test]
    fn cada_etapa_activa_tiene_objetivo() {
        for e in etapa::BROTE..=etapa::ANTIDOTO {
            assert!(!Pandemia { etapa: e, ..Default::default() }.objetivo().is_empty(), "etapa {e}");
        }
    }

    #[test]
    fn el_estado_se_guarda_y_se_lee() {
        let p = Pandemia { etapa: etapa::ANALIZADA, avance: 33.0, focos: vec![(1.0, 2.0)], alfas_muertos: 2, ..Default::default() };
        let json = serde_json::to_string(&p).unwrap();
        let q: Pandemia = serde_json::from_str(&json).unwrap();
        assert_eq!((q.etapa, q.alfas_muertos, q.focos.len()), (etapa::ANALIZADA, 2, 1));
    }
}

/// QA: `HA_UIDUMP=1` imprime cada 6 s los nodos de UI que tocan la esquina inferior izquierda
/// (x < 120 px, y > 70 % del alto), para identificar rectángulos sueltos.
pub fn volcar_ui_esquina(
    time: Res<Time>,
    mut acc: Local<f32>,
    nodos: Query<(Entity, &ComputedNode, &GlobalTransform, Option<&Name>, Option<&BackgroundColor>, Option<&ImageNode>)>,
    ventana: Query<&Window, With<bevy::window::PrimaryWindow>>,
) {
    if crate::flags::var("HA_UIDUMP").is_err() {
        return;
    }
    *acc += time.delta_secs();
    if *acc < 6.0 {
        return;
    }
    *acc = 0.0;
    let Ok(w) = ventana.get_single() else { return };
    let (an, al) = (w.physical_width() as f32, w.physical_height() as f32);
    for (e, c, g, nombre, fondo, img) in &nodos {
        let s = c.size();
        let ctr = g.translation().truncate();
        let (x0, x1, y0, y1) = (ctr.x - s.x / 2.0, ctr.x + s.x / 2.0, ctr.y - s.y / 2.0, ctr.y + s.y / 2.0);
        if x0 < 120.0 && y1 > al * 0.70 && s.x > 8.0 && s.y > 8.0 && s.x < an * 0.9 {
            eprintln!(
                "ha-uidump: {:?} {:?} rect=({x0:.0},{y0:.0})-({x1:.0},{y1:.0}) bg={:?} img={}",
                e,
                nombre.map(|n| n.as_str()),
                fondo.map(|b| b.0.to_srgba().to_f32_array()),
                img.is_some()
            );
        }
    }
}
