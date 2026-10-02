//! Obras, naves, colonias y hallazgos.
//!
//! Nada de esto está programado para ocurrir: una nave sale cuando un pueblo sabe
//! lo suficiente, tiene el material y su gobernante quiere ir. Lo que pasa después
//! (una colonia que se apaga, o crece, o se independiza) sale de las mismas reglas
//! de comida, ánimo y legitimidad que rigen a cualquier otro pueblo.

use super::cronica::Tipo;
use super::mundo::Mundo;
use super::persona::Oficio;
use super::pueblo::*;
use super::saber::*;
use super::DIAS_ANIO;

pub const CUERPOS: [&str; 12] = [
    "la Tierra", "la Luna", "Mercurio", "Venus", "Marte", "Fobos", "Deimos", "Ío", "Europa", "Titán", "Urano", "Neptuno",
];

/// (habitabilidad, días de viaje)
fn ficha_cuerpo(c: u8) -> (f32, u64) {
    match c {
        1 => (0.35, 8),
        2 => (0.10, 40),
        3 => (0.08, 45),
        4 => (0.45, 60),
        5 | 6 => (0.20, 62),
        7 | 8 => (0.20, 140),
        9 => (0.30, 170),
        _ => (0.10, 220),
    }
}

impl Mundo {
    fn destinos_posibles(&self, pueblo: u32) -> Vec<u8> {
        let Some(p) = self.pueblos.get(&pueblo) else { return Vec::new() };
        let alc = p.capacidad().alcance();
        (1u8..=11)
            .filter(|c| alcance_para(*c).map(|a| alc >= a).unwrap_or(false))
            .filter(|c| !self.pueblos.values().any(|q| q.vivo && q.cuerpo == *c))
            .filter(|c| !self.expediciones.iter().any(|e| e.destino == *c))
            .collect()
    }

    fn costo_obra(o: Obra, nivel: u8) -> f32 {
        match o {
            Obra::Biblioteca => 400.0 * (1.0 + nivel as f32),
            Obra::Escuela => 500.0 * (1.0 + nivel as f32),
            Obra::Muralla => 700.0 * (1.0 + nivel as f32),
            Obra::Nave => 6000.0,
            Obra::Refugio => 900.0 * (1.0 + nivel as f32),
        }
    }

    fn elegir_obra(&mut self, id: u32) -> Option<Proyecto> {
        let p = self.pueblos.get(&id)?;
        let g = p.gobernante.and_then(|g| self.personas.get(&g))?.clone();
        let guerra = self.guerra_de(id).is_some();
        let amenaza = p.vecinos.iter().any(|v| self.fuerza_militar(*v, false) > self.fuerza_militar(id, true) * 1.3);
        let mut ops: Vec<(f32, Obra, u8)> = Vec::new();
        if p.saberes.tiene(ESCRITURA) && p.biblioteca < 3 {
            ops.push((0.8 + g.curiosidad, Obra::Biblioteca, 0));
        }
        if p.saberes.tiene(ESCRITURA) && p.escuela < 3 {
            ops.push((0.6 + g.curiosidad + 0.5 * g.bondad, Obra::Escuela, 0));
        }
        if p.saberes.tiene(ARQUITECTURA) && p.muralla < 3 {
            ops.push((0.2 + if guerra { 2.5 } else { 0.0 } + if amenaza { 1.2 } else { 0.0 } + g.ambicion * 0.5, Obra::Muralla, 0));
        }
        let bomba_cerca = p.saberes.tiene(NUCLEAR) && (p.arsenal > 0 || p.vecinos.iter().any(|v| self.pueblos.get(v).map(|q| q.arsenal > 0).unwrap_or(false)));
        if bomba_cerca && p.saberes.tiene(ARQUITECTURA) && p.refugio < 3 {
            ops.push((0.4 + 1.6 * g.audacia.min(0.5) + if guerra { 1.2 } else { 0.0 } + 0.8 * (1.0 - g.bondad), Obra::Refugio, 0));
        }
        let destinos = self.destinos_posibles(id);
        if !destinos.is_empty() && p.saberes.tiene(SOPORTE_VITAL) && g.audacia > 0.35 && p.mat > 1500.0 {
            let d = destinos[0];
            ops.push((1.0 + 2.5 * g.audacia + g.curiosidad, Obra::Nave, d));
        }
        if ops.is_empty() {
            return None;
        }
        let total: f32 = ops.iter().map(|o| o.0).sum();
        let mut u = self.rng.f() * total;
        for (w, o, d) in ops {
            if u < w {
                let nivel = match o {
                    Obra::Biblioteca => p.biblioteca,
                    Obra::Escuela => p.escuela,
                    Obra::Muralla => p.muralla,
                    Obra::Nave => 0,
                    Obra::Refugio => p.refugio,
                };
                return Some(Proyecto { obra: o, prog: 0.0, costo: Self::costo_obra(o, nivel), destino: d });
            }
            u -= w;
        }
        None
    }

