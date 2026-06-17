import { getClass } from "@hunger-arena/shared";
import { MenuLayout, PrimaryButton } from "./MenuLayout";
import { useGameStore } from "../../store/useGameStore";
import { isLoggedIn, logout } from "../../net/auth";

const MODE_LABEL: Record<string, string> = {
  offline: "Offline",
  lan: "LAN",
  online: "Online",
};

export function MainMenu() {
  const go = useGameStore((s) => s.go);
  const mode = useGameStore((s) => s.mode);
  const classId = useGameStore((s) => s.classId);
  const profile = useGameStore((s) => s.profile);
  const setProfile = useGameStore((s) => s.setProfile);
  const cls = getClass(classId);

  return (
    <MenuLayout>
      <div className="float-in flex w-full max-w-md flex-col items-center gap-5">
        <div className="text-center">
          <div className="text-5xl font-black text-[var(--color-night)] drop-shadow">
            Hunger Arena
          </div>
          <div className="text-sm font-semibold text-[var(--color-night)]/70">
            supervivencia · oleadas · multijugador
          </div>
        </div>

        {/* Profile card */}
        <div className="w-full rounded-2xl bg-[var(--color-cream)]/85 p-4 text-[var(--color-bark)] shadow-lg backdrop-blur">
          <div className="flex items-center justify-between">
            <span className="font-bold">
              {profile?.displayName ?? "Invitado"}
              {profile?.hasAccount ? " ✓" : ""}
            </span>
            <span className="text-xs opacity-70">
              {profile ? `XP ${profile.xp} · 🪙 ${profile.coins}` : "—"}
            </span>
          </div>
          <div className="mt-1 text-xs opacity-70">
            Mejor oleada: {profile?.bestWave ?? 0} · Partidas:{" "}
            {profile?.matches ?? 0}
          </div>
        </div>

        <PrimaryButton className="w-full" onClick={() => go("game")}>
          ▶ Jugar
        </PrimaryButton>

        <div className="grid w-full grid-cols-2 gap-3">
          <MenuChip label="Modo" value={MODE_LABEL[mode]} onClick={() => go("mode")} />
          <MenuChip label="Clase" value={cls.name} onClick={() => go("class")} />
          <MenuChip
            label="Cuenta"
            value={isLoggedIn() ? "Conectado" : "Invitado"}
            onClick={() => go("auth")}
          />
          <MenuChip label="Ajustes" value="⚙" onClick={() => go("settings")} />
        </div>

        {isLoggedIn() && (
          <button
            className="text-xs font-semibold text-[var(--color-night)]/60 underline"
            onClick={() => {
              logout();
              setProfile(null);
              go("menu");
            }}
          >
            Cerrar sesión
          </button>
        )}
      </div>
    </MenuLayout>
  );
}

function MenuChip({
  label,
  value,
  onClick,
}: {
  label: string;
  value: string;
  onClick: () => void;
}) {
  return (
    <button
      onClick={onClick}
      className="rounded-xl bg-white/70 px-4 py-3 text-left shadow transition hover:bg-white/90"
    >
      <div className="text-[10px] font-bold uppercase tracking-wide text-[var(--color-bark)]/60">
        {label}
      </div>
      <div className="font-bold text-[var(--color-bark)]">{value}</div>
    </button>
  );
}
