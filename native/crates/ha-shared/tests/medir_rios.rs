//! **MEDIR LOS RÍOS ANTES DE TOCARLOS.** Hermano de `medir_cascadas` y de
//! `medir_acantilados`: ninguno de éstos es un guardián, son las preguntas que
//! hay que responder con un número antes de decidir si el agua del mundo baja de
//! la montaña o simplemente está pintada en diagonal.
//!
//! ```sh
//! cargo test -p ha-shared --test medir_rios -- --ignored --nocapture
//! ```
//!
//! Lo que mide cada uno:
//!
//! - `cuanta_agua_corre`: qué porción de la tierra es cauce, repartida por
//!   terraza. Es el tamaño del sistema: sin ríos en la sierra no hay cascadas.
//! - `hasta_donde_llega_el_cauce`: se sigue el río aguas abajo, celda a celda,
//!   y se anota dónde termina — mar, lago, o **en seco**, que es el defecto que
//!   se quiere sacar.
//! - `los_lagos_y_sus_afluentes`: cuántas lagunas tienen un río tocando su
//!   orilla y cuántas están aisladas en mitad del prado.
//! - `el_rio_baja`: cuántos escalones sube y cuántos baja un cauce seguido
//!   aguas abajo. Un río que sube es la firma de que el cauce es ruido y no
//!   pendiente.

use ha_shared::celestial_bodies::CelestialBody;
use ha_shared::geography::hay_rio;
use ha_shared::relief::relief_level;
use ha_shared::world_map::{pond_in, POND_PERIOD};
use std::collections::{BTreeMap, HashSet};

/// Ventanas de muestreo: llano alrededor del origen y un trozo de cordillera.
/// La cordillera central de la Tierra corre al NORTE del spawn
/// (`world_map::EARTH.ranges`), así que un cuadrado centrado en (0,0) mide sólo
/// pradera y diría que en este mundo el agua no baja de ningún lado.
const VENTANAS: &[(&str, i64, i64, i64)] = &[
    ("llano", -600, -600, 1_200),
    ("sierra", 1_400, -5_200, 1_200),
    ("cuenca oeste", -9_000, -600, 1_200),
];

/// El campo continuo de altura, para comparar dos celdas de la misma terraza.
fn campo(b: CelestialBody, tx: i64, tz: i64) -> f32 {
    ha_shared::relief::relief_field(b, tx, tz)
}

/// **La celda de cauce vecina que está más abajo**, o `None` si ninguna baja.
///
/// Es la única forma de "seguir el río" en un mundo sin hidrología: el cauce no
/// tiene dirección declarada, así que se le pregunta al terreno. Se compara el
/// campo continuo, no la terraza, porque dentro de una misma terraza el campo
/// sigue teniendo pendiente y es lo que distingue aguas arriba de aguas abajo.
fn aguas_abajo(
    b: CelestialBody,
    tx: i64,
    tz: i64,
    visto: &HashSet<(i64, i64)>,
) -> Option<(i64, i64)> {
    let h = campo(b, tx, tz);
    let mut mejor: Option<((i64, i64), f32)> = None;
    for (dx, dz) in [
        (1, 0),
        (-1, 0),
        (0, 1),
        (0, -1),
        (1, 1),
        (1, -1),
        (-1, 1),
        (-1, -1),
    ] {
        let (nx, nz) = (tx + dx, tz + dz);
        if visto.contains(&(nx, nz)) {
            continue;
        }
        // El mar y el lago son el final del recorrido, no un paso más: se
        // devuelven aunque no sean "río" (`hay_rio` es falso bajo el mar).
        let liquido = relief_level(b, nx, nz) < 0;
        if !liquido && !hay_rio(b, nx, nz) {
            continue;
        }
        let hn = campo(b, nx, nz);
        if hn >= h {
            continue;
        }
        if mejor.is_none_or(|(_, m)| hn < m) {
            mejor = Some(((nx, nz), hn));
        }
    }
    mejor.map(|(c, _)| c)
}

/// Dónde terminó un recorrido aguas abajo.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum Final {
    /// Llegó a agua libre: mar, cuenca o laguna.
    Liquido,
    /// Se quedó sin vecino más bajo: el cauce se apaga en mitad de la tierra.
    EnSeco,
    /// Se agotó el presupuesto de pasos. No dice nada malo del río.
    SinTope,
}

