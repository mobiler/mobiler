import Foundation
import SharedTypes
import UIKit

// NOTE (verify on macOS): `SharedTypes` is the facet-generated ABI types package
// (Widget/Action/Effect/Request/Requests/PluginCall/PluginResponse/...). `CoreFfi`
// comes from the uniffi-generated bindings for the `shared` crate; depending on the
// Xcode setup it's either in this same target (generated sources compiled in) or a
// module to `import`. project.yml wires that.

/// Drives the Rust core from Swift — the iOS twin of the Android `Core.kt`.
///
/// Speaks only the fixed Mobiler ABI: send an `Action`, receive a `Widget` tree +
/// capability effects. Request/response capabilities resolve **asynchronously**
/// (Swift `async/await` / `Task`), so a network call never blocks the UI — exactly
/// like the Android shell's coroutine resolution.
@MainActor
final class Core: ObservableObject {
    @Published private(set) var view: Widget

    private let core = CoreFfi()

    init() {
        // First frame straight from the core's view model.
        self.view = try! Widget.bincodeDeserialize(input: [UInt8](core.view()))
        // Restore persisted state, then fire Start so the app can load initial data.
        let saved = StoragePlugin.load()
        if !saved.isEmpty { update(.restore(data: saved)) }
        update(.start)
    }

    func update(_ action: Action) {
        process(core.update(data: Data(try! action.bincodeSerialize())))
    }

    private func process(_ effectBytes: Data) {
        let requests = try! Requests.bincodeDeserialize(input: [UInt8](effectBytes)).value
        for request in requests {
            switch request.effect {
            case .render:
                self.view = try! Widget.bincodeDeserialize(input: [UInt8](core.view()))

            // Fire-and-forget: dispatch, ignore the result, don't resolve.
            case .pluginNotify(let notify):
                Task { _ = await Plugins.handle(plugin: notify.plugin, op: notify.op, input: notify.input) }

            // Request/response: dispatch (awaiting async work), resolve the core
            // with the response, then process the effects that produces.
            case .plugin(let call):
                let id = request.id
                Task {
                    let resp = await Plugins.handle(plugin: call.plugin, op: call.op, input: call.input)
                    let next = core.resolve(id: id, data: Data(try! resp.bincodeSerialize()))
                    process(next)
                }
            }
        }
    }
}

// MARK: - Capability plugins (the iOS twin of the Android plugin registry)

/// Dispatches the opaque `{plugin, op, input}` envelope by name. Adding a plugin
/// never touches the wire ABI — only this registry.
enum Plugins {
    static func handle(plugin: String, op: String, input: String) async -> PluginResponse {
        switch plugin {
        case "http": return await HttpPlugin.handle(op: op, input: input)
        case "storage": return StoragePlugin.handle(op: op, input: input)
        case "clipboard": return await ClipboardPlugin.handle(op: op, input: input)
        case "share": return await SharePlugin.handle(op: op, input: input)
        case "browser": return await BrowserPlugin.handle(op: op, input: input)
        case "toast": return await ToastPlugin.handle(op: op, input: input)
        case "device": return await DevicePlugin.handle(op: op, input: input)
        case "haptics": return await HapticsPlugin.handle(op: op, input: input)
        case "dialog": return await DialogPlugin.handle(op: op, input: input)
        default:
            return PluginResponse(ok: false, output: "plugin '\(plugin)' not available in this build")
        }
    }
}

/// HTTP capability (paired with `cx.http`/`get`/`post`/... in Rust). `op` is the
/// method; `input` is `{"url": ..., "body": ...}`. Returns the body; `ok` = 2xx.
enum HttpPlugin {
    static func handle(op: String, input: String) async -> PluginResponse {
        guard
            let data = input.data(using: .utf8),
            let obj = try? JSONSerialization.jsonObject(with: data) as? [String: Any],
            let urlString = obj["url"] as? String,
            let url = URL(string: urlString)
        else {
            return PluginResponse(ok: false, output: "invalid http request")
        }
        var req = URLRequest(url: url)
        req.httpMethod = op
        if let body = obj["body"] as? String {
            req.httpBody = body.data(using: .utf8)
            req.setValue("application/json", forHTTPHeaderField: "Content-Type")
        }
        do {
            let (respData, resp) = try await URLSession.shared.data(for: req)
            let code = (resp as? HTTPURLResponse)?.statusCode ?? 0
            let ok = (200..<300).contains(code)
            return PluginResponse(ok: ok, output: String(data: respData, encoding: .utf8) ?? "")
        } catch {
            return PluginResponse(ok: false, output: error.localizedDescription)
        }
    }
}

/// Persistence capability (paired with `cx.save` + `restore`). Backed by UserDefaults.
enum StoragePlugin {
    private static let key = "mobiler.state"
    static func load() -> String { UserDefaults.standard.string(forKey: key) ?? "" }
    static func handle(op: String, input: String) -> PluginResponse {
        switch op {
        case "save": UserDefaults.standard.set(input, forKey: key); return PluginResponse(ok: true, output: "")
        case "load": return PluginResponse(ok: true, output: load())
        default: return PluginResponse(ok: false, output: "unknown op '\(op)'")
        }
    }
}

/// Clipboard capability — copy text (UIPasteboard is main-actor only).
@MainActor
enum ClipboardPlugin {
    static func handle(op: String, input: String) -> PluginResponse {
        UIPasteboard.general.string = input
        return PluginResponse(ok: true, output: "")
    }
}

