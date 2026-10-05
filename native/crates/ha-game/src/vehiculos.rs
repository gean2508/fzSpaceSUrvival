//! **VEHÍCULOS** (5 oct 2026): coches y colectivos que se manejan, chocan, se rompen, explotan y circulan solos por
//! las calles — el tránsito de la ciudad.
//!
//! * **Física** (`paso`): modelo cinemático de bicicleta, la receta de los juegos cenitales (GTA 1/2, Micro Machines):
//!   aceleración con tope, resistencia, freno más fuerte que el motor, giro que se cierra con la velocidad y marcha
//!   atrás. Sin integrador de fuerzas: estable a cualquier `dt` y sin rebotes raros.
//! * **Control táctil**: el MISMO stick de caminar. La dirección del stick es hacia dónde se quiere ir y su recorrido la
//!   velocidad (*twin-stick*): no hay volante ni pedales que ocupen el pulgar derecho, que sigue libre para el stick de
//!   puntería (tirar desde el coche). Stick hacia atrás = marcha atrás. En escritorio, WASD.
//! * **Entrar y salir**: el botón USAR dice ENTRAR cuando hay un coche a menos de [`ALCANCE_ENTRAR`] y SALIR adentro.
//!   Subirse a uno con conductor se lo roba (el conductor huye, no hay consecuencias).
//! * **Choques**: contra el mundo (las mismas cajas que frenan al jugador, `Obstaculos`), contra otros coches y contra
//!   el jugador a pie. Por encima de [`V_CHOQUE`] el golpe cuesta vida al coche; a 0 explota, queda el chasis y el
//!   conductor sale despedido con daño.
//! * **Atropellos**: un coche rápido le pasa por encima a infectados, animales y aldeanos con el MISMO camino que las balas
//!   (un `Bala` de un solo cuadro delante del capó): cada módulo aplica su propia muerte, sin duplicar reglas.
//! * **Tránsito** (`trafico`): coches que circulan solos por el carril de `roads::lane_at` (mano derecha), centrados en
//!   su mitad de calzada, que frenan ante lo que tienen delante, doblan en las bocacalles y dan la vuelta donde la calle
//!   se acaba. Aparecen en un anillo alrededor del jugador (nunca a la vista) y se retiran al alejarse.
//! * **Balas**: se les puede disparar; la carrocería aguanta y se incendia, y los NPC al volante aceleran y huyen.
//!
//! Costo (regla 2): hay un tope de coches vivos (4 en Android, 8 en escritorio); el pensamiento de la IA corre a 8 Hz y
//! la consulta de carril es de unas decenas de celdas por coche y por vez.

use bevy::prelude::*;
use ha_shared::topdown::TILE_M;

use crate::audio::{PlaySfxAt, Sfx};
use crate::camera::CameraShakeEvent;
use crate::game_phase::GamePhase;
use crate::input::{MovementIntent, TouchActions};
use crate::scene::WorldContextRuntime;
use crate::sim::{Player, PlayerVisual, SimState};
use crate::world2d::{active_surface_body, ActivePlanet};

// ── Números ──────────────────────────────────────────────────────────────────────────────────────────────────────
/// A qué distancia (m) del jugador se puede entrar a un coche.
pub const ALCANCE_ENTRAR: f32 = 3.2;
/// Por debajo de esta velocidad (m/s) un roce no cuenta como choque.
pub const V_CHOQUE: f32 = 3.0;
/// Radio de los círculos de colisión contra el mundo (m).
const R_COCHE: f32 = 0.8;
/// m por pixel del dibujo del pack (32 px = 1 tile = 1 m).
const M_POR_PX: f32 = 1.0 / 32.0;
/// Cuántos coches a la vez.
const TOPE_TRAFICO: usize = if cfg!(target_os = "android") { 4 } else { 8 };
/// Anillo de aparición (m): lejos de lo que se ve (a pie se ven ~25 m).
const ANILLO: (f32, f32) = (42.0, 90.0);
/// Más lejos que esto el coche del tránsito se retira.
const RETIRO: f32 = 140.0;
/// Segundos entre intentos de aparición.
const CADA_APARICION: f32 = 1.2;
/// Segundos que un chasis quemado sigue en el mundo.
const VIDA_CHASIS: f32 = 150.0;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Clase {
    Auto,
    Colectivo,
}

/// Lo que cambia entre clases: tamaño de la caja de choque, motor, vida.
#[derive(Clone, Copy, Debug)]
pub struct Ficha {
    /// Largo y ancho de la huella en el suelo (m).
    pub largo: f32,
    pub ancho: f32,
    pub v_max: f32,
    pub v_atras: f32,
    pub acel: f32,
    pub freno: f32,
    pub vida: f32,
}

impl Clase {
    pub fn ficha(self) -> Ficha {
        match self {
            Clase::Auto => Ficha { largo: 3.6, ancho: 1.7, v_max: 20.0, v_atras: 5.0, acel: 7.0, freno: 16.0, vida: 120.0 },
            Clase::Colectivo => Ficha { largo: 7.0, ancho: 2.2, v_max: 13.0, v_atras: 3.0, acel: 3.6, freno: 11.0, vida: 280.0 },
        }
    }
}

// ── Física pura (probada sin Bevy) ───────────────────────────────────────────────────────────────────────────────

/// Estado cinemático de un coche.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Cuerpo {
    pub pos: Vec2,
    /// Rumbo (rad): `atan2(z, x)` — 0 = este, π/2 = sur (pantalla abajo).
    pub rumbo: f32,
    /// m/s con signo (negativo = marcha atrás).
    pub vel: f32,
}

/// Lo que quiere el conductor: hacia dónde apuntar el coche y a qué velocidad (con signo).
#[derive(Clone, Copy, Debug)]
pub struct Mando {
    pub rumbo: f32,
    pub v_obj: f32,
}

/// Ángulo envuelto a (-π, π].
pub fn envolver(a: f32) -> f32 {
    let mut a = a % std::f32::consts::TAU;
    if a > std::f32::consts::PI {
        a -= std::f32::consts::TAU;
    } else if a <= -std::f32::consts::PI {
        a += std::f32::consts::TAU;
    }
    a
}

pub fn dir_de(rumbo: f32) -> Vec2 {
    Vec2::new(rumbo.cos(), rumbo.sin())
}

/// **Del stick al mando** (twin-stick): la dirección del stick es hacia dónde ir. Si apunta casi hacia atrás del coche
/// se maneja en reversa hacia ese lado (el rumbo objetivo se da vuelta), así el pulgar nunca tiene que "dar la
/// vuelta" a mano para retroceder.
pub fn mando_de_stick(rumbo_actual: f32, stick: Vec2, f: &Ficha) -> Mando {
    let mag = stick.length().clamp(0.0, 1.0);
    if mag < 0.12 {
        return Mando { rumbo: rumbo_actual, v_obj: 0.0 };
    }
    let objetivo = stick.y.atan2(stick.x);
    let d = envolver(objetivo - rumbo_actual);
    if d.abs() > 2.2 {
        // Hacia atrás: el coche apunta al lado contrario y retrocede.
        Mando { rumbo: envolver(objetivo + std::f32::consts::PI), v_obj: -mag * f.v_atras }
    } else {
        Mando { rumbo: objetivo, v_obj: mag * f.v_max }
    }
}

/// Un paso de física SIN colisión. Devuelve la nueva posición deseada (el llamador la prueba contra el mundo).
pub fn paso(c: &mut Cuerpo, m: Mando, f: &Ficha, dt: f32) -> Vec2 {
    let dt = dt.min(0.05);
    // Acelerador/freno: lo que falta de velocidad, saturado.
    let falta = m.v_obj - c.vel;
    let frena = c.vel * falta < 0.0 || (m.v_obj == 0.0 && c.vel != 0.0);
    let a = if frena { f.freno } else { f.acel };
    let dv = (falta.signum() * a * dt).clamp(-falta.abs(), falta.abs());
    c.vel += dv;
    // Resistencia (el coche rueda y se frena solo).
    c.vel -= c.vel * 0.12 * dt;
    if m.v_obj == 0.0 && c.vel.abs() < 0.15 {
        c.vel = 0.0;
    }
    c.vel = c.vel.clamp(-f.v_atras, f.v_max);
    // Giro: ángulo de volante que se cierra con la velocidad. Con el coche casi parado puede girar poco.
    let d = envolver(m.rumbo - c.rumbo);
    let sentido = if c.vel < -0.05 { -1.0 } else { 1.0 };
    let volante = (d * 2.2).clamp(-1.0, 1.0) * sentido * 0.62 / (1.0 + c.vel.abs() / 14.0);
    let base = (f.largo * 0.62).max(1.0);
    c.rumbo = envolver(c.rumbo + c.vel / base * volante.tan() * dt);
    c.pos + dir_de(c.rumbo) * c.vel * dt
}

