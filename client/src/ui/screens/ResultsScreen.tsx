import { MenuLayout, PrimaryButton } from "./MenuLayout";
import { useGameStore } from "../../store/useGameStore";

export function ResultsScreen() {
  const go = useGameStore((s) => s.go);
  const result = useGameStore((s) => s.lastResult);
  const profile = useGameStore((s) => s.profile);

  return (
    <MenuLayout title="Fin de la partida">
      <div className="float-in flex w-full max-w-sm flex-col items-center gap-5 rounded-2xl bg-[var(--color-cream)]/90 p-8 text-center text-[var(--color-bark)] shadow-xl">
        <div className="text-6xl">🏆</div>
        <div>
          <div className="text-sm font-semibold uppercase tracking-wide opacity-70">
            Oleadas superadas
          </div>
          <div className="text-5xl font-black text-[var(--color-sunset)]">
            {result?.waves ?? 0}
          </div>
        </div>

        {profile && (
          <div className="grid w-full grid-cols-3 gap-2 text-sm">
            <Stat label="XP" value={profile.xp} />
            <Stat label="Monedas" value={profile.coins} />
            <Stat label="Mejor" value={profile.bestWave} />
          </div>
        )}

        <div className="flex w-full gap-3">
          <PrimaryButton className="flex-1" onClick={() => go("game")}>
            Reintentar
          </PrimaryButton>
          <button
            className="flex-1 rounded-2xl bg-black/15 px-6 py-4 font-bold text-[var(--color-bark)]"
            onClick={() => go("menu")}
          >
            Menú
          </button>
        </div>
      </div>
    </MenuLayout>
  );
}

function Stat({ label, value }: { label: string; value: number }) {
  return (
    <div className="rounded-xl bg-white/70 p-2">
      <div className="text-[10px] font-bold uppercase opacity-60">{label}</div>
      <div className="text-lg font-extrabold">{value}</div>
    </div>
  );
}
