//! Compañero — la "mano derecha" del jugador. Su cabeza es la MISMA de los aldeanos
//! (`ha_shared::sims`, clase `Companero`): barras que decaen (descanso, compañía, ocio,
//! seguridad), y en cada decisión elige entre lo que el mundo le ofrece — acompañarte,
//! sentarse a descansar cuando parás, dar una vuelta, charlar, rezar si se lo pedís. La
//! orden del jugador (`CompanionOrder`) inclina la balanza; no la reemplaza.
//!
//! La IA frugal (IAFZ-64) se quitó de acá: era ruido de rumbo sobre un "seguir" hecho a
//! mano, y su hill-climb aprendía a quedarse cerca — cosa que ya es un anuncio. Ver
//! biblia §6.

use bevy::prelude::*;
use ha_shared::sims::{self, Actividad, Anuncio, Clase, Motivo, Vida};
use ha_shared::CelestialBody;

use crate::scene::WorldContextRuntime;
use crate::sim::Player;
use crate::world2d::{active_surface_body, render_xy, sort_z, ActivePlanet, TileWorld};

const AI_HZ: f32 = 15.0;
const TURN: f32 = 2.8;
const VEL: f32 = 5.2; // un pelín más rápido que la fauna: te alcanza
const FOLLOW_MIN: f32 = 2.0; // no se te encima
const LEASH: f32 = 55.0; // si te alejás más (volaste/teleport), reaparece a tu lado
/// Hoja del Character Generator que viste al compañero (ver `society::spawn_npc`).
const COMPANION_SKIN: &str = "npc/char_7";
/// Medida humana: la misma que el jugador y los aldeanos (el cuadro es 1:2).
const COMPANION_SIZE: Vec2 = Vec2::new(0.95, 1.9);
const COMPANION_UID: u64 = 990;

#[derive(Component)]
pub struct Companion {
    x: f32,
    z: f32,
    heading: f32,
    body: CelestialBody,
    vida: Vida,
    prev_px: f32,
    prev_pz: f32,
    /// Cuánto lleva el jugador quieto (s): sentarse a descansar sólo si te ve parar.
    quieto: f32,
    bump: f32,
    say_cd: f32,
    was_founded: bool,
    /// Su lugar en el mundo: donde vive su vida cuando no lo mandás a seguirte. Se corre a donde esté
    /// cuando le decís «vivi tu vida».
    casa: (f32, f32),
    /// Para detectar el cambio de orden.
    orden_previa: CompanionTask,
    /// Cuánto lleva trabajando desde la última entrega (s).
    cosecha: f32,
}

impl Companion {
    /// Qué está haciendo, en una línea para el menú.
    pub fn estado(&self) -> String {
        self.vida.actividad().verbo(Clase::Companero).to_string()
    }

    pub fn pos(&self) -> (f32, f32) {
        (self.x, self.z)
    }
}

/// Tarea asignada al compañero (mano derecha): lo maneja el jugador (o el tester
/// vía comando). `Follow` = te sigue; `Gather` = deambula amplio (reúne/explora);
/// `Pray` = orbita cerca del ídolo (venera).
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum CompanionTask {
    /// Hace su vida alrededor de su casa: pasea, descansa, charla si pasás cerca. No te sigue.
    #[default]
    Libre,
    Follow,
    /// Trabaja: recorre la zona y te trae recursos de a poco.
    Gather,
    Pray,
}

#[derive(Resource, Default)]
pub struct CompanionOrder(pub CompanionTask);

pub struct CompanionPlugin;

impl Plugin for CompanionPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<CompanionOrder>()
            .add_systems(Update, (spawn_companion, tick_companion).chain());
    }
}

