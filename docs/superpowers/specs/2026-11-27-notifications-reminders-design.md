# `notifications` plugin: reliable reminders, management, wall-clock times, actions and events — design

Date: 2026-11-27
Status: draft, awaiting maintainer approval
Requests: the five `docs/requests/2026-11-26-notification-*.md` (actions, events, management, reliability,
wallclock-trigger). Those docs are temporary and not committed, so their substance is restated here.
Constrained by: ADR-0002, ADR-0003, ADR-0006, ADR-0010, ADR-0011, ADR-0013, ADR-0018, ADR-0034, ADR-0035, ADR-0036,
ADR-0039, ADR-0046, ADR-0063, ADR-0067
New ADRs: ADR-0072 (local notifications are routed to their plugin by a marker; iOS router in the template),
ADR-0073 (the `notifications` contract: registry, overdue policy, event schema, limits)

## 1. Intent

Quit Sitting, a project built on mobiler, reminds people to get up. Each time the app opens it plans the
next few days of reminders (about 10 a day, so roughly 30–40 pending) and drops the previous plan.
- The reminders must keep working while the app is closed, after a reboot and after an app update.
- They must fire at local wall-clock times across daylight-saving switches.
- They must offer "Snooze 10 min" and "Open" on the notification itself.
- On the next open, the app wants to learn what the user did with each reminder: opened, snoozed or
  dismissed.

Today the plugin can only schedule a one-off reminder by relative seconds and cancel it.
- **Android:** the reminders die on reboot or update, and tapping one does nothing.
- **iOS:** a tap on a local reminder is forwarded to the `push` plugin as a push "opened" event.

Nothing here is specific to Quit Sitting. Habit, medication and appointment apps need the same
capability, so it ships as general options, never as Quit Sitting's exact values.

## 2. Goals, non-goals, anti-goals

Goals (all five requests):
1. **Reliability (Android):** a persisted registry of scheduled reminders. Re-arm after `BOOT_COMPLETED` and
   `MY_PACKAGE_REPLACED`, and after a time or time-zone change for wall-clock reminders. Document force-stop.
2. **Management:** `cancelAll`, `listPending` and `removeDelivered`.
3. **Wall-clock trigger:** `schedule {at: "YYYY-MM-DD HH:MM[:SS]"}`, plus `permissionStatus` and `openSettings`.
   `requestPermission` on Android 13+ answers after the user responds.
4. **Actions:** up to 3 buttons, kinds `open` and `snooze`. Snooze re-posts natively. A body tap opens the app.
   An opaque `payload` travels with each reminder. A pending limit applies on iOS.
5. **Events:** a `notifications` `events` stream (`opened`/`action`/`snoozed`/`dismissed`), buffered while
   nothing is subscribed and delivered in order.

Non-goals:
- A built-in repeat. The app plans a rolling window, and the maintainer confirmed that's the usage.
- Exact alarms (`SCHEDULE_EXACT_ALARM`). Reminders stay inexact, as today.
- A batch "replace all" op. `cancelAll` plus ~40 `schedule` calls is cheap.
- Web support. The web shell keeps answering `ok:false "plugin 'notifications' not available"` (ADR-0067),
  so no `mobiler-web` release is needed. *(My call: the request's web `permissionStatus` mapping would force a
  library release for a single op.)*
- Rich content: images, sounds or custom channels per reminder.

Anti-goals (failures even if everything technically works):
- A local reminder's tap or button reaching the `push` plugin, or a push tap reaching `notifications`.
- Snooze, dismiss or open needing the Rust core to run (ADR-0036).
- A reminder that silently disappears: after reboot or update, by going over iOS's 64 limit, or by being lost
  from the registry.
- An event delivered twice after a successful flush, or lost before one.
- Existing callers of `schedule {after_seconds}` / `cancel` changing behaviour, apart from the requested
  Android "a tap opens the app".

## 3. Approaches

