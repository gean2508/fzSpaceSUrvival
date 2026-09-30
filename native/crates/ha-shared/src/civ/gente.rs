//! Nacer, casarse, envejecer, morir, heredar.

use super::azar::hash;
use super::cronica::Tipo;
use super::mundo::{oficio_para, Mundo, JUGADOR_ID};
use super::persona::{Oficio, Persona};
use super::saber::*;
use super::DIAS_ANIO;

pub fn objetivo_notables(pob: f32) -> usize {
    ((pob / 45.0) as usize).clamp(6, 32)
}

impl Mundo {
    pub(crate) fn heredar(&mut self, d: &Persona) {
        if d.riqueza <= 0.5 {
            return;
        }
        let hijos: Vec<u32> = self
            .pueblos
            .get(&d.pueblo)
            .map(|p| p.gente.clone())
            .unwrap_or_default()
            .into_iter()
            .filter(|i| self.personas.get(i).map(|x| x.padre == Some(d.id) || x.madre == Some(d.id)).unwrap_or(false))
            .collect();
        let herederos: Vec<u32> = if !hijos.is_empty() { hijos } else { d.pareja.into_iter().collect() };
        if herederos.is_empty() {
            if let Some(p) = self.pueblos.get_mut(&d.pueblo) {
                p.tesoro += d.riqueza;
            }
            return;
        }
        let parte = d.riqueza / herederos.len() as f32;
        for h in herederos {
            if let Some(x) = self.personas.get_mut(&h) {
                x.riqueza += parte;
            }
        }
    }

    pub(crate) fn gente_dia(&mut self) {
        let dia = self.dia;
        let ids: Vec<u32> = self.personas.keys().copied().collect();
        for id in ids {
            if id == JUGADOR_ID {
                continue;
            }
            let (edad, pueblo, oficio) = {
                let p = &self.personas[&id];
                (p.edad(dia), p.pueblo, p.oficio)
            };
            let (salud, hambre, guerra) = match self.pueblos.get(&pueblo) {
                Some(pu) => (pu.capacidad().salud.min(1.2), pu.hambre, self.guerra_de(pueblo).is_some()),
                None => (0.0, 0.0, false),
            };
            let mut q = (0.0015 + 0.00003 * (0.095 * edad).exp()) * (1.8 - 0.6 * salud) + hambre * 0.06;
            if edad < 5.0 {
                q += 0.02;
            }
            if guerra && oficio == Oficio::Soldado {
                q += 0.06;
            }
            let p_dia = q.min(0.95) / DIAS_ANIO as f32;
            if self.rng.p(p_dia) {
                let causa = if edad > 60.0 {
                    "de vejez"
                } else if hambre > 0.5 {
                    "de hambre"
                } else if guerra && oficio == Oficio::Soldado {
                    "en la guerra"
                } else {
                    "de enfermedad"
                };
                self.matar(id, causa);
                continue;
            }
            let p = self.personas.get_mut(&id).unwrap();
            let sueldo = match p.oficio {
                Oficio::Mercader => 0.06,
                Oficio::Erudito => 0.02,
                Oficio::Soldado => 0.03,
                _ => 0.03,
            };
            p.riqueza += sueldo;
        }
    }

