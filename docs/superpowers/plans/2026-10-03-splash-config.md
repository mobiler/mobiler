# `[splash]` Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** a `[splash]` section in `mobiler.toml` (light and dark background, optional logo with a dark variant, a
size) that the CLI syncs into the Android, iOS and web shells.

**Architecture:**
- **New module:** `mobiler/src/splash.rs`. It parses and validates the config, and generates every file's content
  with pure functions.
- **The sync:** writes the files, respects app-owned seed files (header or stock-content check), and undoes itself
  when the section is removed.
- **Triggers:** wired like `[fonts]` (`mobiler build`/`dev`/`watch` and a `mobiler splash sync` command).
- **Shared helper:** marked blocks reuse fonts' `replace_block`, made `pub(crate)` with a neutral error message.
- **Android 12:** the icon attribute goes into a marker block in the framework's v31 themes. It is filled only when a
  logo is set, so no logo means no change at all.

**Tech Stack:** Rust (the `mobiler` CLI), `toml`/`serde`, the `image` crate (PNG only), Android resources, an iOS
asset catalog and xcodegen `project.yml`, and Trunk's `index.html`.

**Spec:** `docs/superpowers/specs/2026-10-03-splash-config-design.md`

## Global Constraints

- **Release:** CLI 0.65.0. No library change. Barbershop first, then the template, ported by patch (ADR-0015).
- **Config:** `background` is required, `#RRGGBB` or `#AARRGGBB`. `background_dark` defaults to `background`.
  `logo`/`logo_dark` are PNG paths relative to the app root. `logo_size` is in dp/pt, default 120, range 24..=288.
- **Resizing:** the logo is fitted inside a `logo_size` square, keeping its aspect ratio, and never upscaled. A source
  smaller than the 4× target is used at its own size, with a warning.
- **It never fails a build:** an invalid config or an unreadable logo warns and leaves every previously synced file
  unchanged.
- **Ownership:** a seed file is overwritten only when it equals the stock template content or carries the header
  `generated from mobiler.toml [splash]; edit mobiler.toml, not this file`. Otherwise it warns, once per file.
- **Hard Android rule:** nothing but resources under `res/` (ADR-0043). A drawable resource named
  `mobiler_splash_icon` already exists, as an alias in the seed `values/mobiler_splash.xml`. The generated Android 12
  icon drawable is therefore named `mobiler_splash_logo_icon`.
- **Dependency:** `image = { version = "0.25", default-features = false, features = ["png"] }`, Lanczos3.
- **Build hygiene:** builds use `CARGO_TARGET_DIR=/media/zmilan/data2/cargo-target-mobiler` (demos:
  `…-barbershop`), because the root disk is tight. Never wipe `~/.gradle`. Emulators only on ports 5560/5562, and
  never `pgrep -f`.
- **Approvals:** every crates.io publish and `v*` tag needs the maintainer's approval. The user runs merge and tag.

## Review Focus

1. **An app that predates the markers** (no splash block in `project.yml` or the v31 themes) must warn "run
   `mobiler upgrade --apply`" and still sync the colours. → Task 2 test `missing_markers_warn_and_colours_still_sync`.
2. **A logo path with spaces or non-ASCII characters**, or a huge PNG (8000²), must work or warn, never panic.
   → Task 2 test `huge_and_odd_named_logo`.
3. **Removing just `logo`** while keeping `[splash]` must restore the colour-only drawable and remove the PNGs and the
   icon line. → Task 2 test `removing_the_logo_undoes_logo_files`.
4. **A CRLF `project.yml` or `index.html`** must keep its line endings (`replace_block` already does). → Task 2 test
   `crlf_files_keep_their_endings`.
5. **`#AARRGGBB` with alpha < FF** is valid on Android, but iOS launch colours ignore alpha, and Android 12's
   `windowSplashScreenBackground` needs an opaque colour. It must warn and use the colour as opaque on those two.
   → Task 1 test `translucent_background_warns_and_is_opaque_where_required`.

---

### Task 1: config, colours, sizes and generated content (pure functions)

