package dev.mobiler.barbershop

import dev.mobiler.barbershop.shared.types.PluginResponse

import kotlinx.coroutines.channels.Channel
import kotlinx.coroutines.channels.awaitClose
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.callbackFlow
import okhttp3.OkHttpClient
import okhttp3.Request
import okhttp3.Response
import okhttp3.WebSocket
import okhttp3.WebSocketListener

// WebSocket (free, bundled). Backed by OkHttp's WebSocket (already a shell dependency). Canonical
// usage rides the streaming primitive:
//   cx.subscribe("ws", "websocket", "stream", url, |r| Msg::Frame(r))  // opens + streams frames
//   cx.plugin("websocket", "send", "hello", ...)                       // send while subscribed
//   cx.unsubscribe("ws")                                               // closes the socket
// `subscribe` opens the socket and emits a PluginResponse per incoming frame (ok:false, "closed" on
// close) until the collecting Job is cancelled (awaitClose tears the socket down). The legacy
// request/response ops (connect/send/recv/close, with a Channel buffer) remain for back-compat.
// The plugin instance is a registry singleton, so it holds one connection for the app's lifetime.
class WebSocketPlugin(private val application: android.app.Application) : MobilerPlugin {
    private val client = OkHttpClient()
    private var socket: WebSocket? = null

    // Unlimited buffer so frames arriving between `recv` calls are never lost; a successful
    // close sends a sentinel so the app's recv-loop can terminate cleanly.
    private val incoming = Channel<Pair<Boolean, String>>(Channel.UNLIMITED)

    override suspend fun handle(op: String, input: String): PluginResponse = when (op) {
        "connect" -> connect(input)   // suspends until the socket opens or fails
        "send" -> {
            socket?.send(input)
            if (socket != null) PluginResponse(true, "") else PluginResponse(false, "not connected")
        }
        "recv" -> {
            val (ok, text) = incoming.receive()  // suspends until a frame or close arrives
            PluginResponse(ok, text)
        }
        "close" -> {
            socket?.close(1000, null)
            socket = null
            PluginResponse(true, "")
        }
        else -> PluginResponse(false, "unknown op '$op'")
    }

    // Streaming entrypoint (cx.subscribe): open the socket to `input` and emit a PluginResponse per
    // incoming frame until the collecting Job is cancelled (cx.unsubscribe → awaitClose closes it).
    // Sets `socket` so the `send` op works against the same connection while streaming.
    override fun subscribe(op: String, input: String): Flow<PluginResponse> = callbackFlow {
        val request = Request.Builder().url(input).build()
        val ws = client.newWebSocket(request, object : WebSocketListener() {
            override fun onMessage(webSocket: WebSocket, text: String) {
                trySend(PluginResponse(true, text))
            }
            override fun onClosing(webSocket: WebSocket, code: Int, reason: String) {
                trySend(PluginResponse(false, "closed")); close()
            }
            override fun onFailure(webSocket: WebSocket, t: Throwable, response: Response?) {
                trySend(PluginResponse(false, t.message ?: "closed")); close()
            }
        })
        socket = ws
        awaitClose {
            ws.close(1000, null)
            if (socket === ws) socket = null
        }
    }

    private suspend fun connect(url: String): PluginResponse {
        val opened = kotlinx.coroutines.CompletableDeferred<PluginResponse>()
        val request = Request.Builder().url(url).build()
        socket = client.newWebSocket(request, object : WebSocketListener() {
            override fun onOpen(webSocket: WebSocket, response: Response) {
                if (!opened.isCompleted) opened.complete(PluginResponse(true, ""))
            }
            override fun onMessage(webSocket: WebSocket, text: String) {
                incoming.trySend(true to text)
            }
            override fun onClosing(webSocket: WebSocket, code: Int, reason: String) {
                incoming.trySend(false to "closed")
            }
            override fun onFailure(webSocket: WebSocket, t: Throwable, response: Response?) {
                if (!opened.isCompleted) opened.complete(PluginResponse(false, t.message ?: "connect failed"))
                incoming.trySend(false to "closed")
            }
        })
        // handle() is a suspend fun, so we can await the open/fail callback directly.
        return opened.await()
    }
}
