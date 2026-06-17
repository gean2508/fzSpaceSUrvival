import { SignJWT, jwtVerify } from "jose";
import type { AuthTokens } from "@hunger-arena/shared";

const ACCESS_TTL = "15m";
const REFRESH_TTL = "30d";

let cachedSecret: Uint8Array | null = null;

function secretKey(): Uint8Array {
  if (cachedSecret) return cachedSecret;
  const s = process.env.JWT_SECRET;
  if (!s || s.length < 16) {
    if (process.env.NODE_ENV === "production") {
      // Never run prod with a guessable/short secret.
      throw new Error(
        "JWT_SECRET must be set to a strong value (>=16 chars) in production"
      );
    }
    cachedSecret = new TextEncoder().encode(
      s || "dev-only-insecure-secret-change-me"
    );
    return cachedSecret;
  }
  cachedSecret = new TextEncoder().encode(s);
  return cachedSecret;
}

type TokenType = "access" | "refresh";

async function sign(userId: string, type: TokenType, ttl: string): Promise<string> {
  return new SignJWT({ type })
    .setProtectedHeader({ alg: "HS256" })
    .setSubject(userId)
    .setIssuedAt()
    .setExpirationTime(ttl)
    .sign(secretKey());
}

export async function issueTokens(userId: string): Promise<AuthTokens> {
  const [accessToken, refreshToken] = await Promise.all([
    sign(userId, "access", ACCESS_TTL),
    sign(userId, "refresh", REFRESH_TTL),
  ]);
  return { accessToken, refreshToken };
}

/** Returns the userId if the token is valid and of the expected type. */
export async function verifyToken(
  token: string,
  type: TokenType
): Promise<string | null> {
  try {
    const { payload } = await jwtVerify(token, secretKey());
    if (payload.type !== type || !payload.sub) return null;
    return payload.sub;
  } catch {
    return null;
  }
}
