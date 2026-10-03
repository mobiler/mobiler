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

The `Widget` set spans layout (incl. paged lazy lists), inputs, navigation (tabs/FAB/sheets,
tablet-adaptive **master-detail**), media — images, a controllable **video player**, an embedded
**web view**, an in-app **PDF viewer**, and an interactive **map** (MapKit / MapLibre, no API key) —
feedback, an inline calendar, **data-viz charts** (eight `Chart` styles: bar, line, stacked,
100%-stacked, pie, donut, fitness-style progress rings, radial gauge — plus a variable-width
coverage-gap `RegionChart`), and an **accessibility** wrapper (screen-reader label/hint/role on any widget).

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
mobiler dev --device <serial>   # pick a device when several are connected (or set ANDROID_SERIAL)
mobiler watch           # …same, re-running on every change
mobiler build ios       # build the iOS app (on a Mac)
mobiler plugin list     # list the bundled capability plugins
mobiler plugin add scanner   # install a plugin into the app
mobiler upgrade         # pull the latest generic shells + mobiler-core into an existing app
mobiler display-name "Appointments Admin"   # set the name users see (omit the name to print it)
mobiler fonts sync      # copy mobiler.toml [fonts] into the shells (build/dev/watch do it too)
```

**App display name.** `mobiler new myapp` derives the identifier `Myapp` (Gradle root, Xcode target,
theme) and uses it as the visible name too. `mobiler new myapp --display-name "My App"` or, later,
`mobiler display-name "My App"` sets only what users see: Android `app_name` (the launcher label and
system dialogs such as the notification-permission prompt) and iOS `CFBundleDisplayName` (in
`iOS/project.yml`, which XcodeGen turns into the Info.plist). Identifiers stay unchanged, and
`mobiler upgrade` keeps the value.

**Custom fonts — `mobiler.toml` `[fonts]`.** List a display family (titles, subtitles, the top-bar and
sheet titles) and a body family (everything else) as TrueType/OpenType files, and set
`Theme { font: FontFamily::Custom, .. }`:

```toml
# mobiler.toml (app root)
[fonts]
display = { family = "Space Grotesk", files = ["assets/fonts/SpaceGrotesk-Regular.ttf", "assets/fonts/SpaceGrotesk-SemiBold.ttf"] }
body    = { family = "Roboto", files = ["assets/fonts/Roboto-Regular.ttf", "assets/fonts/Roboto-Medium.ttf"] }
```

`mobiler build`, `dev` and `watch` sync them first (or run `mobiler fonts sync`): the files are copied
into Android `res/font/`, iOS `Sources/Fonts/` (with `UIAppFonts` and the family names in
`iOS/project.yml`) and, if the app has `web/index.html`, `web/fonts/` with generated `@font-face` rules.
Commit the copies so plain Gradle/Xcode/trunk builds work too. Each file's weight and family come from
the font itself. A missing, unreadable or woff/woff2 file is skipped with a warning (the build never
fails), and a role without usable files falls back to the system font. Text-size accessibility scaling
still applies.

## Upgrading an existing app

New framework versions improve the generic native shells (the `Widget`-tree interpreter on
each platform). Because those shells are scaffolded into your project, `mobiler upgrade` pulls
the updates in for you, from the app root:

```bash
cargo install mobiler   # get the new CLI first
cd myapp
mobiler upgrade         # 3-way merge; review results as *.mobiler-new
mobiler upgrade --apply # …or write the merged shells in place (old versions go to .mobiler/backup/)
mobiler upgrade --resolved path/to/File.kt  # a conflict you resolved, or an offer you decline
```

It does a **true 3-way merge**. `mobiler new` snapshots the pristine shells into `.mobiler/base/`
(the merge *ancestor*), so `upgrade` reconciles the ancestor, your current file, and the new
template per file — like `git merge`. Framework improvements apply **and** your edits + plugin
injections survive; only overlapping changes become a conflict (written as `<file>.mobiler-new`
with `<<<<<<<`/`>>>>>>>` markers, never auto-applied). It never touches your Rust app code (`shared/src/`).

Without `--apply`, nothing your build reads changes, apart from creating missing app-owned seed files. Clean
merges, new files and conflicts are offered as `<file>.mobiler-new`, and the `mobiler-core` bump is reported, not
made. `--apply` writes clean merges and new files
in place, after saving the old file under `.mobiler/backup/`, and bumps `mobiler-core`. It never applies a conflict.

Everything left for review is recorded in `.mobiler/pending/`. Every `mobiler upgrade` ends with a warning listing
those files and what to do with each. A review copy you put in place as it is gets
confirmed by the next run (`✓ resolved`). A conflict is settled only when you say so: resolve its review copy, which
can mean keeping your own side, put the result in place, then run `mobiler upgrade --resolved <file>`. From then on
it stays resolved, and later upgrades merge only newer changes. The same command declines any other offered change
and keeps your version. Any other edit doesn't settle anything, for example
from `mobiler plugin add`: the offered change is still merged in, or reported as a conflict.

Commit `.mobiler/` (the baseline, the version stamp and pending reviews), except `.mobiler/backup/` and `.mobiler/new/`,
which the template's `.gitignore` skips.
Review copies of Android resource files, clean or conflicted, go under `.mobiler/new/` instead of next to
the file (a stray file in `res/` breaks the build); the report prints each copy's path. Apps scaffolded before baselines existed fall back to a conservative
reconcile and get a baseline for next time. App-owned files the template provides defaults for (the
launch window colours, below) are created when missing and never changed after that.

## Android versions

Apps run on **Android 8.0 (API 26) and newer** (ADR-0039). Everything works on every supported
version; where Android itself only has a feature on newer versions, older phones get this:

| Feature | Android 8.0+ | Only on newer Android |
|---|---|---|
| Back button / gesture | Works everywhere (Compose `BackHandler`) | The predictive-back animation isn't enabled by the shell on any version |
| App language | Set by the app itself (`ShellLabels`, your own strings) on every version | Android's per-app language setting (13+) isn't used |
| Photo picker (`cx.pick_photo`) | The system file picker | The system photo picker (Android 11+ with current updates) |
| Photo options (`cx.pick_photo_with` / `capture_photo_with`) | JPEG, PNG and WebP are decoded; a HEIC photo answers `unsupported_image` | HEIC decoded (9+); lossy WebP written as `WEBP_LOSSY` (11+) |
| Notification permission (push, notifications, geofence) | No prompt: notifications need no runtime grant | The runtime prompt (13+) |
| `biometric` | Any enrolled biometric, or the device PIN/pattern, on 8–10 | Strong biometrics or the device credential (11+) |
| `bluetooth` | The legacy `BLUETOOTH` / `BLUETOOTH_ADMIN` permissions (declared up to 11) | `BLUETOOTH_SCAN` / `BLUETOOTH_CONNECT` (12+) |
| `geolocation` | The last known location, which can be empty without a recent fix | A fresh current location (11+) |
| `Video` picture-in-picture | Not available | Auto-entering PiP (12+) |
| `sqlite` | The phone's own SQLite: 3.18 on Android 8.0, 3.19 on 8.1, 3.22 on 9–10, 3.28 on 11 | Newer SQL (e.g. UPSERT needs 3.24, `RETURNING` 3.35): use `INSERT OR REPLACE` and friends to stay portable |

## Launch window colours

Before an app draws its first frame, the phone shows a launch window (and, on Android 12+, the system
splash) in the app's background colour. It follows the phone's light/dark setting. Set the two colours to
your design's backgrounds:

| | Light | Dark |
|---|---|---|
| Android | `Android/app/src/main/res/values/mobiler_splash.xml` | `Android/app/src/main/res/values-night/mobiler_splash.xml` |
| iOS | `iOS/Sources/Assets.xcassets/MobilerSplashBackground.colorset` (any appearance) | same file (dark appearance) |

These files belong to your app: `mobiler upgrade` creates them when they are missing and never changes them. A
later release will write them from `mobiler.toml`. To open in the app's own light/dark choice from the first
frame, keep that choice in your `cx.save` state. Your own Android theme items go in
`res/values/themes.xml`; they apply in light and dark mode and on every Android version.

## Debugging a text field on Android

If a `text_field` / `search_field` shows text you didn't expect, turn on the field log in a **debug
build**, restart the app, and reproduce. A release build never logs, whatever the device's settings
([ADR-0040](https://github.com/mobiler/mobiler/blob/main/docs/adr/ADR-0040-field-log-debug-only-never-secure.md)).

```bash
adb shell setprop log.tag.MobilerField DEBUG         # off again: … MobilerField INFO
adb shell setprop log.tag.MobilerFieldValues DEBUG   # optional: raw text instead of length + hash
adb logcat -s MobilerField
```

Each keystroke logs the edit the field received, each render logs the value the app sent, and an
`adopt` line marks the field taking the app's value. Values show as `<len=N #hash>`, enough to tell
which value was adopted over which; with `MobilerFieldValues` on they show as text. A `secure_field`
only ever shows `<secure len=N>`.

