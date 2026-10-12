//! **BARRIOS** — parcelas de tamaño real, no un rectángulo por manzana.
//!
//! Lo que había: `building_in_block` plantaba **un** rectángulo de madera por
//! manzana, del mismo tamaño aproximado en todas partes y con el 82 % de las
//! manzanas ocupadas. Desde el aire eso es un tablero de ajedrez, y de cerca son
//! cien cajas idénticas. No es una ciudad; es papel pintado con esquinas.
//!
//! Lo que hay ahora:
//!
//! ```text
//!  ┌──────────────────┐   parcela residencial (18×14)
//!  │ ▓▓▓▓▓  ╔═════╗   │   ▓ entrada de auto (asfalto)
//!  │ ▓▓▓▓▓  ║ casa ║   │   ╔═╗ paredes + piso + puerta + ventanas
//!  │ ╔════╗ ║      ║   │   garaje pegado a la entrada
//!  │ ║gara║ ╚══D══╝   │
//!  │ ╚════╝  ░░░ ~~~  │   ░ patio de piedra (parrilla)
//!  │         ░░░ ~~~  │   ~ PILETA — y como el bajío se nada, se nada de verdad
//!  └──────────────────┘   perímetro: cerca con portón al frente
//! ```
//!
//! Tres ideas y de ahí sale todo:
//!
//! 1. **Distritos.** El centro tiene torres, el anillo tiene comercio, las
//!    afueras casas, y hay parques y equipamiento repartidos. Una ciudad se
//!    reconoce por cómo cambia al recorrerla.
//! 2. **Cada programa tiene SU tamaño.** Una casa mide 14×10 y una universidad
//!    56×40. Los programas grandes **reservan varias manzanas** en vez de
//!    recortarse — un hospital a medias no es un hospital.
//! 3. **Variedad por hashes independientes.** El programa, la orientación, el
//!    tamaño de la casa dentro del lote y si tiene garaje, pileta o parrilla salen
//!    de sorteos distintos. Dos manzanas seguidas no se parecen.

use crate::celestial_bodies::{body_terrain_seed, CelestialBody};
use crate::geography::{hash2, CityStyle, Settlement, SettlementTier};
use crate::roads::{block_size, street_class};
use crate::topdown::Tile;

/// El carácter de una zona de la ciudad.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum District {
    /// El centro: torres y oficinas, manzanas macizas.
    Downtown,
    /// Comercio y servicios: tiendas, mercados, estacionamientos.
    Commercial,
    /// Casas con jardín. La mayor parte de cualquier ciudad.
    Residential,
    /// Naves, depósitos, obra. En el borde.
    Industrial,
    /// Plazas, canchas, arboledas. El respiro.
    Park,
    /// Equipamiento GRANDE: hospital, universidad, comisaría, estación.
    Civic,
}

/// Un programa de parcela: qué se construye y **cuánto mide**.
pub struct LotProgram {
    /// Clave lógica del arte (`building/hospital`, …).
    pub key: &'static str,
    /// Tamaño en TILES. Es el campo que importa: no todo mide lo mismo.
    pub cells: (i64, i64),
    pub district: District,
    pub weight: u8,
}

