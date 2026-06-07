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

The `Widget` set spans layout, inputs, navigation (tabs/FAB/sheets, tablet-adaptive), media,
feedback, an inline calendar, and **data-viz charts** — eight `Chart` styles (bar, line, stacked,
100%-stacked, pie, donut, fitness-style progress rings, radial gauge) plus a variable-width
coverage-gap `RegionChart`.

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
mobiler upgrade         # 3-way merge; review results as *.mobiler-new
mobiler upgrade --apply # …or write the merged shells in place (a *.mobiler-bak is saved)
```

It does a **true 3-way merge**. `mobiler new` snapshots the pristine shells into `.mobiler/base/`
(the merge *ancestor*), so `upgrade` reconciles the ancestor, your current file, and the new
template per file — like `git merge`. Framework improvements apply **and** your edits + plugin
injections survive; only overlapping changes become a conflict (written as `<file>.mobiler-new`
with `<<<<<<<`/`>>>>>>>` markers, never auto-applied). It bumps your `mobiler-core` dependency and
never touches your Rust app code (`shared/src/`). By default a clean merge is offered as
`<file>.mobiler-new`; `--apply` writes it in place after saving a `.mobiler-bak`. Commit `.mobiler/`
(the baseline + version stamp). Apps scaffolded before baselines existed fall back to a conservative
reconcile and get a baseline for next time.

## Plugins

Advanced native capabilities install as **droppable plugins** — one command, no framework code or
ABI change. Bundled free plugins:

| Plugin | Capability |
|---|---|
| 🔎 `scanner` | barcode / QR scanning |
| 🔐 `biometric` | Face ID / fingerprint auth |
| 🗝️ `securestore` | encrypted key/value (Keychain / Keystore) |
| 🔌 `websocket` | persistent real-time connection (streaming, via `cx.subscribe`) |
| 🔔 `notifications` | local scheduled notifications (reminders) |
| 🔋 `battery` | device battery level (sample) |
| 📶 `connectivity` | network status (online/wifi/cellular/offline) |
| 📄 `filepicker` | system document picker → file URI |
| 📍 `geolocation` | device location → `lat,lng` |
| 📇 `contacts` | system contact picker → `name|phone` |
| 📅 `calendar` | add an event (system editor) |
| 🎙️ `audio` | record (mic) + play |
| 📐 `sensors` | accelerometer / gyroscope → `x,y,z` |
| ✉️ `composer` | email / SMS / phone call via the system apps |
| 🗣️ `tts` | text-to-speech (speak a string aloud) |
| ⭐ `review` | in-app App Store / Play review prompt |
| 📤 `sharefile` | share a file / image via the share sheet |
| 🎬 `video` | record a video → local URI |
| 🎤 `speech` | speech-to-text (dictation) |
| 🗃️ `sqlite` | on-device SQLite (exec / query → JSON) |
| 🔵 `bluetooth` | BLE scan / connect / read |
| 🔑 `oauth` | OAuth 2.0 / OIDC login (system auth browser → redirect) |
| 📲 `push` | remote push notifications (APNs / FCM) — **experimental, not yet device-tested** |
| 💳 `iap` | in-app purchase / subscriptions (StoreKit 2 / Play Billing) — **Android experimental** |

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
HTTP, storage, clipboard, share, browser, toast, device info, haptics, a confirm dialog, the photo picker, camera capture, the date picker, and the time picker.
<!-- capabilities:end -->

Navigation is a core-owned `Nav` stack; dark mode and theming are data in
the `Widget` tree. A `Scaffold` can carry a `Theme` — brand color, corner
radius, density, and font — that every native shell (iOS, Android, web)
applies; `theme: None` keeps the default look. The widget vocabulary and runtime live in the
[`mobiler-ui`](https://crates.io/crates/mobiler-ui) and
[`mobiler-core`](https://crates.io/crates/mobiler-core) crates.

## Roadmap

Mobiler is production-bound. Beyond today's capabilities + plugins, **planned** (order is
demand-driven): video playback/streaming
(HLS + MP4, any URL source), tablet master-detail,
deep links, embedded WebView, and maps. The full list lives on
[GitHub](https://github.com/mobiler/mobiler#roadmap). _Recently shipped: rich form fields, the
`oauth` plugin (OAuth/OIDC login), locale-aware number/currency/date formatting
(`mobiler_core::format`), a device-locale getter (`cx.device_locale`), an in-app PDF viewer
(`PdfView`), a native→core streaming primitive (`cx.subscribe`/`unsubscribe`, with a built-in
`ticker`), a paged feed list (`LazyList` — infinite scroll + pull-to-refresh), remote push
(`push` plugin — APNs/FCM; **experimental**), and in-app purchases (`iap` plugin — StoreKit 2 / Play
Billing: products, purchase, restore, a transactions stream; **Android experimental**). Next up: a
Firebase-on-iOS push variant (`push-fcm`)._

## Links

- Source, demos, and guide: <https://github.com/mobiler/mobiler>

## License

Dual-licensed under either [MIT](https://github.com/mobiler/mobiler/blob/main/LICENSE-MIT) or [Apache-2.0](https://github.com/mobiler/mobiler/blob/main/LICENSE-APACHE), at your option.
