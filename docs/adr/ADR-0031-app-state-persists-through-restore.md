# ADR-0031: An app persists its state as one opaque string it serializes itself and hands to `cx.save`; each shell stores that string in a built-in `storage` capability and sends it back as `Action::Restore { data }` at startup, before `Start`

Status:        Accepted
Date decided:  2026-05-25
Deciding PRs:  none (pre-PR history — commit b88db7a "ABI: add ColorDot + a persistence capability (for the todo port)"; 117b3a8 ported todo onto it; 11eab25 (2026-05-26) put `Start` after `Restore`)
Supersedes:    none
Code anchor:   mobiler-ui/src/lib.rs (Action::Restore, Action::Start), mobiler-core/src/lib.rs (Cx::save, MobilerApp::restore, MobilerApp::init, MobilerShell::update), template iOS Core.swift (StoragePlugin, startup), template Android Core.kt (StoragePlugin, Core init), mobiler-web/src/lib.rs (fn shell startup, perform_notify "storage"/"save", STORAGE_KEY)
Conformance:   mobiler-core/src/lib.rs::shell_dispatches_fired_input_restore_and_start, mobiler-core/src/lib.rs::cx_notify_and_save_enqueue_notifications

## 1. Context (The Problem)

The todo demo was the first app to keep data on the device. Its tasks and projects had to survive
a cold restart. The app is a Rust core behind a fixed ABI (ADR-0001), and platform features are
opaque plugin effects (ADR-0002). The core has no platform app-data location of its own (only the shell knows where an app may
store data), and
`MobilerShell` holds no state across processes. Something had to carry the app's data out to the
platform and back in on the next launch.

## 2. Hypothesis

If the app serializes whatever it wants to keep into one `String` and calls `cx.save(data)`, the
shell stores that string under one fixed key, and at the next startup the shell sends it back as
`Action::Restore { data }` before `Action::Start`, then:

- `MobilerApp::restore(data, model)` rebuilds the model before `init` runs, so `init` and the first
  frames see the saved state;
- the framework never needs to know the model's shape, and the ABI carries only a string;
- the same app code persists on iOS, Android and web. (Decided on the Android shell in b88db7a;
  the iOS template (ab881a2, 2026-05-26) and the web shell followed the same shape.)

### 2.1. Refutation Conditions

- **Condition 1 — `Restore` reaches the app's `restore`, `Start` reaches `init`.**
  - **Validation Metric:** `shell_dispatches_fired_input_restore_and_start` in
    `mobiler-core/src/lib.rs` (sends `Action::Restore { data: "saved" }`, asserts the model saw it,
    then `Action::Start`).
- **Condition 2 — `cx.save` is a `storage`/`save` notification carrying the string unchanged.**
  - **Validation Metric:** `cx_notify_and_save_enqueue_notifications` in `mobiler-core/src/lib.rs`.
- **Condition 3 — every shell sends `Restore` before `Start`.** Not tested: the order lives in each
  shell's startup code. Review. The order today is `AppInfo`, then `Restore`, then `Start`
  (ADR-0018).

## 3. Considered Options & Rationale for Refutation

- **Option A — the framework serializes the whole `Model`** `[reconstructed]`
  It would force `Serialize` on every model and persist fields the app wants fresh (navigation,
  loading flags, form drafts). `MobilerApp::Model` is bound only by `Default`. The todo demo saves
  a `Persisted` subset, not its `Model`.
- **Option B — the app loads its state with a request in `init`** `[reconstructed]`
  The iOS and Android shells do answer a `load` op on the same `storage` capability (web does not).
  But a request resolves
  after the first frame, so the app would first render empty. The commit that chose startup
  delivery sends `Action::Restore before the first frame` (commit b88db7a).
- **Option C — one app string, stored by a built-in shell capability, handed back as an `Action` at startup** `[recorded: commit b88db7a ("Action::Restore before the first frame"); commit 11eab25 ("Template shell fires Action.Start on startup (after Restore).")]`
  Chosen.

## 4. Decision & Rationale for Corroboration

Option C. `Cx::save(data)` is `self.notify("storage", "save", data)`: fire-and-forget, no reply.
`storage` is a built-in capability, present in every shell without `mobiler plugin add`. Each shell
keeps one string: `UserDefaults` key `mobiler.state` on iOS, `SharedPreferences` file `mobiler`
key `state` on Android, `localStorage` key `mobiler.state` on web. At startup each shell reads the
string and, **only if it is non-empty**, sends `Action::Restore { data }`; it then always sends
`Action::Start`. `MobilerShell::update` routes `Restore` to `MobilerApp::restore(&data, model)`,
which has no `Cx` (it can't issue effects) and defaults to doing nothing.

The app owns the format. The todo demo writes `serde_json` of a `Persisted` struct after every
event (`update` and `input`) and reads it back in `restore` (`demos/todo/shared/src/app.rs`). Commit 117b3a8
records the runtime check: "including persistence after a force-stop". Apps with more data than a
blob use the `sqlite` plugin instead (the saldo demo), and secrets belong in `securestore`.

## 5. Consequences (Positive and Negative Predictions)

- **Positive:** persistence is one call and one hook, identical on three platforms, with no ABI
  type beyond a string.
- **Positive:** the app decides what survives a restart. The todo demo does not persist its `Nav`
  stack, so a relaunch opens at the root screen.
- **Negative:** one blob per app. Every save rewrites the whole string, and there is no partial
  update, no versioning and no migration help. An app that changes its format must handle old
  blobs in `restore` itself.
- **Negative:** `cx.save` has no reply. The app can't know the write succeeded, and anything not
  yet saved is lost when the OS kills the process. There is no separate "save on background" hook;
  an app that wants one subscribes to lifecycle events on the `system` stream.
- **Negative:** the stores are plain key/value preferences, not encrypted and not meant for large
  data. On web the key is fixed per origin, so two mobiler apps served from one origin share it.
- **Negative:** `Action::Restore`'s doc says "empty string if none" and `MobilerApp::restore`'s says
  "or empty if nothing was saved", but no shell sends an empty one. On first launch `restore` is not called at all; an app must not rely on it running.
- **Negative:** `restore` gets no `Cx`, so work that needs effects after restoring (a refetch)
  goes in `init`, which runs next.
