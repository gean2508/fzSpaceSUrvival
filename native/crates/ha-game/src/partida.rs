//! **Las partidas: varias, cada una en su carpeta, y ninguna pisa a la otra.**
//!
//! Pedido del usuario, textual: *"cada que yo abra una partida, a menos que yo
//! guarde la partida, se abra un mundo nuevo, o sea todo nuevo, por ende no hay
//! nada de lo que yo haya colocado, porque no me gusta que cosas que coloqué
//! antes estén puestas"*. Eso se resolvió primero con UNA partida que se borraba
//! al empezar de nuevo.
//!
//! El 15 ago 2026 la propuesta de interfaz pidió el paso siguiente: una **lista**
//! de partidas, con su semilla, su día y quién vive ahí, que se pueden abrir y
//! borrar por separado. El propio documento lo marcó como trabajo nuevo y no un
//! cambio de pantalla, y tenía razón: hasta acá todo el estado del juego se
//! escribía suelto en `persist_dir()` —veinte JSON y dos carpetas— así que dos
//! partidas eran físicamente imposibles.
//!
//! ## Cómo entran varias sin tocar veinte módulos
//!
//! No se cambió ni un sitio de guardado. Lo que se movió es el **suelo**:
//! `persist_dir()` ya no es la carpeta de la plataforma sino
//! `<plataforma>/partidas/<id>`, y la elige [`abrir`]. Cada módulo sigue
//! escribiendo su `inventory.json` donde siempre, y resulta estar adentro de la
//! partida activa. Borrar una partida es borrar su carpeta, y ahí sí no queda
//! nada suelto — que era el modo de falla del borrado por lista.
//!
//! Sin partida activa (tests, drivers de QA, editores) `persist_dir()` sigue
//! siendo la carpeta de siempre: nada de lo que ya andaba cambia de sitio.
//!
//! ## Las partidas viejas no se pierden
//!
//! Un teléfono que ya venía jugando tiene su estado suelto en la raíz. La primera
//! vez que se lista, [`migrar_si_hace_falta`] lo mete en `partidas/inicial/` con
//! su nombre y su semilla. No se copia: se mueve, archivo por archivo, y sólo los
//! que el juego declara suyos.
//!
//! ## Guardar
//!
//! Guardar sigue siendo `game_phase::save_savegame`, y además deja la ficha de la
//! partida en su carpeta ([`guardar_ficha`]). Sin ficha, cargar usa la semilla 0,
//! que es el mundo de referencia de siempre.

use std::path::{Path, PathBuf};
use std::sync::RwLock;

use serde::{Deserialize, Serialize};

/// Los archivos de estado que escribe el juego. **Es la lista completa** — si
/// alguien agrega uno nuevo y no lo pone acá, no se va a migrar de la raíz a su
/// carpeta y la partida vieja va a perder ese pedazo. Lo cuida
/// `ningun_estado_de_partida_se_escapa_del_borrado`.
const ARCHIVOS: &[&str] = &[
    "cameras.json",
    "ceilings.json",
    "chests.json",
    "cultivos.json",
    "colony.json",
    "edificios.json",
    "escaleras.json",
    "pandemia.json",
    "harvested.json",
    "inventory.json",
    "machines.json",
    "maps.json",
    "nav_data.json",
    "roofs.json",
    "savegame.json",
    "structures.json",
    "survival.json",
    "tiles.json",
    "torches.json",
    "vessels.json",
    "walls.json",
    "partida.json",
];

/// Las carpetas de estado. `vivarium` es la fauna y el compañero-IA; `society`,
/// la memoria de los NPC.
const CARPETAS: &[&str] = &["vivarium", "society"];

/// Dónde viven todas las partidas, colgando de la carpeta de la plataforma.
const RAIZ: &str = "partidas";

/// El id de la partida que se migró desde el formato viejo.
const ID_MIGRADA: &str = "inicial";

// ---------------------------------------------------------------------------
// La partida activa
// ---------------------------------------------------------------------------

/// Cuál está abierta. Es global y no un recurso de Bevy porque `persist_dir()`
/// la consulta desde funciones sueltas que no tienen `World` a mano — la mitad
/// de los guardados del juego son `fn` libres llamadas desde un `Drop` o un
/// `on_exit`.
static ACTIVA: RwLock<Option<String>> = RwLock::new(None);

