import { useEffect, useRef, useState } from "react";
import type { AbilityDef, MovementState } from "@hunger-arena/shared";
import type { ArenaController } from "@hunger-arena/client-core";
import { isMobile } from "../config";

interface CombatHudProps {
  controller: ArenaController;
  slots: (AbilityDef | undefined)[];
  cooldownPct: number[];
  onSlot: (slot: number, down: boolean) => void;
}

const SLOT_KEYS = ["Q", "E", "R"];

const STATE_LABEL: Record<MovementState, string> = {
  idle: "QUIETO",
  running: "CORRIENDO",
  sprinting: "SPRINT",
  wall_trek: "EN MURO",
  climbing: "ESCALANDO",
  hanging: "COLGADO",
  exhausted: "AGOTADO",
};

interface Survival {
  stamina: number;
  maxStamina: number;
  hunger: number;
  thirst: number;
  state: MovementState;
}

export function CombatHud({
  controller,
  slots,
  cooldownPct,
  onSlot,
}: CombatHudProps) {
  const [hp, setHp] = useState(1);
  const [maxHp, setMaxHp] = useState(1);
  const [dead, setDead] = useState(false);
  const [survival, setSurvival] = useState<Survival | null>(null);
  const flashRef = useRef<HTMLDivElement>(null);
  const prevHp = useRef<number | null>(null);

  useEffect(() => {
    const id = setInterval(() => {
      const s = controller.getLocalStatus();
      if (s) {
        setHp(s.hp);
        setMaxHp(s.maxHp);
        setDead(s.dead);
        if (prevHp.current !== null && s.hp < prevHp.current && flashRef.current) {
          flashRef.current.style.opacity = "0.55";
          requestAnimationFrame(() => {
            if (flashRef.current) flashRef.current.style.opacity = "0";
          });
        }
        prevHp.current = s.hp;
      }
      setSurvival(controller.getLocalSurvival());
    }, 100);
    return () => clearInterval(id);
  }, [controller]);

  const pct = maxHp > 0 ? Math.max(0, Math.min(1, hp / maxHp)) : 0;
  const hpColor = pct > 0.5 ? "#4ade80" : pct > 0.25 ? "#facc15" : "#ef4444";

  return (
    <>
      <div
        ref={flashRef}
        style={{
          position: "fixed",
          inset: 0,
          background:
            "radial-gradient(circle, rgba(255,0,0,0) 55%, rgba(255,0,0,0.6) 100%)",
          opacity: 0,
          transition: "opacity 0.4s ease-out",
          pointerEvents: "none",
          zIndex: 40,
        }}
      />

      {/* Health bar */}
      <div
        style={{
          position: "fixed",
          left: "50%",
          bottom: 24,
          transform: "translateX(-50%)",
          width: 260,
          maxWidth: "60vw",
          zIndex: 30,
          pointerEvents: "none",
        }}
      >
        <div
          style={{
            height: 18,
            borderRadius: 9,
            background: "rgba(0,0,0,0.55)",
            border: "1px solid rgba(255,255,255,0.25)",
            overflow: "hidden",
          }}
        >
          <div
            style={{
              width: `${pct * 100}%`,
              height: "100%",
              background: hpColor,
              transition: "width 0.15s linear, background 0.3s",
            }}
          />
        </div>
        <div
          style={{
            textAlign: "center",
            color: "#e8f4ff",
            fontFamily: "monospace",
            fontSize: 11,
            marginTop: 2,
            textShadow: "0 1px 2px #000",
          }}
        >
          {Math.ceil(hp)} / {maxHp} {dead ? "· ELIMINADO" : ""}
        </div>

        {/* Survival meters: stamina / hunger / thirst + movement state */}
        {survival && (
          <div style={{ marginTop: 6 }}>
            <Meter label="STA" value={survival.stamina / survival.maxStamina} color="#7dd3fc" />
            <Meter label="HAM" value={survival.hunger / 100} color="#f4a259" />
            <Meter label="SED" value={survival.thirst / 100} color="#5eead4" />
            <div
              style={{
                textAlign: "center",
                color: survival.state === "exhausted" ? "#ef4444" : "#cfe0e8",
                fontFamily: "monospace",
                fontSize: 10,
                marginTop: 2,
                textShadow: "0 1px 2px #000",
              }}
            >
              {STATE_LABEL[survival.state]}
            </div>
          </div>
        )}
      </div>

      {/* Ability buttons — PC only (mobile uses the touch cluster). */}
      <div
        style={{
          position: "fixed",
          right: "50%",
          bottom: 52,
          transform: "translateX(170px)",
          display: isMobile ? "none" : "flex",
          gap: 12,
          zIndex: 30,
        }}
      >
        {slots.map((ab, i) =>
          ab ? (
            <button
              key={ab.id}
              onPointerDown={(e) => {
                e.preventDefault();
                onSlot(i, true);
              }}
              onPointerUp={() => onSlot(i, false)}
              onPointerLeave={() => onSlot(i, false)}
              style={{
                position: "relative",
                width: 62,
                height: 62,
                borderRadius: "50%",
                border: "2px solid rgba(0,229,255,0.5)",
                background: "rgba(10,20,40,0.8)",
                color: "#e8f4ff",
                fontFamily: "monospace",
                fontSize: 10,
                cursor: "pointer",
                overflow: "hidden",
                touchAction: "none",
              }}
              title={`${ab.name} (${SLOT_KEYS[i] ?? ""})`}
            >
              <div
                style={{
                  position: "absolute",
                  left: 0,
                  bottom: 0,
                  width: "100%",
                  height: `${(cooldownPct[i] ?? 0) * 100}%`,
                  background: "rgba(0,0,0,0.6)",
                  transition: "height 0.07s linear",
                }}
              />
              <span style={{ position: "relative" }}>{ab.name}</span>
            </button>
          ) : null
        )}
      </div>
    </>
  );
}

function Meter({ label, value, color }: { label: string; value: number; color: string }) {
  const v = Math.max(0, Math.min(1, value));
  return (
    <div style={{ display: "flex", alignItems: "center", gap: 6, marginTop: 3 }}>
      <span
        style={{
          color: "#cfe0e8",
          fontFamily: "monospace",
          fontSize: 9,
          width: 26,
          textShadow: "0 1px 2px #000",
        }}
      >
        {label}
      </span>
      <div
        style={{
          flex: 1,
          height: 7,
          borderRadius: 4,
          background: "rgba(0,0,0,0.5)",
          overflow: "hidden",
        }}
      >
        <div style={{ width: `${v * 100}%`, height: "100%", background: color }} />
      </div>
    </div>
  );
}
