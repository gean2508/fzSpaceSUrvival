//! **FZ STUDIOS — la identidad visual de la interfaz, en un solo sitio.**
//!
//! El 15 ago 2026 el usuario declaró que toda la interfaz —de la splash al
//! cierre— se rehace: *"vamos a borrar el diseño, no las funcionalidades"*. Este
//! módulo es el esqueleto compartido de esa reescritura, para que la splash, el
//! menú, el creador de personaje y los paneles del juego **no vuelvan a
//! inventar cada uno su color y su tipografía**.
//!
//! Trae tres cosas y nada más:
//!
//! 1. [`Paleta`] — morado + turquesa fresco, con naranja neón y fucsia de
//!    acento. Es la marca, no el juego: el mundo sigue con la paleta del pack.
//! 2. [`cartel`] — el rótulo de un panel. **Un título nunca va como texto
//!    pelado**, va en su cartel; fue decisión explícita sobre los mockups.
//! 3. [`ParallaxPlugin`] — el fondo animado en bucle de las pantallas de menú:
//!    tres capas que derivan a distinta velocidad más motas flotando. Cada
//!    pantalla pide su [`Ambiente`], así que el menú y el creador no se ven
//!    iguales aunque compartan el sistema.
//!
//! **Por qué el fondo se dibuja por código y no con PNG:** el fondo viejo era
//! una secuencia de imágenes (`art/ui/menu_bg/*.png`) reproducida como un video.
//! Eso son N texturas de pantalla completa residentes para algo que se mira diez
//! segundos, en un teléfono que ya vive al borde de la RAM (ver
//! `auditoria_celu.md`). Tres nodos de color que se mueven cuestan cero
//! textura y se ven mejor que un GIF de 8 fps.

use bevy::prelude::*;

// ---------------------------------------------------------------------------
// 1. La paleta
// ---------------------------------------------------------------------------

/// Los colores de FZ STUDIOS. Constantes y no literales sueltos: el día que la
/// marca cambie de tono, cambia acá y no en catorce pantallas.
pub struct Paleta;

impl Paleta {
    /// El morado de la marca. Es el color dominante.
    pub const MORADO: Color = Color::srgb(0.478, 0.247, 0.831);
    /// Morado profundo, para fondos y sombras de cartel.
    pub const MORADO_HONDO: Color = Color::srgb(0.153, 0.078, 0.290);
    /// El turquesa fresco. El segundo de la pareja y el que lee mejor sobre morado.
    pub const TURQUESA: Color = Color::srgb(0.184, 0.851, 0.773);
    /// Naranja neón — acento caliente, para lo que llama a la acción.
    pub const NARANJA: Color = Color::srgb(1.000, 0.478, 0.161);
    /// Fucsia — acento de peligro y de selección.
    pub const FUCSIA: Color = Color::srgb(1.000, 0.239, 0.651);
    /// La tinta: el fondo más oscuro, con sesgo morado para que nada se vea gris.
    pub const TINTA: Color = Color::srgb(0.055, 0.043, 0.094);
    /// Texto principal sobre tinta.
    pub const TEXTO: Color = Color::srgb(0.937, 0.945, 0.980);
    /// Texto secundario.
    pub const TEXTO_SUAVE: Color = Color::srgb(0.639, 0.663, 0.769);
}

// ---------------------------------------------------------------------------
// 2. El cartel
// ---------------------------------------------------------------------------

