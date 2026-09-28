# Type Scale Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Apps can set size, weight and family per text style (`Theme.type_scale`), and get
`Display` and `Headline` text styles with platform defaults. Accessibility text scaling keeps
working, and apps without a scale render exactly as before.

**Architecture:**
- **ABI:** `TextStyle` gains `Display` and `Headline`. `Theme` gains `type_scale: Option<TypeScale>`,
  holding one `Option<TypeSpec { size, weight, family: FamilyRole }>` per style.
- **Web:** per-style CSS variables with the old values as fallbacks.
- **Android** builds the Compose `TextStyle` from the spec.
- **iOS** builds a `Font` from the spec, scaling system sizes with `UIFontMetrics`.
- **Scope:** barbershop gets the implementation. The other demos only get the new enum cases.

**Tech Stack:** Rust, Leptos/CSS, Compose M3, SwiftUI.

**Spec:** `docs/superpowers/specs/2026-09-28-type-scale-design.md`

## Global Constraints

- **Unchanged without a scale.** With `type_scale: None`, every existing style renders
  byte-identically, and the web `theme_css` output is unchanged.
- **Where each spec applies:**
  - `title` → Title text and the top-bar title
  - `headline` → Headline text and sheet titles
  - `display` → Display text
  - the rest → their own style
- **Defaults when no spec is set:**
  - Display: 36, weight 700
  - Headline: 24, weight 600
  - Under `Custom`, both use the display family.
- **Line height** (only where a spec is set): 1.25× size for display/headline/title, 1.45× for the
  others. iOS keeps its natural line height.
- **Accessibility scaling:** Android `sp`; web `rem` = size/16; iOS `Font.custom(…, relativeTo:)` or
  `.system(size: UIFontMetrics(forTextStyle:).scaledValue(for:))`.
  - `relativeTo` per style: display → `.largeTitle`, headline/title → `.title2`,
    subtitle → `.subheadline`, body/emphasis → `.body`, caption → `.caption`.
- **Weight** is clamped to 100..=900 and rounded to the nearest 100. On iOS it maps to the nearest
  `Font.Weight`.
- **Repo rules:** don't touch templates, don't publish. Build rules and CI Swift gate are as in the
  previous plans.

## Review Focus

1. **No scale must stay identical.** Pinned by the Task 2 web test `theme_css_without_scale_is_unchanged`
   and the Task 5 coffee/todo pixel diffs vs a `main` build.
2. **Largest accessibility text setting.** Every style must grow, including system-font specs on
   iOS. Pinned by the Task 5 Android `font_scale` check; iOS uses `@Environment(\.dynamicTypeSize)`
   to re-render (reviewed).
3. **A partial scale.** Only the set styles change; the rest keep today's look. Pinned by the Task 2
   web test `theme_css_emits_only_set_specs`.
