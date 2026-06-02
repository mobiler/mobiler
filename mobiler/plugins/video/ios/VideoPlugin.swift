import SharedTypes
import UIKit
import UniformTypeIdentifiers

/// Free bundled plugin: record a video with the camera. op "record" → a local file:// URL on
/// success, ok=false on cancel. Needs NSCameraUsageDescription + NSMicrophoneUsageDescription.
@MainActor
enum VideoPlugin {
    static func handle(op: String, input: String) async -> PluginResponse {
        guard op == "record" else { return PluginResponse(ok: false, output: "unknown op '\(op)'") }
        guard UIImagePickerController.isSourceTypeAvailable(.camera) else {
            return PluginResponse(ok: false, output: "camera not available")
        }
        guard let presenter = frontmostViewController() else {
            return PluginResponse(ok: false, output: "no view controller to present from")
        }
        return await withCheckedContinuation { cont in
            let picker = UIImagePickerController()
            picker.sourceType = .camera
            picker.mediaTypes = [UTType.movie.identifier]
            picker.cameraCaptureMode = .video
            let delegate = VideoDelegate { cont.resume(returning: $0) }
            VideoDelegate.retained = delegate
            picker.delegate = delegate
            presenter.present(picker, animated: true)
        }
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

private final class VideoDelegate: NSObject, UIImagePickerControllerDelegate, UINavigationControllerDelegate {
    static var retained: VideoDelegate?
    private let onResult: (PluginResponse) -> Void
    init(onResult: @escaping (PluginResponse) -> Void) { self.onResult = onResult }

    func imagePickerController(_ picker: UIImagePickerController, didFinishPickingMediaWithInfo info: [UIImagePickerController.InfoKey: Any]) {
        picker.dismiss(animated: true)
        if let url = info[.mediaURL] as? URL {
            finish(PluginResponse(ok: true, output: url.absoluteString))
        } else {
            finish(PluginResponse(ok: false, output: "no video"))
        }
    }
    func imagePickerControllerDidCancel(_ picker: UIImagePickerController) {
        picker.dismiss(animated: true)
        finish(PluginResponse(ok: false, output: "cancelled"))
    }
    private func finish(_ r: PluginResponse) {
        onResult(r)
        VideoDelegate.retained = nil
    }
}