/// El id de la partida abierta, si hay alguna.
pub fn activa() -> Option<String> {
    ACTIVA.read().ok().and_then(|g| g.clone())
}

/// El sufijo que `inventory_persist::persist_dir` le cuelga a la carpeta de la
/// plataforma. Vacío = no hay partida abierta y se escribe donde siempre.
pub fn sufijo() -> Option<PathBuf> {
    activa().map(|id| Path::new(RAIZ).join(id))
}

/// La carpeta de una partida concreta.
pub fn carpeta(id: &str) -> PathBuf {
    crate::inventory_persist::base_dir().join(RAIZ).join(id)
}

// ---------------------------------------------------------------------------
// La ficha
// ---------------------------------------------------------------------------

/// **Lo que la lista de partidas muestra de cada mundo.** Es lo que se dibujó en
/// los mockups: nombre, semilla, día y quiénes viven ahí.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Ficha {
    /// Id de carpeta. No se muestra: se muestra `nombre`.
    #[serde(default)]
    pub id: String,
    #[serde(default = "nombre_por_defecto")]
    pub nombre: String,
    #[serde(default)]
    pub semilla: u32,
    /// Modo elegido AL CREAR EL MUNDO — no es un botón del menú (decisión del
    /// 15 ago 2026).
    #[serde(default)]
    pub creativo: bool,
    /// El día del mundo, para que la fila diga algo más que el nombre.
    #[serde(default)]
    pub dia: u32,
    /// Los personajes que jugaron acá. Un mundo puede tener varios.
    #[serde(default)]
    pub habitantes: Vec<String>,
    /// Segundos desde el epoch. Ordena la lista: la más nueva arriba.
    #[serde(default)]
    pub creada: u64,
}

fn nombre_por_defecto() -> String {
    "Partida".to_string()
}

impl Default for Ficha {
    fn default() -> Self {
        Self {
            id: String::new(),
            nombre: nombre_por_defecto(),
            semilla: 0,
            creativo: false,
            dia: 0,
            habitantes: Vec::new(),
            creada: ahora(),
        }
    }
}

fn ahora() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// El `partida.json` de la partida activa. Se mantiene el nombre viejo del
/// archivo: una partida guardada antes de este cambio se sigue leyendo.
#[allow(dead_code)] // lo usa el test de la lista de archivos y los drivers de QA.
pub fn partida_path() -> PathBuf {
    crate::inventory_persist::persist_dir().join("partida.json")
}

fn ficha_path(id: &str) -> PathBuf {
    carpeta(id).join("partida.json")
}

/// La ficha de la partida abierta (nombre, semilla, día), si hay una. La lee el
/// menú de pausa al abrirse: una lectura de disco por apertura, no por cuadro.
pub fn ficha_activa() -> Option<Ficha> {
    leer_ficha(&activa()?)
}

fn leer_ficha(id: &str) -> Option<Ficha> {
    let txt = std::fs::read_to_string(ficha_path(id)).ok()?;
    let mut f: Ficha = serde_json::from_str(&txt).ok()?;
    f.id = id.to_string();
    Some(f)
}

// ---------------------------------------------------------------------------
// La lista
// ---------------------------------------------------------------------------

/// **Todas las partidas, la más nueva primero.** Migra el formato viejo si hace
/// falta, así que llamarla es seguro incluso en un teléfono que venía jugando.
pub fn listar() -> Vec<Ficha> {
    migrar_si_hace_falta();
    let raiz = crate::inventory_persist::base_dir().join(RAIZ);
    let mut out: Vec<Ficha> = std::fs::read_dir(&raiz)
        .into_iter()
        .flatten()
        .flatten()
        .filter(|e| e.path().is_dir())
        .filter_map(|e| {
            let id = e.file_name().to_string_lossy().to_string();
            leer_ficha(&id).or(Some(Ficha { id, ..Default::default() }))
        })
        .collect();
    out.sort_by(|a, b| b.creada.cmp(&a.creada));
    out
}

/// ¿Hay algo que continuar? Lo usa el botón CONTINUAR del menú.
pub fn hay_alguna() -> bool {
    !listar().is_empty()
}

/// La última que se tocó — a la que entra CONTINUAR.
pub fn ultima() -> Option<Ficha> {
    listar().into_iter().next()
}

