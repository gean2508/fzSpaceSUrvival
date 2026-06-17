import { hash, verify } from "@node-rs/argon2";

/** Argon2id with sensible defaults. Prebuilt binary — no node-gyp needed. */
export function hashPassword(password: string): Promise<string> {
  return hash(password);
}

export function verifyPassword(hashed: string, password: string): Promise<boolean> {
  return verify(hashed, password);
}
