# Ki · Mundo Abierto — AI Guidelines

Sandbox de role-play 2D top-down 3/4 + vuelo Saiyan (Ki).
**Servidor Colyseus** + **cliente nativo Bevy (2D)** + sim determinista compartida.

## 📖 Documento único: `biblia.md`

Todo el conocimiento del proyecto vive en **`biblia.md`** (raíz). No hay otros
`.md`: se consolidaron ahí el 31 jul 2026. Antes de trabajar, leé la sección que
corresponda:

| Vas a tocar | Leé |
|---|---|
| mundo, vuelo, cámara, cielo, transiciones | §4 (marco único) + §5 (mundo) |
| fauna, NPCs, sociedad, IA frugal | §6 |
| controles, UI, luz, audio | §7 |
| assets, sprites, editores | §8 |
| build / publicar el APK | §9 |
| render, streaming, FPS, tirones | **§10 — obligatorio antes de optimizar** |
| tests y drivers de QA | §11 |
| idioma, multijugador (LAN/online) | §15 |
| algo se rompió | §12 |
| qué falta hacer | §13 |
| "¿por qué no hacemos X?" | §14 (cementerio) |
| gameplay nuevo, "¿qué hacemos para que sea divertido?" | §16 (pilares) + §19 (ideas de 25+ juegos) |

Si algo del documento contradice al código, **gana el código** — pero actualizá el
documento.

## Estructura

```
packages/shared/   Sim determinista (TS) — verdad del servidor
server/            Colyseus + auth + Postgres + /updates
client/            Landing ONLY — descarga del APK
native/            ha-shared · ha-assets · ha-audio · iafz-sys · ha-game (Bevy) · ha-asset-tools (HA Studio)
```

## Comandos

```bash
./build-y-publicar.sh     # todo: compila APK + publica + sirve (ver biblia §9)
npm run play              # ídem vía npm
npm run test              # vitest de shared
npm run test:native       # cargo test
npm run validate:arch     # límites de arquitectura
npm run qa:suite          # drivers de QA
./editor_assets           # HA Studio (editores)
```

## Reglas (detalle en biblia §2)

**Regla de procesos — no dejar nada vivo.** Todo proceso que lance (ha-game de QA,
cargo, gradle, `adb connect`, `leo-celu`, esperas en background) lo lanzo con
`timeout`, y antes de cerrar el turno verifico con `ps` que no quedó ninguno
(PC y celu: `adb shell pidof com.hungerarena.app` sólo si lo estoy probando).
Un comando que se cuelga se mata, no se deja en background. Nunca dos builds o
tests de cargo a la vez, y con el disco bajo 5 GB no se arranca ninguno.
**El celu es del usuario: se prueba SÓLO cuando avisa que lo conectó**, con ráfagas cortas, con
la guarda de primer plano (`dumpsys window | grep mCurrentFocus` = el juego) y limpiando
`debug.ha.*` al terminar (`setprop debug.ha.x ""`: un `0` cuenta como prendido).
**Con el celu enchufado NUNCA `adb kill-server`, `adb disconnect` ni `adb connect` a otra
IP**: reinician el USB y el celu vuelve a pedir el modo de conexión y las huellas. El
servidor de adb se deja vivo (es el único proceso permitido) y se opera con
`adb -s <serie>`.


**Regla de cola — un pedido nuevo NO frena lo en curso.** Se apila en la lista de tareas, se sigue con
lo que corre y se vacía la cola entera, sin dejar nada inconcluso (decisión del usuario, 1 oct 2026).

**Regla cero — lo que se empieza se termina. Cero human in the loop.** Prohibido
dejar algo incompleto: no hay "queda pendiente", ni "si querés sigo". Se hace
entero (código + tests + prueba + biblia + APK publicado y probado en el celu)
sin preguntar. Sólo corta un bloqueo físico real, y se dice tal cual (biblia §2).

0. **Antes de tocar código, preguntale al grafo.** `graphify-out/` tiene el mapa
   del repo: `graphify explain "<concepto>"` / `graphify path "A" "B"` ubican el
   código y sus vecinos sin grepear a ciegas. Al terminar, `graphify update .`
   para dejarlo al día. Nunca volcar `graph.json` ni `GRAPH_REPORT.md` enteros a
   la conversación — se consultan, no se leen. Detalle: skill `graphify-first`.
1. **NO 3D.** Se borró a propósito por perf. No reintroducir nada de §14.
2. **60 fps del Redmi manda.** Nada nuevo cuesta antes de que haya arte asignado.
3. Sim determinista sólo en `/shared` (TS), espejada en `ha-shared` (Rust). Nunca
   bifurcar reglas. Cambio de catálogo → `UPDATE_CATALOG_PARITY=1 npm test`.
4. Servidor autoritativo, clientes input-only. Sin IPs hardcodeadas
   (`VITE_SERVER_HOST` / `HA_SERVER_HOST` / `UPDATE_BASE_URL`).
5. `client/` es sólo la landing — sin motor de juego.
6. **La UI habla castellano de verdad.** El juego embarca su fuente
   (`ha-game/assets/fonts/ui.ttf`, ver `fuente.rs`): escribí con acentos, `¿` y
   `¡`. Lo que esa fuente NO tenga (emoji, `▲`, `←`) sigue saliendo caja, y el
   test `la_fuente_dibuja_todo_el_texto_de_la_ui` lo caza — esas formas se
   dibujan como nodos de UI, no como texto. La vieja regla "ASCII-only" quedó
   sin efecto: era un parche por la fuente pelada de Bevy, no una decisión de
   diseño.
