import SharedTypes
import UIKit

/// Free bundled plugin: share a file via the system share sheet (no permission). op "file",
/// input = a file URL (e.g. a file:// from the photo/camera/audio plugins) → presents
/// UIActivityViewController; resolves "shared"/"cancelled".
@MainActor
enum SharefilePlugin {
    static func handle(op: String, input: String) async -> PluginResponse {
        guard op == "file" else { return PluginResponse(ok: false, output: "unknown op '\(op)'") }
        guard let url = URL(string: input) ?? (input.hasPrefix("/") ? URL(fileURLWithPath: input) : nil) else {
            return PluginResponse(ok: false, output: "bad uri")
        }
        guard let presenter = frontmostViewController() else {
            return PluginResponse(ok: false, output: "no view controller to present from")
        }
        return await withCheckedContinuation { cont in
            var resumed = false
            let vc = UIActivityViewController(activityItems: [url], applicationActivities: nil)
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
