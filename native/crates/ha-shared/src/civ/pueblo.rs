//! Un pueblo (aldea, villa, ciudad o colonia) como unidad política.

use serde::{Deserialize, Serialize};

use super::saber::{Capacidad, Saberes, N_SABERES};

pub const N_FUENTES: usize = 5;
pub const NOMBRE_FUENTE: [&str; N_FUENTES] = ["la fuerza", "el linaje", "la fe", "el voto", "la riqueza"];

/// La CONVENCIÓN: cuánto pesa cada fuente de poder para que la gente acepte a
/// quien manda. Nadie decreta "esto es una monarquía": el jefe es quien más pesa
/// según ESTOS pesos, y el nombre del régimen se deduce de cuál domina. Si alguien
/// gana por una fuente que la convención no valoraba y el pueblo lo acepta, la
/// convención se corre hacia esa fuente (así nacen las repúblicas y los golpes).
#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub struct Convencion {
    pub w: [f32; N_FUENTES],
}

impl Convencion {
    pub const FUERZA: usize = 0;
    pub const LINAJE: usize = 1;
    pub const FE: usize = 2;
    pub const VOTO: usize = 3;
    pub const RIQUEZA: usize = 4;

    pub fn nueva(w: [f32; N_FUENTES]) -> Self {
        let mut c = Self { w };
        c.normalizar();
        c
    }
    pub fn normalizar(&mut self) {
        let mut t = 0.0;
        for x in self.w.iter_mut() {
            *x = x.max(0.02);
            t += *x;
        }
        for x in self.w.iter_mut() {
            *x /= t;
        }
    }
    pub fn empujar(&mut self, fuente: usize, cuanto: f32) {
        self.w[fuente] += cuanto;
        self.normalizar();
    }
    pub fn dominante(&self) -> usize {
        let mut m = 0;
        for i in 1..N_FUENTES {
            if self.w[i] > self.w[m] {
                m = i;
            }
        }
        m
    }
}

