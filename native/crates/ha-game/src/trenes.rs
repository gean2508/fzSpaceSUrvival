//! **TRENES** (5 oct 2026): cada ciudad tiene una estación y una línea que la une con la ciudad vecina al este
//! (`ha_shared::rail`), con dos vías: el tren va por la de ida y vuelve por la de vuelta.
//!
//! * **La red** se calcula en un hilo de fondo (cuesta barrer cada corredor tile por tile) alrededor del jugador y se
//!   renueva cuando él se aleja. Hasta que está lista no hay vías: no aparece nada a medias.
//! * **Las vías y los andenes** son entidades de este módulo (no del chunk): un riel modular cada 2 tiles y baldosa de
//!   andén, creados sólo cerca del jugador y retirados al alejarse. Así la red puede llegar tarde sin dejar chunks sin
//!   vías.
//! * **Los trenes** (tres coches del pack: `vehicle/tren_exterior_*`) no guardan estado: su posición es una función
//!   pura del reloj (`rail::estado_del_tren`), así que siempre están donde el horario dice y se pueden ver llegar a
//!   la estación, abrir las puertas, esperar 20 s y salir.
//! * **Estación funcional**: con el tren parado y el jugador en el andén, el botón USAR dice ENTRAR; dentro viaja con
//!   el tren y SALIR (sólo parado en una estación) lo deja en el andén.
//! * **Peligro**: un tren en marcha lleva por delante a lo que esté sobre su vía (el mismo camino que las balas,
//!   como el atropello de los coches).
//!
//! Costo: el hilo corre una vez cada varios kilómetros; cerca de una línea hay ~100 rieles + los andenes + 3 coches.

use std::collections::HashMap;
use std::sync::{Arc, OnceLock, RwLock};

use bevy::prelude::*;
use ha_shared::rail::{self, Linea};
use ha_shared::topdown::TILE_M;
use ha_shared::CelestialBody;

use crate::game_phase::GamePhase;
use crate::scene::WorldContextRuntime;
use crate::sim::{Player, PlayerVisual, SimState};
use crate::vehiculos::Conduccion;
use crate::world2d::{active_surface_body, ActivePlanet};

/// Radio (en celdas de asentamiento) del barrido de la red alrededor del jugador.
const RADIO_RED: i64 = 3;
/// A cuántas celdas de asentamiento del centro de la última red se recalcula.
const RENUEVA_CELDAS: i64 = 2;
/// Tiles a cada lado del jugador donde existen vías y andenes.
const ALCANCE_VIAS: i64 = 110;
/// Largo de cada coche del tren (m): `tren_exterior_*` mide 288 / 320 / 320 px.
const COCHES: [(&str, f32); 3] = [("izquierda", 9.0), ("medio", 10.0), ("derecha", 10.0)];
const ALTO_COCHE: f32 = 4.0;
const COLORES: [&str; 5] = ["azul", "verde", "naranja", "gris", "blanco"];
const RIEL_ART: &str = "building/riel_gris_modular_vertical";
const ANDEN_ART: &str = "floor/tren_y_subte_baldosa_lila";

// ── La red, calculada en segundo plano ───────────────────────────────────────────────────────────────────────────

struct Red {
    clave: Option<(u32, u8)>,
    centro: (i64, i64),
    lineas: Arc<Vec<Linea>>,
    calculando: bool,
    /// Sube cada vez que llega una red nueva (los sistemas reconcilian cuando cambia).
    generacion: u32,
}

fn red() -> &'static RwLock<Red> {
    static R: OnceLock<RwLock<Red>> = OnceLock::new();
    R.get_or_init(|| RwLock::new(Red { clave: None, centro: (0, 0), lineas: Arc::new(Vec::new()), calculando: false, generacion: 0 }))
}

/// Las líneas conocidas ahora (vacío mientras la red se calcula) y la generación de la red.
pub fn lineas() -> (Arc<Vec<Linea>>, u32) {
    let r = red().read().unwrap();
    (r.lineas.clone(), r.generacion)
}

