import { LocalTransport } from "./LocalTransport";
import { ColyseusTransport } from "./ColyseusTransport";
import type { ConnectOptions, GameMode, Transport } from "./Transport";

/**
 * Single entry point the UI uses to start a match in any mode. The rest of the
 * app talks only to the returned Transport, never to mode-specific code.
 */
export class SessionManager {
  private transport: Transport | null = null;

  get current(): Transport | null {
    return this.transport;
  }

  async start(mode: GameMode, opts: ConnectOptions): Promise<Transport> {
    this.stop();
    const transport: Transport =
      mode === "offline"
        ? new LocalTransport()
        : new ColyseusTransport(mode);
    this.transport = transport;
    await transport.connect(opts);
    return transport;
  }

  stop(): void {
    this.transport?.disconnect();
    this.transport = null;
  }
}
