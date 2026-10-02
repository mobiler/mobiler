import Foundation
import ImageIO
import SharedTypes
import UniformTypeIdentifiers

/// cx.pick_photo_with / capture_photo_with (ADR-0045): re-encode a picked or captured image to the asked
/// format and limits, upright and without metadata, into tmp. Mirrors mobiler_core::photo's rules.
enum PhotoPipeline {
    private struct Opts { var format = "original"; var maxDimension: Int?; var maxBytes: Int?; var quality = 85; var strip = true }

    private static func parse(_ input: String) -> Opts {
        var o = Opts()
        guard let data = input.data(using: .utf8), let j = (try? JSONSerialization.jsonObject(with: data)) as? [String: Any] else { return o }
        if let f = j["format"] as? String { o.format = f }
        o.maxDimension = ((j["max_dimension"] as? NSNumber)?.intValue).flatMap { $0 > 0 ? $0 : nil }
        o.maxBytes = (j["max_bytes"] as? NSNumber)?.intValue
        if let q = (j["quality"] as? NSNumber)?.intValue { o.quality = min(max(q, 1), 100) }
        if let s = j["strip_metadata"] as? Bool { o.strip = s }
        return o
    }

    private static func ladder(_ start: Int) -> [Int] {
        var out = [start], q = start
        while q > 45 { q = max(q - 10, 45); out.append(q) }
        return out
    }

    /// mobiler_core::photo::target_size: longest side ≤ max (never upscale), at most maxPixels.
    private static func target(_ w: Int, _ h: Int, _ maxDim: Int?) -> (Int, Int) {
        let longest = max(w, h)
        let dimensionCap = maxDim.flatMap { $0 > 0 && longest > $0 ? $0 : nil }
        var scale = dimensionCap.map { Double($0) / Double(longest) } ?? 1.0
        let pixels = Double(w) * Double(h) * scale * scale
        let pixelCapped = pixels > maxPixels
        if pixelCapped { scale *= (maxPixels / pixels).squareRoot() }
        if scale >= 1 { return (w, h) }
        var tw = max(1, Int((Double(w) * scale).rounded(.down)))
        var th = max(1, Int((Double(h) * scale).rounded(.down)))
        // When max_dimension is what scaled it, the long side is exactly max and the short side is
        // integer-scaled from it (floating floor can land one short, and a square must stay square).
        if let m = dimensionCap, !pixelCapped {
            if w >= h { tw = m; th = max(1, h * m / w) } else { th = m; tw = max(1, w * m / h) }
        }
        return (tw, th)
    }

    private static let maxPixels = 16_000_000.0

    private static let cleanPngChunks: Set<String> = ["IHDR", "PLTE", "IDAT", "IEND", "tRNS", "gAMA", "cHRM", "sRGB", "iCCP", "sBIT", "pHYs", "bKGD", "cICP"]

    /// mobiler_core::photo::png_is_clean: only a PNG (by its bytes) whose every chunk is harmless is
    /// proven free of metadata; anything else counts as carrying some.
    private static func pngIsClean(_ data: Data) -> Bool {
        let b = [UInt8](data)
        guard b.count >= 8, Array(b[0..<8]) == [0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A] else { return false }
        var i = 8
        while true {
            guard b.count - i >= 12 else { return false }
            let len = Int(b[i]) << 24 | Int(b[i + 1]) << 16 | Int(b[i + 2]) << 8 | Int(b[i + 3])
            let kind = String(decoding: b[(i + 4)..<(i + 8)], as: UTF8.self)
            guard cleanPngChunks.contains(kind) else { return false }
            let next = i + 12 + len
            guard next <= b.count else { return false }
            // Nothing may follow IEND (a cropped screenshot can keep the original there).
            if kind == "IEND" { return next == b.count }
            i = next
        }
    }

    static func process(_ url: URL, input: String) -> PluginResponse {
        let o = parse(input)
        guard let src = CGImageSourceCreateWithURL(url as CFURL, nil),
              let props = CGImageSourceCopyPropertiesAtIndex(src, 0, nil) as? [CFString: Any],
              var w = props[kCGImagePropertyPixelWidth] as? Int, var h = props[kCGImagePropertyPixelHeight] as? Int
        else { return PluginResponse(ok: false, output: "unsupported_image") }
        let orientation = props[kCGImagePropertyOrientation] as? Int ?? 1
        if orientation >= 5 { swap(&w, &h) } // 5–8 are the 90°/270° variants
        let utType = (CGImageSourceGetType(src) as String?).flatMap { UTType($0) }
        let mime = utType?.preferredMIMEType ?? "image/jpeg"
        let size = (try? url.resourceValues(forKeys: [.fileSizeKey]).fileSize) ?? 0
        // Fail closed (ADR-0045): only a PNG proven clean by its chunks counts as metadata-free.
        let hasMetadata = !pngIsClean((try? Data(contentsOf: url)) ?? Data())
        let fits = (o.maxDimension.map { max(w, h) <= $0 } ?? true) && (o.maxBytes.map { size <= $0 } ?? true)
        if o.format == "original" && fits && !(o.strip && hasMetadata) {
            return ok(url, mime, size, w, h)
        }
        let png = o.format == "png" || (o.format == "original" && utType == .png)
        let (tw, th) = target(w, h, o.maxDimension)
        let thumbOpts: [CFString: Any] = [
            kCGImageSourceCreateThumbnailFromImageAlways: true,
            kCGImageSourceCreateThumbnailWithTransform: true, // applies the orientation
            kCGImageSourceThumbnailMaxPixelSize: max(tw, th),
        ]
        guard let image = CGImageSourceCreateThumbnailAtIndex(src, 0, thumbOpts as CFDictionary) else {
            return PluginResponse(ok: false, output: "unsupported_image")
        }
        let outType: UTType = png ? .png : .jpeg // iOS can't encode WebP → JPEG
        for q in png ? [100] : ladder(o.quality) {
            let data = NSMutableData()
            guard let dest = CGImageDestinationCreateWithData(data as CFMutableData, outType.identifier as CFString, 1, nil) else { break }
            CGImageDestinationAddImage(dest, image, [kCGImageDestinationLossyCompressionQuality: Double(q) / 100] as CFDictionary)
            guard CGImageDestinationFinalize(dest) else { break }
            if o.maxBytes.map({ data.length <= $0 }) ?? true {
                let out = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString + (png ? ".png" : ".jpg"))
                guard (try? data.write(to: out)) != nil else { return PluginResponse(ok: false, output: "unavailable") }
                return ok(out, outType.preferredMIMEType ?? "image/jpeg", data.length, image.width, image.height)
            }
        }
        return PluginResponse(ok: false, output: "too_large")
    }

    /// `process`, then delete `source` unless it is the returned handle (a pass-through): it is an
    /// intermediate copy that may still carry the original's metadata.
    static func processReplacing(_ source: URL, input: String) -> PluginResponse {
        let result = process(source, input: input)
        // Keep `source` only when it is the returned handle (a pass-through); a failed re-encode
        // leaves nothing the app was given, so the copy goes too.
        let passedThrough = result.ok && (try? Photo.bincodeDeserialize(input: result.output))?.handle == source.absoluteString
        if !passedThrough { try? FileManager.default.removeItem(at: source) }
        return result
    }

    private static func ok(_ url: URL, _ mime: String, _ size: Int, _ w: Int, _ h: Int) -> PluginResponse {
        let photo = Photo(handle: url.absoluteString, mime: mime, bytes: UInt64(size), width: UInt32(w), height: UInt32(h))
        return PluginResponse(ok: true, output: (try? photo.bincodeSerialize()) ?? [])
    }
}
