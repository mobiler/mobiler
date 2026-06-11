import Foundation
import SharedTypes
import UIKit
import PhotosUI
import UniformTypeIdentifiers

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
    // Live streaming subscriptions (cx.subscribe), keyed by subscription key, so
    // cx.unsubscribe(key) can cancel the matching native source.
    private var streamTasks: [String: Task<Void, Never>] = [:]

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

            // Fire-and-forget: dispatch, ignore the result, don't resolve. The
            // `stream`/`unsubscribe` control notify cancels a live subscription.
            case .pluginNotify(let notify):
                if notify.plugin == "stream", notify.op == "unsubscribe" {
                    streamTasks[notify.input]?.cancel()
                    streamTasks[notify.input] = nil
                } else {
                    Task { _ = await Plugins.handle(plugin: notify.plugin, op: notify.op, input: notify.input) }
                }

            // Request/response: dispatch (awaiting async work), resolve the core
            // with the response, then process the effects that produces.
            case .plugin(let call):
                let id = request.id
                Task {
                    let resp = await Plugins.handle(plugin: call.plugin, op: call.op, input: call.input)
                    let next = core.resolve(id: id, data: Data(try! resp.bincodeSerialize()))
                    process(next)
                }

            // Streaming subscription: start a native source that resolves the SAME
            // request id repeatedly (one PluginResponse per event). `emit` hops to the
            // main actor to touch the core/view. Parked by key for unsubscribe.
            case .pluginStream(let call):
                let id = request.id
                let emit: @Sendable (PluginResponse) -> Void = { [weak self] resp in
                    Task { @MainActor in
                        guard let self else { return }
                        self.process(self.core.resolve(id: id, data: Data(try! resp.bincodeSerialize())))
                    }
                }
                streamTasks[call.key] = Task {
                    await Plugins.subscribe(plugin: call.plugin, op: call.op, input: call.input, emit: emit)
                }
            }
        }
    }
}

// MARK: - Capability plugins (the iOS twin of the Android plugin registry)

/// Built-in `ticker` stream: emits an incrementing counter every `input` ms until the
/// subscription's Task is cancelled (cx.unsubscribe). The deterministic demonstrator
/// for the streaming primitive (cx.subscribe).
enum TickerStream {
    static func run(input: String, emit: @Sendable (PluginResponse) -> Void) async {
        let ms = UInt64(input) ?? 1000
        var count = 0
        while !Task.isCancelled {
            try? await Task.sleep(nanoseconds: ms * 1_000_000)
            if Task.isCancelled { break }
            count += 1
            emit(PluginResponse(ok: true, output: String(count)))
        }
    }
}

/// Streaming dispatch for the built-in `system` source: bridges deep-link + lifecycle events from
/// SystemBridge (App.swift) into the cx.subscribe stream. Attaches on subscribe — flushing any
/// buffered launch deep-link — parks until the Task is cancelled (cx.unsubscribe), then detaches.
enum SystemStream {
    static func run(emit: @escaping @Sendable (PluginResponse) -> Void) async {
        let sink: @Sendable (String) -> Void = { emit(PluginResponse(ok: true, output: $0)) }
        await MainActor.run { SystemBridge.shared.attach(sink) }
        await withTaskCancellationHandler {
            while !Task.isCancelled { try? await Task.sleep(nanoseconds: 1_000_000_000) }
        } onCancel: {
            Task { @MainActor in SystemBridge.shared.detach() }
        }
    }
}