    pub(crate) fn obras_dia(&mut self) {
        let ids: Vec<u32> = self.pueblos.keys().copied().collect();
        for id in ids {
            if !self.pueblos[&id].vivo {
                continue;
            }
            self.abastecer_colonia(id);
            self.extincion_colonia(id);
            if self.pueblos[&id].proyecto.is_none() {
                if self.rng.p(0.05) {
                    let pr = self.elegir_obra(id);
                    self.pueblos.get_mut(&id).unwrap().proyecto = pr;
                }
                continue;
            }
            let guerra = self.guerra_de(id).is_some();
            let (fin, obra, destino) = {
                let p = self.pueblos.get_mut(&id).unwrap();
                let cap = p.capacidad();
                let mut pr = p.proyecto.unwrap();
                let mut rate = p.pob.min(4000.0) * 0.02 * (0.6 + cap.mat.min(3.0)) * if guerra { 0.5 } else { 1.0 };
                let gasto = rate * 0.8;
                if p.mat < gasto {
                    rate *= (p.mat / gasto).max(0.0);
                }
                p.mat = (p.mat - gasto.min(p.mat)).max(0.0);
                pr.prog += rate;
                let fin = pr.prog >= pr.costo;
                p.proyecto = if fin { None } else { Some(pr) };
                (fin, pr.obra, pr.destino)
            };
            if !fin {
                continue;
            }
            let nombre = self.nombre_pueblo(id);
            match obra {
                Obra::Biblioteca => self.pueblos.get_mut(&id).unwrap().biblioteca += 1,
                Obra::Escuela => self.pueblos.get_mut(&id).unwrap().escuela += 1,
                Obra::Muralla => self.pueblos.get_mut(&id).unwrap().muralla += 1,
                Obra::Refugio => self.pueblos.get_mut(&id).unwrap().refugio += 1,
                Obra::Nave => {
                    // el cielo cambia mientras se construye: se retoma el rumbo o se cancela
                    let libres = self.destinos_posibles(id);
                    let d = if libres.contains(&destino) { Some(destino) } else { libres.first().copied() };
                    match d {
                        Some(d) => self.lanzar(id, d),
                        None => self.pueblos.get_mut(&id).unwrap().mat += 1000.0,
                    }
                    continue;
                }
            }
            self.hecho(Tipo::Obra, id, 0, format!("{nombre} termina su {}.", obra.nombre()));
        }
    }

    fn abastecer_colonia(&mut self, id: u32) {
        let Some(senor) = self.pueblos[&id].senor else { return };
        if self.pueblos[&senor].cuerpo == self.pueblos[&id].cuerpo || !self.pueblos[&senor].vivo {
            return;
        }
        let (falta_c, falta_m) = {
            let p = &self.pueblos[&id];
            ((p.pob * 2.0 - p.comida).max(0.0), (p.pob * 1.5 - p.mat).max(0.0))
        };
        let (dc, dm) = {
            let s = &self.pueblos[&senor];
            (falta_c.min((s.comida - s.pob * 3.0).max(0.0) * 0.05), falta_m.min((s.mat - s.pob * 2.0).max(0.0) * 0.05))
        };
        let s = self.pueblos.get_mut(&senor).unwrap();
        s.comida -= dc;
        s.mat -= dm;
        let p = self.pueblos.get_mut(&id).unwrap();
        p.comida += dc;
        p.mat += dm;
    }

    fn extincion_colonia(&mut self, id: u32) {
        let p = &self.pueblos[&id];
        if p.cuerpo == 0 || p.hambre < 0.95 || p.pob > 14.0 || !self.rng.p(0.05) {
            return;
        }
        let nombre = p.nombre.clone();
        let gente = p.gente.clone();
        for g in gente {
            self.matar(g, "sin aire ni comida");
        }
        let p = self.pueblos.get_mut(&id).unwrap();
        p.vivo = false;
        p.proyecto = None;
        self.hecho(Tipo::Tragedia, id, 0, format!("La colonia {nombre} se apaga: no quedó nadie."));
    }

