# Mobiler HTTP Capability — multipart/form-data upload

**Date:** 2026-07-20
**Status:** design approved, not yet implemented
**Builds on:** Release B streaming transfers (`2026-07-18-mobiler-http-capability-b-streaming-design.md`; shipped core 0.33 / web 0.33.1 / CLI 0.49.0)

## Motivation

Release B shipped **raw-body-only** upload: the file *is* the request body (`PUT`/`POST`).
That serves Stoa (raw encrypted blobs) and presigned S3/GCS PUTs. It does **not** serve
the other dominant upload shape — a classic `multipart/form-data` endpoint (an HTML
`<form enctype="multipart/form-data">`, or an API taking a file plus metadata fields in
one request). Release B deferred this explicitly: *"addable later as a `.multipart()`
builder option with no ABI change."* This is that follow-up.

## Scope decision: streaming upload only

Multipart lives on **`cx.upload`**, not on `cx.request`.

- Multipart's real use is **file upload to a multipart endpoint** — exactly the streaming
  path. A large file must stream from disk; `cx.request` builds the whole body in the core
  as one `Vec<u8>`, the precise thing Release B exists to avoid.
- `cx.request` already covers the no-file case via a JSON body — no gap there.
- A small in-memory multipart form with no file is a niche `cx.request` could grow later
  if a concrete consumer appears. YAGNI now.

**v1 = exactly one streamed file part + N text fields.** Multiple file parts per request
are deferred (rare; each needs its own handle).

## Rust API

```rust
cx.upload(url, source)                    // source: path / content:// / file:// / blob:
    .multipart("file")                    // the file becomes a form-data part named "file"
    .filename("photo.jpg")                // optional; default = last path segment of source
    .file_content_type("image/jpeg")      // optional; default = "application/octet-stream"
    .field("title", "My Photo")           // extra text fields, repeatable
    .field("album", "vacation")
    .bearer(&token)
    .start(key, on_event);
```

- **`.multipart(field)`** switches the body from raw to `multipart/form-data` and flips the
  **default method to POST** (only if the caller hasn't already called `.method(...)`).
  Multipart endpoints are POST, not PUT. No-op on `cx.download` (download has no body),
  mirroring how `.method` is upload-only today.
- **`.filename(name)`** / **`.file_content_type(ct)`** override the file part's
  `filename=` and its `Content-Type`. Defaults: filename = the last `/`-segment of the
  source handle (query/fragment stripped); content-type = `application/octet-stream`.
- **`.field(name, value)`** appends a text part; repeatable, order preserved.
- Without `.multipart(...)`, upload is unchanged raw-body (Release B behavior).

Signatures added to `TransferBuilder` (upload only; each is a no-op when the builder is a
download):

```rust
pub fn multipart(self, field: impl Into<String>) -> Self;
pub fn filename(self, name: impl Into<String>) -> Self;
pub fn file_content_type(self, ct: impl Into<String>) -> Self;
pub fn field(self, name: impl Into<String>, value: impl Into<String>) -> Self;
```

## Envelope — no ABI change

The multipart config rides as an optional field on the existing `TransferReq` JSON
(`PluginCall.input` stays a `String`; no `PluginCall`/`PluginResponse`/`TransferEvent`
struct change, so **no shared-type ABI break** and no codegen change):

```rust
#[derive(Serialize)]
struct Multipart {
    field: String,                       // form field name for the file part
    #[serde(skip_serializing_if = "Option::is_none")]
    filename: Option<String>,            // override; else shell infers from source
    #[serde(skip_serializing_if = "Option::is_none")]
    file_content_type: Option<String>,   // override; else "application/octet-stream"
    fields: Vec<HttpHeader>,             // text fields — reuse HttpHeader { name, value }
}
// TransferReq gains:  #[serde(skip_serializing_if="Option::is_none")] multipart: Option<Multipart>
```

When `multipart` is present, the shell builds a `multipart/form-data` body; when absent,
the shell's existing raw-body path is untouched.

## Shells

All three keep the transfer **streaming** (file never fully in core memory) and keep the
Release B semantics unchanged: ~10/sec progress throttle, terminal `Done` always emitted,
non-2xx → `Done{Response{status}}`, only failure → `Done{TransportError}`, cancel silent
+ partial cleanup.

**Web (`mobiler-web`, clean).** Build a `FormData`: `form.append(field, blob, filename)`
for the file part (the picker/photo `blob:` fetched back into a `Blob`) plus
`form.append(name, value)` for each text field, then `xhr.send_with_opt_form_data(&form)`.
The browser sets `Content-Type: multipart/form-data; boundary=...` itself — the shell must
**not** set a `Content-Type` header in multipart mode (it would clobber the boundary).
`xhr.upload().onprogress` still fires. Needs the `web-sys` `FormData` feature.

**Android (`transfer` plugin, clean).** Wrap the existing streaming file-part
`RequestBody` (`streamingUploadBody`) in an OkHttp `MultipartBody.Builder().setType(FORM)`
via `addFormDataPart(field, filename, streamingBody)` plus `addFormDataPart(name, value)`
for text fields. OkHttp streams multipart parts, so the counting sink on the file part
still reports progress. In multipart mode the app-supplied `Content-Type` is not applied
to the whole request (OkHttp sets the multipart type); the file part's content-type comes
from the streaming body's `contentType()` (the override or octet-stream).

**iOS (`transfer` plugin, the one wrinkle).** `URLSessionUploadTask(fromFile:)` uploads a
raw file, not multipart. So the plugin composes a **temp multipart file** on disk:
`--boundary` + each text-field part, then the file part's headers
(`Content-Disposition: form-data; name="field"; filename="..."` + `Content-Type`), then the
source file bytes streamed in, then the closing `--boundary--`. It then does
`uploadTask(fromFile: tempMultipartFile)` with `Content-Type: multipart/form-data;
boundary=...`. This is **disk → disk → socket**: memory stays flat. Progress via
`didSendBodyData` reports the whole envelope's bytes (fine — total includes the small
part-header overhead). The temp file is deleted on completion, cancel, and error (same
partial-cleanup discipline as the download path).

