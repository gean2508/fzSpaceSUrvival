// Iluminación 2D en DOS PASADAS, las dos sobre el mismo uniform y la misma
// pantalla completa (hijas de la cámara, ver `sky2d::spawn_sky`):
//
//   1. `fragment`     — SOMBRA, con blending MULTIPLICATIVO (`dst * src`).
//   2. `fragment_add` — LUZ, con blending ADITIVO (`dst + src`).
//
// ## Por qué dos pasadas y no el velo alpha de antes
//
// El velo viejo era una sola capa alpha: `final = velo*α + mundo*(1-α)`. Ese
// primer término es COLOR SÓLIDO — no depende del mundo—, así que la noche
// entera venía con una sábana de color encima: a α 0.978 el 97% de lo que veías
// era el velo y el mundo asomaba un 2%. Eso es exactamente lo que se leía como
// "todo opaco, descolorido, un brillo blanquecino sobre las cosas": no era el
// halo de las luces, era el velo mismo aplastando el albedo.
//
// Multiplicar en vez de pintar arregla eso de raíz: `final = mundo × sombra`
// **conserva el albedo y la saturación** — la noche oscurece el pasto pero el
// pasto sigue siendo verde, la piedra gris, y por eso todo se ve NÍTIDO. Y la
// luz, que es lo que SUMA fotones, se suma (pasada 2): dos antorchas que se
// solapan dan más luz, nunca una mancha lechosa, y ninguna capa puede "tapar"
// lo que hay debajo porque sumar no destruye información.
//
// ## El gradiente
//
// Las dos pasadas se evalúan POR PÍXEL sobre la escena entera, sprites
// incluidos. Eso da gratis lo que un tinte por-sprite no puede: un árbol con una
// fogata a su izquierda recibe más luz en su mitad izquierda que en la derecha
// —la cara que da a la luz se ilumina más que la que no—, y el mismo árbol diez
// metros más lejos recibe mucho menos. La caída lleva un término
// inverso-cuadrático (`near`) justo para que ese gradiente se VEA en el pozo, en
// vez de un disco parejo con un borde.
//
// ## Y el borde sigue siendo suave
//
//   1. caída poly6 `(1-x²)³`: derivada CERO en los dos extremos, así que la luz
//      no "termina" en ningún píxel concreto,
//   2. la suma aditiva no tiene dónde hacer escalón (no hay `min`/`max` entre
//      luces, ni opacidades que se relevan),
//   3. dither de ±1 LSB: sin él, un degradé largo sobre 8 bits deja anillos
//      concéntricos que en el móvil se leen como "el borde de la luz".
// ## RECALIBRADO (jul 2026) — "la luz lava los colores"
//
// El reporte: con la linterna en la mano, la noche se veía como una **mancha
// marrón desaturada** que cubría media pantalla, con el centro casi blanco y sin
// contraste; el pixel art de debajo desaparecía. Cuatro causas, no una:
//
//  1. **El pozo era gigante** (`LIGHT_REACH` 2.6 × radio 5.4 = 14 m). Una luz que
//     cubre la pantalla no se lee como luz: se lee como haber subido el brillo,
//     porque no queda noche con la cual contrastar. Arreglado en `light2d.rs`.
//  2. **La noche era demasiado negra** (`NIGHT_FLOOR` 0.04). Contra un fondo casi
//     negro, CUALQUIER luz parece un foco quemado — el ojo compara. Subirla a
//     0.115 hace que se vea el mundo de noche, y de paso que el charco no sea el
//     único sitio con información.
//  3. **La lámpara teñía poco** (`LAMP_TINT` 0.42 ⇒ 58 % blanco). El pozo salía
//     neutro/blancuzco en vez de cálido. A 0.80 el charco conserva el ámbar de la
//     llama, que es lo que pedía el reporte: "que conserve la tonalidad ambiental
//     y no blanquee el centro".
//  4. **Se sumaban demasiados fotones** (`ADD_GAIN` 0.38 + `BULB_GAIN` 0.52 sobre
//     un bulbo grande). Sumar luz sobre un albedo oscuro tira el color hacia el de
//     la lámpara: eso es correcto en poca cantidad y es *lavado* en mucha. A la
//     mitad, el centro sigue siendo el punto más brillante pero el material de
//     abajo se sigue reconociendo.
//
// La regla que queda: **el color del pozo lo pone el multiplicador (pasada 1), no
// la suma (pasada 2).** La suma es sólo el remate del centro.
//
// ## AMPLIADO (ago 2026) — "que se vea como la referencia"
//
// Tres cosas nuevas, ninguna de las cuales cuesta un pase extra ni un byte más de
// ancho de banda: las tres viven DENTRO de las dos pasadas que ya existían.
//
//  1. **Halo (el "bloom" de cada luz).** Un lóbulo ancho y suave —`(1-x²)²`, más
//     gordo que el poly6 del pozo— sumado con poca ganancia. Es lo que hace que un
//     farol se lea como un farol y no como una mancha en el piso: el ojo espera un
//     resplandor alrededor del vidrio, no un corte. Bloom de verdad (leer la
//     pantalla, downsample, blur, upsample) es un mip-chain por frame — en una GPU
//     de tiles eso es el recurso escaso, y para un charco radial da lo mismo.
//  2. **Viñeta.** La pasada de SOMBRA es un multiplicador a pantalla completa: un
//     término que sólo depende de la posición en pantalla sale GRATIS ahí. Cierra
//     el cuadro por los bordes como en las capturas de referencia, y de paso tapa
//     el borde de la nada donde el mundo deja de estar sembrado.
//  3. **Bitono frío/cálido.** La sombra se enfría un pelo más de lo que ya
//     estaba y la luz queda tal cual: es el contraste de temperatura que hace
//     que un pueblo de noche se vea ámbar contra azul y no gris contra gris.
//
// Y el bucle se abarató para poder subir el presupuesto de luces sin pagarlo:
// **`lights[i].z` ya no es el alcance sino `1/alcance²`, y `tints[i].w` es
// `1/(radio·BULB_R)`.** Así el rechazo de una luz fuera de alcance —que es el
// caso del 90 % de las luces en el 90 % de los píxeles— cuesta una resta, un
// `dot`, una multiplicación y una comparación: ni raíz cuadrada ni división. La
// raíz sólo se paga por las luces que SÍ tocan el píxel. Medido en operaciones:
// el rechazo pasó de ~11 a ~6, y por eso 40 luces cuestan hoy lo que costaban 24.
#import bevy_sprite::mesh2d_vertex_output::VertexOutput

