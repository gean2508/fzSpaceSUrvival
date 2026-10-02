//! Diplomacia, comercio, guerra, paz, vasallaje y migración.
//!
//! Una guerra no está guionada: un gobernante con ambición, codicia o hambre mira
//! a su vecino y hace la cuenta (fuerza relativa, botín, cariño, culto, fatiga).
//! Cuánto llega su brazo lo dice lo que sabe (a pie, por mar, por aire).

use super::cronica::Tipo;
use super::mundo::Mundo;
use super::pueblo::*;
use super::saber::*;
use super::DIAS_ANIO;

fn sigmoide(x: f32) -> f32 {
    1.0 / (1.0 + (-x).exp())
}

impl Mundo {
    pub fn guerra_de(&self, pueblo: u32) -> Option<usize> {
        self.guerras.iter().position(|g| g.bando_a.contains(&pueblo) || g.bando_b.contains(&pueblo))
    }

    fn en_guerra_entre(&self, a: u32, b: u32) -> bool {
        self.guerras.iter().any(|g| {
            (g.bando_a.contains(&a) && g.bando_b.contains(&b)) || (g.bando_a.contains(&b) && g.bando_b.contains(&a))
        })
    }

    pub(crate) fn diplomacia_anual(&mut self) {
        self.armar_vecinos();
        let ids: Vec<u32> = self.pueblos.keys().copied().filter(|i| self.pueblos[i].vivo).collect();
        // 1. actitudes, comercio, alianzas
        for a in &ids {
            for b in self.contactos(*a) {
                if b < *a && self.contactos(b).contains(a) {
                    continue; // el par ya se evaluó desde el otro lado
                }
                let (ca, cb) = (self.pueblos[a].culto, self.pueblos[&b].culto);
                let lib = self.pueblos[a].leyes.libertad_culto.min(self.pueblos[&b].leyes.libertad_culto);
                let guerra = self.en_guerra_entre(*a, b);
                let ruido = self.rng.campana() * 0.06;
                let r = self.relacion_mut(*a, b);
                let mut d = ruido - r.actitud * 0.05;
                if ca != cb {
                    d -= 0.05 * (1.0 - lib);
                } else {
                    d += 0.02;
                }
                if r.comercio {
                    d += 0.03;
                }
                if r.alianza {
                    d += 0.03;
                }
                if guerra {
                    d -= 0.12;
                }
                r.actitud = (r.actitud + d).clamp(-1.0, 1.0);
                r.comercio = !guerra && r.actitud > 0.02;
                if r.alianza && r.actitud < 0.15 {
                    r.alianza = false;
                }
                let quiere_alianza = !r.alianza && r.actitud > 0.5 && !guerra;
                if quiere_alianza && self.rng.p(0.2) {
                    self.relacion_mut(*a, b).alianza = true;
                    let (na, nb) = (self.nombre_pueblo(*a), self.nombre_pueblo(b));
                    self.hecho(Tipo::Alianza, *a, b, format!("{na} y {nb} sellan una alianza."));
                }
            }
        }
        // 2. declaraciones de guerra
        for a in &ids {
            self.evaluar_guerra(*a);
        }
        // 3. fatiga que baja, tributo, migración
        for id in &ids {
            if let Some(p) = self.pueblos.get_mut(id) {
                p.bajas *= 0.75;
            }
            self.tributo_o_rebelion(*id);
            self.migrar(*id);
        }
    }

