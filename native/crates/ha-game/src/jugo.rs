//! **Jugo: que el golpe y la carga de Ki se SIENTAN** (biblia §20, ítem 1 del top 10).
//!
//! Hades, Vampire Survivors y Balatro viven del segundo: cada golpe cruje. Hasta el
//! 3 oct 2026 un golpe cuerpo a cuerpo sólo bajaba un número y teñía al blanco de
//! rojo: ni sonido, ni chispa, ni nada que dijera cuánto pegaste. Acá se junta todo
//! en un lugar (verdad única, §2): quien pega manda un [`Golpe`] y este módulo
//! responde con
//! * **parada de golpe** (*hit-stop*): el tiempo del juego casi se congela 45 ms
//!   (75 ms si mata). Se mide en tiempo REAL, así que no se alarga sola;
//! * **chispas** del color del blanco (las mismas de `armas.rs`);
//! * **número de daño** que salta y se apaga (dorado si mata);
//! * **sonido** de impacto en el lugar y **sacudida** de cámara proporcional.
//!
//! Y la **carga de Ki** en tierra (mantener SALTO): piedritas y polvo que suben
//! alrededor de los pies, cada vez más seguido, y un temblor que crece; al despegar,
//! un anillo de polvo y una sacudida. Cupo por cuadro: como mucho [`MAX_MOTAS`]
//! partículas vivas en total (regla 2: el Redmi manda).
//!
//! Las balas NO paran el tiempo (una ráfaga sería un tartamudeo): sólo el golpe a mano.

use bevy::prelude::*;

use crate::audio::{PlaySfxAt, Sfx};
use crate::camera::CameraShakeEvent;

/// Un golpe que acaba de pegar. `en` es (x, z) del mundo (z hacia el sur).
#[derive(Event, Clone, Copy, Debug)]
pub struct Golpe {
    pub en: Vec2,
    pub dano: f32,
    pub letal: bool,
    /// Color de las chispas (sangre, savia, polvo…).
    pub color: Color,
}

/// Parada de golpe pendiente, en segundos REALES.
#[derive(Resource, Default)]
pub struct ParadaDeGolpe {
    restante: f32,
}

/// Velocidad del tiempo durante la parada: casi quieto, no congelado (sigue
/// leyéndose como movimiento, como en Hades).
const VELOCIDAD_PARADA: f32 = 0.06;
const PARADA_GOLPE: f32 = 0.045;
const PARADA_LETAL: f32 = 0.075;
/// Tope duro: aunque lleguen muchos golpes juntos, nunca más que esto seguido.
const PARADA_MAX: f32 = 0.12;

/// Partículas vivas del jugo (números + motas de Ki), en total.
pub const MAX_MOTAS: usize = 48;

#[derive(Component)]
struct NumeroDeDano {
    vida: f32,
    vel: f32,
}

const VIDA_NUMERO: f32 = 0.7;

#[derive(Component)]
struct MotaDeKi {
    vel: Vec2,
    vida: f32,
    vida0: f32,
}

pub struct JugoPlugin;

impl Plugin for JugoPlugin {
    fn build(&self, app: &mut App) {
        app.add_event::<Golpe>()
            .init_resource::<ParadaDeGolpe>()
            .add_systems(Update, (responder_golpes, parada_de_golpe, numeros, carga_de_ki, motas).chain());
    }
}

/// Cuánto dura la parada de un golpe (pura, para el test).
pub fn duracion_parada(letal: bool, ya: f32) -> f32 {
    let p = if letal { PARADA_LETAL } else { PARADA_GOLPE };
    (ya.max(p)).min(PARADA_MAX)
}

/// Cuánta sacudida da un golpe: crece con el daño y satura (un hacha no es un terremoto).
pub fn trauma_de(dano: f32, letal: bool) -> f32 {
    let base = 0.04 + 0.012 * dano.clamp(0.0, 10.0);
    if letal { base + 0.06 } else { base }
}

/// El número que se muestra: entero si es entero, un decimal si no.
pub fn texto_de_dano(dano: f32) -> String {
    let r = (dano * 10.0).round() / 10.0;
    if (r - r.round()).abs() < 1e-3 {
        format!("{}", r.round() as i32)
    } else {
        format!("{r:.1}").replace('.', ",")
    }
}