const MAX_VEIL_LIGHTS: u32 = 40u;

/// Nivel al que la noche cerrada multiplica el albedo del mundo (1 = mediodía).
/// Es EL número de la noche: `contraste = pozo / esto`. A 0.02 no ves nada, a
/// 0.15 es un atardecer permanente.
const NIGHT_FLOOR: f32 = 0.115;

/// Cuánto del matiz de la sombra se conserva (0 = gris neutro, 1 = el navy /
/// óxido / hielo puro de `sky2d::body_night_veil`). Sin desaturar algo, dividir
/// por la luma deja un azul eléctrico.
const SHADE_SAT: f32 = 0.88;

/// Cuánto TIÑE la lámpara el albedo dentro de su pozo. Es el término "ambiente
/// de color": no suma brillo, cambia el color de lo que ya está. Alto tiñe todo
/// de naranja plano; en 0 el pozo se ve blanco/neutro y la luz no se siente
/// cálida hasta el bulbo.
const LAMP_TINT: f32 = 0.80;

/// Ganancia de la pasada aditiva. Lo que decide cuánto ILUMINA una luz (vs sólo
/// destapar el albedo). Medido: a 0.62 el pozo entero de una antorcha llegaba a
/// blanco —sumar 0.62 en lineal es sRGB 0.82 sobre un albedo que ya se ve— y se
/// perdía el gradiente justo donde más se mira. A 0.34 el centro sigue siendo el
/// punto más brillante de la pantalla pero el degradé hasta el borde se lee.
const ADD_GAIN: f32 = 0.20;

/// Brillo del bulbo del emisor. Reemplaza a los quads de halo/neblina que había
/// antes (eran `Sprite` alpha: mezclaban HACIA su color y por eso desaturaban
/// media pantalla — la "niebla blanca"). Sumar no desatura nada.
const BULB_GAIN: f32 = 0.26;

/// Tamaño del bulbo como fracción del radio declarado del emisor. **Chico a
/// propósito.** Una llama SÍ quema a blanco, pero en un punto: al 100% del radio
/// el blanco cubría un disco de 4 m (medido, ~120 px en el encuadre de a pie) y
/// eso es exactamente "el brillo blanco que deja todo opaco". El calor tiene que
/// venir del color que rodea al punto, no del punto agrandado.
const BULB_R: f32 = 0.16;

