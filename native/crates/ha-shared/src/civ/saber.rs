//! La FÍSICA del conocimiento.
//!
//! Nadie le dicta a la gente cuándo inventar nada: esto sólo dice qué hace falta
//! saber y tener a mano para que una idea sea POSIBLE, como la gravedad dice qué se
//! cae. Que alguien la busque, cuándo, y si la historia llega o no a la Luna, lo
//! deciden las personas (`ciencia.rs`) según su curiosidad, su necesidad y lo que
//! encuentren tirado.
//!
//! Cada saber pide:
//! - **prerrequisitos**: otros saberes que quien investiga ya tiene en la cabeza;
//! - **sustrato**: algo físico en el lugar (hierro, petróleo, cielo despejado...);
//! - **costo**: días-persona de estudio dedicado.
//!
//! Y da **efectos** sobre lo que un pueblo puede hacer (comer, fabricar, pelear,
//! investigar, curar, llegar lejos).

use serde::{Deserialize, Serialize};

// --- sustratos (lo que el lugar tiene) ---------------------------------------
pub const S_PIEDRA: u32 = 1 << 0;
pub const S_MADERA: u32 = 1 << 1;
pub const S_AGUA: u32 = 1 << 2;
pub const S_FERTIL: u32 = 1 << 3;
pub const S_ARCILLA: u32 = 1 << 4;
pub const S_COBRE: u32 = 1 << 5;
pub const S_HIERRO: u32 = 1 << 6;
pub const S_CARBON: u32 = 1 << 7;
pub const S_PETROLEO: u32 = 1 << 8;
pub const S_CIELO: u32 = 1 << 9;
pub const S_ANIMALES: u32 = 1 << 10;
pub const S_PLANTAS: u32 = 1 << 11;
pub const S_ARENA: u32 = 1 << 12;
pub const S_URANIO: u32 = 1 << 13;
pub const S_MAR: u32 = 1 << 14;

/// (bit, probabilidad de que un sitio cualquiera lo tenga).
pub const SUSTRATOS: [(u32, f32); 15] = [
    (S_PIEDRA, 0.90),
    (S_MADERA, 0.80),
    (S_AGUA, 0.90),
    (S_FERTIL, 0.72),
    (S_ARCILLA, 0.50),
    (S_COBRE, 0.35),
    (S_HIERRO, 0.30),
    (S_CARBON, 0.26),
    (S_PETROLEO, 0.14),
    (S_CIELO, 0.80),
    (S_ANIMALES, 0.80),
    (S_PLANTAS, 0.70),
    (S_ARENA, 0.42),
    (S_URANIO, 0.06),
    (S_MAR, 0.25),
];

// --- índices de saberes -------------------------------------------------------
pub const FUEGO: u8 = 0;
pub const PIEDRA_TALLADA: u8 = 1;
pub const AGRICULTURA: u8 = 2;
pub const DOMESTICACION: u8 = 3;
pub const ALFARERIA: u8 = 4;
pub const TEJIDO: u8 = 5;
pub const RUEDA: u8 = 6;
pub const ESCRITURA: u8 = 7;
pub const MATEMATICA: u8 = 8;
pub const ASTRONOMIA: u8 = 9;
pub const BRONCE: u8 = 10;
pub const HIERRO: u8 = 11;
pub const NAVEGACION: u8 = 12;
pub const MEDICINA: u8 = 13;
pub const ARQUITECTURA: u8 = 14;
pub const DERECHO: u8 = 15;
pub const MONEDA: u8 = 16;
pub const MILICIA: u8 = 17;
pub const FILOSOFIA: u8 = 18;
pub const IMPRENTA: u8 = 19;
pub const QUIMICA: u8 = 20;
pub const POLVORA: u8 = 21;
pub const MECANICA: u8 = 22;
pub const VAPOR: u8 = 23;
pub const ELECTRICIDAD: u8 = 24;
pub const COMBUSTION: u8 = 25;
pub const TELECOM: u8 = 26;
pub const COMPUTACION: u8 = 27;
pub const AVIACION: u8 = 28;
pub const MATERIALES: u8 = 29;
pub const MEDICINA_MODERNA: u8 = 30;
pub const NUCLEAR: u8 = 31;
pub const COHETERIA: u8 = 32;
pub const MECANICA_ORBITAL: u8 = 33;
pub const SOPORTE_VITAL: u8 = 34;
pub const ASTRONAUTICA: u8 = 35;
pub const PROPULSION_AVANZADA: u8 = 36;
pub const PROPULSION_PROFUNDA: u8 = 37;
pub const OPTICA: u8 = 38;
pub const METODO: u8 = 39;
pub const CUANTICA: u8 = 40;
pub const ARMAS_NUCLEARES: u8 = 41;
pub const COMPUTACION_CUANTICA: u8 = 42;
pub const FUSION: u8 = 43;
pub const N_SABERES: usize = 44;

