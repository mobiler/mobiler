# Dark theme gaps: launch window, first-frame appearance, selection and error roles

**Request:** Moj Termin (appointments admin app), 2026-10-01, "four gaps in light/dark theming". The request doc
is temporary and is deleted after the release, so its substance is restated below.
**Release:** libraries ui 0.30 / core 0.41 / web 0.41 (new `ColorRoles` fields), then a CLI release that ports
the shells, adds the launch-window resources and the upgrade "seed" class.
**Constrained by:** ADR-0008 (ABI grows by appending; a minor bump), ADR-0009 (libraries first; template code
using the new ABI lands in the CLI PR), ADR-0012 (upgrade three-way merge), ADR-0015 (barbershop first, then the
templates), ADR-0019 (optional roles; unset renders as before), ADR-0020 (appearance; the OS value is readable),
ADR-0022 (files declared in `mobiler.toml` are synced into the shells; the model for the next release's
`[splash]`), ADR-0039 (Android minimum API 26; newer APIs behind a version check).
**New records:** ADR-0041 (the shell never sets an app-level night mode), ADR-0042 (upgrade "seed" files).
**Next release (not in scope, but this design must not need rework for it):** a `[splash]` section in
`mobiler.toml` (light and dark background, an optional image), synced into the shells.

## Problem

Moj Termin uses `Theme.palette` (both sets), `with_appearance`, custom fonts, a type scale and shapes. A code
review found four things only the framework can change:

1. **White launch on a dark phone (Android).** The template theme is `android:Theme.Material.Light.NoActionBar`
   with no night variant. Before Compose's first frame, and on the Android 12+ system splash, the window is
   light. The owner's first report was "the app stays white on a dark phone".
2. **No selection role.** The design has a separate selection colour (`sel`: light `#1f8276`, dark `#4fb3a4`).
   The shells draw focused field borders, checkboxes, toggles and progress in `primary`. Their dark `primary`
   must stay `#1f8276` for white text on filled buttons (4.65:1), but as a mark it is 2.60:1 on the muted fill
   and 2.72:1 on the card, below the 3:1 WCAG 1.4.11 asks of focus indicators.
3. **The app's own appearance choice arrives late.** The app reads its stored choice from `securestore` in
   `init`; until the reply the scaffold says System, so Dark-on-a-light-phone shows a light frame first.
4. **No error roles.** Error text and filled Danger buttons use the `danger` pair (`#93000a` / `#ffb4ab`; the
   filled button swaps the pair). The design's error text is `err` (`#b3261e` / `#ff8f85`), its destructive
   fill `err-fill` (`#b3261e` / `#c0392b`) with white text.

## Decisions (approved 2026-10-01)

### 1. Launch window: dark on a dark phone, built on the files `[splash]` will write

**Upgrade "seed" class (ADR-0042).** `mobiler upgrade` today skips app-owned (`Own`) files entirely, even when
they are missing (`mobiler/src/upgrade.rs`, `classify` and the `Class::Own` early return). A new app-owned
resource would never reach an existing app, and a shell file referencing it would break the build. A new class,
`Seed`:
- `classify` returns `Seed` for an explicit list of paths (below).
- Upgrade **creates** a missing seed file from the template and reports it as added; it **never** reads, merges
  or overwrites an existing one. No baseline is written for it.
- `mobiler new` writes seed files like any other file.
- The next release's `[splash]` sync writes into the same files. With no `[splash]` section the sync leaves them
  alone, so a hand edit survives.

**Android seed files (the values `[splash]` will own):**
- `app/src/main/res/values/mobiler_splash.xml`:
  - `<color name="mobiler_splash_background">`: the shell's unthemed light background (the default M3 light
    scheme's `background` for the shell's material3 version, read at implementation).
  - `<drawable name="mobiler_splash_icon">@mipmap/ic_launcher</drawable>`: the icon Android 12+ already shows.
- `app/src/main/res/values-night/mobiler_splash.xml`: `mobiler_splash_background`, the unthemed dark
  background.
- `app/src/main/res/drawable/mobiler_launch.xml`: a `<layer-list>` with one item, the background colour. The next
  release adds a centred bitmap item here (a branded launch window on Android 8–11).