/// Luz ambiente de la noche, como fracción del tinte de sombra. Es el único
/// término plano que queda y existe para una sola cosa: que en la oscuridad
/// total se adivinen las siluetas. En 0 la noche sin luna es negro absoluto.
const AMBIENT_GAIN: f32 = 0.30;

/// Ganancia del HALO: el resplandor ancho alrededor de cada luz, o sea el
/// "bloom" sin pase de bloom. Va bajo a propósito — su gracia es cubrir mucha
/// superficie con muy poco, que es exactamente lo que hace un blur gaussiano.
/// Subirlo lava la escena igual que subir `ADD_GAIN` (ver el recalibrado de
/// julio): el halo tiene que ADIVINARSE, no verse. **0,15 y no 0,085**: con
/// 0,085 no se distinguía del pozo en la captura de `HA_NIGHTTEST` y el farol
/// seguía leyéndose como una mancha en el piso en vez de como algo encendido.
const HALO_GAIN: f32 = 0.15;

/// Tamaño del halo como múltiplo del bulbo. El resplandor de un farol es varias
/// veces su vidrio; con 1.0 no se distinguiría del bulbo y no habría halo.
const HALO_R: f32 = 5.0;

/// Cuánto cierra la viñeta las esquinas del cuadro (0 = ninguna). Es un
/// multiplicador, así que 0.34 = las esquinas quedan al 66 % de su brillo.
/// Medido contra las capturas de referencia: por encima de 0.45 se lee como un
/// agujero negro en las esquinas, por debajo de 0.20 no se nota.
const VIGNETTE: f32 = 0.34;

/// Cuánto de la viñeta hay a pleno DÍA. **Cero, y no por gusto:** `toggle_light_passes`
/// APAGA estas dos pasadas cuando la noche llega a 0 y no hay ninguna luz
/// encendida (a mediodía multiplicar por 1 y sumar 0 son cuatro millones de
/// píxeles leídos y escritos para dejar la pantalla igual, ver `LightPass`). Si
/// la viñeta valiera algo justo en ese punto, al apagarse la pasada las esquinas
/// pegarían un salto de brillo. Atada a `night`, entra y sale con la noche y no
/// hay nada que saltar. La viñeta es, además, un recurso de encuadre nocturno:
/// a pleno sol se lee como suciedad en el lente.
const VIGNETTE_DAY: f32 = 0.0;

/// Enfriamiento extra de la sombra: cuánto se le baja el rojo (y se le sube el
/// azul) al multiplicador fuera de los pozos. Es el bitono — ámbar contra azul —
/// y es la mitad de por qué una noche con faroles se ve "de película".
const COOL_SHADE: f32 = 0.10;

/// Caída radial — **gemela de `falloff()` en `light2d.rs`**. Kernel poly6
/// `(1 - x²)³`: derivada 0 en los dos extremos. La gaussiana renormalizada que
/// había antes moría en x = 1 con pendiente −0.27 y ese escalón, contra una noche
/// casi negra, es el "corte al ras" que se veía como un disco recortado.
fn falloff(x: f32) -> f32 {
    let t = 1.0 - x * x;
    return t * t * t;
}

/// El mismo poly6 pero tomando **x² ya calculado**: el bucle trabaja con
/// distancias al cuadrado para no pagar una raíz por luz y por píxel.
fn falloff2(x2: f32) -> f32 {
    let t = 1.0 - x2;
    return t * t * t;
}

/// Viñeta: 1 en el centro del cuadro, `1 - fuerza` en las esquinas.
/// `frag` son píxeles de pantalla y `size` el viewport, así que la elipse sigue
/// al encuadre real y no se deforma al girar el teléfono.
fn vignette(frag: vec2<f32>, size: vec2<f32>, fuerza: f32) -> f32 {
    if size.x < 1.0 || size.y < 1.0 || fuerza <= 0.0 {
        return 1.0;
    }
    // −1..1 con el centro en 0. `r` vale ~1.41 en la esquina.
    let ndc = frag / size * 2.0 - vec2<f32>(1.0);
    let r = length(ndc);
    // Arranca pasada la mitad del cuadro: lo que el ojo mira no se toca.
    let k = smoothstep(0.62, 1.42, r);
    return 1.0 - fuerza * k * k;
}

