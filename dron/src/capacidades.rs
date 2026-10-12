//! Prueba de CAPACIDADES: qué sabe hacer el dron, medido, no argumentado.
//!
//! Las otras pruebas del proyecto (`escalera`, `brecha`, `duelo`) miden
//! aptitud — un número compuesto que sirve para comparar genomas entre sí
//! pero no dice si el aparato hace las cosas que uno espera de un dron. Esto
//! contesta eso en castellano:
//!
//!   1. ¿Despega?
//!   2. ¿Va de A a B — y FRENA al llegar?
//!   3. ¿Se queda quieto cuando llegó?
//!   4. ¿Se posa sin romperse?
//!   5. ¿Se recupera de un vuelco brutal sin estrellarse?
//!   6. ¿Esquiva obstáculos?
//!   7. ¿Entrega un paquete? — la misión entera, de punta a punta.
//!
//! Cada prueba imprime el número crudo y un veredicto. Si algo no está
//! implementado se dice, no se maquilla.
//!
//! **La vara está al lado.** Cada prueba de misión se corre también con el
//! guiado clásico (`mision::Piloto::Clasico`), que es el controlador que
//! usaría cualquiera. Un "SÍ" del genoma en una fila donde el clásico también
//! dice "SÍ" no prueba que la red aporte; lo que hay que mirar es dónde gana y
//! dónde pierde, y eso sólo se ve con las dos columnas puestas.
//!
//! **Ojo con comparar contra tablas viejas**: el criterio de "va de A a B"
//! cambió. Antes aprobaba terminar a menos de 1.5 m del objetivo, que un dron
//! pasando de largo a 4 m/s cumple sin querer. Ahora hay que ENTRAR y quedarse
//! medio segundo adentro a menos de 0.6 m/s. Es más difícil a propósito: era
//! el agujero por el que se colaba el orbitar.

use crate::gym3d::{pasos_por_decision, sensores};
use crate::iafz;
use crate::mision;
use crate::pid3d::{Estabilizador3, Guiado};
use crate::sim3d::{
    Camara, Dron3, Motores, Obstaculo, Ruido, Sensores, Viento3, DT, INCLINACION_COMODA,
};

/// Agente fuera del rango que usa el gym (0..96) y del visor (120).
const PRUEBA: i32 = 110;

/// Cuánto puede moverse en la ventana final y seguir contando como "quieto".
///
/// Calibrado contra la vara honesta, no elegido a gusto: medido el día de la
/// auditoría, el piloto CLÁSICO se mueve **0.02 m** y queda a 0.01 m del
/// objetivo en las tres rutas. El umbral está en 0.50 — **25× más permisivo
/// que el clásico** — justamente para que reprobar no pueda ser un artefacto
/// de haber apretado la vara. El genoma instalado igual reprueba 0/3,
/// moviéndose 0.54-0.61 m: eso es la órbita de la sección 7.8 medida de
/// frente, no un umbral duro.
const QUIETO: f32 = 0.50;

/// Un vuelco brutal inyectado a mano: el dron queda de costado pasado de
/// vertical, girando fuerte. Mucho peor que el "golpe" del visor — esto es
/// "te agarró una ráfaga y quedaste dado vuelta".
pub struct Vuelco {
    pub en_paso: usize,
    pub roll: f32,
    pub p: f32,
}

pub struct Resultado {
    /// Distancia media al OBJETIVO en la ventana final. Mide puntería,
    /// no quietud: un dron orbitando y uno clavado a la misma distancia dan el
    /// mismo número. Se conserva porque es información real, pero la pregunta
    /// "¿se queda quieto?" la contesta `excursion`, no esto.
    pub dist_media: f32,
    /// QUIETUD de verdad: lo más que se aleja de su propia posición media en la
    /// ventana final. Un dron inmóvil da ~0 esté donde esté; uno que orbita a
    /// 0.9 m de radio da ~0.9 aunque su distancia media al objetivo sea la misma.
    ///
    /// Antes acá había un `deriva_media` que sumaba la distancia al objetivo y
    /// se reportaba como "deriva": la capacidad 3 no medía quietud en absoluto,
    /// era una copia peor de la capacidad 2. Medido el día de la auditoría: las
    /// tres rutas daban 0.89 m de "deriva" con 0.89 m de distancia al objetivo,
    /// dígito por dígito, porque eran la MISMA cantidad. Es el caso que motivó
    /// la regla 3.18 (el instrumento de medición también se audita) — y la señal
    /// que lo delata es justamente ésa: **dos métricas que dan el mismo número
    /// dígito por dígito son la misma métrica**, no dos confirmaciones.
    pub excursion: f32,
    pub caido: bool,
    /// Segundos que tardó en volver a vuelo estable tras el vuelco, si volvió.
    pub segundos_recuperacion: Option<f32>,
    pub inclinacion_max: f32,
}

