# Extended FAB Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** `with_extended_fab(scaffold, icon, label, on_press)` renders an icon + label FAB on every
shell. `with_fab` and every existing FAB stay identical.

**Architecture:**
- ABI: `Fab.label: Option<String>`, appended last.
- A new core builder.
- Each shell branches on `label`: web `.fab-extended`, Android `ExtendedFloatingActionButton`, and an
  iOS `HStack` in the existing FAB button.

**Tech Stack:** Rust (mobiler-ui/core/web), Compose M3, SwiftUI.

**Spec:** `docs/superpowers/specs/2026-09-28-extended-fab-design.md`

## Global Constraints

- **`label: None` renders exactly as today.** Web markup and CSS for `.fab` are untouched; Android and
  iOS take the existing branch unchanged.
- **Extended look:**
  - height 56, horizontal padding 16 (web 16 right / 14 left around the glyph), gap 12
  - label font 16 / semibold
  - same colours (`fab` / `on_fab`), shape (`Theme.shapes.fab` else the shell default), anchor and
    shadow
- **Accessibility:** the label is the accessible name; the icon is decorative.
- **Repo rules:**
  - don't touch the templates or other demo shells (beyond what typegen regenerates)
  - don't publish
  - Swift is gated in CI
  - per-demo `CARGO_TARGET_DIR`
  - commit bodies via `git commit -F -` with a quoted heredoc
  - **Android AVD is shared:** check `adb shell dumpsys window | grep mCurrentFocus` shows
    `dev.mobiler.barbershop` before every tap burst

## Review Focus

1. **Round FAB unchanged.** Pinned by Task 4 coffee/todo pixel identity and the untouched `.fab`
   rule.
2. **Snackbar still clears it.** Web measures `.fab`, which the extended FAB still carries; iOS pads
   by the 56 height; Android M3 stacks it. Pinned by the Task 4 CDP check.
3. **A long label.** It must not overflow the screen: web `max-width: calc(100vw - 36px)` with
   ellipsis, Android M3 wraps, iOS `lineLimit(1)`. Judged by the reviewer.
