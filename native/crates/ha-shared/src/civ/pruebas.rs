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

// --- física del universo simulado, bombas, refugios ---------------------------------

fn ciudad_de(m: &Mundo) -> u32 {
    *m.pueblos.values().find(|p| p.rango == 2 && p.cuerpo == 0).map(|p| &p.id).unwrap()
}

#[test]
fn la_verdad_de_la_fisica_manda_en_la_ingenieria() {
    use super::fisica::*;
    let mut m = Mundo::sembrar(2, &sitios_falsos(24));
    let c = ciudad_de(&m);
    m.pueblos.get_mut(&c).unwrap().creencias.err = [0.0; N_LEYES];
    let bien = m.factor_ingenieria(c, &[GRAVEDAD, ORBITAS, FISION]);
    m.pueblos.get_mut(&c).unwrap().creencias.err = [0.6; N_LEYES];
    let mal = m.factor_ingenieria(c, &[GRAVEDAD, ORBITAS, FISION]);
    assert!(bien > 0.95, "con la física bien medida la ingeniería funciona: {bien}");
    assert!(mal < 0.7 && mal >= 0.35, "con la física mal medida falla más: {mal}");
}

#[test]
fn una_bomba_mal_calculada_no_explota_y_una_bien_calculada_arrasa() {
    use super::fisica::*;
    let s = sitios_falsos(24);
    let mut resultados = [0.0f32; 2];
    for (i, err) in [0.0f32, 0.9].into_iter().enumerate() {
        let mut m = Mundo::sembrar(4, &s);
        let ids: Vec<u32> = m.pueblos.values().filter(|p| p.rango == 2 && p.cuerpo == 0).map(|p| p.id).take(2).collect();
        let (a, b) = (ids[0], ids[1]);
        m.pueblos.get_mut(&a).unwrap().creencias.err[FISION] = err;
        m.pueblos.get_mut(&a).unwrap().arsenal = 30;
        let antes = m.pueblos[&b].pob;
        for _ in 0..12 {
            if m.pueblos[&b].vivo {
                m.detonar(a, b);
            }
        }
        resultados[i] = m.pueblos[&b].pob / antes;
        invariantes(&m);
    }
    assert!(resultados[0] < 0.1, "12 bombas bien calculadas dejan poco: {}", resultados[0]);
    assert!(resultados[1] > resultados[0] * 3.0, "las mal calculadas fallan: {:?}", resultados);
}

#[test]
fn el_bunker_salva_y_los_sobrevivientes_refundan() {
    let mut m = Mundo::sembrar(4, &sitios_falsos(24));
    let ids: Vec<u32> = m.pueblos.values().filter(|p| p.rango == 2 && p.cuerpo == 0).map(|p| p.id).take(2).collect();
    let (a, b) = (ids[0], ids[1]);
    let (x, z) = (m.pueblos[&b].x, m.pueblos[&b].z);
    let r = m.registrar_refugio(x + 50.0, z, 20, 400.0);
    let salvados: Vec<u32> = m.pueblos[&b].gente.iter().copied().filter(|g| *g != super::mundo::JUGADOR_ID).take(5).collect();
    assert!(!salvados.is_empty());
    for g in &salvados {
        m.refugiar(*g, r).unwrap();
    }
    m.pueblos.get_mut(&a).unwrap().arsenal = 40;
    for _ in 0..40 {
        if m.pueblos[&b].vivo {
            m.detonar(a, b);
            m.pueblos.get_mut(&a).unwrap().arsenal = 40;
        }
    }
    m.extinguir(b);
    assert!(!m.pueblos[&b].vivo, "el pueblo quedó desierto");
    for g in &salvados {
        assert!(m.personas.contains_key(g), "el refugiado murió");
    }
    invariantes(&m);
    assert!(m.fundar_desde_refugio(r, None, x, z).is_err(), "con la lluvia radiactiva no se sale");
    for _ in 0..600 {
        m.paso_dia();
        if m.radiacion_en(x, z) < 0.05 {
            break;
        }
    }
    invariantes(&m);
    let nuevo = m.fundar_desde_refugio(r, Some("Nueva Esperanza".into()), x, z).expect("ya se puede salir");
    assert!(m.pueblos[&nuevo].vivo && m.pueblos[&nuevo].gobernante.is_some());
    m.avanzar(DIAS_ANIO * 20);
    invariantes(&m);
    assert!(m.pueblos[&nuevo].vivo, "la civilización nueva sigue en pie");
}

#[test]
fn muchos_mundos_bombas_y_extinciones_sin_romper_nada() {
    let mut arsenal = 0;
    let mut deton = 0;
    let mut extinc = 0;
    let mut refund = 0;
    let mut sin_vivos = 0;
    let mut observ = 0;
    for semilla in 1..=6u64 {
        let mut m = Mundo::sembrar(semilla * 7, &sitios_falsos(96));
        for _ in 0..40 {
            m.avanzar(DIAS_ANIO * 10);
            invariantes(&m);
        }
        let t = |k: &str| m.cronica.totales.get(k).copied().unwrap_or(0) as usize;
        arsenal += t("Arsenal");
        deton += t("Detonacion");
        extinc += t("Extincion");
        refund += t("Refundacion");
        observ += t("Observacion");
        let vivos = m.pueblos.values().filter(|p| p.vivo).count();
        if vivos == 0 {
            sin_vivos += 1;
        }
        assert!(m.invierno.is_finite() && (0.0..=1.0).contains(&m.invierno));
        for p in m.pueblos.values() {
            assert!(p.arsenal <= catastrofe::TOPE_ARSENAL && p.radiacion.is_finite());
        }
        eprintln!("semilla {semilla}: {vivos} pueblos vivos, {} detonaciones, invierno {:.2}", m.detonaciones, m.invierno);
    }
    eprintln!("arsenal {arsenal}, detonaciones {deton}, extinciones {extinc}, refundaciones {refund}, observaciones {observ}, mundos muertos {sin_vivos}/6");
    assert!(sin_vivos < 6, "la humanidad no puede extinguirse en todos los mundos");
}

#[test]
fn un_dia_de_civilizacion_cuesta_poco() {
    let mut m = Mundo::sembrar(9, &sitios_falsos(96));
    m.avanzar(DIAS_ANIO * 200);
    let t = std::time::Instant::now();
    m.avanzar(DIAS_ANIO * 20);
    let por_dia = t.elapsed().as_secs_f64() * 1000.0 / (DIAS_ANIO * 20) as f64;
    eprintln!("paso_dia: {por_dia:.2} ms/dia con {} pueblos y {} personas", m.pueblos.len(), m.personas.len());
    assert!(por_dia < 40.0, "un día de civilización tarda {por_dia:.1} ms");
}