**Files:**
- Create: `mobiler/src/splash.rs`
- Modify: `mobiler/src/main.rs` (add `mod splash;`)

**Interfaces:**
- Produces:
  - `pub struct SplashSpec { background: String, background_dark: Option<String>, logo: Option<String>, logo_dark: Option<String>, logo_size: Option<u32> }` (serde `Deserialize`, `deny_unknown_fields`)
  - `pub struct Rgba { r: u8, g: u8, b: u8, a: u8 }` with `fn parse(s: &str) -> Result<Rgba, String>`, `fn android(&self) -> String` (`#AARRGGBB`), `fn opaque(self) -> Rgba`
  - `pub struct Splash { light: Rgba, dark: Rgba, logo: Option<String>, logo_dark: Option<String>, size: u32, warnings: Vec<String> }`
  - `pub fn validate(spec: &SplashSpec) -> Result<Splash, String>`
  - `pub fn fit(src_w: u32, src_h: u32, size: u32) -> (u32, u32)` (dp box)
  - `pub const HEADER: &str = "generated from mobiler.toml [splash]; edit mobiler.toml, not this file";`
  - `pub fn android_colors_xml(c: Rgba, with_icon_alias: bool) -> String`
  - `pub fn android_launch_xml(logo: Option<(u32, u32)>) -> String`
  - `pub fn android_icon_xml(w: u32, h: u32) -> String` (the inset drawable for Android 12)
  - `pub fn android_theme_icon_lines(on: bool) -> Vec<String>`
  - `pub fn ios_colorset_json(light: Rgba, dark: Rgba) -> String`
  - `pub fn ios_imageset_json(has_dark: bool) -> String`
  - `pub fn ios_plist_lines(on: bool) -> Vec<String>`
  - `pub fn web_lines(light: Rgba, dark: Rgba, logo: Option<(u32, u32)>, has_dark: bool) -> Vec<String>`

- [ ] **Step 1: Write the failing tests** (at the end of `splash.rs`):

