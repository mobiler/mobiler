# ADR-0045: A photo picked or captured with options is re-encoded by the shell under one set of rules — passed through untouched only when its format is kept, it fits both limits and it is proven free of metadata (a PNG with only harmless chunks) or metadata is kept; otherwise upright, capped (longest side, 16 MP), encoded (Original keeps PNG, else JPEG), stepped down in quality to 45 to fit or refused, and without metadata — and the reply is a typed Photo or a machine-code reason

Status:        Accepted
Date decided:  2026-10-02
Deciding PRs:  #276
Supersedes:    none
Code anchor:   mobiler-core/src/photo.rs (needs_reencode, target_size, quality_ladder, output_format, Photo, PhotoError, PhotoOptions), the shells' photo pipelines (Android PhotoPipeline.kt, iOS PhotoPipeline.swift, mobiler-web process_photo)
Conformance:   mobiler-core/src/photo.rs::pass_through_only_when_original_fits_and_clean, mobiler-core/src/photo.rs::quality_ladder_steps_down_to_45, mobiler-core/src/photo.rs::target_size_caps_the_longest_side_and_never_upscales, mobiler-core/src/photo.rs::target_size_caps_the_pixel_count, mobiler-core/src/photo.rs::original_keeps_png_and_writes_everything_else_as_jpeg, mobiler-core/src/photo.rs::only_a_png_with_harmless_chunks_is_clean

## 1. Context (The Problem)

`cx.pick_photo` and `cx.capture_photo` return the original file. The appointments team (Moj Termin request,
2026-10-01, a temporary request document, so quoted here): "Real originals are 4–12 MB from a phone camera; HEIC by
default on iPhone; carrying EXIF, including the GPS position where the photo was taken. Our server (like most) accepts
PNG/JPG/WebP up to a size limit (4 MB for us)." And: "Phone camera photos, however, fail our server's checks almost
every time."

The core cannot fix this: it never sees the pixels (ADR-0027). Only a shell can decode and re-encode, so all three
shells must do it, and an app needs them to agree on what it gets back.

## 2. Hypothesis

If every shell follows the same rules for `pick_photo_with` / `capture_photo_with`, and the rules live once in
mobiler-core as tested functions, then:

- an app gets a file its server accepts (format and size) on every shell, or a reason it can show;
- a photo never leaks its GPS position by default;
- a file that is already acceptable (a PNG screenshot) passes through byte-identical;
- a file over `max_bytes` is never returned.

### 2.1. Refutation Conditions

- **Condition 1 — pass-through only when nothing must change.**
  - **Validation Metric:** `pass_through_only_when_original_fits_and_clean`.
- **Condition 2 — the quality ladder floors at 45.**
  - **Validation Metric:** `quality_ladder_steps_down_to_45`.
- **Condition 3 — never upscale; the longest side is capped.**
  - **Validation Metric:** `target_size_caps_the_longest_side_and_never_upscales`.
- **Condition 4 — Original keeps PNG, writes everything else as JPEG.**
  - **Validation Metric:** `original_keeps_png_and_writes_everything_else_as_jpeg`.
- **Condition 5 — metadata detection fails closed.** With `strip_metadata`, only a file proven clean passes
  through: a PNG by its signature bytes (not its name or declared type) whose every chunk is on an allow-list
  (pixels, palette, transparency, colour description). A JPEG, WebP or HEIC, a PNG with `eXIf`/`tEXt`/`iTXt`/`zTXt`,
  or a truncated file is re-encoded.
  - **Validation Metric:** `only_a_png_with_harmless_chunks_is_clean`.
- **Condition 6 — at most 16 MP.** A re-encoded image is scaled to at most `MAX_PIXELS` even without
  `max_dimension`, which bounds decode memory and stays within Safari's canvas limit.
  - **Validation Metric:** `target_size_caps_the_pixel_count`.

The shells' native code (Kotlin, Swift) mirrors these functions and is checked by review and by emulator runs of the
team's acceptance checks; the web shell calls them directly.

