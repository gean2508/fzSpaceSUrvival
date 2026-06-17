import { Router, type Request, type Response } from "express";
import {
  LoginSchema,
  RefreshSchema,
  RegisterSchema,
  type AuthResponse,
} from "@hunger-arena/shared";
import { getStore } from "../db/store.js";
import { issueTokens, verifyToken } from "./jwt.js";
import { hashPassword, verifyPassword } from "./passwords.js";
import { slog } from "../logger.js";

/**
 * REST auth: guest-first means accounts are optional, but when used they let
 * progress follow a player across devices. Guest progress on `deviceId` is
 * linked to the new account on register.
 */
export function authRouter(): Router {
  const router = Router();
  const store = getStore();

  router.post("/register", async (req: Request, res: Response) => {
    const parsed = RegisterSchema.safeParse(req.body);
    if (!parsed.success) {
      return res.status(400).json({ error: "invalid_body" });
    }
    const { email, password, displayName, deviceId } = parsed.data;
    try {
      if (await store.findUserByEmail(email)) {
        return res.status(409).json({ error: "email_taken" });
      }
      const passwordHash = await hashPassword(password);
      const user = await store.createUser(email, passwordHash);
      const profile = deviceId
        ? await store.linkDeviceToUser(deviceId, user.id, displayName)
        : await store.getOrCreate(user.id);
      const tokens = await issueTokens(user.id);
      const out: AuthResponse = { ...tokens, profile };
      slog("auth", "registered", { email });
      return res.json(out);
    } catch (e) {
      slog("auth", "register failed", {
        msg: e instanceof Error ? e.message : String(e),
      });
      return res.status(500).json({ error: "server_error" });
    }
  });

  router.post("/login", async (req: Request, res: Response) => {
    const parsed = LoginSchema.safeParse(req.body);
    if (!parsed.success) {
      return res.status(400).json({ error: "invalid_body" });
    }
    const { email, password } = parsed.data;
    const user = await store.findUserByEmail(email);
    if (!user || !(await verifyPassword(user.passwordHash, password))) {
      return res.status(401).json({ error: "invalid_credentials" });
    }
    const profile =
      (await store.findProfileByUserId(user.id)) ??
      (await store.getOrCreate(user.id));
    const tokens = await issueTokens(user.id);
    const out: AuthResponse = { ...tokens, profile };
    slog("auth", "login", { email });
    return res.json(out);
  });

  router.post("/refresh", async (req: Request, res: Response) => {
    const parsed = RefreshSchema.safeParse(req.body);
    if (!parsed.success) {
      return res.status(400).json({ error: "invalid_body" });
    }
    const userId = await verifyToken(parsed.data.refreshToken, "refresh");
    if (!userId) return res.status(401).json({ error: "invalid_token" });
    const tokens = await issueTokens(userId);
    return res.json(tokens);
  });

  return router;
}
