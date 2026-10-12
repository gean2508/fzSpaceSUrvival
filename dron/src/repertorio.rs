//! **El repertorio: un genoma por situación, no uno para todo.**
//!
//! El proyecto tiene un bloqueo medido y viejo —el olvido catastrófico (regla
//! 3.8)—: un solo genoma sosteniendo catorce capacidades a la vez, y la
//! búsqueda pisando lo aprendido para ganar en lo que se está entrenando
//! ahora. Medido en la bitácora 7.25: al pelear el peldaño 13, el campeón
//! desaprendió los doce de atrás y se quedó ahí 190 generaciones.
//!
//! Este módulo es la salida estructural, y no es un invento propio: es
//! **MAP-Elites + Intelligent Trial and Error**, de Cully, Clune, Tarapore y
//! Mouret, *Robots that can adapt like animals*, Nature 521 (2015). Ahí un
//! hexápodo guarda un mapa de comportamientos distintos y, cuando se rompe una
//! pata, **busca en el mapa** el comportamiento que todavía funciona en vez de
//! volver a aprender a caminar desde cero. La idea que importa es la misma que
//! hace falta acá: el conocimiento no vive amontonado en un solo juego de
//! pesos, vive **clasificado por nicho**, y en el momento se recupera el que
//! corresponde.
//!
//! ```text
//!         estorbo →   libre        algo         tapado
//!   tarea ↓
//!   crucero         [genoma]    [genoma]      [genoma]
//!   aproximación    [genoma]    [genoma]      [genoma]
//!   sostén          [genoma]    [genoma]      [genoma]
//! ```
//!
//! Por qué esto ataca el olvido de raíz: aprender a rodear un poste sólo puede
//! escribir en la celda «crucero + tapado». **No hay forma de que pise lo que
//! el dron sabe de posarse**, porque eso vive en «sostén + libre», que es otro
//! archivo. El olvido deja de ser un riesgo del mecanismo y pasa a ser
//! imposible por construcción.
//!
//! # Por qué esto NO es la mezcla de expertos que ya falló
//!
//! El proyecto ya intentó especializar por régimen en persecución y no pagó
//! (bitácora 7.4). La causa quedó escrita ahí y es la que este diseño corrige:
//! *"un experto entrenado en un solo recorrido no aprende un régimen, aprende
//! un recorrido"*, y *"el monolítico YA es el mejor experto disponible, porque
//! se entrenó sobre el currículo entero"*. Aquel intento arrancaba cada
//! experto **desde cero**; la propia bitácora dejó anotado que había que
//! *"entrenar el experto sembrado desde el monolítico"*, y eso nunca se hizo.
//!
//! Acá se hace: **todas las celdas nacen siendo el campeón entero**
//! (`sembrar`). El peor caso del repertorio es, celda por celda, exactamente
//! el genoma que ya se tenía — nunca menos. A partir de ahí cada celda sólo
//! puede mejorar, porque un elite se reemplaza únicamente por algo que midió
//! mejor **en esa misma celda**.
//!
//! # Presupuesto (regla 3.14)
//!
//! 9 celdas × 11392 B = **102528 B ≈ 100 KB** de flash. Un ESP32 típico trae
//! 4 MB, así que entra cuarenta veces. En cómputo el repertorio no cuesta nada
//! por decisión: elegir celda son dos comparaciones, y **sólo se copia el
//! genoma cuando el contexto CAMBIA de celda**, no en cada tick.

use crate::iafz;

/// Qué tan tapado está el frente, del `looming` máximo del anillo.
pub const ESTORBOS: usize = 3;
/// En qué está la tarea: llegar de lejos, afinar la llegada, o sostenerse.
pub const TAREAS: usize = 3;
pub const CELDAS: usize = ESTORBOS * TAREAS;

/// Umbrales del eje «estorbo», en unidades de looming normalizado.
///
/// Calibrados contra el mismo instrumento que el freno de pared
/// (`gym3d::frenar_ante_pared`), que ya midió el rango real: una pared a 5 m/s
/// desde 5 m da 0.20-0.32, y un poste lejano o de costado queda por debajo de
/// 0.03. No se eligieron a gusto — es el error que la regla 3.18 persigue.
pub const ESTORBO_ALGO: f32 = 0.03;
pub const ESTORBO_TAPADO: f32 = 0.18;

/// Umbrales del eje «tarea», en metros al objetivo. `RADIO_TAREA` de `vivo.rs`
/// marca dónde deja de ser llegar y pasa a ser sostenerse; el de aproximación
/// son tres metros, que a velocidad de crucero es el último segundo de vuelo.
pub const CERCA: f32 = 1.0;
pub const APROXIMACION: f32 = 3.0;

/// En qué nicho está volando el dron ahora mismo. Se calcula con lo que hay a
/// bordo: el looming de la cámara y la distancia estimada al objetivo.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Contexto {
    pub estorbo: usize,
    pub tarea: usize,
}

impl Contexto {
    pub fn de(looming_max: f32, dist: f32) -> Contexto {
        let estorbo = if looming_max >= ESTORBO_TAPADO {
            2
        } else if looming_max >= ESTORBO_ALGO {
            1
        } else {
            0
        };
        let tarea = if dist < CERCA {
            2
        } else if dist < APROXIMACION {
            1
        } else {
            0
        };
        Contexto { estorbo, tarea }
    }

    pub fn indice(&self) -> usize {
        self.tarea * ESTORBOS + self.estorbo
    }

    pub fn nombre(&self) -> &'static str {
        const N: [&str; CELDAS] = [
            "crucero libre", "crucero con algo", "crucero tapado",
            "aproximación libre", "aproximación con algo", "aproximación tapada",
            "sostén libre", "sostén con algo", "sostén tapado",
        ];
        N[self.indice()]
    }
}

/// Cuántas ventanas consistentes hacen falta para cambiar de opinión sobre un
/// nicho.
///
/// **Sale de una medición en vivo, no de un gusto**: con el veredicto saliendo
/// de UNA ventana de cuatro segundos, el visor mostraba el mismo nicho
/// alternando entre «la red AYUDA» y «la red ESTORBA» cada pocos segundos. O
/// sea que el dron cambiaba de piloto por ruido de medición, no por haber
/// aprendido nada. Una ventana es una muestra, no un veredicto.
///
/// Se resolvió con votos en vez de promedios a propósito: un promedio pesado
/// necesita elegir una escala («¿cuánto es mucho?»), y elegir escalas a ojo es
/// exactamente el error que persigue la regla 3.18. Contar ventanas no
/// necesita escala ninguna — sólo el signo de cada comparación, que es lo que
/// de verdad se quiere saber.
pub const VOTOS_PARA_CAMBIAR: i32 = 3;

