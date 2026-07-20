import Foundation
import SharedTypes

/// Streaming file upload/download (free, bundled). See `mobiler-plugin.toml` + the Android twin
/// (`android/TransferPlugin.kt`) for the manifest. Rides the streaming primitive:
///   cx.upload(url, source).start("up", Msg.xfer)   // PUT (default)/POST source → url
///   cx.download(url, dest).start("dl", Msg.xfer)    // GET url → dest
///   cx.unsubscribe(key)                             // cancels
/// Envelope JSON (see mobiler-core::transfer::TransferReq): {"url","source"|"dest","method"?,
/// "headers":[{"name","value"}],"multipart"?:{"field","filename"?,"file_content_type"?,
/// "fields"?:[{"name","value"}]}}. `op` is "upload" or "download" (the plugin op, not the HTTP verb).
/// When `multipart` is present, the upload branch below composes a temp `multipart/form-data`
/// file on disk (text fields first, then the file part streamed in) and uploads THAT instead of
/// `source` directly — `URLSessionUploadTask(fromFile:)` has no built-in multipart support. See
/// the Android twin (`TransferPlugin.kt`'s `MultipartBody`) and mobiler-web's `start_web_upload`
/// for the same envelope handled two other ways.
///
/// CANCEL SEMANTICS (must match Android — see TransferPlugin.kt): `cx.unsubscribe` cancels this
/// subscription's Task. Whether that lands before the URLSessionTask is even created, before it's
/// resumed, or mid-flight, we emit NOTHING for it — the app already unsubscribed and isn't
/// listening for a terminal event (this mirrors the web shell's silent pre-send cancel; see
/// mobiler-web's `TransferHandle::drop`). `TransferDelegate.cancelledByApp`, flipped by the
/// `onCancel` handler before `task.cancel()` runs, distinguishes that deliberate teardown from a
/// genuine transport failure, which DOES emit `Done{TransportError}`.
enum TransferPlugin {
    static func handle(op: String, input: String) async -> PluginResponse {
        PluginResponse(ok: false, output: "transfer is a streaming capability — use cx.subscribe")
    }

