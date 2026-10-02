//! La civilización SE VE: lo que `ha_shared::civ` decide en datos, acá ocurre delante del
//! jugador. El panel MUNDO queda como consola de depuración; esto es el juego.
//!
//! * **Escenas de pueblo** (sólo el pueblo que tenés alrededor): votantes en la plaza el día
//!   de la elección, soldados y una fogata de campamento hacia el enemigo mientras dura la
//!   guerra, presos y desterrados cuando la justicia dicta, y pobladores caminando por los
//!   pueblos que nacieron de un refugio (no tienen NPCs de `society.rs`: nadie los sembró).
//! * **El Registro**: un objeto en la plaza que evoluciona con los saberes del pueblo (mesa
//!   con pergaminos, escritorio, monitor, consola cuántica). Tocarlo abre la consola.
//!   El radiotelescopio aparece cuando el pueblo tiene astronomía y telecomunicaciones.
//! * **Efectos**: bombas (bola de fuego, onda, hongo, destello de pantalla), cohetes que
//!   despegan de los pueblos, lluvia radiactiva que hiere, refugios con su entrada.
//!
//! Todo con presupuesto: los figurantes son sprites livianos (sin cerebro, sin colisión),
//! con tope global, y NUNCA apuntan al jugador (regla: nadie sigue al jugador).

use bevy::input::touch::{TouchInput, TouchPhase};
use bevy::prelude::*;
use bevy::window::PrimaryWindow;
use ha_shared::civ::cronica::Tipo;
use ha_shared::civ::saber;

use crate::art2d::ArtLibrary;
use crate::civilizacion::Civ;
use crate::input::{touch_blocked, touch_in_move_zone, TouchBlockRects, TouchLayout};
use crate::sim::{Player, SimState};
use crate::society::{Npc, Society};
use crate::world2d::{render_xy, sort_z};

const MAX_FIGURANTES: usize = 28;
/// Hasta dónde se arma la escena de un pueblo (m del centro).
const RADIO_ESCENA: f32 = 110.0;
/// Hasta dónde se ve una bomba / un despegue con efecto completo.
const RADIO_BOMBA: f32 = 1800.0;
const RADIO_COHETE: f32 = 600.0;
const VELOCIDAD_PASEO: f32 = 1.5;
const RADIACION_DANIA: f32 = 0.10;
const TAMANO_FIGURANTE: Vec2 = Vec2::new(0.95, 1.9);
const NUM_SKINS: u64 = 20;
const PICK_REGISTRO: f32 = 3.0;

// ─────────────────────────── estado ───────────────────────────

#[derive(Resource, Default)]
pub struct Vida {
    cursor: u64,
    iniciado: bool,
    /// (pueblo, segundos que quedan) del día de elección que se está mostrando.
    votacion: Option<(u32, f32)>,
    aviso: String,
    aviso_t: f32,
    radiacion: f32,
}

/// Todo lo que este módulo pone en el mundo lleva esta marca: al salir de la partida se va.
#[derive(Component)]
struct Escena;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Rol {
    Votante,
    Soldado,
    Preso,
    Desterrado,
    Poblador,
}

#[derive(Component)]
struct Figurante {
    rol: Rol,
    pueblo: u32,
    x: f32,
    z: f32,
    dest: (f32, f32),
    ancla: (f32, f32),
    radio: f32,
    espera: f32,
    ttl: f32,
    semilla: u32,
}

#[derive(Component)]
struct Campamento {
    pueblo: u32,
}

#[derive(Component)]
struct Registro {
    pueblo: u32,
    clave: &'static str,
    x: f32,
    z: f32,
}

#[derive(Component)]
struct Radiotelescopio {
    pueblo: u32,
}

#[derive(Component)]
struct EntradaRefugio {
    id: u32,
}

#[derive(Component)]
struct Bola {
    t: f32,
    x: f32,
    z: f32,
    tipo: FaseBomba,
}

#[derive(Clone, Copy)]
enum FaseBomba {
    Fuego,
    Onda,
    Hongo(u8),
}

#[derive(Component)]
struct Cohete {
    t: f32,
    x: f32,
    z: f32,
}

#[derive(Component)]
struct Destello {
    fuerza: f32,
    t: f32,
}

#[derive(Component)]
struct AvisoTexto;

fn azar(semilla: u32) -> f32 {
    let mut h = semilla.wrapping_mul(0x9E37_79B1) ^ 0x85EB_CA6B;
    h ^= h >> 15;
    h = h.wrapping_mul(0x2C1B_3C6D);
    h ^= h >> 12;
    h = h.wrapping_mul(0x297A_2D39);
    h ^= h >> 15;
    (h & 0xFFFF) as f32 / 65535.0
}

fn punto_en_disco(centro: (f32, f32), radio: f32, semilla: u32) -> (f32, f32) {
    let a = azar(semilla) * std::f32::consts::TAU;
    let r = radio * azar(semilla ^ 0xABCD).sqrt();
    (centro.0 + a.cos() * r, centro.1 + a.sin() * r)
}

// ─────────────────────────── plugin ───────────────────────────

