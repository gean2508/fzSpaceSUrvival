//! El MUNDO: pueblos, personas, relaciones y la crónica. Aquí vive el estado y el
//! paso de un día; cada sistema (ciencia, gobierno, guerra...) es un `impl Mundo`
//! en su archivo.

use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap};

use super::azar::{hash, unidad, Azar};
use super::cronica::{Cronica, Tipo};
use super::persona::{Estudio, Oficio, Persona};
use super::pueblo::*;
use super::saber::*;
use super::{nombres, DIAS_ANIO};

/// Un sitio donde sembrar un pueblo (lo entrega la geografía del juego).
#[derive(Clone, Debug)]
pub struct Sitio {
    pub x: f32,
    pub z: f32,
    pub rango: u8,
    pub nombre: Option<String>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Jugador {
    /// Prestigio por pueblo: lo que la gente de ahí piensa de vos.
    pub prestigio: BTreeMap<u32, f32>,
    pub candidato: Option<u32>,
    pub pueblo: Option<u32>,
}

pub const JUGADOR_ID: u32 = 1;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Mundo {
    pub semilla: u64,
    pub dia: u64,
    pub rng: Azar,
    pub pueblos: BTreeMap<u32, Pueblo>,
    pub personas: BTreeMap<u32, Persona>,
    pub relaciones: Vec<Relacion>,
    #[serde(skip)]
    pub idx_rel: HashMap<(u32, u32), usize>,
    pub guerras: Vec<Guerra>,
    pub artefactos: Vec<Artefacto>,
    pub expediciones: Vec<Expedicion>,
    pub cultos: Vec<Culto>,
    pub cronica: Cronica,
    pub jugador: Jugador,
    pub sig_persona: u32,
    pub sig_pueblo: u32,
    pub sig_guerra: u32,
    pub sig_artefacto: u32,
    pub sig_expedicion: u32,
    pub saberes_mundo: Saberes,
    /// ¿Aparecen ruinas y piezas de tecnología perdida?
    pub hallazgos: bool,
    #[serde(default)]
    pub bajas: Vec<super::jugador::Baja>,
    /// Mejor precisión alcanzada por alguien en el mundo, por ley (nivel de `fisica::NIVELES`).
    #[serde(default)]
    pub hitos_fisica: [u8; super::fisica::N_LEYES],
    /// 0..1: invierno nuclear (humo en la atmósfera) que enfría y achica las cosechas.
    #[serde(default)]
    pub invierno: f32,
    #[serde(default)]
    pub detonaciones: u32,
    /// Día de la última bomba: la memoria del horror enfría el dedo en el botón.
    #[serde(default)]
    pub ultima_bomba: u64,
    #[serde(default)]
    pub refugios: Vec<super::catastrofe::Refugio>,
    #[serde(default)]
    pub sig_refugio: u32,
}

impl Mundo {
    pub fn vacio(semilla: u64) -> Self {
        Self {
            semilla,
            dia: 0,
            rng: Azar::nuevo(semilla),
            pueblos: BTreeMap::new(),
            personas: BTreeMap::new(),
            relaciones: Vec::new(),
            idx_rel: HashMap::new(),
            guerras: Vec::new(),
            artefactos: Vec::new(),
            expediciones: Vec::new(),
            cultos: Vec::new(),
            cronica: Cronica::default(),
            jugador: Jugador::default(),
            sig_persona: JUGADOR_ID + 1,
            sig_pueblo: 1,
            sig_guerra: 1,
            sig_artefacto: 1,
            sig_expedicion: 1,
            saberes_mundo: Saberes::NADA,
            hallazgos: true,
            bajas: Vec::new(),
            hitos_fisica: [0; super::fisica::N_LEYES],
            invierno: 0.0,
            detonaciones: 0,
            ultima_bomba: 0,
            refugios: Vec::new(),
            sig_refugio: 1,
        }
    }

