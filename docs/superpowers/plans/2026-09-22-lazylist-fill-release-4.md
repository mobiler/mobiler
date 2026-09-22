# LazyList Fill + Polish (Release 4) — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:**
- A `LazyList` can fill the rest of the screen (`with_fill`), in place of stopping at a fixed height inside a page scroll.
- The last shell-drawn English strings (the Android PDF error and the web iframe titles) become `ShellLabels`.
- iOS reads its theme from the root scaffold only, and a replacement confirm always appears.

**Architecture:**
- **ABI:** `LazyList.fill: bool` (last field) and three new `ShellLabels` fields after `done`. That makes mobiler-ui 0.27.
- **Core:** `with_fill` plus the builder setters. mobiler-core 0.38.
- **Shells:** each shell applies the same fill rule. The body is the fill list, or a `Column` with a fill list as a direct child (first one only). In that case the body stops page-scrolling and the list takes the remaining height. Anything else keeps today's cap.

**Tech Stack:** Rust (facet typegen), Leptos/WASM + CSS, Jetpack Compose M3, SwiftUI/UIKit.

**Spec:** `docs/superpowers/specs/2026-09-22-lazylist-fill-and-polish-design.md`.

## Global Constraints

- **Versions:**
  - `mobiler-ui` 0.26.0 → **0.27.0**
  - `mobiler-core` 0.37.0 → **0.38.0**; its `mobiler-ui` dep becomes `"0.27.0"`
  - `mobiler-web` 0.37.0 → **0.38.0**; its `mobiler-core` dep becomes `"0.38"`
  - CLI 0.54.0 → **0.55.0**; template pin `mobiler-core = "0.38"`
- **ABI:**
  - `Widget::LazyList` field order: `children, on_load_more, loading, has_more, on_refresh, refreshing, end_label, fill: bool`. `fill` is **last**.
  - `ShellLabels` field order: `back, load_more, refresh, ok, cancel, done, pdf_error, pdf_title, web_title`.
- **Fill rule (identical in every shell):** the scaffold `body` is in fill mode when either:
  - the body is `LazyList { fill: true }`, or
  - the body is `Column` and one of its **direct** children is `LazyList { fill: true }`. Only the **first** such child counts.

  Every other `fill: true` list keeps today's cap: Android `heightIn(max = 480.dp)`, iOS `.frame(height: 420)`, web `max-height: 60vh`.
- **In fill mode:** there is no page scroll. Children before and after the fill list keep their natural height. The fill list takes the remaining height and scrolls itself; load-more and pull-to-refresh are unchanged.
- **Scaffolds with no fill list must render exactly as they do today.**
- **Labels** follow the Release 3 rules: scaffold label, else default, with empty treated as absent. The new defaults are:
  - Android PDF error: `"PDF: $error"` when no label is set; with a label, the label alone.
  - Web iframe titles: `"PDF"` and `"Web"`.
- **iOS:**
  - `ActiveTheme.current` and `ActiveLabels.current` are both set in `Core.swift` from the root view (init and `.render`). `Render.swift` no longer writes `ActiveTheme.current`.
  - A replacement confirm is presented in the dismiss completion, or after the running transition, never while a dismissal is in flight.
- **Release shape:**
  - **PR-A** (`feat/lazylist-fill`): libs, 5 demo shells, barbershop showcase, versions, docs. Template untouched. Rebase-merge.
  - Then `release-libs`.
  - **PR-C** (`feat/lazylist-fill-cli`): template port, pin, CLI 0.55.0. Then `release-cli` with tag `v0.55.0`.
  - **Ask the user before each merge, publish and tag.**
- **Demos:** `demos/{coffee,todo,barbershop,saldo}` and `demos/fullstack-todo/mobile`.
  - Android `<pkg>`: `dev/mobiler/{coffee,todo,barbershop}`, `rs/mobiler/saldo`, `dev/mobiler/mobile`.
  - iOS `Render.swift` is identical in barbershop/coffee/todo/fullstack-todo; saldo differs, so port by anchor.
  - `Core.swift` is ported by anchor in all 5.
  - Edit barbershop first.
- **Environment:**
  - Every cargo/trunk/mobiler build uses `CARGO_TARGET_DIR=<demo or app dir>/target`, `JAVA_HOME=~/jdk21` and `ANDROID_HOME=/home/zmilan/Android/Sdk`.
  - CLI: `/media/zmilan/data2/cargo-target/debug/mobiler` (after `cargo build -p mobiler`).
  - AVD: `mobiler_verify_p7` with `ANDROID_AVD_HOME=/media/zmilan/data2/android-avd`. **Never leave barbershop's Profile tab on Android.**
  - iOS compiles on macOS CI only.
