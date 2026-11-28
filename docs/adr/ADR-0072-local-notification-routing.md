# ADR-0072: A local notification is routed to the `notifications` plugin by a marker it stamps on everything it creates: on iOS the template's `AppDelegate` stays the only notification delegate and sends marked notifications to an always-present `LocalNotificationBridge` in `App.swift`, never to `PushBridge`; on Android the plugin's own `LocalOpens` reads its marked launch intent, whose `mobiler.local` category keeps its PendingIntent apart from push's

Status:        Accepted
Date decided:  2026-11-27
Deciding PRs:  #324 (CLI 0.76.0)
Supersedes:    none
Code anchor:   mobiler/templates/iOS/Sources/App.swift (`AppDelegate.userNotificationCenter(_:willPresent:…)`, `userNotificationCenter(_:didReceive:…)`, `LocalNotificationBridge`), mobiler/plugins/notifications/ios/NotificationsPlugin.swift (`bootstrap`), mobiler/plugins/notifications/ios/NotificationsEvents.swift (`NotificationResponses`), mobiler/plugins/notifications/android/NotificationReceivers.kt (`LocalNotif`, `reminderLaunchIntent`, `postReminder`, `openIntent`), mobiler/plugins/notifications/android/ReminderEvents.kt (`LocalOpens`), mobiler/plugins/notifications/mobiler-plugin.toml (`app_launch`), mobiler/src/plugin.rs (`check_local_bridge`), demos/barbershop (the same App.swift router and plugin, ADR-0015)
Conformance:   xtask/tests/adr_conformance.rs::adr_0072_local_notifications_never_reach_push, mobiler/src/plugin.rs::add_notifications_refuses_on_old_app_swift, mobiler/src/plugin.rs::add_notifications_inserts_receivers_boot_permission_and_app_launch, mobiler/src/plugin.rs::add_push_still_works_beside_notifications

## 1. Context (The Problem)

Before this decision the `notifications` plugin scheduled a reminder and nothing handled what the user did with it:
- **iOS:** the template's `AppDelegate` is the app's `UNUserNotificationCenterDelegate`, and it forwarded every
  notification response to `PushBridge`. A tap on a local reminder reached the `push` plugin as a push `opened`
  event.
- **Android:** a reminder carried no content intent, so tapping it did nothing.

The upgraded plugin (ADR-0073) has to receive taps, action buttons (snooze, open) and dismissals, including while
the app is dead, since the core never runs then (ADR-0036). Push must keep working beside it unchanged. iOS allows
one notification-center delegate per app, so on iOS the question is who owns it.

## 2. Hypothesis

If the plugin stamps a marker on everything it creates and each platform routes on that marker before anything
else looks at the notification, then a local reminder's tap, button or dismissal reaches only the `notifications`
plugin, a push reaches only the `push` plugin, and neither plugin needs to know the other exists.

**The markers:**
- **iOS:** every request the plugin adds carries `userInfo["mobiler_local"] = <id>` (ADR-0073 checks every
  `UNNotificationRequest` the plugin builds, one request at a time). `LocalNotificationBridge.isLocal` is `userInfo["mobiler_local"] != nil`.
- **Android:** every tray tap (the body, and each `open` button) opens the launcher activity through
  `reminderLaunchIntent`, which adds the category `mobiler.local`, with the extras `mobiler_local_id`,
  `mobiler_local_action` (`""` for a body tap, the action id for a button) and `mobiler_local_payload` when the
  reminder has a payload.

**iOS: the router lives in the template.**
- The template's `AppDelegate` stays the only `UNUserNotificationCenterDelegate`. No mobiler plugin sets the
  delegate.
- `willPresent`: a marked notification is answered inline with `[.banner, .sound]` and returns; `PushBridge` is not
  called. An unmarked one takes the push path as before.
- `didReceive`: a marked response goes to `LocalNotificationBridge.shared.didReceive(response, completion:)` and
  returns, for **every** action (tap, button, dismissal); `PushBridge` is not called. An unmarked one takes the push
  path as before (only the default action becomes a push `opened`).
- `LocalNotificationBridge` is always present and references nothing from the plugin, so an app without the
  plugin compiles and the bridge stays dormant. It holds one optional handler:
  - with a handler attached, the handler gets the response and owns the completion handler (it must call it once);
  - with none attached, the bridge calls the completion **at once** and buffers only the response (at most 32,
    oldest dropped); `attach` replays the buffer in order, with a no-op completion. Holding the completion for an
    attach that may never come would keep a background action's launch time open.
- The plugin attaches its handler in `NotificationsPlugin.bootstrap()`, which `plugin add` inserts at the
  `mobiler:app-launch` anchor inside `didFinishLaunching` (`app_launch` in its manifest, ADR-0011). That runs
  before iOS delivers the response that launched the app, so a snooze pressed while the app was dead is handled
  natively. The buffer covers an app without the plugin, or a detach.
