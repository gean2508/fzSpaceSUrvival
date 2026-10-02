//! **DESCUBRIR DE VERDAD: experimentar, anotar, inducir, enseñar y equivocarse.**
//!
//! Los eruditos NO tiran un dado sobre una lista (eso es `ciencia.rs`, que sigue en pie para la
//! cultura general). Acá cada año hacen EXPERIMENTOS con la materia oculta del mundo
//! (`materia.rs`): eligen dos cosas que tienen a mano y un calor, el mundo responde según sus leyes
//! y ellos lo anotan en el **cuaderno** del pueblo. Lo que sale se aprende por inducción:
//!
//! * **Una receta es lo observado repetidamente**: `(a, b, calor) → material`. Cuenta como
//!   conocimiento cuando se la confirmó al menos dos veces y salió la mayoría de las veces (el fuego
//!   tiene ruido: cerca de un umbral a veces falla, así que hay que probar más calor).
//! * **La confianza es de cada pueblo**: lo que llega de oído (comercio) sólo cuenta cuando alguien
//!   del pueblo lo repite.
//! * **Los mitos nacen solos.** Cada experimento ocurre bajo una circunstancia irrelevante (luna,
//!   hora, un gesto). Un erudito poco agudo anota que «sólo funciona con luna llena» y, si es
//!   crédulo, sólo la prueba con luna llena: la receta queda atada a un ritual y no cuenta hasta que
//!   alguien la repite bajo otra circunstancia y sale igual. Un pueblo aislado de crédulos conserva su
//!   mito para siempre.
//! * **La técnica sale de la materia**: lo que un pueblo puede hacer (herramientas, armas,
//!   recipientes, construcción) es lo mejor que sabe producir de forma confiable, calculado de las
//!   propiedades REALES de los materiales; el calor al que llega depende de si sabe hacer cerámica
//!   (un horno) y carbón. Nada de esto está escrito como «saber»: aparece, o no, según la semilla.

use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

use super::azar::hash;
use super::cronica::Tipo;
use super::materia::*;
use super::mundo::{Mundo, JUGADOR_ID};
use super::persona::Oficio;
use super::saber::*;

/// Calor en pasos de 0.05 (0..=20).
pub const PASOS_CALOR: u8 = 20;
/// Tope de materiales a mano por pueblo y de recetas por cuaderno: el costo no crece sin freno.
const MAX_A_MANO: usize = 160;
const MAX_ENTRADAS: usize = 3000;
const SIN_B: u64 = u64::MAX;

/// Una receta observada.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Entrada {
    pub a: u64,
    pub b: u64,
    pub calor: u8,
    /// Lo que salió la última vez que salió algo (`None` = nada / arde).
    pub resultado: Option<u64>,
    /// Qué ley lo produjo (para contar la historia): «cuece», «alea», «reduce»…
    pub ley: String,
    pub n: u16,
    pub ok: u16,
    /// Bitmask de las circunstancias (0..8) bajo las que se probó y salió.
    pub ctx_vistos: u8,
    /// ¿Creen que sólo funciona bajo cierta circunstancia? (el mito).
    pub ritual: bool,
    pub ctx_creido: u8,
    /// Llegó por comercio y nadie del pueblo la confirmó todavía.
    pub de_oido: bool,
}

impl Entrada {
    /// ¿Es conocimiento firme? Dos confirmaciones, la mayoría de las veces, y sin mito.
    pub fn firme(&self) -> bool {
        self.resultado.is_some() && self.ok >= 2 && self.ok as u32 * 10 >= self.n as u32 * 6 && !self.ritual
    }
}

/// Lo que un pueblo (o una persona) lleva anotado.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Cuaderno {
    pub entradas: BTreeMap<u64, Entrada>,
    /// Materiales derivados que conocen (los crudos se conocen por el lugar).
    pub materiales: BTreeSet<u64>,
}

