//! **Qué está haciendo AHORA el entrenamiento que corre en otro proceso.**
//!
//! Regla 3.3: lo que se afirma tiene que poder mirarse en el visor. El panel
//! viejo leía un archivo que sólo escribía la escalera de averías (borrada el
//! 2026-09-26), así que estuvo siempre vacío mientras `entrenarmision` molía
//! generaciones.
//!
//! # Por qué se lee el LOG y no un archivo de estado nuevo
//!
//! Lo obvio sería que `gym3d::entrenar` publicara un `.json` cada generación,
//! como ya hace `persecucion::leer_estado`. No sirve para el caso que hay que
//! resolver: **las corridas que ya están en el aire fueron lanzadas con el
//! binario anterior y no pueden empezar a publicar nada.** Un entrenamiento
//! que hay que reiniciar para poder mirarlo no se puede mirar — y reiniciar
//! cuesta todas las generaciones que lleva encima.
//!
//! El log, en cambio, ya existe y ya se escribe solo. Y no hay que adivinar
//! dónde está: `/proc/PID/fd/1` es el archivo al que ese proceso tiene
//! redirigida la salida, sea cual sea el nombre que le haya puesto quien lo
//! lanzó. Así el panel anda con las corridas de ahora, sin tocar el
//! entrenador (que además está al lado del camino caliente, y la sección 4
//! prohíbe tocarlo con un A/B corriendo).
//!
//! # Qué NO se muestra, a propósito
//!
//! `mejor` es el valor de UNA generación y rebota muchísimo (medido: el mismo
//! brazo alterna entre 1074.5 y −44.9 en diez generaciones). Se muestra igual
//! porque es lo que el log tiene, pero va con su cartel — mostrarlo como si
//! fuera "cuánto sabe el genoma" es exactamente el malentendido que la regla
//! 3.20 persigue. El número que sí dice algo es `rifas`: cuántas veces cruzó
//! el umbral y NO confirmó desde el último ascenso.

use std::path::{Path, PathBuf};

/// Un entrenamiento vivo: de qué proceso sale y a qué archivo escribe.
pub struct Corrida {
    pub pid: u32,
    /// Nombre corto para el panel. Sale de la carpeta del log (`mono2`,
    /// `rep2`), que es como se nombran los brazos de un A/B en el scratchpad.
    pub etiqueta: String,
    pub repertorio: bool,
    pub semilla: String,
    pub log: PathBuf,
}

/// Lo que se pudo leer del log. Todo opcional porque un log recién creado
/// todavía no tiene ninguno de estos renglones.
#[derive(Default)]
pub struct Datos {
    pub gen: i64,
    pub generaciones: i64,
    pub peldano: i64,
    pub peldanos: i64,
    pub nombre: String,
    pub mejor: Option<f32>,
    pub mediana: Option<f32>,
    pub estancado: bool,
    pub necesita: Option<f32>,
    pub trabado_hace: i64,
    pub aprobado_gen: i64,
    pub aprobado_peldano: i64,
    /// Veces que cruzó el umbral y la confirmación lo rechazó, desde el
    /// último ascenso. Es la medida honesta de "cruza por sorteo, no sabe"
    /// (regla 3.20) y la única de este panel que no rebota generación a
    /// generación.
    pub rifas: i64,
}

/// Los entrenamientos vivos, sacados de `/proc`.
pub fn corridas_vivas() -> Vec<Corrida> {
    let mut v = Vec::new();
    let Ok(rd) = std::fs::read_dir("/proc") else { return v };
    for e in rd.flatten() {
        let Some(nom) = e.file_name().to_str().map(str::to_string) else { continue };
        let Ok(pid) = nom.parse::<u32>() else { continue };
        let Ok(crudo) = std::fs::read(format!("/proc/{pid}/cmdline")) else { continue };
        let args: Vec<String> = crudo
            .split(|b| *b == 0)
            .filter(|c| !c.is_empty())
            .map(|c| String::from_utf8_lossy(c).into_owned())
            .collect();
        // El subcomando es el argumento 1: entrenarmision o
        // entrenarpersecucion. El propio visor queda afuera
        // porque el suyo es "visor".
        if args.len() < 2 || !args[1].starts_with("entrenar") { continue }
        // A qué archivo tiene redirigida la salida. Si es una tubería o una
        // terminal el symlink no apunta a un archivo y esa corrida no se
        // puede mirar — se saltea en vez de inventar una ruta.
        let Ok(log) = std::fs::read_link(format!("/proc/{pid}/fd/1")) else { continue };
        if !log.is_file() { continue }
        let etiqueta = log
            .parent()
            .and_then(|p| p.file_name())
            .and_then(|s| s.to_str())
            .map(str::to_string)
            .unwrap_or_else(|| format!("pid {pid}"));
        v.push(Corrida {
            pid,
            etiqueta,
            repertorio: args.iter().any(|a| a == "repertorio"),
            semilla: args
                .iter()
                .find_map(|a| a.strip_prefix("semilla="))
                .unwrap_or("")
                .to_string(),
            log,
        });
    }
    v.sort_by(|a, b| a.etiqueta.cmp(&b.etiqueta));
    v
}