/// Sigue el cauce desde una celda hasta donde llegue. Devuelve el final, cuántos
/// pasos duró y cuántos escalones bajó (negativo = subió).
fn seguir(b: CelestialBody, tx: i64, tz: i64, tope: usize) -> (Final, usize, i32) {
    let nivel0 = relief_level(b, tx, tz) as i32;
    let mut visto: HashSet<(i64, i64)> = HashSet::new();
    let (mut x, mut z) = (tx, tz);
    visto.insert((x, z));
    for pasos in 0..tope {
        if relief_level(b, x, z) < 0 {
            return (Final::Liquido, pasos, nivel0 - relief_level(b, x, z) as i32);
        }
        let Some((nx, nz)) = aguas_abajo(b, x, z, &visto) else {
            return (Final::EnSeco, pasos, nivel0 - relief_level(b, x, z) as i32);
        };
        x = nx;
        z = nz;
        visto.insert((x, z));
    }
    (Final::SinTope, tope, nivel0 - relief_level(b, x, z) as i32)
}

/// **¿CUÁNTA AGUA CORRE, Y POR QUÉ TERRAZA?**
#[test]
#[ignore = "herramienta de medición, no un guardián"]
fn cuanta_agua_corre() {
    let b = CelestialBody::Earth;
    for &(nombre, x0, z0, lado) in VENTANAS {
        let mut tierra = 0usize;
        let mut rio = 0usize;
        let mut por_nivel: BTreeMap<i8, (usize, usize)> = Default::default();
        for tz in z0..z0 + lado {
            for tx in x0..x0 + lado {
                let l = relief_level(b, tx, tz);
                if l < 0 {
                    continue;
                }
                tierra += 1;
                let e = por_nivel.entry(l).or_default();
                e.0 += 1;
                if hay_rio(b, tx, tz) {
                    rio += 1;
                    e.1 += 1;
                }
            }
        }
        let pct = 100.0 * rio as f32 / tierra.max(1) as f32;
        println!("\n{nombre}: {rio} celdas de río en {tierra} de tierra ({pct:.2} %)");
        for (l, (t, r)) in por_nivel {
            let p = 100.0 * r as f32 / t.max(1) as f32;
            println!("  terraza {l:2}  {t:8} tierra  {r:7} río  {p:5.2} %");
        }
    }
}

/// **¿HASTA DÓNDE LLEGA EL CAUCE?**
///
/// Se arranca de las celdas de río más altas de la ventana —las cabeceras— y se
/// baja. Un río que se apaga en seco es el que hoy se corta a media ladera
/// porque el entramado de ruido no sabe adónde va.
#[test]
#[ignore = "herramienta de medición, no un guardián"]
fn hasta_donde_llega_el_cauce() {
    let b = CelestialBody::Earth;
    for &(nombre, x0, z0, lado) in VENTANAS {
        // Cabeceras: celda de río cuyos ocho vecinos de cauce están más abajo.
        let mut cabeceras = vec![];
        for tz in (z0..z0 + lado).step_by(7) {
            for tx in (x0..x0 + lado).step_by(7) {
                if !hay_rio(b, tx, tz) {
                    continue;
                }
                let h = campo(b, tx, tz);
                let arriba = [(1, 0), (-1, 0), (0, 1), (0, -1)]
                    .iter()
                    .any(|(dx, dz)| hay_rio(b, tx + dx, tz + dz) && campo(b, tx + dx, tz + dz) > h);
                if !arriba {
                    cabeceras.push((tx, tz));
                }
            }
        }
        let mut finales: BTreeMap<Final, usize> = Default::default();
        let mut pasos_total = 0usize;
        for &(tx, tz) in cabeceras.iter().take(400) {
            let (f, pasos, _) = seguir(b, tx, tz, 3_000);
            *finales.entry(f).or_default() += 1;
            pasos_total += pasos;
        }
        let n = cabeceras.len().min(400).max(1);
        println!(
            "\n{nombre}: {} cabeceras, se siguen {n}, {} pasos de media",
            cabeceras.len(),
            pasos_total / n
        );
        for (f, c) in finales {
            println!("  {f:?}: {c}");
        }
    }
}

