package dev.mobiler.coffee

import kotlinx.coroutines.channels.Channel
import okhttp3.OkHttpClient
import okhttp3.Request
import okhttp3.Response
import okhttp3.WebSocket
import okhttp3.WebSocketListener

// WebSocket (free, bundled). A persistent connection bridged into the request/response ABI via
// four ops; the app pumps `recv` in a loop to stream incoming frames:
//   connect: input = the ws:// or wss:// URL           → ok:true once open (ok:false on failure)
//   send:    input = the text frame to send            → ok:true
//   recv:    input = ""  (suspends for the next frame) → ok:true, output = frame text;
//                                                          ok:false, output = "closed" when the
//                                                          socket closes (stop looping)
//   close:   input = ""                                → ok:true
// Backed by OkHttp's WebSocket (already a shell dependency). Incoming frames + close are queued in
// a Channel so `recv` never drops a message between calls. The plugin instance is a registry
// singleton, so it holds one connection for the app's lifetime.
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
