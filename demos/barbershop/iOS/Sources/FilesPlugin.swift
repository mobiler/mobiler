import SharedTypes
import UIKit
import UniformTypeIdentifiers

/// Free bundled plugin: app-sandbox file I/O + download + export (see mobiler-plugin.toml for the op
/// JSON). Paths are relative to the app's Documents dir (sandbox); `read`/`export` also accept a
/// file:// URI. No permission needed (export uses the system save picker).
enum FilesPlugin {
    static func handle(op: String, input: String) async -> PluginResponse {
        let obj = (input.data(using: .utf8).flatMap { try? JSONSerialization.jsonObject(with: $0) }) as? [String: Any] ?? [:]
        let docs = (try? FileManager.default.url(for: .documentDirectory, in: .userDomainMask, appropriateFor: nil, create: true))
            ?? FileManager.default.temporaryDirectory

        // Resolve a sandbox-relative path (or an absolute file:// URI) to a URL; reject `..` escapes.
        func resolve(_ path: String) -> URL? {
            if path.hasPrefix("file://") { return URL(string: path) }
            if path.hasPrefix("/") { return URL(fileURLWithPath: path) }
            let u = docs.appendingPathComponent(path)
            return u.standardizedFileURL.path.hasPrefix(docs.standardizedFileURL.path) ? u : nil
        }

        switch op {
        case "read":
            guard let url = (obj["path"] as? String).flatMap(resolve) else { return PluginResponse(ok: false, output: "bad path") }
            guard let data = try? Data(contentsOf: url) else { return PluginResponse(ok: false, output: "not found") }
            if obj["base64"] as? Bool == true { return PluginResponse(ok: true, output: data.base64EncodedString()) }
            return PluginResponse(ok: true, output: String(data: data, encoding: .utf8) ?? "")
        case "write", "append":
            guard let url = (obj["path"] as? String).flatMap(resolve) else { return PluginResponse(ok: false, output: "bad path") }
            let content = obj["content"] as? String ?? ""
            let bytes: Data = (obj["base64"] as? Bool == true) ? (Data(base64Encoded: content) ?? Data()) : Data(content.utf8)
            try? FileManager.default.createDirectory(at: url.deletingLastPathComponent(), withIntermediateDirectories: true)
            do {
                if op == "append", FileManager.default.fileExists(atPath: url.path), let h = try? FileHandle(forWritingTo: url) {
                    defer { try? h.close() }
                    try h.seekToEnd()
                    try h.write(contentsOf: bytes)
                } else {
                    try bytes.write(to: url)
                }
                return PluginResponse(ok: true, output: url.absoluteString)
            } catch { return PluginResponse(ok: false, output: error.localizedDescription) }
        case "delete":
            guard let url = (obj["path"] as? String).flatMap(resolve) else { return PluginResponse(ok: false, output: "bad path") }
            try? FileManager.default.removeItem(at: url)
            return PluginResponse(ok: true, output: "")
        case "list":
            let dir = (obj["dir"] as? String).flatMap(resolve) ?? docs
            let entries = (try? FileManager.default.contentsOfDirectory(at: dir, includingPropertiesForKeys: [.fileSizeKey, .isDirectoryKey])) ?? []
            let arr: [[String: Any]] = entries.map { u in
                let rv = try? u.resourceValues(forKeys: [.fileSizeKey, .isDirectoryKey])
                return ["name": u.lastPathComponent, "size": rv?.fileSize ?? 0, "dir": rv?.isDirectory ?? false]
            }
            let json = (try? JSONSerialization.data(withJSONObject: arr)).flatMap { String(data: $0, encoding: .utf8) } ?? "[]"
            return PluginResponse(ok: true, output: json)
        case "download":
            // Simple fire-and-forget download: one await of the whole response body via
            // URLSession.data(from:), no progress and no mid-flight cancel (a real in-flight
            // `URLSessionTask.cancel()`/partial-file cleanup only exists on the streaming path).
            // For progress reporting and a real cancel (`cx.unsubscribe`), use the `transfer`
            // plugin's `cx.download(url, dest)` instead — see mobiler/plugins/transfer. The two
            // are intentionally separate code paths: this one is the convenience op for a small
            // file where nobody needs a progress bar.
            guard let src = (obj["url"] as? String).flatMap(URL.init(string:)),
                  let dst = (obj["path"] as? String).flatMap(resolve) else { return PluginResponse(ok: false, output: "bad path/url") }
            do {
                let (data, _) = try await URLSession.shared.data(from: src)
                try? FileManager.default.createDirectory(at: dst.deletingLastPathComponent(), withIntermediateDirectories: true)
                try data.write(to: dst)
                return PluginResponse(ok: true, output: dst.absoluteString)
            } catch { return PluginResponse(ok: false, output: error.localizedDescription) }
        case "export":
            guard let url = (obj["path"] as? String).flatMap(resolve) else { return PluginResponse(ok: false, output: "bad path") }
            guard FileManager.default.fileExists(atPath: url.path) else { return PluginResponse(ok: false, output: "not found") }
            return await exportViaPicker(url: url)
        default:
            return PluginResponse(ok: false, output: "unknown op '\(op)'")
        }
    }

    @MainActor
    private static func exportViaPicker(url: URL) async -> PluginResponse {
        guard let presenter = frontmostViewController() else { return PluginResponse(ok: false, output: "no view controller to present from") }
        return await withCheckedContinuation { cont in
            // asCopy: true exports a copy; the original stays in the sandbox.
            let picker = UIDocumentPickerViewController(forExporting: [url], asCopy: true)
            let delegate = ExportDelegate { cont.resume(returning: $0) }
            ExportDelegate.retained = delegate // the picker holds its delegate weakly
            picker.delegate = delegate
            presenter.present(picker, animated: true)
        }
    }

    // A standalone plugin file can't call Core.swift's private helper, so it finds the frontmost VC
    // itself (same approach as filepicker/scanner).
    @MainActor
    private static func frontmostViewController() -> UIViewController? {
        let keyWindow = UIApplication.shared.connectedScenes
            .compactMap { $0 as? UIWindowScene }
            .first { $0.activationState == .foregroundActive }?
            .keyWindow
        var top = keyWindow?.rootViewController
        while let presented = top?.presentedViewController { top = presented }
        return top
    }
}

private final class ExportDelegate: NSObject, UIDocumentPickerDelegate {
    static var retained: ExportDelegate?
    private let onResult: (PluginResponse) -> Void
    init(_ onResult: @escaping (PluginResponse) -> Void) { self.onResult = onResult }
    func documentPicker(_ controller: UIDocumentPickerViewController, didPickDocumentsAt urls: [URL]) {
        finish(PluginResponse(ok: true, output: "exported"))
    }
    func documentPickerWasCancelled(_ controller: UIDocumentPickerViewController) {
        finish(PluginResponse(ok: false, output: "cancelled"))
    }
    private func finish(_ r: PluginResponse) {
        ExportDelegate.retained = nil
        onResult(r)
    }
}
