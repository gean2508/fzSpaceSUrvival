import AsyncStorage from "@react-native-async-storage/async-storage";
import type { AuthResponse, AuthTokens, ProfileData } from "@hunger-arena/shared";
import { SERVER_HTTP_URL } from "../config";
import { getCachedDeviceId } from "./identity";

const TOKEN_KEY = "arena-auth";

interface StoredAuth {
  accessToken: string;
  refreshToken: string;
  email: string;
}

export async function getStoredAuth(): Promise<StoredAuth | null> {
  const raw = await AsyncStorage.getItem(TOKEN_KEY);
  if (!raw) return null;
  try {
    return JSON.parse(raw) as StoredAuth;
  } catch {
    return null;
  }
}

export async function getAccessToken(): Promise<string | undefined> {
  return (await getStoredAuth())?.accessToken;
}

export async function isLoggedIn(): Promise<boolean> {
  return (await getStoredAuth()) != null;
}

async function store(tokens: AuthTokens, email: string): Promise<void> {
  await AsyncStorage.setItem(TOKEN_KEY, JSON.stringify({ ...tokens, email }));
}

export async function logout(): Promise<void> {
  await AsyncStorage.removeItem(TOKEN_KEY);
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
    deviceId: getCachedDeviceId(),
  });
  await store(out, email);
  return out.profile;
}

export async function login(email: string, password: string): Promise<ProfileData> {
  const out = await post<AuthResponse>("/auth/login", { email, password });
  await store(out, email);
  return out.profile;
}

export async function refresh(): Promise<boolean> {
  const auth = await getStoredAuth();
  if (!auth) return false;
  try {
    const out = await post<AuthTokens>("/auth/refresh", {
      refreshToken: auth.refreshToken,
    });
    await store(out, auth.email);
    return true;
  } catch {
    await logout();
    return false;
  }
}
