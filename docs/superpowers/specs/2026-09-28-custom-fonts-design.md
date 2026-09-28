# Custom fonts: a display and a body family from bundled files

**Request:** `docs/custom-fonts.md` (Moj Termin design set, priority 3, blocking).
**Release:** part of the single Moj Termin design release. Nothing is published until the whole set
is done. This work lands on `main` as CLI + core + mobiler-web + **barbershop** shells. The template
shells are ported in the release CLI PR (see memory `template-shell-lands-after-publish`).

## Problem

`Theme.font` is a closed enum of system fonts (`System | Rounded | Serif | Monospace`). No shell can
render a bundled font file. The design needs Space Grotesk (400/500/600) for display text and
Roboto (400/500/700) for body text, on all three platforms.

## Decisions

1. **`mobiler.toml` `[fonts]` is the source of truth.** It is a new app-root config file; the CLI has
   none today.
   - The app keeps its source files where it likes (e.g. `assets/fonts/`) and lists them per role:

     ```toml
     [fonts]
     display = { family = "Space Grotesk", files = ["assets/fonts/SpaceGrotesk-Regular.ttf", "..."] }
     body    = { family = "Roboto",        files = ["assets/fonts/Roboto-Regular.ttf", "..."] }
     ```

   - `family` is optional. If it's missing, the family comes from the first file's name table.
2. **The CLI syncs the fonts into each shell.** `mobiler fonts sync` runs the sync, and so do
   `mobiler build` and `mobiler dev` at the start of every run.
   - The synced copies are ordinary committed files, so a plain Gradle, Xcode or trunk build works too.
   - The sync is idempotent: files a role no longer lists are removed.
3. **Weights come from the files, not their names.** The CLI reads each file's OS/2 `usWeightClass`
   and name table (family, name ID 16 or 1).
   - It accepts TTF and OTF (sfnt `0x00010000`, `true`, `OTTO`).
   - Anything else is skipped with a warning, including woff/woff2, a missing file, an unreadable file
     and a duplicate weight.
   - The sync never fails the build. A role with no usable files is simply not synced, and the shells
     fall back to the system font.
4. **`FontFamily::Custom`** is a new last variant. With it, the display role applies to `Title` and
   `Subtitle` text, the scaffold top-bar title and sheet titles. The body role applies to everything
   else. A role without synced files falls back to the system font, per role.
   - **Breaking for exhaustive matches.** Every shell's `when`/`switch` over `FontFamily` gains the
     `Custom` case, so all demo shells need an update. It's the same situation as the Scaffold pattern
     change, and it's acceptable (no live apps).
5. **Accessibility scaling is preserved:**
   - Android uses `sp`.
   - iOS uses `Font.custom(_:size:relativeTo:)`.
   - Web sizes stay relative.

## What the sync writes

| Shell | Files | Registration |
|---|---|---|
| Android | `app/src/main/res/font/mobiler_<role>_<weight>.ttf` (e.g. `mobiler_display_600.ttf`) | none needed. The shell looks resources up by name at runtime (`getIdentifier`), so there is no compile-time dependency on the files existing |
| iOS | `iOS/Sources/Fonts/mobiler-<role>-<weight>.<ext>`. `Sources/` is already an xcodegen source dir, so files there become bundle resources | `project.yml` `info.properties`: `UIAppFonts` lists those paths (`Fonts/…`), plus `MobilerFontDisplay` / `MobilerFontBody` = the family name. The sync writes a CLI-owned block between `# mobiler:fonts-begin` / `# mobiler:fonts-end` markers, which it replaces on each run and inserts above `# mobiler:info-plist` if missing |
| Web (only if `web/index.html` exists) | `web/fonts/mobiler-<role>-<weight>.<ext>` + `web/fonts/fonts.css` (`@font-face` per file: `font-family: "mobiler-display"` / `"mobiler-body"`, `font-weight: <w>`) | `index.html` gets a CLI-owned block between `<!-- mobiler:fonts-begin -->` / `<!-- mobiler:fonts-end -->` containing `<link data-trunk rel="copy-dir" href="fonts"/>` + `<link rel="stylesheet" href="fonts/fonts.css"/>`, inserted before `</head>` if missing |