**Android shell themes (written now, unchanged by `[splash]`):**
- `values/themes.xml`: parent `android:Theme.Material.Light.NoActionBar` (as today), plus
  `android:windowBackground = @drawable/mobiler_launch`.
- `values-night/themes.xml`: the same style with parent `android:Theme.Material.NoActionBar`.
- `values-v31/themes.xml` and `values-night-v31/themes.xml`: the matching parent, the same window background,
  plus `android:windowSplashScreenBackground = @color/mobiler_splash_background` and
  `android:windowSplashScreenAnimatedIcon = @drawable/mobiler_splash_icon`. The background is set explicitly
  because the 12+ splash derives it from `windowBackground` only when that is a single colour, and
  `mobiler_launch` is a layer-list.
- The status and navigation bar styling stays with `enableEdgeToEdge` as today.

**iOS (prepared, no visual change):**
- Seed: `iOS/Sources/Assets.xcassets/MobilerSplashBackground.colorset/Contents.json`, white for any appearance
  and black for dark: exactly the system background the empty `UILaunchScreen` shows today.
- Shell: `project.yml` `UILaunchScreen: { UIColorName: MobilerSplashBackground }`. The next release adds
  `UIImageName` and the image set.
- `classify` today marks every `Assets.xcassets/` path `Own`; the seed rule takes precedence for this colour set.

**Web:** nothing. The template has no web page; an app's `index.html` is its own.

**Docs:** the CLI README gains a short "Launch window colours" section: which seed files to edit for the light
and dark background (and the iOS colour set), and that a later release will write them from `mobiler.toml`.

### 2. Appearance from the first frame (documentation; ADR-0041)

- On Android the shell's Core sends `AppInfo`, `Restore` and `Start` synchronously while it is created, before
  the first frame, so the first frame already shows the core's state after `Start`. iOS and web do the same
  before their first render. The flash comes from the app reading its choice asynchronously.
- **Guidance:** keep the appearance choice in the app's `cx.save` state (it is not a secret). `Restore` arrives
  before `Start`, so the first render already carries it. Written in the `with_appearance` doc comment and the
  `mobiler-core` README's appearance section.
- **What stays:** on Android the system splash (12+) and the pre-12 launch window follow the OS night mode, not
  the app's own choice. Making them follow the app needs `UiModeManager.setApplicationNightMode` (API 31+),
  which rewrites the configuration the shell reads the OS appearance from (`Core.kt`), so the System appearance
  query and stream (ADR-0020) would report the app's choice instead. **ADR-0041** records that the shell never
  sets an app-level night mode. Its conformance test asserts no shell source calls `setApplicationNightMode` or
  `setDefaultNightMode`.
- No shell-side "last appearance" hint: on Android the only pre-first-frame surface is the system launch
  window, which a running app cannot paint.

### 3. `ColorRoles.selection`

`pub selection: Option<Rgb>`, appended to `ColorRoles`. Doc: "Focus and selection marks: the focused field's
border, label and cursor, checked toggles and checkboxes, sliders, progress. Unset: the shell's current colour."

