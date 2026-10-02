//! 2D sky — day/night clear color, night overlay, starfield in vacuum.
//!
//! Replaces the Ghibli dome + fog + starfield shells: on Earth the clear
//! color runs a one-hour day cycle; airless bodies and space render near-black
//! with camera-following star sprites (never a crossable shell — biblia §1).

use bevy::prelude::*;

use crate::camera::GameCamera;
use crate::scene::{RegionId, RegionRuntime};
use crate::sim::{Player, SimState};
use crate::transition::FlightTransition;

/// Cuánto dura un día entero, en segundos reales.
///
/// 30 jul 2026: 720 → 1200 → **3600 (1 hora)**. El ciclo viejo daba 8 min de luz
/// y 4 de noche: alcanzaba para ver el efecto, no para VIVIR una jornada — te
/// encontrabas la noche antes de terminar de cortar un árbol. Estirarlo no cambia
/// ninguna curva (todo lo demás trabaja en FASE 0..1): sólo reparte la hora en
/// **25 min de día, 25 de noche, 5 de amanecer y 5 de atardecer** (ver
/// [`DAWN_START`] y compañía), que es donde una salida a explorar —ir, hacer algo
/// y volver— entra entera en una parte del día.
pub const DAY_SECS: f32 = 3600.0;

/// **Los cuatro tramos del día, en FASE** (0 = medianoche, 0.5 = mediodía). Están
/// acá y no repartidos por el archivo porque son UN reparto: el sol sale en
/// [`DAWN_START`], se pone en [`NIGHT_START`], y entre medio la curva de
/// [`DayClock::daylight`] sube y baja justo en los tramos de crepúsculo. Tocar uno
/// solo desincroniza el arco de la luz, las sombras y el velo.
///
/// Con [`DAY_SECS`] = 3600 s los minutos reales caen así:
/// noche 47:30→12:30 · amanecer 12:30→17:30 · día 17:30→42:30 · atardecer 42:30→47:30.
pub const DAWN_START: f32 = 12.5 / 60.0;
/// Fin del amanecer: de acá a [`DUSK_START`] es día pleno (25 min).
pub const DAY_START: f32 = 17.5 / 60.0;
/// Empieza el atardecer (5 min).
pub const DUSK_START: f32 = 42.5 / 60.0;
/// Cae la noche (25 min, cruzando la medianoche).
pub const NIGHT_START: f32 = 47.5 / 60.0;

/// Días de juego que tarda la luna en completar su ciclo (nueva → llena →
/// nueva). Ocho es corto de verdad, y ese es el punto: en una sesión larga se
/// nota que hay **noches con luna y noches sin ella**, que es lo que le da valor
/// a llevar una antorcha.
const LUNAR_DAYS: f32 = 8.0;

/// Opacidad máxima del velo nocturno. Alta a propósito: de noche apenas se ve
/// por dónde vas, y por eso una farola o una fogata (ver `light2d`) valen algo.
///
/// La "niebla negra densa" NO se arregla bajando esto —bajarlo da atardecer
/// permanente, no noche—. Se arregla con el COLOR del velo: ver `sync_sky`.
///
/// 29 jul 2026: 0.965 → 0.985 → 0.992 → **0.978**. El CONTRASTE de una luz es
/// `pozo / velo`, así que este número manda más que cualquier ajuste de las
/// luces. Bajó de 0.992 al pasar la sombra ambiental de negro puro al **verde
/// azulado de la referencia**: con un color de verdad en el velo hace falta
/// dejar pasar algo más de albedo, o el término constante aplasta la variación
/// del mundo y vuelve la "niebla plana" (ver `sync_sky`).
///
/// **Este número no servía de nada hasta hoy**: la linterna multiplicaba
/// `night_alpha` por 0.45 y el flag `lit` nunca se apagaba, así que la noche real
/// corría a α≈0.44 pasara lo que pasara acá. Ver el bloque de la linterna abajo.
const NIGHT_VEIL: f32 = 0.978;

/// **La sombra de cada mundo tiene su color.** Es el término SÓLIDO del velo, en
/// lineal (el espacio en el que mezcla el shader), y es lo que decide de qué
/// color es "no estar iluminado" en ese cuerpo. Vale más que cualquier tinte de
/// lámpara: la mitad de que una antorcha se sienta acogedora es el contraste
/// contra el color de su sombra, así que cambiar esta línea cambia el ánimo del
/// planeta entero.
///
/// Los valores son bajísimos porque se pintan a α ≈ 0.98: 0.03 lineal ya es un
/// 19% en pantalla. Subirlos lava el mundo y vuelve la "niebla plana".
fn body_night_veil(body: ha_shared::CelestialBody) -> Vec3 {
    use ha_shared::CelestialBody as B;
    match body {
        // Navy oscuro: noche azul de planeta con aire y agua. Es el único cuerpo
        // donde el cielo mismo aporta color a la sombra.
        B::Earth => Vec3::new(0.0035, 0.0062, 0.0310),
        // Gris azulado frío y un punto más claro que la Tierra: sin atmósfera no
        // hay cielo que tiña nada, pero la Tierra llena de luz cenicienta el
        // regolito (el "earthshine" que hace que las fotos Apolo no sean negras).
        B::Moon => Vec3::new(0.0080, 0.0103, 0.0194),
        // Óxido. La sombra marciana no es negra: el polvo suspendido devuelve rojo.
        B::Mars => Vec3::new(0.0273, 0.0080, 0.0061),
        // Ocre de azufre, y CLARA: bajo esa atmósfera nunca hay noche cerrada,
        // hay una penumbra amarillenta permanente.
        B::Venus => Vec3::new(0.0344, 0.0194, 0.0044),
        // Roca pelada al sol desnudo: sombra dura, casi sin color.
        B::Mercury => Vec3::new(0.0050, 0.0048, 0.0052),
        // Azufre volcánico — verde amarillento enfermizo.
        B::Io => Vec3::new(0.0230, 0.0210, 0.0048),
        // Hielo: cian pálido, la sombra más fría del sistema.
        B::Europa => Vec3::new(0.0062, 0.0170, 0.0230),
        // Neblina de metano: naranja apagado y difuso.
        B::Titan => Vec3::new(0.0250, 0.0140, 0.0060),
        // Gigantes de hielo: azul verdoso profundo.
        B::Uranus | B::Neptune => Vec3::new(0.0040, 0.0150, 0.0230),
        // Lunas de Marte y el Sol: rocas sin nada que las tiña.
        B::Phobos | B::Deimos | B::Sun => Vec3::new(0.0060, 0.0055, 0.0050),
    }
}

