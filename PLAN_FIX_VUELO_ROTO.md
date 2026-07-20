# Plan — Corregir vuelo creativo roto + velocidad de viaje

> Estado: **HECHO Y VERIFICADO (20 jul 2026)**. Prioridad absoluta: corregir los
> bugs (el usuario: "sí o sí corrige todos los errores primero"). La velocidad/
> distancia es complemento. **No se rompió nada de lo que ya servía** (cohete,
> aterrizaje libre, transición gradual, sombras — todo del turno anterior
> intacto, confirmado por rockettest + smoke).
>
> **Verificación:** 98 tests ha-shared · `HA_FLYTEST` ahora exige que las nubes
> se DESPEJEN en órbita (antes solo comprobaba entrar a fase Orbit, punto ciego)
> → PASS, cover=0.00, zoom continuo x1.09 · nuevo `HA_MOONTEST`: Luna llega a
> órbita despejada sin loop de "cuadrado blanco" (cover=0.00) + mide crucero ·
> `HA_ROCKETTEST` + smoke completos sin regresión.
>
> **Detalle no reportado que quedó pendiente (NO tocado, fuera de alcance):** el
> HUD tiene decenas de prompts con caracteres no-ASCII (`→ · í`) que la fuente
> del juego no renderiza (salen cajas □) — issue preexistente y generalizado
> (biblia §7 fuente ASCII-only). Arreglarlo es una limpieza aparte de `hud.rs`.

## 1. Diagnóstico (todos comparten UNA causa)

Mi commit anterior (`7d7b580`, "cámara continua") quitó el `entry_alt =
clear_end + 60` que ponía la entrada a órbita POR ENCIMA de las nubes, y lo
reemplazó por entrar en `swap_up` para lograr continuidad de zoom. Eso rompió el
vuelo porque **`creative_fly_orbit` NO tiene control vertical** — solo lateral
(`intent.move_x/z`). Entonces:

- **Tierra — "nunca salgo de las nubes":** entrás a órbita en `swap_up` (360m,
  `cover=1.0`, nubes llenas). El único movimiento es lateral (tangencial), que
  casi no cambia la altitud → las nubes NUNCA se despejan. Atrapado.
- **Luna — "cuadrado blanco":** `swap_up=165` < `APPROACH_LAND_M=200`, así que
  apenas entrás a órbita ya cumplís `touching` → `teleport_to_body` te devuelve
  a la superficie en loop. El "cuadrado" es el disco de la Luna llenando la
  pantalla / el overlay oscuro, reteletransportando cada frame.
- **Botón ESPACIO:** `GoSpace` enciende un `RocketFlight` coast en
  `orbital_start_above_pad()` = 336m de altitud, DENTRO de la banda de nubes
  (`clear_start=400`). `sync_flight_world_state` maneja `cover=1.0` a esa altura
  → ves nubes, no espacio. Y el cohete sin empuje cae.

## 2. Correcciones

### F1 — Control vertical en órbita (arregla nubes + luna)
`creative_fly_orbit` gana movimiento **radial**: JUMP = subir (alejarse del
cuerpo más cercano), USE-mantenido = bajar (acercarse). Misma `speed` que el
lateral. Así el ascenso es UNO continuo: mantenés JUMP desde el suelo, cruzás el
swap bajo nubes llenas, y **seguís subiendo** con JUMP → las nubes se despejan
gradualmente revelando el planeta como disco (justo lo que el usuario pidió:
"que se difumine hasta ver el planeta completo desde el punto donde salimos"),
sin cortes. La continuidad de zoom del commit anterior se conserva (zoom sigue
la altitud, 180 en `swap_up` creciendo).

**Arming de aterrizaje** (`CreativeFlight.land_armed`): al entrar a órbita =
`false`; se pone `true` una vez que `alt_orbit > clear_end` (ya despejaste). El
aterrizaje solo dispara con `land_armed && touching`. Esto elimina el loop de la
Luna (entrás a 165m, no armado → no aterriza; subís, despejás, armás; después
podés descender a aterrizar) y quita el "USE-edge aterriza desde cualquier lado"
(confuso). Aterrizar ahora es: descendés (USE) hasta acercarte y toca solo.

### F2 — Botón ESPACIO a espacio real
`GoSpace` deja de encender un cohete a media nube. Ahora, en creativo, pone el
personaje en órbita libre sobre la Tierra a una altitud **por encima de
`clear_end`** (nubes despejadas, `land_armed=true`, region=Space). Consistente
con el modelo de vuelo creativo. (TIERRA/LUNA ya funcionaban vía
`teleport_to_body` — no se tocan.)

### F3 — Sin cambios al cohete
El `RocketFlight` real (despegue desde el pad/cabina, física, SOI) NO se toca.
Ya alcanza el espacio bien (verificado en `rockettest`: alt 810m, cover 0,
region Space). Los bugs son EXCLUSIVOS del vuelo creativo del personaje.

## 3. Complemento — velocidad de viaje (~10 s, no instantáneo)

El "salgo en 2 segundos" es el vuelo creativo: `speed = view_h * 0.35`, y con
zoom de sistema (`view_h` hasta 2.000.000) da ~700.000 m/s = instantáneo. El
cohete real NO tiene este problema (cap 640 m/s → ~80 s a la Luna, ya
significativo).

**Fix:** cap a la velocidad orbital creativa (`MAX_ORBIT_SPEED`). Con un cap
alrededor de ~5.500 m/s, cruzar a la Luna (52 km) tarda ~10 s — el número que
pediste — y deja de ser instantáneo por más que zoomees afuera. Cuerpos más
lejanos tardan proporcionalmente más (tiene sentido: están más lejos, y para eso
construís mejores naves). **Se ajusta MIDIENDO** en un harness (no a ciegas).

**Distancias:** NO las agrando en esta pasada. Escalar las posiciones de los
cuerpos toca efemérides/órbitas/SOI (riesgoso), y el resultado que querés —que
viajar TENGA sentido temporal— se logra con el cap de velocidad sin ese riesgo.
Si después querés el sistema físicamente más grande, es un cambio aparte y
cuidadoso (un multiplicador global en `body_center_at` + `moon_center_at` +
`helio_radius` + SOI, con tests). Lo dejo anotado como follow-up.

## 4. Verificación
- `cargo test -p ha-shared` (no debería cambiar; se corre igual).
- `HA_FLYTEST` extendido: subir Tierra → confirmar que `cover` llega a 0 y
  `region=Space` DESPUÉS de seguir subiendo en órbita (antes se quedaba en 1.0).
- Nuevo `HA_MOONTEST`: teleport a Luna → subir → confirmar que NO hay loop de
  reteletransporte (llega a órbita despejada, no "cuadrado blanco").
- Medición de velocidad: volar lateral y cronometrar el cruce Tierra→Luna ≈ 10 s.
- `HA_ROCKETTEST` + smoke completos sin regresión.