```rust
#[cfg(test)]
mod tests {
    use super::*;

    fn spec(bg: &str) -> SplashSpec {
        SplashSpec { background: bg.into(), background_dark: None, logo: None, logo_dark: None, logo_size: None }
    }

    #[test]
    fn colours_parse_rgb_and_argb() {
        assert_eq!(Rgba::parse("#FFFBFE").unwrap(), Rgba { r: 0xFF, g: 0xFB, b: 0xFE, a: 0xFF });
        assert_eq!(Rgba::parse("#801C1B1F").unwrap(), Rgba { r: 0x1C, g: 0x1B, b: 0x1F, a: 0x80 });
        assert_eq!(Rgba::parse("#fffbfe").unwrap().android(), "#FFFFFBFE");
        for bad in ["FFFBFE", "#FFF", "#GGGGGG", "", "#FFFBFE00FF"] {
            assert!(Rgba::parse(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn validate_applies_defaults_and_ranges() {
        let s = validate(&spec("#FFFFFF")).unwrap();
        assert_eq!((s.dark, s.size), (s.light, 120));
        let mut bad = spec("#FFFFFF");
        bad.logo_size = Some(500);
        assert!(validate(&bad).unwrap_err().contains("24..=288"));
        assert!(validate(&spec("white")).is_err());
    }

    #[test]
    fn translucent_background_warns_and_is_opaque_where_required() {
        let s = validate(&spec("#80FFFFFF")).unwrap();
        assert!(s.warnings.iter().any(|w| w.contains("opaque")), "{:?}", s.warnings);
        assert!(ios_colorset_json(s.light, s.dark).contains("\"alpha\" : \"1.000\""));
    }

    #[test]
    fn fit_keeps_aspect_and_never_upscales_the_box() {
        assert_eq!(fit(1000, 500, 120), (120, 60));
        assert_eq!(fit(500, 1000, 120), (60, 120));
        assert_eq!(fit(800, 800, 120), (120, 120));
        assert_eq!(fit(1, 1000, 120), (1, 120)); // never 0
    }

    #[test]
    fn generated_android_files() {
        let c = Rgba::parse("#1C1B1F").unwrap();
        let colors = android_colors_xml(c, true);
        assert!(colors.contains(HEADER));
        assert!(colors.contains(r#"<color name="mobiler_splash_background">#FF1C1B1F</color>"#));
        assert!(colors.contains(r#"<drawable name="mobiler_splash_icon">@mipmap/ic_launcher</drawable>"#));
        assert!(!android_colors_xml(c, false).contains("mobiler_splash_icon"));

        let plain = android_launch_xml(None);
        assert!(plain.contains("@color/mobiler_splash_background") && !plain.contains("mobiler_splash_logo"));
        let with = android_launch_xml(Some((120, 60)));
        assert!(with.contains(r#"android:width="120dp""#) && with.contains(r#"android:height="60dp""#));
        assert!(with.contains(r#"android:gravity="center""#) && with.contains("@drawable/mobiler_splash_logo"));

        // Android 12 shows the icon in a 240dp box masked to a 160dp circle: fit inside its 113dp square.
        let icon = android_icon_xml(120, 60);
        assert!(icon.contains("<inset") && icon.contains("@drawable/mobiler_splash_logo"));
        assert!(icon.contains(r#"android:insetLeft="63.5dp""#), "{icon}"); // (240 - 113) / 2
        assert_eq!(android_theme_icon_lines(false), Vec::<String>::new());
        assert_eq!(
            android_theme_icon_lines(true),
            [r#"<item name="android:windowSplashScreenAnimatedIcon">@drawable/mobiler_splash_logo_icon</item>"#]
        );
    }

    #[test]
    fn generated_ios_and_web() {
        let (l, d) = (Rgba::parse("#FFFBFE").unwrap(), Rgba::parse("#1C1B1F").unwrap());
        let cs = ios_colorset_json(l, d);
        assert!(cs.contains("\"red\" : \"0xFF\"") && cs.contains("\"luminosity\"") && cs.contains("\"red\" : \"0x1C\""));
        let is = ios_imageset_json(true);
        assert!(is.contains("mobiler-splash-logo@3x.png") && is.contains("mobiler-splash-logo-dark@3x.png"));
        assert!(!ios_imageset_json(false).contains("dark"));
        assert_eq!(ios_plist_lines(true), ["UIImageName: MobilerSplashLogo"]);
        assert!(ios_plist_lines(false).is_empty());
        let web = web_lines(l, d, Some((120, 60)), true).join("\n");
        assert!(web.contains("prefers-color-scheme: dark") && web.contains("#FFFBFE") && web.contains("#1C1B1F"));
        assert!(web.contains("splash/mobiler-splash-logo.png") && web.contains("splash/mobiler-splash-logo-dark.png"));
        assert!(web.contains("body:empty") && web.contains("120px 60px"));
        assert!(web.contains(r#"rel="copy-dir" href="splash""#));
    }
}
```

- [ ] **Step 2: Run them; expect a compile failure** (nothing is defined yet).

Run: `CARGO_TARGET_DIR=/media/zmilan/data2/cargo-target-mobiler cargo test -q -p mobiler splash 2>&1 | tail -5`

- [ ] **Step 3: Implement the module.** Key code:

