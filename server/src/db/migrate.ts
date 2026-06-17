import { fileURLToPath } from "node:url";
import { dirname, resolve } from "node:path";
import { migrate } from "drizzle-orm/node-postgres/migrator";
import { getDb } from "./client.js";
import { slog } from "../logger.js";

/**
 * Apply pending migrations on boot when Postgres is configured. No-op (and
 * never throws) when persistence is disabled, so the server always starts.
 */
export async function runMigrations(): Promise<void> {
  const db = getDb();
  if (!db) return;
  try {
    const here = dirname(fileURLToPath(import.meta.url));
    const migrationsFolder = resolve(here, "../../drizzle");
    await migrate(db, { migrationsFolder });
    slog("db", "migrations applied");
  } catch (e) {
    slog("db", "migration failed (continuing without persistence)", {
      msg: e instanceof Error ? e.message : String(e),
    });
  }
}
