import Foundation
import SharedTypes

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
