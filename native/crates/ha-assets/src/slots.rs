//! Catálogo de **slots** del juego: las claves lógicas que el juego dibuja
//! (`decor/tree`, `player/astronaut`, `items/apple`, `machines/…`, `tiles/water`).
//! El editor siembra su lista con esto para que NUNCA esté vacío y para que
//! puedas reskinear lo que el juego usa. Derivado de los catálogos reales
//! (`ha-shared/items.rs`, `machines.rs`) + las claves documentadas en
//! `assets/art/README.md`.
//!
//! **Cada slot lleva NOMBRE HUMANO** (`label`, en español). La clave es para el
//! código; el nombre es para vos: en el editor se lee "Puerta de madera", no
//! `door/wood`. Si agregás una clave nueva, ponele nombre — un slot sin nombre
//! es un slot que nadie va a saber qué es cuando le toque asignarle el pixel.
//!
//! "en un futuro que actualice": agregá la clave nueva acá y aparece en el editor.

/// Un slot de arte del juego.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SlotDef {
    /// Clave lógica = ruta (`"decor/tree"`). Es lo que el juego pide a ArtLibrary.
    pub key: &'static str,
    /// Nombre humano en español (`"Árbol"`). Es lo que se muestra en el editor.
    pub label: &'static str,
    /// Categoría para agrupar/filtrar en el editor.
    pub category: &'static str,
    /// Hint: suele ser animado (agua, npc). El usuario puede cambiarlo por slot.
    pub animated: bool,
}

const fn s(key: &'static str, label: &'static str, category: &'static str) -> SlotDef {
    SlotDef { key, label, category, animated: false }
}
const fn anim(key: &'static str, label: &'static str, category: &'static str) -> SlotDef {
    SlotDef { key, label, category, animated: true }
}

/// Nombre humano de una categoría (encabezados del árbol del editor).
pub fn category_label(category: &str) -> &'static str {
    match category {
        "build" => "CONSTRUCCIÓN — piso, pared, puerta, ventana",
        "room" => "CONSTRUCCIÓN — cuartos y casas prefabricadas",
        "structure" => "CONSTRUCCIÓN — estructuras grandes",
        "player" => "JUGADOR",
        "npc" => "PERSONAJES Y ANIMALES",
        "ship" => "NAVE",
        "decor" => "DECORADO DEL MUNDO — árboles, rocas, minerales",
        "prop" => "OBJETOS DE CALLE",
        "furniture" => "MUEBLES DE INTERIOR",
        "building" => "EDIFICIOS ENTEROS",
        "vehicle" => "VEHÍCULOS",
        "nature" => "PLANTAS Y FLORES",
        "props" => "COFRES Y CONTENEDORES",
        "tiles" => "SUELO DEL MUNDO — pasto, arena, agua, roca",
        "relieve" => "RELIEVE — acantilados y rampas",
        "agua" => "AGUA — decorado y pesca",
        "parcela" => "PATIO Y PARCELA — cercas, piletas, canchas",
        "ui" => "INTERFAZ / HUD",
        "backgrounds" => "FONDOS Y CIELO",
        "items" => "ÍTEMS DEL INVENTARIO",
        "machines" => "MÁQUINAS",
        _ => "OTROS",
    }
}

/// Ítems del juego (`ha-shared/items.rs`). Cada `items/<id>` reemplaza el ícono
/// procedural en hotbar/bolsa/craft y el sprite de la máquina.
const ITEM_IDS: &[(&str, &str)] = &[
    ("apple", "Manzana"),
    ("axe", "Hacha de hierro"),
    ("bread", "Pan"),
    ("dirt", "Tierra"),
    ("gem_blue", "Gema azul"),
    ("gem_red", "Gema roja"),
    ("gold_ingot", "Lingote de oro"),
    ("grape", "Uvas"),
    ("hand", "Mano (sin ítem)"),
    ("iron_ingot", "Lingote de hierro"),
    ("jetpack", "Mochila cohete"),
    ("lantern", "Farol"),
    ("leather", "Cuero"),
    ("meat", "Carne"),
    ("orange", "Naranja"),
    ("part_engine", "Pieza: motor"),
    ("part_fin", "Pieza: aleta"),
    ("part_lander", "Pieza: patas de aterrizaje"),
    ("part_nose", "Pieza: punta"),
    ("part_tank", "Pieza: tanque de combustible"),
    ("pickaxe", "Pico de hierro"),
    ("pie", "Tarta"),
    ("potion_blue", "Poción azul"),
    ("potion_green", "Poción verde"),
    ("probe", "Sonda de navegación"),
    ("sand", "Arena"),
    ("shield_wood", "Escudo de madera"),
    ("snow", "Nieve"),
    ("soup", "Sopa"),
    ("sticks", "Palos"),
    ("stone", "Piedra"),
    ("stone_axe", "Hacha de piedra"),
    ("stone_pickaxe", "Pico de piedra"),
    ("torch", "Antorcha"),
    ("wood", "Madera"),
    ("wood_axe", "Hacha de madera"),
    ("wood_pickaxe", "Pico de madera"),
];

/// Máquinas colocables (`ha-shared/machines.rs`).
const MACHINE_IDS: &[(&str, &str)] = &[
    ("wood_extractor", "Extractor de madera"),
    ("stone_extractor", "Extractor de piedra"),
    ("launch_pad", "Plataforma de lanzamiento"),
    ("regolith_drill", "Perforadora de regolito"),
    ("antenna", "Antena"),
    ("habitat", "Hábitat de colonos"),
];

