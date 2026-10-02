//! LA FÍSICA DEL MUNDO — lo que el universo del juego ES, y lo que su gente CREE que es.
//!
//! Hay una verdad oculta: unas cuantas constantes que rigen esta simulación (la
//! gravedad de la Tierra es la del juego; el resto sale de la semilla). Nadie las
//! conoce al empezar. Cada pueblo lleva sus **creencias**: para cada ley, un error
//! relativo con signo (creído = verdad · (1 + error)). Los eruditos con la teoría y
//! el cielo despejado observan y miden, y el error baja hasta el **piso** que
//! permiten sus instrumentos (ojo desnudo → astrolabio → telescopio → método
//! científico → computadora → computadora cuántica). Un experimento que sale mal
//! también enseña.
//!
//! Lo que esto NO es: ciencia real. Es un modelo honesto de "medir una constante
//! oculta con error decreciente", y lo que importa es que las consecuencias
//! dependen de la VERDAD y no de lo que se cree: un cohete con la gravedad mal
//! medida falla, una bomba con la masa crítica mal calculada no explota entera, un
//! reactor mal entendido se derrite. No hay "desbloqueo por tiempo mirando el cielo".

use serde::{Deserialize, Serialize};

use super::azar::unidad;
use super::cronica::Tipo;
use super::mundo::Mundo;
use super::persona::Oficio;
use super::saber::*;

pub const N_LEYES: usize = 7;
pub const GRAVEDAD: usize = 0;
pub const CALENDARIO: usize = 1;
pub const ORBITAS: usize = 2;
pub const TERMICA: usize = 3;
pub const ELECTRICA: usize = 4;
pub const FISION: usize = 5;
pub const CUANTO: usize = 6;

pub struct Ley {
    pub nombre: &'static str,
    pub unidad: &'static str,
    /// Saber cuya teoría hace falta para siquiera plantearse la medición.
    pub teoria: u8,
    /// ¿Se mide mirando el cielo (necesita cielo despejado)?
    pub cielo: bool,
    /// Error con sólo la teoría básica y a simple vista.
    pub base: f32,
    /// (saber, piso de error que permite): el mejor de los que se tengan.
    pub pisos: &'static [(u8, f32)],
}

pub static LEYES: [Ley; N_LEYES] = [
    Ley {
        nombre: "la caída de los cuerpos",
        unidad: "m/s²",
        teoria: MECANICA,
        cielo: false,
        base: 0.45,
        pisos: &[(MECANICA, 0.12), (METODO, 0.04), (COMPUTACION, 0.006), (MATERIALES, 0.003)],
    },
    Ley {
        nombre: "el año solar",
        unidad: "días",
        teoria: MATEMATICA,
        cielo: true,
        base: 0.40,
        pisos: &[(ASTRONOMIA, 0.12), (OPTICA, 0.05), (METODO, 0.03), (COMPUTACION, 0.006), (MECANICA_ORBITAL, 0.002)],
    },
    Ley {
        nombre: "el movimiento de la Luna y los planetas",
        unidad: "días",
        teoria: ASTRONOMIA,
        cielo: true,
        base: 0.40,
        pisos: &[(ASTRONOMIA, 0.15), (OPTICA, 0.05), (METODO, 0.03), (COMPUTACION, 0.006), (MECANICA_ORBITAL, 0.002)],
    },
    Ley {
        nombre: "el calor y el trabajo",
        unidad: "u.",
        teoria: VAPOR,
        cielo: false,
        base: 0.50,
        pisos: &[(VAPOR, 0.15), (QUIMICA, 0.10), (METODO, 0.04), (ELECTRICIDAD, 0.03), (MATERIALES, 0.006)],
    },
    Ley {
        nombre: "la carga eléctrica",
        unidad: "u.",
        teoria: ELECTRICIDAD,
        cielo: false,
        base: 0.45,
        pisos: &[(ELECTRICIDAD, 0.12), (METODO, 0.04), (TELECOM, 0.02), (COMPUTACION, 0.006)],
    },
    Ley {
        nombre: "la masa crítica",
        unidad: "kg",
        teoria: NUCLEAR,
        cielo: false,
        base: 0.50,
        pisos: &[(NUCLEAR, 0.30), (METODO, 0.05), (COMPUTACION, 0.01), (CUANTICA, 0.004)],
    },
    Ley {
        nombre: "el cuanto de acción",
        unidad: "u.",
        teoria: CUANTICA,
        cielo: false,
        base: 0.50,
        pisos: &[(CUANTICA, 0.30), (COMPUTACION, 0.03), (COMPUTACION_CUANTICA, 0.003)],
    },
];

