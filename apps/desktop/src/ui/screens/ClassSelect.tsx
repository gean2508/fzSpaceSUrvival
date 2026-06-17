import { CLASSES, getAbility } from "@hunger-arena/shared";
import { MenuLayout } from "./MenuLayout";
import { useGameStore } from "../../store/useGameStore";

const CLASS_ICON: Record<string, string> = {
  scout: "🏹",
  warrior: "🛡️",
  mage: "🔥",
};

export function ClassSelect() {
  const go = useGameStore((s) => s.go);
  const classId = useGameStore((s) => s.classId);
  const setClassId = useGameStore((s) => s.setClassId);

  return (
    <MenuLayout title="Elige clase" onBack={() => go("menu")}>
      <div className="float-in flex w-full max-w-2xl flex-col gap-3 md:flex-row">
        {Object.values(CLASSES).map((c) => (
          <button
            key={c.id}
            onClick={() => {
              setClassId(c.id);
              go("menu");
            }}
            className={
              "flex flex-1 flex-col items-center gap-2 rounded-2xl p-5 text-center shadow-lg transition " +
              (classId === c.id
                ? "bg-[var(--color-sunset)] text-white ring-4 ring-white/60"
                : "bg-[var(--color-cream)]/90 text-[var(--color-bark)] hover:bg-white")
            }
          >
            <span className="text-4xl">{CLASS_ICON[c.id] ?? "⚔️"}</span>
            <div className="text-lg font-extrabold">{c.name}</div>
            <div className="text-xs opacity-80">
              ❤ {c.maxHp} · 👟 {c.moveSpeed}
            </div>
            <div className="mt-1 flex flex-col gap-0.5 text-[11px] opacity-90">
              {c.abilities.map((aid) => {
                const ab = getAbility(aid);
                return <span key={aid}>• {ab?.name ?? aid}</span>;
              })}
            </div>
          </button>
        ))}
      </div>
    </MenuLayout>
  );
}
