//! **TODO lo que el jugador puede plantar en el mundo, en una sola tabla.**
//!
//! El problema que resolvió: el catálogo de arte (`ha-assets/slots.rs`) tenía 150
//! objetos con nombre en castellano —árboles, muebles, autos, edificios enteros—
//! y el menú CONSTRUIR ofrecía **catorce**. O sea: el 90 % del mundo era algo que
//! el juego dibujaba solo y vos no podías tocar. Podías texturizar una heladera en
//! HA Studio y no había ninguna manera de poner una heladera.
//!
//! Ahora la lista de acá ES el menú. Agregar un objeto al juego es agregar UNA
//! línea a [`PLACEABLES`]: aparece en su categoría, con su costo, su huella y su
//! slot de arte. No hay que tocar la UI, ni la colocación, ni la persistencia.
//!
//! ## Por qué reusa `LargeAsset` y no inventa un sistema
//!
//! El motor ya sabía colocar "un sprite con huella que se registra en
//! `BuiltStructures`" — era el camino de los tres assets grandes (peñasco,
//! estatua, monumento). Ese camino ya resuelve lo difícil: huella despejada,
//! colisión fina desde el arte (`collision_footprint`), Y-sort, persistencia y
//! cobro de materiales. Extenderlo a 120 objetos no costó una línea de lógica
//! nueva: sólo creció la tabla. Un sistema paralelo habría duplicado los cuatro.
//!
//! ## El costo NO se escribe a mano
//!
//! 120 recetas a mano son 120 decisiones de balance que envejecen mal y que nadie
//! recuerda por qué son así. Acá el precio sale de **de qué está hecho** el objeto
//! y de **cuánto ocupa** ([`Material::cost_for`]): una mesa de madera de 2×1 sale
//! 6 de madera y una casa de 9×7 sale 189, por la misma regla. Es predecible para
//! el jugador (lo grande cuesta más, lo de metal cuesta metal) y se retunea
//! moviendo UN número por material.
//!
//! ⚠ Sin arte asignado, un colocable **igual funciona**: cae al quad de color
//! procedural, como todo el resto del decor (biblia §8). Por eso la tabla puede
//! adelantarse al pixel — declarás el slot, lo ponés en el mundo, y le asignás la
//! textura en HA Studio cuando quieras.

use crate::topdown::LargeAsset;

/// Categoría de un colocable. **Es la pestaña del menú CONSTRUIR**: con 120+
/// objetos una lista plana no se puede navegar en un celular, así que la
/// categoría no es adorno taxonómico, es la única forma de encontrar algo.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum PlaceCat {
    /// Árboles, rocas, vetas, flora: lo que hace que un terreno parezca un lugar.
    Naturaleza,
    /// Bancos, faroles, carteles: mobiliario urbano.
    Calle,
    /// Camas, mesas, heladeras: lo que va adentro de una casa.
    Mueble,
    /// Casas y edificios enteros, de una sola pieza.
    Edificio,
    /// Cercas, parrillas, canchas, juegos: el patio.
    Parcela,
    /// Muelles, botes, boyas, pesca.
    Agua,
    /// Autos, colectivos, helicópteros.
    Vehiculo,
    /// Peñascos, estatuas, monumentos.
    Estructura,
}

impl PlaceCat {
    /// Encabezado de la sección en el menú. **ASCII puro**: la fuente del juego no
    /// dibuja acentos (biblia §2, regla 6) — salen cajas.
    pub fn label(self) -> &'static str {
        match self {
            Self::Naturaleza => "NATURALEZA - arboles, rocas, flora",
            Self::Calle => "CALLE - bancos, faroles, carteles",
            Self::Mueble => "MUEBLES - para adentro de la casa",
            Self::Edificio => "EDIFICIOS - casas enteras de una pieza",
            Self::Parcela => "PATIO - cercas, juegos, canchas",
            Self::Agua => "AGUA - muelles, botes, pesca",
            Self::Vehiculo => "VEHICULOS",
            Self::Estructura => "ESTRUCTURAS - penascos, estatuas",
        }
    }

    /// Nombre corto — el que entra en un botón de filtro. El largo explica de qué
    /// va la categoría y sirve de encabezado; en una fila de fichas no entra.
    pub fn chip(self) -> &'static str {
        match self {
            Self::Naturaleza => "NATURALEZA",
            Self::Calle => "CALLE",
            Self::Mueble => "MUEBLES",
            Self::Edificio => "EDIFICIOS",
            Self::Parcela => "PATIO",
            Self::Agua => "AGUA",
            Self::Vehiculo => "VEHICULOS",
            Self::Estructura => "ESTRUCTURAS",
        }
    }

    /// Orden de las secciones en el menú: primero lo que se usa todo el tiempo.
    pub fn all() -> &'static [PlaceCat] {
        &[
            PlaceCat::Naturaleza,
            PlaceCat::Mueble,
            PlaceCat::Calle,
            PlaceCat::Parcela,
            PlaceCat::Edificio,
            PlaceCat::Agua,
            PlaceCat::Vehiculo,
            PlaceCat::Estructura,
        ]
    }
}

/// De qué está hecho un colocable — es lo único que decide con qué se paga.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Material {
    Madera,
    Piedra,
    Metal,
    /// Vivo: se planta, no se construye. Sale casi gratis a propósito — poblar el
    /// mundo de árboles tiene que ser barato o nadie lo hace.
    Planta,
    /// Cuero/tela: toldos, carpas, hamacas.
    Tela,
    /// Tierra/arena movida: canteros, areneros.
    Tierra,
}

impl Material {
    /// Ítem con el que se paga y cuánto por tile de huella.
    ///
    /// Los números salen de los precios que YA existían para las piezas sueltas
    /// (`BUILD_PIECES`: 1 madera el piso; `EDGE_PIECES`: 2 la pared) escalados a
    /// objetos que ocupan volumen. Un mueble no puede salir lo mismo que un piso.
    const fn per_tile(self) -> (&'static str, u32) {
        match self {
            Self::Madera => ("wood", 3),
            Self::Piedra => ("stone", 4),
            Self::Metal => ("iron_ingot", 2),
            Self::Planta => ("sticks", 1),
            Self::Tela => ("leather", 2),
            Self::Tierra => ("dirt", 2),
        }
    }

    /// Costo total de un objeto de `w × h` tiles. Mínimo 1: nada es gratis, o el
    /// menú tendría entradas que se colocan infinitas veces sin tocar la mochila.
    pub const fn cost_for(self, w: i64, h: i64) -> (&'static str, u32) {
        let (item, per) = self.per_tile();
        let tiles = (w * h) as u32;
        let qty = per * tiles;
        (item, if qty == 0 { 1 } else { qty })
    }
}

/// Una línea de la tabla. El costo se CALCULA acá, no se escribe.
const fn p(
    id: &'static str,
    label: &'static str,
    art: &'static str,
    cat: PlaceCat,
    mat: Material,
    w: i64,
    h: i64,
) -> LargeAsset {
    let (cost_item, cost_qty) = mat.cost_for(w, h);
    LargeAsset { id, label, art, cat, w, h, cost_item, cost_qty }
}

