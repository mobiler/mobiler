import SharedTypes
import UIKit

/// Free bundled plugin: share a file or image through the system share sheet (no permission). Op "file".
/// Input is a bare handle (a file URL or an absolute path), or a JSON object {source, mime?, text?, name?, headers?}
/// whose source may also be an https URL (fetched here, up to 50 MB) or a path relative to Documents (the `files`
/// plugin's paths). The file reaches the sheet under its proper name and extension, with `text` alongside.
/// Replies: ok "shared" / ok:false "cancelled" from the sheet, else ok:false "<code>" or "<code>: <detail>" (ADR-0068).
@MainActor
enum SharefilePlugin {
    static func handle(op: String, input: String) async -> PluginResponse {
        guard op == "file" else { return PluginResponse(ok: false, output: "unavailable: unknown op '\(op)'") }
        let item: URL
        let text: String?
        do {
            let ask = try Source.parse(input, bareAllowed: true)
            text = ask.text
            item = try await stage(ask)
        } catch let r as Refused {
            return PluginResponse(ok: false, output: r.reply)
        } catch {
            return PluginResponse(ok: false, output: "failed: \(error.localizedDescription)")
        }
        guard let presenter = frontmostViewController() else {
            return PluginResponse(ok: false, output: "no_activity")
        }
        var items: [Any] = [item]
        if let text, !text.isEmpty { items.append(text) }
        return await withCheckedContinuation { cont in
            var resumed = false
            let vc = UIActivityViewController(activityItems: items, applicationActivities: nil)
            vc.completionWithItemsHandler = { _, completed, _, _ in
                if resumed { return }
                resumed = true
                cont.resume(returning: PluginResponse(ok: completed, output: completed ? "shared" : "cancelled"))
            }
            // iPad presents the sheet in a popover, which needs a source.
            vc.popoverPresentationController?.sourceView = presenter.view
            vc.popoverPresentationController?.sourceRect = CGRect(
                x: presenter.view.bounds.midX, y: presenter.view.bounds.midY, width: 0, height: 0)
            presenter.present(vc, animated: true)
        }
    }

    /// The file to hand to the sheet. A bare handle is shared where it is (today's behaviour). A JSON source is
    /// fetched (https) or copied (local) into Caches/share under its proper name, so the receiver sees the right
    /// name and type.
    private static func stage(_ ask: Source.Ask) async throws -> URL {
        let isBare = !(ask.mime != nil || ask.text != nil || ask.name != nil || !ask.headers.isEmpty)
        let lower = ask.source.lowercased()
        if lower.hasPrefix("https://") {
            let (file, declared) = try await Source.fetch(ask)
            let mime = Source.sniffImage(file) ?? ask.mime.map(Source.bareMime).flatMap { $0.isEmpty ? nil : $0 } ?? declared
            return try Source.place(file, as: Source.fileName(ask.source, ask.name, mime, fallback: "shared"), move: true)
        }
        let local = try Source.localFile(ask.source)
        if isBare { return local }
        let mime = Source.sniffImage(local) ?? ask.mime.map(Source.bareMime) ?? ""
        return try Source.place(local, as: Source.fileName(local.lastPathComponent, ask.name, mime, fallback: "shared"), move: false)
    }

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

/// A refusal with its reply code (ADR-0068).
private struct Refused: Error {
    let reply: String
    init(_ code: String, _ detail: String? = nil) {
        reply = (detail?.isEmpty ?? true) ? code : "\(code): \(detail!)"
    }
}

/// Source rules, the same in the sharefile and saveimage plugins (each plugin is copied into an app on its
/// own, so neither can depend on the other). Keep the two in step.
private enum Source {
    static let maxBytes: Int64 = 50 * 1024 * 1024
    static let keep: TimeInterval = 24 * 60 * 60

    struct Ask {
        var source: String
        var mime: String?
        var text: String?
        var name: String?
        var headers: [String: String]
    }