/// Hasta dónde llega un pueblo que sabe esto. Crece con el saber.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug, Serialize, Deserialize)]
pub enum Alcance {
    Tierra = 0,
    Mar = 1,
    Aire = 2,
    Luna = 3,
    Planetas = 4,
    Exterior = 5,
}

#[derive(Clone, Copy, Default, Debug)]
pub struct Efecto {
    pub comida: f32,
    pub mat: f32,
    pub mil: f32,
    pub inv: f32,
    pub salud: f32,
    pub orden: f32,
    pub com: f32,
    pub defensa: f32,
}

const fn ef(comida: f32, mat: f32, mil: f32, inv: f32, salud: f32, orden: f32, com: f32, defensa: f32) -> Efecto {
    Efecto { comida, mat, mil, inv, salud, orden, com, defensa }
}

pub struct Def {
    pub nombre: &'static str,
    pub pre: &'static [u8],
    /// Días-persona de estudio dedicado (a inteligencia media).
    pub costo: f32,
    pub sustrato: u32,
    pub efecto: Efecto,
    pub alcance: Alcance,
    /// Las ideas de frontera sólo las persigue quien sueña con lo lejano
    /// (rasgo `explorador`): nadie llega a la Luna por accidente.
    pub frontera: bool,
    /// Cuánta gente hace falta en el pueblo para que la práctica sea corriente.
    pub masa: f32,
}

const fn d(
    nombre: &'static str,
    pre: &'static [u8],
    costo: f32,
    sustrato: u32,
    efecto: Efecto,
) -> Def {
    Def { nombre, pre, costo, sustrato, efecto, alcance: Alcance::Tierra, frontera: false, masa: 0.0 }
}

const fn con(mut x: Def, alcance: Alcance, frontera: bool, masa: f32) -> Def {
    x.alcance = alcance;
    x.frontera = frontera;
    x.masa = masa;
    x
}