/// Cuánto crepúsculo hay (0..1): pico en el MEDIO del amanecer y del atardecer.
/// Es la curva que tiñe cielo y velo de naranja/violeta — la "vibra lofi" del
/// atardecer sale de acá.
///
/// Los centros salen de los tramos ([`DAWN_START`]…): así el tinte cálido pega
/// exactamente cuando el sol está rasante, y no a destiempo. El ancho es un pelo
/// mayor que el tramo (5 min = 0.083 de fase) a propósito: el color del cielo
/// empieza a virar un poco ANTES de que baje la luz y termina de irse un poco
/// después, que es como se lee un atardecer de verdad.
pub fn dusk_amount(phase: f32) -> f32 {
    let bump = |c: f32, w: f32| {
        let t = ((phase - c).abs() / w).min(1.0);
        1.0 - t * t * (3.0 - 2.0 * t) // smoothstep invertido: 1 en el centro
    };
    let w = (DAY_START - DAWN_START) * 1.05;
    bump((DAWN_START + DAY_START) * 0.5, w).max(bump((DUSK_START + NIGHT_START) * 0.5, w))
}

/// Creative ascent fade band (biblia §11): sky darkens into space.
const SKY_FADE_START_M: f32 = 150.0;
const SKY_FADE_END_M: f32 = 1200.0;

#[derive(Resource)]
pub struct DayClock {
    pub t: f32,
}

impl Default for DayClock {
    fn default() -> Self {
        // Start mid-morning. `HA_DAYPHASE=<0..1>` arranca en otra hora (0 =
        // medianoche, 0.5 = mediodía) — así se pueden probar/capturar la noche y
        // el sistema de luces (`light2d`) sin esperar la hora entera del ciclo.
        // La PARTE ENTERA es el día: `HA_DAYPHASE=2.9` arranca la noche del tercer
        // día, que es la única forma de ver una luna que no sea la nueva del día 0
        // (ver `moon_illum`) sin jugar media hora.
        let phase = crate::flags::var("HA_DAYPHASE")
            .ok()
            .and_then(|v| v.parse::<f32>().ok())
            .map(|p| p.max(0.0))
            .unwrap_or(0.28);
        Self { t: DAY_SECS * phase }
    }
}

impl DayClock {
    /// 0..1 phase of the day (0 = midnight).
    pub fn phase(&self) -> f32 {
        (self.t / DAY_SECS).fract()
    }

    /// Hora del reloj de juego `(hora, minuto)` — fase 0 = 00:00, 0.5 = 12:00.
    /// Lo usa el reloj del HUD; el ciclo entero dura `DAY_SECS`.
    pub fn clock_hm(&self) -> (u32, u32) {
        let mins = (self.phase() * 24.0 * 60.0) as u32 % 1440;
        (mins / 60, mins % 60)
    }

    /// Nombre ASCII del tramo del día (el HUD no dibuja acentos —
    /// memoria `game-font-ascii-only`).
    pub fn phase_name(&self) -> &'static str {
        let p = self.phase();
        match p {
            _ if p < DAWN_START => "NOCHE",
            _ if p < DAY_START => "AMANECER",
            _ if p < DUSK_START => "DIA",
            _ if p < NIGHT_START => "ATARDECER",
            _ => "NOCHE",
        }
    }

    /// 0 = noche cerrada, 1 = día pleno. Las rampas son EXACTAMENTE los tramos de
    /// crepúsculo (5 min cada uno): a las 12:30 arranca a subir, a las 17:30 está
    /// a pleno, y baja igual del otro lado. Todo lo que reacciona al día
    /// (luciérnagas, velo, luces, fauna) cuelga de este número.
    pub fn daylight(&self) -> f32 {
        let p = self.phase();
        let up = ((p - DAWN_START) / (DAY_START - DAWN_START)).clamp(0.0, 1.0);
        let down = ((NIGHT_START - p) / (NIGHT_START - DUSK_START)).clamp(0.0, 1.0);
        up.min(down)
    }

    /// Qué día de juego va corriendo (0, 1, 2…). Lo usa la fase lunar.
    pub fn day_index(&self) -> f32 {
        (self.t / DAY_SECS).floor()
    }

    /// Fracción iluminada de la luna esta noche (0 = luna nueva → noche cerrada,
    /// 1 = llena). Cambia de una noche a la otra, nunca dentro de la misma.
    pub fn moon_illum(&self) -> f32 {
        // El +3 corre el ciclo para que la PRIMERA noche de una partida tenga
        // luna casi llena. Con el ciclo a pelo, el día 0 caía en luna nueva: la
        // primera noche del juego era la única sin luna, y eso se lee como que la
        // luna no existe, no como una noche especial. La luna nueva llega al
        // quinto día, cuando ya se entendió que hay ciclo.
        let k = (self.day_index() + 3.0) / LUNAR_DAYS;
        0.5 - 0.5 * (k * std::f32::consts::TAU).cos()
    }
}

