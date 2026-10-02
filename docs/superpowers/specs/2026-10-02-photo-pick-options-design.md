# Photo pick options: format, size and stripped metadata

**Request:** Moj Termin (appointments admin app), 2026-10-01, "options on `pick_photo` / `capture_photo`". The
request doc is temporary and deleted after the release; its substance is restated below.
**Release:** libraries core 0.43 / web 0.43 (new types and calls; the web pipeline), barbershop shells first; then
a CLI release that ports the template shells, adds `androidx.exifinterface` and registers `Photo` in codegen.
**Constrained by:** ADR-0002 (capabilities are `{plugin, op, input}`), ADR-0003 (`PluginResponse` stays `{ok,
output}`), ADR-0004 (typed payloads in `output` via `BincodeFfiFormat`, registered in codegen), ADR-0007
(`pick_photo` / `capture_photo` keep their signature and byte-identical input; new behaviour is a new call), ADR-0008
(types grow by appending), ADR-0009 (template code using new ABI lands after the libraries are published), ADR-0013
(`ok` is honest; a failure carries a reason), ADR-0015 (barbershop first), ADR-0016 (no shell-drawn English: reasons
are machine codes), ADR-0021 (iOS 17), ADR-0027 (bytes never enter the core; the shell returns a handle), ADR-0035
(no new permissions), ADR-0039 (Android minimum 26; newer APIs behind version checks).
**New record:** ADR-0045 (the photo pipeline contract).

## Problem

`cx.pick_photo` / `cx.capture_photo` return the original file: an Android `content://` URI, an iOS temp file, or a
web `blob:`. Phone camera originals are 4–12 MB, HEIC by default on iPhone, and carry EXIF including the GPS position.
Servers commonly accept PNG/JPG/WebP up to a size limit (Moj Termin: 4 MB). The core cannot fix this: it never sees
the pixels (ADR-0027). Each use wants different sizes (a support attachment, a salon logo, a cover photo), so the
choice is per call.

## Decisions (approved 2026-10-02)

### 1. API (mobiler-core)

```rust
cx.pick_photo_with(PhotoOptions::new().jpeg().max_dimension(2048).max_bytes(4 * 1024 * 1024), Msg::Picked);
cx.capture_photo_with(PhotoOptions::new().jpeg().max_dimension(1600), Msg::Captured);
// Msg::Picked(Result<Photo, PhotoError>)

pub struct Photo { pub handle: String, pub mime: String, pub bytes: u64, pub width: u32, pub height: u32 }
pub enum PhotoError { Cancelled, TooLarge, UnsupportedImage, Unavailable, Failed(String) }
```

- `PhotoOptions` is a value type like `Confirm` / `Picker` (`mobiler-core/src/dialog.rs` pattern): private fields,
  `#[must_use]` setters, serialized as a JSON string input (`to_input()`), unset options omitted. Fields:
  - `format: PhotoFormat` — `Original` (default) | `Jpeg` | `Png` | `Webp`; setters `.jpeg()`, `.png()`, `.webp()`,
    `.original()`.
  - `max_dimension: Option<u32>` — the longest side in pixels; never upscales.
  - `max_bytes: Option<u64>`.
  - `quality: u8` — the starting quality for lossy formats, default 85, clamped to 1..=100.
  - `strip_metadata: bool` — default `true`; `.keep_metadata()` turns it off.
- `pick_photo_with` / `capture_photo_with` call the same plugin and op as the plain calls (`photo`/`pick`,
  `camera`/`capture`) with the options as input. `pick_photo` / `capture_photo` are unchanged, input `""`
  (ADR-0007).
- **Reply:** on success `ok: true` and `output` = a bincoded `Photo` (`BincodeFfiFormat`, like `DeviceInfo`); on
  failure `ok: false` and `output` = a machine code: `cancelled`, `too_large`, `unsupported_image`, `unavailable`, or
  any other text (→ `Failed(text)`). The app writes its own wording (ADR-0016). A shell that predates the options
  ignores the input and answers a plain URI string: an `ok: true` reply that doesn't decode as `Photo` becomes
  `Failed("unsupported shell")`, never a wrong `Photo`.
- `Photo.mime` is what the shell actually produced (e.g. `image/png` where a browser can't encode WebP), and the handle
  is a new file in the shell's cache that `cx.upload` and `image()` accept.

### 2. The pipeline (identical rules on every shell — ADR-0045)

1. **Pass-through.** If `format` is `Original`, the image already fits `max_dimension` and `max_bytes`, and
   `strip_metadata` is off or the file carries no metadata, the original file is returned untouched (a copy into the
   cache where the original is a transient grant). A PNG screenshot with `Original` + `max_bytes` arrives
   byte-identical.
2. **Otherwise re-encode:** decode; apply the EXIF orientation to the pixels (portraits stay upright); scale so the
   longest side ≤ `max_dimension`; encode in the requested format. `Original` that must be re-encoded keeps PNG as PNG
   and writes everything else (HEIC, WebP, …) as JPEG.
3. **`max_bytes`** with a lossy format (JPEG, WebP): start at `quality`, then step down by 10 to a floor of 45 until
   the result fits; if it still doesn't, `ok: false` / `too_large`. PNG over `max_bytes`: `too_large` (no quality to
   lower). The pipeline never silently returns a file over `max_bytes`.
