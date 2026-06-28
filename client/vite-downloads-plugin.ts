import { createReadStream, existsSync, statSync } from "node:fs";
import path from "node:path";
import type { Connect, Plugin } from "vite";

const MIME: Record<string, string> = {
  ".apk": "application/vnd.android.package-archive",
  ".ipa": "application/octet-stream",
  ".AppImage": "application/x-executable",
};

function serveDownloads(root: string): Connect.NextHandleFunction {
  const downloadsDir = path.join(root, "public/downloads");

  return (req, res, next) => {
    if (req.method !== "GET" && req.method !== "HEAD") return next();
    const url = req.url?.split("?")[0] ?? "";
    if (!url.startsWith("/downloads/")) return next();

    const filename = decodeURIComponent(url.slice("/downloads/".length));
    if (!filename || filename.includes("..") || filename.includes("/")) return next();

    const filePath = path.join(downloadsDir, filename);
    if (!existsSync(filePath)) return next();

    const stat = statSync(filePath);
    const ext = path.extname(filename);
    res.setHeader("Content-Type", MIME[ext] ?? "application/octet-stream");
    res.setHeader("Content-Disposition", `attachment; filename="${filename}"`);
    res.setHeader("Content-Length", String(stat.size));
    res.setHeader("Cache-Control", "no-cache");

    if (req.method === "HEAD") {
      res.statusCode = 200;
      res.end();
      return;
    }

    res.statusCode = 200;
    createReadStream(filePath).pipe(res);
  };
}

/** Serves /downloads/* as attachment with correct MIME (dev + preview). */
export function landingDownloadsPlugin(appRoot: string): Plugin {
  return {
    name: "hunger-arena-landing-downloads",
    configureServer(server) {
      server.middlewares.use(serveDownloads(appRoot));
    },
    configurePreviewServer(server) {
      server.middlewares.use(serveDownloads(appRoot));
    },
  };
}
