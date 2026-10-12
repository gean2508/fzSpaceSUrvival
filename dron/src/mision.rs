//! Misiones: lo que de verdad tiene que hacer el aparato.
//!
//! El gym viejo (`gym3d::episodio`) entrena una sola cosa: "andá hacia ese
//! punto". Eso alcanza para medir navegación y no alcanza para nada más, y
//! encima es la raíz de la deuda #1 —el orbitar—, por una razón de incentivos
//! que conviene dejar escrita:
//!
//! **Con un solo objetivo, llegar no abre ninguna puerta.** Orbitar a 1.5 m
//! cobraba casi lo mismo que entrar, porque después de entrar no había nada
//! más que ganar. El bono de llegada era un premio suelto, y cuatro variantes
//! distintas de ese bono dieron todas peor (documentado en
//! `gym3d::Navegacion::paso`). No era un problema de calibrar el bono: era que
//! la tarea no pedía frenar.
//!
//! Acá la misión es una CADENA de puntos, y el punto siguiente sólo existe si
//! frenaste en el anterior. Orbitar deja de ser "casi igual de bueno" y pasa a
//! costar todo el resto de la misión. Cuentas del peldaño de ida y vuelta
//! (dos puntos a 7 m):
//!
//! ```text
//!   orbitar a 1.5 m del primero:  50 × (7 − 1.5)                      =  275
//!   hacer los dos:                50×7 + 300 + 50×7 + 300 + 600 + 200 = 2100
//! ```
//!
//! 7.6× de diferencia, y no hay forma de farmearla: el progreso es
//! telescópico (ir y volver suma cero) y el resto sólo se cobra llegando.
//!
//! La otra mitad de lo que agrega este archivo es que el aparato pueda
//! **tocar el suelo**: despegar y posarse son maniobras, no accidentes. Eso
//! está en `sim3d::Dron3::tocar_suelo`.

use crate::gym3d::{consignas_hibridas_punto, pasos_por_decision, sensores, Actitud};
use crate::iafz;
use crate::pid3d::{Estabilizador3, Guiado};
use crate::sim3d::{Camara, Dron3, Motores, Obstaculo, RasgosVisuales, Ruido, Sensores, Viento3, DT};

/// Un punto de la misión.
#[derive(Clone, Copy, Debug)]
pub struct Punto {
    pub x: f32,
    pub y: f32,
    pub z: f32,
    /// Radio de llegada. Entrar no alcanza: hay que entrar y QUEDARSE.
    pub radio: f32,
    /// Si acá hay que posarse en el suelo en vez de quedarse flotando.
    pub posar: bool,
    /// Qué hace el dron al cumplir este punto: agarrar una carga (el imán se
    /// prende) o soltarla. Ver `tarea_desde_texto`.
    pub accion: Accion,
}

/// Lo que el dron hace al cumplir un punto (tarea 3 del producto: un imán en
/// el dron y otro en la caja).
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Accion {
    Nada,
    /// Kilos que se suman al agarrar.
    Agarrar(f32),
    Soltar,
}

impl Punto {
    pub fn aire(x: f32, y: f32, z: f32) -> Self {
        Punto { x, y, z, radio: 1.0, posar: false, accion: Accion::Nada }
    }
    /// Una plataforma de aterrizaje: se llega al suelo, no a una altura.
    pub fn suelo(x: f32, y: f32) -> Self {
        Punto { x, y, z: 0.0, radio: 1.2, posar: true, accion: Accion::Nada }
    }
}

#[derive(Clone)]
pub struct Mision {
    pub nombre: &'static str,
    pub partida: (f32, f32, f32),
    /// Arranca posada en el suelo (y entonces la primera maniobra es despegar).
    pub desde_suelo: bool,
    pub puntos: Vec<Punto>,
    pub obstaculos: Vec<Obstaculo>,
    pub semilla: u32,
    pub pasos: usize,
}

/// Sólo para medir con y sin mapa en la misma build (`SIN_PLAN=1` al
/// arrancar el binario, ver `main`). Vuela siempre en `true`.
pub static PLAN_ACTIVO: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(true);
/// Sólo para medir con y sin la bajada por divergencia (`SIN_DIVERGENCIA=1`).
pub static BAJADA_DIVERGENCIA: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(true);
/// Sólo para medir con y sin localización por el mapa (`SIN_SLAM=1`).
pub static LOCALIZAR: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(true);

impl Mision {
    /// ¿Esta misión vuela con mapa y ruta (`plan.rs`)? Las TAREAS dichas sí;
    /// las del currículo no, a propósito: el gym mide maniobras con la vara
    /// reactiva, y agregarle un planificador cambiaría la vara de todos los
    /// peldaños (regla 3.2) sin que nada lo pidiera.
    pub fn planifica(&self) -> bool {
        self.nombre == "tarea" && PLAN_ACTIVO.load(std::sync::atomic::Ordering::Relaxed)
    }

    pub fn total(&self) -> usize {
        self.puntos.len()
    }
}

/// Qué pasó con el tramo en este paso.
#[derive(PartialEq, Clone, Copy, Debug)]
pub enum Avance {
    /// Sigue persiguiendo el mismo punto.
    Sigue,
    /// Lo completó: entró, frenó (o se posó) y se pasa al siguiente.
    Llego,
    /// Se posó donde no debía. Misión fallada.
    PosadaIndebida,
}

/// Sigue la cadena de puntos: dice cuál se persigue ahora y cuándo se pasó al
/// siguiente.
///
/// Está separado a propósito para que **el visor use exactamente esta regla**,
/// no una imitación parecida. Es la lección que el proyecto ya aprendió con
/// los postes: el gym entrenaba esquivando y el visor no tenía obstáculos, así
/// que "esquiva" no se podía ni verificar ni desmentir desde afuera. Si la
/// condición de "llegó" viviera duplicada en `web.rs`, pasaría lo mismo con
/// "entrega": se vería al dron posarse en el navegador sin ninguna garantía de
/// que eso es lo que el gym premió.
pub struct Seguidor {
    pub indice: usize,
    quieto: u32,
    /// Si ya salió del suelo. Hasta que eso pase, estar apoyado es lo normal.
    pub ya_despego: bool,
}

/// La carga que lleva el dron después de cumplir los primeros `hechos`
/// puntos de la misión. Sin estado: se recalcula del plan, así el gym y el
/// visor no pueden discrepar (regla 3.3).
pub fn carga_tras(m: &Mision, hechos: usize) -> f32 {
    let mut kg = 0.0;
    for p in m.puntos.iter().take(hechos) {
        match p.accion {
            Accion::Agarrar(k) => kg += k,
            Accion::Soltar => kg = 0.0,
            Accion::Nada => {}
        }
    }
    kg
}

/// **Una tarea que se le DICE, no que se compila** (sección 1 del contrato:
/// «un robot al que se le pueda decir una tarea que nadie entrenó»). El
/// cerebro sabe maniobras —despegar, ir, esquivar, posarse—; la tarea es sólo
/// la cadena, y se arma en runtime con este lenguaje, sin reentrenar nada:
///
/// ```text
///   despegar 3        sube a 3 m donde está
///   ir 6 0 3          vuela a (6, 0) a 3 m (la altura es opcional)
///   posar             se posa donde está (o `posar X Y`)
///   agarrar 0.3       prende el imán: 0.3 kg más (por defecto 0.2)
///   soltar            apaga el imán
///   caja 5 2 a -3 4   una entrega: la caja en (5,2) va a (-3,4); pueden ser
///                     varias seguidas y el ORDEN lo decide el dron
///                     (`plan::orden_de_entregas`: el recorrido más corto)
///   volver            vuelve a la partida y se posa
/// ```
///
/// Órdenes separadas por `;` o saltos de línea. Arranca en el suelo, en
/// `partida`.
#[cfg(test)]
pub fn tarea_desde_texto(texto: &str, partida: (f32, f32)) -> Result<Vec<Punto>, String> {
    tarea_con_mapa(texto, partida, None)
}

/// Lo mismo, pero ordenando las cajas por el largo de la RUTA sobre un mapa
/// ya conocido (lo que el dron vio en viajes anteriores), no en línea recta:
/// un destino detrás de un bosque de postes está más lejos de lo que parece.
pub fn tarea_con_mapa(texto: &str, partida: (f32, f32), mapa: Option<&crate::plan::Mapa>) -> Result<Vec<Punto>, String> {
    let mut puntos: Vec<Punto> = Vec::new();
    let (mut x, mut y) = (partida.0, partida.1);
    let mut crucero = 3.0f32;
    let mut en_suelo = true;
    let mut cajas: Vec<crate::plan::Trabajo> = Vec::new();
    let vuelve = texto.split(|c| c == ';' || c == '\n')
        .any(|o| o.split_whitespace().next() == Some("volver"));

    // Despega si hace falta: `ir` desde el piso no es una orden rara, es la
    // forma natural de decirlo.
    fn subir(puntos: &mut Vec<Punto>, en_suelo: &mut bool, x: f32, y: f32, crucero: f32) {
        if *en_suelo {
            puntos.push(Punto::aire(x, y, crucero));
            *en_suelo = false;
        }
    }
    // Las cajas se juntan y se ordenan todas juntas (`plan::orden_de_entregas`)
    // en cuanto aparece otra orden o se termina el texto.
    let vaciar = |cajas: &mut Vec<crate::plan::Trabajo>, puntos: &mut Vec<Punto>,
                      x: &mut f32, y: &mut f32, en_suelo: &mut bool, crucero: f32| {
        if cajas.is_empty() {
            return;
        }
        let por_ruta = |a: (f32, f32), b: (f32, f32)| match mapa {
            Some(m) => m.largo(a, b, crate::plan::MARGEN_RUTA),
            None => crate::plan::recta(a, b),
        };
        let orden = crate::plan::orden_de_entregas(
            (*x, *y), cajas, vuelve.then_some(partida), &por_ruta);
        for i in orden {
            let t = cajas[i];
            subir(puntos, en_suelo, *x, *y, crucero);
            puntos.push(Punto::aire(t.caja.0, t.caja.1, crucero));
            let mut p = Punto::suelo(t.caja.0, t.caja.1);
            p.accion = Accion::Agarrar(t.kg);
            puntos.push(p);
            puntos.push(Punto::aire(t.caja.0, t.caja.1, crucero));
            puntos.push(Punto::aire(t.destino.0, t.destino.1, crucero));
            let mut p = Punto::suelo(t.destino.0, t.destino.1);
            p.accion = Accion::Soltar;
            puntos.push(p);
            *x = t.destino.0;
            *y = t.destino.1;
            *en_suelo = true;
        }
        cajas.clear();
    };

    for (n, orden) in texto.split(|c| c == ';' || c == '\n').enumerate() {
        let t: Vec<&str> = orden.split_whitespace().collect();
        if t.is_empty() {
            continue;
        }
        let num = |k: usize| -> Result<f32, String> {
            t.get(k).ok_or(format!("orden {}: falta un número en «{}»", n + 1, orden.trim()))?
                .parse::<f32>().map_err(|_| format!("orden {}: «{}» no es un número", n + 1, t[k]))
        };
        if t[0] != "caja" {
            vaciar(&mut cajas, &mut puntos, &mut x, &mut y, &mut en_suelo, crucero);
        }
        match t[0] {
            "caja" => {
                // caja CX CY a DX DY [kg]
                if t.get(3) != Some(&"a") {
                    return Err(format!("orden {}: se dice «caja X Y a X Y [kg]»", n + 1));
                }
                let kg = if t.len() > 6 { num(6)? } else { 0.2 };
                if !(0.0..=1.0).contains(&kg) {
                    return Err(format!("orden {}: {kg} kg está fuera de lo que levanta un 5\" (0-1 kg)", n + 1));
                }
                cajas.push(crate::plan::Trabajo { caja: (num(1)?, num(2)?), destino: (num(4)?, num(5)?), kg });
            }
            "despegar" => {
                crucero = if t.len() > 1 { num(1)? } else { crucero };
                puntos.push(Punto::aire(x, y, crucero));
                en_suelo = false;
            }
            "ir" => {
                let (nx, ny) = (num(1)?, num(2)?);
                if t.len() > 3 {
                    crucero = num(3)?;
                }
                if crucero < 0.5 {
                    return Err(format!("orden {}: volar a menos de 0.5 m no es ir, es posarse", n + 1));
                }
                subir(&mut puntos, &mut en_suelo, x, y, crucero);
                x = nx;
                y = ny;
                puntos.push(Punto::aire(x, y, crucero));
            }
            "posar" => {
                if t.len() > 2 { x = num(1)?; y = num(2)?; }
                puntos.push(Punto::suelo(x, y));
                en_suelo = true;
            }
            "volver" => {
                subir(&mut puntos, &mut en_suelo, x, y, crucero);
                x = partida.0;
                y = partida.1;
                puntos.push(Punto::aire(x, y, crucero));
                puntos.push(Punto::suelo(x, y));
                en_suelo = true;
            }
            "agarrar" | "soltar" => {
                let ultimo = puntos.last_mut()
                    .ok_or(format!("orden {}: «{}» necesita un punto antes", n + 1, t[0]))?;
                ultimo.accion = if t[0] == "soltar" {
                    Accion::Soltar
                } else {
                    let kg = if t.len() > 1 { num(1)? } else { 0.2 };
                    if !(0.0..=1.0).contains(&kg) {
                        return Err(format!("orden {}: {kg} kg está fuera de lo que levanta un 5\" (0-1 kg)", n + 1));
                    }
                    Accion::Agarrar(kg)
                };
            }
            otra => return Err(format!(
                "orden {}: no conozco «{otra}» (despegar, ir, posar, agarrar, soltar, caja, volver)", n + 1)),
        }
    }
    vaciar(&mut cajas, &mut puntos, &mut x, &mut y, &mut en_suelo, crucero);
    if puntos.is_empty() {
        return Err("la tarea está vacía".into());
    }
    Ok(puntos)
}

/// La misión de una tarea dicha en texto.
pub fn mision_de_tarea(texto: &str, partida: (f32, f32), obstaculos: Vec<Obstaculo>) -> Result<Mision, String> {
    mision_de_tarea_con_mapa(texto, partida, obstaculos, None)
}

pub fn mision_de_tarea_con_mapa(
    texto: &str, partida: (f32, f32), obstaculos: Vec<Obstaculo>, mapa: Option<&crate::plan::Mapa>,
) -> Result<Mision, String> {
    let puntos = tarea_con_mapa(texto, partida, mapa)?;
    let pasos = 200 * (20 + 12 * puntos.len());
    Ok(Mision {
        nombre: "tarea",
        partida: (partida.0, partida.1, 0.0),
        desde_suelo: true,
        puntos,
        obstaculos,
        semilla: 4242,
        pasos,
    })
}

impl Seguidor {
    pub fn nuevo(desde_suelo: bool) -> Self {
        Seguidor { indice: 0, quieto: 0, ya_despego: !desde_suelo }
    }

    pub fn punto<'a>(&self, m: &'a Mision) -> Option<&'a Punto> {
        m.puntos.get(self.indice)
    }

    pub fn completa(&self, m: &Mision) -> bool {
        self.indice >= m.puntos.len()
    }

    pub fn paso(&mut self, m: &Mision, d: &Dron3) -> Avance {
        let Some(p) = m.puntos.get(self.indice).copied() else {
            return Avance::Sigue;
        };
        if d.z > 0.3 {
            self.ya_despego = true;
        }
        let horizontal = ((p.x - d.x).powi(2) + (p.y - d.y).powi(2)).sqrt();

        // Se posó donde no debía: o en un tramo que no pedía posada, o en uno
        // que sí la pedía pero afuera de la plataforma. Las dos cosas son la
        // misma falla —el paquete quedó en el pasto—. Sin esto, posarse corto
        // dejaba al dron sentado en el suelo quemando el reloj sin que nada lo
        // delatara.
        if d.apoyado && self.ya_despego && (!p.posar || horizontal >= p.radio) {
            return Avance::PosadaIndebida;
        }

        let listo = if p.posar {
            d.apoyado && horizontal < p.radio
        } else {
            // Llegar es ESTAR adentro del radio y estar QUIETO, pero lo que
            // reinicia la cuenta es irse del radio, no un pico de velocidad.
            //
            // Antes cualquier paso por encima del umbral volvía el contador a
            // cero, y con viento y ruido eso es casi imposible de sostener
            // medio segundo seguido: medido, el campeón se quedaba **a 0.47 m
            // del punto** —bien adentro del radio de 1 m— y marcaba 0 de 1
            // tramos. No es que no llegara; es que la vara pedía medio segundo
            // sin una sola ráfaga. Ahora los pasos quietos se acumulan mientras
            // no se salga, que es lo que de verdad quiere decir "se quedó".
            let dist = (horizontal * horizontal + (p.z - d.z).powi(2)).sqrt();
            let vel = (d.vx * d.vx + d.vy * d.vy + d.vz * d.vz).sqrt();
            if dist >= p.radio {
                self.quieto = 0;
            } else if vel < VEL_QUIETA {
                self.quieto += 1;
            }
            self.quieto >= PASOS_QUIETO
        };
        if !listo {
            return Avance::Sigue;
        }
        self.indice += 1;
        self.quieto = 0;
        if p.posar {
            // Entregó y quedó apoyado, legítimamente. El tramo que sigue es
            // despegar otra vez, así que hasta que no se despegue estar en el
            // suelo vuelve a ser lo normal y no una misión fallada.
            self.ya_despego = false;
        }
        Avance::Llego
    }
}

/// Cuánto hay que estar quieto dentro del radio para que el punto cuente.
/// 0.5 s a 200 Hz. Sin esto, "llegar" se cumple pasando de largo a 4 m/s, que
/// es exactamente lo que hacía el genoma viejo.
const PASOS_QUIETO: u32 = 100;
/// Qué tan quieto. 1.0 m/s es la cuarta parte de la velocidad de crucero: un
/// dron adentro de un metro del punto y moviéndose a menos de eso está parado
/// encima del punto para cualquier criterio razonable, y un dron pasando de
/// largo a 4 m/s no lo cumple ni de casualidad.
///
/// Era 0.6 y eso choca con la **resolución que tiene la red cerca de cero**.
/// Las salidas horizontales crudas viven en un rango de ±1.7 (medido con
/// `mision N 0 salidas`), y `consigna` reparte los 4 m/s sobre ese rango: una
/// salida residual de 0.3 —ruido para la evolución— ya pide 0.52 m/s. Con las
/// dos salidas apenas fuera de cero el módulo da 0.74 m/s y el dron no
/// calificaba como quieto por estar parado. Se veía así: el campeón terminaba
/// **a 0.86 m del punto**, bien adentro del radio de 1 m, y marcaba 0 tramos.
///
/// Que esto no relaja la vara lo garantiza otra prueba: "se queda quieto"
/// (capacidad 3) mide la deriva media en los últimos 3 segundos y es
/// independiente de este umbral.
const VEL_QUIETA: f32 = 1.0;

/// Puntos por metro acortado hacia el punto actual. Telescópico: ir y volver
/// suma exactamente cero, así que no se puede farmear dando vueltas.
const PROGRESO: f32 = 50.0;
/// Por cada punto de la cadena que se completa.
const LLEGADA: f32 = 300.0;
/// Por terminar la misión entera.
const COMPLETA: f32 = 600.0;
/// Por cada paso de tiempo que sobra al terminar. Un repartidor que tarda el
/// doble sirve la mitad; sin esto, quedarse flotando después de entregar no
/// cuesta nada.
const SOBRA_TIEMPO: f32 = 0.15;
/// Posarse donde la misión no lo pidió no rompe el aparato, pero es una misión
/// fallada: el paquete quedó en el pasto. Cuesta menos que estrellarse (-60) y
/// bastante más que nada, para que "me siento y espero" no sea una estrategia.
const POSADA_INDEBIDA: f32 = -30.0;

