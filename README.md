# Hunger Arena

Supervivencia multijugador 3D — servidor autoritativo Colyseus + **cliente nativo Bevy** (Rust/ECS).

## Jugar (solo esto)

```bash
npm install
npm run play
```

En el celular (misma WiFi): abrí la URL que imprime el comando → **Descargar APK** → instalar → jugar.

| Comando | Qué hace |
|---------|----------|
| `npm run play` | Compila APK nativo, publica en landing, levanta server + web |
| `npm run play -- --no-ship` | Solo levanta server + landing (APK ya publicado) |
| `npm run ship:android` | Solo build + publicar APK (sin levantar servicios) |

## Estructura

```
packages/shared/   # Sim determinista (TS) — servidor + tests
server/            # Colyseus + Postgres + /updates/manifest
client/            # Landing — descarga APK (sin motor de juego)
native/            # Cliente de juego Bevy (Rust ECS, shaders Ghibli)
```

Ver [ARCHITECTURE.md](ARCHITECTURE.md) y [native/README.md](native/README.md).

## Desarrollo

```bash
npm run dev:server          # :2567
npm run dev:client          # landing :5173
npm run dev:native          # cliente desktop Linux (debug)
npm test                    # sim unit tests
npm run test:native         # ha-shared Rust tests
```

---

_Documentación histórica del stack React/R3F retirado en favor de `native/`._
