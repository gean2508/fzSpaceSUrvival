//! Cuadricóptero en tres ejes, con ruido de sensores.
//!
//! El paso de 2D a 3D no es cosmético: es lo que obliga a las cuatro consignas
//! (alabeo, cabeceo, guiñada, altura) y lo que hace que el modelo se parezca a
//! lo que un controlador de vuelo comercial espera recibir.
//!
//! Convención: `z` hacia arriba. Ángulos en radianes.
//!   roll  (φ) inclina hacia los costados  -> mueve en `y`
//!   pitch (θ) inclina adelante/atrás      -> mueve en `x`
//!   yaw   (ψ) gira sobre sí mismo         -> apunta

pub const G: f32 = 9.81;

/// Sectores del anillo de proximidad. Cinco sobre ±1.4 rad da 32° por sector:
/// grueso, pero alcanza para "¿por qué lado está el hueco?", que es la
/// pregunta que el resumen viejo no podía contestar. El presupuesto manda —
/// son 10 de los 14 bits de visión (2 por sector) y no hay más.
pub const SECTORES: usize = 5;

/// Tope de looming (tasa de acercamiento, 1/s) que el anillo representa. Es el
/// mismo rango que `tools/ojo.py` usa para normalizar la divergencia del flujo
/// óptico, y tiene que seguir siéndolo: si divergen, el genoma entrenado en el
/// simulador lee mal a la cámara real.
pub const RANGO_LOOMING: f32 = 4.0;

/// Medio campo del anillo de proximidad, en radianes: el cono dentro del cual
/// un obstáculo existe para el piloto. Era un literal `1.4` adentro de
/// `Camara::nueva` y nada más; ahora es público porque **el piloto clásico
/// también lo necesita** para saber a qué rumbo corresponde cada sector del
/// anillo, y si los dos números divergieran el piloto estaría esquivando hacia
/// un rumbo que el sensor no mide (regla 3.3: una sola implementación, o las
/// dos mitades describen drones distintos).
pub const CAMPO_OBSTACULOS: f32 = 1.4;

/// El rumbo, en el marco del cuerpo, del CENTRO del sector `i` del anillo.
/// Es la inversa exacta del reparto que hace `Camara::ver_con`
/// (`t = (rumbo/campo + 1)/2`, `i = t*SECTORES`), y vive al lado de él por el
/// mismo motivo: si una de las dos se corrige sola, el piloto apunta a un
/// lugar que el sensor no mide.
pub fn rumbo_de_sector(i: usize) -> f32 {
    CAMPO_OBSTACULOS * (2.0 * (i as f32 + 0.5) / SECTORES as f32 - 1.0)
}

// ---------------------------------------------------------------------------
// EL APARATO CONCRETO
//
// Hasta acá el dron simulado no era ningún dron: era 0.8 kg con empuje 2.5×peso
// y arrastre lineal, números elegidos para que el gym anduviera. Con eso el
// techo de velocidad daba ~4 m/s (14 km/h) y **seguir una moto era imposible
// por física, no por falta de entrenamiento**. Como el punto final del proyecto
// es hardware, los números de acá en adelante son los de un aparato que se
// puede comprar, no los que le convienen al simulador.
//
// El aparato elegido: **quad de 5 pulgadas tipo persecución** (clase freestyle
// con cámara y batería 6S 1300 mAh). Es la categoría que de verdad sigue a un
// vehículo por carretera; un fotográfico tipo Mavic no pasa de ~19 m/s y no
// sirve para el caso de uso.
//
//   masa en vuelo        0.90 kg   (marco 5" + 6S 1300 + cámara + gimbal chico)
//   empuje máximo        4.0 × peso   (35.3 N, cuatro motores 2207 con hélice 5")
//   inclinación máxima   acos(1/4) = 75.5°  — la del empuje, no una regla
//   velocidad techo      ~31 m/s = 112 km/h (calculada abajo, y con prueba)
// ---------------------------------------------------------------------------

pub const MASA: f32 = 0.90;
pub const IXX: f32 = 0.0075; // inercia en alabeo y cabeceo, 5" con batería centrada
pub const IZZ: f32 = 0.0140; // en guiñada cuesta más girar
pub const DT: f32 = 1.0 / 200.0;

/// Relación empuje/peso. 4.0 no es generoso: es lo normal en un 5" de 6S. Un
/// aparato con menos no puede sostener 75° de inclinación y por lo tanto no
/// puede correr, porque la velocidad máxima la fija el ángulo que puede
/// mantener sin caerse.
pub const EMPUJE_PESO: f32 = 4.0;
pub const EMPUJE_MAX: f32 = EMPUJE_PESO * MASA * G;
pub const TORQUE_MAX: f32 = 0.42;

/// Radio de la hélice de 5": 0.0635 m.
pub const RADIO_HELICE: f32 = 0.0635;
/// Altura de los rotores sobre el piso con el dron apoyado (tren + marco).
pub const ALTURA_ROTOR: f32 = 0.04;

/// **Efecto suelo**: cerca del piso el chorro de las hélices rebota y el mismo
/// motor empuja más. Cheeseman y Bennett (1955), la fórmula de manual:
/// `T_suelo / T = 1 / (1 − (R / 4h)²)`, con `h` la altura del rotor. Se
/// agregó (bitácora 7.59) porque es EL efecto que hace fallar los aterrizajes
/// reales y que un controlador sin compensación no conoce —Neural Lander
/// (Shi et al., ICRA 2019) lo aprende con una red residual sobre el modelo
/// nominal—, y el simulador no lo tenía: el clásico aterrizaba con un modelo
/// perfecto de algo que en el mundo no es así. Con 5": +11% a 5 cm del piso,
/// +3% a 9 cm, nada arriba de ~20 cm. Tope 1.3: la fórmula diverge en h → R/4
/// y los datos medidos se aplanan antes.
pub fn efecto_suelo(z: f32) -> f32 {
    let h = (z + ALTURA_ROTOR).max(0.01);
    let r = RADIO_HELICE / (4.0 * h);
    (1.0 / (1.0 - (r * r).min(0.9))).min(1.3)
}
pub const TORQUE_YAW_MAX: f32 = 0.14; // guiñada tiene mucha menos autoridad

/// Arrastre LINEAL: el de los rotores, no el del aire.
///
/// Existe de verdad y es el que domina despacio. Un rotor que se traslada ve
/// aire entrando de costado y genera una fuerza proporcional a la velocidad
/// (*rotor drag* / batimiento de pala). Es el término que hace que un dron
/// quieto en el aire frene solo si le sacás la inclinación.
pub const ARRASTRE: f32 = 0.12;

/// Arrastre CUADRÁTICO: el del aire contra el fuselaje. **Esto faltaba y era
/// el error grande.**
///
/// Con sólo arrastre lineal la velocidad terminal crece sin techo con la
/// inclinación y el aparato sería infinitamente rápido, que es exactamente por
/// qué nadie podía explicar de dónde salía el límite de 4 m/s: salía de que
/// nunca se probó rápido. La fuerza real es ½·ρ·Cd·A·v²:
///
///   ρ = 1.225 kg/m³, Cd ≈ 1.1 (un quad es una caja, no un ala),
///   A ≈ 0.045 m² de área frontal inclinado
///   → ½·1.225·1.1·0.045 = 0.0303 N por (m/s)²
///
/// En aceleración, dividido por la masa: 0.0303 / 0.90 = 0.0337 1/m.
///
/// Consecuencia medible (y hay prueba que la verifica): en vuelo estacionado a
/// inclinación θ el equilibrio es m·g·tanθ = c₁·m·v + c₂·v², así que a 75° da
/// **31.2 m/s = 112 km/h**. Ese es el techo del aparato, y coincide con lo que
/// hace un 5" real. Ya no es un número inventado: sale de la masa, del empuje
/// y del área frontal.
pub const ARRASTRE_CUAD: f32 = 0.0337;

/// Más allá de este ángulo el aparato está en ACTITUD EXTREMA. Antes esto era
/// muerte instantánea, y era un error: un dron de carreras se inclina así para
/// frenar en seco o esquivar, y una mosca que recibe una ráfaga se va mucho
/// más que 60° y se endereza. Matar ahí le enseñaba a la evolución a no
/// arriesgarse nunca — quedaba plana como un helicóptero de juguete.
///
/// Subido de 1.05 (60°) a 1.40 (80°) al pasar al aparato de persecución: con
/// el techo viejo, **volar a velocidad de crucero contaba como estar cayéndose**
/// —75° es la actitud NORMAL a 31 m/s— y la evolución habría aprendido a no
/// pasar de 60°, o sea a no pasar de 21 m/s. 80° deja margen de maniobra y
/// sigue marcando lo que importa: pasado de ahí el empuje ya no sostiene el
/// peso (cos 80° = 0.17, harían falta 5.8×peso) y el aparato cae.
pub const INCLINACION_EXTREMA: f32 = 1.40; // ~80°
/// Cuánto puede quedarse pasada de ese ángulo antes de darla por perdida:
/// 0.6 s a 200 Hz. Es la ventana para RECUPERAR. Si no se endereza en ese
/// tiempo no está maniobrando, está tumbando.
pub const PASOS_RECUPERACION: u32 = 120;
/// Envolvente de vuelo cómodo. Inclinarse por debajo de esto no se castiga:
/// para trasladarse hay que inclinarse, castigarlo era castigar volar. Sube
/// junto con la extrema y por el mismo motivo: a 1.20 rad (69°) el aparato va a
/// ~26 m/s, que es velocidad de persecución, no una emergencia.
pub const INCLINACION_COMODA: f32 = 1.20; // ~69°