- **Waiting on long work:** use `run_in_background` (you are notified when it exits) or `wait <pid>`. **Never poll with `pgrep -f …` or `until grep … file` loops.** They self-match and leave zombie shells. Before reporting, make sure no loop you started is still running.
- **Commits:** subject, blank line, trailer `Co-Authored-By: Claude Opus 5 (1M context) <noreply@anthropic.com>`. Never stage `docs/confirm-dialog-labels.md`, `docs/render-before-requests.md`, `docs/large-touch-targets.md` or `.superpowers/`.
- **Scratch dir:** `SCRATCH=/tmp/claude-1000/-home-zmilan-working-docker-rust-mobiler/0aff67bb-5734-4226-ad08-fa524bb65d17/scratchpad`

---

### Task 1: ABI + core — `LazyList.fill`, `with_fill`, new `ShellLabels` fields

**Files:**
- `mobiler-ui/src/lib.rs`: `LazyList` (~563); `ShellLabels` (~360) and its builder; round-trip tests.
- `mobiler-core/src/lib.rs`: `lazy_list`, `lazy_list_static`, `with_refresh`'s and `with_end_label`'s LazyList arms carry `fill`; new `with_fill`; tests.
- `mobiler-web/src/lib.rs`: add `fill: _` to the LazyList pattern so it compiles. Task 2 uses it.

**Interfaces:**
- Produces `pub fn with_fill(widget: Widget) -> Widget`.
- Produces `ShellLabels::{pdf_error, pdf_title, web_title}` fields and chaining setters.

- [ ] **Step 1: Branch.** `git switch main && git pull --ff-only && git switch -c feat/lazylist-fill`.
- [ ] **Step 2: Failing tests.**
  - `mobiler-ui`:
    - Add `fill: false` to the existing LazyList round-trip literals.
    - Add one with `fill: true`.
    - Extend the labelled-scaffold round-trip with `.pdf_error("Greška").pdf_title("Dokument").web_title("Stranica")`.
  - `mobiler-core`:

```rust
    #[test]
    fn with_fill_marks_a_lazy_list_and_keeps_its_other_fields() {
        assert!(matches!(lazy_list(vec![], false, true, Ev::Tap), Widget::LazyList { fill: false, .. }));
        assert!(matches!(lazy_list_static(vec![]), Widget::LazyList { fill: false, .. }));
        let l = with_fill(with_end_label(with_refresh(lazy_list(vec![text("a")], false, true, Ev::Tap), false, Ev::Open(1)), "End"));
        assert!(matches!(&l, Widget::LazyList { fill: true, on_refresh: Some(_), end_label: Some(e), on_load_more: Some(_), .. } if e == "End"));
        // Other orders keep fill too.
        let l2 = with_end_label(with_refresh(with_fill(lazy_list(vec![], false, true, Ev::Tap)), false, Ev::Tap), "E");
        assert!(matches!(l2, Widget::LazyList { fill: true, .. }));
        assert!(matches!(with_fill(text("x")), Widget::Text { .. }));
        assert_eq!(ShellLabels::new().pdf_error("x").pdf_error, Some("x".to_string()));
    }
```

- [ ] **Step 3:** Run `cargo test -p mobiler-ui -p mobiler-core`. Expect FAIL.
- [ ] **Step 4: Implement.**
  - Add `fill: bool` last in `LazyList`, with a doc comment: "Fill the rest of the screen instead of the fixed height, when this list is the scaffold body or a direct child of the body's column (the first one). Set with `with_fill`."
  - Add the three `ShellLabels` fields after `done`, each with a one-line doc comment:
    - `pdf_error`: "Android PdfView error text".
    - `pdf_title`: "Web PDF iframe title (accessibility)".
    - `web_title`: "Web WebView iframe title (accessibility)".
  - Add the three builder setters.
  - `with_fill`:

```rust
/// Let a paged list fill the rest of the screen: when it is the scaffold body (or a direct child
/// of the body's column — the first such list), the body stops scrolling as a page and the list
/// takes the remaining height, scrolling itself. Anywhere else it keeps its normal bounded height.
/// Combines with [`with_refresh`] and [`with_end_label`] in any order. No-op on other widgets.
#[must_use]
pub fn with_fill(widget: Widget) -> Widget {
    match widget {
        Widget::LazyList { children, on_load_more, loading, has_more, on_refresh, refreshing, end_label, .. } => {
            Widget::LazyList { children, on_load_more, loading, has_more, on_refresh, refreshing, end_label, fill: true }
        }
        other => other,
    }
}
```

  - `with_refresh` and `with_end_label` must carry `fill` through.
  - Fix every Rust compile error across `grep -rn "Widget::LazyList {" --include=*.rs mobiler-core mobiler-web demos | grep -v target`.
- [ ] **Step 5: Verify.**
  - `cargo test -p mobiler-ui -p mobiler-core && cargo clippy -p mobiler-core --all-targets -- -D warnings`
  - `(cd mobiler-web && cargo check --target wasm32-unknown-unknown)`
  - `cargo test` in each demo.
- [ ] **Step 6: Commit.** `feat(lazylist): with_fill — a list that fills the screen; ShellLabels for PDF/web titles`, with the trailer.

---

### Task 2: Web — fill layout + iframe titles

**Files:** `mobiler-web/src/lib.rs` (Scaffold arm ~1956-2040, LazyList arm ~1720, PdfView ~1394, WebView ~1403) and `mobiler-web/src/mobiler.css`.

- [ ] **Step 1: The fill rule.** Add a helper:

```rust
/// The scaffold body's fill list, per the shared rule: the body itself, or the first direct child of
/// the body's column, that is a `LazyList { fill: true }`. `None` ⇒ the body scrolls as a page.
fn body_fill_index(body: &Widget) -> Option<Option<usize>> {
    match body {
        Widget::LazyList { fill: true, .. } => Some(None),
        Widget::Column { children } => children.iter().position(|c| matches!(c, Widget::LazyList { fill: true, .. })).map(Some),
        _ => None,
    }
}
```

  The meaning of the return value:
  - `Some(None)`: the body itself is the fill list.
  - `Some(Some(i))`: child `i` of the body's column is the fill list.
  - `None`: not fill mode.
- [ ] **Step 2: The Scaffold arm.**
  - When `body_fill_index(body).is_some()`, add ` scaffold-fill` to the scaffold's class.
  - Render the body so that exactly the chosen fill list gets the class `lazylist-fill`. Pass a flag down: a `thread_local!` "fill target" pointer compared by address (`std::ptr::eq`) in the LazyList arm is the simplest option. If it's `Some(Some(i))`, the Column arm renders normally and the LazyList arm checks whether it is the marked widget.
  - A LazyList that is **not** the marked one must not get the class, even if `fill: true`.
  - Clear the marker after rendering the body.
- [ ] **Step 3: CSS.** Append:

```css
/* LazyList fill — the scaffold body stops page-scrolling and the chosen list takes the rest. */
.scaffold.scaffold-fill { height: 100dvh; min-height: 0; }
.scaffold-fill > .scaffold-body { display: flex; flex-direction: column; min-height: 0; overflow: hidden; }
.scaffold-fill > .scaffold-body > .col { flex: 1; min-height: 0; display: flex; flex-direction: column; }
.scaffold-fill > .scaffold-body > .lazylist-fill,
.scaffold-fill > .scaffold-body > .col > .lazylist-fill { flex: 1; min-height: 0; max-height: none; }
```

  Before relying on it, check the real class names: the Column's class (`.col`?) and whether `.scaffold-body` is a direct child of `.scaffold`. Adapt the selectors to the DOM. Also check the tablet rail layout (`@media` block with `grid-template-rows: auto 1fr`): the `body` grid area needs `min-height: 0`, and `.scaffold.scaffold-fill` there needs `height: 100dvh`. Add rules if needed, so the list fills there too.
- [ ] **Step 4: iframe titles.** Replace `title="PDF"` with `title=shell_label(|l| l.pdf_title.clone(), "PDF")`, and `title="Web"` with `title=shell_label(|l| l.web_title.clone(), "Web")`. Compute both before `view!`.
- [ ] **Step 5: Verify.** Run `cd mobiler-web && cargo check --target wasm32-unknown-unknown && cargo clippy --target wasm32-unknown-unknown -- -D warnings`, then `CARGO_TARGET_DIR=$PWD/target RUSTUP_TOOLCHAIN=stable trunk build` in `demos/barbershop/web`. Remove `dist/` and `target/` afterwards.
- [ ] **Step 6: Commit.** `feat(web): LazyList fill layout; app titles for PDF/web frames`, with the trailer.

