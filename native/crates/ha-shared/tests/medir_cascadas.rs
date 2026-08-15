//! **MEDIR LAS CASCADAS Y LAS MESETAS ANTES DE TOCARLAS.** Hermano de
//! `medir_acantilados`: ninguno de estos es un guardián, son las preguntas que
//! hay que responder con un número antes de decidir qué está roto en el salto de
//! agua y en el cierre de una meseta.
//!
//! ```sh
//! cargo test -p ha-shared --test medir_cascadas -- --ignored --nocapture
//! ```
//!
//! Lo que mide cada uno:
//!
//! - `material_de_las_cascadas`: de qué material es la pared que rodea a cada
//!   salto. Decide cuántas variantes teñidas hay que cortar del recorte del pack
//!   (hoy el chorro trae su propia roca terracota y no se tiñe de nada).
//! - `encaje_de_la_cascada`: si la boca cae en la misma fila que el labio de la
//!   pared de al lado y si la poza cae en la del pie. Las dos puntas, que es lo
//!   que se ve.
//! - `pie_de_pared_contra_agua`: cuántas filas de llano quedan entre el pie
//!   dibujado de una pared y el líquido. La regla pide dos: la orilla y una de
//!   césped puro.
//! - `mesetas_selladas`: mesetas sin una sola rampa ni bordillo por donde subir.

use ha_shared::agua::{
    cascada_at, filas_de_hombro, filas_de_dibujo, hombro_de_garganta, ANCHO_CASCADA,
};
use ha_shared::celestial_bodies::CelestialBody;
use ha_shared::geography::{biome_at, biome_tile, Biome};
use ha_shared::relief::{alto_de_pared, climbable_edge, relief_cell, relief_level, ReliefCell};

/// Un bordillo es una pared de un escalón: dos celdas de dibujo. Lo que antes
/// decidía una cota regional ahora lo dice la altura (ver `relief::SALTO_DE_TERRAZA`).
fn ledge_look(b: CelestialBody, tx: i64, tz: i64) -> bool {
    alto_de_pared(b, tx, tz) == 1
}
use ha_shared::topdown::Tile;
use std::collections::BTreeMap;

use ha_shared::agua::cauce;

/// El material de la pared tal como lo resuelve `world2d::cliff_material`:
/// `suelo_del_relieve` (el tile de bioma, con el mojado pasado a pasto) mapeado a
/// las cinco familias que corta `import-acantilado.py`.
fn material(b: CelestialBody, tx: i64, tz: i64) -> &'static str {
    let t = biome_tile(b, biome_at(b, tx, tz), tx, tz);
    match t {
        Tile::Sand => "arena",
        Tile::Ice => "nieve",
        Tile::Regolith => "regolito",
        Tile::Basalt | Tile::RockRed => "basalto",
        _ => "pasto",
    }
}

/// La clave de arte que pediría `world2d::terrain_art_key` para una cara de
/// acantilado acá — incluidos los cuatro pastos de clima.
fn clave_de_pared(b: CelestialBody, tx: i64, tz: i64) -> String {
    let familia = if ledge_look(b, tx, tz) { "ledge" } else { "cliff" };
    let mat = material(b, tx, tz);
    if mat != "pasto" {
        return format!("{familia}/{mat}");
    }
    let clima = match biome_at(b, tx, tz) {
        Biome::Forest => "pasto_bosque",
        Biome::Savanna => "pasto_seco",
        Biome::Swamp => "pasto_pantano",
        Biome::Highland | Biome::Mountain => "pasto_sierra",
        _ => "pasto",
    };
    format!("{familia}/{clima}")
}

/// Recorre las anclas de cascada de una región y las devuelve con sus pisos.
fn anclas(b: CelestialBody, x0: i64, x1: i64, z0: i64, z1: i64) -> Vec<(i64, i64, i64)> {
    let mut v = vec![];
    for tz in z0..z1 {
        for tx in x0..x1 {
            if relief_cell(b, tx, tz) != ReliefCell::Cliff {
                continue;
            }
            if let Some(pisos) = cascada_at(b, tx, tz) {
                v.push((tx, tz, pisos));
            }
        }
    }
    v
}

