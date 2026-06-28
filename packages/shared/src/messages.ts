import { z } from "zod";
import { ABILITY_MASK, CRAFT_INTENT_MASK } from "./sim/types.js";

export const ROOM_NAME = "survival" as const;

/**
 * Client -> server per-tick input. The server validates and clamps this; it is
 * the ONLY movement authority. Positions are never accepted from clients.
 */
export const InputMessageSchema = z.object({
  seq: z.number().int().nonnegative(),
  moveX: z.number().min(-1).max(1),
  moveZ: z.number().min(-1).max(1),
  aimX: z.number().min(-1).max(1),
  aimZ: z.number().min(-1).max(1),
  abilities: z
    .number()
    .int()
    .min(0)
    .max(ABILITY_MASK | CRAFT_INTENT_MASK),
  sprint: z.number().int().min(0).max(1),
  action: z.number().int().min(0).max(1),
});
export type InputMessage = z.infer<typeof InputMessageSchema>;

/** Join options the client may pass (validated server-side). */
export const JoinOptionsSchema = z.object({
  classId: z.string().min(1).max(32),
  deviceId: z.string().min(1).max(128).optional(),
  token: z.string().max(4096).optional(),
});
export type JoinOptions = z.infer<typeof JoinOptionsSchema>;

export const MESSAGE = {
  input: "input",
  ping: "ping",
  pong: "pong",
} as const;
