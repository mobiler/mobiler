# Shell Labels (Release 2) — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Apps word the confirm dialog, the date/time pickers and the paged list's end text in their own language, and a destructive confirm names its action in the danger colour. Ships as ui 0.25 / core 0.36 / web 0.36 + CLI 0.53.

**Architecture:**
- **Core (`mobiler-core`):** gains the `Confirm` and `Picker` label builders in a new `dialog.rs`, plus `Cx::confirm_with` and `Cx::pick_date_with` / `pick_time_with`. They send the same `dialog`/`confirm` and `datetime`/`date|time` requests with optional JSON fields, so the plain `confirm` stays byte-identical on the wire.
- **`mobiler-ui`:** `Widget::LazyList` gains `end_label: Option<String>`. This is a wire-ABI change. `None` renders nothing, and it's set with `with_end_label`.
- **Android:** the confirm dialog moves from `android.app.AlertDialog` to a Compose Material 3 `AlertDialog`, drawn by `MainActivity` from a `ConfirmHost` bridge so it picks up the theme's error colour and Large density.
- **Web:** `window.confirm` is replaced by a DOM modal.
- **iOS:** uses `.destructive` and the supplied labels.

**Tech Stack:** Rust (facet typegen ABI, crux_core 0.18), Leptos/WASM + CSS (web), Jetpack Compose Material 3 (Android), SwiftUI/UIKit iOS 16+ (iOS).

**Spec:** `docs/superpowers/specs/2026-09-21-render-first-and-shell-labels-design.md` (Release 2 section). Requests: `docs/confirm-dialog-labels.md` (untracked).

## Global Constraints

- **Versions:** `mobiler-ui` 0.24.0 → **0.25.0**, `mobiler-core` 0.35.1 → **0.36.0** (its `mobiler-ui` dep `version = "0.25.0"`), `mobiler-web` 0.35.0 → **0.36.0** (its `mobiler-core` dep `version = "0.36"`), CLI 0.52.1 → **0.53.0**, template pin `mobiler-core = "0.36"`.
- **Wire compatibility:**
  - `cx.confirm(title, message, then)` must serialize **byte-identically** to 0.35: `{"title":…,"message":…}`. Unset optional fields are omitted, and `destructive: false` is omitted.
  - `pick_date` / `pick_time` keep sending input `""`.
  - Every shell accepts both the old and the new input. Missing, empty or unparsable fields fall back to today's text: `OK` / `Cancel` for confirm; `Pick a date` / `Pick a time` / `Cancel` / `Done` for iOS pickers; Android's platform defaults for Android pickers.
- **Confirm response unchanged:** `ok: true` / output `"ok"` on confirm; `ok: false` / output `"cancel"` on the cancel button, back, Escape, or a tap outside the dialog.
- **LazyList end:** with `end_label: None`, an exhausted paged list (`has_more == false`, `on_load_more` set) renders **nothing** at the end on all three shells. `Some(text)` renders `text` exactly where `"End of list"` rendered before, in the same style.
- **Out of scope (do not build):**
  - `LazyList { fill }`.
  - Relabelling the browser's native date/time picker (web accepts the labels and ignores them).
  - Resizing iOS system alerts.
  - Any other hard-coded shell string. Task 9 lists them; it does not fix them.
- **Release shape = two PRs**, because the CI lane `scaffold + build (template, Android)` scaffolds against **crates.io** `mobiler-core`:
  - **PR-A** (`feat/shell-labels`): libs, the 5 demos' shells, the barbershop showcase, version bumps and docs. **The template is untouched.** Rebase-merge, one commit per task.
  - Then `release-libs` publishes ui 0.25.0 → core 0.36.0 → web 0.36.0.
  - **PR-C** (`feat/shell-labels-cli`): port the template's Android `Core.kt` + `MainActivity.kt` and iOS `Core.swift` + `Render.swift`, pin core `"0.36"`, bump the CLI to 0.53.0. Then `release-cli` with tag `v0.53.0`.
- **The five demo shells:** `demos/{coffee,todo,barbershop,saldo}` and `demos/fullstack-todo/mobile`, each with `Android/app/src/main/java/<pkg>/{Core.kt,MainActivity.kt}` and `iOS/Sources/{Core.swift,Render.swift}`.
  - The Android `<pkg>` paths are `dev/mobiler/{coffee,todo,barbershop}`, `rs/mobiler/saldo` and `dev/mobiler/mobile`.
  - iOS `Render.swift` is **byte-identical** in coffee/todo/barbershop/fullstack-todo and the template, but **saldo diverges**.
  - iOS `Core.swift` and both Android files drift in every demo.
  - Rule: edit barbershop first. `cp` barbershop's `Render.swift` to the 3 identical demos. Port everything else **by anchor**; never copy whole drifting files.
- Generated `*/generated/` and `SharedTypes` are gitignored. **Commit source only.**
- **iOS Swift compiles only on macOS CI** (the `iOS build (…)` lanes). Android and web compile locally.
- **Local environment:**
  - Global cargo target dir: `/media/zmilan/data2/cargo-target`. Every `mobiler dev` / `mobiler build` must set `CARGO_TARGET_DIR=<app dir>/target`, or the APK ships without `libshared.so`.
  - The sandbox has blocked `mobiler dev` inside `demos/barbershop`. If that happens, `rsync` the demo into the scratchpad and point its path deps back at the repo crates.
  - Toolchain: `JAVA_HOME=~/jdk21`, `ANDROID_HOME=/home/zmilan/Android/Sdk`.
  - adb: `/home/zmilan/Android/Sdk/platform-tools/adb`.
  - AVD: `mobiler_verify_p7` with `ANDROID_AVD_HOME=/media/zmilan/data2/android-avd`. The fallback is `mobiler_pixel7`, whose storage is small, so uninstall throwaway packages first.
- Commits end with `Co-Authored-By: Claude Opus 5 (1M context) <noreply@anthropic.com>` as a trailer after a blank line. **Never** put it in the subject line.
- `SCRATCH=/tmp/claude-1000/-home-zmilan-working-docker-rust-mobiler/0aff67bb-5734-4226-ad08-fa524bb65d17/scratchpad`

---

### Task 1: LazyList `end_label` — ABI, core builders, web shell

**Files:**
- Modify: `mobiler-ui/src/lib.rs` (`Widget::LazyList` ~line 563; round-trip test ~715)
- Modify: `mobiler-core/src/lib.rs` (`with_refresh` LazyList arm ~1267, `lazy_list` ~1286, `lazy_list_static` ~1299, new `with_end_label`, the builder tests ~1750, the `pub use mobiler_ui::{…}` block is unaffected)
- Modify: `mobiler-web/src/lib.rs` (`Widget::LazyList` arm ~1500)

**Interfaces:**
- Produces: `Widget::LazyList { children, on_load_more, loading, has_more, on_refresh, refreshing, end_label: Option<String> }` (field **last**); `pub fn with_end_label(widget: Widget, label: impl Into<String>) -> Widget` in `mobiler_core`.

- [ ] **Step 1: Branch**

```bash
cd /home/zmilan/working_docker/rust/mobiler && git switch main && git pull --ff-only && git switch -c feat/shell-labels
```

- [ ] **Step 2: Failing tests**

In `mobiler-ui/src/lib.rs`, replace the existing LazyList round-trip line with two:

```rust
        round_trips(&Widget::LazyList { children: vec![Widget::Divider], on_load_more: Some("more".to_string()), loading: false, has_more: true, on_refresh: Some("refresh".to_string()), refreshing: false, end_label: None });
        round_trips(&Widget::LazyList { children: vec![], on_load_more: Some("more".to_string()), loading: false, has_more: false, on_refresh: None, refreshing: false, end_label: Some("Kraj liste".to_string()) });
```