pub struct CivVidaPlugin;

impl Plugin for CivVidaPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Vida>()
            .add_systems(
                OnExit(crate::game_phase::GamePhase::Playing),
                (limpiar_escena, crate::salir::reiniciar::<Vida>).chain(),
            )
            .add_systems(
                Update,
                (
                    vida_eventos,
                    vida_escena,
                    vida_props,
                    vida_mover,
                    vida_efectos,
                    vida_radiacion,
                    vida_hud,
                    vida_tap,
                )
                    .run_if(crate::game_phase::world_active),
            );
        #[cfg(not(target_os = "android"))]
        app.add_systems(Update, vida_prueba.run_if(crate::game_phase::world_active));
    }
}

fn limpiar_escena(mut commands: Commands, todo: Query<Entity, With<Escena>>) {
    for e in &todo {
        commands.entity(e).despawn_recursive();
    }
}

fn en_la_tierra(soc: &Society) -> bool {
    soc.active_body() == Some(0)
}

// ─────────────────────────── eventos de la crónica ───────────────────────────

#[allow(clippy::too_many_arguments)]
fn vida_eventos(
    mut commands: Commands,
    time: Res<Time>,
    mut vida: ResMut<Vida>,
    civ: ResMut<Civ>,
    soc: Res<Society>,
    art: Res<ArtLibrary>,
    glow: Option<Res<crate::light2d::GlowTexture>>,
    player: Query<&Transform, With<Player>>,
    figurantes: Query<&Figurante>,
    mut reloj: Local<f32>,
) {
    *reloj -= time.delta_secs();
    let Some(m) = civ.mundo_ref() else { return };
    if vida.votacion.is_some() {
        let dt = time.delta_secs();
        if let Some((_, t)) = vida.votacion.as_mut() {
            *t -= dt;
        }
        if vida.votacion.is_some_and(|(_, t)| t <= 0.0) {
            vida.votacion = None;
        }
    }
    vida.aviso_t = (vida.aviso_t - time.delta_secs()).max(0.0);
    if *reloj > 0.0 {
        return;
    }
    *reloj = 0.5;
    if !vida.iniciado {
        vida.iniciado = true;
        vida.cursor = m.cronica.siguiente;
        return;
    }
    let Ok(pt) = player.get_single() else { return };
    let (px, pz) = (pt.translation.x, -pt.translation.y);
    let tierra = en_la_tierra(&soc);
    let dia = m.dia;
    let nuevos: Vec<_> = m.cronica.desde(vida.cursor).cloned().collect();
    vida.cursor = m.cronica.siguiente;
    let mut total = figurantes.iter().count();
    for h in nuevos {
        // La puesta al día (ausencia del jugador) no reproduce el pasado como espectáculo.
        if h.dia + 2 < dia {
            continue;
        }
        let pos_de = |id: u32| m.pueblos.get(&id).filter(|p| p.cuerpo == 0).map(|p| (p.x, p.z));
        match h.tipo {
            Tipo::Detonacion => {
                let Some((bx, bz)) = pos_de(h.otro) else { continue };
                if !tierra {
                    continue;
                }
                let d = ((bx - px).powi(2) + (bz - pz).powi(2)).sqrt();
                if d <= RADIO_BOMBA {
                    if let Some(g) = glow.as_ref() {
                        explotar(&mut commands, g, bx, bz);
                    }
                    let fuerza = (1.0 - d / RADIO_BOMBA).clamp(0.15, 1.0);
                    commands.spawn((
                        Node {
                            position_type: PositionType::Absolute,
                            width: Val::Percent(100.0),
                            height: Val::Percent(100.0),
                            ..default()
                        },
                        BackgroundColor(Color::srgba(1.0, 0.97, 0.85, 0.0)),
                        GlobalZIndex(880),
                        Destello { fuerza, t: 0.0 },
                        Escena,
                    ));
                    vida.aviso = h.texto.clone();
                } else {
                    vida.aviso = "Un resplandor blanco ilumina el horizonte.".into();
                }
                vida.aviso_t = 9.0;
            }
            Tipo::Lanzamiento => {
                let Some((cx, cz)) = pos_de(h.pueblo) else { continue };
                let d = ((cx - px).powi(2) + (cz - pz).powi(2)).sqrt();
                if tierra && d <= RADIO_COHETE {
                    spawn_cohete(&mut commands, &art, glow.as_deref(), cx, cz);
                    vida.aviso = h.texto.clone();
                    vida.aviso_t = 7.0;
                }
            }
            Tipo::Eleccion => {
                if let Some((cx, cz)) = pos_de(h.pueblo) {
                    if tierra && ((cx - px).powi(2) + (cz - pz).powi(2)).sqrt() <= RADIO_ESCENA * 1.5 {
                        vida.votacion = Some((h.pueblo, 30.0));
                    }
                }
            }
            Tipo::Justicia => {
                let Some((cx, cz)) = pos_de(h.pueblo) else { continue };
                if !tierra || ((cx - px).powi(2) + (cz - pz).powi(2)).sqrt() > RADIO_ESCENA || total >= MAX_FIGURANTES {
                    continue;
                }
                let preso = h.texto.contains("va preso");
                let desterrado = h.texto.contains("desterrado");
                if !preso && !desterrado {
                    continue;
                }
                let sem = (h.id as u32).wrapping_mul(2654435761);
                let inicio = punto_en_disco((cx, cz), 6.0, sem);
                let (rol, dest, ancla, radio, ttl) = if preso {
                    let celda = (cx + 22.0, cz + 14.0);
                    (Rol::Preso, celda, celda, 1.2, 120.0)
                } else {
                    let a = azar(sem ^ 7) * std::f32::consts::TAU;
                    let lejos = (cx + a.cos() * (RADIO_ESCENA + 30.0), cz + a.sin() * (RADIO_ESCENA + 30.0));
                    (Rol::Desterrado, lejos, lejos, 0.0, 70.0)
                };
                spawn_figurante(&mut commands, &art, rol, h.pueblo, inicio, dest, ancla, radio, ttl, sem);
                total += 1;
                vida.aviso = h.texto.clone();
                vida.aviso_t = 6.0;
            }
            Tipo::Extincion | Tipo::Refundacion => {
                if tierra {
                    if let Some((cx, cz)) = pos_de(h.pueblo) {
                        if ((cx - px).powi(2) + (cz - pz).powi(2)).sqrt() <= RADIO_BOMBA {
                            vida.aviso = h.texto.clone();
                            vida.aviso_t = 8.0;
                        }
                    }
                }
            }
            _ => {}
        }
    }
}

