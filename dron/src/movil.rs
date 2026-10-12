//! El objetivo que se mueve: una moto, un auto, una persona corriendo.
//!
//! # Por qué esto no es una lista de puntos
//!
//! Hasta acá todo objetivo del proyecto era un `Punto` con `x, y, z` fijos. Con
//! eso se puede entrenar "ir a" y "quedarse quieto", pero **no se puede
//! entrenar seguir**, y seguir es otro problema, no el mismo más difícil:
//!
//! - Ir a un punto es un problema de REGULACIÓN: el error tiende a cero y el
//!   dron se queda. Frenar al llegar es la maniobra.
//! - Seguir un móvil es un problema de PERSECUCIÓN: el error NUNCA tiende a
//!   cero por sí solo, porque el objetivo se va. Hay que igualar su velocidad,
//!   no anular la distancia. Un controlador que sólo mira la posición siempre
//!   va atrás — el retardo se convierte en distancia, y la distancia crece con
//!   la velocidad del objetivo.
//!
//! La consecuencia práctica: para seguir a 25 m/s hay que ANTICIPAR. El dron
//! tiene que apuntar a donde la moto VA a estar, no a donde está. Eso exige que
//! la velocidad del objetivo sea una entrada del piloto, y por eso existe este
//! archivo.
//!
//! # Por qué una moto no es un punto que se traslada
//!
//! Una moto es **no holónoma**: va hacia donde apunta y no puede desplazarse de
//! costado. Y tiene límites: acelera hasta ~7 m/s², frena hasta ~8, y en curva
//! no puede pasar de ~0.8 g de aceleración lateral sin caerse. Todo eso hace
//! que su trayectoria sea PREDECIBLE a corto plazo, que es justamente lo que un
//! perseguidor puede explotar. Un objetivo que se teletransporta al azar no
//! tiene esa estructura y entrenar contra él enseñaría a no anticipar.

use crate::sim3d::DT;

/// Aceleración longitudinal máxima de una moto de calle, m/s². 0-100 km/h en
/// unos 4 s da 6.9; se redondea a 7.
pub const ACEL_MAX: f32 = 7.0;
/// Frenada máxima. Frena mejor de lo que acelera, como cualquier vehículo.
pub const FRENO_MAX: f32 = 8.0;
/// Aceleración lateral máxima en curva: 0.8 g. Más que eso y se cae. Es lo que
/// impone el radio mínimo de giro: r = v²/a_lat, así que a 25 m/s el radio más
/// cerrado que puede tomar son 80 m.
pub const ACEL_LATERAL_MAX: f32 = 7.85;
/// Techo de velocidad del móvil. 33 m/s = 119 km/h. Por encima de esto el dron
/// (techo 31.8 m/s) no la alcanza ni en línea recta, y una misión así sería
/// imposible por construcción en vez de difícil.
pub const VEL_MAX: f32 = 33.0;

/// Radio de giro más cerrado que la GEOMETRÍA permite, en metros. Una moto
/// haciendo una U completa a paso de hombre gira en unos 3 m.
///
/// Existe porque el límite de adherencia solo (`a_lat/v`) es una hipérbola que
/// tiende a infinito cuando la velocidad tiende a cero — y eso decía que a
/// 1 m/s la moto puede girar a 7.85 rad/s, o sea **450 grados por segundo**.
/// Medido antes de este arreglo, arrancando de cero con el manubrio a fondo:
///
/// ```text
///   rapidez 3.54 m/s   giro 4.41 rad/s   (253°/s: daba vueltas como un trompo)
/// ```
///
/// A baja velocidad el que manda no es el agarre, es cuánto se puede doblar el
/// manubrio: ω = v / R_min. El cruce entre los dos límites está en
/// √(a_lat · R_min) = 4.85 m/s — abajo manda la geometría, arriba el agarre.
/// Eso es la cinemática de bicicleta bien puesta, y es lo que hace que
/// maniobrar despacio se sienta como una moto y no como un helicóptero.
pub const RADIO_MIN: f32 = 3.0;

