import { z } from "zod";

/** Per-platform release entry served by GET /updates/manifest */
export const PlatformReleaseSchema = z.object({
  version: z.string().regex(/^\d+\.\d+\.\d+(-[\w.]+)?$/),
  /** Android integer build — compared when semver ties. */
  versionCode: z.number().int().positive().optional(),
  url: z.string().url(),
  /** Lowercase hex SHA-256 of the artifact (recommended in prod). */
  sha256: z
    .string()
    .regex(/^[a-f0-9]{64}$/i)
    .optional(),
  mandatory: z.boolean().default(false),
  notes: z.string().max(2000).optional(),
  sizeBytes: z.number().int().positive().optional(),
});

export type PlatformRelease = z.infer<typeof PlatformReleaseSchema>;

export const UpdateManifestSchema = z.object({
  android: PlatformReleaseSchema.optional(),
  desktop: PlatformReleaseSchema.optional(),
  publishedAt: z.string(),
});

export type UpdateManifest = z.infer<typeof UpdateManifestSchema>;

export type UpdatePlatform = "android" | "desktop";

/** Parse manifest JSON from the update server. */
export function parseUpdateManifest(json: unknown): UpdateManifest {
  return UpdateManifestSchema.parse(json);
}

/** Semver compare: negative if a<b, 0 if equal, positive if a>b (numeric segments only). */
export function compareSemver(a: string, b: string): number {
  const strip = (v: string) => v.split("-")[0];
  const pa = strip(a).split(".").map((n) => parseInt(n, 10) || 0);
  const pb = strip(b).split(".").map((n) => parseInt(n, 10) || 0);
  const len = Math.max(pa.length, pb.length, 3);
  for (let i = 0; i < len; i++) {
    const da = pa[i] ?? 0;
    const db = pb[i] ?? 0;
    if (da !== db) return da > db ? 1 : -1;
  }
  return 0;
}

export interface UpdateCheckResult {
  available: boolean;
  release: PlatformRelease | null;
  reason?: "newer_version" | "newer_build";
}

/**
 * Returns true when the server offers a newer build than the installed app.
 * Uses semver first; on tie, compares Android versionCode when both provided.
 */
export function isUpdateAvailable(
  currentVersion: string,
  release: PlatformRelease,
  currentVersionCode?: number
): UpdateCheckResult {
  const cmp = compareSemver(currentVersion, release.version);
  if (cmp < 0) {
    return { available: true, release, reason: "newer_version" };
  }
  if (cmp > 0) {
    return { available: false, release: null };
  }
  if (
    release.versionCode != null &&
    currentVersionCode != null &&
    release.versionCode > currentVersionCode
  ) {
    return { available: true, release, reason: "newer_build" };
  }
  return { available: false, release: null };
}
