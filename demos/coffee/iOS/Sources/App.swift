import SwiftUI
import SharedTypes
import UIKit
import UserNotifications

/// App entry — the generic Mobiler shell. `Core` drives the Rust core; `render`
/// turns its `Widget` tree into SwiftUI. The whole UI is decided in Rust.
@main
struct CoffeeApp: App {
    // Bridges UIKit app-lifecycle + APNs/notification callbacks (which only arrive on a
    // UIApplicationDelegate) into the SwiftUI app — see AppDelegate + PushBridge below.
    @UIApplicationDelegateAdaptor(AppDelegate.self) private var appDelegate
    @StateObject private var core = Core()
    // Tracks foreground/background for the `system` lifecycle events (see SystemBridge).
    @Environment(\.scenePhase) private var scenePhase

    var body: some Scene {
        WindowGroup {
            RootView(core: core)
                // Inbound system events → SystemBridge → the `system` stream. `.onOpenURL` delivers
                // deep links (custom scheme / universal link) at launch and while running; scenePhase
                // reports foreground/background.
                .onOpenURL { SystemBridge.shared.didOpen(url: $0) }
                .onChange(of: scenePhase) { newPhase in SystemBridge.shared.didChangeScene(newPhase) }
        }
    }
}

private struct RootView: View {
    @ObservedObject var core: Core
    // Regular width (iPad / large landscape) caps the content column so a phone
    // layout doesn't stretch edge-to-edge on a big screen; compact fills as before.
    @Environment(\.horizontalSizeClass) private var hSize
    var body: some View {
        // Re-renders whenever the core publishes a new view model. A Scaffold
        // renders its own bars + scrollable body; any other root we wrap in a
        // scroll view so tall content scrolls (mirrors the Android shell).
        if case .scaffold = core.view {
            render(core.view) { core.update($0) }
        } else {
            ScrollView {
                render(core.view) { core.update($0) }
                    .padding(16)
                    .frame(maxWidth: hSize == .regular ? 760 : .infinity, alignment: .leading)
                    .frame(maxWidth: .infinity)  // center the capped column
            }
        }
    }
}

// UIKit app-delegate adaptor — the ONLY place APNs token + remote-notification callbacks arrive in a
// SwiftUI app. It forwards them to `PushBridge` (below). Inert unless the `push` plugin's `register`
// op runs: a push-less app never calls registerForRemoteNotifications, so it costs nothing.
final class AppDelegate: NSObject, UIApplicationDelegate, UNUserNotificationCenterDelegate {
    func application(
        _ application: UIApplication,
        didFinishLaunchingWithOptions launchOptions: [UIApplication.LaunchOptionsKey: Any]? = nil
    ) -> Bool {
        UNUserNotificationCenter.current().delegate = self
        return true
    }

    func application(_ application: UIApplication, didRegisterForRemoteNotificationsWithDeviceToken deviceToken: Data) {
        let hex = deviceToken.map { String(format: "%02x", $0) }.joined()
        // Forward both: the hex string (native-APNs push plugin) and the raw Data (the Firebase-only
        // push plugin hands the raw token to Messaging.apnsToken).
        PushBridge.shared.didRegister(token: hex, raw: deviceToken)
    }

    func application(_ application: UIApplication, didFailToRegisterForRemoteNotificationsWithError error: Error) {
        PushBridge.shared.didFail(error: error)
    }

    // Foreground receipt — show the banner AND forward the payload to the events stream.
    func userNotificationCenter(
        _ center: UNUserNotificationCenter,
        willPresent notification: UNNotification,
        withCompletionHandler completionHandler: @escaping (UNNotificationPresentationOptions) -> Void
    ) {
        PushBridge.shared.didReceive(userInfo: notification.request.content.userInfo)
        completionHandler([.banner, .sound])
    }

    // Tap — forward the payload. Also fires when a tap LAUNCHES the app; PushBridge buffers it until
    // the core subscribes.
    func userNotificationCenter(
        _ center: UNUserNotificationCenter,
        didReceive response: UNNotificationResponse,
        withCompletionHandler completionHandler: @escaping () -> Void
    ) {
        PushBridge.shared.didReceive(userInfo: response.notification.request.content.userInfo)
        completionHandler()
    }
}

