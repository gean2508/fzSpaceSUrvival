# Escaneo movimiento / cuevas / UI — reporte para merge

## Parkour (`parkour.ts` / `parkour.rs`)

| Pregunta | Hallazgo |
|----------|----------|
| ¿Automático vs manual? | **Mixto:** sprint/move automático; **jump** = tap USE (edge); **climb** = USE sostenido en pared ≤3m; **mantle** = USE en `hanging` |
| ¿Obstáculos vs terreno? | **Solo obstáculos AABB** de `generateWorld`. Terreno de `world_spawn` (playa/pradera/acantilado) **no** entra en sim |
| `groundHeightAt` | Max altura de cajas bajo (x,z) — no malla de suelo |

**FASE 2 pendiente:** extender `groundHeightAt` + blocking con heightfield de isla (no sistema paralelo).

## Portal cueva (`scene.rs`)

| Pregunta | Hallazgo |
|----------|----------|
| ¿Exploración real? | **No — placeholder visual** |
| Comportamiento | USE cerca del portal (≤2.8m) alterna `RegionId::Overworld` ↔ `Cave(1)` |
| Efectos | Lerp cielo oscuro, antorchas, cristales, ocultar decor chunks |
| Falta | Teleport, geometría de cueva, sim separada |

**Acción:** reportar solamente; diseño en `design/world/README.md` antes de implementar.

## Natación

**No implementada aún.** Océano = anillo visual (`OCEAN_HALF`); movimiento clamped en `WORLD_HALF`.

Cuando se implemente: estado `swimming` en shared, relajar clamp en zona agua, preset atmósfera azul (lerp existente).

## UI icons — FASE 1 (esta sesión)

| Antes | Después |
|-------|---------|
| Patterns hardcodeados en `hud/icons.rs` | Catálogo `content/ui/icons.ts` + `IconFactory` |
| 5 texturas raster 1× en Startup | Mismo — lazy cache por `kind` |
| ~66 UI nodes (baseline v1.0.35) | Objetivo: **no empeorar** — verificar F3/ha-perf post-install |

**Editar ícono:** cambiar entrada en `packages/shared/src/content/ui/icons.ts` → rebuild APK.

## Perf / fugas (auditoría rápida)

- `HudIconAtlas.ready` + `IconFactory` cache — sin rebuild por frame ✓
- `ensure_hud_icon_atlas` — guard `if atlas.ready` ✓
- `chunk_manager` — early return si chunk sin cambio ✓
- `log_perf_periodic` — cada 5s, no cada frame ✓
- `update_crash_context` — PostUpdate, costo O(1) ✓

Sin ciclos infinitos detectados en hot paths revisados.
