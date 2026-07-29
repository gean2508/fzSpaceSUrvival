# Haciendo luces bonitas

Referencia: captura nocturna (farol sobre cofre + luciérnagas). Objetivo: **mismo look de iluminación** en nuestro Bevy 2D, sin reventar 60 FPS en el Redmi.

## Graphify (contexto)

- Grafo en `graphify-out/` (freschezza ~2026-07-26; `light2d` **no** está indexado → correr `graphify update .` al cerrar).
- Nodos útiles hoy: `NightOverlay` (`sky2d.rs`), cielo/día, `Emission` en assets.
- Dependencias reales (código, no grafo): `light2d` ← spawners (`world2d`, `structures`, `society`, `companion`) + `ambient` (reusa `GlowTexture`) + `sky2d` (velo) + `ArtLibrary.emission*`.

## Qué pide la imagen (desglose)

| Efecto | Qué se ve | Técnico |
|--------|-----------|---------|
| Piscina de luz | Tierra/pasto **con su color** dentro del radio | La oscuridad se **agujerea**, no se tapa con naranja |
| Bloom / aura | Núcleo caliente + halo suave en el emisor | Glow aditivo en el foco |
| Sprites reaccionan | Cara hacia la luz clara, espalda en sombra | Iluminación **por píxel** (normal fake o mapa) |
| Partículas | Luciérnagas con glow **y** micro-luz al piso | Cada una es emisor `Light2d` chico |
| Ambiente | Fuera del radio: casi negro, siluetas | Velo fuerte + sin aporte de luz |

## Qué tenemos hoy (y por qué no se parece)

```
sprites / Mesh2d terreno  →  NightOverlay α≈0.965  →  GlowQuad aditivo (z alto)
                              (aplasta el albedo)      (mancha naranja encima)
```

1. **`light2d.rs`**: halos aditivos (Core+Halo; Mist apagada) + `light_sprites` (multiplica `Sprite.color`).
2. **El terreno es `Mesh2d` + vertex color** (`world2d`), **no** `Sprite` → `light_sprites` **no lo toca**.
3. El velo casi opaco tapa el albedo; el glow encima se lee como **disco naranja**, no como “se ve el piso iluminado”.
4. Luciérnagas (`ambient.rs`): solo quads de glow; **no** insertan `Light2d` → no iluminan alrededor.
5. **No hay normal maps ni shader lit** → no hay cara clara / espalda oscura.

Mito a aclarar: “hay que cambiar los PNG”. **No es lo primero.** Lo primero es **revelar** el mundo bajo la luz. Los normal maps (o normales esféricas fake en shader) son la fase de *relieve*, no del pozo de luz.

## Arquitectura objetivo

```
mundo (tiles Mesh2d + sprites)
    ↓
NightOverlay CON AGUJEROS (shader: α *= 1 - falloff luces)  ← reveía albedo
    ↓
Glow Core/Halo/Mist aditivo suave                          ← bloom del emisor
    ↓
light_sprites / LitSprite (cálido + direccional)           ← relieve en sprites
    ↓
HUD
```

Presupuesto (mantener): `MAX_LIGHTS` 28 Android / 96 desktop, `LIGHT_HZ` 20, una textura de glow compartida, `HA_NO_LIGHTS=1`.

## Fases de ejecución

### Fase A — Agujeros en el velo (impacto visual #1)
- Convertir `NightOverlay` a `Mesh2d` + `NightVeilMaterial` (uniforms: color + hasta N luces en espacio cámara).
- Cada tick: volcar luces activas (`Light2d` + cam) al material.
- Fragment: `alpha = veil_a * Π(1 - smooth_falloff(d, radius*k) * strength)`.
- Resultado: el piso y props **muestran su color** dentro del radio (como la foto).

### Fase B — Bloom / mist del emisor
- Reactivar capa **Mist** (ancha, α muy baja) + subir un poco Core.
- Glow sigue encima del velo: solo “aire caliente” / bloom, ya no hace el trabajo de revelar.
- Ajustar `shape()` para que no lave el sprite del emisor.

### Fase C — Partículas emisoras
- Luciérnagas: insertar `Light2d` chico (radio ~1.2–2 m, verde lima, flicker, intensity baja).
- Entran al mismo presupuesto/culling; iluminan un poquito de pasto (agujero + tint).

### Fase D — Sprites que “reaccionan” (sin re-pintar assets)
- **No** regenerar PNGs todavía.
- Mejorar `light_sprites`: aporte más fuerte y cálido ahora que el velo agujerea (se nota).
- Añadir shader `LitSprite` opcional con **normal esférica fake** desde UV (`n = normalize(vec3(uv*2-1, z))`) + mismas luces uniforms — look “cara lit / espalda shadow” sin normal maps.
- Opt-in por componente `LightReceiver` en jugador / NPCs / props cercanos (no migrar todo el mundo de golpe).
- **Fase E (después, assets):** normal maps reales en Image Lab si el fake no alcanza.

### Fase F — Cierre
- `graphify update .`
- Probar con `HA_DAYPHASE=0` (noche).
- No tocar UI / HUD / controles.

## Fuera de alcance (ahora)

- Shadow maps / oclusión real entre árboles.
- Bloom HDR de post-process Bevy (caro en mobile; el mist aditivo alcanza).
- Reescribir todo `Sprite` del juego a `Mesh2d`.
- Cambiar arte de tiles.