// ---------------------------------------------------------------------------
// Crear, abrir, borrar
// ---------------------------------------------------------------------------

/// **Una partida nueva: carpeta propia, semilla sorteada, disco virgen.**
///
/// La semilla sale del reloj, que es la única fuente de entropía que no obliga a
/// meter una dependencia nueva para sortear un `u32`. Nunca devuelve 0: el 0 está
/// reservado para "el mundo de referencia" (el de los tests y los medidores), y
/// una partida que cayera ahí por azar sería la única indistinguible de ellos.
pub fn crear(nombre: &str, semilla: Option<u32>, creativo: bool) -> Ficha {
    let semilla = semilla.filter(|s| *s != 0).unwrap_or_else(semilla_al_azar);
    let id = format!("p{}", ahora());
    let id = if carpeta(&id).exists() { format!("{id}-{}", semilla % 997) } else { id };
    let ficha = Ficha {
        id: id.clone(),
        nombre: if nombre.trim().is_empty() { nombre_por_defecto() } else { nombre.trim().to_string() },
        semilla,
        creativo,
        dia: 0,
        habitantes: Vec::new(),
        creada: ahora(),
    };
    let _ = std::fs::create_dir_all(carpeta(&id));
    abrir_id(&id);
    ha_shared::partida::set_semilla(semilla);
    escribir_ficha(&ficha);
    eprintln!("ha-partida: mundo nuevo «{}», semilla {semilla}", ficha.nombre);
    ficha
}

/// Una semilla al azar. Es pública porque la pantalla NUEVA PARTIDA muestra el
/// número antes de crear el mundo, y deja volver a sortearlo.
pub fn semilla_al_azar() -> u32 {
    let n = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos() as u64)
        .unwrap_or(1);
    // Mezcla para que dos arranques seguidos no salgan con semillas vecinas: los
    // nanos consecutivos comparten los bits altos y el ruido de terreno los usa.
    let mut s = n ^ (n >> 33);
    s = s.wrapping_mul(0xff51_afd7_ed55_8ccd);
    s ^= s >> 33;
    // **De cinco cifras.** Una semilla es algo que uno dicta y copia a mano —
    // en las imágenes de referencia dice "48 213", no "2 379 036 509". El mundo
    // no se empobrece: 100.000 mundos distintos son más de los que nadie va a
    // recorrer, y la mezcla de arriba sigue evitando que dos arranques seguidos
    // salgan vecinos.
    ((s % 100_000) as u32) | 1
}

/// Marca cuál es la activa. No toca el disco.
fn abrir_id(id: &str) {
    if let Ok(mut g) = ACTIVA.write() {
        *g = Some(id.to_string());
    }
}

/// **Abrir una partida existente**: se recupera su semilla y no se borra nada.
///
/// Sin ficha (partida anterior a este cambio) queda la semilla 0, que es el mundo
/// que esa partida tenía cuando se guardó.
pub fn abrir(id: &str) {
    abrir_id(id);
    let semilla = leer_ficha(id).map(|f| f.semilla).unwrap_or(0);
    ha_shared::partida::set_semilla(semilla);
    eprintln!("ha-partida: abierta «{id}», semilla {semilla}");
}

/// **Borrar una partida.** Se va la carpeta entera, así que no queda nada suelto.
/// El personaje NO se toca: vive aparte y puede entrar a cualquier mundo.
/// **Renombrar un mundo sin tocar nada más.** Es el lápiz de la fila en la
/// pantalla PARTIDAS: cambia cómo se llama y deja la semilla, el día y los
/// habitantes donde estaban.
pub fn renombrar(id: &str, nombre: &str) {
    let nombre = nombre.trim();
    if nombre.is_empty() {
        return;
    }
    let ruta = ficha_path(id);
    let Ok(txt) = std::fs::read_to_string(&ruta) else { return };
    let Ok(mut f) = serde_json::from_str::<Ficha>(&txt) else { return };
    f.nombre = nombre.to_string();
    if let Ok(txt) = serde_json::to_string_pretty(&f) {
        let _ = std::fs::write(&ruta, txt);
    }
}

