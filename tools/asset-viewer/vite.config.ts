import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { defineConfig, type Plugin, type Connect } from "vite";

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const REPO_ROOT = path.resolve(__dirname, "../..");
const DEV_DIR = path.join(__dirname, "dev");
const OVERRIDES_PATH = path.join(DEV_DIR, "asset_overrides.json");
const OVERRIDE_SIGNAL = path.join(DEV_DIR, ".override_signal");
const CATALOG_PATH = path.join(
  REPO_ROOT,
  "native/crates/ha-game/assets/models/catalog-kinds.txt",
);

const SKIP_DIRS = new Set([
  ".git",
  "node_modules",
  "target",
  "dist",
  ".turbo",
  ".cargo",
  "build",
  "intermediates",
  ".gradle",
]);

const ASSET_EXT = new Set([".glb", ".gltf", ".blend"]);

type AssetEntry = {
  path: string;
  rel: string;
  name: string;
  ext: string;
  size: number;
  mtimeMs: number;
  category: string;
  inCatalog: boolean | null;
  catalogKind: string | null;
};

function loadCatalog(): Set<string> {
  if (!fs.existsSync(CATALOG_PATH)) return new Set();
  const text = fs.readFileSync(CATALOG_PATH, "utf8");
  return new Set(
    text
      .split("\n")
      .map((l) => l.trim())
      .filter(Boolean),
  );
}

function categorize(rel: string): string {
  if (rel.includes("tools/blender/staging")) return "blender-staging";
  if (rel.includes("tools/blender")) return "blender-pipeline";
  if (rel.includes("native/crates/ha-game/assets/models")) return "game-models";
  if (rel.includes("native/crates/ha-game/tests/fixtures")) return "test-fixtures";
  if (rel.endsWith(".blend")) return "blend-source";
  return "other";
}

function walk(dir: string, out: AssetEntry[], catalog: Set<string>): void {
  let entries: fs.Dirent[];
  try {
    entries = fs.readdirSync(dir, { withFileTypes: true });
  } catch {
    return;
  }
  for (const ent of entries) {
    const full = path.join(dir, ent.name);
    if (ent.isDirectory()) {
      if (SKIP_DIRS.has(ent.name)) continue;
      walk(full, out, catalog);
      continue;
    }
    const ext = path.extname(ent.name).toLowerCase();
    if (!ASSET_EXT.has(ext)) continue;
    const rel = path.relative(REPO_ROOT, full).replaceAll("\\", "/");
    const base = path.basename(ent.name, ext);
    let stat: fs.Stats;
    try {
      stat = fs.statSync(full);
    } catch {
      continue;
    }
    const inCatalog = ext === ".glb" ? catalog.has(base) : null;
    out.push({
      path: full,
      rel,
      name: ent.name,
      ext,
      size: stat.size,
      mtimeMs: stat.mtimeMs,
      category: categorize(rel),
      inCatalog,
      catalogKind: inCatalog ? base : null,
    });
  }
}

function scanAssets(): AssetEntry[] {
  const catalog = loadCatalog();
  const out: AssetEntry[] = [];
  walk(REPO_ROOT, out, catalog);
  out.sort((a, b) => a.rel.localeCompare(b.rel));
  return out;
}

function safeResolve(relPath: string): string | null {
  const decoded = decodeURIComponent(relPath).replaceAll("\\", "/");
  if (decoded.includes("..")) return null;
  const full = path.resolve(REPO_ROOT, decoded);
  if (!full.startsWith(REPO_ROOT + path.sep) && full !== REPO_ROOT) return null;
  if (!fs.existsSync(full)) return null;
  return full;
}

function resolveAssetPath(relOrAbs: string): string | null {
  const decoded = decodeURIComponent(relOrAbs).replaceAll("\\", "/");
  if (path.isAbsolute(decoded)) {
    if (!fs.existsSync(decoded)) return null;
    const ext = path.extname(decoded).toLowerCase();
    if (!ASSET_EXT.has(ext)) return null;
    return decoded;
  }
  return safeResolve(decoded);
}

function loadOverrides(): Record<string, string> {
  try {
    if (!fs.existsSync(OVERRIDES_PATH)) return {};
    return JSON.parse(fs.readFileSync(OVERRIDES_PATH, "utf8")) as Record<string, string>;
  } catch {
    return {};
  }
}

