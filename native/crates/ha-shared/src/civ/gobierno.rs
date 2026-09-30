//! Quién manda, por qué, y cómo deja de mandar. Leyes y justicia.
//!
//! No hay "tipos de gobierno" como reglas: hay una CONVENCIÓN (pesos de cinco
//! fuentes de poder) y personas con rasgos. Manda quien más pesa. El nombre del
//! régimen se deduce. Y la convención se mueve con lo que pasa: un golpe que la
//! gente acepta la corre hacia la fuerza; una elección limpia, hacia el voto.

use super::cronica::Tipo;
use super::mundo::{Mundo, JUGADOR_ID};
use super::persona::Oficio;
use super::pueblo::*;
use super::saber::*;
use super::DIAS_ANIO;

const ELECCION_CADA: u64 = 4 * DIAS_ANIO;

impl Mundo {
    /// Las cinco componentes de poder de una persona en su pueblo (0..~1 cada una).
    pub fn poder_componentes(&self, pueblo: u32, id: u32) -> [f32; N_FUENTES] {
        let (Some(pu), Some(p)) = (self.pueblos.get(&pueblo), self.personas.get(&id)) else { return [0.0; N_FUENTES] };
        let max_riq = pu.gente.iter().filter_map(|g| self.personas.get(g)).map(|x| x.riqueza).fold(1.0f32, f32::max);
        let mut linaje = if p.casa == pu.casa_reinante { 1.0 } else { 0.0 };
        if linaje == 0.0 {
            if let Some(par) = p.pareja.and_then(|x| self.personas.get(&x)) {
                if par.casa == pu.casa_reinante {
                    linaje = 0.5;
                }
            }
        }
        let prestigio = if id == JUGADOR_ID { self.jugador.prestigio.get(&pueblo).copied().unwrap_or(0.0) } else { 0.0 };
        [
            p.fuerza * 0.5 + p.audacia * 0.2 + if p.oficio == Oficio::Soldado { 0.3 } else { 0.0 },
            linaje,
            p.fe * 0.4 + p.carisma * 0.2 + if p.oficio == Oficio::Sacerdote { 0.4 } else { 0.0 } + if p.culto == pu.culto { 0.1 } else { 0.0 },
            (p.carisma * 0.5 + (p.fama / 10.0).min(0.3) + p.bondad * 0.2 + prestigio).min(1.2),
            p.riqueza / max_riq,
        ]
    }

    pub fn poder(&self, pueblo: u32, id: u32) -> f32 {
        let Some(pu) = self.pueblos.get(&pueblo) else { return 0.0 };
        let c = self.poder_componentes(pueblo, id);
        let base: f32 = (0..N_FUENTES).map(|i| pu.conv.w[i] * c[i]).sum();
        let intel = self.personas.get(&id).map(|p| p.inteligencia * 0.05).unwrap_or(0.0);
        base + intel
    }

    fn candidatos(&self, pueblo: u32) -> Vec<u32> {
        let Some(pu) = self.pueblos.get(&pueblo) else { return Vec::new() };
        pu.gente
            .iter()
            .copied()
            .filter(|g| {
                self.personas
                    .get(g)
                    .map(|p| p.edad(self.dia) >= 18.0 && p.libre(self.dia) && (*g != JUGADOR_ID || self.jugador.candidato == Some(pueblo)))
                    .unwrap_or(false)
            })
            .collect()
    }

    fn fuente_decisiva(&self, pueblo: u32, ganador: u32, rival: Option<u32>) -> usize {
        let pu = &self.pueblos[&pueblo];
        let a = self.poder_componentes(pueblo, ganador);
        let b = rival.map(|r| self.poder_componentes(pueblo, r)).unwrap_or([0.0; N_FUENTES]);
        let mut mejor = (0, f32::MIN);
        for i in 0..N_FUENTES {
            let v = pu.conv.w[i] * (a[i] - b[i]);
            if v > mejor.1 {
                mejor = (i, v);
            }
        }
        mejor.0
    }