```rust
//! `mobiler.toml` `[splash]`: the launch screen's background (light/dark) and an optional logo, synced
//! into the Android, iOS and web shells (ADR-0048). Pure generators here; `sync` writes them.

use serde::Deserialize;

pub const HEADER: &str = "generated from mobiler.toml [splash]; edit mobiler.toml, not this file";
const ANDROID12_SQUARE_DP: u32 = 113; // the square inscribed in Android 12's 160dp icon circle
const ANDROID12_BOX_DP: u32 = 240;

#[derive(Deserialize, Debug, Clone)]
#[serde(deny_unknown_fields)]
pub struct SplashSpec {
    pub background: String,
    pub background_dark: Option<String>,
    pub logo: Option<String>,
    pub logo_dark: Option<String>,
    pub logo_size: Option<u32>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rgba { pub r: u8, pub g: u8, pub b: u8, pub a: u8 }

impl Rgba {
    pub fn parse(s: &str) -> Result<Rgba, String> {
        let hex = s.strip_prefix('#').ok_or_else(|| format!("{s:?}: a colour is #RRGGBB or #AARRGGBB"))?;
        let byte = |i: usize| u8::from_str_radix(&hex[i..i + 2], 16).map_err(|_| format!("{s:?}: not a hex colour"));
        match hex.len() {
            6 if hex.is_ascii() => Ok(Rgba { r: byte(0)?, g: byte(2)?, b: byte(4)?, a: 0xFF }),
            8 if hex.is_ascii() => Ok(Rgba { a: byte(0)?, r: byte(2)?, g: byte(4)?, b: byte(6)? }),
            _ => Err(format!("{s:?}: a colour is #RRGGBB or #AARRGGBB")),
        }
    }
    pub fn android(&self) -> String { format!("#{:02X}{:02X}{:02X}{:02X}", self.a, self.r, self.g, self.b) }
    pub fn css(&self) -> String { format!("#{:02X}{:02X}{:02X}", self.r, self.g, self.b) }
    pub fn opaque(self) -> Rgba { Rgba { a: 0xFF, ..self } }
}

pub struct Splash { pub light: Rgba, pub dark: Rgba, pub logo: Option<String>, pub logo_dark: Option<String>, pub size: u32, pub warnings: Vec<String> }

pub fn validate(spec: &SplashSpec) -> Result<Splash, String> {
    let light = Rgba::parse(&spec.background)?;
    let dark = spec.background_dark.as_deref().map(Rgba::parse).transpose()?.unwrap_or(light);
    let size = spec.logo_size.unwrap_or(120);
    if !(24..=288).contains(&size) {
        return Err(format!("logo_size {size}: must be in 24..=288 (dp/pt)"));
    }
    let mut warnings = Vec::new();
    if light.a != 0xFF || dark.a != 0xFF {
        warnings.push("a translucent background is used as opaque on iOS and the Android 12+ splash".into());
    }
    Ok(Splash { light, dark, logo: spec.logo.clone(), logo_dark: spec.logo_dark.clone(), size, warnings })
}

pub fn fit(w: u32, h: u32, size: u32) -> (u32, u32) {
    let scale = f64::from(size) / f64::from(w.max(h));
    let side = |v: u32| ((f64::from(v) * scale).round() as u32).max(1);
    (side(w), side(h))
}
```

The generator functions write exactly what the tests assert:
- the XML and JSON files start with a comment carrying `HEADER`; the iOS JSON files don't, because JSON has no
  comments. The imageset and colorset JSON need no header, because ownership of iOS seeds is decided by stock
  content (Task 2);
- iOS colours use `opaque()`;
- `android_icon_xml` computes the inset per side as `(240 - fitted dp) / 2`, using the fitted size of the logo box
  scaled into the 113 dp square (`fit(w, h, 113)`), formatted with one decimal and a trailing `.0` dropped;
- `web_lines` returns the lines that go between the markers (without the markers):
  - a `<link data-trunk rel="copy-dir" href="splash"/>` when there is a logo;
  - a `<style>` with `html{background:<light>}`, `body:empty{background:url(splash/mobiler-splash-logo.png) center/<w>px <h>px no-repeat}`, and an `@media (prefers-color-scheme: dark)` block for the dark colour and logo.

- [ ] **Step 4: Run the tests until they pass, then clippy.**

Run: `… cargo test -q -p mobiler splash` and `… cargo clippy -q -p mobiler -- -D warnings 2>&1 | grep splash.rs`. Expected: all pass, no splash.rs findings.

- [ ] **Step 5: Commit:** `feat(cli): [splash] config and generated content (pure)`.

### Task 2: the sync (files, logo resizing, ownership, removal)

