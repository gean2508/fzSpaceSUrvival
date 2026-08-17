//! **AUDITORÍA DEL MAPA — el mundo ENTERO, medido por PANTALLA.**
//!
//! Dos errores de método que esta auditoría existe para no repetir:
//!
//! 1. **Medir sólo alrededor del spawn.** La Tierra son 60 km de lado
//!    (±30.000 tiles); una ventana de ±1.500 es el **0,25 % del área** y encima
//!    la parte más llana y más aburrida. Medida ahí, la Tierra da 78 % sabana,
//!    0 % bosque, 0 % montaña y 0,03 % océano — y todo "pasa". Acá se barre
//!    hasta el borde caminable, con paso.
//! 2. **Promediar sobre el planeta.** El jugador no ve el planeta, ve **una
//!    pantalla**. Una pantalla de ladera puede ser 90 % roca mientras la media
//!    del mundo dice 6 %. Por eso la medida que manda es cuántas ventanas del
//!    tamaño de la cámara salen ilegibles.
//!
//! ```sh
//! cargo test -p ha-shared --release --test auditoria_del_mapa -- --ignored --nocapture
//! ```
//!
//! Ninguno es guardián: son números para leer y decidir.

use ha_shared::celestial_bodies::body_walk_radius;
use ha_shared::geography;
use ha_shared::relief::{self, ReliefCell};
use ha_shared::CelestialBody;

/// El ancho de la cámara en tiles, al zoom en que se juega. Sale de las capturas
/// de `relieve-*.png`: 1280 px de ancho con el tile a ~16 px.
const PANTALLA_X: i64 = 80;
const PANTALLA_Z: i64 = 40;

/// El radio caminable del cuerpo, en tiles.
fn radio(b: CelestialBody) -> i64 {
    (body_walk_radius(b) / ha_shared::topdown::TILE_M) as i64
}

/// **EL MUNDO ES UN DISCO, NO UN CUADRADO.** El borde se hunde en mar abierto a
/// partir de `d = 0,80` del radio (`world_map::platform`) y la latitud del clima
/// es la distancia al centro (`geography::coldness_at`), así que fuera del
/// círculo no hay mundo: hay mar polar.
///
/// Barrer el cuadrado de ±r mete **el 21,5 % de las muestras fuera del disco**, y
/// todas caen en el mismo sitio —la esquina helada—, así que el censo salía con
/// un 42 % de `Ice` que nadie va a ver nunca. Se descarta acá y no en cada test.
fn dentro_del_mundo(b: CelestialBody, x: i64, z: i64) -> bool {
    let r = radio(b) as f64;
    ((x * x + z * z) as f64).sqrt() <= r
}

// ---------------------------------------------------------------------------
// 1. El censo de la superficie — el mundo entero, con paso
// ---------------------------------------------------------------------------

