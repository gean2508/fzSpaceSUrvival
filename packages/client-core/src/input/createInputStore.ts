import { create } from "zustand";
import type { InputState } from "@hunger-arena/shared";

export type Platform = "pc" | "touch";

/**
 * Client intent WITHOUT seq — ArenaController adds seq each tick.
 * MUST match InputState fields (see packages/shared InputMessageSchema).
 */
export type InputPayload = Omit<InputState, "seq">;

function blankPayload(): InputPayload {
  return { moveX: 0, moveZ: 0, aimX: 0, aimZ: 1, abilities: 0, sprint: 0, action: 0 };
}

export interface InputStore {
  platform: Platform;
  /** Mutated in place every frame — never subscribe for rendering. */
  payload: InputPayload;
  setPlatform(p: Platform): void;
  setMove(x: number, z: number): void;
  setAim(x: number, z: number): void;
  setSprint(on: boolean): void;
  setAction(on: boolean): void;
  setAbilityBit(slot: number, on: boolean): void;
  resetTransient(): void;
  resetPayload(): void;
}

/** Factory — one store per app (desktop=pc, mobile=touch). */
export function createInputStore(defaultPlatform: Platform) {
  return create<InputStore>((set, get) => ({
    platform: defaultPlatform,
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
}
