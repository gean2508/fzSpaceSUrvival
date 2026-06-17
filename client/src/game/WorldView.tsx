import { useRef } from "react";
import { useFrame } from "@react-three/fiber";
import * as THREE from "three";
import type { ArenaController } from "./ArenaController";
import { buildBlock, buildInteractable } from "../gfx/ProceduralMeshBuilder";

interface WorldViewProps {
  controller: ArenaController;
}

/**
 * Renders the static world geometry (climbable blocks) and gatherable nodes.
 * Built lazily once obstacles arrive in a snapshot; node visibility tracks the
 * authoritative `looted` flag. No React state on the hot path.
 */
export function WorldView({ controller }: WorldViewProps) {
  const groupRef = useRef<THREE.Group>(null);
  const builtObstacles = useRef(false);
  const nodes = useRef<Map<string, THREE.Group>>(new Map());

  useFrame(() => {
    const group = groupRef.current;
    if (!group) return;

    // Build climbable blocks once.
    if (!builtObstacles.current) {
      const obs = controller.getObstacles();
      if (obs.length) {
        for (const o of obs) {
          const block = buildBlock(o.halfW, o.height, o.halfD);
          block.position.set(o.x, o.height / 2, o.z);
          group.add(block);
        }
        builtObstacles.current = true;
      }
    }

    // Sync gatherable nodes (create on first sight, hide when looted).
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

  return <group ref={groupRef} />;
}