/// Velocidad de equilibrio a una inclinación dada: la raíz de
/// m·g·tanθ = c₁·m·v + c₂_masa·m·v². Es el techo físico del aparato y lo usa
/// tanto la prueba como el guiado clásico para no pedir lo imposible.
///
/// El sobre que sale de los números de arriba, medido (no estimado):
///
/// | inclinación | velocidad | equivalente |
/// |---|---|---|
/// | 10° | 5.6 m/s | 20 km/h — paso de bicicleta |
/// | 30° | 11.3 m/s | 41 km/h |
/// | 50° | 16.9 m/s | 61 km/h — moto en ciudad |
/// | 60° | 20.7 m/s | 75 km/h |
/// | 69° | 25.8 m/s | 93 km/h |
/// | 75.5° | 31.8 m/s | **114 km/h** — el techo, empuje al límite |
///
/// El dron viejo vivía en la primera fila.
pub fn velocidad_techo(inclinacion: f32) -> f32 {
    let a = inclinacion.clamp(0.0, 1.5).tan() * G; // aceleración disponible
    // c2·v² + c1·v − a = 0
    let (c1, c2) = (ARRASTRE, ARRASTRE_CUAD);
    (-c1 + (c1 * c1 + 4.0 * c2 * a).sqrt()) / (2.0 * c2)
}

/// La INVERSA de `velocidad_techo`: qué inclinación hace falta para CRUCERO a
/// una velocidad dada. `atan(arrastre(v) / g)`.
///
/// Existe porque `Guiado::actitud_desde_velocidad` convertía error de
/// velocidad en ángulo con una ganancia lineal fija (`k_vel`), que es la
/// aproximación de ángulo chico (ángulo ≈ aceleración/g) — válida a 4 m/s,
/// donde el ángulo de crucero real es ~6°, pero **falsa a 25 m/s**, donde hace
/// falta ~68°. Sin este trim, sostener 68° con un P puro exige un error de
/// velocidad permanente de decenas de m/s (medido: con k_vel=0.10 el guiado se
/// asentaba a 19.2 m/s persiguiendo un objetivo a 25, un 23% corto, PARA
/// SIEMPRE) o, si se sube la ganancia para cerrar ese error, entra en el ciclo
/// límite de `pid3d::persecucion` (pitch oscilando 0.45↔1.5 rad, medido en
/// `persecucion::traza`). Ninguna de las dos falla por casualidad: es la
/// consecuencia inevitable de pedirle a una aproximación lineal que cubra un
/// rango donde deja de serlo.
///
/// Con el trim puesto, el P sólo tiene que corregir el RESIDUO (la diferencia
/// entre la velocidad actual y la de crucero), que sí es chico en régimen, así
/// que k_vel puede quedar bajo — estable — sin sacrificar el punto de
/// equilibrio.
pub fn angulo_de_crucero(v: f32) -> f32 {
    let (c1, c2) = (ARRASTRE, ARRASTRE_CUAD);
    let mag = v.abs();
    ((c1 + c2 * mag) * mag / G).atan().copysign(v)
}


#[cfg(test)]
/// La inclinación máxima que el empuje puede SOSTENER: acos(peso/empuje_max).
/// Con 4.0×peso son 75.5°. Más que eso y el aparato baja aunque acelere a
/// fondo. Usada por las pruebas de sobre de vuelo (`pruebas_sobre`,
/// `movil::pruebas::ningun_recorrido_es_inalcanzable`); `Guiado::con_reserva`
/// resuelve el mismo cociente en línea porque además necesita la reserva de
/// empuje, no el límite absoluto.
pub fn inclinacion_sostenible() -> f32 {
    (1.0 / EMPUJE_PESO).clamp(-1.0, 1.0).acos()
}

/// Averías: lo que se rompe en un dron de verdad.
///
/// Todas son fallas reales y documentadas, no dificultad inventada:
/// un motor que pierde empuje, una carga que se corrió y desbalancea, viento
/// que supera lo previsto. (El «GPS congelado» se borró: el dron no tiene GPS.)
#[derive(Clone, Copy, Default)]
pub struct Averia {
    /// Torque constante parásito: motor dañado o centro de masa corrido.
    pub torque_parasito: (f32, f32, f32),
    /// Fracción de empuje perdida, 0 a 1.
    pub perdida_empuje: f32,
    /// Multiplicador del viento.
    pub viento_mult: f32,
    /// Carga extra en kg.
    pub masa_extra: f32,
}

/// Distancia de cada motor a cada eje de un quad X de 5" (marco de 220 mm de
/// diagonal: 110 mm al centro, × cos 45°). Torque de roll/pitch por newton de
/// diferencia de empuje.
pub const BRAZO_EJE: f32 = 0.078;
/// Torque de guiñada por newton de empuje (arrastre de la hélice): del orden
/// de 0.016 m para hélices de 5".
pub const K_GUINADA: f32 = 0.016;

/// Lo que CUATRO motores pueden dar de verdad, dado un pedido.
///
/// Hasta la bitácora 7.38 el simulador aplicaba empuje y torques por
/// separado, cada uno con su tope: a acelerador máximo seguía habiendo torque
/// completo, que en un quad X es imposible (el torque sale de DIFERENCIAS
/// entre motores). Medido con `dron estres`: el vuelo sano del clásico pedía
/// algo imposible en el 1.3% (peldaño 7) al 5.4% (peldaño 13) de los pasos,
/// o sea que el simulador era optimista justo en las maniobras más duras.
///
/// Prioridades de *airmode* (Betaflight): primero guiñada se recorta, después
/// roll/pitch, y el empuje se CORRE para que el torque que queda entre —se
/// sacrifica altura antes que actitud, porque perder actitud es caerse—.
/// Geometría del quad X de 5" (`BRAZO_EJE`, `K_GUINADA`).
///
/// **Primera versión, equivocada y medida:** se normalizó suponiendo que
/// `TORQUE_MAX` es lo máximo que dan los motores (medio aparato a fondo, el
/// otro apagado). Con la geometría real eso da 2 × 8.8 N × 0.078 m = 1.37
/// N·m: `TORQUE_MAX` (0.42) es el tope del CONTROLADOR, no de los motores.
/// Con la normalización mala, un giro combinado de 43% en roll y pitch ya
/// saturaba, el airmode subía el empuje y el clásico se estrellaba posando
/// (peldaño 12, misión 2, de 1294 a −43.7).
pub fn mezclar_x(empuje: f32, tx: f32, ty: f32, tz: f32, techo: f32) -> (f32, f32, f32, f32) {
    let fmax = techo / 4.0;
    let (a, b) = (BRAZO_EJE, K_GUINADA);
    let signos = [(1.0f32, 1.0f32, 1.0f32), (-1.0, 1.0, -1.0), (-1.0, -1.0, 1.0), (1.0, -1.0, -1.0)];
    let (mut r, mut p, mut y) = (tx / (4.0 * a), ty / (4.0 * a), tz / (4.0 * b));
    let rango = |r: f32, p: f32, y: f32| {
        let (mut lo, mut hi) = (f32::INFINITY, f32::NEG_INFINITY);
        for (sr, sp, sy) in signos {
            let d = sr * r + sp * p + sy * y;
            lo = lo.min(d);
            hi = hi.max(d);
        }
        (lo, hi)
    };
    let (lo, hi) = rango(r, p, y);
    if hi - lo > fmax {
        // No entra ni con el empuje corrido: primero se recorta la guiñada.
        let (lo0, hi0) = rango(r, p, 0.0);
        if hi0 - lo0 > fmax {
            y = 0.0;
            let k = fmax / (hi0 - lo0);
            r *= k;
            p *= k;
        } else {
            let mut k = 1.0f32;
            for _ in 0..20 {
                let (l, h) = rango(r, p, y * k);
                if h - l <= fmax { break; }
                k *= 0.8;
            }
            y *= k;
            if { let (l, h) = rango(r, p, y); h - l > fmax } { y = 0.0; }
        }
    }
    let (lo, hi) = rango(r, p, y);
    let base = (empuje / 4.0).clamp(-lo, (fmax - hi).max(-lo));
    (base * 4.0, r * 4.0 * a, p * 4.0 * a, y * 4.0 * b)
}

/// ¿Este pedido (empuje, torques) cabe en CUATRO motores de verdad?
///
/// El modelo aplica empuje y torques por separado, cada uno con su tope, y eso
/// es más generoso que un quad real: en un X, el torque sale de DIFERENCIAS de
/// empuje entre motores, así que a acelerador máximo (o mínimo) no queda
/// margen para girar. Mezclador X normalizado: medio aparato a fondo y el
/// otro medio apagado da exactamente `TORQUE_MAX` (y `TORQUE_YAW_MAX` en
/// guiñada). Devuelve cuánto se sale el motor más exigido, en fracción del
/// máximo por motor (0 = cabe).
pub fn exceso_de_mezcla(empuje: f32, tx: f32, ty: f32, tz: f32) -> f32 {
    let fmax = EMPUJE_MAX / 4.0;
    let (a, b) = (BRAZO_EJE, K_GUINADA);
    let (r, p, y) = (tx / (4.0 * a), ty / (4.0 * a), tz / (4.0 * b));
    let base = empuje / 4.0;
    let mut peor = 0.0f32;
    for (sr, sp, sy) in [(1.0, 1.0, 1.0), (-1.0, 1.0, -1.0), (-1.0, -1.0, 1.0), (1.0, -1.0, -1.0)] {
        let f = base + sr * r + sp * p + sy * y;
        peor = peor.max((f - fmax).max(-f) / fmax);
    }
    peor.max(0.0)
}

impl Averia {
    pub const NINGUNA: Averia = Averia {
        torque_parasito: (0.0, 0.0, 0.0), perdida_empuje: 0.0,
        viento_mult: 1.0, masa_extra: 0.0,
    };
}

/// Con qué velocidad vertical se puede tocar el suelo sin romper nada. 1.2 m/s
/// es el orden de lo que aguanta un tren de aterrizaje de dron chico; por
/// encima de eso se dobla una pata o se parte una hélice.
pub const VZ_POSADA: f32 = 1.2;
/// Y cuánto se puede estar derrapando al tocar: aterrizar de costado engancha
/// una pata y lo da vuelta.
pub const VH_POSADA: f32 = 1.0;
/// Y cuán torcido. Con más de 20° una pata toca antes que las otras.
pub const INCLINACION_POSADA: f32 = 0.35;