/// Qué está haciendo el conductor ahora mismo.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Maniobra {
    /// Mantener velocidad.
    Crucero,
    /// Acelerar hasta la velocidad objetivo.
    Acelerar(f32),
    /// Frenar hasta la velocidad objetivo (0 = semáforo).
    Frenar(f32),
    /// Girar con un radio dado (positivo izquierda, negativo derecha), en m.
    Curva(f32),
    /// Detenido. El semáforo, el peaje, el tráfico.
    Parado,
}

/// Un tramo del recorrido: qué hace y por cuánto tiempo.
#[derive(Clone, Copy, Debug)]
pub struct Tramo {
    pub maniobra: Maniobra,
    pub segundos: f32,
}

/// Lo que hace una persona con los controles: acelerador y manubrio, cada uno
/// de −1 a 1.
///
/// **Es lo que se graba, y a propósito no se graba la trayectoria.** Guardar
/// posiciones sería guardar un resultado; guardar los MANDOS es guardar la
/// intención, y reproducirlos por la misma cinemática da exactamente el mismo
/// recorrido, siempre. Eso es lo que convierte "manejé un rato" en un caso de
/// entrenamiento reproducible en vez de una anécdota.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Mando {
    /// Positivo acelera, negativo frena. −1 es frenada de emergencia.
    pub acelerador: f32,
    /// Positivo izquierda, negativo derecha. ±1 es el giro más cerrado que la
    /// adherencia permite A ESTA VELOCIDAD, no un ángulo fijo de manubrio —
    /// por eso a 100 km/h el mismo ±1 abre la curva solo, como en la realidad.
    pub giro: f32,
}

/// Cada cuántos pasos de física se muestrea un mando al grabar. 10 pasos a
/// 200 Hz son 20 Hz de muestreo: más que suficiente para la mano de una
/// persona (que no pasa de unos pocos Hz) y barato de guardar — un minuto de
/// manejo son 1200 muestras.
pub const PASOS_POR_MUESTRA: usize = 10;

/// De dónde salen las órdenes de este móvil.
#[derive(Clone, Debug)]
enum Fuente {
    /// Los recorridos sintéticos de `recorridos::`.
    Guion,
    /// Un recorrido manejado por una persona, reproducido paso a paso.
    Grabacion { mandos: Vec<Mando> },
    /// Alguien lo está manejando AHORA (el visor).
    Vivo(Mando),
}

/// El móvil. Cinemática de bicicleta simplificada: avanza hacia donde apunta,
/// con velocidad y rumbo acotados por lo que la física del vehículo permite.
#[derive(Clone, Debug)]
pub struct Movil {
    pub x: f32,
    pub y: f32,
    /// Los móviles de este proyecto van por el suelo. `z` existe porque una
    /// carretera sube y baja, y un dron que sigue a altura constante sobre un
    /// puente se estrella contra el puente.
    pub z: f32,
    /// Rumbo, rad. Hacia dónde apunta y por lo tanto hacia dónde se mueve.
    pub rumbo: f32,
    /// Rapidez escalar, m/s. Siempre ≥ 0: una moto no anda para atrás.
    pub rapidez: f32,
    /// Velocidad de giro actual, rad/s. Se guarda para que el rumbo no salte.
    pub giro: f32,
    /// Aceleración longitudinal (dv/dt) y angular (dω/dt) del paso anterior.
    ///
    /// Existen porque el punto de persecución que va detrás del móvil TIENE
    /// aceleración propia —centrípeta sobre todo— y un controlador que no la
    /// conoce sólo puede reaccionar tarde. Medido: en curva, el error del
    /// dron es 5× más LATERAL que longitudinal (se abre hacia afuera, no se
    /// queda atrás), que es exactamente la firma de una aceleración
    /// centrípeta no compensada.
    pub acel: f32,
    pub giro_punto: f32,
    guion: Vec<Tramo>,
    /// Índice del tramo (con guion) o de la muestra (con grabación).
    tramo: usize,
    t_tramo: f32,
    fuente: Fuente,
    /// Contador de submuestreo para grabar/reproducir a `PASOS_POR_MUESTRA`.
    sub: usize,
    /// Pendiente del terreno bajo el móvil, rad. Es lo que hace subir a `z`.
    pendiente: f32,
}

