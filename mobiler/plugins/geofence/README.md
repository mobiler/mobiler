# geofence — background geofencing + significant-location-change (free, bundled)

> ⚠️ **Experimental — not yet device-tested.** Background-trigger paths can't be exercised in CI or
> the simulator without real device movement. Everything compiles + installs on both platforms, but
> the actual enter/exit + significant-change delivery has **not** been validated on a physical device
> yet. Verify on a real device (simulate a route / drop a pin near a registered region) and please
> report what you find. (Mobiler itself is experimental — see the project README.)

```bash
mobiler plugin add geofence
```

Native **background location monitoring** for the mobile shells — geofence enter/exit and
significant-location-change. The OS keeps watching even when the app is closed; on a trigger the
shell posts a **local notification** *and* **buffers an event** that's delivered to your core on the
next foreground (the launch-from-dead pattern, like `push`/`system`). The Rust core itself never runs
in the background — this is native-scheduled work plus buffered delivery. No ABI change.

```rust
// 1) Ask for ALWAYS location authorization (required for triggers while the app is closed).
cx.plugin("geofence", "requestPermission", "", Msg::GeoPerm),

// 2) Register / remove circular regions. `notify_title`/`notify_body` is the local notification
//    shown when the region is crossed.
cx.plugin("geofence", "add",
    r#"{"id":"shop","lat":47.3769,"lng":8.5417,"radius":150,"notify_title":"Nearby","notify_body":"You're at the shop"}"#,
    Msg::Noop),
cx.plugin("geofence", "remove", r#"{"id":"shop"}"#, Msg::Noop),
cx.plugin("geofence", "list", "", Msg::GeoList),  // → ["shop", ...]

// 3) Coarse, low-power background location updates (≈ significant change).
cx.plugin("geofence", "startSignificantChanges", "", Msg::Noop),
cx.plugin("geofence", "stopSignificantChanges", "", Msg::Noop),

// 4) Subscribe to events (the streaming primitive) — do this at startup so an enter/exit that woke
//    a dead process (buffered by the shell) isn't missed.
cx.subscribe("geofence", "geofence", "events", "", Msg::GeoEvent),
Msg::GeoEvent(r) => if r.ok {
    // r.as_text() = {"type":"geofence","id":"shop","event":"enter"|"exit"}
    //            or {"type":"location","lat":..,"lng":..}
},
```

See `app-core-usage.rs` for a fuller `update`-loop example.

## What the plugin does

- **iOS** — a single long-lived `CLLocationManager` + delegate (`CLCircularRegion` monitoring +
  `startMonitoringSignificantLocationChanges`). Re-created at app launch via the `// mobiler:app-launch`
  hook so a background relaunch delivers the queued region event to a live delegate. On a trigger it
  posts a `UNUserNotification` and buffers the event in `UserDefaults`, flushing to the stream on
  subscribe.
- **Android** — Play Services `GeofencingClient` + `FusedLocationProviderClient`, delivered to a
  statically-declared `GeofenceBroadcastReceiver` (runs even when the process is dead) that posts a
  notification and persists the event in `SharedPreferences` until the core subscribes.
- **web** — native-only; calls return `ok:false` (graceful degradation).

## Permissions / setup

- **iOS** — geofencing needs **Always** location authorization. iOS prompts in two steps
  (When-In-Use, then a deferred Always upgrade); the user must choose **Always** for triggers to fire
  while closed. The plugin adds `NSLocation*UsageDescription` strings + `UIBackgroundModes=[location]`
  to `project.yml`.
- **Android** — needs **Google Play Services** on the device, `ACCESS_FINE_LOCATION`, and
  `ACCESS_BACKGROUND_LOCATION` (the **"Allow all the time"** grant — a *separate* settings-screen
  choice on Android 11+), plus `POST_NOTIFICATIONS` (API 33+). `requestPermission` fires the
  foreground prompt; background access is then enabled by the user in system settings.
