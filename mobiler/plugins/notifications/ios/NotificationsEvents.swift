import Foundation
import SharedTypes
import UserNotifications

// The notifications plugin's `events` stream (spec §4.5), the handler for the responses App.swift routes
// to LocalNotificationBridge (taps, action buttons, dismissals), the native snooze, and the categories.
//   {"type":"opened"|"action"|"snoozed"|"dismissed","id":7,"action_id"?:"…","payload"?:"…","at":"YYYY-MM-DD HH:MM:SS"}
// Every event is first written to a persisted, ordered buffer (UserDefaults "mobiler.notifications.events",
// at most 100, oldest dropped), then flushed in order into the attached sink; each event is removed only
// after the sink returned, so a crash mid-flush re-delivers rather than loses (at-least-once into the
// shell's stream, as geofence, ADR-0036). The same contract and JSON as Android's ReminderEvents.kt.

/// The persisted buffer and the one sink. A second attach replaces the first (ADR-0006); a detach by a
/// replaced subscriber is ignored (ADR-0034).
@MainActor
final class NotificationEvents {
    static let shared = NotificationEvents()
    static let maxEvents = 100

    private let bufferKey = "mobiler.notifications.events"
    private var sink: (@Sendable (String) -> Void)?
    private var sinkId = 0

    /// Buffers `event`, then delivers the buffer if a sink is attached.
    func emit(_ event: String) {
        var list = load()
        list.append(event)
        if list.count > Self.maxEvents { list.removeFirst(list.count - Self.maxEvents) }
        save(list)
        flush()
    }

    /// Attaches the stream's sink and flushes the buffer into it in order. Returns the token for `detach`,
    /// or nil when the subscribing task was already cancelled (a late attach must not replace a newer
    /// subscription's sink).
    func attach(_ sink: @escaping @Sendable (String) -> Void) -> Int? {
        guard !Task.isCancelled else { return nil }
        sinkId += 1
        self.sink = sink
        flush()
        return sinkId
    }

    /// Clears the sink only if `id` is still the attached one: a stale unsubscribe can't silence the live one.
    func detach(_ id: Int) {
        if id == sinkId { sink = nil }
    }

    private func flush() {
        guard let sink else { return }
        var list = load()
        while let first = list.first {
            sink(first)
            list.removeFirst()
            save(list)
        }
    }

    private func load() -> [String] {
        UserDefaults.standard.stringArray(forKey: bufferKey) ?? []
    }

    private func save(_ list: [String]) {
        if list.isEmpty {
            UserDefaults.standard.removeObject(forKey: bufferKey)
        } else {
            UserDefaults.standard.set(list, forKey: bufferKey)
        }
    }
}

/// What the handler needs from a response, copied out on the main actor (Sendable, unlike the response).
struct ReminderResponse: Sendable {
    let actionIdentifier: String
    let id: Int
    let title: String
    let body: String
    let categoryIdentifier: String
    let payload: String?
    let actionsJson: String?
    let lateMinutes: Int
    let token: String?

    init?(_ response: UNNotificationResponse) {
        let request = response.notification.request
        let info = request.content.userInfo
        guard let id = NotificationsPlugin.reminderId(info, identifier: request.identifier) else { return nil }
        self.actionIdentifier = response.actionIdentifier
        self.id = id
        self.title = request.content.title
        self.body = request.content.body
        self.categoryIdentifier = request.content.categoryIdentifier
        self.payload = info[ReminderKeys.payload] as? String
        self.actionsJson = info[ReminderKeys.actions] as? String
        self.lateMinutes = (info[ReminderKeys.lateMinutes] as? NSNumber)?.intValue ?? ReminderInput.defaultLateMinutes
        self.token = info[ReminderKeys.token] as? String
    }
}

/// iOS's completion handler, carried into the handling Task (it may be called from any thread).
final class NotifCompletion: @unchecked Sendable {
    private let body: () -> Void
    init(_ body: @escaping () -> Void) { self.body = body }
    func call() { body() }
}

