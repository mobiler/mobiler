# Render First (Release 1) — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** An update's screen change shows immediately, not one network round trip later: core emits `Render` first, the Android shell stops awaiting requests inline, and Android text fields can't be rewound by a render.

**Architecture:** One-line reorder in `MobilerShell::update` (mobiler-core) fixes every shell at the root. The Android `Core.kt` `process` loop launches each `Effect.Plugin` in its own coroutine (as iOS/web already do). Android `MainActivity.kt` text fields keep a local `TextFieldValue` via a small `FieldSync` holder that ignores echoes of the field's own edits. No API or wire-ABI change.

**Tech Stack:** Rust (crux_core 0.18), Kotlin / Jetpack Compose Material 3.

**Spec:** `docs/superpowers/specs/2026-09-21-render-first-and-shell-labels-design.md` (Release 1 section). Request: `docs/render-before-requests.md`.

## Global Constraints

- **No API/ABI change.** No `mobiler-ui` change, no new public items in `mobiler-core`. Versions: `mobiler-core` 0.35.0 → **0.35.1**, CLI 0.52.0 → **0.52.1**. `mobiler-ui` (0.24.0) and `mobiler-web` (0.35.0) are **not** bumped or published.
- **Effect order:** `Render` first; then notifications, requests, streams in their **current** relative order.
- **Android:** requests still *start* in batch order. `PluginNotify` and `PluginStream` handling stays as it is.
- **Release shape = two PRs.** The CI lane `scaffold + build (template, Android)` scaffolds against **crates.io** `mobiler-core`, so the template's `mobiler-core = "0.35.1"` pin can only land after 0.35.1 is published.
  - **PR-A** (`fix/render-first`): core fix + test + core version 0.35.1 + Android `Core.kt`/`MainActivity.kt` changes in the **template and all 5 demos** + NOTES.md. Template `Cargo.toml.tmpl` pin untouched.
  - Publish `mobiler-core` 0.35.1 (release-libs skill, core only).
  - **PR-C** (`fix/render-first-cli`): template pin `mobiler-core = "0.35.1"` + CLI 0.52.1. Then release-cli, tag `v0.52.1`, then post-release.
- The 5 demo Android shells: `demos/{coffee,todo,barbershop,saldo}/Android` and `demos/fullstack-todo/mobile/Android`. They drift from the template (package names + customizations). Port **by anchor** (the exact code blocks quoted below appear in all of them), never by copying whole files.
- Generated `*/generated/` and `SharedTypes` are gitignored. Commit source only.
- Cargo's global target dir is `/media/zmilan/data2/cargo-target` (the local CLI binary is `/media/zmilan/data2/cargo-target/debug/mobiler`).
- Local Android builds: `JAVA_HOME=~/jdk21 ANDROID_HOME=/home/zmilan/Android/Sdk`. adb is `/home/zmilan/Android/Sdk/platform-tools/adb`. AVDs: `mobiler_pixel7`, `Pixel_Fold_API_36`.
- Commits end with `Co-Authored-By: Claude Opus 5 (1M context) <noreply@anthropic.com>`.

---

### Task 1: Core emits `Render` first

**Files:**
- Modify: `mobiler-core/src/lib.rs:402-419` (`MobilerShell::update`, the effect assembly)
- Test: `mobiler-core/src/lib.rs` (tests module, next to `shell_ignores_a_malformed_fired_token`, ~line 1940)

**Interfaces:**
- Consumes: existing `Cx::notify`, `Cx::plugin`, `Cx::subscribe`, `MobilerShell`, `Effect`.
- Produces: nothing new. Behaviour: the `Command` returned by `MobilerShell::update` yields `Effect::Render` first.

- [ ] **Step 1: Create the branch**

```bash
cd /home/zmilan/working_docker/rust/mobiler && git switch -c fix/render-first
```

- [ ] **Step 2: Write the failing test**

Add after `shell_ignores_a_malformed_fired_token` in the `tests` module:

```rust
    // An app whose one event makes a notify, a request and a subscription, in that
    // call order — used to pin the effect order `MobilerShell::update` emits.
    #[derive(Default)]
    struct EffectsApp;

    impl MobilerApp for EffectsApp {
        type Event = CounterEv;
        type Model = CounterModel;
        fn update(&self, _ev: CounterEv, model: &mut CounterModel, cx: &mut Cx<CounterEv>) {
            model.count += 1;
            cx.plugin("http", "get", "{}", |_r| CounterEv::Inc);
            cx.notify("toast", "show", "hi");
            cx.subscribe("tick", "ticker", "start", "", |_r| CounterEv::Inc);
            cx.plugin("device", "model", "", |_r| CounterEv::Inc);
        }
        fn view(&self, model: &CounterModel) -> Widget {
            text(format!("{}", model.count))
        }
    }

    #[test]
    fn shell_renders_before_requests_notifications_and_streams() {
        use crux_core::App as _;
        let shell = MobilerShell::<EffectsApp>::default();
        let mut m = CounterModel::default();
        let mut cmd = shell.update(Action::Fired { token: serde_json::to_string(&CounterEv::Inc).unwrap() }, &mut m);
        let kinds: Vec<String> = cmd
            .effects()
            .map(|e| match e {
                Effect::Render(_) => "render".to_string(),
                Effect::PluginNotify(r) => format!("notify:{}", r.operation.plugin),
                Effect::Plugin(r) => format!("plugin:{}", r.operation.plugin),
                Effect::PluginStream(r) => format!("stream:{}", r.operation.plugin),
            })
            .collect();
        // Render first (the model is final when update returns, so the first frame shows it);
        // then notifications, requests (in call order), streams — the order shells already rely on.
        assert_eq!(kinds, ["render", "notify:toast", "plugin:http", "plugin:device", "stream:ticker"]);
    }
```

`input`/`restore`/`init` have default bodies in the `MobilerApp` trait, so `EffectsApp` only needs `update` and `view`.

- [ ] **Step 3: Run it and confirm it fails because Render is last**

Run: `cd /home/zmilan/working_docker/rust/mobiler && cargo test -p mobiler-core shell_renders_before -- --nocapture`
Expected: FAIL, with `left` = `["notify:toast", "plugin:http", "plugin:device", "stream:ticker", "render"]`. If the observed order differs in any other way (e.g. `Command::all` interleaves), STOP and report it; the test's expectation of relative order would need rethinking.

- [ ] **Step 4: Move `render()` first**

In `MobilerShell::update`, replace

```rust
        let mut commands: Vec<Command<Effect, Action>> = Vec::new();
        for op in cx.notifications {
```

with

```rust
        // Render first: the model is final once the app's handler returns, so the first frame
        // must show it. A shell that awaits a request before looking at later effects (the
        // Android shell once did) would otherwise show the change a full round trip late —
        // a controlled text field then reverts keystrokes. Replies re-render via their own
        // `Fired` update.
        let mut commands: Vec<Command<Effect, Action>> = vec![render()];
        for op in cx.notifications {
```

and delete the later `commands.push(render());` line (just before `Command::all(commands)`).

- [ ] **Step 5: Run the test, the core suite and clippy**

Run: `cargo test -p mobiler-core && cargo clippy -p mobiler-core --all-targets -- -D warnings`
Expected: all pass, no warnings.

- [ ] **Step 6: Run the demo cores that inspect effects**

Run: `cd demos/fullstack-sqlx && cargo test` then `cd ../barbershop && cargo test` (and `demos/{coffee,todo,saldo,fullstack-todo}` with `cargo test`).
Expected: all pass (`fullstack-sqlx`'s `plugin_request_count` counts, it doesn't depend on order).

- [ ] **Step 7: Commit**

```bash
git add mobiler-core/src/lib.rs
git commit -m "fix(core): emit Render before an update's requests

A shell that awaits a request before applying later effects showed an
update's screen change one network round trip late; a controlled text
field then reverted keystrokes (appointments app search-as-you-type).

Co-Authored-By: Claude Opus 5 (1M context) <noreply@anthropic.com>"
```

---

### Task 2: Android shell launches each request in its own coroutine

