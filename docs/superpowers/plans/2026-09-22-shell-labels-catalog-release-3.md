# App-wide Shell Labels (Release 3) — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Apps set the text the shells draw themselves once, on the scaffold (`with_labels(ShellLabels…)`). It covers back buttons, web's "Load more" / "Refresh", and the dialog/picker defaults. In the same release, confirms become one-at-a-time on web and iOS, the web confirm modal traps focus, and the demos' old clippy lints are fixed.

**Architecture:**
- **ABI (mobiler-ui):** a new wire type, `ShellLabels`, with public `Option<String>` fields and a builder. It lives in mobiler-ui next to `Theme`. `Widget::Scaffold` gains `labels: Option<ShellLabels>` as its **last** field.
- **Core (mobiler-core):** re-exports it and adds `with_labels`. Every scaffold builder and combinator carries the field through.
- **Shells:** each one stores the current scaffold's labels next to where it stores the theme, so dialog code, which never sees the view, can read them. Precedence: per-call label > scaffold label > English default.

**Tech Stack:** Rust (facet typegen, crux_core 0.18), Leptos/WASM (web), Jetpack Compose M3 (Android), SwiftUI/UIKit (iOS).

**Spec:** `docs/superpowers/specs/2026-09-22-shell-labels-catalog-design.md`.

## Global Constraints

- **Versions:**
  - `mobiler-ui` 0.25.0 → **0.26.0**.
  - `mobiler-core` 0.36.0 → **0.37.0**, with `mobiler-ui = { path = "../mobiler-ui", version = "0.26.0" }`.
  - `mobiler-web` 0.36.0 → **0.37.0**, with `mobiler-core = { …, version = "0.37" }`.
  - CLI 0.53.0 → **0.54.0**, and the template pin becomes `mobiler-core = "0.37"`.
- **ABI:**
  - `ShellLabels { back, load_more, refresh, ok, cancel, done }`, all `Option<String>`, in exactly this field order. It derives `Facet, Serialize, Deserialize, Clone, Debug, Default, PartialEq, Eq`.
  - `Widget::Scaffold.labels: Option<ShellLabels>` is the **last** field.
- **Precedence** for every string: a label on the specific call (`confirm_with` / `Picker`), then the scaffold's `ShellLabels`, then today's default. Today's defaults are unchanged:
  - Confirm: `OK` / `Cancel`.
  - iOS picker: `Done` / `Cancel`, plus its title default.
  - Android picker: the platform's own buttons.
  - Web: `Load more`, `↻ Refresh`, `‹`, and `‹ Back` on Split.
  - Android: `‹ Back` on Split, and `contentDescription "Back"` on the scaffold back button.
  - iOS: `Back` on Split.
- **Where each label goes:**
  - `back`: the Split back-button text on all shells (Android/web `‹ {back}`, iOS `Label({back}, chevron)`), and the Scaffold back-button accessible name (Android `contentDescription`, iOS `.accessibilityLabel`, web `aria-label`).
  - `load_more`: web LazyList button.
  - `refresh`: web LazyList refresh button, shown as `↻ {refresh}`, plus the `aria-label` of the web scaffold's `↻` button.
  - `ok` / `cancel`: confirm defaults on all shells.
  - `cancel` / `done`: iOS and Android picker defaults.
- **Wire:** plain `cx.confirm` stays byte-identical, because the core never injects labels.
- **One confirm at a time:** a new confirm answers the open one `ok: false` / output `"cancel"` and replaces it. This applies to web and iOS; Android already does it. Pickers are out of scope.
- **Out of scope:**
  - Android "PDF: could not load PDF".
  - Web iframe `title`s.
  - `LazyList { fill }`.
  - Making CI's clippy step gating.
- **Release shape = two PRs:**
  - **PR-A** (`feat/shell-labels-catalog`): libs, the 5 demos' shells, the barbershop showcase, lint fixes, versions and docs. **The template is untouched.** Rebase-merge.
  - Then `release-libs`.
  - **PR-C** (`feat/shell-labels-catalog-cli`): template port, pin, CLI 0.54.0. Then `release-cli` with tag `v0.54.0`.
  - **Ask the user before each merge, publish and tag.**
- **Demo shells:** `demos/{coffee,todo,barbershop,saldo}` and `demos/fullstack-todo/mobile`.
  - Android `<pkg>` paths: `dev/mobiler/{coffee,todo,barbershop}`, `rs/mobiler/saldo`, `dev/mobiler/mobile`.
  - Only barbershop, coffee and saldo have `DialogPlugin` / `DateTimePlugin` in Android `Core.kt`. todo and fullstack-todo don't.
  - iOS `Core.swift` has them in all 5.
  - iOS `Render.swift` is byte-identical in barbershop, coffee, todo and fullstack-todo; saldo diverges.
  - Rule: edit barbershop first, `cp` its `Render.swift` to the 3 identical demos, and port everything else **by anchor**.
