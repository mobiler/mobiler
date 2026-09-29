# Step Indicator Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** `steps(3, 2)` draws three equal segments with two filled, spoken as "Step 2 of 3" (or the app's
`ShellLabels.step_of` template), never as a percentage.

**Architecture:**
- ABI: `Widget::Steps { total, current }` and `ShellLabels.step_of`.
- Core: the `steps` builder and a `step_text` helper.
- Web: a flex row of spans with progressbar ARIA.
- Barbershop native: a weighted Row / HStack with one accessibility label.
- Every other demo shell: a simple version of the same.

**Tech Stack:** Rust (ui/core/web), Compose, SwiftUI.

**Spec:** `docs/superpowers/specs/2026-09-29-step-progress-design.md`

## Global Constraints

- **Look:** segments 4 tall, gap 4, fully rounded; filled = palette `primary` else brand; the rest =
  `surface_muted` else the shell's muted surface.
- **`total == 0`** renders nothing; `current` is clamped to `total` in the builder and the shells.
- **Spoken text:** the `step_of` template with `{current}` / `{total}` replaced. An empty or missing
  template falls back to `"Step {current} of {total}"`. One element, and no progress range info on
  Android.
- **Generated names:** Kotlin `Widget.Steps` (`total`, `current` as `UByte`) and `ShellLabels.stepOf`;
  Swift `.steps(let total, let current)` and `stepOf`. Check the generated files.
- **All shells:** find them with `grep -rln 'case .rating(\|is Widget.Rating' demos mobiler/templates`
  and update every demo, including `demos/fullstack-todo/mobile`; not the templates.
- **Repo rules:**
  - don't publish
  - Swift is gated in CI
  - per-demo `CARGO_TARGET_DIR`
  - commit bodies via `git commit -F -` with a quoted heredoc
  - **the AVD is shared:** check `mCurrentFocus` before every action

## Review Focus

1. **No percentage announced** on any shell. Web gets `aria-valuetext`. Android must not use
   `ProgressIndicator` or `progressBarRangeInfo`. iOS gets `children: .ignore`.
2. **Hand-built `Widget::Steps { total: 3, current: 9 }`** is clamped by the shells.
3. **The language template** is read from the root scaffold's labels on each shell.
4. **Width:** the segments fill the parent width, with equal weights.

---

### Task 1: ABI + core

- [ ] **Tests** (mobiler-core):

```rust
#[test]
fn steps_builder_clamps_and_step_text_fills_the_template() {
    assert!(matches!(steps(3, 2), Widget::Steps { total: 3, current: 2 }));
    assert!(matches!(steps(3, 9), Widget::Steps { total: 3, current: 3 }));
    assert_eq!(step_text(None, 2, 3), "Step 2 of 3");
    assert_eq!(step_text(Some(""), 2, 3), "Step 2 of 3");
    assert_eq!(step_text(Some("Korak {current} od {total}"), 2, 3), "Korak 2 od 3");
    assert_eq!(ShellLabels::new().step_of("Korak {current} od {total}").step_of.as_deref(), Some("Korak {current} od {total}"));
}
```

  mobiler-ui: round-trip `Widget::Steps { total: 3, current: 2 }`.
- [ ] **RED**, then implement:
  - `Steps` (with a doc comment) appended to `Widget`.
  - `ShellLabels.step_of: Option<String>` appended, doc "the step indicator's spoken text: `{current}`
    / `{total}` placeholders", plus the `.step_of()` builder, like the others.
  - In core: `#[must_use] pub fn steps(total: u8, current: u8) -> Widget` (clamping), and
    `pub fn step_text(template: Option<&str>, current: u8, total: u8) -> String`.
  - Export both.
  - mobiler-web: a placeholder `Widget::Steps { .. } => view! { <span></span> }.into_any()` arm so it
    compiles (Task 2 renders it).
- [ ] **GREEN.** Run the tests, clippy, the wasm check, and `cargo check` on every demo including
  `fullstack-todo/mobile`. Commit: `feat(ui): Widget::Steps + ShellLabels.step_of`

### Task 2: Web

- [ ] Render it:

```rust
Widget::Steps { total, current } => {
    let (total, current) = (*total, (*current).min(*total));
    if total == 0 { return view! { <span></span> }.into_any(); }
    let label = mobiler_core::step_text(shell_label_opt(|l| l.step_of.clone()).as_deref(), current, total);
    let segs = (0..total).map(|i| view! { <span class=if i < current { "step step-on" } else { "step" }></span> }).collect_view();
    view! { <div class="steps" role="progressbar" aria-valuemin="1" aria-valuemax=total.to_string()
                 aria-valuenow=current.to_string() aria-valuetext=label>{segs}</div> }.into_any()
}
```

  Use the existing `shell_label(...)` accessor pattern. If it only returns a `String` with a default,
  pass the template through it with default `""` and let `step_text` fall back.
- [ ] CSS:

```css
/* Step indicator (steps): equal segments, the first `current` filled. */
.steps { display: flex; gap: 4px; width: 100%; }
.step { flex: 1; height: 4px; border-radius: 2px; background: var(--surface-2); }
.step-on { background: var(--primary); }
```

- [ ] Run the tests and wasm clippy. Commit: `feat(web): step indicator`

### Task 3: Native shells

- [ ] **Barbershop Android**, in the `Render` `when`:

```kotlin
is Widget.Steps -> {
    val total = widget.total.toInt()
    val current = minOf(widget.current.toInt(), total)
    if (total > 0) {
        val tpl = ActiveLabels.current?.stepOf?.takeIf { it.isNotEmpty() } ?: "Step {current} of {total}"
        val spoken = tpl.replace("{current}", "$current").replace("{total}", "$total")
        val on = LocalPalette.current?.primary?.color() ?: MaterialTheme.colorScheme.primary
        val off = MaterialTheme.colorScheme.surfaceVariant
        // One element named by the step text; no progress range, so TalkBack never reads a percentage.
        Row(Modifier.fillMaxWidth().clearAndSetSemantics { contentDescription = spoken }, horizontalArrangement = Arrangement.spacedBy(4.dp)) {
            repeat(total) { i -> Box(Modifier.weight(1f).height(4.dp).clip(RoundedCornerShape(50)).background(if (i < current) on else off)) }
        }
    }
}
```

  Add the `clearAndSetSemantics` import if missing. The Row isn't clickable, so clearing semantics is
  safe.
- [ ] **Barbershop iOS**, in the render switch:

```swift
case .steps(let total, let current):
    let t = Int(total), c = min(Int(current), Int(total))
    if t == 0 { return AnyView(EmptyView()) }
    let tpl = (ActiveLabels.current?.stepOf).flatMap { $0.isEmpty ? nil : $0 } ?? "Step {current} of {total}"
    let spoken = tpl.replacingOccurrences(of: "{current}", with: "\(c)").replacingOccurrences(of: "{total}", with: "\(t)")
    return AnyView(HStack(spacing: 4) {
        ForEach(0..<t, id: \.self) { i in
            Capsule().fill(i < c ? role(pal?.primary, else: ActiveTheme.current?.brandColor ?? .accentColor)
                                 : role(pal?.surfaceMuted, else: Color(.systemFill))).frame(height: 4)
        }
    }.frame(maxWidth: .infinity).accessibilityElement(children: .ignore).accessibilityLabel(spoken))
```

  Check that `ActiveLabels` exists in each iOS demo; if a demo lacks it, use the English default there.
- [ ] **coffee / todo / saldo / fullstack-todo/mobile** (Android + iOS): the same code, using the
  palette accessors only where that shell has them, and `MaterialTheme` / `.accentColor` + `.systemFill`
  otherwise.
- [ ] **Build** all five APKs. Commit: `feat(android,ios): step indicator in every demo shell`

### Task 4: Barbershop + acceptance

- [ ] **Test first:** serialize `booking_sheet(...)` (use a real service from the model the way
  `app.view` builds it). It contains `"Steps":{"total":3,"current":2}`. RED.
- [ ] Add `caption("Step 2 of 3")` and `steps(3, 2)` at the top of `booking_sheet`. GREEN, then
  clippy.
- [ ] **Web CDP:**
  - open Services, then a card, then "Book this"
  - `.steps` has 3 `.step`, 2 `.step-on`, equal widths, and `aria-valuetext` "Step 2 of 3"
  - no exceptions
  - coffee/todo pixel-identical to `main`
- [ ] **Android AVD** (foreground check before every action): open the booking sheet; a node has the
  content-desc "Step 2 of 3"; take a screenshot.
- [ ] **Commit:** `feat(barbershop): step indicator in the booking sheet`

### Task 5: Docs, review, PR

- [ ] Add a NOTES.md section, then run a fresh whole-branch review (opus) and a fix pass.
- [ ] Ship with ship-pr (25 checks) and squash-merge.
- [ ] Update the memory and `start.md`.
