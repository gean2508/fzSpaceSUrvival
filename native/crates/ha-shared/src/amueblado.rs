//! **El amueblado de una casa generada, cuarto por cuarto, con paso garantizado.**
//!
//! Reemplaza al sorteo por celda de `worldgen::interior_decor_at`, que tenía tres
//! defectos que se veían juntos (reporte del usuario del 29 sep 2026: *"me quedo
//! atorado"*, *"atravieso los muebles"*):
//!
//! 1. **Sorteaba el 55 % de las celdas pegadas a un muro**, y cada mueble ocupa
//!    varias: se pisaban entre sí y el cuarto quedaba tapado de muebles.
//! 2. **Ningún mueble tenía colisión**: se caminaba a través de camas y roperos.
//! 3. Darles colisión con ese reparto habría encerrado al jugador — el sorteo no
//!    sabía de pasillos.
//!
//! Ahora la casa se amuebla ENTERA de una vez (determinista, cacheada): se
//! separan los cuartos por relleno, a cada uno se le da un tema (la casa: living,
//! dormitorio, cocina y baño, de mayor a menor), y los muebles van **contra la
//! pared norte** —como los dibuja el pack, de frente a la cámara— y los angostos
//! contra las paredes de los costados. Cada colocación se prueba contra la
//! invariante que importa: **toda celda libre del cuarto sigue unida a sus
//! puertas**. Si un mueble corta el paso, no se pone.
//!
//! La colisión de cada mueble es su PIE: el ancho del sprite y la franja de abajo
//! (lo que apoya en el piso). Lo de arriba —el respaldo, la cabecera, el ropero
//! alto— se dibuja sobre la pared de atrás y no frena, igual que un muro.

use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::{Arc, Mutex, OnceLock};

use crate::celestial_bodies::CelestialBody;
use crate::topdown::{topdown_tile, Tile};
use crate::worldgen::{hash_cell, ExtraDecor};

/// Un mueble colocado. Ocupa las celdas `x0..x0+cw` × `z0..z0+prof`; el sprite
/// se dibuja con el pie en `z0+prof` y centrado en `x0 + cw/2`.
#[derive(Clone, Copy, Debug)]
pub struct Mueble {
    pub decor: &'static ExtraDecor,
    pub x0: i64,
    pub z0: i64,
    pub cw: i64,
    pub prof: i64,
}

impl Mueble {
    /// Centro horizontal y línea del pie del sprite, en metros de mundo.
    pub fn centro_pie(&self) -> (f32, f32) {
        (self.x0 as f32 + self.cw as f32 * 0.5, (self.z0 + self.prof) as f32)
    }

    /// Caja de colisión `[x0, z0, x1, z1]` en metros: el pie del mueble, con un
    /// margen chico para que rozar el borde del dibujo no sea un choque.
    pub fn caja(&self) -> [f32; 4] {
        let (cx, pie) = self.centro_pie();
        let medio = (self.decor.size[0].min(self.cw as f32) * 0.5 - 0.06).max(0.2);
        [cx - medio, pie - self.prof as f32 + 0.08, cx + medio, pie - 0.04]
    }

    /// Celda ancla: donde el chunk lo siembra y con la que se lo levanta.
    pub fn ancla(&self) -> (i64, i64) {
        (self.x0, self.z0)
    }
}

/// **Muebles que se abren** — para saquear o guardar, como en Project Zomboid
/// (pedido del usuario, 29 sep 2026). Qué mueble es cuál se eligió MIRANDO la
/// lámina de los temas (los singles del pack están numerados, no nombrados):
/// los roperos de 1×3 del dormitorio, la cómoda y las mesitas con cajones, las
/// heladeras, la alacena y los vanitorios del baño.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Contenedor {
    Heladera,
    Alacena,
    Ropero,
    Comoda,
    Botiquin,
}

