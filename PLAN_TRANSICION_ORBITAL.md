# Plan — Transición superficie ↔ espacio (todos los cuerpos)

> Estado: **IMPLEMENTADO (19 jul 2026)** — las 6 fases completas, verificadas por
> tests (98 verdes) + capturas (`HA_ROCKETTEST`/`HA_FLYTEST`/smoke). Detalle final
> en biblia §13-nonies. Desvíos vs el plan: (1) el swap de subida quedó en
> `swap_up = atmo*3.0` (360 m Tierra) y el de bajada en `swap_down = atmo*2.5`
> (300 m) — ambos DENTRO de la banda de cobertura llena; el `swap*0.85` original
> violaba esa invariante y un test lo detectó; (2) guardián + driver se fusionaron
> en `sync_flight_world_state` (un solo escritor, sin carreras de orden); (3) fix
> extra de geometría en órbita creativa (distancias XZ) que además habilitó el
> auto-aterrizaje en cuerpos chicos.
> Alcance: SOLO la transición despegue/salida al espacio/regreso, para la Tierra
> y todos los cuerpos celestes. No toca física de vuelo, ni combustible, ni UI.

---

## 1. Diagnóstico (por qué la transición "crea conflicto")

Hoy conviven **DOS transiciones distintas** para el mismo viaje:

| | Vuelo creativo (`creative_fly.rs`) | Cohete (`rocket_flight.rs`) |
|---|---|---|
| Subida | Gradual: zoom sigue altura, nubes tapan el swap a 300 m | **NINGUNA: corte instantáneo** |
| Región | Cambia a `Space` recién a `SWAP_ALT`, bajo nubes llenas | `sync_flight_world_state` pone `Space` **en el primer frame de ignición** (`rocket_flight.rs:485-502`) |
| Bajada | Fases Descend con nubes | Corte instantáneo al touchdown + teleport a spawn fijo |
| Por cuerpo | Curva de nubes **hardcodeada de la Tierra** (190/290/700 m) — la Luna sin aire también mostraría nubes | Nada |

Consecuencias concretas del lado cohete (lo que se siente como "conflicto"):

1. **Ignición = cutscene encubierta.** Al apretar DESPEGAR (pad `launch_pad.rs:373-383`
   o cabina `ship_interior.rs:345-361`), `RegionId` pasa a `Space` al instante:
   `world2d` despawnea TODOS los chunks (línea 608), `orbital2d` spawnea el planeta
   como círculo, y en un frame pasás de ver tiles a ver un disco con un puntito.
   Exactamente el "corte feo" que B8 prohíbe y que en creativo ya se resolvió.
2. **El mecanismo previsto está muerto.** `RocketFlight.surface_flight` ("render
   stays in surface frame") se escribe en 3 lados pero **nadie lo lee**;
   `surface_ref_s` está declarado y jamás usado; `escape_alt()` solo gatea la
   velocidad máxima (`rocket_flight.rs:368`), no la transición. El comentario de
   cabecera del módulo ("render maps to the surface strip below escape altitude")
   describe un sistema que ya no existe.
3. **Cámara con salto.** `follow_player_camera` snapea `view_h` 30→320 al entrar
   a vuelo (`camera.rs:190-196`) — otro golpe visual encima del swap de región.
4. **Aterrizaje = teleport seco.** Touchdown (`rocket_flight.rs:390-421`) cambia
   región y teletransporta al walker a `walkable_spawn(body, 0, 6)` en el mismo
   frame. Sin descenso visible, sin nubes, sin continuidad.
5. **Nada es por-cuerpo.** Las nubes creativas usan constantes de la Tierra para
   todos los cuerpos (nubes en la Luna = mal, SFS-correcto es círculo pelado);
   y `sync_sky` (`sky2d.rs:141-155`) trata TODA superficie no-Tierra como vacío
   (`space_f=1.0`) aunque 6 cuerpos del catálogo tienen `atmo_top_m > 0`
   (120/80/60/55/50/45 m) — un planeta con atmósfera hoy tiene cielo negro.
6. **Biblia desincronizada.** §1 dice "despegar/aterrizar = zoom de cámara (no
   cambio de coords)"; §11 dice `SWAP_ALT 200`; el código real hace swap de
   región a 300 m bajo nubes. Hay que dejar UNA verdad escrita.