/// **CONSTRUCCIÓN — piezas que el jugador coloca** (LimeZu `Room_Builder_32x32.png`).
///
/// Son las piezas de la pestaña CONSTRUIR de la mochila: el piso que pisás, la
/// pared que levantás, la puerta que cruzás. Autotile: el editor guarda poses
/// `center`/`edge_*`/`corner_*`/`inner_*` y el juego elige la variante por
/// vecinos (mismo pipeline que `tiles/water`). `door/*` además admite poses
/// `open`/`closed` (se abre sola por proximidad).
///
/// Sin arte asignado NO queda un hueco: cada pieza se dibuja con su color de
/// código (`Tile::color` / `walls::fallback_color`) — la puerta es un cuadrado
/// café, la pared de piedra un cuadrado gris. Asignás el pixel cuando querés.
const BUILD_ART_KEYS: &[(&str, &str)] = &[
    ("floor/wood", "Piso de madera"),
    ("floor/stone", "Piso de piedra"),
    ("wall/wood", "Pared de madera"),
    ("wall/stone", "Pared de piedra"),
    ("door/wood", "Puerta"),
    ("window/basic", "Ventana"),
    ("stairs/down", "Escalera para bajar"),
    ("stairs/up", "Escalera para subir"),
    // Vereda: el suelo urbano CAMINABLE (`Tile::Pavement`). Hasta que existió, la
    // obra civil pintaba las aceras con la roca del subsuelo, que es sólida — o
    // sea que las veredas de todas las ciudades eran muros.
    ("floor/vereda", "Vereda"),
    ("floor/asfalto", "Asfalto (calle)"),
    // TAPAS (`build_grid::CEILING_PIECES`). El tumbado es la cara lisa que se ve
    // desde adentro del cuarto; los techos son de AFUERA y usan autotile
    // (`center`/`edge_*`/`corner_*`/`inner_*`), así que con recortar esas poses
    // una vez salen faldones, cumbreras y esquinas sin piezas sueltas.
    ("ceiling/madera", "Tumbado de madera"),
    ("roof/teja", "Techo de teja (modular)"),
    ("roof/chapa", "Techo de chapa (modular)"),
];

/// Plantillas de habitación prefab (LimeZu `6_Home_Designs`): arte opcional de
/// los presets de construcción (`room_small`…) — la LÓGICA vive en `ha-shared`.
const ROOM_KEYS: &[(&str, &str)] = &[
    ("room/room_small", "Cuarto chico (5x4)"),
    ("room/room_medium", "Cuarto mediano (7x6)"),
    ("room/room_large", "Salón grande (9x8)"),
    ("room/casa_1", "Casa de dos ambientes (9x7)"),
];

/// Assets GRANDES colocables (A2): un sprite único con footprint de colisión,
/// que el jugador coloca como una casa y rodea (Y-sort). Claves = `LARGE_ASSETS`
/// en `ha-shared::topdown`. El usuario asigna el pixel y pinta la huella (A1).
const STRUCTURE_KEYS: &[(&str, &str)] = &[
    ("structure/boulder", "Peñasco grande (2x2)"),
    ("structure/statue", "Estatua (2x3)"),
    ("structure/monument", "Monumento (4x4)"),
];

/// Decoración del mundo (referenciada en `world2d.rs`/`ship_interior.rs`). Sufijo
/// `_moon`/`_mars` = variante por cuerpo (cae a la genérica si no está). Las
/// especies de árbol (`tree_*`) las elige `pick_tree_species`; `tree` es el
/// fallback genérico.
const DECOR_KEYS: &[(&str, &str)] = &[
    ("decor/tree", "Árbol (genérico)"),
    ("decor/tree_roble", "Árbol: roble"),
    ("decor/tree_pino", "Árbol: pino"),
    ("decor/tree_abedul", "Árbol: abedul"),
    ("decor/tree_arce", "Árbol: arce"),
    ("decor/tree_sauce", "Árbol: sauce"),
    ("decor/tree_palmera", "Árbol: palmera"),
    ("decor/tree_arbusto", "Árbol: arbusto grande"),
    ("decor/tree_seco", "Árbol seco"),
    ("decor/rock", "Roca"),
    ("decor/rock_moon", "Roca lunar"),
    ("decor/rock_mars", "Roca marciana"),
    ("decor/ore_iron", "Veta de hierro"),
    ("decor/ore_gold", "Veta de oro"),
    ("decor/gem_red", "Cristal rojo"),
    ("decor/gem_blue", "Cristal azul"),
    ("decor/mine_hole", "Boca de mina"),
    ("decor/ladder", "Escalera de mano"),
    ("decor/cockpit_seat", "Asiento de cabina"),
    ("decor/fuel_source", "Surtidor de combustible"),
];

/// Props de exterior (LimeZu `3_City_Props` / `17_Garden` / `11_Camping`).
/// Sembrados deterministas en superficie (Tierra) por `world2d.rs`; cero costo
/// si el slot no tiene arte asignado.
const PROP_KEYS: &[(&str, &str)] = &[
    ("prop/banco", "Banco de plaza"),
    ("prop/farol", "Farol de calle"),
    ("prop/tacho_basura", "Tacho de basura"),
    ("prop/cartel", "Cartel"),
    ("prop/hidrante", "Hidrante"),
    ("prop/cerca", "Cerca suelta"),
    ("prop/buzon", "Buzón de calle"),
    ("prop/barril", "Barril"),
    ("prop/caja", "Caja de madera"),
    ("prop/mesa_picnic", "Mesa de picnic"),
    ("prop/carpa", "Carpa"),
    ("prop/maceta", "Maceta"),
    ("prop/estatua", "Estatua chica"),
    ("prop/pozo", "Pozo de agua"),
];

