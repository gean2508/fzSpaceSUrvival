import {
  TICK_MS,
  STAMINA_MAX,
  getClass,
  stepMovement,
  type EntitySnapshot,
  type InputState,
  type InteractableSnapshot,
  type MovementBody,
  type MovementState,
  type Obstacle,
  type WorldSnapshot,
} from "@hunger-arena/shared";
import type { Transport } from "../net/Transport";

/** Render the world this far in the past so remote interpolation has data. */
const INTERP_DELAY_MS = 100;
const SNAPSHOT_BUFFER_MS = 1000;
const DT = TICK_MS / 1000;

/** What renderers actually draw. Decoupled from network/sim internals. */
export interface RenderEntity {
  id: string;
  kind: string;
  classId: string;
  x: number;
  y: number;
  z: number;
  rot: number;
  hp: number;
  maxHp: number;
  dead: boolean;
  team: number;
  state: MovementState;
  isLocal: boolean;
}

export interface SyncStats {
  mode: string;
  localId: string | null;
  bufferedSnapshots: number;
  pendingInputs: number;
  reconcileErrorM: number;
  authoritative: { x: number; z: number } | null;
  predicted: { x: number; z: number } | null;
  lastAckSeq: number;
  tick: number;
  entityCount: number;
  rttMs: number;
}

interface Stamped {
  t: number;
  snap: WorldSnapshot;
}

export type RawInput = Omit<InputState, "seq">;

/**
 * Client game brain. Owns:
 *  - local prediction + server reconciliation (responsive, cheat-resistant),
 *  - remote entity interpolation (smooth motion from 20Hz snapshots).
 * Identical for offline and networked play; reconciliation is a no-op offline.
 */
export class ArenaController {
  private snapshots: Stamped[] = [];
  private seq = 0;
  private pending: { seq: number; input: InputState }[] = [];

  private predicted: MovementBody | null = null;
  private prevLocal = { x: 0, y: 0, z: 0, rot: 0 };
  private curLocal = { x: 0, y: 0, z: 0, rot: 0 };
  private lastStepTime = 0;
  private obstacles: Obstacle[] = [];

  private inputProvider: () => RawInput = () => ({
    moveX: 0,
    moveZ: 0,
    aimX: 0,
    aimZ: 1,
    abilities: 0,
    sprint: 0,
    action: 0,
  });

  private tickTimer: ReturnType<typeof setInterval> | null = null;
  private unsub: (() => void) | null = null;

  private lastAckSeq = 0;
  private reconcileErrorM = 0;

  constructor(private transport: Transport) {}

  start(inputProvider: () => RawInput): void {
    this.inputProvider = inputProvider;
    this.unsub = this.transport.onSnapshot((s) => this.onSnapshot(s));
    const existing = this.transport.getSnapshot();
    if (existing) this.onSnapshot(existing);
    this.tickTimer = setInterval(() => this.tickStep(), TICK_MS);
  }

  stop(): void {
    if (this.tickTimer) clearInterval(this.tickTimer);
    this.tickTimer = null;
    this.unsub?.();
    this.unsub = null;
    this.snapshots = [];
    this.pending = [];
    this.predicted = null;
  }

  // ---- input + prediction ----------------------------------------------

  private tickStep(): void {
    const raw = this.inputProvider();
    this.seq++;
    const input: InputState = { seq: this.seq, ...raw };
    this.transport.sendInput(input);

    if (this.predicted) {
      this.pending.push({ seq: this.seq, input });
      this.prevLocal = { ...this.curLocal };
      stepMovement(this.predicted, input, this.obstacles, DT);
      this.curLocal = {
        x: this.predicted.x,
        y: this.predicted.y,
        z: this.predicted.z,
        rot: this.predicted.rot,
      };
      this.lastStepTime = performance.now();
    }
  }

  // ---- snapshots + reconciliation --------------------------------------

  private onSnapshot(snap: WorldSnapshot): void {
    const now = performance.now();
    if (snap.obstacles && snap.obstacles.length) this.obstacles = snap.obstacles;
    this.snapshots.push({ t: now, snap });
    const cutoff = now - SNAPSHOT_BUFFER_MS;
    while (this.snapshots.length > 2 && this.snapshots[0].t < cutoff) {
      this.snapshots.shift();
    }
    this.reconcileLocal(snap);
  }