struct NightVeilMaterial {
    // rgb = tinte de la sombra (lineal), a = cuánta noche hay (0..1)
    color: vec4<f32>,
    // xy = viewport en PÍXELES (para la viñeta), z/w = libres.
    view: vec4<f32>,
    // count + padding (16-byte align antes de los arrays)
    count: u32,
    _p0: u32,
    _p1: u32,
    _p2: u32,
    // xy = world pos (m), z = **1/alcance²** (no el alcance), w = fuerza 0..1
    lights: array<vec4<f32>, 40>,
    // rgb = color de esa luz (lineal), w = **1/(radio·BULB_R)** para el bulbo
    tints: array<vec4<f32>, 40>,
}

@group(2) @binding(0)
var<uniform> material: NightVeilMaterial;

/// Ruido hash barato por píxel (Dave Hoskins). Solo se usa para el dither.
fn hash12(p: vec2<f32>) -> f32 {
    var p3 = fract(vec3<f32>(p.x, p.y, p.x) * 0.1031);
    p3 += dot(p3, vec3<f32>(p3.y, p3.z, p3.x) + 33.33);
    return fract((p3.x + p3.y) * p3.z);
}

/// Normaliza un color a luma 1 conservando el matiz, y lo desatura un poco.
/// Sirve para las dos pasadas: el tinte de sombra y el de lámpara se declaran
/// con magnitudes arbitrarias (venían de una época en que se pintaban a pelo) y
/// acá sólo interesa su COLOR — el nivel lo ponen las constantes de arriba.
fn hue_of(c: vec3<f32>, sat: f32) -> vec3<f32> {
    let lum = max(dot(c, vec3<f32>(0.2126, 0.7152, 0.0722)), 1e-5);
    return mix(vec3<f32>(1.0), c / lum, sat);
}

// ── Pasada 1: SOMBRA (blend multiplicativo `dst * src`) ──────────────────────
//
// Sale un MULTIPLICADOR, no un color: 1.0 = "acá no pasa nada" (mediodía),
// `NIGHT_FLOOR` = noche cerrada. Dentro de un pozo de luz vuelve a ~1 pero
// teñido del color de la lámpara: el suelo recupera su albedo y encima se lee
// cálido, que es la mitad de que una fogata se sienta fogata.
@fragment
fn fragment(mesh: VertexOutput) -> @location(0) vec4<f32> {
    let p = mesh.world_position.xy;
    let n = min(material.count, MAX_VEIL_LIGHTS);

    var cover = 1.0; // 1 = sombra entera, 0 = pozo pleno
    var tint = vec3<f32>(0.0);
    var tint_w = 0.0;

    for (var i = 0u; i < n; i = i + 1u) {
        let l = material.lights[i];
        // **Sin raíz y sin división.** `l.z` viene como 1/alcance², así que
        // rechazar una luz que no llega a este píxel cuesta una resta, un `dot`,
        // una multiplicación y una comparación. Es el 90 % de los casos y es lo
        // que paga subir el presupuesto de 24 a 40 luces sin costar más.
        let dp = p - l.xy;
        let x2 = dot(dp, dp) * l.z;
        if x2 >= 1.0 {
            continue;
        }
        let s = falloff2(x2) * clamp(l.w, 0.0, 1.0);
        // Unión suave: N luces débiles se funden sin producir un escalón donde
        // una "gana" sobre otra (que es lo que hacía `min`).
        cover *= 1.0 - s;
        tint += material.tints[i].rgb * s;
        tint_w += s;
    }
    let lit = 1.0 - cover;

    // Nivel de la sombra según cuánta noche hay, con el matiz del cuerpo.
    let night = clamp(material.color.a, 0.0, 1.0);
    let shade_lvl = mix(1.0, NIGHT_FLOOR, night);
    var shade = hue_of(material.color.rgb, SHADE_SAT * night) * shade_lvl;

    // Dentro del pozo el albedo vuelve casi entero, teñido de la lámpara.
    if tint_w > 0.001 {
        // Acotado a 1: un multiplicador > 1 no aclara nada (el target es unorm y
        // recorta) pero SÍ desbalancea el matiz — el rojo de una llama se iría a
        // 1.33 y volvería recortado a 1.0, o sea que el tinte real dejaría de ser
        // el declarado. Aclarar es trabajo de la pasada aditiva.
        let lamp = min(hue_of(tint / tint_w, LAMP_TINT), vec3<f32>(1.0));
        shade = mix(shade, lamp, lit);
    } else {
        shade = mix(shade, vec3<f32>(1.0), lit);
    }

    // **Bitono.** La sombra se enfría; el pozo NO (ya está teñido de la lámpara).
    // Un pueblo de noche se ve ámbar contra azul, no gris contra gris, y ese
    // contraste de temperatura es gratis acá: son dos multiplicaciones sobre un
    // valor que ya se estaba escribiendo.
    let frio = vec3<f32>(1.0 - COOL_SHADE, 1.0 - COOL_SHADE * 0.35, 1.0 + COOL_SHADE * 0.5);
    shade *= mix(vec3<f32>(1.0), frio, night * (1.0 - lit));

    // **Viñeta.** Sale gratis: esta pasada YA es un multiplicador a pantalla
    // completa, así que cerrar el cuadro es una multiplicación más por píxel.
    // De día pesa un tercio (ver `VIGNETTE_DAY`).
    let v_fuerza = VIGNETTE * mix(VIGNETTE_DAY, 1.0, night);
    shade *= vignette(mesh.position.xy, material.view.xy, v_fuerza);

    let dither = (hash12(mesh.position.xy) - 0.5) * (1.2 / 255.0);
    // α = 1: el blend multiplicativo ignora α para el color, pero dejarlo en 1
    // evita que el canal alpha del target se coma la escena si alguien mira ahí.
    return vec4<f32>(max(shade + vec3<f32>(dither), vec3<f32>(0.0)), 1.0);
}

