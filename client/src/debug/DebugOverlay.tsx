import { useEffect, useState } from "react";
import type { ArenaController, SyncStats } from "../game/ArenaController";

function ts() {
  return new Date().toISOString().slice(11, 23);
}

/** In-memory client log — rule 9 observability. */
const clientLogs: string[] = [];
const MAX_LOGS = 120;

export function clog(tag: string, message: string, data?: unknown) {
  const extra = data !== undefined ? ` ${JSON.stringify(data)}` : "";
  const line = `${ts()} [${tag}] ${message}${extra}`;
  clientLogs.push(line);
  if (clientLogs.length > MAX_LOGS) clientLogs.shift();
  console.log(`[client] ${line}`);
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

  const num = (n: number) => (Math.round(n * 1000) / 1000).toString();
  const pos = (p: { x: number; z: number } | null) =>
    p ? `${num(p.x)}, ${num(p.z)}` : "—";

  return (
    <div
      style={{
        position: "fixed",
        top: 8,
        right: 8,
        width: 340,
        maxHeight: "70vh",
        overflow: "auto",
        background: "rgba(0,0,0,0.88)",
        border: "1px solid rgba(0,229,255,0.35)",
        borderRadius: 6,
        color: "#a8d4ff",
        fontFamily: "monospace",
        fontSize: 10,
        padding: 8,
        zIndex: 100,
        pointerEvents: "auto",
      }}
    >
      <div style={{ color: "#00e5ff", fontWeight: "bold", marginBottom: 6 }}>
        DEBUG · sync inspector (` para ocultar)
      </div>
      <div>Mode: {mode}</div>
      <div>Server: {serverUrl}</div>
      <div>Status: {status}</div>
      <div>FPS: {fps}</div>
      <div style={{ color: stats.rttMs > 150 ? "#ffb86b" : "#7CFC9B" }}>
        RTT: {mode === "offline" ? "n/a (local)" : `${stats.rttMs} ms`}
      </div>
      <div>Tick: {stats.tick}</div>
      <div>Entities: {stats.entityCount}</div>
      <div>Local id: {stats.localId ?? "—"}</div>
      <div>Snapshots buffered: {stats.bufferedSnapshots}</div>
      <div>Pending inputs: {stats.pendingInputs}</div>
      <div>Last ack seq: {stats.lastAckSeq}</div>
      <div>Predicted: {pos(stats.predicted)}</div>
      <div>Authoritative: {pos(stats.authoritative)}</div>
      <div
        style={{
          color: stats.reconcileErrorM > 0.5 ? "#ff6b6b" : "#7CFC9B",
        }}
      >
        Reconcile error: {num(stats.reconcileErrorM)} m
      </div>
      <hr style={{ borderColor: "#333", margin: "6px 0" }} />
      {logs.slice(-14).map((l, i) => (
        <div key={i}>{l}</div>
      ))}
    </div>
  );
}

export function useDebugToggle() {
  const [visible, setVisible] = useState(
    () =>
      typeof window !== "undefined" &&
      (new URLSearchParams(window.location.search).has("debug") ||
        localStorage.getItem("arena-debug") === "1")
  );

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "`") {
        setVisible((v) => {
          const next = !v;
          localStorage.setItem("arena-debug", next ? "1" : "0");
          return next;
        });
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, []);

  return visible;
}
