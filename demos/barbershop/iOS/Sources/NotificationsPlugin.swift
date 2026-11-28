import Foundation
import SharedTypes
import UIKit
import UserNotifications

// Local scheduled notifications. The OS holds scheduled notifications, so they fire even when the app is
// closed; no background plumbing is needed. Ops (input is JSON; a failure is ok:false "<code>" or
// "<code>: <detail>", codes invalid_input | past | pending_limit | unsupported | failed):
//   requestPermission: ""     → ok = allowed, "granted" | "denied"
//   permissionStatus: ""      → ok:true, "granted" | "denied" | "notDetermined" (never prompts)
//   openSettings: ""          → ok:true when the app's notification settings page opened
//   schedule: {"id":7,"title":"…","body":"…", "at":"YYYY-MM-DD HH:MM" | "after_seconds":N,
//              "payload"?:"…","actions"?:[…],"late_minutes"?:1440}   → ok:true ""
//   cancel:   {"id":7}                                      → ok:true (pending, snooze and tray)
//   cancelAll: ""                                           → ok:true (every reminder of this plugin)
//   listPending: ""     → [{"id":7,"fire_at":"YYYY-MM-DD HH:MM:SS","snoozed":false}] by fire_at
//   removeDelivered: {"ids":[1,2]} or "" for all           → ok:true (this plugin's tray entries)
//
// The input rules and failure strings are the Android plugin's (ReminderRegistry.kt `parseSchedule`),
// byte for byte. Every request has identifier `mobiler.<id>` and userInfo `mobiler_local: <id>` (the
// marker App.swift routes on, LocalNotificationBridge), plus `payload`, the `actions` JSON, `late_minutes`,
// `snoozed`, `fire_at_s` and a per-schedule `mobiler_token`. iOS has no registry: the pending requests are
// the state. `late_minutes` is accepted and stored but has no effect on iOS: the OS delivers missed
// notifications itself after power-on (the documented platform difference).
//
// Stream (cx.subscribe("notifications", "events", "")): {"type":"opened"|"snoozed"|"dismissed",…}, see
// NotificationsEvents.swift. `bootstrap()` (App.swift's `mobiler:app-launch`) attaches the response handler
// before iOS delivers a launch response, so a snooze pressed while the app was dead is handled natively.
enum NotificationsPlugin {
    static func handle(op: String, input: String) async -> PluginResponse {
        let r: NotifReply
        switch op {
        case "requestPermission": r = await requestPermission()
        case "permissionStatus": r = await permissionStatus()
        case "openSettings": r = await openSettings()
        // The ops that read or change the pending set run one at a time (NotificationsLane), so the limit
        // check and the add can't interleave with another op.
        case "schedule": r = await NotificationsLane.shared.run { await NotificationsPlugin.schedule(input) }
        case "cancel": r = await NotificationsLane.shared.run { NotificationsPlugin.cancel(input) }
        case "cancelAll": r = await NotificationsLane.shared.run { await NotificationsPlugin.cancelAll() }
        case "listPending": r = await NotificationsLane.shared.run { await NotificationsPlugin.listPending() }
        case "removeDelivered": r = await NotificationsLane.shared.run { await NotificationsPlugin.removeDelivered(input) }
        default: r = NotifReply(false, "unsupported: unknown op '\(op)'")
        }
        return PluginResponse(ok: r.ok, output: r.output)
    }

    /// The events stream: flushes the persisted buffer in order, then delivers live until unsubscribed
    /// (the subscription's Task is cancelled). A second subscriber replaces the first.
    static func subscribe(op: String, input: String, emit: @escaping @Sendable (PluginResponse) -> Void) async {
        guard op == "events" else {
            emit(PluginResponse(ok: false, output: "unsupported: unknown stream '\(op)'"))
            return
        }
        let sink: @Sendable (String) -> Void = { emit(PluginResponse(ok: true, output: $0)) }
        guard let id = await MainActor.run(body: { NotificationEvents.shared.attach(sink) }) else { return }
        await withTaskCancellationHandler {
            while !Task.isCancelled {
                try? await Task.sleep(nanoseconds: 1_000_000_000)
            }
        } onCancel: {
            Task { @MainActor in NotificationEvents.shared.detach(id) }
        }
    }

    /// Launch hook (App.swift `mobiler:app-launch`, inside didFinishLaunching, on the main actor): attach
    /// the response handler now, before iOS hands over the response that launched the app, then
    /// re-register the categories of the reminders still pending or in the tray.
    @MainActor
    static func bootstrap() {
        LocalNotificationBridge.shared.attach { response, completion in
            NotificationResponses.handle(response, completion: completion)
        }
        Task {
            await NotificationsLane.shared.run { await NotificationsPlugin.registerCategories(pending: nil, adding: []) }
        }
    }

    static let maxPending = 64

    static func requestIdentifier(_ id: Int) -> String { "mobiler.\(id)" }

    // --- permission ---

    private static func requestPermission() async -> NotifReply {
        do {
            let granted = try await UNUserNotificationCenter.current()
                .requestAuthorization(options: [.alert, .sound, .badge])
            return NotifReply(granted, granted ? "granted" : "denied")
        } catch {
            return NotifReply(false, "failed: \(error.localizedDescription)")
        }
    }

