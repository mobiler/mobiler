import Foundation
import SharedTypes

/// Streaming file upload/download (free, bundled). See `mobiler-plugin.toml` + the Android twin
/// (`android/TransferPlugin.kt`) for the manifest. Rides the streaming primitive:
///   cx.upload(url, source).start("up", Msg.xfer)   // PUT (default)/POST source → url
///   cx.download(url, dest).start("dl", Msg.xfer)    // GET url → dest
///   cx.unsubscribe(key)                             // cancels
/// Envelope JSON (see mobiler-core::transfer::TransferReq): {"url","source"|"dest","method"?,
/// "headers":[{"name","value"}]}. `op` is "upload" or "download" (the plugin op, not the HTTP verb).
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

        // Resolves a sandbox-relative path, an absolute path, or a `file://` URI to a URL. iOS has
        // no `content://` scheme (that's Android-only), so this is simpler than the Android twin.
        func resolve(_ path: String) -> URL {
            path.hasPrefix("file://") ? (URL(string: path) ?? URL(fileURLWithPath: path)) : URL(fileURLWithPath: path)
        }

        let uploadSource: URL?
        let downloadDest: URL?
        if op == "upload" {
            request.httpMethod = (obj["method"] as? String) ?? "PUT"
            guard let source = obj["source"] as? String else {
                emit(response(for: .done(outcome: .transportError(message: "missing upload source"), handle: nil)))
                return
            }
            uploadSource = resolve(source)
            downloadDest = nil
        } else {
            guard let dest = obj["dest"] as? String else {
                emit(response(for: .done(outcome: .transportError(message: "missing download dest"), handle: nil)))
                return
            }
            uploadSource = nil
            downloadDest = resolve(dest)
        }

        let delegate = TransferDelegate(emit: emit, dest: downloadDest)
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

/// Bridges URLSessionTaskDelegate/URLSessionDownloadDelegate callbacks to `emit`. One instance per
/// transfer (unlike the websocket plugin's single long-lived connection actor) — a transfer is a
/// one-shot request, not a persistent connection. `@unchecked Sendable`: all mutable state here is
/// only ever touched from URLSession's delegate queue (serial by default) plus the two call sites
/// in `subscribe` above that run strictly before/after the session is alive, so there's no
/// concurrent access — same reasoning the bluetooth plugin's `BleManager` uses.
private final class TransferDelegate: NSObject, URLSessionTaskDelegate, URLSessionDownloadDelegate, @unchecked Sendable {
    private let emit: @Sendable (PluginResponse) -> Void
    private let dest: URL? // set only for downloads — where to move the finished temp file
    private var lastEmit = Date.distantPast
    private var cancelledByApp = false
    private var finished = false
    /// Resumes the continuation `subscribe` is suspended on. Set right before `task.resume()`.
    var onFinish: (() -> Void)?

    init(emit: @escaping @Sendable (PluginResponse) -> Void, dest: URL?) {
        self.emit = emit
        self.dest = dest
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
    // touched). For uploads this is the ONLY completion path.
    func urlSession(_ session: URLSession, task: URLSessionTask, didCompleteWithError error: Error?) {
        guard !finished else { return }
        finished = true
        defer { onFinish?() }
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