/// Un vuelo instrumentado hacia un objetivo fijo. `vuelco` inyecta un upset a
/// mitad de camino.
///
/// Sigue existiendo aparte de `mision::volar` porque mide dos cosas que una
/// misión no expresa bien: la DERIVA en los últimos segundos (qué tan quieto
/// se queda después de llegar) y el tiempo de RECUPERACIÓN tras un vuelco. Las
/// pruebas que sí son misiones —despegue, A a B, posada, esquivar, entrega—
/// usan `mision::volar_con`, que es el mismo código que corre el gym.
pub fn volar(
    escenario: (f32, f32, f32, f32, f32, f32, u32),
    pasos: usize,
    ruido: Ruido,
    vuelco: Option<Vuelco>,
) -> Resultado {
    volar_con(escenario, pasos, ruido, vuelco, true)
}

/// Igual, pero pudiendo apagar la red. Con `usar_red = false` las cuatro
/// salidas quedan en cero y, por la identidad de la regla 3.15, lo que vuela
/// es EXACTAMENTE el piloto clásico — sin duplicar la cadena de control, que
/// es justo lo que la regla 3.3 prohíbe. Sirve para calibrar los umbrales de
/// este informe contra la vara honesta en vez de a ojo.
pub fn volar_con(
    escenario: (f32, f32, f32, f32, f32, f32, u32),
    pasos: usize,
    ruido: Ruido,
    vuelco: Option<Vuelco>,
    usar_red: bool,
) -> Resultado {
    let (x0, y0, z0, ox, oy, oz, semilla) = escenario;
    let mut d = Dron3::nuevo(x0, y0, z0);
    let mut est = Estabilizador3::nuevo();
    let guiado = Guiado::default();
    let mut viento = Viento3::nuevo(semilla);
    let mut sensor = Sensores::nuevo(ruido, semilla ^ 0xBEEF);
    let mut baliza = crate::sim3d::Baliza::nueva(semilla ^ 0xBA11);
    let mut bajada = crate::gym3d::BajadaPorDivergencia::default();
    let mut divergencia_vista = 0.0f32;
    let mut camara = Camara::nueva(semilla ^ 0xCA11);
    let mut motores = Motores::nuevo();
    iafz::reset_memoria(PRUEBA);

    let (mut roll_sp, mut pitch_sp, mut yaw_sp, mut alt_sp) = (0.0, 0.0, 0.0, oz);
    let mut o_previa = [0.0f32; iafz::SALIDAS];
    let mut inclinacion_max = 0.0f32;
    let mut paso_vuelco = None;
    let mut segundos_recuperacion = None;
    // Ventana final para medir si se queda quieto: el último 25% del vuelo.
    let inicio_ventana = pasos * 3 / 4;
    let (mut suma_dist, mut n_dist) = (0.0f32, 0u32);
    // Las posiciones de la ventana, para medir quietud contra el centroide
    // propio. Hace falta guardarlas: el centroide no se sabe hasta el final.
    let mut posiciones: Vec<(f32, f32, f32)> = Vec::new();

    for paso in 0..pasos {
        if let Some(v) = &vuelco {
            if paso == v.en_paso && !d.caido {
                d.roll = v.roll;
                d.p = v.p;
                paso_vuelco = Some(paso);
            }
        }

        let medido = sensor.leer(&d);
        let (bx, by) = baliza.ver(&d, &medido, (ox, oy, oz), &ruido);
        if paso % pasos_por_decision() == 0 {
            let vista = camara.ver(&d, ox, oy, oz, &ruido);
            divergencia_vista = vista.divergencia;
            let (lo, hi) = sensores(&medido, &vista, bx, by, oz);
            let mut o = if usar_red {
                iafz::pensar_multi(PRUEBA, lo, hi)
            } else {
                [0.0; iafz::SALIDAS]
            };
            // El informe honesto tiene que medir la MISMA cadena que vuela
            // (regla 3.3). Sin esto, `capacidades` seguiría reportando lo que
            // el genoma logra pudiendo temblar — que es justo lo que la
            // sección 7.5 de la biblia dice que no describe a un aparato real.
            crate::persecucion::limitar_sacudida(
                &mut o, &mut o_previa, crate::persecucion::tope_de_sacudida());
            // Mismo híbrido que el gym (regla 3.3: una sola implementación).
            let c = crate::gym3d::consignas_hibridas_punto(&o, &medido, bx, by, oz, &guiado, &vista);
            (roll_sp, pitch_sp, yaw_sp, alt_sp) = (c.roll, c.pitch, c.yaw, c.alt);
        }

        // La MISMA bajada final que el gym (regla 3.3).
        if let Some(vz) = bajada.paso(crate::gym3d::en_bajada_final(&medido, bx, by, oz), divergencia_vista) {
            let base = crate::gym3d::altura_de_aproximacion(&medido, bx, by, oz);
            alt_sp = medido.z + vz / 1.6 + (alt_sp - base);
        }
        let pedido = est.control(&medido, alt_sp, roll_sp, pitch_sp, yaw_sp);
        let (e, tx, ty, tz) = motores.aplicar(pedido, DT);
        d.paso(e, tx, ty, tz, viento.siguiente());
        if d.caido {
            break;
        }

        let inc = d.roll.abs().max(d.pitch.abs());
        inclinacion_max = inclinacion_max.max(inc);
        let dist = ((ox - d.x).powi(2) + (oy - d.y).powi(2) + (oz - d.z).powi(2)).sqrt();

        // Recuperado = actitud dentro de la envolvente cómoda Y girando poco,
        // igual criterio que el premio de `gym3d::Actitud`. Sin el segundo
        // requisito, un dron que pasa por vertical a toda velocidad contaría
        // como "recuperado" en el instante que cruza cero.
        if let Some(pv) = paso_vuelco {
            if segundos_recuperacion.is_none()
                && inc < INCLINACION_COMODA
                && (d.p.abs() + d.q.abs()) < 1.0
            {
                segundos_recuperacion = Some((paso - pv) as f32 * DT);
            }
        }

        if paso >= inicio_ventana {
            suma_dist += dist;
            n_dist += 1;
            posiciones.push((d.x, d.y, d.z));
        }
    }

    // Quietud: radio de la nube de posiciones alrededor de su propio centroide.
    let excursion = if posiciones.is_empty() {
        f32::NAN
    } else {
        let n = posiciones.len() as f32;
        let (cx, cy, cz) = posiciones.iter().fold((0.0, 0.0, 0.0), |(a, b, c), (x, y, z)| {
            (a + x / n, b + y / n, c + z / n)
        });
        posiciones.iter().fold(0.0f32, |peor, (x, y, z)| {
            peor.max(((x - cx).powi(2) + (y - cy).powi(2) + (z - cz).powi(2)).sqrt())
        })
    };

    Resultado {
        dist_media: if n_dist > 0 { suma_dist / n_dist as f32 } else { f32::NAN },
        excursion,
        caido: d.caido,
        segundos_recuperacion,
        inclinacion_max,
    }
}

