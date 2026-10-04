//! **Vehículos: qué hay, cómo andan y cómo maneja un vecino.**
//!
//! Tres cosas, puras y deterministas, para que el cliente (`ha-game/src/vehiculos.rs`)
//! sólo tenga que dibujar y preguntar:
//!
//! 1. **El catálogo** ([`Clase`], [`ficha`], [`MODELOS`]): de auto a lancha, con sus
//!    números. Las MEDIDAS no están acá: salen del sprite medido por
//!    `scripts/import-vehiculos.py` (un auto del pack mide 2,3 × 4,1 m porque así
//!    lo dibujó el pack, no porque alguien lo escribió).
//! 2. **La física** ([`paso`]): arcade a lo GTA 2 / San Andreas visto desde arriba.
//!    Se maneja por cualquier parte que pise una persona —campo, playa, vereda—,
//!    pero el SUELO manda ([`Suelo`]): en asfalto rinde todo, en pasto la mitad y
//!    en arena se entierra. Choca contra lo mismo que frena a quien camina (el
//!    cliente le pasa `Obstaculos`), así que el relieve se sube por las mismas
//!    rampas y un acantilado es una pared también al volante.
//! 3. **El conductor** ([`conducir`]): un vecino al volante sigue la MANO de la
//!    calle con la misma regla que pinta la línea divisoria (`roads::carril_con`);
//!    en un cruce sigue derecho o dobla a la derecha, que es lo único que se hace
//!    sin cruzar el carril contrario, y en una calle sin salida da la vuelta.
//!
//! Rust-only, como `civ`: no lo corre el servidor (el multijugador todavía no
//! comparte mundo, biblia §15). Si algún día lo corre, se espeja en TS.

use crate::topdown::Tile;

/// Qué clase de vehículo es. Decide los números ([`ficha`]) y por dónde puede ir.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Clase {
    Auto,
    Taxi,
    Colectivo,
    Camion,
    Basura,
    Bomberos,
    Ambulancia,
    /// El coche de policía: sólo nace cuando hay búsqueda (`ha-game/src/policia.rs`).
    Patrulla,
    CasaRodante,
    Moto,
    Bici,
    Lancha,
}

/// Por dónde anda.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Medio {
    Tierra,
    Agua,
}

/// Los números de una clase. Velocidades en m/s (1 tile = 1 m), giro en rad/s a
/// velocidad de crucero.
#[derive(Clone, Copy, Debug)]
pub struct Ficha {
    pub nombre: &'static str,
    pub vel_max: f32,
    pub acel: f32,
    pub freno: f32,
    pub giro: f32,
    /// Vida de la carrocería. A cero queda chatarra: no arranca más.
    pub salud: f32,
    pub medio: Medio,
    /// Dos ruedas: rinde más en el campo y se pasa por donde un auto no.
    pub dos_ruedas: bool,
    /// La bici no tiene motor: no hace ruido ni se roba "con las llaves".
    pub motor: bool,
    /// Metros que rinde con el tanque lleno. `0` = no gasta (bici, lancha de juguete).
    pub alcance: f32,
}

pub fn ficha(c: Clase) -> Ficha {
    let f = |nombre, vel_max, acel, freno, giro, salud| Ficha {
        nombre,
        vel_max,
        acel,
        freno,
        giro,
        salud,
        medio: Medio::Tierra,
        dos_ruedas: false,
        motor: true,
        alcance: 12_000.0,
    };
    match c {
        Clase::Auto => f("Auto", 20.0, 6.0, 12.0, 2.4, 100.0),
        Clase::Taxi => f("Taxi", 20.0, 6.0, 12.0, 2.4, 100.0),
        Clase::Colectivo => Ficha { alcance: 9_000.0, ..f("Colectivo", 13.0, 3.0, 8.0, 1.5, 220.0) },
        Clase::Camion => Ficha { alcance: 8_000.0, ..f("Camión", 14.0, 3.0, 8.0, 1.5, 250.0) },
        Clase::Basura => f("Camión de basura", 12.0, 2.5, 8.0, 1.4, 260.0),
        Clase::Bomberos => f("Autobomba", 16.0, 3.5, 9.0, 1.5, 300.0),
        Clase::Ambulancia => f("Ambulancia", 19.0, 5.0, 11.0, 2.0, 150.0),
        Clase::CasaRodante => f("Casa rodante", 13.0, 2.8, 8.0, 1.4, 200.0),
        Clase::Patrulla => Ficha { alcance: 20_000.0, ..f("Patrulla", 23.0, 7.5, 13.0, 2.6, 160.0) },
        Clase::Moto => Ficha { dos_ruedas: true, alcance: 18_000.0, ..f("Moto", 24.0, 9.0, 14.0, 3.2, 50.0) },
        Clase::Bici => Ficha { dos_ruedas: true, motor: false, alcance: 0.0, ..f("Bicicleta", 8.0, 4.0, 8.0, 3.4, 30.0) },
        Clase::Lancha => Ficha { medio: Medio::Agua, alcance: 9_000.0, ..f("Lancha", 16.0, 4.0, 4.0, 1.8, 120.0) },
    }
}

/// **Cada clave de arte con su clase.** Las claves las publica
/// `scripts/import-vehiculos.py` (y `dibujar-vehiculos.py` la moto y la bici);
/// que existan con sus cuatro poses lo comprueba `todo_modelo_tiene_sus_cuatro_poses`
/// en `ha-game`.
pub const MODELOS: &[(&str, Clase)] = &[
    ("vehicle/coche_1", Clase::Auto),
    ("vehicle/coche_2", Clase::Auto),
    ("vehicle/coche_3", Clase::Auto),
    ("vehicle/coche_4", Clase::Auto),
    ("vehicle/coche_5", Clase::Auto),
    ("vehicle/coche_6", Clase::Auto),
    ("vehicle/coche_7", Clase::Auto),
    ("vehicle/coche_8", Clase::Auto),
    ("vehicle/coche_9", Clase::Auto),
    ("vehicle/coche_10", Clase::Auto),
    ("vehicle/coche_11", Clase::Taxi),
    ("vehicle/coche_12", Clase::Auto),
    ("vehicle/coche_13", Clase::Auto),
    ("vehicle/coche_14", Clase::Auto),
    ("vehicle/coche_15", Clase::Auto),
    ("vehicle/coche_16", Clase::Auto),
    ("vehicle/coche_17", Clase::Auto),
    ("vehicle/coche_18", Clase::Auto),
    ("vehicle/coche_19", Clase::Auto),
    ("vehicle/coche_20", Clase::Auto),
    ("vehicle/coche_21", Clase::Auto),
    ("vehicle/coche_22", Clase::Auto),
    ("vehicle/coche_23", Clase::Auto),
    ("vehicle/coche_24", Clase::Auto),
    ("vehicle/coche_25", Clase::Auto),
    ("vehicle/coche_26", Clase::Auto),
    ("vehicle/colectivo_1", Clase::Colectivo),
    ("vehicle/colectivo_2", Clase::Colectivo),
    ("vehicle/colectivo_3", Clase::Colectivo),
    ("vehicle/colectivo_4", Clase::Colectivo),
    ("vehicle/colectivo_5", Clase::Colectivo),
    ("vehicle/colectivo_6", Clase::Colectivo),
    ("vehicle/casa_rodante", Clase::CasaRodante),
    ("vehicle/ambulancia", Clase::Ambulancia),
    ("vehicle/lancha", Clase::Lancha),
    ("vehicle/camion", Clase::Camion),
    ("vehicle/basura", Clase::Basura),
    ("vehicle/bomberos", Clase::Bomberos),
    ("vehicle/moto", Clase::Moto),
    ("vehicle/bici", Clase::Bici),
];

