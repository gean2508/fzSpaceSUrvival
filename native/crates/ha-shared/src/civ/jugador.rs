//! El jugador es una persona más del mundo: no ve la simulación desde afuera, la
//! habita. Lo que hace (donar, gobernar, enseñar, matar) entra por las mismas
//! puertas que las acciones de cualquiera y el mundo responde con sus reglas.

use serde::{Deserialize, Serialize};

use super::cronica::Tipo;
use super::mundo::{Mundo, JUGADOR_ID};
use super::persona::{Oficio, Persona};
use super::pueblo::*;
use super::saber::*;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Decreto {
    Impuesto(f32),
    PenaRobo(u8),
    PenaHomicidio(u8),
    LibertadCulto(f32),
    Servicio(f32),
    Guerra(u32),
    Paz(u32),
}

/// Alguien que murió y que el juego tal vez tiene encarnado como NPC.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Baja {
    pub persona: u32,
    pub npc: u32,
    pub causa: String,
}

impl Mundo {
    /// Da de alta al jugador en un pueblo (una sola vez). Si ya existe, lo muda.
    pub fn entrar_jugador(&mut self, pueblo: u32, nombre: &str) {
        if !self.pueblos.contains_key(&pueblo) {
            return;
        }
        if self.personas.contains_key(&JUGADOR_ID) {
            self.trasladar_jugador(pueblo);
            return;
        }
        let dia = self.dia;
        let p = Persona {
            id: JUGADOR_ID,
            nombre: nombre.to_string(),
            casa: 0xFFFF_FF01,
            pueblo,
            nacio: dia as i64 - 25 * super::DIAS_ANIO as i64,
            mujer: false,
            inteligencia: 0.6,
            curiosidad: 0.6,
            ambicion: 0.5,
            carisma: 0.6,
            fuerza: 0.6,
            bondad: 0.5,
            fe: 0.4,
            codicia: 0.3,
            audacia: 0.7,
            oficio: Oficio::Campesino,
            culto: self.pueblos[&pueblo].culto,
            ..Default::default()
        };
        self.personas.insert(JUGADOR_ID, p);
        self.pueblos.get_mut(&pueblo).unwrap().gente.push(JUGADOR_ID);
        self.jugador.pueblo = Some(pueblo);
    }

    pub fn trasladar_jugador(&mut self, pueblo: u32) {
        let Some(p) = self.personas.get(&JUGADOR_ID) else { return };
        if p.pueblo != pueblo && self.pueblos.contains_key(&pueblo) {
            self.mudar(JUGADOR_ID, pueblo);
        }
        self.jugador.pueblo = Some(pueblo);
    }

    pub fn prestigio(&self, pueblo: u32) -> f32 {
        self.jugador.prestigio.get(&pueblo).copied().unwrap_or(0.0)
    }

    fn sumar_prestigio(&mut self, pueblo: u32, d: f32) {
        let e = self.jugador.prestigio.entry(pueblo).or_insert(0.0);
        *e = (*e + d).clamp(-1.0, 1.0);
    }

    fn es_gobernante_jugador(&self, pueblo: u32) -> bool {
        self.pueblos.get(&pueblo).map(|p| p.gobernante == Some(JUGADOR_ID)).unwrap_or(false)
    }

    /// Da parte de su riqueza al pueblo: comida, tesoro y cariño.
    pub fn donar(&mut self, pueblo: u32, comida: f32, monedas: f32) {
        let Some(p) = self.pueblos.get_mut(&pueblo) else { return };
        p.comida += comida;
        p.tesoro += monedas;
        p.moral = (p.moral + (comida + monedas) / (p.pob * 20.0 + 100.0)).min(1.0);
        let d = ((comida + monedas) / (p.pob * 4.0 + 50.0)).min(0.25);
        self.sumar_prestigio(pueblo, d);
        if let Some(j) = self.personas.get_mut(&JUGADOR_ID) {
            j.fama += d * 4.0;
        }
        if d > 0.1 {
            let (l, n) = (self.nombre_pueblo(pueblo), self.nombre_persona(JUGADOR_ID));
            self.hecho(Tipo::Jugador, pueblo, JUGADOR_ID, format!("{n} socorre a {l} con una gran donación."));
        }
    }

