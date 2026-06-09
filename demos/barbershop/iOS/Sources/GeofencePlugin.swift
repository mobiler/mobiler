import CoreLocation
import Foundation
import SharedTypes
import UserNotifications

// geofence (free, bundled, EXPERIMENTAL). Native background location monitoring — geofence enter/exit
// + significant-location-change. The OS keeps watching while the app is closed; on a trigger this posts
// a local notification AND buffers an event (UserDefaults) that flushes to the cx stream on subscribe —
// so an event that woke a dead process still reaches the core on next launch. Two surfaces:
//   cx.plugin("geofence","add",{"id":"shop","lat":..,"lng":..,"radius":150,"notify_title":..,"notify_body":..})
//   cx.subscribe(key,"geofence","events","",on) → {"type":"geofence","id":"shop","event":"enter"|"exit"}
//                                               / {"type":"location","lat":..,"lng":..}
//
// `GeofencePlugin.bootstrap()` runs at app launch (App.swift `// mobiler:app-launch`) so a background
// relaunch re-creates the CLLocationManager + delegate the OS delivers the queued region event to.
enum GeofencePlugin {
    static func handle(op: String, input: String) async -> PluginResponse {
        switch op {
        case "requestPermission": return await GeofenceMonitor.shared.requestPermission()
        case "add": return await GeofenceMonitor.shared.add(input)
        case "remove": return await GeofenceMonitor.shared.remove(input)
        case "list": return await GeofenceMonitor.shared.list()
        case "startSignificantChanges": return await GeofenceMonitor.shared.startSignificantChanges()
        case "stopSignificantChanges": return await GeofenceMonitor.shared.stopSignificantChanges()
        default: return PluginResponse(ok: false, output: "unknown op '\(op)'")
        }
    }

    // Streaming entrypoint (cx.subscribe): attach an event sink to the monitor and stay parked until
    // the subscription's Task is cancelled (cx.unsubscribe), then detach. Buffered events flush on attach.
    static func subscribe(op: String, input: String, emit: @escaping @Sendable (PluginResponse) -> Void) async {
        let sink: @Sendable (String) -> Void = { emit(PluginResponse(ok: true, output: $0)) }
        await MainActor.run { GeofenceMonitor.shared.attach(sink) }
        await withTaskCancellationHandler {
            while !Task.isCancelled {
                try? await Task.sleep(nanoseconds: 1_000_000_000)
            }
        } onCancel: {
            Task { @MainActor in GeofenceMonitor.shared.detach() }
        }
    }

    // Launch hook — re-arm monitoring so queued background events reach a live delegate.
    static func bootstrap() {
        Task { @MainActor in GeofenceMonitor.shared.bootstrap() }
    }
}

// Owns the single CLLocationManager + delegate for the process lifetime (created at launch via
// bootstrap()). Mirrors PushBridge's buffer-and-flush, but persists the buffer in UserDefaults so an
// event that fired in a since-killed background process is still delivered on the next launch.
@MainActor
final class GeofenceMonitor: NSObject, CLLocationManagerDelegate {
    static let shared = GeofenceMonitor()

    private let manager = CLLocationManager()
    private var sink: (@Sendable (String) -> Void)?
    private let bufferKey = "mobiler.geofence.buffer"      // [String] of pending event JSON
    private let slcKey = "mobiler.geofence.slc"            // Bool: significant-change armed
    private let notifKey = "mobiler.geofence.notif"        // [id: [title, body]]

    override init() {
        super.init()
        manager.delegate = self
        manager.desiredAccuracy = kCLLocationAccuracyHundredMeters
    }

    // --- launch ---

    func bootstrap() {
        // The delegate is set in init; region monitors persist in the OS across launches (don't re-add).
        // Only significant-change needs re-arming after a relaunch.
        if UserDefaults.standard.bool(forKey: slcKey) {
            manager.startMonitoringSignificantLocationChanges()
        }
    }

    // --- ops ---

    func requestPermission() -> PluginResponse {
        manager.requestAlwaysAuthorization()
        let ok = manager.authorizationStatus == .authorizedAlways
        return PluginResponse(ok: ok, output: ok ? "granted" : "requested")
    }