#[derive(Clone, Copy, Debug, Default)]
pub struct Dron3 {
    pub x: f32,
    pub y: f32,
    pub z: f32,
    pub vx: f32,
    pub vy: f32,
    pub vz: f32,
    pub roll: f32,
    pub pitch: f32,
    pub yaw: f32,
    pub p: f32, // velocidad angular de alabeo
    pub q: f32, // de cabeceo
    pub r: f32, // de guiñada
    pub caido: bool,
    /// Pasos seguidos en actitud extrema. Se resetea apenas se endereza.
    pub pasos_extremo: u32,
    /// Está en el suelo, entero. Es lo que separa POSARSE de ESTRELLARSE.
    pub apoyado: bool,
    /// Si tocar el suelo despacio y derecho cuenta como posarse.
    ///
    /// Está apagado por defecto a propósito. Las pruebas viejas del proyecto
    /// —`escalera`, `brecha`, la línea de base— no tienen ninguna
    /// misión de aterrizaje: ahí abajo sólo hay suelo, y tocarlo sigue siendo
    /// estrellarse igual que siempre. Así sus números siguen siendo
    /// comparables con los del README en vez de mejorar de golpe por un cambio
    /// de reglas. Lo prende quien tiene un punto de posada: `mision.rs`, el
    /// informe de capacidades y el visor.
    pub posarse_vale: bool,
    /// Con qué velocidad vertical tocó el suelo. Queda guardado para poder
    /// decir si el aterrizaje fue suave o un golpe, en vez de sólo si vivió.
    pub vz_toque: f32,
}

impl Dron3 {
    pub fn nuevo(x: f32, y: f32, z: f32) -> Self {
        Dron3 { x, y, z, ..Default::default() }
    }

    /// Habilita el aterrizaje: tocar el suelo despacio y derecho deja al dron
    /// `apoyado` en vez de `caido`.
    pub fn con_aterrizaje(mut self) -> Self {
        self.posarse_vale = true;
        self
    }

    /// Arranca posado en el suelo, que es de donde sale un dron de verdad.
    pub fn en_suelo(x: f32, y: f32) -> Self {
        Dron3 { x, y, z: 0.0, apoyado: true, posarse_vale: true, ..Default::default() }
    }

    /// Si está pasado del ángulo de actitud extrema AHORA. No implica caído:
    /// implica que está maniobrando fuerte o que lo golpearon.
    pub fn extremo(&self) -> bool {
        self.roll.abs() > INCLINACION_EXTREMA || self.pitch.abs() > INCLINACION_EXTREMA
    }

    /// Un paso de integración. Los límites se aplican acá y no en el
    /// controlador: así ninguna consigna puede pedir algo que el motor no da.
    pub fn paso(&mut self, empuje: f32, tx: f32, ty: f32, tz: f32, viento: (f32, f32)) {
        self.paso_averiado(empuje, tx, ty, tz, viento, &Averia::NINGUNA)
    }

    /// Igual que `paso` pero con el aparato roto de alguna manera.
    pub fn paso_averiado(
        &mut self, empuje: f32, tx: f32, ty: f32, tz: f32,
        viento: (f32, f32), av: &Averia,
    ) {
        if self.caido {
            return;
        }
        let masa = MASA + av.masa_extra;
        let t = (empuje * (1.0 - av.perdida_empuje)).clamp(0.0, EMPUJE_MAX)
            * efecto_suelo(self.z);
        let (tx, ty, tz) = (tx + av.torque_parasito.0,
                            ty + av.torque_parasito.1,
                            tz + av.torque_parasito.2);
        let viento = (viento.0 * av.viento_mult, viento.1 * av.viento_mult);
        let (sr, cr) = (self.roll.sin(), self.roll.cos());
        let (sp, cp) = (self.pitch.sin(), self.pitch.cos());
        let (sy, cy) = (self.yaw.sin(), self.yaw.cos());

        // El empuje sale del eje z del cuerpo, rotado al mundo. Por eso para
        // moverse en horizontal no queda otra que inclinarse.
        //
        // El arrastre tiene los DOS términos, y el viento entra donde tiene que
        // entrar: el aire no empuja al dron, el dron se mueve respecto AL AIRE.
        // Antes el viento se sumaba como aceleración suelta, lo que quería decir
        // que un dron quieto en un viento de 3 m/s aceleraba para siempre en vez
        // de estabilizarse desplazado. Ahora el arrastre se calcula sobre la
        // velocidad relativa al aire, que es lo que hace un fluido de verdad: el
        // viento arrastra hasta que el dron viaja con él, y no más.
        let (arx, ary, arz) = (self.vx - viento.0, self.vy - viento.1, self.vz);
        let rapidez_aire = (arx * arx + ary * ary + arz * arz).sqrt();
        let cuad = ARRASTRE_CUAD * rapidez_aire * (MASA / masa);
        let fx = -(ARRASTRE + cuad) * arx;
        let fy = -(ARRASTRE + cuad) * ary;
        let fz = -(ARRASTRE + cuad) * arz;

        let ax = (t / masa) * (cr * sp * cy + sr * sy) + fx;
        let ay = (t / masa) * (cr * sp * sy - sr * cy) + fy;
        let az = (t / masa) * (cr * cp) - G + fz;

        self.vx += ax * DT;
        self.vy += ay * DT;
        self.vz += az * DT;
        self.x += self.vx * DT;
        self.y += self.vy * DT;
        self.z += self.vz * DT;

        self.p += (tx.clamp(-TORQUE_MAX, TORQUE_MAX) / IXX) * DT - ARRASTRE * self.p * DT;
        self.q += (ty.clamp(-TORQUE_MAX, TORQUE_MAX) / IXX) * DT - ARRASTRE * self.q * DT;
        self.r += (tz.clamp(-TORQUE_YAW_MAX, TORQUE_YAW_MAX) / IZZ) * DT - ARRASTRE * self.r * DT;

        self.roll += self.p * DT;
        self.pitch += self.q * DT;
        self.yaw += self.r * DT;
        // Sin esto, un vuelo largo acumula miles de radianes: medido en el
        // visor tras 4.5 h corriendo sin reiniciarse, yaw llegó a 5072.7 rad
        // (más de 800 vueltas). f32 sólo tiene ~7 dígitos de precisión, así
        // que a esa magnitud sin.cos empiezan a perder exactitud — un vuelo
        // de duración indefinida como el del visor lo iba a sufrir tarde o
        // temprano. Envolver a ±π es gratis y no cambia nada más.
        self.yaw = (self.yaw + std::f32::consts::PI).rem_euclid(std::f32::consts::TAU)
            - std::f32::consts::PI;

        // Tocar el suelo SIGUE siendo fatal — ahí no hay nada que recuperar, y
        // aflojar eso es lo que hace que la evolución encuentre estrategias
        // suicidas que puntúan (ver README). Lo que cambia es la inclinación:
        // ya no mata sola, abre una ventana para enderezarse.
        if self.extremo() {
            self.pasos_extremo += 1;
        } else {
            self.pasos_extremo = 0;
        }
        if self.pasos_extremo > PASOS_RECUPERACION {
            self.caido = true;
            self.z = self.z.max(0.0);
            return;
        }
        self.tocar_suelo();
    }

    /// Qué pasa cuando `z` llega a cero.
    ///
    /// Antes había una sola respuesta —morir— y eso hacía que el aparato no
    /// pudiera ni despegar ni entregar nada: un dron que no puede tocar el
    /// suelo no hace misiones, hace acrobacia indefinida. Ahora hay tres
    /// estados distintos y la diferencia entre ellos es física, no un premio:
    ///
    /// - **posado**: llegó despacio (< 1.2 m/s de bajada), sin derrapar y
    ///   derecho. El piso lo sostiene y puede volver a despegar.
    /// - **estrellado**: llegó de cualquier otra forma.
    /// - **despegando**: ya estaba apoyado y el empuje superó al peso.
    fn tocar_suelo(&mut self) {
        if self.apoyado && self.z > 0.0 {
            self.apoyado = false; // el empuje ganó: despegó
            return;
        }
        if self.z > 0.0 {
            return;
        }
        self.z = 0.0;
        if self.apoyado {
            // Quieto en el piso: el suelo sostiene el peso y frena todo. Si el
            // empuje da para subir, `vz` queda positiva y despega el paso que
            // viene.
            if self.vz <= 0.0 {
                self.vz = 0.0;
                self.vx = 0.0;
                self.vy = 0.0;
                self.p = 0.0;
                self.q = 0.0;
                self.r = 0.0;
            }
            return;
        }
        // Viene del aire y acaba de tocar.
        self.vz_toque = self.vz;
        let horizontal = (self.vx * self.vx + self.vy * self.vy).sqrt();
        let derecho = self.roll.abs().max(self.pitch.abs()) < INCLINACION_POSADA;
        if self.posarse_vale && self.vz > -VZ_POSADA && horizontal < VH_POSADA && derecho {
            self.apoyado = true;
            self.vx = 0.0;
            self.vy = 0.0;
            self.vz = 0.0;
            self.p = 0.0;
            self.q = 0.0;
            self.r = 0.0;
        } else {
            self.caido = true;
        }
    }
}

#[cfg(test)]
mod pruebas_suelo {
    use super::*;

    /// Un dron cae al suelo con la velocidad vertical dada y se devuelve si
    /// quedó posado (true) o estrellado (false).
    fn tocar(vz: f32, vh: f32, roll: f32, vale: bool) -> (bool, bool) {
        // Arranca EN el suelo con velocidad de bajada: así el primer paso lo
        // mete abajo de cero seguro y dispara `tocar_suelo`. (Con z = 0.001 y
        // una bajada lenta el paso no alcanzaba a cruzar el piso y la prueba
        // no probaba nada — lo cual encontró esta misma prueba.)
        let mut d = Dron3 { z: 0.0, vz, vx: vh, roll, posarse_vale: vale, ..Default::default() };
        d.paso(0.0, 0.0, 0.0, 0.0, (0.0, 0.0));
        (d.apoyado, d.caido)
    }

