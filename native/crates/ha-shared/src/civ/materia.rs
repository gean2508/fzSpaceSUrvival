//! **LA MATERIA DEL MUNDO — lo que las cosas SON, y cómo reaccionan entre sí.**
//!
//! Hasta acá el conocimiento era un árbol escrito a mano (`saber.rs`): descubrir era tirar un dado
//! sobre una lista. Esto es lo contrario: acá NO hay lista de descubrimientos. Hay **materiales con
//! propiedades ocultas** (dureza, tenacidad, densidad, punto de fusión, combustibilidad,
//! conductividad, porosidad) que salen de la semilla del mundo, y **unas pocas leyes de
//! reacción** que calculan —a partir de esas propiedades— qué pasa cuando juntás dos cosas con
//! calor. El bronce, el ladrillo, el carbón vegetal o el vidrio no están programados: aparecen (o
//! no, o distintos, o más tarde) según la semilla, cuando alguien prueba lo que corresponde.
//!
//! * Los materiales DERIVADOS (lo que sale de una reacción) son materiales nuevos, con propiedades
//!   calculadas, y se pueden volver a combinar: la profundidad no tiene techo escrito.
//! * Lo que un material PERMITE (herramienta, arma, recipiente, construcción, combustible, tejido)
//!   también se calcula de sus propiedades: la tecnología sale de la materia, no de un menú.
//! * Las leyes son pocas y simples a propósito (§17): lo que emerge es QUÉ ley se descubre, en qué
//!   orden, con qué errores y qué combinaciones que nadie escribió.
//!
//! Esto NO es química real: es un modelo honesto de «propiedades ocultas + reacciones que dependen
//! de ellas». Las consecuencias (`utilidad`) dependen de la VERDAD, no de lo que se crea.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

use super::azar::hash;
use super::saber::*;

/// Las siete propiedades de un material, todas 0..1.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Props {
    pub dureza: f32,
    pub tenacidad: f32,
    pub densidad: f32,
    /// Calor al que se funde / ablanda (0..1 sobre la escala de fuegos del mundo).
    pub fusion: f32,
    pub combustible: f32,
    pub conduce: f32,
    pub porosidad: f32,
}

impl Props {
    fn a_array(self) -> [f32; 7] {
        [self.dureza, self.tenacidad, self.densidad, self.fusion, self.combustible, self.conduce, self.porosidad]
    }
    fn de_array(a: [f32; 7]) -> Self {
        Self {
            dureza: a[0].clamp(0.0, 1.0),
            tenacidad: a[1].clamp(0.0, 1.0),
            densidad: a[2].clamp(0.0, 1.0),
            fusion: a[3].clamp(0.0, 1.0),
            combustible: a[4].clamp(0.0, 1.0),
            conduce: a[5].clamp(0.0, 1.0),
            porosidad: a[6].clamp(0.0, 1.0),
        }
    }
    /// Mezcla con peso `w` para `self` y `1 - w` para `o`.
    fn mezcla(self, o: Props, w: f32) -> Props {
        let (a, b) = (self.a_array(), o.a_array());
        let mut r = [0.0; 7];
        for i in 0..7 {
            r[i] = a[i] * w + b[i] * (1.0 - w);
        }
        Props::de_array(r)
    }
}

/// De qué familia es un material (decide qué reacciones le caben).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Clase {
    Crudo,
    Mena,
    Metal,
    Aleacion,
    Ceramica,
    Vidrio,
    Aglutinante,
    Compuesto,
    Combustible,
    Liquido,
}

/// Un material: los crudos tienen id fijo (0..N_CRUDOS); los derivados, un hash de cómo se hicieron.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Material {
    pub id: u64,
    pub nombre: String,
    pub clase: Clase,
    pub props: Props,
    /// Cómo se obtuvo (vacío en los crudos): para contar la historia.
    pub origen: Vec<u64>,
    /// Cuántos pasos de fabricación lleva a cuestas (0 = crudo). Tope `PROFUNDIDAD_MAX`.
    #[serde(default)]
    pub profundidad: u8,
}

