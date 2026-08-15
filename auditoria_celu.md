# Auditoría del celu — Redmi Note 11, 15 ago 2026

Medido sobre el **APK 1.0.347** ya instalado (14 ago 21:05), por USB. Ninguna
medición de acá viene de la PC: todo salió del teléfono.

| | |
|---|---|
| Equipo | Redmi Note 11 (`selene` / 21121119VL) |
| SoC | MediaTek Helio G88 (`MT6769H`), GPU Mali-G52 |
| RAM | 3.797.996 kB (3,80 GB) + 4,00 GB de ZRAM |
| Android | 13 (MIUI) |
| Instrumentos | `logcat`, `dumpsys meminfo · gfxinfo · wallpaper`, `/proc/meminfo`, `ps`, `top`, y el contador propio `ha-perf` |

---

## 0. El resumen

Los tres síntomas reportados —**se lleva el fondo de pantalla**, **va lento**,
**se traba**— **no son tres bugs: son un solo hecho medido**. Al entrar al mundo
el juego pide cerca de un gigabyte en un teléfono que tenía 520 MB disponibles.
Todo lo demás es consecuencia.

| Medida | Valor | |
|---|---:|---|
| FPS en juego | **28–39** | la regla del proyecto son 60 |
| Frame | 25,7–36,1 ms | jitter hasta 7,7 |
| Pico de RSS del juego | **997 MB** | durante «cargando texturas» (ver el aviso de abajo) |
| PSS del juego en juego | 191 MB | de los cuales 98 MB están en swap |
| RSS estable en juego | 872 MB | + 153 MB propios swapeados |
| Swap del sistema en uso | **1,94 GB** | de 4,00 GB de ZRAM |
| `MemAvailable` del sistema | 520 MB → **258 MB** | −50 % al entrar al mundo |
| Procesos que mató el kernel | **9** | en 8 segundos |
| CPU del juego | 187 % | 67 hilos |
| Entidades vivas | 5.087 | **4.263 son de chunk (84 %)** |

> ⚠ **Cuidado con qué número se cita.** `ps -o RSS` da 872 MB en juego y
> `dumpsys meminfo` da 191 MB de PSS con 137 MB de `Rss Total`. No se
> contradicen: miden cosas distintas. `ps` cuenta todas las páginas residentes
> del proceso —incluidas las del APK mapeado y las reservas de la GPU— y PSS
> reparte lo compartido entre quienes lo comparten. **La cifra honesta del "peso
> propio" del juego es el PSS (191 MB); la cifra honesta de "cuántas páginas
> físicas tiene que encontrar el kernel" es la RSS.** El síntoma que se ve —el
> fondo que desaparece— lo produce la segunda, no la primera.
>
> Y lo que no depende de cómo se atribuya nada: **`MemAvailable` se partió al
> medio, el LMK mató 9 procesos y el swap llegó a 1,94 GB.**

---

## 1. El fondo de pantalla: no lo borra el juego, se lo comen

El fondo lo dibuja **`com.miui.miwallpaper`, un proceso aparte** del launcher
(`dumpsys wallpaper` →
`mWallpaperComponent=ComponentInfo{com.miui.miwallpaper/…ImageWallpaper}`).
Esa conexión ya tiene un `mLastDiedTime` registrado.

Lo que se midió mientras cargaba el mundo:

| Proceso | Antes | Después | |
|---|---:|---:|---|
| `com.hungerarena.app` | 490 MB | 872 MB | creció 78 % |
| `com.miui.home` (launcher) | 116 MB | 60 MB | **le sacaron 48 %** |
| `com.miui.miwallpaper` (el fondo) | 52 MB | 36 MB | **le sacaron 30 %** |
| `MemAvailable` del sistema | 520 MB | 258 MB | −50 % |

El kernel no los mató, pero les **arrancó las páginas**. Sin su bitmap residente,
MIUI redibuja el escritorio sin fondo. Desde afuera se ve como "el juego me borró
el fondo"; por dentro es presión de memoria.

**El teléfono ya estaba al borde antes de abrir nada**: con el juego cerrado,
`MemFree` 83 MB y `MemAvailable` 520 MB.

---

## 2. La matanza, tal como quedó en el log

Ocho segundos de `logcat` al entrar al mundo. **Ningún proceso del juego murió;
murió todo lo demás.**

