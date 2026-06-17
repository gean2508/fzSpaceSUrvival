import { Client, type Room } from "@colyseus/sdk";
import {
  GameRoomState,
  MESSAGE,
  ROOM_NAME,
  SYNC_MESSAGE,
  type EntityKind,
  type EntitySnapshot,
  type InputState,
  type InteractableKind,
  type InteractableSnapshot,
  type MatchStatus,
  type MovementState,
  type Obstacle,
  type ProfileData,
  type ProgressDelta,
  type WorldSnapshot,
} from "@hunger-arena/shared";
import type {
  ConnectionStatus,
  ConnectOptions,
  GameMode,
  ProfileListener,
  SnapshotListener,
  StatusListener,
  Transport,
} from "./Transport";

/**
 * Networked transport (LAN/online). Connects to an authoritative Colyseus
 * server and maps room state into the shared WorldSnapshot shape so rendering
 * and prediction code are identical to offline play.
 */
export class ColyseusTransport implements Transport {
  readonly mode: GameMode;
  status: ConnectionStatus = "idle";

  private client: Client | null = null;
  private room: Room<GameRoomState> | null = null;
  private snapshotListeners = new Set<SnapshotListener>();
  private statusListeners = new Set<StatusListener>();
  private profileListeners = new Set<ProfileListener>();
  private rtt = 0;
  private pingTimer: ReturnType<typeof setInterval> | null = null;

  constructor(mode: Exclude<GameMode, "offline"> = "online") {
    this.mode = mode;
  }

  async connect(opts: ConnectOptions): Promise<void> {
    if (!opts.serverUrl) throw new Error("serverUrl required for networked play");
    this.setStatus("connecting");
    try {
      this.client = new Client(opts.serverUrl);
      this.room = await this.client.joinOrCreate<GameRoomState>(ROOM_NAME, {
        classId: opts.classId,
        deviceId: opts.deviceId,
        token: opts.token,
      });

      this.room.onStateChange(() => this.emitSnapshot());
      this.room.onLeave(() => this.setStatus("closed"));
      this.room.onError((_code, message) => this.setStatus("error", message));
      this.room.onMessage(SYNC_MESSAGE.profile, (profile: ProfileData) => {
        for (const cb of this.profileListeners) cb(profile);
      });
      this.room.onMessage(MESSAGE.pong, (sentAt: number) => {
        this.rtt = Math.max(0, Math.round(performance.now() - sentAt));
      });
      this.startPing();

      this.setStatus("connected");
      this.emitSnapshot();
    } catch (e) {
      this.setStatus("error", e instanceof Error ? e.message : String(e));
      throw e;
    }
  }

  private buildSnapshot(): WorldSnapshot | null {
    const room = this.room;
    if (!room?.state?.entities) return null;

    const entities: EntitySnapshot[] = [];
    const lastProcessedInput: Record<string, number> = {};
    room.state.entities.forEach((e, id) => {
      entities.push({
        id,
        kind: e.kind as EntityKind,
        classId: e.classId,
        x: e.x,
        y: e.y,
        z: e.z,
        vx: e.vx,
        vz: e.vz,
        rot: e.rot,
        hp: e.hp,
        maxHp: e.maxHp,
        dead: e.dead,
        team: e.team,
        ownerId: e.ownerId,
        state: e.state as MovementState,
        stamina: e.stamina,
        maxStamina: e.maxStamina,
        hunger: e.hunger,
        thirst: e.thirst,
      });
      if (e.kind === "player") lastProcessedInput[id] = e.lastSeq;
    });

    const obstacles: Obstacle[] = [];
    room.state.obstacles.forEach((o) =>
      obstacles.push({
        id: o.id,
        x: o.x,
        z: o.z,
        halfW: o.halfW,
        halfD: o.halfD,
        height: o.height,
      })
    );
    const interactables: InteractableSnapshot[] = [];
    room.state.interactables.forEach((n) =>
      interactables.push({
        id: n.id,
        kind: n.kind as InteractableKind,
        x: n.x,
        z: n.z,
        radius: n.radius,
        looted: n.looted,
      })
    );

    return {
      tick: room.state.tick,
      timeMs: 0,
      status: room.state.status as MatchStatus,
      wave: room.state.wave,
      entities,
      obstacles,
      interactables,
      lastProcessedInput,
    };
  }

  private emitSnapshot(): void {
    const snap = this.buildSnapshot();
    if (!snap) return;
    for (const cb of this.snapshotListeners) cb(snap);
  }

  private startPing(): void {
    this.stopPing();
    this.pingTimer = setInterval(() => {
      if (this.room?.connection?.isOpen) {
        this.room.send(MESSAGE.ping, performance.now());
      }
    }, 2000);
  }

  private stopPing(): void {
    if (this.pingTimer) clearInterval(this.pingTimer);
    this.pingTimer = null;
  }

  getRtt(): number {
    return this.rtt;
  }

  sendInput(input: InputState): void {
    if (this.room?.connection?.isOpen) this.room.send(MESSAGE.input, input);
  }

  getSnapshot(): WorldSnapshot | null {
    return this.buildSnapshot();
  }

  getLocalId(): string | null {
    return this.room?.sessionId ?? null;
  }

  onSnapshot(cb: SnapshotListener): () => void {
    this.snapshotListeners.add(cb);
    return () => this.snapshotListeners.delete(cb);
  }

  onStatus(cb: StatusListener): () => void {
    this.statusListeners.add(cb);
    return () => this.statusListeners.delete(cb);
  }

  onProfile(cb: ProfileListener): () => void {
    this.profileListeners.add(cb);
    return () => this.profileListeners.delete(cb);
  }

  sendProgress(delta: ProgressDelta): void {
    if (this.room?.connection?.isOpen) {
      this.room.send(SYNC_MESSAGE.syncProgress, delta);
    }
  }

  private setStatus(status: ConnectionStatus, error?: string): void {
    this.status = status;
    for (const cb of this.statusListeners) cb(status, error);
  }

  disconnect(): void {
    this.stopPing();
    this.room?.leave();
    this.room = null;
    this.client = null;
    this.setStatus("closed");
    this.snapshotListeners.clear();
    this.statusListeners.clear();
    this.profileListeners.clear();
  }
}