fn explotar(commands: &mut Commands, glow: &crate::light2d::GlowTexture, x: f32, z: f32) {
    let r = render_xy(x, z);
    let zz = crate::layers::OVERHEAD + 0.5;
    let mut nuevo = |color: Color, tam: f32, fase: FaseBomba, dz: f32| {
        commands.spawn((
            Sprite { image: glow.0.clone(), color, custom_size: Some(Vec2::splat(tam)), ..default() },
            Transform::from_xyz(r.x, r.y, zz + dz),
            Bola { t: 0.0, x, z, tipo: fase },
            Escena,
        ));
    };
    nuevo(Color::srgba(1.0, 0.85, 0.45, 1.0), 10.0, FaseBomba::Fuego, 0.0);
    nuevo(Color::srgba(1.0, 0.6, 0.25, 0.55), 10.0, FaseBomba::Onda, -0.01);
    for i in 0..3u8 {
        nuevo(Color::srgba(0.95, 0.5, 0.2, 0.9), 10.0, FaseBomba::Hongo(i), 0.01 * (i as f32 + 1.0));
    }
}

fn spawn_cohete(commands: &mut Commands, art: &ArtLibrary, glow: Option<&crate::light2d::GlowTexture>, x: f32, z: f32) {
    let tam = art.tamano_real("ship/rocket", None, Vec2::new(2.0, 7.0));
    let (sprite, _) = art
        .world_sprite("ship/rocket", tam)
        .unwrap_or_else(|| (Sprite::from_color(Color::srgb(0.85, 0.85, 0.9), tam), None));
    let r = render_xy(x, z);
    let mut e = commands.spawn((sprite, Transform::from_xyz(r.x, r.y, crate::layers::OVERHEAD + 0.4), Cohete { t: 0.0, x, z }, Escena));
    if let Some(g) = glow {
        e.with_children(|c| {
            c.spawn((
                Sprite { image: g.0.clone(), color: Color::srgba(1.0, 0.7, 0.3, 0.9), custom_size: Some(Vec2::splat(6.0)), ..default() },
                Transform::from_xyz(0.0, -tam.y * 0.5 - 1.5, -0.01),
            ));
        });
    }
}

fn spawn_figurante(
    commands: &mut Commands,
    art: &ArtLibrary,
    rol: Rol,
    pueblo: u32,
    inicio: (f32, f32),
    dest: (f32, f32),
    ancla: (f32, f32),
    radio: f32,
    ttl: f32,
    semilla: u32,
) {
    let skin = format!("npc/char_{}", (semilla as u64 % NUM_SKINS) + 1);
    let mut sprite = art.sprite_or_color(&skin, Color::srgb(0.85, 0.82, 0.75), TAMANO_FIGURANTE);
    sprite.color = match rol {
        Rol::Soldado => Color::srgb(1.0, 0.78, 0.74),
        Rol::Preso => Color::srgb(0.72, 0.72, 0.78),
        Rol::Desterrado => Color::srgb(0.82, 0.78, 0.7),
        _ => Color::WHITE,
    };
    let r = render_xy(inicio.0, inicio.1);
    commands.spawn((
        sprite,
        Transform::from_xyz(r.x, r.y + TAMANO_FIGURANTE.y * 0.5, sort_z(inicio.1)),
        Figurante { rol, pueblo, x: inicio.0, z: inicio.1, dest, ancla, radio, espera: azar(semilla ^ 99) * 3.0, ttl, semilla },
        Escena,
    ));
}