```
06:56:25.958 lowmemorykiller: Kill 'com.miui.msa.global' (22708) to free 83212kB rss, 39824kB swap; reason: cw
06:56:26.062 lowmemorykiller: Kill 'com.android.camera' (22839) to free 79416kB rss, 39556kB swap;
             reason: critical pressure and device is low on memory
06:56:26.403 lowmemorykiller: Kill 'com.android.htmlviewer:remote' (22877) to free 50148kB rss, 42404kB swap; reason: cw
06:56:26.627 lowmemorykiller: Kill 'com.xiaomi.bluetooth'  (22500) to free 48336kB rss, 46032kB swap; reason: cw
06:56:26.709 lowmemorykiller: Kill 'com.modemdebug'        (22892) to free 43060kB rss, 43368kB swap; reason: cw
06:56:26.806 lowmemorykiller: Kill 'com.xiaomi.mtb'        (22894) to free 45000kB rss, 42956kB swap; reason: cw
06:56:31.539 lowmemorykiller: Kill 'com.android.camera'    (23408) to free 79680kB rss, 40096kB swap; reason: cw
06:56:34.233 lowmemorykiller: Kill 'com.facebook.system'   (23033) to free 38320kB rss, 45496kB swap; reason: cw
06:56:34.473 lowmemorykiller: Kill 'com.facebook.stella'   (23467) to free 126252kB rss, 59652kB swap; reason: cw
```

`reason: cw` = *cache watermark*. El otro habla solo.

---

## 3. El tirón es la ZRAM, no el render

- **1,94 GB comprimidos en swap.**
- **`kswapd0` al 34 % de un núcleo** (visto en `top`).
- El proceso del juego tiene **153 MB propios en swap** (`/proc/PID/status`:
  `VmSwap: 152976 kB`), y su heap nativo mide 48 MB de PSS contra **90 MB de
  `SwapPss`**: dos tercios del heap está comprimido afuera.

Cada página que el juego vuelve a tocar cuesta una descompresión. **Por eso el
tirón no aparece en ningún perfil de render: no ocurre en el render.** Los `span`
propios del juego suman 1–2 ms sobre un frame de 26–36 ms.

Y el juego no compite solo por CPU: `com.miui.securitycenter.remote` estaba al
**80,8 %** durante la medición.

---

## 4. De dónde sale el gigabyte

Todo el arte del juego, descomprimido a RGBA8 de una vez, son **5.038 MB** sobre
un teléfono de 3.798 MB. No se carga todo junto, pero la proporción dice dónde
está el peso.

| Carpeta | PNG | RGBA8 |
|---|---:|---:|
| `chargen/pelo` | 200 | **1.835 MB** |
| `chargen/ropa` | 132 | **1.211 MB** |
| `chargen/accesorio` | 84 | **771 MB** |
| `pack` (el mundo) | 169 | 691 MB |
| `npc` | 21 | 188 MB |
| `retrato` | 280 | 138 MB |
| `chargen/cuerpo` + `ojos` | 16 | 147 MB |
| resto (atlas, ui, props, muebles…) | 261 | 57 MB |
| **TOTAL** | **1.163** | **5.038 MB** |

**El generador de personaje son 3.964 MB de los 5.038: el 79 %.** Cada peinado es
una hoja de animación completa de 1792×1280 —9,2 MB en RAM— y hay doscientos.

En el APK pasa exactamente lo mismo:

| Contenido | Archivos | Peso | Del total |
|---|---:|---:|---:|
| `assets/art/chargen` | 433 | **101,0 MB** | **57 %** |
| `lib` (binario nativo) | 2 | 44,7 MB | 25 % |
| `assets/art/pack` | 168 | 12,2 MB | 7 % |
| `assets/art/npc` | 21 | 9,1 MB | 5 % |
| resto | 1.103 | 10,2 MB | 6 % |
| **TOTAL** | | **177,2 MB** | |

**El 57 % de lo que se descarga es para una pantalla que se usa una vez.**

### El matiz, que es buena noticia

El arte que pasa por `manifest.json` **ya** se carga sólo a GPU cuando el slot
declara su `grid` — la optimización que documenta `art2d::cargar_png` (medida en
su momento con `HA_RAMART=1`: 306 imágenes = 238 MB de CPU de más). Hoy son **223
páginas sin copia en CPU** contra 46 que sí la pagan, y esas 46 suman 2 MB.

El problema es que **894 PNG están fuera del manifest** —casi todo `chargen`— y
por esa puerta no pasan.

### Cuánto de cada hoja de personaje se usa de verdad

Una hoja de `chargen` mide 56 × 20 = **1.120 celdas** de 32×64. Las 20
animaciones de `chargen_index::CHAR_ANIMS` (quieto, respirar, caminar, dormir,
sentarse, celular, empujar carrito, agacharse, regalar, levantar, tirar, golpear,
puñetazo, apuñalar, sacar arma, apuntar, disparar, recibir daño) ocupan
**514 celdas: el 46 %**.

**El otro 54 % de cada hoja son píxeles muertos** — las columnas de props que el
propio índice documenta como "no son cuadros del personaje" (la cama de `sleep`,
el celular de `phone_intro`, la caja de `gift`, el fogonazo de `shoot`) y el
sobrante de cada fila.

| Alcance | Celdas | Por hoja | 200 peinados |
|---|---:|---:|---:|
| Hoja completa, como está hoy | 1.120 | 9,17 MB | 1.835 MB |
| Sólo las 20 animaciones | 514 | 4,21 MB | 842 MB |
| Sólo quieto + respirar + caminar | 52 | **0,43 MB** | **85 MB** |