/// **EL catálogo.** Orden dentro de cada categoría = orden en el menú.
///
/// Las huellas están en tiles de 1 m y son el tamaño VISUAL además del de
/// colisión (`structures.rs` escala el sprite a `w × h`), así que salieron de
/// preguntarse cuánto mide la cosa en la vida real: una silla ocupa un metro, un
/// auto dos por cuatro, una casa chica cinco por cuatro. Si el arte que le
/// asignes tiene otra proporción, se estira — cambiá la huella acá, es el mismo
/// número para las dos cosas.
pub static PLACEABLES: &[LargeAsset] = &[
    // ── NATURALEZA ──────────────────────────────────────────────────────────
    p("tree", "Arbol (generico)", "decor/tree", PlaceCat::Naturaleza, Material::Planta, 1, 1),
    p("tree_roble", "Arbol: roble", "decor/tree_roble", PlaceCat::Naturaleza, Material::Planta, 2, 2),
    p("tree_pino", "Arbol: pino", "decor/tree_pino", PlaceCat::Naturaleza, Material::Planta, 1, 2),
    p("tree_abedul", "Arbol: abedul", "decor/tree_abedul", PlaceCat::Naturaleza, Material::Planta, 1, 2),
    p("tree_arce", "Arbol: arce", "decor/tree_arce", PlaceCat::Naturaleza, Material::Planta, 2, 2),
    p("tree_sauce", "Arbol: sauce", "decor/tree_sauce", PlaceCat::Naturaleza, Material::Planta, 2, 2),
    p("tree_palmera", "Arbol: palmera", "decor/tree_palmera", PlaceCat::Naturaleza, Material::Planta, 1, 2),
    p("tree_arbusto_g", "Arbol: arbusto grande", "decor/tree_arbusto", PlaceCat::Naturaleza, Material::Planta, 1, 1),
    p("tree_seco", "Arbol seco", "decor/tree_seco", PlaceCat::Naturaleza, Material::Planta, 1, 2),
    p("rock", "Roca", "decor/rock", PlaceCat::Naturaleza, Material::Piedra, 1, 1),
    p("rock_moon", "Roca lunar", "decor/rock_moon", PlaceCat::Naturaleza, Material::Piedra, 1, 1),
    p("rock_mars", "Roca marciana", "decor/rock_mars", PlaceCat::Naturaleza, Material::Piedra, 1, 1),
    p("ore_iron", "Veta de hierro", "decor/ore_iron", PlaceCat::Naturaleza, Material::Piedra, 1, 1),
    p("ore_gold", "Veta de oro", "decor/ore_gold", PlaceCat::Naturaleza, Material::Piedra, 1, 1),
    p("gem_red_rock", "Cristal rojo", "decor/gem_red", PlaceCat::Naturaleza, Material::Piedra, 1, 1),
    p("gem_blue_rock", "Cristal azul", "decor/gem_blue", PlaceCat::Naturaleza, Material::Piedra, 1, 1),
    p("nature_arbusto", "Arbusto", "nature/arbusto", PlaceCat::Naturaleza, Material::Planta, 1, 1),
    p("nature_flores_rojas", "Flores rojas", "nature/flores_rojas", PlaceCat::Naturaleza, Material::Planta, 1, 1),
    p("nature_flores_amarillas", "Flores amarillas", "nature/flores_amarillas", PlaceCat::Naturaleza, Material::Planta, 1, 1),
    p("nature_pasto", "Mata de pasto", "nature/pasto", PlaceCat::Naturaleza, Material::Planta, 1, 1),
    p("nature_tocon", "Tocon", "nature/tocon", PlaceCat::Naturaleza, Material::Planta, 1, 1),
    p("nature_tronco", "Tronco caido", "nature/tronco", PlaceCat::Naturaleza, Material::Planta, 2, 1),
    p("nature_hongo", "Hongo", "nature/hongo", PlaceCat::Naturaleza, Material::Planta, 1, 1),
    p("nature_cactus", "Cactus", "nature/cactus", PlaceCat::Naturaleza, Material::Planta, 1, 1),
    // ── MUEBLES ─────────────────────────────────────────────────────────────
    p("mueble_cama", "Cama", "furniture/cama", PlaceCat::Mueble, Material::Madera, 3, 3),
    p("mueble_mesa", "Mesa", "furniture/mesa", PlaceCat::Mueble, Material::Madera, 3, 2),
    p("mueble_silla", "Silla", "furniture/silla", PlaceCat::Mueble, Material::Madera, 2, 2),
    p("mueble_sofa", "Sofa", "furniture/sofa", PlaceCat::Mueble, Material::Tela, 3, 2),
    p("mueble_cocina", "Cocina", "furniture/cocina", PlaceCat::Mueble, Material::Metal, 2, 2),
    p("mueble_heladera", "Heladera", "furniture/heladera", PlaceCat::Mueble, Material::Metal, 2, 3),
    p("mueble_pileta", "Pileta de cocina", "furniture/pileta", PlaceCat::Mueble, Material::Metal, 2, 2),
    p("mueble_inodoro", "Inodoro", "furniture/inodoro", PlaceCat::Mueble, Material::Piedra, 2, 2),
    p("mueble_banera", "Banera", "furniture/banera", PlaceCat::Mueble, Material::Piedra, 3, 2),
    p("mueble_estante", "Estante", "furniture/estante", PlaceCat::Mueble, Material::Madera, 3, 2),
    p("mueble_biblioteca", "Biblioteca", "furniture/biblioteca", PlaceCat::Mueble, Material::Madera, 3, 3),
    p("mueble_lampara", "Lampara", "furniture/lampara", PlaceCat::Mueble, Material::Metal, 2, 2),
    p("mueble_escritorio", "Escritorio", "furniture/escritorio", PlaceCat::Mueble, Material::Madera, 3, 2),
    p("mueble_ropero", "Ropero", "furniture/ropero", PlaceCat::Mueble, Material::Madera, 3, 3),
    p("mueble_mostrador", "Mostrador", "furniture/mostrador", PlaceCat::Mueble, Material::Madera, 4, 2),
    p("mueble_planta", "Planta de interior", "furniture/planta", PlaceCat::Mueble, Material::Planta, 2, 2),
    // ── CALLE ───────────────────────────────────────────────────────────────
    p("calle_banco", "Banco de plaza", "prop/banco", PlaceCat::Calle, Material::Madera, 3, 2),
    p("calle_farol", "Farol de calle", "prop/farol", PlaceCat::Calle, Material::Metal, 2, 3),
    p("calle_tacho", "Tacho de basura", "prop/tacho_basura", PlaceCat::Calle, Material::Metal, 2, 2),
    p("calle_cartel", "Cartel", "prop/cartel", PlaceCat::Calle, Material::Madera, 2, 3),
    p("calle_hidrante", "Hidrante", "prop/hidrante", PlaceCat::Calle, Material::Metal, 2, 2),
    p("calle_cerca_suelta", "Cerca suelta", "prop/cerca", PlaceCat::Calle, Material::Madera, 2, 2),
    p("calle_buzon", "Buzon de calle", "prop/buzon", PlaceCat::Calle, Material::Metal, 2, 2),
    p("calle_barril", "Barril", "prop/barril", PlaceCat::Calle, Material::Madera, 2, 2),
    p("calle_caja", "Caja de madera", "prop/caja", PlaceCat::Calle, Material::Madera, 2, 2),
    p("calle_mesa_picnic", "Mesa de picnic", "prop/mesa_picnic", PlaceCat::Calle, Material::Madera, 3, 3),
    // 4x4: el tamaño sale del PNG del pack (128x128 px), no de una estimación.
    // Estaba en 2x2 y la carpa se colocaba a la mitad de lo que se dibuja.
    p("calle_carpa", "Carpa", "prop/carpa", PlaceCat::Calle, Material::Tela, 6, 6),
    p("calle_maceta", "Maceta", "prop/maceta", PlaceCat::Calle, Material::Piedra, 2, 2),
    p("calle_estatua_chica", "Estatua chica", "prop/estatua", PlaceCat::Calle, Material::Piedra, 2, 3),
    p("calle_pozo", "Pozo de agua", "prop/pozo", PlaceCat::Calle, Material::Piedra, 3, 3),
    // ── PATIO / PARCELA ─────────────────────────────────────────────────────
    p("patio_fence_madera", "Cerca de madera", "fence/madera", PlaceCat::Parcela, Material::Madera, 1, 1),
    p("patio_fence_alambre", "Cerca de alambre", "fence/alambre", PlaceCat::Parcela, Material::Metal, 1, 1),
    p("patio_fence_piedra", "Muro bajo de piedra", "fence/piedra", PlaceCat::Parcela, Material::Piedra, 1, 1),
    p("patio_fence_seto", "Seto (cerco vivo)", "fence/seto", PlaceCat::Parcela, Material::Planta, 1, 1),
    p("patio_parrilla", "Parrilla", "prop/parrilla", PlaceCat::Parcela, Material::Metal, 1, 1),
    p("patio_auto_est", "Auto estacionado", "prop/auto_estacionado", PlaceCat::Parcela, Material::Metal, 3, 6),
    p("patio_pileta_borde", "Borde de pileta", "prop/pileta_borde", PlaceCat::Parcela, Material::Piedra, 1, 1),
    p("patio_trampolin", "Trampolin", "prop/trampolin", PlaceCat::Parcela, Material::Metal, 1, 3),
    p("patio_cancha_basket", "Cancha de basquet", "prop/cancha_basket", PlaceCat::Parcela, Material::Piedra, 8, 11),
    p("patio_cancha_tenis", "Cancha de tenis", "prop/cancha_tenis", PlaceCat::Parcela, Material::Piedra, 11, 16),
    p("patio_arco_basket", "Aro de basquet", "prop/arco_basket", PlaceCat::Parcela, Material::Metal, 1, 3),
    p("patio_tobogan", "Tobogan", "prop/tobogan", PlaceCat::Parcela, Material::Metal, 3, 3),
    p("patio_hamaca", "Hamaca", "prop/hamaca", PlaceCat::Parcela, Material::Tela, 3, 1),
    p("patio_arenero", "Arenero", "prop/arenero", PlaceCat::Parcela, Material::Tierra, 3, 3),
    p("patio_tendedero", "Tendedero", "prop/tendedero", PlaceCat::Parcela, Material::Tela, 3, 1),
    p("patio_buzon_casa", "Buzon de casa", "prop/buzon_casa", PlaceCat::Parcela, Material::Madera, 1, 1),
    p("patio_cesped", "Cesped cortado", "prop/cesped_cortado", PlaceCat::Parcela, Material::Planta, 3, 3),
    p("patio_cantero", "Cantero de flores", "prop/cantero", PlaceCat::Parcela, Material::Tierra, 3, 1),
    // ── EDIFICIOS ───────────────────────────────────────────────────────────
    p("edif_casa_chica", "Casa chica", "building/casa_chica", PlaceCat::Edificio, Material::Madera, 7, 6),
    p("edif_casa_mediana", "Casa mediana", "building/casa_mediana", PlaceCat::Edificio, Material::Madera, 10, 8),
    p("edif_casa_grande", "Casa grande", "building/casa_grande", PlaceCat::Edificio, Material::Madera, 13, 10),
    p("edif_casa_country", "Casa de campo", "building/casa_country", PlaceCat::Edificio, Material::Madera, 11, 8),
    p("edif_villa", "Villa / chalet", "building/villa", PlaceCat::Edificio, Material::Piedra, 14, 11),
    p("edif_duplex", "Duplex", "building/duplex", PlaceCat::Edificio, Material::Madera, 8, 11),
    p("edif_garaje", "Garaje", "building/garaje", PlaceCat::Edificio, Material::Madera, 6, 7),
    p("edif_tienda", "Tienda", "building/tienda", PlaceCat::Edificio, Material::Madera, 8, 7),
    p("edif_mercado", "Mercado", "building/mercado", PlaceCat::Edificio, Material::Piedra, 14, 11),
    p("edif_shopping", "Shopping", "building/shopping", PlaceCat::Edificio, Material::Piedra, 16, 16),
    p("edif_oficina", "Oficina", "building/oficina", PlaceCat::Edificio, Material::Piedra, 11, 11),
    p("edif_torre", "Torre de departamentos", "building/torre", PlaceCat::Edificio, Material::Piedra, 11, 14),
    p("edif_gasolinera", "Gasolinera", "building/gasolinera", PlaceCat::Edificio, Material::Metal, 11, 8),
    p("edif_hospital", "Hospital", "building/hospital", PlaceCat::Edificio, Material::Piedra, 16, 14),
    p("edif_hotel", "Hotel", "building/hotel", PlaceCat::Edificio, Material::Piedra, 14, 16),
    p("edif_universidad", "Universidad", "building/universidad", PlaceCat::Edificio, Material::Piedra, 16, 14),
    p("edif_escuela", "Escuela", "building/escuela", PlaceCat::Edificio, Material::Piedra, 14, 11),
    p("edif_comisaria", "Comisaria", "building/comisaria", PlaceCat::Edificio, Material::Piedra, 11, 8),
    p("edif_bomberos", "Cuartel de bomberos", "building/bomberos", PlaceCat::Edificio, Material::Piedra, 11, 10),
    p("edif_correo", "Correo", "building/correo", PlaceCat::Edificio, Material::Piedra, 8, 8),
    p("edif_estacion_tren", "Estacion de tren", "building/estacion_tren", PlaceCat::Edificio, Material::Piedra, 16, 11),
    p("edif_deposito", "Deposito", "building/deposito", PlaceCat::Edificio, Material::Metal, 11, 11),
    p("edif_granero", "Granero", "building/granero", PlaceCat::Edificio, Material::Madera, 10, 8),
    p("edif_nave_industrial", "Nave industrial", "building/nave_industrial", PlaceCat::Edificio, Material::Metal, 16, 14),
    p("edif_base_militar", "Base militar", "building/base_militar", PlaceCat::Edificio, Material::Metal, 16, 16),
    // ── AGUA Y PESCA ────────────────────────────────────────────────────────
    p("agua_tronco", "Tronco flotante", "agua/tronco_flotante", PlaceCat::Agua, Material::Planta, 3, 1),
    p("agua_nenufar", "Nenufar", "agua/nenufar", PlaceCat::Agua, Material::Planta, 1, 1),
    p("agua_boya", "Boya", "agua/boya", PlaceCat::Agua, Material::Metal, 1, 1),
    p("agua_flotador", "Flotador", "agua/flotador", PlaceCat::Agua, Material::Tela, 1, 1),
    p("agua_estrella", "Estrella de mar", "agua/estrella_mar", PlaceCat::Agua, Material::Planta, 1, 1),
    p("agua_bote", "Bote", "agua/bote", PlaceCat::Agua, Material::Madera, 3, 4),
    p("agua_muelle", "Muelle", "agua/muelle", PlaceCat::Agua, Material::Madera, 3, 7),
    p("agua_junco", "Juncos", "agua/junco", PlaceCat::Agua, Material::Planta, 1, 1),
    p("agua_pesca_spot", "Zona de pesca", "fishing/spot", PlaceCat::Agua, Material::Planta, 3, 3),
    p("agua_pesca_cania", "Cana de pescar", "fishing/caña", PlaceCat::Agua, Material::Madera, 1, 1),
    p("agua_pesca_balde", "Balde de pesca", "fishing/balde", PlaceCat::Agua, Material::Metal, 1, 1),
    // ── VEHÍCULOS ───────────────────────────────────────────────────────────
    p("veh_auto", "Auto", "vehicle/auto", PlaceCat::Vehiculo, Material::Metal, 3, 6),
    p("veh_taxi", "Taxi", "vehicle/taxi", PlaceCat::Vehiculo, Material::Metal, 3, 6),
    p("veh_bus", "Colectivo", "vehicle/bus", PlaceCat::Vehiculo, Material::Metal, 4, 12),
    // 10x7: medido del PNG del pack (320x224 px). Estaba en 4x6 — el helicóptero
    // se dibujaba enorme y su colisión ocupaba menos de la mitad.
    p("veh_helicoptero", "Helicoptero", "vehicle/helicoptero", PlaceCat::Vehiculo, Material::Metal, 15, 10),
    p("veh_tanque", "Tanque", "vehicle/tanque", PlaceCat::Vehiculo, Material::Metal, 4, 8),
    // ── ESTRUCTURAS (las tres originales) ───────────────────────────────────
    p("boulder", "Penasco grande", "structure/boulder", PlaceCat::Estructura, Material::Piedra, 3, 3),
    p("statue", "Estatua", "structure/statue", PlaceCat::Estructura, Material::Piedra, 3, 4),
    p("monument", "Monumento", "structure/monument", PlaceCat::Estructura, Material::Piedra, 6, 6),
    // ── DECOR FUNCIONAL suelto ──────────────────────────────────────────────
    p("decor_mine_hole", "Boca de mina", "decor/mine_hole", PlaceCat::Estructura, Material::Piedra, 2, 2),
    p("decor_ladder", "Escalera de mano", "decor/ladder", PlaceCat::Estructura, Material::Madera, 2, 3),
    p("decor_cockpit", "Asiento de cabina", "decor/cockpit_seat", PlaceCat::Estructura, Material::Metal, 2, 2),
    p("decor_fuel", "Surtidor de combustible", "decor/fuel_source", PlaceCat::Estructura, Material::Metal, 2, 3),
    // ── DEL PACK ────────────────────────────────────────────────────────────
    // Generado por `scripts/import-pack.py` desde los PNG sueltos del pack. La
    // huella sale del tamaño del archivo (32 px = 1 tile), o sea de MIRAR el
    // arte; el nombre lo puso una persona mirando la hoja de contacto. Entre los
    // marcadores no se edita a mano.
    // === PACK:START ===
    p("nature_manzanas_camping", "Manzanas del bosque", "nature/manzanas_camping", PlaceCat::Naturaleza, Material::Planta, 1, 1),
    p("prop_sillon_camping", "Sillon de camping", "prop/sillon_camping", PlaceCat::Parcela, Material::Tela, 1, 3),
    p("prop_mochila", "Mochila", "prop/mochila", PlaceCat::Parcela, Material::Tela, 1, 1),
    p("prop_pelota", "Pelota", "prop/pelota", PlaceCat::Parcela, Material::Tela, 1, 1),
    p("prop_barbacoa", "Barbacoa", "prop/barbacoa", PlaceCat::Parcela, Material::Metal, 1, 3),
    p("prop_canasto", "Canasto", "prop/canasto", PlaceCat::Parcela, Material::Madera, 1, 1),
    p("prop_mesa_con_bancos", "Mesa con bancos", "prop/mesa_con_bancos", PlaceCat::Parcela, Material::Madera, 3, 3),
    p("prop_binoculares", "Binoculares", "prop/binoculares", PlaceCat::Parcela, Material::Metal, 1, 1),
    p("prop_botella", "Botella", "prop/botella", PlaceCat::Parcela, Material::Metal, 1, 1),
    p("prop_balde", "Balde", "prop/balde", PlaceCat::Parcela, Material::Metal, 1, 1),
    p("agua_boya_rio", "Boya de rio", "agua/boya_rio", PlaceCat::Agua, Material::Tela, 1, 1),
    p("prop_fogata", "Fogata", "prop/fogata", PlaceCat::Parcela, Material::Madera, 1, 1),
    p("prop_fogata_malvaviscos", "Fogata con malvaviscos", "prop/fogata_malvaviscos", PlaceCat::Parcela, Material::Madera, 1, 1),
    p("prop_silla_camping", "Silla de camping", "prop/silla_camping", PlaceCat::Parcela, Material::Metal, 1, 3),
    p("prop_lenia_cortada", "Lenia cortada", "prop/lenia_cortada", PlaceCat::Parcela, Material::Madera, 1, 3),
    p("prop_banco_lenia", "Banco de lenia", "prop/banco_lenia", PlaceCat::Parcela, Material::Madera, 3, 1),
    p("prop_anafe_electrico", "Anafe electrico", "prop/anafe_electrico", PlaceCat::Parcela, Material::Metal, 3, 1),
    p("agua_balde_pescado", "Balde con pescado", "agua/balde_pescado", PlaceCat::Agua, Material::Metal, 1, 3),
    p("agua_cajon_pescado", "Cajon de pescado", "agua/cajon_pescado", PlaceCat::Agua, Material::Madera, 3, 3),
    p("agua_pesquero", "Pesquero", "agua/pesquero", PlaceCat::Agua, Material::Madera, 3, 6),
    p("agua_madera_flotante", "Madera flotante", "agua/madera_flotante", PlaceCat::Agua, Material::Madera, 1, 1),
    p("prop_olla_colgante", "Olla colgante", "prop/olla_colgante", PlaceCat::Parcela, Material::Metal, 4, 3),
    p("prop_farol_camping", "Farol de camping", "prop/farol_camping", PlaceCat::Parcela, Material::Metal, 1, 1),
    p("agua_salvavidas", "Salvavidas", "agua/salvavidas", PlaceCat::Agua, Material::Tela, 1, 3),
    p("building_casilla_grande", "Casilla rodante grande", "building/casilla_grande", PlaceCat::Edificio, Material::Metal, 14, 8),
    p("building_casilla_mediana", "Casilla rodante mediana", "building/casilla_mediana", PlaceCat::Edificio, Material::Metal, 11, 8),
    p("building_casilla_chica", "Casilla rodante chica", "building/casilla_chica", PlaceCat::Edificio, Material::Metal, 8, 8),
    p("nature_musgo", "Musgo", "nature/musgo", PlaceCat::Naturaleza, Material::Planta, 1, 1),
    p("nature_hongos_bosque", "Hongos del bosque", "nature/hongos_bosque", PlaceCat::Naturaleza, Material::Planta, 1, 1),
    p("agua_muelle_camping", "Muelle de tablones", "agua/muelle_camping", PlaceCat::Agua, Material::Madera, 3, 4),
    p("agua_barril_muelle", "Barril de muelle", "agua/barril_muelle", PlaceCat::Agua, Material::Madera, 1, 1),
    p("agua_tacho_muelle", "Tacho de muelle", "agua/tacho_muelle", PlaceCat::Agua, Material::Metal, 1, 1),
    p("agua_pasarela_muelle", "Pasarela de muelle", "agua/pasarela_muelle", PlaceCat::Agua, Material::Madera, 1, 6),
    p("agua_cajones_muelle", "Cajones de muelle", "agua/cajones_muelle", PlaceCat::Agua, Material::Madera, 1, 1),
    p("agua_poste_muelle", "Poste de muelle", "agua/poste_muelle", PlaceCat::Agua, Material::Madera, 1, 3),
    p("nature_pinia", "Pinia", "nature/pinia", PlaceCat::Naturaleza, Material::Planta, 1, 1),
    p("prop_plato", "Plato", "prop/plato", PlaceCat::Parcela, Material::Metal, 1, 1),
    p("nature_roca_bosque", "Roca del bosque", "nature/roca_bosque", PlaceCat::Naturaleza, Material::Piedra, 1, 1),
    p("prop_soga", "Soga", "prop/soga", PlaceCat::Parcela, Material::Tela, 1, 1),
    p("prop_cartelera_camping", "Cartelera de camping", "prop/cartelera_camping", PlaceCat::Parcela, Material::Madera, 10, 6),
    p("prop_bolsa_dormir", "Bolsa de dormir", "prop/bolsa_dormir", PlaceCat::Parcela, Material::Tela, 1, 3),
    p("nature_brote", "Brote", "nature/brote", PlaceCat::Naturaleza, Material::Planta, 1, 1),
    p("nature_tocon_bosque", "Tocon del bosque", "nature/tocon_bosque", PlaceCat::Naturaleza, Material::Planta, 2, 1),
    p("prop_mesa_camping", "Mesa de camping", "prop/mesa_camping", PlaceCat::Parcela, Material::Madera, 1, 3),
    p("prop_mesita_camping", "Mesita de camping", "prop/mesita_camping", PlaceCat::Parcela, Material::Madera, 1, 1),
    p("prop_cubierta", "Cubierta usada", "prop/cubierta", PlaceCat::Parcela, Material::Tela, 1, 1),
    p("nature_arbol_bosque", "Arbol del bosque", "nature/arbol_bosque", PlaceCat::Naturaleza, Material::Planta, 4, 4),
    p("nature_arbol_muerto", "Arbol muerto", "nature/arbol_muerto", PlaceCat::Naturaleza, Material::Planta, 3, 3),
    p("prop_tablon", "Tablon", "prop/tablon", PlaceCat::Parcela, Material::Madera, 3, 3),
    p("vehicle_ambulancia", "Ambulancia", "vehicle/ambulancia", PlaceCat::Vehiculo, Material::Metal, 4, 10),
    p("vehicle_bote", "Bote", "vehicle/bote", PlaceCat::Vehiculo, Material::Madera, 3, 6),
    p("vehicle_colectivo", "Colectivo", "vehicle/colectivo", PlaceCat::Vehiculo, Material::Metal, 4, 10),
    p("prop_parada_colectivo", "Parada de colectivo", "prop/parada_colectivo", PlaceCat::Calle, Material::Metal, 8, 4),
    p("prop_cartel_parada", "Cartel de parada", "prop/cartel_parada", PlaceCat::Calle, Material::Metal, 2, 4),
    p("vehicle_casa_rodante", "Casa rodante", "vehicle/casa_rodante", PlaceCat::Vehiculo, Material::Metal, 3, 9),
    p("prop_carro_frutas", "Carro de frutas y flores", "prop/carro_frutas", PlaceCat::Calle, Material::Madera, 4, 6),
    p("building_estacion_servicio", "Estacion de servicio", "building/estacion_servicio", PlaceCat::Edificio, Material::Metal, 16, 15),
    p("prop_surtidor", "Surtidor de nafta", "prop/surtidor", PlaceCat::Calle, Material::Metal, 10, 9),
    p("prop_cartel_precios", "Cartel de precios", "prop/cartel_precios", PlaceCat::Calle, Material::Metal, 4, 9),
    p("vehicle_helipuerto", "Helipuerto", "vehicle/helipuerto", PlaceCat::Vehiculo, Material::Metal, 16, 16),
    p("prop_carro_panchos", "Carro de panchos", "prop/carro_panchos", PlaceCat::Calle, Material::Metal, 10, 9),
    p("vehicle_camion_helados", "Camion de helados", "vehicle/camion_helados", PlaceCat::Vehiculo, Material::Metal, 10, 9),
    p("vehicle_tuk_tuk_pizza", "Tuk tuk de pizza", "vehicle/tuk_tuk_pizza", PlaceCat::Vehiculo, Material::Metal, 9, 9),
    p("prop_telefono_publico", "Telefono publico", "prop/telefono_publico", PlaceCat::Calle, Material::Metal, 3, 4),
    p("prop_carro_comida", "Carro de comida", "prop/carro_comida", PlaceCat::Calle, Material::Madera, 4, 4),
    p("prop_silla_puesto", "Silla de puesto", "prop/silla_puesto", PlaceCat::Calle, Material::Metal, 2, 2),
    p("prop_mesa_puesto", "Mesa de puesto", "prop/mesa_puesto", PlaceCat::Calle, Material::Metal, 2, 3),
    p("vehicle_camion_tacos", "Camion de tacos", "vehicle/camion_tacos", PlaceCat::Vehiculo, Material::Metal, 10, 8),
    p("prop_cajero", "Cajero automatico", "prop/cajero", PlaceCat::Calle, Material::Metal, 3, 4),
    p("prop_antena_ciudad", "Antena de ciudad", "prop/antena_ciudad", PlaceCat::Calle, Material::Metal, 4, 6),
    p("prop_barril_ciudad", "Barril de ciudad", "prop/barril_ciudad", PlaceCat::Calle, Material::Metal, 2, 3),
    p("prop_banco_ciudad", "Banco de ciudad", "prop/banco_ciudad", PlaceCat::Calle, Material::Madera, 3, 3),
    p("prop_cartelera", "Cartelera publicitaria", "prop/cartelera", PlaceCat::Calle, Material::Metal, 8, 9),
    p("prop_tacho_negro", "Tacho negro", "prop/tacho_negro", PlaceCat::Calle, Material::Metal, 3, 3),
    p("prop_tacho_azul", "Tacho azul", "prop/tacho_azul", PlaceCat::Calle, Material::Metal, 3, 3),
    p("prop_tacho_verde", "Tacho verde", "prop/tacho_verde", PlaceCat::Calle, Material::Metal, 3, 3),
    p("prop_ladrillos", "Pila de ladrillos", "prop/ladrillos", PlaceCat::Calle, Material::Piedra, 2, 2),
    p("prop_escalera_rota", "Escalera rota", "prop/escalera_rota", PlaceCat::Calle, Material::Metal, 2, 3),
    p("prop_barril_marron", "Barril marron", "prop/barril_marron", PlaceCat::Calle, Material::Madera, 2, 3),
    p("prop_carro_golosinas", "Carro de golosinas", "prop/carro_golosinas", PlaceCat::Calle, Material::Metal, 4, 4),
    p("prop_auto_chocado", "Auto chocado", "prop/auto_chocado", PlaceCat::Calle, Material::Metal, 6, 4),
    p("prop_cono_transito", "Cono de transito", "prop/cono_transito", PlaceCat::Calle, Material::Tela, 2, 3),
    p("prop_contenedor", "Contenedor", "prop/contenedor", PlaceCat::Calle, Material::Metal, 6, 4),
    p("building_casa_contenedor", "Casa contenedor", "building/casa_contenedor", PlaceCat::Edificio, Material::Metal, 7, 7),
    p("prop_cartel_peligro", "Cartel de peligro", "prop/cartel_peligro", PlaceCat::Calle, Material::Metal, 2, 2),
    p("prop_bebedero", "Bebedero", "prop/bebedero", PlaceCat::Calle, Material::Metal, 3, 4),
    p("prop_dron", "Dron", "prop/dron", PlaceCat::Calle, Material::Metal, 2, 2),
    p("prop_contenedor_basura", "Contenedor de basura", "prop/contenedor_basura", PlaceCat::Calle, Material::Metal, 3, 3),
    p("prop_cargador_electrico", "Cargador electrico", "prop/cargador_electrico", PlaceCat::Calle, Material::Metal, 2, 3),
    p("prop_caja_electrica", "Caja electrica", "prop/caja_electrica", PlaceCat::Calle, Material::Metal, 3, 6),
    p("prop_poste_electrico", "Poste electrico", "prop/poste_electrico", PlaceCat::Calle, Material::Metal, 2, 6),
    p("nature_arbusto_flores", "Arbusto con flores", "nature/arbusto_flores", PlaceCat::Naturaleza, Material::Planta, 2, 1),
    p("nature_flores_ciudad", "Flores de vereda", "nature/flores_ciudad", PlaceCat::Naturaleza, Material::Planta, 1, 1),
    p("prop_fuente", "Fuente de plaza", "prop/fuente", PlaceCat::Calle, Material::Piedra, 3, 4),
    p("prop_rejilla", "Rejilla de desague", "prop/rejilla", PlaceCat::Calle, Material::Metal, 2, 2),
    p("vehicle_camion_basura", "Camion de basura", "vehicle/camion_basura", PlaceCat::Vehiculo, Material::Metal, 4, 12),
    p("prop_ropa_tendida", "Ropa tendida", "prop/ropa_tendida", PlaceCat::Calle, Material::Tela, 6, 4),
    p("prop_hidrante_ciudad", "Hidrante de ciudad", "prop/hidrante_ciudad", PlaceCat::Calle, Material::Metal, 2, 3),
    p("prop_cartel_info", "Cartel informativo", "prop/cartel_info", PlaceCat::Calle, Material::Metal, 2, 3),
    p("building_casilla_chatarra", "Casilla de chatarra", "building/casilla_chatarra", PlaceCat::Edificio, Material::Metal, 8, 8),
    p("building_kiosco_cafe", "Kiosco de cafe", "building/kiosco_cafe", PlaceCat::Edificio, Material::Metal, 8, 13),
    p("building_kiosco_helados", "Kiosco de helados", "building/kiosco_helados", PlaceCat::Edificio, Material::Metal, 10, 16),
    p("building_kiosco_info", "Kiosco de informes", "building/kiosco_info", PlaceCat::Edificio, Material::Metal, 7, 8),
    p("building_kiosco_hongo", "Kiosco hongo", "building/kiosco_hongo", PlaceCat::Edificio, Material::Metal, 8, 10),
    p("prop_buzon_ciudad", "Buzon de ciudad", "prop/buzon_ciudad", PlaceCat::Calle, Material::Metal, 2, 3),
    p("prop_tapa_alcantarilla", "Tapa de alcantarilla", "prop/tapa_alcantarilla", PlaceCat::Calle, Material::Metal, 2, 2),
    p("prop_parquimetro", "Parquimetro", "prop/parquimetro", PlaceCat::Calle, Material::Metal, 2, 3),
    p("prop_cabina_telefonica", "Cabina telefonica", "prop/cabina_telefonica", PlaceCat::Calle, Material::Metal, 4, 8),
    p("prop_poste_ciudad", "Poste de ciudad", "prop/poste_ciudad", PlaceCat::Calle, Material::Metal, 2, 3),
    p("prop_maceta_ciudad", "Maceta de vereda", "prop/maceta_ciudad", PlaceCat::Calle, Material::Piedra, 2, 2),
    p("building_casilla_electrica", "Casilla electrica", "building/casilla_electrica", PlaceCat::Edificio, Material::Metal, 10, 11),
    p("prop_chatarra", "Chatarra", "prop/chatarra", PlaceCat::Calle, Material::Metal, 2, 2),
    p("prop_pila_chatarra", "Pila de chatarra", "prop/pila_chatarra", PlaceCat::Calle, Material::Metal, 8, 4),
    p("nature_arbusto_ciudad", "Arbusto de vereda", "nature/arbusto_ciudad", PlaceCat::Naturaleza, Material::Planta, 1, 1),
    p("prop_panel_solar", "Panel solar", "prop/panel_solar", PlaceCat::Calle, Material::Metal, 3, 4),
    p("prop_barriles_apilados", "Barriles apilados", "prop/barriles_apilados", PlaceCat::Calle, Material::Metal, 4, 4),
    p("prop_barrera", "Barrera", "prop/barrera", PlaceCat::Calle, Material::Metal, 8, 3),
    p("prop_farol_ciudad", "Farol de ciudad", "prop/farol_ciudad", PlaceCat::Calle, Material::Metal, 3, 6),
    p("prop_valla_transito", "Valla de transito", "prop/valla_transito", PlaceCat::Calle, Material::Metal, 3, 2),
    p("prop_semaforo", "Semaforo", "prop/semaforo", PlaceCat::Calle, Material::Metal, 2, 6),
    p("prop_senial_transito", "Senial de transito", "prop/senial_transito", PlaceCat::Calle, Material::Metal, 2, 4),
    p("prop_pila_basura", "Pila de basura", "prop/pila_basura", PlaceCat::Calle, Material::Metal, 6, 3),
    p("prop_tacho_ciudad", "Tacho de vereda", "prop/tacho_ciudad", PlaceCat::Calle, Material::Metal, 2, 3),
    p("nature_arbol_ciudad", "Arbol de vereda", "nature/arbol_ciudad", PlaceCat::Naturaleza, Material::Planta, 2, 3),
    p("building_entrada_estacionamiento", "Entrada de estacionamiento", "building/entrada_estacionamiento", PlaceCat::Edificio, Material::Piedra, 6, 8),
    p("building_tanque_agua", "Tanque de agua", "building/tanque_agua", PlaceCat::Edificio, Material::Metal, 4, 10),
    p("building_turbina_eolica", "Turbina eolica", "building/turbina_eolica", PlaceCat::Edificio, Material::Metal, 8, 14),
    p("furniture_batidora", "Batidora", "furniture/batidora", PlaceCat::Mueble, Material::Metal, 2, 2),
    p("furniture_licuadora", "Licuadora", "furniture/licuadora", PlaceCat::Mueble, Material::Metal, 2, 3),
    p("furniture_cafetera", "Cafetera", "furniture/cafetera", PlaceCat::Mueble, Material::Metal, 2, 3),
    p("furniture_microondas", "Microondas", "furniture/microondas", PlaceCat::Mueble, Material::Metal, 2, 2),
    p("furniture_estante_microondas", "Estante con microondas", "furniture/estante_microondas", PlaceCat::Mueble, Material::Metal, 2, 3),
    p("furniture_dispensador_agua", "Dispensador de agua", "furniture/dispensador_agua", PlaceCat::Mueble, Material::Metal, 2, 3),
    p("furniture_parrilla_industrial", "Parrilla industrial", "furniture/parrilla_industrial", PlaceCat::Mueble, Material::Metal, 3, 3),
    p("furniture_parrilla_reja", "Parrilla con reja", "furniture/parrilla_reja", PlaceCat::Mueble, Material::Metal, 3, 3),
    p("furniture_parrilla_brochetas", "Parrilla con brochetas", "furniture/parrilla_brochetas", PlaceCat::Mueble, Material::Metal, 3, 3),
    p("furniture_olla_arrocera", "Olla arrocera", "furniture/olla_arrocera", PlaceCat::Mueble, Material::Metal, 2, 3),
    p("furniture_olla_presion", "Olla a presion", "furniture/olla_presion", PlaceCat::Mueble, Material::Metal, 2, 3),
    p("furniture_delantal_blanco", "Delantal blanco", "furniture/delantal_blanco", PlaceCat::Mueble, Material::Tela, 2, 3),
    p("furniture_delantal_verde", "Delantal verde", "furniture/delantal_verde", PlaceCat::Mueble, Material::Tela, 2, 3),
    p("furniture_delantal_azul", "Delantal azul", "furniture/delantal_azul", PlaceCat::Mueble, Material::Tela, 2, 3),
    p("furniture_delantal_rojo", "Delantal rojo", "furniture/delantal_rojo", PlaceCat::Mueble, Material::Tela, 2, 3),
    p("furniture_lampara_pie", "Lampara de pie", "furniture/lampara_pie", PlaceCat::Mueble, Material::Metal, 2, 4),
    p("furniture_lampara_pie_alta", "Lampara de pie alta", "furniture/lampara_pie_alta", PlaceCat::Mueble, Material::Metal, 2, 4),
    p("furniture_tabla_cortar", "Tabla de cortar", "furniture/tabla_cortar", PlaceCat::Mueble, Material::Madera, 2, 2),
    p("furniture_tabla_cortar_larga", "Tabla de cortar larga", "furniture/tabla_cortar_larga", PlaceCat::Mueble, Material::Madera, 3, 2),
    p("furniture_ropero_morado", "Ropero morado", "furniture/ropero_morado", PlaceCat::Mueble, Material::Madera, 3, 4),
    p("furniture_televisor", "Televisor", "furniture/televisor", PlaceCat::Mueble, Material::Metal, 3, 3),
    p("furniture_espejo_pie", "Espejo de pie", "furniture/espejo_pie", PlaceCat::Mueble, Material::Madera, 2, 3),
    p("furniture_banco_tapizado", "Banco tapizado", "furniture/banco_tapizado", PlaceCat::Mueble, Material::Tela, 3, 2),
    p("furniture_mesa_luz", "Mesa de luz", "furniture/mesa_luz", PlaceCat::Mueble, Material::Madera, 2, 3),
    p("furniture_espejo_pared", "Espejo de pared", "furniture/espejo_pared", PlaceCat::Mueble, Material::Madera, 2, 2),
    p("furniture_cortina", "Cortina", "furniture/cortina", PlaceCat::Mueble, Material::Tela, 2, 3),
    p("furniture_planta_maceta_alta", "Planta en maceta alta", "furniture/planta_maceta_alta", PlaceCat::Mueble, Material::Planta, 3, 3),
    p("furniture_palmera_interior", "Palmera de interior", "furniture/palmera_interior", PlaceCat::Mueble, Material::Planta, 3, 3),
    p("furniture_planta_chica", "Planta chica", "furniture/planta_chica", PlaceCat::Mueble, Material::Planta, 2, 3),
    p("furniture_planta_colgante", "Planta colgante", "furniture/planta_colgante", PlaceCat::Mueble, Material::Planta, 2, 3),
    p("furniture_planta_alta", "Planta alta", "furniture/planta_alta", PlaceCat::Mueble, Material::Planta, 2, 3),
    p("furniture_planta_frondosa", "Planta frondosa", "furniture/planta_frondosa", PlaceCat::Mueble, Material::Planta, 3, 3),
    p("furniture_tocador", "Tocador con espejo", "furniture/tocador", PlaceCat::Mueble, Material::Madera, 3, 3),
    p("furniture_lampara_mesa", "Lampara de mesa", "furniture/lampara_mesa", PlaceCat::Mueble, Material::Metal, 2, 2),
    p("furniture_almohadon", "Almohadon", "furniture/almohadon", PlaceCat::Mueble, Material::Tela, 2, 2),
    p("furniture_sofa_cama", "Sofa cama", "furniture/sofa_cama", PlaceCat::Mueble, Material::Tela, 3, 3),
    p("furniture_sofa_lampara", "Sofa con lampara", "furniture/sofa_lampara", PlaceCat::Mueble, Material::Tela, 3, 3),
    p("furniture_sofa_claro", "Sofa claro", "furniture/sofa_claro", PlaceCat::Mueble, Material::Tela, 3, 3),
    p("furniture_armario_madera", "Armario de madera", "furniture/armario_madera", PlaceCat::Mueble, Material::Madera, 3, 4),
    p("furniture_armario_espejo", "Armario con espejo", "furniture/armario_espejo", PlaceCat::Mueble, Material::Madera, 3, 4),
    p("furniture_alacena", "Alacena", "furniture/alacena", PlaceCat::Mueble, Material::Madera, 3, 3),
    p("furniture_vitrina", "Vitrina", "furniture/vitrina", PlaceCat::Mueble, Material::Madera, 3, 3),
    p("furniture_placard_angosto", "Placard angosto", "furniture/placard_angosto", PlaceCat::Mueble, Material::Madera, 2, 4),
    p("furniture_placard_espejo", "Placard con espejo", "furniture/placard_espejo", PlaceCat::Mueble, Material::Madera, 2, 4),
    p("furniture_estante_plantas", "Estante con plantas", "furniture/estante_plantas", PlaceCat::Mueble, Material::Madera, 2, 3),
    p("furniture_mesa_con_lampara", "Mesa con lampara", "furniture/mesa_con_lampara", PlaceCat::Mueble, Material::Madera, 2, 3),
    p("furniture_mesa_con_planta", "Mesa con planta", "furniture/mesa_con_planta", PlaceCat::Mueble, Material::Madera, 2, 3),
    p("furniture_canasto_frutas", "Canasto de frutas", "furniture/canasto_frutas", PlaceCat::Mueble, Material::Madera, 2, 3),
    // === PACK:END ===
];

