import { useEffect, useState } from "react";
import { ScrollView, StyleSheet, Text, View } from "react-native";
import type { ArenaController, SyncStats } from "@hunger-arena/client-core";

function ts() {
  return new Date().toISOString().slice(11, 23);
}

const clientLogs: string[] = [];
const MAX_LOGS = 120;

export function clog(tag: string, message: string, data?: unknown) {
  const extra = data !== undefined ? ` ${JSON.stringify(data)}` : "";
  const line = `${ts()} [${tag}] ${message}${extra}`;
  clientLogs.push(line);
  if (clientLogs.length > MAX_LOGS) clientLogs.shift();
  console.log(`[mobile] ${line}`);
}

interface DebugOverlayProps {
  controller: ArenaController | null;
  visible: boolean;
  serverUrl: string;
  mode: string;
  status: string;
}

const EMPTY_STATS: SyncStats = {
  mode: "—",
  localId: null,
  bufferedSnapshots: 0,
  pendingInputs: 0,
  reconcileErrorM: 0,
  authoritative: null,
  predicted: null,
  lastAckSeq: 0,
  tick: 0,
  entityCount: 0,
  rttMs: 0,
};

export function DebugOverlay({
  controller,
  visible,
  serverUrl,
  mode,
  status,
}: DebugOverlayProps) {
  const [logs, setLogs] = useState<string[]>([]);
  const [stats, setStats] = useState<SyncStats>(EMPTY_STATS);
  const [fps, setFps] = useState(0);

  useEffect(() => {
    if (!visible) return;
    let frames = 0;
    let last = performance.now();
    let raf = 0;
    const loop = () => {
      frames++;
      const now = performance.now();
      if (now - last >= 500) {
        setFps(Math.round((frames * 1000) / (now - last)));
        frames = 0;
        last = now;
      }
      raf = requestAnimationFrame(loop);
    };
    raf = requestAnimationFrame(loop);

    const id = setInterval(() => {
      setLogs([...clientLogs]);
      if (controller) setStats(controller.getStats());
    }, 250);

    return () => {
      cancelAnimationFrame(raf);
      clearInterval(id);
    };
  }, [visible, controller]);

  if (!visible) return null;

  return (
    <View style={styles.panel} pointerEvents="none">
      <Text style={styles.title}>DEBUG · {mode} · {status}</Text>
      <Text style={styles.line}>FPS {fps} · RTT {stats.rttMs}ms · tick {stats.tick}</Text>
      <Text style={styles.line}>
        ents {stats.entityCount} · buf {stats.bufferedSnapshots} · pending {stats.pendingInputs}
      </Text>
      <Text style={styles.line}>
        reconcile {stats.reconcileErrorM.toFixed(2)}m · ack {stats.lastAckSeq}
      </Text>
      <Text style={styles.line}>{serverUrl}</Text>
      <ScrollView style={styles.logs}>
        {logs.slice(-8).map((l, i) => (
          <Text key={i} style={styles.logLine}>
            {l}
          </Text>
        ))}
      </ScrollView>
    </View>
  );
}

const styles = StyleSheet.create({
  panel: {
    position: "absolute",
    left: 8,
    bottom: 8,
    maxWidth: "55%",
    maxHeight: 180,
    backgroundColor: "rgba(0,0,0,0.72)",
    borderRadius: 8,
    padding: 8,
    zIndex: 50,
  },
  title: { color: "#7dd3fc", fontFamily: "monospace", fontSize: 10, fontWeight: "700" },
  line: { color: "#e8f4ff", fontFamily: "monospace", fontSize: 9, marginTop: 2 },
  logs: { marginTop: 4, maxHeight: 80 },
  logLine: { color: "rgba(232,244,255,0.75)", fontFamily: "monospace", fontSize: 8 },
});