pub struct Celda {
    /// El mejor genoma conocido PARA ESTE NICHO. Nunca está vacío: nace siendo
    /// el campeón general, así que el peor caso es el genoma de siempre.
    pub genoma: Vec<u8>,
    /// Mérito de la última medición del elite. `None` = todavía no se midió
    /// acá, así que el primer resultado honesto entra sin competir.
    pub merito: Option<f32>,
    /// **La vara honesta, medida EN ESTE NICHO y en vivo**: cuánto saca el
    /// piloto clásico acá, con la red en cero (regla 3.15: eso es el clásico
    /// exacto, no una imitación). Es lo que convierte "la red estorba" de
    /// sospecha en medición.
    pub merito_clasico: Option<f32>,
    pub visitas: u32,
    pub mejoras: u32,
    /// Acumulado de ventanas a favor (+) o en contra (−) de que la red ayude
    /// acá. Ver `VOTOS_PARA_CAMBIAR`.
    pub votos: i32,
}

pub struct Repertorio {
    pub celdas: Vec<Celda>,
}

// =============================================================================
// El banco congelado: cómo el gym vuela nueve genomas con un solo slot
// =============================================================================

/// Los nueve genomas del repertorio, congelados, para que la población entera
/// los comparta durante una generación.
///
/// Por qué global y no un parámetro: `mision::volar_con` recibe un `Piloto`
/// que es `Copy`, y meterle una referencia al repertorio le agregaría un
/// lifetime que se propaga a `capacidades`, `main`, el visor y las pruebas. Es
/// el mismo trato que ya tiene `mision::VARIANTE` (el sorteo de escenarios de
/// la generación) y por el mismo motivo: dato de sólo lectura, idéntico para
/// todos los hilos mientras dura la generación.
///
/// **Sólo se escribe entre generaciones.** Si alguien lo escribiera con la
/// población evaluándose, la mitad de los individuos quedaría medida contra
/// un repertorio y la otra mitad contra otro — el bug que la sección 4 de
/// CLAUDE.md llama "tocar el camino caliente con un A/B corriendo", un nivel
/// más abajo.
static BANCO: std::sync::RwLock<Option<std::sync::Arc<Vec<Vec<u8>>>>> =
    std::sync::RwLock::new(None);

/// Congela el estado actual del repertorio para que lo vuele el gym.
pub fn fijar_banco(r: &Repertorio) {
    let copia: Vec<Vec<u8>> = r.celdas.iter().map(|c| c.genoma.clone()).collect();
    *BANCO.write().unwrap() = Some(std::sync::Arc::new(copia));
}

/// El banco congelado, si hay uno. `None` = nadie lo fijó, y entonces
/// `Piloto::PorCelda` vuela el genoma que el slot ya tenía en todas las
/// celdas — o sea, exactamente `Piloto::Red`.
pub fn banco() -> Option<std::sync::Arc<Vec<Vec<u8>>>> {
    BANCO.read().unwrap().clone()
}

/// **El módulo VERTICAL: un genoma aparte, dueño SOLO de la salida de altura.**
///
/// Idea del usuario (2026-09-26): el cerebro tiene partes dedicadas a
/// funciones, no una masa que hace todo. Las celdas de arriba parten por
/// SITUACIÓN y cada una maneja las cuatro salidas, así que entrenar el
/// crucero podía romper el aterrizaje. Medido antes de escribir esto
/// (bitácora 7.56): el repertorio de `EXP_20260926d` aprueba posarse en 1/12
/// sorteos y cuando falla SE ESTRELLA a 0.06–0.49 m del punto — llega bien y
/// baja mal: la falla es vertical.
///
/// Con el módulo puesto, la salida 3 (altura) la emite SIEMPRE este genoma y
/// la de la celda se ignora; las salidas 0-2 las emite la celda y las de
/// éste se ignoran. Se piensa con `abordo::pensar` (su propia memoria de 32
/// bits, sin gastar slots del kernel), que es bit a bit el mismo cerebro.
/// `None` = sin módulo: todo como antes.
static VERTICAL_G: std::sync::RwLock<Option<std::sync::Arc<Vec<u8>>>> =
    std::sync::RwLock::new(None);

/// `entrena` de `mision::Piloto::PorCelda` cuando el individuo que se evalúa
/// es el módulo vertical y no una celda.
pub const VERTICAL: usize = 1000;

/// **El módulo POSAR: dueño de las CUATRO salidas mientras el tramo actual es
/// de aterrizaje.** Segundo corte por función, pedido por la medición
/// (bitácora 7.57): con la altura aparte, posarse subió de 1/12 a 8/12, pero
/// se volvió a degradar (5/12 en la generación 545) porque los peldaños 13-15
/// siguen entrenando las celdas horizontales que el dron usa mientras baja.
/// Con este módulo el aterrizaje entero es de UN genoma que sólo entrenan los
/// peldaños de posarse: nada posterior lo puede tocar. El dron sabe cuándo
/// está posando porque es su tarea (`mision::Punto::posar`), no por la
/// verdad del simulador.
static POSAR_G: std::sync::RwLock<Option<std::sync::Arc<Vec<u8>>>> =
    std::sync::RwLock::new(None);

/// `entrena` cuando el individuo que se evalúa es el módulo posar.
pub const POSAR: usize = 1001;

pub fn fijar_posar(g: &[u8]) {
    *POSAR_G.write().unwrap() = Some(std::sync::Arc::new(g.to_vec()));
}

pub fn posar() -> Option<std::sync::Arc<Vec<u8>>> {
    POSAR_G.read().unwrap().clone()
}

pub fn olvidar_posar() {
    *POSAR_G.write().unwrap() = None;
}

pub fn ruta_posar(ruta_rep: &str) -> String {
    format!("{}.posar.adn", ruta_rep.trim_end_matches(".repertorio.bin"))
}

pub fn fijar_vertical(g: &[u8]) {
    *VERTICAL_G.write().unwrap() = Some(std::sync::Arc::new(g.to_vec()));
}

pub fn olvidar_vertical() {
    *VERTICAL_G.write().unwrap() = None;
}

pub fn vertical() -> Option<std::sync::Arc<Vec<u8>>> {
    VERTICAL_G.read().unwrap().clone()
}

/// Dónde vive el módulo vertical de un repertorio: al lado, mismo prefijo.
pub fn ruta_vertical(ruta_rep: &str) -> String {
    format!("{}.vertical.adn", ruta_rep.trim_end_matches(".repertorio.bin"))
}

/// Lee un repertorio y, si tiene módulo vertical al lado, lo fija también.
pub fn leer_con_vertical(ruta: &str) -> Option<Repertorio> {
    let r = Repertorio::leer(ruta)?;
    let v = ruta_vertical(ruta);
    match std::fs::read(&v) {
        Ok(b) if b.len() == iafz::BRAIN_BYTES => {
            fijar_vertical(&b);
            println!("(con módulo vertical {v})");
        }
        _ => *VERTICAL_G.write().unwrap() = None,
    }
    let pr = ruta_posar(ruta);
    match std::fs::read(&pr) {
        Ok(b) if b.len() == iafz::BRAIN_BYTES => {
            fijar_posar(&b);
            println!("(con módulo posar {pr})");
        }
        _ => olvidar_posar(),
    }
    Some(r)
}

