import * as THREE from "three";

/**
 * Toon / cel-shading helpers for the Ghibli art pass.
 *
 * A gradient map is a tiny 1D texture that quantizes lighting into a few flat
 * bands, producing the hand-painted cel-shaded look instead of smooth PBR
 * shading. We generate it at runtime (no PNG assets to ship) and cache it.
 */

const gradientCache = new Map<number, THREE.DataTexture>();

/**
 * Build a stepped gradient map with `steps` luminance bands.
 * Lower step counts = chunkier, more stylized shading.
 */
export function makeToonGradient(steps = 4): THREE.DataTexture {
  const cached = gradientCache.get(steps);
  if (cached) return cached;

  const data = new Uint8Array(steps);
  for (let i = 0; i < steps; i++) {
    // Bias toward brighter bands so shadows stay soft and warm, not muddy.
    const t = i / (steps - 1);
    data[i] = Math.round(Math.pow(t, 0.8) * 255);
  }

  const tex = new THREE.DataTexture(data, steps, 1, THREE.RedFormat);
  tex.minFilter = THREE.NearestFilter;
  tex.magFilter = THREE.NearestFilter;
  tex.generateMipmaps = false;
  tex.needsUpdate = true;
  gradientCache.set(steps, tex);
  return tex;
}

/** Create a cel-shaded material with optional soft emissive rim. */
export function makeToonMaterial(
  color: THREE.ColorRepresentation,
  opts: { steps?: number; emissive?: THREE.ColorRepresentation; emissiveIntensity?: number } = {}
): THREE.MeshToonMaterial {
  const mat = new THREE.MeshToonMaterial({
    color,
    gradientMap: makeToonGradient(opts.steps ?? 4),
  });
  if (opts.emissive !== undefined) {
    mat.emissive = new THREE.Color(opts.emissive);
    mat.emissiveIntensity = opts.emissiveIntensity ?? 0.35;
  }
  return mat;
}

/**
 * Inverted-hull outline: a slightly larger back-face mesh behind the entity,
 * giving the clean ink-line silhouette typical of anime / Ghibli rendering.
 * Cheaper and more reliable on mobile than a postprocessing edge pass.
 */
export function makeOutline(
  geometry: THREE.BufferGeometry,
  thickness = 0.06,
  color: THREE.ColorRepresentation = 0x2b2f3a
): THREE.Mesh {
  const mat = new THREE.MeshBasicMaterial({ color, side: THREE.BackSide });
  const outline = new THREE.Mesh(geometry, mat);
  outline.scale.multiplyScalar(1 + thickness);
  outline.renderOrder = -1;
  return outline;
}

/** Dispose cached GPU textures (call on full teardown / HMR). */
export function disposeToonCache() {
  gradientCache.forEach((t) => t.dispose());
  gradientCache.clear();
}