/// Muebles de interior (LimeZu `Interiors_32x32.png`). Van dentro de edificios
/// (generados o construidos); pivote BottomCenter + Y-sort por la fila del pie.
const FURNITURE_KEYS: &[(&str, &str)] = &[
    ("furniture/cama", "Cama"),
    ("furniture/mesa", "Mesa"),
    ("furniture/silla", "Silla"),
    ("furniture/sofa", "Sofá"),
    ("furniture/cocina", "Cocina"),
    ("furniture/heladera", "Heladera"),
    ("furniture/pileta", "Pileta de cocina"),
    ("furniture/inodoro", "Inodoro"),
    ("furniture/banera", "Bañera"),
    ("furniture/estante", "Estante"),
    ("furniture/biblioteca", "Biblioteca"),
    ("furniture/lampara", "Lámpara"),
    ("furniture/escritorio", "Escritorio"),
    ("furniture/ropero", "Ropero"),
    ("furniture/mostrador", "Mostrador"),
    ("furniture/planta", "Planta de interior"),
];

/// Fachadas/edificios (LimeZu `4_Generic_Buildings` / `24_Additional_Houses`).
/// Cada edificio se recorta en POSES por capa: `base` (piso+paredes, Y-sort),
/// `roof` (techo, capa Roof → translúcido al entrar). Ver §techo en building.rs.
const BUILDING_KEYS: &[(&str, &str)] = &[
    // Residencial — la VARIEDAD es el punto: un barrio donde todas las casas son
    // la misma casa se lee como un error, no como un barrio. Fuentes: LimeZu
    // `24_Additional_Houses` (215 singles) y `7_Villas` (194).
    ("building/casa_chica", "Casa chica"),
    ("building/casa_mediana", "Casa mediana"),
    ("building/casa_grande", "Casa grande"),
    ("building/casa_country", "Casa de campo"),
    ("building/villa", "Villa / chalet"),
    ("building/duplex", "Dúplex"),
    ("building/garaje", "Garaje"),
    // Comercio y oficina (`9_Shopping_Center`, `16_Office`).
    ("building/tienda", "Tienda"),
    ("building/mercado", "Mercado"),
    ("building/shopping", "Shopping"),
    ("building/oficina", "Oficina"),
    ("building/torre", "Torre de departamentos"),
    ("building/gasolinera", "Gasolinera"),
    // Equipamiento cívico: son los edificios GRANDES, los que no caben en una
    // parcela residencial (`12_Hotel_and_Hospital`, `13_School`, `15_Police`,
    // `18_Fire_Station`, `22_Post_Office`, `20_Subway_and_Train_Station`).
    ("building/hospital", "Hospital"),
    ("building/hotel", "Hotel"),
    ("building/universidad", "Universidad"),
    ("building/escuela", "Escuela"),
    ("building/comisaria", "Comisaría"),
    ("building/bomberos", "Cuartel de bomberos"),
    ("building/correo", "Correo"),
    ("building/estacion_tren", "Estación de tren"),
    // Industria y campo (`8_Worksite`, `23_Military_Base`).
    ("building/deposito", "Depósito"),
    ("building/granero", "Granero"),
    ("building/nave_industrial", "Nave industrial"),
    ("building/base_militar", "Base militar"),
];

/// **RELIEVE** — la cara de los acantilados y las rampas (`crate::relief`).
///
/// Autotile igual que `floor/*`: el editor guarda las poses `center` / `edge_*` /
/// `corner_*` / `inner_*` y el juego elige la variante por vecinos. La variante de
/// MATERIAL la elige el bioma (`world2d::terrain_art_key`), así que la misma pared
/// se ve de roca en la sierra y de arena en el desierto; si una variante no está
/// curada, cae sola a `cliff/roca` y no queda ninguna celda sin dibujar.
///
/// Fuente sugerida: LimeZu `1_Terrains_and_Fences` y los autotiles de
/// `Autotiles_32x32`.
const RELIEF_KEYS: &[(&str, &str)] = &[
    ("cliff/roca", "Acantilado de roca"),
    ("cliff/pasto", "Acantilado con pasto"),
    ("cliff/arena", "Acantilado de arena"),
    ("cliff/nieve", "Acantilado nevado"),
    ("cliff/regolito", "Acantilado de regolito (Luna)"),
    ("cliff/basalto", "Acantilado de basalto"),
    ("slope/roca", "Rampa de roca"),
    ("slope/pasto", "Rampa con pasto"),
    ("slope/arena", "Rampa de arena"),
    ("slope/nieve", "Rampa nevada"),
    // BORDILLOS: el desnivel de UN escalón, el que se trepa y se baja sin buscar
    // rampa. Tiene arte propio para que se distinga de un muro de un vistazo —
    // si mirás un borde y no sabés si podés subirlo, la exploración se vuelve
    // prueba y error. Sin curar, cae al acantilado de siempre.
    ("ledge/roca", "Bordillo de roca (se trepa)"),
    ("ledge/pasto", "Bordillo con pasto (se trepa)"),
    ("ledge/arena", "Bordillo de arena (se trepa)"),
    ("ledge/nieve", "Bordillo nevado (se trepa)"),
    ("ledge/regolito", "Bordillo de regolito (se trepa)"),
    ("ledge/basalto", "Bordillo de basalto (se trepa)"),
];