/// **El rótulo de un panel.** Nunca un título como texto pelado.
///
/// Es una barra de morado profundo con el filo turquesa arriba y el texto en
/// versalitas espaciadas. Se devuelve para que quien lo llame lo meta donde
/// quiera; no asume que sea el primer hijo de nada.
pub fn cartel(padre: &mut ChildBuilder, texto: &str, s: f32) {
    padre
        .spawn((
            Node {
                padding: UiRect::axes(Val::Px(18.0 * s), Val::Px(7.0 * s)),
                flex_direction: FlexDirection::Column,
                align_items: AlignItems::Center,
                border: UiRect::top(Val::Px(2.0 * s)),
                ..default()
            },
            BorderColor(Paleta::TURQUESA),
            BackgroundColor(Paleta::MORADO_HONDO),
            BorderRadius::all(Val::Px(4.0 * s)),
            Name::new("Cartel"),
        ))
        .with_children(|c| {
            c.spawn((
                Text::new(texto.to_uppercase()),
                TextFont { font_size: (17.0 * s).clamp(12.0, 22.0), ..default() },
                TextColor(Paleta::TEXTO),
            ));
        });
}

// ---------------------------------------------------------------------------
// 3. El fondo parallax
// ---------------------------------------------------------------------------

/// **De qué ambiente es esta pantalla.** El sistema es el mismo; lo que cambia
/// son los tres colores y hacia dónde deriva cada capa, y con eso el menú y el
/// creador de personaje se sienten dos sitios distintos.
#[derive(Component, Clone, Copy, PartialEq, Eq, Debug)]
pub enum Ambiente {
    /// Menú principal: morado hondo, capas lentas, motas turquesa.
    Menu,
    /// **Creador de personaje: un bosque de noche, con faroles y luciérnagas.**
    /// Es el fondo propio que pedía el mockup del 15 ago 2026 — el creador no se
    /// puede ver igual que el menú, porque es otro sitio. Antes era el mismo
    /// parallax en tonos vino, que se leía como "el menú pero rojo".
    Creador,
}

impl Ambiente {
    /// `[capa lejana, capa media, capa cercana]`.
    fn capas(self) -> [Color; 3] {
        match self {
            Ambiente::Menu => [
                Color::srgb(0.086, 0.055, 0.157),
                Color::srgb(0.145, 0.086, 0.263),
                Color::srgb(0.212, 0.114, 0.365),
            ],
            // Bosque de noche: tres masas de follaje, de la más lejana y azulada
            // a la más cercana y verde. El morado de la marca sigue abajo de
            // todo, así que no se despega de la identidad.
            Ambiente::Creador => [
                Color::srgb(0.063, 0.078, 0.129),
                Color::srgb(0.075, 0.137, 0.129),
                Color::srgb(0.098, 0.192, 0.145),
            ],
        }
    }

    fn mota(self) -> Color {
        match self {
            Ambiente::Menu => Paleta::TURQUESA,
            // Luciérnagas: amarillo verdoso cálido, no el naranja de la marca.
            Ambiente::Creador => Color::srgb(0.804, 1.0, 0.451),
        }
    }

    /// Cuántos faroles cuelga el ambiente. Sólo el bosque tiene: en el menú no
    /// hay nada de qué colgarlos.
    fn faroles(self) -> usize {
        match self {
            Ambiente::Menu => 0,
            Ambiente::Creador => 3,
        }
    }
}

/// Una capa del parallax: su velocidad y su fase.
#[derive(Component)]
struct CapaParallax {
    /// Fracción de ancho por segundo. Negativa = deriva a la izquierda.
    velocidad: f32,
    /// Amplitud del vaivén vertical, en fracción de alto.
    vaiven: f32,
    fase: f32,
}

/// Una mota flotando. Sube despacio y reaparece abajo.
#[derive(Component)]
struct Mota {
    velocidad: f32,
    deriva: f32,
    fase: f32,
}

/// Cuántas motas por pantalla. Bajas a propósito: son nodos de UI y el Redmi
/// paga cada uno por frame (regla 2).
const MOTAS: usize = 14;

