//! **PLANOS: casas, tiendas, talleres y torres que se pueden levantar de una vez** (jugador y NPC).
//!
//! El edificio modular (`edificio.rs`) se arma cuarto por cuarto: un jugador paciente llega a una
//! casa, pero nadie va a construir un pueblo así. Un **plano** es una receta de edificio completa:
//! qué cuartos, cuántos pisos, dónde van la puerta y las ventanas y el vano entre ambientes, para qué
//! sirve cada cuarto (vivienda, comercio, taller…) y cuánto cuesta. El MISMO plano lo usa el jugador
//! (ficha EDIFICIO → PLANO) y los constructores de los pueblos (`society::constructor_obra`), así que
//! un pueblo se llena de casas distintas, no de la misma caja repetida.
//!
//! Las fachadas siguen siendo las bandas del pack (planta baja, pisos, azotea, de a 9 celdas), y el
//! amueblado sale de los mismos muebles del pack que las casas generadas
//! (`interior_siembra`): `amueblar_cuarto` pone cada pieza contra la pared norte —como la dibuja el
//! pack— y **prueba que toda celda libre del cuarto siga unida a sus puertas**, igual que
//! `amueblado.rs`: un mueble que corta el paso no se pone.
//!
//! Todo es geometría determinista (sin Bevy): se prueba acá, se dibuja en `ha-game`.

use std::collections::{BTreeSet, VecDeque};

use crate::build_grid::{WallAxis, WallKind};
use crate::edificio::{Borde, Cuarto, Edificio, MODULO};
use crate::interior_siembra as tabla;
use crate::worldgen::{hash_cell, ExtraDecor};

/// Profundidad de un ambiente, en celdas (la de `edificios::FONDO_AMBIENTE`).
pub const FONDO: i64 = 5;

/// Para qué se usa un cuarto: decide con qué muebles se llena.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Rol {
    Living,
    Dormitorio,
    Cocina,
    Bano,
    Tienda,
    Taller,
    Oficina,
    Deposito,
}

impl Rol {
    fn muebles(self) -> &'static [ExtraDecor] {
        match self {
            Rol::Living => tabla::LIVING,
            Rol::Dormitorio => tabla::DORMITORIO,
            Rol::Cocina => tabla::COCINA,
            Rol::Bano => tabla::BANO,
            Rol::Tienda => tabla::SALA,
            Rol::Taller => tabla::SOTANO,
            Rol::Oficina => tabla::OFICINA,
            Rol::Deposito => tabla::SOTANO,
        }
    }
}

/// Qué es el edificio (para elegir plano según la necesidad del pueblo).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Uso {
    Vivienda,
    Comercio,
    Taller,
    Torre,
}

/// Una abertura del plano, relativa a la celda del frente: `(dx, dz, eje, tipo)`. **Cada módulo de 7
/// del frente lleva una puerta en su centro**: la banda del pack dibuja una puerta por módulo y la
/// abertura tiene que caer justo ahí (si no, se ve una puerta por la que no se pasa).
type Ab = (i64, i64, WallAxis, WallKind);

pub struct Plano {
    pub id: &'static str,
    pub nombre: &'static str,
    pub uso: Uso,
    /// `(dx, dz, ancho, alto)` relativos a la celda del frente del primer cuarto, y su rol.
    pub cuartos: &'static [(i64, i64, i64, i64, Rol)],
    pub pisos: i64,
    pub aberturas: &'static [Ab],
    pub madera: u32,
    pub piedra: u32,
}

use WallAxis::{H, V};
use WallKind::{Door, Doorway};
use Rol::*;