**Lo que SÍ está bien y no se toca:** la física (`tick_rocket_flight` siempre en
el plano mundial, SOI, drag, paracaídas), el modelo creativo probado por capturas
(suelo lleno → nubes → círculo), el streaming ancho (`range=10`) durante vuelo,
el overlay de nubes como UI Node bajo el HUD (`GlobalZIndex(-50)`).

---

## 2. Diseño objetivo — UNA capa de transición, compartida y por cuerpo

**Principio (refuerza B8):** la transición es SIEMPRE la misma coreografía,
la use el cohete o el personaje creativo, en la Tierra o en cualquier cuerpo:

```
superficie (tiles, zoom cerca)
   → ascenso: zoom sigue la altura, cielo se funde, cobertura entra (nubes si hay atmósfera)
   → swap de marco a swap_alt, TAPADO por la cobertura (nunca se ve)
   → espacio (vista orbital SFS: el cuerpo emerge como círculo y se achica)
   → descenso: lo mismo exacto en reversa, sobre CUALQUIER cuerpo aterrizable
```

La física nunca se entera: el swap es solo render/región (como hoy en creativo),
y el input jamás se bloquea.

### 2.1 `ha-shared/src/transition.rs` — perfil por cuerpo (NUEVO, testeable)

```rust
pub struct TransitionProfile {
    pub swap_alt: f32,        // altura del cambio de marco superficie↔orbital
    pub cover_start: f32,     // la cobertura empieza a entrar
    pub cover_full: f32,      // cobertura llena (tapa el swap)
    pub clear_alt: f32,       // despeja del todo, ya en espacio
    pub sky_fade_end: f32,    // cielo 100 % vacío (estrellas)
    pub has_clouds: bool,     // atmo_top > 0
}
pub fn transition_profile(body: CelestialBody) -> TransitionProfile
```

- **Derivado de `atmo_top_m`** con la Tierra como calibración: los factores se
  eligen tal que Tierra (atmo 120) reproduzca EXACTO los números ya tuneados y
  verificados por capturas (cover 190→290, swap 300, clear 700, fade 900).
  Ej.: `cover_start = atmo*1.58`, `cover_full = atmo*2.42`, `swap = atmo*2.5`,
  `clear = atmo*5.83`, `fade = atmo*7.5`. Un planeta con atmo 60 transiciona a
  mitad de altura — más chico, más rápido al espacio: correcto estilo SFS.
- **Cuerpos sin aire (Luna, asteroides):** `has_clouds = false`. Sin nubes:
  cobertura = un **fundido breve de pantalla** (mismo overlay UI, color oscuro,
  banda corta alrededor de un `swap_alt` piso de ~140 m — el mínimo donde el
  terreno con `range=10` todavía llena `SWAP_VIEW_H`). Círculo pelado sobre
  negro al salir: SFS-correcto, y es el "fundido breve" que la biblia ya
  preveía para cuerpos airless (§11 histórico).
- **Color de cobertura por cuerpo** (campo `cover_color`): Tierra blanca,
  planetas con atmósfera fina un tinte propio (ej. salmón marciano) — gratis y
  vende la identidad de cada cuerpo.
- **Histéresis integrada:** `swap_up = swap_alt`, `swap_down = swap_alt*0.85`
  (helpers en el perfil) para que flotar justo en el borde (p. ej. autopilot
  hover a 500 m en un cuerpo chico) no haga ping-pong de región.
- Tests en `ha-shared`: Tierra == números tuneados; monotonicidad
  (cover_start < cover_full ≤ swap < clear < fade); airless sin nubes con piso
  de swap; gap de histéresis > 0. (Native-only: no toca `packages/shared`, no
  hay paridad TS que regenerar.)

### 2.2 `ha-game/src/transition.rs` — recurso compartido (NUEVO)

```rust
#[derive(Resource, Default)]
pub struct FlightTransition {
    pub mode: Mode,           // None | Ascent | Descent  (solo informativo/HUD)
    pub body: CelestialBody,
    pub alt: f32,
    pub cover: f32,           // 0..1 → overlay de nubes/fundido (sky2d)
    pub sky_blend: f32,       // 0..1 → fade del ClearColor a vacío (sky2d)
    pub view_h: Option<f32>,  // zoom pedido mientras transiciona (camera)
    pub surface_anchor: Vec2, // celda de pad/aterrizaje en el marco superficie
}
```