pub struct Informe {
    pub aptitud: f32,
    /// |Δroll| + |Δpitch| por decisión: cuánto SACUDE el mando. La aptitud no
    /// lo mira (ver CLAUDE.md 7.5), así que si no se reporta acá no se ve.
    pub sacudida: f32,
    pub puntos_hechos: usize,
    pub total_puntos: usize,
    pub completa: bool,
    pub caido: bool,
    pub posado_fuera_de_lugar: bool,
    /// Segundos que tardó (o los que duró el intento).
    pub segundos: f32,
    /// Distancia al punto que estaba persiguiendo cuando terminó.
    pub dist_final: f32,
    /// Lo más cerca que pasó de un poste. Negativo = lo rozó.
    pub roce_min: f32,
    /// Con qué velocidad vertical tocó el suelo la última vez. 0 si no tocó.
    pub vz_toque: f32,
    /// **Cuántas decisiones pasó en cada celda del repertorio.**
    ///
    /// Es el censo que contesta la pregunta que bloqueaba conectar el
    /// currículo al repertorio: *cuando una misión atraviesa varias celdas,
    /// ¿cómo se reparte el crédito?* No se reparte — se congela todo menos
    /// una celda y la diferencia de aptitud ES el crédito de esa celda. Pero
    /// para elegir QUÉ celda entrena cada peldaño hace falta saber dónde
    /// gasta el tiempo cada peldaño, y eso es un número, no una intuición
    /// (regla 3.5). Ver `crate::censo`.
    ///
    /// Se llena siempre que haya `vista` (los pilotos que usan la red), y en
    /// el clásico sólo con `censar(true)`: calcularla ahí cuesta una mirada
    /// de cámara por decisión que el clásico no usa para nada.
    pub celdas: [u32; crate::repertorio::CELDAS],
    /// El mapa que armó en vuelo (vacío si la misión no planifica).
    pub mapa: crate::plan::Mapa,
    /// Cuánto se equivocaba el dron sobre SU posición al terminar (m): la
    /// deriva de la odometría sin GPS, después de las correcciones.
    pub error_odometria: f32,
}

/// Quién vuela la misión.
#[derive(Clone, Copy)]
pub enum Piloto {
    /// El genoma, agente `id` del kernel.
    Red(i32),
    /// La vara honesta: guiado clásico sobre lecturas filtradas, el mismo de
    /// `gym3d::linea_base_guiado`. Si el genoma no le gana a esto en las
    /// misiones, hay que decirlo — para eso está.
    Clasico,
    /// **El repertorio entero**: en cada decisión vuela el genoma de la celda
    /// en la que el dron está AHORA (`repertorio::Contexto`), igual que hace
    /// `vivo.rs` en vuelo libre.
    ///
    /// `slot` es el agente del kernel donde se cargan los genomas — uno solo,
    /// no nueve: con 48 individuos por generación, nueve slots cada uno no
    /// entran en los 128 del kernel, y compartir slots congelados entre hilos
    /// sería una carrera sobre `recurrent_32[]`. En vez de eso se copia el
    /// genoma de la celda al slot propio **cuando la celda cambia** (una
    /// misión cambia de celda del orden de diez veces: 11 KB de memcpy contra
    /// 2000 pasos de física, no se nota).
    ///
    /// `entrena` es la celda que NO se trae del banco congelado: ahí se deja
    /// el genoma que el slot ya tenía, o sea el individuo que se está
    /// evaluando. Todo lo demás es idéntico para toda la población, así que
    /// cualquier diferencia de aptitud es atribuible a esa celda y a nada más
    /// — ésa es la respuesta a "cómo se reparte el crédito cuando una misión
    /// atraviesa tres celdas": no se reparte, se congela.
    ///
    /// `entrena >= CELDAS` = no se entrena ninguna: vuela el repertorio tal
    /// como está guardado. Es la forma de medirlo contra la vara.
    PorCelda { slot: i32, entrena: usize },
}

/// Una misión volada por el agente `id`.
pub fn volar(
    id: i32,
    m: &Mision,
    ruido: Ruido,
    traza: Option<&mut Vec<(f32, f32, f32)>>,
) -> Informe {
    volar_con(Piloto::Red(id), m, ruido, traza)
}

pub fn volar_con(
    piloto: Piloto,
    m: &Mision,
    ruido: Ruido,
    mut traza: Option<&mut Vec<(f32, f32, f32)>>,
) -> Informe {
    let mut d = if m.desde_suelo {
        Dron3::en_suelo(m.partida.0, m.partida.1)
    } else {
        Dron3::nuevo(m.partida.0, m.partida.1, m.partida.2).con_aterrizaje()
    };
    let mut est = Estabilizador3::nuevo();
    let guiado = Guiado::default();
    let mut viento = Viento3::nuevo(m.semilla);
    let averia = {
        let e = ESTRES.read().ok().and_then(|g| *g).unwrap_or(crate::sim3d::Averia::NINGUNA);
        let a = aparato_de(m);
        crate::sim3d::Averia {
            masa_extra: e.masa_extra + a.masa_extra,
            perdida_empuje: 1.0 - (1.0 - e.perdida_empuje) * (1.0 - a.perdida_empuje),
            viento_mult: e.viento_mult * a.viento_mult,
            torque_parasito: (e.torque_parasito.0 + a.torque_parasito.0,
                              e.torque_parasito.1 + a.torque_parasito.1,
                              e.torque_parasito.2 + a.torque_parasito.2),
        }
    };
    let medir_mezcla = MEDIR_MEZCLA.load(std::sync::atomic::Ordering::Relaxed);
    let mut sensor = Sensores::nuevo(ruido, m.semilla ^ 0xBEEF);
    let mut baliza = crate::sim3d::Baliza::nueva(m.semilla ^ 0xBA11);
    let mut camara = Camara::nueva(m.semilla ^ 0xCA11);
    // El mapa y la ruta (`plan.rs`), sólo si la misión planifica.
    let mut mapa = crate::plan::Mapa::default();
    let mut bajada = crate::gym3d::BajadaPorDivergencia::default();
    let mut divergencia_vista = 0.0f32;
    let planifica = m.planifica();
    let mut via: Option<((f32, f32), (f32, f32))> = None;
    let mut motores = Motores::nuevo();
    // Filtro pasabajos, la defensa clásica contra sensores sucios. Sólo lo usa
    // el piloto clásico: la red tiene que arreglárselas con la lectura cruda.
    let mut filtrado: Option<Dron3> = None;
    if let Piloto::Red(id) = piloto {
        iafz::reset_memoria(id);
    }
    // El repertorio (celdas + módulo vertical) lo resuelve `Volador`, el
    // mismo que usa el visor.
    let mut volador = match piloto {
        Piloto::PorCelda { slot, entrena } => Some(crate::repertorio::Volador::nuevo(
            slot, entrena, crate::repertorio::banco(), crate::repertorio::vertical(),
            crate::repertorio::posar())),
        _ => None,
    };
    let mut celdas = [0u32; crate::repertorio::CELDAS];

    let mut aptitud = 0.0f32;
    let mut actitud = Actitud::default();
    let mut seguidor = Seguidor::nuevo(m.desde_suelo);
    let mut dist_anterior: Option<f32> = None;
    let mut roce_min = f32::INFINITY;
    let mut posado_fuera_de_lugar = false;
    let mut paso_final = m.pasos;

    let (mut roll_sp, mut pitch_sp, mut yaw_sp, mut alt_sp) = (0.0, 0.0, 0.0, m.partida.2);
    // Lo último que la vara vio del anillo de proximidad. Se sostiene entre
    // pasos de decisión, igual que las salidas de la red.
    let mut anillo_clasico = [0.0f32; crate::sim3d::SECTORES];
    let mut rumbo_clasico = 0.0f32;
    let mut o_previa = [0.0f32; iafz::SALIDAS];
    let (mut sacudida, mut decisiones) = (0.0f32, 0u32);

    for paso in 0..m.pasos {
        let Some(p) = seguidor.punto(m).copied() else {
            paso_final = paso;
            break;
        };
        // `(px, py)` es el punto en el MAPA (lo que ve la cámara y lo que
        // juzga el `Seguidor`); `(ox, oy)` es dónde CREE el dron que está,
        // en su marco odométrico (sin GPS: `sim3d::Baliza`).
        let (px, py, oz) = (p.x, p.y, p.z);

        // El controlador NUNCA ve el estado real: sólo la lectura sucia.
        let medido = sensor.leer(&d);
        let (ox, oy) = baliza.ver(&d, &medido, (px, py, oz), &ruido);
        let (ox, oy) = if planifica {
            planificar(&mut mapa, &mut via, &mut camara, &mut sensor, &mut baliza, paso, &d, &medido, (ox, oy, oz), &m.obstaculos, &ruido)
        } else {
            (ox, oy)
        };
        // **El filtro pasabajos corre SIEMPRE, no sólo para el clásico.**
        //
        // Estaba adentro de la rama clásica, así que el híbrido le pasaba
        // lecturas CRUDAS a `Guiado::consignas` mientras el piloto clásico de
        // verdad usa las filtradas. La propiedad "red en cero ⟺ clásico" era
        // cierta como función pero falsa en la práctica: el híbrido partía de
        // un clásico MÁS RUIDOSO que el real, y eso se pagaba en sacudida
        // (medido: x10.81 contra el clásico, con el límite de residuo ya
        // puesto — el temblor no venía del residuo sino de la base).
        {
            let f = filtrado.get_or_insert(medido);
            let a = 0.08;
            f.x += (medido.x - f.x) * a;
            f.y += (medido.y - f.y) * a;
            f.z += (medido.z - f.z) * a;
            f.vx += (medido.vx - f.vx) * a;
            f.vy += (medido.vy - f.vy) * a;
            f.vz += (medido.vz - f.vz) * a;
            f.roll = medido.roll;
            f.pitch = medido.pitch;
            f.yaw = medido.yaw;
            f.p = medido.p;
            f.q = medido.q;
            f.r = medido.r;
        }
        let filt = filtrado.unwrap_or(medido);
        let decide = paso % pasos_por_decision() == 0;
        match piloto {
            Piloto::Red(_) | Piloto::PorCelda { .. } if decide => {
                let id = match piloto {
                    Piloto::Red(id) => id,
                    Piloto::PorCelda { slot, .. } => slot,
                    Piloto::Clasico => unreachable!(),
                };
                let vista = camara.ver_con(&d, px, py, oz, &ruido, &m.obstaculos);
                divergencia_vista = vista.divergencia;
                // **En qué celda del repertorio está volando AHORA.** Se
                // calcula con lo de a bordo —el looming del anillo y la
                // distancia ESTIMADA— exactamente igual que en vuelo libre
                // (`vivo.rs`), no con la verdad del simulador: si el gym
                // eligiera celda con información que el dron real no tiene,
                // el repertorio entrenado acá no serviría allá.
                let ctx = celda_de(&vista, &filt, ox, oy, oz);
                celdas[ctx.indice()] += 1;
                let (lo, hi) = sensores(&medido, &vista, ox, oy, oz);
                let mut o = match volador.as_mut() {
                    Some(v) => v.pensar(ctx.indice(), p.posar, lo, hi),
                    None => iafz::pensar_multi(id, lo, hi),
                };
                // MISMO límite de sacudida que en persecución, por la MISMA
                // razón y con la misma función (regla 3.3: una sola
                // implementación, o las dos tareas miden cosas distintas).
                // Acá el residuo se limita antes de `consignas_red`, que es
                // donde la red se suma al guiado.
                crate::persecucion::limitar_sacudida(
                    &mut o, &mut o_previa, crate::persecucion::tope_de_sacudida());
                // HÍBRIDO: el clásico navega, la red corrige acotada. Ver
                // `gym3d::consignas_hibridas_punto` para la medición que lo
                // justifica (el genoma estaba al 9% del clásico usando
                // `consignas_red`, que lo reemplazaba en vez de corregirlo).
                let c = consignas_hibridas_punto(&o, &filt, ox, oy, oz, &guiado, &vista);
                sacudida += (c.roll - roll_sp).abs() + (c.pitch - pitch_sp).abs();
                decisiones += 1;
                (roll_sp, pitch_sp, yaw_sp, alt_sp) = (c.roll, c.pitch, c.yaw, c.alt);
            }
            Piloto::Clasico => {
                // **La vara ahora MIRA.** La mirada de cámara ya no está
                // detrás del interruptor del censo: el clásico la usa para
                // esquivar (ver `Guiado::consignas_esquivando` para por qué
                // dejó de ser ciego y qué costaba que lo fuera). Se toma en
                // los pasos de decisión y se sostiene entre ellos, al mismo
                // ritmo con el que la red percibe: si la vara viera a 200 Hz
                // y la red a 40, la comparación mediría la tasa de sensado y
                // no el piloto.
                if decide {
                    let vista = camara.ver_con(&d, px, py, oz, &ruido, &m.obstaculos);
                    divergencia_vista = vista.divergencia;
                    if censando() {
                        celdas[celda_de(&vista, &filt, ox, oy, oz).indice()] += 1;
                    }
                    anillo_clasico = vista.anillo;
                    rumbo_clasico = vista.obs_horizontal;
                }
                let (r_ant, p_ant) = (roll_sp, pitch_sp);
                (roll_sp, pitch_sp, yaw_sp) =
                    guiado.consignas_esquivando(&filt, ox, oy, &anillo_clasico, rumbo_clasico);
                // El clásico también se mide: sin su número, "la red sacude
                // mucho" no tiene contra qué compararse.
                sacudida += (roll_sp - r_ant).abs() + (pitch_sp - p_ant).abs();
                decisiones += 1;
                // MISMA regla de aproximación que el híbrido (regla 3.3): el
                // clásico tampoco baja mientras no esté alineado sobre la
                // plataforma. Ver `gym3d::altura_de_aproximacion` para la
                // medición que lo justifica.
                alt_sp = crate::gym3d::altura_de_aproximacion(&filt, ox, oy, oz);
            }
            _ => {}
        }

        // Bajada final a divergencia constante, igual para la vara y la red
        // (regla 3.15): se pide la velocidad vertical en términos de la
        // MISMA lectura de altura que usa el estabilizador, así el ruido del
        // barómetro se cancela. La corrección de altura de la red, si la hay,
        // se conserva encima.
        let bajando = BAJADA_DIVERGENCIA.load(std::sync::atomic::Ordering::Relaxed)
            && crate::gym3d::en_bajada_final(&filt, ox, oy, oz);
        if let Some(vz) = bajada.paso(bajando, divergencia_vista) {
            let base = crate::gym3d::altura_de_aproximacion(&filt, ox, oy, oz);
            alt_sp = medido.z + vz / 1.6 + (alt_sp - base);
        }
        let pedido = est.control(&medido, alt_sp, roll_sp, pitch_sp, yaw_sp);
        let (e, tx, ty, tz) = motores.aplicar(pedido, DT);
        if medir_mezcla && !d.caido {
            use std::sync::atomic::Ordering::Relaxed;
            PASOS_TOTALES.fetch_add(1, Relaxed);
            if crate::sim3d::exceso_de_mezcla(e, tx, ty, tz) > 0.01 {
                PASOS_FUERA.fetch_add(1, Relaxed);
            }
        }
        let averia_ahora = crate::sim3d::Averia {
            masa_extra: averia.masa_extra + carga_tras(m, seguidor.indice),
            ..averia
        };
        d.paso_averiado(e, tx, ty, tz, viento.siguiente(), &averia_ahora);
        for o in &m.obstaculos {
            let dd = o.distancia(&d);
            if dd.is_finite() {
                roce_min = roce_min.min(dd);
            }
            if o.choca(&d) {
                d.caido = true;
            }
        }

        if let Some(t) = traza.as_deref_mut() {
            if paso % 10 == 0 {
                t.push((d.x, d.y, d.z));
            }
        }
        if d.caido {
            paso_final = paso;
            break;
        }

        // --- aptitud, medida sobre el estado REAL, no sobre el medido ---
        let dist = ((ox - d.x).powi(2) + (oy - d.y).powi(2) + (oz - d.z).powi(2)).sqrt();
        if let Some(anterior) = dist_anterior {
            aptitud += (anterior - dist) * PROGRESO;
        }
        dist_anterior = Some(dist);
        aptitud += actitud.paso(&d);

        // --- ¿llegó a este punto? La regla vive en `Seguidor`, que es la
        //     misma que corre el visor. ---
        match seguidor.paso(m, &d) {
            Avance::Sigue => {}
            Avance::PosadaIndebida => {
                posado_fuera_de_lugar = true;
                paso_final = paso;
                break;
            }
            Avance::Llego => {
                aptitud += LLEGADA;
                dist_anterior = None;
                if seguidor.completa(m) {
                    // Misión cumplida: cobra lo que sobró de reloj.
                    aptitud += COMPLETA + (m.pasos - paso) as f32 * SOBRA_TIEMPO;
                    paso_final = paso;
                    break;
                }
            }
        }
    }

    // El slot vuelve a ser el individuo, no la última celda que se le copió
    // encima. Si esto faltara, la evolución cruzaría y mutaría el genoma de
    // «sostén libre» creyendo que es el aspirante.
    if let Some(v) = volador.as_ref() {
        v.devolver();
    }

    let indice = seguidor.indice;
    let completa = seguidor.completa(m);
    if d.caido {
        aptitud = aptitud * 0.1 - 60.0;
    } else if posado_fuera_de_lugar {
        aptitud = aptitud * 0.1 + POSADA_INDEBIDA;
    }

    let dist_final = m
        .puntos
        .get(indice.min(m.puntos.len().saturating_sub(1)))
        .map(|p| ((p.x - d.x).powi(2) + (p.y - d.y).powi(2) + (p.z - d.z).powi(2)).sqrt())
        .unwrap_or(0.0);

    Informe {
        aptitud,
        sacudida: sacudida / decisiones.max(1) as f32,
        puntos_hechos: indice,
        total_puntos: m.puntos.len(),
        completa,
        caido: d.caido,
        posado_fuera_de_lugar,
        segundos: paso_final as f32 * DT,
        dist_final,
        roce_min,
        vz_toque: d.vz_toque,
        celdas,
        mapa,
        error_odometria: {
            let c = sensor.leer(&d);
            ((c.x - d.x).powi(2) + (c.y - d.y).powi(2)).sqrt()
        },
    }
}