/// Spawnea al compañero UNA vez, en la superficie, junto al jugador.
fn spawn_companion(
    mut commands: Commands,
    art: Res<crate::art2d::ArtLibrary>,
    ctx: Res<WorldContextRuntime>,
    planet: Res<ActivePlanet>,
    existing: Query<(), With<Companion>>,
    player: Query<&Transform, With<Player>>,
) {
    if !existing.is_empty() {
        return;
    }
    // Sólo en superficie (la mano derecha camina con vos; volar es F2).
    if !matches!(ctx.active, ha_shared::WorldContext::Overworld | ha_shared::WorldContext::Luna) {
        return;
    }
    let Ok(pt) = player.get_single() else {
        return;
    };
    let (px, pz) = (pt.translation.x, -pt.translation.y);
    let body = active_surface_body(&ctx, &planet);

    let (cx, cz) = (px + 1.5, pz + 1.5);
    // **Cuerpo entero, como un aldeano.** El slot `npc/companion` apuntaba a un PNG
    // de 39×39 que es SÓLO UNA CABEZA (resto del arte 3D horneado, `e4d9595`), y
    // estirado a 1,7 m cuadrado el compañero era una cabeza flotante (lo reportó
    // el usuario el 28 sep 2026). Se viste con una hoja del Character Generator,
    // la misma familia y la misma medida humana que los aldeanos y el jugador.
    let sprite = art.sprite_or_color(COMPANION_SKIN, Color::srgb(0.35, 0.7, 1.0), COMPANION_SIZE);
    let mut ent = commands.spawn((
        sprite,
        Transform::from_xyz(cx, -cz, sort_z(cz)),
        Companion {
            x: cx,
            z: cz,
            heading: 0.0,
            body,
            vida: Vida::nueva_con(COMPANION_UID, 0.5),
            prev_px: px,
            prev_pz: pz,
            quieto: 0.0,
            bump: 0.0,
            say_cd: 3.0,
            was_founded: false,
            casa: (cx, cz),
            orden_previa: CompanionTask::Libre,
            cosecha: 0.0,
        },
        Name::new("Companion"),
    ));
    crate::light2d::attach(&mut ent, &art, &[COMPANION_SKIN], COMPANION_SIZE);
    eprintln!("ha-companion: spawn");
}