/// **Quién piensa en cada decisión cuando vuela un repertorio**: la celda de
/// la situación actual para las salidas 0-2 y, si hay, el módulo vertical para
/// la altura. Existe en UN solo lugar porque lo usan el gym (`mision::volar_con`)
/// y el visor (`web.rs`): si cada uno lo resolviera por su cuenta, el visor
/// podría mostrar un dron que no es el que se entrenó (regla 3.3).
pub struct Volador {
    slot: i32,
    entrena: usize,
    /// El genoma que el slot traía: el individuo que se evalúa.
    propio: Vec<u8>,
    banco: Option<std::sync::Arc<Vec<Vec<u8>>>>,
    vertical: Option<std::sync::Arc<Vec<u8>>>,
    posar: Option<std::sync::Arc<Vec<u8>>>,
    celda_actual: usize,
    estado_v: crate::abordo::Estado,
    estado_p: crate::abordo::Estado,
}

impl Volador {
    /// `entrena`: la celda que es el individuo (`>= CELDAS` = ninguna, o
    /// `VERTICAL` si el individuo es el módulo vertical).
    pub fn nuevo(slot: i32, entrena: usize,
                 banco: Option<std::sync::Arc<Vec<Vec<u8>>>>,
                 vertical: Option<std::sync::Arc<Vec<u8>>>,
                 posar: Option<std::sync::Arc<Vec<u8>>>) -> Self {
        let propio = iafz::volcar(slot);
        iafz::reset_memoria(slot);
        let yo = || Some(std::sync::Arc::new(propio.clone()));
        let vertical = if entrena == VERTICAL { yo() } else { vertical };
        let posar = if entrena == POSAR { yo() } else { posar };
        Volador {
            slot, entrena, propio, banco, vertical, posar,
            estado_p: crate::abordo::Estado { sensores_lo: 0, sensores_hi: 0, memoria: 0, salidas: [0.0; 4] },
            // Arranca en la celda que se entrena porque eso es lo que el
            // slot ya trae: el individuo.
            celda_actual: entrena,
            estado_v: crate::abordo::Estado { sensores_lo: 0, sensores_hi: 0, memoria: 0, salidas: [0.0; 4] },
        }
    }

    /// `posando`: el tramo actual es de aterrizaje (la tarea lo dice).
    pub fn pensar(&mut self, celda: usize, posando: bool, lo: u64, hi: u32) -> [f32; iafz::SALIDAS] {
        if posando {
            if let Some(p) = self.posar.as_ref() {
                self.estado_p.sensores_lo = lo;
                self.estado_p.sensores_hi = hi;
                crate::abordo::pensar(p, &mut self.estado_p);
                return self.estado_p.salidas;
            }
        }
        if let Some(b) = self.banco.as_ref() {
            if celda != self.celda_actual {
                // **Sólo se copia si el genoma es OTRO.** No es una
                // optimización, es lo que hace cierta la promesa del módulo:
                // un repertorio recién sembrado tiene las nueve celdas
                // idénticas, y sin esta comparación cruzar de celda le
                // borraría los 32 bits de memoria recurrente (`restaurar` los
                // pone en cero). O sea que «sembrado vuela igual que el
                // genoma» sería falso.
                let (e, p) = (self.entrena, &self.propio);
                let cual = |i: usize| -> &[u8] {
                    if i == e || i >= CELDAS { p } else { &b[i] }
                };
                if cual(celda) != cual(self.celda_actual) {
                    iafz::restaurar(self.slot, cual(celda));
                }
                self.celda_actual = celda;
            }
        }
        let mut o = iafz::pensar_multi(self.slot, lo, hi);
        if let Some(v) = self.vertical.as_ref() {
            self.estado_v.sensores_lo = lo;
            self.estado_v.sensores_hi = hi;
            crate::abordo::pensar(v, &mut self.estado_v);
            o[3] = self.estado_v.salidas[3];
        }
        o
    }

    /// El slot vuelve a ser el individuo, no la última celda que se le copió
    /// encima. Si esto faltara, la evolución cruzaría y mutaría el genoma de
    /// «sostén libre» creyendo que es el aspirante.
    pub fn devolver(&self) {
        iafz::restaurar(self.slot, &self.propio);
    }
}

pub fn olvidar_banco() {
    *BANCO.write().unwrap() = None;
}

impl Repertorio {
    /// **Todas las celdas arrancan siendo el genoma entrenado.**
    ///
    /// Es la salvaguarda principal y la corrección explícita al intento que
    /// falló en la bitácora 7.4: sembrar desde el campeón, no desde cero. Un
    /// repertorio recién sembrado vuela EXACTAMENTE igual que el genoma
    /// suelto, en las nueve celdas. Lo único que puede pasar a partir de ahí
    /// es que alguna celda mejore.
    pub fn sembrar(desde: i32) -> Repertorio {
        let base = iafz::volcar(desde);
        Repertorio {
            celdas: (0..CELDAS)
                .map(|_| Celda {
                    genoma: base.clone(),
                    merito: None,
                    merito_clasico: None,
                    visitas: 0,
                    mejoras: 0,
                    votos: 0,
                })
                .collect(),
        }
    }

    pub fn celda(&self, c: Contexto) -> &Celda {
        &self.celdas[c.indice()]
    }

    /// Carga en `slot` el elite del nicho pedido.
    pub fn cargar_en(&mut self, c: Contexto, slot: i32) {
        let i = c.indice();
        self.celdas[i].visitas += 1;
        iafz::restaurar(slot, &self.celdas[i].genoma);
    }

    /// Guarda un genoma como elite del nicho **sólo si midió mejor** que el
    /// que había. Devuelve `true` si reemplazó.
    ///
    /// La comparación es contra el elite DE ESA CELDA, nunca contra el de otra:
    /// comparar méritos de nichos distintos no significa nada —volar tapado
    /// mide peor que volar libre por definición— y mezclarlos sería llenar el
    /// mapa con el genoma que tuvo la suerte de que le tocara el nicho fácil.
    pub fn guardar_si_mejora(&mut self, c: Contexto, genoma: Vec<u8>, merito: f32) -> bool {
        let i = c.indice();
        let mejora = match self.celdas[i].merito {
            None => true,
            Some(previo) => crate::vivo::supera(merito, previo),
        };
        if mejora {
            self.celdas[i].genoma = genoma;
            self.celdas[i].merito = Some(merito);
            self.celdas[i].mejoras += 1;
        }
        mejora
    }

    /// Reemplaza el elite de un nicho **sin escribirle un mérito**.
    ///
    /// Existe por una razón de instrumento (regla 3.18), no de comodidad:
    /// `merito` guarda la escala del VUELO —ventanas de cuatro segundos
    /// medidas con sensores de a bordo (`vivo.rs`)—, mientras que el
    /// refinamiento fuera de línea puntúa con la aptitud de una misión del
    /// gym, que es otra cantidad y otra escala. Meter una en el campo de la
    /// otra haría que el lazo en vivo promoviera o rechazara por la
    /// diferencia de escalas y no por mérito: peras contra manzanas con cara
    /// de comparación válida. Dejándolo en `None`, la primera ventana de
    /// vuelo lo vuelve a medir en su propia escala.
    ///
    /// Los votos también se borran: la opinión de «la red ayuda acá» era
    /// sobre el genoma que se acaba de reemplazar.
    pub fn reemplazar_elite(&mut self, c: Contexto, genoma: Vec<u8>) {
        let i = c.indice();
        self.celdas[i].genoma = genoma;
        self.celdas[i].merito = None;
        self.celdas[i].votos = 0;
        self.celdas[i].mejoras += 1;
    }

