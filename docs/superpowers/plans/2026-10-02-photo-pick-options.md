# Photo Pick Options Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** `cx.pick_photo_with` / `cx.capture_photo_with` take per-call options (format, max dimension, max bytes,
quality, strip metadata) and return a typed `Photo` (handle, MIME, bytes, width, height) or a `PhotoError`, with the
same pipeline rules on Android, iOS and web.

**Architecture:** mobiler-core gets a `photo` module: the options value type (JSON input, like `Confirm`), the typed
reply (bincoded, like `DeviceInfo`), the error codes, and the pipeline's pure rules (`needs_reencode`,
`target_size`, `quality_ladder`, `output_format`) that ADR-0045 pins. Each shell runs the pipeline natively on the
picked file and returns a new handle. PR A: libraries + barbershop; publish core/web 0.43; PR B: template port.

**Tech Stack:** Rust (mobiler-core, mobiler-web/web-sys canvas), Kotlin (BitmapFactory, androidx.exifinterface),
Swift (ImageIO), CSS-free.

**Spec:** `docs/superpowers/specs/2026-10-02-photo-pick-options-design.md`

## Global Constraints

- `pick_photo` / `capture_photo` keep their signature and send input `""` (ADR-0007); the existing test
  `cx_pick_and_capture_photo_request_the_right_plugin` stays unchanged and green.
- `_with` calls use the same plugin/op (`photo`/`pick`, `camera`/`capture`) with a JSON input; a shell treats an
  EMPTY input as the old behaviour and any non-empty input as options (absent keys = defaults).
- Input JSON keys: `format` (`"original"|"jpeg"|"png"|"webp"`), `max_dimension` (u32), `max_bytes` (u64), `quality`
  (1..=100, default 85), `strip_metadata` (bool, default true).
- Success: `ok: true`, `output` = bincoded `Photo` (BincodeFfiFormat, ADR-0004), registered in codegen. Failure:
  `ok: false`, `output` = one of `cancelled`, `too_large`, `unsupported_image`, `unavailable`, or other text.
- Pipeline rules (ADR-0045): pass through untouched iff format is Original AND fits both limits AND (strip_metadata
  off OR no metadata); else re-encode with EXIF orientation applied, longest side ≤ max_dimension (never upscale),
  Original→PNG stays PNG / anything else → JPEG; lossy quality ladder start, −10 … floor 45; never return a file over
  max_bytes (→ `too_large`); PNG over max_bytes → `too_large`. Reported width/height are the upright dimensions.
- Android minimum API 26: HEIC decodes on 28+ only (26/27 → `unsupported_image`); WebP lossy encoder on 30+, else the
  deprecated `WEBP`. iOS 17. No new permissions (ADR-0035).
- Shells draw no English (ADR-0016): reasons are the machine codes above.
- Barbershop first; template ported in PR B after core/web 0.43 are on crates.io (ADR-0009, ADR-0015).
- Build rules: `export JAVA_HOME=~/jdk21`; per-demo `CARGO_TARGET_DIR`; `df -h /` before builds; never wipe
  `~/.gradle`; emulators only on ports 5560 (`mobiler_pixel7`) / 5562 (`mobiler_api26`); never `pgrep -f` polling.
- Commits via `git commit -F <file>`; end with `Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>`.
- Fresh independent code + security review before each PR ships (maintainer rule).

## Review Focus

1. **A rotated photo (EXIF orientation 6/8):** the reported width/height and the `max_dimension` check use the upright
   dimensions; the output is upright. Tests: core `target_size` is orientation-agnostic (callers pass upright dims —
   Task 1 doc); Android/iOS emulator check with an orientation-6 JPEG (Task 7).
2. **`max_bytes` below anything achievable** (e.g. 10 KB for a 12 MP photo): `too_large`, never a file over the limit.
   Test: web unit test `ladder_never_returns_over_limit` (Task 3) + Android emulator (Task 7).
3. **A new core with an old shell** (input ignored, plain URI answered): `Failed("unsupported shell")`, not a wrong
   `Photo`. Test: core `ok_reply_that_is_not_a_photo_is_failed` (Task 1).
4. **Cancel** on every shell → `Cancelled`; on the web the dialog's cancel no longer hangs. Test: core
   `codes_map_to_errors` (Task 1); web headless cancel check (Task 7).
5. **A huge image** (e.g. 12000×9000) on Android must not run out of memory: decode with `inSampleSize` before the
   exact scale. Test: Android emulator with a large pushed image (Task 7).

---

Note vs the spec's Testing section: ADR-0045's shared rules are pinned by mobiler-core unit tests (the rules live
there as functions the web shell calls), not by web-only tests.

## PR A: libraries + barbershop

Branch `feat/photo-pick-options` (the spec is committed there).

### Task 1: mobiler-core `photo` module and the `_with` calls

**Files:**
- Create: `mobiler-core/src/photo.rs`
- Modify: `mobiler-core/src/lib.rs` (module + re-exports near line 10–22; `pick_photo_with` / `capture_photo_with` next to `pick_photo` ~line 342)

**Interfaces:**
- Produces: `PhotoFormat { Original, Jpeg, Png, Webp }`; `PhotoOptions` (builder: `new()`, `jpeg()`, `png()`,
  `webp()`, `original()`, `max_dimension(u32)`, `max_bytes(u64)`, `quality(u8)`, `keep_metadata()`; getters
  `format()`, `max_dimension_opt()`, `max_bytes_opt()`, `quality_value()`, `strip_metadata()`; `to_input()`,
  `pub fn from_input(&str) -> PhotoOptions`); `Photo { handle: String, mime: String, bytes: u64, width: u32, height:
  u32 }` with `encode()` / `decode()`; `PhotoError { Cancelled, TooLarge, UnsupportedImage, Unavailable,
  Failed(String) }` with `code()`; rules `needs_reencode(format, fits: bool, strip: bool, has_metadata: bool) -> bool`,
  `target_size(w: u32, h: u32, max: Option<u32>) -> (u32, u32)`, `quality_ladder(start: u8) -> Vec<u8>`,
  `output_format(requested: PhotoFormat, source_is_png: bool) -> PhotoFormat`, `PhotoFormat::mime() -> &str`,
  `PhotoFormat::extension() -> &str`; `Cx::pick_photo_with` / `Cx::capture_photo_with`.