impl Movil {
    /// Un móvil parado en un punto, mirando hacia `rumbo`, con el guion dado.
    pub fn nuevo(x: f32, y: f32, rumbo: f32, rapidez: f32, guion: Vec<Tramo>) -> Self {
        Movil {
            x, y, z: 0.0, rumbo, rapidez: rapidez.clamp(0.0, VEL_MAX),
            giro: 0.0, acel: 0.0, giro_punto: 0.0, guion, tramo: 0, t_tramo: 0.0,
            fuente: Fuente::Guion, sub: 0, pendiente: 0.0,
        }
    }

    /// Un móvil que maneja una persona en vivo. Arranca parado.
    pub fn en_vivo(x: f32, y: f32, rumbo: f32) -> Self {
        Movil {
            x, y, z: 0.0, rumbo, rapidez: 0.0, giro: 0.0, acel: 0.0, giro_punto: 0.0,
            guion: Vec::new(), tramo: 0, t_tramo: 0.0,
            fuente: Fuente::Vivo(Mando::default()), sub: 0, pendiente: 0.0,
        }
    }

    /// Un móvil que reproduce lo que manejó una persona. Determinista: los
    /// mismos mandos por la misma cinemática dan el mismo recorrido.
    pub fn grabado(mandos: Vec<Mando>) -> Self {
        Movil {
            x: 0.0, y: 0.0, z: 0.0, rumbo: 0.0, rapidez: 0.0, giro: 0.0,
            acel: 0.0, giro_punto: 0.0,
            guion: Vec::new(), tramo: 0, t_tramo: 0.0,
            fuente: Fuente::Grabacion { mandos }, sub: 0, pendiente: 0.0,
        }
    }

    /// Le pasa los controles del momento a un móvil en vivo. Lo llama el visor
    /// con lo que toca el usuario.
    pub fn manejar(&mut self, mando: Mando) {
        if let Fuente::Vivo(m) = &mut self.fuente {
            *m = Mando {
                acelerador: mando.acelerador.clamp(-1.0, 1.0),
                giro: mando.giro.clamp(-1.0, 1.0),
            };
        }
    }

    /// El mando que tiene puesto ahora mismo (para grabarlo).
    pub fn mando_actual(&self) -> Mando {
        match &self.fuente {
            Fuente::Vivo(m) => *m,
            _ => Mando::default(),
        }
    }

    /// La tasa de giro máxima a la velocidad actual: el MENOR de los dos
    /// límites reales de una moto — el agarre (a_lat/v, manda rápido) y la
    /// geometría de dirección (v/R_min, manda despacio). Ver `RADIO_MIN`.
    fn techo_de_giro(&self) -> f32 {
        let por_agarre = if self.rapidez > 0.1 { ACEL_LATERAL_MAX / self.rapidez } else { 0.0 };
        let por_geometria = self.rapidez / RADIO_MIN;
        por_agarre.min(por_geometria)
    }

    /// Convierte lo que pide una persona en (rapidez objetivo, giro objetivo).
    /// Es el único lugar donde "apretar el acelerador" se vuelve física.
    fn desde_mando(&self, m: Mando) -> (f32, f32) {
        // Acelerador: hacia el techo si empuja, hacia cero si frena. El
        // objetivo es una VELOCIDAD y el límite de aceleración lo aplica el
        // paso, así que soltar el acelerador deja rodar (no frena en seco).
        let objetivo = if m.acelerador >= 0.0 {
            self.rapidez + m.acelerador * ACEL_MAX * 0.5 // hacia adelante
        } else {
            self.rapidez + m.acelerador * FRENO_MAX * 0.5 // frenando
        }
        .clamp(0.0, VEL_MAX);
        // Manubrio: ±1 es el giro más cerrado que la moto da a ESTA velocidad
        // (agarre arriba, geometría abajo — ver `techo_de_giro`). Por eso no
        // hay que "aprender" a no volcarse en las curvas rápidas ni a no girar
        // como un trompo parado: la moto no deja pedir lo imposible, igual que
        // la de verdad.
        (objetivo, m.giro * self.techo_de_giro())
    }

