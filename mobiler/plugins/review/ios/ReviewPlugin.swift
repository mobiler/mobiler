import SharedTypes
import UIKit
import StoreKit

/// Free bundled plugin: request an in-app App Store review (no permission). op "request" →
/// "requested". The system rate-limits how often the prompt actually appears.
@MainActor
enum ReviewPlugin {
    static func handle(op: String, input: String) async -> PluginResponse {
        guard op == "request" else { return PluginResponse(ok: false, output: "unknown op '\(op)'") }
        guard let scene = UIApplication.shared.connectedScenes
            .compactMap({ $0 as? UIWindowScene })
            .first(where: { $0.activationState == .foregroundActive })
        else {
            return PluginResponse(ok: false, output: "no active scene")
        }
        AppStore.requestReview(in: scene)
        return PluginResponse(ok: true, output: "requested")
    }
}
