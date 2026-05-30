import UIKit

// Reports the battery level as a percentage string ("0".."100"). `PluginResponse` is the
// iOS shell's ABI type. Installed by `mobiler plugin add battery`.
@MainActor
enum BatteryPlugin {
    static func handle(op: String, input: String) async -> PluginResponse {
        guard op == "level" else { return PluginResponse(ok: false, output: "unknown op '\(op)'") }
        UIDevice.current.isBatteryMonitoringEnabled = true
        let level = UIDevice.current.batteryLevel   // 0.0–1.0, or < 0 if unknown
        guard level >= 0 else { return PluginResponse(ok: false, output: "battery level unavailable") }
        return PluginResponse(ok: true, output: String(Int(level * 100)))
    }
}