/// Los umbrales de precisión que cuentan como hito mundial.
pub const NIVELES: [f32; 6] = [0.25, 0.10, 0.03, 0.01, 0.003, 0.001];

/// Las constantes REALES de este universo, derivadas de la semilla. Sólo la
/// gravedad viene del juego; es lo que sentirá el jugador al caer.
pub fn verdad(semilla: u64) -> [f32; N_LEYES] {
    let u = |k: u64| 0.85 + 0.30 * unidad(semilla ^ (k + 1).wrapping_mul(0x00C0_FFEE));
    [
        crate::constants::GRAVITY,
        365.25 * u(1),
        27.32 * u(2),
        1.0 * u(3),
        1.0 * u(4),
        52.0 * u(5),
        1.0 * u(6),
    ]
}

pub fn nivel_de(mag: f32) -> u8 {
    NIVELES.iter().take_while(|n| mag <= **n).count() as u8
}

impl Ley {
    /// El mejor error posible con estos saberes.
    pub fn piso(&self, s: Saberes) -> f32 {
        self.pisos.iter().filter(|(sab, _)| s.tiene(*sab)).map(|(_, p)| *p).fold(self.base, f32::min)
    }
}

/// Lo que un pueblo cree de cada ley: error relativo con signo.
#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub struct Creencias {
    pub err: [f32; N_LEYES],
    /// Cuántas observaciones-persona acumuló (para mostrar, no para decidir).
    pub obs: [u32; N_LEYES],
}

impl Default for Creencias {
    fn default() -> Self {
        Self { err: [0.6; N_LEYES], obs: [0; N_LEYES] }
    }
}

impl Creencias {
    /// Un pueblo que ya sabe algo arranca con la precisión de lo que sabe.
    pub fn inicial(saberes: Saberes, h: u64) -> Self {
        let mut c = Self::default();
        for (k, ley) in LEYES.iter().enumerate() {
            let signo = if unidad(h ^ (k as u64 + 1) * 0x9E37) < 0.5 { -1.0 } else { 1.0 };
            let mag = if saberes.tiene(ley.teoria) {
                let piso = ley.piso(saberes);
                (piso * (1.4 + 1.2 * unidad(h ^ (k as u64 + 40) * 0x51ED))).min(ley.base)
            } else {
                0.6
            };
            c.err[k] = signo * mag;
        }
        c
    }

    pub fn mag(&self, k: usize) -> f32 {
        self.err[k].abs()
    }

    /// Lo que se cree que vale la ley, dada la verdad.
    pub fn creido(&self, k: usize, verdad: &[f32; N_LEYES]) -> f32 {
        verdad[k] * (1.0 + self.err[k])
    }
}

/// Una fila para mostrar (consola de depuración o registro del mundo).
#[derive(Clone, Debug)]
pub struct LeyVista {
    pub nombre: &'static str,
    pub unidad: &'static str,
    pub conoce: bool,
    pub creido: f32,
    pub verdad: f32,
    pub error: f32,
    pub nivel: u8,
    pub obs: u32,
}

impl Mundo {
    pub fn verdad_fisica(&self) -> [f32; N_LEYES] {
        verdad(self.semilla)
    }

    /// La física tal como la ve un pueblo (y como es de verdad, para quien depura).
    pub fn ver_fisica(&self, pueblo: u32) -> Vec<LeyVista> {
        let v = self.verdad_fisica();
        let Some(p) = self.pueblos.get(&pueblo) else { return Vec::new() };
        (0..N_LEYES)
            .map(|k| LeyVista {
                nombre: LEYES[k].nombre,
                unidad: LEYES[k].unidad,
                conoce: p.saberes.tiene(LEYES[k].teoria),
                creido: p.creencias.creido(k, &v),
                verdad: v[k],
                error: p.creencias.mag(k),
                nivel: nivel_de(p.creencias.mag(k)),
                obs: p.creencias.obs[k],
            })
            .collect()
    }

