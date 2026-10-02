//! **Idioma de la interfaz: español (el original) e inglés.**
//!
//! El juego escribe TODO en castellano, y así sigue siendo la fuente de verdad.
//! El inglés es una segunda versión que se elige en AJUSTES → IDIOMA y se guarda
//! en `idioma.cfg`. No se reescribió cada sitio que arma un texto: un sistema
//! (`traducir_textos`) mira los `Text`/`Text2d` que cambian y, si el idioma es
//! inglés, los pasa por el diccionario de abajo. Guarda el original en
//! `TextoEs` para poder volver al español sin reconstruir nada.
//!
//! Cómo se traduce un texto (`traducir`):
//! 1. coincidencia exacta con `FRASES`;
//! 2. si no, por **tramos**: se parte por ` · `, ` | ` y saltos de línea;
//! 3. si no, reemplazo de frases conocidas dentro del texto (más largas primero),
//!    que cubre los textos con números (`Mantené para picar {}`, `página 2 de 5`).
//!
//! Para agregar un texto nuevo: una línea en `FRASES`. Lo que no esté sigue
//! saliendo en castellano — nunca roto ni vacío.

use bevy::prelude::*;
use std::borrow::Cow;
use std::collections::HashMap;
use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::OnceLock;

#[derive(Resource, Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum Idioma {
    #[default]
    Es,
    En,
}

impl Idioma {
    pub fn otro(self) -> Self {
        match self {
            Idioma::Es => Idioma::En,
            Idioma::En => Idioma::Es,
        }
    }
    /// Cómo se llama a sí mismo (el botón de AJUSTES lo muestra siempre así).
    pub fn nombre(self) -> &'static str {
        match self {
            Idioma::Es => "ESPAÑOL",
            Idioma::En => "ENGLISH",
        }
    }
    pub fn codigo(self) -> &'static str {
        match self {
            Idioma::Es => "es",
            Idioma::En => "en",
        }
    }
}

/// Espejo global del idioma, para `tr()` desde código que no es un sistema.
static ACTUAL: AtomicU8 = AtomicU8::new(0);

pub fn idioma_actual() -> Idioma {
    if ACTUAL.load(Ordering::Relaxed) == 1 { Idioma::En } else { Idioma::Es }
}

fn archivo() -> std::path::PathBuf {
    crate::flags::var("HA_IDIOMA_FILE")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|_| std::path::PathBuf::from("idioma.cfg"))
}

fn cargar() -> Idioma {
    match std::fs::read_to_string(archivo()).ok().as_deref().map(str::trim) {
        Some("en") => Idioma::En,
        _ => Idioma::Es,
    }
}

fn guardar(i: Idioma) {
    if let Err(e) = std::fs::write(archivo(), i.codigo()) {
        eprintln!("idioma: no se pudo guardar: {e}");
    }
}

/// **Traduce un texto al idioma actual.** En español devuelve el mismo `&str`
/// sin copiar.
pub fn tr(es: &str) -> Cow<'_, str> {
    if idioma_actual() == Idioma::Es {
        return Cow::Borrowed(es);
    }
    traducir(es)
}

fn indice() -> &'static HashMap<&'static str, &'static str> {
    static I: OnceLock<HashMap<&'static str, &'static str>> = OnceLock::new();
    I.get_or_init(|| FRASES.iter().copied().collect())
}

/// Las frases ordenadas de más larga a más corta, para el reemplazo parcial.
fn por_largo() -> &'static Vec<(&'static str, &'static str)> {
    static I: OnceLock<Vec<(&'static str, &'static str)>> = OnceLock::new();
    I.get_or_init(|| {
        let mut v: Vec<_> = FRASES.iter().copied().filter(|(es, _)| es.chars().count() >= 5).collect();
        v.sort_by_key(|(es, _)| std::cmp::Reverse(es.len()));
        v
    })
}

/// Traduce al inglés (sin mirar el idioma actual).
pub fn traducir(es: &str) -> Cow<'_, str> {
    if es.is_empty() {
        return Cow::Borrowed(es);
    }
    if let Some(en) = indice().get(es) {
        return Cow::Borrowed(en);
    }
    // Plantillas con número: "página 2 de 5", "día 3", "hace 4 días".
    if let Some(r) = es.strip_prefix("página ") {
        return Cow::Owned(format!("page {}", r.replace(" de ", " of ")));
    }
    if let Some(r) = es.strip_prefix("día ") {
        return Cow::Owned(format!("day {r}"));
    }
    if let Some(r) = es.strip_prefix("DÍA ") {
        return Cow::Owned(format!("DAY {r}"));
    }
    if let Some(n) = es.strip_prefix("hace ").and_then(|r| r.strip_suffix(" días")) {
        return Cow::Owned(format!("{n} days ago"));
    }
    // Por tramos.
    for sep in ["\n", " · ", " | ", "  |  "] {
        if es.contains(sep) {
            let partes: Vec<String> = es.split(sep).map(|p| traducir(p).into_owned()).collect();
            let r = partes.join(sep);
            return if r == es { Cow::Borrowed(es) } else { Cow::Owned(r) };
        }
    }
    // Reemplazo de frases conocidas.
    let mut r = es.to_string();
    for (a, b) in por_largo() {
        if r.contains(a) {
            r = r.replace(a, b);
        }
    }
    if r == es { Cow::Borrowed(es) } else { Cow::Owned(r) }
}