    fn evaluar_guerra(&mut self, a: u32) {
        let (senor, en_guerra) = (self.pueblos[&a].senor, self.guerra_de(a).is_some());
        if senor.is_some() || en_guerra {
            return;
        }
        let Some(g) = self.pueblos[&a].gobernante.and_then(|g| self.personas.get(&g)).cloned() else { return };
        for b in self.contactos(a) {
            if self.guerra_de(b).is_some() || self.aliados(a, b) || self.pueblos[&b].senor == Some(a) || self.pueblos[&a].senor == Some(b) {
                continue;
            }
            let (pa, pb) = (&self.pueblos[&a], &self.pueblos[&b]);
            let (fa, fb) = (self.fuerza_militar(a, false), self.fuerza_militar(b, true));
            if fa < 5.0 {
                continue;
            }
            let botin = ((pb.tesoro + pb.pob) / (pa.tesoro.max(0.0) + pa.pob + 1.0)).clamp(0.0, 2.0);
            let culto = if pa.culto != pb.culto { 1.0 } else { 0.0 };
            let fatiga = pa.bajas * 8.0;
            let s = 0.8 * g.ambicion + 0.5 * g.codicia * botin + 0.5 * (fa / fb.max(1.0)).ln().clamp(-1.5, 1.5)
                - 0.9 * g.bondad
                - 1.2 * self.actitud(a, b)
                + 0.4 * g.fe * culto * (1.0 - pb.leyes.libertad_culto * 0.0)
                + 0.8 * pa.hambre
                - fatiga
                + self.rng.campana() * 0.3;
            let mut prob = sigmoide((s - 1.0) * 3.0) * 0.3;
            if pa.arsenal > 0 && pb.arsenal > 0 {
                prob *= 0.04; // la destrucción mutua asegurada disuade: es física, no diplomacia
            } else if pb.arsenal > 0 {
                prob *= 0.25; // nadie ataca a la ligera a quien tiene la bomba
            } else if pa.arsenal > 0 {
                prob *= 1.6; // quien la tiene y el otro no, se siente impune
            } else if pa.saberes.tiene(NUCLEAR) && pb.saberes.tiene(NUCLEAR) {
                prob *= 0.15;
            }
            if !self.rng.p(prob) {
                continue;
            }
            let causa = if pa.hambre > 0.4 {
                "el hambre"
            } else if culto > 0.0 && g.fe > 0.6 {
                "el culto"
            } else if botin > 1.2 && g.codicia > 0.5 {
                "el botín"
            } else {
                "la ambición de su jefe"
            };
            self.declarar_guerra(a, b, causa);
            return;
        }
    }

    pub fn declarar_guerra(&mut self, a: u32, b: u32, causa: &str) {
        let mut ba = vec![a];
        let mut bb = vec![b];
        let alia = |m: &Mundo, p: u32| -> Vec<u32> {
            m.relaciones
                .iter()
                .filter(|r| r.alianza && (r.a == p || r.b == p))
                .map(|r| if r.a == p { r.b } else { r.a })
                .collect()
        };
        for c in alia(self, a) {
            if self.guerra_de(c).is_none() && c != b && !bb.contains(&c) && self.rng.p(0.5) {
                ba.push(c);
            }
        }
        for c in alia(self, b) {
            if self.guerra_de(c).is_none() && c != a && !ba.contains(&c) && self.rng.p(0.6) {
                bb.push(c);
            }
        }
        let id = self.sig_guerra;
        self.sig_guerra += 1;
        let (na, nb) = (self.nombre_pueblo(a), self.nombre_pueblo(b));
        self.guerras.push(Guerra { id, bando_a: ba, bando_b: bb, inicio: self.dia, avance: 0.0, causa: causa.to_string() });
        let r = self.relacion_mut(a, b);
        r.actitud = (r.actitud - 0.4).max(-1.0);
        r.comercio = false;
        r.alianza = false;
        self.hecho(Tipo::Guerra, a, b, format!("{na} declara la guerra a {nb} por {causa}."));
    }

