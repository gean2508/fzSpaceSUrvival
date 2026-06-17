import { randomUUID } from "node:crypto";
import { eq } from "drizzle-orm";
import {
  awardFor,
  type MatchResult,
  type ProfileData,
  type ProgressDelta,
} from "@hunger-arena/shared";
import { getDb, type Database } from "./client.js";
import { matchHistory, profiles, users } from "./schema.js";
import { slog } from "../logger.js";

export interface UserRecord {
  id: string;
  email: string;
  passwordHash: string;
}

function toData(row: {
  deviceId: string;
  displayName: string;
  xp: number;
  coins: number;
  bestWave: number;
  matches: number;
  userId: string | null;
}): ProfileData {
  return {
    deviceId: row.deviceId,
    displayName: row.displayName,
    xp: row.xp,
    coins: row.coins,
    bestWave: row.bestWave,
    matches: row.matches,
    hasAccount: row.userId != null,
  };
}

/** Persistence boundary. Two impls: Postgres (Drizzle) and in-memory fallback. */
export interface ProfileStore {
  getOrCreate(deviceId: string): Promise<ProfileData>;
  applyMatch(deviceId: string, result: MatchResult): Promise<ProfileData>;
  mergeProgress(deviceId: string, delta: ProgressDelta): Promise<ProfileData>;
  // accounts
  createUser(email: string, passwordHash: string): Promise<UserRecord>;
  findUserByEmail(email: string): Promise<UserRecord | null>;
  findProfileByUserId(userId: string): Promise<ProfileData | null>;
  /** Link a guest device's profile to an account (carries progress over). */
  linkDeviceToUser(
    deviceId: string,
    userId: string,
    displayName?: string
  ): Promise<ProfileData>;
}

class MemoryStore implements ProfileStore {
  private map = new Map<string, ProfileData>();

  async getOrCreate(deviceId: string): Promise<ProfileData> {
    let p = this.map.get(deviceId);
    if (!p) {
      p = {
        deviceId,
        displayName: "Invitado",
        xp: 0,
        coins: 0,
        bestWave: 0,
        matches: 0,
        hasAccount: false,
      };
      this.map.set(deviceId, p);
    }
    return { ...p };
  }

  async applyMatch(deviceId: string, result: MatchResult): Promise<ProfileData> {
    const p = await this.getOrCreate(deviceId);
    const { xp, coins } = awardFor(result);
    const next: ProfileData = {
      ...p,
      xp: p.xp + xp,
      coins: p.coins + coins,
      bestWave: Math.max(p.bestWave, result.waves),
      matches: p.matches + 1,
    };
    this.map.set(deviceId, next);
    return { ...next };
  }

  async mergeProgress(deviceId: string, delta: ProgressDelta): Promise<ProfileData> {
    const p = await this.getOrCreate(deviceId);
    const next: ProfileData = {
      ...p,
      xp: p.xp + delta.xp,
      coins: p.coins + delta.coins,
      bestWave: Math.max(p.bestWave, delta.bestWave),
      matches: p.matches + delta.matches,
    };
    this.map.set(deviceId, next);
    return { ...next };
  }

  private usersByEmail = new Map<string, UserRecord>();
  private deviceByUser = new Map<string, string>();

  async createUser(email: string, passwordHash: string): Promise<UserRecord> {
    if (this.usersByEmail.has(email)) throw new Error("email already registered");
    const user: UserRecord = { id: randomUUID(), email, passwordHash };
    this.usersByEmail.set(email, user);
    return user;
  }

  async findUserByEmail(email: string): Promise<UserRecord | null> {
    return this.usersByEmail.get(email) ?? null;
  }

  async findProfileByUserId(userId: string): Promise<ProfileData | null> {
    const device = this.deviceByUser.get(userId);
    if (!device) return null;
    return (await this.getOrCreate(device)) ?? null;
  }