/// **¿DE QUÉ COLOR ES LA ROCA ALREDEDOR DE CADA SALTO?**
///
/// El recorte del pack (`Waterfall_Modular_*`) trae su garganta dibujada: dos
/// columnas de roca terracota con estrías verticales. El kit de acantilado, en
/// cambio, se TIÑE con la losa que corona (`import-acantilado.py::LOSA_DEL_MATERIAL`)
/// y sale un canto rodado gris malva. Si las cascadas caen sobre pared de pasto
/// de sierra, el chorro es una cicatriz naranja en un muro verde grisáceo.
#[test]
#[ignore = "herramienta de medición, no un guardián"]
fn material_de_las_cascadas() {
    let b = CelestialBody::Earth;
    let mut por_clave: BTreeMap<String, usize> = Default::default();
    let mut bordillos = 0usize;
    let mut sitios = anclas(b, -900, 900, -900, 900);
    // **Y la SIERRA aparte.** El recorte de alrededor del origen es casi todo
    // llano, y ahí no sale un solo `pasto_sierra`: si las cascadas de montaña
    // piden otra losa, hay que teñir esa variante también. Se busca la primera
    // sierra de verdad (terraza ≥ 5) como hace `medir_acantilados::corte_de_una_cascada`.
    let mut sierras = 0usize;
    'busca: for cz in (-16_000..16_000).step_by(900) {
        for cx in (-16_000..16_000).step_by(900) {
            if relief_level(b, cx, cz) < 5 {
                continue;
            }
            let v = anclas(b, cx - 250, cx + 250, cz - 250, cz + 250);
            sierras += v.len();
            sitios.extend(v);
            if sierras >= 20 {
                break 'busca;
            }
        }
    }
    println!("  (de ellas, {sierras} halladas en sierra de terraza >= 5)");
    for &(tx, tz, _) in &sitios {
        // La pared que se ve al lado de la garganta: la celda justo fuera del
        // ancho del chorro, a los dos costados.
        for x in [tx - 1, tx + ANCHO_CASCADA] {
            *por_clave.entry(clave_de_pared(b, x, tz)).or_default() += 1;
            if ledge_look(b, x, tz) {
                bordillos += 1;
            }
        }
    }
    println!("\nanclas de cascada en 1.800x1.800: {}", sitios.len());
    println!("paredes contiguas por clave de arte:");
    for (k, n) in &por_clave {
        println!("  {k:24} {n}");
    }
    println!(
        "  de esas, BORDILLO (dibujo de 2 celdas, no 3): {bordillos} de {}",
        sitios.len() * 2
    );
}

/// **LAS DOS PUNTAS DEL CHORRO.**
///
/// Arriba: la boca se dibuja en la fila `tz` del ancla, pero las tres columnas
/// del salto pueden arrancar en filas distintas (`agua::salto_vecino` tolera un
/// tile) y la pared de al lado cuelga su labio en SU fila. Si no coinciden, el
/// canto de roca asoma por encima del agua.
///
/// Abajo: la pared de acantilado se dibuja con tres celdas (labio, cuerpo, pie)
/// y el bordillo con DOS. La cascada dibuja `1 + pisos + alto_de_poza` filas. Si
/// no coinciden, el chorro termina antes o después que la roca de al lado.
#[test]
#[ignore = "herramienta de medición, no un guardián"]
fn encaje_de_la_cascada() {
    let b = CelestialBody::Earth;
    let sitios = anclas(b, -900, 900, -900, 900);
    let mut desfase_columna: BTreeMap<i64, usize> = Default::default();
    let mut desfase_pared: BTreeMap<i64, usize> = Default::default();
    let mut alto_pared: BTreeMap<&str, usize> = Default::default();
    let mut agua_al_pie: BTreeMap<&str, usize> = Default::default();

    for &(tx, tz, pisos) in &sitios {
        // ¿En qué fila tiene su canto cada una de las tres columnas del chorro?
        for dx in 0..ANCHO_CASCADA {
            let mut hallada = None;
            for dz in -2..=2 {
                let (x, z) = (tx + dx, tz + dz);
                if relief_level(b, x, z + 1) < relief_level(b, x, z)
                    && relief_level(b, x, z) > 0
                    && cauce(b, x, z - 1)
                {
                    hallada = Some(dz);
                    break;
                }
            }
            *desfase_columna.entry(hallada.unwrap_or(99)).or_default() += 1;
        }
        // ¿Y las dos paredes que flanquean la garganta?
        for x in [tx - 1, tx + ANCHO_CASCADA] {
            let mut hallada = None;
            for dz in -2..=2 {
                let z = tz + dz;
                if relief_cell(b, x, z) == ReliefCell::Cliff
                    && relief_level(b, x, z + 1) < relief_level(b, x, z)
                {
                    hallada = Some(dz);
                    break;
                }
            }
            *desfase_pared.entry(hallada.unwrap_or(99)).or_default() += 1;
            *alto_pared
                .entry(if ledge_look(b, x, tz) { "bordillo (2)" } else { "pared (3)" })
                .or_default() += 1;
        }
        // El pie del dibujo del chorro: fila `tz + 1 + pisos` (la primera de la
        // poza) y la de abajo (el reventón). ¿Sobre qué caen?
        let filas = filas_de_dibujo(pisos);
        for (nombre, z) in [("poza 1.ª", tz + 1 + pisos), ("reventón", tz + 2 + pisos)] {
            let mojado = cauce(b, tx + 1, z);
            *agua_al_pie.entry(if mojado { nombre } else { "SECO" }).or_default() += 1;
        }
        let _ = filas;
    }

    println!("\nanclas: {}", sitios.len());
    println!("desfase en filas del canto de cada COLUMNA del chorro (0 = donde se dibuja la boca):");
    for (d, n) in &desfase_columna {
        println!("  {}{d:+}  {n}", if *d == 99 { "sin canto " } else { "" });
    }
    println!("desfase del canto de la PARED que flanquea la garganta:");
    for (d, n) in &desfase_pared {
        println!("  {}{d:+}  {n}", if *d == 99 { "sin canto " } else { "" });
    }
    println!("alto dibujado de esa pared: {alto_pared:?}");
    println!("qué hay bajo las filas de poza: {agua_al_pie:?}");
}