---

### Task 3: Android — fill layout + PDF error label (5 demos)

**Files:** `MainActivity.kt` in all 5 demos.

- [ ] **Step 1:** Add `val LocalFillList = compositionLocalOf { false }` (file-private) and a helper `private fun bodyFillIndex(body: Widget): Int?`, which follows the same rule as the web helper:
  - `-1` means the body itself is the fill list.
  - `i ≥ 0` means child `i` of the body's column is the fill list.
  - `null` means not fill mode.
- [ ] **Step 2: The Scaffold body.** Anchor: the `val column: @Composable BoxScope.() -> Unit = { Column(...) { … Render(screen.body, send) } }` block and the two `Box(... verticalScroll ...)` usages right after it.
  - When `bodyFillIndex(screen.body) != null`:
    - The `Column` uses `.fillMaxSize()` in place of only `fillMaxWidth()`. Keep the width cap, centering and padding.
    - Its content renders the body's children itself when the body is a Column, calling `Render(child, send)` for each child. For the fill child it renders `Box(Modifier.weight(1f).fillMaxWidth()) { CompositionLocalProvider(LocalFillList provides true) { Render(child, send) } }`. If the body itself is the fill list (`-1`), that Box wraps `Render(screen.body, send)`.
    - The two wrapping `Box`es drop `.verticalScroll(...)`.
  - When it is null, keep the code **exactly** as today.
  - Keep `screen.refreshing`'s `LinearProgressIndicator` first in the Column either way.
- [ ] **Step 3: The LazyList branch.** Read `val fillParent = LocalFillList.current && widget.fill`. Where the code has `Modifier.fillMaxWidth().heightIn(max = 480.dp)` (twice: the LazyColumn and the PullToRefreshBox), use `if (fillParent) Modifier.fillMaxSize() else <the exact original>`. Then provide `LocalFillList provides false` around the list's children, so a nested list doesn't inherit the flag.
- [ ] **Step 4: The PDF error.** Change `error != null -> Text("PDF: $error")` to `error != null -> Text(ActiveLabels.current?.pdfError?.takeIf { it.isNotEmpty() } ?: "PDF: $error")`. Check the generated Kotlin property name.
- [ ] **Step 5:** Port all of the above to the 4 other demos by anchor. Compile barbershop and todo with `mobiler dev --no-install` (`CARGO_TARGET_DIR=$PWD/target`, JDK, SDK; wait with `run_in_background`). Clean up afterwards.
- [ ] **Step 6: Commit.** `feat(android): LazyList fill layout; app label for the PDF error`, with the trailer.

---

### Task 4: iOS — fill layout, theme from the root, confirm present-after-dismiss (5 demos)

**Files:** `Render.swift` (barbershop first, then `cp` to coffee/todo/fullstack-todo; saldo by anchor) and `Core.swift` (all 5, by anchor).

- [ ] **Step 1: `Render.swift`, fill.**
  - Add an environment key `FillListKey` with default `false`, and `EnvironmentValues.fillList`.
  - Add a helper `bodyFillIndex(_ body: Widget) -> Int?` with the same rule and encoding as Android.
  - In `ScaffoldView`'s body, where it has `ScrollView { VStack(alignment: .leading, spacing: 6) { … render(self.content, send) } … }`, branch on `bodyFillIndex(content)`:
    - **nil:** exactly today's code.
    - **Otherwise:** the same modifiers chain, but no `ScrollView`: a `VStack(alignment: .leading, spacing: 6)` that renders the Column's children one by one. The fill child renders as `render(child, send).environment(\.fillList, true).frame(maxHeight: .infinity)`, or the whole body that way for `-1`. The VStack then gets `.frame(maxWidth: …, maxHeight: .infinity, alignment: .top)`. Keep `.refreshableIf`, `.id(route)`, `.transition` and the overlay.
  - `LazyListView`:
    - Add `@Environment(\.fillList) private var fillList` and use `.frame(height: fillList && fill ? nil : 420)`.
    - It must be pixel-identical when not filling. If the code has `.frame(height: 420)`, replace it with a conditional `Group`/modifier that applies exactly `.frame(height: 420)` when not filling and `.frame(maxHeight: .infinity)` when filling.
    - Pass `fill` in from the `.lazyList(...)` case, whose binding count is now **8**.
    - Reset `.environment(\.fillList, false)` for the list's children.
