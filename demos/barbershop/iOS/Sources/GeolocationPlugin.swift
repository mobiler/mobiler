import SharedTypes
import CoreLocation

/// Free bundled plugin: current device location. op "get" → "lat,lng", ok=false on denial/error.
/// CLLocationManager drives the when-in-use authorization prompt, then a one-shot requestLocation.
/// Needs NSLocationWhenInUseUsageDescription (added to Info.plist by `plugin add`).
@MainActor
enum GeolocationPlugin {
    static func handle(op: String, input: String) async -> PluginResponse {
        guard op == "get" else { return PluginResponse(ok: false, output: "unknown op '\(op)'") }
        return await withCheckedContinuation { cont in
            let delegate = LocationDelegate { cont.resume(returning: $0) }
            LocationDelegate.retained = delegate // CLLocationManager holds its delegate weakly
            delegate.start()
        }
    }
}

private final class LocationDelegate: NSObject, CLLocationManagerDelegate {
    static var retained: LocationDelegate?
    private let manager = CLLocationManager()
    private let onResult: (PluginResponse) -> Void
    private var done = false
    init(onResult: @escaping (PluginResponse) -> Void) {
        self.onResult = onResult
        super.init()
        manager.delegate = self
    }

    func start() { proceed(manager.authorizationStatus) }

    func locationManagerDidChangeAuthorization(_ m: CLLocationManager) { proceed(m.authorizationStatus) }

    private func proceed(_ status: CLAuthorizationStatus) {
        switch status {
        case .authorizedWhenInUse, .authorizedAlways: manager.requestLocation()
        case .denied, .restricted: finish(PluginResponse(ok: false, output: "denied"))
        default: manager.requestWhenInUseAuthorization() // .notDetermined → wait for the callback
        }
    }

    func locationManager(_ m: CLLocationManager, didUpdateLocations locs: [CLLocation]) {
        guard let c = locs.first?.coordinate else { finish(PluginResponse(ok: false, output: "no location")); return }
        finish(PluginResponse(ok: true, output: "\(c.latitude),\(c.longitude)"))
    }

    func locationManager(_ m: CLLocationManager, didFailWithError error: Error) {
        finish(PluginResponse(ok: false, output: error.localizedDescription))
    }

    private func finish(_ r: PluginResponse) {
        if done { return }
        done = true
        onResult(r)
        LocationDelegate.retained = nil
    }
}
