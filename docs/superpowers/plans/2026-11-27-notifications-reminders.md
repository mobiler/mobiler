# Notifications reminders Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Upgrade the bundled `notifications` plugin into a reliable reminder system: an Android registry with reboot
re-arm, `at` wall-clock times, management ops, permission status/settings, action buttons with native snooze, and a
buffered events stream. Local notification taps are routed away from `push` on iOS.

**Architecture:** All behaviour lives in the plugin (`mobiler/plugins/notifications/{android,ios}`). On iOS only, the
template `App.swift` gains a plugin-agnostic `LocalNotificationBridge`, and the `AppDelegate` routes notifications
marked `mobiler_local` to it. Android reaches taps the way `push`'s `PushOpens` does. It ships as one CLI release
(0.76.0) with no library or ABI change.

**Tech Stack:** Kotlin (AlarmManager, NotificationCompat, SharedPreferences, java.time), Swift (UserNotifications),
Rust CLI (plugin manifest, `plugin.rs` guard, xtask conformance).

**Spec:** `docs/superpowers/specs/2026-11-27-notifications-reminders-design.md` (approved 2026-11-27). Read it
first. Its §4 holds the exact op contract, JSON shapes and per-platform behaviour that every task below implements.

## Global Constraints

- The op contract is spec §4.1–4.2, and the event schema is §4.5 verbatim. Failure outputs are `<code>` or
  `<code>: <detail>`, with codes from `invalid_input`, `past`, `pending_limit`, `unsupported`, `failed` only.
- Existing callers stay unchanged:
  - `schedule {id,title,body,after_seconds}` and `cancel {id}` behave as today;
  - a missing `id` defaults to 1;
  - the one deliberate change is that an Android body tap now opens the app.
- Markers:
  - iOS: `userInfo["mobiler_local"] = <id>` and request identifier `mobiler.<id>`;
  - Android: intent extras `mobiler_local_id`, `mobiler_local_action`, and `mobiler_v2=true` on every new alarm.
- Android registry: SharedPreferences `mobiler_notifications`, key `r.<id>`, value JSON per spec §4.3, written
  with `commit()`.
- Event buffer: at most 100 events, oldest dropped. Android prefs key `events`; iOS UserDefaults key
  `mobiler.notifications.events`. At-least-once into the stream.
- Limits:
  - ≤ 3 actions; kinds `open` and `snooze`; `snooze_minutes` 1..=1440;
  - action id non-empty and ≤ 64 bytes, unique; label non-empty;
  - payload ≤ 1024 UTF-8 bytes; `late_minutes` 0..=10080, default 1440;
  - iOS pending limit 64 (`pending_limit: 64 pending notifications on iOS`).
- `late_minutes` applies only to re-arms (boot, update, time or zone change, plugin init), never when the alarm itself fires.
- Alarms stay inexact (`setAndAllowWhileIdle`); no `SCHEDULE_EXACT_ALARM`. Android minSdk 26 (ADR-0039).
- The core never runs while the app is closed (ADR-0036). Snooze, dismiss and re-arm are native only.
- Barbershop first, then templates (ADR-0015). CLI-managed demos are upgraded (ADR-0063).
- No `mobiler-web` change: web keeps "plugin not available".
- Builds:
  - Use per-app `CARGO_TARGET_DIR` under `/media/zmilan/data2`, never a shared one, and check `df -h /` (stop under 5 GB).
  - Use JDK 21 and `ANDROID_HOME=/home/zmilan/Android/Sdk`.
  - Use your own emulator on port 5560; emulator-5554 belongs to another session.
  - No `pgrep -f` or marker-file polling.
- iOS can't be compiled on this host. Write Swift with care: CI's iOS lanes are the first compile.
- Commits use `-F`/a quoted heredoc and end with `Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>`.
  Never commit `docs/requests/` or `.superpowers/`.

## Review Focus