/// Un nombre que escribe una persona (`taxi`, `colectivo`, `coche_7`) → la clave de arte.
/// Lo usa la consola (`/auto taxi`).
pub fn modelo_por_nombre(nombre: &str) -> Option<&'static str> {
    let n = nombre.trim().to_lowercase();
    if let Some((k, _)) = MODELOS.iter().find(|(k, _)| k.strip_prefix("vehicle/") == Some(n.as_str())) {
        return Some(k);
    }
    Some(match n.as_str() {
        "auto" | "coche" | "carro" => "vehicle/coche_1",
        "taxi" => "vehicle/coche_11",
        "patrulla" | "policia" | "policía" => "vehicle/coche_13",
        "colectivo" | "bus" | "micro" => "vehicle/colectivo_1",
        "camion" | "camión" => "vehicle/camion",
        "basura" => "vehicle/basura",
        "bomberos" | "autobomba" => "vehicle/bomberos",
        "ambulancia" => "vehicle/ambulancia",
        "casa" | "casa_rodante" => "vehicle/casa_rodante",
        "lancha" | "barco" => "vehicle/lancha",
        "moto" => "vehicle/moto",
        "bici" | "bicicleta" => "vehicle/bici",
        _ => return None,
    })
}

pub fn clase_de(clave: &str) -> Option<Clase> {
    MODELOS.iter().find(|(k, _)| *k == clave).map(|&(_, c)| c)
}

/// **Qué modelo sale en el tránsito**, sorteado por `h`. Mayoría de autos, algún
/// colectivo y camión, pocas motos y muy de vez en cuando una ambulancia o una
/// autobomba: una ciudad con media flota de bomberos no se cree.
pub fn modelo_de_transito(h: u32) -> &'static str {
    let r = h % 1000;
    let autos: Vec<&str> = MODELOS.iter().filter(|(_, c)| *c == Clase::Auto).map(|(k, _)| *k).collect();
    let de = |c: Clase| -> Vec<&'static str> { MODELOS.iter().filter(|(_, x)| *x == c).map(|(k, _)| *k).collect() };
    let elige = |v: Vec<&'static str>| v[(h / 1000) as usize % v.len()];
    match r {
        0..=699 => autos[(h / 1000) as usize % autos.len()],
        700..=779 => "vehicle/coche_11",
        780..=839 => elige(de(Clase::Colectivo)),
        840..=889 => "vehicle/camion",
        890..=909 => "vehicle/basura",
        910..=929 => "vehicle/ambulancia",
        930..=939 => "vehicle/bomberos",
        940..=949 => "vehicle/casa_rodante",
        _ => "vehicle/moto",
    }
}

/// El suelo bajo las ruedas, reducido a lo que importa para manejar.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Suelo {
    Asfalto,
    /// Vereda, losa, piso, roca pelada, rampa: firme pero no es pista.
    Firme,
    Tierra,
    Pasto,
    Arena,
    /// Hielo y nieve: rinde poco y no agarra.
    Hielo,
    /// Agua de poco fondo: un auto la vadea despacio, la lancha encalla.
    Bajio,
    /// Agua honda: ningún auto entra; es el medio de la lancha.
    Hondo,
}

pub fn suelo_de(t: Tile) -> Suelo {
    match t {
        Tile::Asphalt => Suelo::Asfalto,
        Tile::Dirt => Suelo::Tierra,
        Tile::Grass => Suelo::Pasto,
        Tile::Sand => Suelo::Arena,
        Tile::Ice => Suelo::Hielo,
        Tile::Water => Suelo::Bajio,
        Tile::WaterDeep => Suelo::Hondo,
        _ => Suelo::Firme,
    }
}

impl Suelo {
    /// Fracción de la velocidad máxima que permite este suelo (0 = no se entra).
    pub fn tope(self, f: &Ficha) -> f32 {
        match f.medio {
            Medio::Agua => match self {
                Suelo::Hondo => 1.0,
                Suelo::Bajio => 0.5,
                _ => 0.0,
            },
            Medio::Tierra => {
                let campo = if f.dos_ruedas { 0.15 } else { 0.0 };
                match self {
                    Suelo::Asfalto => 1.0,
                    Suelo::Firme => 0.85,
                    Suelo::Tierra => 0.72 + campo,
                    Suelo::Pasto => 0.62 + campo,
                    Suelo::Arena => 0.42 + campo,
                    Suelo::Hielo => 0.5,
                    Suelo::Bajio => 0.28,
                    Suelo::Hondo => 0.0,
                }
            }
        }
    }

    /// Cuánto responde a la dirección y al acelerador (1 = asfalto seco).
    pub fn agarre(self) -> f32 {
        match self {
            Suelo::Asfalto | Suelo::Hondo => 1.0,
            Suelo::Firme => 0.95,
            Suelo::Tierra => 0.8,
            Suelo::Pasto => 0.75,
            Suelo::Arena => 0.6,
            Suelo::Hielo => 0.35,
            Suelo::Bajio => 0.5,
        }
    }
}

/// El vehículo en el mundo. `rumbo` en radianes sobre el plano XZ con la misma
/// convención que el jugador (`cos` → x, `sin` → z; `+z` es el sur de la pantalla).
#[derive(Clone, Copy, Debug)]
pub struct Estado {
    pub x: f32,
    pub z: f32,
    pub rumbo: f32,
    /// Velocidad a lo largo del rumbo (negativa = marcha atrás).
    pub vel: f32,
    pub salud: f32,
    /// Medidas en metros, del sprite medido.
    pub largo: f32,
    pub ancho: f32,
    /// Combustible, 0..1 (1 = tanque lleno).
    pub comb: f32,
}

impl Estado {
    pub fn nuevo(x: f32, z: f32, rumbo: f32, f: &Ficha, largo: f32, ancho: f32) -> Self {
        Self { x, z, rumbo, vel: 0.0, salud: f.salud, largo, ancho, comb: 1.0 }
    }

    pub fn chatarra(&self) -> bool {
        self.salud <= 0.0
    }

    /// ¿Tiene con qué andar? (la bici nunca se queda sin nada)
    pub fn con_combustible(&self, f: &Ficha) -> bool {
        f.alcance <= 0.0 || self.comb > 0.0
    }

    /// Los círculos con que choca: uno por cada "ancho" de largo, sobre el eje.
    pub fn probes(&self, x: f32, z: f32, rumbo: f32) -> ([(f32, f32); 4], usize, f32) {
        let r = self.ancho * 0.45;
        let n = ((self.largo / self.ancho).ceil() as usize).clamp(1, 4);
        let medio = (self.largo * 0.5 - r).max(0.0);
        let (c, s) = (rumbo.cos(), rumbo.sin());
        let mut out = [(x, z); 4];
        for (i, o) in out.iter_mut().enumerate().take(n) {
            let t = if n == 1 { 0.0 } else { -medio + 2.0 * medio * i as f32 / (n - 1) as f32 };
            *o = (x + c * t, z + s * t);
        }
        (out, n, r)
    }
}

/// Lo que pide quien maneja.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Mando {
    /// −1..1: positivo acelera, negativo frena y después da marcha atrás.
    pub acel: f32,
    /// −1..1: positivo gira en sentido de rumbo creciente.
    pub giro: f32,
}