4. **Density::Large.** The FAB height stays 56 (the round FAB doesn't grow today either).

---

### Task 1: ABI + builder

**Files:** `mobiler-ui/src/lib.rs`, `mobiler-core/src/lib.rs`.

- [ ] **Step 1: Tests.**
  - mobiler-ui: `round_trips(&Fab { icon: Icon::Add, on_press: …, label: Some("Novi termin".into()) })`.
    Use the existing round-trip helper and an `ActionToken` the way other tests build one.
  - mobiler-core:

```rust
#[test]
fn with_extended_fab_sets_the_label_and_with_fab_does_not() {
    let s = with_extended_fab(scaffold("T", false, vec![], text("x")), Icon::Add, "Novi termin", Ev::Tap);
    assert!(matches!(&s, Widget::Scaffold { fab: Some(Fab { label: Some(l), .. }), .. } if l == "Novi termin"));
    let r = with_fab(scaffold("T", false, vec![], text("x")), Icon::Add, Ev::Tap);
    assert!(matches!(&r, Widget::Scaffold { fab: Some(Fab { label: None, .. }), .. }));
}
```

  Adjust the `scaffold(...)` arguments to its real signature.
- [ ] **Step 2: RED.**
- [ ] **Step 3: Implement.**
  - `Fab` gains `/// Extended FAB: the label shown next to the icon (also its accessible name). None = the round FAB.` `pub label: Option<String>,`,
    with a `// BREAKING (Moj Termin design release): Fab literals need label` note.
  - `with_fab` builds `label: None`.
  - Add `pub fn with_extended_fab<E: Serialize>(widget, icon, label: impl Into<String>, on_press: E) -> Widget`
    with the same shape as `with_fab`. Factor a private `set_fab(widget, Fab)` to avoid duplicating the
    Scaffold destructure. Re-export if core re-exports builders by name.
- [ ] **Step 4: GREEN.** Run the ui and core tests, clippy, the wasm check, and `cargo check` on every
  demo's shared/app-core crate.
- [ ] **Step 5: Commit:** `feat(ui)!: Fab.label — an extended FAB (icon + label)`

### Task 2: Web

**Files:** `mobiler-web/src/lib.rs` (the FAB render), `mobiler-web/src/mobiler.css`.

- [ ] **Step 1: Implement.** In the scaffold render, `fab_btn`: when `f.label` is `Some(l)`, render
  `<button class="fab fab-extended" aria-label=l><span class="fab-icon" aria-hidden="true">{icon_glyph}</span><span class="fab-label">{l}</span></button>`.
  Otherwise use today's exact markup. CSS:

```css
/* Extended FAB (with_extended_fab): icon + label in the same anchored button. */
.fab-extended { width: auto; max-width: calc(100vw - 36px); padding: 0 16px 0 14px; gap: 12px;
  font-size: 16px; font-weight: 600; font-family: var(--font); }
.fab-extended .fab-icon { font-size: 24px; line-height: 1; }
.fab-extended .fab-label { white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
```

- [ ] **Step 2:** Run the tests and wasm clippy. Commit: `feat(web): extended FAB`

### Task 3: Android + iOS (barbershop)

- [ ] **Android** `MainActivity.kt` `floatingActionButton = { … }`: when `fab.label` is non-null, use

```kotlin
ExtendedFloatingActionButton(
    onClick = { send(Action.Fired(fab.onPress)) },
    icon = { Icon(iconFor(fab.icon), contentDescription = null) },
    text = { Text(label) },
    shape = shapeOf { it.fab } ?: FloatingActionButtonDefaults.extendedFabShape,
)
```

  Otherwise keep today's `FloatingActionButton` call. Copy any `containerColor` / `contentColor` the
  round FAB passes; today it relies on the theme's `primaryContainer`, which the extended one reads
  too. Import `ExtendedFloatingActionButton`. Build the APK.
- [ ] **iOS** `Render.swift`, the FAB overlay button content:
  - `if let label = fab.label`:
    `HStack(spacing: 12) { Image(systemName: sfSymbol(fab.icon)).font(.title2); Text(label).font(.body.weight(.semibold)).lineLimit(1) }.padding(.horizontal, 16).frame(height: 56)`
  - otherwise today's `Image(...).font(.title2).frame(width: 56, height: 56)`
  - then the same `.background`, `.foregroundColor`, `.clipShape` and `.shadow` chain
  - add `.accessibilityLabel(fab.label ?? "")` only when the label is set
  - it's simplest to extract the content into a `@ViewBuilder` func and apply the shared modifiers
    once
- [ ] **Commit:** `feat(android,ios): extended FAB (barbershop)`

### Task 4: Barbershop + acceptance

- [ ] **Step 1: Test first:** barbershop app-core, `app.view(&model)` on Home matches
  `Widget::Scaffold { fab: Some(Fab { label: Some(l), .. }), .. }` with `l == "Book"`. RED.
- [ ] **Step 2:** Replace `with_fab(..., Icon::Calendar, Msg::Book)` with
  `with_extended_fab(..., Icon::Calendar, "Book", Msg::Book)`. GREEN, then clippy.
- [ ] **Step 3: Web CDP** (barbershop, 412×915):
  - `.fab-extended` text is "Book", `aria-label` is "Book", height 56, bottom < `.tabbar` top
  - cancel a booking on Bookings: the `.snackbar` bottom is 12px above the FAB top
  - no exceptions
  - coffee/todo pixel-identical to `main` (a `main` worktree)
- [ ] **Step 4: Android AVD** (check the foreground app first): the "Book" text node lies inside the
  FAB's bounds, and the FAB's bottom is above the NavigationBar's top. Take a screenshot.
- [ ] **Step 5: Commit:** `feat(barbershop): extended "Book" FAB`

### Task 5: Docs, review, PR

- [ ] Add a NOTES.md section, then run a fresh whole-branch review (opus) and a fix pass.
- [ ] Ship with ship-pr (25 checks) and squash-merge.
- [ ] Update the memory and `start.md`.