/// El catálogo. Todos los anchos son múltiplos de `MODULO` (7): la fachada del pack se dibuja en
/// bandas de 7 y un frente que no calza parte una ventana al medio.
pub static PLANOS: &[Plano] = &[
    Plano {
        id: "cabana",
        nombre: "Cabaña",
        uso: Uso::Vivienda,
        cuartos: &[(-3, -4, 7, FONDO, Living)],
        pisos: 0,
        aberturas: &[(0, 1, H, Door)],
        madera: 40,
        piedra: 10,
    },
    Plano {
        id: "casa",
        nombre: "Casa",
        uso: Uso::Vivienda,
        cuartos: &[(-3, -4, 7, FONDO, Living), (-3, -9, 7, FONDO, Dormitorio)],
        pisos: 1,
        aberturas: &[(0, 1, H, Door), (0, -4, H, Doorway)],
        madera: 60,
        piedra: 30,
    },
    Plano {
        id: "casa_doble",
        nombre: "Casa ancha",
        uso: Uso::Vivienda,
        cuartos: &[(-3, -4, 7, FONDO, Living), (4, -4, 7, FONDO, Cocina)],
        pisos: 1,
        aberturas: &[(0, 1, H, Door), (7, 1, H, Door), (4, -2, V, Doorway)],
        madera: 90,
        piedra: 40,
    },
    Plano {
        id: "residencia",
        nombre: "Residencia",
        uso: Uso::Vivienda,
        cuartos: &[
            (-3, -4, 7, FONDO, Living),
            (4, -4, 7, FONDO, Cocina),
            (-3, -9, 7, FONDO, Dormitorio),
            (4, -9, 7, FONDO, Bano),
        ],
        pisos: 2,
        aberturas: &[
            (0, 1, H, Door),
            (7, 1, H, Door),
            (4, -2, V, Doorway),
            (0, -4, H, Doorway),
            (7, -4, H, Doorway),
        ],
        madera: 160,
        piedra: 90,
    },
    Plano {
        id: "tienda",
        nombre: "Tienda",
        uso: Uso::Comercio,
        cuartos: &[(-3, -4, 7, FONDO, Tienda), (-3, -9, 7, FONDO, Deposito)],
        pisos: 1,
        aberturas: &[(0, 1, H, Door), (0, -4, H, Doorway)],
        madera: 70,
        piedra: 40,
    },
    Plano {
        id: "torre",
        nombre: "Torre",
        uso: Uso::Torre,
        cuartos: &[(-3, -4, 7, FONDO, Oficina)],
        pisos: 3,
        aberturas: &[(0, 1, H, Door)],
        madera: 80,
        piedra: 60,
    },
    Plano {
        id: "galpon",
        nombre: "Galpón",
        uso: Uso::Taller,
        cuartos: &[(-3, -4, 7, FONDO, Taller), (4, -4, 7, FONDO, Deposito)],
        pisos: 0,
        aberturas: &[(0, 1, H, Door), (7, 1, H, Door), (4, -2, V, Doorway)],
        madera: 90,
        piedra: 50,
    },
    Plano {
        id: "renta",
        nombre: "Departamentos",
        uso: Uso::Vivienda,
        cuartos: &[(-3, -4, 7, FONDO, Living), (4, -4, 7, FONDO, Dormitorio), (11, -4, 7, FONDO, Cocina)],
        pisos: 3,
        aberturas: &[(0, 1, H, Door), (7, 1, H, Door), (14, 1, H, Door)],
        madera: 200,
        piedra: 160,
    },
];

pub fn plano(id: &str) -> Option<&'static Plano> {
    PLANOS.iter().find(|p| p.id == id)
}

impl Plano {
    /// Celdas de planta (un piso).
    pub fn area(&self) -> i64 {
        self.cuartos.iter().map(|c| c.2 * c.3).sum()
    }