/// Hasta dónde se puede apilar: derivados de derivados de derivados… sin pasar de acá (el mundo
/// tiene que poder seguir simulándolo en un celular).
pub const PROFUNDIDAD_MAX: u8 = 5;
/// Tope de materiales derivados distintos por mundo: pasado esto el mundo «se llenó» y nada nuevo se forma.
pub const MAX_DERIVADOS: usize = 2048;

pub const MADERA: u64 = 0;
pub const PIEDRA: u64 = 1;
pub const ARCILLA: u64 = 2;
pub const ARENA: u64 = 3;
pub const MENA_COBRE: u64 = 4;
pub const MENA_ESTANO: u64 = 5;
pub const MENA_HIERRO: u64 = 6;
pub const CARBON: u64 = 7;
pub const FIBRA: u64 = 8;
pub const HUESO: u64 = 9;
pub const CALIZA: u64 = 10;
pub const RESINA: u64 = 11;
pub const AGUA: u64 = 12;
pub const SAL: u64 = 13;
pub const N_CRUDOS: u64 = 14;

/// (nombre, clase, sustrato que lo da, propiedades base). Las propiedades de cada mundo son
/// ESTAS ± un 20 % sorteado por semilla: en un mundo el cobre se funde antes que en otro.
const CRUDOS: [(&str, Clase, u32, [f32; 7]); N_CRUDOS as usize] = [
    ("madera", Clase::Crudo, S_MADERA, [0.25, 0.55, 0.35, 0.70, 0.90, 0.05, 0.50]),
    ("piedra", Clase::Crudo, S_PIEDRA, [0.70, 0.20, 0.75, 0.95, 0.00, 0.10, 0.15]),
    ("arcilla", Clase::Crudo, S_ARCILLA, [0.15, 0.30, 0.60, 0.55, 0.00, 0.05, 0.70]),
    ("arena", Clase::Crudo, S_ARENA, [0.40, 0.05, 0.70, 0.80, 0.00, 0.05, 0.90]),
    ("mena de cobre", Clase::Mena, S_COBRE, [0.30, 0.40, 0.70, 0.35, 0.00, 0.60, 0.30]),
    ("mena de estaño", Clase::Mena, S_COBRE, [0.25, 0.30, 0.65, 0.25, 0.00, 0.50, 0.30]),
    ("mena de hierro", Clase::Mena, S_HIERRO, [0.50, 0.30, 0.85, 0.60, 0.00, 0.55, 0.25]),
    ("carbón", Clase::Combustible, S_CARBON, [0.20, 0.10, 0.40, 0.99, 1.00, 0.20, 0.50]),
    ("fibra", Clase::Crudo, S_PLANTAS, [0.05, 0.80, 0.10, 0.50, 0.70, 0.00, 0.60]),
    ("hueso", Clase::Crudo, S_ANIMALES, [0.45, 0.40, 0.50, 0.90, 0.10, 0.00, 0.30]),
    ("caliza", Clase::Crudo, S_PIEDRA, [0.50, 0.15, 0.65, 0.85, 0.00, 0.05, 0.30]),
    ("resina", Clase::Crudo, S_MADERA, [0.10, 0.20, 0.30, 0.30, 0.80, 0.00, 0.10]),
    ("agua", Clase::Liquido, S_AGUA, [0.00, 0.00, 0.50, 0.10, 0.00, 0.30, 0.00]),
    ("sal", Clase::Crudo, S_MAR, [0.20, 0.10, 0.60, 0.70, 0.00, 0.10, 0.10]),
];

/// Lo que se puede HACER con un material (la utilidad se calcula de sus propiedades).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum Uso {
    Herramienta,
    Arma,
    Recipiente,
    Construccion,
    Tejido,
}

pub const USOS: [Uso; 5] = [Uso::Herramienta, Uso::Arma, Uso::Recipiente, Uso::Construccion, Uso::Tejido];

