import { useEffect, useRef } from "react";
import { Canvas } from "@react-three/fiber";
import { EffectComposer, Bloom, Vignette } from "@react-three/postprocessing";
import { GameScene } from "../game/GameScene";
import { useArena } from "../game/useArena";
import { useCombat } from "../input/useCombat";
import { usePcControls } from "../input/usePcControls";
import { useDesktopAim } from "../input/useDesktopAim";
import { CombatHud } from "./CombatHud";
import { DebugOverlay, useDebugToggle } from "../debug/DebugOverlay";
import { usePersistence } from "../persistence/usePersistence";
import { getDeviceId } from "../net/identity";
import { useGameStore } from "../store/useGameStore";
import { useControlStore } from "../store/useControlStore";
import { SERVER_URL } from "../config";

export function Game() {
  const mode = useGameStore((s) => s.mode);
  const classId = useGameStore((s) => s.classId);
  const endMatch = useGameStore((s) => s.endMatch);
  const go = useGameStore((s) => s.go);
  const setProfile = useGameStore((s) => s.setProfile);

  const setPlatform = useControlStore((s) => s.setPlatform);
  const setAim = useControlStore((s) => s.setAim);
  const resetPayload = useControlStore((s) => s.resetPayload);

  const canvasWrap = useRef<HTMLDivElement>(null);
  const debugVisible = useDebugToggle();
  const combat = useCombat(classId);

  useEffect(() => {
    setPlatform("pc");
    return () => resetPayload();
  }, [setPlatform, resetPayload]);

  usePcControls(true, combat.pressSlot);
  useDesktopAim(canvasWrap, setAim, true);

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
    deviceId: getDeviceId(),
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
      <div className="flex h-full flex-col items-center justify-center gap-4 bg-[var(--color-night)] p-6 text-center">
        <p className="max-w-md font-mono text-sm text-red-300 whitespace-pre-wrap">
          Error ({mode}) — {SERVER_URL}
          {"\n\n"}
          {error}
          {"\n\n"}
          ¿Server corriendo? npm run dev:server
        </p>
        <button
          className="rounded-xl bg-[var(--color-sunset)] px-6 py-3 font-bold text-white shadow-lg"
          onClick={() => go("menu")}
        >
          Volver al menú
        </button>
      </div>
    );
  }

  if (!controller) {
    return (
      <div className="menu-sky flex h-full items-center justify-center">
        <div className="float-in rounded-2xl bg-black/20 px-8 py-6 font-mono text-lg text-white backdrop-blur">
          {mode === "offline"
            ? "Iniciando simulación local…"
            : `Conectando a ${SERVER_URL}…`}
        </div>
      </div>
    );
  }

  return (
    <div ref={canvasWrap} className="relative h-full w-full">
      <Canvas
        shadows
        orthographic
        camera={{ position: [14, 18, 14], zoom: 42, near: 0.1, far: 200 }}
        dpr={[1, 2]}
        gl={{ antialias: true }}
        style={{ width: "100%", height: "100%" }}
      >
        <GameScene controller={controller} onAim={setAim} perf={debugVisible} />
        <EffectComposer>
          <Bloom
            mipmapBlur
            intensity={0.32}
            luminanceThreshold={0.82}
            luminanceSmoothing={0.35}
            radius={0.55}
          />
          <Vignette eskil={false} offset={0.22} darkness={0.42} />
        </EffectComposer>
      </Canvas>

      <p className="pointer-events-none absolute left-1/2 top-3 z-20 -translate-x-1/2 rounded-lg bg-black/40 px-3 py-1 font-mono text-[10px] text-white/80">
        Click para capturar mouse · WASD mover · Shift sprint · Espacio parkour · Q/E/R habilidades · Esc suelta mouse
      </p>

      <CombatHud
        controller={controller}
        slots={combat.slots}
        cooldownPct={combat.cooldownPct}
        onSlot={combat.pressSlot}
      />

      <button
        className="fixed top-3 right-3 z-30 rounded-lg bg-black/50 px-3 py-1.5 font-mono text-xs text-white/90 backdrop-blur"
        onClick={() => go("menu")}
      >
        ◀ Menú
      </button>

      <DebugOverlay
        controller={controller}
        visible={debugVisible}
        serverUrl={SERVER_URL}
        mode={mode}
        status={status}
      />
    </div>
  );
}
