# ADR-0073: The `notifications` plugin answers its ops with stable failure codes, keeps every Android reminder in a persisted registry that is re-armed after a reboot, an update, a time or time-zone change and plugin init, applies `late_minutes` only to such re-arms, marks everything it creates, and reports what the user did through a buffered `events` stream delivered at least once into the stream

Status:        Superseded by ADR-0074
Date decided:  2026-11-27
Deciding PRs:  #324 (CLI 0.76.0)
Supersedes:    none
Code anchor:   mobiler/plugins/notifications/android/ReminderRegistry.kt (`parseSchedule`, `resolveWall`, `rearmDecision`, `receiverShouldPost`, `snoozeEntry`, `ReminderRegistry`), mobiler/plugins/notifications/android/NotificationReceivers.kt (`LocalNotif`, `alarmIntent`, `armAlarm`, `cancelAlarm`, `postReminder`, `rearmAll`, `NotificationReceiver`, `NotificationRearmReceiver`, `NotificationActionReceiver`), mobiler/plugins/notifications/android/NotificationsPlugin.kt, mobiler/plugins/notifications/android/ReminderEvents.kt (`EventBuffer`), mobiler/plugins/notifications/ios/NotificationsPlugin.swift, mobiler/plugins/notifications/ios/NotificationsInput.swift (`ReminderInput`, `WallTime`), mobiler/plugins/notifications/ios/NotificationsEvents.swift (`NotificationEvents`, `NotificationCategories`, `snooze`), mobiler/plugins/notifications/mobiler-plugin.toml, mobiler/templates/iOS/Sources/Core.swift (`Plugins.inOrder`), mobiler/src/plugin.rs (`check_local_bridge`, `in_order_names`), demos/barbershop (the same plugin and shell, ADR-0015)
Conformance:   xtask/tests/adr_conformance.rs::adr_0073_plugin_declares_rearm_and_marks_every_request, mobiler/src/plugin.rs::add_notifications_inserts_receivers_boot_permission_and_app_launch, mobiler/src/plugin.rs::add_notifications_refuses_on_old_core_swift

## 1. Context (The Problem)

The bundled `notifications` plugin could schedule a one-off reminder by relative seconds and cancel it. On Android
an alarm died with a reboot or an app update, and a tap did nothing; on iOS a tap reached the `push` plugin
(ADR-0072). An app building on mobiler (a reminder app that re-plans its next few days of reminders, about 30–40,
every time it opens) asked for reliable reminders, management ops, local wall-clock times across daylight-saving
switches, snooze and open buttons, and a report of what the user did with each reminder.

The plugin's contract is what apps code against, and parts of it are hard to change once shipped: the failure
codes apps match on, the stored registry an update must still read, the overdue policy, the event schema, and what
"delivered" means. The core never runs while the app is closed (ADR-0036), so everything that happens then
(posting, snooze, dismissal, re-arm) has to be native.

## 2. Hypothesis

If the plugin keeps the contract below on both platforms, then reminders survive what the platform throws at them
(within the limits named in §5), existing calls keep working (§5 names the inputs and outputs that changed), and
the app learns of every interaction at least once, in order.

**Ops and failures.** Input and output are JSON strings in the `{ok, output}` shape (ADR-0003). A failure is
`ok: false` with `<code>` or `<code>: <detail>`, in ADR-0068's style; the codes are `invalid_input`, `past`,
`pending_limit`, `unsupported` and `failed`. Codes may be added, never renamed; the detail text is not part of the
contract. The ops: `requestPermission`, `permissionStatus`, `openSettings`, `schedule`, `cancel`, `cancelAll`,
`listPending`, `removeDelivered`, and the stream `events`.

**`schedule` input** (Android `parseSchedule`, iOS `ReminderInput.parseSchedule`, the same rules and strings):
- `id`: an integer in 1..=2³¹−1. **A missing `id` is 1**, on `schedule` and `cancel`, as before. A present id that
  is a string, a fraction, a boolean or out of range is `invalid_input`. Re-scheduling an id replaces it, and with
  it a pending snooze of that id.
