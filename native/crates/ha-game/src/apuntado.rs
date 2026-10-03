//! **APUNTADO ASISTIDO** (3 oct 2026) — que disparar se sienta como en *Black Ops 2 Zombies*: el jugador decide a QUIÉN
//! le tira, el juego se encarga de que la bala llegue. Matemática pura (sin ECS) para que `armas` la enchufe en
//! `apuntar_y_disparar` y para que se pueda probar sin abrir el juego. Receta completa: skill `disparo-y-apuntado`.
//!
//! Por qué hace falta en ESTE juego y no es una muleta:
//! * **El dedo tapa al blanco.** Se dispara apoyando el dedo en el mundo; la yema (7-10 mm ≈ 40-60 px ≈ 1,5 m a pie,
//!   ver `targeting::PICK_FORGIVENESS`) cubre al infectado, y el centroide que reporta Android cae medio metro al lado.
//!   Un tiro «exacto al píxel» castiga al dedo, no a la puntería.
//! * **Un control de consola tiene ayuda y nadie lo nota** (Halo: *bullet magnetism* + fricción; CoD: *aim slowdown*
//!   + rotación en hip-fire). La regla de oro de todos: *sutil*. El jugador tiene que sentir que apuntó él.
//!
//! Las tres piezas, en el orden en que se usan:
//! 1. [`elegir_blanco`] — **a quién**: el blanco más alineado con la intención dentro de un cono angosto, medido en
//!    metros laterales (no en grados crudos: a 2 m un grado no es nada, a 25 m es medio cuerpo). Con **histéresis**:
//!    el blanco que ya tenías gana a uno apenas mejor, así una ráfaga no salta entre dos infectados pegados.
//! 2. [`rumbo_asistido`] — **hacia dónde sale la bala**: se dobla hacia el punto de intercepción (con adelanto si el
//!    blanco corre) sólo hasta `doblez_max`; fuera de eso, la bala va donde apuntaste.
//! 3. [`zona_muerta_radial`] — para un stick (mando o táctil): zona muerta radial *escalada*, sin «escalón» en el borde
//!    ni imán a los ejes.

use bevy::math::Vec2;

/// Algo a lo que se le puede tirar. `id` es estable entre cuadros (los bits de la `Entity`) para la histéresis.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Blanco {
    pub id: u64,
    /// Posición en el suelo `(x, z)`, la misma convención que `Bala::pos`.
    pub pos: Vec2,
    /// m/s; cero si no se sabe (alcanza para infectados, que caminan lento).
    pub vel: Vec2,
    /// Radio de impacto (m): el mismo que usa el módulo dueño del blanco (`infectados::RADIO_BLANCO` = 0,55).
    pub radio: f32,
}

/// Perillas. Los valores por defecto están medidos contra la escala del juego: 36 px por metro a pie en el Redmi.
#[derive(Clone, Copy, Debug)]
pub struct Asistencia {
    /// Medio ángulo del cono de candidatos (grados). ~15° de cada lado = el «cono de 30°» de los twin-stick.
    pub cono_grados: f32,
    /// Error lateral máximo (m) aunque el ángulo entre: lejos, el cono se abre demasiado.
    pub lateral_max: f32,
    /// Si el dedo cayó a menos de `radio * dedo_factor` de un blanco, ESE es el elegido, sin discusión.
    pub dedo_factor: f32,
    /// El blanco anterior se queda mientras su puntaje no sea peor que `histeresis` veces el mejor.
    pub histeresis: f32,
    /// Lo máximo que se dobla una bala hacia el blanco (grados). Más que esto ya se nota: «el juego apunta por mí».
    pub doblez_max: f32,
}

impl Default for Asistencia {
    fn default() -> Self {
        Self { cono_grados: 15.0, lateral_max: 1.6, dedo_factor: 1.8, histeresis: 1.35, doblez_max: 9.0 }
    }
}

