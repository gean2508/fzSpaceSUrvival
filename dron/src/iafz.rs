//! FFI al kernel IAFZ-64 (C). Mismo cerebro que usa la fauna del juego, para
//! que un genoma entrenado aca se pueda llevar alla y al reves.
#![allow(dead_code)]

extern "C" {
    fn init_iafz_64(id: i32, seed: u32);
    fn think_iafz_64(id: i32, sens_lo: u64, sens_hi: u32) -> f32;
    fn think_multi_iafz_64(id: i32, sens_lo: u64, sens_hi: u32, out: *mut f32);
    fn mutate_iafz_64(source_id: i32, target_id: i32, rate: f32);
    fn nudge_iafz_64(source_id: i32, target_id: i32, bits: i32, paso: f32, seed: u32);
    fn crossover_iafz_64(p1: i32, p2: i32, child: i32);
    fn reset_echo_iafz_64(id: i32);
    fn leer_activaciones_iafz_64(id: i32, l1: *mut u64, l2: *mut u64, mem: *mut u32);
    fn export_iafz_64(id: i32, buf: *mut u8);
    fn import_iafz_64(id: i32, buf: *const u8);
    fn save_iafz_64(id: i32, filename: *const u8) -> i32;
    fn load_iafz_64(id: i32, filename: *const u8) -> i32;
}

/// Capacidad estatica del kernel (MAX_AGENTS en iafz_64.h).
pub const MAX_AGENTES: i32 = 128;
/// Consignas que emite el cerebro (SALIDAS en iafz_64.h).
pub const SALIDAS: usize = 4;
/// Bits de sensores que ve el cerebro (SENSOR_BITS en iafz_64.h).
pub const SENSOR_BITS: u32 = 96;
/// Neuronas por capa binaria (NEURONS en iafz_64.h) — las dos capas, oculta
/// y estrategia, tienen el mismo tamaño.
pub const NEURONAS: u32 = 64;
/// Bits de memoria recurrente (MEMORY_BITS en iafz_64.h).
pub const MEMORIA_BITS: u32 = 32;
/// Tamaño de un genoma en disco (IAFZ_BRAIN_BYTES en iafz_64.h). Si cambia, los
/// genomas viejos dejan de cargar a propósito.
pub const BRAIN_BYTES: usize = 11392;

pub fn init(id: i32, seed: u32) { unsafe { init_iafz_64(id, seed) } }
pub fn pensar(id: i32, lo: u64, hi: u32) -> f32 { unsafe { think_iafz_64(id, lo, hi) } }

/// Las cuatro consignas de una sola pasada. Las dos capas binarias -lo caro- se
/// calculan una vez y se comparten: medido, 3.8x mas rapido y 3.7x menos
/// memoria que tener cuatro cerebros separados.
pub fn pensar_multi(id: i32, lo: u64, hi: u32) -> [f32; SALIDAS] {
    let mut o = [0f32; SALIDAS];
    unsafe { think_multi_iafz_64(id, lo, hi, o.as_mut_ptr()) };
    o
}
pub fn mutar(origen: i32, destino: i32, tasa: f32) { unsafe { mutate_iafz_64(origen, destino, tasa) } }
pub fn cruzar(p1: i32, p2: i32, hijo: i32) { unsafe { crossover_iafz_64(p1, p2, hijo) } }
/// Copia `origen` en `destino` y le da un empujón FINO: `bits` bits sueltos
/// volteados y los pesos de salida corridos `paso`. Es el paso de búsqueda de
/// la adaptación en vuelo (`vivo.rs`); `mutar` salta demasiado lejos para eso.
pub fn empujar(origen: i32, destino: i32, bits: i32, paso: f32, semilla: u32) {
    unsafe { nudge_iafz_64(origen, destino, bits, paso, semilla) }
}
/// Copia exacta, sin tocar un bit.
pub fn clonar(origen: i32, destino: i32) { unsafe { mutate_iafz_64(origen, destino, 0.0) } }

/// El genoma de un slot como bloque de bytes, sin pasar por disco. Es lo que
/// usa el repertorio (`repertorio.rs`): en vuelo no se escriben archivos, se
/// copian bloques. Mismo formato que `guardar`, así que un volcado se puede
/// escribir a disco tal cual.
pub fn volcar(id: i32) -> Vec<u8> {
    let mut v = vec![0u8; BRAIN_BYTES];
    unsafe { export_iafz_64(id, v.as_mut_ptr()) };
    v
}

/// Restaura un genoma volcado. Deja la memoria recurrente en cero: el eco del
/// vuelo anterior no es parte de lo aprendido.
pub fn restaurar(id: i32, bytes: &[u8]) -> bool {
    if bytes.len() != BRAIN_BYTES {
        return false;
    }
    unsafe { import_iafz_64(id, bytes.as_ptr()) };
    true
}
/// Pone en cero las cuatro proyecciones de salida de un genoma volcado: con
/// eso la red emite exactamente 0 y el piloto híbrido ES el clásico (regla
/// 3.15). Offset = las dos capas empaquetadas (2 × 64 × 17 B, `iafz_64.h`).
pub fn salidas_en_cero(bytes: &mut [u8]) {
    const INICIO: usize = 2 * NEURONAS as usize * 17;
    const LARGO: usize = SALIDAS * NEURONAS as usize * 4;
    bytes[INICIO..INICIO + LARGO].fill(0);
}

#[test]
fn salidas_en_cero_emite_cero() {
    init(0, 7);
    let mut g = volcar(0);
    salidas_en_cero(&mut g);
    restaurar(0, &g);
    for s in 0..20u64 {
        let o = pensar_multi(0, s.wrapping_mul(0x9E37_79B9_7F4A_7C15), s as u32 * 7919);
        assert!(o.iter().all(|x| *x == 0.0), "salidas {o:?}");
    }
}