pub fn clave(a: u64, b: Option<u64>, calor: u8) -> u64 {
    let (x, y) = match b {
        None => (a, SIN_B),
        Some(b) if a <= b => (a, b),
        Some(b) => (b, a),
    };
    hash(hash(x ^ 0xC0DE) ^ y.rotate_left(21) ^ ((calor as u64) << 56))
}

impl Cuaderno {
    pub fn firmes(&self) -> impl Iterator<Item = &Entrada> {
        self.entradas.values().filter(|e| e.firme())
    }

    /// Materiales que el pueblo sabe producir de forma confiable.
    pub fn producibles(&self) -> BTreeSet<u64> {
        self.firmes().filter_map(|e| e.resultado).collect()
    }

    /// Anota un experimento. `ritual_creido`: el erudito, poco agudo, atribuyó el éxito a la
    /// circunstancia `ctx`. Devuelve `true` si la receta pasó a ser conocimiento firme AHORA.
    pub fn anotar(&mut self, a: u64, b: Option<u64>, calor: u8, res: Option<(u64, &str)>, ctx: u8, ritual_creido: bool) -> bool {
        let k = clave(a, b, calor);
        let e = self.entradas.entry(k).or_insert(Entrada {
            a,
            b: b.unwrap_or(SIN_B),
            calor,
            resultado: None,
            ley: String::new(),
            n: 0,
            ok: 0,
            ctx_vistos: 0,
            ritual: false,
            ctx_creido: 0,
            de_oido: false,
        });
        let antes = e.firme();
        e.n = e.n.saturating_add(1);
        e.de_oido = false; // alguien del pueblo la probó
        if let Some((id, ley)) = res {
            e.ok = e.ok.saturating_add(1);
            e.resultado = Some(id);
            e.ley = ley.to_string();
            e.ctx_vistos |= 1 << ctx;
            if e.ok == 1 && ritual_creido {
                e.ritual = true;
                e.ctx_creido = ctx;
            } else if e.ritual && e.ctx_vistos.count_ones() >= 2 {
                // Salió bajo otra circunstancia: el mito se cae.
                e.ritual = false;
            }
        }
        !antes && e.firme()
    }
}

/// `(nombre de la ley, frase)` para contar un descubrimiento.
fn frase(ley: &str) -> &'static str {
    match ley {
        "cuece" => "al ponerla al fuego se endurece",
        "carboniza" => "tapada con arcilla y a fuego lento no se quema: se vuelve carbón",
        "reduce" => "con carbón y mucho calor suelta un metal",
        "alea" => "fundidos juntos dan un metal nuevo",
        "calcina" => "calcinada se vuelve un polvo que liga",
        "vitrifica" => "con un fundente se vuelve vidrio",
        "mezcla" => "mezcladas en frío se vuelven otra cosa",
        _ => "da algo nuevo",
    }
}

impl Mundo {
    /// Los mundos viejos (guardados antes de la materia) no la traen: se crea al vuelo.
    pub(crate) fn asegurar_materia(&mut self) {
        if self.materia.materiales.is_empty() {
            self.materia = Materia::nueva(self.semilla);
        }
    }

    /// Calor al que llega un pueblo, según lo que sabe producir.
    pub fn calor_del_pueblo(&self, pueblo: u32) -> f32 {
        let Some(p) = self.pueblos.get(&pueblo) else { return 0.3 };
        let prod = p.cuaderno.producibles();
        let ceramica = prod.iter().any(|id| self.materia.get(*id).map(|m| m.clase == Clase::Ceramica).unwrap_or(false));
        let carbon = prod.iter().any(|id| self.materia.get(*id).map(|m| m.clase == Clase::Combustible).unwrap_or(false))
            || self.sustrato_accesible(pueblo) & S_CARBON != 0;
        calor_maximo(ceramica, carbon)
    }

