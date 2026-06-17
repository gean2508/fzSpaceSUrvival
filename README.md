# Hunger Arena

Juego de **supervivencia multijugador 3D top-down** (estilo Brawl Stars) con
servidor autoritativo, combate de habilidades por clases, NPCs vivos, persistencia
de progreso, cuentas opcionales y estética anime inspirada en Studio Ghibli —
funcionando **offline, LAN y online con un solo código**.

Stack: **Colyseus** (red + estado autoritativo) · **React Three Fiber** (render
toon nativo vía **Expo + expo-gl**) · **PostgreSQL + Drizzle** (persistencia).

---

## Características del MVP

- **Simulación determinista compartida** (`/shared`, TS puro) reusada por el
  servidor y por el modo offline en el navegador → offline == online.
- **Servidor autoritativo input-only**: el cliente solo envía intenciones; el
  server calcula todas las posiciones (base anti-cheat).
- **Predicción + reconciliación** del jugador local e **interpolación** de
  remotos/NPCs.
- **Combate data-driven**: 3 clases (Scout, Warrior, Mage), habilidades con
  proyectiles, melee, AoE, dash, cooldowns y daño server-side.
- **NPCs vivos**: percepción, FSM + steering, personalidades, modo supervivencia
  con oleadas crecientes.
- **Persistencia**: perfiles, progreso (XP/monedas/mejor oleada), historial de
  partidas en Postgres; mirror offline en AsyncStorage (móvil) con sync/merge al reconectar.
- **Auth invitado-primero**: device-id para jugar ya; cuenta opcional (argon2 +
  JWT) que vincula el progreso de invitado.
- **UI nativa (móvil)**: zustand + React Native (`View`/`Text`/`TouchableOpacity`),
  pantallas splash/menú/modo/clase/auth/ajustes/resultados, HUD de combate,
  joysticks multi-touch (hasta 4 dedos) con `react-native-gesture-handler`.
- **Landing web (`client/`)**: página de descarga del APK para instalar en el celular.
- **Observabilidad y debug**: overlay (FPS, RTT, tick, entidades, reconcile
  error), `r3f-perf`, error boundary, `@colyseus/monitor` + `/playground` en dev.
- **Seguridad**: helmet, CORS estricto, rate-limit, validación zod, token-bucket
  anti-flood, secretos por env, WSS en prod.

---

## Estructura del monorepo

```
hunger-arena/
├── packages/
│   └── shared/     # Sim determinista, schema Colyseus, tipos, zod, abilities, AI, tests
├── apps/
│   ├── mobile/     # Expo + R3F native (expo-gl), gestos multi-touch
│   └── desktop/    # Tauri + R3F WebGL, Pointer Lock + WASD → .AppImage Linux
├── server/         # SurvivalRoom autoritativa, auth REST, Drizzle/Postgres
├── client/         # Landing web: botón descarga APK
├── scripts/        # Tests e2e (join, survival, auth, persist)
├── turbo.json      # Orquestación de builds (npm workspaces)
├── OPTIMIZATION.md · ARCHITECTURE.md · .cursorrules · CLAUDE.md
```

Más detalle en [ARCHITECTURE.md](ARCHITECTURE.md).

---

## Requisitos

- Node.js 20+ y npm 9+
- (Opcional) Docker para Postgres local. Sin DB, el server corre con store
  en memoria.
- Misma WiFi (laptop + teléfono) para probar LAN/móvil.

---

## Desarrollo rápido

```bash
npm install

# server (:2567) + landing de descarga (:5173)
npm run dev

# o por separado
npm run dev:server
npm run dev:client          # landing APK
npm run dev:mobile          # app Expo
npm run dev:desktop         # Tauri (ventana nativa Linux)
```

Abrir **http://localhost:5173** para la página de descarga del APK.

**Jugar en el celular:** `npm run dev:mobile` (Expo Go o build nativo).

**Jugar en PC (Linux):** `npm run dev:desktop` (Tauri + ventana nativa sin browser chrome).

Debug overlay: activar en Ajustes (móvil) o tecla `` ` `` (desktop).

Dev tools del servidor: monitor en `http://localhost:2567/colyseus`,
playground en `http://localhost:2567/playground` (solo si `NODE_ENV != production`).

