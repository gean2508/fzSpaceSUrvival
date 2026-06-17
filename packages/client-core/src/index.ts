/**
 * Hunger Arena — Client Engine (runtime layer).
 *
 * Sits between platform apps (mobile/desktop) and packages/shared (sim truth).
 * Apps MUST import netcode + prediction from here — never fork ArenaController.
 */

export { ArenaController, type RawInput, type RenderEntity, type SyncStats } from "./engine/ArenaController.js";

export {
  type Transport,
  type GameMode,
  type ConnectionStatus,
  type ConnectOptions,
  type SnapshotListener,
  type StatusListener,
  type ProfileListener,
} from "./transport/Transport.js";
export { LocalTransport } from "./transport/LocalTransport.js";
export { ColyseusTransport } from "./transport/ColyseusTransport.js";
export { SessionManager } from "./transport/SessionManager.js";

export {
  createInputStore,
  type InputPayload,
  type InputStore,
  type Platform,
} from "./input/createInputStore.js";