impl Contenedor {
    pub fn nombre(self) -> &'static str {
        match self {
            Contenedor::Heladera => "HELADERA",
            Contenedor::Alacena => "ALACENA",
            Contenedor::Ropero => "ROPERO",
            Contenedor::Comoda => "CÓMODA",
            Contenedor::Botiquin => "BOTIQUÍN",
        }
    }

    /// Lo que hay adentro la primera vez que se abre. Una de cada cuatro está
    /// vacía (alguien pasó antes). `semilla` sale del mueble: el mismo mueble
    /// tiene siempre lo mismo.
    pub fn botin(self, semilla: u32) -> Vec<(&'static str, u32)> {
        let mut s = semilla | 1;
        let mut azar = move |n: u32| {
            s ^= s << 13;
            s ^= s >> 17;
            s ^= s << 5;
            s % n.max(1)
        };
        if azar(4) == 0 {
            return Vec::new();
        }
        let tabla: &[(&str, u32, u32)] = match self {
            Contenedor::Heladera => &[
                ("apple", 1, 3), ("orange", 1, 2), ("egg", 1, 3), ("beef", 1, 2), ("pork", 1, 2),
                ("chicken", 1, 2), ("bread", 1, 1), ("carrot", 1, 3), ("soup", 1, 1),
            ],
            Contenedor::Alacena => &[
                ("bread", 1, 2), ("wheat", 1, 3), ("wheat_seeds", 2, 5), ("pumpkin_seeds", 1, 3),
                ("pie", 1, 1), ("apple", 1, 2), ("sticks", 2, 4),
            ],
            Contenedor::Ropero => &[
                ("leather", 1, 2), ("wool", 1, 3), ("feather", 1, 3), ("name_tag", 1, 1),
                ("torch", 1, 3), ("lantern", 1, 1),
            ],
            Contenedor::Comoda => &[
                ("torch", 1, 2), ("sticks", 1, 4), ("map_small", 1, 1), ("mapa_tesoro", 1, 1), ("name_tag", 1, 1),
                ("iron_ingot", 1, 2), ("gold_ingot", 1, 1), ("gem_blue", 1, 1), ("bone", 1, 2),
            ],
            Contenedor::Botiquin => &[
                ("potion_green", 1, 2), ("potion_blue", 1, 1), ("supply", 1, 1), ("apple", 1, 1),
            ],
        };
        let cuantos = 1 + azar(3) as usize;
        let mut fuera: Vec<(&'static str, u32)> = Vec::new();
        for _ in 0..cuantos {
            let (it, a, b) = tabla[azar(tabla.len() as u32) as usize];
            let n = a + azar(b - a + 1);
            match fuera.iter_mut().find(|(i, _)| *i == it) {
                Some(e) => e.1 += n,
                None => fuera.push((it, n)),
            }
        }
        // **Ropa del creador de personajes** (Project Zomboid: la ropa se encuentra). El ropero
        // casi siempre trae una prenda; la cómoda, a veces.
        let prob_ropa = match self {
            Contenedor::Ropero => 7,
            Contenedor::Comoda => 2,
            _ => 0,
        };
        if azar(10) < prob_ropa {
            fuera.push((crate::armadura::prenda_al_azar(azar(1000)), 1));
        }
        fuera
    }
}

/// Qué contenedor es este mueble, si es uno.
pub fn contenedor_de(key: &str) -> Option<Contenedor> {
    let n = key.strip_prefix("furniture/")?;
    Some(match n {
        // Mirados uno por uno en la revisión visual del 29 sep 2026
        // (`scripts/revision-visual.json`): `dormitorio_483..503` eran bancos de
        // trabajo con herramientas, no cómodas, y `bano_101/105` son banquetas.
        "cocina_158" | "cocina_162" => Contenedor::Heladera,
        "cocina_121" | "cocina_124" | "living_42" => Contenedor::Alacena,
        "dormitorio_515" | "dormitorio_519" | "dormitorio_523" | "dormitorio_527" | "dormitorio_531"
        | "dormitorio_535" | "living_37" | "living_39" | "living_1" | "living_103" => Contenedor::Ropero,
        "dormitorio_538" | "dormitorio_429" | "living_51" | "living_53" | "living_55" | "living_57"
        | "living_63" | "living_65" | "living_67" | "living_70" => Contenedor::Comoda,
        "bano_1" | "bano_2" | "bano_5" | "bano_9" | "bano_12" => Contenedor::Botiquin,
        _ => return None,
    })
}

/// Celdas que ocupa un sprite de `size` metros: ancho y profundidad del pie.
/// La profundidad es el alto menos una celda (lo que se levanta sobre la pared de
/// atrás), y como mínimo una.
pub fn medida_en_celdas(size: [f32; 2]) -> (i64, i64) {
    let cw = size[0].round().max(1.0) as i64;
    let alto = size[1].round().max(1.0) as i64;
    // Lo angosto y alto (ropero, heladera, lámpara) apoya UNA celda: el resto es
    // su altura dibujada. Lo ancho apoya la mitad de su alto (una cama de 2×3
    // apoya 2; una mesa de 2×2, 1).
    let prof = if cw == 1 { 1 } else { ((alto as f32) * 0.5).round().max(1.0) as i64 };
    (cw, prof)
}