/// ¿El segmento `a→b` toca la caja orientada `(centro, rumbo, largo, ancho)`? (Liang–Barsky en el marco del coche.)
pub fn segmento_toca_caja(a: Vec2, b: Vec2, centro: Vec2, rumbo: f32, largo: f32, ancho: f32) -> bool {
    let (c, s) = (rumbo.cos(), rumbo.sin());
    let local = |p: Vec2| {
        let d = p - centro;
        Vec2::new(d.x * c + d.y * s, -d.x * s + d.y * c)
    };
    let (p0, p1) = (local(a), local(b));
    let d = p1 - p0;
    let (mut t0, mut t1) = (0.0f32, 1.0f32);
    for (p, q, lim) in [(p0.x, d.x, largo * 0.5), (p0.y, d.y, ancho * 0.5)] {
        if q.abs() < 1e-6 {
            if p.abs() > lim {
                return false;
            }
        } else {
            let (mut ta, mut tb) = ((-lim - p) / q, (lim - p) / q);
            if ta > tb {
                std::mem::swap(&mut ta, &mut tb);
            }
            t0 = t0.max(ta);
            t1 = t1.min(tb);
            if t0 > t1 {
                return false;
            }
        }
    }
    true
}

/// Octante de `roads::lane_at` → dirección en el suelo `(x, z)` (norte = z negativo).
pub fn octante_a_vec(o: u8) -> Vec2 {
    match o {
        0 => Vec2::new(1.0, 0.0),
        2 => Vec2::new(0.0, -1.0),
        4 => Vec2::new(-1.0, 0.0),
        _ => Vec2::new(0.0, 1.0),
    }
}

/// Giro a la derecha (en pantalla, con z hacia abajo) de un vector.
pub fn derecha_de(v: Vec2) -> Vec2 {
    Vec2::new(-v.y, v.x)
}

// ── Componentes ──────────────────────────────────────────────────────────────────────────────────────────────────

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Conductor {
    Nadie,
    Jugador,
    Npc,
}

#[derive(Component, Clone, Debug)]
pub struct Vehiculo {
    pub clase: Clase,
    pub clave: String,
    pub chasis: Option<String>,
    pub cuerpo: Cuerpo,
    pub vida: f32,
    pub conductor: Conductor,
    pub destruido: bool,
    /// Segundos desde la explosión (el chasis se retira solo).
    pub edad_chasis: f32,
    /// Dirección del dibujo (0=D, 1=A, 2=I, 3=Ab), con histéresis.
    dir4: u8,
    /// Pose y tamaño con el que está dibujado ahora (para no rehacer el sprite cada cuadro).
    pose: (u8, bool),
    humo_t: f32,
    atropello_cd: f32,
    choque_cd: f32,
    ia: Ia,
}

#[derive(Clone, Copy, Debug, Default)]
struct Ia {
    rumbo_obj: f32,
    v_obj: f32,
    piensa_en: f32,
    bloqueado: f32,
    perdido: f32,
    mem: Rumbo,
    /// Huye (le dispararon): más rápido y sin respetar nada.
    panico: f32,
    semilla: u32,
}

/// Quién maneja y a qué coche está cerca.
#[derive(Resource, Default)]
pub struct Conduccion {
    pub vehiculo: Option<Entity>,
    pub cerca: Option<Entity>,
    /// Texto del medidor (velocidad y vida), para el HUD.
    pub medidor: String,
}

impl Conduccion {
    pub fn activa(&self) -> bool {
        self.vehiculo.is_some()
    }

    /// El botón USAR se ocupa de entrar/salir.
    pub fn boton_activo(&self) -> bool {
        self.vehiculo.is_some() || self.cerca.is_some()
    }

    pub fn etiqueta(&self) -> &'static str {
        if self.vehiculo.is_some() { "SALIR" } else { "ENTRAR" }
    }
}

#[derive(Component)]
struct Humo {
    vida: f32,
    sube: f32,
}

#[derive(Component)]
struct MedidorHud;

#[derive(Resource, Default)]
struct Trafico {
    acc: f32,
    tick: u32,
}

pub struct VehiculosPlugin;

impl Plugin for VehiculosPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Conduccion>()
            .init_resource::<Trafico>()
            .add_systems(
                Update,
                (
                    buscar_cercano,
                    entrar_o_salir,
                    pensar_ia,
                    mover_vehiculos,
                    choques_entre_coches,
                    dibujar_vehiculos,
                    humo_y_fuego,
                    medidor,
                )
                    .chain()
                    .run_if(in_state(GamePhase::Playing)),
            )
            .add_systems(Update, aparecer_trafico.run_if(in_state(GamePhase::Playing)))
            .add_systems(
                Update,
                (atropellar, balas_a_vehiculos)
                    .chain()
                    .after(crate::armas::ArmasSet::Mover)
                    .before(crate::armas::ArmasSet::Impactos)
                    .run_if(in_state(GamePhase::Playing)),
            )
            .add_systems(OnExit(GamePhase::Playing), limpiar);
    }
}

fn limpiar(
    mut commands: Commands,
    mut cond: ResMut<Conduccion>,
    q: Query<Entity, Or<(With<Vehiculo>, With<Humo>, With<MedidorHud>)>>,
    mut player: Query<&mut Visibility, With<Player>>,
) {
    for e in &q {
        commands.entity(e).despawn_recursive();
    }
    *cond = Conduccion::default();
    for mut v in &mut player {
        *v = Visibility::Inherited;
    }
}

fn hash(mut s: u32) -> u32 {
    s ^= s << 13;
    s ^= s >> 17;
    s ^= s << 5;
    s.wrapping_mul(2_654_435_761)
}

fn azar01(s: &mut u32) -> f32 {
    *s = hash(*s | 1);
    (*s >> 8) as f32 / 16_777_216.0
}

// ── Aparición del tránsito ───────────────────────────────────────────────────────────────────────────────────────

const AUTOS: &[&str] = &[
    "vehicle/auto_1", "vehicle/auto_2", "vehicle/auto_3", "vehicle/auto_4", "vehicle/auto_5", "vehicle/auto_6",
    "vehicle/auto_7", "vehicle/auto_8", "vehicle/auto_9", "vehicle/auto_10", "vehicle/auto_11", "vehicle/auto_12",
    "vehicle/auto_13", "vehicle/auto_14", "vehicle/auto_15", "vehicle/auto_16", "vehicle/auto_17", "vehicle/auto_18",
    "vehicle/auto_19", "vehicle/auto_20", "vehicle/auto_21", "vehicle/auto_22", "vehicle/auto_23", "vehicle/auto_24",
    "vehicle/auto_25", "vehicle/auto_26",
];
const COLECTIVOS: &[&str] = &["vehicle/colectivo_1", "vehicle/colectivo_2", "vehicle/colectivo_3", "vehicle/colectivo_4", "vehicle/colectivo_5", "vehicle/colectivo_6"];
const POSES: [&str; 4] = ["derecha", "arriba", "izquierda", "abajo"];

fn tiene_poses(art: &crate::art2d::ArtLibrary, clave: &str) -> bool {
    POSES.iter().all(|p| art.has_pose(clave, p))
}