/// **La luz que viene del cielo**: el sol de día, la luna de noche (si hay). Es
/// la fuente que decide HACIA DÓNDE caen las sombras del mundo — por eso vive en
/// un recurso y no dentro del dibujo del disco.
///
/// `dir` está en coordenadas de PANTALLA y apunta HACIA la fuente: el sol sale
/// por la derecha (este), cruza por arriba al mediodía y se pone por la
/// izquierda. La sombra de cualquier objeto es exactamente `-dir`, y su largo
/// sale de `elevation` (rasante al amanecer, cortita al mediodía).
#[derive(Resource, Clone, Copy)]
pub struct SkyLight {
    pub dir: Vec2,
    /// 0 = en el horizonte, 1 = en el cenit.
    pub elevation: f32,
    /// Cuánta luz DIRECCIONAL tira (0..1). La tapan las nubes: un día nublado no
    /// proyecta sombras, y esa es justo la diferencia que se ve.
    pub strength: f32,
    /// Color de esa luz (cálido al ras, blanco al mediodía, azul de luna). Lo lee
    /// `light2d::cast_shadows` para teñir la sombra con el complementario.
    pub color: Vec3,
}

impl Default for SkyLight {
    fn default() -> Self {
        Self {
            dir: Vec2::new(0.0, 1.0),
            elevation: 1.0,
            strength: 1.0,
            color: Vec3::new(1.0, 0.97, 0.9),
        }
    }
}

/// Arco de un astro: `t` 0 = sale (por el este/derecha), 0.5 = cenit, 1 = se pone.
/// Devuelve `(dirección en pantalla, elevación 0..1)`.
fn arc(t: f32) -> (Vec2, f32) {
    let th = std::f32::consts::PI * t.clamp(0.0, 1.0);
    (Vec2::new(th.cos(), th.sin().max(0.0)), th.sin().max(0.0))
}

/// Kept for HUD compat: lantern state (crafted light) — dims the night overlay.
#[derive(Resource, Default)]
pub struct PlayerLanternControl {
    pub equipped: bool,
    pub lit: bool,
}

#[derive(Component)]
struct NightOverlay;

#[derive(Component)]
struct StarSprite;

/// Sitio y tamaño de una estrella en unidades de PANTALLA (1.0 = alto del
/// viewport). El cielo se re-escala al encuadre vivo, así que llena la pantalla
/// tanto a 30 m de zoom como a 100 km.
#[derive(Component)]
struct StarField {
    unit: Vec2,
    size: f32,
    /// Brillo propio de la estrella; `sync_sky` lo multiplica por el fundido del
    /// campo (así el cielo entra y sale sin que todas queden del mismo brillo).
    alpha: f32,
    /// PARALAJE: cuánto arrastra la cámara a esta estrella. Las brillantes ("cerca")
    /// se mueven más que las tenues ("lejos"), así el cielo tiene profundidad cuando
    /// caminás en vez de ser una calcomanía pegada al visor. El campo se envuelve
    /// (`rem_euclid`), así que nunca se agota por mucho que andes.
    plx: f32,
}

/// Camera-following cover layer for every ascent/descent (rocket AND creative)
/// — hides the ground (and the surface↔orbit swap) mid-atmosphere, clears into
/// space. Alpha + tint are driven by `FlightTransition` (clouds on bodies with
/// an atmosphere, a brief dark fade on airless ones). Drop a `decor/clouds`
/// texture later for real clouds.
#[derive(Component)]
pub struct CloudOverlay;

// **NO se dibuja ningún astro, y es a propósito.** Hubo un disco de sol y otro de
// luna (halo + cuerpo, colgados de la cámara como las estrellas) y los dos se
// sacaron: esto es una vista CENITAL, así que un disco en pantalla no queda "allá
// arriba en el cielo" — queda como un manchón blanco flotando sobre los techos,
// justo delante de donde estás mirando. Fue el reporte, textual: "no se tiene que
// ver ese sol, sólo se tienen que ver las sombras".
//
// El astro se lee por lo que HACE, que es más información y no menos: el color del
// cielo (`sync_sky`), el brillo del mundo, el tinte de la luz y sobre todo la
// SOMBRA — hacia dónde cae, cuánto mide, de qué color es (`SkyLight` →
// `light2d::cast_shadows`). La fase de la luna sigue existiendo y sigue decidiendo
// cuánta luz tenés esta noche; lo único que no hay es su calcomanía.
//
// Si algún día hace falta volver a ponerlos: el patrón está en el historial de
// esta rama (componente `SkyDisc` + sistema `sync_sky_discs`), pero lo correcto en
// cenital es un icono de HUD, no un quad en el mundo.

pub struct Sky2dPlugin;

