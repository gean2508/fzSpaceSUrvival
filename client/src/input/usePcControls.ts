import { useEffect } from "react";
import { useControlStore } from "../store/useControlStore";

/** Q/E/R → ability slots 0/1/2 (slot 3 reserved). */
const ABILITY_KEYS: Record<string, number> = { q: 0, e: 1, r: 2 };

/**
 * PC keyboard scheme. Mouse aim is handled by the raycaster plane in the scene
 * (calls setAim). Active only while the platform is "pc"; listeners are torn
 * down otherwise so the platforms never interfere.
 *
 *  WASD / arrows → move      Shift → sprint
 *  Space         → parkour/interact action
 *  Q / E / R     → abilities
 */
export function usePcControls(
  enabled: boolean,
  pressSlot: (slot: number, down: boolean) => void
) {
  const setMove = useControlStore((s) => s.setMove);
  const setSprint = useControlStore((s) => s.setSprint);
  const setAction = useControlStore((s) => s.setAction);

  useEffect(() => {
    if (!enabled) return;
    const keys: Record<string, boolean> = {};

    const recompute = () => {
      let dx = 0;
      let dz = 0;
      if (keys["a"] || keys["arrowleft"]) dx -= 1;
      if (keys["d"] || keys["arrowright"]) dx += 1;
      if (keys["w"] || keys["arrowup"]) dz -= 1;
      if (keys["s"] || keys["arrowdown"]) dz += 1;
      const len = Math.hypot(dx, dz);
      if (len > 1) {
        dx /= len;
        dz /= len;
      }
      setMove(dx, dz);
    };

    const onDown = (e: KeyboardEvent) => {
      const k = e.key.toLowerCase();
      if (keys[k]) return; // ignore auto-repeat
      keys[k] = true;
      if (k === "shift") setSprint(true);
      else if (k === " ") setAction(true);
      else if (k in ABILITY_KEYS) pressSlot(ABILITY_KEYS[k], true);
      else recompute();
    };
    const onUp = (e: KeyboardEvent) => {
      const k = e.key.toLowerCase();
      keys[k] = false;
      if (k === "shift") setSprint(false);
      else if (k === " ") setAction(false);
      else if (k in ABILITY_KEYS) pressSlot(ABILITY_KEYS[k], false);
      else recompute();
    };
    const onBlur = () => {
      for (const k of Object.keys(keys)) keys[k] = false;
      setMove(0, 0);
      setSprint(false);
      setAction(false);
    };

    window.addEventListener("keydown", onDown);
    window.addEventListener("keyup", onUp);
    window.addEventListener("blur", onBlur);
    return () => {
      window.removeEventListener("keydown", onDown);
      window.removeEventListener("keyup", onUp);
      window.removeEventListener("blur", onBlur);
      onBlur();
    };
  }, [enabled, pressSlot, setMove, setSprint, setAction]);
}