/// **¿TIENEN AFLUENTE LOS LAGOS?**
///
/// Las lagunas repartidas (`world_map::pond_in`) son agua porque hunden el
/// terreno, no porque un río las llene. Esto cuenta cuántas tienen de verdad un
/// cauce tocando su orilla.
#[test]
#[ignore = "herramienta de medición, no un guardián"]
fn los_lagos_y_sus_afluentes() {
    let b = CelestialBody::Earth;
    let mut lagunas = 0usize;
    let mut mojadas = 0usize;
    let mut con_afluente = 0usize;
    let mut afluentes: BTreeMap<usize, usize> = Default::default();
    for rz in -6..6 {
        for rx in -6..6 {
            let Some(((cx, cz), r)) = pond_in(b, rx, rz) else {
                continue;
            };
            lagunas += 1;
            // ¿La laguna se llenó? Una en la cumbre queda por encima del mar.
            if relief_level(b, cx, cz) >= 0 {
                continue;
            }
            mojadas += 1;
            // La orilla: el anillo de celdas secas pegadas al agua. Se cuenta un
            // afluente por cada celda de orilla que sea cauce.
            let mut n = 0usize;
            let paso = (r / 60.0).ceil() as i64;
            for a in 0..360 {
                let ang = a as f32 * std::f32::consts::PI / 180.0;
                for k in 0..paso.max(1) * 6 {
                    let d = r + k as f32;
                    let tx = cx + (d * ang.cos()) as i64;
                    let tz = cz + (d * ang.sin()) as i64;
                    if relief_level(b, tx, tz) < 0 {
                        continue;
                    }
                    if hay_rio(b, tx, tz) {
                        n += 1;
                    }
                    break;
                }
            }
            if n > 0 {
                con_afluente += 1;
            }
            *afluentes.entry(n.min(20)).or_default() += 1;
        }
    }
    println!(
        "\nlagunas declaradas: {lagunas} · con agua: {mojadas} · con al menos un afluente: {con_afluente}"
    );
    println!("  (celda de laguna = {POND_PERIOD} tiles de lado)");
    for (n, c) in afluentes {
        println!("  {n:2} celdas de orilla con cauce: {c} lagunas");
    }
}

/// **¿EL RÍO BAJA?**
///
/// Se sigue el cauce en las dos direcciones posibles desde una celda cualquiera
/// y se mira cuántas de sus vecinas de cauce están por encima. Un cauce sano
/// tiene una vecina arriba y una abajo; uno que es puro ruido tiene tantas
/// arriba como abajo y no se puede recorrer.
#[test]
#[ignore = "herramienta de medición, no un guardián"]
fn el_rio_baja() {
    let b = CelestialBody::Earth;
    for &(nombre, x0, z0, lado) in VENTANAS {
        let mut celdas = 0usize;
        let mut sin_salida = 0usize;
        let mut bajada_total = 0i64;
        let mut recorridos = 0usize;
        for tz in (z0..z0 + lado).step_by(11) {
            for tx in (x0..x0 + lado).step_by(11) {
                if !hay_rio(b, tx, tz) {
                    continue;
                }
                celdas += 1;
                let visto = HashSet::new();
                if aguas_abajo(b, tx, tz, &visto).is_none() {
                    sin_salida += 1;
                    continue;
                }
                let (_, pasos, baja) = seguir(b, tx, tz, 400);
                if pasos > 0 {
                    recorridos += 1;
                    bajada_total += baja as i64;
                }
            }
        }
        println!(
            "\n{nombre}: {celdas} celdas de cauce muestreadas · {sin_salida} sin vecina más baja \
             ({:.1} %) · bajada media de {recorridos} recorridos: {:.2} escalones",
            100.0 * sin_salida as f32 / celdas.max(1) as f32,
            bajada_total as f32 / recorridos.max(1) as f32
        );
    }
}