    /// Todo lo que un pueblo tiene a mano para experimentar.
    fn a_mano(&self, pueblo: u32) -> Vec<u64> {
        let Some(p) = self.pueblos.get(&pueblo) else { return Vec::new() };
        let mut v: BTreeSet<u64> = Materia::crudos_de(self.sustrato_accesible(pueblo)).into_iter().collect();
        v.extend(p.cuaderno.materiales.iter().copied());
        v.into_iter().collect()
    }

    /// Un experimento de `persona` en su pueblo. `None` si no tiene con qué.
    fn experimentar(&mut self, persona: u32, mano: &mut Vec<u64>, tmax: &mut u8) -> Option<()> {
        let (pueblo, inteligencia, curiosidad, fe) = {
            let p = self.personas.get(&persona)?;
            (p.pueblo, p.inteligencia, p.curiosidad, p.fe)
        };
        if mano.len() < 2 || *tmax < 4 {
            return None;
        }
        let tm = *tmax;
        // Candidatos: se prefiere lo no probado y lo que combina derivados (la técnica compone).
        let mut mejor: Option<(f32, u64, Option<u64>, u8)> = None;
        // **Confirmar lo que salió**: un hallazgo de una sola vez no es conocimiento. Con buena
        // chance se repite una receta que dio algo y todavía no es firme.
        if self.rng.p(0.55 + 0.3 * curiosidad) {
            let sin_confirmar: Vec<(u64, Option<u64>, u8)> = self.pueblos[&pueblo]
                .cuaderno
                .entradas
                .values()
                .filter(|e| e.resultado.is_some() && !e.firme() && e.n < 6)
                .take(24)
                .map(|e| (e.a, if e.b == SIN_B { None } else { Some(e.b) }, e.calor))
                .collect();
            if !sin_confirmar.is_empty() {
                let (a, b, c) = sin_confirmar[self.rng.idx(sin_confirmar.len())];
                mejor = Some((9.0, a, b, c.min(tm)));
            }
        }
        for _ in 0..(4 + (curiosidad * 6.0) as usize) {
            let a = mano[self.rng.idx(mano.len())];
            let b = if self.rng.p(0.35) { None } else { Some(mano[self.rng.idx(mano.len())]) };
            if b == Some(a) {
                continue;
            }
            let calor = 4 + (self.rng.u64() % (tm as u64 - 3)) as u8;
            let k = clave(a, b, calor);
            let n = self.pueblos[&pueblo].cuaderno.entradas.get(&k).map(|e| e.n).unwrap_or(0);
            let derivado = |x: u64| x & (1 << 62) != 0;
            let novedad = if n == 0 { 2.0 } else { 1.0 / (1.0 + n as f32) };
            let compone = if derivado(a) || b.map(derivado).unwrap_or(false) { 0.5 } else { 0.0 };
            let score = novedad + compone + 0.4 * self.rng.f();
            if mejor.map(|m| score > m.0).unwrap_or(true) {
                mejor = Some((score, a, b, calor));
            }
        }
        let (_, a, b, calor) = mejor?;
        // Si ya creen que sólo funciona bajo cierta circunstancia, un crédulo la respeta.
        let k = clave(a, b, calor);
        let creido = self.pueblos[&pueblo].cuaderno.entradas.get(&k).filter(|e| e.ritual).map(|e| e.ctx_creido);
        let credulo = fe > 0.55 && inteligencia < 0.6;
        let ctx = match (creido, credulo) {
            (Some(c), true) => c,
            _ => (self.rng.u64() % 8) as u8,
        };
        let ruido = 0.92 + 0.16 * self.rng.f();
        let t = (calor as f32 / PASOS_CALOR as f32) * ruido;
        let res = self.materia.reaccionar(a, b, t, false);
        let (id, ley, nuevo_mundo) = match res {
            Resultado::Obtiene(m, ley) => {
                let nuevo = self.materia.registrar(&m);
                (Some(m.id), ley, nuevo)
            }
            Resultado::Quema => (None, "arde", false),
            Resultado::Nada => (None, "", false),
        };
        // ¿Atribuye el éxito a la circunstancia? Los poco agudos, a veces.
        let atribuye = id.is_some() && self.rng.p((1.0 - inteligencia) * 0.45);
        let firme_ahora = {
            let c = &mut self.pueblos.get_mut(&pueblo)?.cuaderno;
            if let Some(id) = id {
                if !c.materiales.contains(&id) && id & (1 << 62) != 0 {
                    c.materiales.insert(id);
                }
            }
            c.anotar(a, b, calor, id.map(|i| (i, ley)), ctx, atribuye)
        };
        if let Some(id) = id {
            if id & (1 << 62) != 0 && !mano.contains(&id) && mano.len() < MAX_A_MANO {
                mano.push(id);
            }
        }
        if firme_ahora {
            // Un horno o carbón nuevo cambia el calor al que llega el pueblo.
            *tmax = (self.calor_del_pueblo(pueblo) * PASOS_CALOR as f32).floor() as u8;
            self.contar_descubrimiento(persona, pueblo, id.unwrap_or(0), ley, nuevo_mundo);
        }
        Some(())
    }