**How taps and button presses reach the plugin.** The maintainer picked **A** on 2026-11-27.
- **A. A marker-based router (chosen).**
  - The plugin stamps every reminder it creates with a marker: iOS `userInfo["mobiler_local"]`, Android an
    intent extra.
  - **iOS:** the template's `AppDelegate` is the only `UNUserNotificationCenterDelegate`. It routes marked
    notifications to a new always-present `LocalNotificationBridge` in `App.swift`, which stays dormant without
    the plugin. Everything else goes to `PushBridge`, as now. The plugin attaches its handler at launch through
    the existing `mobiler:app-launch` hook (`app_launch = [...]` in its manifest), so a snooze pressed while the
    app is dead is handled natively.
  - **Android:** no template change. The plugin follows `push`'s `PushOpens` precedent
    (`ActivityLifecycleCallbacks` reading the launch intent). Its own manifest-declared receivers handle
    snooze, dismiss and reboot.
- B. The plugin replaces the iOS delegate at launch and chains to the `AppDelegate`. Rejected: whoever sets the
  delegate last wins, so a future plugin or a launch-order change silently breaks push or snooze.

**Where the reminder registry lives (Android).** This is my call.
- SharedPreferences, one JSON string per reminder under key `r.<id>`, written with `commit()`. Chosen: small,
  synchronous and crash-safe for ~40 entries, and readable from a receiver without the core.
- A file or a SQLite table. Rejected: more code for no gain at this size.

## 4. Design

### 4.1 Ops

Input and output are JSON strings in the existing `{ok, output}` shape (ADR-0003). Every failure is
`ok:false` with a stable code, optionally followed by `: <detail>`, in the ADR-0068 style:
`invalid_input`, `past`, `pending_limit`, `unsupported`, `failed`.

| Op | Input | Success output |
|---|---|---|
| `requestPermission` | `""` | `ok` = allowed. Output `"granted"` / `"denied"`. **Android 13+:** answers after the user responds to the system prompt (an `ActivityResult` callback, waiting ≤ 60 s). When already decided, it answers at once. |
| `permissionStatus` | `""` | `ok:true`, `"granted"` \| `"denied"` \| `"notDetermined"`. Never prompts. iOS provisional and ephemeral map to `granted`. Android 13+ tells `denied` from `notDetermined` with a persisted "asked once" flag. Android < 13: `granted` or `denied` (`areNotificationsEnabled`). |
| `openSettings` | `""` | `ok:true` when the page opened. iOS 16+ uses `openNotificationSettingsURLString`, else `openSettingsURLString`. Android uses `ACTION_APP_NOTIFICATION_SETTINGS`, falling back to the app details page. |
| `schedule` | see 4.2 | `ok:true`, `""` |
| `cancel` | `{"id":7}` | `ok:true`. Removes the pending reminder (and a pending snooze of it), the delivered one and the registry entry. |
| `cancelAll` | `""` | `ok:true`. Every pending reminder and snooze plus every delivered reminder **this plugin created**; the registry is emptied. |
| `listPending` | `""` | `ok:true`, `[{"id":7,"fire_at":"YYYY-MM-DD HH:MM:SS","snoozed":false}]` sorted by `fire_at`, device-local time; `[]` when none. |
| `removeDelivered` | `{"ids":[1,2]}`, or `""` for all | `ok:true`. Removes only tray entries this plugin created. Pending reminders are untouched. |

### 4.2 `schedule`

```json
{"id": 7, "title": "Stand up", "body": "You've been sitting 45 min",
 "at": "2026-12-01 09:45",            // or "after_seconds": 2700 — exactly one of the two
 "payload": "opaque ≤ 1024 bytes",
 "actions": [{"id":"snooze","label":"Snooze 10 min","kind":"snooze","snooze_minutes":10},
             {"id":"open","label":"Open","kind":"open"}],
 "late_minutes": 30}
```

- **`id`** is an integer in 1..=2³¹−1. Re-scheduling an existing id replaces it, which also cancels a pending
  snooze of that id. A **missing** `id` keeps today's default of 1, on `schedule` and on `cancel`, so existing
  callers are unchanged. Only a present but out-of-range or non-integer `id` is `invalid_input`. *(My call, from
  the "existing callers unchanged" anti-goal.)*
- **`at`** is a local wall-clock time in the device's current time zone.
  - Times in a spring-forward gap fire at the first valid instant after it.
  - Times in a fall-back overlap fire at the earlier instant.
  - A past `at` → `ok:false "past"`.
  - Giving both `at` and `after_seconds`, or neither → `invalid_input`.
  - `after_seconds` keeps exactly today's behaviour (≥ 1 s on iOS, as today).
