//! **Adaptación en vuelo**: el dron sigue mejorando después de salir del gym.
//!
//! El gym evoluciona *antes* de volar, con la verdad del simulador a la vista y
//! todo el tiempo del mundo. Eso deja un genoma que sabe lo que le tocó en el
//! currículo y nada más. Este módulo es lo otro: una búsqueda local que corre
//! **mientras vuela**, con lo único que un dron real tiene a bordo, para que
//! encontrarse con algo que no vio en el entrenamiento no sea el final de la
//! historia.
//!
//! # Por qué NO es "mutar y ver qué pasa"
//!
//! En vuelo un error no cuesta puntos, cuesta el aparato. Así que la estructura
//! no es una población evolucionando al azar, es **búsqueda local anclada**:
//!
//! ```text
//!   ANCLA  ── lo que el dron YA sabe. Nunca se pierde, nunca se degrada.
//!     │       Sólo lo reemplaza algo que MIDIÓ mejor que él.
//!     ├─→ ASPIRANTE = ancla + empujón fino (unos pocos bits)
//!     │      vuela una ventana corta y se mide
//!     ├─→ ¿mejor que el ancla POR UN MARGEN?  → el aspirante pasa a ser ancla
//!     └─→ ¿no?                                → se tira, el ancla sigue intacta
//! ```
//!
//! Las seis salvaguardas, que son el diseño entero y no un agregado:
//!
//! 1. **El ancla es inmutable hasta que algo le gane medido.** El peor caso de
//!    la adaptación es quedarse exactamente como estaba.
//! 2. **Se re-mide el ancla en CADA ciclo**, alternando ventanas ancla /
//!    aspirante. Si se comparara contra un mérito viejo, el dron adoptaría
//!    cualquier variante que hubiera tenido la suerte de volar con menos
//!    viento — estaría evolucionando hacia el ruido del momento, que es
//!    exactamente lo que hay que evitar.
//! 3. **Margen de promoción**: no alcanza con empatar o ganar por poco. Sin
//!    margen, la mitad de las variantes "gana" por azar de medición.
//! 4. **El empujón es FINO** (`iafz::empujar`, unos pocos bits). La mutación
//!    del gym voltea ~32 de 64 bits por palabra: eso explora, no refina.
//! 5. **No se adapta en maniobra crítica.** Cerca del suelo vuela el ancla y
//!    nada más: una posada no es lugar para probar una variante.
//! 6. **Piso duro**: si hasta el ancla mide mal, se apaga la red — y por la
//!    identidad de la regla 3.15 eso es EXACTAMENTE el piloto clásico. El peor
//!    caso del sistema entero sigue siendo el modelo físico, nunca menos.
//!
//! # El mérito se calcula con lo que hay A BORDO
//!
//! Ésta es la restricción que decide si esto sirve para hardware o es un juego
//! de simulador: el mérito **no puede mirar la verdad del simulador**. Un dron
//! real no sabe dónde está de verdad. Sabe:
//!
//! - cuánto se acercó a donde quería ir, según su propio estimador;
//! - cuánto se le agrandaron las cosas en la cámara (`looming` del anillo), que
//!   es "qué tan cerca estuve de chocar" sin necesidad de saber dónde están los
//!   obstáculos;
//! - cuánto sacudió el mando, porque las consignas las emite él;
//! - si se golpeó, por el pico de la IMU.
//!
//! Las cuatro cosas existen igual en el DBPOWER que en el simulador. Por eso el
//! mérito se define acá con eso y con nada más.

use crate::iafz;

/// Slots del kernel para el ancla y el aspirante. 122/123 quedan arriba de los
/// del visor (120/121) y de `capacidades` (110), y fuera del rango que usa el
/// gym para la población y los expertos congelados (0..48 y 96..127 se reparten
/// en la evaluación, pero la adaptación en vuelo no corre durante un
/// entrenamiento).
pub const ANCLA: i32 = 122;
pub const ASPIRANTE: i32 = 123;

/// Cuántas decisiones dura una ventana de medición.
///
/// A 100 Hz de decisión son 4 segundos. El número sale de un compromiso que se
/// puede razonar con la física del aparato, no de gusto: más corto y la ventana
/// mide una ráfaga en vez del piloto; más largo y el dron tarda un minuto en
/// probar una sola idea, con lo cual "adaptarse en vivo" deja de serlo.
pub const VENTANA: u32 = 400;

/// Evidencia mínima para que una ventana cuente.
///
/// Las ventanas también se cierran cuando el dron CAMBIA DE NICHO (pasar de
/// "crucero libre" a "crucero tapado" es otra tarea, y mezclar las dos en una
/// medición es comparar peras con manzanas). Pero una ventana de tres
/// decisiones no mide un piloto, mide una ráfaga: por debajo de este mínimo se
/// tira sin juzgar a nadie. Un segundo a 100 Hz.
pub const VENTANA_MINIMA: u32 = 100;

