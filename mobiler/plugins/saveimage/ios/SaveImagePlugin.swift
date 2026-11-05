import ImageIO
import Photos
import SharedTypes
import UIKit
import UniformTypeIdentifiers

/// Free bundled plugin: save an image to Photos. Op "save", input {source, name?, headers?}: an https URL (fetched
/// here, up to 50 MB), a file URL, an absolute path, or a path relative to Documents (a local file must be the app's
/// own). The bytes must be a PNG, JPEG, GIF or WebP and are saved unchanged. Asks for add-only photo access
/// (NSPhotoLibraryAddUsageDescription).
/// Replies: ok "saved" once Photos has it, else ok:false "<code>" or "<code>: <detail>" (ADR-0068).
@MainActor
enum SaveImagePlugin {
    static func handle(op: String, input: String) async -> PluginResponse {
        guard op == "save" else { return PluginResponse(ok: false, output: "unavailable: unknown op '\(op)'") }
        var fetched: URL?
        defer { if let fetched { try? FileManager.default.removeItem(at: fetched) } }
        do {
            let ask = try Source.parse(input, bareAllowed: false)
            let file: URL
            if ask.source.lowercased().hasPrefix("https://") {
                file = try await Source.fetch(ask).0
                fetched = file
            } else {
                file = try Source.localFile(ask.source)
                guard FileManager.default.isReadableFile(atPath: file.path) else { throw Refused("failed", "can't read the file") }
            }
            guard let mime = Source.sniffImage(file),
                  let image = CGImageSourceCreateWithURL(file as CFURL, nil),
                  CGImageSourceGetCount(image) > 0, CGImageSourceGetStatus(image) == .statusComplete else {
                throw Refused("not_an_image")
            }
            let status = await PHPhotoLibrary.requestAuthorization(for: .addOnly)
            guard status == .authorized || status == .limited else { throw Refused("denied") }
            let name = Source.fileName(ask.source, ask.name, mime, fallback: "image")
            let type = UTType(mimeType: mime)?.identifier
            // Photos reads the file during performChanges; a fetched file is removed only after that returns (defer).
            // The type is given explicitly: a fetched file's name ends in .part.
            try await PHPhotoLibrary.shared().performChanges {
                let options = PHAssetResourceCreationOptions()
                options.originalFilename = name
                options.uniformTypeIdentifier = type
                PHAssetCreationRequest.forAsset().addResource(with: .photo, fileURL: file, options: options)
            }
            return PluginResponse(ok: true, output: "saved")
        } catch let r as Refused {
            return PluginResponse(ok: false, output: r.reply)
        } catch {
            return PluginResponse(ok: false, output: "failed: \(error.localizedDescription)")
        }
    }
}

/// A refusal with its reply code (ADR-0068).
private struct Refused: Error {
    let reply: String
    init(_ code: String, _ detail: String? = nil) {
        reply = (detail?.isEmpty ?? true) ? code : "\(code): \(detail!)"
    }
}

/// Follows a redirect only to https, and carries the caller's headers only to the original host.
private final class RedirectGuard: NSObject, URLSessionTaskDelegate {
    let origin: URL
    let headerNames: [String]
    var refusedInsecure = false

    init(origin: URL, headerNames: [String]) {
        self.origin = origin
        self.headerNames = headerNames
    }

