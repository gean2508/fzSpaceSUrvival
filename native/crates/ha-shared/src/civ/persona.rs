//! Una persona NOTABLE: alguien con nombre, rasgos, saberes y parientes.
//! La multitud es el número `pob` del pueblo; las personas son quienes la mueven.

use serde::{Deserialize, Serialize};

use super::saber::Saberes;
use super::DIAS_ANIO;

#[derive(Clone, Copy, PartialEq, Eq, Debug, Default, Serialize, Deserialize)]
pub enum Oficio {
    #[default]
    Campesino,
    Artesano,
    Erudito,
    Soldado,
    Sacerdote,
    Mercader,
}

impl Oficio {
    pub fn etiqueta(self) -> &'static str {
        match self {
            Oficio::Campesino => "campesino",
            Oficio::Artesano => "artesano",
            Oficio::Erudito => "erudito",
            Oficio::Soldado => "soldado",
            Oficio::Sacerdote => "sacerdote",
            Oficio::Mercader => "mercader",
        }
    }
}

#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize)]
pub struct Estudio {
    pub saber: u8,
    pub prog: f32,
    /// Costo total ya fijado al elegir (con enseñanza / artefacto aplicados).
    pub costo: f32,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Persona {
    pub id: u32,
    pub nombre: String,
    pub casa: u32,
    pub pueblo: u32,
    pub nacio: i64,
    pub mujer: bool,
    // rasgos 0..1
    pub inteligencia: f32,
    pub curiosidad: f32,
    pub ambicion: f32,
    pub carisma: f32,
    pub fuerza: f32,
    pub bondad: f32,
    pub fe: f32,
    pub codicia: f32,
    pub audacia: f32,
    pub oficio: Oficio,
    pub saberes: Saberes,
    pub estudio: Option<Estudio>,
    pub pareja: Option<u32>,
    pub padre: Option<u32>,
    pub madre: Option<u32>,
    pub hijos: u16,
    pub riqueza: f32,
    pub fama: f32,
    pub culto: u8,
    pub preso_hasta: u64,
    /// Id del `NpcRecord` del juego que la encarna ahora mismo (0 = ninguno).
    pub npc: u32,
}

impl Persona {
    pub fn edad(&self, dia: u64) -> f32 {
        (dia as i64 - self.nacio) as f32 / DIAS_ANIO as f32
    }
    pub fn adulta(&self, dia: u64) -> bool {
        self.edad(dia) >= 16.0
    }
    pub fn libre(&self, dia: u64) -> bool {
        self.preso_hasta <= dia
    }
    pub fn parientes_cercanos(&self, o: &Persona) -> bool {
        self.id == o.id
            || self.padre.is_some() && self.padre == o.padre
            || self.madre.is_some() && self.madre == o.madre
            || Some(o.id) == self.padre
            || Some(o.id) == self.madre
            || Some(self.id) == o.padre
            || Some(self.id) == o.madre
    }
}
