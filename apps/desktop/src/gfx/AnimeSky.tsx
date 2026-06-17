import { useFrame } from "@react-three/fiber";
import { useMemo, useRef } from "react";
import * as THREE from "three";
import type { AtmosphereState } from "./atmosphere";
import { hexToVec3 } from "./atmosphere";
import { PALETTE } from "../game/art/palette";
import { createSkyMaterial } from "./shaders/skyMaterial";
import { createCloudMaterial } from "./shaders/cloudMaterial";

interface AnimeSkyProps {
  atmos: React.MutableRefObject<AtmosphereState>;
}

const CLOUD_LAYERS: {
  y: number;
  w: number;
  d: number;
  seed: number;
  drift: [number, number];
}[] = [
  { y: 26, w: 90, d: 70, seed: 0, drift: [0.018, 0.007] },
  { y: 32, w: 110, d: 55, seed: 2.3, drift: [0.012, 0.011] },
  { y: 38, w: 75, d: 85, seed: 5.1, drift: [0.022, 0.005] },
  { y: 44, w: 130, d: 45, seed: 8.7, drift: [0.009, 0.014] },
];

/**
 * Procedural Ghibli sky dome + drifting cel-shaded cloud planes.
 * Drives shared atmosphere uniforms (cloud offset, horizon color, day phase).
 */
export function AnimeSky({ atmos }: AnimeSkyProps) {
  const skyMat = useMemo(() => createSkyMaterial(), []);
  const cloudMats = useMemo(() => CLOUD_LAYERS.map(() => createCloudMaterial()), []);

  const cloudRefs = useRef<THREE.Mesh[]>([]);

  useFrame((_, delta) => {
    const a = atmos.current;
    a.time += delta;
    a.cloudOffset.x += delta * 0.35;
    a.cloudOffset.y += delta * 0.12;

    // Gentle midday → warm afternoon cycle (visual only).
    a.dayPhase = 0.38 + Math.sin(a.time * 0.015) * 0.08 + 0.08;

    const warm = 0.35 + Math.sin(a.time * 0.02 + 1.2) * 0.15;
    a.horizonColor.setRGB(
      0.82 + warm * 0.12,
      0.78 + warm * 0.08,
      0.68 + warm * 0.05
    );
    a.zenithColor.setRGB(0.35, 0.58, 0.78);

    skyMat.uniforms.uTime.value = a.time;
    skyMat.uniforms.uDayPhase.value = a.dayPhase;
    const zen = hexToVec3(PALETTE.skyTop);
    zen.lerp(new THREE.Vector3(a.zenithColor.r, a.zenithColor.g, a.zenithColor.b), 0.45);
    skyMat.uniforms.uZenith.value.copy(zen);
    skyMat.uniforms.uHorizon.value.set(a.horizonColor.r, a.horizonColor.g, a.horizonColor.b);
    skyMat.uniforms.uWarm.value.copy(hexToVec3("#f4b8a8"));

    cloudMats.forEach((mat, i) => {
      const layer = CLOUD_LAYERS[i];
      mat.uniforms.uTime.value = a.time;
      mat.uniforms.uOffset.value.set(
        a.cloudOffset.x * layer.drift[0] * 40,
        a.cloudOffset.y * layer.drift[1] * 40
      );
      mat.uniforms.uSeed.value = layer.seed;
    });
  });

  return (
    <group renderOrder={-200}>
      <mesh scale={150} frustumCulled={false}>
        <sphereGeometry args={[1, 32, 18]} />
        <primitive object={skyMat} attach="material" />
      </mesh>

      {CLOUD_LAYERS.map((layer, i) => (
        <mesh
          key={i}
          ref={(m) => {
            if (m) cloudRefs.current[i] = m;
          }}
          position={[0, layer.y, -8]}
          rotation={[-Math.PI / 2, 0, 0]}
          frustumCulled={false}
        >
          <planeGeometry args={[layer.w, layer.d, 1, 1]} />
          <primitive object={cloudMats[i]} attach="material" />
        </mesh>
      ))}
    </group>
  );
}
