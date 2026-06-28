import type { AssetListResponse } from "./types";

export async function fetchAssets(): Promise<AssetListResponse> {
  const res = await fetch("/api/assets");
  if (!res.ok) throw new Error(`scan failed: ${res.status}`);
  return res.json();
}

export async function assignKindOverride(kind: string, rel: string): Promise<void> {
  const res = await fetch("/api/assign-kind", {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({ kind, path: rel }),
  });
  if (!res.ok) {
    const text = await res.text();
    throw new Error(text || `assign failed: ${res.status}`);
  }
}

export function fileUrl(rel: string): string {
  return `/api/file?path=${encodeURIComponent(rel)}`;
}

export async function openInBlender(rel: string): Promise<void> {
  const res = await fetch(`/api/open-blender?path=${encodeURIComponent(rel)}`, {
    method: "POST",
  });
  if (!res.ok) {
    const text = await res.text();
    throw new Error(text || `open failed: ${res.status}`);
  }
}

export function formatBytes(n: number): string {
  if (n < 1024) return `${n} B`;
  if (n < 1024 * 1024) return `${(n / 1024).toFixed(1)} KB`;
  return `${(n / (1024 * 1024)).toFixed(2)} MB`;
}

export function formatDate(ms: number): string {
  return new Date(ms).toLocaleString();
}
