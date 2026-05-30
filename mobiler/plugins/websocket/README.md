# websocket — persistent real-time connection (free, bundled)

```bash
mobiler plugin add websocket
```

A WebSocket bridged into the request/response ABI via four ops. The app pumps `recv` in a loop to
stream incoming frames.

```rust
cx.plugin("websocket", "connect", "wss://echo.websocket.org", Msg::WsOpen),
Msg::WsOpen(r) => if r.ok { cx.plugin("websocket", "recv", "", Msg::WsFrame) },  // start the loop
Msg::WsFrame(r) => {
    if r.ok { /* r.output = frame */ cx.plugin("websocket", "recv", "", Msg::WsFrame); } // re-issue
    else { /* r.output == "closed" → stop */ }
}
// cx.plugin("websocket", "send", "hello", …)   cx.plugin("websocket", "close", "", …)
```

- **Ops:** `connect` (input = ws/wss URL, resolves when open), `send` (input = text frame), `recv`
  (suspends for the next frame; `ok:false, output "closed"` when the socket closes — stop looping),
  `close`.
- **Streaming model:** there's no push in the ABI, so receiving is a **self-re-issuing `recv` loop**
  — each frame's handler kicks off the next `recv`. Frames arriving between calls are queued (Android
  Channel / URLSession buffering), so none are dropped.
- **Android:** OkHttp `WebSocket` (already a shell dependency — no extra Gradle dep).
- **iOS:** `URLSessionWebSocketTask` (system framework — no package). iOS 16 target ✓.
- **Web:** graceful `ok:false` (a browser `WebSocket`-based handler could be added later).
- One connection per app (the plugin instance is a registry singleton). Testable against any echo
  server (e.g. `wss://echo.websocket.org`) — no special hardware, unlike scanner/biometric.

See `app-core-usage.rs` for a full connect → recv-loop → send → close example.