- [ ] **Step 1: Write the failing tests** — `mobiler-core/src/photo.rs` test module (create the file with only the
  test module and `use super::*;` first so it fails to compile):

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::PluginResponse;

    #[test]
    fn options_json_omits_defaults_and_round_trips() {
        assert_eq!(PhotoOptions::new().to_input(), "{}");
        let o = PhotoOptions::new().jpeg().max_dimension(2048).max_bytes(4 << 20).quality(80).keep_metadata();
        assert_eq!(o.to_input(), r#"{"format":"jpeg","max_dimension":2048,"max_bytes":4194304,"quality":80,"strip_metadata":false}"#);
        assert_eq!(PhotoOptions::from_input(&o.to_input()), o);
        assert_eq!(PhotoOptions::from_input("{}"), PhotoOptions::new());
        assert_eq!(PhotoOptions::from_input("not json"), PhotoOptions::new());
        assert_eq!(PhotoOptions::new().quality(0).quality_value(), 1);
        assert_eq!(PhotoOptions::new().quality(200).quality_value(), 100);
    }

    #[test]
    fn photo_round_trips() {
        let p = Photo { handle: "file:///c/p.jpg".into(), mime: "image/jpeg".into(), bytes: 1_234_567, width: 2048, height: 1536 };
        assert_eq!(Photo::decode(&p.encode()), Ok(p));
    }

    #[test]
    fn codes_map_to_errors() {
        for (code, e) in [("cancelled", PhotoError::Cancelled), ("too_large", PhotoError::TooLarge), ("unsupported_image", PhotoError::UnsupportedImage), ("unavailable", PhotoError::Unavailable)] {
            assert_eq!(from_response(&PluginResponse::text(false, code)), Err(e.clone()));
            assert_eq!(e.code(), code);
        }
        assert_eq!(from_response(&PluginResponse::text(false, "disk full")), Err(PhotoError::Failed("disk full".into())));
    }

    #[test]
    fn ok_reply_that_is_not_a_photo_is_failed() {
        // An old shell ignores the options and answers a plain URI.
        assert_eq!(from_response(&PluginResponse::text(true, "content://media/1")), Err(PhotoError::Failed("unsupported shell".into())));
    }

    #[test]
    fn pass_through_only_when_original_fits_and_clean() {
        assert!(!needs_reencode(PhotoFormat::Original, true, true, false));
        assert!(!needs_reencode(PhotoFormat::Original, true, false, true));
        assert!(needs_reencode(PhotoFormat::Original, true, true, true));
        assert!(needs_reencode(PhotoFormat::Original, false, false, false));
        assert!(needs_reencode(PhotoFormat::Jpeg, true, false, false));
    }

    #[test]
    fn target_size_caps_the_longest_side_and_never_upscales() {
        assert_eq!(target_size(4000, 3000, Some(2048)), (2048, 1536));
        assert_eq!(target_size(3000, 4000, Some(2048)), (1536, 2048));
        assert_eq!(target_size(800, 600, Some(2048)), (800, 600));
        assert_eq!(target_size(800, 600, None), (800, 600));
        assert_eq!(target_size(10_000, 1, Some(100)), (100, 1));
    }

    #[test]
    fn quality_ladder_steps_down_to_45() {
        assert_eq!(quality_ladder(85), vec![85, 75, 65, 55, 45]);
        assert_eq!(quality_ladder(50), vec![50, 45]);
        assert_eq!(quality_ladder(30), vec![30]);
        assert!(quality_ladder(100).iter().all(|q| *q >= 45 || *q == 100));
    }

    #[test]
    fn original_keeps_png_and_writes_everything_else_as_jpeg() {
        assert_eq!(output_format(PhotoFormat::Original, true), PhotoFormat::Png);
        assert_eq!(output_format(PhotoFormat::Original, false), PhotoFormat::Jpeg);
        assert_eq!(output_format(PhotoFormat::Webp, false), PhotoFormat::Webp);
        assert_eq!((PhotoFormat::Jpeg.mime(), PhotoFormat::Jpeg.extension()), ("image/jpeg", "jpg"));
    }
}
```

  And in `mobiler-core/src/lib.rs`'s test module:

```rust
    #[test]
    fn photo_with_calls_send_options_on_the_same_plugin() {
        let mut cx = Cx::<Ev>::default();
        cx.pick_photo_with(PhotoOptions::new().jpeg(), |_| Ev::Tap);
        cx.capture_photo_with(PhotoOptions::new(), |_| Ev::Tap);
        let (p, _) = &cx.requests[0];
        assert_eq!((p.plugin.as_str(), p.op.as_str(), p.input.as_str()), ("photo", "pick", r#"{"format":"jpeg"}"#));
        let (c, _) = &cx.requests[1];
        assert_eq!((c.plugin.as_str(), c.op.as_str(), c.input.as_str()), ("camera", "capture", "{}"));
    }
```

- [ ] **Step 2: Run** `CARGO_TARGET_DIR=$PWD/target cargo test -p mobiler-core photo` → compile errors (types and
  functions undefined).

- [ ] **Step 3: Implement** `mobiler-core/src/photo.rs` above the test module:

```rust
//! Per-call photo options for [`Cx::pick_photo_with`](crate::Cx::pick_photo_with) and
//! [`Cx::capture_photo_with`](crate::Cx::capture_photo_with): the format, size and metadata the
//! shell re-encodes a picked or captured image to (ADR-0045), and the typed reply. The pipeline
//! rules every shell follows live here as functions, so they are stated and tested once.

use facet::Facet;
use serde::{Deserialize, Serialize};

use crate::PluginResponse;

/// The image format a photo is delivered in.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PhotoFormat {
    /// Keep the picked file's format if it can pass through; when it must be re-encoded, PNG stays
    /// PNG and anything else (HEIC, WebP, …) becomes JPEG.
    #[default]
    Original,
    Jpeg,
    Png,
    /// Lossy WebP. iOS can't encode WebP (→ JPEG) and Safari's canvas can't (→ PNG); `Photo::mime`
    /// says what was produced.
    Webp,
}

impl PhotoFormat {
    pub fn mime(self) -> &'static str {
        match self {
            Self::Png => "image/png",
            Self::Webp => "image/webp",
            Self::Jpeg | Self::Original => "image/jpeg",
        }
    }

    pub fn extension(self) -> &'static str {
        match self {
            Self::Png => "png",
            Self::Webp => "webp",
            Self::Jpeg | Self::Original => "jpg",
        }
    }

    fn is_original(&self) -> bool {
        *self == Self::Original
    }
}

/// Options for a picked or captured photo. Unset options keep the defaults: `Original` format, no
/// size limits, quality 85, metadata stripped.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct PhotoOptions {
    #[serde(skip_serializing_if = "PhotoFormat::is_original")]
    format: PhotoFormat,
    #[serde(skip_serializing_if = "Option::is_none")]
    max_dimension: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    max_bytes: Option<u64>,
    #[serde(skip_serializing_if = "is_default_quality")]
    quality: u8,
    #[serde(skip_serializing_if = "is_true")]
    strip_metadata: bool,
}

