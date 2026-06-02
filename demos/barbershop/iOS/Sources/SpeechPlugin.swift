import SharedTypes
import Foundation
import Speech
import AVFoundation

/// Speech-to-text — matches the Android contract: op "listen" → recognized text, or ok=false
/// "cancelled"/error. Requests speech + mic authorization, records via AVAudioEngine, and resolves
/// with the best transcription after the result is final or a short max-listen window elapses.
/// Needs NSSpeechRecognitionUsageDescription + NSMicrophoneUsageDescription.
enum SpeechPlugin {
    static func handle(op: String, input: String) async -> PluginResponse {
        guard op == "listen" else { return PluginResponse(ok: false, output: "unknown op '\(op)'") }

        let authorized = await withCheckedContinuation { (cont: CheckedContinuation<Bool, Never>) in
            SFSpeechRecognizer.requestAuthorization { status in cont.resume(returning: status == .authorized) }
        }
        guard authorized else { return PluginResponse(ok: false, output: "denied") }

        return await withCheckedContinuation { cont in
            let session = SpeechSession()
            SpeechSession.retained = session
            session.start { result in
                SpeechSession.retained = nil
                cont.resume(returning: result)
            }
        }
    }
}

// Owns the audio engine + recognition task for one listen. `@unchecked Sendable` because the
// recognizer/engine callbacks fire on arbitrary queues; all mutable state is funneled onto the
// main queue, and `finished` guards against a double-resume.
private final class SpeechSession: @unchecked Sendable {
    nonisolated(unsafe) static var retained: SpeechSession?

    private let engine = AVAudioEngine()
    private let recognizer = SFSpeechRecognizer()
    private var request: SFSpeechAudioBufferRecognitionRequest?
    private var task: SFSpeechRecognitionTask?
    private var finished = false
    private var best = ""
    private var onDone: ((PluginResponse) -> Void)?

    func start(_ onDone: @escaping (PluginResponse) -> Void) {
        self.onDone = onDone
        guard let recognizer = recognizer, recognizer.isAvailable else {
            finish(PluginResponse(ok: false, output: "recognizer unavailable")); return
        }
        do {
            let audio = AVAudioSession.sharedInstance()
            try audio.setCategory(.record, mode: .measurement, options: .duckOthers)
            try audio.setActive(true, options: .notifyOthersOnDeactivation)

            let request = SFSpeechAudioBufferRecognitionRequest()
            request.shouldReportPartialResults = true
            self.request = request

            let node = engine.inputNode
            let format = node.outputFormat(forBus: 0)
            node.installTap(onBus: 0, bufferSize: 1024, format: format) { [weak self] buffer, _ in
                self?.request?.append(buffer)
            }
            engine.prepare()
            try engine.start()

            task = recognizer.recognitionTask(with: request) { [weak self] result, error in
                DispatchQueue.main.async {
                    guard let self = self else { return }
                    if let result = result {
                        self.best = result.bestTranscription.formattedString
                        if result.isFinal { self.finish(PluginResponse(ok: true, output: self.best)) }
                    }
                    if error != nil {
                        self.finish(self.best.isEmpty
                            ? PluginResponse(ok: false, output: "error")
                            : PluginResponse(ok: true, output: self.best))
                    }
                }
            }
            // Max listen window — stop capturing; the recognizer delivers the final result.
            DispatchQueue.main.asyncAfter(deadline: .now() + 5) { [weak self] in self?.stopListening() }
        } catch {
            finish(PluginResponse(ok: false, output: error.localizedDescription))
        }
    }

    private func stopListening() {
        request?.endAudio()
        if engine.isRunning { engine.stop() }
        engine.inputNode.removeTap(onBus: 0)
        DispatchQueue.main.asyncAfter(deadline: .now() + 1) { [weak self] in
            guard let self = self, !self.finished else { return }
            self.finish(self.best.isEmpty
                ? PluginResponse(ok: false, output: "cancelled")
                : PluginResponse(ok: true, output: self.best))
        }
    }

    private func finish(_ r: PluginResponse) {
        if finished { return }
        finished = true
        task?.cancel()
        request?.endAudio()
        if engine.isRunning { engine.stop() }
        engine.inputNode.removeTap(onBus: 0)
        try? AVAudioSession.sharedInstance().setActive(false, options: .notifyOthersOnDeactivation)
        onDone?(r)
    }
}
