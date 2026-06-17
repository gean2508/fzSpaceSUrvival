import { create } from "zustand";
import { isMobile } from "../config";

export type Platform = "pc" | "touch";

/**
 * Normalized input both platforms emit toward the simulation. This object is
 * MUTATED IN PLACE every frame (hot path) — never store it in React state or
 * subscribe to it for rendering. Read it via `useControlStore.getState().payload`.
 */
export interface InputPayload {
  moveX: number; // -1..1 (left stick / WASD)
  moveZ: number;
  aimX: number; // unit look/aim direction (right pad / mouse)
  aimZ: number;
  abilities: number; // bitmask (combat buttons / Q E R)
  sprint: number; // 0 | 1
  action: number; // 0 | 1 (parkour: climb/mantle, interact/gather)
}

function blankPayload(): InputPayload {
  return { moveX: 0, moveZ: 0, aimX: 0, aimZ: 1, abilities: 0, sprint: 0, action: 0 };
}

interface ControlState {
  /** Active platform. Only this platform's listeners/UI are mounted. */
  platform: Platform;
  /** Live, mutated-in-place input. Do not subscribe for rendering. */
  payload: InputPayload;

  setPlatform(p: Platform): void;
  setMove(x: number, z: number): void;
  setAim(x: number, z: number): void;
  setSprint(on: boolean): void;
  setAction(on: boolean): void;
  setAbilityBit(slot: number, on: boolean): void;
  /** Clear held buttons (abilities/sprint/action) between frames or on teardown. */
  resetTransient(): void;
  /** Full reset when leaving the match or switching platform. */
  resetPayload(): void;
}

export const useControlStore = create<ControlState>((set, get) => ({
  platform: isMobile ? "touch" : "pc",
  payload: blankPayload(),

  setPlatform: (platform) => set({ platform }),

  setMove: (x, z) => {
    const p = get().payload;
    p.moveX = x;
    p.moveZ = z;
  },
  setAim: (x, z) => {
    const len = Math.hypot(x, z);
    const p = get().payload;
    if (len > 0.0001) {
      p.aimX = x / len;
      p.aimZ = z / len;
    }
  },
  setSprint: (on) => {
    get().payload.sprint = on ? 1 : 0;
  },
  setAction: (on) => {
    get().payload.action = on ? 1 : 0;
  },
  setAbilityBit: (slot, on) => {
    const p = get().payload;
    const bit = 1 << slot;
    if (on) p.abilities |= bit;
    else p.abilities &= ~bit;
  },
  resetTransient: () => {
    const p = get().payload;
    p.abilities = 0;
    p.sprint = 0;
    p.action = 0;
  },
  resetPayload: () => {
    const p = get().payload;
    p.moveX = 0;
    p.moveZ = 0;
    p.aimX = 0;
    p.aimZ = 1;
    p.abilities = 0;
    p.sprint = 0;
    p.action = 0;
  },
}));