    /// La tasa de giro que corresponde a un radio pedido, recortada por lo que
    /// la adherencia permite a la velocidad actual.
    fn giro_desde_radio(&self, r: f32) -> f32 {
        let deseado = self.rapidez / r; // rad/s con signo
        let techo = self.techo_de_giro();
        deseado.clamp(-techo, techo)
    }

    fn avanzar_grabacion(&mut self) {
        self.sub += 1;
        if self.sub >= PASOS_POR_MUESTRA {
            self.sub = 0;
            self.tramo += 1;
        }
    }

    /// Velocidad en el marco del mundo. Es lo que el perseguidor necesita
    /// medir para anticipar.
    pub fn velocidad(&self) -> (f32, f32, f32) {
        (self.rapidez * self.rumbo.cos(),
         self.rapidez * self.rumbo.sin(),
         self.rapidez * self.pendiente.sin())
    }



    pub fn maniobra(&self) -> Maniobra {
        self.guion.get(self.tramo).map_or(Maniobra::Crucero, |t| t.maniobra)
    }

    /// Se terminó el guion: el recorrido llegó a su fin.
    ///
    /// Un móvil manejado en vivo NUNCA termina — lo termina la persona cuando
    /// suelta los controles, no el reloj.
    pub fn termino(&self) -> bool {
        match &self.fuente {
            Fuente::Guion => self.tramo >= self.guion.len(),
            Fuente::Grabacion { mandos, .. } => self.tramo >= mandos.len(),
            Fuente::Vivo(_) => false,
        }
    }

    /// Un paso de simulación, al mismo `DT` que el dron.
    pub fn paso(&mut self) {
        // De dónde sale lo que la moto va a hacer este paso. Las tres fuentes
        // terminan produciendo lo MISMO —una aceleración deseada y una tasa de
        // giro deseada— y de ahí para abajo la física es idéntica. Eso es lo
        // que hace que un recorrido manejado por una persona sea un ciudadano
        // de primera en el gym y no un caso especial.
        let (objetivo, giro_objetivo) = match self.fuente.clone() {
            Fuente::Guion => {
                let m = self.maniobra();
                self.avanzar_guion();
                let objetivo = match m {
                    Maniobra::Crucero | Maniobra::Curva(_) => self.rapidez,
                    Maniobra::Acelerar(v) => v.clamp(0.0, VEL_MAX),
                    Maniobra::Frenar(v) => v.clamp(0.0, VEL_MAX),
                    Maniobra::Parado => 0.0,
                };
                let giro = match m {
                    Maniobra::Curva(r) if r.abs() > 0.5 => self.giro_desde_radio(r),
                    _ => 0.0,
                };
                (objetivo, giro)
            }
            Fuente::Grabacion { mandos, .. } => {
                let mando = mandos.get(self.tramo).copied().unwrap_or_default();
                self.avanzar_grabacion();
                self.desde_mando(mando)
            }
            Fuente::Vivo(mando) => self.desde_mando(mando),
        };

        // --- longitudinal ---
        // El límite de aceleración/frenada se aplica ACÁ, una sola vez, sea
        // cual sea la fuente: ni el guion ni una persona pueden hacer que la
        // moto cambie de velocidad más rápido de lo que la física permite.
        let dv = objetivo - self.rapidez;
        let limite = if dv >= 0.0 { ACEL_MAX } else { FRENO_MAX } * DT;
        let rapidez_antes = self.rapidez;
        self.rapidez = (self.rapidez + dv.clamp(-limite, limite)).clamp(0.0, VEL_MAX);
        // Las derivadas reales de este paso: es lo que el perseguidor necesita
        // para compensar la aceleración del punto que persigue en vez de
        // reaccionar a ella (ver `acel` / `giro_punto` en la estructura).
        self.acel = (self.rapidez - rapidez_antes) / DT;

        // --- lateral ---
        // El rumbo no salta: la moto se inclina para entrar en curva y eso
        // tarda. 0.25 s de constante de tiempo es lo que tarda un motociclista
        // en tumbar la moto.
        let giro_antes = self.giro;
        self.giro += (giro_objetivo - self.giro) * (DT / 0.25).min(1.0);
        self.giro_punto = (self.giro - giro_antes) / DT;
        self.rumbo += self.giro * DT;
        self.rumbo = (self.rumbo + std::f32::consts::PI)
            .rem_euclid(std::f32::consts::TAU) - std::f32::consts::PI;

        // --- traslación ---
        let (vx, vy, vz) = self.velocidad();
        self.x += vx * DT;
        self.y += vy * DT;
        self.z = (self.z + vz * DT).max(0.0);
    }

