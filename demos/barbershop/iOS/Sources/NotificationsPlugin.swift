import Foundation
import SharedTypes
import UserNotifications

// Local scheduled notifications (paid plugin). The OS holds scheduled notifications, so they fire
// even when the app is closed — no background plumbing needed. Ops (input is JSON):
//   requestPermission: ""                                       → ok = authorized
//   schedule: {"id":1,"title":"...","body":"...","after_seconds":10} → ok:true (fires later)
//   cancel:   {"id":1}                                          → ok:true
// `id` is stamped into the request identifier so cancel can target it.
enum NotificationsPlugin {
    static func handle(op: String, input: String) async -> PluginResponse {
        switch op {
        case "requestPermission": return await requestPermission()
        case "schedule": return await schedule(input)
        case "cancel": return cancel(input)
        default: return PluginResponse(ok: false, output: "unknown op '\(op)'")
        }
    }

    private static func requestPermission() async -> PluginResponse {
        do {
            let granted = try await UNUserNotificationCenter.current()
                .requestAuthorization(options: [.alert, .sound, .badge])
            return PluginResponse(ok: granted, output: granted ? "granted" : "denied")
        } catch {
            return PluginResponse(ok: false, output: error.localizedDescription)
        }
    }

    private static func schedule(_ input: String) async -> PluginResponse {
        guard
            let data = input.data(using: .utf8),
            let obj = try? JSONSerialization.jsonObject(with: data) as? [String: Any]
        else { return PluginResponse(ok: false, output: "invalid input JSON") }

        let id = (obj["id"] as? Int) ?? 1
        let title = (obj["title"] as? String) ?? "Reminder"
        let body = (obj["body"] as? String) ?? ""
        let after = (obj["after_seconds"] as? Double) ?? (obj["after_seconds"] as? Int).map(Double.init) ?? 0

        let content = UNMutableNotificationContent()
        content.title = title
        content.body = body
        content.sound = .default
        // A non-repeating time-interval trigger; minimum 1s.
        let trigger = UNTimeIntervalNotificationTrigger(timeInterval: max(1, after), repeats: false)
        let request = UNNotificationRequest(identifier: "mobiler.\(id)", content: content, trigger: trigger)
        do {
            try await UNUserNotificationCenter.current().add(request)
            return PluginResponse(ok: true, output: "")
        } catch {
            return PluginResponse(ok: false, output: error.localizedDescription)
        }
    }

    private static func cancel(_ input: String) -> PluginResponse {
        let id = (try? JSONSerialization.jsonObject(with: Data(input.utf8)) as? [String: Any])?
            .flatMap { $0["id"] as? Int } ?? 1
        UNUserNotificationCenter.current().removePendingNotificationRequests(withIdentifiers: ["mobiler.\(id)"])
        return PluginResponse(ok: true, output: "")
    }
}