In `mobiler-core/src/lib.rs` builder tests, update the `lazy_list` assertion's pattern to include `end_label: None`, and add after the `with_refresh` assertion:

```rust
        // with_end_label sets the end text; nothing else changes. Default is None (render nothing).
        assert!(matches!(lazy_list_static(vec![text("a")]), Widget::LazyList { end_label: None, .. }));
        assert!(matches!(
            with_end_label(lazy_list(vec![text("a")], false, false, Ev::Tap), "Kraj liste"),
            Widget::LazyList { on_load_more: Some(_), has_more: false, end_label: Some(l), .. } if l == "Kraj liste"
        ));
        // with_refresh keeps an end label set before it, and with_end_label keeps refresh.
        assert!(matches!(
            with_refresh(with_end_label(lazy_list(vec![], false, false, Ev::Tap), "End"), false, Ev::Open(1)),
            Widget::LazyList { on_refresh: Some(_), end_label: Some(l), .. } if l == "End"
        ));
        assert!(matches!(
            with_end_label(with_refresh(lazy_list(vec![], false, false, Ev::Tap), false, Ev::Open(1)), "End"),
            Widget::LazyList { on_refresh: Some(_), end_label: Some(l), .. } if l == "End"
        ));
        // No-op on other widgets.
        assert!(matches!(with_end_label(text("x"), "End"), Widget::Text { .. }));
```

- [ ] **Step 3: Run, expect compile failure**

Run: `cargo test -p mobiler-ui 2>&1 | tail -5` → FAIL: `struct variant Widget::LazyList has no field named end_label`.

- [ ] **Step 4: Add the field**

In `mobiler-ui/src/lib.rs`, add as the **last** field of `LazyList`, and extend its doc comment:

```rust
        /// Text shown under an exhausted paged list (`has_more == false` with `on_load_more` set),
        /// e.g. "You're all caught up" in the app's language. `None` shows nothing. Set with
        /// `with_end_label`.
        end_label: Option<String>,
```

- [ ] **Step 5: Core builders**

In `mobiler-core/src/lib.rs`:
- `with_refresh`'s LazyList arm: destructure `Widget::LazyList { children, on_load_more, loading, has_more, end_label, .. }` and pass `end_label` through.
- `lazy_list` and `lazy_list_static`: add `end_label: None`.
- Add after `lazy_list_static`:

```rust
/// Text shown at the end of an exhausted paged list (a [`lazy_list`] whose `has_more` is false) —
/// e.g. "You're all caught up", in the app's language. Without it nothing is shown there. Combines
/// with [`with_refresh`] in either order. No-op on other widgets.
#[must_use]
pub fn with_end_label(widget: Widget, label: impl Into<String>) -> Widget {
    match widget {
        Widget::LazyList { children, on_load_more, loading, has_more, on_refresh, refreshing, .. } => {
            Widget::LazyList { children, on_load_more, loading, has_more, on_refresh, refreshing, end_label: Some(label.into()) }
        }
        other => other,
    }
}
```

- [ ] **Step 6: Web shell**

In `mobiler-web/src/lib.rs`, the `Widget::LazyList { children, on_load_more, loading, has_more, on_refresh, refreshing }` arm: add `end_label` to the pattern and replace the `end_cap` line with:

```rust
            // The app's own end text (e.g. "Kraj liste"); nothing when it didn't set one.
            let end_cap = (!*has_more && on_load_more.is_some())
                .then(|| end_label.clone())
                .flatten()
                .map(|label| view! { <div class="lazylist-end">{label}</div> });
```

Also update the comment above the arm: `/ "end" caption` → `/ the app's end caption (if set)`.

- [ ] **Step 7: Fix every other compile error**

Run `grep -rn "Widget::LazyList {" --include=*.rs mobiler-core mobiler-web demos | grep -v target` and add `end_label` (or `..`) wherever a full pattern or constructor fails to compile. Demo cores normally use the builders, so expect few hits or none.

- [ ] **Step 8: Verify**

```bash
cargo test -p mobiler-ui && cargo test -p mobiler-core && cargo clippy -p mobiler-core --all-targets -- -D warnings
(cd mobiler-web && cargo check --target wasm32-unknown-unknown && cargo clippy --target wasm32-unknown-unknown -- -D warnings)
```

Expected: all green.

- [ ] **Step 9: Commit**

```bash
git add mobiler-ui/src/lib.rs mobiler-core/src/lib.rs mobiler-web/src/lib.rs
git commit -m "feat(lazylist): app-supplied end_label; nothing shown by default

Co-Authored-By: Claude Opus 5 (1M context) <noreply@anthropic.com>"
```

(Add any demo file Step 7 touched.)

---

### Task 2: LazyList `end_label` — Android shells (5 demos)

**Files:** `MainActivity.kt` in the 5 demos (paths per Global Constraints). The template is **not** touched (PR-C).

**Interfaces:** Consumes Task 1's `end_label` (Kotlin: `widget.endLabel: String?`, regenerated by `mobiler dev`/codegen).

- [ ] **Step 1: Replace the end-cap block** in each demo's `is Widget.LazyList ->` branch. Anchor:

```kotlin
                    } else if (!hasMore && onLoadMore != null) {
                        item {
                            Text(
                                "End of list",
```

Replace the whole `else if` block (through its closing `}` of `item { … }` and the `else if`) with:

```kotlin
                    } else if (!hasMore && onLoadMore != null && endLabel != null) {
                        item {
                            Text(
                                endLabel,
                                color = MaterialTheme.colorScheme.onSurfaceVariant,
                                textAlign = TextAlign.Center,
                                modifier = Modifier.fillMaxWidth().padding(8.dp),
                            )
                        }
                    }
```

and add `val endLabel = widget.endLabel` next to `val hasMore = widget.hasMore` at the top of the branch.

- [ ] **Step 2: Check** `grep -rn '"End of list"' demos --include=MainActivity.kt | grep -v /build/` → no output.

- [ ] **Step 3: Compile barbershop** (regenerates Kotlin types):

```bash
cd /home/zmilan/working_docker/rust/mobiler && cargo build -p mobiler
cd demos/barbershop && CARGO_TARGET_DIR=$PWD/target JAVA_HOME=~/jdk21 ANDROID_HOME=/home/zmilan/Android/Sdk /media/zmilan/data2/cargo-target/debug/mobiler dev --no-install
```

Expected: `BUILD SUCCESSFUL`. Other demos: CI.

- [ ] **Step 4: Commit** the 5 `MainActivity.kt` files: `feat(android): LazyList end label from the app`, with the Co-Authored-By trailer.

---

### Task 3: LazyList `end_label` — iOS shells (5 demos)

**Files:** `demos/barbershop/iOS/Sources/Render.swift` (then `cp` to coffee, todo, fullstack-todo/mobile), `demos/saldo/iOS/Sources/Render.swift` (by anchor).

- [ ] **Step 1: barbershop `Render.swift`**
  - The case (~line 200): `case .lazyList(let children, let onLoadMore, let loading, let hasMore, let onRefresh, let refreshing):` → add `, let endLabel` at the end, and pass `endLabel: endLabel,` into `LazyListView(…)` (before `send:`).
  - `LazyListView`: add `let endLabel: String?` after `let refreshing: Bool`.
  - Replace

```swift
                } else if !hasMore && onLoadMore != nil {
                    Text("End of list").font(.footnote).foregroundColor(.secondary)
                        .frame(maxWidth: .infinity).padding(8)
                }
```

with

```swift
                } else if !hasMore && onLoadMore != nil, let endLabel {
                    Text(endLabel).font(.footnote).foregroundColor(.secondary)
                        .frame(maxWidth: .infinity).padding(8)
                }
```

