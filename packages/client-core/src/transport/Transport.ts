import type {
  InputState,
  ProfileData,
  ProgressDelta,
  WorldSnapshot,
} from "@hunger-arena/shared";

export type ProfileListener = (profile: ProfileData) => void;

export type GameMode = "offline" | "lan" | "online";

export type ConnectionStatus =
  | "idle"
  | "connecting"
  | "connected"
  | "error"
  | "closed";

export interface ConnectOptions {
  classId: string;
  deviceId: string;
  /** ws:// or wss:// url for lan/online. Ignored offline. */
  serverUrl?: string;
  /** JWT for online accounts. Optional (guests allowed on lan/offline). */
  token?: string;
}

export type SnapshotListener = (snap: WorldSnapshot) => void;
export type StatusListener = (status: ConnectionStatus, error?: string) => void;

/**
 * Unifies offline (LocalTransport) and networked (ColyseusTransport) play so
 * the rest of the client never branches on game mode.
 */
export interface Transport {
  readonly mode: GameMode;
  readonly status: ConnectionStatus;

  connect(opts: ConnectOptions): Promise<void>;
  /** Latest intent for this frame. Sent to server or fed to local sim. */
  sendInput(input: InputState): void;
  getSnapshot(): WorldSnapshot | null;
  /** Local player's entity id, once connected. */
  getLocalId(): string | null;

  onSnapshot(cb: SnapshotListener): () => void;
  onStatus(cb: StatusListener): () => void;

  /** Networked transports deliver the server's authoritative profile. */
  onProfile(cb: ProfileListener): () => void;
  /** Upload offline-earned progress to be merged server-side. */
  sendProgress(delta: ProgressDelta): void;

  /** Round-trip time to the server in ms. 0 for offline / not-yet-measured. */
  getRtt(): number;

  disconnect(): void;
}