/// El original en español de un texto que se tradujo, y lo que escribimos.
#[derive(Component)]
pub struct TextoEs {
    es: String,
    en: String,
}

/// Cualquier cosa con un texto editable (`Text` de UI, `Text2d` del mundo).
fn procesar(
    commands: &mut Commands,
    e: Entity,
    texto: &mut String,
    marca: Option<Mut<TextoEs>>,
    idioma: Idioma,
) {
    match (marca, idioma) {
        (Some(mut m), Idioma::En) => {
            if *texto == m.en {
                return;
            }
            m.es = texto.clone();
            let en = traducir(&m.es).into_owned();
            m.en = en.clone();
            *texto = en;
        }
        (Some(mut m), Idioma::Es) => {
            // Si el texto es lo que escribimos nosotros, se vuelve al original;
            // si cambió, el dueño ya escribió en español.
            if *texto == m.en && m.en != m.es {
                *texto = m.es.clone();
            }
            m.en = m.es.clone();
        }
        (None, Idioma::En) => {
            let en = traducir(texto).into_owned();
            if en != *texto {
                commands.entity(e).insert(TextoEs { es: texto.clone(), en: en.clone() });
                *texto = en;
            }
        }
        (None, Idioma::Es) => {}
    }
}

fn traducir_textos(
    mut commands: Commands,
    idioma: Res<Idioma>,
    mut ui: Query<(Entity, &mut Text, Option<&mut TextoEs>), Changed<Text>>,
    mut mundo: Query<(Entity, &mut Text2d, Option<&mut TextoEs>), (Changed<Text2d>, Without<Text>)>,
) {
    if idioma.is_changed() {
        return;
    }
    let i = *idioma;
    for (e, mut t, m) in &mut ui {
        procesar(&mut commands, e, &mut t.0, m, i);
    }
    for (e, mut t, m) in &mut mundo {
        procesar(&mut commands, e, &mut t.0, m, i);
    }
}

/// Al cambiar de idioma se repasan TODOS los textos.
fn recorrer_todo(
    mut commands: Commands,
    idioma: Res<Idioma>,
    mut ui: Query<(Entity, &mut Text, Option<&mut TextoEs>)>,
    mut mundo: Query<(Entity, &mut Text2d, Option<&mut TextoEs>), Without<Text>>,
) {
    if !idioma.is_changed() {
        return;
    }
    ACTUAL.store(if *idioma == Idioma::En { 1 } else { 0 }, Ordering::Relaxed);
    guardar(*idioma);
    let i = *idioma;
    for (e, mut t, m) in &mut ui {
        // Se fuerza el original: el texto actual puede ser el traducido.
        let mut buf = match &m {
            Some(m) if t.0 == m.en => m.es.clone(),
            _ => t.0.clone(),
        };
        if let Some(mut m) = m {
            m.es = buf.clone();
            m.en = if i == Idioma::En { traducir(&buf).into_owned() } else { buf.clone() };
            t.0 = m.en.clone();
        } else if i == Idioma::En {
            let en = traducir(&buf).into_owned();
            if en != buf {
                commands.entity(e).insert(TextoEs { es: std::mem::take(&mut buf), en: en.clone() });
                t.0 = en;
            }
        }
    }
    for (e, mut t, m) in &mut mundo {
        let buf = match &m {
            Some(m) if t.0 == m.en => m.es.clone(),
            _ => t.0.clone(),
        };
        if let Some(mut m) = m {
            m.es = buf.clone();
            m.en = if i == Idioma::En { traducir(&buf).into_owned() } else { buf };
            t.0 = m.en.clone();
        } else if i == Idioma::En {
            let en = traducir(&buf).into_owned();
            if en != buf {
                commands.entity(e).insert(TextoEs { es: buf, en: en.clone() });
                t.0 = en;
            }
        }
    }
}

pub struct IdiomaPlugin;

impl Plugin for IdiomaPlugin {
    fn build(&self, app: &mut App) {
        let i = cargar();
        ACTUAL.store(if i == Idioma::En { 1 } else { 0 }, Ordering::Relaxed);
        app.insert_resource(i)
            .add_systems(PostUpdate, (recorrer_todo, traducir_textos).chain());
    }
}