- exactly one of `at` (`"YYYY-MM-DD HH:MM"` or `":SS"`, strict: a real date and a 00–23 hour) and
  `after_seconds`. Both or neither is `invalid_input`. An `at` not after now is `past`. `after_seconds` keeps its
  old reading (a number or numeric string, truncated); below 1 it fires at once on Android and after 1 s on iOS.
- `actions`: at most 3; kinds `open` and `snooze` (`snooze_minutes` 1..=1440); ids unique, non-empty, at most 64
  UTF-8 bytes; labels non-empty.
- `title` at most 256 and `body` at most 2048 UTF-8 bytes (on Android both ride in the snooze button's
  PendingIntent extra).
- `payload`: a string of at most 1024 UTF-8 bytes. `late_minutes`: 0..=10080, default 1440.
- Any violation is `invalid_input` and nothing is scheduled. A malformed `cancel` or `removeDelivered` input is
  `invalid_input` too.

**Daylight saving and zones.** An `at` in a spring-forward gap resolves to **the transition instant itself**
(Europe/Belgrade 2027-03-28 02:30 → 03:00:00), not shifted by the gap's length as `ZonedDateTime.of` would
(03:30). One in a fall-back overlap resolves to the earlier instant. After a later time-zone change:
- **Android** re-computes `fire_at_ms` from the stored original wall time (`wall`) in the new zone, at re-arm.
- **iOS** keeps the trigger's resolved local date and time (no zone in its components), which iOS matches in the
  device's current zone. A gap time therefore stays at the resolved 03:00 rather than the requested 02:30.

**Android: the registry.**
- SharedPreferences `mobiler_notifications`, key `r.<id>`, value
  `{id, title, body, fire_at_ms, wall?, payload?, actions?, late_minutes, snoozed}`, every write `commit()`ed.
- Written **before** the alarm is set. A schedule whose alarm can't be set puts back the entry it would have
  replaced and answers `failed`.
- Removed when the reminder posts, on `cancel` and on `cancelAll`. A snooze writes a new entry (`snoozed: true`,
  no `wall`) after the fired one was removed.
