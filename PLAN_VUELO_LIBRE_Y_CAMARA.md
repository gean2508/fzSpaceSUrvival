# Plan — Vuelo libre sin cortes, cámara continua, sombras, panel fantasma

> Estado: **IMPLEMENTADO Y VERIFICADO (20 jul 2026)**. Alcance: los 4 bugs/pedidos
> concretos reportados en el vuelo creativo del player + una propuesta de diseño
> para el punto de escala/distancias (§5, NO implementado — queda como fase
> siguiente). Detalle final y desvíos vs el plan en biblia §13-decies.
> **Regla del usuario: no tocar nada que ya funcione.** Cada cambio fue
> quirúrgico y acotado a un archivo/sistema; cero refactors de física.
>
> **Verificación:** 98 tests `ha-shared` verdes · harness `HA_FLYTEST` extendido
> con máquina de estados (climb→orbit→fuerza mid-descend→abort JUMP→confirma
> vuelta a Orbit→aterriza) + detector automático de saltos de cámara — 3/3
> corridas deterministas, ratio máximo x1.10 (antes del fix: x56 en el peor
> caso) · `HA_ROCKETTEST` sin regresión · smoke `HA_AUTODRIVE` completo PASS.
> Un bug adicional (no reportado por el usuario) apareció y se corrigió en el
> propio harness de verificación: el primer INPUT sintético podía perderse en
> silencio si caía en un frame bloqueado por la pantalla de carga — se resolvió
> ordenando explícitamente el sistema de test antes del lector de teclado
> (`input/mod.rs`: `desktop_input` pasa a `pub mod` para permitirlo).

## 1. Diagnóstico (causa raíz de cada síntoma, no el síntoma)

**a) "El modal del panel de control aparece a cada rato sin razón"**
`ship_interior.rs::cockpit_seating` sienta al jugador (y esto BLOQUEA el
movimiento vía `sync_ui_gameplay_block`) con solo **pisar** la celda del asiento
`COCKPIT_CELL (305,300)` — sin apretar nada. El cuarto es chico (12×8) y el spawn
de abordaje `(305.5, 302.5)` queda a 2 celdas del asiento; caminar hacia la sala
de máquinas `FUEL_SOURCE_CELL (300,300)` (misma fila z=300) cruza cerca. Cualquier
paseo por la mitad norte del cuarto te sienta y te congela sin querer.

**b) "Cuando reingreso los controles dejan de servir, no puedo subir — baja
automático, al tocar el suelo sí puedo subir"**
`creative_fly.rs::creative_fly_descend` es **asimétrico** respecto a
`creative_fly_climb`: en Climb, soltar JUMP/USE flota (hover); en Descend,
soltar cae solo (caída suave por defecto) y **JUMP no sube, solo frena la caída
a velocidad 0** (nunca revierte a ascenso). No es un bug oculto: es que Descend
nunca te deja volver a subir — tenés que tocar tierra primero. Eso ES "los
controles dejan de servir".

**c) "No entiendo por qué la cámara tiene que cambiar al salir al espacio /
las nubes se cortan y aparece el círculo azul de golpe"**
Bug real de continuidad, no de diseño. Al cruzar `swap_up` en
`creative_fly_climb`, el código (mi propio commit de ayer) fuerza
`fly.alt = clear_end + 60` de un salto, y como `creative_fly_orbit` corre en el
MISMO frame (systems chained), el zoom pasa de `SWAP_VIEW_H` (180 m) a
`(radio+alt)*ORBIT_ZOOM_FACTOR` (~10.000 m para la Tierra) **en un solo frame**:
de ahí el "corte" — las nubes se despejan Y la cámara salta a un plano alejadísimo
en el mismo instante, así que ves el círculo azul chiquito con vos parado encima,
de golpe. La fórmula de zoom orbital nunca fue continua con el zoom de superficie
en el punto de swap — el salto ya existía antes de mi cambio de ayer (yo solo lo
agrandé). Se arregla anclando la fórmula orbital para que en `alt = swap_up` dé
EXACTO el mismo zoom que la superficie tenía ahí (180 m), y crezca desde ahí.

