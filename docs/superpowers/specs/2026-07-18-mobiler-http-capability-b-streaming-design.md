# Mobiler HTTP Capability — Release B: streaming transfers

**Date:** 2026-07-18 (sketch) · **Promoted to full spec:** 2026-07-20
**Status:** design approved, not yet implemented
**Depends on:** `2026-07-18-mobiler-http-capability-a-design.md` (shipped: core 0.32 / web 0.32 / CLI 0.48.1)

## Motivation

Release A covers request/response for small and medium payloads: the whole body
crosses the FFI boundary as one `Vec<u8>`. That is wrong for large transfers, and
mobiler already has the hole — the `files` plugin performs downloads **natively**,
bypassing `cx.http`, precisely because a 20MB file cannot sensibly cross the boundary
as a single value.

Stoa hits the same wall from the other direction. As an E2EE messenger it needs media
attachments, voice messages, and avatars — all client-side encrypted before upload,
making them uniformly random, incompressible bytes. A 20MB video is the normal case,
not the edge.

## The central decision: paths and handles, not bytes

The core must never hold the payload. Large transfers pass **string handles**; the
bytes go disk → socket (upload) or socket → disk (download) entirely on the native
side.

Consequences:

- Memory stays flat regardless of payload size, on every shell.
- `PluginCall.input` stays a `String` — a handle is text. This is exactly why Release
  A's input-side asymmetry was sound rather than a shortcut.
- Base64 never enters the picture; there is nothing to encode.

### A handle is already a string on every shell

The grounding decision, and the resolution of the sketch's largest open question. A
"file handle" is **already a plaintext string** everywhere in the shipped code:

| Shell | Handle produced by picker/photo | Written by files plugin |
|---|---|---|
| Android | `content://…` URI (`FilePickerPlugin.kt`) | `filesDir`-relative path |
| iOS | `file://…` (`url.absoluteString`) | `.documentDirectory`-relative path |
| Web | `blob:…` object URL (`take_image` → `Url::create_object_url_with_blob`) | — (no filesystem) |

So `FilePath` needs **no new opaque type**. It is a `String` the shell knows how to
open. It already interoperates with the `filepicker`, `photo`/`camera`, and `files`
plugins, which all produce or consume exactly these strings.

## It rides an existing primitive

A transfer emits progress repeatedly and then terminates — the exact shape of the C5
streaming primitive (`cx.subscribe` / `cx.unsubscribe` / `Effect::PluginStream`, Crux
multi-resolve; `PluginStreamCall { key, plugin, op, input }`, delivered as a repeated
`StreamContinuation = Fn(PluginResponse) -> E`).

| Transfer concept | Existing primitive |
|---|---|
| progress events | stream events |
| completion | terminal stream event |
| **cancel the transfer** | `cx.unsubscribe(key)` → shell cancels the task |

Cancellation comes essentially for free (Android already does `streamJobs[key].cancel()`
on unsubscribe; iOS/web tear down their source). This matters — an uncancellable 20MB
upload on a mobile connection is a bug, and the repo's "don't defer primitive
lifecycle" lesson says teardown belongs in v1. This is a **second consumer of an
existing subsystem, not a new subsystem.**

## ABI shape — not a struct change

Release A established the pattern: a rich payload rides *inside* `PluginResponse.output`
as bincode, registered as a shared type; the fixed narrow-waist structs
(`PluginCall`/`PluginResponse`) do not change. Release B follows it exactly.

Each stream event is one of:

```rust
#[derive(Facet, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[repr(C)]
pub enum TransferEvent {
    /// A progress tick. `total` is None when the size is unknown (chunked response).
    Progress { transferred: u64, total: Option<u64> },
    /// The transfer finished. Carries Release A's HttpOutcome — the app sees the same
    /// status/headers/body vocabulary it already knows. For download, `body` is empty
    /// (the bytes went to disk); the delivered handle is in the accompanying field.
    Done { outcome: HttpOutcome, handle: Option<String> },
}
```

`handle` on `Done` carries the download destination the shell actually wrote (the
sandbox path on native, the `blob:` URL on web). It is `None` for uploads.

