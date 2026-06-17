import { useEffect, useState } from "react";

/**
 * The game is landscape-only. On touch devices in portrait we show a rotate
 * prompt and (best-effort) request a landscape lock. Desktop is unaffected.
 */
export function OrientationGuard() {
  const [portrait, setPortrait] = useState(false);

  useEffect(() => {
    const mq = window.matchMedia("(orientation: portrait)");
    const update = () => setPortrait(mq.matches);
    update();
    mq.addEventListener("change", update);

    // Best-effort lock (supported on some Android browsers / installed PWAs).
    const lock = (screen.orientation as ScreenOrientation & {
      lock?: (o: string) => Promise<void>;
    })?.lock;
    lock?.call(screen.orientation, "landscape").catch(() => {});

    return () => mq.removeEventListener("change", update);
  }, []);

  if (!portrait) return null;
  return (
    <div
      style={{
        position: "fixed",
        inset: 0,
        zIndex: 9999,
        display: "flex",
        flexDirection: "column",
        alignItems: "center",
        justifyContent: "center",
        gap: 16,
        background: "#2b2f3a",
        color: "#fff7e6",
        fontFamily: "system-ui, sans-serif",
        textAlign: "center",
        padding: 24,
      }}
    >
      <div style={{ fontSize: 48 }}>📱↻</div>
      <div style={{ fontSize: 18, fontWeight: 800 }}>Gira tu dispositivo</div>
      <div style={{ fontSize: 13, opacity: 0.8 }}>
        Hunger Arena se juega en horizontal (landscape).
      </div>
    </div>
  );
}
