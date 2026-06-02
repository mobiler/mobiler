import SharedTypes
import AVFoundation

/// Free bundled plugin: audio record + play.
/// - op "record", input = seconds (default 3) → records from the mic to an .m4a, returns its
///   file:// URL. Prompts for mic access (NSMicrophoneUsageDescription, added by `plugin add`).
/// - op "play", input = a URI → plays to completion, returns "done". No permission.
enum AudioPlugin {
    static func handle(op: String, input: String) async -> PluginResponse {
        switch op {
        case "record": return await record(seconds: Double(input) ?? 3)
        case "play": return await play(input)
        default: return PluginResponse(ok: false, output: "unknown op '\(op)'")
        }
    }

    private static func record(seconds: Double) async -> PluginResponse {
        let dur = min(max(seconds, 1), 60)
        let granted = await withCheckedContinuation { (cont: CheckedContinuation<Bool, Never>) in
            AVAudioSession.sharedInstance().requestRecordPermission { cont.resume(returning: $0) }
        }
        guard granted else { return PluginResponse(ok: false, output: "denied") }
        let url = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString + ".m4a")
        do {
            let session = AVAudioSession.sharedInstance()
            try session.setCategory(.playAndRecord, mode: .default)
            try session.setActive(true)
            let settings: [String: Any] = [
                AVFormatIDKey: Int(kAudioFormatMPEG4AAC),
                AVSampleRateKey: 44100.0,
                AVNumberOfChannelsKey: 1,
            ]
            let recorder = try AVAudioRecorder(url: url, settings: settings)
            guard recorder.record(forDuration: dur) else {
                return PluginResponse(ok: false, output: "could not start recording")
            }
            try await Task.sleep(nanoseconds: UInt64((dur + 0.3) * 1_000_000_000))
            recorder.stop()
            return PluginResponse(ok: true, output: url.absoluteString)
        } catch {
            return PluginResponse(ok: false, output: error.localizedDescription)
        }
    }

    private static func play(_ uri: String) async -> PluginResponse {
        guard let url = URL(string: uri) else { return PluginResponse(ok: false, output: "bad uri") }
        return await withCheckedContinuation { cont in
            do {
                let delegate = AudioPlayDelegate { cont.resume(returning: $0) }
                let player = try AVAudioPlayer(contentsOf: url)
                delegate.player = player // retain the player for the playback's lifetime
                AudioPlayDelegate.retained = delegate
                player.delegate = delegate
                guard player.play() else {
                    AudioPlayDelegate.retained = nil
                    cont.resume(returning: PluginResponse(ok: false, output: "could not start playback"))
                    return
                }
            } catch {
                cont.resume(returning: PluginResponse(ok: false, output: error.localizedDescription))
            }
        }
    }
}

private final class AudioPlayDelegate: NSObject, AVAudioPlayerDelegate {
    static var retained: AudioPlayDelegate?
    var player: AVAudioPlayer?
    private let onResult: (PluginResponse) -> Void
    private var done = false
    init(onResult: @escaping (PluginResponse) -> Void) { self.onResult = onResult }

    func audioPlayerDidFinishPlaying(_ player: AVAudioPlayer, successfully flag: Bool) {
        finish(PluginResponse(ok: flag, output: flag ? "done" : "play error"))
    }
    func audioPlayerDecodeErrorDidOccur(_ player: AVAudioPlayer, error: Error?) {
        finish(PluginResponse(ok: false, output: error?.localizedDescription ?? "decode error"))
    }
    private func finish(_ r: PluginResponse) {
        if done { return }
        done = true
        onResult(r)
        AudioPlayDelegate.retained = nil
    }
}
