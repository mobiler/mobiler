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

// MARK: - `schedule` input (the Android parseSchedule rules and messages)

struct NotifInputError: Error {
    let message: String
}

struct ReminderAction: Sendable {
    let id: String
    let label: String
    let kind: String          // "open" | "snooze"
    let snoozeMinutes: Int?   // snooze only, 1...1440
}

struct ReminderSpec {
    let id: Int
    let title: String
    let body: String
    /// When it is due (an `at`'s resolved instant, or now + the interval).
    let fireAt: Date
    /// `at`: the resolved local components for UNCalendarNotificationTrigger. nil for `after_seconds`.
    let calendarComponents: DateComponents?
    /// `after_seconds`: the interval, at least 1 s (as before).
    let interval: TimeInterval
    let payload: String?
    /// The validated actions as canonical JSON, or nil for none.
    let actionsJson: String?
    let lateMinutes: Int
}

enum ReminderInput {
    static let maxActions = 3
    static let maxActionIdBytes = 64
    static let maxPayloadBytes = 1024
    static let maxLateMinutes: Int64 = 10080
    static let defaultLateMinutes = 1440

    private static func invalid(_ detail: String) -> NotifInputError {
        NotifInputError(message: "invalid_input: \(detail)")
    }

    static func parseSchedule(_ input: String, now: Date, zone: TimeZone) throws -> ReminderSpec {
        guard let o = jsonObject(input) else { throw invalid("not a JSON object") }

        // A missing id keeps the old default of 1; only a present but bad one is rejected.
        let id = try intField(o, "id", 1, Int64(Int32.max)).map { Int($0) } ?? 1
        let title = present(o, "title").map { text($0) } ?? ""
        let body = present(o, "body").map { text($0) } ?? ""

        let atValue = present(o, "at")
        let afterValue = present(o, "after_seconds")
        if (atValue != nil) == (afterValue != nil) { throw invalid("give exactly one of `at` and `after_seconds`") }

        let nowMs = Int64((now.timeIntervalSince1970 * 1000).rounded(.down))
        let fireAt: Date
        var components: DateComponents? = nil
        var interval: TimeInterval = 1
        if let at = atValue {
            guard let raw = at as? String else { throw invalid("`at` must be a \"YYYY-MM-DD HH:MM\" string") }
            guard let wall = WallTime.parse(raw) else {
                throw invalid("`at` must be \"YYYY-MM-DD HH:MM\" or \"YYYY-MM-DD HH:MM:SS\"")
            }
            fireAt = Date(timeIntervalSince1970: TimeInterval(wall.epochSeconds(in: zone)))
            components = WallTime.triggerComponents(fireAt, zone: zone)
        } else {
            // Exactly the old behaviour: a number (or numeric string) of seconds, truncated; ≤ 0 fires now
            // (iOS: after 1 s, its minimum).
            guard let v = afterValue, let secs = afterSeconds(v) else { throw invalid("`after_seconds` must be a number") }
            if secs > (Int64.max - nowMs) / 1000 { throw invalid("`after_seconds` is too large") }
            interval = TimeInterval(max(1, secs))
            fireAt = now.addingTimeInterval(interval)
        }

        var payload: String? = nil
        if let p = present(o, "payload") {
            guard let s = p as? String else { throw invalid("`payload` must be a string") }
            if s.utf8.count > maxPayloadBytes { throw invalid("`payload` is over \(maxPayloadBytes) bytes") }
            payload = s
        }

        var actionsJson: String? = nil
        if let a = present(o, "actions") { actionsJson = try parseActions(a) }
        let lateMinutes = try intField(o, "late_minutes", 0, maxLateMinutes).map { Int($0) } ?? defaultLateMinutes

        if atValue != nil {
            let fireMs = Int64(fireAt.timeIntervalSince1970) * 1000
            if fireMs <= nowMs { throw NotifInputError(message: "past") }
        }

        return ReminderSpec(id: id, title: title, body: body, fireAt: fireAt, calendarComponents: components,
                            interval: interval, payload: payload, actionsJson: actionsJson, lateMinutes: lateMinutes)
    }

    /// The validated actions as canonical JSON (fixed key order), or nil for an empty array.
    private static func parseActions(_ v: Any) throws -> String? {
        guard let arr = v as? [Any] else { throw invalid("`actions` must be an array") }
        if arr.count > maxActions { throw invalid("at most \(maxActions) actions") }
        if arr.isEmpty { return nil }
        var out: [ReminderAction] = []
        // Compared as bytes (UTF-16 on Android): Swift String equality would merge canonically equal ids.
        var ids = Set<Data>()
        for (i, item) in arr.enumerated() {
            guard let a = item as? [String: Any] else { throw invalid("action \(i) must be an object") }
            guard let aid = a["id"] as? String else { throw invalid("action \(i) needs a string `id`") }
            if aid.isEmpty || aid.utf8.count > maxActionIdBytes {
                throw invalid("action \(i) `id` must be 1..=\(maxActionIdBytes) bytes")
            }
            if !ids.insert(Data(aid.utf8)).inserted { throw invalid("action id `\(aid)` is repeated") }
            guard let label = a["label"] as? String else { throw invalid("action \(i) needs a string `label`") }
            if label.isEmpty { throw invalid("action \(i) `label` is empty") }
            switch (a["kind"] as? String) ?? "" {
            case "open":
                out.append(ReminderAction(id: aid, label: label, kind: "open", snoozeMinutes: nil))
            case "snooze":
                guard let mins = try intField(a, "snooze_minutes", 1, 1440) else {
                    throw invalid("snooze action \(i) needs `snooze_minutes` in 1..=1440")
                }
                out.append(ReminderAction(id: aid, label: label, kind: "snooze", snoozeMinutes: Int(mins)))
            default:
                throw invalid("action \(i) `kind` must be \"open\" or \"snooze\"")
            }
        }
        return canonicalJson(out)
    }