/// Puntaje de un blanco (menor es mejor) o `None` si no entra. Proyecta la intención sobre la distancia del blanco y
/// mide cuánto le erra de costado — el consejo de *Everything I learned about dual-stick shooter controls*: proyectar
/// primero, comparar después.
fn puntaje(origen: Vec2, dir: Vec2, alcance: f32, b: &Blanco, cfg: &Asistencia) -> Option<f32> {
    let al_blanco = b.pos - origen;
    let dist = al_blanco.length();
    if dist < 1e-3 || dist > alcance + b.radio {
        return None;
    }
    let adelante = al_blanco.dot(dir);
    if adelante <= 0.0 {
        return None;
    }
    let lateral = dir.perp_dot(al_blanco).abs();
    let angulo = (lateral / dist).asin().to_degrees();
    if angulo > cfg.cono_grados && lateral > b.radio {
        return None;
    }
    // Lo que el cuerpo del blanco ya cubre no cuenta como error.
    let error = (lateral - b.radio).max(0.0);
    if error > cfg.lateral_max {
        return None;
    }
    // Error lateral manda; la distancia desempata (a igual error, el más cercano es la amenaza).
    Some(error + dist * 0.02)
}

/// **A quién le tirás.** `dir` es la intención (normalizada): del jugador al dedo, o el stick. `dedo` es el punto tocado
/// si lo hay. `previo` es el `id` del blanco del cuadro anterior. Devuelve el índice en `blancos`.
///
/// El llamador filtra antes lo que no se ve (paredes) y lo que no es enemigo si no quiere ayuda contra aldeanos: la
/// ayuda nunca tiene que convertir un tiro errado en un crimen.
pub fn elegir_blanco(
    origen: Vec2,
    dir: Vec2,
    dedo: Option<Vec2>,
    alcance: f32,
    blancos: &[Blanco],
    previo: Option<u64>,
    cfg: &Asistencia,
) -> Option<usize> {
    let dir = dir.normalize_or_zero();
    if dir == Vec2::ZERO {
        return None;
    }
    // 1) El dedo encima de alguien: lo eligió el jugador.
    if let Some(p) = dedo {
        let mut mejor: Option<(usize, f32)> = None;
        for (i, b) in blancos.iter().enumerate() {
            let d = b.pos.distance(p);
            if d <= b.radio * cfg.dedo_factor && b.pos.distance(origen) <= alcance + b.radio && mejor.map_or(true, |(_, m)| d < m) {
                mejor = Some((i, d));
            }
        }
        if let Some((i, _)) = mejor {
            return Some(i);
        }
    }
    // 2) El más alineado dentro del cono, con histéresis para el que ya venías siguiendo.
    let mut mejor: Option<(usize, f32)> = None;
    let mut del_previo: Option<(usize, f32)> = None;
    for (i, b) in blancos.iter().enumerate() {
        let Some(s) = puntaje(origen, dir, alcance, b, cfg) else { continue };
        if Some(b.id) == previo {
            del_previo = Some((i, s));
        }
        if mejor.map_or(true, |(_, m)| s < m) {
            mejor = Some((i, s));
        }
    }
    match (mejor, del_previo) {
        (Some((_, m)), Some((ip, sp))) if sp <= m * cfg.histeresis + 0.05 => Some(ip),
        (Some((i, _)), _) => Some(i),
        _ => None,
    }
}

/// Punto de intercepción: dónde va a estar el blanco cuando llegue una bala a `vel_bala` m/s. Si no hay solución (el
/// blanco es más rápido que la bala y se aleja) devuelve la posición actual.
pub fn intercepcion(origen: Vec2, b: &Blanco, vel_bala: f32) -> Vec2 {
    let r = b.pos - origen;
    let v = b.vel;
    // |r + v t| = s t  →  (v·v − s²) t² + 2 (r·v) t + r·r = 0
    let a = v.length_squared() - vel_bala * vel_bala;
    let bq = 2.0 * r.dot(v);
    let c = r.length_squared();
    let t = if a.abs() < 1e-6 {
        if bq.abs() < 1e-6 { 0.0 } else { (-c / bq).max(0.0) }
    } else {
        let disc = bq * bq - 4.0 * a * c;
        if disc < 0.0 {
            0.0
        } else {
            let sq = disc.sqrt();
            let t1 = (-bq - sq) / (2.0 * a);
            let t2 = (-bq + sq) / (2.0 * a);
            let t = match (t1 > 0.0, t2 > 0.0) {
                (true, true) => t1.min(t2),
                (true, false) => t1,
                (false, true) => t2,
                _ => 0.0,
            };
            t.min(1.5)
        }
    };
    b.pos + v * t
}

