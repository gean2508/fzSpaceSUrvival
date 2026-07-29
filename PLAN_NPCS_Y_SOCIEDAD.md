# PLAN — NPCs, sociedad viva y vida multi-planeta

> **Qué es esto.** La auditoría completa de TODO lo que hoy es "NPC" en el juego, cada
> defecto encontrado con su arreglo exacto, y el diseño —ya teorizado, listo para tipear—
> de la sociedad multi-planeta con linajes e **hijos**. Está escrito para que **cualquier
> modelo o persona lo ejecute sin volver a pensarlo**: cada tarea dice qué archivo tocar,
> qué constante poner y cómo verificar que quedó bien.
>
> Sustituye y absorbe a `PLAN_SOCIEDAD_MEMORIA.md` (F6). Depende de:
> `PLAN_VIVARIO_VIDA_EMERGENTE.md` (patrón de persistencia + activación por cuerpo),
> `PLAN_COMPANERO_IA.md` (hill-climb del compañero) y `PLAN_MAESTRO.md` §4 (reglas
> inviolables). Fuente de verdad de la percepción: `ha-shared/src/fauna_brain.rs`.

---

## 0. Reglas para quien implemente esto

1. **No podar lo emergente.** La memoria, los roles y la cultura **abren reacciones**; no
   escriben guiones. Si una fase te tienta a hardcodear "el aldeano X hace Y", está mal
   planteada. (`ai-life-emergence-no-prune`.)
2. **Una fase = un commit verde.** `cargo check -p ha-game` + `cargo test -p ha-assets`
   + una captura del driver correspondiente (§8). Nada de fases a medio camino.
3. **Presupuesto primero.** Antes de agregar un sistema, decí en qué renglón de la tabla
   de §6 entra y cuánto se come. Si no entra, no se hace.
4. **ASCII-only en todo lo que dibuja el juego.** La fuente no tiene acentos ni emoji
   (`game-font-ascii-only`). Este documento sí puede tenerlos.
5. **Nada de números mágicos sueltos.** Toda constante nueva va arriba del archivo, con
   un comentario que diga *por qué* ese valor.
6. **La simulación manda, el render obedece.** Ningún sistema de sociedad puede depender
   de que la entidad esté spawneada: los NPCs lejos son datos.
7. **Determinismo.** Todo lo procedural (nombres, semillas, culturas, siembra) sale de
   hashes de `(uid, body, celda)`. Nunca de `rand` ni del reloj de pared.

---

## 1. Mapa del sistema hoy

### 1.1 Los cinco subsistemas que hoy son "NPC"

| Archivo | Qué es | Cerebros | Persistencia | Estado |
|---|---|---|---|---|
| `society.rs` | Aldeanos/colonos con familia, reputación, fe | IAFZ `72..120` | `society/society.json` + `g_<uid>.adn` | El núcleo |
| `companion.rs` | Mano derecha del jugador, hill-climb online | IAFZ `990/991` | `vivarium/companion.adn` | Aparte, sano |
| `fauna.rs` | Fauna ambiental + residentes del vivario | IAFZ `0..48` / `48..72` | `vivarium/*.json` + `.adn` | Referencia de patrón |
| `colony.rs` | Contador de colonos enviados (dato, no agentes) | — | con el resto del save | Alimenta la siembra lunar |
| `npc_menu.rs` + `dialog.rs` | Panel de conversación + burbujas de mundo | — | ninguna | Presentación |

### 1.2 `society.rs` — cómo funciona hoy

- **Entidad `Npc`**: `brain, x, z, heading, family, uid, energy, age, faith, converted,
  home_x/z, say_cd, history`.
- **Activación por cuerpo** (`manage_society`): si cambiás de cuerpo, snapshotea los vivos
  a `Society.records`, despawnea, y siembra/instancia los del cuerpo nuevo. Los de otros
  cuerpos viven como `NpcRecord` en memoria + JSON.
- **Siembra**: Tierra → 9 aldeanos alrededor del jugador con `rep` base `0.4`;
  Luna → `colony.state.colonists` alrededor de los hábitats; otros planetas →
  3..9 nativos con disposición según `planet_temperament_disposition(body)`.
- **Memoria**: `family_rep: HashMap<u32, f32>` en `[-1, +1]`, persistente. Matar a alguien
  = `-0.7` a su familia y `-0.2` a los testigos.
- **Conducta** (`tick_society`, 15 Hz): el cerebro IAFZ aporta esquive con autoridad
  reducida (`* 0.35`); encima se le suma un rumbo por propósito (fiel → orbita al ídolo;
  rencor fuerte → te encara; rencor leve → huye; si no → deambula cerca de `home`).
- **Religión**: recurso global `Religion { founded, name, followers }`; se funda sola si
  juntás `FOUND_MIN_FLOCK = 3` NPCs cordiales en `FOUND_RADIUS = 13 m`. Cerca del ídolo
  (vos) ganan `faith`; a `0.6` se convierten.
- **Charla**: `npc_line(rep, converted, faith, founded, name, variant)` elige una frase de
  una tabla por contexto; se encola en `dialog::SpeechQueue` y se materializa como `Text2d`.

### 1.3 El cerebro (y por qué hoy casi no se usa)

