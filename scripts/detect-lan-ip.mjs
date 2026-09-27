#!/usr/bin/env node
/**
 * **La IP de ESTA PC en la red de casa**, la que se hornea en el APK
 * (`VITE_SERVER_HOST` / `HA_SERVER_HOST` / `UPDATE_BASE_URL`).
 *
 * Antes devolvía "la primera IPv4 que no sea interna", y ese orden **no lo
 * decide nadie**: `os.networkInterfaces()` las lista como se las da el sistema.
 * En esta PC conviven la WiFi (192.168.1.x) y Tailscale (100.83.x.x), así que
 * según el día el APK salía horneado con la IP del tailnet — y entonces el celu
 * en la WiFi de casa no encontraba el servidor. Ése es el "no sirve el servidor
 * local con la IP que usábamos": el servidor estaba bien, la IP horneada no.
 *
 * Ahora hay un orden explícito:
 *
 *   1. **LAN de verdad** — 192.168.x.x, 10.x.x.x, 172.16–31.x.x.
 *   2. **Tailscale** (100.64.0.0/10, el rango CGNAT del tailnet) sólo si no hay
 *      LAN: sirve para el celu en el tailnet y es mejor que nada.
 *   3. `127.0.0.1` si la máquina está sin red.
 *
 * Y se saltean las interfaces virtuales (docker, puentes, veth, VPN de trabajo):
 * ninguna de esas direcciones es alcanzable desde el teléfono.
 *
 * `--tailscale` imprime la del tailnet (vacío si no hay), que es lo que usa el
 * banner de `play.mjs` para ofrecer las dos URLs.
 */
import os from "node:os";

/** Interfaces que NO son "la red de casa" aunque tengan IPv4. */
const VIRTUALES = /^(docker|br-|veth|virbr|vmnet|vboxnet|zt|wg|tun|utun)/i;

/** 100.64.0.0/10 — el rango CGNAT donde vive Tailscale. */
function esTailnet(ip) {
  const [a, b] = ip.split(".").map(Number);
  return a === 100 && b >= 64 && b <= 127;
}

function esLan(ip) {
  const [a, b] = ip.split(".").map(Number);
  return (
    a === 192 && b === 168 ||
    a === 10 ||
    (a === 172 && b >= 16 && b <= 31)
  );
}

const lan = [];
const tailnet = [];
const otras = [];
for (const [nombre, ifaces] of Object.entries(os.networkInterfaces())) {
  if (VIRTUALES.test(nombre)) continue;
  for (const iface of ifaces ?? []) {
    if (iface.family !== "IPv4" || iface.internal) continue;
    if (esTailnet(iface.address)) tailnet.push(iface.address);
    else if (esLan(iface.address)) lan.push(iface.address);
    else otras.push(iface.address);
  }
}

if (process.argv.includes("--tailscale")) {
  process.stdout.write(tailnet[0] ?? "");
} else {
  process.stdout.write(lan[0] ?? otras[0] ?? tailnet[0] ?? "127.0.0.1");
}