    private static func permissionStatus() async -> NotifReply {
        let settings = await UNUserNotificationCenter.current().notificationSettings()
        switch settings.authorizationStatus {
        case .notDetermined: return NotifReply(true, "notDetermined")
        case .denied: return NotifReply(true, "denied")
        case .authorized, .provisional, .ephemeral: return NotifReply(true, "granted")
        @unknown default: return NotifReply(true, "denied")
        }
    }

    // iOS 16+ (the deployment target is 17) opens the app's own notification settings page.
    @MainActor
    private static func openSettings() async -> NotifReply {
        guard let url = URL(string: UIApplication.openNotificationSettingsURLString) else {
            return NotifReply(false, "failed: no settings page could be opened")
        }
        let opened = await UIApplication.shared.open(url)
        return opened ? NotifReply(true, "") : NotifReply(false, "failed: no settings page could be opened")
    }

    // --- ops on the pending set (always inside NotificationsLane) ---

    private static func schedule(_ input: String) async -> NotifReply {
        let now = Date()
        let spec: ReminderSpec
        do {
            spec = try ReminderInput.parseSchedule(input, now: now, zone: TimeZone.current)
        } catch let e as NotifInputError {
            return NotifReply(false, e.message)
        } catch {
            return NotifReply(false, "invalid_input")
        }

        let center = UNUserNotificationCenter.current()
        let identifier = requestIdentifier(spec.id)
        // iOS keeps at most 64 pending requests per app (all of them, not only this plugin's) and silently
        // drops the rest, so refuse a NEW id at the limit. Replacing a pending id never counts as new.
        let pending = await center.pendingNotificationRequests()
        if pending.count >= maxPending && !pending.contains(where: { $0.identifier == identifier }) {
            return NotifReply(false, "pending_limit: 64 pending notifications on iOS")
        }

        let canonical = spec.actionsJson ?? "[]"
        let content = UNMutableNotificationContent()
        content.title = spec.title.isEmpty ? "Reminder" : spec.title
        content.body = spec.body
        content.sound = .default
        content.categoryIdentifier = NotificationCategories.identifier(canonical)
        var info: [AnyHashable: Any] = [
            LocalNotificationBridge.markerKey: spec.id,
            ReminderKeys.token: UUID().uuidString,
            ReminderKeys.lateMinutes: spec.lateMinutes,
            ReminderKeys.snoozed: false,
            ReminderKeys.fireAt: spec.fireAt.timeIntervalSince1970,
        ]
        if let p = spec.payload { info[ReminderKeys.payload] = p }
        if let a = spec.actionsJson { info[ReminderKeys.actions] = a }
        content.userInfo = info

        let trigger: UNNotificationTrigger
        if let comps = spec.calendarComponents {
            trigger = UNCalendarNotificationTrigger(dateMatching: comps, repeats: false)
        } else {
            trigger = UNTimeIntervalNotificationTrigger(timeInterval: spec.interval, repeats: false)
        }

        // setNotificationCategories replaces the whole set: register the union, this reminder's included.
        await registerCategories(pending: pending, adding: [canonical])
        do {
            // A request with the identifier of a pending one replaces it, which also drops a pending snooze.
            try await center.add(UNNotificationRequest(identifier: identifier, content: content, trigger: trigger))
            return NotifReply(true, "")
        } catch {
            return NotifReply(false, "failed: \(error.localizedDescription)")
        }
    }

    private static func cancel(_ input: String) -> NotifReply {
        // A missing id (or empty input) keeps the old default of 1.
        var id = 1
        if !ReminderInput.isBlank(input) {
            guard let o = ReminderInput.jsonObject(input) else {
                return NotifReply(false, "invalid_input: not a JSON object")
            }
            if let v = ReminderInput.present(o, "id") {
                guard let n = ReminderInput.validId(v) else {
                    return NotifReply(false, "invalid_input: `id` must be an integer in 1..=2147483647")
                }
                id = n
            }
        }
        let center = UNUserNotificationCenter.current()
        // The reminder and a pending snooze of it share the identifier.
        center.removePendingNotificationRequests(withIdentifiers: [requestIdentifier(id)])
        center.removeDeliveredNotifications(withIdentifiers: [requestIdentifier(id)])
        return NotifReply(true, "")
    }

    private static func cancelAll() async -> NotifReply {
        let center = UNUserNotificationCenter.current()
        let pending = await center.pendingNotificationRequests().filter { isOurs($0) }.map(\.identifier)
        if !pending.isEmpty { center.removePendingNotificationRequests(withIdentifiers: pending) }
        let delivered = await center.deliveredNotifications().filter { isOurs($0.request) }.map(\.request.identifier)
        if !delivered.isEmpty { center.removeDeliveredNotifications(withIdentifiers: delivered) }
        return NotifReply(true, "")
    }