- **Install guard:** `mobiler plugin add notifications` refuses, writing nothing, while the app's `App.swift`
  lacks `final class LocalNotificationBridge` (`check_local_bridge`, beside `check_push_bridge`); the message says
  to run `mobiler upgrade --apply` first. The same check also requires `Core.swift`'s `Plugins.inOrder` to name the
  plugin (ADR-0073).

**Android: the plugin handles its own taps; no template change.**
- `LocalOpens` follows `push`'s `PushOpens`: `ActivityLifecycleCallbacks` read the resumed activity's intent. It
  acts only on an intent carrying `mobiler_local_id`, removes the three extras so a tap is reported once, ignores
  an intent relaunched from Recents (`FLAG_ACTIVITY_LAUNCHED_FROM_HISTORY`), and buffers an `opened` event
  (ADR-0073), with `action_id` for an `open` button.
- **The category keeps the PendingIntents apart.** Both plugins use the notification id as the PendingIntent
  request code with `FLAG_UPDATE_CURRENT`, and extras don't take part in PendingIntent identity. Without the
  category, reminder 7's tap intent and push notification 7's would be one PendingIntent, and whichever was posted
  last would overwrite the other's extras. The category takes part in identity (`filterEquals`) but not in
  resolving the explicit launcher intent. It is a category, not intent data, because `MainActivity` reads intent
  data as a deep link.
- Snooze and dismiss go to the plugin's own non-exported `NotificationActionReceiver` (ADR-0073), never through
  the activity.

**Neither plugin names the other.** The `notifications` sources never mention `PushBridge`, `PushBus`,
`PushOpens` or `mobiler_push`; the `push` and `push-firebase-only` sources never mention `LocalNotificationBridge`,
`mobiler_local`, `LocalOpens` or `ReminderEvents`. Push's tap markers (`mobiler_push_payload`, FCM's
`google.message_id`) and the local ones are disjoint.

### 2.1. Refutation Conditions

- **Condition 1 — a marked notification never reaches `PushBridge` on iOS.**
  Both delegate methods test `isLocal` before any `PushBridge` call; the marked branch does the local call, returns
  and doesn't touch `PushBridge`; `PushBridge.shared.didReceive` is called from nowhere else in `App.swift`; the
  template's and barbershop's `LocalNotificationBridge` are the same.
  - **Validation Metric:** `adr_0072_local_notifications_never_reach_push`.
- **Condition 2 — the bridge never holds iOS's completion without a handler, and its buffer is bounded.**
  - **Validation Metric:** `adr_0072_local_notifications_never_reach_push` (the `didReceive` shape and
    `maxBuffered = 32`).
- **Condition 3 — on Android a reminder tap can't share a PendingIntent with a push, and `LocalOpens` acts only on
  its own extra.**
  - **Validation Metric:** `adr_0072_local_notifications_never_reach_push` (`reminderLaunchIntent` adds
    `mobiler.local`; every `PendingIntent.getActivity(` in the plugin is built from `openIntent`; `consume` returns
    unless `mobiler_local_id` is present).
- **Condition 4 — the plugins stay apart, and the plugin attaches at launch.**
  - **Validation Metric:** `adr_0072_local_notifications_never_reach_push` (the name lists; `app_launch` holds
    `NotificationsPlugin.bootstrap()`), `add_notifications_inserts_receivers_boot_permission_and_app_launch`,
    `add_push_still_works_beside_notifications`.
- **Condition 5 — an app without the router can't install the plugin.**
  - **Validation Metric:** `add_notifications_refuses_on_old_app_swift` (refused, nothing written).
- **Condition 6 (runtime):** on an Android emulator, a cold and a warm body tap give one `opened` with its payload;
  a push tap in the same app still reaches `push`. On the maintainer's iPhone: a cold-start tap, a lock-screen
  snooze with the app dead, a dismissal (Clear all included). The Android taps were checked on API 26 and 33
  emulators in this work; a push tap beside a reminder and the iPhone pass belong to the release's device pass.

## 3. Considered Options & Rationale for Refutation

- **Option A — a marker-based router; on iOS the template's `AppDelegate` routes to an always-present bridge**
  `[recorded: docs/superpowers/specs/2026-11-27-notifications-reminders-design.md §3, "The maintainer picked A on 2026-11-27"]`
  Chosen.
- **Option B — the plugin replaces the iOS delegate at launch and chains to the `AppDelegate`**
  `[recorded: docs/superpowers/specs/2026-11-27-notifications-reminders-design.md §3]`
  Rejected: "whoever sets the delegate last wins, so a future plugin or a launch-order change silently breaks push
  or snooze."
