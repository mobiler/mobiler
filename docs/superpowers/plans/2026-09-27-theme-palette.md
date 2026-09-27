# Theme Palette Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** `Theme.palette` (light + dark `ColorRoles`) recolours every themed widget on web, Android
and iOS. Unset roles keep today's colours, and `palette: None` looks exactly as before.

**Architecture:**
- New ABI types (`Palette`, `ColorRoles`, `TonePair`, `Rgba`) sit in mobiler-ui, with
  `Theme.palette` as the last field.
- Each shell resolves the active set from `dark_mode` and reads each role with its current colour
  as the fallback:
  - web: CSS variables with `var(--x, <old>)` fallbacks
  - Android: a palette-built M3 scheme plus a `LocalPalette` CompositionLocal
  - iOS: `ActivePalette` plus a `role(…, else:)` helper
- Native shell work happens in **barbershop's** shells, which are near-identical to the templates.
  The templates are ported in the design release's final CLI PR, so this branch never touches
  `mobiler/templates/`.

**Tech Stack:** Rust (facet typegen), Leptos/WASM + CSS, Kotlin/Compose M3, SwiftUI.

**Spec:** `docs/superpowers/specs/2026-09-27-theme-palette-design.md`

## Global Constraints

- **`palette: None` must render identically to 0.39 on every shell.** Every change is either
  gated on the palette being set or reads a variable whose fallback is today's exact value.
- A role left `None` inside a set falls back to the shell's current colour, never black or
  transparent.
- The active set is `if dark_mode { dark } else { light }`.
- **Filled toned buttons:** `on_container` fill with `container` text. Every other tone use:
  `container` background with `on_container` foreground.
- **Fixed colours:** chart series palette, RegionChart, `ProjectColor`, map marker and the `Box`
  image scrim. They are never palette-driven.
- **Breaking-change note text** (verbatim, in the `Theme` doc comment and the READMEs):
  > **Breaking in mobiler-ui 0.29 / mobiler-core 0.40:** `Theme` gained `palette` (and further
  > design-release fields follow). Code that lists every field in a `Theme { … }` literal no longer
  > compiles. Write `Theme { seed, ..Default::default() }` and set only what you need.
- **Do not bump crate versions and do not publish.** The design set ships as one release, later.
- **Do not edit `mobiler/templates/`** on this branch.
- **Build rules:**
  - Use a per-demo `CARGO_TARGET_DIR=$PWD/target` for demo builds, and
    `JAVA_HOME=~/jdk21 ANDROID_HOME=/home/zmilan/Android/Sdk`.
  - The CLI binary is `/media/zmilan/data2/cargo-target/debug/mobiler` (build it with
    `cargo build -p mobiler`).
  - Run long builds with `run_in_background`, never with `pgrep`/marker polling.
- **Swift:** a static touched from closures uses `nonisolated(unsafe)`. Swift cannot be compiled
  locally; the macOS CI lanes are the compile check.
- **Web palette test values:** the Moj Termin set from `docs/theme-palette-tokens.md` §3.

## Review Focus

1. **Partial set.** A palette with only `background` set must leave every other widget at today's
   colour.
   - Pinned by the Task 2 test `palette_css_emits_only_set_roles`.
   - Pinned by the Task 5 light-mode check: barbershop light leaves `on_primary`, `fab`,
     `on_fab` and `scrim` unset.
2. **`palette: None` must be byte-identical on web.** `theme_css` output must not change for a
   theme without a palette.
   - Pinned by the Task 2 test `theme_css_without_palette_is_unchanged`.
   - Plus the Task 5 before/after screenshots.
3. **Dark flips must swap sets.** Toggling `dark_mode` with a palette must switch every role,
   including the web page background behind the scaffold and Android system-bar icons.
   - Pinned by the Task 5 dark↔light toggle check on both shells.
4. **Dialogs and portals must pick up the palette.** The web confirm modal, Android AlertDialog,
   ModalBottomSheet and iOS sheet are drawn outside the normal tree.
   - Pinned by the Task 5 check that opens a sheet and a confirm with the palette.
5. **Readable filled toned buttons.** On dark, a filled Danger button is `#ffb4ab` fill with
   `#4d2626` text; on light, `#93000a` with `#ffdad6`.
   - Pinned by the Task 5 sampled colours on barbershop's danger button.

---

### Task 0: Baselines (before any code change)

**Files:** none (the output goes to the scratchpad `baseline/`).

- [ ] **Step 1: Web baselines.**
  - `demos/coffee/web` and `demos/todo/web`: run `CARGO_TARGET_DIR=$PWD/target trunk build`,
    serve `dist/` on 8801/8802, and take headless Chrome screenshots at 430×1000 into
    `baseline/web-coffee.png` and `baseline/web-todo.png`.
  - Command:
    `google-chrome --headless=new --disable-gpu --no-sandbox --hide-scrollbars --window-size=430,1000 --virtual-time-budget=8000 --screenshot=<png> http://127.0.0.1:<port>/`
  - Also capture the dark variant if the demo has a dark toggle reachable by default. If not, one
    shot per demo is enough.
- [ ] **Step 2: Android baseline.**
  - Boot AVD `mobiler_verify_p7` (`ANDROID_AVD_HOME=/media/zmilan/data2/android-avd`, headless,
    swiftshader), then run `mobiler dev` in `demos/coffee`.
  - Dismiss the "System UI isn't responding" dialog if it shows ("Wait").
  - Screenshot to `baseline/android-coffee.png`. Leave the emulator running for Task 3.