#[allow(clippy::too_many_arguments)]
fn aparecer_trafico(
    mut commands: Commands,
    time: Res<Time>,
    mut tr: ResMut<Trafico>,
    ctx: Res<WorldContextRuntime>,
    planet: Res<ActivePlanet>,
    art: Res<crate::art2d::ArtLibrary>,
    obs: crate::obstaculos::Obstaculos,
    player: Query<&Transform, With<Player>>,
    coches: Query<&Vehiculo>,
) {
    tr.acc += time.delta_secs();
    if tr.acc < CADA_APARICION {
        return;
    }
    tr.acc = 0.0;
    tr.tick = tr.tick.wrapping_add(1);
    if !matches!(ctx.active, ha_shared::WorldContext::Overworld) {
        return;
    }
    let body = active_surface_body(&ctx, &planet);
    if !ha_shared::geography::has_civilisation(body) {
        return;
    }
    let vivos = coches.iter().filter(|v| !v.destruido && v.conductor != Conductor::Jugador).count();
    if vivos >= TOPE_TRAFICO {
        return;
    }
    let Ok(ptf) = player.get_single() else { return };
    let p = Vec2::new(ptf.translation.x, -ptf.translation.y);
    let mut s = hash(tr.tick.wrapping_mul(0x9e37_79b9) ^ (p.x as i32 as u32) ^ ((p.y as i32 as u32) << 7));
    for _ in 0..14 {
        let ang = azar01(&mut s) * std::f32::consts::TAU;
        let rad = ANILLO.0 + azar01(&mut s) * (ANILLO.1 - ANILLO.0);
        let q = p + dir_de(ang) * rad;
        let (tx, tz) = (q.x.floor() as i64, q.y.floor() as i64);
        let Some(oct) = ha_shared::roads::lane_at(body, tx, tz) else { continue };
        let centro = Vec2::new(tx as f32 + 0.5, tz as f32 + 0.5);
        if coches.iter().any(|v| v.cuerpo.pos.distance(centro) < 14.0) {
            continue;
        }
        if !obs.libre(body, centro.x, centro.y, R_COCHE) {
            continue;
        }
        let colectivo = azar01(&mut s) < 0.12;
        let (clase, lista) = if colectivo { (Clase::Colectivo, COLECTIVOS) } else { (Clase::Auto, AUTOS) };
        let candidatos: Vec<&&str> = lista.iter().filter(|k| tiene_poses(&art, k)).collect();
        if candidatos.is_empty() {
            return;
        }
        let clave = (**candidatos[(azar01(&mut s) * candidatos.len() as f32) as usize % candidatos.len()]).to_string();
        let rumbo = octante_a_vec(oct).to_angle();
        let f = clase.ficha();
        commands.spawn((
            Vehiculo {
                clase,
                clave,
                chasis: None,
                cuerpo: Cuerpo { pos: centro, rumbo, vel: 0.0 },
                vida: f.vida,
                conductor: Conductor::Npc,
                destruido: false,
                edad_chasis: 0.0,
                dir4: 255,
                pose: (255, false),
                humo_t: 0.0,
                atropello_cd: 0.0,
                choque_cd: 0.0,
                ia: Ia { rumbo_obj: rumbo, v_obj: 0.0, mem: Rumbo { semilla: s, ..default() }, ..default() },
            },
            Sprite::default(),
            Transform::from_xyz(centro.x, -centro.y, crate::layers::ysort(centro.y)),
            Visibility::Hidden,
            Name::new("Vehiculo"),
        ));
        return;
    }
}

// ── Cercanía, entrar y salir ─────────────────────────────────────────────────────────────────────────────────────

fn buscar_cercano(
    mut cond: ResMut<Conduccion>,
    flight: Res<crate::flight::Flight>,
    fly: Res<crate::creative_fly::CreativeFlight>,
    player: Query<&Transform, With<Player>>,
    coches: Query<(Entity, &Vehiculo)>,
) {
    if cond.vehiculo.is_some() {
        cond.cerca = None;
        return;
    }
    let Ok(ptf) = player.get_single() else { return };
    if flight.active || fly.flying() {
        cond.cerca = None;
        return;
    }
    let p = Vec2::new(ptf.translation.x, -ptf.translation.y);
    let mut mejor: Option<(Entity, f32)> = None;
    for (e, v) in &coches {
        if v.destruido {
            continue;
        }
        let f = v.clase.ficha();
        // Distancia al borde de la caja, no al centro: un colectivo mide 7 m.
        let d = (v.cuerpo.pos.distance(p) - f.largo * 0.5).max(0.0);
        if d <= ALCANCE_ENTRAR && mejor.map_or(true, |(_, m)| d < m) {
            mejor = Some((e, d));
        }
    }
    let nuevo = mejor.map(|(e, _)| e);
    if cond.cerca != nuevo {
        cond.cerca = nuevo;
    }
}

#[allow(clippy::too_many_arguments)]
fn entrar_o_salir(
    mut cond: ResMut<Conduccion>,
    actions: Res<TouchActions>,
    gameplay_block: Res<crate::game_ui::UiGameplayBlock>,
    ctx: Res<WorldContextRuntime>,
    planet: Res<ActivePlanet>,
    obs: crate::obstaculos::Obstaculos,
    mut player: Query<(&mut SimState, &mut PlayerVisual, &mut Visibility), With<Player>>,
    mut coches: Query<&mut Vehiculo>,
) {
    if gameplay_block.blocked || !actions.use_edge {
        return;
    }
    let Ok((mut sim, mut visual, mut vis)) = player.get_single_mut() else { return };
    let body = active_surface_body(&ctx, &planet);
    if let Some(e) = cond.vehiculo {
        // SALIR: por el costado libre, o por detrás, o encima.
        let Ok(mut v) = coches.get_mut(e) else {
            cond.vehiculo = None;
            *vis = Visibility::Inherited;
            return;
        };
        let f = v.clase.ficha();
        let d = dir_de(v.cuerpo.rumbo);
        let lado = derecha_de(d);
        let salida = [lado, -lado, -d, d]
            .into_iter()
            .map(|dir| v.cuerpo.pos + dir * (f.ancho * 0.5 + 0.9 + if dir == d || dir == -d { f.largo * 0.5 - f.ancho * 0.5 } else { 0.0 }))
            .find(|p| obs.libre(body, p.x, p.y, ha_shared::topdown::PLAYER_RADIUS))
            .unwrap_or(v.cuerpo.pos + lado * (f.ancho * 0.5 + 0.9));
        v.conductor = Conductor::Nadie;
        sim.body.x = salida.x;
        sim.body.z = salida.y;
        visual.from_x = salida.x;
        visual.to_x = salida.x;
        visual.from_z = salida.y;
        visual.to_z = salida.y;
        *vis = Visibility::Inherited;
        cond.vehiculo = None;
        return;
    }
    let Some(e) = cond.cerca else { return };
    let Ok(mut v) = coches.get_mut(e) else { return };
    if v.destruido {
        return;
    }
    // Entrar: si tenía conductor, lo bajamos (se lo roba).
    v.conductor = Conductor::Jugador;
    v.ia.perdido = 0.0;
    cond.vehiculo = Some(e);
    cond.cerca = None;
    *vis = Visibility::Hidden;
}

// ── IA del tránsito ──────────────────────────────────────────────────────────────────────────────────────────────

fn celda(p: Vec2) -> (i64, i64) {
    (p.x.floor() as i64, p.y.floor() as i64)
}

fn lane(body: ha_shared::CelestialBody, p: Vec2) -> Option<u8> {
    let (tx, tz) = celda(p);
    ha_shared::roads::lane_at(body, tx, tz)
}

/// Centro del carril: cuántas celdas seguidas del mismo sentido hay a cada lado (perpendicular a la marcha) y el
/// punto medio, como desplazamiento con signo a lo largo de `perp`.
fn centro_de_carril(body: ha_shared::CelestialBody, p: Vec2, oct: u8, perp: Vec2) -> f32 {
    let mut lado = |signo: f32| {
        let mut n = 0.0;
        for k in 1..=8 {
            if lane(body, p + perp * signo * k as f32) == Some(oct) {
                n = k as f32;
            } else {
                break;
            }
        }
        n
    };
    let (a, b) = (lado(1.0), lado(-1.0));
    // Medio del carril respecto del CENTRO DE LA CELDA, menos lo que el coche ya está corrido dentro de su celda:
    // continuo (sin escalón al cruzar de celda), que es lo que evita que el coche serpentee.
    let centro_celda = Vec2::new(p.x.floor() + 0.5, p.y.floor() + 0.5);
    (a - b) * 0.5 - (p - centro_celda).dot(perp)
}