/// **Arma el fondo dentro del nodo que se le pase.**
///
/// Las capas van en posición absoluta y a pantalla completa, así que no empujan
/// al contenido: quien llame a esto puede seguir metiendo hijos normalmente.
pub fn fondo(padre: &mut ChildBuilder, ambiente: Ambiente, s: f32) {
    let capas = ambiente.capas();
    // Tres bandas anchas, cada una más rápida y más cerca que la anterior. Se
    // dibujan más anchas que la pantalla (140 %) para que al derivar no aparezca
    // el borde.
    for (i, color) in capas.iter().enumerate() {
        let f = i as f32;
        padre.spawn((
            Node {
                position_type: PositionType::Absolute,
                left: Val::Percent(-20.0),
                top: Val::Percent(8.0 + f * 24.0),
                width: Val::Percent(140.0),
                height: Val::Percent(42.0 - f * 6.0),
                ..default()
            },
            BackgroundColor(color.with_alpha(0.85)),
            BorderRadius::all(Val::Percent(50.0)),
            CapaParallax {
                velocidad: -0.012 * (f + 1.0),
                vaiven: 0.010 * (f + 1.0),
                fase: f * 2.1,
            },
            Name::new("CapaParallax"),
        ));
    }
    // **Los faroles.** Un halo cálido y quieto, colgado a distintas alturas. No
    // se animan a propósito: lo que se mueve en esta pantalla es el personaje,
    // y tres luces parpadeando al lado le roban el ojo.
    for i in 0..ambiente.faroles() {
        let f = i as f32;
        let ancho = (26.0 + f * 8.0) * s.max(0.6);
        padre.spawn((
            Node {
                position_type: PositionType::Absolute,
                left: Val::Percent(12.0 + f * 33.0),
                top: Val::Percent(14.0 + (i % 2) as f32 * 21.0),
                width: Val::Px(ancho),
                height: Val::Px(ancho),
                ..default()
            },
            BackgroundColor(Paleta::NARANJA.with_alpha(0.13)),
            BorderRadius::all(Val::Percent(50.0)),
            Name::new("FarolParallax"),
        ));
        // El corazón del farol: chico y más encendido, si no el halo solo se lee
        // como una mancha y no como una luz.
        padre.spawn((
            Node {
                position_type: PositionType::Absolute,
                left: Val::Percent(12.0 + f * 33.0 + 1.4),
                top: Val::Percent(14.0 + (i % 2) as f32 * 21.0 + 2.6),
                width: Val::Px(ancho * 0.22),
                height: Val::Px(ancho * 0.22),
                ..default()
            },
            BackgroundColor(Color::srgba(1.0, 0.88, 0.60, 0.85)),
            BorderRadius::all(Val::Percent(50.0)),
            Name::new("FarolLlama"),
        ));
    }
    let mota = ambiente.mota();
    for i in 0..MOTAS {
        let f = i as f32 / MOTAS as f32;
        let lado = 2.0 + (i * 37 % 94) as f32;
        padre.spawn((
            Node {
                position_type: PositionType::Absolute,
                left: Val::Percent(lado),
                top: Val::Percent(f * 100.0),
                width: Val::Px((2.0 + (i % 3) as f32) * s.max(0.6)),
                height: Val::Px((2.0 + (i % 3) as f32) * s.max(0.6)),
                ..default()
            },
            BackgroundColor(mota.with_alpha(0.20 + 0.25 * ((i % 4) as f32 / 4.0))),
            BorderRadius::all(Val::Percent(50.0)),
            Mota { velocidad: 1.6 + (i % 5) as f32 * 0.7, deriva: 0.6 + f, fase: f * 6.28 },
            Name::new("MotaParallax"),
        ));
    }
}


// ---------------------------------------------------------------------------
// 4. El fondo de VERDAD: el mismo bosque del mockup
// ---------------------------------------------------------------------------

/// **El parallax con el arte del pack** (`parallax.py` del mockup, en Bevy).
///
/// Tres capas del mismo mundo a distinta velocidad sobre un piso de pasto
/// azulejado, un velo que da la hora del día y motas que suben. Es lo que se ve
/// detrás del menú, de las listas y del creador de personaje en las imágenes de
/// referencia — el fondo abstracto de rectángulos que había antes no era eso.
///
/// **El costo**: un nodo azulejado para el piso (uno, no 300: lo repite la GPU
/// con `NodeImageMode::Tiled`) y ~62 sprites repartidos en tres capas, cada capa
/// duplicada para que el bucle no muestre el corte. Sólo vive en pantallas de
/// menú, donde no hay mundo que dibujar.
pub mod bosque {
    use super::*;
    use crate::art2d::ArtLibrary;

