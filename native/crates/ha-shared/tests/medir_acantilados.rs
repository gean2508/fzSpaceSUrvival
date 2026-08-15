//! **MEDIR EL RELIEVE ANTES DE TOCARLO.** Ninguno de estos es un guardián: son
//! las reglas que se usaron para decidir qué estaba roto en el muro de terraza y
//! para saber si el arreglo sirvió. Están `#[ignore]` porque barren cientos de
//! miles de celdas y porque su respuesta es un número que se lee, no un
//! `assert`.
//!
//! ```sh
//! cargo test -p ha-shared --test medir_acantilados -- --ignored --nocapture
//! ```
//!
//! Lo que midieron, y por qué está acá:
//!
//! - `formas_de_cara`: el 99,4 % de las caras pide una pieza orientada. El
//!   autotile no era el problema.
//! - `esquinas_concavas`: **1.522 esquinas cóncavas contra 3.154 caras** — la
//!   mitad del perímetro no pedía pieza y el muro salía punteado. Ése era el
//!   problema (`world2d::spawn_chunk_canto_concavo`).
//! - `tamano_de_las_terrazas`: el 99,3 % de la tierra alta está en mesetas de
//!   400 tiles o más. Las mesetas YA se cierran; no hace falta ningún falloff.
//! - `edificado_sobre_acantilado`: 32.572 caras pisadas por algo edificado antes
//!   del retiro por desnivel (`city::retiro_del_desnivel`), 1.122 después.
//! - `lotes_partidos_por_una_cara`: el 24,3 % de las parcelas tenía una cara
//!   adentro y el 75,5 % ya estaba en llano perfecto.
//! - `el_muro_cambia_de_estilo`: el **8,86 %** de los pares de celdas vecinas de
//!   un mismo muro cambiaba de familia (bordillo de 2 celdas contra pared de 3)
//!   con la regla vieja de la pendiente. Desde que la familia la decide la
//!   ALTURA (`relief::alto_de_pared <= 1` es bordillo) queda en 0,08 %.
//! - `cuantos_escalones_baja_una_cara`: imprime DOS números que no son el mismo.
//!   El salto de terraza de una cara es siempre 1 (dos celdas pegadas no se
//!   llevan dos terrazas) — de leer sólo ése salió la doctrina de que "no existe
//!   la pared de dos escalones". El que importa es el ALTO DE LA PARED, que va
//!   de 1 a 10 porque `relief::SALTO_DE_TERRAZA` no reparte la escalera en
//!   escalones iguales.
//! - `piezas_del_muro` y `mapa_de_la_zona_mas_accidentada`: mapas ASCII para
//!   mirar un sitio concreto (`HA_X`/`HA_Z`).

use ha_shared::celestial_bodies::CelestialBody;
use ha_shared::relief::{alto_de_pared, relief_cell, relief_level, ReliefCell};
use ha_shared::topdown::Tile;

fn forma(n: bool, e: bool, s: bool, w: bool) -> &'static str {
    match (!n, !e, !s, !w) {
        (false, false, false, false) => "center(sin lado)",
        (true, false, false, false) => "edge_n",
        (false, true, false, false) => "edge_e",
        (false, false, true, false) => "edge_s",
        (false, false, false, true) => "edge_w",
        (true, true, false, false) => "corner_ne",
        (true, false, false, true) => "corner_nw",
        (false, true, true, false) => "corner_se",
        (false, false, true, true) => "corner_sw",
        (true, false, true, false) => "OPUESTOS n+s",
        (false, true, false, true) => "OPUESTOS e+w",
        (true, true, true, false) => "TRES sin w",
        (true, true, false, true) => "TRES sin s",
        (true, false, true, true) => "TRES sin e",
        (false, true, true, true) => "TRES sin n",
        (true, true, true, true) => "CUATRO",
    }
}

#[test]
#[ignore = "herramienta de medición, no un guardián"]
fn formas_de_cara() {
    let b = CelestialBody::Earth;
    let mut counts = std::collections::BTreeMap::new();
    let mut total = 0usize;
    for tz in -500..500 {
        for tx in -500..500 {
            if relief_cell(b, tx, tz) != ReliefCell::Cliff {
                continue;
            }
            let l = relief_level(b, tx, tz);
            let same = |dx: i64, dz: i64| relief_level(b, tx + dx, tz + dz) >= l;
            let f = forma(same(0, -1), same(1, 0), same(0, 1), same(-1, 0));
            *counts.entry(f).or_insert(0usize) += 1;
            total += 1;
        }
    }
    println!("\ncaras de acantilado: {total}");
    let mut v: Vec<_> = counts.into_iter().collect();
    v.sort_by_key(|(_, n)| std::cmp::Reverse(*n));
    for (k, n) in v {
        println!("  {k:18} {n:7}  {:5.1}%", n as f32 * 100.0 / total as f32);
    }
}

