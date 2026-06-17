import { create } from "zustand";
import type { ProfileData } from "@hunger-arena/shared";
import type { GameMode } from "../net/Transport";

export type Screen =
  | "splash"
  | "menu"
  | "mode"
  | "class"
  | "auth"
  | "settings"
  | "game"
  | "results";

export interface MatchSummary {
  waves: number;
  mode: GameMode;
}

interface GameStore {
  screen: Screen;
  mode: GameMode;
  classId: string;
  profile: ProfileData | null;
  lastResult: MatchSummary | null;

  go: (screen: Screen) => void;
  setMode: (mode: GameMode) => void;
  setClassId: (classId: string) => void;
  setProfile: (profile: ProfileData | null) => void;
  endMatch: (summary: MatchSummary) => void;
}

export const useGameStore = create<GameStore>((set) => ({
  screen: "splash",
  mode: "offline",
  classId: "scout",
  profile: null,
  lastResult: null,

  go: (screen) => set({ screen }),
  setMode: (mode) => set({ mode }),
  setClassId: (classId) => set({ classId }),
  setProfile: (profile) => set({ profile }),
  endMatch: (summary) => set({ lastResult: summary, screen: "results" }),
}));
