# mobiler

> **Build mobile apps in Rust — the logic *and* the UI — once, rendered to real native widgets on Android, iOS, and the web.**

**Status: experimental.** One Rust core drives three generic shells — Android
(Jetpack Compose / Material 3), iOS (SwiftUI), and the web (Leptos/WASM) — with no
per-app native code.

<img src="https://raw.githubusercontent.com/mobiler/mobiler/main/demos/coffee/screenshots/parity.png" alt="The same coffee storefront on Android, iOS, and the web" width="780">

*The `coffee` demo: one Rust core, the same `Widget` tree, rendered by the stock
Android, iOS, and web shells — no per-platform UI code.*

`mobiler` is the CLI that scaffolds and drives apps built on
[Crux](https://github.com/redbadger/crux): a Rust core owns all state and logic, and
its `view` returns a `Widget` tree that thin, **app-agnostic** shells render into real
native widgets. Each shell is generic — built once from a fixed wire ABI and reused for
every app — so you write the app once, in Rust, and it runs natively everywhere.

## Install

```bash
cargo install mobiler
```

Run `mobiler doctor` to check your host. You'll want the Rust toolchain, the Android
SDK/NDK, and an emulator or device; iOS builds need a Mac with Xcode.

## Usage

```bash
mobiler doctor          # check the host has everything needed
mobiler new myapp       # scaffold a new app (Rust core + generic Android & iOS shells)
cd myapp
mobiler dev             # build core → generate types → build APK → install + launch
mobiler watch           # …same, re-running on every change
mobiler build ios       # build the iOS app (on a Mac)
```

Your app lives in `shared/src/app.rs` as a `MobilerApp` — typed `Msg` events, a
`Model`, and a `view` built from widget builders:

```rust
fn view(&self, model: &Model) -> Widget {
    column(vec![
        title("Counter"),
        text(format!("count: {}", model.count)),
        button("Increment", ButtonStyle::Filled, Msg::Increment),
    ])
}
```

The same core also runs on the web with the
[`mobiler-web`](https://crates.io/crates/mobiler-web) shell — one line plus
[Trunk](https://trunkrs.dev).

Device APIs are async **capabilities** via `cx`, fulfilled by the generic shell on
every platform — adding one is a shell-registry entry, never an ABI change. Built in:

<!-- capabilities:start format=inline (generated from capabilities.json — run `cargo run -p xtask -- gen-readme`) -->
HTTP, storage, clipboard, share, browser, toast, device info, haptics, a confirm dialog, the photo picker, and camera capture.
<!-- capabilities:end -->

Navigation is a core-owned `Nav` stack; dark mode and theming are data in
the `Widget` tree. The widget vocabulary and runtime live in the
[`mobiler-ui`](https://crates.io/crates/mobiler-ui) and
[`mobiler-core`](https://crates.io/crates/mobiler-core) crates.

## Links

- Source, demos, and guide: <https://github.com/mobiler/mobiler>

## License

Dual-licensed under either [MIT](https://github.com/mobiler/mobiler/blob/main/LICENSE-MIT) or [Apache-2.0](https://github.com/mobiler/mobiler/blob/main/LICENSE-APACHE), at your option.
