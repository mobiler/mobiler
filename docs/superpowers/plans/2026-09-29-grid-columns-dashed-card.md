# Grid Columns and Dashed Card Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** `with_columns(grid(..), n)` gives exactly n equal columns, and `CardStyle::Dashed` draws a
dashed-outline card. Existing grids and card styles are unchanged.

**Architecture:**
- ABI: `Grid.columns: Option<u8>` and `CardStyle::Dashed`, both appended.
- A core `with_columns` modifier.
- Web: an inline grid-template and `.card-dashed`.
- Barbershop Android: the column count, and a dashed outline drawn behind a `Box`.
- Barbershop iOS: `GridView` columns and a dashed stroke in `CardMod`.
- Other shells (coffee, todo, saldo, **fullstack-todo/mobile**): the enum case (drawn as outlined) and
  the iOS grid arity.

**Tech Stack:** Rust (ui/core/web), Compose, SwiftUI.

**Spec:** `docs/superpowers/specs/2026-09-29-grid-columns-dashed-card-design.md`

## Global Constraints

- **Unchanged when unset.** With `columns: None`, the markup and the column rule stay as today.
  Existing card styles are untouched.
- **Columns:** clamped to 1–4 (builder and shells), equal widths, gap 12, a short last row keeps its
  empty cells.
- **Dashed:** no fill, 1.5 stroke, dash 6 / gap 4, palette `outline` else the shell's outline
  colour, the card radius. Pressable like any card.
- **Generated names:** Kotlin `CardStyle.DASHED`, Swift `.dashed`. Check the generated files; typegen
  upper-cases without underscores.
- **All shells:** find them with `grep -rln 'CardStyle.BRAND\|case .brand:\|case .grid(' demos`. That
  includes `demos/fullstack-todo/mobile`.
- **Repo rules:**
  - don't touch the templates
  - don't publish
  - Swift is gated in CI
  - per-demo `CARGO_TARGET_DIR`
  - commit bodies via `git commit -F -` with a quoted heredoc
  - **the AVD is shared:** check `mCurrentFocus` before every action

## Review Focus

1. **`None` grids unchanged** on every shell. Pinned by Task 4's coffee/todo pixel diffs and the CDP
   check that existing grids compute 2 columns.
2. **Dashed + pressable.** The click and long-press still fire, and the ripple/press is clipped to the
   shape. Pinned by the Task 4 web click and the Android tap.
3. **Columns clamp at the shell** too, for an app that builds `Widget::Grid` by hand. Reviewer
   judgment.
4. **The dashed stroke isn't clipped in half** on Android: the stroke is drawn inside the clip, so
   inset it by half the stroke width. Reviewer judgment plus the screenshot.

---

### Task 1: ABI + builder

- [ ] **Test** (mobiler-core):

```rust
#[test]
fn with_columns_sets_clamps_and_ignores_other_widgets() {
    assert!(matches!(with_columns(grid(vec![]), 3), Widget::Grid { columns: Some(3), .. }));
    assert!(matches!(with_columns(grid(vec![]), 0), Widget::Grid { columns: Some(1), .. }));
    assert!(matches!(with_columns(grid(vec![]), 9), Widget::Grid { columns: Some(4), .. }));
    assert!(matches!(grid(vec![]), Widget::Grid { columns: None, .. }));
    assert!(matches!(with_columns(text("x"), 3), Widget::Text { .. }));
}
```

  mobiler-ui: round-trip `Widget::Grid { children: vec![], columns: Some(3) }` and
  `Widget::Card { .., style: CardStyle::Dashed, .. }` (build the card the way existing tests do).
- [ ] **RED**, then implement:
  - the field and variant, each with a BREAKING doc note
  - `grid` builds `columns: None`
  - `with_columns` (`#[must_use]`, `n.clamp(1, 4)`)
  - mobiler-web: `card_class` gains `CardStyle::Dashed => "card-dashed"`, and the `Widget::Grid`
    arm gains `..` (Task 2 renders it)
- [ ] **GREEN.** Run the ui/core/web tests, clippy, the wasm check, and `cargo check` on every demo,
  including `demos/fullstack-todo/mobile`. Commit:
  `feat(ui)!: Grid.columns + CardStyle::Dashed`

### Task 2: Web

- [ ] **Grid:** when `columns` is `Some(n)`, `<div class="grid" style=format!("grid-template-columns:repeat({},minmax(0,1fr))", n.clamp(1,4))>`.
  With `None`, today's markup exactly.
