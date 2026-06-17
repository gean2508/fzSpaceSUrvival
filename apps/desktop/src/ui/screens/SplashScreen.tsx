import { useEffect } from "react";
import { MenuLayout } from "./MenuLayout";
import { useGameStore } from "../../store/useGameStore";

export function SplashScreen() {
  const go = useGameStore((s) => s.go);

  useEffect(() => {
    const t = setTimeout(() => go("menu"), 1800);
    return () => clearTimeout(t);
  }, [go]);

  return (
    <MenuLayout>
      <div
        className="float-in flex cursor-pointer flex-col items-center"
        onClick={() => go("menu")}
      >
        <div className="text-6xl font-black tracking-tight text-[var(--color-night)] drop-shadow-lg">
          Hunger
        </div>
        <div className="-mt-2 text-6xl font-black tracking-tight text-[var(--color-sunset)] drop-shadow-lg">
          Arena
        </div>
        <p className="mt-4 animate-pulse text-sm font-semibold text-[var(--color-night)]/70">
          toca para empezar
        </p>
      </div>
    </MenuLayout>
  );
}
