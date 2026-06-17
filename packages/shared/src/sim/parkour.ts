import {
  CLIMB_SPEED,
  FRICTION,
  GRAVITY,
  MANTLE_FORWARD,
  MAX_CLIMB_HEIGHT,
  SPRINT_MULT,
  STAMINA_CLIMB_DRAIN,
  STAMINA_EXHAUSTED_RECOVER,
  STAMINA_MAX,
  STAMINA_REGEN,
  STAMINA_SPRINT_DRAIN,
  STEP_HEIGHT,
  WORLD_HALF,
} from "./constants.js";
import type { InputState, MovementState, Obstacle } from "./types.js";
import { groundHeightAt } from "./world.js";

/**
 * Mutable body the parkour state machine operates on. Superset of the old
 * MovementBody so existing callers keep working. Verticality (y/vy), stamina
 * and the FSM state live here so client prediction and server stay in lockstep.
 */
export interface MovementBody {
  x: number;
  y: number;
  z: number;
  vx: number;
  vz: number;
  vy: number;
  rot: number;
  speed: number;
  radius: number;
  stamina: number;
  maxStamina: number;
  state: MovementState;
}

/** Ensure older bodies (created without parkour fields) get sane defaults. */
export function ensureParkourFields(b: Partial<MovementBody> & { speed: number; radius: number }): MovementBody {
  const body = b as MovementBody;
  if (body.y === undefined) body.y = 0;
  if (body.vy === undefined) body.vy = 0;
  if (body.stamina === undefined) body.stamina = STAMINA_MAX;
  if (body.maxStamina === undefined) body.maxStamina = STAMINA_MAX;
  if (body.state === undefined) body.state = "idle";
  return body;
}

function clampWorld(b: MovementBody): void {
  const lim = WORLD_HALF - b.radius;
  if (b.x < -lim) {
    b.x = -lim;
    b.vx = 0;
  } else if (b.x > lim) {
    b.x = lim;
    b.vx = 0;
  }
  if (b.z < -lim) {
    b.z = -lim;
    b.vz = 0;
  } else if (b.z > lim) {
    b.z = lim;
    b.vz = 0;
  }
}

/**
 * Resolve horizontal collision against obstacle AABBs. Boxes whose top is below
 * the body's feet + STEP_HEIGHT are walkable (we stand on them); taller boxes
 * block. Returns the blocking obstacle we're pressing into (for climbing), or
 * null. Deterministic: obstacles are iterated in array order.
 */
function resolveBlocking(b: MovementBody, obstacles: Obstacle[]): Obstacle | null {
  let pressing: Obstacle | null = null;
  for (const o of obstacles) {
    // Standing on / above this box → not a wall.
    if (b.y + STEP_HEIGHT >= o.height) continue;
    const r = b.radius;
    const dx = b.x - o.x;
    const dz = b.z - o.z;
    const px = o.halfW + r - Math.abs(dx); // penetration on X
    const pz = o.halfD + r - Math.abs(dz); // penetration on Z
    if (px <= 0 || pz <= 0) continue; // no overlap

    // Push out along the least-penetrating axis (the wall face we hit).
    if (px < pz) {
      b.x += dx >= 0 ? px : -px;
      if ((dx >= 0 && b.vx < 0) || (dx < 0 && b.vx > 0)) b.vx = 0;
    } else {
      b.z += dz >= 0 ? pz : -pz;
      if ((dz >= 0 && b.vz < 0) || (dz < 0 && b.vz > 0)) b.vz = 0;
    }
    pressing = o;
  }
  return pressing;
}

/** Nearest obstacle whose footprint overlaps the body (ignores vertical skip). */
function findTouchingWall(b: MovementBody, obstacles: Obstacle[]): Obstacle | null {
  let best: Obstacle | null = null;
  let bestPen = 0;
  for (const o of obstacles) {
    const dx = b.x - o.x;
    const dz = b.z - o.z;
    const px = o.halfW + b.radius - Math.abs(dx);
    const pz = o.halfD + b.radius - Math.abs(dz);
    if (px <= 0 || pz <= 0) continue;
    const pen = Math.min(px, pz);
    if (pen > bestPen) {
      bestPen = pen;
      best = o;
    }
  }
  return best;
}

/** Wall the body is climbing or hanging on (horizontal grip + grace while ascending). */
function findClimbWall(b: MovementBody, obstacles: Obstacle[]): Obstacle | null {
  const touch = findTouchingWall(b, obstacles);
  if (touch) return touch;
  for (const o of obstacles) {
    if (o.height > MAX_CLIMB_HEIGHT) continue;
    if (b.y >= o.height) continue;
    const dx = b.x - o.x;
    const dz = b.z - o.z;
    const grace = 0.4;
    if (
      Math.abs(dx) <= o.halfW + b.radius + grace &&
      Math.abs(dz) <= o.halfD + b.radius + grace
    ) {
      return o;
    }
  }
  return null;
}

/** Is the body actively pushing toward the wall box? */
function pushingInto(b: MovementBody, o: Obstacle, mx: number, mz: number): boolean {
  const tx = o.x - b.x;
  const tz = o.z - b.z;
  const len = Math.hypot(tx, tz);
  if (len < 0.0001) return true;
  return (mx * tx + mz * tz) / len > 0.2;
}

