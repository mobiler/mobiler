# Theme palette: explicit colour roles, light + dark

**Request:** `docs/theme-palette-tokens.md` (appointments / Moj Termin design set, priority 1, blocking).
**Release:** part of the single Moj Termin design release. Target libs are mobiler-ui 0.29 /
mobiler-core 0.40 / mobiler-web 0.40, with the CLI after. **Nothing is published until the whole
design set is done** (agreed 2026-09-27). This feature lands on `main` as core + mobiler-web + demo
shells. Template shells are ported in the release's final CLI PR, because the scaffold lane builds
templates against the *published* core; see memory `template-shell-lands-after-publish`.

## Problem

`Theme` carries two colours (`seed`, `accent`). Every other colour is baked into each shell:
- Android copies three slots onto a baseline M3 purple scheme.
- iOS uses system colours.
- Web has a fixed light/dark variable set.

So the design's 26 roles × light/dark cannot be expressed. The warm charcoal background, cream text
and the status pairs all come out as each shell's defaults.

## Decisions

1. **`Theme.palette: Option<Palette>`**, as the request proposed. This is a **breaking change** for
   code that builds `Theme` as a full struct literal. We accept it, because there is no live app
   yet (user, 2026-09-27). The type-scale and component-shapes requests will extend `Theme` the
   same way, so the whole look stays in one struct.
2. **Unset means unchanged.** `palette: None` renders exactly as 0.39 on every shell. A role left
   `None` inside a set falls back to that shell's current value for the role.
3. **Filled toned buttons flip the pair.** They use the `on_container` colour as the fill and the
   `container` colour as the text. Every soft use takes the pair as given. There is no extra
   "strong" colour per tone.
4. **The known inconsistencies of the `None` path are not fixed here** (web dark badges, the
   near-invisible avatar dot, undefined web vars `--fg`/`--border`/`--danger`), because that
   would change existing looks. With a palette set, every one of them is resolved by the role
   wiring. The `None`-path fixes are logged as follow-ups.
5. **Data colours stay fixed:** chart series palette, RegionChart, `ProjectColor`, the map marker
   and the `Box` image scrim.

## ABI (mobiler-ui)

```rust
pub struct Theme {
    pub seed: Rgb,
    pub accent: Option<Rgb>,
    pub corner: Corner,
    pub density: Density,
    pub font: FontFamily,
    /// Explicit colour roles for light and dark. `None` = the shell's own colours (unchanged).
    pub palette: Option<Palette>,
}

pub struct Palette { pub light: ColorRoles, pub dark: ColorRoles }

/// Every role optional: `None` = the shell's current colour for that role.
#[derive(Default, ...)]
pub struct ColorRoles {
    pub background: Option<Rgb>,
    pub surface: Option<Rgb>,
    pub surface_bar: Option<Rgb>,
    pub surface_muted: Option<Rgb>,
    pub on_surface: Option<Rgb>,
    pub on_surface_variant: Option<Rgb>,
    pub outline: Option<Rgb>,
    pub outline_variant: Option<Rgb>,
    pub primary: Option<Rgb>,
    pub on_primary: Option<Rgb>,
    pub primary_text: Option<Rgb>,
    pub secondary_container: Option<Rgb>,
    pub on_secondary_container: Option<Rgb>,
    pub fab: Option<Rgb>,
    pub on_fab: Option<Rgb>,
    pub success: Option<TonePair>,
    pub warning: Option<TonePair>,
    pub danger: Option<TonePair>,
    pub info: Option<TonePair>,
    pub scrim: Option<Rgba>,
}

pub struct TonePair { pub container: Rgb, pub on_container: Rgb }
pub struct Rgba { pub r: u8, pub g: u8, pub b: u8, pub a: u8 }   // u8 alpha keeps Theme: Eq
```

- All new types derive `Facet, Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq`,
  `#[repr(C)]`. `Theme` stays `Copy + Eq`. `Palette` and `ColorRoles` derive `Default`.
- `Theme::default()` sets `palette: None`.
- `Rgb::hex(0x231f20)` is a `const fn` taking a `u32`, and `Rgba::new(r, g, b, a)`.
  `TonePair::new(container, on_container)`.
- `Theme`'s doc comment gains a **breaking-change note**, the one the user asked for:
  > **Breaking in mobiler-ui 0.29 / mobiler-core 0.40:** `Theme` gained `palette` (and further
  > design-release fields follow). Code that lists every field in a `Theme { … }` literal no longer
  > compiles. Write `Theme { seed, ..Default::default() }` and set only what you need.

  The same note goes in an "Upgrading" section of the root `README.md` and `mobiler-core/README.md`.

**Mode selection:** the active set is `if dark_mode { dark } else { light }`. The
system-appearance request later adds "follow the system" on top of this same selection.

## Role → widget mapping (applies only when `palette` is set; each role unset = current value)