**Files:**
- Modify:
  - `mobiler/src/splash.rs`: add `sync`, `SyncReport`, `stock` and the file I/O;
  - `mobiler/src/fonts.rs`: make `replace_block`, `write_if_changed` and `Manifest` `pub(crate)`, and change the
    error text to "unbalanced marker lines (fix them by hand)" and "no anchor to insert the block at";
  - `mobiler/Cargo.toml`: the `image` dependency.

**Interfaces:**
- Consumes everything from Task 1.
- Produces:
  - `pub struct SyncReport { pub written: Vec<String>, pub removed: Vec<String>, pub warnings: Vec<String> }`
  - `pub fn sync(root: &Path) -> anyhow::Result<SyncReport>`
  - `pub fn stock_seeds() -> Vec<(&'static str, &'static [u8])>`: the stock template content of each seed path, read
    from the embedded `TEMPLATES` (`crate::upgrade::TEMPLATES`, made `pub(crate)`) with `{{NAME}}` and
    `{{PACKAGE}}` substitution not needed, since the seeds have none. Assert that in a test.

**Markers** (the template gains them in Task 4; the sync never inserts them into a framework file):
- Android `values-v31/mobiler_themes.xml` and `values-night-v31/mobiler_themes.xml`, inside the `Base.Theme.{{NAME}}`
  style: `<!-- mobiler:splash-begin -->` and `<!-- mobiler:splash-end -->`.
- iOS `project.yml`, under `UILaunchScreen:` after `UIColorName`: `# mobiler:splash-begin` and `# mobiler:splash-end`.
- `web/index.html`: inserted before `</head>` if absent. It's an app file; fonts does the same.

**File map the sync writes:**

| Path | Owner | Written when |
|---|---|---|
| `Android/app/src/main/res/values/mobiler_splash.xml` | seed | always (light colour, with the icon alias line) |
| `Android/app/src/main/res/values-night/mobiler_splash.xml` | seed | always (dark colour, without the alias) |
| `Android/app/src/main/res/drawable/mobiler_launch.xml` | seed | always (with or without the logo item) |
| `Android/app/src/main/res/drawable-xxxhdpi/mobiler_splash_logo.png` | sync | logo |
| `Android/app/src/main/res/drawable-night-xxxhdpi/mobiler_splash_logo.png` | sync | logo_dark |
| `Android/app/src/main/res/drawable/mobiler_splash_logo_icon.xml` | sync | logo |
| the two v31 theme marker blocks | sync | logo → the item line; else empty |
| `iOS/Sources/Assets.xcassets/MobilerSplashBackground.colorset/Contents.json` | seed | always |
| `iOS/Sources/Assets.xcassets/MobilerSplashLogo.imageset/{Contents.json, mobiler-splash-logo@3x.png, mobiler-splash-logo-dark@3x.png}` | sync | logo |
| `iOS/project.yml` marker block | sync | logo → `UIImageName: MobilerSplashLogo`; else empty |
| `web/index.html` block + `web/splash/*.png` | sync | if `web/index.html` exists |

- [ ] **Step 1: Write the failing tests.** Use a temp app root helper `app()` that writes the stock seeds (from
  `stock_seeds()`), the two v31 theme files and `iOS/project.yml` with markers, and a `mobiler.toml`. PNG fixtures
  come from `image::RgbaImage::from_pixel(w, h, …).save(path)`.