**d) "Nada que flote proyecta sombra"**
Cierto — no hay ningún sistema de sombra en el juego nativo salvo la de las
máquinas (`machines.rs`, un sprite oscuro fijo bajo el sprite). El jugador en
Climb/Descend y el cohete en ascenso/descenso no tienen nada que los ancle
visualmente al suelo.

## 2. Diseño de la corrección (complementado, no solo lo pedido)

**a) Panel de control → gate a un press explícito de USE**
`cockpit_seating` pasa a exigir `actions.use_edge` (apretar USE, no solo pisar)
estando parado en la celda, igual que la ignición del pad (`try_ignite_on_hold`)
ya exige HOLD explícito y no dispara solo por estar cerca. Complemento: agrego
un cartelito mínimo "USE: sentarse" (nuevo componente propio en
`ship_interior.rs`, sin tocar `hud.rs::build_interact_prompt` — esa función es
gigante y compartida, no vale la pena el riesgo por un hint) que aparece SOLO
parado en la celda y NO sentado, así sigue siendo descubrible.

**b) Descend simétrico a Climb — JUMP sí revierte a ascenso**
`creative_fly_descend` pasa a permitir que JUMP sostenido empuje `fly.alt` hacia
arriba de verdad (no solo `vy=0`) y, si cruza `swap_up` de nuevo, entrega de
vuelta a `Phase::Orbit` — el mismo hand-off que ya usa Climb. Se mantiene la
caída suave por defecto al soltar todo (así aterrizar sigue sintiéndose natural,
sin tener que sostener USE todo el trayecto) — el único cambio real es que JUMP
ahora es un control de verdad, no un freno. Con esto el "no puedo subir"
desaparece: en cualquier punto del descenso podés cambiar de opinión y volver
al espacio.

**c) Cámara continua en el swap (fix de raíz, no un parche visual)**
- Se elimina el salto `fly.alt = clear_end + 60`: `fly.alt` sigue siendo
  continuo al entregar de Climb a Orbit (el mismo valor con el que veníamos
  subiendo).
- La fórmula de zoom orbital deja de ser independiente: pasa a anclarse en el
  punto de swap. `dist_swap = radio + swap_up`; `zoom = SWAP_VIEW_H *
  (dist_actual / dist_swap)^1.6`, clamped a `[SWAP_VIEW_H, ORBIT_ZOOM_MAX]`. En
  `alt = swap_up` da exactamente 180 m (el mismo zoom que tenías parado mirando
  el pad) — CERO salto — y sigue creciendo con la altura para el efecto SFS de
  "el círculo se aleja/aparece el sistema" que igual te encantó, solo que ahora
  se revela gradual, nunca de un salto.
- Mismo criterio en el aterrizaje: la entrada a `Descend` ya no fuerza
  `fly.alt = swap_down`, usa la altitud real medida en el momento del "touching"
  (siempre <200 m ahí), así el primer frame de superficie arranca del mismo
  número que traías en órbita.
- **Complemento a "debería conservar la misma cámara":** hoy en órbita el pellizco/
  scroll del usuario está BLOQUEADO por completo mientras `fly.flying()` — inconsistente
  con el cohete (que sí te deja pellizcar una vez en el espacio puro). Habilito
  pellizco/scroll como un MULTIPLICADOR sobre la fórmula automática, solo durante
  `Phase::Orbit` (Climb/Descend siguen bloqueados — ahí la transición necesita
  dueño único del zoom para no repetir el flicker de §11). Así, si preferís
  mantener un encuadre más cercano "como en tierra" navegando entre planetas,
  podés pellizcar para achicarlo vos — sin perder el auto-reveal que dijiste que
  te encanta (multiplicador default = 1.0, cambia solo si vos tocás la pantalla).