    fn avanzar_guion(&mut self) {
        if self.termino() {
            return;
        }
        self.t_tramo += DT;
        if self.t_tramo >= self.guion[self.tramo].segundos {
            self.t_tramo = 0.0;
            self.tramo += 1;
        }
    }

    /// Le pone pendiente al terreno: la moto sube o baja un desnivel. Un dron
    /// que sigue a altura absoluta constante choca con el puente.
    pub fn con_pendiente(mut self, rad: f32) -> Self {
        self.pendiente = rad;
        self
    }
}

/// Recorridos de verdad, no ruido.
///
/// Cada uno es una situación que existe y que rompe a un perseguidor distinto.
/// El orden es de menos a más difícil y es el que usa el currículo.
/// Guardar y leer lo que manejó una persona.
///
/// Formato trivial a propósito, igual que el genoma bio: una línea por
/// muestra, `acelerador giro`. Se puede mirar con `cat`, versionar en git y
/// arreglar a mano si hace falta. Un minuto de manejo son 1200 líneas de
/// ~12 bytes: 14 KB. No vale la pena nada más complicado.
pub mod grabacion {
    use super::Mando;

    pub fn guardar(mandos: &[Mando], ruta: &str) -> std::io::Result<()> {
        if let Some(dir) = std::path::Path::new(ruta).parent() {
            std::fs::create_dir_all(dir)?;
        }
        let mut s = String::with_capacity(mandos.len() * 12);
        for m in mandos {
            s.push_str(&format!("{:.4} {:.4}\n", m.acelerador, m.giro));
        }
        std::fs::write(ruta, s)
    }

    pub fn cargar(ruta: &str) -> std::io::Result<Vec<Mando>> {
        let texto = std::fs::read_to_string(ruta)?;
        Ok(texto
            .lines()
            .filter_map(|l| {
                let mut it = l.split_whitespace();
                Some(Mando {
                    acelerador: it.next()?.parse().ok()?,
                    giro: it.next()?.parse().ok()?,
                })
            })
            .collect())
    }

    /// Todas las grabaciones que haya en `recorridos/`, ordenadas por nombre.
    /// Es lo que el entrenador suma al currículo: cada vez que el usuario
    /// maneja y guarda, el dron tiene un caso real más contra el que medirse.
    pub fn todas(dir: &str) -> Vec<(String, Vec<Mando>)> {
        let Ok(entradas) = std::fs::read_dir(dir) else { return Vec::new() };
        let mut v: Vec<_> = entradas
            .filter_map(|e| e.ok())
            .map(|e| e.path())
            .filter(|p| p.extension().is_some_and(|x| x == "mandos"))
            .collect();
        v.sort();
        v.iter()
            .filter_map(|p| {
                let nombre = p.file_stem()?.to_string_lossy().to_string();
                let mandos = cargar(p.to_str()?).ok()?;
                if mandos.is_empty() { None } else { Some((nombre, mandos)) }
            })
            .collect()
    }
}

pub mod recorridos {
    use super::*;

    /// Recta a velocidad de ciudad. El caso fácil: un perseguidor que iguale la
    /// velocidad lo resuelve. Existe para separar "no sabe seguir" de "no sabe
    /// anticipar".
    pub fn recta_ciudad() -> Movil {
        Movil::nuevo(0.0, 0.0, 0.0, 11.0, vec![
            Tramo { maniobra: Maniobra::Crucero, segundos: 40.0 },
        ])
    }