**Files (same edit in each):**
- Modify: `mobiler/templates/Android/app/src/main/java/__PACKAGE_PATH__/Core.kt` (~line 446-451)
- Modify: `demos/barbershop/Android/app/src/main/java/dev/mobiler/barbershop/Core.kt` (~462)
- Modify: `demos/coffee/Android/app/src/main/java/dev/mobiler/coffee/Core.kt` (~447)
- Modify: `demos/todo/Android/app/src/main/java/dev/mobiler/todo/Core.kt` (~183)
- Modify: `demos/saldo/Android/app/src/main/java/rs/mobiler/saldo/Core.kt` (~487)
- Modify: `demos/fullstack-todo/mobile/Android/app/src/main/java/dev/mobiler/mobile/Core.kt` (~256)

**Interfaces:**
- Consumes: Task 1 not required (independent), but ships in the same PR.
- Produces: nothing new.

- [ ] **Step 1: Replace the anchor block in every file**

Anchor (identical in all six; the comment lines above it may differ; keep or replace them):

```kotlin
                is Effect.Plugin -> {
                    val resp = dispatch(effect.value.plugin, effect.value.op, effect.value.input)
                    process(core.resolve(request.id, resp.bincodeSerialize()))
                }
```

Replace with:

```kotlin
                // Request/response: launch it, so later effects in this batch (a Render, other
                // requests) apply at once instead of waiting a network round trip. It resolves
                // the core with the response and processes the effects that produces. On
                // Main.immediate the launch runs up to dispatch's first suspension, so requests
                // still start in batch order; core calls stay on the main thread.
                is Effect.Plugin -> {
                    val call = effect.value
                    val id = request.id
                    viewModelScope.launch {
                        val resp = dispatch(call.plugin, call.op, call.input)
                        process(core.resolve(id, resp.bincodeSerialize()))
                    }
                }
```

In the template, the comment directly above the anchor is
`// Request/response: dispatch (awaiting any async work), resolve the` / `// core with the response, then process the effects that produces.`; delete those two lines (the new comment replaces them). Do the same wherever a demo has them.

- [ ] **Step 2: Confirm `viewModelScope` and `launch` are in scope in every file**

Run: `grep -L "import kotlinx.coroutines.launch" $(grep -rl "is Effect.Plugin ->" mobiler/templates/Android demos --include=Core.kt | grep -v /build/)`
Expected: no output (all files already import `launch`; `process` is a member of the ViewModel that already uses `viewModelScope.launch` in `update` and for streams). If a file is listed, add the import.

- [ ] **Step 3: Confirm no inline-await remains**

Run: `grep -rn -A1 "is Effect.Plugin ->" mobiler/templates/Android demos --include=Core.kt | grep -v /build/ | grep "val resp = dispatch"`
Expected: no output.

- [ ] **Step 4: Compile barbershop's Android shell**

Run (from `demos/barbershop`): `JAVA_HOME=~/jdk21 ANDROID_HOME=/home/zmilan/Android/Sdk /media/zmilan/data2/cargo-target/debug/mobiler dev --no-install` (build the CLI first with `cargo build -p mobiler` at the repo root if the binary is stale).
Expected: `BUILD SUCCESSFUL`. The other 4 demos and the template are compiled by CI in PR-A.

- [ ] **Step 5: Commit**

```bash
git add mobiler/templates/Android demos/*/Android demos/fullstack-todo/mobile/Android
git commit -m "fix(android): don't await a request before applying the batch's other effects

Co-Authored-By: Claude Opus 5 (1M context) <noreply@anthropic.com>"
```

(Check `git status` first: only the six `Core.kt` files should be staged.)

---

### Task 3: Android text fields keep their own editing state

**Files (same edit in each `MainActivity.kt`):**
- Modify: `mobiler/templates/Android/app/src/main/java/__PACKAGE_PATH__/MainActivity.kt` (`is Widget.TextField` ~1030, `is Widget.SearchField` ~1055, imports, a new private class near other file-level helpers)
- Modify: the 5 demo `MainActivity.kt` files (paths as in Task 2, file `MainActivity.kt`)

**Interfaces:**
- Produces: `private class FieldSync(initial: String)` with `var field: TextFieldValue`, `fun onEdit(next: TextFieldValue, send: (String) -> Unit)`, `fun onAppValue(v: String)` — file-private, used only by the two field branches.