`fauna_brain::sense()` codifica 32 bits: 3 de paredes, 2 de bioma, 3+1 de comida, 1 de
energía, 3+1 de vecino. **`society.rs` lo llama con `foods: &[]`** — o sea, 4 de los 14
bits informativos están siempre en cero, y el genoma `FAUNA.adn` fue entrenado en el gym
justamente para forrajear. Traducción: **el "cerebro" de los NPCs aporta ruido de esquive
y nada más**; toda la conducta social sale del `if/else` de propósito. No está mal como
punto de partida, pero hay que decirlo en voz alta: hoy **no hay IA social**, hay una
máquina de estados con un poco de textura encima.

---

## 2. AUDITORÍA — defectos verificados

Severidad: **P0** rompe datos o el juego · **P1** rompe la simulación · **P2** explotable
o incoherente · **P3** presentación / deuda.

### P0 — pérdida de datos

**A1. `harm_events` guarda el mundo SIN los NPCs vivos.**
`harm_events` termina con `save_society(&soc, &[])`. `save_society` escribe
`soc.records + live`, y los NPCs del cuerpo activo **fueron sacados de `records`** al
instanciarse. Con `live = &[]`, el archivo que queda en disco **no contiene el pueblo
entero**. Si el juego se cierra en los ≤20 s siguientes (o crashea), el pueblo desaparece.
*Fix:* que `harm_events` no guarde; en su lugar marcá `soc.dirty = true` y que
`save_society_periodic` haga flush inmediato cuando `dirty` esté puesto. Un solo camino de
guardado, siempre con `live`.

**A2. Los genomas `g_<uid>.adn` nunca se borran.**
Cada NPC que nace escribe su `.adn`; los que morís no lo borran. Cientos de archivos
huérfanos en el directorio de guardado del teléfono.
*Fix:* en `harm_events` y en cualquier muerte futura,
`let _ = std::fs::remove_file(npc_genome_path(uid));`.

**A3. `Religion` no se persiste.**
`Religion` es `#[derive(Resource, Default)]` y no está en `SocietySave`, pero
`Npc.converted` **sí** se guarda. Al reabrir el juego: `founded = false`, `name = ""`, y
los fieles siguen siendo fieles → `npc_line` devuelve literalmente **`"Alabado sea !"`** y
**`"Somos ."`**. Bug visible en pantalla.
*Fix:* agregar `religion: Option<ReligionSave>` a `SocietySave` y cargarlo en
`manage_society` junto con el resto.

### P1 — la simulación

**B1. El pueblo se deshace solo (deriva del hogar).**
`spawn_npc` hace `home_x = rec.x, home_z = rec.z`. Como `rec` es la posición *actual*
guardada, cada vez que salís y volvés al cuerpo el "hogar" se re-ancla donde el NPC estaba
parado. Repetido N veces es una caminata aleatoria: **el pueblo se difunde hasta
desaparecer**.
*Fix:* `home_x/home_z` son parte de la identidad → agregarlos a `NpcRecord` y fijarlos en
la siembra. Nunca derivarlos de la posición.

**B2. Los nativos de otros planetas se siembran en el origen del mundo.**
`seed_society(..., origin = (0.0, 0.0))` para todo cuerpo que no sea Tierra/Luna. Aterrizás
donde sea y los nativos están en `(0,0)`, típicamente a kilómetros. **La feature
multi-planeta es invisible en la práctica.**
*Fix:* sembrar alrededor del punto de aterrizaje (igual que la Tierra) **la primera vez**,
y guardar ese punto como `Colony.center` del cuerpo (ver §4.2) para que no se mueva nunca
más.

**B3. Genocidio = borrón y cuenta nueva.**
La siembra corre `if recs.is_empty()`. Matás a todos → `records` de ese cuerpo queda vacío
→ la próxima visita **siembra un pueblo nuevo con familias nuevas y reputación limpia**. La
memoria, que es el corazón del sistema, se lava matando.
*Fix:* la reputación se ancla a la **CULTURA del cuerpo** (`body_rep`), no sólo a la
familia. Familia extinta → su rencor se transfiere (atenuado) a `body_rep`, y toda familia
nueva de ese cuerpo nace con `rep = clamp(base_cultura + body_rep)`. Además: un cuerpo
"vaciado" no re-siembra hasta que pasa un tiempo de mundo largo (`RESEED_AFTER_DAYS`), y
cuando lo hace, los nuevos **saben** lo que pasó.

**B4. No hay selección → no hay evolución.**
`age` se acumula pero nunca mata (deliberado, §0.5 del plan del vivario), y no hay
depredación ni combate. Con `energy += dt * 0.006` y repro a `0.97`, cada NPC se reproduce
cada ~83 s **para siempre** hasta saturar `MAX_NPCS = 48`, y de ahí nada cambia. La
mutación por nacimiento no tiene con qué competir: **F6.2 "evolución" es nominal**.
*Fix (en dos partes):*
  - la reproducción deja de ser un temporizador y pasa a depender de **necesidades
    satisfechas** (comida en el pueblo, techo, pareja — §3.4);
  - la selección la da el **fitness de colonia** del tick abstracto (§4.2), no la muerte
    individual: las familias que rinden mal aportan menos hijos. Se conserva la regla
    "nadie se esfuma": la presión es en la natalidad, no en la mortalidad.

**B5. Los testigos incluyen a la propia familia de la víctima.**
El filtro de `witnesses` sólo excluye la entidad muerta, no a los de su familia. Su familia
se come `-0.7` **y además** `-0.2` por cada pariente cercano.
*Fix:* `.filter(|(we, n)| *we != e && n.family != family)`.