fn responder_golpes(
    mut commands: Commands,
    mut golpes: EventReader<Golpe>,
    mut parada: ResMut<ParadaDeGolpe>,
    mut sfx: EventWriter<PlaySfxAt>,
    mut shake: EventWriter<CameraShakeEvent>,
    fuente: Option<Res<crate::fuente::FuenteUi>>,
    vivos: Query<(), Or<(With<NumeroDeDano>, With<MotaDeKi>)>>,
) {
    let mut cupo = MAX_MOTAS.saturating_sub(vivos.iter().count());
    for g in golpes.read() {
        parada.restante = duracion_parada(g.letal, parada.restante);
        crate::armas::chispa_de_impacto(&mut commands, g.en, g.color);
        sfx.send(PlaySfxAt(Sfx::BulletHit, Vec2::new(g.en.x, -g.en.y)));
        shake.send(CameraShakeEvent { trauma: trauma_de(g.dano, g.letal) });
        if cupo == 0 {
            continue;
        }
        cupo -= 1;
        let color = if g.letal { Color::srgb(1.0, 0.82, 0.25) } else { Color::srgb(1.0, 0.97, 0.9) };
        let px = if g.letal { 26.0 } else { 20.0 };
        let mut font = TextFont { font_size: 32.0, ..default() };
        if let Some(f) = &fuente {
            font.font = f.0.clone();
        }
        // Un poco corrido al azar (por la posición) para que dos golpes seguidos no se tapen.
        let dx = ((g.en.x * 12.9898 + g.en.y * 78.233).sin() * 43758.545).fract() * 0.5 - 0.25;
        commands.spawn((
            Text2d::new(texto_de_dano(g.dano)),
            font,
            TextColor(color),
            Transform::from_xyz(g.en.x + dx, -g.en.y + 1.1, 19.0),
            crate::textos_mundo::TextoEnMundo { px, font_size: 32.0 },
            NumeroDeDano { vida: VIDA_NUMERO, vel: 1.6 },
            Name::new("NumeroDeDano"),
        ));
    }
}

/// Mientras quede parada, el tiempo del juego va casi quieto; la cuenta se lleva en
/// tiempo REAL (si no, la parada se estiraría a sí misma).
fn parada_de_golpe(real: Res<Time<Real>>, mut virt: ResMut<Time<Virtual>>, mut parada: ResMut<ParadaDeGolpe>) {
    if parada.restante > 0.0 {
        parada.restante = (parada.restante - real.delta_secs()).max(0.0);
        let v = if parada.restante > 0.0 { VELOCIDAD_PARADA } else { 1.0 };
        if (virt.relative_speed() - v).abs() > 1e-4 {
            virt.set_relative_speed(v);
        }
    } else if (virt.relative_speed() - 1.0).abs() > 1e-4 {
        virt.set_relative_speed(1.0);
    }
}

fn numeros(
    mut commands: Commands,
    real: Res<Time<Real>>,
    mut q: Query<(Entity, &mut NumeroDeDano, &mut Transform, &mut TextColor)>,
) {
    // Tiempo real: el número tiene que seguir saltando durante la parada.
    let dt = real.delta_secs().min(0.1);
    for (e, mut n, mut tf, mut c) in &mut q {
        n.vida -= dt;
        if n.vida <= 0.0 {
            commands.entity(e).despawn();
            continue;
        }
        tf.translation.y += n.vel * dt;
        n.vel = (n.vel - 3.0 * dt).max(0.2);
        let a = (n.vida / (VIDA_NUMERO * 0.5)).clamp(0.0, 1.0);
        c.0 = c.0.with_alpha(a);
    }
}

