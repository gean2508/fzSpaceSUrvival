import path from "node:path";
import { existsSync } from "node:fs";

function findRepoRoot(appDir) {
  let dir = appDir;
  for (let i = 0; i < 6; i++) {
    if (existsSync(path.resolve(dir, "packages/shared/package.json"))) return dir;
    const parent = path.resolve(dir, "..");
    if (parent === dir) break;
    dir = parent;
  }
  throw new Error(`monorepo root not found from ${appDir}`);
}

/** Vite resolve aliases for Hunger Arena monorepo workspace packages. */
export function hungerArenaViteResolve(appDir) {
  const repoRoot = findRepoRoot(appDir);
  const workspaceDist = (pkg) =>
    path.resolve(repoRoot, "packages", pkg, "dist", "index.js");

  return {
    alias: {
      "@hunger-arena/shared": workspaceDist("shared"),
    },
    fsAllow: repoRoot,
    optimizeDepsInclude: ["@hunger-arena/shared"],
  };
}
