# ADR-0065: The bundled `sqlite` plugin runs its calls one at a time, in the order the core issued them, on one connection; a write's `ok: true` means it is committed before the next call starts

Status:        Accepted
Date decided:  2026-10-20
Deciding PRs:  #310
Supersedes:    none
Code anchor:   mobiler/plugins/sqlite/{android/SqlitePlugin.kt,ios/SqlitePlugin.swift}, mobiler/templates/iOS/Sources/Core.swift (`Plugins.inOrder`, `Core.start`), mobiler/templates/Android/app/src/main/java/__PACKAGE_PATH__/Core.kt (`process`), demos/barbershop (the same shell code and plugin, ADR-0015)
Conformance:   xtask/tests/adr_conformance.rs::adr_0065_sqlite_calls_run_in_issue_order, xtask/tests/adr_conformance.rs::adr_0063_every_demo_but_barbershop_is_cli_managed

## 1. Context (The Problem)

An app's core writes and then reads in one update: Saldo's recurring rules post their transactions and
then reload everything (`materialize` → `load_all`), a restore replays its statements and then reloads,
and every migration step is an `exec` the next one depends on. The core issues these as separate
`cx.plugin("sqlite", …)` calls and expects them to run in that order.

Nothing promised it. The iOS shell started every request in its own `Task`, and `SqlitePlugin.handle`
was a nonisolated async function that opened a fresh connection per call, so the calls reached SQLite
in whatever order the cooperative pool ran them. A reload could run before the writes it was meant to
see. In Saldo that reload could find a recurring rule not yet advanced and post its transaction a
second time. Android happened to be in order: Core.kt starts each request on the main dispatcher, and
the plugin did its work on the main thread without suspending, which also blocked the UI on every
query. Any change that moved the work off the main thread (to `Dispatchers.IO`, as other plugins do)
would have lost the order there too.

The tutorial (Saldo, ch. 8 §3 and ch. 9 §4) already told readers the plugin "serializes on one
connection". That was not true on either platform.

## 2. Hypothesis

If every shell starts the `sqlite` calls in the order the core issued them, and the plugin runs them
on one serial lane that owns one connection, then:
- a call runs only after every call issued before it has finished, so a `query` after an `exec` sees
  the write;
