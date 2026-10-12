//! **Todo lo que se puede poner en el mundo tiene que poder texturizarse.**
//!
//! Son dos listas en dos crates distintos por una razón buena (el editor abre sin
//! el motor, ver `ha-assets/Cargo.toml`), y por eso mismo se separan solas: alguien
//! agrega un colocable nuevo en `ha-shared/placeables.rs` y su slot no existe en
//! HA Studio, así que el objeto queda para siempre como un cuadrado de color sin
//! forma de asignarle el pixel. Es exactamente el agujero que este trabajo vino a
//! cerrar, así que va cerrado con un test y no con una convención.

use std::collections::HashSet;

#[test]
fn todo_colocable_tiene_su_slot_en_el_editor() {
    let slots: HashSet<&str> = ha_assets::slots::game_slots().iter().map(|s| s.key).collect();
    let faltan: Vec<&str> = ha_shared::placeables::PLACEABLES
        .iter()
        .map(|a| a.art)
        .filter(|k| !slots.contains(k))
        .collect();
    assert!(
        faltan.is_empty(),
        "estos colocables no se pueden texturizar (no hay slot en HA Studio): {faltan:?}"
    );
}

#[test]
fn toda_pieza_de_construir_tiene_su_slot() {
    // Las piezas de celda y de borde también se dibujan con arte del editor.
    let slots: HashSet<&str> = ha_assets::slots::game_slots().iter().map(|s| s.key).collect();
    let faltan: Vec<&str> = ha_shared::build_grid::EDGE_PIECES
        .iter()
        .map(|e| e.art)
        .filter(|k| !slots.contains(k))
        .collect();
    assert!(faltan.is_empty(), "piezas de borde sin slot: {faltan:?}");
}