- [ ] **Step 2: Copy + verify identity**

```bash
for d in demos/coffee demos/todo demos/fullstack-todo/mobile; do cp demos/barbershop/iOS/Sources/Render.swift $d/iOS/Sources/Render.swift; done
md5sum demos/{barbershop,coffee,todo}/iOS/Sources/Render.swift demos/fullstack-todo/mobile/iOS/Sources/Render.swift
```

Expected: four identical hashes. **Before copying**, confirm with `git diff --stat` that only barbershop's Render.swift changed in Step 1 (the four must have been identical beforehand: `git show HEAD:demos/coffee/iOS/Sources/Render.swift | md5sum` equals barbershop's HEAD hash).

- [ ] **Step 3: saldo by anchor.** Apply Step 1's three edits to `demos/saldo/iOS/Sources/Render.swift` (its case is at ~line 202).

- [ ] **Step 4: Check** `grep -rn '"End of list"' demos --include=Render.swift` → no output. Swift compiles on CI only.

- [ ] **Step 5: Commit** the 5 files: `feat(ios): LazyList end label from the app` + trailer.

---

### Task 4: `Confirm` / `Picker` label builders (core)

**Files:**
- Create: `mobiler-core/src/dialog.rs`
- Modify: `mobiler-core/src/lib.rs` (module + re-export near line 10-18; `confirm` ~302; `pick_date`/`pick_time` ~322-332; tests near `cx_confirm_serializes_title_message_and_routes_ok` ~1697)

**Interfaces:**
- Produces (all re-exported at the crate root as `mobiler_core::{Confirm, Picker}`):
  - `Confirm::new(title, message) -> Confirm`
  - `.confirm_label(l)`, `.cancel_label(l)`, `.destructive()`
  - `Picker::new() -> Picker`
  - `.title(t)`, `.confirm_label(l)`, `.cancel_label(l)`
  - `Cx::confirm_with(&mut self, dialog: Confirm, then)`
  - `Cx::pick_date_with(&mut self, picker: Picker, then)`
  - `Cx::pick_time_with(&mut self, picker: Picker, then)`
  - Here `then: impl FnOnce(PluginResponse) -> E + Send + 'static`.
- Wire: confirm input `{"title","message","confirm_label"?,"cancel_label"?,"destructive"?}`. Picker input `{"title"?,"confirm_label"?,"cancel_label"?}` (the `_with` variants always send JSON, `{}` when nothing is set).

- [ ] **Step 1: Failing tests** — add to the lib.rs tests module:

```rust
    #[test]
    fn cx_confirm_stays_byte_identical_on_the_wire() {
        let mut cx = Cx::<Ev>::default();
        cx.confirm("Delete?", "This cannot be undone.", |_| Ev::Tap);
        let (call, _) = cx.requests.pop().unwrap();
        assert_eq!(call.input, r#"{"title":"Delete?","message":"This cannot be undone."}"#);
    }

    #[test]
    fn cx_confirm_with_sends_labels_and_destructive() {
        let mut cx = Cx::<Ev>::default();
        cx.confirm_with(
            Confirm::new("Otkazati termin?", "Klijent dobija obaveštenje.")
                .confirm_label("Otkaži termin")
                .cancel_label("Ne, vrati se")
                .destructive(),
            |r| if r.ok { Ev::Tap } else { Ev::Open(0) },
        );
        let (call, then) = cx.requests.pop().unwrap();
        assert_eq!((call.plugin.as_str(), call.op.as_str()), ("dialog", "confirm"));
        let v: serde_json::Value = serde_json::from_str(&call.input).unwrap();
        assert_eq!(v["title"], "Otkazati termin?");
        assert_eq!(v["message"], "Klijent dobija obaveštenje.");
        assert_eq!(v["confirm_label"], "Otkaži termin");
        assert_eq!(v["cancel_label"], "Ne, vrati se");
        assert_eq!(v["destructive"], true);
        assert!(matches!(then(PluginResponse::text(true, "ok")), Ev::Tap));
        // Unset labels are omitted, not sent as null.
        let mut cx = Cx::<Ev>::default();
        cx.confirm_with(Confirm::new("T", "M").confirm_label("Go"), |_| Ev::Tap);
        let v: serde_json::Value = serde_json::from_str(&cx.requests.pop().unwrap().0.input).unwrap();
        assert!(v.get("cancel_label").is_none() && v.get("destructive").is_none());
    }

    #[test]
    fn cx_pickers_keep_empty_input_and_with_variants_send_labels() {
        let mut cx = Cx::<Ev>::default();
        cx.pick_date(|_| Ev::Tap);
        cx.pick_time(|_| Ev::Tap);
        assert!(cx.requests.iter().all(|(c, _)| c.input.is_empty()));

        let mut cx = Cx::<Ev>::default();
        cx.pick_date_with(Picker::new().title("Izaberi datum").confirm_label("Izaberi").cancel_label("Otkaži"), |_| Ev::Tap);
        cx.pick_time_with(Picker::new(), |_| Ev::Tap);
        let (date, _) = &cx.requests[0];
        assert_eq!((date.plugin.as_str(), date.op.as_str()), ("datetime", "date"));
        let v: serde_json::Value = serde_json::from_str(&date.input).unwrap();
        assert_eq!((v["title"].as_str(), v["confirm_label"].as_str(), v["cancel_label"].as_str()), (Some("Izaberi datum"), Some("Izaberi"), Some("Otkaži")));
        let (time, _) = &cx.requests[1];
        assert_eq!((time.op.as_str(), time.input.as_str()), ("time", "{}"));
    }
```

If `PluginResponse::text` isn't the constructor name, use the one the existing `cx_confirm_serializes_title_message_and_routes_ok` test uses.

- [ ] **Step 2: Run** `cargo test -p mobiler-core cx_confirm_with cx_pickers 2>&1 | tail -5` → FAIL (unresolved `Confirm`/`Picker`/`confirm_with`).

- [ ] **Step 3: Create `mobiler-core/src/dialog.rs`**

```rust
//! Labels for the built-in confirm dialog and date/time pickers, so an app can word the buttons in
//! its own language and say what they do ("Cancel booking" / "Keep it") instead of a generic
//! OK / Cancel. Used with [`Cx::confirm_with`](crate::Cx::confirm_with),
//! [`Cx::pick_date_with`](crate::Cx::pick_date_with) and
//! [`Cx::pick_time_with`](crate::Cx::pick_time_with).

use serde::Serialize;

/// A confirm dialog: title and message, optionally the two button labels, and whether the
/// confirming action is destructive (danger colour on Android and web, `.destructive` on iOS).
/// Unset labels keep the shell defaults (`OK` / `Cancel`).
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Confirm {
    title: String,
    message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    confirm_label: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    cancel_label: Option<String>,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    destructive: bool,
}

impl Confirm {
    #[must_use]
    pub fn new(title: impl Into<String>, message: impl Into<String>) -> Self {
        Self { title: title.into(), message: message.into(), confirm_label: None, cancel_label: None, destructive: false }
    }

    /// The confirming button — name the action ("Cancel booking"), not "OK".
    #[must_use]
    pub fn confirm_label(mut self, label: impl Into<String>) -> Self {
        self.confirm_label = Some(label.into());
        self
    }

    /// The dismissing button — the way out ("Keep it").
    #[must_use]
    pub fn cancel_label(mut self, label: impl Into<String>) -> Self {
        self.cancel_label = Some(label.into());
        self
    }

    /// The confirming action destroys something: the shells draw it in the danger style.
    #[must_use]
    pub fn destructive(mut self) -> Self {
        self.destructive = true;
        self
    }

    pub(crate) fn to_input(&self) -> String {
        serde_json::to_string(self).expect("serialize confirm")
    }
}

/// Labels for the native date/time picker; unset ones keep the shell defaults. The web shell opens
/// the browser's own picker, which draws its chrome in the browser's language and ignores these.
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct Picker {
    #[serde(skip_serializing_if = "Option::is_none")]
    title: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    confirm_label: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    cancel_label: Option<String>,
}

impl Picker {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// The picker's title (iOS action sheet title, Android dialog title).
    #[must_use]
    pub fn title(mut self, title: impl Into<String>) -> Self {
        self.title = Some(title.into());
        self
    }

    /// The button that accepts the picked value.
    #[must_use]
    pub fn confirm_label(mut self, label: impl Into<String>) -> Self {
        self.confirm_label = Some(label.into());
        self
    }

    /// The button that dismisses the picker without a value.
    #[must_use]
    pub fn cancel_label(mut self, label: impl Into<String>) -> Self {
        self.cancel_label = Some(label.into());
        self
    }

    pub(crate) fn to_input(&self) -> String {
        serde_json::to_string(self).expect("serialize picker")
    }
}
```

