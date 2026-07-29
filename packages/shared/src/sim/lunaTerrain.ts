/**
 * Lunar height field — TS mirror of `native/crates/ha-shared/src/luna_terrain.rs`.
 * Arcade scale; Mare + crater topology for gameplay walk sim.
 */

import {
  COYOTE_TIME_SECS,
  FRICTION,
  GRAVITY,
  JUMP_STAMINA_COST,
  JUMP_VELOCITY,
  MAX_WALK_SLOPE,
  SPRINT_MULT,
  STAMINA_MAX,
  STAMINA_REGEN,
  STAMINA_SPRINT_DRAIN,
  STEP_HEIGHT,
} from "./constants.js";
import { MOON_GAMEPLAY_RADIUS_M } from "./cosmic_scale.js";
import type { MovementBody } from "./parkour.js";
import type { InputState } from "./types.js";

export const LUNA_TANGENT_HALF = Math.PI * MOON_GAMEPLAY_RADIUS_M * 0.5;

const CRATER_SEED = 0x1a6a6001;

interface LunaCrater {
  dir: [number, number, number];
  angularR: number;
  depth: number;
  rim: number;
}

function hashU32(x: number): number {
  let v = x >>> 0;
  v ^= v >>> 16;
  v = Math.imul(v, 0x7feb352d) >>> 0;
  v ^= v >>> 15;
  v = Math.imul(v, 0x846ca68b) >>> 0;
  v ^= v >>> 16;
  return v >>> 0;
}

function normalize3(v: [number, number, number]): [number, number, number] {
  const len = Math.hypot(v[0], v[1], v[2]) || 1e-6;
  return [v[0] / len, v[1] / len, v[2] / len];
}

function dot3(a: [number, number, number], b: [number, number, number]): number {
  return a[0] * b[0] + a[1] * b[1] + a[2] * b[2];
}

function craterCatalog(): LunaCrater[] {
  const out: LunaCrater[] = [];
  for (let i = 0; i < 56; i++) {
    const h = hashU32(CRATER_SEED ^ Math.imul(i, 0x9e3779b9));
    const h2 = hashU32(h);
    const h3 = hashU32(h2);
    const theta = (h / 0xffffffff) * Math.PI * 2;
    const phi = (h2 / 0xffffffff) * Math.PI * 0.92 + 0.04;
    const dir: [number, number, number] = [
      Math.sin(phi) * Math.cos(theta),
      Math.cos(phi),
      Math.sin(phi) * Math.sin(theta),
    ];
    const scale = (h3 & 3) === 0 ? 1.35 : 1.0;
    const angularR = (0.012 + ((h2 & 0xff) / 255) * 0.045) * scale;
    const depth = (2.5 + ((h3 & 0x7f) / 127) * 9.0) * scale;
    out.push({ dir, angularR, depth, rim: depth * 0.22 });
  }
  return out;
}

const CRATERS = craterCatalog();

function smoothNoise3(dir: [number, number, number], freq: number): number {
  const p = [dir[0] * freq, dir[1] * freq, dir[2] * freq];
  const ix = Math.floor(p[0]);
  const iy = Math.floor(p[1]);
  const iz = Math.floor(p[2]);
  const fx = p[0] - ix;
  const fy = p[1] - iy;
  const fz = p[2] - iz;
  const ux = fx * fx * (3 - 2 * fx);
  const uy = fy * fy * (3 - 2 * fy);
  const uz = fz * fz * (3 - 2 * fz);
  const h = (x: number, y: number, z: number) => {
    const s = Math.sin(x * 127.1 + y * 311.7 + z * 74.7) * 43758.5453;
    return s - Math.floor(s);
  };
  const a = h(ix, iy, iz);
  const b = h(ix + 1, iy, iz);
  const c = h(ix, iy + 1, iz);
  const d = h(ix + 1, iy + 1, iz);
  const e = h(ix, iy, iz + 1);
  const f = h(ix + 1, iy, iz + 1);
  const g = h(ix, iy + 1, iz + 1);
  const hh = h(ix + 1, iy + 1, iz + 1);
  const x0 = a + (b - a) * ux;
  const x1 = c + (d - c) * ux;
  const x2 = e + (f - e) * ux;
  const x3 = g + (hh - g) * ux;
  const y0 = x0 + (x1 - x0) * uy;
  const y1 = x2 + (x3 - x2) * uy;
  return y0 + (y1 - y0) * uz;
}