    /// Siembra el mundo. Nadie sabe nada de "la historia": sólo qué gente hay en cada
    /// sitio, qué sabe y qué tiene a mano.
    pub fn sembrar(semilla: u64, sitios: &[Sitio]) -> Self {
        let mut m = Self::vacio(semilla);
        m.cultos = vec![
            Culto { id: 0, nombre: "Culto del Sol".into(), fundador: "los antiguos".into(), dia: 0, severidad: 0.0 },
            Culto { id: 1, nombre: "Culto de la Tierra".into(), fundador: "los antiguos".into(), dia: 0, severidad: -0.3 },
            Culto { id: 2, nombre: "Culto de las Aguas".into(), fundador: "los antiguos".into(), dia: 0, severidad: 0.2 },
        ];
        for (i, s) in sitios.iter().enumerate() {
            let h = hash(semilla ^ (i as u64 + 1).wrapping_mul(0x9E37_79B9));
            m.sembrar_pueblo(s, h);
        }
        m.reindexar();
        m.armar_vecinos();
        for id in m.pueblos.keys().copied().collect::<Vec<_>>() {
            m.recalcular_saberes(id);
            m.elegir_gobernante(id, false);
        }
        m.saberes_mundo = m.pueblos.values().fold(Saberes::NADA, |a, p| a.union(p.saberes));
        m
    }

    fn sembrar_pueblo(&mut self, s: &Sitio, h: u64) {
        let id = self.sig_pueblo;
        self.sig_pueblo += 1;
        let mut p = Pueblo::vacio(id);
        p.nombre = s.nombre.clone().unwrap_or_else(|| nombres::pueblo(h));
        p.x = s.x;
        p.z = s.z;
        p.rango = s.rango;
        let mut sust = 0u32;
        for (k, (bit, prob)) in SUSTRATOS.iter().enumerate() {
            if unidad(h ^ (k as u64 + 7) * 0x51ED) < *prob {
                sust |= bit;
            }
        }
        sust |= S_AGUA;
        p.sustrato = sust;
        let (carga, ini) = match s.rango {
            0 => (170.0 + 120.0 * unidad(h ^ 1), 0.5),
            1 => (700.0 + 500.0 * unidad(h ^ 2), 0.6),
            _ => (3200.0 + 2400.0 * unidad(h ^ 3), 0.7),
        };
        p.carga = carga;
        p.saberes = saberes_iniciales(s.rango, h);
        p.creencias = super::fisica::Creencias::inicial(p.saberes, h);
        p.pob = (super::economia::carga_efectiva(&p) * (ini + 0.25)).max(30.0);
        p.comida = p.pob * 6.0;
        p.tesoro = p.pob * 2.0;
        p.culto = (hash(h ^ 9) % 3) as u8;
        p.moral = 0.55 + 0.2 * unidad(h ^ 4);
        p.legit = 0.55 + 0.3 * unidad(h ^ 5);
        p.orden = 0.5 + 0.3 * unidad(h ^ 6);
        // Convención de partida: las aldeas son de linaje y fuerza; las ciudades
        // ya conocen el voto y el dinero (si su derecho / moneda existe).
        let mut w = [0.3 + 0.2 * unidad(h ^ 11), 0.3 + 0.2 * unidad(h ^ 12), 0.1 + 0.1 * unidad(h ^ 13), 0.08, 0.06];
        if p.saberes.tiene(DERECHO) {
            w[Convencion::VOTO] += 0.12 * unidad(h ^ 14) + 0.06 * s.rango as f32;
        }
        if p.saberes.tiene(MONEDA) {
            w[Convencion::RIQUEZA] += 0.14 * unidad(h ^ 15) + 0.05 * s.rango as f32;
        }
        p.conv = Convencion::nueva(w);
        p.casa_reinante = (hash(h ^ 21) % 1_000_000) as u32;
        if s.rango >= 1 {
            p.biblioteca = 1;
            p.escuela = 1;
            p.archivo = p.saberes;
        }
        if s.rango >= 2 {
            p.muralla = 1;
        }
        self.pueblos.insert(id, p);
        let n = ((self.pueblos[&id].pob / 45.0) as usize).clamp(6, 32);
        self.poblar(id, n, h);
    }