---

## Persistencia (Postgres)

Opcional para dev; el server cae a un store en memoria si no hay `DATABASE_URL`.

```bash
# Postgres local con Docker
docker compose up -d            # ver docker-compose.yml

cd server
cp .env.example .env            # define DATABASE_URL y JWT_SECRET
npm run db:migrate              # aplica migraciones (también corre al boot)
```

---

## Modos de juego

| Modo | Transporte | Notas |
|------|-----------|-------|
| **Offline** | `LocalTransport` | `GameSimulation` en el cliente (móvil o desktop), progreso local |
| **LAN** | `ColyseusTransport` | Apunta a la IP del laptop; invitado permitido |
| **Online** | `ColyseusTransport` | Server remoto, cuenta + JWT, progreso en Postgres |

Ambos clientes instancian la misma `GameSimulation` offline y envían el mismo
`InputPayload` al servidor. El URL del server:

- **Móvil:** `EXPO_PUBLIC_SERVER_HOST` / `EXPO_PUBLIC_SERVER_PORT`
- **Desktop:** `VITE_SERVER_HOST` / `VITE_SERVER_PORT`

---

## App móvil (Expo + React Native)

El juego corre en `apps/mobile/` con **@react-three/fiber/native** sobre **expo-gl**
(OpenGL ES / Metal). Importa la sim desde `@hunger-arena/shared` vía Metro
(`metro.config.js` + alias en `tsconfig.json`).

### Desarrollo en dispositivo

```bash
hostname -I                      # IP del laptop, ej 192.168.1.42

# Terminal 1 — server autoritativo
npm run dev:server

# Terminal 2 — app móvil apuntando al laptop
cd apps/mobile
EXPO_PUBLIC_SERVER_HOST=192.168.1.42 EXPO_PUBLIC_SERVER_PORT=2567 npx expo start
```

Escaneá el QR con Expo Go (Android) o ejecutá en emulador: `npx expo run:android`.

### Prebuild y APK Android

Genera los proyectos nativos (`android/`, `ios/`) a partir del config de Expo:

```bash
cd apps/mobile
npm run build -w @hunger-arena/shared   # desde la raíz, o: cd .. && npm run build -w @hunger-arena/shared

# IP del server embebida en el binario (LAN)
EXPO_PUBLIC_SERVER_HOST=192.168.1.42 npx expo prebuild --platform android

# Compilar APK release (requiere Android SDK + JDK 17)
cd android
./gradlew assembleRelease
```

El APK queda en:

`apps/mobile/android/app/build/outputs/apk/release/app-release.apk`

Copialo a la landing para descarga desde el celular:

```bash
cp apps/mobile/android/app/build/outputs/apk/release/app-release.apk \
   client/public/downloads/hunger-arena.apk
```

Luego serví la landing (`npm run dev:client` o `npm run build -w @hunger-arena/client`)
y abrí `http://<tu-ip>:5173` en el teléfono → **Descargar APK Android**.

### Dependencias nativas clave

| Paquete | Uso |
|---------|-----|
| `expo-gl` | Contexto WebGL → OpenGL ES / Metal |
| `@react-three/fiber` | Canvas 3D nativo (`/native`) |
| `react-native-gesture-handler` | Joysticks multi-touch (4 dedos) |
| `@colyseus/sdk` | WebSocket nativo hacia `server/` |
| `@react-native-async-storage/async-storage` | Perfil offline + auth |

---

## App desktop Linux (Tauri + WebGL)

Cliente nativo en `apps/desktop/`: ventana Tauri (sin barra de navegador) con
**@react-three/fiber** estándar sobre WebGL. Controles PC: **Pointer Lock** +
WASD/Shift/Espacio/QER. Sin controles táctiles.

### Desarrollo

```bash
# Requiere Rust toolchain + dependencias Linux para Tauri
# https://tauri.app/start/prerequisites/

npm run dev:server
VITE_SERVER_HOST=192.168.1.42 npm run dev:desktop
```

### Compilar `.AppImage` (Linux)

```bash
npm run build:desktop
# o manualmente:
cd apps/desktop
VITE_SERVER_HOST=192.168.1.42 npm run build:appimage
```