Behaviour to get right (this is the reason for the design; check each case against the code):
- The field owns text, cursor and selection. Each user edit that changes the text is sent to the core and remembered as *pending* (sent, not yet echoed).
- A rendered `value` equal to a pending text is an **echo**: drop that entry and every older one; keep the local state. This covers a late render of an older keystroke (pending `["N","Ni"]`, render `"N"` → keep `"Ni"`).
- A rendered `value` that is not pending is an **app-side change** (clear, formatting, programmatic set): adopt it, cursor at the end, clear pending.
- The same `value` rendered again (every recomposition) is ignored — only a *changed* `value` is considered.

- [ ] **Step 1: Add imports** (skip any the file already has)

```kotlin
import androidx.compose.runtime.SideEffect
import androidx.compose.ui.text.TextRange
import androidx.compose.ui.text.input.TextFieldValue
```

- [ ] **Step 2: Add `FieldSync` as a file-level private class** (e.g. just above the composable that contains the big `when (widget)` render switch)

```kotlin
/** Local editing state for a controlled text field. The field owns its text, cursor and
 *  selection; the app's rendered `value` is adopted only when it isn't an echo of an edit the
 *  field itself sent (a current or a late one), so a delayed render can never rewind the text
 *  or move the cursor. An app-side change (clear, formatting) is adopted, cursor at the end. */
private class FieldSync(initial: String) {
    var field by mutableStateOf(TextFieldValue(initial, TextRange(initial.length)))
    private val pending = ArrayDeque<String>()
    private var lastApp = initial

    fun onEdit(next: TextFieldValue, send: (String) -> Unit) {
        val changed = next.text != field.text
        field = next
        if (changed) {
            pending.addLast(next.text)
            send(next.text)
        }
    }

    fun onAppValue(v: String) {
        if (v == lastApp) return
        lastApp = v
        val echo = pending.lastIndexOf(v)
        if (echo >= 0) {
            repeat(echo + 1) { pending.removeFirst() }
            return
        }
        pending.clear()
        if (v != field.text) field = TextFieldValue(v, TextRange(v.length))
    }
}
```

- [ ] **Step 3: Use it in `Widget.TextField`**

Inside the `is Widget.TextField -> { ... }` block, before `OutlinedTextField(`, add:

```kotlin
            val sync = remember(widget.id) { FieldSync(widget.value) }
            SideEffect { sync.onAppValue(widget.value) }
```

and change the two lines

```kotlin
                value = widget.value,
                onValueChange = { send(Action.Input(widget.id, InputValue.Text(it))) },
```

to

```kotlin
                value = sync.field,
                onValueChange = { sync.onEdit(it) { t -> send(Action.Input(widget.id, InputValue.Text(t))) } },
```

- [ ] **Step 4: Use it in `Widget.SearchField`**

Replace

```kotlin
        is Widget.SearchField -> OutlinedTextField(
            value = widget.value,
            onValueChange = { send(Action.Input(widget.id, InputValue.Text(it))) },
```

with

```kotlin
        is Widget.SearchField -> {
            val sync = remember(widget.id) { FieldSync(widget.value) }
            SideEffect { sync.onAppValue(widget.value) }
            OutlinedTextField(
            value = sync.field,
            onValueChange = { sync.onEdit(it) { t -> send(Action.Input(widget.id, InputValue.Text(t))) } },
```

and close the new block: after the `OutlinedTextField(...)` call's closing `)`, add `}`. Re-indent the call's arguments by 4 spaces so the block reads cleanly.

- [ ] **Step 5: Port to the 5 demos**

In each demo `MainActivity.kt`, find the same two anchors (`is Widget.TextField ->` … `value = widget.value,` / `onValueChange = { send(Action.Input(widget.id, InputValue.Text(it))) },`, and `is Widget.SearchField -> OutlinedTextField(`) and apply Steps 1-4 identically.

Check: `grep -rn "value = widget.value," mobiler/templates/Android demos --include=MainActivity.kt | grep -v /build/`
Expected: no remaining hits in `TextField`/`SearchField` branches. (If a hit remains in some other widget, e.g. a slider, read it; only text fields are in scope.)

- [ ] **Step 6: Compile barbershop**

Run (from `demos/barbershop`): `JAVA_HOME=~/jdk21 ANDROID_HOME=/home/zmilan/Android/Sdk /media/zmilan/data2/cargo-target/debug/mobiler dev --no-install`
Expected: `BUILD SUCCESSFUL`.