```rust
#[test]
fn colours_only_rewrites_stock_seeds() { /* [splash] background only → both values XMLs hold the colours + HEADER;
    mobiler_launch.xml has no logo item; no PNGs; theme blocks and project.yml block empty */ }
#[test]
fn logo_writes_every_platform() { /* 1200×600 logo, size 120 → drawable-xxxhdpi PNG is 480×240; launch item 120×60dp;
    mobiler_splash_logo_icon.xml exists; both theme blocks hold the item line; imageset PNG is 360×180 (3×);
    project.yml block holds UIImageName; web/index.html (present) gets the block + web/splash/ PNGs */ }
#[test]
fn small_logo_is_not_upscaled_and_warns() { /* 100×100 logo, size 120 → PNG stays 100×100; a warning names the 480px target */ }
#[test]
fn hand_edited_seed_is_left_alone_with_a_warning() { /* edit values/mobiler_splash.xml by hand → unchanged after sync,
    warning names the file; the other seeds are still written */ }
#[test]
fn second_sync_is_a_no_op() { /* sync twice → second report has no `written` entries, and file mtimes are unchanged */ }
#[test]
fn broken_logo_leaves_everything_unchanged() { /* sync with a good logo, then point logo at a non-PNG → report has a
    warning, every file is byte-identical to before */ }
#[test]
fn removing_the_logo_undoes_logo_files() { /* logo then no logo → PNGs, icon xml, imageset gone; launch xml colour-only;
    theme and plist blocks empty; colours kept */ }
#[test]
fn removing_splash_undoes_the_sync_but_keeps_colours() { /* drop the section → logo files gone, blocks empty, the web
    block removed, colour seeds as last written */ }
#[test]
fn missing_markers_warn_and_colours_still_sync() { /* project.yml and themes without markers → a warning says run
    `mobiler upgrade --apply`; colours still written */ }
#[test]
fn crlf_files_keep_their_endings() { /* CRLF project.yml → the block is written, every line still ends in \r\n */ }
#[test]
fn huge_and_odd_named_logo() { /* "assets/logo ć.png", 4000×4000 → written at 480×480, no panic */ }
#[test]
fn no_mobiler_toml_or_no_section_touches_nothing() { /* no file changes, empty report */ }
#[test]
fn stock_seeds_are_the_template_and_have_no_placeholders() { /* every SEED_PATHS entry has stock content without "{{" */ }
```

  Each `/* … */` is the test body to write with plain `fs` assertions. Spell out every assertion listed. The tests
  are the contract, so none is optional.

- [ ] **Step 2: Run them; expect failures** (`sync` is missing).

- [ ] **Step 3: Implement `sync`.**
  - **Read the section.** Read `mobiler.toml` with a local `#[derive(Deserialize)] struct Manifest { splash: Option<SplashSpec> }`; unknown sections are ignored.
  - **No section:**
    - if any sync-owned file exists, or a marker block isn't empty, undo: delete the sync-owned files, empty the
      blocks, remove the web block, rewrite `mobiler_launch.xml` colour-only if it carries the HEADER;
    - otherwise return an empty report.
  - **Validate.** `validate(spec)`; an error becomes a warning and the sync stops before writing.
  - **Decode the logos.** Use `image::open` and check the format is PNG; any failure becomes a warning and stops before
    writing. Then compute `fit` for both.
  - **Resize.** Android gets 4× and iOS 3×, using `imageops::resize` with Lanczos3. Never exceed the source size; if
    clamped, warn.
  - **Seeds** are written only if `owned_by_sync(path)`: the file is missing, or equals `stock_seeds()` content, or
    contains `HEADER` (JSON seeds: missing, or equal to stock, or equal to the last-written content recorded in
    `.mobiler/splash.json`). The ledger records each written seed's fingerprint, so a JSON seed the sync wrote is
    recognised later. Use `fingerprint` from `upgrade.rs`, made `pub(crate)`.
  - **Sync-owned files** are always written or removed.
  - **Marker blocks:** call `crate::fonts::replace_block(text, begin, end, anchor, |indent| lines)`.
    - For framework files with no markers: a warning "…run `mobiler upgrade --apply` to get the splash markers", and
      that file is skipped.
    - For `web/index.html`, the anchor is `</head>`.
  - **Write** every file through `write_if_changed`, and collect `written` and `removed`.

- [ ] **Step 4: Run the tests until they pass; then run the full `cargo test -q -p mobiler` and clippy** (no new
  findings in splash.rs or fonts.rs).

- [ ] **Step 5: Commit:** `feat(cli): [splash] sync — logo resizing, ownership, removal`.

### Task 3: wire it into the CLI

**Files:** modify `mobiler/src/main.rs`, `build.rs`, `dev.rs`, `watch.rs` and `doctor.rs`.