impl Plugin for Sky2dPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(bevy::prelude::OnExit(crate::game_phase::GamePhase::Playing), (crate::salir::reiniciar::<SkyAttached>,));
        app.init_resource::<DayClock>()
            .init_resource::<PlayerLanternControl>()
            .init_resource::<SkyLight>()
            .init_resource::<SkyAttached>()
            .add_systems(Startup, spawn_cloud_overlay)
            // Sólo en partida: en splash/menú el cielo no tiene a quién pintarle
            // el día (y `sync_sky` es quien escribe `ClearColor`, que en el
            // arranque tiene que quedarse en el void del menú — ver `boot_ui`).
            .add_systems(
                Update,
                (
                    spawn_sky_attachments,
                    advance_day_clock,
                    // El astro se calcula ANTES de pintar el cielo (que lo usa
                    // para el color del sol bajo) y antes de las sombras.
                    sync_sky_light,
                    sync_sky,
                    sync_cloud_overlay,
                )
                    .chain()
                    .run_if(crate::game_phase::world_active),
            );
    }
}

fn advance_day_clock(time: Res<Time>, mut clock: ResMut<DayClock>) {
    clock.t += time.delta_secs();
}

/// El velo de noche y las estrellas son HIJOS de la cámara. Antes se creaban en
/// `Startup`, donde el orden contra `camera::spawn_camera` (otro `Startup`, de
/// otro plugin) NO está definido: si el cielo corría primero la query salía
/// vacía y **el velo de noche no se creaba nunca** — la noche no oscurecía y las
/// estrellas no aparecían. Ahora se intenta cada frame hasta que la cámara
/// existe, y recién ahí se marca hecho.
#[derive(Resource, Default)]
struct SkyAttached(bool);

fn spawn_sky_attachments(
    mut commands: Commands,
    mut done: ResMut<SkyAttached>,
    camera: Query<Entity, With<GameCamera>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<crate::light2d::NightVeilMaterial>>,
    mut add_materials: ResMut<Assets<crate::light2d::LightAddMaterial>>,
) {
    if done.0 {
        return;
    }
    let Ok(cam) = camera.get_single() else {
        return;
    };
    done.0 = true;
    let veil = materials.add(crate::light2d::NightVeilMaterial {
        uniform: crate::light2d::NightVeilUniform::default(),
    });
    commands.insert_resource(crate::light2d::NightVeilMat(veil.clone()));
    let light_add = add_materials.add(crate::light2d::LightAddMaterial {
        uniform: crate::light2d::NightVeilUniform::default(),
    });
    commands.insert_resource(crate::light2d::LightAddMat(light_add.clone()));
    commands.entity(cam).with_children(|p| {
        // Las DOS pasadas de iluminación, delante del mundo y detrás de la UI
        // (ver `light2d` + `night_veil.wgsl`). El ORDEN importa y sale del z:
        //
        //   z 40 → SOMBRA, blend multiplicativo: `mundo × sombra`.
        //   z 41 → LUZ, blend aditivo: `+ fotones`.
        //
        // Primero oscurecer y después sumar, que es como funciona la realidad. Al
        // revés (sumar y luego multiplicar) la luz de las lámparas se oscurecería
        // con la noche y no se vería nada.
        p.spawn((
            NightOverlay,
            crate::light2d::LightPass::Shade,
            Mesh2d(meshes.add(Rectangle::new(4.0, 2.4))),
            MeshMaterial2d(veil),
            Transform::from_xyz(0.0, 0.0, 40.0),
        ));
        p.spawn((
            NightOverlay,
            crate::light2d::LightPass::Add,
            Mesh2d(meshes.add(Rectangle::new(4.0, 2.4))),
            MeshMaterial2d(light_add),
            Transform::from_xyz(0.0, 0.0, 41.0),
        ));
        // Static star pattern that follows the camera (skybox, never crossed).
        // Denser field with varied size/brightness/tint for parallax-less depth;
        // all flat quads → cost is trivial even at a few hundred.
        let mut seed = 0x1357_9bdfu32;
        for i in 0..900 {
            seed ^= seed << 13;
            seed ^= seed >> 17;
            seed ^= seed << 5;
            let x = ((seed & 0xffff) as f32 / 65535.0 - 0.5) * 2.4;
            let y = (((seed >> 16) & 0xffff) as f32 / 65535.0 - 0.5) * 1.5;
            // A few bright "near" stars, most faint "far" ones.
            let bright = i % 11 == 0;
            // Tamaño en FRACCIÓN de la altura del viewport → píxeles constantes a
            // cualquier zoom. Medido en el teléfono (1080 de alto): con 0.007-0.013 la
            // estrella salía de 8 a 14 px, o sea CONFETI, no cielo. Una estrella tiene
            // que ser un punto: 1-3 px la tenue, 3-5 px la brillante.
            let s = if bright { 0.0028 + (seed % 4) as f32 * 0.0006 } else { 0.0012 + (seed % 5) as f32 * 0.0004 };
            let a = if bright { 0.95 } else { 0.45 + (seed % 6) as f32 * 0.07 };
            // Subtle warm/cool tint variation.
            let (r, g, b) = match seed % 5 {
                0 => (1.0, 0.92, 0.85),
                1 => (0.85, 0.9, 1.0),
                _ => (0.96, 0.96, 1.0),
            };
            // Posición y tamaño se guardan NORMALIZADOS (fracción de la altura
            // del viewport) y `sync_sky` los multiplica por el encuadre vivo.
            // Antes sólo se escalaba el TAMAÑO: las estrellas se quedaban
            // dentro de ±1.2 m de la cámara, así que en el espacio (encuadre de
            // decenas de km) el cielo entero era UN PUNTO de 2 px en el centro
            // de la pantalla. De ahí el "no se ve nada" ahí arriba.
            // Paralaje por "distancia": la estrella brillante está más cerca y se
            // desplaza más. Son valores chicos a propósito — el cielo tiene que
            // acompañar el paseo, no salir corriendo.
            let plx = if bright { 0.0035 + (seed % 7) as f32 * 0.0004 } else { 0.0008 + (seed % 5) as f32 * 0.0002 };
            p.spawn((
                StarSprite,
                // Una estrella ES luz: `UnlitSprite` la saca de `light2d::light_sprites`,
                // que si no le multiplica el color por la luz del sitio y de noche —justo
                // cuando tiene que verse— la deja negra. Ése era el motivo real de "no
                // tenía las estrellas": se dibujaban, pero apagadas.
                crate::sim::UnlitSprite,
                StarField { unit: Vec2::new(x, y), size: s, alpha: a, plx },
                Sprite::from_color(Color::srgba(r, g, b, 0.0), Vec2::ONE),
                // DETRÁS de todo el mundo (los discos planetarios están en
                // z ≈ −20). Antes iban en +30, o sea delante del suelo: pisando
                // la Luna (cielo siempre "vacío") las estrellas se dibujaban
                // ENCIMA del regolito, y en órbita salpicaban el planeta.
                Transform::from_xyz(0.0, 0.0, -400.0),
                Visibility::Hidden,
            ));
        }
    });
}