/// Cuántas celdas LLANAS son esquina cóncava del muro: los 4 vecinos al mismo
/// nivel o más alto, pero una diagonal más baja. El autotile del pack tiene una
/// pieza para eso (`inner_*`) y hoy no se pide nunca.
#[test]
#[ignore = "herramienta de medición, no un guardián"]
fn esquinas_concavas() {
    let b = CelestialBody::Earth;
    let (mut concavas, mut caras) = (0usize, 0usize);
    for tz in -400..400 {
        for tx in -400..400 {
            let l = relief_level(b, tx, tz);
            if l <= 0 {
                continue;
            }
            match relief_cell(b, tx, tz) {
                ReliefCell::Cliff => {
                    caras += 1;
                    continue;
                }
                ReliefCell::Ramp | ReliefCell::Pared => continue,
                ReliefCell::Flat => {}
            }
            let orto_alto = [(1i64, 0i64), (-1, 0), (0, 1), (0, -1)]
                .into_iter()
                .all(|(dx, dz)| relief_level(b, tx + dx, tz + dz) >= l);
            let diag_baja = [(1i64, 1i64), (-1, 1), (1, -1), (-1, -1)]
                .into_iter()
                .any(|(dx, dz)| {
                    let n = relief_level(b, tx + dx, tz + dz);
                    n >= 0 && n < l
                });
            if orto_alto && diag_baja {
                concavas += 1;
            }
        }
    }
    println!("\ncaras: {caras} · esquinas cóncavas sin pieza: {concavas} ({:.1}% del muro)", concavas as f32 * 100.0 / caras.max(1) as f32);
}

/// Busca la ventana más accidentada y la dibuja.
#[test]
#[ignore = "herramienta de medición, no un guardián"]
fn mapa_de_la_zona_mas_accidentada() {
    let b = CelestialBody::Earth;
    let (mut mejor, mut mejor_n) = ((0i64, 0i64), 0usize);
    for oz in (-3000..3000).step_by(120) {
        for ox in (-3000..3000).step_by(120) {
            let mut n = 0;
            for dz in (0..120).step_by(6) {
                for dx in (0..120).step_by(6) {
                    if relief_cell(b, ox + dx, oz + dz) == ReliefCell::Cliff {
                        n += 1;
                    }
                }
            }
            if n > mejor_n {
                mejor_n = n;
                mejor = (ox + 60, oz + 60);
            }
        }
    }
    println!("\nventana más accidentada: {mejor:?} ({mejor_n} caras muestreadas)");
    for rz in -22..22 {
        let mut line = String::new();
        for rx in -60..60 {
            let (tx, tz) = (mejor.0 + rx, mejor.1 + rz);
            let c = match relief_cell(b, tx, tz) {
                ReliefCell::Cliff => '#',
                ReliefCell::Pared => '=',
                ReliefCell::Ramp => '/',
                ReliefCell::Flat => {
                    let l = relief_level(b, tx, tz);
                    match l {
                        l if l <= -2 => '~',
                        -1 => ':',
                        l => char::from_digit(l.min(9) as u32, 10).unwrap_or('+'),
                    }
                }
            };
            line.push(c);
        }
        println!("{line}");
    }
}

/// ¿Cuántos LOTES de ciudad tienen una cara de acantilado adentro? Es el número
/// que decide si rechazarlos deja la ciudad en pie o la vacía.
#[test]
#[ignore = "herramienta de medición, no un guardián"]
fn lotes_partidos_por_una_cara() {
    let b = CelestialBody::Earth;
    let mut sitios = vec![];
    for sx in -12..12 {
        for sz in -12..12 {
            if let Some(s) = ha_shared::geography::settlement_in(b, sx, sz) {
                sitios.push(s);
            }
        }
    }
    let (mut lotes, mut con_cara, mut con_cara_pad) = (0usize, 0usize, 0usize);
    let mut desnivel = std::collections::BTreeMap::new();
    for s in sitios.iter().take(25) {
        let r = s.radius;
        let mut vistos = std::collections::HashSet::new();
        for dz in -r..r {
            for dx in -r..r {
                let (tx, tz) = (s.center.0 + dx, s.center.1 + dz);
                let Some(l) = ha_shared::city::lot_at(b, s, tx, tz) else { continue };
                if !vistos.insert(l.min) {
                    continue;
                }
                lotes += 1;
                let mut cara = false;
                let mut cara_pad = false;
                let (mut lo, mut hi) = (i8::MAX, i8::MIN);
                for j in -1..=l.h {
                    for i in -1..=l.w {
                        let (x, z) = (l.min.0 + i, l.min.1 + j);
                        let dentro = (0..l.w).contains(&i) && (0..l.h).contains(&j);
                        if relief_cell(b, x, z) == ReliefCell::Cliff {
                            cara_pad = true;
                            cara |= dentro;
                        }
                        if dentro {
                            let n = relief_level(b, x, z);
                            lo = lo.min(n);
                            hi = hi.max(n);
                        }
                    }
                }
                con_cara += cara as usize;
                con_cara_pad += cara_pad as usize;
                *desnivel.entry(hi - lo).or_insert(0usize) += 1;
            }
        }
    }
    println!("\nlotes: {lotes}");
    println!("  con una cara ADENTRO:            {con_cara}  ({:.1}%)", con_cara as f32 * 100.0 / lotes as f32);
    println!("  con una cara adentro o pegada:   {con_cara_pad}  ({:.1}%)", con_cara_pad as f32 * 100.0 / lotes as f32);
    println!("  desnivel (escalones) por lote: {desnivel:?}");
}