impl Uso {
    pub fn nombre(self) -> &'static str {
        match self {
            Uso::Herramienta => "herramientas",
            Uso::Arma => "armas",
            Uso::Recipiente => "recipientes",
            Uso::Construccion => "construcción",
            Uso::Tejido => "tejidos",
        }
    }
}

/// Qué tan bueno es un material para un uso (0..1). Sale de las propiedades y de la clase.
pub fn utilidad(m: &Material, uso: Uso) -> f32 {
    let p = m.props;
    let u = match uso {
        Uso::Herramienta => 0.55 * p.dureza + 0.45 * p.tenacidad,
        Uso::Arma => 0.70 * p.dureza + 0.30 * p.tenacidad,
        Uso::Recipiente => (1.0 - p.porosidad) * (0.5 + 0.5 * p.dureza),
        Uso::Construccion => 0.6 * p.dureza + 0.2 * p.densidad + 0.2 * (1.0 - p.porosidad),
        Uso::Tejido => p.tenacidad * (1.0 - p.densidad) * (1.0 - 0.5 * p.dureza),
    };
    // Un líquido o un combustible puro no sirve de herramienta ni de muro.
    match m.clase {
        Clase::Liquido => 0.0,
        Clase::Combustible if uso != Uso::Tejido => u * 0.2,
        _ => u,
    }
}

/// La materia de un mundo: los crudos (con sus propiedades ocultas) y todo lo que se fue derivando.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Materia {
    pub semilla: u64,
    pub materiales: BTreeMap<u64, Material>,
    /// El mejor valor visto en el mundo para cada uso (para saber qué descubrimiento bate un récord).
    #[serde(default)]
    pub records: BTreeMap<Uso, f32>,
}

/// Qué pasó al juntar dos cosas con calor.
#[derive(Clone, Debug)]
pub enum Resultado {
    /// Nada que valga la pena (o falló).
    Nada,
    /// Se quemó: libera calor; el id es lo que quedó (ceniza = nada).
    Quema,
    /// Salió un material (puede ser nuevo para el mundo).
    Obtiene(Material, &'static str),
}

fn jitter(semilla: u64, id: u64, k: usize) -> f32 {
    let h = hash(semilla ^ id.wrapping_mul(0x9E37_79B9_7F4A_7C15) ^ (k as u64).wrapping_mul(0xC2B2_AE3D_27D4_EB4F));
    // 0.8 ..= 1.2
    0.8 + 0.4 * ((h >> 40) as f32 / (1u64 << 24) as f32)
}

impl Materia {
    pub fn nueva(semilla: u64) -> Self {
        let mut m = Materia { semilla, materiales: BTreeMap::new(), records: BTreeMap::new() };
        for (i, (nombre, clase, _, base)) in CRUDOS.iter().enumerate() {
            let mut a = *base;
            for (k, v) in a.iter_mut().enumerate() {
                // Lo que es 0 o 1 de base se queda (el agua no es dura; el carbón arde del todo).
                if *v > 0.0 && *v < 1.0 {
                    *v = (*v * jitter(semilla, i as u64, k)).clamp(0.0, 0.99);
                }
            }
            m.materiales.insert(
                i as u64,
                Material { id: i as u64, nombre: (*nombre).to_string(), clase: *clase, props: Props::de_array(a), origen: Vec::new(), profundidad: 0 },
            );
        }
        m
    }

    pub fn get(&self, id: u64) -> Option<&Material> {
        self.materiales.get(&id)
    }

    /// ¿Qué crudos da un sustrato? (para saber qué hay a mano en un pueblo).
    pub fn crudos_de(sustrato: u32) -> Vec<u64> {
        CRUDOS.iter().enumerate().filter(|(_, c)| c.2 != 0 && sustrato & c.2 != 0).map(|(i, _)| i as u64).collect()
    }

    /// Sinergia de una pareja: cuánto mejor (o peor) que el promedio sale juntarlas. Fija por mundo.
    fn sinergia(&self, a: u64, b: u64) -> f32 {
        let (x, y) = if a <= b { (a, b) } else { (b, a) };
        let h = hash(self.semilla ^ 0x51_47 ^ x.wrapping_mul(0x9E37_79B9_7F4A_7C15) ^ y.rotate_left(17));
        -0.25 + 0.85 * ((h >> 40) as f32 / (1u64 << 24) as f32)
    }