/// El compañero decide con la misma cabeza que un aldeano y camina hacia lo que eligió.
#[allow(clippy::type_complexity)]
fn tick_companion(
    time: Res<Time>,
    ctx: Res<WorldContextRuntime>,
    planet: Res<ActivePlanet>,
    tiles: Res<TileWorld>,
    order: Res<CompanionOrder>,
    day: Res<crate::sky2d::DayClock>,
    religion: Res<crate::society::Religion>,
    mut speech: ResMut<crate::dialog::SpeechQueue>,
    player: Query<&Transform, (With<Player>, Without<Companion>)>,
    mut q: Query<(&mut Companion, &mut Transform, &mut Sprite)>,
    mut inv: ResMut<crate::inventory_persist::PlayerInventory>,
    mut persist: ResMut<crate::inventory_persist::InventoryPersistState>,
    mut acc: Local<f32>,
    mut ads: Local<Vec<Anuncio>>,
) {
    // ~15 Hz como la fauna (batería). En pausa dt=0 → no corre.
    *acc += time.delta_secs();
    if *acc < 1.0 / AI_HZ {
        return;
    }
    let dt = (*acc).min(0.2);
    *acc = 0.0;

    let Ok((mut c, mut tf, mut sprite)) = q.get_single_mut() else {
        return;
    };
    let Ok(pt) = player.get_single() else {
        return;
    };
    let c = &mut *c;
    let (px, pz) = (pt.translation.x, -pt.translation.y);
    let body = active_surface_body(&ctx, &planet);

    // Si cambiaste de cuerpo o te alejaste mucho (volaste), reaparece a tu lado —
    // sigue siendo tu mano derecha.
    let dx = px - c.x;
    let dz = pz - c.z;
    // Sólo reaparece a tu lado si cambiaste de cuerpo, o si lo mandaste a seguirte y te fuiste muy lejos.
    // Libre, se queda en su casa: tiene su vida.
    if body != c.body || (order.0 == CompanionTask::Follow && (dx * dx + dz * dz).sqrt() > LEASH) {
        c.x = px + 1.5;
        c.z = pz + 1.5;
        c.casa = (c.x, c.z);
        c.body = body;
        c.vida.abandonar();
    }
    // Al soltarlo (vivi tu vida) o mandarlo a trabajar, su casa es donde esté.
    if order.0 != c.orden_previa {
        if matches!(order.0, CompanionTask::Libre | CompanionTask::Gather) {
            c.casa = (c.x, c.z);
        }
        c.orden_previa = order.0;
        c.cosecha = 0.0;
        c.vida.abandonar();
    }
    let dx = px - c.x;
    let dz = pz - c.z;
    let dist = (dx * dx + dz * dz).sqrt().max(0.001);

    let pspeed = ((px - c.prev_px).powi(2) + (pz - c.prev_pz).powi(2)).sqrt() / dt.max(1e-3);
    c.prev_px = px;
    c.prev_pz = pz;
    c.quieto = if pspeed < 0.3 { c.quieto + dt } else { 0.0 };
    c.bump = (c.bump - dt).max(0.0);

    // ── LA CABEZA ─────────────────────────────────────────────────────────────
    use std::f32::consts::{FRAC_PI_2, PI, TAU};
    let fase = day.phase();
    c.vida.tick(Clase::Companero, fase, dt, (c.x, c.z));
    if c.vida.necesita_decidir(Clase::Companero, fase) {
        let desired = match order.0 {
            CompanionTask::Libre => 6.0,
            CompanionTask::Follow => FOLLOW_MIN,
            CompanionTask::Pray => 1.3,
            CompanionTask::Gather => 7.0,
        };
        let azar = |k: u64| {
            let h = sims::hash(time.elapsed().as_micros() as u64 ^ k.wrapping_mul(0x9E37_79B9));
            (h >> 40) as f32 / (1u64 << 24) as f32
        };
        ads.clear();
        let (hx, hz) = c.casa;
        let cerca_mio = dist < 14.0;
        match order.0 {
            // Te sigue: sólo si se lo pediste.
            CompanionTask::Follow => {
                ads.push(Anuncio::nuevo(Actividad::Seguir, px, pz).alcance(1e4).uso(desired).dur(6.0).bono(0.9));
                ads.push(Anuncio::nuevo(Actividad::Charlar, px, pz).alcance(1e4).uso(2.5).dur(8.0).gana(Motivo::Social, 0.05));
                if c.quieto > 2.0 {
                    ads.push(Anuncio::nuevo(Actividad::Descansar, px + 1.0, pz + 1.0).alcance(1e4).uso(1.5).dur(14.0).gana(Motivo::Sueno, 0.03));
                }
            }
            CompanionTask::Pray => {
                ads.push(Anuncio::nuevo(Actividad::Rezar, px, pz).alcance(1e4).uso(1.3).dur(12.0).bono(1.2));
                ads.push(Anuncio::nuevo(Actividad::Seguir, px, pz).alcance(1e4).uso(2.0).dur(6.0).bono(0.3));
            }
            // Trabaja: recorre una zona amplia alrededor de su casa.
            CompanionTask::Gather => {
                for k in 0..3u64 {
                    let ang = azar(1 + k) * TAU;
                    let r = 8.0 + azar(7 + k) * 22.0;
                    ads.push(Anuncio::nuevo(Actividad::Jugar, hx + ang.cos() * r, hz + ang.sin() * r).alcance(1e4).uso(1.5).dur(10.0).gana(Motivo::Ocio, 0.02).bono(0.9));
                }
            }
            // Vive su vida: pasea cerca de su casa, descansa, charla si pasás, reza de vez en cuando.
            CompanionTask::Libre => {
                for k in 0..3u64 {
                    let ang = azar(1 + k) * TAU;
                    let r = 2.0 + azar(7 + k) * 8.0;
                    ads.push(Anuncio::nuevo(Actividad::Jugar, hx + ang.cos() * r, hz + ang.sin() * r).alcance(1e4).uso(1.5).dur(9.0).gana(Motivo::Ocio, 0.05));
                }
                ads.push(Anuncio::nuevo(Actividad::Descansar, hx + 1.0, hz + 1.0).alcance(1e4).uso(1.5).dur(16.0).gana(Motivo::Sueno, 0.04).bono(if c.vida.actividad() == Actividad::Jugar { 0.3 } else { 0.0 }));
                ads.push(Anuncio::nuevo(Actividad::Rezar, hx, hz).alcance(1e4).uso(1.3).dur(10.0).bono(0.1));
                // Sólo si pasás cerca se acerca a charlar: no te persigue.
                if cerca_mio {
                    ads.push(Anuncio::nuevo(Actividad::Charlar, px, pz).alcance(1e4).uso(2.5).dur(8.0).gana(Motivo::Social, 0.05).bono(0.5));
                }
            }
        }
        c.vida.decidir(Clase::Companero, (c.x, c.z), fase, &ads);
    }
    // Lo que persigue se mueve con vos.
    if let Some((_, _, _)) = c.vida.destino() {
        match c.vida.actividad() {
            Actividad::Seguir | Actividad::Charlar | Actividad::Rezar | Actividad::Descansar if order.0 == CompanionTask::Follow || order.0 == CompanionTask::Pray || c.vida.actividad() == Actividad::Charlar => {
                let (ox, oz) = if c.vida.actividad() == Actividad::Descansar { (1.0, 1.0) } else { (0.0, 0.0) };
                c.vida.apuntar(px + ox, pz + oz);
            }
            _ => {}
        }
    }
    c.vida.por_resolver();

    // ── MOVER ─────────────────────────────────────────────────────────────────
    let toward = dz.atan2(dx);
    let act = c.vida.actividad();
    let (goal, speed): (Option<f32>, f32) = match c.vida.destino() {
        Some((tx, tz, false)) => (Some((tz - c.z).atan2(tx - c.x)), if dist > 6.0 { VEL } else { VEL * 0.85 }),
        Some((_, _, true)) => match act {
            // Rezando orbita; jugando o siguiendo se mueve un poco en el lugar.
            Actividad::Rezar => (Some(toward + FRAC_PI_2), VEL * 0.25),
            Actividad::Seguir | Actividad::Jugar => {
                (Some(c.heading + (c.x * 0.3 + time.elapsed_secs() * 0.5).sin() * 0.3), VEL * 0.2)
            }
            _ => (None, 0.0),
        },
        None => (Some(c.heading + (c.x * 0.3 + time.elapsed_secs() * 0.5).sin() * 0.3), VEL * 0.15),
    };
    if let Some(g) = goal {
        let diff = (g - c.heading + PI).rem_euclid(TAU) - PI;
        c.heading += diff.clamp(-1.0, 1.0) * TURN * dt * 0.9;
    }
    if speed > 0.0 {
        let nx = c.x + c.heading.cos() * speed * dt;
        let nz = c.z + c.heading.sin() * speed * dt;
        if tiles.solid_at(body, nx.floor() as i64, nz.floor() as i64) {
            c.heading += 1.3; // rebota
            if c.bump <= 0.0 {
                c.vida.trabar();
                c.bump = 2.5;
            }
        } else {
            c.x = nx;
            c.z = nz;
        }
    }

    // ── RENDER ────────────────────────────────────────────────────────────────
    let r = render_xy(c.x, c.z);
    tf.translation = Vec3::new(r.x, r.y, sort_z(c.z));
    // Skin natural; leve dorado si ya fundaron el culto (es el primer discípulo).
    sprite.color = if religion.founded {
        Color::srgb(1.0, 0.96, 0.78)
    } else {
        Color::WHITE
    };

    // ── TRABAJAR: cada ~25 s de paseo por la zona te trae un recurso (llega a tu mochila si estás a < 90 m) ──
    if order.0 == CompanionTask::Gather && c.vida.actividad() == Actividad::Jugar {
        c.cosecha += dt;
        if c.cosecha >= 25.0 && dist < 90.0 {
            c.cosecha = 0.0;
            let botin = ["wood", "stone", "sticks", "apple"][(sims::hash(time.elapsed().as_millis() as u64) % 4) as usize];
            inv.add(botin, 1);
            persist.mark_dirty();
            let nombre = match botin { "wood" => "madera", "stone" => "piedra", "sticks" => "palos", _ => "una manzana" };
            crate::dialog::say(&mut speech, c.x, c.z, format!("Te dejé {nombre} en la mochila."), Color::srgb(0.6, 0.9, 1.0));
        }
    }

    // ── CHAT (mano derecha): reacciona al culto y a cómo está; conversa ────────
    c.say_cd -= dt;
    let just_founded = religion.founded && !c.was_founded;
    c.was_founded = religion.founded;
    if just_founded {
        crate::dialog::say(
            &mut speech,
            c.x,
            c.z,
            format!("¡Contigo hasta el fin, {}!", religion.name),
            Color::srgb(1.0, 0.9, 0.35),
        );
        c.say_cd = 6.0;
    } else if c.say_cd <= 0.0 {
        c.say_cd = 9.0;
        let line: String = if let Some(q) = c.vida.queja(Clase::Companero) {
            q.to_string()
        } else {
            match (act, order.0) {
                (Actividad::Descansar, _) => "Un ratito de descanso, jefe.".into(),
                (Actividad::Jugar, _) => "Voy a echar un vistazo.".into(),
                (_, CompanionTask::Follow) if religion.founded => "Los fieles crecen, jefe.".into(),
                (_, CompanionTask::Follow) => "A tus órdenes.".into(),
                (_, CompanionTask::Libre) => "Hago mi vida por acá, jefe.".into(),
                (_, CompanionTask::Gather) => "Trabajando la zona.".into(),
                (_, CompanionTask::Pray) => "Que el Ki nos guíe.".into(),
            }
        };
        crate::dialog::say(&mut speech, c.x, c.z, line, Color::srgb(0.6, 0.9, 1.0));
    }
}