/// **Hacia dónde sale la bala.** Dobla `dir` hacia la intercepción con `blanco`, pero nunca más de `doblez_max` grados:
/// si el jugador apuntó a 40° del infectado, la bala va a 40°. Sin blanco, `dir` tal cual.
pub fn rumbo_asistido(origen: Vec2, dir: Vec2, blanco: Option<&Blanco>, vel_bala: f32, cfg: &Asistencia) -> Vec2 {
    let dir = dir.normalize_or_zero();
    let Some(b) = blanco else { return dir };
    let ideal = (intercepcion(origen, b, vel_bala) - origen).normalize_or_zero();
    if ideal == Vec2::ZERO || dir == Vec2::ZERO {
        return if dir == Vec2::ZERO { ideal } else { dir };
    }
    let delta = dir.angle_to(ideal);
    let max = cfg.doblez_max.to_radians();
    if delta.abs() <= max {
        ideal
    } else {
        dir
    }
}

/// ¿La mira está «en rojo»? (Halo: *red reticle*). Verdadero cuando `rumbo_asistido` va a pegar de lleno: es lo que la
/// mira tiene que mostrar ENCIMA del blanco, que el dedo no tapa.
pub fn en_rojo(origen: Vec2, dir: Vec2, b: &Blanco, vel_bala: f32, cfg: &Asistencia) -> bool {
    let ideal = (intercepcion(origen, b, vel_bala) - origen).normalize_or_zero();
    let dir = dir.normalize_or_zero();
    ideal != Vec2::ZERO && dir != Vec2::ZERO && dir.angle_to(ideal).abs() <= cfg.doblez_max.to_radians()
}

/// Zona muerta **radial escalada** (Josh Sutphin, *Doing thumbstick dead zones right*): por debajo de `zm` es cero; por
/// encima se reescala a `0..1` para que no haya escalón en el borde, y al ser radial no imanta a los ejes.
pub fn zona_muerta_radial(v: Vec2, zm: f32) -> Vec2 {
    let m = v.length();
    if m <= zm || m < 1e-6 {
        return Vec2::ZERO;
    }
    v / m * ((m - zm) / (1.0 - zm)).min(1.0)
}