/// Una decisión de la capa de ruta (`plan.rs`), compartida por el gym, el
/// informe y el visor (regla 3.3): anota en el mapa lo que la cámara ve y
/// devuelve el punto al que volar AHORA — el próximo vértice de una ruta que
/// rodea lo ya mapeado, o el objetivo mismo si el camino está libre.
///
/// `obj` está en el marco del dron (el que sale de la baliza). `via` guarda
/// el último vértice elegido y para qué objetivo: se recalcula cada 20 pasos
/// (10 Hz) o apenas el objetivo cambia. Al posarse, en el último metro y
/// medio, manda el objetivo: la ruta no tiene nada que decir encima de la
/// plataforma.
#[allow(clippy::too_many_arguments)]
pub fn planificar(
    mapa: &mut crate::plan::Mapa,
    via: &mut Option<((f32, f32), (f32, f32))>,
    camara: &mut Camara,
    sensor: &mut Sensores,
    baliza: &mut crate::sim3d::Baliza,
    paso: usize,
    real: &Dron3,
    medido: &Dron3,
    obj: (f32, f32, f32),
    obstaculos: &[Obstaculo],
    ruido: &Ruido,
) -> (f32, f32) {
    if paso % 200 == 0 && std::env::var_os("DBG_SLAM").is_some() {
        let ey = (medido.yaw - real.yaw + std::f32::consts::PI).rem_euclid(std::f32::consts::TAU) - std::f32::consts::PI;
        eprintln!("t{:5.1} err_pos {:5.2} (σ {:4.2})  err_rumbo {:+.3} (σ {:.3})  mapa {}",
            paso as f32 * DT, ((medido.x-real.x).powi(2)+(medido.y-real.y).powi(2)).sqrt(), sensor.incertidumbre(),
            ey, sensor.incertidumbre_rumbo(), mapa.obstaculos.len());
    }
    if paso % 10 == 0 {
        mapa.envejecer(10.0 * DT);
        let vistos: Vec<(f32, f32, f32, f32)> = camara.medir_postes(real, obstaculos, ruido)
            .into_iter()
            .map(|(b, dist, r)| {
                let a = b + medido.yaw;
                (medido.x + dist * a.cos(), medido.y + dist * a.sin(), r,
                 (0.15 * dist).hypot(dist * ruido.actitud).max(0.08))
            })
            .collect();
        let (mut cx, mut cy) = (0.0f32, 0.0f32);
        // **SLAM con anclas** (bitácora 7.48): lo anotado cerca de casa, al
        // arrancar, corrige al dron. Con dos o más anclas a la vista,
        // posición y rumbo juntos por triangulación; con una, sólo posición.
        if LOCALIZAR.load(std::sync::atomic::Ordering::Relaxed) {
            let tol = 2.5 * sensor.incertidumbre().max(0.3);
            let pares: Vec<((f32, f32), (f32, f32))> = vistos.iter()
                .filter_map(|&(x, y, r, _)| mapa.ancla_de(x, y, r, tol).map(|q| ((x, y), q)))
                .collect();
            let (ps, qs): (Vec<_>, Vec<_>) = pares.iter().cloned().unzip();
            let triang = crate::plan::Mapa::pose_por_anclas((medido.x, medido.y), &ps, &qs)
                // Compuertas de plausibilidad: la deriva es lenta. Un giro de
                // más de 0.3 rad o un corrimiento de más de 3 m de una vez es
                // una asociación equivocada —anclas casi simétricas alrededor
                // de casa encajan también giradas 90° o 180°—, no deriva.
                // El techo de giro escala con la duda de rumbo (si no, un error
                // ya mayor que el techo no se corrige nunca), pero nunca llega
                // a 1 rad: un encaje girado 90° es simetría, no deriva.
                .filter(|&(dx, dy, dth)| {
                    let techo_giro = (3.0 * sensor.incertidumbre_rumbo()).clamp(0.3, 1.0);
                    let techo_pos = (3.0 * sensor.incertidumbre_bruta()).clamp(3.0, 10.0);
                    dth.abs() < techo_giro && dx.hypot(dy) < techo_pos
                });
            if let Some((dx, dy, dth)) = triang {
                // Ganancia fija y chica: la triangulación se repite 20 veces
                // por segundo mientras se ven, así que converge sin saltos.
                const G: f32 = 0.2;
                sensor.corregir(dx * G, dy * G, 0.3);
                sensor.corregir_rumbo(dth * G, 0.3);
                baliza.reubicar((medido.x, medido.y), dx * G, dy * G, dth * G);
                cx += dx * G;
                cy += dy * G;
            } else if pares.len() == 1 {
                let ((x, y), _) = pares[0];
                let (_, _, r, sg) = *vistos.iter().find(|v| (v.0, v.1) == (x, y)).unwrap();
                if let Some((dx, dy, k)) = mapa.ubicar(x, y, r, sg, sensor.incertidumbre()) {
                    sensor.corregir(dx, dy, k);
                    baliza.reubicar((medido.x, medido.y), dx, dy, 0.0);
                    cx += dx;
                    cy += dy;
                }
            }
        }
        for &(x, y, r, sigma) in &vistos {
            mapa.registrar_desde(x + cx, y + cy, r, sigma, sensor.incertidumbre_bruta());
        }
    }
    let (ox, oy, oz) = obj;
    let dh = ((ox - medido.x).powi(2) + (oy - medido.y).powi(2)).sqrt();
    if oz <= 0.05 && dh < 1.5 {
        return (ox, oy);
    }
    let cambio = via.map_or(true, |(o, _)| (o.0 - ox).abs() + (o.1 - oy).abs() > 0.3);
    if cambio || paso % 20 == 0 {
        let p = mapa.proximo((medido.x, medido.y), (ox, oy), crate::plan::MARGEN_RUTA);
        *via = Some(((ox, oy), p));
    }
    via.map_or((ox, oy), |(_, p)| p)
}

/// En qué celda del repertorio cae este instante de vuelo.
///
/// Vive acá y no copiada en cada lugar que la necesita por la regla 3.3: si el
/// gym eligiera celda con una fórmula y `vivo.rs` con otra, el genoma
/// entrenado para «crucero tapado» se activaría en otro momento del vuelo y
/// nadie se enteraría. `repertorio::Contexto::de` es la fórmula; esto sólo le
/// arma las dos entradas con lo que hay a bordo.
pub fn celda_de(
    vista: &RasgosVisuales, medido: &Dron3, ox: f32, oy: f32, oz: f32,
) -> crate::repertorio::Contexto {
    let looming = vista.anillo.iter().cloned().fold(0.0f32, f32::max);
    let dist = ((ox - medido.x).powi(2) + (oy - medido.y).powi(2) + (oz - medido.z).powi(2)).sqrt();
    crate::repertorio::Contexto::de(looming, dist)
}

/// **Dónde gasta el tiempo cada peldaño, en celdas del repertorio.**
///
/// Una fila por peldaño, cada una en fracción de decisiones. Es el censo que
/// hace falta para enchufar el currículo al repertorio: sin él, decidir qué
/// celda entrena cada peldaño sería adivinar (regla 3.5).
pub fn censo_de_celdas(piloto: Piloto) -> Vec<[f32; crate::repertorio::CELDAS]> {
    let antes = censando();
    if matches!(piloto, Piloto::Clasico) {
        censar(true);
    }
    let mut tabla = Vec::with_capacity(PELDANOS);
    for p in 0..PELDANOS {
        let mut acc = [0u32; crate::repertorio::CELDAS];
        for m in &misiones(p) {
            for r in ruidos(p) {
                let inf = volar_con(piloto, m, *r, None);
                for (i, c) in inf.celdas.iter().enumerate() {
                    acc[i] += c;
                }
            }
        }
        let total = acc.iter().sum::<u32>().max(1) as f32;
        let mut frac = [0f32; crate::repertorio::CELDAS];
        for i in 0..crate::repertorio::CELDAS {
            frac[i] = acc[i] as f32 / total;
        }
        tabla.push(frac);
    }
    censar(antes);
    tabla
}

/// Cuánto tiene que crecer una celda para que el peldaño la reclame como
/// suya. Por debajo de esto el peldaño no visita ningún lugar nuevo —es la
/// misma capacidad, más difícil— y entonces hereda la celda del peldaño
/// anterior en vez de inventarse una.
///
/// 0.03 sale del censo, no del gusto (regla 3.18): los peldaños que de verdad
/// estrenan una celda la estrenan por goleada (esquivar +0.10 en «crucero
/// tapado», despegar +0.13 en «sostén libre», ir a B +0.20 en «crucero
/// libre»), y los que no estrenan nada se quedan en +0.01 o menos. Entre 0.01
/// y 0.10 no hay nada: el umbral cae en un hueco ancho, así que moverlo no
/// cambia la asignación.
const GANANCIA_CELDA_NUEVA: f32 = 0.03;

/// **Qué celda del repertorio entrena cada peldaño.**
///
/// La regla, y por qué no es el máximo a secas: el censo dice que «esquivar»
/// pasa el 56% del tiempo en «crucero libre» y sólo el 10% en «crucero
/// tapado». Tomar el máximo le daría «crucero libre», que es la celda donde
/// ya viven los seis peldaños de «ir a B» — y entonces el repertorio no
/// separaría nada. Lo que hace distinto a «esquivar» no es dónde pasa más
/// tiempo, es **dónde pasa tiempo que los peldaños anteriores no pisaban**.
/// Ese 10% es exactamente la maniobra que el peldaño viene a enseñar.
///
/// Así que cada peldaño se queda con la celda que MÁS CRECE respecto de todo
/// lo anterior, y si no crece nada en ninguna, hereda la del peldaño previo.
/// Medido con la vara honesta, la asignación que sale es:
///
/// ```text
///   peldaños  1-2   frenar, rumbo          → aproximación libre
///   peldaños  3-7   ir a B, sensores duros → crucero libre
///   peldaños  8-9   esquivar               → crucero tapado
///   peldaños 10-15  despegar, posarse, ida y vuelta, entrega → sostén libre
/// ```
///
/// Y ahí está la predicción que hace falsificable todo esto: el conflicto
/// medido de la corrida trabada —entrenar «posarse» rompía «esquivar»— pasa a
/// ser entre celdas DISTINTAS. Con «crucero tapado» congelado, sólo el 12% de
/// la misión de esquive puede cambiar mientras se entrena a posarse.
pub fn asignacion_de_celdas(censo: &[[f32; crate::repertorio::CELDAS]]) -> Vec<usize> {
    let mut asig = Vec::with_capacity(censo.len());
    let mut techo = [0f32; crate::repertorio::CELDAS];
    for (p, frac) in censo.iter().enumerate() {
        let mut mejor = (0usize, f32::NEG_INFINITY);
        for c in 0..crate::repertorio::CELDAS {
            let gana = frac[c] - techo[c];
            if gana > mejor.1 {
                mejor = (c, gana);
            }
        }
        let celda = if mejor.1 >= GANANCIA_CELDA_NUEVA || p == 0 {
            mejor.0
        } else {
            asig[p - 1]
        };
        asig.push(celda);
        for c in 0..crate::repertorio::CELDAS {
            techo[c] = techo[c].max(frac[c]);
        }
    }
    asig
}

/// Interruptor del censo de celdas del piloto clásico. Ver `Informe::celdas`.
static CENSO: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
pub fn censando() -> bool {
    CENSO.load(std::sync::atomic::Ordering::Relaxed)
}
pub fn censar(si: bool) {
    CENSO.store(si, std::sync::atomic::Ordering::Relaxed);
}

// =============================================================================
// El catálogo: la escalera de misiones
// =============================================================================

/// Peldaños del currículo, de menos a más. No es dificultad decorativa: **cada
/// peldaño agrega exactamente UNA capacidad** y las anteriores se siguen
/// exigiendo.
///
/// 0. **frenar**      — un punto a 2 m ADELANTE. Lo único nuevo es parar.
/// 1. **rumbo**       — un punto a 2 m en cualquier dirección, atrás incluida.
/// 2. **ir a B 40%**  — la ruta de verdad, acortada al 40%.
/// 3. **ir a B 70%**  — la misma, más lejos.
/// 4. **ir a B**      — distancia completa, sin postes.
/// 5. **sensores duros** — la misma ruta completa, ahora con el GPS saltando
///    0.6 m y medio segundo de retardo. Ver `ruidos`.
/// 6. **esquivar uno** — la misma ruta con UN poste en el medio.
/// 7. **esquivar**    — con los postes que haya: hasta dos por ruta.
/// 8. **despegar**    — arranca posado en el suelo y tiene que subir.
/// 9. **posarse bajo**— ya encima de la plataforma a 1.5 m: sólo tocar suave.
/// 10. **posarse**    — desde 3-4 m y con la plataforma corrida.
/// 11. **ida y vuelta**— dos puntos. Acá orbitar cuesta media misión.
/// 12. **entrega**    — la cadena de seis: despegar, cruzar, posarse,
///    despegar, volver, posarse. Es la misión que el proyecto quería hacer.
///
/// La primera versión tenía cuatro peldaños y saltaba de "2 m sin postes" a
/// "6 m con postes" de un tirón; medido, el campeón se clavaba en el peldaño 0
/// y el currículo lo promovía por aburrimiento (traza: 500 generaciones, tres
/// peldaños subidos todos con el cartel "estancado").
///
/// El desdoblamiento de **frenar** y **rumbo** salió de mirar los nueve
/// números del peldaño 0 en vez de la aptitud sola, y es el mejor ejemplo de
/// por qué `detalle` existe:
///
/// ```text
///   misión         limpio       real       duro
///   1               986.3      969.9       20.0
///   2                -7.4       -0.5       10.9     <-- objetivo DETRÁS
///   3              1057.9     1064.9     1000.5
/// ```
///
/// El campeón cumplía dos de tres con mil puntos y sacaba −7.4 en la otra
/// **con sensores perfectos**, así que no era ruido: era la única misión cuyo
/// objetivo estaba detrás del dron. El primer peldaño le estaba pidiendo girar
/// antes de enseñarle a frenar, y como la aptitud es el peor caso, las otras
/// dos mil-y-pico no movían el número ni una décima. Ahora frenar va derecho
/// adelante y girar tiene su propio escalón.
///
/// Los peldaños intermedios de **ir a B** (40%, 70% y 85%) salieron del mismo
/// método, aplicado dos veces. Con "rumbo" aprobado a 2 m, el salto directo a
/// los 6–9 m de la ruta completa dejaba al campeón clavado en 222.3 durante 40
/// generaciones: 222/50 son 4.4 m de progreso, o sea que avanzaba más de la
/// mitad del camino y se quedaba ahí. Es el mismo desierto sin gradiente que
/// el proyecto ya había encontrado en el gym del objetivo suelto: acercar el
/// objetivo y alejarlo de a poco.
///
/// El 85% se agregó después y **fue un error de método que no sirvió**; está
/// documentado en el `match` de `misiones` con la medición que lo desmiente.
/// Resumen: se decidió con una sola semilla (la sección 4 dice que eso no
/// decide nada) y se justificó con una causa razonada, no medida (regla 3.5).
/// Medido después, ese escalón se pasa en UNA generación mientras el cuello
/// real —el 70%— cuesta 648. Se deja porque sacarlo reordena la trayectoria
/// entera y no estorba, pero si algún día se rehace la escalera midiendo con
/// varias semillas, es el primero que sobra.
pub const PELDANOS: usize = 15;

/// Aptitud que sólo se puede alcanzar COMPLETANDO todas las misiones del
/// peldaño, en todos los niveles de ruido.
///
/// Sirve para que el currículo suba por aprobar y no por aburrirse. Antes se
/// subía de peldaño al estancarse, y eso tiene un modo de falla feo: un genoma
/// que no aprendió a frenar se estanca igual, sube igual, y llega a la entrega
/// completa sin saber hacer el primer tramo. Acá el umbral es `LLEGADA` por
/// punto de la cadena, que es plata que sólo se cobra llegando —el progreso
/// solo no alcanza para pagarlo— y como la aptitud es el PEOR caso, pasarlo
/// significa que completó todas.
pub fn umbral_aprobado(peldano: usize) -> f32 {
    let puntos = misiones(peldano)[0].puntos.len();
    // `LLEGADA` por punto MÁS el bono de misión cumplida: eso es exactamente
    // lo que paga una misión completa y nada más. Sin el `COMPLETA` el umbral
    // se pasaba llegando a un punto de dos y cobrando el progreso del otro.
    LLEGADA * puntos as f32 + COMPLETA
}

/// Las tres rutas base, las mismas de `gym3d::ESCENARIOS`, para que lo que se
/// mide acá sea comparable con lo que el proyecto venía midiendo.
const RUTAS: &[(f32, f32, f32, f32, f32, f32, u32)] = &[
    (0.0, 0.0, 3.0, 6.0, 0.0, 3.0, 11),
    (0.0, 0.0, 2.0, -5.0, 5.0, 5.0, 29),
    (0.0, 0.0, 4.0, 4.0, -7.0, 2.5, 47),
];

/// **Las rutas cubren TODAS las direcciones, no tres.**
///
/// Las tres rutas históricas de arriba son `+x` nivelado, `−x+y` subiendo y
/// `+x−y` bajando. Eso deja sin entrenar medio mundo: `−x` puro, `+y` puro,
/// `−y` puro, y casi todas las diagonales. Un dron que sólo practicó tres
/// rumbos no está entrenado para volar, está entrenado para esos tres rumbos —
/// es el mismo defecto que tenían los postes fijos (bitácora 7.17), en el eje
/// de la dirección en vez del eje del obstáculo.
///
/// Acá las tres rutas de cada generación salen repartidas en azimut (120° entre
/// sí, con el trío entero rotando de generación en generación) y con desnivel
/// propio, así que entre generaciones se barren los 360° del plano y las tres
/// bandas verticales: subir, bajar y nivelado. Las combinaciones dan las ocho
/// diagonales y todo lo que hay en el medio.
///
/// Dentro de UNA generación siguen siendo las mismas tres para todos los
/// individuos, igual que con los postes: si no, el campeón se elegiría por
/// haber sacado el rumbo fácil.
fn rutas() -> Vec<(f32, f32, f32, f32, f32, f32, u32)> {
    let v = variante();
    // El trío rota con la variante. `0.618` es la proporción áurea: hace que
    // las rotaciones sucesivas no se repitan en ciclos cortos, que es
    // exactamente lo que pasaría con un paso "redondo" como 0.1 vuelta.
    //
    // **La parte fraccionaria se saca en f64 y recién ahí se pasa a f32, y
    // eso NO es prolijidad: es un bug medido.** Con `(v as f32) * 0.618…`,
    // para `v` grande el producto vale ~1e10 y a esa magnitud el paso de un
    // `f32` es ~1024 — o sea que `v` y `v+1` dan el MISMO ángulo, y sumarle
    // `i·τ/3` tampoco cambia nada: las tres rutas se apilan en el mismo
    // azimut. Se cazó porque un instrumento nuevo (`dron sorteos`) reportó
    // dispersión **0.0 exacta** entre doce sorteos distintos, que es la
    // señal que la regla 3.18 manda perseguir: dos mediciones que dan el
    // mismo número dígito por dígito son la misma medición. Con variantes
    // chicas (las del entrenamiento: `semilla ^ gen`) el bug no se veía;
    // apareció apenas algo usó un `u32` grande.
    let giro = (((v as f64) * 0.618_033_988_749_894_8).fract() as f32)
        * std::f32::consts::TAU;
    (0..3)
        .map(|i| {
            let s = ((v as u64) << 20) ^ ((i as u64 + 1) << 3);
            let azimut = giro + i as f32 * std::f32::consts::TAU / 3.0;
            let largo = entre(s, 11, 5.0, 9.0);
            let z0 = entre(s, 12, 2.0, 4.5);
            // Desnivel: una ruta sube, otra baja, otra queda casi nivelada, y
            // cuál es cuál rota con la variante.
            let dz = match (v as usize + i) % 3 {
                0 => entre(s, 13, 1.0, 2.5),
                1 => entre(s, 13, -2.5, -1.0),
                _ => entre(s, 13, -0.4, 0.4),
            };
            // El destino nunca queda bajo tierra ni pegado al piso: a menos de
            // un metro el tren de aterrizaje manda y eso es la tarea de
            // posarse, que tiene sus propios peldaños.
            let z1 = (z0 + dz).clamp(1.2, 6.0);
            (
                0.0,
                0.0,
                z0,
                azimut.cos() * largo,
                azimut.sin() * largo,
                z1,
                RUTAS[i].6,
            )
        })
        .collect()
}

