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
        o.maxDimension = (j["max_dimension"] as? NSNumber)?.intValue
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

    private static func target(_ w: Int, _ h: Int, _ maxDim: Int?) -> (Int, Int) {
        let longest = max(w, h)
        guard let m = maxDim, m > 0, longest > m else { return (w, h) }
        let s = Double(m) / Double(longest)
        return (max(1, Int((Double(w) * s).rounded())), max(1, Int((Double(h) * s).rounded())))
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
        let hasMetadata = orientation != 1 || props[kCGImagePropertyGPSDictionary] != nil || props[kCGImagePropertyExifDictionary] != nil
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
            guard let dest = CGImageDestinationCreateWithData(data, outType.identifier as CFString, 1, nil) else { break }
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

    private static func ok(_ url: URL, _ mime: String, _ size: Int, _ w: Int, _ h: Int) -> PluginResponse {
        let photo = Photo(handle: url.absoluteString, mime: mime, bytes: UInt64(size), width: UInt32(w), height: UInt32(h))
        return PluginResponse(ok: true, output: (try? photo.bincodeSerialize()) ?? [])
    }
}
