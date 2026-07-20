# Plan — Zoom continuo que revela los planetas (sin cortes, mismo cuerpo)

> Estado: **HECHO Y VERIFICADO VISUALMENTE (20 jul 2026)**. El usuario: los
> planetas ya no se ven al salir de las nubes (regresión que YO metí), y quiere
> que el disco crezca al acercarse y sea el MISMO cuerpo que ves = el que pisás,
> sin cortes, en la nave y en creativo, en TODOS los cuerpos.
>
> **Verificado por capturas (no solo logs):**
> - Tierra creativo: al despejar las nubes se ve el **disco azul** (antes negro).
> - Luna creativo: se ve el **disco gris**; lejos chico, cerca grande (crece al
>   acercarse). Sin cuadrado blanco, sin loop.
> - Cohete: en el espacio ve la **Tierra como disco azul con halo de atmósfera**.
> - Continuidad de zoom VISIBLE: x1.4 máx (el zoom-out grande a escala de disco
>   ocurre bajo cobertura casi opaca, tapado — el test ahora solo marca saltos
>   con cover<0.5). 98 tests ha-shared, flytest, moontest, rockettest, smoke PASS.
> - **Fundido de cuerpos sin aire = color del cuerpo** (gris para la Luna) en vez
>   de negro → el swap tile↔círculo bajo el fundido es invisible (gris→gris).

## 1. Causa raíz (una sola)

Mi commit `7d7b580` cambió el zoom orbital a un formato "anclado"
(`SWAP_VIEW_H * (dist/dist_swap)^1.6`) para evitar el salto de cámara. Efecto
colateral: al despejar las nubes el viewport queda en ~300 m, pero la Tierra
tiene radio 1500 m → **el disco queda fuera de pantalla, ves negro/vacío**. Lo
mismo la Luna (moontest-clear mostró espacio negro sin Luna). Antes el zoom era
`(radio+alt)*4.5` que SÍ mostraba el planeta como disco (lo que al usuario le
gustaba), pero ese zoom grande daba un SALTO brusco en el swap (la queja
original que me llevó a anclar). Las dos cosas se pueden tener a la vez con un
zoom **continuo por altitud**.

## 2. Diseño — UN zoom continuo de la altitud, del suelo al espacio profundo

Función única `flight_view_h(body, alt)` usada por TODO (creativo climb/orbit/
descend Y cohete), vía `FlightTransition`:

```
orbital(alt) = (radio + alt) * ORBIT_REVEAL_FACTOR   // muestra el disco (SFS)
si alt >= swap_up:
    orbital(alt)                    // el disco CRECE al bajar, se achica al subir
si no (bajo el swap):
    rise = GROUND + (SWAP_VIEW_H - GROUND) * (alt/swap_up)   // 30→180, sensación de subir
    zf   = smoothstep(0.9, 1.0, cover_at(alt))               // 0 hasta cover casi lleno
    lerp(rise, orbital(alt), zf)     // bajo cobertura llena → orbital; visible → rise
```

**Por qué funciona (las 3 propiedades que pide el usuario):**
1. **Continuo en el swap:** donde `cover==1` (banda `[cover_full, clear_start]`,
   que contiene swap_up y swap_down), la rama de abajo da EXACTAMENTE
   `orbital(alt)` = la rama de arriba. Cero salto, suba o baje, en cualquier
   cuerpo. Y en el suelo (`cover==0`) da `rise`→30 (zoom de caminar).
2. **Planeta visible + crece al acercarse:** en órbita `orbital(alt)` =
   `(radio+alt)*FACTOR`. Bajás (alt↓) → viewport↓ → el disco (tamaño mundial
   fijo) llena más pantalla = crece. Subís → se achica. El disco es el círculo
   real de `orbital2d` en la posición real del cuerpo — el MISMO que pisás al
   aterrizar (aterrizar = swap a la superficie de ESE cuerpo).
3. **Sin mostrar el borde finito del terreno:** `zf=smoothstep(0.9,1)` mantiene
   el zoom ≤180 (el terreno con `range=10` llena eso) hasta que la cobertura
   está casi opaca; recién ahí zoomea a escala orbital, tapado. En cuerpos con
   aire = nubes; sin aire = el fundido oscuro (misma banda) tapa el zoom-out y
   el swap. El usuario acepta las nubes en la Tierra; en airless el fundido es
   el análogo (breve, tapa el swap tile↔círculo → sin corte visible).

`ORBIT_REVEAL_FACTOR ≈ 2.5` (Tierra a alt=swap_up: `(1860)*2.5=4650`, disco de
3000 m = ~64% de pantalla; se afina MIRANDO capturas).

## 3. Cambios (acotados, unificados por `FlightTransition`)

- **`transition.rs`:** `ORBIT_REVEAL_FACTOR` + `flight_view_h(body, alt)`.
  `FlightTransition::drive` set `view_h = Some(flight_view_h(...))` para TODOS
  los modos (Ascent/Descent/Space), no solo superficie.
- **`camera.rs`:** cuando `trans.view_h` está seteado (todo vuelo), la cámara lo
  usa; en modo Space multiplica por `user_zoom_mult` (pellizco). Quita el snap a
  `FLIGHT_DEFAULT_H` mientras hay transición.
- **`creative_fly.rs`:** `creative_fly_orbit` deja de escribir `zoom.view_h`
  (lo maneja drive+camera). Mantiene movimiento (JUMP/USE radial, stick),
  arming y el swap a Descend en `swap_down` (cover=1 → continuo). Climb/Descend
  ya llaman `drive`, heredan el zoom nuevo.
- **`rocket_flight.rs`:** ya llama `drive(Space,...)` en órbita → hereda el zoom
  que revela el planeta (antes el cohete en espacio quedaba a 320 m sin ver el
  cuerpo). El sprite del cohete escala con el zoom para no desaparecer.

## 4. Qué NO se toca
Física de vuelo, perfiles de transición (`ha-shared`), aterrizaje/despegue
libre, sombras, cap de velocidad. Distancias del sistema solar. El cohete real.

## 5. Verificación (visual, exhaustiva — el usuario lo pidió)
- `HA_FLYTEST` (Tierra): capturas al despejar → **debe verse el disco azul**;
  seguir subiendo → se achica; bajar → crece. Log `view_h` continuo (sin saltos).
- `HA_MOONTEST` (Luna): al despejar el fundido → **debe verse el disco de la
  Luna** (no negro); acercarse → crece; aterrizar → misma Luna, sin corte.
- `HA_ROCKETTEST`: el cohete en espacio ve el planeta; despegue/aterrizaje
  continuos.
- Smoke completo. 98 tests ha-shared.
- Revisar CADA captura (no confiar solo en logs). Afinar FACTOR + umbral
  smoothstep mirando.