Un solo **escritor por frame** (el vuelo activo: cohete O creativo — son
mutuamente excluyentes, `creative_fly_enter` ya chequea `flight.active`) y
lectores pasivos:

- `sky2d::sync_cloud_overlay` lee `cover` + `cover_color` (hoy lee
  `CreativeFlight::cloud` — se desacopla).
- `sky2d::sync_sky` lee `sky_blend` (reemplaza a `LaunchAtmosphere`, que queda
  absorbido; migrar sus 2 lectores: `sky2d.rs`, `ui_editor.rs:595`).
- `camera::follow_player_camera` respeta `view_h` mientras `mode != None`
  (y se ELIMINA el snap a `FLIGHT_DEFAULT_H` cuando hay transición activa —
  el zoom queda continuo por construcción; el default 320 m queda solo para
  entrar a espacio sin transición, p. ej. teleports del MENÚ).
- `world2d::stream_chunks` amplía `range` a 10 cuando `mode != None` (además
  del `fly.flying()` actual).
- `rocket_flight::sync_flight_world_state` — ver 2.3.

### 2.3 Cohete: ascenso gradual (el fix principal)

En top-down, subir en vertical NO se mueve por el mapa: el cohete queda sobre el
pad y el mundo "se aleja". Es exactamente la coreografía creativa, así que el
cohete la reusa:

1. **Ignición** (pad, cabina, relanzamiento off-world): además de `begin_at`,
   guardar `surface_anchor` = posición render del sitio de lanzamiento (pad en
   Tierra, posición del walker en relanzamientos). Reemplaza al muerto
   `surface_ref_s` (se borra). `surface_flight` pasa a ser LEÍDO de verdad:
   marca "transición de superficie en curso sobre este cuerpo".
2. **`sync_flight_world_state` se vuelve el guardián B8 real:** mientras
   `alt < swap_up` sobre un cuerpo aterrizable, la región QUEDA en superficie
   (tiles vivos, pad visible). El swap a `Space` ocurre recién a `swap_up`,
   con `cover` llena tapándolo. La física ya corre en el plano mundial desde
   el frame 0 — solo se gatea la región/render (cero patadas, cero clamps).
3. **Render del cohete bajo `swap_alt`** (`sync_rocket_visual`): sprite en
   `surface_anchor` (marco superficie, y-sort correcto), escala constante;
   zoom `GROUND_VIEW_H → SWAP_VIEW_H` según `alt/swap_alt`; `cover` y
   `sky_blend` desde el perfil del cuerpo. Sobre `swap_alt`: exactamente el
   render orbital de hoy (`render_pos_2d`, escala por altura).
4. **HUD sin cambios** — la telemetría (radar/↕/↔) ya funciona igual en ambos
   marcos porque lee la física.

### 2.4 Cohete: descenso + aterrizaje gradual (todos los cuerpos)

1. **Pre-aterrizaje:** en `Space`, si `body_landable(flight.body)` y
   `alt < swap_down` → swap a la región superficie del cuerpo (reusa
   `set_surface_region`) CON `cover` llena (por perfil, ya está llena a esa
   altura). `surface_anchor` = **zona de aterrizaje del cuerpo** (Tierra: el
   pad; otros: la celda `walkable_spawn` estándar — una "zona de aterrizaje"
   fija y deterministica por cuerpo, que es lo que el touchdown ya hace hoy;
   mapear el ángulo orbital a la grilla 2D queda explícitamente FUERA de este
   plan). El terreno streamea alrededor del anchor mientras las nubes tapan.
2. **Bajada visible:** cohete descendiendo sobre el anchor, zoom cerrando
   `SWAP_VIEW_H → GROUND_VIEW_H`, nubes despejando según `alt`.
3. **Touchdown:** lógica actual intacta (velocidad segura, daño, walker al
   anchor) — pero ahora es el final de un descenso que se VIO, no un teleport.
   Si vuelve a subir sin tocar (aborta), la histéresis lo devuelve a `Space`
   a `swap_up` — relanzar desde el aire ya funciona porque la física nunca
   paró. Flush B10 en el swap (ya existe en las transiciones de vuelo).

### 2.5 Creativo: migrar a la capa compartida (sin cambiar la Tierra)