- The ops, the firing receiver, the snooze and the re-arm all hold one lock (`synchronized(ReminderRegistry)`)
  around their registry and alarm work, and the ops that touch the registry run without suspending, in the order
  the shell starts them (ADR-0065's Android path). So `cancelAll` followed by 40 `schedule`s leaves exactly those 40.

**Android: alarms and markers.**
- One place arms an alarm: `armAlarm`, `setAndAllowWhileIdle(RTC_WAKEUP)`, inexact, so no `SCHEDULE_EXACT_ALARM`
  (minSdk 26, ADR-0039). Its PendingIntent comes from `alarmIntent`, the only intent the plugin aims at
  `NotificationReceiver`, which carries `mobiler_v2 = true`, `mobiler_local_id` and `mobiler_local_action`.
- Every PendingIntent of reminder `id` uses request code `id` and `FLAG_IMMUTABLE`; they are told apart by a field
  that takes part in PendingIntent identity, never by extras: the body tap has category `mobiler.local`
  (ADR-0072), open button `i` adds `mobiler.local.a<i>`, snooze button `i` has action `mobiler.local.snooze.<i>`,
  the delete intent has `mobiler.local.dismiss`.
- Every tray entry posted from the registry has tag `mobiler.local` on channel `mobiler_reminders`; a legacy
  alarm's (below) is posted untagged on that channel, as the old plugin did. `cancel`, `cancelAll` and
  `removeDelivered` remove only tagged entries and untagged ones on that channel, never another plugin's.

**Android: the receivers.**
- `NotificationReceiver` (not exported) posts a v2 alarm's registry entry and removes it. No entry means it was
  cancelled: nothing is posted. An entry not yet due belongs to a replacement scheduled while this alarm was in
  flight: it is left for its own alarm.
- **`late_minutes` never applies when an alarm fires.** An inexact alarm that Doze delayed is on time by
  definition, so a normal delay never drops a reminder.
- `NotificationRearmReceiver` is exported so the system can reach it; it filters `BOOT_COMPLETED`,
  `MY_PACKAGE_REPLACED`, `TIMEZONE_CHANGED` and `TIME_SET` and acts on no other action. The manifest adds
  `RECEIVE_BOOT_COMPLETED`. It and plugin init (which also recovers after a
  force-stop, once the app is opened) run `rearmAll`. For each entry, after re-computing a wall-clock one in the
  current zone (`rearmDecision`):
  - due in the future → armed;
  - overdue while its alarm still exists (`FLAG_NO_CREATE` finds the PendingIntent: no reboot or force-stop
    cleared it) **and the re-compute left `fire_at_ms` unchanged** → re-armed for its past time, so it fires as
    soon as the system allows; `late_minutes` doesn't apply, it is merely late;
  - any other overdue one (its alarm is gone, or a time-zone change moved it into the past) → posted now if at
    most `late_minutes` late (measured from the re-computed `fire_at_ms`), else dropped; either way its alarm is
    cancelled. So flying Belgrade to Tokyo with reminders at 11:00–17:00 and `late_minutes: 30` drops the ones now
    more than 30 minutes behind instead of firing them all at once.
- `NotificationActionReceiver` (not exported) handles snooze (writes the snoozed entry, arms it, clears the tray
  entry, buffers `snoozed`) and the delete intent (buffers `dismissed`). Native only, with no core.
- **Legacy alarms.** An alarm set by the pre-registry plugin has no `mobiler_v2` extra; the firing receiver still
  posts it from its old `title`/`body` extras, so a reminder pending across the update isn't lost. It has the same
  PendingIntent identity as a v2 alarm of its id, so `cancel` of that id or a new `schedule` of it replaces or
  cancels it. `cancelAll` can't reach it: it isn't in the registry, and AlarmManager can't list alarms. It fires
  once at most.

**iOS.** The OS holds the requests, so there is no registry; the pending requests are the state.
- Every request has identifier `mobiler.<id>` and `userInfo` with `mobiler_local: <id>` (the ADR-0072 marker),
  `late_minutes`, `snoozed`, `fire_at_s`, a per-schedule `mobiler_token`, and `payload` and `actions` when given.
  `at` uses a `UNCalendarNotificationTrigger`, `after_seconds` a time-interval trigger.
- **Pending limit:** iOS keeps at most 64 pending requests per app, counted across the whole app. Adding a **new**
  id at 64 answers `pending_limit: 64 pending notifications on iOS` and adds nothing. Replacing a pending id never
  counts as new, and a snooze is never refused for the limit.
- `late_minutes` is stored but has no effect: iOS delivers missed notifications itself after power-on.
- **Categories:** one per distinct action set, **including the empty set**, so every reminder can report
  `dismissed` (`.customDismissAction`). Identifier `mobiler.c.<FNV-1a 64 of the canonical actions JSON>`. A snooze
  button has no `.foreground` (handled in the background); `open` has `.foreground`. `setNotificationCategories`
  replaces the whole set, so the plugin always registers the union of what iOS has, what it registered this
  process and what pending or delivered reminders need: the set only grows. `bootstrap()` re-registers at launch.
- The ops that read or change the pending set run one at a time on `NotificationsLane`, and `Plugins.inOrder`
  names `notifications` (ADR-0065's mechanism) in the template and every demo, so calls reach the plugin in issue
  order. `mobiler plugin add notifications` refuses, writing nothing, while the app's `Core.swift` has no live
  `static let inOrder` line naming `"notifications"` (`check_local_bridge`), and says to run
  `mobiler upgrade --apply` first.
- `cancelAll` (pending and delivered) and `removeDelivered ""` (delivered) take marked requests and pre-marker
  ones (identifier `mobiler.<digits>`), and nothing else.

**Snooze, on both platforms.** Native: the reminder is re-posted `snooze_minutes` later under the same id, marked
`snoozed`, and `snoozed` is buffered. When the id was re-scheduled since the tray entry was posted (Android: a
registry entry exists; iOS: a pending `mobiler.<id>` with another `mobiler_token`), the new plan wins: the snooze
only clears that tray entry, posts nothing and reports nothing. `cancel` and `cancelAll` remove the tray entry
itself, and its buttons with it.

**Permission.** `requestPermission` never answers from the plugin's own memory. iOS always calls
`requestAuthorization` (the system shows a prompt only while undecided). Android 13+ launches the system request
whenever the permission isn't granted and an activity is showing (the system answers at once, with no dialog, when
it was denied for good), waits up to 60 s, and answers the resulting status; below 13, or with no activity, it
answers the current status. `permissionStatus` never prompts.

**The `events` stream.** `cx.subscribe(…, "notifications", "events", "", …)`. Each emission is `ok: true` with
`{"type":"opened"|"action"|"snoozed"|"dismissed","id":7,"action_id"?:"…","payload"?:"…","at":"YYYY-MM-DD HH:MM:SS"}`
(`at` is when it happened, device-local; `action` is reserved for future kinds and not emitted today).
- Every event is written first to a persisted, ordered buffer (Android prefs key `events`; iOS UserDefaults
  `mobiler.notifications.events`) of at most 100, oldest dropped, then flushed in order into the attached sink.
  Each is removed only after the sink returned, so a crash mid-flush re-delivers rather than loses.
- The guarantee is **at-least-once into the shell's stream**, as `geofence`'s (ADR-0036): an event handed to the
  stream is not re-delivered if the process dies, or the subscription ends, before the core handles it. There is no
  app-side ack.
- A second subscriber replaces the first (ADR-0006); a stale unsubscribe can't detach the live one (ADR-0034). A
  cold-start tap's `opened` waits in the buffer for the first subscriber.
- The `opened` payload is untrusted input (ADR-0072 §5): the README says to treat it like a deep link.

### 2.1. Refutation Conditions

- **Condition 1 — the plugin declares what the re-arm needs.**
  `RECEIVE_BOOT_COMPLETED`; the re-arm receiver exported and filtering exactly the four broadcasts it handles; the
  alarm and action receivers not exported.
  - **Validation Metric:** `adr_0073_plugin_declares_rearm_and_marks_every_request`,
    `add_notifications_inserts_receivers_boot_permission_and_app_launch` (`plugin add` writes them).
- **Condition 2 — everything the plugin creates is marked.**
  iOS, per request, on the live code with every `//` comment removed and string contents blanked (so neither a
  comment nor a string can satisfy it): each use of `UNNotificationRequest` is either a construction
  (`UNNotificationRequest(` or `.init(`, with any spacing or line break) or a type use, else the test fails. For
  each construction, in its enclosing `func` and before it: its `content:` is a `let C =
  UNMutableNotificationContent()`; `C.userInfo` is assigned exactly once in the func, before the request, to an
  `[AnyHashable: Any]` literal variable that contains `LocalNotificationBridge.markerKey:` and is never reassigned,
  cleared or given a new marker; its `identifier:` is a `let … = requestIdentifier(…)`. Android: AlarmManager is
  called only in `armAlarm`, whose intent carries the three markers.
  - **Validation Metric:** `adr_0073_plugin_declares_rearm_and_marks_every_request`.
- **Condition 3 — iOS calls keep issue order.**
  - **Validation Metric:** `adr_0073_plugin_declares_rearm_and_marks_every_request` (`Plugins.inOrder` in the
    template and barbershop; the CLI-managed demos equal the template by ADR-0063's test),
    `add_notifications_refuses_on_old_core_swift` (an app whose `inOrder` lacks the plugin, or names it only in a
    comment, can't install it; nothing is written).
- **Condition 4 — the input rules, wall-clock resolution, re-arm policy, snooze and event buffer.**
  Unit-tested in plain JVM tests in `demos/barbershop/Android/app/src/test/…/ReminderLogicTest.kt`
  (`missingIdDefaultsTo1`, `presentButBadIdIsInvalid`, `bothAtAndAfterIsInvalid`, `pastAtIsPast`,
  `titleAndBodyAreCapped`, `gapResolvesAfter`, `overlapResolvesEarlier`, `rearmRecomputesWallInNewZone`,
  `rearmPostsLateWithinWindowAndDropsTooLate`, `rearmLeavesALiveOverdueAlarmToFire`,
  `rearmAppliesLateMinutesWhenAZoneChangeMovesALiveAlarmIntoThePast`,
  `receiverIgnoresAnEntryNotYetDue`, `snoozeLeavesANewerScheduleOfTheIdAlone`,
  `reminderJsonRoundTripsQuotesNewlinesNonAscii`, …) and `ReminderEventsTest.kt` (`bufferCapsAt100DropsOldest`,
  `flushInOrderThenEmpty`, `secondSubscriberReplacesFirst`, `crashMidFlushRedeliversRemainder`,
  `liveSinkFailureKeepsTheEventBuffered`, `eventJsonShape`). These run with Gradle
  (`./gradlew :app:testDebugUnitTest`), not in CI, so they can't be cited under `Conformance:`.
- **Condition 5 (runtime).** Android emulators (API 26 and 33) in this work: snooze with the process dead
  re-posts; cold and warm taps give one `opened` across a kill before subscribe; a swipe gives `dismissed`;
  `cancelAll` after a snooze leaves nothing to fire; an `at` reminder and a time-zone change. The maintainer's
  iPhone pass covers a cold-start tap, a lock-screen snooze with the app dead, dismissal (Clear all included), a
  payload with quotes, a newline and emoji, the 64 limit and an overlap-hour `at`.

## 3. Considered Options & Rationale for Refutation

- **Option A — a SharedPreferences registry, one JSON string per reminder** `[recorded: docs/superpowers/specs/2026-11-27-notifications-reminders-design.md §3]`
  Chosen: "small, synchronous and crash-safe for ~40 entries, and readable from a receiver without the core."
- **Option B — a file or a SQLite table for the registry** `[recorded: docs/superpowers/specs/2026-11-27-notifications-reminders-design.md §3]`
  Rejected: "more code for no gain at this size."
- **Option C — a fixed 24 h overdue window** `[recorded: docs/superpowers/specs/2026-11-27-notifications-reminders-design.md §4.2]`
  The request's suggestion. Rejected for `late_minutes` (default 1440): a two-hour-late "stand up" is noise to one
  app and fine to another.
- **Option D — apply `late_minutes` whenever a reminder is late, also when its alarm fires** `[recorded: docs/superpowers/specs/2026-11-27-notifications-reminders-design.md §4.2]`
  Rejected: "An inexact alarm that Doze delays is on time by definition, so normal delays never drop a reminder."
  The build extended this to a re-arm that finds the alarm still alive, but only while the re-compute leaves its
  time unchanged: a reminder that a time-zone change moved into the past was never delayed by Doze.
- **Option E — drop a legacy alarm (no registry entry) when it fires** `[recorded: docs/superpowers/specs/2026-11-27-notifications-reminders-design.md §4.3]`
  Rejected: reminders pending across the update would be lost ("never silently disappears").
- **Option F — end-to-end delivery with an app-side ack op** `[recorded: docs/superpowers/specs/2026-11-27-notifications-reminders-design.md §4.5]`
  Deferred as a non-goal; at-least-once into the stream matches `geofence`.
- **Option G — a built-in repeat, a batch "replace all" op, exact alarms** `[recorded: docs/superpowers/specs/2026-11-27-notifications-reminders-design.md §2]`
  Non-goals: the app plans a rolling window, and `cancelAll` plus ~40 `schedule` calls is cheap.
- **Option H — let `ZonedDateTime.of` resolve a gap (shift by the gap's length)** `[reconstructed]`
  Rejected: 02:30 would become 03:30, half an hour after the first valid instant the spec asks for. Android takes
  the transition instant from the zone rules; iOS finds it by a search over the offsets and hands the trigger the
  resolved time.
- **Option I — a `requestPermission` fast path that answers "denied" from the rationale state without asking**
  `[reconstructed]`
  Rejected in the build: it would take a prompt dismissed with Back for a permanent denial, while the system
  itself answers a permanent denial at once with no dialog.

## 4. Decision & Rationale for Corroboration

The contract in §2. Every rule an app can observe is the same on both platforms except where the platform decides:
the iOS pending limit, `late_minutes` (iOS delivers missed notifications itself), and what a later time-zone
change does to a gap time. Those differences are in the plugin README.

**Mutation proof** (2026-11-28; each mutation reverted after it was seen to fail, then the suite was green):
- Removing `android.permission.RECEIVE_BOOT_COMPLETED` from the manifest's `permissions` failed
  `adr_0073_plugin_declares_rearm_and_marks_every_request` ("the manifest lost RECEIVE_BOOT_COMPLETED").
- Removing the `BOOT_COMPLETED` `<action>` from the re-arm receiver failed it ("the re-arm receiver doesn't filter
  BOOT_COMPLETED").
- Removing `LocalNotificationBridge.markerKey: r.id,` from the snooze request (`NotificationsEvents.swift`) failed
  it ("…/NotificationsEvents.swift: a UNNotificationRequest is built without the mobiler_local marker (`info` lacks
  it)"); so did commenting that line out with `//`.
- Moving that marker into a trailing comment (`ReminderKeys.payload: "", // LocalNotificationBridge.markerKey:
  r.id,`) failed it the same way. The first version of the test, which kept trailing comments, would have passed.
- Removing `LocalNotificationBridge.markerKey: spec.id,` from the `schedule` request failed it
  ("…/NotificationsPlugin.swift: a UNNotificationRequest is built without the mobiler_local marker (`info` lacks
  it)").
- Adding a function to `NotificationsInput.swift` that builds an unmarked request with the call broken over two
  lines (`UNNotificationRequest(` then `identifier: "x", content: content, …`) failed it ("…/NotificationsInput.swift:
  a UNNotificationRequest's content `content` must get its userInfo exactly once, before the request"). The first
  version of the test skipped that call.
- Adding a second, unmarked `UNNotificationRequest.init(identifier: identifier, content: plain, …)` inside
  `schedule`, after the marked one, failed it ("… content `plain` must get its userInfo exactly once, before the
  request"). The first version, which checked per function, would have passed.
- Adding `content.userInfo = [:]` after the marked assignment in `snooze` failed it ("… content `content` must get
  its userInfo exactly once, before the request").
- The scan's own unit test `swift_requests_are_marked_per_request` covers these shapes, a marker inside a string
  literal, a cleared `info`, and an unknown use (`UNNotificationRequest.self`).
- Removing `putExtra(LocalNotif.EXTRA_V2, true)` from `alarmIntent` failed it ("the alarm intent doesn't
  `putExtra(LocalNotif.EXTRA_V2, true)`").
- Arming the alarm directly in `NotificationsPlugin.schedule` (`setAndAllowWhileIdle` with an unmarked
  `NotificationReceiver` intent) instead of `armAlarm` failed it ("…/NotificationsPlugin.kt sets an alarm with
  `setAndAllowWhileIdle(` outside armAlarm").
- Taking `notifications` out of the template's `Plugins.inOrder` failed it ("…/Core.swift's Plugins.inOrder
  doesn't name notifications").
- Disabling the `Core.swift` check in `check_local_bridge` (`if false && !in_order_names(…)`) failed
  `add_notifications_refuses_on_old_core_swift` ("an old Core.swift must stop the install"); so did letting
  `in_order_names` count `//`-commented lines (`.filter(|_l| true)`), since the fixture names `notifications` only
  in a comment.
- (JVM, by hand) Restoring the unconditional `alarmAlive -> ARM` in `rearmDecision` failed
  `rearmAppliesLateMinutesWhenAZoneChangeMovesALiveAlarmIntoThePast` (expected DROP, got ARM). Removing the
  `title` byte check from `parseSchedule` at first did **not** fail `titleAndBodyAreCapped`: the test's `error(…)`
  helper called itself instead of `kotlin.error`, parsed its own message as input and "failed" with
  `invalid_input`, so every negative input assertion passed vacuously. With the helper fixed, removing the
  `title`, `body` or `payload` byte check, the `at`/`after_seconds` exclusivity check or the repeated-action-id
  check each failed its test (`titleAndBodyAreCapped`, `payloadOver1024BytesInvalid`, `bothAtAndAfterIsInvalid`,
  `duplicateActionIdInvalid`).

## 5. Consequences (Positive and Negative Predictions)

- **Positive:** Android reminders survive a reboot, an app update and a time or zone change; a force-stop is
  recovered the next time the app opens.
- **Positive:** an app can re-plan on every open with `cancelAll` and a batch of `schedule`s and get exactly the new
  set, on both platforms.
- **Positive:** existing callers of `schedule {after_seconds}` and `cancel` that pass a numeric `id` (or none), a
  `title` and `body` within the caps and an `after_seconds` keep working with the same success answers, and an
  Android body tap now opens the app.
- **Negative:** failure outputs changed from free text (`invalid input JSON`) to the codes, and Android 13+'s
  `requestPermission` now waits for the user and answers `granted` or `denied` where it answered `requested`. An
  app that matched the old strings has to follow.
- **Negative:** some inputs the old plugin accepted are now `invalid_input`: a string `id` (`"7"`, which the old
  Android plugin read as 7 and the old iOS one as 1), a `schedule` with neither `at` nor `after_seconds` (it fired at once), and a `title`
  over 256 or `body` over 2048 UTF-8 bytes.
- **Negative:** delivery is at-least-once into the stream, not end-to-end: an app must tolerate a duplicate event
  after a crash mid-flush, and loses an event handed to the stream if the process dies or the subscription ends
  first.
- **Negative:** a legacy Android alarm isn't reached by `cancelAll`; it fires once after the update unless its id
  is cancelled or re-scheduled.
- **Negative:** a force-stop cancels every Android alarm and blocks the receivers until the app is opened again;
  only re-planning on launch covers it. The init re-arm then posts, with sound, every missed reminder within
  `late_minutes` (up to a day's worth by default), racing the app's re-plan; the README says to lower
  `late_minutes` to avoid the burst.
- **Negative:** `cancelAll` removes pending snoozes and every delivered reminder, so a re-plan built on it loses a
  snooze the user just pressed (on iOS a snooze with the app dead launches it in the background, where a re-plan
  can follow at once). The README's re-plan reads `listPending` first, keeps snoozed ids and cancels the rest by
  id.
- **Negative:** `late_minutes` has no effect on iOS.
- **Negative:** after a time-zone change, a gap-time reminder fires at the resolved 03:00 on iOS but at the
  requested 02:30 on Android when that time exists in the new zone.
- **Negative:** the overlap rule on iOS rests on iOS matching a calendar trigger's first occurrence; only the
  iPhone pass checks it.
- **Negative:** iOS categories only grow; a category for an action set no longer used stays registered.
- **Negative:** the iOS limit counts every pending request in the app, not only this plugin's, and a snooze is
  never refused, so a snooze can take the app past 64, where iOS's own limit applies.
- **Negative:** a snooze pressed at the moment the app cancels that reminder can still be applied after the cancel
  (the tray entry it came from was already fired and out of the registry), bringing the reminder back once as a
  snooze. The window is the time between the press and the cancel.
- **Negative:** the Kotlin unit tests run only by hand; CI compiles the plugin in the demos that install it
  (barbershop, coffee) but doesn't run them.
- **Negative:** no web support: the web shell answers "plugin not available" (ADR-0067).
