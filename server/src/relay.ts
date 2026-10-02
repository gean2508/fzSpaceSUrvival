import { createServer, type Socket } from "node:net";
import { slog } from "./logger.js";

/**
 * Relay del multijugador del cliente nativo (biblia §15). Es el MISMO hub que
 * `native/crates/ha-game/src/red.rs::Hub`, pero siempre prendido: JSON por línea
 * sobre TCP. No simula nada — asigna ids, guarda el último estado de cada uno
 * para el que llega tarde y reenvía. Cada cliente corre su mundo con la semilla
 * de la sala; esto sólo transporta quién está, dónde y qué dice.
 *
 * Mensajes: hola / id / ent / est / chat / chau (ver red.rs).
 */
const MAX_JUGADORES = 8;
const MAX_LINEA = 2048;

interface Par {
  sock: Socket;
  nombre: string;
  piel: string;
  ultimo?: Record<string, unknown>;
}

const limpiar = (s: unknown, max: number) =>
  String(s ?? "").replace(/[\u0000-\u001f\u007f]/g, "").slice(0, max);

export function startRelay(port: number, host: string) {
  const pares = new Map<number, Par>();
  let siguiente = 1;
  let semilla = 0;

  const enviar = (s: Socket, m: unknown) => {
    if (!s.destroyed) s.write(JSON.stringify(m) + "\n");
  };
  const difundir = (m: unknown, salvo?: number) => {
    for (const [id, p] of pares) if (id !== salvo) enviar(p.sock, m);
  };
  const quitar = (id: number) => {
    if (!pares.delete(id)) return;
    difundir({ t: "chau", id });
    if (pares.size === 0) semilla = 0; // sala vacía: la próxima elige mundo
    slog("relay", "salió", { id, quedan: pares.size });
  };

  const server = createServer((sock) => {
    sock.setNoDelay(true);
    sock.setEncoding("utf8");
    sock.setTimeout(30_000, () => sock.destroy());
    let miId: number | null = null;
    let buf = "";
    sock.on("data", (chunk: string) => {
      buf += chunk;
      if (buf.length > MAX_LINEA * 4) return sock.destroy();
      let nl: number;
      while ((nl = buf.indexOf("\n")) >= 0) {
        const linea = buf.slice(0, nl);
        buf = buf.slice(nl + 1);
        if (linea.length > MAX_LINEA) return sock.destroy();
        let m: Record<string, unknown>;
        try {
          m = JSON.parse(linea);
        } catch {
          continue;
        }
        if (m.t === "hola" && miId === null) {
          if (pares.size >= MAX_JUGADORES) return sock.destroy();
          if (pares.size === 0) {
            const s = Number(m.semilla) >>> 0;
            semilla = s || ((Date.now() % 99999) | 1);
          }
          const id = siguiente++;
          const nombre = limpiar(m.nombre, 24);
          const piel = limpiar(m.piel, 40);
          enviar(sock, { t: "id", id, semilla });
          for (const [pid, p] of pares) {
            enviar(sock, { t: "ent", id: pid, nombre: p.nombre, piel: p.piel });
            if (p.ultimo) enviar(sock, p.ultimo);
          }
          difundir({ t: "ent", id, nombre, piel });
          pares.set(id, { sock, nombre, piel });
          miId = id;
          slog("relay", "entró", { id, nombre, total: pares.size, semilla });
        } else if (m.t === "est" && miId !== null) {
          const x = Number(m.x), z = Number(m.z), rumbo = Number(m.rumbo);
          if (![x, z, rumbo].every(Number.isFinite)) continue;
          const est = { t: "est", id: miId, x, z, cuerpo: Number(m.cuerpo) & 255, rumbo, vuelo: !!m.vuelo, pts: Number(m.pts) >>> 0, baj: Number(m.baj) >>> 0 };
          const p = pares.get(miId);
          if (p) p.ultimo = est;
          difundir(est, miId);
        } else if (m.t === "chat" && miId !== null) {
          difundir({ t: "chat", id: miId, msg: limpiar(m.msg, 140) });
        }
      }
    });
    sock.on("close", () => miId !== null && quitar(miId));
    sock.on("error", () => sock.destroy());
  });
  server.listen(port, host, () => slog("boot", "relay multijugador escuchando", { host, port }));
  return server;
}