## Resolved decisions

- **Method:** `.multipart()` defaults the method to **POST**; an explicit `.method(...)`
  still wins.
- **Filename inference:** last `/`-segment of the source handle, with any `?query`/
  `#fragment` stripped; empty → `"file"`. Overridable via `.filename()`.
- **File content-type:** `application/octet-stream` default (servers overwhelmingly sniff
  or accept it); overridable via `.file_content_type()`. No extension-based sniffing in v1
  — an override is the escape hatch. **Documented cross-shell difference:** the override
  is honored on iOS/Android; on **web** the browser derives the file part's `Content-Type`
  from the `Blob`'s own MIME type (the picker/photo blob already carries one), so a
  `.file_content_type()` override is native-only in v1. Intentional, not a bug — honoring
  it on web means re-wrapping the Blob, deferred.
- **Text-field order** is preserved; the file part is emitted **last** (typical multipart
  ordering, and lets a server read metadata fields before the large blob).
- **Cancel / throttle / terminal** semantics are exactly Release B's — unchanged.

## Testing

- **mobiler-core (unit):** `.multipart("f").field("k","v")` produces an envelope with the
  `multipart` object (field, fields[]) and method flipped to POST; an explicit `.method`
  before `.multipart` is preserved; `.filename`/`.file_content_type` land in the config;
  `.multipart` on a download is a no-op.
- **Web (integration):** the headless-Chrome recipe against a local server that parses
  `multipart/form-data` — assert the file part arrives under the right field name with the
  right filename, the text fields arrive, progress ticks fire, and the terminal `Done`
  carries the server's status.
- **iOS / Android:** per-demo CI build lanes compile the shells; the barbershop "Send a
  file" card gains a multipart variant so a real multipart upload runs on device (the
  temp-file path on iOS especially wants a device pass).
- **Cross-shell parity check (whole-branch review):** the three shells must agree on the
  wire result — same field name, same filename default, file part last, text fields
  present. Byte-diff a captured multipart body across shells where feasible.

## Release process

Two-stage, same shape as Release B — but **no template/codegen change** (multipart is
envelope-only; no new registered type).

- **PR-A (libs):** `mobiler-core` (builder methods + `Multipart` envelope) + `mobiler-web`
  (the `FormData` path) + the barbershop multipart demo variant. Bump `mobiler-core`
  0.33.0 → **0.34.0**, `mobiler-web` 0.33.1 → **0.34.0** (+ its core dep pin). `mobiler-ui`
  untouched. Then `release-libs`.
- **PR-C (CLI):** the `transfer` plugin's iOS temp-file multipart + Android `MultipartBody`
  path, re-synced into barbershop, core pin bump, `mobiler` 0.49.0 → **0.50.0**. No
  template `codegen.rs` change. Then `release-cli`, tag `v0.50.0`.

Land each with `ship-pr`; the whole-branch review must compare the three shells' multipart
output. Run `post-release` with the **zero-manual-steps** upgrade check.

## Out of scope

- **Multiple file parts** per request (each needs its own handle) — a later addition.
- **Multipart on `cx.request`** (small in-memory forms) — a later addition if a concrete
  consumer needs it.
- **Extension-based content-type sniffing** — the `.file_content_type()` override covers it.
- Everything Release B already excluded: background transfers, resumable/chunked uploads.