type Cache = Mutex<HashMap<(u8, i32, i64, i64), Arc<Vec<Mueble>>>>;

fn cache() -> &'static Cache {
    static C: OnceLock<Cache> = OnceLock::new();
    C.get_or_init(|| Mutex::new(HashMap::new()))
}

/// La casa (rect edificado y programa) que cubre esta celda, si hay.
pub fn casa_en(body: CelestialBody, tx: i64, tz: i64) -> Option<(i64, i64, i64, i64, &'static str)> {
    casa_en_z(body, 0, tx, tz)
}

/// Igual, pero en el nivel `z`: los interiores (`interiores`) viven en su propia banda de pisos y su
/// «casa» es el rectángulo del plano.
pub fn casa_en_z(body: CelestialBody, z: i32, tx: i64, tz: i64) -> Option<(i64, i64, i64, i64, &'static str)> {
    if crate::interiores::es_interior(z) {
        let r = crate::interiores::rect_de_z(z)?;
        return (tx >= r.0 && tz >= r.1 && tx < r.0 + r.2 && tz < r.1 + r.3).then_some(r);
    }
    if z != 0 || body != CelestialBody::Earth {
        return None;
    }
    let s = crate::geography::settlement_covering(body, tx, tz)?;
    let r = crate::city::roofed_rect(body, &s, tx, tz)?;
    (tx >= r.0 && tz >= r.1 && tx < r.0 + r.2 && tz < r.1 + r.3).then_some(r)
}