When scripting input with `adb shell input text`, wait until the keyboard is shown
(`adb shell dumpsys input_method | grep mInputShown=true`) before typing: a key injected while the keyboard is still starting can be delivered twice.

## Plugins

Advanced native capabilities install as **droppable plugins** — one command, no framework code or
ABI change. Bundled free plugins:

| Plugin | Capability |
|---|---|
| 🔎 `scanner` | barcode / QR scanning |
| 🔐 `biometric` | Face ID / fingerprint auth |
| 🗝️ `securestore` | encrypted key/value (Keychain / Keystore) |
| 🔌 `websocket` | persistent real-time connection (streaming, via `cx.subscribe`) |
| ⇅ `transfer` | streaming file upload/download — progress + cancel, multipart/form-data (`cx.upload`/`cx.download`) |
| 🔔 `notifications` | local scheduled notifications (reminders) |
| 🔋 `battery` | device battery level (sample) |
| 📶 `connectivity` | network status (online/wifi/cellular/offline) |
| 📄 `filepicker` | system document picker → file URI |
| 📍 `geolocation` | device location → `lat,lng` (framework LocationManager, no deps) |
| 🛰️ `geolocation-fused` | device location via Play Services FusedLocationProvider (higher accuracy; alt to `geolocation`) |
| 📇 `contacts` | system contact picker → `name|phone` |
| 📅 `calendar` | add an event (system editor) |
| 🎙️ `audio` | record (mic) + play |
| 📐 `sensors` | accelerometer / gyroscope → `x,y,z` |
| ✉️ `composer` | email / SMS / phone call via the system apps |
| 🗣️ `tts` | text-to-speech (speak a string aloud) |
| ⭐ `review` | in-app App Store / Play review prompt |
| 📤 `sharefile` | share a file / image via the share sheet |
| 📁 `files` | read/write/list app files, download a URL, export to Files/Downloads |
| 🎬 `video` | record a video → local URI |
| 🎤 `speech` | speech-to-text (dictation) |
| 🗃️ `sqlite` | on-device SQLite (exec / query → JSON) |
| 🔵 `bluetooth` | BLE scan / connect / read / write / notify (notify is a `cx.subscribe` stream) |
| 🔑 `oauth` | OAuth 2.0 / OIDC login (system auth browser → redirect) |
| 📲 `push` | remote push notifications (APNs / FCM) — **experimental, not yet device-tested** |
| 🔥 `push-firebase-only` | push via Firebase on both platforms (one FCM token; alternative to `push`) — **experimental** |
| 💳 `iap` | in-app purchase / subscriptions (StoreKit 2 / Play Billing) — **experimental, not yet device-tested** |
| 🗺️ `geofence` | background geofence enter/exit + significant-location-change — **experimental** |
| ⏰ `background-fetch` | periodic background wake (BGTaskScheduler / WorkManager) — **experimental** |
| 📊 `analytics` | product analytics + crash reporting (Firebase Analytics + Crashlytics) — **experimental** |

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
HTTP, storage, clipboard, share, browser, toast, snackbar, device info, the app's version/build, the OS light/dark setting, haptics, a confirm dialog, the photo picker, camera capture, the date picker, and the time picker.
<!-- capabilities:end -->

