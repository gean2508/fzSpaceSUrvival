import { WORLD_HALF } from "./constants.js";

/** Island biomes — deterministic from height, moisture, and position. */
export type BiomeKind =
  | "meadow"
  | "forest"
  | "beach"
  | "highland"
  | "mesa"
  | "swamp"
  | "paramo"
  | "alpine";

/** Tallest landmark — NE cordillera (metres, human-scale 1u=1m). */
export const EVEREST = { x: 62, z: 68, peakHeight: 28, snowLine: 18 } as const;

export const MESA = { x: -42, z: -38, radius: 22, top: 8.5 } as const;

const RIVER_BED = { x: 0, z: -18, halfW: 4.5, depth: 1.2 } as const;

function hash2(x: number, z: number): number {
  const s = Math.sin(x * 127.1 + z * 311.7) * 43_758.5453;
  return s - Math.floor(s);
}

function smoothNoise(x: number, z: number): number {
  const ix = Math.floor(x);
  const iz = Math.floor(z);
  const fx = x - ix;
  const fz = z - iz;
  const ux = fx * fx * (3 - 2 * fx);
  const uz = fz * fz * (3 - 2 * fz);
  const a = hash2(ix, iz);
  const b = hash2(ix + 1, iz);
  const c = hash2(ix, iz + 1);
  const d = hash2(ix + 1, iz + 1);
  return a + (b - a) * ux + (c - a) * uz + (a - b - c + d) * ux * uz;
}

function fbm(x: number, z: number, octaves: number, lac = 2.0, gain = 0.5): number {
  let amp = 1;
  let freq = 1;
  let sum = 0;
  let norm = 0;
  for (let i = 0; i < octaves; i++) {
    sum += smoothNoise(x * freq, z * freq) * amp;
    norm += amp;
    amp *= gain;
    freq *= lac;
  }
  return sum / norm;
}

/** Ridged multifractal — sharp mountain ridges (industry-standard terrain trick). */
function ridgedNoise(x: number, z: number, octaves: number): number {
  let amp = 0.5;
  let freq = 1;
  let sum = 0;
  let weight = 1;
  for (let i = 0; i < octaves; i++) {
    let n = smoothNoise(x * freq, z * freq);
    n = 1 - Math.abs(n * 2 - 1);
    n *= n;
    n *= weight;
    weight = Math.min(1, Math.max(0, n * 2));
    sum += n * amp;
    amp *= 0.5;
    freq *= 2.1;
  }
  return sum;
}

function moistureAt(x: number, z: number): number {
  const coast = Math.hypot(x, z) / WORLD_HALF;
  const wet = fbm(x * 0.03 + 90, z * 0.03, 3);
  return wet * (1 - coast * 0.35);
}

function continentalHeight(x: number, z: number): number {
  const base = fbm(x * 0.012, z * 0.012, 4) * 4;
  const ridges = ridgedNoise(x * 0.035 + 12, z * 0.035, 4) * 10.5;
  const detail = fbm(x * 0.09, z * 0.09, 2) * 1.8;
  let h = base + ridges + detail;

  const dEverest = Math.hypot(x - EVEREST.x, z - EVEREST.z);
  const peak =
    EVEREST.peakHeight * Math.exp(-(dEverest * dEverest) / (2 * 17 * 17));
  h += peak;

  const dMesa = Math.hypot(x - MESA.x, z - MESA.z);
  if (dMesa < MESA.radius) {
    const edge = 1 - Math.pow(dMesa / MESA.radius, 2);
    h = Math.max(h, MESA.top * edge + h * (1 - edge * 0.65));
  }

  const rdx = Math.abs(x - RIVER_BED.x);
  if (rdx < RIVER_BED.halfW && z > -55 && z < 40) {
    const t = 1 - rdx / RIVER_BED.halfW;
    h -= RIVER_BED.depth * t * t;
  }

  return h;
}

