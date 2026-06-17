import { z } from "zod";

/** Player progression, keyed by device (guest) and optionally a user account. */
export interface ProfileData {
  deviceId: string;
  displayName: string;
  xp: number;
  coins: number;
  bestWave: number;
  matches: number;
  /** Set once an account is linked (Phase 6). */
  hasAccount: boolean;
}

/** Result of a finished match, used to award progression. */
export interface MatchResult {
  mode: string;
  waves: number;
  kills: number;
  durationMs: number;
}

/**
 * Offline-earned progress the client uploads on reconnect. Bounded to keep a
 * tampered client from injecting absurd values (server still authoritative).
 */
export const ProgressDeltaSchema = z.object({
  xp: z.number().int().min(0).max(1_000_000),
  coins: z.number().int().min(0).max(1_000_000),
  bestWave: z.number().int().min(0).max(10_000),
  matches: z.number().int().min(0).max(100_000),
});
export type ProgressDelta = z.infer<typeof ProgressDeltaSchema>;

export const SYNC_MESSAGE = {
  profile: "profile", // server -> client: authoritative profile
  syncProgress: "syncProgress", // client -> server: offline deltas to merge
} as const;

/** XP/coins economy shared by server and offline client (no drift). */
export function awardFor(result: MatchResult): { xp: number; coins: number } {
  return {
    xp: result.waves * 10 + result.kills * 5,
    coins: result.waves * 2 + result.kills,
  };
}