`creative_fly.rs` deja sus constantes privadas (`SWAP_ALT`, `cloud_from_alt`,
bandas 190/290/700/900) y consume `transition_profile(body)` + escribe
`FlightTransition`. En la Tierra los números son idénticos por calibración →
cero regresión sobre lo verificado por capturas. Lo NUEVO que gana gratis:
- En la Luna/asteroides: sin nubes, fundido breve, círculo pelado (hoy: nubes
  terrestres incorrectas).
- En planetas con atmósfera fina: bandas escaladas + color propio.
- `CreativeFlight::cloud` se elimina (lo reemplaza `FlightTransition.cover`).

### 2.6 Cielo por cuerpo en superficie (cierra el círculo)

`sync_sky`: en `RegionId::Luna`/`Planet(b)`, en vez de `space_f = 1.0` fijo:
- `atmo_top == 0` → vacío + estrellas (como hoy: correcto).
- `atmo_top > 0` → cielo tintado del cuerpo (color del perfil) con fade por
  altura escalado al perfil (como la Tierra hace con 150→1200). Así al
  aterrizar en un planeta con aire, el cielo que atravesaste en el descenso
  es el mismo que ves parado en el suelo — continuidad completa ida y vuelta.

---

## 3. Qué NO cambia (anti-desastre, god-node degree 60)

- `tick_rocket_flight` (física, SOI, drag, paracaídas, autopilot, fuel): **intacto**
  salvo guardar `surface_anchor` y el touchdown apuntando al anchor.
- Los ~40 lectores de `RocketFlight` (HUD, survival O₂ presurizado, máquinas,
  menú…): la struct solo pierde el campo muerto `surface_ref_s`.
- `orbital2d`, ship builder, nav computer, persistencias: intactos.
- Nada de `packages/shared` (TS): esto es geometría/render del cliente nativo.

## 4. Orden de implementación (cada paso compila + tests verdes)

| Paso | Contenido | Riesgo |
|---|---|---|
| **F1** | `ha-shared/transition.rs` (perfil + tests) + `ha-game/transition.rs` (recurso) | nulo (nada lo consume aún) |
| **F2** | Migrar creativo + sky2d + camera + world2d al recurso compartido; borrar `LaunchAtmosphere` y `CreativeFlight::cloud` | bajo (misma Tierra por calibración; verificar con flytest) |
| **F3** | Ascenso gradual del cohete (2.3) — pad, cabina y relanzamiento off-world | medio (tocar `sync_flight_world_state` / `sync_rocket_visual`) |
| **F4** | Descenso/aterrizaje gradual (2.4) con histéresis | medio |
| **F5** | Cielo por cuerpo en superficie (2.6) | bajo |
| **F6** | Biblia: reescribir §1 nota top-down + §11 (números reales), fila en §4, header de `rocket_flight.rs`; `graphify update .`; commit | — |

## 5. Verificación

- `npm run test:native` (tests nuevos del perfil + los 90 existentes).
- **Desktop con capturas (§11):** el harness `HA_FLYTEST` ya prueba el ascenso
  creativo; se agrega `HA_ROCKETTEST=1` en `debug/autodrive.rs` (ya ignicia
  cohetes en la línea 390): despegue del pad → capturas seriadas (suelo lleno →
  nubes → círculo) y descenso → (círculo → nubes → pad). Mismo criterio visual
  que validó el modelo creativo.
- Checklist device (Redmi, cuando esté conectado): despegue pad, despegue
  cabina, salida a espacio, aterrizaje Luna (sin nubes, fundido), relanzamiento
  lunar, regreso y aterrizaje Tierra, hover autopilot cerca del borde de swap
  (sin flicker de región).

## 6. Riesgos conocidos

- **Tuning de bandas en cuerpos chicos:** los factores derivados pueden requerir
  ajuste en device (§6: no tunear vuelo a ciegas) — por eso van centralizados en
  UN perfil, no regados por el código.
- **Velocidad del cohete ≫ creativo:** el cohete cruza la banda de nubes mucho
  más rápido (thrust real). Si el paso por nubes resulta demasiado corto para
  tapar el swap, el perfil admite ensanchar `cover_full→clear` sin tocar código.
- **Doble escritor de zoom:** cámara ya tuvo la guerra pinch-vs-creativo
  (flicker §11) — la regla queda única: si `FlightTransition.mode != None`, la
  transición manda; si no, el usuario. Un solo `if`, un solo dueño.
