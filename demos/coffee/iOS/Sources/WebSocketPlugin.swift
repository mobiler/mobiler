import Foundation
import SharedTypes

// WebSocket (free, bundled). A persistent connection bridged into the request/response ABI via
// four ops; the app pumps `recv` in a loop to stream incoming frames:
//   connect: input = the ws:// or wss:// URL           → ok:true once open
//   send:    input = the text frame to send            → ok:true
//   recv:    input = ""  (suspends for the next frame) → ok:true, output = frame text;
//                                                          ok:false, output = "closed" on close
//   close:   input = ""                                → ok:true
// Backed by URLSessionWebSocketTask (system framework — no package). The Swift Plugins registry
// dispatches to `WebSocketPlugin.handle`, which forwards to a shared actor-isolated instance so
// the task/state survive across calls.
enum WebSocketPlugin {
    static func handle(op: String, input: String) async -> PluginResponse {
        await WebSocketConnection.shared.handle(op: op, input: input)
    }
}

// Holds the single connection. @MainActor-isolated for safe mutable state across the async ops.
@MainActor
private final class WebSocketConnection {
    static let shared = WebSocketConnection()
    private var task: URLSessionWebSocketTask?

    func handle(op: String, input: String) async -> PluginResponse {
        switch op {
        case "connect": return connect(input)
        case "send": return await send(input)
        case "recv": return await recv()
        case "close":
            task?.cancel(with: .goingAway, reason: nil)
            task = nil
            return PluginResponse(ok: true, output: "")
        default:
            return PluginResponse(ok: false, output: "unknown op '\(op)'")
        }
    }

    private func connect(_ urlString: String) -> PluginResponse {
        guard let url = URL(string: urlString) else {
            return PluginResponse(ok: false, output: "invalid url")
        }
        let t = URLSession.shared.webSocketTask(with: url)
        task = t
        t.resume()  // URLSessionWebSocketTask connects lazily; the first send/receive drives it.
        return PluginResponse(ok: true, output: "")
    }

    private func send(_ text: String) async -> PluginResponse {
        guard let task else { return PluginResponse(ok: false, output: "not connected") }
        do {
            try await task.send(.string(text))
            return PluginResponse(ok: true, output: "")
        } catch {
            return PluginResponse(ok: false, output: error.localizedDescription)
        }
    }

    private func recv() async -> PluginResponse {
        guard let task else { return PluginResponse(ok: false, output: "closed") }
        do {
            switch try await task.receive() {
            case .string(let s): return PluginResponse(ok: true, output: s)
            case .data(let d): return PluginResponse(ok: true, output: String(decoding: d, as: UTF8.self))
            @unknown default: return PluginResponse(ok: true, output: "")
            }
        } catch {
            // A receive error means the socket closed/failed → tell the app to stop its recv-loop.
            self.task = nil
            return PluginResponse(ok: false, output: "closed")
        }
    }
}