    static func subscribe(op: String, input: String, emit: @escaping @Sendable (PluginResponse) -> Void) async {
        guard
            let data = input.data(using: .utf8),
            let obj = try? JSONSerialization.jsonObject(with: data) as? [String: Any],
            let urlString = obj["url"] as? String,
            let url = URL(string: urlString)
        else {
            emit(response(for: .done(outcome: .transportError(message: "malformed transfer envelope"), handle: nil)))
            return
        }

        var request = URLRequest(url: url)
        if let headers = obj["headers"] as? [[String: Any]] {
            for h in headers {
                guard let name = h["name"] as? String, let value = h["value"] as? String else { continue }
                request.addValue(value, forHTTPHeaderField: name)
            }
        }

        let docs = (try? FileManager.default.url(for: .documentDirectory, in: .userDomainMask, appropriateFor: nil, create: true))
            ?? FileManager.default.temporaryDirectory

        // Resolves a `file://` URI, an absolute path, or a path relative to the app's Documents dir
        // (rejecting `..` escapes) to a URL — exactly FilesPlugin's `resolve`, so a relative path
        // here and in `files` land in the same place (e.g. a `transfer` download to "x.bin" is
        // where `files`'s "read" of "x.bin" looks). iOS has no `content://` scheme (Android-only),
        // so this is simpler than the Android twin.
        func resolve(_ path: String) -> URL? {
            if path.hasPrefix("file://") { return URL(string: path) }
            if path.hasPrefix("/") { return URL(fileURLWithPath: path) }
            let u = docs.appendingPathComponent(path)
            return u.standardizedFileURL.path.hasPrefix(docs.standardizedFileURL.path) ? u : nil
        }

        let uploadSource: URL?
        let downloadDest: URL?
        // Set only for a multipart upload: the composed temp file actually handed to
        // `uploadTask(fromFile:)`, and the URL the delegate (or the pre-resume cancel branch
        // below, if the race lands before `task.resume()` ever runs) must delete on every
        // terminal outcome — success, transport error, or app cancel.
        var multipartTempFile: URL?
        if op == "upload" {
            request.httpMethod = (obj["method"] as? String) ?? "PUT"
            guard let source = obj["source"] as? String, let resolved = resolve(source) else {
                emit(response(for: .done(outcome: .transportError(message: "missing or invalid upload source"), handle: nil)))
                return
            }
            if let mp = obj["multipart"] as? [String: Any] {
                // Parsed defensively (as?, not forced) so a malformed envelope emits a graceful
                // Done{TransportError} — like the guards above — instead of crashing.
                guard let field = mp["field"] as? String, !field.isEmpty else {
                    emit(response(for: .done(outcome: .transportError(message: "malformed multipart config"), handle: nil)))
                    return
                }
                var fields: [(String, String)] = []
                if let rawFields = mp["fields"] as? [[String: Any]] {
                    for f in rawFields {
                        guard let name = f["name"] as? String, let value = f["value"] as? String else {
                            emit(response(for: .done(outcome: .transportError(message: "malformed multipart config"), handle: nil)))
                            return
                        }
                        fields.append((name, value))
                    }
                }

                let boundary = "Boundary-\(UUID().uuidString)"
                let tmp = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
                FileManager.default.createFile(atPath: tmp.path, contents: nil)
                guard let out = try? FileHandle(forWritingTo: tmp) else {
                    try? FileManager.default.removeItem(at: tmp)
                    emit(response(for: .done(outcome: .transportError(message: "cannot create multipart temp file"), handle: nil)))
                    return
                }
                // Sanitizes a field name / filename destined for a `Content-Disposition` header
                // line: `"` would prematurely close the quoted value, and a raw CR/LF would inject
                // a header line. Not applied to field VALUES — those sit in the body after a blank
                // line, framed by the random UUID boundary, so they can't collide with it. v1:
                // quotes/CRLF only — full RFC 7578 percent/quoted-string escaping is deferred (the
                // Android twin does the same escaping via OkHttp's MultipartBody, not raw
                // interpolation, so this brings iOS to parity, not "matches Android" as the header
                // comment used to (incorrectly) claim).
                func sanitize(_ s: String) -> String {
                    s.replacingOccurrences(of: "\"", with: "%22")
                        .replacingOccurrences(of: "\r", with: "")
                        .replacingOccurrences(of: "\n", with: "")
                }
                // All FileHandle I/O below uses the THROWING modern APIs
                // (`write(contentsOf:)`/`read(upToCount:)`, iOS 13.4+ — this template targets iOS
                // 16) inside this do/catch. The legacy `write(_:)`/`readData(ofLength:)` raise an
                // uncatchable Objective-C NSException on an I/O failure (disk full, permission
                // revoked mid-write) that is PROCESS-FATAL — Swift `try`/`catch` cannot intercept
                // it, and composing this temp file temporarily doubles disk usage (source + copy),
                // so a large upload on a near-full device is a plausible trigger. Any thrown error
                // here closes both handles, deletes the temp file, and emits a graceful
                // Done{TransportError} — the same terminal the guards above already use — instead
                // of crashing the process, matching the Android twin (OkHttp throws a catchable
                // IOException).
                do {
                    func w(_ s: String) throws {
                        if let d = s.data(using: .utf8) { try out.write(contentsOf: d) }
                    }
                    // Text fields first, in order.
                    for f in fields {
                        try w("--\(boundary)\r\nContent-Disposition: form-data; name=\"\(sanitize(f.0))\"\r\n\r\n\(f.1)\r\n")
                    }
                    // File part LAST, streamed from disk in 64KB chunks — never loaded fully into
                    // memory. Content-Type here is `multipart.file_content_type` if present, else
                    // "application/octet-stream" — deliberately NOT the caller's request-level
                    // Content-Type header (that header is a different thing: the *request's*
                    // Content-Type, which multipart mode overrides below to the boundary type
                    // anyway). Falling through to a caller header here would be exactly the Task 5
                    // Android bug this must not repeat.
                    let filename = (mp["filename"] as? String) ?? inferFilename(source)
                    let ctype = (mp["file_content_type"] as? String) ?? "application/octet-stream"
                    try w("--\(boundary)\r\nContent-Disposition: form-data; name=\"\(sanitize(field))\"; filename=\"\(sanitize(filename))\"\r\nContent-Type: \(ctype)\r\n\r\n")
                    guard let rh = try? FileHandle(forReadingFrom: resolved) else {
                        try? out.close()
                        try? FileManager.default.removeItem(at: tmp)
                        emit(response(for: .done(outcome: .transportError(message: "cannot open upload source '\(source)'"), handle: nil)))
                        return
                    }
                    while true {
                        guard let chunk = try rh.read(upToCount: 64 * 1024) else { break } // nil == EOF
                        if chunk.isEmpty { break }
                        try out.write(contentsOf: chunk)
                    }
                    try? rh.close()
                    try w("\r\n--\(boundary)--\r\n")
                    try out.close()
                } catch {
                    try? out.close()
                    try? FileManager.default.removeItem(at: tmp)
                    emit(response(for: .done(outcome: .transportError(message: "failed to compose multipart body: \(error.localizedDescription)"), handle: nil)))
                    return
                }
                // Authoritative: replaces (not appends — `setValue`, not `addValue`) any
                // caller-supplied Content-Type header added by the loop above, so the boundary
                // actually on the wire always matches the body we just wrote.
                request.setValue("multipart/form-data; boundary=\(boundary)", forHTTPHeaderField: "Content-Type")
                multipartTempFile = tmp
            }
            uploadSource = multipartTempFile ?? resolved
            downloadDest = nil
        } else {
            guard let dest = obj["dest"] as? String, let resolved = resolve(dest) else {
                emit(response(for: .done(outcome: .transportError(message: "missing or invalid download dest"), handle: nil)))
                return
            }
            uploadSource = nil
            downloadDest = resolved
        }

        let delegate = TransferDelegate(emit: emit, dest: downloadDest, tempUploadFile: multipartTempFile)
        // A dedicated session (not .shared) so this transfer's delegate/lifetime is self-contained,
        // matching the websocket plugin's `URLSession(configuration: .default)` precedent.
        let session = URLSession(configuration: .default, delegate: delegate, delegateQueue: nil)
        let task: URLSessionTask
        if op == "upload", let uploadSource {
            task = session.uploadTask(with: request, fromFile: uploadSource)
        } else {
            task = session.downloadTask(with: request)
        }

        await withTaskCancellationHandler {
            await withCheckedContinuation { (cont: CheckedContinuation<Void, Never>) in
                // Cancel race (mirrors mobiler-web's `start_web_upload` comment): `onCancel` below
                // can fire before this closure runs at all (the enclosing Task was already
                // cancelled the instant it was created). Checking here, right before `resume()`,
                // is what keeps that case silent — the task never starts, so no delegate callback
                // ever fires and nothing is ever emitted for it.
                if Task.isCancelled {
                    delegate.markCancelledByApp()
                    // The task above was created but never resumed, so no delegate callback will
                    // ever fire to clean up a multipart temp file — do it directly here.
                    if let multipartTempFile {
                        try? FileManager.default.removeItem(at: multipartTempFile)
                    }
                    cont.resume()
                    return
                }
                delegate.onFinish = { cont.resume() }
                task.resume()
            }
        } onCancel: {
            delegate.markCancelledByApp()
            task.cancel()
        }
        session.finishTasksAndInvalidate()
    }