#[test]
#[ignore]
fn de_que_esta_hecha_la_tierra() {
    let b = CelestialBody::Earth;
    let r = radio(b);
    // Paso 7: primo respecto de los períodos del relieve (34, 110, 1400…), así
    // que la muestra no se sincroniza con ninguna trama.
    let paso = 23i64;
    let (mut flat, mut cliff, mut ramp, mut pared, mut agua) = (0u64, 0u64, 0u64, 0u64, 0u64);
    let mut por_nivel = [(0u64, 0u64); 14];
    let mut z = -r;
    while z < r {
        let mut x = -r;
        while x < r {
            if !dentro_del_mundo(b, x, z) {
                x += paso;
                continue;
            }
            let l = relief::relief_level(b, x, z);
            if l < 0 {
                agua += 1;
                x += paso;
                continue;
            }
            let idx = (l as usize).min(13);
            por_nivel[idx].0 += 1;
            match relief::relief_cell(b, x, z) {
                ReliefCell::Flat => flat += 1,
                ReliefCell::Cliff => cliff += 1,
                ReliefCell::Ramp => ramp += 1,
                ReliefCell::Pared => {
                    pared += 1;
                    por_nivel[idx].1 += 1;
                }
            }
            x += paso;
        }
        z += paso;
    }
    let tierra = flat + cliff + ramp + pared;
    let todo = tierra + agua;
    println!("--- CENSO (Tierra ENTERA, ±{r} tiles, 1 de cada {}) ---", paso * paso);
    println!("muestras {todo}   agua {agua} ({:.1}%)   tierra {tierra} ({:.1}%)",
        agua as f64 * 100.0 / todo as f64, tierra as f64 * 100.0 / todo as f64);
    let pc = |n: u64| n as f64 * 100.0 / tierra.max(1) as f64;
    println!("  Flat  {flat:>10} {:.2}%", pc(flat));
    println!("  Cliff {cliff:>10} {:.2}%", pc(cliff));
    println!("  Ramp  {ramp:>10} {:.2}%", pc(ramp));
    println!("  PARED {pared:>10} {:.2}%  <-- roca que no se camina", pc(pared));
    println!("--- por escalón: cuánta tierra hay ahí, y cuánta es PARED ---");
    for (l, (n, p)) in por_nivel.iter().enumerate() {
        if *n == 0 {
            continue;
        }
        println!(
            "  nivel {l:>2}: {:>6.2}% de la tierra, de eso pared {:.1}%",
            *n as f64 * 100.0 / tierra as f64,
            *p as f64 * 100.0 / *n as f64
        );
    }
}

// ---------------------------------------------------------------------------
// 2. La medida que importa: ¿cuántas PANTALLAS son ilegibles?
// ---------------------------------------------------------------------------

/// Una pantalla se lee mal cuando la roca de pared se come el cuadro. Las
/// ventanas se reparten por todo el radio caminable, no alrededor del spawn.
#[test]
#[ignore]
fn cuantas_pantallas_son_una_losa() {
    let b = CelestialBody::Earth;
    let r = radio(b);
    // 90×90 ventanas repartidas por los 60 km: una cada ~660 m.
    let n_lado = 90i64;
    let mut total = 0u32;
    let mut losa = 0u32;
    let mut peor = (0.0f64, 0i64, 0i64);
    let mut hist = [0u32; 11];
    for iz in 0..n_lado {
        for ix in 0..n_lado {
            let x0 = -r + (2 * r) * ix / n_lado;
            let z0 = -r + (2 * r) * iz / n_lado;
            let (mut tierra, mut roca) = (0u32, 0u32);
            for z in z0..z0 + PANTALLA_Z {
                for x in x0..x0 + PANTALLA_X {
                    if relief::relief_level(b, x, z) < 0 {
                        continue;
                    }
                    tierra += 1;
                    if relief::relief_cell(b, x, z) == ReliefCell::Pared {
                        roca += 1;
                    }
                }
            }
            if tierra < (PANTALLA_X * PANTALLA_Z / 2) as u32 {
                continue;
            }
            total += 1;
            let f = roca as f64 / tierra as f64;
            hist[((f * 10.0) as usize).min(10)] += 1;
            if f > 0.50 {
                losa += 1;
            }
            if f > peor.0 {
                peor = (f, x0, z0);
            }
        }
    }
    println!("--- PANTALLAS (80x40 tiles, mitad tierra o más, TODO el mundo) ---");
    println!("pantallas medidas: {total}");
    println!("con más del 50 % de roca de pared: {losa} ({:.1}%)",
        losa as f64 * 100.0 / total.max(1) as f64);
    println!("la peor: {:.1}% de roca en ({}, {})", peor.0 * 100.0, peor.1, peor.2);
    for (d, n) in hist.iter().enumerate() {
        println!("  {:>3}-{:>3}% roca : {n}", d * 10, d * 10 + 10);
    }
}

// ---------------------------------------------------------------------------
// 3. Rachas de roca: cómo se lee una ladera al bajarla
// ---------------------------------------------------------------------------

