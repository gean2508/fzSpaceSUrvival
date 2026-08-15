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
            Tile::StoneFloor, Tile::Wall, Tile::WoodWall, Tile::StoneBrick,
            Tile::Door, Tile::Window, Tile::Step, Tile::Stairwell, Tile::Air,
        ];
        for tile in all {
            let Some(art) = tile.art_key() else { continue };
            assert!(
                slots.contains_key(art),
                "el tile {tile:?} dibuja `{art}` y ese slot no existe en el editor"
            );
        }
    }

    /// **Toda pieza de construcción tiene PIXEL, no sólo slot.**
    ///
    /// El resto de los tests de acá prueban que la pieza sea *asignable* desde el
    /// editor. Eso deja abierto el agujero por el que nos colamos hasta el 4 ago
    /// 2026: `wall/wood`, `wall/stone`, `window/basic`, `stairs/up`,
    /// `stairs/down`, `floor/stone`, `ceiling/madera`, `roof/teja` y `roof/chapa`
    /// tenían slot con nombre y NINGÚN PNG, así que levantabas una pared y salía
    /// el cuadrado de color de `walls::fallback_color`. Nadie se enteraba salvo
    /// jugando.
    ///
    /// La causa fue de importación: `import-pack-masivo.py` recorre las carpetas
    /// `*_Singles_32x32` del pack, y las hojas de construcción (`Room_Builder`,
    /// `17_Visibile_Upstairs_System`) no tienen ninguna. Las trae ahora
    /// `scripts/import-construccion.py`.
    ///
    /// Este test es el que avisa si vuelve a pasar: **construir es lo que se
    /// juega**, y una pieza de construir sin pixel es un bug visible en el primer
    /// minuto de partida. El decorado del mundo NO entra acá a propósito — ése sí
    /// puede esperar al pixel (biblia §8: sin arte, fallback procedural).
    #[test]
    fn toda_pieza_de_construccion_tiene_pixel() {
        let manifest = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../ha-game/assets/art/manifest.json");
        let raw = std::fs::read_to_string(&manifest)
            .unwrap_or_else(|e| panic!("no pude leer {}: {e}", manifest.display()));
        let json: serde_json::Value = serde_json::from_str(&raw).expect("manifest.json ilegible");
        let sprites = json.get("sprites").and_then(|s| s.as_object()).expect("sin `sprites`");

        let mut claves: Vec<(&str, String)> = Vec::new();
        for p in EDGE_PIECES {
            // **El VANO es la excepción, y no por vaga.** Un vano sin pixel no cae
            // al cuadrado de color: `walls::marco_de_vano` le dibuja dos jambas y un
            // dintel, justamente porque un vano pintado entero se vería idéntico a
            // una pared y la pieza sería invisible. Exigirle arte acá obligaría a
            // inventarle un recorte al pack cuando el paño de fachada no trae
            // ninguno con hueco — que es el caso medido en `import-fachadas.py`.
            if p.kind == ha_shared::build_grid::WallKind::Doorway {
                continue;
            }
            claves.push((p.art, format!("la pieza de borde «{}»", p.label)));
        }
        for p in ha_shared::build_grid::CEILING_PIECES {
            claves.push((p.art, format!("la tapa «{}»", p.label)));
        }
        for p in BUILD_PIECES {
            if let Some(art) = p.tile.art_key() {
                claves.push((art, format!("la pieza «{}»", p.label)));
            }
        }
        for (art, quien) in claves {
            assert!(
                sprites.contains_key(art),
                "{quien} usa `{art}` y el manifest no tiene arte para esa clave: \
                 en el juego sale un cuadrado de color. Corré \
                 `python3 scripts/import-construccion.py`."
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

    /// **El kit modular entra por bandas de un módulo, no por columnas.**
    ///
    /// Es el mismo error dos veces: `import-pack-masivo.py` metió las bandas de
    /// `5_Floor_Modular_Buildings` como 92 columnas de 1×3 y 31 de 1×4 —recortes
    /// de 1 celda de dibujos que el artista hizo anchos—, y con eso una pared
    /// salía con media ventana. `scripts/import-modulares.py` lo trae entero.
    ///
    /// Este test es el que avisa si alguien vuelve a rebanarlo: toda la cáscara
    /// (azotea, tejado, pisos y plantas bajas) mide [`edificio::MODULO`] celdas
    /// de frente, y su alto es el que declara `edificio.rs` para esa banda.
    #[test]
    fn el_kit_modular_mide_lo_que_dibujo_el_artista() {
        use ha_shared::edificio::{ALTO_AZOTEA, ALTO_PISO, ALTO_PLANTA_BAJA, MODULO};

        let manifest = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../ha-game/assets/art/manifest.json");
        let raw = std::fs::read_to_string(&manifest)
            .unwrap_or_else(|e| panic!("no pude leer {}: {e}", manifest.display()));
        let json: serde_json::Value = serde_json::from_str(&raw).expect("manifest.json ilegible");
        let sprites = json.get("sprites").and_then(|s| s.as_object()).expect("sin `sprites`");

        let mut vistos = 0;
        for (clave, entrada) in sprites {
            let base = clave.split('#').next().unwrap_or(clave);
            let alto_max = match base {
                "edificio/azotea" | "edificio/azotea_clara" | "edificio/tejado"
                | "edificio/tejado_liso" => ALTO_AZOTEA,
                k if k.starts_with("edificio/piso_") => ALTO_PISO,
                k if k.starts_with("edificio/pb_") => ALTO_PLANTA_BAJA,
                _ => continue,
            };
            let rect = entrada.get("rect").expect("sprite sin rect");
            let w = rect.get("w").and_then(|v| v.as_i64()).expect("rect sin w");
            let h = rect.get("h").and_then(|v| v.as_i64()).expect("rect sin h");
            assert_eq!(
                w, MODULO,
                "`{clave}` mide {w} celdas de frente y el módulo del pack es {MODULO}: \
                 alguien volvió a rebanar las bandas. Corré `python3 scripts/import-modulares.py`"
            );
            assert!(
                h <= alto_max,
                "`{clave}` mide {h} celdas de alto y la banda declara {alto_max} en edificio.rs"
            );
            vistos += 1;
        }
        assert!(
            vistos >= 40,
            "sólo {vistos} piezas de cáscara en el manifest: falta correr \
             `python3 scripts/import-modulares.py`"
        );
    }

    /// **Ninguna tabla del agua puede sembrar una clave sin píxel.**
    ///
    /// No es lo mismo que un slot sin arte en el editor: acá la clave está
    /// dentro de una tabla de PESOS, así que se lleva su parte de los sorteos y
    /// devuelve nada. El síntoma es traicionero — el lago no queda con un
    /// cuadrado de color, queda con MENOS cosas de las que la tabla dice, y
    /// nadie puede notar a ojo que falta lo que nunca se dibujó. Es exactamente
    /// lo que pasaba con `agua/junco`, que Modern Exteriors no dibujó nunca.
    #[test]
    fn el_agua_no_siembra_claves_sin_pixel() {
        use ha_shared::agua;

        let manifest = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../ha-game/assets/art/manifest.json");
        let raw = std::fs::read_to_string(&manifest)
            .unwrap_or_else(|e| panic!("no pude leer {}: {e}", manifest.display()));
        let json: serde_json::Value = serde_json::from_str(&raw).expect("manifest.json ilegible");
        let sprites = json.get("sprites").and_then(|s| s.as_object()).expect("sin `sprites`");

        for (nombre, tabla) in [
            ("RIBERA", agua::RIBERA),
            ("REMANSO", agua::REMANSO),
            ("CORRIENTE", agua::CORRIENTE),
            ("ABIERTO", agua::ABIERTO),
        ] {
            for e in tabla {
                assert!(
                    sprites.contains_key(e.key),
                    "la tabla {nombre} siembra `{}` con peso {} y el manifest no tiene ese \
                     dibujo: esa fracción de la orilla queda vacía sin que se note. \
                     O le asignás arte en HA Studio, o sale de la tabla. \
                     Corré `python3 scripts/import-agua.py`.",
                    e.key,
                    e.weight
                );
            }
        }

        // Las orillas + la hondura + las cascadas, que son lo que dibuja el agua
        // en sí: sin ellas el lago vuelve a ser la plancha azul.
        for clave in ["tiles/water", "tiles/water_arena", "tiles/water_deep", "agua/cascada"] {
            assert!(
                sprites.contains_key(clave),
                "falta `{clave}` en el manifest: corré `python3 scripts/import-agua.py`"
            );
        }
    }

    /// **El relieve no se dibuja con colores planos.**
    ///
    /// El acantilado, el bordillo y la rampa tuvieron siempre el respaldo de
    /// `procedural_art` —un cuadrado gris con la cara marrón— y eso es
    /// exactamente lo que se pidió sacar. El respaldo se queda como red de
    /// seguridad para los cuerpos que el pack no cubre (roca desnuda, nieve,
    /// regolito, basalto), pero **el relieve de la Tierra tiene que salir del
    /// pack**: `world2d::cliff_material` sólo pide "pasto" y "arena" en este
    /// planeta, y esas seis claves tienen que existir.
    #[test]
    fn el_relieve_de_la_tierra_tiene_arte_del_pack() {
        let manifest = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../ha-game/assets/art/manifest.json");
        let raw = std::fs::read_to_string(&manifest).expect("manifest.json legible");
        let json: serde_json::Value = serde_json::from_str(&raw).expect("manifest.json ilegible");
        let sprites = json.get("sprites").and_then(|s| s.as_object()).expect("sin `sprites`");
        for clave in [
            "cliff/pasto",
            "cliff/arena",
            "ledge/pasto",
            "ledge/arena",
            "slope/pasto",
            "slope/arena",
        ] {
            assert!(
                sprites.contains_key(clave),
                "falta `{clave}` en el manifest: el relieve volvería al cuadrado \
                 dibujado por código. Corré `python3 scripts/import-relieve.py`"
            );
        }
    }
}