- [ ] **Step 4: Wire it in `lib.rs`**
  - Add `pub mod dialog;` with the other modules and `pub use dialog::{Confirm, Picker};` with the other re-exports.
  - Replace the body of `confirm` (drop its local `Confirm` struct) with `self.confirm_with(Confirm::new(title, message), then);` and add below it:

```rust
    /// As [`confirm`](Self::confirm), with the app's own button labels and an optional destructive
    /// style — e.g. `Confirm::new("Cancel booking?", msg).confirm_label("Cancel booking")
    /// .cancel_label("Keep it").destructive()`. Same response: `ok` is `true` only if confirmed.
    pub fn confirm_with(&mut self, dialog: Confirm, then: impl FnOnce(PluginResponse) -> E + Send + 'static) {
        self.plugin("dialog", "confirm", dialog.to_input(), then);
    }
```

  - After `pick_date` / `pick_time`, add:

```rust
    /// As [`pick_date`](Self::pick_date), with the app's own title and button labels.
    pub fn pick_date_with(&mut self, picker: Picker, then: impl FnOnce(PluginResponse) -> E + Send + 'static) {
        self.plugin("datetime", "date", picker.to_input(), then);
    }

    /// As [`pick_time`](Self::pick_time), with the app's own title and button labels.
    pub fn pick_time_with(&mut self, picker: Picker, then: impl FnOnce(PluginResponse) -> E + Send + 'static) {
        self.plugin("datetime", "time", picker.to_input(), then);
    }
```

- [ ] **Step 5: Verify** `cargo test -p mobiler-core && cargo clippy -p mobiler-core --all-targets -- -D warnings && cargo test -p mobiler-core --doc` → green (the existing `cx_confirm_serializes_title_message_and_routes_ok` must still pass unchanged).

- [ ] **Step 6: Commit** `mobiler-core/src/dialog.rs mobiler-core/src/lib.rs`: `feat(core): confirm_with / pick_*_with — app-supplied dialog and picker labels` + trailer.

---

### Task 5: Web confirm modal

**Files:**
- Modify: `mobiler-web/src/lib.rs` (the `dialog`/`confirm` branch in `perform` ~864; a new `confirm_modal` fn near `show_toast` ~1120)
- Modify: `mobiler-web/src/mobiler.css` (new block after the `.sheet` rules ~116)
- Modify: `mobiler-web/Cargo.toml` (web-sys features: add `"KeyboardEvent"`, `"DomTokenList"`)

**Interfaces:** Consumes the confirm wire format from Task 4. `take_datetime` is unchanged (it never reads `input`; that is the documented behaviour).

Why it's built this way (keep these properties):
- It is mounted on `<body>`, **not** inside `.scaffold`. Leptos re-renders the tree on every `Render`, which could drop a child we injected mid-dialog and leave the future waiting forever.
- It copies the scaffold's theme onto the scrim: its `style` attribute (brand CSS variables) and its `theme-dark` / `density-large` classes. That way the modal matches dark mode, the brand and Large density.

- [ ] **Step 1: CSS** (append after `@keyframes sheet-rise …`):

```css
/* Confirm dialog (cx.confirm / cx.confirm_with) — the web twin of the native alert. Mounted on
   <body>; the scrim carries the scaffold's theme vars + theme-dark/density-large classes. */
.confirm-scrim { position: fixed; inset: 0; z-index: 30; display: flex; align-items: center; justify-content: center; padding: 16px; background: rgba(0,0,0,0.45); }
.confirm-card { width: 100%; max-width: 360px; background: var(--surface); color: var(--ink); border-radius: var(--radius); padding: 20px 20px 12px; box-shadow: 0 12px 32px rgba(0,0,0,0.3); font-family: var(--font); }
.confirm-title { font-size: 18px; font-weight: 700; margin-bottom: 8px; }
.confirm-message { margin: 0 0 16px; color: var(--muted); }
.confirm-actions { display: flex; justify-content: flex-end; flex-wrap: wrap; gap: 8px; }
```

- [ ] **Step 2: `confirm_modal`** (add after `show_toast`):

```rust
/// What the `dialog`/`confirm` request asks for (see `mobiler_core::Confirm`); missing fields keep
/// the defaults, so a plain `cx.confirm` shows OK / Cancel.
struct ConfirmAsk {
    title: String,
    message: String,
    confirm_label: String,
    cancel_label: String,
    destructive: bool,
}

impl ConfirmAsk {
    fn parse(input: &str) -> Self {
        let v: serde_json::Value = serde_json::from_str(input).unwrap_or(serde_json::Value::Null);
        let s = |k: &str, d: &str| v.get(k).and_then(serde_json::Value::as_str).filter(|s| !s.is_empty()).unwrap_or(d).to_string();
        Self {
            title: s("title", ""),
            message: s("message", ""),
            confirm_label: s("confirm_label", "OK"),
            cancel_label: s("cancel_label", "Cancel"),
            destructive: v.get("destructive").and_then(serde_json::Value::as_bool).unwrap_or(false),
        }
    }
}