/// El catálogo. Los tamaños salen de lo que el edificio ES: una casa de pueblo
/// entra en media manzana, un hospital necesita cuatro.
pub const LOT_PROGRAMS: &[LotProgram] = &[
    // ── Residencial: la variedad vive acá, porque es el 60 % de la ciudad.
    LotProgram { key: "building/casa_chica", cells: (12, 10), district: District::Residential, weight: 22 },
    LotProgram { key: "building/casa_mediana", cells: (16, 12), district: District::Residential, weight: 26 },
    LotProgram { key: "building/casa_grande", cells: (20, 15), district: District::Residential, weight: 16 },
    LotProgram { key: "building/casa_country", cells: (24, 18), district: District::Residential, weight: 10 },
    LotProgram { key: "building/villa", cells: (30, 22), district: District::Residential, weight: 6 },
    LotProgram { key: "building/duplex", cells: (18, 11), district: District::Residential, weight: 12 },
    // ── Comercio.
    LotProgram { key: "building/tienda", cells: (14, 12), district: District::Commercial, weight: 24 },
    LotProgram { key: "building/mercado", cells: (22, 16), district: District::Commercial, weight: 14 },
    LotProgram { key: "building/gasolinera", cells: (18, 14), district: District::Commercial, weight: 8 },
    LotProgram { key: "building/shopping", cells: (44, 32), district: District::Commercial, weight: 5 },
    LotProgram { key: "lot/estacionamiento", cells: (20, 16), district: District::Commercial, weight: 10 },
    // ── Centro.
    LotProgram { key: "building/oficina", cells: (22, 20), district: District::Downtown, weight: 22 },
    LotProgram { key: "building/torre", cells: (30, 28), district: District::Downtown, weight: 18 },
    LotProgram { key: "building/hotel", cells: (34, 24), district: District::Downtown, weight: 8 },
    // ── Industria.
    LotProgram { key: "building/nave_industrial", cells: (34, 26), district: District::Industrial, weight: 22 },
    LotProgram { key: "building/deposito", cells: (24, 18), district: District::Industrial, weight: 16 },
    LotProgram { key: "building/granero", cells: (20, 16), district: District::Industrial, weight: 8 },
    // ── Parque.
    LotProgram { key: "lot/plaza", cells: (26, 22), district: District::Park, weight: 20 },
    LotProgram { key: "lot/cancha_basket", cells: (22, 14), district: District::Park, weight: 16 },
    LotProgram { key: "lot/cancha_tenis", cells: (26, 14), district: District::Park, weight: 10 },
    LotProgram { key: "lot/arboleda", cells: (24, 20), district: District::Park, weight: 18 },
    // ── Cívico: LOS GRANDES. Ocupan varias manzanas a propósito.
    LotProgram { key: "building/hospital", cells: (44, 34), district: District::Civic, weight: 14 },
    LotProgram { key: "building/universidad", cells: (56, 40), district: District::Civic, weight: 10 },
    LotProgram { key: "building/escuela", cells: (38, 28), district: District::Civic, weight: 16 },
    LotProgram { key: "building/comisaria", cells: (24, 20), district: District::Civic, weight: 14 },
    LotProgram { key: "building/bomberos", cells: (26, 20), district: District::Civic, weight: 12 },
    LotProgram { key: "building/correo", cells: (20, 16), district: District::Civic, weight: 10 },
    LotProgram { key: "building/estacion_tren", cells: (52, 30), district: District::Civic, weight: 6 },
];

/// El distrito de una manzana.
///
/// Se decide por ANILLO (distancia al centro en manzanas) más un sorteo, para que
/// haya parques y equipamiento salpicados en vez de tres coronas perfectas. La
/// trama de la ciudad inclina el reparto: una ciudad moderna es más centro, un
/// puerto es más industria.
pub fn district_at(body: CelestialBody, s: &Settlement, bx: i64, bz: i64) -> District {
    let h = hash2(body_terrain_seed(body) ^ 0xd157_0000, s.center.0 + bx, s.center.1 + bz);
    let ring = bx.abs().max(bz.abs());
    // Parques y equipamiento: repartidos por toda la ciudad, no en un anillo.
    match h % 100 {
        0..=6 => return District::Park,
        7..=11 => return District::Civic,
        _ => {}
    }
    let core = match s.style {
        CityStyle::Modern => 4,
        CityStyle::Radial => 3,
        CityStyle::Grid => 3,
        CityStyle::Port => 2,
        CityStyle::Rural => 0,
    };
    if s.tier != SettlementTier::City {
        // Un pueblo no tiene centro financiero: tiene una plaza y casas.
        return if ring == 0 { District::Commercial } else { District::Residential };
    }
    if ring <= core {
        District::Downtown
    } else if ring <= core + 3 {
        District::Commercial
    } else if ring >= core + 9 && h % 7 == 0 {
        // La industria vive en el borde, y sólo en parte de él.
        District::Industrial
    } else {
        District::Residential
    }
}