1. **`cancelAll` then ~40 `schedule` calls, in quick succession (Quit Sitting's re-plan on every open).** It must
   leave exactly the new set pending, with no stale snooze and no race between registry writes and alarm cancels.
   Test in Task 2, checked on the emulator in Task 6.
2. **A snooze pressed, then the app opened and re-planned before the snooze fires.** The snooze is cancelled
   (cancelAll/replace) and never fires afterwards. Covered in Task 3 and Task 6.
3. **A cold-start tap while the core subscribes late, or never.** `opened` waits in the buffer, survives a process
   kill before subscribe, and is delivered exactly once on the next subscribe. Covered in Task 3 (Android) and
   Task 4 (iOS).
4. **An `at` in a DST gap or overlap, and a time-zone change after scheduling.** The gap fires right after it, the
   overlap fires at the earlier instant, and a zone change re-computes `fire_at` on Android. Covered in Task 2.
5. **A payload or label with quotes, newlines and non-ASCII.** It round-trips byte-exact through the registry, the
   notification and the event JSON. Covered in Task 2 (Android) and Task 4 (iOS).

---

### Task 1: iOS router in barbershop's `App.swift`

**Files:**
- Modify: `demos/barbershop/iOS/Sources/App.swift` (the AppDelegate's `willPresent`/`didReceive`, plus a new
  `LocalNotificationBridge` next to `PushBridge`)

**Interfaces:**
- Produces, in Swift, `@MainActor final class LocalNotificationBridge`:
  - `static let shared`;
  - `func attach(_ handler: @escaping (UNNotificationResponse, @escaping () -> Void) -> Void)`;
  - `func detach()`;
  - `func didReceive(_ response: UNNotificationResponse, completion: @escaping () -> Void)`, which calls the handler,
    or, when no handler is attached, buffers `(response, completion)` in memory and replays it on `attach`;
  - `static func isLocal(_ userInfo: [AnyHashable: Any]) -> Bool`, true when `userInfo["mobiler_local"] != nil`.
- AppDelegate rule:
  - `willPresent` with a local notification → `completionHandler([.banner, .sound])`, with no PushBridge call;
  - `didReceive` with a local notification → `LocalNotificationBridge.shared.didReceive(response, completion: completionHandler)`;
  - everything else unchanged.

- [ ] **Step 1:** Implement the bridge and the routing. The bridge references nothing from any plugin, so an app
  without the plugin compiles. Comment it in the style of `PushBridge`.
- [ ] **Step 2:** Diff barbershop's `App.swift` against the template's. The only new differences are this feature's.
- [ ] **Step 3:** Commit. Swift is compiled by CI's barbershop iOS lane, so say so in the commit body.

### Task 2: Android plugin — registry, `schedule`, management, re-arm

**Files:**
- Modify: `mobiler/plugins/notifications/android/NotificationsPlugin.kt` (ops: `schedule`, `cancel`,
  `cancelAll`, `listPending`, `removeDelivered`)
- Create: `mobiler/plugins/notifications/android/ReminderRegistry.kt`: the registry, input validation, and
  wall-clock parsing
- Create: `mobiler/plugins/notifications/android/NotificationReceivers.kt`, holding:
  - `NotificationReceiver`, moved here and extended: it posts from the registry, keeps the legacy path, and builds
    the content intent;
  - `NotificationRearmReceiver`
- Modify: `mobiler/plugins/notifications/mobiler-plugin.toml`, adding:
  - the new `sources`;
  - permission `android.permission.RECEIVE_BOOT_COMPLETED`;
  - a `<receiver android:name=".NotificationRearmReceiver" android:exported="true">` with intent-filters for
    `BOOT_COMPLETED`, `MY_PACKAGE_REPLACED`, `TIMEZONE_CHANGED`, `TIME_SET`.

**Interfaces:**
- `object ReminderRegistry` (Kotlin):
  - `fun put(ctx: Context, r: Reminder)`
  - `fun get(ctx: Context, id: Int): Reminder?`
  - `fun remove(ctx: Context, id: Int)`
  - `fun all(ctx: Context): List<Reminder>`
  - `fun clear(ctx: Context)`
- `data class Reminder(id: Int, title: String, body: String, fireAtMs: Long, wall: String?, payload: String?,
  actionsJson: String?, lateMinutes: Int, snoozed: Boolean)`, with `fun toJson(): String` and
  `companion fun fromJson(s: String): Reminder?`.
- `fun parseSchedule(input: String, nowMs: Long, zone: ZoneId): Result<Reminder>`. The failure message is the
  spec's `<code>: <detail>`, and validation follows spec §4.2 exactly.
- `fun wallToEpochMs(wall: String, zone: ZoneId): Long?`, built with `ZonedDateTime.of(LocalDateTime, zone)`:
  gap → after, overlap → earlier.
- `fun armAlarm(ctx: Context, r: Reminder)` and `fun cancelAlarm(ctx: Context, id: Int)`: request code `id`, the
  extras from the Global Constraints, `FLAG_IMMUTABLE`.
- `fun rearmAll(ctx: Context, nowMs: Long)`, used by the re-arm receiver and plugin init: re-compute `wall`, then
  arm the future ones, post the ones late within `late_minutes`, and drop the rest.

- [ ] **Step 1:** Put the pure logic (`parseSchedule`, `wallToEpochMs`, `Reminder` JSON) where it can be
  unit-tested on the JVM. If barbershop's Android project has no JVM unit-test setup, add a minimal `testImplementation("junit:junit:4.13.2")`
  test source set to barbershop only (not the template), with tests copied from the plugin's logic file. Then
  write failing tests:
  - `missingIdDefaultsTo1`
  - `bothAtAndAfterIsInvalid`
  - `pastAtIsPast`
  - `fourActionsInvalid`
  - `snoozeMinutesBounds` (0 and 1441 invalid)
  - `duplicateActionIdInvalid`
  - `payloadOver1024BytesInvalid` (1024 OK, 1025 invalid, counted in UTF-8 bytes)
  - `lateMinutesDefault1440`
  - `gapResolvesAfter`: `Europe/Belgrade` `2027-03-28 02:30` → 03:00 local
  - `overlapResolvesEarlier`: `Europe/Belgrade` `2027-10-31 02:30` → the +02:00 instant
  - `reminderJsonRoundTripsQuotesNewlinesNonAscii` (Review Focus 5)
- [ ] **Step 2:** Run `./gradlew :app:testDebugUnitTest` in demos/barbershop/Android. Expect them to FAIL.
- [ ] **Step 3:** Implement. `NotificationReceiver` behaviour:
  - a legacy intent (no `mobiler_v2`) posts from its `title`/`body` extras;
  - otherwise it reads the registry: no entry → no post; entry → post with a content intent (launcher activity
    plus the `mobiler_local_*` extras) and remove the entry.
  
  Plugin init calls `rearmAll`.
- [ ] **Step 4:** Tests pass. Install into barbershop with the workspace CLI: `mobiler plugin add notifications`
  from demos/barbershop. Barbershop gains the plugin here (spec §4.6). `assembleDebug` builds.
- [ ] **Step 5:** Emulator check (API 34), all on barbershop through a temporary scratch call or the Task 6 card,
  whichever exists:
  - schedule `at` now+2 min → it fires;
  - `cancelAll` followed at once by 40 schedules → `listPending` has exactly 40 entries (Review Focus 1);
  - reboot the emulator → a pending reminder still fires;
  - a zone change → the wall reminder is re-computed.
  
  Record the outputs.
- [ ] **Step 6:** Commit.

### Task 3: Android plugin — actions, snooze, dismiss, taps, events, permission ops

**Files:**
- Modify: `mobiler/plugins/notifications/android/NotificationReceivers.kt` (action `PendingIntent`s and the delete
  intent; new `NotificationActionReceiver`)
- Create: `mobiler/plugins/notifications/android/ReminderEvents.kt`: `object ReminderEvents`, the persisted
  buffer plus the sink, and `object LocalOpens` (the `PushOpens` pattern)
- Modify: `NotificationsPlugin.kt`: `subscribe("events")`, `permissionStatus`, `openSettings`, and
  `requestPermission` that waits for the result
- Modify: `mobiler-plugin.toml`, adding `<receiver android:name=".NotificationActionReceiver" android:exported="false"/>`

**Interfaces:**
- `object ReminderEvents`:
  - `fun emit(ctx: Context, type: String, id: Int, actionId: String?, payload: String?)` builds the spec §4.5 JSON
    with `at` as local `yyyy-MM-dd HH:mm:ss`;
  - `fun attach(ctx: Context, sink: (String) -> Unit): Int` flushes in order, removing each event after the sink
    returns;
  - `fun detach(id: Int)`.
- `object LocalOpens { fun install(app: Application) }` consumes `mobiler_local_id`/`mobiler_local_action`,
  ignores `FLAG_ACTIVITY_LAUNCHED_FROM_HISTORY`, and removes the extras after consuming them.
- A snooze rewrites the registry entry (`snoozed=true`, new `fireAtMs`), re-arms it, cancels the tray entry and
  emits `snoozed`. Dismiss emits `dismissed`.
- `requestPermission` (API 33+): register a one-shot `ActivityResultCallback` on `MobilerActivity.current`'s
  `activityResultRegistry` (key `mobiler_notif_perm`), launch it, and await it (≤ 60 s) with
  `suspendCancellableCoroutine`. It sets the "asked once" pref (`asked=true`). No template change. If this can't
  work without one, stop and report BLOCKED.

- [ ] **Step 1:** Write the failing JVM tests for `ReminderEvents` (pure buffer logic with a fake prefs map):
  - `bufferCapsAt100DropsOldest`
  - `flushInOrderThenEmpty`
  - `crashMidFlushRedeliversRemainder`: throw from the sink after 2
  - `eventJsonShape`, which checks field names, omits `action_id`/`payload` when null, and matches the `at` format
- [ ] **Step 2:** FAIL, then implement, then PASS.
- [ ] **Step 3:** Emulator check (API 34; and API 26 for snooze and tap):
  - Snooze with the app swiped away → it re-posts after `snooze_minutes` (use 1).
  - A tap, cold and warm → exactly one `opened` with the payload on the next subscribe (Review Focus 3).
  - Swipe away → `dismissed`.
  - Snooze, then open the app and `cancelAll` → the snooze never fires (Review Focus 2).
  - `permissionStatus` on a fresh install → `notDetermined`; after allow → `granted`; after deny → `denied`.
  - `openSettings` lands on the app's notification settings.
  
  Record it all.
- [ ] **Step 4:** Commit.

### Task 4: iOS plugin

**Files:**
- Modify: `mobiler/plugins/notifications/ios/NotificationsPlugin.swift`, splitting it into
  `NotificationsPlugin.swift` (ops) and `NotificationsEvents.swift` (buffer + handler) if it grows past ~250 lines
- Modify: `mobiler-plugin.toml`, adding:
  - `register_stream = 'case "notifications": await NotificationsPlugin.subscribe(op: op, input: input, emit: emit)'`;
  - `app_launch = ["NotificationsPlugin.bootstrap()"]`;
  - any new sources.

**Interfaces:**
- `static func handle(op:input:) async -> PluginResponse`, for every op in spec §4.1.
- `static func subscribe(op:input:emit:) async`, for `events`.
- `static func bootstrap()`: `LocalNotificationBridge.shared.attach(handler)` and category re-registration.
- Category identifier: `"mobiler.c." + fnv1a64Hex(canonicalActionsJson)`. This includes the empty set `"[]"`.
  Always `.customDismissAction`. `snooze` actions have no `.foreground`; `open` actions use `.foreground`.
- The validation rules and failure strings are identical to Android's, from spec §4.2.
- `at` → `UNCalendarNotificationTrigger(dateMatching: year…second in Calendar.current, repeats: false)`. A past
  time → `past`.
- Pending limit: before adding a new id, count `getPendingNotificationRequests()`. ≥ 64 and the id is not
  already pending → `pending_limit: 64 pending notifications on iOS`.

- [ ] **Step 1:** Implement against the Task 1 bridge API. Read the Swift as a compiler would. Payload, title and
  labels are carried as plain `String` in `userInfo` and JSON-encoded with `JSONSerialization`, never string
  concatenation (Review Focus 5).
- [ ] **Step 2:** Install into barbershop iOS with `mobiler plugin add notifications` (already added in Task 2,
  so re-add). Commit. CI compiles it; the maintainer's iPhone pass is in Task 9.

### Task 5: CLI — install guard, manifest tests, README, usage

**Files:**
- Modify: `mobiler/src/plugin.rs`: `check_local_bridge`, modelled on `check_push_bridge`, called at the same place
- Modify: `mobiler/plugins/notifications/README.md` and `app-core-usage.rs`: every op, the event schema, Quit
  Sitting-style re-plan usage, force-stop, `late_minutes`, platform differences, and the 64 limit
- Test: `mobiler/src/plugin.rs` tests

**Interfaces:**
- `fn check_local_bridge(root: &Path, src: &Source, manifest: &Manifest) -> Result<()>`. It bails with
  "`notifications` needs a newer iOS/Sources/App.swift (LocalNotificationBridge). Run `mobiler upgrade --apply`
  first, then re-run `mobiler plugin add notifications`." when any iOS source references
  `LocalNotificationBridge` and the app's `App.swift` lacks `final class LocalNotificationBridge`.

- [ ] **Step 1:** Write the failing tests:
  - `add_notifications_inserts_receivers_boot_permission_and_app_launch`: on a scaffolded app with the new
    template, the manifest has all three receivers and `RECEIVE_BOOT_COMPLETED`, and the iOS app-launch anchor
    has `NotificationsPlugin.bootstrap()`;
  - `add_notifications_refuses_on_old_app_swift`;
  - `add_push_still_works_beside_notifications`.
- [ ] **Step 2:** FAIL, then implement, then PASS. Run `cargo test -p mobiler`. Commit.

### Task 6: Barbershop "Reminders" card + full emulator pass

**Files:**
- Modify: barbershop's app core (`demos/barbershop/app-core/src/lib.rs`) to add a "Reminders" card. It has
  buttons for:
  - request permission;
  - show status;
  - schedule a reminder at now+2 min with Snooze 1 min + Open;
  - list pending;
  - cancel all.
  
  An events log shows the last 10 `events` emissions; subscribe on start.
- Modify: `demos/barbershop/README.md`.

- [ ] **Step 1:** Implement the card, following the existing card style. Run `cargo test` in barbershop core.
- [ ] **Step 2:** Run the emulator end-to-end on API 34 and API 26, covering Review Focus 1–4 through the card.
  Take screenshots into the scratchpad. Commit.

### Task 7: Template + demos

**Files:**
- Modify: `mobiler/templates/iOS/Sources/App.swift`, ported from barbershop's Task 1 diff
- Demos:
  - `mobiler upgrade --apply` in saldo, coffee, todo and fullstack-todo/mobile;
  - in `demos/coffee`, re-add `mobiler plugin add notifications`, since coffee uses the plugin.

- [ ] **Step 1:** Port. Diff the template `App.swift` against barbershop's: only the pre-existing differences may
  remain. Upgrade the demos and re-add the plugin in coffee. Coffee's Android must still build.
- [ ] **Step 2:** `cargo test -p xtask` (ADR-0063) and `cargo test --workspace` are green. Commit.

### Task 8: ADR-0072, ADR-0073, conformance

**Files:** Create `docs/adr/ADR-0072-local-notification-routing.md` and
`docs/adr/ADR-0073-notifications-contract.md`. Modify `docs/adr/index.md` and `xtask/tests/adr_conformance.rs`.

- [ ] **Step 1:** Write the ADRs from the spec (§3 approaches, §4 decisions) as the code implements them.
  - ADR-0072: the marker, the template router and the Android `LocalOpens` path; push and local never cross.
  - ADR-0073: the op contract and failure codes, the registry, the `late_minutes` re-arm-only rule, the legacy
    alarm path, the event schema and at-least-once-into-the-stream, the 64 limit, and the empty-set category.
- [ ] **Step 2:** Conformance tests:
  - `adr_0072_local_notifications_never_reach_push`: in both template and barbershop `App.swift`, each delegate
    method checks `isLocal` before any `PushBridge` call;
  - `adr_0073_plugin_declares_rearm_and_marks_every_request`: the manifest has the 4 broadcast actions and
    `RECEIVE_BOOT_COMPLETED`; every `UNNotificationRequest(` in the iOS plugin and every `armAlarm` path sets the
    marker (a source check).
- [ ] **Step 3: Mutation proof.**
  - Drop `isLocal` from `didReceive` → `adr_0072_…` fails.
  - Drop `BOOT_COMPLETED` → `adr_0073_…` fails.
  - Remove the marker from one request → `adr_0073_…` fails.
  
  Record each in §4. Revert.
- [ ] **Step 4:** `cargo test -p xtask`. Commit.

### Task 9: Docs, version, verify, ship

- [ ] **Step 1:** Update the docs:
  - the root `README.md` and `mobiler/README.md` plugin rows;
  - the site plugin/guide pages (`git grep -n -i 'notifications' -- site/src/content/docs '*README.md'`);
  - the upgrade note: existing apps run `mobiler upgrade --apply`, then re-add the plugin.
  
  Bump the CLI to 0.76.0, plus the Cargo.lock and the demo `.mobiler/version` files via `upgrade --apply`. Run
  `node scripts/sync.mjs` in site/. Commit.
- [ ] **Step 2:** Final whole-branch code + security review (Fable). Fix it in one wave, re-review, then open the
  PR with `ship-pr`. Fill the ADR `Deciding PRs:` lines with the PR number before merge. The merge and the `v0.76.0`
  tag happen with the maintainer's go, followed by `post-release`.
- [ ] **Step 3:** The maintainer's iPhone pass after TestFlight:
  - snooze with the app killed;
  - Open and a body tap;
  - swipe → `dismissed`;
  - events after a relaunch;
  - a 65th new id → `pending_limit`.
  
  Then delete the five `docs/requests/2026-11-26-notification-*.md` docs.