## 3. Considered Options & Rationale for Refutation

- **Option A — the app re-encodes** `[reconstructed]`
  Rejected: the core never holds the pixels (ADR-0027), so an app can't.
- **Option B — one global setting** `[recorded: Moj Termin request, 2026-10-01, quoted here]`
  The team: "Each wants different sizes, so the choice should be per call, not one global setting." Rejected.
- **Option C — return the best effort even if it exceeds `max_bytes`** `[reconstructed]`
  Rejected: the server rejects it anyway, and the app can't tell before uploading. A `too_large` reason lets the app
  say so.
- **Option D — the rules above, per call, typed reply** `[recorded: docs/superpowers/specs/2026-10-02-photo-pick-options-design.md, decisions 1–2]`
  Chosen. The reply is a bincoded `Photo` (ADR-0004); failures are machine codes the app words itself (ADR-0016);
  `pick_photo` / `capture_photo` are unchanged (ADR-0007).

## 4. Decision & Rationale for Corroboration

Option D. `PhotoOptions` is a JSON input on the existing `photo`/`pick` and `camera`/`capture` ops (an empty input is
the old call). Metadata is stripped by default; a re-encoded file never carries any. Reported dimensions are upright.

Metadata detection fails closed (Condition 5). A first version used per-shell heuristics (a list of EXIF tags on
Android, the `{GPS}`/`{Exif}` dictionaries on iOS, "every non-PNG" on the web); the security review found each could
miss location data (XMP-only or IPTC-only files, a PNG with an `iTXt` packet, a JPEG renamed `.png` on the web), and
Android treated a file it couldn't parse as clean. The byte-level allow-list replaced them on every shell. An
intermediate copy of the original (the iOS picker copy, the Android and iOS camera capture) is deleted after a
re-encode.

**Mutation proof:**
- `needs_reencode` without the `strip_metadata && has_metadata` term failed
  `pass_through_only_when_original_fits_and_clean`.
- A ladder floor of 35 failed `quality_ladder_steps_down_to_45`.
- `target_size` without the `longest > m` guard (so it upscales) failed
  `target_size_caps_the_longest_side_and_never_upscales`.
- `output_format(Original, png)` returning JPEG failed `original_keeps_png_and_writes_everything_else_as_jpeg`.
- Adding `iTXt` to the allow-list failed `only_a_png_with_harmless_chunks_is_clean`.
- Skipping the PNG signature check (clean chunks behind a JPEG signature) failed it too.
- Disabling the pixel cap failed `target_size_caps_the_pixel_count`.
- Reverting restored green.

## 5. Consequences (Positive and Negative Predictions)

- **Positive:** uploads of camera photos work on servers with format and size limits; location data is dropped by
  default.
- **Negative:** HEIC can't be decoded on Android 8.0–8.1 or in browsers other than Safari (`unsupported_image`).
- **Negative:** iOS can't encode WebP (JPEG instead) and Safari's canvas can't (PNG instead); `Photo.mime` says what
  was produced.
- **Negative:** with `strip_metadata` (the default) every JPEG, WebP and HEIC is re-encoded, even a clean one, and so is
  a PNG with any text chunk: an iOS screenshot carries an XMP "Screenshot" note, so it is re-encoded losslessly (same
  pixels, not byte-identical). `.keep_metadata()` lets such files pass through.
- **Negative:** a photo over 16 MP is scaled down to 16 MP even without `max_dimension`.
- **Negative:** a transparent PNG written as JPEG gets an opaque (black) background.
- **Negative:** the produced files in the cache are not pruned by the framework; the OS may evict them.
- **Negative:** the quality floor of 45 can make an aggressive `max_bytes` fail where scaling down further would fit;
  the app can lower `max_dimension`.
- **Negative:** the plain `pick_photo` / `capture_photo` still return originals with their metadata.
- **Negative:** the native rules are copies of the core's functions, kept in step by review, not by a shared test.