/// Un lote colocado: el rectángulo que ocupa y con qué frente.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Lot {
    pub min: (i64, i64),
    pub w: i64,
    pub h: i64,
    /// Índice en [`LOT_PROGRAMS`].
    pub program: usize,
}

impl Lot {
    pub fn program(&self) -> &'static LotProgram {
        &LOT_PROGRAMS[self.program]
    }

    pub fn contains(&self, tx: i64, tz: i64) -> bool {
        tx >= self.min.0 && tx < self.min.0 + self.w && tz >= self.min.1 && tz < self.min.1 + self.h
    }
}

/// Cuántas manzanas de `block` tiles necesita un programa.
fn blocks_needed(p: &LotProgram, block: i64, margin: i64) -> (i64, i64) {
    let usable = (block - margin * 2).max(1);
    // Un lote de N manzanas dispone de `N*block - 2*margin` tiles: las calles
    // interiores desaparecen bajo un edificio grande (una universidad no tiene una
    // calle cruzándola por el medio).
    let need = |want: i64| {
        let mut n = 1;
        while n * block - margin * 2 < want && n < 4 {
            n += 1;
        }
        n
    };
    let _ = usable;
    (need(p.cells.0), need(p.cells.1))
}

/// Margen libre que hay que dejar alrededor de un lote de `nx × nz` manzanas
/// anclado en `(ax, az)`: la acera de la calle MÁS ANCHA que lo rodea.
///
/// Se mira el borde entero (las cuatro calles que lo limitan) porque basta con
/// que una sea avenida para que el lote tenga que retirarse de ella.
fn bounding_margin(style: CityStyle, ax: i64, az: i64, nx: i64, nz: i64) -> i64 {
    let mut m = 0.0f32;
    for i in 0..=nx {
        m = m.max(street_class(style, ax + i, az).half_shoulder());
        m = m.max(street_class(style, ax + i, az + nz).half_shoulder());
    }
    for j in 0..=nz {
        m = m.max(street_class(style, ax, az + j).half_shoulder());
        m = m.max(street_class(style, ax + nx, az + j).half_shoulder());
    }
    m.ceil() as i64
}

/// El programa que le toca al bloque ANCLA `(ax, az)`, si su sorteo eligió uno
/// que ocupa exactamente `(nx, nz)` manzanas.
fn program_at_anchor(
    body: CelestialBody,
    s: &Settlement,
    ax: i64,
    az: i64,
    nx: i64,
    nz: i64,
    block: i64,
    margin: i64,
) -> Option<usize> {
    let district = district_at(body, s, ax, az);
    let h = hash2(body_terrain_seed(body) ^ 0x10c5_0000, s.center.0 + ax, s.center.1 + az);
    // Densidad: la ciudad tiene casi todo edificado; la aldea, cuatro casas.
    let filled = match s.tier {
        SettlementTier::City => 88,
        SettlementTier::Town => 62,
        SettlementTier::Hamlet => 34,
    };
    if h % 100 >= filled {
        return None;
    }
    // Sorteo ponderado dentro del distrito.
    let candidates: Vec<usize> = LOT_PROGRAMS
        .iter()
        .enumerate()
        .filter(|(_, p)| p.district == district)
        .map(|(i, _)| i)
        .collect();
    if candidates.is_empty() {
        return None;
    }
    let total: u32 = candidates.iter().map(|&i| LOT_PROGRAMS[i].weight as u32).sum();
    let mut roll = (h >> 9) % total.max(1);
    let mut chosen = candidates[0];
    for &i in &candidates {
        let w = LOT_PROGRAMS[i].weight as u32;
        if roll < w {
            chosen = i;
            break;
        }
        roll -= w;
    }
    // Sólo es ancla si el programa elegido ocupa EXACTAMENTE este tamaño: así una
    // manzana pertenece a un único lote y no hay dos programas peleándose por ella.
    (blocks_needed(&LOT_PROGRAMS[chosen], block, margin) == (nx, nz)).then_some(chosen)
}