    /// Recta de carretera, 25 m/s (90 km/h). Mismo problema, pero ahora el
    /// retardo del lazo cuesta 25 m por segundo de atraso en vez de 11.
    pub fn recta_carretera() -> Movil {
        Movil::nuevo(0.0, 0.0, 0.0, 25.0, vec![
            Tramo { maniobra: Maniobra::Crucero, segundos: 40.0 },
        ])
    }

    /// Semáforo: crucero, frena a cero, espera, arranca fuerte.
    ///
    /// Rompe al perseguidor que aprendió a igualar velocidad y nada más: cuando
    /// la moto frena, él sigue de largo y se la pasa. Es el equivalente en
    /// persecución del "no frena al llegar" que tenía el genoma en A→B.
    pub fn semaforo() -> Movil {
        Movil::nuevo(0.0, 0.0, 0.0, 14.0, vec![
            Tramo { maniobra: Maniobra::Crucero, segundos: 6.0 },
            Tramo { maniobra: Maniobra::Frenar(0.0), segundos: 4.0 },
            Tramo { maniobra: Maniobra::Parado, segundos: 5.0 },
            Tramo { maniobra: Maniobra::Acelerar(20.0), segundos: 8.0 },
            Tramo { maniobra: Maniobra::Crucero, segundos: 17.0 },
        ])
    }

    /// Curva larga a velocidad de carretera. La extrapolación recta se equivoca
    /// y hay que usar el giro, no sólo la velocidad.
    pub fn curva_carretera() -> Movil {
        Movil::nuevo(0.0, 0.0, 0.0, 22.0, vec![
            Tramo { maniobra: Maniobra::Crucero, segundos: 5.0 },
            Tramo { maniobra: Maniobra::Curva(90.0), segundos: 14.0 },
            Tramo { maniobra: Maniobra::Crucero, segundos: 4.0 },
            Tramo { maniobra: Maniobra::Curva(-70.0), segundos: 12.0 },
            Tramo { maniobra: Maniobra::Crucero, segundos: 5.0 },
        ])
    }

    /// Zigzag entre autos: curvas cortas alternadas a velocidad media. Es el
    /// caso que castiga al perseguidor que se pega demasiado — si copia cada
    /// maniobra, oscila.
    pub fn esquivando_trafico() -> Movil {
        let c = |r: f32| Tramo { maniobra: Maniobra::Curva(r), segundos: 1.6 };
        Movil::nuevo(0.0, 0.0, 0.0, 16.0, vec![
            Tramo { maniobra: Maniobra::Crucero, segundos: 3.0 },
            c(28.0), c(-28.0), c(-28.0), c(28.0),
            Tramo { maniobra: Maniobra::Crucero, segundos: 3.0 },
            c(-24.0), c(24.0), c(24.0), c(-24.0),
            Tramo { maniobra: Maniobra::Crucero, segundos: 6.0 },
        ])
    }

    /// El recorrido completo, con todo junto y con desnivel. Es el examen.
    pub fn ruta_completa() -> Movil {
        Movil::nuevo(0.0, 0.0, 0.0, 12.0, vec![
            Tramo { maniobra: Maniobra::Acelerar(24.0), segundos: 7.0 },
            Tramo { maniobra: Maniobra::Curva(110.0), segundos: 9.0 },
            Tramo { maniobra: Maniobra::Frenar(9.0), segundos: 4.0 },
            Tramo { maniobra: Maniobra::Curva(-30.0), segundos: 5.0 },
            Tramo { maniobra: Maniobra::Acelerar(28.0), segundos: 8.0 },
            Tramo { maniobra: Maniobra::Crucero, segundos: 7.0 },
        ]).con_pendiente(0.05)
    }

    /// Los seis, en orden de dificultad.
    pub fn todos() -> Vec<(&'static str, fn() -> Movil)> {
        vec![
            ("recta de ciudad",     recta_ciudad as fn() -> Movil),
            ("recta de carretera",  recta_carretera),
            ("semáforo",            semaforo),
            ("curva de carretera",  curva_carretera),
            ("zigzag en tráfico",   esquivando_trafico),
            ("ruta completa",       ruta_completa),
        ]
    }
}

#[cfg(test)]
mod pruebas {
    use super::*;

