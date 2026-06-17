/**
 * APK download URL served from the hosted landing page.
 * Place your built APK at client/public/downloads/hunger-arena.apk
 * or override at build time:
 *   VITE_APK_URL=https://tu-dominio.com/hunger-arena.apk npm run build
 */
export const APK_URL =
  import.meta.env.VITE_APK_URL ?? "/downloads/hunger-arena.apk";

export const APK_VERSION = import.meta.env.VITE_APK_VERSION ?? "1.0.0";
