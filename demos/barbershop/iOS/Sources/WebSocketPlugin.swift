import Foundation
import SharedTypes

// WebSocket (free, bundled). A persistent connection backed by URLSessionWebSocketTask (system
// framework — no package). Canonical usage rides the streaming primitive:
//   cx.subscribe("ws", "websocket", "stream", url, |r| Msg::Frame(r))  // opens + streams frames
//   cx.plugin("websocket", "send", "hello", ...)                       // send while subscribed
//   cx.unsubscribe("ws")                                               // closes the socket
// `stream` (op) opens the socket and emits a PluginResponse per incoming frame until the
// subscription is cancelled (ok:false, "closed" on close). The legacy request/response ops
// (connect/send/recv/close) remain for back-compat. The Plugins registry dispatches `subscribe`
// (streaming) + `handle` (request/response) to a shared actor so the socket survives across calls.
enum WebSocketPlugin {
    static func handle(op: String, input: String) async -> PluginResponse {
        await WebSocketConnection.shared.handle(op: op, input: input)
    }

    // Streaming entrypoint (cx.subscribe): `input` is the ws:// / wss:// URL. Emits each incoming
    // frame; runs until the subscription's Task is cancelled (cx.unsubscribe).
    static func subscribe(op: String, input: String, emit: @escaping @Sendable (PluginResponse) -> Void) async {
        await WebSocketConnection.shared.stream(url: input, emit: emit)
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

    // Open the socket and emit one PluginResponse per incoming frame until the enclosing Task is
    // cancelled (unsubscribe). The cancellation handler cancels the URLSession task, which makes the
    // in-flight `receive()` throw and ends the loop. `send` works against the same `task` meanwhile.
    func stream(url urlString: String, emit: @Sendable (PluginResponse) -> Void) async {
        guard let url = URL(string: urlString) else {
            emit(PluginResponse(ok: false, output: "invalid url"))
            return
        }
        let t = URLSession.shared.webSocketTask(with: url)
        task = t
        t.resume()
        await withTaskCancellationHandler {
            while !Task.isCancelled {
                do {
                    switch try await t.receive() {
                    case .string(let s): emit(PluginResponse(ok: true, output: s))
                    case .data(let d): emit(PluginResponse(ok: true, output: String(decoding: d, as: UTF8.self)))
                    @unknown default: break
                    }
                } catch {
                    emit(PluginResponse(ok: false, output: "closed"))
                    break
                }
            }
        } onCancel: {
            t.cancel(with: .goingAway, reason: nil)
        }
        if task === t { task = nil }
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
