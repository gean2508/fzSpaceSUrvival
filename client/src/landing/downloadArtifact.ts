/**
 * Same-origin APK download — direct URL, no fetch/buffer (75MB OOMs mobile Chrome).
 * Vite plugin sets Content-Disposition + Content-Length on /downloads/*.
 */
export function triggerFileDownload(url: string, filename: string): void {
  const sameOrigin = new URL(url, window.location.href).origin === window.location.origin;
  if (!sameOrigin) {
    throw new Error("download_cross_origin");
  }

  const a = document.createElement("a");
  a.href = url;
  a.download = filename;
  a.rel = "noopener";
  a.style.display = "none";
  document.body.appendChild(a);
  a.click();
  a.remove();
}

export function downloadErrorMessage(err: unknown): string {
  const msg = err instanceof Error ? err.message : String(err);
  if (msg.startsWith("download_")) return "No se pudo descargar el archivo.";
  return msg;
}