    #[test]
    fn posarse_despacio_no_es_estrellarse() {
        let (apoyado, caido) = tocar(-0.5, 0.2, 0.0, true);
        assert!(apoyado && !caido, "tocar a 0.5 m/s y derecho tiene que ser posarse");
    }

    #[test]
    fn bajar_a_plomo_rompe() {
        let (apoyado, caido) = tocar(-3.0, 0.0, 0.0, true);
        assert!(!apoyado && caido, "tocar a 3 m/s tiene que romper");
    }

    #[test]
    fn aterrizar_de_costado_rompe() {
        let (apoyado, caido) = tocar(-0.3, 0.0, 1.0, true);
        assert!(!apoyado && caido, "tocar a 57° engancha una pata");
        let (apoyado, caido) = tocar(-0.3, 2.5, 0.0, true);
        assert!(!apoyado && caido, "tocar derrapando a 2.5 m/s engancha una pata");
    }

    /// Las pruebas viejas del proyecto (escalera, brecha, línea de
    /// base) no tienen misión de aterrizaje: para ellas tocar el suelo tiene
    /// que seguir siendo fatal, o sus números del README dejan de ser
    /// comparables por un cambio de reglas y no por una mejora.
    #[test]
    fn sin_permiso_tocar_el_suelo_sigue_siendo_fatal() {
        let (apoyado, caido) = tocar(-0.1, 0.0, 0.0, false);
        assert!(!apoyado && caido);
    }
}

/// El sobre de vuelo del aparato. Estas pruebas son la diferencia entre
/// "creemos que puede seguir una moto" y saberlo: integran la física de verdad
/// y miden a cuánto llega.
#[cfg(test)]
mod pruebas_sobre {
    use super::*;

    /// Vuela `segundos` inclinado a `theta` con el empuje justo para no perder
    /// altura, y devuelve la rapidez horizontal final.
    fn crucero(theta: f32, segundos: f32) -> f32 {
        let mut d = Dron3 { z: 50.0, pitch: theta, ..Default::default() };
        let empuje = (MASA * G / theta.cos()).min(EMPUJE_MAX);
        for _ in 0..(segundos / DT) as u32 {
            // Se mantiene la actitud fija: acá se mide el sobre del AIRE, no el
            // del controlador.
            d.pitch = theta;
            d.q = 0.0;
            d.paso(empuje, 0.0, 0.0, 0.0, (0.0, 0.0));
            d.z = 50.0; // se ignora la altura: interesa el techo horizontal
            d.vz = 0.0;
        }
        (d.vx * d.vx + d.vy * d.vy).sqrt()
    }

    /// El número que decide si el proyecto tiene sentido: una moto en ciudad va
    /// a 11-22 m/s (40-80 km/h) y en carretera a 25-33. Un dron que no pasa de
    /// 4 m/s no la sigue ni parado en un semáforo.
    #[test]
    fn alcanza_velocidad_de_moto() {
        let v = crucero(inclinacion_sostenible() * 0.995, 40.0);
        assert!(v > 28.0, "el aparato tiene que pasar de 28 m/s (100 km/h), dio {v:.1}");
        assert!(v < 40.0, "más de 40 m/s con este arrastre sería física mal hecha, dio {v:.1}");
    }

    /// La fórmula cerrada y la integración numérica tienen que coincidir. Si se
    /// separan, una de las dos está mal y el guiado clásico estaría pidiendo
    /// velocidades imposibles.
    #[test]
    fn la_formula_del_techo_coincide_con_la_integracion() {
        for theta in [0.3f32, 0.6, 0.9, 1.2] {
            let medido = crucero(theta, 60.0);
            let formula = velocidad_techo(theta);
            let error = (medido - formula).abs() / formula;
            assert!(error < 0.05,
                "a {theta:.1} rad: integrado {medido:.2} vs fórmula {formula:.2} ({:.0}% de error)",
                error * 100.0);
        }
    }

    /// Sin arrastre cuadrático la velocidad terminal era lineal en la
    /// inclinación y no tenía techo. Con él, duplicar la aceleración disponible
    /// NO duplica la velocidad: crece como la raíz. Esa es la firma del término
    /// que faltaba, y es lo que hace que correr cueste.
    #[test]
    fn el_arrastre_cuadratico_impone_rendimientos_decrecientes() {
        let lento = velocidad_techo(0.3);
        let rapido = velocidad_techo(1.2);
        let razon_aceleracion = 1.2f32.tan() / 0.3f32.tan();
        let razon_velocidad = rapido / lento;
        assert!(razon_velocidad < razon_aceleracion * 0.6,
            "con arrastre cuadrático la velocidad crece mucho menos que la aceleración: \
             ×{razon_aceleracion:.1} de aceleración dio sólo ×{razon_velocidad:.1} de velocidad");
    }

    /// El viento no es una aceleración eterna: es aire que se mueve. Un dron sin
    /// empuje horizontal metido en viento tiene que terminar VIAJANDO CON el
    /// viento, no acelerando sin fin.
    #[test]
    fn el_viento_arrastra_hasta_igualar_no_para_siempre() {
        let mut d = Dron3 { z: 50.0, ..Default::default() };
        for _ in 0..4000 {
            d.paso(MASA * G, 0.0, 0.0, 0.0, (6.0, 0.0));
            d.z = 50.0;
            d.vz = 0.0;
        }
        assert!((d.vx - 6.0).abs() < 0.3,
            "tendría que viajar con el viento a ~6 m/s, va a {:.2}", d.vx);
    }
}

/// Ruido de sensores.
///
/// **Es la pieza que decide si esto vuela fuera del simulador.** Un genoma
/// criado con sensores perfectos aprende a confiar en decimales que un
/// giroscopio real no tiene, y se rompe en el primer vuelo. Es el error clásico
/// de sim-to-real, y se evita entrenando con el ruido puesto.
///
/// Las magnitudes son las de sensores baratos de verdad: MPU-6050 para el
/// giróscopo, barómetro para la altura, GPS para la posición horizontal.
#[derive(Clone, Copy)]
pub struct Ruido {
    pub giro: f32,     // rad/s de desvío
    pub actitud: f32,  // rad
    pub posicion: f32, // m
    pub altura: f32,   // m
    pub velocidad: f32,
    /// Retardo del lazo, en pasos. Los sensores reales llegan tarde, y un
    /// controlador afinado sin retardo oscila apenas se lo pones.
    pub retardo: usize,
}

impl Ruido {
    /// Sin ruido: el simulador de juguete. Sirve de referencia, no de destino.
    pub const LIMPIO: Ruido = Ruido {
        giro: 0.0, actitud: 0.0, posicion: 0.0, altura: 0.0, velocidad: 0.0, retardo: 0,
    };
    /// Sensores baratos reales.
    pub const REAL: Ruido = Ruido {
        giro: 0.05, actitud: 0.02, posicion: 0.25, altura: 0.10, velocidad: 0.12, retardo: 3,
    };
    /// Peor que la realidad, a propósito: si el genoma aguanta acá, el salto al
    /// aparato no lo sorprende.
    pub const DURO: Ruido = Ruido {
        giro: 0.12, actitud: 0.05, posicion: 0.60, altura: 0.25, velocidad: 0.30, retardo: 6,
    };
}

/// Lo que el controlador *cree* que está pasando: el estado real, ensuciado y
/// retrasado. Todo lo que ve la red pasa por acá.
pub struct Sensores {
    ruido: Ruido,
    estado: u32,
    cola: std::collections::VecDeque<Dron3>,
    /// **Odometría: la posición horizontal que el dron cree tener, SIN GPS.**
    /// Decisión del usuario, innegociable (contrato, sección 1): el aparato no
    /// tiene GPS. Lo que tiene un dron de juguete es flujo óptico mirando al
    /// piso (PMW3901 y parecidos, del orden de 5 dólares) + IMU: mide
    /// VELOCIDAD, y la posición sale de integrarla, así que DERIVA.
    odom: Option<(f32, f32)>,
    /// Error de escala del flujo óptico (depende de la altura medida y de la
    /// textura del piso): fijo por vuelo, sorteado ±ESCALA_FLUJO.
    escala_flujo: f32,
    /// Sesgo del flujo (m/s), fijo por vuelo.
    sesgo_flujo: (f32, f32),
    /// Sesgo del giróscopo en guiñada (rad/s), fijo por vuelo, y el error de
    /// rumbo acumulado. Sin GPS ni magnetómetro el rumbo sale de integrar el
    /// giro, y el error de rumbo ROTA toda la odometría.
    sesgo_guinada: f32,
    error_rumbo: f32,
    /// Cuánto puede estar equivocada la odometría (1σ, m), según el propio
    /// dron: crece con lo recorrido y baja cuando algo de referencia lo
    /// corrige (`corregir`).
    sigma_odom: f32,
    /// La duda sobre el propio RUMBO (1σ, rad): sin brújula, el giróscopo
    /// deriva y la duda crece con el tiempo.
    sigma_rumbo: f32,
    /// La duda acumulada SIN correcciones: nunca baja. Decide qué puede ser
    /// ancla (`plan::Mapeado::ancla`): si se usara la duda ya corregida, un
    /// dron que se cree seguro convertiría en ancla lo que anotó con deriva
    /// real, y se fabricaría referencias falsas (bitácora 7.48).
    sigma_bruta: f32,
}

