# Mobiler

[![CI](https://github.com/mobiler/mobiler/actions/workflows/ci.yml/badge.svg)](https://github.com/mobiler/mobiler/actions/workflows/ci.yml)
[![crates.io](https://img.shields.io/crates/v/mobiler.svg)](https://crates.io/crates/mobiler)

> **Build mobile apps in Rust — the logic *and* the UI — rendered to real native widgets.**

**Status: experimental (v0.4.0).** Android (native Jetpack Compose) is the shipped
shell; the *same* app core also renders on the **web** (Leptos/WASM — see the
full-stack demo), and an **iOS** (SwiftUI) shell is in progress. APIs may still change.

## What it is

Mobiler builds on [Crux](https://github.com/redbadger/crux): a Rust core owns all
state, events, and business logic — none of it in the native layer. On top of that,
a Mobiler core's `view` returns a fixed **`Widget` tree**, and a thin, **app-agnostic
shell** renders that tree to real platform widgets. Events flow back into the core as
typed messages.

The shell is **generic**: it's built once from a fixed wire ABI and renders *any*
Mobiler app — no per-app native code, no per-app UI codegen. That's the whole idea:

- **Android** → Jetpack Compose → Material 3 (shipped)
- **Web** → DOM via Leptos/WASM (the `mobiler-web` shell — demonstrated in `demos/fullstack-todo`)
- **iOS** → SwiftUI (in progress)

…all driven by the same Rust core.

## What a Mobiler app looks like

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
            // Device APIs are capabilities (here, the native "toast" plugin).
            Msg::Greet => cx.notify("toast", "show", "Hello from Rust!"),
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

/// The shell renders this — `Event`/`ViewModel` are the fixed Mobiler ABI, so the
/// native shell stays generic and is built once for every app.
pub type App = MobilerShell<Counter>;
```

You write typed `Msg` events, a `Model`, and a `view` built from widget builders.
Mobiler serializes events into opaque tokens behind the scenes; the shell never sees
your app's types.

## Key ideas

- **Generic shell** — one prebuilt shell renders any app; adding a platform = writing
  one shell, not one-per-app.
- **Capabilities = plugins** — device APIs (toast, storage, **HTTP**, …) are async
  effects fulfilled by the shell's plugin registry. Unknown plugin → graceful "not
  available in this build", which is the line between the free generic shell and
  custom/cloud builds.
- **Navigation** — a core-owned `Nav` stack drives animated push/pop and the system
  back button.
- **Theme-as-data** — e.g. dark mode is a value in the `Widget` tree; the shell themes
  the whole app from it.

## Repository layout

| Path | What |
|------|------|
| `mobiler/` | The `mobiler` CLI (crate + embedded `templates/` scaffold) |
| `mobiler-ui/` | The fixed UI wire ABI — app-agnostic `Widget` tree + `Action` protocol |
| `mobiler-core/` | The runtime — `MobilerApp` trait, Crux shell adapter, typed widget builders, capabilities |
| `demos/todo/` | Todo / projects showcase (lists, cards, nav, per-project colors, dark mode) |
| `demos/coffee/` | Coffee-shop storefront (network images, hero overlay, product grid) |
| `demos/fullstack-todo/` | One Axum server + shared `domain`, rendered native **and** on web |

> **Monorepo** for now. Each demo under `demos/` is a self-contained project (its own
> workspace, like `mobiler new` produces) — so any can be extracted to its own repo.

## Demos

The full-stack demo is the clearest illustration — the **same Rust core** rendered
natively on Android and as a web app, both backed by one server:

| Android (Compose) | Web (Widget→DOM) |
|:---:|:---:|
| <img src="demos/fullstack-todo/screenshots/mobile.png" width="200" alt="native"> | <img src="demos/fullstack-todo/screenshots/web.png" width="280" alt="web"> |

See [`demos/fullstack-todo`](demos/fullstack-todo/), [`demos/todo`](demos/todo/), and
[`demos/coffee`](demos/coffee/).

## Quick start

You'll need the Rust toolchain (with Android targets), the Android SDK/NDK, and an
emulator or device.

```bash
cargo install mobiler        # or, from this repo: cargo build -p mobiler
mobiler doctor               # check your host has everything
mobiler new myapp            # scaffold an app (Rust core + generic Android shell)
cd myapp
mobiler dev                  # build core → generate types → build APK → install + launch
#   mobiler watch            # …and rebuild on every change
```

Edit `shared/src/app.rs` (your `MobilerApp`) and re-run `mobiler dev`.

## License

Dual-licensed under either of [MIT](LICENSE-MIT) or [Apache-2.0](LICENSE-APACHE), at
your option.
