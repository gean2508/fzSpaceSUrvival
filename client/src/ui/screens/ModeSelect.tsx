import { MenuLayout } from "./MenuLayout";
import { useGameStore } from "../../store/useGameStore";
import type { GameMode } from "../../net/Transport";

const MODES: { id: GameMode; name: string; desc: string; icon: string }[] = [
  { id: "offline", name: "Offline", desc: "Juega sin conexión. Tu progreso se guarda y sincroniza luego.", icon: "🌙" },
  { id: "lan", name: "LAN", desc: "Conéctate al servidor de tu red local (misma WiFi).", icon: "🏠" },
  { id: "online", name: "Online", desc: "Juega con cualquiera en el servidor remoto.", icon: "🌍" },
];

export function ModeSelect() {
  const go = useGameStore((s) => s.go);
  const mode = useGameStore((s) => s.mode);
  const setMode = useGameStore((s) => s.setMode);

  return (
    <MenuLayout title="Elige modo" onBack={() => go("menu")}>
      <div className="float-in flex w-full max-w-md flex-col gap-3">
        {MODES.map((m) => (
          <button
            key={m.id}
            onClick={() => {
              setMode(m.id);
              go("menu");
            }}
            className={
              "flex items-center gap-4 rounded-2xl p-4 text-left shadow-lg transition " +
              (mode === m.id
                ? "bg-[var(--color-sunset)] text-white ring-4 ring-white/60"
                : "bg-[var(--color-cream)]/90 text-[var(--color-bark)] hover:bg-white")
            }
          >
            <span className="text-3xl">{m.icon}</span>
            <span>
              <div className="text-lg font-extrabold">{m.name}</div>
              <div className="text-xs opacity-80">{m.desc}</div>
            </span>
          </button>
        ))}
      </div>
    </MenuLayout>
  );
}