// ─────────────────────────── escena del pueblo ───────────────────────────

#[allow(clippy::too_many_arguments)]
fn vida_escena(
    mut commands: Commands,
    time: Res<Time>,
    vida: Res<Vida>,
    civ: Res<Civ>,
    soc: Res<Society>,
    art: Res<ArtLibrary>,
    player: Query<&Transform, With<Player>>,
    npcs: Query<&Transform, (With<Npc>, Without<Player>)>,
    mut figurantes: Query<(Entity, &mut Figurante)>,
    campamentos: Query<(Entity, &Campamento)>,
    mut reloj: Local<f32>,
) {
    *reloj -= time.delta_secs();
    if *reloj > 0.0 {
        return;
    }
    *reloj = 1.0;
    let Some(m) = civ.mundo_ref() else { return };
    let Ok(pt) = player.get_single() else { return };
    let (px, pz) = (pt.translation.x, -pt.translation.y);
    // **Las colonias de otros cuerpos también se ven**: la escena se arma con los pueblos del
    // cuerpo que tenés debajo (la Luna, Marte…), no sólo con los de la Tierra.
    let cuerpo = soc.active_body();
    let cerca = if let Some(c) = cuerpo {
        m.pueblos
            .values()
            .filter(|p| p.vivo && p.cuerpo == c)
            .map(|p| (p.id, ((p.x - px).powi(2) + (p.z - pz).powi(2)).sqrt()))
            .filter(|(_, d)| *d < RADIO_ESCENA * 1.4)
            .min_by(|a, b| a.1.partial_cmp(&b.1).unwrap())
            .map(|(id, _)| id)
    } else {
        None
    };
    let Some(id) = cerca else {
        for (e, f) in &mut figurantes {
            if matches!(f.rol, Rol::Votante | Rol::Soldado | Rol::Poblador) {
                commands.entity(e).despawn_recursive();
            }
        }
        for (e, _) in &campamentos {
            commands.entity(e).despawn_recursive();
        }
        return;
    };
    let p = &m.pueblos[&id];
    let centro = (p.x, p.z);

    // ¿Cuánta gente quiere cada papel?
    let votantes = match vida.votacion {
        Some((v, _)) if v == id => ((3.0 + p.pob / 40.0) as usize).min(10),
        _ => 0,
    };
    let enemigo = m.guerras.iter().find_map(|g| {
        let otros = if g.bando_a.contains(&id) {
            &g.bando_b
        } else if g.bando_b.contains(&id) {
            &g.bando_a
        } else {
            return None;
        };
        otros.iter().filter_map(|o| m.pueblos.get(o)).filter(|q| Some(q.cuerpo) == cuerpo).map(|q| (q.x, q.z)).next()
    });
    let soldados = if enemigo.is_some() { ((2.0 + p.pob / 60.0) as usize).min(8) } else { 0 };
    let con_npcs = npcs
        .iter()
        .filter(|t| ((t.translation.x - centro.0).powi(2) + (-t.translation.y - centro.1).powi(2)).sqrt() < 70.0)
        .count();
    let pobladores = if con_npcs < 2 && p.pob >= 1.0 { ((p.pob / 8.0).ceil() as usize).clamp(1, 8) } else { 0 };

    let punto_campamento = enemigo.map(|(ex, ez)| {
        let (dx, dz) = (ex - centro.0, ez - centro.1);
        let l = (dx * dx + dz * dz).sqrt().max(1.0);
        (centro.0 + dx / l * 48.0, centro.1 + dz / l * 48.0)
    });

    let mut tiene = [0usize; 3];
    let mut total = 0usize;
    for (e, mut f) in &mut figurantes {
        total += 1;
        let (idx, quiere) = match f.rol {
            Rol::Votante => (0, votantes),
            Rol::Soldado => (1, soldados),
            Rol::Poblador => (2, pobladores),
            _ => continue,
        };
        if f.pueblo != id {
            commands.entity(e).despawn_recursive();
            total -= 1;
            continue;
        }
        tiene[idx] += 1;
        if tiene[idx] > quiere {
            // Sobra gente: se va caminando y desaparece lejos.
            let a = azar(f.semilla ^ 5) * std::f32::consts::TAU;
            f.dest = (centro.0 + a.cos() * (RADIO_ESCENA + 20.0), centro.1 + a.sin() * (RADIO_ESCENA + 20.0));
            f.ancla = f.dest;
            f.radio = 0.0;
            f.ttl = f.ttl.min(12.0);
        }
    }

    let mut siembra = |commands: &mut Commands, rol: Rol, falta: usize, ancla: (f32, f32), radio: f32| {
        for i in 0..falta {
            if total >= MAX_FIGURANTES {
                break;
            }
            let sem = (id.wrapping_mul(7919))
                .wrapping_add((rol as u32).wrapping_mul(104729))
                .wrapping_add(i as u32 * 31)
                .wrapping_add(time.elapsed_secs() as u32);
            // Los votantes vienen de las casas; los soldados, de la plaza; los pobladores, del pueblo.
            let inicio = punto_en_disco(centro, RADIO_ESCENA * 0.6, sem);
            let dest = punto_en_disco(ancla, radio, sem ^ 0x55);
            spawn_figurante(commands, &art, rol, id, inicio, dest, ancla, radio, f32::INFINITY, sem);
            total += 1;
        }
    };
    if votantes > tiene[0] {
        siembra(&mut commands, Rol::Votante, votantes - tiene[0], centro, 9.0);
    }
    if let (true, Some(camp)) = (soldados > tiene[1], punto_campamento) {
        siembra(&mut commands, Rol::Soldado, soldados - tiene[1], camp, 7.0);
    }
    if pobladores > tiene[2] {
        siembra(&mut commands, Rol::Poblador, pobladores - tiene[2], centro, 28.0);
    }

    // Fogata del campamento mientras dure la guerra.
    let hay = campamentos.iter().find(|(_, c)| c.pueblo == id).map(|(e, _)| e);
    match (punto_campamento, hay) {
        (Some(c), None) => {
            let clave = "prop/fogata_1";
            let tam = art.tamano_real(clave, None, Vec2::new(1.2, 1.2));
            let (mut sprite, anim) = art
                .world_sprite(clave, tam)
                .unwrap_or_else(|| (Sprite::from_color(Color::srgb(0.9, 0.45, 0.15), tam), None));
            sprite.color = Color::WHITE;
            let r = render_xy(c.0, c.1);
            let mut e = commands.spawn((sprite, Transform::from_xyz(r.x, r.y + tam.y * 0.5, sort_z(c.1)), Campamento { pueblo: id }, Escena));
            crate::light2d::attach(&mut e, &art, &[clave], tam);
            if let Some(a) = anim {
                e.insert(a);
            }
        }
        (None, Some(e)) => commands.entity(e).despawn_recursive(),
        _ => {}
    }
    for (e, c) in &campamentos {
        if c.pueblo != id {
            commands.entity(e).despawn_recursive();
        }
    }
}

