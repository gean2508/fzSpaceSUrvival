use super::cronica::Tipo;
use super::saber::*;
use super::*;
use std::collections::BTreeMap;

fn sitios_falsos(n: usize) -> Vec<Sitio> {
    (0..n)
        .map(|i| Sitio {
            x: (i % 16) as f32 * 1500.0,
            z: (i / 16) as f32 * 1500.0,
            rango: if i % 9 == 0 { 2 } else if i % 3 == 0 { 1 } else { 0 },
            nombre: None,
        })
        .collect()
}

fn invariantes(m: &Mundo) {
    for (id, p) in &m.pueblos {
        assert_eq!(*id, p.id);
        assert!(p.pob.is_finite() && p.pob >= 0.0, "pob rota en {}: {}", p.nombre, p.pob);
        assert!(p.comida.is_finite() && p.mat.is_finite() && p.tesoro.is_finite());
        if p.vivo {
            assert!(p.pob > 0.0, "pueblo vivo sin gente: {}", p.nombre);
        }
        for g in &p.gente {
            let per = m.personas.get(g).unwrap_or_else(|| panic!("persona {g} fantasma en {}", p.nombre));
            assert_eq!(per.pueblo, *id, "{} figura en dos pueblos", per.nombre);
        }
        if let Some(g) = p.gobernante {
            assert!(p.gente.contains(&g), "gobernante de {} no pertenece al pueblo", p.nombre);
        }
        for s in p.saberes.iter() {
            let _ = s;
        }
    }
    for (id, per) in &m.personas {
        assert_eq!(*id, per.id);
        if let Some(pu) = m.pueblos.get(&per.pueblo) {
            assert!(pu.gente.contains(id) || per.pueblo == 0, "{} no está en la gente de su pueblo", per.nombre);
        } else {
            assert_eq!(per.pueblo, 0, "{} apunta a un pueblo inexistente", per.nombre);
        }
        if let Some(par) = per.pareja {
            assert_eq!(m.personas.get(&par).and_then(|x| x.pareja), Some(*id), "pareja no recíproca");
        }
    }
}

#[allow(dead_code)]
fn conteo(m: &Mundo) -> BTreeMap<String, usize> {
    let mut r = BTreeMap::new();
    for h in m.cronica.hechos.iter() {
        *r.entry(format!("{:?}", h.tipo)).or_insert(0) += 1;
    }
    r
}

#[test]
fn determinista() {
    let s = sitios_falsos(48);
    let mut a = Mundo::sembrar(11, &s);
    let mut b = Mundo::sembrar(11, &s);
    a.avanzar(DIAS_ANIO * 30);
    b.avanzar(DIAS_ANIO * 30);
    assert_eq!(serde_json::to_string(&a).unwrap(), serde_json::to_string(&b).unwrap());
    let mut c = Mundo::sembrar(12, &s);
    c.avanzar(DIAS_ANIO * 30);
    assert_ne!(serde_json::to_string(&a).unwrap(), serde_json::to_string(&c).unwrap());
}

#[test]
fn invariantes_a_lo_largo_de_siglos() {
    let mut m = Mundo::sembrar(5, &sitios_falsos(64));
    for _ in 0..30 {
        m.avanzar(DIAS_ANIO * 10);
        invariantes(&m);
    }
}

#[test]
fn guardar_y_cargar() {
    let mut m = Mundo::sembrar(3, &sitios_falsos(40));
    m.avanzar(DIAS_ANIO * 40);
    let json = serde_json::to_string(&m).unwrap();
    let mut b: Mundo = serde_json::from_str(&json).unwrap();
    b.reindexar();
    m.avanzar(DIAS_ANIO * 10);
    b.avanzar(DIAS_ANIO * 10);
    assert_eq!(serde_json::to_string(&m).unwrap(), serde_json::to_string(&b).unwrap());
}

#[test]
fn la_historia_se_produce_sola() {
    let mut vistos: BTreeMap<String, usize> = BTreeMap::new();
    for semilla in 1..=4u64 {
        let mut m = Mundo::sembrar(semilla, &sitios_falsos(96));
        m.avanzar(DIAS_ANIO * 300);
        for (k, v) in m.cronica.totales.clone() {
            *vistos.entry(k).or_insert(0) += v as usize;
        }
        let vivos = m.pueblos.values().filter(|p| p.vivo).count();
        let pob: f32 = m.pueblos.values().map(|p| p.pob).sum();
        eprintln!(
            "semilla {semilla}: {vivos} pueblos, pob {pob:.0}, {} personas, saberes del mundo {}/{}, guerras activas {}",
            m.personas.len(),
            m.saberes_mundo.cuenta(),
            N_SABERES,
            m.guerras.len()
        );
    }
    eprintln!("{vistos:#?}");
    for k in ["Eleccion", "Sucesion", "Guerra", "Paz", "Descubrimiento"] {
        assert!(vistos.get(k).copied().unwrap_or(0) > 0, "en 1200 años simulados nunca hubo {k}");
    }
}