//                                      comida mat  mil   inv   salud orden com   defensa
pub static DEFS: [Def; N_SABERES] = [
    d("Fuego", &[], 6.0, S_MADERA, ef(0.10, 0.0, 0.0, 0.0, 0.05, 0.0, 0.0, 0.0)),
    d("Piedra tallada", &[], 6.0, S_PIEDRA, ef(0.0, 0.10, 0.10, 0.0, 0.0, 0.0, 0.0, 0.0)),
    d("Agricultura", &[PIEDRA_TALLADA], 20.0, S_FERTIL, ef(0.80, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0)),
    d("Domesticación", &[AGRICULTURA], 25.0, S_ANIMALES, ef(0.30, 0.0, 0.10, 0.0, 0.0, 0.0, 0.10, 0.0)),
    d("Alfarería", &[FUEGO], 15.0, S_ARCILLA, ef(0.10, 0.0, 0.0, 0.0, 0.0, 0.0, 0.10, 0.0)),
    d("Tejido", &[], 12.0, S_PLANTAS, ef(0.0, 0.0, 0.0, 0.0, 0.05, 0.0, 0.10, 0.0)),
    d("Rueda", &[PIEDRA_TALLADA], 20.0, S_MADERA, ef(0.0, 0.15, 0.0, 0.0, 0.0, 0.0, 0.20, 0.0)),
    d("Escritura", &[ALFARERIA], 40.0, 0, ef(0.0, 0.0, 0.0, 0.30, 0.0, 0.10, 0.10, 0.0)),
    d("Matemática", &[ESCRITURA], 60.0, 0, ef(0.0, 0.0, 0.0, 0.20, 0.0, 0.0, 0.0, 0.0)),
    d("Astronomía", &[MATEMATICA], 80.0, S_CIELO, ef(0.05, 0.0, 0.0, 0.10, 0.0, 0.0, 0.0, 0.0)),
    d("Bronce", &[FUEGO, PIEDRA_TALLADA], 50.0, S_COBRE, ef(0.0, 0.40, 0.50, 0.0, 0.0, 0.0, 0.10, 0.0)),
    d("Hierro", &[BRONCE], 80.0, S_HIERRO, ef(0.20, 0.50, 0.70, 0.0, 0.0, 0.0, 0.0, 0.0)),
    con(d("Navegación", &[TEJIDO, MATEMATICA], 70.0, S_MAR, ef(0.05, 0.0, 0.0, 0.0, 0.0, 0.0, 0.40, 0.0)), Alcance::Mar, false, 0.0),
    d("Medicina", &[ESCRITURA], 70.0, S_PLANTAS, ef(0.0, 0.0, 0.0, 0.0, 0.35, 0.0, 0.0, 0.0)),
    d("Arquitectura", &[PIEDRA_TALLADA, MATEMATICA], 70.0, 0, ef(0.0, 0.10, 0.0, 0.0, 0.0, 0.05, 0.0, 0.30)),
    d("Derecho", &[ESCRITURA], 80.0, 0, ef(0.0, 0.0, 0.0, 0.0, 0.0, 0.40, 0.0, 0.0)),
    d("Moneda", &[BRONCE, ESCRITURA], 60.0, 0, ef(0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.50, 0.0)),
    d("Milicia", &[BRONCE, DERECHO], 70.0, 0, ef(0.0, 0.0, 0.60, 0.0, 0.0, 0.05, 0.0, 0.10)),
    d("Filosofía", &[ESCRITURA, MATEMATICA], 100.0, 0, ef(0.0, 0.0, 0.0, 0.40, 0.0, 0.10, 0.0, 0.0)),
    d("Imprenta", &[ESCRITURA, HIERRO], 120.0, 0, ef(0.0, 0.0, 0.0, 0.50, 0.0, 0.0, 0.10, 0.0)),
    d("Química", &[MEDICINA, BRONCE, FILOSOFIA], 150.0, 0, ef(0.10, 0.0, 0.20, 0.10, 0.10, 0.0, 0.0, 0.0)),
    d("Pólvora", &[QUIMICA, HIERRO], 130.0, 0, ef(0.0, 0.0, 1.00, 0.0, 0.0, 0.0, 0.0, 0.0)),
    d("Mecánica", &[RUEDA, HIERRO, MATEMATICA], 160.0, 0, ef(0.20, 0.50, 0.20, 0.0, 0.0, 0.0, 0.10, 0.0)),
    d("Máquina de vapor", &[MECANICA], 200.0, S_CARBON, ef(0.20, 1.00, 0.0, 0.0, 0.0, 0.0, 0.40, 0.0)),
    d("Electricidad", &[MECANICA, QUIMICA], 260.0, S_COBRE, ef(0.0, 0.60, 0.0, 0.30, 0.10, 0.0, 0.0, 0.0)),
    d("Combustión interna", &[VAPOR, QUIMICA], 260.0, S_PETROLEO, ef(0.50, 0.60, 1.00, 0.0, 0.0, 0.0, 0.50, 0.0)),
    d("Telecomunicaciones", &[ELECTRICIDAD], 220.0, 0, ef(0.0, 0.0, 0.0, 0.40, 0.0, 0.10, 0.30, 0.0)),
    d("Computación", &[ELECTRICIDAD, MATEMATICA], 320.0, S_ARENA, ef(0.0, 0.40, 0.0, 1.00, 0.0, 0.0, 0.0, 0.0)),
    con(d("Aviación", &[COMBUSTION, MECANICA], 300.0, 0, ef(0.0, 0.0, 1.50, 0.0, 0.0, 0.0, 0.30, 0.0)), Alcance::Aire, false, 0.0),
    d("Materiales avanzados", &[QUIMICA, ELECTRICIDAD, HIERRO], 350.0, S_ARENA, ef(0.0, 0.80, 0.30, 0.0, 0.0, 0.0, 0.0, 0.0)),
    d("Medicina moderna", &[MEDICINA, QUIMICA, ELECTRICIDAD], 250.0, 0, ef(0.0, 0.0, 0.0, 0.0, 0.60, 0.0, 0.0, 0.0)),
    d("Energía nuclear", &[ELECTRICIDAD, QUIMICA, MATEMATICA], 500.0, S_URANIO, ef(0.0, 1.00, 0.30, 0.0, 0.0, 0.0, 0.0, 0.0)),
    con(d("Cohetería", &[COMBUSTION, MATERIALES, QUIMICA], 450.0, S_PETROLEO, ef(0.0, 0.0, 1.00, 0.0, 0.0, 0.0, 0.0, 0.0)), Alcance::Aire, true, 0.0),
    con(d("Mecánica orbital", &[ASTRONOMIA, COMPUTACION, MECANICA], 500.0, S_CIELO, ef(0.0, 0.0, 0.0, 0.30, 0.0, 0.0, 0.0, 0.0)), Alcance::Aire, true, 0.0),
    con(d("Soporte vital", &[MEDICINA_MODERNA, MATERIALES], 450.0, 0, ef(0.0, 0.0, 0.0, 0.0, 0.20, 0.0, 0.0, 0.0)), Alcance::Aire, true, 0.0),
    con(d("Astronáutica", &[COHETERIA, MECANICA_ORBITAL, SOPORTE_VITAL], 700.0, 0, ef(0.0, 0.30, 0.0, 0.30, 0.0, 0.0, 0.0, 0.0)), Alcance::Luna, true, 0.0),
    con(d("Propulsión avanzada", &[ASTRONAUTICA, NUCLEAR, COMPUTACION], 900.0, 0, ef(0.0, 0.0, 0.0, 0.20, 0.0, 0.0, 0.0, 0.0)), Alcance::Planetas, true, 0.0),
    con(d("Propulsión profunda", &[PROPULSION_AVANZADA, MATERIALES], 1400.0, 0, ef(0.0, 0.0, 0.0, 0.20, 0.0, 0.0, 0.0, 0.0)), Alcance::Exterior, true, 0.0),
    d("Óptica", &[MATEMATICA, ALFARERIA], 90.0, S_ARENA, ef(0.0, 0.0, 0.0, 0.20, 0.05, 0.0, 0.0, 0.0)),
    d("Método científico", &[FILOSOFIA, MATEMATICA, IMPRENTA], 160.0, 0, ef(0.05, 0.10, 0.0, 0.60, 0.05, 0.0, 0.0, 0.0)),
    d("Mecánica cuántica", &[METODO, ELECTRICIDAD], 420.0, 0, ef(0.0, 0.20, 0.0, 0.50, 0.0, 0.0, 0.0, 0.0)),
    d("Armas nucleares", &[NUCLEAR, POLVORA], 400.0, 0, ef(0.0, 0.0, 2.00, 0.0, 0.0, 0.0, 0.0, 0.30)),
    d("Computación cuántica", &[COMPUTACION, CUANTICA], 650.0, 0, ef(0.0, 0.30, 0.0, 1.20, 0.10, 0.0, 0.20, 0.0)),
    d("Fusión nuclear", &[NUCLEAR, CUANTICA, COMPUTACION], 1000.0, 0, ef(0.20, 2.00, 0.0, 0.40, 0.0, 0.0, 0.0, 0.0)),
];

