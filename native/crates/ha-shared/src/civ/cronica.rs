//! La CRÓNICA: lo que pasó, en castellano, para quien mire el mundo desde arriba.
//! Es un anillo con cursor monótono: el juego pide "lo nuevo desde X".

use serde::{Deserialize, Serialize};
use std::collections::VecDeque;

use super::DIAS_ANIO;

#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub enum Tipo {
    Descubrimiento,
    Hito,
    Olvido,
    Eleccion,
    Sucesion,
    Golpe,
    Revolucion,
    Ley,
    Guerra,
    Batalla,
    Paz,
    Alianza,
    Anexion,
    Independencia,
    Fundacion,
    Lanzamiento,
    Llegada,
    Tragedia,
    Hallazgo,
    Culto,
    Crimen,
    Justicia,
    Hambruna,
    Obra,
    Jugador,
    Muerte,
}

impl Tipo {
    /// Cuánto pesa para la vista de lo importante (0 = ruido, 3 = historia).
    pub fn peso(self) -> u8 {
        use Tipo::*;
        match self {
            Hito | Lanzamiento | Llegada | Independencia | Anexion | Guerra | Revolucion | Paz => 3,
            Descubrimiento | Golpe | Eleccion | Sucesion | Fundacion | Tragedia | Hallazgo | Culto
            | Hambruna | Jugador => 2,
            Olvido | Ley | Alianza | Obra | Justicia | Batalla => 1,
            Crimen | Muerte => 0,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Hecho {
    pub id: u64,
    pub dia: u64,
    pub tipo: Tipo,
    pub pueblo: u32,
    pub otro: u32,
    pub texto: String,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Cronica {
    pub hechos: VecDeque<Hecho>,
    pub siguiente: u64,
    /// Cuántos hechos de cada tipo hubo en total (aunque ya no estén en la lista).
    #[serde(default)]
    pub totales: std::collections::BTreeMap<String, u32>,
}

pub const CAPACIDAD: usize = 1200;

impl Cronica {
    pub fn anotar(&mut self, dia: u64, tipo: Tipo, pueblo: u32, otro: u32, texto: String) {
        self.hechos.push_back(Hecho { id: self.siguiente, dia, tipo, pueblo, otro, texto });
        self.siguiente += 1;
        *self.totales.entry(format!("{tipo:?}")).or_insert(0) += 1;
        while self.hechos.len() > CAPACIDAD {
            // lo trivial se olvida antes que lo que cambió el mundo
            let i = self.hechos.iter().take(CAPACIDAD / 2).position(|h| h.tipo.peso() <= 2).unwrap_or(0);
            self.hechos.remove(i);
        }
    }

    /// Los hechos con `id >= cursor`.
    pub fn desde(&self, cursor: u64) -> impl Iterator<Item = &Hecho> {
        self.hechos.iter().filter(move |h| h.id >= cursor)
    }

    pub fn ultimos(&self, n: usize, peso_min: u8) -> Vec<&Hecho> {
        let mut v: Vec<&Hecho> = self.hechos.iter().rev().filter(|h| h.tipo.peso() >= peso_min).take(n).collect();
        v.reverse();
        v
    }
}

pub fn fecha(dia: u64) -> String {
    format!("Año {}, día {}", dia / DIAS_ANIO + 1, dia % DIAS_ANIO + 1)
}