pub fn misiones(peldano: usize) -> Vec<Mision> {
    let todas = rutas();
    let r: &[(f32, f32, f32, f32, f32, f32, u32)] = &todas;
    match peldano.min(PELDANOS - 1) {
        // frenar: un solo punto cerca y ADELANTE (el dron arranca con la nariz
        // en +x), en el aire, sin nada que estorbe. Lo único que hay que
        // aprender acá es parar encima del punto.
        // Los dos peldaños cortos también salen de las rutas rotadas, no de
        // tres vectores escritos a mano: frenar viniendo del norte no es el
        // mismo problema que frenar viniendo del sudoeste cuesta abajo, y
        // practicar siempre los mismos tres deja los demás sin aprender.
        0 => r.iter().map(|&x| corta_hacia(x, 2.0, "frenar")).collect(),
        // rumbo: la misma distancia corta pero en cualquier dirección, atrás
        // incluida. Se puede resolver girando la nariz o trasladándose de
        // costado y para atrás — las dos salidas existen y las dos sirven.
        // `girar_media_vuelta` deja el objetivo DETRÁS: es el peldaño que
        // obliga a resolver el rumbo, no sólo a empujar hacia adelante.
        1 => r.iter().map(|&x| corta_hacia(girar_media_vuelta(x), 2.0, "rumbo")).collect(),
        // ir a B: la ruta de verdad, acercada primero y alejada después. Sin
        // estos escalones intermedios el premio de llegar queda detrás de
        // varios metros sin nada que cobrar y la población no los cruza.
        2 => r.iter().map(|&x| ida(acortar(x, 0.40), Vec::new())).collect(),
        3 => r.iter().map(|&x| ida(acortar(x, 0.70), Vec::new())).collect(),
        // Escalón del 85%: SE AGREGÓ CON MAL MÉTODO Y NO SIRVIÓ. Se deja con
        // la medición que lo desmiente, porque un intento fallido documentado
        // vale tanto como uno que salió bien (regla 3.4).
        //
        // Se agregó porque una corrida se atascó 1860 generaciones en el salto
        // de 70% a 100%. Los dos errores:
        //
        // 1. Se decidió con UNA sola semilla, y la sección 4 dice explícito
        //    que una sola corrida no decide nada porque la búsqueda es caótica.
        // 2. La causa que se escribió acá ("acoplamiento vertical+horizontal
        //    de bajar en diagonal") era una HIPÓTESIS RAZONADA, nunca medida.
        //    Regla 3.5: eso no se hace.
        //
        // Qué pasó al medirlo de verdad, corrida siguiente con el escalón
        // puesto:
        //
        // ```text
        //    70%  →  648 generaciones (gen  75 → 723)   <-- el cuello REAL
        //    85%  →    1 generación   (gen 723 → 724)   <-- este escalón
        //   100%  →  sigue costando igual que antes
        // ```
        //
        // Se pasa en una generación: no resuelve nada, el cuello estaba antes.
        // Y de paso desmintió la razón por la que se mató la corrida anterior:
        // 650 generaciones clavado **se destrabaron solas**, así que "el valor
        // no se movió en N generaciones" NO prueba convergencia.
        //
        // No se saca porque sacarlo vuelve a reordenar toda la trayectoria y
        // no hay evidencia de que estorbe — cuesta una generación. Si algún
        // día se rehace la escalera con varias semillas, éste es el primero
        // que sobra.
        4 => r.iter().map(|&x| ida(acortar(x, 0.85), Vec::new())).collect(),
        // distancia completa, todavía sin postes.
        5 => r.iter().map(|&x| ida(x, Vec::new())).collect(),
        // sensores duros: las MISMAS misiones que el peldaño anterior. Lo
        // único que cambia es el rango de ruido con el que se evalúan, que lo
        // decide `ruidos`. Aguantar un GPS que salta es una capacidad, no un
        // impuesto que se le cobra a todas las demás.
        6 => r.iter().map(|&x| {
            let mut m = ida(x, Vec::new());
            m.nombre = "sensores duros";
            m
        }).collect(),
        // esquivar UN poste. Medido: con los sets completos el campeón no
        // chocaba —pasaba a 1.49 m del poste— pero se quedaba a 3.3 m del
        // objetivo en las dos rutas de DOS postes, mientras que la de un poste
        // la dejaba a 1.39 m, casi resuelta. Otra vez dos capacidades en un
        // escalón: rodear un obstáculo, y encadenar dos rodeos seguidos.
        7 => r.iter().map(|&x| ida(x, postes_en_ruta(x, 1, variante()))).collect(),
        // esquivar todos los que haya.
        8 => r.iter().enumerate().map(|(i, &x)| ida(x, postes_de(x, i))).collect(),
        // despegar: arranca posado en el suelo.
        //
        // **Estos tres eran COORDENADAS FIJAS hasta hoy, y eso es exactamente
        // lo que la bitácora 7.28 ya había medido como la causa principal del
        // olvido** — "lo que paga es memorizar tres mapas". La aleatorización
        // se aplicó a las rutas y a los postes y **se olvidó en los peldaños
        // 9, 10 y 11**, que son justo los tres donde las corridas con el
        // criterio de aprobación honesto se quedaron trabadas (bitácora
        // 7.32). No se ve mirando el código: se ve porque `dron sorteos`
        // reporta dispersión **0.0 exacta** en las misiones propias mientras
        // el examen de repaso dispersa 28-1059 (regla 3.18).
        9 => (0..3).map(|i| despegue_variado(i)).collect(),
        // posarse desde bajo: ya está encima de la plataforma a metro y medio,
        // así que lo único nuevo es tocar el suelo despacio. Sin este escalón,
        // el peldaño de posada mezcla "bajar tres metros sin pasarse" con
        // "llegar a la plataforma" y el campeón se clavaba en 132 — 2.6 m de
        // los 3 de progreso, o sea que bajaba casi todo y llegaba al piso
        // demasiado rápido.
        // Aleatorizados por el mismo motivo que el 9. El "desde arriba del
        // punto" se conserva: lo que este escalón enseña es tocar el suelo
        // despacio, no llegar — llegar es el 11.
        10 => (0..3).map(|i| posada_variada(i, true)).collect(),
        // posarse de verdad: desde más arriba y con la plataforma corrida, así
        // que hay que llegar Y bajar.
        11 => (0..3).map(|i| posada_variada(i, false)).collect(),
        // ida y vuelta: dos puntos con postes. Acá orbitar cuesta media misión.
        12 => r.iter().enumerate().map(|(i, &x)| vuelta(x, postes_de(x, i))).collect(),
        // La entrega entera, primero SIN exigir el peor ruido — ver el
        // porqué medido en `ruidos`, `PELDANO_ENTREGA_SUAVE`: el clásico
        // falla acá a cualquier nivel de ruido (nunca esquiva, por diseño),
        // así que no es la vara la que decide este split. Lo decide el
        // campeón: la MISMA misión le sale 4185-4495 con ruido duro y se
        // estrella con ruido limpio — esquive por suerte del ruido, no
        // esquive robusto. Separar "encadenar la misión con obstáculos" de
        // "hacerlo también con los sensores rotos" baja de nueve a seis los
        // casos que tienen que salir bien a la vez, mismo remedio que ya usa
        // `ir a B`→`sensores duros` (peldaño 5→6), un escalón más arriba.
        // **Ruta larga, sin postes** (bitácora 7.43): la única que ejercita
        // la deriva sin GPS. Las demás miden 5-9 m y la deriva queda adentro
        // del radio de llegada; acá la ida y vuelta es de 20-36 m y volver a
        // la base exige usar lo que la cámara ve (`sim3d::Baliza`).
        13 => r.iter().map(|&x| larga(x)).collect(),
        // **La entrega, con rutas ALEATORIAS.** Eran tres rutas fijas
        // escritas a mano —(6,0), (−5,5), (4,−7)—, o sea lo mismo que la
        // bitácora 7.28 midió como la causa principal del olvido (una
        // memoria de coordenadas que otro peldaño pisa), olvidado justo en
        // el peldaño final.
        //
        // **La entrega con ruido DURO se sacó de la escalera** (era el 16):
        // la vara la aprobaba en 22 de 48, por los aterrizajes. DURO es «peor
        // que la realidad, a propósito» y la posada corrida ya lo excluía por
        // la misma medición (7.29). Un peldaño que la vara no aprueba mide el
        // sorteo (regla 3.7). Aterrizar con ruido duro queda abierto
        // (bitácora 7.44): tres arreglos medidos, ninguno sirvió.
        _ => r.iter().enumerate().map(|(i, &x)| entrega(x, i)).collect(),
    }
}

/// **Qué juego de obstáculos toca ahora.** Es lo que convierte "esquivar" de
/// memoria en capacidad.
///
/// El problema que resuelve, medido en la bitácora 7.17: el currículo usaba
/// SIEMPRE los tres sets fijos de `gym3d::obstaculos`, así que el campeón
/// sacaba 3/3 esquivando **los postes con los que se entrenó** y 1/3 con
/// postes en posiciones nuevas — peor que el piloto clásico, que saca 2/3. Eso
/// no es esquivar: es haberse aprendido tres mapas de memoria. Con el mismo
/// escenario repetido diez mil veces, la evolución no tiene ningún motivo para
/// aprender la regla general, y no la aprende.
///
/// El arreglo es aleatorización de dominio, lo mismo que el proyecto ya hace
/// con el RUIDO de sensores ("sensores: TODO el rango") pero que nunca se
/// aplicó a los obstáculos. La variante cambia una vez **por generación**:
/// dentro de una generación todos los individuos ven exactamente los mismos
/// postes —si no, el campeón se elegiría por suerte de sorteo y la selección
/// dejaría de medir el genoma— y entre generaciones cambian, así que
/// memorizar deja de pagar.
/// Avería inyectada a TODAS las misiones, para `dron estres`. Las averías
/// (`sim3d::Averia`) existían desde la escalera de averías (borrada), pero el gym de
/// misiones nunca las usó: el currículo entero se entrenaba con el aparato
/// sano (bitácora 7.38).
pub static ESTRES: std::sync::RwLock<Option<crate::sim3d::Averia>> = std::sync::RwLock::new(None);

/// **El aparato de cada misión no es el nominal** (bitácora 7.59).
///
/// La red corrige al clásico, y el clásico está hecho con la física EXACTA
/// del simulador: masa, motores, arrastre. Contra un modelo perfecto no hay
/// nada que corregir salvo el ruido de sensores, y así estaba midiéndose la
/// entrega. En el mundo cada vuelo es otro aparato: la batería no está igual
/// de cargada, la caja pesa otra cosa, un motor rinde menos, el centro de
/// masa no está en el centro, el viento cambia. Es donde el aprendizaje
/// residual gana en la literatura (Neural-Fly, O'Connell et al., Science
/// Robotics 2022: viento; Residual Policy Learning, Silver et al. 2018:
/// modelo mal especificado). El clásico sigue suponiendo el aparato nominal
/// —como cualquier controlador de vuelo real— y la red lo ve sólo por sus
/// sensores.
///
/// Rangos: masa −0.08 a +0.18 kg (0.90 nominal: batería más chica a caja
/// chica), 0 a 12% menos empuje (batería a media carga, hélice gastada),
/// viento ×0.6 a ×1.8, centro de masa corrido hasta 0.012 N·m (3% del torque
/// máximo). Sale de la semilla de la misión y del sorteo: todos los pilotos
/// de una generación ven el mismo aparato.
pub static FISICA_VARIABLE: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(true);

pub fn aparato_de(m: &Mision) -> crate::sim3d::Averia {
    if !FISICA_VARIABLE.load(std::sync::atomic::Ordering::Relaxed) {
        return crate::sim3d::Averia::NINGUNA;
    }
    let s = (m.semilla as u64) ^ ((variante() as u64) << 32) ^ 0xA9A7_0000;
    crate::sim3d::Averia {
        masa_extra: entre(s, 1, -0.08, 0.18),
        perdida_empuje: entre(s, 2, 0.0, 0.12),
        viento_mult: entre(s, 3, 0.6, 1.8),
        torque_parasito: (entre(s, 4, -0.012, 0.012), entre(s, 5, -0.012, 0.012), 0.0),
    }
}
/// Pasos de física y pasos cuyo pedido no cabe en cuatro motores (exceso >1%).
pub static PASOS_TOTALES: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
pub static PASOS_FUERA: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
pub static MEDIR_MEZCLA: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

static VARIANTE: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);

pub fn variante() -> u32 {
    VARIANTE.load(std::sync::atomic::Ordering::Relaxed)
        .wrapping_add(SORTEO_EXTRA.with(|c| c.get()))
}

thread_local! {
    /// Corrimiento del sorteo DENTRO de una evaluación, por hilo.
    ///
    /// Tiene que ser `thread_local` y no otro atómico global: la población se
    /// evalúa en hilos, y si un hilo cambiara la variante global le movería el
    /// escenario a los otros en medio de su medición. Ese bug no se vería en
    /// el número —todos siguen sacando algo— pero cada individuo quedaría
    /// medido en un escenario distinto, que es justo lo que la aptitud no
    /// puede permitirse.
    static SORTEO_EXTRA: std::cell::Cell<u32> = const { std::cell::Cell::new(0) };
}

/// **Cuántos juegos de escenarios INDEPENDIENTES ve cada individuo por
/// generación.**
///
/// Uno (el valor histórico) significa que la nota de toda la población sale de
/// un solo sorteo, y entonces el campeón de la generación es, en parte, el que
/// tuvo el sorteo más benévolo: con 64 individuos, tomar el máximo de sus
/// mínimos elige al afortunado (la *maldición del ganador*). Eso es la regla
/// 3.20 un nivel más abajo que la aprobación — no en qué se acepta, sino en
/// qué se SELECCIONA.
///
/// Cuesta lineal, así que no se sube a ciegas: se mide. Ver bitácora 7.32.
static SORTEOS: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(1);

pub fn sorteos() -> u32 {
    SORTEOS.load(std::sync::atomic::Ordering::Relaxed).max(1)
}

pub fn fijar_sorteos(n: u32) {
    SORTEOS.store(n.max(1), std::sync::atomic::Ordering::Relaxed);
}

/// La fija el entrenador una vez por generación, y los informes cuando quieren
/// un escenario reproducible.
/// El candado de los escenarios para las PRUEBAS. Ver
/// `pruebas::escenarios_para_mi` para la medición que lo obliga.
#[cfg(test)]
static ESCENARIOS_EN_USO: std::sync::Mutex<()> = std::sync::Mutex::new(());

/// Lo toma cualquier prueba de cualquier módulo que vuele misiones.
///
/// `unwrap_or_else` y no `unwrap`: si otra prueba entró en pánico con el
/// candado tomado, el veneno no tiene que convertir un fallo en doce que
/// tapan cuál fue el original.
#[cfg(test)]
pub(crate) fn escenarios_para_pruebas() -> std::sync::MutexGuard<'static, ()> {
    ESCENARIOS_EN_USO.lock().unwrap_or_else(|e| e.into_inner())
}

pub fn fijar_variante(v: u32) {
    VARIANTE.store(v, std::sync::atomic::Ordering::Relaxed);
}

/// Revoltijo determinista (splitmix64). Misma entrada, misma salida, siempre:
/// la aptitud tiene que seguir siendo función del genoma y la variante, no del
/// orden en que los hilos llegaron.
fn mezclar(mut x: u64) -> u64 {
    x = x.wrapping_add(0x9E3779B97F4A7C15);
    let mut z = x;
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58476D1CE4E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D049BB133111EB);
    z ^ (z >> 31)
}

/// Un número en [a, b] a partir de la semilla y un índice de campo.
fn entre(semilla: u64, campo: u64, a: f32, b: f32) -> f32 {
    let u = (mezclar(semilla ^ (campo.wrapping_mul(0x517C_C1B7_2722_0A95))) >> 11) as f32
        / (1u64 << 53) as f32;
    a + (b - a) * u
}

/// Postes puestos **en el camino**, no en coordenadas fijas.
///
/// Se colocan a lo largo del segmento partida→objetivo con un desvío lateral,
/// porque un poste que no estorba no enseña nada. Los márgenes no son de
/// adorno: la biblia ya lo tenía escrito —"un poste puesto justo donde ya está
/// el dron no da tiempo de reacción a nadie, es física, no un bug"—, así que
/// se reservan `MARGEN_PARTIDA` metros para ver y reaccionar, y
/// `MARGEN_OBJETIVO` para que frenar encima del punto no sea chocar.
///
/// Si la ruta es más corta que los dos márgenes juntos (los peldaños de "ir a
/// B" acortado al 40% lo son), devuelve menos postes o ninguno: es preferible
/// un peldaño sin obstáculo a uno imposible.
fn postes_en_ruta(
    r: (f32, f32, f32, f32, f32, f32, u32),
    cuantos: usize,
    variante: u32,
) -> Vec<Obstaculo> {
    // **Simétricos, 3.0 y 3.0** (eran 2.5 y 1.5). En «ida y vuelta» y
    // «entrega» cada tramo se recorre en los dos sentidos, así que el margen
    // del OBJETIVO es el margen de PARTIDA de la vuelta: con 1.5 m la vuelta
    // arrancaba desde un hover con el poste a 1.4 m de la superficie, y un
    // sensor de looming no ve nada estando quieto. Clásico, 24 sorteos
    // (bitácora 7.36):
    //
    // ```text
    //   márgenes    peldaño 8   peldaño 9   peldaño 13
    //   2.5 / 1.5     20          15            0
    //   2.5 / 2.5     17          13            3
    //   3.0 / 3.0     22          16            6
    // ```
    const MARGEN_PARTIDA: f32 = 3.0;
    const MARGEN_OBJETIVO: f32 = 3.0;
    let (x0, y0, _, bx, by, _, sal) = r;
    let (dx, dy) = (bx - x0, by - y0);
    let largo = (dx * dx + dy * dy).sqrt();
    if largo < MARGEN_PARTIDA + MARGEN_OBJETIVO {
        return Vec::new();
    }
    // **Separación mínima entre postes a lo largo de la ruta.** Con los
    // márgenes de 3 m, en una ruta de 7 m los dos postes caían en una franja
    // de 1 m y armaban una PARED con una rendija de 0.25 m (sorteo 3, misión
    // 2 del peldaño 14: postes a 0.76 m uno del otro sobre la ruta, 1.08 m de
    // costado). Eso no es «encadenar dos rodeos», que es lo que el peldaño
    // dice medir: es un escenario distinto que el generador fabricaba por
    // accidente. Si no entran separados, va uno solo (bitácora 7.43).
    const SEPARACION_MIN: f32 = 2.5;
    let franja_total = largo - MARGEN_PARTIDA - MARGEN_OBJETIVO;
    let cuantos = cuantos.min(1 + (franja_total / SEPARACION_MIN) as usize);
    let (ux, uy) = (dx / largo, dy / largo);
    // Perpendicular unitaria, para desviar el poste del eje exacto.
    let (px, py) = (-uy, ux);
    let t_min = MARGEN_PARTIDA / largo;
    let t_max = 1.0 - MARGEN_OBJETIVO / largo;

    let base = (variante as u64) << 32 | sal as u64;
    let mut v = Vec::with_capacity(cuantos);
    for k in 0..cuantos {
        let s = base ^ ((k as u64 + 1) << 8);
        // Repartidos: cada poste vive en su propia franja del camino, así que
        // dos postes no se apilan en el mismo metro y el escenario de "dos
        // postes" sigue siendo el de encadenar dos rodeos.
        let franja = (t_max - t_min) / cuantos as f32;
        let t = t_min + franja * (k as f32 + entre(s, 1, 0.15, 0.85));
        let lateral = entre(s, 2, -1.3, 1.3);
        v.push(Obstaculo {
            x: x0 + ux * (t * largo) + px * lateral,
            y: y0 + uy * (t * largo) + py * lateral,
            radio: entre(s, 3, 0.4, 0.8),
            alto: entre(s, 4, 3.0, 8.0),
        });
    }
    v
}

/// Cuántos postes lleva cada ruta. Se conserva el reparto histórico (la ruta 0
/// con uno, las otras dos con dos) para que los peldaños sigan midiendo lo
/// mismo que medían: rodear uno, y encadenar dos rodeos.
fn cuantos_postes(cual: usize) -> usize {
    if cual % 3 == 0 { 1 } else { 2 }
}

fn postes_de(r: (f32, f32, f32, f32, f32, f32, u32), cual: usize) -> Vec<Obstaculo> {
    postes_en_ruta(r, cuantos_postes(cual), variante())
}

/// Una misión corta EN LA DIRECCIÓN de la ruta: mismo rumbo y mismo desnivel,
/// pero a `metros` de la partida. Así los peldaños de frenar y rumbo heredan
/// la cobertura de direcciones de `rutas()` en vez de tener tres vectores
/// propios escritos a mano.
fn corta_hacia(
    r: (f32, f32, f32, f32, f32, f32, u32),
    metros: f32,
    nombre: &'static str,
) -> Mision {
    let (x0, y0, z0, bx, by, bz, s) = r;
    let (dx, dy) = (bx - x0, by - y0);
    let largo = (dx * dx + dy * dy).sqrt().max(0.001);
    let f = metros / largo;
    Mision {
        nombre,
        partida: (x0, y0, z0),
        desde_suelo: false,
        // El desnivel se escala igual que la distancia: un tramo de dos metros
        // no puede pedir los tres metros de subida de la ruta entera.
        puntos: vec![Punto::aire(x0 + dx * f, y0 + dy * f, z0 + (bz - z0) * f)],
        obstaculos: Vec::new(),
        semilla: s,
        // 8 s alcanzan de sobra para 2 m: el peldaño mide si FRENA, no si
        // aguanta. Episodios cortos acá es lo que hace barato el currículo.
        pasos: 1600,
    }
}