    /// El edificio, con la celda `(ax, az)` en el medio del frente del primer cuarto.
    pub fn edificio(&self, ax: i64, az: i64) -> Option<Edificio> {
        let mut it = self.cuartos.iter();
        let (dx, dz, w, h, _) = it.next()?;
        let mut e = Edificio::nuevo(Cuarto::new(ax + dx, az + dz, *w, *h));
        for (dx, dz, w, h, _) in it {
            if !e.ampliar(Cuarto::new(ax + dx, az + dz, *w, *h)) {
                return None;
            }
        }
        for _ in 0..self.pisos {
            e.apilar();
        }
        for (dx, dz, eje, kind) in self.aberturas {
            if !e.abrir(Borde::new(ax + dx, az + dz, *eje), *kind) {
                return None;
            }
        }
        Some(e)
    }

    /// El rol del cuarto que contiene la celda `(ax+dx, az+dz)`.
    fn rol_de(&self, c: &Cuarto, ax: i64, az: i64) -> Rol {
        self.cuartos
            .iter()
            .find(|(dx, dz, w, h, _)| c.x0 == ax + dx && c.z0 == az + dz && c.w == *w && c.h == *h)
            .map(|c| c.4)
            .unwrap_or(Rol::Living)
    }

    /// Todos los muebles del plano ya colocados (en coordenadas de mundo).
    pub fn muebles(&self, ax: i64, az: i64, semilla: u32) -> Vec<Colocado> {
        let Some(e) = self.edificio(ax, az) else { return Vec::new() };
        let mut out = Vec::new();
        for c in e.cuartos() {
            let rol = self.rol_de(c, ax, az);
            // Las celdas de ESTE cuarto pegadas a una abertura (puerta, vano) tienen que quedar libres.
            let mut puertas: Vec<(i64, i64)> = Vec::new();
            for (b, k) in e.aberturas() {
                if !k.is_entrance() {
                    continue;
                }
                for (x, z) in b.celdas() {
                    if c.contiene(x, z) {
                        puertas.push((x, z));
                    }
                }
            }
            out.extend(amueblar_cuarto(rol, c.x0, c.z0, c.w, c.h, &puertas, hash_cell(semilla, c.x0, c.z0)));
        }
        out
    }
}

/// Un mueble colocado: ocupa `x0..x0+cw` × `z0..z0+prof` y se dibuja con el pie en `z0+prof`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Colocado {
    pub key: &'static str,
    pub x0: i64,
    pub z0: i64,
    pub cw: i64,
    pub prof: i64,
}

/// **Amuebla un cuarto** `x0..x0+w` × `z0..z0+h` con los muebles del pack de su rol: una hilera
/// contra la pared norte y, a veces, una pieza angosta contra el costado oeste. `puertas` son las
/// celdas del cuarto pegadas a una abertura: nunca se tapan, y toda celda libre tiene que seguir
/// unida a ellas (si un mueble corta el paso, no se pone).
pub fn amueblar_cuarto(rol: Rol, x0: i64, z0: i64, w: i64, h: i64, puertas: &[(i64, i64)], semilla: u32) -> Vec<Colocado> {
    let mut s = semilla | 1;
    let mut azar = move |n: u32| {
        s ^= s << 13;
        s ^= s >> 17;
        s ^= s << 5;
        s % n.max(1)
    };
    // Piezas que entran: hasta 3 de fondo, sin pasar de la mitad del cuarto, enteras en celdas.
    let candidatas: Vec<&ExtraDecor> = rol
        .muebles()
        .iter()
        .filter(|d| {
            let (cw, pr) = (d.size[0].round() as i64, d.size[1].round() as i64);
            cw >= 1 && cw <= w - 2 && pr >= 1 && pr <= 3.min(h / 2 + 1)
        })
        .collect();
    let mut puestos: Vec<Colocado> = Vec::new();
    if candidatas.is_empty() {
        return puestos;
    }
    let libre_de_puertas = |x: i64, z: i64, cw: i64, pr: i64| -> bool {
        // Ni sobre una celda de puerta ni en la que está justo delante.
        !puertas.iter().any(|&(px, pz)| {
            (x - 1..x + cw + 1).contains(&px) && (z - 1..z + pr + 1).contains(&pz)
        })
    };
    let mut x = x0 + (azar(2) as i64);
    let mut intentos = 0;
    while x < x0 + w && puestos.len() < 5 && intentos < 24 {
        intentos += 1;
        let d = candidatas[azar(candidatas.len() as u32) as usize];
        let (cw, pr) = (d.size[0].round() as i64, d.size[1].round() as i64);
        if x + cw > x0 + w {
            x += 1;
            continue;
        }
        if !libre_de_puertas(x, z0, cw, pr) {
            x += 1;
            continue;
        }
        let m = Colocado { key: d.key, x0: x, z0, cw, prof: pr };
        puestos.push(m);
        if conectado(x0, z0, w, h, &puestos, puertas) {
            x += cw + azar(2) as i64;
        } else {
            puestos.pop();
            x += 1;
        }
    }
    // Una pieza angosta contra el costado oeste, debajo de la hilera norte.
    if azar(3) != 0 {
        let fondo_norte = puestos.iter().filter(|m| m.x0 == x0).map(|m| m.prof).max().unwrap_or(0);
        if let Some(d) = candidatas.iter().find(|d| d.size[0].round() as i64 == 1 && d.size[1].round() as i64 <= 2) {
            let pr = d.size[1].round() as i64;
            let z = z0 + fondo_norte;
            if z + pr <= z0 + h - 2 && libre_de_puertas(x0, z, 1, pr) {
                let m = Colocado { key: d.key, x0, z0: z, cw: 1, prof: pr };
                puestos.push(m);
                if !conectado(x0, z0, w, h, &puestos, puertas) {
                    puestos.pop();
                }
            }
        }
    }
    puestos
}