/// Cuánto tiene que ganarle el aspirante al ancla para reemplazarlo.
///
/// No es cero a propósito: dos ventanas distintas nunca se vuelan en
/// condiciones idénticas —viento, ruido de sensores, dónde estaba el objetivo—
/// así que una diferencia chica es indistinguible del azar. Pedir un 5% hace
/// que promover signifique algo.
pub const MARGEN: f32 = 0.05;

/// **Cuántas ventanas seguidas tiene que ganar un aspirante antes de que se
/// lo ascienda.**
///
/// No es prudencia genérica: es el mismo error que ya costó dos conclusiones
/// falsas en este proyecto (bitácora 7.30), acá en su tercera encarnación y
/// la más cara de todas, porque ésta corre EN VUELO sobre el dron real.
///
/// - El gym aprobaba un peldaño por cruzar el umbral en UNA generación con
///   escenarios sorteados: aprobó el peldaño final con 2817.4 y diez
///   generaciones después marcaba −96.6.
/// - `repertorio::refinar_nicho` ascendía por ganar UNA misión: una de cuatro
///   semillas terminó 760 puntos por debajo de donde arrancó.
/// - Y acá se ascendía por ganar UNA ventana de cuatro segundos, que es la
///   medición más ruidosa de las tres (viento, ráfaga, el tramo de la
///   trayectoria que tocó). El propio módulo ya había medido ese ruido: por
///   eso existe `repertorio::VOTOS_PARA_CAMBIAR`, que pide TRES ventanas
///   consistentes para cambiar de opinión sobre si la red ayuda. Ese cuidado
///   estaba puesto en el veredicto y faltaba en el ascenso, que es la
///   decisión que de verdad escribe en el genoma.
///
/// Dos y no tres a propósito: cada ventana cuesta segundos de vuelo real y
/// tres ventanas por variante haría el ciclo demasiado lento para que se
/// aprenda algo en una batería. Con dos, la probabilidad de ascender por
/// suerte pasa de p a p². **Lo que esto NO cubre**, y hay que decirlo: el
/// ancla se mide una sola vez por ciclo, así que las dos ventanas del
/// aspirante se comparan contra la MISMA medición del ancla. Achica el ruido
/// del aspirante, no el del ancla.
pub const MUESTRAS_PARA_PROMOVER: u32 = 2;

/// Bits que voltea un empujón. Pocos: es refinar, no saltar.
pub const BITS_EMPUJON: i32 = 3;
/// Cuánto corre los pesos de salida. `out_action_weights` vive en ±1, así que
/// 0.02 es un ajuste fino de verdad.
pub const PASO_SALIDA: f32 = 0.02;

// **Acá vivía un PISO absoluto, y era un error de calibración de manual
// (regla 3.18): un número elegido a ojo en vez de medido contra la vara.**
//
// Medido en el visor, en vivo, apenas se encendió: el dron sostenía posición
// perfectamente y el piso lo apagaba igual, porque en la fase de sostén el
// mérito es estructuralmente negativo (siempre queda algo de error) mientras
// que en crucero es positivo. Un mismo umbral no puede servir para las dos
// cosas — comparaba peras con manzanas y ganaba siempre la manzana.
//
// Lo reemplaza algo que el proyecto ya tenía y sale gratis: la fase
// `Fase::Clasico` mide el piloto clásico EN ESE MISMO NICHO (red en cero =
// clásico exacto, regla 3.15). Así "la red estorba acá" pasa de ser un número
// inventado a una comparación contra la vara honesta, por nicho, y en vez de
// apagar todo el sistema se apaga la red **sólo en el nicho donde estorba**.

/// Cada cuántas adopciones se vuelca el repertorio a disco. Tres es un
/// compromiso: escribir en cada una castigaría la flash del chip sin
/// necesidad, y esperar demasiado convierte un corte de energía en horas de
/// aprendizaje perdidas.
pub const ADOPCIONES_POR_GUARDADO: u32 = 3;

/// Altura por debajo de la cual no se experimenta: se vuela el ancla y punto.
pub const ALTURA_CRITICA: f32 = 1.0;

/// Dentro de este radio la tarea deja de ser "llegar" y pasa a ser "quedarse".
/// Es el mismo metro que usa la condición de llegada de una misión
/// (`mision::Punto`), para que la adaptación en vuelo no persiga una vara
/// distinta de la que el gym premia.
pub const RADIO_TAREA: f32 = 1.0;

/// Tope de progreso que se le acredita a UNA decisión, en metros.
///
/// No es una preferencia, es física: el aparato tiene techo de 31.8 m/s
/// (`sim3d`) y decide a 100 Hz, así que en una decisión no puede acercarse más
/// de 0.32 m ni por milagro. Un salto mayor **no es progreso**, es que cambió
/// el objetivo —en una entrega el punto salta al tramo siguiente— y sin este
/// tope ese salto entra al mérito como si el dron hubiera volado 70 metros en
/// una centésima.
///
/// Lo encontró una prueba, no el razonamiento: `una_variante_mejor_si_
/// reemplaza_al_ancla` fallaba porque la ventana del aspirante arrancaba con un
/// salto de objetivo y el mérito quedaba arruinado antes del primer metro real.
/// Acotar acá lo arregla aunque alguien se olvide de avisar que cambió el
/// tramo, que es la clase de salvaguarda que sobrevive al mantenimiento.
pub const MAX_AVANCE_POR_DECISION: f32 = 0.4;