/// La misma ruta con el destino del otro lado de la partida. Es lo que hace
/// que el peldaño de rumbo tenga el objetivo DETRÁS del dron, que arranca con
/// la nariz en +x.
fn girar_media_vuelta(
    r: (f32, f32, f32, f32, f32, f32, u32),
) -> (f32, f32, f32, f32, f32, f32, u32) {
    let (x0, y0, z0, bx, by, bz, s) = r;
    (x0, y0, z0, x0 - (bx - x0), y0 - (by - y0), bz, s)
}

/// La misma ruta con el objetivo traído hacia la partida. `f` = 1.0 la deja
/// como está; 0.4 pone el punto al 40% del camino.
fn acortar(
    r: (f32, f32, f32, f32, f32, f32, u32),
    f: f32,
) -> (f32, f32, f32, f32, f32, f32, u32) {
    let (x0, y0, z0, bx, by, bz, s) = r;
    (x0, y0, z0, x0 + (bx - x0) * f, y0 + (by - y0) * f, z0 + (bz - z0) * f, s)
}

/// Una sola pierna, un solo poste, sin las otras cinco maniobras alrededor.
/// Para entrenar esquivar AISLADO (`repertorio::refinar_nicho`): la misión
/// compuesta (`entrega`) diluye esta capacidad contra otras cinco a la vez, y
/// eso es justamente lo que se quiere evitar acá. El poste incluye desvíos
/// CASI CERO a propósito —`lateral` llega a ±0.15 m, no ±1.3 como en
/// `postes_en_ruta`— porque un poste centrado en el camino es el caso que
/// midió peor en vivo (bitácora 7.29/7.30): dead-center es justo lo que la
/// aleatorización ancha rara vez produce.
pub fn mision_esquive_aislado(semilla: u64) -> Mision {
    let largo = entre(semilla, 21, 6.0, 10.0);
    let z = entre(semilla, 22, 2.0, 4.0);
    let lateral = entre(semilla, 23, -0.6, 0.6);
    let t = entre(semilla, 24, 0.35, 0.65);
    let radio = entre(semilla, 25, 0.4, 0.8);
    let alto = entre(semilla, 26, 3.0, 8.0);
    Mision {
        nombre: "esquivar aislado",
        partida: (0.0, 0.0, z),
        desde_suelo: false,
        puntos: vec![Punto::aire(largo, 0.0, z)],
        obstaculos: vec![Obstaculo { x: largo * t, y: lateral, radio, alto }],
        semilla: (semilla & 0xFFFF_FFFF) as u32,
        pasos: 2400,
    }
}

fn ida(r: (f32, f32, f32, f32, f32, f32, u32), obs: Vec<Obstaculo>) -> Mision {
    Mision {
        nombre: if obs.is_empty() { "ir a B" } else { "esquivar" },
        partida: (r.0, r.1, r.2),
        desde_suelo: false,
        puntos: vec![Punto::aire(r.3, r.4, r.5)],
        obstaculos: obs,
        semilla: r.6,
        pasos: 2400,
    }
}

fn despegue(x: f32, y: f32, alto: f32, semilla: u32) -> Mision {
    Mision {
        nombre: "despegar",
        partida: (x, y, 0.0),
        desde_suelo: true,
        puntos: vec![Punto::aire(x, y, alto)],
        obstaculos: Vec::new(),
        semilla,
        pasos: 1600,
    }
}

/// Un despegue sorteado: dónde está posado y a qué altura tiene que subir.
///
/// Los rangos son los que cubrían las tres misiones fijas que reemplaza
/// —altura 2 a 4 m, partida dentro de unos pocos metros del origen— abiertos
/// un poco para que entre lo que había en el medio y no sólo los tres puntos
/// que estaban escritos. El `i` mantiene las tres misiones distintas ENTRE SÍ
/// dentro de una misma generación, igual que hace `rutas`.
fn despegue_variado(i: usize) -> Mision {
    let s = ((variante() as u64) << 20) ^ ((i as u64 + 1) << 5) ^ 0xDE5;
    let ang = entre(s, 1, 0.0, std::f32::consts::TAU);
    let rad = entre(s, 2, 0.0, 3.5);
    despegue(
        rad * ang.cos(),
        rad * ang.sin(),
        entre(s, 3, 1.8, 4.5),
        (s & 0xFFFF_FFFF) as u32,
    )
}

/// Una posada sorteada. Con `encima`, la plataforma queda debajo del dron y lo
/// único que hay que aprender es bajar despacio (peldaño 10); sin él, la
/// plataforma está corrida y hay que llegar Y bajar (peldaño 11).
fn posada_variada(i: usize, encima: bool) -> Mision {
    let s = ((variante() as u64) << 20) ^ ((i as u64 + 1) << 7) ^ 0x805;
    let ang = entre(s, 1, 0.0, std::f32::consts::TAU);
    let rad = entre(s, 2, 0.0, 2.0);
    let (x0, y0) = (rad * ang.cos(), rad * ang.sin());
    let z0 = if encima { entre(s, 3, 1.2, 1.8) } else { entre(s, 3, 2.3, 4.2) };
    let (px, py) = if encima {
        (x0, y0)
    } else {
        let a = entre(s, 4, 0.0, std::f32::consts::TAU);
        let d = entre(s, 5, 1.2, 3.0);
        (x0 + d * a.cos(), y0 + d * a.sin())
    };
    posada(x0, y0, z0, px, py, (s & 0xFFFF_FFFF) as u32)
}

fn posada(x0: f32, y0: f32, z0: f32, px: f32, py: f32, semilla: u32) -> Mision {
    Mision {
        nombre: "posarse",
        partida: (x0, y0, z0),
        desde_suelo: false,
        puntos: vec![Punto::suelo(px, py)],
        obstaculos: Vec::new(),
        semilla,
        pasos: 2400,
    }
}

/// Ida y vuelta de 4× el largo de la ruta (20-36 m), sin postes.
fn larga(r: (f32, f32, f32, f32, f32, f32, u32)) -> Mision {
    let (x0, y0, z0, bx, by, bz, s) = r;
    let lejos = (x0 + (bx - x0) * 4.0, y0 + (by - y0) * 4.0);
    let largo = ((lejos.0 - x0).powi(2) + (lejos.1 - y0).powi(2)).sqrt();
    Mision {
        nombre: "ruta larga",
        partida: (x0, y0, z0),
        desde_suelo: false,
        puntos: vec![Punto::aire(lejos.0, lejos.1, bz), Punto::aire(x0, y0, z0)],
        obstaculos: Vec::new(),
        semilla: s,
        // Ida y vuelta a ~3 m/s de crucero más 12 s de margen.
        pasos: ((2.0 * largo / 3.0 + 12.0) * 200.0) as usize,
    }
}

fn vuelta(r: (f32, f32, f32, f32, f32, f32, u32), obs: Vec<Obstaculo>) -> Mision {
    Mision {
        nombre: "ida y vuelta",
        partida: (r.0, r.1, r.2),
        desde_suelo: false,
        puntos: vec![Punto::aire(r.3, r.4, r.5), Punto::aire(r.0, r.1, r.2)],
        obstaculos: obs,
        semilla: r.6,
        pasos: 4400,
    }
}

/// La misión de verdad: entregar un paquete y volver a la base.
///
/// Seis maniobras encadenadas, cada una de las cuales el proyecto listaba por
/// separado: despegar, ir a B, posarse (entregar), despegar otra vez, volver a
/// A, posarse. Con postes en el medio.
fn entrega(r: (f32, f32, f32, f32, f32, f32, u32), obs: usize) -> Mision {
    let (bx, by, semilla) = (r.3, r.4, r.6);
    let crucero = r.2.clamp(2.5, 4.5);
    Mision {
        nombre: "entrega",
        partida: (0.0, 0.0, 0.0),
        desde_suelo: true,
        puntos: vec![
            Punto::aire(0.0, 0.0, crucero),     // despegar
            Punto::aire(bx, by, crucero),       // cruzar hasta B
            Punto::suelo(bx, by),               // posarse: entregar
            Punto::aire(bx, by, crucero),       // despegar de nuevo
            Punto::aire(0.0, 0.0, crucero),     // volver a A
            Punto::suelo(0.0, 0.0),             // posarse en la base
        ],
        obstaculos: postes_de((0.0, 0.0, crucero, bx, by, crucero, semilla), obs),
        semilla,
        pasos: 9000, // 45 s
    }
}

/// Los NUEVE números que hay detrás de la aptitud de un peldaño: tres misiones
/// por tres niveles de ruido.
///
/// Existe por la trampa que la biblia ya documenta —"la aptitud es el PEOR
/// caso"—: un campeón clavado en el mismo número durante cuarenta
/// generaciones casi nunca está estancado en todo, está estancado en UN caso
/// que define el mínimo mientras los otros ocho mejoran sin que se note. Sin
/// esta tabla eso es invisible.
/// Qué está EMITIENDO el cerebro, no qué hace el dron.
///
/// Las cuatro salidas del kernel son sumas de pesos flotantes sobre las
/// neuronas encendidas, y después pasan por `tanh(o·0.25)`. Si el |o| típico
/// cae en la zona saturada de esa tanh, la red es un interruptor de tres
/// posiciones (todo a un lado, todo al otro) y no puede pedir "un poquito" de
/// nada — que es justo lo que hace falta para frenar encima de un punto. Esta
/// tabla dice si eso está pasando, en vez de dejarlo como sospecha.
pub fn salidas(id: i32, peldano: usize) {
    let ms = misiones(peldano);
    let m = &ms[0];
    let mut d = Dron3::nuevo(m.partida.0, m.partida.1, m.partida.2).con_aterrizaje();
    let mut est = Estabilizador3::nuevo();
    let guiado = Guiado::default();
    let mut viento = Viento3::nuevo(m.semilla);
    let mut sensor = Sensores::nuevo(Ruido::REAL, m.semilla ^ 0xBEEF);
    let mut baliza = crate::sim3d::Baliza::nueva(m.semilla ^ 0xBA11);
    let mut camara = Camara::nueva(m.semilla ^ 0xCA11);
    let mut motores = Motores::nuevo();
    iafz::reset_memoria(id);
    let p = m.puntos[0];

    let mut crudas = [Vec::new(), Vec::new(), Vec::new(), Vec::new()];
    let (mut roll_sp, mut pitch_sp, mut yaw_sp, mut alt_sp) = (0.0, 0.0, 0.0, p.z);
    for paso in 0..m.pasos {
        let medido = sensor.leer(&d);
        let (bx, by) = baliza.ver(&d, &medido, (p.x, p.y, p.z), &Ruido::REAL);
        if paso % pasos_por_decision() == 0 {
            let vista = camara.ver_con(&d, p.x, p.y, p.z, &Ruido::REAL, &m.obstaculos);
            let (lo, hi) = sensores(&medido, &vista, bx, by, p.z);
            let o = iafz::pensar_multi(id, lo, hi);
            for k in 0..4 {
                crudas[k].push(o[k]);
            }
            let c = consignas_hibridas_punto(&o, &medido, bx, by, p.z, &guiado, &vista);
            (roll_sp, pitch_sp, yaw_sp, alt_sp) = (c.roll, c.pitch, c.yaw, c.alt);
        }
        let pedido = est.control(&medido, alt_sp, roll_sp, pitch_sp, yaw_sp);
        let (e, tx, ty, tz) = motores.aplicar(pedido, DT);
        d.paso(e, tx, ty, tz, viento.siguiente());
        if d.caido {
            break;
        }
    }

    println!("SALIDAS CRUDAS del cerebro — misión «{}», {} decisiones\n", m.nombre, crudas[0].len());
    println!("   {:<22} {:>8} {:>8} {:>8} {:>10}", "salida", "media", "mín", "máx", "saturada");
    for (k, nombre) in ["v adelante", "v lateral", "guiñada", "altura"].iter().enumerate() {
        let v = &crudas[k];
        if v.is_empty() {
            continue;
        }
        let media = v.iter().sum::<f32>() / v.len() as f32;
        let min = v.iter().cloned().fold(f32::INFINITY, f32::min);
        let max = v.iter().cloned().fold(f32::NEG_INFINITY, f32::max);
        // "Saturada" = |tanh(o·0.25)| > 0.9, o sea |o| > 5.9: ahí un cambio
        // en la salida del cerebro ya casi no mueve la consigna.
        let sat = v.iter().filter(|o| crate::gym3d::consigna(**o, 1.0).abs() > 0.9).count();
        println!("   {nombre:<22} {media:>8.2} {min:>8.2} {max:>8.2} {:>9.0}%",
                 sat as f32 / v.len() as f32 * 100.0);
    }
    println!("\n   media lejos de cero = sesgo: la red pide algo aunque no haga falta.");
    println!("   saturada alto = interruptor de tres posiciones, no control fino.");
}

/// Por qué está trabado el campeón, y si al pelear con este peldaño está
/// **rompiendo los que ya había aprobado**.
///
/// Existe porque avisar "TRABADO, necesita 900 y va 222.8" dice QUÉ pasa y no
/// POR QUÉ, que es justo lo que hace falta para destrabarlo. Las tres
/// preguntas que contesta, en orden de utilidad:
///
/// 1. **¿Cuál de los nueve casos define el mínimo?** Casi nunca están los
///    nueve mal; hay uno que arrastra al resto y es el único que importa.
/// 2. **¿Cómo falla ese caso?** No es lo mismo estrellarse contra un poste
///    (problema de percepción o de maniobra) que quedarse sin reloj a 3 m
///    (problema de navegación) o posarse fuera de la plataforma (problema de
///    precisión). Cada uno se ataca distinto.
/// 3. **¿Está desaprendiendo lo de atrás?** Olvido catastrófico: el genoma
///    puede estar perdiendo el peldaño 3 mientras pelea con el 7, y sin
///    medirlo nadie se entera. Se re-evalúan TODOS los peldaños ya aprobados
///    y se marca el que dejó de aprobar.
pub fn telemetria_estancado(piloto: Piloto, peldano: usize) {
    let ms = misiones(peldano);
    let umbral = umbral_aprobado(peldano);

    // --- 1 y 2: cuál caso arrastra el mínimo, y cómo falla ---
    let mut peor: Option<(f32, usize, Ruido, Informe)> = None;
    for (i, m) in ms.iter().enumerate() {
        for r in ruidos(peldano) {
            let inf = volar_con(piloto, m, *r, None);
            if peor.as_ref().map_or(true, |(a, _, _, _)| inf.aptitud < *a) {
                peor = Some((inf.aptitud, i, *r, inf));
            }
        }
    }
    let Some((apt, i_peor, ruido_peor, inf)) = peor else { return };
    let nombre_ruido = if ruido_peor.posicion == 0.0 { "limpio" }
                       else if ruido_peor.posicion < 0.4 { "real" } else { "DURO" };

    println!("      ┌ el mínimo lo define la misión {} con ruido {} ({:.1} de {:.0})",
             i_peor + 1, nombre_ruido, apt, umbral);
    println!("      │ termina: {} — {}/{} tramos, queda a {:.2} m",
             final_del_vuelo(&inf), inf.puntos_hechos, inf.total_puntos, inf.dist_final);
    if inf.roce_min.is_finite() {
        println!("      │ poste más cercano: {:.2} m {}", inf.roce_min,
                 if inf.roce_min <= 0.0 { "← LO CHOCÓ" } else { "(no lo tocó)" });
    }
    // La diferencia entre "no sabe" y "no llega a tiempo" cambia el arreglo.
    if !inf.caido && !inf.completa && !inf.posado_fuera_de_lugar {
        println!("      │ no se estrelló: es navegación o reloj, no percepción");
    }

    // --- 2.5: el examen de repaso, que `aptitud()` SÍ mira y esta telemetría
    // no mostraba. Bug real, encontrado midiendo: acá decía "el mínimo lo
    // define la misión 3 (49.2 de 900)" mientras el entrenador reportaba
    // "mejor 10.8" — dos números que no cuadraban porque `aptitud()` es
    // `min(misiones propias, examen_de_repaso)` y esto sólo mostraba el
    // primer término. Con 11 peldaños desaprendidos, el repaso era el que de
    // verdad definía el mínimo, y el diagnóstico apuntaba al lugar
    // equivocado — el mismo bug que ya se había encontrado y arreglado en
    // `persecucion::telemetria_estancado` esta sesión, acá sin arreglar
    // todavía.
    if peldano > 0 {
        // El número real: llamando a la MISMA función que usa `aptitud()`,
        // no una copia — así no puede volver a divergir.
        let peor_repaso = examen_de_repaso(piloto, peldano);
        // Y de qué peldaño viejo sale, sólo para el mensaje (no para el
        // número: si esto se rompiera, `peor_repaso` de arriba seguiría
        // siendo el correcto).
        let umbral_hoy = umbral_aprobado(peldano);
        let mut de_donde = 0usize;
        let mut mejor_visto = f32::INFINITY;
        for p in 0..peldano {
            for m in &misiones(p) {
                for r in ruidos(p) {
                    let escalado = volar_con(piloto, m, *r, None).aptitud / umbral_aprobado(p) * umbral_hoy;
                    if escalado < mejor_visto {
                        mejor_visto = escalado;
                        de_donde = p;
                    }
                }
            }
        }
        println!("      ├ examen de repaso: peor caso «{}» escalado a {:.1}{}",
                 misiones(de_donde)[0].nombre, peor_repaso,
                 if peor_repaso < apt { " ← ESTE es el que define el mínimo real" } else { "" });
    }

    // --- 3: ¿está rompiendo lo que ya sabía? ---
    if peldano > 0 {
        let mut roto = Vec::new();
        for p in 0..peldano {
            if aptitud(piloto, p, true, Ruido::REAL) < umbral_aprobado(p) {
                roto.push(p + 1);
            }
        }
        if roto.is_empty() {
            println!("      └ los {peldano} peldaños de atrás siguen aprobados (no está desaprendiendo)");
        } else {
            println!("      └ ⚠ DESAPRENDIÓ los peldaños {roto:?} — está rompiendo lo de atrás \
                      para pelear éste");
        }
    } else {
        println!("      └ es el primer peldaño: no hay nada atrás que romper");
    }
}

pub fn detalle(piloto: Piloto, peldano: usize) {
    let ms = misiones(peldano);
    println!("PELDAÑO {} — «{}»   ·   umbral de aprobado {:.0}\n",
             peldano + 1, ms[0].nombre, umbral_aprobado(peldano));
    // Se imprimen los tres niveles siempre, pero sólo cuentan para el mínimo
    // los que el peldaño de verdad exige (`ruidos`). Sin la marca, la tabla
    // haría creer que el ruido duro está trabando un peldaño que todavía no lo
    // pide.
    let cuentan = ruidos(peldano);
    let marca = |r: Ruido| if cuentan.iter().any(|c| c.posicion == r.posicion) { "" } else { " (no cuenta)" };
    println!("   sensores que exige este peldaño: limpio, real{}\n",
             if cuentan.len() > 2 { " y DURO" } else { marca(Ruido::DURO) });
    println!("   {:<10} {:>10} {:>10} {:>10}", "misión", "limpio", "real", "duro");
    let mut peor = f32::INFINITY;
    for (i, m) in ms.iter().enumerate() {
        let mut fila = String::new();
        for r in [Ruido::LIMPIO, Ruido::REAL, Ruido::DURO] {
            let inf = volar_con(piloto, m, r, None);
            if cuentan.iter().any(|c| c.posicion == r.posicion) {
                peor = peor.min(inf.aptitud);
            }
            fila.push_str(&format!("{:>10.1}", inf.aptitud));
        }
        // La segunda línea dice QUÉ pasó, que es lo que el número no cuenta.
        let inf = volar_con(piloto, m, Ruido::DURO, None);
        // Y el poste: sin este dato, "SE ESTRELLÓ" no distingue chocar un
        // obstáculo de caerse solo, y el arreglo de una cosa no es el de la
        // otra (regla 3.5).
        let poste = if inf.roce_min.is_finite() {
            format!(", poste a {:.2} m{}", inf.roce_min,
                    if inf.roce_min <= 0.0 { " ← LO CHOCÓ" } else { "" })
        } else { String::new() };
        println!("   {:<10}{}    ({} con ruido duro: {}/{} tramos, queda a {:.2} m{})",
                 format!("{}", i + 1), fila, final_del_vuelo(&inf),
                 inf.puntos_hechos, inf.total_puntos, inf.dist_final, poste);
    }
    println!("\n   aptitud del peldaño (el PEOR de los nueve): {peor:.1}");
    // La sacudida, al lado del clásico. La aptitud es ciega a esto (7.5) y en
    // persecución resultó que el genoma PERDÍA por temblar, así que acá la
    // pregunta es la misma y no estaba medida.
    let sac_red: f32 = ms.iter()
        .map(|m| volar_con(piloto, m, Ruido::REAL, None).sacudida)
        .sum::<f32>() / ms.len() as f32;
    let sac_cla: f32 = ms.iter()
        .map(|m| volar_con(Piloto::Clasico, m, Ruido::REAL, None).sacudida)
        .sum::<f32>() / ms.len() as f32;
    println!("   sacudida del mando: clásico {sac_cla:.5} · red {sac_red:.5} · x{:.2}",
             sac_red / sac_cla.max(1e-9));
}

