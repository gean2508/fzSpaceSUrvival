import AsyncStorage from "@react-native-async-storage/async-storage";

const DEVICE_KEY = "arena-device-id";

/** Stable per-device id used for guest play and progress linking. */
export async function getDeviceId(): Promise<string> {
  let id = await AsyncStorage.getItem(DEVICE_KEY);
  if (!id) {
    id = `mob-${Math.random().toString(36).slice(2)}-${Date.now()}`;
    await AsyncStorage.setItem(DEVICE_KEY, id);
  }
  return id;
}

/** Sync helper for hot paths that already resolved the id once. */
let cachedDeviceId: string | null = null;

export function getCachedDeviceId(): string {
  if (!cachedDeviceId) throw new Error("device id not ready — call ensureDeviceId() first");
  return cachedDeviceId;
}

export async function ensureDeviceId(): Promise<string> {
  if (!cachedDeviceId) cachedDeviceId = await getDeviceId();
  return cachedDeviceId;
}
