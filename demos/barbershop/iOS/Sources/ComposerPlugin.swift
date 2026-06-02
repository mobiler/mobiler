import SharedTypes
import UIKit
import MessageUI

/// Free bundled plugin: open the system composer for email / SMS / a phone call (no permission).
/// - op "email", input {to?, subject?, body?} → MFMailComposeViewController (falls back to mailto:)
/// - op "sms",   input {to?, body?}           → MFMessageComposeViewController (falls back to sms:)
/// - op "call",  input {number}               → opens the dialer (tel:)
@MainActor
enum ComposerPlugin {
    static func handle(op: String, input: String) async -> PluginResponse {
        let obj = (try? JSONSerialization.jsonObject(with: Data(input.utf8))) as? [String: Any] ?? [:]
        switch op {
        case "email": return await email(obj)
        case "sms": return await sms(obj)
        case "call": return await call(obj)
        default: return PluginResponse(ok: false, output: "unknown op '\(op)'")
        }
    }

    private static func call(_ obj: [String: Any]) async -> PluginResponse {
        let number = ((obj["number"] as? String) ?? "").filter { !$0.isWhitespace }
        guard let url = URL(string: "tel:\(number)") else {
            return PluginResponse(ok: false, output: "invalid number")
        }
        // Don't gate on canOpenURL: since iOS 9 it returns false for any scheme not declared
        // in LSApplicationQueriesSchemes, false-negativing even when an app can handle it.
        // `open(_:)` needs no whitelist and reports the real result.
        let opened = await UIApplication.shared.open(url)
        return PluginResponse(ok: opened, output: opened ? "opened" : "cannot place call")
    }

    private static func email(_ obj: [String: Any]) async -> PluginResponse {
        // canSendMail() only reflects Apple Mail with a configured account — it ignores
        // third-party mail apps (Gmail, Outlook, …). So when it's false we still try the
        // mailto: URL, which the system routes to the user's default mail app.
        guard MFMailComposeViewController.canSendMail() else { return await openURLFallback("mailto:", obj) }
        guard let presenter = frontmostViewController() else {
            return PluginResponse(ok: false, output: "no view controller to present from")
        }
        return await withCheckedContinuation { cont in
            let vc = MFMailComposeViewController()
            if let to = obj["to"] as? String, !to.isEmpty { vc.setToRecipients([to]) }
            vc.setSubject((obj["subject"] as? String) ?? "")
            vc.setMessageBody((obj["body"] as? String) ?? "", isHTML: false)
            let delegate = MailDelegate { cont.resume(returning: $0) }
            MailDelegate.retained = delegate
            vc.mailComposeDelegate = delegate
            presenter.present(vc, animated: true)
        }
    }

    private static func sms(_ obj: [String: Any]) async -> PluginResponse {
        guard MFMessageComposeViewController.canSendText() else { return await openURLFallback("sms:", obj) }
        guard let presenter = frontmostViewController() else {
            return PluginResponse(ok: false, output: "no view controller to present from")
        }
        return await withCheckedContinuation { cont in
            let vc = MFMessageComposeViewController()
            if let to = obj["to"] as? String, !to.isEmpty { vc.recipients = [to] }
            vc.body = (obj["body"] as? String) ?? ""
            let delegate = MessageDelegate { cont.resume(returning: $0) }
            MessageDelegate.retained = delegate
            vc.messageComposeDelegate = delegate
            presenter.present(vc, animated: true)
        }
    }

    // No Apple Mail/Messages composer → open the scheme URL, which the system routes to the
    // user's default mail/SMS app (e.g. Gmail). No canOpenURL gate (see `call` above for why).
    private static func openURLFallback(_ scheme: String, _ obj: [String: Any]) async -> PluginResponse {
        let to = (obj["to"] as? String) ?? ""
        var comps = URLComponents(string: "\(scheme)\(to)")
        var q: [URLQueryItem] = []
        if let s = obj["subject"] as? String, !s.isEmpty { q.append(URLQueryItem(name: "subject", value: s)) }
        if let b = obj["body"] as? String, !b.isEmpty { q.append(URLQueryItem(name: "body", value: b)) }
        if !q.isEmpty { comps?.queryItems = q }
        guard let url = comps?.url else {
            return PluginResponse(ok: false, output: "invalid url")
        }
        let opened = await UIApplication.shared.open(url)
        return PluginResponse(ok: opened, output: opened ? "opened" : "no app to handle this")
    }

    private static func frontmostViewController() -> UIViewController? {
        let keyWindow = UIApplication.shared.connectedScenes
            .compactMap { $0 as? UIWindowScene }
            .first { $0.activationState == .foregroundActive }?
            .keyWindow
        var top = keyWindow?.rootViewController
        while let presented = top?.presentedViewController { top = presented }
        return top
    }
}

private final class MailDelegate: NSObject, MFMailComposeViewControllerDelegate {
    static var retained: MailDelegate?
    private let onResult: (PluginResponse) -> Void
    init(onResult: @escaping (PluginResponse) -> Void) { self.onResult = onResult }
    func mailComposeController(_ controller: MFMailComposeViewController, didFinishWith result: MFMailComposeResult, error: Error?) {
        controller.dismiss(animated: true)
        finish(PluginResponse(ok: result == .sent, output: result == .sent ? "sent" : "cancelled"))
    }
    private func finish(_ r: PluginResponse) { onResult(r); MailDelegate.retained = nil }
}

private final class MessageDelegate: NSObject, MFMessageComposeViewControllerDelegate {
    static var retained: MessageDelegate?
    private let onResult: (PluginResponse) -> Void
    init(onResult: @escaping (PluginResponse) -> Void) { self.onResult = onResult }
    func messageComposeViewController(_ controller: MFMessageComposeViewController, didFinishWith result: MessageComposeResult) {
        controller.dismiss(animated: true)
        finish(PluginResponse(ok: result == .sent, output: result == .sent ? "sent" : "cancelled"))
    }
    private func finish(_ r: PluginResponse) { onResult(r); MessageDelegate.retained = nil }
}
