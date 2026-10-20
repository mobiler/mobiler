---
title: Capabilities and plugins
description: Call device features from your Rust core, add plugins, and handle the answers as messages.
---

Your core never calls a platform API directly. It asks the shell through `cx`, and the answer comes back
as one of your messages. That keeps the core pure and testable, and lets each platform fulfil the request
its own way.

## Built-in capabilities

Every app made by `mobiler new` has them, on Android, iOS and the web:

```rust
fn update(&self, msg: Msg, model: &mut Model, cx: &mut Cx<Msg>) {
    match msg {
        Msg::Saved => cx.toast("Saved"),
        Msg::Share => cx.share(model.link.clone()),
        Msg::Refresh => cx.get(model.url.clone(), |resp| Msg::Fetched(resp.text().unwrap_or_default().to_string())),
        Msg::Fetched(body) => model.body = body,
        // …
    }
}
```

Some, like the toast, are fire-and-forget. Others take a closure that turns the platform's answer into a
message for your `update`. The full list, with each platform's limits, is in
[Built-in capabilities](/reference/capabilities/).

## Plugins

A plugin is a native feature packaged for Mobiler: a Kotlin half, a Swift half, and what they need in the
manifest and `Info.plist`. Add it from the app root:

```sh
mobiler plugin list          # what's available
mobiler plugin add sqlite    # copy it into your shells and register it
```

Then call it by name, with an operation and a text input:

```rust
cx.plugin("sqlite", "exec", "CREATE TABLE IF NOT EXISTS note (id INTEGER PRIMARY KEY, body TEXT)", |r| {
    Msg::Ready(r.ok)
});
cx.plugin("sqlite", "query", "SELECT id, body FROM note", |r| {
    Msg::Notes(r.as_text().unwrap_or_default().to_string())
});
```

Every answer is a `PluginResponse`: `ok`, plus a text or bytes payload. Each plugin's folder, linked from
[Plugins](/reference/plugins/), documents its operations and the shape of their input and output: in its
README where it has one, otherwise at the top of its Kotlin and Swift sources.

**Streams.** Plugins that produce many events (a WebSocket, sensor readings, a download's progress) use
`cx.subscribe(key, plugin, op, input, on_event)`, which calls `on_event` for every event until
`cx.unsubscribe(key)`.

**Where plugins run.** Plugins run in the Android and iOS apps. The web shell doesn't run them yet: a call
answers `ok: false` with "not available", so handle that case if your core also runs in a browser.

**Updates.** A plugin's native code is copied into your app, so a new CLI release doesn't change it by
itself. `mobiler upgrade` tells you when a plugin you use has a newer version, and running
`mobiler plugin add <name>` again brings it in.

## Testing code that uses capabilities

Because a capability is a request rather than a call, a test can drive `update`, look at what the core
asked for, and feed back the answer it wants, with no device involved.