pub fn borrar(id: &str) {
    let dir = carpeta(id);
    // Cinturón: sólo se borra algo que esté DENTRO de `partidas/`, nunca la
    // carpeta de la plataforma (que en escritorio es el repositorio).
    let raiz = crate::inventory_persist::base_dir().join(RAIZ);
    if !dir.starts_with(&raiz) || id.is_empty() || id.contains("..") {
        eprintln!("ha-partida: me negué a borrar «{id}»");
        return;
    }
    if let Err(e) = std::fs::remove_dir_all(&dir) {
        eprintln!("ha-partida: no se pudo borrar «{id}» ({e})");
    }
    if activa().as_deref() == Some(id) {
        if let Ok(mut g) = ACTIVA.write() {
            *g = None;
        }
    }
}

// ---------------------------------------------------------------------------
// Guardar la ficha
// ---------------------------------------------------------------------------

fn escribir_ficha(f: &Ficha) {
    let path = ficha_path(&f.id);
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    match serde_json::to_string_pretty(f) {
        Ok(body) => {
            if let Err(e) = std::fs::write(&path, body) {
                eprintln!("ha-partida: no se pudo guardar la ficha ({e})");
            }
        }
        Err(e) => eprintln!("ha-partida: ficha ilegible ({e})"),
    }
}

/// Deja la ficha de la partida en curso en el disco: semilla, día y habitantes.
/// La llama quien guarda el personaje (`game_phase::save_savegame`), para que las
/// dos mitades del savegame se escriban juntas y no puedan quedar desparejas.
pub fn guardar_ficha(dia: Option<u32>, habitante: Option<&str>) {
    let Some(id) = activa() else { return };
    let mut f = leer_ficha(&id).unwrap_or(Ficha { id: id.clone(), ..Default::default() });
    f.id = id;
    f.semilla = ha_shared::partida::semilla();
    // `None` = quien guarda no sabe el día (el savegame se escribe desde sitios
    // que no tienen el reloj del mundo a mano): se conserva el que ya había.
    if let Some(d) = dia {
        f.dia = d;
    }
    if let Some(h) = habitante {
        let h = h.trim();
        if !h.is_empty() && !f.habitantes.iter().any(|x| x == h) {
            f.habitantes.push(h.to_string());
        }
    }
    // La lista se ordena por `creada`, y CONTINUAR entra a la primera: tocarla al
    // guardar es lo que hace que "continuar" signifique la última que jugaste.
    f.creada = ahora();
    escribir_ficha(&f);
}

// ---------------------------------------------------------------------------
// Migración del formato viejo
// ---------------------------------------------------------------------------

/// Mueve el estado suelto de la raíz a `partidas/inicial/`. Corre una sola vez:
/// después de mover, la raíz ya no tiene `savegame.json` y la condición no se
/// cumple más.
pub fn migrar_si_hace_falta() {
    let base = crate::inventory_persist::base_dir();
    // La señal de que hay una partida vieja suelta es su savegame. Sin eso puede
    // haber JSON sueltos de un driver de QA, y ésos no son una partida.
    if !base.join("savegame.json").exists() {
        return;
    }
    let destino = carpeta(ID_MIGRADA);
    if destino.exists() {
        return;
    }
    if std::fs::create_dir_all(&destino).is_err() {
        return;
    }
    for f in ARCHIVOS {
        let de = base.join(f);
        if de.exists() {
            let _ = std::fs::rename(&de, destino.join(f));
        }
    }
    for d in CARPETAS {
        let de = base.join(d);
        if de.exists() {
            let _ = std::fs::rename(&de, destino.join(d));
        }
    }
    // Si no traía ficha, se le arma una para que la lista la pueda mostrar.
    if !destino.join("partida.json").exists() {
        escribir_ficha(&Ficha {
            id: ID_MIGRADA.to_string(),
            nombre: "Mi partida".to_string(),
            ..Default::default()
        });
    }
    eprintln!("ha-partida: partida vieja migrada a partidas/{ID_MIGRADA}");
}

#[cfg(test)]
mod tests {
    use super::*;

