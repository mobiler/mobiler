# mobiler

> **Build mobile apps in Rust — the logic *and* the UI — rendered to real native widgets.**

**Status: experimental (v0.4.0).** Android (native Jetpack Compose) is the shipped
shell; the same core also renders on the web (Leptos/WASM) and an iOS shell is in
progress.

`mobiler` is the CLI that scaffolds and drives apps built on
[Crux](https://github.com/redbadger/crux): a Rust core owns all state and logic, and
its `view` returns a `Widget` tree that a thin, **app-agnostic** shell renders into
real native widgets. The shell is generic — built once from a fixed ABI and reused for
every app — so you write the app once, in Rust.

## Install

```bash
cargo install mobiler
```

You'll also need the Rust toolchain (with Android targets), the Android SDK/NDK, and an
emulator or device. Run `mobiler doctor` to check your host.

## Usage

```bash
mobiler doctor          # check the host has everything needed
mobiler new myapp       # scaffold a new app (Rust core + generic Android shell)
cd myapp
mobiler dev             # build core → generate types → build APK → install + launch
mobiler watch           # …same, re-running on every change
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

Device APIs (toast, storage, HTTP, …) are async **capabilities** via `cx`; navigation
is a core-owned `Nav` stack; dark mode and theming are data in the `Widget` tree. The
widget vocabulary and runtime live in the
[`mobiler-ui`](https://crates.io/crates/mobiler-ui) and
[`mobiler-core`](https://crates.io/crates/mobiler-core) crates.

## Links

- Source, demos, and guide: <https://github.com/mobiler/mobiler>

## License

Dual-licensed under either [MIT](https://github.com/mobiler/mobiler/blob/main/LICENSE-MIT) or [Apache-2.0](https://github.com/mobiler/mobiler/blob/main/LICENSE-APACHE), at your option.
