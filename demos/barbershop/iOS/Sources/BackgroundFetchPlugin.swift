import BackgroundTasks
import Foundation
import SharedTypes
import UserNotifications

// background-fetch (free, bundled, EXPERIMENTAL). The OS wakes the app on its own schedule to run a
// brief task that posts a local notification AND buffers a {"type":"fetch","id":..} event delivered to
// the core on next foreground (launch-from-dead, like push/system). The Rust core never runs in the
// background — this is a native opportunistic wake + buffered delivery.
//
// The single task identifier `mobiler.refresh` is registered at launch (App.swift `// mobiler:app-launch`
// → bootstrap()) and listed in BGTaskSchedulerPermittedIdentifiers. BGTaskScheduler.register MUST run
// before didFinishLaunching returns, which a lazy request/response plugin can't do.
private let TASK_ID = "mobiler.refresh"

enum BackgroundFetchPlugin {
    static func handle(op: String, input: String) async -> PluginResponse {
        switch op {
        case "schedule": return await BackgroundFetchBridge.shared.schedule(input)
        case "cancel": return await BackgroundFetchBridge.shared.cancel()
        default: return PluginResponse(ok: false, output: "unknown op '\(op)'")
        }
    }

    // Streaming entrypoint (cx.subscribe): attach a sink; buffered fetch events flush on attach.
    static func subscribe(op: String, input: String, emit: @escaping @Sendable (PluginResponse) -> Void) async {
        let sink: @Sendable (String) -> Void = { emit(PluginResponse(ok: true, output: $0)) }
        await MainActor.run { BackgroundFetchBridge.shared.attach(sink) }
        await withTaskCancellationHandler {
            while !Task.isCancelled {
                try? await Task.sleep(nanoseconds: 1_000_000_000)
            }
        } onCancel: {
            Task { @MainActor in BackgroundFetchBridge.shared.detach() }
        }
    }

    // Launch hook — register the BGTask handler (must happen before didFinishLaunching returns).
    static func bootstrap() {
        BGTaskScheduler.shared.register(forTaskWithIdentifier: TASK_ID, using: nil) { task in
            // BGAppRefreshTask runs ~30s in the background.
            Task { @MainActor in
                BackgroundFetchBridge.shared.handleRefresh()
                BackgroundFetchBridge.shared.submitRequest()  // reschedule the next wake
                task.setTaskCompleted(success: true)
            }
        }
    }
}

// Owns the buffer + scheduling state for the process lifetime. Persists pending events in UserDefaults
// so a wake that ran in a since-killed background process is still delivered on the next launch.
@MainActor
final class BackgroundFetchBridge {
    static let shared = BackgroundFetchBridge()

    private var sink: (@Sendable (String) -> Void)?
    private let bufferKey = "mobiler.bgfetch.buffer"   // [String] of pending event JSON
    private let labelKey = "mobiler.bgfetch.label"     // app's opaque id echoed in events
    private let intervalKey = "mobiler.bgfetch.interval"
    private let titleKey = "mobiler.bgfetch.title"
    private let bodyKey = "mobiler.bgfetch.body"

    func schedule(_ input: String) -> PluginResponse {
        guard let data = input.data(using: .utf8),
              let obj = try? JSONSerialization.jsonObject(with: data) as? [String: Any]
        else { return PluginResponse(ok: false, output: "invalid input JSON") }
        let d = UserDefaults.standard
        d.set((obj["id"] as? String) ?? "refresh", forKey: labelKey)
        let interval = (obj["min_interval_seconds"] as? Double)
            ?? (obj["min_interval_seconds"] as? Int).map(Double.init) ?? 3600
        d.set(interval, forKey: intervalKey)
        d.set((obj["notify_title"] as? String) ?? "", forKey: titleKey)
        d.set((obj["notify_body"] as? String) ?? "", forKey: bodyKey)
        submitRequest()
        return PluginResponse(ok: true, output: "")
    }

    func cancel() -> PluginResponse {
        BGTaskScheduler.shared.cancel(taskRequestWithIdentifier: TASK_ID)
        return PluginResponse(ok: true, output: "")
    }

    func submitRequest() {
        let interval = UserDefaults.standard.double(forKey: intervalKey)
        let request = BGAppRefreshTaskRequest(identifier: TASK_ID)
        request.earliestBeginDate = Date(timeIntervalSinceNow: max(60, interval))
        try? BGTaskScheduler.shared.submit(request)
    }

    // Called when the OS runs the refresh task — notify + buffer an event.
    func handleRefresh() {
        let d = UserDefaults.standard
        let title = d.string(forKey: titleKey) ?? ""
        if !title.isEmpty {
            let content = UNMutableNotificationContent()
            content.title = title
            content.body = d.string(forKey: bodyKey) ?? ""
            content.sound = .default
            let request = UNNotificationRequest(identifier: "mobiler.bgfetch", content: content, trigger: nil)
            UNUserNotificationCenter.current().add(request)
        }
        let label = d.string(forKey: labelKey) ?? "refresh"
        emit("{\"type\":\"fetch\",\"id\":\(Self.jsonString(label))}")
    }

    // --- stream sink (buffer-and-flush) ---

    func attach(_ sink: @escaping @Sendable (String) -> Void) {
        self.sink = sink
        let buf = UserDefaults.standard.stringArray(forKey: bufferKey) ?? []
        for payload in buf { sink(payload) }
        UserDefaults.standard.removeObject(forKey: bufferKey)
    }

    func detach() { sink = nil }

    private func emit(_ payload: String) {
        if let sink {
            sink(payload)
        } else {
            var buf = UserDefaults.standard.stringArray(forKey: bufferKey) ?? []
            buf.append(payload)
            UserDefaults.standard.set(buf, forKey: bufferKey)
        }
    }

    // JSON-encode a string (quotes + escapes): encode `[s]`, strip the brackets.
    private static func jsonString(_ s: String) -> String {
        if let data = try? JSONSerialization.data(withJSONObject: [s]),
           let arr = String(data: data, encoding: .utf8), arr.count >= 2 {
            return String(arr.dropFirst().dropLast())
        }
        return "\"\""
    }
}
