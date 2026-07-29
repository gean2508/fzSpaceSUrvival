# Cómo compilar y probar el juego — manual (sin la IA)

Guía para que compiles el APK, levantes los servidores y lo pruebes VOS mismo,
cuando te quedes sin tokens. Todo desde la raíz del repo (`phaser-arena/`).

**El atajo:** casi todo se reduce a **un solo comando**:

```bash
./build-y-publicar.sh
```

Eso compila el APK del juego, lo publica en la web de descarga, y levanta el
servidor + la landing. Cuando termine te imprime una URL. Abrís esa URL en el
celu (en la **misma WiFi** que la PC), descargás el APK, lo instalás y jugás.

---

## 0. Requisitos (una sola vez)

Ya deberían estar instalados en tu máquina (esto es sólo para reinstalar):

- **Node.js** (v18+) y **npm** — orquesta el build y sirve la web.
- **Rust** (`rustup`) con el target de Android:
  ```bash
  rustup target add aarch64-linux-android
  cargo install cargo-ndk
  ```
- **Android SDK + NDK** — el build usa `ANDROID_HOME`/`ANDROID_NDK_ROOT`.
  El script `scripts/lib/android-env.mjs` los autodetecta; si falla, te dice qué
  variable falta.
- Dependencias del repo (una vez): `npm install`

---

## 1. El comando único — `./build-y-publicar.sh`

```bash
./build-y-publicar.sh            # build completo + tests + publica + sirve
./build-y-publicar.sh sin-tests  # igual, pero SALTA los tests (más liviano en RAM)
./build-y-publicar.sh rapido     # NO recompila: sólo sirve el APK ya publicado
./build-y-publicar.sh editor     # abre SÓLO el editor de assets (no toca el juego)
./build-y-publicar.sh sonido     # abre SÓLO el editor de sonido / mini-DAW
```

- **`completo`** — lo normal. Corre los gates de test antes de compilar.
- **`sin-tests`** — si tu PC tiene poca RAM y se cuelga/OOM en la fase de tests
  (log congelado en "Compiling ha-game" 10+ min), usá este. El build real de
  `cargo ndk` igual valida que compile; sólo se saltan los tests extra que
  recompilan el binario de test de Bevy dos veces (ver biblia §7).
- **`rapido`** — si NO cambiaste nada del juego y sólo querés volver a servir el
  APK que ya está publicado (arranca en segundos).

Cuando termina, verás algo como:

```
║  2. Abrí en Chrome:  → http://192.168.1.X:5173
║  APK directo:        http://192.168.1.X:5173/downloads/hunger-arena.apk
```

Dejá la terminal abierta (mantiene vivos el servidor y la web). Cortá con `Ctrl+C`.

---

## 2. Editor de assets (para ponerle texturas al juego)

El editor es un programa de **desarrollo aparte** — NO corre junto al juego.

```bash
./build-y-publicar.sh editor      # o directamente:  ./editor_assets
```

Flujo dentro del editor:

1. Barra de arriba → **Arte fuente**: elegí la carpeta con tus PNG (LimeZu, etc.)
   y **Cargar PNGs**.
2. Panel izquierdo: la lista de **slots que el juego usa** (personaje, árboles,
   rocas, ítems, máquinas, tiles, UI…). Elegí uno.
3. Panel derecho: elegí el PNG fuente (click = preview).
4. **Asignar PNG entero**, o abrí **✂ Recortar hoja** para cortar un sprite de
   una hoja grande (arrastrás en la grilla para seleccionar).
5. **Guardar (actualiza el juego)** en la barra de arriba. Escribe el
   `manifest.json` del juego y lo verifica releyéndolo del disco.

### Layout del editor

- Los paneles izquierdo y derecho son **redimensionables**: arrastrá el borde
  para dar más o menos ancho a cada sección.
- El **lienzo** (donde recortás) tiene control **"alto lienzo"** — subilo si te
  queda mucho espacio vacío, para trabajar más cómodo.

### Animaciones (frames) — cómo funciona

- La **selección** que arrastrás en el lienzo es **UN frame** (su tamaño en
  celdas, ej. 3×3 = 9 cuadraditos = un frame).
- El campo **"frames (cantidad)"** es **cuántos frames** tiene la animación
  (ej. 5). Son bloques **contiguos** del mismo tamaño; la **dirección** (→ / ↓)
  dice hacia dónde siguen. El **preview** reproduce lo que verás en el juego.

### Animaciones de personaje / NPC (un atlas, filas = animaciones)

En un slot de personaje (`player/character`, `npc/colonist`) se abre la sección
**"🎬 animaciones nombradas"**: una matriz idle/walk × 4 direcciones. Flujo:

1. Marcá en el lienzo el **primer frame** de una fila del atlas.
2. Botón **"toda la fila →"** para autocompletar los frames (columnas).
3. Click en la casilla de la matriz (ej. `walk_left`) para elegir qué animación es.
4. **💾 Guardar animación / pose.** Repetí por cada fila.

El **jugador ya usa** estas animaciones en el juego. Los colonos (`npc/colonist`)
se ven parados en la base lunar con su animación idle.

### Colisión de un asset (🚧 bloquea el paso)