/// **DÓNDE TERMINA CADA RÍO DEL PLANETA.**
///
/// Es la medición que decide si hay hidrografía. Los otros tests de este archivo
/// siguen el cauce CELDA A CELDA sobre el campo de altura, y eso deja de servir
/// desde que el río impone su propio perfil (`rios::perfil_del_cauce`): dentro de
/// una terraza el fondo es casi horizontal, así que el seguidor se atasca en el
/// ruido de coma flotante y reporta "EnSeco" aunque el cauce siga. Acá se le
/// pregunta al TRAZADOR, que es quien sabe por qué paró.
///
/// ```sh
/// cargo test -p ha-shared --test medir_rios donde_termina -- --ignored --nocapture
/// ```
#[test]
#[ignore]
fn donde_termina_cada_rio() {
    use ha_shared::rios::{destinos, Destino, CUENCA};
    let b = CelestialBody::Earth;
    let mut cuenta: BTreeMap<String, usize> = BTreeMap::new();
    let mut cuencas_con_rio = 0;
    let mut cuencas = 0;
    // Una franja del planeta ancha de verdad: 21×21 cuencas son 18.900 tiles de
    // lado, o sea continente entero más océano.
    for cz in -10..=10 {
        for cx in -10..=10 {
            cuencas += 1;
            let d = destinos(b, cx, cz);
            if !d.is_empty() {
                cuencas_con_rio += 1;
            }
            for x in d {
                let k = match x {
                    Destino::Mar => "mar",
                    Destino::Lago => "lago",
                    Destino::SinLlegar => "sin llegar (agotó zancadas)",
                    Destino::Estancado => "estancado (dejó de bajar)",
                };
                *cuenta.entry(k.to_string()).or_default() += 1;
            }
        }
    }
    let total: usize = cuenta.values().sum();
    println!("cuenca = {CUENCA} tiles · {cuencas_con_rio} de {cuencas} cuencas con naciente");
    println!("ríos trazados: {total}");
    for (k, n) in &cuenta {
        println!("  {k:32} {n:5}  ({:.1} %)", 100.0 * *n as f32 / total.max(1) as f32);
    }
    let llegan = cuenta.get("mar").copied().unwrap_or(0) + cuenta.get("lago").copied().unwrap_or(0);
    println!("LLEGAN AL AGUA: {llegan} de {total} ({:.1} %)", 100.0 * llegan as f32 / total.max(1) as f32);
}

// ---------------------------------------------------------------------------
// LA TALLA DEL VALLE: las tres preguntas que hay que responder con un número
// antes de tocar `HONDURA_EN_ESCALONES`, `VALLE_MINIMO` o `RUIDO_EN_EL_CAUCE`
// ---------------------------------------------------------------------------

/// **¿SE PUEDE CAMINAR EL VALLE, O EL PROPIO RÍO FABRICA EL FARALLÓN?**
///
/// `relief::MAX_FIELD_STEP_PER_TILE` (0,020) es la cota de cuánto puede cambiar
/// el campo entre dos celdas vecinas, y no es decorativa: el descarte barato de
/// `near_step_boundary` se apoya en ella. La talla del río la violaba —el empalme
/// pasaba de la cota lisa del cauce al ruido entero en 3,6 tiles, hasta 0,06 por
/// tile— y de ahí salían los acantilados en zig-zag cruzando el agua.
///
/// ```sh
/// cargo test -p ha-shared --release --test medir_rios el_campo_no_salta -- --ignored --nocapture
/// ```
#[test]
#[ignore]
fn el_campo_no_salta_en_el_valle() {
    let b = CelestialBody::Earth;
    const COTA: f32 = 0.020;
    for (nombre, x0, z0, lado) in VENTANAS {
        let mut saltos: Vec<f32> = Vec::new();
        for tz in *z0..z0 + lado {
            for tx in *x0..x0 + lado {
                // Sólo dentro del valle: fuera, el campo es el de siempre.
                if ha_shared::rios::perfil_del_cauce(b, tx, tz).is_none() {
                    continue;
                }
                let h = campo(b, tx, tz);
                let mut peor = 0.0f32;
                for (dx, dz) in [(1i64, 0i64), (0, 1), (1, 1), (1, -1)] {
                    peor = peor.max((campo(b, tx + dx, tz + dz) - h).abs());
                }
                saltos.push(peor);
            }
        }
        if saltos.is_empty() {
            println!("{nombre:<14} sin valle en la ventana");
            continue;
        }
        saltos.sort_by(|a, c| a.partial_cmp(c).unwrap());
        let p = |q: f32| saltos[((saltos.len() as f32 - 1.0) * q) as usize];
        let fuera = saltos.iter().filter(|s| **s > COTA).count();
        println!(
            "{nombre:<14} celdas de valle {:6} | mediana {:.4} p99 {:.4} max {:.4} | \
             por encima de {COTA}: {fuera} ({:.2} %)",
            saltos.len(),
            p(0.5),
            p(0.99),
            saltos[saltos.len() - 1],
            100.0 * fuera as f32 / saltos.len() as f32,
        );
    }
}

