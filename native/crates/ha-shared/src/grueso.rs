//! **MODO GRUESO: el mundo visto de lejos, sin ríos y sin tocar las cachés.**
//!
//! Medido el 27 sep 2026 con gdb sobre el vuelo (`HA_FLYTEST`): en 16 de 18
//! muestras el juego estaba dentro de `world_map::platform`, llamada por
//! `rios::trazar_brazo`. Los ríos se trazan paso a paso y se cachean por bloques
//! de 128 celdas; desde el aire el terreno se muestrea cada decenas o cientos de
//! metros, así que CADA muestra cae en un bloque nuevo, la caché no sirve y cada
//! fallo recorre hasta 121 trazados. Un río de 3-6 celdas, visto a esa escala, no
//! ocupa ni un píxel.
//!
//! Dentro de [`con`]:
//! - `rios::perfil_del_cauce` y `rios::cauce_en` responden "no hay río";
//! - las cachés por celda (`LEVEL_CACHE`, `PARED_CACHE`, `CELL_CACHE`,
//!   `SUPERFICIE_CACHE`) se LEEN pero no se ESCRIBEN: un valor sin río no puede
//!   quedar guardado y aparecerle después al que camina por la orilla.
//!
//! Lo usa sólo el cliente, para lo que se ve de lejos: la malla del terreno en
//! vuelo, el disco del planeta y la capacidad de carga de la ecología. Nada de
//! la simulación autoritativa pasa por acá (regla 3: la sim no se bifurca).

use std::cell::Cell;

thread_local! {
    static PROFUNDIDAD: Cell<u32> = const { Cell::new(0) };
}

/// ¿Estamos muestreando en grueso?
#[inline]
pub fn activo() -> bool {
    PROFUNDIDAD.with(|p| p.get() > 0)
}

/// Corre `f` en modo grueso (anidable).
pub fn con<R>(f: impl FnOnce() -> R) -> R {
    struct Salida;
    impl Drop for Salida {
        fn drop(&mut self) {
            PROFUNDIDAD.with(|p| p.set(p.get() - 1));
        }
    }
    PROFUNDIDAD.with(|p| p.set(p.get() + 1));
    let _s = Salida;
    f()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::CelestialBody;

    /// Muestrear en grueso NO puede cambiar lo que después ve el que camina: las
    /// cachés no se escriben en ese modo.
    #[test]
    fn el_grueso_no_ensucia_las_caches() {
        let b = CelestialBody::Earth;
        // buscar una celda con río cerca del origen
        let mut celda = None;
        'b: for tz in (-3000..3000).step_by(7) {
            for tx in (-3000..3000).step_by(7) {
                if crate::rios::cauce_en(b, tx, tz) {
                    celda = Some((tx, tz));
                    break 'b;
                }
            }
        }
        let (tx, tz) = celda.expect("no hay ni un río cerca del origen: el test no prueba nada");
        // Las cachés son por hilo: un hilo muestrea en grueso y DESPUÉS en fino; otro,
        // sólo en fino. Si el grueso se hubiera quedado en la caché, difieren.
        let sucio = std::thread::spawn(move || {
            let _ = con(|| crate::relief::relief_field(b, tx, tz));
            let _ = con(|| crate::geography::biome_at(b, tx, tz));
            (crate::relief::relief_field(b, tx, tz), crate::geography::biome_at(b, tx, tz))
        })
        .join()
        .unwrap();
        let limpio = std::thread::spawn(move || {
            (crate::relief::relief_field(b, tx, tz), crate::geography::biome_at(b, tx, tz))
        })
        .join()
        .unwrap();
        assert_eq!(sucio, limpio, "el valor grueso quedó en la caché");
    }
}

#[cfg(test)]
mod ciudad_sin_lagos {
    /// **Una ciudad tierra adentro no tiene lagos en las manzanas.** Antes del 27 sep
    /// 2026 el casco de (−3248, 3313) tenía 2.623 celdas de agua, todas terreno bajo
    /// el mar hundido por el ruido fino (`relief::rellenar_pueblo`).
    #[test]
    fn una_ciudad_tierra_adentro_no_tiene_lagos() {
        use crate::CelestialBody;
        let b = CelestialBody::Earth;
        let (cx, cz) = (-3248i64, 3313i64);
        assert!(crate::geography::settlement_covering(b, cx, cz).is_some(), "la ciudad del caso se movió");
        let (mut agua, mut total) = (0, 0);
        for tz in (cz - 200..cz + 200).step_by(2) {
            for tx in (cx - 300..cx + 300).step_by(2) {
                total += 1;
                let t = crate::topdown::topdown_tile(b, 0, tx, tz);
                if matches!(t, crate::topdown::Tile::Water | crate::topdown::Tile::WaterDeep) {
                    agua += 1;
                }
            }
        }
        assert!(agua * 200 < total, "{agua} de {total} celdas del casco son agua");
    }
}