/// Qué PIEZA pide cada celda del muro alrededor de un punto. `HA_X`/`HA_Z`.
#[test]
#[ignore = "herramienta de medición, no un guardián"]
fn piezas_del_muro() {
    let b = CelestialBody::Earth;
    let ox: i64 = std::env::var("HA_X").ok().and_then(|v| v.parse().ok()).unwrap_or(-3244);
    let oz: i64 = std::env::var("HA_Z").ok().and_then(|v| v.parse().ok()).unwrap_or(2933);
    let sigla = |tx: i64, tz: i64| -> char {
        let l = relief_level(b, tx, tz);
        match relief_cell(b, tx, tz) {
            ReliefCell::Pared => '=',
            ReliefCell::Ramp => '/',
            ReliefCell::Flat => {
                if l <= 0 {
                    return '.';
                }
                // ¿esquina cóncava sin pieza?
                let orto = [(1i64, 0i64), (-1, 0), (0, 1), (0, -1)]
                    .into_iter()
                    .all(|(dx, dz)| relief_level(b, tx + dx, tz + dz) >= l);
                let diag = [(1i64, 1i64), (-1, 1), (1, -1), (-1, -1)]
                    .into_iter()
                    .any(|(dx, dz)| {
                        let n = relief_level(b, tx + dx, tz + dz);
                        n >= 0 && n < l
                    });
                if orto && diag {
                    '?'
                } else {
                    char::from_digit(l.min(9) as u32, 10).unwrap()
                }
            }
            ReliefCell::Cliff => {
                let same = |dx: i64, dz: i64| relief_level(b, tx + dx, tz + dz) >= l;
                match (!same(0, -1), !same(1, 0), !same(0, 1), !same(-1, 0)) {
                    (true, false, false, false) => 'n',
                    (false, true, false, false) => 'e',
                    (false, false, true, false) => 's',
                    (false, false, false, true) => 'w',
                    (true, true, false, false) => 'N', // corner_ne
                    (true, false, false, true) => 'M', // corner_nw
                    (false, true, true, false) => 'S', // corner_se
                    (false, false, true, true) => 'W', // corner_sw
                    _ => '*',
                }
            }
        }
    };
    println!("\npiezas alrededor de ({ox},{oz}):");
    println!("  n/e/s/w = edge · N/M/S/W = corner_ne/nw/se/sw · ? = esquina cóncava SIN pieza\n");
    for rz in -16..16 {
        let mut line = String::new();
        for rx in -40..40 {
            line.push(sigla(ox + rx, oz + rz));
        }
        println!("{line}");
    }
}

/// ¿De qué TAMAÑO son las terrazas? Una meseta que se lee como meseta mide
/// decenas de tiles; un islote de tres es una mancha de roca.
#[test]
#[ignore = "herramienta de medición, no un guardián"]
fn tamano_de_las_terrazas() {
    let b = CelestialBody::Earth;
    const R: i64 = 400;
    const N: usize = (2 * R) as usize;
    let mut lvl = vec![0i8; N * N];
    for j in 0..N {
        for i in 0..N {
            lvl[j * N + i] = relief_level(b, -R + i as i64, -R + j as i64);
        }
    }
    let mut visto = vec![false; N * N];
    let mut areas: Vec<usize> = vec![];
    for s in 0..N * N {
        if visto[s] || lvl[s] <= 0 {
            continue;
        }
        let l = lvl[s];
        let mut pila = vec![s];
        visto[s] = true;
        let mut area = 0usize;
        while let Some(p) = pila.pop() {
            area += 1;
            let (i, j) = (p % N, p / N);
            for (di, dj) in [(1i64, 0i64), (-1, 0), (0, 1), (0, -1)] {
                let (ni, nj) = (i as i64 + di, j as i64 + dj);
                if ni < 0 || nj < 0 || ni >= N as i64 || nj >= N as i64 {
                    continue;
                }
                let q = nj as usize * N + ni as usize;
                if !visto[q] && lvl[q] == l {
                    visto[q] = true;
                    pila.push(q);
                }
            }
        }
        areas.push(area);
    }
    areas.sort_unstable();
    let total: usize = areas.iter().sum();
    let cuenta = |lo: usize, hi: usize| areas.iter().filter(|&&a| a >= lo && a < hi).count();
    let sup = |lo: usize, hi: usize| areas.iter().filter(|&&a| a >= lo && a < hi).sum::<usize>();
    println!("\nterrazas (nivel > 0) en {N}x{N}: {} regiones, {total} tiles", areas.len());
    for (lo, hi, nombre) in [
        (1, 4, "1-3 tiles   (mota)"),
        (4, 16, "4-15 tiles  (islote)"),
        (16, 64, "16-63 tiles (parche)"),
        (64, 400, "64-399      (meseta chica)"),
        (400, usize::MAX, "400+        (meseta)"),
    ] {
        println!(
            "  {nombre:24} {:5} regiones · {:7} tiles ({:4.1}% de la tierra alta)",
            cuenta(lo, hi),
            sup(lo, hi),
            sup(lo, hi) as f32 * 100.0 / total.max(1) as f32
        );
    }
    if !areas.is_empty() {
        println!("  mediana {} tiles · máxima {}", areas[areas.len() / 2], areas[areas.len() - 1]);
    }
}