**B6. `deaths` es código muerto.**
`let deaths: Vec<(Entity, i32)> = Vec::new();` nunca se llena y se itera igual.
*Fix:* borrarlo, o cablearlo al canal de daño real cuando exista combate (§3.6).

**B7. El mundo se congela cuando no mirás.**
`tick_society` hace `continue` fuera de `RENDER_R = 80 m`, y los otros cuerpos son
registros inertes. El pueblo no crece, no come, no construye si no estás parado ahí. Choca
de frente con el principio "la vida está AHÍ y vive SIN el jugador".
*Fix:* el tick abstracto de colonia de §4.2 (LOD 2 y 3).

**B8. Los NPCs no duermen ni reaccionan al día.**
Con la noche ahora realmente oscura (`sky2d::NIGHT_VEIL = 0.86`) se ve rarísimo que sigan
paseando a las 3 AM en la penumbra.
*Fix:* rutina diaria (§3.4). Es además el mejor escaparate del sistema de luz: al
anochecer se van a la casa y **se encienden las ventanas**.

### P2 — explotables e incoherencias

**C1. Cualquier toque de habilidad mata al aldeano más cercano al cursor.**
`harm_events` sólo pide `tap.slot.is_some()` + cursor inicializado + `HARM_RADIUS = 2.6 m`
**del cursor** (no del jugador). No hay vida, ni intención de atacar, ni distancia máxima
al jugador. Usar cualquier habilidad cerca del pueblo es un homicidio accidental que te
arruina la reputación para siempre.
*Fix:* ver §3.6 — la habilidad tiene que ser de ataque, el NPC tiene `health`, y el golpe
requiere estar dentro de `PLAYER_REACH`.

**C2. `CALMAR` sin cooldown.** `+0.15` por click, spam → cualquier rencor se borra en
segundos. *Fix:* un uso por NPC cada `CALM_COOLDOWN = 120 s` de mundo, con techo
(`rep` no puede pasar de `-0.1` sólo calmando: para volver a estar bien hay que **hacer**
algo, no hablar).

**C3. `CONVERTIR` sin cooldown y sin condiciones.** `+0.3` de fe por click → dos clicks
convierten hasta a un `HOSTIL`. *Fix:* exigir `rep > AGGRO_THRESHOLD`, un intento cada
`CONVERT_COOLDOWN = 60 s`, y que la probabilidad dependa de `rep` y de cuántos fieles ya
tenga alrededor (contagio social, no botón mágico).

**C4. `COMERCIAR` es un stub.** Dice "No tengo nada para comerciar aun". *Fix:* §3.5.

**D5. El menú se abre desde cualquier toque, incluso sobre el joystick.**
`pick_npc` lee `TouchInput::Started` crudo y sólo ignora los toques sobre su propio panel.
Un dedo apoyado en el stick izquierdo con un NPC cerca abre el panel
(`touch-ui-stick-zones` documenta exactamente esta clase de bug).
*Fix:* consultar `TouchBlockRects` / las zonas de stick de `input::TouchLayout` antes de
aceptar el toque.

**D6. El menú no se cierra al alejarte.** Sólo cierra si la entidad deja de existir.
*Fix:* cerrar si `dist(jugador, npc) > TALK_RANGE = 6 m`.

**D7. `religion.followers` se cuenta dos veces.** `tick_society` lo recalcula de cero cada
tick; `npc_menu` hace `+= 1`. El incremento del menú se pisa siempre → código muerto que
parece que hace algo. *Fix:* borrar el `+=`.

**D8. `CONVERT_AT` está duplicado** (`society.rs` y `npc_menu.rs`, ambos `0.6`).
*Fix:* dejarlo público en `society.rs` y que el menú lo importe.

### P3 — presentación y deuda

**E1. Lluvia de burbujas.** `SAY_COOLDOWN = 7 s` por NPC × ~30 NPCs en 30 m → ~4
burbujas/s, cada una viva 2.8 s → **~12 textos superpuestos** permanentes. Verificado en
capturas: el centro de la pantalla es una sopa ilegible.
*Fix:* presupuesto global de burbujas (`MAX_BUBBLES = 3`), cooldown global además del
individual, prioridad (lo que te dicen a vos > el chatter), y que la burbuja **siga a su
NPC** en vez de quedar clavada en el aire.

**E2. Las burbujas no tienen dueño.** Se spawnean en una posición fija y flotan. Si el NPC
camina, su frase queda atrás. *Fix:* `SpeechBubble { owner: Entity }` + seguir el
transform del dueño.

**E3. Los NPCs no se animan ni miran a ningún lado.** `spawn_npc` usa
`sprite_or_color("npc/char_N")` = PNG entero, sin poses. Peor: los `art/npc/char_XX.png`
que hay hoy son recortes de **42×42 px** (un frame suelto), no hojas — así que no hay de
dónde sacar direcciones. El jugador sí tiene poses. Los aldeanos se ven como calcomanías
deslizándose.
*Fix:* horneá los NPCs con el **Character Generator** (§2.9) para que cada uno sea una
HOJA completa, y después reusá el camino del jugador: `art.pose(key, dir4(heading))`.

### 2.9 Character Generator (capas) — reemplaza los 20 skins premade