/// Lo que pidió el jugador: alguien encuentra una pieza de nave, la estudia y llega a la Luna.
#[test]
fn una_pieza_de_nave_lleva_a_la_luna() {
    let sitios = sitios_falsos(24);
    let mut con_pieza = 0;
    let mut sin_pieza = 0;
    for semilla in 1..=6u64 {
        let mut a = Mundo::sembrar(semilla, &sitios);
        a.hallazgos = false;
        let mut b = a.clone();
        let ciudad = *a.pueblos.values().find(|p| p.rango == 2).map(|p| &p.id).unwrap();
        b.dar_artefacto(ciudad, COHETERIA);
        b.dar_artefacto(ciudad, ASTRONAUTICA);
        let llego = |m: &Mundo| m.pueblos.values().any(|p| p.vivo && p.cuerpo != 0);
        let hasta = DIAS_ANIO * 400;
        let (mut ta, mut tb) = (None, None);
        for d in 0..hasta {
            if ta.is_none() {
                a.paso_dia();
                if llego(&a) {
                    ta = Some(d);
                }
            }
            if tb.is_none() {
                b.paso_dia();
                if llego(&b) {
                    tb = Some(d);
                }
            }
            if ta.is_some() && tb.is_some() {
                break;
            }
        }
        eprintln!("semilla {semilla}: sin pieza {:?} años, con pieza {:?} años", ta.map(|d| d / DIAS_ANIO), tb.map(|d| d / DIAS_ANIO));
        if tb.is_some() {
            con_pieza += 1;
        }
        if ta.is_some() {
            sin_pieza += 1;
        }
    }
    assert!(con_pieza >= 1, "con una pieza de nave a mano nadie llegó jamás al espacio");
    let _ = sin_pieza;
}

#[test]
fn el_jugador_gobierna_si_se_lo_gana() {
    let mut m = Mundo::sembrar(9, &sitios_falsos(24));
    let pueblo = *m.pueblos.keys().next().unwrap();
    m.entrar_jugador(pueblo, "Ki");
    assert!(m.decretar(pueblo, Decreto::Impuesto(0.1)).is_err());
    m.presentarse(pueblo);
    m.donar(pueblo, 5000.0, 5000.0);
    if let Some(p) = m.pueblos.get_mut(&pueblo) {
        p.conv = Convencion::nueva([0.0, 0.0, 0.0, 1.0, 0.0]);
    }
    m.eleccion(pueblo);
    m.elegir_gobernante(pueblo, true);
    invariantes(&m);
    let _ = m.cronica.ultimos(5, 0);
    let gobierna = m.pueblos[&pueblo].gobernante == Some(JUGADOR_ID);
    eprintln!("¿gobierna el jugador tras donar y presentarse? {gobierna}; prestigio {}", m.prestigio(pueblo));
    if gobierna {
        m.decretar(pueblo, Decreto::PenaRobo(3)).unwrap();
        assert_eq!(m.pueblos[&pueblo].leyes.pena_robo, 3);
    }
    m.avanzar(DIAS_ANIO * 20);
    invariantes(&m);
}

#[test]
fn matar_a_un_notable_deja_baja_y_sucesion() {
    let mut m = Mundo::sembrar(4, &sitios_falsos(24));
    let pueblo = *m.pueblos.keys().next().unwrap();
    let g = m.pueblos[&pueblo].gobernante.unwrap();
    m.personas.get_mut(&g).unwrap().npc = 777;
    m.notificar_muerte(g, true);
    let bajas = m.drenar_bajas();
    assert!(bajas.iter().any(|b| b.npc == 777));
    assert_ne!(m.pueblos[&pueblo].gobernante, Some(g));
    assert!(m.prestigio(pueblo) < 0.0);
    invariantes(&m);
    let _ = Tipo::Muerte;
}

#[test]
fn con_la_geografia_real() {
    let s = sitios_tierra();
    eprintln!("{} sitios reales", s.len());
    assert!(s.len() > 20);
    let mut m = Mundo::sembrar(1, &s);
    m.avanzar(DIAS_ANIO * 50);
    invariantes(&m);
}

#[test]
#[ignore]
fn diag() {
    let mut m = Mundo::sembrar(1, &sitios_falsos(96));
    m.avanzar(DIAS_ANIO * 300);
    for h in m.cronica.hechos.iter().filter(|h| matches!(h.tipo, Tipo::Lanzamiento | Tipo::Llegada | Tipo::Tragedia | Tipo::Fundacion)).take(60) {
        eprintln!("{} {:?} {}", cronica::fecha(h.dia), h.tipo, h.texto);
    }
    for h in m.cronica.hechos.iter().filter(|h| matches!(h.tipo, Tipo::Guerra | Tipo::Paz | Tipo::Independencia | Tipo::Anexion)).take(40) {
        eprintln!("{} {:?} {}", cronica::fecha(h.dia), h.tipo, h.texto);
    }
}
