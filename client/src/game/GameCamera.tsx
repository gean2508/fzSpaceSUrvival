import { useFrame, useThree } from "@react-three/fiber";
import { useMemo, useRef } from "react";
import * as THREE from "three";
import type { ArenaController } from "./ArenaController";

interface GameCameraProps {
  controller: ArenaController;
}

const CLOSED = new THREE.Vector3(14, 18, 14);
const OPEN = new THREE.Vector3(8, 11, 28);

/**
 * Adaptive orthographic follow camera with dynamic horizon tilt.
 * Sprinting / open exploration lerps pitch toward the horizon (10–15% more sky);
 * close combat snaps back to a tight top-down framing.
 */
export function GameCamera({ controller }: GameCameraProps) {
  const { camera } = useThree();
  const target = useRef(new THREE.Vector3());
  const look = useRef(new THREE.Vector3());
  const horizon = useRef(0);
  const offset = useMemo(() => new THREE.Vector3(), []);

  useFrame((_state, delta) => {
    const p = controller.getLocalPosition();
    const focusY = p.y * 0.6 + Math.min(p.y, 2);
    target.current.set(p.x, focusY, p.z);

    const goal = controller.getHorizonBlend();
    const kH = 1 - Math.pow(0.0012, delta);
    horizon.current += (goal - horizon.current) * kH;

    offset.copy(CLOSED).lerp(OPEN, horizon.current);
    const desired = target.current.clone().add(offset);

    const k = 1 - Math.pow(0.0016, delta);
    camera.position.lerp(desired, k);

    // Pitch opens with horizon: look target drops slightly so sky enters frame.
    const lookY = focusY - horizon.current * 1.8;
    look.current.set(p.x, lookY, p.z);
    camera.lookAt(look.current);
  });

  return null;
}