#[allow(clippy::too_many_arguments)]
fn pensar_ia(
    time: Res<Time>,
    ctx: Res<WorldContextRuntime>,
    planet: Res<ActivePlanet>,
    player: Query<&Transform, With<Player>>,
    mut commands: Commands,
    mut q: Query<(Entity, &mut Vehiculo)>,
) {
    let body = active_surface_body(&ctx, &planet);
    let dt = time.delta_secs();
    let Ok(ptf) = player.get_single() else { return };
    let jugador = Vec2::new(ptf.translation.x, -ptf.translation.y);
    let otros: Vec<(Entity, Vec2, f32)> = q.iter().map(|(e, v)| (e, v.cuerpo.pos, v.cuerpo.vel)).collect();
    for (e, mut v) in &mut q {
        if v.conductor != Conductor::Npc || v.destruido {
            continue;
        }
        v.ia.piensa_en -= dt;
        v.ia.panico = (v.ia.panico - dt).max(0.0);
        if v.ia.piensa_en > 0.0 {
            continue;
        }
        v.ia.piensa_en = 0.125;
        let pos = v.cuerpo.pos;
        let f = v.clase.ficha();
        let tope = if v.ia.panico > 0.0 { f.v_max } else { f.v_max * 0.45 };
        let Some(oct) = lane(body, pos) else {
            // Se salió de la calzada (choque, empujón): frena y, si no vuelve, se retira.
            v.ia.v_obj = 0.0;
            v.ia.perdido += 0.125;
            if v.ia.perdido > 8.0 {
                commands.entity(e).despawn_recursive();
            }
            continue;
        };
        v.ia.perdido = 0.0;
        let vel_abs = v.cuerpo.vel.abs();
        let (rumbo_obj, v_factor) = decidir_rumbo(body, pos, v.cuerpo.rumbo, oct, vel_abs, 0.125, &mut v.ia.mem);
        v.ia.rumbo_obj = rumbo_obj;
        if v.ia.mem.sin_salida {
            // Fin de calle: si nadie lo ve se retira ya; si no, espera unos segundos y se va.
            v.ia.bloqueado += 0.125;
            if pos.distance(jugador) > 35.0 || v.ia.bloqueado > 6.0 {
                commands.entity(e).despawn_recursive();
                continue;
            }
        }
        let d0 = dir_de(rumbo_obj);
        // Velocidad: tope por clase, más lento si va a doblar, y frena ante lo que tiene delante.
        let mut vel = tope * if v.ia.panico > 0.0 { 1.0 } else { v_factor };
        let h = dir_de(v.cuerpo.rumbo);
        let bloqueo = |otro: Vec2| {
            let r = otro - pos;
            let adelante = r.dot(h);
            adelante > 0.0 && adelante < 4.0 + v.cuerpo.vel.abs() * 0.9 && (r - h * adelante).length() < f.ancho * 0.5 + 1.1
        };
        let mut bloqueado = bloqueo(jugador);
        for (oe, op, _) in &otros {
            if *oe != e && bloqueo(*op) {
                bloqueado = true;
            }
        }
        if bloqueado && v.ia.panico <= 0.0 {
            vel = 0.0;
            v.ia.bloqueado += 0.125;
        } else {
            v.ia.bloqueado = 0.0;
        }
        v.ia.v_obj = vel;
    }
}


/// Punto central del carril de la celda de `p` (sobre el eje `perp`; a lo largo, el centro de la celda).
fn centro_carril_abs(body: ha_shared::CelestialBody, p: Vec2, oct: u8, perp: Vec2) -> Vec2 {
    let centro_celda = Vec2::new(p.x.floor() + 0.5, p.y.floor() + 0.5);
    centro_celda + perp * (centro_de_carril(body, centro_celda, oct, perp))
}

/// Un giro decidido: se sigue derecho hasta el comienzo de una curva de Bézier cuadrática (entrada en el carril propio,
/// vértice en el cruce de los dos ejes de carril, salida en el carril nuevo) y se recorre por persecución pura. Una
/// curva y no un rumbo crudo: el coche tiene radio de giro y apuntar «a la calle nueva» lo hacía pasarse del carril.
#[derive(Clone, Copy, Debug)]
pub struct Giro {
    pub oct_t: u8,
    pub resta: f32,
    /// Dirección de la calle de la que viene y hacia la que va.
    pub d0: Vec2,
    pub c: Vec2,
    /// Vértice de la curva: donde se cruzan los ejes de los dos carriles.
    pub apice: Vec2,
    /// Largo (m) de la entrada y de la salida de la curva.
    pub radio: f32,
}

impl Giro {
    pub fn entrada(&self) -> Vec2 {
        self.apice - self.d0 * self.radio
    }
    pub fn salida(&self) -> Vec2 {
        self.apice + self.c * self.radio
    }
    pub fn punto(&self, t: f32) -> Vec2 {
        let (p0, p2) = (self.entrada(), self.salida());
        p0 * (1.0 - t) * (1.0 - t) + self.apice * 2.0 * (1.0 - t) * t + p2 * t * t
    }
    /// Parámetro de la curva más cercano a `pos`.
    pub fn progreso(&self, pos: Vec2) -> f32 {
        (0..=16)
            .map(|k| k as f32 / 16.0)
            .min_by(|a, b| self.punto(*a).distance_squared(pos).total_cmp(&self.punto(*b).distance_squared(pos)))
            .unwrap_or(0.0)
    }
}

/// Radio (m) al que se empieza a doblar a la velocidad de giro (3,6 m/s).
const RADIO_GIRO: f32 = 5.0;

/// Memoria del conductor entre dos pensamientos.
#[derive(Clone, Copy, Debug, Default)]
pub struct Rumbo {
    pub giro_cd: f32,
    pub semilla: u32,
    /// Giro en curso: octante de la calle a la que va, segundos que le quedan y el punto del carril nuevo al que
    /// apunta (persecución pura: el coche traza el arco hacia el CENTRO del carril, no hacia el rumbo crudo). **Se compromete**: en medio de la
    /// bocacalle el carril de la celda sigue siendo el de la calle de donde viene, y sin esto el coche se arrepentía
    /// del giro a cada pensamiento y terminaba en el cantero.
    pub compromiso: Option<Giro>,
    /// Último sentido de marcha confiable: dentro de una bocacalle el carril de la celda no dice nada útil (la
    /// retícula le da el de la calle cruzada), así que ahí se conserva el de antes.
    pub ultimo_oct: Option<u8>,
    /// Eligió dar la vuelta porque la calle se acaba: quien maneja la IA puede retirar el coche si nadie lo ve.
    pub sin_salida: bool,
}

fn octante_de(d: Vec2) -> u8 {
    if d.x > 0.5 { 0 } else if d.y < -0.5 { 2 } else if d.x < -0.5 { 4 } else { 6 }
}

/// **Hacia dónde conduce la IA desde `pos`**, que está en un carril de octante `oct`. Devuelve `(rumbo objetivo,
/// factor de velocidad 0..1)`. Sólo mira el mapa: se prueba sobre calles reales sin abrir el juego.
///
/// Sigue el sentido del carril, se centra en su mitad de calzada, dobla en las bocacalles (a veces, y sólo hacia carriles
/// que van PARA donde dobla), da la vuelta donde la calle se acaba y llega despacio a todo lo anterior.
pub fn decidir_rumbo(body: ha_shared::CelestialBody, pos: Vec2, rumbo_actual: f32, oct: u8, vel_abs: f32, dt: f32, st: &mut Rumbo) -> (f32, f32) {
    st.giro_cd = (st.giro_cd - dt).max(0.0);
    st.sin_salida = false;
    // **El sentido de marcha es el del PROPIO coche** (cardinal más cercana a su rumbo), no el que diga la celda: dentro de
    // una bocacalle la retícula le da a cada celda el sentido de la calle que cruza (medido: el carril este de una
    // avenida muestra «<<<<<» al pasar por un cruce), y seguirlo hacía dar media vuelta al coche en pleno cruce.
    // El octante de la celda sólo se usa para arrancar (`aparecer_trafico`) y para centrarse en el carril.
    let _ = oct;
    let oct = octante_de(dir_de(rumbo_actual));
    st.ultimo_oct = Some(oct);
    // 1) Giro en curso: derecho hasta el punto de giro, arco hacia el carril nuevo, y listo al estar en él.
    if let Some(mut g) = st.compromiso {
        g.resta -= dt;
        // Terminó cuando está EN el carril nuevo y ya apunta hacia donde va (si no, el arco se cortaba a medias).
        let alineado = envolver(rumbo_actual - octante_a_vec(g.oct_t).to_angle()).abs() < 0.45;
        if (oct == g.oct_t && alineado && g.progreso(pos) >= 0.9) || g.resta <= 0.0 {
            st.compromiso = None;
        } else {
            st.compromiso = Some(g);
            let falta = (g.apice - pos).dot(g.d0);
            let rumbo = if falta > g.radio + 0.5 {
                // Aún no: sigue su carril (centrado) hasta el comienzo de la curva.
                let perp = derecha_de(g.d0);
                let desvio = centro_de_carril(body, pos, oct, perp);
                let corrige = if desvio.abs() < 0.25 { 0.0 } else { (desvio * 0.22).clamp(-0.45, 0.45) };
                envolver(g.d0.to_angle() + corrige)
            } else {
                let t = g.progreso(pos);
                if t >= 0.97 {
                    g.c.to_angle()
                } else {
                    (g.punto((t + 0.22).min(1.0)) - pos).to_angle()
                }
            };
            return (rumbo, 0.28);
        }
    }
    let d0 = octante_a_vec(oct);
    let perp = derecha_de(d0);
    let oct_act = oct;
    let adelante = |d: Vec2, m: f32| lane(body, pos + d * m);
    let largo_vista = 3.5 + vel_abs * 0.3;
    // Velocidad: a un fin de calle o a una bocacalle se llega despacio (el radio de giro crece con la velocidad).
    let v_factor = if adelante(d0, 13.0).is_none() || cruce(body, pos + d0 * 10.0, d0) || cruce(body, pos, d0) { 0.4 } else { 1.0 };
    let (tx, tz) = celda(pos);
    let quiere_doblar = st.giro_cd <= 0.0 && cruce(body, pos + d0 * 9.0, d0) && hash(st.semilla ^ (tx as u32).wrapping_mul(31) ^ tz as u32) % 100 < 28;
    if adelante(d0, largo_vista.max(12.0)).is_none() || quiere_doblar {
        let der = derecha_de(d0);
        let candidatos = [der, -der];
        let orden = if azar01(&mut st.semilla) < 0.5 { [0usize, 1] } else { [1, 0] };
        // Para cada lado: ¿hay adelante (6..14 m, y si no más cerca) un carril de esa calle que vaya PARA donde se
        // dobla? Se mira desde cada celda de la calle propia hacia la calle nueva, a 4 m: ahí cae el carril.
        for i in orden {
            let c = candidatos[i];
            let oct_t = octante_de(c);
            let hallado = (6..=14).chain(1..6).map(|m| pos + d0 * m as f32).find_map(|q| {
                let probe = q + c * 4.0;
                (lane(body, probe) == Some(oct_t)).then(|| centro_carril_abs(body, probe, oct_t, derecha_de(c)))
            });
            if let Some(nl) = hallado {
                // Vértice: sobre el eje de MI carril (lateral actual) a la altura del eje del carril nuevo.
                let lat = centro_carril_abs(body, pos, oct, perp).dot(perp);
                let apice = d0 * nl.dot(d0) + perp * lat;
                let radio = if c == der { 5.5 } else { 6.5 };
                st.giro_cd = 4.0;
                st.compromiso = Some(Giro { oct_t, resta: 8.0, d0, c, apice, radio });
                return (d0.to_angle(), 0.4);
            }
        }
        // Nada a los costados: si además la calle se acaba, es un callejón sin salida. Un coche no gira 180° en una
        // calle de 7 m: se detiene y quien maneja la IA lo retira (a la vista, tras unos segundos; si no, ya).
        if adelante(d0, largo_vista).is_none() {
            st.sin_salida = true;
            return (d0.to_angle(), 0.0);
        }
    }
    // Centrado en el carril: corrige según cuánto se desvió del medio de su mitad de calzada.
    let desvio = if en_bocacalle(body, pos, d0) { 0.0 } else { centro_de_carril(body, pos, oct_act, perp) };
    let corrige = if desvio.abs() < 0.25 { 0.0 } else { (desvio * 0.22).clamp(-0.45, 0.45) };
    (envolver(d0.to_angle() + corrige), v_factor)
}