- **Local environment** (unchanged from Release 2):
  - Every `mobiler dev` / `mobiler build` needs `CARGO_TARGET_DIR=<app>/target`, `JAVA_HOME=~/jdk21` and `ANDROID_HOME=/home/zmilan/Android/Sdk`. The CLI binary is `/media/zmilan/data2/cargo-target/debug/mobiler`.
  - If the sandbox blocks running inside a demo dir, rsync the demo into `$SCRATCH` and point its path deps at the absolute repo crates.
  - AVD: `mobiler_verify_p7` with `ANDROID_AVD_HOME=/media/zmilan/data2/android-avd`.
  - **Never leave barbershop's Profile tab on Android.** Doing so triggers the pre-existing swiftshader RenderThread SIGSEGV.
  - iOS Swift compiles only on macOS CI.
- Commits: subject, blank line, then the trailer `Co-Authored-By: Claude Opus 5 (1M context) <noreply@anthropic.com>`. Never stage the `docs/*.md` request files or `.superpowers/`.
- `SCRATCH=/tmp/claude-1000/-home-zmilan-working-docker-rust-mobiler/0aff67bb-5734-4226-ad08-fa524bb65d17/scratchpad`

---

### Task 1: `ShellLabels` ABI + core (`with_labels`, combinators carry `labels`)

**Files:**
- `mobiler-ui/src/lib.rs`: a new struct next to `Theme` (~line 335); the `Scaffold` variant (~line 638); round-trip tests (~715-770).
- `mobiler-core/src/lib.rs`: add `ShellLabels` to the `pub use mobiler_ui::{…}` list.
  - Scaffold builders: `scaffold` ~1146, `scaffold_back` ~1155, and the full constructor at ~1175.
  - Combinators that rebuild `Widget::Scaffold { … }`: ~1196 (with_theme), ~1218, ~1240, ~1263 (with_refresh). Check each one's name.
  - A new `with_labels`, plus builder tests.
- `mobiler-web/src/lib.rs`: add `labels` to the `Widget::Scaffold { … }` pattern (~1878). The field is unused until Task 2, so bind it as `labels: _`.

**Interfaces:**
- Produces:
  - `mobiler_ui::ShellLabels` (re-exported as `mobiler_core::ShellLabels`), with `ShellLabels::new()` and the chaining setters `back`, `load_more`, `refresh`, `ok`, `cancel`, `done` (each takes `impl Into<String>`).
  - `mobiler_core::with_labels(widget: Widget, labels: ShellLabels) -> Widget`.
  - `Widget::Scaffold { …, depth, labels }`.

- [ ] **Step 1: Branch.** `git switch main && git pull --ff-only && git switch -c feat/shell-labels-catalog`

- [ ] **Step 2: Failing tests.**

  In `mobiler-ui`'s tests, add a round-trip of a labelled scaffold. Copy the existing un-themed Scaffold round-trip, and add `labels: Some(ShellLabels::new().back("Nazad").load_more("Učitaj još").refresh("Osveži").ok("U redu").cancel("Otkaži").done("Gotovo"))`. Also add `labels: None` to every existing Scaffold literal in the tests.

  In the `mobiler-core` builder tests, add:

```rust
    #[test]
    fn with_labels_sets_scaffold_labels_and_combinators_keep_them() {
        let l = ShellLabels::new().back("Nazad").ok("U redu");
        assert!(matches!(scaffold("T", false, vec![], text("b")), Widget::Scaffold { labels: None, .. }));
        let s = with_labels(scaffold("T", false, vec![], text("b")), l.clone());
        assert!(matches!(&s, Widget::Scaffold { labels: Some(x), .. } if *x == l));
        // Every scaffold combinator must carry labels through, in either order.
        let themed = with_theme(s.clone(), Theme { seed: Rgb::new(1, 2, 3), ..Default::default() });
        assert!(matches!(&themed, Widget::Scaffold { labels: Some(x), .. } if *x == l));
        let refreshed = with_refresh(s.clone(), false, Ev::Tap);
        assert!(matches!(&refreshed, Widget::Scaffold { labels: Some(x), .. } if *x == l));
        let fabbed = with_fab(s.clone(), Icon::Calendar, Ev::Tap);
        assert!(matches!(&fabbed, Widget::Scaffold { labels: Some(x), .. } if *x == l));
        let sheeted = with_sheet(s, "Sheet", text("c"), Ev::Tap);
        assert!(matches!(&sheeted, Widget::Scaffold { labels: Some(x), .. } if *x == l));
        // No-op elsewhere.
        assert!(matches!(with_labels(text("x"), ShellLabels::new()), Widget::Text { .. }));
        // Builder fills only what was set.
        assert_eq!(ShellLabels::new().done("Gotovo"), ShellLabels { done: Some("Gotovo".into()), ..ShellLabels::default() });
    }
```

  The scaffold combinators are `with_theme`, `with_fab`, `with_sheet` and `with_refresh` (checked in planning). The full constructors are `scaffold`, `scaffold_back` and `nav_scaffold`. Also add `assert!(matches!(nav_scaffold(…), Widget::Scaffold { labels: None, .. }))`, using `nav_scaffold`'s real signature.