const DEFAULT_QUALITY: u8 = 85;

fn is_default_quality(q: &u8) -> bool {
    *q == DEFAULT_QUALITY
}

fn is_true(b: &bool) -> bool {
    *b
}

impl Default for PhotoOptions {
    fn default() -> Self {
        Self { format: PhotoFormat::Original, max_dimension: None, max_bytes: None, quality: DEFAULT_QUALITY, strip_metadata: true }
    }
}

impl PhotoOptions {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }
    #[must_use]
    pub fn jpeg(mut self) -> Self {
        self.format = PhotoFormat::Jpeg;
        self
    }
    #[must_use]
    pub fn png(mut self) -> Self {
        self.format = PhotoFormat::Png;
        self
    }
    #[must_use]
    pub fn webp(mut self) -> Self {
        self.format = PhotoFormat::Webp;
        self
    }
    #[must_use]
    pub fn original(mut self) -> Self {
        self.format = PhotoFormat::Original;
        self
    }
    /// The longest side in pixels; a smaller image is never upscaled.
    #[must_use]
    pub fn max_dimension(mut self, px: u32) -> Self {
        self.max_dimension = Some(px);
        self
    }
    /// The largest file allowed. A lossy format steps its quality down to fit; if it can't, the call
    /// fails with [`PhotoError::TooLarge`] rather than return a larger file.
    #[must_use]
    pub fn max_bytes(mut self, bytes: u64) -> Self {
        self.max_bytes = Some(bytes);
        self
    }
    /// The starting quality for a lossy format, 1–100 (default 85).
    #[must_use]
    pub fn quality(mut self, q: u8) -> Self {
        self.quality = q.clamp(1, 100);
        self
    }
    /// Keep the original's metadata when it can pass through untouched (default: stripped, which
    /// drops EXIF and the GPS position). A re-encoded image never carries metadata.
    #[must_use]
    pub fn keep_metadata(mut self) -> Self {
        self.strip_metadata = false;
        self
    }

    pub fn format(&self) -> PhotoFormat {
        self.format
    }
    pub fn max_dimension_opt(&self) -> Option<u32> {
        self.max_dimension
    }
    pub fn max_bytes_opt(&self) -> Option<u64> {
        self.max_bytes
    }
    pub fn quality_value(&self) -> u8 {
        self.quality.clamp(1, 100)
    }
    pub fn strip_metadata(&self) -> bool {
        self.strip_metadata
    }

    pub(crate) fn to_input(&self) -> String {
        serde_json::to_string(self).expect("serialize photo options")
    }

    /// Parse a `_with` call's input (for shells written in Rust); anything unparsable is the defaults.
    pub fn from_input(input: &str) -> Self {
        let mut o: Self = serde_json::from_str(input).unwrap_or_default();
        o.quality = o.quality.clamp(1, 100);
        o
    }
}

/// A delivered photo: a new file the shell wrote (`file://` on native, `blob:` on web) that
/// `image(…)` and `cx.upload` accept, with what the app needs to label an upload.
#[derive(Facet, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[repr(C)]
pub struct Photo {
    pub handle: String,
    /// What the shell actually produced, e.g. `image/jpeg`.
    pub mime: String,
    pub bytes: u64,
    /// Upright dimensions (EXIF orientation applied).
    pub width: u32,
    pub height: u32,
}

impl Photo {
    /// Serialize for a `PluginResponse.output` (crux's FFI format, like [`crate::DeviceInfo`]).
    pub fn encode(&self) -> Vec<u8> {
        use crux_core::bridge::{BincodeFfiFormat, FfiFormat};
        let mut buffer = Vec::new();
        BincodeFfiFormat::serialize(&mut buffer, self).expect("encode Photo");
        buffer
    }

    pub fn decode(bytes: &[u8]) -> Result<Self, String> {
        use crux_core::bridge::{BincodeFfiFormat, FfiFormat};
        BincodeFfiFormat::deserialize(bytes).map_err(|e| e.to_string())
    }
}

/// Why a `_with` call delivered no photo. Word it yourself: the shells send only these codes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PhotoError {
    Cancelled,
    /// It could not be made to fit `max_bytes`.
    TooLarge,
    /// The platform can't decode it (e.g. HEIC on Android 8 or a non-Safari browser).
    UnsupportedImage,
    /// No picker or camera on this device.
    Unavailable,
    Failed(String),
}

impl PhotoError {
    pub fn code(&self) -> &str {
        match self {
            Self::Cancelled => "cancelled",
            Self::TooLarge => "too_large",
            Self::UnsupportedImage => "unsupported_image",
            Self::Unavailable => "unavailable",
            Self::Failed(text) => text,
        }
    }

    fn from_code(code: &str) -> Self {
        match code {
            "cancelled" => Self::Cancelled,
            "too_large" => Self::TooLarge,
            "unsupported_image" => Self::UnsupportedImage,
            "unavailable" => Self::Unavailable,
            other => Self::Failed(other.to_string()),
        }
    }
}

pub(crate) fn from_response(r: &PluginResponse) -> Result<Photo, PhotoError> {
    if r.ok {
        Photo::decode(&r.output).map_err(|_| PhotoError::Failed("unsupported shell".into()))
    } else {
        Err(PhotoError::from_code(&String::from_utf8_lossy(&r.output)))
    }
}

/// ADR-0045 rule: the original passes through untouched only when the format is `Original`, it fits
/// both limits, and there is no metadata to strip.
pub fn needs_reencode(format: PhotoFormat, fits: bool, strip_metadata: bool, has_metadata: bool) -> bool {
    format != PhotoFormat::Original || !fits || (strip_metadata && has_metadata)
}

/// ADR-0045 rule: scale so the longest (upright) side is at most `max`; never upscale.
pub fn target_size(width: u32, height: u32, max: Option<u32>) -> (u32, u32) {
    let longest = width.max(height);
    match max {
        Some(m) if m > 0 && longest > m => {
            let scale = f64::from(m) / f64::from(longest);
            let fit = |v: u32| ((f64::from(v) * scale).round() as u32).max(1);
            (fit(width), fit(height))
        }
        _ => (width, height),
    }
}

/// ADR-0045 rule: the qualities a lossy format tries for `max_bytes`: the start, then −10 to a floor of 45.
pub fn quality_ladder(start: u8) -> Vec<u8> {
    let start = start.clamp(1, 100);
    let mut steps = vec![start];
    let mut q = start;
    while q > 45 {
        q = q.saturating_sub(10).max(45);
        steps.push(q);
    }
    steps
}

