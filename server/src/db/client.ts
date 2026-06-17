import pg from "pg";
import { drizzle, type NodePgDatabase } from "drizzle-orm/node-postgres";
import * as schema from "./schema.js";
import { slog } from "../logger.js";

export type Database = NodePgDatabase<typeof schema>;

let pool: pg.Pool | null = null;
let dbInstance: Database | null = null;

/**
 * Returns a Drizzle db if DATABASE_URL is set, else null. The game must run
 * without Postgres (offline/LAN dev), so callers degrade gracefully.
 */
export function getDb(): Database | null {
  if (dbInstance) return dbInstance;
  const url = process.env.DATABASE_URL;
  if (!url) {
    slog("db", "DATABASE_URL not set — persistence disabled (in-memory)");
    return null;
  }
  try {
    pool = new pg.Pool({ connectionString: url, max: 10 });
    pool.on("error", (err) => slog("db", "pool error", { msg: err.message }));
    dbInstance = drizzle(pool, { schema });
    slog("db", "Postgres pool created");
    return dbInstance;
  } catch (e) {
    slog("db", "failed to init Postgres — persistence disabled", {
      msg: e instanceof Error ? e.message : String(e),
    });
    return null;
  }
}

export async function closeDb(): Promise<void> {
  await pool?.end();
  pool = null;
  dbInstance = null;
}
