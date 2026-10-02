//! LA BOMBA, EL INVIERNO Y LO QUE VIENE DESPUÉS.
//!
//! Nada de esto está guionado. Un pueblo que sabe fisión y tiene uranio a mano puede
//! armar ojivas; el que las tiene y va perdiendo (o su jefe es soberbio) puede usarlas;
//! el golpeado responde. Una bomba mal calculada no explota como debe (la verdad de
//! `fisica` manda). Después vienen la lluvia radiactiva, el invierno nuclear, el hambre
//! y, si nadie se cubrió, la extinción de pueblos enteros.
//!
//! Lo que se salva: quien esté en un refugio (el que levantan los pueblos, o el que
//! construye el jugador con sus propias manos y al que invita a la gente). Los
//! sobrevivientes pueden salir y fundar un pueblo nuevo, y los pueblos vecinos que
//! quedan en pie repueblan las ruinas: surge otra civilización.

use serde::{Deserialize, Serialize};

use super::azar::hash;
use super::cronica::Tipo;
use super::fisica::{Creencias, FISION, N_LEYES};
use super::gente::objetivo_notables;
use super::mundo::{Mundo, JUGADOR_ID};
use super::DIAS_ANIO;
use super::nombres;
use super::pueblo::*;
use super::saber::*;

pub const TOPE_ARSENAL: u16 = 40;

/// Un refugio antiatómico: un lugar con gente adentro y comida para X días.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Refugio {
    pub id: u32,
    pub x: f32,
    pub z: f32,
    /// Pueblo más cercano al construirse (para la crónica).
    pub pueblo: u32,
    pub capacidad: u16,
    pub comida: f32,
    pub gente: Vec<u32>,
    pub creado: u64,
    /// Lo que saben los que se refugiaron: la física que se llevan bajo tierra.
    pub creencias: Creencias,
}

impl Mundo {
    fn cercano(&self, x: f32, z: f32, cuerpo: u8) -> Option<(u32, f32)> {
        self.pueblos
            .values()
            .filter(|p| p.cuerpo == cuerpo)
            .map(|p| (p.id, ((p.x - x).powi(2) + (p.z - z).powi(2)).sqrt()))
            .min_by(|a, b| a.1.partial_cmp(&b.1).unwrap().then(a.0.cmp(&b.0)))
    }

    /// Lluvia radiactiva en un punto de la Tierra: la del pueblo más cercano,
    /// atenuada por la distancia.
    pub fn radiacion_en(&self, x: f32, z: f32) -> f32 {
        self.pueblos
            .values()
            .filter(|p| p.cuerpo == 0 && p.radiacion > 0.0)
            .map(|p| {
                let d = ((p.x - x).powi(2) + (p.z - z).powi(2)).sqrt();
                p.radiacion * (1.0 - d / 3000.0).max(0.0)
            })
            .fold(0.0, f32::max)
    }

    // --- refugios: la puerta del jugador ---------------------------------------------

    /// El jugador (o un pueblo) terminó un refugio en (x, z).
    pub fn registrar_refugio(&mut self, x: f32, z: f32, capacidad: u16, comida: f32) -> u32 {
        let id = self.sig_refugio;
        self.sig_refugio += 1;
        let pueblo = self.cercano(x, z, 0).map(|c| c.0).unwrap_or(0);
        self.refugios.push(Refugio {
            id,
            x,
            z,
            pueblo,
            capacidad,
            comida: comida.max(0.0),
            gente: Vec::new(),
            creado: self.dia,
            creencias: Creencias::default(),
        });
        let lugar = self.nombre_pueblo(pueblo);
        self.hecho(Tipo::Refugio, pueblo, 0, format!("Se termina un refugio bajo tierra cerca de {lugar}."));
        id
    }

    pub fn abastecer_refugio(&mut self, refugio: u32, comida: f32) {
        if let Some(r) = self.refugios.iter_mut().find(|r| r.id == refugio) {
            r.comida += comida.max(0.0);
        }
    }