    /// Prepara y despega una expedición desde `origen` hacia el cuerpo `destino`.
    pub fn lanzar(&mut self, origen: u32, destino: u8) {
        let dia = self.dia;
        let mut cand: Vec<u32> = self.pueblos[&origen]
            .gente
            .iter()
            .copied()
            .filter(|g| self.personas.get(g).map(|p| p.adulta(dia) && p.libre(dia) && p.edad(dia) < 55.0 && *g != super::mundo::JUGADOR_ID).unwrap_or(false))
            .collect();
        cand.sort_by(|a, b| {
            let f = |i: &u32| {
                let p = &self.personas[i];
                p.audacia + p.inteligencia * 0.5 + if p.oficio == Oficio::Erudito { 0.3 } else { 0.0 }
            };
            f(b).partial_cmp(&f(a)).unwrap().then(a.cmp(b))
        });
        let n = (3 + self.rng.idx(4)).min(cand.len().saturating_sub(2));
        if n == 0 {
            return;
        }
        let tripulacion: Vec<u32> = cand.into_iter().take(n).collect();
        for t in &tripulacion {
            if let Some(o) = self.pueblos.get_mut(&origen) {
                o.gente.retain(|x| x != t);
                if o.gobernante == Some(*t) {
                    o.gobernante = None;
                }
            }
            if let Some(p) = self.personas.get_mut(t) {
                p.pueblo = 0;
                p.estudio = None;
            }
        }
        let s = self.pueblos[&origen].saberes;
        let calidad = (0.5
            + [COMPUTACION, MECANICA_ORBITAL, SOPORTE_VITAL, MATERIALES, ASTRONAUTICA].iter().filter(|x| s.tiene(**x)).count() as f32 * 0.07
            + if destino >= 2 && s.tiene(PROPULSION_AVANZADA) { 0.1 } else { 0.0 }
            + if destino >= 7 && s.tiene(PROPULSION_PROFUNDA) { 0.1 } else { 0.0 })
        .clamp(0.3, 0.95);
        // Una nave calculada con la gravedad mal medida no llega: la verdad manda, no la creencia.
        let calidad = (calidad * self.factor_ingenieria(origen, &[super::fisica::GRAVEDAD, super::fisica::ORBITAS])).clamp(0.12, 0.95);
        let prov = self.pueblos[&origen].mat.min(600.0) * 0.5 + 100.0;
        self.pueblos.get_mut(&origen).unwrap().mat -= prov.min(self.pueblos[&origen].mat);
        let id = self.sig_expedicion;
        self.sig_expedicion += 1;
        let (_, dias) = ficha_cuerpo(destino);
        self.expediciones.push(Expedicion { id, origen, destino, salida: dia, llegada: dia + dias, tripulacion, provisiones: prov, calidad });
        let (no, nd) = (self.nombre_pueblo(origen), CUERPOS[destino as usize]);
        self.hecho(Tipo::Lanzamiento, origen, destino as u32, format!("{no} lanza una nave tripulada hacia {nd}."));
    }

    pub(crate) fn expediciones_dia(&mut self) {
        let dia = self.dia;
        let mut i = 0;
        while i < self.expediciones.len() {
            if self.expediciones[i].llegada > dia {
                i += 1;
                continue;
            }
            let e = self.expediciones.remove(i);
            let nd = CUERPOS[e.destino as usize];
            let no = self.nombre_pueblo(e.origen);
            if !self.rng.p(e.calidad) {
                for t in &e.tripulacion {
                    self.matar(*t, "en el espacio");
                }
                self.hecho(Tipo::Tragedia, e.origen, e.destino as u32, format!("La nave de {no} se pierde rumbo a {nd}."));
                self.aprender_de_falla(e.origen, &[super::fisica::GRAVEDAD, super::fisica::ORBITAS]);
                continue;
            }
            let existente = self.pueblos.values().find(|p| p.vivo && p.cuerpo == e.destino).map(|p| p.id);
            match existente {
                Some(c) => {
                    for t in &e.tripulacion {
                        if let Some(p) = self.personas.get_mut(t) {
                            p.pueblo = 0;
                        }
                        self.mudar(*t, c);
                    }
                    self.pueblos.get_mut(&c).unwrap().pob += e.tripulacion.len() as f32 * 3.0;
                    self.hecho(Tipo::Llegada, c, e.origen, format!("Llegan colonos de {no} a {}.", self.nombre_pueblo(c)));
                }
                None => {
                    self.hecho(Tipo::Llegada, e.origen, e.destino as u32, format!("Los primeros humanos llegan a {nd}, enviados por {no}."));
                    self.fundar_colonia(&e);
                }
            }
        }
    }