    static func parse(_ input: String, bareAllowed: Bool) throws -> Ask {
        let trimmed = input.trimmingCharacters(in: .whitespacesAndNewlines)
        var ask: Ask
        if trimmed.hasPrefix("{") {
            guard let data = trimmed.data(using: .utf8),
                  let o = (try? JSONSerialization.jsonObject(with: data)) as? [String: Any] else { throw Refused("bad_input") }
            let headers = (o["headers"] as? [String: Any])?.compactMapValues { $0 as? String } ?? [:]
            ask = Ask(source: ((o["source"] as? String) ?? "").trimmingCharacters(in: .whitespacesAndNewlines),
                      mime: o["mime"] as? String, text: o["text"] as? String, name: o["name"] as? String, headers: headers)
        } else {
            guard bareAllowed else { throw Refused("bad_input", "expected {\"source\": …}") }
            ask = Ask(source: trimmed, mime: nil, text: nil, name: nil, headers: [:])
        }
        let lower = ask.source.lowercased()
        if ask.source.isEmpty { throw Refused("bad_input") }
        if lower.hasPrefix("http://") { throw Refused("insecure_source") }
        if !(lower.hasPrefix("https://") || lower.hasPrefix("file://")),
           lower.range(of: "^[a-z][a-z0-9+.-]*:", options: .regularExpression) != nil {
            throw Refused("unsupported_source")
        }
        return ask
    }

    /// The folder fetched files land in (Caches/share), pruned of files older than a day.
    static func shareDir() throws -> URL {
        let caches = try FileManager.default.url(for: .cachesDirectory, in: .userDomainMask, appropriateFor: nil, create: true)
        let dir = caches.appendingPathComponent("share", isDirectory: true)
        try FileManager.default.createDirectory(at: dir, withIntermediateDirectories: true)
        let cutoff = Date().addingTimeInterval(-keep)
        let items = (try? FileManager.default.contentsOfDirectory(at: dir, includingPropertiesForKeys: [.contentModificationDateKey])) ?? []
        for item in items {
            let modified = (try? item.resourceValues(forKeys: [.contentModificationDateKey]))?.contentModificationDate ?? .distantPast
            if modified < cutoff { try? FileManager.default.removeItem(at: item) }
        }
        return dir
    }

    /// A file:// URL, an absolute path, or a path relative to Documents (never climbing out of it).
    static func localFile(_ src: String) throws -> URL {
        let url: URL
        if src.lowercased().hasPrefix("file://") {
            guard let u = URL(string: src) else { throw Refused("bad_input") }
            url = u
        } else if src.hasPrefix("/") {
            url = URL(fileURLWithPath: src)
        } else {
            let docs = try FileManager.default.url(for: .documentDirectory, in: .userDomainMask, appropriateFor: nil, create: true)
                .standardizedFileURL
            url = docs.appendingPathComponent(src).standardizedFileURL
            guard url.path.hasPrefix(docs.path + "/") else { throw Refused("bad_input", "path leaves Documents") }
        }
        var isDir: ObjCBool = false
        guard FileManager.default.fileExists(atPath: url.path, isDirectory: &isDir), !isDir.boolValue else { throw Refused("not_found") }
        return url
    }

    /// Fetch an https source into the share folder: a non-2xx status or a network error is fetch_failed, more than
    /// 50 MB is too_large. Returns the file and the response's declared type.
    static func fetch(_ ask: Ask) async throws -> (URL, String) {
        guard let url = URL(string: ask.source) else { throw Refused("bad_input") }
        var req = URLRequest(url: url, timeoutInterval: 60)
        for (k, v) in ask.headers { req.setValue(v, forHTTPHeaderField: k) }
        let downloaded: URL
        let response: URLResponse
        do {
            (downloaded, response) = try await URLSession.shared.download(for: req)
        } catch {
            throw Refused("fetch_failed", error.localizedDescription)
        }
        defer { try? FileManager.default.removeItem(at: downloaded) }
        if let http = response as? HTTPURLResponse, !(200..<300).contains(http.statusCode) {
            throw Refused("fetch_failed", "HTTP \(http.statusCode)")
        }
        let size = (try? FileManager.default.attributesOfItem(atPath: downloaded.path)[.size] as? Int64) ?? 0
        if size > maxBytes { throw Refused("too_large") }
        let out = try shareDir().appendingPathComponent("in-\(UUID().uuidString).part")
        try FileManager.default.moveItem(at: downloaded, to: out)
        return (out, bareMime(response.mimeType ?? ""))
    }