/// Borra la memoria recurrente. Obligatorio entre episodios: si no, el agente
/// arranca el vuelo con el eco del vuelo anterior y la evaluacion no es limpia.
pub fn reset_memoria(id: i32) { unsafe { reset_echo_iafz_64(id) } }

/// (capa oculta, capa de estrategia, memoria) de la ÚLTIMA decisión — un bit
/// por neurona, no el promedio ni nada calculado: es lo que el kernel dejó
/// prendido de verdad. Para el visor ("nuestra red"), no cambia el vuelo.
pub fn activaciones(id: i32) -> (u64, u64, u32) {
    let (mut l1, mut l2, mut mem) = (0u64, 0u64, 0u32);
    unsafe { leer_activaciones_iafz_64(id, &mut l1, &mut l2, &mut mem) };
    (l1, l2, mem)
}

/// **Versión del CÓDIGO DE SENSORES.** Hay que subirla cada vez que cambie el
/// significado de los 96 bits de entrada.
///
/// Existe por un modo de falla que pasó y es silencioso: el usuario estaba
/// mirando el visor y reportó que el dron «vuela muy tambaleante de adelante
/// para atrás». El genoma instalado se había entrenado tres cambios de sensores
/// antes, así que leía BASURA en los bits de visión. Y como el piloto es
/// híbrido (el modelo físico vuela, la red corrige encima — regla 3.15), no se
/// caía: el clásico hacía el trabajo y la red le sumaba ruido acotado a 100 Hz.
/// Se veía como un dron que vuela y entrega, pero tembloroso. Medido: 2.95×
/// más brusco que el clásico.
///
/// La sección 4 de la biblia ya avisaba que cambiar el código de sensores
/// invalida los genomas «más que cualquier otra cosa», pero no había NADA que
/// lo hiciera cumplir: cargar un genoma viejo funcionaba sin una sola queja.
///
/// El nombre quedó corto y conviene saberlo: no versiona sólo el código de
/// sensores, versiona CUALQUIER cambio que haga que los mismos bits de un
/// genoma signifiquen otra cosa — sensores O el cableado del kernel
/// (`csrc/iafz_64.c`). Un cambio de kernel no toca un solo bit de
/// `sensores_guia` y sin embargo invalida los genomas igual.
///
/// Historial:
///   1 — código binario original
///   2 — termómetro comprimido
///   3 — `obs_altura` en lugar de `divergencia`
///   4 — anillo de proximidad (5 sectores) en lugar de rumbo+amenaza
///   5 — el anillo pasa a medir LOOMING en vez de distancia
///   6 — capa de estrategia con `ctx_hi`. **Probada y REVERTIDA** por
///       medición (bitácora 7.25): aprendía más rápido y olvidaba los 12
///       peldaños de atrás. El número queda QUEMADO a propósito — no se
///       reusa para otra cosa, porque hay genomas marcados v6 en scratchpads
///       que no hablan el idioma de hoy y reciclar el 6 los haría pasar por
///       compatibles.
pub const VERSION_SENSORES: u32 = 5;

/// Dónde se anota la versión de un genoma. Archivo aparte porque el formato del
/// genoma son 11392 bytes crudos de `Brain64` y no tiene lugar para metadatos
/// sin romper la compatibilidad con el kernel C.
fn ruta_version(ruta: &str) -> String {
    format!("{ruta}.sensores")
}

pub fn guardar(id: i32, ruta: &str) -> bool {
    let c = std::ffi::CString::new(ruta).unwrap();
    let ok = unsafe { save_iafz_64(id, c.as_ptr() as *const u8) == 1 };
    if ok {
        let _ = std::fs::write(ruta_version(ruta), VERSION_SENSORES.to_string());
    }
    ok
}

/// Carga un genoma **sin** verificar compatibilidad. Para herramientas que
/// saben lo que hacen (comparar genomas viejos, por ejemplo).
pub fn cargar(id: i32, ruta: &str) -> bool {
    let c = std::ffi::CString::new(ruta).unwrap();
    unsafe { load_iafz_64(id, c.as_ptr() as *const u8) == 1 }
}

/// Con qué versión de sensores se entrenó este genoma, si se puede saber.
/// `None` = sin marca, o sea anterior a que existiera esta verificación.
pub fn version_de(ruta: &str) -> Option<u32> {
    std::fs::read_to_string(ruta_version(ruta)).ok()?.trim().parse().ok()
}

/// Carga verificando que el genoma hable el mismo idioma que los sensores de
/// hoy. Devuelve `Err` con la explicación si no.
///
/// Lo usa todo lo que VUELA (visor, capacidades). Un genoma incompatible no da
/// error ni se cae: vuela mal de una forma que parece un defecto del diseño y
/// no un archivo viejo, que es exactamente por qué esto hace falta.
pub fn cargar_compatible(id: i32, ruta: &str) -> Result<(), String> {
    if !cargar(id, ruta) {
        return Err(format!("no pude leer {ruta}"));
    }
    match version_de(ruta) {
        Some(v) if v == VERSION_SENSORES => Ok(()),
        Some(v) => Err(format!(
            "{ruta} se entrenó con sensores v{v} y hoy son v{VERSION_SENSORES}:              los bits significan otra cosa, así que la red lee basura.              No se cae —el modelo físico sigue volando— pero le suma ruido              encima y el vuelo sale tembloroso. Hay que reentrenar."
        )),
        None => Err(format!(
            "{ruta} no tiene marca de versión de sensores (es anterior a que              existiera la verificación). Si se entrenó antes del último cambio              de codificación, la red está leyendo basura. Reentrenar, o              marcarlo a mano si estás seguro de que es de v{VERSION_SENSORES}."
        )),
    }
}