/// Dónde está el astro AHORA y cuánta luz tira. Es la única fuente de verdad de
/// la dirección de las sombras (`light2d::cast_shadows`).
fn sync_sky_light(
    clock: Res<DayClock>,
    weather: Res<crate::weather::Weather>,
    region: Res<RegionRuntime>,
    tiles: Res<crate::world2d::TileWorld>,
    world_ctx: Res<crate::scene::WorldContextRuntime>,
    planet: Res<crate::world2d::ActivePlanet>,
    mut sky: ResMut<SkyLight>,
) {
    let p = clock.phase();
    let body = crate::world2d::active_surface_body(&world_ctx, &planet);
    let airless = ha_shared::transition_profile(body).sky_fade_end <= 0.0;
    // Sólo el espacio/vacío apaga el sol: estar en tu túnel no borra el cielo
    // del mundo que tenés encima (§4 — es el mismo sitio).
    let underground = crate::world2d::surface_region_active(&region).is_none();
    // Bajo tierra no entra ni el sol ni la luna: ahí la única luz es la que
    // llevás puesta, y sin fuente direccional las sombras se quedan bajo los pies.
    if underground {
        *sky = SkyLight { strength: 0.0, ..default() };
        return;
    }
    // Sin aire no hay nubes que tapen nada: el sol pega desnudo (sombras duras),
    // que es exactamente el aspecto de las fotos de la Luna.
    let cloud = if airless { 1.0 } else { weather.direct_mul() };

    // Sol: sale con el amanecer y se pone cuando cae la noche. **No se DIBUJA**
    // (ver el comentario de arriba, donde estaba `SkyDisc`): lo único que existe de
    // él es esta dirección — o sea, sus sombras.
    if (DAWN_START..NIGHT_START).contains(&p) {
        let (dir, elev) = arc((p - DAWN_START) / (NIGHT_START - DAWN_START));
        // Del naranja rasante al blanco del mediodía.
        let low = Vec3::new(1.0, 0.66, 0.38);
        let high = Vec3::new(1.0, 0.97, 0.90);
        *sky = SkyLight {
            dir,
            elevation: elev,
            strength: (0.25 + 0.75 * elev) * cloud,
            color: low.lerp(high, elev.powf(0.6)),
        };
        return;
    }
    // Luna: cruza la noche entera (`NIGHT_START` → `DAWN_START` pasando por
    // medianoche). Si está nueva no hay nadie arriba y la noche es de las antorchas.
    let night_span = 1.0 - (NIGHT_START - DAWN_START);
    let t = (p - NIGHT_START).rem_euclid(1.0) / night_span;
    let (dir, elev) = arc(t);
    let illum = clock.moon_illum();
    *sky = SkyLight {
        dir,
        elevation: elev,
        // Una luna llena en el cenit es ~1/500 del sol; acá vale 0.2 porque lo
        // que hace es dar una DIRECCIÓN a las sombras nocturnas, no iluminar.
        strength: 0.20 * illum * elev * cloud,
        color: Vec3::new(0.60, 0.72, 1.0),
    };
}