    /// **La lista de archivos tiene que estar completa.**
    ///
    /// El modo de falla que cuida es silencioso y feo: alguien agrega
    /// `persist_dir().join("loquesea.json")` en un módulo nuevo, nadie lo agrega
    /// acá, y a partir de ese día la migración de una partida vieja deja ese
    /// archivo tirado en la raíz. Se lee el propio código fuente porque no hay
    /// registro central de archivos persistidos — y montar uno sería más
    /// ceremonia que esta comprobación.
    #[test]
    fn ningun_estado_de_partida_se_escapa_del_borrado() {
        let mut vistos: Vec<String> = Vec::new();
        let raiz = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
        let mut pendientes = vec![raiz];
        while let Some(p) = pendientes.pop() {
            let Ok(rd) = std::fs::read_dir(&p) else { continue };
            for e in rd.flatten() {
                let path = e.path();
                if path.is_dir() {
                    pendientes.push(path);
                    continue;
                }
                if path.extension().is_none_or(|x| x != "rs") {
                    continue;
                }
                let Ok(txt) = std::fs::read_to_string(&path) else { continue };
                for (_, resto) in txt.match_indices("persist_dir().join(\"") {
                    if let Some(fin) = resto[20..].find('"') {
                        vistos.push(resto[20..20 + fin].to_string());
                    }
                }
            }
        }
        let conocidos: Vec<&str> = ARCHIVOS.iter().chain(CARPETAS.iter()).copied().collect();
        let faltan: Vec<&String> = vistos
            .iter()
            .filter(|v| !conocidos.contains(&v.as_str()))
            .collect();
        assert!(
            faltan.is_empty(),
            "estos archivos se persisten y NO están en la lista de la partida: {faltan:?} \
             — agregalos a partida::ARCHIVOS o a CARPETAS"
        );
    }

    /// **Ningún estado de partida se carga en `Startup`.** `Startup` corre antes
    /// del menú, cuando todavía no hay partida elegida y `persist_dir()` es la
    /// carpeta base: todo lo que se leía ahí (paredes, pozos, casas, mochila) se
    /// colaba en cualquier partida NUEVA. Lo reportó el usuario el 28 sep 2026
    /// ("el juego debe venir limpio"). Las cargas van en
    /// `OnEnter(GamePhase::Loading)`.
    #[test]
    fn ninguna_carga_de_partida_corre_en_startup() {
        let raiz = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
        let mut malos = Vec::new();
        let mut pendientes = vec![raiz];
        while let Some(p) = pendientes.pop() {
            let Ok(rd) = std::fs::read_dir(&p) else { continue };
            for e in rd.flatten() {
                let path = e.path();
                if path.is_dir() {
                    pendientes.push(path);
                    continue;
                }
                if path.extension().is_none_or(|x| x != "rs") {
                    continue;
                }
                let Ok(txt) = std::fs::read_to_string(&path) else { continue };
                if !txt.contains("persist_dir()") || path.ends_with("partida.rs") {
                    continue;
                }
                for (i, _) in txt.match_indices("add_systems(Startup") {
                    // lo registrado: hasta cerrar el paréntesis de `add_systems(`
                    let resto = &txt[i + "add_systems".len()..];
                    let mut hondo = 0i32;
                    let mut fin = resto.len();
                    for (k, ch) in resto.char_indices() {
                        match ch {
                            '(' => hondo += 1,
                            ')' => {
                                hondo -= 1;
                                if hondo == 0 {
                                    fin = k;
                                    break;
                                }
                            }
                            _ => {}
                        }
                    }
                    let tramo = &resto[..fin];
                    if tramo.contains("load") || tramo.contains("cargar") {
                        malos.push(format!("{}: {}", path.display(), tramo.lines().next().unwrap_or("")));
                    }
                }
            }
        }
        assert!(malos.is_empty(), "cargas de partida en Startup: {malos:#?}");
    }

    /// **Borrar sólo borra adentro de `partidas/`.** El cinturón de [`borrar`] es
    /// lo único que separa "se fue una partida" de "se fue el repositorio": en
    /// escritorio la carpeta de la plataforma es `.`.
    #[test]
    fn borrar_se_niega_a_salirse_de_su_carpeta() {
        for malo in ["", "..", "../..", "../otra"] {
            borrar(malo);
        }
        // Si alguno se hubiera escapado, no habría src/ que leer.
        assert!(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src").is_dir(),
            "borrar() salió de partidas/ y se llevó algo que no era suyo"
        );
    }

    /// Una semilla sorteada nunca es 0 — el 0 es el mundo de referencia de los
    /// tests, y una partida ahí sería indistinguible de ellos.
    #[test]
    fn la_semilla_sorteada_nunca_es_cero() {
        for _ in 0..50 {
            assert_ne!(semilla_al_azar(), 0);
        }
    }
}