/// Un vuelo contado tramo por tramo: qué punto perseguía, dónde estaba, a qué
/// velocidad y cuánto tardó. Es la traza con la que se diagnostica, en vez de
/// razonar sobre el código y perder corridas de cuatro minutos.
pub fn informe_de_vuelo(piloto: Piloto, m: &Mision, ruido: Ruido) {
    println!("MISIÓN «{}»   ·   {} puntos   ·   {:.0} s de reloj   ·   {} poste(s)",
             m.nombre, m.puntos.len(), m.pasos as f32 * DT, m.obstaculos.len());
    println!("piloto: {}\n", match piloto {
        Piloto::Red(_) => "el genoma",
        Piloto::PorCelda { .. } => "el repertorio (un genoma por celda)",
        Piloto::Clasico => "guiado clásico (la vara honesta)",
    });
    for (i, p) in m.puntos.iter().enumerate() {
        println!("   punto {}: ({:>5.1},{:>5.1},{:>4.1})  radio {:.1}  {}",
                 i + 1, p.x, p.y, p.z, p.radio,
                 match (p.posar, p.accion) {
                     (true, Accion::Nada) => "POSARSE".to_string(),
                     (false, Accion::Nada) => "flotar".to_string(),
                     (b, a) => format!("{} · {:?}", if b { "POSARSE" } else { "flotar" }, a),
                 });
    }
    for o in &m.obstaculos {
        println!("   poste en ({:>5.2},{:>5.2})  radio {:.2}  alto {:.1}", o.x, o.y, o.radio, o.alto);
    }
    println!();

    let mut t = Vec::new();
    let inf = volar_con(piloto, m, ruido, Some(&mut t));
    println!("   {} de {} tramos   ·   {:.1} s   ·   aptitud {:.1}",
             inf.puntos_hechos, inf.total_puntos, inf.segundos, inf.aptitud);
    println!("   terminó a {:.2} m del punto {}   ·   {}",
             inf.dist_final, inf.puntos_hechos + 1, final_del_vuelo(&inf));
    if inf.roce_min.is_finite() {
        println!("   pasó a {:.2} m del poste más cercano", inf.roce_min);
    }
    if m.planifica() {
        println!("   mapa: {} postes confirmados ({} reales en la misión)",
                 inf.mapa.confirmados().count(), m.obstaculos.len());
    }
    println!("   error de su propia posición al final: {:.2} m (sin GPS)", inf.error_odometria);
    if inf.vz_toque != 0.0 {
        println!("   tocó el suelo a {:.2} m/s (límite {:.2})",
                 inf.vz_toque.abs(), crate::sim3d::VZ_POSADA);
    }
    // Cada entrada de `t` son 10 pasos de física (0.05 s). |v| sale de la
    // diferencia entre entradas consecutivas — no se guardaba la velocidad,
    // así que el encabezado prometía una columna que nunca se imprimía.
    let paso_traza = 10.0 * DT;
    let vel = |i: usize| -> f32 {
        if i == 0 { return 0.0; }
        let (x0, y0, z0) = t[i - 1];
        let (x1, y1, z1) = t[i];
        ((x1-x0).powi(2) + (y1-y0).powi(2) + (z1-z0).powi(2)).sqrt() / paso_traza
    };
    println!("\n   traza (cada 2 s):   t      x      y      z     |v|");
    for (i, &(x, y, z)) in t.iter().enumerate() {
        // `DRON_TRAZA_FINA=1`: la traza entera a 0.05 s, para los choques
        // que pasan antes de los primeros 2 s y no dejan ni una fila.
        if i % 40 != 0 && std::env::var_os("DRON_TRAZA_FINA").is_none() {
            continue;
        }
        println!("                     {:>4.0}s {:>6.2} {:>6.2} {:>6.2}  {:>5.2} m/s",
                 i as f32 * paso_traza, x, y, z, vel(i));
    }
    // Los últimos 2 s a resolución completa (0.05 s): es donde se decide si
    // el dron ENTRA o se queda orbitando cerca — a 2 s de grano eso no se ve.
    let desde = t.len().saturating_sub(40);
    if desde > 0 {
        println!("\n   últimos 2 s, a full (0.05 s):   t      x      y      z     |v|");
        for i in desde..t.len() {
            let (x, y, z) = t[i];
            println!("                     {:>5.2}s {:>6.2} {:>6.2} {:>6.2}  {:>5.2} m/s",
                     i as f32 * paso_traza, x, y, z, vel(i));
        }
    }
}

fn final_del_vuelo(i: &Informe) -> &'static str {
    if i.caido {
        "SE ESTRELLÓ"
    } else if i.posado_fuera_de_lugar {
        "se posó donde no debía"
    } else if i.completa {
        "MISIÓN CUMPLIDA"
    } else {
        "se quedó sin reloj"
    }
}

/// Dibuja al campeón volando la misión, visto desde arriba más el perfil de
/// altura. Se llama entre generaciones, así que el despegue y la posada se ven
/// aparecer en vivo — que es la única forma de creerle al número.
pub fn dibujar_campeon(id: i32, gen: usize, aptitud: f32, peldano: usize) {
    const ANCHO: usize = 66;
    const ALTO: usize = 13;

    let ms = misiones(peldano);
    let m = &ms[ms.len() - 1];
    let mut t = Vec::new();
    let inf = volar(id, m, Ruido::REAL, Some(&mut t));
    if t.is_empty() {
        return;
    }

    let (mut x0, mut x1) = (0.0f32, 0.0f32);
    let (mut y0, mut y1) = (0.0f32, 0.0f32);
    let mut zmax: f32 = 1.0;
    for p in &m.puntos {
        x0 = x0.min(p.x); x1 = x1.max(p.x);
        y0 = y0.min(p.y); y1 = y1.max(p.y);
        zmax = zmax.max(p.z);
    }
    for &(x, y, z) in &t {
        x0 = x0.min(x); x1 = x1.max(x);
        y0 = y0.min(y); y1 = y1.max(y);
        zmax = zmax.max(z);
    }
    let mx = ((x1 - x0) * 0.1).max(0.5);
    let my = ((y1 - y0) * 0.1).max(0.5);
    let (x0, x1, y0, y1) = (x0 - mx, x1 + mx, y0 - my, y1 + my);

    let mut mapa = vec![vec![' '; ANCHO]; ALTO];
    let cx = |x: f32| (((x - x0) / (x1 - x0) * (ANCHO - 1) as f32) as usize).min(ANCHO - 1);
    let cy = |y: f32| (((y1 - y) / (y1 - y0) * (ALTO - 1) as f32) as usize).min(ALTO - 1);
    for (i, &(x, y, _)) in t.iter().enumerate() {
        mapa[cy(y)][cx(x)] = if i == 0 { 'o' } else { '.' };
    }
    for (i, p) in m.puntos.iter().enumerate() {
        // Los puntos ya hechos se marcan distinto: se ve cuánto de la cadena
        // completó sin tener que leer el número.
        mapa[cy(p.y)][cx(p.x)] =
            if i < inf.puntos_hechos { '+' } else if p.posar { 'H' } else { 'X' };
    }
    for o in &m.obstaculos {
        mapa[cy(o.y)][cx(o.x)] = 'T';
    }
    let &(fx, fy, _) = t.last().unwrap();
    mapa[cy(fy)][cx(fx)] = '#';

    let mut perfil = vec![' '; ANCHO];
    let paso = (t.len().max(1) + ANCHO - 1) / ANCHO;
    for (c, trozo) in t.chunks(paso.max(1)).take(ANCHO).enumerate() {
        let z = trozo.iter().map(|p| p.2).sum::<f32>() / trozo.len() as f32;
        let n = (z / zmax.max(0.001) * 8.0).clamp(0.0, 8.0) as usize;
        perfil[c] = [' ', '_', '.', '-', '=', '+', '*', '#', '@'][n];
    }

    print!("\x1b[H\x1b[J");
    println!("  ENTRENANDO — generación {gen}    mejor aptitud {aptitud:.1}");
    println!("  misión «{}» ({} puntos), campeón en vivo\n", m.nombre, m.total());
    println!("  desde arriba    o partida  . ruta  X punto  H plataforma  + hecho  T poste  # final");
    println!("  +{}+", "-".repeat(ANCHO));
    for f in mapa {
        println!("  |{}|", f.into_iter().collect::<String>());
    }
    println!("  +{}+", "-".repeat(ANCHO));
    println!("  altura (0 a {zmax:.1} m)");
    println!("  |{}|", perfil.into_iter().collect::<String>());
    println!();
    println!("  {} de {} puntos   ·   {:.1} s   ·   aptitud {:.1}{}",
             inf.puntos_hechos, inf.total_puntos, inf.segundos, inf.aptitud,
             if inf.caido { "   ¡SE ESTRELLÓ!" }
             else if inf.posado_fuera_de_lugar { "   se posó donde no debía" }
             else if inf.completa { "   MISIÓN CUMPLIDA" } else { "" });
}

/// La aptitud de un genoma en un peldaño: el PEOR caso sobre misiones ×
/// niveles de ruido, igual que `gym3d::aptitud_robusta`. Un cambio que mejora
/// ocho de nueve casos y no toca el peor no mueve el número, y eso es a
/// propósito.
pub fn aptitud(piloto: Piloto, peldano: usize, robusto: bool, ruido: Ruido) -> f32 {
    terminos(piloto, peldano, robusto, ruido)
        .into_iter()
        .fold(f32::INFINITY, f32::min)
}

/// **Cuánto pesa el promedio al ORDENAR la población.** No toca la aprobación.
///
/// El problema que resuelve, medido: la aptitud es `min` sobre todos los
/// términos, y un `min` no tiene gradiente en ningún término salvo el peor. Un
/// entrenamiento se quedó **380 generaciones clavado en 39.2** en el peldaño 13
/// porque el mínimo lo definía el examen de repaso —que a su vez es un `min`
/// sobre doce peldaños olvidados—: una mutación que recuperaba once de los doce
/// **no movía el número ni un decimal**, así que la selección no la veía y se
/// perdía. Recuperarse exigía arreglar todo a la vez, que es justo lo que la
/// mutación al azar no hace.
///
/// La salida no es aflojar la vara (eso es la regla 3.2 y no se toca): es
/// separar **con qué se ordena** de **con qué se aprueba**. Aprobar sigue
/// pidiendo `min >= umbral`, el peor caso entero, igual que siempre. Ordenar
/// usa `min + PESO × (media − min)`, que es monótona en TODOS los términos, así
/// que el progreso parcial se ve y se hereda.
///
/// El peso es chico a propósito: con 0.15 el mínimo sigue mandando —dos
/// genomas con peor caso distinto se ordenan por el peor caso— y el promedio
/// sólo decide entre los que empatan ahí, que es exactamente el caso que
/// antes se resolvía por sorteo.
const PESO_MEDIA: f32 = 0.15;

/// Los términos crudos cuyo mínimo ES la aptitud: cada misión × cada ruido del
/// peldaño, más cada peldaño anterior del examen de repaso (ya escalado). Se
/// exponen para poder agregarlos de dos formas distintas sin duplicar el
/// cálculo — que es lo que la regla 3.3 prohíbe.
fn terminos(piloto: Piloto, peldano: usize, robusto: bool, ruido: Ruido) -> Vec<f32> {
    let n = sorteos();
    let mut v = Vec::new();
    for k in 0..n {
        // Cada vuelta es un juego de escenarios distinto — mismo genoma, mismo
        // peldaño, otro sorteo. Ver `SORTEOS`. El paso es primo y grande para
        // que los sorteos de una generación no se solapen con los de la
        // siguiente.
        SORTEO_EXTRA.with(|c| c.set(k.wrapping_mul(65_537)));
        let ms = misiones(peldano);
        let rs: &[Ruido] = if robusto { ruidos(peldano) } else { std::slice::from_ref(&ruido) };
        v.reserve(ms.len() * rs.len() + peldano);
        for r in rs {
            for m in &ms {
                v.push(volar_con(piloto, m, *r, None).aptitud);
            }
        }
        if robusto {
            v.extend(terminos_de_repaso(piloto, peldano));
        }
    }
    // Dejarlo en cero no es opcional: el hilo sigue vivo y la próxima
    // evaluación tiene que arrancar en el sorteo de la generación, no en el
    // último corrimiento que usó.
    SORTEO_EXTRA.with(|c| c.set(0));
    v
}

/// La aptitud HONESTA (peor caso, la que aprueba) y la de ORDEN (suave, la que
/// guía la búsqueda), calculadas de una sola pasada.
pub fn aptitud_y_orden(piloto: Piloto, peldano: usize, robusto: bool, ruido: Ruido) -> (f32, f32) {
    let (peor, orden, _) = aptitud_orden_y_terminos(piloto, peldano, robusto, ruido);
    (peor, orden)
}

/// Lo mismo, devolviendo también los términos de los que salieron.
///
/// **Existe para que una prueba no tenga que volar dos veces.** La versión
/// anterior de `ordenar_con_la_suave_no_afloja_lo_que_aprueba` llamaba a
/// `aptitud_y_orden` y después a `terminos` y comparaba los dos resultados, lo
/// que da por sentado que dos evaluaciones separadas ven el mismo sorteo de
/// escenarios. No lo ven siempre: los escenarios son estado global y las
/// pruebas corren en paralelo, así que la prueba reprobaba sola (4 corridas de
/// 6 antes del candado, 1 de 12 después). Pidiendo los términos de la MISMA
/// pasada, la propiedad que la prueba quiere verificar —que la honesta es el
/// mínimo exacto y la de orden nunca queda por debajo— se verifica sobre una
/// sola evaluación, que es además lo que de verdad hace el entrenador.
pub fn aptitud_orden_y_terminos(
    piloto: Piloto, peldano: usize, robusto: bool, ruido: Ruido,
) -> (f32, f32, Vec<f32>) {
    let v = terminos(piloto, peldano, robusto, ruido);
    if v.is_empty() {
        return (f32::INFINITY, f32::INFINITY, v);
    }
    let peor = v.iter().cloned().fold(f32::INFINITY, f32::min);
    let media = v.iter().sum::<f32>() / v.len() as f32;
    (peor, peor + PESO_MEDIA * (media - peor), v)
}

/// **El examen de repaso: lo que impide que el genoma olvide lo que ya sabía.**
///
/// Sin esto el sistema tiene un agujero que se ve apenas se mide. La aptitud
/// del peldaño N sólo evaluaba las misiones del peldaño N, así que **nada
/// castigaba olvidar los anteriores**, y la evolución optimiza únicamente lo
/// que se mide. Lo que salió cuando `telemetria_estancado` finalmente lo miró:
///
/// ```text
///   gen 92  TRABADO en «peldaño 7/13: esquivar» hace 50 generaciones
///         └ ⚠ DESAPRENDIÓ los peldaños [1, 2, 3, 4, 5, 6]
/// ```
///
/// Los seis. Todos. Había aprobado frenar, rumbo, ir a B y sensores duros, y
/// para cuando peleaba esquivar ya no sabía hacer ninguno. Eso explica el
/// misterio del genoma que quedó instalado: sacaba 1/5 en "ir de A a B"
/// **a pesar de haber aprobado ese peldaño** — lo aprobó y después lo olvidó.
///
/// El arreglo es el estándar contra el olvido catastrófico: seguir tomando
/// examen de lo viejo. Cada peldaño anterior aporta su misión más
/// representativa, y como los umbrales son distintos (un peldaño de un punto
/// pide 900, la entrega pide 2400) se comparan **en fracción del propio
/// umbral**, reescalada al del peldaño actual. Así el mínimo tiene sentido:
/// estar al 50% de lo que pide un peldaño viejo pesa igual que estar al 50%
/// del actual.
///
/// El costo es de un episodio por peldaño anterior (no los nueve), con ruido
/// real. En el peldaño 12 eso es ~2× el precio de una evaluación, que es
/// barato comparado con entrenar trece peldaños para tirar el resultado.
fn examen_de_repaso(piloto: Piloto, peldano: usize) -> f32 {
    if peldano == 0 {
        return f32::INFINITY; // no hay nada atrás que olvidar
    }
    let umbral_hoy = umbral_aprobado(peldano);
    let mut peor = f32::INFINITY;
    for p in 0..peldano {
        // TODAS las misiones del peldaño viejo, no una.
        //
        // La primera versión tomaba una sola (`misiones(p)[p % len]`) para
        // ahorrar episodios, y el genoma hizo exactamente lo que hace
        // cualquiera al que le avisan qué entra en el examen: aprendió esa
        // misión y olvidó las otras dos. La telemetría lo cazó enseguida —
        // seguía diciendo `DESAPRENDIÓ los peldaños [1]` **con el examen de
        // repaso ya puesto**, porque el examen medía una cosa y la
        // verificación medía tres. Un examen más angosto que la capacidad que
        // dice proteger no protege nada: enseña a pasar el examen.
        // Y con TODOS los ruidos que ese peldaño exigía, no sólo el real.
        //
        // El mismo error dos veces, en los dos ejes. Primero el examen tomaba
        // una misión de tres y el genoma olvidó las otras dos. Se arregló, y
        // volvió a aparecer: `DESAPRENDIÓ los peldaños [4]` **con el examen
        // completo en misiones**, porque el examen usaba sólo `Ruido::REAL` y
        // la verificación usa `ruidos(p)` — el genoma quedó bien en el ruido
        // examinado y mal en el que no. El examen tiene que cubrir EXACTAMENTE
        // lo que se exigió para aprobar, en todos los ejes; si no, el genoma
        // encuentra el eje descubierto y lo abandona.
        for m in &misiones(p) {
            for r in ruidos(p) {
                let apt = volar_con(piloto, m, *r, None).aptitud;
                // Fracción del umbral viejo, llevada a la escala del de hoy.
                peor = peor.min(apt / umbral_aprobado(p) * umbral_hoy);
            }
        }
    }
    peor
}

/// Lo mismo que `examen_de_repaso` pero devolviendo cada término en vez de su
/// mínimo, para que `aptitud_y_orden` pueda promediarlos. Comparte la fórmula
/// de escalado con la de arriba a propósito: si divergieran, el número que
/// ordena y el que aprueba dejarían de hablar del mismo examen.
/// El peor peldaño anterior, tal como lo ve la aptitud (ya escalado).
///
/// Público porque sin esto **no se puede saber qué término está atando la
/// nota**, y eso importa: un genoma puede sacar 1011 en las misiones de su
/// propio peldaño y −50 de aptitud, y la diferencia entera es que olvidó algo
/// de atrás. Mirar sólo `detalle` —que imprime las misiones propias— hace
/// ver un peldaño resuelto donde lo que hay es olvido (regla 3.8). Lo usa
/// `dron sorteos`.
pub fn peor_de_repaso(piloto: Piloto, peldano: usize) -> f32 {
    terminos_de_repaso(piloto, peldano)
        .into_iter()
        .fold(f32::INFINITY, f32::min)
}

