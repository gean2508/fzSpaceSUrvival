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
| algo se rompió | §12 |
| qué falta hacer | §13 |
| "¿por qué no hacemos X?" | §14 (cementerio) |

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
6. **UI del juego ASCII-only** (la fuente no renderiza acentos, flechas, `·` ni
   emoji: salen cajas).
7. **Assets por ID lógico.** Antes de spawnear o referenciar cualquier asset visual,
   mirá el catálogo de slots en `native/crates/ha-assets/src/slots.rs` (fuente de
   verdad de qué IDs existen y cómo se llaman en castellano). El arte se declara en
   `native/crates/ha-game/assets/art/manifest.json`, que **escribe el usuario desde
   HA Studio** (`./editor_assets`).
   - **Nunca generes PNGs.** Si falta arte, creá el slot y dejá que el usuario le
     asigne el pixel; sin arte el juego cae al fallback procedural y funciona igual.
   - **Nunca inventes rects, offsets, pivotes ni claves de variante** — todo eso
     sale del editor. Las variantes son `objeto#variante` y se piden con
     `ArtLibrary::resolved_key(objeto, variante)` o `variant_key_seeded(objeto, celda)`.
   - Si agregás una pieza colocable, declarale su slot con `label` o falla el test
     (`every_slot_has_a_human_label`, `slot_audit.rs`).
   - `native/assets/CATALOGO.md` es un volcado **regenerable** desde el botón
     "📄 Catálogo" del editor. Sirve de referencia rápida; la verdad es `slots.rs`.
8. **Una transición es un cambio de marco, jamás una cutscene**: no bloquea input ni
   cámara, no tapa la pantalla.
9. **No podar la conducta emergente** de fauna/NPCs.
10. Flujo del usuario: `./build-y-publicar.sh` → URL en el celu → descargar APK →
    instalar → jugar.