- [ ] **Step 2: `Render.swift`, theme.** Remove `ActiveTheme.current = theme` from the `.scaffold` case. `ScaffoldView` keeps getting `theme:`.
- [ ] **Step 3: Copy and verify.** First confirm the 4 identical files matched at HEAD. Then `cp` barbershop's file over them and verify with md5. Port Steps 1–2 to saldo by anchor.
- [ ] **Step 4: `Core.swift`, theme from the root.** Where `ActiveLabels.current` is set from the root view (init and `case .render`), also set `ActiveTheme.current`, using the same 13-value `.scaffold` pattern binding `theme` at position 6. Set it to `nil` when the root isn't a scaffold.
- [ ] **Step 5: `Core.swift` `DialogPlugin`, present after dismiss.** Restructure so the new alert is presented only once any outgoing alert has finished dismissing:
  - Inside the `withCheckedContinuation` body, after building the alert and `done`, and after setting `openConfirm = (alert, done)`, call a helper `presentWhenReady(alert, from: presenter, afterDismissing: prevToDismiss)`:
    - When `prevToDismiss` is non-nil (we dismiss it): `prevPresenter.dismiss(animated: false) { presenter.present(alert, animated: true) }`.
    - When the top controller is an alert that `isBeingDismissed`: `if let tc = that.transitionCoordinator { tc.animate(alongsideTransition: nil) { _ in presenter.present(alert, animated: true) } } else { DispatchQueue.main.async { presenter.present(alert, animated: true) } }`.
    - Otherwise: present immediately.
  - Keep:
    - answering the previous confirm `ok:false` exactly once;
    - `openConfirm` compared by `ObjectIdentifier`;
    - `nonisolated(unsafe)` on the static;
    - no strong capture of `alert` in `done`.
  - The helper is `@MainActor`, and its closures touch only UIKit objects and `presenter`.
  - All 5 demos' DialogPlugin blocks must be byte-identical.
- [ ] **Step 6: Port and check.**
  - md5: the 4 identical `Render.swift` files, and the DialogPlugin block ×5.
  - `grep -c "ActiveTheme.current = " Render.swift` = 0 in all 5.
  - `grep -c "ActiveTheme.current" Core.swift` ≥ 2 in all 5.
  - Read the Swift twice for compile errors: pattern arity, environment key syntax, optional chaining, main-actor isolation of the closures (UIKit completion blocks are main-thread), and `nonisolated(unsafe)` reuse.
- [ ] **Step 7: Commit.** `feat(ios): LazyList fill layout; theme from the root; confirm presented after dismissal`, with the trailer.

---

### Task 5: Barbershop — Feed tab + labels + tests

**Files:** `demos/barbershop/app-core/src/lib.rs`.

- [ ] **Step 1: Failing tests.**

```rust
    #[test]
    fn feed_tab_body_is_a_column_with_a_fill_list() {
        let (app, mut model) = app();
        model.tab = Tab::Feed;
        match app.view(&model) {
            Widget::Scaffold { body, labels: Some(l), .. } => {
                let Widget::Column { children } = *body else { panic!("feed body should be a column") };
                assert_eq!(children.iter().filter(|c| matches!(c, Widget::LazyList { fill: true, .. })).count(), 1);
                assert!(l.pdf_error.is_some() && l.pdf_title.is_some() && l.web_title.is_some());
            }
            other => panic!("expected a scaffold, got {other:?}"),
        }
    }
```

  Also update any test that counts tabs or asserts that the Profile body contains the feed.
- [ ] **Step 2:** Run `cargo test`. Expect FAIL.
- [ ] **Step 3: Implement.**
  - Add `Tab::Feed` and a tab entry labelled "Feed", after Bookings and before Profile, with an existing `Icon`. Check the `Icon` enum for something list-like, and use the closest one.
  - `feed_screen(model)` returns `column(vec![caption("Pull to refresh · scroll for more"), with_fill(<the existing feed list expression>)])`. Move the list out of `feed_card` and delete `feed_card` from the Profile screen.
  - Add `.pdf_error("Couldn't open the document")`, `.pdf_title("Shop price list")` and `.web_title("Shop website")` to the `ShellLabels` chain.
  - Wire the new tab in the `match model.tab` and the tab-selection message handling.
- [ ] **Step 4:**
  - `cargo test && cargo clippy --all-targets -- -D warnings` must be clean. Task 6 of Release 3 made barbershop clean.
  - Web build with `CARGO_TARGET_DIR=$PWD/target RUSTUP_TOOLCHAIN=stable trunk build`, then remove `dist/` and `target/`.
