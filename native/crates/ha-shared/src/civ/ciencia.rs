//! Investigar, enseñar, difundir. Nadie le dice a nadie qué inventar: cada
//! erudito mira lo que su cultura ya sabe, lo que el sitio permite y lo que su
//! pueblo necesita, y elige. La "historia de la ciencia" es lo que sale de sumar
//! esas elecciones.

use std::collections::HashMap;

use super::cronica::Tipo;
use super::mundo::{Mundo, JUGADOR_ID};
use super::persona::{Estudio, Oficio};
use super::saber::*;
use super::DIAS_ANIO;

/// El saber y todo lo que hay debajo (su cadena de prerrequisitos).
pub fn cierre(s: u8) -> Saberes {
    let mut r = Saberes::NADA.con(s);
    for p in DEFS[s as usize].pre {
        r = r.union(cierre(*p));
    }
    r
}

impl Mundo {
    pub fn artefacto_cubre(&self, pueblo: u32, s: u8) -> bool {
        self.artefactos.iter().any(|a| a.pueblo == pueblo && cierre(a.saber).tiene(s))
    }

    fn sustrato_accesible(&self, pueblo: u32) -> u32 {
        let mut s = self.pueblos.get(&pueblo).map(|p| p.sustrato).unwrap_or(0);
        for r in &self.relaciones {
            if r.comercio && (r.a == pueblo || r.b == pueblo) {
                let otro = if r.a == pueblo { r.b } else { r.a };
                s |= self.pueblos.get(&otro).map(|p| p.sustrato).unwrap_or(0);
            }
        }
        s
    }

    fn hay_maestro(&self, pueblo: u32, s: u8, salvo: u32) -> bool {
        self.pueblos
            .get(&pueblo)
            .map(|p| p.gente.iter().any(|g| *g != salvo && self.personas.get(g).map(|x| x.saberes.tiene(s)).unwrap_or(false)))
            .unwrap_or(false)
    }

    /// ¿Qué estudiaría esta persona hoy? `None` si nada le interesa o le alcanza.
    pub fn elegir_estudio(&mut self, id: u32) -> Option<Estudio> {
        let p = self.personas.get(&id)?.clone();
        let pu = self.pueblos.get(&p.pueblo)?;
        let cultura = p.saberes.union(pu.saberes);
        let acc = self.sustrato_accesible(p.pueblo);
        let guerra = self.guerra_de(p.pueblo).is_some();
        let (hambre, orden) = (pu.hambre, pu.orden);
        let amb = pu.gobernante.and_then(|g| self.personas.get(&g)).map(|g| g.ambicion).unwrap_or(0.5);
        let mut mejor: Option<(f32, u8, f32)> = None;
        for s in 0..N_SABERES as u8 {
            if p.saberes.tiene(s) {
                continue;
            }
            let def = &DEFS[s as usize];
            if def.frontera && !(p.audacia >= 0.5 && p.curiosidad >= 0.5) {
                continue;
            }
            let local = pu.saberes.tiene(s);
            let art = self.artefacto_cubre(p.pueblo, s);
            let pre_propios = def.pre.iter().all(|x| p.saberes.tiene(*x));
            let pre_cultura = def.pre.iter().all(|x| cultura.tiene(*x));
            let permitido = if local { pre_propios || art } else { pre_cultura || art };
            if !permitido {
                continue;
            }
            if !local && !art && def.sustrato != 0 && acc & def.sustrato == 0 {
                continue;
            }
            let mut costo = def.costo;
            if local {
                costo *= if self.hay_maestro(p.pueblo, s, id) { 0.4 } else { 0.65 };
            }
            if art {
                let faltan = faltan(s, cultura) as f32;
                costo = costo.min(def.costo * 0.35 * (1.0 + 0.5 * faltan));
            }
            let e = def.efecto;
            let necesidad = 0.15
                + e.comida * (1.0 + 3.0 * hambre)
                + e.mil * (0.3 + if guerra { 2.5 } else { 0.0 } + amb)
                + e.salud * 0.8
                + e.orden * (1.0 + 2.0 * (1.0 - orden))
                + e.mat * 0.6
                + e.com * 0.5
                + e.inv * (0.4 + p.curiosidad)
                + 0.6 * p.curiosidad
                + if def.frontera { 0.9 * p.audacia } else { 0.0 };
            let noise = 0.6 + 0.8 * self.rng.f();
            let score = necesidad / costo.sqrt() * noise;
            if mejor.map(|m| score > m.0).unwrap_or(true) {
                mejor = Some((score, s, costo));
            }
        }
        mejor.map(|(_, saber, costo)| Estudio { saber, prog: 0.0, costo })
    }

