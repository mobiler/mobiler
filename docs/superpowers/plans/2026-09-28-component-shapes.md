# Component Shapes Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** `Theme.shapes` sets a corner radius per component (card, button, fab, sheet top, chip,
badge, input), either `Dp(n)` or `Pill`. Unset keeps today's shapes.

**Architecture:**
- **ABI:** `Shapes` and `Radius`.
- **Web:** `--r-<component>` variables with today's radii as fallbacks.
- **Android and iOS (barbershop):** a shape is passed only when the radius is set.
- **Demos:** only barbershop's `Theme` literal gains the field.

**Tech Stack:** Rust, CSS, Compose M3, SwiftUI (iOS 17: `UnevenRoundedRectangle`, `.buttonBorderShape`).

**Spec:** `docs/superpowers/specs/2026-09-28-component-shapes-design.md`

## Global Constraints

- **Unchanged without shapes.** With `shapes: None`, everything renders byte-identically. The web
  `theme_css` output is unchanged, and no native call site gains an argument.
- **Pill:**
  - Android: `RoundedCornerShape(percent = 50)`
  - iOS: `Capsule()` / `.capsule`
  - web: `999px`
- **Dp(n):** `n.dp` / `n` pt / `n`px.
- **The sheet radius** applies to its top corners only.
- **Today's web radii** (the fallbacks):
  - `.card` `var(--radius)`
  - `.btn` 999px, `.chip` 999px, `.badge` 999px
  - `.fab` 18px
  - `.sheet` `20px 20px 0 0`
  - `.field` 10px, `.searchfield` 999px
- **Repo rules:** don't touch templates, don't publish. The Swift gate is CI. Build rules are as in
  previous plans.

## Review Focus

1. **No shapes must stay identical.** Pinned by the Task 2 test `theme_css_without_shapes_is_unchanged`
   and the Task 5 coffee/todo pixel diffs.
2. **Pill at `Density::Large`.** Pinned by the Task 5 Android check with large controls on.
3. **Sheet bottom corners.** They must stay square, since the sheet is flush with the screen bottom.
   Pinned by the Task 5 web CDP check (`border-bottom-left-radius` 0).
4. **A partial `Shapes`.** Only the set components change. Pinned by the Task 2 test
   `theme_css_emits_only_set_radii`.

---

### Task 1: ABI

- [ ] **Step 1: Tests.**
  - mobiler-ui: `round_trips(&Theme { shapes: Some(Shapes { card: Some(Radius::Dp(12)), button: Some(Radius::Pill), ..Default::default() }), ..Default::default() })`
  - `Theme::default().shapes == None`
- [ ] **Step 2: RED.**
- [ ] **Step 3: Implement.**
  - `Shapes { card, button, fab, sheet_top, chip, badge, input: Option<Radius> }` (`Default`, Copy, Eq).
  - `enum Radius { Dp(u8), Pill }`.
  - `Theme.shapes: Option<Shapes>`, with `Default` = `None`.
  - Re-exports in mobiler-core.
  - Barbershop's `Theme` literal gains `shapes: None`.
- [ ] **Step 4: GREEN.** Run tests, clippy, the wasm check, and `cargo check` on every demo.
- [ ] **Step 5: Commit:** `feat(ui)!: Theme.shapes — per-component corner radii`

### Task 2: Web

- [ ] **Step 1: Tests.**
  - `theme_css_without_shapes_is_unchanged`: `Some(Shapes::default())` must equal the default theme's output.
  - `theme_css_emits_only_set_radii`: card `Dp(12)` + button `Pill` gives `--r-card:12px;--r-button:999px;`
    and no `--r-fab`.
- [ ] **Step 2: RED.**
- [ ] **Step 3: Implement.** `shapes_css(&Shapes)` appends to `typography` in `theme_css`. CSS
  (exact fallbacks):
  - `.card` `border-radius: var(--r-card, var(--radius))`
  - `.btn` `var(--r-button, 999px)`
  - `.fab` `var(--r-fab, 18px)`
  - `.sheet` `var(--r-sheet-top, 20px) var(--r-sheet-top, 20px) 0 0`
  - `.chip` `var(--r-chip, 999px)`
  - `.badge` `var(--r-badge, 999px)`
  - `.field` `var(--r-input, 10px)`
  - `.searchfield` `var(--r-input, 999px)`
- [ ] **Step 4: GREEN.** Run tests, clippy, and the wasm check.
- [ ] **Step 5: Commit:** `feat(web): Theme.shapes — per-component radius vars`

### Task 3: Android (barbershop)

- [ ] **Step 1: Helper** in `ui/theme/Theme.kt`:
  `fun radiusShape(r: Radius): Shape = when (r) { is Radius.Pill -> RoundedCornerShape(percent = 50); is Radius.Dp -> RoundedCornerShape(r.value.toInt().dp) }`.
  Check the generated Kotlin for the `Radius` sealed-class shape and field name.