fn vida_mover(
    mut commands: Commands,
    time: Res<Time>,
    mut q: Query<(Entity, &mut Figurante, &mut Transform, &mut Sprite)>,
) {
    let dt = time.delta_secs().min(0.1);
    for (e, mut f, mut tf, mut sprite) in &mut q {
        f.ttl -= dt;
        if f.ttl <= 0.0 {
            commands.entity(e).despawn_recursive();
            continue;
        }
        let (dx, dz) = (f.dest.0 - f.x, f.dest.1 - f.z);
        let d = (dx * dx + dz * dz).sqrt();
        if d > 0.4 {
            let paso = (VELOCIDAD_PASEO * dt).min(d);
            f.x += dx / d * paso;
            f.z += dz / d * paso;
            if dx.abs() > 0.05 {
                sprite.flip_x = dx < 0.0;
            }
        } else if f.radio > 0.0 {
            f.espera -= dt;
            if f.espera <= 0.0 {
                let sem = f.semilla.wrapping_add((time.elapsed_secs() * 7.0) as u32);
                f.dest = punto_en_disco(f.ancla, f.radio, sem);
                f.espera = 2.0 + azar(sem ^ 3) * 5.0;
                f.semilla = f.semilla.wrapping_add(1);
            }
        }
        let r = render_xy(f.x, f.z);
        tf.translation.x = r.x;
        tf.translation.y = r.y + TAMANO_FIGURANTE.y * 0.5;
        tf.translation.z = sort_z(f.z);
    }
}

// ─────────────────────────── objetos: Registro, radiotelescopio, refugios ───────────────────────────

fn clave_registro(s: ha_shared::civ::saber::Saberes) -> &'static str {
    if s.tiene(saber::COMPUTACION_CUANTICA) {
        "building/control_monitor_grande"
    } else if s.tiene(saber::COMPUTACION) {
        "building/monitor"
    } else if s.tiene(saber::IMPRENTA) || s.tiene(saber::ELECTRICIDAD) {
        "furniture/escritorio"
    } else {
        "building/carpa_mesa_papel"
    }
}

