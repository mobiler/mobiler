# notifications — local scheduled notifications (free, bundled)

```bash
mobiler plugin add notifications
```

Schedules **local** notifications that fire even when the app is closed: appointment and reminder
alerts, habit nudges. It supports wall-clock times, action buttons (snooze, open), an opaque payload,
cancel and list, and an events stream that tells the app what the user did with a reminder.

**Existing apps:** the iOS side needs the notification router in `iOS/Sources/App.swift`
(`LocalNotificationBridge`). Run `mobiler upgrade --apply` first, then `mobiler plugin add notifications`
again: upgrade reports a stale plugin body but never rewrites a plugin (ADR-0011). `plugin add` refuses,
and writes nothing, until `App.swift` has the router.

## Ops

Every op takes and returns strings in the usual `{ok, output}` shape. A failure is `ok:false` with a
stable code, optionally followed by `: <detail>`.

| Op | Input | Success output |
|---|---|---|
| `requestPermission` | `""` | `ok` = allowed; output `"granted"` or `"denied"`. On Android 13+ it answers after the user responds to the system prompt (waits up to 60 s); when already decided it answers at once. |
| `permissionStatus` | `""` | `"granted"`, `"denied"` or `"notDetermined"`. Never prompts. iOS provisional and ephemeral count as `granted`. Android below 13 reports only `granted` or `denied`. |
| `openSettings` | `""` | `ok:true` when the page opened: the app's notification settings, or the app details page as a fallback. |
| `schedule` | see below | `""` |
| `cancel` | `{"id":7}` | `ok:true`. Removes the pending reminder, a pending snooze of it, the delivered one and its stored entry. |
| `cancelAll` | `""` | `ok:true`. Every pending reminder and snooze, and every delivered reminder **this plugin created**. |
| `listPending` | `""` | `[{"id":7,"fire_at":"YYYY-MM-DD HH:MM:SS","snoozed":false}]`, sorted by `fire_at`, device-local time; `[]` when none. |
| `removeDelivered` | `{"ids":[1,2]}`, or `""` for all | `ok:true`. Clears only tray entries this plugin created; pending reminders stay. |

Failure codes: `invalid_input` (bad JSON, field or range; nothing is scheduled), `past` (`at` is already
behind), `pending_limit` (iOS only, see below), `unsupported` (an op or stream this plugin does not have), `failed` (the OS refused or threw).

## `schedule`

```json
{"id": 7, "title": "Stand up", "body": "You've been sitting 45 min",
 "at": "2026-12-01 09:45",
 "payload": "opaque, up to 1024 bytes",
 "actions": [{"id":"snooze","label":"Snooze 10 min","kind":"snooze","snooze_minutes":10},
             {"id":"open","label":"Open","kind":"open"}],
 "late_minutes": 30}
```

- **`id`**: an integer in 1..=2147483647. Scheduling an existing id **replaces** it, which also cancels a
  pending snooze of that id. A missing `id` means 1 (as before), on `schedule` and on `cancel`.
- **`at`** or **`after_seconds`**: exactly one. `at` is a local wall-clock time `"YYYY-MM-DD HH:MM"`
  (or with `:SS`) in the device's current time zone. `after_seconds` is relative to now; values below 1 fire after 1 s on iOS and at once on Android.
  Both or neither is `invalid_input`; an `at` in the past is `past`.
- **`actions`**: at most 3. `kind` is `open` (opens the app) or `snooze` (needs `snooze_minutes`,
  1..=1440; re-posts the reminder after that long without opening the app). Action ids are unique,
  non-empty and at most 64 bytes; labels are non-empty.
- **`payload`**: an opaque string of at most 1024 UTF-8 bytes, returned in events. Put an id or a short
  JSON object in it, not data.
- **`late_minutes`**: 0..=10080, default 1440. How late a reminder found overdue during a **re-arm** may
  still be posted (see Android below). It never applies when the alarm itself fires, so a delayed alarm
  is never dropped.
- A tap on the notification body always opens the app, with or without an `open` action.
- With no `actions`, `payload` or `late_minutes` the request is the original reminder, so existing
  callers are unchanged.

## Events