/// **AGUA** — para que el líquido no se vea como una plancha azul vacía.
///
/// Sembrados deterministas sobre celdas de agua. `fishing/spot` tiene **cuatro
/// variantes** (los `Fishing_Spot_1..4` de `11_Camping`): se declaran como
/// variantes del mismo objeto en el editor y el juego alterna entre ellas por
/// celda (`variant_key_seeded`), así que un lago no repite el mismo spot.
const WATER_KEYS: &[(&str, &str)] = &[
    ("agua/tronco_flotante", "Tronco flotante"),
    ("agua/nenufar", "Nenúfar"),
    ("agua/boya", "Boya"),
    ("agua/flotador", "Flotador"),
    ("agua/estrella_mar", "Estrella de mar"),
    ("agua/bote", "Bote"),
    ("agua/muelle", "Muelle"),
    ("agua/junco", "Juncos"),
    ("fishing/spot", "Zona de pesca"),
    ("fishing/caña", "Caña de pescar"),
    ("fishing/balde", "Balde de pesca"),
];

/// **PARCELA** — lo que hace que una casa sea una casa y no una caja: la cerca,
/// la parrilla, el auto en la entrada, la pileta, la cancha.
/// Fuentes: `1_Terrains_and_Fences`, `17_Garden`, `14_Swimming_Pool`,
/// `13_School` (canchas), `10_Vehicles`.
const LOT_KEYS: &[(&str, &str)] = &[
    ("fence/madera", "Cerca de madera"),
    ("fence/alambre", "Cerca de alambre"),
    ("fence/piedra", "Muro bajo de piedra"),
    ("fence/seto", "Seto (cerco vivo)"),
    ("prop/parrilla", "Parrilla"),
    ("prop/auto_estacionado", "Auto estacionado"),
    ("prop/pileta_borde", "Borde de pileta"),
    ("prop/trampolin", "Trampolín"),
    ("prop/cancha_basket", "Cancha de básquet"),
    ("prop/cancha_tenis", "Cancha de tenis"),
    ("prop/arco_basket", "Aro de básquet"),
    ("prop/tobogan", "Tobogán"),
    ("prop/hamaca", "Hamaca"),
    ("prop/arenero", "Arenero"),
    ("prop/tendedero", "Tendedero"),
    ("prop/buzon_casa", "Buzón de casa"),
    ("prop/cesped_cortado", "Césped cortado"),
    ("prop/cantero", "Cantero de flores"),
];

/// Vehículos animados (LimeZu `Animated_32x32/Vehicles_32x32`).
const VEHICLE_KEYS: &[(&str, &str)] = &[
    ("vehicle/auto", "Auto"),
    ("vehicle/taxi", "Taxi"),
    ("vehicle/bus", "Colectivo"),
    ("vehicle/helicoptero", "Helicóptero"),
    ("vehicle/tanque", "Tanque"),
];

/// Flora/decoración natural extra (LimeZu `17_Garden`). Complementa los
/// `decor/tree_*` existentes — no los duplica.
const NATURE_KEYS: &[(&str, &str)] = &[
    ("nature/arbusto", "Arbusto"),
    ("nature/flores_rojas", "Flores rojas"),
    ("nature/flores_amarillas", "Flores amarillas"),
    ("nature/pasto", "Mata de pasto"),
    ("nature/tocon", "Tocón"),
    ("nature/tronco", "Tronco caído"),
    ("nature/hongo", "Hongo"),
    ("nature/cactus", "Cactus"),
];

/// Elementos de UI / HUD (hoy procedurales en `ui_theme.rs`/`hud.rs`/`hud/*`).
/// Audit completo: cada elemento visible tiene su slot; el HUD pide primero el
/// sprite y cae al dibujo procedural si no hay arte. Ítems y máquinas NO van acá
/// (ya tienen `items/*` / `machines/*`).
const UI_KEYS: &[(&str, &str)] = &[
    // Paneles / botones base.
    ("ui/panel", "Panel de fondo"),
    ("ui/button", "Botón"),
    ("ui/button_pressed", "Botón apretado"),
    ("ui/hotbar_slot", "Casilla de la barra rápida"),
    ("ui/hotbar_slot_selected", "Casilla seleccionada"),
    ("ui/slot_empty", "Casilla vacía"),
    ("ui/close_button", "Botón cerrar"),
    ("ui/tab_inventario", "Pestaña: inventario"),
    ("ui/tab_craft", "Pestaña: fabricar"),
    ("ui/tab_construir", "Pestaña: construir"),
    // Barras de survival (corazones + energía + hambre + sed + oxígeno).
    ("ui/heart_full", "Corazón lleno"),
    ("ui/heart_half", "Corazón medio"),
    ("ui/heart_empty", "Corazón vacío"),
    ("ui/energy_full", "Energía llena"),
    ("ui/energy_empty", "Energía vacía"),
    ("ui/hunger_full", "Hambre llena"),
    ("ui/hunger_half", "Hambre media"),
    ("ui/hunger_empty", "Hambre vacía"),
    ("ui/thirst_full", "Sed llena"),
    ("ui/thirst_half", "Sed media"),
    ("ui/thirst_empty", "Sed vacía"),
    ("ui/oxygen_full", "Oxígeno lleno"),
    ("ui/oxygen_empty", "Oxígeno vacío"),
    // Controles táctiles / acciones.
    ("ui/joystick_base", "Joystick: base"),
    ("ui/joystick_knob", "Joystick: perilla"),
    ("ui/use_button", "Botón usar"),
    ("ui/jump_button", "Botón saltar"),
    ("ui/bag_button", "Botón mochila"),
    ("ui/menu_button", "Botón menú"),
    ("ui/crosshair", "Mira"),
    ("ui/lantern_button", "Botón farol"),
    ("ui/build_chip", "Chip de modo construir"),
    ("ui/hold_progress_bg", "Barra de mantener: fondo"),
    ("ui/hold_progress_fill", "Barra de mantener: relleno"),
    // Iconos de acción / interacción.
    ("ui/icon_atacar", "Ícono: atacar"),
    ("ui/icon_usar", "Ícono: usar"),
    ("ui/icon_cavar", "Ícono: cavar"),
    ("ui/icon_interactuar", "Ícono: interactuar"),
    ("ui/icon_craft", "Ícono: fabricar"),
    // Menú del juego / mapa.
    ("ui/menu_tierra", "Menú: Tierra"),
    ("ui/menu_luna", "Menú: Luna"),
    ("ui/menu_espacio", "Menú: espacio"),
    ("ui/menu_creativo", "Menú: modo creativo"),
    ("ui/menu_ajustes", "Menú: ajustes"),
    ("ui/minimapa_marco", "Marco del minimapa"),
];