    /// Mete a una persona al refugio (el jugador la invitó y ella aceptó).
    pub fn refugiar(&mut self, persona: u32, refugio: u32) -> Result<(), &'static str> {
        let Some(p) = self.personas.get(&persona) else { return Err("Esa persona ya no está.") };
        if p.refugio != 0 {
            return Err("Ya está a resguardo.");
        }
        let (pueblo_p, saberes) = (p.pueblo, p.saberes);
        let mejor = self.pueblos.get(&pueblo_p).map(|q| q.creencias);
        let Some(r) = self.refugios.iter_mut().find(|r| r.id == refugio) else { return Err("Ese refugio no existe.") };
        if r.gente.len() >= r.capacidad as usize {
            return Err("El refugio está lleno.");
        }
        r.gente.push(persona);
        if let Some(c) = mejor {
            for k in 0..N_LEYES {
                if c.mag(k) < r.creencias.mag(k) {
                    r.creencias.err[k] = c.err[k];
                }
            }
        }
        let _ = saberes;
        self.personas.get_mut(&persona).unwrap().refugio = refugio;
        Ok(())
    }

    pub fn sacar_del_refugio(&mut self, persona: u32) {
        let Some(p) = self.personas.get_mut(&persona) else { return };
        let r = std::mem::take(&mut p.refugio);
        if let Some(r) = self.refugios.iter_mut().find(|x| x.id == r) {
            r.gente.retain(|x| *x != persona);
        }
    }

    /// Los refugiados salen y fundan un pueblo nuevo en (x, z). Falla si afuera
    /// todavía no se puede vivir.
    pub fn fundar_desde_refugio(&mut self, refugio: u32, nombre: Option<String>, x: f32, z: f32) -> Result<u32, &'static str> {
        let Some(r) = self.refugios.iter().find(|r| r.id == refugio).cloned() else { return Err("Ese refugio no existe.") };
        let gente: Vec<u32> = r.gente.iter().copied().filter(|g| self.personas.contains_key(g)).collect();
        if gente.is_empty() {
            return Err("No hay nadie en el refugio.");
        }
        if self.radiacion_en(x, z) > 0.10 {
            return Err("Afuera todavía hay demasiada radiación.");
        }
        let id = self.sig_pueblo;
        self.sig_pueblo += 1;
        let h = hash(self.semilla ^ (id as u64 + 1).wrapping_mul(0x9E37) ^ self.dia);
        let vecino_sustrato = self.cercano(x, z, 0).and_then(|c| self.pueblos.get(&c.0)).map(|p| p.sustrato).unwrap_or(0);
        let mut saberes = Saberes::NADA;
        for g in &gente {
            saberes = saberes.union(self.personas[g].saberes);
        }
        let culto = self.personas[&gente[0]].culto;
        let mut p = Pueblo::vacio(id);
        p.nombre = nombre.filter(|n| !n.trim().is_empty()).unwrap_or_else(|| format!("Nueva {}", nombres::pueblo(h)));
        p.x = x;
        p.z = z;
        p.rango = 0;
        p.sustrato = vecino_sustrato | S_PIEDRA | S_MADERA | S_AGUA | S_FERTIL | S_CIELO;
        p.carga = 190.0;
        p.pob = gente.len() as f32 + 4.0;
        p.comida = r.comida + p.pob * 8.0;
        p.tesoro = 40.0;
        p.saberes = saberes;
        p.archivo = saberes;
        p.creencias = r.creencias;
        p.culto = culto;
        p.conv = Convencion::nueva([0.20, 0.15, 0.10, 0.45, 0.10]);
        p.legit = 0.8;
        p.moral = 0.75;
        p.fundado = self.dia;
        self.pueblos.insert(id, p);
        for g in &gente {
            self.mudar(*g, id);
            self.personas.get_mut(g).unwrap().refugio = 0;
        }
        self.refugios.retain(|q| q.id != refugio);
        if gente.contains(&JUGADOR_ID) {
            self.jugador.pueblo = Some(id);
        }
        self.armar_vecinos();
        self.recalcular_saberes(id);
        self.elegir_gobernante(id, true);
        let nombre = self.pueblos[&id].nombre.clone();
        self.hecho(
            Tipo::Refundacion,
            id,
            r.pueblo,
            format!("{} sobrevivientes salen del refugio y fundan {nombre}: empieza otra civilización.", gente.len()),
        );
        Ok(id)
    }

    // --- la bomba ----------------------------------------------------------------------

    pub(crate) fn accidente_nuclear(&mut self, id: u32) {
        let gente = self.pueblos[&id].gente.clone();
        {
            let p = self.pueblos.get_mut(&id).unwrap();
            p.pob *= 0.88;
            p.radiacion = (p.radiacion + 0.35).min(1.0);
            p.moral = (p.moral - 0.2).max(0.0);
        }
        for g in gente {
            if g != JUGADOR_ID && self.personas.get(&g).map(|p| p.refugio == 0).unwrap_or(false) && self.rng.p(0.15) {
                self.matar(g, "en un accidente nuclear");
            }
        }
        self.aprender_de_falla(id, &[FISION]);
        let n = self.nombre_pueblo(id);
        self.hecho(Tipo::Tragedia, id, 0, format!("El reactor de {n} se descontrola: hay que evacuar."));
    }

    /// Una ojiva de `atac` sobre `blanco`.
    pub fn detonar(&mut self, atac: u32, blanco: u32) {
        let dia = self.dia;
        if !self.pueblos.contains_key(&blanco) || self.pueblos.get(&atac).map(|a| a.arsenal == 0).unwrap_or(true) {
            return;
        }
        let mag = self.pueblos[&atac].creencias.mag(FISION);
        let p_falla = (mag * 1.3 - 0.06).clamp(0.0, 0.85);
        self.pueblos.get_mut(&atac).unwrap().arsenal -= 1;
        let (na, nb) = (self.nombre_pueblo(atac), self.nombre_pueblo(blanco));
        if self.rng.p(p_falla) {
            let b = self.pueblos.get_mut(&blanco).unwrap();
            b.radiacion = (b.radiacion + 0.12).min(1.0);
            b.pob *= 0.97;
            self.aprender_de_falla(atac, &[FISION]);
            self.hecho(Tipo::Tragedia, atac, blanco, format!("La bomba de {na} no detona como debía sobre {nb}: los cálculos estaban mal."));
            return;
        }
        let gente = self.pueblos[&blanco].gente.clone();
        let muertes = {
            let b = self.pueblos.get_mut(&blanco).unwrap();
            let prot = 0.2 * b.refugio.min(3) as f32;
            let kill = 0.62 * (1.0 - prot);
            b.pob *= 1.0 - kill;
            b.radiacion = (b.radiacion + 0.7).min(1.0);
            b.comida *= 0.4;
            b.muralla = 0;
            b.proyecto = None;
            b.moral = (b.moral - 0.3).max(0.0);
            b.legit = (b.legit - 0.25).max(0.0);
            b.orden = (b.orden - 0.3).max(0.0);
            b.golpeado = dia;
            b.golpeado_por = atac;
            kill
        };
        if self.rng.p(0.5) {
            let b = self.pueblos.get_mut(&blanco).unwrap();
            b.archivo = Saberes::NADA;
            b.biblioteca = 0;
        }
        for g in gente {
            if g != JUGADOR_ID && self.personas.get(&g).map(|p| p.refugio == 0).unwrap_or(false) && self.rng.p(muertes) {
                self.matar(g, "en la explosión");
            }
        }
        let (bx, bz, bc) = {
            let b = &self.pueblos[&blanco];
            (b.x, b.z, b.cuerpo)
        };
        let ids: Vec<u32> = self.pueblos.keys().copied().collect();
        for o in ids {
            if o == blanco {
                continue;
            }
            let q = self.pueblos.get_mut(&o).unwrap();
            if q.cuerpo != bc {
                continue;
            }
            let d = ((q.x - bx).powi(2) + (q.z - bz).powi(2)).sqrt();
            q.radiacion = (q.radiacion + 0.25 * (1.0 - d / 4000.0).max(0.0)).min(1.0);
        }
        if bc == 0 {
            self.invierno = (self.invierno + 0.03).min(1.0);
        }
        self.detonaciones += 1;
        self.ultima_bomba = dia;
        self.relacion_mut(atac, blanco).actitud = -1.0;
        if let Some(gi) = self.guerra_de(atac) {
            let lado_a = self.guerras[gi].bando_a.contains(&atac);
            let gm = &mut self.guerras[gi];
            gm.avance = (gm.avance + if lado_a { 0.25 } else { -0.25 }).clamp(-1.2, 1.2);
        }
        self.hecho(Tipo::Detonacion, atac, blanco, format!("{na} lanza un arma nuclear sobre {nb}."));
    }

    /// ¿Alguien de esta guerra aprieta el botón? Cada 5 días, por guerra.
    pub(crate) fn uso_nuclear(&mut self, idx: usize) {
        if idx >= self.guerras.len() {
            return;
        }
        let g = self.guerras[idx].clone();
        let dia = self.dia;
        for lado in 0..2 {
            let (propio, rival, signo) = if lado == 0 { (&g.bando_a, &g.bando_b, 1.0f32) } else { (&g.bando_b, &g.bando_a, -1.0) };
            for &p in propio.iter() {
                let Some(pu) = self.pueblos.get(&p) else { continue };
                if !pu.vivo || pu.arsenal == 0 {
                    continue;
                }
                let vivo = |m: &Mundo, r: &u32| m.pueblos.get(r).map(|q| q.vivo && q.pob > 1.0).unwrap_or(false);
                let represalia = pu.golpeado + 15 >= dia && pu.golpeado > 0 && rival.contains(&pu.golpeado_por) && vivo(self, &pu.golpeado_por);
                let objetivo = if represalia { Some(pu.golpeado_por) } else { rival.iter().copied().find(|r| vivo(self, r)) };
                let Some(objetivo) = objetivo else { continue };
                if self.distancia(p, objetivo) > Self::alcance_tiles(pu.capacidad().alcance()) {
                    continue;
                }
                let Some(gob) = pu.gobernante.and_then(|x| self.personas.get(&x)) else { continue };
                let desespero = (-signo * self.guerras[idx].avance).max(0.0) + 0.4 * pu.hambre;
                let agresion = (gob.audacia + gob.ambicion) * 0.5 - 0.6 * gob.bondad;
                let enemigo_armado = self.pueblos[&objetivo].arsenal > 0;
                let tabu = if self.ultima_bomba == 0 { 1.0 } else { ((dia - self.ultima_bomba) as f32 / (DIAS_ANIO * 40) as f32).clamp(0.03, 1.0) };
                let mut prob = (0.0015 + 0.03 * desespero + 0.012 * agresion.max(0.0)) * tabu;
                if enemigo_armado && desespero < 0.5 {
                    prob *= 0.25;
                }
                if represalia {
                    prob = 0.65;
                }
                if !self.rng.p(prob) {
                    continue;
                }
                let n = (1 + self.rng.idx(3)).min(self.pueblos[&p].arsenal as usize);
                for _ in 0..n {
                    self.detonar(p, objetivo);
                }
            }
        }
    }

    // --- el día después ----------------------------------------------------------------

    pub(crate) fn catastrofe_dia(&mut self) {
        let invierno = self.invierno;
        let ids: Vec<u32> = self.pueblos.keys().copied().collect();
        for id in ids {
            let (vivo, rad, refugio) = {
                let p = &self.pueblos[&id];
                (p.vivo, p.radiacion, p.refugio.min(3) as f32)
            };
            if rad > 0.0 {
                let p = self.pueblos.get_mut(&id).unwrap();
                p.radiacion *= 0.985;
                if p.radiacion < 0.002 {
                    p.radiacion = 0.0;
                }
            }
            if !vivo {
                continue;
            }
            if rad > 0.02 {
                let prot = 1.0 - 0.25 * refugio;
                self.pueblos.get_mut(&id).unwrap().pob *= 1.0 - 0.012 * rad * prot;
                if rad > 0.05 {
                    let gente = self.pueblos[&id].gente.clone();
                    for g in gente {
                        if g != JUGADOR_ID && self.personas.get(&g).map(|p| p.refugio == 0).unwrap_or(false) && self.rng.p(0.006 * rad * prot) {
                            self.matar(g, "por la radiación");
                        }
                    }
                }
            }
            if self.pueblos[&id].pob < 5.0 && (rad > 0.02 || invierno > 0.05 || self.pueblos[&id].pob <= 0.0) {
                self.extinguir(id);
            }
        }
        self.invierno *= 0.9955;
        if self.invierno < 0.002 {
            self.invierno = 0.0;
        }
        self.refugio_dia();
    }

    fn refugio_dia(&mut self) {
        for i in 0..self.refugios.len() {
            let n = self.refugios[i].gente.len();
            if n == 0 {
                continue;
            }
            let r = &mut self.refugios[i];
            r.comida = (r.comida - 0.4 * n as f32).max(0.0);
            if r.comida > 0.0 {
                continue;
            }
            for g in self.refugios[i].gente.clone() {
                if self.rng.p(0.04) {
                    self.matar(g, "de hambre en el refugio");
                }
            }
        }
    }

    /// Nadie queda: el pueblo se vuelve ruinas. Sólo escapan los que estaban a resguardo
    /// (y el jugador, que nunca muere por las reglas de la civilización).
    pub(crate) fn extinguir(&mut self, id: u32) {
        let gente = self.pueblos[&id].gente.clone();
        self.pueblos.get_mut(&id).unwrap().gobernante = None;
        if self.jugador.pueblo == Some(id) {
            self.jugador.pueblo = None;
        }
        for g in gente {
            let salvo = g == JUGADOR_ID || self.personas.get(&g).map(|p| p.refugio != 0).unwrap_or(false);
            if salvo {
                if let Some(p) = self.personas.get_mut(&g) {
                    p.pueblo = 0;
                }
                self.pueblos.get_mut(&id).unwrap().gente.retain(|x| *x != g);
            } else {
                self.matar(g, "por la catástrofe");
            }
        }
        let nombre = {
            let p = self.pueblos.get_mut(&id).unwrap();
            p.vivo = false;
            p.pob = 0.0;
            p.proyecto = None;
            p.arsenal = 0;
            p.nombre.clone()
        };
        self.retirar_de_guerras(id);
        self.hecho(Tipo::Extincion, id, 0, format!("{nombre} queda desierta: nadie sobrevivió."));
    }

    fn retirar_de_guerras(&mut self, id: u32) {
        for g in self.guerras.iter_mut() {
            g.bando_a.retain(|x| *x != id);
            g.bando_b.retain(|x| *x != id);
        }
        self.guerras.retain(|g| !g.bando_a.is_empty() && !g.bando_b.is_empty());
    }

    pub(crate) fn catastrofe_anual(&mut self) {
        let ids: Vec<u32> = self.pueblos.keys().copied().collect();
        // armarse (o desarmarse)
        for id in &ids {
            let (vivo, cuerpo, tiene, gob, arsenal, mat, tesoro) = {
                let p = &self.pueblos[id];
                (p.vivo, p.cuerpo, p.saberes.tiene(ARMAS_NUCLEARES), p.gobernante, p.arsenal, p.mat, p.tesoro)
            };
            if !vivo || cuerpo != 0 || !tiene {
                continue;
            }
            let Some(g) = gob.and_then(|x| self.personas.get(&x)).cloned() else { continue };
            let miedo = self.pueblos[id].vecinos.iter().any(|v| self.pueblos.get(v).map(|q| q.arsenal > 0).unwrap_or(false)) as u8 as f32
                + if self.guerra_de(*id).is_some() { 0.5 } else { 0.0 };
            let uranio = self.sustrato_accesible(*id) & S_URANIO != 0;
            let p_arma = (0.10 + 0.35 * miedo + 0.25 * g.ambicion - 0.2 * g.bondad).clamp(0.0, 0.9);
            if uranio && mat >= 600.0 && tesoro > 50.0 && self.rng.p(p_arma) {
                let n = (1 + (mat / 3000.0) as usize).min((mat / 600.0) as usize).clamp(1, 3);
                let primera = arsenal == 0;
                let p = self.pueblos.get_mut(id).unwrap();
                p.arsenal = (p.arsenal + n as u16).min(TOPE_ARSENAL);
                p.mat -= 600.0 * n as f32;
                p.tesoro -= 60.0 * n as f32;
                if primera {
                    let nombre = p.nombre.clone();
                    self.hecho(Tipo::Arsenal, *id, 0, format!("{nombre} prueba su primera arma nuclear."));
                }
            } else if arsenal > 0 && g.bondad > 0.65 && miedo == 0.0 && self.rng.p(0.08) {
                self.pueblos.get_mut(id).unwrap().arsenal = arsenal / 2;
                let nombre = self.nombre_pueblo(*id);
                self.hecho(Tipo::Arsenal, *id, 0, format!("{nombre} desmantela la mitad de su arsenal."));
            }
        }
        self.repoblar_ruinas();
    }

    /// Los vecinos que quedaron en pie vuelven a poblar las ruinas cuando ya se puede.
    fn repoblar_ruinas(&mut self) {
        if self.invierno > 0.3 {
            return;
        }
        let muertos: Vec<u32> = self.pueblos.values().filter(|p| !p.vivo && p.cuerpo == 0 && p.radiacion < 0.05 && p.fundado + 1 < self.dia).map(|p| p.id).collect();
        for d in muertos {
            // un pueblo que no fue arrasado sino abandonado por otra causa no se repuebla aquí
            if self.pueblos[&d].golpeado == 0 && self.pueblos[&d].pob > 0.0 {
                continue;
            }
            if !self.rng.p(0.12) {
                continue;
            }
            let (dx, dz) = (self.pueblos[&d].x, self.pueblos[&d].z);
            let mut mejor: Option<(f32, u32)> = None;
            for q in self.pueblos.values() {
                if !q.vivo || q.cuerpo != 0 || q.pob < 80.0 || q.hambre > 0.2 || q.radiacion > 0.05 {
                    continue;
                }
                let dist = ((q.x - dx).powi(2) + (q.z - dz).powi(2)).sqrt();
                if dist <= Self::alcance_tiles(q.capacidad().alcance()) && mejor.map(|m| dist < m.0).unwrap_or(true) {
                    mejor = Some((dist, q.id));
                }
            }
            let Some((_, src)) = mejor else { continue };
            let mov = (self.pueblos[&src].pob * 0.12).clamp(20.0, 60.0);
            let (saberes, creencias, culto, nombre_src) = {
                let s = self.pueblos.get_mut(&src).unwrap();
                s.pob -= mov;
                (s.saberes, s.creencias, s.culto, s.nombre.clone())
            };
            {
                let p = self.pueblos.get_mut(&d).unwrap();
                p.vivo = true;
                p.pob = mov;
                p.comida = mov * 6.0;
                p.mat = 0.0;
                p.tesoro = mov;
                p.saberes = saberes;
                p.archivo = Saberes::NADA;
                p.creencias = creencias;
                p.culto = culto;
                p.biblioteca = 0;
                p.escuela = 0;
                p.muralla = 0;
                p.refugio = 0;
                p.hambre = 0.0;
                p.moral = 0.7;
                p.legit = 0.7;
                p.senor = Some(src);
                p.fundado = self.dia;
                p.golpeado = 0;
                p.conv = Convencion::nueva([0.25, 0.25, 0.1, 0.3, 0.1]);
                if !p.nombre.starts_with("Nueva ") {
                    p.nombre = format!("Nueva {}", p.nombre);
                }
            }
            for k in 0..objetivo_notables(mov).min(8) {
                let h = self.rng.u64() ^ k as u64;
                let edad = 18.0 + 25.0 * self.rng.f();
                let nuevo = self.crear_persona(d, edad, None, None, h);
                let mut sab = Saberes::NADA;
                for s in saberes.iter() {
                    if DEFS[s as usize].costo <= 90.0 {
                        sab = sab.con(s);
                    }
                }
                self.personas.get_mut(&nuevo).unwrap().saberes = sab;
            }
            self.armar_vecinos();
            self.recalcular_saberes(d);
            self.elegir_gobernante(d, true);
            let nombre = self.pueblos[&d].nombre.clone();
            self.hecho(Tipo::Refundacion, d, src, format!("Gente de {nombre_src} vuelve a poblar las ruinas: nace {nombre}."));
        }
    }
}