/// **Dónde CREE el dron que está el objetivo, sin GPS.**
///
/// El objetivo está en el mapa de la misión, y el mapa coincide con el marco
/// de la odometría sólo al despegar: después la odometría deriva (escala del
/// flujo, sesgo, rumbo). Volar a la coordenada del mapa es volar a donde el
/// dron CREE que está el punto, y en la entrega eso costaba la vara de 6 a 2
/// de 24 (bitácora 7.39).
///
/// Lo que hace un dron real sin GPS: el punto está MARCADO (una baliza, un
/// patrón en el piso) y cuando la cámara lo ve, mide dónde está RELATIVO a él
/// —rumbo en la imagen y distancia por el tamaño aparente, que es lo que
/// `Camara::ver_base` ya le da a la red— y ubica el punto en su propio marco:
/// `posición odométrica + lo que ve`. Así la deriva deja de importar cerca
/// del objetivo, que es donde importa. Mientras no lo ve, vuela con la última
/// estimación (o con la del mapa, si nunca lo vio).
///
/// Ve la baliza si está en el cono frontal de la cámara, entre
/// `RADIO_CONGELADO` y `ALCANCE_BALIZA`.
pub struct Baliza {
    estado: u32,
    punto: (f32, f32),
    estimado: Option<(f32, f32)>,
}

pub const ALCANCE_BALIZA: f32 = 20.0;
pub const RADIO_CONGELADO: f32 = 1.5;
/// Error de distancia por tamaño aparente: 5% de la distancia (1σ).
pub const ERROR_RANGO: f32 = 0.05;
/// Cuánto pesa cada medición visual contra la estimación (por paso de 5 ms).
pub const GANANCIA_BALIZA: f32 = 0.02;
/// Sólo para medir con y sin baliza en la misma corrida; vuela siempre en `true`.
pub static BALIZA_ACTIVA: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(true);

impl Baliza {
    pub fn nueva(semilla: u32) -> Self {
        Baliza { estado: semilla.wrapping_mul(747_796_405).wrapping_add(7), punto: (f32::NAN, f32::NAN), estimado: None }
    }

    fn gauss(&mut self) -> f32 {
        let mut s = 0.0f32;
        for _ in 0..4 {
            self.estado ^= self.estado << 13;
            self.estado ^= self.estado >> 17;
            self.estado ^= self.estado << 5;
            s += self.estado as f32 / u32::MAX as f32 - 0.5;
        }
        s * 0.87
    }

    /// Cuando el SLAM corrige la posición creída del dron (`plan.rs`), lo
    /// que la baliza estimó en el marco viejo tiene que moverse con él: si no,
    /// el dron se corrige y el objetivo queda donde estaba — y se posa al
    /// lado de la base (1.2-1.9 m, bitácora 7.48). Traslada y gira alrededor
    /// de la posición del dron.
    pub fn reubicar(&mut self, pos: (f32, f32), dx: f32, dy: f32, dth: f32) {
        if let Some(e) = self.estimado.as_mut() {
            let (s, c) = (dth.sin(), dth.cos());
            let (rx, ry) = (e.0 - pos.0, e.1 - pos.1);
            *e = (pos.0 + c * rx - s * ry + dx, pos.1 + s * rx + c * ry + dy);
        }
    }

    /// `real`: el dron de verdad (lo que la cámara fotografía). `medido`: lo
    /// que el dron cree (odometría). `obj`: el punto en el mapa. Devuelve el
    /// punto en el marco del dron.
    pub fn ver(&mut self, real: &Dron3, medido: &Dron3, obj: (f32, f32, f32), ruido: &Ruido) -> (f32, f32) {
        if (obj.0, obj.1) != self.punto {
            self.punto = (obj.0, obj.1);
            self.estimado = None;
        }
        if !SIN_GPS || !BALIZA_ACTIVA.load(std::sync::atomic::Ordering::Relaxed) {
            return (obj.0, obj.1);
        }
        let (dx, dy) = (obj.0 - real.x, obj.1 - real.y);
        let dh = (dx * dx + dy * dy).sqrt();
        let (s, c) = (real.yaw.sin(), real.yaw.cos());
        let (adelante, lateral) = (dx * c + dy * s, -dx * s + dy * c);
        let rumbo = lateral.atan2(adelante);
        // A menos de `RADIO_CONGELADO` la estimación se CONGELA: medir la
        // plataforma a 200 Hz desde encima hacía temblar el punto ±1.5 cm y
        // eso sostenía la órbita del clásico sobre la plataforma (peldaño 12:
        // 39/48 con, 46/48 sin baliza; congelada abajo, 45/48; bitácora 7.44).
        let visible = dh >= RADIO_CONGELADO
            && adelante > 0.05 && rumbo.abs() < CAMPO_OBSTACULOS && dh < ALCANCE_BALIZA;
        if visible {
            let hay = if ruido.actitud > 0.0 { 1.0 } else { 0.0 };
            let rumbo_m = rumbo + self.gauss() * ruido.actitud * hay;
            let rango_m = dh * (1.0 + self.gauss() * ERROR_RANGO * hay);
            // El rumbo se mide en la imagen (marco del cuerpo) y se pasa al
            // marco propio con el rumbo ESTIMADO: el error de rumbo de la
            // odometría entra acá igual que entra en el flujo.
            let a = rumbo_m + medido.yaw;
            let medida = (medido.x + rango_m * a.cos(), medido.y + rango_m * a.sin());
            let e = self.estimado.get_or_insert(medida);
            e.0 += (medida.0 - e.0) * GANANCIA_BALIZA;
            e.1 += (medida.1 - e.1) * GANANCIA_BALIZA;
        }
        self.estimado.unwrap_or((obj.0, obj.1))
    }
}

/// ±8% de error de escala y hasta 0.04 m/s de sesgo: el orden de lo que se
/// reporta para sensores de flujo baratos con altura por ToF. No es una
/// medición propia todavía — es el primer supuesto del modelo sin GPS, y se
/// dice así (regla 3.18).
pub const ESCALA_FLUJO: f32 = 0.08;
/// El aparato no tiene GPS (contrato §1). Queda como constante sólo para
/// poder medir el antes/después; no se vuelve a poner en `false` para entrenar.
pub const SIN_GPS: bool = true;
pub const SESGO_FLUJO: f32 = 0.04;
/// Sesgo residual de un MPU-6050 calibrado en el piso: del orden de 0.5°/s.
pub const SESGO_GUINADA: f32 = 0.009;

impl Sensores {
    pub fn nuevo(ruido: Ruido, semilla: u32) -> Self {
        Sensores {
            ruido,
            estado: semilla.wrapping_mul(2_246_822_519).wrapping_add(1),
            cola: std::collections::VecDeque::new(),
            odom: None,
            escala_flujo: 0.0,
            sesgo_flujo: (0.0, 0.0),
            sesgo_guinada: 0.0,
            error_rumbo: 0.0,
            sigma_odom: 0.0,
            sigma_rumbo: 0.0,
            sigma_bruta: 0.0,
        }
    }

    /// La duda acumulada sin correcciones (ver el campo).
    pub fn incertidumbre_bruta(&self) -> f32 {
        self.sigma_bruta
    }

    /// La duda del dron sobre su propio rumbo (1σ, rad).
    pub fn incertidumbre_rumbo(&self) -> f32 {
        self.sigma_rumbo
    }

    /// **Corrige el rumbo con una referencia** (el ángulo al que se ve un
    /// poste conocido): `d` es cuánto girar el rumbo creído, `k` la ganancia.
    pub fn corregir_rumbo(&mut self, d: f32, k: f32) {
        self.error_rumbo += d;
        self.sigma_rumbo *= (1.0 - k.clamp(0.0, 1.0)).sqrt();
    }

    /// La duda del dron sobre su propia posición (1σ, m).
    pub fn incertidumbre(&self) -> f32 {
        self.sigma_odom
    }

    /// **Corrige la odometría con una referencia** (`plan::Mapa::ubicar`):
    /// mueve la posición creída `(dx, dy)` y achica la duda según cuánto se
    /// le creyó a la referencia (`k`, la ganancia de Kalman).
    pub fn corregir(&mut self, dx: f32, dy: f32, k: f32) {
        if let Some((x, y)) = self.odom {
            self.odom = Some((x + dx, y + dy));
        }
        self.sigma_odom *= (1.0 - k.clamp(0.0, 1.0)).sqrt();
    }

    fn gauss(&mut self) -> f32 {
        // Suma de uniformes: se acerca a una normal y no necesita logaritmos.
        let mut s = 0.0f32;
        for _ in 0..4 {
            self.estado ^= self.estado << 13;
            self.estado ^= self.estado >> 17;
            self.estado ^= self.estado << 5;
            s += self.estado as f32 / u32::MAX as f32 - 0.5;
        }
        s * 0.87
    }

