import { z } from "zod";
import type { ProfileData } from "./profile.js";

export const RegisterSchema = z.object({
  email: z.string().email().max(254),
  password: z.string().min(6).max(200),
  displayName: z.string().min(1).max(40).optional(),
  deviceId: z.string().min(1).max(128).optional(),
});
export type RegisterBody = z.infer<typeof RegisterSchema>;

export const LoginSchema = z.object({
  email: z.string().email().max(254),
  password: z.string().min(1).max(200),
});
export type LoginBody = z.infer<typeof LoginSchema>;

export const RefreshSchema = z.object({
  refreshToken: z.string().min(1).max(4096),
});
export type RefreshBody = z.infer<typeof RefreshSchema>;

export interface AuthTokens {
  accessToken: string;
  refreshToken: string;
}

export interface AuthResponse extends AuthTokens {
  profile: ProfileData;
}
