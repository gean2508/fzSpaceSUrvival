//! Gym de NEUROEVOLUCIÓN para la fauna (B4.1b, `ANALISIS_IA_FRUGAL.md`).
//!
//! "Fase Cámara" de HIVE_OS: evoluciona OFFLINE (a toda velocidad, sin render) un
//! genoma IAFZ-64 que aprende a FORRAJEAR en el mundo REAL del juego
//! (`ha_shared::topdown`), evitando obstáculos y con el **bioma como sensor** (el
//! mismo cerebro se comporta distinto en pasto / arena / agua). Selección
//! darwiniana + mutación/crossover (crate `iafz-sys`). Guarda el campeón como
//! `genomes/FAUNA.adn`, que el juego embebe y carga (fauna con propósito).
//!
//! Uso: `./editor_assets gym` (o `cargo run -p ha-asset-tools -- gym`).

use std::f32::consts::TAU;
use std::process::ExitCode;

use ha_shared::fauna_brain::{sense, solid_at as solid_body};
use ha_shared::CelestialBody;

const BODY: CelestialBody = CelestialBody::Earth;
fn solid_at(x: f32, z: f32) -> bool {
    solid_body(BODY, x, z)
}

const POP: i32 = 48; // tamaño de la colmena
const ELITE: i32 = 6; // sobreviven sin cambios
const GENS: usize = 160; // generaciones
const STEPS: usize = 500; // pasos por episodio
const FOODS: usize = 55; // arena compartida para K agentes
const SEEDS: &[u32] = &[1, 7, 23]; // eval en 3 mundos (anti-overfitting, "espejo")
const TURN: f32 = 2.4;
const SPEED: f32 = 3.2;
const EAT_DIST2: f32 = 0.6;

fn d2(ax: f32, az: f32, bx: f32, bz: f32) -> f32 {
    let (dx, dz) = (ax - bx, az - bz);
    dx * dx + dz * dz
}

/// Reparte comida determinista alrededor del origen para una semilla.
fn make_foods(seed: u32) -> Vec<(f32, f32)> {
    let mut s = seed.wrapping_mul(2654435761).wrapping_add(1);
    let mut rng = || {
        s ^= s << 13;
        s ^= s >> 17;
        s ^= s << 5;
        (s as f32 / u32::MAX as f32) - 0.5
    };
    (0..FOODS)
        .map(|_| (rng() * 40.0, rng() * 40.0))
        .filter(|&(x, z)| !solid_at(x, z))
        .collect()
}

const K: usize = 5; // agentes por episodio (MULTI-AGENTE → evoluciona lo social)
const SCRATCH: i32 = 100; // ids de trabajo, fuera de la población (0..POP)