fn pedir_red(body: CelestialBody, px: i64, pz: i64) {
    let clave = (ha_shared::partida::epoca(), body as u8);
    {
        let r = red().read().unwrap();
        if r.calculando {
            return;
        }
        let lejos = (px - r.centro.0).abs().max((pz - r.centro.1).abs()) > RENUEVA_CELDAS * ha_shared::geography::SITE_PERIOD;
        if r.clave == Some(clave) && !lejos {
            return;
        }
    }
    red().write().unwrap().calculando = true;
    let _ = std::thread::Builder::new().name("ha-red-ferroviaria".into()).stack_size(8 << 20).spawn(move || {
        let t0 = std::time::Instant::now();
        let lineas = rail::lineas_cerca(body, px, pz, RADIO_RED);
        eprintln!("ha-trenes: red calculada en {:.1} s: {} línea(s) alrededor de ({px},{pz})", t0.elapsed().as_secs_f32(), lineas.len());
        let mut r = red().write().unwrap();
        r.clave = Some(clave);
        r.centro = (px, pz);
        r.lineas = Arc::new(lineas);
        r.calculando = false;
        r.generacion = r.generacion.wrapping_add(1);
    });
}

// ── Componentes ──────────────────────────────────────────────────────────────────────────────────────────────────

#[derive(Component)]
struct Pieza;

#[derive(Component)]
struct Tren {
    linea: Linea,
    color: usize,
    /// Dirección con la que está dibujado (para no rehacer los sprites cada cuadro).
    dibujado_hacia: i8,
}

#[derive(Component)]
struct Coche(usize);

#[derive(Resource, Default)]
struct Vistos {
    piezas: HashMap<(u32, u8, i64), Entity>,
    generacion: u32,
    trenes: HashMap<u32, Entity>,
    peligro_cd: f32,
}

pub struct TrenesPlugin;

impl Plugin for TrenesPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Vistos>()
            .add_systems(
                Update,
                (actualizar_red, limpiar_decoracion, reconciliar_vias, reconciliar_trenes, mover_trenes, subir_o_bajar, peligro_en_la_via)
                    .chain()
                    .after(crate::input::joystick::smooth_movement_intent)
                    .run_if(in_state(GamePhase::Playing)),
            )
            .add_systems(OnExit(GamePhase::Playing), limpiar);
    }
}

fn limpiar(
    mut commands: Commands,
    mut v: ResMut<Vistos>,
    mut cond: ResMut<Conduccion>,
    q: Query<Entity, Or<(With<Pieza>, With<Tren>)>>,
    mut player: Query<&mut Visibility, With<Player>>,
) {
    for e in &q {
        commands.entity(e).despawn_recursive();
    }
    *v = Vistos::default();
    cond.tren_cerca = false;
    cond.en_tren = None;
    for mut vis in &mut player {
        *vis = Visibility::Inherited;
    }
}

pub(crate) fn actualizar_red(ctx: Res<WorldContextRuntime>, planet: Res<ActivePlanet>, player: Query<&Transform, With<Player>>) {
    if !matches!(ctx.active, ha_shared::WorldContext::Overworld) {
        return;
    }
    let body = active_surface_body(&ctx, &planet);
    if !ha_shared::geography::has_civilisation(body) {
        return;
    }
    let Ok(p) = player.get_single() else { return };
    pedir_red(body, p.translation.x.floor() as i64, (-p.translation.y).floor() as i64);
}

/// **Cuando llega una línea nueva se barre la decoración que ya estaba plantada sobre ella**: el sembrado de decoración
/// (`worldgen::surface_decor_at`) deja de plantar en su corredor desde que la línea se midió, pero los árboles y piedras de
/// los chunks ya armados siguen ahí (y se veían en medio de las vías).
fn limpiar_decoracion(
    mut commands: Commands,
    mut visto: Local<u32>,
    decor: Query<(Entity, &Transform), With<crate::world2d::SurfaceDecor>>,
) {
    // La clave junta las dos generaciones: la del memo de líneas (sube al medir una) y la de la red publicada (llega
    // al final del hilo): la limpieza tiene que correr cuando LAS DOS ya están al día.
    let (lineas, gen_red) = lineas();
    let gen = rail::generacion().wrapping_mul(1_000_003).wrapping_add(gen_red);
    if gen == *visto {
        return;
    }
    *visto = gen;
    for (e, tf) in &decor {
        let (tx, tz) = (tf.translation.x.floor() as i64, (-tf.translation.y).floor() as i64);
        if lineas.iter().any(|l| {
            let [x0, z0, x1, z1] = l.caja();
            tx >= x0 - 1 && tx <= x1 + 1 && tz >= z0 - 1 && tz <= z1 + 1
        }) {
            commands.entity(e).despawn_recursive();
        }
    }
}

fn jugador_tile(player: &Query<&Transform, With<Player>>) -> Option<(i64, i64)> {
    player.get_single().ok().map(|p| (p.translation.x.floor() as i64, (-p.translation.y).floor() as i64))
}

