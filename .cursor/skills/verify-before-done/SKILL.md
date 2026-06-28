# Verify Before Done — Starship

Use this skill **before marking any task complete** or opening a PR. It extends `labiblia.md` §19 (merge checklist) and §16 (troubleshooting patterns). New debug-infra rules (Jun 2026) are additive — do not skip existing gates.

## When to apply

- Any gameplay, content, netcode, perf, or Android change
- Adding files under `packages/shared/src/content/`
- Hold/tick/in-progress interactions (mine, place machine, ability cooldown)
- WS message schema changes (server ↔ client)
- Runtime code that can panic or regress FPS

## §19 — Merge gates (existing)

Run and confirm green before done:

```bash
npm test                                    # shared — includes catalog parity + golden movement
npm run test:native                         # cargo test -p ha-shared
cargo check -p ha-game --features android
node .cursor/skills/monorepo-architecture/scripts/check-boundaries.mjs
```

Android/render/lifecycle changes also:

```bash
./scripts/verify-android-startup.sh
./scripts/verify-android-sustained.sh       # optional, perf/lifecycle
```

Architecture invariants:

- Sim logic only in `packages/shared` (Rust mirrors via `ha-shared`, never fork rules)
- No client positions over WS — input only
- Landing `client/` stays download-only
- Zod on every new WS message type

## §16 — Failure patterns to grep before blaming "random bugs"

| Symptom | Likely cause | Check |
|---------|--------------|-------|
| Silent desync | Constant drift TS↔Rust | `golden-movement.test.ts`, `cargo test -p ha-shared golden_movement` |
| New content missing on device | Catalog not ported | Generic parity harness (below) |
| Hold bar infinite after resume | Real time vs Virtual time | `sync_lifecycle_pause`, hold uses `Time` not wall clock |
| WS join fails no logs | Zod reject silenced | Server log must show `validation.issues` |
| "Feels faster" without numbers | No perf baseline | `perf-history.jsonl` delta vs previous entry |
| Crash with no context | Generic Rust panic only | `ha-crash` tag + `ha-last-crash.json` on device |

## Debug infra rules (Jun 2026 — mandatory)

### 1. Catalog parity — generic harness only

Any new module under `packages/shared/src/content/` (ships, bodies, etc.) **must**:

1. Register in `packages/shared/src/test-utils/catalog-parity.ts` (`CATALOG_PARITY_MODULES`)
2. Port fields to `ha-shared` and register in `native/crates/ha-shared/src/catalog_parity.rs` (`rust_module_entries`)
3. Regenerate snapshot: `UPDATE_CATALOG_PARITY=1 npm test -w @hunger-arena/shared -- src/test-utils/catalog-parity.test.ts`

**Do not** write bespoke per-catalog parity tests. Extend the harness.

### 2. Pause/resume on hold/tick state

Any new hold, channel, or in-progress action (like mine, place machine, ability cooldown) **must** answer:

> What happens if `nativeSetPaused(true)` fires mid-action?

Checklist before merge:

- [ ] Progress uses paused game time (Virtual), not wall clock
- [ ] Resume continues from saved progress (no infinite bar)
- [ ] Cancel/release paths reset state (no item consumed without effect)
- [ ] Add or extend test in `packages/shared/src/test-utils/pause-hold.test.ts` and/or `ha-shared/src/hold.rs`

Shared logic: `packages/shared/src/sim/hold.ts` ↔ `ha-shared/src/hold.rs`.

### 3. WS validation logging

Any change to server↔client messages **must** confirm zod failures log with context:

- Message type (`messageType` in slog)
- Failed field paths (`validation.issues` via `server/src/validation.ts`)

Never `safeParse` + silent return without `slog("security", ...)`.

### 4. Perf-sensitive changes — trend, not vibes

Any perf-sensitive change **must**:

1. Run `./scripts/smoke-android-perf.sh` on device (or CI when available)
2. Append entry via `scripts/append-perf-history.sh` → `perf-history.jsonl`
3. Compare FPS/frame_ms vs **previous line** in the JSONL, not memory

Document regression or improvement in PR notes with numbers.

### 5. Structured crash reporting

New runtime-fallible code (unwrap on user data, indexing, asset lookup) **must**:

- Be reachable under the panic hook (`install_crash_hook` in `ha-game`)
- Produce grep-friendly `ha-crash` logcat line with frame, region, input, hold, last perf samples
- Optionally persist `ha-last-crash.json` in app cache on Android

Do not assume generic Rust panic output is enough for field debugging.

### 6. UI icons — catalog + IconFactory only

Before creating any new HUD/UI icon:

1. Check `packages/shared/src/content/ui/icons.ts` — add a **data entry** (shape, palette, cells, variant)
2. Mirror in `native/crates/ha-shared/src/ui/icons.rs`
3. Register `"ui_icons"` parity if fields change
4. Render via `IconFactory` (`ha-game/src/ui/icon_factory.rs`) — **never** hardcode pixel patterns in `hud/icons.rs` or stack extra Bevy UI nodes per pixel

Changing an icon = edit catalog entry + rebuild. One identifiable place per icon.

## Determinism golden test

Movement/constants changes **must** keep TS and Rust in lockstep:

```bash
# After intentional sim change, regenerate golden:
UPDATE_GOLDEN_MOVEMENT=1 npm test -w @hunger-arena/shared -- src/test-utils/golden-movement.test.ts
cargo test -p ha-shared golden_movement
```

Same file: `packages/shared/golden-movement.json`.

## Pre-ship sanity (Android crash hook)

To verify crash reporting on device (temporary, revert after):

1. Add a one-line test panic behind a debug flag or `#[cfg(feature = "crash-test")]`
2. Install APK, trigger panic, confirm `adb logcat | rg ha-crash` shows JSON context
3. Revert test panic before merge

## Quick self-check script

```bash
npm test -w @hunger-arena/shared
cargo test -p ha-shared
cargo check -p ha-game --features android
node .cursor/skills/monorepo-architecture/scripts/check-boundaries.mjs
```

If all green + checklist above satisfied → OK to mark done.
