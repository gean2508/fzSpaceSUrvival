//! Creencias que nacen, se contagian y a veces se vuelven la ley.
//! Nadie decide qué culto existe: un sacerdote muy creyente y carismático lo funda,
//! y se extiende por contacto y por convicción.

use super::cronica::Tipo;
use super::mundo::{Mundo, JUGADOR_ID};
use super::persona::Oficio;
use super::pueblo::*;

const MAX_CULTOS: usize = 200;

const RAICES: [&str; 12] = ["Luz", "Ceniza", "Río", "Cielo", "Raíz", "Estrella", "Trueno", "Silencio", "Hierro", "Luna", "Fuego", "Viento"];
const TITULOS: [&str; 6] = ["Camino de la", "Hermandad del", "Culto de la", "Orden del", "Los Hijos del", "Pacto de la"];

impl Mundo {
    pub fn nombre_culto(&self, id: u8) -> String {
        self.cultos.iter().find(|c| c.id == id).map(|c| c.nombre.clone()).unwrap_or_else(|| "la fe de siempre".into())
    }

    pub(crate) fn cultos_anual(&mut self) {
        let dia = self.dia;
        let ids: Vec<u32> = self.pueblos.keys().copied().collect();
        for id in ids {
            if !self.pueblos[&id].vivo {
                continue;
            }
            // fundación
            let profeta = self.pueblos[&id]
                .gente
                .iter()
                .copied()
                .filter(|g| *g != JUGADOR_ID)
                .filter_map(|g| self.personas.get(&g))
                .filter(|p| p.adulta(dia) && p.fe > 0.7 && p.carisma > 0.55 && (p.oficio == Oficio::Sacerdote || p.audacia > 0.6))
                .map(|p| (p.fe + p.carisma + p.audacia, p.id))
                .fold(None::<(f32, u32)>, |a, b| if a.map(|x| b.0 > x.0).unwrap_or(true) { Some(b) } else { a });
            if let Some((_, pid)) = profeta {
                if self.rng.p(0.01) && self.cultos.len() < MAX_CULTOS {
                    self.fundar_culto(id, pid);
                }
            }
            // conversión de la gente por el culto dominante del pueblo
            let fe_pueblo = self.pueblos[&id].culto;
            let libertad = self.pueblos[&id].leyes.libertad_culto;
            let gente = self.pueblos[&id].gente.clone();
            for g in gente {
                let Some(p) = self.personas.get_mut(&g) else { continue };
                if p.culto != fe_pueblo && self.rng.p(0.08 * (1.0 - libertad * 0.8) + 0.01) {
                    p.culto = fe_pueblo;
                }
            }
            // contagio entre pueblos en contacto
            let contactos = self.contactos(id);
            for c in contactos {
                let (cc, w_fe, fe_c) = {
                    let q = &self.pueblos[&c];
                    (q.culto, q.conv.w[Convencion::FE], q.legit)
                };
                let (mio, lib, moral) = {
                    let p = &self.pueblos[&id];
                    (p.culto, p.leyes.libertad_culto, p.moral)
                };
                if cc != mio && self.rng.p(0.012 * lib * (0.5 + fe_c) * (1.0 + w_fe) * (1.2 - moral)) {
                    self.pueblos.get_mut(&id).unwrap().culto = cc;
                    let (a, b) = (self.nombre_pueblo(id), self.nombre_culto(cc));
                    self.hecho(Tipo::Culto, id, c, format!("{a} adopta {b}."));
                }
            }
        }
    }

    fn fundar_culto(&mut self, pueblo: u32, profeta: u32) {
        let h = self.rng.u64();
        let nombre = format!("{} {}", TITULOS[(h % 6) as usize], RAICES[((h >> 8) % 12) as usize]);
        if self.cultos.iter().any(|c| c.nombre == nombre) {
            return;
        }
        let id = self.cultos.iter().map(|c| c.id as u32).max().unwrap_or(0) + 1;
        if id > 250 {
            return;
        }
        let (fundador, bondad, fe) = {
            let p = &self.personas[&profeta];
            (p.nombre.clone(), p.bondad, p.fe)
        };
        let severidad = ((0.5 - bondad) * 1.6 + (fe - 0.7)).clamp(-1.0, 1.0);
        let dia = self.dia;
        self.cultos.push(Culto { id: id as u8, nombre: nombre.clone(), fundador: fundador.clone(), dia, severidad });
        if let Some(p) = self.personas.get_mut(&profeta) {
            p.culto = id as u8;
            p.fama += 2.0;
        }
        let seguidores: Vec<u32> = self.pueblos[&pueblo].gente.clone();
        let mut n = 0;
        for g in seguidores {
            let cerca = self.personas.get(&g).map(|p| p.fe * 0.5 + p.carisma * 0.0 + (1.0 - p.inteligencia) * 0.1 > 0.35).unwrap_or(false);
            if cerca && g != JUGADOR_ID && self.rng.p(0.45) {
                self.personas.get_mut(&g).unwrap().culto = id as u8;
                n += 1;
            }
        }
        let lugar = self.nombre_pueblo(pueblo);
        // un culto con mucha gente y un profeta fuerte llega a ser la fe del pueblo
        if n >= 4 && self.pueblos[&pueblo].conv.w[Convencion::FE] > 0.1 {
            self.pueblos.get_mut(&pueblo).unwrap().culto = id as u8;
        }
        self.hecho(Tipo::Culto, pueblo, profeta, format!("{fundador} funda {nombre} en {lugar}."));
    }
}