El pack `PRUEBAS-CODIGOS/Character_Generator/` es el generador por capas de LimeZu:
cuerpos + ojos + ropa + peinados + accesorios, apilados **en ese orden exacto**
(`CHARACTER_GENERATOR.txt`). Su instructivo dice de componerlos a mano en Piskel; eso no
escala para un pueblo entero ni para un creador de personaje jugable.

Traído al código en `native/crates/ha-asset-tools/src/chargen.rs`:

- **Se hornea, no se compone en runtime.** Las hojas del pack son de 1854×1312 px (~9,7 MB
  ya decodificada, cada capa). Apilar 5 de esas por NPC en el teléfono es inviable, y
  además rompería la regla de "un PNG por slot" (`imglab-multipng-editor`). El generador
  corre en la herramienta de dev y escribe **un PNG por personaje**, a media escala
  (celdas 16×32, el layout que ya usan `player/character` y el atlas del creador).
- **Determinista.** `Catalog::layers_from_seed(uid)` con SplitMix64: el aldeano `uid = 37`
  tiene siempre la misma cara, en la PC y en el Redmi. 9 cuerpos × 7 ojos × 132 ropas ×
  200 peinados × 84 accesorios ≈ **1,4 millones de combinaciones**, contra los 20 premade
  de hoy.
- **Soporta niños** (`Bodies_kids` / `Outfits_kids` / …) → los hijos de §4.5 se ven como
  hijos sin arte nuevo. El accesorio sale 1 de cada 4 (si no, el pueblo parece un desfile).

```bash
# Rehornear los 20 aldeanos + el jugador default, media escala:
./editor_assets chargen --npcs 20 --seed 7 --player
```

*Pendiente de esta pieza (va en S3):* que `character_create.rs` deje de elegir "hoja
entera + tinte" y pase a elegir **capa por capa** (cuerpo/ojos/ropa/pelo/accesorio) con
preview, guardando los 5 índices en `SelectedCharacter`; y que `society::spawn_npc` derive
la hoja de `uid` en vez del `uid % 20` actual.

**E4. `history` no se persiste.** `NpcRecord` no la lleva → al volver al cuerpo, "últimos
mensajes" está vacío. *Fix:* guardar las últimas 3 (no 5: peso).

**E5. Los nombres colisionan.** `NAMES[uid % 24] + " " + (uid/24)%100` repite cada 2400
uids. Poco probable, pero el sufijo numérico además se lee mal ("Ana 0").
*Fix:* nombre + apellido de familia (§3.2): "Ana Vero", "Beto Vero" — se lee como pueblo,
no como inventario.

**E6. `family_rep` crece para siempre.** Cada familia sembrada deja su entrada aunque el
cuerpo no se visite nunca más. *Fix:* al pasar a `body_rep` (B3), podar familias extintas.

**E7. Dos copias del mismo patrón.** `manage_society`/`save_society_periodic` y
`manage_vivarium`/`save_vivarium_periodic` son el mismo algoritmo escrito dos veces.
*Fix:* extraer `persistent_agents.rs` (§3.1) — **hacerlo recién en S7**, cuando el diseño
esté quieto; refactorizar antes es cambiarlo dos veces.

**E8. Presupuesto de cerebros sin plan.** `0..48` fauna, `48..72` residentes, `72..120`
NPCs, `990/991` compañero, `MAX_AGENTS = 1000`. Al pasar a multi-planeta con LOD 2 hay que
decidir explícitamente que **sólo los del cuerpo activo tienen cerebro** (§4.1) o el rango
no alcanza.

---

## 3. El modelo objetivo

### 3.1 Agente persistente (concepto unificador)

Todo ser vivo del juego es lo mismo con distinta piel:

```
AgentRecord {
  uid: u64,            // identidad estable, nunca se reusa
  body: u8,            // en qué cuerpo celeste vive
  kind: AgentKind,     // Fauna | Resident | Npc | Companion
  x, z, heading,       // dónde
  genome: GenomeRef,   // archivo .adn propio
  vitals: Vitals,      // energía, salud, edad
  social: Option<SocialRecord>,  // sólo NPCs: familia, linaje, rol, fe
}
```

Tres niveles de detalle (LOD), y esta es **la** decisión de arquitectura del documento:

| LOD | Cuándo | Qué corre | Costo |
|---|---|---|---|
| **1 — vivo** | Cuerpo activo, a < `RENDER_R` del jugador o de una cámara | Entidad ECS + cerebro IAFZ a 15 Hz + render + luz | El de hoy |
| **2 — abstracto** | Cuerpo activo, lejos · o cuerpo inactivo con colonia | Tick de **colonia** cada `COLONY_TICK = 30 s` de mundo: recursos, natalidad, obras, humor | ~microsegundos |
| **3 — dormido** | Cuerpo sin colonia | Nada. Se recalcula por tiempo transcurrido al volver | Cero |

Un NPC que pasa de LOD 1 a 2 **no desaparece**: deja de tener entidad y pasa a ser una
línea en el censo de su colonia. Al volver, se re-instancia desde el censo con su uid,
su familia y su genoma.

### 3.2 Identidad y linaje