/**
 * Single source of truth for player movement + parkour. Used by the server AND
 * client prediction. Same inputs + obstacles → identical result.
 */
export function stepMovement(
  raw: Partial<MovementBody> & { speed: number; radius: number },
  input: InputState,
  obstacles: Obstacle[],
  dt: number
): void {
  const b = ensureParkourFields(raw);

  let mx = input.moveX;
  let mz = input.moveZ;
  const moveLen = Math.hypot(mx, mz);
  if (moveLen > 1) {
    mx /= moveLen;
    mz /= moveLen;
  }

  // Facing: aim wins, else move direction.
  if (input.aimX !== 0 || input.aimZ !== 0) {
    b.rot = Math.atan2(input.aimX, input.aimZ);
  } else if (moveLen > 0.01) {
    b.rot = Math.atan2(mx, mz);
  }

  // ---- EXHAUSTED: forced down, slow, no climbing until recovered ----
  if (b.state === "exhausted") {
    b.stamina = Math.min(b.maxStamina, b.stamina + STAMINA_REGEN * dt);
    groundLocomotion(b, mx, mz, 0.5, dt);
    resolveBlocking(b, obstacles);
    applyGravity(b, obstacles, dt);
    clampWorld(b);
    if (b.stamina >= STAMINA_EXHAUSTED_RECOVER) b.state = "idle";
    return;
  }

  // ---- HANGING: hold edge; press action to mantle, release to drop ----
  if (b.state === "hanging") {
    b.stamina -= STAMINA_CLIMB_DRAIN * 0.5 * dt;
    b.vx = 0;
    b.vz = 0;
    b.vy = 0;
    if (b.stamina <= 0) {
      b.stamina = 0;
      b.state = "exhausted";
      return;
    }
    if (input.action) {
      // Mantle: hop onto the top surface, nudge forward over the ledge.
      b.y += 0.05;
      b.x += Math.sin(b.rot) * MANTLE_FORWARD;
      b.z += Math.cos(b.rot) * MANTLE_FORWARD;
      b.state = "running";
    }
    clampWorld(b);
    return;
  }

  // ---- CLIMBING: ascend wall face ----
  if (b.state === "climbing") {
    b.stamina -= STAMINA_CLIMB_DRAIN * dt;
    if (b.stamina <= 0) {
      b.stamina = 0;
      b.state = "exhausted";
      b.vy = 0;
      return;
    }
    const wall = findClimbWall(b, obstacles);
    if (!wall || !input.action) {
      // Let go: drop back to ground locomotion.
      b.state = "idle";
      return;
    }
    b.y += CLIMB_SPEED * dt;
    b.vx = 0;
    b.vz = 0;
    if (b.y >= wall.height - 0.05) {
      b.y = wall.height;
      b.state = "hanging";
    }
    clampWorld(b);
    return;
  }

  // ---- GROUND: idle / running / sprinting / wall_trek ----
  const wantsSprint = input.sprint === 1 && moveLen > 0.1 && b.stamina > 0;
  const speedMult = wantsSprint ? SPRINT_MULT : 1;
  groundLocomotion(b, mx, mz, speedMult, dt);

  // Stamina: sprint drains, otherwise regen.
  if (wantsSprint) {
    b.stamina = Math.max(0, b.stamina - STAMINA_SPRINT_DRAIN * dt);
  } else {
    b.stamina = Math.min(b.maxStamina, b.stamina + STAMINA_REGEN * dt);
  }

  const wall = resolveBlocking(b, obstacles);
  applyGravity(b, obstacles, dt);
  clampWorld(b);

  // Decide ground state / initiate climb.
  if (wall && pushingInto(b, wall, mx, mz)) {
    const canClimb =
      wall.height <= MAX_CLIMB_HEIGHT &&
      b.stamina > 0 &&
      b.y < wall.height - STEP_HEIGHT;
    if (canClimb && input.action) {
      b.state = "climbing";
    } else {
      b.state = "wall_trek";
    }
  } else if (moveLen > 0.1) {
    b.state = wantsSprint ? "sprinting" : "running";
  } else {
    b.state = "idle";
  }
}

function groundLocomotion(
  b: MovementBody,
  mx: number,
  mz: number,
  mult: number,
  dt: number
): void {
  const targetVx = mx * b.speed * mult;
  const targetVz = mz * b.speed * mult;
  const k = Math.min(1, FRICTION * dt);
  b.vx += (targetVx - b.vx) * k;
  b.vz += (targetVz - b.vz) * k;
  b.x += b.vx * dt;
  b.z += b.vz * dt;
}

/** Snap to / fall toward the surface under the body. */
function applyGravity(b: MovementBody, obstacles: Obstacle[], dt: number): void {
  const ground = groundHeightAt(b.x, b.z, obstacles);
  if (b.y > ground + 0.001) {
    b.vy -= GRAVITY * dt;
    b.y += b.vy * dt;
    if (b.y <= ground) {
      b.y = ground;
      b.vy = 0;
    }
  } else {
    b.y = ground;
    b.vy = 0;
  }
}