/// Lo que se mide de una ventana. Todo sale de sensores de a bordo.
#[derive(Default, Clone, Copy)]
pub struct Merito {
    /// Metros ganados hacia el objetivo (negativo si se alejó).
    pub progreso: f32,
    /// **Lo que falta cuando ya llegó.** Suma de `distancia + rapidez` en las
    /// decisiones en que estaba dentro del radio de la tarea.
    ///
    /// Sin esto la adaptación se queda ciega justo donde el proyecto tiene su
    /// bloqueo medido: una vez adentro del radio el progreso vale ~0, así que
    /// todas las variantes empatan y ninguna puede ganar — un dron orbitando a
    /// 0.9 m mide igual que uno clavado en el punto. Medido en vivo en el
    /// visor mientras se construía esto: el dron sostenía 0.54-0.74 m con la
    /// adaptación encendida y el motor descartaba todo porque no había
    /// diferencia que medir. Con este término, acercarse y frenar SÍ paga.
    pub error_cerca: f32,
    /// Suma del `looming` máximo del anillo. Alto = pasó raspando.
    pub riesgo: f32,
    /// |Δroll| + |Δpitch| entre decisiones: cuánto tironeó el mando.
    pub sacudida: f32,
    /// Golpes detectados en la ventana.
    pub golpes: u32,
    pub pasos: u32,
}

impl Merito {
    /// El número único con el que se comparan dos ventanas, **por decisión**
    /// para que ventanas de distinto largo sean comparables.
    ///
    /// Los pesos no son arbitrarios: están puestos para que un golpe
    /// (`GOLPE = 50`) arruine la ventana por más progreso que haya hecho —
    /// llegar rápido chocando no es llegar—, y para que el riesgo pese más que
    /// la sacudida, porque raspar un obstáculo es peor que volar brusco.
    pub fn valor(&self) -> f32 {
        if self.pasos == 0 {
            return 0.0;
        }
        const RIESGO: f32 = 0.8;
        const SACUDIDA: f32 = 0.3;
        const GOLPE: f32 = 50.0;
        // `SOSTEN` pesa igual que un metro de progreso por metro de error:
        // quedarse a medio metro del punto cuesta lo mismo que no haber
        // recorrido ese medio metro, que es exactamente lo que es.
        const SOSTEN: f32 = 1.0;
        (self.progreso - RIESGO * self.riesgo - SACUDIDA * self.sacudida
            - GOLPE * self.golpes as f32
            - SOSTEN * self.error_cerca)
            / self.pasos as f32
    }
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Fase {
    /// **Midiendo la vara honesta**: red en cero, o sea el piloto clásico
    /// exacto (regla 3.15). Sin esto no hay forma de saber si la red ayuda o
    /// estorba en este nicho, y "estorba" es un resultado real y medido del
    /// proyecto, no una hipótesis (bitácora 7.4).
    Clasico,
    /// Midiendo lo que ya se sabe.
    Ancla,
    /// Midiendo una variante.
    Aspirante,
}

pub struct Adaptacion {
    /// El conocimiento clasificado por situación. Ver `repertorio.rs`.
    pub repertorio: crate::repertorio::Repertorio,
    /// El cerebro tal como salió del gym. Se guarda para poder MOSTRAR qué
    /// sinapsis cambió el dron aprendiendo en vuelo: sin la foto del antes no
    /// hay forma de dibujar el después.
    base: Vec<u8>,
    /// En qué nicho se está midiendo la ventana en curso.
    ctx: crate::repertorio::Contexto,
    fase: Fase,
    acc: Merito,
    /// Mérito del ancla en su ÚLTIMA ventana. Se refresca cada ciclo.
    merito_ancla: Option<f32>,
    /// Semilla del próximo empujón. Incremental para que la secuencia de
    /// variantes probadas sea reproducible y auditable después del vuelo.
    fuente: u32,
    pub promovidos: u32,
    pub descartados: u32,
    /// Piso duro disparado: la red queda apagada hasta que se reinicie.
    pub red_apagada: bool,
    /// Ventanas que se anularon por entrar en maniobra crítica.
    pub anuladas: u32,
    /// Cuántas adopciones van sin guardar. **"Aprende en vivo" no puede
    /// significar "hasta que se corte la luz"**: guardar sólo al apagar con la
    /// tecla deja todo lo aprendido colgando de que alguien se acuerde de
    /// apagarlo bien, y un dron real se queda sin batería sin avisar.
    pub sin_guardar: u32,
    /// Ventanas seguidas que el aspirante VIGENTE le viene ganando al ancla.
    /// Ver `MUESTRAS_PARA_PROMOVER`.
    aciertos: u32,
    /// Distancia al objetivo en la decisión anterior, para sacar el progreso.
    dist_previa: Option<f32>,
    ultima_consigna: (f32, f32),
    encendida: bool,
}

impl Adaptacion {
    #[allow(clippy::needless_update)]
    fn vacia(repertorio: crate::repertorio::Repertorio) -> Self {
        Adaptacion {
            base: repertorio.celdas[0].genoma.clone(),
            repertorio,
            ctx: crate::repertorio::Contexto { estorbo: 0, tarea: 0 },
            fase: Fase::Ancla,
            acc: Merito::default(),
            merito_ancla: None,
            fuente: 1,
            promovidos: 0,
            descartados: 0,
            red_apagada: false,
            anuladas: 0,
            sin_guardar: 0,
            aciertos: 0,
            dist_previa: None,
            ultima_consigna: (0.0, 0.0),
            encendida: false,
        }
    }
}

impl Adaptacion {
    /// Arranca desde un genoma ya entrenado. **Siempre se siembra**: adaptar
    /// desde pesos al azar no es "aprender en vivo", es volar basura acotada
    /// hasta que el piso duro la apague. La estructura sobre la que se
    /// evoluciona es justamente lo que el dron ya sabe.
    pub fn sembrar(desde: i32) -> Self {
        let repertorio = crate::repertorio::Repertorio::sembrar(desde);
        iafz::clonar(desde, ANCLA);
        iafz::clonar(desde, ASPIRANTE);
        let mut a = Adaptacion::vacia(repertorio);
        a.encendida = true;
        a
    }