```
SocialRecord {
  family: u32,          // apellido/clan
  parents: [u64; 2],    // uids (0 = desconocido / fundador)
  partner: Option<u64>,
  birth_day: u32,       // día de mundo → edad = hoy - birth_day
  stage: LifeStage,     // Child | Adult | Elder
  role: Role,           // Farmer | Miner | Builder | Guard | Priest | Child | Wanderer
  faith: f32, cult: Option<u16>,
  traits: [i8; 4],      // valentía, sociabilidad, curiosidad, devoción (heredables)
}
```

- **Nombre** = `nombre_pila(uid)` + `apellido(family)`, ambos por tabla ASCII + hash.
- **Etapas**: `Child` hasta `ADULT_AT_DAYS = 6` días de mundo (sprite al 70%, no trabaja,
  sigue a un padre), `Elder` a partir de `ELDER_AT_DAYS = 40` (camina lento, más
  devoción/influencia, no se reproduce).
- **`traits`** se heredan como promedio de los padres ±1 de mutación. Sesgan la conducta:
  valentía → umbral de huida; sociabilidad → radio de congregación; curiosidad → cuánto se
  aleja del hogar; devoción → velocidad de conversión. **Es la evolución que hoy no
  existe**, y es legible por el jugador (a diferencia de los pesos del genoma).

### 3.3 De "reputación por familia" a memoria de hechos

La reputación como número suelto no puede explicar *por qué* te odian. Se cambia por un
**libro de hechos** acotado, por cuerpo:

```
Deed { day: u32, kind: DeedKind, family: u32, weight: f32 }
DeedKind = Killed | Healed | Gifted | Stole | Defended | Built | Converted | Broke_promise
```

- Cada colonia guarda como mucho `MAX_DEEDS = 64` hechos (los más pesados y los más
  recientes; el resto se **funde** en `body_rep`).
- `rep(familia) = base_cultura + body_rep + Σ(peso × decaimiento(días))`.
  El decaimiento es lento para `Killed` (medio año de mundo) y rápido para `Stole`.
- **Beneficio directo:** el NPC puede *decir* la razón — "vos mataste a Beto Vero" —
  usando el hecho de mayor peso. La conversación deja de ser genérica sin necesidad de
  ningún LLM.

### 3.4 Necesidades y rutina diaria

`Needs { food, rest, safety, belonging }` en `[0,1]`, se degradan lento (LOD 1) o por
fórmula al volver (LOD 2). La rutina las satisface y **es lo que hace que el pueblo se vea
vivo**:

| Franja (fase del día) | Qué hacen |
|---|---|
| 0.25–0.45 | Salen de casa, van a su puesto de trabajo (rol) |
| 0.45–0.55 | Comen en la plaza / se juntan (sube `belonging`) |
| 0.55–0.78 | Trabajan |
| 0.78–0.86 (**ATARDECER**) | Vuelven a casa. **Se encienden las ventanas y los faroles** |
| 0.86–0.25 (**NOCHE**) | Duermen adentro. Sólo los `Guard` patrullan, con antorcha |

Enganche con la luz (ya implementado, `light2d.rs`): la casa lleva una pose etiquetada
`Luces` y una `emission`; el guardia lleva un slot con `emission` de antorcha con
`flicker` alto. **No hace falta código nuevo de render — sólo marcar los assets en el
editor.**

### 3.5 Roles y economía del pueblo

Los roles no se asignan: **se eligen solos** por escasez. Cada colonia lleva
`Stock { food, wood, stone, tools }`. Cada `COLONY_TICK`, el NPC adulto sin rol (o con el
suyo saturado) toma el rol cuyo **déficit relativo** sea mayor. Producción por rol y por
tick, escalada por `traits` y por el humor de la colonia. Esto da, gratis:

- **Comercio real** (arregla C4): el NPC ofrece lo que le sobra y pide lo que le falta;
  el precio sale del déficit. El botón `COMERCIAR` abre eso.
- **Crecimiento visible**: con `Stock.wood` y un `Builder`, la colonia **coloca una
  estructura** de verdad (`structures.rs` ya sabe hacerlo y ya se persiste). Volvés a los
  tres días y hay una casa más. Ese es el momento "está vivo".
- **Hambre → migración** (§4.4).

### 3.6 Daño, salud y muerte con causa

- `Npc.health: f32` (arranca en 1.0). El golpe hace `ATTACK_DAMAGE = 0.5` → dos golpes.
- El golpe sólo cuenta si: la habilidad es **de ataque**, el cursor está sobre el NPC
  (`HARM_RADIUS`) **y** el NPC está a menos de `PLAYER_REACH = 3.2 m` del jugador.
- Herido y no muerto: huye, y su familia registra `Deed::Killed` con peso reducido
  (agresión). Curarlo genera `Deed::Healed` → **la reconciliación existe**.
- Un `Guard` con `rep <= AGGRO_THRESHOLD` **devuelve el golpe** (hoy la hostilidad no
  hace nada, B/C). Sin esto, no hay riesgo y la memoria no importa.

### 3.7 Conversación: contexto → intención → línea

Sin LLM, sin árbol de diálogo escrito a mano:

1. **Contexto**: rol, etapa, necesidades, hecho más pesado, clima social, hora, si sos
   fundador de su culto.
2. **Intención**: la de mayor prioridad — `Acusar`, `Pedir_ayuda`, `Ofrecer_trato`,
   `Predicar`, `Saludar`, `Advertir`, `Contar_novedad`.
