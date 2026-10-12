//! dron — red binaria IAFZ-64 por encima de un PID, entrenada por evolución.
//!
//! La regla del proyecto, y la razón de que exista separado del juego:
//! **la red nunca toca los motores**. Emite una consigna de cabeceo; el PID la
//! convierte en empuje y torque y garantiza que el aparato se mantenga en el
//! aire aunque el genoma sea basura. Eso es lo que permite evolucionar sin
//! romper hélices, y lo que hará viable subir esto a hardware real.
//!
//!   cargo run --release -- entrenar [generaciones]
//!   cargo run --release -- volar [escenario]
//!   cargo run --release -- pid            (vuela sin red: la línea de base)
//!   cargo run --release -- duelo          (vos contra el genoma, en vivo)

mod abordo;
mod capacidades;
mod codigo;
mod crtp;
mod gym3d;
mod iafz;
mod mision;
mod movil;
mod persecucion;
mod plan;
mod puente;
mod dbpower;
mod pid;
mod pid3d;
mod progreso;
mod sim3d;
mod repertorio;
mod vivo;
mod web;


const GENOMA3D: &str = "genomes/DRON3D_64.adn";

/// El puente a un controlador de vuelo real. Ver `puente.rs` para por qué MSP
/// y por qué `MSP_SET_RAW_RC`.
fn puente_cli(args: &[String]) -> i32 {
    use std::io::Write;
    let seco = args.iter().any(|a| a == "--seco");
    let destino = args.iter().skip(1).find(|a| a.starts_with("/dev/"));
    if !seco && destino.is_none() {
        eprintln!("uso:  dron puente --seco            (muestra los bytes, sin hardware)");
        eprintln!("      dron puente /dev/ttyUSB0      (manda al controlador de vuelo)");
        eprintln!();
        eprintln!("El puerto se abre como archivo: hay que configurarlo antes con");
        eprintln!("  stty -F /dev/ttyUSB0 115200 raw -echo");
        return 2;
    }

    let mut seg = puente::Seguridad::nuevo();
    // Sin `--armar` no se arma: el software nunca decide eso solo.
    seg.armar(args.iter().any(|a| a == "--armar"));

    let mut salida: Option<std::fs::File> = match destino {
        Some(d) => match std::fs::OpenOptions::new().write(true).open(d) {
            Ok(f) => Some(f),
            Err(e) => {
                eprintln!("no pude abrir {d}: {e}");
                return 1;
            }
        },
        None => None,
    };

    println!("PUENTE MSP · {} · {} Hz · armado: {}",
             destino.map(String::as_str).unwrap_or("seco (sin hardware)"),
             1000 / puente::PERIODO_MS, seg.armado());
    println!("(Ctrl-C para cortar)\n");

    // Consignas de prueba: quietas. Este comando existe para verificar el
    // CABLEADO y los signos contra un aparato atado, no para volar — por eso
    // manda neutro y no hay forma de que mande otra cosa sin tocar el código.
    let c = gym3d::Consignas::default();
    for i in 0..(if seco { 5 } else { u64::MAX }) {
        seg.hay_datos();
        let ch = seg.ciclo(&c, 0.0);
        let trama = puente::trama_rc(&ch);
        match salida.as_mut() {
            Some(f) => {
                if f.write_all(&trama).is_err() {
                    eprintln!("se cortó el puerto serie");
                    return 1;
                }
                let _ = f.flush();
            }
            None => {
                println!("  #{i}  canales {:?}", &ch[..5]);
                println!("      {} bytes: {}", trama.len(),
                         trama.iter().map(|b| format!("{b:02X}"))
                              .collect::<Vec<_>>().join(" "));
            }
        }
        std::thread::sleep(std::time::Duration::from_millis(puente::PERIODO_MS));
    }
    0
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    // `fisicanominal`: el aparato de todas las misiones es el nominal, como
    // antes de la bitácora 7.59. Para comparar contra lo medido antes.
    if args.iter().any(|a| a == "fisicanominal") {
        mision::FISICA_VARIABLE.store(false, std::sync::atomic::Ordering::Relaxed);
    }
    if std::env::var_os("SIN_PLAN").is_some() {
        mision::PLAN_ACTIVO.store(false, std::sync::atomic::Ordering::Relaxed);
    }
    if std::env::var_os("SIN_DIVERGENCIA").is_some() {
        mision::BAJADA_DIVERGENCIA.store(false, std::sync::atomic::Ordering::Relaxed);
    }
    if std::env::var_os("SIN_SLAM").is_some() {
        mision::LOCALIZAR.store(false, std::sync::atomic::Ordering::Relaxed);
    }
    let cmd = args.first().map(String::as_str).unwrap_or("ayuda");

    match cmd {
        // Entrena MISIONES completas: despegar, ir, posarse, volver, posarse.
        // Es el gym que reemplaza al del objetivo suelto — ver `mision.rs`
        // para por qué encadenar puntos es lo que mata el orbitar.
        "entrenarmision" => {
            let gens = args.get(1).and_then(|s| s.parse().ok()).unwrap_or(200);
            let robusto = !args.iter().any(|a| a == "rapido");
            // "saltar" permite avanzar de peldaño sin aprobarlo, tras
            // estancarse. Apagado por defecto a propósito (regla 3.7): es una
            // decisión de una persona, no del entrenador.
            let saltar = args.iter().any(|a| a == "saltar");
            let ver_cada = if args.iter().any(|a| a == "ver") { 5 } else { 0 };
            let semilla = args.iter()
                .find_map(|a| a.strip_prefix("semilla="))
                .and_then(|s| s.parse().ok())
                .unwrap_or(2026u32);
            let salida = args.iter()
                .find_map(|a| a.strip_prefix("salida="))
                .unwrap_or(GENOMA3D)
                .to_string();
            // "sorteos=N": cuántos juegos de escenarios independientes ve cada
            // individuo por generación. 1 es el histórico; más de 1 le saca
            // ruido a la SELECCIÓN (la maldición del ganador, ver
            // `mision::SORTEOS`) al costo de N veces el cómputo. Es un
            // argumento y no una constante porque la única forma honesta de
            // decidirlo es un A/B (regla 3.20 + sección 4).
            let sorteos: u32 = args.iter()
                .find_map(|a| a.strip_prefix("sorteos="))
                .and_then(|s| s.parse().ok())
                .unwrap_or(1);
            mision::fijar_sorteos(sorteos);
            if sorteos > 1 {
                println!("  · {sorteos} sorteos de escenarios por individuo y generación \
                          (cuesta x{sorteos})");
            }
            // "peldano=N" fija el currículo en un solo escalón (para depurar
            // uno sin correr toda la escalera de nuevo) y "sembrar=RUTA"
            // arranca la población desde un genoma que ya domina los
            // anteriores — mismo patrón que ya usa "entrenarexperto".
            let peldano_fijo = args.iter()
                .find_map(|a| a.strip_prefix("peldano="))
                .and_then(|s| s.parse().ok());
            let sembrar_desde = args.iter()
                .find_map(|a| a.strip_prefix("sembrar="))
                .map(String::from);
            // **"repertorio"**: en vez de un genoma para los quince peldaños,
            // nueve —uno por celda— y cada peldaño entrena la suya. Necesita
            // `sembrar=RUTA` (ver `gym3d::Config::repertorio`).
            let repertorio = args.iter().any(|a| a == "repertorio");
            // "desde=N": arrancar la escalera en el peldaño N (1-based) en vez
            // del primero. Para atacar el peldaño donde una corrida se trabó
            // sin remoler los de abajo. Exige `sembrar=`.
            let peldano_inicial = args.iter()
                .find_map(|a| a.strip_prefix("desde="))
                .and_then(|s| s.parse::<usize>().ok())
                .map(|n| n.max(1) - 1)
                .unwrap_or(0);
            println!("MISIONES · {gens} generaciones · sensores: {}",
                     if robusto { "TODO el rango (aleatorizacion de dominio)" }
                     else { "ruido real solamente (modo rapido)" });
            println!("escalera de {} peldaños: frenar → rumbo → ir a B (40/70/85/100%) →", mision::PELDANOS);
            println!("  esquivar → despegar → posarse → ida y vuelta → ruta larga → entrega (suave → DURO)\n");
            let t = std::time::Instant::now();
            let mejor = gym3d::entrenar(&gym3d::Config {
                generaciones: gens, semilla, salida,
                ruido: sim3d::Ruido::REAL, robusto, ver_cada,
                tarea: gym3d::Tarea::Mision, saltar, peldano_fijo,
                sembrar_desde, repertorio, peldano_inicial,
                // El modo repertorio siembra la población entera por
                // construcción; para que un A/B contra el monolítico compare
                // UNA sola variable, el otro brazo tiene que hacer lo mismo.
                sembrar_todo: repertorio || args.iter().any(|a| a == "sembrartodo"),
                modulo_vertical: args.iter().any(|a| a == "vertical"),
                modulo_posar: args.iter().any(|a| a == "posar"),
            });
            println!("mejor aptitud: {mejor:.1}   ({:.1?})", t.elapsed());
        }
        "entrenarpersecucion" => {
            let gens = args.get(1).and_then(|s| s.parse().ok()).unwrap_or(200);
            let robusto = !args.iter().any(|a| a == "rapido");
            let saltar = args.iter().any(|a| a == "saltar");
            let ver_cada = if args.iter().any(|a| a == "ver") { 5 } else { 0 };
            let semilla = args.iter()
                .find_map(|a| a.strip_prefix("semilla="))
                .and_then(|s| s.parse().ok())
                .unwrap_or(2026u32);
            // Sin default a GENOMA3D a propósito (regla 3.11): perseguir usa
            // sensores distintos de mision/punto (`Guia` con velocidad del
            // objetivo) — pisar el genoma principal con esto lo dejaría
            // ilegible para las otras tareas y viceversa. Un genoma de
            // persecución es un archivo aparte hasta que se decida lo
            // contrario.
            let salida = args.iter()
                .find_map(|a| a.strip_prefix("salida="))
                .unwrap_or("genomes/DRON3D_PERSECUCION.adn")
                .to_string();
            println!("PERSECUCIÓN · {gens} generaciones · sensores: {}",
                     if robusto { "TODO el rango (aleatorizacion de dominio)" }
                     else { "ruido real solamente (modo rapido)" });
            // La escalera se lee del disco: los 6 sintéticos más una grabación
            // por cada vez que alguien manejó en la cabina y la guardó.
            let escalera = persecucion::recorridos_del_curriculo();
            println!("escalera de {} peldaños, uno por recorrido:", escalera.len());
            for (i, (nombre, _)) in escalera.iter().enumerate() {
                println!("  {}. {nombre}", i + 1);
            }
            println!();
            let t = std::time::Instant::now();
            let mejor = gym3d::entrenar(&gym3d::Config {
                generaciones: gens, semilla, salida,
                ruido: sim3d::Ruido::REAL, robusto, ver_cada,
                tarea: gym3d::Tarea::Persecucion, saltar, peldano_fijo: None,
                // `sembrar=RUTA sembrartodo` con un genoma de salidas en cero
                // (`dron residuocero`) arranca la población EN el clásico, como
                // el aprendizaje de política residual (Silver et al. 2018,
                // Johannink et al. 2019: el residuo nace en cero). Sin eso,
                // medido el 2026-09-26: 200 generaciones clavado en 12226.0,
                // 150 puntos DEBAJO del clásico (12375.4) — la población al
                // azar nunca encontró el «no corregir nada» que ya valía más.
                sembrar_desde: args.iter().find_map(|a| a.strip_prefix("sembrar=")).map(String::from),
                repertorio: false, peldano_inicial: 0,
                sembrar_todo: args.iter().any(|a| a == "sembrartodo"),
                modulo_vertical: false,
                modulo_posar: false,
            });
            println!("mejor aptitud: {mejor:.1}   ({:.1?})", t.elapsed());
        }
        // Vuela UNA misión y la dibuja, con el piloto que se le pida. Es la
        // herramienta de diagnóstico: el proyecto se rige por mirar la traza,
        // no por razonar sobre el código.
        //   dron mision [peldaño 1-16] [cual] [clasico]
        //
        // **El peldaño es 1-BASED, igual que en `sorteos` y en `celdas`.**
        // Era 0-based sólo acá, y eso es una trampa de instrumento que ya
        // costó un diagnóstico equivocado: `mision 13 0 detalle` imprimía
        // «PELDAÑO 14» y se leyó como el 13, así que se diagnosticó la
        // entrega creyendo estar mirando ida-y-vuelta (regla 3.18 — el mismo
        // número significando dos cosas en dos comandos es un instrumento
        // mentiroso). El cartel que imprime el comando siempre dijo la verdad;
        // lo que estaba mal era que hubiera que acordarse de restar uno.
        // `dron estres [peldaño] [n=24]`: el clásico (la vara) bajo cada
        // avería del simulador, contra el mismo peldaño sano. Contesta «¿qué
        // perturbación real rompe al piloto y no la estamos midiendo?».
        "estres" => {
            use sim3d::Averia;
            let peldano = args.get(1).and_then(|s| s.parse::<usize>().ok()).unwrap_or(7).max(1) - 1;
            let n: u32 = args.iter().find_map(|a| a.strip_prefix("n=")).and_then(|s| s.parse().ok()).unwrap_or(24);
            let umbral = mision::umbral_aprobado(peldano);
            let casos: Vec<(&str, Averia)> = vec![
                ("sano", Averia::NINGUNA),
                ("carga corrida 0.05 N·m (roll)", Averia { torque_parasito: (0.05, 0.0, 0.0), ..Averia::NINGUNA }),
                ("motor flojo: guiñada 0.03 N·m", Averia { torque_parasito: (0.0, 0.0, 0.03), ..Averia::NINGUNA }),
                ("15% menos empuje", Averia { perdida_empuje: 0.15, ..Averia::NINGUNA }),
                ("40% menos empuje", Averia { perdida_empuje: 0.40, ..Averia::NINGUNA }),
                ("caja de 300 g", Averia { masa_extra: 0.30, ..Averia::NINGUNA }),
                ("caja de 150 g", Averia { masa_extra: 0.15, ..Averia::NINGUNA }),
                ("viento x3", Averia { viento_mult: 3.0, ..Averia::NINGUNA }),
            ];
            println!("ESTRÉS — peldaño {} «{}», clásico, {n} sorteos, ruido REAL, umbral {umbral:.0}\n",
                     peldano + 1, mision::misiones(peldano)[0].nombre);
            for (nombre, av) in casos {
                *mision::ESTRES.write().unwrap() = Some(av);
                mision::MEDIR_MEZCLA.store(nombre == "sano", std::sync::atomic::Ordering::Relaxed);
                let mut ok = 0;
                let mut notas = Vec::new();
                for i in 0..n {
                    mision::fijar_variante(500_001 + i);
                    let a = mision::aptitud(mision::Piloto::Clasico, peldano, false, sim3d::Ruido::REAL);
                    if a >= umbral { ok += 1; }
                    notas.push(a);
                }
                notas.sort_by(|a, b| a.partial_cmp(b).unwrap());
                println!("  {:<32} aprueba {:>2}/{n}   mediana {:>8.1}", nombre, ok, notas[notas.len() / 2]);
            }
            use std::sync::atomic::Ordering::Relaxed;
            let (t, f) = (mision::PASOS_TOTALES.load(Relaxed), mision::PASOS_FUERA.load(Relaxed));
            println!("\n  pasos del vuelo SANO cuyo pedido no cabe en 4 motores: {f} de {t} ({:.1}%)",
                     100.0 * f as f64 / t.max(1) as f64);
            *mision::ESTRES.write().unwrap() = None;
        }
        // `dron tarea "despegar 3; ir 6 0; posar; agarrar 0.3; ..." [clasico]
        // [genoma=RUTA] [ruido=limpio|real|duro] [postes=x,y,r;x,y,r]`: vuela
        // una tarea DICHA, sin reentrenar (`mision::tarea_desde_texto`).
        "tarea" => {
            let texto = args.get(1).cloned().unwrap_or_default();
            let postes: Vec<sim3d::Obstaculo> = args.iter()
                .find_map(|a| a.strip_prefix("postes="))
                .map(|p| p.split(';').filter_map(|t| {
                    let v: Vec<f32> = t.split(',').filter_map(|x| x.parse().ok()).collect();
                    (v.len() >= 2).then(|| sim3d::Obstaculo {
                        x: v[0], y: v[1], radio: *v.get(2).unwrap_or(&0.6), alto: 6.0 })
                }).collect())
                .unwrap_or_default();
            let m = match mision::mision_de_tarea(&texto, (0.0, 0.0), postes) {
                Ok(m) => m,
                Err(e) => { eprintln!("{e}"); std::process::exit(1); }
            };
            let genoma = args.iter().find_map(|a| a.strip_prefix("genoma=")).unwrap_or(GENOMA3D);
            let piloto = if args.iter().any(|a| a == "clasico") {
                mision::Piloto::Clasico
            } else {
                if !iafz::cargar(110, genoma) { eprintln!("no encontré {genoma}"); std::process::exit(1); }
                mision::Piloto::Red(110)
            };
            let ruido = match args.iter().find_map(|a| a.strip_prefix("ruido=")) {
                Some("limpio") => sim3d::Ruido::LIMPIO,
                Some("duro") => sim3d::Ruido::DURO,
                _ => sim3d::Ruido::REAL,
            };
            mision::informe_de_vuelo(piloto, &m, ruido);
        }
        // `dron litewing [--seco] [--armar] [--signos] [--nuevo] [IP:PUERTO]`:
        // el puente CRTP por WiFi (`crtp.rs`). En seco muestra los bytes; con
        // `--signos` hace una secuencia lenta de alabeo, cabeceo y guiñada
        // para verificar los signos CON LAS HÉLICES SACADAS; sin `--armar`
        // manda «parar». Primer vuelo: `--armar` sostiene 0.4 m en el lugar
        // (la controladora con su sensor), para ver si se sostiene.
        "litewing" => {
            let seco = args.iter().any(|a| a == "--seco");
            let signos = args.iter().any(|a| a == "--signos");
            let destino = args.iter().skip(1).find(|a| a.contains(':')).map(String::as_str)
                .unwrap_or(crtp::DIRECCION);
            let mut enlace = crtp::Enlace::nuevo(!args.iter().any(|a| a == "--nuevo"));
            enlace.seguridad.armar(args.iter().any(|a| a == "--armar"));
            let sock = if seco { None } else {
                match std::net::UdpSocket::bind("0.0.0.0:0") {
                    Ok(s) => Some(s),
                    Err(e) => { eprintln!("no pude abrir un socket UDP: {e}"); std::process::exit(1); }
                }
            };
            println!("PUENTE CRTP · {} · {} Hz · armado: {} · protocolo {}",
                     if seco { "seco (sin hardware)" } else { destino },
                     1000 / puente::PERIODO_MS, enlace.seguridad.armado(),
                     if enlace.protocolo_viejo { "viejo (ESP-Drone)" } else { "nuevo" });
            let ciclos = if seco { 6 } else if signos { 12 * 1000 / puente::PERIODO_MS } else { u64::MAX };
            for i in 0..ciclos {
                let t = i * puente::PERIODO_MS / 1000; // segundos
                let c = if signos {
                    // 2 s por paso: alabeo +10°, nada, cabeceo +10°, nada,
                    // guiñada +30°/s, nada. Mirar hacia dónde quiere ir.
                    let (r, p, y) = match t / 2 {
                        0 => (0.17, 0.0, 0.0), 2 => (0.0, 0.17, 0.0), 4 => (0.0, 0.0, 0.5),
                        _ => (0.0, 0.0, 0.0),
                    };
                    gym3d::Consignas { roll: r, pitch: p, yaw: y, alt: 0.0 }
                } else {
                    gym3d::Consignas { roll: 0.0, pitch: 0.0, yaw: 0.0, alt: 0.4 }
                };
                enlace.seguridad.hay_datos();
                let d = enlace.ciclo(&c);
                match &sock {
                    Some(s) => { let _ = s.send_to(&d, destino); }
                    None => println!("  #{i}  {} bytes: {}", d.len(),
                                     d.iter().map(|b| format!("{b:02X}")).collect::<Vec<_>>().join(" ")),
                }
                std::thread::sleep(std::time::Duration::from_millis(puente::PERIODO_MS));
            }
        }
        "mision" => {
            let peldano = args.get(1).and_then(|s| s.parse::<usize>().ok())
                              .unwrap_or(4).max(1) - 1;
            let cual = args.get(2).and_then(|s| s.parse().ok()).unwrap_or(0usize);
            let clasico = args.iter().any(|a| a == "clasico");
            let genoma = args.iter()
                .find_map(|a| a.strip_prefix("genoma="))
                .unwrap_or(GENOMA3D);
            // `rep=RUTA`: volar el REPERTORIO entero, igual que `sorteos`.
            // Sin esto no se podía mirar CÓMO falla un repertorio, sólo
            // cuántas veces (el «olvida posarse» de la bitácora 7.55).
            let mut _banco = None;
            let piloto = if clasico {
                mision::Piloto::Clasico
            } else if let Some(rep) = args.iter().find_map(|a| a.strip_prefix("rep="))
                .and_then(repertorio::leer_con_vertical) {
                repertorio::fijar_banco(&rep);
                _banco = Some(rep);
                iafz::init(110, 1);
                mision::Piloto::PorCelda { slot: 110, entrena: repertorio::CELDAS }
            } else {
                if !iafz::cargar(110, genoma) {
                    eprintln!("no encontré {genoma}");
                    std::process::exit(1);
                }
                mision::Piloto::Red(110)
            };
            // `ruido=` para la traza. Sin esto `informe_de_vuelo` sólo podía
            // mirar con ruido REAL, y hay fallas que SÓLO pasan con sensores
            // limpios (misión 3 del peldaño 12: 106.6 en limpio contra 1135.1
            // en real). Diagnosticar eso sin poder elegir el ruido es imposible.
            let ruido = match args.iter().find_map(|a| a.strip_prefix("ruido=")) {
                Some("limpio") => sim3d::Ruido::LIMPIO,
                Some("duro") => sim3d::Ruido::DURO,
                _ => sim3d::Ruido::REAL,
            };
            // `variante=N`: pinchar el sorteo N de `dron sorteos` (que usa
            // 500_001 + i) para mirar por qué falla ESE sorteo.
            if let Some(v) = args.iter().find_map(|a| a.strip_prefix("variante="))
                .and_then(|s| s.parse::<u32>().ok()) {
                mision::fijar_variante(v);
            }
            // `masa_extra=KG`: volar el informe con una carga encima.
            if let Some(kg) = args.iter().find_map(|a| a.strip_prefix("masa_extra="))
                .and_then(|s| s.parse::<f32>().ok()) {
                *mision::ESTRES.write().unwrap() =
                    Some(sim3d::Averia { masa_extra: kg, ..sim3d::Averia::NINGUNA });
            }
            if args.iter().any(|a| a == "salidas") {
                mision::salidas(110, peldano);
            } else if args.iter().any(|a| a == "detalle") {
                mision::detalle(piloto, peldano);
            } else {
                let ms = mision::misiones(peldano);
                let m = &ms[cual % ms.len()];
                mision::informe_de_vuelo(piloto, m, ruido);
            }
        }
        // El puente a hardware: convierte las consignas del cerebro en canales
        // de radio MSP y los escribe a un puerto serie (o a stdout con
        // `--seco`, para mirar los bytes sin tener la placa conectada).
        //
        //   dron puente --seco               ver los paquetes, sin hardware
        //   dron puente /dev/ttyUSB0         mandarlos de verdad
        "puente" => std::process::exit(puente_cli(&args)),
        // El informe de capacidades. Acepta `genoma=RUTA` igual que
        // `capacidadestraza`: antes estaba cableado a GENOMA3D y un `genoma=`
        // pasado por línea de comandos se IGNORABA EN SILENCIO — o sea que
        // medir un genoma recién entrenado devolvía, sin avisar, los números
        // del genoma instalado. Se cazó porque el encabezado del informe
        // imprime qué archivo cargó (regla 3.12), que es exactamente para lo
        // que está. Un argumento que se ignora sin chistar es un instrumento
        // roto (regla 3.18).
        "capacidades" => {
            let genoma = args.iter()
                .find_map(|a| a.strip_prefix("genoma="))
                .unwrap_or(GENOMA3D);
            capacidades::informe(genoma);
        }
        // Refina UN nicho del repertorio fuera de línea, con el mismo lazo
        // anclado de `vivo.rs` pero a velocidad de gym (regla 3.19: la pieza
        // ya existía y sólo estaba conectada al vuelo, no al entrenamiento).
        //
        //   dron refinar [gens] [genoma=RUTA] [rep=RUTA] [salida=RUTA] [semilla=N]
        //
        // `salida=` explícito por la regla 3.11: por defecto NO pisa el
        // repertorio instalado — escribe al lado y lo dice.
        "refinar" => {
            let gens: u32 = args.get(1).and_then(|s| s.parse().ok()).unwrap_or(400);
            let genoma = args.iter()
                .find_map(|a| a.strip_prefix("genoma="))
                .unwrap_or(GENOMA3D);
            let rep_ruta = args.iter()
                .find_map(|a| a.strip_prefix("rep="))
                .unwrap_or(repertorio::Repertorio::ARCHIVO);
            let salida = args.iter()
                .find_map(|a| a.strip_prefix("salida="))
                .unwrap_or("genomes/DRON3D_REPERTORIO_REFINADO.bin");
            let semilla: u32 = args.iter()
                .find_map(|a| a.strip_prefix("semilla="))
                .and_then(|s| s.parse().ok())
                .unwrap_or(1);

            const CARGA: i32 = 121;
            if let Err(e) = iafz::cargar_compatible(CARGA, genoma) {
                println!("no pude cargar {genoma}: {e}");
                return;
            }
            // Si hay repertorio en disco se refina ÉSE; si no, se siembra del
            // genoma, que por construcción vuela idéntico a él (ver
            // `Repertorio::sembrar`).
            let (mut rep, de_donde) = match repertorio::Repertorio::leer(rep_ruta) {
                Some(r) => (r, rep_ruta.to_string()),
                None => (repertorio::Repertorio::sembrar(CARGA), format!("sembrado de {genoma}")),
            };

            // El nicho que se refina: crucero (lejos del objetivo) con el
            // frente TAPADO — que es exactamente la situación de la misión
            // sintética de esquive aislado, y la que falla en vivo.
            let ctx = repertorio::Contexto { estorbo: 2, tarea: 0 };
            println!("REFINAR — nicho «{}», {gens} generaciones, semilla {semilla}", ctx.nombre());
            println!("repertorio: {de_donde}");

            // La vara honesta sobre el MISMO lote held-out, antes de nada
            // (regla 3.18: ningún umbral se elige a gusto; apagar la red da
            // el clásico gratis, regla 3.15).
            let vara = repertorio::merito_clasico_esquive(semilla);

            let t = std::time::Instant::now();
            let (mejoro, antes, despues) = repertorio::refinar_nicho(&mut rep, ctx, gens, semilla);
            println!("\n  vara honesta (clásico) : {vara:8.1}");
            println!("  elite anterior         : {antes:8.1}");
            println!("  tras refinar           : {despues:8.1}   ({:+.1})", despues - antes);
            println!("  ¿entra al repertorio?  : {}", if mejoro { "SÍ" } else { "no — se descarta" });
            println!("  ¿le gana al clásico?   : {}", if despues > vara { "sí" } else { "NO (la red estorba en este nicho)" });

            // Y el mismo lote contado en castellano, que es lo que de verdad
            // se quería arreglar: ¿sigue chocando el poste? (regla 3.5: la
            // aptitud es un promedio y esconde el modo de falla).
            // Tras `refinar_nicho`, ANCLA tiene el refinado y ASPIRANTE el
            // elite viejo — ya cargados, no hay que volver a leerlos.
            let n = repertorio::HELDOUT;
            let (lc, rc) = repertorio::conteo_heldout(semilla, mision::Piloto::Clasico);
            let (la, ra) = repertorio::conteo_heldout(semilla, mision::Piloto::Red(repertorio::REFINO_ASPIRANTE));
            let (ld, rd) = repertorio::conteo_heldout(semilla, mision::Piloto::Red(repertorio::REFINO_ANCLA));
            println!("\n  de {n} postes nuevos      llega limpio   roza el poste");
            println!("  clásico                    {lc:2}/{n}          {rc:2}");
            println!("  elite anterior             {la:2}/{n}          {ra:2}");
            println!("  tras refinar               {ld:2}/{n}          {rd:2}");
            println!("  ({:.1?})", t.elapsed());

            if mejoro {
                match rep.escribir(salida) {
                    Ok(()) => println!("\nescrito en {salida}"),
                    Err(e) => println!("\nno pude escribir {salida}: {e}"),
                }
            }
        }
        // **¿Cuánta de la nota de un genoma es el genoma y cuánta el sorteo?**
        //
        //   dron sorteos [peldaño 1-15] [genoma=RUTA|rep=RUTA] [n=N]
        //
        // Evalúa el MISMO genoma en el MISMO peldaño N veces, cambiando sólo
        // el sorteo de escenarios, e imprime la dispersión y cuántas veces
        // habría aprobado. Es el instrumento que le faltaba a la regla 3.20:
        // sin esto, "cruzó el umbral" y "sabe hacer la maniobra" se ven
        // idénticos desde afuera — que es exactamente cómo el proyecto se
        // comió un "15/15 en la generación 100" que no existía (bitácora
        // 7.30). Un genoma que aprueba 1 de 12 sorteos no aprendió nada: ganó
        // la lotería.
        //
        // **`rep=RUTA` mide un REPERTORIO, no un genoma suelto**, y hace falta:
        // desde que el gimnasio entrena por celdas (bitácora 7.33), el brazo
        // del repertorio aprueba un peldaño volando el banco entero —su celda
        // más las ocho congeladas— y no el genoma de la celda solo. Preguntarle
        // a la celda suelta "¿en cuántos sorteos aprobás?" contesta sobre un
        // piloto que nunca voló, y ése es justo el error que la regla 3.18
        // prohíbe: el instrumento tiene que medir al piloto que se entrenó.
        "sorteos" => {
            let peldano = args.get(1).and_then(|s| s.parse::<usize>().ok()).unwrap_or(1).max(1) - 1;
            let genoma = args.iter()
                .find_map(|a| a.strip_prefix("genoma="))
                .unwrap_or(GENOMA3D);
            let rep_ruta = args.iter().find_map(|a| a.strip_prefix("rep="));
            let n: u32 = args.iter()
                .find_map(|a| a.strip_prefix("n="))
                .and_then(|s| s.parse().ok())
                .unwrap_or(12);
            const SORTEO: i32 = 119;
            if let Err(e) = iafz::cargar_compatible(SORTEO, genoma) {
                println!("no pude cargar {genoma}: {e}");
                return;
            }
            // Con `rep=` el piloto es el banco entero: `entrena: CELDAS` quiere
            // decir "ninguna celda es el individuo", así que las nueve salen
            // del repertorio y el slot cargado arriba no se usa para volar.
            // Y `clasico` mide la VARA en la misma escala y con los mismos
            // sorteos. Hace falta para no confundir "el genoma no lo sabe
            // hacer" con "este peldaño no lo aprueba nadie": si la vara honesta
            // también aprueba 2 de 12, el número del genoma no dice nada sobre
            // el genoma (regla 3.18 — ningún umbral se lee sin calibrarlo
            // contra la vara, y apagar la red la da gratis, regla 3.15).
            let mut _banco = None;
            let piloto = if args.iter().any(|a| a == "clasico") {
                println!("(midiendo la VARA HONESTA — guiado clásico, red apagada)");
                mision::Piloto::Clasico
            } else { match rep_ruta {
                Some(r) => match repertorio::leer_con_vertical(r) {
                    Some(rep) => {
                        repertorio::fijar_banco(&rep);
                        _banco = Some(rep);
                        println!("(midiendo el REPERTORIO {r}, no un genoma suelto)");
                        mision::Piloto::PorCelda { slot: SORTEO, entrena: repertorio::CELDAS }
                    }
                    None => {
                        println!("no pude leer el repertorio {r}");
                        return;
                    }
                },
                None => mision::Piloto::Red(SORTEO),
            } };
            let umbral = mision::umbral_aprobado(peldano);
            let mut notas: Vec<f32> = Vec::with_capacity(n as usize);
            for i in 0..n {
                mision::fijar_variante(500_001 + i);
                notas.push(mision::aptitud(piloto, peldano, true, sim3d::Ruido::REAL));
            }
            // Y el desglose que evita el malentendido caro: la aptitud es el
            // mínimo entre las misiones del peldaño Y el examen de repaso, así
            // que un genoma puede tener las propias impecables y la nota en
            // rojo porque olvidó algo de atrás (regla 3.8).
            // Y los dos términos POR SEPARADO en cada sorteo: sin esto no se
            // sabe cuál de los dos aporta el ruido, que es lo que decide
            // dónde hay que gastar evaluaciones (regla 3.5: diagnosticar
            // antes de tocar).
            let mut props: Vec<f32> = Vec::with_capacity(n as usize);
            let mut reps: Vec<f32> = Vec::with_capacity(n as usize);
            for i in 0..n {
                mision::fijar_variante(500_001 + i);
                // Los ruidos que EXIGE este peldaño (`mision::ruidos`), no
                // siempre los tres: hasta el 2026-09-26 esto pedía también DURO
                // en los peldaños 12 y 14, que el entrenador aprueba con limpio
                // y real — el instrumento medía más estricto que la vara que
                // dice medir (regla 3.18, bitácora 7.43).
                props.push(
                    mision::ruidos(peldano).iter()
                        .map(|r| mision::aptitud(piloto, peldano, false, *r))
                        .fold(f32::INFINITY, f32::min),
                );
                reps.push(mision::peor_de_repaso(piloto, peldano));
            }
            let rango = |v: &[f32]| {
                let mut o = v.to_vec();
                o.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
                (o[0], o[o.len() / 2], o[o.len() - 1])
            };
            let (p_min, p_med, p_max) = rango(&props);
            let (r_min, r_med, r_max) = rango(&reps);
            let propias = p_min;
            let repaso = r_min;
            let pasan = notas.iter().filter(|&&v| v >= umbral).count();
            let mut ord = notas.clone();
            ord.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
            println!("SORTEOS — peldaño {}, {n} juegos de escenarios distintos", peldano + 1);
            println!("piloto: {}\numbral de aprobación: {umbral:.1}\n",
                     if matches!(piloto, mision::Piloto::Clasico) { "guiado clásico (la vara)" }
                     else { rep_ruta.unwrap_or(genoma) });
            println!("  peor    {:>9.1}", ord[0]);
            println!("  mediana {:>9.1}", ord[ord.len() / 2]);
            println!("  mejor   {:>9.1}", ord[ord.len() - 1]);
            println!("\n  término              peor   mediana     mejor   dispersión");
            println!("  misiones propias {p_min:>9.1} {p_med:>9.1} {p_max:>9.1}   {:>9.1}",
                     p_max - p_min);
            if repaso.is_finite() {
                println!("  examen de repaso {r_min:>9.1} {r_med:>9.1} {r_max:>9.1}   {:>9.1}{}",
                         r_max - r_min,
                         if repaso < propias { "   ← ata la nota" } else { "" });
            }
            // Y cuál peldaño viejo es el olvidado. Si siempre es el mismo, es
            // un conflicto concreto; si rota, es olvido general.
            if repaso.is_finite() {
                mision::fijar_variante(500_001);
                let mut por_p = mision::repaso_por_peldano(piloto, peldano);
                por_p.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal));
                println!("\n  peldaños de atrás peor recordados:");
                for (p, v) in por_p.iter().take(4) {
                    println!("    peldaño {:>2}  {:>9.1}{}", p + 1, v,
                             if *v < 0.0 { "   ← lo perdió" } else { "" });
                }
            }
            // **SORTEO POR SORTEO, no sólo el resumen.** Sin esto no se puede
            // separar las dos explicaciones de un "aprueba 3 de 12", y son
            // opuestas: si los sorteos que pasa cada piloto son DISTINTOS, el
            // peldaño se puede hacer y lo que falta es robustez; si hay sorteos
            // que no pasa NINGÚN piloto —la vara honesta incluida—, esos
            // sorteos son escenarios donde la física no da tiempo y el peldaño
            // está pidiendo más que la perfección en esa fracción de los
            // sorteos (regla 3.16, y §3.3: "un poste puesto justo donde ya está
            // el dron no da tiempo de reacción a nadie").
            print!("\n  sorteo a sorteo: ");
            for (i, v) in notas.iter().enumerate() {
                print!("{}{}", if i > 0 { " " } else { "" },
                       if *v >= umbral { format!("{}✓", i + 1) } else { format!("{}·", i + 1) });
            }
            println!();
            println!("\n  aprueba en {pasan} de {n} sorteos");
            println!("  {}", match pasan {
                0 => "no sabe hacerlo — y eso es una respuesta clara",
                p if p as u32 == n => "lo sabe hacer: aprueba con cualquier escenario",
                _ => "APRUEBA A VECES — con el criterio viejo, una de esas veces \
                      alcanzaba para dar el peldaño por aprobado (regla 3.20)",
            });
        }
        // **¿Dónde gasta el tiempo cada peldaño, en celdas del repertorio?**
        //
        //   dron celdas [genoma=RUTA|clasico] [peldano=N]
        //
        // Es la medición que faltaba para poder enchufar el currículo al
        // repertorio. El repertorio parte el conocimiento en nueve celdas y
        // promete que aprender en una no puede pisar otra; para aprovechar
        // eso hay que saber **qué celda usa cada peldaño**, y eso es un censo,
        // no una intuición (regla 3.5). La tabla contesta además la pregunta
        // que decide si la promesa sirve acá: si los peldaños que hoy se
        // pelean entre sí (esquivar contra posarse, medido) viven en celdas
        // distintas, separarlos alcanza; si viven en la misma, no.
        "celdas" => {
            let usa_clasico = args.iter().any(|a| a == "clasico");
            let genoma = args.iter()
                .find_map(|a| a.strip_prefix("genoma="))
                .unwrap_or(GENOMA3D);
            const CENSO: i32 = 118;
            let piloto = if usa_clasico {
                mision::censar(true);
                mision::Piloto::Clasico
            } else {
                match iafz::cargar_compatible(CENSO, genoma) {
                    Ok(()) => mision::Piloto::Red(CENSO),
                    Err(e) => {
                        println!("no pude cargar {genoma}: {e}\n(probá `dron celdas clasico`)");
                        return;
                    }
                }
            };
            println!("CENSO DE CELDAS — piloto: {}",
                     if usa_clasico { "guiado clásico (la vara)".to_string() }
                     else { genoma.to_string() });
            println!("celdas: 0-2 crucero (libre/algo/tapado), 3-5 aproximación, 6-8 sostén\n");
            mision::fijar_variante(777);
            let tabla = mision::censo_de_celdas(piloto);
            let asig = mision::asignacion_de_celdas(&tabla);
            let nombre = |c: usize| repertorio::Contexto {
                estorbo: c % repertorio::ESTORBOS, tarea: c / repertorio::ESTORBOS,
            }.nombre();
            for (p, frac) in tabla.iter().enumerate() {
                print!("  peldaño {:>2} {:<16}", p + 1, mision::misiones(p)[0].nombre);
                for f in frac.iter() {
                    if *f < 0.005 { print!("    ·"); } else { print!(" {:>4.0}%", f * 100.0); }
                }
                println!("   → entrena «{}»", nombre(asig[p]));
            }
            // **Chequeo de instrumento (regla 3.18).** Un repertorio recién
            // sembrado tiene las nueve celdas idénticas, así que volarlo tiene
            // que dar EXACTAMENTE lo mismo que volar el genoma suelto. Si no
            // da, cualquier comparación repertorio-contra-monolítico está
            // midiendo además ese defecto, y no se puede saber cuál es cuál.
            // `apt=N` lo verifica sobre la aptitud completa del peldaño N
            // —misiones propias, los tres ruidos y el examen de repaso—, que
            // es la cantidad con la que de verdad se entrena.
            if let Some(n) = args.iter().find_map(|a| a.strip_prefix("apt="))
                                        .and_then(|s| s.parse::<usize>().ok()) {
                let p = n.max(1) - 1;
                let r = repertorio::Repertorio::sembrar(CENSO);
                repertorio::fijar_banco(&r);
                mision::fijar_variante(777);
                let mono = mision::aptitud(mision::Piloto::Red(CENSO), p, true, sim3d::Ruido::REAL);
                mision::fijar_variante(777);
                let rep = mision::aptitud(
                    mision::Piloto::PorCelda { slot: CENSO, entrena: repertorio::CELDAS },
                    p, true, sim3d::Ruido::REAL);
                println!("\n  aptitud en el peldaño {}:  genoma suelto {mono:.1}   \
                          repertorio sembrado {rep:.1}   {}", p + 1,
                         if (mono - rep).abs() < 1e-3 { "· idénticos, el instrumento está sano" }
                         else { "← ¡NO son iguales! el banco no vuela como su genoma" });
                repertorio::olvidar_banco();
            }
            // **El solapamiento**: cuánto del tiempo de dos peldaños cae en las
            // mismas celdas. 1.00 = viven exactamente en el mismo lugar, así que
            // entrenar uno puede pisar al otro aunque el repertorio esté puesto;
            // 0.00 = no pueden estorbarse ni queriendo. Es la cota superior del
            // daño que el repertorio NO puede evitar.
            println!("\n  SOLAPAMIENTO (1.00 = comparten todas las celdas)");
            print!("      ");
            for p in 0..mision::PELDANOS { print!(" {:>4}", p + 1); }
            println!();
            for a in 0..mision::PELDANOS {
                print!("  {:>3} ", a + 1);
                for b in 0..mision::PELDANOS {
                    let s: f32 = (0..repertorio::CELDAS)
                        .map(|i| tabla[a][i].min(tabla[b][i]))
                        .sum();
                    print!(" {:>4.2}", s);
                }
                println!();
            }
        }
        // **Mide un repertorio contra el genoma suelto y contra la vara.**
        //
        //   dron probarrepertorio rep=RUTA genoma=RUTA [peldanos=9,12]
        //
        // Los tres pilotos en la MISMA escala y con el MISMO sorteo de
        // escenarios: el clásico (la vara, regla 3.15 — apagar la red la da
        // gratis), el genoma monolítico y el repertorio. Sin los tres juntos,
        // "el repertorio mejoró" no significa nada: puede ser que los dos
        // estén por debajo de la vara.
        "probarrepertorio" => {
            let rep_ruta = args.iter()
                .find_map(|a| a.strip_prefix("rep="))
                .unwrap_or(repertorio::Repertorio::ARCHIVO);
            let genoma = args.iter()
                .find_map(|a| a.strip_prefix("genoma="))
                .unwrap_or(GENOMA3D);
            let peldanos: Vec<usize> = args.iter()
                .find_map(|a| a.strip_prefix("peldanos="))
                .unwrap_or("9,12")
                .split(',')
                .filter_map(|s| s.trim().parse::<usize>().ok())
                .map(|n| n.max(1) - 1)
                .collect();
            const PRUEBA: i32 = 116;
            let Some(rep) = repertorio::Repertorio::leer(rep_ruta) else {
                println!("no pude leer el repertorio {rep_ruta}");
                return;
            };
            if !iafz::cargar(PRUEBA, genoma) {
                println!("no pude leer {genoma}");
                return;
            }
            repertorio::fijar_banco(&rep);
            let por_celda = mision::Piloto::PorCelda { slot: PRUEBA, entrena: repertorio::CELDAS };
            println!("REPERTORIO {rep_ruta}\ngenoma suelto {genoma}\n");
            // **N sorteos, no uno.** Un solo juego de escenarios mide el
            // sorteo tanto como al piloto (regla 3.20), y acá se decide si un
            // mecanismo nuevo entra o no: con una muestra la respuesta sería
            // un volado. Se informan mediana y peor caso de los N.
            let n: u32 = args.iter().find_map(|a| a.strip_prefix("n="))
                             .and_then(|s| s.parse().ok()).unwrap_or(5);
            let resumen = |v: &mut Vec<f32>| -> (f32, f32) {
                v.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
                (v[v.len() / 2], v[0])
            };
            println!("  {n} sorteos de escenarios por casilla · mediana (peor)\n");
            println!("  peldaño                umbral        clásico           genoma       repertorio");
            for p in &peldanos {
                let u = mision::umbral_aprobado(*p);
                let (mut cs, mut gs, mut rs) = (Vec::new(), Vec::new(), Vec::new());
                for k in 0..n {
                    let v = 777 + k * 1_000_003;
                    mision::fijar_variante(v);
                    cs.push(mision::aptitud(mision::Piloto::Clasico, *p, true, sim3d::Ruido::REAL));
                    mision::fijar_variante(v);
                    gs.push(mision::aptitud(mision::Piloto::Red(PRUEBA), *p, true, sim3d::Ruido::REAL));
                    mision::fijar_variante(v);
                    rs.push(mision::aptitud(por_celda, *p, true, sim3d::Ruido::REAL));
                }
                let (cm, cp) = resumen(&mut cs);
                let (gm, gp) = resumen(&mut gs);
                let (rm, rp) = resumen(&mut rs);
                println!("  {:>2} {:<18} {u:>8.1}  {cm:>7.1} ({cp:>7.1})  {gm:>7.1} ({gp:>7.1})  \
                          {rm:>7.1} ({rp:>7.1}){}",
                         p + 1, mision::misiones(*p)[0].nombre,
                         if rm > gm + 1.0 { "  ← el repertorio gana" } else { "" });
            }
            // Y CUÁL peldaño viejo arrastra la nota, con los dos pilotos: es
            // la pregunta que decide si el repertorio sirve (regla 3.8).
            for p in &peldanos {
                if *p == 0 { continue; }
                mision::fijar_variante(777);
                let g = mision::repaso_por_peldano(mision::Piloto::Red(PRUEBA), *p);
                mision::fijar_variante(777);
                let r = mision::repaso_por_peldano(por_celda, *p);
                println!("\n  examen de repaso en el peldaño {} (peor primero):", p + 1);
                let mut ord: Vec<usize> = (0..g.len()).collect();
                ord.sort_by(|a, b| g[*a].1.partial_cmp(&g[*b].1).unwrap());
                println!("      peldaño     genoma  repertorio");
                for i in ord.iter().take(5) {
                    println!("      {:>7}  {:>9.1}  {:>10.1}{}", g[*i].0 + 1, g[*i].1, r[*i].1,
                             if r[*i].1 > g[*i].1 + 1.0 { "   ← recordado mejor" } else { "" });
                }
            }
            repertorio::olvidar_banco();
        }
        // **Arma un repertorio con las fotos de peldaño de una corrida.**
        //
        //   dron armarrepertorio corrida=PREFIJO [salida=RUTA]
        //
        // El entrenador ya guarda una foto del campeón en CADA peldaño
        // aprobado (`PREFIJO.peldanoN`). Esas fotos son, literalmente, el
        // genoma que sabía hacer esa maniobra **el día que la aprobó** —antes
        // de que los peldaños siguientes se la pisaran—. Y la asignación de
        // celdas dice qué peldaño le corresponde a cada celda. Juntando las
        // dos cosas, cualquier corrida monolítica vieja se convierte en un
        // repertorio sin gastar una sola generación.
        //
        // Es la regla 3.17 aplicada adentro del repo: la pieza que faltaba ya
        // estaba, guardada por un mecanismo que se había puesto para otra cosa
        // (no perder el mejor resultado de una corrida larga, bitácora 7.25).
        "armarrepertorio" => {
            let Some(pref) = args.iter().find_map(|a| a.strip_prefix("corrida=")) else {
                println!("uso: dron armarrepertorio corrida=PREFIJO [salida=RUTA]");
                return;
            };
            let salida = args.iter()
                .find_map(|a| a.strip_prefix("salida="))
                .unwrap_or("genomes/DRON3D_REPERTORIO_ARMADO.bin");
            const ARMA: i32 = 117;
            // La asignación se calcula igual que en el entrenador: censo con
            // la vara y celda que cada peldaño ESTRENA (regla 3.3, una sola
            // implementación o el repertorio armado no sería el que se
            // entrena).
            mision::fijar_variante(777);
            let asig = mision::asignacion_de_celdas(
                &mision::censo_de_celdas(mision::Piloto::Clasico));
            // De cada celda, la foto del ÚLTIMO peldaño suyo que se aprobó:
            // es la más avanzada que todavía tenía esa capacidad probada.
            let mut fotos: Vec<Option<(usize, String)>> = vec![None; repertorio::CELDAS];
            let mut ultima: Option<String> = None;
            for p in 0..mision::PELDANOS {
                let f = format!("{pref}.peldano{}", p + 1);
                if std::path::Path::new(&f).exists() {
                    fotos[asig[p]] = Some((p + 1, f.clone()));
                    ultima = Some(f);
                }
            }
            let Some(base) = ultima else {
                println!("no encontré ninguna foto {pref}.peldanoN");
                return;
            };
            // Semilla: la foto más avanzada. Las celdas que ningún peldaño
            // reclama —el censo dice que cinco de las nueve casi no se
            // visitan— se quedan con ella, nunca con ruido.
            if !iafz::cargar(ARMA, &base) {
                println!("no pude leer {base}");
                return;
            }
            let mut rep = repertorio::Repertorio::sembrar(ARMA);
            println!("REPERTORIO ARMADO desde {pref}");
            for c in 0..repertorio::CELDAS {
                let ctx = repertorio::Contexto {
                    estorbo: c % repertorio::ESTORBOS, tarea: c / repertorio::ESTORBOS };
                match &fotos[c] {
                    Some((p, f)) => {
                        if iafz::cargar(ARMA, f) {
                            rep.reemplazar_elite(ctx, iafz::volcar(ARMA));
                            println!("  {:<22} ← peldaño {p:>2} ({f})", ctx.nombre());
                        }
                    }
                    None => println!("  {:<22} ← sin peldaño propio: queda la foto más avanzada",
                                     ctx.nombre()),
                }
            }
            match rep.escribir(salida) {
                Ok(()) => println!("\nescrito en {salida}"),
                Err(e) => println!("\nno pude escribir {salida}: {e}"),
            }
        }
        // Saca el genoma de UNA celda del repertorio a un `.adn` suelto.
        //
        //   dron extraercelda [0-8] [rep=RUTA] salida=RUTA
        //
        // Para qué: sin esto, lo que aprende una celda **sólo se puede mirar
        // con la adaptación en vuelo encendida**, y ahí el visor pasa un buen
        // rato «midiendo la vara» con la red en cero — o sea mostrando el
        // piloto clásico, que es exactamente el malentendido que costó la
        // regla 3.12. Con la celda extraída se la instala como genoma
        // principal y se la mira volar directo, sin intermediarios.
        "extraercelda" => {
            let cual: usize = args.get(1).and_then(|s| s.parse().ok()).unwrap_or(2);
            let rep_ruta = args.iter()
                .find_map(|a| a.strip_prefix("rep="))
                .unwrap_or(repertorio::Repertorio::ARCHIVO);
            let salida = match args.iter().find_map(|a| a.strip_prefix("salida=")) {
                Some(s) => s,
                None => { println!("falta salida=RUTA (regla 3.11)"); return; }
            };
            let r = match repertorio::Repertorio::leer(rep_ruta) {
                Some(r) => r,
                None => { println!("no pude leer {rep_ruta}"); return; }
            };
            if cual >= r.celdas.len() {
                println!("celda {cual} fuera de rango (hay {})", r.celdas.len());
                return;
            }
            const EXTRAE: i32 = 120;
            iafz::restaurar(EXTRAE, &r.celdas[cual].genoma);
            if iafz::guardar(EXTRAE, salida) {
                let c = repertorio::Contexto { estorbo: cual % repertorio::ESTORBOS,
                                               tarea: cual / repertorio::ESTORBOS };
                println!("celda {cual} «{}» → {salida}", c.nombre());
            } else {
                println!("no pude escribir {salida}");
            }
        }
        // Depura UNA ruta de «ir de A a B» tramo a tramo, red vs. clásico.
        //   dron capacidadestraza [ruta 0-4] [genoma=RUTA]
        "capacidadestraza" => {
            let ruta = args.get(1).and_then(|s| s.parse().ok()).unwrap_or(0usize);
            let genoma = args.iter()
                .find_map(|a| a.strip_prefix("genoma="))
                .unwrap_or(GENOMA3D);
            capacidades::trazar_a_b(genoma, ruta);
        }
        // `dron residuocero ENTRADA SALIDA`: el mismo genoma con las cuatro
        // salidas en cero. Por la identidad de la regla 3.15 vuela EXACTAMENTE
        // el clásico (verificado con `perseguir`: idéntico en los 6
        // recorridos), así que es la semilla honesta de un residuo.
        "residuocero" => {
            let (Some(ent), Some(sal)) = (args.get(1), args.get(2)) else {
                eprintln!("uso: dron residuocero ENTRADA SALIDA");
                std::process::exit(2);
            };
            iafz::init(0, 1);
            if !iafz::cargar(0, ent) { eprintln!("no pude leer {ent}"); std::process::exit(1); }
            let mut g = iafz::volcar(0);
            iafz::salidas_en_cero(&mut g);
            iafz::restaurar(0, &g);
            if !iafz::guardar(0, sal) { eprintln!("no pude escribir {sal}"); std::process::exit(1); }
            println!("  {sal}: salidas en cero (vuela el clásico exacto)");
        }
        "perseguir" => {
            const AGENTE: i32 = 0;
            let con_genoma = !args.iter().any(|a| a == "solo-clasico");
            // Por defecto lee el genoma DE PERSECUCIÓN, no `GENOMA3D` — son
            // sensores distintos (`Guia`), cargarle uno al otro sería ruido,
            // no una consigna. `genoma=RUTA` pisa esto para leer otro archivo
            // (por ejemplo el que va guardando `entrenarpersecucion` en vivo).
            let ruta = args.iter()
                .find_map(|a| a.strip_prefix("genoma="))
                .unwrap_or("genomes/DRON3D_PERSECUCION.adn");
            iafz::init(AGENTE, 1);
            let id = if con_genoma && iafz::cargar(AGENTE, ruta) {
                Some(AGENTE)
            } else {
                if con_genoma {
                    println!("  (no se pudo cargar {ruta}: sólo se mide el clásico)");
                }
                None
            };
            persecucion::informe(id);
        }
        "visor" => {
            let puerto = args.get(1).and_then(|s| s.parse().ok()).unwrap_or(8080);
            std::process::exit(web::servir(GENOMA3D, puerto));
        }
        _ => {
            println!("uso: dron entrenarmision [gens] [ver] [rapido] [semilla=N] [salida=RUTA]");
            println!("       LA tarea: despegar, cruzar, posarse, volver, posarse\n");
            println!("     dron capacidades            qué sabe hacer, en castellano");
            println!("     dron perseguir [solo-clasico] [genoma=RUTA]   ¿sigue a un vehículo que se mueve?");
            println!("     dron entrenarpersecucion [gens] [rapido] [semilla=N] [salida=RUTA]");
            println!("       la tarea nueva: sostener el puesto detrás de un vehículo en movimiento");
            println!("     dron mision [peldaño 1-16] [cual]    vuela una misión y la dibuja");
            println!("     dron mision [peldaño] 0 clasico      la misma con el guiado clásico");
            println!("     dron mision [peldaño] 0 detalle      los 9 casos detrás de la aptitud");
            println!("     dron mision [peldaño] 0 salidas      qué EMITE el cerebro");
            println!("     dron sorteos [peldaño] [clasico|rep=RUTA] [n=N]   ¿o fue el sorteo?");
            println!("     dron visor [puerto]         visor 3D en el navegador\n");
            println!("     dron estres [peldaño] [n=N]   la vara bajo averías reales (regla 3.18)");
            println!("     dron tarea \"despegar 3; ir 6 0; posar; agarrar 0.3; ...\" [clasico]   una tarea dicha, sin reentrenar");
            println!("     dron refinar [gens]           refina un nicho del repertorio fuera de línea");
            println!("     dron celdas [clasico]         qué celda del repertorio usa cada peldaño");
            println!("     dron puente ...               consignas → canales MSP para una placa real");
            println!("     dron litewing [--seco|--signos|--armar]   consignas → CRTP por WiFi (LiteWing/ESP-Drone)");
        }
    }
}






