# scanner — barcode / QR scanner (free, bundled)

A **free, bundled** Mobiler plugin. Single-shot: open the camera, scan one code, return it.
Bundled with the CLI alongside `battery`, so:

```bash
mobiler plugin add scanner
```

## App-side usage (Rust)

```rust
Msg::ScanPressed => cx.plugin("scanner", "scan", "", Msg::Scanned),
Msg::Scanned(resp) => {
    if resp.ok {
        // resp.output == "<format>:<value>" — e.g. "qr:https://…", "ean13:9781234567897"
        if let Some((format, value)) = resp.output.split_once(':') { /* … */ }
    } else {
        // "cancelled" / "camera not available" (e.g. simulator) / permission denied
    }
}
```

See `app-core-usage.rs` for a fuller example.

## What it returns

`ok:true` → `output = "<format>:<value>"`. Formats: `qr`, `ean13`, `ean8`, `upca`, `upce`,
`code128`, `code39`, `code93`, `codabar`, `itf`, `datamatrix`, `pdf417`, `aztec`, `other`.
`ok:false` → `output` is the reason (`cancelled`, `camera not available`, an error message).

## How it works per platform

- **Android:** ML Kit's **`GmsBarcodeScanner`** (Google Play Services) — a complete prebuilt
  scanner UI that handles the camera preview **and asks for the camera permission itself**, so the
  plugin needs **no CAMERA permission in the manifest** and no CameraX plumbing. Adds one Gradle
  dependency (`play-services-code-scanner`) via the manifest's `gradle_deps`.
- **iOS:** a full-screen **`AVCaptureMetadataOutput`** camera session (system frameworks only — no
  SPM package). Requires **`NSCameraUsageDescription`** (added by the manifest). No camera on the
  **simulator** → returns `ok:false` there; test on a device.
- **Web:** not implemented → graceful `ok:false` ("plugin 'scanner' not available"). The app
  handles that like any unavailable capability. (A `BarcodeDetector`-based web handler could be
  added later where browsers support it.)

## Install mechanics (what `mobiler plugin add scanner` does)

- Copies `ScannerPlugin.kt` → the app's package dir; `ScannerPlugin.swift` → `iOS/Sources/`.
- Registers it in `Core.kt` (plugins map) and `Core.swift` (Plugins switch).
- Adds the ML Kit Gradle dependency to `Android/app/build.gradle.kts`.
- Adds `NSCameraUsageDescription` to the iOS `project.yml` Info.plist.
- Idempotent (re-running adds nothing).

## Testing

Camera needs **real hardware** (no camera on the Android emulator's default config or the iOS
simulator). Build to a physical phone and scan any QR/barcode (a product, or generate one online).
Android is the quickest path (no Apple entitlement). See the NFC package's `TESTING-ON-DEVICE.md`
(private `mobiler/plugins` repo) for the general device-test pattern.

## Free vs. a future paid `scanner-pro`

This standard scanner is intentionally free (drives adoption — see the framework's
`CAPABILITIES-ROADMAP.md`). A future paid `scanner-pro` is the place for batch/continuous scanning,
niche symbologies (driver's licenses, GS1), or OCR/MRZ — not built yet.
