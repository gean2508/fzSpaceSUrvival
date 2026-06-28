# Hunger Arena — Native Engine (DROP-REACT)

Rust **Bevy ECS** client. No React, R3F, DOM, or React Native in the render path.

## Stack choice

| Layer | Technology |
|-------|------------|
| ECS + loop | **Bevy 0.15** (60 FPS target, FixedUpdate @ 20 Hz sim) |
| Brain | **`ha-shared`** — Rust port of `packages/shared` constants, palette, movement |
| Shaders | WGSL cel-shading (3 bands) + inverted-hull outline `#2b2f3a` |
| Input | Left virtual joystick + right-screen look drag (touch) + WASD/arrows (desktop) |
| HUD | `bevy_ui` Minecraft-like pixel HUD (hearts, drumsticks, hotbar; no DOM) |

TypeScript `packages/shared` stays authoritative for **online netcode** until parity tests land; native reuses identical numeric constants and ground locomotion.

## Layout

```
native/
  Cargo.toml              workspace
  crates/
    ha-shared/            constants, palette, classes, input, movement (from TS)
    ha-game/              Bevy binary + shaders in assets/
  scripts/
    setup-android.sh      NDK + rust targets
    build-android.sh      APK via cargo-ndk
    build-desktop.sh      Linux release binary
  Makefile
```

## Desktop (Linux)

**Deps (Pop/Ubuntu):**

```bash
sudo apt install build-essential pkg-config libxkbcommon-dev libwayland-dev
```

**Run (dev):**

```bash
cd native
make run
# or: cargo run -p ha-game
```

**Release binary:**

```bash
make release
./target/release/ha-game
```

**Windows (cross from Linux):**

```bash
rustup target add x86_64-pc-windows-gnu
sudo apt install mingw-w64
make windows
# → target/x86_64-pc-windows-gnu/release/ha-game.exe
```

## Android

**One-time setup:**

```bash
cd native
./scripts/setup-android.sh
```

Requires Android SDK + NDK (`ANDROID_HOME`, `NDK_HOME`). Installs `cargo-ndk` and rust targets `aarch64-linux-android`, `armv7-linux-androideabi`.

**Build APK:**

```bash
./scripts/build-android.sh
# → target/android-artifacts/hunger-arena-native.apk
```

**Install on device/emulator:**

```bash
adb install -r ../client/public/downloads/hunger-arena.apk
adb shell am start -n com.hungerarena.app/.MainActivity
```

## Controls

| Platform | Move | Look / View | Sprint |
|----------|------|-------------|--------|
| Desktop | WASD / arrows | IJKL aim (camera yaw follows) | Shift |
| Mobile | Virtual joystick (bottom-left) | drag anywhere on right half | push to edge |

## Gameplay HUD

- Health lives top-left as procedural pixel hearts.
- Energy/stamina lives top-right as procedural drumstick icons.
- The bottom hotbar keeps gameplay items visible; the far-right `BAG` slot opens crafting/inventory.
- The always-on minimap is intentionally removed. Map UI returns later as a crafted/usable item.
- The top-center `MENU` button opens options/editor controls, keeping UI editing out of the normal play surface.

## Tests (shared port)

```bash
cargo test -p ha-shared
```

## Migration status

- [x] Constants + palette + classes from `packages/shared` / `client-gfx`
- [x] Ground `step_movement` port (20 Hz)
- [x] Ghibli toon material + outline shell
- [x] Meadow terrain + hero capsule
- [x] Orthographic follow camera
- [x] Touch move stick + right-screen look drag + Minecraft-like pixel HUD
- [ ] Full parkour FSM + obstacles from `parkour.ts`
- [x] Retire `apps/mobile` + `apps/desktop` R3F clients
- [ ] Colyseus transport in `ha-game` (online multiplayer)

Legacy React clients and `client/` landing remain until native reaches feature parity.
