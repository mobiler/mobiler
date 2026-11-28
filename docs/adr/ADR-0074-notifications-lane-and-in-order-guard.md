# ADR-0074: The `notifications` plugin keeps ADR-0073's contract, except that its Android registry, alarm and tray ops run off the main thread on one single-thread lane (`ReminderLane`) in the order the shell starts them, plugin init queues its re-arm on that lane, and `mobiler plugin add` accepts a `Plugins.inOrder` declaration laid out over any number of lines

Status:        Accepted
Date decided:  2026-11-28
Deciding PRs:  #TBD (CLI 0.76.1)
Supersedes:    ADR-0073
Code anchor:   mobiler/plugins/notifications/android/ReminderRegistry.kt (`ReminderLane`), mobiler/plugins/notifications/android/NotificationsPlugin.kt (`handle`, `init`), mobiler/src/plugin.rs (`check_local_bridge`, `in_order_names`, `swift_without_comments`, `swift_array_strings`), demos/barbershop (the same plugin, ADR-0015); everything else as listed in ADR-0073's `Code anchor:`
Conformance:   xtask/tests/adr_conformance.rs::adr_0074_android_registry_ops_run_on_the_lane, mobiler/src/plugin.rs::add_notifications_refuses_on_old_core_swift, mobiler/src/plugin.rs::add_notifications_accepts_a_multi_line_in_order, mobiler/src/plugin.rs::add_notifications_refuses_a_multi_line_in_order_naming_it_in_a_comment, mobiler/src/plugin.rs::in_order_names_reads_the_declaration_only, xtask/tests/adr_conformance.rs::adr_0073_plugin_declares_rearm_and_marks_every_request, mobiler/src/plugin.rs::add_notifications_inserts_receivers_boot_permission_and_app_launch

## 1. Context (The Problem)

ADR-0073 (CLI 0.76.0) fixed the `notifications` plugin's contract. The release review found two places where the
code had to change, which changes three sentences of that record:

- **Android ran every op on the main thread.** Core.kt starts each plugin call on the main dispatcher, and
  `NotificationsPlugin.handle` did its work there without suspending. That is how it kept the calls in issue order.
  But a re-plan (`cancelAll` and about 40 `schedule`s, the case the requesting app has) then made about 40
  `commit()`ed SharedPreferences writes and as many AlarmManager and NotificationManager binder calls on the main
  thread. While plugin init's re-arm thread held the registry lock, the main thread also waited on that lock.
  ADR-0065 had already named this risk for `sqlite` ("which also blocked the UI on every query") and solved it with
  a lane. `securestore` (CLI 0.66.1) uses the same lane.
- **The install guard read only a one-line `inOrder`.** `in_order_names` looked for a single line holding both
  `static let inOrder` and `"notifications"`. An app that had reformatted the set across several lines was refused,
  even though it named the plugin.

A third, smaller problem: one mutation proof in ADR-0073 §4 names code that no longer exists, so it can't be re-run.

Merged ADRs are immutable, so this record supersedes ADR-0073 and restates only what changes.

## 2. Hypothesis

**ADR-0073's contract stays in force unchanged, except for the three clauses below.** The ops and failure codes,
the `schedule` input rules, daylight saving and zones, the Android registry, alarms, markers and receivers, the iOS
side, snooze, permission, the `events` stream, the refutation conditions and the consequences all still apply.
**For everything not replaced here, read ADR-0073**: superseding it doesn't retire that contract. This record also
cites ADR-0073's conformance tests, so the contract stays guarded.

**(a) Android op order.** ADR-0073 §2, "Android: the registry", said:

