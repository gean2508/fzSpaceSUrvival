# Player ↔ World Design Contract

This document ties **player capabilities** (parkour, controls, stamina) to **world generation**
rules so every island supports the intended gameplay. Source of truth for movement numbers:
`packages/shared/src/sim/constants.ts` + `parkour.ts` (mirrored in `ha-shared`).

## Player capabilities

| Action | Input | Sim rule |
|--------|-------|----------|
| Move | Left stick (38% screen), camera-relative | `groundLocomotion`, friction 12/s |
| Sprint | Stick pushed past ~82% radius | ×1.6 speed, drains stamina 12/s |
| Look | Right 62% drag | Yaw/pitch on camera only |
| Jump | **USE tap** (edge, not hold) | `JUMP_VELOCITY` 6.8, costs 8 stamina |
| Climb | **USE hold** at climbable wall | ≤3.0 m tall, drains 26/s |
| Mantle | **USE** while hanging | Nudge forward 0.6 m onto ledge |
| Step up | Walk into low ledge | Auto if ≤0.35 m (`STEP_HEIGHT`) |

**USE disambiguation:** tap → jump (if grounded, not at climbable wall); hold → interact/climb
(`HoldInteractState.active` suppresses jump edge).

## World must support

### Verticality

- **Climbable walls:** obstacle height **0.5–3.0 m** (above 3 m = blocking cliff, not climb).
- **Steppable ledges:** 0–0.35 m without climb animation.
- **Mantle targets:** flat tops at climb max height with ≥0.6 m forward clearance after mantle.
- **Ground height:** `groundHeightAt(x,z)` = max obstacle top under AABB footprint (parkour gravity).

### Obstacle density (seed 42 baseline)

- **28 collision obstacles** across `WORLD_HALF = 96` (~192×192 playable).
- Min spacing ~8 m in generator — enough lanes for sprint + jump, not maze-like.
- Mix: short cover (1–1.5 m), climb walls (2–3 m), occasional tall blockers (>3 m) as landmarks.

### Island layout

- **Beach ring** at edges: low slope, no sudden pits — safe spawn and tutorial movement.
- **Interior:** rolling height via props + obstacles; cliffs use `height > MAX_CLIMB_HEIGHT` intentionally.
- **Ocean** (`OCEAN_HALF = 384`) is visual only; player clamped to `WORLD_HALF - radius`.

### Chunk streaming (40×40)

- Collision list is **full world** at sim init (28 boxes — cheap).
- Visual decor streams 3×3 chunks; climb/jump uses sim obstacles, not render LOD.

## Control feel (native client)

| Parameter | Value | Rationale |
|-----------|-------|-----------|
| Stick radius | ~13.2% of min(screen w,h) | Larger target, finer control |
| Deadzone | 0.07 | Less drift, still forgiving |
| Input smooth | 42/s exponential | Responsive but not twitchy |
| Sim tick | 20 Hz + visual lerp | Matches server; smooth between ticks |
| Touch split | 38% move / 62% look | Matches `layout.rs` |

## Map design checklist (content authors)

1. Every climb route has a **landing** ≥ player radius + 0.6 m.
2. No mandatory jumps over gaps **> 4 m** without alternative path (jump arc ~2 m at default velocity).
3. Stamina pads: regen 14/s — long climb chains need rest or shorter walls.
4. Interactables placed so **hold USE** at wall doesn’t block needed **tap jump** (avoid overlap at same button moment).
5. FP view: keep interact prompts within **INTERACT_REACH** (~2.5 m) of camera forward.

## Performance note (before animation/mobs / space features)

Smoke target on Redmi: **30+ FPS** required before ships, interiors, or colonists. Current ~14–18 FPS @ 2297×1080.

Do not add viewmodel/ mob animation / flight scenes until F3/ha-perf breakdown is green. Animation time source: `Time<Virtual>` (respects pause).

Space exploration design: `native/docs/SPACE_SURVIVAL_VISION.md`.

## Files

- Sim: `packages/shared/src/sim/parkour.ts`, `constants.ts`
- Native mirror: `native/crates/ha-shared/src/parkour.rs`, `world.rs`
- Client wiring: `native/crates/ha-game/src/sim.rs`, `input/joystick.rs`, `input/layout.rs`
- World gen: `packages/shared/src/sim/generateWorld.ts`, `native/crates/ha-shared/src/world.rs`