    pub(crate) fn ciencia_dia(&mut self) {
        let dia = self.dia;
        let mut colab: HashMap<(u32, u8), u32> = HashMap::new();
        for p in self.personas.values() {
            if let Some(e) = p.estudio {
                *colab.entry((p.pueblo, e.saber)).or_insert(0) += 1;
            }
        }
        let ids: Vec<u32> = self.personas.keys().copied().collect();
        for id in ids {
            if id == JUGADOR_ID {
                continue;
            }
            let (adulta, libre, oficio, cur, estudio, pueblo) = {
                let p = &self.personas[&id];
                (p.adulta(dia), p.libre(dia), p.oficio, p.curiosidad, p.estudio, p.pueblo)
            };
            if !adulta || !libre {
                continue;
            }
            let Some(est) = estudio else {
                let prob = if oficio == Oficio::Erudito { 0.25 } else if cur > 0.65 { 0.02 } else { 0.0 };
                if prob > 0.0 && self.rng.p(prob) {
                    let e = self.elegir_estudio(id);
                    self.personas.get_mut(&id).unwrap().estudio = e;
                }
                continue;
            };
            let Some(pu) = self.pueblos.get(&pueblo) else { continue };
            let guerra = self.guerra_de(pueblo).is_some();
            let otros = colab.get(&(pueblo, est.saber)).copied().unwrap_or(1).saturating_sub(1).min(5) as f32;
            let p = &self.personas[&id];
            let rate = 0.55 * (0.4 + 1.2 * p.inteligencia)
                * (1.0 + 0.35 * pu.biblioteca as f32)
                * if p.oficio == Oficio::Erudito { 1.0 } else { 0.3 }
                * (1.0 + 0.4 * otros)
                * (1.0 - 0.5 * pu.hambre)
                * if guerra { 0.7 } else { 1.0 };
            let nuevo = est.prog + rate;
            if nuevo < est.costo {
                self.personas.get_mut(&id).unwrap().estudio = Some(Estudio { prog: nuevo, ..est });
                continue;
            }
            self.terminar_estudio(id, est.saber);
        }
    }

    fn terminar_estudio(&mut self, id: u32, s: u8) {
        let (nombre, pueblo, costo) = {
            let p = self.personas.get_mut(&id).unwrap();
            p.estudio = None;
            p.saberes = p.saberes.con(s);
            p.fama += DEFS[s as usize].costo / 300.0;
            (p.nombre.clone(), p.pueblo, DEFS[s as usize].costo)
        };
        // Lo que se sabe se sabe entero: un erudito con un saber ya tiene el camino.
        let mut cambio = true;
        while cambio {
            cambio = false;
            let sab = self.personas[&id].saberes;
            for x in sab.iter() {
                for pre in DEFS[x as usize].pre {
                    if !sab.tiene(*pre) {
                        self.personas.get_mut(&id).unwrap().saberes = sab.con(*pre);
                        cambio = true;
                    }
                }
            }
        }
        let lugar = self.nombre_pueblo(pueblo);
        let nombre_saber = DEFS[s as usize].nombre;
        let nuevo_en_pueblo = !self.pueblos.get(&pueblo).map(|p| p.saberes.tiene(s)).unwrap_or(true);
        if !self.saberes_mundo.tiene(s) {
            self.saberes_mundo = self.saberes_mundo.con(s);
            self.hecho(
                Tipo::Hito,
                pueblo,
                0,
                format!("{nombre}, de {lugar}, alcanza {nombre_saber}: el mundo no lo había conocido nunca."),
            );
        } else if nuevo_en_pueblo && costo >= 200.0 {
            self.hecho(Tipo::Descubrimiento, pueblo, 0, format!("{lugar} domina {nombre_saber} gracias a {nombre}."));
        }
    }

    pub(crate) fn ciencia_anual(&mut self) {
        // Difusión por comercio: lo que uno sabe le llega al socio como una idea a medias.
        let rels: Vec<(u32, u32, f32)> =
            self.relaciones.iter().filter(|r| r.comercio).map(|r| (r.a, r.b, r.actitud)).collect();
        for (a, b, _) in rels {
            for (de, a_) in [(a, b), (b, a)] {
                let (Some(pd), Some(pa)) = (self.pueblos.get(&de), self.pueblos.get(&a_)) else { continue };
                if !pd.vivo || !pa.vivo {
                    continue;
                }
                let com = pd.capacidad().com.min(2.0) + pa.capacidad().com.min(2.0);
                if !self.rng.p(0.18 + 0.12 * com) {
                    continue;
                }
                let acc = self.sustrato_accesible(a_);
                let mut cand: Vec<u8> = Vec::new();
                for s in 0..N_SABERES as u8 {
                    let def = &DEFS[s as usize];
                    if pd.saberes.tiene(s)
                        && !pa.saberes.tiene(s)
                        && def.pre.iter().all(|x| pa.saberes.tiene(*x))
                        && (def.sustrato == 0 || acc & def.sustrato != 0)
                    {
                        cand.push(s);
                    }
                }
                if cand.is_empty() {
                    continue;
                }
                let s = cand[self.rng.idx(cand.len())];
                let quien = pa
                    .gente
                    .iter()
                    .copied()
                    .filter(|g| self.personas.get(g).map(|x| x.adulta(self.dia) && x.estudio.is_none()).unwrap_or(false))
                    .max_by(|x, y| {
                        let (px, py) = (&self.personas[x], &self.personas[y]);
                        (px.inteligencia + px.curiosidad).partial_cmp(&(py.inteligencia + py.curiosidad)).unwrap()
                    });
                if let Some(q) = quien {
                    let costo = DEFS[s as usize].costo * 0.5;
                    self.personas.get_mut(&q).unwrap().estudio = Some(Estudio { saber: s, prog: costo * 0.4, costo });
                }
            }
        }
        // Las bibliotecas guardan lo que se sabe.
        for p in self.pueblos.values_mut() {
            if p.vivo && p.biblioteca > 0 {
                p.archivo = p.archivo.union(p.saberes);
            }
        }
        let _ = DIAS_ANIO;
    }
}
