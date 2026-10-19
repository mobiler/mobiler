import SharedTypes
import UIKit

/// Free bundled plugin: a native single-choice picker. op "choose", input {"title", "options":[…],
/// "selected":Int} → ok with the chosen index as text, or ok=false "cancel". A wheel in a medium sheet.
@MainActor
enum PickerPlugin {
    static func handle(op: String, input: String) async -> PluginResponse {
        guard op == "choose" else { return PluginResponse(ok: false, output: "unknown op '\(op)'") }
        guard let data = input.data(using: .utf8),
              let obj = (try? JSONSerialization.jsonObject(with: data)) as? [String: Any],
              let options = obj["options"] as? [String], !options.isEmpty else {
            return PluginResponse(ok: false, output: "bad input")
        }
        let title = obj["title"] as? String
        let selected = obj["selected"] as? Int ?? 0
        guard let presenter = frontmostViewController() else {
            return PluginResponse(ok: false, output: "no view controller to present from")
        }
        return await withCheckedContinuation { cont in
            var resumed = false
            func done(_ r: PluginResponse) { if !resumed { resumed = true; cont.resume(returning: r) } }
            let vc = PickerSheetController(
                title: title, options: options, selected: selected,
                onDone: { done(PluginResponse(ok: true, output: String($0))) },
                onCancel: { done(PluginResponse(ok: false, output: "cancel")) })
            vc.modalPresentationStyle = .pageSheet
            if let sheet = vc.sheetPresentationController {
                sheet.detents = [.medium()]
                sheet.prefersGrabberVisible = true
            }
            vc.presentationController?.delegate = vc // swipe-to-dismiss = cancel
            presenter.present(vc, animated: true)
        }
    }

    // A standalone plugin file can't call Core.swift's private topViewController(), so it
    // finds the frontmost VC itself (as the filepicker plugin does).
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

private final class PickerSheetController: UIViewController, UIPickerViewDataSource, UIPickerViewDelegate, UIAdaptivePresentationControllerDelegate {
    private let options: [String]
    private let initialSelected: Int
    private let onDone: (Int) -> Void
    private let onCancel: () -> Void
    private let picker = UIPickerView()
    private var finished = false

    init(title: String?, options: [String], selected: Int, onDone: @escaping (Int) -> Void, onCancel: @escaping () -> Void) {
        self.options = options
        self.initialSelected = selected
        self.onDone = onDone
        self.onCancel = onCancel
        super.init(nibName: nil, bundle: nil)
        self.title = title
    }
    @available(*, unavailable) required init?(coder: NSCoder) { fatalError("init(coder:) unavailable") }

    override func viewDidLoad() {
        super.viewDidLoad()
        view.backgroundColor = .systemBackground
        // Match the app's brand tint (this sheet is presented outside SwiftUI's `.tint`).
        if let seed = ActiveTheme.current?.seed {
            view.tintColor = UIColor(red: CGFloat(seed.r) / 255, green: CGFloat(seed.g) / 255,
                                     blue: CGFloat(seed.b) / 255, alpha: 1)
        }

        let toolbar = UIToolbar()
        toolbar.translatesAutoresizingMaskIntoConstraints = false
        let titleItem = UIBarButtonItem(title: title, style: .plain, target: nil, action: nil)
        titleItem.isEnabled = false
        // The title is a non-tappable label; keep it fully legible (a disabled item is greyed out by default).
        let titleAttrs: [NSAttributedString.Key: Any] = [
            .foregroundColor: UIColor.label,
            .font: UIFont.preferredFont(forTextStyle: .headline),
        ]
        titleItem.setTitleTextAttributes(titleAttrs, for: .normal)
        titleItem.setTitleTextAttributes(titleAttrs, for: .disabled)
        toolbar.items = [
            UIBarButtonItem(barButtonSystemItem: .cancel, target: self, action: #selector(cancelTapped)),
            UIBarButtonItem(barButtonSystemItem: .flexibleSpace, target: nil, action: nil),
            titleItem,
            UIBarButtonItem(barButtonSystemItem: .flexibleSpace, target: nil, action: nil),
            UIBarButtonItem(barButtonSystemItem: .done, target: self, action: #selector(doneTapped)),
        ]

        picker.translatesAutoresizingMaskIntoConstraints = false
        picker.dataSource = self
        picker.delegate = self
        if initialSelected >= 0, initialSelected < options.count {
            picker.selectRow(initialSelected, inComponent: 0, animated: false)
        }

        view.addSubview(toolbar)
        view.addSubview(picker)
        NSLayoutConstraint.activate([
            toolbar.topAnchor.constraint(equalTo: view.safeAreaLayoutGuide.topAnchor, constant: 16),
            toolbar.leadingAnchor.constraint(equalTo: view.leadingAnchor),
            toolbar.trailingAnchor.constraint(equalTo: view.trailingAnchor),
            picker.topAnchor.constraint(equalTo: toolbar.bottomAnchor),
            picker.leadingAnchor.constraint(equalTo: view.leadingAnchor),
            picker.trailingAnchor.constraint(equalTo: view.trailingAnchor),
            picker.bottomAnchor.constraint(equalTo: view.safeAreaLayoutGuide.bottomAnchor),
        ])
    }

    @objc private func doneTapped() {
        guard !finished else { return }
        finished = true
        let idx = picker.selectedRow(inComponent: 0)
        dismiss(animated: true) { self.onDone(idx) }
    }
    @objc private func cancelTapped() {
        guard !finished else { return }
        finished = true
        dismiss(animated: true) { self.onCancel() }
    }
    func presentationControllerDidDismiss(_ presentationController: UIPresentationController) {
        guard !finished else { return }
        finished = true
        onCancel()
    }

    func numberOfComponents(in pickerView: UIPickerView) -> Int { 1 }
    func pickerView(_ pickerView: UIPickerView, numberOfRowsInComponent component: Int) -> Int { options.count }
    func pickerView(_ pickerView: UIPickerView, titleForRow row: Int, forComponent component: Int) -> String? {
        row >= 0 && row < options.count ? options[row] : nil
    }
}