/// **¿EL CAUCE ESTÁ MÁS ABAJO QUE SU ORILLA?**
///
/// Es la pregunta que `HONDURA` no contestaba: valía 0,030 de campo y un escalón
/// mide 0,0467, así que la talla no bajaba el cauce ni un nivel — sólo corría la
/// frontera de cuantización. Un río que va al mismo nivel que su orilla no tiene
/// orilla: el relieve le planta el canto de la terraza encima.
#[test]
#[ignore]
fn el_cauce_baja_de_terraza() {
    let b = CelestialBody::Earth;
    for (nombre, x0, z0, lado) in VENTANAS {
        let (mut cauce, mut baja) = (0u32, 0u32);
        for tz in *z0..z0 + lado {
            for tx in *x0..x0 + lado {
                if !hay_rio(b, tx, tz) {
                    continue;
                }
                cauce += 1;
                let mio = relief_level(b, tx, tz);
                // La orilla: la celda seca más cercana a lo ancho, en las cuatro
                // direcciones, hasta 6 tiles.
                // **La orilla se busca FUERA del valle**, no a la primera celda
                // seca: el valle está aplanado a propósito (`rios::VALLE_MINIMO`)
                // y medir contra él era medir el ancho del valle, no la talla.
                let mut orilla = mio;
                for (dx, dz) in [(1i64, 0i64), (-1, 0), (0, 1), (0, -1)] {
                    for k in 1..=14 {
                        let (x, z) = (tx + dx * k, tz + dz * k);
                        if ha_shared::rios::perfil_del_cauce(b, x, z).is_none() {
                            orilla = orilla.max(relief_level(b, x, z));
                            break;
                        }
                    }
                }
                if orilla > mio {
                    baja += 1;
                }
            }
        }
        println!(
            "{nombre:<14} cauce {cauce:6} celdas | con la orilla más alta: {baja} ({:.1} %)",
            100.0 * baja as f32 / cauce.max(1) as f32
        );
    }
}

/// **¿CUÁNTAS VECES CORTA UN ACANTILADO EL CAUCE?**
///
/// Ninguno de los guardianes viejos cubre esto: `agua::ningun_acantilado_toca_el_agua`
/// y `relief::the_shore_is_never_a_cliff` preguntan por `relief_level < 0`, o sea
/// mar y lagos, y un río vive en nivel ≥ 0. Un corte es una celda de cauce que es
/// roca **y no es la garganta de una cascada**: ahí el agua tiene que pasar y el
/// recorte del pack ya trae su roca a los dos lados.
#[test]
#[ignore]
fn el_cauce_no_lo_corta_un_acantilado() {
    use ha_shared::relief::{relief_cell, ReliefCell};
    let b = CelestialBody::Earth;
    for (nombre, x0, z0, lado) in VENTANAS {
        let (mut cauce, mut roca, mut garganta) = (0u32, 0u32, 0u32);
        for tz in *z0..z0 + lado {
            for tx in *x0..x0 + lado {
                if !hay_rio(b, tx, tz) {
                    continue;
                }
                cauce += 1;
                if !matches!(relief_cell(b, tx, tz), ReliefCell::Cliff | ReliefCell::Pared) {
                    continue;
                }
                if ha_shared::agua::en_la_garganta(b, tx, tz) {
                    garganta += 1;
                } else {
                    roca += 1;
                }
            }
        }
        println!(
            "{nombre:<14} cauce {cauce:6} | CORTADO por roca {roca} ({:.2} %) | \
             en garganta (correcto) {garganta}",
            100.0 * roca as f32 / cauce.max(1) as f32
        );
    }
}

/// **DÓNDE ESTÁN LAS CASCADAS**, para apuntar el guardián a una ventana que las
/// tenga. Temporal.
#[test]
#[ignore]
fn donde_estan_las_cascadas() {
    use ha_shared::relief::relief_level;
    let b = CelestialBody::Earth;
    let mut sitios = Vec::new();
    'busca: for cz in (-16_000..16_000).step_by(900) {
        for cx in (-16_000..16_000).step_by(900) {
            if relief_level(b, cx, cz) < 5 {
                continue;
            }
            let mut n = 0;
            for tz in cz - 250..cz + 250 {
                for tx in cx - 250..cx + 250 {
                    if ha_shared::agua::cascada_at(b, tx, tz).is_some() {
                        n += 1;
                    }
                }
            }
            if n > 0 {
                sitios.push((cx, cz, n));
                if sitios.len() >= 8 {
                    break 'busca;
                }
            }
        }
    }
    for (x, z, n) in &sitios {
        println!("cascadas en ({x},{z}): {n}");
    }
    println!("total de ventanas con cascada: {}", sitios.len());
}