/// ADR-0045 rule: the format a re-encode writes.
pub fn output_format(requested: PhotoFormat, source_is_png: bool) -> PhotoFormat {
    match requested {
        PhotoFormat::Original if source_is_png => PhotoFormat::Png,
        PhotoFormat::Original => PhotoFormat::Jpeg,
        other => other,
    }
}
```

  In `lib.rs`: `pub mod photo;` next to `pub mod device;`, `pub use photo::{Photo, PhotoError, PhotoFormat,
  PhotoOptions};`, and after `capture_photo`:

```rust
    /// [`pick_photo`](Self::pick_photo) with per-call options: the shell re-encodes the picked image to
    /// the format and limits asked for (applying its orientation and, by default, dropping its EXIF and
    /// GPS data) and delivers a [`Photo`] (a new file handle plus MIME, size and dimensions), or a
    /// [`PhotoError`]. The rules are ADR-0045's; `pick_photo` itself is unchanged.
    pub fn pick_photo_with(&mut self, options: PhotoOptions, then: impl FnOnce(Result<Photo, PhotoError>) -> E + Send + 'static) {
        self.plugin("photo", "pick", options.to_input(), move |r| then(photo::from_response(&r)));
    }

    /// [`capture_photo`](Self::capture_photo) with per-call options, like
    /// [`pick_photo_with`](Self::pick_photo_with).
    pub fn capture_photo_with(&mut self, options: PhotoOptions, then: impl FnOnce(Result<Photo, PhotoError>) -> E + Send + 'static) {
        self.plugin("camera", "capture", options.to_input(), move |r| then(photo::from_response(&r)));
    }
```

  Adjust to the `plugin(...)` signature in lib.rs if `input` must be `impl Into<String>` (it takes `""` today, so a
  `String` works). If `serde_json` isn't already a mobiler-core dependency, it is (dialog.rs uses it).

- [ ] **Step 4: Run** `cargo test -p mobiler-core` and `cargo clippy -p mobiler-core -- -D warnings` (repo-local
  `CARGO_TARGET_DIR`) → all green; `cx_pick_and_capture_photo_request_the_right_plugin` unchanged and passing.

- [ ] **Step 5: Commit** `feat(core): pick_photo_with / capture_photo_with — PhotoOptions, Photo, PhotoError`.

### Task 2: ADR-0045 and its mutation proof

**Files:**
- Create: `docs/adr/ADR-0045-photo-pipeline.md`; Modify: `docs/adr/index.md`

- [ ] **Step 1: Mutation proof** of the core rule tests (each mutation in `photo.rs`, run `cargo test -p
  mobiler-core photo`, record the failing test, revert):
  - `needs_reencode` without the `strip_metadata && has_metadata` term → `pass_through_only_when_original_fits_and_clean` fails.
  - `quality_ladder` floor 35 instead of 45 → `quality_ladder_steps_down_to_45` fails.
  - `target_size` without the `longest > m` guard (so it upscales) → `target_size_caps_the_longest_side_and_never_upscales` fails.
  - `output_format(Original, true)` → Jpeg → `original_keeps_png_and_writes_everything_else_as_jpeg` fails.
- [ ] **Step 2: Write ADR-0045** from `docs/adr/TEMPLATE.md`. Claim: "A photo picked or captured with options is
  re-encoded by the shell under one set of rules: it passes through untouched only when its format is kept, it fits
  both limits and it carries no metadata to strip; otherwise it is upright, capped to the longest side, encoded
  (Original keeps PNG, else JPEG), stepped down in quality (to 45) to fit max_bytes or refused, and carries no
  metadata; the reply is a typed Photo or a machine-code reason". Context: the Moj Termin request (quote: "Phone
  camera photos, however, fail our server's checks almost every time"; HEIC; EXIF GPS; the four acceptance checks).
  Options: A per-app re-encode `[reconstructed]` (rejected: the core never sees pixels, ADR-0027); B a global setting
  `[recorded: Moj Termin request, 2026-10-01, quoted here]` ("Each wants different sizes, so the choice should be per
  call, not one global setting"); C silently return a larger file `[reconstructed]` (rejected: servers reject it;
  the app can't tell); D the pipeline above `[recorded: spec]` chosen. §4 the mutation proofs from Step 1.
  Consequences (negative): HEIC fails on Android 8 and non-Safari browsers; WebP falls back on iOS/Safari; web can't
  detect metadata (re-encodes every non-PNG when stripping); quality 45 floor may fail an aggressive limit; the
  plain calls still return originals with metadata. Conformance: the four core tests (`mobiler-core/src/photo.rs::…`).
  Deciding PRs: PR A's number. Index row.
- [ ] **Step 3:** `cargo test -p xtask` (ADR lint) → green. **Commit** `docs: ADR-0045 photo pipeline`.

### Task 3: web shell — cancel fix and the pipeline

**Files:**
- Modify: `mobiler-web/Cargo.toml` (web-sys features: `HtmlCanvasElement`, `CanvasRenderingContext2d`,
  `ImageBitmap`, `BlobCallback` not needed — `to_blob` takes a `&Function`)
- Modify: `mobiler-web/src/lib.rs` (`perform` photo/camera dispatch ~line 996; `take_image` ~line 1120)

**Interfaces:**
- Consumes: Task 1's `PhotoOptions::from_input`, getters, `Photo`, `needs_reencode`, `target_size`,
  `quality_ladder`, `output_format`, `PhotoFormat::mime`.

- [ ] **Step 1: Failing pure tests** (new test module at the end of `lib.rs`):

```rust
#[cfg(test)]
mod photo_tests {
    use super::*;

    #[test]
    fn web_has_metadata_rule() {
        // The web can't inspect metadata: every non-PNG counts as carrying some.
        assert!(!web_has_metadata("image/png"));
        assert!(web_has_metadata("image/jpeg"));
        assert!(web_has_metadata("image/webp"));
    }

    #[test]
    fn ladder_never_returns_over_limit() {
        // pick_encoded tries the sizes a ladder produced and takes the first within the limit.
        assert_eq!(pick_encoded(&[(85, 900), (75, 700), (65, 500)], Some(600)), Some(65));
        assert_eq!(pick_encoded(&[(85, 900), (75, 800)], Some(100)), None);
        assert_eq!(pick_encoded(&[(85, 900)], None), Some(85));
    }
}
```

- [ ] **Step 2: Run** `cd mobiler-web && CARGO_TARGET_DIR=$PWD/target cargo test --lib photo_tests` → compile errors.

- [ ] **Step 3: Implement.**

```rust
/// The web can't read a file's metadata without parsing it, so for `strip_metadata` every non-PNG
/// counts as carrying some (ADR-0045 negative consequence).
fn web_has_metadata(mime: &str) -> bool {
    mime != "image/png"
}

