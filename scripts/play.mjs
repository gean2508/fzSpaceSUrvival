#!/usr/bin/env node
/**
 * One command: ship APK + start server + landing. User only opens URL on phone.
 *
 * Usage: npm run play
 *        npm run play -- --no-ship     (skip rebuild if APK already published)
 *        npm run play -- --skip-tests  (build APK but skip test gates — evita OOM en PCs de poca RAM)
 *        npm run play -- --host 100.x.y.z  (bake this server host into the APK —
 *            e.g. a Tailscale IP so the phone can DOWNLOAD + PLAY from OUTSIDE the LAN)
 */
import { execSync, spawn } from "node:child_process";
import { existsSync, statSync } from "node:fs";
import { resolve, dirname } from "node:path";
import { fileURLToPath } from "node:url";

/** Puertos que ocupa el juego: el servidor Colyseus y la landing de Vite. */
const PUERTOS = [
  { n: 2567, que: "servidor" },
  { n: 5173, que: "landing" },
];

/**
 * **EL PUERTO YA TOMADO ERA EL "NO LEVANTA EL SERVIDOR".**
 *
 * Una corrida anterior de `./build-y-publicar.sh` deja `npm run dev:server` y
 * `dev:client` vivos: cerrar la terminal no los mata. La corrida siguiente
 * compila el APK entero, lanza el servidor, y éste muere al segundo con un
 * `EADDRINUSE 0.0.0.0:2567` perdido entre el banner de Colyseus — y como acá
 * abajo un hijo que se cae arrastraba al otro, se caía también la landing. El
 * usuario veía la compilación terminar bien y ningún servidor.
 *
 * Así que se pregunta ANTES de compilar, y si el que ocupa el puerto es un
 * `node` nuestro, se lo baja: es su propia corrida anterior, no otra cosa.
 */