    /// Siembra `n` notables. Los saberes avanzados quedan en manos de unos pocos.
    fn poblar(&mut self, pueblo: u32, n: usize, h: u64) {
        let saberes_p = self.pueblos[&pueblo].saberes;
        let umbral = 90.0 + 160.0 * self.pueblos[&pueblo].escuela as f32;
        let mut ids = Vec::new();
        let casa_r = self.pueblos[&pueblo].casa_reinante;
        for k in 0..n {
            let hh = hash(h ^ (k as u64 + 100).wrapping_mul(0xA24B));
            let edad = 17.0 + 45.0 * unidad(hh ^ 1);
            let id = self.crear_persona(pueblo, edad, None, None, hh);
            let p = self.personas.get_mut(&id).unwrap();
            if k == 0 {
                p.casa = casa_r;
            }
            let mut sab = Saberes::NADA;
            for s in saberes_p.iter() {
                if DEFS[s as usize].costo <= umbral {
                    sab = sab.con(s);
                }
            }
            p.saberes = sab;
            ids.push(id);
        }
        // Los saberes por encima del umbral: 1 a 3 portadores cada uno.
        for s in saberes_p.iter() {
            if DEFS[s as usize].costo > umbral {
                let portadores = 1 + (hash(h ^ s as u64) % 3) as usize;
                for j in 0..portadores {
                    let who = ids[(hash(h ^ (s as u64) << 8 ^ j as u64) % ids.len() as u64) as usize];
                    let p = self.personas.get_mut(&who).unwrap();
                    p.saberes = p.saberes.con(s);
                }
            }
        }
        // Los portadores deben tener también los prerrequisitos (coherencia mental).
        for id in &ids {
            let p = self.personas.get_mut(id).unwrap();
            let mut cambio = true;
            while cambio {
                cambio = false;
                for s in p.saberes.iter().collect::<Vec<_>>() {
                    for pre in DEFS[s as usize].pre {
                        if !p.saberes.tiene(*pre) {
                            p.saberes = p.saberes.con(*pre);
                            cambio = true;
                        }
                    }
                }
            }
            if p.saberes.cuenta() > 12 {
                p.oficio = Oficio::Erudito;
            }
        }
    }

    pub fn crear_persona(&mut self, pueblo: u32, edad: f32, padre: Option<u32>, madre: Option<u32>, h: u64) -> u32 {
        let id = self.sig_persona;
        self.sig_persona += 1;
        let mujer = unidad(h ^ 0x51) < 0.5;
        let mut rasgo = |k: u64, base: f32, pa: Option<f32>, ma: Option<f32>| -> f32 {
            let azar = unidad(h ^ k.wrapping_mul(0x9E37)) * 2.0 - 1.0;
            let herencia = match (pa, ma) {
                (Some(a), Some(b)) => (a + b) * 0.5,
                (Some(a), None) | (None, Some(a)) => a,
                _ => base,
            };
            (herencia * 0.6 + (0.5 + 0.3 * azar) * 0.4 + 0.15 * azar).clamp(0.02, 0.98)
        };
        let pp = padre.and_then(|i| self.personas.get(&i)).cloned();
        let mm = madre.and_then(|i| self.personas.get(&i)).cloned();
        macro_rules! r {
            ($k:expr, $f:ident) => {
                rasgo($k, 0.5, pp.as_ref().map(|x| x.$f), mm.as_ref().map(|x| x.$f))
            };
        }
        let mut p = Persona {
            id,
            nombre: nombres::persona(mujer, h),
            casa: pp.as_ref().map(|x| x.casa).or(mm.as_ref().map(|x| x.casa)).unwrap_or((hash(h ^ 0x77) % 1_000_000) as u32),
            pueblo,
            nacio: self.dia as i64 - (edad * DIAS_ANIO as f32) as i64,
            mujer,
            inteligencia: r!(1, inteligencia),
            curiosidad: r!(2, curiosidad),
            ambicion: r!(3, ambicion),
            carisma: r!(4, carisma),
            fuerza: r!(5, fuerza),
            bondad: r!(6, bondad),
            fe: r!(7, fe),
            codicia: r!(8, codicia),
            audacia: r!(9, audacia),
            padre,
            madre,
            culto: self.pueblos.get(&pueblo).map(|x| x.culto).unwrap_or(0),
            riqueza: 5.0 * unidad(h ^ 0x33),
            ..Default::default()
        };
        p.oficio = oficio_para(&p, h);
        if let Some(pu) = self.pueblos.get_mut(&pueblo) {
            pu.gente.push(id);
        }
        self.personas.insert(id, p);
        id
    }

    pub fn hecho(&mut self, tipo: Tipo, pueblo: u32, otro: u32, texto: String) {
        self.cronica.anotar(self.dia, tipo, pueblo, otro, texto);
    }

    pub fn nombre_pueblo(&self, id: u32) -> String {
        self.pueblos.get(&id).map(|p| p.nombre.clone()).unwrap_or_else(|| "un lugar olvidado".into())
    }

    pub fn nombre_persona(&self, id: u32) -> String {
        self.personas.get(&id).map(|p| p.nombre.clone()).unwrap_or_else(|| "alguien".into())
    }

    // --- relaciones -----------------------------------------------------------

