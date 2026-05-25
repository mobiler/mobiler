# mobiler

> **React Native, but Rust + Compose.** A CLI for building mobile apps whose logic *and* UI are written in Rust and rendered to real native widgets.

**Status: early & experimental (v0.1). Android only. APIs will change.**

`mobiler` scaffolds and drives mobile apps built on [Crux](https://github.com/redbadger/crux): a Rust core owns all state, events, and business logic, and its view function returns a `Widget` tree that a thin, app-agnostic Jetpack Compose shell renders into native Material 3 widgets. Events flow back into the Rust core across the FFI (uniffi + bincode). You write the app once, in Rust; the native shell stays generic.

## Install

```bash
cargo install mobiler
```

You'll also need the Rust toolchain (with Android targets), the Android SDK/NDK, and an emulator or device. Run `mobiler doctor` to check your host.

## Usage

```bash
mobiler doctor          # check the host has everything needed
mobiler new myapp       # scaffold a new app
cd myapp
mobiler dev             # build the Rust core, generate Kotlin types,
                        # build the APK, install, and launch
mobiler watch           # same as dev, re-running on every change
mobiler add-widget Slider --field value:f32   # add a Widget variant + Compose arm
```

## Links

- Source, demos, and design notes: <https://github.com/mobiler/mobiler>

## License

Dual-licensed under either [MIT](https://github.com/mobiler/mobiler/blob/main/LICENSE-MIT) or [Apache-2.0](https://github.com/mobiler/mobiler/blob/main/LICENSE-APACHE), at your option.
