import type { AuthResponse, AuthTokens, ProfileData } from "@hunger-arena/shared";
import { SERVER_HTTP_URL } from "../config";
import { getDeviceId } from "./identity";

const TOKEN_KEY = "arena-auth";

interface StoredAuth {
  accessToken: string;
  refreshToken: string;
  email: string;
}

export function getStoredAuth(): StoredAuth | null {
  if (typeof window === "undefined") return null;
  const raw = localStorage.getItem(TOKEN_KEY);
  if (!raw) return null;
  try {
    return JSON.parse(raw) as StoredAuth;
  } catch {
    return null;
  }
}

export function getAccessToken(): string | undefined {
  return getStoredAuth()?.accessToken;
}

export function isLoggedIn(): boolean {
  return getStoredAuth() != null;
}

function store(tokens: AuthTokens, email: string): void {
  localStorage.setItem(
    TOKEN_KEY,
    JSON.stringify({ ...tokens, email })
  );
}

export function logout(): void {
  localStorage.removeItem(TOKEN_KEY);
}

async function post<T>(path: string, body: unknown): Promise<T> {
  const res = await fetch(SERVER_HTTP_URL + path, {
    method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify(body),
  });
  const json = await res.json().catch(() => ({}));
  if (!res.ok) {
    throw new Error((json as { error?: string }).error ?? `http_${res.status}`);
  }
  return json as T;
}

export async function register(
  email: string,
  password: string,
  displayName?: string
): Promise<ProfileData> {
  const out = await post<AuthResponse>("/auth/register", {
    email,
    password,
    displayName,
    deviceId: getDeviceId(),
  });
  store(out, email);
  return out.profile;
}

export async function login(
  email: string,
  password: string
): Promise<ProfileData> {
  const out = await post<AuthResponse>("/auth/login", { email, password });
  store(out, email);
  return out.profile;
}

/** Refresh the access token if a refresh token is stored. */
export async function refresh(): Promise<boolean> {
  const auth = getStoredAuth();
  if (!auth) return false;
  try {
    const out = await post<AuthTokens>("/auth/refresh", {
      refreshToken: auth.refreshToken,
    });
    store(out, auth.email);
    return true;
  } catch {
    logout();
    return false;
  }
}