- [ ] **Step 2: Call sites** (grep in MainActivity). Each passes `shape = …` only when set, using
  `activeTheme?.shapes?.<c>?.let { radiusShape(it) }`. Where the parameter has a default, use
  `?: <that default>`, and check the exact default names:
  - **Cards:** each `Card(`/`OutlinedCard(` gets `shape = s ?: CardDefaults.shape` (OutlinedCard:
    `CardDefaults.outlinedShape`). The Brand card clip becomes `s ?: MaterialTheme.shapes.medium`.
  - **Buttons** in MobilerButton, all four styles: `shape = s ?: ButtonDefaults.shape` (text:
    `ButtonDefaults.textShape`, outlined: `ButtonDefaults.outlinedShape`, tonal:
    `ButtonDefaults.filledTonalShape`).
  - **FAB:** `shape = s ?: FloatingActionButtonDefaults.shape`.
  - **Sheet:** `ModalBottomSheet(shape = sheetTop?.let { RoundedCornerShape(topStart = …, topEnd = …) } ?: BottomSheetDefaults.ExpandedShape)`.
    For `Pill`, use a large dp, e.g. 999.dp, on the top corners.
  - **Chip:** `shape = s ?: FilterChipDefaults.shape`.
  - **Badge:** `clip(s ?: RoundedCornerShape(50))`, at the existing badge Box.
  - **Inputs:** `OutlinedTextField(shape = s ?: OutlinedTextFieldDefaults.shape)`. The search field
    keeps `RoundedCornerShape(50)` as its fallback.
- [ ] **Step 3: Build.** Barbershop APK. Commit: `feat(android): Theme.shapes (barbershop)`

### Task 4: iOS (barbershop)

- [ ] **Step 1: Helpers** in `Render.swift`:

```swift
/// Theme.shapes radius → a shape; nil = the site's own shape (unchanged).
enum ShapeTokens {
    static var shapes: Shapes? { ActiveTheme.current?.shapes }
    static func shape(_ r: Radius?) -> AnyShape? {
        switch r {
        case .none: return nil
        case .some(.pill): return AnyShape(Capsule())
        case .some(.dp(let n)): return AnyShape(RoundedRectangle(cornerRadius: CGFloat(n)))
        }
    }
}
```

  Check the generated Swift for the case names `.dp(UInt8)` / `.pill`. `AnyShape` needs iOS 16+.
- [ ] **Step 2: Sites.** Use `ShapeTokens.shape(ShapeTokens.shapes?.card) ?? AnyShape(RoundedRectangle(cornerRadius: today))`.
  - **CardMod:** `let shape` becomes `AnyShape`.
  - **Buttons:** `PaletteButtonStyle`/`LargeButtonStyle`/`TonalButtonStyle` clip with the button
    shape instead of `Capsule()`. For the `.borderedProminent` / `.bordered` path, add
    `.buttonBorderShape(r == .pill ? .capsule : .roundedRectangle(radius: n))` only when set.
  - **FAB:** instead of `RoundedRectangle(18)`.
  - **Sheet panel:** `UnevenRoundedRectangle(topLeadingRadius: r, topTrailingRadius: r)` when set,
    otherwise today's `RoundedRectangle(cornerRadius: 20)`. For `Pill`, use a large radius.
  - **Chip** capsule, **badge** capsule, **input:** the `PaletteFieldStyle` background plus a plain
    style when `input` is set; the search capsule.
- [ ] **Step 3:** Compile scrutiny, then commit: `feat(ios): Theme.shapes (barbershop)`

### Task 5: Barbershop shapes + acceptance

- [ ] **Step 1: Set the shapes.** Barbershop gets card `Dp(12)`, button `Pill`, fab `Dp(16)`,
  sheet_top `Dp(28)`, chip `Dp(8)`, badge `Pill`, input `Dp(12)`, plus a core test.
- [ ] **Step 2: Web CDP.** Check computed values:
  - `.card` 12px, `.btn` 999px, `.fab` 16px
  - `.chip` 8px, `.badge` 999px, `.field` 12px (search 12px)
  - `.sheet` top-left 28px and bottom-left 0 (open it via Services → card → Book this)
  - no exceptions

  Coffee/todo must be pixel-identical to `main`.
- [ ] **Step 3: Android.**
  - Take a screenshot. Sample a card's corner (the background shows at the outer corner pixel) and
    check the button ends are round.
  - Turn on "Large controls" (Profile tab toggle, which avoids the crash when leaving the Profile
    tab): the buttons are still pills in the screenshot.
- [ ] **Step 4: Commit.**

### Task 6: Docs, review, PR

- [ ] NOTES.md section, then a fresh review and fix pass, then ship-pr (25 checks) and
  squash-merge. Update memory and `start.md`.