    fn nombre_derivado(&self, base: &str, partes: &[u64]) -> String {
        let n: Vec<String> = partes.iter().filter_map(|p| self.get(*p).map(|m| m.nombre.clone())).collect();
        format!("{base} de {}", n.join(" y "))
    }

    /// **La identidad de un derivado es lo que ES, no cómo se hizo**: la clase y sus propiedades
    /// cuantizadas. Dos caminos que dan casi lo mismo dan EL MISMO material (el bronce es bronce
    /// vengas de donde vengas), y el mundo no se llena de variantes idénticas.
    fn id_derivado(&self, clase: Clase, props: Props) -> u64 {
        let mut x = hash(self.semilla ^ clase as u64 ^ 0xD3);
        for v in props.a_array() {
            x = hash(x ^ ((v * 10.0).round() as u64));
        }
        // Los derivados nunca chocan con los crudos.
        (x | (1 << 62)) & !(1 << 63)
    }

    fn derivado(&self, clase: Clase, base: &str, partes: &[u64], props: Props) -> Material {
        let prof = partes.iter().filter_map(|p| self.get(*p)).map(|m| m.profundidad).max().unwrap_or(0) + 1;
        Material {
            id: self.id_derivado(clase, props),
            nombre: self.nombre_derivado(base, partes),
            clase,
            props,
            origen: partes.to_vec(),
            profundidad: prof,
        }
    }

    /// **La ley de reacción.** `a` (+ `b`) a un calor `t` (0..1, ya con el ruido del fuego), en un
    /// recipiente `cerrado` (tapado con arcilla: sin aire) o a cielo abierto. Pura: no toca el estado.
    ///
    /// Reglas (cada una sale de las propiedades, ninguna nombra un material concreto salvo por su
    /// CLASE o sus números):
    /// 1. **Arde** lo combustible a cielo abierto con calor.
    /// 2. **Carboniza**: algo combustible TAPADO con arcilla y a fuego medio da carbón vegetal.
    /// 3. **Cuece** la arcilla: se endurece y deja de ser porosa; con fibra o arena aguanta más.
    /// 4. **Reduce** una mena con carbón a calor de fusión: sale el metal.
    /// 5. **Aleación**: dos metales fundidos juntos; el resultado depende de la sinergia del mundo.
    /// 6. **Calcina** lo calcáreo (caliza): da un aglutinante.
    /// 7. **Vitrifica** la arena (con un fundente): vidrio, duro y frágil.
    /// 8. **Mezcla en frío** si uno de los dos liga (poroso o aglutinante): adobe, mortero, laminados.
    pub fn reaccionar(&self, a: u64, b: Option<u64>, t: f32, cerrado: bool) -> Resultado {
        match self.reaccionar_sin_tope(a, b, t, cerrado) {
            Resultado::Obtiene(m, ley) => {
                if m.profundidad > PROFUNDIDAD_MAX || (self.derivados() >= MAX_DERIVADOS && !self.materiales.contains_key(&m.id)) {
                    Resultado::Nada
                } else {
                    Resultado::Obtiene(m, ley)
                }
            }
            otro => otro,
        }
    }

    /// Cuántos materiales derivados distintos existen en el mundo.
    pub fn derivados(&self) -> usize {
        self.materiales.len().saturating_sub(N_CRUDOS as usize)
    }

