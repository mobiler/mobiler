# `[splash]` in mobiler.toml: launch colours and a logo, synced into every shell

**Asked by:** the maintainer. On 2026-10-01 they asked that app builders such as the appointments team can set their
own launch screen. On 2026-10-03 they scoped it: colours plus a logo.
**Release:** CLI 0.65.0. No library change.
**Constrained by:**
- ADR-0022: `mobiler.toml` sections are synced into the shells by `mobiler build`/`dev`/`watch` and a `sync`
  command; a bad input warns and never fails a build.
- ADR-0039: Android minimum 26; newer APIs behind version qualifiers.
- ADR-0041: the shells never set an app-level night mode; light/dark resources follow the system.
- ADR-0042: seed files are app-owned; `upgrade` creates them when missing and never touches them. The `[splash]`
  sync is the writer ADR-0042 anticipated.
- ADR-0043: no side files in Android `res/`.
- ADR-0046: upgrade review and pending rules. Template changes reach apps through `mobiler upgrade`.
- ADR-0015: barbershop first, then the template.

**New record:** ADR-0048, the splash sync contract.

## Problem

An app's launch screen today is the template's: a plain background colour, the launcher icon on Android 12+, and
nothing on iOS or the web. To brand it, an app builder must hand-edit several platform files:
- two Android `mobiler_splash.xml` files and a layer-list;
- an iOS colourset and imageset, plus `project.yml`;
- `web/index.html`.

They must also know each platform's rules: Android 12's icon circle, iOS's unscalable launch image, and density
buckets.

## Decisions (approved 2026-10-03)

### 1. Configuration

```toml
[splash]
background      = "#FFFBFE"                       # light; #RRGGBB or #AARRGGBB
background_dark = "#1C1B1F"                       # optional; default = background
logo            = "assets/splash/logo.png"        # optional; a PNG, transparent background
logo_dark       = "assets/splash/logo-dark.png"   # optional; default = logo
logo_size       = 120                             # optional; dp / pt, default 120, range 24..=288
```

- `background` is required when `[splash]` is present.
- The logo is fitted inside a `logo_size` × `logo_size` box, keeping its aspect ratio. It is never scaled up: a
  source smaller than the 4× target is used at its own size, with a warning.
- Paths are relative to the app root.

### 2. The sync

- **When it runs:** `mobiler build`, `mobiler dev` and `mobiler watch` run it, as they do the fonts sync, and
  `mobiler watch` also watches `mobiler.toml` and the logo files. `mobiler splash sync` runs it on demand and prints
  what it wrote.
- **It never fails a build.** A bad colour, an out-of-range size, or an unreadable or non-PNG logo prints a warning
  and leaves every previously synced file unchanged, as `[fonts]` does with an unreadable font.
- **It is idempotent:** a file is rewritten only when its content differs.
- **Resizing** uses the `image` crate with only PNG support (`default-features = false, features = ["png"]`), with
  Lanczos3 filtering.

### 3. What it writes

**Android:**

| File | Content |
|---|---|
| `res/values/mobiler_splash.xml` (seed) | `mobiler_splash_background` = background |
| `res/values-night/mobiler_splash.xml` (seed) | `mobiler_splash_background` = background_dark |
| `res/drawable/mobiler_launch.xml` (seed) | a layer-list of the colour, plus (with a logo) `<item android:gravity="center" android:width="Wdp" android:height="Hdp" android:drawable="@drawable/mobiler_splash_logo"/>`, where W×H is the fitted size |
| `res/drawable-xxxhdpi/mobiler_splash_logo.png` | the logo at 4× the fitted size (Android scales it down for lower densities) |
| `res/drawable-night-xxxhdpi/mobiler_splash_logo.png` | the dark logo, when `logo_dark` is set |
| `res/drawable/mobiler_splash_icon.xml` | the Android 12+ splash icon (below) |

**Android 12+ system splash.** The framework's `values-v31` and `values-night-v31` `mobiler_themes.xml` set
`android:windowSplashScreenAnimatedIcon` to `@drawable/mobiler_splash_icon`. The icon box is 240 dp, and Android masks
it to a 160 dp circle.
- **With a logo**, the sync writes `mobiler_splash_icon.xml` as an `inset` of `@drawable/mobiler_splash_logo`. The
  inset fits the logo's fitted box inside the circle's 113 dp inscribed square, centred. So a logo larger than 113 dp
  shows smaller on Android 12+ than on 8–11. That is an Android limit, and it is documented.
- **Without a logo**, the default `mobiler_splash_icon.xml` must render exactly as today's default splash, which shows
  the launcher icon. The template ships it, and the sync restores it when the logo is removed. This is verified by an
  emulator pixel comparison on API 36. If no drawable can match today's rendering, the theme sets the attribute only
  through the sync. The plan picks the mechanism; the requirement is "no visible change without a logo".

**iOS:**

| File | Content |
|---|---|
| `Sources/Assets.xcassets/MobilerSplashBackground.colorset/Contents.json` (seed) | background, and background_dark under the dark appearance |
| `Sources/Assets.xcassets/MobilerSplashLogo.imageset/` | `Contents.json` plus the logo at @3x of the fitted size, and the dark logo under the dark appearance |
| `project.yml` | a marked block `# mobiler:splash-begin` … `# mobiler:splash-end` under `UILaunchScreen`, holding `UIImageName: MobilerSplashLogo` (with a logo) |