/// The handler `bootstrap()` attaches to LocalNotificationBridge. Each response becomes at most one event,
/// buffered (persisted) before the completion handler is called.
enum NotificationResponses {
    @MainActor
    static func handle(_ response: UNNotificationResponse, completion: @escaping () -> Void) {
        guard let r = ReminderResponse(response) else {
            completion()
            return
        }
        let done = NotifCompletion(completion)
        Task { @MainActor in
            if let event = await NotificationsPlugin.event(for: r) {
                NotificationEvents.shared.emit(event)
            }
            done.call()
        }
    }
}

extension NotificationsPlugin {
    /// The event for one response, after doing its native work (a snooze), or nil for none.
    /// - a body tap → `opened`; an `open` button → `opened` with its `action_id`;
    /// - a `snooze` button → re-post in `snooze_minutes`, then `snoozed` with its `action_id`;
    /// - a dismissal (swipe away, clear; the category's .customDismissAction) → `dismissed`.
    static func event(for r: ReminderResponse) async -> String? {
        switch r.actionIdentifier {
        case UNNotificationDefaultActionIdentifier:
            return NotifJSON.event("opened", id: r.id, actionId: nil, payload: r.payload)
        case UNNotificationDismissActionIdentifier:
            return NotifJSON.event("dismissed", id: r.id, actionId: nil, payload: r.payload)
        default:
            let actions = r.actionsJson.flatMap { ReminderInput.decodeActions($0) } ?? []
            // Compared as bytes, as the ids were validated.
            guard let action = actions.first(where: { $0.id.utf8.elementsEqual(r.actionIdentifier.utf8) }) else {
                return nil
            }
            if action.kind == "snooze" {
                let minutes = action.snoozeMinutes ?? 10
                let snoozed = await NotificationsLane.shared.run { await NotificationsPlugin.snooze(r, minutes: minutes) }
                return snoozed ? NotifJSON.event("snoozed", id: r.id, actionId: action.id, payload: r.payload) : nil
            }
            return NotifJSON.event("opened", id: r.id, actionId: action.id, payload: r.payload)
        }
    }

    /// Re-posts the reminder `minutes` from now under the same identifier, marked `snoozed`, keeping its
    /// token. Its tray entry is removed either way. When the id was re-scheduled since (a pending request
    /// `mobiler.<id>` with another token), the new plan wins: nothing is posted and no event follows (false),
    /// as on Android. Runs inside NotificationsLane.
    static func snooze(_ r: ReminderResponse, minutes: Int) async -> Bool {
        let center = UNUserNotificationCenter.current()
        let identifier = requestIdentifier(r.id)
        center.removeDeliveredNotifications(withIdentifiers: [identifier])
        let pending = await center.pendingNotificationRequests()
        if let current = pending.first(where: { $0.identifier == identifier }),
           (current.content.userInfo[ReminderKeys.token] as? String) != r.token {
            return false
        }
        let interval = TimeInterval(max(1, minutes) * 60)
        let content = UNMutableNotificationContent()
        content.title = r.title
        content.body = r.body
        content.sound = .default
        content.categoryIdentifier = r.categoryIdentifier
        var info: [AnyHashable: Any] = [
            LocalNotificationBridge.markerKey: r.id,
            ReminderKeys.lateMinutes: r.lateMinutes,
            ReminderKeys.snoozed: true,
            ReminderKeys.fireAt: Date().addingTimeInterval(interval).timeIntervalSince1970,
        ]
        if let t = r.token { info[ReminderKeys.token] = t }
        if let p = r.payload { info[ReminderKeys.payload] = p }
        if let a = r.actionsJson { info[ReminderKeys.actions] = a }
        content.userInfo = info
        // A snooze never counts against the pending limit (spec §4.2).
        let trigger = UNTimeIntervalNotificationTrigger(timeInterval: interval, repeats: false)
        do {
            try await center.add(UNNotificationRequest(identifier: identifier, content: content, trigger: trigger))
            return true
        } catch {
            return false
        }
    }

