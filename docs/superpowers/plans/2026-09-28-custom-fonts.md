# Custom Fonts Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** An app lists a display and a body font family in `mobiler.toml` `[fonts]`. The CLI syncs
the files into the Android, iOS and web shells, and `FontFamily::Custom` renders titles in the
display family and everything else in the body family, with a per-role system fallback.

**Architecture:**
- A new CLI module `mobiler/src/fonts.rs`:
  - a TOML manifest
  - an sfnt reader (weight + family)
  - an idempotent sync that writes fixed-name copies and marker-delimited registration blocks
  - `mobiler fonts sync`, plus an automatic call from `build` / `dev` / `watch`
- Shells find the synced fonts at runtime:
  - Android: resource lookup by name
  - iOS: Info.plist family keys, checked against the registered families
  - web: `@font-face` families `mobiler-display` / `mobiler-body`

**Tech Stack:** Rust (clap, toml, serde), Kotlin/Compose M3 Typography, SwiftUI `Font.custom`, CSS `@font-face` via trunk `copy-dir`.

**Spec:** `docs/superpowers/specs/2026-09-28-custom-fonts-design.md`

## Global Constraints

- **Existing font choices (`System`, `Rounded`, `Serif`, `Monospace`, or no theme) render
  byte-identically.** The web `theme_css` output for them is unchanged, and native code paths for them
  are untouched.
- **The sync never fails a build.** Problems are warnings that name the file and the reason, and the
  bad file is skipped.
- **Synced names:**
  - Android: `res/font/mobiler_<role>_<weight>.ttf|otf` (lower-case, underscores, valid resource names).
  - iOS: `iOS/Sources/Fonts/mobiler-<role>-<weight>.<ext>`. `UIAppFonts` lists **bare filenames**,
    because XcodeGen copies source resources flat into the bundle.
  - Web: `web/fonts/mobiler-<role>-<weight>.<ext>` + `web/fonts/fonts.css`.
- **Marker blocks:**
  - iOS `project.yml`: `# mobiler:fonts-begin` … `# mobiler:fonts-end`, 8-space indented keys,
    inserted above `# mobiler:info-plist`.
  - web `index.html`: `<!-- mobiler:fonts-begin -->` … `<!-- mobiler:fonts-end -->`, before
    `</head>`.
- **Roles:**
  - display: `Title`, `Subtitle`, the top-bar title and sheet titles
  - body: everything else
- **Weights:** 100..=900 in steps of 100. A file's weight is rounded to the nearest hundred and
  clamped to that range.
- Don't touch `mobiler/templates/`. Don't bump versions or publish.
- **Build rules** are as in previous plans: per-demo `CARGO_TARGET_DIR`, background long builds, no
  `pgrep` polling, a `main` worktree for baselines. Swift is compiled by CI only.

## Review Focus

1. **Idempotence and cleanup.** A second sync changes nothing. Removing a role or file deletes the
   stale copies and block lines. Pinned by the Task 2 tests `sync_is_idempotent` and
   `removing_a_role_cleans_up`.
2. **Hostile font input.** Truncated or garbage bytes, a woff2 file, a missing file or a
   0-length table must give a warning and a skip, never a panic. Pinned by the Task 1 test
   `sfnt_rejects_garbage_without_panicking` and the Task 2 test `bad_files_are_skipped_with_warnings`.
3. **An app with no `mobiler.toml`.** Sync is a no-op that touches no file, and build/dev behave
   exactly as before. Pinned by the Task 2 test `no_manifest_touches_nothing`.
4. **A marker block missing or duplicated in a hand-edited file.** The sync replaces exactly one
   block, or inserts one at the anchor, and never duplicates it. Pinned by the Task 2 test
   `block_is_replaced_not_duplicated`.
5. **Missing role at runtime.** `Custom` with only `body` synced uses the system font for titles on
   every shell. Pinned by the Task 6 Android deleted-file check and by the web fallback stack in
   `--font-display`.

---

### Task 1: sfnt reader + manifest types (CLI)

**Files:** create `mobiler/src/fonts.rs`; `mobiler/src/main.rs` (`mod fonts;`).

**Interfaces (produces):**
- `pub struct FontInfo { pub family: String, pub weight: u16 }`
- `pub fn read_font_info(bytes: &[u8]) -> Result<FontInfo, String>`. The `Err` is a human reason
  (e.g. "not a TrueType/OpenType file (woff2?)").