- [ ] **Step 1: Write the failing tests.**
  - `watch_paths_include_logo_files` in `splash.rs`: `watch_paths(root)` returns `mobiler.toml` and the logo files'
    directories.
  - `doctor_line_reports_state` in `splash.rs`: it returns one of "splash: not set (template launch screen)",
    "splash: #FFFBFE / #1C1B1F, logo 120dp", or "splash: mobiler.toml [splash] invalid: …".
- [ ] **Step 2: Run; expect a compile failure.**
- [ ] **Step 3: Implement.**
  - **`splash.rs`:**
    - `pub fn sync_for_build(root)` prints warnings as `warning: splash: …`, like fonts.
    - `pub fn run_sync_cli()` prints each written or removed path and the warnings. It prints "No [splash] in
      mobiler.toml — the template launch screen is used." when there's nothing to do.
    - Plus `watch_paths` and `doctor_line`.
  - **`main.rs`:** add `Splash { #[command(subcommand)] cmd: SplashCmd }` with
    `enum SplashCmd { /// Write mobiler.toml [splash] into the Android, iOS and web shells (idempotent). Sync }`, and
    match it like `Fonts`.
  - **`build.rs`/`dev.rs`:** call `crate::splash::sync_for_build(&project.root)` right after the fonts call.
  - **`watch.rs`:** extend the watched paths with `crate::splash::watch_paths`.
  - **`doctor.rs`:** print `crate::splash::doctor_line` after the fonts line.
- [ ] **Step 4: Run** `cargo test -q -p mobiler -p xtask` and clippy. Expected: green.
- [ ] **Step 5: Commit:** `feat(cli): mobiler splash sync; build/dev/watch/doctor run it`.

### Task 4: shell markers, barbershop reference, emulator verification

**Files:**
- **Barbershop:**
  - `demos/barbershop/Android/app/src/main/res/values-v31/mobiler_themes.xml` and `values-night-v31/` (markers);
  - `demos/barbershop/iOS/project.yml` (markers);
  - `demos/barbershop/mobiler.toml` (a `[splash]` section);
  - `demos/barbershop/assets/splash/logo.png` and `logo-dark.png`.
- **Template** (ported by patch): the same marker edits.

- [ ] **Step 1: Add the markers.**
  - **Android:** in each v31 theme, inside the style, after the `windowSplashScreenBackground` item, add two lines:
    `<!-- mobiler:splash-begin -->` and `<!-- mobiler:splash-end -->`.
  - **iOS:** in `project.yml` under `UILaunchScreen:`, after `UIColorName: MobilerSplashBackground`, add
    `# mobiler:splash-begin` and `# mobiler:splash-end` at the same indentation.
  - Keep the `{{NAME}}` and `{{PACKAGE}}` placeholders in the template.
- [ ] **Step 2: Add barbershop's logo.** Generate a simple barbershop logo PNG (1024×1024, transparent, a scissors or
  "FH" monogram drawn with PIL) and a dark variant, both under `assets/splash/`. Then add to `mobiler.toml`:

```toml
[splash]
background = "#FFFBFE"
background_dark = "#1C1B1F"
logo = "assets/splash/logo.png"
logo_dark = "assets/splash/logo-dark.png"
logo_size = 120
```

- [ ] **Step 3: Sync and build.** Run `cd demos/barbershop && <workspace mobiler> splash sync`, then build the
  Android debug APK with `bash Android/build-android.sh`. Commit the synced files: barbershop is the reference.
- [ ] **Step 4: Emulator and web verification** (a background agent; its brief forbids `pgrep -f` and requires data2
  target dirs). Screenshots of the launch window and splash on API 26 and API 36, in light and dark (`cmd uimode night
  yes/no`), captured with `screenrecord` or quick `screencap`s right after `am start -W`.
  - Expected: the background colour plus the centred logo at about 120 dp on API 26. On API 36, the Android 12 splash
    shows the logo inside the icon circle.
  - **No-logo check:** a build with `logo` removed must match the 0.64.2 launch screen on API 36, by pixel comparison
    of the first splash frame. The theme block is empty, so the merged theme is identical.
  - **Web:** a headless-Chrome screenshot of `web/index.html` served before the wasm loads (e.g. with the wasm request
    blocked through CDP), in light and dark.
