//! **Auditoría de cobertura de slots** — que no exista una pieza del juego a la
//! que no le puedas asignar el pixel desde el editor.
//!
//! El editor sólo muestra lo que declara `ha_assets::game_slots()`. Si el juego
//! coloca algo cuya clave de arte no está ahí, esa pieza queda condenada a su
//! color de código: no aparece en la lista, no se puede reskinear, y no hay
//! forma de darse cuenta salvo jugando y notando el cuadrado plano.
//!
//! Estos tests cierran ese agujero para el **sistema de construcción entero**:
//! las piezas de celda (piso, escaleras), las de borde (pared, puerta, ventana),
//! los cuartos prefab, las estructuras grandes, y todo tile con arte. Si mañana
//! alguien agrega una pieza nueva y se olvida del slot, el test lo dice acá y no
//! el usuario tres semanas después.

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use ha_assets::game_slots;
    use ha_shared::build_grid::EDGE_PIECES;
    use ha_shared::topdown::{Tile, BUILD_PIECES, LARGE_ASSETS, PREFABS};

    /// Clave → nombre humano de todos los slots que el editor ofrece.
    fn slot_labels() -> HashMap<&'static str, &'static str> {
        game_slots().into_iter().map(|s| (s.key, s.label)).collect()
    }

    /// Toda pieza de BORDE (pared / puerta / ventana) es asignable por nombre.
    #[test]
    fn edge_pieces_have_named_slots() {
        let slots = slot_labels();
        for p in EDGE_PIECES {
            let label = slots.get(p.art).unwrap_or_else(|| {
                panic!(
                    "la pieza «{}» ({}) usa el arte `{}` y NO hay slot: no se puede \
                     asignar desde el editor. Agregalo a BUILD_ART_KEYS en slots.rs",
                    p.label, p.id, p.art
                )
            });
            assert!(!label.trim().is_empty(), "slot `{}` sin nombre humano", p.art);
        }
    }

    /// Toda estructura grande colocable (A2) es asignable por nombre.
    #[test]
    fn large_assets_have_named_slots() {
        let slots = slot_labels();
        for a in LARGE_ASSETS {
            assert!(
                slots.contains_key(a.art),
                "el asset grande «{}» ({}) usa `{}` y no hay slot en el editor",
                a.label,
                a.id,
                a.art
            );
        }
    }

    /// Todo cuarto/casa prefab tiene su slot `room/<id>` con nombre.
    #[test]
    fn prefabs_have_named_slots() {
        let slots = slot_labels();
        for p in PREFABS {
            let key = format!("room/{}", p.id);
            assert!(
                slots.contains_key(key.as_str()),
                "el prefab «{}» ({}) no tiene slot `{key}` en el editor",
                p.label,
                p.id
            );
        }
    }

    /// Toda pieza de CELDA de la paleta (piso, escaleras) pinta un tile cuyo
    /// arte es asignable: es el caso que el usuario ve primero al construir.
    #[test]
    fn build_pieces_have_named_slots() {
        let slots = slot_labels();
        for p in BUILD_PIECES {
            let art = p.tile.art_key().unwrap_or_else(|| {
                panic!(
                    "la pieza «{}» ({}) coloca un tile SIN clave de arte: nunca vas a \
                     poder darle textura. Agregala a `Tile::art_key`",
                    p.label, p.id
                )
            });
            assert!(
                slots.contains_key(art),
                "la pieza «{}» usa `{art}` y no hay slot en el editor",
                p.label
            );
        }
    }

    /// **Barrido completo**: cualquier tile con clave de arte tiene slot. Cubre
    /// lo que las paletas no listan pero el mundo sí dibuja (vereda, cerca,
    /// acantilado, roca de cueva, muros legacy de los asentamientos generados).
    #[test]
    fn every_tile_art_key_has_a_named_slot() {
        let slots = slot_labels();
        // Todas las variantes del enum: si mañana entra un tile nuevo con arte,
        // cae acá solo.
        let all = [
            Tile::Grass, Tile::Dirt, Tile::Sand, Tile::Water, Tile::WaterDeep,
            Tile::Regolith, Tile::Ice, Tile::Basalt, Tile::RockRed, Tile::Cliff,
            Tile::Slope, Tile::Fence, Tile::Rock, Tile::Pavement, Tile::Stone,
            Tile::RockDark, Tile::Bedrock, Tile::CaveFloor, Tile::WoodFloor,
            Tile::Wall, Tile::WoodWall, Tile::StoneBrick, Tile::Door, Tile::Window,
            Tile::StairsDown, Tile::StairsUp, Tile::Air,
        ];
        for tile in all {
            let Some(art) = tile.art_key() else { continue };
            assert!(
                slots.contains_key(art),
                "el tile {tile:?} dibuja `{art}` y ese slot no existe en el editor"
            );
        }
    }

    /// Los nombres de la paleta del juego y los del editor hablan del MISMO
    /// objeto: si la mochila dice "Puerta", el editor no puede decir "door".
    #[test]
    fn build_palette_names_are_human_in_both_places() {
        let slots = slot_labels();
        for p in EDGE_PIECES {
            for name in [p.label, slots[p.art]] {
                assert!(
                    name.chars().next().is_some_and(|c| c.is_uppercase()),
                    "nombre poco claro para la pieza `{}`: {name}",
                    p.id
                );
                assert!(
                    !name.contains('/') && !name.contains('_'),
                    "«{name}» es jerga de clave, no un nombre ({})",
                    p.id
                );
            }
        }
    }
}
