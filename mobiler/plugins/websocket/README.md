# websocket — persistent real-time connection, streaming (free, bundled)

```bash
mobiler plugin add websocket
```

A WebSocket that rides Mobiler's **streaming primitive** (`cx.subscribe`/`unsubscribe`): `subscribe`
opens the socket and pushes one event per incoming frame into `update` — no app-driven receive loop.

```rust
// Open + stream: one Msg::Frame per incoming frame, until close.
cx.subscribe("ws", "websocket", "stream", "wss://echo.websocket.org", Msg::Frame),
Msg::Frame(r) => {
    if r.ok { /* r.output = frame text */ }
    else    { /* r.output == "closed" → the socket dropped */ }
}
// Send while subscribed:           cx.plugin("websocket", "send", "hello", …)
// Stop streaming + close:          cx.unsubscribe("ws")
```

- **Streaming op:** `subscribe(key, "websocket", "stream", url, on_frame)` — opens the socket and
  emits a `PluginResponse` per frame (`ok:false, output "closed"` when it drops). `cx.unsubscribe(key)`
  closes it. Send with the `send` request/response op against the same socket.
- **Legacy ops** (back-compat): `connect`/`send`/`recv`/`close` still work (the pre-streaming
  self-re-issuing `recv` loop), but `subscribe` is the canonical path.
- **Android:** OkHttp `WebSocket` (already a shell dependency — no extra Gradle dep); the streaming
  side is a `callbackFlow` over the `WebSocketListener`, torn down on `awaitClose`.
- **iOS:** `URLSessionWebSocketTask` (system framework — no package); the stream loops `receive()`
  and is cancelled via `withTaskCancellationHandler` on unsubscribe. iOS 16 target ✓.
- **Web:** the shell opens a browser `WebSocket` and streams its `onmessage` frames (handled in the
  web shell's `start_stream`).
- One connection per app (the plugin instance is a registry singleton). Testable against any echo
  server (e.g. `wss://echo.websocket.org`) — no special hardware.

See `app-core-usage.rs` for a full subscribe → send → unsubscribe example.