/// Share capability — the system share sheet (UIActivityViewController).
@MainActor
enum SharePlugin {
    static func handle(op: String, input: String) -> PluginResponse {
        guard let presenter = topViewController() else {
            return PluginResponse(ok: false, output: "no view controller to present from")
        }
        let sheet = UIActivityViewController(activityItems: [input], applicationActivities: nil)
        sheet.popoverPresentationController?.sourceView = presenter.view // iPad anchor
        presenter.present(sheet, animated: true)
        return PluginResponse(ok: true, output: "")
    }
}

/// Open a URL externally (Safari / the default handler).
@MainActor
enum BrowserPlugin {
    static func handle(op: String, input: String) -> PluginResponse {
        guard let url = URL(string: input) else {
            return PluginResponse(ok: false, output: "invalid url")
        }
        UIApplication.shared.open(url)
        return PluginResponse(ok: true, output: "")
    }
}

/// Device info — request/response. `model` returns e.g. "Apple iPhone (iOS 18.0)".
@MainActor
enum DevicePlugin {
    static func handle(op: String, input: String) -> PluginResponse {
        switch op {
        case "model":
            let d = UIDevice.current
            return PluginResponse(ok: true, output: "Apple \(d.model) (\(d.systemName) \(d.systemVersion))")
        default:
            return PluginResponse(ok: false, output: "unknown op '\(op)'")
        }
    }
}

/// Haptic tap — iOS has no permission requirement. `op` is the style.
@MainActor
enum HapticsPlugin {
    static func handle(op: String, input: String) -> PluginResponse {
        let style: UIImpactFeedbackGenerator.FeedbackStyle = switch op {
        case "light": .light
        case "heavy": .heavy
        default: .medium
        }
        UIImpactFeedbackGenerator(style: style).impactOccurred()
        return PluginResponse(ok: true, output: "")
    }
}

/// Toast — iOS has no native toast, so show a transient padded label in the key
/// window (the SwiftUI/UIKit twin of Android's Toast / the web's `.toast` div).
@MainActor
enum ToastPlugin {
    static func handle(op: String, input: String) -> PluginResponse {
        guard let window = keyWindow() else { return PluginResponse(ok: false, output: "no window") }
        let label = PaddedLabel()
        label.text = input
        label.numberOfLines = 0
        label.textColor = .white
        label.textAlignment = .center
        label.font = .systemFont(ofSize: 14)
        label.backgroundColor = UIColor.black.withAlphaComponent(0.85)
        label.layer.cornerRadius = 18
        label.clipsToBounds = true
        label.alpha = 0
        label.translatesAutoresizingMaskIntoConstraints = false
        window.addSubview(label)
        NSLayoutConstraint.activate([
            label.centerXAnchor.constraint(equalTo: window.centerXAnchor),
            label.bottomAnchor.constraint(equalTo: window.safeAreaLayoutGuide.bottomAnchor, constant: -32),
            label.leadingAnchor.constraint(greaterThanOrEqualTo: window.leadingAnchor, constant: 24),
            label.trailingAnchor.constraint(lessThanOrEqualTo: window.trailingAnchor, constant: -24),
        ])
        UIView.animate(withDuration: 0.2) { label.alpha = 1 }
        UIView.animate(withDuration: 0.3, delay: 2.3) { label.alpha = 0 } completion: { _ in label.removeFromSuperview() }
        return PluginResponse(ok: true, output: "")
    }
}

/// Confirm dialog — request/response. Presents a UIAlertController and awaits the
/// user's choice (`ok` = confirmed) via a continuation, so the core resolves only
/// once they tap. Input is JSON `{title, message}`.
@MainActor
enum DialogPlugin {
    static func handle(op: String, input: String) async -> PluginResponse {
        guard op == "confirm" else { return PluginResponse(ok: false, output: "unknown op '\(op)'") }
        let obj = (try? JSONSerialization.jsonObject(with: Data(input.utf8))) as? [String: Any]
        let title = obj?["title"] as? String ?? ""
        let message = obj?["message"] as? String ?? ""
        guard let presenter = topViewController() else {
            return PluginResponse(ok: false, output: "no view controller to present from")
        }
        return await withCheckedContinuation { cont in
            let alert = UIAlertController(
                title: title.isEmpty ? nil : title, message: message, preferredStyle: .alert)
            alert.addAction(UIAlertAction(title: "Cancel", style: .cancel) { _ in
                cont.resume(returning: PluginResponse(ok: false, output: "cancel"))
            })
            alert.addAction(UIAlertAction(title: "OK", style: .default) { _ in
                cont.resume(returning: PluginResponse(ok: true, output: "ok"))
            })
            presenter.present(alert, animated: true)
        }
    }
}

/// A UILabel with inner padding (UILabel alone has none) — for the toast pill.
private final class PaddedLabel: UILabel {
    private let insets = UIEdgeInsets(top: 10, left: 18, bottom: 10, right: 18)
    override func drawText(in rect: CGRect) { super.drawText(in: rect.inset(by: insets)) }
    override var intrinsicContentSize: CGSize {
        let s = super.intrinsicContentSize
        return CGSize(width: s.width + insets.left + insets.right, height: s.height + insets.top + insets.bottom)
    }
}

/// The active key window — where the shell hangs modals/toasts (it owns no VC).
@MainActor
private func keyWindow() -> UIWindow? {
    (UIApplication.shared.connectedScenes
        .first { $0.activationState == .foregroundActive } as? UIWindowScene)?.keyWindow
}

/// The frontmost view controller — modals (the share sheet) present from here.
@MainActor
private func topViewController() -> UIViewController? {
    var top = keyWindow()?.rootViewController
    while let presented = top?.presentedViewController { top = presented }
    return top
}
