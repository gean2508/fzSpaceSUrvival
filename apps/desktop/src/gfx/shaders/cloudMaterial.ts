import * as THREE from "three";
import { NOISE_GLSL } from "./noise.glsl";

/** Procedural cel-shaded cloud planes drifting in world space. */
export function createCloudMaterial(): THREE.ShaderMaterial {
  return new THREE.ShaderMaterial({
    transparent: true,
    depthWrite: false,
    side: THREE.DoubleSide,
    uniforms: {
      uTime: { value: 0 },
      uOffset: { value: new THREE.Vector2(0, 0) },
      uCloudColor: { value: new THREE.Vector3(1, 0.97, 0.92) },
      uShadowColor: { value: new THREE.Vector3(0.82, 0.86, 0.92) },
      uScale: { value: 0.018 },
      uSeed: { value: 0 },
    },
    vertexShader: /* glsl */ `
      varying vec2 vUv;
      varying vec3 vWorldPos;
      void main() {
        vUv = uv;
        vec4 wp = modelMatrix * vec4(position, 1.0);
        vWorldPos = wp.xyz;
        gl_Position = projectionMatrix * viewMatrix * wp;
      }
    `,
    fragmentShader: /* glsl */ `
      uniform float uTime;
      uniform vec2 uOffset;
      uniform vec3 uCloudColor;
      uniform vec3 uShadowColor;
      uniform float uScale;
      uniform float uSeed;
      varying vec2 vUv;
      varying vec3 vWorldPos;
      ${NOISE_GLSL}

      void main() {
        vec2 uv = vWorldPos.xz * uScale + uOffset + vec2(uSeed * 13.7, uSeed * 7.3);
        float density = toonCloud(uv + uTime * 0.008);
        float rim = smoothstep(0.55, 0.85, density) - smoothstep(0.85, 1.0, density);
        vec3 col = mix(uShadowColor, uCloudColor, density);
        col += rim * 0.25;
        float alpha = density * 0.82;
        if (alpha < 0.04) discard;
        gl_FragColor = vec4(col, alpha);
      }
    `,
  });
}