## Criterio de éxito

De noche, junto a un farol/fogata/`emission`:
1. Se ve **pasto/tierra con color** en un círculo suave (no mancha naranja opaca).
2. El emisor tiene **núcleo + aura**.
3. Luciérnagas dejan un **micro-pozo** verde.
4. Jugador/NPC cerca del foco se **aclara** (tinte cálido; relieve per-píxel = Fase E).
5. Android sigue estable (sin subir `MAX_LIGHTS`).

---

## Estado de ejecución (2026-07-27)

| Fase | Estado | Qué quedó |
|------|--------|-----------|
| A Agujeros velo | ✅ | `night_veil.wgsl` + `NightVeilMaterial`; `NightOverlay` es `Mesh2d`; `tick_lights` llena hasta 16 luces |
| B Bloom/mist | ✅ | Mist reactivada; Core/Halo retocados |
| C Partículas | ✅ | Luciérnagas con `Light2d { draw_glow: false }`; intensity late con el beat |
| D Sprites | ✅ parcial | `light_sprites` más fuerte + sesgo lateral barato (sin normal map) |
| E Normal maps | ⏳ | Solo si el fake no alcanza — Image Lab / assets |
| F graphify | ⏳ | `graphify update .` tras probar |

Probar: `HA_DAYPHASE=0 cargo run -p ha-game` (noche) junto a un asset con `emission`.

---

## Pasada 2 (2026-07-28) — borde duro en móvil + "niebla negra"

Síntomas reportados: en el celular se veía **el borde de cada luz**, y la
oscuridad era *"una niebla negra densa que no deja ver nada"*, con los charcos
de luz como **huecos recortados** — lo contrario al efecto buscado.

### Las cuatro causas (ninguna era el arte)

1. **La α del velo no se dithereaba.** Un degradé de 0.965 → 0 sobre 8 bits deja
   anillos concéntricos; el más externo se lee como "el borde de la luz". En el
   móvil (banda de color más pobre) saltaba a la vista. → dither de ±1 LSB con
   hash por píxel en `night_veil.wgsl`, y dither ordenado 2×2 en la textura de
   glow (que además pasó de 96 a 160 px).
2. **El COLOR del velo, no su α, era la niebla.** Con α 0.965 y velo `0.010`
   *lineal*, el término constante aportaba más que el albedo del mundo: todo
   quedaba del mismo gris azulado, sin tono ni contraste. Medido contra la
   captura de referencia: zonas sin luz debían dar luminancia ~14/255, daban ~54.
   → velo casi negro (`0.0012, 0.0016, 0.0045`) **manteniendo α alta**: la mezcla
   pasa a ser ≈ `albedo × 0.035`, o sea un *oscurecimiento* que conserva el
   matiz. Sigue siendo de noche, pero el pasto sigue siendo verde.
   **Bajar `NIGHT_VEIL` NO es la solución** — da atardecer permanente.
3. **Cada sistema usaba su propia caída y su propio presupuesto.** El velo
   agujereaba con `smootherstep` a `radius*2.0` y las 16 primeras luces *de la
   query*; los sprites se teñían con `smoothstep` a `radius*2.6` y otras 16
   arbitrarias; el halo se dibujaba para hasta 96. Resultado: sprites aclarados
   fuera de todo charco, y auras flotando sobre suelo negro. → una sola
   `falloff()` gaussiana renormalizada, un solo `LIGHT_REACH`, un solo
   `cull_radius(view_h)` y un solo conjunto (`VEIL_LIGHTS`, ahora 24) para velo +
   halo + tinte.
4. **El halo tapaba en vez de iluminar.** Los `GlowQuad` son `Sprite` normales
   (blending por alpha): sobre una noche de verdad, α 0.32 se lee como mancha
   lechosa. → alphas a un tercio; quien revela el suelo es el agujero del velo.

### Zoom / encuadre

- Corte de luces por `cull_radius(view_h) + radius * LIGHT_REACH`: antes se
  contaba sólo `radius`, así que un farol apenas fuera de cuadro se apagaba con
  su charco todavía en pantalla → el pozo desaparecía de golpe al panear o
  alejar.
- Presupuesto por **importancia** (`light_priority` = distancia / potencia), no
  por distancia cruda: un bosque con decenas de luciérnagas se quedaba con los 24
  slots y dejaba al farol de al lado sin charco.
- Luciérnagas a `radius 0.7` (antes 1.5 → charco de 8 m, del tamaño del de un
  farol): micro-pozo, como pide la referencia.
- Verificado con `zoom 12 / 20 / 30 / 45` (comando nuevo del tester) + noche
  forzada: misma lectura a todo encuadre, sin anillos ni bordes.

### UI

El HUD es `bevy_ui` puro (`Node`, cero `Sprite`) → vive en otro pase de render y
**nada de esto lo toca**. Verificado en las capturas: hearts, hotbar, reloj y
botones quedan a contraste pleno con el mundo en negro. `light_sprites` además
ignora todo lo que esté sobre `WORLD_Z_MAX`.

### Herramienta de prueba

`HA_TESTER=1` acepta dos comandos nuevos por `tester-cmd.txt`:
`zoom <metros>` (fija `CameraZoom::view_h`) y `shot <nombre>`.

Comprobado: 106 tests verdes, `HA_NO_LIGHTS=1` sigue apagando todo sin panic.