> ⚠ **Un error que casi escribo.** Leyendo `sim::face_player_sprite` parecía que
> el jugador usaba sólo las celdas 0, 1 y 2 —el camino de respaldo cuando el slot
> no tiene poses—, o sea un 0,3 % de la hoja. Es falso:
> `art2d::registrar_poses_de_personaje` **registra las poses en código** (no en el
> manifest) derivándolas del mismo índice con el que se horneó la hoja, así que
> `has_poses` da verdadero y el juego sí anima las 20. La cifra buena es el 46 %,
> no el 0,3 %.

### La precarga de la cortina

`loading::claves_del_mundo` pide **575 claves** que tocan **54 páginas de atlas =
119 MB**, todas durante la cortina. Es exactamente el momento del pico.

---

## 5. Arranque y cuadro por cuadro

| Fase | Medida |
|---|---|
| Arranque en frío hasta `MainActivity` | 3.212 ms (`am start -W`) |
| Menú principal | 58–68 fps, 490 MB RSS |
| «Preparando el mundo → cargando texturas» | pico de 997 MB |
| En juego | 28–39 fps, 872 MB RSS |

Contador propio en juego:

```
ha-perf fps=28 frame_ms=36.1 jitter=7.7 res=2400x1080 ents=5088
        spr[vis=537 lotes=140 tex=28 tileart=159] ground=81 chunk=4265
        span[cam=0 chk=299 tgt=0 hud=78 ptc=0 atm=0 lit=506]us
```

- **4.263 de las 5.087 entidades son de chunk** — y el escenario medido es un
  campo casi vacío.
- `gfxinfo` reporta 100 % de frames con jank y un percentil 90 de 2.600 ms, pero
  con sólo 3 frames de muestra: el juego dibuja por `SurfaceView`, así que ese
  contador no lo ve bien. **El bueno es `ha-perf`.**

### La escala de render, por confirmar

`DEFAULT_RENDER_SCALE` es **0,8** y `render/scale.rs` está escrito para eso, pero
el contador reporta `res=2400x1080`. Falta comprobar si el mundo dibuja a
1920×864 o a nativo: en un Mali-G52 la diferencia es un 36 % de píxeles por
frame.

---

## 6. Lo que se ve en pantalla y no es velocidad

De la captura en juego, cuatro cosas anotadas ahora que están vistas:

| Qué | Dónde | Lectura |
|---|---|---|
| Damero de dos tonos en el suelo | pasto y tierra, todo el mapa | La variante de losa se sortea por celda y las variantes **no son intercambiables**. Es lo que marcó el usuario en la hoja del editor. |
| Borde verde en escalera contra la tierra | toda la mancha de tierra | La orilla se dibuja **sólo del lado de la tierra** (`world2d::spawn_chunk_orilla`); al pasto le falta su mitad de la transición, que el pack sí dibujó. |
| Recuadro blanco roto arriba a la izquierda | HUD, junto a `CAM (0)` | Algo se dibuja con el recorte equivocado. Sin diagnosticar. |
| El joystick dibuja una hoja de gamepads | abajo a la izquierda | El círculo del stick muestra el atlas entero en vez de un recorte. Sin diagnosticar. |

---

## 7. Qué hacer, por rendimiento sobre esfuerzo

### 1. Sacar `chargen` del camino — ataca las tres quejas a la vez
El 79 % de la RAM potencial y el 57 % del APK, para una pantalla que se usa una
vez. Tres caminos combinables:
- **Empaquetarlo en páginas de atlas con `grid`**, que es lo que le da la carga
  sólo-GPU que el resto del arte ya tiene.
- **Miniaturas para la paleta** y la hoja completa sólo del peinado elegido.
- **Recortar las hojas a los cuadros que el juego de verdad dibuja.**

### 2. Poner techo a la precarga de la cortina — es donde el kernel se rompe
575 claves / 54 páginas / 119 MB de golpe. Cargar por tandas con un presupuesto
en MB, en vez de todo lo que las nueve tablas de siembra mencionan.

### 3. Bajar las 4.263 entidades de chunk — es el fps directo
El 84 % de las entidades vivas, en un campo casi vacío. Cada sprite por celda es
coste de CPU por frame aunque no se mueva. Misma cuenta que ya bajó la siembra de
vereda del 6 % al 3 %.

### 4. Confirmar que la escala de render se aplica — barato de comprobar

---

## Cómo repetir esta medición

```bash
adb shell am force-stop com.hungerarena.app
adb logcat -c
adb shell am start -W -n com.hungerarena.app/.MainActivity
# muestrear mientras se juega
adb shell "cat /proc/meminfo | grep MemAvailable"
adb shell "ps -A -o RSS,NAME | grep -E 'hungerarena|miwallpaper|miui.home'"
adb logcat -d | grep -E "lowmemorykiller|ha-perf"
adb shell dumpsys meminfo com.hungerarena.app
adb shell "top -b -n 1 -q -o %CPU,RES,ARGS | sort -rn | head"
```