- [ ] **Step 5: Commit.** `feat(barbershop): Feed tab with a list that fills the screen; PDF/web labels`, with the trailer.

---

### Task 6: Sweep (no commit unless fixes)

- [ ] Run `cargo test` in all demos, `clippy -D warnings` in coffee/todo/fullstack-sqlx/barbershop, the mobiler-web wasm check, and `trunk build` for all 6 web demos. Each build gets its own `CARGO_TARGET_DIR`.
- [ ] Compile Android saldo and coffee.
- [ ] Audit: confirm that no `heightIn(max = 480.dp)`, `.frame(height: 420)` or `60vh` cap was changed for non-fill lists. Diff each against main and read the hunks.
- [ ] Commit fixes only if something fails.

### Task 7: Runtime acceptance (no commit)

- [ ] **Web (barbershop, CDP):**
  1. Feed tab: `document.scrollingElement.scrollHeight <= clientHeight + 1`, so the page doesn't scroll. The `.lazylist-fill` bottom is within 2px of the `.tabbar` top, or of the viewport bottom on the wide layout. Check at 412×915 **and** at 1200×900 for the rail layout.
  2. Scroll the list to the end and click "Show more" repeatedly. Rows load, and "You're all caught up" appears.
  3. Profile: the PDF iframe `title` is "Shop price list", and the web iframe title is "Shop website".
  4. Home and Bookings: the page still scrolls as before (`scrollHeight > clientHeight`) and no element has `lazylist-fill`.
  5. Coffee: unchanged. The confirm shows OK / Cancel.
  6. No JS exceptions.
- [ ] **Android (barbershop), never leaving Profile:**
  1. From Home, go to the Feed tab. The list's bounds bottom is just above the tab bar (uiautomator). It scrolls, and load-more adds rows.
  2. Bookings still scrolls as a page.
- [ ] Put evidence in `$SCRATCH/r4-evidence/`. Tear everything down. Leave no wait loops behind.

### Task 8: Versions, docs, NOTES, commit; final review; ship PR-A (ask first)

- [ ] **Versions** per Global Constraints. Refresh every lockfile (`grep -rl 'name = "mobiler-core"' --include=Cargo.lock . | grep -v /target/`) and verify none still references ui 0.26.0 / core 0.37.0 / web 0.37.0.
- [ ] **`capabilities.json`:** mention `with_fill` wherever LazyList builders are listed, if they are. Run `gen-readme` and then `--check`.
- [ ] **README:** add a one-line `with_fill` mention next to `with_end_label` / `with_labels`.
- [ ] **NOTES.md** (gitignored) gets a Release 4 subsection covering:
  - why the rule counts only direct children and the first list;
  - how each shell implements fill;
  - the iOS present-after-dismiss and root-only theme changes;
  - the Task 7 results;
  - what remains.
- [ ] **Commit** the versions, locks, docs, spec and this plan: `chore(release): mobiler-ui 0.27.0, mobiler-core 0.38.0, mobiler-web 0.38.0 + docs`, with the trailer.
- [ ] Run the final whole-branch review, then **ask the user**, then ship-pr with rebase-merge.

### Task 9: Publish libs (ask first)

- [ ] Run release-libs in order: ui 0.27.0 → core 0.38.0 → web 0.38.0. Dry-run first and confirm each is indexed.

### Task 10: Template port + CLI 0.55.0 (PR-C)

- [ ] Branch `feat/lazylist-fill-cli`. Port the merged barbershop shell hunks into the template by anchor, keeping its tokens; `cp` `Render.swift` if the template matched barbershop's pre-change file.
- [ ] Pin `"0.38"`, set CLI 0.55.0, run `cargo update -p mobiler` and `cargo test -p mobiler`.
- [ ] Scaffold smoke: resolves core 0.38.0 / ui 0.27.0, and the APK contains `libshared.so`.
- [ ] Commit, ship-pr (ask the user before merging), squash.

### Task 11: Release CLI + post-release (ask first)

- [ ] Run release-cli: packaging pre-check, then tag `v0.55.0`.
- [ ] Post-release:
  - fresh install and upgrade from 0.54.0;
  - `start.md` and memory;
  - screenshots (the barbershop README may want a Feed-tab shot);
  - clean caches;
  - check that no zombie wait loops are left under the session.