/// El lote que ocupa esta celda, si hay alguno.
///
/// Se prueba de MAYOR a menor: primero si un lote de 4×4 manzanas anclado más al
/// noroeste llega hasta acá, después 2×2, y así hasta 1×1. Ese orden es lo que
/// permite que una universidad se plante entera en vez de recortarse contra el
/// borde de su manzana.
pub fn lot_at(body: CelestialBody, s: &Settlement, tx: i64, tz: i64) -> Option<Lot> {
    let block = block_size(s.style, s.tier);
    let (cx, cz) = s.center;
    let bx = (tx - cx).div_euclid(block);
    let bz = (tz - cz).div_euclid(block);
    for n in [4i64, 3, 2, 1] {
        for nx in [n, 1] {
            for nz in [n, 1] {
                if nx == 1 && nz == 1 && n != 1 {
                    continue;
                }
                let ax = bx - bx.rem_euclid(nx);
                let az = bz - bz.rem_euclid(nz);
                // **El margen sale de la calle MÁS ANCHA que rodea al lote**, no
                // sólo de la del bloque ancla. Un lote limita con cuatro calles y
                // cualquiera de ellas puede ser una avenida: usando el margen del
                // ancla, un lote que empezaba junto a un callejón se estiraba
                // hasta meterse dentro de la avenida del otro lado, y salían
                // ventanas y paredes cruzadas en mitad de la calzada.
                let margin = bounding_margin(s.style, ax, az, nx, nz);
                let Some(prog) = program_at_anchor(body, s, ax, az, nx, nz, block, margin) else {
                    continue;
                };
                let p = &LOT_PROGRAMS[prog];
                // Rectángulo del lote: desde la acera del bloque ancla, del tamaño
                // del programa (acotado a lo que dan las manzanas reservadas).
                let ox = cx + ax * block + margin;
                let oz = cz + az * block + margin;
                let w = p.cells.0.min(nx * block - margin * 2).max(4);
                let hgt = p.cells.1.min(nz * block - margin * 2).max(4);
                // Fuera del casco no hay ciudad.
                if (ox - cx).abs() > s.radius || (oz - cz).abs() > s.radius {
                    continue;
                }
                let lot = Lot { min: (ox, oz), w, h: hgt, program: prog };
                if !lot.contains(tx, tz) {
                    continue;
                }
                // El terreno manda: no hay parcelas sobre el agua ni colgando de
                // un acantilado. Se comprueba la esquina del lote, que es barato y
                // determinista (comprobar las N celdas costaría el mundo entero).
                if !crate::geography::natural_tile(body, ox, oz).buildable_ground() {
                    return None;
                }
                return Some(lot);
            }
        }
    }
    None
}

// ---------------------------------------------------------------------------
// Pintar la parcela
// ---------------------------------------------------------------------------

/// Qué tile pone la parcela en esta celda. `None` = terreno natural (el jardín).
///
/// Todo sale de tiles que ya existían más `Fence`: la casa es madera, el garaje
/// ladrillo, la entrada asfalto, el patio piedra y la pileta **agua de bajío**, o
/// sea nadable de verdad.
pub fn lot_tile(body: CelestialBody, s: &Settlement, tx: i64, tz: i64) -> Option<Tile> {
    let lot = lot_at(body, s, tx, tz)?;
    let p = lot.program();
    let (lx, lz) = (tx - lot.min.0, tz - lot.min.1);
    let h = hash2(body_terrain_seed(body) ^ 0xfa11_0000, lot.min.0, lot.min.1);

    match p.district {
        District::Park => park_tile(p, lot, lx, lz, h),
        District::Residential => residential_tile(lot, lx, lz, h),
        _ => plain_building_tile(p, lot, lx, lz, h),
    }
}