Salida:

`apps/desktop/src-tauri/target/release/bundle/appimage/Hunger Arena_0.1.0_amd64.AppImage`

Ejecutar:

```bash
chmod +x "Hunger Arena_0.1.0_amd64.AppImage"
./"Hunger Arena_0.1.0_amd64.AppImage"
```

---

## Landing web + descarga APK

`client/` ya no ejecuta el juego: es la página principal con el botón de descarga.

```bash
npm run dev:client
# http://localhost:5173 o http://<ip-laptop>:5173 desde el celular
```

Variables opcionales al compilar la landing:

| Variable | Default | Descripción |
|----------|---------|-------------|
| `VITE_APK_URL` | `/downloads/hunger-arena.apk` | URL del APK |
| `VITE_APK_VERSION` | `1.0.0` | Versión mostrada en la UI |

---

## Variables de entorno

### App móvil (`apps/mobile/`)
| Variable | Default | Descripción |
|----------|---------|-------------|
| `EXPO_PUBLIC_SERVER_HOST` | `localhost` | Host del server Colyseus |
| `EXPO_PUBLIC_SERVER_PORT` | `2567` | Puerto WebSocket |

### App desktop (`apps/desktop/`)
| Variable | Default | Descripción |
|----------|---------|-------------|
| `VITE_SERVER_HOST` | `localhost` | Host del server Colyseus |
| `VITE_SERVER_PORT` | `2567` | Puerto WebSocket |

### Landing web (`client/`)
| Variable | Default | Descripción |
|----------|---------|-------------|
| `VITE_APK_URL` | `/downloads/hunger-arena.apk` | Enlace de descarga del APK |
| `VITE_APK_VERSION` | `1.0.0` | Versión mostrada |

### Servidor
| Variable | Default | Descripción |
|----------|---------|-------------|
| `PORT` | `2567` | Puerto |
| `HOST` | `0.0.0.0` | Interfaz |
| `DATABASE_URL` | — | Postgres; si falta, store en memoria |
| `JWT_SECRET` | dev fallback | **Obligatorio en producción** (>=16 chars) |
| `CORS_ORIGINS` | — | Allowlist en prod, ej `https://a.com,https://b.com` |
| `NODE_ENV` | — | `production` desactiva dev tools y exige `JWT_SECRET` |

---

## Tests

```bash
npm test -w @hunger-arena/shared    # unit tests de la sim (vitest)

# e2e (con el server corriendo): valida netcode, oleadas, auth, persistencia
node scripts/test-join.mjs
node scripts/test-survival.mjs
node scripts/test-auth.mjs
node scripts/test-persist.mjs
```

---

## Observabilidad (Regla 9)

- **Servidor**: logs con timestamp (`connect`/`disconnect`/`room`/`db`/`auth`/
  `security`/`boot`).
- **Cliente**: overlay debug (FPS, RTT, tick, entidades, reconcile error, ring de
  logs) + `r3f-perf` (draw calls, GPU). Errores no atrapados → ring de logs y
  error boundary (no más pantalla negra).

| Síntoma | Causa probable |
|---------|----------------|
| `response.room is undefined` | Usar `@colyseus/sdk` 0.17, no `colyseus.js` 0.16 |
| Campos `undefined` en estado | Tipo definido en `/client` en vez de `/shared` |
| Rubber-banding constante | Timestep distinto o inputs desincronizados |
| Browser OK, APK no conecta | Falta `network_security_config.xml` cleartext |
| Lag / stutter | Posiciones en `useState` — usar `useRef` + `useFrame` |

---

## Comandos útiles

```bash
npm run dev              # server + landing
npm run dev:server       # solo Colyseus :2567
npm run dev:client       # solo landing :5173
npm run dev:mobile       # Expo
npm run dev:desktop      # Tauri
npm run build            # shared + landing
npm run build:desktop    # shared + AppImage Linux
npm run build:server     # compilar server TS
npm run start:server     # server compilado
```

Reglas de rendimiento en [OPTIMIZATION.md](OPTIMIZATION.md). Estilo gráfico en
`.cursor/skills/ghibli-toon-graphics/`.
