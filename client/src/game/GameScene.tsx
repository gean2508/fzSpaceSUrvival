import { SoftShadows } from "@react-three/drei";
import type { ThreeEvent } from "@react-three/fiber";
import { useFrame, useThree } from "@react-three/fiber";
import * as THREE from "three";
import { useMemo, useRef, useEffect } from "react";
import { Perf } from "r3f-perf";
import { EntityView } from "./EntityView";
import { GameCamera } from "./GameCamera";
import type { ArenaController } from "./ArenaController";
import { AnimeSky } from "../gfx/AnimeSky";
import { ProceduralWorld } from "../gfx/ProceduralWorld";
import { createAtmosphereState } from "../gfx/atmosphere";
import { PALETTE } from "./art/palette";

interface GameSceneProps {
  controller: ArenaController;
  onAim?: (x: number, z: number) => void;
  perf?: boolean;
}

/** Invisible ground plane that turns mouse position into an aim direction. */
function AimPlane({
  controller,
  onAim,
}: {
  controller: ArenaController;
  onAim: (x: number, z: number) => void;
}) {
  const handle = (e: ThreeEvent<PointerEvent>) => {
    const lp = controller.getLocalPosition();
    const dx = e.point.x - lp.x;
    const dz = e.point.z - lp.z;
    const len = Math.hypot(dx, dz);
    if (len > 0.001) onAim(dx / len, dz / len);
  };
  return (
    <mesh rotation={[-Math.PI / 2, 0, 0]} position={[0, 0.02, 0]} onPointerMove={handle}>
      <planeGeometry args={[400, 400]} />
      <meshBasicMaterial transparent opacity={0} depthWrite={false} />
    </mesh>
  );
}

export function GameScene({ controller, onAim, perf }: GameSceneProps) {
  const { scene } = useThree();
  const atmos = useRef(createAtmosphereState());
  const fog = useMemo(
    () => new THREE.FogExp2(new THREE.Color(PALETTE.fog).getHex(), 0.012),
    []
  );

  useEffect(() => {
    scene.fog = fog;
    scene.background = new THREE.Color(PALETTE.fog);
    return () => {
      scene.fog = null;
    };
  }, [scene, fog]);

  useFrame(() => {
    const h = atmos.current.horizonColor;
    fog.color.setRGB(h.r, h.g, h.b);
    if (scene.background instanceof THREE.Color) {
      scene.background.copy(h);
    }
  });

  return (
    <>
      {perf && <Perf position="bottom-left" minimal />}
      <AnimeSky atmos={atmos} />

      {/* Anime lighting: warm sun + cool ambient for deep toon shadows */}
      <ambientLight intensity={0.42} color="#8a9ec8" />
      <hemisphereLight args={["#a8c8e8", PALETTE.meadowDeep, 0.55]} />
      <directionalLight
        position={[18, 28, 12]}
        intensity={1.65}
        color={PALETTE.sunset}
        castShadow
        shadow-mapSize={[2048, 2048]}
        shadow-camera-left={-32}
        shadow-camera-right={32}
        shadow-camera-top={32}
        shadow-camera-bottom={-32}
        shadow-bias={-0.0004}
      />
      <SoftShadows size={28} samples={10} focus={0.55} />

      <GameCamera controller={controller} />
      {onAim && <AimPlane controller={controller} onAim={onAim} />}
      <ProceduralWorld controller={controller} atmos={atmos} />
      <EntityView controller={controller} />
    </>
  );
}
