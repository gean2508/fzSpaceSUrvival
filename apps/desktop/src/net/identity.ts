const DEVICE_KEY = "arena-device-id";

/** Stable per-device id used for guest play and progress linking (Phase 6). */
export function getDeviceId(): string {
  if (typeof window === "undefined") return "server";
  let id = localStorage.getItem(DEVICE_KEY);
  if (!id) {
    id =
      typeof crypto !== "undefined" && "randomUUID" in crypto
        ? crypto.randomUUID()
        : `dev-${Math.random().toString(36).slice(2)}-${Date.now()}`;
    localStorage.setItem(DEVICE_KEY, id);
  }
  return id;
}
