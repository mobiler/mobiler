#!/usr/bin/env bash
set -euo pipefail

# Reproducible iOS (simulator) build for a Mobiler app — macOS only, no Xcode UI.
# This is deliberately a single non-interactive script because it IS the unit a
# cloud build worker will run (Build Service, P0).
#
# Prereqs on the Mac:  Xcode + CLT, rustup, xcodegen (`brew install xcodegen`).
# Simulator build → no code signing, no Apple account.
#
# VERIFY-ON-MAC notes are inline; expect to adjust step 2/3 paths on first run.

IOS_DIR="$(cd "$(dirname "$0")" && pwd)"
APP_ROOT="$(cd "$IOS_DIR/.." && pwd)"
SHARED="$APP_ROOT/shared"
GEN="$IOS_DIR/generated"
SIM_TARGET="${SIM_TARGET:-aarch64-apple-ios-sim}"   # Apple Silicon sim; x86_64-apple-ios on Intel
ARCH="${SIM_TARGET%%-*}"; [ "$ARCH" = "aarch64" ] && ARCH=arm64   # the slice the static lib provides
SCHEME="Coffee"
BUNDLE_ID="dev.mobiler.coffee"

rustup target add "$SIM_TARGET"
rm -rf "$GEN"; mkdir -p "$GEN/lib"

# 1) Rust core (uniffi) as a static lib for the simulator (linked into the app).
( cd "$SHARED" && cargo build --release --features uniffi --target "$SIM_TARGET" )
cp "$APP_ROOT/target/$SIM_TARGET/release/libshared.a" "$GEN/lib/"

# 2) A host build of the core so uniffi's bindgen can introspect the library
#    (it reads target/debug/libshared, not the simulator static lib above).
( cd "$APP_ROOT" && cargo build -p shared --features uniffi )

# 3) Swift ABI types (SharedTypes package) + uniffi Swift FFI bindings.
#    Mirrors the Kotlin codegen, which emits both — needs the host lib from step 2.
( cd "$SHARED" && cargo run --bin codegen --features codegen -- --language swift --output-dir "$GEN" )

# Clang only auto-discovers a module map literally named `module.modulemap`; the
# codegen emits `sharedFFI.modulemap`, so expose it under the discoverable name
# (SWIFT_INCLUDE_PATHS points the importer at $GEN). This is what lets
# `import sharedFFI` in shared.swift resolve the RustBuffer/RustCallStatus symbols.
cp "$GEN/sharedFFI.modulemap" "$GEN/module.modulemap"

# 4) Generate the Xcode project from project.yml.
( cd "$IOS_DIR" && xcodegen generate )

# 5) Build for the simulator (no signing).
( cd "$IOS_DIR" && xcodebuild -project "$SCHEME.xcodeproj" -scheme "$SCHEME" \
    -sdk iphonesimulator -configuration Debug -derivedDataPath build \
    ARCHS="$ARCH" ONLY_ACTIVE_ARCH=NO CODE_SIGNING_ALLOWED=NO build )

APP="$IOS_DIR/build/Build/Products/Debug-iphonesimulator/$SCHEME.app"
echo "✅ Built: $APP"
echo
echo "Run on a simulator + screenshot:"
echo "  xcrun simctl boot 'iPhone 15' 2>/dev/null || true"
echo "  xcrun simctl install booted '$APP'"
echo "  xcrun simctl launch booted '$BUNDLE_ID'"
echo "  sleep 3 && xcrun simctl io booted screenshot ios.png"
