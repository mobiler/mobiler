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
SCHEME="Coffee"
BUNDLE_ID="dev.mobiler.coffee"

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
    # Signed device build → installable .ipa for TestFlight / App Store. Two-phase, so a
    # brand-new team with NO registered devices can build: archive UNSIGNED, then let
    # -exportArchive mint the App Store *distribution* profile (which carries no device
    # list, so it needs zero registered devices) via the App Store Connect API key and sign
    # during export. A signed archive instead makes automatic signing try to create a
    # *development* profile, which DOES embed a device list and fails on a deviceless team
    # ("your team has no devices…"). project.yml already disables signing — what we want here.
    : "${DEVELOPMENT_TEAM:?set DEVELOPMENT_TEAM (your Apple team id) for a signed build}"
    AUTH=()
    if [ -n "${ASC_KEY_ID:-}" ] && [ -n "${ASC_ISSUER_ID:-}" ] && [ -n "${ASC_KEY_PATH:-}" ]; then
        AUTH=(-allowProvisioningUpdates
              -authenticationKeyPath "$ASC_KEY_PATH"
              -authenticationKeyID "$ASC_KEY_ID"
              -authenticationKeyIssuerID "$ASC_ISSUER_ID")
    fi
    xcodebuild -project "$SCHEME.xcodeproj" -scheme "$SCHEME" \
        -sdk iphoneos -configuration Release -derivedDataPath build \
        -archivePath "build/$SCHEME.xcarchive" \
        ARCHS="$ARCH" ONLY_ACTIVE_ARCH=NO \
        CODE_SIGNING_ALLOWED=NO \
        archive
    cat > build/ExportOptions.plist <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict>
  <key>method</key><string>${EXPORT_METHOD}</string>
  <key>teamID</key><string>${DEVELOPMENT_TEAM}</string>
  <key>signingStyle</key><string>automatic</string>
  <key>destination</key><string>export</string>
</dict></plist>
PLIST
    xcodebuild -exportArchive -archivePath "build/$SCHEME.xcarchive" \
        -exportOptionsPlist build/ExportOptions.plist -exportPath build/ipa \
        ${AUTH[@]+"${AUTH[@]}"}
    echo "✅ Built (device, signed): $IOS_DIR/build/ipa/$SCHEME.ipa"
    if [ "${UPLOAD:-0}" = "1" ]; then
        # altool finds the API key by id in a standard dir — stage it there, then upload.
        : "${ASC_KEY_ID:?UPLOAD=1 needs ASC_KEY_ID / ASC_ISSUER_ID / ASC_KEY_PATH}"
        mkdir -p "$HOME/.appstoreconnect/private_keys"
        cp "$ASC_KEY_PATH" "$HOME/.appstoreconnect/private_keys/AuthKey_${ASC_KEY_ID}.p8"
        echo "Uploading $SCHEME.ipa to App Store Connect / TestFlight…"
        xcrun altool --upload-app -f "build/ipa/$SCHEME.ipa" --type ios \
            --apiKey "$ASC_KEY_ID" --apiIssuer "$ASC_ISSUER_ID"
        echo "✅ Uploaded — it appears in TestFlight after processing (a few minutes)."
    else
        echo "   To upload: set UPLOAD=1 with ASC_KEY_ID / ASC_ISSUER_ID / ASC_KEY_PATH."
    fi
fi