    pub fn presentarse(&mut self, pueblo: u32) {
        self.jugador.candidato = Some(pueblo);
        self.trasladar_jugador(pueblo);
    }

    pub fn retirarse(&mut self) {
        self.jugador.candidato = None;
    }

    /// Una ley o una guerra, sólo si el jugador es quien manda.
    pub fn decretar(&mut self, pueblo: u32, d: Decreto) -> Result<(), &'static str> {
        if !self.es_gobernante_jugador(pueblo) {
            return Err("No gobernás este pueblo.");
        }
        let lugar = self.nombre_pueblo(pueblo);
        match d {
            Decreto::Impuesto(v) => self.pueblos.get_mut(&pueblo).unwrap().leyes.impuesto = v.clamp(0.0, 0.6),
            Decreto::PenaRobo(v) => self.pueblos.get_mut(&pueblo).unwrap().leyes.pena_robo = v.min(3),
            Decreto::PenaHomicidio(v) => self.pueblos.get_mut(&pueblo).unwrap().leyes.pena_homicidio = v.min(3),
            Decreto::LibertadCulto(v) => self.pueblos.get_mut(&pueblo).unwrap().leyes.libertad_culto = v.clamp(0.0, 1.0),
            Decreto::Servicio(v) => self.pueblos.get_mut(&pueblo).unwrap().leyes.servicio = v.clamp(0.0, 1.0),
            Decreto::Guerra(otro) => {
                if !self.pueblos.get(&otro).map(|q| q.vivo).unwrap_or(false) || otro == pueblo {
                    return Err("Ese pueblo no existe.");
                }
                self.declarar_guerra(pueblo, otro, "el decreto del jugador");
                return Ok(());
            }
            Decreto::Paz(otro) => {
                let Some(g) = self.guerras.iter().position(|x| {
                    (x.bando_a.contains(&pueblo) && x.bando_b.contains(&otro)) || (x.bando_b.contains(&pueblo) && x.bando_a.contains(&otro))
                }) else {
                    return Err("No hay guerra con ese pueblo.");
                };
                self.cerrar_guerra(g, 0);
                return Ok(());
            }
        }
        self.hecho(Tipo::Ley, pueblo, JUGADOR_ID, format!("Nuevo decreto en {lugar}."));
        self.sumar_prestigio(pueblo, 0.0);
        Ok(())
    }

    /// Le enseña un saber al pueblo (lo pone en manos del mejor erudito y en el archivo).
    /// El jugador no crea conocimiento de la nada: pide los prerrequisitos.
    pub fn ensenar(&mut self, pueblo: u32, saber: u8) -> Result<(), &'static str> {
        let Some(pu) = self.pueblos.get(&pueblo) else { return Err("Ese pueblo no existe.") };
        if (saber as usize) >= N_SABERES {
            return Err("Ese saber no existe.");
        }
        if !prerrequisitos_ok(saber, pu.saberes) {
            return Err("Todavía les falta la base para entenderlo.");
        }
        let mejor = pu
            .gente
            .iter()
            .copied()
            .filter(|g| *g != JUGADOR_ID)
            .filter_map(|g| self.personas.get(&g))
            .filter(|p| p.adulta(self.dia))
            .max_by(|a, b| a.inteligencia.partial_cmp(&b.inteligencia).unwrap().then(b.id.cmp(&a.id)))
            .map(|p| p.id);
        let pu = self.pueblos.get_mut(&pueblo).unwrap();
        pu.archivo = pu.archivo.con(saber);
        pu.saberes = pu.saberes.con(saber);
        let base = pu.saberes;
        if let Some(m) = mejor {
            let p = self.personas.get_mut(&m).unwrap();
            p.saberes = p.saberes.union(base);
        }
        self.sumar_prestigio(pueblo, 0.12);
        let (l, n) = (self.nombre_pueblo(pueblo), self.nombre_persona(JUGADOR_ID));
        self.hecho(Tipo::Jugador, pueblo, JUGADOR_ID, format!("{n} enseña {} en {l}.", DEFS[saber as usize].nombre));
        Ok(())
    }

    /// Entrega una pieza de tecnología ajena a un pueblo.
    pub fn entregar_artefacto(&mut self, pueblo: u32, saber: u8) -> Option<u32> {
        if !self.pueblos.contains_key(&pueblo) {
            return None;
        }
        self.sumar_prestigio(pueblo, 0.1);
        Some(self.dar_artefacto(pueblo, saber))
    }

    /// Empuja al pueblo hacia una fuente de poder (propaganda, sedición, sermón).
    pub fn incitar(&mut self, pueblo: u32, fuente: usize) {
        let Some(p) = self.pueblos.get_mut(&pueblo) else { return };
        if fuente >= N_FUENTES {
            return;
        }
        p.conv.empujar(fuente, 0.04);
        p.legit = (p.legit - 0.06).max(0.0);
        self.sumar_prestigio(pueblo, -0.02);
    }

    /// Frena los ánimos entre dos pueblos.
    pub fn mediar(&mut self, a: u32, b: u32) {
        if !self.pueblos.contains_key(&a) || !self.pueblos.contains_key(&b) {
            return;
        }
        self.relacion_mut(a, b).actitud += 0.25;
        self.sumar_prestigio(a, 0.05);
        self.sumar_prestigio(b, 0.05);
        let g = self.guerras.iter().position(|x| {
            (x.bando_a.contains(&a) && x.bando_b.contains(&b)) || (x.bando_b.contains(&a) && x.bando_a.contains(&b))
        });
        if let Some(g) = g {
            if self.rng.p(0.5) {
                self.cerrar_guerra(g, 0);
            }
        }
    }

    pub fn persona_de_npc(&self, npc: u32) -> Option<u32> {
        if npc == 0 {
            return None;
        }
        self.personas.values().find(|p| p.npc == npc).map(|p| p.id)
    }

    /// El juego avisa que un NPC encarnado murió (o lo mató el jugador).
    pub fn notificar_muerte(&mut self, persona: u32, por_jugador: bool) {
        let Some(v) = self.personas.get(&persona) else { return };
        let (pueblo, nombre, fama, gobernaba) = (v.pueblo, v.nombre.clone(), v.fama, self.pueblos.get(&v.pueblo).map(|p| p.gobernante == Some(persona)).unwrap_or(false));
        if por_jugador {
            let culpa = 0.15 + 0.15 * (fama / 10.0).min(1.0) + if gobernaba { 0.3 } else { 0.0 };
            self.sumar_prestigio(pueblo, -culpa);
            let l = self.nombre_pueblo(pueblo);
            let n = self.nombre_persona(JUGADOR_ID);
            if fama > 2.0 || gobernaba {
                self.hecho(Tipo::Crimen, pueblo, JUGADOR_ID, format!("{n} asesina a {nombre} en {l}."));
            }
            if let Some(pu) = self.pueblos.get_mut(&pueblo) {
                pu.moral = (pu.moral - 0.03).max(0.0);
                pu.crimen = (pu.crimen + 0.05).min(1.0);
            }
        }
        self.matar(persona, if por_jugador { "a manos del jugador" } else { "un accidente" });
    }

    /// El juego consume las bajas de personas que tenían cuerpo en el mundo.
    pub fn drenar_bajas(&mut self) -> Vec<Baja> {
        std::mem::take(&mut self.bajas)
    }
}