fn veredicto(ok: bool) -> &'static str {
    if ok { "SÍ" } else { "NO" }
}

/// (x0, y0, z0, x, y, z, semilla) de las cinco rutas de «ir de A a B» del
/// informe. Public y compartida con `trazar_a_b` — dos copias de esta lista
/// es exactamente el bug que la regla 3.3 prohíbe: se depura una ruta y el
/// informe mide otra sin que nadie se entere.
pub const RUTAS_A_B: &[(f32, f32, f32, f32, f32, f32, u32)] = &[
    (0.0, 0.0, 3.0, 6.0, 0.0, 3.0, 11),
    (0.0, 0.0, 2.0, -5.0, 5.0, 5.0, 29),
    (0.0, 0.0, 4.0, 4.0, -7.0, 2.5, 47),
    (0.0, 0.0, 3.0, 9.0, 9.0, 6.0, 77),   // más lejos que cualquiera del gym
    (0.0, 0.0, 6.0, -8.0, -3.0, 1.5, 91), // bajada larga
];

/// Depura UNA ruta de «ir de A a B» con los dos pilotos, tramo a tramo. Sale
/// de una pregunta del usuario mirando el visor: "¿por qué la red se queda
/// a un lado y el clásico no?" — la traza cada 2 s es lo que contesta eso
/// sin adivinar (regla 3.5: medir, no argumentar).
pub fn trazar_a_b(genoma: &str, ruta: usize) {
    let Some(&(x0, y0, z0, x, y, z, s)) = RUTAS_A_B.get(ruta) else {
        eprintln!("ruta {ruta} no existe — hay {} (0..{})", RUTAS_A_B.len(), RUTAS_A_B.len() - 1);
        return;
    };
    if !iafz::cargar(PRUEBA, genoma) {
        eprintln!("no encontré {genoma}");
        return;
    }
    let m = prueba("A a B", (x0, y0, z0), false, vec![mision::Punto::aire(x, y, z)], Vec::new(), s, 2400);
    println!("=== EL GENOMA ===");
    mision::informe_de_vuelo(mision::Piloto::Red(PRUEBA), &m, Ruido::REAL);
    println!("\n=== EL CLÁSICO ===");
    mision::informe_de_vuelo(mision::Piloto::Clasico, &m, Ruido::REAL);
}