    /// Registers the union of the categories iOS already has (the app's own, other plugins', earlier
    /// launches'), the ones this plugin registered in this process, those of every reminder still pending
    /// or in the tray (from its stored actions JSON), and `extra` (canonical actions JSON). It must be the
    /// union: setNotificationCategories REPLACES the whole set. Runs inside NotificationsLane.
    static func registerCategories(pending given: [UNNotificationRequest]?, adding extra: [String]) async {
        let center = UNUserNotificationCenter.current()
        let pending: [UNNotificationRequest]
        if let given {
            pending = given
        } else {
            pending = await center.pendingNotificationRequests()
        }
        let delivered = await center.deliveredNotifications()
        let existing = await center.notificationCategories()

        var canonicals = extra
        for request in pending + delivered.map(\.request) where LocalNotificationBridge.isLocal(request.content.userInfo) {
            canonicals.append((request.content.userInfo[ReminderKeys.actions] as? String) ?? "[]")
        }
        var byId: [String: UNNotificationCategory] = [:]
        for c in existing { byId[c.identifier] = c }
        for (id, c) in NotificationCategories.registered { byId[id] = c }
        for canonical in canonicals {
            guard let c = NotificationCategories.make(canonical) else { continue }
            byId[c.identifier] = c
            NotificationCategories.registered[c.identifier] = c
        }
        center.setNotificationCategories(Set(byId.values))
    }
}

/// One category per distinct action set, the empty set included, so every reminder reports `dismissed`
/// (.customDismissAction). Identifier `mobiler.c.<fnv1a64 of the canonical actions JSON>`. A snooze button
/// has no .foreground (iOS handles it in the background, the app stays where it is); `open` uses .foreground.
enum NotificationCategories {
    /// Registered by this process (only touched inside NotificationsLane), so a category set a moment ago is
    /// kept even if iOS's own list doesn't show it yet.
    static var registered: [String: UNNotificationCategory] = [:]

    static func identifier(_ canonical: String) -> String {
        "mobiler.c." + fnv1a64Hex(canonical)
    }

    static func make(_ canonical: String) -> UNNotificationCategory? {
        guard let actions = ReminderInput.decodeActions(canonical) else { return nil }
        let buttons = actions.map {
            UNNotificationAction(identifier: $0.id, title: $0.label, options: $0.kind == "open" ? [.foreground] : [])
        }
        return UNNotificationCategory(identifier: identifier(canonical), actions: buttons,
                                      intentIdentifiers: [], options: [.customDismissAction])
    }

    /// FNV-1a 64 over the UTF-8 bytes, as 16 lower-case hex digits.
    static func fnv1a64Hex(_ s: String) -> String {
        var h: UInt64 = 0xcbf2_9ce4_8422_2325
        for b in s.utf8 {
            h ^= UInt64(b)
            h = h &* 0x0000_0100_0000_01b3
        }
        let hex = String(h, radix: 16)
        return String(repeating: "0", count: 16 - hex.count) + hex
    }
}

enum NotifJSON {
    /// A JSON string literal (quotes and escapes) via JSONSerialization, never by concatenation: encode
    /// `[s]` and strip the brackets (as bytes, so a leading combining mark can't merge with the quote).
    static func string(_ s: String) -> String {
        guard let data = try? JSONSerialization.data(withJSONObject: [s], options: [.withoutEscapingSlashes]),
              data.count >= 2 else { return "\"\"" }
        return String(decoding: data.dropFirst().dropLast(), as: UTF8.self)
    }

    /// Device-local "YYYY-MM-DD HH:MM:SS" (Gregorian, en_US_POSIX), as Android's reminderLocalTime.
    static func localTime(_ date: Date) -> String {
        let f = DateFormatter()
        f.locale = Locale(identifier: "en_US_POSIX")
        f.calendar = Calendar(identifier: .gregorian)
        f.timeZone = TimeZone.current
        f.dateFormat = "yyyy-MM-dd HH:mm:ss"
        return f.string(from: date)
    }

    /// The event JSON of spec §4.5, key order as Android's: `action_id` and `payload` are omitted when nil;
    /// `at` is when it happened, local time.
    static func event(_ type: String, id: Int, actionId: String?, payload: String?) -> String {
        var s = "{\"type\":\(string(type)),\"id\":\(id)"
        if let a = actionId { s += ",\"action_id\":\(string(a))" }
        if let p = payload { s += ",\"payload\":\(string(p))" }
        return s + ",\"at\":\(string(localTime(Date())))}"
    }
}
