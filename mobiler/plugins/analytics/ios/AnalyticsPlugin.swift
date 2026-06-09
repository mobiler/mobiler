import FirebaseAnalytics
import FirebaseCore
import FirebaseCrashlytics
import Foundation
import SharedTypes

// analytics (free, bundled, EXPERIMENTAL). Product analytics + automatic crash capture via Firebase
// Analytics + Crashlytics. Fire-and-forget request/response (no stream). Crash capture is AUTOMATIC once
// Firebase is configured. Ops (input is JSON unless noted):
//   logEvent {name, params?}   setUserId <id>   setUserProperty {name, value}   setEnabled "true"|"false"
//   log <message>              recordError {message, domain?}                    testCrash ""  (dev only)
//
// `AnalyticsPlugin.bootstrap()` runs at launch (App.swift `// mobiler:app-launch`) and configures
// Firebase IF a GoogleService-Info.plist is bundled. Without it, the app does NOT crash — every op
// short-circuits to ok:false ("Firebase not configured") so the plugin is safe to add before config.
enum AnalyticsPlugin {
    // Launch hook — configure Firebase early so Crashlytics catches startup crashes. Guarded twice:
    // skip if already configured (push-firebase-only may have), and skip if no config file is present.
    static func bootstrap() {
        guard FirebaseApp.app() == nil else { return }
        guard Bundle.main.path(forResource: "GoogleService-Info", ofType: "plist") != nil else { return }
        FirebaseApp.configure()
    }

    static func handle(op: String, input: String) async -> PluginResponse {
        guard FirebaseApp.app() != nil else {
            return PluginResponse(ok: false, output: "Firebase not configured — add GoogleService-Info.plist")
        }
        switch op {
        case "logEvent": return logEvent(input)
        case "setUserId":
            Analytics.setUserID(input.isEmpty ? nil : input)
            return PluginResponse(ok: true, output: "")
        case "setUserProperty": return setUserProperty(input)
        case "setEnabled": return setEnabled(input)
        case "log":
            Crashlytics.crashlytics().log(input)
            return PluginResponse(ok: true, output: "")
        case "recordError": return recordError(input)
        case "testCrash":
            // Deliberate crash to verify Crashlytics wiring — the report uploads on the NEXT launch.
            fatalError("analytics testCrash")
        default:
            return PluginResponse(ok: false, output: "unknown op '\(op)'")
        }
    }

    private static func logEvent(_ input: String) -> PluginResponse {
        guard let obj = json(input), let name = obj["name"] as? String else {
            return PluginResponse(ok: false, output: "expected {name, params?}")
        }
        let params = obj["params"] as? [String: Any]
        Analytics.logEvent(name, parameters: params)
        return PluginResponse(ok: true, output: "")
    }

    private static func setUserProperty(_ input: String) -> PluginResponse {
        guard let obj = json(input), let name = obj["name"] as? String else {
            return PluginResponse(ok: false, output: "expected {name, value}")
        }
        Analytics.setUserProperty(obj["value"] as? String, forName: name)
        return PluginResponse(ok: true, output: "")
    }

    private static func setEnabled(_ input: String) -> PluginResponse {
        let on = (input == "true")
        Analytics.setAnalyticsCollectionEnabled(on)
        Crashlytics.crashlytics().setCrashlyticsCollectionEnabled(on)
        return PluginResponse(ok: true, output: "")
    }

    private static func recordError(_ input: String) -> PluginResponse {
        let obj = json(input)
        let message = (obj?["message"] as? String) ?? "error"
        let domain = (obj?["domain"] as? String) ?? "mobiler.analytics"
        let error = NSError(domain: domain, code: 0, userInfo: [NSLocalizedDescriptionKey: message])
        Crashlytics.crashlytics().record(error: error)
        return PluginResponse(ok: true, output: "")
    }

    private static func json(_ input: String) -> [String: Any]? {
        guard let data = input.data(using: .utf8),
              let obj = try? JSONSerialization.jsonObject(with: data) as? [String: Any]
        else { return nil }
        return obj
    }
}