    /// Elige a quien manda según la convención. `log`: anotar en la crónica.
    pub fn elegir_gobernante(&mut self, pueblo: u32, log: bool) {
        let cand = self.candidatos(pueblo);
        if cand.is_empty() {
            return;
        }
        let previo = self.pueblos[&pueblo].gobernante;
        let mut orden: Vec<(f32, u32)> = cand.iter().map(|c| (self.poder(pueblo, *c) + if Some(*c) == previo { 0.03 } else { 0.0 }, *c)).collect();
        orden.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap().then(a.1.cmp(&b.1)));
        let ganador = orden[0].1;
        let rival = orden.get(1).map(|x| x.1);
        let fuente = self.fuente_decisiva(pueblo, ganador, rival);
        self.coronar(pueblo, ganador, fuente, log);
    }

    fn coronar(&mut self, pueblo: u32, ganador: u32, fuente: usize, log: bool) {
        let (nombre, casa) = {
            let p = &self.personas[&ganador];
            (p.nombre.clone(), p.casa)
        };
        let lugar = self.nombre_pueblo(pueblo);
        let dia = self.dia;
        let pu = self.pueblos.get_mut(&pueblo).unwrap();
        let cambio_casa = pu.casa_reinante != casa;
        pu.gobernante = Some(ganador);
        pu.casa_reinante = casa;
        pu.gobierna_desde = dia;
        if fuente != pu.conv.dominante() {
            pu.conv.empujar(fuente, 0.03);
        }
        if log {
            let (tipo, frase) = match fuente {
                Convencion::LINAJE => (Tipo::Sucesion, if cambio_casa { "asciende al mando" } else { "hereda el mando" }),
                Convencion::VOTO => (Tipo::Eleccion, "es elegido"),
                Convencion::FE => (Tipo::Sucesion, "es consagrado por el clero"),
                Convencion::RIQUEZA => (Tipo::Sucesion, "compra el mando"),
                _ => (Tipo::Sucesion, "se impone por la fuerza"),
            };
            self.hecho(tipo, pueblo, ganador, format!("{nombre} {frase} en {lugar}."));
        }
    }

    pub(crate) fn gobierno_anual(&mut self) {
        let ids: Vec<u32> = self.pueblos.keys().copied().collect();
        for id in ids {
            if !self.pueblos[&id].vivo {
                continue;
            }
            self.deriva_convencion(id);
            let tiene = self.pueblos[&id].gobernante.map(|g| self.personas.contains_key(&g)).unwrap_or(false);
            if !tiene {
                self.pueblos.get_mut(&id).unwrap().gobernante = None;
                self.elegir_gobernante(id, true);
            }
            self.revisar_leyes(id);
            self.desafios(id);
        }
    }

    fn deriva_convencion(&mut self, id: u32) {
        let guerra = self.guerra_de(id).is_some();
        let sacerdotes = {
            let pu = &self.pueblos[&id];
            let n = pu.gente.len().max(1) as f32;
            pu.gente.iter().filter(|g| self.personas.get(g).map(|p| p.oficio == Oficio::Sacerdote).unwrap_or(false)).count() as f32 / n
        };
        let dia = self.dia;
        let pu = self.pueblos.get_mut(&id).unwrap();
        let s = pu.saberes;
        let mut cambios = [0.0f32; N_FUENTES];
        if s.tiene(DERECHO) && s.tiene(FILOSOFIA) {
            cambios[Convencion::VOTO] += 0.006;
        }
        if s.tiene(IMPRENTA) {
            cambios[Convencion::VOTO] += 0.005;
        }
        if s.tiene(MONEDA) {
            cambios[Convencion::RIQUEZA] += 0.003;
        }
        if guerra {
            cambios[Convencion::FUERZA] += 0.012;
        }
        cambios[Convencion::FE] += 0.02 * (sacerdotes - 0.1);
        if dia.saturating_sub(pu.gobierna_desde) > 6 * DIAS_ANIO {
            cambios[Convencion::LINAJE] += 0.004;
        }
        cambios[Convencion::FUERZA] -= 0.002;
        for i in 0..N_FUENTES {
            pu.conv.w[i] += cambios[i];
        }
        pu.conv.normalizar();
    }

    /// El gobernante revisa las leyes según quién es.
    pub fn revisar_leyes(&mut self, id: u32) {
        let Some(g) = self.pueblos[&id].gobernante.and_then(|g| self.personas.get(&g)).cloned() else { return };
        let guerra = self.guerra_de(id).is_some();
        let pu = self.pueblos.get_mut(&id).unwrap();
        let antes = pu.leyes;
        let l = &mut pu.leyes;
        let obj_imp = (0.08 + 0.35 * g.codicia - 0.10 * g.bondad + if guerra { 0.06 } else { 0.0 }).clamp(0.02, 0.5);
        l.impuesto += (obj_imp - l.impuesto).clamp(-0.04, 0.04);
        let dureza = (1.0 - g.bondad) * 3.0 + if guerra { 0.3 } else { 0.0 };
        let obj_robo = dureza.round().clamp(0.0, 3.0) as u8;
        if obj_robo > l.pena_robo {
            l.pena_robo += 1;
        } else if obj_robo < l.pena_robo {
            l.pena_robo -= 1;
        }
        l.pena_homicidio = (l.pena_robo + 1).min(3).max(if g.bondad < 0.4 { 2 } else { 1 });
        let obj_culto = (1.0 - 0.95 * g.fe * (0.5 + 0.5 * pu.conv.w[Convencion::FE] * 2.0)).clamp(0.05, 1.0);
        l.libertad_culto += (obj_culto - l.libertad_culto).clamp(-0.1, 0.1);
        let obj_serv = (0.08 + 0.6 * g.ambicion * if guerra { 1.0 } else { 0.5 }).clamp(0.02, 0.7);
        l.servicio += (obj_serv - l.servicio).clamp(-0.05, 0.05);
        let cambio = antes.pena_robo != l.pena_robo;
        // leyes duras: orden sube, legitimidad baja (miedo, no aprecio)
        if l.pena_robo >= 3 {
            pu.legit -= 0.02;
        }
        if l.libertad_culto < 0.3 {
            pu.legit -= 0.015;
        }
        if cambio {
            let nombre = pu.nombre.clone();
            let imp = (pu.leyes.impuesto * 100.0).round();
            let pena = ["multa", "cárcel", "destierro", "pena de muerte"][pu.leyes.pena_robo as usize];
            self.hecho(Tipo::Ley, id, g.id, format!("{} decreta en {}: impuesto del {}% y {} para el robo.", g.nombre, nombre, imp, pena));
        }
    }

    fn desafios(&mut self, id: u32) {
        let dia = self.dia;
        let Some(inc) = self.pueblos[&id].gobernante else { return };
        // elecciones periódicas donde el voto pesa
        let (voto, ult) = (self.pueblos[&id].conv.w[Convencion::VOTO], self.pueblos[&id].ult_eleccion);
        if voto >= 0.28 && dia - ult >= ELECCION_CADA {
            self.eleccion(id);
            return;
        }
        let cand = self.candidatos(id);
        let mut mejor: Option<(f32, u32)> = None;
        for c in cand {
            if c == inc {
                continue;
            }
            let p = self.poder(id, c);
            if mejor.map(|m| p > m.0).unwrap_or(true) {
                mejor = Some((p, c));
            }
        }
        let inc_poder = self.poder(id, inc) + 0.05;
        if let Some((pc, ch)) = mejor {
            if pc > inc_poder {
                let fuente = self.fuente_decisiva(id, ch, Some(inc));
                let legit = self.pueblos[&id].legit;
                let amb = self.personas[&ch].ambicion;
                let edad_inc = self.personas[&inc].edad(dia);
                let ocurre = match fuente {
                    Convencion::FUERZA => self.rng.p(amb * (0.15 + 0.85 * (1.0 - legit))),
                    Convencion::LINAJE => edad_inc > 60.0 && self.rng.p(0.4),
                    Convencion::FE => self.rng.p(0.25),
                    Convencion::RIQUEZA => self.rng.p(0.2),
                    _ => false, // el voto sólo actúa el día de la elección
                };
                if ocurre {
                    self.desplazar(id, inc, ch, fuente);
                }
            }
        }
        let (legit, moral) = (self.pueblos[&id].legit, self.pueblos[&id].moral);
        if legit < 0.22 && moral < 0.42 && self.rng.p(0.5) {
            self.revolucion(id);
        }
    }

    fn desplazar(&mut self, id: u32, viejo: u32, nuevo: u32, fuente: usize) {
        let lugar = self.nombre_pueblo(id);
        let (nv, nn, bondad_n) = (self.nombre_persona(viejo), self.nombre_persona(nuevo), self.personas[&nuevo].bondad);
        if fuente == Convencion::FUERZA {
            self.hecho(Tipo::Golpe, id, nuevo, format!("Golpe en {lugar}: {nn} derriba a {nv}."));
            let mata = self.rng.p((1.0 - bondad_n) * 0.6);
            if mata {
                self.matar(viejo, "ejecutado tras el golpe");
            } else {
                self.exiliar(viejo);
            }
        } else {
            let frase = if fuente == Convencion::LINAJE { "abdica en" } else { "cede el mando a" };
            self.hecho(Tipo::Sucesion, id, nuevo, format!("{nv} {frase} {nn} en {lugar}."));
        }
        if let Some(pu) = self.pueblos.get_mut(&id) {
            pu.gobernante = Some(nuevo);
        }
        self.coronar(id, nuevo, fuente, false);
        let pu = self.pueblos.get_mut(&id).unwrap();
        if fuente == Convencion::FUERZA {
            pu.conv.empujar(Convencion::FUERZA, 0.05);
            pu.legit = (pu.legit + 0.15).min(0.6);
        }
    }

    /// Lo saca del pueblo: se va a un vecino (o desaparece de la escena).
    pub(crate) fn exiliar(&mut self, id: u32) {
        let Some(p) = self.personas.get(&id) else { return };
        let origen = p.pueblo;
        let destinos = self.contactos(origen);
        if destinos.is_empty() {
            self.matar(id, "en el exilio");
            return;
        }
        let d = destinos[self.rng.idx(destinos.len())];
        self.mudar(id, d);
    }

    pub(crate) fn mudar(&mut self, id: u32, destino: u32) {
        let Some(p) = self.personas.get_mut(&id) else { return };
        let origen = p.pueblo;
        p.pueblo = destino;
        p.preso_hasta = 0;
        if let Some(o) = self.pueblos.get_mut(&origen) {
            o.gente.retain(|x| *x != id);
            if o.gobernante == Some(id) {
                o.gobernante = None;
            }
        }
        if let Some(d) = self.pueblos.get_mut(&destino) {
            d.gente.push(id);
        }
        if let Some(par) = self.personas.get(&id).and_then(|p| p.pareja) {
            if self.personas.get(&par).map(|x| x.pueblo == origen).unwrap_or(false) {
                self.personas.get_mut(&id).unwrap().pareja = None;
                self.personas.get_mut(&par).unwrap().pareja = None;
            }
        }
    }

    pub fn eleccion(&mut self, id: u32) {
        let mut cand: Vec<(f32, u32)> = self.candidatos(id).into_iter().map(|c| (self.poder(id, c), c)).collect();
        cand.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap().then(a.1.cmp(&b.1)));
        cand.truncate(3);
        if cand.is_empty() {
            return;
        }
        let pesos: Vec<f32> = cand.iter().map(|c| (c.0 * 7.0).exp()).collect();
        let total: f32 = pesos.iter().sum();
        let mut u = self.rng.f() * total;
        let mut gana = 0;
        for (i, p) in pesos.iter().enumerate() {
            if u < *p {
                gana = i;
                break;
            }
            u -= p;
        }
        let ganador = cand[gana].1;
        let pct = (100.0 * pesos[gana] / total).round();
        let lugar = self.nombre_pueblo(id);
        let nombre = self.nombre_persona(ganador);
        let dia = self.dia;
        let previo = self.pueblos[&id].gobernante;
        {
            let pu = self.pueblos.get_mut(&id).unwrap();
            pu.ult_eleccion = dia;
            pu.gobernante = Some(ganador);
            pu.casa_reinante = self.personas[&ganador].casa;
            pu.gobierna_desde = dia;
            pu.conv.empujar(Convencion::VOTO, 0.015);
            pu.legit = (pu.legit + 0.08).min(1.0);
        }
        let reelecto = previo == Some(ganador);
        self.hecho(
            Tipo::Eleccion,
            id,
            ganador,
            format!("Elecciones en {lugar}: {nombre} {} con el {pct}% de los votos.", if reelecto { "es reelegido" } else { "gana" }),
        );
    }

    fn revolucion(&mut self, id: u32) {
        let Some(inc) = self.pueblos[&id].gobernante else { return };
        let cand = self.candidatos(id);
        let lugar = self.nombre_pueblo(id);
        // el que más ha ganado en el descontento: quien menos se parece al depuesto
        let mut mejor: Option<(f32, u32, usize)> = None;
        for c in cand {
            if c == inc {
                continue;
            }
            let comp = self.poder_componentes(id, c);
            let mut top = (0usize, 0.0f32);
            for (i, v) in comp.iter().enumerate() {
                if *v > top.1 {
                    top = (i, *v);
                }
            }
            let p = self.personas[&c].ambicion + top.1 + self.personas[&c].carisma;
            if mejor.map(|m| p > m.0).unwrap_or(true) {
                mejor = Some((p, c, top.0));
            }
        }
        let Some((_, nuevo, fuente)) = mejor else { return };
        let (nv, nn) = (self.nombre_persona(inc), self.nombre_persona(nuevo));
        self.hecho(Tipo::Revolucion, id, nuevo, format!("Revolución en {lugar}: el pueblo derriba a {nv} y sigue a {nn}."));
        if self.rng.p(0.5) {
            self.matar(inc, "linchado");
        } else {
            self.exiliar(inc);
        }
        {
            let pu = self.pueblos.get_mut(&id).unwrap();
            pu.conv.empujar(fuente, 0.15);
            pu.legit = 0.65;
            pu.moral = (pu.moral + 0.2).min(1.0);
            pu.leyes.impuesto *= 0.6;
            pu.gobernante = Some(nuevo);
        }
        self.coronar(id, nuevo, fuente, false);
        let _ = self.pueblos[&id].regimen();
    }

    // --- justicia -------------------------------------------------------------------

    pub(crate) fn justicia_dia(&mut self) {
        let dia = self.dia;
        let ids: Vec<u32> = self.pueblos.keys().copied().collect();
        for id in ids {
            let (vivo, hambre, orden, robo, homi, pob) = {
                let p = &self.pueblos[&id];
                (p.vivo, p.hambre, p.orden, p.leyes.pena_robo, p.leyes.pena_homicidio, p.pob)
            };
            if !vivo {
                continue;
            }
            // la multitud: delitos anónimos que sólo mueven el ánimo del pueblo
            let lam = pob * 0.00002 * (1.0 + 3.0 * hambre + 2.0 * (1.0 - orden));
            if self.rng.p(lam.min(0.9)) {
                self.pueblos.get_mut(&id).unwrap().crimen += 0.02;
            }
            let gente: Vec<u32> = self.pueblos[&id].gente.clone();
            if gente.len() < 3 {
                continue;
            }
            for g in &gente {
                let Some(p) = self.personas.get(g) else { continue };
                if *g == JUGADOR_ID || !p.adulta(dia) || !p.libre(dia) {
                    continue;
                }
                let robar = p.codicia > 0.5 && p.bondad < 0.4 && self.rng.p(0.0012 * (1.0 + 3.0 * hambre) * p.codicia);
                let matar_ = !robar && p.ambicion + (1.0 - p.bondad) > 1.35 && self.rng.p(0.00012);
                if !(robar || matar_) {
                    continue;
                }
                let victima = gente[self.rng.idx(gente.len())];
                if victima == *g || !self.personas.contains_key(&victima) {
                    continue;
                }
                let pillado = self.rng.p(orden * 0.7 + 0.1);
                let pena = if robar { robo } else { homi };
                let (nombre_c, nombre_v) = (self.nombre_persona(*g), self.nombre_persona(victima));
                let lugar = self.nombre_pueblo(id);
                self.pueblos.get_mut(&id).unwrap().crimen += if robar { 0.05 } else { 0.3 };
                if robar {
                    let botin = (self.personas[&victima].riqueza * 0.3).min(8.0);
                    self.personas.get_mut(&victima).unwrap().riqueza -= botin;
                    self.personas.get_mut(g).unwrap().riqueza += botin;
                } else {
                    self.hecho(Tipo::Crimen, id, *g, format!("{nombre_c} mata a {nombre_v} en {lugar}."));
                    self.matar(victima, "asesinado");
                }
                if !pillado {
                    continue;
                }
                match pena {
                    0 => {
                        let multa = self.personas[g].riqueza * 0.5;
                        self.personas.get_mut(g).unwrap().riqueza -= multa;
                        if robar {
                            self.personas.get_mut(&victima).unwrap().riqueza += multa;
                        }
                    }
                    1 => {
                        let dias = 3 + self.rng.idx(6) as u64;
                        self.personas.get_mut(g).unwrap().preso_hasta = dia + dias;
                        self.hecho(Tipo::Justicia, id, *g, format!("{nombre_c} va preso en {lugar} por {}.", if robar { "robar" } else { "matar" }));
                    }
                    2 => {
                        self.hecho(Tipo::Justicia, id, *g, format!("{nombre_c} es desterrado de {lugar}."));
                        self.exiliar(*g);
                    }
                    _ => {
                        self.hecho(Tipo::Justicia, id, *g, format!("{nombre_c} es ejecutado en {lugar}."));
                        self.matar(*g, "ejecutado");
                    }
                }
                break; // un delito por pueblo y día
            }
        }
    }
}