/// Fondos / cielo (hoy color + estrellas).
const BG_KEYS: &[(&str, &str)] = &[
    ("backgrounds/sky_day", "Cielo de día"),
    ("backgrounds/sky_night", "Cielo de noche"),
    ("backgrounds/stars", "Estrellas"),
];


/// **Objetos importados del pack** (LimeZu Modern Interiors/Exteriors).
///
/// Se generan con `scripts/import-pack.py` a partir de una lista de
/// `tema + indice -> nombre en castellano`: el arte ya existe (son PNG sueltos,
/// un archivo = un objeto), así que acá sólo viven la clave y el nombre. La
/// huella sale del TAMAÑO del PNG, no de un número escrito a mano.
///
/// Entre los marcadores no se edita a mano: se regenera.
const PACK_KEYS: &[(&str, &str)] = &[
    // === PACK:START ===
    ("nature/manzanas_camping", "Manzanas del bosque"),
    ("prop/sillon_camping", "Sillon de camping"),
    ("prop/mochila", "Mochila"),
    ("prop/pelota", "Pelota"),
    ("prop/barbacoa", "Barbacoa"),
    ("prop/canasto", "Canasto"),
    ("prop/mesa_con_bancos", "Mesa con bancos"),
    ("prop/binoculares", "Binoculares"),
    ("prop/botella", "Botella"),
    ("prop/balde", "Balde"),
    ("agua/boya_rio", "Boya de rio"),
    ("prop/fogata", "Fogata"),
    ("prop/fogata_malvaviscos", "Fogata con malvaviscos"),
    ("prop/silla_camping", "Silla de camping"),
    ("prop/lenia_cortada", "Lenia cortada"),
    ("prop/banco_lenia", "Banco de lenia"),
    ("prop/anafe_electrico", "Anafe electrico"),
    ("agua/balde_pescado", "Balde con pescado"),
    ("agua/cajon_pescado", "Cajon de pescado"),
    ("agua/pesquero", "Pesquero"),
    ("agua/madera_flotante", "Madera flotante"),
    ("prop/olla_colgante", "Olla colgante"),
    ("prop/farol_camping", "Farol de camping"),
    ("agua/salvavidas", "Salvavidas"),
    ("building/casilla_grande", "Casilla rodante grande"),
    ("building/casilla_mediana", "Casilla rodante mediana"),
    ("building/casilla_chica", "Casilla rodante chica"),
    ("nature/musgo", "Musgo"),
    ("nature/hongos_bosque", "Hongos del bosque"),
    ("agua/muelle_camping", "Muelle de tablones"),
    ("agua/barril_muelle", "Barril de muelle"),
    ("agua/tacho_muelle", "Tacho de muelle"),
    ("agua/pasarela_muelle", "Pasarela de muelle"),
    ("agua/cajones_muelle", "Cajones de muelle"),
    ("agua/poste_muelle", "Poste de muelle"),
    ("nature/pinia", "Pinia"),
    ("prop/plato", "Plato"),
    ("nature/roca_bosque", "Roca del bosque"),
    ("prop/soga", "Soga"),
    ("prop/cartelera_camping", "Cartelera de camping"),
    ("prop/bolsa_dormir", "Bolsa de dormir"),
    ("nature/brote", "Brote"),
    ("nature/tocon_bosque", "Tocon del bosque"),
    ("prop/mesa_camping", "Mesa de camping"),
    ("prop/mesita_camping", "Mesita de camping"),
    ("prop/cubierta", "Cubierta usada"),
    ("nature/arbol_bosque", "Arbol del bosque"),
    ("nature/arbol_muerto", "Arbol muerto"),
    ("prop/tablon", "Tablon"),
    ("vehicle/ambulancia", "Ambulancia"),
    ("vehicle/bote", "Bote"),
    ("vehicle/colectivo", "Colectivo"),
    ("prop/parada_colectivo", "Parada de colectivo"),
    ("prop/cartel_parada", "Cartel de parada"),
    ("vehicle/casa_rodante", "Casa rodante"),
    ("prop/carro_frutas", "Carro de frutas y flores"),
    ("building/estacion_servicio", "Estacion de servicio"),
    ("prop/surtidor", "Surtidor de nafta"),
    ("prop/cartel_precios", "Cartel de precios"),
    ("vehicle/helipuerto", "Helipuerto"),
    ("prop/carro_panchos", "Carro de panchos"),
    ("vehicle/camion_helados", "Camion de helados"),
    ("vehicle/tuk_tuk_pizza", "Tuk tuk de pizza"),
    ("prop/telefono_publico", "Telefono publico"),
    ("prop/carro_comida", "Carro de comida"),
    ("prop/silla_puesto", "Silla de puesto"),
    ("prop/mesa_puesto", "Mesa de puesto"),
    ("vehicle/camion_tacos", "Camion de tacos"),
    ("prop/cajero", "Cajero automatico"),
    ("prop/antena_ciudad", "Antena de ciudad"),
    ("prop/barril_ciudad", "Barril de ciudad"),
    ("prop/banco_ciudad", "Banco de ciudad"),
    ("prop/cartelera", "Cartelera publicitaria"),
    ("prop/tacho_negro", "Tacho negro"),
    ("prop/tacho_azul", "Tacho azul"),
    ("prop/tacho_verde", "Tacho verde"),
    ("prop/ladrillos", "Pila de ladrillos"),
    ("prop/escalera_rota", "Escalera rota"),
    ("prop/barril_marron", "Barril marron"),
    ("prop/carro_golosinas", "Carro de golosinas"),
    ("prop/auto_chocado", "Auto chocado"),
    ("prop/cono_transito", "Cono de transito"),
    ("prop/contenedor", "Contenedor"),
    ("building/casa_contenedor", "Casa contenedor"),
    ("prop/cartel_peligro", "Cartel de peligro"),
    ("prop/bebedero", "Bebedero"),
    ("prop/dron", "Dron"),
    ("prop/contenedor_basura", "Contenedor de basura"),
    ("prop/cargador_electrico", "Cargador electrico"),
    ("prop/caja_electrica", "Caja electrica"),
    ("prop/poste_electrico", "Poste electrico"),
    ("nature/arbusto_flores", "Arbusto con flores"),
    ("nature/flores_ciudad", "Flores de vereda"),
    ("prop/fuente", "Fuente de plaza"),
    ("prop/rejilla", "Rejilla de desague"),
    ("vehicle/camion_basura", "Camion de basura"),
    ("prop/ropa_tendida", "Ropa tendida"),
    ("prop/hidrante_ciudad", "Hidrante de ciudad"),
    ("prop/cartel_info", "Cartel informativo"),
    ("building/casilla_chatarra", "Casilla de chatarra"),
    ("building/kiosco_cafe", "Kiosco de cafe"),
    ("building/kiosco_helados", "Kiosco de helados"),
    ("building/kiosco_info", "Kiosco de informes"),
    ("building/kiosco_hongo", "Kiosco hongo"),
    ("prop/buzon_ciudad", "Buzon de ciudad"),
    ("prop/tapa_alcantarilla", "Tapa de alcantarilla"),
    ("prop/parquimetro", "Parquimetro"),
    ("prop/cabina_telefonica", "Cabina telefonica"),
    ("prop/poste_ciudad", "Poste de ciudad"),
    ("prop/maceta_ciudad", "Maceta de vereda"),
    ("building/casilla_electrica", "Casilla electrica"),
    ("prop/chatarra", "Chatarra"),
    ("prop/pila_chatarra", "Pila de chatarra"),
    ("nature/arbusto_ciudad", "Arbusto de vereda"),
    ("prop/panel_solar", "Panel solar"),
    ("prop/barriles_apilados", "Barriles apilados"),
    ("prop/barrera", "Barrera"),
    ("prop/farol_ciudad", "Farol de ciudad"),
    ("prop/valla_transito", "Valla de transito"),
    ("prop/semaforo", "Semaforo"),
    ("prop/senial_transito", "Senial de transito"),
    ("prop/pila_basura", "Pila de basura"),
    ("prop/tacho_ciudad", "Tacho de vereda"),
    ("nature/arbol_ciudad", "Arbol de vereda"),
    ("building/entrada_estacionamiento", "Entrada de estacionamiento"),
    ("building/tanque_agua", "Tanque de agua"),
    ("building/turbina_eolica", "Turbina eolica"),
    ("furniture/batidora", "Batidora"),
    ("furniture/licuadora", "Licuadora"),
    ("furniture/cafetera", "Cafetera"),
    ("furniture/microondas", "Microondas"),
    ("furniture/estante_microondas", "Estante con microondas"),
    ("furniture/dispensador_agua", "Dispensador de agua"),
    ("furniture/parrilla_industrial", "Parrilla industrial"),
    ("furniture/parrilla_reja", "Parrilla con reja"),
    ("furniture/parrilla_brochetas", "Parrilla con brochetas"),
    ("furniture/olla_arrocera", "Olla arrocera"),
    ("furniture/olla_presion", "Olla a presion"),
    ("furniture/delantal_blanco", "Delantal blanco"),
    ("furniture/delantal_verde", "Delantal verde"),
    ("furniture/delantal_azul", "Delantal azul"),
    ("furniture/delantal_rojo", "Delantal rojo"),
    ("furniture/lampara_pie", "Lampara de pie"),
    ("furniture/lampara_pie_alta", "Lampara de pie alta"),
    ("furniture/tabla_cortar", "Tabla de cortar"),
    ("furniture/tabla_cortar_larga", "Tabla de cortar larga"),
    ("furniture/ropero_morado", "Ropero morado"),
    ("furniture/televisor", "Televisor"),
    ("furniture/espejo_pie", "Espejo de pie"),
    ("furniture/banco_tapizado", "Banco tapizado"),
    ("furniture/mesa_luz", "Mesa de luz"),
    ("furniture/espejo_pared", "Espejo de pared"),
    ("furniture/cortina", "Cortina"),
    ("furniture/planta_maceta_alta", "Planta en maceta alta"),
    ("furniture/palmera_interior", "Palmera de interior"),
    ("furniture/planta_chica", "Planta chica"),
    ("furniture/planta_colgante", "Planta colgante"),
    ("furniture/planta_alta", "Planta alta"),
    ("furniture/planta_frondosa", "Planta frondosa"),
    ("furniture/tocador", "Tocador con espejo"),
    ("furniture/lampara_mesa", "Lampara de mesa"),
    ("furniture/almohadon", "Almohadon"),
    ("furniture/sofa_cama", "Sofa cama"),
    ("furniture/sofa_lampara", "Sofa con lampara"),
    ("furniture/sofa_claro", "Sofa claro"),
    ("furniture/armario_madera", "Armario de madera"),
    ("furniture/armario_espejo", "Armario con espejo"),
    ("furniture/alacena", "Alacena"),
    ("furniture/vitrina", "Vitrina"),
    ("furniture/placard_angosto", "Placard angosto"),
    ("furniture/placard_espejo", "Placard con espejo"),
    ("furniture/estante_plantas", "Estante con plantas"),
    ("furniture/mesa_con_lampara", "Mesa con lampara"),
    ("furniture/mesa_con_planta", "Mesa con planta"),
    ("furniture/canasto_frutas", "Canasto de frutas"),
    // === PACK:END ===
];