/// Los primeros `max` bytes: ahí está el encabezado con el total de
/// generaciones, que no vuelve a aparecer nunca más en el log.
fn cabeza(p: &Path, max: usize) -> String {
    use std::io::Read;
    let Ok(mut f) = std::fs::File::open(p) else { return String::new() };
    let mut b = vec![0u8; max];
    let n = f.read(&mut b).unwrap_or(0);
    String::from_utf8_lossy(&b[..n]).into_owned()
}

/// Los últimos `max` bytes. Un log de mil generaciones pasa el megabyte y el
/// panel lo pide cada pocos segundos: leerlo entero sería pagar el archivo
/// completo por tres renglones.
fn cola(p: &Path, max: u64) -> String {
    use std::io::{Read, Seek, SeekFrom};
    let Ok(mut f) = std::fs::File::open(p) else { return String::new() };
    let largo = f.metadata().map(|m| m.len()).unwrap_or(0);
    let _ = f.seek(SeekFrom::Start(largo.saturating_sub(max)));
    let mut b = Vec::new();
    let _ = f.read_to_end(&mut b);
    String::from_utf8_lossy(&b).into_owned()
}

/// El número que sigue a `clave` en el renglón, sin la puntuación pegada
/// (`900,` y `50.9)` son números en estos logs).
fn tras(linea: &str, clave: &str) -> Option<f32> {
    let mut it = linea.split_whitespace();
    while let Some(t) = it.next() {
        if t.trim_matches(|c: char| !c.is_alphanumeric()) == clave {
            let bruto = it.next()?;
            let limpio = bruto.trim_matches(|c: char| {
                !c.is_ascii_digit() && c != '.' && c != '-'
            });
            return limpio.parse().ok();
        }
    }
    None
}

/// `peldaño 9/15: esquivar` → (9, 15, "esquivar"). Sirve igual dentro de
/// `«peldaño 7/15: sensores duros»` y con el `(estancado)` pegado al final.
fn peldano_de(linea: &str) -> Option<(i64, i64, String)> {
    let i = linea.rfind("peldaño ")? + "peldaño ".len();
    let resto = &linea[i..];
    let dp = resto.find(':')?;
    let (pt, nom) = (&resto[..dp], &resto[dp + 1..]);
    let (a, b) = pt.split_once('/')?;
    let p = a.trim().parse().ok()?;
    let tot = b.trim().parse().ok()?;
    let mut nombre = nom.trim();
    if let Some(j) = nombre.find('»') { nombre = &nombre[..j]; }
    if let Some(j) = nombre.find("   ") { nombre = &nombre[..j]; }
    Some((p, tot, nombre.trim().to_string()))
}

fn parsear(cabeza: &str, cola: &str) -> Datos {
    let mut d = Datos::default();
    for l in cabeza.lines() {
        if let Some(r) = l.trim().strip_prefix("MISIONES · ") {
            d.generaciones = r.split_whitespace().next()
                .and_then(|t| t.parse().ok()).unwrap_or(0);
            break;
        }
    }
    for l in cola.lines() {
        let t = l.trim();
        // La generación sale de CUALQUIER renglón que arranque con "gen":
        // los de "mejor/mediana" salen cada diez, y los de confirmación
        // salen cuando pasan, así que juntos dan el número más fresco.
        if let Some(r) = t.strip_prefix("gen ") {
            if let Some(g) = r.split_whitespace().next().and_then(|x| x.parse::<i64>().ok()) {
                if g >= d.gen { d.gen = g; }
            }
        }
        if t.contains("→ peldaño") && t.contains("APROBADO") {
            d.aprobado_gen = d.gen;
            if let Some((p, tot, nom)) = peldano_de(t) {
                d.aprobado_peldano = p;
                d.peldano = p;
                d.peldanos = tot;
                d.nombre = nom;
            }
            // Ascendió: las rifas de abajo eran del peldaño anterior.
            d.rifas = 0;
        } else if t.contains("NO aprueba") {
            d.rifas += 1;
        } else if t.contains(" mejor ") && t.contains(" mediana ") {
            d.mejor = tras(t, "mejor");
            d.mediana = tras(t, "mediana");
            d.estancado = t.contains("(estancado)");
            if let Some((p, tot, nom)) = peldano_de(t) {
                d.peldano = p;
                d.peldanos = tot;
                d.nombre = nom;
            }
        } else if t.contains("TRABADO en") {
            d.trabado_hace = tras(t, "hace").unwrap_or(0.0) as i64;
            d.necesita = tras(t, "necesita");
            if let Some((p, tot, nom)) = peldano_de(t) {
                d.peldano = p;
                d.peldanos = tot;
                d.nombre = nom;
            }
        }
    }
    d
}

