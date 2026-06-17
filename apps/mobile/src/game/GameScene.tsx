import { useFrame, useThree } from "@react-three/fiber/native";
import * as THREE from "three";
import { useMemo, useRef, useEffect } from "react";
import { EntityView } from "./EntityView";
import { GameCamera } from "./GameCamera";
import type { ArenaController } from "@hunger-arena/client-core";
import { AnimeSky } from "../gfx/AnimeSky";
import { ProceduralWorld } from "../gfx/ProceduralWorld";
import { createAtmosphereState } from "../gfx/atmosphere";
import { PALETTE } from "./art/palette";

interface GameSceneProps {
  controller: ArenaController;
}

export function GameScene({ controller }: GameSceneProps) {
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
      <AnimeSky atmos={atmos} />
      <ambientLight intensity={0.42} color="#8a9ec8" />
      <hemisphereLight args={["#a8c8e8", PALETTE.meadowDeep, 0.55]} />
      <directionalLight
        position={[18, 28, 12]}
        intensity={1.65}
        color={PALETTE.sunset}
        castShadow
        shadow-mapSize={[1024, 1024]}
        shadow-camera-left={-32}
        shadow-camera-right={32}
        shadow-camera-top={32}
        shadow-camera-bottom={-32}
        shadow-bias={-0.0004}
      />
      <GameCamera controller={controller} />
      <ProceduralWorld controller={controller} atmos={atmos} />
      <EntityView controller={controller} />
    </>
  );
}
