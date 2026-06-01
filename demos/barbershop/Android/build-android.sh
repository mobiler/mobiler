#!/usr/bin/env bash
set -euo pipefail

# Reproducible Android build for a Mobiler app — produces a debug APK with no
# device or UI interaction. This is exactly what `mobiler dev` runs for the build
# (minus install/launch), captured as one non-interactive script: the unit a cloud
# build worker runs (Build Service, P0). The iOS twin is templates/iOS/build-ios.sh.
#
# Prereqs: Rust (+ Android targets), the Android SDK + NDK, and a JDK. Run
# `mobiler doctor` to check. Set JAVA_HOME if your JDK isn't the default.

# build-android.sh lives in Android/, so the app root (with shared/ + Android/) is ..
APP_ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$APP_ROOT"

# 1) Build the Rust core (host): compiles it and produces the uniffi metadata the
#    codegen step reads. (The Android-ABI .so is cross-compiled by gradle in step 3.)
cargo build -p shared --features uniffi

# 2) Generate the Kotlin ABI types + uniffi bindings.
rm -rf Android/generated
cargo run -p shared --bin codegen --features codegen,facet_typegen -- \
  --language kotlin --output-dir Android/generated

# 3) Build the APK (gradle cross-compiles the Android-ABI libs + assembles).
#    For a release build / store bundle the worker would use :app:assembleRelease or
#    :app:bundleRelease (+ signing) — debug is the simulator-equivalent default here.
./Android/gradlew -p Android --no-daemon :app:assembleDebug

APK="Android/app/build/outputs/apk/debug/app-debug.apk"
echo "✅ Built: $APP_ROOT/$APK"
echo "Install + launch on a device/emulator:"
echo "  adb install -r '$APK'"
echo "  adb shell am start -n <applicationId>/.MainActivity   # see Android/app/build.gradle.kts"
