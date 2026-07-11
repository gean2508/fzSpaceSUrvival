import { describe, expect, it } from "vitest";

import {
  advanceProbe,
  defaultNavData,
  isMapped,
  markMapped,
  probeTransmitSecs,
  probeTravelSecs,
  PROBE_SCAN_SECS,
  PROBE_SPEED_FACTOR,
  PROBE_TRANSMIT_CAP_SECS,
  type ProbeMission,
} from "./navData.js";

describe("navData", () => {
  it("starts with only Earth mapped", () => {
    const nav = defaultNavData();
    expect(isMapped(nav, "earth")).toBe(true);
    expect(isMapped(nav, "moon")).toBe(false);
  });

  it("markMapped is idempotent", () => {
    const nav = defaultNavData();
    expect(markMapped(nav, "moon")).toBe(true);
    expect(markMapped(nav, "moon")).toBe(false);
    expect(nav.mapped.filter((b) => b === "moon")).toHaveLength(1);
  });

  it("probe travels at PROBE_SPEED_FACTOR over crewed speed", () => {
    expect(probeTravelSecs(1000, 10)).toBeCloseTo(1000 / (10 * PROBE_SPEED_FACTOR));
  });

  it("transmission delay is capped", () => {
    expect(probeTransmitSecs(10_000_000)).toBeLessThanOrEqual(PROBE_TRANSMIT_CAP_SECS);
    expect(probeTransmitSecs(0)).toBeGreaterThan(0);
  });

  it("probe lifecycle delivers exactly once", () => {
    const probe: ProbeMission = {
      targetBody: "moon",
      launchedT: 0,
      arriveT: 100,
      distanceM: 52_000,
      phase: "outbound",
    };
    expect(advanceProbe(probe, 50)).toBe(false);
    expect(probe.phase).toBe("outbound");
    expect(advanceProbe(probe, 105)).toBe(false);
    expect(probe.phase).toBe("scanning");
    const doneT = 100 + PROBE_SCAN_SECS + probeTransmitSecs(52_000) + 1;
    expect(advanceProbe(probe, doneT)).toBe(true);
    expect(probe.phase).toBe("delivered");
    expect(advanceProbe(probe, doneT + 5)).toBe(false);
  });
});