/// The web confirm dialog: a modal card on `<body>` that resolves `true` on the confirm button and
/// `false` on the cancel button, Escape or a click on the backdrop. Enter activates the focused
/// button (focus starts on cancel for a destructive dialog, else on confirm). Replaces
/// `window.confirm`, which can't be relabelled or styled.
async fn confirm_modal(ask: ConfirmAsk) -> bool {
    use wasm_bindgen::{closure::Closure, JsCast};
    let Some(doc) = web_sys::window().and_then(|w| w.document()) else { return false };
    let Some(body) = doc.body() else { return false };
    let el = |tag: &str, class: &str| -> Option<web_sys::HtmlElement> {
        let e = doc.create_element(tag).ok()?.dyn_into::<web_sys::HtmlElement>().ok()?;
        e.set_class_name(class);
        Some(e)
    };
    let confirm_class = if ask.destructive { "btn btn-filled btn-danger" } else { "btn btn-filled" };
    let (Some(scrim), Some(card), Some(message), Some(actions), Some(cancel), Some(confirm)) = (
        el("div", "confirm-scrim"),
        el("div", "confirm-card"),
        el("p", "confirm-message"),
        el("div", "confirm-actions"),
        el("button", "btn btn-text"),
        el("button", confirm_class),
    ) else {
        return false;
    };

    // Inherit the scaffold's theme: brand vars (inline style) + dark / Large-density classes.
    if let Some(scaffold) = doc.query_selector(".scaffold").ok().flatten() {
        if let Some(style) = scaffold.get_attribute("style") {
            let _ = scrim.set_attribute("style", &style);
        }
        let classes = scaffold.class_list();
        for c in ["theme-dark", "density-large"] {
            if classes.contains(c) {
                let _ = scrim.class_list().add_1(c);
            }
        }
    }

    let _ = card.set_attribute("role", "alertdialog");
    let _ = card.set_attribute("aria-modal", "true");
    if !ask.title.is_empty() {
        if let Some(title) = el("div", "confirm-title") {
            title.set_text_content(Some(&ask.title));
            let _ = card.append_child(&title);
        }
    }
    message.set_text_content(Some(&ask.message));
    cancel.set_text_content(Some(&ask.cancel_label));
    confirm.set_text_content(Some(&ask.confirm_label));
    let _ = actions.append_child(&cancel);
    let _ = actions.append_child(&confirm);
    let _ = card.append_child(&message);
    let _ = card.append_child(&actions);
    let _ = scrim.append_child(&card);
    let _ = body.append_child(&scrim);
    let _ = if ask.destructive { cancel.focus() } else { confirm.focus() };

    let (tx, rx) = futures_channel::oneshot::channel::<bool>();
    let tx = std::rc::Rc::new(std::cell::RefCell::new(Some(tx)));
    let answer = move |tx: &std::rc::Rc<std::cell::RefCell<Option<futures_channel::oneshot::Sender<bool>>>>, ok: bool| {
        if let Some(tx) = tx.borrow_mut().take() {
            let _ = tx.send(ok);
        }
    };
    let (t1, t2, t3, t4) = (tx.clone(), tx.clone(), tx.clone(), tx.clone());
    let on_cancel = Closure::wrap(Box::new(move || answer(&t1, false)) as Box<dyn FnMut()>);
    let on_confirm = Closure::wrap(Box::new(move || answer(&t2, true)) as Box<dyn FnMut()>);
    let scrim_node: web_sys::EventTarget = scrim.clone().into();
    let on_backdrop = Closure::wrap(Box::new(move |e: web_sys::Event| {
        // Only a click on the backdrop itself, not one that bubbled up from the card.
        if e.target().as_ref() == Some(&scrim_node) {
            answer(&t3, false);
        }
    }) as Box<dyn FnMut(web_sys::Event)>);
    let on_key = Closure::wrap(Box::new(move |e: web_sys::KeyboardEvent| {
        if e.key() == "Escape" {
            answer(&t4, false);
        }
    }) as Box<dyn FnMut(web_sys::KeyboardEvent)>);
    cancel.set_onclick(Some(on_cancel.as_ref().unchecked_ref()));
    confirm.set_onclick(Some(on_confirm.as_ref().unchecked_ref()));
    let _ = scrim.add_event_listener_with_callback("click", on_backdrop.as_ref().unchecked_ref());
    let _ = doc.add_event_listener_with_callback("keydown", on_key.as_ref().unchecked_ref());

    let ok = rx.await.unwrap_or(false);
    let _ = doc.remove_event_listener_with_callback("keydown", on_key.as_ref().unchecked_ref());
    scrim.remove();
    ok
}
```

The closures are dropped when the function returns, after the listener is removed and the scrim is gone, so there's no `forget()` and nothing leaks. The `answer` closure is `Copy` (it captures nothing), so it can be used in all four handlers. If the compiler disagrees, turn it into a plain `fn answer(tx: &Rc<RefCell<Option<Sender<bool>>>>, ok: bool)`.

- [ ] **Step 3: Use it in `perform`.** Replace the whole `if call.plugin == "dialog" && call.op == "confirm" { … }` block with:

```rust
    if call.plugin == "dialog" && call.op == "confirm" {
        let ok = confirm_modal(ConfirmAsk::parse(&call.input)).await;
        return PluginResponse::text(ok, if ok { "ok" } else { "cancel" });
    }
```

- [ ] **Step 4: Cargo features.** Add `"KeyboardEvent"` and `"DomTokenList"` (needed by `class_list()`) to `mobiler-web/Cargo.toml`'s web-sys feature list (the first line of the list is fine). If the compiler names another missing feature, add it the same way.

- [ ] **Step 5: Verify**

```bash
cd mobiler-web && cargo check --target wasm32-unknown-unknown && cargo clippy --target wasm32-unknown-unknown -- -D warnings
```

Expected: green. The runtime check is in Task 10.

- [ ] **Step 6: Commit** `mobiler-web/src/lib.rs mobiler-web/src/mobiler.css mobiler-web/Cargo.toml mobiler-web/Cargo.lock` (if the lock changed): `feat(web): confirm dialog as a themed modal with app labels` + trailer.

---

### Task 6: Android — Compose confirm dialog + picker labels (5 demos)

**Files:** in each of the 5 demos, `Core.kt` (DialogPlugin, DateTimePlugin, a new `ConfirmHost`) and `MainActivity.kt` (a new `ConfirmDialog` composable, drawn from `App()`). Edit barbershop first, compile, then port by anchor.

**Interfaces:**
- Produces, in `Core.kt`:
  - `class ConfirmRequest(title, message, confirmLabel, cancelLabel, destructive, answer: (Boolean) -> Unit)`
  - `object ConfirmHost { var pending: ConfirmRequest? }`, backed by Compose state
- Consumed by `MainActivity.kt`.

- [ ] **Step 1: `Core.kt` — replace `DialogPlugin`** (the whole class and its KDoc) with:

```kotlin
/** A confirm dialog waiting for the user. DialogPlugin sets it; MainActivity's composition draws it
 *  as a Material 3 AlertDialog (so it gets the theme's error colour and Density.LARGE sizes) and
 *  calls [answer] with the choice. */
class ConfirmRequest(
    val title: String,
    val message: String,
    val confirmLabel: String,
    val cancelLabel: String,
    val destructive: Boolean,
    val answer: (Boolean) -> Unit,
)

object ConfirmHost {
    var pending by mutableStateOf<ConfirmRequest?>(null)
}

/** Official, bundled plugin: confirm dialog (request/response). Input is JSON
 *  {title, message, confirm_label?, cancel_label?, destructive?}; missing labels keep OK / Cancel.
 *  Resolves ok=true when confirmed; cancel, back or a tap outside resolve ok=false. */
class DialogPlugin : MobilerPlugin {
    override suspend fun handle(op: String, input: String): PluginResponse {
        if (op != "confirm") return PluginResponse(false, "unknown op '$op'")
        if (MobilerActivity.current?.get() == null) return PluginResponse(false, "no activity")
        val obj = runCatching { JSONObject(input) }.getOrElse { JSONObject() }
        return withContext(Dispatchers.Main) {
            suspendCancellableCoroutine { cont ->
                lateinit var request: ConfirmRequest
                request = ConfirmRequest(
                    title = obj.optString("title"),
                    message = obj.optString("message"),
                    confirmLabel = obj.optString("confirm_label").ifEmpty { "OK" },
                    cancelLabel = obj.optString("cancel_label").ifEmpty { "Cancel" },
                    destructive = obj.optBoolean("destructive", false),
                ) { ok ->
                    if (ConfirmHost.pending === request) ConfirmHost.pending = null
                    if (cont.isActive) cont.resumeWith(Result.success(PluginResponse(ok, if (ok) "ok" else "cancel")))
                }
                // One dialog at a time: a still-open one is answered "cancel" before the new one shows.
                ConfirmHost.pending?.answer?.invoke(false)
                ConfirmHost.pending = request
                cont.invokeOnCancellation { if (ConfirmHost.pending === request) ConfirmHost.pending = null }
            }
        }
    }
}
```

Imports: `Core.kt` already imports `mutableStateOf`/`getValue`/`setValue`. Remove `import android.app.AlertDialog` if nothing else in the file uses it (`grep -n "AlertDialog" Core.kt`).

- [ ] **Step 2: `Core.kt` — picker labels.** In `DateTimePlugin.handle`, after the `"now"` early return and the `activity` line, add:

```kotlin
        // Optional app labels ({title?, confirm_label?, cancel_label?}); "" (plain pick_date) → defaults.
        val labels = runCatching { JSONObject(input) }.getOrNull()
        val title = labels?.optString("title").orEmpty()
        val confirmLabel = labels?.optString("confirm_label").orEmpty()
        val cancelLabel = labels?.optString("cancel_label").orEmpty()
