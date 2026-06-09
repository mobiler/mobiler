# analytics — product analytics + crash reporting, Firebase (free, bundled)

> ⚠️ **Experimental — not yet device-tested.** Everything compiles + installs on both platforms, but the
> Firebase round-trip can't be exercised in CI (no Firebase project / config files committed). Verify on
> a real device with your own Firebase project, and please report what you find. (Mobiler itself is
> experimental — see the project README.)

```bash
mobiler plugin add analytics
```

Product analytics (events, user id, user properties) **plus automatic native crash reporting**, via
**Firebase Analytics + Crashlytics** on both iOS and Android. Fire-and-forget — no ABI change. Crash
capture is **automatic** once Firebase is configured; the API is just for events + crash context.

```rust
// Events
cx.plugin("analytics", "logEvent",
    r#"{"name":"booking_confirmed","params":{"service":"fade","value":35}}"#, Msg::Noop),
cx.plugin("analytics", "setUserId", "user-123", Msg::Noop),
cx.plugin("analytics", "setUserProperty", r#"{"name":"plan","value":"pro"}"#, Msg::Noop),

// Consent / privacy — toggles BOTH analytics and crash collection
cx.plugin("analytics", "setEnabled", "false", Msg::Noop),

// Crash reporting (auto-capture needs no call; these enrich the next report)
cx.plugin("analytics", "log", "entered checkout", Msg::Noop),                 // breadcrumb
cx.plugin("analytics", "recordError", r#"{"message":"sync failed"}"#, Msg::Noop), // non-fatal
cx.plugin("analytics", "testCrash", "", Msg::Noop),  // force a crash to verify wiring (dev only)
```

See `app-core-usage.rs` for a fuller `update`-loop example.

## What the plugin does

- **iOS** — Firebase Analytics + Crashlytics via SwiftPM. `FirebaseApp.configure()` runs **at app
  launch** (the `// mobiler:app-launch` hook) so Crashlytics catches startup crashes — but **only if a
  `GoogleService-Info.plist` is bundled**; without it the app does **not** crash and every op returns
  `ok:false`.
- **Android** — Firebase Analytics + Crashlytics; `FirebaseApp` auto-initializes (no launch hook). The
  `google-services` + `crashlytics` Gradle plugins are applied automatically.
- **web** — native-only; calls return `ok:false` (graceful degradation).

## Setup (one-time, per app)

1. Create a Firebase project and register your iOS + Android app ids.
2. **iOS** — download **`GoogleService-Info.plist`** into `iOS/Sources/`. (Without it the plugin is inert.)
3. **Android** — download **`google-services.json`** into `Android/app/`. The Gradle build **fails**
   without it.
4. **iOS crash symbolication (optional)** — Crashlytics *captures* crashes out of the box, but for
   readable (symbolicated) stack traces add a Run Script build phase that runs Firebase's `upload-symbols`
   over your dSYMs (see the Crashlytics iOS docs). Not automated by this plugin.

If you also install **`push-firebase-only`**, the two share one `FirebaseApp.configure()` (guarded) and
one set of config files — no conflict.