/// ¿Estoy DENTRO de una bocacalle? Hay calzada a los dos costados y de frente muy cerca.
fn en_bocacalle(body: ha_shared::CelestialBody, pos: Vec2, d: Vec2) -> bool {
    let der = derecha_de(d);
    lane(body, pos + der * 3.0).is_some() && lane(body, pos - der * 3.0).is_some() && lane(body, pos + d * 3.0).is_some() && lane(body, pos - d * 3.0).is_some() && {
        // y la calle cruzada llega lejos (no es sólo el ensanche de una avenida)
        lane(body, pos + der * 7.0).is_some() || lane(body, pos - der * 7.0).is_some()
    }
}

/// ¿Hay bocacalle acá? Calzada a los dos costados y de frente.
fn cruce(body: ha_shared::CelestialBody, pos: Vec2, d: Vec2) -> bool {
    let der = derecha_de(d);
    lane(body, pos + der * 5.0).is_some() && lane(body, pos - der * 5.0).is_some() && lane(body, pos + d * 5.0).is_some()
}

// ── Movimiento, choques y daño ───────────────────────────────────────────────────────────────────────────────────

fn bola_choca(obs: &crate::obstaculos::Obstaculos, body: ha_shared::CelestialBody, c: &Cuerpo, f: &Ficha, pos: Vec2) -> bool {
    let d = dir_de(c.rumbo);
    let sep = (f.largo * 0.5 - R_COCHE).max(0.0);
    [-sep, 0.0, sep].into_iter().any(|o| {
        let q = pos + d * o;
        !obs.libre(body, q.x, q.y, R_COCHE)
    })
}

#[allow(clippy::too_many_arguments)]
fn mover_vehiculos(
    mut commands: Commands,
    time: Res<Time>,
    intent: Res<MovementIntent>,
    cond: Res<Conduccion>,
    ctx: Res<WorldContextRuntime>,
    planet: Res<ActivePlanet>,
    obs: crate::obstaculos::Obstaculos,
    mut sfx: EventWriter<PlaySfxAt>,
    mut shake: EventWriter<CameraShakeEvent>,
    mut player: Query<(&mut SimState, &mut PlayerVisual, &mut Visibility), With<Player>>,
    mut coches: Query<(Entity, &mut Vehiculo)>,
) {
    let dt = time.delta_secs().min(0.05);
    let body = active_surface_body(&ctx, &planet);
    for (e, mut v) in &mut coches {
        v.choque_cd = (v.choque_cd - dt).max(0.0);
        v.atropello_cd = (v.atropello_cd - dt).max(0.0);
        if v.destruido {
            v.edad_chasis += dt;
            if v.edad_chasis > VIDA_CHASIS && cond.vehiculo != Some(e) {
                commands.entity(e).despawn_recursive();
            }
            continue;
        }
        let f = v.clase.ficha();
        let mando = match v.conductor {
            Conductor::Jugador => mando_de_stick(v.cuerpo.rumbo, Vec2::new(intent.move_x, intent.move_z), &f),
            Conductor::Npc => Mando { rumbo: v.ia.rumbo_obj, v_obj: v.ia.v_obj },
            Conductor::Nadie => Mando { rumbo: v.cuerpo.rumbo, v_obj: 0.0 },
        };
        let mut c = v.cuerpo;
        let destino = paso(&mut c, mando, &f, dt);
        // Prueba el movimiento; si choca, no avanza y pierde velocidad. Un coche ya encimado (cargaste partida,
        // te empujaron) puede moverse hasta salir: el choque sólo cuenta si el paso EMPEORA la situación.
        let ya_trabado = bola_choca(&obs, body, &c, &f, v.cuerpo.pos);
        if !ya_trabado && bola_choca(&obs, body, &c, &f, destino) {
            let vi = c.vel.abs();
            if vi > V_CHOQUE && v.choque_cd <= 0.0 {
                let dano = (vi - V_CHOQUE + 1.0) * 6.0;
                v.vida -= dano;
                v.choque_cd = 0.4;
                let en = c.pos + dir_de(c.rumbo) * f.largo * 0.5;
                crate::armas::chispa_de_impacto(&mut commands, en, Color::srgb(1.0, 0.8, 0.3));
                sfx.send(PlaySfxAt(Sfx::Crumble, Vec2::new(c.pos.x, -c.pos.y)));
                if v.conductor == Conductor::Jugador {
                    shake.send(CameraShakeEvent { trauma: (vi / 20.0).clamp(0.1, 0.6) });
                }
            }
            c.vel = -c.vel * 0.25;
            // Rebote mínimo hacia atrás para despegar del obstáculo.
            c.pos -= dir_de(c.rumbo) * c.vel.signum() * -0.04;
        } else {
            c.pos = destino;
        }
        v.cuerpo = c;
        if v.vida <= 0.0 {
            destruir(&mut commands, e, &mut v, &mut sfx, &mut shake);
            if cond.vehiculo == Some(e) {
                if let Ok((mut sim, _, mut vis)) = player.get_single_mut() {
                    sim.hp = (sim.hp - 30.0).max(1.0);
                    *vis = Visibility::Inherited;
                }
            }
        }
        if v.conductor == Conductor::Jugador {
            if let Ok((mut sim, mut visual, _)) = player.get_single_mut() {
                sim.body.x = c.pos.x;
                sim.body.z = c.pos.y;
                visual.from_x = c.pos.x;
                visual.to_x = c.pos.x;
                visual.from_z = c.pos.y;
                visual.to_z = c.pos.y;
                visual.to_rot = c.rumbo;
                visual.from_rot = c.rumbo;
            }
        }
    }
}

