# Pinned Bottom Bar Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** `with_bottom_bar(scaffold, children)` pins a row of actions above the tabs and the system
navigation. The body ends above it, the FAB and snackbar float above it, and a sheet covers it.
Scaffolds without a bar are unchanged.

**Architecture:**
- ABI: `Scaffold.bottom_bar: Option<Vec<Widget>>`, passed through by every scaffold-rebuilding builder.
- Web: a sticky `.bottombar` between the body and the tab bar, with a grid area on wide layouts.
- Android: `Column { PinnedBar; NavigationBar }` in the M3 `bottomBar` slot.
- iOS: an HStack between the body and the tabs in `ScaffoldView`.
- Other demo shells: the Swift pattern arity only.

**Tech Stack:** Rust (ui/core/web), Compose M3, SwiftUI.

**Spec:** `docs/superpowers/specs/2026-09-29-pinned-bottom-bar-design.md`

## Global Constraints

- **`None` or an empty `Vec`:** today's layout exactly (web markup unchanged, no Android/iOS nodes
  added).
- **Bar look:** equal-width children, 12 gap, padding 16/12, `surface_bar` background, a
  hairline top divider.
- **Order:** body, bar, tabs, system inset. The bar rises with the keyboard.
- **Swift:** the `.scaffold(...)` pattern becomes 15 positions in every demo, including
  `fullstack-todo/mobile`; find them with `grep -rn 'case .scaffold(' demos`. Kotlin reads by name,
  `widget.bottomBar`.
- **Repo rules:**
  - don't touch the templates
  - don't publish
  - Swift is gated in CI
  - per-demo `CARGO_TARGET_DIR`
  - commit bodies via `git commit -F -` with a quoted heredoc
  - **the AVD is shared:** check `mCurrentFocus` before every action

## Review Focus

1. **Pass-through.** Every core builder that rebuilds `Widget::Scaffold` keeps `bottom_bar`. Pinned
   by the Task 1 test over all of them; the compiler catches missing fields, but not ones silently
   set to `None`.
2. **The FAB above the bar on web** is measured, not hard-coded. Pinned by the Task 4 CDP check.
3. **Wide web layout** (the ≥768 rail): the bar belongs to the body column. Pinned by a Task 4 CDP
   check at 1024px.
4. **Keyboard:** Android `imePadding` without double-counting the insets. Reviewer judgment plus a
   Task 4 AVD check if a text-field screen with a bar exists (else reviewer).

---

### Task 1: ABI + builder + pass-through

- [ ] **Test** (mobiler-core):

```rust
#[test]
fn with_bottom_bar_sets_and_every_scaffold_builder_keeps_it() {
    let bar = vec![text("A"), text("B")];
    let s = with_bottom_bar(scaffold("T", false, vec![], text("x")), bar.clone());
    assert!(matches!(&s, Widget::Scaffold { bottom_bar: Some(b), .. } if b.len() == 2));
    assert!(matches!(with_bottom_bar(text("x"), bar.clone()), Widget::Text { .. }));
    assert!(matches!(scaffold("T", false, vec![], text("x")), Widget::Scaffold { bottom_bar: None, .. }));
    // Every other scaffold builder keeps it.
    let keep = |w: Widget| matches!(w, Widget::Scaffold { bottom_bar: Some(_), .. });
    assert!(keep(with_fab(s.clone(), Icon::Add, Ev::Tap)));
    assert!(keep(with_extended_fab(s.clone(), Icon::Add, "x", Ev::Tap)));
    assert!(keep(with_sheet(s.clone(), "t", text("s"), Ev::Tap)));
    assert!(keep(with_refresh(s.clone(), false, Ev::Tap)));
    assert!(keep(with_theme(s.clone(), Theme::default())));
    assert!(keep(with_labels(s.clone(), ShellLabels::new())));
    assert!(keep(with_appearance(s.clone(), Appearance::System)));
}
```

  Adjust each call to its real signature. `grep -n 'Widget::Scaffold {' mobiler-core/src/lib.rs` for
  any builder not listed, and add it. mobiler-ui: round-trip a scaffold with a bar.
- [ ] **RED**, then implement:
  - The field, with doc + BREAKING note.
  - `scaffold(..)` sets `bottom_bar: None`.
  - Every rebuilding builder passes `bottom_bar` through.
  - `#[must_use] pub fn with_bottom_bar(widget, children: Vec<Widget>) -> Widget`.
  - The mobiler-web scaffold arm destructures `bottom_bar: _` for now.
- [ ] **GREEN.** Run the tests, clippy, the wasm check, and `cargo check` on every demo. Commit:
  `feat(ui)!: Scaffold.bottom_bar + with_bottom_bar`

### Task 2: Web

- [ ] In the scaffold arm, with `Some(v)` and `v` non-empty:
  - `let bar = view! { <div class="bottombar">{cells}</div> }`, where each cell is
    `<div class="bottombar-cell">{render(child)}</div>`.
  - Place `{bar}` after the body div, before `{fab_btn}`.
  - Add `has-bottombar` to the scaffold's class.
- [ ] **CSS:**