    /// 1 = conocimiento perfecto de esas leyes, →0.35 = a ciegas. Es lo que separa a
    /// quien construye con la verdad de quien construye con lo que cree.
    pub fn factor_ingenieria(&self, pueblo: u32, leyes: &[usize]) -> f32 {
        let Some(p) = self.pueblos.get(&pueblo) else { return 0.35 };
        if leyes.is_empty() {
            return 1.0;
        }
        let m: f32 = leyes.iter().map(|k| p.creencias.mag(*k)).sum::<f32>() / leyes.len() as f32;
        (1.0 - 0.55 * m.min(1.0)).clamp(0.35, 1.0)
    }

    /// Un fracaso enseña: el error de esas leyes cae a la mitad para quien lo vivió.
    pub fn aprender_de_falla(&mut self, pueblo: u32, leyes: &[usize]) {
        let piso = |s: Saberes, k: usize| LEYES[k].piso(s);
        if let Some(p) = self.pueblos.get_mut(&pueblo) {
            let s = p.saberes;
            for k in leyes {
                let m = (p.creencias.mag(*k) * 0.55).max(piso(s, *k));
                p.creencias.err[*k] = p.creencias.err[*k].signum() * m;
                p.creencias.obs[*k] += 10;
            }
        }
    }

    /// Aprender un saber trae su teoría: de a poco deja de ser total ignorancia.
    pub(crate) fn aprender_teoria(&mut self, pueblo: u32, saber: u8) {
        let Some(p) = self.pueblos.get_mut(&pueblo) else { return };
        for (k, ley) in LEYES.iter().enumerate() {
            if ley.teoria == saber && p.creencias.mag(k) > ley.base {
                p.creencias.err[k] = p.creencias.err[k].signum() * ley.base;
            }
        }
    }

    pub(crate) fn fisica_dia(&mut self) {
        let dia = self.dia;
        let ids: Vec<u32> = self.pueblos.keys().copied().collect();
        let mut cambios: Vec<(u32, usize, f32, u32)> = Vec::new();
        for id in ids {
            let (saberes, gente, hambre) = {
                let p = &self.pueblos[&id];
                if !p.vivo {
                    continue;
                }
                (p.saberes, p.gente.clone(), p.hambre)
            };
            let guerra = self.guerra_de(id).is_some();
            let cielo = self.sustrato_accesible(id) & S_CIELO != 0;
            let metodo = saberes.tiene(METODO);
            for (k, ley) in LEYES.iter().enumerate() {
                let mag = self.pueblos[&id].creencias.mag(k);
                if !saberes.tiene(ley.teoria) {
                    // sin teoría viva, la medición se pierde despacio
                    if mag < 0.6 {
                        cambios.push((id, k, (mag + (0.6 - mag) * 0.0005).min(0.6), 0));
                    }
                    continue;
                }
                if ley.cielo && !cielo {
                    continue;
                }
                let n = gente
                    .iter()
                    .filter(|g| {
                        self.personas
                            .get(g)
                            .map(|x| {
                                x.saberes.tiene(ley.teoria)
                                    && x.adulta(dia)
                                    && x.libre(dia)
                                    && (x.oficio == Oficio::Erudito || x.curiosidad > 0.65)
                            })
                            .unwrap_or(false)
                    })
                    .count()
                    .min(8);
                if n == 0 {
                    continue;
                }
                let rate = 0.004
                    * if metodo { 3.0 } else { 1.0 }
                    * (n as f32).sqrt()
                    / 2.5
                    * (1.0 - 0.5 * hambre)
                    * if guerra { 0.6 } else { 1.0 };
                let piso = ley.piso(saberes);
                let nuevo = if mag > piso { piso + (mag - piso) * (1.0 - rate) } else { mag };
                cambios.push((id, k, nuevo, n as u32));
            }
        }
        for (id, k, mag, n) in cambios {
            let Some(p) = self.pueblos.get_mut(&id) else { continue };
            p.creencias.err[k] = p.creencias.err[k].signum() * mag;
            p.creencias.obs[k] += n;
            let nivel = nivel_de(mag);
            if nivel > self.hitos_fisica[k] {
                self.hitos_fisica[k] = nivel;
                let lugar = p.nombre.clone();
                if nivel >= 2 {
                    self.hecho(
                        Tipo::Observacion,
                        id,
                        0,
                        format!(
                            "Los eruditos de {lugar} miden {} con un error de {:.1} %: nadie en el mundo lo había logrado.",
                            LEYES[k].nombre,
                            mag * 100.0
                        ),
                    );
                }
            }
        }
    }

