# ADR-0036: Background features are native plugin code that does not depend on the Rust core running: the plugin does its work natively and hands each event to its stream, which delivers it to a live subscriber or holds it until the app next subscribes

Status:        Accepted
Date decided:  2026-06-09
Deciding PRs:  #139
Supersedes:    none
Code anchor:   mobiler/plugins/geofence/** (GeofenceMonitor, GeofenceBroadcastReceiver, GeofenceBus), mobiler/plugins/background-fetch/** (BackgroundFetchBridge, BackgroundFetchWorker, BackgroundFetchBus), mobiler/templates/iOS/Sources/App.swift (`// mobiler:app-launch`), mobiler/src/plugin.rs (`ios.app_launch`, array-valued `info_plist`)
Conformance:   none — the decision is an absence (no plugin starts a core or sends it an `Action` from a background wake), and it lives in Swift and Kotlin that no test runs. The plugins' install tests in `mobiler/src/plugin.rs` check wiring, not this. Review enforces it.

## 1. Context (The Problem)

Apps wanted background behaviour: react when the user enters a place, refresh data periodically.
Both platforms start this work on their own schedule, often with the app's process dead:
iOS relaunches the app for a region crossing or a `BGAppRefreshTask`; Android starts the process
for a `BroadcastReceiver` or a WorkManager `Worker`, with no Activity.

The mobiler core lives inside the UI host. On Android it is the `Core` ViewModel, created by
`MainActivity`; on iOS it is a `@StateObject` on the SwiftUI `App`. It is driven by `Action`s and
its effects are rendered by a live shell. A background wake has neither.

ADR-0006 had shipped a keyed stream primitive, and the push plugin (#118) had introduced the
buffer-until-attach pattern that ADR-0034 reused for a launch deep link one day before this work
began.

## 2. Hypothesis

If background features are native plugin code that does its work without the core, and each
event is handed to the plugin's stream (sent straight to an attached subscriber, else persisted in
`UserDefaults` / `SharedPreferences` and flushed when one attaches), with a local notification
where the plugin posts one (geofence crossings always; a fetch wake only with `notify_title`),
then:

- background features need no ABI change and no core runtime outside the UI host;
- an event that fired while the process was dead still reaches the app on its next launch;
- where a notification is posted, the user learns of the event at once, even with no core.

### 2.1. Refutation Conditions

- **Condition 1 — the work needs no core.** No background entry point (`GeofenceBroadcastReceiver`,
  `BackgroundFetchWorker`, the `BGTaskScheduler` handler, the `CLLocationManager` delegate)
  creates a core; each only does its native work and calls the plugin's `emit`. (If a core is
  already alive with a subscription, `emit` reaches it; see §5.)
  - **Validation Metric:** review of the plugin sources. No test.
- **Condition 2 — dead-process events survive.** An event with no subscriber is persisted and
  flushed on the next `attach`.
  - **Validation Metric:** review. Both plugins are marked experimental and were not
    device-tested when #139 merged.

## 3. Considered Options & Rationale for Refutation

- **Option A — run the core headless in the background** `[reconstructed]`
  Start a core in the Worker, receiver or `BGTask` handler and send it an `Action`, so app logic
  (for example a sync) runs there. Not written down as an option at the time. It would need a
  core that can run without a UI host, a way to answer its effects with no shell on screen, and a
  way to reconcile its state with the foreground core. The recorded scope rules it out.
- **Option B — native-scheduled work plus buffered delivery** `[recorded: PR #139 body; commit 029d78d]`
  Chosen. The PR: "The Rust core never runs in the background — native-scheduled work + buffered
  delivery (the roadmap's exact scope). web degrades to `ok:false`." The plugin sources repeat
  it: "The Rust core never runs in the background."
  `[recorded: mobiler/plugins/background-fetch/android/BackgroundFetchPlugin.kt]`.
  That wording is stronger than the code: the work never needs the core, but an event does reach
  a core that is still alive and subscribed (§5).

## 4. Decision & Rationale for Corroboration

Option B, in two opt-in plugins (ADR-0010), split by permission profile. Neither changed
`mobiler-ui`, `mobiler-core` or `mobiler-web` ("no lib ABI bump — ui/core/web stay
0.20/0.27/0.28" `[recorded: PR #139 body]`).

- **geofence:** iOS `GeofenceMonitor` (one `CLLocationManager`, re-created at launch through
  `GeofencePlugin.bootstrap()`); Android `GeofencingClient` delivering to a statically declared
  `GeofenceBroadcastReceiver`. A crossing posts a notification and calls `emit`.
- **background-fetch:** iOS `BGTaskScheduler` task `mobiler.refresh`, registered at launch by
  `BackgroundFetchPlugin.bootstrap()`; Android a WorkManager `BackgroundFetchWorker`. A wake posts
  a notification (if a title was given) and emits `{"type":"fetch","id":…}`.
- **Delivery:** `emit` sends to the attached sink if there is one; otherwise it appends to a
  persisted buffer. `attach` (called from `cx.subscribe`) flushes the buffer. The app is told to
  subscribe at startup so a buffered event isn't missed.

#139 added the iOS `// mobiler:app-launch` anchor and the plugin `ios.app_launch` field for this,
because both must run at launch: `BGTaskScheduler.register` synchronously, before
`didFinishLaunchingWithOptions` returns. The geofence re-arm is deferred to a main-actor `Task`,
which runs just after it returns. It is inserted by `plugin add` (ADR-0011).

## 5. Consequences (Positive and Negative Predictions)

- **Positive:** shipped as CLI 0.41 with no library release. New background plugins follow the
  same shape: native trigger, notification, persisted event, stream.
- **Positive:** the core keeps one host and one lifecycle. The `system` lifecycle event
  (ADR-0034) stays the core's signal that the app is leaving the foreground.
- **Negative:** app logic cannot run in the background. "Refresh my data" means the native task
  only notifies; the app re-fetches when it next runs. Anything the app must compute before
  showing a notification is out of reach.
- **Negative:** the core is not kept out of the background. The decision is only that the work
  does not need it. `emit` delivers straight to an attached sink, and a subscription outlives
  backgrounding: on Android the stream is collected in the `Core` ViewModel's `viewModelScope`,
  which survives `onStop`; on iOS the stream `Task` survives suspension. So when the process is
  still alive, a crossing or a wake reaches the core (`core.resolve`, then the app's `update`)
  while the app is in the background. An event buffers when no subscription is attached: a wake
  into a dead process (no `MainActivity`, hence no `Core`, on Android) or after the subscription
  was torn down. Whether an iOS background relaunch creates the `@StateObject` core and subscribes is
  not verified.
- **Negative:** timing is the OS's. Android WorkManager enforces a 15-minute minimum, and iOS
  runs `BGAppRefreshTask` when it chooses. `min_interval_seconds` is a floor.
- **Negative:** web has no equivalent; both plugins answer `ok: false` there.
- **Negative:** iOS `detach()` in both plugins clears the sink unconditionally (Android checks
  `sink === s`), the same pattern ADR-0034 flags for `SystemBridge`. A replaced subscription's
  cancellation can leave the live one detached, so events buffer until the next `attach`.
