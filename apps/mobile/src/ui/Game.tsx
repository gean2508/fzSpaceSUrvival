import { useEffect, useRef, useState } from "react";
import { StyleSheet, Text, TouchableOpacity, View } from "react-native";
import { Canvas } from "@react-three/fiber/native";
import { GameScene } from "../game/GameScene";
import { useArena } from "../game/useArena";
import { useCombat } from "../input/useCombat";
import { TouchControls } from "../input/TouchControls";
import { CombatHud } from "./CombatHud";
import { DebugOverlay } from "../debug/DebugOverlay";
import { usePersistence } from "../persistence/usePersistence";
import { ensureDeviceId } from "../net/identity";
import { useGameStore } from "../store/useGameStore";
import { useControlStore } from "../store/useControlStore";
import { SERVER_URL } from "../config";
import { COLORS } from "../theme/colors";
import AsyncStorage from "@react-native-async-storage/async-storage";

export function Game() {
  const mode = useGameStore((s) => s.mode);
  const classId = useGameStore((s) => s.classId);
  const endMatch = useGameStore((s) => s.endMatch);
  const go = useGameStore((s) => s.go);
  const setProfile = useGameStore((s) => s.setProfile);
  const resetPayload = useControlStore((s) => s.resetPayload);

  const [debugVisible, setDebugVisible] = useState(false);
  const [deviceId, setDeviceId] = useState<string | null>(null);
  const combat = useCombat(classId);

  useEffect(() => {
    ensureDeviceId().then(setDeviceId);
    AsyncStorage.getItem("arena-debug").then((v) => setDebugVisible(v === "1"));
    return () => resetPayload();
  }, [resetPayload]);

  const { controller, transport, status, error } = useArena({
    mode,
    classId,
    getRawInput: () => {
      const p = useControlStore.getState().payload;
      return {
        moveX: p.moveX,
        moveZ: p.moveZ,
        aimX: p.aimX,
        aimZ: p.aimZ,
        abilities: p.abilities,
        sprint: p.sprint,
        action: p.action,
      };
    },
  });

  const { profile } = usePersistence({
    transport,
    controller,
    mode,
    deviceId: deviceId ?? "pending",
  });

  useEffect(() => {
    if (profile) setProfile(profile);
  }, [profile, setProfile]);

  const ended = useRef(false);
  useEffect(() => {
    if (!controller) return;
    ended.current = false;
    const id = setInterval(() => {
      const st = controller.getLocalStatus();
      if (!ended.current && (controller.getWorldStatus() === "ended" || st?.dead)) {
        ended.current = true;
        endMatch({ waves: controller.getWave(), mode });
      }
    }, 400);
    return () => clearInterval(id);
  }, [controller, endMatch, mode]);

  if (error) {
    return (
      <View style={styles.center}>
        <Text style={styles.errorText}>
          Error ({mode}) — {SERVER_URL}
          {"\n\n"}
          {error}
          {"\n\n"}
          ¿Server corriendo? npm run dev:server
        </Text>
        <TouchableOpacity style={styles.menuBtn} onPress={() => go("menu")}>
          <Text style={styles.menuBtnText}>Volver al menú</Text>
        </TouchableOpacity>
      </View>
    );
  }

  if (!controller) {
    return (
      <View style={styles.center}>
        <View style={styles.loadingCard}>
          <Text style={styles.loadingText}>
            {mode === "offline"
              ? "Iniciando simulación local…"
              : `Conectando a ${SERVER_URL}…`}
          </Text>
        </View>
      </View>
    );
  }

  return (
    <View style={styles.root}>
      <Canvas
        shadows
        orthographic
        camera={{ position: [14, 18, 14], zoom: 42, near: 0.1, far: 200 }}
        gl={{ antialias: true }}
        style={styles.canvas}
      >
        <GameScene controller={controller} />
      </Canvas>

      <TouchControls
        slots={combat.slots}
        cooldownPct={combat.cooldownPct}
        pressSlot={combat.pressSlot}
      />

      <CombatHud controller={controller} />

      <TouchableOpacity style={styles.backMenu} onPress={() => go("menu")}>
        <Text style={styles.backMenuText}>◀ Menú</Text>
      </TouchableOpacity>

      <DebugOverlay
        controller={controller}
        visible={debugVisible}
        serverUrl={SERVER_URL}
        mode={mode}
        status={status}
      />
    </View>
  );
}

const styles = StyleSheet.create({
  root: { flex: 1, backgroundColor: COLORS.night },
  canvas: { flex: 1 },
  center: {
    flex: 1,
    alignItems: "center",
    justifyContent: "center",
    backgroundColor: COLORS.night,
    padding: 24,
  },
  errorText: {
    color: "#fca5a5",
    fontFamily: "monospace",
    fontSize: 13,
    textAlign: "center",
    marginBottom: 16,
  },
  menuBtn: {
    borderRadius: 12,
    backgroundColor: COLORS.sunset,
    paddingHorizontal: 24,
    paddingVertical: 12,
  },
  menuBtnText: { color: COLORS.white, fontWeight: "700" },
  loadingCard: {
    borderRadius: 16,
    backgroundColor: "rgba(0,0,0,0.25)",
    paddingHorizontal: 28,
    paddingVertical: 20,
  },
  loadingText: { color: COLORS.white, fontSize: 16, fontFamily: "monospace" },
  backMenu: {
    position: "absolute",
    top: 12,
    right: 12,
    zIndex: 30,
    borderRadius: 8,
    backgroundColor: "rgba(0,0,0,0.5)",
    paddingHorizontal: 12,
    paddingVertical: 8,
  },
  backMenuText: { color: "rgba(255,255,255,0.9)", fontFamily: "monospace", fontSize: 12 },
});