    /// Igual que `sembrar`, pero con un repertorio que ya venía aprendido de
    /// vuelos anteriores. Lo que hace que "aprende en vivo" no se borre al
    /// apagar el dron.
    pub fn con_repertorio(desde: i32, repertorio: crate::repertorio::Repertorio) -> Self {
        iafz::clonar(desde, ANCLA);
        iafz::clonar(desde, ASPIRANTE);
        let mut a = Adaptacion::vacia(repertorio);
        a.encendida = true;
        a
    }

    /// ¿Conviene volcar el repertorio a disco ahora? Se pregunta después de
    /// cada ventana; el que llama decide dónde escribir.
    pub fn toca_guardar(&self) -> bool {
        self.sin_guardar >= ADOPCIONES_POR_GUARDADO
    }

    pub fn marcar_guardado(&mut self) {
        self.sin_guardar = 0;
    }

    /// Cuántos bits de sinapsis cambió cada una de las 128 neuronas respecto
    /// del cerebro con el que salió del gym, en el nicho que está volando.
    pub fn sinapsis_cambiadas(&self) -> Vec<u32> {
        crate::repertorio::sinapsis_cambiadas(
            &self.base,
            &self.repertorio.celda(self.ctx).genoma,
        )
    }

    /// El nicho en el que está volando ahora.
    pub fn contexto(&self) -> crate::repertorio::Contexto {
        self.ctx
    }

    pub fn encendida(&self) -> bool {
        self.encendida && !self.red_apagada
    }


    #[cfg(test)]
    pub fn fase(&self) -> Fase {
        self.fase
    }

    /// Qué genoma tiene que volar en esta decisión.
    pub fn slot(&self) -> i32 {
        match self.fase {
            Fase::Aspirante => ASPIRANTE,
            _ => ANCLA,
        }
    }

    /// **Si la red tiene que hablar en esta decisión.** `false` hace que las
    /// cuatro salidas queden en cero, que por la identidad de la regla 3.15 es
    /// el piloto clásico exacto. Pasa en dos casos: cuando toca medir la vara,
    /// y cuando este nicho ya tiene medido que la red estorba acá.
    pub fn usar_red(&self) -> bool {
        if !self.encendida() {
            return true; // apagada del todo: no interviene, vuela lo de siempre
        }
        if self.fase == Fase::Clasico {
            return false;
        }
        self.repertorio.red_ayuda(self.ctx) != Some(false)
    }

    /// Una decisión de vuelo. Devuelve `true` si acaba de cerrar una ventana.
    ///
    /// `z` y `objetivo_z` deciden si estamos en maniobra crítica; `dist` es la
    /// distancia al objetivo **según el estimador del dron**, no la verdadera.
    #[allow(clippy::too_many_arguments)]
    pub fn observar(
        &mut self,
        dist: f32,
        vel: f32,
        z: f32,
        looming_max: f32,
        consigna: (f32, f32),
        golpe: bool,
    ) -> bool {
        if !self.encendida() {
            return false;
        }
        // Salvaguarda 5: cerca del suelo no se experimenta. La ventana en curso
        // se anula entera —no se juzga media ventana— y se vuelve al ancla.
        if z < ALTURA_CRITICA {
            if self.fase == Fase::Aspirante {
                self.anuladas += 1;
            }
            self.fase = Fase::Ancla;
            self.acc = Merito::default();
            self.dist_previa = Some(dist);
            self.ultima_consigna = consigna;
            return false;
        }

        // ¿Seguimos en el mismo nicho? Pasar de "crucero libre" a "crucero
        // tapado" es cambiar de tarea, no seguir la misma: una ventana que
        // cruza dos nichos no mide a nadie. Se cierra lo que haya —si alcanzó
        // el mínimo de evidencia— y se arranca de nuevo con el elite del
        // nicho nuevo.
        let ctx = crate::repertorio::Contexto::de(looming_max, dist);
        if ctx != self.ctx {
            self.cambiar_de_nicho(ctx);
        }

        if let Some(previa) = self.dist_previa {
            self.acc.progreso +=
                (previa - dist).clamp(-MAX_AVANCE_POR_DECISION, MAX_AVANCE_POR_DECISION);
        }
        self.dist_previa = Some(dist);
        // Ya llegó: a partir de acá la tarea no es avanzar, es sostenerse. Se
        // cobra lo que falta para estar clavado en el punto.
        if dist < RADIO_TAREA {
            self.acc.error_cerca += dist + vel;
        }
        self.acc.riesgo += looming_max;
        self.acc.sacudida += (consigna.0 - self.ultima_consigna.0).abs()
            + (consigna.1 - self.ultima_consigna.1).abs();
        self.ultima_consigna = consigna;
        if golpe {
            self.acc.golpes += 1;
        }
        self.acc.pasos += 1;

        if self.acc.pasos >= VENTANA {
            self.cerrar_ventana();
            return true;
        }
        false
    }