/// **CUÁL peldaño anterior es el que está olvidado**, y con qué nota.
///
/// `peor_de_repaso` dice cuánto, no cuál, y "cuál" es la pregunta que decide
/// qué hacer: si el que se olvida es siempre el mismo, es un conflicto
/// concreto entre dos capacidades y se ataca ahí; si va rotando, es olvido
/// general y el remedio es estructural. Sin este renglón las dos cosas se ven
/// idénticas desde afuera. Era la deuda 2 de la bitácora 7.31.
pub fn repaso_por_peldano(piloto: Piloto, peldano: usize) -> Vec<(usize, f32)> {
    if peldano == 0 {
        return Vec::new();
    }
    let umbral_hoy = umbral_aprobado(peldano);
    (0..peldano)
        .map(|p| {
            let mut peor = f32::INFINITY;
            for m in &misiones(p) {
                for r in ruidos(p) {
                    let apt = volar_con(piloto, m, *r, None).aptitud / umbral_aprobado(p) * umbral_hoy;
                    peor = peor.min(apt);
                }
            }
            (p, peor)
        })
        .collect()
}

fn terminos_de_repaso(piloto: Piloto, peldano: usize) -> Vec<f32> {
    let mut v = Vec::new();
    if peldano == 0 {
        return v;
    }
    let umbral_hoy = umbral_aprobado(peldano);
    for p in 0..peldano {
        let (mut suma, mut suma_clasico, mut n) = (0.0f32, 0.0f32, 0u32);
        for (mi, m) in misiones(p).iter().enumerate() {
            for (ri, r) in ruidos(p).iter().enumerate() {
                let apt = volar_con(piloto, m, *r, None).aptitud;
                // Cada misión vieja tiene que seguir pasando SU umbral.
                v.push(apt / umbral_aprobado(p) * umbral_hoy);
                suma += apt;
                suma_clasico += clasico_en(p, mi, ri, m, *r);
                n += 1;
            }
        }
        // **Piso contra el clásico, por PELDAÑO y no por misión** (regla
        // 3.8). La primera versión (7.44) comparaba misión por misión: con
        // ~100 comparaciones por sorteo, el ruido de UNA sola alcanzaba para
        // reprobar aunque la red fuera, en promedio, tan buena como el
        // clásico — el campeón de la corrida `EXP_20260926_final` tenía
        // medianas de 1296-1467 en todos los peldaños viejos (umbral 1200) y
        // reprobaba 12 de 12 sorteos. Es la regla 3.20: un guardián que
        // decide con una muestra (bitácora 7.47).
        let (media, media_clasico) = (suma / n.max(1) as f32, suma_clasico / n.max(1) as f32);
        let piso = umbral_aprobado(p).max(media_clasico * PISO_CLASICO_REPASO);
        v.push(media / piso * umbral_hoy);
    }
    v
}

/// Qué fracción de lo que saca el clásico tiene que conservar la red en cada
/// misión de un peldaño viejo.
pub const PISO_CLASICO_REPASO: f32 = 0.95;

/// Lo que saca el clásico en una misión vieja, calculado una vez por
/// escenario: toda la población de una generación comparte variante, así que
/// sin caché se volaría el mismo clásico 48 veces por generación.
fn clasico_en(p: usize, mi: usize, ri: usize, m: &Mision, r: Ruido) -> f32 {
    use std::collections::HashMap;
    use std::sync::Mutex;
    static CACHE: Mutex<Option<HashMap<(u32, u32, usize, usize, usize), f32>>> = Mutex::new(None);
    let clave = (variante(), SORTEO_EXTRA.with(|c| c.get()), p, mi, ri);
    if let Some(a) = CACHE.lock().unwrap().as_ref().and_then(|c| c.get(&clave)) {
        return *a;
    }
    let a = volar_con(Piloto::Clasico, m, r, None).aptitud;
    let mut g = CACHE.lock().unwrap();
    let c = g.get_or_insert_with(HashMap::new);
    if c.len() > 20_000 {
        c.clear();
    }
    c.insert(clave, a);
    a
}

/// Desde qué peldaño entra el ruido DURO.
pub const PELDANO_DURO: usize = 6;

/// Con qué sensores se entrena cada peldaño.
///
/// La aleatorización de dominio **no se negocia para evaluar**: el número del
/// simulador sólo significa algo afuera si vale en el peor sensor, y por eso
/// el informe de capacidades y los peldaños de arriba la exigen entera.
///
/// Pero meterla desde el primer peldaño tiene un costo que se puede medir, y
/// se midió. Este es el campeón trabado en «ir a B», los nueve casos:
///
/// ```text
///   misión         limpio       real       duro
///   1             1001.2     1067.6     1038.7
///   2             1252.5     1253.6      100.2
///   3             1075.7     1082.2       97.9
/// ```
///
/// Cumplía las tres misiones con sensores limpios y reales —mil y pico de
/// puntos— y se caía a 100 con sensores duros. Como la aptitud es el peor de
/// los nueve, **el mínimo lo definía siempre el ruido y nunca la capacidad que
/// el peldaño quería enseñar**: navegar, esquivar y posarse quedaban invisibles
/// detrás de un problema de filtrado.
///
/// Así que aguantar sensores rotos pasa a ser lo que ya es —una capacidad
/// más— y tiene su propio escalón, igual que frenar o girar. Abajo de él se
/// entrena con sensores limpios y reales; de `PELDANO_DURO` para arriba, con
/// todo el rango y para siempre.
pub fn ruidos(peldano: usize) -> &'static [Ruido] {
    const SUAVE: &[Ruido] = &[Ruido::LIMPIO, Ruido::REAL];
    const TODO: &[Ruido] = &[Ruido::LIMPIO, Ruido::REAL, Ruido::DURO];
    // **La posada con plataforma CORRIDA no exige ruido duro, y es una
    // medición, no una concesión.** Con `Ruido::DURO` el piloto clásico —modelo
    // físico perfecto, cero ruido de aprendizaje— se queda FLOTANDO a 0.85 m y
    // no baja nunca: su velocidad horizontal estimada no baja del umbral que le
    // da permiso para descender, porque persiguiendo una posición tan sucia se
    // mueve de verdad 0.3-0.6 m entre muestras. Posarse derrapando a ~1 m/s es
    // un choque de verdad (`VH_POSADA` = 1.0), así que el freno está bien
    // puesto: lo que estaba mal era exigirlo.
    //
    // ```text
    //   peldaño 12, misión 3, clásico:   limpio 1208.7 · real 1163.5 · DURO 88.3
    //   aptitud del peldaño (el peor):   88.3 de un umbral de 900
    // ```
    //
    // Un peldaño que ni la vara honesta puede aprobar no mide al genoma, lo
    // cuelga: el entrenamiento se quedó 330 generaciones clavado en 20.4 acá.
    // Y es el mismo defecto de siempre —dos capacidades en un escalón— porque
    // el peldaño 11 (posarse ya encima de la plataforma) SÍ aprueba con ruido
    // duro (1274.8): lo que no se puede es navegar Y posarse con precisión con
    // los sensores rotos a la vez. Aguantar un GPS que salta es una capacidad
    // aparte, exactamente como dice el comentario de `PELDANO_DURO`.
    // **La entrega entera, primero sin ruido duro — separa un eje de
    // dificultad, no "arregla la vara".** Corrección sobre mi propia primera
    // nota acá: medí con `mision 15 1/2 clasico ruido=limpio` esperando que el
    // clásico sólo se estrellara con ruido duro, y **no es así** — se
    // estrella contra el mismo poste con sensores LIMPIOS, porque nunca tiene
    // mecanismo de esquive (regla 3.1, por diseño): eso no es un defecto de
    // la vara, es lo mismo que ya pasa en CUALQUIER peldaño con postes
    // (`esquivar`, `ida y vuelta`). El clásico no está para aprobar esto.
    //
    // La razón real del split está en el CAMPEÓN, no en el clásico: en el
    // mismo genoma entrenado, la misma misión saca 4185-4495 (cumplida, 6/6)
    // con ruido DURO y se estrella con ruido limpio/real — la red no tiene
    // esquive robusto, tiene esquive que a veces el ruido empuja por
    // casualidad al lugar correcto. Pedirle el mínimo de NUEVE casos a la vez
    // (regla 3.8) cuando el propio campeón es así de inconsistente entre
    // ellos es la misma receta que colgó la posada corrida 330 generaciones:
    // demasiadas capacidades en un escalón. Sacar el ruido duro no regala
    // nada —el clásico sigue sin aprobar esto tampoco— pero baja de 9 a 6 los
    // casos que tienen que salir bien a la vez. El ruido duro de verdad queda
    // como el ÚLTIMO peldaño de la escalera (ver `misiones`, el catch-all
    // `_`), para medir la robustez por separado de la capacidad.
    if peldano == PELDANO_POSADA_CORRIDA || peldano == PELDANO_ENTREGA_SUAVE { return SUAVE; }
    if peldano < PELDANO_DURO { SUAVE } else { TODO }
}

/// El peldaño de «posarse de verdad» (plataforma corrida). Ver `ruidos`.
pub const PELDANO_POSADA_CORRIDA: usize = 11;
/// La entrega entera, todavía sin el peor ruido. Ver `ruidos`.
pub const PELDANO_ENTREGA_SUAVE: usize = 14;

#[cfg(test)]
mod pruebas {
    /// **El módulo vertical no cambia nada mientras sea el mismo genoma.**
    /// Con las nueve celdas y el vertical iguales, la altura que emite el
    /// módulo (con su propia memoria, por `abordo::pensar`) es bit a bit la
    /// que emitía la celda, así que el vuelo tiene que dar IDÉNTICO. Si esto
    /// se rompe, lo que mida una corrida con `vertical` mezcla el efecto de
    /// separar funciones con un piloto distinto.
    #[test]
    fn el_modulo_vertical_igual_a_las_celdas_vuela_identico() {
        let _g = escenarios_para_pruebas();
        const SLOT: i32 = 103;
        iafz::init(SLOT, 4242);
        let r = crate::repertorio::Repertorio::sembrar(SLOT);
        crate::repertorio::fijar_banco(&r);
        fijar_variante(9);
        let mut con_otro = false;
        for p in [0usize, 10, 11] {
            for m in &misiones(p) {
                crate::repertorio::olvidar_vertical();
                let sin = volar_con(Piloto::PorCelda { slot: SLOT, entrena: crate::repertorio::CELDAS }, m, Ruido::REAL, None).aptitud;
                crate::repertorio::fijar_vertical(&iafz::volcar(SLOT));
                let con = volar_con(Piloto::PorCelda { slot: SLOT, entrena: crate::repertorio::CELDAS }, m, Ruido::REAL, None).aptitud;
                let como_ind = volar_con(Piloto::PorCelda { slot: SLOT, entrena: crate::repertorio::VERTICAL }, m, Ruido::REAL, None).aptitud;
                // Y el módulo posar, igual a las celdas, tampoco cambia nada:
                // durante el aterrizaje piensa él (con su memoria propia) y
                // fuera del aterrizaje ni se consulta.
                crate::repertorio::fijar_posar(&iafz::volcar(SLOT));
                let con_posar = volar_con(Piloto::PorCelda { slot: SLOT, entrena: crate::repertorio::CELDAS }, m, Ruido::REAL, None).aptitud;
                let posar_ind = volar_con(Piloto::PorCelda { slot: SLOT, entrena: crate::repertorio::POSAR }, m, Ruido::REAL, None).aptitud;
                crate::repertorio::olvidar_posar();
                assert_eq!(sin.to_bits(), con_posar.to_bits(), "peldaño {p}: {sin} contra {con_posar} con módulo posar");
                assert_eq!(sin.to_bits(), posar_ind.to_bits(), "peldaño {p}: {sin} contra {posar_ind} entrenando posar");
                assert_eq!(sin.to_bits(), con.to_bits(), "peldaño {p}: {sin} sin módulo, {con} con módulo");
                assert_eq!(sin.to_bits(), como_ind.to_bits(), "peldaño {p}: {sin} contra {como_ind} entrenando el vertical");
                // Y un vertical DISTINTO tiene que notarse: si no, el módulo
                // no está conectado a nada.
                let mut otro = iafz::volcar(SLOT);
                iafz::salidas_en_cero(&mut otro);
                crate::repertorio::fijar_vertical(&otro);
                let dist = volar_con(Piloto::PorCelda { slot: SLOT, entrena: crate::repertorio::CELDAS }, m, Ruido::REAL, None).aptitud;
                con_otro |= dist.to_bits() != sin.to_bits();
            }
        }
        crate::repertorio::olvidar_vertical();
        crate::repertorio::olvidar_banco();
        assert!(con_otro, "cambiar el módulo vertical no cambió ningún vuelo: no está conectado");
        // Un módulo posar DISTINTO tiene que cambiar posarse (peldaño 12).
        crate::repertorio::fijar_banco(&r);
        let m = &misiones(11)[0];
        let sin = volar_con(Piloto::PorCelda { slot: SLOT, entrena: crate::repertorio::CELDAS }, m, Ruido::REAL, None).aptitud;
        let mut otro = iafz::volcar(SLOT);
        iafz::salidas_en_cero(&mut otro);
        crate::repertorio::fijar_posar(&otro);
        let con = volar_con(Piloto::PorCelda { slot: SLOT, entrena: crate::repertorio::CELDAS }, m, Ruido::REAL, None).aptitud;
        crate::repertorio::olvidar_posar();
        crate::repertorio::olvidar_banco();
        assert_ne!(sin.to_bits(), con.to_bits(), "cambiar el módulo posar no cambió el aterrizaje: no está conectado");
    }

    use super::*;

