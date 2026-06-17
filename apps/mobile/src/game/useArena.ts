import { useEffect, useRef, useState } from "react";
import {
  SessionManager,
  ArenaController,
  type RawInput,
  type ConnectionStatus,
  type GameMode,
  type Transport,
} from "@hunger-arena/client-core";
import { ensureDeviceId } from "../net/identity";
import { getAccessToken } from "../net/auth";
import { SERVER_URL } from "../config";
import { clog } from "../debug/DebugOverlay";

export interface ArenaOptions {
  mode: GameMode;
  classId: string;
  getRawInput: () => RawInput;
}

export function useArena({ mode, classId, getRawInput }: ArenaOptions) {
  const [controller, setController] = useState<ArenaController | null>(null);
  const [transport, setTransport] = useState<Transport | null>(null);
  const [status, setStatus] = useState<ConnectionStatus>("idle");
  const [error, setError] = useState<string | null>(null);
  const inputRef = useRef(getRawInput);
  inputRef.current = getRawInput;

  useEffect(() => {
    const session = new SessionManager();
    let ctrl: ArenaController | null = null;
    let cancelled = false;
    setStatus("connecting");
    setError(null);

    (async () => {
      try {
        const deviceId = await ensureDeviceId();
        const token = await getAccessToken();
        clog("arena", "connecting", { mode });
        const transport = await session.start(mode, {
          classId,
          deviceId,
          serverUrl: SERVER_URL,
          token,
        });
        if (cancelled) {
          session.stop();
          return;
        }
        transport.onStatus((s, err) => {
          setStatus(s);
          if (err) setError(err);
        });
        ctrl = new ArenaController(transport);
        ctrl.start(() => inputRef.current());
        setController(ctrl);
        setTransport(transport);
        setStatus(transport.status);
        clog("arena", "started", { mode });
      } catch (e) {
        const msg = e instanceof Error ? e.message : String(e);
        clog("error", "arena start failed", { msg });
        setError(msg);
        setStatus("error");
      }
    })();

    return () => {
      cancelled = true;
      ctrl?.stop();
      session.stop();
      setController(null);
      setTransport(null);
    };
  }, [mode, classId]);

  return { controller, transport, status, error };
}