Uses (unset = exactly today's colour at each site):
- **Android:**
  - `paletteFieldColors`: `focusedBorderColor`, `focusedLabelColor`, `cursorColor` and the text-selection
    handles/highlight (`TextSelectionColors`), for both `TextField` and `SearchField`.
  - `Switch` (checked track; thumb stays as today), `Checkbox` (checked box), `Slider` (thumb, active track).
  - `LinearProgressIndicator`, `CircularProgressIndicator`, `PullToRefreshBox`'s indicator.
- **iOS:**
  - A local `.tint(selection)` on `Toggle`, `Slider`, determinate `ProgressView` and the text fields (cursor).
    The scaffold-wide `.tint(primary)` stays for everything else.
  - The checkbox's checked colour: `selection`, else today's `Color.accentColor`.
  - Field focus border: `PaletteFieldStyle` gains `@FocusState`; when focused **and** `selection` is set, the
    border is `selection`. Unset: no focus border, as today.
- **Web:**
  - `palette_css` emits `--selection` only when set.
  - `mobiler.css`: checkbox and slider `accent-color: var(--selection, var(--primary))`; toggle checked
    `var(--selection, var(--primary))`; progress `var(--selection, var(--pal-primary, var(--accent)))`.
  - Field focus: `.field:focus` and `.search-input:focus` get a border colour (and for `.field`, the outline)
    from `--selection` through a fallback that leaves today's rendering when it is unset.
- **Not changed:** filled buttons, the selected calendar day and the selected chip/segment fills. They are fills
  with text on them and keep `primary` / `secondary_container`. Native date and time picker dialogs are not
  palette-aware today and stay so.

### 4. `ColorRoles.error`, `error_fill`, `on_error_fill`

Appended after `selection`, all `Option<Rgb>`:
- `error`: "Error text and the invalid field's border; outlined and text Danger buttons. Unset: `danger`'s
  on-container colour, as today."
- `error_fill` / `on_error_fill`: "A filled Danger button's background and its text. Unset: the `danger` pair,
  swapped, as today."

Uses:
- **Android:** `paletteScheme` maps M3 `error` ← `error ?: danger.onContainer` (so the field error, its border
  and the destructive confirm button follow it). `toneStrong` for Danger: filled ← `error_fill` /
  `on_error_fill` when set; outlined and text ← `error` when set; else today's mapping.
- **iOS:** the field error text: `error`, else today's `.red`. `paletteColors` for a filled Danger button:
  `error_fill` / `on_error_fill` when set; outlined and text Danger: `error` when set. The system confirm alert
  stays system red (UIKit can't theme it).
- **Web:** `palette_css` emits `--error`, `--error-fill`, `--on-error-fill` only when set. `.field-invalid` and
  `.field-error`: `var(--error, var(--danger, #d33))`. `.btn-filled.btn-danger`: background
  `var(--error-fill, var(--tone))`, text `var(--on-error-fill, <today's>)`; outlined and text Danger:
  `var(--error, <today's>)`. The destructive confirm (a filled Danger button) follows.

### 5. ABI and compatibility

- Four fields appended to `ColorRoles` (ADR-0008): a full `ColorRoles { … }` literal stops compiling; apps write
  `..Default::default()`. Barbershop's `dark` palette literal (`demos/barbershop/app-core/src/lib.rs`) gains it.
- `mobiler-ui` field docs: `primary` and the tone pairs' docs mention what `selection` / `error_fill` now take
  over when set.
- The `mobiler-core` README's Upgrading section notes the new roles.

## Testing

- **mobiler-ui:** a round-trip test with the four roles set; `Theme::default()` still has no palette.
- **mobiler-web:** `theme_css_without_palette_is_unchanged` stays byte-identical; a new test that each new
  variable is emitted only when its role is set; a palette with none of the new roles gives byte-identical CSS to
  before.
- **CLI (`upgrade.rs`):** `classify` returns `Seed` for the listed paths (and `Own` still for the rest of
  `Assets.xcassets/`); upgrading an app that lacks the seed files creates them; upgrading an app whose seed file
  was edited leaves it byte-identical; `mobiler new` writes them. Each new test is seen to fail first.
- **ADR conformance:** ADR-0041 (no shell sets an app-level night mode) and ADR-0042 (the seed list and its
  semantics), each proven by a mutation recorded in the ADR.
- **Runtime (Android emulator, barbershop with the Moj Termin palette plus `sel` and `err`):**
  - Cold start on a dark emulator (`adb shell cmd uimode night yes`): no white frame, including the 12+ splash.
    Also once on an API 26 image (no system splash; the launch window is dark).
  - With `selection` = `#4fb3a4` in dark, a focused text field's border measures ≥ 3:1 against its fill
    (sampled from a screenshot).
  - A field error and a filled Danger button show `err` / `err-fill`.
  - With no palette, screenshots of the coffee demo match before and after (ADR-0019).
- **iOS:** CI builds every demo; rendering is checked by review (no simulator on this host).
- **Upgrade smoke after publish:** an app scaffolded with CLI 0.60.2 upgrades with `--apply`, gains the seed files,
  keeps a hand edit to a seed file on a second upgrade, and builds.

## Moj Termin's acceptance checks

- A cold start on a dark emulator shows no white frame, including the system splash.
- With `selection` set to `#4fb3a4`, a focused text field in dark measures at least 3:1 against its fill.
- An app set to Dark on a light phone opens dark from the first frame. Covered by keeping the choice in
  `cx.save` state; the Android system splash still follows the OS (ADR-0041), which we tell them.
- A field error and a filled Danger button show the design's `err` / `err-fill`.
