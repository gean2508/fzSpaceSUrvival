/**
 * Ghibli-inspired palette (warm, slightly desaturated, painterly).
 * Single source of truth for in-world colors so the look stays consistent.
 * Mirrors the CSS tokens in client/src/index.css.
 */
export const PALETTE = {
  // Sky / atmosphere (anime)
  skyZenith: "#3d6a9e",
  skyTop: "#8ec5e6", // legacy alias used by atmosphere + skill
  skyHorizon: "#f5d0c8",
  skyWarm: "#f4b8a8",
  skyNadir: "#8aab6e",
  sunset: "#f4a259",
  fog: "#cfe3df",

  // Ground / world
  meadow: "#9bc36b",
  meadowDeep: "#6f9e54",
  earth: "#caa472",
  bark: "#5a4632",
  water: "#7fc4c9",

  // Entities
  hero: "#5ec2d6", // local player
  rival: "#e07a7a", // remote players
  foe: "#d98a5a", // NPCs
  spark: "#ffd98a", // projectiles

  // UI accents
  cream: "#fff7e6",
  night: "#2b2f3a",
} as const;

export type PaletteKey = keyof typeof PALETTE;