3. **Línea**: plantilla ASCII de esa intención, rellenada con datos reales
   (`"Mataste a {nombre}. No lo olvidamos."`, `"Nos falta {recurso}. Traeme {n} y te doy
   {x}."`). La `variant` desempata por `uid`.

`npc_line` se convierte en `npc_speak(ctx) -> (Intent, String)`. El menú muestra las
intenciones abiertas como botones **en vez de** los cuatro fijos de hoy.

### 3.8 Cultos (plural)

`Religion` pasa de recurso global a `cults: Vec<Cult>` con `id, name, founder_uid,
home_body, followers, tenets: [Tenet; 2]`. Un NPC con devoción alta y sin culto puede
**fundar el suyo** → dos cultos en un pueblo → tensión, conversiones cruzadas, y a
`rep` bajo entre cultos, conflicto. Es la fuente de "sociedades y guerras por evolución"
que pide `ai-life-emergence-no-prune`, sin escribir una sola guerra a mano.

---

## 4. Vida multi-planeta

### 4.1 Regla de oro

**Todo cuerpo con colonia tiene censo; sólo el cuerpo activo tiene entidades.** El censo
vive en `society/colonies/<body>.json` para que el archivo no crezca sin control y para
poder cargar sólo lo que hace falta.

```
Colony {
  body: u8,
  center: (f32, f32),      // dónde está el pueblo — se fija UNA vez (arregla B2)
  founded_day: u32,
  culture: Culture,        // del temperamento del cuerpo (F5) + deriva propia
  stock: Stock,
  deeds: Vec<Deed>,        // la memoria (§3.3)
  body_rep: f32,
  census: Vec<AgentRecord>,
  last_tick_day: u32,      // para ponerse al día al volver
}
```

### 4.2 Tick abstracto (LOD 2/3) — las fórmulas

Corre cada `COLONY_TICK = 30 s` de mundo para colonias activas, y **de golpe por
diferencia de días** cuando volvés a un cuerpo dormido (`catch_up(days)`, con tope
`MAX_CATCHUP_DAYS = 30` para que volver tras un mes no cueste un frame entero):

```
producción_rol = base_rol × Σ(traits relevantes) × salud_colonia
stock += producción − consumo(población)
salud_colonia = clamp(stock.food / (población × RACION), 0, 1)
natalidad  = P_BASE × salud_colonia × (parejas_adultas / población) × (1 − población/CAP)
mortalidad = 0                      // nadie se esfuma (regla del vivario)
obras: si stock.wood > COSTO_CASA y hay Builder → colocar estructura, población CAP += 2
humor += (salud_colonia − 0.5) × k ; si humor < 0 durante N ticks → migración (§4.4)
```

`CAP` (población máxima) sale de las casas construidas: **el pueblo crece porque
construye**, no porque sí. Eso arregla B4 sin matar a nadie.

### 4.3 Siembra y cultura por planeta

- **Dónde**: la primera vez que pisás un cuerpo, `Colony.center` = tu punto de aterrizaje
  ajustado a la celda caminable más cercana. Se guarda y **no se recalcula nunca más**.
- **Cuántos**: `3 + hash(body) % 7` nativos, salvo Tierra (9, tu pueblo) y Luna
  (`colony.state.colonists`, los que mandaste vos).
- **Cultura**: `planet_temperament_disposition` se amplía de un `f32` a una `Culture`:

| Temperamento | Disposición | Rasgo dominante | Cómo se nota |
|---|---|---|---|
| Amigable | `+0.35` | sociabilidad alta | Te ofrecen trato al primer encuentro |
| Neutral | `+0.05` | equilibrado | Te ignoran hasta que hagas algo |
| Tenebroso | `−0.30` | curiosidad baja, devoción alta | Te vigilan; cultos que nacen solos |
| Hostil | `−0.60` | valentía alta | Guardias que te encaran de entrada |

La cultura además sesga la **paleta de nombres** y la arquitectura (qué slot de estructura
construyen), que es lo que hace que un planeta "se sienta" distinto sin arte nuevo.

### 4.4 Migración y colonización

- **Migración forzada**: `humor < 0` sostenido → una familia entera sale de la colonia y
  funda otra a `MIGRATE_DIST = 300..800 m`, con su memoria (`deeds` copiados) y su cultura,
  que a partir de ahí **deriva por separado**. Dos pueblos en un planeta, con historias
  distintas y opinión distinta sobre vos.
- **Colonización entre cuerpos**: sólo por el jugador (mandás colonos, `colony.rs` ya
  lleva la cuenta) o por un culto en expansión con `followers > 12`. La colonia hija hereda
  `culture` y `body_rep` del padre, atenuados.

### 4.5 HIJOS — reproducción, herencia y el hijo del jugador

**NPC × NPC.** Dos adultos sin pareja, misma colonia, `belonging` alto y `salud_colonia >
0.6` → se emparejan (`partner`). Una pareja con `Needs.food > 0.7` y `CAP` libre engendra
en el tick abstracto. El hijo:
- `family` = la del progenitor con más arraigo (más días en la colonia); el otro apellido
  queda en `parents` → **los árboles genealógicos existen y son consultables**;
- `traits` = promedio de los padres ±1;
- genoma = `iafz_sys::crossover(padre, madre)` + `mutate(NPC_MUTATE)`. `crossover` ya está
  en `iafz-sys` y hoy no se usa — **este es su lugar**;
