import Foundation

// The notifications plugin's `schedule` input: the validation rules and failure strings of Android's
// ReminderRegistry.kt `parseSchedule` (byte for byte), the canonical actions JSON, and the wall-clock
// parsing and DST resolution of an `at` time.

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
    static let maxTitleBytes = 256
    static let maxBodyBytes = 2048
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
        // Capped as on Android, where both travel in the snooze button's PendingIntent extra.
        if title.utf8.count > maxTitleBytes { throw invalid("`title` is over \(maxTitleBytes) bytes") }
        if body.utf8.count > maxBodyBytes { throw invalid("`body` is over \(maxBodyBytes) bytes") }

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