    pub fn reindexar(&mut self) {
        for p in self.pueblos.values_mut() {
            p.sin_portador.resize(N_SABERES, 0);
        }
        self.idx_rel.clear();
        for (i, r) in self.relaciones.iter().enumerate() {
            self.idx_rel.insert((r.a, r.b), i);
        }
    }

    pub fn relacion(&self, a: u32, b: u32) -> Option<&Relacion> {
        let k = if a < b { (a, b) } else { (b, a) };
        self.idx_rel.get(&k).map(|i| &self.relaciones[*i])
    }

    pub fn relacion_mut(&mut self, a: u32, b: u32) -> &mut Relacion {
        let k = if a < b { (a, b) } else { (b, a) };
        if let Some(i) = self.idx_rel.get(&k) {
            return &mut self.relaciones[*i];
        }
        self.relaciones.push(Relacion { a: k.0, b: k.1, actitud: 0.0, comercio: false, alianza: false });
        let i = self.relaciones.len() - 1;
        self.idx_rel.insert(k, i);
        &mut self.relaciones[i]
    }

    pub fn actitud(&self, a: u32, b: u32) -> f32 {
        self.relacion(a, b).map(|r| r.actitud).unwrap_or(0.0)
    }

    pub fn aliados(&self, a: u32, b: u32) -> bool {
        self.relacion(a, b).map(|r| r.alianza).unwrap_or(false)
    }

    /// Hasta dónde llega un pueblo por tierra/mar/aire, en tiles.
    pub fn alcance_tiles(al: Alcance) -> f32 {
        match al {
            Alcance::Tierra => 4_200.0,
            Alcance::Mar => 7_500.0,
            Alcance::Aire | Alcance::Luna | Alcance::Planetas | Alcance::Exterior => 60_000.0,
        }
    }

    /// Los 6 vecinos más cercanos del mismo cuerpo (sin tope de distancia: quién
    /// los alcanza de verdad lo decide `contactos`).
    pub fn armar_vecinos(&mut self) {
        let lista: Vec<(u32, u8, f32, f32)> =
            self.pueblos.values().filter(|p| p.vivo).map(|p| (p.id, p.cuerpo, p.x, p.z)).collect();
        for (id, cuerpo, x, z) in lista.iter().copied() {
            let mut cand: Vec<(f32, u32)> = lista
                .iter()
                .filter(|o| o.0 != id && o.1 == cuerpo)
                .map(|o| (((o.2 - x).powi(2) + (o.3 - z).powi(2)).sqrt(), o.0))
                .collect();
            cand.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap().then(a.1.cmp(&b.1)));
            cand.truncate(6);
            if let Some(p) = self.pueblos.get_mut(&id) {
                p.vecinos = cand.into_iter().map(|c| c.1).collect();
            }
        }
    }

    pub fn distancia(&self, a: u32, b: u32) -> f32 {
        match (self.pueblos.get(&a), self.pueblos.get(&b)) {
            (Some(p), Some(q)) => ((p.x - q.x).powi(2) + (p.z - q.z).powi(2)).sqrt(),
            _ => f32::MAX,
        }
    }

    /// Vecinos que `a` de verdad alcanza (mismo cuerpo, dentro de su alcance, vivos).
    pub fn contactos(&self, a: u32) -> Vec<u32> {
        let Some(p) = self.pueblos.get(&a) else { return Vec::new() };
        let reach = Self::alcance_tiles(p.capacidad().alcance());
        p.vecinos
            .iter()
            .copied()
            .filter(|b| self.pueblos.get(b).map(|q| q.vivo && q.cuerpo == p.cuerpo).unwrap_or(false))
            .filter(|b| self.distancia(a, *b) <= reach)
            .collect()
    }

    // --- personas ---------------------------------------------------------------

    /// Quita a alguien del mundo. Si gobernaba, el pueblo elige sucesor.
    pub fn matar(&mut self, id: u32, causa: &str) {
        let Some(p) = self.personas.remove(&id) else { return };
        if let Some(par) = p.pareja.and_then(|x| self.personas.get_mut(&x)) {
            par.pareja = None;
        }
        self.heredar(&p);
        if p.refugio != 0 {
            if let Some(r) = self.refugios.iter_mut().find(|r| r.id == p.refugio) {
                r.gente.retain(|x| *x != id);
            }
        }
        if p.npc != 0 {
            self.bajas.push(super::jugador::Baja { persona: id, npc: p.npc, causa: causa.to_string() });
        }
        let mut reinaba = false;
        if let Some(pu) = self.pueblos.get_mut(&p.pueblo) {
            pu.gente.retain(|x| *x != id);
            if pu.gobernante == Some(id) {
                pu.gobernante = None;
                reinaba = true;
            }
        }
        if p.fama > 3.0 || reinaba {
            let lugar = self.nombre_pueblo(p.pueblo);
            self.hecho(Tipo::Muerte, p.pueblo, 0, format!("Muere {} de {} ({}).", p.nombre, lugar, causa));
        }
        if reinaba {
            self.elegir_gobernante(p.pueblo, true);
        }
    }

