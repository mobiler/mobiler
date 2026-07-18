# background-fetch — periodic background wake (free, bundled)

> ⚠️ **Experimental — not yet device-tested.** The OS decides *when* (and whether) a background wake
> fires, so it can't be exercised reliably in CI or the simulator. Everything compiles + installs on
> both platforms, but the actual wake → notification → event delivery has **not** been validated on a
> physical device yet. Verify on a real device and please report what you find. (Mobiler itself is
> experimental — see the project README.)

```bash
mobiler plugin add background-fetch
```

A periodic, OS-scheduled wake for the mobile shells. The OS runs a brief native task on its own
schedule; that task posts a **local notification** *and* **buffers an event** delivered to your core on
the next foreground (the launch-from-dead pattern, like `push`/`system`). The Rust core never runs in
the background — this is a native opportunistic wake plus buffered delivery. No ABI change.

```rust
// 1) Schedule a periodic wake. `min_interval_seconds` is a FLOOR (OS-throttled; Android enforces a
//    15-minute minimum). `notify_title`/`notify_body` (optional) is the notification shown on wake.
cx.plugin("background-fetch", "schedule",
    r#"{"id":"refresh","min_interval_seconds":3600,"notify_title":"Updated","notify_body":"New data ready"}"#,
    Msg::Noop),
cx.plugin("background-fetch", "cancel", r#"{"id":"refresh"}"#, Msg::Noop),

// 2) Subscribe to wake events — do this at startup so a wake that ran while the app was dead
//    (buffered by the shell) isn't missed.
cx.subscribe("background-fetch", "background-fetch", "events", "", Msg::Fetch),
Msg::Fetch(r) => if r.ok { /* r.as_text() = {"type":"fetch","id":"refresh"} → re-fetch your data */ },
```

See `app-core-usage.rs` for a fuller `update`-loop example.

## What the plugin does

- **iOS** — `BGTaskScheduler` with the identifier `mobiler.refresh`, **registered at app launch** via
  the `// mobiler:app-launch` hook (BGTaskScheduler.register must run before `didFinishLaunching`
  returns) and listed in `BGTaskSchedulerPermittedIdentifiers`. Each `BGAppRefreshTask` posts a
  notification, buffers a `fetch` event in `UserDefaults`, and reschedules the next wake. The `id` you
  pass is an opaque label echoed back in the event.
- **Android** — WorkManager `PeriodicWorkRequest` (auto-initialized; no launch hook) → a
  `BackgroundFetchWorker` that posts a notification and persists the event in `SharedPreferences` until
  the core subscribes.
- **web** — native-only; calls return `ok:false` (graceful degradation).

## Timing reality

Background wakes are **best-effort and OS-throttled** — not a precise timer. iOS schedules
`BGAppRefreshTask` opportunistically (based on usage patterns, battery, network); Android enforces a
**15-minute minimum** periodic interval. Treat `min_interval_seconds` as a floor, never a guarantee.
For exact-time alerts, use the `notifications` plugin (scheduled local notifications) instead.
