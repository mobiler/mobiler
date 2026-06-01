import SharedTypes
import UIKit
import UniformTypeIdentifiers

/// Free bundled plugin: pick a file via the system document picker (no permission).
/// op "pick" → a local file:// URL on success (the picker copies the file), ok=false on cancel.
@MainActor
enum FilePickerPlugin {
    static func handle(op: String, input: String) async -> PluginResponse {
        guard op == "pick" else { return PluginResponse(ok: false, output: "unknown op '\(op)'") }
        guard let presenter = frontmostViewController() else {
            return PluginResponse(ok: false, output: "no view controller to present from")
        }
        return await withCheckedContinuation { cont in
            // asCopy: true delivers a copy in a temp dir we can read without a security scope.
            let picker = UIDocumentPickerViewController(forOpeningContentTypes: [.item], asCopy: true)
            picker.allowsMultipleSelection = false
            let delegate = DocPickerDelegate { cont.resume(returning: $0) }
            DocPickerDelegate.retained = delegate // the picker holds its delegate weakly
            picker.delegate = delegate
            presenter.present(picker, animated: true)
        }
    }

    // A standalone plugin file can't call Core.swift's private topViewController(), so it
    // finds the frontmost VC itself (same approach as the scanner plugin).
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

private final class DocPickerDelegate: NSObject, UIDocumentPickerDelegate {
    static var retained: DocPickerDelegate?
    private let onResult: (PluginResponse) -> Void
    init(onResult: @escaping (PluginResponse) -> Void) { self.onResult = onResult }

    func documentPicker(_ controller: UIDocumentPickerViewController, didPickDocumentsAt urls: [URL]) {
        guard let url = urls.first else { finish(PluginResponse(ok: false, output: "cancelled")); return }
        finish(PluginResponse(ok: true, output: url.absoluteString))
    }
    func documentPickerWasCancelled(_ controller: UIDocumentPickerViewController) {
        finish(PluginResponse(ok: false, output: "cancelled"))
    }
    private func finish(_ r: PluginResponse) {
        onResult(r)
        DocPickerDelegate.retained = nil
    }
}