/// Categoría de un slot del pack, deducida de su prefijo (la carpeta a la que se
/// copió el PNG). Es la misma agrupación que ya usa el editor.
fn pack_category(key: &str) -> &'static str {
    match key.split('/').next().unwrap_or("") {
        "furniture" => "furniture",
        "prop" => "prop",
        "nature" => "nature",
        "agua" => "agua",
        "vehicle" => "vehicle",
        "building" => "building",
        "structure" => "structure",
        _ => "decor",
    }
}

/// Lista canónica completa de slots del juego. `items/<id>` y `machines/<id>` se
/// generan de los catálogos; el resto son claves fijas documentadas.
///
/// **Orden = prioridad de trabajo**: la construcción va PRIMERA (es lo que se
/// juega), después el mundo, después la interfaz.
pub fn game_slots() -> Vec<SlotDef> {
    let mut out = Vec::new();
    // — CONSTRUCCIÓN — piezas de celda/borde, cuartos prefab y estructuras.
    for &(k, l) in BUILD_ART_KEYS {
        out.push(s(k, l, "build"));
    }
    for &(k, l) in ROOM_KEYS {
        out.push(s(k, l, "room"));
    }
    for &(k, l) in STRUCTURE_KEYS {
        out.push(s(k, l, "structure"));
    }
    // Personaje / nave. `player/character` es la clave REAL que el juego dibuja
    // (PLAYER_ART_KEY en sim.rs): admite poses idle/walk × 4 direcciones y va
    // animado. `player/astronaut` queda como alias histórico.
    out.push(anim("player/character", "Jugador", "player"));
    out.push(anim("player/astronaut", "Jugador con traje espacial", "player"));
    // NPCs — mismo esquema de poses que el jugador (idle_down…). `npc/colonist`
    // se dibuja en la base lunar (uno por colono; ver world2d::sync_colonists).
    out.push(anim("npc/colonist", "Colono", "npc"));
    out.push(anim("npc/aldeano", "Aldeano", "npc"));
    out.push(anim("npc/comerciante", "Comerciante", "npc"));
    // Fauna con cerebro IAFZ (FASE 8) — especies del pack LimeZu Modern Farm.
    // Poses del pack por dirección: idle/walk/eat/drink (perro: + run/bark).
    out.push(anim("npc/fauna_cow", "Vaca", "npc"));
    out.push(anim("npc/fauna_chicken", "Gallina", "npc"));
    out.push(anim("npc/fauna_sheep", "Oveja", "npc"));
    out.push(anim("npc/fauna_pig", "Cerdo", "npc"));
    out.push(anim("npc/fauna_dog", "Perro", "npc"));
    out.push(s("ship/rocket", "Cohete", "ship"));
    // Decoración.
    for &(k, l) in DECOR_KEYS {
        out.push(s(k, l, "decor"));
    }
    // Props de exterior / muebles / edificios / vehículos / flora.
    for &(k, l) in PROP_KEYS {
        out.push(s(k, l, "prop"));
    }
    for &(k, l) in FURNITURE_KEYS {
        out.push(s(k, l, "furniture"));
    }
    for &(k, l) in BUILDING_KEYS {
        out.push(s(k, l, "building"));
    }
    for &(k, l) in VEHICLE_KEYS {
        out.push(anim(k, l, "vehicle"));
    }
    for &(k, l) in NATURE_KEYS {
        out.push(s(k, l, "nature"));
    }
    // Contenedores con estado: el juego conmuta las poses `closed`/`open` y las
    // reproduce una sola vez (queda abierto mientras lo usás). Ver `chests.rs`.
    out.push(anim("props/chest", "Cofre", "props"));
    // Tiles del terreno. Todos con autotile; el agua además animada.
    out.push(anim("tiles/water", "Agua", "tiles"));
    out.push(anim("tiles/water_deep", "Agua profunda", "tiles"));
    out.push(s("tiles/grass", "Pasto", "tiles"));
    out.push(s("tiles/sand", "Arena", "tiles"));
    out.push(s("tiles/dirt", "Tierra", "tiles"));
    out.push(s("tiles/stone", "Piedra del suelo", "tiles"));
    out.push(s("tiles/regolith", "Regolito (Luna)", "tiles"));
    // Faltaban aunque el juego YA los pedía en `tile_art_key`: sin slot no había
    // forma de asignarles arte desde el editor.
    out.push(s("tiles/basalt", "Basalto", "tiles"));
    out.push(s("tiles/ice", "Hielo", "tiles"));
    out.push(s("tiles/roca", "Roca sólida (pared de cueva)", "tiles"));
    // Relieve: la geometría del mundo hecha arte (acantilados y rampas).
    for &(k, l) in RELIEF_KEYS {
        out.push(s(k, l, "relieve"));
    }
    // Decoración del agua. Los spots de pesca son animados (el chapoteo).
    for &(k, l) in WATER_KEYS {
        if k.starts_with("fishing/spot") || k == "agua/flotador" {
            out.push(anim(k, l, "agua"));
        } else {
            out.push(s(k, l, "agua"));
        }
    }
    // Parcela: cercas, parrillas, piletas, canchas.
    for &(k, l) in LOT_KEYS {
        out.push(s(k, l, "parcela"));
    }
    // UI / HUD (Sprites2D de interfaz).
    for &(k, l) in UI_KEYS {
        out.push(s(k, l, "ui"));
    }
    // Fondos.
    for &(k, l) in PACK_KEYS {
        out.push(s(k, l, pack_category(k)));
    }
    for &(k, l) in BG_KEYS {
        out.push(s(k, l, "backgrounds"));
    }
    // Ítems (íconos + sprite de máquina).
    for &(id, label) in ITEM_IDS {
        out.push(SlotDef {
            key: leak(format!("items/{id}")),
            label,
            category: "items",
            animated: false,
        });
    }
    // Máquinas colocadas.
    for &(id, label) in MACHINE_IDS {
        out.push(SlotDef {
            key: leak(format!("machines/{id}")),
            label,
            category: "machines",
            animated: false,
        });
    }
    out
}