    /// El dron entró a otra situación: se cierra lo medido (si alcanza), se
    /// trae el elite del nicho nuevo y se empieza de cero ahí.
    fn cambiar_de_nicho(&mut self, ctx: crate::repertorio::Contexto) {
        if self.acc.pasos >= VENTANA_MINIMA {
            self.cerrar_ventana();
        } else if self.acc.pasos > 0 {
            // Evidencia insuficiente: se tira sin juzgar a nadie. Condenar a
            // una variante por tres decisiones sería tan malo como adoptarla.
            self.anuladas += 1;
        }
        self.acc = Merito::default();
        self.aciertos = 0;
        self.dist_previa = None;
        self.ctx = ctx;
        // Cada nicho tiene SU conocimiento. Traerlo es el "recurro a la
        // memoria del genoma que sirve para este problema".
        // Nicho nuevo: se arranca midiendo la vara, después lo conocido, y
        // recién ahí se prueba algo. Es el orden que hace que cada comparación
        // tenga una referencia fresca.
        self.fase = Fase::Clasico;
        self.repertorio.cargar_en(ctx, ANCLA);
        self.merito_ancla = None;
    }

    fn cerrar_ventana(&mut self) {
        let v = self.acc.valor();
        let ctx = self.ctx;
        self.acc = Merito::default();
        match self.fase {
            // La vara honesta de ESTE nicho, recién medida. No se compara con
            // la de otro nicho ni con un número elegido a mano: volar tapado
            // mide peor que volar libre por la situación, no por el piloto.
            Fase::Clasico => {
                self.repertorio.refrescar_clasico(ctx, v);
                self.fase = Fase::Ancla;
                self.repertorio.cargar_en(ctx, ANCLA);
                iafz::reset_memoria(ANCLA);
            }
            Fase::Ancla => {
                self.merito_ancla = Some(v);
                self.repertorio.refrescar_merito(ctx, v);
                // Salvaguarda 4: empujón fino desde el ancla, nunca desde el
                // aspirante anterior — si se encadenaran empujones sin validar,
                // la deriva se acumularía sin que nada la mida.
                self.fuente = self.fuente.wrapping_add(1);
                iafz::empujar(ANCLA, ASPIRANTE, BITS_EMPUJON, PASO_SALIDA, self.fuente);
                iafz::reset_memoria(ASPIRANTE);
                self.fase = Fase::Aspirante;
            }
            Fase::Aspirante => {
                // **Una ventana no alcanza para ascender a nadie**
                // (`MUESTRAS_PARA_PROMOVER`). El aspirante tiene que ganar
                // varias seguidas; una sola derrota lo tira.
                let gana = match self.merito_ancla {
                    Some(a) => supera(v, a),
                    // Sin ancla medida no hay con qué comparar: no se asciende
                    // a ciegas, se vuelve a empezar el ciclo.
                    None => false,
                };
                if gana {
                    self.aciertos += 1;
                    if self.aciertos < MUESTRAS_PARA_PROMOVER {
                        // Sigue a prueba: MISMA variante, ventana nueva. No se
                        // vuelve a empujar (eso encadenaría derivas sin medir)
                        // ni se re-mide la vara todavía.
                        iafz::reset_memoria(ASPIRANTE);
                        return;
                    }
                    // El árbitro final vive en el repertorio: compara contra el
                    // elite DE ESTE nicho y sólo escribe ahí. Aprender a rodear
                    // un poste no tiene forma de pisar lo que el dron sabe de
                    // posarse.
                    let genoma = iafz::volcar(ASPIRANTE);
                    if self.repertorio.guardar_si_mejora(ctx, genoma, v) {
                        self.promovidos += 1;
                        self.sin_guardar += 1;
                        self.merito_ancla = Some(v);
                    } else {
                        self.descartados += 1;
                    }
                } else {
                    self.descartados += 1;
                }
                self.aciertos = 0;
                // Vuelta al principio del ciclo: se re-mide la vara antes que
                // nada. El viento de hace doce segundos no es el de ahora.
                self.fase = Fase::Clasico;
                self.repertorio.cargar_en(ctx, ANCLA);
                iafz::reset_memoria(ANCLA);
            }
        }
    }