- nace `Child`: sprite al 70%, sigue a un padre, no trabaja, y **hereda la reputación** —
  si su familia te odia, nace prevenido (era el "imprint" de F6.2).

**Jugador × NPC.** Es lo que pediste y encaja sin forzar nada:
- Requisitos: `rep(familia) > 0.55`, muchas interacciones, y un gesto costoso (regalo caro
  o una casa construida para esa familia) → se abre la intención `Proponer_vinculo`.
- Aceptado: `partner = PLAYER_UID` (uid reservado `1`), y esa familia pasa a
  `Family::Allied` — te defiende, te abre comercio sin margen y te da refugio.
- **El hijo del jugador**: nace en la colonia, `parents = [PLAYER_UID, npc_uid]`, con la
  skin mezclada del creador de personaje. Crece con `ADULT_AT_DAYS`. Al llegar a `Adult`:
  - te sigue como un segundo compañero (con cerebro propio, hill-climb como
    `companion.rs`), **o**
  - se queda y hereda tu rol en la colonia (si fundaste un culto, es su heredero natural).
- **Herencia**: si el jugador muere (cuando exista muerte del jugador), el hijo adulto es
  el punto de continuación. Eso convierte los hijos en una mecánica, no en un adorno.
- **Salvaguarda de diseño**: nada de esto se scriptea. Son condiciones que **abren
  intenciones**; el jugador decide. Y ningún NPC se reproduce con el jugador sin que la
  reputación se lo haya ganado.

### 4.6 Muerte por vejez: cuándo sí

Hoy no debe existir (regla del vivario: nadie se esfuma). Se habilita **sólo** cuando estén
las tres cosas: etapas de vida visibles (§3.2), funeral observable (el pueblo se junta,
`Deed` de duelo) y herencia (la casa y el rol pasan al hijo). Antes de eso, un anciano que
desaparece es un bug para el jugador, no una historia.

### 4.7 Observación remota

`sightings.rs` ya publica avistamientos por cuerpo y `camera_feed.rs` los muestra. Con el
censo de §4.1 la cámara pasa a poder mostrar **el pueblo entero de otro planeta** (posición
+ rol + humor) sin instanciar nada: es leer el censo. Eso convierte el "mundo que vive sin
vos" en algo que **se puede ver**, que es lo único que lo hace valer.

---

## 5. Cómo se ve todo esto (enganche con la luz y el reloj)

Ya implementado en esta tanda y de lo que se cuelga la fase S5:

- `light2d.rs` — halo/foco/neblina por asset, declarado en el manifest (`emission`) o
  pintado con el pincel 💡 del Image Lab. Nocturno por defecto.
- `sky2d.rs` — atardecer cálido propio, noche a `NIGHT_VEIL = 0.86` (oscura de verdad),
  `clock_hm()` y `phase_name()` para el HUD.
- `ambient.rs` — luciérnagas ancladas al MUNDO (paralaje), blur neón.
- `hud.rs` — reloj del día arriba a la derecha.

Lo que falta marcar en el editor (trabajo de assets, sin código): `emission` en
`prop/farol` (hecho como ejemplo), fogatas, `room/casa_*` con pose `Luces` para las
ventanas, y la antorcha del guardia.

---

## 6. Presupuestos (Redmi, 60 fps)

| Sistema | Tope | Frecuencia | Nota |
|---|---|---|---|
| NPCs con entidad (LOD 1) | 48 | 15 Hz | Igual que hoy |
| Colonias con tick abstracto | 12 | cada 30 s de mundo | Sólo aritmética |
| Censo total (todos los cuerpos) | 400 registros | — | ~40 KB de JSON |
| Burbujas simultáneas | 3 | — | Arregla E1 |
| Luces encendidas | 28 (Android) | 20 Hz | `light2d::MAX_LIGHTS` |
| Luciérnagas | 40 (×2 quads) | por frame | Batcheadas, 1 draw call |
| Escritura a disco | 1 cada 20 s + al pausar | — | Único camino (A1) |

**Regla:** si una fase sube alguno de estos topes, tiene que bajar otro o mostrar medición
en el Redmi (`ha-perf fps=`) antes de mergear.

---

## 7. Fases

Cada una es un commit verde e independiente. **En este orden** — las primeras arreglan lo
que está roto y no requieren diseño nuevo.