/// The first quality whose encoded size fits `max_bytes` (`(quality, size)` in ladder order).
fn pick_encoded(tries: &[(u8, u64)], max_bytes: Option<u64>) -> Option<u8> {
    tries.iter().find(|(_, size)| max_bytes.map_or(true, |m| *size <= m)).map(|(q, _)| *q)
}
```

  Refactor `take_image` into `pick_file(capture) -> Result<web_sys::File, &'static str>` that also listens for the
  input's `cancel` event (resolve `Err("cancelled")`); `take_image(capture)` keeps its old behaviour on top of it
  (blob URL of the file). Add:

```rust
async fn take_photo_with(capture: bool, input: &str) -> PluginResponse {
    let opts = mobiler_core::PhotoOptions::from_input(input);
    let file = match pick_file(capture).await {
        Ok(f) => f,
        Err(code) => return PluginResponse::text(false, code),
    };
    match process_photo(&file, &opts).await {
        Ok(photo) => PluginResponse { ok: true, output: photo.encode() },
        Err(code) => PluginResponse::text(false, code),
    }
}

async fn process_photo(file: &web_sys::File, o: &mobiler_core::PhotoOptions) -> Result<mobiler_core::Photo, String> {
    use mobiler_core::photo::{needs_reencode, output_format, quality_ladder, target_size};
    use wasm_bindgen::JsCast;
    let window = web_sys::window().ok_or("unavailable")?;
    let mime = file.type_();
    let size = file.size() as u64;
    // createImageBitmap applies the EXIF orientation by default (imageOrientation "from-image").
    let promise = window.create_image_bitmap_with_blob(file).map_err(|_| "unsupported_image".to_string())?;
    let bitmap: web_sys::ImageBitmap = wasm_bindgen_futures::JsFuture::from(promise).await
        .map_err(|_| "unsupported_image".to_string())?.unchecked_into();
    let (w, h) = (bitmap.width(), bitmap.height());
    let fits = o.max_dimension_opt().map_or(true, |m| w.max(h) <= m) && o.max_bytes_opt().map_or(true, |m| size <= m);
    if !needs_reencode(o.format(), fits, o.strip_metadata(), web_has_metadata(&mime)) {
        let handle = web_sys::Url::create_object_url_with_blob(file).map_err(|_| "unavailable".to_string())?;
        return Ok(mobiler_core::Photo { handle, mime, bytes: size, width: w, height: h });
    }
    let format = output_format(o.format(), mime == "image/png");
    let (tw, th) = target_size(w, h, o.max_dimension_opt());
    let doc = window.document().ok_or("unavailable")?;
    let canvas: web_sys::HtmlCanvasElement = doc.create_element("canvas").map_err(|_| "unavailable")?.unchecked_into();
    canvas.set_width(tw);
    canvas.set_height(th);
    let ctx: web_sys::CanvasRenderingContext2d = canvas.get_context("2d").ok().flatten().ok_or("unavailable")?.unchecked_into();
    ctx.draw_image_with_image_bitmap_and_dw_and_dh(&bitmap, 0.0, 0.0, f64::from(tw), f64::from(th)).map_err(|_| "unsupported_image".to_string())?;
    let lossy = format != mobiler_core::PhotoFormat::Png;
    let ladder = if lossy { quality_ladder(o.quality_value()) } else { vec![100] };
    for q in ladder {
        let blob = canvas_to_blob(&canvas, format.mime(), f64::from(q) / 100.0).await?;
        if o.max_bytes_opt().map_or(true, |m| blob.size() as u64 <= m) {
            let handle = web_sys::Url::create_object_url_with_blob(&blob).map_err(|_| "unavailable".to_string())?;
            return Ok(mobiler_core::Photo { handle, mime: blob.type_(), bytes: blob.size() as u64, width: tw, height: th });
        }
    }
    Err("too_large".into())
}

async fn canvas_to_blob(canvas: &web_sys::HtmlCanvasElement, mime: &str, quality: f64) -> Result<web_sys::Blob, String> {
    use wasm_bindgen::{closure::Closure, JsCast};
    let (tx, rx) = futures_channel::oneshot::channel::<Option<web_sys::Blob>>();
    let tx = std::cell::RefCell::new(Some(tx));
    let cb = Closure::once(move |blob: Option<web_sys::Blob>| {
        if let Some(tx) = tx.borrow_mut().take() {
            let _ = tx.send(blob);
        }
    });
    canvas
        .to_blob_with_type_and_encoder_options(cb.as_ref().unchecked_ref(), mime, &wasm_bindgen::JsValue::from_f64(quality))
        .map_err(|_| "unavailable".to_string())?;
    cb.forget();
    rx.await.ok().flatten().ok_or_else(|| "unavailable".to_string())
}
```

  `pick_encoded` documents the ladder rule and is what the loop does inline; if clippy flags it unused outside tests,
  mark it `#[cfg(test)]` or use it in the loop by collecting tries — prefer using it. Dispatch in `perform`:
  `photo`/`pick` and `camera`/`capture` call `take_image(..)` when `call.input.is_empty()`, else
  `take_photo_with(.., &call.input)`. Exact web-sys method names: verify against the web-sys version in Cargo.lock
  (`create_image_bitmap_with_blob`, `draw_image_with_image_bitmap_and_dw_and_dh`,
  `to_blob_with_type_and_encoder_options`); the compiler names any that differ.

- [ ] **Step 4: Run** `cargo test --lib` and `cargo clippy --target wasm32-unknown-unknown -- -D warnings` in
  `mobiler-web` → green.
- [ ] **Step 5: Commit** `feat(web): photo options pipeline; picker cancel answers`.

### Task 4: Android barbershop shell

**Files:**
- Create: `demos/barbershop/Android/app/src/main/java/dev/mobiler/barbershop/PhotoPipeline.kt`
- Modify: `demos/barbershop/Android/app/src/main/java/dev/mobiler/barbershop/Core.kt` (`PhotoPlugin` ~439,
  `CameraPlugin` ~461, registry ~570)
- Modify: `demos/barbershop/Android/app/build.gradle.kts` (dependency)
- Modify: `demos/barbershop/shared/src/bin/codegen.rs` (`.register_type::<mobiler_core::Photo>()?`)

**Interfaces:**
- Consumes: the input JSON keys and reply codes (Global Constraints); generated Kotlin `Photo(handle: String, mime:
  String, bytes: ULong, width: UInt, height: UInt)`.
- Produces: `PhotoPipeline.process(context: Context, source: Uri, input: String): PluginResponse`.

