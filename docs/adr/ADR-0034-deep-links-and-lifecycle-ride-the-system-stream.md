# ADR-0034: Deep links and app lifecycle reach the core as tagged JSON events on a built-in `system` stream over `cx.subscribe`, not as `Action` variants or an installable plugin; a launch deep link is buffered until the app subscribes

Status:        Accepted
Date decided:  2026-06-08
Deciding PRs:  #134
Supersedes:    none
Code anchor:   mobiler/templates/iOS/Sources/App.swift (SystemBridge, `.onOpenURL`, scenePhase), mobiler/templates/iOS/Sources/Core.swift (`case "system"` → SystemStream), mobiler/templates/iOS/project.yml (CFBundleURLTypes), mobiler/templates/Android/app/src/main/java/__PACKAGE_PATH__/Core.kt (SystemBus, SystemPlugin), mobiler/templates/Android/app/src/main/java/__PACKAGE_PATH__/MainActivity.kt (launch intent, onNewIntent, onStart/onStop), mobiler/templates/Android/app/src/main/AndroidManifest.xml (singleTask + VIEW/BROWSABLE filter), mobiler-web/src/lib.rs (`("system", "events")` source, StreamHandle::System, system_deeplink, system_lifecycle)
Conformance:   none — the `system` source lives only in shell code (Swift, Kotlin, the wasm-only web stream code); no Rust test exercises it, and the decision (no new `Action` variant) is an absence that no existing test checks. Review, plus the device checks recorded in PR #134.

## 1. Context (The Problem)

Apps needed two kinds of inbound OS events: a URL that opens the app (a deep link), and the app
moving between foreground and background. Both start in native entry-point code (iOS `.onOpenURL`
and `scenePhase`, Android `onNewIntent` and `onStart`/`onStop`, the browser's URL and page
visibility). Both happen many times over the life of the process.

A deep link can also *launch* the app. It then arrives before the core has run `init`, so there is
nothing yet to deliver it to.

ADR-0006 had shipped a keyed streaming primitive (`cx.subscribe` / `cx.unsubscribe`,
`Effect::PluginStream`) three days earlier. Push notifications already used a buffer-until-attach
bridge (`PushBridge` / `PushBus`) for the same launch-from-dead problem.

## 2. Hypothesis

If every shell exposes one built-in stream source, plugin `"system"`, op `"events"`, that emits
tagged JSON (`{"type":"deeplink","url":…}` and `{"type":"lifecycle","state":"active"|"background"}`),
and delivers a launch deep link to the first subscriber (iOS and Android buffer it; web emits the
current URL on subscribe), then:

- deep links and lifecycle need no change to `mobiler-ui` or `mobiler-core` (no ABI change);
- an app opts in by calling `cx.subscribe(key, "system", "events", "", on_event)`, and an app that
  never subscribes pays nothing;
- a link that launched the app still reaches it, once it subscribes.

### 2.1. Refutation Conditions

- **Condition 1 — no ABI change.** The feature must ship without touching `mobiler-ui` or
  `mobiler-core`.
  - **Validation Metric:** review. PR #134 bumped only mobiler-web (0.26→0.27) and the CLI
    (0.37→0.38): "ui/core unchanged at 0.19/0.26" `[recorded: PR #134 body]`.
- **Condition 2 — the launch link survives.** A deep link that cold-launches the app must reach
  the core after it subscribes.
  - **Validation Metric:** review, plus the emulator check recorded in PR #134 ("cold-launch deep
    link auto-routes to Profile"). No automated test covers it.
- **Condition 3 — teardown.** `cx.unsubscribe` must detach the native listeners (ADR-0006).
  - **Validation Metric:** review. Web drops the `popstate` / `visibilitychange` listeners with
    the `StreamHandle::System` value; Android's `awaitClose` calls `SystemBus.detach`; iOS detaches
    the `SystemBridge` sink, unconditionally (see §5).

## 3. Considered Options & Rationale for Refutation

- **Option A — new `Action` variants (e.g. a deep-link action and a lifecycle action)** `[reconstructed]`
  The shape ADR-0018 later used for `AppInfo`. It would have been an ABI change to `Action`
  (ADR-0008) and a libraries-first, two-PR release (ADR-0009), and every app would receive the
  events whether it wanted them or not. The launch link would still need a buffer, because the
  shell cannot know when the app is ready to route it. Nobody wrote this option down at the time.