    /// Resumen corto para el visor y los informes.
    pub fn resumen(&self) -> String {
        format!(
            "[{}] {} · adoptadas {} · descartadas {} · anuladas {}{}",
            self.ctx.nombre(),
            match self.fase {
                Fase::Clasico => "midiendo la vara (red en cero)",
                Fase::Ancla => "midiendo lo conocido",
                Fase::Aspirante => "probando variante",
            },
            self.promovidos,
            self.descartados,
            self.anuladas,
            match self.repertorio.red_ayuda(self.ctx) {
                Some(true) => " · acá la red AYUDA",
                Some(false) => " · acá la red ESTORBA → vuela clásico",
                None => " · sin veredicto todavía",
            }
        )
    }
}

/// ¿El aspirante le gana al ancla por el margen exigido?
///
/// Separado y público porque el signo del mérito complica la cuenta y es
/// exactamente el tipo de detalle que se escribe mal una vez y nadie vuelve a
/// mirar: con un mérito base NEGATIVO, `base * 1.05` es **más chico** que
/// `base`, así que multiplicar pediría menos en vez de más. Se exige el margen
/// sobre el valor absoluto, que quiere decir lo mismo en los dos signos.
pub fn supera(aspirante: f32, ancla: f32) -> bool {
    if !ancla.is_finite() {
        return false;
    }
    aspirante > ancla + MARGEN * ancla.abs().max(1.0)
}

#[cfg(test)]
mod pruebas {
    use super::*;

    /// La salvaguarda principal, dicha en una prueba: empatar no alcanza, y
    /// quedar peor tampoco. El ancla sólo cae ante algo medido mejor.
    #[test]
    fn empatar_no_promueve_y_perder_menos() {
        assert!(!supera(10.0, 10.0), "empatar no puede promover");
        assert!(!supera(10.4, 10.0), "ganar por menos del margen tampoco");
        assert!(supera(11.0, 10.0), "ganar por más del margen sí");
    }

    /// El caso que se escribe mal solo: con mérito negativo, "mejor" es menos
    /// negativo. Multiplicar por 1.05 pediría MENOS y promovería basura.
    #[test]
    fn con_merito_negativo_mejor_sigue_siendo_mayor() {
        assert!(!supera(-10.0, -10.0));
        assert!(!supera(-10.5, -10.0), "empeorar nunca promueve");
        assert!(supera(-8.0, -10.0), "menos negativo es mejor y sí promueve");
        // Y la trampa concreta: `base * (1+MARGEN)` con base negativa da -10.5,
        // o sea que un aspirante PEOR pasaría el filtro.
        assert!(-10.0 * (1.0 + MARGEN) < -10.0);
    }

    #[test]
    fn un_golpe_arruina_la_ventana_por_mucho_progreso_que_haya() {
        let bueno = Merito { progreso: 40.0, pasos: 400, ..Default::default() };
        let con_golpe = Merito { progreso: 40.0, golpes: 1, pasos: 400, ..Default::default() };
        assert!(con_golpe.valor() < 0.0, "chocar no puede dar mérito positivo");
        assert!(bueno.valor() > con_golpe.valor());
    }

    /// Rozar sin chocar también tiene que costar: es la diferencia entre
    /// esquivar y tener suerte.
    #[test]
    fn pasar_raspando_mide_peor_que_pasar_con_aire() {
        let holgado = Merito { progreso: 20.0, riesgo: 1.0, pasos: 400, ..Default::default() };
        let raspando = Merito { progreso: 20.0, riesgo: 60.0, pasos: 400, ..Default::default() };
        assert!(holgado.valor() > raspando.valor());
    }

    /// Ventanas de distinto largo tienen que ser comparables, o el árbitro
    /// premiaría al que midió más tiempo en vez de al que voló mejor.
    #[test]
    fn el_merito_es_por_decision_no_por_ventana() {
        let corta = Merito { progreso: 10.0, pasos: 100, ..Default::default() };
        let larga = Merito { progreso: 40.0, pasos: 400, ..Default::default() };
        assert!((corta.valor() - larga.valor()).abs() < 1e-6);
    }

    #[test]
    fn sin_pasos_no_hay_merito_inventado() {
        assert_eq!(Merito::default().valor(), 0.0);
    }

    /// Cambiar de tramo mueve el objetivo de golpe. Sin tope, ese salto entra
    /// al mérito como si el dron hubiera volado setenta metros en una
    /// centésima de segundo y arruina (o regala) la ventana entera.
    #[test]
    fn un_salto_de_objetivo_no_cuenta_como_vuelo() {
        let _a_solas = a_solas();
        iafz::init(SEMILLERO, 55);
        let mut a = Adaptacion::sembrar(SEMILLERO);
        a.observar(5.0, 0.0, 5.0, 0.0, (0.0, 0.0), false);
        // El objetivo salta 70 m de una decisión a la otra.
        a.observar(75.0, 0.0, 5.0, 0.0, (0.0, 0.0), false);
        assert!(
            a.acc.progreso >= -MAX_AVANCE_POR_DECISION,
            "un salto de tramo se acreditó como retroceso real: {}",
            a.acc.progreso
        );
    }

