# Ghibli Toon — Reference Snippets

Copy/paste building blocks. Keep colors coming from `PALETTE`.

## Palette (client/src/game/art/palette.ts)

| Key          | Hex       | Use                         |
| ------------ | --------- | --------------------------- |
| skyTop       | `#8ec5e6` | upper sky / hemi sky        |
| skyHorizon   | `#dbe7c9` | horizon glow                |
| sunset       | `#f4a259` | key light / accents         |
| fog          | `#cfe3df` | fog + background            |
| meadow       | `#9bc36b` | ground                      |
| meadowDeep   | `#6f9e54` | ground base / hemi ground   |
| earth        | `#caa472` | dirt / paths                |
| bark         | `#5a4632` | trunks / dark detail        |
| water        | `#7fc4c9` | water                       |
| hero         | `#5ec2d6` | local player                |
| rival        | `#e07a7a` | remote players              |
| foe          | `#d98a5a` | NPCs                        |
| spark        | `#ffd98a` | projectiles / glow          |
| cream        | `#fff7e6` | rim / highlights / UI       |
| night        | `#2b2f3a` | outlines / UI dark          |

## Toon material

```ts
import { makeToonMaterial, makeOutline } from "./art/toon";
import { PALETTE } from "./art/palette";

const mat = makeToonMaterial(PALETTE.hero, {
  steps: 4,
  emissive: PALETTE.hero,
  emissiveIntensity: 0.2,
});
group.add(makeOutline(geometry, 0.07, 0x2b2f3a)); // ink silhouette
```

JSX form for static meshes:

```tsx
<meshToonMaterial color={PALETTE.meadow} gradientMap={makeToonGradient(4)} />
```

## Lighting + atmosphere

```tsx
<hemisphereLight args={[PALETTE.skyTop, PALETTE.meadowDeep, 0.7]} />
<directionalLight position={[14, 22, 10]} intensity={1.5} color={PALETTE.sunset} castShadow />
<SoftShadows size={28} samples={12} focus={0.6} />
<color attach="background" args={[PALETTE.fog]} />
<fog attach="fog" args={[PALETTE.fog, 40, 90]} />
<Sky distance={450000} sunPosition={[14, 12, 10]} turbidity={4} rayleigh={1.2} />
<Clouds material={THREE.MeshBasicMaterial}>
  <Cloud seed={1} bounds={[26, 4, 12]} volume={9} color={PALETTE.cream} opacity={0.7} />
</Clouds>
```

## Postprocessing (subtle)

```tsx
<EffectComposer>
  <Bloom mipmapBlur intensity={0.45} luminanceThreshold={0.75} luminanceSmoothing={0.2} />
  <Vignette offset={0.25} darkness={0.55} />
</EffectComposer>
```

## Tuning notes
- Too plasticky? Lower `steps` to 3 and reduce `emissiveIntensity`.
- Too neon? Lower bloom `intensity` / raise `luminanceThreshold`.
- Muddy shadows? Raise hemi light intensity or warm the directional color.
- Mobile jank? Cap `dpr={[1,2]}`, drop shadow map to 1024, fewer cloud segments.
- Outlines z-fighting? Increase outline thickness slightly or set `renderOrder`.