impl Default for Convencion {
    fn default() -> Self {
        Self::nueva([0.3, 0.3, 0.15, 0.15, 0.1])
    }
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub struct Leyes {
    pub impuesto: f32,
    /// 0 multa · 1 cárcel · 2 destierro · 3 muerte
    pub pena_robo: u8,
    pub pena_homicidio: u8,
    /// 0..1: cuánto se tolera otro culto.
    pub libertad_culto: f32,
    /// 0..1: fracción de la gente que sirve en armas.
    pub servicio: f32,
}

impl Default for Leyes {
    fn default() -> Self {
        Self { impuesto: 0.10, pena_robo: 1, pena_homicidio: 2, libertad_culto: 0.6, servicio: 0.15 }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub enum Obra {
    Biblioteca,
    Escuela,
    Muralla,
    Nave,
    Refugio,
}

impl Obra {
    pub fn nombre(self) -> &'static str {
        match self {
            Obra::Biblioteca => "biblioteca",
            Obra::Escuela => "escuela",
            Obra::Muralla => "muralla",
            Obra::Nave => "nave",
            Obra::Refugio => "refugio antiatómico",
        }
    }
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub struct Proyecto {
    pub obra: Obra,
    pub prog: f32,
    pub costo: f32,
    /// Sólo `Nave`: cuerpo destino.
    pub destino: u8,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Pueblo {
    pub id: u32,
    pub nombre: String,
    /// Cuerpo celeste (`CelestialBody as u8`).
    pub cuerpo: u8,
    pub x: f32,
    pub z: f32,
    /// 0 aldea · 1 villa · 2 ciudad
    pub rango: u8,
    pub sustrato: u32,
    /// Qué tan vivible es el sitio (Tierra 1.0; colonias menos).
    pub habit: f32,
    pub pob: f32,
    pub carga: f32,
    pub comida: f32,
    pub mat: f32,
    pub tesoro: f32,
    pub saberes: Saberes,
    pub archivo: Saberes,
    pub sin_portador: Vec<u16>,
    pub conv: Convencion,
    pub casa_reinante: u32,
    pub gobernante: Option<u32>,
    pub gobierna_desde: u64,
    pub leyes: Leyes,
    pub legit: f32,
    pub orden: f32,
    pub moral: f32,
    pub hambre: f32,
    pub crimen: f32,
    pub culto: u8,
    pub biblioteca: u8,
    pub escuela: u8,
    pub muralla: u8,
    pub proyecto: Option<Proyecto>,
    pub gente: Vec<u32>,
    /// A quién le debe vasallaje.
    pub senor: Option<u32>,
    pub fundado: u64,
    pub ult_eleccion: u64,
    pub vivo: bool,
    pub bajas: f32,
    pub vecinos: Vec<u32>,
    /// Lo que la gente cree de las leyes del universo (contra la verdad en `fisica`).
    #[serde(default)]
    pub creencias: super::fisica::Creencias,
    /// Ojivas nucleares listas.
    #[serde(default)]
    pub arsenal: u16,
    /// 0..1: lluvia radiactiva que todavía envenena el lugar.
    #[serde(default)]
    pub radiacion: f32,
    /// Nivel de refugios antiatómicos (0..3).
    #[serde(default)]
    pub refugio: u8,
    #[serde(default)]
    pub golpeado: u64,
    #[serde(default)]
    pub golpeado_por: u32,
}

impl Pueblo {
    pub fn capacidad(&self) -> Capacidad {
        Capacidad::de(self.saberes)
    }

    pub fn vacio(id: u32) -> Self {
        Self {
            id,
            nombre: String::new(),
            cuerpo: 0,
            x: 0.0,
            z: 0.0,
            rango: 0,
            sustrato: 0,
            habit: 1.0,
            pob: 50.0,
            carga: 100.0,
            comida: 100.0,
            mat: 0.0,
            tesoro: 0.0,
            saberes: Saberes::NADA,
            archivo: Saberes::NADA,
            sin_portador: vec![0; N_SABERES],
            conv: Convencion::default(),
            casa_reinante: 0,
            gobernante: None,
            gobierna_desde: 0,
            leyes: Leyes::default(),
            legit: 0.7,
            orden: 0.7,
            moral: 0.6,
            hambre: 0.0,
            crimen: 0.0,
            culto: 0,
            biblioteca: 0,
            escuela: 0,
            muralla: 0,
            proyecto: None,
            gente: Vec::new(),
            senor: None,
            fundado: 0,
            ult_eleccion: 0,
            vivo: true,
            bajas: 0.0,
            vecinos: Vec::new(),
            creencias: super::fisica::Creencias::default(),
            arsenal: 0,
            radiacion: 0.0,
            refugio: 0,
            golpeado: 0,
            golpeado_por: 0,
        }
    }

    /// El nombre del régimen, DEDUCIDO de la convención (no decretado).
    pub fn regimen(&self) -> &'static str {
        let f = self.conv.dominante();
        let grande = self.rango >= 1 && self.pob > 400.0;
        match f {
            Convencion::FUERZA => if grande { "dictadura militar" } else { "caudillaje" },
            Convencion::LINAJE => if grande { "monarquía" } else { "cacicazgo" },
            Convencion::FE => "teocracia",
            Convencion::VOTO => if grande { "república" } else { "asamblea" },
            _ => "oligarquía",
        }
    }
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub struct Relacion {
    pub a: u32,
    pub b: u32,
    /// -1 odio .. +1 afecto
    pub actitud: f32,
    pub comercio: bool,
    pub alianza: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Guerra {
    pub id: u32,
    pub bando_a: Vec<u32>,
    pub bando_b: Vec<u32>,
    pub inicio: u64,
    /// Ventaja acumulada de A sobre B, -1..1.
    pub avance: f32,
    pub causa: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Artefacto {
    pub id: u32,
    /// El saber que encarna (y toda su cadena de prerrequisitos).
    pub saber: u8,
    pub pueblo: u32,
    pub nombre: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Expedicion {
    pub id: u32,
    pub origen: u32,
    pub destino: u8,
    pub salida: u64,
    pub llegada: u64,
    pub tripulacion: Vec<u32>,
    pub provisiones: f32,
    pub calidad: f32,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Culto {
    pub id: u8,
    pub nombre: String,
    pub fundador: String,
    pub dia: u64,
    /// -1 misericordioso .. +1 severo
    pub severidad: f32,
}
