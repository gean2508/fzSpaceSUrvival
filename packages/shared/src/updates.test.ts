import { describe, expect, it } from "vitest";
import {
  compareSemver,
  isUpdateAvailable,
  parseUpdateManifest,
} from "./updates.js";

describe("compareSemver", () => {
  it("orders numeric segments", () => {
    expect(compareSemver("1.0.0", "1.0.1")).toBeLessThan(0);
    expect(compareSemver("2.0.0", "1.9.9")).toBeGreaterThan(0);
    expect(compareSemver("1.2.3", "1.2.3")).toBe(0);
  });

  it("ignores prerelease suffix for comparison", () => {
    expect(compareSemver("1.0.0-beta", "1.0.0")).toBe(0);
  });
});

describe("isUpdateAvailable", () => {
  const release = {
    version: "1.1.0",
    url: "http://localhost/artifact.apk",
    mandatory: false,
    versionCode: 5,
  };

  it("detects newer semver", () => {
    const r = isUpdateAvailable("1.0.0", release);
    expect(r.available).toBe(true);
    expect(r.reason).toBe("newer_version");
  });

  it("uses versionCode when semver ties on Android", () => {
    const r = isUpdateAvailable("1.1.0", release, 4);
    expect(r.available).toBe(true);
    expect(r.reason).toBe("newer_build");
  });

  it("returns false when already current", () => {
    expect(isUpdateAvailable("1.1.0", release, 5).available).toBe(false);
    expect(isUpdateAvailable("2.0.0", release).available).toBe(false);
  });
});

describe("parseUpdateManifest", () => {
  it("validates manifest shape", () => {
    const m = parseUpdateManifest({
      publishedAt: new Date().toISOString(),
      android: {
        version: "1.0.1",
        versionCode: 2,
        url: "http://localhost/apk",
        mandatory: true,
      },
    });
    expect(m.android?.version).toBe("1.0.1");
    expect(m.android?.mandatory).toBe(true);
  });
});