    pub fn leer(&mut self, real: &Dron3) -> Dron3 {
        self.cola.push_back(*real);
        while self.cola.len() > self.ruido.retardo + 1 {
            self.cola.pop_front();
        }
        let base = *self.cola.front().unwrap_or(real);

        let mut m = base;
        if self.odom.is_none() {
            // Arranca sabiendo dónde está (la partida ES el origen del marco
            // de la misión) y con los errores del flujo sorteados por vuelo.
            // Sin ruido: el sesgo/escala sólo existen si hay ruido de sensor.
            let hay = if self.ruido.velocidad > 0.0 { 1.0 } else { 0.0 };
            self.escala_flujo = self.gauss() * ESCALA_FLUJO * hay;
            self.sesgo_flujo = (self.gauss() * SESGO_FLUJO * hay, self.gauss() * SESGO_FLUJO * hay);
            self.sesgo_guinada = self.gauss() * SESGO_GUINADA * hay;
            self.odom = Some((base.x, base.y));
        }
        // Velocidad horizontal medida por el flujo: escala mala + sesgo +
        // ruido. Y la posición es su integral — nadie la corrige nunca.
        // El flujo mide en el marco del cuerpo; pasarlo al mundo usa el rumbo
        // ESTIMADO, así que el error de rumbo gira la velocidad medida.
        self.error_rumbo += self.sesgo_guinada * DT;
        let (se, ce) = (self.error_rumbo.sin(), self.error_rumbo.cos());
        let (rvx, rvy) = (base.vx * ce - base.vy * se, base.vx * se + base.vy * ce);
        let fvx = rvx * (1.0 + self.escala_flujo) + self.sesgo_flujo.0
                  + self.gauss() * self.ruido.velocidad;
        let fvy = rvy * (1.0 + self.escala_flujo) + self.sesgo_flujo.1
                  + self.gauss() * self.ruido.velocidad;
        let (ox, oy) = self.odom.unwrap();
        let (ox, oy) = (ox + fvx * DT, oy + fvy * DT);
        self.odom = Some((ox, oy));
        // Lo que el dron sabe de su propio error: la escala del flujo (8%) y
        // el rumbo que deriva hacen que la duda crezca con lo recorrido.
        let rapidez = (fvx * fvx + fvy * fvy).sqrt();
        self.sigma_odom += (0.01 + (ESCALA_FLUJO + 0.05) * rapidez) * DT;
        self.sigma_rumbo += SESGO_GUINADA * DT;
        self.sigma_bruta += (0.01 + (ESCALA_FLUJO + 0.05) * rapidez) * DT;
        // La duda de rumbo también empuja la de posición: un rumbo errado
        // gira lo que se avanza.
        self.sigma_odom += self.sigma_rumbo * rapidez * DT;
        if SIN_GPS {
            m.x = ox;
            m.y = oy;
            m.vx = fvx;
            m.vy = fvy;
        } else {
            m.x += self.gauss() * self.ruido.posicion;
            m.y += self.gauss() * self.ruido.posicion;
        }
        m.z += self.gauss() * self.ruido.altura;
        if !SIN_GPS {
            m.vx += self.gauss() * self.ruido.velocidad;
            m.vy += self.gauss() * self.ruido.velocidad;
        }
        m.vz += self.gauss() * self.ruido.velocidad;
        m.roll += self.gauss() * self.ruido.actitud;
        m.pitch += self.gauss() * self.ruido.actitud;
        m.yaw += self.gauss() * self.ruido.actitud + if SIN_GPS { self.error_rumbo } else { 0.0 };
        m.p += self.gauss() * self.ruido.giro;
        m.q += self.gauss() * self.ruido.giro;
        m.r += self.gauss() * self.ruido.giro;
        m
    }
}

/// Motores y batería: lo que separa un simulador de juguete de uno que sirve.
///
/// Tres efectos que existen en cualquier dron real y que cambian el problema
/// de control por completo:
///
/// 1. **Retardo.** Un ESC + hélice no cambia de empuje al instante: tarda unos
///    50 ms. Un controlador afinado sin retardo oscila apenas se lo ponés, y un
///    genoma criado sin retardo aprende a dar golpes de mando que en el aire se
///    convierten en un péndulo.
/// 2. **No linealidad.** El empuje va con el CUADRADO del acelerador, no lineal.
///    Al 50 % de acelerador tenés 25 % de empuje, no la mitad.
/// 3. **Caída de batería.** A medida que se descarga, el empuje máximo baja. El
///    dron que despegó sobrado termina el vuelo justo.
pub struct Motores {
    empuje: f32,
    tx: f32,
    ty: f32,
    tz: f32,
    /// 1.0 llena, 0.0 vacía.
    pub bateria: f32,
}

/// Constante de tiempo del conjunto ESC + hélice.
const TAU_MOTOR: f32 = 0.05;

impl Default for Motores {
    fn default() -> Self {
        Self::nuevo()
    }
}

impl Motores {
    pub fn nuevo() -> Self {
        Motores { empuje: MASA * G, tx: 0.0, ty: 0.0, tz: 0.0, bateria: 1.0 }
    }

    /// Convierte lo que PIDE el controlador en lo que los motores realmente
    /// entregan este paso.
    pub fn aplicar(&mut self, pedido: (f32, f32, f32, f32), dt: f32) -> (f32, f32, f32, f32) {
        // La batería recorta el techo: al final del vuelo hay ~78% del empuje.
        let techo = EMPUJE_MAX * (0.78 + 0.22 * self.bateria);
        let objetivo = pedido.0.clamp(0.0, techo);

        // Retardo de primer orden hacia lo pedido.
        let k = (dt / TAU_MOTOR).min(1.0);
        self.empuje += (objetivo - self.empuje) * k;
        self.tx += (pedido.1 - self.tx) * k;
        self.ty += (pedido.2 - self.ty) * k;
        self.tz += (pedido.3 - self.tz) * k;

        // No linealidad, bien puesta. El empuje va con el CUADRADO del
        // acelerador, pero todo controlador de vuelo real linealiza invirtiendo
        // esa curva: acelerador = sqrt(empuje/max). Aplicar el cuadrado SIN esa
        // inversion fue un bug mio y el dron no podia ni sostenerse: pedia 7.85 N
        // para flotar y los motores entregaban 3.1.
        //
        // Con la inversion puesta, lo que queda de real -y es lo que importa- es
        // el retardo, la saturacion y la caida de bateria.
        let acelerador = (self.empuje / EMPUJE_MAX).clamp(0.0, 1.0).sqrt();
        let real = (acelerador * acelerador * EMPUJE_MAX).min(techo);

        // Consumo proporcional al empuje. Un vuelo de 10 s gasta poco; la caída
        // se nota en vuelos largos, que es cuando importa.
        self.bateria = (self.bateria - acelerador * dt * 0.004).max(0.0);

        mezclar_x(real, self.tx, self.ty, self.tz, techo)
    }
}

pub struct Viento3 {
    estado: u32,
    actual: (f32, f32),
}

impl Viento3 {
    pub fn nuevo(semilla: u32) -> Self {
        Viento3 { estado: semilla.wrapping_mul(2_654_435_761).wrapping_add(1), actual: (0.0, 0.0) }
    }
    pub fn siguiente(&mut self) -> (f32, f32) {
        let mut r = || {
            self.estado ^= self.estado << 13;
            self.estado ^= self.estado >> 17;
            self.estado ^= self.estado << 5;
            (self.estado as f32 / u32::MAX as f32 - 0.5) * 3.0
        };
        let (ox, oy) = (r(), r());
        self.actual.0 += (ox - self.actual.0) * 0.01;
        self.actual.1 += (oy - self.actual.1) * 0.01;
        self.actual
    }
}

/// Un obstáculo: poste vertical (árbol, columna, antena). Es la forma más
/// simple que sigue siendo honesta para un dron — tiene que rodearlo, no
/// puede pasarle por arriba salvo que suba de verdad.
#[derive(Clone, Copy)]
pub struct Obstaculo {
    pub x: f32,
    pub y: f32,
    pub radio: f32,
    pub alto: f32,
}

impl Obstaculo {
    /// Distancia horizontal del dron a la SUPERFICIE del poste (negativa si
    /// está adentro). Arriba del poste devuelve infinito: ahí no estorba.
    pub fn distancia(&self, d: &Dron3) -> f32 {
        if d.z > self.alto {
            return f32::INFINITY;
        }
        ((d.x - self.x).powi(2) + (d.y - self.y).powi(2)).sqrt() - self.radio
    }

    pub fn choca(&self, d: &Dron3) -> bool {
        self.distancia(d) <= 0.0
    }
}

/// Cámara: lo que el dron ve, reducido a lo que cabe en bits.
///
/// **Los píxeles nunca entran a la red.** Una imagen de 16x16 en escala de
/// grises son 2048 bits; el cerebro tiene 96. Lo que entra son RASGOS que un
/// extractor calcula sobre la imagen — que es exactamente como se hace en un
/// microcontrolador, no un atajo.
///
/// Acá los rasgos se calculan de forma analítica a partir del estado real, sin
/// renderizar nada. No es una trampa: el flujo óptico de una cámara mirando al
/// suelo ES `velocidad / altura` más la rotación, y eso es precisamente lo que
/// entrega un sensor tipo PMW3901. Se simula la SALIDA del sensor, no la foto.
#[derive(Default)]
pub struct RasgosVisuales {
    /// Flujo óptico horizontal y lateral (rad/s aparentes). Reemplaza al GPS:
    /// es como los drones baratos mantienen posición bajo techo.
    pub flujo_x: f32,
    pub flujo_y: f32,
    /// Divergencia: cuánto "crece" la escena. Da el tiempo hasta el impacto.
    ///
    /// **Ya NO entra en los 96 bits**: es exactamente `vz / altura`, y las dos
    /// magnitudes van por separado con mucha mejor resolución (velocidad 18
    /// bits, altura 7). Es el mismo motivo por el que salieron `flujo_x` y
    /// `flujo_y`. Se conserva el campo porque `tools/ojo.py` lo usa como la
    /// comparación limpia contra una cámara real —ahí sí se mide desde píxeles
    /// y es la única forma de contrastar percepción real contra la simulada.
    pub divergencia: f32,
    /// Cuánto sobresale por encima del dron la punta del obstáculo que más
    /// amenaza, en metros. Positivo = todavía tapa; negativo = se le puede
    /// pasar por arriba. Ver `Camara::ver_con` para por qué hacía falta.
    pub obs_altura: f32,
    /// **ANILLO DE PROXIMIDAD**: qué tan cerca está lo más cercano en cada
    /// sector del campo de visión, de izquierda a derecha. 0 = despejado,
    /// 1 = encima.
    ///
    /// Reemplaza al par `obs_horizontal`/`obs_proximidad`, que resumía TODOS
    /// los obstáculos en un rumbo promedio y una amenaza sumada. Ese resumen
    /// tenía un modo de falla que se midió y es fatal: **dos postes a los
    /// costados promedian a "rumbo cero"**, o sea que el dron leía "el hueco
    /// está al centro" justo cuando el centro era el único lugar bloqueado.
    /// Y con obstáculos que no memorizó sacaba 1/3, peor que el piloto
    /// clásico (2/3).
    ///
    /// El anillo no promedia nada: cada sector dice lo suyo, así que "izquierda
    /// bloqueada, centro bloqueado, derecha libre" es representable. Es además
    /// lo que una cámara de verdad produce naturalmente —divergencia por
    /// columnas de la imagen— en vez de una abstracción con forma de
    /// simulador.
    pub anillo: [f32; SECTORES],
    /// Objetivo dentro de la imagen, en radianes desde el centro.
    pub obj_horizontal: f32,
    pub obj_vertical: f32,
    /// Tamaño aparente del objetivo, de 0 a 1. Es la única fuente de distancia
    /// que da una cámara sola.
    pub obj_tamano: f32,
    /// Confianza: 0 si el objetivo no está en el campo de visión, o si el suelo
    /// está tan lejos que el flujo es puro ruido.
    pub confianza: f32,
    /// Rumbo COMBINADO de los obstáculos a la vista, en radianes desde el
    /// centro de la imagen — promedio ponderado por amenaza, no el rumbo del
    /// peor solo. 0 si no hay ninguno a la vista. Ver el comentario grande de
    /// `Camara::ver_con` para la razón (regla 3.17: técnica de la industria).
    pub obs_horizontal: f32,
    /// Peligro TOTAL, de 0 (lejos o nada) a 1 (saturado) — la SUMA de la
    /// amenaza de cada obstáculo a la vista, no sólo la del peor. Dos postes
    /// moderadamente cerca ahora pesan más que uno solo muy cerca, que es lo
    /// que corresponde: hay más de un lado por donde chocar.
    pub obs_proximidad: f32,
}