```css
/* Pinned bottom bar (with_bottom_bar): the screen's main actions, above the tabs. */
.bottombar { position: sticky; bottom: 0; z-index: 4; display: flex; gap: 12px; padding: 12px 16px;
  background: var(--surface-bar, var(--bg)); border-top: 1px solid var(--line); }
.scaffold:has(.tabbar) > .bottombar { bottom: var(--tabbar-h, 57px); }
.bottombar-cell { flex: 1; min-width: 0; }
.bottombar-cell > .btn { width: 100%; }
.has-bottombar .fab { bottom: calc(84px + var(--bottombar-h, 76px)); }
```

  In the ≥768 media block, add `"rail bar"` to `grid-template-areas`
  (`"rail topbar" "rail body" "rail bar"`), set `grid-template-rows: auto 1fr auto` only when there's
  a bar (`.scaffold:has(.tabbar):has(.bottombar)`), and add `.scaffold:has(.tabbar) > .bottombar { grid-area: bar; bottom: 0; }`.
- [ ] **Measured sizes.** After render, set `--bottombar-h` (the bar's height) and `--tabbar-h` (the
  tab bar's height) as inline style properties on the scaffold element. Use a post-render effect or
  `request_animation_frame`, following how the shell already does post-render DOM work (grep
  `request_animation_frame` / `Effect::new`). Keep the fallbacks in the CSS so it works before the
  first measurement.
- [ ] **Snackbar:** add `top_of(".bottombar")` to its `tops` list. The bar is bottom-anchored, so it
  counts.
- [ ] Run the tests and wasm clippy. Commit: `feat(web): pinned bottom bar`

### Task 3: Native shells

- [ ] **Barbershop Android:** the `bottomBar = { … }` slot becomes

```kotlin
bottomBar = {
    Column {
        val bar = widget.bottomBar.orEmpty()
        if (bar.isNotEmpty()) {
            // with_bottom_bar: the screen's main actions, pinned above the tabs; rises with the
            // keyboard when there are no tabs (the tabs themselves stay behind it, as today).
            Surface(color = LocalPalette.current?.surfaceBar?.color() ?: MaterialTheme.colorScheme.surfaceContainer) {
                Column(if (!wide && widget.tabs.isNotEmpty()) Modifier else Modifier.navigationBarsPadding().imePadding()) {
                    HorizontalDivider(color = LocalPalette.current?.outlineVariant?.color() ?: MaterialTheme.colorScheme.outlineVariant)
                    Row(Modifier.fillMaxWidth().padding(horizontal = 16.dp, vertical = 12.dp), horizontalArrangement = Arrangement.spacedBy(12.dp)) {
                        bar.forEach { Box(Modifier.weight(1f)) { Render(it, send) } }
                    }
                }
            }
        }
        <today's NavigationBar block, unchanged>
    }
},
```

  Check that `Render` inside the bar has `send` in scope, and that buttons can fill a weighted Box.
  If the `MobilerButton` isn't full-width by default, wrap it in `Modifier.fillMaxWidth()` via the
  Box. Import `navigationBarsPadding`, `imePadding` and `HorizontalDivider` if missing.
- [ ] **Barbershop iOS**, in `ScaffoldView` after the body group (before `if showBottomTabs`), with a
  `bottomBar: [SharedTypes.Widget]?` property threaded from the `.scaffold` case (15th position):

```swift
if let bar = bottomBar, !bar.isEmpty {
    // with_bottom_bar: the screen's main actions, pinned above the tabs (the body ends above it;
    // the keyboard safe area lifts it with the keyboard).
    PaletteDivider()
    HStack(spacing: 12) {
        ForEach(Array(bar.enumerated()), id: \.offset) { _, w in render(w, send).frame(maxWidth: .infinity) }
    }
    .padding(.horizontal, 16).padding(.vertical, 12)
    .background(role(pal?.surfaceBar, else: Color(.systemBackground)))
}
```

- [ ] **Other iOS shells:** add `_` as the 15th position of `.scaffold(...)`.
- [ ] Build all five APKs. Commit: `feat(android,ios): pinned bottom bar (barbershop); scaffold arity in every demo`

### Task 4: Barbershop demo + acceptance

- [ ] **Test first:** with `model.tab = Tab::Services` and `selected_service = Some(0)`, `app.view`
  is a scaffold whose `bottom_bar` holds 2 buttons ("Book", "Call"). With no selection, it's `None`.
  RED.
- [ ] **Implement:** after the refresh/sheet wiring, `if model.tab == Tab::Services { if let Some(i) = model.selected_service { root = with_bottom_bar(root, vec![button("Book", ButtonStyle::Filled, Msg::OpenService(i)), button("Call", ButtonStyle::Tonal, Msg::CallShop)]); } }`.
  Match `Msg::OpenService`'s argument type. GREEN, then clippy.
- [ ] **Web CDP** (412×915, then 1024×768):
  - open Services and select a service
  - `.bottombar` exists; its bottom equals the `.tabbar` top (phone)
  - scroll to the end: the last body child's bottom ≤ the bar's top
  - the `.fab` bottom ≤ the bar top
  - the snackbar (swipe-cancel isn't on this screen, so trigger `CallShop`, or skip if there's no
    snackbar source here) sits above the bar
  - open the sheet via "Book": `.sheet` covers the bar (its rect overlaps and it's on top)
  - wide: the bar is in the body column (its left ≥ the rail's right)
  - no exceptions
  - coffee/todo pixel-identical to `main`
- [ ] **Android AVD** (foreground check before every action):
  - open the service detail; the bar's bounds bottom == the NavigationBar top
  - scroll; the bar hasn't moved
  - the FAB is above the bar
  - take a screenshot
- [ ] **Commit:** `feat(barbershop): pinned Book / Call bar on service detail`

### Task 5: Docs, review, PR

- [ ] Add a NOTES.md section, then run a fresh whole-branch review (opus) and a fix pass.
- [ ] Ship with ship-pr (25 checks) and squash-merge.
- [ ] Update the memory and `start.md`.