- [ ] **Step 7: Commit**

```bash
git add mobiler/templates/Android demos/*/Android demos/fullstack-todo/mobile/Android
git commit -m "fix(android): text fields keep their own editing state

A render arriving after further keystrokes no longer rewinds the field or
moves the cursor; app-side changes (clear, formatting) are still adopted.

Co-Authored-By: Claude Opus 5 (1M context) <noreply@anthropic.com>"
```

---

### Task 4: Runtime acceptance on the emulator (throwaway app, no commit)

Everything in this task lives in the scratchpad and is thrown away.
`SCRATCH=/tmp/claude-1000/-home-zmilan-working-docker-rust-mobiler/0aff67bb-5734-4226-ad08-fa524bb65d17/scratchpad`

- [ ] **Step 1: Scaffold with the local CLI and point it at the local core**

```bash
cd /home/zmilan/working_docker/rust/mobiler && cargo build -p mobiler
rm -rf $SCRATCH/r1verify && cd $SCRATCH && /media/zmilan/data2/cargo-target/debug/mobiler new r1verify --package dev.mobiler.r1verify
```

In `$SCRATCH/r1verify/shared/Cargo.toml`, replace `mobiler-core = "0.35"` with
`mobiler-core = { path = "/home/zmilan/working_docker/rust/mobiler/mobiler-core" }`.
In `$SCRATCH/r1verify/Android/app/src/main/AndroidManifest.xml`, add `android:usesCleartextTraffic="true"` to `<application` (the test server is plain http).

- [ ] **Step 2: Replace `shared/src/app.rs` with the probe app**

```rust
use mobiler_core::{ButtonStyle, Cx, InputValue, MobilerApp, MobilerShell, Widget, button, column, search_field, text};
use serde::{Deserialize, Serialize};

const BASE: &str = "http://127.0.0.1:8765";

#[derive(Serialize, Deserialize, Clone, Debug)]
pub enum Msg { Found, Save, Saved }

#[derive(Default)]
pub struct Model { q: String, sent: u32, replies: u32, status: String }

#[derive(Default)]
pub struct {{NAME}}App;

impl MobilerApp for {{NAME}}App {
    type Event = Msg;
    type Model = Model;
    fn update(&self, event: Msg, model: &mut Model, cx: &mut Cx<Msg>) {
        match event {
            Msg::Found => model.replies += 1,
            Msg::Save => {
                model.status = "Saving…".into();
                cx.get(format!("{BASE}/save"), |_| Msg::Saved);
            }
            Msg::Saved => model.status = "Saved".into(),
        }
    }
    fn input(&self, id: &str, value: InputValue, model: &mut Model, cx: &mut Cx<Msg>) {
        if let ("q", InputValue::Text(v)) = (id, value) {
            model.q = v;
            model.sent += 1;
            cx.get(format!("{BASE}/customers?q={}", model.q), |_| Msg::Found);
        }
    }
    fn view(&self, model: &Model) -> Widget {
        column(vec![
            search_field("q", "Search", model.q.clone()),
            text(format!("sent {} replies {}", model.sent, model.replies)),
            button("Save", ButtonStyle::Filled, Msg::Save),
            text(format!("status: {}", model.status)),
        ])
    }
}

pub type App = MobilerShell<{{NAME}}App>;
```

