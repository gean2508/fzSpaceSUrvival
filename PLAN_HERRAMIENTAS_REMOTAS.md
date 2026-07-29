# PLAN — Herramientas remotas centralizadas (web sobre Tailscale)

> Anexo de `PLAN_MAESTRO.md`. Objetivo: editar el juego **desde el trabajo** (o cualquier
> lado) por Tailscale, con **una sola interfaz web** que agrupa las herramientas —
> **assets**, **mundo** y **música** — cada una con su estructura, todo centralizado.
> Relacionado: [[tailscale-remote-access]], [[self-serve-build-no-tokens]].

## 0. Visión (del usuario, 2026-07-25)

Aprovechar el server que ya corre en Tailscale (`100.83.97.54`) para conectarse a una
**GUI web de las herramientas**: editor de assets, editor de mundo, editor musical — hoy
son apps de escritorio (egui, `./editor_assets gui|mundo|sonido`). Quiero abrir una URL
desde el trabajo y editar el mundo / los assets / crear música, todo en un mismo lugar.
Adaptar al proyecto, no al pie de la letra.

**Modelo THIN-CLIENT (aclaración del usuario, 2026-07-25):** la interfaz corre en el CELU
(un clic, un navegador) pero **todo se procesa en la COMBO** (la PC dev, `pop-os`), porque
el celu **no tiene los assets ni el proyecto**. El celu sólo manda acciones; los archivos
(`manifest`, `world_edits.json`, `audio.json`, PNGs) viven y se editan **en la combo**, y
los cambios **se reflejan en la combo y en el juego al guardar**. O sea: server en la
combo, cliente liviano por Tailscale. Nada pesado corre en el celu.

## 1. Idea rectora: reusar la arquitectura DATA-DRIVEN

Las tres herramientas ya son **data-driven** — no editan código, editan archivos:

| Herramienta | Fuente de verdad | Doc |
|---|---|---|
| Assets | `native/assets/manifest.ron` (+ `manifest.json`), catálogo | `ASSET_SYSTEM_ARCHITECTURE.md` |
| Mundo | `world_edits.json` (overrides sobre el mundo procedural) | editor `mundo` |
| Música | `native/assets/audio.json` (+ crate `ha-audio`) | [[audio-editor-daw]] |

Entonces la web NO reimplementa la lógica del juego: **lee y escribe esos mismos archivos**
por una API HTTP chica, y el juego los consume igual que hoy (drop-in + `sync:art`,
recompilar para música). Un solo servidor (bind `0.0.0.0`, alcanzable por Tailscale) sirve
la SPA-hub y la API. Cada pestaña = un editor; comparten shell, auth y catálogo.

## 2. Arquitectura

```
tools-server (Node/express, :4600, bind 0.0.0.0)   ← alcanzable por Tailscale
  ├─ /            SPA hub (una página, pestañas: Assets | Mundo | Música)
  ├─ /api/assets  GET/PUT manifest.json, listar PNGs, subir/recortar, regenerar catálogo
  ├─ /api/world   GET/PUT world_edits.json, tiles/overrides, preview del mundo
  ├─ /api/audio   GET/PUT audio.json, render de preview WAV
  └─ /api/fs      picker de archivos acotado a native/assets/ (sandbox)
```

- **Auth**: token simple (Tailscale ya limita a la tailnet, pero igual protegemos escritura).
- **Sandbox**: toda ruta acotada a `native/assets/` y a los JSON del juego (nunca fuera).
- **Concurrencia**: lock optimista por archivo (mtime/ETag) para no pisar ediciones.
- **Reflejo en el juego**: escribir el archivo + disparar `npm run sync:art` cuando aplica;
  música requiere recompilar (avisar en la UI). Hot-reload donde el motor ya lo soporta.

## 3. Fases

- **FR1 — Infra + hub**: `tools-server` (express, bind 0.0.0.0, token), SPA shell con las 3
  pestañas y health-check; un modo en `build-y-publicar.sh` (`herramientas`) que lo levanta
  y muestra la URL Tailscale. Read-only primero (ver catálogo/estado) para validar acceso.
- **FR2 — Assets (web)**: ver catálogo + tabla de slots; editar campos simples (id lógico,
  colisión, escala) → `PUT manifest`; subir PNG; **regenerar `CATALOGO.md`**. (El slicer de
  sprite-sheets con canvas llega en FR2.2 — es lo más pesado.)
- **FR3 — Mundo (web)**: canvas WYSIWYG (pincel/relleno/cuentagotas) sobre `world_edits.json`
  — port de las funciones del editor `mundo`; preview del terreno procedural + overrides.
- **FR4 — Música (web)**: editor de `audio.json` (mini-DAW): tracks/patrones, preview WAV
  sintetizado server-side (reusa `ha-audio` vía un binario de render), guardar.
- **FR5 — Integración/vivo**: reflejar cambios en una instancia de juego corriendo (sync:art
  + recarga), historial/undo por archivo, y (opcional) editar mientras el juego está abierto
  en el celu por Tailscale.

## 4. Riesgos / decisiones

- **Paridad con egui**: los editores nativos (sobre todo el slicer y el mini-DAW) son ricos;
  la web arranca por lo más usado y sube. NO romper los editores nativos — coexisten (misma
  fuente de verdad de archivos).
- **Escritura remota = peligro**: sandbox estricto + token + lock. Nunca exponer fuera de la
  tailnet (no publicar la URL a Internet).
- **`ha-audio` render**: exponer un binario `render-audio <audio.json> <out.wav>` para que la
  web previsualice swithout duplicar el sintetizador.
- **Build**: el `tools-server` es Node puro (sin tocar el APK) → liviano, no compite con los
  builds Rust pesados.

## 5. Estado

- [x] FR1 — infra + hub: `scripts/tools-server.mjs` (Node puro, bind 0.0.0.0:4600, alcanzable
  por Tailscale). Hub web mobile-friendly con pestañas (Assets/Música/Mundo) que **leen y
  EDITAN** los JSON reales (`manifest.json` de arte, `audio.json`, `world_edits.json`) con
  textarea + Guardar. Writes: token (env `TOOLS_TOKEN` o autogenerado, se imprime al arrancar),
  validación JSON, backup `.bak`, sandbox al repo. Lanzar: `./build-y-publicar.sh herramientas`
  o `npm run tools`. Probado (list/read/write/validación/403). Falta pulir a editores ricos
  (FR2 slicer, FR3 canvas de mundo, FR4 mini-DAW) + reflejo en vivo (sync:art/recompilar).
- [ ] FR2 — assets web (catálogo/slots; luego slicer).
- [ ] FR3 — mundo web (canvas overrides).
- [ ] FR4 — música web (audio.json + preview WAV).
- [x] FR5 (v1) — botón **"Aplicar (sync:art)"** en el hub → corre `npm run sync:art` en la
  combo (token). Edit → aplicar → recompilar y se ve en el juego. Falta hot-reload sin
  recompilar + reflejo en instancia corriendo.
</content>