    /// Lo lejano: matorral y piedritas, chiquito y lento.
    ///
    /// Sin `prop/cesped_cortado` (29 sep 2026): es una LOSA plana de césped, un
    /// cuadrado sin silueta, y a escala 2 salía como parches cuadrados más
    /// claros y más oscuros por todo el fondo — "está roto al fondo". Tampoco
    /// nada marcado `S` en `scripts/revision-visual.json`.
    const LEJOS: [&str; 4] =
        ["nature/arbusto_1", "nature/arbusto_10", "nature/flores_rojo", "nature/piedra_1"];
    /// La capa del medio del bosque de día.
    const MEDIO: [&str; 4] =
        ["nature/arbol_10", "nature/arbusto_1", "nature/piedra_1", "prop/farol_de_calle_1"];
    /// La capa del medio de noche: los faroles son los que dan la noche.
    const MEDIO_NOCHE: [&str; 5] = [
        "prop/farol_de_calle_1",
        "prop/farol_de_calle_2",
        "nature/arbol_10",
        "nature/arbusto_1",
        "prop/farol",
    ];
    /// Lo cercano: árboles grandes y oscurecidos.
    const CERCA: [&str; 3] = ["nature/arbol_1", "nature/arbol_100", "nature/arbol_103"];

    /// El lado del azulejo de pasto en unidades del mockup (32 px de pack × 3).
    const AZULEJO: f32 = 96.0;

    /// Cuántas motas flotan. Las mismas 60 del mockup.
    const MOTAS: usize = 60;