The template's `project.yml` gains the empty marker block. An app whose `project.yml` lacks it gets a warning: "run
`mobiler upgrade --apply` to get the splash marker". Its logo isn't shown on iOS until then; the colours still work.

**Web** (only when `web/index.html` exists, like fonts):

- **The marked block** `<!-- mobiler:splash-begin -->` … `<!-- mobiler:splash-end -->` goes before `</head>`. It
  holds a `<style>` that sets the page background from the colours, using `@media (prefers-color-scheme: dark)`.
- **With a logo**, the block also centres the logo with a CSS background on `body:empty` (until the shell mounts). It
  uses `<picture>`-free CSS: `background-image` with an `image-set` of the light logo, swapped for the dark one in the
  dark media query.
- **The logo files** go to `web/splash/` and are copied by Trunk through a `copy-dir` link inside the block.
- **Removed `[splash]`:** the block is removed.

### 4. Who owns the files

The seed files stay app-owned. The sync overwrites a seed file only when the file is either:
- byte-identical to the stock template content of the CLI version that wrote it, which the sync knows for every
  released template version since seeds shipped (CLI 0.61.0); or
- marked with the sync's header comment: `generated from mobiler.toml [splash]; edit mobiler.toml, not this file`.

Any other content counts as hand-edited. The sync leaves it alone and warns once per file: "<file> was edited by hand —
remove your edits (or delete the file) to let [splash] manage it".

Files that only the sync creates are the sync's own:
- the logo PNGs;
- `mobiler_splash_icon.xml` when it holds a logo;
- the imageset;
- the marked blocks.

**Removing `[splash]`**, after a sync left files:
- the logo files go;
- `mobiler_launch.xml` goes back to colour-only;
- `mobiler_splash_icon.xml` goes back to the default;
- the `UIImageName` and web blocks are removed;
- the colour files stay as last synced, since they are app-owned again.

It never touches a hand-edited file.

### 5. Upgrade and release

- **Template changes** (ADR-0015, barbershop first):
  - `values-v31/` and `values-night-v31/mobiler_themes.xml` get the icon attribute, if the plan's mechanism needs it;
  - `project.yml` gets the marker block;
  - the default `drawable/mobiler_splash_icon.xml` becomes a seed path, if it is a file the app may own.
- **Existing apps** get these through `mobiler upgrade --apply`. A new seed path is created when missing (ADR-0042).
- **Barbershop** gets a `[splash]` section with a logo, as the reference.
- **Release:** CLI 0.65.0 (minor: a new config section and a new command).

## Testing

**Unit tests** (`mobiler/src/splash.rs`):
- colour parsing and errors;
- size validation;
- the fitted size for landscape, portrait, square and a too-small source;
- each generated file's exact content (both Android XMLs, the layer-list with and without a logo, the inset icon, the
  iOS colourset and imageset JSON);
- marker block insert, replace and remove, for `project.yml` and `index.html`;
- the ownership rule: stock template content, header-marked and hand-edited;
- removal;
- idempotence;
- a broken logo leaves the files unchanged.

**Android emulators** (API 26 and API 36), barbershop, light and dark:
- the launch window with no logo and with a logo;
- on API 36, the Android 12 splash with a logo;
- on API 36 without a logo, a pixel comparison against the 0.64.2 build.

**Web:** a headless-Chrome screenshot of `web/index.html` before the app mounts, light and dark.

**iOS:** CI compile. The launch screen itself needs a device or simulator; that check is deferred and recorded in the
PR.

**ADR-0048 conformance:**
- every seed path the sync writes is in `SEED_PATHS`;
- the template carries the marker block and the theme attribute.

Each is proven by a mutation.

**Upgrade:** an app made with 0.64.2 gets `[splash]` added. Then `mobiler upgrade --apply` and `mobiler build android`
must produce an APK, with no file inside `res/` other than resources.

## READMEs

- **`mobiler/README.md`:** a `[splash]` section next to the "Launch window colours" section, which it supersedes.
- **The root `README.md`:** the upgrade and launch notes.
- **The CLI `--help`** for `mobiler splash sync`.

## Amendments

- **2026-10-03 (plan):** the Android 12 icon uses a marker block in the framework's v31 themes. The sync fills it only
  with a logo, so the no-logo theme is byte-identical to today, and no default drawable has to match it. The generated
  icon drawable is `mobiler_splash_logo_icon`, because `mobiler_splash_icon` is already an alias in the seed
  `values/mobiler_splash.xml`. iOS JSON seeds are recognised as the sync's own through a ledger
  (`.mobiler/splash.json`), since JSON has no comments for a header.
- **2026-10-03 (review of the implementation):**
  - **Android 12 icon geometry:** without an icon background the box is 288 dp, masked to a 192 dp circle, so the
    icon uses percentage insets. The logo keeps its dp size up to the inscribed square (0.4714 of the box, about
    136 dp at 288), in either box.
  - **`web/splash/`** loses only the sync's own logos.
  - **The ledger** stays inactive after removal, and kept seeds say they are app-owned.
  - **Stock detection** reads CRLF as LF.
  - **The fonts sync** no longer touches an app whose `mobiler.toml` has no `[fonts]`.
  - **Undo** runs only when the ledger is active. A missing ledger leaves the sync's files as they are: consistent,
    but stale. This deviates from the plan.