/// Corre un episodio MULTI-AGENTE: K copias del MISMO genoma (mismos pesos, memoria
/// propia por id) forrajean en una arena compartida **sintiéndose entre sí** (bits
/// sociales del sensor). Fitness = promedio por agente → premia genomas que coordinan
/// (cubrir comida, no amontonarse) → la conducta de grupo puede EVOLUCIONAR offline.
fn run_episode(id: i32, seed: u32) -> f32 {
    let mut foods = make_foods(seed);
    if foods.is_empty() {
        return 0.0;
    }
    let mut x = [0f32; K];
    let mut z = [0f32; K];
    let mut heading = [0f32; K];
    let mut energy = [1f32; K];
    let mut eaten = [0f32; K];
    let mut walls = [0f32; K];
    for k in 0..K {
        iafz_sys::mutate(id, SCRATCH + k as i32, 0.0); // copia el genoma (sin mutar)
        iafz_sys::reset_echo(SCRATCH + k as i32);
        let a = (seed as f32 + k as f32 * 1.7).sin() * TAU;
        x[k] = a.cos() * 3.0;
        z[k] = a.sin() * 3.0;
        heading[k] = a;
    }
    let dt = 0.06f32;
    for _ in 0..STEPS {
        let neigh: Vec<(f32, f32)> = (0..K).filter(|&k| energy[k] > 0.0).map(|k| (x[k], z[k])).collect();
        let mut all_dead = true;
        for k in 0..K {
            if energy[k] <= 0.0 {
                continue;
            }
            all_dead = false;
            let s = sense(BODY, x[k], z[k], heading[k], energy[k], &foods, &neigh);
            let steer = iafz_sys::think(SCRATCH + k as i32, s).tanh();
            heading[k] += steer * TURN * dt;
            let (nx, nz) = (x[k] + heading[k].cos() * SPEED * dt, z[k] + heading[k].sin() * SPEED * dt);
            if solid_at(nx, nz) {
                heading[k] += 1.3;
                walls[k] += 1.0;
            } else {
                x[k] = nx;
                z[k] = nz;
            }
            energy[k] -= dt * 0.03;
            if let Some(i) = foods.iter().position(|&(fx, fz)| d2(x[k], z[k], fx, fz) < EAT_DIST2) {
                foods.swap_remove(i);
                eaten[k] += 1.0;
                energy[k] = (energy[k] + 0.35).min(1.0);
                if foods.is_empty() {
                    break;
                }
            }
        }
        if all_dead || foods.is_empty() {
            break;
        }
    }
    let sum: f32 = (0..K).map(|k| eaten[k] * 100.0 + energy[k] * 20.0 - walls[k] * 0.5).sum();
    sum / K as f32
}

/// Fitness promedio sobre varias semillas (mundos distintos) = generaliza, no memoriza.
fn fitness(id: i32) -> f32 {
    SEEDS.iter().map(|&sd| run_episode(id, sd)).sum::<f32>() / SEEDS.len() as f32
}

pub fn run_gym() -> ExitCode {
    println!("== GYM de fauna (neuroevolución IAFZ-64) ==");
    let genome_out = concat!(env!("CARGO_MANIFEST_DIR"), "/../iafz-sys/genomes/FAUNA.adn");

    // Población inicial aleatoria.
    for id in 0..POP {
        iafz_sys::init(id, 0x1234u32.wrapping_add((id as u32).wrapping_mul(2654435761)));
    }
    let mut order: Vec<i32> = (0..POP).collect();
    let mut best_fit = f32::MIN;

    for gi in 0..GENS {
        // Evaluar y ordenar por fitness (desc).
        let mut scored: Vec<(i32, f32)> = order.iter().map(|&id| (id, fitness(id))).collect();
        scored.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap());
        order = scored.iter().map(|(id, _)| *id).collect();
        let champ = scored[0].1;
        if champ > best_fit {
            best_fit = champ;
        }
        if gi % 20 == 0 || gi == GENS - 1 {
            println!(
                "gen {gi:3} | campeon {:.1} | mediana {:.1}",
                champ,
                scored[scored.len() / 2].1
            );
        }
        if gi == GENS - 1 {
            break;
        }
        // Reproducción: la élite pasa intacta; el resto = crossover de la élite + mutación.
        for rank in ELITE..POP {
            let child = order[rank as usize];
            let p1 = order[(rank % ELITE) as usize];
            let p2 = order[((rank + 1) % ELITE) as usize];
            iafz_sys::crossover(p1, p2, child);
            let rate = if rank < POP * 3 / 4 { 0.08 } else { 0.02 }; // caos vs pulido
            iafz_sys::mutate(child, child, rate);
        }
    }

    // Guardar el campeón.
    let champ_id = order[0];
    if iafz_sys::save(champ_id, genome_out) {
        println!("✓ Campeón guardado en {genome_out} (fitness {best_fit:.1}).");
        println!("  Recompilá el juego para que la fauna lo use.");
        ExitCode::SUCCESS
    } else {
        eprintln!("✗ No se pudo guardar el genoma en {genome_out}");
        ExitCode::FAILURE
    }
}