- **Option B — an installable plugin (`mobiler plugin add deeplinks`)** `[reconstructed]`
  The events start in the app's entry points (`App.swift`, `MainActivity`, the manifest's
  intent-filter and `launchMode`, `project.yml`'s `CFBundleURLTypes`). Those are template files,
  not plugin files, and a plugin only inserts lines at anchors (ADR-0011). Shipping it in the
  template makes it present in every app. The template's own comment calls `SystemBridge`
  "Always-present, plugin-agnostic" `[recorded: mobiler/templates/iOS/Sources/App.swift]`.
- **Option C — separate channels for deep links and lifecycle** `[reconstructed]`
  Two channels would build the same native→core plumbing twice. Rejected in favour of one source
  whose events carry a `type` tag.
- **Option D — one built-in `system` stream over the existing primitive** `[recorded: PR #134 body; commit 0b6d0ce]`
  Chosen. The commit: "A native→core inbound-event channel riding the existing streaming primitive
  — no ABI change". The PR: "Deep links buffer until the core subscribes (launch-from-dead), exactly the
  push `PushBridge`/`PushBus` pattern."

### Why this differs from ADR-0018 (`Action::AppInfo`)

ADR-0018 made the app's version an `Action` because the value is needed synchronously in `init`
and never changes. Its spec: "the value is known before `init`"
`[recorded: docs/superpowers/specs/2026-09-27-app-info-design.md]`. ADR-0018 itself sends the other
case elsewhere: "anything that changes at runtime belongs in a request or a stream".

Deep links and lifecycle are the other case. They are events, not a value: zero or many per run,
arriving at any time. `init` does not need them synchronously; the app can subscribe in `init` and
route the buffered launch link on the first event. So a stream fits, and it avoided an ABI change
and a library publish for ui and core.

## 4. Decision & Rationale for Corroboration

Option D. Each shell registers `"system"` in its built-in stream dispatch, next to `ticker`:

- **iOS:** `SystemBridge` in `App.swift` forwards `.onOpenURL` and `scenePhase`; `Core.swift`
  routes `case "system"` to `SystemStream`. `project.yml` registers `CFBundleURLTypes`, with the
  bundle id as the default scheme.
- **Android:** `MainActivity` sends the launch intent, `onNewIntent`, and `onStart`/`onStop` to
  `SystemBus`; `SystemPlugin.subscribe` is a `callbackFlow` over it. The manifest sets
  `launchMode="singleTask"` and a VIEW/BROWSABLE intent-filter on the application id.
- **Web:** `mobiler-web` answers `("system", "events")` with the current URL as a deep link and the
  page visibility as lifecycle, then listens to `popstate` and `visibilitychange`.

Deep links buffer until a subscriber attaches. Lifecycle is not buffered; the current state is sent
on attach. PR #134 recorded verification on all three platforms: web in headless Chrome, a
cold-launch deep link on the Android emulator, and iOS on a device via TestFlight.

## 5. Consequences (Positive and Negative Predictions)

- **Positive:** shipped as web 0.27 + CLI 0.38 with no ui/core release. Apps opt in by subscribing.
- **Positive:** a new kind of inbound OS event can join as a new `type` on the same stream without
  an ABI change.
- **Negative:** the payload is stringly-typed JSON inside `PluginResponse.output`. The core has no
  typed helper; each app parses and demuxes `type` itself (barbershop's `Msg::SystemEvent`).
- **Negative:** on iOS and Android the bridge holds one sink. A second `system` subscription
  replaces the first, which then stops receiving. Web creates listeners per subscription, so the
  shells differ here.
- **Negative:** iOS teardown is unsafe. `SystemBridge.detach()` clears the sink unconditionally,
  from a separate main-actor task. Cancelling a subscription that was already replaced silences
  the live one, and an unsubscribe followed by a resubscribe can race so the new subscription goes
  quiet. Android only clears the sink if it is still the same one (`if (sink === s)`). Found while
  writing this record; not yet fixed.
- **Negative:** "deep link" means different things per platform. On web it is the page URL at
  subscribe time and on `popstate`, not an external open.
- **Negative:** on Android, the default scheme (the application id) is the same one the `oauth`
  plugin's redirect activity claims, so an app with both gets a disambiguation chooser. PR #134
  records this; iOS is unaffected.
- **Negative:** only custom schemes are wired. Universal links / App Links (https plus server
  files) are not.
- **Negative:** the stream is not the only inbound source. The OS appearance later got its own
  built-in `appearance` stream (ADR-0020) rather than a `type` on `system`, so "one channel for OS
  events" does not hold in general.
