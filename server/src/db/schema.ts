import { sql } from "drizzle-orm";
import {
  integer,
  pgTable,
  text,
  timestamp,
  uuid,
} from "drizzle-orm/pg-core";

/** Accounts (Phase 6). Guests have a profile but no user row. */
export const users = pgTable("users", {
  id: uuid("id").primaryKey().defaultRandom(),
  email: text("email").unique(),
  passwordHash: text("password_hash"),
  createdAt: timestamp("created_at", { withTimezone: true })
    .notNull()
    .default(sql`now()`),
});

/** Progression, keyed by device id; optionally linked to a user account. */
export const profiles = pgTable("profiles", {
  id: uuid("id").primaryKey().defaultRandom(),
  deviceId: text("device_id").notNull().unique(),
  userId: uuid("user_id").references(() => users.id),
  displayName: text("display_name").notNull().default("Invitado"),
  xp: integer("xp").notNull().default(0),
  coins: integer("coins").notNull().default(0),
  bestWave: integer("best_wave").notNull().default(0),
  matches: integer("matches").notNull().default(0),
  updatedAt: timestamp("updated_at", { withTimezone: true })
    .notNull()
    .default(sql`now()`),
});

export const matchHistory = pgTable("match_history", {
  id: uuid("id").primaryKey().defaultRandom(),
  profileId: uuid("profile_id")
    .notNull()
    .references(() => profiles.id),
  mode: text("mode").notNull(),
  waves: integer("waves").notNull().default(0),
  kills: integer("kills").notNull().default(0),
  durationMs: integer("duration_ms").notNull().default(0),
  createdAt: timestamp("created_at", { withTimezone: true })
    .notNull()
    .default(sql`now()`),
});