#[test]
#[ignore = "herramienta de medición, no un guardián"]
fn edificado_sobre_acantilado() {
    let b = CelestialBody::Earth;
    // Barre alrededor de unas cuantas ciudades reales.
    let mut sitios = vec![];
    for sx in -12..12 {
        for sz in -12..12 {
            if let Some(s) = ha_shared::geography::settlement_in(b, sx, sz) {
                sitios.push(s);
            }
        }
    }
    println!("\nnúcleos hallados: {}", sitios.len());
    let (mut celdas, mut choques, mut cortes_calle) = (0usize, 0usize, 0usize);
    let mut lotes_con_cara = 0usize;
    let mut lotes = 0usize;
    for s in sitios.iter().take(40) {
        let r = s.radius;
        let mut vistos = std::collections::HashSet::new();
        for dz in (-r..r).step_by(2) {
            for dx in (-r..r).step_by(2) {
                let (tx, tz) = (s.center.0 + dx, s.center.1 + dz);
                celdas += 1;
                let cara = relief_cell(b, tx, tz) == ReliefCell::Cliff;
                if !cara {
                    continue;
                }
                let t = ha_shared::topdown::topdown_tile(b, 0, tx, tz);
                // Si el tile final NO es acantilado, algo se plantó encima.
                if t != Tile::Cliff {
                    choques += 1;
                    if matches!(t, Tile::Asphalt | Tile::Pavement) {
                        cortes_calle += 1;
                    }
                }
                if let Some(l) = ha_shared::city::lot_at(b, s, tx, tz) {
                    if vistos.insert(l.min) {
                        lotes_con_cara += 1;
                    }
                }
                let _ = &mut lotes;
            }
        }
    }
    println!("celdas muestreadas: {celdas}");
    println!("caras de acantilado pisadas por algo edificado: {choques} (de ellas calzada/acera: {cortes_calle})");
    println!("lotes distintos con al menos una cara adentro: {lotes_con_cara}");
}

/// **¿Cuánto salta el estilo a lo largo de un mismo muro?** Dos vecinas de la
/// misma pared que piden arte distinto se ven como dos sets mezclados.
#[test]
#[ignore = "herramienta de medición, no un guardián"]
fn el_muro_cambia_de_estilo() {
    use ha_shared::geography::{biome_at, biome_tile};
    let ledge_look = |b: CelestialBody, tx: i64, tz: i64| ha_shared::relief::alto_de_pared(b, tx, tz) == 1;
    let b = CelestialBody::Earth;
    // material tal como lo resuelve `world2d::cliff_material`
    let material = |tx: i64, tz: i64| -> &'static str {
        let t = biome_tile(b, biome_at(b, tx, tz), tx, tz);
        match t {
            Tile::Sand => "arena",
            Tile::Ice => "nieve",
            Tile::Regolith => "regolito",
            Tile::Basalt | Tile::RockRed => "basalto",
            Tile::Water | Tile::WaterDeep => "pasto",
            _ => "pasto",
        }
    };
    let (mut pares, mut cambia_mat, mut cambia_fam) = (0usize, 0usize, 0usize);
    let mut usos: std::collections::BTreeMap<(&str, bool), usize> = Default::default();
    for tz in -600..600 {
        for tx in -600..600 {
            if relief_cell(b, tx, tz) != ReliefCell::Cliff {
                continue;
            }
            *usos.entry((material(tx, tz), ledge_look(b, tx, tz))).or_default() += 1;
            // vecinas de muro: las 8, para no perder las diagonales (el muro baja
            // en diagonal casi siempre).
            for (dx, dz) in [(1i64, 0i64), (0, 1), (1, 1), (1, -1)] {
                let (nx, nz) = (tx + dx, tz + dz);
                if relief_cell(b, nx, nz) != ReliefCell::Cliff {
                    continue;
                }
                pares += 1;
                if material(tx, tz) != material(nx, nz) {
                    cambia_mat += 1;
                }
                if ledge_look(b, tx, tz) != ledge_look(b, nx, nz) {
                    cambia_fam += 1;
                }
            }
        }
    }
    println!("\npares de celdas de muro vecinas: {pares}");
    println!(
        "  cambian de MATERIAL (cliff/arena vs cliff/pasto…): {cambia_mat} ({:.1}%)",
        cambia_mat as f32 * 100.0 / pares.max(1) as f32
    );
    println!(
        "  cambian de FAMILIA (bordillo de 2 celdas vs pared de 3): {cambia_fam} ({:.1}%)",
        cambia_fam as f32 * 100.0 / pares.max(1) as f32
    );
    println!("  reparto (material, ¿es bordillo?): {usos:?}");
}