/// Un conjunto de saberes: 44 bits caben en un `u64`.
#[derive(Clone, Copy, Default, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct Saberes(pub u64);

impl Saberes {
    pub const NADA: Saberes = Saberes(0);

    pub fn con(self, s: u8) -> Self {
        Self(self.0 | (1u64 << s))
    }
    pub fn tiene(self, s: u8) -> bool {
        self.0 & (1u64 << s) != 0
    }
    pub fn sin(self, s: u8) -> Self {
        Self(self.0 & !(1u64 << s))
    }
    pub fn union(self, o: Self) -> Self {
        Self(self.0 | o.0)
    }
    pub fn contiene(self, o: Self) -> bool {
        self.0 & o.0 == o.0
    }
    pub fn cuenta(self) -> u32 {
        self.0.count_ones()
    }
    pub fn iter(self) -> impl Iterator<Item = u8> {
        (0..N_SABERES as u8).filter(move |s| self.tiene(*s))
    }
    pub fn de(lista: &[u8]) -> Self {
        lista.iter().fold(Self::NADA, |a, s| a.con(*s))
    }
}

/// ¿Están todos los prerrequisitos de `s` en `sabe`?
pub fn prerrequisitos_ok(s: u8, sabe: Saberes) -> bool {
    DEFS[s as usize].pre.iter().all(|p| sabe.tiene(*p))
}

