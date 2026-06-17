import { useRef } from "react";
import { useFrame } from "@react-three/fiber";
import * as THREE from "three";
import type { ArenaController, RenderEntity } from "./ArenaController";
import { PALETTE } from "./art/palette";
import { buildCharacter, makeCelMaterial } from "../gfx/ProceduralMeshBuilder";

interface EntityViewProps {
  controller: ArenaController;
}

type PoolKind = "projectile" | "character";

function poolKind(e: RenderEntity): PoolKind {
  return e.kind === "projectile" ? "projectile" : "character";
}

function colorFor(e: RenderEntity): string {
  if (e.kind === "projectile") return PALETTE.spark;
  if (e.kind === "npc") return PALETTE.foe;
  return e.isLocal ? PALETTE.hero : PALETTE.rival;
}

interface PooledMesh {
  group: THREE.Group;
  body: THREE.MeshToonMaterial;
}

const projGeo = new THREE.SphereGeometry(0.25, 16, 16);

function buildProjectile(): PooledMesh {
  const group = new THREE.Group();
  const body = makeCelMaterial(PALETTE.spark);
  body.emissive = new THREE.Color(PALETTE.spark);
  body.emissiveIntensity = 0.9;
  group.add(new THREE.Mesh(projGeo, body));
  return { group, body };
}

/**
 * Renders all entities every frame via refs + useFrame (hot path, no React
 * state). Meshes are pooled per kind. Now reads vertical (y) for parkour and
 * uses the procedural cel-shaded character meshes.
 */
export function EntityView({ controller }: EntityViewProps) {
  const groupRef = useRef<THREE.Group>(null);
  const active = useRef<Map<string, PooledMesh>>(new Map());
  const free = useRef<Record<PoolKind, PooledMesh[]>>({ projectile: [], character: [] });
  const tmp = useRef(new THREE.Color());

  useFrame(() => {
    const group = groupRef.current;
    if (!group) return;

    const ents = controller.getRenderEntities(performance.now());
    const seen = new Set<string>();

    ents.forEach((e, id) => {
      if (e.dead) return;
      seen.add(id);
      const kind = poolKind(e);
      let pm = active.current.get(id);
      if (!pm) {
        pm =
          free.current[kind].pop() ??
          (kind === "projectile" ? buildProjectile() : buildCharacter(colorFor(e)));
        pm.group.visible = true;
        group.add(pm.group);
        active.current.set(id, pm);
        pm.group.userData.kind = kind;
        tmp.current.set(colorFor(e));
        pm.body.color.copy(tmp.current);
        if (pm.body.emissive.getHex() !== 0) pm.body.emissive.copy(tmp.current);
      }
      const baseY = kind === "projectile" ? 0.5 : 0.6;
      pm.group.position.set(e.x, e.y + baseY, e.z);
      pm.group.rotation.y = e.rot;
    });

    active.current.forEach((pm, id) => {
      if (!seen.has(id)) {
        pm.group.visible = false;
        group.remove(pm.group);
        const kind = pm.group.userData.kind as PoolKind;
        free.current[kind].push(pm);
        active.current.delete(id);
      }
    });
  });

  return <group ref={groupRef} />;
}