/// ¿Cuánto BAJA de verdad una cara? Son dos números distintos y confundirlos
/// costó una vuelta entera:
///
/// - **el salto de TERRAZA** (`relief_level` de acá menos el del vecino) es
///   siempre 1 y lo va a ser siempre, porque el campo de altura no puede cambiar
///   más de `MAX_FIELD_STEP_PER_TILE` por tile: dos celdas pegadas nunca se
///   llevan dos terrazas. Este test imprimía sólo eso, salía `{1: 7779}` y de ahí
///   nació la doctrina de que "en este mundo no existe la pared de dos
///   escalones";
/// - **el alto de la PARED** ([`alto_de_pared`]) es lo que mide el muro que
///   cuelga de ese canto, y ése va de 1 a 10 según la altura, porque
///   `relief::SALTO_DE_TERRAZA` no reparte la escalera en escalones iguales.
///
/// El segundo es el que decide cuántos `cuerpo` apila el kit y cuántos tramos
/// mide la cascada. Se imprimen los dos, para que no se vuelvan a confundir.
#[test]
#[ignore = "herramienta de medición, no un guardián"]
fn cuantos_escalones_baja_una_cara() {
    let b = CelestialBody::Earth;
    let mut caida: std::collections::BTreeMap<i8, usize> = Default::default();
    let mut alto: std::collections::BTreeMap<i64, usize> = Default::default();
    let mut sur = 0usize;
    for tz in -600..600 {
        for tx in -600..600 {
            if relief_cell(b, tx, tz) != ReliefCell::Cliff {
                continue;
            }
            let l = relief_level(b, tx, tz);
            // el desnivel MÁS GRANDE contra un vecino ortogonal pisable
            let d = [(1i64, 0i64), (-1, 0), (0, 1), (0, -1)]
                .into_iter()
                .filter_map(|(dx, dz)| {
                    let n = relief_level(b, tx + dx, tz + dz);
                    (n >= 0 && n < l).then_some(l - n)
                })
                .max()
                .unwrap_or(0);
            *caida.entry(d).or_default() += 1;
            let h = alto_de_pared(b, tx, tz);
            if h > 0 {
                *alto.entry(h).or_default() += 1;
            }
            if relief_level(b, tx, tz + 1) < l {
                sur += 1;
            }
        }
    }
    let total: usize = caida.values().sum();
    println!("\ncaras: {total} · de ellas miran al SUR (enseñan pared entera): {sur} ({:.1}%)", sur as f32 * 100.0 / total.max(1) as f32);
    println!("salto de TERRAZA de la cara (siempre 1, por construcción): {caida:?}");
    println!("alto de la PARED que cuelga, en escalones: {alto:?}");
}

/// **EL CORTE VERTICAL DE UNA CASCADA.** El sitio se busca —no se escribe a
/// mano— y se vuelca la columna del salto celda por celda: qué hay arriba del
/// labio, qué tile es cada fila de la caída, dónde empieza el agua de abajo y
/// hasta dónde llega el DIBUJO que hoy pone `world2d::spawn_chunk_cascadas`.
///
/// Es la medición que hace falta para saber si el chorro queda al ras de la
/// meseta: sin ella lo único que hay es una captura y el ojo.
#[test]
#[ignore = "herramienta de medición, no un guardián"]
fn corte_de_una_cascada() {
    use ha_shared::agua::{cascada_at, ANCHO_CASCADA};
    let b = CelestialBody::Earth;
    let mut sitios = vec![];
    let mut cz = -16_000i64;
    'busca: while cz < 16_000 {
        let mut cx = -16_000i64;
        while cx < 16_000 {
            if relief_level(b, cx, cz) >= 5 {
                for tz in cz - 90..cz + 90 {
                    for tx in cx - 90..cx + 90 {
                        if let Some(alto) = cascada_at(b, tx, tz) {
                            sitios.push((tx, tz, alto));
                            if sitios.len() >= 6 {
                                break 'busca;
                            }
                        }
                    }
                }
            }
            cx += 900;
        }
        cz += 900;
    }
    println!("\ncascadas halladas: {}", sitios.len());
    for (tx, tz, alto) in sitios {
        println!(
            "\n=== salto en ({tx},{tz}) alto={alto} · terraza del labio={} · terraza al pie={}",
            relief_level(b, tx, tz - 1),
            relief_level(b, tx, tz + 1)
        );
        for dz in -3..(alto + 6) {
            let z = tz + dz;
            let mut fila = String::new();
            for dx in -2..(ANCHO_CASCADA + 2) {
                let x = tx + dx;
                let c = match relief_cell(b, x, z) {
                    ReliefCell::Cliff => '#',
                    ReliefCell::Pared => '+',
                    ReliefCell::Ramp => '/',
                    ReliefCell::Flat => match ha_shared::geography::natural_tile(b, x, z) {
                        Tile::Water => '~',
                        Tile::WaterDeep => '=',
                        _ => char::from_digit(relief_level(b, x, z).clamp(0, 9) as u32, 10)
                            .unwrap_or('+'),
                    },
                };
                fila.push(c);
            }
            let marca = if dz < 0 {
                "arriba"
            } else if dz == 0 {
                "LABIO (canto de la pared)"
            } else if dz <= alto {
                "caída"
            } else {
                "abajo"
            };
            println!("  z{dz:+3}  {fila}   {marca}");
        }
    }
}