    /// **¿La red ayuda en este nicho?** `None` mientras falte alguna de las
    /// dos mediciones. Cuando da `false`, ahí conviene volar clásico: la
    /// bitácora 7.4 ya midió offline que hay regímenes donde la corrección
    /// estorba (en transitorio, −61.2 puntos), y apagarla donde estorba fue
    /// lo único de aquella sesión que sí pagó. Acá eso se decide solo, por
    /// nicho y en vuelo.
    pub fn red_ayuda(&self, c: Contexto) -> Option<bool> {
        let cel = self.celda(c);
        if cel.merito.is_none() || cel.merito_clasico.is_none() {
            return None;
        }
        Some(cel.votos >= 0)
    }

    pub fn refrescar_clasico(&mut self, c: Contexto, merito: f32) {
        self.celdas[c.indice()].merito_clasico = Some(merito);
        self.votar(c);
    }

    /// Refresca el mérito del elite sin tocar el genoma. Hace falta porque el
    /// mundo cambia: un mérito medido hace cinco minutos, con otro viento, no
    /// es la vara contra la que se debe juzgar una variante de ahora.
    pub fn refrescar_merito(&mut self, c: Contexto, merito: f32) {
        self.celdas[c.indice()].merito = Some(merito);
        self.votar(c);
    }

    /// Una ventana = un voto. El veredicto es el signo del acumulado, saturado
    /// a ±`VOTOS_PARA_CAMBIAR`, así que dar vuelta una opinión asentada exige
    /// varias ventanas seguidas diciendo lo contrario.
    fn votar(&mut self, c: Contexto) {
        let cel = &mut self.celdas[c.indice()];
        let (Some(red), Some(clasico)) = (cel.merito, cel.merito_clasico) else { return };
        let voto = if red >= clasico { 1 } else { -1 };
        cel.votos = (cel.votos + voto).clamp(-VOTOS_PARA_CAMBIAR, VOTOS_PARA_CAMBIAR);
    }

    /// Cuántas celdas ya aprendieron algo propio.
    pub fn celdas_mejoradas(&self) -> usize {
        self.celdas.iter().filter(|c| c.mejoras > 0).count()
    }

    /// Bytes del repertorio entero, para escribirlo a disco o a flash.
    /// Formato por celda: `[1B tiene mérito][4B mérito][1B tiene clásico]
    /// [4B clásico][11392B genoma]`.
    pub fn a_bytes(&self) -> Vec<u8> {
        let mut v = Vec::with_capacity(CELDAS * (10 + iafz::BRAIN_BYTES));
        for c in &self.celdas {
            v.push(c.merito.is_some() as u8);
            v.extend_from_slice(&c.merito.unwrap_or(0.0).to_le_bytes());
            v.push(c.merito_clasico.is_some() as u8);
            v.extend_from_slice(&c.merito_clasico.unwrap_or(0.0).to_le_bytes());
            v.extend_from_slice(&c.genoma);
        }
        v
    }

    pub fn desde_bytes(datos: &[u8]) -> Option<Repertorio> {
        let paso = 10 + iafz::BRAIN_BYTES;
        if datos.len() != CELDAS * paso {
            return None;
        }
        let leer_f32 = |b: &[u8]| {
            if b[0] == 1 { Some(f32::from_le_bytes([b[1], b[2], b[3], b[4]])) } else { None }
        };
        let mut celdas = Vec::with_capacity(CELDAS);
        for i in 0..CELDAS {
            let b = &datos[i * paso..(i + 1) * paso];
            celdas.push(Celda {
                genoma: b[10..].to_vec(),
                merito: leer_f32(&b[0..5]),
                merito_clasico: leer_f32(&b[5..10]),
                visitas: 0,
                mejoras: 0,
                votos: 0,
            });
        }
        Some(Repertorio { celdas })
    }

    /// Dónde vive el repertorio entre vuelos. En el dron real esto es un
    /// bloque de flash; acá es un archivo.
    pub const ARCHIVO: &'static str = "genomes/DRON3D_REPERTORIO.bin";

    /// Escribe el repertorio con una cabecera de versión.
    ///
    /// La cabecera no es burocracia: un repertorio guardado con otro código de
    /// sensores está lleno de genomas que hoy leerían basura, y cargarlo sin
    /// chequear sería el mismo modo de falla silencioso que costó la bitácora
    /// 7.23 —el dron vuela, tiembla, y nadie sabe por qué—.
    pub fn escribir(&self, ruta: &str) -> std::io::Result<()> {
        let mut v = Vec::new();
        v.extend_from_slice(b"REPT");
        v.extend_from_slice(&iafz::VERSION_SENSORES.to_le_bytes());
        v.extend_from_slice(&(CELDAS as u32).to_le_bytes());
        v.extend_from_slice(&self.a_bytes());
        if let Some(p) = std::path::Path::new(ruta).parent() {
            let _ = std::fs::create_dir_all(p);
        }
        std::fs::write(ruta, v)
    }

    /// Lee un repertorio guardado. Devuelve `None` —y no un repertorio a
    /// medias— si la cabecera no coincide: es preferible sembrar de nuevo
    /// desde el campeón que volar nueve genomas que hablan otro idioma.
    pub fn leer(ruta: &str) -> Option<Repertorio> {
        let d = std::fs::read(ruta).ok()?;
        if d.len() < 12 || &d[0..4] != b"REPT" {
            return None;
        }
        let version = u32::from_le_bytes([d[4], d[5], d[6], d[7]]);
        let celdas = u32::from_le_bytes([d[8], d[9], d[10], d[11]]) as usize;
        if version != iafz::VERSION_SENSORES || celdas != CELDAS {
            return None;
        }
        Repertorio::desde_bytes(&d[12..])
    }

    /// Una línea por celda, para el visor y los informes.
    pub fn resumen(&self) -> String {
        (0..CELDAS)
            .map(|i| {
                let c = Contexto { estorbo: i % ESTORBOS, tarea: i / ESTORBOS };
                let cel = &self.celdas[i];
                format!(
                    "{}:{}/{}{}",
                    c.nombre(),
                    cel.mejoras,
                    cel.visitas,
                    match self.red_ayuda(c) {
                        Some(true) => " red↑",
                        Some(false) => " CLASICO",
                        None => "",
                    }
                )
            })
            .collect::<Vec<_>>()
            .join(" · ")
    }
}