// ── Vías y andenes ───────────────────────────────────────────────────────────────────────────────────────────────

fn reconciliar_vias(
    mut commands: Commands,
    mut vistos: ResMut<Vistos>,
    art: Res<crate::art2d::ArtLibrary>,
    ctx: Res<WorldContextRuntime>,
    player: Query<&Transform, With<Player>>,
    mut acc: Local<f32>,
    time: Res<Time>,
) {
    *acc += time.delta_secs();
    if *acc < 0.4 {
        return;
    }
    *acc = 0.0;
    let (lineas, gen) = lineas();
    let activo = matches!(ctx.active, ha_shared::WorldContext::Overworld);
    let Some((px, pz)) = jugador_tile(&player).filter(|_| activo) else {
        for (_, e) in vistos.piezas.drain() {
            commands.entity(e).despawn_recursive();
        }
        return;
    };
    if vistos.generacion != gen {
        // Llegó una red nueva: se rehace todo (las ids de línea pueden haber cambiado).
        for (_, e) in vistos.piezas.drain() {
            commands.entity(e).despawn_recursive();
        }
        vistos.generacion = gen;
    }
    // Qué piezas deberían existir ahora.
    let mut deseadas: Vec<(u32, u8, i64)> = Vec::new();
    for l in lineas.iter() {
        if (pz - l.z).abs() > ALCANCE_VIAS + 8 {
            continue;
        }
        let (x0, x1) = ((px - ALCANCE_VIAS).max(l.x_oeste() - rail::MEDIO_ANDEN), (px + ALCANCE_VIAS).min(l.x_este() + rail::MEDIO_ANDEN));
        if x0 > x1 {
            continue;
        }
        // Rieles: una pieza cada 2 tiles, por vía.
        for via in 0..2u8 {
            for s in (x0.div_euclid(2))..=(x1.div_euclid(2)) {
                deseadas.push((l.id, via, s));
            }
        }
        // Andenes: una baldosa por celda (clave aparte: `via` = 2 + fila del andén).
        for est in l.paradas {
            for x in (est - rail::MEDIO_ANDEN).max(x0)..=(est + rail::MEDIO_ANDEN).min(x1) {
                for f in 1..rail::SEPARACION_VIAS {
                    deseadas.push((l.id, 2 + f as u8, x));
                }
            }
        }
    }
    deseadas.sort_unstable();
    deseadas.dedup();
    // Retirar lo que ya no corresponde.
    let set: std::collections::HashSet<(u32, u8, i64)> = deseadas.iter().copied().collect();
    vistos.piezas.retain(|k, e| {
        let ok = set.contains(k);
        if !ok {
            commands.entity(*e).despawn_recursive();
        }
        ok
    });
    // Crear lo que falta (con tope por pasada, por si el jugador cae de golpe en medio de una línea).
    let mut creadas = 0;
    for k in deseadas {
        if vistos.piezas.contains_key(&k) || creadas >= 160 {
            continue;
        }
        let Some(l) = lineas.iter().find(|l| l.id == k.0) else { continue };
        let ent = if k.1 < 2 {
            let x = (k.2 * 2) as f32 + 1.0;
            let z = l.fila_via(k.1) as f32 + 0.5;
            // Riel modular de 2×1 tiles, plano sobre el suelo (por debajo de los pies de todo el mundo).
            art.world_sprite(RIEL_ART, Vec2::new(2.0 * TILE_M, TILE_M)).map(|(s, _)| {
                let ent = commands
                    .spawn((Pieza, s, Transform::from_xyz(x, -z, crate::layers::FLOOR + 0.03), Name::new("Riel")))
                    .id();
                crate::light2d::sin_sombra(&mut commands.entity(ent));
                ent
            })
        } else {
            let x = k.2 as f32 + 0.5;
            let z = (l.z + (k.1 - 2) as i64) as f32 + 0.5;
            art.world_sprite(ANDEN_ART, Vec2::splat(TILE_M)).map(|(s, _)| {
                let ent = commands
                    .spawn((Pieza, s, Transform::from_xyz(x, -z, crate::layers::FLOOR + 0.02), Name::new("Anden")))
                    .id();
                crate::light2d::sin_sombra(&mut commands.entity(ent));
                ent
            })
        };
        if let Some(e) = ent {
            vistos.piezas.insert(k, e);
            creadas += 1;
        }
    }
}

// ── Los trenes ───────────────────────────────────────────────────────────────────────────────────────────────────