- [ ] **Step 3: Ledger note** of the baseline paths. No commit.

---

### Task 1: ABI types, `Theme.palette`, breaking-change note, demo literals

**Files:**
- Modify: `mobiler-ui/src/lib.rs`:
  - `Rgb` (~301-313): add `hex`
  - `Theme` (~335-363): new field + doc note + Default
  - new types after `Theme`
  - tests (~760-790)
- Modify: `mobiler-core/src/lib.rs`: add `Palette, ColorRoles, TonePair, Rgba` to the
  `pub use mobiler_ui::{…}` list at ~31
- Modify: `demos/coffee/shared/src/app.rs:259`, `demos/todo/shared/src/app.rs:300`,
  `demos/barbershop/app-core/src/lib.rs:1175`, `demos/saldo/shared/src/app.rs:1019`
- Modify: `README.md`, `mobiler-core/README.md` (the "Upgrading" section)

**Interfaces:**
- Produces:
  - `mobiler_ui::{Palette { light, dark: ColorRoles }, ColorRoles { 20 Option fields, see spec }, TonePair { container, on_container: Rgb }, Rgba { r, g, b, a: u8 }}`
  - `Theme.palette: Option<Palette>`
  - `Rgb::hex(u32) -> Rgb` (const)
  - `TonePair::new(Rgb, Rgb)`
  - `Rgba::new(u8, u8, u8, u8)`
  - All derive `Facet, Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq` and
    `#[repr(C)]`. `ColorRoles` and `Palette` also derive `Default`.
  - Generated Kotlin names: `palette`, `light`, `dark`, `surfaceBar`, `onSurfaceVariant`,
    `onContainer`, …. Swift uses the same camelCase.

- [ ] **Step 1: Write failing tests** in mobiler-ui's test module (next to `action_round_trips`):

```rust
    #[test]
    fn rgb_hex_splits_channels() {
        assert_eq!(Rgb::hex(0x231f20), Rgb::new(0x23, 0x1f, 0x20));
        assert_eq!(Rgb::hex(0xffffff), Rgb::new(255, 255, 255));
    }

    #[test]
    fn theme_default_has_no_palette_and_palette_round_trips() {
        assert_eq!(Theme::default().palette, None);
        let dark = ColorRoles {
            background: Some(Rgb::hex(0x231f20)),
            danger: Some(TonePair::new(Rgb::hex(0x4d2626), Rgb::hex(0xffb4ab))),
            scrim: Some(Rgba::new(0, 0, 0, 82)),
            ..Default::default()
        };
        let theme = Theme { palette: Some(Palette { light: ColorRoles::default(), dark }), ..Default::default() };
        round_trips(&theme);
        assert_eq!(ColorRoles::default().surface, None);
    }
```

- [ ] **Step 2: Run** `cargo test -p mobiler-ui 2>&1 | tail -5`. **Expected:** compile errors
  (`Rgb::hex`, `ColorRoles`, `TonePair`, `Rgba`, `Palette` not found; `Theme` has no `palette`).

- [ ] **Step 3: Implement** in `mobiler-ui/src/lib.rs`.

  In `impl Rgb`:

```rust
    /// `Rgb::hex(0x231f20)` — a colour from a 24-bit hex literal, as design tokens are written.
    #[must_use]
    pub const fn hex(v: u32) -> Self {
        Self { r: ((v >> 16) & 0xff) as u8, g: ((v >> 8) & 0xff) as u8, b: (v & 0xff) as u8 }
    }
```

  Replace the `Theme` doc comment's first line block by prepending the verbatim breaking-change
  note (Global Constraints) as a `///` paragraph. Add the field last:

```rust
    /// Explicit colour roles for light and dark (`dark_mode` picks the set). `None` = the shell's
    /// own colours, exactly as before; a role left `None` inside a set keeps the shell's colour too.
    pub palette: Option<Palette>,
```

  In `Default for Theme`, add `palette: None,`. After `impl Default for Theme`, add:

```rust
/// A light and a dark set of colour roles — the design's tokens. See [`ColorRoles`].
#[derive(Facet, Serialize, Deserialize, Clone, Copy, Debug, Default, PartialEq, Eq)]
#[repr(C)]
pub struct Palette {
    pub light: ColorRoles,
    pub dark: ColorRoles,
}

/// Colour roles; every one optional (`None` = the shell's current colour for that role).
#[derive(Facet, Serialize, Deserialize, Clone, Copy, Debug, Default, PartialEq, Eq)]
#[repr(C)]
pub struct ColorRoles {
    /// Page background.
    pub background: Option<Rgb>,
    /// Cards, sheets, dialogs.
    pub surface: Option<Rgb>,
    /// Top bar, bottom navigation, navigation rail.
    pub surface_bar: Option<Rgb>,
    /// Chips, input fills, segmented track, Filled cards, skeletons, the Neutral tone.
    pub surface_muted: Option<Rgb>,
    /// Text and icons.
    pub on_surface: Option<Rgb>,
    /// Secondary text (captions, unselected tabs).
    pub on_surface_variant: Option<Rgb>,
    /// Field and outlined-button borders.
    pub outline: Option<Rgb>,
    /// Hairlines and dividers.
    pub outline_variant: Option<Rgb>,
    /// Filled buttons, selected calendar day, toggles.
    pub primary: Option<Rgb>,
    pub on_primary: Option<Rgb>,
    /// Primary-coloured text/icons on the background (text buttons, selected tab, back button).
    pub primary_text: Option<Rgb>,
    /// Tonal buttons, selected segment/chip, navigation indicator.
    pub secondary_container: Option<Rgb>,
    pub on_secondary_container: Option<Rgb>,
    pub fab: Option<Rgb>,
    pub on_fab: Option<Rgb>,
    /// One container/on-container pair per [`Tone`]. Filled toned buttons swap the pair.
    pub success: Option<TonePair>,
    pub warning: Option<TonePair>,
    pub danger: Option<TonePair>,
    pub info: Option<TonePair>,
    /// Sheet and dialog scrim.
    pub scrim: Option<Rgba>,
}

/// A tone's soft background and its foreground.
#[derive(Facet, Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[repr(C)]
pub struct TonePair {
    pub container: Rgb,
    pub on_container: Rgb,
}

impl TonePair {
    #[must_use]
    pub const fn new(container: Rgb, on_container: Rgb) -> Self { Self { container, on_container } }
}

/// A colour with alpha (`a`: 0 = transparent, 255 = opaque).
#[derive(Facet, Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[repr(C)]
pub struct Rgba {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: u8,
}

impl Rgba {
    #[must_use]
    pub const fn new(r: u8, g: u8, b: u8, a: u8) -> Self { Self { r, g, b, a } }
}
```

  Re-export the four types from mobiler-core (add them to the existing `pub use mobiler_ui::{…}`).