  async linkDeviceToUser(
    deviceId: string,
    userId: string,
    displayName?: string
  ): Promise<ProfileData> {
    const p = await this.getOrCreate(deviceId);
    const next: ProfileData = {
      ...p,
      hasAccount: true,
      displayName: displayName ?? p.displayName,
    };
    this.map.set(deviceId, next);
    this.deviceByUser.set(userId, deviceId);
    return { ...next };
  }
}

class DrizzleStore implements ProfileStore {
  constructor(private db: Database) {}

  private async row(deviceId: string) {
    const [r] = await this.db
      .select()
      .from(profiles)
      .where(eq(profiles.deviceId, deviceId))
      .limit(1);
    return r;
  }

  async getOrCreate(deviceId: string): Promise<ProfileData> {
    const existing = await this.row(deviceId);
    if (existing) return toData(existing);
    const [created] = await this.db
      .insert(profiles)
      .values({ deviceId })
      .onConflictDoNothing()
      .returning();
    return toData(created ?? (await this.row(deviceId))!);
  }

  async applyMatch(deviceId: string, result: MatchResult): Promise<ProfileData> {
    const current = await this.row(deviceId);
    const base = current ?? (await (async () => {
      await this.getOrCreate(deviceId);
      return (await this.row(deviceId))!;
    })());
    const { xp, coins } = awardFor(result);
    const [updated] = await this.db
      .update(profiles)
      .set({
        xp: base.xp + xp,
        coins: base.coins + coins,
        bestWave: Math.max(base.bestWave, result.waves),
        matches: base.matches + 1,
        updatedAt: new Date(),
      })
      .where(eq(profiles.deviceId, deviceId))
      .returning();
    await this.db.insert(matchHistory).values({
      profileId: updated.id,
      mode: result.mode,
      waves: result.waves,
      kills: result.kills,
      durationMs: result.durationMs,
    });
    return toData(updated);
  }

  async mergeProgress(deviceId: string, delta: ProgressDelta): Promise<ProfileData> {
    await this.getOrCreate(deviceId);
    const base = (await this.row(deviceId))!;
    const [updated] = await this.db
      .update(profiles)
      .set({
        xp: base.xp + delta.xp,
        coins: base.coins + delta.coins,
        bestWave: Math.max(base.bestWave, delta.bestWave),
        matches: base.matches + delta.matches,
        updatedAt: new Date(),
      })
      .where(eq(profiles.deviceId, deviceId))
      .returning();
    return toData(updated);
  }

  async createUser(email: string, passwordHash: string): Promise<UserRecord> {
    const [u] = await this.db
      .insert(users)
      .values({ email, passwordHash })
      .returning();
    return { id: u.id, email: u.email ?? email, passwordHash };
  }

  async findUserByEmail(email: string): Promise<UserRecord | null> {
    const [u] = await this.db
      .select()
      .from(users)
      .where(eq(users.email, email))
      .limit(1);
    if (!u || !u.email || !u.passwordHash) return null;
    return { id: u.id, email: u.email, passwordHash: u.passwordHash };
  }

  async findProfileByUserId(userId: string): Promise<ProfileData | null> {
    const [p] = await this.db
      .select()
      .from(profiles)
      .where(eq(profiles.userId, userId))
      .limit(1);
    return p ? toData(p) : null;
  }

  async linkDeviceToUser(
    deviceId: string,
    userId: string,
    displayName?: string
  ): Promise<ProfileData> {
    await this.getOrCreate(deviceId);
    const base = (await this.row(deviceId))!;
    const [updated] = await this.db
      .update(profiles)
      .set({
        userId,
        displayName: displayName ?? base.displayName,
        updatedAt: new Date(),
      })
      .where(eq(profiles.deviceId, deviceId))
      .returning();
    return toData(updated);
  }
}

let store: ProfileStore | null = null;

/** Singleton store: Postgres when configured, in-memory otherwise. */
export function getStore(): ProfileStore {
  if (store) return store;
  const db = getDb();
  store = db ? new DrizzleStore(db) : new MemoryStore();
  slog("db", `profile store: ${db ? "postgres" : "memory"}`);
  return store;
}