export function lunaDirFromTangent(x: number, z: number): [number, number, number] {
  const r = MOON_GAMEPLAY_RADIUS_M;
  const sx = x / r;
  const sz = z / r;
  return normalize3([sx, 1 - 0.5 * (sx * sx + sz * sz), sz]);
}

export function lunaTangentFromDir(dir: [number, number, number]): [number, number] {
  const d = normalize3(dir);
  const dx = d[0];
  const dy = Math.max(d[1], 0.02);
  const dz = d[2];
  const denom = dx * dx + dz * dz;
  if (denom < 1e-8) return [0, 0];
  const disc = dy * dy + 2 * denom;
  const k = (-dy + Math.sqrt(disc)) / denom;
  const r = MOON_GAMEPLAY_RADIUS_M;
  return [r * k * dx, r * k * dz];
}

export function lunaDisplacementAtDir(dir: [number, number, number]): number {
  const d = normalize3(dir);
  let h = 0;
  const mare = smoothNoise3(d, 2.4) * 1.2 - 0.4;
  h += mare * 0.35;
  h += smoothNoise3(d, 11.0) * 0.45;
  h += smoothNoise3(d, 23.0) * 0.18;
  for (const c of CRATERS) {
    const angle = Math.acos(Math.max(-1, Math.min(1, dot3(d, c.dir))));
    if (angle >= c.angularR * 1.15) continue;
    const t = 1 - Math.min(1, angle / c.angularR);
    const bowl = t * t * (3 - 2 * t);
    h -= c.depth * bowl;
    if (t > 0.72) {
      const rimT = Math.min(1, (t - 0.72) / 0.28);
      h += c.rim * rimT;
    }
  }
  return Math.max(-14, Math.min(6, h));
}

export function lunaDisplacementAt(x: number, z: number): number {
  return lunaDisplacementAtDir(lunaDirFromTangent(x, z));
}

export function lunaGroundYAt(x: number, z: number): number {
  const r = MOON_GAMEPLAY_RADIUS_M;
  const dir = lunaDirFromTangent(x, z);
  const disp = lunaDisplacementAtDir(dir);
  const centerY = -r;
  return centerY + dir[1] * (r + disp);
}

export function lunaSlopeAt(x: number, z: number, sample = 0.75): number {
  const h0 = lunaGroundYAt(x, z);
  const hx = lunaGroundYAt(x + sample, z) - h0;
  const hz = lunaGroundYAt(x, z + sample) - h0;
  return Math.hypot(hx, hz) / Math.max(0.1, sample);
}

export function lunaWrapTangent(v: number): number {
  const period = LUNA_TANGENT_HALF * 2;
  let t = v % period;
  if (t < -LUNA_TANGENT_HALF) t += period;
  if (t > LUNA_TANGENT_HALF) t -= period;
  return Math.max(-LUNA_TANGENT_HALF, Math.min(LUNA_TANGENT_HALF, t));
}

export function lunaClampTangent(body: MovementBody): void {
  body.x = lunaWrapTangent(body.x);
  body.z = lunaWrapTangent(body.z);
}

function blockLunaSteepStep(body: MovementBody, fromX: number, fromZ: number): void {
  const h0 = lunaGroundYAt(fromX, fromZ);
  for (let i = 1; i <= 4; i++) {
    const t = i / 4;
    const sx = fromX + (body.x - fromX) * t;
    const sz = fromZ + (body.z - fromZ) * t;
    const rise = lunaGroundYAt(sx, sz) - h0;
    if (rise > STEP_HEIGHT + 0.05) {
      const slope = lunaSlopeAt(sx, sz);
      if (slope > 0.38 || rise > STEP_HEIGHT * 1.4) {
        body.x = fromX;
        body.z = fromZ;
        body.vx = 0;
        body.vz = 0;
        return;
      }
    }
  }
}

