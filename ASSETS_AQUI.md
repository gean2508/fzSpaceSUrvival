# 📂 Dónde van los assets del juego

**La carpeta de assets está acá (muy adentro, por eso no la encontrabas):**

```
native/crates/ha-game/assets/art/
```

Ahí adentro hay subcarpetas por categoría: `tiles/ decor/ items/ machines/
player/ ship/ ui/ backgrounds/`. Poné los PNG con el nombre = clave y el juego
los usa solo (fallback al arte procedural mientras falten). Después de agregar
o renombrar: `npm run sync:art` y rebuild.

## Mapa: tus packs LimeZu (en `PRUEBAS-CODIGOS/`) → dónde van

> `PRUEBAS-CODIGOS/` es material de referencia (680MB, NO se sube al repo ni al
> APK). De ahí se extraen los sprites concretos que usemos.

| Necesito | De tu pack | Va a |
|----------|-----------|------|
| **Personaje** (caminar 4-dir) | `moderninteriors-win/2_Characters/Character_Generator/0_Premade_Characters/` (spritesheet completo) | `player/astronaut.png` (+ animación, Fase 2/6) |
| **Terreno/suelo** (pasto, tierra, arena, agua) | `modernexteriors-win/Modern_Exteriors_16x16/` (autotiles + tileset) | `tiles/` (atlas + autotiling, Fase 6) |
| **Árboles, rocas, props** | `modernexteriors-win/.../Modern_Exteriors_Complete_Singles_16x16/` | `decor/tree.png`, `decor/rock.png`, … |
| **Interiores/muebles** (base lunar, naves) | `moderninteriors-win/1_Interiors/` | `tiles/` / `decor/` (Fase 6) |
| **UI** (paneles, botones, iconos) | `modernuserinterface-win/16x16/` | `ui/` (Fase 6) |

## Estado actual (ver `MIGRATION_PLAN.md`)

Estamos migrando de vista lateral a **top-down 3/4** para poder usar estos
assets. **Fase 1 hecha** (núcleo top-down en `ha-shared`). Los assets se
enganchan de verdad en **Fase 2** (personaje animado) y **Fase 6** (tilesets).
Por ahora el juego sigue con arte procedural + el side-scroller viejo hasta el
swap de la Fase 2.