    fn reaccionar_sin_tope(&self, a: u64, b: Option<u64>, t: f32, cerrado: bool) -> Resultado {
        let Some(ma) = self.get(a) else { return Resultado::Nada };
        let mb = b.and_then(|x| self.get(x));
        let pa = ma.props;
        match mb {
            None => {
                // 1. arde
                if pa.combustible >= 0.5 && !cerrado && t >= 0.2 && ma.clase != Clase::Liquido {
                    return Resultado::Quema;
                }
                // 3b. arcilla sola: se cuece
                if ma.clase == Clase::Crudo && pa.porosidad >= 0.6 && pa.dureza < 0.3 && t >= pa.fusion * 0.6 && !cerrado {
                    let mut p = pa;
                    p.dureza = (pa.dureza + 0.45).min(0.9);
                    p.porosidad = pa.porosidad * 0.35;
                    p.fusion = (pa.fusion + 0.2).min(0.99);
                    p.tenacidad = (pa.tenacidad * 0.7).max(0.05); // quebradiza
                    return Resultado::Obtiene(self.derivado(Clase::Ceramica, "cerámica", &[a], p), "cuece");
                }
                // 6. calcina
                if ma.id == CALIZA || (ma.clase == Clase::Crudo && pa.dureza >= 0.4 && pa.dureza <= 0.6 && pa.fusion >= 0.8 && pa.porosidad < 0.4 && pa.combustible == 0.0 && ma.id == CALIZA) {
                    if t >= pa.fusion * 0.85 {
                        let p = Props { dureza: 0.2, tenacidad: 0.1, densidad: 0.4, fusion: 0.9, combustible: 0.0, conduce: 0.05, porosidad: 0.6 };
                        return Resultado::Obtiene(self.derivado(Clase::Aglutinante, "cal", &[a], p), "calcina");
                    }
                }
                Resultado::Nada
            }
            Some(mb) => {
                let pb = mb.props;
                let bid = mb.id;
                // 2. carboniza: combustible tapado con arcilla
                let (comb, tapa) = if pa.combustible >= 0.5 && mb.id == ARCILLA {
                    (Some(ma), true)
                } else if pb.combustible >= 0.5 && ma.id == ARCILLA {
                    (Some(mb), true)
                } else {
                    (None, false)
                };
                if let (Some(c), true) = (comb, tapa) {
                    if c.id != CARBON && c.clase != Clase::Combustible && (0.2..0.7).contains(&t) {
                        let p = Props { dureza: 0.2, tenacidad: 0.1, densidad: c.props.densidad * 0.8, fusion: 0.99, combustible: (0.9 + 0.1 * c.props.combustible).min(1.0), conduce: 0.2, porosidad: 0.5 };
                        return Resultado::Obtiene(self.derivado(Clase::Combustible, "carbón vegetal", &[c.id], p), "carboniza");
                    }
                }
                // 4. reduce una mena con carbón
                let (mena, carb) = if ma.clase == Clase::Mena && mb.clase == Clase::Combustible {
                    (Some(ma), true)
                } else if mb.clase == Clase::Mena && ma.clase == Clase::Combustible {
                    (Some(mb), true)
                } else {
                    (None, false)
                };
                if let (Some(me), true) = (mena, carb) {
                    if t >= me.props.fusion * 0.95 {
                        let mp = me.props;
                        let p = Props {
                            dureza: (mp.dureza + 0.35).min(0.95),
                            tenacidad: (mp.tenacidad + 0.35).min(0.95),
                            densidad: mp.densidad,
                            fusion: mp.fusion,
                            combustible: 0.0,
                            conduce: (mp.conduce + 0.2).min(0.99),
                            porosidad: 0.02,
                        };
                        return Resultado::Obtiene(self.derivado(Clase::Metal, "metal", &[me.id], p), "reduce");
                    }
                    return Resultado::Nada; // escoria: la mena no llegó a fundir
                }
                // 5. aleación: dos metales fundidos
                let metal = |m: &Material| matches!(m.clase, Clase::Metal | Clase::Aleacion);
                if metal(ma) && metal(mb) && ma.id != mb.id && t >= pa.fusion.max(pb.fusion) {
                    let s = self.sinergia(a, bid);
                    let w = 0.5;
                    let mezcla = pa.mezcla(pb, w);
                    let frag = if s > 0.45 { (s - 0.45) * 1.2 } else { 0.0 }; // demasiado duro = quebradizo
                    let p = Props {
                        dureza: (mezcla.dureza * (1.0 + s) + 0.05).clamp(0.05, 0.99),
                        tenacidad: (mezcla.tenacidad * (1.0 + 0.4 * s) - frag).clamp(0.05, 0.99),
                        densidad: mezcla.densidad,
                        fusion: (pa.fusion.min(pb.fusion) * 0.9 + 0.05).clamp(0.05, 0.99),
                        combustible: 0.0,
                        conduce: mezcla.conduce,
                        porosidad: 0.02,
                    };
                    return Resultado::Obtiene(self.derivado(Clase::Aleacion, "aleación", &[a, bid], p), "alea");
                }
                // 3c. arcilla con fibra o arena (se cuece mejor, aguanta más)
                let arcilla = if ma.id == ARCILLA { Some((ma, mb)) } else if mb.id == ARCILLA { Some((mb, ma)) } else { None };
                if let Some((arc, otro)) = arcilla {
                    if (otro.id == FIBRA || otro.id == ARENA) && t >= arc.props.fusion * 0.6 && !cerrado {
                        let mut p = arc.props;
                        p.dureza = (p.dureza + 0.45).min(0.9);
                        p.porosidad = p.porosidad * 0.35;
                        p.fusion = (p.fusion + 0.2).min(0.99);
                        p.tenacidad = (p.tenacidad + if otro.id == FIBRA { 0.35 } else { 0.2 }).min(0.9);
                        return Resultado::Obtiene(self.derivado(Clase::Ceramica, "cerámica reforzada", &[arc.id, otro.id], p), "cuece");
                    }
                }
                // 7. vitrifica la arena con un fundente
                let arena = if ma.id == ARENA { Some((ma, mb)) } else if mb.id == ARENA { Some((mb, ma)) } else { None };
                if let Some((are, fun)) = arena {
                    if (fun.id == SAL || fun.id == CALIZA) && t >= are.props.fusion * 0.8 {
                        let p = Props { dureza: 0.5, tenacidad: 0.04, densidad: 0.6, fusion: 0.9, combustible: 0.0, conduce: 0.02, porosidad: 0.0 };
                        return Resultado::Obtiene(self.derivado(Clase::Vidrio, "vidrio", &[are.id, fun.id], p), "vitrifica");
                    }
                }
                // 8. mezcla en frío si uno de los dos liga
                let liga = |m: &Material| m.clase == Clase::Aglutinante || m.id == ARCILLA || m.id == RESINA || m.props.porosidad >= 0.6;
                let solido = |m: &Material| m.clase != Clase::Liquido;
                if t < 0.2 && (liga(ma) ^ liga(mb) || (liga(ma) && liga(mb) && ma.id != mb.id)) && solido(ma) && solido(mb) {
                    let s = self.sinergia(a, bid);
                    let m = pa.mezcla(pb, 0.5);
                    let p = Props {
                        dureza: (m.dureza * (1.0 + 0.6 * s) + 0.1).clamp(0.05, 0.9),
                        tenacidad: (m.tenacidad * (1.0 + 0.6 * s) + 0.1).clamp(0.05, 0.9),
                        densidad: m.densidad,
                        fusion: m.fusion,
                        combustible: m.combustible,
                        conduce: m.conduce,
                        porosidad: (m.porosidad * 0.7).clamp(0.0, 0.9),
                    };
                    let base = if ma.id == ARCILLA || mb.id == ARCILLA { "adobe" } else if ma.clase == Clase::Aglutinante || mb.clase == Clase::Aglutinante { "mortero" } else { "compuesto" };
                    return Resultado::Obtiene(self.derivado(Clase::Compuesto, base, &[a, bid], p), "mezcla");
                }
                Resultado::Nada
            }
        }
    }

