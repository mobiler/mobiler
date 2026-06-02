import SharedTypes
import UIKit
import EventKit
import EventKitUI

/// Free bundled plugin: add a calendar event via the system editor. op "add", input JSON
/// {title, start, end} (start/end = epoch millis) → "saved" / "cancelled". Presents
/// EKEventEditViewController (which prompts for calendar access). Needs the calendar
/// usage-description keys (added to Info.plist by `plugin add`).
@MainActor
enum CalendarPlugin {
    static func handle(op: String, input: String) async -> PluginResponse {
        guard op == "add" else { return PluginResponse(ok: false, output: "unknown op '\(op)'") }
        guard let presenter = frontmostViewController() else {
            return PluginResponse(ok: false, output: "no view controller to present from")
        }
        let obj = (try? JSONSerialization.jsonObject(with: Data(input.utf8))) as? [String: Any]
        let store = EKEventStore()
        let event = EKEvent(eventStore: store)
        event.title = (obj?["title"] as? String) ?? ""
        if let ms = (obj?["start"] as? NSNumber)?.doubleValue, ms > 0 {
            event.startDate = Date(timeIntervalSince1970: ms / 1000)
        }
        if let ms = (obj?["end"] as? NSNumber)?.doubleValue, ms > 0 {
            event.endDate = Date(timeIntervalSince1970: ms / 1000)
        }
        event.calendar = store.defaultCalendarForNewEvents
        return await withCheckedContinuation { cont in
            let vc = EKEventEditViewController()
            vc.eventStore = store
            vc.event = event
            let delegate = EventEditDelegate { cont.resume(returning: $0) }
            EventEditDelegate.retained = delegate
            vc.editViewDelegate = delegate
            presenter.present(vc, animated: true)
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

private final class EventEditDelegate: NSObject, EKEventEditViewDelegate {
    static var retained: EventEditDelegate?
    private let onResult: (PluginResponse) -> Void
    init(onResult: @escaping (PluginResponse) -> Void) { self.onResult = onResult }

    func eventEditViewController(_ controller: EKEventEditViewController, didCompleteWith action: EKEventEditViewAction) {
        controller.dismiss(animated: true)
        switch action {
        case .saved: finish(PluginResponse(ok: true, output: "saved"))
        default: finish(PluginResponse(ok: false, output: "cancelled"))
        }
    }
    private func finish(_ r: PluginResponse) {
        onResult(r)
        EventEditDelegate.retained = nil
    }
}