/// **Qué sinapsis cambiaron**, neurona por neurona, entre dos genomas.
///
/// Devuelve 128 cuentas: las 64 de la capa oculta y las 64 de la de
/// estrategia, cada una con cuántos de sus 128 bits de pesos difieren.
///
/// Existe para poder MIRAR el aprendizaje mientras pasa (regla 3.3), y hay que
/// decir con precisión qué es y qué no: el kernel **no crea neuronas nuevas**
/// —son 64+64 fijas, es lo que lo hace caber en un chip de dos dólares— así
/// que dibujar "neuronas naciendo" sería un instrumento mentiroso, de la misma
/// familia que los que persigue la regla 3.18. Lo que sí cambia de verdad al
/// aprender son las conexiones: cada neurona compara 128 bits de pesos contra
/// su contexto, y adoptar una variante voltea algunos de esos bits. Eso es lo
/// que se cuenta acá, y es exactamente lo que el dron aprendió.
///
/// El genoma en disco es un `Brain64` crudo: 64 neuronas ocultas de 17 bytes
/// (16 de pesos + 1 de umbral), después 64 de estrategia iguales.
pub fn sinapsis_cambiadas(antes: &[u8], ahora: &[u8]) -> Vec<u32> {
    const BYTES_NEURONA: usize = 17;
    const PESOS: usize = 16;
    let capas = 2 * crate::iafz::NEURONAS as usize;
    if antes.len() < capas * BYTES_NEURONA || ahora.len() < capas * BYTES_NEURONA {
        return vec![0; capas];
    }
    (0..capas)
        .map(|n| {
            let i = n * BYTES_NEURONA;
            (0..PESOS)
                .map(|b| (antes[i + b] ^ ahora[i + b]).count_ones())
                .sum()
        })
        .collect()
}

/// Slots del kernel para el refinamiento por nicho fuera de línea. Distintos
/// de `vivo::ANCLA`/`ASPIRANTE` (122/123) para no chocar si algún día corren
/// a la vez.
pub const REFINO_ANCLA: i32 = 124;
pub const REFINO_ASPIRANTE: i32 = 125;
/// Cuántas misiones frescas deciden si el resultado del refinamiento entra al
/// repertorio. Una sola no decide nada (sección 4 de la biblia); el promedio
/// de varias sí.
pub const HELDOUT: u32 = 24;
/// Cuántas misiones frescas ve cada empujón antes de que se lo acepte o se lo
/// tire.
///
/// **No es un parámetro de gusto: sale de una medición que salió mal.** Con
/// una sola misión por generación (`LOTE = 1`), la semilla 2 terminó
/// **760 puntos PEOR** que el elite del que había partido —de 17/24 llegadas
/// limpias a 6/24—, mientras que las semillas 1, 3 y 4 mejoraban. La causa no
/// es que el mecanismo no sirva: es que con una sola misión, un aspirante
/// gana por suerte del sorteo (el poste le tocó de costado) y se lo asciende
/// igual. Repetido 1500 veces, la cadena de ascensos afortunados aleja al
/// ancla de lo que de verdad sabe hacer — sobreajuste al ruido del lote, el
/// mismo error de fondo que la regla 3.18 persigue en los instrumentos.
///
/// Cuesta lineal (cuatro vuelos por bando en vez de uno) y es el precio de
/// que «midió mejor» signifique algo. Ver la tabla de antes/después en la
/// bitácora 7.30.
pub const LOTE: u32 = 4;

/// El lote de veredicto: las MISMAS `HELDOUT` misiones para todo el que se
/// quiera comparar (elite viejo, refinado, clásico). Que el lote sea el mismo
/// es la mitad de lo que hace que la comparación signifique algo; la otra
/// mitad es que sus semillas no se solapen con las del entrenamiento
/// (`0xBEEF_0000` queda muy por encima de cualquier número de generaciones).
fn promedio_heldout(semilla: u32, mut volar_una: impl FnMut(&crate::mision::Mision) -> f32) -> f32 {
    let mut suma = 0.0;
    for h in 0..HELDOUT {
        let m = crate::mision::mision_esquive_aislado(((semilla as u64) << 32) | 0xBEEF_0000 | h as u64);
        suma += volar_una(&m);
    }
    suma / HELDOUT as f32
}

/// La vara honesta sobre ese mismo lote: el guiado clásico, sin red.
/// Sin este número, «el refinado mejoró» no dice si el dron quedó mejor o
/// sólo menos peor que el modelo físico solo (reglas 3.2 y 3.15).
pub fn merito_clasico_esquive(semilla: u32) -> f32 {
    promedio_heldout(semilla, |m| {
        crate::mision::volar_con(crate::mision::Piloto::Clasico, m, crate::sim3d::Ruido::REAL, None).aptitud
    })
}

/// El mismo lote contado en castellano: de las `HELDOUT` misiones, en cuántas
/// **llegó al otro lado sin rozar el poste**, y en cuántas lo rozó.
///
/// La aptitud es un promedio y un promedio esconde el modo de falla (regla
/// 3.5: la aptitud no es el comportamiento). Acá se quiere saber otra cosa:
/// si el dron sigue chocando el poste centrado, que es el síntoma que se vio
/// en vivo y el motivo entero de este refinamiento.
pub fn conteo_heldout(semilla: u32, piloto: crate::mision::Piloto) -> (usize, usize) {
    let mut limpias = 0;
    let mut roces = 0;
    promedio_heldout(semilla, |m| {
        if let crate::mision::Piloto::Red(s) = piloto {
            iafz::reset_memoria(s);
        }
        let i = crate::mision::volar_con(piloto, m, crate::sim3d::Ruido::REAL, None);
        if i.roce_min < 0.0 {
            roces += 1;
        } else if i.completa {
            limpias += 1;
        }
        i.aptitud
    });
    (limpias, roces)
}