  private reconcileLocal(snap: WorldSnapshot): void {
    const localId = this.transport.getLocalId();
    if (!localId) return;
    const ent = snap.entities.find((e) => e.id === localId);
    if (!ent) return;

    const def = getClass(ent.classId);
    if (!this.predicted) {
      this.predicted = {
        x: ent.x,
        y: ent.y,
        z: ent.z,
        vx: ent.vx,
        vz: ent.vz,
        vy: 0,
        rot: ent.rot,
        speed: def.moveSpeed,
        radius: def.radius,
        stamina: ent.stamina ?? STAMINA_MAX,
        maxStamina: ent.maxStamina ?? STAMINA_MAX,
        state: ent.state ?? "idle",
      };
      this.curLocal = { x: ent.x, y: ent.y, z: ent.z, rot: ent.rot };
      this.prevLocal = { ...this.curLocal };
      this.lastStepTime = performance.now();
      return;
    }

    // Measure pre-correction error for the sync inspector.
    this.reconcileErrorM = Math.hypot(
      this.predicted.x - ent.x,
      this.predicted.z - ent.z
    );

    const ackSeq = snap.lastProcessedInput[localId] ?? 0;
    this.lastAckSeq = ackSeq;

    // Snap to authoritative, then replay inputs the server hasn't seen yet.
    this.predicted.x = ent.x;
    this.predicted.y = ent.y;
    this.predicted.z = ent.z;
    this.predicted.vx = ent.vx;
    this.predicted.vz = ent.vz;
    this.predicted.rot = ent.rot;
    this.predicted.speed = def.moveSpeed;
    this.predicted.radius = def.radius;
    this.predicted.stamina = ent.stamina ?? this.predicted.stamina;
    this.predicted.maxStamina = ent.maxStamina ?? this.predicted.maxStamina;
    this.predicted.state = ent.state ?? this.predicted.state;

    this.pending = this.pending.filter((p) => p.seq > ackSeq);
    for (const p of this.pending) stepMovement(this.predicted, p.input, this.obstacles, DT);

    this.curLocal = {
      x: this.predicted.x,
      y: this.predicted.y,
      z: this.predicted.z,
      rot: this.predicted.rot,
    };
  }

  // ---- render output ----------------------------------------------------

  getRenderEntities(now: number): Map<string, RenderEntity> {
    const out = new Map<string, RenderEntity>();
    const localId = this.transport.getLocalId();

    const interp = this.interpolatedRemote(now);

    for (const e of interp) {
      out.set(e.id, {
        id: e.id,
        kind: e.kind,
        classId: e.classId,
        x: e.x,
        y: e.y,
        z: e.z,
        rot: e.rot,
        hp: e.hp,
        maxHp: e.maxHp,
        dead: e.dead,
        team: e.team,
        state: e.state ?? "idle",
        isLocal: e.id === localId,
      });
    }

    // Local entity: prediction (responsive), render-interpolated for 60fps.
    if (localId && this.predicted) {
      const alpha = Math.min(1, (now - this.lastStepTime) / TICK_MS);
      const x = lerp(this.prevLocal.x, this.curLocal.x, alpha);
      const y = lerp(this.prevLocal.y, this.curLocal.y, alpha);
      const z = lerp(this.prevLocal.z, this.curLocal.z, alpha);
      const base = out.get(localId);
      out.set(localId, {
        id: localId,
        kind: "player",
        classId: base?.classId ?? "scout",
        x,
        y,
        z,
        rot: this.curLocal.rot,
        hp: base?.hp ?? 100,
        maxHp: base?.maxHp ?? 100,
        dead: base?.dead ?? false,
        team: base?.team ?? 0,
        state: this.predicted.state,
        isLocal: true,
      });
    }

    return out;
  }

  /** Interpolate all entities from the snapshot buffer at (now - delay). */
  private interpolatedRemote(now: number): EntitySnapshot[] {
    if (this.snapshots.length === 0) return [];
    const renderTime = now - INTERP_DELAY_MS;

    let older: Stamped | null = null;
    let newer: Stamped | null = null;
    for (let i = this.snapshots.length - 1; i >= 0; i--) {
      if (this.snapshots[i].t <= renderTime) {
        older = this.snapshots[i];
        newer = this.snapshots[i + 1] ?? this.snapshots[i];
        break;
      }
    }
    if (!older) {
      older = this.snapshots[0];
      newer = this.snapshots[Math.min(1, this.snapshots.length - 1)];
    }

    const span = newer!.t - older!.t;
    const alpha = span > 0 ? clamp((renderTime - older!.t) / span, 0, 1) : 0;

    const newerById = new Map(newer!.snap.entities.map((e) => [e.id, e]));
    return older!.snap.entities.map((a) => {
      const b = newerById.get(a.id) ?? a;
      return {
        ...a,
        x: lerp(a.x, b.x, alpha),
        y: lerp(a.y, b.y, alpha),
        z: lerp(a.z, b.z, alpha),
        rot: lerpAngle(a.rot, b.rot, alpha),
      };
    });
  }