**d) Sombras**
Sprite oscuro simple (mismo idioma visual que `machines.rs`: óvalo semitransparente
`srgba(0,0,0,α)`), spawneado una vez, sincronizado cada frame:
- **Jugador:** visible mientras `fly.flying() && !fly.in_orbit()` (Climb/Descend,
  o sea sobre terreno visible de CUALQUIER cuerpo) — no en `Orbit` (ahí no hay
  suelo bajo el personaje, es un puntito contra el disco, una sombra no tendría
  sobre qué caer). Tamaño y alpha decaen con la altitud (grande y oscura en el
  suelo, se achica y desvanece según subís) — refuerza la sensación de altura.
- **Cohete:** visible mientras `trans.in_surface_frame()` (ascenso/descenso
  bajo el swap, en pad o en cualquier aterrizaje), misma curva de tamaño/alpha
  por altitud, posicionada en `surface_anchor`.
- **"Cuando esté con luz":** alpha adicional × `DayClock.daylight()` en la
  Tierra (se desvanece de noche, como una sombra real) — en otros cuerpos, que
  hoy no tienen ciclo día/noche propio, se muestra siempre (no hay un estado de
  "luz" por-cuerpo del que colgarse sin inventar uno nuevo; anotado como mejora
  futura si se agrega ciclo día/noche por cuerpo).

## 3. Qué NO se toca en esta pasada

- La física de vuelo (thrust/coast/SOI/drag) — cero cambios.
- El sistema de transición por cuerpo (`ha-shared/transition.rs`, perfil,
  histéresis) — se usa tal cual, no se retunean sus números.
- El aterrizaje/despegue libre (ángulo real, sin zonas fijas) del turno anterior.
- El zoom del COHETE durante Ascent/Descend (`trans.view_h`) — ya era continuo
  por construcción (interpola `GROUND_VIEW_H→SWAP_VIEW_H` linealmente); el bug
  de salto era EXCLUSIVO del vuelo creativo del player (fórmula orbital propia).

## 4. Verificación

- `cargo test -p ha-shared` (no debería tocarse nada de ahí, pero se corre igual).
- `HA_FLYTEST=1 HA_AUTOPLAY=1 cargo run -p ha-game --features desktop` — capturas
  de la subida completa; ahora además loguea el `zoom.view_h` en cada beat para
  confirmar que no hay salto (`Δview_h` entre frames consecutivos acotado).
- `HA_AUTODRIVE=1` smoke completo (no debería romperse por estos cambios).
- Prueba manual dirigida (vía captura): Climb → Orbit → Descend sobre la Luna →
  a mitad de Descend, sostener JUMP y confirmar que vuelve a `Phase::Orbit`
  (log `ha-creative: reached orbit`) sin tocar tierra primero.
- Cockpit: caminar cerca del asiento SIN apretar USE y confirmar que el panel
  NO aparece; apretar USE parado en la celda y confirmar que SÍ aparece.

## 5. Propuesta para el punto de escala/distancias (NO implementado hoy)

Tenés razón en el riesgo: con ship builder libre (motores/piezas sin límite,
estilo SFS) y sistema solar en escala de juego (no real), llegar rápido entre
cuerpos acorta el gameplay. La solución correcta **no es** solo "agrandar las
distancias a escala real" — sin más, eso solo cambia el problema por el opuesto:
viajes de horas reales sin nada que hacer en el medio (lo que rompió el ritmo
de KSP/SFS antes de que ambos agregaran **time warp**). La solución real, que
proponen los dos juegos de referencia: **aceleración de tiempo mientras coasteás**
(sin empuje, lejos de un cuerpo) — mantener un botón acelera la simulación
×10/×50/×1000, se cancela solo al tocar el mando o entrar en la SOI de un cuerpo.
Con eso SÍ se puede llevar las distancias a una escala mucho más real sin matar
el ritmo: los tramos "vacíos" se aceleran, los tramos con maniobra/aterrizaje
siguen en tiempo real. Es un sistema nuevo no trivial (tocar el loop de física
+ death por el guard B8 de "nunca bloquear input/cámara" — el warp debe seguir
siendo interrumpible al instante) — lo dejo como **fase siguiente**, separada de
esta tanda de fixes, para no meter riesgo de física en el mismo commit que las
4 correcciones de arriba.