| Fase | Qué | Arregla / entrega | Riesgo | Estado |
|---|---|---|---|---|
| **S1** | Persistencia sana: un solo camino de guardado, borrar `.adn` huérfanos, persistir `Religion` y `home_x/z`, podar `family_rep` | A1 A2 A3 B1 E4 E6 | 🟢 bajo | ✅ hecho (verificado 2026-07-29 leyendo `society.rs`: `dirty`-flag, `remove_file` en `harm_events`, `ReligionSave`, `home_x/z: Option<f32>` como identidad, `history` persistido, poda de `family_rep` por `living` set) |
| **S2** | Higiene de gameplay: salud + alcance del golpe, cooldowns de CALMAR/CONVERTIR, testigos sin la familia de la víctima, cerrar el menú por distancia, respetar zonas de stick, borrar `deaths` y el `+=` de followers | C1 C2 C3 C4(parcial) B5 B6 D5 D6 D7 D8 | 🟢 bajo | ✅ hecho (verificado: `ATTACK_DAMAGE`/`PLAYER_REACH`, `CALM_COOLDOWN`/`CONVERT_COOLDOWN`, filtro de testigos excluye familia, `TALK_RANGE` en `npc_menu.rs`, `touch_blocked`/`touch_in_move_zone`, `CONVERT_AT` único) |
| **S3** | Presentación: presupuesto de burbujas + dueño + prioridad; nombre y apellido; poses direccionales de NPC | E1 E2 E3 E5 | 🟢 bajo | ◐ parcial — `MAX_BUBBLES=3` + `owner` + `priority` en `dialog.rs` ✅, nombre+apellido (`npc_name`) ✅; **falta E3** (poses direccionales — `spawn_npc` sigue usando 1 PNG entero por skin, sin `dir4(heading)`) |
| **S4** | `Colony` + censo + `center` fijo + siembra en el punto de aterrizaje + `body_rep` | B2 B3 | 🟡 medio | ✅ hecho (`fixed_center`, `soc.centers`, `soc.body_rep`/`nudge_body_rep`, disposición nativa = temperamento + body_rep) — como campos de `Society`, no como struct `Colony` separado con `census`, pero cumple el objetivo |
| **S5** | Rutina diaria + necesidades + luces del pueblo al atardecer | B8 | 🟡 medio | ☐ pendiente — sin `Needs{food,rest,safety,belonging}`, sin franja horaria en `tick_society`, sin enganche a `light2d`/`sky2d` |
| **S6** | Tick abstracto de colonia (LOD 2/3) + roles por escasez + obras + comercio real | B4 B7 C4 | 🔴 alto | ◐ parcial — `colony_step`/`Role`/`Stock`/`tick_colony_economy` (LOD2 economía para cuerpos inactivos) + `try_trade` ✅; **falta**: obras/`Builder` colocando estructura real, humor de colonia, migración por humor negativo |
| **S7** | Refactor `persistent_agents.rs` (fauna + residentes + NPCs comparten patrón) | E7 E8 | 🟡 medio | ☐ pendiente |
| **S8** | Linaje: parejas, `crossover`, etapas, `traits`, hijos NPC×NPC | §4.5 | 🟡 medio | ☐ pendiente |
| **S9** | Hijos del jugador, herencia, cultos múltiples, migración | §3.8 §4.4 §4.5 | 🔴 alto | ☐ pendiente |

---

## 8. Verificación

Comandos y criterios. **Una fase sin su verificación no está hecha.**

```bash
cargo check -p ha-game && cargo test -p ha-assets && cargo test -p ha-shared
# Panel de NPC + captura (desktop, sin teléfono):
HA_AUTOPLAY=1 HA_NPCSHOT=/tmp/npc.png ./target/debug/ha-game
# Hora del día concreta (para ver atardecer / noche / luces):
HA_AUTOPLAY=1 HA_DAYPHASE=0.80 HA_NPCSHOT=/tmp/dusk.png ./target/debug/ha-game
# Perf + comportamiento en bucle largo (mide fps y busca crashes):
HA_TESTER=1 HA_AUTOPLAY=1 ./target/debug/ha-game    # matar con: pkill -x ha-game
# Sin luces, para medir cuánto cuestan:
HA_NO_LIGHTS=1 HA_TESTER=1 HA_AUTOPLAY=1 ./target/debug/ha-game
```

Criterios por fase:

- **S1**: matar un NPC → cerrar el juego a los 5 s → reabrir: **el pueblo sigue completo**
  y el rencor persiste. El directorio de guardado no acumula `.adn` de muertos. Los fieles
  dicen el nombre del culto, no `"Alabado sea !"`.
- **S2**: usar una habilidad no-ofensiva al lado de un aldeano **no lo mata**. Dos golpes
  para matar. `CALMAR` spameado no pasa de `-0.1`.
- **S3**: captura con 30 NPCs cerca → **como mucho 3 burbujas** en pantalla, cada una sobre
  su NPC.
- **S4**: aterrizar en Marte → los nativos están **a la vista**, no en `(0,0)`. Matar a
  todos, irse, volver: el cuerpo **no** re-siembra con reputación limpia.
- **S5**: `HA_DAYPHASE=0.80` → los NPCs caminan hacia sus casas; `0.95` → están adentro y
  hay ventanas encendidas.
- **S6**: dejar `HA_TESTER` corriendo 20 min en la Tierra, irse a la Luna y volver:
  `Stock` cambió, hay **una estructura nueva** y la población subió. Sin caída de fps.
- **S8/S9**: un hijo nace, crece a `Adult` en `ADULT_AT_DAYS`, y su `traits` es el promedio
  de los padres ±1 (test unitario, no captura).

---

## 9. Reglas inviolables de este subsistema

1. **Nadie se esfuma.** La muerte es visible y tiene causa. La presión evolutiva va por
   natalidad, no por mortalidad silenciosa.
2. **La memoria no se lava.** Ni matando a todos, ni recargando, ni cambiando de planeta.
3. **Un solo camino de escritura a disco**, con `live` incluido, siempre.
4. **Los NPCs lejos son datos, no entidades** — y aun así el mundo avanza.
5. **Nada de guiones.** Roles, cultos, guerras y migraciones **emergen** de necesidades y
   memoria. Si hace falta un `if npc.name == "..."`, el diseño está mal.
6. **ASCII-only** en todo lo que se dibuja.
7. **Determinismo** en nombres, siembra y cultura: mismo mundo, mismo resultado.
