import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { getAbility, getClass, type AbilityDef } from "@hunger-arena/shared";
import { useControlStore } from "../store/useControlStore";

/**
 * Owns ability metadata + client-predicted cooldowns for the HUD. The actual
 * input bits flow through the unified control store (set via `pressSlot`), so
 * touch buttons and PC keys share one path into the simulation.
 */
export function useCombat(classId: string) {
  const setAbilityBit = useControlStore((s) => s.setAbilityBit);

  const slots = useMemo<(AbilityDef | undefined)[]>(() => {
    const def = getClass(classId);
    return def.abilities.map((id) => getAbility(id));
  }, [classId]);

  const readyAt = useRef<number[]>([0, 0, 0, 0]);
  const [cooldownPct, setCooldownPct] = useState<number[]>(() =>
    slots.map(() => 0)
  );

  const pressSlot = useCallback(
    (slot: number, down: boolean) => {
      setAbilityBit(slot, down);
      if (down) {
        const ab = slots[slot];
        if (ab && performance.now() >= readyAt.current[slot]) {
          readyAt.current[slot] = performance.now() + ab.cooldownMs;
        }
      }
    },
    [slots, setAbilityBit]
  );

  useEffect(() => {
    const id = setInterval(() => {
      const now = performance.now();
      setCooldownPct(
        slots.map((ab, i) => {
          if (!ab) return 0;
          const remain = readyAt.current[i] - now;
          return remain > 0 ? Math.min(1, remain / ab.cooldownMs) : 0;
        })
      );
    }, 66);
    return () => clearInterval(id);
  }, [slots]);

  return { slots, cooldownPct, pressSlot };
}
