import SharedTypes
import CoreMotion

/// Free bundled plugin: one-shot motion sensor read (no permission). op "read", input
/// "accelerometer" (default) or "gyroscope" → "x,y,z". Starts updates, takes the first
/// sample, stops.
enum SensorsPlugin {
    static func handle(op: String, input: String) async -> PluginResponse {
        guard op == "read" else { return PluginResponse(ok: false, output: "unknown op '\(op)'") }
        let sensor = input.isEmpty ? "accelerometer" : input
        let mgr = CMMotionManager()
        let queue = OperationQueue()
        return await withCheckedContinuation { cont in
            var done = false
            func finish(_ r: PluginResponse) { if !done { done = true; cont.resume(returning: r) } }
            switch sensor {
            case "accelerometer":
                guard mgr.isAccelerometerAvailable else {
                    finish(PluginResponse(ok: false, output: "accelerometer unavailable")); return
                }
                mgr.accelerometerUpdateInterval = 0.1
                mgr.startAccelerometerUpdates(to: queue) { data, _ in
                    guard let a = data?.acceleration else { return }
                    mgr.stopAccelerometerUpdates()
                    finish(PluginResponse(ok: true, output: "\(a.x),\(a.y),\(a.z)"))
                }
            case "gyroscope":
                guard mgr.isGyroAvailable else {
                    finish(PluginResponse(ok: false, output: "gyroscope unavailable")); return
                }
                mgr.gyroUpdateInterval = 0.1
                mgr.startGyroUpdates(to: queue) { data, _ in
                    guard let r = data?.rotationRate else { return }
                    mgr.stopGyroUpdates()
                    finish(PluginResponse(ok: true, output: "\(r.x),\(r.y),\(r.z)"))
                }
            default:
                finish(PluginResponse(ok: false, output: "unknown sensor '\(sensor)'"))
            }
        }
    }
}
