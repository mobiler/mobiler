#!/usr/bin/env bash
set -euo pipefail

# Reproducible iOS build for a Mobiler app — macOS only, no Xcode UI. A single
# non-interactive script because it IS the unit a cloud build worker runs (Build
# Service, P0). Prereqs: Xcode + CLT, rustup, xcodegen (`brew install xcodegen`).
#
# Usage:
#   build-ios.sh            # simulator build (default) — no signing, no Apple account
#   build-ios.sh device     # device build:
#       • unsigned (default): a compile-check (CODE_SIGNING_ALLOWED=NO) — validates the
#         iphoneos build for free in CI; NOT installable.
#       • signed: set EXPORT_METHOD (app-store | ad-hoc | development) + DEVELOPMENT_TEAM
#         (your 10-char Apple team id) with a matching signing identity + provisioning
#         profile available → archives + exports an installable .ipa (TestFlight / OTA).

MODE="${1:-sim}"
IOS_DIR="$(cd "$(dirname "$0")" && pwd)"
APP_ROOT="$(cd "$IOS_DIR/.." && pwd)"
SHARED="$APP_ROOT/shared"
GEN="$IOS_DIR/generated"
SCHEME="Todo"
BUNDLE_ID="dev.mobiler.todo"

case "$MODE" in
  sim)    RUST_TARGET="${RUST_TARGET:-aarch64-apple-ios-sim}"; XCODE_SDK="iphonesimulator" ;;
  device) RUST_TARGET="${RUST_TARGET:-aarch64-apple-ios}";     XCODE_SDK="iphoneos" ;;
  *) echo "usage: build-ios.sh [sim|device]" >&2; exit 2 ;;
esac
ARCH="${RUST_TARGET%%-*}"; [ "$ARCH" = "aarch64" ] && ARCH=arm64   # the slice the static lib provides

rustup target add "$RUST_TARGET"
rm -rf "$GEN"; mkdir -p "$GEN/lib"

# 1) Rust core (uniffi) as a static lib for the target slice (linked into the app).
( cd "$SHARED" && cargo build --release --features uniffi --target "$RUST_TARGET" )
cp "$APP_ROOT/target/$RUST_TARGET/release/libshared.a" "$GEN/lib/"

# 2) A host build of the core so uniffi's bindgen can introspect the library
#    (it reads target/debug/libshared, not the static lib above).
( cd "$APP_ROOT" && cargo build -p shared --features uniffi )

# 3) Swift ABI types (SharedTypes package) + uniffi Swift FFI bindings (needs step 2).
( cd "$SHARED" && cargo run --bin codegen --features codegen -- --language swift --output-dir "$GEN" )

# Clang only auto-discovers a module map literally named `module.modulemap`; the
# codegen emits `sharedFFI.modulemap`, so expose it under the discoverable name.
cp "$GEN/sharedFFI.modulemap" "$GEN/module.modulemap"

# 4) Generate the Xcode project from project.yml.
( cd "$IOS_DIR" && xcodegen generate )

cd "$IOS_DIR"

if [ "$MODE" = "sim" ]; then
    # Build for the simulator (no signing).
    xcodebuild -project "$SCHEME.xcodeproj" -scheme "$SCHEME" \
        -sdk iphonesimulator -configuration Debug -derivedDataPath build \
        ARCHS="$ARCH" ONLY_ACTIVE_ARCH=NO CODE_SIGNING_ALLOWED=NO build
    APP="$IOS_DIR/build/Build/Products/Debug-iphonesimulator/$SCHEME.app"
    echo "✅ Built (simulator): $APP"
    echo
    echo "Run on a simulator + screenshot:"
    echo "  xcrun simctl boot 'iPhone 16' 2>/dev/null || true"
    echo "  xcrun simctl install booted '$APP'"
    echo "  xcrun simctl launch booted '$BUNDLE_ID'"
    echo "  sleep 3 && xcrun simctl io booted screenshot ios.png"

elif [ -z "${EXPORT_METHOD:-}" ]; then
    # Device compile-check (unsigned). Validates the iphoneos build with no Apple
    # account — what CI runs for free. Produces no installable artifact; set
    # EXPORT_METHOD (+ DEVELOPMENT_TEAM) to export a signed .ipa instead.
    xcodebuild -project "$SCHEME.xcodeproj" -scheme "$SCHEME" \
        -sdk iphoneos -configuration Release -derivedDataPath build \
        ARCHS="$ARCH" ONLY_ACTIVE_ARCH=NO CODE_SIGNING_ALLOWED=NO build
    echo "✅ Built (device, unsigned compile-check)."
    echo "   Set EXPORT_METHOD=app-store|ad-hoc|development + DEVELOPMENT_TEAM for a signed .ipa."

else
    # Signed device build → installable .ipa (TestFlight / ad-hoc OTA). Needs an
    # Apple account: a signing identity in the keychain + a provisioning profile for
    # $BUNDLE_ID. Automatic signing uses $DEVELOPMENT_TEAM.
    : "${DEVELOPMENT_TEAM:?set DEVELOPMENT_TEAM (your Apple team id) for a signed build}"
    xcodebuild -project "$SCHEME.xcodeproj" -scheme "$SCHEME" \
        -sdk iphoneos -configuration Release -derivedDataPath build \
        -archivePath "build/$SCHEME.xcarchive" \
        ARCHS="$ARCH" ONLY_ACTIVE_ARCH=NO \
        CODE_SIGN_STYLE=Automatic DEVELOPMENT_TEAM="$DEVELOPMENT_TEAM" \
        archive
    cat > build/ExportOptions.plist <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict>
  <key>method</key><string>${EXPORT_METHOD}</string>
  <key>teamID</key><string>${DEVELOPMENT_TEAM}</string>
  <key>destination</key><string>export</string>
</dict></plist>
PLIST
    xcodebuild -exportArchive -archivePath "build/$SCHEME.xcarchive" \
        -exportOptionsPlist build/ExportOptions.plist -exportPath build/ipa
    echo "✅ Built (device, signed): $IOS_DIR/build/ipa/$SCHEME.ipa"
    echo "   Upload to TestFlight: xcrun altool --upload-app -f build/ipa/$SCHEME.ipa \\"
    echo "     --type ios --apiKey \$ASC_KEY_ID --apiIssuer \$ASC_ISSUER_ID"
fi