    /// **Los escenarios de misión son estado GLOBAL, y las pruebas corren en
    /// paralelo.**
    ///
    /// `VARIANTE` es un `AtomicU32` de módulo —tiene que serlo, el entrenador
    /// lo lee desde todos los hilos de rayon— así que dos pruebas que vuelen
    /// misiones al mismo tiempo se pisan los escenarios, y una que llame dos
    /// veces a `misiones()` esperando los mismos postes puede recibir dos
    /// juegos distintos.
    ///
    /// No es hipotético: medido antes de poner este candado, `cargo test
    /// --release` **reprobaba 4 corridas de 6**, siempre en
    /// `ordenar_con_la_suave_no_afloja_lo_que_aprueba` y siempre en un peldaño
    /// distinto (1, 6, …). Es el peor tipo de instrumento roto, porque la
    /// batería «pasa» si se la corre dos veces y se mira la que salió bien —
    /// y esta sesión reportó «105 passed» dos veces sobre corridas que
    /// tuvieron suerte (regla 3.18: el instrumento de medición también se
    /// audita).
    ///
    /// Toda prueba que vuele misiones toma este candado, **esté en este módulo
    /// o en otro**: `repertorio` y `capacidades` también vuelan misiones, y
    /// con el candado sólo acá la reprobación bajó de 4 de 6 a 1 de 10 —
    /// menos, pero sigue siendo un sorteo. Por eso vive en `mision` y es
    /// `pub(crate)`.
    fn escenarios_para_mi() -> std::sync::MutexGuard<'static, ()> {
        super::escenarios_para_pruebas()
    }

    /// **La escalera termina en la entrega con sensores reales** (bitácora
    /// 7.44): la entrega con ruido DURO se sacó porque la vara la aprobaba en
    /// 22 de 48. Fija las dos cosas: el último peldaño es la entrega, y no
    /// pide ruido duro.
    #[test]
    fn la_escalera_termina_en_la_entrega_sin_ruido_duro() {
        let _escenarios = escenarios_para_mi();
        assert_eq!(PELDANO_ENTREGA_SUAVE, PELDANOS - 1);
        assert_eq!(misiones(PELDANOS - 1)[0].nombre, "entrega");
        assert_eq!(ruidos(PELDANOS - 1).len(), 2, "la entrega no debe pedir ruido duro");
    }

    /// **Ordenar nunca puede inflar lo que aprueba.** La aptitud de orden es
    /// más permisiva que la honesta por construcción (suaviza el `min` con la
    /// media), así que si alguna vez se usara para aprobar, se estarían
    /// promoviendo genomas que no cumplen el peor caso — justo lo que la regla
    /// 3.2 existe para impedir. Esta prueba fija las dos propiedades que hacen
    /// segura la separación: la honesta ES el mínimo, y la de orden nunca queda
    /// por debajo de ella.
    #[test]
    fn ordenar_con_la_suave_no_afloja_lo_que_aprueba() {
        let _escenarios = escenarios_para_mi();
        crate::iafz::init(77, 3);
        for p in [0usize, 5, 11] {
            // UNA sola evaluación para los tres números: ver
            // `aptitud_orden_y_terminos`. Con dos llamadas, esta prueba
            // reprobaba sola cuando otra prueba le cambiaba los escenarios
            // entre medio.
            let (honesta, orden, v) =
                aptitud_orden_y_terminos(Piloto::Red(77), p, true, Ruido::REAL);
            let minimo = v.iter().cloned().fold(f32::INFINITY, f32::min);
            assert!((honesta - minimo).abs() < 1e-3,
                "peldaño {}: la aptitud honesta tiene que ser el mínimo exacto \
                 ({honesta} contra {minimo})", p + 1);
            assert!(orden >= honesta - 1e-3,
                "peldaño {}: la de orden ({orden}) quedó por DEBAJO de la honesta \
                 ({honesta}) — la suavización sólo puede subir", p + 1);
            // Y la honesta es la que se compara contra el umbral, nunca la otra.
            assert!(aptitud(Piloto::Red(77), p, true, Ruido::REAL) <= orden + 1e-3);
        }
    }

    /// **Ningún peldaño SIN OBSTÁCULOS puede pedir más de lo que la vara
    /// honesta puede dar.** Es el equivalente, para la entrega, de
    /// `persecucion::ningun_umbral_pide_mas_que_la_perfeccion` (regla 3.16) —
    /// que existía sólo del lado de persecución y por eso este agujero vivió
    /// meses sin que nadie lo viera.
    ///
    /// Qué caza, medido el día que se escribió: el peldaño 12 («posarse» con
    /// la plataforma corrida) pedía 900 y el piloto CLÁSICO —modelo físico
    /// perfecto, cero ruido de aprendizaje— sacaba **88.3**, porque con ruido
    /// duro no logra asentarse lo suficiente para bajar. Un entrenamiento se
    /// quedó 330 generaciones clavado en 20.4 contra ese umbral imposible, y
    /// el log no lo decía: el cartel de TRABADO es idéntico para "esto es
    /// difícil" y para "esto no se puede".
    ///
    /// # La exención de los peldaños con postes, y por qué estaba mal
    ///
    /// Esta prueba se saltea los peldaños con obstáculos. **La razón que
    /// estuvo escrita acá era falsa**, y conviene dejarla anotada porque fue
    /// la que permitió que los peldaños 8 y 9 quedaran sin calibrar durante
    /// meses (bitácora 7.34). Decía:
    ///
    /// > «ahí el clásico saca 0/3 A PROPÓSITO (choca siempre) y el genoma 3/3
    /// > — es el mejor resultado del proyecto y la única evidencia de que la
    /// > red aporta capacidad propia»
    ///
    /// Ese «genoma 3/3» era el conjunto de ENTRENAMIENTO, y la bitácora 7.17
    /// ya lo había refutado con postes nuevos: **el genoma saca 1/3 y el
    /// clásico 2/3**. O sea que la prueba que existe para cazar umbrales
    /// imposibles tenía instrucciones de mirar para otro lado justo en los
    /// peldaños donde el umbral era imposible, y la justificación de mirar
    /// para otro lado era un resultado que el propio proyecto ya sabía falso.
    ///
    /// La exención se mantiene, pero por el motivo verdadero y con el número
    /// a la vista: con postes sorteados al azar **hay sorteos donde la física
    /// no da tiempo** (contrato §3.3), así que el peor caso sobre todos los
    /// sorteos nunca va a llegar al umbral y exigirlo acá sería exigir lo
    /// imposible. Lo que sí se exige es que la vara no vuelva a quedarse
    /// ciega: medido sobre 24 sorteos con `dron sorteos N clasico n=24`, el
    /// clásico aprueba el peldaño 8 en 12 y el 9 en 7 (era 4 y 2 cuando
    /// `Guiado::consignas` ni miraba los obstáculos). Acá se pide poquísimo
    /// —que pase al menos uno de tres sorteos— porque la prueba tiene que
    /// correr en la batería y volar 24 sorteos cuesta minutos; alcanza para
    /// cazar la regresión que importa, que es que el esquive de la vara se
    /// apague.
    ///
    /// **Deuda declarada, no resuelta:** 12 de 24 sigue siendo una rifa. El
    /// peldaño 8 no está calibrado; está menos descalibrado.
    #[test]
    fn ningun_peldano_sin_postes_pide_mas_de_lo_que_el_clasico_puede() {
        let _escenarios = escenarios_para_mi();
        for p in 0..PELDANOS {
            if misiones(p).iter().any(|m| !m.obstaculos.is_empty()) {
                // **Se saltea, y la regresión se caza en otro lado.** Acá no
                // se puede pinchar un sorteo concreto: `VARIANTE` es un
                // atómico GLOBAL y las pruebas corren en paralelo, así que un
                // `fijar_variante` acá le cambia los escenarios a la prueba
                // que esté corriendo al lado — pasó, y dejó a
                // `ordenar_con_la_suave_no_afloja_lo_que_aprueba` midiendo
                // otra misión a mitad de vuelo. Y pedir «uno de tres sorteos
                // al azar» con una tasa real de 5/12 falla una corrida de
                // cada cinco: una prueba que parpadea no protege nada.
                //
                // Que el esquive de la vara siga puesto lo verifican, sin
                // tocar nada global y en microsegundos,
                // `pid3d::pruebas_esquive`: con el anillo despejado el comando
                // es EXACTAMENTE el clásico, y con el sector central tapado se
                // corre de costado.
                continue;
            }
            let clasico = {
                let ms = misiones(p);
                let mut peor = f32::INFINITY;
                for r in ruidos(p) {
                    for m in &ms {
                        peor = peor.min(volar_con(Piloto::Clasico, m, *r, None).aptitud);
                    }
                }
                peor
            };
            let umbral = umbral_aprobado(p);
            assert!(clasico >= umbral,
                "peldaño {} «{}» pide {umbral:.0} y el piloto clásico sólo llega a \
                 {clasico:.1}: es un umbral que NADIE puede aprobar, no un peldaño \
                 difícil. Un entrenamiento se va a clavar ahí para siempre.",
                p + 1, misiones(p)[0].nombre);
        }
    }

    /// **La propiedad que hace que el híbrido valga la pena, para la tarea de
    /// ENTREGA.** Con las cuatro salidas en cero, la red tiene que dar
    /// EXACTAMENTE el piloto clásico: ése es el piso desde el que empieza a
    /// buscar la evolución, y es lo que `consignas_red` no daba (ahí cero
    /// significaba "quedate quieto", así que había que aprender a navegar
    /// desde cero — medido, el genoma quedaba al 9% del clásico).
    #[test]
    fn con_la_red_en_cero_la_entrega_es_exactamente_el_clasico() {
        let _escenarios = escenarios_para_mi();
        let guiado = crate::pid3d::Guiado::default();
        let ceros = [0.0f32; iafz::SALIDAS];
        for (x, y, z, ox, oy, oz) in [
            (0.0f32, 0.0f32, 3.0f32, 6.0f32, 0.0f32, 3.0f32),
            (1.0, -2.0, 2.0, -5.0, 5.0, 5.0),
            (3.0, 4.0, 4.5, 4.0, -7.0, 2.5),
        ] {
            let d = Dron3 { x, y, z, vx: 1.5, vy: -0.5, yaw: 0.4, ..Default::default() };
            let vista = crate::sim3d::RasgosVisuales::default(); // sin obstáculos: el freno no debe tocar nada
            let h = crate::gym3d::consignas_hibridas_punto(&ceros, &d, ox, oy, oz, &guiado, &vista);
            let (r, p, yw) = guiado.consignas(&d, ox, oy);
            assert_eq!(h.roll.to_bits(), r.to_bits());
            assert_eq!(h.pitch.to_bits(), p.to_bits());
            assert_eq!(h.yaw.to_bits(), yw.to_bits());
            assert_eq!(h.alt.to_bits(), oz.to_bits());
        }
    }

    fn dron_en(x: f32, y: f32, z: f32, v: f32) -> Dron3 {
        Dron3 { x, y, z, vx: v, ..Default::default() }
    }

    /// **El dron tiene que entrenar yendo a todos lados, no a tres lugares.**
    ///
    /// Un quad real se traslada inclinándose, así que puede ir en cualquier
    /// dirección del plano y combinarla con subir o bajar: las ocho diagonales
    /// existen de verdad y la física del simulador ya las modela
    /// (`sim3d::angulo_de_crucero`). Lo que faltaba no era el movimiento, eran
    /// los DESTINOS: las tres rutas fijas de siempre son `+x` nivelado,
    /// `−x+y` subiendo y `+x−y` bajando, o sea que el currículo nunca pedía ir
    /// a `−y`, ni a `−x` puro, ni a la mayoría de las diagonales.
    ///
    /// Esta prueba barre variantes y exige que se cubran los ocho rumbos del
    /// compás Y las tres bandas verticales (subiendo, bajando, nivelado).
    #[test]
    fn el_curriculo_cubre_todas_las_direcciones() {
        let _escenarios = escenarios_para_mi();
        use std::collections::HashSet;
        let mut rumbos = HashSet::new();
        let mut verticales = HashSet::new();
        for v in 0..64u32 {
            fijar_variante(v);
            for (x0, y0, z0, bx, by, bz, _) in rutas() {
                let ang = (by - y0).atan2(bx - x0);
                // Octante del compás: N, NE, E, SE, S, SO, O, NO.
                let oct = ((ang / std::f32::consts::TAU * 8.0).round() as i32).rem_euclid(8);
                rumbos.insert(oct);
                let dz = bz - z0;
                verticales.insert(if dz > 0.5 { 1 } else if dz < -0.5 { -1 } else { 0 });
            }
        }
        fijar_variante(0);
        assert_eq!(rumbos.len(), 8,
            "el currículo sólo pide ir hacia {} de los 8 rumbos: {:?}", rumbos.len(), rumbos);
        assert_eq!(verticales.len(), 3,
            "faltan bandas verticales (subir/bajar/nivelado): {:?}", verticales);
    }

    /// Y las direcciones tienen que ser alcanzables de verdad: nada de
    /// destinos bajo tierra o pegados al piso, que es tarea de los peldaños de
    /// posada y no de los de navegación.
    #[test]
    fn ninguna_ruta_manda_al_dron_bajo_tierra() {
        let _escenarios = escenarios_para_mi();
        for v in 0..64u32 {
            fijar_variante(v);
            for (_, _, z0, _, _, bz, _) in rutas() {
                assert!(z0 > 1.0 && bz > 1.0, "ruta con altura inviable: {z0} → {bz}");
                assert!(z0 < 8.0 && bz < 8.0, "ruta fuera del sobre de vuelo: {z0} → {bz}");
            }
        }
        fijar_variante(0);
    }

    /// La regla que define todo el gym: pasar cerca NO es llegar. Es el agujero
    /// por el que se colaba el orbitar.
    #[test]
    fn pasar_de_largo_no_cuenta_como_llegar() {
        let _escenarios = escenarios_para_mi();
        let m = corta_hacia((0.0, 0.0, 3.0, 6.0, 0.0, 3.0, 11), 2.0, "frenar");
        let mut s = Seguidor::nuevo(false);
        // Encima del punto pero a 4 m/s: aunque se quede mil pasos ahí, no
        // acumula nada, porque a esa velocidad no está quieto.
        for _ in 0..1000 {
            assert_eq!(s.paso(&m, &dron_en(2.0, 0.0, 3.0, 4.0)), Avance::Sigue);
        }
    }

    #[test]
    fn entrar_y_quedarse_si_cuenta() {
        let _escenarios = escenarios_para_mi();
        let m = corta_hacia((0.0, 0.0, 3.0, 6.0, 0.0, 3.0, 11), 2.0, "frenar");
        let mut s = Seguidor::nuevo(false);
        let quieto = dron_en(2.0, 0.0, 3.0, 0.1);
        for _ in 0..(PASOS_QUIETO - 1) {
            assert_eq!(s.paso(&m, &quieto), Avance::Sigue);
        }
        assert_eq!(s.paso(&m, &quieto), Avance::Llego);
        assert!(s.completa(&m));
    }

    /// Un pico de velocidad por una ráfaga no puede borrar medio segundo de
    /// haber estado quieto; salirse del radio sí.
    #[test]
    fn una_rafaga_no_reinicia_la_cuenta_pero_irse_si() {
        let _escenarios = escenarios_para_mi();
        let m = corta_hacia((0.0, 0.0, 3.0, 6.0, 0.0, 3.0, 11), 2.0, "frenar");
        let quieto = dron_en(2.0, 0.0, 3.0, 0.1);
        let rafaga = dron_en(2.0, 0.0, 3.0, 3.0); // adentro del radio, rápido
        let afuera = dron_en(5.0, 0.0, 3.0, 0.1); // lejos

        let mut s = Seguidor::nuevo(false);
        for _ in 0..(PASOS_QUIETO - 1) {
            s.paso(&m, &quieto);
        }
        assert_eq!(s.paso(&m, &rafaga), Avance::Sigue, "la ráfaga no suma");
        assert_eq!(s.paso(&m, &quieto), Avance::Llego, "pero tampoco borra lo acumulado");

        let mut s = Seguidor::nuevo(false);
        for _ in 0..(PASOS_QUIETO - 1) {
            s.paso(&m, &quieto);
        }
        s.paso(&m, &afuera);
        assert_eq!(s.paso(&m, &quieto), Avance::Sigue, "irse del radio sí reinicia");
    }

    /// Posarse en el tramo que lo pide es llegar; posarse en cualquier otro es
    /// la misión fallada, no un vuelo raro.
    #[test]
    fn posarse_donde_no_va_falla_la_mision() {
        let _escenarios = escenarios_para_mi();
        let m = despegue(0.0, 0.0, 3.0, 11); // el único punto es AIRE
        let mut s = Seguidor::nuevo(true);
        s.paso(&m, &dron_en(0.0, 0.0, 1.0, 0.0)); // despegó
        let posado = Dron3 { apoyado: true, ..dron_en(0.0, 0.0, 0.0, 0.0) };
        assert_eq!(s.paso(&m, &posado), Avance::PosadaIndebida);
    }

    /// Después de entregar, estar en el suelo vuelve a ser lo normal: el tramo
    /// siguiente es despegar. Sin esto la entrega se declaraba fallada en el
    /// instante posterior a completar la posada.
    #[test]
    fn despues_de_posarse_el_suelo_deja_de_ser_falla() {
        let _escenarios = escenarios_para_mi();
        let m = entrega((0.0, 0.0, 3.0, 6.0, 0.0, 3.0, 11), 0);
        let mut s = Seguidor::nuevo(true);
        s.indice = 2; // tramo 3: posarse en B
        s.ya_despego = true;
        let posado = Dron3 { apoyado: true, ..dron_en(6.0, 0.0, 0.0, 0.0) };
        assert_eq!(s.paso(&m, &posado), Avance::Llego);
        assert_eq!(s.indice, 3);
        // Sigue apoyado, y el tramo 4 es aire: NO puede contar como falla.
        assert_eq!(s.paso(&m, &posado), Avance::Sigue);
    }

    /// El umbral de aprobado tiene que ser inalcanzable sin completar: es lo
    /// que hace que el currículo suba por aprender y no por aburrirse.
    #[test]
    fn el_umbral_solo_se_paga_completando() {
        let _escenarios = escenarios_para_mi();
        for p in 0..PELDANOS {
            let puntos = misiones(p)[0].puntos.len() as f32;
            assert_eq!(umbral_aprobado(p), LLEGADA * puntos + COMPLETA);
        }
    }
}

#[cfg(test)]
mod sin_gps {
    use super::*;
    use std::sync::atomic::Ordering;

    /// Ida y vuelta de `largo` metros sin postes, con el clásico. Devuelve
    /// tramos cumplidos y distancia final media (verdad) sobre 12 rumbos.
    fn ida_y_vuelta(largo: f32, ruido: Ruido) -> (usize, f32) {
        let (mut hechos, mut suma) = (0usize, 0.0f32);
        for k in 0..12u32 {
            let a = k as f32 * std::f32::consts::TAU / 12.0 + 0.3;
            let m = Mision {
                nombre: "larga",
                partida: (0.0, 0.0, 3.0),
                desde_suelo: false,
                puntos: vec![Punto::aire(largo * a.cos(), largo * a.sin(), 3.0), Punto::aire(0.0, 0.0, 3.0)],
                obstaculos: Vec::new(),
                semilla: 9000 + k,
                pasos: (largo * 2.0 / 3.0 * 200.0) as usize + 4000,
            };
            let inf = volar_con(Piloto::Clasico, &m, ruido, None);
            hechos += inf.puntos_hechos;
            suma += inf.dist_final;
        }
        (hechos, suma / 12.0)
    }

    /// **La baliza tiene que corregir la deriva donde la deriva importa.** En
    /// las misiones del gym (5-9 m) la deriva queda adentro del radio de
    /// llegada y la baliza no mueve la vara (bitácora 7.42). En una ruta de
    /// 40 m ida y vuelta, sin baliza el dron vuelve a donde CREE que está la
    /// base; con baliza, a donde la VE.
    #[test]
    fn la_baliza_corrige_la_deriva_en_rutas_largas() {
        let _g = crate::mision::escenarios_para_pruebas();
        crate::sim3d::BALIZA_ACTIVA.store(false, Ordering::Relaxed);
        let (h0, d0) = ida_y_vuelta(40.0, Ruido::DURO);
        crate::sim3d::BALIZA_ACTIVA.store(true, Ordering::Relaxed);
        let (h1, d1) = ida_y_vuelta(40.0, Ruido::DURO);
        println!("MEDICION 40 m ida y vuelta, ruido DURO: sin baliza {h0}/24 tramos, termina a {d0:.2} m; con baliza {h1}/24, {d1:.2} m");
        assert!(h1 >= h0 && d1 < d0, "la baliza no corrigió la deriva");
    }
}

#[cfg(test)]
mod piso_de_repaso {
    use super::*;

    /// El piso contra el clásico no puede hacer reprobar al propio clásico:
    /// en los peldaños que aprueba siempre (1-11, 48/48), su examen de repaso
    /// tiene que quedar arriba del umbral de hoy.
    #[test]
    fn el_clasico_pasa_su_propio_repaso() {
        let _g = crate::mision::escenarios_para_pruebas();
        for p in [3usize, 8, 10] {
            let t = terminos_de_repaso(Piloto::Clasico, p);
            let peor = t.iter().cloned().fold(f32::INFINITY, f32::min);
            assert!(peor >= umbral_aprobado(p),
                "peldaño {}: el clásico reprueba su propio repaso ({peor:.1} < {:.1})",
                p + 1, umbral_aprobado(p));
        }
    }
}

#[cfg(test)]
mod tareas_dichas {
    use super::*;

    #[test]
    fn el_lenguaje_de_tareas_arma_la_cadena_y_la_carga() {
        let p = tarea_desde_texto("despegar 3; ir 6 0; posar; agarrar 0.3; despegar 2; ir 0 0 4; posar; soltar", (0.0, 0.0)).unwrap();
        assert_eq!(p.len(), 6);
        assert_eq!((p[1].x, p[1].y, p[1].z), (6.0, 0.0, 3.0), "`ir` sin altura usa la de crucero");
        assert!(p[2].posar && p[2].x == 6.0, "`posar` sin coordenadas se posa donde está");
        assert_eq!(p[2].accion, Accion::Agarrar(0.3));
        assert_eq!(p[4].z, 4.0);
        let m = mision_de_tarea("despegar 3; ir 6 0; posar; agarrar 0.3; despegar 2; ir 0 0 4; posar; soltar", (0.0, 0.0), Vec::new()).unwrap();
        assert_eq!(carga_tras(&m, 2), 0.0);
        assert_eq!(carga_tras(&m, 3), 0.3, "después de posarse y agarrar lleva la caja");
        assert_eq!(carga_tras(&m, 6), 0.0, "después de soltar ya no");
    }

    /// Varias cajas: cada una se agarra ANTES de soltarse, todas se
    /// entregan, y con `volver` termina posado en la partida.
    #[test]
    fn varias_cajas_se_entregan_todas_y_vuelve() {
        let p = tarea_desde_texto("caja 6 2 a -4 5; caja -3 -4 a 8 -3; caja 2 7 a 9 4; volver", (0.0, 0.0)).unwrap();
        let agarres: Vec<(f32, f32)> = p.iter().filter(|q| matches!(q.accion, Accion::Agarrar(_))).map(|q| (q.x, q.y)).collect();
        let sueltas: Vec<(f32, f32)> = p.iter().filter(|q| q.accion == Accion::Soltar).map(|q| (q.x, q.y)).collect();
        assert_eq!(agarres.len(), 3);
        assert_eq!(sueltas.len(), 3);
        for (c, d) in [((6.0, 2.0), (-4.0, 5.0)), ((-3.0, -4.0), (8.0, -3.0)), ((2.0, 7.0), (9.0, 4.0))] {
            let i = agarres.iter().position(|&a| a == c).expect("falta agarrar una caja");
            assert_eq!(sueltas[i], d, "la caja de {c:?} tiene que soltarse en {d:?}");
        }
        let fin = p.last().unwrap();
        assert!(fin.posar && fin.x == 0.0 && fin.y == 0.0, "con `volver` termina posado en la partida");
        let m = mision_de_tarea("caja 6 2 a -4 5; volver", (0.0, 0.0), Vec::new()).unwrap();
        assert!(m.planifica(), "las tareas dichas vuelan con mapa y ruta");
    }

    /// Con el mapa de un viaje anterior, una caja cerca en línea recta pero
    /// detrás de una pared de postes deja de ser «la más cercana».
    #[test]
    fn el_mapa_conocido_cambia_el_orden() {
        let mut mapa = crate::plan::Mapa::default();
        for k in -4..=4 {
            for _ in 0..5 {
                mapa.registrar(1.5, k as f32 * 0.8, 0.4);
            }
        }
        let texto = "caja 3 0 a 3 1; caja -4 0 a -4 1";
        let primera = |p: &[Punto]| p.iter().find(|q| matches!(q.accion, Accion::Agarrar(_))).map(|q| (q.x, q.y)).unwrap();
        assert_eq!(primera(&tarea_con_mapa(texto, (0.0, 0.0), None).unwrap()), (3.0, 0.0),
            "sin mapa, la más cercana en línea recta");
        assert_eq!(primera(&tarea_con_mapa(texto, (0.0, 0.0), Some(&mapa)).unwrap()), (-4.0, 0.0),
            "con la pared en el mapa, la otra queda más cerca por RUTA");
    }

    #[test]
    fn una_orden_mal_dicha_se_rechaza_con_el_motivo() {
        assert!(tarea_desde_texto("volar 3", (0.0, 0.0)).unwrap_err().contains("no conozco"));
        assert!(tarea_desde_texto("ir 3", (0.0, 0.0)).is_err());
        assert!(tarea_desde_texto("agarrar", (0.0, 0.0)).unwrap_err().contains("necesita un punto"));
        assert!(tarea_desde_texto("despegar 3; agarrar 5", (0.0, 0.0)).is_err(), "5 kg no lo levanta");
        assert!(tarea_desde_texto("", (0.0, 0.0)).is_err());
    }

    /// Una tarea que NADIE entrenó —ir a buscar una caja, llevarla a otro
    /// lado, volver— la cumple el clásico con la caja puesta y sin GPS.
    #[test]
    fn una_tarea_nueva_con_caja_se_cumple() {
        let _g = crate::mision::escenarios_para_pruebas();
        let m = mision_de_tarea(
            "despegar 3; ir 7 2; posar; agarrar 0.3; despegar 3; ir -4 5; posar; soltar; despegar 3; ir 0 0; posar",
            (0.0, 0.0), Vec::new()).unwrap();
        let inf = volar_con(Piloto::Clasico, &m, Ruido::REAL, None);
        assert_eq!(inf.puntos_hechos, m.puntos.len(), "cumplió {} de {}", inf.puntos_hechos, m.puntos.len());
    }
}