fn destruir(
    commands: &mut Commands,
    _e: Entity,
    v: &mut Vehiculo,
    sfx: &mut EventWriter<PlaySfxAt>,
    shake: &mut EventWriter<CameraShakeEvent>,
) {
    v.destruido = true;
    v.vida = 0.0;
    v.cuerpo.vel = 0.0;
    v.conductor = Conductor::Nadie;
    v.edad_chasis = 0.0;
    for k in 0..3 {
        let a = k as f32 * 2.1;
        crate::armas::chispa_de_impacto(commands, v.cuerpo.pos + Vec2::new(a.cos(), a.sin()) * 0.8, Color::srgb(1.0, 0.45, 0.1));
    }
    sfx.send(PlaySfxAt(Sfx::Crumble, Vec2::new(v.cuerpo.pos.x, -v.cuerpo.pos.y)));
    shake.send(CameraShakeEvent { trauma: 0.7 });
}

fn choques_entre_coches(
    mut commands: Commands,
    mut sfx: EventWriter<PlaySfxAt>,
    mut shake: EventWriter<CameraShakeEvent>,
    cond: Res<Conduccion>,
    mut q: Query<(Entity, &mut Vehiculo)>,
) {
    let mut v: Vec<(Entity, Vec2, f32, f32, f32, bool)> = q
        .iter()
        .map(|(e, c)| (e, c.cuerpo.pos, c.cuerpo.rumbo, c.cuerpo.vel, c.clase.ficha().largo, c.destruido))
        .collect();
    let mut cambios: Vec<(Entity, Vec2, f32, f32)> = Vec::new(); // (coche, empuje, dvel, dano)
    for i in 0..v.len() {
        for j in (i + 1)..v.len() {
            let (a, b) = (v[i], v[j]);
            let r = (a.4.min(b.4)) * 0.38 + 0.6;
            let d = b.1.distance(a.1);
            if d >= r || d < 1e-3 {
                continue;
            }
            let n = (b.1 - a.1) / d;
            let solape = r - d;
            let rel = (dir_de(a.2) * a.3 - dir_de(b.2) * b.3).dot(n).abs();
            let (ma, mb): (f32, f32) = (if a.5 { 0.0 } else { 1.0 }, if b.5 { 0.0 } else { 1.0 });
            let tot = (ma + mb).max(1e-3);
            cambios.push((a.0, -n * solape * (ma / tot).max(0.15), -a.3 * 0.3, if rel > V_CHOQUE { (rel - V_CHOQUE + 1.0) * 5.0 } else { 0.0 }));
            cambios.push((b.0, n * solape * (mb / tot).max(0.15), -b.3 * 0.3, if rel > V_CHOQUE { (rel - V_CHOQUE + 1.0) * 5.0 } else { 0.0 }));
            if rel > V_CHOQUE {
                let medio = (a.1 + b.1) * 0.5;
                crate::armas::chispa_de_impacto(&mut commands, medio, Color::srgb(1.0, 0.8, 0.3));
                sfx.send(PlaySfxAt(Sfx::Crumble, Vec2::new(medio.x, -medio.y)));
                if cond.vehiculo == Some(a.0) || cond.vehiculo == Some(b.0) {
                    shake.send(CameraShakeEvent { trauma: (rel / 20.0).clamp(0.1, 0.6) });
                }
            }
            v[i].1 -= n * solape * 0.5;
            v[j].1 += n * solape * 0.5;
        }
    }
    for (e, empuje, dvel, dano) in cambios {
        if let Ok((_, mut c)) = q.get_mut(e) {
            if c.destruido {
                continue;
            }
            c.cuerpo.pos += empuje;
            if dano > 0.0 {
                c.cuerpo.vel += dvel;
                c.vida -= dano;
                if c.conductor == Conductor::Npc {
                    c.ia.panico = c.ia.panico.max(3.0);
                }
            }
        }
    }
}

// ── Atropellos y balas ───────────────────────────────────────────────────────────────────────────────────────────

/// El capó como una bala de UN cuadro: la matan `mover_balas` al día siguiente (alcance ≈ 0) y mientras tanto la
/// prueban `balas_a_infectados` / `balas_a_fauna` / `balas_a_aldeanos`, que son quienes saben matar a su gente.
#[allow(clippy::too_many_arguments)]
fn atropellar(mut commands: Commands, mut coches: Query<(Entity, &mut Vehiculo)>) {
    for (e, mut v) in &mut coches {
        if v.destruido || v.cuerpo.vel.abs() < 3.5 || v.atropello_cd > 0.0 {
            continue;
        }
        let f = v.clase.ficha();
        let d = dir_de(v.cuerpo.rumbo) * v.cuerpo.vel.signum();
        let lado = derecha_de(d) * (f.ancho * 0.45);
        let frente = v.cuerpo.pos + d * f.largo * 0.5;
        v.atropello_cd = 0.3;
        commands.spawn(crate::armas::Bala {
            pos: frente + lado,
            prev: frente - lado,
            vel: Vec2::ZERO,
            dano: v.cuerpo.vel.abs() * 5.0,
            origen: frente,
            alcance: 0.01,
            atraviesa: 8,
            golpeados: vec![e],
            viva: true,
            impacto: Sfx::BulletHit,
            rayo: false,
        });
    }
}

#[allow(clippy::too_many_arguments)]
fn balas_a_vehiculos(
    mut commands: Commands,
    mut balas: Query<&mut crate::armas::Bala>,
    mut sfx: EventWriter<PlaySfxAt>,
    mut shake: EventWriter<CameraShakeEvent>,
    mut coches: Query<(Entity, &mut Vehiculo)>,
) {
    for mut b in &mut balas {
        if !b.viva {
            continue;
        }
        for (e, mut v) in &mut coches {
            if b.ya_golpeo(e) || v.conductor == Conductor::Jugador {
                continue;
            }
            let f = v.clase.ficha();
            if !segmento_toca_caja(b.prev, b.pos, v.cuerpo.pos, v.cuerpo.rumbo, f.largo, f.ancho) {
                continue;
            }
            let dano = b.dano_ahora() * 0.7;
            let en = b.pos;
            crate::armas::chispa_de_impacto(&mut commands, en, if b.rayo { Color::srgb(0.4, 1.0, 0.6) } else { Color::srgb(1.0, 0.85, 0.4) });
            sfx.send(PlaySfxAt(Sfx::BulletWall, Vec2::new(en.x, -en.y)));
            b.registrar(e);
            if v.destruido {
                continue;
            }
            v.vida -= dano;
            if v.conductor == Conductor::Npc {
                v.ia.panico = 8.0;
            }
            if v.vida <= 0.0 {
                destruir(&mut commands, e, &mut v, &mut sfx, &mut shake);
            }
            if !b.viva {
                break;
            }
        }
    }
}

// ── Dibujo ───────────────────────────────────────────────────────────────────────────────────────────────────────

fn dibujar_vehiculos(
    art: Res<crate::art2d::ArtLibrary>,
    mut q: Query<(&mut Vehiculo, &mut Sprite, &mut Transform, &mut Visibility)>,
) {
    for (mut v, mut sprite, mut tf, mut vis) in &mut q {
        let previa = if v.dir4 == 255 { None } else { Some(v.dir4) };
        let dir = crate::sim::dir4_de_rumbo(v.cuerpo.rumbo, previa);
        let f = v.clase.ficha();
        let lateral = dir == 0 || dir == 2;
        let chasis = v.destruido;
        if v.dir4 != dir || v.pose.1 != chasis || v.pose.0 == 255 {
            v.dir4 = dir;
            v.pose = (dir, chasis);
            let nuevo = if chasis {
                // El chasis quemado del pack (`auto_chocado`); si no hay, el mismo coche ennegrecido.
                let clave = v.chasis.get_or_insert_with(|| "vehicle/auto_chocado_1".to_string()).clone();
                art.world_sprite(&clave, Vec2::new(4.0, 3.0)).map(|(s, _)| s)
            } else {
                None
            };
            match nuevo {
                Some(s) => *sprite = s,
                None => {
                    let pose = POSES[dir as usize];
                    if let Some((rect, _)) = art.pose(&v.clave, pose) {
                        let tam = Vec2::new(rect[2] as f32, rect[3] as f32) * M_POR_PX;
                        if let Some(mut s) = art.pose_sprite(&v.clave, pose, tam) {
                            if chasis {
                                s.color = Color::srgb(0.18, 0.17, 0.17);
                            }
                            *sprite = s;
                        }
                    }
                }
            }
        }
        // El dibujo se apoya con su base sobre el borde de adelante de la huella (3/4 cenital).
        let prof = if lateral { f.ancho } else { f.largo } * 0.5;
        let alto = sprite.custom_size.map_or(2.0, |s| s.y);
        tf.translation.x = v.cuerpo.pos.x;
        tf.translation.y = -(v.cuerpo.pos.y + prof) + alto * 0.5;
        tf.translation.z = crate::layers::ysort(v.cuerpo.pos.y + prof * 0.5);
        if *vis != Visibility::Inherited {
            *vis = Visibility::Inherited;
        }
    }
}