Encoded via `crux_core::bridge::BincodeFfiFormat` (crux's `=1.3` fixint format — **never
bincode directly**, per Release A's hard-won lesson) into the stream's
`PluginResponse.output`, and registered in every app's `codegen.rs` with
`.register_type::<TransferEvent>()?` so the Swift/Kotlin decoders exist. `HttpOutcome` is
already registered from Release A; `TransferEvent` nests it, so registering
`TransferEvent` is sufficient.

**No `PluginCall`/`PluginResponse` change → no shared-struct break.** Still a two-stage
release, because the core gains builders and the three shells gain a streaming
`transfer` source (same release shape as Release A).

## Rust API

A builder that terminates in a subscription, mirroring how the app already consumes
`cx.subscribe` — the app matches on `TransferEvent`, exactly as Release A's callers
match on `HttpOutcome`:

```rust
// Upload: the file at `handle` becomes the raw request body (PUT by default).
cx.upload(url, handle)                       // handle: content:// | file:// | blob: | sandbox path
    .method("PUT")                           // optional; default PUT
    .bearer(&token)
    .header("Content-Type", "application/octet-stream")
    .start("upload-42", |ev| match ev {      // caller-chosen key; returns the key
        TransferEvent::Progress { transferred, total } => Event::UpProgress { transferred, total },
        TransferEvent::Done { outcome, .. }            => Event::UpDone(outcome),
    });

// Download: bytes stream to `dest` (a sandbox path on native; a filename hint on web).
cx.download(url, dest)
    .bearer(&token)
    .start("dl-42", |ev| match ev {
        TransferEvent::Progress { transferred, total } => Event::DlProgress { transferred, total },
        TransferEvent::Done { outcome, handle }        => Event::DlDone { outcome, handle },
    });

// Cancel either, any time:
cx.unsubscribe("upload-42");
```

Signatures (final naming pinned during planning against the shipped `Cx`):

```rust
impl<E> Cx<E> {
    pub fn upload(&mut self, url: impl Into<String>, source: impl Into<String>) -> TransferBuilder<'_, E>;
    pub fn download(&mut self, url: impl Into<String>, dest: impl Into<String>) -> TransferBuilder<'_, E>;
}
impl<'a, E> TransferBuilder<'a, E> {
    pub fn method(self, m: impl Into<String>) -> Self;   // upload only; default PUT
    pub fn header(self, name: impl Into<String>, value: impl Into<String>) -> Self;
    pub fn bearer(self, token: impl AsRef<str>) -> Self;
    /// Subscribe under `key`; `on_event` fires per progress tick and once at Done.
    /// Returns `key` so the caller can `cx.unsubscribe(key)`.
    pub fn start(self, key: impl Into<String>, on_event: impl Fn(TransferEvent) -> E + Send + 'static) -> String;
}
```

`start` builds the request envelope JSON (`{url, source|dest, method, headers}`) into
`input` and calls `cx.subscribe(key, "transfer", "upload"|"download", input, decode+dispatch)`.

**Upload is raw-body only in v1** (the file *is* the body). This is what Stoa needs (it
PUTs raw encrypted blobs to its own DS) and what S3/GCS presigned-URL uploads need.
`multipart/form-data` is deferred; it can arrive later as a `.multipart(field_name)`
builder option with no ABI change — a different request framing in the shell, nothing
more.

## The `transfer` plugin (streaming)

A new streaming plugin named `transfer`, registered via the existing
`register_stream` / `mobiler:plugins-stream` machinery (the websocket/system precedent).
Ops: `upload`, `download`.

### iOS
`URLSessionUploadTask(fromFile:)` / `URLSessionDownloadTask` with a
`URLSessionTaskDelegate`: `didSendBodyData` (upload progress) / `didWriteData` (download
progress), `didCompleteWithError` / `didFinishDownloadingTo` (terminal). Upload reads
directly from the file URL (no in-memory copy). Download hands back a temp URL, which
the plugin moves to `dest` (overwriting) and reports as the `handle`.

### Android
OkHttp. Upload: a custom `RequestBody` that streams from the source `Uri`/path and
reports bytes as it writes. Download: `response.body.source()` copied to the `dest`
sink in a loop, reporting bytes. The `callbackFlow` + `streamJobs[key].cancel()` path
already cancels the coroutine on `unsubscribe`; the copy loop must check
`isActive`/`ensureActive` so cancel actually stops the socket read and triggers cleanup.

### Web — two documented asymmetries
1. **`fetch()` has no upload progress.** Only `XMLHttpRequest`'s `upload.onprogress`
   does. So web **upload** uses `XHR`; web **download** uses `fetch` + `ReadableStream`
   (which does report progress via the reader). This inconsistency is deliberate and
   must be commented at both call sites so no one "unifies" them later.
2. **No filesystem.** Web `download` streams into an in-memory/OPFS blob and the `Done`
   event's `handle` is a `blob:` URL — the same shape `take_image` returns, consumable
   by an `<img>`/renderer or a share/save action. The `dest` argument is treated as a
   filename hint on web, honored literally only on iOS/Android. Web `upload` source is a
   `blob:` URL (from the picker/photo plugin); the shell `fetch`es it back into a `Blob`
   and sends it as the XHR body.

## Resolved design decisions

- **Overwrite policy:** `download` overwrites `dest` by default; no policy argument in
  v1 (matches the `files` plugin's `write`). A policy arg can be added later without an
  ABI change.
- **Progress throttling:** the shell throttles progress to ~10/sec (or emit on ≥1%
  change), because every event round-trips the FFI *and* triggers a full view render.
  The terminal `Done` is always emitted. Throttling is shell-side; the core sees
  whatever the shell forwards.
- **Partial artifacts:** on cancel or transport failure, the shell **deletes** the
  partially-written download file — no resume in v1, and a half-written file that looks
  complete is a footgun. Upload has no artifact to clean (the source is the user's
  file).
- **Terminal semantics:** a non-2xx response is still a `Done { outcome: Response {
  status } }`, not an error — the app branches on `outcome.status()` exactly as in
  Release A. Only a failure to obtain any response (offline, cancelled mid-flight) is
  `Done { outcome: TransportError }`.

## Relationship to the `files` plugin

Release B subsumes the `files` plugin's native-download workaround. Its current
`download` op does a blocking `URL(url).openStream().copyTo(...)` with no progress and
no cancel (`FilesPlugin.kt:61`). Once `cx.download` exists, that op should delegate to
the shared transfer implementation (or be documented as the fire-and-forget convenience
variant). Confirm during planning that the two agree on sandbox path semantics.

## Testing

- **mobiler-core (unit):** builder emits the expected `PluginStreamCall` (key, plugin
  `transfer`, op, envelope JSON with url/source-or-dest/method/headers); `TransferEvent`
  bincode round-trip via `BincodeFfiFormat`, both variants; the `start` continuation
  decodes a progress tick and a terminal `Done` and dispatches each to `on_event`.
- **Web (integration):** the headless-Chrome/CDP recipe against a local server that
  accepts a large PUT and serves a large GET — assert progress ticks arrive, the
  terminal `Done` carries the right status, a `blob:` handle comes back from download,
  and `unsubscribe` aborts an in-flight transfer (XHR `.abort()` / stream reader
  cancel).
- **iOS / Android:** per-demo CI build lanes cover compilation. One demo gets a device
  pass for a real large upload + download with a visible progress bar (this is also the
  first time the throttling and cancel paths run on hardware).
- **Consumer proof:** wire an upload+download into a demo (barbershop "attach a photo"
  or saldo "export/restore over the network") with a progress UI, so the capability has
  run in a real app before it ships.

## Release process

Two-stage, per the repo's ABI convention (identical shape to Release A).

- **PR-A (libs):** `mobiler-core` (builders + `TransferEvent`), `mobiler-web` (the
  `transfer` stream source: XHR upload + fetch/ReadableStream download), the demo
  `codegen.rs` registrations, and a demo consumer with a progress UI. Bump
  `mobiler-core` + `mobiler-web` minor. `mobiler-ui` untouched (transfers are not a UI
  concern). Then `release-libs`.
- **PR-C (CLI):** the `transfer` plugin (`mobiler/plugins/transfer/` iOS + Android +
  toml, registered as a stream plugin), the template `codegen.rs` registration, the
  core pin bump, and the CLI minor bump. Then `release-cli`, tag `vX.Y.Z`.

Land each PR with `ship-pr`; the whole-branch review must specifically compare the three
shells' progress/cancel/terminal semantics (Release A's highest-value findings were
cross-shell divergences). Run `post-release` — including the **zero-manual-steps**
upgrade check that 0.48.1 established.

## Explicitly out of scope

- **Background transfers** (continuing while backgrounded). iOS background `URLSession`
  is a substantial subsystem with its own delegate lifecycle and relaunch-into-
  completion semantics. Its own pass.
- **Resumable / chunked uploads** — needs server-side support Stoa's DS lacks.
- **multipart/form-data** — deferred; addable as a `.multipart()` builder option with no
  ABI change.
- **Concurrent-transfer scheduling / prioritisation** — platform defaults apply.