    fn contar_descubrimiento(&mut self, persona: u32, pueblo: u32, id: u64, ley: &str, nuevo_mundo: bool) {
        let Some(m) = self.materia.get(id).cloned() else { return };
        let mejor_uso = USOS.iter().map(|u| (utilidad(&m, *u), *u)).fold((0.0f32, Uso::Herramienta), |a, b| if b.0 > a.0 { b } else { a });
        let record = self.materia.records.get(&mejor_uso.1).copied().unwrap_or(0.0);
        let es_record = mejor_uso.0 > record + 0.04;
        if es_record {
            self.materia.records.insert(mejor_uso.1, mejor_uso.0);
        }
        // La crónica no se llena de cada ladrillo: sólo lo que bate un récord del mundo, o lo que
        // un pueblo sabe hacer bien y no sabía.
        let (nombre, lugar) = (self.personas.get(&persona).map(|p| p.nombre.clone()).unwrap_or_default(), self.nombre_pueblo(pueblo));
        if es_record && self.cronica_materia_este_anio < 3 {
            self.cronica_materia_este_anio += 1;
            self.hecho(
                Tipo::Hito,
                pueblo,
                0,
                format!("{nombre}, de {lugar}, descubre que {} ({}): el mejor material del mundo para {}.", m.nombre, frase(ley), mejor_uso.1.nombre()),
            );
        } else if nuevo_mundo && mejor_uso.0 >= 0.5 && self.cronica_materia_este_anio < 3 {
            self.cronica_materia_este_anio += 1;
            self.hecho(Tipo::Descubrimiento, pueblo, 0, format!("{nombre}, de {lugar}, descubre {}: {}.", m.nombre, frase(ley)));
        }
    }