| Role | Widgets |
|---|---|
| `background` | page / root, behind the scaffold (web `html`/`body` too) |
| `surface` | cards Elevated/Outlined, sheet panel, confirm dialog, swipe-row cover, avatar status ring, web pdf/donut hole |
| `surface_bar` | top bar, bottom nav bar, nav rail |
| `surface_muted` | Filled card, unselected chip, input fills (text/search field), segmented track, skeleton, `Tone::Neutral` badge background |
| `on_surface` | body/title text, icons, calendar day numbers, chart axes and gauge text, checkbox labels |
| `on_surface_variant` | captions, unselected tabs and segments, weekday labels, search icon, chart ticks, `Tone::Neutral` badge text |
| `outline` | text/search field borders, outlined button and stepper borders, unselected chip border |
| `outline_variant` | dividers, Outlined card border, top/tab bar hairlines, sheet handle, progress and toggle track, chart gridlines and ring track |
| `primary` / `on_primary` | Filled neutral button, selected calendar day (+ dots on it), toggle/checkbox/slider/progress fill, Brand card start colour / its text |
| `primary_text` | Text and Outlined neutral button text, selected tab/rail item, back button, rating stars, calendar dots on the background, chart series 0 |
| `secondary_container` / `on_…` | Tonal neutral button, selected segment, selected chip, nav rail/bar selected indicator |
| `fab` / `on_fab` | FAB |
| `success/warning/danger/info` | badge, avatar status dot, swipe action, Tonal/Outlined/Text toned buttons: **container behind on_container**. Filled toned buttons: **on_container fill, container text**. `danger` also covers the text-field error border/text and the destructive confirm button |
| `scrim` | sheet and confirm scrims (not the `Box` image scrim) |

`CardStyle::Brand` with a palette: a gradient from `primary` to `theme.accent ?? primary`, with
`on_primary` text.

## Per shell

**Android** (`MainActivity.kt`, `ui/theme/Theme.kt`)
- Palette set: start from today's themed scheme and override every M3 slot a role maps to:
  - background, surface, surfaceContainer{Lowest..Highest} (cards, sheet, dialog read these by
    default), surfaceVariant
  - onSurface, onSurfaceVariant, outline, outlineVariant
  - primary, onPrimary, secondaryContainer, onSecondaryContainer
  - primaryContainer/onPrimaryContainer (for the FAB), error pair from `danger`, scrim
- Widgets that don't read a slot get explicit colours from a `LocalPalette` CompositionLocal (the
  resolved `ColorRoles` for the active mode): top bar and nav bar/rail (`surface_bar`), tone pairs,
  `primary_text`, the flipped filled-tone pair.
- With a palette, system bar icon contrast follows `dark_mode`
  (`enableEdgeToEdge(SystemBarStyle…)`). It is unchanged without one.

**iOS** (`Render.swift`, `Core.swift`)
- `ActiveTheme` also stores the **resolved** roles for the current mode
  (`ActivePalette.current: ColorRoles?`, set where `ActiveTheme` is set, from `dark_mode`).
- A helper `role(\.surface, else: <today's colour>)` returns the palette colour or today's value.
- Every system colour / `.white` on a primary fill / `gray@x` listed in the inventory is routed
  through it.
- Palette set: the scaffold, top bar and tab bar get `.background(...)` fills (`background`,
  `surface_bar`) with `ignoresSafeArea` where needed.
- New statics touched from closures use `nonisolated(unsafe)`, per the `ActiveTheme` precedent and
  the iOS lesson.

**Web** (`mobiler-web`)
- `theme_css` also emits one CSS variable per set role of the **active** set, inline on
  `.scaffold` (which the confirm portal already copies):
  - existing names: `--bg`, `--surface`, `--surface-2`, `--ink`, `--muted`, `--primary`,
    `--primary-ink`, `--accent-soft`
  - new names: `--surface-bar`, `--outline`, `--line` (= outline_variant), `--primary-text`,
    `--on-secondary-container`, `--fab`, `--on-fab`, `--tone-{success,warning,danger,info}` and
    `-on`, `--scrim`
- CSS rules read the new variables with today's value as the fallback (`var(--surface-bar, var(--bg))`),
  so the `None` path renders identically.
- Palette set: the active `background` is also written to `document.documentElement` (page
  behind the scaffold), and `color-scheme: dark|light` is set on the scaffold.
- Palette set: `--fg`, `--border` and `--danger` are defined, so the palette path has no
  light-only fallbacks.

## Demos and verification

- **Demo literals:** coffee, todo, barbershop and saldo switch their `Theme { … }` literals to
  `..Default::default()` (the breaking change, in the same commit).
- **barbershop** gets the Moj Termin palette from the request (dark default, light via its existing
  dark-mode toggle). It is the appointments-like demo.
- **Acceptance 1–3:** Android (AVD) and web (headless Chrome), light and dark. Screenshots, plus
  pixel colours sampled at known widgets (page background, card, top bar, text, badge per tone,
  filled/tonal button, selected segment, FAB) compared with the request's hex values.
- **Acceptance 4 (`None` unchanged):** coffee and todo, before/after screenshots on Android and
  web, pixel-identical apart from anti-aliasing.
- **Acceptance 5:** a core unit test covers the role resolution helper where it is Rust (web). The
  shells use a fallback expression per role. A demo leaves some roles unset (e.g. the light
  `on_primary`, `fab`), and the screenshot shows the fallback, not black or transparent.
- **iOS:** macOS CI compile only. A runtime check waits for a simulator session.

## Out of scope

- System appearance / "follow the system" (request 2).
- Fonts, type scale, shapes (3–5).
- Fixing the `None`-path inconsistencies (follow-up list).
- The `open_url` callback, which is a separate small item in the same release.
