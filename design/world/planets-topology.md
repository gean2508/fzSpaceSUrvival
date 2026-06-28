# Topología planetaria — ciencia + perf

Diseño de suelo y relieve para **Tierra (Acto I)**, **Luna**, y cuerpos futuros.  
Objetivo: exploración creíble inspirada en datos reales, **sin voxels Minecraft** (curvas suaves + height field), manteniendo **30 FPS mínimo en Redmi**.

---

## Principios

| Regla | Por qué |
|-------|---------|
| **Height field único** (`terrainHeightAt`) | Colisión = render; determinismo TS ↔ Rust |
| **Curvas suaves, no cubos** | Menos fall-through, mejor lectura Ghibli + Tierra |
| **Tessellation adaptativa** | Más vértices solo en chunks empinados |
| **Malla conservadora** | Vértices = max(local) para no ver huecos en crestas |
| **Footprint ampliado en pendiente** | Colisión circular bajo el jugador en montaña |
| **Bioma por elevación + humedad** | Modelo Earth-like (costa → pradera → montaña → nieve) |

---

## Tierra (isla 192×192 m, escala 1u = 1 m)

Inspiración: **isla-oasis** con macizo tipo Himalaya (Everest ~28 m arcade), meseta Colorado, río, pantano costero.

### Capas del height field

1. **Continental base** — FBM baja freq (≈12 km wavelength en escala real → 0.012 en mundo)
2. **Cordilleras** — ridged multifractal suavizado (amplitud ~10.5 m, no 14)
3. **Detalle** — FBM media freq para variación local
4. **Landmarks gaussianos**
   - Everest: (62, 68), pico 28 m, σ=17 m (macizo ancho)
   - Mesa: (-42, -38), radio 22 m, meseta 8.5 m
   - Río: z≈-18, ancho 9 m, profundidad 1.2 m
5. **Biome post-process** — meadow aplana interior; beach taper en costa

### Biomas (colores Tierra)

| Bioma | Elevación / condición | Paleta |
|-------|----------------------|--------|
| Beach | borde isla | arena / dunas |
| Meadow | interior bajo, seco | verde pradera |
| Forest | húmedo, bajo | verde bosque |
| Swamp | húmedo, muy bajo | verde oscuro |
| Highland | 9–14 m | marrón tierra |
| Paramo | >14 m, seco | pardo verdoso |
| Alpine | cerca Everest, >18 m | roca + nieve |
| Mesa | meseta SW | roca rojiza |

### Datos reales de referencia (escala arcade)

| Real | En juego |
|------|----------|
| Everest 8 849 m | Pico landmark 28 m (visible desde ~80 m) |
| Línea nieve ~5 500 m | `snowLine` 18 m |
| Pendiente media sendero 15–25° | `terrainSlopeAt` < 0.45 en rutas jugables |
| Radio isla explorables | `WORLD_HALF` 96 m |

---

## Luna (`WorldContext::Luna`)

Inspiración: **Mare + highlands** (Apollo, LRO).

| Feature | Diseño propuesto |
|---------|------------------|
| Gravedad | 0.17× Tierra (`GRAVITY_LUNA_SCALE`) |
| Regolito | gris claro, pocos FBM, cráteres baja freq |
| Maria | planicies basálticas oscuras, bordes suaves |
| Highlands | ridged noise baja amplitud, 2–6 m |
| Cráteres | depresiones gaussianas + borde anillo (shader `planet_surface.wgsl`) |
| Color | gris-anular; sin atmósfera azul |

**Perf:** mismo chunk 40×40; step heightmap 4–5 m en llanuras, 2 m en bordes de cráter.

---

## Espacio / otros cuerpos (futuro)

| Cuerpo | Topología | Notas |
|--------|-----------|-------|
| Marte | mesetas + cañones ridged | óxido rojo, sin océano |
| Europa | grietas + hielo liso | misión instanciada |
| Titán | dunas + lagos hidrocarburo | atmósfera naranja |

---

## Perf budget (Android)

| Parámetro | Valor actual | Límite |
|-----------|--------------|--------|
| `HEIGHTMAP_STEP` plano | 3.5 m | ≤ 4 m |
| `HEIGHTMAP_STEP` empinado | 2.0 m | ≥ 1.5 m |
| Chunks activos | ~25 | no subir sin LOD |
| Draw calls terreno | 1 por chunk | merged mesh OK |
| Flora scatter | 120 | subir solo con instancing |

**Target:** 30 FPS estable en montaña; 60 FPS en pradera cuando render pipeline lo permita.

---

## Archivos de implementación

| Capa | Archivo |
|------|---------|
| Height field TS | `packages/shared/src/sim/terrain.ts` |
| Height field Rust | `native/crates/ha-shared/src/terrain.rs` |
| Colisión suelo | `world.ts` / `world.rs` — `groundHeightAtFootprint` |
| Mesh + color vértice | `native/crates/ha-game/src/render/mesh_batch.rs` |
| Spawn + materiales | `native/crates/ha-game/src/world_spawn.rs` |
| Shader toon Tierra | `native/crates/ha-game/assets/shaders/toon.wgsl` |

---

## No hacer (por ahora)

- **Voxels Minecraft** — más draw calls, peor colisión en bordes de cubo
- **Doble geometría** (heightmap + cilindros Everest) — z-fight y huecos
- **Plano base bajo heightmap** — se veía a través del suelo
- **Subdividir toda la isla a 1 m** — mata FPS en Redmi

---

## Próximos pasos

1. Luna: `terrain_luna.rs` espejo TS con mare/highlands
2. Shader planeta en superficie local (transición orbital → a pie)
3. Golden snapshot heights tras suavizado de crestas
4. Smoke test: subir Everest sin fall-through + log `terrain_slope_at` en perf
