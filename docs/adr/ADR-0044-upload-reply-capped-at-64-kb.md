# ADR-0044: An upload's terminal `Done` carries the server's reply in its `Response` body, cut at the same 64 KB on every shell and not marked as cut; a download's body still never enters the core

Status:        Accepted
Date decided:  2026-10-02
Deciding PRs:  #268
Supersedes:    none
Code anchor:   mobiler/plugins/transfer/android/TransferPlugin.kt (UPLOAD_BODY_CAP, cappedUploadBody), mobiler/plugins/transfer/ios/TransferPlugin.swift (uploadBodyCap, urlSession(_:dataTask:didReceive:)), mobiler-web/src/lib.rs (UPLOAD_BODY_CAP, capped_upload_body), mobiler-core/src/transfer.rs (TransferEvent::Done)
Conformance:   mobiler-web/src/lib.rs::upload_body_is_kept_up_to_the_cap

## 1. Context (The Problem)

ADR-0027 keeps a transfer's bytes out of the core: the core passes a string handle, and a native
plugin moves the bytes between disk and socket. Following that, every shell dropped an upload's
response body: Android emitted `emptyList()`, iOS `body: []`, web `vec![]`.

An upload endpoint's reply is usually not file data but what the app needs next. The appointments
team reported (Moj Termin request, 2026-10-01, a temporary request document, so quoted here): "Ours
answers `{"url": "…"}`, and the app attaches that URL to a message. With the body dropped, the app
could not continue. The device showed 'Slanje nije uspelo' after a 200 upload." They asked to "read
the response body and put it in `Done.outcome` the way `cx.request` does. Cap it, for example at 64
KB, so a misbehaving server can't make the shell buffer a large reply."

## 2. Hypothesis

If every shell puts an upload's reply into `Done`'s `HttpOutcome::Response.body`, cut at the same
64 KB, then:

- an app reads an upload's result the same way it reads `cx.request`'s, on every shell;
- the core never receives more than 64 KB from an upload, so ADR-0027's concern (file-sized byte
  streams crossing the FFI) still holds;
- a download is unchanged: its body goes to `dest` and `Done`'s body stays empty.

### 2.1. Refutation Conditions

- **Condition 1 — the reply is kept up to the cap and cut there.**
  - **Validation Metric:** `upload_body_is_kept_up_to_the_cap` in `mobiler-web/src/lib.rs` (the
    web shell's cap). The Android and iOS caps have no unit harness; they are checked by review and
    by the barbershop demo, whose "Send a file" card shows the reply size.

## 3. Considered Options & Rationale for Refutation

- **Option A — keep dropping the reply (ADR-0027 as written)** `[recorded: the shells before this record]`
  Rejected: an app cannot finish an upload flow whose result is in the reply.
- **Option B — no cap, like `cx.request`** `[reconstructed]`
  Rejected: an upload endpoint misbehaving (or a download URL used for an upload) could push a
  file-sized reply across the FFI, which ADR-0027 exists to prevent.
- **Option C — a cap the app sets per call** `[reconstructed]`
  Rejected for now: no app needs a larger reply, and a builder option would change the transfer
  payload that every shell parses. It can come later as a new `with_*` option (ADR-0007).
- **Option D — fail the upload (`ok: false`) when the reply exceeds the cap** `[reconstructed]`
  Rejected: the upload itself succeeded; failing it would make the app retry a completed upload.
- **Option E — a fixed 64 KB cap on every shell, cut silently** `[recorded: Moj Termin request, 2026-10-01, quoted here]`
  Chosen.

## 4. Decision & Rationale for Corroboration

Option E. Android reads the OkHttp body source up to the cap and stops; a failure while reading is
reported as `TransportError`, since OkHttp would otherwise swallow it and leave the upload without a
`Done`. iOS collects `URLSessionDataDelegate` data up to the cap and drops the rest as it arrives. Web
reads the XHR's `responseText` and cuts it. A non-2xx reply carries its body too.

**Mutation proof:** making `capped_upload_body` return the whole reply failed
`upload_body_is_kept_up_to_the_cap` (`left: 65546, right: 65536`); reverting restored green.

## 5. Consequences (Positive and Negative Predictions)

- **Positive:** upload flows that return a URL or id work on every shell.
- **Negative:** a cut reply is not marked as cut; a reply over 64 KB (say, JSON) arrives unparsable.
  The app can compare `Content-Length` when it matters. Marking it would need a new `HttpOutcome`
  field (ADR-0003, ADR-0008).
- **Negative:** on the web the browser still holds the whole reply; the cap bounds only what crosses
  into the core. iOS keeps receiving (and dropping) the rest; Android stops reading.
- **Negative:** the web shell reads the reply as text, so a non-UTF-8 or binary reply's bytes can
  differ from the native shells'.
- **Negative:** apps with the `transfer` plugin installed get this only after re-adding the plugin;
  `mobiler upgrade` reports the plugin as drifted (ADR-0011).