fn reloj(time: &Time) -> f32 {
    (time.elapsed_secs_f64() % 86_400.0) as f32
}

fn reconciliar_trenes(
    mut commands: Commands,
    mut vistos: ResMut<Vistos>,
    ctx: Res<WorldContextRuntime>,
    player: Query<&Transform, With<Player>>,
    cond: Res<Conduccion>,
) {
    let (lineas, _) = lineas();
    let activo = matches!(ctx.active, ha_shared::WorldContext::Overworld);
    let Some((px, pz)) = jugador_tile(&player).filter(|_| activo) else { return };
    // Un tren existe mientras el jugador esté a menos de 260 tiles de su línea (o viaje en él).
    let mut vivos: Vec<u32> = Vec::new();
    for l in lineas.iter() {
        let dz = (pz - l.z).abs();
        let cerca = dz < 160 && px > l.x_oeste() - 260 && px < l.x_este() + 260;
        let viaja = cond.en_tren == Some(l.id);
        if !(cerca || viaja) {
            continue;
        }
        vivos.push(l.id);
        if vistos.trenes.contains_key(&l.id) {
            continue;
        }
        let e = commands
            .spawn((
                Tren { linea: *l, color: l.id as usize % COLORES.len(), dibujado_hacia: 0 },
                Transform::from_xyz(l.x_oeste() as f32, -(l.z as f32), 0.0),
                Visibility::Hidden,
                Name::new("Tren"),
            ))
            .id();
        vistos.trenes.insert(l.id, e);
    }
    vistos.trenes.retain(|id, e| {
        let ok = vivos.contains(id);
        if !ok {
            commands.entity(*e).despawn_recursive();
        }
        ok
    });
}

#[allow(clippy::too_many_arguments)]
fn mover_trenes(
    mut commands: Commands,
    time: Res<Time>,
    art: Res<crate::art2d::ArtLibrary>,
    mut trenes: Query<(Entity, &mut Tren, &mut Transform, &mut Visibility, Option<&Children>), Without<Coche>>,
    mut coches: Query<(&Coche, &mut Sprite)>,
) {
    let t = reloj(&time);
    for (e, mut tren, mut tf, mut vis, hijos) in &mut trenes {
        let st = rail::estado_del_tren(&tren.linea, t);
        let hacia: i8 = if st.via == 0 { 1 } else { -1 };
        // Se (re)dibujan los coches si cambió el sentido o todavía no existen.
        if tren.dibujado_hacia != hacia || hijos.is_none() {
            for h in hijos.into_iter().flat_map(|h| h.iter()) {
                commands.entity(*h).despawn_recursive();
            }
            tren.dibujado_hacia = hacia;
            let total: f32 = COCHES.iter().map(|c| c.1).sum();
            let mut x = -total * 0.5;
            let orden: Vec<usize> = if hacia > 0 { vec![0, 1, 2] } else { vec![2, 1, 0] };
            commands.entity(e).with_children(|p| {
                for i in orden {
                    let (nombre, largo) = COCHES[i];
                    let clave = format!("vehicle/tren_exterior_{}_{}", COLORES[tren.color], nombre);
                    let Some((mut s, _)) = art.world_sprite(&clave, Vec2::new(largo, ALTO_COCHE)) else {
                        x += largo;
                        continue;
                    };
                    s.flip_x = hacia < 0;
                    // La base del dibujo apoya en la fila de la vía; el cuerpo crece hacia el norte.
                    p.spawn((Coche(i), s, Transform::from_xyz(x + largo * 0.5, ALTO_COCHE * 0.5 - 0.35, 0.0)));
                    x += largo;
                }
            });
        }
        let z = tren.linea.fila_via(st.via) as f32 + 1.0;
        tf.translation = Vec3::new(st.x, -z, crate::layers::ysort(z));
        *vis = Visibility::Inherited;
        for h in hijos.into_iter().flat_map(|h| h.iter()) {
            if let Ok((_, mut s)) = coches.get_mut(*h) {
                let a = st.opacidad;
                if (s.color.alpha() - a).abs() > 0.01 {
                    s.color = s.color.with_alpha(a);
                }
            }
        }
    }
}

// ── Subir y bajar ────────────────────────────────────────────────────────────────────────────────────────────────