- [ ] **Step 4: Fix the demo literals.** In each of the four demo files, add
  `..Default::default()` as the last line of the `Theme { … }` literal. Keep every listed field;
  this is the minimal breaking-change fix. Barbershop's test pattern at `lib.rs:2331` already uses
  `..` and needs nothing.

- [ ] **Step 5: README "Upgrading" note.**
  - Add a short `## Upgrading` section (or extend an existing one) to `README.md` and
    `mobiler-core/README.md`, with the verbatim note.
  - Check that the section sits outside the generated capability markers.
  - Then run `cargo run -q -p xtask -- gen-readme --check`. **Expected:** "in sync".

- [ ] **Step 6: Verify.** Run:

```bash
cargo test -p mobiler-ui 2>&1 | grep 'test result'
cargo test -p mobiler-core 2>&1 | grep 'test result'
cargo clippy -p mobiler-core --all-targets -- -D warnings 2>&1 | tail -1
for d in demos/coffee demos/todo demos/saldo demos/barbershop demos/fullstack-todo demos/fullstack-sqlx; do (cd $d && CARGO_TARGET_DIR=$PWD/target cargo check -q --workspace 2>&1 | tail -2 || echo "FAIL $d"); done
(cd mobiler-web && cargo check -q --target wasm32-unknown-unknown 2>&1 | tail -1)
```

**Expected:** all pass, no `FAIL`.

- [ ] **Step 7: Commit** (the ui, core, demo and README files):
  `feat(ui)!: Theme.palette — light/dark colour roles (breaking: Theme literals need ..Default::default())`

---

### Task 2: Web shell reads the palette

**Files:**
- Modify: `mobiler-web/src/lib.rs`:
  - `theme_css` (~2144-2171) and its two call sites (~2065, ~2081)
  - scaffold render (~2044-2049) for `color-scheme` and the document background
- Modify: `mobiler-web/src/mobiler.css`: the rules listed below
- Test: `mobiler-web/src/lib.rs` `#[cfg(test)] mod palette_tests`

**Interfaces:**
- Consumes: Task 1 types.
- Produces:
  - `fn palette_css(roles: &ColorRoles) -> String`, which returns CSS custom properties for the
    **set** roles only.
  - `fn theme_css(t: &Theme, dark: bool) -> String`, where the new `dark` parameter picks the set.

**Variable map** (the `palette_css` output):

| role | vars |
|---|---|
| background | `--bg` |
| surface | `--surface` |
| surface_bar | `--surface-bar` |
| surface_muted | `--surface-2` |
| on_surface | `--ink`, `--fg` |
| on_surface_variant | `--muted` |
| outline | `--outline` |
| outline_variant | `--line`, `--border` |
| primary | `--primary` |
| on_primary | `--primary-ink` |
| primary_text | `--primary-text`, `--accent` |
| secondary_container | `--accent-soft` |
| on_secondary_container | `--on-secondary-container` |
| fab / on_fab | `--fab` / `--on-fab` |
| success etc. | `--tone-success` = container, `--tone-success-on` = on_container (same for warning/danger/info); danger also sets `--danger` = on_container |
| scrim | `--scrim` as `rgba(r,g,b,a/255 to 3 decimals)` |

- [ ] **Step 1: Write failing tests** at the end of `mobiler-web/src/lib.rs`:

```rust
#[cfg(test)]
mod palette_tests {
    use super::*;
    use mobiler_core::{ColorRoles, Palette, Rgb, Rgba, Theme, TonePair};

    #[test]
    fn palette_css_emits_only_set_roles() {
        let r = ColorRoles { background: Some(Rgb::hex(0x231f20)), ..Default::default() };
        assert_eq!(palette_css(&r), "--bg:rgb(35,31,32);");
    }

    #[test]
    fn palette_css_maps_pairs_and_scrim() {
        let r = ColorRoles {
            danger: Some(TonePair::new(Rgb::hex(0x4d2626), Rgb::hex(0xffb4ab))),
            scrim: Some(Rgba::new(0, 0, 0, 82)),
            outline_variant: Some(Rgb::hex(0x4a4442)),
            ..Default::default()
        };
        let css = palette_css(&r);
        assert!(css.contains("--tone-danger:rgb(77,38,38);"));
        assert!(css.contains("--tone-danger-on:rgb(255,180,171);"));
        assert!(css.contains("--danger:rgb(255,180,171);"));
        assert!(css.contains("--line:rgb(74,68,66);--border:rgb(74,68,66);"));
        assert!(css.contains("--scrim:rgba(0,0,0,0.322);"));
    }

    #[test]
    fn theme_css_without_palette_is_unchanged() {
        let t = Theme::default();
        let expected = "--primary:rgb(92,107,192);--accent:rgb(92,107,192);--accent2:rgb(92,107,192);--accent-soft:rgba(92,107,192,0.16);--radius:14px;--gap:12px;--pad:14px;--font:system-ui, -apple-system, \"Segoe UI\", Roboto, sans-serif;";
        assert_eq!(theme_css(&t, false), expected);
        assert_eq!(theme_css(&t, true), expected);
    }

    #[test]
    fn theme_css_appends_the_active_set() {
        let p = Palette {
            light: ColorRoles { background: Some(Rgb::hex(0xfaf7f0)), ..Default::default() },
            dark: ColorRoles { background: Some(Rgb::hex(0x231f20)), ..Default::default() },
        };
        let t = Theme { palette: Some(p), ..Default::default() };
        assert!(theme_css(&t, true).ends_with("--bg:rgb(35,31,32);"));
        assert!(theme_css(&t, false).ends_with("--bg:rgb(250,247,240);"));
    }
}
```

  Before writing the expected string in `theme_css_without_palette_is_unchanged`, run the current
  `theme_css(&Theme::default())` once (e.g. with a temporary `dbg!` in a test) and paste its
  **exact** output. The literal above is the expected shape; the recorded value wins.

- [ ] **Step 2: Run** `cd mobiler-web && cargo test 2>&1 | tail -5`. **Expected:** compile errors
  (`palette_css` missing, `theme_css` takes 1 argument).

- [ ] **Step 3: Implement.**

```rust
fn rgb_css(c: Rgb) -> String { format!("rgb({},{},{})", c.r, c.g, c.b) }

/// The palette's set roles as CSS custom properties (unset roles emit nothing, so `mobiler.css`'s
/// fallbacks — today's colours — stay in force).
fn palette_css(p: &ColorRoles) -> String {
    let mut s = String::new();
    let mut put = |names: &[&str], c: Option<Rgb>| {
        if let Some(c) = c {
            for n in names { s.push_str(&format!("{n}:{};", rgb_css(c))); }
        }
    };
    put(&["--bg"], p.background);
    put(&["--surface"], p.surface);
    put(&["--surface-bar"], p.surface_bar);
    put(&["--surface-2"], p.surface_muted);
    put(&["--ink", "--fg"], p.on_surface);
    put(&["--muted"], p.on_surface_variant);
    put(&["--outline"], p.outline);
    put(&["--line", "--border"], p.outline_variant);
    put(&["--primary"], p.primary);
    put(&["--primary-ink"], p.on_primary);
    put(&["--primary-text", "--accent"], p.primary_text);
    put(&["--accent-soft"], p.secondary_container);
    put(&["--on-secondary-container"], p.on_secondary_container);
    put(&["--fab"], p.fab);
    put(&["--on-fab"], p.on_fab);
    for (name, pair) in [("success", p.success), ("warning", p.warning), ("danger", p.danger), ("info", p.info)] {
        if let Some(t) = pair {
            put(&[&format!("--tone-{name}")], Some(t.container));
            put(&[&format!("--tone-{name}-on")], Some(t.on_container));
            if name == "danger" { put(&["--danger"], Some(t.on_container)); }
        }
    }
    if let Some(a) = p.scrim {
        s.push_str(&format!("--scrim:rgba({},{},{},{:.3});", a.r, a.g, a.b, f32::from(a.a) / 255.0));
    }
    s
}
```

  (If the borrow checker rejects the closure capturing `s` while `format!` names are temporaries,
  make `put` a small `fn put(s: &mut String, names: &[&str], c: Option<Rgb>)`. The output order
  must stay as listed.)

  Then in `theme_css`:
  - Add a `dark: bool` parameter.
  - After the existing `format!(…)`, append
    `t.palette.map(|p| palette_css(if dark { &p.dark } else { &p.light })).unwrap_or_default()`.
  - Update both call sites to pass the scaffold's `dark_mode`.
  - Keep the existing string byte-identical for the no-palette case.

- [ ] **Step 4: Page background and color-scheme.** In the scaffold render, when
  `theme.and_then(|t| t.palette)` is `Some`:
  - Set the `.scaffold` inline style to also include `color-scheme:dark;` or `color-scheme:light;`.
  - Set `document.documentElement().style().set_property("background", <active background css>)`,
    only if that role is set.
  - When it is `None`, remove that property (`remove_property`) so switching palettes off leaves no
    residue.

  Use the existing `web_sys` handles in the file.

- [ ] **Step 5: CSS rules read the new vars with today's values as fallbacks.** Edit
  `mobiler.css` (line numbers from the inventory; confirm with grep):