/// Los muebles de la casa de ese rect (cacheado: se calcula una vez).
pub fn muebles_de(body: CelestialBody, casa: (i64, i64, i64, i64, &'static str)) -> Arc<Vec<Mueble>> {
    muebles_de_z(body, 0, casa)
}

pub fn muebles_de_z(body: CelestialBody, z: i32, casa: (i64, i64, i64, i64, &'static str)) -> Arc<Vec<Mueble>> {
    let k = (body as u8, z, casa.0, casa.1);
    if let Some(v) = cache().lock().ok().and_then(|c| c.get(&k).cloned()) {
        return v;
    }
    let v = Arc::new(amueblar_z(body, z, casa));
    if let Ok(mut c) = cache().lock() {
        if c.len() > 512 {
            c.clear();
        }
        c.insert(k, v.clone());
    }
    v
}

/// El mueble anclado en esta celda, si hay.
pub fn mueble_anclado(body: CelestialBody, tx: i64, tz: i64) -> Option<Mueble> {
    mueble_anclado_z(body, 0, tx, tz)
}

pub fn mueble_anclado_z(body: CelestialBody, z: i32, tx: i64, tz: i64) -> Option<Mueble> {
    if topdown_tile(body, z, tx, tz) != Tile::WoodFloor {
        return None;
    }
    let casa = casa_en_z(body, z, tx, tz)?;
    muebles_de_z(body, z, casa).iter().copied().find(|m| m.ancla() == (tx, tz))
}

/// Los muebles de las casas que toca la caja `[x0, z0, x1, z1]`.
pub fn muebles_cerca(body: CelestialBody, caja: [f32; 4]) -> Vec<Mueble> {
    muebles_cerca_z(body, 0, caja)
}

pub fn muebles_cerca_z(body: CelestialBody, nivel: i32, caja: [f32; 4]) -> Vec<Mueble> {
    let mut vistas: Vec<(i64, i64)> = Vec::new();
    let mut fuera = Vec::new();
    for (x, z) in [(caja[0], caja[1]), (caja[2], caja[1]), (caja[0], caja[3]), (caja[2], caja[3])] {
        let Some(casa) = casa_en_z(body, nivel, x.floor() as i64, z.floor() as i64) else { continue };
        if vistas.contains(&(casa.0, casa.1)) {
            continue;
        }
        vistas.push((casa.0, casa.1));
        fuera.extend(muebles_de_z(body, nivel, casa).iter().copied());
    }
    fuera
}

fn es_casa(lote: &str) -> bool {
    crate::interiores::arquetipo_de(lote) == crate::interiores::Arq::Vivienda
        || matches!(
        lote,
        "building/casa_chica"
            | "building/casa_mediana"
            | "building/casa_grande"
            | "building/casa_country"
            | "building/villa"
            | "building/duplex"
            | "building/casa_hilera"
    )
}

/// Qué tabla de muebles le toca a un cuarto: la casa reparte sus ambientes de
/// mayor a menor; los demás edificios tienen un solo tema.
fn tabla_de_cuarto(lote: &str, orden: usize) -> Option<&'static [ExtraDecor]> {
    use crate::interior_siembra as m;
    if es_casa(lote) {
        const CASA: [&[ExtraDecor]; 5] = [m::LIVING, m::DORMITORIO, m::COCINA, m::BANO, m::DORMITORIO];
        return Some(CASA[orden % CASA.len()]);
    }
    crate::worldgen::tabla_interior_de(lote)
}

/// **Amuebla una casa entera.** Pura y determinista (el mismo mundo, los mismos
/// muebles), así que no se guarda nada.
pub fn amueblar(body: CelestialBody, casa: (i64, i64, i64, i64, &'static str)) -> Vec<Mueble> {
    amueblar_z(body, 0, casa)
}

pub fn amueblar_z(body: CelestialBody, nivel: i32, casa: (i64, i64, i64, i64, &'static str)) -> Vec<Mueble> {
    let (hx, hz, hw, hd, lote) = casa;
    let t = |x: i64, z: i64| topdown_tile(body, nivel, x, z);
    let es_muro = |x: i64, z: i64| matches!(t(x, z), Tile::WoodWall | Tile::StoneBrick | Tile::Window);
    let mut piso: HashSet<(i64, i64)> = HashSet::new();
    let mut puertas: Vec<(i64, i64)> = Vec::new();
    for z in hz..hz + hd {
        for x in hx..hx + hw {
            match t(x, z) {
                Tile::WoodFloor => {
                    piso.insert((x, z));
                }
                Tile::Door => puertas.push((x, z)),
                _ => {}
            }
        }
    }
    // Cuartos: relleno sobre el piso (las puertas separan).
    let mut sin_cuarto = piso.clone();
    let mut cuartos: Vec<Vec<(i64, i64)>> = Vec::new();
    let mut semillas: Vec<(i64, i64)> = piso.iter().copied().collect();
    semillas.sort();
    for s in semillas {
        if !sin_cuarto.remove(&s) {
            continue;
        }
        let mut cuarto = vec![s];
        let mut cola = VecDeque::from([s]);
        while let Some((x, z)) = cola.pop_front() {
            for (dx, dz) in [(1, 0), (-1, 0), (0, 1), (0, -1)] {
                let n = (x + dx, z + dz);
                if sin_cuarto.remove(&n) {
                    cuarto.push(n);
                    cola.push_back(n);
                }
            }
        }
        cuarto.sort();
        cuartos.push(cuarto);
    }
    cuartos.sort_by(|a, b| b.len().cmp(&a.len()).then(a[0].cmp(&b[0])));

    let mut fuera = Vec::new();
    for (orden, cuarto) in cuartos.iter().enumerate() {
        let Some(tabla) = tabla_de_cuarto(lote, orden) else { continue };
        if cuarto.len() < 4 || tabla.is_empty() {
            continue;
        }
        let celdas: HashSet<(i64, i64)> = cuarto.iter().copied().collect();
        // Las puertas del cuarto y sus celdas de entrada: se reservan (la de al
        // lado de la puerta y la siguiente) para que nunca se tape una entrada.
        let mut entradas: Vec<(i64, i64)> = Vec::new();
        let mut reservadas: HashSet<(i64, i64)> = HashSet::new();
        for &(px, pz) in &puertas {
            for (dx, dz) in [(1i64, 0i64), (-1, 0), (0, 1), (0, -1)] {
                let a = (px + dx, pz + dz);
                if celdas.contains(&a) {
                    entradas.push(a);
                    reservadas.insert(a);
                    let b = (a.0 + dx, a.1 + dz);
                    if celdas.contains(&b) {
                        reservadas.insert(b);
                    }
                }
            }
        }
        if entradas.is_empty() {
            entradas.push(cuarto[0]);
        }
        let mut ocupadas: HashSet<(i64, i64)> = HashSet::new();
        // **Lo que cada mueble TAPA a la vista**, además de lo que pisa (29 sep
        // 2026). Un ropero apoya una celda pero su dibujo sube tres: contra la
        // pared de un costado se apilaban uno bajo otro y cada uno tapaba la mitad
        // del de arriba ("los muebles están cortados"). Dos dibujos no se pisan.
        let mut a_la_vista: HashSet<(i64, i64)> = HashSet::new();
        let limite = (cuarto.len() as f32 * 0.34) as usize;
        let mut sem = hash_cell(0x3ab1_e0d0 ^ orden as u32 ^ (nivel as u32).wrapping_mul(0x9e37), hx, hz) | 1;
        let mut azar = move || {
            sem ^= sem << 13;
            sem ^= sem >> 17;
            sem ^= sem << 5;
            sem
        };
        // Candidatos: contra la pared norte (cualquier ancho) y contra las de los
        // costados (sólo los de una celda de ancho).
        let mut sitios: Vec<(i64, i64, bool)> = Vec::new();
        for &(x, z) in cuarto {
            if es_muro(x, z - 1) {
                sitios.push((x, z, false));
            } else if es_muro(x - 1, z) || es_muro(x + 1, z) {
                sitios.push((x, z, true));
            }
        }
        for i in (1..sitios.len()).rev() {
            let j = (azar() as usize) % (i + 1);
            sitios.swap(i, j);
        }
        let peso_total: u32 = tabla.iter().map(|d| d.weight).sum::<u32>().max(1);
        // Todo cuarto que PUEDA tener un mueble que se abre, lo tiene: es lo que
        // hace que entrar a una casa valga la pena (se prueba primero con ésos).
        let contenedores: Vec<&'static ExtraDecor> =
            tabla.iter().filter(|d| contenedor_de(d.key).is_some()).collect();
        let mut hay_contenedor = false;
        for (x, z, costado) in sitios {
            if ocupadas.len() >= limite {
                break;
            }
            for intento in 0..6 {
                let decor: &'static ExtraDecor = if !hay_contenedor && !contenedores.is_empty() && intento < 3 {
                    contenedores[(azar() as usize) % contenedores.len()]
                } else {
                    let mut r = azar() % peso_total;
                    tabla
                        .iter()
                        .find(|d| {
                            if r < d.weight {
                                true
                            } else {
                                r -= d.weight;
                                false
                            }
                        })
                        .unwrap_or(&tabla[0])
                };
                let (cw, prof) = medida_en_celdas(decor.size);
                if costado && cw > 1 {
                    continue;
                }
                let (x0, z0) = (x, z);
                let pie: Vec<(i64, i64)> = (0..prof)
                    .flat_map(|dz| (0..cw).map(move |dx| (x0 + dx, z0 + dz)))
                    .collect();
                if !pie
                    .iter()
                    .all(|c| celdas.contains(c) && !ocupadas.contains(c) && !reservadas.contains(c))
                {
                    continue;
                }
                // El respaldo cae sobre la PARED, no sobre otro cuarto.
                if !costado && !(0..cw).all(|dx| es_muro(x0 + dx, z0 - 1)) {
                    continue;
                }
                let alto = decor.size[1].round().max(1.0) as i64;
                let dibujo: Vec<(i64, i64)> = (prof - alto..prof)
                    .flat_map(|dz| (0..cw).map(move |dx| (x0 + dx, z0 + dz)))
                    .collect();
                if dibujo.iter().any(|c| a_la_vista.contains(c)) {
                    continue;
                }
                let prueba: HashSet<(i64, i64)> = ocupadas.iter().chain(pie.iter()).copied().collect();
                if !transitable(&celdas, &prueba, &entradas) {
                    continue;
                }
                ocupadas = prueba;
                a_la_vista.extend(dibujo);
                hay_contenedor |= contenedor_de(decor.key).is_some();
                fuera.push(Mueble { decor, x0, z0, cw, prof });
                break;
            }
        }
    }
    fuera
}

/// **La invariante del amueblado**: todas las celdas libres del cuarto están
/// unidas entre sí y con cada entrada. Si no, un mueble dejó un rincón encerrado
/// o tapó el paso a una puerta.
pub fn transitable(
    cuarto: &HashSet<(i64, i64)>,
    ocupadas: &HashSet<(i64, i64)>,
    entradas: &[(i64, i64)],
) -> bool {
    let libres: HashSet<(i64, i64)> = cuarto.difference(ocupadas).copied().collect();
    let Some(&inicio) = entradas.iter().find(|e| libres.contains(e)) else {
        return false;
    };
    let mut vistos = HashSet::from([inicio]);
    let mut cola = VecDeque::from([inicio]);
    while let Some((x, z)) = cola.pop_front() {
        for (dx, dz) in [(1, 0), (-1, 0), (0, 1), (0, -1)] {
            let n = (x + dx, z + dz);
            if libres.contains(&n) && vistos.insert(n) {
                cola.push_back(n);
            }
        }
    }
    vistos.len() == libres.len() && entradas.iter().all(|e| vistos.contains(e))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Casas reales del mundo: ningún mueble se encima con otro, ninguno queda
    /// fuera del piso y la mayoría de las casas tiene muebles.
    #[test]
    fn toda_casa_amueblada_se_camina() {
        let body = CelestialBody::Earth;
        let mut casas = 0;
        let mut con_muebles = 0;
        let mut vistas = HashSet::new();
        'fuera: for sx in -8..8 {
            for sz in -8..8 {
                let Some(s) = crate::geography::settlement_in(body, sx, sz) else { continue };
                let (cx, cz) = s.center;
                for dz in (-200..200).step_by(7) {
                    for dx in (-200..200).step_by(7) {
                        let Some(casa) = casa_en(body, cx + dx, cz + dz) else { continue };
                        if !vistas.insert((casa.0, casa.1)) {
                            continue;
                        }
                        casas += 1;
                        let muebles = amueblar(body, casa);
                        if !muebles.is_empty() {
                            con_muebles += 1;
                        }
                        let mut usadas = HashSet::new();
                        let mut vistas_dibujo = HashSet::new();
                        for m in &muebles {
                            // Lo que el dibujo tapa tampoco se comparte: un mueble
                            // no puede quedar cortado por el de abajo.
                            let alto = m.decor.size[1].round().max(1.0) as i64;
                            for dz in m.prof - alto..m.prof {
                                for dx in 0..m.cw {
                                    assert!(
                                        vistas_dibujo.insert((m.x0 + dx, m.z0 + dz)),
                                        "un mueble tapa a otro en {casa:?}"
                                    );
                                }
                            }
                            for dz in 0..m.prof {
                                for dx in 0..m.cw {
                                    let c = (m.x0 + dx, m.z0 + dz);
                                    assert!(usadas.insert(c), "muebles encimados en {casa:?}");
                                    assert_eq!(topdown_tile(body, 0, c.0, c.1), Tile::WoodFloor);
                                }
                            }
                            let c = m.caja();
                            assert!(c[0] < c[2] && c[1] < c[3], "caja vacía");
                        }
                        if casas >= 40 {
                            break 'fuera;
                        }
                    }
                }
            }
        }
        assert!(casas > 5, "no encontré casas ({casas})");
        assert!(con_muebles * 2 > casas, "{con_muebles} de {casas} con muebles");
    }

    #[test]
    fn un_mueble_que_corta_el_cuarto_no_pasa() {
        let cuarto: HashSet<(i64, i64)> = (0..3).flat_map(|x| (0..3).map(move |z| (x, z))).collect();
        let entrada = [(1, 2)];
        let fila: HashSet<(i64, i64)> = (0..3).map(|x| (x, 1)).collect();
        assert!(!transitable(&cuarto, &fila, &entrada));
        let rincon: HashSet<(i64, i64)> = [(0, 0)].into_iter().collect();
        assert!(transitable(&cuarto, &rincon, &entrada));
    }

    /// Los contenedores existen en las tablas y dan cosas que existen.
    #[test]
    fn los_contenedores_se_saquean() {
        use crate::interior_siembra as m;
        for tabla in [m::COCINA, m::DORMITORIO, m::BANO, m::LIVING] {
            assert!(tabla.iter().any(|d| contenedor_de(d.key).is_some()), "una tabla sin contenedores");
        }
        let mut algo = 0;
        for c in [Contenedor::Heladera, Contenedor::Alacena, Contenedor::Ropero, Contenedor::Comoda, Contenedor::Botiquin] {
            for sem in 0..50u32 {
                for (it, n) in c.botin(sem.wrapping_mul(2654435761)) {
                    assert!(n >= 1);
                    assert_ne!(crate::item_display_name(it), "Item", "{it} no es un ítem");
                    algo += 1;
                }
            }
        }
        assert!(algo > 100);
    }

    #[test]
    fn el_pie_es_la_parte_de_abajo() {
        assert_eq!(medida_en_celdas([1.0, 2.0]), (1, 1), "un ropero alto apoya una celda");
        assert_eq!(medida_en_celdas([2.0, 3.0]), (2, 2), "una cama apoya dos");
        assert_eq!(medida_en_celdas([1.0, 3.0]), (1, 1), "un ropero de tres celdas apoya una");
        assert_eq!(medida_en_celdas([2.0, 2.0]), (2, 1), "una mesa apoya una fila");
        assert_eq!(medida_en_celdas([1.0, 1.0]), (1, 1));
    }
}