/// ¿Baja un río de sierra encadenando saltos, o cada cascada está sola?
///
/// La pregunta la abrió atar el alto del chorro al desnivel: si todos los saltos
/// miden un piso, la variedad de tamaño tiene que venir de otro lado o no venir.
#[test]
#[ignore = "herramienta de medición, no un guardián"]
fn saltos_encadenados() {
    use ha_shared::agua::cascada_at;
    let b = CelestialBody::Earth;
    let mut anclas = vec![];
    let mut cz = -16_000i64;
    while cz < 16_000 {
        let mut cx = -16_000i64;
        while cx < 16_000 {
            if relief_level(b, cx, cz) >= 4 {
                for tz in cz - 120..cz + 120 {
                    for tx in cx - 120..cx + 120 {
                        if cascada_at(b, tx, tz).is_some() {
                            anclas.push((tx, tz));
                        }
                    }
                }
            }
            cx += 1_500;
        }
        cz += 1_500;
    }
    println!("\nanclas de cascada halladas: {}", anclas.len());
    let mut con_vecina = 0usize;
    let mut distancias = vec![];
    for &(tx, tz) in &anclas {
        let mut mejor: Option<i64> = None;
        for &(ox, oz) in &anclas {
            if (ox, oz) == (tx, tz) || oz <= tz {
                continue;
            }
            if (ox - tx).abs() <= 12 && oz - tz <= 90 {
                mejor = Some(mejor.map_or(oz - tz, |m: i64| m.min(oz - tz)));
            }
        }
        if let Some(d) = mejor {
            con_vecina += 1;
            distancias.push(d);
        }
    }
    distancias.sort_unstable();
    println!(
        "con otro salto río abajo (±12 tiles, hasta 90 abajo): {con_vecina} · distancias: {:?}",
        &distancias[..distancias.len().min(20)]
    );
    // Y cuántas PAREDES cruza el río más abajo, sean ancla o no: es lo que dice
    // si el desnivel sigue escalonando aunque no se dibuje chorro.
    let mut escalones = vec![];
    for &(tx, tz) in anclas.iter().take(40) {
        let n = (1..90)
            .filter(|k| {
                (0..3).any(|d| relief_cell(b, tx + d, tz + k) == ReliefCell::Cliff)
            })
            .count();
        escalones.push(n);
    }
    println!("paredes que el río vuelve a cruzar en los 90 tiles de abajo: {escalones:?}");
}

/// **CUÁNTA ROCA SE AHORRAN LOS TOPES DE LA PARED.**
///
/// Compara el alto que pide la escalera del mundo (`SALTO_DE_TERRAZA`) con el que
/// de verdad se dibuja después de `relief::meseta_detras` y `relief::hueco_al_sur`.
/// Es la medida que justifica los dos topes: un montículo de dos filas pedía diez
/// escalones y tapaba once filas de la llanura de abajo.
#[test]
#[ignore = "herramienta de medición, no un guardián"]
fn cuanto_recortan_los_topes_de_la_pared() {
    use ha_shared::relief::{alto_de_pared, altura_de_terraza, relief_level, MAX_PARED};
    let b = CelestialBody::Earth;
    let (mut cantos, mut bruto, mut capado) = (0u64, 0i64, 0i64);
    let mut hist_bruto = [0u32; 11];
    let mut hist_capado = [0u32; 11];
    for (cx, cz) in [(0i64, 0i64), (1_900, -4_900), (-3_450, 3_150), (2_400, -1_800)] {
        for tz in cz - 200..cz + 200 {
            for tx in cx - 200..cx + 200 {
                let l = relief_level(b, tx, tz);
                if l <= 0 {
                    continue;
                }
                let s = relief_level(b, tx, tz + 1);
                if s < 0 || s >= l {
                    continue;
                }
                let raw =
                    (altura_de_terraza(l) - altura_de_terraza(s)).clamp(1, MAX_PARED as i32) as i64;
                let cap = alto_de_pared(b, tx, tz);
                cantos += 1;
                bruto += raw + 1;
                capado += cap + 1;
                hist_bruto[(raw as usize).min(10)] += 1;
                hist_capado[(cap as usize).min(10)] += 1;
            }
        }
    }
    println!(
        "cantos {cantos} · filas de roca: bruto {bruto} · con topes {capado} ({:.0}% menos)",
        (1.0 - capado as f64 / bruto.max(1) as f64) * 100.0
    );
    println!("  alto bruto  (1..10): {:?}", &hist_bruto[1..]);
    println!("  alto capado (1..10): {:?}", &hist_capado[1..]);
}

/// **AGUA QUE EL AUTOTILE NO SABE DIBUJAR.**
///
/// Una celda mojada con tres o cuatro lados en tierra —o con los dos lados
/// opuestos en tierra— no tiene pieza en el pack: el cliente cae al interior y
/// pinta un cuadrado liso sin orilla. Es el bug que el usuario marcó en círculo
/// rojo por todo el borde de la laguna. Con el charco de pantano a período 9 esto
/// daba **1,70 %**; con `geography::CHARCO_PERIODO` y `orilla_dibujable`, 0,36 %.
#[test]
#[ignore = "herramienta de medición, no un guardián"]
fn agua_que_no_se_puede_dibujar() {
    use ha_shared::geography::natural_tile;
    use ha_shared::topdown::Tile;
    let b = CelestialBody::Earth;
    let mojada = |tx: i64, tz: i64| matches!(natural_tile(b, tx, tz), Tile::Water | Tile::WaterDeep);
    let (mut agua, mut tres, mut cuatro, mut opuestos) = (0u64, 0u64, 0u64, 0u64);
    for tz in -2_300..-1_700 {
        for tx in 1_900..2_500 {
            if !mojada(tx, tz) {
                continue;
            }
            agua += 1;
            let (n, e, s, w) =
                (mojada(tx, tz - 1), mojada(tx + 1, tz), mojada(tx, tz + 1), mojada(tx - 1, tz));
            match [n, e, s, w].iter().filter(|v| !**v).count() {
                4 => cuatro += 1,
                3 => tres += 1,
                2 if (!n && !s) || (!e && !w) => opuestos += 1,
                _ => {}
            }
        }
    }
    let malas = tres + cuatro + opuestos;
    println!(
        "agua {agua} · sin forma {malas} ({:.2} %) — sueltas {cuatro}, penínsulas {tres}, canal de 1 {opuestos}",
        malas as f64 * 100.0 / agua.max(1) as f64
    );
}