    /// The raster image type a file's first bytes show (PNG, JPEG, GIF, WebP), else nil.
    static func sniffImage(_ url: URL) -> String? {
        guard let h = try? FileHandle(forReadingFrom: url) else { return nil }
        defer { try? h.close() }
        let b = [UInt8]((try? h.read(upToCount: 16)) ?? Data())
        if b.starts(with: [0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A]) { return "image/png" }
        if b.starts(with: [0xFF, 0xD8, 0xFF]) { return "image/jpeg" }
        if b.starts(with: Array("GIF87a".utf8)) || b.starts(with: Array("GIF89a".utf8)) { return "image/gif" }
        if b.count >= 12, Array(b[0..<4]) == Array("RIFF".utf8), Array(b[8..<12]) == Array("WEBP".utf8) { return "image/webp" }
        return nil
    }

    static func bareMime(_ m: String) -> String {
        (m.split(separator: ";", maxSplits: 1).first.map(String.init) ?? "").trimmingCharacters(in: .whitespaces).lowercased()
    }

    /// The name a receiver sees: `name`, else the source's last segment; folder parts, control and bidi-override
    /// characters dropped; for a known type its extension is added when missing or different.
    static func fileName(_ source: String, _ name: String?, _ mime: String, fallback: String) -> String {
        let path = String(source.split(whereSeparator: { $0 == "?" || $0 == "#" }).first ?? "")
        let picked = (name?.trimmingCharacters(in: .whitespaces).isEmpty ?? true) ? path : name!
        let last = String(picked.split(whereSeparator: { $0 == "/" || $0 == "\\" }).last ?? "")
        let bidi: Set<Unicode.Scalar> = ["\u{200E}", "\u{200F}", "\u{202A}", "\u{202B}", "\u{202C}", "\u{202D}", "\u{202E}",
                                         "\u{2066}", "\u{2067}", "\u{2068}", "\u{2069}"]
        var base = String(String.UnicodeScalarView(last.unicodeScalars.filter {
            !CharacterSet.controlCharacters.contains($0) && !bidi.contains($0)
        })).trimmingCharacters(in: .whitespaces)
        if base.isEmpty || base == "." || base == ".." { base = fallback }
        let allowed: [String]
        switch bareMime(mime) {
        case "image/png": allowed = ["png"]
        case "image/jpeg": allowed = ["jpg", "jpeg"]
        case "image/webp": allowed = ["webp"]
        case "image/gif": allowed = ["gif"]
        case "application/pdf": allowed = ["pdf"]
        default: return base
        }
        let ext = base.contains(".") ? String(base.split(separator: ".").last ?? "").lowercased() : nil
        if let ext, allowed.contains(ext) { return base }
        return "\(base).\(allowed[0])"
    }

    /// Move or copy `file` into the share folder under `name` (unique), so the receiver sees that name.
    static func place(_ file: URL, as name: String, move: Bool) throws -> URL {
        let dir = try shareDir()
        let stem = (name as NSString).deletingPathExtension
        let ext = (name as NSString).pathExtension
        var out = dir.appendingPathComponent(name)
        var i = 1
        while FileManager.default.fileExists(atPath: out.path) {
            out = dir.appendingPathComponent(ext.isEmpty ? "\(stem)-\(i)" : "\(stem)-\(i).\(ext)")
            i += 1
        }
        if move { try FileManager.default.moveItem(at: file, to: out) } else { try FileManager.default.copyItem(at: file, to: out) }
        return out
    }
}