#[allow(clippy::too_many_arguments)]
fn vida_props(
    mut commands: Commands,
    time: Res<Time>,
    civ: Res<Civ>,
    soc: Res<Society>,
    art: Res<ArtLibrary>,
    player: Query<&Transform, With<Player>>,
    registros: Query<(Entity, &Registro)>,
    telescopios: Query<(Entity, &Radiotelescopio)>,
    entradas: Query<(Entity, &EntradaRefugio)>,
    mut reloj: Local<f32>,
) {
    *reloj -= time.delta_secs();
    if *reloj > 0.0 {
        return;
    }
    *reloj = 1.5;
    let Some(m) = civ.mundo_ref() else { return };
    let Ok(pt) = player.get_single() else { return };
    let (px, pz) = (pt.translation.x, -pt.translation.y);
    let cuerpo = soc.active_body();

    let cerca = if let Some(c) = cuerpo {
        m.pueblos
            .values()
            .filter(|p| p.vivo && p.cuerpo == c)
            .map(|p| (p.id, ((p.x - px).powi(2) + (p.z - pz).powi(2)).sqrt()))
            .filter(|(_, d)| *d < RADIO_ESCENA * 1.4)
            .min_by(|a, b| a.1.partial_cmp(&b.1).unwrap())
            .map(|(id, _)| id)
    } else {
        None
    };

    // Registro: uno por pueblo cercano, con el arte de su nivel de saberes.
    let clave_ok = cerca.map(|id| (id, clave_registro(m.pueblos[&id].saberes)));
    for (e, r) in &registros {
        if clave_ok != Some((r.pueblo, r.clave)) {
            commands.entity(e).despawn_recursive();
        }
    }
    if let Some((id, clave)) = clave_ok {
        if !registros.iter().any(|(_, r)| r.pueblo == id && r.clave == clave) {
            let p = &m.pueblos[&id];
            let (x, z) = (p.x + 5.0, p.z - 3.0);
            let tam = art.tamano_real(clave, None, Vec2::new(1.6, 1.3));
            let (sprite, anim) = art
                .world_sprite(clave, tam)
                .unwrap_or_else(|| (Sprite::from_color(Color::srgb(0.45, 0.85, 0.95), tam), None));
            let r = render_xy(x, z);
            let mut e = commands.spawn((sprite, Transform::from_xyz(r.x, r.y + tam.y * 0.5, sort_z(z)), Registro { pueblo: id, clave, x, z }, Escena));
            crate::light2d::attach(&mut e, &art, &[clave], tam);
            if let Some(a) = anim {
                e.insert(a);
            }
        }
    }

    // Radiotelescopio: astronomía + telecomunicaciones.
    let quiere_tele = cerca.filter(|id| {
        let s = m.pueblos[id].saberes;
        s.tiene(saber::ASTRONOMIA) && s.tiene(saber::TELECOM)
    });
    for (e, t) in &telescopios {
        if quiere_tele != Some(t.pueblo) {
            commands.entity(e).despawn_recursive();
        }
    }
    if let Some(id) = quiere_tele {
        if !telescopios.iter().any(|(_, t)| t.pueblo == id) {
            let p = &m.pueblos[&id];
            let (x, z) = (p.x - 14.0, p.z - 10.0);
            let clave = "prop/antena_3";
            let tam = art.tamano_real(clave, None, Vec2::new(2.0, 5.0));
            let (sprite, _) = art
                .world_sprite(clave, tam)
                .unwrap_or_else(|| (Sprite::from_color(Color::srgb(0.7, 0.7, 0.75), tam), None));
            let r = render_xy(x, z);
            commands.spawn((sprite, Transform::from_xyz(r.x, r.y + tam.y * 0.5, sort_z(z)), Radiotelescopio { pueblo: id }, Escena));
        }
    }

    // Refugios: la entrada se ve mientras estés cerca.
    let visibles: Vec<_> = if en_la_tierra(&soc) {
        m.refugios.iter().filter(|r| ((r.x - px).powi(2) + (r.z - pz).powi(2)).sqrt() < 200.0).collect()
    } else {
        Vec::new()
    };
    for (e, en) in &entradas {
        if !visibles.iter().any(|r| r.id == en.id) {
            commands.entity(e).despawn_recursive();
        }
    }
    for r in visibles {
        if entradas.iter().any(|(_, en)| en.id == r.id) {
            continue;
        }
        let clave = "building/bunker_entrada";
        let tam = art.tamano_real(clave, None, Vec2::new(3.0, 2.5));
        let (sprite, _) = art
            .world_sprite(clave, tam)
            .unwrap_or_else(|| (Sprite::from_color(Color::srgb(0.35, 0.36, 0.4), tam), None));
        let p = render_xy(r.x, r.z);
        commands.spawn((sprite, Transform::from_xyz(p.x, p.y + tam.y * 0.5, sort_z(r.z)), EntradaRefugio { id: r.id }, Escena));
    }
}

/// Tocar el Registro abre la consola de la civilización (es un objeto del mundo, no un menú).
#[allow(clippy::too_many_arguments)]
fn vida_tap(
    mouse: Res<ButtonInput<MouseButton>>,
    mut touches: EventReader<TouchInput>,
    windows: Query<&Window, With<PrimaryWindow>>,
    cam: Query<(&Camera, &GlobalTransform), With<crate::camera::GameCamera>>,
    player: Query<&Transform, With<Player>>,
    registros: Query<&Registro>,
    touch_layout: Res<TouchLayout>,
    blocks: Res<TouchBlockRects>,
    mut civ: ResMut<Civ>,
) {
    let screen = if mouse.just_pressed(MouseButton::Left) {
        windows.get_single().ok().and_then(|w| w.cursor_position())
    } else {
        touches
            .read()
            .find(|t| t.phase == TouchPhase::Started && !touch_in_move_zone(t.position, &touch_layout) && !touch_blocked(t.position, &blocks.0))
            .map(|t| t.position)
    };
    let Some(screen) = screen else { return };
    if civ.panel {
        return;
    }
    let (Ok((camera, cam_tf)), Ok(win), Ok(pt)) = (cam.get_single(), windows.get_single(), player.get_single()) else { return };
    let Some(mundo) = crate::render::scale::window_to_world_2d(camera, cam_tf, win, screen) else { return };
    let (px, pz) = (pt.translation.x, -pt.translation.y);
    for r in &registros {
        let d_toque = render_xy(r.x, r.z).distance(mundo);
        let d_jugador = ((r.x - px).powi(2) + (r.z - pz).powi(2)).sqrt();
        if d_toque <= PICK_REGISTRO && d_jugador <= 14.0 {
            civ.abrir_desde_registro();
            return;
        }
    }
}