    /// Un generador chico y repetible: el fondo tiene que salir igual en cada
    /// arranque, si no la pantalla "parpadea de sitio" al volver al menú.
    struct Az(u64);
    impl Az {
        fn nuevo(semilla: u64) -> Self {
            Self(semilla.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407))
        }
        fn siguiente(&mut self) -> u64 {
            self.0 = self.0.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
            self.0 >> 11
        }
        fn rango(&mut self, a: f32, b: f32) -> f32 {
            a + (self.siguiente() % 100_000) as f32 / 100_000.0 * (b - a)
        }
        fn indice(&mut self, n: usize) -> usize {
            (self.siguiente() % n.max(1) as u64) as usize
        }
    }

    /// Una capa del parallax: cuánto recorre por segundo, en unidades del mockup.
    #[derive(Component)]
    pub struct Capa {
        /// Anchos de pantalla por segundo (negativo = hacia la izquierda).
        velocidad: f32,
        /// Desplazamiento acumulado, en unidades.
        x: f32,
        /// De qué copia se trata: la segunda va corrida un ancho.
        copia: f32,
        k: f32,
    }

    /// Una mota (o luciérnaga) que sube.
    #[derive(Component)]
    pub struct Mota {
        velocidad: f32,
        k: f32,
    }

    /// El velo que le da la hora del día al mismo bosque.
    fn velo(ambiente: Ambiente) -> Color {
        match ambiente {
            Ambiente::Menu => Color::srgba(0.063, 0.047, 0.102, 0.439),
            Ambiente::Creador => Color::srgba(0.071, 0.094, 0.267, 0.588),
        }
    }

    /// Los props de cada capa según la hora.
    fn capas(ambiente: Ambiente) -> [(&'static [&'static str], usize, f32, f32, f32, f32); 3] {
        match ambiente {
            // (props, cuántos, escala, y0, y1, oscurecer)
            Ambiente::Menu => [
                (&LEJOS[..], 34, 2.0, -40.0, 1080.0, 0.0),
                (&MEDIO[..], 20, 3.0, -60.0, 980.0, 0.0),
                (&CERCA[..], 8, 5.0, -300.0, 740.0, 0.34),
            ],
            Ambiente::Creador => [
                (&LEJOS[..], 30, 2.0, -40.0, 1080.0, 0.0),
                (&MEDIO_NOCHE[..], 22, 3.0, -60.0, 980.0, 0.0),
                (&CERCA[..], 8, 5.0, -300.0, 740.0, 0.5),
            ],
        }
    }

    /// **Arma el fondo entero dentro del nodo que se le pase.**
    ///
    /// `k` es el factor del mockup (`ui_pack::k`): todo lo de acá está en las
    /// coordenadas de la imagen de referencia.
    pub fn fondo(padre: &mut ChildBuilder, art: &ArtLibrary, ambiente: Ambiente, k: f32) {
        // — el piso —
        let piso = Node {
            position_type: PositionType::Absolute,
            left: Val::Px(0.0),
            top: Val::Px(0.0),
            width: Val::Percent(100.0),
            height: Val::Percent(100.0),
            ..default()
        };
        match art.ui_icon("tiles/grass") {
            Some(mut img) => {
                img.image_mode = bevy::ui::widget::NodeImageMode::Tiled {
                    tile_x: true,
                    tile_y: true,
                    stretch_value: AZULEJO * k / 32.0,
                };
                padre.spawn((piso, img));
            }
            // Sin pack cargado, pasto plano: la pantalla nunca queda negra.
            None => {
                padre.spawn((piso, BackgroundColor(Color::srgb(0.278, 0.592, 0.341))));
            }
        }

        // — las tres capas, cada una con su copia corrida un ancho —
        for (i, (props, cuantos, escala, y0, y1, oscurecer)) in
            capas(ambiente).into_iter().enumerate()
        {
            let velocidad = (i as f32 + 1.0) * 0.045;
            for copia in [0.0_f32, 1.0] {
                let mut az = Az::nuevo(3 + i as u64);
                padre
                    .spawn((
                        Node {
                            position_type: PositionType::Absolute,
                            left: Val::Px(copia * crate::ui_pack::ANCHO * k),
                            top: Val::Px(0.0),
                            width: Val::Px(crate::ui_pack::ANCHO * k),
                            height: Val::Percent(100.0),
                            ..default()
                        },
                        Capa { velocidad, x: 0.0, copia, k },
                        Name::new("CapaBosque"),
                    ))
                    .with_children(|capa| {
                        for _ in 0..cuantos {
                            let clave = props[az.indice(props.len())];
                            let x = az.rango(0.0, crate::ui_pack::ANCHO);
                            let y = az.rango(y0, y1);
                            let Some(px) = art.ui_icon_px(clave) else { continue };
                            let Some(img) = art.ui_icon(clave) else { continue };
                            let (w, h) = (px.x * escala, px.y * escala);
                            let mut nodo = crate::ui_pack::caja(x, y, w, h, k);
                            nodo.overflow = Overflow::visible();
                            let mut ec = capa.spawn((nodo, img));
                            if oscurecer > 0.0 {
                                // El velo de la capa cercana: el mockup la apaga
                                // contra el fondo para que no compita con la UI.
                                ec.with_children(|s| {
                                    s.spawn((
                                        Node {
                                            position_type: PositionType::Absolute,
                                            left: Val::Px(0.0),
                                            top: Val::Px(0.0),
                                            width: Val::Percent(100.0),
                                            height: Val::Percent(100.0),
                                            ..default()
                                        },
                                        BackgroundColor(Color::srgba(
                                            0.039, 0.031, 0.071, oscurecer,
                                        )),
                                    ));
                                });
                            }
                        }
                    });
            }
        }

        // — el velo de la hora —
        padre.spawn((
            Node {
                position_type: PositionType::Absolute,
                left: Val::Px(0.0),
                top: Val::Px(0.0),
                width: Val::Percent(100.0),
                height: Val::Percent(100.0),
                ..default()
            },
            BackgroundColor(velo(ambiente)),
        ));

        // — las motas —
        let luciernagas = ambiente == Ambiente::Creador;
        let color = if luciernagas {
            Color::srgba(0.737, 1.0, 0.690, 0.72)
        } else {
            Color::srgba(1.0, 0.933, 0.745, 0.66)
        };
        let mut az = Az::nuevo(10);
        for _ in 0..MOTAS {
            let x = az.rango(0.0, crate::ui_pack::ANCHO);
            let y = az.rango(0.0, crate::ui_pack::ALTO);
            let r = [3.0_f32, 4.0, 6.0][az.indice(3)];
            let v = az.rango(0.35, 0.9) * 120.0;
            padre.spawn((
                crate::ui_pack::caja(x, y, r * 2.0, r * 2.0, k),
                BackgroundColor(color),
                BorderRadius::all(Val::Percent(50.0)),
                Mota { velocidad: v, k },
                Name::new("MotaBosque"),
            ));
        }
    }

    /// Corre las capas. El bucle cierra solo: al pasar un ancho, vuelve a cero.
    pub fn mover(
        time: Res<Time<bevy::time::Real>>,
        mut capas: Query<(&mut Node, &mut Capa)>,
        mut motas: Query<(&mut Node, &Mota), Without<Capa>>,
    ) {
        let dt = time.delta_secs();
        for (mut nodo, mut capa) in &mut capas {
            capa.x -= capa.velocidad * crate::ui_pack::ANCHO * dt;
            if capa.x <= -crate::ui_pack::ANCHO {
                capa.x += crate::ui_pack::ANCHO;
            }
            nodo.left = Val::Px((capa.x + capa.copia * crate::ui_pack::ANCHO) * capa.k);
        }
        for (mut nodo, mota) in &mut motas {
            if let Val::Px(y) = nodo.top {
                let mut y2 = y - mota.velocidad * mota.k * dt;
                if y2 < -20.0 * mota.k {
                    y2 = (crate::ui_pack::ALTO + 10.0) * mota.k;
                }
                nodo.top = Val::Px(y2);
            }
        }
    }
}