    pub(crate) fn gente_anual(&mut self) {
        let dia = self.dia;
        let pueblos: Vec<u32> = self.pueblos.keys().copied().collect();
        for pid in pueblos {
            if !self.pueblos[&pid].vivo {
                continue;
            }
            let gente = self.pueblos[&pid].gente.clone();
            // mayoría de edad: aprende lo común de su pueblo y elige oficio
            let umbral = 90.0 + 160.0 * self.pueblos[&pid].escuela as f32;
            let saberes_p = self.pueblos[&pid].saberes;
            for id in &gente {
                let (edad, h) = {
                    let p = &self.personas[id];
                    (p.edad(dia), hash(p.id as u64 ^ dia))
                };
                if (16.0..17.0).contains(&edad) {
                    let mut sab = self.personas[id].saberes;
                    for s in saberes_p.iter() {
                        if DEFS[s as usize].costo <= umbral {
                            sab = sab.con(s);
                        }
                    }
                    let p = self.personas.get_mut(id).unwrap();
                    p.saberes = sab;
                    p.oficio = oficio_para(p, h);
                }
            }
            // casamientos entre solteros del pueblo
            let mut solteros: Vec<u32> = gente
                .iter()
                .copied()
                .filter(|i| {
                    let p = &self.personas[i];
                    p.pareja.is_none() && p.edad(dia) >= 17.0 && p.edad(dia) < 55.0 && *i != JUGADOR_ID
                })
                .collect();
            let mut k = 0;
            while k < solteros.len() {
                let a = solteros[k];
                let mut mejor: Option<(usize, f32)> = None;
                for (j, b) in solteros.iter().enumerate().skip(k + 1) {
                    let (pa, pb) = (&self.personas[&a], &self.personas[b]);
                    if pa.mujer == pb.mujer || pa.parientes_cercanos(pb) {
                        continue;
                    }
                    let afin = 1.0 - (pa.edad(dia) - pb.edad(dia)).abs() / 30.0
                        + 0.2 * (pa.riqueza.min(20.0) * pb.riqueza.min(20.0)).sqrt() / 20.0
                        + 0.3 * self.rng.f();
                    if mejor.map(|m| afin > m.1).unwrap_or(afin > 0.5) {
                        mejor = Some((j, afin));
                    }
                }
                if let Some((j, _)) = mejor {
                    let b = solteros[j];
                    self.personas.get_mut(&a).unwrap().pareja = Some(b);
                    self.personas.get_mut(&b).unwrap().pareja = Some(a);
                    solteros.remove(j);
                }
                k += 1;
            }
            // hijos
            let mujeres: Vec<u32> = gente
                .iter()
                .copied()
                .filter(|i| {
                    let p = &self.personas[i];
                    p.mujer && p.pareja.is_some() && (16.0..42.0).contains(&p.edad(dia))
                })
                .collect();
            let hambre = self.pueblos[&pid].hambre;
            let cap_gente = objetivo_notables(self.pueblos[&pid].pob) * 2;
            for m in mujeres {
                if !self.personas.contains_key(&m) {
                    continue;
                }
                if !self.rng.p(0.26 * (1.0 - hambre)) {
                    continue;
                }
                let padre = self.personas[&m].pareja;
                self.personas.get_mut(&m).unwrap().hijos += 1;
                if let Some(pa) = padre.and_then(|x| self.personas.get_mut(&x)) {
                    pa.hijos += 1;
                }
                if self.pueblos[&pid].gente.len() < cap_gente {
                    let h = self.rng.u64();
                    self.crear_persona(pid, 0.0, padre, Some(m), h);
                }
            }
            // reposición: si faltan adultos notables, sale alguien del pueblo
            let adultos = self.pueblos[&pid].gente.iter().filter(|i| self.personas[i].adulta(dia)).count();
            let meta = objetivo_notables(self.pueblos[&pid].pob).min(self.pueblos[&pid].pob as usize / 8 + 1);
            if adultos < meta {
                for _ in 0..(meta - adultos).min(3) {
                    let h = self.rng.u64();
                    let edad = 18.0 + 30.0 * self.rng.f();
                    let id = self.crear_persona(pid, edad, None, None, h);
                    let mut sab = super::saber::Saberes::NADA;
                    for s in saberes_p.iter() {
                        if DEFS[s as usize].costo <= umbral {
                            sab = sab.con(s);
                        }
                    }
                    self.personas.get_mut(&id).unwrap().saberes = sab;
                }
            }
            // fama que se enfría
            for id in self.pueblos[&pid].gente.clone() {
                if let Some(p) = self.personas.get_mut(&id) {
                    p.fama *= 0.93;
                }
            }
        }
    }
}