/// **VOLCADO ASCII DE UNA ZONA**: tile natural, celda de relieve y escalón, lado a
/// lado. Es la herramienta con la que se vio que una loma entera salía de roca y
/// que las manchas de nivel 9 tenían dos filas de fondo — cosas que ninguna media
/// enseña porque son de la FORMA, no del promedio.
///
/// `ZX`/`ZZ`/`ZR` mueven el centro y el radio:
/// `ZX=1763 ZZ=-5086 ZR=26 cargo test -p ha-shared --test medir_acantilados -- --ignored --nocapture mapa_de_zona`
#[test]
#[ignore = "herramienta de medición, no un guardián"]
fn mapa_de_zona() {
    use ha_shared::geography::natural_tile;
    use ha_shared::relief::relief_level;
    use ha_shared::topdown::Tile;
    let env = |k: &str, d: i64| std::env::var(k).ok().and_then(|s| s.parse().ok()).unwrap_or(d);
    let b = CelestialBody::Earth;
    let (cx, cz, r) = (env("ZX", 1_763), env("ZZ", -5_086), env("ZR", 26));
    for tz in cz - r..cz + r {
        let mut s = String::new();
        let mut lv = String::new();
        for tx in cx - r..cx + r {
            s.push(match natural_tile(b, tx, tz) {
                Tile::Water => 'w',
                Tile::WaterDeep => 'W',
                Tile::Grass => '.',
                Tile::Sand => 's',
                Tile::Ice => 'I',
                Tile::Dirt => 'd',
                Tile::Slope => '/',
                Tile::Cliff => match relief_cell(b, tx, tz) {
                    ReliefCell::Cliff => 'M',
                    ReliefCell::Pared => '#',
                    ReliefCell::Ramp => '/',
                    ReliefCell::Flat => 'c',
                },
                _ => '?',
            });
            let l = relief_level(b, tx, tz);
            lv.push(if l < 0 { '~' } else { char::from_digit(l as u32 % 10, 10).unwrap() });
        }
        println!("{tz:>7} {s}  {lv}");
    }
}

/// **¿EL TERRENO SUBE ESCALÓN POR ESCALÓN, O SALTA?**
///
/// El histograma dice que el 13 % del planeta vive en los niveles 4-7 y que de
/// esos niveles **no cuelga ni una cara**. Las dos cosas juntas sólo pueden ser
/// ciertas si esos niveles no son terrazas sino franjas: el terreno los cruza tan
/// rápido que nunca llegan a tener un canto propio.
///
/// Esto lo mide de frente: un transecto de la llanura a la cordillera, contando
/// cuántos tiles seguidos dura cada nivel. Una terraza de verdad dura decenas de
/// tiles; una franja degenerada, uno o dos.
#[test]
#[ignore = "herramienta de medición, no un guardián"]
fn cuanto_dura_cada_nivel() {
    use ha_shared::relief::relief_level;
    let b = CelestialBody::Earth;
    let mut tramos: std::collections::BTreeMap<i8, Vec<i64>> = Default::default();
    // Cuatro transectos largos que salen del llano y entran en la cordillera.
    for (x0, z0, dx, dz) in [
        (0i64, 0i64, 1i64, -3i64),
        (0, 0, 2, -5),
        (500, -1000, 1, -4),
        (-1000, 1000, 3, -7),
    ] {
        let (mut actual, mut largo) = (relief_level(b, x0, z0), 0i64);
        for i in 0..6_000i64 {
            let l = relief_level(b, x0 + dx * i, z0 + dz * i);
            if l == actual {
                largo += 1;
            } else {
                if actual >= 0 {
                    tramos.entry(actual).or_default().push(largo);
                }
                actual = l;
                largo = 1;
            }
        }
    }
    println!("\ncuánto dura cada nivel, en tiles seguidos de un transecto:");
    println!("  nivel  tramos   mediana   máx   (un nivel que no es superficie da mediana 1-2)");
    for (nivel, mut v) in tramos {
        v.sort_unstable();
        let med = v[v.len() / 2];
        let max = *v.last().unwrap();
        println!("  {nivel:>5} {:>7} {med:>9} {max:>5}", v.len());
    }
}