- [ ] **Step 5: Template port and scaffold.** Port the marker edits to the template by patch and check parity. Then
  `cargo test -q -p mobiler -p xtask`. Scaffold a scratch app with the workspace CLI, add a `[splash]` with a logo,
  run `mobiler build android` to an APK, and delete the scratch app.
- [ ] **Step 6: Commit:** `feat: splash markers in the shells; barbershop uses [splash] (reference)`.

### Task 5: ADR-0048, conformance, READMEs, version

**Files:**
- Create: `docs/adr/ADR-0048-splash-sync.md`.
- Modify:
  - `docs/adr/index.md`, `xtask/tests/adr_conformance.rs`;
  - `mobiler/README.md` (a `[splash]` section next to "Launch window colours", which now points to it), root
    `README.md`;
  - `mobiler/Cargo.toml` (0.65.0) and the lockfile.

- [ ] **Step 1: Write the conformance test.** `adr_0048_splash_markers_and_seeds`:
  - the template's two v31 theme files contain exactly one `mobiler:splash-begin` and one `mobiler:splash-end`;
  - the template's `project.yml` has them under `UILaunchScreen:`;
  - every seed path the sync writes (the three Android seeds and the colorset) is in `SEED_PATHS`. Read `SEED_PATHS`
    from `mobiler/src/upgrade.rs` by text, the way the existing xtask tests read source files.
- [ ] **Step 2: Mutation proofs.** Delete the marker from one template v31 theme, and drop one path from `SEED_PATHS`.
  Each must fail the test. Restore, and record both in ADR §4.
- [ ] **Step 3: Write ADR-0048** in the standard layout. Its headline: "`mobiler.toml [splash]` is synced into the
  shells by the CLI. It writes app-owned seed files only while they are stock or carry its header, fills marker blocks
  in framework files, and owns only the logo files it creates." Conformance: the test above, plus the Task 2 tests
  `hand_edited_seed_is_left_alone_with_a_warning`, `broken_logo_leaves_everything_unchanged` and
  `removing_splash_undoes_the_sync_but_keeps_colours`. Their mutation proofs:
  - ignore the HEADER check;
  - write before validating the logo;
  - skip the undo.

  Each must be run and recorded. Add the index row, and cite ADR-0042 and ADR-0022.
- [ ] **Step 4: READMEs.** `mobiler/README.md` gets the config block, the per-platform behaviour (including the
  Android 12 circle limit and the iOS upgrade note), the ownership rule and `mobiler splash sync`. The root `README.md`
  points to it from the theming and launch text: grep `README.md` for "Launch window", "splash" and "mobiler_splash".
- [ ] **Step 5: Bump the version** to 0.65.0. Run `cargo update -q -p mobiler`, then `cargo test -q -p mobiler -p
  xtask`.
- [ ] **Step 6: Commit:** `docs(adr): ADR-0048 [splash] sync; CLI 0.65.0`.

### Task 6: reviews, upgrade check, PR, release

- [ ] **A fresh opus code and security review** of the branch. Focus on file writes, path handling of `logo`
  (`../`, absolute paths outside the app: allowed as read-only sources, but warn?), PNG decoding of untrusted files,
  and the ownership rule. Fix Critical and Important findings test-first, and re-review the fix pass.
- [ ] **Upgrade end to end** (an agent): scaffold with 0.64.2, add `[splash]` with a logo, run the workspace CLI's
  `upgrade --apply` (the markers arrive) and then `build android`. Expect an APK and no non-resource file in `res/`.
- [ ] **PR** (ship-pr). The body lists the READMEs checked. CI green. The user merges.
- [ ] **Release CLI 0.65.0.** Packaging pre-check, then the user tags after approval. Then post-release: a smoke test
  from crates.io, plus updating `start.md` and memory.