    fileprivate static func response(for ev: TransferEvent) -> PluginResponse {
        let ok: Bool
        switch ev {
        case .progress: ok = true
        case .done(let outcome, _):
            if case .response(let status, _, _) = outcome { ok = (200..<300).contains(Int(status)) } else { ok = false }
        }
        return PluginResponse(ok: ok, output: (try? ev.bincodeSerialize()) ?? [])
    }
}

/// Last `/`-segment of a handle, `?query`/`#fragment` stripped; empty -> "file". Mirrors
/// mobiler-web's `infer_filename` and the Android twin's `inferFilename` so a multipart upload
/// with no explicit `filename` override picks the same name on every platform.
private func inferFilename(_ source: String) -> String {
    // Index-based (not `split`, which returns `[]` — and traps on `[0]` — for an empty string)
    // so an empty/edge-case `source` degrades to "file" instead of crashing.
    let cut = source.firstIndex { $0 == "?" || $0 == "#" }
    let truncated = cut.map { String(source[source.startIndex..<$0]) } ?? source
    let name = truncated.lastIndex(of: "/").map { String(truncated[truncated.index(after: $0)...]) } ?? truncated
    return name.isEmpty ? "file" : name
}

/// Bridges URLSessionTaskDelegate/URLSessionDownloadDelegate callbacks to `emit`. One instance per
/// transfer (unlike the websocket plugin's single long-lived connection actor) — a transfer is a
/// one-shot request, not a persistent connection. `@unchecked Sendable`: all mutable state here is
/// only ever touched from URLSession's delegate queue (serial by default) plus the two call sites
/// in `subscribe` above that run strictly before/after the session is alive, so there's no
/// concurrent access — same reasoning the bluetooth plugin's `BleManager` uses.
private final class TransferDelegate: NSObject, URLSessionTaskDelegate, URLSessionDownloadDelegate, @unchecked Sendable {
    private let emit: @Sendable (PluginResponse) -> Void
    private let dest: URL? // set only for downloads — where to move the finished temp file
    private let tempUploadFile: URL? // set only for a multipart upload — the composed temp file to delete
    private var lastEmit = Date.distantPast
    private var cancelledByApp = false
    private var finished = false
    /// Resumes the continuation `subscribe` is suspended on. Set right before `task.resume()`.
    var onFinish: (() -> Void)?

