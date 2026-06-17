import * as THREE from "three";
import { NOISE_GLSL } from "./noise.glsl";

/**
 * Inverted sky dome: 3-point anime gradient (zenith / horizon / nadir) with
 * subtle procedural variation. No texture assets.
 */
export function createSkyMaterial(): THREE.ShaderMaterial {
  return new THREE.ShaderMaterial({
    side: THREE.BackSide,
    depthWrite: false,
    fog: false,
    uniforms: {
      uTime: { value: 0 },
      uDayPhase: { value: 0.42 },
      uZenith: { value: new THREE.Vector3(0.24, 0.42, 0.62) },
      uHorizon: { value: new THREE.Vector3(0.86, 0.82, 0.72) },
      uWarm: { value: new THREE.Vector3(0.96, 0.72, 0.66) },
      uNadir: { value: new THREE.Vector3(0.55, 0.68, 0.52) },
    },
    vertexShader: /* glsl */ `
      varying vec3 vWorldPos;
      void main() {
        vec4 wp = modelMatrix * vec4(position, 1.0);
        vWorldPos = wp.xyz;
        gl_Position = projectionMatrix * viewMatrix * wp;
      }
    `,
    fragmentShader: /* glsl */ `
      uniform float uTime;
      uniform float uDayPhase;
      uniform vec3 uZenith;
      uniform vec3 uHorizon;
      uniform vec3 uWarm;
      uniform vec3 uNadir;
      varying vec3 vWorldPos;
      ${NOISE_GLSL}

      void main() {
        vec3 dir = normalize(vWorldPos);
        float h = dir.y * 0.5 + 0.5;

        vec3 zenith = mix(uZenith, vec3(0.18, 0.28, 0.55), uDayPhase * 0.15);
        vec3 horizon = mix(uHorizon, uWarm, smoothstep(0.35, 0.65, uDayPhase));
        vec3 nadir = uNadir;

        vec3 col = h > 0.5
          ? mix(horizon, zenith, smoothstep(0.5, 1.0, h))
          : mix(nadir, horizon, smoothstep(0.0, 0.5, h));

        float shimmer = snoise(dir.xz * 3.0 + uTime * 0.02) * 0.03;
        col += shimmer;

        gl_FragColor = vec4(col, 1.0);
      }
    `,
  });
}