/// Una misión de prueba armada a mano, fuera del catálogo del gym: son rutas
/// que el genoma NO vio entrenando, que es lo único que mide generalización.
fn prueba(nombre: &'static str, partida: (f32, f32, f32), desde_suelo: bool,
          puntos: Vec<mision::Punto>, obs: Vec<Obstaculo>, semilla: u32,
          pasos: usize) -> mision::Mision {
    mision::Mision { nombre, partida, desde_suelo, puntos, obstaculos: obs, semilla, pasos }
}

/// Corre una misión con los dos pilotos y devuelve (genoma, clásico).
fn duelo(m: &mision::Mision) -> (mision::Informe, mision::Informe) {
    (
        mision::volar_con(mision::Piloto::Red(PRUEBA), m, Ruido::REAL, None),
        mision::volar_con(mision::Piloto::Clasico, m, Ruido::REAL, None),
    )
}

/// Cómo terminó, en una palabra.
fn final_de(i: &mision::Informe) -> &'static str {
    if i.caido {
        "ESTRELLADO"
    } else if i.posado_fuera_de_lugar {
        "se posó fuera"
    } else if i.completa {
        "cumplida"
    } else {
        "no llegó"
    }
}

pub fn informe(genoma: &str) {
    if !iafz::cargar(PRUEBA, genoma) {
        println!("no pude cargar {genoma}");
        return;
    }
    println!("CAPACIDADES — qué sabe hacer el dron, medido\n");
    println!("genoma: {genoma}   ·   sensores: ruido real (MPU-6050 + baro + GPS)");
    println!("al lado, la vara honesta: el mismo vuelo con guiado clásico\n");

    // --- 1. despegar ---
    println!("1. ¿DESPEGA?  (arranca posado en el suelo y tiene que subir y quedarse)");
    let despegues: &[(f32, f32, f32, u32)] = &[
        (0.0, 0.0, 3.0, 11),
        (2.0, -1.0, 5.0, 29),
        (-3.0, 2.0, 1.8, 47), // vuelo bajo: menos margen antes de volver al piso
    ];
    let (mut desp_red, mut desp_cla) = (0, 0);
    for (i, &(x, y, alto, s)) in despegues.iter().enumerate() {
        let m = prueba("despegue", (x, y, 0.0), true,
                       vec![mision::Punto::aire(x, y, alto)], Vec::new(), s, 2400);
        let (r, c) = duelo(&m);
        desp_red += r.completa as i32;
        desp_cla += c.completa as i32;
        println!("   despegue {} a {:>3.1} m   genoma: {:<11} en {:>4.1} s   ·   clásico: {}",
                 i + 1, alto, final_de(&r), r.segundos, final_de(&c));
    }
    println!("   → genoma {}/{}   ·   clásico {}/{}\n",
             desp_red, despegues.len(), desp_cla, despegues.len());

    // --- 2. ir de A a B y FRENAR ---
    // Rutas con más distancia y más variedad que las tres del gym, para que
    // "llegar" no sea sólo repetir lo que ya vio entrenando. Public para que
    // `trazar_a_b` (más abajo) pueda depurar CUALQUIERA de estas rutas con la
    // MISMA que se mide acá, en vez de tener dos listas que puedan divergir.
    let rutas = RUTAS_A_B;
    println!("2. ¿VA DE A A B?  (entrar al metro del punto y QUEDARSE, no pasar de largo)");
    let (mut llegadas, mut llegadas_cla) = (0, 0);
    for (i, r) in rutas.iter().enumerate() {
        let m = prueba("A a B", (r.0, r.1, r.2), false,
                       vec![mision::Punto::aire(r.3, r.4, r.5)], Vec::new(), r.6, 2400);
        let (res, cla) = duelo(&m);
        llegadas += res.completa as i32;
        llegadas_cla += cla.completa as i32;
        // El clásico calculaba `dist_final` igual que el genoma pero nunca se
        // imprimía — así que "queda a 0.9 m" parecía un defecto sólo de la
        // red cuando nadie había mirado si el clásico también se conforma con
        // el radio de tolerancia. Ahora se ven los dos números, uno al lado
        // del otro.
        println!("   ruta {} → ({:>4.0},{:>4.0},{:>3.0})   genoma: queda a {:>5.2} m  {:<11} {}   ·   clásico: queda a {:>5.2} m  {:<11} {}",
                 i + 1, r.3, r.4, r.5, res.dist_final, final_de(&res),
                 veredicto(res.completa), cla.dist_final, final_de(&cla),
                 veredicto(cla.completa));
    }
    println!("   → genoma {}/{}   ·   clásico {}/{}\n",
             llegadas, rutas.len(), llegadas_cla, rutas.len());

    // --- 3. quedarse quieto ---
    // QUIETO = se mueve poco, no "está cerca". Son dos cosas distintas y hasta
    // la auditoría acá se medía la segunda con el nombre de la primera: un dron
    // orbitando a 0.9 m aprobaba igual que uno clavado. Ahora el veredicto sale
    // de `excursion` (cuánto se aleja de su propia posición media) y la
    // distancia al objetivo se sigue imprimiendo al lado, que es dato real pero
    // ya lo mide la capacidad 2.
    println!("3. ¿SE QUEDA QUIETO CUANDO LLEGA?  (cuánto se MUEVE en los últimos 3 s)");
    let (mut quietos, mut quietos_cla) = (0, 0);
    for (i, r) in rutas.iter().take(3).enumerate() {
        let res = volar(*r, 2400, Ruido::REAL, None);
        let cla = volar_con(*r, 2400, Ruido::REAL, None, false);
        let ok = !res.caido && res.excursion < QUIETO;
        let ok_cla = !cla.caido && cla.excursion < QUIETO;
        quietos += ok as i32;
        quietos_cla += ok_cla as i32;
        println!("   ruta {}   genoma: se mueve {:>5.2} m (a {:>4.2} m del obj)  {}   ·   clásico: se mueve {:>5.2} m (a {:>4.2} m)  {}",
                 i + 1, res.excursion, res.dist_media, veredicto(ok),
                 cla.excursion, cla.dist_media, veredicto(ok_cla));
    }
    println!("   → genoma {}/3   ·   clásico {}/3\n", quietos, quietos_cla);

    // --- 4. posarse ---
    // El criterio no es "llegó al suelo", es "llegó ENTERO": menos de 1.2 m/s
    // de bajada, sin derrapar y derecho (`sim3d::Dron3::tocar_suelo`). Bajar a
    // plomo y romper el tren cuenta como estrellarse, igual que en la vida.
    println!("4. ¿SE POSA SIN ROMPERSE?  (baja desde el aire hasta una plataforma)");
    let posadas: &[(f32, f32, f32, f32, f32, u32)] = &[
        (0.0, 0.0, 3.0, 0.0, 0.0, 11),   // derecho para abajo
        (0.0, 0.0, 4.0, 3.0, 0.0, 29),   // desplazada: hay que llegar Y bajar
        (0.0, 0.0, 2.5, -2.0, 2.5, 47),
    ];
    let (mut pos_red, mut pos_cla) = (0, 0);
    for (i, &(x, y, z, px, py, s)) in posadas.iter().enumerate() {
        let m = prueba("posada", (x, y, z), false,
                       vec![mision::Punto::suelo(px, py)], Vec::new(), s, 3000);
        let (r, c) = duelo(&m);
        pos_red += r.completa as i32;
        pos_cla += c.completa as i32;
        println!("   posada {} en ({:>4.1},{:>4.1})   genoma: toca a {:>5.2} m/s  {:<11} {}   ·   clásico: {}",
                 i + 1, px, py, r.vz_toque.abs(), final_de(&r),
                 veredicto(r.completa), veredicto(c.completa));
    }
    println!("   → genoma {}/{}   ·   clásico {}/{}   (límite: 1.20 m/s de bajada)\n",
             pos_red, posadas.len(), pos_cla, posadas.len());

    // --- 5. recuperarse de un vuelco ---
    println!("5. ¿SE RECUPERA DE UN VUELCO?  (se lo pone de costado a mitad de vuelo)");
    let vuelcos: &[(&str, f32, f32)] = &[
        ("de costado 60°", 1.05, 3.0),
        ("de costado 90°", 1.57, 5.0),
        ("casi invertido 140°", 2.44, 6.0),
    ];
    let mut recuperados = 0;
    for (nombre, roll, p) in vuelcos {
        let res = volar(rutas[0], 2400, Ruido::REAL,
                        Some(Vuelco { en_paso: 700, roll: *roll, p: *p }));
        let ok = !res.caido && res.segundos_recuperacion.is_some();
        recuperados += ok as i32;
        let t = match res.segundos_recuperacion {
            Some(s) => format!("{s:.2} s"),
            None => "nunca".to_string(),
        };
        println!("   {nombre:<20}  se endereza en {t:>8}   inclinación máx {:>4.0}°   {}{}",
                 res.inclinacion_max.to_degrees(), veredicto(ok),
                 if res.caido { "  ESTRELLADO" } else { "" });
    }
    println!("   → {}/{} vuelcos\n", recuperados, vuelcos.len());

    // --- 6. esquivar ---
    //
    // **Se mide DOS VECES a propósito, y la segunda es la que vale.**
    //
    // Hasta que el usuario dijo "el dron vale verga para esquivar" mirando el
    // visor, esta prueba corría sólo con `gym3d::obstaculos(i)` — o sea LOS
    // MISMOS postes con los que el genoma entrenó. El "3/3 contra 0/3 del
    // clásico" que el proyecto venía citando como su mejor resultado estaba
    // medido sobre las tres configuraciones memorizadas.
    //
    // La biblia ya advertía exactamente esto para la entrega ("rutas que el
    // genoma NO vio entrenando — entregar donde ya entrenó no mide nada") y
    // esta prueba hacía justo lo contrario. Es la regla 3.18 otra vez: el
    // instrumento decía "esquiva" cuando lo que medía era "recuerda".
    //
    // Ahora se corren los dos bloques y se reportan por separado: lo que
    // recuerda, y lo que generaliza. El segundo es el que describe al aparato.
    println!("6. ¿ESQUIVA OBSTÁCULOS?  (postes puestos EN la trayectoria)");
    println!("   6a. postes que YA VIO entrenando — mide memoria, no capacidad");
    let (mut esquivados, mut esquivados_cla) = (0, 0);
    let mut pruebas_obs = 0;
    for (i, r) in rutas.iter().take(3).enumerate() {
        let obs = crate::gym3d::obstaculos(i).to_vec();
        let n_obs = obs.len();
        // Aprobar es llegar, frenar Y no chocar. Esquivar estrellándose no
        // cuenta, y quedarse quieto en el arranque para no chocar tampoco.
        let m = prueba("esquivar", (r.0, r.1, r.2), false,
                       vec![mision::Punto::aire(r.3, r.4, r.5)], obs, r.6, 2400);
        let (res, cla) = duelo(&m);
        esquivados += res.completa as i32;
        esquivados_cla += cla.completa as i32;
        pruebas_obs += 1;
        let roce = if res.roce_min.is_finite() {
            format!("{:.2} m", res.roce_min)
        } else {
            "nunca lo vio".to_string()
        };
        println!("   ruta {} con {} poste(s)   genoma: pasó a {:>12} del poste  {:<11} {}   ·   clásico: {}",
                 i + 1, n_obs, roce, final_de(&res),
                 veredicto(res.completa), veredicto(cla.completa));
    }
    println!("   → genoma {}/{}   ·   clásico {}/{}", esquivados, pruebas_obs,
             esquivados_cla, pruebas_obs);

    // 6b. Postes en posiciones que el genoma NUNCA vio. Mismas rutas, para que
    // lo único que cambie sea dónde están los obstáculos. Son estorbos
    // razonables —en la trayectoria, esquivables rodeando— no trampas: si acá
    // choca, es que memorizó tres escenarios en vez de aprender a esquivar.
    use crate::sim3d::Obstaculo;
    let novedosos: [&[Obstaculo]; 3] = [
        // ruta 1 (0,0,3)→(6,0,3): slalom, uno a cada lado del eje
        &[Obstaculo { x: 2.2, y: 0.7, radio: 0.6, alto: 6.0 },
          Obstaculo { x: 4.2, y: -0.7, radio: 0.6, alto: 6.0 }],
        // ruta 2 (0,0,2)→(-5,5,5): corridos respecto de los de entrenamiento
        &[Obstaculo { x: -1.4, y: 2.6, radio: 0.6, alto: 5.0 },
          Obstaculo { x: -3.6, y: 2.6, radio: 0.5, alto: 7.0 }],
        // ruta 3 (0,0,4)→(4,-7,2.5): tres, y el último cerca del objetivo
        &[Obstaculo { x: 0.9, y: -3.1, radio: 0.7, alto: 5.0 },
          Obstaculo { x: 2.6, y: -4.2, radio: 0.5, alto: 4.0 },
          Obstaculo { x: 3.6, y: -6.0, radio: 0.5, alto: 3.0 }],
    ];
    println!("   6b. postes NUEVOS, nunca vistos — esto es lo que de verdad sabe hacer");
    let (mut esq_n, mut esq_n_cla, mut pruebas_n) = (0, 0, 0);
    for (i, r) in rutas.iter().take(3).enumerate() {
        let obs = novedosos[i].to_vec();
        let n_obs = obs.len();
        let m = prueba("esquivar", (r.0, r.1, r.2), false,
                       vec![mision::Punto::aire(r.3, r.4, r.5)], obs, r.6, 2400);
        let (res, cla) = duelo(&m);
        esq_n += res.completa as i32;
        esq_n_cla += cla.completa as i32;
        pruebas_n += 1;
        let roce = if res.roce_min.is_finite() {
            format!("{:.2} m", res.roce_min)
        } else {
            "nunca lo vio".to_string()
        };
        println!("   ruta {} con {} poste(s)   genoma: pasó a {:>12} del poste  {:<11} {}   ·   clásico: {}",
                 i + 1, n_obs, roce, final_de(&res),
                 veredicto(res.completa), veredicto(cla.completa));
    }
    println!("   → genoma {}/{}   ·   clásico {}/{}\n", esq_n, pruebas_n,
             esq_n_cla, pruebas_n);

    // --- 7. la misión entera ---
    // Seis maniobras encadenadas. Es la prueba que el proyecto pedía y la
    // única que no se puede aprobar por casualidad: cada tramo sólo existe si
    // el anterior salió.
    println!("7. ¿ENTREGA UN PAQUETE?  (despegar → cruzar → posarse → despegar → volver → posarse)");
    println!("   rutas que el genoma NO vio entrenando — entregar donde ya entrenó no mide nada");
    let (mut entregas, mut entregas_cla) = (0, 0);
    let entrega_ms: Vec<mision::Mision> = [
        // (destino x, y, altura de crucero, postes, semilla)
        (5.0, 3.0, 3.0, vec![Obstaculo { x: 2.5, y: 1.5, radio: 0.6, alto: 6.0 }], 101u32),
        (-4.0, -4.0, 3.5, vec![Obstaculo { x: -2.0, y: -2.0, radio: 0.5, alto: 5.0 }], 103),
        (7.0, -2.0, 2.5, Vec::new(), 107), // sin postes: aísla la cadena de maniobras
    ]
    .into_iter()
    .map(|(bx, by, crucero, obs, s)| {
        prueba("entrega", (0.0, 0.0, 0.0), true,
               vec![
                   mision::Punto::aire(0.0, 0.0, crucero),  // despegar
                   mision::Punto::aire(bx, by, crucero),    // cruzar
                   mision::Punto::suelo(bx, by),            // entregar
                   mision::Punto::aire(bx, by, crucero),    // despegar de nuevo
                   mision::Punto::aire(0.0, 0.0, crucero),  // volver
                   mision::Punto::suelo(0.0, 0.0),          // posarse en la base
               ], obs, s, 9000)
    })
    .collect();
    for (i, m) in entrega_ms.iter().enumerate() {
        let (r, c) = duelo(m);
        entregas += r.completa as i32;
        entregas_cla += c.completa as i32;
        println!("   entrega {}   genoma: {}/{} tramos en {:>4.1} s  {:<11} {}   ·   clásico: {}/{} {}",
                 i + 1, r.puntos_hechos, r.total_puntos, r.segundos, final_de(&r),
                 veredicto(r.completa), c.puntos_hechos, c.total_puntos,
                 veredicto(c.completa));
    }
    println!("   → genoma {}/{}   ·   clásico {}/{}\n",
             entregas, entrega_ms.len(), entregas_cla, entrega_ms.len());

    // 8. Una TAREA DICHA que nadie entrenó: varias cajas, el orden lo elige
    // el dron, con mapa y ruta (`plan.rs`). Tres bosques distintos.
    println!("8. ¿HACE UNA TAREA QUE NADIE LE ENSEÑÓ?  (tres cajas, él elige el orden, con mapa)");
    let tarea = "caja 6 2 a -4 5; caja -3 -4 a 8 -3; caja 2 7 a 9 4; volver";
    let bosques: [Vec<Obstaculo>; 3] = [
        vec![Obstaculo { x: 3.0, y: 1.0, radio: 0.6, alto: 6.0 }, Obstaculo { x: -2.0, y: 3.0, radio: 0.5, alto: 6.0 },
             Obstaculo { x: 4.0, y: -4.0, radio: 0.6, alto: 6.0 }],
        vec![Obstaculo { x: 1.5, y: 4.0, radio: 0.5, alto: 6.0 }, Obstaculo { x: 6.0, y: -1.0, radio: 0.6, alto: 6.0 },
             Obstaculo { x: -3.5, y: 0.5, radio: 0.5, alto: 6.0 }, Obstaculo { x: 7.5, y: 2.5, radio: 0.5, alto: 6.0 }],
        vec![Obstaculo { x: -1.0, y: -2.5, radio: 0.6, alto: 6.0 }, Obstaculo { x: 4.5, y: 5.5, radio: 0.5, alto: 6.0 },
             Obstaculo { x: 0.5, y: 6.0, radio: 0.5, alto: 6.0 }],
    ];
    let (mut tareas, mut tareas_cla) = (0, 0);
    for (i, obs) in bosques.iter().enumerate() {
        let m = mision::mision_de_tarea(tarea, (0.0, 0.0), obs.clone()).expect("la tarea de prueba está bien dicha");
        let (r, c) = duelo(&m);
        tareas += r.completa as i32;
        tareas_cla += c.completa as i32;
        println!("   bosque {}   genoma: {}/{} tramos {:<11} {}   ·   clásico: {}/{} {}",
                 i + 1, r.puntos_hechos, r.total_puntos, final_de(&r), if r.completa { "SÍ" } else { "NO" },
                 c.puntos_hechos, c.total_puntos, if c.completa { "SÍ" } else { "NO" });
    }
    println!("   → genoma {}/3   ·   clásico {}/3\n", tareas, tareas_cla);

    println!("RESUMEN                    genoma    clásico");
    println!("   despegar               {:>3}/{}      {:>3}/{}", desp_red, despegues.len(), desp_cla, despegues.len());
    println!("   ir de A a B y frenar   {:>3}/{}      {:>3}/{}", llegadas, rutas.len(), llegadas_cla, rutas.len());
    println!("   quedarse quieto        {:>3}/3      {:>3}/3", quietos, quietos_cla);
    println!("   posarse entero         {:>3}/{}      {:>3}/{}", pos_red, posadas.len(), pos_cla, posadas.len());
    println!("   recuperar de vuelco    {:>3}/{}", recuperados, vuelcos.len());
    println!("   esquivar (ya vistos)   {:>3}/{}      {:>3}/{}", esquivados, pruebas_obs, esquivados_cla, pruebas_obs);
    println!("   esquivar (NUEVOS)      {:>3}/{}      {:>3}/{}   ← el que vale", esq_n, pruebas_n, esq_n_cla, pruebas_n);
    println!("   entregar un paquete    {:>3}/{}      {:>3}/{}", entregas, entrega_ms.len(), entregas_cla, entrega_ms.len());
    println!("   tarea dicha (3 cajas)  {:>3}/3      {:>3}/3", tareas, tareas_cla);
}
