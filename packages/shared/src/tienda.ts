/**
 * **Tienda + Pase de batalla — el catálogo.**
 *
 * Una sola verdad para lo que se vende, a cuánto y qué da cada nivel del pase.
 * El servidor cobra con ESTO (nunca con un precio que mande el cliente) y el
 * juego dibuja la tienda con su espejo en `ha-shared/src/tienda.rs`; los dos se
 * vigilan con el módulo `tienda` de `catalog-parity`.
 *
 * Reglas de diseño (biblia §17):
 * - **Sólo cosmética.** Nada de lo que se compra pega más fuerte ni aguanta más:
 *   el camuflaje es la misma arma con otra piel, el aura es el mismo vuelo con
 *   otro color. Un sandbox de rol no se gana con la tarjeta.
 * - Dos monedas: **gemas** (se compran con plata, y algunas salen del pase) y
 *   **monedas** (se ganan jugando). Lo que vale monedas también se puede sacar
 *   sin poner un peso.
 * - El pase premium se paga con gemas, como en los juegos de referencia.
 */
import { z } from "zod";

/** Los ids de las armas, en el orden de `ha-game/src/armas.rs::ARMAS`. */
export const TIENDA_ARMAS = [
  "pistola",
  "thompson",
  "ppsh",
  "mp40",
  "suomi",
  "pps43",
  "ak",
  "escopeta",
  "raygun",
] as const;

/**
 * Los camuflajes que NO caen como botín: sólo se compran o se ganan en el pase.
 * Los seis del botín (115, singularidad…) siguen siendo del mundo.
 */
export const TIENDA_CAMOS = [
  { id: "oro", nombre: "Oro macizo", gemasPorArma: 120, gemasPack: 780 },
  { id: "neon", nombre: "Neón", gemasPorArma: 100, gemasPack: 650 },
  { id: "hielo", nombre: "Hielo eterno", gemasPorArma: 100, gemasPack: 650 },
  // Exclusivo del pase (temporada 1): no tiene precio.
  { id: "dragon", nombre: "Dragón de Ki", gemasPorArma: 0, gemasPack: 0 },
] as const;

/**
 * Las auras de Ki: tres tonos (afuera → núcleo), lo mismo que pinta
 * `creative_fly::sync_player_aura`. `saiyan` es la de siempre y es gratis.
 */
export const TIENDA_AURAS = [
  { id: "saiyan", nombre: "Saiyan dorado", gemas: 0, monedas: 0, tonos: [[1.0, 0.42, 0.06], [1.0, 0.66, 0.14], [1.0, 0.92, 0.62]] },
  { id: "dios_azul", nombre: "Dios azul", gemas: 300, monedas: 0, tonos: [[0.05, 0.35, 1.0], [0.2, 0.62, 1.0], [0.75, 0.95, 1.0]] },
  { id: "rosado", nombre: "Rosado", gemas: 300, monedas: 0, tonos: [[0.85, 0.08, 0.45], [1.0, 0.35, 0.7], [1.0, 0.82, 0.93]] },
  { id: "legendario", nombre: "Legendario", gemas: 0, monedas: 6000, tonos: [[0.1, 0.75, 0.15], [0.35, 1.0, 0.35], [0.85, 1.0, 0.7]] },
  { id: "carmesi", nombre: "Carmesí", gemas: 0, monedas: 4000, tonos: [[0.8, 0.04, 0.04], [1.0, 0.25, 0.12], [1.0, 0.7, 0.55]] },
  { id: "instinto", nombre: "Instinto plateado", gemas: 600, monedas: 0, tonos: [[0.55, 0.6, 0.75], [0.8, 0.85, 0.95], [1.0, 1.0, 1.0]] },
  // Exclusiva del pase (premium, nivel 15).
  { id: "ego", nombre: "Ego violeta", gemas: 0, monedas: 0, tonos: [[0.4, 0.05, 0.75], [0.62, 0.25, 1.0], [0.92, 0.75, 1.0]] },
] as const;

/** Los paquetes de gemas que se pagan con plata. `centavos` en dólares. */
export const TIENDA_GEMAS = [
  { id: "gemas_100", gemas: 100, extra: 0, centavos: 99 },
  { id: "gemas_520", gemas: 500, extra: 20, centavos: 499 },
  { id: "gemas_1080", gemas: 1000, extra: 80, centavos: 999 },
  { id: "gemas_2200", gemas: 2000, extra: 200, centavos: 1999 },
] as const;

export type TiendaTipo = "camo" | "pack" | "aura" | "pase";

export interface TiendaItem {
  /** `camo:<arma>:<camo>`, `pack:<camo>`, `aura:<id>`, `pase:<temporada>`. */
  id: string;
  tipo: TiendaTipo;
  nombre: string;
  /** 0 = no se vende por gemas. */
  gemas: number;
  /** 0 = no se vende por monedas. */
  monedas: number;
  /** Lo que entrega: un pack da los nueve camos de su color. */
  da: string[];
}

export const PASE_TEMPORADA = {
  id: "t1",
  nombre: "Temporada 1 · Despertar del Ki",
  /** Inclusive / exclusive, en UTC. */
  desde: "2026-10-01T00:00:00Z",
  hasta: "2027-01-01T00:00:00Z",
  niveles: 30,
  xpPorNivel: 1000,
  gemasPremium: 950,
} as const;

