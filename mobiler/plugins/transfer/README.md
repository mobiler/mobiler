# transfer — streaming file upload/download, progress + cancel (free, bundled)

```bash
mobiler plugin add transfer
```

Moves large files by **handle** (a sandbox path, a `content://`/`file://` URI, or a web `blob:`
URL) — the bytes never cross the FFI. Rides Mobiler's **streaming primitive**
(`cx.subscribe`/`unsubscribe`, wrapped by the `cx.upload`/`cx.download` builders): `subscribe`
moves the file natively and pushes a `TransferEvent` per progress tick and once at completion.

```rust
// Upload: PUT (default) `source` (a local path/content-URI) to `url`.
cx.upload(url, source).bearer(&token).start("up", Msg::Xfer);

// Download: GET `url` to `dest` (a sandbox-relative path).
cx.download(url, dest).start("dl", Msg::Xfer);

Msg::Xfer(TransferEvent::Progress { transferred, total }) => {
    model.percent = total.map(|t| transferred as f64 / t as f64 * 100.0);
}
Msg::Xfer(TransferEvent::Done { outcome, handle }) => {
    match outcome {
        HttpOutcome::Response { status, .. } if (200..300).contains(&status) => {
            model.saved_at = handle; // Some(path) for a download; None for an upload
        }
        HttpOutcome::Response { status, .. } => model.error = Some(format!("http {status}")),
        HttpOutcome::TransportError { message } => model.error = Some(message),
    }
}

// Cancel a transfer in flight:
cx.unsubscribe("up");
```

- **Builders:** `cx.upload(url, source)` / `cx.download(url, dest)` return a `TransferBuilder` —
  chain `.method(...)` (upload only, default `PUT`), `.header(name, value)`, `.bearer(token)`, then
  `.start(key, on_event)` to subscribe. `cx.unsubscribe(key)` cancels.
- **Events:** `TransferEvent::Progress { transferred, total }` (throttled to ~10/sec; `total` is
  `None` for a chunked/unknown-length transfer) and exactly one terminal
  `TransferEvent::Done { outcome, handle }`. `outcome` is the same `HttpOutcome` the `http`
  capability uses — a non-2xx response is still `Done{Response}`, not an error; only a genuine
  transport failure is `Done{TransportError}`. `handle` is the destination path/URL the shell wrote
  for a **download** (`None` for an upload; the response body itself is never inlined — it goes
  straight to disk).
- **Cancel semantics:** cancelling (`cx.unsubscribe`) — at any point, before or during the
  transfer — emits **no terminal event**; the app has already stopped listening. A **partial
  download file is deleted** on cancel or on a genuine failure. This matches the web shell's
  pre-send-cancel behavior and is identical across iOS/Android/web (see the cancel-semantics
  comment atop each native plugin file for the exact mechanics).
- **Android:** upload streams from the source `Uri`/path via a custom OkHttp `RequestBody`
  (`contentResolver.openInputStream` for `content://`, else a `File`) with a byte-counting sink for
  progress; download streams `client.newCall(req).execute()`'s response body straight to the `dest`
  file in a loop, checking `ensureActive()` each iteration so `cx.unsubscribe` → `call.cancel()`
  actually interrupts a stalled socket read. OkHttp is already a template dependency (`http`).
- **iOS:** `URLSessionUploadTask(fromFile:)` / `URLSessionDownloadTask`, both driven by a
  `URLSessionTaskDelegate`/`URLSessionDownloadDelegate` (`didSendBodyData`/`didWriteData` →
  progress; completion → `Done`). A finished download's temp file is moved to `dest`, overwriting.
  System framework — no package. iOS 16 target ✓.
- **Web:** see the published `mobiler-web` shell (Release A/B) — XHR for upload progress, `fetch` +
  `ReadableStream` for download progress, handing back a `blob:` handle.
- One `URLSession`/OkHttp `Call` per transfer (not a shared connection, unlike `websocket`) — each
  `start()` is an independent request.

See `app-core-usage.rs` for a full upload/download + progress + cancel example.