fn envolver(a: f32) -> f32 {
    let tau = std::f32::consts::TAU;
    let mut a = a.rem_euclid(tau);
    if a > std::f32::consts::PI {
        a -= tau;
    }
    a
}

/// **El stick manda el RUMBO, no el volante** (control relativo a la pantalla, a
/// lo GTA 2): apuntás hacia dónde querés ir y el vehículo dobla hacia ahí con su
/// radio de giro; la fuerza del stick es el acelerador. Apuntar hacia atrás frena
/// y, ya parado, da marcha atrás con la cola hacia donde apuntás. En una pantalla
/// táctil es lo único que se maneja sin mirar los dedos, y en teclado WASD
/// funciona igual.
pub fn mando_desde_stick(e: &Estado, sx: f32, sz: f32) -> Mando {
    let mag = (sx * sx + sz * sz).sqrt().min(1.0);
    if mag < 0.15 {
        return Mando { acel: 0.0, giro: 0.0 };
    }
    let deseado = sz.atan2(sx);
    let diff = envolver(deseado - e.rumbo);
    let atras = diff.abs() > 1.95; // ~112°
    if !atras {
        return Mando { acel: mag, giro: (diff * 2.5).clamp(-1.0, 1.0) };
    }
    if e.vel > 1.0 {
        return Mando { acel: -1.0, giro: 0.0 };
    }
    // Marcha atrás: la COLA apunta al stick.
    let diff_cola = envolver(deseado - (e.rumbo + std::f32::consts::PI));
    Mando { acel: -mag, giro: (-diff_cola * 2.5).clamp(-1.0, 1.0) }
}

/// Resultado de un paso de física.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Paso {
    /// Velocidad a la que chocó (0 si no chocó). El cliente la usa para el sonido,
    /// el daño a quien atropella y el sacudón de cámara.
    pub choque: f32,
}

/// A partir de qué velocidad un choque abolla.
pub const CHOQUE_QUE_DANIA: f32 = 4.0;

/// **Un paso de física.**
///
/// - `suelo(tx, tz)`: el suelo de la celda.
/// - `choca(x, z, r)`: ¿un círculo de radio `r` en `(x, z)` pisa algo que frena?
///   Es `Obstaculos` del lado del cliente: el mismo mundo con el que choca el
///   jugador a pie.
pub fn paso(
    e: &mut Estado,
    m: Mando,
    f: &Ficha,
    dt: f32,
    suelo: &impl Fn(i64, i64) -> Suelo,
    choca: &impl Fn(f32, f32, f32) -> bool,
) -> Paso {
    let mut out = Paso::default();
    let s = suelo(e.x.floor() as i64, e.z.floor() as i64);
    let agarre = s.agarre();
    let (mut acel, giro) = if e.chatarra() { (0.0, 0.0) } else { (m.acel.clamp(-1.0, 1.0), m.giro.clamp(-1.0, 1.0)) };
    // Sin nafta el motor no responde: el auto rueda por inercia hasta frenarse solo.
    if !e.con_combustible(f) && acel > 0.0 {
        acel = 0.0;
    }
    if f.alcance > 0.0 && acel != 0.0 {
        // Gasto por metro recorrido, más un goteo por tener el pie en el acelerador.
        e.comb = (e.comb - (e.vel.abs() * dt + 0.4 * acel.abs() * dt) / f.alcance).max(0.0);
    }
    let tope = f.vel_max * s.tope(f).max(0.05);
    let tope_atras = tope * 0.35;

    // --- Velocidad ---
    if acel > 0.0 {
        if e.vel < -0.2 {
            e.vel = (e.vel + f.freno * acel * dt).min(0.0);
        } else {
            e.vel += f.acel * agarre * acel * dt;
        }
    } else if acel < 0.0 {
        if e.vel > 0.2 {
            e.vel = (e.vel + f.freno * acel * dt).max(0.0);
        } else {
            e.vel += f.acel * 0.6 * agarre * acel * dt;
        }
    } else {
        // Rodando suelto: el roce del suelo lo frena (el pasto más que el asfalto).
        let roce = 1.5 + (1.0 - s.tope(f).min(1.0)) * 4.0;
        e.vel = if e.vel > 0.0 { (e.vel - roce * dt).max(0.0) } else { (e.vel + roce * dt).min(0.0) };
    }
    // Pasado del tope (salió del asfalto a fondo): se frena de a poco, no de golpe.
    if e.vel > tope {
        e.vel = (e.vel - 8.0 * dt).max(tope);
    }
    if e.vel < -tope_atras {
        e.vel = (e.vel + 8.0 * dt).min(-tope_atras);
    }

    // --- Rumbo: sólo gira si rueda (un auto parado no da vueltas en el lugar) ---
    let rueda = (e.vel.abs() / 3.0).min(1.0);
    let signo = if e.vel < 0.0 { -1.0 } else { 1.0 };
    let nuevo_rumbo = envolver(e.rumbo + giro * f.giro * agarre.max(0.5) * rueda * signo * dt);

    // --- Movimiento con colisión ---
    let medidas = *e;
    let bloquea = |x: f32, z: f32, rumbo: f32| -> bool {
        let (ps, n, r) = medidas.probes(x, z, rumbo);
        // El suelo se mira en las cuatro ESQUINAS de la carrocería (no en el centro
        // de los círculos): un auto no puede tener el paragolpes metido en el mar.
        let (c, sn) = (rumbo.cos(), rumbo.sin());
        let (hl, ha) = (medidas.largo * 0.5, medidas.ancho * 0.5);
        for (a, b) in [(hl, ha), (hl, -ha), (-hl, ha), (-hl, -ha)] {
            let (px, pz) = (x + c * a - sn * b, z + sn * a + c * b);
            if s_no_admite(f, suelo(px.floor() as i64, pz.floor() as i64)) {
                return true;
            }
        }
        ps.iter().take(n).any(|&(px, pz)| choca(px, pz, r))
    };
    let (dx, dz) = (nuevo_rumbo.cos() * e.vel * dt, nuevo_rumbo.sin() * e.vel * dt);
    if !bloquea(e.x + dx, e.z + dz, nuevo_rumbo) {
        e.x += dx;
        e.z += dz;
        e.rumbo = nuevo_rumbo;
    } else if !bloquea(e.x + dx, e.z, e.rumbo) && dx.abs() > 1e-4 {
        // Roza: se desliza por la pared perdiendo velocidad.
        e.x += dx;
        e.vel *= 0.9;
    } else if !bloquea(e.x, e.z + dz, e.rumbo) && dz.abs() > 1e-4 {
        e.z += dz;
        e.vel *= 0.9;
    } else {
        out.choque = e.vel.abs();
        e.vel = -e.vel * 0.2;
        if !bloquea(e.x, e.z, nuevo_rumbo) {
            e.rumbo = nuevo_rumbo;
        }
    }
    if out.choque > CHOQUE_QUE_DANIA {
        e.salud -= (out.choque - CHOQUE_QUE_DANIA) * 6.0;
    }
    out
}

fn s_no_admite(f: &Ficha, s: Suelo) -> bool {
    s.tope(f) <= 0.0
}

// ---------------------------------------------------------------------------
// Semáforos
// ---------------------------------------------------------------------------

/// La luz que ve un conductor.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Luz {
    Verde,
    Amarillo,
    Rojo,
}

/// Duración de un ciclo completo, en segundos.
pub const CICLO_SEMAFORO: f64 = 34.0;

