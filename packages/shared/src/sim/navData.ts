// Navigation data — probe missions + mapped bodies gating the autopilot.
// DESIGN TRUTH for constants and rules; `ha-shared/src/nav_data.rs` is the
// numeric mirror (rule: sim in shared, Rust mirrors, never fork values).
//
// Bodies are referenced by string id here — the arcade ephemeris (positions,
// distances) lives client-side in Rust; travel time takes distance as input.

/** Probes fly this factor over the crewed transit burn speed (no mass/life). */
export const PROBE_SPEED_FACTOR = 2.0;
/** Fixed surface-scan phase once the probe arrives (seconds). */
export const PROBE_SCAN_SECS = 20;
/** Data transmission back home: base + distance-scaled, capped (arcade). */
export const PROBE_TRANSMIT_BASE_SECS = 5;
export const PROBE_TRANSMIT_SECS_PER_M = 0.0001;
export const PROBE_TRANSMIT_CAP_SECS = 30;

/** Bodies known at the start of a save — everything else is earned. */
export const DEFAULT_MAPPED_BODIES: readonly string[] = ["earth"];

export type ProbePhase = "outbound" | "scanning" | "transmitting" | "delivered";

export interface ProbeMission {
  targetBody: string;
  launchedT: number;
  arriveT: number;
  distanceM: number;
  phase: ProbePhase;
}

export interface NavData {
  mapped: string[];
  probes: ProbeMission[];
}

export function defaultNavData(): NavData {
  return { mapped: [...DEFAULT_MAPPED_BODIES], probes: [] };
}

export function probeTravelSecs(distanceM: number, crewedBurnSpeed: number): number {
  return distanceM / Math.max(1, crewedBurnSpeed * PROBE_SPEED_FACTOR);
}

export function probeTransmitSecs(distanceM: number): number {
  return Math.min(
    PROBE_TRANSMIT_CAP_SECS,
    PROBE_TRANSMIT_BASE_SECS + distanceM * PROBE_TRANSMIT_SECS_PER_M,
  );
}

export function isMapped(nav: NavData, body: string): boolean {
  return nav.mapped.includes(body);
}

/** Returns true if newly mapped (false when already known). */
export function markMapped(nav: NavData, body: string): boolean {
  if (isMapped(nav, body)) return false;
  nav.mapped.push(body);
  return true;
}

/** Advance a probe against the shared clock; true when it just delivered. */
export function advanceProbe(probe: ProbeMission, t: number): boolean {
  const scannedT = probe.arriveT + PROBE_SCAN_SECS;
  const deliveredT = scannedT + probeTransmitSecs(probe.distanceM);
  const next: ProbePhase =
    t >= deliveredT ? "delivered" : t >= scannedT ? "transmitting" : t >= probe.arriveT ? "scanning" : "outbound";
  const deliveredNow = next === "delivered" && probe.phase !== "delivered";
  probe.phase = next;
  return deliveredNow;
}