    pub(crate) fn guerras_5d(&mut self) {
        let mut a_cerrar: Vec<(usize, i32)> = Vec::new();
        for idx in 0..self.guerras.len() {
            let g = self.guerras[idx].clone();
            let fa: f32 = g.bando_a.iter().enumerate().map(|(i, p)| self.fuerza_militar(*p, false) * if i == 0 { 1.0 } else { 0.6 }).sum();
            let fb: f32 = g.bando_b.iter().enumerate().map(|(i, p)| self.fuerza_militar(*p, true) * if i == 0 { 1.0 } else { 0.6 }).sum();
            let ratio = fa / (fa + fb + 0.001);
            let dur = self.dia - g.inicio;
            {
                let gm = &mut self.guerras[idx];
                gm.avance = (gm.avance + (ratio - 0.5) * 0.14).clamp(-1.2, 1.2);
            }
            // bajas: cada bando sangra según la fuerza del otro
            for (bando, dano) in [(&g.bando_a, 0.011 * (1.0 - ratio)), (&g.bando_b, 0.011 * ratio)] {
                for p in bando {
                    let mut muertos_notables = Vec::new();
                    if let Some(pu) = self.pueblos.get_mut(p) {
                        pu.pob = (pu.pob * (1.0 - dano)).max(8.0);
                        pu.bajas += dano;
                        pu.moral = (pu.moral - 0.02 - dano * 2.0).max(0.0);
                        for id in &pu.gente {
                            muertos_notables.push(*id);
                        }
                    }
                    for id in muertos_notables {
                        let sol = self.personas.get(&id).map(|x| x.oficio == super::persona::Oficio::Soldado).unwrap_or(false);
                        if sol && self.rng.p(dano * 2.0) {
                            self.matar(id, "en batalla");
                        }
                    }
                }
            }
            if dur % 20 == 0 && self.rng.p(0.4) {
                let (na, nb) = (self.nombre_pueblo(g.bando_a[0]), self.nombre_pueblo(g.bando_b[0]));
                let quien = if ratio > 0.5 { &na } else { &nb };
                self.hecho(Tipo::Batalla, g.bando_a[0], g.bando_b[0], format!("Batalla entre {na} y {nb}: {quien} se lleva la mejor parte."));
            }
            self.uso_nuclear(idx);
            // saqueo cuando alguien va ganando de sobra
            let av = self.guerras[idx].avance;
            if av.abs() > 0.55 && self.rng.p(0.08) {
                let (ganador, perdedor) = if av > 0.0 { (g.bando_a[0], g.bando_b[0]) } else { (g.bando_b[0], g.bando_a[0]) };
                self.saquear(ganador, perdedor);
            }
            let fin = if av >= 1.0 {
                Some(1)
            } else if av <= -1.0 {
                Some(-1)
            } else if dur > 90 || (dur > 30 && self.rng.p(0.03 + dur as f32 / 4000.0)) {
                Some(0)
            } else {
                None
            };
            if let Some(f) = fin {
                a_cerrar.push((idx, f));
            }
        }
        for (idx, f) in a_cerrar.into_iter().rev() {
            self.cerrar_guerra(idx, f);
        }
    }

    fn saquear(&mut self, ganador: u32, perdedor: u32) {
        let botin = self.pueblos[&perdedor].tesoro.max(0.0) * 0.3;
        let quema = self.rng.p(0.35);
        {
            let p = self.pueblos.get_mut(&perdedor).unwrap();
            p.tesoro -= botin;
            p.legit = (p.legit - 0.12).max(0.0);
            p.comida *= 0.6;
            if quema {
                p.archivo = Saberes::NADA;
                p.biblioteca = 0;
                for e in p.creencias.err.iter_mut() {
                    *e = e.signum() * (e.abs() + (0.6 - e.abs()) * 0.25);
                }
            }
        }
        self.pueblos.get_mut(&ganador).unwrap().tesoro += botin;
        for i in 0..self.artefactos.len() {
            if self.artefactos[i].pueblo == perdedor && self.rng.p(0.5) {
                self.artefactos[i].pueblo = ganador;
            }
        }
        let (ng, np) = (self.nombre_pueblo(ganador), self.nombre_pueblo(perdedor));
        let extra = if quema { " La biblioteca arde." } else { "" };
        self.hecho(Tipo::Tragedia, perdedor, ganador, format!("{ng} saquea {np}.{extra}"));
    }

    /// `resultado`: 1 gana A, -1 gana B, 0 tablas.
    pub(crate) fn cerrar_guerra(&mut self, idx: usize, resultado: i32) {
        let g = self.guerras.remove(idx);
        let (na, nb) = (self.nombre_pueblo(g.bando_a[0]), self.nombre_pueblo(g.bando_b[0]));
        for a in &g.bando_a {
            for b in &g.bando_b {
                let r = self.relacion_mut(*a, *b);
                r.actitud = (r.actitud + 0.1).min(1.0);
            }
        }
        let anios = (self.dia - g.inicio) as f32 / DIAS_ANIO as f32;
        if resultado == 0 {
            self.hecho(Tipo::Paz, g.bando_a[0], g.bando_b[0], format!("{na} y {nb} firman la paz tras {:.1} años sin vencedor.", anios));
            return;
        }
        let (gan, per) = if resultado > 0 { (g.bando_a[0], g.bando_b[0]) } else { (g.bando_b[0], g.bando_a[0]) };
        let (ng, np) = (self.nombre_pueblo(gan), self.nombre_pueblo(per));
        let amb = self.pueblos[&gan].gobernante.and_then(|x| self.personas.get(&x)).map(|x| x.ambicion).unwrap_or(0.5);
        let debil = self.pueblos[&per].pob < 0.45 * self.pueblos[&gan].pob;
        if debil && amb > 0.5 && self.pueblos[&per].senor.is_none() {
            self.pueblos.get_mut(&per).unwrap().senor = Some(gan);
            self.pueblos.get_mut(&per).unwrap().legit *= 0.6;
            self.hecho(Tipo::Anexion, per, gan, format!("{ng} vence a {np} tras {:.1} años y la somete como vasalla.", anios));
        } else {
            let tributo = self.pueblos[&per].tesoro.max(0.0) * 0.4;
            self.pueblos.get_mut(&per).unwrap().tesoro -= tributo;
            self.pueblos.get_mut(&gan).unwrap().tesoro += tributo;
            self.pueblos.get_mut(&gan).unwrap().legit = (self.pueblos[&gan].legit + 0.12).min(1.0);
            self.hecho(Tipo::Paz, gan, per, format!("{ng} vence a {np} y le impone la paz y un tributo."));
        }
        // la victoria alimenta a quien la ganó
        if let Some(gb) = self.pueblos[&gan].gobernante {
            if let Some(p) = self.personas.get_mut(&gb) {
                p.fama += 2.0;
            }
        }
    }