4. **Metadata:** a re-encoded file carries no EXIF, GPS or camera data whatever `strip_metadata` says (the encoders
   don't copy it); `strip_metadata` only decides whether a file that could pass through must be re-encoded to drop
   it.
5. **Errors:** a cancelled picker → `cancelled`; an image the platform can't decode → `unsupported_image`; no camera
   / picker → `unavailable`.

### 3. Per shell

- **Android (barbershop, then template):**
  - Dependency `androidx.exifinterface:exifinterface` (orientation and the metadata check, from a stream).
  - Decode with `ImageDecoder` on API 28+ (HEIC/HEIF supported), else `BitmapFactory`; HEIC on 26/27 →
    `unsupported_image`. Downscale while decoding (`setTargetSize` / `inSampleSize`) then exact-scale.
  - Encode with `Bitmap.compress`: JPEG; PNG; WebP as `WEBP_LOSSY` on API 30+, else `WEBP`.
  - Output: `cacheDir/photos/<uuid>.<ext>`, returned as a `file://` URI. Pass-through copies the original there (the
    picker URI's read grant is temporary).
  - Work runs off the main thread.
- **iOS (barbershop, then template):**
  - ImageIO: `CGImageSourceCreateThumbnailAtIndex` with `kCGImageSourceCreateThumbnailWithTransform` and
    `kCGImageSourceThumbnailMaxPixelSize` (the shell already has this for `image()`), so orientation is applied and
    the size capped in one step; metadata check via `CGImageSourceCopyPropertiesAtIndex`.
  - Encode with `CGImageDestination` (JPEG / PNG; WebP is not encodable on iOS → JPEG, reported in `mime`).
  - Output: `tmp/<uuid>.<ext>`, `file://`. Capture goes through the same pipeline from the camera image.
- **Web (mobiler-web):**
  - `createImageBitmap(file, {imageOrientation: "from-image"})`, a canvas sized to the scaled image, `toBlob(type,
    quality)`; the result's `blob.type` is the reported MIME (Safari has no WebP encoder → PNG). A metadata check
    isn't possible without parsing, so on the web `strip_metadata` re-encodes any JPEG/HEIC/WebP original and passes
    PNG through.
  - Browsers other than Safari can't decode HEIC → `unsupported_image`.
  - Output: a `blob:` URL.
  - Side fix: cancelling the file dialog currently never answers (only `onchange` is wired); the `cancel` event now
    answers `cancelled` for both the plain and the `_with` calls.
  - web-sys features gain `HtmlCanvasElement`, `CanvasRenderingContext2d`, `ImageBitmap`, `ImageBitmapOptions`,
    `ImageOrientation`.

### 4. Demo

Barbershop's "Send a file" card switches to `pick_photo_with(PhotoOptions::new().jpeg().max_dimension(2048)
.max_bytes(4 << 20))` and fills the upload's filename and content type from `Photo` (today it hard-codes `photo.jpg`
with no content type) and shows the result's size and dimensions.

## Testing

- **mobiler-core:** `PhotoOptions` JSON (defaults omitted, each setter), `Photo` round-trip, `PhotoError` from each
  code and from free text, an undecodable `ok: true` reply → `Failed`, the `_with` calls' plugin/op/input, and the
  existing `cx_pick_and_capture_photo_request_the_right_plugin` unchanged.
- **mobiler-web:** the pure parts (target size from `max_dimension`, the quality ladder, MIME/extension mapping,
  options parsing) as unit tests; the canvas path in headless Chrome.
- **Android emulator (API 36 and 26):** Moj Termin's checks with pushed test images: a large JPEG with GPS EXIF and a
  portrait orientation tag → JPEG ≤ 4 MB, no GPS (`exiftool`), upright; a PNG screenshot with `Original` +
  `max_bytes` → byte-identical (sha256); a HEIC → JPEG on API 36, `unsupported_image` on API 26; an image that can't
  fit → `too_large`; cancel → `cancelled`.
- **Web (headless Chrome):** JPEG with EXIF orientation → upright, re-encoded within limits; PNG pass-through; cancel.
- **iOS:** CI compile; the logic mirrors the tested shells.
- **ADR-0045 conformance:** the pipeline's shared rules are pinned by the web unit tests (the quality ladder, the
  pass-through rule, never-over-`max_bytes`), each proven by mutation.

## Moj Termin's acceptance checks

- A 12 MB iPhone HEIC camera photo, picked with `Jpeg / 2048 / 4 MB`, is accepted by their upload (a JPEG under 4 MB).
- The uploaded JPEG has no GPS EXIF.
- A portrait photo arrives upright.
- A 1.5 MB PNG screenshot with `Original` + `max_bytes` arrives byte-identical.