/// Se cuenta la RACHA de `Pared` en el eje Z, que es la dirección en la que
/// cuelgan las paredes. Se barre en franjas repartidas por todo el mundo, no en
/// un cuadrado del centro.
#[test]
#[ignore]
fn rachas_de_roca_hacia_el_sur() {
    let b = CelestialBody::Earth;
    let r = radio(b);
    let mut hist = [0u32; 26];
    let mut total = 0u64;
    let mut suma = 0u64;
    // 400 columnas repartidas por el ancho del mundo, cada una barrida entera.
    for i in 0..400i64 {
        let x = -r + (2 * r) * i / 400;
        let mut racha = 0usize;
        for z in -r..r {
            if relief::relief_level(b, x, z) >= 0
                && relief::relief_cell(b, x, z) == ReliefCell::Pared
            {
                racha += 1;
            } else if racha > 0 {
                hist[racha.min(25)] += 1;
                total += 1;
                suma += racha as u64;
                racha = 0;
            }
        }
    }
    println!("--- RACHAS de roca de pared, mirando al sur (400 columnas del mundo) ---");
    println!("rachas: {total}, filas por racha: {:.2}", suma as f64 / total.max(1) as f64);
    for (n, c) in hist.iter().enumerate() {
        if *c > 0 {
            println!("  {n:>2} filas: {c}");
        }
    }
}

// ---------------------------------------------------------------------------
// 4. Las mesetas de la MONTAÑA: ¿queda algo que pisar arriba?
// ---------------------------------------------------------------------------

/// Se mide sobre la cordillera y los tres montes sueltos, que es donde el
/// problema existe: en el valle del spawn no hay meseta que medir.
#[test]
#[ignore]
fn mesetas_sin_suelo_propio() {
    for (nombre, cx, cz) in sitios_de_montana() {
        mesetas_en(nombre, cx, cz);
    }
}

/// Los sitios con relieve de verdad, en tiles. Salen de la plantilla
/// (`world_map::EARTH`) pasada a tiles con el radio caminable.
fn sitios_de_montana() -> Vec<(&'static str, i64, i64)> {
    let b = CelestialBody::Earth;
    let r = body_walk_radius(b) as i64;
    let p = |nx: f32, nz: f32| ((nx * r as f32) as i64, (nz * r as f32) as i64);
    let mut v = Vec::new();
    for (n, x, z) in [
        ("Cordillera (oeste)", -0.46f32, -0.10f32),
        ("Cordillera (centro)", -0.14, -0.22),
        ("Cordillera (este)", 0.42, -0.06),
        ("Sierra del Sur", -0.10, 0.42),
        ("Monte Solitario", 0.54, -0.30),
        ("Pico del Cuervo", -0.34, 0.11),
        ("Monte Ceniza", 0.52, 0.30),
        ("Cañón del Viento", 0.72, -0.24),
        ("Valle central (control)", 0.0, 0.0),
    ] {
        let (tx, tz) = p(x, z);
        v.push((n, tx, tz));
    }
    v
}