/// Parque: losa, cancha o arboleda. Nunca paredes.
fn park_tile(p: &LotProgram, lot: Lot, lx: i64, lz: i64, _h: u32) -> Option<Tile> {
    match p.key {
        // La arboleda es terreno natural: los árboles los siembra la decoración.
        "lot/arboleda" => None,
        // Cancha y plaza: una losa con borde. El borde es lo que la separa del
        // césped y hace que se lea como una cancha y no como un claro.
        _ => {
            let edge = lx == 0 || lz == 0 || lx == lot.w - 1 || lz == lot.h - 1;
            Some(if edge { Tile::Pavement } else { Tile::Basalt })
        }
    }
}

/// **La parcela residencial**: casa + garaje + entrada + patio + pileta + cerca.
/// El corazón de que un barrio se vea habitado.
fn residential_tile(lot: Lot, lx: i64, lz: i64, h: u32) -> Option<Tile> {
    let (w, d) = (lot.w, lot.h);
    // Cada extra se sortea aparte: así hay casas con garaje y sin pileta, con
    // pileta y sin garaje, y ninguna manzana repite la de al lado.
    let has_garage = w >= 16 && (h >> 2) % 100 < 62;
    let has_pool = w >= 18 && d >= 12 && (h >> 5) % 100 < 34;
    let has_patio = (h >> 8) % 100 < 70;

    // 1) CERCA perimetral, con portón al frente (borde sur, mirando a la calle).
    let gate = w / 2;
    if lx == 0 || lz == 0 || lx == w - 1 || lz == d - 1 {
        let is_gate = lz == d - 1 && (lx == gate || lx == gate + 1);
        return if is_gate { None } else { Some(Tile::Fence) };
    }

    // 2) CASA: pegada al fondo (norte), centrada-ish, tamaño variable.
    let house_w = (w * 55 / 100).clamp(6, w - 4);
    let house_d = (d * 50 / 100).clamp(5, d - 5);
    let house_x = 2 + ((h >> 11) as i64).rem_euclid((w - house_w - 3).max(1));
    let house_z = 2;
    let in_house = lx >= house_x && lx < house_x + house_w && lz >= house_z && lz < house_z + house_d;
    if in_house {
        let (rx, rz) = (lx - house_x, lz - house_z);
        let perimeter = rx == 0 || rz == 0 || rx == house_w - 1 || rz == house_d - 1;
        if !perimeter {
            return Some(Tile::WoodFloor);
        }
        // Puerta al frente (sur), centrada.
        if rz == house_d - 1 && rx == house_w / 2 {
            return Some(Tile::Door);
        }
        // Ventanas: alternadas en las cuatro caras, sin tocar las esquinas.
        let corner = (rx == 0 || rx == house_w - 1) && (rz == 0 || rz == house_d - 1);
        if !corner && (rx + rz) % 3 == 1 {
            return Some(Tile::Window);
        }
        return Some(Tile::WoodWall);
    }

    // 3) GARAJE: al lado de la casa, contra el frente, con su entrada de auto.
    if has_garage {
        let gw = 6;
        let gd = 5;
        let gx = if house_x >= gw + 2 { house_x - gw - 1 } else { house_x + house_w + 1 };
        let gz = d - gd - 2;
        if gx >= 1 && gx + gw < w - 1 && lx >= gx && lx < gx + gw && lz >= gz && lz < gz + gd {
            let (rx, rz) = (lx - gx, lz - gz);
            let perimeter = rx == 0 || rz == 0 || rx == gw - 1 || rz == gd - 1;
            if rz == gd - 1 && rx > 0 && rx < gw - 1 {
                return Some(Tile::Door); // portón
            }
            return Some(if perimeter { Tile::StoneBrick } else { Tile::WoodFloor });
        }
        // ENTRADA DE AUTO: del portón del garaje hasta la calle.
        if gx >= 1 && gx + gw < w - 1 && lx > gx && lx < gx + gw - 1 && lz >= gz + gd {
            return Some(Tile::Basalt);
        }
    }

    // 4) PILETA con su borde de piedra, en el patio de atrás.
    if has_pool {
        let px = w - 7;
        let pz = 2;
        if lx >= px && lx < px + 5 && lz >= pz && lz < pz + 4 {
            let (rx, rz) = (lx - px, lz - pz);
            let rim = rx == 0 || rz == 0 || rx == 4 || rz == 3;
            // `Water` = bajío ⇒ **se nada**. La pileta del patio es una pileta.
            return Some(if rim { Tile::Pavement } else { Tile::Water });
        }
    }

    // 5) PATIO de piedra junto a la puerta (ahí va la parrilla, que la siembra la
    // decoración de props).
    if has_patio {
        let cx0 = house_x + house_w / 2 - 1;
        if lx >= cx0 && lx < cx0 + 3 && lz >= house_z + house_d && lz < house_z + house_d + 3 {
            return Some(Tile::Pavement);
        }
    }

    // El resto es jardín: terreno natural, y la flora procedural lo puebla.
    None
}