/// **¿HAY PASTO SECO EN LA NIEVE?** — el parche khaki de las capturas.
///
/// El material de una pared sale de `superficie_de` de SU celda. Si el río talla
/// el cauce por debajo de la grada de sierra (5), la celda de al lado deja de ser
/// montaña y vuelve a ser sabana: un acantilado teñido de pasto seco plantado en
/// mitad de un nevado. Esto lo cuenta.
#[test]
#[ignore]
fn pasto_seco_dentro_de_la_nieve() {
    use ha_shared::geography::{superficie_de, Pasto};
    use ha_shared::relief::relief_level;
    use ha_shared::topdown::Tile;
    let b = CelestialBody::Earth;
    for (nombre, cx, cz) in [("sierra cascadas", -6_100i64, -6_100i64), ("sierra", 1_400, -5_200)] {
        let (mut seco, mut seco_en_nieve, mut celdas) = (0u32, 0u32, 0u32);
        for tz in cz - 250..cz + 250 {
            for tx in cx - 250..cx + 250 {
                celdas += 1;
                let s = superficie_de(b, tx, tz);
                if s.pasto != Pasto::Seco {
                    continue;
                }
                seco += 1;
                // ¿tiene nieve a la vista, en un radio de 6 tiles?
                let nieve = (-6..=6i64).step_by(3).any(|dx| {
                    (-6..=6i64).step_by(3).any(|dz| {
                        superficie_de(b, tx + dx, tz + dz).tile == Tile::Ice
                    })
                });
                if nieve {
                    seco_en_nieve += 1;
                }
            }
        }
        println!(
            "{nombre:<18} celdas {celdas} | pasto seco {seco} | DE ÉSAS con nieve a 6 tiles: \
             {seco_en_nieve} ({:.2} % del cuadro)",
            100.0 * seco_en_nieve as f32 / celdas as f32
        );
        let alto = relief_level(b, cx, cz);
        println!("                   (nivel del centro: {alto})");
    }
}

/// **¿HACIA DÓNDE VA EL CAUCE DONDE PODRÍA HABER SALTO?** El histograma que
/// decide si la regla "sólo cascadas de frente" deja algo en pie.
#[test]
#[ignore]
fn rumbo_del_cauce_en_los_saltos() {
    use ha_shared::relief::{alto_de_pared, relief_cell, ReliefCell};
    let b = CelestialBody::Earth;
    for (cx, cz) in [(-6_100i64, -6_100i64), (-7_000, -6_100), (-4_300, -6_100)] {
        let (mut cand, mut sin_rumbo, mut sur, mut sur_cono, mut norte, mut lado) =
            (0u32, 0u32, 0u32, 0u32, 0u32, 0u32);
        for tz in cz - 300..cz + 300 {
            for tx in cx - 300..cx + 300 {
                if !hay_rio(b, tx, tz - 1) {
                    continue;
                }
                if !matches!(relief_cell(b, tx, tz), ReliefCell::Cliff | ReliefCell::Pared) {
                    continue;
                }
                if alto_de_pared(b, tx, tz) == 0 {
                    continue;
                }
                cand += 1;
                match ha_shared::rios::rumbo_del_cauce(b, tx, tz - 1) {
                    None => sin_rumbo += 1,
                    Some((dx, dz)) => {
                        if dz > 0.0 {
                            sur += 1;
                            if dz >= dx.abs() {
                                sur_cono += 1;
                            }
                        } else if dz < 0.0 {
                            norte += 1;
                        } else {
                            lado += 1;
                        }
                    }
                }
            }
        }
        println!(
            "({cx},{cz}) candidatos {cand} | sin rumbo {sin_rumbo} | al sur {sur} \
             (dentro del cono de 45°: {sur_cono}) | al norte {norte} | de lado {lado}"
        );
    }
}