// ── Pasada 2: LUZ (blend aditivo `dst + src`) ───────────────────────────────
//
// Fotones. Se SUMAN, así que dos luces que se solapan dan más luz y nunca una
// mancha que tapa: es el "blend entre luces" bien hecho. El gradiente por píxel
// vive acá: `near` (inverso-cuadrático) es lo que hace que el borde del pozo se
// vea mucho más apagado que el centro, y que la cara de un objeto que da a la
// luz reciba más que la de atrás.
@fragment
fn fragment_add(mesh: VertexOutput) -> @location(0) vec4<f32> {
    let p = mesh.world_position.xy;
    let n = min(material.count, MAX_VEIL_LIGHTS);
    let night = clamp(material.color.a, 0.0, 1.0);

    // Ambiente: el único término plano, y bajísimo. Es "ver la silueta".
    var acc = material.color.rgb * (AMBIENT_GAIN * night);

    for (var i = 0u; i < n; i = i + 1u) {
        let l = material.lights[i];
        let t = material.tints[i];
        // Mismo rechazo barato que la pasada de sombra: `l.z` = 1/alcance².
        let dp = p - l.xy;
        let d2 = dot(dp, dp);
        let x2 = d2 * l.z;
        if x2 >= 1.0 {
            continue;
        }
        let k = clamp(l.w, 0.0, 1.0);
        // Difusa: poly6 (borde sin final) × inverso-cuadrático (gradiente).
        let near = 1.0 / (1.0 + 8.0 * x2);
        let diffuse = falloff2(x2) * (0.26 + 0.74 * near) * k;
        // Bulbo y HALO: los dos salen del radio DECLARADO del emisor
        // (`t.w` = 1/(radio·BULB_R)), no del alcance del pozo — así una vela
        // tiene punto chiquito y halo chiquito, y una hoguera los tiene gordos.
        // `w <= 0` = el emisor ya dibuja su propio brillo (luciérnagas,
        // `Light2d::draw_glow == false`) y sumarle otro lo quemaría a blanco.
        var nucleo = 0.0;
        if t.w > 0.001 {
            // La raíz SÓLO acá: para las luces que de verdad tocan este píxel.
            let d = sqrt(d2);
            let ub = clamp(1.0 - d * t.w, 0.0, 1.0);
            // Bulbo: el punto caliente, exponente alto = chiquito y quemado.
            nucleo = pow(ub, 3.5) * BULB_GAIN;
            // **Halo — el "bloom" de esta luz.** Mismo centro, `HALO_R` veces
            // más ancho y con poca ganancia: es lo que hace que un farol se lea
            // como un resplandor y no como una mancha recortada en el piso.
            // `(1-u)²` tiene derivada 0 en el borde igual que el poly6, así que
            // tampoco tiene dónde hacer escalón.
            let uh = clamp(1.0 - d * t.w / HALO_R, 0.0, 1.0);
            nucleo += uh * uh * HALO_GAIN;
        }
        acc += t.rgb * (diffuse * ADD_GAIN + nucleo * k);
    }

    let dither = (hash12(mesh.position.xy + vec2<f32>(37.0, 11.0)) - 0.5) * (1.4 / 255.0);
    return vec4<f32>(max(acc + vec3<f32>(dither), vec3<f32>(0.0)), 0.0);
}
