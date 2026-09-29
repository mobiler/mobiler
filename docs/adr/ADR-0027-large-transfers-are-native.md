# ADR-0027: A transfer's bytes never enter the core; the core passes a string handle (path, `content://`, `file://`, `blob:`) and a native plugin moves the bytes between disk and socket

Status:        Accepted
Date decided:  2026-06-08
Deciding PRs:  #135 (first instance: the `files` plugin's native `download` op); #195 (2026-07-20) makes it the rule for transfers: `cx.upload` / `cx.download` + `TransferEvent` + web shell; #196 (2026-07-20) the native `transfer` plugin
Supersedes:    none
Code anchor:   mobiler-core/src/transfer.rs (TransferBuilder, TransferEvent), mobiler-core/src/lib.rs (Cx::upload, Cx::download), mobiler/plugins/transfer/**, mobiler/plugins/files/** (`download` op), mobiler-web/src/lib.rs (the `("transfer", "upload" | "download")` stream source)
Conformance:   mobiler-core/src/lib.rs::upload_builder_emits_transfer_stream_call, mobiler-core/src/lib.rs::download_builder_uses_dest_and_no_default_method

## 1. Context (The Problem)

`cx.http` returns the whole response body to the core as one value. Before the full-REST work
that body was a `String`; since ADR-0003 it is a `Vec<u8>` inside `HttpOutcome`. Either way it
crosses the FFI in one piece and sits in core memory. That is fine for JSON. It is not fine for a
20 MB video or an encrypted attachment: the value is copied across the boundary, held whole, and
there is no progress and no cancel.

The first capability to hit this was the `files` plugin (#135). Its `download` op fetched the URL
natively and wrote the file itself, instead of going through `cx.http`. The Release A spec later
named the reason: the `files` plugin downloads
`[recorded: docs/superpowers/specs/2026-07-18-mobiler-http-capability-a-design.md]`
"natively precisely because `cx.http`'s String body could not carry them." Stoa then needed large
uploads as well as downloads.

## 2. Hypothesis

If large transfers pass only a **string handle** through the core, and a native plugin moves the
bytes disk → socket (upload) or socket → disk (download), then:

- on Android and iOS, memory stays flat whatever the file size;
- `PluginCall.input` can stay a `String` (ADR-0003), because a handle is text;
- no new ABI type is needed: the handles the `filepicker`, `photo` and `files` plugins already
  produce (`content://`, `file://`, `blob:`, sandbox-relative paths) are the currency;
- progress and cancel come from the streaming primitive (ADR-0006), with no new effect.

### 2.1. Refutation Conditions

- **Condition 1 — an upload passes a handle, not bytes.** `cx.upload` emits a stream call to the
  `transfer` plugin whose `input` names the source by handle; it does not issue a `cx.http`
  request with a body.
  - **Validation Metric:** `upload_builder_emits_transfer_stream_call` in
    `mobiler-core/src/lib.rs` (asserts one stream, plugin `transfer`, op `upload`, and
    `input.source == "file:///tmp/a.enc"`).
- **Condition 2 — a download names a destination, not a returned body.** `cx.download` sends the
  `dest` handle to the shell; the result comes back as a handle in `TransferEvent::Done`.
  - **Validation Metric:** `download_builder_uses_dest_and_no_default_method` in
    `mobiler-core/src/lib.rs`.
- **Condition 3 — the native side really streams.** A shell that reads the whole file into memory
  before sending still passes both tests above. Review checks this: the Android `transfer` plugin
  streams through a custom OkHttp `RequestBody` and a copy loop, iOS uses
  `URLSessionUploadTask(fromFile:)` / `URLSessionDownloadTask`. Nothing mechanical guards it.

## 3. Considered Options & Rationale for Refutation

- **Option A — large bodies through `cx.http` as one value** `[recorded: PR #195 body, "Release A moves a whole body across the FFI as one `Vec<u8>` — wrong for a 20MB video."]`
  Rejected: memory grows with the file, and a one-shot request has no progress or cancel.
- **Option B — bytes in the request, e.g. base64 in `PluginCall.input`** `[reconstructed]`
  The streaming spec notes the chosen design's consequence, "Base64 never enters the picture; there
  is nothing to encode.", but does not weigh this option. Rejected: it inflates the payload and still holds it whole in the core. Changing `input` to bytes
  would touch every plugin parser on every shell (ADR-0003).
- **Option C — stream the bytes through the core in chunks over `cx.subscribe`** `[reconstructed]`
  Not chosen: every chunk would cross the FFI and wake `update`, for data the core never reads.
- **Option D — a new opaque file-handle type in the ABI** `[recorded: same spec, "So `FilePath` needs **no new opaque type**."]`
  Rejected: every shell already represents a file as a plain string.
- **Option E — handles through the core, bytes on the native side** `[recorded: same spec, "The core must never hold the payload."]`
  Chosen.

## 4. Decision & Rationale for Corroboration

Option E. The `files` plugin's `download` (#135, "`download` (native binary URL→file)") was the
first instance. Release B made it the rule: `cx.upload(url, source)` / `cx.download(url, dest)`
build a `TransferReq` of handles and call `cx.subscribe(key, "transfer", op, input, …)`; each
`TransferEvent` (`Progress` / `Done { outcome, handle }`) rides inside `output` as
`BincodeFfiFormat` bytes (ADR-0003, ADR-0004). For a download, `Done`'s `HttpOutcome` body is
empty and `handle` is where the shell wrote the file. `cx.unsubscribe(key)` cancels (ADR-0006).

The web shell has no filesystem, so it downloads into a `Blob` and returns a `blob:` URL. The
app's update code never sees the bytes, but the shell collects every chunk into a `Vec<u8>` in the
same wasm memory before building the `Blob`, so web downloads are not flat (see §5). It uploads with XHR, the only web API with upload
progress.

`files.download` stays as the small-file convenience op with no progress or cancel. #196 kept
the two paths apart on purpose: "**`files.download`** documented on both shells as the
intentional fire-and-forget convenience variant" `[recorded: PR #196 body]`.

The two tests assert the builder shape only. They are pre-existing behavioural tests and are not
re-proven by mutation (README, adaptations).

## 5. Consequences (Positive and Negative Predictions)

- **Positive:** file size doesn't change core memory; multipart/form-data was later added as a
  builder option with no ABI change.
- **Positive:** handles from the picker, camera and `files` plugins feed straight into transfers.
- **Negative:** on web, a download is held whole in wasm memory (a `Vec<u8>`, then a copy into
  the `Blob`), twice the file size at the peak. Streaming it into the `Blob` would fix that.
- **Negative:** the rule covers transfers only. `files.read` with its opt-in `base64` flag still
  loads a whole file into the core.
- **Negative:** a handle is only meaningful on the shell that made it (`content://` on Android,
  `blob:` on web). The core can't open, hash or inspect the file. An app that needs the bytes, for
  example to encrypt before upload, must do that on the native side or read the file through
  another plugin.
- **Negative:** there are two download paths, `files.download` and `cx.download`, with different
  guarantees (no progress, no cancel, and on iOS the whole body in native memory for
  `files.download`). An app can pick the wrong one.
- **Negative:** `cx.http` still carries whole bodies. Nothing stops an app from pulling a large
  file through it; where "large" starts is the app's judgement.
- **Negative:** the byte-moving code is written three times (Kotlin, Swift, web Rust), and its
  streaming behaviour is checked by review, not by a test (Condition 3). Background transfers
  and resumable uploads are out of scope.
