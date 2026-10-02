import express from "express";
import cors from "cors";
import helmet from "helmet";
import rateLimit from "express-rate-limit";
import { resolve } from "node:path";
import { defineServer, defineRoom } from "colyseus";
import { ROOM_NAME } from "@hunger-arena/shared";
import { SurvivalRoom } from "./rooms/SurvivalRoom.js";
import { runMigrations } from "./db/migrate.js";
import { authRouter } from "./auth/routes.js";
import { updatesRouter, updatesDir } from "./updates/routes.js";
import { slog } from "./logger.js";
import { startRelay } from "./relay.js";

const PORT = Number(process.env.PORT) || 2567;
const HOST = process.env.HOST || "0.0.0.0";
const IS_DEV = process.env.NODE_ENV !== "production";

/**
 * CORS allowlist. In dev we allow any origin (LAN testing from phones with
 * arbitrary IPs). In prod, set CORS_ORIGINS="https://a.com,https://b.com".
 */
const allowedOrigins = (process.env.CORS_ORIGINS ?? "")
  .split(",")
  .map((o) => o.trim())
  .filter(Boolean);

const corsOptions: cors.CorsOptions = {
  origin: IS_DEV
    ? true
    : (origin, cb) => {
        if (!origin || allowedOrigins.includes(origin)) return cb(null, true);
        cb(new Error("not_allowed_by_cors"));
      },
};

// Throttle auth endpoints to blunt credential-stuffing / brute force.
const authLimiter = rateLimit({
  windowMs: 15 * 60 * 1000,
  max: 30,
  standardHeaders: true,
  legacyHeaders: false,
  message: { error: "rate_limited" },
});

await runMigrations();

const server = defineServer({
  rooms: {
    [ROOM_NAME]: defineRoom(SurvivalRoom),
  },
  express: (app) => {
    app.use(helmet({ contentSecurityPolicy: false }));
    app.use(cors(corsOptions));
    app.use(express.json({ limit: "16kb" }));
    app.get("/", (_req, res) => {
      res.json({ ok: true, room: ROOM_NAME, port: PORT });
    });
    app.use("/auth", authLimiter, authRouter());
    app.use("/updates", updatesRouter());
    app.use(
      "/updates/artifacts",
      express.static(resolve(updatesDir, "artifacts"), {
        maxAge: IS_DEV ? 0 : "1h",
        setHeaders(res, filePath) {
          const name = filePath.split("/").pop() ?? "download";
          res.setHeader("Content-Disposition", `attachment; filename="${name}"`);
          if (filePath.endsWith(".apk")) {
            res.setHeader("content-type", "application/vnd.android.package-archive");
          } else if (filePath.endsWith(".AppImage")) {
            res.setHeader("content-type", "application/x-executable");
          } else if (filePath.endsWith(".ipa")) {
            res.setHeader("content-type", "application/octet-stream");
          }
        },
      })
    );

    // Dev-only introspection tools. Imported dynamically so they can live in
    // devDependencies and never ship in a production install.
    if (IS_DEV) {
      void (async () => {
        try {
          const { monitor } = await import("@colyseus/monitor");
          const { playground } = await import("@colyseus/playground");
          app.use("/colyseus", monitor());
          app.use("/playground", playground());
          slog("boot", "dev tools mounted", {
            monitor: "/colyseus",
            playground: "/playground",
          });
        } catch (e) {
          slog("boot", "dev tools unavailable", {
            msg: e instanceof Error ? e.message : String(e),
          });
        }
      })();
    }
  },
});

startRelay(Number(process.env.RELAY_PORT) || 2568, HOST);

server.listen(PORT, HOST, undefined, () => {
  slog("boot", "Colyseus server listening", {
    host: HOST,
    port: PORT,
    ws: `ws://localhost:${PORT}`,
    room: ROOM_NAME,
  });
});
