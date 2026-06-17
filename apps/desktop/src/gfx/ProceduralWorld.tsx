import { useFrame } from "@react-three/fiber";
import { useLayoutEffect, useMemo, useRef } from "react";
import * as THREE from "three";
import { WORLD_HALF } from "@hunger-arena/shared";
import type { ArenaController } from "@hunger-arena/client-core";
import type { AtmosphereState } from "./atmosphere";
import { hexToVec3 } from "./atmosphere";
import { PALETTE } from "../game/art/palette";
import { createGroundMaterial, createBlockMaterial } from "./shaders/groundMaterial";
import { buildInteractable } from "./ProceduralMeshBuilder";

const CELL = 1;
const dummy = new THREE.Object3D();

interface ProceduralWorldProps {
  controller: ArenaController;
  atmos: React.MutableRefObject<AtmosphereState>;
}

/**
 * Modular instanced ground grid (grass wind shader + cloud cookies + fog) and
 * instanced climbable blocks. Interactables stay as lightweight procedural
 * meshes (low count). 100% code-generated — no external textures.
 */
export function ProceduralWorld({ controller, atmos }: ProceduralWorldProps) {
  const groundMat = useMemo(() => {
    const m = createGroundMaterial();
    m.uniforms.uGrassA.value.copy(hexToVec3(PALETTE.meadow));
    m.uniforms.uGrassB.value.copy(hexToVec3(PALETTE.meadowDeep));
    m.uniforms.uDirt.value.copy(hexToVec3(PALETTE.earth));
    m.uniforms.uHorizonFog.value.copy(hexToVec3(PALETTE.fog));
    return m;
  }, []);
  const blockMat = useMemo(() => createBlockMaterial(), []);

  const groundRef = useRef<THREE.InstancedMesh>(null);
  const blocksRef = useRef<THREE.InstancedMesh>(null);
  const builtBlocks = useRef(false);
  const nodes = useRef<Map<string, THREE.Group>>(new Map());
  const groupRef = useRef<THREE.Group>(null);

  const gridCount = useMemo(() => {
    const n = Math.floor((WORLD_HALF * 2) / CELL);
    return n * n;
  }, []);

  const groundGeo = useMemo(() => {
    const g = new THREE.PlaneGeometry(CELL, CELL, 4, 4);
    g.rotateX(-Math.PI / 2);
    return g;
  }, []);

  const blockGeo = useMemo(() => new THREE.BoxGeometry(1, 1, 1), []);

  useLayoutEffect(() => {
    const mesh = groundRef.current;
    if (!mesh) return;
    const half = WORLD_HALF;
    let i = 0;
    for (let iz = -half; iz < half; iz += CELL) {
      for (let ix = -half; ix < half; ix += CELL) {
        dummy.position.set(ix + CELL * 0.5, 0, iz + CELL * 0.5);
        dummy.rotation.set(0, 0, 0);
        dummy.scale.set(1, 1, 1);
        dummy.updateMatrix();
        mesh.setMatrixAt(i++, dummy.matrix);
      }
    }
    mesh.instanceMatrix.needsUpdate = true;
  }, [gridCount]);

  useFrame(() => {
    const a = atmos.current;
    groundMat.uniforms.uTime.value = a.time;
    groundMat.uniforms.uCloudOffset.value.copy(a.cloudOffset);
    groundMat.uniforms.uHorizonFog.value.set(
      a.horizonColor.r,
      a.horizonColor.g,
      a.horizonColor.b
    );

    const group = groupRef.current;
    if (!group) return;

    if (!builtBlocks.current) {
      const obs = controller.getObstacles();
      const bMesh = blocksRef.current;
      if (obs.length && bMesh) {
        let i = 0;
        for (const o of obs) {
          dummy.position.set(o.x, o.height / 2, o.z);
          dummy.scale.set(o.halfW * 2, o.height, o.halfD * 2);
          dummy.rotation.set(0, 0, 0);
          dummy.updateMatrix();
          bMesh.setMatrixAt(i++, dummy.matrix);
          const climbable = o.height <= 3;
          blockMat.color.set(climbable ? PALETTE.earth : PALETTE.bark);
        }
        bMesh.count = i;
        bMesh.instanceMatrix.needsUpdate = true;
        builtBlocks.current = true;
      }
    }

    for (const n of controller.getInteractables()) {
      let mesh = nodes.current.get(n.id);
      if (!mesh) {
        mesh = buildInteractable(n.kind);
        mesh.position.set(n.x, 0, n.z);
        group.add(mesh);
        nodes.current.set(n.id, mesh);
      }
      mesh.visible = !n.looted;
    }
  });

  return (
    <group ref={groupRef}>
      <instancedMesh
        ref={groundRef}
        args={[groundGeo, groundMat, gridCount]}
        receiveShadow
        frustumCulled={false}
      />
      <instancedMesh
        ref={blocksRef}
        args={[blockGeo, blockMat, 32]}
        castShadow
        receiveShadow
      />

      {/* Arena boundary ring */}
      <mesh rotation={[-Math.PI / 2, 0, 0]} position={[0, 0.03, 0]}>
        <ringGeometry args={[WORLD_HALF - 0.15, WORLD_HALF, 96]} />
        <meshBasicMaterial
          color={PALETTE.cream}
          transparent
          opacity={0.45}
          side={THREE.DoubleSide}
        />
      </mesh>

      {/* Deep meadow base disc (depth under modular tiles) */}
      <mesh rotation={[-Math.PI / 2, 0, 0]} position={[0, -0.02, 0]} receiveShadow>
        <circleGeometry args={[WORLD_HALF + 6, 48]} />
        <meshBasicMaterial color={PALETTE.meadowDeep} />
      </mesh>
    </group>
  );
}
