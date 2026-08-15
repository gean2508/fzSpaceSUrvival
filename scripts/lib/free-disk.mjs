/**
 * Purga de residuos de compilación — el disco de esta máquina vive al borde.
 *
 * 101 GB totales y `native/target/` sola se come ~24 GB. Cuando llega a cero el
 * síntoma NO es "no space left": es el linker muriendo con `Bus error`, o cargo
 * cayéndose sin mensaje, y se pierde media hora buscando un bug de compilación
 * que no existe. Por eso esto corre SOLO al final de cada build, sin que nadie
 * se acuerde de invocarlo.
 *
 * Qué se borra y por qué es seguro:
 *
 * - `target/{debug,release-native}/incremental/` — caché de compilación
 *   incremental. Es lo más grande que se puede tirar sin coste real: cargo la
 *   rehace sola y sólo cuesta una compilación un poco más lenta. Es ~3 GB.
 * - los `deps` de cada perfil — NO se tocan. Borrarlos obliga a recompilar TODO
 *   (15 GB, 10+ minutos) y eso no es limpieza, es castigo.
 *
 * O sea: agresivo con la caché, intocable con los artefactos. Si aun así el
 * disco queda por debajo del umbral, se avisa fuerte en vez de borrar más.
 */
import { existsSync, rmSync, statSync } from "node:fs";
import { execSync } from "node:child_process";
import { resolve } from "node:path";

/** Por debajo de esto el próximo build corre riesgo de morir enlazando. */
const WARN_FREE_GB = 5;

function freeGb(path) {
  try {
    // `df -Pk` es POSIX y estable de parsear; la 4ª columna son bloques libres.
    const out = execSync(`df -Pk ${JSON.stringify(path)}`, { encoding: "utf8" });
    const line = out.trim().split("\n").pop();
    return Number(line.split(/\s+/)[3]) / 1024 / 1024;
  } catch {
    return null;
  }
}

function sizeGb(path) {
  try {
    const out = execSync(`du -sk ${JSON.stringify(path)} 2>/dev/null`, { encoding: "utf8" });
    return Number(out.split(/\s+/)[0]) / 1024 / 1024;
  } catch {
    return 0;
  }
}

/**
 * Borra las cachés regenerables de `nativeDir/target` y reporta.
 * @param {string} nativeDir raíz del workspace de Rust (`.../native`)
 * @param {{quiet?: boolean}} opts
 */
export function freeDisk(nativeDir, opts = {}) {
  const target = resolve(nativeDir, "target");
  if (!existsSync(target)) return;
  const before = freeGb(target);
  let freed = 0;
  for (const profile of ["debug", "release-native", "release"]) {
    const dir = resolve(target, profile, "incremental");
    if (!existsSync(dir)) continue;
    freed += sizeGb(dir);
    rmSync(dir, { recursive: true, force: true });
  }
  const after = freeGb(target);
  if (!opts.quiet) {
    const f = freed > 0.05 ? ` (${freed.toFixed(1)} GB de caché incremental)` : "";
    console.log(`\n🧹 disco: ${after?.toFixed(1) ?? "?"} GB libres${f}`);
  }
  if (after !== null && after < WARN_FREE_GB) {
    const t = sizeGb(target).toFixed(0);
    console.warn(
      `\n⚠  Quedan ${after.toFixed(1)} GB libres — el próximo enlazado puede morir con "Bus error".\n` +
        `   \`native/target\` ocupa ${t} GB. Para recuperar el grueso:  cargo clean --manifest-path native/Cargo.toml\n` +
        `   (cuesta una recompilación completa; por eso no se hace solo.)`
    );
  }
}

// Ejecutable suelto:  node scripts/lib/free-disk.mjs
if (import.meta.url === `file://${process.argv[1]}`) {
  freeDisk(resolve(process.argv[2] ?? "native"));
}
