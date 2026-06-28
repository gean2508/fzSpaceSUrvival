/** Landing copy — fStyle (no client-core dependency). */
export const LANDING_COPY = {
  brand: "Starship",
  landingEyebrow: "AGENCIA ESPACIAL SURVIVAL",
  landingTagline: "Exploración 3D nativa. Isla, cuevas, máquinas — offline o LAN.",
  landingCta: "DESCARGAR APK",
  landingExplore: "VER BRIEFING",
  pressStart: "TAP PARA JUGAR",
} as const;

export const UPDATES_MANIFEST_PATH = "/updates/manifest";

export function updatesManifestUrl(httpBase: string): string {
  const base = httpBase.replace(/\/$/, "");
  return `${base}${UPDATES_MANIFEST_PATH}`;
}