/// La carga del aura en tierra: motas que suben de los pies, temblor creciente y,
/// al despegar, un anillo de polvo y una sacudida.
#[allow(clippy::too_many_arguments)]
fn carga_de_ki(
    mut commands: Commands,
    time: Res<Time>,
    fly: Res<crate::creative_fly::CreativeFlight>,
    player: Query<&Transform, With<crate::sim::Player>>,
    vivos: Query<(), Or<(With<NumeroDeDano>, With<MotaDeKi>)>>,
    mut shake: EventWriter<CameraShakeEvent>,
    mut acumulado: Local<f32>,
    mut semilla: Local<u32>,
    mut volaba: Local<bool>,
) {
    let Ok(p) = player.get_single() else { return };
    let pie = Vec2::new(p.translation.x, p.translation.y - 0.35);
    let mut cupo = MAX_MOTAS.saturating_sub(vivos.iter().count());
    let mut azar = || {
        *semilla = semilla.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
        (*semilla >> 8) as f32 / (1u32 << 24) as f32
    };
    let vuela = fly.flying();
    // El despegue: anillo de polvo + sacudida, una sola vez.
    if vuela && !*volaba {
        shake.send(CameraShakeEvent { trauma: 0.22 });
        for k in 0..12usize.min(cupo) {
            let a = k as f32 / 12.0 * std::f32::consts::TAU + azar() * 0.3;
            commands.spawn(mota(pie, Vec2::new(a.cos() * 3.2, a.sin() * 1.4), 0.45, Color::srgba(0.82, 0.76, 0.64, 0.8)));
        }
        cupo = cupo.saturating_sub(12);
    }
    *volaba = vuela;
    if vuela || !fly.charging {
        *acumulado = 0.0;
        return;
    }
    // Fracción de la carga (0 → 1 hasta despegar).
    let f = (fly.charge_t / crate::creative_fly::TAKEOFF_HOLD).clamp(0.0, 1.0);
    shake.send(CameraShakeEvent { trauma: 0.006 + 0.02 * f });
    // De 6 a 30 motas por segundo según la carga.
    *acumulado += time.delta_secs() * (6.0 + 24.0 * f);
    while *acumulado >= 1.0 && cupo > 0 {
        *acumulado -= 1.0;
        cupo -= 1;
        let x = (azar() - 0.5) * 1.4;
        let piedra = azar() < 0.35;
        let color = if piedra { Color::srgb(0.55, 0.52, 0.48) } else { Color::srgba(0.45, 0.8, 1.0, 0.85) };
        let vida = 0.5 + azar() * 0.4;
        commands.spawn(mota(pie + Vec2::new(x, (azar() - 0.5) * 0.3), Vec2::new(-x * 0.3, 1.2 + 1.8 * f + azar()), vida, color));
    }
    *acumulado = acumulado.min(2.0);
}

fn mota(en: Vec2, vel: Vec2, vida: f32, color: Color) -> impl Bundle {
    (
        Sprite { color, custom_size: Some(Vec2::splat(0.12)), ..default() },
        Transform::from_xyz(en.x, en.y, 18.1),
        MotaDeKi { vel, vida, vida0: vida },
        Name::new("MotaDeKi"),
    )
}

fn motas(mut commands: Commands, real: Res<Time<Real>>, mut q: Query<(Entity, &mut MotaDeKi, &mut Transform, &mut Sprite)>) {
    let dt = real.delta_secs().min(0.1);
    for (e, mut m, mut tf, mut s) in &mut q {
        m.vida -= dt;
        if m.vida <= 0.0 {
            commands.entity(e).despawn();
            continue;
        }
        tf.translation.x += m.vel.x * dt;
        tf.translation.y += m.vel.y * dt;
        m.vel *= 1.0 - 2.5 * dt;
        let a = s.color.alpha();
        s.color = s.color.with_alpha(a.min((m.vida / m.vida0).clamp(0.0, 1.0)));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn la_parada_es_corta_y_no_se_acumula() {
        assert_eq!(duracion_parada(false, 0.0), PARADA_GOLPE);
        assert_eq!(duracion_parada(true, 0.0), PARADA_LETAL);
        // Diez golpes seguidos no estiran la parada más allá del tope.
        let mut p = 0.0;
        for _ in 0..10 {
            p = duracion_parada(true, p);
        }
        assert!(p <= PARADA_MAX);
    }

    #[test]
    fn la_sacudida_crece_con_el_dano_y_satura() {
        assert!(trauma_de(6.0, false) > trauma_de(1.0, false));
        assert!(trauma_de(1.0, true) > trauma_de(1.0, false));
        assert_eq!(trauma_de(50.0, false), trauma_de(10.0, false));
        assert!(trauma_de(50.0, true) < 0.25);
    }

    #[test]
    fn el_numero_se_lee_en_castellano() {
        assert_eq!(texto_de_dano(6.0), "6");
        assert_eq!(texto_de_dano(2.3), "2,3");
        assert_eq!(texto_de_dano(0.999), "1");
    }
}
