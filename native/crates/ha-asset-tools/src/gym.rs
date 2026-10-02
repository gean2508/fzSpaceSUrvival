//! Gym de NEUROEVOLUCIÓN para la fauna (`biblia.md` §6).
//!
//! "Fase Cámara" de HIVE_OS: evoluciona OFFLINE (a toda velocidad, sin render) un
//! genoma IAFZ-64 que aprende a FORRAJEAR en el mundo REAL del juego
//! (`ha_shared::topdown`), evitando obstáculos y con el **bioma como sensor** (el
//! mismo cerebro se comporta distinto en pasto / arena / agua). Selección
//! darwiniana + mutación/crossover (crate `iafz-sys`). Guarda el campeón como
//! `genomes/FAUNA.adn`, que el juego embebe y carga (fauna con propósito).
//!
//! Uso: `./editor_assets gym` (entrena) · `./editor_assets gym-medir` (mide el genoma instalado
//! contra la vara). Método tomado del proyecto dron (`~/Documents/mods/dron/CLAUDE.md`, reglas
//! 3.2, 3.15, 3.16, 3.20 y bitácora 7.54), aplicado a la fauna:
//!
//! 1. **La vara honesta**: `steer = 0` es la persecución determinista pura (`referencia`). Un
//!    genoma que no la supera en mundos que NO vio entrenando no se instala, aunque "saque
//!    puntos" — estorba (en el dron, dos corridas enteras estaban por debajo de su vara).
//! 2. **El residuo nace en cero**: la población arranca con las salidas de acción en 0
//!    (`iafz_sys::poner_salidas_en_cero`), o sea EN la vara, y la evolución sólo puede sumarle.
//!    Antes arrancaba al azar y nunca encontraba el «no corregir» que ya valía más.
//! 3. **Un umbral tiene techo**: se pide `min(vara × 1,02, techo × 0,995)`; pedir más que la
//!    perfección traba el entrenamiento sin avisar.
//! 4. **Nada se decide con una muestra**: la validación usa `VAL_SEEDS` mundos distintos de los de
//!    entrenamiento (12, con `p^k·consultas ≪ 1`), y el fitness de selección mezcla promedio y
//!    PEOR mundo (aleatorización de dominio).
//! 5. **Versión de sensores**: el campeón se guarda con `FAUNA.adn.sensores` (ver
//!    `fauna_brain::SENSE_VERSION`); una prueba de `ha-game` exige que coincida.

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
/// Mundos de ENTRENAMIENTO (selección). El fitness es mitad promedio, mitad peor mundo.
const SEEDS: &[u32] = &[1, 7, 23, 41, 59, 83];
/// Mundos de VALIDACIÓN, que el entrenamiento nunca ve: deciden si el campeón se instala.
const VAL_SEEDS: &[u32] = &[101, 113, 127, 139, 151, 163, 181, 193, 211, 229, 241, 257];
/// Mundos de CONFIRMACIÓN: distintos de los dos grupos anteriores. El campeón se ELIGE entre 6
/// sobre `VAL_SEEDS` (el ganador de 6 sorteos queda inflado); acá se lo vuelve a medir sin esa
/// ventaja, como pide la regla 3.20 del dron.
const CONF_SEEDS: &[u32] = &[307, 311, 313, 317, 331, 337, 347, 349, 353, 359, 367, 373];
/// Cuánto tiene que superar a la vara (×) para instalarse, topado por el techo teórico.
const SUPERAR_VARA: f32 = 1.02;
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
fn run_episode(id: Option<i32>, seed: u32) -> f32 {
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
        if let Some(id) = id {
            iafz_sys::mutate(id, SCRATCH + k as i32, 0.0); // copia el genoma (sin mutar)
            iafz_sys::reset_echo(SCRATCH + k as i32);
        }
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
            // `None` = la vara: sin red, `steer = 0` → persecución pura de la comida.
            let steer = if id.is_some() { iafz_sys::think(SCRATCH + k as i32, s).tanh() } else { 0.0 };
            // CAPA DE LOCOMOCION — tiene que ser IDENTICA a la de fauna.rs, o
            // el gym entrena una accion y el juego ejecuta otra.
            // `heading += steer * TURN * dt` es un integrador: steer constante
            // = circulo perfecto. Con rumbo de referencia + persecucion, steer
            // constante = recta.
            let referencia = {
                let mut mejor = f32::MAX;
                let mut rumbo = heading[k];
                for &(fx, fz) in &foods {
                    let d = d2(fx, fz, x[k], z[k]);
                    if d < mejor {
                        mejor = d;
                        rumbo = (fz - z[k]).atan2(fx - x[k]);
                    }
                }
                rumbo
            };
            let deseado = referencia + steer * 2.1;
            let diff = (deseado - heading[k] + std::f32::consts::PI).rem_euclid(TAU)
                - std::f32::consts::PI;
            heading[k] += diff.clamp(-1.0, 1.0) * TURN * dt;
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

/// Fitness de SELECCIÓN: mitad promedio, mitad el peor de los mundos de entrenamiento.
fn fitness(id: i32) -> f32 {
    let v: Vec<f32> = SEEDS.iter().map(|&sd| run_episode(Some(id), sd)).collect();
    let media = v.iter().sum::<f32>() / v.len() as f32;
    let peor = v.iter().cloned().fold(f32::MAX, f32::min);
    0.5 * media + 0.5 * peor
}

/// Lo máximo que puede sacar un agente en un mundo: comerse todas las comidas repartidas entre K
/// y terminar con la energía llena. Sin descontar caminar ni paredes: es un techo, no una meta.
fn techo(seed: u32) -> f32 {
    let comidas = make_foods(seed).len() as f32;
    (comidas / K as f32) * 100.0 + 20.0
}

struct Medida {
    media: f32,
    peor: f32,
    techo: f32,
}

fn medir(id: Option<i32>, seeds: &[u32]) -> Medida {
    let v: Vec<f32> = seeds.iter().map(|&sd| run_episode(id, sd)).collect();
    Medida {
        media: v.iter().sum::<f32>() / v.len() as f32,
        peor: v.iter().cloned().fold(f32::MAX, f32::min),
        techo: seeds.iter().map(|&sd| techo(sd)).sum::<f32>() / seeds.len() as f32,
    }
}

/// El umbral para instalar un genoma: superar a la vara, pero nunca pedir más que el techo.
fn umbral(vara: &Medida) -> f32 {
    (vara.media * SUPERAR_VARA).min(vara.techo * 0.995)
}

/// `gym-medir`: ¿el `FAUNA.adn` instalado supera a la vara en mundos que no vio entrenando?
pub fn run_medir() -> ExitCode {
    let ruta = concat!(env!("CARGO_MANIFEST_DIR"), "/../iafz-sys/genomes/FAUNA.adn");
    iafz_sys::init(0, 1);
    if !iafz_sys::load(0, ruta) {
        eprintln!("no pude leer {ruta}");
        return ExitCode::FAILURE;
    }
    let vara = medir(None, VAL_SEEDS);
    let red = medir(Some(0), VAL_SEEDS);
    let (vc, rc) = (medir(None, CONF_SEEDS), medir(Some(0), CONF_SEEDS));
    println!("  confirmación (otros 12 mundos): vara {:.1} · genoma {:.1} · {:+.1}", vc.media, rc.media, rc.media - vc.media);
    iafz_sys::poner_salidas_en_cero(0);
    let cero = medir(Some(0), VAL_SEEDS);
    println!("== medir FAUNA.adn en {} mundos que el gym no usa para entrenar ==", VAL_SEEDS.len());
    println!("  vara (steer = 0)          media {:7.1}   peor {:7.1}   techo {:7.1}", vara.media, vara.peor, vara.techo);
    println!("  genoma instalado          media {:7.1}   peor {:7.1}", red.media, red.peor);
    println!("  mismo, salidas en cero    media {:7.1}   (debe igualar a la vara: {:+.1})", cero.media, cero.media - vara.media);
    println!("  umbral para instalar      {:7.1}", umbral(&vara));
    // El veredicto exige las DOS tandas de mundos: una sola puede ser suerte (así el genoma
    // instalado «aportaba +20» en una y daba −108 en la otra).
    let dif = ((red.media - vara.media) + (rc.media - vc.media)) / 2.0;
    if red.media >= umbral(&vara) && rc.media >= umbral(&vc) {
        println!("  VEREDICTO: aporta ({dif:+.1} sobre la vara).");
    } else if red.media >= vara.media && rc.media >= vc.media {
        println!("  VEREDICTO: no estorba pero tampoco aporta lo que se exige ({dif:+.1}).");
    } else {
        println!("  VEREDICTO: ESTORBA ({dif:+.1} debajo de la vara): la fauna se mueve mejor sin la red.");
    }
    ExitCode::SUCCESS
}

pub fn run_gym() -> ExitCode {
    println!("== GYM de fauna (neuroevolución IAFZ-64) ==");
    let genome_out = concat!(env!("CARGO_MANIFEST_DIR"), "/../iafz-sys/genomes/FAUNA.adn");

    // La vara y su umbral, medidos UNA vez en los mundos de validación.
    let vara = medir(None, VAL_SEEDS);
    let meta = umbral(&vara);
    println!("vara (steer = 0): media {:.1} · techo {:.1} · umbral para instalar {:.1}", vara.media, vara.techo, meta);

    // Población inicial: EL RESIDUO NACE EN CERO. Cada individuo es una red al azar con las
    // salidas de acción en 0 (= la vara exacta); el 0 es el genoma instalado, también en cero,
    // para no tirar lo que las capas binarias ya aprendieron.
    for id in 0..POP {
        iafz_sys::init(id, 0x1234u32.wrapping_add((id as u32).wrapping_mul(2654435761)));
    }
    let instalado = concat!(env!("CARGO_MANIFEST_DIR"), "/../iafz-sys/genomes/FAUNA.adn");
    if std::path::Path::new(instalado).exists() {
        iafz_sys::load(0, instalado);
    }
    for id in 0..POP {
        iafz_sys::poner_salidas_en_cero(id);
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

    // VALIDACIÓN: de los mejores de la élite, el que más suma en mundos NUEVOS. Se instala sólo si
    // supera el umbral (vara × 1,02, topado por el techo); si no, NO se toca el genoma instalado.
    let mut mejor: Option<(i32, Medida)> = None;
    for &id in order.iter().take(ELITE as usize) {
        let m = medir(Some(id), VAL_SEEDS);
        println!("  validación id {id}: media {:.1} · peor {:.1}", m.media, m.peor);
        if mejor.as_ref().map(|(_, b)| m.media > b.media).unwrap_or(true) {
            mejor = Some((id, m));
        }
    }
    let (champ_id, m) = mejor.expect("la élite no está vacía");
    println!("campeón: {:.1} contra vara {:.1} (umbral {:.1}; techo {:.1})", m.media, vara.media, meta, vara.techo);
    // Confirmación en mundos que ni el entrenamiento ni la elección vieron.
    let vara_c = medir(None, CONF_SEEDS);
    let meta_c = umbral(&vara_c);
    let conf = medir(Some(champ_id), CONF_SEEDS);
    println!("confirmación: {:.1} contra vara {:.1} (umbral {:.1})", conf.media, vara_c.media, meta_c);
    // Dos niveles, como en el dron: APORTA (supera el umbral en las dos tandas) o, si no llega,
    // NO ESTORBA (no queda debajo de la vara en ninguna) Y le gana al genoma instalado. Lo
    // segundo importa: el instalado ya estorbaba (−108 en la confirmación) y dejarlo «por
    // prudencia» sería peor que reemplazarlo por uno que al menos iguala a la vara.
    let aporta = m.media >= meta && conf.media >= meta_c;
    let no_estorba = m.media >= vara.media && conf.media >= vara_c.media;
    let mut instalado_mejor = false;
    if std::path::Path::new(genome_out).exists() {
        iafz_sys::init(SCRATCH + 50, 1);
        if iafz_sys::load(SCRATCH + 50, genome_out) {
            let (iv, ic) = (medir(Some(SCRATCH + 50), VAL_SEEDS), medir(Some(SCRATCH + 50), CONF_SEEDS));
            println!("genoma instalado hoy: {:.1} / {:.1} (validación / confirmación)", iv.media, ic.media);
            instalado_mejor = iv.media + ic.media >= m.media + conf.media;
        }
    }
    if !(aporta || (no_estorba && !instalado_mejor)) {
        println!("✗ NO se instala: ni aporta ni (no estorba y mejora al instalado). FAUNA.adn queda como estaba.");
        println!("  (best_fit de selección {best_fit:.1}; es en mundos de entrenamiento, no cuenta)");
        return ExitCode::SUCCESS;
    }
    println!("{} — se instala.", if aporta { "APORTA" } else { "NO ESTORBA y mejora al instalado" });
    let _ = meta;
    if iafz_sys::save(champ_id, genome_out) {
        let _ = std::fs::write(format!("{genome_out}.sensores"), format!("{}\n", ha_shared::fauna_brain::SENSE_VERSION));
        println!("✓ Campeón guardado en {genome_out} (+{:.1} / +{:.1} sobre la vara en validación / confirmación).", m.media - vara.media, conf.media - vara_c.media);
        println!("  Recompilá el juego para que la fauna lo use.");
        ExitCode::SUCCESS
    } else {
        eprintln!("✗ No se pudo guardar el genoma en {genome_out}");
        ExitCode::FAILURE
    }
}
