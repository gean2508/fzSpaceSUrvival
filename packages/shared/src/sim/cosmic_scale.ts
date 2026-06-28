/**
 * SI scale — 1 world unit = 1 metre everywhere on planetary surfaces.
 * Orbital hub visuals compress distance only for rendering (see HUB_*).
 */

export const METERS_PER_UNIT = 1;

/** Average adult — collision + camera reference. */
export const HUMAN_HEIGHT_M = 1.75;
export const HUMAN_EYE_HEIGHT_M = 1.63;
export const PLAYER_COLLISION_RADIUS_M = 0.35;

/** Astronomical radii (m). */
export const EARTH_RADIUS_M = 6_371_000;
export const MOON_RADIUS_M = 1_737_400;
export const SUN_RADIUS_M = 696_000_000;

/** Mean centre-to-centre distance (m). */
export const EARTH_MOON_DISTANCE_M = 384_400_000;

/** Low Earth orbit — hub deck altitude above Earth surface (m). */
export const LEO_ALTITUDE_M = 408_000;

/** Moon / Earth radius ratio (~0.273). */
export const MOON_TO_EARTH_RADIUS_RATIO = MOON_RADIUS_M / EARTH_RADIUS_M;

/**
 * Hub scene uses compressed visual metres so the globe fits the deck (ratios stay real).
 * Surface gameplay (island, Luna walk) uses true metres.
 */
export const HUB_EARTH_VISUAL_RADIUS_M = 22;
export const HUB_EARTH_VISUAL_DISTANCE_M = 55;
export const HUB_MOON_VISUAL_RADIUS_M = HUB_EARTH_VISUAL_RADIUS_M * MOON_TO_EARTH_RADIUS_RATIO;

/** Walkable Luna sphere radius (arcade scale, metres). */
export const MOON_GAMEPLAY_RADIUS_M = 420;

/** Star dome in orbital void (m). */
export const SPACE_STARFIELD_RADIUS_M = 128;

/** Moon in LEO sky — compressed layout (ratios preserved). */
export const SPACE_MOON_SKY_X_M = 42;
export const SPACE_MOON_SKY_Y_M = 14;
export const SPACE_MOON_SKY_Z_M = -36;

/** Angular diameter helper — radians subtended by a sphere at distance. */
export function angularRadiusRad(bodyRadiusM: number, distanceM: number): number {
  if (distanceM <= bodyRadiusM) return Math.PI / 2;
  return Math.asin(Math.min(1, bodyRadiusM / distanceM));
}