fn humo_y_fuego(
    mut commands: Commands,
    time: Res<Time>,
    glow: Option<Res<crate::light2d::GlowTexture>>,
    mut coches: Query<&mut Vehiculo>,
    mut humos: Query<(Entity, &mut Humo, &mut Transform, &mut Sprite)>,
) {
    let dt = time.delta_secs();
    for (e, mut h, mut tf, mut s) in &mut humos {
        h.vida -= dt;
        if h.vida <= 0.0 {
            commands.entity(e).despawn();
            continue;
        }
        tf.translation.y += h.sube * dt;
        s.color = s.color.with_alpha((h.vida / 1.2).clamp(0.0, 0.55));
    }
    let Some(glow) = glow else { return };
    for mut v in &mut coches {
        let f = v.clase.ficha();
        let frac = v.vida / f.vida;
        if v.destruido || frac < 0.4 {
            v.humo_t -= dt;
            if v.humo_t <= 0.0 {
                v.humo_t = if v.destruido { 0.12 } else { 0.22 };
                let capo = v.cuerpo.pos + dir_de(v.cuerpo.rumbo) * f.largo * 0.3;
                let (color, tam) = if v.destruido { (Color::srgba(0.12, 0.1, 0.1, 0.5), 1.3) } else { (Color::srgba(0.45, 0.45, 0.45, 0.45), 0.9) };
                commands.spawn((
                    Humo { vida: 1.2, sube: 1.4 },
                    crate::sim::UnlitSprite,
                    Sprite { image: glow.0.clone(), color, custom_size: Some(Vec2::splat(tam)), ..default() },
                    Transform::from_xyz(capo.x, -capo.y, crate::layers::ysort(capo.y) + 0.01),
                ));
                if v.destruido {
                    commands.spawn((
                        Humo { vida: 0.5, sube: 0.6 },
                        crate::sim::UnlitSprite,
                        Sprite { image: glow.0.clone(), color: Color::srgba(1.0, 0.5, 0.12, 0.55), custom_size: Some(Vec2::splat(0.9)), ..default() },
                        Transform::from_xyz(capo.x, -capo.y, crate::layers::ysort(capo.y) + 0.02),
                    ));
                }
            }
        }
    }
}

fn medidor(
    mut commands: Commands,
    mut cond: ResMut<Conduccion>,
    coches: Query<&Vehiculo>,
    layout: Res<crate::input::TouchLayout>,
    mut hud: Query<(Entity, &mut Text), With<MedidorHud>>,
) {
    let Some(e) = cond.vehiculo else {
        for (h, _) in &hud {
            commands.entity(h).despawn_recursive();
        }
        cond.medidor.clear();
        return;
    };
    let Ok(v) = coches.get(e) else { return };
    let f = v.clase.ficha();
    let texto = format!("{} km/h · {:.0} %", (v.cuerpo.vel.abs() * 3.6).round() as i32, (v.vida / f.vida * 100.0).max(0.0));
    cond.medidor = texto.clone();
    if let Some((_, mut t)) = hud.iter_mut().next() {
        if t.0 != texto {
            t.0 = texto;
        }
        return;
    }
    let k = crate::ui_pack::k(&layout);
    commands.spawn((
        Text::new(texto),
        TextFont { font_size: (22.0 * k).clamp(15.0, 26.0), ..default() },
        TextColor(Color::srgb(1.0, 0.92, 0.7)),
        Node {
            position_type: PositionType::Absolute,
            bottom: Val::Px((150.0 * k).max(120.0)),
            left: Val::Percent(50.0),
            margin: UiRect::left(Val::Px(-120.0)),
            width: Val::Px(240.0),
            justify_content: JustifyContent::Center,
            ..default()
        },
        TextLayout::new_with_justify(JustifyText::Center),
        GlobalZIndex(41),
        MedidorHud,
        Name::new("MedidorHud"),
    ));
}

#[allow(dead_code)]
const _TILE: f32 = TILE_M;

#[cfg(test)]
mod tests {
    use super::*;

    const F: Ficha = Ficha { largo: 3.6, ancho: 1.7, v_max: 20.0, v_atras: 5.0, acel: 7.0, freno: 16.0, vida: 120.0 };

    fn correr(c: &mut Cuerpo, m: Mando, seg: f32) {
        let dt = 1.0 / 60.0;
        for _ in 0..(seg / dt) as usize {
            c.pos = paso(c, m, &F, dt);
        }
    }

    #[test]
    fn acelera_hasta_la_velocidad_objetivo_sin_pasarse() {
        let mut c = Cuerpo { pos: Vec2::ZERO, rumbo: 0.0, vel: 0.0 };
        correr(&mut c, Mando { rumbo: 0.0, v_obj: 12.0 }, 6.0);
        assert!((c.vel - 12.0).abs() < 0.6, "vel {}", c.vel);
        assert!(c.pos.x > 30.0 && c.pos.y.abs() < 1e-3, "recto: {:?}", c.pos);
    }

    #[test]
    fn frena_mas_fuerte_de_lo_que_acelera() {
        let mut c = Cuerpo { pos: Vec2::ZERO, rumbo: 0.0, vel: 14.0 };
        let mut t = 0.0;
        while c.vel > 0.5 && t < 5.0 {
            c.pos = paso(&mut c, Mando { rumbo: 0.0, v_obj: 0.0 }, &F, 1.0 / 60.0);
            t += 1.0 / 60.0;
        }
        assert!(t < 1.2, "frenó en {t} s");
    }

    #[test]
    fn dobla_hacia_el_stick_y_el_radio_de_giro_crece_con_la_velocidad() {
        // Giro sostenido (objetivo lejano): el radio = v / velocidad angular.
        let radio = |v: f32| {
            let mut c = Cuerpo { pos: Vec2::ZERO, rumbo: 0.0, vel: v };
            let m = Mando { rumbo: std::f32::consts::PI * 0.99, v_obj: v };
            let dt = 1.0 / 60.0;
            for _ in 0..30 {
                c.pos = paso(&mut c, m, &F, dt);
            }
            let r0 = c.rumbo;
            for _ in 0..30 {
                c.pos = paso(&mut c, m, &F, dt);
            }
            v / ((c.rumbo - r0).abs() / 0.5).max(1e-3)
        };
        let (lento, rapido) = (radio(3.0), radio(18.0));
        assert!(rapido > lento * 1.8, "radio lento {lento} rápido {rapido}");
        assert!(lento < 6.0, "un auto a paso de hombre dobla en una calle: {lento}");
    }

    #[test]
    fn el_stick_hacia_atras_es_marcha_atras() {
        let m = mando_de_stick(0.0, Vec2::new(-1.0, 0.0), &F);
        assert!(m.v_obj < 0.0);
        let mut c = Cuerpo { pos: Vec2::ZERO, rumbo: 0.0, vel: 0.0 };
        correr(&mut c, m, 2.0);
        assert!(c.vel < -2.0 && c.pos.x < -2.0, "retrocede: {c:?}");
        let adelante = mando_de_stick(0.0, Vec2::new(1.0, 0.1), &F);
        assert!(adelante.v_obj > 0.0);
        let quieto = mando_de_stick(0.0, Vec2::new(0.05, 0.0), &F);
        assert_eq!(quieto.v_obj, 0.0);
    }

    #[test]
    fn en_reversa_el_volante_se_invierte_para_acercarse_al_objetivo() {
        // Quiero ir hacia el oeste-sur desde mirando al este: marcha atrás doblando.
        let m = mando_de_stick(0.0, Vec2::new(-1.0, 1.0).normalize(), &F);
        let mut c = Cuerpo { pos: Vec2::ZERO, rumbo: 0.0, vel: 0.0 };
        let antes = envolver(m.rumbo - c.rumbo).abs();
        correr(&mut c, m, 2.5);
        let despues = envolver(m.rumbo - c.rumbo).abs();
        assert!(despues < antes, "antes {antes} después {despues}");
    }

    #[test]
    fn la_caja_orientada_recibe_balas_por_el_costado_y_no_por_fuera() {
        let c = Vec2::new(10.0, 10.0);
        // Coche mirando al este: 3,6 de largo (x) y 1,7 de ancho (z).
        assert!(segmento_toca_caja(Vec2::new(10.0, 5.0), Vec2::new(10.0, 15.0), c, 0.0, 3.6, 1.7));
        assert!(!segmento_toca_caja(Vec2::new(10.0, 5.0), Vec2::new(10.0, 8.0), c, 0.0, 3.6, 1.7));
        assert!(!segmento_toca_caja(Vec2::new(5.0, 11.5), Vec2::new(15.0, 11.5), c, 0.0, 3.6, 1.7), "pasa de largo por el costado");
        // Girado 90°: lo largo queda a lo largo de z.
        assert!(segmento_toca_caja(Vec2::new(10.0, 7.0), Vec2::new(10.0, 8.5), c, std::f32::consts::FRAC_PI_2, 3.6, 1.7));
    }