/** Cuánta XP da cada cosa que el juego informa, y el techo diario. */
export const PASE_XP = {
  meta: 120,
  baja: 15,
  minuto: 10,
  topeDiario: 3000,
} as const;

/** Una recompensa del pase: `monedas:N`, `gemas:N` o el id de un ítem. */
export type Recompensa = string;

/**
 * Qué da cada nivel, gratis y premium. Determinista y escrita a mano adentro de
 * una regla para que el espejo en Rust la reproduzca igual (ver
 * `tienda.rs::recompensa_pase`).
 */
export function recompensaPase(nivel: number): { gratis: Recompensa; premium: Recompensa } {
  const especiales: Record<number, { gratis: Recompensa; premium: Recompensa }> = {
    1: { gratis: "monedas:300", premium: "camo:pistola:dragon" },
    5: { gratis: "monedas:600", premium: "camo:thompson:dragon" },
    8: { gratis: "aura:carmesi", premium: "camo:mp40:dragon" },
    10: { gratis: "gemas:50", premium: "camo:ak:dragon" },
    12: { gratis: "monedas:800", premium: "camo:ppsh:dragon" },
    15: { gratis: "monedas:1000", premium: "aura:ego" },
    18: { gratis: "monedas:800", premium: "camo:suomi:dragon" },
    20: { gratis: "gemas:50", premium: "camo:pps43:dragon" },
    25: { gratis: "monedas:1500", premium: "camo:escopeta:dragon" },
    30: { gratis: "gemas:100", premium: "camo:raygun:dragon" },
  };
  const e = especiales[nivel];
  if (e) return e;
  // El resto: monedas en la gratuita y gemas cada tres niveles en la premium
  // (el pase premium devuelve más gemas de las que cuesta si se completa).
  const gratis = `monedas:${100 + 20 * nivel}`;
  const premium = nivel % 3 === 0 ? "gemas:100" : `monedas:${300 + 30 * nivel}`;
  return { gratis, premium };
}

function construirCatalogo(): TiendaItem[] {
  const out: TiendaItem[] = [];
  for (const c of TIENDA_CAMOS) {
    for (const a of TIENDA_ARMAS) {
      out.push({
        id: `camo:${a}:${c.id}`,
        tipo: "camo",
        nombre: c.nombre,
        gemas: c.gemasPorArma,
        monedas: 0,
        da: [`camo:${a}:${c.id}`],
      });
    }
    if (c.gemasPack > 0) {
      out.push({
        id: `pack:${c.id}`,
        tipo: "pack",
        nombre: `Arsenal ${c.nombre}`,
        gemas: c.gemasPack,
        monedas: 0,
        da: TIENDA_ARMAS.map((a) => `camo:${a}:${c.id}`),
      });
    }
  }
  for (const a of TIENDA_AURAS) {
    out.push({ id: `aura:${a.id}`, tipo: "aura", nombre: a.nombre, gemas: a.gemas, monedas: a.monedas, da: [`aura:${a.id}`] });
  }
  out.push({
    id: `pase:${PASE_TEMPORADA.id}`,
    tipo: "pase",
    nombre: `Pase premium · ${PASE_TEMPORADA.nombre}`,
    gemas: PASE_TEMPORADA.gemasPremium,
    monedas: 0,
    da: [`pase:${PASE_TEMPORADA.id}`],
  });
  return out;
}

export const TIENDA_CATALOGO: readonly TiendaItem[] = construirCatalogo();

export function itemTienda(id: string): TiendaItem | undefined {
  return TIENDA_CATALOGO.find((i) => i.id === id);
}

/** Lo que se tiene desde el primer día, sin comprar nada. */
export const TIENDA_DE_REGALO = ["aura:saiyan"] as const;

/** Nivel del pase para una XP dada (1..niveles; 0 = todavía nada). */
export function nivelPase(xp: number): number {
  return Math.min(PASE_TEMPORADA.niveles, Math.floor(Math.max(0, xp) / PASE_TEMPORADA.xpPorNivel));
}

// ── Mensajes HTTP ───────────────────────────────────────────────────────────

const DeviceId = z.string().min(8).max(64).regex(/^[A-Za-z0-9_-]+$/);

export const TiendaComprarSchema = z.object({ deviceId: DeviceId, item: z.string().max(64) });
export const TiendaReclamarSchema = z.object({
  deviceId: DeviceId,
  nivel: z.number().int().min(1).max(PASE_TEMPORADA.niveles),
  premium: z.boolean(),
});
export const TiendaXpSchema = z.object({
  deviceId: DeviceId,
  metas: z.number().int().min(0).max(1000).default(0),
  bajas: z.number().int().min(0).max(10000).default(0),
  minutos: z.number().int().min(0).max(1440).default(0),
});
export const TiendaPagoSchema = z.object({ deviceId: DeviceId, paquete: z.string().max(32) });

/** El estado de la cuenta que ve el juego. */
export interface TiendaEstado {
  gemas: number;
  monedas: number;
  tiene: string[];
  pase: { temporada: string; xp: number; nivel: number; premium: boolean; reclamados: string[]; xpHoy: number };
  /** `true` si el servidor tiene un medio de pago real configurado. */
  pagos: boolean;
}

/** XP que vale un informe, antes del tope diario. */
export function xpDeInforme(i: { metas: number; bajas: number; minutos: number }): number {
  return i.metas * PASE_XP.meta + i.bajas * PASE_XP.baja + i.minutos * PASE_XP.minuto;
}