| Rule | Change |
|---|---|
| `.topbar` (~429), `.tabbar` (~443), rail | `background: var(--surface-bar, var(--bg));` |
| `.field` (~364) | `background: var(--field-bg, var(--surface));` and add `--field-bg: var(--surface-2)` to `palette_css` output only when `surface_muted` is set (append `--field-bg` to that `put` names list: `put(&["--surface-2", "--field-bg"], …)`, then update the test expectation accordingly) |
| `.field` border, searchfield, stepper border | `var(--outline, var(--line))` |
| `.btn-outlined` / `.btn-text` / `.btn-tonal` / `.tab.selected` / `.split-back` / rating colour | `var(--primary-text, <current value>)` |
| `.btn-tonal`, `.chip.selected`, `.segment.selected`, rail selected | text `var(--on-secondary-container, <current>)`; `.segment.selected` background `var(--seg-sel-bg, var(--primary))` and text `var(--seg-sel-fg, #fff)`. `palette_css` sets `--seg-sel-bg` = secondary_container and `--seg-sel-fg` = on_secondary_container (add to the `put` lists and update the tests) |
| `.fab` (~453) | `background: var(--fab, var(--primary)); color: var(--on-fab, #fff);` |
| `.cal-sel` (~274), toggle knob, toned filled (`#fff`), brand card text | `var(--primary-ink, #fff)` where the fill is primary; brand card text `var(--primary-ink, #fff)` |
| `.tone-success/-warning/-danger/-info` (~197-201) | `background: var(--tone-X, <current>); color: var(--tone-X-on, <current>);` |
| toned buttons (~331-343) | filled: background `var(--tone-X-on, var(--tone))`, text `var(--tone-X, #fff)` (the flipped pair); tonal: background `var(--tone-X, var(--tone-soft))`, text `var(--tone-X-on, var(--tone))`; outlined/text: `var(--tone-X-on, var(--tone))` |
| `.sheet-scrim` (~107), `.confirm-scrim` (~120) | `background: var(--scrim, rgba(0,0,0,.45));` |

  Neutral-tone badges already read `--surface-2`/`--muted`, and cards read `--surface`: no change.
  For the dark toned-button block (~335-340), give the same `var(--tone-X…, <dark current>)` so the
  dark `None` path stays identical.

- [ ] **Step 6: Verify.** Run:

```bash
cd mobiler-web && cargo test 2>&1 | grep -E 'test result|FAILED'
cargo clippy --all-targets -- -D warnings 2>&1 | tail -1
cargo check --target wasm32-unknown-unknown 2>&1 | tail -1
```

  Then rebuild `demos/coffee/web` and `demos/todo/web` and screenshot them exactly as in Task 0.
  Compare with the baselines:

```bash
python3 -c "from PIL import Image, ImageChops; import sys; a,b=Image.open(sys.argv[1]).convert('RGB'),Image.open(sys.argv[2]).convert('RGB'); print(ImageChops.difference(a,b).getbbox())" baseline/web-coffee.png after/web-coffee.png
```

**Expected:** tests pass; clippy clean; the wasm check finishes; `getbbox()` prints `None`
(identical) for both demos.

- [ ] **Step 7: Commit:** `feat(web): Theme.palette → CSS variables, today's colours as fallbacks`

---

### Task 3: Android shell (barbershop) reads the palette

**Files:**
- Modify: `demos/barbershop/Android/app/src/main/java/dev/mobiler/barbershop/ui/theme/Theme.kt`
- Modify: `demos/barbershop/Android/app/src/main/java/dev/mobiler/barbershop/MainActivity.kt`
  (sites below; confirm the line numbers with grep, since barbershop is offset from the template)

**Interfaces:**
- Consumes: the generated Kotlin `Theme.palette: Palette?`, `Palette.light/.dark: ColorRoles`,
  `ColorRoles.background: Rgb?` …, `TonePair.container/.onContainer`, `Rgba.r/g/b/a`.
- Produces:
  - `val LocalPalette = staticCompositionLocalOf<ColorRoles?> { null }` (in Theme.kt)
  - `fun Rgb.color(): Color`
  - `fun Rgba.color(): Color`

- [ ] **Step 1: Theme.kt.** Add the helpers and the palette scheme, and provide `LocalPalette`:

```kotlin
import androidx.compose.material3.ColorScheme
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.staticCompositionLocalOf
import dev.mobiler.barbershop.shared.types.ColorRoles
import dev.mobiler.barbershop.shared.types.Rgb
import dev.mobiler.barbershop.shared.types.Rgba

/** The active palette set (null = no palette: every widget keeps its current colours). */
val LocalPalette = staticCompositionLocalOf<ColorRoles?> { null }
fun Rgb.color() = Color(r.toInt(), g.toInt(), b.toInt())
fun Rgba.color() = Color(r.toInt(), g.toInt(), b.toInt(), a.toInt())

/** Every M3 slot a role maps to; unset roles keep `base`'s slot. */
private fun paletteScheme(base: ColorScheme, p: ColorRoles): ColorScheme {
    val surface = p.surface?.color()
    return base.copy(
        background = p.background?.color() ?: base.background,
        onBackground = p.onSurface?.color() ?: base.onBackground,
        surface = surface ?: base.surface,
        onSurface = p.onSurface?.color() ?: base.onSurface,
        surfaceVariant = p.surfaceMuted?.color() ?: base.surfaceVariant,
        onSurfaceVariant = p.onSurfaceVariant?.color() ?: base.onSurfaceVariant,
        surfaceContainerLowest = surface ?: base.surfaceContainerLowest,
        surfaceContainerLow = surface ?: base.surfaceContainerLow,
        surfaceContainer = p.surfaceBar?.color() ?: base.surfaceContainer,
        surfaceContainerHigh = surface ?: base.surfaceContainerHigh,
        surfaceContainerHighest = surface ?: base.surfaceContainerHighest,
        outline = p.outline?.color() ?: base.outline,
        outlineVariant = p.outlineVariant?.color() ?: base.outlineVariant,
        primary = p.primary?.color() ?: base.primary,
        onPrimary = p.onPrimary?.color() ?: base.onPrimary,
        secondaryContainer = p.secondaryContainer?.color() ?: base.secondaryContainer,
        onSecondaryContainer = p.onSecondaryContainer?.color() ?: base.onSecondaryContainer,
        primaryContainer = p.fab?.color() ?: base.primaryContainer,
        onPrimaryContainer = p.onFab?.color() ?: base.onPrimaryContainer,
        error = p.danger?.onContainer?.color() ?: base.error,
        onError = p.danger?.container?.color() ?: base.onError,
        errorContainer = p.danger?.container?.color() ?: base.errorContainer,
        onErrorContainer = p.danger?.onContainer?.color() ?: base.onErrorContainer,
        scrim = p.scrim?.color() ?: base.scrim,
    )
}
```

  In `FadehouseTheme`:
  - Compute `val roles = theme?.palette?.let { if (darkTheme) it.dark else it.light }`.
  - In the `theme != null` branch, after `base.copy(primary = seed, …)`, apply
    `.let { s -> roles?.let { paletteScheme(s, it) } ?: s }`.
  - Wrap `MaterialTheme(…)` in `CompositionLocalProvider(LocalPalette provides roles) { … }`.

  Note: `scrim` in the M3 scheme is opaque, and ModalBottomSheet applies its own alpha. So with a
  palette scrim, pass `scrimColor = LocalPalette.current?.scrim?.color() ?: BottomSheetDefaults.ScrimColor`
  explicitly at the sheet (Step 3), and leave the scheme's `scrim` as today's.
  **Remove the `scrim = …` line from `paletteScheme`.**

- [ ] **Step 2: Tones.** In `toneColors(tone)` and `toneStrong(tone)` (MainActivity ~1473/~1486):
  - Add a `roles: ColorRoles?` parameter. Callers pass `LocalPalette.current`; they are all
    composables, so read it there.
  - At the top of each function, return the palette pair when set, otherwise fall through to
    today's table.

```kotlin
private fun tonePair(roles: ColorRoles?, tone: Tone): TonePair? = when (tone) {
    Tone.SUCCESS -> roles?.success
    Tone.WARNING -> roles?.warning
    Tone.DANGER -> roles?.danger
    Tone.INFO -> roles?.info
    Tone.NEUTRAL -> null
}
// toneColors (soft):   tonePair(roles, tone)?.let { return it.container.color() to it.onContainer.color() }
//   NEUTRAL with palette: (roles?.surfaceMuted?.color() ?: <current>) to (roles?.onSurfaceVariant?.color() ?: <current>)
// toneStrong (filled): tonePair(roles, tone)?.let { return it.onContainer.color() to it.container.color() }   // flipped
```

  Also:
  - Avatar status dot: with a palette, use `toneColors(...).second`, the on_container, so the dot
    is visible. Without a palette, keep `.first`.
  - SwipeAction: with a palette, background `container`, label `onContainer`; without one, today's
    values.

- [ ] **Step 3: Explicit colours where no M3 slot applies** (each is `LocalPalette.current?.X?.color() ?: <today's expression>`):

| Site | Role |
|---|---|
| TopAppBar (~1251, `containerColor = colorScheme.surface`) | `surfaceBar` |
| NavigationBar (~1256), NavigationRail (~1228) `containerColor` | `surfaceBar` |
| Text/Outlined neutral button `contentColor`, Rating/star tint, Split back TextButton, calendar dots on the background, chart series 0 | `primaryText` |
| FAB (~1270) | the containerColor/contentColor come from the scheme's primaryContainer, which the palette sets; no change needed. Verify in Task 5 |
| ModalBottomSheet (~1212) `scrimColor` | `scrim` (Rgba) else `BottomSheetDefaults.ScrimColor` |
| Card BRAND (~944-954) | gradient end stays `secondary`; content `onPrimary` from the palette when set (`LocalPalette.current?.onPrimary?.color() ?: Color.White`) |
| Card FILLED (~937) | `surfaceMuted` (surfaceVariant already mapped; no change needed) |
| Skeleton (~491) | `surfaceMuted` when set, else today's `onSurface @0.12` |
| OutlinedTextField/Search `unfocusedContainerColor`/`focusedContainerColor` | `surfaceMuted` when set (pass `OutlinedTextFieldDefaults.colors(...)` only when the palette is set; otherwise keep the call unchanged) |

- [ ] **Step 4: System bars follow `dark_mode` with a palette.** In the activity content (where
  `dark` is computed, ~294):
  - With `appTheme?.palette != null`, use a `LaunchedEffect(dark)` that calls
    `enableEdgeToEdge(statusBarStyle = if (dark) SystemBarStyle.dark(android.graphics.Color.TRANSPARENT) else SystemBarStyle.light(android.graphics.Color.TRANSPARENT, android.graphics.Color.TRANSPARENT), navigationBarStyle = <same>)`.
  - Without a palette, leave today's single `enableEdgeToEdge()` alone.
  - The activity reference is available as `this@MainActivity`; if the composable is outside the
    activity, pass a lambda in.