/// Dispersión efectiva (grados): la primera bala va derecha (*first-shot accuracy*), la ráfaga abre hasta el
/// `def` del arma en ~0,4 s y correr la abre un 35 % más. `rafaga_s` = segundos de gatillo sostenido.
pub fn dispersion_efectiva(def_grados: f32, rafaga_s: f32, corriendo: bool) -> f32 {
    let abre = (rafaga_s / 0.4).clamp(0.0, 1.0);
    let base = def_grados * (0.25 + 0.75 * abre);
    if corriendo { base * 1.35 } else { base }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn b(id: u64, x: f32, y: f32) -> Blanco {
        Blanco { id, pos: Vec2::new(x, y), vel: Vec2::ZERO, radio: 0.55 }
    }
    const O: Vec2 = Vec2::ZERO;

    #[test]
    fn el_dedo_encima_de_un_infectado_lo_elige_aunque_haya_otro_mas_alineado() {
        let bs = [b(1, 10.0, 0.0), b(2, 6.0, 3.0)];
        let cfg = Asistencia::default();
        let i = elegir_blanco(O, Vec2::X, Some(Vec2::new(6.4, 2.6)), 30.0, &bs, None, &cfg);
        assert_eq!(i, Some(1));
    }

    #[test]
    fn el_cono_toma_al_alineado_y_deja_afuera_al_de_costado() {
        let bs = [b(1, 12.0, 1.0), b(2, 5.0, 5.0)];
        let cfg = Asistencia::default();
        assert_eq!(elegir_blanco(O, Vec2::X, None, 30.0, &bs, None, &cfg), Some(0));
        // Apuntando a 90° no hay nadie.
        assert_eq!(elegir_blanco(O, Vec2::new(0.0, -1.0), None, 30.0, &bs, None, &cfg), None);
    }

    #[test]
    fn lejos_el_cono_no_se_abre_mas_que_el_error_lateral_maximo() {
        // A 28 m, 12° son ~5,8 m de costado: entra en el cono pero no en el lateral.
        let a = 12f32.to_radians();
        let bs = [b(1, 28.0 * a.cos(), 28.0 * a.sin())];
        assert_eq!(elegir_blanco(O, Vec2::X, None, 30.0, &bs, None, &Asistencia::default()), None);
    }

    #[test]
    fn nunca_ayuda_hacia_atras_ni_fuera_de_alcance() {
        let bs = [b(1, -5.0, 0.0), b(2, 40.0, 0.0)];
        assert_eq!(elegir_blanco(O, Vec2::X, None, 30.0, &bs, None, &Asistencia::default()), None);
    }

    #[test]
    fn la_histeresis_no_salta_entre_dos_pegados() {
        let cfg = Asistencia::default();
        // El 2 está apenas mejor alineado que el 1, que es el que veníamos siguiendo.
        let bs = [b(1, 10.0, 1.1), b(2, 10.0, 0.9)];
        assert_eq!(elegir_blanco(O, Vec2::X, None, 30.0, &bs, Some(1), &cfg), Some(0));
        assert_eq!(elegir_blanco(O, Vec2::X, None, 30.0, &bs, None, &cfg), Some(1));
        // Pero si el previo quedó claramente peor, se cambia.
        let bs = [b(1, 10.0, 2.0), b(2, 10.0, 0.0)];
        assert_eq!(elegir_blanco(O, Vec2::X, None, 30.0, &bs, Some(1), &cfg), Some(1));
    }

    #[test]
    fn la_bala_se_dobla_poco_y_sólo_si_ya_casi_apuntabas() {
        let cfg = Asistencia::default();
        let t = b(1, 10.0, 1.0); // ~5,7°
        let r = rumbo_asistido(O, Vec2::X, Some(&t), 70.0, &cfg);
        assert!((r - Vec2::new(10.0, 1.0).normalize()).length() < 1e-4);
        assert!(en_rojo(O, Vec2::X, &t, 70.0, &cfg));
        let lejos = b(2, 10.0, 4.0); // ~21,8°
        assert_eq!(rumbo_asistido(O, Vec2::X, Some(&lejos), 70.0, &cfg), Vec2::X);
        assert!(!en_rojo(O, Vec2::X, &lejos, 70.0, &cfg));
    }

    #[test]
    fn el_adelanto_le_pega_a_un_blanco_que_cruza() {
        // Un perro cruzando a 6 m/s a 15 m; bala a 66 m/s.
        let t = Blanco { id: 1, pos: Vec2::new(15.0, 0.0), vel: Vec2::new(0.0, 6.0), radio: 0.55 };
        let p = intercepcion(O, &t, 66.0);
        let tiempo = p.length() / 66.0;
        let real = t.pos + t.vel * tiempo;
        assert!(p.distance(real) < 0.05, "intercepción {p:?} vs {real:?}");
        assert!(p.y > 1.0);
    }

    #[test]
    fn la_intercepcion_imposible_apunta_a_donde_esta() {
        let t = Blanco { id: 1, pos: Vec2::new(10.0, 0.0), vel: Vec2::new(100.0, 0.0), radio: 0.5 };
        assert_eq!(intercepcion(O, &t, 40.0), t.pos);
    }

    #[test]
    fn la_zona_muerta_radial_no_tiene_escalon_ni_imanta_ejes() {
        assert_eq!(zona_muerta_radial(Vec2::new(0.1, 0.05), 0.15), Vec2::ZERO);
        let justo = zona_muerta_radial(Vec2::new(0.16, 0.0), 0.15);
        assert!(justo.length() < 0.02, "sin escalón: {justo:?}");
        let diag = zona_muerta_radial(Vec2::new(0.5, 0.5), 0.15);
        assert!((diag.x - diag.y).abs() < 1e-6, "la diagonal sigue diagonal");
        assert!((zona_muerta_radial(Vec2::new(1.0, 0.0), 0.15).length() - 1.0).abs() < 1e-6);
    }

    #[test]
    fn la_primera_bala_va_derecha_y_la_rafaga_abre() {
        assert!(dispersion_efectiva(4.0, 0.0, false) <= 1.0);
        assert!((dispersion_efectiva(4.0, 2.0, false) - 4.0).abs() < 1e-6);
        assert!(dispersion_efectiva(4.0, 2.0, true) > 4.0);
    }
}