/// **La luz del eje por el que se circula**, según el reloj del mundo.
///
/// Un ciclo de 34 s: norte-sur verde 14 s, amarillo 2, todo rojo 1; este-oeste
/// verde 14 s, amarillo 2, todo rojo 1. Es una función del reloj y nada más: todos
/// los cruces del mundo van en fase, así que no hay que guardar un byte por
/// semáforo, el cliente y el servidor ven lo mismo, y dos jugadores en la misma
/// esquina ven la misma luz. `rumbo` es el octante de `roads::lane_at` (`0` este,
/// `2` norte, `4` oeste, `6` sur).
pub fn luz_en(t: f64, rumbo: u8) -> Luz {
    let ns = rumbo % 4 == 2;
    let f = t.rem_euclid(CICLO_SEMAFORO);
    let (inicio, _) = if ns { (0.0, 17.0) } else { (17.0, 34.0) };
    let d = f - inicio;
    if (0.0..14.0).contains(&d) {
        Luz::Verde
    } else if (14.0..16.0).contains(&d) {
        Luz::Amarillo
    } else {
        Luz::Rojo
    }
}

/// Metros que hacen falta para clavar el freno desde `vel` (con margen: frena al
/// 60 % de lo que da el vehículo, que es como frena alguien que no quiere
/// derrapar).
pub fn distancia_de_frenado(vel: f32, f: &Ficha) -> f32 {
    vel * vel / (2.0 * f.freno * 0.6)
}

// ---------------------------------------------------------------------------
// La patrulla
// ---------------------------------------------------------------------------

/// Memoria del que persigue: se atasca contra una pared y tiene que dar marcha
/// atrás sin que nadie se lo diga.
#[derive(Clone, Copy, Debug, Default)]
pub struct Persecucion {
    /// Segundos casi parado sin estar donde quería.
    pub atasco: f32,
    /// Segundos que le quedan de marcha atrás para desatascarse.
    pub reversa: f32,
    /// Hacia qué lado gira al desatascarse (±1).
    pub lado: f32,
}

/// **Perseguir un punto.** El perseguidor apunta el rumbo hacia el objetivo con el
/// mismo control que el jugador (`mando_desde_stick`), acelera a fondo lejos y se
/// acomoda a unos 6 m. Si lleva más de 1,2 s empujando una pared da marcha atrás
/// girando hacia el lado contrario.
pub fn perseguir(e: &Estado, f: &Ficha, p: &mut Persecucion, objetivo: (f32, f32), dt: f32) -> Mando {
    let (dx, dz) = (objetivo.0 - e.x, objetivo.1 - e.z);
    let d = (dx * dx + dz * dz).sqrt();
    if p.reversa > 0.0 {
        p.reversa -= dt;
        return Mando { acel: -1.0, giro: p.lado };
    }
    if d < 6.0 {
        // Llegó: frena a fondo y se queda.
        p.atasco = 0.0;
        return Mando { acel: if e.vel > 0.5 { -1.0 } else { 0.0 }, giro: 0.0 };
    }
    if e.vel.abs() < 0.8 && d > 8.0 {
        p.atasco += dt;
        if p.atasco > 1.2 {
            p.atasco = 0.0;
            p.reversa = 1.1;
            p.lado = if (e.x + e.z).floor() as i64 % 2 == 0 { 1.0 } else { -1.0 };
        }
    } else {
        p.atasco = 0.0;
    }
    let mag = if d > 30.0 { 1.0 } else { (d / 30.0).clamp(0.45, 1.0) };
    let _ = f;
    mando_desde_stick(e, dx / d * mag, dz / d * mag)
}

// ---------------------------------------------------------------------------
// Gasolineras
// ---------------------------------------------------------------------------

/// La clave del lote que genera la ciudad (`city::LOT_PROGRAMS`).
pub const LOTE_GASOLINERA: &str = "building/gasolinera";

/// ¿Esta pieza colocada (la clave de arte) es un surtidor? Cuenta lo que el
/// jugador planta con el editor (`decor/fuel_source`, `edif_gasolinera`) y las
/// piezas de estación de servicio del pack.
pub fn es_surtidor(art: &str) -> bool {
    ["gasolinera", "surtidor", "estacion_de_servicio", "fuel_source"].iter().any(|k| art.contains(k))
}

/// Rectángulo `[x0, z0, x1, z1]` (tiles) de la gasolinera generada que cubre la
/// celda `(tx, tz)`, si la hay. El lote entero cuenta como surtidor: el playón
/// donde se para el auto es parte de la parcela.
pub fn gasolinera_en(body: crate::celestial_bodies::CelestialBody, tx: i64, tz: i64) -> Option<[i64; 4]> {
    let s = crate::geography::settlement_covering(body, tx, tz)?;
    let lot = crate::city::lot_at(body, &s, tx, tz)?;
    (lot.program().key == LOTE_GASOLINERA).then(|| [lot.min.0, lot.min.1, lot.min.0 + lot.w, lot.min.1 + lot.h])
}

/// La gasolinera generada más cercana a `(x, z)` dentro de `radio` metros: su
/// centro y la distancia. Es la pista de «no tenés nafta, andá para allá».
///
/// Barre la zona de a `PASO` tiles preguntando por el lote (memoizado por el
/// asentamiento), no celda a celda: un lote de gasolinera mide 14 × 14 como
/// mínimo, así que un paso de 10 no puede saltearse ninguno. Cuesta unas mil
/// consultas con radio 200, y se llama cada varios segundos y sólo con el tanque
/// bajo.
pub fn gasolinera_cercana(
    body: crate::celestial_bodies::CelestialBody,
    x: f32,
    z: f32,
    radio: f32,
) -> Option<(f32, f32, f32)> {
    const PASO: i64 = 10;
    let r = radio as i64;
    let (cx, cz) = (x.floor() as i64, z.floor() as i64);
    let mut mejor: Option<(f32, f32, f32)> = None;
    let mut dz = -r;
    while dz <= r {
        let mut dx = -r;
        while dx <= r {
            if let Some([x0, z0, x1, z1]) = gasolinera_en(body, cx + dx, cz + dz) {
                let (gx, gz) = ((x0 + x1) as f32 * 0.5, (z0 + z1) as f32 * 0.5);
                let d = ((gx - x).powi(2) + (gz - z).powi(2)).sqrt();
                if d <= radio && mejor.is_none_or(|(_, _, m)| d < m) {
                    mejor = Some((gx, gz, d));
                }
            }
            dx += PASO;
        }
        dz += PASO;
    }
    mejor
}

// ---------------------------------------------------------------------------
// El conductor
// ---------------------------------------------------------------------------

/// Octante de `roads::lane_at` (`0` este, `2` norte, `4` oeste, `6` sur) → vector.
pub fn dir_octante(o: u8) -> (i64, i64) {
    match o % 8 {
        0 => (1, 0),
        2 => (0, -1),
        4 => (-1, 0),
        _ => (0, 1),
    }
}

fn opuesto(o: u8) -> u8 {
    (o + 4) % 8
}

/// La memoria de un vecino al volante.
#[derive(Clone, Copy, Debug)]
pub struct Conductor {
    /// La mano por la que va (octante cardinal).
    pub rumbo: u8,
    pub en_cruce: bool,
    /// Decidido al entrar al cruce: sigue derecho (si no, dobla con la mano).
    pub derecho: bool,
    pub semilla: u32,
    /// Velocidad a la que le gusta ir, fracción de la máxima (cada uno la suya).
    pub apuro: f32,
}

