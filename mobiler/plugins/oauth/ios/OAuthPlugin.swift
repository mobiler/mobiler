import AuthenticationServices
import SharedTypes
import UIKit

// OAuth 2.0 / OIDC login (free, bundled). Presents the system auth browser via
// ASWebAuthenticationSession — a private, SSO-capable web session that the OS dismisses
// automatically once the provider redirects to the app's custom URL scheme, handing the
// redirect URL straight back (no Info.plist scheme registration needed; the session
// intercepts `callbackURLScheme` itself).
//
// op "login", input = JSON {"url": "<authorize URL>", "scheme": "<callback scheme>"} →
//   ok:true,  output = the full redirect URL ("<scheme>://…?code=…&state=…")
//   ok:false, output = "cancelled" (user dismissed) or an error message.
// The Rust core builds the authorize URL (client_id, redirect_uri, scope, state, PKCE) and,
// on success, parses the code/state, exchanges via cx.http, and stores tokens via securestore.
@MainActor
enum OAuthPlugin {
    // Retained for the lifetime of the in-flight session — ASWebAuthenticationSession must outlive
    // `handle`'s synchronous return, and its `presentationContextProvider` is held *weakly*, so the
    // provider must be retained too. `nonisolated(unsafe)`: the session's completion handler is a
    // nonisolated closure but always runs on the main thread (single-shot), so these are safe to
    // clear from it.
    nonisolated(unsafe) private static var session: ASWebAuthenticationSession?
    nonisolated(unsafe) private static var presenter: OAuthPresenter?

    static func handle(op: String, input: String) async -> PluginResponse {
        guard op == "login" else { return PluginResponse(ok: false, output: "unknown op '\(op)'") }
        guard let data = input.data(using: .utf8),
              let obj = try? JSONSerialization.jsonObject(with: data) as? [String: Any],
              let urlStr = obj["url"] as? String, let url = URL(string: urlStr) else {
            return PluginResponse(ok: false, output: "input must be JSON {\"url\":…, \"scheme\":…}")
        }
        let scheme = (obj["scheme"] as? String) ?? ""

        return await withCheckedContinuation { (cont: CheckedContinuation<PluginResponse, Never>) in
            let presenter = OAuthPresenter()
            let session = ASWebAuthenticationSession(
                url: url,
                callbackURLScheme: scheme.isEmpty ? nil : scheme
            ) { callbackURL, error in
                // Exactly one of this completion / the start()-failed branch runs, so resume once.
                Self.session = nil
                Self.presenter = nil
                if let cb = callbackURL {
                    cont.resume(returning: PluginResponse(ok: true, output: cb.absoluteString))
                } else if let err = error as? ASWebAuthenticationSessionError, err.code == .canceledLogin {
                    cont.resume(returning: PluginResponse(ok: false, output: "cancelled"))
                } else {
                    cont.resume(returning: PluginResponse(ok: false, output: error?.localizedDescription ?? "failed"))
                }
            }
            session.presentationContextProvider = presenter
            // false = allow the system browser's existing session (SSO). Set true for a private,
            // cookie-less session that never reuses an existing login.
            session.prefersEphemeralWebBrowserSession = false
            Self.session = session
            Self.presenter = presenter
            if !session.start() {
                Self.session = nil
                Self.presenter = nil
                cont.resume(returning: PluginResponse(ok: false, output: "could not start auth session"))
            }
        }
    }
}

// Supplies the window ASWebAuthenticationSession anchors its UI to.
final class OAuthPresenter: NSObject, ASWebAuthenticationPresentationContextProviding {
    func presentationAnchor(for session: ASWebAuthenticationSession) -> ASPresentationAnchor {
        let scene = UIApplication.shared.connectedScenes
            .compactMap { $0 as? UIWindowScene }
            .first { $0.activationState == .foregroundActive }
        return scene?.keyWindow ?? ASPresentationAnchor()
    }
}
