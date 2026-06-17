import { useEffect, useRef, useState } from "react";
import { StyleSheet, Text, View } from "react-native";
import type { MovementState } from "@hunger-arena/shared";
import type { ArenaController } from "@hunger-arena/client-core";

interface CombatHudProps {
  controller: ArenaController;
}

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

export function CombatHud({ controller }: CombatHudProps) {
  const [hp, setHp] = useState(1);
  const [maxHp, setMaxHp] = useState(1);
  const [dead, setDead] = useState(false);
  const [survival, setSurvival] = useState<Survival | null>(null);
  const [flash, setFlash] = useState(0);
  const prevHp = useRef<number | null>(null);

  useEffect(() => {
    const id = setInterval(() => {
      const s = controller.getLocalStatus();
      if (s) {
        setHp(s.hp);
        setMaxHp(s.maxHp);
        setDead(s.dead);
        if (prevHp.current !== null && s.hp < prevHp.current) {
          setFlash(0.55);
          setTimeout(() => setFlash(0), 400);
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
      <View pointerEvents="none" style={[styles.flash, { opacity: flash }]} />

      <View pointerEvents="none" style={styles.wrap}>
        <View style={styles.hpTrack}>
          <View style={[styles.hpFill, { width: `${pct * 100}%`, backgroundColor: hpColor }]} />
        </View>
        <Text style={styles.hpText}>
          {Math.ceil(hp)} / {maxHp} {dead ? "· ELIMINADO" : ""}
        </Text>

        {survival && (
          <View style={styles.survival}>
            <Meter label="STA" value={survival.stamina / survival.maxStamina} color="#7dd3fc" />
            <Meter label="HAM" value={survival.hunger / 100} color="#f4a259" />
            <Meter label="SED" value={survival.thirst / 100} color="#5eead4" />
            <Text
              style={[
                styles.state,
                survival.state === "exhausted" && { color: "#ef4444" },
              ]}
            >
              {STATE_LABEL[survival.state]}
            </Text>
          </View>
        )}
      </View>
    </>
  );
}

function Meter({ label, value, color }: { label: string; value: number; color: string }) {
  const v = Math.max(0, Math.min(1, value));
  return (
    <View style={styles.meterRow}>
      <Text style={styles.meterLabel}>{label}</Text>
      <View style={styles.meterTrack}>
        <View style={[styles.meterFill, { width: `${v * 100}%`, backgroundColor: color }]} />
      </View>
    </View>
  );
}

const styles = StyleSheet.create({
  flash: {
    ...StyleSheet.absoluteFill,
    backgroundColor: "rgba(255,0,0,0.45)",
    zIndex: 40,
  },
  wrap: {
    position: "absolute",
    left: "20%",
    right: "20%",
    bottom: 24,
    zIndex: 30,
    alignItems: "center",
  },
  hpTrack: {
    width: "100%",
    maxWidth: 260,
    height: 18,
    borderRadius: 9,
    backgroundColor: "rgba(0,0,0,0.55)",
    borderWidth: 1,
    borderColor: "rgba(255,255,255,0.25)",
    overflow: "hidden",
  },
  hpFill: { height: "100%" },
  hpText: {
    color: "#e8f4ff",
    fontFamily: "monospace",
    fontSize: 11,
    marginTop: 2,
  },
  survival: { width: "100%", maxWidth: 260, marginTop: 6 },
  state: {
    textAlign: "center",
    color: "#cfe0e8",
    fontFamily: "monospace",
    fontSize: 10,
    marginTop: 2,
  },
  meterRow: { flexDirection: "row", alignItems: "center", gap: 6, marginTop: 3 },
  meterLabel: {
    color: "#cfe0e8",
    fontFamily: "monospace",
    fontSize: 9,
    width: 26,
  },
  meterTrack: {
    flex: 1,
    height: 7,
    borderRadius: 4,
    backgroundColor: "rgba(0,0,0,0.5)",
    overflow: "hidden",
  },
  meterFill: { height: "100%" },
});
