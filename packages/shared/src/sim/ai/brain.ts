import type { AiState, NpcAi } from "./types.js";

export interface Vec {
  x: number;
  z: number;
}

interface TargetInfo {
  x: number;
  z: number;
  radius: number;
  dist: number;
}

/**
 * Pure FSM transition for one NPC. Decides the next behavioural state from
 * perception + personality. Steering/movement is applied by the simulation.
 */
export function decideState(
  ai: NpcAi,
  hpFrac: number,
  target: TargetInfo | null
): AiState {
  const p = ai.personality;
  // Courageous NPCs rarely flee; cowardly ones bail at half health.
  const fleeThreshold = (1 - p.courage) * 0.5;

  if (target) {
    if (hpFrac < fleeThreshold) return "flee";
    if (target.dist <= p.attackRange + target.radius) return "attack";
    return "chase";
  }
  return "wander";
}

/** Desired unit move direction for a state. Null means "hold position". */
export function desiredDir(
  state: AiState,
  npc: Vec,
  ai: NpcAi,
  target: TargetInfo | null
): Vec | null {
  switch (state) {
    case "chase":
      return target ? unit(target.x - npc.x, target.z - npc.z) : null;
    case "flee":
      return target ? unit(npc.x - target.x, npc.z - target.z) : null;
    case "attack":
      return null;
    case "return":
      return unit(ai.homeX - npc.x, ai.homeZ - npc.z);
    case "wander":
      return { x: Math.sin(ai.wanderAngle), z: Math.cos(ai.wanderAngle) };
    default:
      return null;
  }
}

function unit(x: number, z: number): Vec {
  const l = Math.hypot(x, z);
  return l > 0.0001 ? { x: x / l, z: z / l } : { x: 0, z: 0 };
}