Navigation is a core-owned `Nav` stack; dark mode and theming are data in
the `Widget` tree. A `Scaffold` can carry a `Theme` — brand color, corner
radius, density, and font — that every native shell (iOS, Android, web)
applies; `theme: None` keeps the default look. `Density::Large` gives busy hands bigger controls
(56-unit buttons and segmented controls, 48-unit chips and calendar day cells, larger control labels)
while body text follows the system font scale. Buttons take a `Tonal` style, a `tone` (e.g.
`Tone::Danger`), a leading icon, and full width via `button_with(label, style, on_press, ButtonOpts)`.
`calendar_in(locale, …, markers)` localizes the month title, weekday header and week start and draws
0–3 busy dots per day, and `scroller_hinted(children)` fades a horizontal scroller's trailing edge to
hint there is more. These need the updated shells: in an existing app, run `mobiler upgrade`.
The widget vocabulary and runtime live in the
[`mobiler-ui`](https://crates.io/crates/mobiler-ui) and
[`mobiler-core`](https://crates.io/crates/mobiler-core) crates.

## Roadmap

Mobiler is production-bound. Beyond today's capabilities + plugins, **planned** (order is
demand-driven): animations / view transitions, and on-device hardening of the experimental plugins.
The full list lives on
[GitHub](https://github.com/mobiler/mobiler#roadmap). _Recently shipped: **`Density::Large`** (bigger touch targets), toned/tonal/icon/wide buttons, a
localized calendar with busy-dot markers, and a hinted (edge-fade) scroller; **card long-press**
(`with_long_press(card, E)` — press-and-hold for a secondary action, alongside a card's tap),
**FusedLocation** (`geolocation-fused` plugin — higher-accuracy location via Play Services) and
**BLE write/notify** (the `bluetooth` plugin gained a write op + a characteristic-change notify
stream), an interactive **map**
(`Map` — iOS MapKit / Android MapLibre / web MapLibre-GL, no API key; markers + tap events), an
**accessibility** wrapper (`a11y` — screen-reader label/hint/role on any widget), **analytics + crash
reporting** (`analytics` plugin — Firebase Analytics + Crashlytics; **experimental**), **background
geofencing + periodic wake** (`geofence` / `background-fetch` plugins; **experimental**), a **two-pane
master-detail** (`Split` — side-by-side on tablets/landscape, push-nav on phones), an **app `files` plugin**
(read/write/list + download + export to Files/Downloads), rich form fields, the
`oauth` plugin (OAuth/OIDC login), locale-aware number/currency/date formatting
(`mobiler_core::format`), a device-locale getter (`cx.device_locale`), the app's own version/build
(`cx.app_info()`, synchronous, set before `init`), the phone's OS version and model
(`cx.device_info`), an in-app PDF viewer
(`PdfView`), a native→core streaming primitive (`cx.subscribe`/`unsubscribe`, with a built-in
`ticker`), a paged feed list (`LazyList` — infinite scroll + pull-to-refresh), remote push
(`push` plugin — APNs/FCM; **experimental**), and in-app purchases (`iap` plugin — StoreKit 2 / Play
Billing: products, purchase, restore, a transactions stream; **experimental**), and a
Firebase-everywhere push variant (`push-firebase-only` — FCM on both platforms via the Firebase iOS
SDK; **experimental**), a controllable **video player** (`Video` — AVPlayer / Media3 ExoPlayer /
`<video>`: HLS + MP4, app-driven play/seek + position/ended events), an embedded **web view**
(`WebView`, + a `mobiler_core::bunny` URL helper), and **Video v2** (poster, start-offset, captions,
playback rate/volume, full transport state, playlist/queue, Picture-in-Picture, and hls.js for web
HLS on Chrome/Firefox), and **deep links + app lifecycle** (the built-in `system` stream —
inbound deep-link URLs [custom scheme, default = the app's bundle id] + foreground/background events,
on iOS/Android/web)._

## Links

- Source, demos, and guide: <https://github.com/mobiler/mobiler>

## License

Dual-licensed under either [MIT](https://github.com/mobiler/mobiler/blob/main/LICENSE-MIT) or [Apache-2.0](https://github.com/mobiler/mobiler/blob/main/LICENSE-APACHE), at your option.