> The ops, the firing receiver, the snooze and the re-arm all hold one lock (`synchronized(ReminderRegistry)`)
> around their registry and alarm work, and the ops that touch the registry run without suspending, in the order
> the shell starts them (ADR-0065's Android path).

Replaced by:

- The ops that touch the registry, the alarms or the tray (`schedule`, `cancel`, `cancelAll`, `listPending`,
  `removeDelivered`) run through `ReminderLane.run { … }` (ReminderRegistry.kt).
- `ReminderLane` is one `Executors.newSingleThreadExecutor` per process (thread `mobiler-notifications`), used as a
  coroutine dispatcher and entered with `withContext(NonCancellable + dispatcher)`.
- Why the order holds: Core.kt starts each call on the main dispatcher in the order the core issued it, with FIFO
  launches and replies processed in their own looper turn (ADR-0065 §2). `withContext` hands the block to the
  executor before the call first suspends. So each call joins the lane's queue in issue order, and the single
  thread runs the calls one at a time, each to completion without suspending.
- The ops, the firing receiver, the snooze and the re-arm still hold `synchronized(ReminderRegistry)` around their
  registry and alarm work. `cancelAll` followed by 40 `schedule`s still leaves exactly those 40.
- `NonCancellable`: a call already queued still runs when the screen's scope is cancelled, so a re-plan isn't cut
  off halfway.
- `requestPermission`, `permissionStatus` and `openSettings` need the activity, or only read the permission state,
  and touch no registry, alarm or tray state. They stay on the main dispatcher where the shell calls them.
- **Plugin init** queues its re-arm on the lane (`ReminderLane.post { runCatching { rearmAll(…) } }`) instead of
  starting a thread of its own. The plugin is built in the ViewModel's `plugins` map, which is initialised before
  its `init` block sends AppInfo, Restore and Start. So the re-arm is queued before any op, and the app's first
  calls run after it.
- The receivers (`NotificationReceiver`, `NotificationRearmReceiver`, `NotificationActionReceiver`) keep their own
  paths; the registry lock orders them against the lane.
- The lane's own behaviour is also unit-tested on the plain JVM, run by hand with Gradle, not in CI:
  `demos/barbershop/Android/app/src/test/java/dev/mobiler/barbershop/ReminderLaneTest.kt`.
  - `callsRunOnOneLaneThreadInLaunchOrder`: 200 calls launched from one thread, with a `post` queued first, run in
    launch order on one lane thread, never the caller's.
  - `aQueuedCallStillRunsWhenTheScopeIsCancelled`: calls queued before their scope is cancelled still all run, in
    order.

**(b) The install guard.** ADR-0073 §2, iOS, said:

> `mobiler plugin add notifications` refuses, writing nothing, while the app's `Core.swift` has no live
> `static let inOrder` line naming `"notifications"` (`check_local_bridge`), and says to run
> `mobiler upgrade --apply` first.

Replaced by: `mobiler plugin add notifications` refuses, writing nothing, while the app's `Core.swift` has no live
`static let inOrder` **declaration** naming `"notifications"`, on one line or across several (`check_local_bridge`).
It still says to run `mobiler upgrade --apply` first.
- "Live" means after removing `//` line comments and `/* */` block comments (nested, as Swift allows) in
  `swift_without_comments`. A string literal is copied unchanged, so a `//` inside one is not a comment.
- "Naming" means a whole string literal equal to `notifications` between the declaration's `[` and its matching
  `]` (`swift_array_strings`). A name only in a comment, only after the `]`, or only as part of a longer literal
  doesn't count.

**(c) The stale mutation note.** ADR-0073 §4 said:

> so did letting `in_order_names` count `//`-commented lines (`.filter(|_l| true)`), since the fixture names
> `notifications` only in a comment.

That `.filter` no longer exists. The current mutation proofs of the guard, in §4 below, replace this note.

### 2.1. Refutation Conditions

- **Condition 1 — the Android registry ops run on the lane.**
  The following hold on live code (whole-line, block and trailing `//` comments removed):
  - `handle` sends each of `schedule`, `cancel`, `cancelAll`, `listPending` and `removeDelivered` through
    `ReminderLane.run { … }`, and keeps `requestPermission` and `openSettings` off it;
  - `ReminderLane` is one `newSingleThreadExecutor`, with no pool or shared dispatcher, and `run` enters it with
    `withContext(NonCancellable + dispatcher)`;
  - plugin init queues `rearmAll` with `ReminderLane.post`, and NotificationsPlugin.kt starts no `Thread(`;
  - barbershop's two files equal the bundled ones.
  - **Validation Metric:** `adr_0074_android_registry_ops_run_on_the_lane`.
- **Condition 2 — the guard accepts any layout and only live names.**
  - **Validation Metric:** `add_notifications_accepts_a_multi_line_in_order` (a multi-line `inOrder` with comments
    inside installs), `add_notifications_refuses_a_multi_line_in_order_naming_it_in_a_comment` (the name only in a
    `//`, a block or a nested block comment: refused, nothing written), `add_notifications_refuses_on_old_core_swift`
    (the one-line case), `in_order_names_reads_the_declaration_only`.
- **Condition 3 — the rest of ADR-0073 holds.** ADR-0073's refutation conditions apply unchanged.
  - **Validation Metric:** `adr_0073_plugin_declares_rearm_and_marks_every_request`,
    `add_notifications_inserts_receivers_boot_permission_and_app_launch`, and ADR-0073's JVM tests, run by hand.

## 3. Considered Options & Rationale for Refutation

- **Option A — keep the ops on the main thread** `[recorded: the coordinator's release review of #324, 2026-11-28, quoted here]`
  "a re-plan does ~40 SharedPreferences `commit()` writes plus AlarmManager/NotificationManager binder calls on the
  main thread, and Main also waits on the registry lock while the init re-arm thread works." Rejected: about 40 committed writes and binder calls per re-plan on the UI thread, plus a wait on the registry
  lock while init re-arms.
- **Option B — `Dispatchers.IO`** `[recorded: ADR-0065 §1]`
  Rejected for the reason ADR-0065 gives for `sqlite`: a shared pool lets two calls finish in the other order, so a
  `schedule` could land before the `cancelAll` issued ahead of it.
- **Option C — keep init's re-arm on its own thread** `[reconstructed]`
  Rejected. It races the app's first calls for the lock: which one runs first depends on thread scheduling. On the
  lane it always runs first, and one thread fewer exists.
- **Option D — receivers on the lane too** `[reconstructed]`
  Rejected. A receiver runs with no app UI, often in a process the system just started, under `goAsync`'s deadline.
  Queuing it behind a re-plan would add delay for no ordering gain, because the lock already orders it.
- **Option E — require the one-line `inOrder` and document it** `[reconstructed]`
  Rejected. The guard exists to catch a Core.swift that doesn't run the plugin in order, not one whose formatting
  differs from the template's.

## 4. Decision & Rationale for Corroboration

The three replacements in §2, with everything else from ADR-0073 unchanged. On an API 33 emulator (2026-11-28),
barbershop's "Re-plan 40" device check reported `before 0 → 40 ok, 0 failed; pending: 40` and then
`before 40 → 40 ok, 0 failed; pending: 40`. A two-minute `at` reminder fired. After a force-stop and relaunch, the
lane's re-arm set all 40 alarms again.

**Mutation proof** (2026-11-28; each mutation reverted after it was seen to fail, then the suite was green):
- `"cancel" -> cancel(input)` in `handle` (off the lane) failed `adr_0074_android_registry_ops_run_on_the_lane`
  ("NotificationsPlugin.handle doesn't run `cancel` on ReminderLane").
- Turning the `listPending` branch into `listPending() // "listPending" -> ReminderLane.run { listPending() }` (the
  lane call only in a trailing comment) failed it ("… doesn't run `listPending` on ReminderLane").
- `Executors.newFixedThreadPool(4)` for the lane failed it ("ReminderLane must be one single-thread executor").
- `withContext(dispatcher)` without `NonCancellable` failed it ("ReminderLane.run must enter the lane
  NonCancellable").
- Restoring init's own `Thread({ … rearmAll … }).start()` failed it ("plugin init must queue rearmAll on
  ReminderLane").
- Appending a line to barbershop's ReminderRegistry.kt failed it ("barbershop's ReminderRegistry.kt differs from
  the bundled plugin").
- Making `in_order_names` read the raw text instead of `swift_without_comments(core_swift)` failed
  `add_notifications_refuses_on_old_core_swift` and
  `add_notifications_refuses_a_multi_line_in_order_naming_it_in_a_comment`. This replaces ADR-0073's
  `.filter(|_l| true)` note.
- The pre-change one-line `in_order_names` refuses `add_notifications_accepts_a_multi_line_in_order`'s fixture by
  construction: no single line there holds both `static let inOrder` and `"notifications"`.
- (JVM, by hand) Dropping `NonCancellable` failed `aQueuedCallStillRunsWhenTheScopeIsCancelled`. A 4-thread pool
  failed both `ReminderLaneTest` tests.

## 5. Consequences (Positive and Negative Predictions)

- **Positive:** a re-plan no longer does disk or binder work on Android's main thread, and the main thread never
  waits on the registry lock for the init re-arm.
- **Positive:** the init re-arm runs before the app's first calls every time, instead of racing them.
- **Positive:** an app whose `inOrder` is formatted across lines can install the plugin.
- **Negative:** every registry op costs a thread hop and back, and one slow op (a binder call stalled by the
  system) holds up every later one. These are small local operations, so this is the price of the order, as for
  `sqlite`.
- **Negative:** `requestPermission`, `permissionStatus` and `openSettings` are outside the lane, so their answers can
  arrive before those of lane ops issued earlier. They share no state with those ops, and the core matches answers
  by request id, so no result changes. Only the order the answers arrive in differs.
- **Negative:** the force-stop burst that ADR-0073 §5 describes no longer races the re-plan: it always happens first.
  Every missed reminder within `late_minutes` is posted before the app's re-plan can cancel it. The README's advice
  to lower `late_minutes` stands.
- **Negative:** the lane thread lives for the process and is never shut down.
- **Negative:** the guard's Swift scanner knows ordinary `"…"` literals, `//` and nested `/* */`, not raw (`#"…"#`)
  or multi-line (`"""…"""`) string literals. Such a literal before the declaration that contains `/*` can hide the
  declaration (a wrong refusal). One that contains a whole `static let inOrder … = ["notifications"]` would be read
  as live (a wrong acceptance). Neither occurs in the template's Core.swift.
- **Negative:** two records now describe one plugin: a reader has to read ADR-0073 with this record's three
  replacements applied.