- `#[derive(Deserialize, Default)] pub struct Manifest { pub fonts: Option<FontsSection> }`
- `pub struct FontsSection { pub display: Option<RoleSpec>, pub body: Option<RoleSpec> }`
- `pub struct RoleSpec { pub family: Option<String>, pub files: Vec<String> }`

- [ ] **Step 1: Failing tests** (in `fonts.rs` `#[cfg(test)]`), with a helper that builds a minimal
  sfnt:

```rust
    /// A minimal sfnt with just OS/2 (weight) and name (family, nameID 1, Windows UTF-16BE).
    fn tiny_font(weight: u16, family: &str) -> Vec<u8> {
        let name_str: Vec<u8> = family.encode_utf16().flat_map(|u| u.to_be_bytes()).collect();
        let mut name = Vec::new();
        name.extend_from_slice(&0u16.to_be_bytes()); // format
        name.extend_from_slice(&1u16.to_be_bytes()); // count
        name.extend_from_slice(&(6u16 + 12).to_be_bytes()); // stringOffset
        for v in [3u16, 1, 0x409, 1, name_str.len() as u16, 0] { name.extend_from_slice(&v.to_be_bytes()); }
        name.extend_from_slice(&name_str);
        let mut os2 = vec![0u8; 8];
        os2[4..6].copy_from_slice(&weight.to_be_bytes());
        let tables: [(&[u8; 4], &Vec<u8>); 2] = [(b"OS/2", &os2), (b"name", &name)];
        let mut out = Vec::new();
        out.extend_from_slice(&0x0001_0000u32.to_be_bytes());
        out.extend_from_slice(&(tables.len() as u16).to_be_bytes());
        out.extend_from_slice(&[0u8; 6]);
        let mut offset = 12 + 16 * tables.len();
        let mut body = Vec::new();
        for (tag, data) in tables {
            out.extend_from_slice(*tag);
            out.extend_from_slice(&0u32.to_be_bytes());
            out.extend_from_slice(&(offset as u32).to_be_bytes());
            out.extend_from_slice(&(data.len() as u32).to_be_bytes());
            body.extend_from_slice(data);
            offset += data.len();
        }
        out.extend_from_slice(&body);
        out
    }

    #[test]
    fn sfnt_reads_weight_and_family() {
        let i = read_font_info(&tiny_font(600, "Space Grotesk")).unwrap();
        assert_eq!((i.family.as_str(), i.weight), ("Space Grotesk", 600));
    }

    #[test]
    fn sfnt_rejects_garbage_without_panicking() {
        assert!(read_font_info(b"wOF2\0\0\0\0garbage").is_err());
        assert!(read_font_info(&[]).is_err());
        let mut t = tiny_font(400, "X");
        t.truncate(20);
        assert!(read_font_info(&t).is_err());
    }

    #[test]
    fn manifest_parses_roles() {
        let m: Manifest = toml::from_str(r#"[fonts]
display = { family = "Space Grotesk", files = ["a.ttf"] }
body = { files = ["b.ttf", "c.ttf"] }"#).unwrap();
        let f = m.fonts.unwrap();
        assert_eq!(f.display.unwrap().family.as_deref(), Some("Space Grotesk"));
        assert_eq!(f.body.unwrap().files.len(), 2);
    }
```

- [ ] **Step 2: RED.** Run `cargo test -p mobiler fonts:: 2>&1 | grep -E '^error' | head`.

- [ ] **Step 3: Implement.**
  - `read_font_info`:
    - Check the magic (`00010000`, `true`, `OTTO`). Otherwise
      `Err("not a TrueType/OpenType file (woff/woff2 aren't supported natively)")`.
    - Read `numTables` and walk the records with bounds-checked `u16`/`u32` readers (`get(..)` →
      `Err("truncated font file")`).
    - OS/2: `usWeightClass` at +4.
    - name: prefer nameID 16, then nameID 1.
      - Platform 3: decode UTF-16BE.
      - Platform 1: Latin-1/ASCII.
    - Missing tables → `Err("no OS/2/name table")`.
  - Serde structs as listed.

- [ ] **Step 4: GREEN.** Run `cargo test -p mobiler fonts::` and `cargo clippy -p mobiler --all-targets -- -D warnings`.

- [ ] **Step 5: Commit:** `feat(cli): font file reader + mobiler.toml [fonts] manifest`

---

### Task 2: Sync (CLI) + `mobiler fonts sync` + build/dev/watch hook