// Always-present, plugin-agnostic forwarder between the AppDelegate and the optional `push` plugin.
// The plugin attaches a token-waiter (register op) + an event sink (events stream); until a sink is
// attached, inbound payloads buffer and flush on attach — so a notification that launched a dead
// process still reaches the app. App.swift references nothing from the plugin, so this compiles and
// stays dormant in push-less apps.
@MainActor
final class PushBridge {
    static let shared = PushBridge()

    private var tokenWaiters: [(Result<String, Error>) -> Void] = []
    private var sink: (@Sendable (String) -> Void)?
    private var buffer: [String] = []

    /// The raw APNs device token (set by the AppDelegate). The Firebase-only push plugin reads this to
    /// hand to `Messaging.messaging().apnsToken`; the native-APNs plugin uses the hex string instead.
    private(set) var rawAPNsToken: Data?

    // --- called by the push plugin ---

    /// Await the APNs device token (resolved by the AppDelegate's didRegister callback).
    func awaitToken() async throws -> String {
        try await withCheckedThrowingContinuation { cont in
            tokenWaiters.append { cont.resume(with: $0) }
        }
    }

    func attach(_ sink: @escaping @Sendable (String) -> Void) {
        self.sink = sink
        for payload in buffer { sink(payload) }
        buffer.removeAll()
    }

    func detach() { sink = nil }

    // --- called by the AppDelegate ---

    func didRegister(token: String, raw: Data) {
        rawAPNsToken = raw
        if tokenWaiters.isEmpty {
            // An out-of-band rotation (no register call in flight) → notify the app via the stream.
            emit("{\"type\":\"token_refresh\",\"token\":\"\(token)\"}")
        } else {
            let waiters = tokenWaiters
            tokenWaiters.removeAll()
            for resume in waiters { resume(.success(token)) }
        }
    }

    func didFail(error: Error) {
        let waiters = tokenWaiters
        tokenWaiters.removeAll()
        for resume in waiters { resume(.failure(error)) }
    }

    func didReceive(userInfo: [AnyHashable: Any]) {
        emit(Self.jsonString(from: userInfo))
    }

    private func emit(_ payload: String) {
        if let sink { sink(payload) } else { buffer.append(payload) }
    }

    private static func jsonString(from userInfo: [AnyHashable: Any]) -> String {
        let stringKeyed = Dictionary(uniqueKeysWithValues: userInfo.compactMap { key, value in
            (key as? String).map { ($0, value) }
        })
        if let data = try? JSONSerialization.data(withJSONObject: stringKeyed),
           let json = String(data: data, encoding: .utf8) {
            return json
        }
        return "{}"
    }
}

// Always-present, plugin-agnostic forwarder for inbound *system* events — deep-link URLs (`.onOpenURL`)
// and app lifecycle (scenePhase) — into the built-in `system` stream (cx.subscribe). Deep links arriving
// before the core subscribes BUFFER and flush on attach (launch-from-dead), exactly like PushBridge;
// lifecycle changes emit live, and the current state is sent on attach. Dormant until something
// subscribes to "system".
@MainActor
final class SystemBridge {
    static let shared = SystemBridge()

    private var sink: (@Sendable (String) -> Void)?
    private var buffer: [String] = []
    private var lastPhase: ScenePhase = .active

    func attach(_ sink: @escaping @Sendable (String) -> Void) {
        self.sink = sink
        for payload in buffer { sink(payload) }
        buffer.removeAll()
        // Tell the freshly-subscribed app the current lifecycle state.
        emitLifecycle(lastPhase == .background ? "background" : "active")
    }
    func detach() { sink = nil }

    func didOpen(url: URL) {
        emit("{\"type\":\"deeplink\",\"url\":\(Self.jsonString(url.absoluteString))}")
    }

    func didChangeScene(_ phase: ScenePhase) {
        lastPhase = phase
        switch phase {
        case .active: emitLifecycle("active")
        case .background: emitLifecycle("background")
        default: break  // .inactive is a transient app-switcher state — ignore
        }
    }

    private func emitLifecycle(_ state: String) {
        // Lifecycle is live-only (not buffered) — a fresh subscriber gets the current state on attach.
        sink?("{\"type\":\"lifecycle\",\"state\":\"\(state)\"}")
    }

    private func emit(_ payload: String) {
        if let sink { sink(payload) } else { buffer.append(payload) }
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