- [ ] **Step 5: Build.** Run:
  `cd demos/barbershop && CARGO_TARGET_DIR=$PWD/target JAVA_HOME=~/jdk21 ANDROID_HOME=/home/zmilan/Android/Sdk timeout 1500 <cli> build android` (in the background).
  **Expected:** an APK path, with no `e:` lines.

- [ ] **Step 6: Commit:** `feat(barbershop/android): palette roles in the Android shell`

---

### Task 4: iOS shell (barbershop) reads the palette

**Files:**
- Modify: `demos/barbershop/iOS/Sources/Render.swift`, `demos/barbershop/iOS/Sources/Core.swift`

**Interfaces:**
- Consumes: the generated Swift `Theme.palette: Palette?`, `ColorRoles` (camelCase), `TonePair`,
  `Rgba`.
- Produces:
  - `enum ActivePalette { nonisolated(unsafe) static var current: ColorRoles? }`
  - `func role(_ c: Rgb?, else fallback: Color) -> Color`
  - `extension Rgb { var color: Color }`, `extension Rgba { var color: Color }`

- [ ] **Step 1: Resolution.** Next to `ActiveTheme` in Render.swift:

```swift
/// The active palette set (light or dark by the root scaffold's dark mode); nil = no palette.
enum ActivePalette {
    nonisolated(unsafe) static var current: ColorRoles?
}
extension Rgb { var color: Color { Color(red: Double(r) / 255, green: Double(g) / 255, blue: Double(b) / 255) } }
extension Rgba { var color: Color { Color(red: Double(r) / 255, green: Double(g) / 255, blue: Double(b) / 255, opacity: Double(a) / 255) } }
/// The palette colour for a role, else the shell's current colour.
func role(_ c: Rgb?, else fallback: Color) -> Color { c?.color ?? fallback }
```

  In Core.swift, at **both** places `ActiveTheme.current` is set (init and `.render`), bind the
  dark flag and set the palette. The Scaffold's 5th field (index 4) is `dark_mode`.

```swift
if case let .scaffold(_, _, _, _, dark, theme, _, _, _, _, _, _, labels) = view {
    ActiveTheme.current = theme; ActiveLabels.current = labels
    ActivePalette.current = theme?.palette.map { dark ? $0.dark : $0.light }
} else { ActiveTheme.current = nil; ActiveLabels.current = nil; ActivePalette.current = nil }
```

- [ ] **Step 2: Route colours through `role`.**
  - Every site is `role(ActivePalette.current?.X, else: <today's exact expression>)`.
  - For tones, use a `toneColors` variant that checks the palette pair first.
  - Sites (Render.swift lines from the inventory, confirm with grep):