    fn correr(mut m: Movil, segundos: f32) -> Movil {
        for _ in 0..(segundos / DT) as u32 {
            m.paso();
        }
        m
    }

    /// Lo más básico: una moto en recta avanza velocidad × tiempo.
    #[test]
    fn la_recta_es_una_recta() {
        let m = correr(recorridos::recta_carretera(), 10.0);
        assert!((m.x - 250.0).abs() < 1.0, "10 s a 25 m/s son 250 m, dio {:.1}", m.x);
        assert!(m.y.abs() < 0.01, "una recta no se va de costado, dio {:.3}", m.y);
    }

    /// Una moto NO puede tomar una curva más cerrada de lo que la adherencia
    /// permite. Si se le pide un radio imposible, abre la curva — no se
    /// teletransporta.
    #[test]
    fn la_curva_imposible_se_abre_sola() {
        // A 30 m/s, 10 m de radio pide 90 m/s² de aceleración lateral: 9 g.
        let m = Movil::nuevo(0.0, 0.0, 0.0, 30.0, vec![
            Tramo { maniobra: Maniobra::Curva(10.0), segundos: 5.0 },
        ]);
        let m = correr(m, 3.0);
        let lateral = m.rapidez * m.giro.abs();
        assert!(lateral <= ACEL_LATERAL_MAX * 1.05,
            "la aceleración lateral no puede pasar de {ACEL_LATERAL_MAX:.1} m/s², dio {lateral:.1}");
    }

    /// El semáforo tiene que llegar a cero de verdad. Si no frenara, la misión
    /// de persecución no probaría nada nuevo respecto de seguir en recta.
    #[test]
    fn el_semaforo_para_del_todo() {
        let m = correr(recorridos::semaforo(), 12.0);
        assert!(m.rapidez < 0.1, "a los 12 s tiene que estar parada, va a {:.2} m/s", m.rapidez);
        let m = correr(m, 10.0);
        assert!(m.rapidez > 15.0, "y después arrancar, va a {:.2} m/s", m.rapidez);
    }

    /// La frenada respeta el límite físico: nada de parar en seco.
    #[test]
    fn frenar_lleva_su_tiempo() {
        let m = Movil::nuevo(0.0, 0.0, 0.0, 24.0, vec![
            Tramo { maniobra: Maniobra::Frenar(0.0), segundos: 10.0 },
        ]);
        // 24 m/s a 8 m/s² son 3 s como mínimo.
        let a_2s = correr(m.clone(), 2.0);
        assert!(a_2s.rapidez > 5.0,
            "a los 2 s todavía no puede estar parada, va a {:.1}", a_2s.rapidez);
        let a_4s = correr(m, 4.0);
        assert!(a_4s.rapidez < 0.1, "a los 4 s sí, va a {:.1}", a_4s.rapidez);
    }



    /// Manejar en vivo tiene que mover la moto de verdad: acelerador a fondo
    /// acelera, freno a fondo para, manubrio gira. Es la prueba tonta que
    /// evita que el visor tenga controles que no hacen nada.
    #[test]
    fn los_controles_en_vivo_manejan_la_moto() {
        let mut m = Movil::en_vivo(0.0, 0.0, 0.0);
        m.manejar(Mando { acelerador: 1.0, giro: 0.0 });
        for _ in 0..1000 { m.paso(); }           // 5 s a fondo
        assert!(m.rapidez > 15.0, "5 s de acelerador tienen que dar velocidad, dio {:.1}", m.rapidez);
        let v = m.rapidez;

        m.manejar(Mando { acelerador: -1.0, giro: 0.0 });
        for _ in 0..1000 { m.paso(); }           // 5 s de freno
        assert!(m.rapidez < v * 0.2, "5 s de freno tienen que frenarla, quedó en {:.1}", m.rapidez);

        m.manejar(Mando { acelerador: 1.0, giro: 1.0 });
        let rumbo0 = m.rumbo;
        for _ in 0..600 { m.paso(); }
        assert!((m.rumbo - rumbo0).abs() > 0.3, "el manubrio tiene que girarla");
    }