/// **NINGUNA PARED CON LOS PIES EN EL AGUA — Y UN CÉSPED DE POR MEDIO.**
///
/// Regla del usuario: entre el pie dibujado del acantilado y el líquido tiene que
/// haber al menos DOS celdas — la que dibuja el borde césped/agua y una de césped
/// puro, que es la que toca la roca. Mide cuántas filas de llano hay de verdad.
#[test]
#[ignore = "herramienta de medición, no un guardián"]
fn pie_de_pared_contra_agua() {
    let b = CelestialBody::Earth;
    let mut reparto: BTreeMap<i64, usize> = Default::default();
    let mut caras = 0usize;
    let mut pegadas: Vec<(i64, i64, i64)> = vec![];
    for tz in -600..600 {
        for tx in -600..600 {
            if relief_cell(b, tx, tz) != ReliefCell::Cliff {
                continue;
            }
            let l = relief_level(b, tx, tz);
            // Sólo las caras que CUELGAN al sur: son las que enseñan la pared.
            if relief_level(b, tx, tz + 1) >= l {
                continue;
            }
            if ha_shared::agua::en_la_garganta(b, tx, tz) {
                continue;
            }
            caras += 1;
            // El pie dibujado: 2 filas abajo si es pared de 3 celdas, 1 si es
            // bordillo de 2.
            let pie = alto_de_pared(b, tx, tz) + 1;
            // Cuántas filas de tierra hay entre el pie y el primer líquido.
            let mut libres = -1i64;
            for k in 0..12 {
                let z = tz + pie + 1 + k;
                if ha_shared::geography::natural_tile(b, tx, z) == Tile::Water
                    || ha_shared::geography::natural_tile(b, tx, z) == Tile::WaterDeep
                {
                    libres = k;
                    break;
                }
            }
            *reparto.entry(libres).or_default() += 1;
            // **Los sitios donde el agua toca la roca, con nombre y apellido.**
            // Un contador dice cuántos hay; para saber POR QUÉ hace falta ir a
            // mirar la celda, y escribirlos a mano en la siguiente corrida es
            // exactamente el paso que se salta uno cuando el número es chico.
            if libres == 0 && pegadas.len() < 20 {
                pegadas.push((tx, tz, alto_de_pared(b, tx, tz)));
            }
        }
    }
    if !pegadas.is_empty() {
        println!("\nceldas con el agua pegada al pie (x, z, alto de la pared): {pegadas:?}");
    }
    println!("\ncaras que cuelgan al sur (fuera de garganta): {caras}");
    println!("filas de LLANO entre el pie dibujado y el líquido (-1 = no hay líquido en 12):");
    for (k, n) in &reparto {
        let etiqueta = match k {
            -1 => "sin agua cerca".to_string(),
            0 => "AGUA PEGADA AL PIE".to_string(),
            1 => "1 fila: la orilla toca la roca".to_string(),
            n => format!("{n} filas"),
        };
        println!("  {k:3}  {n:7}  {etiqueta}");
    }
}