/// Las frases. Español → inglés.
const FRASES: &[(&str, &str)] = &[
    // Civilización (fragmentos: sirven dentro de textos con números y nombres)
    ("Culto:", "Cult:"),
    ("Saberes:", "Knowledge:"),
    ("Estado:", "State:"),
    ("En viaje:", "Traveling:"),
    ("rumbo a", "bound for"),
    ("llega en", "arrives in"),
    ("días.", "days."),
    ("Fuera de la Tierra hay gente en:", "Outside Earth there are people on:"),
    ("ningún lado todavía", "nowhere yet"),
    ("contra", "against"),
    ("desde el año", "since year"),
    ("ni se lo plantean.", "don't even consider it."),
    ("Obra en marcha:", "Work in progress:"),
    ("Saberes descubiertos en el mundo:", "Knowledge discovered in the world:"),
    ("Cultos:", "Cults:"),
    ("pueblos vivos", "living towns"),
    ("habitantes", "inhabitants"),
    ("personas con nombre", "named people"),
    ("Cavás y sellás un refugio bajo tierra. Falta abastecerlo e invitar a la gente.", "You dig and seal an underground shelter. It still needs supplies and people."),
    ("Guardás", "You store"),
    ("manzanas en el refugio.", "apples in the shelter."),
    ("Les enseñás", "You teach them"),
    ("No queda nada que puedas enseñarles ahora.", "There is nothing left you can teach them now."),
    ("Mediás entre", "You mediate between"),
    ("Movés los ánimos. La gente lo nota, y a algunos no les gusta.", "You stir people up. They notice, and some dislike it."),
    ("Te presentás como candidato en", "You run for office in"),
    ("La próxima elección dirá qué piensan de vos.", "The next election will tell what they think of you."),
    ("Los sobrevivientes salen a la luz y fundan", "The survivors come out into the light and found"),
    ("recibe una pieza de tecnología ajena: ya saben por dónde empezar.", "receives a piece of foreign technology: they know where to start."),
    ("Lo que", "What"),
    ("cree de las leyes de ESTE universo (y lo que es de verdad):", "believes about the laws of THIS universe (and what is true):"),
    ("Todavía no vivís en ningún pueblo: no sabés qué creen de la física.", "You don't live in any town yet: you don't know what they believe about physics."),
    ("Lo que hagas entra por las mismas puertas que las de cualquiera: el mundo responde con sus reglas.", "What you do goes through the same doors as anyone's: the world answers with its own rules."),
    ("Las leyes sólo las pueden cambiar", "Only the following can change the laws:"),
    ("Ana va preso en la plaza por robar.", "Ana is jailed in the square for stealing."),
    ("es desterrado de la plaza.", "is banished from the square."),
    ("lanza un arma nuclear sobre la plaza.", "launches a nuclear weapon at the square."),
    ("lanza una nave tripulada hacia la Luna.", "launches a crewed ship to the Moon."),
    ("Resplandor", "Glare"),
    ("preso", "jailed"),
    ("desterrado", "banished"),
    ("Régimen", "Regime"),
    ("ABASTECER", "STOCK"),
    ("Doña", "Mrs."),
    // Alijos
    ("ALIJO a", "CACHE"),
    ("Abrí un alijo", "Open a cache"),
    ("Cinco alijos", "Five caches"),
    // Gameplay
    ("CADÁVER", "CORPSE"),
    ("ROPERO", "WARDROBE"),
    ("HELADERA", "FRIDGE"),
    ("ALACENA", "PANTRY"),
    ("CÓMODA", "DRESSER"),
    ("BOTIQUÍN", "MEDICINE CABINET"),
    ("Novato", "Rookie"),
    ("Explorador", "Explorer"),
    ("Pionero", "Pioneer"),
    ("Leyenda", "Legend"),
    ("Ropa", "Clothes"),
    ("Gorra roja", "Red cap"),
    ("Gorra azul", "Blue cap"),
    ("Gorro de lana gris", "Grey wool hat"),
    ("Calzado", "Footwear"),
    ("Necesito", "I need"),
    ("Llevás", "You have"),
    // Experimentos / técnica
    ("Experimentos:", "Experiments:"),
    ("recetas firmes", "confirmed recipes"),
    ("lo mejor que sabe hacer:", "best it can make:"),
    ("cree en", "believes in"),
    ("que todavía no probó sin él", "it has not tested without it"),
    ("EXPERIMENTAR", "EXPERIMENT"),
    ("EXPERIMENTOS", "EXPERIMENTS"),
    ("PROBAR", "TRY"),
    ("CALOR −", "HEAT −"),
    ("CALOR +", "HEAT +"),
    ("SOLTAR A", "DROP A"),
    ("SOLTAR B", "DROP B"),
    ("RECETAS (tocá una para repetirla):", "RECIPES (tap one to repeat it):"),
    ("Arcilla", "Clay"),
    ("Carbón", "Coal"),
    ("Mena de cobre", "Copper ore"),
    ("Mena de estaño", "Tin ore"),
    ("Mena de hierro", "Iron ore"),
    ("Caliza", "Limestone"),
    ("Resina", "Resin"),
    ("Sal", "Salt"),
    ("Metal", "Metal"),
    ("Aleación", "Alloy"),
    ("Cerámica", "Ceramic"),
    ("Vidrio", "Glass"),
    ("Aglutinante", "Binder"),
    ("Compuesto", "Composite"),
    ("Carbón vegetal", "Charcoal"),
    // Multijugador
    ("MULTIJUGADOR", "MULTIPLAYER"),
    ("ANFITRIÓN LAN", "HOST LAN"),
    ("CERRAR LAN", "CLOSE LAN"),
    ("ONLINE", "ONLINE"),
    ("DESCONECTAR", "DISCONNECT"),
    ("UNIRSE", "JOIN"),
    ("SERVIDOR ONLINE", "ONLINE SERVER"),
    ("Buscando anfitriones en tu red...", "Looking for hosts on your network..."),
    ("Un amigo abre su mundo con MENÚ · MULTIJUGADOR · ANFITRIÓN LAN.", "A friend opens their world with MENU · MULTIPLAYER · HOST LAN."),
    ("sin conexión", "offline"),
    ("conectando...", "connecting..."),
    ("jugadores en la sala", "players in the room"),
    ("jugando", "playing"),
    ("Te encuentran en", "Others find you at"),
    ("conectado ·", "connected ·"),
    ("anfitrión ·", "host ·"),
    ("entró", "joined"),
    ("salió", "left"),
    ("¡Hola!", "Hello!"),
    ("¡Vení!", "Come here!"),
    ("¡Ayuda!", "Help!"),
    ("Gracias.", "Thanks."),
    // Menú principal / paneles
    ("JUGAR", "PLAY"),
    ("CONTINUAR", "CONTINUE"),
    ("NUEVA PARTIDA", "NEW GAME"),
    ("PARTIDA", "GAME"),
    ("PARTIDAS", "SAVED GAMES"),
    ("PERSONAJE", "CHARACTER"),
    ("PERSONAJES", "CHARACTERS"),
    ("AJUSTES", "SETTINGS"),
    ("CONTROLES", "CONTROLS"),
    ("ACTUALIZAR", "UPDATE"),
    ("CRÉDITOS", "CREDITS"),
    ("SALIR", "QUIT"),
    ("VOLVER", "BACK"),
    ("MENÚ", "MENU"),
    ("MENU", "MENU"),
    ("MENÚ PRINCIPAL", "MAIN MENU"),
    ("PANTALLA COMPLETA", "FULLSCREEN"),
    ("IDIOMA", "LANGUAGE"),
    ("Idioma", "Language"),
    ("SIN PERSONAJE", "NO CHARACTER"),
    ("CONSOLA DESTRABADA", "CONSOLE UNLOCKED"),
    ("Mover · el stick de la izquierda", "Move · the left stick"),
    ("Mirar y tocar · el dedo sobre el mundo", "Look and tap · your finger on the world"),
    ("Volar · mantené SALTAR en el aire", "Fly · hold JUMP in the air"),
    ("Mochila · el botón de abajo a la derecha", "Backpack · the bottom-right button"),
    ("Acercar y alejar · pellizcá la pantalla", "Zoom · pinch the screen"),
    ("Estás en la versión", "You are on version"),
    ("El APK nuevo se baja de la misma dirección", "The new APK downloads from the same address"),
    ("desde la que instalaste éste.", "you installed this one from."),
    ("Pixel art · LimeZu", "Pixel art · LimeZu"),
    ("Motor · Bevy", "Engine · Bevy"),
    // Ajustes (en juego)
    ("Cámara", "Camera"),
    ("Movimiento", "Movement"),
    ("Curva", "Curve"),
    ("Resolución", "Resolution"),
    ("Efectos", "Effects"),
    ("Música", "Music"),
    ("libre", "free"),
    // Listas / creación
    ("NUEVO", "NEW"),
    ("ELEGIR", "PICK"),
    ("ELEGIDO", "PICKED"),
    ("EDITAR", "EDIT"),
    ("BORRAR", "DELETE"),
    ("¿SEGURO?", "SURE?"),
    ("AL AZAR", "RANDOM"),
    ("ALEATORIO", "RANDOM"),
    ("CREAR MUNDO", "CREATE WORLD"),
    ("CREAR PERSONAJE", "CREATE CHARACTER"),
    ("CREATIVO", "CREATIVE"),
    ("SUPERVIVENCIA", "SURVIVAL"),
    ("MODO", "MODE"),
    ("NOMBRE", "NAME"),
    ("TU NOMBRE", "YOUR NAME"),
    ("SEMILLA", "SEED"),
    ("ESPACIO", "SPACE"),
    ("LISTO", "DONE"),
    ("ÚLTIMA", "LAST"),
    ("recién", "just now"),
    ("semilla", "seed"),
    ("Todavía no hay ninguno.", "None yet."),
    ("Todavía no hay ninguna. Tocá NUEVA PARTIDA.", "None yet. Tap NEW GAME."),
    ("Todavía no entró a ningún mundo.", "Has not entered any world yet."),
    ("Se junta, se fabrica, se pasa hambre.", "Gather, craft, go hungry."),
    ("Todo el catálogo a mano y vuelo libre.", "The whole catalog at hand and free flight."),
    ("El modo se elige acá. Después, en el juego, se cambia con la consola.", "Pick the mode here. In game, change it with the console."),
    ("Un personaje no pertenece a un mundo: entra al que elijas.", "A character does not belong to a world: it enters whichever you choose."),
    ("La papelera pide confirmación y borra sólo esa partida — el personaje no se toca.", "The bin asks for confirmation and deletes only that game — the character is untouched."),
    ("viven acá:", "live here:"),
    ("tocá el nombre para cambiarlo", "tap the name to change it"),
    ("ACCESORIOS", "ACCESSORIES"),
    ("CAPA", "LAYER"),
    ("COLOR", "COLOR"),
    ("CONFIRMAR", "CONFIRM"),
    ("CANCELAR", "CANCEL"),
    ("DE FRENTE", "FRONT"),
    ("DE ESPALDAS", "BACK"),
    ("DERECHA", "RIGHT"),
    ("IZQUIERDA", "LEFT"),
    ("HUECOS", "SLOTS"),
    ("NINGUNO", "NONE"),
    ("REINICIAR", "RESET"),
    ("GORRO", "HAT"),
    ("OJOS", "EYES"),
    ("PELO", "HAIR"),
    ("PIEL", "SKIN"),
    ("ROPA", "CLOTHES"),
    ("CONTENTA", "HAPPY"),
    ("DORMIDA", "ASLEEP"),
    ("ENOJADA", "ANGRY"),
    ("NEUTRA", "NEUTRAL"),
    ("TRISTE", "SAD"),
    // Carga
    ("PREPARANDO EL MUNDO", "PREPARING THE WORLD"),
    ("pintando el planeta...", "painting the planet..."),
    ("vistiendo a la gente...", "dressing the people..."),
    ("generando terreno...", "generating terrain..."),
    ("Cada planeta tiene relieve propio. Ninguno es una bola lisa.", "Every planet has its own relief. None is a smooth ball."),
    ("De noche la luz sale de las antorchas: llevate una.", "At night the light comes from torches: bring one."),
    ("La fauna aprende: lo que te funcionó ayer hoy puede fallar.", "Fauna learn: what worked yesterday may fail today."),
    ("Los aldeanos se acuerdan de lo que hiciste la última vez.", "Villagers remember what you did last time."),
    ("Mantené el dedo en el suelo para volar. Soltar aterriza.", "Keep your finger down to fly. Letting go lands."),
    ("Podés editar el mundo y queda guardado en tu partida.", "You can edit the world and it is saved in your game."),
    ("Subí derecho y el mundo se vuelve mapa. Nunca hay corte.", "Fly straight up and the world becomes a map. No cuts, ever."),
    ("Tocá a un aldeano para hablarle. Tiene cara y humor propios.", "Tap a villager to talk. They have their own face and mood."),
    // HUD / acciones
    ("SALTAR", "JUMP"),
    ("USAR", "USE"),
    ("BAJAR", "DOWN"),
    ("IZARSE", "PULL UP"),
    ("TREPAR", "CLIMB"),
    ("COMER", "EAT"),
    ("BEBER", "DRINK"),
    ("VESTIR", "WEAR"),
    ("MOCHILA", "BACKPACK"),
    ("FABRICAR", "CRAFT"),
    ("CONSTRUIR", "BUILD"),
    ("EN LA MANO", "IN HAND"),
    ("en mano:", "in hand:"),
    ("lo que llevás encima", "what you carry"),
    ("HAMBRE", "HUNGER"),
    ("SED", "THIRST"),
    ("TODO", "ALL"),
    ("PAREDES", "WALLS"),
    ("PIEZAS", "PIECES"),
    ("TECHOS", "ROOFS"),
    ("EDIFICIO", "BUILDING"),
    ("ANTERIOR", "PREVIOUS"),
    ("SIGUIENTE", "NEXT"),
    ("página", "page"),
    ("sin costo", "free"),
    ("AMANECER", "DAWN"),
    ("ATARDECER", "DUSK"),
    ("NOCHE", "NIGHT"),
    ("DÍA", "DAY"),
    ("DESPEJADO", "CLEAR"),
    ("LLUVIA", "RAIN"),
    ("NUBLADO", "CLOUDY"),
    ("TORMENTA", "STORM"),
    ("Mantené para cavar", "Hold to dig"),
    ("Mantené para picar", "Hold to mine"),
    ("Mantené para recoger", "Hold to pick up"),
    ("Mantené para levantar", "Hold to lift"),
    ("Mantené para beber", "Hold to drink"),
    ("Mantené para llenar el cubo", "Hold to fill the bucket"),
    ("Mantené para volcar el agua", "Hold to pour the water"),
    ("Mantené para volcar la lava", "Hold to pour the lava"),
    ("Mantené para cosechar", "Hold to harvest"),
    ("Mantené para sembrar", "Hold to plant"),
    ("Mantené para colocar", "Hold to place"),
    ("Mantené para arrancarlo", "Hold to pull it up"),
    ("Mantené para darle el hueso (domarlo)", "Hold to give the bone (tame it)"),
    ("Mantené para ponerle nombre a", "Hold to name"),
    ("Mantené para dar", "Hold to give"),
    ("Mantené para", "Hold to"),
    ("Mantené:", "Hold:"),
    ("Tocá para golpear", "Tap to hit"),
    ("Tocá para abrir:", "Tap to open:"),
    ("Tocá para ordeñar a", "Tap to milk"),
    ("Tocá a", "Tap"),
    ("para que se siente", "to make it sit"),
    ("para que te siga", "to make it follow you"),
    ("Tocá una pila: la mueve entera | Mantené: elegís cuántas", "Tap a stack: moves it whole | Hold: pick how many"),
    ("Acercate a una máquina", "Get close to a machine"),
    ("Esto no se puede beber", "You can't drink this"),
    ("Esto no se puede juntar", "You can't collect this"),
    ("La lava se enfrió en roca", "The lava cooled into rock"),
    ("Volcaste el agua", "You poured the water"),
    ("Volcaste la lava", "You poured the lava"),
    ("Todavía no hay fruta", "No fruit yet"),
    ("Una planta más", "One floor more"),
    ("Una planta menos", "One floor less"),
    ("Ordeñaste a", "You milked"),
    ("se sienta y espera", "sits and waits"),
    ("te sigue", "follows you"),
    ("en celo", "in heat"),
    ("cría", "calf"),
    ("¡enojado!", "angry!"),
    ("murió", "died"),
    ("domado", "tamed"),
    ("¡Nació una cría de", "A baby was born:"),
    ("listo para cosechar", "ready to harvest"),
    ("sembrado", "sown"),
    ("brote", "sprout"),
    ("creciendo", "growing"),
    ("regado", "watered"),
    ("Faltan", "Missing"),
    ("EDITAR UI", "EDIT UI"),
    ("GUARDAR", "SAVE"),
    ("GUARDAR TODO ->", "STORE ALL ->"),
    ("<- SACAR TODO", "<- TAKE ALL"),
    ("COFRE", "CHEST"),
    ("CERRAR", "CLOSE"),
    ("RECOGER", "PICK UP"),
    ("COLOCAR", "PLACE"),
    ("PONERLE NOMBRE", "NAME IT"),
    ("El nombre queda sobre el animal y lo hace tuyo.", "The name stays over the animal and makes it yours."),
    ("MAPA ORBITAL", "ORBITAL MAP"),
    ("TIERRA", "EARTH"),
    ("LUNA", "MOON"),
    ("SONDA", "PROBE"),
    ("AUTO", "AUTO"),
    ("GIRAR:", "ROTATE:"),
    ("NORTE", "NORTH"),
    ("ESTE", "EAST"),
    ("OESTE", "WEST"),
    ("SUR", "SOUTH"),
    ("PISO", "FLOOR"),
    ("SUBSUELO", "BASEMENT"),
    ("Abre una puerta en la pared apuntada", "Opens a door in the pointed wall"),
    ("Abre una ventana", "Opens a window"),
    ("Hormigón", "Concrete"),
    ("CARGANDO", "LOADING"),
    // NPC / compañero / sociedad
    ("HABLAR", "TALK"),
    ("ENCARGO", "ERRAND"),
    ("VENDER", "SELL"),
    ("COMPRAR", "BUY"),
    ("No llevás nada que valga un lingote.", "You have nothing worth a gold ingot."),
    ("¡Trato hecho!", "Deal!"),
    ("lingotes de oro.", "gold ingots."),
    ("Una prenda sale", "A garment costs"),
    ("CHARLAR", "CHAT"),
    ("COMERCIAR", "TRADE"),
    ("PREDICAR", "PREACH"),
    ("CONVERTIR", "CONVERT"),
    ("CALMAR", "CALM"),
    ("HOSTIL", "HOSTILE"),
    ("SER PAREJA", "BE PARTNERS"),
    ("SEGUIRME", "FOLLOW ME"),
    ("EXPLORAR", "EXPLORE"),
    ("REZAR", "PRAY"),
    ("Compañero", "Companion"),
    ("orden:", "order:"),
    ("explora por la zona", "explores the area"),
    ("reza cerca del ídolo", "prays near the idol"),
    ("tu pareja", "your partner"),
    ("pareja de", "partner of"),
    ("fiel del Ki", "Ki believer"),
    ("hijo/a tuyo y de", "your child with"),
    ("hijo/a de", "child of"),
    ("últimos mensajes:", "latest messages:"),
    ("(silencio... toca HABLAR)", "(silence... tap TALK)"),
    ("ALDEANO", "VILLAGER"),
    ("ALDEA", "VILLAGE"),
    ("CIUDAD", "CITY"),
    ("CONSTRUCTOR", "BUILDER"),
    ("CREYENTE", "BELIEVER"),
    ("FIEL", "FAITHFUL"),
    ("GRANJERO", "FARMER"),
    ("MINERO", "MINER"),
    ("SIN OFICIO", "NO TRADE"),
    ("Los Hijos del Ki", "The Children of Ki"),
    ("Voy con vos.", "I'm with you."),
    ("Rezo un rato.", "I'll pray a while."),
    ("Voy a mirar por ahí.", "I'll take a look around."),
    ("A tus órdenes.", "At your service."),
    ("Qué lindo día para andar.", "What a nice day for a walk."),
    ("Cuenta conmigo.", "Count on me."),
    ("Si te falta algo, avisame.", "If you need anything, tell me."),
    ("Me quedo con vos.", "I'm staying with you."),
    ("¡Creo en el Ki!", "I believe in Ki!"),
    ("Contame más...", "Tell me more..."),
    ("No tengo nada para comerciar aún.", "I have nothing to trade yet."),
    ("Traeme manzanas y hablamos.", "Bring me apples and we'll talk."),
    ("Cada uno se rebusca.", "Everyone fends for themselves."),
    ("Yo no fui.", "It wasn't me."),
    ("Uno con el Ki.", "One with Ki."),
    ("No te queremos aquí.", "We don't want you here."),
    ("No me fío de ti.", "I don't trust you."),
    ("Pagarás por lo que hiciste.", "You'll pay for what you did."),
    ("Al mediodía rezamos en el santuario.", "At noon we pray at the shrine."),
    ("Tomá,", "Here,"),
    // Civilización / MUNDO
    ("ACCIONES", "ACTIONS"),
    ("CRÓNICA", "CHRONICLE"),
    ("MUNDO", "WORLD"),
    ("PUEBLO", "TOWN"),
    ("PRESENTARME", "RUN FOR OFFICE"),
    ("RETIRARME", "WITHDRAW"),
    ("ENSEÑAR", "TEACH"),
    ("MEDIAR", "MEDIATE"),
    ("PEDIR PAZ", "ASK FOR PEACE"),
    ("FUNDAR PUEBLO NUEVO", "FOUND A NEW TOWN"),
    ("CONSTRUIR REFUGIO", "BUILD SHELTER"),
    ("ABASTECER REFUGIO", "STOCK SHELTER"),
    ("INVITAR AL REFUGIO", "INVITE TO SHELTER"),
    ("DONAR 5 MANZANAS", "DONATE 5 APPLES"),
    ("ENTREGAR PIEZA DE NAVE", "DELIVER SHIP PART"),
    ("IMPUESTO +", "TAX +"),
    ("IMPUESTO -", "TAX -"),
    ("PENA ROBO +", "THEFT PENALTY +"),
    ("PENA ROBO -", "THEFT PENALTY -"),
    ("CULTOS LIBRES +", "FREE CULTS +"),
    ("CULTOS LIBRES -", "FREE CULTS -"),
    ("AGITAR: FE", "STIR UP: FAITH"),
    ("AGITAR: FUERZA", "STIR UP: FORCE"),
    ("AGITAR: VOTO", "STIR UP: VOTE"),
    ("FÍSICA", "PHYSICS"),
    ("cárcel", "jail"),
    ("en paz", "at peace"),
    ("intuición", "intuition"),
    ("Año", "Year"),
    ("Gente:", "People:"),
    ("Comida:", "Food:"),
    ("Materiales:", "Materials:"),
    ("Tesoro:", "Treasury:"),
    ("Régimen:", "Regime:"),
    ("Gobierna:", "Ruled by:"),
    ("Guerras en curso", "Wars in progress"),
    ("Tu pueblo no está en guerra.", "Your town is not at war."),
    ("El mundo todavía no cargó.", "The world has not loaded yet."),
    ("Todavía no pasó nada digno de contarse.", "Nothing worth telling has happened yet."),
    ("Acercate a un pueblo: hasta que no vivas en uno, no sos de nadie.", "Get close to a town: until you live in one, you belong to nobody."),
    ("Todavía no vivís en ningún pueblo.", "You don't live in any town yet."),
    ("Acercate a uno: la gente te va a ir conociendo.", "Get close to one: people will get to know you."),
    ("Decretás la paz.", "You decree peace."),
    ("Retirás tu candidatura.", "You withdraw your candidacy."),
    ("No hay ningún refugio a mano.", "There is no shelter at hand."),
    ("No llevás comida para guardar.", "You carry no food to store."),
    ("No llevás ninguna pieza de nave.", "You carry no ship part."),
    ("Un refugio pide", "A shelter needs"),
    ("de piedra y", "of stone and"),
    ("de madera.", "of wood."),
    ("personas bajan al refugio.", "people go down to the shelter."),
    ("Impuesto al", "Tax at"),
    ("Pena por robo:", "Theft penalty:"),
    ("Libertad de cultos al", "Freedom of worship at"),
    ("Tu prestigio acá:", "Your standing here:"),
    ("Radiación en el pueblo:", "Radiation in town:"),
    ("Refugios en el mundo:", "Shelters in the world:"),
    ("Invierno nuclear en el mundo:", "Nuclear winter in the world:"),
    ("RADIACIÓN", "RADIATION"),
    ("buscá un refugio", "find a shelter"),
    ("Un resplandor blanco ilumina el horizonte.", "A white glare lights up the horizon."),
    ("Elección en la plaza.", "Election in the square."),
    // Objetivos / ítems
    ("Construcción", "Construction"),
    ("Módulo de lanzamiento", "Launch module"),
    ("Rumbo a la Luna", "Heading to the Moon"),
    ("Veta de hierro", "Iron vein"),
    ("Veta de oro", "Gold vein"),
    ("Árbol", "Tree"),
    ("Manzana", "Apple"),
    ("Hacha de hierro", "Iron axe"),
    ("Pan", "Bread"),
    ("Tierra", "Dirt"),
    ("Gema azul", "Blue gem"),
    ("Gema roja", "Red gem"),
    ("Gema", "Gem"),
    ("Lingote de oro", "Gold ingot"),
    ("Lingote de hierro", "Iron ingot"),
    ("Hierro", "Iron"),
    ("Oro", "Gold"),
    ("Uvas", "Grapes"),
    ("Mano (sin ítem)", "Hand (no item)"),
    ("Mochila cohete", "Rocket pack"),
    ("Farol", "Lantern"),
    ("Cuero", "Leather"),
    ("Gorro de cuero", "Leather cap"),
    ("Chaleco de cuero", "Leather vest"),
    ("Abrigo de lana", "Wool coat"),
    ("Botas de cuero", "Leather boots"),
    ("Carne de res", "Beef"),
    ("Carne de cerdo", "Pork"),
    ("Carne de cordero", "Mutton"),
    ("Carne de conejo", "Rabbit meat"),
    ("Carne", "Meat"),
    ("Pollo crudo", "Raw chicken"),
    ("Semillas de trigo", "Wheat seeds"),
    ("Semillas de calabaza", "Pumpkin seeds"),
    ("Trigo", "Wheat"),
    ("Zanahoria", "Carrot"),
    ("Calabaza", "Pumpkin"),
    ("Pluma", "Feather"),
    ("Lana", "Wool"),
    ("Hueso", "Bone"),
    ("Huevo", "Egg"),
    ("Etiqueta", "Name tag"),
    ("Cubo con agua", "Water bucket"),
    ("Cubo con leche", "Milk bucket"),
    ("Cubo con lava", "Lava bucket"),
    ("Cubo", "Bucket"),
    ("Naranja", "Orange"),
    ("Pieza: motor", "Part: engine"),
    ("Pieza: aleta", "Part: fin"),
    ("Pieza: patas de aterrizaje", "Part: landing legs"),
    ("Pieza: punta", "Part: nose"),
    ("Pieza: tanque de combustible", "Part: fuel tank"),
    ("Pico de hierro", "Iron pickaxe"),
    ("Pico de piedra", "Stone pickaxe"),
    ("Pico de madera", "Wood pickaxe"),
    ("Hacha de piedra", "Stone axe"),
    ("Hacha de madera", "Wood axe"),
    ("Tarta", "Pie"),
    ("Poción azul", "Blue potion"),
    ("Poción verde", "Green potion"),
    ("Sonda de navegación", "Navigation probe"),
    ("Arena", "Sand"),
    ("Escudo de madera", "Wooden shield"),
    ("Nieve", "Snow"),
    ("Sopa", "Soup"),
    ("Palos", "Sticks"),
    ("Piedra", "Stone"),
    ("Antorcha azul", "Blue torch"),
    ("Antorcha verde", "Green torch"),
    ("Antorcha violeta", "Violet torch"),
    ("Antorcha", "Torch"),
    ("Madera", "Wood"),
    ("Extractor de madera", "Wood extractor"),
    ("Extractor de piedra", "Stone extractor"),
    ("Plataforma de lanzamiento", "Launch pad"),
    ("Perforadora de regolito", "Regolith drill"),
    ("Antena", "Antenna"),
    ("Hábitat de colonos", "Settler habitat"),
    ("Piso de madera", "Wood floor"),
    ("Piso de piedra", "Stone floor"),
    ("Pared de madera", "Wood wall"),
    ("Pared de piedra", "Stone wall"),
    ("Puerta de madera", "Wood door"),
    ("Puerta de piedra", "Stone door"),
    ("Ventana de madera", "Wood window"),
    ("Ventana de piedra", "Stone window"),
    ("Vano de madera (marco)", "Wood doorway (frame)"),
    ("Vano de piedra (marco)", "Stone doorway (frame)"),
    ("Pared de local", "Shopfront wall"),
    ("Vano de local", "Shopfront doorway"),
    ("EN PRUEBAS CON LEO", "TESTING WITH LEO"),
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn traduce_exacto_por_tramos_y_por_frase() {
        assert_eq!(traducir("JUGAR"), "PLAY");
        assert_eq!(traducir("Mantené para picar"), "Hold to mine");
        assert_eq!(traducir("Compañero  |  te sigue"), "Companion  |  follows you");
        assert_eq!(traducir("página 2 de 5"), "page 2 of 5");
        // Lo desconocido queda en castellano, no roto.
        assert_eq!(traducir("Doña Rosa"), "Doña Rosa");
    }

    #[test]
    fn no_hay_claves_repetidas_con_traducciones_distintas() {
        let mut m: HashMap<&str, &str> = HashMap::new();
        for (es, en) in FRASES {
            if let Some(prev) = m.insert(es, en) {
                assert_eq!(prev, *en, "clave repetida con otra traducción: {es}");
            }
        }
    }

    #[test]
    fn las_frases_cortas_no_rompen_palabras() {
        // Las de menos de 5 letras sólo valen como texto entero.
        assert_eq!(traducir("Dedo de la mano"), "Dedo de la mano");
        assert_eq!(traducir("desde"), "desde");
    }
}
