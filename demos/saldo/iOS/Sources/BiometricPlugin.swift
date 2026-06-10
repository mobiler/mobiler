import LocalAuthentication
import SharedTypes

// Biometric authentication (free, bundled). Presents Face ID / Touch ID via LocalAuthentication
// (system framework — no package). `input` is the reason string shown in the prompt (optional).
// Returns ok:true on success; ok:false with a reason on cancel / failure / unavailable.
// Requires NSFaceIDUsageDescription (added by the plugin manifest) for Face ID.
enum BiometricPlugin {
    static func handle(op: String, input: String) async -> PluginResponse {
        guard op == "authenticate" else { return PluginResponse(ok: false, output: "unknown op '\(op)'") }
        let context = LAContext()
        // Allow device passcode fallback, mirroring Android's DEVICE_CREDENTIAL.
        let policy: LAPolicy = .deviceOwnerAuthentication
        var error: NSError?
        guard context.canEvaluatePolicy(policy, error: &error) else {
            return PluginResponse(ok: false, output: error?.localizedDescription ?? "biometric unavailable")
        }
        let reason = input.isEmpty ? "Authenticate" : input
        do {
            let ok = try await context.evaluatePolicy(policy, localizedReason: reason)
            return PluginResponse(ok: ok, output: ok ? "ok" : "failed")
        } catch {
            return PluginResponse(ok: false, output: error.localizedDescription)
        }
    }
}