    #[test]
    fn los_octantes_de_carril_mapean_a_direcciones_del_suelo() {
        assert_eq!(octante_a_vec(0), Vec2::new(1.0, 0.0));
        assert_eq!(octante_a_vec(2), Vec2::new(0.0, -1.0)); // norte = z negativo
        assert_eq!(octante_a_vec(4), Vec2::new(-1.0, 0.0));
        assert_eq!(octante_a_vec(6), Vec2::new(0.0, 1.0));
        // Mano derecha: yendo al este la derecha es el sur (z +).
        assert_eq!(derecha_de(Vec2::new(1.0, 0.0)), Vec2::new(0.0, 1.0));
        assert_eq!(derecha_de(Vec2::new(0.0, -1.0)), Vec2::new(1.0, 0.0));
    }

    #[test]
    fn un_coche_ia_sigue_su_carril_por_una_calle_recta_real() {
        use ha_shared::celestial_bodies::CelestialBody;
        // Una celda de calzada recta del mundo: el carril dice a dónde ir y el coche avanza en esa dirección.
        let body = CelestialBody::Earth;
        let mut probado = 0;
        'b: for sx in -6..6i64 {
            for sz in -6..6i64 {
                let Some(s) = ha_shared::geography::settlement_in(body, sx, sz) else { continue };
                let (cx, cz) = s.center;
                // Barrido corto por la calle central: la primera celda de calzada con carril.
                for d in 0..60i64 {
                    let (tx, tz) = (cx + d, cz);
                    if let Some(o) = ha_shared::roads::lane_at(body, tx, tz) {
                        let v = octante_a_vec(o);
                        assert!(v.length() > 0.99);
                        probado += 1;
                        if probado >= 5 {
                            break 'b;
                        }
                        break;
                    }
                }
            }
        }
        assert!(probado >= 1, "ninguna ciudad con calzada en la muestra");
    }

    #[test]
    fn la_ia_recorre_la_ciudad_real_sin_salirse_de_la_calzada() {
        use ha_shared::celestial_bodies::CelestialBody;
        let body = CelestialBody::Earth;
        let f = Clase::Auto.ficha();
        let mut corridas = 0;
        let mut fuera_total = 0.0f32;
        let mut recorrido_total = 0.0f32;
        'b: for sx in -8..8i64 {
            for sz in -8..8i64 {
                let Some(s) = ha_shared::geography::settlement_in(body, sx, sz) else { continue };
                if s.radius < 100 {
                    continue; // sólo ciudades con calles de verdad
                }
                // Arranque: la primera celda con carril sobre el eje horizontal de la ciudad.
                let Some((tx, tz, oct)) = (0..80i64).find_map(|d| {
                    let (x, z) = (s.center.0 + d, s.center.1);
                    ha_shared::roads::lane_at(body, x, z).map(|o| (x, z, o))
                }) else { continue };
                let pos0 = Vec2::new(tx as f32 + 0.5, tz as f32 + 0.5);
                let mut c = Cuerpo { pos: pos0, rumbo: octante_a_vec(oct).to_angle(), vel: 0.0 };
                let mut mem = Rumbo { semilla: 0x1234_5678u32 ^ (sx as u32) ^ ((sz as u32) << 8), ..default() };
                let (mut fuera, mut recorrido, mut sin_carril) = (0.0f32, 0.0f32, 0.0f32);
                let mut callejones = 0;
                let dt = 1.0 / 30.0;
                let mut rumbo_obj = c.rumbo;
                let mut vfac = 1.0f32;
                let mut piensa = 0.0f32;
                for _ in 0..(120.0 / dt) as usize {
                    piensa -= dt;
                    if piensa <= 0.0 {
                        piensa = 0.125;
                        match lane(body, c.pos) {
                            Some(o) => {
                                let (r, vf) = decidir_rumbo(body, c.pos, c.rumbo, o, c.vel.abs(), 0.125, &mut mem);
                                rumbo_obj = r;
                                vfac = vf;
                                sin_carril = 0.0;
                                if mem.sin_salida {
                                    callejones += 1;
                                    break; // se retira: lo que importa es que no se salió de la calzada hasta acá
                                }
                            }
                            None => sin_carril += 0.125,
                        }
                    }
                    let antes = c.pos;
                    c.pos = paso(&mut c, Mando { rumbo: rumbo_obj, v_obj: f.v_max * 0.45 * vfac }, &f, dt);
                    recorrido += c.pos.distance(antes);
                    if lane(body, c.pos).is_none() {
                        fuera += dt;
                    }
                }
                assert!(sin_carril < 6.0, "la IA se perdió {sin_carril} s en la ciudad de {:?}", s.center);
                println!("ciudad {:?} fuera={fuera:.1}s recorrido={recorrido:.0}m callejon={callejones}", s.center);
                fuera_total += fuera;
                recorrido_total += recorrido;
                corridas += 1;
                if corridas >= 6 {
                    break 'b;
                }
            }
        }
        assert!(corridas >= 3, "pocas ciudades probadas: {corridas}");
        let prom_fuera = fuera_total / corridas as f32;
        assert!(prom_fuera < 3.0, "demasiado tiempo fuera de la calzada: {prom_fuera} s de 120");
        assert!(recorrido_total / corridas as f32 > 500.0, "avanzó poco: {}", recorrido_total / corridas as f32);
    }

    #[test]
    #[ignore]
    fn diag_ia() {
        use ha_shared::celestial_bodies::CelestialBody;
        let body = CelestialBody::Earth;
        let f = Clase::Auto.ficha();
        let (cx, cz): (i64, i64) = (std::env::var("CX").unwrap().parse().unwrap(), std::env::var("CZ").unwrap().parse().unwrap());
        let s = ha_shared::geography::settlement_in(body, cx.div_euclid(2600), cz.div_euclid(2600)).unwrap();
        println!("centro {:?} radio {} tier {:?}", s.center, s.radius, s.tier);
        let (tx, tz, oct) = (0..80i64).find_map(|d| { let (x, z) = (s.center.0 + d, s.center.1); ha_shared::roads::lane_at(body, x, z).map(|o| (x, z, o)) }).unwrap();
        let mut c = Cuerpo { pos: Vec2::new(tx as f32 + 0.5, tz as f32 + 0.5), rumbo: octante_a_vec(oct).to_angle(), vel: 0.0 };
        let mut mem = Rumbo { semilla: 7, ..default() };
        let dt = 1.0 / 30.0;
        let mut rumbo_obj = c.rumbo;
        let mut vfac = 1.0f32;
        let mut piensa = 0.0;
        for k in 0..(120.0 / dt) as usize {
            piensa -= dt;
            if piensa <= 0.0 {
                piensa = 0.125;
                if let Some(o) = lane(body, c.pos) { let (r, vf) = decidir_rumbo(body, c.pos, c.rumbo, o, c.vel.abs(), 0.125, &mut mem); rumbo_obj = r; vfac = vf; }
            }
            c.pos = paso(&mut c, Mando { rumbo: rumbo_obj, v_obj: f.v_max * 0.45 * vfac }, &f, dt);
            if let Some(g) = mem.compromiso { if k % 6 == 0 { println!("  compromiso oct={} resta={:.1} W=({:.1},{:.1}) pos=({:.1},{:.1})", g.oct_t, g.resta, g.apice.x, g.apice.y, c.pos.x, c.pos.y); } }
            if k % 15 == 0 || lane(body, c.pos).is_none() { println!("t={:.1} pos=({:.1},{:.1}) rumbo={:.2} obj={:.2} vel={:.1} lane={:?}", k as f32 * dt, c.pos.x, c.pos.y, c.rumbo, rumbo_obj, c.vel, lane(body, c.pos)); }
        }
    }

    #[test]
    #[ignore]
    fn diag_mapa() {
        use ha_shared::celestial_bodies::CelestialBody;
        let body = CelestialBody::Earth;
        let (cx, cz): (i64, i64) = (std::env::var("MX").unwrap().parse().unwrap(), std::env::var("MZ").unwrap().parse().unwrap());
        for z in -14..14i64 {
            let mut row = String::new();
            for x in -30..30i64 {
                row.push(match ha_shared::roads::lane_at(body, cx + x, cz + z) { Some(0) => '>', Some(2) => '^', Some(4) => '<', Some(6) => 'v', _ => if x == 0 && z == 0 { '+' } else { '.' } });
            }
            println!("{row}");
        }
    }
}