    /// Recalcula lo que el pueblo sabe: lo que alguien sostiene, lo que hay en el
    /// archivo y la tradición viva. Un saber sin portador ni archivo se olvida
    /// pasado un año.
    pub fn recalcular_saberes(&mut self, id: u32) {
        let Some(pu) = self.pueblos.get(&id) else { return };
        let mut sostenido = pu.archivo;
        for g in &pu.gente {
            if let Some(p) = self.personas.get(g) {
                sostenido = sostenido.union(p.saberes);
            }
        }
        let tradicion = pu.pob >= 25.0;
        let mut nuevo = pu.saberes;
        let mut sin = pu.sin_portador.clone();
        sin.resize(N_SABERES, 0);
        let mut olvidados = Vec::new();
        for s in 0..N_SABERES as u8 {
            let en_pie = sostenido.tiene(s) || (tradicion && DEFS[s as usize].costo <= 25.0 && pu.saberes.tiene(s));
            if en_pie {
                nuevo = nuevo.con(s);
                sin[s as usize] = 0;
            } else if pu.saberes.tiene(s) {
                sin[s as usize] += 1;
                if sin[s as usize] as u64 >= DIAS_ANIO {
                    nuevo = nuevo.sin(s);
                    sin[s as usize] = 0;
                    olvidados.push(s);
                }
            }
        }
        // sin prerrequisito no hay saber que se sostenga en la práctica común
        let pu = self.pueblos.get_mut(&id).unwrap();
        pu.saberes = nuevo;
        pu.sin_portador = sin;
        let nombre = pu.nombre.clone();
        for s in olvidados {
            if DEFS[s as usize].costo >= 120.0 {
                self.hecho(Tipo::Olvido, id, 0, format!("{} olvida {}: ya nadie sabe practicarla.", nombre, DEFS[s as usize].nombre));
            }
        }
    }

    // --- paso de un día -----------------------------------------------------------

    pub fn paso_dia(&mut self) {
        self.dia += 1;
        let d = self.dia;
        self.economia_dia();
        self.catastrofe_dia();
        self.gente_dia();
        self.ciencia_dia();
        self.fisica_dia();
        self.obras_dia();
        self.expediciones_dia();
        self.justicia_dia();
        if d % 5 == 0 {
            self.guerras_5d();
        }
        if d % DIAS_ANIO == 0 {
            self.anual();
        }
        let ids: Vec<u32> = self.pueblos.keys().copied().collect();
        for id in ids {
            self.recalcular_saberes(id);
        }
    }

    fn anual(&mut self) {
        self.ciencia_anual();
        self.fisica_anual();
        self.catastrofe_anual();
        self.gente_anual();
        self.gobierno_anual();
        self.diplomacia_anual();
        self.colonias_anual();
        self.hallazgos_anual();
        self.cultos_anual();
        self.innovacion_anual();
    }

    /// Avanza `dias` días.
    pub fn avanzar(&mut self, dias: u64) {
        for _ in 0..dias {
            self.paso_dia();
        }
    }

    pub fn estudio_de(&self, id: u32) -> Option<Estudio> {
        self.personas.get(&id).and_then(|p| p.estudio)
    }
}

pub fn oficio_para(p: &Persona, h: u64) -> Oficio {
    let r = unidad(h ^ 0xFEED);
    if p.inteligencia > 0.62 && p.curiosidad > 0.55 && r < 0.75 {
        Oficio::Erudito
    } else if p.fuerza > 0.65 && p.audacia > 0.45 && r < 0.8 {
        Oficio::Soldado
    } else if p.fe > 0.7 && r < 0.8 {
        Oficio::Sacerdote
    } else if p.codicia > 0.6 && p.carisma > 0.5 && r < 0.8 {
        Oficio::Mercader
    } else if p.inteligencia > 0.5 && r < 0.4 {
        Oficio::Artesano
    } else {
        Oficio::Campesino
    }
}