function blockLunaUnwalkableAscent(body: MovementBody, fromX: number, fromZ: number): void {
  const dx = body.x - fromX;
  const dz = body.z - fromZ;
  if (dx * dx + dz * dz < 1e-6) return;
  const sample = 0.75;
  const h0 = lunaGroundYAt(fromX, fromZ);
  const gx = (lunaGroundYAt(fromX + sample, fromZ) - h0) / sample;
  const gz = (lunaGroundYAt(fromX, fromZ + sample) - h0) / sample;
  if (gx * dx + gz * dz <= 0) return;
  if (lunaSlopeAt(fromX, fromZ, sample) > MAX_WALK_SLOPE) {
    body.x = fromX;
    body.z = fromZ;
    body.vx = 0;
    body.vz = 0;
  }
}

function applyLunaGravity(b: MovementBody, dt: number, gravityScale: number): void {
  const ground = lunaGroundYAt(b.x, b.z);
  const slope = lunaSlopeAt(b.x, b.z);
  const snapEps = 0.08 + slope * 0.12;
  const grounded = b.y <= ground + snapEps && b.vy <= 0.05;
  if (grounded) {
    b.coyoteSec = COYOTE_TIME_SECS;
  } else {
    b.coyoteSec = Math.max(0, (b.coyoteSec ?? 0) - dt);
  }
  if (b.y > ground + 0.001 || b.vy > 0.001) {
    b.vy -= GRAVITY * gravityScale * dt;
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

/** Lunar surface locomotion — no island terrain or toroidal wrap. */
export function stepLunaMovement(
  body: MovementBody,
  input: InputState,
  dt: number,
  jumpPressed = false,
  gravityScale = 0.17
): void {
  if (!body.maxStamina) body.maxStamina = STAMINA_MAX;
  if (!body.stamina) body.stamina = STAMINA_MAX;
  if (!body.coyoteSec) body.coyoteSec = 0;

  let mx = input.moveX;
  let mz = input.moveZ;
  const moveLen = Math.hypot(mx, mz);
  if (moveLen > 1) {
    mx /= moveLen;
    mz /= moveLen;
  }
  if (input.aimX !== 0 || input.aimZ !== 0) {
    body.rot = Math.atan2(input.aimX, input.aimZ);
  } else if (moveLen > 0.01) {
    body.rot = Math.atan2(mx, mz);
  }

  const wantsSprint = input.sprint === 1 && moveLen > 0.1 && body.stamina > 0;
  const speedMult = wantsSprint ? SPRINT_MULT : 1;
  const prevX = body.x;
  const prevZ = body.z;

  const targetVx = mx * body.speed * speedMult;
  const targetVz = mz * body.speed * speedMult;
  const moving = Math.abs(mx) + Math.abs(mz) > 0.05;
  const rate = moving ? 20 : FRICTION;
  const k = Math.min(1, rate * dt);
  body.vx += (targetVx - body.vx) * k;
  body.vz += (targetVz - body.vz) * k;
  body.x += body.vx * dt;
  body.z += body.vz * dt;

  blockLunaSteepStep(body, prevX, prevZ);
  blockLunaUnwalkableAscent(body, prevX, prevZ);

  if (wantsSprint) {
    body.stamina = Math.max(0, body.stamina - STAMINA_SPRINT_DRAIN * dt);
  } else {
    body.stamina = Math.min(body.maxStamina, body.stamina + STAMINA_REGEN * dt);
  }

  if (jumpPressed && (body.coyoteSec ?? 0) > 0 && body.stamina >= JUMP_STAMINA_COST) {
    const ground = lunaGroundYAt(body.x, body.z);
    if (body.y <= ground + 0.12) {
      body.vy = JUMP_VELOCITY;
      body.stamina = Math.max(0, body.stamina - JUMP_STAMINA_COST);
      body.coyoteSec = 0;
    }
  }

  lunaClampTangent(body);
  applyLunaGravity(body, dt, gravityScale);

  if (body.vy <= 0.05) {
    const floor = lunaGroundYAt(body.x, body.z);
    if (body.y < floor) {
      body.y = floor;
      body.vy = 0;
    }
  }

  body.state = moveLen > 0.1 ? (wantsSprint ? "sprinting" : "running") : "idle";
}