/// Cuántos prerrequisitos directos faltan.
pub fn faltan(s: u8, sabe: Saberes) -> usize {
    DEFS[s as usize].pre.iter().filter(|p| !sabe.tiene(**p)).count()
}

/// Lo que un pueblo puede hacer, dado lo que se practica en él.
#[derive(Clone, Copy, Debug, Default)]
pub struct Capacidad {
    pub comida: f32,
    pub mat: f32,
    pub mil: f32,
    pub inv: f32,
    pub salud: f32,
    pub orden: f32,
    pub com: f32,
    pub defensa: f32,
    pub alcance: Option<Alcance>,
}

impl Capacidad {
    pub fn de(sabe: Saberes) -> Self {
        let mut c = Capacidad::default();
        let mut alc = Alcance::Tierra;
        for s in sabe.iter() {
            let e = DEFS[s as usize].efecto;
            c.comida += e.comida;
            c.mat += e.mat;
            c.mil += e.mil;
            c.inv += e.inv;
            c.salud += e.salud;
            c.orden += e.orden;
            c.com += e.com;
            c.defensa += e.defensa;
            alc = alc.max(DEFS[s as usize].alcance);
        }
        c.alcance = Some(alc);
        c
    }

    pub fn alcance(&self) -> Alcance {
        self.alcance.unwrap_or(Alcance::Tierra)
    }
}

/// ¿Qué alcance hace falta para llegar a un cuerpo? `None` = inalcanzable.
pub fn alcance_para(cuerpo: u8) -> Option<Alcance> {
    match cuerpo {
        0 => Some(Alcance::Tierra),
        1 => Some(Alcance::Luna),
        2 | 3 | 4 | 5 | 6 => Some(Alcance::Planetas),
        7 | 8 | 9 | 10 | 11 => Some(Alcance::Exterior),
        _ => None,
    }
}

