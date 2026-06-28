/**
 * Landing page config — static download paths (same-origin, pre-built artifacts).
 */

const port = import.meta.env.VITE_SERVER_PORT ?? "2567";

function envHost(): string {
  return import.meta.env.VITE_SERVER_HOST ?? "localhost";
}

/** Resolve API base for manifest metadata only — downloads always use /downloads/*. */
export function resolveServerHttpUrl(): string {
  if (import.meta.env.VITE_SERVER_HTTP_URL) {
    return import.meta.env.VITE_SERVER_HTTP_URL.replace(/\/$/, "");
  }
  if (typeof window !== "undefined") {
    const pageHost = window.location.hostname;
    if (pageHost && pageHost !== "localhost" && pageHost !== "127.0.0.1") {
      return `http://${pageHost}:${port}`;
    }
  }
  return `http://${envHost()}:${port}`;
}

/** Pre-built binaries in Vite `public/downloads/` — copied at publish time. */
export const STATIC_DOWNLOADS = {
  android: {
    url: "/downloads/hunger-arena.apk",
    filename: "hunger-arena.apk",
    minBytes: 5_000_000,
  },
  ios: {
    url: "/downloads/hunger-arena.ipa",
    filename: "hunger-arena.ipa",
    minBytes: 1_000_000,
  },
  linux: {
    url: "/downloads/hunger-arena.AppImage",
    filename: "hunger-arena.AppImage",
    minBytes: 1_000_000,
  },
} as const;

export const APK_VERSION = import.meta.env.VITE_APK_VERSION ?? "1.0.0";
export const IOS_VERSION = import.meta.env.VITE_IOS_VERSION ?? APK_VERSION;
export const LINUX_VERSION = import.meta.env.VITE_LINUX_VERSION ?? "0.1.0";