**Files:** `mobiler/src/fonts.rs`, `mobiler/src/main.rs` (a `Fonts { cmd }` subcommand with a
`sync` action), `mobiler/src/build.rs`, `mobiler/src/dev.rs` (`pipeline` start: covers dev and
watch), `mobiler/src/doctor.rs` (report).

**Interfaces (produces):**
- `pub fn sync(root: &Path) -> anyhow::Result<SyncReport>`, where
  `SyncReport { warnings: Vec<String>, synced: Vec<(String /*role*/, String /*family*/, Vec<u16>)> }`
- `pub fn run_sync_cli() -> anyhow::Result<()>` prints the report.

- [ ] **Step 1: Failing tests** (a temp dir via `tempfile`, or `std::env::temp_dir()` + a unique
  subdir if `tempfile` isn't a dependency). Build an app tree with `Android/app/src/main/res/`,
  `iOS/project.yml` (containing an 8-space `        # mobiler:info-plist` line), `web/index.html`
  (with `</head>`), `assets/fonts/{d400,d600,b400}.ttf` (from `tiny_font`) and `mobiler.toml`. Tests:
  - `sync_writes_every_shell`:
    - `res/font/mobiler_display_400.ttf` and `mobiler_display_600.ttf` exist.
    - `iOS/Sources/Fonts/mobiler-body-400.ttf` exists.
    - `project.yml` has exactly one begin/end block containing
      `UIAppFonts: ["mobiler-body-400.ttf", "mobiler-display-400.ttf", "mobiler-display-600.ttf"]`
      (sorted), `MobilerFontDisplay: "Space Grotesk"` and `MobilerFontBody: "Roboto"`.
    - `web/fonts/fonts.css` has an `@font-face` with `font-family: "mobiler-display"; font-weight: 600;`.
    - `index.html` has one block with `copy-dir` and the stylesheet link.
  - `sync_is_idempotent`: the second sync leaves every written file byte-identical (compare
    contents).
  - `removing_a_role_cleans_up`: drop `display` from the toml and sync. The display files are gone
    from all shells, and the blocks list only body.
  - `bad_files_are_skipped_with_warnings`: add a missing path, a woff2-magic file and a truncated
    file. Sync succeeds, `warnings.len() == 3`, and every warning contains the file name.
  - `no_manifest_touches_nothing`: with no `mobiler.toml`, `sync` writes nothing (no `Fonts` dir, and
    `project.yml` and `index.html` are unchanged).
  - `block_is_replaced_not_duplicated`: pre-seed `project.yml` with an old block, sync, and assert
    exactly one `# mobiler:fonts-begin`.
  - `no_web_dir_is_fine`: with no `web/`, the sync succeeds and the other shells are written.

- [ ] **Step 2: RED.** Run `cargo test -p mobiler fonts::`.

- [ ] **Step 3: Implement.**
  - If there is no `mobiler.toml`, return an empty report without touching any file. This includes
    not cleaning up: with no manifest, mobiler's font copies are the app's business.
  - Otherwise, per role, for each file:
    - read the bytes
    - run `read_font_info`
    - round the weight to the nearest 100 and clamp it to 100..=900
    - dedupe by weight (a duplicate is a warning: "second file for weight N ignored")
  - The family is `spec.family` or the first file's family.
  - Desired set: `(role, weight, ext, bytes)`.
  - Android: ensure `res/font/` exists, write the desired files (only if the bytes differ), and
    delete any `mobiler_*` file not in the desired set.
  - iOS: the same for `iOS/Sources/Fonts/` (`mobiler-*`), then rewrite the marker block. If no roles
    are left, write an empty block (begin/end only).
  - Web: only if `web/index.html` exists. Handle `web/fonts/` the same way, write `fonts.css`, and
    rewrite the index block.
  - Blocks: find begin..end and replace it; if there is none, insert before the anchor
    (`# mobiler:info-plist` / `</head>`). If the anchor is missing, warn and skip that registration.
  - `Command::Fonts { cmd: FontsCmd::Sync }` → `run_sync_cli`.
  - `build::run` and `dev::pipeline`: at the start, `if let Ok(r) = fonts::sync(&project.root) { for w in r.warnings { eprintln!("warning: fonts: {w}") } }`.
    An `Err` also becomes a warning and never aborts the build.
  - `doctor`: one line listing the `[fonts]` roles and files found, or "no [fonts]".

- [ ] **Step 4: GREEN.** Run `cargo test -p mobiler && cargo clippy -p mobiler --all-targets -- -D warnings`.

- [ ] **Step 5: Commit:** `feat(cli): mobiler fonts sync — copy [fonts] into Android/iOS/web shells (auto on build/dev)`

---

### Task 3: ABI + web

**Files:** `mobiler-ui/src/lib.rs` (`FontFamily::Custom` + doc; a round-trip test);
`mobiler-web/src/lib.rs` (`theme_css`); `mobiler-web/src/mobiler.css` (title rules).

- [ ] **Step 1: Failing web test:**

```rust
    #[test]
    fn custom_font_sets_body_and_display_stacks() {
        let t = Theme { font: FontFamily::Custom, ..Default::default() };
        let css = theme_css(&t, false);
        assert!(css.contains("--font:\"mobiler-body\", system-ui"));
        assert!(css.contains("--font-display:\"mobiler-display\", var(--font);"));
        let sys = theme_css(&Theme::default(), false);
        assert!(!sys.contains("--font-display"));
    }
```

  Plus a mobiler-ui round-trip: `round_trips(&Theme { font: FontFamily::Custom, ..Default::default() })`.

- [ ] **Step 2: RED.** `Custom` doesn't exist.

- [ ] **Step 3: Implement.**
  - Append `Custom` to `FontFamily`, with this doc: "The app's `mobiler.toml` `[fonts]`, synced
    into the shells by the CLI: the display family for titles/subtitles, the body family for the
    rest; a role without files falls back to the system font."
  - `theme_css`: `FontFamily::Custom => "\"mobiler-body\", system-ui, -apple-system, \"Segoe UI\", Roboto, sans-serif"`.
    Then, only for `Custom`, append `--font-display:"mobiler-display", var(--font);` after the base
    string (before the palette part).
  - CSS: `.t-title`, `.t-subtitle`, `.topbar .title` and `.sheet-title` get
    `font-family: var(--font-display, var(--font));`. Check with grep that `--font` is what they
    inherit today (`body`/`.scaffold` `font-family: var(--font)`); the Task 6 pixel diff is the
    proof.
  - Fix every Rust exhaustive match on `FontFamily` (the compiler lists them).

- [ ] **Step 4: GREEN + wider.** Run mobiler-ui/core/web tests, clippy, `wasm32 check`, and `cargo check` of every demo workspace.

- [ ] **Step 5: Commit:** `feat(ui,web): FontFamily::Custom — body/display font stacks from the synced fonts`

---

### Task 4: Android (barbershop) + every demo's exhaustive `when`

**Files:** `demos/*/Android/.../ui/theme/Type.kt` (all 5 demos: the `when` gains `CUSTOM`);
barbershop's `Type.kt` and `Theme.kt` (Custom implementation).

- [ ] **Step 1: The other demos** (coffee, saldo, todo, fullstack-todo): in `typographyFor`'s `when`,
  add `CUSTOM` next to `SYSTEM` (→ `FontFamily.Default`). That's a compile fix only, and it keeps
  their look.

- [ ] **Step 2: Barbershop `Type.kt`:**

```kotlin
/** A synced font role (`mobiler fonts sync` → res/font/mobiler_<role>_<weight>), looked up by name so
 *  the shell compiles without it; null when the app synced none (the system font is used). */
fun syncedFamily(context: android.content.Context, role: String): FontFamily? {
    val fonts = (100..900 step 100).mapNotNull { w ->
        val id = context.resources.getIdentifier("mobiler_${role}_$w", "font", context.packageName)
        if (id != 0) androidx.compose.ui.text.font.Font(id, FontWeight(w)) else null
    }
    return if (fonts.isEmpty()) null else FontFamily(fonts)
}

fun typographyFor(font: ModelFontFamily?, context: android.content.Context): Typography {
    if (font == ModelFontFamily.CUSTOM) {
        val display = syncedFamily(context, "display") ?: FontFamily.Default
        val body = syncedFamily(context, "body") ?: FontFamily.Default
        val t = Typography()
        return t.copy(
            displayLarge = t.displayLarge.copy(fontFamily = display), displayMedium = t.displayMedium.copy(fontFamily = display),
            displaySmall = t.displaySmall.copy(fontFamily = display), headlineLarge = t.headlineLarge.copy(fontFamily = display),
            headlineMedium = t.headlineMedium.copy(fontFamily = display), headlineSmall = t.headlineSmall.copy(fontFamily = display),
            titleLarge = t.titleLarge.copy(fontFamily = display), titleMedium = t.titleMedium.copy(fontFamily = display),
            titleSmall = t.titleSmall.copy(fontFamily = body),
            bodyLarge = baseBody.copy(fontFamily = body), bodyMedium = t.bodyMedium.copy(fontFamily = body),
            bodySmall = t.bodySmall.copy(fontFamily = body), labelLarge = t.labelLarge.copy(fontFamily = body),
            labelMedium = t.labelMedium.copy(fontFamily = body), labelSmall = t.labelSmall.copy(fontFamily = body),
        )
    }
    val family = when (font) {
        ModelFontFamily.SERIF -> FontFamily.Serif
        ModelFontFamily.MONOSPACE -> FontFamily.Monospace
        ModelFontFamily.ROUNDED, ModelFontFamily.SYSTEM, ModelFontFamily.CUSTOM, null -> FontFamily.Default
    }
    return Typography(bodyLarge = baseBody.copy(fontFamily = family))
}
```

  - In `Theme.kt`, the call becomes `typographyFor(theme?.font, LocalContext.current)` (the import
    already exists).
  - The mapping: widget `TITLE` → headlineMedium (display), `SUBTITLE` → titleMedium (display), the
    app bar and sheet → titleLarge (display), and everything else → body/label (body). No
    MainActivity change is needed.
  - Cache: `remember(context)` isn't available in a plain function. The lookup is cheap (9
    `getIdentifier` calls per role per theme composition), so it's accepted.