Replace `{{NAME}}` with the struct name the scaffold generated (read the original `app.rs` before overwriting; it's in the `pub struct …App;` line and the `pub type App` line).

- [ ] **Step 3: Start a 2 s-latency server that logs each request**

```bash
cat > $SCRATCH/slow.py <<'EOF'
import http.server, time, sys
class H(http.server.BaseHTTPRequestHandler):
    def do_GET(self):
        print(self.path, flush=True); time.sleep(2)
        self.send_response(200); self.send_header("Content-Type","application/json"); self.end_headers(); self.wfile.write(b"[]")
http.server.ThreadingHTTPServer(("127.0.0.1", 8765), H).serve_forever()
EOF
python3 $SCRATCH/slow.py > $SCRATCH/slow.log 2>&1 &
```

- [ ] **Step 4: Boot the AVD, reverse the port, install**

```bash
ADB=/home/zmilan/Android/Sdk/platform-tools/adb
/home/zmilan/Android/Sdk/emulator/emulator -avd mobiler_pixel7 -no-window -no-audio -no-snapshot &
$ADB wait-for-device && until [ "$($ADB shell getprop sys.boot_completed | tr -d '\r')" = 1 ]; do sleep 2; done
$ADB reverse tcp:8765 tcp:8765
cd $SCRATCH/r1verify && JAVA_HOME=~/jdk21 ANDROID_HOME=/home/zmilan/Android/Sdk /media/zmilan/data2/cargo-target/debug/mobiler dev
```

(Waiting on boot uses a loop, not a bare `sleep`; if the harness blocks foreground sleeps, use the Monitor tool with an until-loop.)

- [ ] **Step 5: Acceptance 1 — fast typing keeps every keystroke**

Tap the search field (find its bounds with `$ADB shell uiautomator dump /sdcard/u.xml && $ADB shell cat /sdcard/u.xml`), then `$ADB shell input text Nikola`. Wait ~3 s for replies, dump the UI again.
Expected: the field shows `Nikola`; the text reads `sent 6 replies 6`; the last line of `$SCRATCH/slow.log` is `/customers?q=Nikola`; the log has exactly 6 `/customers` lines.
Then `$ADB shell input keyevent KEYCODE_DEL` once: field shows `Nikol`, no snap-back after replies arrive.

- [ ] **Step 6: Acceptance 2 — the tap shows its state before the reply**

Tap Save, and within 1 s dump the UI.
Expected: `status: Saving…` visible (the server holds the reply 2 s). After ~3 s: `status: Saved`.

- [ ] **Step 7: Baseline check (proves the test catches the bug)**

Build the unfixed CLI and core from `main` in a separate worktree (the branch is untouched):

```bash
cd /home/zmilan/working_docker/rust/mobiler && git worktree add $SCRATCH/base main
(cd $SCRATCH/base && cargo build -p mobiler --target-dir $SCRATCH/base-target)
cd $SCRATCH && $SCRATCH/base-target/debug/mobiler new r1base --package dev.mobiler.r1base
```

Repeat Steps 1-2 for `$SCRATCH/r1base`, pointing its `mobiler-core` path at `$SCRATCH/base/mobiler-core`. (It installs alongside r1verify; the package ids differ.) Truncate `$SCRATCH/slow.log`, install r1base with `mobiler dev`, repeat Step 5.
Expected: the field loses keystrokes (e.g. shows `iN`-like text) and more than 6 `/customers` requests are logged. If the baseline does **not** reproduce, STOP and report: the probe isn't exercising the bug, so Step 5's pass proves nothing.
Clean up: `git worktree remove $SCRATCH/base && rm -rf $SCRATCH/base-target`.

- [ ] **Step 8: Barbershop regression spot check**

Install barbershop (`cd demos/barbershop && mobiler dev`), open each tab, use one text field (type, backspace, clear), trigger one request-backed action if it has one. Expected: behaves as before.

- [ ] **Step 9: Tear down**

```bash
$ADB emu kill; kill %1 2>/dev/null; pkill -f slow.py
```

Record pass/fail + the observed values (field text, sent/replies counts, log lines) for the PR body.

---

### Task 5: NOTES, versions, ship PR-A

**Files:**
- Modify: `mobiler-core/Cargo.toml` (`version = "0.35.1"`)
- Modify: `NOTES.md` (gitignored — edit, don't `git add`)
- Lockfiles: root `Cargo.lock`, `mobiler-web/Cargo.lock`, each demo's `Cargo.lock`

- [ ] **Step 1: Bump core and refresh locks**

Set `version = "0.35.1"` in `mobiler-core/Cargo.toml`. `mobiler-web/Cargo.toml` keeps `mobiler-core = { path = "../mobiler-core", version = "0.35" }` (0.35.1 satisfies it). Then:

```bash
cargo update -p mobiler-core
(cd mobiler-web && cargo update -p mobiler-core)
for d in demos/coffee demos/todo demos/barbershop demos/saldo demos/fullstack-todo demos/fullstack-sqlx; do (cd $d && cargo update -p mobiler-core); done
grep -rn 'name = "mobiler-core"' -A1 --include=Cargo.lock . | grep 'version = "0.35.0"'
```

Expected: the final grep prints nothing.

- [ ] **Step 2: Full local verification**

```bash
cargo test -p mobiler-core && cargo clippy -p mobiler-core --all-targets -- -D warnings
cargo test -p mobiler
(cd mobiler-web && cargo check --target wasm32-unknown-unknown)
```

Expected: all green.

- [ ] **Step 3: NOTES.md**

Add a subsection "Render first (core 0.35.1 / CLI 0.52.1)" with the *why*: the appointments app's search field lost keystrokes because Android awaited each request inline and `Render` came last; both fixes (root in core, shell parity with iOS/web) plus `FieldSync`'s echo rule; the acceptance numbers from Task 4. Bump the `*Last updated:*` footer.

- [ ] **Step 4: Commit the bump**

```bash
git add mobiler-core/Cargo.toml Cargo.lock mobiler-web/Cargo.lock demos/*/Cargo.lock demos/*/*/Cargo.lock \
  docs/superpowers/specs/2026-09-21-render-first-and-shell-labels-design.md docs/superpowers/plans/2026-09-21-render-first-release-1.md
git commit -m "chore(release): mobiler-core 0.35.1

Co-Authored-By: Claude Opus 5 (1M context) <noreply@anthropic.com>"
```

(`git status` first; stage only lockfiles that changed.)

- [ ] **Step 5: Ship PR-A**

Use the **ship-pr** skill on `fix/render-first`. PR body: the problem (link `docs/render-before-requests.md` findings in prose; the doc itself is untracked), the three changes, Task 4's acceptance results, "no API/ABI change". Merge with **rebase-merge** so the fix commits stay separate. All CI lanes must be green, including every `Android build (…)` lane and `scaffold + build (template, Android)`.

---

### Task 6: Publish core 0.35.1

- [ ] **Step 1:** Use the **release-libs** skill for `mobiler-core` **only** (ui and web unchanged). Respect its irreversible-publish gate: confirm with the user before `cargo publish`.
- [ ] **Step 2:** Verify: `cargo search mobiler-core --limit 1` shows `0.35.1`.

---

### Task 7: Pin the template, bump the CLI, ship PR-C, release

**Files:**
- Modify: `mobiler/templates/shared/Cargo.toml.tmpl:36` → `mobiler-core = "0.35.1"`
- Modify: `mobiler/Cargo.toml` → `version = "0.52.1"`; root `Cargo.lock` (`cargo update -p mobiler`)

- [ ] **Step 1: Branch from updated main**

```bash
git switch main && git pull --ff-only && git switch -c fix/render-first-cli
```

- [ ] **Step 2: Pin and bump**

Edit the two files above, then `cargo update -p mobiler`. Check the CLI's tests that pin the template version (`grep -rn '"0.35"' mobiler/src`) and update any assertion that expects the old pin.

- [ ] **Step 3: Verify a fresh scaffold resolves the published core**

```bash
cargo build -p mobiler && cargo test -p mobiler
rm -rf $SCRATCH/r1scaffold && cd $SCRATCH && /media/zmilan/data2/cargo-target/debug/mobiler new r1scaffold --package dev.mobiler.r1scaffold
cd r1scaffold && cargo build -p shared && grep -A1 'name = "mobiler-core"' Cargo.lock
```

Expected: `version = "0.35.1"`.

- [ ] **Step 4: Commit and ship**

```bash
git add mobiler/templates/shared/Cargo.toml.tmpl mobiler/Cargo.toml Cargo.lock
git commit -m "chore(cli): pin mobiler-core 0.35.1 in the template + mobiler 0.52.1

Co-Authored-By: Claude Opus 5 (1M context) <noreply@anthropic.com>"
```

Use the **ship-pr** skill (squash-merge is fine for this single commit).

- [ ] **Step 5: Release** — use the **release-cli** skill (tag `v0.52.1`), then the **post-release** skill (fresh-install check, start.md + auto-memory, cache cleanup; no screenshot refresh — nothing visual changed).

- [ ] **Step 6: Tell the app team what to run**: `mobiler upgrade` (updates `Core.kt`, `MainActivity.kt` and the `mobiler-core` pin to 0.35.1), then rebuild.
