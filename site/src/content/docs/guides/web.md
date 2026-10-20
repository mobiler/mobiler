---
title: Add a web client
description: Run your app's Rust core in the browser with the mobiler-web shell and Trunk.
---

The web shell is a library, [`mobiler-web`](https://crates.io/crates/mobiler-web). It drives your core and
renders its widget tree to the DOM with Leptos, compiled to WebAssembly. Your web client is a small Trunk
project with one line of Rust.

## Move the app into its own crate

`mobiler new` puts the app in `shared/`, which also carries the native bindings. To share it with the web,
move the app (Model, Msg, `MobilerApp`, `App`) into a plain library crate, here `app-core`, and make
`shared` re-export it:

1. Move `shared/src/app.rs` to `app-core/src/lib.rs`, and give the crate a manifest:

   ```toml
   # app-core/Cargo.toml
   [package]
   name = "app-core"
   version = "0.1.0"
   edition.workspace = true

   [dependencies]
   crux_core.workspace = true
   serde = { workspace = true, features = ["derive"] }
   mobiler-core = "0.48"
   ```

2. Add it to the workspace in the root `Cargo.toml`: `members = ["shared", "app-core"]`.

3. Point `shared` at it. In `shared/Cargo.toml`, add `app-core = { path = "../app-core" }` under
   `[dependencies]`. In `shared/src/lib.rs`, replace `mod app;` and `pub use app::*;` with:

   ```rust
   pub use app_core::*;
   ```

The mobile app builds as before: `shared` still provides the bindings, now over `app-core`. Keep
`app-core`'s `mobiler-core` version equal to `shared`'s; `mobiler upgrade` moves the one in `shared` and
lists the other for you to change.

The [barbershop demo](/examples/#barbershop-fade-house) is laid out exactly like this.

## The web crate

Keep it in its own Cargo workspace (it builds for `wasm32-unknown-unknown`):

```toml
# web/Cargo.toml
[package]
name = "myapp-web"
version = "0.1.0"
edition = "2021"

[workspace]

[dependencies]
mobiler-web = "0.49"
app-core = { path = "../app-core" }
```

```rust
// web/src/main.rs
fn main() {
    mobiler_web::run::<app_core::App>();
}
```

```html
<!-- web/index.html -->
<!DOCTYPE html>
<html lang="en">
  <head>
    <meta charset="utf-8" />
    <meta name="viewport" content="width=device-width, initial-scale=1" />
    <title>My App</title>
    <link data-trunk rel="rust" />
  </head>
  <body></body>
</html>
```

Then `cd web && trunk serve`, and open the address it prints.

Two things to know about this layout:

- `mobiler watch` follows `shared/src`, not `app-core/src`. After moving the app, run `mobiler dev` again
  after a change, or keep `trunk serve` open for the web.
- `mobiler upgrade` (CLI 0.73) reports `mobiler-web 0.49 -> 0.48 (edit by hand)` for `web/Cargo.toml`.
  Ignore it: `mobiler-web` 0.49 is the current release and works with `mobiler-core` 0.48.

## What to know

- **No CSS needed.** The shell injects its own stylesheet, first in `<head>`, so your own stylesheet can
  override any widget class.
- **Every widget renders,** including charts (SVG), video, maps (MapLibre GL), PDF and web views.
- **Built-in capabilities work through browser APIs, with limits a page can't get past:** a copied text
  can't be marked sensitive or expire, and the clipboard needs a secure context (HTTPS or localhost);
  `cx.device_info` answers `None`; haptics do nothing where the browser has no vibration (iOS Safari);
  and `cx.now()` isn't answered by the web shell yet, so take the date from your server there.
- **Plugins:** `websocket` and `transfer` work on the web. Any other plugin request answers `ok: false`
  ("not available"), and a stream from one never sends an event; handle that in `update`.
- **Fonts and the launch screen:** when the app has `web/index.html`, `mobiler build` and `dev` also write
  the `[fonts]` and `[splash]` options there. A web-only workflow runs `mobiler fonts sync` and
  `mobiler splash sync` itself.
- **Tabs:** the web keeps each tab's scroll position and switches at once, like the native shells.

For a product with a server too, see [One core for mobile, web and server](/guides/full-stack/).
