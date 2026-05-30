# notifications — local scheduled notifications (free, bundled)

```bash
mobiler plugin add notifications
```

Schedules **local** notifications that fire even when the app is closed — appointment/reminder
alerts (built for the scheduling vertical, but generally useful).

## App-side usage (Rust)

```rust
cx.plugin("notifications", "requestPermission", "", Msg::NotifAllowed),     // ask once
cx.plugin("notifications", "schedule",
    r#"{"id":1,"title":"Appointment","body":"in 1 hour","after_seconds":3600}"#, Msg::Scheduled),
cx.plugin("notifications", "cancel", r#"{"id":1}"#, Msg::Noop),
```

`id` is an integer so `cancel` can target a specific reminder; `after_seconds` is relative to now
(compute it from the appointment time minus the lead time). See `app-core-usage.rs`.

## How it works per platform

- **iOS:** `UNUserNotificationCenter` — `requestAuthorization` then a non-repeating
  `UNTimeIntervalNotificationTrigger`. The OS holds the scheduled notification, so it fires when the
  app is closed. No Info.plist key needed for local notifications.
- **Android:** `AlarmManager` (`setAndAllowWhileIdle`) → a statically-declared **`NotificationReceiver`**
  that posts to a "Reminders" channel — fires without a live process. Needs **`POST_NOTIFICATIONS`**
  (API 33+) and the `<receiver>` in the manifest; the plugin adds both (`permissions` +
  `manifest_application`). Timing is **inexact** (avoids the restricted exact-alarm permission) —
  fine for reminders; switch to exact alarms only if you need to-the-minute precision.
- **Web:** graceful `ok:false`.

## Testing

Needs a **real device** (notification permission + actually seeing the banner; emulator/simulator are
unreliable). Quickest: `schedule` ~10–15s out, background the app, watch the banner appear. Android
13+ and iOS both show the permission prompt on `requestPermission`.

## Push (server-sent) is a later, separate plugin

This is **local scheduled** only (the device schedules its own reminders — no server). Server-sent
push (APNs/FCM, e.g. "your booking was confirmed" from a backend) is the future `push` plugin, which
needs device-token registration + a backend — build it once the scheduling server exists.