    fn fundar_colonia(&mut self, e: &Expedicion) {
        let id = self.sig_pueblo;
        self.sig_pueblo += 1;
        let (hab, _) = ficha_cuerpo(e.destino);
        let origen = self.pueblos[&e.origen].clone();
        let mut p = Pueblo::vacio(id);
        p.nombre = format!("Base {} {}", CUERPOS[e.destino as usize].trim_start_matches("la "), id);
        p.cuerpo = e.destino;
        p.x = 0.0;
        p.z = 0.0;
        p.rango = 0;
        p.habit = hab;
        p.sustrato = S_PIEDRA | S_ARENA | S_CIELO;
        p.carga = 80.0;
        p.pob = (e.tripulacion.len() as f32 * 4.0).max(12.0);
        p.comida = e.provisiones * 2.0;
        p.mat = e.provisiones;
        p.tesoro = 100.0;
        p.saberes = e.tripulacion.iter().filter_map(|t| self.personas.get(t)).fold(origen.saberes, |a, x| a.union(x.saberes));
        p.archivo = origen.archivo;
        p.creencias = origen.creencias;
        p.biblioteca = 1;
        p.escuela = 1;
        p.culto = origen.culto;
        p.senor = Some(e.origen);
        p.fundado = self.dia;
        p.conv = Convencion::nueva([0.4, 0.1, 0.05, 0.3, 0.15]);
        p.legit = 0.75;
        p.leyes = Leyes { servicio: 0.3, ..origen.leyes };
        self.pueblos.insert(id, p);
        for t in &e.tripulacion {
            if self.personas.contains_key(t) {
                self.personas.get_mut(t).unwrap().pueblo = id;
                self.pueblos.get_mut(&id).unwrap().gente.push(*t);
            }
        }
        let r = self.relacion_mut(e.origen, id);
        r.actitud = 0.6;
        r.comercio = true;
        self.armar_vecinos();
        self.elegir_gobernante(id, true);
        let nombre = self.pueblos[&id].nombre.clone();
        self.hecho(Tipo::Fundacion, id, e.origen, format!("Se funda {nombre}."));
    }

    pub(crate) fn colonias_anual(&mut self) {}

    pub(crate) fn hallazgos_anual(&mut self) {
        if !self.hallazgos {
            return;
        }
        let ids: Vec<u32> = self.pueblos.keys().copied().collect();
        for id in ids {
            if !self.pueblos[&id].vivo || self.pueblos[&id].cuerpo != 0 || !self.rng.p(0.00015) {
                continue;
            }
            let objetivos = [COHETERIA, MECANICA_ORBITAL, SOPORTE_VITAL, ASTRONAUTICA];
            let s = objetivos[self.rng.idx(objetivos.len())];
            self.dar_artefacto(id, s);
        }
        let _ = DIAS_ANIO;
    }

    /// Un objeto de tecnología que el pueblo no puede fabricar todavía pero sí desarmar.
    pub fn dar_artefacto(&mut self, pueblo: u32, saber: u8) -> u32 {
        let id = self.sig_artefacto;
        self.sig_artefacto += 1;
        let nombre = match saber {
            COHETERIA => "un motor de cohete",
            MECANICA_ORBITAL => "una computadora de a bordo",
            SOPORTE_VITAL => "un módulo de soporte vital",
            ASTRONAUTICA => "los restos de una nave",
            _ => "una pieza de tecnología ajena",
        }
        .to_string();
        self.artefactos.push(Artefacto { id, saber, pueblo, nombre: nombre.clone() });
        let lugar = self.nombre_pueblo(pueblo);
        self.hecho(Tipo::Hallazgo, pueblo, 0, format!("En {lugar} aparece {nombre}. Los eruditos empiezan a estudiarlo."));
        id
    }
}
