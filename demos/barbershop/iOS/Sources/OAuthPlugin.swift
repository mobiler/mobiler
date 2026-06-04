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
    // Retained for the lifetime of the in-flight session (ASWebAuthenticationSession and its
    // presentation-context provider must outlive `handle`'s synchronous return).
    private static var session: ASWebAuthenticationSession?
    private static let presenter = OAuthPresenter()

    static func handle(op: String, input: String) async -> PluginResponse {
        guard op == "login" else { return PluginResponse(ok: false, output: "unknown op '\(op)'") }
        guard let data = input.data(using: .utf8),
              let obj = try? JSONSerialization.jsonObject(with: data) as? [String: Any],
              let urlStr = obj["url"] as? String, let url = URL(string: urlStr) else {
            return PluginResponse(ok: false, output: "input must be JSON {\"url\":…, \"scheme\":…}")
        }
        let scheme = (obj["scheme"] as? String) ?? ""

        return await withCheckedContinuation { (cont: CheckedContinuation<PluginResponse, Never>) in
            var resumed = false
            func finish(_ r: PluginResponse) {
                if resumed { return }
                resumed = true
                Self.session = nil
                cont.resume(returning: r)
            }
            let session = ASWebAuthenticationSession(
                url: url,
                callbackURLScheme: scheme.isEmpty ? nil : scheme
            ) { callbackURL, error in
                if let cb = callbackURL {
                    finish(PluginResponse(ok: true, output: cb.absoluteString))
                } else if let err = error as? ASWebAuthenticationSessionError, err.code == .canceledLogin {
                    finish(PluginResponse(ok: false, output: "cancelled"))
                } else {
                    finish(PluginResponse(ok: false, output: error?.localizedDescription ?? "failed"))
                }
            }
            session.presentationContextProvider = Self.presenter
            // false = allow the system browser's existing session (SSO). Set true for a private,
            // cookie-less session that never reuses an existing login.
            session.prefersEphemeralWebBrowserSession = false
            Self.session = session
            if !session.start() {
                finish(PluginResponse(ok: false, output: "could not start auth session"))
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