#[allow(clippy::too_many_arguments, clippy::type_complexity)]
fn sync_sky(
    clock: Res<DayClock>,
    weather: Res<crate::weather::Weather>,
    region: Res<RegionRuntime>,
    trans: Res<FlightTransition>,
    tiles: Res<crate::world2d::TileWorld>,
    world_ctx: Res<crate::scene::WorldContextRuntime>,
    planet: Res<crate::world2d::ActivePlanet>,
    mut clear: ResMut<ClearColor>,
    player: Query<&SimState, With<Player>>,
    camera: Query<(&OrthographicProjection, &GlobalTransform), With<GameCamera>>,
    mut overlay: Query<&mut Transform, (With<NightOverlay>, Without<StarSprite>)>,
    veil_mat: Option<Res<crate::light2d::NightVeilMat>>,
    mut veil_assets: ResMut<Assets<crate::light2d::NightVeilMaterial>>,
    mut stars: Query<
        (&StarField, &mut Visibility, &mut Transform, &mut Sprite),
        (With<StarSprite>, Without<NightOverlay>),
    >,
    time: Res<Time>,
    mut velo_suave: Local<Option<Vec3>>,
) {
    let cam_pos = camera
        .get_single()
        .map(|(_, gt)| gt.translation().truncate())
        .unwrap_or(Vec2::ZERO);
    let view_h = camera
        .get_single()
        .ok()
        .map(|(p, _)| match p.scaling_mode {
            bevy::render::camera::ScalingMode::FixedVertical { viewport_height } => viewport_height,
            _ => 30.0,
        })
        .unwrap_or(30.0);

    // How "spacey" is the view? 0 = surface sky by day, 1 = void. Bodies with
    // an atmosphere get their own tinted sky (from the transition profile);
    // airless surfaces are void + stars, as always.
    let mut space_f: f32;
    let mut body_sky: Option<Vec3> = None;
    let day = clock.daylight();
    match region.active {
        RegionId::Overworld { .. } | RegionId::Cave(_) => {
            space_f = 0.0;
            // Walking/flying high on Earth fades the atmosphere out (§11).
            if let Ok(sim) = player.get_single() {
                let h = sim.body.y;
                space_f = ((h - SKY_FADE_START_M) / (SKY_FADE_END_M - SKY_FADE_START_M))
                    .clamp(0.0, 1.0);
            }
            if trans.in_surface_frame() {
                space_f = space_f.max(trans.sky_blend.clamp(0.0, 1.0));
            }
        }
        RegionId::Luna | RegionId::Planet(_) => {
            let body = match region.active {
                RegionId::Planet(b) => b,
                _ => ha_shared::CelestialBody::Moon,
            };
            let p = ha_shared::transition_profile(body);
            if p.sky_fade_end > 0.0 {
                // Same on-foot fade band as Earth, scaled to this atmosphere,
                // so the sky you land under matches the descent you flew.
                let start = p.sky_fade_end * (SKY_FADE_START_M / 900.0);
                let end = p.sky_fade_end * (SKY_FADE_END_M / 900.0);
                space_f = player
                    .get_single()
                    .map(|sim| ((sim.body.y - start) / (end - start)).clamp(0.0, 1.0))
                    .unwrap_or(0.0);
                if trans.in_surface_frame() {
                    space_f = space_f.max(trans.sky_blend.clamp(0.0, 1.0));
                }
                body_sky = Some(Vec3::from_array(p.cover_color) * 0.85);
            } else {
                space_f = 1.0;
            }
        }
        _ => space_f = 1.0,
    }

    // Paleta del cielo. La Tierra corre su ciclo con TRES tramos, no dos: el
    // ATARDECER (y el amanecer) tienen su propio color cálido, que es de donde
    // sale la vibra "lofi/mágica" — pasar de día a noche interpolando un solo
    // par daba un gris sucio. `dusk_amt` es 1 en el pico del crepúsculo.
    let day_sky = Vec3::new(0.45, 0.68, 0.85);
    let dusk_sky = Vec3::new(0.86, 0.44, 0.30); // naranja/rosa de atardecer
    let night_sky = Vec3::new(0.03, 0.035, 0.10); // azul profundo, no negro
    // Nublado: el cielo pierde el azul y baja de brillo, y con él se apaga el
    // crepúsculo (bajo una tormenta no hay atardecer rosa, hay plomo). Es el
    // MISMO `cover` que apaga el sol y las sombras — un solo número mueve todo.
    let cover = weather.cover;
    let dusk_amt = dusk_amount(clock.phase()) * (1.0 - cover * 0.85);
    let sky = body_sky.unwrap_or_else(|| {
        let base = night_sky.lerp(day_sky, day);
        let clear_sky = base.lerp(dusk_sky, dusk_amt * 0.85);
        // Gris de nube, oscurecido con la propia luz del día (de noche una nube
        // no es gris claro: es un techo negro).
        let overcast = Vec3::new(0.46, 0.48, 0.52) * (0.10 + 0.90 * day);
        clear_sky.lerp(overcast, cover * 0.85)
    });
    let void = Vec3::new(0.01, 0.01, 0.03);
    let rgb = sky.lerp(void, space_f);
    *clear = ClearColor(Color::srgb(rgb.x, rgb.y, rgb.z));

    // Underground (Z-Level < 0): near-black cave, lantern cuts the dark.
    let underground = tiles.z < 0;
    if underground {
        let cave = Vec3::new(0.03, 0.03, 0.05);
        *clear = ClearColor(Color::srgb(cave.x, cave.y, cave.z));
    }

    // Velo de noche. La noche es MUY oscura a propósito (apenas ves por dónde
    // vas → la luz vale algo y el mundo se siente mágico); el crepúsculo no
    // oscurece con gris sino con un violeta cálido que casa con el cielo
    // naranja. La linterna sigue abriendo el velo.
    // **La curva, no sólo el tope.** Con `(1 - day)` a secas el atardecer heredaba
    // casi toda la opacidad de la noche: a las 19:30 (`day` 0.2) el velo ya iba a
    // α 0.79 y, como el crepúsculo pinta violeta SÓLIDO encima, la pantalla
    // entera quedaba de un magenta plano — el mundo debajo no se veía. Elevarlo a
    // 3.5 deja el atardecer transparente (α≈0.45 a `day` 0.2) y hunde el último
    // tramo: la oscuridad llega DE GOLPE cuando cae el sol, que es lo que se
    // siente real, en vez de un gris que crece parejo toda la tarde.
    //
    // El nublado entra por acá y no por otra parte: `daylight_mul` baja el día
    // EFECTIVO, así que un cielo tapado oscurece el mundo y adelanta el encendido
    // de las luces (`light2d::darkness` usa el mismo factor) sin tocar ninguna de
    // estas curvas. Una tormenta de mediodía queda como un atardecer plomizo.
    let day_eff = day * weather.daylight_mul();
    let night_norm = (1.0 - day_eff).powf(3.5);
    let mut night_alpha = night_norm * NIGHT_VEIL * (1.0 - space_f);
    // Piso de penumbra del nublado. La curva de la noche está elevada a 3.5 (así
    // el atardecer queda transparente), y eso mismo hace que un mediodía tapado
    // apenas se oscurezca: 0.62 de día efectivo da α 0.18. Una tormenta tiene que
    // LEERSE como tormenta aunque sea mediodía, así que se le pone un mínimo.
    night_alpha = night_alpha.max(cover * 0.26 * (1.0 - space_f));
    // **Casi negro, y ese es el punto.** Es color LINEAL (el shader escribe a un
    // target sRGB): 0.010 lineal ya es ~9% en pantalla, y con α 0.965 ese término
    // constante aportaba MÁS que el albedo del mundo → todo quedaba del mismo
    // gris azulado, sin tono ni contraste: la "niebla negra densa". Con el velo
    // casi negro la mezcla es ~albedo × 0.035, o sea un OSCURECIMIENTO: igual de
    // noche, pero el pasto sigue siendo verde y la piedra gris, y por eso se
    // entiende lo que mirás. Subir estos números vuelve a lavar todo.
    // Bajado junto con `NIGHT_VEIL`: este término es color SÓLIDO (se pinta a
    // pelo, no multiplica), así que al subir α pasa a pesar más que el albedo que
    // deja pasar el velo. Con α 0.985 y el color viejo la mezcla volvía a ser
    // "todo del mismo azul": la mitad del brillo venía de acá y no del mundo.
    // **El color de la sombra, y es de cada cuerpo.** El principio salió de tu
    // referencia: las zonas oscuras de esa captura miden (7,38,33) / (20,41,34)
    // en sRGB — un color de verdad, nunca negro. La mitad de lo "acogedor" de esa
    // imagen es contraste complementario: fuego cálido contra sombra fría. Con la
    // sombra negra el mismo fuego se lee como una linterna sobre carbón.
    //
    // Lo que cambia por planeta es el matiz (ver `body_night_veil`): la Tierra su
    // navy, Marte su óxido, Europa su hielo. El shader mezcla en lineal.
    let mut veil = body_night_veil(crate::world2d::active_surface_body(&world_ctx, &planet));
    // El violeta del crepúsculo es LUZ QUE QUEDA EN EL CIELO, así que tiene que
    // apagarse cuando ya no queda ninguna. `dusk_amount` sigue >0 hasta la fase
    // 0.90 — bien entrada la noche cerrada—, y ahí el velo ya va a α 0.99: el
    // tinte, que es color SÓLIDO, se pintaba a plena opacidad sobre un mundo
    // negro y el resultado era una sábana magenta lisa, sin mundo debajo.
    // Ponderarlo por el día que sobra lo deja donde se ve bonito (el sol
    // bajando) y deja la noche cerrada NEUTRA, que es lo que hace que una
    // antorcha se lea naranja de verdad.
    // Penumbra de tormenta: PLOMO, no la noche azul del planeta. Es la diferencia
    // entre "se hizo de noche a las tres de la tarde" (raro) y "se puso feo".
    veil = veil.lerp(Vec3::new(0.0170, 0.0185, 0.0220), (cover * day).clamp(0.0, 1.0) * 0.85);
    let dusk_paint = dusk_amt * (1.0 - night_norm);
    if dusk_paint > 0.0 {
        // Bajado 3× (era 0.22,0.08,0.18): incluso ponderado, a esa saturación se
        // leía como un baño rosa. Ahora es un tinte en el aire — el pasto sigue
        // verde debajo.
        veil = veil.lerp(Vec3::new(0.075, 0.027, 0.060), dusk_paint);
    }
    if underground {
        // La cueva es más cerrada que la noche, pero por el mismo mecanismo:
        // más α, no más color (si no, vuelve la niebla plana).
        night_alpha = 0.992;
        // El matiz del fondo es el del BIOMA donde estás (`ha_shared::subsuelo`), y cambia de a poco
        // al cruzar de uno a otro (nada de saltos de color).
        let meta = player
            .get_single()
            .map(|s| {
                let b = crate::world2d::active_surface_body(&world_ctx, &planet);
                let v = ha_shared::subsuelo::bioma_en(b, s.body.x.floor() as i64, s.body.z.floor() as i64).velo();
                Vec3::new(v[0], v[1], v[2])
            })
            .unwrap_or(Vec3::new(0.0008, 0.0038, 0.0034));
        let actual = velo_suave.unwrap_or(meta);
        let nuevo = actual.lerp(meta, (time.delta_secs() * 1.5).min(1.0));
        *velo_suave = Some(nuevo);
        veil = nuevo;
    } else {
        *velo_suave = None;
    }
    // La linterna **NO** aclara la pantalla entera. Antes hacía
    // `night_alpha *= 0.45`, o sea: llevar una antorcha subía el brillo del mundo
    // hasta el horizonte, incluso a tus espaldas. Doble contabilidad, además: el
    // ítem en la mano YA es un `Light2d` real (`held_item::item_light`) que
    // agujerea el velo y tiñe los sprites a su alrededor. Con el multiplicador
    // global el velo corría a α 0.443 en vez de 0.985 — la noche entera valía la
    // mitad y por eso "no se notaba de dónde alumbraba" nada: el charco del farol
    // competía contra un mundo ya medio iluminado.
    //
    // El recurso sigue existiendo porque el botón LAMP decide si el ítem emite;
    // lo que se fue es el atajo que saltaba el sistema de luces.
    if crate::flags::var("HA_PERFLOG").is_ok() {
        // Caza del parpadeo (1 oct 2026): un cuadro de pantalla "de día" en plena noche.
        static ULTIMO: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
        let antes = f32::from_bits(ULTIMO.swap(night_alpha.to_bits(), std::sync::atomic::Ordering::Relaxed));
        if (night_alpha - antes).abs() > 0.15 {
            eprintln!(
                "ha-veil: salto de alfa {antes:.3} -> {night_alpha:.3} space_f={space_f:.2} day={day:.2} cover={cover:.2} sky_blend={:.2} region={:?}",
                trans.sky_blend, region.active
            );
        }
    }
    // Sólo se escribe si CAMBIÓ: `Assets::get_mut` marca el material como modificado y obliga
    // al render a rehacer su bind group, a 60 veces por segundo (y es de donde sospechamos el
    // cuadro de pantalla "de día" en plena noche).
    let color_velo = Vec4::new(veil.x, veil.y, veil.z, night_alpha);
    if let Some(vh) = veil_mat.as_ref() {
        if veil_assets.get(&vh.0).is_some_and(|m| m.uniform.color != color_velo) {
            if let Some(mat) = veil_assets.get_mut(&vh.0) {
                mat.uniform.color = color_velo;
            }
        }
    }
    for mut tf in &mut overlay {
        // Cover the viewport at any zoom.
        tf.scale = Vec3::splat(view_h * 1.6);
    }

    // Estrellas en el vacío / de noche a gran altura. Posición Y tamaño se
    // escalan al encuadre: el campo cubre siempre la pantalla entera.
    //
    // La visibilidad es un FUNDIDO, no un interruptor. Con un umbral duro
    // (`space_f > 0.55`) bastaba quedarse flotando justo en la banda para que
    // `space_f` cruzara el corte ida y vuelta cada frame: las 320 estrellas
    // aparecían y desaparecían: EL parpadeo del vuelo/espacio. Un fundido no
    // puede oscilar porque no tiene borde donde oscilar.
    // LAS ESTRELLAS SON DEL ESPACIO. Hubo un intento de traerlas a ras del suelo de
    // noche y el resultado, dicho por el usuario, fue que "parecía nieve" — y con
    // razón: en cenital no hay cielo en pantalla, hay suelo, así que puntos blancos
    // sobre el paisaje se leen como copos, no como estrellas. La idea se quedó, pero
    // donde corresponde: `weather.rs` la convirtió en nieve de verdad (con manto que
    // se acumula). Acá el campo vuelve a atarse a la ALTURA.
    let stars_f = crate::universe::smootherstep(0.30, 0.70, space_f)
        .max(if day < 0.25 { crate::universe::smootherstep(0.12, 0.35, space_f) } else { 0.0 });
    // Envoltura del campo: el paralaje desplaza cada estrella y al salirse por un
    // borde reaparece por el otro, así el cielo es infinito sin más quads.
    let (span_x, span_y) = (2.4 * view_h, 1.5 * view_h);
    for (field, mut vis, mut tf, mut sprite) in &mut stars {
        let want = if stars_f > 0.002 { Visibility::Visible } else { Visibility::Hidden };
        if *vis != want {
            *vis = want;
        }
        if stars_f <= 0.002 {
            continue;
        }
        let wrap = |v: f32, span: f32| (v + span * 0.5).rem_euclid(span) - span * 0.5;
        tf.translation.x = wrap(field.unit.x * view_h - cam_pos.x * field.plx, span_x);
        tf.translation.y = wrap(field.unit.y * view_h - cam_pos.y * field.plx, span_y);
        sprite.custom_size = Some(Vec2::splat((field.size * view_h).max(0.001)));
        sprite.color.set_alpha(field.alpha * stars_f);
    }
}

/// Full-screen UI overlay for the ascent/descent cover. A UI Node (renders
/// reliably on top, unlike the old camera-child sprite which never drew).
fn spawn_cloud_overlay(mut commands: Commands) {
    commands.spawn((
        CloudOverlay,
        Node {
            position_type: PositionType::Absolute,
            left: Val::Px(0.0),
            top: Val::Px(0.0),
            width: Val::Percent(100.0),
            height: Val::Percent(100.0),
            ..default()
        },
        BackgroundColor(Color::srgba(0.86, 0.89, 0.95, 0.0)),
        // Negative z: above the 2D world (UI pass always is) but BELOW the HUD so
        // the clouds cover the ground, never the hearts/hotbar/buttons.
        GlobalZIndex(-50),
        bevy::ui::FocusPolicy::Pass,
        crate::game_ui::GameplayUi,
        Name::new("CloudOverlay"),
    ));
}

fn sync_cloud_overlay(
    trans: Res<FlightTransition>,
    mut q: Query<&mut BackgroundColor, With<CloudOverlay>>,
) {
    let [r, g, b] = trans.cover_color;
    for mut bg in &mut q {
        *bg = BackgroundColor(Color::srgba(r, g, b, trans.cover.clamp(0.0, 1.0)));
    }
}
