//! **Rastro causal y actividad** (biblia §17, reglas 4 y 5).
//!
//! * `registrar(tipo, detalle)` anota un evento del mundo en un anillo global de `CAPACIDAD`
//!   entradas, desde cualquier sistema y sin parámetros nuevos (un `Mutex` estático: se llama
//!   unas pocas veces por minuto, no por cuadro). Responde «¿por qué pasó esto?» sin adivinar:
//!   `/causal` en la consola lista los últimos.
//! * Cada tipo también cuenta cuántas veces ocurrió y cuántos tipos DISTINTOS se vieron: la
//!   versión barata de la *actividad evolutiva* de Bedau — si el contador de novedades se
//!   estanca, el mundo se quedó sin cosas nuevas. `/actividad` lo muestra. Es una medida cruda:
//!   no compara contra un modelo nulo (eso pide muchas semillas, ver checklist de biblia §2).

use bevy::prelude::*;
use std::collections::{BTreeMap, VecDeque};
use std::sync::Mutex;

const CAPACIDAD: usize = 200;

#[derive(Default)]
struct Libro {
    eventos: VecDeque<(u64, String, String)>,
    por_tipo: BTreeMap<String, u64>,
    siguiente: u64,
}

static LIBRO: Mutex<Option<Libro>> = Mutex::new(None);

fn con<R>(f: impl FnOnce(&mut Libro) -> R) -> Option<R> {
    let mut g = LIBRO.lock().ok()?;
    Some(f(g.get_or_insert_with(Libro::default)))
}

/// Anota un evento. `tipo` es corto y estable (`baja`, `alijo`, `meta`…); `detalle`, libre.
pub fn registrar(tipo: &str, detalle: impl Into<String>) {
    let d = detalle.into();
    let _ = con(|l| {
        l.siguiente += 1;
        let n = l.siguiente;
        *l.por_tipo.entry(tipo.to_string()).or_insert(0) += 1;
        l.eventos.push_back((n, tipo.to_string(), d));
        while l.eventos.len() > CAPACIDAD {
            l.eventos.pop_front();
        }
    });
}

/// Los últimos `n` eventos, del más viejo al más nuevo, como «#12 baja: infectado en (3,4)».
pub fn ultimos(n: usize) -> Vec<String> {
    con(|l| {
        let desde = l.eventos.len().saturating_sub(n);
        l.eventos.iter().skip(desde).map(|(i, t, d)| format!("#{i} {t}: {d}")).collect()
    })
    .unwrap_or_default()
}

/// `(eventos totales, tipos distintos)` y el detalle por tipo, en una línea.
pub fn resumen() -> String {
    con(|l| {
        let por: Vec<String> = l.por_tipo.iter().map(|(t, n)| format!("{t} {n}")).collect();
        let (i, h, d) = cerebros();
        format!(
            "{} eventos · {} tipos · {} · cerebros: {i} instinto, {h} hábito, {d} deliberación",
            l.siguiente,
            l.por_tipo.len(),
            por.join(", ")
        )
    })
    .unwrap_or_default()
}

/// **Presupuesto cognitivo** (§17 regla 6): cuántos cerebros hay en cada nivel. Instinto = red
/// IAFZ (fauna), hábito = Sims (aldeanos, compañero), deliberación = los «héroes» (hoy sólo el
/// jugador, y en multijugador los otros jugadores). Lo escribe `contar_cerebros` cada 5 s.
static CEREBROS: Mutex<(u32, u32, u32)> = Mutex::new((0, 0, 0));

pub fn cerebros() -> (u32, u32, u32) {
    CEREBROS.lock().map(|g| *g).unwrap_or_default()
}

pub struct CausalPlugin;

impl Plugin for CausalPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, contar_cerebros.run_if(in_state(crate::game_phase::GamePhase::Playing)))
            .add_systems(OnExit(crate::game_phase::GamePhase::Playing), |mut t: Local<f32>| {
                *t = 0.0;
                vaciar();
            });
    }
}

fn contar_cerebros(
    time: Res<Time>,
    mut acc: Local<f32>,
    animales: Query<(), With<crate::fauna::Animal>>,
    aldeanos: Query<(), With<crate::society::Npc>>,
    compa: Query<(), With<crate::companion::Companion>>,
    remotos: Query<(), With<crate::multijugador::JugadorRemoto>>,
) {
    *acc += time.delta_secs();
    if *acc < 5.0 {
        return;
    }
    *acc = 0.0;
    let v = (animales.iter().count() as u32, (aldeanos.iter().count() + compa.iter().count()) as u32, 1 + remotos.iter().count() as u32);
    if let Ok(mut g) = CEREBROS.lock() {
        *g = v;
    }
}

/// Para las pruebas y para empezar una partida de cero.
pub fn vaciar() {
    let _ = con(|l| *l = Libro::default());
}

#[cfg(test)]
mod tests {
    use super::*;

    // El libro es global: las pruebas que lo usan van en UNA sola para no pisarse.
    #[test]
    fn anota_numera_cuenta_y_tiene_techo() {
        vaciar();
        registrar("baja", "infectado en (3,4)");
        registrar("alijo", "abierto");
        registrar("baja", "otro");
        let u = ultimos(2);
        assert_eq!(u, vec!["#2 alijo: abierto".to_string(), "#3 baja: otro".to_string()]);
        let r = resumen();
        assert!(r.starts_with("3 eventos · 2 tipos"), "{r}");
        assert!(r.contains("cerebros:"), "{r}");
        assert!(r.contains("baja 2") && r.contains("alijo 1"));
        for i in 0..CAPACIDAD * 2 {
            registrar("ruido", format!("{i}"));
        }
        assert_eq!(ultimos(10_000).len(), CAPACIDAD, "el anillo no crece sin freno");
        vaciar();
        assert!(ultimos(5).is_empty());
    }
}
