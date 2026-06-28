import { describe, expect, it } from "vitest";
import {
  assembleSpaceModule,
  canAssembleSpaceModule,
  createSpaceStationState,
  SPACE_STATION_MODULE_COUNT,
} from "./space_base.js";

describe("space station assembly", () => {
  it("modules assemble in order", () => {
    const station = createSpaceStationState();
    const inv: Record<string, number> = {
      wood: 20,
      sticks: 10,
      stone: 20,
      iron_ingot: 20,
      gold_ingot: 5,
    };
    expect(canAssembleSpaceModule(station, 1, inv)).toBe(false);
    expect(assembleSpaceModule(station, 0, inv)).toBe(true);
    expect(assembleSpaceModule(station, 1, inv)).toBe(true);
    expect(assembleSpaceModule(station, 2, inv)).toBe(true);
    expect(station.ready).toBe(true);
    expect(station.assembledModules).toBe(SPACE_STATION_MODULE_COUNT);
  });
});
