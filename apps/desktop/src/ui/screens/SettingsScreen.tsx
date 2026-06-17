import { useState } from "react";
import { MenuLayout } from "./MenuLayout";
import { useGameStore } from "../../store/useGameStore";
import { SERVER_URL } from "../../config";

export function SettingsScreen() {
  const go = useGameStore((s) => s.go);
  const [debug, setDebug] = useState(
    () => localStorage.getItem("arena-debug") === "1"
  );

  const toggleDebug = () => {
    const next = !debug;
    localStorage.setItem("arena-debug", next ? "1" : "0");
    setDebug(next);
  };

  return (
    <MenuLayout title="Ajustes" onBack={() => go("menu")}>
      <div className="float-in flex w-full max-w-sm flex-col gap-3 rounded-2xl bg-[var(--color-cream)]/90 p-6 text-[var(--color-bark)] shadow-xl">
        <Row label="Servidor">
          <span className="font-mono text-xs">{SERVER_URL}</span>
        </Row>
        <Row label="Overlay de debug (tecla `)">
          <button
            onClick={toggleDebug}
            className={
              "rounded-full px-4 py-1 text-sm font-bold text-white transition " +
              (debug ? "bg-[var(--color-meadow-deep)]" : "bg-black/30")
            }
          >
            {debug ? "ON" : "OFF"}
          </button>
        </Row>
        <p className="text-[11px] opacity-70">
          Para jugar en LAN desde el móvil, compila con{" "}
          <span className="font-mono">VITE_SERVER_HOST=&lt;ip-laptop&gt;</span>.
        </p>
      </div>
    </MenuLayout>
  );
}

function Row({ label, children }: { label: string; children: React.ReactNode }) {
  return (
    <div className="flex items-center justify-between gap-3 border-b border-black/10 pb-2">
      <span className="text-sm font-bold">{label}</span>
      {children}
    </div>
  );
}