    private static func listPending() async -> NotifReply {
        var rows: [(at: Date, id: Int, snoozed: Bool)] = []
        let pending = await UNUserNotificationCenter.current().pendingNotificationRequests()
        for r in pending {
            let info = r.content.userInfo
            guard LocalNotificationBridge.isLocal(info), let id = reminderId(info, identifier: r.identifier) else { continue }
            let stored = (info[ReminderKeys.fireAt] as? Double).map { Date(timeIntervalSince1970: $0) }
            let next: Date?
            if let t = r.trigger as? UNCalendarNotificationTrigger {
                // Follows a time-zone change, as the trigger itself does.
                next = t.nextTriggerDate() ?? stored
            } else if let t = r.trigger as? UNTimeIntervalNotificationTrigger {
                // A non-repeating interval trigger's nextTriggerDate() counts from NOW, not from when it was
                // added: the stored instant is the real one.
                next = stored ?? t.nextTriggerDate()
            } else {
                next = stored
            }
            guard let at = next else { continue }
            rows.append((at, id, (info[ReminderKeys.snoozed] as? Bool) ?? false))
        }
        rows.sort { $0.at != $1.at ? $0.at < $1.at : $0.id < $1.id }
        let items = rows.map {
            "{\"id\":\($0.id),\"fire_at\":\(NotifJSON.string(NotifJSON.localTime($0.at))),\"snoozed\":\($0.snoozed)}"
        }
        return NotifReply(true, "[" + items.joined(separator: ",") + "]")
    }

    private static func removeDelivered(_ input: String) async -> NotifReply {
        let center = UNUserNotificationCenter.current()
        if ReminderInput.isBlank(input) {
            let delivered = await center.deliveredNotifications().filter { isOurs($0.request) }.map(\.request.identifier)
            if !delivered.isEmpty { center.removeDeliveredNotifications(withIdentifiers: delivered) }
            return NotifReply(true, "")
        }
        guard let o = ReminderInput.jsonObject(input), let arr = o["ids"] as? [Any] else {
            return NotifReply(false, "invalid_input: expected {\"ids\":[…]} or \"\"")
        }
        var identifiers: [String] = []
        for v in arr {
            guard let id = ReminderInput.validId(v) else {
                return NotifReply(false, "invalid_input: ids must be integers in 1..=2147483647")
            }
            identifiers.append(requestIdentifier(id))
        }
        // `mobiler.<id>` with an integer id is this plugin's alone (geofence uses `mobiler.geofence.<id>`).
        if !identifiers.isEmpty { center.removeDeliveredNotifications(withIdentifiers: identifiers) }
        return NotifReply(true, "")
    }

    // --- helpers ---

    /// A reminder of this plugin: marked, or a pre-marker one (identifier `mobiler.<digits>`).
    static func isOurs(_ r: UNNotificationRequest) -> Bool {
        if LocalNotificationBridge.isLocal(r.content.userInfo) { return true }
        return idFromIdentifier(r.identifier) != nil
    }

    static func reminderId(_ info: [AnyHashable: Any], identifier: String) -> Int? {
        if let n = info[LocalNotificationBridge.markerKey] as? NSNumber, n.intValue >= 1 { return n.intValue }
        return idFromIdentifier(identifier)
    }

    private static func idFromIdentifier(_ identifier: String) -> Int? {
        guard identifier.hasPrefix("mobiler.") else { return nil }
        let rest = identifier.dropFirst("mobiler.".count)
        guard !rest.isEmpty, rest.utf8.allSatisfy({ $0 >= 48 && $0 <= 57 }), let id = Int(rest), id >= 1 else { return nil }
        return id
    }
}

/// An op's answer. Sendable (PluginResponse is generated and not), so it can cross the lane's tasks.
struct NotifReply: Sendable {
    let ok: Bool
    let output: String
    init(_ ok: Bool, _ output: String) {
        self.ok = ok
        self.output = output
    }
}

/// The userInfo keys of a reminder request (the marker is LocalNotificationBridge.markerKey).
enum ReminderKeys {
    static let payload = "payload"
    static let actions = "actions"
    static let lateMinutes = "late_minutes"
    static let snoozed = "snoozed"
    /// Epoch seconds the reminder is due (Double). listPending reads it for interval triggers.
    static let fireAt = "fire_at_s"
    /// A fresh UUID per schedule, kept by a snooze: tells a re-scheduled id from the one being snoozed.
    static let token = "mobiler_token"
}

/// Runs the plugin's pending-set work one call at a time, in the order the calls reach it. The chain is
/// built on the main actor, before the first await, so each call waits for the one started before it.
/// This gives mutual exclusion (a limit check and its add can't interleave with a cancelAll); the ORDER in
/// which the core's calls reach the plugin is the shell's (`Plugins.inOrder` in Core.swift, ADR-0065).
@MainActor
final class NotificationsLane {
    static let shared = NotificationsLane()
    private var tail: Task<Void, Never>?

    func run<T: Sendable>(_ body: @escaping @Sendable () async -> T) async -> T {
        let previous = tail
        let task = Task<T, Never> {
            await previous?.value
            return await body()
        }
        tail = Task { _ = await task.value }
        return await task.value
    }
}
