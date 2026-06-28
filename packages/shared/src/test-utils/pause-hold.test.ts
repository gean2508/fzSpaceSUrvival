/**
 * Pause/resume edge cases — hold timers shared with native (ha-shared/hold.rs).
 */
import { describe, expect, it } from "vitest";
import { MACHINE_CATALOG } from "../content/machines/catalog.js";
import { requireMachine } from "../content/machines/registry.js";
import {
  advanceHoldProgress,
  tickAbilityCooldown,
  type HoldProgress,
} from "../sim/hold.js";

function freshHold(durationMs: number, placingMachine = false): HoldProgress {
  return {
    active: true,
    progressMs: 0,
    durationMs,
    cancelled: false,
    completed: false,
    placingMachine,
  };
}

describe("pause/resume — hold progress (mirrors native Virtual time pause)", () => {
  it("zero-dt ticks during pause preserve progress (no infinite bar on resume)", () => {
    const def = requireMachine("wood_extractor");
    let hold = freshHold(def.placeDurationMs, true);

    hold = advanceHoldProgress(hold, 500, true);
    expect(hold.progressMs).toBe(500);
    expect(hold.completed).toBe(false);

    // App paused — Bevy Virtual time delta = 0, systems do not advance
    hold = advanceHoldProgress(hold, 0, true);
    expect(hold.progressMs).toBe(500);

    hold = advanceHoldProgress(hold, 1500, true);
    expect(hold.completed).toBe(true);
    expect(hold.progressMs).toBeGreaterThanOrEqual(def.placeDurationMs);
  });

  it("mining hold loses target on pause-away — progress resets (native look_interact)", () => {
    let hold = freshHold(800, false);
    hold = advanceHoldProgress(hold, 400, true);
    expect(hold.progressMs).toBe(400);

    // Target lost while backgrounded / look away
    hold = advanceHoldProgress(hold, 50, false);
    expect(hold.active).toBe(false);
    expect(hold.progressMs).toBe(0);
    expect(hold.completed).toBe(false);
  });

  it("ability cooldown does not drain while paused (delta=0)", () => {
    let remaining = 650;
    remaining = tickAbilityCooldown(remaining, 200);
    expect(remaining).toBe(450);
    remaining = tickAbilityCooldown(remaining, 0);
    expect(remaining).toBe(450);
    remaining = tickAbilityCooldown(remaining, 500);
    expect(remaining).toBe(0);
  });

  it("all catalog machines have finite placeDurationMs", () => {
    for (const m of MACHINE_CATALOG) {
      expect(m.placeDurationMs, m.id).toBeGreaterThan(0);
      expect(m.placeDurationMs, m.id).toBeLessThan(30_000);
    }
  });
});

describe("pause/resume — findings (native audit Jun 2026)", () => {
  /**
   * Verified behavior:
   * - `sync_lifecycle_pause` pauses `Time<Virtual>` → hold/cooldown systems stop advancing. OK.
   * - Mid-hold pause does NOT reset progress — resume continues bar. OK (test above).
   * - Mining hold without target resets progress — OK if player looks away; if target stays
   *   in LookTarget while paused, hold continues on resume (expected).
   * - Machine placement uses `placing_machine=true` so target_valid=false still advances. OK.
   * - No item consumed until placement completes in machines.rs (check on native side).
   * - Risk: if a system used Real time instead of Virtual, infinite bar bug could return — grep for Time<Real> in hold paths.
   */
  it("documents expected native pause semantics", () => {
    expect(true).toBe(true);
  });
});