/// **LA EXCEPCIÓN DE LA GARGANTA: ¿deja paredes con los pies en el agua?**
///
/// `relief::bajo_el_dibujo_de_una_pared` seca el río bajo el pie de cada muro
/// para que la roca no se dibuje sobre el agua, **salvo en la garganta de una
/// cascada**: ahí el cauce cruza el escalón y el agua tiene que pasar. Esa
/// exención es correcta para las tres celdas del chorro —el recorte del pack
/// trae su propia roca— pero se aplica a un radio, así que puede alcanzar a las
/// paredes que flanquean la garganta, que sí se dibujan.
///
/// Ésas son las que quedarían "flotando sobre el agua" que se pidió que no haya.
#[test]
#[ignore = "herramienta de medición, no un guardián"]
fn pared_junto_a_la_garganta() {
    let b = CelestialBody::Earth;
    let sitios = anclas(b, -900, 900, -900, 900);
    let mut mojadas = 0usize;
    let mut secas = 0usize;
    let mut sin_pared = 0usize;
    for &(tx, tz, _) in &sitios {
        for x in [tx - 1, tx + ANCHO_CASCADA] {
            if relief_cell(b, x, tz) != ReliefCell::Cliff {
                sin_pared += 1;
                continue;
            }
            // Las filas que cubre el dibujo de esa pared: cuerpo y pie.
            let pie = alto_de_pared(b, x, tz) + 1;
            let mojado = (1..=pie).any(|k| {
                matches!(
                    ha_shared::geography::natural_tile(b, x, tz + k),
                    Tile::Water | Tile::WaterDeep
                )
            });
            if mojado {
                mojadas += 1;
            } else {
                secas += 1;
            }
        }
    }
    println!("\nparedes que flanquean una garganta: {}", sitios.len() * 2);
    println!("  con el pie SOBRE EL AGUA: {mojadas}");
    println!("  con el pie en tierra:     {secas}");
    println!("  la celda no es pared:     {sin_pared}");
}

/// **TODA GARGANTA TIENE SUS DOS HOMBROS, Y LOS DOS DEL MISMO ALTO.**
///
/// Éste **sí** es un guardián, y es el que sostiene el módulo: una cascada es el
/// chorro más una pared a cada lado, siempre las dos, siempre empezando en la
/// fila de la boca y terminando en la del pie. Antes eso dependía de dónde
/// hubiera puesto el relieve su borde de terraza y salía cualquier cosa —de 22
/// hombros medidos, 9 no eran ni pared y 7 estaban desfasados una fila—, que es
/// la banda de acantilado suelta y más corta que el chorro de las capturas.
///
/// Si alguien mueve el ancla del salto, el ancho del recorte o la ventana al
/// ras, esto se cae antes de que se vea en pantalla.
#[test]
fn toda_garganta_tiene_sus_dos_hombros() {
    let b = CelestialBody::Earth;
    let sitios = anclas(b, -900, 900, -900, 900);
    assert!(!sitios.is_empty(), "el recorte no tiene una sola cascada que medir");
    for &(tx, tz, pisos) in &sitios {
        for x in [tx - 1, tx + ANCHO_CASCADA] {
            let hombro = hombro_de_garganta(b, x, tz);
            assert_eq!(
                hombro,
                Some((tz, pisos)),
                "el salto de ({tx},{tz}) de {pisos} piso(s) no tiene hombro en x={x}",
            );
        }
        // Y la roca del hombro termina donde el chorro deja de ser roca. El
        // hombro dibuja `pisos + 2` filas (labio + un cuerpo por piso + pie), o
        // sea su última fila es la `pisos + 1` contando desde el canto; el
        // chorro llega una más abajo, que es el reventón sobre el agua.
        assert_eq!(filas_de_hombro(pisos), pisos + 2, "el hombro no mide labio + {pisos} + pie");
        assert_eq!(
            (pisos + 1) + 1,
            filas_de_dibujo(pisos),
            "el pie del hombro y la poza del chorro no se encuentran ({pisos} piso(s))",
        );
    }

    // Y nadie MÁS es hombro: si `hombro_de_garganta` contestara de más, el
    // mundo se llenaría de paredes de tres celdas donde no hay ningún salto.
    let mut de_mas = 0usize;
    for tz in -200..200 {
        for tx in -200..200 {
            if hombro_de_garganta(b, tx, tz).is_some()
                && cascada_at(b, tx + 1, tz).is_none()
                && cascada_at(b, tx - ANCHO_CASCADA, tz).is_none()
            {
                de_mas += 1;
            }
        }
    }
    assert_eq!(de_mas, 0, "hay {de_mas} celdas que se creen hombro sin garganta al lado");
}