    /// **La propiedad de la que depende que el manejo sirva para entrenar.**
    /// Reproducir los mandos grabados tiene que dar EXACTAMENTE el mismo
    /// recorrido, siempre. Si no, lo grabado no es un caso de entrenamiento
    /// reproducible sino una anécdota, y el gym estaría midiendo contra algo
    /// distinto cada vez que corre.
    #[test]
    fn reproducir_lo_grabado_da_el_mismo_recorrido() {
        // Alguien maneja: acelera, dobla, frena, dobla al otro lado.
        let guion = [
            (Mando { acelerador: 1.0,  giro:  0.0 }, 40),
            (Mando { acelerador: 0.3,  giro:  0.8 }, 30),
            (Mando { acelerador: -0.6, giro: -0.5 }, 25),
            (Mando { acelerador: 0.9,  giro:  0.2 }, 35),
        ];
        let mut vivo = Movil::en_vivo(0.0, 0.0, 0.0);
        let mut grabado = Vec::new();
        for (mando, muestras) in guion {
            vivo.manejar(mando);
            for _ in 0..muestras {
                // Una muestra = PASOS_POR_MUESTRA pasos de física, igual que
                // como lo graba el visor.
                grabado.push(vivo.mando_actual());
                for _ in 0..PASOS_POR_MUESTRA { vivo.paso(); }
            }
        }

        let mut repro = Movil::grabado(grabado.clone());
        while !repro.termino() { repro.paso(); }

        assert!((repro.x - vivo.x).abs() < 0.01 && (repro.y - vivo.y).abs() < 0.01,
            "reproducir tiene que caer en el mismo punto: grabado ({:.2},{:.2}) vs vivo ({:.2},{:.2})",
            repro.x, repro.y, vivo.x, vivo.y);
        assert!((repro.rapidez - vivo.rapidez).abs() < 0.01,
            "y a la misma velocidad: {:.2} vs {:.2}", repro.rapidez, vivo.rapidez);

        // Y dos reproducciones de lo mismo tienen que ser idénticas entre sí.
        let mut otra = Movil::grabado(grabado);
        while !otra.termino() { otra.paso(); }
        assert_eq!((otra.x.to_bits(), otra.y.to_bits()), (repro.x.to_bits(), repro.y.to_bits()),
            "dos reproducciones de la misma grabación tienen que dar bit a bit lo mismo");
    }

    /// Ida y vuelta por disco sin perder nada apreciable.
    #[test]
    fn guardar_y_leer_una_grabacion() {
        let mandos = vec![
            Mando { acelerador: 0.5, giro: -0.25 },
            Mando { acelerador: -1.0, giro: 0.75 },
            Mando { acelerador: 0.0, giro: 0.0 },
        ];
        let ruta = std::env::temp_dir().join("prueba_grabacion.mandos");
        let ruta = ruta.to_str().unwrap();
        grabacion::guardar(&mandos, ruta).unwrap();
        let leidos = grabacion::cargar(ruta).unwrap();
        assert_eq!(leidos.len(), mandos.len());
        for (a, b) in leidos.iter().zip(&mandos) {
            assert!((a.acelerador - b.acelerador).abs() < 1e-3);
            assert!((a.giro - b.giro).abs() < 1e-3);
        }
        let _ = std::fs::remove_file(ruta);
    }

    /// El dron tiene que poder alcanzarla. Si el móvil más rápido supera el
    /// techo del dron, la misión es imposible por construcción y entrenar
    /// contra ella enseña a rendirse.
    #[test]
    fn ningun_recorrido_es_inalcanzable() {
        let techo_dron = crate::sim3d::velocidad_techo(crate::sim3d::inclinacion_sostenible() * 0.99);
        for (nombre, hacer) in recorridos::todos() {
            let mut m = hacer();
            let mut pico: f32 = 0.0;
            while !m.termino() {
                m.paso();
                pico = pico.max(m.rapidez);
            }
            assert!(pico < techo_dron * 0.95,
                "«{nombre}» llega a {pico:.1} m/s y el dron sólo da {techo_dron:.1}: \
                 la misión sería imposible, no difícil");
        }
    }
}