- [ ] **Step 1:** add `implementation("androidx.exifinterface:exifinterface:1.3.7")` to the app's dependencies.
  (Ruling vs the spec's "ImageDecoder on API 28+": `BitmapFactory` alone decodes HEIF from API 28 and fails on
  26/27, which gives the same behaviour with one code path.)
- [ ] **Step 2:** create `PhotoPipeline.kt`:

```kotlin
package dev.mobiler.barbershop

import android.content.Context
import android.graphics.Bitmap
import android.graphics.BitmapFactory
import android.graphics.Matrix
import android.net.Uri
import android.os.Build
import androidx.exifinterface.media.ExifInterface
import dev.mobiler.barbershop.shared.types.Photo
import dev.mobiler.barbershop.shared.types.PluginResponse
import org.json.JSONObject
import java.io.ByteArrayOutputStream
import java.io.File
import java.util.UUID

/** cx.pick_photo_with / capture_photo_with (ADR-0045): re-encode a picked or captured image to the
 *  asked format and limits, upright and without metadata, into the app's cache. The rules mirror
 *  mobiler_core::photo (needs_reencode, target_size, quality_ladder, output_format). */
internal object PhotoPipeline {
    private class Opts(val format: String, val maxDimension: Int?, val maxBytes: Long?, val quality: Int, val strip: Boolean)

    private fun parse(input: String): Opts {
        val j = runCatching { JSONObject(input) }.getOrElse { JSONObject() }
        return Opts(
            j.optString("format", "original"),
            if (j.has("max_dimension")) j.getInt("max_dimension") else null,
            if (j.has("max_bytes")) j.getLong("max_bytes") else null,
            j.optInt("quality", 85).coerceIn(1, 100),
            j.optBoolean("strip_metadata", true),
        )
    }

    private fun fail(code: String) = PluginResponse(false, code)

    private fun ladder(start: Int): List<Int> {
        val out = mutableListOf(start)
        var q = start
        while (q > 45) { q = maxOf(q - 10, 45); out += q }
        return out
    }

    private fun target(w: Int, h: Int, max: Int?): Pair<Int, Int> {
        val longest = maxOf(w, h)
        if (max == null || max <= 0 || longest <= max) return w to h
        val s = max.toDouble() / longest
        return maxOf(1, Math.round(w * s).toInt()) to maxOf(1, Math.round(h * s).toInt())
    }

    private val metadataTags = listOf(
        ExifInterface.TAG_GPS_LATITUDE, ExifInterface.TAG_MAKE, ExifInterface.TAG_MODEL,
        ExifInterface.TAG_DATETIME_ORIGINAL, ExifInterface.TAG_SOFTWARE,
    )

    fun process(context: Context, source: Uri, input: String): PluginResponse {
        val o = parse(input)
        val cr = context.contentResolver
        val mime = cr.getType(source) ?: "image/jpeg"
        val original = cr.openInputStream(source)?.use { it.readBytes() } ?: return fail("unsupported_image")
        val bounds = BitmapFactory.Options().apply { inJustDecodeBounds = true }
        BitmapFactory.decodeByteArray(original, 0, original.size, bounds)
        if (bounds.outWidth <= 0 || bounds.outHeight <= 0) return fail("unsupported_image") // e.g. HEIC on API 26/27
        val exif = runCatching { ExifInterface(original.inputStream()) }.getOrNull()
        val orientation = exif?.getAttributeInt(ExifInterface.TAG_ORIENTATION, ExifInterface.ORIENTATION_NORMAL) ?: ExifInterface.ORIENTATION_NORMAL
        val swaps = orientation in setOf(ExifInterface.ORIENTATION_ROTATE_90, ExifInterface.ORIENTATION_ROTATE_270, ExifInterface.ORIENTATION_TRANSPOSE, ExifInterface.ORIENTATION_TRANSVERSE)
        val (w, h) = if (swaps) bounds.outHeight to bounds.outWidth else bounds.outWidth to bounds.outHeight
        val hasMetadata = exif != null && (orientation != ExifInterface.ORIENTATION_NORMAL && orientation != ExifInterface.ORIENTATION_UNDEFINED || metadataTags.any { exif.getAttribute(it) != null })
        val fits = (o.maxDimension == null || maxOf(w, h) <= o.maxDimension) && (o.maxBytes == null || original.size <= o.maxBytes)
        val dir = File(context.cacheDir, "photos").apply { mkdirs() }
        if (o.format == "original" && fits && !(o.strip && hasMetadata)) {
            val ext = when (mime) { "image/png" -> "png"; "image/webp" -> "webp"; "image/heic", "image/heif" -> "heic"; else -> "jpg" }
            val out = File(dir, "${UUID.randomUUID()}.$ext").apply { writeBytes(original) }
            return ok(out, mime, original.size.toLong(), w, h)
        }
        val png = o.format == "png" || (o.format == "original" && mime == "image/png")
        val webp = o.format == "webp"
        val (tw, th) = target(w, h, o.maxDimension)
        // Decode at a power-of-two sample first (a 12000×9000 photo must not run out of memory).
        var sample = 1
        while (maxOf(bounds.outWidth, bounds.outHeight) / (sample * 2) >= maxOf(tw, th)) sample *= 2
        val decoded = BitmapFactory.decodeByteArray(original, 0, original.size, BitmapFactory.Options().apply { inSampleSize = sample })
            ?: return fail("unsupported_image")
        val matrix = Matrix().apply {
            when (orientation) {
                ExifInterface.ORIENTATION_ROTATE_90 -> postRotate(90f)
                ExifInterface.ORIENTATION_ROTATE_180 -> postRotate(180f)
                ExifInterface.ORIENTATION_ROTATE_270 -> postRotate(270f)
                ExifInterface.ORIENTATION_FLIP_HORIZONTAL -> postScale(-1f, 1f)
                ExifInterface.ORIENTATION_FLIP_VERTICAL -> postScale(1f, -1f)
                ExifInterface.ORIENTATION_TRANSPOSE -> { postRotate(90f); postScale(-1f, 1f) }
                ExifInterface.ORIENTATION_TRANSVERSE -> { postRotate(270f); postScale(-1f, 1f) }
            }
        }
        val upright = Bitmap.createBitmap(decoded, 0, 0, decoded.width, decoded.height, matrix, true)
        val scaled = if (upright.width == tw && upright.height == th) upright else Bitmap.createScaledBitmap(upright, tw, th, true)
        @Suppress("DEPRECATION")
        val (format, outMime, ext) = when {
            png -> Triple(Bitmap.CompressFormat.PNG, "image/png", "png")
            webp && Build.VERSION.SDK_INT >= Build.VERSION_CODES.R -> Triple(Bitmap.CompressFormat.WEBP_LOSSY, "image/webp", "webp")
            webp -> Triple(Bitmap.CompressFormat.WEBP, "image/webp", "webp")
            else -> Triple(Bitmap.CompressFormat.JPEG, "image/jpeg", "jpg")
        }
        for (q in if (png) listOf(100) else ladder(o.quality)) {
            val bytes = ByteArrayOutputStream().also { scaled.compress(format, q, it) }.toByteArray()
            if (o.maxBytes == null || bytes.size <= o.maxBytes) {
                val out = File(dir, "${UUID.randomUUID()}.$ext").apply { writeBytes(bytes) }
                return ok(out, outMime, bytes.size.toLong(), tw, th)
            }
        }
        return fail("too_large")
    }

    private fun ok(file: File, mime: String, size: Long, w: Int, h: Int): PluginResponse =
        PluginResponse(true, Photo(Uri.fromFile(file).toString(), mime, size.toULong(), w.toUInt(), h.toUInt()).bincodeSerialize().map { it.toUByte() })
}
```

  Match `PluginResponse`'s byte-list constructor and the `toUByteList` helper style already used in `Core.kt`
  (e.g. `Core.kt:153` for `DeviceInfo`); if a `PluginResponse(Boolean, String)` helper exists, use it for `fail`.