- [ ] **Step 3: Compile.** Background `mobiler build android` in barbershop. **Expected:** APK. The
  other demos compile via CI (their `Type.kt` change is one enum case).

- [ ] **Step 4: Commit:** `feat(android): FontFamily.CUSTOM — synced display/body families (barbershop), CUSTOM case in every demo`

---

### Task 5: iOS (barbershop) + every demo's exhaustive `switch`

**Files:** `demos/*/iOS/Sources/Render.swift` (`fontDesign`: `case .custom: .default` in all 5);
barbershop `Render.swift` (Custom implementation).

- [ ] **Step 1:** In all 5 demos, add `case .custom: .default` to
  `var fontDesign: Font.Design { switch font { … } }`. Grep for any other `switch` over `FontFamily`.

- [ ] **Step 2: Barbershop.**

```swift
/// Synced font families (`mobiler fonts sync` writes MobilerFontDisplay/MobilerFontBody + UIAppFonts):
/// usable only if the family actually registered; nil → the system font.
enum CustomFonts {
    static let display: String? = registered("MobilerFontDisplay")
    static let body: String? = registered("MobilerFontBody")
    private static func registered(_ key: String) -> String? {
        guard let family = Bundle.main.infoDictionary?[key] as? String, !family.isEmpty,
              !UIFont.fontNames(forFamilyName: family).isEmpty else { return nil }
        return family
    }
    static var active: Bool { if case .some(.custom) = ActiveTheme.current?.font { return true }; return false }
    /// `size`/`relativeTo` = the system style it replaces (keeps Dynamic Type).
    static func font(_ family: String?, size: CGFloat, relativeTo: Font.TextStyle) -> Font? {
        guard active, let family else { return nil }
        return Font.custom(family, size: size, relativeTo: relativeTo)
    }
}
```

  - `TextStyleMod`: under `Custom`, use `CustomFonts.font(…) ?? <today's system font>`.
    - `.title`: display, 34 / `.largeTitle`, `.bold()`
    - `.subtitle`: display, 20 / `.title3`, `.weight(.semibold)`
    - `.caption`: body, 13 / `.footnote`
    - `.emphasis`: body, 17 / `.body`, semibold
    - `.body`: body, 17 / `.body`
  - `ScaffoldView`:
    - The top-bar `Text(title).font(.headline)` becomes
      `.font(CustomFonts.font(CustomFonts.display, size: 17, relativeTo: .headline)?.weight(.semibold) ?? .headline)`.
    - The sheet title `.font(.title3.bold())` becomes
      `CustomFonts.font(CustomFonts.display, size: 20, relativeTo: .title3)?.bold() ?? .title3.bold()`.
    - Add a root default: `.font(CustomFonts.font(CustomFonts.body, size: 17, relativeTo: .body) ?? .body)`
      applied only when custom. Use a small modifier that returns `content` unchanged otherwise, so
      buttons and fields inherit the body family.
  - Without `Custom`, every expression evaluates to today's font.

