import * as THREE from "three";
import { NOISE_GLSL } from "./noise.glsl";

/**
 * Modular grass ground tile: vertex wind sway + cloud-shadow cookies + horizon fog.
 * All GPU — no texture maps.
 */
export function createGroundMaterial(): THREE.ShaderMaterial {
  return new THREE.ShaderMaterial({
    uniforms: {
      uTime: { value: 0 },
      uCloudOffset: { value: new THREE.Vector2(0, 0) },
      uCloudScale: { value: 0.035 },
      uGrassA: { value: new THREE.Vector3(0.61, 0.76, 0.42) },
      uGrassB: { value: new THREE.Vector3(0.44, 0.62, 0.33) },
      uDirt: { value: new THREE.Vector3(0.58, 0.48, 0.35) },
      uHorizonFog: { value: new THREE.Vector3(0.81, 0.89, 0.87) },
      uFogDensity: { value: 0.028 },
    },
    vertexShader: /* glsl */ `
      uniform float uTime;
      varying vec2 vWorldXZ;
      varying float vWind;
      varying float vEdge;
      ${NOISE_GLSL}

      void main() {
        vec4 wp = instanceMatrix * vec4(position, 1.0);
        vec3 pos = wp.xyz;

        vec2 local = position.xz + 0.5;
        float edge = min(min(local.x, 1.0 - local.x), min(local.y, 1.0 - local.y));
        float grassTuft = smoothstep(0.0, 0.14, edge);

        float wind = snoise(floor(pos.xz) * 0.35 + uTime * 0.35);
        pos.y += wind * 0.06 * grassTuft;
        pos.y += snoise(pos.xz * 0.8 + uTime * 0.2) * 0.02;

        vWorldXZ = pos.xz;
        vWind = wind;
        vEdge = grassTuft;

        vec4 mv = viewMatrix * vec4(pos, 1.0);
        gl_Position = projectionMatrix * mv;
      }
    `,
    fragmentShader: /* glsl */ `
      uniform float uTime;
      uniform vec2 uCloudOffset;
      uniform float uCloudScale;
      uniform vec3 uGrassA;
      uniform vec3 uGrassB;
      uniform vec3 uDirt;
      uniform vec3 uHorizonFog;
      uniform float uFogDensity;
      varying vec2 vWorldXZ;
      varying float vWind;
      varying float vEdge;
      ${NOISE_GLSL}

      void main() {
        vec2 cell = floor(vWorldXZ);
        float patch = snoise(cell * 0.17 + 3.1);
        vec3 base = mix(uGrassB, uGrassA, smoothstep(-0.2, 0.6, patch));
        base = mix(base, uDirt, (1.0 - vEdge) * 0.55);

        vec2 cuv = vWorldXZ * uCloudScale + uCloudOffset;
        float cloud = toonCloud(cuv + uTime * 0.006);
        float shadow = mix(1.0, 0.72, cloud * 0.85);
        base *= shadow;

        float dist = length(vWorldXZ);
        float fog = 1.0 - exp(-dist * uFogDensity);
        base = mix(base, uHorizonFog, fog * 0.92);

        gl_FragColor = vec4(base, 1.0);
      }
    `,
  });
}

/** Instanced toon blocks for climbable obstacles (simple cel bands). */
export function createBlockMaterial(): THREE.MeshToonMaterial {
  const data = new Uint8Array([80, 140, 200, 255]);
  const tex = new THREE.DataTexture(data, 4, 1, THREE.RedFormat);
  tex.minFilter = THREE.NearestFilter;
  tex.magFilter = THREE.NearestFilter;
  tex.needsUpdate = true;
  return new THREE.MeshToonMaterial({
    color: new THREE.Color("#caa472"),
    gradientMap: tex,
  });
}
