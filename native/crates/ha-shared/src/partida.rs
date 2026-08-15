//! **LA SEMILLA DE LA PARTIDA: un mundo nuevo cada vez que abrís el juego.**
//!
//! Hasta ahora el mundo era UNO. `celestial_bodies::body_terrain_seed` devolvía
//! una constante compilada por cuerpo, así que la Tierra de la partida de hoy era
//! tile por tile la misma que la de ayer: los mismos montes, las mismas ciudades,
//! el mismo lago. Y como el estado del mundo se autoguarda en disco
//! (`inventory_persist::persist_dir`), lo que hubieras colocado la vez anterior
//! seguía ahí al empezar de nuevo. Pedido del usuario, textual: *"cada que yo
//! abra una partida, a menos que yo guarde la partida, se abra un mundo nuevo, o
//! sea todo nuevo, por ende no hay nada de lo que yo haya colocado"*.
//!
//! Acá vive el número que lo consigue: una semilla de PARTIDA que se mezcla con
//! la del cuerpo. Con 0 el mundo es exactamente el de siempre —que es lo que
//! necesitan los tests y los medidores, y por eso es el valor por defecto—; con
//! cualquier otro valor, todo el ruido del planeta se corre y sale otro mundo.
//!
//! **La plantilla del cuerpo NO se mueve.** Los continentes, la cordillera
//! central y las ciudades con nombre viven en `world_map` como datos autorados:
//! son la geografía que el juego promete. Lo que cambia con la semilla es el
//! ruido que la rellena — dónde cae cada monte, cada bosque, cada laguna y cada
//! pueblo sorteado.
//!
//! ## La época
//!
//! Media docena de funciones del mundo memorizan su respuesta por celda
//! (`relief::LEVEL_CACHE`, `geography::SITIOS`, `city`, `cuevas`). Esas cachés se
//! llenaron con la semilla vieja, así que cambiarla en caliente devolvería una
//! mezcla de dos mundos. [`epoca`] sube con cada cambio y entra en la clave de
//! todas ellas: al cambiar la semilla, lo memorizado deja de valer solo.

use std::sync::atomic::{AtomicU32, Ordering};

static SEMILLA: AtomicU32 = AtomicU32::new(0);
static EPOCA: AtomicU32 = AtomicU32::new(0);

/// La semilla de la partida en curso. `0` = el mundo de referencia.
pub fn semilla() -> u32 {
    SEMILLA.load(Ordering::Relaxed)
}

/// Cuántas veces cambió la semilla en este proceso. Entra en la clave de todas
/// las cachés de mundo para que lo memorizado con la semilla anterior no se
/// mezcle con lo nuevo.
pub fn epoca() -> u32 {
    EPOCA.load(Ordering::Relaxed)
}

/// Fija la semilla de la partida. Idempotente: fijar la misma no invalida nada.
pub fn set_semilla(s: u32) {
    if SEMILLA.swap(s, Ordering::Relaxed) != s {
        EPOCA.fetch_add(1, Ordering::Relaxed);
    }
}