fn mesetas_en(nombre: &str, cx: i64, cz: i64) {
    let b = CelestialBody::Earth;
    let lado = 500i64;
    let (x0, z0) = (cx - lado / 2, cz - lado / 2);
    let w = lado as usize;
    let h = lado as usize;
    let mut nivel = vec![0i8; w * h];
    let mut celda = vec![ReliefCell::Flat; w * h];
    for z in 0..h {
        for x in 0..w {
            let (tx, tz) = (x0 + x as i64, z0 + z as i64);
            nivel[z * w + x] = relief::relief_level(b, tx, tz);
            celda[z * w + x] = relief::relief_cell(b, tx, tz);
        }
    }
    let mut visto = vec![false; w * h];
    let (mut mesetas, mut sin_suelo, mut celdas_sin_suelo) = (0u32, 0u32, 0u64);
    let mut peor = (0usize, 0i64, 0i64, 0usize);
    for z in 0..h {
        for x in 0..w {
            let i = z * w + x;
            let l = nivel[i];
            if visto[i] || l <= 0 {
                continue;
            }
            let mut pila = vec![(x, z)];
            visto[i] = true;
            let (mut n, mut flats) = (0usize, 0usize);
            while let Some((cx2, cz2)) = pila.pop() {
                n += 1;
                if celda[cz2 * w + cx2] == ReliefCell::Flat {
                    flats += 1;
                }
                for (dx, dz) in [(1i64, 0i64), (-1, 0), (0, 1), (0, -1)] {
                    let nx = cx2 as i64 + dx;
                    let nz = cz2 as i64 + dz;
                    if nx < 0 || nz < 0 || nx >= w as i64 || nz >= h as i64 {
                        continue;
                    }
                    let j = nz as usize * w + nx as usize;
                    if visto[j] || nivel[j] != l {
                        continue;
                    }
                    visto[j] = true;
                    pila.push((nx as usize, nz as usize));
                }
            }
            if n < 40 {
                continue;
            }
            mesetas += 1;
            if flats * 10 < n {
                sin_suelo += 1;
                celdas_sin_suelo += n as u64;
                if n > peor.0 {
                    peor = (n, x0, z0, flats);
                }
            }
        }
    }
    // Reparto crudo de la ventana entera: es lo que se ve al volar por encima.
    let mut cuenta = [0u32; 4];
    for c in &celda {
        cuenta[match c {
            ReliefCell::Flat => 0,
            ReliefCell::Cliff => 1,
            ReliefCell::Ramp => 2,
            ReliefCell::Pared => 3,
        }] += 1;
    }
    let tot = (w * h) as f64;
    println!(
        "{nombre:<24} ({cx:>7},{cz:>7})  mesetas {mesetas:>4}  sin suelo {sin_suelo:>4}  \
         celdas atrapadas {celdas_sin_suelo:>7}  |  ventana: Flat {:.0}% Cliff {:.0}% Ramp {:.0}% PARED {:.0}%",
        cuenta[0] as f64 * 100.0 / tot,
        cuenta[1] as f64 * 100.0 / tot,
        cuenta[2] as f64 * 100.0 / tot,
        cuenta[3] as f64 * 100.0 / tot,
    );
    let _ = peor;
}

// ---------------------------------------------------------------------------
// 5. El agua, en todo el mundo
// ---------------------------------------------------------------------------

#[test]
#[ignore]
fn el_agua_se_puede_dibujar() {
    let b = CelestialBody::Earth;
    let r = radio(b);
    let paso = 17i64;
    let (mut charcos_1, mut islas_1, mut pared_en_agua, mut agua, mut total) =
        (0u64, 0u64, 0u64, 0u64, 0u64);
    let mut z = -r;
    while z < r {
        let mut x = -r;
        while x < r {
            if !dentro_del_mundo(b, x, z) {
                x += paso;
                continue;
            }
            total += 1;
            let l = relief::relief_level(b, x, z);
            let veci: Vec<i8> = [(1i64, 0i64), (-1, 0), (0, 1), (0, -1)]
                .iter()
                .map(|(dx, dz)| relief::relief_level(b, x + dx, z + dz))
                .collect();
            if l < 0 {
                agua += 1;
                if veci.iter().all(|&n| n >= 0) {
                    charcos_1 += 1;
                }
            } else {
                if veci.iter().all(|&n| n < 0) {
                    islas_1 += 1;
                }
                if relief::alto_de_pared(b, x, z) > 1 && relief::relief_level(b, x, z + 1) < 0 {
                    pared_en_agua += 1;
                }
            }
            x += paso;
        }
        z += paso;
    }
    println!("--- AGUA (Tierra entera, 1 de cada {}) ---", paso * paso);
    println!("muestras {total}, de agua {agua} ({:.1}%)", agua as f64 * 100.0 / total as f64);
    println!("charcos de UN tile: {charcos_1}");
    println!("islas de UN tile: {islas_1}");
    println!("paredes con el pie en el agua: {pared_en_agua}");
}

