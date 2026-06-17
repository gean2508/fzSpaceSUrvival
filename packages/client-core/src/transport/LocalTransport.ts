import {
  GameSimulation,
  TICK_MS,
  type InputState,
  type ProgressDelta,
  type WorldSnapshot,
} from "@hunger-arena/shared";
import type {
  ConnectionStatus,
  ConnectOptions,
  ProfileListener,
  SnapshotListener,
  StatusListener,
  Transport,
} from "./Transport";

const LOCAL_ID = "local";

/**
 * Offline transport: runs the shared GameSimulation in-browser at the fixed
 * tick. No prediction needed (zero latency). Same sim the server uses online.
 */
export class LocalTransport implements Transport {
  readonly mode = "offline" as const;
  status: ConnectionStatus = "idle";

  private sim: GameSimulation | null = null;
  private loop: ReturnType<typeof setInterval> | null = null;
  private input: InputState | null = null;
  private snapshotListeners = new Set<SnapshotListener>();
  private statusListeners = new Set<StatusListener>();

  async connect(opts: ConnectOptions): Promise<void> {
    this.setStatus("connecting");
    this.sim = new GameSimulation({
      seed: Date.now() & 0xffffffff,
      survival: true,
    });
    this.sim.addPlayer(LOCAL_ID, opts.classId, 0);

    this.loop = setInterval(() => this.fixedStep(), TICK_MS);
    this.setStatus("connected");
    this.emitSnapshot();
  }

  private fixedStep(): void {
    if (!this.sim) return;
    if (this.input) this.sim.queueInput(LOCAL_ID, this.input);
    this.sim.step();
    this.emitSnapshot();
  }

  private emitSnapshot(): void {
    if (!this.sim) return;
    const snap = this.sim.snapshot();
    for (const cb of this.snapshotListeners) cb(snap);
  }

  sendInput(input: InputState): void {
    this.input = input;
  }

  getSnapshot(): WorldSnapshot | null {
    return this.sim ? this.sim.snapshot() : null;
  }

  getLocalId(): string | null {
    return this.sim ? LOCAL_ID : null;
  }

  onSnapshot(cb: SnapshotListener): () => void {
    this.snapshotListeners.add(cb);
    return () => this.snapshotListeners.delete(cb);
  }

  onStatus(cb: StatusListener): () => void {
    this.statusListeners.add(cb);
    return () => this.statusListeners.delete(cb);
  }

  // Offline play has no server profile; persistence is handled client-side.
  onProfile(_cb: ProfileListener): () => void {
    return () => {};
  }

  sendProgress(_delta: ProgressDelta): void {
    /* no-op offline */
  }

  getRtt(): number {
    return 0;
  }

  private setStatus(status: ConnectionStatus, error?: string): void {
    this.status = status;
    for (const cb of this.statusListeners) cb(status, error);
  }

  disconnect(): void {
    if (this.loop) clearInterval(this.loop);
    this.loop = null;
    this.sim = null;
    this.input = null;
    this.setStatus("closed");
    this.snapshotListeners.clear();
    this.statusListeners.clear();
  }
}
