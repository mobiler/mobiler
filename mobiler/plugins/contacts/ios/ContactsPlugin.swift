import SharedTypes
import UIKit
import Contacts
import ContactsUI

/// Free bundled plugin: pick a contact via the system picker (no permission — CNContactPicker
/// grants access to just the chosen contact). op "pick" → "name|phone", ok=false on cancel.
@MainActor
enum ContactsPlugin {
    static func handle(op: String, input: String) async -> PluginResponse {
        guard op == "pick" else { return PluginResponse(ok: false, output: "unknown op '\(op)'") }
        guard let presenter = frontmostViewController() else {
            return PluginResponse(ok: false, output: "no view controller to present from")
        }
        return await withCheckedContinuation { cont in
            let picker = CNContactPickerViewController()
            let delegate = ContactPickerDelegate { cont.resume(returning: $0) }
            ContactPickerDelegate.retained = delegate
            picker.delegate = delegate
            presenter.present(picker, animated: true)
        }
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

private final class ContactPickerDelegate: NSObject, CNContactPickerDelegate {
    static var retained: ContactPickerDelegate?
    private let onResult: (PluginResponse) -> Void
    init(onResult: @escaping (PluginResponse) -> Void) { self.onResult = onResult }

    func contactPicker(_ picker: CNContactPickerViewController, didSelect contact: CNContact) {
        let name = CNContactFormatter.string(from: contact, style: .fullName) ?? ""
        let phone = contact.phoneNumbers.first?.value.stringValue ?? ""
        finish(PluginResponse(ok: true, output: "\(name)|\(phone)"))
    }
    func contactPickerDidCancel(_ picker: CNContactPickerViewController) {
        finish(PluginResponse(ok: false, output: "cancelled"))
    }
    private func finish(_ r: PluginResponse) {
        onResult(r)
        ContactPickerDelegate.retained = nil
    }
}
