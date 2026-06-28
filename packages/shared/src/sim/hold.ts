/** Hold / cooldown tick helpers — mirrors native hold systems; used by pause/resume tests. */

export type HoldProgress = {
  active: boolean;
  progressMs: number;
  durationMs: number;
  cancelled: boolean;
  completed: boolean;
  /** Hammer placement without interact target (native `placing_machine`). */
  placingMachine: boolean;
};

/** Mirrors `tick_hold_interact` in ha-game when target_valid = entity present OR placing machine. */
export function advanceHoldProgress(
  hold: HoldProgress,
  deltaMs: number,
  targetValid: boolean
): HoldProgress {
  if (hold.completed || hold.cancelled) {
    return { ...hold, active: false };
  }
  if (!hold.active) {
    return hold;
  }
  if (!targetValid && !hold.placingMachine) {
    return { ...hold, active: false, progressMs: 0 };
  }
  const progressMs = hold.progressMs + Math.max(0, deltaMs);
  if (progressMs >= hold.durationMs) {
    return { ...hold, active: false, completed: true, progressMs };
  }
  return { ...hold, progressMs };
}

/** Mirrors `tick_ability_cooldown` — pass deltaMs=0 while app paused. */
export function tickAbilityCooldown(remainingMs: number, deltaMs: number): number {
  if (remainingMs <= 0 || deltaMs <= 0) {
    return Math.max(0, remainingMs);
  }
  return Math.max(0, remainingMs - deltaMs);
}