  /** Latest authoritative status of the local player (for HUD: hp, class). */
  getLocalStatus(): {
    hp: number;
    maxHp: number;
    classId: string;
    dead: boolean;
  } | null {
    const localId = this.transport.getLocalId();
    const latest = this.snapshots[this.snapshots.length - 1]?.snap;
    if (!localId || !latest) return null;
    const ent = latest.entities.find((e) => e.id === localId);
    return ent
      ? { hp: ent.hp, maxHp: ent.maxHp, classId: ent.classId, dead: ent.dead }
      : null;
  }

  /** Predicted local position (for pointer-aim direction + camera follow). */
  getLocalPosition(): { x: number; y: number; z: number } {
    return { x: this.curLocal.x, y: this.curLocal.y, z: this.curLocal.z };
  }

  getObstacles(): Obstacle[] {
    return this.obstacles;
  }

  getInteractables(): InteractableSnapshot[] {
    return this.snapshots[this.snapshots.length - 1]?.snap.interactables ?? [];
  }

  /** Local player's survival readout for the HUD. */
  getLocalSurvival(): {
    stamina: number;
    maxStamina: number;
    hunger: number;
    thirst: number;
    state: MovementState;
  } | null {
    const localId = this.transport.getLocalId();
    const latest = this.snapshots[this.snapshots.length - 1]?.snap;
    if (!localId || !latest) return null;
    const ent = latest.entities.find((e) => e.id === localId);
    if (!ent) return null;
    return {
      stamina: this.predicted?.stamina ?? ent.stamina,
      maxStamina: ent.maxStamina,
      hunger: ent.hunger,
      thirst: ent.thirst,
      state: this.predicted?.state ?? ent.state,
    };
  }

  getWorldStatus(): string {
    return this.snapshots[this.snapshots.length - 1]?.snap.status ?? "playing";
  }

  getWave(): number {
    return this.snapshots[this.snapshots.length - 1]?.snap.wave ?? 0;
  }

  /**
   * 0 = tight top-down (close combat), 1 = open horizon (sprint / exploration).
   * Drives orthographic camera pitch tilt in GameCamera.
   */
  getHorizonBlend(): number {
    const surv = this.getLocalSurvival();
    const state = surv?.state;
    if (state === "sprinting") return 1;

    const lp = this.getLocalPosition();
    const latest = this.snapshots[this.snapshots.length - 1]?.snap;
    if (latest) {
      for (const e of latest.entities) {
        if (e.kind === "npc" && !e.dead) {
          const d = Math.hypot(e.x - lp.x, e.z - lp.z);
          if (d < 7) return 0.05;
        }
      }
    }

    if (state === "running" || state === "wall_trek") return 0.65;
    if (state === "idle") return 0.2;
    return 0.1;
  }

  getStats(): SyncStats {
    const localId = this.transport.getLocalId();
    const latest = this.snapshots[this.snapshots.length - 1]?.snap;
    const auth =
      latest && localId
        ? latest.entities.find((e) => e.id === localId) ?? null
        : null;
    return {
      mode: this.transport.mode,
      localId,
      bufferedSnapshots: this.snapshots.length,
      pendingInputs: this.pending.length,
      reconcileErrorM: this.reconcileErrorM,
      authoritative: auth ? { x: auth.x, z: auth.z } : null,
      predicted: this.predicted
        ? { x: this.predicted.x, z: this.predicted.z }
        : null,
      lastAckSeq: this.lastAckSeq,
      tick: latest?.tick ?? 0,
      entityCount: latest?.entities.length ?? 0,
      rttMs: this.transport.getRtt(),
    };
  }
}

function lerp(a: number, b: number, t: number): number {
  return a + (b - a) * t;
}

function clamp(v: number, lo: number, hi: number): number {
  return Math.max(lo, Math.min(hi, v));
}

function lerpAngle(a: number, b: number, t: number): number {
  let d = b - a;
  while (d > Math.PI) d -= Math.PI * 2;
  while (d < -Math.PI) d += Math.PI * 2;
  return a + d * t;
}