/// Dispatches the opaque `{plugin, op, input}` envelope by name. Adding a plugin
/// never touches the wire ABI — only this registry.
enum Plugins {
    /// Streaming dispatch (cx.subscribe): start a long-lived source that calls `emit`
    /// per event until cancelled. Streaming-capable capabilities are matched here.
    static func subscribe(plugin: String, op: String, input: String, emit: @escaping @Sendable (PluginResponse) -> Void) async {
        switch plugin {
        case "ticker": await TickerStream.run(input: input, emit: emit)
        case "system": await SystemStream.run(emit: emit)
        // mobiler:plugins-stream — streaming plugins inserted above this line
        default: break
        }
    }

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
        case "datetime": return await DateTimePlugin.handle(op: op, input: input)
        case "picker": return await PickerPlugin.handle(op: op, input: input)
        case "photo": return await PhotoPlugin.handle(op: op, input: input)
        case "camera": return await CameraPlugin.handle(op: op, input: input)
        case "sqlite": return await SqlitePlugin.handle(op: op, input: input)
        case "files": return await FilesPlugin.handle(op: op, input: input)
        case "filepicker": return await FilePickerPlugin.handle(op: op, input: input)
        case "biometric": return await BiometricPlugin.handle(op: op, input: input)
        // mobiler:plugins — `mobiler plugin add` inserts plugin cases above this line
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
        case "locale":
            return PluginResponse(ok: true, output: Locale.preferredLanguages.first ?? Locale.current.identifier)
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

/// Native date / time capability — request/response. `op`:
///   "now"  → the current local date-time as "yyyy-MM-dd HH:mm:ss" (no UI, resolves immediately),
///   "date" → a picker returning ISO "YYYY-MM-DD", "time" → a picker returning 24-hour "HH:MM".
/// Pickers resolve `ok=false` on cancel. The pickers present a UIDatePicker in an action sheet.
@MainActor
enum DateTimePlugin {
    static func handle(op: String, input: String) async -> PluginResponse {
        // "now" needs no UI — stamp the current local date-time and return at once.
        if op == "now" {
            let fmt = DateFormatter()
            fmt.locale = Locale(identifier: "en_US_POSIX")
            fmt.dateFormat = "yyyy-MM-dd HH:mm:ss"
            return PluginResponse(ok: true, output: fmt.string(from: Date()))
        }
        let mode: UIDatePicker.Mode
        switch op {
        case "date": mode = .date
        case "time": mode = .time
        default: return PluginResponse(ok: false, output: "unknown op '\(op)'")
        }
        guard let presenter = topViewController() else {
            return PluginResponse(ok: false, output: "no view controller to present from")
        }
        return await withCheckedContinuation { cont in
            let picker = UIDatePicker()
            picker.datePickerMode = mode
            picker.preferredDatePickerStyle = .wheels
            picker.translatesAutoresizingMaskIntoConstraints = false

            // An action sheet with blank message lines reserves room for the wheel picker.
            let alert = UIAlertController(
                title: op == "date" ? "Pick a date" : "Pick a time",
                message: "\n\n\n\n\n\n\n\n\n", preferredStyle: .actionSheet)
            alert.view.addSubview(picker)
            NSLayoutConstraint.activate([
                picker.centerXAnchor.constraint(equalTo: alert.view.centerXAnchor),
                picker.topAnchor.constraint(equalTo: alert.view.topAnchor, constant: 48),
                picker.widthAnchor.constraint(equalTo: alert.view.widthAnchor, constant: -16),
            ])

            let fmt = DateFormatter()
            fmt.locale = Locale(identifier: "en_US_POSIX")
            fmt.dateFormat = op == "date" ? "yyyy-MM-dd" : "HH:mm"

            var resumed = false
            func done(_ r: PluginResponse) { if !resumed { resumed = true; cont.resume(returning: r) } }
            alert.addAction(UIAlertAction(title: "Cancel", style: .cancel) { _ in
                done(PluginResponse(ok: false, output: "cancel"))
            })
            alert.addAction(UIAlertAction(title: "Done", style: .default) { _ in
                done(PluginResponse(ok: true, output: fmt.string(from: picker.date)))
            })
            // iPad presents action sheets in a popover, which needs a source.
            alert.popoverPresentationController?.sourceView = presenter.view
            alert.popoverPresentationController?.sourceRect = CGRect(
                x: presenter.view.bounds.midX, y: presenter.view.bounds.midY, width: 0, height: 0)
            presenter.present(alert, animated: true)
        }
    }
}

/// Native single-choice picker — request/response. Input `{"title": String, "options": [String],
/// "selected": Int}`. Presents a `UIPickerView` **wheel** in a half-height **bottom sheet** (a Cancel /
/// title / Done toolbar above the wheel) — like the new-transaction sheet, so it has no stray popover
/// arrow. Resolves `ok=true` with the chosen index as a string, or `ok=false` on cancel/dismiss.
@MainActor
enum PickerPlugin {
    static func handle(op: String, input: String) async -> PluginResponse {
        guard op == "choose" else { return PluginResponse(ok: false, output: "unknown op '\(op)'") }
        guard let data = input.data(using: .utf8),
              let obj = (try? JSONSerialization.jsonObject(with: data)) as? [String: Any],
              let options = obj["options"] as? [String], !options.isEmpty else {
            return PluginResponse(ok: false, output: "bad input")
        }
        let title = obj["title"] as? String
        let selected = obj["selected"] as? Int ?? 0
        guard let presenter = topViewController() else {
            return PluginResponse(ok: false, output: "no view controller to present from")
        }
        return await withCheckedContinuation { cont in
            var resumed = false
            func done(_ r: PluginResponse) { if !resumed { resumed = true; cont.resume(returning: r) } }
            let vc = PickerSheetController(
                title: title, options: options, selected: selected,
                onDone: { done(PluginResponse(ok: true, output: String($0))) },
                onCancel: { done(PluginResponse(ok: false, output: "cancel")) })
            vc.modalPresentationStyle = .pageSheet
            if let sheet = vc.sheetPresentationController {
                sheet.detents = [.medium()]
                sheet.prefersGrabberVisible = true
            }
            vc.presentationController?.delegate = vc // swipe-to-dismiss = cancel
            presenter.present(vc, animated: true)
        }
    }
}

/// The bottom-sheet host for the picker wheel — a Cancel / title / Done toolbar over a single-column
/// `UIPickerView`. Owns its data source/delegate (retained by the presentation, so no static needed).
private final class PickerSheetController: UIViewController, UIPickerViewDataSource, UIPickerViewDelegate, UIAdaptivePresentationControllerDelegate {
    private let options: [String]
    private let initialSelected: Int
    private let onDone: (Int) -> Void
    private let onCancel: () -> Void
    private let picker = UIPickerView()
    private var finished = false