```

and in both the `"date"` and `"time"` branches, right before `dlg.show()`:

```kotlin
                        if (title.isNotEmpty()) dlg.setTitle(title)
                        // The dialog is its own click listener: POSITIVE delivers the value, NEGATIVE cancels.
                        if (confirmLabel.isNotEmpty()) dlg.setButton(DialogInterface.BUTTON_POSITIVE, confirmLabel, dlg)
                        if (cancelLabel.isNotEmpty()) dlg.setButton(DialogInterface.BUTTON_NEGATIVE, cancelLabel, dlg)
```

Add `import android.content.DialogInterface`. (AOSP `DatePickerDialog`/`TimePickerDialog` implement `DialogInterface.OnClickListener`: POSITIVE → the value callback, NEGATIVE → `cancel()` → our `setOnCancelListener` → `ok=false`. Task 10 verifies this at runtime.)

- [ ] **Step 3: `MainActivity.kt` — draw the dialog.** In `App()`, inside `{{NAME}}Theme(…) { … }` (in demos: the demo's theme function) and **after** the `Surface(...) { … }` block, add:

```kotlin
        // A pending confirm from the dialog capability (see ConfirmHost in Core.kt).
        ConfirmHost.pending?.let { ConfirmDialog(it) }
```

and add the composable (e.g. just below `App`):

```kotlin
// The confirm capability's dialog: the app's own labels, the confirming action in the theme's error
// colour when destructive, and Density.LARGE button height + label size like other buttons.
@Composable
private fun ConfirmDialog(req: ConfirmRequest) {
    val large = isLarge
    val labelSize = if (large) 16.sp else TextUnit.Unspecified
    val buttonModifier = if (large) Modifier.heightIn(min = 56.dp) else Modifier
    AlertDialog(
        onDismissRequest = { req.answer(false) },
        title = if (req.title.isNotEmpty()) { { Text(req.title) } } else null,
        text = { Text(req.message) },
        confirmButton = {
            TextButton(
                onClick = { req.answer(true) },
                modifier = buttonModifier,
                colors = if (req.destructive) ButtonDefaults.textButtonColors(contentColor = MaterialTheme.colorScheme.error) else ButtonDefaults.textButtonColors(),
            ) { Text(req.confirmLabel, fontSize = labelSize) }
        },
        dismissButton = {
            TextButton(onClick = { req.answer(false) }, modifier = buttonModifier) { Text(req.cancelLabel, fontSize = labelSize) }
        },
    )
}
```

Import `androidx.compose.material3.AlertDialog` if missing (check first; `TextButton` and `ButtonDefaults` are already imported in the template-derived files).

- [ ] **Step 4: Compile barbershop** (the Task 2 Step 3 command). Expect `BUILD SUCCESSFUL`.

- [ ] **Step 5: Port to coffee, todo, saldo, fullstack-todo/mobile.** Same four edits by anchor: the `class DialogPlugin` block, the `DateTimePlugin` insertions, and the `App()` line plus the `ConfirmDialog` composable.

  Check each:
  - `grep -c "object ConfirmHost" Core.kt` = 1
  - `grep -c "ConfirmHost.pending?.let" MainActivity.kt` = 1
  - `grep -c 'setPositiveButton("OK")' Core.kt` = 0

  Then extract the `DialogPlugin` block from each file and md5 it; all 5 must match.

- [ ] **Step 6: Commit** the 10 files: `feat(android): confirm dialog in Compose with app labels + danger style; picker labels` + trailer.

---

### Task 7: iOS — confirm labels / destructive + picker labels (5 demos)

**Files:** `iOS/Sources/Core.swift` in all 5 demos (drifting — edit barbershop, port by anchor).

- [ ] **Step 1: `DialogPlugin`.** After `let message = …`, add:

```swift
        // Optional app labels; a plain `cx.confirm` sends none and keeps OK / Cancel.
        let confirmLabel = (obj?["confirm_label"] as? String).flatMap { $0.isEmpty ? nil : $0 } ?? "OK"
        let cancelLabel = (obj?["cancel_label"] as? String).flatMap { $0.isEmpty ? nil : $0 } ?? "Cancel"
        let destructive = obj?["destructive"] as? Bool ?? false
```

Replace `UIAlertAction(title: "Cancel", style: .cancel)` with `UIAlertAction(title: cancelLabel, style: .cancel)`. Replace `UIAlertAction(title: "OK", style: .default)` with `UIAlertAction(title: confirmLabel, style: destructive ? .destructive : .default)`.

Update the doc comment's input line to `Input is JSON {title, message, confirm_label?, cancel_label?, destructive?}; system alerts keep their own size.`

- [ ] **Step 2: `DateTimePlugin`.** After the `default: return …` of the `switch op`, add:

```swift
        // Optional app labels ({title?, confirm_label?, cancel_label?}); "" (plain pick_date) → defaults.
        let labels = (try? JSONSerialization.jsonObject(with: Data(input.utf8))) as? [String: Any]
        func label(_ key: String, _ fallback: String) -> String {
            (labels?[key] as? String).flatMap { $0.isEmpty ? nil : $0 } ?? fallback
        }
```

Then change the sheet title to `label("title", op == "date" ? "Pick a date" : "Pick a time")`, `"Cancel"` to `label("cancel_label", "Cancel")`, and `"Done"` to `label("confirm_label", "Done")`.

- [ ] **Step 3: Port** to coffee, todo, saldo, fullstack-todo/mobile by anchor (the anchors were confirmed to exist in all 5 during planning). Check each with:
  - `grep -c 'title: "OK", style: .default' Core.swift` = 0
  - `grep -c 'destructive ? .destructive : .default' Core.swift` = 1
- [ ] **Step 4: Commit** the 5 files: `feat(ios): confirm and picker labels from the app; destructive style` + trailer. Swift compiles on CI only.

---

### Task 8: Barbershop showcase + tests

**Files:** `demos/barbershop/app-core/src/lib.rs`

**Interfaces:** Consumes `Confirm`, `Picker`, `with_end_label` (Tasks 1, 4).

- [ ] **Step 1: Failing tests** (in the demo's `#[cfg(test)] mod tests`):