/// Saberes que un sitio de cierto rango ya tiene al arrancar el mundo. Las aldeas
/// viven de la tierra; los pueblos ya escriben y forjan; las ciudades son modernas
/// pero NO tienen el programa espacial (se perdió): ver `Astronáutica`.
pub fn saberes_iniciales(rango: u8, h: u64) -> Saberes {
    let base: &[u8] = &[FUEGO, PIEDRA_TALLADA, AGRICULTURA, DOMESTICACION, ALFARERIA, TEJIDO, RUEDA];
    let pueblo: &[u8] = &[
        ESCRITURA, MATEMATICA, BRONCE, HIERRO, NAVEGACION, MEDICINA, ARQUITECTURA, DERECHO, MONEDA, MILICIA,
    ];
    let ciudad: &[u8] = &[
        ASTRONOMIA, OPTICA, FILOSOFIA, IMPRENTA, METODO, QUIMICA, POLVORA, MECANICA, VAPOR, ELECTRICIDAD, COMBUSTION,
        TELECOM, COMPUTACION, AVIACION, MATERIALES, MEDICINA_MODERNA,
    ];
    let mut s = Saberes::de(base);
    if rango >= 1 {
        s = s.union(Saberes::de(pueblo));
    }
    if rango >= 2 {
        s = s.union(Saberes::de(ciudad));
    }
    // Un pueblo/aldea no siempre lo sabe todo lo de su escalón: lo perdió o nunca llegó.
    if rango < 2 {
        for (i, sab) in pueblo.iter().enumerate() {
            if rango == 0 || rango == 1 {
                let r = super::azar::unidad(h ^ (i as u64 + 1) * 0x51ED);
                if rango == 0 && r < 0.75 && rango < 1 {
                    s = s.sin(*sab);
                }
                if rango == 1 && r < 0.12 {
                    s = s.sin(*sab);
                }
            }
        }
        // sin prerrequisitos no hay saber: limpieza de huecos
        let mut cambio = true;
        while cambio {
            cambio = false;
            for x in s.iter().collect::<Vec<_>>() {
                if !prerrequisitos_ok(x, s) {
                    s = s.sin(x);
                    cambio = true;
                }
            }
        }
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn la_tabla_esta_en_orden_y_es_un_dag() {
        assert_eq!(DEFS.len(), N_SABERES);
        for (i, d) in DEFS.iter().enumerate() {
            for p in d.pre {
                assert!((*p as usize) < i, "{} depende de un saber posterior", d.nombre);
            }
            assert!(d.costo > 0.0);
        }
        assert_eq!(DEFS[ASTRONAUTICA as usize].nombre, "Astronáutica");
        assert_eq!(DEFS[FUEGO as usize].nombre, "Fuego");
        assert_eq!(DEFS[PROPULSION_PROFUNDA as usize].nombre, "Propulsión profunda");
        assert_eq!(DEFS[COHETERIA as usize].nombre, "Cohetería");
        assert_eq!(DEFS[COMPUTACION as usize].nombre, "Computación");
        assert_eq!(DEFS[MEDICINA_MODERNA as usize].nombre, "Medicina moderna");
    }

    #[test]
    fn se_puede_llegar_de_la_nada_a_la_luna() {
        // Cerradura: repetir "aprender todo lo alcanzable" tiene que llegar a Astronáutica.
        let mut s = Saberes::NADA;
        loop {
            let antes = s;
            for i in 0..N_SABERES as u8 {
                if prerrequisitos_ok(i, s) {
                    s = s.con(i);
                }
            }
            if s == antes {
                break;
            }
        }
        assert_eq!(s.cuenta() as usize, N_SABERES);
    }

    #[test]
    fn las_ciudades_arrancan_sin_programa_espacial() {
        let s = saberes_iniciales(2, 7);
        assert!(s.tiene(COMBUSTION) && s.tiene(COMPUTACION));
        assert!(!s.tiene(ASTRONAUTICA) && !s.tiene(COHETERIA));
        assert!(Capacidad::de(s).alcance() < Alcance::Luna);
    }

    #[test]
    fn las_aldeas_arrancan_coherentes() {
        for h in 0..200u64 {
            let s = saberes_iniciales(0, h);
            for x in s.iter() {
                assert!(prerrequisitos_ok(x, s), "hueco en aldea (h={h}, saber={})", DEFS[x as usize].nombre);
            }
        }
    }
}