- [ ] **Step 3:** `PhotoPlugin` / `CameraPlugin` take the application (`PhotoPlugin(application)`,
  `CameraPlugin(application)` in the registry) and, after getting the URI, branch:

```kotlin
        if (uri == null) return PluginResponse(false, "cancelled")
        if (input.isEmpty()) return PluginResponse(true, uri)
        return withContext(Dispatchers.IO) { PhotoPipeline.process(app, Uri.parse(uri), input) }
```

  and a missing launcher answers `"unavailable"` when `input` is non-empty (the old text stays for the plain call).
- [ ] **Step 4:** register `Photo` in `demos/barbershop/shared/src/bin/codegen.rs` next to `DeviceInfo`.
- [ ] **Step 5: Build** barbershop (`bash Android/build-android.sh`, JDK 21, per-demo target) → BUILD SUCCESSFUL.
- [ ] **Step 6: Commit** `feat(android): photo options pipeline (barbershop)`.

### Task 5: iOS barbershop shell

**Files:**
- Create: `demos/barbershop/iOS/Sources/PhotoPipeline.swift`
- Modify: `demos/barbershop/iOS/Sources/Core.swift` (`PhotoPlugin` ~798, `PhotoPickerDelegate`, `CameraPlugin` ~854)

- [ ] **Step 1:** create `PhotoPipeline.swift`:

```swift
import Foundation
import ImageIO
import UniformTypeIdentifiers

/// cx.pick_photo_with / capture_photo_with (ADR-0045): re-encode a picked or captured image to the asked
/// format and limits, upright and without metadata, into tmp. Mirrors mobiler_core::photo's rules.
enum PhotoPipeline {
    private struct Opts { var format = "original"; var maxDimension: Int?; var maxBytes: Int?; var quality = 85; var strip = true }

    private static func parse(_ input: String) -> Opts {
        var o = Opts()
        guard let data = input.data(using: .utf8), let j = (try? JSONSerialization.jsonObject(with: data)) as? [String: Any] else { return o }
        if let f = j["format"] as? String { o.format = f }
        o.maxDimension = (j["max_dimension"] as? NSNumber)?.intValue
        o.maxBytes = (j["max_bytes"] as? NSNumber)?.intValue
        if let q = (j["quality"] as? NSNumber)?.intValue { o.quality = min(max(q, 1), 100) }
        if let s = j["strip_metadata"] as? Bool { o.strip = s }
        return o
    }

    private static func ladder(_ start: Int) -> [Int] {
        var out = [start], q = start
        while q > 45 { q = max(q - 10, 45); out.append(q) }
        return out
    }

    private static func target(_ w: Int, _ h: Int, _ maxDim: Int?) -> (Int, Int) {
        let longest = max(w, h)
        guard let m = maxDim, m > 0, longest > m else { return (w, h) }
        let s = Double(m) / Double(longest)
        return (max(1, Int((Double(w) * s).rounded())), max(1, Int((Double(h) * s).rounded())))
    }

    static func process(_ url: URL, input: String) -> PluginResponse {
        let o = parse(input)
        guard let src = CGImageSourceCreateWithURL(url as CFURL, nil),
              let props = CGImageSourceCopyPropertiesAtIndex(src, 0, nil) as? [CFString: Any],
              var w = props[kCGImagePropertyPixelWidth] as? Int, var h = props[kCGImagePropertyPixelHeight] as? Int
        else { return PluginResponse(ok: false, output: "unsupported_image") }
        let orientation = props[kCGImagePropertyOrientation] as? Int ?? 1
        if orientation >= 5 { swap(&w, &h) } // 5–8 are the 90°/270° variants
        let utType = (CGImageSourceGetType(src) as String?).flatMap { UTType($0) }
        let mime = utType?.preferredMIMEType ?? "image/jpeg"
        let size = (try? url.resourceValues(forKeys: [.fileSizeKey]).fileSize) ?? 0
        let hasMetadata = orientation != 1 || props[kCGImagePropertyGPSDictionary] != nil || props[kCGImagePropertyExifDictionary] != nil
        let fits = (o.maxDimension.map { max(w, h) <= $0 } ?? true) && (o.maxBytes.map { size <= $0 } ?? true)
        if o.format == "original" && fits && !(o.strip && hasMetadata) {
            return ok(url, mime, size, w, h)
        }
        let png = o.format == "png" || (o.format == "original" && utType == .png)
        let (tw, th) = target(w, h, o.maxDimension)
        let thumbOpts: [CFString: Any] = [
            kCGImageSourceCreateThumbnailFromImageAlways: true,
            kCGImageSourceCreateThumbnailWithTransform: true, // applies the orientation
            kCGImageSourceThumbnailMaxPixelSize: max(tw, th),
        ]
        guard let image = CGImageSourceCreateThumbnailAtIndex(src, 0, thumbOpts as CFDictionary) else {
            return PluginResponse(ok: false, output: "unsupported_image")
        }
        let outType: UTType = png ? .png : .jpeg // iOS can't encode WebP → JPEG
        for q in png ? [100] : ladder(o.quality) {
            let data = NSMutableData()
            guard let dest = CGImageDestinationCreateWithData(data, outType.identifier as CFString, 1, nil) else { break }
            CGImageDestinationAddImage(dest, image, [kCGImageDestinationLossyCompressionQuality: Double(q) / 100] as CFDictionary)
            guard CGImageDestinationFinalize(dest) else { break }
            if o.maxBytes.map({ data.length <= $0 }) ?? true {
                let out = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString + (png ? ".png" : ".jpg"))
                guard (try? data.write(to: out)) != nil else { return PluginResponse(ok: false, output: "unavailable") }
                return ok(out, outType.preferredMIMEType ?? "image/jpeg", data.length, image.width, image.height)
            }
        }
        return PluginResponse(ok: false, output: "too_large")
    }

    private static func ok(_ url: URL, _ mime: String, _ size: Int, _ w: Int, _ h: Int) -> PluginResponse {
        let photo = Photo(handle: url.absoluteString, mime: mime, bytes: UInt64(size), width: UInt32(w), height: UInt32(h))
        return PluginResponse(ok: true, output: (try? photo.bincodeSerialize()) ?? [])
    }
}
```