    /// **Una vez por año**: experimentan, se difunde por comercio y se recalcula lo que cada pueblo
    /// puede hacer con lo que sabe.
    pub(crate) fn descubrimiento_anual(&mut self) {
        self.asegurar_materia();
        self.cronica_materia_este_anio = 0;
        let ids: Vec<u32> = self.personas.keys().copied().collect();
        let mut cache: std::collections::HashMap<u32, (Vec<u64>, u8)> = std::collections::HashMap::new();
        for id in ids {
            if id == JUGADOR_ID {
                continue;
            }
            let (adulta, libre, oficio, cur, intel, pueblo) = {
                let p = &self.personas[&id];
                (p.adulta(self.dia), p.libre(self.dia), p.oficio, p.curiosidad, p.inteligencia, p.pueblo)
            };
            if !adulta || !libre {
                continue;
            }
            let Some(pu) = self.pueblos.get(&pueblo) else { continue };
            if !pu.vivo {
                continue;
            }
            let metodo = pu.saberes.tiene(METODO) as usize * 3;
            // Investigar no es diario: una persona se pone a experimentar de vez en cuando (una vez cada
            // varios años) y entonces hace una tanda. Eso da el ritmo de una HISTORIA (siglos), no de un
            // taller con todos probando todo a la vez desde el año uno.
            let ritmo = match oficio {
                Oficio::Erudito => 0.04,
                Oficio::Artesano => 0.012,
                _ if cur > 0.7 => 0.004,
                _ => 0.0,
            };
            if ritmo == 0.0 || !self.rng.p(ritmo) {
                continue;
            }
            let n = match oficio {
                Oficio::Erudito => 2 + (intel * 3.0) as usize + pu.biblioteca as usize + metodo,
                Oficio::Artesano => 1 + (intel * 2.0) as usize + metodo / 3,
                _ => 1,
            };
            if n == 0 {
                continue;
            }
            if !cache.contains_key(&pueblo) {
                let mut mano = self.a_mano(pueblo);
                if mano.len() > MAX_A_MANO {
                    mano.truncate(MAX_A_MANO);
                }
                let t = (self.calor_del_pueblo(pueblo) * PASOS_CALOR as f32).floor() as u8;
                cache.insert(pueblo, (mano, t));
            }
            let (mut mano, mut tmax) = cache.remove(&pueblo).expect("recién puesto");
            for _ in 0..n {
                if self.experimentar(id, &mut mano, &mut tmax).is_none() {
                    break;
                }
            }
            cache.insert(pueblo, (mano, tmax));
        }
        self.podar_cuadernos();
        self.difundir_cuadernos();
        let ids: Vec<u32> = self.pueblos.iter().filter(|(_, p)| p.vivo).map(|(i, _)| *i).collect();
        for id in ids {
            self.recalcular_bonus_materia(id);
        }
    }

    /// El cuaderno no crece sin freno: pasado el tope se olvidan las observaciones sueltas.
    fn podar_cuadernos(&mut self) {
        for p in self.pueblos.values_mut() {
            let c = &mut p.cuaderno;
            if c.entradas.len() > MAX_ENTRADAS {
                c.entradas.retain(|_, e| e.ok >= 1 || e.n >= 3);
            }
            if c.entradas.len() > MAX_ENTRADAS {
                // Último recurso: se quedan las firmes.
                c.entradas.retain(|_, e| e.firme());
            }
        }
    }

    /// Por comercio, un pueblo le cuenta al socio hasta dos recetas firmes (mitos incluidos).
    fn difundir_cuadernos(&mut self) {
        let rels: Vec<(u32, u32)> = self.relaciones.iter().filter(|r| r.comercio).map(|r| (r.a, r.b)).collect();
        for (a, b) in rels {
            for (de, para) in [(a, b), (b, a)] {
                let (Some(pd), Some(pp)) = (self.pueblos.get(&de), self.pueblos.get(&para)) else { continue };
                if !pd.vivo || !pp.vivo {
                    continue;
                }
                let com = pd.capacidad().com.min(2.0) + pp.capacidad().com.min(2.0);
                let candidatas: Vec<(u64, Entrada)> =
                    pd.cuaderno.entradas.iter().filter(|(k, e)| e.ok >= 2 && !pp.cuaderno.entradas.contains_key(*k)).map(|(k, e)| (*k, e.clone())).collect();
                if candidatas.is_empty() {
                    continue;
                }
                for _ in 0..2.min(candidatas.len()) {
                    if !self.rng.p(0.2 + 0.12 * com) {
                        continue;
                    }
                    let (k, mut e) = candidatas[self.rng.idx(candidatas.len())].clone();
                    e.n = 1;
                    e.ok = 1;
                    e.de_oido = true;
                    if let Some(res) = e.resultado {
                        if let Some(p) = self.pueblos.get_mut(&para) {
                            p.cuaderno.entradas.entry(k).or_insert(e);
                            if res & (1 << 62) != 0 {
                                p.cuaderno.materiales.insert(res);
                            }
                        }
                    }
                }
            }
        }
    }