    static func canonicalJson(_ actions: [ReminderAction]) -> String {
        let items = actions.map { a -> String in
            var s = "{\"id\":\(NotifJSON.string(a.id)),\"label\":\(NotifJSON.string(a.label)),\"kind\":\(NotifJSON.string(a.kind))"
            if let m = a.snoozeMinutes { s += ",\"snooze_minutes\":\(m)" }
            return s + "}"
        }
        return "[" + items.joined(separator: ",") + "]"
    }

    /// The actions stored in a request (canonical JSON written by `schedule`); nil when unreadable.
    static func decodeActions(_ json: String) -> [ReminderAction]? {
        guard let arr = (try? JSONSerialization.jsonObject(with: Data(json.utf8))) as? [Any] else { return nil }
        var out: [ReminderAction] = []
        for item in arr {
            guard let a = item as? [String: Any], let id = a["id"] as? String, let label = a["label"] as? String,
                  let kind = a["kind"] as? String else { return nil }
            let mins = (a["snooze_minutes"] as? NSNumber)?.intValue
            out.append(ReminderAction(id: id, label: label, kind: kind, snoozeMinutes: mins))
        }
        return out
    }

    /// An integer field in lo...hi, or nil when absent / null. A fraction or a non-number is rejected.
    private static func intField(_ o: [String: Any], _ key: String, _ lo: Int64, _ hi: Int64) throws -> Int64? {
        guard let v = present(o, key) else { return nil }
        guard let n = number(v) else { throw invalid("`\(key)` must be an integer") }
        let d = n.doubleValue
        if d.isNaN || d.isInfinite || d != d.rounded(.down) { throw invalid("`\(key)` must be an integer") }
        if d < Double(lo) || d > Double(hi) { throw invalid("`\(key)` must be in \(lo)..=\(hi)") }
        return n.int64Value
    }

    /// `after_seconds` as Android reads it: a number truncated, or a numeric string (integer, else decimal).
    private static func afterSeconds(_ v: Any) -> Int64? {
        if let n = number(v) {
            let type = String(cString: n.objCType)
            return (type == "d" || type == "f") ? saturatingInt64(n.doubleValue) : n.int64Value
        }
        if let s = v as? String {
            let t = s.trimmingCharacters(in: .whitespacesAndNewlines)
            if let l = Int64(t) { return l }
            if let d = Double(t) { return saturatingInt64(d) }
        }
        return nil
    }

    /// Kotlin's Double.toLong(): truncates, NaN → 0, saturates at the Int64 bounds (Int64(d) would trap).
    private static func saturatingInt64(_ d: Double) -> Int64 {
        if d.isNaN { return 0 }
        if d >= Double(Int64.max) { return Int64.max }
        if d <= Double(Int64.min) { return Int64.min }
        return Int64(d)
    }

    /// An id as `cancel` and `removeDelivered` accept it: an integral number in 1...Int32.max.
    static func validId(_ v: Any) -> Int? {
        guard let n = number(v) else { return nil }
        let d = n.doubleValue
        guard d == d.rounded(.down), d >= 1, d <= Double(Int32.max) else { return nil }
        return Int(d)
    }

    /// A JSON number (JSONSerialization yields NSNumber for booleans too; those are not numbers).
    static func number(_ v: Any) -> NSNumber? {
        guard let n = v as? NSNumber, CFGetTypeID(n as CFTypeRef) != CFBooleanGetTypeID() else { return nil }
        return n
    }

    /// org.json's optString: a string as is, any other non-null value as its JSON text.
    private static func text(_ v: Any) -> String {
        if let s = v as? String { return s }
        if let n = v as? NSNumber {
            if CFGetTypeID(n as CFTypeRef) == CFBooleanGetTypeID() { return n.boolValue ? "true" : "false" }
            return n.stringValue
        }
        if JSONSerialization.isValidJSONObject(v), let d = try? JSONSerialization.data(withJSONObject: v),
           let s = String(data: d, encoding: .utf8) { return s }
        return ""
    }

    /// The value for `key`, or nil when it is absent or JSON null (org.json's isNull).
    static func present(_ o: [String: Any], _ key: String) -> Any? {
        guard let v = o[key], !(v is NSNull) else { return nil }
        return v
    }