4. **The sheet title and top-bar title follow headline/title.** Pinned by the Task 5 web CDP check
   (`.sheet-title` needs an open sheet; barbershop's booking sheet opens from a service card) and
   the top-bar check.
5. **Out-of-range weights** (0, 1000). They must clamp, never crash. Pinned by the Task 2 test
   `weights_are_clamped`.

---

### Task 1: ABI + core builders

**Files:** `mobiler-ui/src/lib.rs`, `mobiler-core/src/lib.rs`.

**Interfaces (produces):**
- `TextStyle::{Display, Headline}`, appended to the enum.
- `pub struct TypeScale { display, headline, title, subtitle, body, emphasis, caption: Option<TypeSpec> }`
  (`Default`).
- `pub struct TypeSpec { pub size: u8, pub weight: u16, pub family: FamilyRole }`, with
  `TypeSpec::new(u8, u16, FamilyRole)`.
- `pub enum FamilyRole { Display, Body }`.
- `Theme.type_scale: Option<TypeScale>`, with `Default` = `None`.
- All derive `Facet, Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq` + `#[repr(C)]`.
- mobiler-core: `display(text)`, `headline(text)`, and re-exports of `TypeScale`, `TypeSpec`,
  `FamilyRole`.

- [ ] **Step 1: Failing tests.**
  - mobiler-ui: round-trip a `Theme { type_scale: Some(TypeScale { display: Some(TypeSpec::new(36, 700, FamilyRole::Display)), ..Default::default() }), ..Default::default() }`,
    plus `Widget::Text { style: TextStyle::Display }`.
  - mobiler-core: `matches!(display("x"), Widget::Text { style: TextStyle::Display, .. })`, and the
    same for `headline`.
- [ ] **Step 2: RED.**
- [ ] **Step 3: Implement.** Doc comments per the spec. Fix every Rust exhaustive match (web
  `text_class` gets `t-display` / `t-headline`, for Task 2).
- [ ] **Step 4: GREEN.** Run ui/core tests, clippy, the web wasm check, and `cargo check` on every
  demo workspace.
- [ ] **Step 5: Commit:** `feat(ui)!: TextStyle Display/Headline + Theme.type_scale`

---

### Task 2: Web

**Files:** `mobiler-web/src/lib.rs` (`theme_css`, `text_class`), `mobiler-web/src/mobiler.css`.

**Interfaces:** `fn type_scale_css(ts: &TypeScale, custom_font: bool) -> String`.

- [ ] **Step 1: Failing tests:**

```rust
    #[test]
    fn theme_css_without_scale_is_unchanged() {
        // (already covered by theme_css_without_palette_is_unchanged — keep it passing; add:)
        let t = Theme { type_scale: Some(TypeScale::default()), ..Default::default() };
        assert_eq!(theme_css(&t, false), theme_css(&Theme::default(), false));
    }

    #[test]
    fn theme_css_emits_only_set_specs() {
        let ts = TypeScale { display: Some(TypeSpec::new(36, 700, FamilyRole::Display)), ..Default::default() };
        let css = theme_css(&Theme { type_scale: Some(ts), ..Default::default() }, false);
        assert!(css.contains("--ts-display-size:2.25rem;--ts-display-weight:700;--ts-display-lh:1.25;"), "{css}");
        assert!(!css.contains("--ts-body"));
        assert!(!css.contains("--ts-display-family"), "family only under Custom");
    }

    #[test]
    fn custom_font_scale_sets_the_family() {
        let ts = TypeScale { caption: Some(TypeSpec::new(12, 400, FamilyRole::Body)), ..Default::default() };
        let t = Theme { font: FontFamily::Custom, type_scale: Some(ts), ..Default::default() };
        assert!(theme_css(&t, false).contains("--ts-caption-family:var(--font);"));
    }

    #[test]
    fn weights_are_clamped() {
        let ts = TypeScale { body: Some(TypeSpec::new(16, 0, FamilyRole::Body)), title: Some(TypeSpec::new(22, 1000, FamilyRole::Display)), ..Default::default() };
        let css = theme_css(&Theme { type_scale: Some(ts), ..Default::default() }, false);
        assert!(css.contains("--ts-body-weight:100;") && css.contains("--ts-title-weight:900;"), "{css}");
    }
```

- [ ] **Step 2: RED.**
- [ ] **Step 3: Implement.**
  - `type_scale_css` formats each set spec:
    - `--ts-<s>-size:<size/16, up to 4 decimals, trailing zeros trimmed>rem;`
    - `--ts-<s>-weight:<clamped>;`
    - `--ts-<s>-lh:<1.25|1.45>;`
    - under `Custom` only: `--ts-<s>-family:var(--font-display)` or `var(--font)`
  - `theme_css` appends it after the display font var (before the palette part), and only when
    `type_scale` is set.
  - `text_class`: `Display` → `t-display`, `Headline` → `t-headline`.
  - CSS: each `.t-*` rule uses `var(--ts-<s>-size, <old>)`, `var(--ts-<s>-weight, <old>)` and
    `line-height: var(--ts-<s>-lh, <old or normal>)`, keeping the exact old values. Where a rule had
    no line-height, use `var(--ts-<s>-lh, normal)` only if `normal` matches the inherited value.
    Otherwise use a separate rule `:is(.t-title)[style]`… **Simplest:** add the line-height
    declaration only through `.ts-set` scoping. The scaffold gets class `type-scale` when a scale is
    set, and `.type-scale .t-title { line-height: var(--ts-title-lh, inherit) }`. That keeps the
    no-scale path exact.
  - New `.t-display { font-size: var(--ts-display-size, 2.25rem); font-weight: var(--ts-display-weight, 700); margin: 0; line-height: var(--ts-display-lh, 1.2); }`
    and `.t-headline` (1.5rem / 600).
  - `.topbar .title` reads `--ts-title-size/weight`, and `.sheet-title` reads `--ts-headline-*`,
    both with their old values as fallbacks.
  - Family: `.font-custom .t-<s> { font-family: var(--ts-<s>-family, <current>) }`. The current
    value is display for title/subtitle/topbar/sheet/display/headline and body otherwise; add
    `.t-display`/`.t-headline` to the display selector.
- [ ] **Step 4: GREEN.** Run tests, clippy, and the wasm check.
- [ ] **Step 5: Commit:** `feat(web): type scale — per-style CSS vars, Display/Headline styles`

---

### Task 3: Android (barbershop) + demos' `when`

**Files:** barbershop `MainActivity.kt` (`typographyFor(style)`, top bar, sheet title) and
`ui/theme/Type.kt` (the spec → `TextStyle` helper); every demo's `MainActivity.kt` `when` over
`ModelTextStyle` (add `DISPLAY` → `displaySmall.copy(fontWeight = FontWeight.Bold)`,
`HEADLINE` → `headlineSmall.copy(fontWeight = FontWeight.SemiBold)`).

- [ ] **Step 1: Other demos.** Add the two `when` cases, exactly as the defaults above.
- [ ] **Step 2: Barbershop Type.kt:**

```kotlin
/** A type-scale spec as a Compose TextStyle over `base`: size (sp, scales with the system font size),
 *  weight (clamped, nearest 100), line height (1.25× for display/headline/title, else 1.45×), and the
 *  synced display/body family under FontFamily.CUSTOM. */
fun applySpec(base: androidx.compose.ui.text.TextStyle, spec: TypeSpec, tight: Boolean, custom: Boolean, context: android.content.Context): androidx.compose.ui.text.TextStyle {
    val w = ((spec.weight.toInt() + 50) / 100 * 100).coerceIn(100, 900)
    val family = if (custom) syncedFamily(context, if (spec.family == FamilyRole.DISPLAY) "display" else "body") else null
    return base.copy(
        fontSize = spec.size.toInt().sp,
        fontWeight = FontWeight(w),
        lineHeight = (spec.size.toInt() * if (tight) 1.25f else 1.45f).sp,
        fontFamily = family ?: base.fontFamily,
    )
}
```

  `MainActivity.typographyFor(style)`: keep today's slot as `base`, add `DISPLAY`/`HEADLINE`
  defaults, then `activeTheme?.typeScale?.<style>?.let { applySpec(base, it, tight, custom, LocalContext.current) } ?: base`.

  - Top bar: `title = { Text(widget.title, style = specOrDefault(title, MaterialTheme.typography.titleLarge)) }`.
  - Sheet title: `specOrDefault(headline, titleLarge)`.
  - When the spec is unset, the expression is today's style.
- [ ] **Step 3: Build.** Barbershop APK.
- [ ] **Step 4: Commit:** `feat(android): type scale + Display/Headline (barbershop), new TextStyle cases in every demo`

---

### Task 4: iOS (barbershop) + demos' `switch`

**Files:** every demo's `Render.swift` `TextStyleMod` (the new cases with the default fonts) and
barbershop's `Render.swift` (spec support).

- [ ] **Step 1: Other demos.** Add `case .display: return AnyView(content.font(.system(size: UIFontMetrics(forTextStyle: .largeTitle).scaledValue(for: 36), weight: .bold, design: design)))`
  and `.headline` (24, `.semibold`, `.title2`). Add `@Environment(\.dynamicTypeSize) private var dts`
  with `let _ = dts` to `TextStyleMod`. Also check for any other exhaustive switch over `TextStyle`.
- [ ] **Step 2: Barbershop.**

```swift
/// A type-scale spec as a Font: the synced custom family (Font.custom relativeTo → Dynamic Type) or the
/// system font at a UIFontMetrics-scaled size (Font.system(size:) doesn't scale by itself).
enum TypeScaleFonts {
    static func weight(_ w: UInt16) -> Font.Weight {
        switch (min(max(Int(w), 100), 900) + 50) / 100 * 100 {
        case 100: return .ultraLight
        case 200: return .thin
        case 300: return .light
        case 400: return .regular
        case 500: return .medium
        case 600: return .semibold
        case 700: return .bold
        case 800: return .heavy
        default: return .black
        }
    }
    static func font(_ spec: TypeSpec, relativeTo: Font.TextStyle, uiStyle: UIFont.TextStyle, design: Font.Design) -> Font {
        let family = spec.family == .display ? CustomFonts.display : CustomFonts.body
        if let f = CustomFonts.font(family, size: CGFloat(spec.size), relativeTo: relativeTo) { return f.weight(weight(spec.weight)) }
        return .system(size: UIFontMetrics(forTextStyle: uiStyle).scaledValue(for: CGFloat(spec.size)), weight: weight(spec.weight), design: design)
    }
    static var scale: TypeScale? { ActiveTheme.current?.typeScale }
}
```

  - `TextStyleMod`: for each style, `if let spec = TypeScaleFonts.scale?.<style> { font(spec, …) } else { today's expression }`.
    `.display`/`.headline` fall back to the defaults.
  - Top-bar title: `TypeScaleFonts.scale?.title.map { font($0, relativeTo: .title2, uiStyle: .title2, design:) } ?? <today's>`.
  - Sheet title: the same, with headline.
  - The generated field name is `typeScale`, and `FamilyRole` cases are `.display`/`.body`. Check both
    against the codegen.
- [ ] **Step 3: Compile scrutiny.** CI is the gate.
- [ ] **Step 4: Commit:** `feat(ios): type scale + Display/Headline (barbershop), new TextStyle cases in every demo`

---

### Task 5: Barbershop scale + acceptance

- [ ] **Step 1.** Barbershop's theme gets the scale:
  - display 36/700 Display
  - headline 24/600 Display
  - title 22/600 Display
  - subtitle 14/500 Body
  - body 16/400 Body
  - emphasis 16/600 Body
  - caption 12/400 Body

  Home shows `display("12:00 – 12:45")` and `headline("Booking")` near the top. A core test checks
  `view` carries the scale. Run barbershop tests and clippy.
- [ ] **Step 2: Web CDP.** Check computed values:
  - `.t-display`: `font-size` 36px, weight 700, `font-family` starts with `mobiler-display`, and
    `line-height` 45px
  - `.topbar .title`: 22px / 600
  - `.t-caption`: 12px
  - `.t-body`: 16px, line-height 23.2px

  Open the booking sheet and check `.sheet-title` is 24px / 600. No exceptions. Coffee/todo are
  pixel-identical to `main`.
- [ ] **Step 3: Android.**
  - Using uiautomator bounds, check that the `12:00 – 12:45` node height ÷ a body text node height is
    ≈ 36/16 (±15%, since line heights differ: 45/23.2 ≈ 1.94).
  - Run `adb shell settings put system font_scale 1.3`, restart the app, and check the display node
    height grows ≈ 1.3×. Reset to 1.0 afterwards.
  - Coffee vs `main`: the reasoning ruling as before (only enum cases are added), unless a build is
    cheap.
- [ ] **Step 4: Commit:** `feat(barbershop): Moj Termin type scale + Display time`

---

### Task 6: Docs, review, PR

- [ ] NOTES.md section. Add `display()`/`headline()` and the type scale to `mobiler-core/README.md`
  if it lists text builders, and to the root README theming section.
- [ ] Fresh whole-branch review and fix pass.
- [ ] ship-pr (25 checks), squash-merge, no publish. Update memory and `start.md`.
