import { useEffect, useState } from "react";
import { updatesManifestUrl } from "./theme";
import type { UpdateManifest } from "@hunger-arena/shared";
import {
  APK_VERSION,
  IOS_VERSION,
  LINUX_VERSION,
  resolveServerHttpUrl,
  STATIC_DOWNLOADS,
} from "./config";

export type DownloadPlatform = "android" | "ios" | "linux";

export interface DownloadOption {
  platform: DownloadPlatform;
  label: string;
  icon: string;
  version: string | null;
  /** Same-origin URL — always /downloads/... */
  url: string | null;
  /** Filename saved on device */
  filename: string | null;
  sizeBytes: number | null;
  available: boolean;
  status: "ready" | "soon" | "unavailable";
  notes?: string;
}

async function probeStaticFile(
  url: string,
  minBytes: number
): Promise<{ ok: boolean; sizeBytes: number | null }> {
  try {
    const res = await fetch(url, { method: "HEAD" });
    if (!res.ok) return { ok: false, sizeBytes: null };
    const len = Number(res.headers.get("content-length") ?? 0);
    if (len >= minBytes) return { ok: true, sizeBytes: len };
    return { ok: false, sizeBytes: len > 0 ? len : null };
  } catch {
    return { ok: false, sizeBytes: null };
  }
}

function versionForPlatform(
  platform: DownloadPlatform,
  manifest?: UpdateManifest
): string | null {
  if (platform === "android") {
    return manifest?.android?.version ?? APK_VERSION;
  }
  if (platform === "ios") {
    return IOS_VERSION;
  }
  return manifest?.desktop?.version ?? LINUX_VERSION;
}

async function resolvePlatform(
  platform: DownloadPlatform,
  manifest?: UpdateManifest
): Promise<DownloadOption> {
  const meta = {
    android: { label: "Android", icon: "🤖", key: "android" as const },
    ios: { label: "iOS", icon: "🍎", key: "ios" as const },
    linux: { label: "Linux", icon: "🐧", key: "linux" as const },
  }[platform];

  const spec = STATIC_DOWNLOADS[meta.key];
  const { ok, sizeBytes } = await probeStaticFile(spec.url, spec.minBytes);

  const manifestRelease =
    platform === "android"
      ? manifest?.android
      : platform === "linux"
        ? manifest?.desktop
        : undefined;

  if (ok) {
    return {
      platform,
      label: meta.label,
      icon: meta.icon,
      version: versionForPlatform(platform, manifest),
      url: spec.url,
      filename: spec.filename,
      sizeBytes: sizeBytes ?? manifestRelease?.sizeBytes ?? null,
      available: true,
      status: "ready",
    };
  }

  const unavailableNotes: Record<DownloadPlatform, string> = {
    android: "APK no empaquetado — npm run ship:android",
    ios: "IPA no publicado — build iOS + sync a public/downloads/",
    linux: "AppImage no publicado — npm run build:desktop",
  };

  return {
    platform,
    label: meta.label,
    icon: meta.icon,
    version: null,
    url: null,
    filename: null,
    sizeBytes: null,
    available: false,
    status: platform === "ios" && !ok ? "soon" : "unavailable",
    notes: unavailableNotes[platform],
  };
}

export function formatFileSize(bytes: number | null): string | null {
  if (!bytes || bytes <= 0) return null;
  if (bytes >= 1_000_000) return `${(bytes / 1_000_000).toFixed(0)} MB`;
  return `${Math.round(bytes / 1000)} KB`;
}

export function useDownloadOptions() {
  const [options, setOptions] = useState<DownloadOption[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [serverOnline, setServerOnline] = useState<boolean | null>(null);
  const [serverUrl, setServerUrl] = useState("");

  useEffect(() => {
    let cancelled = false;

    (async () => {
      const base = resolveServerHttpUrl();
      if (!cancelled) setServerUrl(base);

      let manifest: UpdateManifest | undefined;
      try {
        const health = await fetch(`${base}/updates/health`);
        if (!cancelled) setServerOnline(health.ok);
      } catch {
        if (!cancelled) setServerOnline(false);
      }

      try {
        const res = await fetch(updatesManifestUrl(base));
        if (res.ok) manifest = await res.json();
      } catch (e) {
        if (!cancelled) setError(e instanceof Error ? e.message : String(e));
      }

      const [android, ios, linux] = await Promise.all([
        resolvePlatform("android", manifest),
        resolvePlatform("ios", manifest),
        resolvePlatform("linux", manifest),
      ]);

      if (!cancelled) {
        setOptions([android, ios, linux]);
        if (android.available || ios.available || linux.available) {
          setError(null);
        }
      }
      if (!cancelled) setLoading(false);
    })();

    return () => {
      cancelled = true;
    };
  }, []);

  return { options, loading, error, serverOnline, serverUrl };
}