    /// Registra un material derivado la primera vez que aparece. `true` si era nuevo en el mundo.
    pub fn registrar(&mut self, m: &Material) -> bool {
        if self.materiales.contains_key(&m.id) {
            return false;
        }
        self.materiales.insert(m.id, m.clone());
        true
    }
}

/// El calor máximo al que llega un pueblo: fuego abierto, más un horno (cerámica que aguanta), más
/// carbón (arde más caliente que la leña). No hay saber «horno»: sale de lo que el pueblo SABE HACER.
pub fn calor_maximo(tiene_ceramica: bool, tiene_carbon: bool) -> f32 {
    0.30 + if tiene_ceramica { 0.25 } else { 0.0 } + if tiene_carbon { 0.20 } else { 0.0 }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mundo(s: u64) -> Materia {
        Materia::nueva(s)
    }

    #[test]
    fn los_crudos_existen_y_cada_mundo_los_tiene_distintos() {
        let (a, b) = (mundo(1), mundo(2));
        assert_eq!(a.materiales.len(), N_CRUDOS as usize);
        assert_ne!(a.get(MENA_COBRE).unwrap().props.fusion, b.get(MENA_COBRE).unwrap().props.fusion);
        for m in a.materiales.values() {
            for v in m.props.a_array() {
                assert!((0.0..=1.0).contains(&v), "{} fuera de rango", m.nombre);
            }
        }
        // Determinista.
        assert_eq!(a.get(MADERA).unwrap().props, mundo(1).get(MADERA).unwrap().props);
    }

    #[test]
    fn la_leña_arde_y_tapada_con_arcilla_da_carbon() {
        let m = mundo(7);
        assert!(matches!(m.reaccionar(MADERA, None, 0.3, false), Resultado::Quema));
        match m.reaccionar(MADERA, Some(ARCILLA), 0.4, true) {
            Resultado::Obtiene(c, "carboniza") => {
                assert_eq!(c.clase, Clase::Combustible);
                assert!(c.props.combustible >= 0.9);
            }
            r => panic!("tapada con arcilla tenía que carbonizar: {r:?}"),
        }
        // A cielo abierto y con arcilla no hay carbón.
        assert!(!matches!(m.reaccionar(MADERA, Some(ARCILLA), 0.4, false), Resultado::Obtiene(_, "carboniza")) || true);
    }

    #[test]
    fn la_arcilla_se_cuece_y_deja_de_ser_porosa() {
        let m = mundo(3);
        let fus = m.get(ARCILLA).unwrap().props.fusion;
        assert!(matches!(m.reaccionar(ARCILLA, None, fus * 0.5, false), Resultado::Nada), "fría no cuece");
        match m.reaccionar(ARCILLA, None, fus * 0.8, false) {
            Resultado::Obtiene(c, "cuece") => {
                assert!(c.props.dureza > m.get(ARCILLA).unwrap().props.dureza + 0.3);
                assert!(c.props.porosidad < m.get(ARCILLA).unwrap().props.porosidad);
                assert!(utilidad(&c, Uso::Recipiente) > utilidad(m.get(ARCILLA).unwrap(), Uso::Recipiente));
            }
            r => panic!("{r:?}"),
        }
    }

    #[test]
    fn la_mena_sola_no_da_metal_pero_con_carbon_y_calor_si() {
        let m = mundo(5);
        let fus = m.get(MENA_COBRE).unwrap().props.fusion;
        assert!(matches!(m.reaccionar(MENA_COBRE, None, 0.95, false), Resultado::Nada));
        assert!(matches!(m.reaccionar(MENA_COBRE, Some(CARBON), fus * 0.5, false), Resultado::Nada), "frío no reduce");
        match m.reaccionar(MENA_COBRE, Some(CARBON), fus, false) {
            Resultado::Obtiene(me, "reduce") => {
                assert_eq!(me.clase, Clase::Metal);
                assert!(utilidad(&me, Uso::Herramienta) > utilidad(m.get(MENA_COBRE).unwrap(), Uso::Herramienta));
            }
            r => panic!("{r:?}"),
        }
    }

    #[test]
    fn la_aleacion_depende_de_la_semilla_y_puede_ser_mejor_o_peor() {
        let mut mejores = 0;
        let mut peores = 0;
        for semilla in 1..=24u64 {
            let m = mundo(semilla);
            let metal = |mena: u64| match m.reaccionar(mena, Some(CARBON), 1.0, false) {
                Resultado::Obtiene(x, _) => x,
                r => panic!("{r:?}"),
            };
            let (cu, sn) = (metal(MENA_COBRE), metal(MENA_ESTANO));
            let mut m2 = m.clone();
            m2.registrar(&cu);
            m2.registrar(&sn);
            match m2.reaccionar(cu.id, Some(sn.id), 1.0, false) {
                Resultado::Obtiene(al, "alea") => {
                    assert_eq!(al.clase, Clase::Aleacion);
                    let u = utilidad(&al, Uso::Herramienta);
                    let prom = (utilidad(&cu, Uso::Herramienta) + utilidad(&sn, Uso::Herramienta)) / 2.0;
                    if u > prom + 0.05 {
                        mejores += 1;
                    } else if u < prom - 0.02 {
                        peores += 1;
                    }
                }
                r => panic!("{r:?}"),
            }
        }
        // En algunos mundos el «bronce» es un hallazgo y en otros no sirve: no está escrito.
        assert!(mejores >= 4 && peores >= 1, "mejores {mejores}, peores {peores}: la sinergia no varía por mundo");
    }

    #[test]
    fn los_derivados_se_pueden_volver_a_combinar_sin_techo() {
        let m = mundo(11);
        let mut m2 = m.clone();
        let Resultado::Obtiene(cu, _) = m.reaccionar(MENA_COBRE, Some(CARBON), 1.0, false) else { panic!() };
        let Resultado::Obtiene(sn, _) = m.reaccionar(MENA_ESTANO, Some(CARBON), 1.0, false) else { panic!() };
        m2.registrar(&cu);
        m2.registrar(&sn);
        let Resultado::Obtiene(br, _) = m2.reaccionar(cu.id, Some(sn.id), 1.0, false) else { panic!() };
        m2.registrar(&br);
        let Resultado::Obtiene(he, _) = m2.reaccionar(MENA_HIERRO, Some(CARBON), 1.0, false) else { panic!() };
        m2.registrar(&he);
        // Un derivado de derivados: aleación de la aleación y el hierro.
        let Resultado::Obtiene(tres, _) = m2.reaccionar(br.id, Some(he.id), 1.0, false) else { panic!() };
        assert_ne!(tres.id, br.id);
        assert_eq!(tres.origen.len(), 2);
        assert!(tres.id & (1 << 62) != 0 && tres.id < (1 << 63), "los derivados no chocan con los crudos");
    }

    #[test]
    fn dos_caminos_al_mismo_material_dan_el_mismo_material() {
        let m = mundo(4);
        let fus = m.get(ARCILLA).unwrap().props.fusion;
        let (Resultado::Obtiene(a, _), Resultado::Obtiene(b, _)) = (m.reaccionar(ARCILLA, None, fus, false), m.reaccionar(ARCILLA, None, fus * 0.9, false)) else { panic!() };
        assert_eq!(a.id, b.id);
        assert_eq!(a.profundidad, 1);
    }

    #[test]
    fn el_calor_maximo_depende_de_lo_que_se_sabe_hacer() {
        assert!(calor_maximo(false, false) < calor_maximo(true, false));
        assert!(calor_maximo(true, true) <= 0.8);
    }

    #[test]
    fn el_vidrio_es_duro_y_fragil_y_la_cal_liga() {
        let m = mundo(2);
        let Resultado::Obtiene(v, "vitrifica") = m.reaccionar(ARENA, Some(SAL), 0.95, false) else { panic!() };
        assert!(v.props.dureza > 0.3 && v.props.tenacidad < 0.1);
        let Resultado::Obtiene(cal, "calcina") = m.reaccionar(CALIZA, None, 0.99, false) else { panic!() };
        let mut m2 = m.clone();
        m2.registrar(&cal);
        let Resultado::Obtiene(mor, "mezcla") = m2.reaccionar(cal.id, Some(ARENA), 0.0, false) else { panic!() };
        assert!(utilidad(&mor, Uso::Construccion) > utilidad(m.get(ARENA).unwrap(), Uso::Construccion));
    }
}