function quienOcupa(puerto) {
  try {
    const out = execSync(`ss -lptnH 'sport = :${puerto}'`, { encoding: "utf8" });
    const m = out.match(/users:\(\("([^"]+)",pid=(\d+)/);
    return m ? { proceso: m[1], pid: Number(m[2]) } : null;
  } catch {
    return null; // sin `ss` o sin nadie escuchando
  }
}

function liberarPuertos() {
  for (const { n, que } of PUERTOS) {
    const duenio = quienOcupa(n);
    if (!duenio) continue;
    if (duenio.proceso !== "node") {
      console.error(
        `\n✗ El puerto ${n} (${que}) lo tiene ${duenio.proceso} (pid ${duenio.pid}), que no es nuestro.\n` +
          `  Cerralo y volvé a correr, o cambiá el puerto.`,
      );
      process.exit(1);
    }
    console.log(`⟲ Puerto ${n} (${que}) ocupado por una corrida anterior (pid ${duenio.pid}) — la bajo.`);
    try {
      process.kill(duenio.pid, "SIGTERM");
    } catch {
      /* ya no estaba */
    }
  }
  // Un SIGTERM tarda un instante en soltar el socket. Se espera a que suelte de
  // verdad en vez de dormir un número mágico.
  const hasta = Date.now() + 5000;
  while (Date.now() < hasta && PUERTOS.some(({ n }) => quienOcupa(n))) {
    execSync("sleep 0.2");
  }
  for (const { n, que } of PUERTOS) {
    if (quienOcupa(n)) {
      console.error(`\n✗ El puerto ${n} (${que}) sigue ocupado. Mirá: ss -lptn 'sport = :${n}'`);
      process.exit(1);
    }
  }
}

const ROOT = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const args = process.argv.slice(2);
const noShip = args.includes("--no-ship");
const skipTests = args.includes("--skip-tests");
// Optional explicit server host baked into the APK (e.g. a Tailscale IP so the
// phone downloads + plays from OUTSIDE the LAN): `npm run play -- --host 100.x.y.z`.
// The dev server + landing bind ALL interfaces (vite host:true, colyseus 0.0.0.0),
// so they stay reachable on the LAN IP too — only the APK's baked host is this one.
const hostIdx = args.indexOf("--host");
const explicitHost = hostIdx >= 0 ? args[hostIdx + 1] : null;

const lanHost =
  explicitHost ||
  execSync("node scripts/detect-lan-ip.mjs", {
    cwd: ROOT,
    encoding: "utf8",
  }).trim();

const apkPath = resolve(ROOT, "client/public/downloads/hunger-arena.apk");
const apkOk = existsSync(apkPath) && statSync(apkPath).size > 5_000_000;

// Antes de compilar nada: que los puertos estén libres. Compilar veinte minutos
// para morir en el `listen` es la peor forma de enterarse.
liberarPuertos();

if (!noShip || !apkOk) {
  try {
    const shipFlags = skipTests ? " --skip-tests" : "";
    execSync(`node scripts/ship-native-android.mjs --host ${lanHost}${shipFlags}`, {
      cwd: ROOT,
      stdio: "inherit",
    });
  } catch (e) {
    if (apkOk) {
      console.error("\n⚠ Build APK falló — siguiendo con el APK ya publicado en client/public/downloads/");
    } else {
      // **Y aun así se levanta el servidor.** Antes esto tiraba la excepción y
      // el comando terminaba ahí: build roto = ni servidor ni landing, que es
      // justo cuando más falta hacen para mirar qué pasó.
      console.error(
        "\n⚠ Build APK falló y no hay APK publicado: la landing va a quedar sin descarga.\n" +
          "  Levanto igual el servidor y la web para que puedas mirar el error de arriba.",
      );
    }
  }
} else {
  console.log("⊘ Skip ship — APK en client/public/downloads/ OK");
}

console.log("\n▶ Levantando servidor + landing…");

const env = {
  ...process.env,
  VITE_SERVER_HOST: lanHost,
  UPDATE_BASE_URL: `http://${lanHost}:2567`,
};

const server = spawn("npm", ["run", "dev:server"], {
  cwd: ROOT,
  env,
  stdio: "inherit",
  detached: false,
});

const client = spawn("npm", ["run", "dev:client"], {
  cwd: ROOT,
  env,
  stdio: "inherit",
  detached: false,
});

const shutdown = () => {
  server.kill("SIGTERM");
  client.kill("SIGTERM");
  process.exit(0);
};
process.on("SIGINT", shutdown);
process.on("SIGTERM", shutdown);

// **Que se caiga uno NO puede llevarse al otro.** Estaban atados con un
// `Promise.all(...).then(shutdown)`: el primero que terminara mataba al
// hermano, así que un servidor que no arranca dejaba también sin landing —
// pantalla en blanco en el celu y nada que leer. Ahora el que sobrevive sigue
// sirviendo y el que murió lo dice con todas las letras.
for (const [nombre, hijo] of [
  ["servidor", server],
  ["landing", client],
]) {
  hijo.on("exit", (code, signal) => {
    if (code === 0 || signal === "SIGTERM") return;
    console.error(
      `\n✗ El ${nombre} se cayó (código ${code ?? signal}). El otro sigue arriba.\n` +
        `  Mirá el error acá arriba; lo más común es el puerto ocupado o un .env que falta.`,
    );
  });
}

setTimeout(() => {
  console.log(`
╔══════════════════════════════════════════════════════════╗
║  HUNGER ARENA — listo para jugar                         ║
╠══════════════════════════════════════════════════════════╣
║  1. Celular en la misma WiFi que esta PC                 ║
║  2. Abrí en Chrome:                                      ║
║     → http://${lanHost}:5173
║  3. Tocá DESCARGAR APK → instalá → abrí                  ║
║                                                          ║
║  APK directo: http://${lanHost}:5173/downloads/hunger-arena.apk
╚══════════════════════════════════════════════════════════╝
`);
}, 4000);

// Sólo cuando se hayan ido LOS DOS termina el comando.
Promise.all([
  new Promise((res) => server.on("exit", res)),
  new Promise((res) => client.on("exit", res)),
]).then(() => process.exit(0));