    init(emit: @escaping @Sendable (PluginResponse) -> Void, dest: URL?, tempUploadFile: URL? = nil) {
        self.emit = emit
        self.dest = dest
        self.tempUploadFile = tempUploadFile
    }

    func markCancelledByApp() { cancelledByApp = true }

    // ~10/sec progress throttle (same rule as the web shell / Android twin). The terminal Done is
    // never throttled — each completion delegate method below emits it unconditionally.
    private func maybeEmitProgress(transferred: Int64, total: Int64) {
        let now = Date()
        guard now.timeIntervalSince(lastEmit) >= 0.1 else { return }
        lastEmit = now
        let t: UInt64? = total > 0 ? UInt64(total) : nil
        emit(TransferPlugin.response(for: .progress(transferred: UInt64(max(0, transferred)), total: t)))
    }

    // Upload progress.
    func urlSession(_ session: URLSession, task: URLSessionTask, didSendBodyData bytesSent: Int64, totalBytesSent: Int64, totalBytesExpectedToSend: Int64) {
        maybeEmitProgress(transferred: totalBytesSent, total: totalBytesExpectedToSend)
    }

    // Download progress.
    func urlSession(_ session: URLSession, downloadTask: URLSessionDownloadTask, didWriteData bytesWritten: Int64, totalBytesWritten: Int64, totalBytesExpectedToWrite: Int64) {
        maybeEmitProgress(transferred: totalBytesWritten, total: totalBytesExpectedToWrite)
    }

    // Download completion: move the system's temp file to `dest` (overwriting), then emit Done
    // with the response headers/status. `didCompleteWithError` also fires right after this (with
    // `error == nil`) — the `finished` guard there skips it so we don't double-emit.
    func urlSession(_ session: URLSession, downloadTask: URLSessionDownloadTask, didFinishDownloadingTo location: URL) {
        finished = true
        defer { onFinish?() }
        guard let dest else { return } // shouldn't happen — a download always sets `dest`
        do {
            try? FileManager.default.removeItem(at: dest) // overwrite
            try FileManager.default.createDirectory(at: dest.deletingLastPathComponent(), withIntermediateDirectories: true)
            try FileManager.default.moveItem(at: location, to: dest)
        } catch {
            try? FileManager.default.removeItem(at: dest)
            if !cancelledByApp {
                emit(TransferPlugin.response(for: .done(outcome: .transportError(message: "failed to save: \(error.localizedDescription)"), handle: nil)))
            }
            return
        }
        guard !cancelledByApp else { return }
        let http = downloadTask.response as? HTTPURLResponse
        let status = UInt16(clamping: http?.statusCode ?? 0)
        let headers = (http?.allHeaderFields ?? [:]).compactMap { key, value -> HttpHeader? in
            guard let name = key as? String else { return nil }
            return HttpHeader(name: name, value: String(describing: value))
        }
        emit(TransferPlugin.response(for: .done(outcome: .response(status: status, headers: headers, body: []), handle: dest.absoluteString)))
    }

    // Fires for both uploads and downloads. For downloads this is a NO-OP when the transfer
    // succeeded (`didFinishDownloadingTo` already emitted Done and set `finished`); it's still the
    // only place a download's cancel/failure surfaces (the temp file is cleaned up by the OS, so
    // there's no partial `dest` to delete on a failed/cancelled download — `dest` was never
    // touched). For uploads this is the ONLY completion path — success, transport error, AND app
    // cancel all land here exactly once, which is why a multipart upload's temp file is deleted
    // unconditionally right below, before the `cancelledByApp` branch that decides whether to emit.
    func urlSession(_ session: URLSession, task: URLSessionTask, didCompleteWithError error: Error?) {
        guard !finished else { return }
        finished = true
        defer { onFinish?() }
        if let tempUploadFile {
            try? FileManager.default.removeItem(at: tempUploadFile)
        }
        guard !cancelledByApp else { return }
        if let error {
            emit(TransferPlugin.response(for: .done(outcome: .transportError(message: error.localizedDescription), handle: nil)))
            return
        }
        // Upload success.
        let http = task.response as? HTTPURLResponse
        let status = UInt16(clamping: http?.statusCode ?? 0)
        let headers = (http?.allHeaderFields ?? [:]).compactMap { key, value -> HttpHeader? in
            guard let name = key as? String else { return nil }
            return HttpHeader(name: name, value: String(describing: value))
        }
        emit(TransferPlugin.response(for: .done(outcome: .response(status: status, headers: headers, body: []), handle: nil)))
    }
}