/// **TODA MESETA SE CIERRA CON UNA RAMPA.**
///
/// `every_terrace_edge_has_a_pass` sólo mira el RATIO global de rampas contra
/// caras: dice que el mundo no está sellado en promedio, no que cada meseta
/// tenga por dónde subir. Esto lo pregunta de verdad, y en dos pasadas:
///
/// 1. **Sin rampa propia**: la componente conexa de terreno alto cuyo borde
///    entero es pared, sin una sola celda de rampa ni de bordillo trepable.
/// 2. **INALCANZABLE**: la misma componente, pero después de un BFS de
///    caminabilidad desde la costa. Una meseta sin rampa propia todavía puede
///    tener quien la alcance desde la terraza de ARRIBA; una inalcanzable no
///    tiene por dónde, y ésa es la que rompe el mundo.
///
/// Caminar de una celda a otra vale si ninguna de las dos frena a pie, que es la
/// regla de `world2d::TileWorld::walk_blocked`: una cara de acantilado bloquea
/// salvo que sea bordillo trepable (`relief::climbable_edge`); la rampa y el
/// llano se pisan siempre.
#[test]
#[ignore = "herramienta de medición, no un guardián"]
fn mesetas_selladas() {
    let b = CelestialBody::Earth;
    const R: i64 = 400;
    let ancho = (R * 2) as usize;
    let idx = |x: i64, z: i64| ((z + R) as usize) * ancho + ((x + R) as usize);
    let mut nivel = vec![0i8; ancho * ancho];
    let mut pisable = vec![false; ancho * ancho];
    for z in -R..R {
        for x in -R..R {
            nivel[idx(x, z)] = relief_level(b, x, z);
            // La misma regla que `walk_blocked`: sólo la cara de acantilado
            // frena, y sólo si no se trepa.
            pisable[idx(x, z)] =
                relief_cell(b, x, z) != ReliefCell::Cliff || climbable_edge(b, x, z);
        }
    }

    // --- ¿QUÉ SE CAMINA DESDE DÓNDE? -----------------------------------------
    // Componentes conexas de lo PISABLE. La más grande es "el mundo": el suelo
    // continuo por el que se anda sin trepar nada. Todo lo que quede fuera está
    // aislado — no se llega ni de abajo ni de arriba.
    //
    // Arrancar de la costa (nivel 0) fue el primer intento y no medía nada: un
    // recorte tierra adentro no tiene una sola celda de nivel 0, así que la
    // frontera salía vacía y las 48 mesetas daban "inalcanzables".
    let mut comp = vec![usize::MAX; ancho * ancho];
    let mut tam_comp: Vec<usize> = vec![];
    for z0 in -R..R {
        for x0 in -R..R {
            let i0 = idx(x0, z0);
            if comp[i0] != usize::MAX || !pisable[i0] || nivel[i0] < 0 {
                continue;
            }
            let id = tam_comp.len();
            let mut n = 0usize;
            let mut cola = std::collections::VecDeque::from([(x0, z0)]);
            comp[i0] = id;
            while let Some((x, z)) = cola.pop_front() {
                n += 1;
                for (dx, dz) in [(1i64, 0i64), (-1, 0), (0, 1), (0, -1)] {
                    let (nx, nz) = (x + dx, z + dz);
                    if !(-R..R).contains(&nx) || !(-R..R).contains(&nz) {
                        continue;
                    }
                    let i = idx(nx, nz);
                    if comp[i] != usize::MAX || !pisable[i] || nivel[i] < 0 {
                        continue;
                    }
                    comp[i] = id;
                    cola.push_back((nx, nz));
                }
            }
            tam_comp.push(n);
        }
    }
    // El precio de abrir pasos: cuánta pared queda. Si esto se dispara, el
    // relieve deja de ser un obstáculo y el mundo se vuelve una alfombra.
    let (mut caras, mut rampas) = (0usize, 0usize);
    for z in -R..R {
        for x in -R..R {
            match relief_cell(b, x, z) {
                ReliefCell::Cliff | ReliefCell::Pared => caras += 1,
                ReliefCell::Ramp => rampas += 1,
                ReliefCell::Flat => {}
            }
        }
    }
    println!(
        "\nborde de terraza: {caras} caras de pared, {rampas} de rampa ({:.1}% pasos)",
        rampas as f32 * 100.0 / (caras + rampas).max(1) as f32
    );
    let mundo = tam_comp
        .iter()
        .enumerate()
        .max_by_key(|(_, n)| **n)
        .map(|(i, _)| i)
        .unwrap_or(usize::MAX);
    println!(
        "\nislas de suelo caminable: {} (la mayor, «el mundo», {} celdas de {})",
        tam_comp.len(),
        tam_comp.get(mundo).copied().unwrap_or(0),
        ancho * ancho
    );
    let alcanzado: Vec<bool> = comp.iter().map(|&c| c == mundo).collect();

    let mut visto = vec![false; ancho * ancho];
    let mut sin_rampa: Vec<(usize, i8, i64, i64)> = vec![];
    let mut inalcanzables: Vec<(usize, i8, i64, i64)> = vec![];
    let mut total = 0usize;
    let mut tamanos: Vec<usize> = vec![];

    for z0 in -R..R {
        for x0 in -R..R {
            let l = nivel[idx(x0, z0)];
            if l < 1 || visto[idx(x0, z0)] {
                continue;
            }
            // Flood fill de la meseta: celdas del MISMO nivel, 4-conexas.
            let mut pila = vec![(x0, z0)];
            visto[idx(x0, z0)] = true;
            let mut celdas = 0usize;
            let mut abierta = false;
            let mut llega = false;
            let mut borde_del_mapa = false;
            let mut perimetro = 0usize;
            while let Some((x, z)) = pila.pop() {
                celdas += 1;
                if alcanzado[idx(x, z)] {
                    llega = true;
                }
                let mut es_borde = false;
                for (dx, dz) in [(1i64, 0i64), (-1, 0), (0, 1), (0, -1)] {
                    let (nx, nz) = (x + dx, z + dz);
                    if !(-R..R).contains(&nx) || !(-R..R).contains(&nz) {
                        borde_del_mapa = true;
                        continue;
                    }
                    let nl = nivel[idx(nx, nz)];
                    if nl == l {
                        if !visto[idx(nx, nz)] {
                            visto[idx(nx, nz)] = true;
                            pila.push((nx, nz));
                        }
                    } else if nl < l {
                        es_borde = true;
                    }
                }
                if es_borde {
                    perimetro += 1;
                    if relief_cell(b, x, z) == ReliefCell::Ramp || climbable_edge(b, x, z) {
                        abierta = true;
                    }
                }
            }
            // Las que tocan el borde del recorte están cortadas: no se juzgan.
            if borde_del_mapa {
                continue;
            }
            total += 1;
            tamanos.push(celdas);
            if !abierta && perimetro > 0 {
                sin_rampa.push((celdas, l, x0, z0));
            }
            if !llega {
                inalcanzables.push((celdas, l, x0, z0));
            }
        }
    }
    tamanos.sort_unstable();
    sin_rampa.sort_unstable_by(|a, b| b.0.cmp(&a.0));
    inalcanzables.sort_unstable_by(|a, b| b.0.cmp(&a.0));
    println!("\nmesetas completas dentro de {}x{}: {total}", R * 2, R * 2);
    if let Some(&max) = tamanos.last() {
        println!(
            "  tamaño: mediana {} celdas, máx {max}",
            tamanos[tamanos.len() / 2]
        );
    }
    println!("  SIN RAMPA PROPIA (todo su borde es pared): {}", sin_rampa.len());
    let grandes = sin_rampa.iter().filter(|s| s.0 >= 20).count();
    println!("    de esas, de 20 celdas o más: {grandes}");
    println!(
        "  INALCANZABLES a pie desde la costa: {}",
        inalcanzables.len()
    );
    for (celdas, l, x, z) in inalcanzables.iter().take(12) {
        println!("    {celdas:6} celdas · nivel {l} · ({x},{z})  INALCANZABLE");
    }
    for (celdas, l, x, z) in sin_rampa.iter().take(12) {
        println!("    {celdas:6} celdas · nivel {l} · ({x},{z})  sin rampa propia");
    }
}
