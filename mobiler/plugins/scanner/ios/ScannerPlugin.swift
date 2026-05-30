import AVFoundation
import UIKit

// Barcode/QR scanner (free, bundled). iOS has no prebuilt scanner like Android's ML Kit, so
// this presents a full-screen AVCaptureMetadataOutput camera session (system frameworks only —
// no SPM package). Single-shot: opens the camera, returns the first code as "<format>:<value>"
// (e.g. "qr:https://…", "ean13:978…"), then dismisses. Requires NSCameraUsageDescription
// (added by the plugin manifest). The simulator has no camera → returns ok:false there.
@MainActor
enum ScannerPlugin {
    static func handle(op: String, input: String) async -> PluginResponse {
        guard op == "scan" else { return PluginResponse(ok: false, output: "unknown op '\(op)'") }
        guard let presenter = topViewController() else {
            return PluginResponse(ok: false, output: "no view controller to present from")
        }
        return await withCheckedContinuation { cont in
            let vc = ScannerViewController { result in cont.resume(returning: result) }
            vc.modalPresentationStyle = .fullScreen
            presenter.present(vc, animated: true)
        }
    }
}

private final class ScannerViewController: UIViewController, AVCaptureMetadataOutputObjectsDelegate {
    private let onResult: (PluginResponse) -> Void
    private let session = AVCaptureSession()
    private var finished = false

    init(onResult: @escaping (PluginResponse) -> Void) {
        self.onResult = onResult
        super.init(nibName: nil, bundle: nil)
    }
    required init?(coder: NSCoder) { fatalError("not used") }

    override func viewDidLoad() {
        super.viewDidLoad()
        view.backgroundColor = .black

        guard let device = AVCaptureDevice.default(for: .video),
              let deviceInput = try? AVCaptureDeviceInput(device: device),
              session.canAddInput(deviceInput)
        else { return finish(PluginResponse(ok: false, output: "camera not available")) }
        session.addInput(deviceInput)

        let output = AVCaptureMetadataOutput()
        guard session.canAddOutput(output) else {
            return finish(PluginResponse(ok: false, output: "scanner output unavailable"))
        }
        session.addOutput(output)
        output.setMetadataObjectsDelegate(self, queue: .main)
        // All the symbologies ML Kit's FORMAT_ALL_FORMATS covers, that iOS supports.
        output.metadataObjectTypes = [
            .qr, .ean13, .ean8, .upce, .code128, .code39, .code93, .codabar,
            .itf14, .interleaved2of5, .dataMatrix, .pdf417, .aztec,
        ]

        let preview = AVCaptureVideoPreviewLayer(session: session)
        preview.frame = view.layer.bounds
        preview.videoGravity = .resizeAspectFill
        view.layer.addSublayer(preview)

        // A Cancel button so the user can back out (maps to ok:false "cancelled").
        let cancel = UIButton(type: .system)
        cancel.setTitle("Cancel", for: .normal)
        cancel.setTitleColor(.white, for: .normal)
        cancel.titleLabel?.font = .systemFont(ofSize: 18, weight: .semibold)
        cancel.frame = CGRect(x: 20, y: 60, width: 100, height: 44)
        cancel.addTarget(self, action: #selector(cancelTapped), for: .touchUpInside)
        view.addSubview(cancel)

        DispatchQueue.global(qos: .userInitiated).async { [weak self] in self?.session.startRunning() }
    }

    @objc private func cancelTapped() { finish(PluginResponse(ok: false, output: "cancelled")) }

    func metadataOutput(_ output: AVCaptureMetadataOutput,
                        didOutput objects: [AVMetadataObject],
                        from connection: AVCaptureConnection) {
        guard let obj = objects.first as? AVMetadataMachineReadableCodeObject,
              let value = obj.stringValue else { return }
        finish(PluginResponse(ok: true, output: "\(formatName(obj.type)):\(value)"))
    }

    private func finish(_ r: PluginResponse) {
        if finished { return }
        finished = true
        if session.isRunning { session.stopRunning() }
        dismiss(animated: true) { self.onResult(r) }
    }

    // A short symbology tag mirroring the Android side, so the Rust core sees the same format names.
    private func formatName(_ t: AVMetadataObject.ObjectType) -> String {
        switch t {
        case .qr: return "qr"
        case .ean13: return "ean13"
        case .ean8: return "ean8"
        case .upce: return "upce"
        case .code128: return "code128"
        case .code39: return "code39"
        case .code93: return "code93"
        case .codabar: return "codabar"
        case .itf14, .interleaved2of5: return "itf"
        case .dataMatrix: return "datamatrix"
        case .pdf417: return "pdf417"
        case .aztec: return "aztec"
        default: return "other"
        }
    }
}