    init(title: String?, options: [String], selected: Int, onDone: @escaping (Int) -> Void, onCancel: @escaping () -> Void) {
        self.options = options
        self.initialSelected = selected
        self.onDone = onDone
        self.onCancel = onCancel
        super.init(nibName: nil, bundle: nil)
        self.title = title
    }
    @available(*, unavailable) required init?(coder: NSCoder) { fatalError("init(coder:) unavailable") }

    override func viewDidLoad() {
        super.viewDidLoad()
        view.backgroundColor = .systemBackground
        // Match the app's brand tint (this sheet is presented outside SwiftUI's `.tint`).
        if let seed = ActiveTheme.current?.seed {
            view.tintColor = UIColor(red: CGFloat(seed.r) / 255, green: CGFloat(seed.g) / 255,
                                     blue: CGFloat(seed.b) / 255, alpha: 1)
        }

        let toolbar = UIToolbar()
        toolbar.translatesAutoresizingMaskIntoConstraints = false
        let titleItem = UIBarButtonItem(title: title, style: .plain, target: nil, action: nil)
        titleItem.isEnabled = false
        // The title is a non-tappable label; keep it fully legible (a disabled item is greyed out by default).
        let titleAttrs: [NSAttributedString.Key: Any] = [
            .foregroundColor: UIColor.label,
            .font: UIFont.preferredFont(forTextStyle: .headline),
        ]
        titleItem.setTitleTextAttributes(titleAttrs, for: .normal)
        titleItem.setTitleTextAttributes(titleAttrs, for: .disabled)
        toolbar.items = [
            UIBarButtonItem(barButtonSystemItem: .cancel, target: self, action: #selector(cancelTapped)),
            UIBarButtonItem(barButtonSystemItem: .flexibleSpace, target: nil, action: nil),
            titleItem,
            UIBarButtonItem(barButtonSystemItem: .flexibleSpace, target: nil, action: nil),
            UIBarButtonItem(barButtonSystemItem: .done, target: self, action: #selector(doneTapped)),
        ]

        picker.translatesAutoresizingMaskIntoConstraints = false
        picker.dataSource = self
        picker.delegate = self
        if initialSelected >= 0, initialSelected < options.count {
            picker.selectRow(initialSelected, inComponent: 0, animated: false)
        }

        view.addSubview(toolbar)
        view.addSubview(picker)
        NSLayoutConstraint.activate([
            toolbar.topAnchor.constraint(equalTo: view.safeAreaLayoutGuide.topAnchor, constant: 16),
            toolbar.leadingAnchor.constraint(equalTo: view.leadingAnchor),
            toolbar.trailingAnchor.constraint(equalTo: view.trailingAnchor),
            picker.topAnchor.constraint(equalTo: toolbar.bottomAnchor),
            picker.leadingAnchor.constraint(equalTo: view.leadingAnchor),
            picker.trailingAnchor.constraint(equalTo: view.trailingAnchor),
            picker.bottomAnchor.constraint(equalTo: view.safeAreaLayoutGuide.bottomAnchor),
        ])
    }

    @objc private func doneTapped() {
        guard !finished else { return }
        finished = true
        let idx = picker.selectedRow(inComponent: 0)
        dismiss(animated: true) { self.onDone(idx) }
    }
    @objc private func cancelTapped() {
        guard !finished else { return }
        finished = true
        dismiss(animated: true) { self.onCancel() }
    }
    func presentationControllerDidDismiss(_ presentationController: UIPresentationController) {
        guard !finished else { return }
        finished = true
        onCancel()
    }

    func numberOfComponents(in pickerView: UIPickerView) -> Int { 1 }
    func pickerView(_ pickerView: UIPickerView, numberOfRowsInComponent component: Int) -> Int { options.count }
    func pickerView(_ pickerView: UIPickerView, titleForRow row: Int, forComponent component: Int) -> String? {
        row >= 0 && row < options.count ? options[row] : nil
    }
}

/// Photo picker — request/response, permission-less (the system PHPicker). Loads the
/// pick into a temp file and returns its `file://` URL (which the image widget renders).
@MainActor
enum PhotoPlugin {
    static func handle(op: String, input: String) async -> PluginResponse {
        guard op == "pick" else { return PluginResponse(ok: false, output: "unknown op '\(op)'") }
        guard let presenter = topViewController() else {
            return PluginResponse(ok: false, output: "no view controller to present from")
        }
        return await withCheckedContinuation { cont in
            var config = PHPickerConfiguration()
            config.filter = .images
            config.selectionLimit = 1
            let picker = PHPickerViewController(configuration: config)
            let delegate = PhotoPickerDelegate { cont.resume(returning: $0) }
            PhotoPickerDelegate.retained = delegate // PHPicker holds its delegate weakly
            picker.delegate = delegate
            presenter.present(picker, animated: true)
        }
    }
}

private final class PhotoPickerDelegate: NSObject, PHPickerViewControllerDelegate {
    static var retained: PhotoPickerDelegate?
    private let onResult: (PluginResponse) -> Void
    init(onResult: @escaping (PluginResponse) -> Void) { self.onResult = onResult }

    func picker(_ picker: PHPickerViewController, didFinishPicking results: [PHPickerResult]) {
        picker.dismiss(animated: true)
        guard let provider = results.first?.itemProvider else {
            finish(PluginResponse(ok: false, output: "cancelled")); return
        }
        // Copy the pick to our own temp file (the system one is short-lived) and return it.
        provider.loadFileRepresentation(forTypeIdentifier: UTType.image.identifier) { url, error in
            guard let url else {
                self.finish(PluginResponse(ok: false, output: error?.localizedDescription ?? "load failed")); return
            }
            let ext = url.pathExtension.isEmpty ? "jpg" : url.pathExtension
            let dest = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString + "." + ext)
            do {
                try FileManager.default.copyItem(at: url, to: dest)
                self.finish(PluginResponse(ok: true, output: dest.absoluteString))
            } catch {
                self.finish(PluginResponse(ok: false, output: error.localizedDescription))
            }
        }
    }

    private func finish(_ r: PluginResponse) {
        onResult(r)
        PhotoPickerDelegate.retained = nil
    }
}

/// Camera capture — request/response. Launches UIImagePickerController(.camera), writes
/// the shot to a temp file and returns its `file://` URL. Requires NSCameraUsageDescription
/// (project.yml). The simulator has no camera, so it returns ok:false there; on a device it
/// presents the system camera. No separate permission API call — iOS prompts on first use.
@MainActor
enum CameraPlugin {
    static func handle(op: String, input: String) async -> PluginResponse {
        guard op == "capture" else { return PluginResponse(ok: false, output: "unknown op '\(op)'") }
        guard UIImagePickerController.isSourceTypeAvailable(.camera) else {
            return PluginResponse(ok: false, output: "camera not available")
        }
        guard let presenter = topViewController() else {
            return PluginResponse(ok: false, output: "no view controller to present from")
        }
        return await withCheckedContinuation { cont in
            let picker = UIImagePickerController()
            picker.sourceType = .camera
            let delegate = CameraCaptureDelegate { cont.resume(returning: $0) }
            CameraCaptureDelegate.retained = delegate // the picker holds its delegate weakly
            picker.delegate = delegate
            presenter.present(picker, animated: true)
        }
    }
}

private final class CameraCaptureDelegate: NSObject, UIImagePickerControllerDelegate, UINavigationControllerDelegate {
    static var retained: CameraCaptureDelegate?
    private let onResult: (PluginResponse) -> Void
    init(onResult: @escaping (PluginResponse) -> Void) { self.onResult = onResult }

    func imagePickerController(_ picker: UIImagePickerController, didFinishPickingMediaWithInfo info: [UIImagePickerController.InfoKey: Any]) {
        picker.dismiss(animated: true)
        guard let image = info[.originalImage] as? UIImage, let data = image.jpegData(compressionQuality: 0.9) else {
            finish(PluginResponse(ok: false, output: "no image")); return
        }
        let dest = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString + ".jpg")
        do {
            try data.write(to: dest)
            finish(PluginResponse(ok: true, output: dest.absoluteString))
        } catch {
            finish(PluginResponse(ok: false, output: error.localizedDescription))
        }
    }

    func imagePickerControllerDidCancel(_ picker: UIImagePickerController) {
        picker.dismiss(animated: true)
        finish(PluginResponse(ok: false, output: "cancelled"))
    }

    private func finish(_ r: PluginResponse) {
        onResult(r)
        CameraCaptureDelegate.retained = nil
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
