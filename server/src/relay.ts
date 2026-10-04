import { createServer, type Socket } from "node:net";
import { slog } from "./logger.js";

/**
 * Relay del multijugador del cliente nativo (biblia §15). Es el MISMO hub que
 * `native/crates/ha-game/src/red.rs::Hub`, pero siempre prendido y con SALAS:
 * JSON por línea sobre TCP. No simula nada — asigna ids, guarda el último
 * estado de cada jugador y el ESTADO DEL MUNDO de cada sala (clave → último
 * `ed`) para el que llega tarde, nombra árbitro al más viejo y reenvía.
 *
 * Mensajes (protocolo v2, ver red.rs): hola / id / arb / ent / est / chat /
 * ed / listo / ev / chau / error / lista / salas.
 */
export const PROTOCOLO = 2;
const MAX_JUGADORES = 8;
const MAX_LINEA = 8192;
const MAX_CLAVES_MUNDO = 200_000;
const MAX_SALAS = 256;
/** Una sala vacía guarda su mundo este tiempo por si vuelven (se cayó el wifi). */
const SALA_VACIA_MS = 30 * 60_000;

interface Par {
  sock: Socket;
  nombre: string;
  piel: string;
  ultimo?: Record<string, unknown>;
}

interface Sala {
  nombre: string;
  semilla: number;
  pares: Map<number, Par>;
  mundo: Map<string, Record<string, unknown>>;
  arbitro: number;
  vaciaDesde: number;
}

const limpiar = (s: unknown, max: number) =>
  String(s ?? "").replace(/[\u0000-\u001f\u007f]/g, "").slice(0, max);

export function startRelay(port: number, host: string) {
  const salas = new Map<string, Sala>();
  let siguiente = 1;

  const enviar = (s: Socket, m: unknown) => {
    if (!s.destroyed) s.write(JSON.stringify(m) + "\n");
  };
  const difundir = (sala: Sala, m: unknown, salvo?: number) => {
    for (const [id, p] of sala.pares) if (id !== salvo) enviar(p.sock, m);
  };
  const info = (s: Sala) => ({
    sala: s.nombre,
    semilla: s.semilla,
    jugadores: s.pares.size,
    anfitrion: s.pares.get(s.arbitro)?.nombre ?? "",
  });
  const barrer = () => {
    const ahora = Date.now();
    for (const [k, s] of salas) if (s.pares.size === 0 && ahora - s.vaciaDesde > SALA_VACIA_MS) salas.delete(k);
  };
  const quitar = (sala: Sala, id: number) => {
    if (!sala.pares.delete(id)) return;
    difundir(sala, { t: "chau", id });
    if (sala.arbitro === id) {
      sala.arbitro = sala.pares.size ? Math.min(...sala.pares.keys()) : 0;
      if (sala.arbitro) difundir(sala, { t: "arb", id: sala.arbitro });
    }
    if (sala.pares.size === 0) sala.vaciaDesde = Date.now();
    slog("relay", "salió", { sala: sala.nombre, id, quedan: sala.pares.size });
  };

  const server = createServer((sock) => {
    sock.setNoDelay(true);
    sock.setEncoding("utf8");
    sock.setTimeout(30_000, () => sock.destroy());
    let miId: number | null = null;
    let sala: Sala | null = null;
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
        if (m.t === "lista" && miId === null) {
          barrer();
          const abiertas = [...salas.values()].filter((s) => s.pares.size > 0).map(info);
          enviar(sock, { t: "salas", salas: abiertas });
        } else if (m.t === "hola" && miId === null) {
          if (Number(m.ver) !== PROTOCOLO) {
            enviar(sock, { t: "error", msg: `versión distinta (sala v${PROTOCOLO}, tu juego v${Number(m.ver) || 1}): actualizá` });
            return sock.end();
          }
          barrer();
          const nombreSala = limpiar(m.sala, 32);
          let s = salas.get(nombreSala);
          if (!s) {
            if (salas.size >= MAX_SALAS) {
              enviar(sock, { t: "error", msg: "el servidor está lleno" });
              return sock.end();
            }
            s = { nombre: nombreSala, semilla: 0, pares: new Map(), mundo: new Map(), arbitro: 0, vaciaDesde: Date.now() };
            salas.set(nombreSala, s);
          }
          if (s.pares.size >= MAX_JUGADORES) {
            enviar(sock, { t: "error", msg: `sala llena (${MAX_JUGADORES})` });
            return sock.end();
          }
          // Sala sin mundo: el primero que entra pone la semilla (0 = sortear).
          if (s.semilla === 0) {
            const pedida = Number(m.semilla) >>> 0;
            s.semilla = pedida || ((Date.now() % 99999) | 1);
          }
          const id = siguiente++;
          if (!s.arbitro) s.arbitro = id;
          const nombre = limpiar(m.nombre, 24);
          const piel = limpiar(m.piel, 40);
          enviar(sock, { t: "id", id, semilla: s.semilla, arbitro: s.arbitro });
          for (const [pid, p] of s.pares) {
            enviar(sock, { t: "ent", id: pid, nombre: p.nombre, piel: p.piel });
            if (p.ultimo) enviar(sock, p.ultimo);
          }
          for (const ed of s.mundo.values()) enviar(sock, ed);
          enviar(sock, { t: "listo" });
          difundir(s, { t: "ent", id, nombre, piel });
          s.pares.set(id, { sock, nombre, piel });
          miId = id;
          sala = s;
          slog("relay", "entró", { sala: s.nombre, id, nombre, total: s.pares.size, semilla: s.semilla });
        } else if (miId !== null && sala) {
          if (m.t === "est") {
            const x = Number(m.x), z = Number(m.z), rumbo = Number(m.rumbo);
            if (![x, z, rumbo].every(Number.isFinite)) continue;
            const est = { t: "est", id: miId, x, z, cuerpo: Number(m.cuerpo) & 255, rumbo, vuelo: !!m.vuelo, pts: Number(m.pts) >>> 0, baj: Number(m.baj) >>> 0, veh: typeof m.veh === "string" ? limpiar(m.veh, 40) : "" };
            const p = sala.pares.get(miId);
            if (p) p.ultimo = est;
            difundir(sala, est, miId);
          } else if (m.t === "chat") {
            difundir(sala, { t: "chat", id: miId, msg: limpiar(m.msg, 140) });
          } else if (m.t === "ed") {
            const k = typeof m.k === "string" ? m.k : "";
            if (!k || k.length > 96) continue;
            if (!sala.mundo.has(k) && sala.mundo.size >= MAX_CLAVES_MUNDO) continue;
            const ed = { t: "ed", k, v: m.v ?? null };
            sala.mundo.set(k, ed);
            difundir(sala, ed, miId);
          } else if (m.t === "ev") {
            const para = Number(m.para) >>> 0;
            const ev = { t: "ev", de: miId, para, k: limpiar(m.k, 24), v: m.v ?? null };
            if (para === 0) difundir(sala, ev, miId);
            else {
              const p = sala.pares.get(para);
              if (p) enviar(p.sock, ev);
            }
          }
        }
      }
    });
    sock.on("close", () => {
      if (miId !== null && sala) quitar(sala, miId);
    });
    sock.on("error", () => sock.destroy());
  });
  server.listen(port, host, () => slog("boot", "relay multijugador escuchando", { host, port, protocolo: PROTOCOLO }));
  return server;
}