/// ¿Toda celda libre del cuarto está unida (4 vecinos) a alguna celda de puerta?
fn conectado(x0: i64, z0: i64, w: i64, h: i64, muebles: &[Colocado], puertas: &[(i64, i64)]) -> bool {
    let ocupada = |x: i64, z: i64| muebles.iter().any(|m| x >= m.x0 && x < m.x0 + m.cw && z >= m.z0 && z < m.z0 + m.prof);
    let mut libres: BTreeSet<(i64, i64)> = BTreeSet::new();
    for z in z0..z0 + h {
        for x in x0..x0 + w {
            if !ocupada(x, z) {
                libres.insert((x, z));
            }
        }
    }
    let Some(inicio) = puertas.iter().copied().find(|p| libres.contains(p)).or_else(|| libres.iter().next().copied()) else {
        return false;
    };
    let mut visto: BTreeSet<(i64, i64)> = BTreeSet::new();
    let mut cola = VecDeque::from([inicio]);
    visto.insert(inicio);
    while let Some((x, z)) = cola.pop_front() {
        for (dx, dz) in [(1, 0), (-1, 0), (0, 1), (0, -1)] {
            let n = (x + dx, z + dz);
            if libres.contains(&n) && visto.insert(n) {
                cola.push_back(n);
            }
        }
    }
    visto.len() == libres.len()
}

/// Qué clase de material pide cada acabado de fachada: `(tema, clase, unidades por cada 10 celdas)`.
/// La piedra sola da el hormigón; el ladrillo exige cerámica descubierta; el azul, metal.
pub const ACABADOS: [(&str, Option<&str>, u32); 3] = [("hormigon", None, 0), ("ladrillo", Some("ceramica"), 1), ("azul", Some("metal"), 1)];

