import AsyncStorage from "@react-native-async-storage/async-storage";
import {
  awardFor,
  type MatchResult,
  type ProfileData,
  type ProgressDelta,
} from "@hunger-arena/shared";

interface StoredProfile {
  deviceId: string;
  profile: ProfileData;
  unsynced: ProgressDelta;
}

const storageKey = (deviceId: string) => `arena-profile:${deviceId}`;

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
  const raw = await AsyncStorage.getItem(storageKey(deviceId));
  if (!raw) return blank(deviceId);
  try {
    return JSON.parse(raw) as StoredProfile;
  } catch {
    return blank(deviceId);
  }
}

async function saveStored(stored: StoredProfile): Promise<void> {
  await AsyncStorage.setItem(storageKey(stored.deviceId), JSON.stringify(stored));
}

export async function saveServerProfile(profile: ProfileData): Promise<void> {
  const existing = (await loadLocal(profile.deviceId)) ?? blank(profile.deviceId);
  existing.profile = profile;
  await saveStored(existing);
}

export async function recordOfflineMatch(
  deviceId: string,
  result: MatchResult
): Promise<ProfileData> {
  const stored = (await loadLocal(deviceId)) ?? blank(deviceId);
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
  await saveStored(stored);
  return stored.profile;
}

export async function getUnsynced(deviceId: string): Promise<ProgressDelta> {
  return (await loadLocal(deviceId)).unsynced;
}

export async function clearUnsynced(deviceId: string): Promise<void> {
  const stored = (await loadLocal(deviceId)) ?? blank(deviceId);
  stored.unsynced = { xp: 0, coins: 0, bestWave: 0, matches: 0 };
  await saveStored(stored);
}

export function hasUnsynced(delta: ProgressDelta): boolean {
  return delta.xp > 0 || delta.coins > 0 || delta.bestWave > 0 || delta.matches > 0;
}