impl Conductor {
    pub fn nuevo(rumbo: u8, semilla: u32) -> Self {
        let apuro = 0.45 + (semilla % 100) as f32 / 100.0 * 0.25;
        Self { rumbo: rumbo % 8 & !1, en_cruce: false, derecho: true, semilla, apuro }
    }
}

/// ¿La calle sigue derecho más allá del cruce? Se mira hasta 20 m.
fn sigue_derecho(calzada: &impl Fn(i64, i64) -> bool, tx: i64, tz: i64, o: u8) -> bool {
    let (dx, dz) = dir_octante(o);
    (1..=20).any(|k| {
        let (x, z) = (tx + dx * k, tz + dz * k);
        calzada(x, z) && !crate::roads::cruce_con(calzada, x, z) && crate::roads::carril_con(calzada, x, z) == Some(o)
    })
}

/// **Un vecino maneja.** Devuelve el mando para [`paso`].
///
/// - `calzada(tx, tz)`: ¿es asfalto? (los tiles cargados del cliente).
/// - `libre_adelante`: metros hasta lo primero que tiene enfrente (otro auto, una
///   persona). `f32::INFINITY` si nada.
///
/// Fuera del asfalto (lo sacaste de la calle, lo chocaste) vuelve hacia la calzada
/// más cercana que vea; si no ve ninguna, se queda parado: un conductor de tránsito
/// no sale a pasear por el campo.
/// **Navegante: el piloto de una lancha del tránsito marítimo.** Avanza a ~60 % de la máxima por agua
/// libre y, si a 8 m o 14 m de proa no hay agua (costa, muelle), gira hacia el lado que despeja.
#[derive(Clone, Copy, Debug, Default)]
pub struct Navegante {
    /// Hacia dónde dobla cuando hay que esquivar (+1 / −1); se invierte si ambos lados están cerrados.
    pub lado: f32,
    pub semilla: u32,
}

/// El mando de un [`Navegante`]. `agua(tx, tz)` dice si la celda se navega.
pub fn navegar(e: &Estado, f: &Ficha, n: &mut Navegante, agua: &impl Fn(i64, i64) -> bool, libre_adelante: f32) -> Mando {
    let lado = if n.lado == 0.0 { 1.0 } else { n.lado };
    let abierto = |ang: f32, d: f32| {
        let (x, z) = (e.x + (e.rumbo + ang).cos() * d, e.z + (e.rumbo + ang).sin() * d);
        agua(x.floor() as i64, z.floor() as i64)
    };
    let cerca = !abierto(0.0, 8.0) || !abierto(0.0, 14.0) || libre_adelante < 10.0;
    if !cerca {
        let tope = 0.6 * f.vel_max;
        return Mando { acel: if e.vel < tope { 0.7 } else { 0.0 }, giro: 0.0 };
    }
    // Costa a la vista: se elige el lado más despejado y se baja la velocidad.
    let a = abierto(0.7 * lado, 10.0);
    let b = abierto(-0.7 * lado, 10.0);
    if !a && b {
        n.lado = -lado;
    } else if !a && !b {
        n.lado = -lado; // callejón sin salida: vuelta completa
    }
    Mando { acel: if e.vel > 0.3 * f.vel_max { -0.4 } else { 0.4 }, giro: n.lado.signum() }
}

