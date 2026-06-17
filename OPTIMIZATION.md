# Optimization Rules — Hunger Arena

Performance is a feature. The game must stay fluid on a mid-range phone (target
**60 fps**, acceptable floor **30 fps**) across offline / LAN / online. These are
the rules every change must respect.

## Budgets (target device: Redmi 10-class Android)
- Frame time: < 16 ms (60 fps). Hard floor 33 ms.
- Draw calls: < 150 in a busy wave.
- Active entities rendered: ~80 without frame drops.
- JS heap: avoid per-frame allocations in the render/sim hot paths.

## Rendering (client / R3F)
1. **No React state for hot objects.** Positions/rotations are mutated via
   `useRef` + `useFrame` only. (See `EntityView.tsx`, `ArenaController.ts`.)
2. **Mesh pooling.** Entities recycle pooled groups per kind instead of
   create/dispose each spawn — kills GC churn during waves. (`EntityView.tsx`.)
3. **Cap pixel ratio.** `<Canvas dpr={[1, 2]}>`. Never render at raw retina dpr.
4. **Cache GPU resources.** Gradient maps and shared geometries are cached
   (`art/toon.ts`); never rebuild per frame/entity.
5. **Cel materials over PBR.** `MeshToonMaterial` is cheaper than
   `MeshStandardMaterial` and is the art direction anyway.
6. **Cheap outlines.** Inverted-hull back-face mesh, not a postprocess edge pass.
7. **Subtle postprocessing only.** One bloom + one vignette. No heavy SSAO/SSR.
8. **Shadows bounded.** Single shadow-casting directional light, tight shadow
   camera frustum, 2048 map (drop to 1024 if needed).
9. **Interpolation, not flooding.** Render remotes ~100 ms in the past and lerp
   between 20 Hz snapshots instead of raising tick rate.
10. **Instancing (future).** When a single NPC/prop type exceeds ~100 instances,
    move it to `InstancedMesh`. Pooling covers current MVP counts.

## Simulation / netcode
1. Fixed tick **20 Hz** (`TICK_MS`). Do not couple sim to render fps.
2. Server is authoritative; clients send **inputs only** (also a security rule).
3. Client-side prediction + reconciliation for the local player; replay only the
   unacked inputs, not the whole history.
4. Keep the shared sim allocation-light; reuse vectors/objects where hot.
5. Snapshot buffer is time-bounded (`SNAPSHOT_BUFFER_MS`) so memory stays flat.

## Network payload
1. Colyseus schema syncs deltas only — keep `EntityState` fields minimal.
2. RTT probe (`ping`/`pong`) is throttled to every 2 s; never per frame.
3. Validate + clamp every inbound message (`zod`) before it touches the sim.

## Profiling workflow
- Toggle the debug overlay with the **`** key (or `?debug`): FPS, RTT, tick,
  entity count, reconcile error, and the `r3f-perf` panel (draw calls, GPU time).
- Watch for: rising draw calls (missing pooling/instancing), reconcile error
  spikes (netcode), heap saw-tooth (per-frame allocation).

## Checklist for any visual/sim PR
- [ ] No new per-frame allocations in `useFrame` / sim `step`.
- [ ] New meshes are pooled or instanced if they spawn in bulk.
- [ ] Colors come from `PALETTE`; materials are toon.
- [ ] dpr cap and shadow budget untouched (or justified).
- [ ] Verified with the debug overlay under a full wave.