    /// Lo que un pueblo puede hacer, calculado de las propiedades REALES de lo que sabe producir
    /// bien (más lo que tiene crudo a mano). Rendimiento decreciente: no desbalancea la sim.
    pub(crate) fn recalcular_bonus_materia(&mut self, pueblo: u32) {
        let (producibles, crudos) = {
            let Some(p) = self.pueblos.get(&pueblo) else { return };
            (p.cuaderno.producibles(), Materia::crudos_de(self.sustrato_accesible(pueblo)))
        };
        let mut mejor = [0.0f32; 5];
        for id in producibles.iter().chain(crudos.iter()) {
            let Some(m) = self.materia.get(*id) else { continue };
            for (i, u) in USOS.iter().enumerate() {
                mejor[i] = mejor[i].max(utilidad(m, *u));
            }
        }
        let firmes = self.pueblos[&pueblo].cuaderno.firmes().count() as f32;
        let derivados = producibles.len() as f32;
        let sobre = |x: f32, base: f32| (x - base).max(0.0);
        let b = [
            0.5 * mejor[2] + 0.3 * sobre(mejor[0], 0.45),                      // comida: recipientes y arado
            0.6 * sobre(mejor[0], 0.45),                                        // mat: herramientas
            0.9 * sobre(mejor[1], 0.50),                                        // mil: armas
            0.04 * (1.0 + firmes).ln(),                                         // inv: saber experimentar
            0.25 * sobre(mejor[2], 0.30),                                       // salud: recipientes limpios
            0.0,                                                                // orden
            (0.03 * derivados).min(0.6),                                        // com: cosas que vender
            0.7 * sobre(mejor[3], 0.45),                                        // defensa: construcción
        ];
        if let Some(p) = self.pueblos.get_mut(&pueblo) {
            p.bonus_materia = b;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn una_receta_es_firme_con_dos_confirmaciones_y_sin_mito() {
        let mut c = Cuaderno::default();
        assert!(!c.anotar(MADERA, Some(ARCILLA), 8, Some((1 << 62 | 5, "carboniza")), 3, false), "una sola vez no alcanza");
        assert!(c.anotar(MADERA, Some(ARCILLA), 8, Some((1 << 62 | 5, "carboniza")), 5, false), "la segunda la confirma");
        assert_eq!(c.firmes().count(), 1);
        // Un fracaso en el medio no la rompe si la mayoría salió.
        c.anotar(MADERA, Some(ARCILLA), 8, None, 1, false);
        assert_eq!(c.firmes().count(), 1);
    }

    #[test]
    fn un_mito_no_cuenta_hasta_que_se_prueba_bajo_otra_circunstancia() {
        let mut c = Cuaderno::default();
        c.anotar(ARCILLA, None, 10, Some((1 << 62 | 7, "cuece")), 2, true); // «sólo con luna llena»
        c.anotar(ARCILLA, None, 10, Some((1 << 62 | 7, "cuece")), 2, false);
        assert_eq!(c.firmes().count(), 0, "atada al ritual: no cuenta");
        c.anotar(ARCILLA, None, 10, Some((1 << 62 | 7, "cuece")), 6, false);
        assert_eq!(c.firmes().count(), 1, "salió bajo otra circunstancia: el mito cae");
    }

    #[test]
    fn la_clave_no_depende_del_orden_de_los_materiales() {
        assert_eq!(clave(3, Some(9), 7), clave(9, Some(3), 7));
        assert_ne!(clave(3, Some(9), 7), clave(3, Some(9), 8));
        assert_ne!(clave(3, None, 7), clave(3, Some(9), 7));
    }
}