| Site | Role |
|---|---|
| `toneColors` (~1455) | success/warning/danger/info pair when set (container, on_container); neutral `surfaceMuted`/`onSurfaceVariant` |
| MobilerButton toned tint (~1336) and filled toned | filled: fill `on_container`, text `container` (flipped); others as today with the pair |
| LargeButtonStyle filled `.white` (~1396), segmented selected `.white` (~303), calendar selected `.white` (~1008, ~1014) | `onPrimary` (segmented: see below) |
| Segmented selected bg `Color.accentColor` (~302) | `secondaryContainer`; text `onSecondaryContainer` (only when the palette is set; otherwise today's accent + white) |
| Segmented track / chip unselected / search bg `gray@0.12` (~310, ~234, ~289) | `surfaceMuted` |
| Skeleton `gray@0.2` (~102) | `surfaceMuted` |
| CardMod Elevated `secondarySystemBackground` (~1422), Filled `tertiarySystemBackground` (~1425) | Elevated `surface`; Filled `surfaceMuted` |
| CardMod Outlined stroke `gray@0.3` (~1427) | `outlineVariant` |
| Brand card fg `.white` (~1434) | `onPrimary` |
| Sheet panel `Color(.systemBackground)` (~1155), swipe cover (~969), avatar ring (~514) | `surface` |
| Sheet scrim `black@0.45` (~1145) | `ActivePalette.current?.scrim?.color ?? Color.black.opacity(0.45)` |
| FAB bg/fg (~1237-1238) | `fab` / `onFab` |
| Caption `.secondary` (~1317), tabs unselected `.secondary` (~1258), weekday (~998) | `onSurfaceVariant` |
| Tabs selected / rail selected / back chevron / rating / chip-selected fg (`.accentColor`) | `primaryText` |
| Rail selected bg `accentColor@0.12` (~1284) | `secondaryContainer` |
| `Divider()` sites (~93, ~1114, ~1181, ~1247) | with a palette: `Rectangle().fill(role(…outlineVariant, else: .clear)).frame(height: 1)` only when `outlineVariant` is set; else keep `Divider()` |
| Field error `.red` (~274) | `danger.onContainer` |

- [ ] **Step 3: Surfaces behind the page and bars.** In `ScaffoldView` (~1087-1304), when
  `ActivePalette.current` is non-nil:
  - `.background(role(p.background, else: .clear).ignoresSafeArea())` on the root stack.
  - Top bar HStack (~1166) and tab HStack (~1248) get `.background(role(p.surfaceBar, else: .clear))`,
    extended into the safe area on the edge side.
  - Default text colour: `.foregroundStyle(role(p.onSurface, else: .primary))` on the body
    container.
  - Without a palette, add none of these modifiers. Use an `if let` / a `ViewModifier` that
    returns content unchanged.

- [ ] **Step 4: Compile check.** Swift can't be compiled locally. Grep that every `ActivePalette`
  / `role(` use is well-formed and that the Core.swift pattern has exactly 13 positions (count the
  commas against the old line). The macOS CI lane `iOS build (demos/barbershop)` is the compile
  gate in Task 6.

- [ ] **Step 5: Commit:** `feat(barbershop/ios): palette roles in the iOS shell`

---

### Task 5: Barbershop gets the Moj Termin palette + acceptance checks

**Files:**
- Modify: `demos/barbershop/app-core/src/lib.rs` (theme at ~1175)
- Output: screenshots in the scratchpad (not committed until Task 6's README decision)

- [ ] **Step 1: Palette in barbershop.** Add a `fn moj_termin_palette() -> Palette` with the §3
  values from `docs/theme-palette-tokens.md`, and set `palette: Some(moj_termin_palette())` on
  barbershop's theme.
  - Dark: every role.
  - Light: every role **except** `on_primary`, `fab`, `on_fab`, `scrim` (the request lists none;
    this also exercises the partial-set fallback).
  - Scrim: `Rgba::new(0, 0, 0, 82)` (0.32 × 255 ≈ 82).
  - Barbershop's existing dark-mode toggle switches the sets.

  Add a core test:

```rust
    #[test]
    fn barbershop_theme_carries_the_moj_termin_palette() {
        let p = moj_termin_palette();
        assert_eq!(p.dark.background, Some(Rgb::hex(0x231f20)));
        assert_eq!(p.dark.danger, Some(TonePair::new(Rgb::hex(0x4d2626), Rgb::hex(0xffb4ab))));
        assert_eq!(p.light.on_primary, None);
    }
```

  Run `cd demos/barbershop && CARGO_TARGET_DIR=$PWD/target cargo test -q -p barbershop-core 2>&1 | grep 'test result'` (use the package name from its Cargo.toml).
  **Expected:** pass.

- [ ] **Step 2: Web acceptance.**
  - Build `demos/barbershop/web` and serve it.
  - With headless Chrome over CDP (Node 22 script, no deps, per memory `mobiler-web-shell`), read
    `getComputedStyle` for: `html` background, `.scaffold` background, `.topbar`, `.card`, body
    text, a `.tone-danger` badge (bg + color), a filled button, a tonal button,
    `.segment.selected`, `.fab`.
  - Do this in dark, then click the dark-mode toggle and repeat for light.
  - Open a sheet or confirm if barbershop has one on the start screen, and read `.sheet-scrim`
    and `.confirm-card`.
  - Listen for `Runtime.exceptionThrown`.
  - **Expected:** each value equals the §3 hex (as `rgb(…)`). Unset light roles show today's
    values: the filled button text is `#fff`-equivalent `--primary-ink`, and the scrim is
    `rgba(0,0,0,0.45)`. No exceptions.

- [ ] **Step 3: Android acceptance.**
  - Run `mobiler dev` in `demos/barbershop` on the AVD and take screenshots, dark then light
    (tap the toggle via `uiautomator` bounds).
  - Sample pixels with PIL at node bounds for: the background margin, the top bar, a card
    interior, a badge interior, a filled/tonal button interior and the FAB.
  - Check the status bar icons: in the dark screenshot they are light-coloured.
  - **Expected:** sampled colours are within ±3 per channel of the §3 values.
  - If barbershop doesn't show a given widget on a reachable screen, note it in the ledger as
    covered by web only, instead of adding UI.
  - Avoid leaving the Profile tab (known swiftshader crash).

- [ ] **Step 4: `None` path unchanged on Android.** Run `mobiler dev` in `demos/coffee`,
  screenshot, and compare with `baseline/android-coffee.png` using PIL `ImageChops.difference`
  `getbbox()`.
  - **Expected:** `None`, or a bbox limited to the status-bar clock (note that case).

- [ ] **Step 5: Commit** the barbershop app-core change:
  `feat(barbershop): Moj Termin palette (dark default) — palette demo + acceptance fixture`.
  Kill the emulator (`adb emu kill`).

---

### Task 6: Docs, review, PR (merge to main; no publish)

- [ ] **Step 1: NOTES.md** (gitignored): add a section "Theme palette (2026-09-27)" covering:
  - `Theme.palette` (breaking) and why `with_palette` was dropped
  - the fallback rule
  - the filled-tone flip
  - the per-shell mechanism (CSS vars with fallbacks / `paletteScheme` + `LocalPalette` /
    `ActivePalette` + `role`)
  - the template-port-later rule
  - the `None`-path follow-ups: web dark badges, avatar dot, undefined `--fg`/`--border`/`--danger`
    on the `None` path, and the web Info button vs badge colour

  Bump the footer.
- [ ] **Step 2: Whole-branch fresh review** (executing-plans final review).
- [ ] **Step 3: ship-pr.** On the PR, **all 25 checks** must pass. This includes
  `iOS build (demos/barbershop)` (the Swift compile gate) and `scaffold + build (template, Android)`:
  the template is unchanged and the published core 0.39 still builds, so this lane stays green.
  Squash-merge. **No version bump, no publish.**
- [ ] **Step 4: Memory + start.md.** Record that the palette is on main awaiting the design
  release, and that the template port list is barbershop's diff in `MainActivity.kt`,
  `ui/theme/Theme.kt`, `Render.swift` and `Core.swift`.
