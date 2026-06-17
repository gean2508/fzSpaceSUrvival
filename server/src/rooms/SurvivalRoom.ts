import { Room, Client } from "colyseus";
import {
  EntityState,
  GameRoomState,
  GameSimulation,
  InputMessageSchema,
  InteractableState,
  JoinOptionsSchema,
  MESSAGE,
  ObstacleState,
  ProgressDeltaSchema,
  SYNC_MESSAGE,
  TICK_MS,
  getClass,
  type InputState,
} from "@hunger-arena/shared";
import { getStore } from "../db/store.js";
import { verifyToken } from "../auth/jwt.js";
import { slog } from "../logger.js";

interface ClientSession {
  deviceId: string;
  killsAtJoin: number;
  joinedAtMs: number;
  lastSeq: number;
  /** Token bucket for input-flood protection. */
  tokens: number;
  lastRefillMs: number;
}

// Allow a little above the 20Hz tick rate to tolerate jitter, but no flooding.
const INPUT_BUCKET_MAX = 40;
const INPUT_REFILL_PER_SEC = 30;

/**
 * Authoritative survival room. Wraps the shared deterministic GameSimulation.
 *
 * Trust model: the server NEVER accepts positions from clients. It only accepts
 * validated input intents and computes all positions itself. This is the base
 * of the anti-cheat strategy (expanded in Phase 10).
 */
export class SurvivalRoom extends Room<{ state: GameRoomState }> {
  state = new GameRoomState();
  private sim = new GameSimulation({
    seed: (Date.now() & 0xffffffff) >>> 0,
    survival: true,
  });
  private store = getStore();
  private sessions = new Map<string, ClientSession>();

  onCreate() {
    this.publishWorld();
    this.setSimulationInterval(() => this.tick(), TICK_MS);

    this.onMessage(MESSAGE.input, (client: Client, raw: unknown) => {
      const session = this.sessions.get(client.sessionId);
      if (!session) return;

      // 1) Flood protection (token bucket).
      if (!this.consumeInputToken(session)) {
        slog("security", "input flood throttled", {
          sessionId: client.sessionId,
        });
        return;
      }

      // 2) Schema validation + clamping (zod).
      const parsed = InputMessageSchema.safeParse(raw);
      if (!parsed.success) {
        slog("security", "rejected invalid input", {
          sessionId: client.sessionId,
        });
        return;
      }
      const input: InputState = parsed.data;

      // 3) Drop stale/duplicate sequence numbers (replay/reorder defense).
      if (input.seq <= session.lastSeq) return;
      session.lastSeq = input.seq;

      this.sim.queueInput(client.sessionId, input);
    });

    // RTT probe: echo the client's timestamp back untouched.
    this.onMessage(MESSAGE.ping, (client: Client, t: unknown) => {
      if (typeof t === "number") client.send(MESSAGE.pong, t);
    });

    // Merge offline-earned progress on (re)connect.
    this.onMessage(SYNC_MESSAGE.syncProgress, async (client: Client, raw: unknown) => {
      const parsed = ProgressDeltaSchema.safeParse(raw);
      const session = this.sessions.get(client.sessionId);
      if (!parsed.success || !session) {
        slog("security", "rejected invalid progress sync", {
          sessionId: client.sessionId,
        });
        return;
      }
      const profile = await this.store.mergeProgress(session.deviceId, parsed.data);
      client.send(SYNC_MESSAGE.profile, profile);
      slog("db", "merged offline progress", { deviceId: session.deviceId });
    });

    slog("room", "SurvivalRoom created", { roomId: this.roomId });
  }

  /**
   * Guest-first auth: a JWT is optional. If supplied it must be valid; if
   * absent the client joins as a guest (required for LAN/offline play).
   */
  async onAuth(_client: Client, options: { token?: string } | undefined) {
    const token = options?.token;
    if (!token) return { guest: true };
    const userId = await verifyToken(token, "access");
    if (!userId) throw new Error("invalid_token");
    return { userId };
  }