7. **Assets por ID lógico.** Antes de spawnear o referenciar cualquier asset visual,
   mirá el catálogo de slots en `native/crates/ha-assets/src/slots.rs` (fuente de
   verdad de qué IDs existen y cómo se llaman en castellano). El arte se declara en
   `native/crates/ha-game/assets/art/manifest.json`, que escriben **los
   importadores de `scripts/`** y que el usuario revisa desde HA Studio
   (`./editor_assets`).
   - **Nunca inventes rects a ojo.** La geometría del recorte sale de un
     importador que la MIDE y se planta si el pack cambió
     (`import-suelo-bordes.py` es el ejemplo corto).
   - **Asignar arte es tarea tuya** (regla 13 de biblia §2, *no human in the
     loop*). El orden es: buscalo en el pack → derivalo del arte en uso →
     adaptá lo que el juego pide → **y si nada de eso da, dibujalo y dejá el
     PNG**. Lo último estuvo prohibido hasta el 14 ago 2026 y dejó de estarlo
     por decisión del usuario: la prohibición era por CÓMO salían mis dibujos
     (un color plano), no por dibujar. Ahora el estilo está medido y hay corsé
     que lo hace cumplir — **skill `pixel-art`, obligatoria antes de generar
     cualquier gráfico**, con `scripts/lib/pixel.py`. **El listón (§0.0 de esa skill) es
     el mínimo: volumen con `scripts/lib/volumen.py`, nunca elipses ni rectángulos planos «de Paint», y
     comparar la pieza contra `python3 scripts/galeria-arte.py` antes de darla por buena.** Lo dibujado vive en
     `art/gen/`, lo comprado en `art/pack/`, y el arte real gana siempre.
   - **Rects, offsets, pivotes y claves de variante se MIDEN, no se escriben** —
     los publica el importador o el editor, nunca un número a ojo. Las variantes
     son `objeto#variante` y se piden con `ArtLibrary::resolved_key(objeto, variante)`
     o `variant_key_seeded(objeto, celda)`.
   - Si agregás una pieza colocable, declarale su slot con `label` o falla el test
     (`every_slot_has_a_human_label`, `slot_audit.rs`).
   - `native/assets/CATALOGO.md` es un volcado **regenerable** desde el botón
     "📄 Catálogo" del editor. Sirve de referencia rápida; la verdad es `slots.rs`.
8. **Antes de colocar algo: ¿es una PIEZA o es un OBJETO? Y después, ¿dónde va?**
   Medio pack es modular y el que no lo mira pone media cosa en el mundo. El
   semáforo es poste + calcomanía de lámparas (`*_Mod_*`); la cerca son doce losas
   de autotile (`Bottom_Left`, `Inner_Corner`, `Pass`…); el galpón es cuerpo +
   puerta + techo; el seto son tramos de pared. **Una pieza suelta no se siembra
   nunca** — se compone en un importador (`scripts/import-semaforos.py` es el
   ejemplo) o se coloca a mano desde el editor.
   - El pack lo dice en el NOMBRE DEL ARCHIVO. El filtro vive en
     `scripts/import-pack-masivo.py::MARCAS_MODULARES`; del lado del juego lo
     cuida el test `ninguna_tabla_siembra_piezas_de_kit`.
   - Y recién después de saber que va sola, decidí DÓNDE: cada tabla de
     `pack_siembra.rs` es un sitio (vereda, calzada, parque, patio), y "cerca de
     gente" quiere decir dentro del casco (`settlement_covering`), no en la misma
     celda de 2,6 km. Detalle en biblia §8.
9. **Una transición es un cambio de marco, jamás una cutscene**: no bloquea input ni
   cámara, no tapa la pantalla.
10. **No podar la conducta emergente** de fauna/NPCs.
11. Flujo del usuario: `./build-y-publicar.sh` → URL en el celu → descargar APK →
    instalar → jugar.
12. **Probá en el celu ANTES de publicar. Publicar es el plan B.** El Redmi se
    alcanza por tailnet sin cable, así que no hay excusa para dar por bueno un
    APK que nadie corrió. El ciclo es:

    ```bash
    leo-celu conectar   # una vez por sesión
    leo-celu probar     # instala, lanza, espera, captura y vuelca el log
    ```

    `probar` devuelve 0 sólo si la app quedó viva, pero **mirá igual la captura**
    (`/tmp/leo-celu-prueba.png`): un proceso vivo con la pantalla en negro sigue
    siendo un fallo. Recién si el teléfono no está accesible o la instalación
    remota falla dos veces, publicás con `./build-y-publicar.sh` para bajarlo a
    mano. Detalle y modos de falla en biblia §9.
13. **Lo que se nos escapó.** Antes de una feature de simulación o de mostrar sim en
    pantalla, pasá el checklist de biblia §2 (*Lo que se nos escapó*): costo medido,
    materialización con cupo por frame, verdad en un solo lugar, consola de texto
    siempre, nombres que no prometan de más, prueba multi-semilla. Cuando algo se
    nos escape de nuevo, agregalo ahí.