// ---------------------------------------------------------------------------
// 6. El reparto de biomas del mundo entero
// ---------------------------------------------------------------------------

#[test]
#[ignore]
fn reparto_de_biomas() {
    let b = CelestialBody::Earth;
    let r = radio(b);
    let paso = 11i64;
    let mut cuenta: std::collections::BTreeMap<String, u64> = Default::default();
    let mut total = 0u64;
    let mut z = -r;
    while z < r {
        let mut x = -r;
        while x < r {
            if dentro_del_mundo(b, x, z) {
                *cuenta.entry(format!("{:?}", geography::biome_at(b, x, z))).or_default() += 1;
                total += 1;
            }
            x += paso;
        }
        z += paso;
    }
    println!("--- BIOMAS (Tierra entera, 1 de cada {}) ---", paso * paso);
    let mut v: Vec<_> = cuenta.into_iter().collect();
    v.sort_by_key(|(_, n)| std::cmp::Reverse(*n));
    for (k, n) in v {
        println!("  {k:<12} {:>6.2}%", n as f64 * 100.0 / total as f64);
    }
}

/// Lo mismo pero SÓLO sobre tierra firme: lo que se ve caminando. Un 60 % de
/// océano tapa el reparto de lo caminable, que es lo que importa para el paisaje.
#[test]
#[ignore]
fn reparto_de_biomas_en_tierra_firme() {
    let b = CelestialBody::Earth;
    let r = radio(b);
    let paso = 11i64;
    let mut cuenta: std::collections::BTreeMap<String, u64> = Default::default();
    let mut total = 0u64;
    let mut z = -r;
    while z < r {
        let mut x = -r;
        while x < r {
            if dentro_del_mundo(b, x, z) && relief::relief_level(b, x, z) >= 0 {
                *cuenta.entry(format!("{:?}", geography::biome_at(b, x, z))).or_default() += 1;
                total += 1;
            }
            x += paso;
        }
        z += paso;
    }
    println!("--- BIOMAS EN TIERRA FIRME (Tierra entera, 1 de cada {}) ---", paso * paso);
    let mut v: Vec<_> = cuenta.into_iter().collect();
    v.sort_by_key(|(_, n)| std::cmp::Reverse(*n));
    for (k, n) in v {
        println!("  {k:<12} {:>6.2}%", n as f64 * 100.0 / total as f64);
    }
}

/// **La humedad, que es quien reparte los biomas de llano.** Si todo el mundo
/// sale sabana no es culpa del `match`: es que `moisture_at` devuelve siempre
/// lo mismo. Acá se ve su histograma contra los umbrales del `match`
/// (0.28 sabana / 0.44 pradera / 0.64 bosque).
#[test]
#[ignore]
fn histograma_de_humedad() {
    let b = CelestialBody::Earth;
    let r = radio(b);
    let paso = 13i64;
    let mut hist = [0u64; 20];
    let mut total = 0u64;
    let (mut min, mut max) = (1.0f32, 0.0f32);
    let mut z = -r;
    while z < r {
        let mut x = -r;
        while x < r {
            if dentro_del_mundo(b, x, z) && relief::relief_level(b, x, z) >= 0 {
                let m = geography::moisture_at(b, x, z);
                min = min.min(m);
                max = max.max(m);
                hist[((m * 20.0) as usize).min(19)] += 1;
                total += 1;
            }
            x += paso;
        }
        z += paso;
    }
    println!("--- HUMEDAD en tierra firme (min {min:.3}, max {max:.3}) ---");
    for (i, n) in hist.iter().enumerate() {
        let a = i as f32 / 20.0;
        let marca = if a >= 0.64 {
            "bosque"
        } else if a >= 0.44 {
            "pradera"
        } else if a >= 0.28 {
            "sabana"
        } else {
            "desierto"
        };
        println!("  {a:.2}-{:.2} {marca:<9} {:>6.2}%", a + 0.05, *n as f64 * 100.0 / total as f64);
    }
}
