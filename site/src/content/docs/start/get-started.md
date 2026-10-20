---
title: Get started
description: Install the mobiler CLI, create an app, and run it on Android, iOS and the web.
---

## What you need

- **Rust** through rustup. The scaffold pins its toolchain and Android targets in `rust-toolchain.toml`.
- **For Android:** the Android SDK and NDK, a JDK, and an emulator or a phone with USB debugging.
- **For iOS:** a Mac with Xcode and [XcodeGen](https://github.com/yonaskolb/XcodeGen). The simulator needs no
  Apple account or signing.
- **For the web:** [Trunk](https://trunkrs.dev) and the `wasm32-unknown-unknown` target.

`mobiler doctor` checks your machine and says what is missing.

## Create an app

```sh
cargo install mobiler
mobiler doctor
mobiler new myapp --package com.example.myapp --display-name "My App"
cd myapp
mobiler dev
```

`mobiler dev` builds the Rust core, generates the shell bindings, builds the Android app, installs it and
launches it. With several devices connected, pick one with `--device <serial>`. `mobiler watch` does the same
on every change.

On a Mac, `mobiler build ios` builds the iOS app (or run `bash iOS/build-ios.sh` for the simulator).

## What you got

```
myapp/
  shared/src/app.rs   your app: Model, Msg, update, view. Start here.
  shared/             the Rust crate the shells load (bindings, codegen)
  Android/            the generic Android shell (Kotlin, Jetpack Compose)
  iOS/                the generic iOS shell (Swift, SwiftUI; project.yml for XcodeGen)
  .mobiler/           the CLI's baseline for upgrades: commit it
  Cargo.toml          the workspace
```

Edit `shared/src/app.rs` and run `mobiler dev` again. You shouldn't need to open `Android/` or `iOS/`: they
render whatever your core returns. Icons, the display name and store signing are the per-platform parts you
set yourself.

Options that change the shells (fonts, the launch screen, deep links, local network on iOS) go in a
`mobiler.toml` at the app root. The build applies them. See [mobiler.toml options](/guides/configuration/).

## Run it on the web

The same core runs in a browser with the `mobiler-web` shell. See [Add a web client](/guides/web/).

## Working with a coding agent

`mobiler new myapp --agentic` adds a `CLAUDE.md` guide to the app, so an agent such as Claude Code writes
idiomatic Mobiler. `--agentic shared-ui` adds guidance for the same UI on mobile and web, and `--agentic api`
for a reusable core with a JSON API backend.

## Next

- Learn the shape of a real app in the [Saldo tutorial](/tutorial/).
- Read real code in the [demo apps](/examples/).
- Add device features: [capabilities and plugins](/guides/capabilities/).
