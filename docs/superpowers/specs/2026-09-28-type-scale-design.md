# Type scale: per-style size, weight and family, plus Display and Headline styles

**Request:** `docs/type-scale.md` (Moj Termin design set, priority 4, blocking).
**Release:** part of the single Moj Termin design release (nothing published until the set is done).
It lands on `main` as core + mobiler-web + **barbershop** shells. Every demo shell gets the new
enum cases. Templates are ported in the release CLI PR.
**Builds on:** `FontFamily::Custom` (PR #225). A spec's `family` picks the synced display/body font.

## Problem

`TextStyle` has five intent styles, and each shell maps them to fixed platform styles. The app can't
set sizes or weights, and there is no style for the design's 36 display number or its 24 sheet
headline.

## Decisions (approved 2026-09-28)

1. **Two new styles.** `TextStyle` gains `Display` and `Headline`, appended last. Their builders are
   `display(text)` and `headline(text)`.
   - It's breaking for exhaustive matches: every demo shell's `when`/`switch` over `TextStyle` gets
     the two cases, like `FontFamily::Custom` did.
2. **`Theme.type_scale: Option<TypeScale>`.**

   ```rust
   pub struct TypeScale {
       pub display: Option<TypeSpec>, pub headline: Option<TypeSpec>, pub title: Option<TypeSpec>,
       pub subtitle: Option<TypeSpec>, pub body: Option<TypeSpec>, pub emphasis: Option<TypeSpec>,
       pub caption: Option<TypeSpec>,
   }
   pub struct TypeSpec { pub size: u8, pub weight: u16, pub family: FamilyRole }
   pub enum FamilyRole { Display, Body }
   ```

   - `None`, or a style left `None`, means today's platform mapping for that style.
   - `TypeSpec::new(size, weight, family)` is provided.
   - `weight` is clamped to 100..=900 and rounded to the nearest 100 by the shells.
3. **Where each spec applies.**

   | Spec | Applies to |
   |---|---|
   | `title` | `Title` text and the scaffold top-bar title |
   | `headline` | `Headline` text and **sheet titles** |
   | `display` | `Display` text |
   | `subtitle` / `body` / `emphasis` / `caption` | their styles |

4. **Defaults for the new styles** when the app sets no scale:
   - **Display:** 36, bold (700). Android `displaySmall` (36sp) at bold. iOS
     `.system(size: 36, weight: .bold)` relative to `.largeTitle`. Web `2.25rem`/700.
   - **Headline:** 24, semibold (600). Android `headlineSmall` (24sp) at semibold. iOS
     `.system(size: 24, weight: .semibold)` relative to `.title2`. Web `1.5rem`/600.
   - **Family:** under `FontFamily::Custom`, Display and Headline use the display family, like Title
     and Subtitle.
5. **Family.** `FamilyRole::Display`/`Body` selects the synced custom family only when
   `font == Custom`. Otherwise it's the theme's system font as today, and the role has no effect.
6. **Line height.** When a spec is set: 1.25× its size for display/headline/title, 1.45× for the
   other styles.
   - Web and Android apply it.
   - iOS SwiftUI can't set a font's line height, so it keeps its natural spacing there. This is an
     accepted platform difference.
7. **Accessibility is preserved:**
   - Android: sizes in `sp`.
   - iOS: custom fonts use `Font.custom(family, size:, relativeTo:)`, which scales with Dynamic Type.
     `Font.system(size:)` does **not** scale, so system-font specs use
     `.system(size: UIFontMetrics(forTextStyle: <relativeTo>).scaledValue(for: size), weight:, design:)`.
     `TextStyleMod` reads `@Environment(\.dynamicTypeSize)` so a text-size change re-renders it. The
     relativeTo styles are display → `.largeTitle`, headline → `.title2`, title → `.title2`,
     subtitle → `.subheadline`, body/emphasis → `.body`, caption → `.caption`.
   - Web: `rem` (size ÷ 16), so browser text size settings scale it.

## Per shell

- **Web.**
  - `theme_css` emits, per set spec, `--ts-<style>-size:<rem>;--ts-<style>-weight:<w>;--ts-<style>-lh:<ratio>;`,
    plus, under `Custom`, `--ts-<style>-family: var(--font-display)` or `var(--font)`.
  - `.t-<style>` rules read `var(--ts-<style>-size, <today's value>)`, and the same for weight and
    line-height. Family is read only under `.font-custom`.
  - New `.t-display` / `.t-headline` classes have the defaults above.
  - The top bar reads `--ts-title-*` and the sheet title reads `--ts-headline-*`, each with its
    current value as the fallback. So a theme without a scale renders byte-identically.
- **Android (barbershop).**
  - `typographyFor(style)` builds the widget `TextStyle` from the spec when set: `fontSize`,
    `fontWeight`, `lineHeight`, and `fontFamily` from the synced role or the theme default. The
    unset case keeps today's slot.
  - Display → `displaySmall.copy(fontWeight = Bold)`, Headline → `headlineSmall.copy(fontWeight = SemiBold)`
    by default.
  - The top bar and the sheet title apply the title/headline spec when set.
- **iOS (barbershop).**
  - `TextStyleMod` gains `.display`/`.headline`.
  - When a spec is set, it uses the spec's size and weight in the resolved family (custom via
    `CustomFonts`, else system with the theme's design) with `relativeTo`.
  - The top-bar title and the sheet title use the title/headline spec when set.
  - The unset path is today's expressions.

## Demo

Barbershop gets the design's scale:
- display 36/700 display family
- headline 24/600 display family
- title 22/600 display family
- subtitle 14/500 body family
- body 16/400 body family
- emphasis 16/600 body family
- caption 12/400 body family

It shows `display("12:00 – 12:45")` and `headline("Next booking")` on Home.

## Verification

- **ui/core unit tests:** builders, round-trips with a `TypeScale`, `TypeSpec::new`.
- **Web tests:**
  - `theme_css` emits `--ts-*` only for set specs.
  - A theme without a scale is byte-identical.
- **Web (CDP):** computed `font-size` / `font-weight` / `line-height` / `font-family` of `.t-display`,
  `.t-title`, the top-bar title and `.t-caption` equal the scale. No exceptions. Coffee/todo
  pixel-identical to `main`.
- **Android (AVD).**
  - Measure a `Display` text node's height vs a `Body` node's (uiautomator bounds): the ratio is
    ≈ 36/16.
  - With `adb shell settings put system font_scale 1.3`, the Display node grows ≈ 1.3× (acceptance 2).
  - Reset the font scale afterwards.
  - Coffee unchanged vs `main`.
- **iOS:** CI compile only.

## Out of scope

- Letter-spacing and per-style colour.
- Italic.
- Scaling the rest of the chrome (buttons, chips) from the scale. Those keep their platform sizes
  in the body family.