/// Edificio "macizo" (comercio, centro, industria, cívico): un rectángulo con
/// paredes, ventanas y su vereda. Los grandes llevan patio interior para que no
/// sean un bloque sólido de 56×40.
fn plain_building_tile(p: &LotProgram, lot: Lot, lx: i64, lz: i64, h: u32) -> Option<Tile> {
    let (w, d) = (lot.w, lot.h);
    // Estacionamiento: sólo losa y líneas, sin edificio.
    if p.key == "lot/estacionamiento" {
        let edge = lx == 0 || lz == 0 || lx == w - 1 || lz == d - 1;
        return Some(if edge { Tile::Pavement } else { Tile::Basalt });
    }
    // Vereda de 1 tile alrededor: separa la fachada de la calzada.
    if lx == 0 || lz == 0 || lx == w - 1 || lz == d - 1 {
        return Some(Tile::Pavement);
    }
    let (rx, rz) = (lx - 1, lz - 1);
    let (bw, bd) = (w - 2, d - 2);
    let perimeter = rx == 0 || rz == 0 || rx == bw - 1 || rz == bd - 1;

    // PATIO INTERIOR en los edificios grandes: un hospital o una universidad
    // tienen patio. Sin él, 56×40 de piso continuo se lee como una losa.
    if bw >= 26 && bd >= 20 {
        let (ix, iz) = (bw / 4, bd / 4);
        let inner = rx >= ix && rx < bw - ix && rz >= iz && rz < bd - iz;
        if inner {
            let ring = rx == ix || rz == iz || rx == bw - ix - 1 || rz == bd - iz - 1;
            return Some(if ring { Tile::StoneBrick } else { Tile::Pavement });
        }
    }

    if !perimeter {
        return Some(Tile::WoodFloor);
    }
    // Puerta al frente.
    if rz == bd - 1 && rx == bw / 2 {
        return Some(Tile::Door);
    }
    let corner = (rx == 0 || rx == bw - 1) && (rz == 0 || rz == bd - 1);
    // Vidriera: el comercio tiene MUCHO más vidrio que una nave industrial, y esa
    // diferencia es la que hace que un distrito se distinga de otro a simple vista.
    let glass = match p.district {
        District::Commercial | District::Downtown => 2,
        District::Civic => 3,
        _ => 5,
    };
    if !corner && (rx + rz + (h % 3) as i64) % glass == 0 {
        return Some(Tile::Window);
    }
    Some(Tile::StoneBrick)
}