pub struct Camara {
    /// Medio campo de visión de la cámara FRONTAL, en radianes.
    /// 0.6 ≈ 69° de apertura total, que es una lente normal.
    pub campo: f32,
    /// Medio campo con el que se detectan OBSTÁCULOS. Mucho más ancho que el
    /// de la cámara frontal, y no es un capricho.
    ///
    /// Con los dos iguales en 0.6 el dron entra en un ciclo límite de
    /// percepción, y se ve clarísimo en la traza de una entrega: esquiva, el
    /// poste se le va del cuadro, deja de verlo, vuelve a encarar, lo ve otra
    /// vez, esquiva. Cuarenta y cinco segundos oscilando frente al mismo poste,
    /// acercándose a 0.29 m y retrocediendo, sin avanzar un metro. No es que no
    /// sepa esquivar —con un poste solo la ruta la resuelve— es que **no puede
    /// sostener la maniobra porque pierde de vista lo que está esquivando**.
    ///
    /// 1.4 rad son 160° de apertura. Es lo honesto por los dos lados: una
    /// mosca ve casi 270° y por eso puede rodear algo sin perderlo, y un dron
    /// real que esquiva no usa la cámara de navegación sino sensores de
    /// proximidad repartidos alrededor del fuselaje. Un cono de 69° para
    /// esquivar no era un desafío, era un sensor mal elegido.
    pub campo_obstaculos: f32,
    estado: u32,
}

impl Camara {
    pub fn nueva(semilla: u32) -> Self {
        Camara {
            campo: 0.6,
            campo_obstaculos: CAMPO_OBSTACULOS,
            estado: semilla.wrapping_mul(2_654_435_761).wrapping_add(7),
        }
    }

    fn ruido(&mut self, escala: f32) -> f32 {
        self.estado ^= self.estado << 13;
        self.estado ^= self.estado >> 17;
        self.estado ^= self.estado << 5;
        (self.estado as f32 / u32::MAX as f32 - 0.5) * escala
    }

    /// **Dónde está cada poste que se ve**, para el mapa (`plan::Mapa`):
    /// rumbo en la imagen (marco del cuerpo), distancia al CENTRO y radio,
    /// con error. Una cámara monocular saca la distancia del tiempo hasta el
    /// contacto (looming) por la velocidad propia, o de un par estéreo; se
    /// supone 15% de error (1σ) en distancia y 20% en radio —del orden de lo
    /// que dan los dos métodos a pocos metros—, y es un supuesto a medir con
    /// la placa (regla 3.18). Sólo lo que está en el cono de obstáculos, a
    /// menos de `ALCANCE_MAPA`, y que el dron no sobrevuela.
    pub fn medir_postes(&mut self, d: &Dron3, obs: &[Obstaculo], ruido: &Ruido) -> Vec<(f32, f32, f32)> {
        const ALCANCE_MAPA: f32 = 10.0;
        let hay = if ruido.actitud > 0.0 { 1.0 } else { 0.0 };
        let (s, c) = (d.yaw.sin(), d.yaw.cos());
        let mut v = Vec::new();
        for o in obs {
            if d.z > o.alto {
                continue;
            }
            let (dx, dy) = (o.x - d.x, o.y - d.y);
            let dist = (dx * dx + dy * dy).sqrt();
            if dist - o.radio > ALCANCE_MAPA {
                continue;
            }
            let adelante = dx * c + dy * s;
            let lateral = -dx * s + dy * c;
            let rumbo = lateral.atan2(adelante);
            if adelante <= 0.05 || rumbo.abs() > self.campo_obstaculos {
                continue;
            }
            // Uniforme de ancho 3.46σ ≈ desvío σ.
            let rumbo_m = rumbo + self.ruido(3.46 * ruido.actitud) * hay;
            let dist_m = dist * (1.0 + self.ruido(3.46 * 0.15) * hay);
            let radio_m = o.radio * (1.0 + self.ruido(3.46 * 0.20) * hay);
            v.push((rumbo_m, dist_m, radio_m.max(0.1)));
        }
        v
    }

    pub fn ver(&mut self, d: &Dron3, ox: f32, oy: f32, oz: f32, ruido: &Ruido) -> RasgosVisuales {
        self.ver_con(d, ox, oy, oz, ruido, &[])
    }

    /// Igual que `ver`, pero además busca obstáculos.
    ///
    /// **Hasta acá se quedaba con el más amenazante y tiraba el resto** —el
    /// más cercano Y más al frente, no sólo el más cercano en línea recta—,
    /// así que dos postes a la vez colapsaban a un solo número: el que
    /// esquivaba primero desaparecía de la percepción apenas dejaba de ser
    /// "el peor", y encadenar dos rodeos quedaba invisible para la red
    /// (`CLAUDE.md`, deuda #4 — medido: 1/3 con dos postes, sin chocar
    /// ninguno pero sin llegar tampoco).
    ///
    /// **Ahora se combinan TODOS los obstáculos a la vista**, con la misma
    /// idea que usa cualquier dron de carrera real para esquivar en
    /// tiempo real: *artificial potential fields* / *velocity obstacles* —
    /// cada obstáculo aporta una contribución de amenaza ponderada por
    /// cercanía y qué tan centrado está, y se SUMAN (regla 3.17: es la
    /// técnica estándar de la industria para evitar reactivo de bajo costo,
    /// no algo inventado acá). Concretamente:
    ///
    /// - `obs_proximidad` es la SUMA de las amenazas — dos postes moderados
    ///   ahora pesan más que uno solo, en vez de que gane el mayor y el otro
    ///   se pierda.
    /// - `obs_horizontal` es el rumbo promedio, ponderado por amenaza — si
    ///   los dos quedan simétricos a los lados, el promedio da cerca de 0
    ///   (el hueco está al centro, que es información real y correcta) en
    ///   vez de mentir "sólo hay uno, a la derecha".
    ///
    /// Sigue siendo DOS números —el presupuesto de sensores no cambia (regla
    /// 3.14)— y el costo es el mismo bucle sobre `obs` que ya existía: sumar
    /// en vez de comparar no cuesta nada extra en el chip barato.
    ///
    /// **Esto cambia el significado de los bits que ve la red** (sección 4:
    /// "cambiar el código de sensores invalida los genomas"): un genoma
    /// entrenado con "el peor solo" alimentado con "la suma" no esquiva
    /// mejor, esquiva al azar. Hay que reentrenar antes de medir nada.
    pub fn ver_con(
        &mut self, d: &Dron3, ox: f32, oy: f32, oz: f32, ruido: &Ruido, obs: &[Obstaculo],
    ) -> RasgosVisuales {
        let mut rasgos = self.ver_base(d, ox, oy, oz, ruido);
        const ALCANCE: f32 = 12.0; // más lejos que esto un poste no se distingue del fondo
        let (s, c) = (d.yaw.sin(), d.yaw.cos());
        let (mut suma_amenaza, mut suma_rumbo_pesado) = (0.0f32, 0.0f32);
        let mut peor_altura = f32::NEG_INFINITY;
        let mut anillo = [0.0f32; SECTORES];
        for o in obs {
            let dist = o.distancia(d);
            if !dist.is_finite() || dist > ALCANCE {
                continue;
            }
            let (dx, dy) = (o.x - d.x, o.y - d.y);
            let adelante = dx * c + dy * s;
            let lateral = -dx * s + dy * c;
            if adelante <= 0.05 {
                continue; // lo dejó atrás
            }
            let rumbo = (lateral / adelante).atan();
            if rumbo.abs() > self.campo_obstaculos {
                continue; // fuera del campo de visión
            }
            // Amenaza: cerca pesa, y estar centrado en la imagen pesa más.
            let cercania = (1.0 - dist / ALCANCE).clamp(0.0, 1.0);
            let centrado = (1.0 - rumbo.abs() / self.campo_obstaculos).clamp(0.0, 1.0);
            let amenaza = cercania * (0.4 + 0.6 * centrado);
            suma_amenaza += amenaza;
            suma_rumbo_pesado += amenaza * rumbo;

            // **Al anillo: el sector que le toca se queda con lo MÁS CERCANO.**
            // Máximo, no suma: dos postes lejanos en el mismo sector no deben
            // sumar hasta parecer uno encima. El sector contesta "qué tan
            // tapado está este pedazo del campo", y eso lo define el que está
            // más cerca.
            let t = ((rumbo / self.campo_obstaculos) + 1.0) * 0.5; // 0..1
            let i = ((t * SECTORES as f32) as usize).min(SECTORES - 1);
            // **LOOMING, no distancia — y esto es lo que hace que el genoma
            // transfiera a una cámara de verdad.**
            //
            // El bug que esto arregla: el simulador metía `cercania`
            // (1 − dist/ALCANCE, o sea DISTANCIA) en los mismos bits en los que
            // `tools/ojo.py` mete divergencia del flujo óptico (v/d, o sea
            // TASA DE ACERCAMIENTO). Son cantidades físicas distintas: un poste
            // a 3 m da siempre lo mismo en distancia, y da CERO en looming si
            // no te estás acercando. Un genoma entrenado con una leía mal a la
            // otra, así que todo el puente de percepción estaba alimentando al
            // cerebro con datos cuyo significado no era el que había aprendido.
            //
            // Se elige que el SIMULADOR hable el idioma de la cámara, no al
            // revés, por tres razones:
            //
            //  1. Es lo que una cámara monocular puede dar sin ayuda. Estimar
            //     distancia con una sola lente necesita saber el tamaño del
            //     objeto; el looming sale del flujo, gratis.
            //  2. Es invariante de escala: un poste grande lejos y uno chico
            //     cerca loomean igual, y para no chocar eso es exactamente lo
            //     que importa.
            //  3. Es lo que usa el bicho de verdad. Los detectores de looming
            //     son el mecanismo de evasión de colisiones de los insectos,
            //     que es de donde salió la mitad de la inspiración de este
            //     proyecto.
            //
            // **El costo, dicho de frente:** quieto en el aire el looming es
            // cero, así que el dron NO VE un poste si no se está moviendo
            // hacia él. Eso no es un defecto del modelo — le pasa igual a
            // cualquier dron con una sola cámara, y es por lo que los aparatos
            // baratos le agregan un sensor de distancia. Acá se acepta a
            // propósito: mejor entrenar contra la limitación real que contra
            // una percepción que el hardware no va a tener.
            let hacia = (d.vx * dx + d.vy * dy) / dist.max(0.05);
            let looming = (hacia / dist.max(0.3)).clamp(0.0, RANGO_LOOMING) / RANGO_LOOMING;
            anillo[i] = anillo[i].max(looming);
            // **El MÁS ALTO de los que estorban, no el promedio.** Para pasar
            // por arriba hay que despejar al más alto: promediar da una altura
            // que no despeja ninguno.
            //
            // Medido, y por eso se cambió: con esta ruta (postes de 5.0 y
            // 3.0 m) el promedio ponderado le decía al dron ~4 m, trepaba a
            // 4.34 y se incrustaba en el de 5 — despejaba el chico y chocaba
            // el grande. Es el mismo defecto que ya había tenido
            // `obs_horizontal` cuando se quedaba con UN obstáculo y perdía el
            // otro: un resumen que promedia cosas incompatibles no describe a
            // ninguna.
            peor_altura = peor_altura.max(o.alto - d.z);
        }
        if suma_amenaza > 0.0 {
            let rumbo_neto = suma_rumbo_pesado / suma_amenaza;
            let n = ruido.actitud.max(0.005);
            rasgos.obs_horizontal = rumbo_neto + self.ruido(n);
            rasgos.obs_proximidad = suma_amenaza.min(1.0);
            // Cuánto SOBRESALE por encima del dron el estorbo que más amenaza,
            // ponderado igual que el rumbo. Positivo = todavía me tapa;
            // negativo = su punta quedó abajo y puedo pasarle por arriba.
            //
            // Sin esto había una asimetría entre lo que el mundo permite y lo
            // que el dron percibe: `Obstaculo::distancia` devuelve infinito si
            // el dron va más alto que el poste —o sea que la física SÍ deja
            // pasar por arriba— pero ningún bit se lo contaba. Había una
            // solución válida, subir, que era literalmente invisible para el
            // piloto. Eso es del proyecto, no del genoma.
            rasgos.obs_altura = peor_altura + self.ruido(n * 2.0);
        }
        // El anillo se escribe SIEMPRE, aunque no haya nada: "todo despejado"
        // es información, y dejarlo con el valor anterior sería memoria falsa.
        let n = ruido.actitud.max(0.005);
        for i in 0..SECTORES {
            rasgos.anillo[i] = (anillo[i] + self.ruido(n)).clamp(0.0, 1.0);
        }
        rasgos
    }