/** Which biome covers world-space (x, z) — uses elevation + moisture, not just rectangles. */
export function biomeAt(x: number, z: number): BiomeKind {
  const dist = Math.hypot(x, z);
  const h = continentalHeight(x, z);
  const wet = moistureAt(x, z);

  const dEverest = Math.hypot(x - EVEREST.x, z - EVEREST.z);
  if (dEverest < 24 && h > EVEREST.snowLine - 2) return "alpine";

  const dMesa = Math.hypot(x - MESA.x, z - MESA.z);
  if (dMesa < MESA.radius + 2 && h > 6) return "mesa";

  if (dist > WORLD_HALF - 8) return "beach";

  if (h > 14 && wet < 0.42) return "paramo";
  if (h > 9) return "highland";
  if (h < 0.4 && wet > 0.62 && dist < WORLD_HALF - 14) return "swamp";
  if (wet > 0.52 && h < 6) return "forest";
  if (h < 2.5 && wet < 0.38 && dist < WORLD_HALF - 12) return "meadow";

  return h > 5 ? "highland" : "meadow";
}

/** Procedural ground elevation (metres). Single coherent height field. */
export function terrainHeightAt(x: number, z: number): number {
  let h = continentalHeight(x, z);
  const biome = biomeAt(x, z);
  const n = smoothNoise(x * 0.18, z * 0.16) * 0.35;

  switch (biome) {
    case "beach":
      h = Math.min(h, 0.6 + n * 0.2);
      break;
    case "meadow":
      h = h * 0.55 + 0.4 + n * 0.5;
      break;
    case "forest":
      h += n * 0.8;
      break;
    case "swamp":
      h = Math.min(h, 0.25 + n * 0.15);
      break;
    case "paramo":
      h += n * 0.4;
      break;
    default:
      h += n * 0.25;
  }

  const dist = Math.hypot(x, z);
  const coastTaper =
    biome !== "alpine" && biome !== "highland" && biome !== "paramo" && biome !== "mesa";
  if (coastTaper && dist > WORLD_HALF - 14) {
    const t = (dist - (WORLD_HALF - 14)) / 14;
    h *= Math.max(0.12, 1 - t * 0.82);
  }

  return Math.max(-0.35, Math.round(h * 100) / 100);
}

export function terrainSlopeAt(x: number, z: number, step = 1.0): number {
  const h0 = terrainHeightAt(x, z);
  const hx = terrainHeightAt(x + step, z) - h0;
  const hz = terrainHeightAt(x, z + step) - h0;
  return Math.hypot(hx, hz) / step;
}

/** Bilinear height on the terrain mesh lattice — matches rendered triangle interpolation. */
export function terrainSurfaceHeightAt(
  x: number,
  z: number,
  gridStep = 1.25
): number {
  const step = Math.max(0.5, gridStep);
  const fx = x / step;
  const fz = z / step;
  const ix0 = Math.floor(fx);
  const iz0 = Math.floor(fz);
  const tx = fx - ix0;
  const tz = fz - iz0;
  const x0 = ix0 * step;
  const z0 = iz0 * step;
  const h00 = terrainHeightAt(x0, z0);
  const h10 = terrainHeightAt(x0 + step, z0);
  const h01 = terrainHeightAt(x0, z0 + step);
  const h11 = terrainHeightAt(x0 + step, z0 + step);
  const hLow = h00 + (h10 - h00) * tx;
  const hHigh = h01 + (h11 - h01) * tx;
  return hLow + (hHigh - hLow) * tz;
}

/** Steep wet slopes — spawn waterfall visuals. */
export function isWaterfallAt(x: number, z: number): boolean {
  const slope = terrainSlopeAt(x, z, 0.8);
  const h = terrainHeightAt(x, z);
  const wet = moistureAt(x, z);
  return slope > 0.55 && h > 3.5 && wet > 0.48 && h < EVEREST.snowLine;
}

export function isEverestSnow(x: number, z: number): boolean {
  const d = Math.hypot(x - EVEREST.x, z - EVEREST.z);
  return d < 16 && terrainHeightAt(x, z) >= EVEREST.snowLine;
}