/// **¿POR QUÉ NO CUELGA NADA DE LOS NIVELES 4 A 7?**
///
/// El histograma decía que el 13 % del planeta vive ahí y que ninguna cara cae de
/// esos escalones, y `cuanto_dura_cada_nivel` descartó la primera hipótesis (no
/// son franjas: duran tanto como el resto). Así que el escalón existe y el canto
/// se pierde en el camino. Esto abre el camino: por cada nivel, cuántas celdas
/// hay, cuántas bajan al sur, cuántas de ésas pasan el descarte barato / `es_canto`
/// / y en qué termina clasificándolas `relief_cell`.
#[test]
#[ignore = "herramienta de medición, no un guardián"]
fn donde_se_pierde_el_canto_de_4_a_7() {
    use ha_shared::relief::canto_de_terraza;
    let b = CelestialBody::Earth;
    // Las mismas ventanas del histograma de `relief.rs`.
    const VENTANAS: &[(i64, i64)] = &[
        (1_900, -4_900),
        (4_092, -4_960),
        (0, 0),
        (2_400, -1_800),
        (-3_100, 900),
        (1_500, 3_300),
        (-2_000, -2_600),
        (4_200, 1_100),
        (-3_450, 3_150),
        (3_000, 3_000),
    ];
    #[derive(Default, Clone, Copy)]
    struct Fila {
        celdas: u32,
        baja_al_sur: u32,
        canto: u32,
        cliff: u32,
        ramp: u32,
        pared: u32,
        flat: u32,
    }
    let mut por_nivel: std::collections::BTreeMap<i8, Fila> = Default::default();
    for (ox, oz) in VENTANAS {
        for tz in oz - 200..oz + 200 {
            for tx in ox - 200..ox + 200 {
                let l = relief_level(b, tx, tz);
                if l <= 0 {
                    continue;
                }
                let f = por_nivel.entry(l).or_default();
                f.celdas += 1;
                let s = relief_level(b, tx, tz + 1);
                if !(s >= 0 && s < l) {
                    continue;
                }
                f.baja_al_sur += 1;
                if canto_de_terraza(b, tx, tz) {
                    f.canto += 1;
                }
                match relief_cell(b, tx, tz) {
                    ReliefCell::Cliff => f.cliff += 1,
                    ReliefCell::Ramp => f.ramp += 1,
                    ReliefCell::Pared => f.pared += 1,
                    ReliefCell::Flat => f.flat += 1,
                }
            }
        }
    }
    println!("\ndónde se pierde el canto (sólo celdas que BAJAN al sur):");
    println!("  nivel   celdas  bajan-sur   es_canto |  Cliff   Ramp  Pared   Flat");
    for (n, f) in &por_nivel {
        println!(
            "  {n:>5} {:>8} {:>10} {:>10} | {:>6} {:>6} {:>6} {:>6}",
            f.celdas, f.baja_al_sur, f.canto, f.cliff, f.ramp, f.pared, f.flat
        );
    }
}

/// **Y DÓNDE ESTÁN, ENTONCES, LOS NIVELES 4 A 7.** El barrido por ventanas dice
/// que ahí no hay ni una celda; la rejilla ancha del histograma dice que son el
/// 13 % del planeta. Los dos no pueden tener razón: esto va a buscar las celdas
/// de esos niveles donde de verdad estén y mira, alrededor de cada una, si de su
/// terraza cuelga alguna cara.
#[test]
#[ignore = "herramienta de medición, no un guardián"]
fn los_niveles_4_a_7_donde_estan() {
    use ha_shared::relief::canto_de_terraza;
    let b = CelestialBody::Earth;
    let mut niveles: std::collections::BTreeMap<i8, u32> = Default::default();
    let mut muestras: std::collections::BTreeMap<i8, Vec<(i64, i64)>> = Default::default();
    for i in -260..260 {
        for j in -260..260 {
            let (tx, tz) = (i * 17, j * 17);
            let l = relief_level(b, tx, tz);
            *niveles.entry(l).or_default() += 1;
            if (4..=7).contains(&l) {
                let v = muestras.entry(l).or_default();
                v.push((tx, tz));
            }
        }
    }
    let total: u32 = niveles.values().sum();
    println!("\nrejilla ancha (±4.420 tiles, paso 17): {total} muestras");
    for (n, c) in &niveles {
        println!("  nivel {n:3}  {c:7}  {:5.2}%", *c as f32 * 100.0 / total as f32);
    }
    println!("\nagregado de 20 ventanas de 61×61 repartidas, por nivel:");
    for (n, sitios) in &muestras {
        let paso = (sitios.len() / 20).max(1);
        let elegidos: Vec<(i64, i64)> = sitios.iter().copied().step_by(paso).take(20).collect();
        let (mut t_mismo, mut t_baja, mut t_cliff, mut t_ramp, mut t_pared) = (0u32, 0u32, 0u32, 0u32, 0u32);
        for (cx, cz) in elegidos {
            let (mut mismo, mut baja, mut canto, mut cliff, mut ramp, mut pared) =
                (0u32, 0u32, 0u32, 0u32, 0u32, 0u32);
            for tz in cz - 30..=cz + 30 {
                for tx in cx - 30..=cx + 30 {
                    if relief_level(b, tx, tz) != *n {
                        continue;
                    }
                    mismo += 1;
                    let s = relief_level(b, tx, tz + 1);
                    if !(s >= 0 && s < *n) {
                        continue;
                    }
                    baja += 1;
                    if canto_de_terraza(b, tx, tz) {
                        canto += 1;
                    }
                    match relief_cell(b, tx, tz) {
                        ReliefCell::Cliff => cliff += 1,
                        ReliefCell::Ramp => ramp += 1,
                        ReliefCell::Pared => pared += 1,
                        ReliefCell::Flat => {}
                    }
                }
            }
            let _ = canto;
            t_mismo += mismo;
            t_baja += baja;
            t_cliff += cliff;
            t_ramp += ramp;
            t_pared += pared;
        }
        println!(
            "  nivel {n}: celdas={t_mismo:6} bajan-sur={t_baja:5} · Cliff={t_cliff:5} Ramp={t_ramp:4} Pared={t_pared:4}  → una cara cada {:.0} celdas",
            t_mismo as f32 / t_cliff.max(1) as f32
        );
    }
}