- [ ] **Step 2:** `PhotoPlugin.handle` passes `input` to `PhotoPickerDelegate`; after the copy to `dest`: `input.isEmpty
  ? PluginResponse(ok: true, output: dest.absoluteString) : PhotoPipeline.process(dest, input: input)`. Cancel answers
  `"cancelled"` (unchanged). `CameraPlugin` with a non-empty input writes `image.jpegData(compressionQuality: 1.0)` to
  tmp and runs `PhotoPipeline.process` on it; unavailable camera answers `"unavailable"` for the `_with` call.
- [ ] **Step 3:** CI compiles iOS (no toolchain here); check the generated Swift initializer label names against
  `DeviceInfo`'s usage in `Core.swift`.
- [ ] **Step 4: Commit** `feat(ios): photo options pipeline (barbershop)`.

### Task 6: barbershop demo

**Files:** Modify `demos/barbershop/app-core/src/lib.rs` (the "Send a file" card ~1103–1131, its Msg and tests)

- [ ] **Step 1: Failing test** in the app-core test module: picking yields `Msg::Picked(Ok(Photo { handle:
  "file:///p.jpg", mime: "image/jpeg", bytes: 1_000, width: 2048, height: 1536 }))` → the upload request's JSON input
  contains `"filename":"photo.jpg"` and `"file_content_type":"image/jpeg"` (inspect `cx.streams`/requests the way the
  existing transfer test does), and `Msg::Picked(Err(PhotoError::TooLarge))` sets a note.
- [ ] **Step 2: Run** → fails (Msg::Picked carries a `PluginResponse` today).
- [ ] **Step 3: Implement:** `cx.pick_photo_with(PhotoOptions::new().jpeg().max_dimension(2048).max_bytes(4 * 1024 *
  1024), Msg::Picked)`; `Msg::Picked(Result<Photo, PhotoError>)`; on `Ok(p)` start the upload with
  `.filename(format!("photo.{}", ext_for(&p.mime)))` and `.file_content_type(&p.mime)` (use the builder's actual
  method names from `mobiler-core/src/transfer.rs`) and note "Uploading 2048×1536 JPEG (1.0 MB)…"; on `Err(e)` a note
  per variant in the demo's own words.
- [ ] **Step 4: Run** barbershop's tests → green. **Commit** `demo(barbershop): Send-a-file uses pick_photo_with`.

### Task 7: runtime verification

Delegate to a subagent with the Global Constraints' build/emulator rules; no code changes except temporary ones
reverted at the end.

- [ ] Android API 36 (`mobiler_pixel7`, 5560) and API 26 (`mobiler_api26`, 5562), barbershop "Send a file" with
  pushed test images (generate on the host with Python/PIL or ImageMagick; push to `/sdcard/Pictures`, media-scan):
  1. a 4000×3000 JPEG with GPS EXIF and orientation 6 → reply JPEG ≤ 4 MB, 1536×2048 (upright portrait), no GPS
     (pull the cached file with `adb exec-out run-as dev.mobiler.barbershop cat cache/photos/<f>` and check with
     `exiftool` or PIL) — Review Focus 1;
  2. a 1.5 MB PNG screenshot picked with a TEMPORARY `.original().max_bytes(4 MB)` variant → sha256 equals the pushed
     file;
  3. a HEIC (generate with `pillow-heif` if available, else skip and say so) → JPEG on API 36, `unsupported_image`
     on API 26;
  4. a temporary `max_bytes(10_000)` → `too_large`, no file — Review Focus 2;
  5. a 12000×9000 JPEG → no crash, result 2048×1536 — Review Focus 5;
  6. cancel the picker → the demo's cancelled note.
- [ ] Web (headless Chrome via CDP, barbershop web build): a JPEG with orientation 6 → upright, within limits; PNG
  pass-through (`Original`); the file dialog's cancel answers `cancelled` (no hang) — Review Focus 4.
- [ ] Record results; revert temporary edits.

### Task 8: reviews, PR A, publish

- [ ] Fresh independent code review + security review of the branch (most capable model). Fix Critical/Important
  test-first; re-review new logic.
- [ ] `ship-pr`: PR body with the request's substance, the verification, and "READMEs checked" (grep
  `pick_photo`, `capture_photo`, `photo` in READMEs and `capabilities.json`; update the capability row + `gen-readme`,
  the core README feature list). Fill ADR-0045's Deciding PRs.
- [ ] `release-libs`: core 0.43.0, web 0.43.0 (web requires core 0.43), ui unchanged — maintainer approval first.

## PR B: CLI (after core/web 0.43 are on crates.io)

### Task 9: template port

**Files:** template Android `Core.kt`, new `PhotoPipeline.kt` (package `{{PACKAGE}}`, imports
`{{PACKAGE_SHARED_TYPES}}.Photo` / `.PluginResponse`), `app/build.gradle.kts` (exifinterface), template iOS
`Core.swift` + new `PhotoPipeline.swift`, template `shared/src/bin/codegen.rs` (`register_type::<mobiler_core::Photo>()`
above the anchor), `shared/Cargo.toml.tmpl` (`mobiler-core = "0.43"`).

- [ ] Port by patch from barbershop's diff (ADR-0015); compare change sets with the package placeholders normalised.
- [ ] `cargo test -p mobiler -p xtask`; scaffold with the workspace CLI into the scratchpad and `mobiler build
  android` → APK; delete it.
- [ ] Commit `feat(cli): template photo options pipeline; pins mobiler-core 0.43`.

### Task 10: ship, release, post-release

- [ ] Fresh review of the port; `ship-pr`; CLI version bump (minor 0.64.0) and `release-cli` with the maintainer's
  approval (hand them the merge and tag commands).
- [ ] `post-release`: fresh scaffold builds; an upgrade from 0.63.0 gains `PhotoPipeline.kt`/`.swift` and the codegen
  line and builds; delete `docs/photo-pick-options.md`; update `start.md` and memory.