- [ ] **Step 3: Compile scrutiny.**
  - Check `Font.custom(_:size:relativeTo:)` (iOS 14+) and `UIFont.fontNames(forFamilyName:)`.
  - Check the `ActiveTheme` access from a nonisolated static (`nonisolated(unsafe)` precedent).
  - CI is the gate.

- [ ] **Step 4: Commit:** `feat(ios): FontFamily.custom — synced display/body families (barbershop), .custom in every demo`

---

### Task 6: Barbershop fonts + acceptance

**Files:**
- `demos/barbershop/assets/fonts/`: SpaceGrotesk-Regular/Medium (release 2.0.0 static),
  SpaceGrotesk-SemiBold (Google Fonts static instance), Roboto-Regular/Medium/Bold
  (roboto-3-classic `android/static`), `SpaceGrotesk-OFL.txt`, `Roboto-OFL.txt`
- `demos/barbershop/mobiler.toml`
- `demos/barbershop/app-core/src/lib.rs`: `font: FontFamily::Custom`
- The synced shell copies, committed.

The files are already downloaded to the scratchpad `fonts/`. The SemiBold URL comes from the Google
Fonts CSS API with a non-woff user agent.

- [ ] **Step 1:** Copy the six TTFs + two licences, and write `mobiler.toml`:

```toml
# Fonts for FontFamily::Custom — `mobiler build`/`dev`/`fonts sync` copy them into the shells.
[fonts]
display = { family = "Space Grotesk", files = ["assets/fonts/SpaceGrotesk-Regular.ttf", "assets/fonts/SpaceGrotesk-Medium.ttf", "assets/fonts/SpaceGrotesk-SemiBold.ttf"] }
body = { family = "Roboto", files = ["assets/fonts/Roboto-Regular.ttf", "assets/fonts/Roboto-Medium.ttf", "assets/fonts/Roboto-Bold.ttf"] }
```

  - Run `mobiler fonts sync` in the barbershop dir. **Expected:** no warnings. The report lists
    display 400/500/600 and body 400/500/700 (it proves the reader's weights on real files).
  - Set barbershop `font: FontFamily::Custom`. Barbershop core tests must pass.

- [ ] **Step 2: Web acceptance** (CDP, per-run port).
  - `document.fonts.ready`, then check each of these is true:
    - `document.fonts.check('600 16px "mobiler-display"')`
    - `document.fonts.check('400 16px "mobiler-body"')`
  - The computed `font-family` of a `.t-title` (or the topbar `.title`) starts with
    `"mobiler-display"`, and of a `.t-body` or caption with `"mobiler-body"`.
  - No exceptions.
  - Coffee/todo web pixel-identical to a `main`-worktree build.
- [ ] **Step 3: Android acceptance.**
  - Run `mobiler dev` in barbershop and take a screenshot. Crop the "Fade House" title and a body
    line.
  - Then temporarily set `font: FontFamily::System` (don't commit), rebuild, and screenshot. The
    crops must differ (the Space Grotesk titles are visibly different).
  - **Missing-file check:** delete `assets/fonts/SpaceGrotesk-SemiBold.ttf` temporarily, then run
    `mobiler build android`. Expect a warning naming the file, a successful build, and
    `res/font/mobiler_display_600.ttf` removed. Restore the file and re-sync afterwards.
  - Coffee Android unchanged vs `main` (only the status-bar patch).
  - Kill the emulator.
- [ ] **Step 4: Commit:** `feat(barbershop): Space Grotesk + Roboto via mobiler.toml [fonts] (FontFamily::Custom)`

---

### Task 7: Docs, review, PR

- [ ] `mobiler/README.md`: a "Custom fonts — `mobiler.toml` `[fonts]`" section with the toml, what
  the sync writes, and the fallbacks. Also add `mobiler fonts sync` to the command list, if there is
  one.
- [ ] NOTES.md section "Custom fonts (2026-09-28)". The template port list: barbershop
  `Type.kt`/`Theme.kt`/`Render.swift`, plus the `project.yml` anchor stays as it is.
- [ ] Fresh whole-branch review, then a fix pass.
- [ ] ship-pr (25 checks, including every demo's iOS lane), squash-merge, no publish. Update memory
  and `start.md`.