- an `exec` answers `ok: true` only once its statement has run to the end, so the write is committed
  (in SQLite's autocommit mode) before the next call starts;
- no SQLite work runs on the main thread.

On Android the order comes from the shell as it is: Core.kt launches each request on the main
dispatcher, and the launches run FIFO, so each call enters `SqlitePlugin.handle` in issue order and
queues on the lane (a single-thread executor) before it first suspends. This is the lane the
`securestore` plugin already uses for the same reason (CLI 0.66.1, #291).

On iOS the shell needed a change, since a serial queue in the plugin alone only gives mutual
exclusion: calls would still reach it in the pool's order. Core.swift now names the plugins whose
calls run in order (`Plugins.inOrder`, `["sqlite"]` today). For those, `Core.start` makes each call's
task wait for the previous call's task, answer and all, and builds that chain on the main actor as the
effects arrive, so the chain's order is the issue order. A notify to such a plugin joins the same
chain. Other plugins still get one `Task` per call. The plugin itself runs on one serial
`DispatchQueue` that owns the connection, which keeps the connection safe even in a shell without the
chain.

### 2.1. Refutation Conditions

- **Condition 1: the iOS shell chains in-order calls as they arrive.** In the template and in
  barbershop, `Plugins.inOrder` names `sqlite`; a request and a notify both start through `start`,
  never a bare `Task`; `start` waits for `laneTails[plugin]` before running the call; the two `start`s
  are identical.
  - **Validation Metric:** `adr_0065_sqlite_calls_run_in_issue_order`.
- **Condition 2: the Android shell starts a request straight into the plugin.** In the template and in
  barbershop, the `Effect.Plugin` branch launches on the view-model scope and calls `dispatch` first,
  and a notify is dispatched inline.
  - **Validation Metric:** `adr_0065_sqlite_calls_run_in_issue_order`.
- **Condition 3: the plugin runs on one lane with one connection.** iOS: one serial queue, the work
  queued on it, one `sqlite3_open`, the connection kept, an `exec` stepped to the end. Android: one
  single-thread executor, `handle` is `withContext(lane)`, one `openOrCreateDatabase`, the connection
  kept, no shared dispatcher.
  - **Validation Metric:** `adr_0065_sqlite_calls_run_in_issue_order`.
- **Condition 4: every app copy is the bundled plugin.** Barbershop's copies equal the bundled files
  rendered with its package (this test); the CLI-managed demos' copies are held equal by ADR-0063's.
  - **Validation Metric:** `adr_0065_sqlite_calls_run_in_issue_order`,
    `adr_0063_every_demo_but_barbershop_is_cli_managed`.

## 3. Considered Options & Rationale for Refutation

- **Option A: a serial queue in the iOS plugin, no shell change** `[recorded: the bug report for this
  PR, 2026-10-20: "enqueue order must equal issue order"]`
  Rejected. The plugin is entered from a different task per call, after a hop off the main actor, so
  the order the calls reach its queue is not the order the core issued them. It gives mutual
  exclusion, not order.
- **Option B: rely on main-actor tasks starting in the order they were created** `[reconstructed]`
  Rejected. The language does not promise it, and each call leaves the main actor at its first
  `await`, after which the calls run concurrently anyway.
- **Option C: run every plugin's calls in order** `[reconstructed]`
  Rejected. A slow `http` request or a `dialog` waiting on the user would hold up every later call to
  the same plugin. Order is a property of a plugin that keeps state between calls, so it is opt-in.
- **Option D: let the core wait for each answer before issuing the next call** `[reconstructed]`
  Rejected. Every app would have to chain its own callbacks, and the migration and restore code in
  Saldo would get much longer. The shells are the one place that can promise it for all apps.
- **Option E: a manifest key (`in_order = true`) that `mobiler plugin add` inserts at an anchor**
  `[reconstructed]`
  Deferred. Only `sqlite` needs it today; a third-party plugin that does would justify the anchor
  then, without changing this decision.

## 4. Decision & Rationale for Corroboration

Option: the shells start in-order plugin calls in issue order (Android as it was; iOS through
`Plugins.inOrder` and `Core.start`), and the `sqlite` plugin runs them on one serial lane with one
connection kept for the life of the process. On iOS an `exec` now steps its statement to the end, like
Android's `execSQL`, so `ok: true` follows the commit and a failure on a later row is reported.

**Mutation proof:** the decision was broken five times on purpose. Each time
`adr_0065_sqlite_calls_run_in_issue_order` failed with the message given; the file was restored and
the test passed again.
- The template's `start` lost `await previous?.value` ("`start` doesn't chain an in-order call behind
  the previous one").
- The template's request case went back to a bare `Task {` ("starts a plugin request without
  `start`").
- The template's `Plugins.inOrder` became `[]` ("Plugins.inOrder doesn't name sqlite").
- The iOS plugin's queue became `attributes: .concurrent` ("the iOS sqlite lane must be one serial
  queue").
- The Android plugin's `handle` ran on `Dispatchers.IO` ("an Android sqlite call must run on the
  lane").

## 5. Consequences (Positive and Negative Predictions)

- **Positive:** an app can issue writes and the read that depends on them in one update, as Saldo
  does, on every platform. The tutorial's "serializes on one connection" is now true.
- **Positive:** SQLite work no longer runs on Android's main thread.
- **Positive:** with one kept connection, a `BEGIN` … `COMMIT` sent as separate `exec`s is one
  transaction; before, each call's connection closed, and an open transaction went with it.
- **Negative:** one slow statement holds up every later `sqlite` call. The calls are small local
  statements, so this is the price of the order.
- **Negative:** an `exec` of `BEGIN` without a `COMMIT` leaves the connection in a transaction for
  every later call, until the process ends.
- **Negative:** iOS calls to `sqlite` no longer overlap: each waits for the previous one's answer to
  be handed to the core. That costs one main-actor hop per call.
- **Negative:** an app gets this only with both halves: the new plugin (`mobiler plugin add sqlite`
  again; `mobiler upgrade` reports the plugin as drifted until then) and, on iOS, the new Core.swift
  (`mobiler upgrade --apply`). With the plugin alone, iOS calls are exclusive but not ordered; with the
  shell alone, they are ordered.
- **Negative:** a plugin outside `Plugins.inOrder` keeps no order on iOS; `securestore` is one (its
  Android lane orders it there only). Adding it is a one-word change under this decision.