When a role (or all of `[fonts]`) goes away, the sync deletes that role's synced files, writes the
iOS/web blocks without it, and, if nothing is left, writes empty blocks.

**Warnings.**
- `mobiler build`/`dev`/`fonts sync` print one line per skipped file, naming the reason.
- `mobiler doctor` reports the `[fonts]` state.

## Shells

- **Android (`ui/theme/Type.kt` + MainActivity text mapping).**
  - `fontFamilyFor(role)` looks up `mobiler_<role>_<w>` for w in 100..900 step 100 via
    `resources.getIdentifier(name, "font", packageName)`.
  - It builds `FontFamily(Font(resId, FontWeight(w)) …)`, cached per role, or returns `null` if none
    are found.
  - With `Custom`, the Typography's title/headline/display slots use the display family (or Default),
    and the body/label slots use the body family.
  - The widget text mapping (`TextStyle.TITLE/SUBTITLE` → display, the rest → body), the top-app-bar
    title and the sheet title follow it.
- **iOS (`Render.swift`).**
  - `CustomFonts.display` / `.body` come from `Bundle.main.infoDictionary["MobilerFontDisplay"/"MobilerFontBody"]`.
    They're used only if `UIFont.fontNames(forFamilyName:)` is non-empty (i.e. the family actually
    registered).
  - With `Custom`, `TextStyleMod` uses `Font.custom(family, size: <the style's current point size>, relativeTo: <the style's text style>)`
    plus the style's current weight.
  - The scaffold title and the sheet title use display.
  - Without a registered family, the system design is used exactly as today.
- **Web (`mobiler-web`).**
  - `theme_css` for `Custom` sets `--font: "mobiler-body", <system stack>` and
    `--font-display: "mobiler-display", var(--font)`.
  - `mobiler.css`: `.t-title`, `.t-subtitle`, `.topbar .title` and the sheet title read
    `var(--font-display, var(--font))`. The fallback keeps other fonts byte-identical.

## ABI

- `FontFamily { System, Rounded, Serif, Monospace, Custom }`: `Custom` is appended.
- Doc: "`Custom` = the app's `mobiler.toml` `[fonts]` (display for titles, body for the rest), synced
  into the shells by the CLI. A role without files falls back to the system font."

## Demo

- Barbershop ships `assets/fonts/` with:
  - Space Grotesk 400/500 (static, from the SpaceGrotesk 2.0.0 release) and 600 (the Google Fonts
    static instance)
  - Roboto 400/500/700 (roboto-3-classic `android/static`)
  - the OFL licence file of each family
- It gets a `mobiler.toml` `[fonts]` listing them, and `theme.font = FontFamily::Custom`.
- The synced shell copies are committed.

## Verification

- **CLI unit tests.**
  - The sfnt reader (weight + family) is tested on a tiny synthetic font built in the test, plus one
    real file if a fixture is small enough.
  - The sync on a temp app tree covers: the Android names; the iOS block contents with
    `UIAppFonts`/keys; `web/fonts.css` + the index block; the idempotence of a second run; role
    removal cleaning up; and a missing, unreadable or woff2 file giving a warning and a skip.
- **Core/ui:** a round-trip test of `FontFamily::Custom`.
- **Web (CDP):**
  - With barbershop, `document.fonts.check('600 16px "mobiler-display"')` returns true.
  - Computed `font-family` on `.t-title` starts with `"mobiler-display"` and on `.t-body` with
    `"mobiler-body"`.
  - No exceptions.
  - Coffee/todo stay pixel-identical to a `main` build.
- **Android (AVD).**
  - Barbershop screenshot. The title glyph shapes differ visibly from Roboto; compare a crop against
    a render with `Custom` removed.
  - Delete one synced file and rebuild: the build warns, the app runs, and that weight falls back.
  - Coffee is unchanged vs `main`.
- **iOS:** CI compile only.

## Out of scope

- The type scale sizes, including the 36/24 display/headline sizes (request 4, which builds on the roles).
- Variable fonts. They are accepted as files, but weight axes aren't instanced.
- Italic.
- Per-widget font overrides.