    pub(crate) fn fisica_anual(&mut self) {
        let ids: Vec<u32> = self.pueblos.keys().copied().collect();
        for id in ids {
            let (vivo, cuerpo, saberes, nombre) = {
                let p = &self.pueblos[&id];
                (p.vivo, p.cuerpo, p.saberes, p.nombre.clone())
            };
            if !vivo {
                continue;
            }
            // El cielo da espectáculos: quien lo mira con método aprende de ellos.
            if saberes.tiene(ASTRONOMIA) && self.sustrato_accesible(id) & S_CIELO != 0 && self.rng.p(0.30) {
                let tipo = self.rng.idx(3);
                let (mut pred, mag_orb) = (false, self.pueblos[&id].creencias.mag(ORBITAS));
                if mag_orb < 0.10 && tipo == 0 {
                    pred = true;
                }
                let f = if saberes.tiene(OPTICA) { 0.85 } else { 0.92 };
                let p = self.pueblos.get_mut(&id).unwrap();
                for k in [CALENDARIO, ORBITAS] {
                    let piso = LEYES[k].piso(saberes);
                    let m = (p.creencias.mag(k) * f).max(piso);
                    p.creencias.err[k] = p.creencias.err[k].signum() * m;
                    p.creencias.obs[k] += 3;
                }
                if pred {
                    p.legit = (p.legit + 0.03).min(1.0);
                    self.hecho(Tipo::Observacion, id, 0, format!("Los astrónomos de {nombre} predicen un eclipse y aciertan: el pueblo los mira distinto."));
                } else if self.rng.p(0.2) {
                    let que = ["un eclipse", "un cometa", "una conjunción de planetas"][tipo];
                    self.hecho(Tipo::Observacion, id, 0, format!("Los astrónomos de {nombre} registran {que}."));
                }
            }
            // Un reactor que no se entiende bien se derrite.
            if saberes.tiene(NUCLEAR) && cuerpo == 0 {
                let m = self.pueblos[&id].creencias.mag(FISION);
                let p_acc = 0.006 * (m / 0.3).powi(2);
                if self.rng.p(p_acc) {
                    self.accidente_nuclear(id);
                }
            }
        }
        self.difundir_creencias();
    }

    /// Lo que se sabe viaja: quien comercia con un vecino que mide mejor le copia
    /// un pedazo de su precisión (sólo si entiende la teoría de esa ley).
    fn difundir_creencias(&mut self) {
        let ids: Vec<u32> = self.pueblos.keys().copied().collect();
        for id in ids {
            let (vivo, vecinos, saberes) = {
                let p = &self.pueblos[&id];
                (p.vivo, p.vecinos.clone(), p.saberes)
            };
            if !vivo {
                continue;
            }
            for k in 0..N_LEYES {
                if !saberes.tiene(LEYES[k].teoria) {
                    continue;
                }
                let mi = self.pueblos[&id].creencias.mag(k);
                let mejor = vecinos
                    .iter()
                    .filter_map(|v| self.pueblos.get(v))
                    .filter(|q| q.vivo)
                    .map(|q| q.creencias.mag(k))
                    .fold(f32::MAX, f32::min);
                if mejor < mi * 0.8 {
                    let nuevo = (mi + 0.25 * (mejor - mi)).max(LEYES[k].piso(saberes));
                    let p = self.pueblos.get_mut(&id).unwrap();
                    p.creencias.err[k] = p.creencias.err[k].signum() * nuevo;
                }
            }
        }
    }
}
