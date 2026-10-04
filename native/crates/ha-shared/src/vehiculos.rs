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
    };
    match c {
        Clase::Auto => f("Auto", 20.0, 6.0, 12.0, 2.4, 100.0),
        Clase::Taxi => f("Taxi", 20.0, 6.0, 12.0, 2.4, 100.0),
        Clase::Colectivo => f("Colectivo", 13.0, 3.0, 8.0, 1.5, 220.0),
        Clase::Camion => f("Camión", 14.0, 3.0, 8.0, 1.5, 250.0),
        Clase::Basura => f("Camión de basura", 12.0, 2.5, 8.0, 1.4, 260.0),
        Clase::Bomberos => f("Autobomba", 16.0, 3.5, 9.0, 1.5, 300.0),
        Clase::Ambulancia => f("Ambulancia", 19.0, 5.0, 11.0, 2.0, 150.0),
        Clase::CasaRodante => f("Casa rodante", 13.0, 2.8, 8.0, 1.4, 200.0),
        Clase::Moto => Ficha { dos_ruedas: true, ..f("Moto", 24.0, 9.0, 14.0, 3.2, 50.0) },
        Clase::Bici => Ficha { dos_ruedas: true, motor: false, ..f("Bicicleta", 8.0, 4.0, 8.0, 3.4, 30.0) },
        Clase::Lancha => Ficha { medio: Medio::Agua, ..f("Lancha", 16.0, 4.0, 4.0, 1.8, 120.0) },
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
}

impl Estado {
    pub fn nuevo(x: f32, z: f32, rumbo: f32, f: &Ficha, largo: f32, ancho: f32) -> Self {
        Self { x, z, rumbo, vel: 0.0, salud: f.salud, largo, ancho }
    }

    pub fn chatarra(&self) -> bool {
        self.salud <= 0.0
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
    let (acel, giro) = if e.chatarra() { (0.0, 0.0) } else { (m.acel.clamp(-1.0, 1.0), m.giro.clamp(-1.0, 1.0)) };
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
pub fn conducir(
    e: &Estado,
    f: &Ficha,
    c: &mut Conductor,
    calzada: &impl Fn(i64, i64) -> bool,
    libre_adelante: f32,
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

    // Velocidad: la suya en recta, despacio doblando, y frena ante lo que tenga
    // enfrente (mantiene distancia en vez de comérselo).
    let mut objetivo = f.vel_max * c.apuro;
    if diff.abs() > 0.45 || cruce {
        objetivo = objetivo.min(5.0);
    }
    let distancia = libre_adelante - e.largo * 0.5;
    if distancia < 3.0 {
        objetivo = 0.0;
    } else if distancia < 12.0 {
        objetivo = objetivo.min((distancia - 3.0) * 1.2);
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

    fn manejar_por_la_ciudad(x: f32, z: f32, o: u8, semilla: u32, segs: f32) -> (f32, usize, usize) {
        let f = ficha(Clase::Auto);
        let rumbo = -(o as f32) * std::f32::consts::FRAC_PI_4;
        let mut e = Estado::nuevo(x, z, rumbo, &f, 4.1, 2.3);
        let mut c = Conductor::nuevo(o, semilla);
        let calzada = |x: i64, z: i64| reticula(x, z);
        let suelo = |x: i64, z: i64| if reticula(x, z) { Suelo::Asfalto } else { Suelo::Pasto };
        let mut recorrido = 0.0;
        let mut fuera = 0;
        let mut contramano = 0;
        let pasos = (segs * 20.0) as usize;
        for _ in 0..pasos {
            let m = conducir(&e, &f, &mut c, &calzada, f32::INFINITY);
            let (x0, z0) = (e.x, e.z);
            paso(&mut e, m, &f, 0.05, &suelo, &|_, _, _| false);
            recorrido += ((e.x - x0).powi(2) + (e.z - z0).powi(2)).sqrt();
            let (tx, tz) = (e.x.floor() as i64, e.z.floor() as i64);
            if !reticula(tx, tz) {
                fuera += 1;
            } else if !crate::roads::cruce_con(&calzada, tx, tz)
                && crate::roads::carril_con(&calzada, tx, tz) == Some(opuesto(c.rumbo))
            {
                contramano += 1;
            }
        }
        (recorrido, fuera, contramano * 100 / pasos.max(1))
    }

    #[test]
    fn el_vecino_maneja_por_su_mano_sin_salirse_de_la_calle() {
        // La mitad sur de la calle este-oeste (z 20..22) va al este.
        let (recorrido, fuera, contramano) = manejar_por_la_ciudad(5.5, 21.5, 0, 7, 90.0);
        assert_eq!(fuera, 0, "se salió del asfalto {fuera} pasos");
        assert!(recorrido > 400.0, "apenas recorrió {recorrido} m en 90 s");
        assert!(contramano <= 5, "{contramano}% del tiempo a contramano");
    }

    /// Checklist «lo que se nos escapó»: prueba multi-semilla, no una corrida feliz.
    #[test]
    fn el_vecino_maneja_bien_con_cualquier_semilla_y_desde_cualquier_mano() {
        let salidas = [(5.5, 21.5, 0u8), (35.5, 18.5, 4), (21.5, 35.5, 2), (18.5, 5.5, 6)];
        for semilla in [1u32, 2, 3, 99, 1234, 777_777] {
            for &(x, z, o) in &salidas {
                let (recorrido, fuera, contramano) = manejar_por_la_ciudad(x, z, o, semilla, 60.0);
                assert_eq!(fuera, 0, "semilla {semilla} desde ({x},{z}): se salió {fuera} pasos");
                assert!(recorrido > 200.0, "semilla {semilla} desde ({x},{z}): recorrió {recorrido}");
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
        let m = conducir(&e, &f, &mut c, &|x, z| reticula(x, z), 4.0);
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
    fn el_suelo_sale_del_tile() {
        assert_eq!(suelo_de(Tile::Asphalt), Suelo::Asfalto);
        assert_eq!(suelo_de(Tile::Grass), Suelo::Pasto);
        assert_eq!(suelo_de(Tile::WaterDeep), Suelo::Hondo);
        assert_eq!(suelo_de(Tile::Pavement), Suelo::Firme);
    }
}