/// Cuántas unidades de la clase de acabado cuesta cubrir la planta `area` por `pisos + 1` niveles.
pub fn costo_acabado(area: i64, pisos: i64, por_decena: u32) -> u32 {
    (((area * (pisos + 1)) as f32 / 10.0).ceil() as u32) * por_decena
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn todos_los_planos_son_edificios_validos_con_entrada_y_multiplos_del_modulo() {
        for p in PLANOS {
            let e = p.edificio(100, 100).unwrap_or_else(|| panic!("{} no arma", p.id));
            assert!(e.tiene_entrada(), "{} no tiene entrada", p.id);
            for (_, _, w, h, _) in p.cuartos {
                assert_eq!(w % MODULO, 0, "{}: el frente no es múltiplo del módulo", p.id);
                assert!(*h >= 3);
            }
            // Todo cuarto toca al resto (una casa no son cajas sueltas) lo garantiza `ampliar`.
            assert!(p.madera > 0 && p.piedra > 0);
            // Las ventanas y puertas son del contorno o de un tabique: `abrir` las aceptó todas.
            assert_eq!(e.cuartos().len(), p.cuartos.len());
        }
    }

    #[test]
    fn cada_modulo_del_frente_tiene_su_puerta_al_centro() {
        for p in PLANOS {
            for (dx, dz, w, h, _) in p.cuartos {
                if dz + h != 1 {
                    continue; // cuarto de atrás: no da a la calle
                }
                for m in 0..(w / MODULO) {
                    let centro = dx + m * MODULO + MODULO / 2;
                    assert!(
                        p.aberturas.iter().any(|(ax, az, eje, k)| *ax == centro && *az == 1 && *eje == H && *k == Door),
                        "{}: el módulo centrado en {centro} no tiene puerta",
                        p.id
                    );
                }
            }
        }
    }

    #[test]
    fn los_ids_son_unicos_y_hay_de_todos_los_usos() {
        let ids: BTreeSet<_> = PLANOS.iter().map(|p| p.id).collect();
        assert_eq!(ids.len(), PLANOS.len());
        for u in [Uso::Vivienda, Uso::Comercio, Uso::Taller, Uso::Torre] {
            assert!(PLANOS.iter().any(|p| p.uso == u), "{u:?}");
        }
        assert!(plano("casa").is_some() && plano("nada").is_none());
    }

    #[test]
    fn el_amueblado_es_determinista_entra_en_el_cuarto_y_deja_paso() {
        let mut con_muebles = 0;
        for p in PLANOS {
            for semilla in 1..=8u32 {
                let a = p.muebles(50, 70, semilla);
                assert_eq!(a, p.muebles(50, 70, semilla), "{} no es determinista", p.id);
                let e = p.edificio(50, 70).unwrap();
                for m in &a {
                    // Cada mueble está dentro de algún cuarto, y el pie (la última fila) también.
                    assert!(
                        e.cuartos().iter().any(|c| m.x0 >= c.x0 && m.x0 + m.cw <= c.x0 + c.w && m.z0 >= c.z0 && m.z0 + m.prof <= c.z0 + c.h),
                        "{}: {} se sale del cuarto",
                        p.id,
                        m.key
                    );
                }
                // Ningún mueble tapa una celda pegada a una abertura.
                for (b, k) in e.aberturas() {
                    if !k.is_entrance() {
                        continue;
                    }
                    for c in b.celdas() {
                        assert!(!a.iter().any(|m| c.0 >= m.x0 && c.0 < m.x0 + m.cw && c.1 >= m.z0 && c.1 < m.z0 + m.prof), "{}: un mueble tapa una puerta", p.id);
                    }
                }
                if !a.is_empty() {
                    con_muebles += 1;
                }
            }
        }
        assert!(con_muebles > PLANOS.len() * 4, "casi ningún plano se amuebla: {con_muebles}");
    }

    #[test]
    fn todo_mueble_existe_en_el_catalogo_de_arte() {
        for r in [Living, Dormitorio, Cocina, Bano, Tienda, Taller, Oficina, Deposito] {
            for d in r.muebles() {
                assert!(d.key.starts_with("furniture/"), "{}", d.key);
            }
        }
    }

    #[test]
    fn el_acabado_cuesta_segun_el_area() {
        assert_eq!(costo_acabado(45, 0, 1), 5);
        assert!(costo_acabado(45, 2, 1) > costo_acabado(45, 0, 1));
    }
}
