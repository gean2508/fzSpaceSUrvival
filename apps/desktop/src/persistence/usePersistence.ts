import { useEffect, useRef, useState } from "react";
import type { ProfileData } from "@hunger-arena/shared";
import type { ArenaController } from "@hunger-arena/client-core";
import type { GameMode, Transport } from "@hunger-arena/client-core";
import { clog } from "../debug/DebugOverlay";
import {
  clearUnsynced,
  getUnsynced,
  hasUnsynced,
  loadLocal,
  recordOfflineMatch,
  saveServerProfile,
} from "./localProfile";

interface Args {
  transport: Transport | null;
  controller: ArenaController | null;
  mode: GameMode;
  deviceId: string;
}

/**
 * Bridges the persistence layer: keeps a local IndexedDB mirror, uploads
 * offline progress to merge when online, and records offline match results.
 */
export function usePersistence({ transport, controller, mode, deviceId }: Args) {
  const [profile, setProfile] = useState<ProfileData | null>(null);
  const recordedEnd = useRef(false);

  // Load local mirror immediately so the UI has data offline.
  useEffect(() => {
    let active = true;
    loadLocal(deviceId).then((s) => {
      if (active) setProfile(s.profile);
    });
    return () => {
      active = false;
    };
  }, [deviceId]);

  // Online: receive authoritative profile + upload offline deltas.
  useEffect(() => {
    if (!transport || mode === "offline") return;
    const off = transport.onProfile((p) => {
      setProfile(p);
      void saveServerProfile(p);
    });

    (async () => {
      const delta = await getUnsynced(deviceId);
      if (hasUnsynced(delta)) {
        transport.sendProgress(delta);
        await clearUnsynced(deviceId);
        clog("sync", "uploaded offline progress", delta);
      }
    })();

    return off;
  }, [transport, mode, deviceId]);

  // Offline: record the match when it ends.
  useEffect(() => {
    if (!controller || mode !== "offline") return;
    recordedEnd.current = false;
    const id = setInterval(() => {
      const stats = controller.getStats();
      const status = controller.getLocalStatus();
      const ended = status?.dead || statusEnded(controller);
      if (ended && !recordedEnd.current) {
        recordedEnd.current = true;
        void recordOfflineMatch(deviceId, {
          mode: "offline",
          waves: stats.tick > 0 ? currentWave(controller) : 0,
          kills: 0,
          durationMs: 0,
        }).then((p) => {
          setProfile(p);
          clog("sync", "recorded offline match", { waves: p.bestWave });
        });
      }
    }, 500);
    return () => clearInterval(id);
  }, [controller, mode, deviceId]);

  return { profile };
}

function statusEnded(controller: ArenaController): boolean {
  return controller.getWorldStatus() === "ended";
}

function currentWave(controller: ArenaController): number {
  return controller.getWave();
}