// ─────────────────────────── efectos ───────────────────────────

fn vida_efectos(
    mut commands: Commands,
    time: Res<Time>,
    mut bolas: Query<(Entity, &mut Bola, &mut Sprite, &mut Transform), (Without<Cohete>, Without<Destello>)>,
    mut cohetes: Query<(Entity, &mut Cohete, &mut Transform), Without<Bola>>,
    mut destellos: Query<(Entity, &mut Destello, &mut BackgroundColor)>,
) {
    let dt = time.delta_secs().min(0.1);
    for (e, mut b, mut s, mut tf) in &mut bolas {
        b.t += dt;
        let t = b.t;
        let r = render_xy(b.x, b.z);
        let (tam, alfa, subida) = match b.tipo {
            FaseBomba::Fuego => (10.0 + 240.0 * (1.0 - (-t * 0.9).exp()), (1.0 - t / 9.0).clamp(0.0, 1.0), 0.0),
            FaseBomba::Onda => (10.0 + 900.0 * (1.0 - (-t * 0.5).exp()), 0.5 * (1.0 - t / 6.0).clamp(0.0, 1.0), 0.0),
            FaseBomba::Hongo(i) => {
                let arranque = i as f32 * 0.8;
                let u = (t - arranque).max(0.0);
                (30.0 + 90.0 * (1.0 - (-u * 0.7).exp()), 0.9 * (1.0 - (t / 14.0)).clamp(0.0, 1.0) * (t > arranque) as u8 as f32, 20.0 + 70.0 * u.min(8.0) / 8.0 * (i as f32 + 1.0))
            }
        };
        s.custom_size = Some(Vec2::splat(tam));
        s.color = s.color.with_alpha(alfa);
        tf.translation.x = r.x;
        tf.translation.y = r.y + subida;
        if t > 14.5 {
            commands.entity(e).despawn_recursive();
        }
    }
    for (e, mut c, mut tf) in &mut cohetes {
        c.t += dt;
        let r = render_xy(c.x, c.z);
        tf.translation.x = r.x;
        tf.translation.y = r.y + 3.0 + 0.9 * c.t * c.t;
        if c.t > 14.0 {
            commands.entity(e).despawn_recursive();
        }
    }
    for (e, mut d, mut bg) in &mut destellos {
        d.t += dt;
        let curva = if d.t < 0.15 { d.t / 0.15 } else { (1.0 - (d.t - 0.15) / 3.0).max(0.0) };
        bg.0 = bg.0.with_alpha(curva * d.fuerza * 0.9);
        if d.t > 3.3 {
            commands.entity(e).despawn_recursive();
        }
    }
}

// ─────────────────────────── radiación ───────────────────────────

fn vida_radiacion(
    time: Res<Time>,
    mut vida: ResMut<Vida>,
    civ: Res<Civ>,
    soc: Res<Society>,
    mut player: Query<(&Transform, &mut SimState), With<Player>>,
    mut reloj: Local<f32>,
) {
    *reloj -= time.delta_secs();
    if *reloj > 0.0 {
        return;
    }
    *reloj = 1.0;
    let Some(m) = civ.mundo_ref() else { return };
    let Ok((pt, mut sim)) = player.get_single_mut() else { return };
    if !en_la_tierra(&soc) {
        vida.radiacion = 0.0;
        return;
    }
    let (px, pz) = (pt.translation.x, -pt.translation.y);
    let bajo_tierra = m.refugios.iter().any(|r| ((r.x - px).powi(2) + (r.z - pz).powi(2)).sqrt() < 10.0);
    let rad = if bajo_tierra { 0.0 } else { m.radiacion_en(px, pz) };
    vida.radiacion = rad;
    if rad > RADIACION_DANIA {
        // Herida suave: la lluvia radiactiva castiga, no mata (el juego no tiene "game over").
        sim.hp = (sim.hp - (rad - RADIACION_DANIA) * 10.0 * (1.0 - sim.armadura)).max(1.0);
    }
}

// ─────────────────────────── HUD ───────────────────────────