    static func jsonObject(_ s: String) -> [String: Any]? {
        guard let v = try? JSONSerialization.jsonObject(with: Data(s.utf8), options: [.fragmentsAllowed]) else { return nil }
        return v as? [String: Any]
    }

    static func isBlank(_ s: String) -> Bool {
        s.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty
    }
}

// MARK: - wall-clock time

/// A local "YYYY-MM-DD HH:MM[:SS]" time, parsed strictly (java.time STRICT: real dates, 00-23 hours).
struct WallTime {
    let year: Int, month: Int, day: Int, hour: Int, minute: Int, second: Int

    static func parse(_ raw: String) -> WallTime? {
        let t = Array(raw.trimmingCharacters(in: .whitespacesAndNewlines).utf8)
        guard t.count == 16 || t.count == 19 else { return nil }
        guard t[4] == 45, t[7] == 45, t[10] == 32, t[13] == 58 else { return nil }  // - - ' ' :
        if t.count == 19 && t[16] != 58 { return nil }
        func digits(_ from: Int, _ len: Int) -> Int? {
            var v = 0
            for i in from..<(from + len) {
                let c = t[i]
                guard c >= 48 && c <= 57 else { return nil }
                v = v * 10 + Int(c - 48)
            }
            return v
        }
        guard let y = digits(0, 4), let mo = digits(5, 2), let d = digits(8, 2),
              let h = digits(11, 2), let mi = digits(14, 2) else { return nil }
        let s: Int
        if t.count == 19 {
            guard let ss = digits(17, 2) else { return nil }
            s = ss
        } else {
            s = 0
        }
        guard (1...12).contains(mo), d >= 1, d <= daysIn(y, mo), h <= 23, mi <= 59, s <= 59 else { return nil }
        return WallTime(year: y, month: mo, day: d, hour: h, minute: mi, second: s)
    }

    private static func daysIn(_ y: Int, _ m: Int) -> Int {
        switch m {
        case 2: return (y % 4 == 0 && y % 100 != 0) || y % 400 == 0 ? 29 : 28
        case 4, 6, 9, 11: return 30
        default: return 31
        }
    }

    /// Days from 1970-01-01 to this date, proleptic Gregorian (H. Hinnant's days_from_civil).
    private var daysSinceEpoch: Int64 {
        let y = Int64(month <= 2 ? year - 1 : year)
        let era = (y >= 0 ? y : y - 399) / 400
        let yoe = y - era * 400
        let mp = Int64((month + 9) % 12)
        let doy = (153 * mp + 2) / 5 + Int64(day) - 1
        let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy
        return era * 146097 + doe - 719468
    }

    /// The instant this wall time names in `zone`, computed here rather than left to iOS's matching:
    /// - normal: the one instant whose local time it is;
    /// - fall-back overlap (it happens twice): the EARLIER instant;
    /// - spring-forward gap (it never happens): the transition itself, the first instant after the gap
    ///   (Europe/Belgrade 2027-03-28 02:30 → 03:00), as Android's resolveWall does.
    /// `local` is the wall time read as if it were UTC. An instant t names it when t + offset(t) == local.
    /// The only offsets in play are the ones a day before and a day after, so the candidates are
    /// local - offsetBefore and local - offsetAfter: keep the valid ones, earliest first. When neither is
    /// valid it is a gap, and the transition lies between the two candidates: a binary search over whole
    /// seconds finds the first second with the later offset (independent of how Foundation reports
    /// transitions).
    func epochSeconds(in zone: TimeZone) -> Int64 {
        let local = daysSinceEpoch * 86400 + Int64(hour * 3600 + minute * 60 + second)
        func offset(_ t: Int64) -> Int64 {
            Int64(zone.secondsFromGMT(for: Date(timeIntervalSince1970: TimeInterval(t))))
        }
        let a = local - offset(local - 86400)
        let b = local - offset(local + 86400)
        let valid = [a, b].filter { $0 + offset($0) == local }
        if let earliest = valid.min() { return earliest }
        var lo = min(a, b), hi = max(a, b)
        let offLo = offset(lo)
        if offset(hi) == offLo { return hi }
        while hi - lo > 1 {
            let mid = lo + (hi - lo) / 2
            if offset(mid) == offLo { lo = mid } else { hi = mid }
        }
        return hi
    }

    /// The trigger's components: the RESOLVED instant's local date and time in the user's calendar, with no
    /// time zone, so the trigger follows the device zone. Handing iOS the resolved time (03:00, not the gap's
    /// 02:30) means its own gap handling never applies; in an overlap iOS's calendar matching picks the
    /// first occurrence, the earlier instant. The user's calendar may not be Gregorian (Buddhist,
    /// Japanese), so the year (and era) are that calendar's, which is what the trigger matches against.
    static func triggerComponents(_ date: Date, zone: TimeZone) -> DateComponents {
        var cal = Calendar.current
        cal.timeZone = zone
        var units: Set<Calendar.Component> = [.year, .month, .day, .hour, .minute, .second]
        if cal.identifier != .gregorian { units.insert(.era) }
        return cal.dateComponents(units, from: date)
    }
}
