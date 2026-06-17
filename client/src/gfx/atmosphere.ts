import * as THREE from "three";
import { PALETTE } from "../game/art/palette";

/** Shared atmosphere state updated by AnimeSky, read by ground shaders + camera. */
export interface AtmosphereState {
  time: number;
  cloudOffset: THREE.Vector2;
  /** 0 = dawn tint, 0.5 = midday, 1 = warm dusk */
  dayPhase: number;
  horizonColor: THREE.Color;
  zenithColor: THREE.Color;
}

export function createAtmosphereState(): AtmosphereState {
  return {
    time: 0,
    cloudOffset: new THREE.Vector2(0, 0),
    dayPhase: 0.42,
    horizonColor: new THREE.Color(PALETTE.skyHorizon),
    zenithColor: new THREE.Color(PALETTE.skyTop),
  };
}

export function hexToVec3(hex: string): THREE.Vector3 {
  const c = new THREE.Color(hex);
  return new THREE.Vector3(c.r, c.g, c.b);
}
