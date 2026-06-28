import path from "node:path";
import { fileURLToPath } from "node:url";
import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";
import tailwindcss from "@tailwindcss/vite";
import { hungerArenaViteResolve } from "../scripts/vite-workspace.mjs";
import { landingDownloadsPlugin } from "./vite-downloads-plugin";

const appRoot = path.dirname(fileURLToPath(import.meta.url));
const ws = hungerArenaViteResolve(appRoot);

export default defineConfig({
  plugins: [react(), tailwindcss(), landingDownloadsPlugin(appRoot)],
  resolve: {
    alias: ws.alias,
  },
  optimizeDeps: {
    include: ws.optimizeDepsInclude,
  },
  server: {
    host: true,
    port: 5173,
    fs: {
      allow: [ws.fsAllow],
    },
  },
  build: {
    outDir: "dist",
    chunkSizeWarningLimit: 900,
  },
});