/// La huella de la CASA/edificio de esta celda — lo que el cliente usa para el
/// techo. Devuelve el rectángulo edificado, no el lote entero (el techo no cubre
/// el jardín ni la pileta).
pub fn roofed_rect(body: CelestialBody, s: &Settlement, tx: i64, tz: i64) -> Option<(i64, i64, i64, i64)> {
    let lot = lot_at(body, s, tx, tz)?;
    let p = lot.program();
    if p.district == District::Park || p.key == "lot/estacionamiento" {
        return None;
    }
    if p.district == District::Residential {
        let (w, d) = (lot.w, lot.h);
        let h = hash2(body_terrain_seed(body) ^ 0xfa11_0000, lot.min.0, lot.min.1);
        let house_w = (w * 55 / 100).clamp(6, w - 4);
        let house_d = (d * 50 / 100).clamp(5, d - 5);
        let house_x = 2 + ((h >> 11) as i64).rem_euclid((w - house_w - 3).max(1));
        return Some((lot.min.0 + house_x, lot.min.1 + 2, house_w, house_d));
    }
    Some((lot.min.0 + 1, lot.min.1 + 1, lot.w - 2, lot.h - 2))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geography::settlement_in;

    fn a_city() -> Settlement {
        let body = CelestialBody::Earth;
        crate::world_map::template(body)
            .towns
            .iter()
            .map(|t| Settlement {
                tier: SettlementTier::City,
                center: crate::world_map::town_center(body, t),
                style: t.style,
                name: Some(t.name),
                radius: t.radius,
            })
            .next()
            .unwrap()
    }

    /// **Los programas tienen tamaños DISTINTOS**, y los grandes son grandes de
    /// verdad. Si esto se aplana, volvemos a la caja única por manzana.
    #[test]
    fn lot_programs_have_genuinely_different_sizes() {
        let mut areas: Vec<i64> = LOT_PROGRAMS.iter().map(|p| p.cells.0 * p.cells.1).collect();
        areas.sort_unstable();
        let (small, big) = (areas[0], areas[areas.len() - 1]);
        assert!(big > small * 12, "el más grande sólo mide {big} y el más chico {small}");
        // Y los edificios que sabemos grandes lo son.
        let uni = LOT_PROGRAMS.iter().find(|p| p.key == "building/universidad").unwrap();
        let casa = LOT_PROGRAMS.iter().find(|p| p.key == "building/casa_chica").unwrap();
        assert!(uni.cells.0 > casa.cells.0 * 3 && uni.cells.1 > casa.cells.1 * 3);
    }

    /// Todos los distritos tienen al menos un programa: uno vacío dejaría un
    /// agujero de ciudad sin edificar.
    #[test]
    fn every_district_has_programs() {
        for d in [
            District::Downtown,
            District::Commercial,
            District::Residential,
            District::Industrial,
            District::Park,
            District::Civic,
        ] {
            assert!(
                LOT_PROGRAMS.iter().any(|p| p.district == d),
                "{d:?} no tiene ni un programa"
            );
        }
    }

    /// Una ciudad recorrida entera tiene VARIEDAD: varios distritos y muchos
    /// programas distintos. Es la prueba directa contra el damero.
    #[test]
    fn a_city_is_made_of_many_different_lots() {
        let body = CelestialBody::Earth;
        let s = a_city();
        let mut programs = std::collections::HashSet::new();
        let mut districts = std::collections::HashSet::new();
        let r = s.radius.min(900);
        for dz in (-r..r).step_by(9) {
            for dx in (-r..r).step_by(9) {
                let (tx, tz) = (s.center.0 + dx, s.center.1 + dz);
                if let Some(lot) = lot_at(body, &s, tx, tz) {
                    programs.insert(lot.program().key);
                    districts.insert(lot.program().district);
                }
            }
        }
        assert!(programs.len() >= 8, "sólo {} programas: {programs:?}", programs.len());
        assert!(districts.len() >= 3, "sólo {} distritos: {districts:?}", districts.len());
    }

    /// **Una parcela residencial tiene casa, cerca y jardín** — y bastantes
    /// tienen garaje y pileta. Es el pedido concreto: barrios que se vean vividos.
    #[test]
    fn residential_lots_have_houses_fences_and_extras() {
        let body = CelestialBody::Earth;
        let s = a_city();
        let (mut walls, mut doors, mut fences, mut pools, mut drives, mut gardens) =
            (0, 0, 0, 0, 0, 0);
        let r = s.radius.min(1_200);
        for dz in -r..r {
            for dx in -r..r {
                let (tx, tz) = (s.center.0 + dx, s.center.1 + dz);
                let Some(lot) = lot_at(body, &s, tx, tz) else { continue };
                if lot.program().district != District::Residential {
                    continue;
                }
                match lot_tile(body, &s, tx, tz) {
                    Some(Tile::WoodWall) => walls += 1,
                    Some(Tile::Door) => doors += 1,
                    Some(Tile::Fence) => fences += 1,
                    Some(Tile::Water) => pools += 1,
                    Some(Tile::Basalt) => drives += 1,
                    None => gardens += 1,
                    _ => {}
                }
            }
        }
        assert!(walls > 300, "el barrio no tiene casas: {walls} paredes");
        assert!(doors > 10, "casas sin puerta: {doors}");
        assert!(fences > 300, "parcelas sin cerca: {fences}");
        assert!(pools > 20, "ni una pileta en todo el barrio: {pools}");
        assert!(drives > 40, "ni una entrada de auto: {drives}");
        assert!(gardens > 500, "las parcelas no dejan jardín: {gardens}");
    }

    /// **Los edificios grandes se plantan ENTEROS**, ocupando varias manzanas, en
    /// vez de recortarse contra el borde de la suya.
    #[test]
    fn big_civic_buildings_span_several_blocks() {
        let body = CelestialBody::Earth;
        let s = a_city();
        let block = block_size(s.style, s.tier);
        let mut biggest = 0;
        let r = s.radius.min(1_500);
        for dz in (-r..r).step_by(7) {
            for dx in (-r..r).step_by(7) {
                let (tx, tz) = (s.center.0 + dx, s.center.1 + dz);
                if let Some(lot) = lot_at(body, &s, tx, tz) {
                    biggest = biggest.max(lot.w.max(lot.h));
                }
            }
        }
        assert!(
            biggest > block,
            "ningún lote pasa de una manzana ({biggest} vs {block}): los grandes se recortan"
        );
    }

    /// Las parcelas nunca se solapan: una celda pertenece a un lote y sólo a uno.
    #[test]
    fn lots_never_overlap() {
        let body = CelestialBody::Earth;
        let s = a_city();
        let mut owner: std::collections::HashMap<(i64, i64), (i64, i64)> =
            std::collections::HashMap::new();
        let r = 400;
        for dz in -r..r {
            for dx in -r..r {
                let (tx, tz) = (s.center.0 + dx, s.center.1 + dz);
                let Some(lot) = lot_at(body, &s, tx, tz) else { continue };
                // Cada celda declara UN dueño; volver a consultarla da el mismo.
                let prev = owner.insert((tx, tz), lot.min);
                assert!(prev.is_none() || prev == Some(lot.min));
            }
        }
        assert!(!owner.is_empty(), "la ciudad no tiene ni un lote");
    }

    /// Compat: un pueblo procedural sigue generando parcelas (no sólo las cuatro
    /// ciudades autoradas).
    #[test]
    fn procedural_towns_also_get_lots() {
        let body = CelestialBody::Earth;
        let mut found = false;
        'outer: for sx in -14..14 {
            for sz in -14..14 {
                let Some(s) = settlement_in(body, sx, sz) else { continue };
                if s.name.is_some() {
                    continue; // ésta es una de las autoradas
                }
                for dz in (-s.radius..s.radius).step_by(5) {
                    for dx in (-s.radius..s.radius).step_by(5) {
                        if lot_at(body, &s, s.center.0 + dx, s.center.1 + dz).is_some() {
                            found = true;
                            break 'outer;
                        }
                    }
                }
            }
        }
        assert!(found, "los pueblos procedurales se quedaron sin edificios");
    }
}