    func urlSession(_ session: URLSession, task: URLSessionTask, willPerformHTTPRedirection response: HTTPURLResponse,
                    newRequest request: URLRequest) async -> URLRequest? {
        guard let next = request.url, next.scheme?.lowercased() == "https" else {
            refusedInsecure = true
            return nil
        }
        var r = request
        if next.host?.lowercased() != origin.host?.lowercased() || (next.port ?? 443) != (origin.port ?? 443) {
            for name in headerNames { r.setValue(nil, forHTTPHeaderField: name) }
        }
        return r
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
        if ask.source.contains(where: { $0 == "\t" || $0 == "\n" || $0 == "\r" }) { throw Refused("unsupported_source") }
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

    /// The app's own file: a file:// URL, an absolute path, or a path relative to Documents, which must lie in
    /// Documents, Library/Caches, Library/Application Support or tmp (never Library/Preferences).
    static func localFile(_ src: String) throws -> URL {
        let fm = FileManager.default
        let docs = try fm.url(for: .documentDirectory, in: .userDomainMask, appropriateFor: nil, create: true)
        let raw: URL
        if src.lowercased().hasPrefix("file://") {
            guard let u = URL(string: src), u.isFileURL else { throw Refused("bad_input") }
            raw = u
        } else if src.hasPrefix("/") {
            raw = URL(fileURLWithPath: src)
        } else {
            raw = docs.appendingPathComponent(src)
        }
        let url = raw.standardizedFileURL.resolvingSymlinksInPath()
        let roots = [docs,
                     try? fm.url(for: .cachesDirectory, in: .userDomainMask, appropriateFor: nil, create: false),
                     try? fm.url(for: .applicationSupportDirectory, in: .userDomainMask, appropriateFor: nil, create: false),
                     fm.temporaryDirectory]
            .compactMap { $0?.standardizedFileURL.resolvingSymlinksInPath().path }
            .map { $0.hasSuffix("/") ? $0 : $0 + "/" }
        guard roots.contains(where: { url.path.hasPrefix($0) }) else { throw Refused("unsupported_source", "not one of the app's files") }
        var isDir: ObjCBool = false
        guard fm.fileExists(atPath: url.path, isDirectory: &isDir), !isDir.boolValue else { throw Refused("not_found") }
        return url
    }

    /// GET an https source into the share folder, stopping at 50 MB: a non-2xx status or a network error is
    /// fetch_failed, a redirect to http is insecure_source, more than 50 MB is too_large. The caller's headers go
    /// only to the original host. Returns the file and the response's declared type.
    static func fetch(_ ask: Ask) async throws -> (URL, String) {
        guard let url = URL(string: ask.source) else { throw Refused("bad_input") }
        var req = URLRequest(url: url, timeoutInterval: 60)
        for (k, v) in ask.headers { req.setValue(v, forHTTPHeaderField: k) }
        let guardian = RedirectGuard(origin: url, headerNames: Array(ask.headers.keys))
        let bytes: URLSession.AsyncBytes
        let response: URLResponse
        do {
            (bytes, response) = try await URLSession.shared.bytes(for: req, delegate: guardian)
        } catch {
            throw Refused("fetch_failed", error.localizedDescription)
        }
        if let http = response as? HTTPURLResponse, !(200..<300).contains(http.statusCode) {
            bytes.task.cancel()
            if guardian.refusedInsecure { throw Refused("insecure_source", "redirected to http") }
            throw Refused("fetch_failed", "HTTP \(http.statusCode)")
        }
        if response.expectedContentLength > maxBytes {
            bytes.task.cancel()
            throw Refused("too_large")
        }
        let out = try shareDir().appendingPathComponent("in-\(UUID().uuidString).part")
        guard FileManager.default.createFile(atPath: out.path, contents: nil),
              let handle = try? FileHandle(forWritingTo: out) else { throw Refused("failed", "can't write the file") }
        do {
            defer { try? handle.close() }
            var buffer = Data()
            buffer.reserveCapacity(64 * 1024)
            var total: Int64 = 0
            for try await byte in bytes {
                buffer.append(byte)
                if buffer.count == 64 * 1024 {
                    total += Int64(buffer.count)
                    if total > maxBytes { bytes.task.cancel(); throw Refused("too_large") }
                    try handle.write(contentsOf: buffer)
                    buffer.removeAll(keepingCapacity: true)
                }
            }
            total += Int64(buffer.count)
            if total > maxBytes { throw Refused("too_large") }
            try handle.write(contentsOf: buffer)
        } catch let r as Refused {
            try? FileManager.default.removeItem(at: out)
            throw r
        } catch {
            try? FileManager.default.removeItem(at: out)
            throw Refused("fetch_failed", error.localizedDescription)
        }
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

    /// Characters a name loses: controls, and the invisible and bidi-override ones that can disguise an extension.
    static func hidden(_ s: Unicode.Scalar) -> Bool {
        CharacterSet.controlCharacters.contains(s) || (0x200B...0x200F).contains(s.value) || (0x202A...0x202E).contains(s.value)
            || (0x2066...0x2069).contains(s.value) || s.value == 0x061C || s.value == 0xFEFF
    }

    /// The name a receiver sees: `name`, else the source's last segment; folder parts and hidden characters
    /// dropped; for a known type its extension is added when missing or different.
    static func fileName(_ source: String, _ name: String?, _ mime: String, fallback: String) -> String {
        let path = source.components(separatedBy: CharacterSet(charactersIn: "?#")).first ?? ""
        let picked = (name?.trimmingCharacters(in: .whitespaces).isEmpty ?? true) ? path : name!
        let last = picked.components(separatedBy: CharacterSet(charactersIn: "/\\")).last ?? ""
        var base = String(String.UnicodeScalarView(last.unicodeScalars.filter { !hidden($0) })).trimmingCharacters(in: .whitespaces)
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
        let ext = base.contains(".") ? (base.components(separatedBy: ".").last ?? "").lowercased() : nil
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
        do {
            if move { try FileManager.default.moveItem(at: file, to: out) } else { try FileManager.default.copyItem(at: file, to: out) }
        } catch {
            throw Refused("failed", "could not stage the file")
        }
        return out
    }
}