- [ ] **CSS:** `.card-dashed { background: transparent; border: 1.5px dashed var(--outline, var(--line)); }`,
  placed next to `.card-outlined`.
- [ ] Run the tests and wasm clippy. Commit: `feat(web): grid columns + dashed card`

### Task 3: Native shells

- [ ] **Barbershop Android:**
  - Grid: `val cols = widget.columns?.toInt()?.coerceIn(1, 4) ?: maxOf(2, (maxWidth.value / 190f).toInt())`.
  - Card `when`:

```kotlin
CardStyle.DASHED -> {
    // No fill; a 1.5dp dashed outline (dash 6 / gap 4) in the palette's outline, inset by half the
    // stroke so the clip doesn't cut it.
    val shape = shapeOf { it.card } ?: CardDefaults.outlinedShape
    val color = LocalPalette.current?.outline?.color() ?: MaterialTheme.colorScheme.outline
    val dashMod = Modifier.fillMaxWidth().clip(shape).then(clickMod).drawBehind {
        val w = 1.5.dp.toPx()
        val inset = Size(size.width - w, size.height - w)
        translate(w / 2, w / 2) {
            drawOutline(shape.createOutline(inset, layoutDirection, this), color,
                style = Stroke(w, pathEffect = PathEffect.dashPathEffect(floatArrayOf(6.dp.toPx(), 4.dp.toPx()))))
        }
    }
    Box(modifier = dashMod) { CardBody(widget.child, send) }
}
```

    Add the imports: `drawBehind`, `Stroke`, `PathEffect`, `Size`, `translate`, `drawOutline`.
    `CardBody` supplies the padding, as for Brand.
- [ ] **Other Android shells** (coffee, todo, saldo, fullstack-todo/mobile): in their `when`,
  `CardStyle.OUTLINED, CardStyle.DASHED -> OutlinedCard(...)`. Use the same call as today's
  OUTLINED line, with the extra condition.
- [ ] **Barbershop iOS:**
  - `case .grid(let children, let columns): GridView(children:columns:send:)`, where
    `GridView.columns: UInt8?` and `let cols = columns.map { Int(min(max($0, 1), 4)) } ?? (hSize == .regular ? 4 : 2)`.
  - In `CardMod`, add `case .dashed:` as spec'd.
- [ ] **Other iOS shells:** `case .grid(let children, _):` and, in their `CardMod`, add `.dashed`
  to the `.outlined` case (`case .outlined, .dashed:`).
- [ ] **Build** the APKs for barbershop, coffee, todo, saldo and fullstack-todo/mobile. Commit:
  `feat(android,ios): grid columns + dashed card (barbershop); new cases in every demo`

### Task 4: Barbershop demo + acceptance

- [ ] **Test first:** serialize `bookings_screen(&model)`. It contains
  `"columns":3` and `"style":"Dashed"`, and "09:00" and "11:30" tiles. RED.
- [ ] **Implement:** under the Status section, add `subtitle("Free times")`,
  `with_columns(grid(["09:00","09:30","10:00","10:30","11:00","11:30"].map(|t| card(text(t), CardStyle::Outlined))), 3)`,
  and `card_button(text("Free 12:45 – 13:30 · 45 min · + Book"), CardStyle::Dashed, Msg::Book)`.
  Match `card_button`'s real signature. GREEN, then clippy.
- [ ] **Web CDP:**
  - the "Free times" grid's computed `grid-template-columns` has 3 equal tracks
  - the other grids compute 2 tracks
  - `.card-dashed` computes `border-top-style: dashed`, `border-top-width: 1.5px` and a transparent
    background
  - clicking it opens the booking UI
  - no exceptions
  - coffee/todo pixel-identical to `main`
- [ ] **Android AVD** (foreground check before every action):
  - the "09:00", "09:30" and "10:00" tiles have equal widths
  - tapping the dashed card opens booking
  - take a screenshot and read the dashes
- [ ] **Commit:** `feat(barbershop): 3-column free-times grid + dashed free-slot card`

### Task 5: Docs, review, PR

- [ ] Add a NOTES.md section, then run a fresh whole-branch review (opus) and a fix pass.
- [ ] Ship with ship-pr (25 checks) and squash-merge.
- [ ] Update the memory and `start.md`.
