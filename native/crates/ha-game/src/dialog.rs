//! Burbujas de diálogo en el MUNDO (Text2d) para NPCs/compañero — el juego se
//! siente vivo (chatter ambiental) y el jugador puede "hablarles" (respuestas
//! predefinidas por CONTEXTO, ver `society::npc_line`). ASCII-only (la fuente no
//! renderiza acentos — memoria `game-font-ascii-only`).

use bevy::prelude::*;

use crate::world2d::{render_xy, sort_z};

/// Escala del Text2d en el mundo (el font_size está en px; esto lo baja a unidades
/// de mundo legibles cerca de un personaje). Ajustable si sale grande/chico.
const BUBBLE_SCALE: f32 = 0.045;

/// E1: presupuesto GLOBAL de burbujas simultáneas — con ~30 NPCs cerca y 7 s de
/// cooldown individual, sin este tope el centro de la pantalla es una sopa de texto
/// ilegible (verificado en capturas). Prioridad: lo que te dicen A VOS (menú/eventos)
/// gana sobre el chatter ambiental cuando no hay lugar.
const MAX_BUBBLES: usize = 3;
/// Cooldown global además del individual por NPC (E1): ni ráfagas de varios NPCs
/// hablando en el mismo instante.
const GLOBAL_BUBBLE_COOLDOWN: f32 = 0.35;

#[derive(Component)]
pub struct SpeechBubble {
    ttl: f32,
    total_ttl: f32,
    /// E2: si tiene dueño, la burbuja lo sigue en vez de quedar clavada en el aire.
    owner: Option<Entity>,
    /// Offset vertical fijo sobre el dueño (altura del personaje + apilado anti-overlap).
    y_offset: f32,
}

struct SpeechEntry {
    x: f32,
    z: f32,
    text: String,
    color: Color,
    owner: Option<Entity>,
    priority: bool,
}

/// Cola de frases a "decir" en el mundo. Cualquier sistema (sociedad, compañero) empuja
/// acá vía `say`/`say_from`; `spawn_bubbles` las materializa respetando el
/// presupuesto global. Desacopla el spawn (Commands) de los sistemas que ya tienen otras
/// queries.
#[derive(Resource, Default)]
pub struct SpeechQueue(Vec<SpeechEntry>);

pub struct DialogPlugin;

impl Plugin for DialogPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<SpeechQueue>()
            .add_systems(Update, (spawn_bubbles, tick_bubbles, follow_owner).chain());
    }
}

fn ascii_sanitize(text: impl Into<String>) -> String {
    let t: String = text.into();
    // ASCII sanitize defensivo (por si alguna frase trae algo raro).
    t.chars().map(|c| if c.is_ascii() { c } else { '?' }).collect()
}

/// Chatter ambiental sin dueño (no sigue a nadie, prioridad normal).
pub fn say(queue: &mut SpeechQueue, x: f32, z: f32, text: impl Into<String>, color: Color) {
    queue.0.push(SpeechEntry { x, z, text: ascii_sanitize(text), color, owner: None, priority: false });
}

/// E2: variante con dueño (y prioridad opcional, E1) — la burbuja sigue al `owner`
/// mientras exista; `priority` la hace ganarle el presupuesto al chatter ambiental.
pub fn say_from(
    queue: &mut SpeechQueue,
    owner: Entity,
    x: f32,
    z: f32,
    text: impl Into<String>,
    color: Color,
    priority: bool,
) {
    queue.0.push(SpeechEntry { x, z, text: ascii_sanitize(text), color, owner: Some(owner), priority });
}

/// Separación (unidades de mundo) entre burbujas apiladas y radio para considerar
/// que dos burbujas "se pisan".
const STACK_STEP: f32 = 0.75;
const STACK_RADIUS: f32 = 1.1;

fn spawn_bubbles(
    mut commands: Commands,
    mut queue: ResMut<SpeechQueue>,
    existing: Query<&Transform, With<SpeechBubble>>,
    mut cooldown: Local<f32>,
    time: Res<Time>,
) {
    *cooldown = (*cooldown - time.delta_secs()).max(0.0);
    if queue.0.is_empty() {
        return;
    }
    let mut alive = existing.iter().count();
    // E1: prioridad primero — si el presupuesto no alcanza para todas, que ganen las
    // dirigidas a vos, no el orden de llegada del chatter ambiental.
    let mut pending = std::mem::take(&mut queue.0);
    pending.sort_by_key(|e| !e.priority);
    let mut occupied: Vec<Vec2> = existing.iter().map(|t| t.translation.truncate()).collect();
    for entry in pending {
        if alive >= MAX_BUBBLES || *cooldown > 0.0 {
            continue; // presupuesto agotado — se pierde (es chatter, no info crítica)
        }
        let SpeechEntry { x, z, text, color, owner, .. } = entry;
        let r = render_xy(x, z);
        let base = Vec2::new(r.x, r.y + 1.9);
        // Sube la burbuja tantos escalones como burbujas cercanas ya haya (anti-overlap).
        let stack = occupied.iter().filter(|p| p.distance(base) < STACK_RADIUS).count() as f32;
        let y_offset = 1.9 + stack * STACK_STEP;
        let pos = Vec2::new(base.x, base.y + stack * STACK_STEP);
        occupied.push(pos);
        commands.spawn((
            Text2d::new(text),
            TextFont { font_size: 22.0, ..default() },
            TextColor(color),
            // Al frente en el Y-sort; +stack en Z para que la de arriba no quede tapada.
            Transform {
                translation: Vec3::new(pos.x, pos.y, sort_z(z) + 5.0 + stack * 0.01),
                scale: Vec3::splat(BUBBLE_SCALE),
                ..default()
            },
            SpeechBubble { ttl: 2.8, total_ttl: 2.8, owner, y_offset },
        ));
        alive += 1;
        *cooldown = GLOBAL_BUBBLE_COOLDOWN;
    }
}

/// E2: reancla la burbuja a la posición ACTUAL de su dueño (si sigue vivo) — antes quedaba
/// clavada en el aire donde nació y el NPC caminaba y la dejaba atrás.
fn follow_owner(
    mut bubbles: Query<(&SpeechBubble, &mut Transform)>,
    owners: Query<&Transform, Without<SpeechBubble>>,
) {
    for (b, mut tf) in &mut bubbles {
        let Some(owner) = b.owner else { continue };
        let Ok(ot) = owners.get(owner) else { continue };
        let float = (b.total_ttl - b.ttl) * 0.4; // sube con el tiempo, como sin dueño
        tf.translation.x = ot.translation.x;
        tf.translation.y = ot.translation.y + b.y_offset + float;
    }
}

fn tick_bubbles(
    time: Res<Time>,
    mut commands: Commands,
    mut bubbles: Query<(Entity, &mut SpeechBubble, &mut Transform, &mut TextColor)>,
) {
    let dt = time.delta_secs();
    for (e, mut b, mut tf, mut col) in &mut bubbles {
        b.ttl -= dt;
        if b.owner.is_none() {
            tf.translation.y += dt * 0.4; // sin dueño: sigue flotando desde donde nació
        }
        // Fade en el ultimo segundo.
        let a = (b.ttl / 1.0).clamp(0.0, 1.0);
        col.0.set_alpha(a);
        if b.ttl <= 0.0 {
            commands.entity(e).despawn();
        }
    }
}