    func add(_ input: String) -> PluginResponse {
        guard let obj = Self.json(input),
              let id = obj["id"] as? String,
              let lat = Self.double(obj["lat"]),
              let lng = Self.double(obj["lng"])
        else { return PluginResponse(ok: false, output: "expected {id,lat,lng,radius?}") }
        let radius = Self.double(obj["radius"]) ?? 100
        let center = CLLocationCoordinate2D(latitude: lat, longitude: lng)
        let maxR = manager.maximumRegionMonitoringDistance
        let region = CLCircularRegion(center: center, radius: min(radius, maxR > 0 ? maxR : radius), identifier: id)
        region.notifyOnEntry = true
        region.notifyOnExit = true

        // Stash the per-region notification text so a background transition can build the alert.
        var notif = (UserDefaults.standard.dictionary(forKey: notifKey) as? [String: [String]]) ?? [:]
        let title = (obj["notify_title"] as? String) ?? "Nearby"
        let body = (obj["notify_body"] as? String) ?? ""
        notif[id] = [title, body]
        UserDefaults.standard.set(notif, forKey: notifKey)

        manager.startMonitoring(for: region)
        return PluginResponse(ok: true, output: "")
    }

    func remove(_ input: String) -> PluginResponse {
        guard let id = (Self.json(input)?["id"] as? String) else {
            return PluginResponse(ok: false, output: "expected {id}")
        }
        for r in manager.monitoredRegions where r.identifier == id {
            manager.stopMonitoring(for: r)
        }
        var notif = (UserDefaults.standard.dictionary(forKey: notifKey) as? [String: [String]]) ?? [:]
        notif.removeValue(forKey: id)
        UserDefaults.standard.set(notif, forKey: notifKey)
        return PluginResponse(ok: true, output: "")
    }

    func list() -> PluginResponse {
        let ids = manager.monitoredRegions.map { "\"\($0.identifier)\"" }.joined(separator: ",")
        return PluginResponse(ok: true, output: "[\(ids)]")
    }

    func startSignificantChanges() -> PluginResponse {
        UserDefaults.standard.set(true, forKey: slcKey)
        manager.startMonitoringSignificantLocationChanges()
        return PluginResponse(ok: true, output: "")
    }

    func stopSignificantChanges() -> PluginResponse {
        UserDefaults.standard.set(false, forKey: slcKey)
        manager.stopMonitoringSignificantLocationChanges()
        return PluginResponse(ok: true, output: "")
    }

    // --- stream sink (buffer-and-flush, like PushBridge but persisted) ---

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

    // --- CLLocationManagerDelegate ---

    nonisolated func locationManager(_ manager: CLLocationManager, didEnterRegion region: CLRegion) {
        Task { @MainActor in self.handleTransition(region.identifier, "enter") }
    }

    nonisolated func locationManager(_ manager: CLLocationManager, didExitRegion region: CLRegion) {
        Task { @MainActor in self.handleTransition(region.identifier, "exit") }
    }

    nonisolated func locationManager(_ manager: CLLocationManager, didUpdateLocations locations: [CLLocation]) {
        guard let loc = locations.last else { return }
        let lat = loc.coordinate.latitude, lng = loc.coordinate.longitude
        Task { @MainActor in self.emit("{\"type\":\"location\",\"lat\":\(lat),\"lng\":\(lng)}") }
    }

    private func handleTransition(_ id: String, _ event: String) {
        emit("{\"type\":\"geofence\",\"id\":\(Self.jsonString(id)),\"event\":\"\(event)\"}")
        postNotification(id: id)
    }

    private func postNotification(id: String) {
        let notif = (UserDefaults.standard.dictionary(forKey: notifKey) as? [String: [String]]) ?? [:]
        guard let pair = notif[id] else { return }
        let content = UNMutableNotificationContent()
        content.title = pair.first ?? "Nearby"
        content.body = pair.count > 1 ? pair[1] : ""
        content.sound = .default
        let request = UNNotificationRequest(identifier: "mobiler.geofence.\(id)", content: content, trigger: nil)
        UNUserNotificationCenter.current().add(request)
    }

    // --- helpers ---

    private static func json(_ input: String) -> [String: Any]? {
        guard let data = input.data(using: .utf8),
              let obj = try? JSONSerialization.jsonObject(with: data) as? [String: Any]
        else { return nil }
        return obj
    }

    private static func double(_ v: Any?) -> Double? {
        if let d = v as? Double { return d }
        if let i = v as? Int { return Double(i) }
        if let s = v as? String { return Double(s) }
        return nil
    }

    // JSON-encode a string (quotes + escapes) without a library: encode `[s]`, strip the brackets.
    private static func jsonString(_ s: String) -> String {
        if let data = try? JSONSerialization.data(withJSONObject: [s]),
           let arr = String(data: data, encoding: .utf8), arr.count >= 2 {
            return String(arr.dropFirst().dropLast())
        }
        return "\"\""
    }
}