/// Cuántos colocables hay en total.
pub fn placeable_count() -> usize {
    PLACEABLES.len()
}

/// Los colocables de una categoría, en orden de tabla.
pub fn placeables_in(cat: PlaceCat) -> impl Iterator<Item = (usize, &'static LargeAsset)> {
    PLACEABLES.iter().enumerate().filter(move |(_, a)| a.cat == cat)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    /// **Un colocable cuya categoría no esté en `all()` no aparece en ningún
    /// lado.** El menú CONSTRUIR dibuja un grupo por categoría de `all()`: si
    /// alguien agrega una categoría nueva y se olvida de listarla ahí, esos
    /// objetos quedan declarados, con nombre e icono, y sin forma de llegar a
    /// ellos desde el juego. Es exactamente la clase de bug que este test
    /// existe para que no vuelva.
    #[test]
    fn toda_categoria_usada_esta_en_el_menu() {
        let en_menu = PlaceCat::all();
        for a in PLACEABLES {
            assert!(
                en_menu.contains(&a.cat),
                "{} es de la categoria {:?}, que no esta en PlaceCat::all() — no se puede elegir en CONSTRUIR",
                a.id,
                a.cat
            );
        }
        // Y al revés: cada grupo del menú tiene algo adentro (una ficha de
        // filtro que abre una lista vacía se lee como que el juego se rompió).
        for &cat in PlaceCat::all() {
            assert!(
                placeables_in(cat).next().is_some(),
                "la categoria {cat:?} no tiene ni un colocable"
            );
        }
    }

    /// El nombre corto entra en una ficha: ASCII (regla 6) y sin frases.
    #[test]
    fn el_nombre_corto_entra_en_una_ficha() {
        for &cat in PlaceCat::all() {
            let chip = cat.chip();
            assert!(chip.is_ascii(), "ficha con caracteres que la fuente no dibuja: {chip}");
            assert!(chip.len() <= 12, "ficha demasiado larga para el boton: {chip}");
        }
    }

    #[test]
    fn todo_colocable_tiene_nombre_humano() {
        // Un objeto sin nombre legible es un objeto que nadie va a encontrar en el
        // menú. Y ASCII puro: la fuente del juego no dibuja acentos (biblia §2.6).
        for a in PLACEABLES {
            assert!(!a.label.trim().is_empty(), "colocable sin nombre: {}", a.id);
            assert_ne!(a.label, a.id, "nombre = id en {}", a.id);
            assert!(
                a.label.is_ascii(),
                "el nombre de {} tiene caracteres que la fuente no dibuja: {}",
                a.id,
                a.label
            );
            assert!(
                a.label.chars().next().is_some_and(|c| c.is_uppercase()),
                "el nombre de {} deberia arrancar en mayuscula: {}",
                a.id,
                a.label
            );
        }
    }

    #[test]
    fn los_ids_y_los_slots_no_se_repiten() {
        // Dos entradas con el mismo id serían indistinguibles al persistir; dos con
        // el mismo slot de arte serían dos botones que ponen exactamente lo mismo.
        let mut ids = HashSet::new();
        let mut arts = HashSet::new();
        for a in PLACEABLES {
            assert!(ids.insert(a.id), "id repetido: {}", a.id);
            assert!(arts.insert(a.art), "slot de arte repetido: {}", a.art);
        }
    }

    #[test]
    fn nada_es_gratis_ni_absurdo() {
        for a in PLACEABLES {
            assert!(a.cost_qty > 0, "{} sale gratis", a.id);
            assert!(a.w > 0 && a.h > 0, "{} no ocupa nada", a.id);
            // Techo de cordura: una huella mayor que el anillo de chunks streameado
            // no se podría ni ver entera al colocarla.
            assert!(a.w <= 16 && a.h <= 16, "{} es absurdamente grande", a.id);
        }
    }

    #[test]
    fn el_costo_crece_con_el_tamanio() {
        // La promesa del modelo: lo grande cuesta más, y del mismo material.
        let (i1, q1) = Material::Madera.cost_for(1, 1);
        let (i2, q2) = Material::Madera.cost_for(3, 3);
        assert_eq!(i1, i2);
        assert!(q2 > q1, "una casa no puede salir lo mismo que una silla");
    }

    #[test]
    fn todas_las_categorias_tienen_algo() {
        // Una pestaña vacía en el menú es un callejón sin salida para el jugador.
        for &cat in PlaceCat::all() {
            assert!(
                placeables_in(cat).next().is_some(),
                "la categoria {cat:?} no tiene ni un objeto"
            );
        }
    }

    #[test]
    fn el_menu_cubre_la_tabla_entera() {
        // Recorrer las categorías tiene que devolver TODO: si una entrada tuviera
        // una categoría fuera de `all()`, existiría en la tabla y sería
        // inalcanzable desde el menú — un objeto fantasma.
        let visto: usize = PlaceCat::all().iter().map(|&c| placeables_in(c).count()).sum();
        assert_eq!(visto, PLACEABLES.len(), "hay colocables que el menu no muestra");
    }
}