/// Los slots derivados de catálogos necesitan `&'static str`. Como `game_slots`
/// se llama una vez al abrir el editor (no en caliente), filtrar la cadena es
/// aceptable y mantiene `SlotDef` `Copy`.
fn leak(sfmt: String) -> &'static str {
    Box::leak(sfmt.into_boxed_str())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn catalog_is_populated_and_has_known_keys() {
        let slots = game_slots();
        assert!(slots.len() > 50, "debe listar decenas de slots reales");
        let keys: Vec<&str> = slots.iter().map(|s| s.key).collect();
        for expected in [
            "player/character", "player/astronaut", "decor/tree", "decor/tree_roble",
            "decor/ladder", "items/apple", "machines/wood_extractor", "tiles/water",
            "prop/banco", "furniture/cama", "building/casa_chica", "vehicle/auto",
            "nature/arbusto", "floor/wood", "wall/wood", "door/wood", "window/basic",
            "stairs/down", "room/room_small", "ui/hunger_full", "ui/thirst_half",
            "ui/oxygen_full", "ui/tab_construir", "ui/hold_progress_fill", "npc/aldeano",
        ] {
            assert!(keys.contains(&expected), "falta slot {expected}");
        }
        // El agua viene marcada animada por defecto.
        assert!(slots.iter().find(|s| s.key == "tiles/water").unwrap().animated);
    }

    #[test]
    fn keys_are_unique() {
        let slots = game_slots();
        let mut seen = std::collections::BTreeSet::new();
        for s in &slots {
            assert!(seen.insert(s.key), "clave duplicada: {}", s.key);
        }
    }

    /// **Todo slot tiene nombre humano.** Un slot sin nombre es un slot que en el
    /// editor se lee como jerga (`structure/boulder` → "boulder") y nadie sabe
    /// qué es cuando le toca asignarle el pixel.
    #[test]
    fn every_slot_has_a_human_label() {
        for slot in game_slots() {
            assert!(!slot.label.trim().is_empty(), "slot sin nombre: {}", slot.key);
            // El nombre no puede ser la clave repetida ni su hoja: eso no es un
            // nombre, es la clave otra vez.
            let leaf = slot.key.split_once('/').map(|(_, n)| n).unwrap_or(slot.key);
            assert_ne!(slot.label, slot.key, "nombre = clave en {}", slot.key);
            assert_ne!(slot.label, leaf, "nombre = hoja de la clave en {}", slot.key);
            // Nombre en español legible: arranca en mayúscula.
            assert!(
                slot.label.chars().next().is_some_and(|c| c.is_uppercase()),
                "el nombre de {} debería arrancar en mayúscula: {}",
                slot.key,
                slot.label
            );
        }
    }

    /// Toda categoría usada tiene encabezado humano (nada cae en "OTROS").
    #[test]
    fn every_category_has_a_header() {
        for slot in game_slots() {
            assert_ne!(
                category_label(slot.category),
                "OTROS",
                "categoría sin encabezado: {}",
                slot.category
            );
        }
    }
}
