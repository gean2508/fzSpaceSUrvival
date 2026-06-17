import { openDB, type DBSchema, type IDBPDatabase } from "idb";
import {
  awardFor,
  type MatchResult,
  type ProfileData,
  type ProgressDelta,
} from "@hunger-arena/shared";

interface StoredProfile {
  deviceId: string;
  profile: ProfileData;
  /** Offline-earned progress not yet merged by an online server. */
  unsynced: ProgressDelta;
}

interface ArenaDB extends DBSchema {
  profile: {
    key: string;
    value: StoredProfile;
  };
}

const DB_NAME = "hunger-arena";
const DB_VERSION = 1;

let dbPromise: Promise<IDBPDatabase<ArenaDB>> | null = null;

function db() {
  if (!dbPromise) {
    dbPromise = openDB<ArenaDB>(DB_NAME, DB_VERSION, {
      upgrade(d) {
        if (!d.objectStoreNames.contains("profile")) {
          d.createObjectStore("profile", { keyPath: "deviceId" });
        }
      },
    });
  }
  return dbPromise;
}

function blank(deviceId: string): StoredProfile {
  return {
    deviceId,
    profile: {
      deviceId,
      displayName: "Invitado",
      xp: 0,
      coins: 0,
      bestWave: 0,
      matches: 0,
      hasAccount: false,
    },
    unsynced: { xp: 0, coins: 0, bestWave: 0, matches: 0 },
  };
}

export async function loadLocal(deviceId: string): Promise<StoredProfile> {
  const d = await db();
  return (await d.get("profile", deviceId)) ?? blank(deviceId);
}

export async function saveServerProfile(profile: ProfileData): Promise<void> {
  const d = await db();
  const existing = (await d.get("profile", profile.deviceId)) ?? blank(profile.deviceId);
  existing.profile = profile;
  await d.put("profile", existing);
}

/** Apply a finished offline match: award progress locally + queue for sync. */
export async function recordOfflineMatch(
  deviceId: string,
  result: MatchResult
): Promise<ProfileData> {
  const d = await db();
  const stored = (await d.get("profile", deviceId)) ?? blank(deviceId);
  const { xp, coins } = awardFor(result);
  stored.profile = {
    ...stored.profile,
    xp: stored.profile.xp + xp,
    coins: stored.profile.coins + coins,
    bestWave: Math.max(stored.profile.bestWave, result.waves),
    matches: stored.profile.matches + 1,
  };
  stored.unsynced = {
    xp: stored.unsynced.xp + xp,
    coins: stored.unsynced.coins + coins,
    bestWave: Math.max(stored.unsynced.bestWave, result.waves),
    matches: stored.unsynced.matches + 1,
  };
  await d.put("profile", stored);
  return stored.profile;
}

export async function getUnsynced(deviceId: string): Promise<ProgressDelta> {
  return (await loadLocal(deviceId)).unsynced;
}

export async function clearUnsynced(deviceId: string): Promise<void> {
  const d = await db();
  const stored = (await d.get("profile", deviceId)) ?? blank(deviceId);
  stored.unsynced = { xp: 0, coins: 0, bestWave: 0, matches: 0 };
  await d.put("profile", stored);
}

export function hasUnsynced(delta: ProgressDelta): boolean {
  return delta.xp > 0 || delta.coins > 0 || delta.bestWave > 0 || delta.matches > 0;
}
