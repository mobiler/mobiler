---
title: How it works
description: The core, the widget tree, the generic shells and capabilities, and how a tap travels through them.
---

Mobiler builds on [Crux](https://github.com/redbadger/crux): a Rust core owns all state and logic, and the
platform layer only draws and reports. Mobiler adds a fixed **widget vocabulary** on top, so the platform
layer can be one generic shell instead of hand-written UI per app.

## An app

```rust
use mobiler_core::*;
use serde::{Deserialize, Serialize};

#[derive(Default)]
struct Counter;

#[derive(Serialize, Deserialize, Clone)]
enum Msg { Increment, Greet }

#[derive(Default)]
struct Model { count: i32 }

impl MobilerApp for Counter {
    type Event = Msg;
    type Model = Model;

    fn update(&self, msg: Msg, model: &mut Model, cx: &mut Cx<Msg>) {
        match msg {
            Msg::Increment => model.count += 1,
            // Device features are capabilities, here the built-in toast.
            Msg::Greet => cx.toast("Hello from Rust!"),
        }
    }

    fn view(&self, model: &Model) -> Widget {
        column(vec![
            title("Counter"),
            text(format!("count: {}", model.count)),
            row(vec![
                button("Increment", ButtonStyle::Filled, Msg::Increment),
                button("Toast", ButtonStyle::Outlined, Msg::Greet),
            ]),
        ])
    }
}

/// What the shells run.
pub type App = MobilerShell<Counter>;
```

That is the whole app on all three platforms. `MobilerApp` has a few more methods with defaults: `init`
runs once at launch, `input` receives text-field and toggle values, and `restore` reloads state saved with
`cx.save`.

## A tap, step by step

1. **The core renders.** `view(model)` returns a `Widget` tree: a column, a title, two buttons. Each
   button carries its `Msg`, serialized into an opaque token.
2. **The shell draws it.** On Android the tree becomes Compose, on iOS SwiftUI, on the web DOM elements.
   The shell knows widgets, never your types.
3. **The user taps.** The shell sends the button's token back. The core turns it into `Msg::Increment`
   and calls `update`.
4. **The core may ask for something.** `cx.toast(…)`, `cx.get(url, …)` or `cx.plugin("sqlite", …)` don't
   run anything in Rust: they become requests the shell fulfils with the platform's own API. The answer
   comes back as one of your messages.
5. **The core renders again**, and the shell draws the new tree.

Because the core never touches the platform, it is a pure function you can test with `cargo test`:
send messages, check the model, check the returned widget tree.

## The shells

The shells are generic programs over a fixed wire format, built once for every app:

- **Android:** Kotlin and Jetpack Compose, Material 3. Android 8.0 (API 26) and newer.
- **iOS:** Swift and SwiftUI.
- **Web:** the `mobiler-web` crate, Leptos compiled to WebAssembly.

`mobiler new` writes the Android and iOS shells into your project, and `mobiler upgrade` brings each new
release's improvements in with a three-way merge, keeping your edits. Your app's own code is the Rust in
`shared/`, plus configuration such as icons and `mobiler.toml`.

## Capabilities and plugins

Device features are **capabilities**: a call on `cx` that becomes a request to the shell.

- **Built in** to every app: HTTP, storage, clipboard, share, the browser, toasts and snackbars, device
  and app info, the system light/dark setting, haptics, confirm dialogs, the photo picker, the camera, and
  date and time pickers. See [Built-in capabilities](/reference/capabilities/).
- **Plugins** add more: `mobiler plugin add sqlite` copies the plugin's Kotlin and Swift into your shells
  and registers it. See [Plugins](/reference/plugins/).

Adding a capability never changes the wire format, and a shell that doesn't know a plugin answers "not
available" instead of crashing, so an app degrades gracefully.

## Navigation, theme and state

- **Navigation** is a `Nav` stack in your model. Push and pop are model changes; the shells animate them
  and wire up the system back button. On Android and iOS a visited tab keeps its screen; the web keeps its scroll position.
- **Theme** is data: a `Theme` on the scaffold sets colours (a seed, or a full light and dark palette),
  corners, fonts and density, and every shell applies it.
- **State** is yours. `cx.save` stores it and `restore` reads it back at launch; apps with more data use the
  `sqlite` plugin or a server.

Every decision behind this design is recorded with its reasons in the
[architecture decision records](/reference/decisions/).