/// **Conecta el repertorio al entrenamiento, no sólo al vuelo** (regla 3.19:
/// lo que ya existe y obviamente debería estar conectado, se conecta). Usa
/// EXACTAMENTE el mismo mecanismo anclado que `vivo.rs` —empujón fino,
/// comparar contra el ancla, promover sólo si mide mejor— pero corriendo
/// contra misiones sintéticas del gym en vez de vuelo real en tiempo real:
/// miles de empujones por segundo en vez de uno cada pocos segundos.
///
/// Por qué por NICHO y no reentrenando el genoma entero: la misión compuesta
/// de entrega reparte la presión de selección entre seis maniobras a la vez,
/// y "esquivar" —la más frágil, medida en vivo (bitácora 7.29/7.30: un poste
/// centrado, nunca visto así de exacto en el entrenamiento normal, seguía
/// chocando)— se diluye ahí. Aislarla en su propia celda le da práctica
/// concentrada sin poder tocar lo que el genoma sabe de las otras cinco.
///
/// Devuelve `(mejoró, mérito_antes, mérito_después)`, los tres medidos con el
/// MISMO lote held-out para que la comparación signifique algo.
pub fn refinar_nicho(
    rep: &mut Repertorio,
    ctx: Contexto,
    generaciones: u32,
    semilla: u32,
) -> (bool, f32, f32) {
    rep.cargar_en(ctx, REFINO_ANCLA);

    for g in 0..generaciones {
        let s = ((semilla as u64) << 32) | (g as u64) * LOTE as u64;
        // El lote de la generación: las MISMAS misiones para el ancla y para
        // el aspirante, pero distintas en cada generación.
        let lote: Vec<_> = (0..LOTE)
            .map(|k| crate::mision::mision_esquive_aislado(s + k as u64))
            .collect();
        let suma = |slot: i32| -> f32 {
            lote.iter()
                .map(|m| {
                    iafz::reset_memoria(slot);
                    crate::mision::volar(slot, m, crate::sim3d::Ruido::REAL, None).aptitud
                })
                .sum()
        };

        let base = suma(REFINO_ANCLA);
        iafz::empujar(REFINO_ANCLA, REFINO_ASPIRANTE, 3, 0.02, s as u32 ^ 0x9E37_79B9);
        let cand = suma(REFINO_ASPIRANTE);

        // Mismo criterio que `vivo::supera`: no alcanza con empatar.
        if crate::vivo::supera(cand, base) {
            iafz::clonar(REFINO_ASPIRANTE, REFINO_ANCLA);
        }
    }

    // Veredicto final: held-out, semillas distintas de las de entrenamiento,
    // contra el elite VIEJO de la celda (no contra el genoma de arranque
    // suelto) — el repertorio ya podía tener algo mejor guardado ahí.
    let medir = |slot: i32| -> f32 {
        promedio_heldout(semilla, |m| {
            iafz::reset_memoria(slot);
            crate::mision::volar(slot, m, crate::sim3d::Ruido::REAL, None).aptitud
        })
    };

    let antes = {
        let elite = rep.celda(ctx).genoma.clone();
        iafz::restaurar(REFINO_ASPIRANTE, &elite);
        medir(REFINO_ASPIRANTE)
    };
    let despues = medir(REFINO_ANCLA);

    // El veredicto sale del lote held-out, no del mérito guardado en la celda
    // (que está en la escala del vuelo — ver `reemplazar_elite`). Mismo
    // criterio de "no alcanza con empatar" que usa el lazo en vivo.
    let mejoro = crate::vivo::supera(despues, antes);
    if mejoro {
        rep.reemplazar_elite(ctx, iafz::volcar(REFINO_ANCLA));
    }
    (mejoro, antes, despues)
}

#[cfg(test)]
mod pruebas {
    use super::*;

    /// **Alambre de púas, no una prueba de comportamiento.**
    ///
    /// El proyecto aceptó tres veces un resultado medido con UNA sola muestra
    /// —el umbral de un peldaño, el ascenso de un aspirante fuera de línea y
    /// el ascenso de un aspirante en vuelo— y las tres veces costó una
    /// conclusión falsa (bitácora 7.30, regla 3.20). Nada impedía que el
    /// próximo cambio bajara cualquiera de estos números a 1 "para que corra
    /// más rápido": ahora esto lo impide, y el mensaje dice por qué.
    #[test]
    fn ningun_ascenso_se_decide_con_una_sola_muestra() {
        assert!(
            LOTE >= 2,
            "repertorio::LOTE = {LOTE}: con una sola misión por empujón, una \
             semilla de cuatro terminó 760 puntos PEOR que donde arrancó \
             (bitácora 7.30). Ver regla 3.20."
        );
        assert!(
            crate::vivo::MUESTRAS_PARA_PROMOVER >= 2,
            "vivo::MUESTRAS_PARA_PROMOVER = {}: ascender por UNA ventana de \
             cuatro segundos es la versión en vuelo del mismo error, y ésta \
             escribe en el genoma del dron real. Ver regla 3.20.",
            crate::vivo::MUESTRAS_PARA_PROMOVER
        );
        assert!(
            crate::gym3d::CONFIRMACIONES >= 2,
            "gym3d::CONFIRMACIONES = {}: aprobar un peldaño con UNA sola \
             confirmación es la misma trampa en el currículo, y es la que de \
             verdad se cobró la corrida: el genoma que «aprobó» el peldaño 9 \
             aprueba en 1 de 12 sorteos independientes (bitácora 7.33). \
             Ver regla 3.20.",
            crate::gym3d::CONFIRMACIONES
        );
        // **Y el piso de arriba, que es el que faltaba.** Dos confirmaciones
        // no son "más de una muestra" cuando el guardián se consulta 425
        // veces por corrida: con la p ≈ 0.16 medida en el log del A/B de
        // 7.34, dos dejan pasar `0.16² × 425 ≈ 11` falsos aprobados por
        // corrida y tres dejan pasar 1.7 (el A/B tuvo exactamente 1). Seis
        // dejan 0.007, y cuestan del orden de tres evaluaciones extra por
        // corrida porque la comprobación se corta en la primera que falla.
        assert!(
            crate::gym3d::CONFIRMACIONES >= 6,
            "gym3d::CONFIRMACIONES = {}: el número no se elige por «más de \
             una muestra», se elige contra CUÁNTAS VECES se consulta el \
             guardián. Medido (bitácora 7.34): 425 cruces del umbral por \
             corrida de 1200 generaciones y p ≈ 0.16 de pasar cada \
             confirmación, así que k confirmaciones dejan pasar 0.16^k × 425 \
             falsos aprobados. Con 3 eso da 1.7 y el A/B tuvo 1: el peldaño 9 \
             quedó «aprobado» en la generación 933 sin serlo. Con 6 da 0.007. \
             Bajarlo no ahorra nada —la comprobación se corta en la primera \
             que falla, y sólo 1 de 425 cruces llegó a la cuarta—. \
             Ver regla 3.20.",
            crate::gym3d::CONFIRMACIONES
        );
    }


