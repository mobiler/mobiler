import SharedTypes
import AVFoundation

/// Free bundled plugin: text-to-speech (no permission). op "speak", input = the text (or
/// {"text": …}). Speaks it and resolves "done" when finished.
enum TtsPlugin {
    static func handle(op: String, input: String) async -> PluginResponse {
        guard op == "speak" else { return PluginResponse(ok: false, output: "unknown op '\(op)'") }
        let text = parseText(input)
        guard !text.isEmpty else { return PluginResponse(ok: false, output: "no text") }
        return await withCheckedContinuation { cont in
            let synth = AVSpeechSynthesizer()
            let delegate = SpeechDelegate { cont.resume(returning: $0) }
            delegate.synth = synth // retain the synthesizer for the utterance's lifetime
            SpeechDelegate.retained = delegate
            synth.delegate = delegate
            synth.speak(AVSpeechUtterance(string: text))
        }
    }

    private static func parseText(_ input: String) -> String {
        if input.hasPrefix("{"),
           let obj = (try? JSONSerialization.jsonObject(with: Data(input.utf8))) as? [String: Any] {
            return (obj["text"] as? String) ?? ""
        }
        return input
    }
}

private final class SpeechDelegate: NSObject, AVSpeechSynthesizerDelegate {
    static var retained: SpeechDelegate?
    var synth: AVSpeechSynthesizer?
    private let onResult: (PluginResponse) -> Void
    private var done = false
    init(onResult: @escaping (PluginResponse) -> Void) { self.onResult = onResult }

    func speechSynthesizer(_ s: AVSpeechSynthesizer, didFinish utterance: AVSpeechUtterance) {
        finish(PluginResponse(ok: true, output: "done"))
    }
    func speechSynthesizer(_ s: AVSpeechSynthesizer, didCancel utterance: AVSpeechUtterance) {
        finish(PluginResponse(ok: false, output: "cancelled"))
    }
    private func finish(_ r: PluginResponse) {
        if done { return }
        done = true
        onResult(r)
        SpeechDelegate.retained = nil
    }
}
