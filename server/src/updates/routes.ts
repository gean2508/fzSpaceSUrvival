import { Router, type Request } from "express";
import { readFileSync, existsSync, statSync } from "node:fs";
import { resolve, dirname } from "node:path";
import { fileURLToPath } from "node:url";
import { parseUpdateManifest, type UpdateManifest } from "@hunger-arena/shared";
import { slog } from "../logger.js";

const here = dirname(fileURLToPath(import.meta.url));
const updatesDir = resolve(here, "../../updates");
const manifestPath = resolve(updatesDir, "manifest.json");
const artifactsDir = resolve(updatesDir, "artifacts");

const MIN_APK_BYTES = 5_000_000;

function readManifestFile(): UpdateManifest {
  if (!existsSync(manifestPath)) {
    return { publishedAt: new Date(0).toISOString() };
  }
  const raw = readFileSync(manifestPath, "utf8");
  return parseUpdateManifest(JSON.parse(raw));
}

function publicBase(req: Request): string {
  const env = process.env.PUBLIC_BASE_URL?.replace(/\/$/, "");
  if (env) return env;
  const host = req.get("host");
  const proto = req.protocol;
  return host ? `${proto}://${host}` : "http://localhost:2567";
}

function rewriteLocalhostUrl(url: string, base: string): string {
  try {
    const u = new URL(url);
    if (u.hostname === "localhost" || u.hostname === "127.0.0.1") {
      return `${base.replace(/\/$/, "")}${u.pathname}`;
    }
  } catch {
    /* keep */
  }
  return url;
}

function artifactReady(name: string): boolean {
  const p = resolve(artifactsDir, name);
  if (!existsSync(p)) return false;
  const size = statSync(p).size;
  if (name.endsWith(".apk") && size < MIN_APK_BYTES) return false;
  return size > 0;
}

function manifestForClient(req: Request): UpdateManifest {
  const base = publicBase(req);
  const m = readManifestFile();
  const out: UpdateManifest = { ...m, publishedAt: m.publishedAt };

  if (m.android?.url && artifactReady("hunger-arena.apk")) {
    out.android = {
      ...m.android,
      url: rewriteLocalhostUrl(m.android.url, base),
    };
  } else {
    delete out.android;
  }

  if (m.desktop?.url && artifactReady("hunger-arena.AppImage")) {
    out.desktop = {
      ...m.desktop,
      url: rewriteLocalhostUrl(m.desktop.url, base),
    };
  } else {
    delete out.desktop;
  }

  return out;
}

/** Update distribution: manifest JSON + static APK/AppImage artifacts. */
export function updatesRouter(): Router {
  const router = Router();

  router.get("/manifest", (req, res) => {
    try {
      const manifest = manifestForClient(req);
      res.setHeader("cache-control", "no-store, no-cache, must-revalidate");
      res.json(manifest);
    } catch (e) {
      slog("security", "invalid update manifest", {
        msg: e instanceof Error ? e.message : String(e),
      });
      res.status(500).json({ error: "invalid_manifest" });
    }
  });

  router.get("/health", (req, res) => {
    res.json({
      ok: true,
      dir: updatesDir,
      hasManifest: existsSync(manifestPath),
      androidApk: artifactReady("hunger-arena.apk"),
      desktopImage: artifactReady("hunger-arena.AppImage"),
      publicBase: publicBase(req),
    });
  });

  return router;
}

export { updatesDir };
