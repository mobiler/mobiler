import Foundation
import SharedTypes
import UIKit
import UserNotifications

// Remote push (free, bundled). Native APNs via system frameworks — no third-party SDK. Two surfaces:
//   cx.plugin("push", "register", "", |r| Msg::PushToken(r))          // → {"token":"…","platform":"apns"}
//   cx.subscribe("push", "push", "events", "", |r| Msg::PushEvent(r)) // received/tapped payloads
//
// The actual APNs plumbing lives in the shell's AppDelegate + `PushBridge` (in App.swift) — only the
// AppDelegate receives the OS token + notification callbacks. This plugin is the thin cx adapter:
// `register` asks for authorization + registers for remote notifications, then awaits the token the
// AppDelegate forwards to PushBridge; `subscribe` attaches an emit sink to PushBridge (which buffers
// any notification that arrived before the core subscribed — e.g. a tap that launched the app).
//
// Needs the `aps-environment` entitlement + Push Notifications enabled on the App ID (the plugin
// manifest adds the entitlement; the dev-portal capability is the one manual iOS step).
enum PushPlugin {
    static func handle(op: String, input: String) async -> PluginResponse {
        switch op {
        case "register": return await register()
        default: return PluginResponse(ok: false, output: "unknown op '\(op)'")
        }
    }

    // Streaming entrypoint (cx.subscribe): attach to PushBridge and stay parked until the
    // subscription's Task is cancelled (cx.unsubscribe), then detach.
    static func subscribe(op: String, input: String, emit: @escaping @Sendable (PluginResponse) -> Void) async {
        let sink: @Sendable (String) -> Void = { emit(PluginResponse(ok: true, output: $0)) }
        await MainActor.run { PushBridge.shared.attach(sink) }
        await withTaskCancellationHandler {
            while !Task.isCancelled {
                try? await Task.sleep(nanoseconds: 1_000_000_000)
            }
        } onCancel: {
            Task { @MainActor in PushBridge.shared.detach() }
        }
    }

    private static func register() async -> PluginResponse {
        do {
            let granted = try await UNUserNotificationCenter.current()
                .requestAuthorization(options: [.alert, .sound, .badge])
            guard granted else { return PluginResponse(ok: false, output: "denied") }
            await MainActor.run { UIApplication.shared.registerForRemoteNotifications() }
            // The AppDelegate's didRegister…DeviceToken resolves this with the hex APNs token.
            let token = try await PushBridge.shared.awaitToken()
            return PluginResponse(ok: true, output: "{\"token\":\"\(token)\",\"platform\":\"apns\"}")
        } catch {
            return PluginResponse(ok: false, output: error.localizedDescription)
        }
    }
}