- [ ] **Step 3:** `cargo test -p mobiler-ui -p mobiler-core 2>&1 | tail -5` → FAIL (`ShellLabels` doesn't exist).

- [ ] **Step 4: mobiler-ui.** Next to `Theme`:

```rust
/// Text the shells draw themselves (back buttons, web list controls, dialog/picker defaults), in the
/// app's language. Set once on the scaffold with `with_labels`; every field is optional and falls
/// back to the shell's built-in English. A label passed to a specific call (`confirm_with`,
/// `Picker`) still wins over these.
#[derive(Facet, Serialize, Deserialize, Clone, Debug, Default, PartialEq, Eq)]
pub struct ShellLabels {
    /// `Split` back-button text on every shell; the Scaffold back button's accessible name.
    pub back: Option<String>,
    /// Web `LazyList` "Load more" button.
    pub load_more: Option<String>,
    /// Web `LazyList` refresh button (shown after a ↻) and the web scaffold refresh button's name.
    pub refresh: Option<String>,
    /// Confirm dialog: the confirming button when the call gives no label.
    pub ok: Option<String>,
    /// Confirm dialog and pickers: the dismissing button when the call gives no label.
    pub cancel: Option<String>,
    /// Pickers: the accepting button when the call gives no label.
    pub done: Option<String>,
}

impl ShellLabels {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }
    #[must_use]
    pub fn back(mut self, l: impl Into<String>) -> Self { self.back = Some(l.into()); self }
    #[must_use]
    pub fn load_more(mut self, l: impl Into<String>) -> Self { self.load_more = Some(l.into()); self }
    #[must_use]
    pub fn refresh(mut self, l: impl Into<String>) -> Self { self.refresh = Some(l.into()); self }
    #[must_use]
    pub fn ok(mut self, l: impl Into<String>) -> Self { self.ok = Some(l.into()); self }
    #[must_use]
    pub fn cancel(mut self, l: impl Into<String>) -> Self { self.cancel = Some(l.into()); self }
    #[must_use]
    pub fn done(mut self, l: impl Into<String>) -> Self { self.done = Some(l.into()); self }
}
```

  Format the setters the way `rustfmt` wants (multi-line); the compact form above is only for the plan. If `#[repr(C)]` or any other attribute is on `Theme` for Facet's sake, check whether a struct with `Option<String>` fields needs it: copy what `Caption` or `MapMarker` (structs with `String` fields) use. Add the field last in `Scaffold`:

```rust
        /// App-wide shell text (see [`ShellLabels`]). `None` ⇒ the shells' English defaults.
        labels: Option<ShellLabels>,
```

- [ ] **Step 5: mobiler-core.**
  - Add `ShellLabels` to the `pub use mobiler_ui::{…}` list.
  - Give the three full constructors `labels: None`.
  - In each combinator: add `labels` to the destructuring pattern (next to `depth`) and pass it into the rebuilt Scaffold.
  - Add:

```rust
/// Set the app-wide shell text (see [`ShellLabels`]) on a scaffold — e.g.
/// `with_labels(root, ShellLabels::new().back("Nazad").ok("U redu").cancel("Otkaži"))` also puts
/// those words on a plain `cx.confirm`. Combines with the other scaffold helpers in any order.
/// No-op on other widgets.
#[must_use]
pub fn with_labels(widget: Widget, labels: ShellLabels) -> Widget {
    match widget {
        Widget::Scaffold { title, body, tabs, back, dark_mode, theme, fab, sheet, on_refresh, refreshing, route, depth, .. } => {
            Widget::Scaffold { title, body, tabs, back, dark_mode, theme, fab, sheet, on_refresh, refreshing, route, depth, labels: Some(labels) }
        }
        other => other,
    }
}
```

- [ ] **Step 6: mobiler-web.** Add `labels: _` to the Scaffold pattern so the crate compiles. Then fix every other Rust compile error across `grep -rn "Widget::Scaffold {" --include=*.rs mobiler-core mobiler-web demos | grep -v target`, adding `labels` or `..` as needed.

- [ ] **Step 7: Verify.**
  - `cargo test -p mobiler-ui -p mobiler-core && cargo clippy -p mobiler-core --all-targets -- -D warnings`
  - `(cd mobiler-web && cargo check --target wasm32-unknown-unknown)`
  - `cargo test` in each demo.
  - Expected: all green.

- [ ] **Step 8: Commit** `feat(labels): ShellLabels on the scaffold — app-wide shell text` + trailer.

---

### Task 2: Web — labels, one confirm at a time, focus trap

**Files:** `mobiler-web/src/lib.rs`.

- [ ] **Step 1: The labels stash.** Near the existing `thread_local!` (~1973), add:

```rust
thread_local! {
    /// The current scaffold's app-wide shell text, set when the Scaffold renders and read by code
    /// that never sees the view (the confirm modal). `None` ⇒ English defaults.
    static ACTIVE_LABELS: std::cell::RefCell<Option<mobiler_core::ShellLabels>> = const { std::cell::RefCell::new(None) };
}

/// The app's label for a piece of shell text, or `default`.
fn shell_label(pick: impl Fn(&mobiler_core::ShellLabels) -> Option<String>, default: &str) -> String {
    ACTIVE_LABELS.with(|l| l.borrow().as_ref().and_then(|l| pick(l)).unwrap_or_else(|| default.to_string()))
}
```

  Use whatever path the file already uses for mobiler-ui/core types (`ShellLabels` may already be in scope through an existing `use`). In the Scaffold arm, bind `labels` and, first thing in the arm, run `ACTIVE_LABELS.with(|l| *l.borrow_mut() = labels.clone());`.

  Also clear the stash when the root isn't a Scaffold. Find the top-level render call that renders `view.get()`, and before it set `ACTIVE_LABELS` to `None` unless the root is a `Widget::Scaffold`. The Scaffold arm then sets it.

- [ ] **Step 2: Use the labels.**
  - Scaffold back button: add `aria-label=shell_label(|l| l.back.clone(), "Back")`.
  - Scaffold `↻` button (~1939): add `aria-label=shell_label(|l| l.refresh.clone(), "Refresh")`.
  - LazyList: the text is `format!("↻ {}", shell_label(|l| l.refresh.clone(), "Refresh"))` and `shell_label(|l| l.load_more.clone(), "Load more")`. The unlabelled output must be exactly `↻ Refresh` / `Load more`, as before.
  - Split: `format!("‹ {}", shell_label(|l| l.back.clone(), "Back"))`.
  - `ConfirmAsk::parse`: the defaults become `shell_label(|l| l.ok.clone(), "OK")` and `shell_label(|l| l.cancel.clone(), "Cancel")`. A per-call non-empty label still wins.

  Values captured in closures inside `view!` need owned `String`s, so compute them before the `view!` block.

- [ ] **Step 3: One confirm at a time.** The goal: a new confirm resolves the old one `false`, the old one's teardown doesn't steal focus, and focus finally returns to where it was before the *first* dialog opened. Add:

```rust
/// The open confirm modal: its answer sender, whether a newer confirm replaced it, and where focus
/// should return when the (last) dialog closes. One confirm at a time — a new one answers the
/// open one `false` and takes over its focus-return target.
struct OpenConfirm {
    tx: std::rc::Rc<std::cell::RefCell<Option<futures_channel::oneshot::Sender<bool>>>>,
    superseded: std::rc::Rc<std::cell::Cell<bool>>,
    restore_to: Option<web_sys::Element>,
}
thread_local! {
    static OPEN_CONFIRM: std::cell::RefCell<Option<OpenConfirm>> = const { std::cell::RefCell::new(None) };
}
```

  In `confirm_modal`:
  - At the start, replace `let previously_focused = doc.active_element();` with: take the old `OpenConfirm` (if any), set its `superseded` to `true`, send `false` on its `tx`, and use its `restore_to` as this dialog's `previously_focused`. Otherwise use `doc.active_element()`.
  - After creating `tx` and a new `superseded` flag, store `OpenConfirm { tx: tx.clone(), superseded: superseded.clone(), restore_to: previously_focused.clone() }` in `OPEN_CONFIRM`.
  - In teardown, after `rx.await`: clear `OPEN_CONFIRM` only if it still holds this dialog's `tx` (`Rc::ptr_eq`). Restore focus only if `!superseded.get()`.

- [ ] **Step 4: Focus trap.** Extend the existing `on_key` closure. It needs clones of `doc`, `card`, `cancel` and `confirm` moved in, so clone them before the closure:

```rust
        if e.key() == "Tab" {
            let first: web_sys::Element = cancel_for_key.clone().into();
            let last: web_sys::Element = confirm_for_key.clone().into();
            let active = doc_for_key.active_element();
            let inside = active.as_ref().is_some_and(|a| card_for_key.contains(Some(a)));
            let (at_first, at_last) = (active.as_ref() == Some(&first), active.as_ref() == Some(&last));
            if e.shift_key() {
                if at_first || !inside { e.prevent_default(); let _ = confirm_for_key.focus(); }
            } else if at_last || !inside {
                e.prevent_default();
                let _ = cancel_for_key.focus();
            }
        }
```

  `Node::contains` takes `Option<&Node>`. Convert with `.as_ref()` / `.dyn_ref::<web_sys::Node>()` as the compiler requires.

- [ ] **Step 5: Verify.** `cd mobiler-web && cargo check --target wasm32-unknown-unknown && cargo clippy --target wasm32-unknown-unknown -- -D warnings`. Then `cd demos/barbershop/web && RUSTUP_TOOLCHAIN=stable trunk build`.

- [ ] **Step 6: Commit** `feat(web): app-wide shell labels; one confirm at a time; modal focus trap` + trailer.

---

### Task 3: Android — labels (5 demos)

**Files:** `MainActivity.kt` in all 5 demos, and `Core.kt` in barbershop, coffee and saldo.

- [ ] **Step 1: `MainActivity.kt`, the labels holder.** Add a file-level public object next to `activeTheme`, and set it in `App()` next to `activeTheme = appTheme`:

```kotlin
/** The current scaffold's app-wide shell text (ShellLabels), set when App() renders. Core.kt's
 *  dialog/picker plugins read it for their defaults; null ⇒ English/platform defaults. */
object ActiveLabels {
    @Volatile var current: ShellLabels? = null
}
```

  In `App()`: `ActiveLabels.current = (view as? Widget.Scaffold)?.labels`.

  The generated Kotlin type name for the struct may be `ShellLabels`. Check the generated `Types.kt` after the Task 1 codegen for the exact name, and whether fields are `loadMore` etc. Follow how `Theme` is referenced in the file (`ModelTheme` alias?).

- [ ] **Step 2: `MainActivity.kt`, the back buttons.**
  - Scaffold `contentDescription = "Back"` → `contentDescription = ActiveLabels.current?.back ?: "Back"`.
  - Split `Text("‹ Back", …)` → `Text("‹ ${ActiveLabels.current?.back ?: "Back"}", …)`.

- [ ] **Step 3: `Core.kt` (barbershop, coffee, saldo only), dialog and picker defaults.**
  - `DialogPlugin`: `.ifEmpty { "OK" }` → `.ifEmpty { ActiveLabels.current?.ok ?: "OK" }`, and likewise `cancel` → `"Cancel"`.
  - `DateTimePlugin`: `val confirmLabel = labels?.optString("confirm_label").orEmpty()` → `…orEmpty().ifEmpty { ActiveLabels.current?.done.orEmpty() }`, and `cancelLabel` the same with `cancel`. An empty result still means "keep the platform button", so the existing `isNotEmpty()` guards stay.

- [ ] **Step 4: Port and check.**
  - Apply the changes to the other demos by anchor.
  - `grep -c "object ActiveLabels"` must be 1 in each `MainActivity.kt`.
  - `grep -c 'contentDescription = "Back"'` must be 0.
  - `grep -c '"‹ Back"'` must be 0.
- [ ] **Step 5: Compile.** Build barbershop and one of todo/fullstack-todo (the ones without dialogs), each with `mobiler dev --no-install` (`CARGO_TARGET_DIR=$PWD/target`, JDK, SDK). Clean up target and build dirs afterwards.
- [ ] **Step 6: Commit** `feat(android): app-wide shell labels for back buttons and dialog/picker defaults` + trailer.

---

### Task 4: iOS — labels + one confirm at a time (5 demos)

**Files:**
- `demos/barbershop/iOS/Sources/Render.swift`, then `cp` it to coffee, todo and fullstack-todo; saldo by anchor.
- `iOS/Sources/Core.swift` in all 5, by anchor.

- [ ] **Step 1: `Render.swift`, the stash.**
  - The `.scaffold(…)` case (~355) gains `, let labels` as its **13th** binding.
  - Set `ActiveLabels.current = labels` next to `ActiveTheme.current = theme`.
  - Add, next to `enum ActiveTheme`:

```swift
/// The current scaffold's app-wide shell text (ShellLabels), set when a Scaffold renders; read by
/// the dialog/picker plugins for their defaults. `nil` ⇒ English defaults.
enum ActiveLabels {
    nonisolated(unsafe) static var current: ShellLabels?
}
```

  Check for any other `.scaffold(` pattern in the file (`grep -n "\.scaffold(" Render.swift`); every one must bind 13 values or use a wildcard.

- [ ] **Step 2: `Render.swift`, the back buttons.**
  - Split `Label("Back", systemImage: "chevron.left")` → `Label(ActiveLabels.current?.back ?? "Back", systemImage: "chevron.left")`.
  - Scaffold back `Image(systemName: "chevron.left")` inside the back `Button` (~1121): add `.accessibilityLabel(ActiveLabels.current?.back ?? "Back")` to the Button.
- [ ] **Step 3: Copy and verify identity.**
  - Before copying, confirm that the 4 files were identical at HEAD: `git show HEAD:demos/<d>/iOS/Sources/Render.swift | md5sum` must be equal for barbershop, coffee, todo and fullstack-todo.
  - Copy, then check the md5s again.
  - Port the 3 edits to saldo by anchor.
- [ ] **Step 4: `Core.swift` `DialogPlugin`, defaults.**
  - `?? "OK"` → `?? (ActiveLabels.current?.ok ?? "OK")`.
  - `?? "Cancel"` → `?? (ActiveLabels.current?.cancel ?? "Cancel")`.
- [ ] **Step 5: `Core.swift` `DialogPlugin`, one at a time.** Add to the enum:

```swift
    /// The confirm alert currently on screen and how to answer it. A new confirm answers it
    /// `ok: false` and dismisses it first — one confirm at a time on every shell.
    private static var open: (alert: UIAlertController, answer: (PluginResponse) -> Void)?
```

  Restructure the presentation so that:
  - Before `topViewController()` is read, if `open` is set: take it, set `open = nil`, call `prev.alert.dismiss(animated: false)`, then `prev.answer(PluginResponse(ok: false, output: "cancel"))`.
  - Inside `withCheckedContinuation`: create the alert, then a resume-once `done(_:)` like `DateTimePlugin`'s. When `done` runs, it clears `open` if `open?.alert === alert`, then resumes. Both actions call `done`. Set `open = (alert, done)` and present.
  - Then read `presenter` (the `guard let presenter = topViewController()` must run **after** the dismissal, so it doesn't return the alert that's being dismissed).
- [ ] **Step 6: `Core.swift` `DateTimePlugin`, defaults.**
  - `label("cancel_label", "Cancel")` → `label("cancel_label", ActiveLabels.current?.cancel ?? "Cancel")`.
  - `label("confirm_label", "Done")` → `label("confirm_label", ActiveLabels.current?.done ?? "Done")`.
- [ ] **Step 7: Port and check.** Apply to all 5 `Core.swift` by anchor.
  - `grep -c "private static var open"` must be 1 in each.
  - md5 the `DialogPlugin` blocks: they must match across all 5.
  - Swift compiles on CI only, so read the Swift twice for: `ShellLabels` optional field names (the generated Swift uses camelCase `loadMore`; check the generated `SharedTypes` naming in how `Theme` fields are used, e.g. `theme.density`), tuple-typed static var syntax, and the main-actor isolation of the static var. The enum is `@MainActor`, so its statics are too.
- [ ] **Step 8: Commit** `feat(ios): app-wide shell labels; one confirm at a time` + trailer.

---

### Task 5: Barbershop showcase + tests

**Files:** `demos/barbershop/app-core/src/lib.rs`.

- [ ] **Step 1: Failing tests.**

```rust
    #[test]
    fn root_scaffold_carries_the_app_labels() {
        let (app, model) = app();
        match app.view(&model) {
            Widget::Scaffold { labels: Some(l), .. } => {
                assert_eq!(l.back.as_deref(), Some("Back to shop"));
                assert_eq!(l.load_more.as_deref(), Some("Show more"));
                assert_eq!(l.refresh.as_deref(), Some("Reload"));
                assert_eq!(l.ok.as_deref(), Some("Sure"));
                assert_eq!(l.cancel.as_deref(), Some("No thanks"));
                assert_eq!(l.done.as_deref(), Some("Pick"));
            }
            other => panic!("expected a labelled scaffold, got {other:?}"),
        }
    }

    #[test]
    fn reschedule_asks_with_a_plain_confirm() {
        let calls = dialog_inputs(Msg::AskReschedule);
        let (plugin, op, v) = &calls[0];
        assert_eq!((plugin.as_str(), op.as_str()), ("dialog", "confirm"));
        // Plain cx.confirm: no per-call labels, so the scaffold's ok/cancel apply in the shell.
        assert!(v.get("confirm_label").is_none() && v.get("cancel_label").is_none());
    }

    #[test]
    fn time_picker_leaves_accept_to_the_scaffold_default() {
        let calls = dialog_inputs(Msg::DatePicked("2026-09-30".into()));
        let (_, op, v) = &calls[0];
        assert_eq!(op, "time");
        assert!(v.get("confirm_label").is_none(), "accept button comes from ShellLabels.done");
        assert_eq!(v["cancel_label"], "Back");
    }
```

  `dialog_inputs` already exists and drives `Msg` through a fresh `MobilerShell`. `DatePicked` with a non-empty date issues the time picker on a fresh model.

- [ ] **Step 2:** `cd demos/barbershop && cargo test` → FAIL.
- [ ] **Step 3: Implement.**
  - `view`: `with_theme(with_labels(root, ShellLabels::new().back("Back to shop").load_more("Show more").refresh("Reload").ok("Sure").cancel("No thanks").done("Pick")), theme)`, with a comment explaining that this sets the app's own wording for shell-drawn text.
  - Add `Msg::AskReschedule` and `Msg::RescheduleAnswered(bool)`.
    - The "Reschedule" button fires `AskReschedule`.
    - `AskReschedule` → `cx.confirm("Reschedule?", "We'll open the booking flow.", |r| Msg::RescheduleAnswered(r.ok))`.
    - `RescheduleAnswered(true)` behaves like `Msg::Book`.
  - `Msg::DatePicked`'s time picker becomes `Picker::new().title("Pick a time").cancel_label("Back")`, with the per-call `confirm_label` dropped so `done` shows. Add a comment saying the date picker keeps its per-call "Next" to show precedence.
  - Imports: add `ShellLabels` and `with_labels`.
- [ ] **Step 4: Verify.** `cargo test && cargo clippy --all-targets` (the known 2 pre-existing lints are fixed in Task 6), then `cd web && RUSTUP_TOOLCHAIN=stable trunk build`.
- [ ] **Step 5: Commit** `feat(barbershop): showcase app-wide shell labels + a plain confirm` + trailer.

---

### Task 6: Pre-existing demo clippy lints

**Files:** as reported. Baseline on main at planning time:
- **coffee:** `shared/src/app.rs:11` doc-markdown backticks, `too_many_lines` (114/100), a collapsible `if`, `i64 → i32` and `usize → u32` casts, `uninlined_format_args`.
- **todo:** `field_reassign_with_default`, in tests.
- **fullstack-sqlx:** `field_reassign_with_default` ×3, in tests.
- **barbershop:** `needless_borrows_for_generic_args` (~1782), `let_and_return` (~1930).

- [ ] **Step 1:** In each demo, run `cargo clippy --all-targets -- -D warnings` and fix every finding. Keep behaviour identical:
  - Casts: use `try_from(..).unwrap_or(<saturating value>)` or `.min()`-guarded conversions that keep today's in-range results. Use `#[allow(clippy::cast_…, reason = "…")]` only where the value range is provably small, and write the reason.
  - `too_many_lines`: add `#[allow(clippy::too_many_lines, reason = "one match arm per message; splitting hurts readability")]` on the function. Don't refactor.
  - `field_reassign_with_default`: use struct-update syntax `Model { x: …, ..Model::default() }`.
- [ ] **Step 2:** In each of the 4 demos, `cargo clippy --all-targets -- -D warnings` is clean and `cargo test` passes.
- [ ] **Step 3: Commit** `chore(demos): fix pre-existing clippy lints (behaviour-neutral)` + trailer.

---

### Task 7: Sweep (no commit unless fixes)

- [ ] **Rust:** `cargo test` in all demos and the mobiler-web wasm check.
- [ ] **Web:** `trunk build` for all 6 web demos.
- [ ] **Android:** compile saldo and coffee with `mobiler dev --no-install`.
- [ ] **String audit:** `grep -nE '"(Back|‹ Back|Load more|↻ Refresh|OK|Cancel|Done)"'` over the three shells in barbershop and in mobiler-web. Every remaining hit must be a default that sits behind a label lookup.
- [ ] If anything fails, fix it and commit `fix(<area>): …` + trailer.

---

### Task 8: Runtime acceptance (no commit)

**Web (barbershop, CDP, real `Input.dispatchMouseEvent` clicks, watching `Runtime.exceptionThrown`):**
1. The Profile feed's LazyList shows "Show more", and its refresh button reads "↻ Reload".
2. The Services tab: open a service's detail in the Split and its back button reads "‹ Back to shop".
3. Bookings → Reschedule → the confirm shows "No thanks" / "Sure".
4. Two confirms in a row:
   - Open the Reschedule confirm.
   - Without closing it, trigger "Cancel next booking" through `Runtime.evaluate` on its button element: the button's own `.click()` bypasses the scrim, which is what's needed here.
   - Exactly one `.confirm-card` must exist, showing "Cancel booking".
   - Escape → no modal left; a booking was *not* removed; focus is back on the Reschedule button.
5. The focus trap: Tab ×3 and Shift+Tab ×3 inside a modal keep `document.activeElement` inside `.confirm-card`.
6. Coffee: its confirm shows "OK" / "Cancel".
7. No JS exceptions at all.

**Android (barbershop), never leaving the Profile tab:**
1. Bookings → Reschedule → the dialog shows "No thanks" / "Sure". Tap "Sure" → the date picker opens (its "Next" / "Not now" buttons are per-call) → Next → the time picker's accept button reads "Pick" and its cancel reads "Back".
2. Services → open a detail → the Split back button reads "‹ Back to shop".
3. Coffee: the confirm shows "OK" / "Cancel".

Record the evidence under `$SCRATCH/r3-evidence/` and tear everything down.

---

### Task 9: Versions, docs, NOTES, commit; then ship PR-A (ask first)

- [ ] **Versions** per Global Constraints. Refresh every lockfile (`grep -rl 'name = "mobiler-core"' --include=Cargo.lock . | grep -v /target/`, then `cargo update -p mobiler-ui -p mobiler-core [-p mobiler-web]` in each dir). Confirm no lock still has 0.25.0 / 0.36.0.
- [ ] **`capabilities.json`:** Confirm notes gain "defaults from the scaffold's ShellLabels". Then `cargo run -p xtask -- gen-readme` and `-- gen-readme --check`.
- [ ] **README audit:** add a one-line `with_labels` mention wherever `with_theme` / `with_refresh` are listed in the READMEs (`grep -rn "with_refresh" README.md mobiler-core/README.md mobiler/README.md`).
- [ ] **NOTES.md** (gitignored): add a subsection with:
  - Why the labels are set once per scaffold (not per widget), and the precedence order.
  - How each shell stashes them.
  - The one-confirm rule, now converged on all shells.
  - The web focus trap.
  - The lint fixes.
  - The Task 8 results.
  - The remaining out-of-scope strings.
- [ ] **Commit** the versions, locks, docs and this plan file: `chore(release): mobiler-ui 0.26.0, mobiler-core 0.37.0, mobiler-web 0.37.0 + docs` + trailer.
- [ ] **Final whole-branch review**, then **ask the user**, then ship-pr (rebase-merge).

### Task 10: Publish libs (ask first)
- [ ] Run **release-libs**: ui 0.26.0 → core 0.37.0 → `cd mobiler-web && cargo publish` 0.37.0, dry run first for each, and confirm each is indexed before the next.

### Task 11: Template port + CLI 0.54.0 (PR-C)
- [ ] Branch `feat/shell-labels-catalog-cli`. Port the merged barbershop shell hunks (`git diff <PR-A base>..<PR-A head> -- demos/barbershop/{Android,iOS}`) into the template by anchor, keeping `{{NAME}}` / `__PACKAGE_PATH__` tokens. `cp` `Render.swift` if the template matched barbershop's pre-change file.
- [ ] Pin `mobiler-core = "0.37"`, set `mobiler/Cargo.toml` to 0.54.0, then `cargo update -p mobiler` and `cargo test -p mobiler`.
- [ ] Scaffold smoke: a fresh `mobiler new` resolves core 0.37.0 / ui 0.26.0, and `mobiler build android` produces an APK with `libshared.so`. Also grep for `object ActiveLabels`, `enum ActiveLabels` and `private static var open`.
- [ ] Commit, ship-pr (ask the user before merging), squash.

### Task 12: Release CLI + post-release (ask first)
- [ ] Run **release-cli**: packaging pre-check, then tag `v0.54.0`.
- [ ] Then **post-release**:
  - Fresh install and scaffold.
  - An upgrade from 0.53.0 that keeps a user edit.
  - `start.md` and memory.
  - Screenshots, if a README image shows a now-relabelled control. Barbershop's feed and Split aren't in the README images, so probably none.
  - Clean caches.