    /// Un vasallo paga; si está harto y es fuerte, se levanta.
    pub(crate) fn tributo_o_rebelion(&mut self, id: u32) {
        let Some(senor) = self.pueblos[&id].senor else { return };
        if !self.pueblos.get(&senor).map(|p| p.vivo).unwrap_or(false) {
            self.pueblos.get_mut(&id).unwrap().senor = None;
            return;
        }
        let (legit, pob, fundado, cuerpo) = {
            let p = &self.pueblos[&id];
            (p.legit, p.pob, p.fundado, p.cuerpo)
        };
        let colonia = cuerpo != self.pueblos[&senor].cuerpo;
        let anios = (self.dia - fundado) as f32 / DIAS_ANIO as f32;
        let mut p_indep = 0.02 + (1.0 - legit) * 0.10;
        if colonia && anios >= 4.0 && pob >= 20.0 {
            p_indep += 0.06;
        }
        if self.pueblos[&senor].legit < 0.35 {
            p_indep += 0.08;
        }
        if self.rng.p(p_indep) {
            self.pueblos.get_mut(&id).unwrap().senor = None;
            let (n, s) = (self.nombre_pueblo(id), self.nombre_pueblo(senor));
            self.hecho(Tipo::Independencia, id, senor, format!("{n} se independiza de {s}."));
            let r = self.relacion_mut(id, senor);
            r.actitud -= 0.3;
        } else {
            let t = self.pueblos[&id].tesoro.max(0.0) * 0.08;
            self.pueblos.get_mut(&id).unwrap().tesoro -= t;
            self.pueblos.get_mut(&senor).unwrap().tesoro += t;
        }
    }

    /// Quien pasa hambre o vive apretado se muda al vecino que mejor está.
    fn migrar(&mut self, id: u32) {
        let (hambre, pob, k) = {
            let p = &self.pueblos[&id];
            (p.hambre, p.pob, super::economia::carga_efectiva(p))
        };
        if !(hambre > 0.25 || pob > k * 1.15) {
            return;
        }
        let mut mejor: Option<(f32, u32)> = None;
        for b in self.contactos(id) {
            let q = &self.pueblos[&b];
            let holgura = q.comida / (q.pob + 1.0) - q.hambre * 5.0 + (super::economia::carga_efectiva(q) - q.pob) / (q.pob + 1.0);
            if self.guerra_de(b).is_none() && mejor.map(|m| holgura > m.0).unwrap_or(true) {
                mejor = Some((holgura, b));
            }
        }
        let Some((h, dest)) = mejor else { return };
        if h < 0.5 {
            return;
        }
        let mov = pob * 0.03;
        self.pueblos.get_mut(&id).unwrap().pob -= mov;
        self.pueblos.get_mut(&dest).unwrap().pob += mov;
        if self.rng.p(0.15) {
            let gente = self.pueblos[&id].gente.clone();
            if !gente.is_empty() {
                let q = gente[self.rng.idx(gente.len())];
                if self.personas.get(&q).map(|p| p.pareja.is_none() && p.adulta(self.dia)).unwrap_or(false) {
                    self.mudar(q, dest);
                }
            }
        }
    }
}