    /// Slot de trabajo de estas pruebas, fuera de los del visor y `capacidades`.
    const SEMILLERO: i32 = 100;

    /// **Estas pruebas no se pueden correr en paralelo, y hasta hoy lo
    /// hacían.**
    ///
    /// Todas escriben en los MISMOS slots del kernel (`SEMILLERO`, `ANCLA`,
    /// `ASPIRANTE`), que son estado global del proceso, y `cargo test` corre
    /// en hilos. Así una prueba le pisaba el genoma a otra y el fallo
    /// aparecía o no según el orden del planificador: se cazó porque una
    /// prueba nueva pasaba sola y fallaba en la tanda completa. Un
    /// instrumento cuyo resultado depende de la suerte del planificador no
    /// mide nada (regla 3.18). Se serializan.
    static CANDADO: std::sync::Mutex<()> = std::sync::Mutex::new(());
    fn a_solas() -> std::sync::MutexGuard<'static, ()> {
        // Si otra prueba falló con el candado tomado, el mutex queda
        // envenenado; eso no tiene nada que ver con ésta.
        CANDADO.lock().unwrap_or_else(|e| e.into_inner())
    }

    fn bytes_del_genoma(slot: i32, nombre: &str) -> Vec<u8> {
        let ruta = std::env::temp_dir().join(nombre);
        let ruta = ruta.to_str().unwrap();
        assert!(iafz::guardar(slot, ruta), "no pude guardar el genoma de la prueba");
        std::fs::read(ruta).unwrap()
    }

    /// Corre una ventana entera empujando `dist` en la dirección pedida.
    /// `acercandose = true` da progreso positivo; `false`, negativo.
    fn correr_ventana(a: &mut Adaptacion, acercandose: bool, golpe_final: bool) {
        let mut dist = if acercandose { 100.0f32 } else { 10.0f32 };
        for i in 0..VENTANA {
            if acercandose { dist -= 0.05; } else { dist += 0.05; }
            let golpe = golpe_final && i == VENTANA - 1;
            a.observar(dist, 0.0, 5.0, 0.0, (0.0, 0.0), golpe);
        }
    }

    /// **La salvaguarda principal, medida y no argumentada**: si la variante
    /// vuela peor, el ancla tiene que quedar BIT A BIT como estaba. Es la
    /// diferencia entre "adaptarse" y "degradarse solo".
    #[test]
    fn una_variante_peor_no_toca_el_ancla() {
        let _a_solas = a_solas();
        iafz::init(SEMILLERO, 12345);
        let mut a = Adaptacion::sembrar(SEMILLERO);
        let antes = bytes_del_genoma(ANCLA, "dron_vivo_antes.adn");

        correr_ventana(&mut a, true, false); // ventana del ancla: va bien
        assert_eq!(a.fase(), Fase::Aspirante, "tras medir el ancla toca probar");
        correr_ventana(&mut a, false, true); // la variante se aleja Y se golpea

        assert_eq!(a.descartados, 1, "la variante peor tiene que descartarse");
        assert_eq!(a.promovidos, 0);
        let despues = bytes_del_genoma(ANCLA, "dron_vivo_despues.adn");
        assert_eq!(antes, despues, "el ancla se modificó pese a perder la comparación");
    }

    /// Y el otro lado: si la variante vuela mejor **en varias ventanas
    /// seguidas**, el ancla SÍ cambia. Sin esto la salvaguarda sería trivial
    /// —un sistema que nunca adopta nada también pasa la prueba de arriba— y
    /// no estaría aprendiendo nada.
    #[test]
    fn una_variante_mejor_si_reemplaza_al_ancla() {
        let _a_solas = a_solas();
        iafz::init(SEMILLERO, 999);
        let mut a = Adaptacion::sembrar(SEMILLERO);
        let antes = bytes_del_genoma(ANCLA, "dron_vivo_antes2.adn");

        correr_ventana(&mut a, false, false); // el ancla mide flojo
        for _ in 0..MUESTRAS_PARA_PROMOVER {
            correr_ventana(&mut a, true, false); // la variante mide mejor
        }

        assert_eq!(a.promovidos, 1, "una variante mejor tiene que adoptarse");
        let despues = bytes_del_genoma(ANCLA, "dron_vivo_despues2.adn");
        assert_ne!(antes, despues, "se adoptó la variante pero el ancla no cambió");
    }