- **`actions`**: at most 3. Kinds are `open` and `snooze`; `snooze` needs `snooze_minutes` in 1..=1440. Action
  ids are unique, non-empty and ≤ 64 bytes; labels are non-empty. Any violation → `invalid_input` and nothing is
  scheduled.
- **`payload`** is at most 1024 UTF-8 bytes, else `invalid_input`.
- **`late_minutes`** (my call, general instead of the request's fixed "24 h"): how late a reminder found
  **overdue during a re-arm** may still be posted. Re-arms happen after a reboot, an app update, a time or
  time-zone change, or plugin init after a force-stop. Range 0..=10080, default 1440 (the request's suggestion).
  Quit Sitting would pass ~30, because a two-hour-late "stand up" is noise.
  - It is measured from `fire_at_ms`.
  - It **never** applies when the alarm itself fires. An inexact alarm that Doze delays is on time by
    definition, so normal delays never drop a reminder. *(My call.)*
- **Pending limit (iOS):** the plugin counts the app's pending requests. If adding a **new** id would exceed
  64 → `ok:false "pending_limit: 64 pending notifications on iOS"` and nothing is added. Replacing an id or a
  snooze re-post never counts as new. Android has no limit.
- A body tap always opens the app, even with no `open` action.
- With no `actions`, `payload` or `late_minutes`, the request is today's reminder.

### 4.3 Android: registry, alarms, receivers (plugin only)

- **Registry:** SharedPreferences `mobiler_notifications`, key `r.<id>`. The value is JSON:
  `{id, title, body, fire_at_ms, wall?: "YYYY-MM-DD HH:MM:SS", payload?, actions?, late_minutes, snoozed}`.
  `wall` is kept for `at` reminders so a time-zone change re-computes `fire_at_ms`.
  - Written before the alarm is set.
  - Removed on fire, on `cancel` and on `cancelAll`.
  - A snooze rewrites the entry with `snoozed:true` and the new `fire_at_ms`.
- **Alarms:** `setAndAllowWhileIdle(RTC_WAKEUP)` as today, request code `id`, `FLAG_IMMUTABLE`.
- **Firing receiver** (`NotificationReceiver`, extended):
  - It reads the registry entry. A missing entry means cancelled, so it posts nothing. The exception is a
    **legacy alarm** set by the pre-registry plugin, recognised by its intent having no `mobiler_v2` extra: it
    still posts from the old `title`/`body` extras, exactly as before, so reminders pending across the update
    aren't lost. *(My call, from the "never silently disappears" anti-goal.)*
  - Otherwise it posts with:
    - a **content intent**: the launcher activity with extras `mobiler_local_id` and `mobiler_local_action=""`;
    - a **delete intent** to `NotificationActionReceiver` (dismissed);
    - one `PendingIntent` per action: `open` → the launcher activity with `mobiler_local_action=<id>`;
      `snooze` → `NotificationActionReceiver`;
    - request codes unique per (id, action index), `FLAG_IMMUTABLE`.
- **`NotificationActionReceiver`** (new, `exported=false`):
  - **snooze:** cancel the tray entry, update the registry, set an alarm in `snooze_minutes`, buffer `snoozed`.
  - **dismiss:** buffer `dismissed`. The registry entry is already gone, because it fired.
- **`NotificationRearmReceiver`** (new, `exported=true` only for the system broadcasts it filters):
  `BOOT_COMPLETED`, `MY_PACKAGE_REPLACED`, `TIMEZONE_CHANGED`, `TIME_SET`. For each registry entry:
  - re-compute `fire_at_ms` from `wall` for wall-clock reminders;
  - if it's still in the future, set the alarm;
  - if it's late within `late_minutes`, post it now;
  - otherwise drop it.
  
  Manifest: permission `RECEIVE_BOOT_COMPLETED`; the `<receiver>` entries are added by the plugin manifest.
- **Plugin init:** re-arm every registry entry, the same as on boot. This also recovers after a force-stop once the
  app is opened again. Re-setting an alarm with the same `PendingIntent` is idempotent.
- **Taps:** a `LocalOpens` object (the `PushOpens` precedent) registers `ActivityLifecycleCallbacks`. It consumes
  `mobiler_local_id` from the resumed activity's intent and ignores `FLAG_ACTIVITY_LAUNCHED_FROM_HISTORY`. It
  buffers `opened` (with `action_id` for an `open` button) and removes the extras so they're consumed once.
- **Force-stop:** Android cancels the alarms and blocks the receivers until the next launch. The README says so
  and tells apps to re-plan on launch, which Quit Sitting already does.

### 4.4 iOS: router in the template, plugin behind it

- **Template `App.swift`:**
  - `willPresent`: if `userInfo["mobiler_local"]` is present → `LocalNotificationBridge.shared.willPresent(...)`
    → banner + sound and no push event. Otherwise the current push path.
  - `didReceive`: if marked → `LocalNotificationBridge.shared.didReceive(response, completion)`, which hands the
    completion handler to the plugin's handler or calls it at once when no handler is attached. Otherwise the
    current push path. Push behaviour is unchanged for unmarked notifications.
  - `LocalNotificationBridge` is always present and plugin-agnostic. It holds an optional handler closure and
    buffers responses that arrive before a handler attaches (cold launch), replaying them on attach. It
    references nothing from the plugin, so push-less and notifications-less apps compile unchanged.
- **Plugin:**
  - `bootstrap()` runs at launch through `app_launch`. It attaches the handler and registers categories.
  - **Category per distinct action set, including the empty set**, so every marked reminder can report
    `dismissed` on iOS as it does on Android *(my call)*. The identifier is
    `mobiler.c.<fnv1a64 of the canonical action JSON>`,
    with `.customDismissAction`. Snooze is a `UNNotificationAction` with no `.foreground`, so iOS handles it in the
    background without bringing the app forward. `open` uses `.foreground`. Categories are re-registered at
    launch from the pending requests' stored action JSON.
  - **Request:** identifier `mobiler.<id>`. `userInfo` holds `mobiler_local:<id>`, `payload`, the actions JSON
    and `late_minutes`.
  - **Triggers:** `at` → `UNCalendarNotificationTrigger` from `Calendar.current` components, so it follows the
    zone and DST. `after_seconds` → a time-interval trigger, as today.
  - **Handler:**
    - default action → buffer `opened`;
    - `open` → `opened` with `action_id`;
    - `snooze` → add a new request with the same identifier and an interval trigger of `snooze_minutes`, then
      buffer `snoozed`;
    - dismiss → `dismissed`;
    - then call the completion handler.
  - **iOS has no registry:** `listPending` uses `getPendingNotificationRequests` filtered by marker, and
    `fire_at` comes from the trigger's `nextTriggerDate()`. A pending snooze has `snoozed:true` in its userInfo.
  - **`late_minutes` on iOS:** the OS delivers missed notifications itself after power-on. This is documented as
    the platform difference.

### 4.5 Events stream

```rust
cx.subscribe("notif", "notifications", "events", "", |r| Msg::NotifEvent(r));
```

- Each emission is `ok:true` with output
  `{"type":"opened"|"action"|"snoozed"|"dismissed","id":7,"action_id"?:"snooze","payload"?:"…","at":"YYYY-MM-DD HH:MM:SS"}`.
  `action` is reserved for future kinds.
- **Buffer:** a persisted ordered list (Android SharedPreferences, iOS UserDefaults) of at most 100 events; the
  oldest are dropped. While a sink is attached, events go straight to it.
- **On attach, the buffer flushes in order.** Each event is removed from the store only after `emit` returns,
  so a crash mid-flush re-delivers rather than loses. Duplicates are impossible after a completed flush.
  - The guarantee is **at-least-once into the shell's stream**, the same as `geofence` (ADR-0036). An event
    handed to the stream is not re-delivered if the process dies before the core handles it.
  - End-to-end delivery would need an app-side ack op, which is a non-goal for now. *(My call.)*
- **Unsubscribe detaches the sink;** later events buffer again. A second subscriber replaces the first (ADR-0006).
- A cold-start tap's `opened` is buffered until the first subscriber, the same rule as deep links (ADR-0034).

### 4.6 Delivery and upgrade
- **One CLI release** (ADR-0009): the plugin sources and manifest, the template `App.swift` router, and a CLI-side
  test. No ABI or library change.
- **Barbershop first** (ADR-0015): its `App.swift` gets the router. Barbershop doesn't install `notifications`
  today, so it **gains the plugin and a new "Reminders" card**: a wall-clock reminder in 2 minutes with Snooze
  and Open, `listPending`, `cancelAll`, and an events log. *(My call.)* `demos/coffee` already uses the plugin;
  it gets `upgrade --apply` (ADR-0063) and a plugin re-add.
- **Install guard:** `mobiler plugin add notifications` refuses with "run `mobiler upgrade --apply` first" when
  the app's `App.swift` lacks `LocalNotificationBridge`, the same as `check_push_bridge` does for `push`.
  *(My call.)*
- **Existing apps:**
  - run `mobiler upgrade --apply` for the router;
  - then `mobiler plugin add notifications` again for the new plugin body, since upgrade reports drift but never
    rewrites a plugin (ADR-0011).
  
  The README and the release note say both.
- **ADR-0063:** the CLI-managed demos are upgraded.

## 5. Verification
- **CLI unit tests:** the plugin manifest parses; `plugin add` inserts the new receivers and the
  `RECEIVE_BOOT_COMPLETED` permission and the iOS `app_launch` line; the template `App.swift` carries the router.
- **Conformance (xtask, each proven by a mutation):**
  - `App.swift` routes marked notifications away from `PushBridge` in both delegate methods;
  - the plugin manifest declares the reboot/update/time receivers;
  - the plugin stamps the marker on every request it creates (source check on both platforms).
- **Android emulator (API 26 and 34):**
  - schedule `at` +2 min with Snooze and Open; snooze with the app killed → re-posts after N min;
  - a body tap cold and warm → `opened` with payload;
  - swipe → `dismissed`; reboot the emulator → the reminder still fires;
  - `cancelAll` → nothing fires; `listPending` is correct;
  - `late_minutes` drop: set the clock past;
  - a time-zone change re-arms a wall-clock reminder.
- **iOS:** compiled by CI. The maintainer's iPhone pass covers snooze (app killed), open, dismiss, events after
  relaunch, and the 64 limit.

## 6. Left to the builder
- Kotlin/Swift structure inside the plugin. Keep the plugin files focused, e.g. a registry file plus a receivers
  file per platform.
- Exact README wording.
- How the Android 13 `requestPermission` waits for the result: an `ActivityResultRegistry` callback via
  `MobilerActivity.current`, or a transparent helper activity. Pick the least invasive one that needs no
  template change. If neither works without one, stop and ask.

## Amendment 2026-11-27 (Task 1)

- `willPresent` for a marked notification is handled inline in the `AppDelegate` (banner + sound, no
  `PushBridge` call); the bridge has no `willPresent` method.
- With no handler attached, `LocalNotificationBridge.didReceive` calls the completion at once and buffers only
  the response (at most 32, oldest dropped); `attach` replays buffered responses in order with a no-op completion.
  The plugin attaches in `didFinishLaunching` (`app_launch`), before iOS delivers a launch response, so the
  buffer only covers an app without the plugin or a detach.

## Amendment 2026-11-28 (final review)

- **Re-arm:** an overdue reminder whose alarm is still alive skips `late_minutes` only when the re-compute did not
  move its time. A time-zone change that moves an `at` reminder into the past (Belgrade to Tokyo) is subject to
  `late_minutes` like any other overdue one, so a day of reminders doesn't fire at once.
- **Input caps:** `title` at most 256 and `body` at most 2048 UTF-8 bytes, else `invalid_input`, on both
  platforms. On Android both travel in the snooze button's PendingIntent extra.
- **Re-plan guidance:** the README tells apps to read `listPending` first, keep snoozed ids and `cancel` the rest
  per id, because `cancelAll` also drops pending snoozes and clears the tray.
- **Install guard:** `plugin add notifications` also refuses while `iOS/Sources/Core.swift`'s `Plugins.inOrder`
  doesn't name `notifications`.