```rust
cx.subscribe("notif", "notifications", "events", "", Msg::NotifEvent)
```

Each emission is `ok:true` with output:

```json
{"type":"opened","id":7,"action_id":"open","payload":"...","at":"2026-12-01 09:45:12"}
```

- `type` is `opened` (a tap, or an `open` button, which also carries `action_id`), `snoozed`,
  `dismissed` (swiped away) or `action` (reserved for future kinds). `payload` appears when the
  reminder had one; `at` is when it happened, device-local.
- Events are buffered natively (the last 100, oldest dropped) while no one is subscribed, and flushed in
  order on subscribe, so the tap that **launched** the app is delivered once the core subscribes.
- The guarantee is **at-least-once into the stream**, as with `geofence` (ADR-0036): an event is
  removed from the buffer only after it is handed to the stream, so a crash mid-flush re-delivers
  rather than loses. Unsubscribing drops events already handed to the stream. There is no app-side
  ack.
- **Treat the `opened` payload as untrusted input, like a deep link.** Another app can launch yours
  with crafted intent extras on Android. Validate any id you read from it and never act on it
  destructively without checking it against your own data.
- The core never runs while the app is closed (ADR-0036), so snooze, dismiss and re-arm are native only.

## Re-plan pattern

Don't try to keep a long schedule alive. On launch and whenever the plan changes, replace it:

```rust
cx.plugin("notifications", "cancelAll", "", Msg::Noop);
// then, for the next few days of reminders:
cx.plugin("notifications", "schedule", input, Msg::Noop);
```

`cancelAll` then ~40 `schedule` calls leaves exactly the new set pending; calls are applied in
order. iOS holds at most **64** pending notifications, so plan about 10 a day for a few days. A new id
over the limit fails with `pending_limit: 64 pending notifications on iOS` and adds nothing;
replacing an id or a snooze never counts as new. Android has no limit. Re-planning on launch is also
what restores reminders after an Android force-stop.

## Platform differences

- **Android:** `AlarmManager` (`setAndAllowWhileIdle`, inexact, no exact-alarm permission) fires a
  statically declared receiver, so it works with no live process. Reminders are stored in a registry
  (SharedPreferences `mobiler_notifications`) and **re-armed after a reboot, an app update, a time or
  time-zone change, and plugin init**. Overdue ones are posted if they are within `late_minutes` of
  their time and dropped otherwise. Needs `POST_NOTIFICATIONS` (API 33+) and
  `RECEIVE_BOOT_COMPLETED`; the plugin adds both and the three receivers.
  **Force-stop** cancels every alarm and blocks the receivers until the user opens the app again; plugin
  init then re-arms them, which is why you re-plan on launch.
- **iOS:** `UNUserNotificationCenter`. The OS holds the requests and **delivers missed notifications
  itself** after power-on, so `late_minutes` has no effect there. No Info.plist key is needed.
  Snooze runs in the background without bringing the app forward. Notification categories (one per
  distinct action set) are registered at launch and **only grow**.
- **Daylight saving:** an `at` inside a spring-forward gap fires at the transition instant; one in a
  fall-back overlap fires at the earlier instant. After a later time-zone change iOS fires at the
  resolved local time (03:00 for a gap time) in the new zone; Android re-computes from the requested
  wall-clock time.
- **Web:** not available (the shell answers "plugin not available").

## Notes

- **Re-scheduled ids:** `cancel` of an id you have re-scheduled removes the current one only. A snooze
  pressed on a stale tray entry of a cancelled or replaced reminder just clears that entry; it does not
  bring the reminder back.
- **Legacy alarms:** reminders set by the plugin before 0.76 still fire once after an upgrade (so none
  are lost), but `cancelAll` does not reach them. Re-plan to replace them.

## Testing

Needs a **real device** (permission prompt and a visible banner; emulators and simulators are
unreliable). Quickest: `schedule` with `after_seconds` ~15, background the app, watch the banner. Try
the Snooze button, then cancel with `cancelAll`; for re-arm, reboot with a reminder pending.

## Push (server-sent) is a separate plugin

This is **local scheduled** only: the device schedules its own reminders, with no server. Server-sent
push is the `push` plugin, and the two work side by side in one app.
