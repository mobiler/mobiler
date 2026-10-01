# Dark Theme Gaps Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Moj Termin's four theming gaps: a dark launch window on a dark phone (built on the files the next
release's `[splash]` will write), guidance for a correct first frame, and the optional `selection`, `error`,
`error_fill`, `on_error_fill` colour roles in all three shells.

**Architecture:** The four roles are appended to `ColorRoles` (mobiler-ui) and read by each shell with today's
colour as the fallback (ADR-0019). The launch window is Android theme resources pointing at app-owned "seed"
resources, a new upgrade class that is created when missing and never overwritten (ADR-0042). Two pull requests:
**PR A** (libraries + barbershop shells + ADR-0041), then the library publish, then **PR B** (CLI: seed class,
template port, ADR-0042).

**Tech Stack:** Rust (mobiler-ui, mobiler-core, mobiler-web/Leptos, the CLI), Kotlin/Compose M3 (Android shell),
SwiftUI (iOS shell), CSS.

**Spec:** `docs/superpowers/specs/2026-10-01-dark-theme-gaps-design.md`

## Global Constraints

- Every unset role renders exactly as today, on every shell (ADR-0019).
- New `ColorRoles` fields are appended after `scrim`, in this order: `selection`, `error`, `error_fill`,
  `on_error_fill`, all `Option<Rgb>` (ADR-0008: a minor bump, ui 0.30 / core 0.41 / web 0.41).
- Shell changes land in `demos/barbershop` first; templates are ported from barbershop's diff in PR B (ADR-0015).
- Template code that uses the new ABI items lands only in PR B, after the libraries are on crates.io (ADR-0009).
- Android code needing an API above 26 is guarded by a version check or a `-v31` resource folder (ADR-0039).
- No shell calls `UiModeManager.setApplicationNightMode` or `AppCompatDelegate.setDefaultNightMode` (ADR-0041).
- Generated Kotlin/Swift field names are camelCase: `selection`, `error`, `errorFill`, `onErrorFill`.
- Build rules on this host: `export JAVA_HOME=~/jdk21` (the inherited JDK 25 breaks gradle); a per-demo
  `CARGO_TARGET_DIR` inside the demo (never the global one); never poll with `pgrep -f` or file markers; never wipe
  `~/.gradle/caches`. Emulator: `mobiler_pixel7` on port 5560 only (`adb -s emulator-5560`); never touch
  `emulator-5554` / `mobiler_verify_p7`.
- Commit messages via `git commit -F <file>` (backticks in `-m` run as commands). End with
  `Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>`.
- Every crates.io publish and `v*` tag needs the maintainer's explicit yes; auto mode blocks the release merge and
  the tag push, so hand the user the `! …` commands.

## Review Focus

1. **iOS palette with `selection` but no `surface_muted` and no input radius** (the `.roundedBorder` branch): a
   focused field must still show the selection border. Task 4 Step 3 covers it.