  async onJoin(client: Client, options?: unknown) {
    const parsed = JoinOptionsSchema.safeParse(options ?? {});
    const classId = parsed.success ? parsed.data.classId : getClass("").id;
    const deviceId =
      (parsed.success && parsed.data.deviceId) || client.sessionId;

    this.sim.addPlayer(client.sessionId, classId, 0);
    this.sessions.set(client.sessionId, {
      deviceId,
      killsAtJoin: this.sim.npcKills,
      joinedAtMs: Date.now(),
      lastSeq: 0,
      tokens: INPUT_BUCKET_MAX,
      lastRefillMs: Date.now(),
    });
    this.syncState();

    const profile = await this.store.getOrCreate(deviceId);
    client.send(SYNC_MESSAGE.profile, profile);

    slog("connect", "player joined", {
      sessionId: client.sessionId,
      classId,
      deviceId,
      players: this.sim.playerCount(),
    });
  }

  async onLeave(client: Client) {
    const session = this.sessions.get(client.sessionId);
    this.sim.removeEntity(client.sessionId);
    this.state.entities.delete(client.sessionId);
    this.sessions.delete(client.sessionId);

    if (session) {
      const result = {
        mode: "survival",
        waves: this.sim.wave,
        kills: Math.max(0, this.sim.npcKills - session.killsAtJoin),
        durationMs: Date.now() - session.joinedAtMs,
      };
      try {
        await this.store.applyMatch(session.deviceId, result);
      } catch (e) {
        slog("db", "applyMatch failed", {
          msg: e instanceof Error ? e.message : String(e),
        });
      }
    }

    slog("disconnect", "player left", {
      sessionId: client.sessionId,
      players: this.sim.playerCount(),
    });
  }

  /** Refill + consume one input token. Returns false when the bucket is empty. */
  private consumeInputToken(session: ClientSession): boolean {
    const now = Date.now();
    const elapsed = (now - session.lastRefillMs) / 1000;
    session.tokens = Math.min(
      INPUT_BUCKET_MAX,
      session.tokens + elapsed * INPUT_REFILL_PER_SEC
    );
    session.lastRefillMs = now;
    if (session.tokens < 1) return false;
    session.tokens -= 1;
    return true;
  }

  /** Push the static world geometry into state once at room creation. */
  private publishWorld() {
    for (const o of this.sim.obstacles) {
      const os = new ObstacleState();
      os.id = o.id;
      os.x = o.x;
      os.z = o.z;
      os.halfW = o.halfW;
      os.halfD = o.halfD;
      os.height = o.height;
      this.state.obstacles.push(os);
    }
    for (const n of this.sim.getInteractables()) {
      const ns = new InteractableState();
      ns.id = n.id;
      ns.kind = n.kind;
      ns.x = n.x;
      ns.z = n.z;
      ns.radius = n.radius;
      ns.looted = n.looted;
      this.state.interactables.push(ns);
    }
  }

  private tick() {
    this.sim.step();
    this.syncState();
  }

  /** Mirror the authoritative sim snapshot into the synced Colyseus state. */
  private syncState() {
    const snap = this.sim.snapshot();
    this.state.tick = snap.tick;
    this.state.status = snap.status;
    this.state.wave = snap.wave;

    const live = new Set<string>();
    for (const e of snap.entities) {
      live.add(e.id);
      let s = this.state.entities.get(e.id);
      if (!s) {
        s = new EntityState();
        s.id = e.id;
        this.state.entities.set(e.id, s);
      }
      s.kind = e.kind;
      s.classId = e.classId;
      s.x = e.x;
      s.y = e.y;
      s.z = e.z;
      s.vx = e.vx;
      s.vz = e.vz;
      s.rot = e.rot;
      s.hp = e.hp;
      s.maxHp = e.maxHp;
      s.dead = e.dead;
      s.team = e.team;
      s.ownerId = e.ownerId;
      s.state = e.state;
      s.stamina = e.stamina;
      s.maxStamina = e.maxStamina;
      s.hunger = e.hunger;
      s.thirst = e.thirst;
      s.lastSeq = snap.lastProcessedInput[e.id] ?? 0;
    }

    for (const id of [...this.state.entities.keys()]) {
      if (!live.has(id)) this.state.entities.delete(id);
    }

    // Reflect authoritative loot state (only flips false → true).
    for (let i = 0; i < this.state.interactables.length; i++) {
      const node = snap.interactables[i];
      if (node && this.state.interactables[i].looted !== node.looted) {
        this.state.interactables[i].looted = node.looted;
      }
    }
  }
}
