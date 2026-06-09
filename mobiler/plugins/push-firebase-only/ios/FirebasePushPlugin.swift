import Foundation
import SharedTypes
import UIKit
import UserNotifications
import FirebaseCore
import FirebaseMessaging

// Remote push, Firebase-only (free, bundled). iOS uses the Firebase iOS SDK (FCM) instead of native
// APNs, so `register` returns an FCM token (matching Android) and the backend speaks one FCM API.
// Opt-in ALTERNATIVE to the `push` plugin; registers under the same cx name "push" so app code is
// identical (install one OR the other). Inbound notifications still arrive via APNs → the shell's
// AppDelegate/PushBridge, so the events stream (`subscribe`) is identical to the native-APNs plugin —
// only the token source differs: FCM mints the registration token after we hand it the raw APNs token.
enum FirebasePushPlugin {
    static func handle(op: String, input: String) async -> PluginResponse {
        switch op {
        case "register": return await register()
        default: return PluginResponse(ok: false, output: "unknown op '\(op)'")
        }
    }

    // Same as the native-APNs plugin's subscribe: attach an emit sink to PushBridge (the AppDelegate
    // forwards received/tapped notifications there) + register it with FCMDelegate so token rotations
    // also reach the stream. Park until cancelled, then detach.
    static func subscribe(op: String, input: String, emit: @escaping @Sendable (PluginResponse) -> Void) async {
        let sink: @Sendable (String) -> Void = { emit(PluginResponse(ok: true, output: $0)) }
        await MainActor.run {
            PushBridge.shared.attach(sink)
            FCMDelegate.shared.refreshSink = sink
        }
        await withTaskCancellationHandler {
            while !Task.isCancelled {
                try? await Task.sleep(nanoseconds: 1_000_000_000)
            }
        } onCancel: {
            Task { @MainActor in
                PushBridge.shared.detach()
                FCMDelegate.shared.refreshSink = nil
            }
        }
    }

    private static func register() async -> PluginResponse {
        do {
            await MainActor.run { FCMDelegate.shared.configureOnce() }
            let granted = try await UNUserNotificationCenter.current()
                .requestAuthorization(options: [.alert, .sound, .badge])
            guard granted else { return PluginResponse(ok: false, output: "denied") }
            await MainActor.run { UIApplication.shared.registerForRemoteNotifications() }
            // Wait for the APNs token (this also populates PushBridge.rawAPNsToken), then hand the raw
            // Data to FCM — iOS FCM relays through APNs, so the APNs token must be wired in for delivery.
            _ = try await PushBridge.shared.awaitToken()
            await MainActor.run {
                if let raw = PushBridge.shared.rawAPNsToken {
                    Messaging.messaging().apnsToken = raw
                }
            }
            let fcm = await FCMDelegate.shared.awaitFCMToken()
            return PluginResponse(ok: true, output: "{\"token\":\"\(fcm)\",\"platform\":\"fcm\"}")
        } catch {
            return PluginResponse(ok: false, output: error.localizedDescription)
        }
    }
}

// Owns Firebase configuration + the FCM registration-token callback (initial token + rotations).
// @MainActor for safe mutable state; the ObjC MessagingDelegate callback hops onto the main actor.
@MainActor
final class FCMDelegate: NSObject, MessagingDelegate {
    static let shared = FCMDelegate()

    private var configured = false
    private var lastToken: String?
    private var fcmWaiters: [(String) -> Void] = []
    var refreshSink: (@Sendable (String) -> Void)?

    func configureOnce() {
        guard !configured else { return }
        configured = true
        // The `analytics` plugin may have already configured Firebase at launch — configure only if not.
        if FirebaseApp.app() == nil { FirebaseApp.configure() }  // reads GoogleService-Info.plist
        Messaging.messaging().delegate = self
    }

    /// Await the FCM registration token. Returns immediately if it already arrived (Firebase can mint
    /// it before `register` gets here), otherwise parks until the delegate callback fires.
    func awaitFCMToken() async -> String {
        if let t = lastToken { return t }
        return await withCheckedContinuation { cont in
            fcmWaiters.append { cont.resume(returning: $0) }
        }
    }

    nonisolated func messaging(_ messaging: Messaging, didReceiveRegistrationToken fcmToken: String?) {
        let token = fcmToken ?? ""
        Task { @MainActor in
            self.lastToken = token
            if self.fcmWaiters.isEmpty {
                // Out-of-band rotation (no register call waiting) → notify the app via the stream.
                self.refreshSink?("{\"type\":\"token_refresh\",\"token\":\"\(token)\"}")
            } else {
                let waiters = self.fcmWaiters
                self.fcmWaiters.removeAll()
                for resume in waiters { resume(token) }
            }
        }
    }
}