fn vida_hud(
    mut commands: Commands,
    vida: Res<Vida>,
    mut textos: Query<(&mut Text, &mut TextColor), With<AvisoTexto>>,
    mut creado: Local<bool>,
    existentes: Query<(), With<AvisoTexto>>,
) {
    if existentes.is_empty() {
        *creado = false;
    }
    if !*creado {
        *creado = true;
        commands.spawn((
            Text::new(""),
            TextFont { font_size: 15.0, ..default() },
            TextColor(Color::srgb(1.0, 0.85, 0.5)),
            TextLayout::new_with_justify(JustifyText::Center),
            Node { position_type: PositionType::Absolute, top: Val::Px(74.0), left: Val::Percent(20.0), width: Val::Percent(60.0), ..default() },
            GlobalZIndex(60),
            AvisoTexto,
            Escena,
        ));
    }
    let (t, color) = if vida.radiacion > RADIACION_DANIA {
        (format!("RADIACIÓN {:.0}%: buscá un refugio", vida.radiacion * 100.0), Color::srgb(1.0, 0.35, 0.3))
    } else if vida.aviso_t > 0.0 {
        (vida.aviso.clone(), Color::srgb(1.0, 0.85, 0.5))
    } else {
        (String::new(), Color::WHITE)
    };
    for (mut txt, mut c) in &mut textos {
        if txt.0 != t {
            txt.0 = t.clone();
        }
        c.0 = color;
    }
}

/// QA de dev: `HA_VIDAPRUEBA=<prefijo>` (con `HA_AUTOPLAY=1`) planta al jugador en el pueblo
/// más grande, inyecta una elección, una guerra, presos, un lanzamiento, un refugio y una bomba,
/// y captura cada escena. No existe en Android.
#[cfg(not(target_os = "android"))]
#[allow(clippy::too_many_arguments)]
fn vida_prueba(
    mut civ: ResMut<Civ>,
    mut commands: Commands,
    mut exit: EventWriter<bevy::app::AppExit>,
    mut player: Query<&mut SimState, With<Player>>,
    mut cuadros: Local<u32>,
    mut sitio: Local<Option<(u32, f32, f32, u32)>>,
) {
    let Ok(prefijo) = crate::flags::var("HA_VIDAPRUEBA") else { return };
    let Some(m) = civ.mundo_mut() else { return };
    *cuadros += 1;
    let n = *cuadros;
    if n == 20 {
        let Some(p) = m.pueblos.values().filter(|p| p.vivo && p.cuerpo == 0).max_by(|a, b| a.pob.partial_cmp(&b.pob).unwrap()) else { return };
        let otro = m.pueblos.values().filter(|q| q.vivo && q.cuerpo == 0 && q.id != p.id).map(|q| q.id).next().unwrap_or(p.id);
        *sitio = Some((p.id, p.x, p.z, otro));
        eprintln!("ha-vidaprueba: pueblo {} en ({:.0},{:.0}) pob {:.0}", p.nombre, p.x, p.z, p.pob);
    }
    let Some((id, x, z, otro)) = *sitio else { return };
    let dia = m.dia;
    if n >= 20 && n < 700 {
        if let Ok(mut sim) = player.get_single_mut() {
            sim.body.x = x + 8.0;
            sim.body.z = z + 2.0;
        }
    }
    let disparar = |commands: &mut Commands, cuadro: u32| {
        let ruta = format!("{prefijo}-{cuadro}.png");
        commands
            .spawn(bevy::render::view::screenshot::Screenshot::primary_window())
            .observe(bevy::render::view::screenshot::save_to_disk(ruta));
    };
    match n {
        80 => {
            m.cronica.anotar(dia, Tipo::Eleccion, id, 0, "Elección en la plaza.".into());
            m.guerras.push(ha_shared::civ::pueblo::Guerra { id: 9999, bando_a: vec![id], bando_b: vec![otro], inicio: dia, avance: 0.0, causa: "prueba".into() });
        }
        100 => disparar(&mut commands, 100),
        140 => {
            m.cronica.anotar(dia, Tipo::Justicia, id, 1, "Ana va preso en la plaza por robar.".into());
            m.cronica.anotar(dia, Tipo::Justicia, id, 1, "Beto es desterrado de la plaza.".into());
        }
        200 => disparar(&mut commands, 200),
        260 => {
            m.cronica.anotar(dia, Tipo::Lanzamiento, id, 3, "Prueba lanza una nave tripulada hacia la Luna.".into());
        }
        300 => disparar(&mut commands, 300),
        340 => {
            m.registrar_refugio(x + 8.0, z + 9.0, 12, 0.0);
            if let Some(p) = m.pueblos.get_mut(&id) {
                p.saberes = p.saberes.union(ha_shared::civ::saber::Saberes((1u64 << ha_shared::civ::saber::N_SABERES) - 1));
            }
        }
        420 => disparar(&mut commands, 420),
        450 => {
            m.cronica.anotar(dia, Tipo::Detonacion, otro, id, "Prueba lanza un arma nuclear sobre la plaza.".into());
        }
        480 => disparar(&mut commands, 480),
        560 => disparar(&mut commands, 560),
        620 => {
            if let Some(p) = m.pueblos.get_mut(&id) {
                p.radiacion = 0.6;
            }
        }
        660 => disparar(&mut commands, 660),
        n if n >= 700 => {
            exit.send(bevy::app::AppExit::Success);
        }
        _ => {}
    }
}