fn limpio(s: &str) -> String {
    s.replace('\\', "").replace('"', "'")
}

fn num(v: Option<f32>) -> String {
    v.map(|x| format!("{x:.1}")).unwrap_or_else(|| "null".to_string())
}

/// Lo que consume el visor. Devuelve `[]` cuando no hay nada entrenando, que
/// es el caso normal: el panel se esconde solo.
pub fn json() -> String {
    let mut partes = Vec::new();
    for c in corridas_vivas() {
        let d = parsear(&cabeza(&c.log, 1024), &cola(&c.log, 96 * 1024));
        // Hace cuánto que el log no crece. Es la prueba de que el panel está
        // vivo y no mostrando una foto vieja: si esto sube, la corrida se
        // colgó o terminó.
        let hace = std::fs::metadata(&c.log)
            .and_then(|m| m.modified())
            .ok()
            .and_then(|t| t.elapsed().ok())
            .map(|e| e.as_secs())
            .unwrap_or(0);
        partes.push(format!(
            r#"{{"pid":{},"etiqueta":"{}","repertorio":{},"semilla":"{}","gen":{},"generaciones":{},"peldano":{},"peldanos":{},"nombre":"{}","mejor":{},"mediana":{},"estancado":{},"necesita":{},"trabadoHace":{},"aprobadoGen":{},"aprobadoPeldano":{},"rifas":{},"hace":{}}}"#,
            c.pid,
            limpio(&c.etiqueta),
            c.repertorio,
            limpio(&c.semilla),
            d.gen,
            d.generaciones,
            d.peldano,
            d.peldanos,
            limpio(&d.nombre),
            num(d.mejor),
            num(d.mediana),
            d.estancado,
            num(d.necesita),
            d.trabado_hace,
            d.aprobado_gen,
            d.aprobado_peldano,
            d.rifas,
            hace
        ));
    }
    format!("[{}]", partes.join(","))
}

#[cfg(test)]
mod pruebas {
    use super::*;

    /// Los renglones son los del log de verdad, copiados tal cual. Si el
    /// formato del entrenador cambia, esto falla acá y no en el visor
    /// mostrando guiones para siempre (regla 3.18: el instrumento también se
    /// audita).
    #[test]
    fn parsea_los_renglones_reales_del_entrenador() {
        let cabeza = "MISIONES · 1200 generaciones · sensores: TODO el rango\n";
        let cola = "\
  gen   0  mejor   1106.9  mediana   -361.7   peldaño 1/15: frenar
  gen   7  → peldaño 9 (APROBADO): peldaño 9/15: esquivar
  gen  81  · cruzó el umbral con 1090.3 pero en la confirmación 1/3 dio -34.2 — fue suerte del sorteo, NO aprueba
  gen  84  · cruzó el umbral con 1025.8 pero en la confirmación 1/3 dio -43.9 — fue suerte del sorteo, NO aprueba
  gen  90  mejor    -44.9  mediana   -600.5   peldaño 9/15: esquivar   (estancado)
  gen 100  TRABADO en «peldaño 9/15: esquivar» hace 93 generaciones (necesita 900, va -44.9) — los peldaños
";
        let d = parsear(cabeza, cola);
        assert_eq!(d.generaciones, 1200);
        assert_eq!(d.gen, 100);
        assert_eq!((d.peldano, d.peldanos), (9, 15));
        assert_eq!(d.nombre, "esquivar");
        assert_eq!(d.mejor, Some(-44.9));
        assert_eq!(d.mediana, Some(-600.5));
        assert!(d.estancado);
        assert_eq!(d.necesita, Some(900.0));
        assert_eq!(d.trabado_hace, 93);
        assert_eq!(d.aprobado_peldano, 9);
        // Las dos rifas son POSTERIORES al ascenso: cuentan.
        assert_eq!(d.rifas, 2);
    }

    /// Un log recién nacido (sólo el encabezado) no puede romper el panel.
    #[test]
    fn un_log_vacio_no_rompe_nada() {
        let d = parsear("MISIONES · 600 generaciones\n", "");
        assert_eq!(d.generaciones, 600);
        assert_eq!(d.gen, 0);
        assert!(d.mejor.is_none());
        assert!(json().starts_with('['));
    }
}