pub struct ParallaxPlugin;

impl Plugin for ParallaxPlugin {
    fn build(&self, app: &mut App) {
        // `Time<Real>` y no el virtual: el fondo del menú tiene que moverse aunque
        // el juego esté pausado, que es justo cuando el menú se ve.
        app.add_systems(Update, (mover_capas, mover_motas, bosque::mover));
    }
}

fn mover_capas(time: Res<Time<bevy::time::Real>>, mut q: Query<(&mut Node, &mut CapaParallax)>) {
    let dt = time.delta_secs();
    let t = time.elapsed_secs();
    for (mut nodo, mut capa) in &mut q {
        capa.fase += dt * capa.velocidad * 100.0;
        // El bucle: se envuelve en 40 % de ancho para que nunca se vea el corte.
        let x = -20.0 + (capa.fase % 40.0 + 40.0) % 40.0 - 20.0;
        nodo.left = Val::Percent(x);
        if let Val::Percent(base) = nodo.height {
            let _ = base;
        }
        // Vaivén vertical suave, desfasado por capa.
        let dy = (t * 0.25 + capa.fase * 0.02).sin() * capa.vaiven * 100.0;
        nodo.margin.top = Val::Percent(dy);
    }
}

fn mover_motas(time: Res<Time<bevy::time::Real>>, mut q: Query<(&mut Node, &mut Mota)>) {
    let dt = time.delta_secs();
    let t = time.elapsed_secs();
    for (mut nodo, mut mota) in &mut q {
        mota.fase += dt;
        if let Val::Percent(y) = nodo.top {
            let mut y2 = y - mota.velocidad * dt;
            if y2 < -4.0 {
                y2 = 104.0;
            }
            nodo.top = Val::Percent(y2);
        }
        nodo.margin.left = Val::Percent((t * 0.4 + mota.fase).sin() * mota.deriva);
    }
}
