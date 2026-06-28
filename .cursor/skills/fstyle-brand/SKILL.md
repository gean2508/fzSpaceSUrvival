---
name: fstyle-brand
description: Apply the Hunger Arena fStyle brand — Far Cry 3 Blood Dragon inspired synthwave UI (menus, HUD, landing, icons, copy). In-game 3D uses hybrid Ghibli + neon dusk. Use for any UI, branding, icons, splash, or theme tokens.
---

# fStyle Brand

**fStyle** = official name for the Blood Dragon–inspired visual identity of Hunger Arena.

## Split

| Layer | Brand | Tokens |
|-------|-------|--------|
| Menus, HUD, landing, splash, icons | **fStyle** | `FSTYLE`, `FSTYLE_UI`, `FSTYLE_COPY` in `packages/client-core/src/theme/fStyle.ts` |
| In-game 3D world | Hybrid Ghibli + neon dusk | `PALETTE`, `HYBRID_LIGHTS` in `packages/client-gfx` |

Read `.cursor/skills/hybrid-bd-ghibli-graphics/SKILL.md` for world rules.

## Tokens (always use these)

```ts
import { FSTYLE, FSTYLE_UI, FSTYLE_COPY } from "@hunger-arena/client-core";
```

- **Never** hardcode `#ff006e` in UI — use `FSTYLE.neonPink`.
- Copy strings from `FSTYLE_COPY` — never invent VHS copy ad hoc.
- `BD` / `BD_UI` / `BD_COPY` are deprecated aliases — prefer `FSTYLE*`.

## Visual rules

1. Void purple backgrounds (`#0a0014`), neon pink + cyan accents, HUD green for stats.
2. Glass cards: `FSTYLE.glass` + `FSTYLE.glassBorder`, sharp 4px radius (not pill-ios).
3. Typography: heavy weight, letter-spacing 1–3, mono for debug/URLs.
4. Scanlines / grid optional on backdrops — not on gameplay HUD readability zones.
5. App icon: dragon-skull visor on grid — regenerate with `npm run generate:icons`.

## Icons & assets

```bash
npm run generate:icons   # apps/mobile/assets + desktop tauri icons
```

Files: `icon.png`, `android-icon-foreground.png`, `android-icon-background.png`, adaptive bg `#0a0014`.

## Debug

- `bootLog(tag, msg)` in `packages/client-core/src/debug/bootLog.ts` — OTA, arena, fatal.
- Enable HUD: Settings → DEBUG HUD → shows FPS, RTT, boot log ring.
- Splash shows boot log strip when debug ON.

## OTA splash

Update gate uses fStyle overlay (`UpdateGate.tsx`). Mandatory patches block menu until install.
