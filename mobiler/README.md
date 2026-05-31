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
mobiler plugin list     # list the bundled capability plugins
mobiler plugin add scanner   # install a plugin into the app
mobiler upgrade         # pull the latest generic shells + mobiler-core into an existing app
```

## Upgrading an existing app

New framework versions improve the generic native shells (the `Widget`-tree interpreter on
each platform). Because those shells are scaffolded into your project, `mobiler upgrade` pulls
the updates in for you, from the app root:

```bash
cargo install mobiler   # get the new CLI first
cd myapp
mobiler upgrade         # bump mobiler-core + write changed shells as *.mobiler-new (review)
mobiler upgrade --apply # …or overwrite the shells in place (a *.mobiler-bak is saved)
```

It's **non-destructive by default**: it bumps your `mobiler-core` dependency, writes any changed
generic shell file beside the original as `<file>.mobiler-new` for you to review/merge, and never
touches your Rust app code (`shared/src/`) or plugin-patched files (`Core.kt`, manifests — these
are offered as `.mobiler-new` to merge by hand, never overwritten). `--apply` overwrites the
generic shells in place after saving a `.mobiler-bak` of each.

## Plugins

Advanced native capabilities install as **droppable plugins** — one command, no framework code or
ABI change. Bundled free plugins:

| Plugin | Capability |
|---|---|
| 🔎 `scanner` | barcode / QR scanning |
| 🔐 `biometric` | Face ID / fingerprint auth |
| 🗝️ `securestore` | encrypted key/value (Keychain / Keystore) |
| 🔌 `websocket` | persistent real-time connection |
| 🔔 `notifications` | local scheduled notifications (reminders) |
| 🔋 `battery` | device battery level (sample) |

```bash
mobiler plugin list
mobiler plugin add scanner
```

`mobiler plugin add` also takes a local package directory (`mobiler-plugin.toml` + native sources),
which is how commercial/licensed plugins ship. Call one from Rust via `cx.plugin("<name>", "<op>",
input, then)`.

### Agent-ready scaffolds: `--agentic`

`mobiler new --agentic [<flavor>]` also writes a `CLAUDE.md` into the project so a coding
agent (e.g. Claude Code) builds idiomatically against Mobiler — it captures the `MobilerApp`
model, the widget-builder vocabulary, the capabilities, and the conventions. An optional
flavor tailors the guide to your architecture:

| Command | The `CLAUDE.md` describes… |
|---|---|
| `mobiler new app --agentic` | a **mobile** app, backend-agnostic — talk to any HTTP API via `cx`, or store data on-device (no web, no assumed server) |
| `… --agentic shared-ui` | the above **plus the same UI on web** (one core rendered on mobile **and** web) |
| `… --agentic api` | the above **plus a reusable core + JSON API** backend (Axum/SQLx; optional separate web UI) |

Without `--agentic`, no `CLAUDE.md` is written.

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
the `Widget` tree. A `Scaffold` can carry a `Theme` — brand color, corner
radius, density, and font — that every native shell (iOS, Android, web)
applies; `theme: None` keeps the default look. The widget vocabulary and runtime live in the
[`mobiler-ui`](https://crates.io/crates/mobiler-ui) and
[`mobiler-core`](https://crates.io/crates/mobiler-core) crates.

## Links

- Source, demos, and guide: <https://github.com/mobiler/mobiler>

## License

Dual-licensed under either [MIT](https://github.com/mobiler/mobiler/blob/main/LICENSE-MIT) or [Apache-2.0](https://github.com/mobiler/mobiler/blob/main/LICENSE-APACHE), at your option.