pub fn conducir(
    e: &Estado,
    f: &Ficha,
    c: &mut Conductor,
    calzada: &impl Fn(i64, i64) -> bool,
    libre_adelante: f32,
    luz: &impl Fn(u8) -> Luz,
) -> Mando {
    let (tx, tz) = (e.x.floor() as i64, e.z.floor() as i64);
    if !calzada(tx, tz) {
        return Mando { acel: 0.0, giro: 0.0 };
    }
    let cruce = crate::roads::cruce_con(calzada, tx, tz);
    if cruce && !c.en_cruce {
        c.semilla = c.semilla.wrapping_mul(1_103_515_245).wrapping_add(12_345);
        c.derecho = (c.semilla >> 16) % 100 < 55 && sigue_derecho(calzada, tx, tz, c.rumbo);
    }
    c.en_cruce = cruce;

    // ¿Qué mano sigue? En el cruce, la decidida al entrar; fuera, la de la celda
    // (que es la que pinta la línea). Si la celda dice la mano CONTRARIA estamos del
    // lado equivocado: se mantiene el rumbo y el centrado lo devuelve a su mitad.
    if !(cruce && c.derecho) {
        if let Some(d) = crate::roads::carril_con(calzada, tx, tz) {
            if d != opuesto(c.rumbo) {
                c.rumbo = d;
            }
        }
    }
    let (fx, fz) = dir_octante(c.rumbo);
    // Derecha del rumbo: girar el vector 90° (este → sur).
    let (rx, rz) = (-fz, fx);

    // ¿Calle sin salida? Nada de asfalto en los próximos 3 m y no es cruce.
    let delante = (1..=3).any(|k| calzada(tx + fx * k, tz + fz * k));
    if !delante && !cruce {
        c.rumbo = opuesto(c.rumbo);
    }

    // Centrado en SU mitad: cuánta calzada hay a la derecha hasta el cordón, y a la
    // izquierda mientras siga siendo de su mano.
    let mut lateral = 0.0;
    if !cruce {
        let der = (1..=6).take_while(|&k| calzada(tx + rx * k, tz + rz * k)).count() as f32;
        let izq = (1..=6)
            .take_while(|&k| {
                let (x, z) = (tx - rx * k, tz - rz * k);
                calzada(x, z) && crate::roads::carril_con(calzada, x, z) == Some(c.rumbo)
            })
            .count() as f32;
        let centro_celda = (tx as f32 + 0.5) * rx as f32 + (tz as f32 + 0.5) * rz as f32;
        let yo = e.x * rx as f32 + e.z * rz as f32;
        lateral = centro_celda + (der - izq) * 0.5 - yo;
    }
    let (fx, fz) = dir_octante(c.rumbo);
    let (rx, rz) = (-fz, fx);
    let ax = e.x + fx as f32 * 4.0 + rx as f32 * lateral;
    let az = e.z + fz as f32 * 4.0 + rz as f32 * lateral;
    let deseado = (az - e.z).atan2(ax - e.x);
    let diff = envolver(deseado - e.rumbo);
    let giro = (diff * 2.5).clamp(-1.0, 1.0);

    // **El semáforo.** Antes de cada cruce se mira hasta 30 m: si la luz de SU eje
    // no es verde y todavía puede clavar el freno, el borde del cruce cuenta como
    // un obstáculo. Si ya no llega a frenar (amarillo tardío) sigue de largo: parar
    // en medio del cruce es peor que cruzar. Una vez adentro no se frena nunca.
    let mut libre_adelante = libre_adelante;
    if !cruce {
        let (fx, fz) = dir_octante(c.rumbo);
        if let Some(k) = (1..=30).find(|&k| crate::roads::cruce_con(calzada, tx + fx * k, tz + fz * k)) {
            // Coordenada, a lo largo del rumbo, del borde cercano del cruce.
            let (a, borde) = if fx != 0 {
                let celda = tx + fx * k;
                (e.x, if fx > 0 { celda as f32 } else { (celda + 1) as f32 })
            } else {
                let celda = tz + fz * k;
                (e.z, if fz > 0 { celda as f32 } else { (celda + 1) as f32 })
            };
            let al_borde = if fx + fz > 0 { borde - a } else { a - borde };
            let paragolpes = al_borde - e.largo * 0.5;
            if luz(c.rumbo) != Luz::Verde && paragolpes >= distancia_de_frenado(e.vel.max(0.0), f) {
                libre_adelante = libre_adelante.min(al_borde);
            }
        }
    }

    // Velocidad: la suya en recta, despacio doblando, y frena ante lo que tenga
    // enfrente (mantiene distancia en vez de comérselo).
    let mut objetivo = f.vel_max * c.apuro;
    if diff.abs() > 0.45 || cruce {
        objetivo = objetivo.min(5.0);
    }
    let distancia = libre_adelante - e.largo * 0.5;
    if distancia < 3.0 {
        objetivo = 0.0;
    } else if distancia < 45.0 {
        // La velocidad desde la que todavía se frena en lo que queda (a la mitad del
        // freno del vehículo, que es como frena quien no quiere derrapar).
        objetivo = objetivo.min(((distancia - 3.0) * f.freno).sqrt());
    }
    let acel = ((objetivo - e.vel) * 0.6).clamp(-1.0, 1.0);
    Mando { acel, giro }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn auto() -> (Ficha, Estado) {
        let f = ficha(Clase::Auto);
        (f, Estado::nuevo(0.5, 0.5, 0.0, &f, 4.1, 2.3))
    }

    fn manejar(e: &mut Estado, f: &Ficha, m: Mando, s: Suelo, segs: f32) {
        for _ in 0..(segs * 20.0) as usize {
            paso(e, m, f, 0.05, &|_, _| s, &|_, _, _| false);
        }
    }

    #[test]
    fn en_asfalto_rinde_mas_que_en_el_pasto_y_en_la_arena() {
        let (f, mut a) = auto();
        let (_, mut p) = auto();
        let (_, mut r) = auto();
        let m = Mando { acel: 1.0, giro: 0.0 };
        manejar(&mut a, &f, m, Suelo::Asfalto, 8.0);
        manejar(&mut p, &f, m, Suelo::Pasto, 8.0);
        manejar(&mut r, &f, m, Suelo::Arena, 8.0);
        assert!(a.x > p.x * 1.3, "asfalto {} vs pasto {}", a.x, p.x);
        assert!(p.x > r.x, "pasto {} vs arena {}", p.x, r.x);
        // Pero se maneja: fuera de la calle se ANDA, como en San Andreas.
        assert!(r.x > 20.0, "en la arena apenas avanzó {}", r.x);
    }

    #[test]
    fn la_moto_rinde_mas_que_el_auto_en_el_campo() {
        let (fa, mut a) = auto();
        let fm = ficha(Clase::Moto);
        let mut m = Estado::nuevo(0.5, 0.5, 0.0, &fm, 2.0, 0.8);
        let fondo = Mando { acel: 1.0, giro: 0.0 };
        manejar(&mut a, &fa, fondo, Suelo::Pasto, 8.0);
        manejar(&mut m, &fm, fondo, Suelo::Pasto, 8.0);
        assert!(m.x > a.x);
    }

    #[test]
    fn la_lancha_navegante_no_toca_la_costa_en_un_minuto() {
        // Mar entre x ∈ [0, 120) y z ∈ [-60, 60); tierra afuera.
        let mar = |tx: i64, tz: i64| (0..120).contains(&tx) && (-60..60).contains(&tz);
        let suelo = |tx: i64, tz: i64| if mar(tx, tz) { Suelo::Hondo } else { Suelo::Asfalto };
        let f = ficha(Clase::Lancha);
        let mut e = Estado::nuevo(60.5, 0.5, 0.0, &f, 3.3, 1.8);
        let mut n = Navegante::default();
        for _ in 0..1200 {
            let m = navegar(&e, &f, &mut n, &mar, f32::INFINITY);
            paso(&mut e, m, &f, 0.05, &suelo, &|_, _, _| false);
            assert!(mar(e.x.floor() as i64, e.z.floor() as i64), "la lancha salió del agua en ({:.1}, {:.1})", e.x, e.z);
        }
        assert!(e.vel > 1.0, "la lancha quedó parada: {}", e.vel);
    }

    #[test]
    fn ningun_auto_entra_al_agua_honda_y_la_lancha_no_sube_a_tierra() {
        // Agua honda desde x >= 10.
        let mar = |tx: i64, _: i64| if tx >= 10 { Suelo::Hondo } else { Suelo::Asfalto };
        let (f, mut e) = auto();
        for _ in 0..200 {
            paso(&mut e, Mando { acel: 1.0, giro: 0.0 }, &f, 0.05, &mar, &|_, _, _| false);
        }
        assert!(e.x + e.largo * 0.5 < 10.5, "el auto se metió al mar: x={}", e.x);

        let fl = ficha(Clase::Lancha);
        let mut l = Estado::nuevo(20.5, 0.5, std::f32::consts::PI, &fl, 3.3, 1.8);
        for _ in 0..200 {
            paso(&mut l, Mando { acel: 1.0, giro: 0.0 }, &fl, 0.05, &mar, &|_, _, _| false);
        }
        assert!(l.x - l.largo * 0.5 > 9.5, "la lancha subió a la calle: x={}", l.x);
        assert!(l.x < 19.0, "la lancha no navegó: x={}", l.x);
    }

    #[test]
    fn un_choque_frena_y_abolla() {
        let (f, mut e) = auto();
        let pared = |x: f32, _: f32, r: f32| x + r > 40.0;
        let mut golpe = 0.0f32;
        for _ in 0..200 {
            let p = paso(&mut e, Mando { acel: 1.0, giro: 0.0 }, &f, 0.05, &|_, _| Suelo::Asfalto, &pared);
            golpe = golpe.max(p.choque);
        }
        assert!(e.x < 40.0);
        assert!(golpe > CHOQUE_QUE_DANIA, "no registró el choque: {golpe}");
        assert!(e.salud < f.salud, "chocó a {golpe} m/s y no se abolló");
    }

    #[test]
    fn la_chatarra_no_arranca() {
        let (f, mut e) = auto();
        e.salud = 0.0;
        manejar(&mut e, &f, Mando { acel: 1.0, giro: 0.0 }, Suelo::Asfalto, 3.0);
        assert!(e.x < 0.6);
    }

    #[test]
    fn el_stick_lleva_el_rumbo_hacia_donde_apunta() {
        let (f, mut e) = auto();
        // Mirando al este, stick al norte (−z).
        for _ in 0..80 {
            let m = mando_desde_stick(&e, 0.0, -1.0);
            paso(&mut e, m, &f, 0.05, &|_, _| Suelo::Asfalto, &|_, _, _| false);
        }
        let norte = -std::f32::consts::FRAC_PI_2;
        assert!(envolver(e.rumbo - norte).abs() < 0.2, "rumbo {}", e.rumbo);
        assert!(e.z < -5.0, "no fue al norte: z={}", e.z);
    }

    #[test]
    fn apuntar_atras_frena_y_despues_da_marcha_atras() {
        let (f, mut e) = auto();
        manejar(&mut e, &f, Mando { acel: 1.0, giro: 0.0 }, Suelo::Asfalto, 2.0);
        assert!(e.vel > 5.0);
        let x0 = e.x;
        for _ in 0..120 {
            let m = mando_desde_stick(&e, -1.0, 0.0);
            paso(&mut e, m, &f, 0.05, &|_, _| Suelo::Asfalto, &|_, _, _| false);
        }
        assert!(e.vel < 0.0, "no dio marcha atrás: vel={}", e.vel);
        assert!(e.x < x0 + 30.0);
        // Y sigue mirando al este: la marcha atrás no da la vuelta al auto.
        assert!(e.rumbo.abs() < 0.3, "rumbo {}", e.rumbo);
    }

    /// Una retícula de calles de 5 de ancho cada 40 tiles, con su cordón.
    fn reticula(tx: i64, tz: i64) -> bool {
        let en = |v: i64| (v.rem_euclid(40) as i64 - 20).abs() <= 2;
        en(tx) || en(tz)
    }

    /// Devuelve (metros recorridos, pasos fuera del asfalto, % a contramano,
    /// entradas a un cruce con rojo de más de 1,5 s, cruces atravesados).
    fn manejar_por_la_ciudad(x: f32, z: f32, o: u8, semilla: u32, segs: f32) -> (f32, usize, usize, usize, usize) {
        let f = ficha(Clase::Auto);
        let rumbo = -(o as f32) * std::f32::consts::FRAC_PI_4;
        let mut e = Estado::nuevo(x, z, rumbo, &f, 4.1, 2.3);
        let mut c = Conductor::nuevo(o, semilla);
        let calzada = |x: i64, z: i64| reticula(x, z);
        let suelo = |x: i64, z: i64| if reticula(x, z) { Suelo::Asfalto } else { Suelo::Pasto };
        let mut recorrido = 0.0;
        let mut fuera = 0;
        let mut contramano = 0;
        let mut en_rojo = 0;
        let mut cruces = 0;
        let mut estaba_en_cruce = false;
        let pasos = (segs * 20.0) as usize;
        // Cada semilla arranca en otro punto del ciclo.
        let t0 = (semilla % 34) as f64;
        for i in 0..pasos {
            let t = t0 + i as f64 * 0.05;
            let luz = |r: u8| luz_en(t, r);
            let rumbo_antes = c.rumbo;
            let m = conducir(&e, &f, &mut c, &calzada, f32::INFINITY, &luz);
            let (x0, z0) = (e.x, e.z);
            paso(&mut e, m, &f, 0.05, &suelo, &|_, _, _| false);
            recorrido += ((e.x - x0).powi(2) + (e.z - z0).powi(2)).sqrt();
            let (tx, tz) = (e.x.floor() as i64, e.z.floor() as i64);
            let en_cruce = crate::roads::cruce_con(&calzada, tx, tz);
            if en_cruce && !estaba_en_cruce {
                cruces += 1;
                if luz_en(t, rumbo_antes) == Luz::Rojo && luz_en(t - 1.5, rumbo_antes) == Luz::Rojo {
                    en_rojo += 1;
                }
            }
            estaba_en_cruce = en_cruce;
            if !reticula(tx, tz) {
                fuera += 1;
            } else if !en_cruce && crate::roads::carril_con(&calzada, tx, tz) == Some(opuesto(c.rumbo)) {
                contramano += 1;
            }
        }
        (recorrido, fuera, contramano * 100 / pasos.max(1), en_rojo, cruces)
    }

    #[test]
    fn el_vecino_maneja_por_su_mano_sin_salirse_de_la_calle() {
        // La mitad sur de la calle este-oeste (z 20..22) va al este.
        let (recorrido, fuera, contramano, en_rojo, _) = manejar_por_la_ciudad(5.5, 21.5, 0, 7, 90.0);
        assert_eq!(fuera, 0, "se salió del asfalto {fuera} pasos");
        assert_eq!(en_rojo, 0, "cruzó {en_rojo} veces con el semáforo en rojo");
        assert!(recorrido > 250.0, "apenas recorrió {recorrido} m en 90 s");
        assert!(contramano <= 5, "{contramano}% del tiempo a contramano");
    }

    /// Checklist «lo que se nos escapó»: prueba multi-semilla, no una corrida feliz.
    #[test]
    fn el_vecino_maneja_bien_con_cualquier_semilla_y_desde_cualquier_mano() {
        let salidas = [(5.5, 21.5, 0u8), (35.5, 18.5, 4), (21.5, 35.5, 2), (18.5, 5.5, 6)];
        for semilla in [1u32, 2, 3, 99, 1234, 777_777] {
            for &(x, z, o) in &salidas {
                let (recorrido, fuera, contramano, en_rojo, _) = manejar_por_la_ciudad(x, z, o, semilla, 90.0);
                assert_eq!(fuera, 0, "semilla {semilla} desde ({x},{z}): se salió {fuera} pasos");
                assert_eq!(en_rojo, 0, "semilla {semilla} desde ({x},{z}): {en_rojo} cruces con rojo");
                assert!(recorrido > 120.0, "semilla {semilla} desde ({x},{z}): recorrió {recorrido}");
                assert!(contramano <= 8, "semilla {semilla} desde ({x},{z}): {contramano}% a contramano");
            }
        }
    }

    #[test]
    fn frena_ante_lo_que_tiene_enfrente() {
        let f = ficha(Clase::Auto);
        let mut e = Estado::nuevo(5.5, 21.5, 0.0, &f, 4.1, 2.3);
        e.vel = 10.0;
        let mut c = Conductor::nuevo(0, 3);
        let m = conducir(&e, &f, &mut c, &|x, z| reticula(x, z), 4.0, &|_| Luz::Verde);
        assert!(m.acel < 0.0, "con algo a 4 m acelera: {m:?}");
    }

    #[test]
    fn el_transito_es_casi_todo_autos_y_todo_modelo_existe() {
        let mut autos = 0;
        for h in 0..5000u32 {
            let k = modelo_de_transito(h.wrapping_mul(2_654_435_761));
            let c = clase_de(k).unwrap_or_else(|| panic!("{k} no está en MODELOS"));
            if matches!(c, Clase::Auto | Clase::Taxi) {
                autos += 1;
            }
            assert_ne!(c, Clase::Lancha, "una lancha en la calle");
        }
        assert!(autos > 3500, "{autos} autos de 5000");
    }

    #[test]
    fn los_nombres_de_la_consola_llevan_a_un_modelo_que_existe() {
        for n in ["auto", "taxi", "colectivo", "camion", "basura", "bomberos", "ambulancia", "casa", "lancha", "moto", "bici", "patrulla", "coche_7", "colectivo_3"] {
            let k = modelo_por_nombre(n).unwrap_or_else(|| panic!("{n} no se entiende"));
            assert!(clase_de(k).is_some(), "{n} → {k} no está en MODELOS");
        }
        assert_eq!(modelo_por_nombre("avioneta"), None);
    }

    #[test]
    fn los_surtidores_se_reconocen_por_la_clave_de_arte() {
        assert!(es_surtidor("building/gasolinera"));
        assert!(es_surtidor("decor/fuel_source"));
        assert!(es_surtidor("building/estacion_de_servicio_2"));
        assert!(!es_surtidor("building/hospital"));
    }

    #[test]
    fn el_suelo_sale_del_tile() {
        assert_eq!(suelo_de(Tile::Asphalt), Suelo::Asfalto);
        assert_eq!(suelo_de(Tile::Grass), Suelo::Pasto);
        assert_eq!(suelo_de(Tile::WaterDeep), Suelo::Hondo);
        assert_eq!(suelo_de(Tile::Pavement), Suelo::Firme);
    }

    #[test]
    fn el_ciclo_del_semaforo_nunca_da_verde_a_los_dos_ejes() {
        let mut ns_verde = 0.0f64;
        let mut ew_verde = 0.0f64;
        let mut t = 0.0;
        while t < CICLO_SEMAFORO * 3.0 {
            let (ns, ew) = (luz_en(t, 2), luz_en(t, 0));
            assert!(!(ns != Luz::Rojo && ew != Luz::Rojo), "t={t}: ns {ns:?} y ew {ew:?} a la vez");
            assert_eq!(luz_en(t, 2), luz_en(t, 6), "norte y sur van juntos");
            assert_eq!(luz_en(t, 0), luz_en(t, 4), "este y oeste van juntos");
            if ns == Luz::Verde {
                ns_verde += 0.05;
            }
            if ew == Luz::Verde {
                ew_verde += 0.05;
            }
            t += 0.05;
        }
        assert!((ns_verde - ew_verde).abs() < 1.0, "reparto parejo: {ns_verde} vs {ew_verde}");
        assert!(ns_verde > 3.0 * 13.0, "el verde dura: {ns_verde}");
    }

    #[test]
    fn con_rojo_se_para_antes_del_cruce_y_con_verde_pasa() {
        let calzada = |x: i64, z: i64| reticula(x, z);
        let suelo = |x: i64, z: i64| if reticula(x, z) { Suelo::Asfalto } else { Suelo::Pasto };
        let f = ficha(Clase::Auto);
        // Calle este-oeste (z 20..22), mitad sur, yendo al este; el cruce es x 18..22.
        for (rojo, nombre) in [(true, "rojo"), (false, "verde")] {
            let mut e = Estado::nuevo(2.5, 21.5, 0.0, &f, 4.1, 2.3);
            let mut c = Conductor::nuevo(0, 5);
            let luz = |_: u8| if rojo { Luz::Rojo } else { Luz::Verde };
            let mut max_x = 0.0f32;
            for _ in 0..(40.0 * 20.0) as usize {
                let m = conducir(&e, &f, &mut c, &calzada, f32::INFINITY, &luz);
                paso(&mut e, m, &f, 0.05, &suelo, &|_, _, _| false);
                max_x = max_x.max(e.x);
            }
            if rojo {
                assert!(max_x + e.largo * 0.5 < 18.0, "{nombre}: el paragolpes llegó a {} (cruce desde x=18)", max_x + e.largo * 0.5);
                assert!(max_x > 8.0, "{nombre}: ni se acercó al cruce ({max_x})");
            } else {
                assert!(max_x > 30.0, "{nombre}: no cruzó ({max_x})");
            }
        }
    }

    #[test]
    fn el_amarillo_tardio_no_clava_el_freno_en_medio_del_cruce() {
        let calzada = |x: i64, z: i64| reticula(x, z);
        let f = ficha(Clase::Auto);
        let mut e = Estado::nuevo(15.5, 21.5, 0.0, &f, 4.1, 2.3);
        e.vel = 8.0; // a 8 m/s el cruce queda a ~0,5 m del paragolpes: ya no llega a frenar
        let mut c = Conductor::nuevo(0, 5);
        let m = conducir(&e, &f, &mut c, &calzada, f32::INFINITY, &|_| Luz::Amarillo);
        assert!(m.acel > -0.2, "frena a fondo pegado al cruce: {m:?}");
    }

    #[test]
    fn sin_combustible_el_motor_no_responde_y_la_bici_nunca_se_queda() {
        let (f, mut e) = auto();
        manejar(&mut e, &f, Mando { acel: 1.0, giro: 0.0 }, Suelo::Asfalto, 2.0);
        assert!(e.comb < 1.0, "no gastó nada");
        e.comb = 0.0;
        let v0 = e.vel;
        manejar(&mut e, &f, Mando { acel: 1.0, giro: 0.0 }, Suelo::Asfalto, 6.0);
        assert!(e.vel < v0 * 0.5, "sin nafta siguió acelerando: {} → {}", v0, e.vel);

        let fb = ficha(Clase::Bici);
        let mut b = Estado::nuevo(0.5, 0.5, 0.0, &fb, 1.6, 0.6);
        b.comb = 0.0;
        manejar(&mut b, &fb, Mando { acel: 1.0, giro: 0.0 }, Suelo::Asfalto, 3.0);
        assert!(b.vel > 3.0, "la bici no pedalea: {}", b.vel);
    }

    #[test]
    fn el_tanque_rinde_lo_que_dice_la_ficha() {
        let (f, mut e) = auto();
        let mut recorrido = 0.0;
        while e.comb > 0.0 && recorrido < 30_000.0 {
            let x0 = e.x;
            paso(&mut e, Mando { acel: 0.6, giro: 0.0 }, &f, 0.05, &|_, _| Suelo::Asfalto, &|_, _, _| false);
            recorrido += e.x - x0;
            if e.x > 5_000.0 {
                e.x = 0.5;
            }
        }
        assert!(e.comb <= 0.0, "ni gastó el tanque en 30 km");
        assert!(recorrido > f.alcance * 0.5 && recorrido < f.alcance * 1.1, "rindió {recorrido} m de {}", f.alcance);
    }

    #[test]
    fn la_patrulla_alcanza_al_que_huye_y_se_desatasca() {
        let f = ficha(Clase::Patrulla);
        let mut e = Estado::nuevo(0.5, 0.5, 0.0, &f, 4.1, 2.3);
        let mut p = Persecucion::default();
        let objetivo = (120.0, 40.0);
        for _ in 0..(30.0 * 20.0) as usize {
            let m = perseguir(&e, &f, &mut p, objetivo, 0.05);
            paso(&mut e, m, &f, 0.05, &|_, _| Suelo::Pasto, &|_, _, _| false);
        }
        let d = ((e.x - objetivo.0).powi(2) + (e.z - objetivo.1).powi(2)).sqrt();
        assert!(d < 12.0, "la patrulla quedó a {d} m");

        // Una pared entre medio: da marcha atrás y la rodea en vez de quedarse clavada.
        let pared = |x: f32, z: f32, r: f32| (x + r > 50.0 && x - r < 52.0) && z > -30.0 && z < 30.0;
        let mut e = Estado::nuevo(0.5, 0.5, 0.0, &f, 4.1, 2.3);
        let mut p = Persecucion::default();
        let objetivo = (120.0, 0.5);
        let mut paso_la_pared = false;
        for _ in 0..(60.0 * 20.0) as usize {
            let m = perseguir(&e, &f, &mut p, objetivo, 0.05);
            paso(&mut e, m, &f, 0.05, &|_, _| Suelo::Pasto, &pared);
            paso_la_pared |= e.x > 55.0;
        }
        assert!(paso_la_pared, "quedó clavada contra la pared en x={}", e.x);
    }

    #[test]
    fn hay_gasolineras_en_las_ciudades_y_la_pista_apunta_a_una() {
        use crate::celestial_bodies::CelestialBody;
        let body = CelestialBody::Earth;
        let mut halladas = 0;
        'b: for sx in -3..=3i64 {
            for sz in -3..=3i64 {
                let Some(s) = crate::geography::settlement_in(body, sx, sz) else { continue };
                if let Some((gx, gz, d)) = gasolinera_cercana(body, s.center.0 as f32, s.center.1 as f32, 300.0) {
                    let r = gasolinera_en(body, gx.floor() as i64, gz.floor() as i64);
                    // El centro del lote es parte del lote.
                    assert!(r.is_some(), "la pista ({gx},{gz}) no cae en una gasolinera");
                    assert!(d <= 300.0);
                    halladas += 1;
                    if halladas >= 3 {
                        break 'b;
                    }
                }
            }
        }
        assert!(halladas >= 1, "ninguna ciudad cercana tiene gasolinera");
    }
}