    fn ver_base(&mut self, d: &Dron3, ox: f32, oy: f32, oz: f32, ruido: &Ruido) -> RasgosVisuales {
        // --- flujo óptico de la cámara inferior ---
        // Bajo cierta altura el suelo llena el cuadro y el flujo se satura; por
        // encima de ~25 m la textura no alcanza y es ruido.
        let altura = d.z.max(0.15);
        let confianza_flujo = if d.z > 25.0 { 0.0 } else { (1.0 - d.z / 25.0).clamp(0.0, 1.0) };

        // Flujo = traslación / altura + rotación. El término rotacional es el
        // que confunde a los sensores baratos y hay que aprender a descontar.
        let flujo_x = (d.vx / altura + d.q) * confianza_flujo;
        let flujo_y = (d.vy / altura - d.p) * confianza_flujo;
        let divergencia = (d.vz / altura) * confianza_flujo;

        // --- objetivo en la imagen (cámara frontal) ---
        let (dx, dy, dz) = (ox - d.x, oy - d.y, oz - d.z);
        let (s, c) = (d.yaw.sin(), d.yaw.cos());
        let adelante = dx * c + dy * s;
        let lateral = -dx * s + dy * c;
        let dist = (dx * dx + dy * dy + dz * dz).sqrt().max(0.05);

        // Sólo se ve si está delante y dentro del campo de visión.
        let horizontal = if adelante > 0.05 { (lateral / adelante).atan() } else { 9.0 };
        let vertical = if adelante > 0.05 { (dz / adelante).atan() - d.pitch } else { 9.0 };
        let visible = horizontal.abs() < self.campo && vertical.abs() < self.campo;

        // Tamaño aparente: un objetivo de ~0.5 m visto a `dist`.
        let tamano = if visible { (0.5 / dist).clamp(0.0, 1.0) } else { 0.0 };

        let n = ruido.actitud.max(0.005);
        RasgosVisuales {
            flujo_x: flujo_x + self.ruido(n * 6.0),
            flujo_y: flujo_y + self.ruido(n * 6.0),
            divergencia: divergencia + self.ruido(n * 3.0),
            obj_horizontal: if visible { horizontal + self.ruido(n) } else { 0.0 },
            obj_vertical: if visible { vertical + self.ruido(n) } else { 0.0 },
            obj_tamano: tamano,
            confianza: if visible { confianza_flujo.max(0.5) } else { confianza_flujo * 0.5 },
            obs_horizontal: 0.0,
            obs_proximidad: 0.0,
            obs_altura: 0.0,
            anillo: [0.0; SECTORES],
        }
    }
}

#[cfg(test)]
mod pruebas_anillo {
    use super::*;

    fn con(vx: f32, vy: f32, poste_x: f32) -> [f32; SECTORES] {
        let d = Dron3 { x: 0.0, y: 0.0, z: 2.0, vx, vy, ..Default::default() };
        let mut cam = Camara::nueva(1);
        let obs = [Obstaculo { x: poste_x, y: 0.0, radio: 0.5, alto: 6.0 }];
        cam.ver_con(&d, 50.0, 0.0, 2.0, &Ruido::LIMPIO, &obs).anillo
    }

    /// **El anillo mide LOOMING, no distancia.** Es la propiedad que hace que
    /// un genoma entrenado acá sirva con una cámara de verdad: `tools/ojo.py`
    /// manda divergencia del flujo óptico, que es esta misma cantidad. Si
    /// alguien vuelve a poner distancia acá, el puente de percepción sigue
    /// "funcionando" pero alimenta al cerebro con otro significado — que es
    /// justo el bug que esta prueba existe para impedir.
    #[test]
    fn el_anillo_mide_acercamiento_no_distancia() {
        // Mismo poste, misma distancia. Lo único que cambia es si el dron va
        // hacia él.
        let quieto = con(0.0, 0.0, 4.0);
        let yendo = con(6.0, 0.0, 4.0);
        let centro = SECTORES / 2;
        assert!(quieto[centro] < 0.01,
            "quieto no hay acercamiento: el anillo tiene que dar ~0, dio {}",
            quieto[centro]);
        assert!(yendo[centro] > 0.1,
            "yendo hacia el poste el anillo tiene que prenderse, dio {}",
            yendo[centro]);
    }

    /// Acercarse más rápido, o estar más cerca, sube el looming. Es lo que le
    /// da al kernel la noción de urgencia.
    #[test]
    fn mas_rapido_o_mas_cerca_loomea_mas() {
        let centro = SECTORES / 2;
        let lento = con(3.0, 0.0, 4.0)[centro];
        let rapido = con(9.0, 0.0, 4.0)[centro];
        assert!(rapido > lento, "más rápido tiene que loomear más: {rapido} vs {lento}");
        let lejos = con(6.0, 0.0, 8.0)[centro];
        let cerca = con(6.0, 0.0, 3.0)[centro];
        assert!(cerca > lejos, "más cerca tiene que loomear más: {cerca} vs {lejos}");
    }

    /// Un poste a un costado prende SU sector, no el del centro. Es la razón
    /// de ser del anillo: con el resumen viejo (rumbo promedio) dos postes
    /// laterales decían "el hueco está al centro" justo cuando el centro era
    /// lo único bloqueado.
    #[test]
    fn cada_poste_prende_su_propio_sector() {
        let d = Dron3 { x: 0.0, y: 0.0, z: 2.0, vx: 6.0, ..Default::default() };
        let mut cam = Camara::nueva(1);
        // Dos postes, uno a cada lado, con el centro LIBRE.
        let obs = [
            Obstaculo { x: 4.0, y: -3.0, radio: 0.5, alto: 6.0 },
            Obstaculo { x: 4.0, y: 3.0, radio: 0.5, alto: 6.0 },
        ];
        let a = cam.ver_con(&d, 50.0, 0.0, 2.0, &Ruido::LIMPIO, &obs).anillo;
        let centro = SECTORES / 2;
        assert!(a[0] > 0.0 || a[SECTORES - 1] > 0.0,
            "los postes laterales tienen que prender sectores de los bordes: {a:?}");
        assert!(a[centro] <= a[0].max(a[SECTORES - 1]),
            "el centro está LIBRE y no puede leerse más tapado que los lados: {a:?}");
    }
}