#[allow(clippy::too_many_arguments)]
fn subir_o_bajar(
    time: Res<Time>,
    actions: Res<crate::input::TouchActions>,
    gameplay_block: Res<crate::game_ui::UiGameplayBlock>,
    mut cond: ResMut<Conduccion>,
    mut player: Query<(&mut SimState, &mut PlayerVisual, &mut Visibility), With<Player>>,
    trenes: Query<&Tren>,
) {
    let t = reloj(&time);
    let Ok((mut sim, mut visual, mut vis)) = player.get_single_mut() else { return };
    // Viajando: el cuerpo va pegado al tren.
    if let Some(id) = cond.en_tren {
        let Some(tren) = trenes.iter().find(|t| t.linea.id == id) else {
            cond.en_tren = None;
            *vis = Visibility::Inherited;
            return;
        };
        let st = rail::estado_del_tren(&tren.linea, t);
        let fila = tren.linea.fila_via(st.via) as f32 + 0.5;
        let (x, z) = (st.x, fila);
        sim.body.x = x;
        sim.body.z = z;
        visual.from_x = x;
        visual.to_x = x;
        visual.from_z = z;
        visual.to_z = z;
        let parado = st.en_estacion.is_some() && st.vel == 0.0;
        cond.tren_parado = parado;
        if parado && actions.use_edge && !gameplay_block.blocked {
            // Bajar: al andén (la fila del medio), junto a la puerta.
            let zb = tren.linea.z as f32 + 2.5;
            sim.body.x = x + 1.5;
            sim.body.z = zb;
            visual.from_x = x + 1.5;
            visual.to_x = x + 1.5;
            visual.from_z = zb;
            visual.to_z = zb;
            cond.en_tren = None;
            cond.tren_parado = false;
            *vis = Visibility::Inherited;
        }
        return;
    }
    cond.tren_parado = false;
    // ¿Hay un tren parado junto a mí?
    let (px, pz) = (sim.body.x, sim.body.z);
    let mut cerca: Option<u32> = None;
    for tren in &trenes {
        let st = rail::estado_del_tren(&tren.linea, t);
        if st.en_estacion.is_some() && st.vel == 0.0 && st.opacidad > 0.9 {
            let fila = tren.linea.fila_via(st.via) as f32;
            if (px - st.x).abs() < 16.0 && pz > fila - 1.5 && pz < fila + 4.6 {
                cerca = Some(tren.linea.id);
            }
        }
    }
    cond.tren_cerca = cerca.is_some() && cond.vehiculo.is_none();
    if cond.tren_cerca && actions.use_edge && !gameplay_block.blocked {
        cond.en_tren = cerca;
        cond.tren_cerca = false;
        *vis = Visibility::Hidden;
    }
}

// ── Peligro ──────────────────────────────────────────────────────────────────────────────────────────────────────

fn peligro_en_la_via(
    mut commands: Commands,
    time: Res<Time>,
    mut vistos: ResMut<Vistos>,
    cond: Res<Conduccion>,
    mut player: Query<&mut SimState, With<Player>>,
    trenes: Query<&Tren>,
) {
    vistos.peligro_cd -= time.delta_secs();
    if vistos.peligro_cd > 0.0 {
        return;
    }
    vistos.peligro_cd = 0.25;
    let t = reloj(&time);
    for tren in &trenes {
        let st = rail::estado_del_tren(&tren.linea, t);
        if st.vel.abs() < 4.0 {
            continue;
        }
        let fila = tren.linea.fila_via(st.via) as f32;
        let signo = st.vel.signum();
        let frente = st.x + signo * 14.5;
        // El frente barre 7 m por delante (lo que recorre en 0,25 s a 28 m/s), a lo ancho de la vía.
        let a = Vec2::new(frente - signo * 7.5, fila + 0.5);
        let b = Vec2::new(frente + signo * 1.0, fila + 0.5);
        commands.spawn(crate::armas::Bala {
            pos: b,
            prev: a,
            vel: Vec2::ZERO,
            dano: 400.0,
            origen: a,
            alcance: 0.01,
            atraviesa: 16,
            golpeados: Vec::new(),
            viva: true,
            impacto: crate::audio::Sfx::BulletHit,
            rayo: false,
        });
        // Y el jugador a pie, si está en la vía.
        if cond.activa() {
            continue;
        }
        if let Ok(mut sim) = player.get_single_mut() {
            let (px, pz) = (sim.body.x, sim.body.z);
            let en_x = (a.x.min(b.x) - 0.6..=a.x.max(b.x) + 0.6).contains(&px);
            if en_x && (pz - (fila + 0.5)).abs() < 0.9 {
                sim.hp = (sim.hp - 60.0).max(1.0);
                sim.body.z = fila + 2.5; // lo tira al andén
            }
        }
    }
}