En cualquier slot de decoración se abre **"🚧 colisión (bloquea el paso)"**:
elegís si ese asset **frena** al jugador o se pisa. Tres opciones:

- **usar default del juego** — árboles bloquean (no pisás el tronco), rocas y
  flora se pisan.
- **bloquea SIEMPRE** — el asset es sólido (una roca grande, un poste).
- **NUNCA bloquea** — se pisa aunque sea un árbol.

Hoy bloquea **la celda base** del asset. La colisión por-celda precisa
(cráteres, la base exacta de una montaña) está teorizada en
`PLAN_COLISIONES_Y_CIUDADES.md`. Guardá y recompilá para aplicarlo al juego.

### Tiles / autotile (agua, cascadas)

En un slot `tiles/*` se abre **"🧩 autotile"**: marcás la celda de cada pieza
(centro, bordes, esquinas) y la asignás a su posición en la grilla 3×3. El juego
elige la pieza correcta según los vecinos y la anima si tiene frames. **Mínimo
útil: guardá "centro".** (El suelo base sin arte sigue como color plano.)

Después de editar assets, recompilá el juego para verlos:
`./build-y-publicar.sh` (o `./editor_assets play`).

---

## 2-bis. Editor de sonido (mini-DAW del juego)

También es un programa de **desarrollo aparte** — NO corre junto al juego:

```bash
./build-y-publicar.sh sonido      # o directamente:  ./editor_assets sonido
```

Edita `native/assets/audio.json` (la "partitura" de TODO el audio: los 22
efectos, el loop del cohete y las pistas musicales). Flujo:

1. **Panel izquierdo**: elegí un SFX (cavar, pasos, puertas…), el loop del
   cohete, una pista musical, o "Mezcla y director" (volúmenes y silencios).
2. **SFX**: cada sonido son **capas de tono** (onda, frecuencia inicio→fin,
   volumen, ataque, vibrato) que se suman, más una **cadena de efectos**
   (reverberación, eco, filtro grave, bitcrush, fundido…). **▶ Escuchar** lo
   reproduce al toque y te muestra la forma de onda.
3. **Pistas musicales**: son generativas (estilo C418) — elegís contexto
   (superficie/espacio/subsuelo), semilla (🎲 para probar variaciones),
   duración, nota base, **escala** (presets pentatónicos o la tuya), densidad
   de notas/silencios y timbres. **➕ nueva pista** agrega más música al juego.
4. **💾 Guardar** escribe el `audio.json` y lo verifica. Para OÍRLO en el
   juego: recompilá (`./build-y-publicar.sh` — el JSON se embebe al compilar).

Si rompés todo: cada SFX tiene "↩ restaurar default", y
`cd native && cargo run -p ha-asset-tools -- sound-init --force` regenera el
archivo entero con el diseño original.

---

## 3. Paso a paso manual (si el script falla y querés depurar)

El script único es equivalente a esta secuencia. Corré cada paso a mano para ver
dónde falla:

```bash
# a) Detectar la IP de tu PC en la LAN (la que ve el celu)
node scripts/detect-lan-ip.mjs

# b) Compilar el APK + publicarlo (reemplazá la IP por la del paso a)
#    --skip-tests evita el OOM de los gates de test en PCs de poca RAM
node scripts/ship-native-android.mjs --host 192.168.1.X --skip-tests

# c) Levantar el servidor (:2567) y la landing (:5173) — en la misma terminal:
VITE_SERVER_HOST=192.168.1.X UPDATE_BASE_URL=http://192.168.1.X:2567 \
  npm run dev:server &     # servidor de juego + /updates
VITE_SERVER_HOST=192.168.1.X npm run dev:client   # web de descarga
```

Verificación rápida de que quedó publicado:

```bash
curl -s http://192.168.1.X:2567/updates/manifest      # manifest OTA
ls -la client/public/downloads/hunger-arena.apk        # APK en la landing
```

### Compilar sólo el juego para PC (probar rápido sin celu)

```bash
cd native && make run        # abre el juego en la PC (útil para iterar arte)
```

### Chequear que compila sin buildear el APK entero

```bash
cd native && cargo check -p ha-game        # valida el juego
cargo test -p ha-assets                    # valida el sistema de assets/editor
```

---

## 4. Problemas comunes

- **El celu no ve la web:** confirmá que celu y PC están en la **misma WiFi** y
  que usás la IP de `detect-lan-ip.mjs` (no `localhost`).
- **Build "colgado" 10+ min en "Compiling ha-game":** es normal en la primera
  compilación (Bevy es grande). Si tenés poca RAM y hace OOM, usá `sin-tests`.
  Verificá que sigue vivo con `ps aux | grep cargo` antes de matarlo.
- **"No space left on device" durante el build:** liberá espacio del target de
  debug: `rm -rf native/target/debug`.
- **El APK no muestra los cambios:** el `versionCode` se bumpea solo al publicar;
  relanzá la app en el celu (la actualización OTA es obligatoria). Reinstalá si
  hace falta.
- **Acceso remoto (fuera de la WiFi):** con Tailscale, pasá el host de la tailnet:
  `node scripts/ship-native-android.mjs --host 100.83.97.54` y serví con esa IP.