```rust
    // Drives the full shell so the test sees the plugin requests the update emits.
    fn dialog_inputs(msg: Msg) -> Vec<(String, String, serde_json::Value)> {
        use crux_core::App as _;
        let shell = App::default();
        let mut model = Model::default();
        let mut cmd = shell.update(mobiler_core::Action::Fired { token: serde_json::to_string(&msg).unwrap() }, &mut model);
        cmd.effects()
            .filter_map(|e| match e {
                mobiler_core::Effect::Plugin(r) => Some((
                    r.operation.plugin.clone(),
                    r.operation.op.clone(),
                    serde_json::from_str(&r.operation.input).unwrap_or(serde_json::Value::Null),
                )),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn cancel_next_asks_with_destructive_labels() {
        let calls = dialog_inputs(Msg::AskCancelNext);
        assert_eq!(calls.len(), 1);
        let (plugin, op, v) = &calls[0];
        assert_eq!((plugin.as_str(), op.as_str()), ("dialog", "confirm"));
        assert_eq!(v["confirm_label"], "Cancel booking");
        assert_eq!(v["cancel_label"], "Keep it");
        assert_eq!(v["destructive"], true);
    }

    #[test]
    fn cancel_next_answer_removes_only_when_confirmed() {
        let (app, mut model) = app();
        let mut cx = Cx::<Msg>::default();
        let before = model.bookings.len();
        app.update(Msg::CancelNextAnswered(false), &mut model, &mut cx);
        assert_eq!(model.bookings.len(), before);
        app.update(Msg::CancelNextAnswered(true), &mut model, &mut cx);
        assert_eq!(model.bookings.len(), before - 1);
    }

    #[test]
    fn booking_flow_pickers_carry_labels() {
        let calls = dialog_inputs(Msg::Book);
        let (plugin, op, v) = &calls[0];
        assert_eq!((plugin.as_str(), op.as_str()), ("datetime", "date"));
        assert_eq!(v["title"], "Pick a day");
    }
```

If `Action`/`Effect` aren't re-exported by `mobiler_core` under those names, use the paths the demo's other tests or `demos/fullstack-sqlx/app-core/src/lib.rs:258` use.

- [ ] **Step 2: Run** `cd demos/barbershop && cargo test` → FAIL (no `AskCancelNext`/`CancelNextAnswered`).

- [ ] **Step 3: Implement**
  - `Msg`: add `AskCancelNext` and `CancelNextAnswered(bool)` next to `CancelBooking(u32)`.
  - `update`:

```rust
            // The destructive "Cancel next booking" button asks first, naming the action on the
            // confirm button (cx.confirm_with) instead of a generic OK / Cancel.
            Msg::AskCancelNext => cx.confirm_with(
                Confirm::new("Cancel your next booking?", "Your barber will be notified.")
                    .confirm_label("Cancel booking")
                    .cancel_label("Keep it")
                    .destructive(),
                |r| Msg::CancelNextAnswered(r.ok),
            ),
            Msg::CancelNextAnswered(ok) => {
                if ok && !model.bookings.is_empty() {
                    let b = model.bookings.remove(0);
                    cx.toast(format!("Cancelled {b}"));
                }
            }
```

  - The `Cancel next booking` `button_with(…)`: `Msg::CancelBooking(0)` → `Msg::AskCancelNext`. Leave "No-show" and the swipe action as they are.
  - `Msg::Book`: replace `cx.pick_date(` with `cx.pick_date_with(Picker::new().title("Pick a day").confirm_label("Next").cancel_label("Not now"), `.
  - `Msg::DatePicked`: replace `cx.pick_time(` with `cx.pick_time_with(Picker::new().title("Pick a time").confirm_label("Next").cancel_label("Back"), `.
  - `Msg::TimePicked`: replace `cx.confirm(` with `cx.confirm_with(Confirm::new("Confirm booking", format!("Book your visit for {date} at {time}?")).confirm_label("Book it").cancel_label("Not yet"), `.
  - `feed_card`: wrap the list: `let list = with_end_label(with_refresh(…), "You're all caught up");`.
  - Imports: add `Confirm, Picker, with_end_label` to the `use mobiler_core::{…}` list.

- [ ] **Step 4: Verify** `cd demos/barbershop && cargo test && cargo clippy --all-targets -- -D warnings`. Build the web demo with `cd web && RUSTUP_TOOLCHAIN=stable trunk build` (if trunk fails for an environment reason, record it and continue; Task 10 builds it again).

- [ ] **Step 5: Commit** `demos/barbershop/app-core/src/lib.rs`: `feat(barbershop): showcase confirm_with, picker labels and the list end label` + trailer.

---

### Task 9: Sweep — every consumer compiles, leftover-strings audit (no commit unless fixes)

- [ ] **Step 1:** Rust: `cargo test` + `cargo clippy --all-targets -- -D warnings` in `demos/{coffee,todo,saldo,fullstack-sqlx,fullstack-todo,barbershop}`; `cd mobiler-web && cargo check --target wasm32-unknown-unknown`. Web demos: `RUSTUP_TOOLCHAIN=stable trunk build` in each `demos/*/web*` dir that has a `Trunk.toml` or `index.html`.
- [ ] **Step 2:** Android: compile one more demo besides barbershop (coffee) with the Task 2 Step 3 command, to catch a port slip.
- [ ] **Step 3: Audit** for remaining hard-coded English UI strings in the shells:

```bash
grep -nE '"(OK|Cancel|Done|Load more|↻ Refresh|Pick a date|Pick a time|End of list)"' mobiler-web/src/lib.rs demos/barbershop/Android/app/src/main/java/dev/mobiler/barbershop/*.kt demos/barbershop/iOS/Sources/*.swift
```

  Record every hit that is **not** a documented default (for example web's "Load more" and "↻ Refresh") in the NOTES entry (Task 11) as a follow-up. Do not fix them.
- [ ] **Step 4:** If a step fails, fix the cause in the owning task's files and commit `fix(<area>): …` + trailer.

---

### Task 10: Runtime acceptance — Android emulator + web (no commit)

Everything is throwaway in `$SCRATCH`. Record pass/fail plus evidence (uiautomator bounds, screenshots) for the PR body.

- [ ] **Step 1: Web (barbershop).** Build `demos/barbershop/web` (`RUSTUP_TOOLCHAIN=stable trunk build`), serve `dist/` with `python3 -m http.server <port>`, then drive it with headless Chrome over CDP.
  - Start Chrome with `google-chrome --headless=new --remote-debugging-port=9222 --remote-allow-origins=* about:blank`.
  - Use a Node 22 script: GET `/json`, take the `page` target's `webSocketDebuggerUrl`, then `Page.navigate`, `Runtime.evaluate` and `Page.captureScreenshot`. Don't use `/json/new`, which is blocked.
  - Checks:
    1. Click "Cancel next booking" → a `.confirm-card` exists with buttons "Keep it" and "Cancel booking". The confirm button has the class `btn-danger`, and `document.activeElement` is the "Keep it" button. Take a screenshot.
    2. Press Escape (dispatch a `keydown` with key `Escape` on `document`) → the modal is gone and the booking count is unchanged.
    3. Reopen, click the backdrop (`.confirm-scrim` itself) → gone, count unchanged.
    4. Reopen, click "Cancel booking" → gone, one booking fewer, toast "Cancelled …".
    5. Switch on the barbershop's "Large controls" toggle (Profile), reopen → the scrim has `density-large` and the confirm button's height is ≥ 56 px (`getBoundingClientRect`). Barbershop has no dark-mode toggle: check `theme-dark` inheritance by evaluating `document.querySelector(".scaffold").classList.add("theme-dark")` before reopening, then confirm the scrim carries `theme-dark`. Take a screenshot.
    6. Go to the feed and click "Load more" until it disappears → the last element of `.lazylist` is `.lazylist-end` with the text "You're all caught up".
    7. Coffee web (plain `cx.confirm`): its confirm shows "OK" / "Cancel", and the confirm button has no `btn-danger`.
- [ ] **Step 2: Android (barbershop).** Boot the AVD (`ANDROID_AVD_HOME=/media/zmilan/data2/android-avd emulator -avd mobiler_verify_p7 -no-window -no-audio -no-snapshot`), wait for `sys.boot_completed` in a loop, and install with `mobiler dev` (`CARGO_TARGET_DIR` set; see Global Constraints about the sandbox and rsync).
  1. Tap "Cancel next booking" → a dialog with "Keep it" and "Cancel booking". Take a screenshot. The "Cancel booking" text colour must be the theme's error red (check it in the screenshot).
  2. Tap "Keep it" → dialog gone, count unchanged. Reopen → press BACK → gone, unchanged. Reopen → tap outside the dialog → gone, unchanged. Reopen → tap "Cancel booking" → one fewer.
  3. Turn on the Large toggle and reopen. With `D=$(adb shell wm density | awk '{print $NF}' | tr -d '\r')`, each dialog button's bounds height in px must be ≥ 56 × D / 160. Take a screenshot.
  4. Tap "Book now" → the date picker's title is "Pick a day", with buttons "Next" and "Not now". "Not now" → no time picker follows. "Book now" again → "Next" → the time picker shows "Pick a time" / "Next" / "Back" → "Next" → the confirm shows "Book it" / "Not yet".
  5. Feed: scroll until the list is exhausted → "You're all caught up" is the last row.
  6. Coffee (plain `cx.confirm`): its dialog shows "OK" / "Cancel" (the upper-case rendering from the old dialog no longer applies; Material 3 text buttons show the label as given).
- [ ] **Step 3:** Tear down: kill the emulator, the http servers and Chrome, and delete the scratch copies.

---

### Task 11: Versions, docs, ship PR-A

- [ ] **Step 1: Versions.**
  - `mobiler-ui/Cargo.toml` → `version = "0.25.0"`.
  - `mobiler-core/Cargo.toml` → `version = "0.36.0"`, with `mobiler-ui = { path = "../mobiler-ui", version = "0.25.0" }`.
  - `mobiler-web/Cargo.toml` → `version = "0.36.0"`, with `mobiler-core = { path = "../mobiler-core", version = "0.36" }`.
  - The CLI and template are untouched here (PR-C).
  - Refresh every lockfile that contains mobiler-ui or mobiler-core. Find them with `grep -rl 'name = "mobiler-core"' --include=Cargo.lock . | grep -v /target/` and run `cargo update -p mobiler-ui -p mobiler-core` in each dir (plus `-p mobiler-web` where present).
  - Confirm with grep that no lock still says `0.24.0` for ui or `0.35.1` for core.
- [ ] **Step 2: Capability docs.** In `capabilities.json`:
  - Confirm `api`: `"cx.confirm(title, message, then) · cx.confirm_with(Confirm::new(t, m).confirm_label(…).cancel_label(…).destructive(), then)"`, notes `"native dialog; app labels + destructive style"`.
  - Date/time `api`s: add ` · cx.pick_date_with(Picker::new().title(…).confirm_label(…).cancel_label(…), then)` (and the same for `pick_time_with`).
  - Then run `cargo run -p xtask -- gen-readme` and `cargo run -p xtask -- gen-readme --check`.
- [ ] **Step 3: README audit.** Search `README.md`, `mobiler-core/README.md`, `mobiler-web/README.md` and `mobiler/README.md` for `End of list`, `window.confirm`, `confirm(` and `lazy_list`. Update any prose that describes the old behaviour, and add a one-line mention of `with_end_label` wherever `lazy_list` builders are listed.
- [ ] **Step 4: NOTES.md** (gitignored, edit only). Add a subsection "Shell labels (ui 0.25 / core 0.36 / web 0.36 / CLI 0.53)" that covers the *why*:
  - `Confirm`/`Picker` builders instead of long argument lists.
  - Byte-identical plain `confirm`.
  - The Android dialog moved into Compose through `ConfirmHost`, for the theme colours and Large density.
  - The web modal is mounted on body and copies the scaffold's theme, because Leptos re-renders.
  - The iOS system alert is left at its own size.
  - The web date/time picker can't be relabelled.
  - LazyList `None` shows nothing.
  - Task 10 results.
  - Task 9's list of leftover strings.
  - Bump `*Last updated:*`.
- [ ] **Step 5: Commit** the versions and lockfiles, `capabilities.json`, the regenerated READMEs and this plan file (`docs/superpowers/plans/2026-09-21-shell-labels-release-2.md`): `chore(release): mobiler-ui 0.25.0, mobiler-core 0.36.0, mobiler-web 0.36.0 + docs` + trailer. Check `git status` first, and never add `docs/confirm-dialog-labels.md`, `docs/render-before-requests.md`, `docs/large-touch-targets.md` or `.superpowers/`.
- [ ] **Step 6: Ship PR-A** with the **ship-pr** skill on `feat/shell-labels`.
  - The PR body covers what changed, Task 10's evidence, and the note that the template and CLI follow in PR-C.
  - All CI lanes must be green, including the 3+ `iOS build (…)` lanes (the first Swift compile of Tasks 3 and 7) and every `Android build (…)` lane.
  - **Rebase-merge.** Ask the user before merging.

---

### Task 12: Publish libs

- [ ] **release-libs** skill: publish `mobiler-ui` 0.25.0, then `mobiler-core` 0.36.0, then `cd mobiler-web && cargo publish` for 0.36.0, confirming each is indexed before the next. **Ask the user before the first `cargo publish`.**

---

### Task 13: Template port, pin, CLI 0.53.0 (PR-C)

**Files:**
- `mobiler/templates/Android/app/src/main/java/__PACKAGE_PATH__/{Core.kt,MainActivity.kt}`
- `mobiler/templates/iOS/Sources/{Core.swift,Render.swift}`
- `mobiler/templates/shared/Cargo.toml.tmpl`
- `mobiler/Cargo.toml`
- `Cargo.lock`
- `mobiler/README.md` if it lists capabilities

- [ ] **Step 1:** `git switch main && git pull --ff-only && git switch -c feat/shell-labels-cli`.
- [ ] **Step 2: Port**, keeping each file's own package name and theme names:
  - iOS `Render.swift`: `cp demos/barbershop/iOS/Sources/Render.swift mobiler/templates/iOS/Sources/Render.swift`, since they were identical before. Verify with `git diff --stat` that the only diff is the Task 3 hunk.
  - Android `Core.kt` / `MainActivity.kt` and iOS `Core.swift`: apply the Task 2, 6 and 7 edits by anchor. In `MainActivity.kt` the theme call is `{{NAME}}Theme(`.
- [ ] **Step 3: Pin and bump.** Set `mobiler-core = "0.36"` in `Cargo.toml.tmpl` and `version = "0.53.0"` in `mobiler/Cargo.toml`, then run `cargo update -p mobiler` and `cargo build -p mobiler && cargo test -p mobiler`.
- [ ] **Step 4: Scaffold smoke.** Run `mobiler new ts2 --package dev.mobiler.ts2` in `$SCRATCH`. Then `cargo build -p shared` must resolve mobiler-core 0.36.0 from crates.io, and `CARGO_TARGET_DIR=$PWD/target mobiler build android` must produce an APK.
- [ ] **Step 5: Commit** `feat(cli): template shells for app dialog/picker labels + list end label, mobiler 0.53.0` + trailer. **ship-pr** (squash is fine), with all CI green including `scaffold + build (template, Android)` and the iOS template lane if one exists.

---

### Task 14: Release CLI + post-release

- [ ] **release-cli** (packaging pre-check, then tag `v0.53.0`). **Ask the user before tagging.** Then **post-release**:
  - Fresh install and scaffold.
  - An upgrade from 0.52.1 with a user edit kept.
  - `start.md` and memory.
  - **Screenshots:** the barbershop confirm dialog changed visibly, so refresh the barbershop README screenshots on the post-release recipe.
  - Clean the caches.
  - Tell the appointments team what to run: `mobiler upgrade --apply`, then use `confirm_with`, `pick_*_with` and `with_end_label`.
  - Delete `docs/confirm-dialog-labels.md` once they confirm.