    /// **Una sola ventana ganada NO asciende a nadie** (regla 3.20).
    ///
    /// Es el contrato que faltaba: el proyecto ya había pagado dos veces por
    /// aceptar con una sola muestra —el umbral de un peldaño y el ascenso
    /// fuera de línea (bitácora 7.30)— y acá, que es el que corre en vuelo
    /// sobre el dron real, seguía alcanzando con una. La prueba de arriba
    /// pasaría igual con `MUESTRAS_PARA_PROMOVER = 1`; ésta no.
    #[test]
    fn una_sola_ventana_ganada_no_asciende() {
        let _a_solas = a_solas();
        iafz::init(SEMILLERO, 4242);
        let mut a = Adaptacion::sembrar(SEMILLERO);
        let antes = bytes_del_genoma(ANCLA, "dron_vivo_antes3.adn");

        correr_ventana(&mut a, false, false); // el ancla mide flojo
        correr_ventana(&mut a, true, false);  // la variante gana UNA vez

        assert_eq!(a.promovidos, 0, "ascendió con una sola ventana ganada");
        assert_eq!(a.descartados, 0, "tampoco se descarta: sigue a prueba");
        let despues = bytes_del_genoma(ANCLA, "dron_vivo_despues3.adn");
        assert_eq!(antes, despues, "el ancla cambió sin que nada lo superara dos veces");
    }

    /// Cerca del suelo no se experimenta: la ventana en curso se anula entera y
    /// se vuelve al ancla. Media ventana juzgada es peor que ninguna.
    #[test]
    fn en_maniobra_critica_vuelve_al_ancla_y_anula_la_ventana() {
        let _a_solas = a_solas();
        iafz::init(SEMILLERO, 7);
        let mut a = Adaptacion::sembrar(SEMILLERO);
        correr_ventana(&mut a, true, false);
        assert_eq!(a.fase(), Fase::Aspirante);

        a.observar(5.0, 0.0, ALTURA_CRITICA - 0.1, 0.0, (0.0, 0.0), false);
        assert_eq!(a.fase(), Fase::Ancla, "bajando hay que volar lo conocido");
        assert_eq!(a.anuladas, 1);
        assert_eq!(a.slot(), ANCLA);
    }

    /// **La vara honesta decide, no un número elegido a mano.** Si en un
    /// nicho la red mide peor que el piloto clásico, ahí se vuela clásico —
    /// que es lo único de la mezcla de expertos de la bitácora 7.4 que sí
    /// pagó: "apagar la red donde estorba". Y se apaga SÓLO en ese nicho: en
    /// los demás sigue corrigiendo.
    #[test]
    fn donde_la_red_mide_peor_que_el_clasico_se_vuela_clasico() {
        use crate::repertorio::Contexto;
        let _a_solas = a_solas();
        iafz::init(SEMILLERO, 21);
        let mut a = Adaptacion::sembrar(SEMILLERO);
        let estorba = Contexto { estorbo: 0, tarea: 2 };
        let ayuda = Contexto { estorbo: 2, tarea: 0 };

        a.repertorio.refrescar_clasico(estorba, -0.2);
        a.repertorio.refrescar_merito(estorba, -0.9); // la red mide PEOR
        a.repertorio.refrescar_clasico(ayuda, -0.9);
        a.repertorio.refrescar_merito(ayuda, -0.2); // acá la red mide mejor

        assert_eq!(a.repertorio.red_ayuda(estorba), Some(false));
        assert_eq!(a.repertorio.red_ayuda(ayuda), Some(true));

        // Parado en el nicho donde estorba: la red se calla.
        a.observar(0.5, 0.0, 5.0, 0.0, (0.0, 0.0), false);
        assert_eq!(a.contexto(), estorba);
        // (la primera ventana de un nicho mide la vara igual, así que se
        // avanza hasta salir de la fase Clasico para probar el veredicto)
        while a.fase() == Fase::Clasico {
            a.observar(0.5, 0.0, 5.0, 0.0, (0.0, 0.0), false);
        }
        assert!(!a.usar_red(), "en un nicho donde la red mide peor tiene que callarse");
    }

    /// Y el complemento: mientras no haya veredicto, la red habla. Callarla
    /// "por las dudas" sería decidir sin medir, que es lo que el proyecto no
    /// hace.
    #[test]
    fn sin_veredicto_la_red_sigue_corrigiendo() {
        let _a_solas = a_solas();
        iafz::init(SEMILLERO, 22);
        let mut a = Adaptacion::sembrar(SEMILLERO);
        a.observar(50.0, 0.0, 5.0, 0.0, (0.0, 0.0), false);
        assert_eq!(a.repertorio.red_ayuda(a.contexto()), None);
        // En la fase de vara la red calla por diseño; en cuanto pasa a medir
        // lo conocido, vuelve a hablar.
        while a.fase() == Fase::Clasico {
            a.observar(50.0, 0.0, 5.0, 0.0, (0.0, 0.0), false);
        }
        assert!(a.usar_red());
    }

    /// Sembrar no es opcional: el ancla arranca siendo el genoma entrenado,
    /// idéntico. Adaptar desde pesos al azar no es aprender en vivo.
    #[test]
    fn sembrar_copia_el_genoma_entrenado_tal_cual() {
        let _a_solas = a_solas();
        iafz::init(SEMILLERO, 4242);
        let origen = bytes_del_genoma(SEMILLERO, "dron_vivo_origen.adn");
        let _ = Adaptacion::sembrar(SEMILLERO);
        assert_eq!(origen, bytes_del_genoma(ANCLA, "dron_vivo_sembrado.adn"));
    }
}
