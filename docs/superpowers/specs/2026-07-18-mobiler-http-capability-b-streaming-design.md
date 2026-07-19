# Mobiler HTTP Capability — Release B: streaming transfers

**Date:** 2026-07-18
**Status:** design sketch — deliberately less detailed than Release A, to be expanded
into a full spec when A has shipped
**Depends on:** `2026-07-18-mobiler-http-capability-a-design.md`

## Motivation

Release A covers request/response for small and medium payloads. It does not cover
large ones, and mobiler has a known hole there: the `files` plugin performs downloads
**natively**, bypassing `cx.http` entirely, because a whole file cannot sensibly cross
the FFI boundary as a single value.

Stoa will hit the same wall from the other direction. As an E2EE messenger it needs
media attachments, voice messages, and avatars — all of which are client-side
encrypted before upload, making them uniformly random, incompressible bytes. A 20MB
video is the normal case, not the edge case.

## The central decision: paths, not bytes

The core must never hold the payload. Large transfers pass **file paths and handles**;
the bytes go disk → socket (or socket → disk) entirely on the native side.

Consequences:

- Memory stays flat regardless of payload size, on every shell.
- `PluginCall.input` can remain a `String` — a path is text. This is why Release A's
  input-side asymmetry is sound rather than a shortcut.
- Base64 never enters the picture; there is nothing to encode.

## It rides an existing primitive

A transfer emits progress repeatedly and then terminates. That is exactly the shape of
the C5 streaming primitive (`cx.subscribe` / `Effect::PluginStream`, Crux
multi-resolve). So:

| Transfer concept | Existing primitive |
|---|---|
| progress events | stream events |
| completion | terminal stream event |
| **cancel the transfer** | `cx.unsubscribe(key)` |

Cancellation comes essentially for free, which matters — an uncancellable 20MB upload
on a mobile connection is a bug, and the repo's own "don't defer primitive lifecycle"
lesson says teardown belongs in v1.

This is a second consumer of an existing subsystem, not a new subsystem.

## API sketch

```rust
cx.upload(url, FilePath("/tmp/attachment.enc"))
    .bearer(&token)
    .on_progress(Event::UploadProgress)    // fraction complete, 0.0..=1.0
    .send(Event::UploadDone);              // terminal: HttpOutcome

cx.download(url, FilePath("/var/cache/inbox/att-42.enc"))
    .bearer(&token)
    .on_progress(Event::DownloadProgress)
    .send(Event::DownloadDone);
```

Both return a subscription key so the app can cancel:

```rust
let key = cx.upload(...).send(...);
// later
cx.unsubscribe(key);
```

Progress events should carry bytes-transferred and total-if-known, not only a
fraction — total is unknown for chunked responses, and the app decides how to present
that.

## Shell notes

**iOS.** `URLSessionUploadTask` / `URLSessionDownloadTask` with a
`URLSessionTaskDelegate` for `didSendBodyData` / `didWriteData`. Download tasks already
write to a temp file and hand back a URL, which fits the path-based model directly.

**Android.** OkHttp with a custom `RequestBody` that reports as it writes for upload,
and a buffered sink to disk for download. Progress must be marshalled back to the main
thread.

**Web — the known asymmetry.** `fetch()` can stream *downloads* via `ReadableStream`
but has **no upload progress**. Upload progress on web still requires
`XMLHttpRequest`'s `upload.onprogress`. So the web shell will likely use XHR for
uploads and fetch for downloads. This looks inconsistent and must be documented
deliberately so it is not "cleaned up" later by someone who does not know why.

Web also has no filesystem in the native sense, so `FilePath` needs a web-side
equivalent — most likely an opaque handle referencing a `Blob`/`File` obtained from the
existing file-picker plugin. **This is the largest open question in Release B** and
should be resolved before the full spec is written.

## Relationship to the `files` plugin

Release B should subsume the `files` plugin's native download workaround. Once
`cx.download` exists, that plugin can delegate rather than carry its own
implementation. Worth confirming during implementation that the two agree on path
semantics and sandbox locations.

## Explicitly out of scope

- **Background transfers** — continuing while the app is backgrounded. iOS background
  `URLSession` is a substantial subsystem with its own delegate lifecycle and
  relaunch-into-completion semantics. Deserves its own pass.
- **Resumable / chunked uploads** — requires server-side support, which Stoa's DS does
  not have.
- **Concurrent-transfer scheduling and prioritisation** — platform defaults apply.

## Open questions for the full spec

1. What is the web-side `FilePath` equivalent, and how does it interoperate with the
   existing file-picker plugin?
2. Does `download` overwrite an existing file, fail, or take a policy argument?
3. Should progress be throttled in the shell (e.g. max ~10/sec) to avoid flooding the
   core with events on a fast connection?
4. What happens to a partially written download file on cancel or transport failure —
   delete, or leave for a future resume?
5. Do uploads need multipart/form-data, or is a raw body sufficient for Stoa and
   likely consumers?