function saveOverrides(data: Record<string, string>): void {
  fs.mkdirSync(DEV_DIR, { recursive: true });
  fs.writeFileSync(OVERRIDES_PATH, JSON.stringify(data, null, 2) + "\n");
  fs.writeFileSync(OVERRIDE_SIGNAL, String(Date.now()));
}

function isValidKind(kind: string): boolean {
  return /^[a-z][a-z0-9_]{0,63}$/.test(kind);
}

function assetApiPlugin(): Plugin {
  return {
    name: "ha-asset-api",
    configureServer(server) {
      server.middlewares.use((req, res, next) => {
        const url = req.url ?? "";
        if (!url.startsWith("/api/")) return next();

        if (url === "/api/assets" || url.startsWith("/api/assets?")) {
          const assets = scanAssets();
          res.setHeader("Content-Type", "application/json");
          res.end(JSON.stringify({ repoRoot: REPO_ROOT, assets }));
          return;
        }

        if (url.startsWith("/api/file?")) {
          const q = new URL(url, "http://local");
          const rel = q.searchParams.get("path");
          if (!rel) {
            res.statusCode = 400;
            res.end("missing path");
            return;
          }
          const full = resolveAssetPath(rel);
          if (!full) {
            res.statusCode = 404;
            res.end("not found");
            return;
          }
          const ext = path.extname(full).toLowerCase();
          const types: Record<string, string> = {
            ".glb": "model/gltf-binary",
            ".gltf": "model/gltf+json",
            ".blend": "application/octet-stream",
          };
          res.setHeader("Content-Type", types[ext] ?? "application/octet-stream");
          fs.createReadStream(full).pipe(res);
          return;
        }

        if (url.startsWith("/api/assign-kind") && req.method === "POST") {
          let body = "";
          req.on("data", (chunk) => {
            body += chunk;
          });
          req.on("end", () => {
            try {
              const parsed = JSON.parse(body) as { kind?: string; path?: string };
              const kind = parsed.kind?.trim() ?? "";
              const rel = parsed.path?.trim() ?? "";
              if (!isValidKind(kind)) {
                res.statusCode = 400;
                res.end(JSON.stringify({ ok: false, error: "invalid kind" }));
                return;
              }
              const full = resolveAssetPath(rel);
              const ext = full ? path.extname(full).toLowerCase() : "";
              if (!full || (ext !== ".glb" && ext !== ".gltf")) {
                res.statusCode = 400;
                res.end(JSON.stringify({ ok: false, error: "path must be .glb or .gltf" }));
                return;
              }
              const storePath = path.isAbsolute(rel)
                ? rel
                : path.relative(REPO_ROOT, full).replaceAll("\\", "/");
              const overrides = loadOverrides();
              overrides[kind] = storePath;
              saveOverrides(overrides);
              res.setHeader("Content-Type", "application/json");
              res.end(JSON.stringify({ ok: true, kind, path: storePath }));
            } catch (e) {
              res.statusCode = 400;
              res.end(JSON.stringify({ ok: false, error: String(e) }));
            }
          });
          return;
        }

        if (url.startsWith("/api/overrides")) {
          res.setHeader("Content-Type", "application/json");
          res.end(JSON.stringify({ overrides: loadOverrides(), repoRoot: REPO_ROOT }));
          return;
        }

        if (url.startsWith("/api/open-blender?") && req.method === "POST") {
          const q = new URL(url, "http://local");
          const rel = q.searchParams.get("path");
          const full = rel ? safeResolve(rel) : null;
          if (!full || !full.endsWith(".blend")) {
            res.statusCode = 400;
            res.end(JSON.stringify({ ok: false, error: "not a .blend file" }));
            return;
          }
          import("node:child_process").then(({ spawn }) => {
            const blender = process.env.BLENDER ?? "blender";
            const child = spawn(blender, [full], {
              detached: true,
              stdio: "ignore",
            });
            child.unref();
            res.setHeader("Content-Type", "application/json");
            res.end(JSON.stringify({ ok: true, blender, file: rel }));
          });
          return;
        }

        next();
      });
    },
  };
}

export default defineConfig({
  root: __dirname,
  plugins: [assetApiPlugin()],
  server: {
    port: 5199,
    strictPort: true,
    fs: { allow: [REPO_ROOT] },
  },
});