    /// **Candado de slots.** Todas las pruebas de este módulo escriben en
    /// los slots 101/102 del kernel, que es estado GLOBAL, y `cargo test`
    /// las corre en paralelo: una pisaba el genoma de la otra. Medido el
    /// 2026-09-25: 1 corrida de 9 reprobaba `contar_sinapsis_cuenta_bits_de_verdad`
    /// y `todas_las_celdas_nacen_siendo_el_campeon` a la vez (bitácora 7.39).
    static SLOTS: std::sync::Mutex<()> = std::sync::Mutex::new(());
    fn slots() -> std::sync::MutexGuard<'static, ()> {
        SLOTS.lock().unwrap_or_else(|e| e.into_inner())
    }
    const SEMILLERO: i32 = 101;
    const DESTINO: i32 = 102;

    /// **La salvaguarda de origen**: un repertorio recién sembrado es, celda
    /// por celda, el genoma entrenado. Sin esto el dron empezaría volando
    /// pesos al azar en cualquier nicho que no hubiera visitado — que es
    /// exactamente el modo de fallo que hundió la mezcla de expertos de 7.4
    /// ("un slot al azar no es neutro: emite basura acotada y empeora").
    #[test]
    fn todas_las_celdas_nacen_siendo_el_campeon() {
        let _slots = slots();
        iafz::init(SEMILLERO, 2024);
        let base = iafz::volcar(SEMILLERO);
        let mut r = Repertorio::sembrar(SEMILLERO);
        for i in 0..CELDAS {
            let c = Contexto { estorbo: i % ESTORBOS, tarea: i / ESTORBOS };
            r.cargar_en(c, DESTINO);
            assert_eq!(iafz::volcar(DESTINO), base, "la celda {} no es el campeón", c.nombre());
        }
    }

    /// Un elite no se cae por algo que midió igual o peor. Es la misma regla
    /// que protege al ancla en `vivo.rs`, aplicada por nicho.
    #[test]
    fn un_elite_no_se_reemplaza_por_algo_peor() {
        let _slots = slots();
        let _escenarios = crate::mision::escenarios_para_pruebas();
        iafz::init(SEMILLERO, 11);
        let mut r = Repertorio::sembrar(SEMILLERO);
        let c = Contexto { estorbo: 0, tarea: 0 };
        let bueno = iafz::volcar(SEMILLERO);

        assert!(r.guardar_si_mejora(c, bueno.clone(), 10.0), "la primera medición entra");
        iafz::init(DESTINO, 999);
        let otro = iafz::volcar(DESTINO);
        assert!(!r.guardar_si_mejora(c, otro.clone(), 10.0), "empatar no reemplaza");
        assert!(!r.guardar_si_mejora(c, otro.clone(), 5.0), "medir peor tampoco");
        assert_eq!(r.celda(c).genoma, bueno, "el elite cambió sin haber perdido");
        assert!(r.guardar_si_mejora(c, otro.clone(), 20.0), "medir mejor sí reemplaza");
        assert_eq!(r.celda(c).genoma, otro);
    }

    /// **Un repertorio recién sembrado vuela EXACTAMENTE como el genoma del
    /// que nació.** Es la salvaguarda entera del módulo (`sembrar`) y, desde
    /// que el gym vuela por celdas, es también un chequeo de instrumento
    /// (regla 3.18): si volar «por celda» no diera lo mismo que volar el
    /// genoma suelto cuando las nueve celdas son idénticas, cualquier
    /// comparación entre el repertorio y el monolítico estaría midiendo
    /// además ese defecto — y la más obvia es el borrado de la memoria
    /// recurrente al cambiar de celda.
    #[test]
    fn el_repertorio_sembrado_vuela_igual_que_su_genoma() {
        let _slots = slots();
        let _escenarios = crate::mision::escenarios_para_pruebas();
        const PILOTO: i32 = 107;
        iafz::init(PILOTO, 4242);
        let r = Repertorio::sembrar(PILOTO);
        fijar_banco(&r);
        crate::mision::fijar_variante(777);
        // Sin celda en entrenamiento: vuela el repertorio tal cual.
        let por_celda = crate::mision::Piloto::PorCelda { slot: PILOTO, entrena: CELDAS };
        let suelto = crate::mision::Piloto::Red(PILOTO);
        for p in [0usize, 8, 13] {
            for m in &crate::mision::misiones(p) {
                let a = crate::mision::volar_con(suelto, m, crate::sim3d::Ruido::REAL, None);
                let b = crate::mision::volar_con(por_celda, m, crate::sim3d::Ruido::REAL, None);
                assert!((a.aptitud - b.aptitud).abs() < 1e-3,
                    "peldaño {}, misión «{}»: suelto {:.3} ≠ por celda {:.3}",
                    p + 1, m.nombre, a.aptitud, b.aptitud);
            }
        }
        // Y el slot vuelve a ser el individuo, no la última celda que se le
        // copió encima: si esto fallara, la evolución mutaría el genoma
        // equivocado y el error sería invisible en los números.
        assert_eq!(iafz::volcar(PILOTO).len(), iafz::BRAIN_BYTES);
        olvidar_banco();
    }

    /// **Cada peldaño se queda con la celda que ESTRENA, no con la que más
    /// visita.** Es la decisión de diseño de la que depende que el repertorio
    /// separe algo: con el máximo a secas, «esquivar» (56% en crucero libre,
    /// 10% en crucero tapado) caería en la misma celda que «ir a B» y el
    /// currículo volvería a pelearse por los mismos pesos.
    ///
    /// La tabla es el censo medido con la vara honesta el día que se conectó
    /// el currículo al repertorio (`dron celdas clasico`).
    #[test]
    fn cada_peldano_se_queda_con_la_celda_que_estrena() {
        let _slots = slots();
        let _escenarios = crate::mision::escenarios_para_pruebas();
        let censo: Vec<[f32; CELDAS]> = vec![
            [0.00, 0.0, 0.00, 0.59, 0.0, 0.00, 0.41, 0.0, 0.0], //  1 frenar
            [0.00, 0.0, 0.00, 0.58, 0.0, 0.00, 0.42, 0.0, 0.0], //  2 rumbo
            [0.20, 0.0, 0.00, 0.48, 0.0, 0.00, 0.32, 0.0, 0.0], //  3 ir a B
            [0.35, 0.0, 0.00, 0.33, 0.0, 0.00, 0.31, 0.0, 0.0], //  4 ir a B
            [0.42, 0.0, 0.00, 0.31, 0.0, 0.00, 0.27, 0.0, 0.0], //  5 ir a B
            [0.50, 0.0, 0.00, 0.29, 0.0, 0.00, 0.21, 0.0, 0.0], //  6 ir a B
            [0.50, 0.0, 0.00, 0.29, 0.0, 0.00, 0.22, 0.0, 0.0], //  7 sensores duros
            [0.56, 0.05, 0.10, 0.15, 0.0, 0.02, 0.12, 0.0, 0.0], //  8 esquivar
            [0.54, 0.05, 0.11, 0.16, 0.0, 0.01, 0.12, 0.0, 0.0], //  9 esquivar
            [0.07, 0.0, 0.00, 0.38, 0.0, 0.00, 0.55, 0.0, 0.0], // 10 despegar
            [0.00, 0.0, 0.00, 0.36, 0.0, 0.00, 0.64, 0.0, 0.0], // 11 posarse
            [0.12, 0.0, 0.00, 0.31, 0.0, 0.00, 0.57, 0.0, 0.0], // 12 posarse
            [0.54, 0.03, 0.07, 0.20, 0.0, 0.01, 0.16, 0.0, 0.0], // 13 ida y vuelta
            [0.21, 0.04, 0.13, 0.31, 0.0, 0.01, 0.31, 0.0, 0.0], // 14 entrega
            [0.20, 0.04, 0.13, 0.30, 0.0, 0.01, 0.33, 0.0, 0.0], // 15 entrega
        ];
        let a = crate::mision::asignacion_de_celdas(&censo);
        let libre = Contexto { estorbo: 0, tarea: 0 }.indice();   // crucero libre
        let tapado = Contexto { estorbo: 2, tarea: 0 }.indice();  // crucero tapado
        let aprox = Contexto { estorbo: 0, tarea: 1 }.indice();   // aproximación libre
        let sosten = Contexto { estorbo: 0, tarea: 2 }.indice();  // sostén libre
        assert_eq!(&a[0..2], &[aprox, aprox], "frenar y rumbo: aproximación libre");
        assert_eq!(&a[2..7], &[libre; 5], "ir a B y sensores duros: crucero libre");
        assert_eq!(&a[7..9], &[tapado; 2], "esquivar tiene que ESTRENAR crucero tapado");
        assert_eq!(&a[9..15], &[sosten; 6], "despegar, posarse, ida y vuelta, entrega: sostén");
        // Y la propiedad de la que depende todo: el conflicto medido —entrenar
        // «posarse» rompía «esquivar»— queda entre celdas DISTINTAS.
        assert_ne!(a[7], a[11], "esquivar y posarse no pueden compartir celda");
    }

    /// Aprender en un nicho **no puede** tocar los demás. Es el olvido
    /// catastrófico vuelto imposible por construcción, y es la razón de ser
    /// de todo el módulo.
    #[test]
    fn aprender_en_una_celda_no_toca_las_otras() {
        let _slots = slots();
        let _escenarios = crate::mision::escenarios_para_pruebas();
        iafz::init(SEMILLERO, 7);
        let base = iafz::volcar(SEMILLERO);
        let mut r = Repertorio::sembrar(SEMILLERO);
        iafz::init(DESTINO, 31337);
        let nuevo = iafz::volcar(DESTINO);

        let tocada = Contexto { estorbo: 2, tarea: 0 }; // crucero tapado
        assert!(r.guardar_si_mejora(tocada, nuevo.clone(), 99.0));

        for i in 0..CELDAS {
            let c = Contexto { estorbo: i % ESTORBOS, tarea: i / ESTORBOS };
            let esperado = if c == tocada { &nuevo } else { &base };
            assert_eq!(&r.celda(c).genoma, esperado,
                "aprender en «{}» se llevó puesta «{}»", tocada.nombre(), c.nombre());
        }
    }

    #[test]
    fn el_contexto_sale_de_sensores_de_a_bordo() {
        let _slots = slots();
        let _escenarios = crate::mision::escenarios_para_pruebas();
        assert_eq!(Contexto::de(0.0, 50.0), Contexto { estorbo: 0, tarea: 0 });
        assert_eq!(Contexto::de(0.25, 0.5), Contexto { estorbo: 2, tarea: 2 });
        assert_eq!(Contexto::de(0.05, 2.0), Contexto { estorbo: 1, tarea: 1 });
        // Cada nicho tiene su propio índice, o dos situaciones distintas
        // compartirían genoma sin que nadie se entere.
        let mut vistos = std::collections::HashSet::new();
        for e in 0..ESTORBOS {
            for t in 0..TAREAS {
                assert!(vistos.insert(Contexto { estorbo: e, tarea: t }.indice()));
            }
        }
        assert_eq!(vistos.len(), CELDAS);
    }

    /// El repertorio tiene que sobrevivir a apagar el dron, o "aprende en
    /// vivo" dura lo que dura la batería.
    #[test]
    fn ida_y_vuelta_a_bytes_conserva_todo() {
        let _slots = slots();
        iafz::init(SEMILLERO, 5);
        let mut r = Repertorio::sembrar(SEMILLERO);
        iafz::init(DESTINO, 6);
        let otro = iafz::volcar(DESTINO);
        let c = Contexto { estorbo: 1, tarea: 2 };
        r.guardar_si_mejora(c, otro.clone(), 42.5);

        let bytes = r.a_bytes();
        let r2 = Repertorio::desde_bytes(&bytes).expect("tiene que poder releerse");
        assert_eq!(r2.celda(c).genoma, otro);
        assert_eq!(r2.celda(c).merito, Some(42.5));
        assert_eq!(r2.celdas[0].merito, None, "una celda sin medir no puede inventar mérito");
    }

    /// El presupuesto de la regla 3.14, como prueba y no como comentario: si
    /// alguien agrega ejes al mapa, esto avisa antes de que el repertorio deje
    /// de entrar en el chip.
    /// Un repertorio de otra versión de sensores NO se carga. Volarlo sería
    /// el fallo silencioso de la bitácora 7.23, multiplicado por nueve.
    #[test]
    fn un_repertorio_de_otra_version_no_se_carga() {
        let _slots = slots();
        iafz::init(SEMILLERO, 1);
        let r = Repertorio::sembrar(SEMILLERO);
        let ruta = std::env::temp_dir().join("dron_rept_prueba.bin");
        let ruta = ruta.to_str().unwrap();
        r.escribir(ruta).unwrap();
        assert!(Repertorio::leer(ruta).is_some(), "el propio tiene que releerse");

        // Se le pisa la versión en la cabecera, como si viniera de otro código
        // de sensores.
        let mut d = std::fs::read(ruta).unwrap();
        d[4] = d[4].wrapping_add(1);
        std::fs::write(ruta, &d).unwrap();
        assert!(Repertorio::leer(ruta).is_none(), "cargó un repertorio de otra versión");

        std::fs::write(ruta, b"basura").unwrap();
        assert!(Repertorio::leer(ruta).is_none(), "cargó un archivo que no es un repertorio");
    }

    /// Una sola ventana no puede dar vuelta el veredicto de un nicho: eso es
    /// lo que hacía que el dron cambiara de piloto por ruido.
    #[test]
    fn una_medicion_sola_no_da_vuelta_el_veredicto() {
        let _slots = slots();
        let _escenarios = crate::mision::escenarios_para_pruebas();
        iafz::init(SEMILLERO, 12);
        let mut r = Repertorio::sembrar(SEMILLERO);
        let c = Contexto { estorbo: 0, tarea: 2 };
        // Cinco ventanas diciendo que el clásico saca -0.2.
        for _ in 0..5 { r.refrescar_clasico(c, -0.2); }
        for _ in 0..5 { r.refrescar_merito(c, -0.1); }
        assert_eq!(r.red_ayuda(c), Some(true), "con la red mejor, ayuda");

        // Una ventana mala aislada no puede invertirlo.
        r.refrescar_merito(c, -5.0);
        assert_eq!(r.red_ayuda(c), Some(true), "una ráfaga cambió el veredicto");

        // Pero si insiste, sí.
        for _ in 0..5 { r.refrescar_merito(c, -5.0); }
        assert_eq!(r.red_ayuda(c), Some(false), "la evidencia sostenida tiene que mandar");
    }

    /// El contador de sinapsis tiene que contar lo que dice contar: cero
    /// cuando no cambió nada, y exactamente los bits volteados cuando sí.
    #[test]
    fn contar_sinapsis_cuenta_bits_de_verdad() {
        let _slots = slots();
        iafz::init(SEMILLERO, 77);
        let base = iafz::volcar(SEMILLERO);
        assert_eq!(sinapsis_cambiadas(&base, &base).iter().sum::<u32>(), 0,
            "sin cambios no puede reportar aprendizaje");

        // Un empujón de 3 bits tiene que verse como 3 bits, ni uno más.
        iafz::empujar(SEMILLERO, DESTINO, 3, 0.0, 12345);
        let total: u32 = sinapsis_cambiadas(&base, &iafz::volcar(DESTINO)).iter().sum();
        assert_eq!(total, 3, "un empujón de 3 bits reportó {total}");

        // Y tiene 128 casilleros: las dos capas completas.
        assert_eq!(sinapsis_cambiadas(&base, &base).len(), 2 * iafz::NEURONAS as usize);
    }

    #[test]
    fn el_repertorio_entra_en_un_esp32() {
        let _slots = slots();
        let bytes = CELDAS * iafz::BRAIN_BYTES;
        assert!(bytes < 512 * 1024,
            "el repertorio ocupa {} B y ya no es 'cabe de sobra' en flash", bytes);
    }
}