2. **A runtime light/dark flip** must re-read `selection` / `error` on iOS (the #243 stale-palette bug class):
   every new read sits in a view that already re-renders on `paletteRoles`, or reads `pal` in `render`. Task 4
   Step 5 checks each site.
3. **`error_fill` set without `on_error_fill` (or the reverse):** each falls back independently to today's value;
   no crash, no transparent text. Tests in Task 2 (web) and the code in Tasks 3–4 do the same.
4. **`mobiler upgrade` without `--apply`, or with a user-edited `themes.xml`:** the theme change becomes a
   `.mobiler-new`, but the seed files are still created, so whichever theme the app keeps still builds. Task 10
   Step 1 test `seed_files_created_even_when_theme_is_offered_as_new`.
5. **An app that already has its own `values-night/themes.xml`:** upgrade must not overwrite it; it gets a
   `.mobiler-new` (Shell class, no baseline). Task 10 Step 1 test `existing_night_theme_is_not_overwritten`.

---

## PR A: libraries + barbershop shells

Start from branch `feat/dark-theme-gaps` (the spec is already committed there).

### Task 1: `ColorRoles` gains `selection`, `error`, `error_fill`, `on_error_fill`

**Files:**
- Modify: `mobiler-ui/src/lib.rs` (struct `ColorRoles`, ~line 468; test `theme_default_has_no_palette_and_palette_round_trips`, ~line 1040)
- Modify: `demos/barbershop/app-core/src/lib.rs` (`moj_termin_palette()`, ~lines 1365–1405)

**Interfaces:**
- Produces: `ColorRoles.selection`, `.error`, `.error_fill`, `.on_error_fill: Option<Rgb>` (Kotlin/Swift:
  `selection`, `error`, `errorFill`, `onErrorFill`).

- [ ] **Step 1: Write the failing test** — append to the mobiler-ui test module:

```rust
    #[test]
    fn selection_and_error_roles_round_trip_and_default_to_none() {
        let d = ColorRoles::default();
        assert_eq!((d.selection, d.error, d.error_fill, d.on_error_fill), (None, None, None, None));
        let dark = ColorRoles {
            selection: Some(Rgb::hex(0x4fb3a4)),
            error: Some(Rgb::hex(0xff8f85)),
            error_fill: Some(Rgb::hex(0xc0392b)),
            on_error_fill: Some(Rgb::hex(0xffffff)),
            ..Default::default()
        };
        round_trips(&Theme { palette: Some(Palette { light: ColorRoles::default(), dark }), ..Default::default() });
    }
```

- [ ] **Step 2: Run it, expect a compile failure** — `cargo test -p mobiler-ui selection_and_error_roles` →
  `error[E0560]: struct ColorRoles has no field named selection`.

- [ ] **Step 3: Add the fields** after `scrim` in `ColorRoles`:

```rust
    /// Sheet and dialog scrim.
    pub scrim: Option<Rgba>,
    /// Focus and selection marks: the focused field's border, label and cursor, checked toggles and
    /// checkboxes, sliders, progress. Unset: the shell's current colour (`primary` on most shells).
    pub selection: Option<Rgb>,
    /// Error text and the invalid field's border; outlined and text Danger buttons. Unset: `danger`'s
    /// on-container colour, as before.
    pub error: Option<Rgb>,
    /// A filled Danger button's background. Unset: the `danger` pair, swapped, as before.
    pub error_fill: Option<Rgb>,
    /// A filled Danger button's text. Unset: as before.
    pub on_error_fill: Option<Rgb>,
```

  Also extend two existing field docs: `primary` → `/// Filled buttons, selected calendar day, toggles (unless
  \`selection\` is set).` and the tone-pair doc → `/// One container/on-container pair per [\`Tone\`]. Filled
  toned buttons swap the pair (Danger: unless \`error_fill\` is set).`

- [ ] **Step 4: Fix barbershop's full literal and give it the design's `sel`/`err` colours.** In
  `moj_termin_palette()`, add to the `dark` literal (which has no `..Default::default()` today) and to `light`:

```rust
        // dark
        selection: Some(Rgb::hex(0x4fb3a4)),
        error: Some(Rgb::hex(0xff8f85)),
        error_fill: Some(Rgb::hex(0xc0392b)),
        on_error_fill: Some(Rgb::hex(0xffffff)),
        // light
        selection: Some(Rgb::hex(0x1f8276)),
        error: Some(Rgb::hex(0xb3261e)),
        error_fill: Some(Rgb::hex(0xb3261e)),
        on_error_fill: Some(Rgb::hex(0xffffff)),
```

  and end the `dark` literal with `..Default::default()` so the next appended role doesn't break it.

- [ ] **Step 5: Run** `cargo test -p mobiler-ui` (all pass) and
  `CARGO_TARGET_DIR=demos/barbershop/target cargo check --manifest-path demos/barbershop/Cargo.toml` (compiles).

- [ ] **Step 6: Commit** `feat(ui): ColorRoles selection, error, error_fill, on_error_fill`.

### Task 2: web shell reads the new roles

**Files:**
- Modify: `mobiler-web/src/lib.rs` (`palette_css`, ~line 2753; `mod palette_tests`, ~line 3319)
- Modify: `mobiler-web/src/mobiler.css` (lines ~87, ~240, ~356–363, ~399–403, ~419, ~433)

**Interfaces:**
- Consumes: Task 1's fields.
- Produces: CSS variables `--selection`, `--error`, `--error-fill`, `--on-error-fill`, emitted only when set.

- [ ] **Step 1: Write the failing tests** in `mod palette_tests`:

```rust
    #[test]
    fn palette_css_emits_selection_and_error_roles_only_when_set() {
        assert_eq!(palette_css(&ColorRoles::default()), "");
        let r = ColorRoles {
            selection: Some(Rgb::hex(0x4fb3a4)),
            error: Some(Rgb::hex(0xff8f85)),
            error_fill: Some(Rgb::hex(0xc0392b)),
            ..Default::default()
        };
        let css = palette_css(&r);
        assert!(css.contains("--selection:rgb(79,179,164);"));
        assert!(css.contains("--error:rgb(255,143,133);"));
        assert!(css.contains("--error-fill:rgb(192,57,43);"));
        assert!(!css.contains("--on-error-fill"), "unset on_error_fill emits nothing (falls back to today's ink)");
    }

    #[test]
    fn palette_without_new_roles_gives_the_same_css_as_before() {
        let r = ColorRoles { background: Some(Rgb::hex(0x231f20)), primary: Some(Rgb::hex(0x1f8276)), ..Default::default() };
        assert_eq!(palette_css(&r), "--bg:rgb(35,31,32);--primary:rgb(31,130,118);--pal-primary:rgb(31,130,118);");
    }
```

- [ ] **Step 2: Run** `cargo test -p mobiler-web palette_` (from `mobiler-web/`, its own workspace) → the first
  test FAILS (`--selection` missing); the second passes already (it pins today's output).

- [ ] **Step 3: Emit the variables** at the end of `palette_css`, before the `scrim` block:

```rust
    put_rgb(&mut s, &["--selection"], p.selection);
    put_rgb(&mut s, &["--error"], p.error);
    put_rgb(&mut s, &["--error-fill"], p.error_fill);
    put_rgb(&mut s, &["--on-error-fill"], p.on_error_fill);
```

- [ ] **Step 4: Read them in `mobiler.css`.** Replace these rules (keep everything else on each line):

```css
.progress-bar { … background: var(--selection, var(--pal-primary, var(--accent, #5C6BC0))); … }
.field-invalid { border-color: var(--error, var(--danger, #d33)); }
.field-error { font-size: 12px; color: var(--error, var(--danger, #d33)); padding-left: 2px; }
.check input { width: 18px; height: 18px; accent-color: var(--selection, var(--primary)); }
.toggle input:checked { background: var(--selection, var(--primary)); }
.slider { width: 100%; accent-color: var(--selection, var(--primary)); }
```

  and add after the `.btn-info` / `.theme-dark .btn-info` tone lines (same specificity, so later wins):

```css
/* Palette error roles (unset: the danger tone above, as before). */
.btn-filled.btn-danger { background: var(--error-fill, var(--tone)); color: var(--on-error-fill, var(--tone-fill-ink, #fff)); }
.theme-dark .btn-filled.btn-danger { color: var(--on-error-fill, var(--tone-fill-ink, #15171f)); }
.btn-outlined.btn-danger { color: var(--error, var(--tone)); box-shadow: inset 0 0 0 1px var(--error, var(--tone)); }
.btn-text.btn-danger { color: var(--error, var(--tone)); }
/* Focus marks only when the palette sets `selection`: the theme's inline style then carries the
   variable. Without it the browser's default focus ring stays, as before. */
[style*="--selection:"] .field:focus { border-color: var(--selection); outline: 2px solid var(--selection); outline-offset: -1px; }
[style*="--selection:"] .searchfield:focus-within { border-color: var(--selection); }
```

  Before relying on the attribute selector, confirm in `lib.rs` (~line 2450, and the overlay copies at ~1327,
  ~1514, `sync_overlay_theme` ~2627) that the theme CSS is set as the `style` attribute of an element that
  *contains* the fields (the app root / overlay root). If it is set on a different element, put the rule on that
  element's class instead and say so in the commit.

- [ ] **Step 5: Run** `cargo test -p mobiler-web` → all pass, including `theme_css_without_palette_is_unchanged`.

- [ ] **Step 6: Visual check.** Build `demos/barbershop/web` (`RUSTUP_TOOLCHAIN=stable trunk build`, its own
  `CARGO_TARGET_DIR`), serve `dist/`, headless-Chrome screenshot (recipe in `.claude/skills/post-release`).
  Also screenshot `demos/coffee/web` before (on `main`) and after: they must be pixel-identical (no palette).

- [ ] **Step 7: Commit** `feat(web): selection and error palette roles`.

### Task 3: Android barbershop shell reads the new roles

**Files:**
- Modify: `demos/barbershop/Android/app/src/main/java/dev/mobiler/barbershop/ui/theme/Theme.kt` (`paletteScheme`, line ~68)
- Modify: `demos/barbershop/Android/app/src/main/java/dev/mobiler/barbershop/MainActivity.kt` (Progress ~635; LazyList ~1262/1277; Toggle/Checkbox/Slider ~1360–1379; Scaffold pull-to-refresh ~1583; `toneStrong` ~1779; `paletteFieldColors` ~1803; `MobilerButton` ~1819–1866; Pager spinner ~2168)

**Interfaces:**
- Consumes: Task 1's fields via `LocalPalette.current` (`ColorRoles?`).
- Produces: `@Composable private fun selectionColor(): Color?`, `@Composable private fun toneLine(tone: Tone): Color`.

- [ ] **Step 1: M3 `error` slot** in `paletteScheme`:

```kotlin
        error = p.error?.color() ?: p.danger?.onContainer?.color() ?: base.error,
```

  (field error text, its border and the destructive confirm button all read `colorScheme.error`).

- [ ] **Step 2: Selection helper + field colours.** Add near `paletteFieldColors`, and replace
  `paletteFieldColors`:

```kotlin
/** The palette's `selection` mark colour; null = M3's default (primary) at each site, as before. */
@Composable
private fun selectionColor(): Color? = LocalPalette.current?.selection?.color()

// Text-field colours from the palette: `surface_muted` fill, `selection` focus border, label, cursor and
// text-selection handles. Any unset role keeps the M3 default.
@Composable
private fun paletteFieldColors(): TextFieldColors {
    val pal = LocalPalette.current ?: return OutlinedTextFieldDefaults.colors()
    val d = OutlinedTextFieldDefaults.colors()
    val muted = pal.surfaceMuted?.color()
    val sel = pal.selection?.color()
    return OutlinedTextFieldDefaults.colors(
        focusedContainerColor = muted ?: d.focusedContainerColor,
        unfocusedContainerColor = muted ?: d.unfocusedContainerColor,
        focusedBorderColor = sel ?: d.focusedIndicatorColor,
        focusedLabelColor = sel ?: d.focusedLabelColor,
        cursorColor = sel ?: d.cursorColor,
        selectionColors = sel?.let { TextSelectionColors(handleColor = it, backgroundColor = it.copy(alpha = 0.4f)) } ?: d.textSelectionColors,
    )
}
```

  Import `androidx.compose.foundation.text.selection.TextSelectionColors`. If a `TextFieldColors` property name
  differs in this material3 version, the compiler names it; use the matching property.

- [ ] **Step 3: Marks.** At each site, pass the colour only when set:

```kotlin
// Toggle
Switch(checked = widget.value, onCheckedChange = { … },
    colors = selectionColor()?.let { SwitchDefaults.colors(checkedTrackColor = it) } ?: SwitchDefaults.colors())
// Checkbox
Checkbox(checked = widget.value, onCheckedChange = { … },
    colors = selectionColor()?.let { CheckboxDefaults.colors(checkedColor = it) } ?: CheckboxDefaults.colors())
// Slider
Slider(…, colors = selectionColor()?.let { SliderDefaults.colors(thumbColor = it, activeTrackColor = it) } ?: SliderDefaults.colors())
// Progress (both branches, ~635/637) and the LazyList / Pager loaders (~1262, ~1558, ~2168)
LinearProgressIndicator(…, color = selectionColor() ?: ProgressIndicatorDefaults.linearColor)
CircularProgressIndicator(…, color = selectionColor() ?: ProgressIndicatorDefaults.circularColor)
```

  Both `PullToRefreshBox` sites (~1277, ~1583) get an explicit state and indicator so the spinner can take the
  colour:

```kotlin
val ptr = rememberPullToRefreshState()
PullToRefreshBox(
    isRefreshing = …, onRefresh = …, modifier = …, state = ptr,
    indicator = {
        PullToRefreshDefaults.Indicator(
            state = ptr, isRefreshing = …, modifier = Modifier.align(Alignment.TopCenter),
            color = selectionColor() ?: PullToRefreshDefaults.indicatorColor,
        )
    },
) { … }
```

  If this material3 version names the default colour differently, use its default (the compiler shows the
  parameter list); unset must equal today's spinner.

- [ ] **Step 4: Danger buttons.** Rename today's `toneStrong` body into `toneStrongBase` and wrap it; add
  `toneLine`:

```kotlin
// Strong (filled) color pair for a toned button. A palette's error_fill / on_error_fill replace a filled
// Danger button's colours, each falling back to today's.
@Composable
private fun toneStrong(tone: Tone): Pair<Color, Color> {
    val (fill, onFill) = toneStrongBase(tone)
    val pal = LocalPalette.current
    if (tone != Tone.DANGER || pal == null) return fill to onFill
    return (pal.errorFill?.color() ?: fill) to (pal.onErrorFill?.color() ?: onFill)
}

// Outlined/text toned buttons' colour: a palette's `error` for Danger, else today's strong colour.
@Composable
private fun toneLine(tone: Tone): Color =
    (if (tone == Tone.DANGER) LocalPalette.current?.error?.color() else null) ?: toneStrongBase(tone).first
```

  In `MobilerButton` add `val line = toneLine(widget.tone)` and use `line` instead of `strong` in the toned
  OUTLINED branch (`contentColor` and `BorderStroke`) and the toned TEXT branch. FILLED keeps `strong`/`onStrong`.

- [ ] **Step 5: Build** `cd demos/barbershop && ./build-android.sh` (or the gradle `:app:assembleDebug` it runs)
  with `JAVA_HOME=~/jdk21` and `CARGO_TARGET_DIR=demos/barbershop/target` → BUILD SUCCESSFUL.

- [ ] **Step 6: Commit** `feat(android): selection and error palette roles (barbershop)`.

### Task 4: iOS barbershop shell reads the new roles

**Files:**
- Modify: `demos/barbershop/iOS/Sources/Render.swift` (progress ~120; field error ~299; search field ~304; toggle ~341; checkbox ~352; slider ~362; `PaletteFieldStyle` ~520–545; `MobilerButton.paletteColors` ~1685)

**Interfaces:**
- Consumes: Task 1's fields (`selection`, `error`, `errorFill`, `onErrorFill` on `ColorRoles`).
- Produces: `extension View { func selectionTint() -> some View }`.

- [ ] **Step 1: Tint helper**, next to `role(_:else:)`:

```swift
extension View {
    /// The palette's `selection` as this control's tint; unset keeps the inherited tint (today's colour).
    @ViewBuilder func selectionTint() -> some View {
        if let s = pal?.selection { self.tint(s.color) } else { self }
    }
}
```

- [ ] **Step 2: Marks.** Add `.selectionTint()` to the determinate `ProgressView(value:)`, the `Toggle`, the
  `Slider`, and the `TextField`s of `.textField` and `.searchField` (cursor). Checkbox:
  `.foregroundColor(value ? role(pal?.selection, else: .accentColor) : .secondary)`.

- [ ] **Step 3: Focus border in `PaletteFieldStyle`.** Add `@FocusState private var focused: Bool` and apply
  `.focused($focused)` to `content` in both branches. Plain branch stroke:
  `shape.stroke(focused && pal?.selection != nil ? role(pal?.selection, else: .clear) : role(pal?.outline, else: pal?.surfaceMuted != nil ? .clear : Color.gray.opacity(0.3)))`.
  `.roundedBorder` branch (Review Focus 1):

```swift
content.textFieldStyle(.roundedBorder)
    .focused($focused)
    .overlay(RoundedRectangle(cornerRadius: 6).stroke(role(pal?.selection, else: .clear), lineWidth: 1)
        .opacity(focused && pal?.selection != nil ? 1 : 0))
```

- [ ] **Step 4: Errors.** Field error text: `.foregroundColor(role(pal?.error, else: .red))`. Rename
  `paletteColors()` to `pairColors()` and add:

```swift
    /// `pairColors()`, with a palette's error roles over a Danger button (each falls back to today's).
    private func paletteColors() -> (fill: Color, fg: Color, stroke: Color)? {
        let base = pairColors()
        guard tone == .danger, let p = pal else { return base }
        switch style {
        case .filled:
            guard p.errorFill != nil || p.onErrorFill != nil else { return base }
            return (role(p.errorFill, else: base?.fill ?? .red), role(p.onErrorFill, else: base?.fg ?? .white), .clear)
        case .outlined:
            guard let e = p.error else { return base }
            return (.clear, e.color, e.color)
        case .text:
            guard let e = p.error else { return base }
            return (.clear, e.color, .clear)
        case .tonal:
            return base
        }
    }
```

- [ ] **Step 5: Re-render check (Review Focus 2).** For each new read, confirm it is either inside `render(…)`
  (rebuilt on every view change, like the existing `role(pal?.surfaceMuted, …)` in `.searchField`) or inside a
  view that declares `@Environment(\.paletteRoles)` (`PaletteFieldStyle` does; `MobilerButton` — check, and add
  `@Environment(\.paletteRoles) private var paletteRoles` + `let _ = paletteRoles` in `body` if missing).

- [ ] **Step 6: Typecheck.** No simulator on this host: rely on the PR's iOS CI lanes. Before pushing, check the
  Swift by eye against the generated names in `demos/barbershop/iOS/generated/` (or the SharedTypes output) for
  `errorFill` / `onErrorFill`.

- [ ] **Step 7: Commit** `feat(ios): selection and error palette roles (barbershop)`.

### Task 5: Android barbershop launch window (seed resources + night themes)

**Files:**
- Create: `demos/barbershop/Android/app/src/main/res/values/mobiler_splash.xml`
- Create: `demos/barbershop/Android/app/src/main/res/values-night/mobiler_splash.xml`
- Create: `demos/barbershop/Android/app/src/main/res/drawable/mobiler_launch.xml`
- Modify: `demos/barbershop/Android/app/src/main/res/values/themes.xml`
- Create: `demos/barbershop/Android/app/src/main/res/values-night/themes.xml`
- Create: `demos/barbershop/Android/app/src/main/res/values-v31/themes.xml`
- Create: `demos/barbershop/Android/app/src/main/res/values-night-v31/themes.xml`

**Interfaces:**
- Produces (the `[splash]` contract for the next release): `@color/mobiler_splash_background` (light and
  night), `@drawable/mobiler_splash_icon`, `@drawable/mobiler_launch`.

- [ ] **Step 1: Seed resources.** Barbershop is an app, so its seed values are its own palette's backgrounds
  (`moj_termin_palette()` `light.background` / `dark.background`; read the hex there). For the *template* (Task
  11) the defaults are the M3 scheme backgrounds; see Task 11 Step 2.

```xml
<!-- values/mobiler_splash.xml -->
<?xml version="1.0" encoding="utf-8"?>
<!-- The launch window and splash. App-owned: `mobiler upgrade` creates this file when missing and never
     changes it. Edit the colours to your design's backgrounds; a later mobiler release can write them from
     mobiler.toml [splash]. -->
<resources>
    <color name="mobiler_splash_background">#LIGHT_BACKGROUND</color>
    <drawable name="mobiler_splash_icon">@mipmap/ic_launcher</drawable>
</resources>
```

```xml
<!-- values-night/mobiler_splash.xml -->
<?xml version="1.0" encoding="utf-8"?>
<!-- The launch window and splash in dark mode. App-owned, like values/mobiler_splash.xml. -->
<resources>
    <color name="mobiler_splash_background">#DARK_BACKGROUND</color>
</resources>
```

```xml
<!-- drawable/mobiler_launch.xml -->
<?xml version="1.0" encoding="utf-8"?>
<!-- The window behind the app until its first frame. App-owned, like values/mobiler_splash.xml. -->
<layer-list xmlns:android="http://schemas.android.com/apk/res/android">
    <item android:drawable="@color/mobiler_splash_background" />
</layer-list>
```

  (`#LIGHT_BACKGROUND` / `#DARK_BACKGROUND` are replaced by the real hex values here, e.g. `#FFF7F4F2`; no
  placeholder may remain in a committed file.)

- [ ] **Step 2: Themes.** `values/themes.xml`:

```xml
<?xml version="1.0" encoding="utf-8"?>
<resources>
    <style name="Theme.Fadehouse" parent="android:Theme.Material.Light.NoActionBar">
        <item name="android:windowBackground">@drawable/mobiler_launch</item>
    </style>
</resources>
```

  `values-night/themes.xml`: identical with `parent="android:Theme.Material.NoActionBar"`.
  `values-v31/themes.xml` (light parent) and `values-night-v31/themes.xml` (dark parent):

```xml
<?xml version="1.0" encoding="utf-8"?>
<resources>
    <!-- Android 12+ system splash. Its background is set explicitly: the splash derives it from
         windowBackground only when that is a plain colour, and mobiler_launch is a layer-list. -->
    <style name="Theme.Fadehouse" parent="android:Theme.Material.Light.NoActionBar">
        <item name="android:windowBackground">@drawable/mobiler_launch</item>
        <item name="android:windowSplashScreenBackground">@color/mobiler_splash_background</item>
        <item name="android:windowSplashScreenAnimatedIcon">@drawable/mobiler_splash_icon</item>
    </style>
</resources>
```

- [ ] **Step 3: Build** barbershop Android as in Task 3 Step 5 → BUILD SUCCESSFUL (an `aapt2` error here means a
  resource name or attribute is wrong; fix before going on).

- [ ] **Step 4: Commit** `feat(android): dark launch window and splash from seed resources (barbershop)`.

### Task 6: iOS barbershop launch colour

**Files:**
- Create: `demos/barbershop/iOS/Sources/Assets.xcassets/MobilerSplashBackground.colorset/Contents.json`
- Modify: `demos/barbershop/iOS/project.yml` (`UILaunchScreen: {}`)

- [ ] **Step 1: Colour set** (barbershop: its palette backgrounds; the template's defaults in Task 11 are white
  and black):

```json
{
  "colors" : [
    { "idiom" : "universal", "color" : { "color-space" : "srgb", "components" : { "red" : "0xF7", "green" : "0xF4", "blue" : "0xF2", "alpha" : "1.000" } } },
    { "idiom" : "universal", "appearances" : [ { "appearance" : "luminosity", "value" : "dark" } ],
      "color" : { "color-space" : "srgb", "components" : { "red" : "0x23", "green" : "0x1F", "blue" : "0x20", "alpha" : "1.000" } } }
  ],
  "info" : { "author" : "xcode", "version" : 1 }
}
```

  (Use barbershop's real palette backgrounds; the values above are examples of the format.)

- [ ] **Step 2: project.yml:** replace `UILaunchScreen: {}` with

```yaml
        UILaunchScreen:
          UIColorName: MobilerSplashBackground
```

- [ ] **Step 3: Commit** `feat(ios): launch screen colour from a seed colour set (barbershop)`. The iOS CI lane
  builds it in Task 9.

### Task 7: first-frame guidance and ADR-0041

**Files:**
- Modify: `mobiler-core/src/lib.rs` (the `with_appearance` doc comment; find it with `grep -n "fn with_appearance"`)
- Modify: `mobiler-core/README.md` (the appearance section; and the Upgrading section)
- Create: `docs/adr/ADR-0041-shell-never-sets-app-night-mode.md`; Modify: `docs/adr/index.md`
- Modify: `xtask/tests/adr_conformance.rs`

- [ ] **Step 1: Doc comment** on `with_appearance`, appended:

```rust
/// To open in the app's own choice from the first frame, keep that choice in the app's `cx.save` state
/// (it is not a secret): `Restore` arrives before `Start`, so the first render already carries it. A value
/// read in `init` from `securestore` or `kv` arrives after the first frame. The Android system splash still
/// follows the OS appearance (ADR-0041).
```

- [ ] **Step 2: README.** In `mobiler-core/README.md`'s appearance section, the same guidance in two sentences.
  In Upgrading: "0.41: `ColorRoles` gains `selection`, `error`, `error_fill`, `on_error_fill`. A full
  `ColorRoles { … }` literal needs `..Default::default()`."

- [ ] **Step 3: Failing conformance test** (append to `xtask/tests/adr_conformance.rs`):

```rust
/// ADR-0041: no shell sets an app-level night mode, so the configuration the shell reads the OS
/// appearance from stays the OS's. Checked over every Kotlin source of the template and the demos.
#[test]
fn adr_0041_no_shell_sets_an_app_level_night_mode() {
    let mut stack = vec![root().join("mobiler/templates/Android"), root().join("demos")];
    let mut checked = 0;
    while let Some(dir) = stack.pop() {
        for entry in std::fs::read_dir(&dir).unwrap_or_else(|e| panic!("{}: {e}", dir.display())).flatten() {
            let path = entry.path();
            if path.is_dir() && !["build", "target", "node_modules", ".gradle"].iter().any(|s| path.ends_with(s)) {
                stack.push(path);
            } else if path.extension().is_some_and(|e| e == "kt") {
                checked += 1;
                let kt = std::fs::read_to_string(&path).unwrap();
                for (i, line) in kt.lines().enumerate() {
                    let code = line.split("//").next().unwrap_or("");
                    for call in ["setApplicationNightMode", "setDefaultNightMode"] {
                        assert!(!code.contains(call), "ADR-0041: {}:{} calls {call}", path.strip_prefix(root()).unwrap().display(), i + 1);
                    }
                }
            }
        }
    }
    assert!(checked > 20, "ADR-0041: expected the shells' Kotlin sources, checked {checked}");
}
```

- [ ] **Step 4: Run** `cargo test -p xtask --test adr_conformance adr_0041` → PASS (nothing calls it today).
  Then the **mutation proof**: add `(getSystemService(UI_MODE_SERVICE) as android.app.UiModeManager).setApplicationNightMode(android.app.UiModeManager.MODE_NIGHT_YES)`
  as a line in the template's `MainActivity.onCreate`, run → FAIL naming the file and line; revert; run → PASS.
  A second mutation: the same call with a trailing `// comment` still fails (comment stripping only removes the
  comment).

- [ ] **Step 5: Write ADR-0041** from `docs/adr/TEMPLATE.md`. Claim: "The shells never set an app-level night
  mode (`UiModeManager.setApplicationNightMode`, `AppCompatDelegate.setDefaultNightMode`); the Android system
  splash follows the OS appearance, and an app that wants its own choice in the first frame keeps it in `cx.save`
  state". Context: the Moj Termin request (2026-10-01, quote item 3 and the acceptance check "An app set to Dark on
  a light phone opens dark from the first frame"). Options: A `setApplicationNightMode` on the app's choice
  `[reconstructed]` (rejected: rewrites the configuration `Core.kt` reads the OS appearance from, so the System
  query and stream of ADR-0020 report the app's choice; also recreates the activity); B a shell-side "last
  appearance" hint `[recorded: Moj Termin request, 2026-10-01, quoted here]` (their preferred option; rejected: on
  Android the only pre-first-frame surface is the system launch window, which a running app cannot paint); C the
  app keeps its choice in `cx.save` state, shells unchanged (chosen). §4 Mutation proof from Step 4. §5 negative:
  an app forcing Dark on a light Android phone still shows a light system splash; iOS's launch screen likewise
  follows the OS. Conformance: `xtask/tests/adr_conformance.rs::adr_0041_no_shell_sets_an_app_level_night_mode`.
  Deciding PRs: PR A's number (fill in after `gh pr create`, before merge). Add the index row.

- [ ] **Step 6: Run** `cargo test -p xtask` → all pass (the ADR lint checks the header, the index and the cited
  test). **Commit** `docs: first-frame appearance guidance; ADR-0041`.

### Task 8: runtime verification (Android emulator)

No code. Delegate to a subagent with the build and emulator rules from Global Constraints.

- [ ] **Step 1: Dark cold start.** Install barbershop debug on `mobiler_pixel7` (port 5560, API 36).
  `adb -s emulator-5560 shell cmd uimode night yes`, force-stop, start the app, and take screenshots
  immediately and every ~150 ms for 1.5 s (`screencap` in a loop). No frame may be white; the splash shows the
  dark `mobiler_splash_background`.
- [ ] **Step 2: API 26.** If an API 26 system image is installed (`sdkmanager --list_installed`), repeat Step 1
  on it (no system splash; the launch window must be dark). If none is installed, say so in the PR; don't install
  one without asking.
- [ ] **Step 3: Focus contrast.** In dark, focus a text field; screenshot; sample the border pixel and the field
  fill pixel; compute the WCAG contrast ratio. Expect ≥ 3:1 with `selection = #4fb3a4`.
- [ ] **Step 4: Errors.** Trigger a field error and show a filled Danger button (use or temporarily add a demo
  screen); sample: error text = `#ff8f85` (dark) / `#b3261e` (light); filled Danger = `#c0392b` (dark) /
  `#b3261e` (light) with white text. Revert any temporary demo change.
- [ ] **Step 5: No-palette unchanged.** Coffee Android (no palette): screenshot of a form screen on `main` and
  on the branch; pixel-identical apart from the status bar clock.
- [ ] **Step 6:** Shut the emulator down (`adb -s emulator-5560 emu kill`); record results in the PR body.

### Task 9: ship PR A, then publish the libraries

- [ ] **Step 1:** `superpowers:requesting-code-review` on the whole branch (three shells side by side: the
  cross-shell-divergence lesson). Fix findings.
- [ ] **Step 2:** `ship-pr` skill. Title `feat: selection and error palette roles; dark launch window
  (barbershop); ADR-0041`. Body: the request's substance (the request doc is deleted later), the runtime results
  from Task 8, and "BREAKING (ADR-0008): ColorRoles gains four fields; full literals need ..Default::default()".
  Fill ADR-0041's `Deciding PRs:` with the PR number before merging. All CI lanes green before merge.
- [ ] **Step 3:** `release-libs` skill: ui 0.30.0 → core 0.41.0 → web 0.41.0. Irreversible: ask the maintainer
  first; give them the commands if auto mode blocks.

## PR B: CLI (after the libraries are live)

New branch from the updated `main`: `feat/cli-dark-theme-gaps`.

### Task 10: upgrade "seed" class and ADR-0042

**Files:**
- Modify: `mobiler/src/upgrade.rs` (`enum Class` ~49; `classify` ~59; `sync_file` ~209; `seed_dir` ~395; tests)
- Create: `docs/adr/ADR-0042-upgrade-seed-files.md`; Modify: `docs/adr/index.md`

**Interfaces:**
- Produces: `Class::Seed`; `const SEED_PATHS: &[&str]` (app-relative paths, after templating).

- [ ] **Step 1: Failing tests** in `upgrade.rs`'s test module:

```rust
    #[test]
    fn seed_paths_classify_as_seed_and_exist_in_the_template() {
        for p in SEED_PATHS {
            assert_eq!(classify(Path::new(p), b"x"), Class::Seed, "{p}");
        }
        // The rest of the asset catalog stays the app's.
        assert_eq!(classify(Path::new("iOS/Sources/Assets.xcassets/AppIcon.appiconset/Contents.json"), b"{}"), Class::Own);
        // Every seed path is shipped by the template, so the list can't rot.
        for p in SEED_PATHS {
            assert!(TEMPLATES.get_file(p).is_some(), "seed path {p} is not in the template");
        }
    }

    #[test]
    fn missing_seed_files_are_created_and_existing_ones_never_touched() {
        let root = skeleton();
        upgrade_at(&root, false).unwrap();
        for p in SEED_PATHS {
            assert!(root.join(p).exists(), "{p} created");
        }
        let edited = "<resources><color name=\"mobiler_splash_background\">#123456</color></resources>\n";
        fs::write(root.join(SEED_PATHS[0]), edited).unwrap();
        upgrade_at(&root, true).unwrap();
        assert_eq!(read(&root, SEED_PATHS[0]), edited, "seed file left byte-identical");
        assert!(!root.join(format!("{}.mobiler-new", SEED_PATHS[0])).exists());
        assert!(!root.join(".mobiler/base").join(SEED_PATHS[0]).exists(), "no baseline for a seed file");
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn seed_files_created_even_when_theme_is_offered_as_new() {
        let root = skeleton();
        let theme = "Android/app/src/main/res/values/themes.xml";
        fs::create_dir_all(root.join(theme).parent().unwrap()).unwrap();
        fs::write(root.join(theme), "<resources><!-- mine --></resources>\n").unwrap();
        upgrade_at(&root, false).unwrap();
        assert_eq!(read(&root, theme), "<resources><!-- mine --></resources>\n");
        assert!(root.join(format!("{theme}.mobiler-new")).exists());
        for p in SEED_PATHS {
            assert!(root.join(p).exists(), "{p} created although the theme was not applied");
        }
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn existing_night_theme_is_not_overwritten() {
        let root = skeleton();
        let night = "Android/app/src/main/res/values-night/themes.xml";
        fs::create_dir_all(root.join(night).parent().unwrap()).unwrap();
        fs::write(root.join(night), "<resources><!-- my night --></resources>\n").unwrap();
        upgrade_at(&root, false).unwrap();
        assert_eq!(read(&root, night), "<resources><!-- my night --></resources>\n");
        let _ = fs::remove_dir_all(&root);
    }
```

  Adjust the names `TEMPLATES` / `skeleton` / `upgrade_at` / `read` to the module's actual helpers (they exist:
  `skeleton()`, `upgrade_at(&root, apply)`, `read(&root, rel)`; find the embedded template `Dir` static with
  `grep -n "include_dir!" mobiler/src/*.rs`). If `get_file` needs the path relative to a sub-dir, adapt.

- [ ] **Step 2: Run** `cargo test -p mobiler seed` → compile FAIL (`Class::Seed`, `SEED_PATHS` undefined). Note:
  the templates gain the seed files only in Task 11; run Task 11 Step 1–3 first if you prefer, or expect the
  "not in the template" assertion to fail until then.

- [ ] **Step 3: Implement.**

```rust
    /// App-owned values the CLI ships a default for (the launch window colours): created when missing,
    /// never read, merged or overwritten after that, and never baselined (ADR-0042).
    Seed,
```

```rust
/// Seed files (ADR-0042): the launch-window values a later `[splash]` sync writes, app-owned meanwhile.
const SEED_PATHS: &[&str] = &[
    "Android/app/src/main/res/values/mobiler_splash.xml",
    "Android/app/src/main/res/values-night/mobiler_splash.xml",
    "Android/app/src/main/res/drawable/mobiler_launch.xml",
    "iOS/Sources/Assets.xcassets/MobilerSplashBackground.colorset/Contents.json",
];
```

  At the top of `classify`, after computing `p`: `if SEED_PATHS.contains(&p.as_str()) { return Class::Seed; }`
  (before the `own` check, so it wins over the `Assets.xcassets/` prefix). In `sync_file`, after `let class = …`:

```rust
    if class == Class::Seed {
        let dst = root.join(&rel);
        if !dst.exists() {
            if let Some(parent) = dst.parent() {
                fs::create_dir_all(parent).with_context(|| format!("creating {}", parent.display()))?;
            }
            fs::write(&dst, &desired).with_context(|| format!("writing {}", dst.display()))?;
            report.added.push(rel.to_string_lossy().to_string());
        }
        return Ok(());
    }
```

  In `seed_dir`, skip `Class::Seed` like `Class::Own` (no baseline). Update the `classify` doc comment.

- [ ] **Step 4: Run** `cargo test -p mobiler` → all pass (after Task 11's template files exist).
  **Mutation proof:** (a) remove the `Seed` early return in `classify` → `seed_paths_classify_as_seed…` and
  `…never_touched` fail; (b) in `sync_file` write the seed file even when it exists → `…never_touched` fails;
  revert → pass. Record both in ADR-0042 §4.

- [ ] **Step 5: ADR-0042.** Claim: "`mobiler upgrade` has a seed class: a listed app-owned file the template
  provides is created when missing and never touched after that". Context: `Own` files are skipped even when
  missing, so a shell file referencing a new app-owned resource breaks every upgraded app; the next release's
  `[splash]` sync needs files it can write that upgrade never fights. Options: A make them `Own` `[reconstructed]`
  (rejected: never reach existing apps → build break); B make them `Shell` `[reconstructed]` (rejected: an app's
  or the sync's values meet three-way merges and `.mobiler-new` conflicts whenever a default changes); C `Seed`
  (chosen). Supersedes nothing; it extends ADR-0012 (say so in Context). Conformance: the two `upgrade.rs` tests
  (format `mobiler/src/upgrade.rs::seed_paths_classify_as_seed_and_exist_in_the_template`, …). Index row.

- [ ] **Step 6:** `cargo test -p xtask` → pass. **Commit** `feat(cli): upgrade seed files (ADR-0042)`.

### Task 11: port barbershop's shells to the template

**Files:**
- Modify: `mobiler/templates/Android/app/src/main/java/__PACKAGE_PATH__/MainActivity.kt`, `…/ui/theme/Theme.kt`
- Modify: `mobiler/templates/iOS/Sources/Render.swift`, `mobiler/templates/iOS/project.yml`
- Modify: `mobiler/templates/Android/app/src/main/res/values/themes.xml`
- Create: the template twins of every file in Tasks 5–6 (`values-night/`, `values-v31/`, `values-night-v31/`
  themes; the three Android seed files; the iOS colour set)
- Modify: the template's `shared/Cargo.toml` pin `mobiler-core = "0.41"` (find it: `grep -rn "mobiler-core" mobiler/templates`)
- Modify: `mobiler/README.md` (new "Launch window colours" section)

- [ ] **Step 1: Port code by patch.** `git diff main -- demos/barbershop/Android/app/src/main/java demos/barbershop/iOS/Sources/Render.swift`
  and apply the same hunks to the template files (they differ only in package/imports; `{{NAME}}` placeholders
  stay). `Render.swift` is byte-identical between them, so copy it and re-diff to confirm.
- [ ] **Step 2: Template resources.** Same XML/JSON as Tasks 5–6, with the style name `Theme.{{NAME}}` and these
  seed defaults: Android light/dark `mobiler_splash_background` = the default M3 `lightColorScheme().background` /
  `darkColorScheme().background` for the template's material3 version (find the version in
  `mobiler/templates/Android/app/build.gradle.kts` or its version catalog; 1.2.0 and later: `#FFFEF7FF` /
  `#FF141218`; 1.1.x: `#FFFFFBFE` / `#FF1C1B1F`; confirm by reading `ColorLightTokens.Background` /
  `ColorDarkTokens.Background` in the material3 sources jar under `~/.gradle/caches`). iOS colour set: white /
  black (today's system background).
- [ ] **Step 3: CLI README** "Launch window colours": which files hold the light and dark background on Android
  (`res/values/mobiler_splash.xml`, `res/values-night/mobiler_splash.xml`) and iOS (the
  `MobilerSplashBackground` colour set), that upgrades create them when missing and never change them, and that a
  later release will write them from `mobiler.toml`.
- [ ] **Step 4: Verify.** `cargo test --workspace`; `cargo test -p xtask` (ADR-0040's and ADR-0041's checks scan
  the template). Scaffold with the workspace CLI into the scratchpad (`cargo run -p mobiler -- new smoke
  --package dev.test.smoke`), confirm the seed files exist, `mobiler build android` → APK. Delete the scratch app.
- [ ] **Step 5: Commit** `feat(cli): template ported — selection/error roles, dark launch window, seed files`.

### Task 12: ship PR B, release the CLI, post-release

- [ ] **Step 1:** `ship-pr` (all native lanes, including `scaffold + build (template, Android)` and the iOS
  lanes). PR body: what an app gets on upgrade, and the Moj Termin note (keep the appearance choice in `cx.save`;
  set the two seed colours).
- [ ] **Step 2:** CLI version bump PR (minor: 0.61.0), then `release-cli`. Irreversible: ask first.
- [ ] **Step 3:** `post-release`, including the upgrade smoke: an app scaffolded with CLI 0.60.2 upgrades with
  `--apply`, gains the seed files, keeps a hand edit to `values/mobiler_splash.xml` on a second upgrade, builds.
- [ ] **Step 4:** Delete `docs/dark-theme-gaps.md` (the request doc; never committed). Update `start.md` and
  memory.
