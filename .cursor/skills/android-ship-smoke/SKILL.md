---
name: android-ship-smoke
description: >-
  Build, install, and smoke-test the native Android APK after ha-game changes.
  Use when shipping to phone, fixing mobile crashes, or after Bevy ECS / Android
  JNI work. Prevents shipping APKs that panic on frame 0 (B0001 query conflicts).
---

# Android ship + smoke (mandatory)

After **any** change under `native/crates/ha-game/` or `native/crates/ha-shared/` that
affects runtime:

1. **Build** (full pipeline — do not hand-copy `.so` with ad-hoc node one-liners):
   ```bash
   npm run ship:android -- --no-bump
   ```
   Or if Rust-only and jniLibs stale:
   ```bash
   node scripts/setup-android-native.mjs
   cd native && cargo ndk -t arm64-v8a -o android/app/src/main/jniLibs build -p ha-game --profile release-native --features android
   cd android && ./gradlew assembleDebug
   ```

2. **Install + smoke** (required before telling the user "listo"):
   ```bash
   adb install -r native/android/app/build/outputs/apk/debug/app-debug.apk
   node scripts/smoke-android.mjs
   ```
   Exit code must be **0**. If smoke fails, read `ha-crash` JSON line in logcat — fix root cause, rebuild, re-smoke.

3. **Common crash class — Bevy B0001**  
   `error[B0001]: Query<...&mut Transform...> conflicts with a previous system parameter`  
   - Never two `&mut Transform` queries on parent/child in one system.  
   - Fix: move only the parent root, use `Without<ChildMarker>`, or `ParamSet`.  
   - Prior hits: `rain_fx.rs`, `moon_world.rs` (`billboard_luna_earth`), `cloud_layer.rs` (`sync_cloud_navigation`).

4. **Logcat filters** when debugging:
   ```bash
   adb logcat -c && adb shell am force-stop com.hungerarena.app
   adb shell am start -n com.hungerarena.app/.MainActivity
   adb logcat -d | rg 'ha-crash|ha-boot|B0001|RustStdoutStderr|FATAL'
   ```

5. **Never** mark Android work done without: device connected check (`adb devices`), install success, smoke pass.