- **Option C — the bridge holds the completion handler until a handler attaches**
  `[recorded: docs/superpowers/specs/2026-11-27-notifications-reminders-design.md, Amendment 2026-11-27 (Task 1)]`
  The spec's first draft. Rejected in the build: the plugin attaches in `didFinishLaunching`, before iOS delivers a
  response, so the buffer only serves an app without the plugin, where the completion would never be called.
- **Option D — a template change on Android (a router in `MainActivity`)** `[recorded: docs/superpowers/specs/2026-11-27-notifications-reminders-design.md §3, "Android: no template change"]`
  Rejected: the `PushOpens` pattern already lets a plugin read its own launch intent, and the plugin's own
  receivers handle snooze and dismiss.
- **Option E — tell the Android tap PendingIntents apart by request code (pack the id and an action index)**
  `[reconstructed]`
  Rejected: ids span 1..=2³¹−1, so any packing of an id and an index into one `Int` collides; the category (and,
  for receivers, the intent action) separates them with the request code left as the id.

## 4. Decision & Rationale for Corroboration

Option A. The delegate stays in one place that every app has, so push and the reminders can't fight over it, and
the plugin still owns all of its behaviour: the bridge is a dormant forwarder. On Android the plugin needed no
template change; the category was added in review, once the tap PendingIntent was seen to collide with push's.

**Mutation proof** (2026-11-28; each mutation reverted after it was seen to fail, then the suite was green):
- Deleting the `if LocalNotificationBridge.isLocal(…) { … return }` branch from the template's `didReceive` failed
  `adr_0072_local_notifications_never_reach_push` ("mobiler/templates/iOS/Sources/App.swift's didReceive doesn't
  test isLocal").
- Replacing barbershop's `didReceive` test with `if false {` failed it ("demos/barbershop/iOS/Sources/App.swift's
  didReceive doesn't test isLocal").
- Removing the `return` from the template's marked `willPresent` branch failed it ("… willPresent marked branch
  falls through to the push path").
- Calling `PushBridge.shared.didReceive(…, kind: .opened)` before the `isLocal` test in the template's
  `didReceive` failed it ("… didReceive calls PushBridge before it tests isLocal").
- Removing `completion()` from the bridge's no-handler path failed it ("… LocalNotificationBridge must hand the
  completion to the handler, or call it at once").
- Dropping `?.addCategory(LocalNotif.CATEGORY)` from `reminderLaunchIntent` failed it ("the reminder launch intent
  must carry the mobiler.local category").
- Making `LocalOpens.consume` act on any intent (`if (intent == null) return`) failed it ("LocalOpens must act
  only on an intent carrying mobiler_local_id").
- Letting the `push` plugin's `PushOpens` read `mobiler_local_payload` failed it ("mobiler/plugins/push/android/
  PushPlugin.kt names the notifications plugin's `mobiler_local`").

## 5. Consequences (Positive and Negative Predictions)

- **Positive:** a reminder tap no longer surfaces as a push `opened` on iOS, and a push tap never becomes a reminder
  event unless the app's own push sender puts `mobiler_local` in it (iOS only, see the Negative below).
- **Positive:** push-less and notifications-less apps compile and behave as before; the bridge costs one dormant
  object.
- **Positive:** a snooze or dismissal while the app is dead is handled natively on both platforms (ADR-0036).
- **Negative:** an existing app needs both halves: `mobiler upgrade --apply` for the router, then
  `mobiler plugin add notifications` again for the new plugin body, since upgrade never rewrites a plugin
  (ADR-0011). The install guard makes the order explicit, but an app that skips the re-add keeps the old plugin.
- **Negative:** the key `mobiler_local` is reserved in a notification's `userInfo`. A server push whose payload
  carries it would be routed to the `notifications` plugin on iOS. When the value is a number whose `intValue` is 1
  or more (`reminderId` reads any `NSNumber`, so `true` and `1.5` count too), it becomes an event there. Nothing
  stops a server sending it. Such a push that also carries a registered category and `actions` can drive a
  snooze too. Only the app's own push sender can do this; another app can't send to it.
- **Negative:** an iOS reminder scheduled by the pre-marker plugin and still pending across the update is unmarked:
  its tap still takes the push path, as before, until it fires or is replaced. `cancelAll` does reach it
  (identifier `mobiler.<digits>`, ADR-0073).
- **Negative:** a third-party SDK that sets or swizzles the notification-center delegate is outside this guarantee.
  With `push-firebase-only`, the iPhone pass has to confirm a reminder tap still reaches the plugin.
- **Negative:** on Android, the launcher activity is exported, so another app can launch it with crafted
  `mobiler_local_*` extras and produce an `opened` event. The README tells apps to treat the payload as untrusted
  input, like a deep link.
- **Negative:** the iOS side is compiled only by CI and checked on the maintainer's device; no simulator test
  drives the router.
