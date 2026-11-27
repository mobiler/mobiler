# `[icon]` and `[assets]` Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** An app declares its launcher icon (`[icon]`) and a folder of bundled files (`[assets]`) in
`mobiler.toml`; the CLI makes every shell match, and `asset:<path>` renders offline in `image`, `avatar` and
the video `poster` on Android, iOS and web.

**Architecture:** Two new CLI sync modules, `mobiler/src/assets.rs` and `mobiler/src/icon.rs`, built on the
`[splash]` pattern (`mobiler/src/splash.rs`, ADR-0048) and run from the same hooks. Assets are never copied
into committed shell files: marker blocks point Gradle, XcodeGen and trunk at the app's folder. Each shell
resolves `asset:` with one small validator. `mobiler-web` ships the web resolver first (libs PR), then the
CLI PR ports barbershop's native changes into the templates.

**Tech Stack:** Rust (CLI, `image` 0.25, `mobiler-web` on Leptos/trunk), Kotlin/Compose + Gradle KTS (Coil 3),
Swift/SwiftUI + XcodeGen.

**Spec:** `docs/superpowers/specs/2026-11-27-icon-and-assets-design.md` (approved 2026-11-27). Read it first.

## Global Constraints

- Release order: Part A (libs PR, `mobiler-web` 0.50.0 → **0.51.0**, `mobiler-ui`/`mobiler-core` unchanged)
  is merged and published before Part B (CLI PR, 0.74.0 → **0.75.0**) starts (ADR-0009). Every publish and
  `v*` tag needs the maintainer's explicit go; auto mode blocks merge/publish, so hand the user `!` commands.
- No ABI change: `mobiler-ui` and `mobiler-core` are not touched.
- Native shell changes land in `demos/barbershop` first and are ported into `mobiler/templates/` from its
  diff (ADR-0015).
- `asset:` rule, identical in all four resolvers: prefix `asset:`; the rest is non-empty, has no leading
  `/`, no `\`, and no segment that is empty, `.` or `..`. A source failing the rule renders as a failed
  load. Inside every shell the files live under `mobiler-assets/` (iOS: under the folder named by Info.plist
  `MobilerAssetsFolder`).
- `[assets] dir` default `"assets"`; must be relative, inside the app root, no `..`, no symlinked
  component (use `crate::fsguard`).
- `[icon]`: `foreground` PNG square, 1024..=16384 px; `background` `#RRGGBB`; optional `monochrome` (same
  rules as foreground); `padding` integer 0..=40, default **18** = % of the visible icon's side left clear
  on each side. Any validation failure writes nothing.
- Sync header text: `generated from mobiler.toml [icon]; edit mobiler.toml, not this file`. Ledger:
  `.mobiler/icon.json` `{"active": bool, "files": {"<rel path>": "<fnv1a64 hex>"}}`.
- Never commit `docs/requests/`. Commit messages via `-F` or quoted heredoc (backticks).
- Builds: per-demo `CARGO_TARGET_DIR` under `/media/zmilan/data2` (shared target-dir links the wrong
  `shared` crate). Subagent briefs must forbid `pgrep -f` / file-marker polling loops.
- Every README, site page (`git grep -n 'splash\|fonts' -- site/src/content/docs`) and the Saldo tutorial
  that describes icons or images moves in the same PR.

## Review Focus

1. **File names with a space or non-ASCII letter** (`asset:guides/step 1.png`, `asset:čaj.png`): load on all
   three shells. Web percent-encodes each segment; Android/iOS pass the path through unchanged. Tests in
   Tasks 2, 3.
2. **A file deleted from `assets/`** disappears from the next build (Gradle `Sync` prunes, trunk copies fresh,
   XcodeGen folder ref re-copies) and its `asset:` shows the placeholder. Checked in Task 12.
3. **Palette, grayscale and 16-bit PNG icon inputs** are accepted and converted (decode to RGBA8). Test in
   Task 7.
4. **An existing app with a hand-edited `ic_launcher.xml` (Saldo before reset)**: Android is skipped with a
   warning naming the file; iOS and web are still written. Test in Task 8.
5. **`mobiler upgrade` on an app with both sections** reports nothing pending and writes nothing under
   `<dir>`. Test in Task 8 (icon seeds) and Task 6 (dir untouched); end-to-end in Task 10.

---

## Part A: libs PR (branch `feat/icon-and-assets`, already holds the spec)

### Task 1: Trunk feasibility gate

**Files:** none committed (scratch app under the scratchpad).

- [ ] **Step 1:** Copy `demos/barbershop/web` to a scratch dir next to a sibling `assets/` holding `a.png` and
  `sub/b c.png`; add `<link data-trunk rel="copy-dir" href="../assets" data-target-path="mobiler-assets"/>`
  to its `index.html`; run `trunk build` (per-demo `CARGO_TARGET_DIR`).
- [ ] **Step 2:** Expected: `dist/mobiler-assets/a.png` and `dist/mobiler-assets/sub/b c.png` exist. If trunk
  rejects a path outside the page's folder or ignores `data-target-path`, **stop and ask the maintainer**
  (the spec's approach A depends on it). Also run XcodeGen's docs check now: can a `type: folder` source be
  given a fixed bundle name? Record the answer; if yes, Task 5 drops the `assets-info` block.

### Task 2: `mobiler-web` resolves `asset:`

**Files:**
- Modify: `mobiler-web/src/lib.rs` (image ~2241, avatar ~2264/2276, video poster ~2361)
- Modify: `mobiler-web/Cargo.toml` (version 0.51.0), `mobiler-web/README.md`, `mobiler-web/CHANGELOG.md` if present

**Interfaces:**
- Produces: `pub(crate) fn image_src(source: &str) -> String` — non-`asset:` strings returned unchanged; a
  valid `asset:p` → `"mobiler-assets/" + p` with each segment percent-encoded (everything outside
  RFC 3986 unreserved); an invalid `asset:` → `""` (an empty `src` fires `error`, so the avatar falls back
  to initials).

- [ ] **Step 1: Write failing tests** in a `#[cfg(test)] mod asset_tests`:
  `image_src("https://x/y.png") == "https://x/y.png"`; `image_src("file:///a.png")` unchanged;
  `image_src("asset:a.png") == "mobiler-assets/a.png"`; `image_src("asset:guides/step 1.png") ==
  "mobiler-assets/guides/step%201.png"`; `image_src("asset:čaj.png") == "mobiler-assets/%C4%8Daj.png"`;
  each of `"asset:"`, `"asset:/a.png"`, `"asset:../a.png"`, `"asset:a/../b.png"`, `"asset:a//b.png"`,
  `"asset:./a.png"`, `"asset:a\\b.png"` → `""`.
- [ ] **Step 2:** `cargo test -p mobiler-web asset_tests` (from `mobiler-web/`) → FAIL (not defined).
- [ ] **Step 3:** Implement `image_src`; route the image `src`, both avatar `<img>`s and `poster_attr` through it.
- [ ] **Step 4:** Tests pass; `cargo clippy` clean; the barbershop web build still compiles against the path dep.
- [ ] **Step 5:** README: an "Images bundled with the app (`asset:`)" paragraph. Commit.

### Task 3: Barbershop native shells + demo card

**Files:**
- Modify: `demos/barbershop/Android/app/src/main/java/dev/mobiler/barbershop/MainActivity.kt` (Image, Avatar,
  `VideoWidget` poster), `demos/barbershop/Android/app/build.gradle.kts` (filled `mobiler:assets` block)
- Modify: `demos/barbershop/iOS/Sources/Render.swift` (`.image`, avatar ~784, `attachPoster` ~2754),
  `demos/barbershop/iOS/project.yml` (filled `assets-sources` + `assets-info` blocks)
- Modify: `demos/barbershop/web/index.html` (filled `mobiler:assets` block)
- Create: `demos/barbershop/assets/demo/hello.png`, `demos/barbershop/assets/demo/photo 1.jpg` (small, < 50 KB each)
- Modify: `demos/barbershop/shared/src/…` (a "Bundled images" card: `image("asset:demo/hello.png", …)`,
  an avatar with `asset:demo/photo 1.jpg`, and `image("asset:demo/missing.png", …)` showing the placeholder)
- Modify: `demos/barbershop/mobiler.toml` (`[assets]` with no `dir`)

**Interfaces:**
- Produces (Kotlin, top-level in MainActivity.kt): `fun assetModel(source: String): String` —
  valid `asset:p` → `"file:///android_asset/mobiler-assets/$p"`; invalid → `""`; others unchanged. Used as
  Coil's `model` for Image and Avatar. Poster: when `poster` starts with `asset:` load with
  `context.assets.open("mobiler-assets/$p")` (java.net.URL can't open `android_asset`).
- Produces (Swift, file-private in Render.swift): `func assetURL(_ source: String) -> URL?` —
  `Bundle.main.resourceURL/<MobilerAssetsFolder>/p` for a valid `asset:p`, else nil. Image and avatar:
  a non-nil result goes through `FileImageView` (invalid → `FileImageView` on a nonexistent URL, so the
  same placeholder/initials). Poster: read the file URL instead of fetching.
- Block contents (exact; Task 6 generates the same text):
  - Gradle, after the `android { }` block, between `// mobiler:assets-begin` / `// mobiler:assets-end`:
    ```kotlin
    val mobilerAssets = tasks.register<Sync>("mobilerAssets") {
        from(rootProject.file("../assets"))
        into(layout.buildDirectory.dir("generated/mobiler-assets/mobiler-assets"))
    }
    android.sourceSets["main"].assets.srcDir(layout.buildDirectory.dir("generated/mobiler-assets"))
    tasks.named("preBuild") { dependsOn(mobilerAssets) }
    ```
  - `project.yml` target `sources`, between `# mobiler:assets-sources-begin/end`: `- path: ../assets`,
    `type: folder`, `buildPhase: resources`. `info.properties`, between `# mobiler:assets-info-begin/end`:
    `MobilerAssetsFolder: assets`.
  - `web/index.html`, before `</head>`, between `<!-- mobiler:assets-begin -->/<!-- mobiler:assets-end -->`:
    `<link data-trunk rel="copy-dir" href="../assets" data-target-path="mobiler-assets"/>`.

- [ ] **Step 1:** Add `assetModel` / `assetURL` with exactly Task 2's validator rule (Task 9 checks the
  source), and wire image, avatar and poster in each shell; add the blocks, assets and the demo card.
- [ ] **Step 2:** Build barbershop Android (`./gradlew assembleDebug`), iOS (`xcodegen` + `xcodebuild` sim),
  web (`trunk build`). All green.
- [ ] **Step 3:** Runtime: Android emulator + web headless: the card shows both images, the avatar, and a
  placeholder for `missing.png`; no crash. Screenshot both. iOS: simulator if available, else note for the
  maintainer's iPhone pass.
- [ ] **Step 4:** Commit.

### Task 4: Ship the libs PR

- [ ] **Step 1:** Fresh code + security review (Fable model) of the branch diff; fix; re-review fixes.
- [ ] **Step 2:** `ship-pr` skill (PR body lists READMEs checked). After merge, `release-libs` for
  `mobiler-web` 0.51.0 only, with the maintainer's go. Then `post-release` install check.

---

## Part B: CLI PR (new branch `feat/icon-and-assets-cli` from main after 0.51.0 is live)

### Task 5: Template ports

**Files:**
- Modify: `mobiler/templates/Android/app/src/main/java/__PACKAGE_PATH__/MainActivity.kt`,
  `mobiler/templates/iOS/Sources/Render.swift` — port Task 3's helpers and widget wiring from barbershop's diff.
- Modify: `mobiler/templates/Android/app/build.gradle.kts` — **empty** `// mobiler:assets-begin/end` pair
  after the `android { }` block.
- Modify: `mobiler/templates/iOS/project.yml` — empty `# mobiler:assets-sources-begin/end` as the last
  entry of the target's `sources`; empty `# mobiler:assets-info-begin/end` inside `info.properties`.

- [ ] **Step 1:** Port; `cargo test -p mobiler` green (template tests, upgrade tests).
- [ ] **Step 2:** Commit.

### Task 6: `assets.rs` sync

**Files:**
- Create: `mobiler/src/assets.rs`
- Modify: `mobiler/src/main.rs` (`mobiler assets sync` subcommand, like `SplashCmd::Sync`),
  `build.rs`, `dev.rs` (call `crate::assets::sync_for_build` after splash), `watch.rs` (watch `<dir>`),
  `doctor.rs` (`doctor_line`)

**Interfaces:**
- Consumes: `crate::fonts::replace_block` (insert-before-`</head>` behaviour for web),
  `splash`'s `block_change`/`apply` pattern for template files (copy the pattern; extract a shared helper
  only if it stays small).
- Produces: `pub struct AssetsSpec { pub dir: String }`; `pub fn validate(root: &Path, spec: &AssetsSpec)
  -> Result<PathBuf, String>`; `pub fn gradle_lines(dir: Option<&str>) -> Vec<String>`;
  `pub fn yml_source_lines(dir: Option<&str>) -> Vec<String>`; `pub fn yml_info_lines(dir: Option<&str>)
  -> Vec<String>`; `pub fn web_lines(dir: Option<&str>) -> Vec<String>`; `pub fn sync(root: &Path) ->
  anyhow::Result<SyncReport>`; `pub fn sync_for_build(root: &Path)`; `pub fn watch_paths(root: &Path) ->
  (Vec<PathBuf>, Vec<PathBuf>)`; `pub fn doctor_line(root: &Path) -> String`. `None` = section absent →
  empty block. `dir` in the generated text is the configured value (`../<dir>`; `MobilerAssetsFolder` =
  its last segment).

- [ ] **Step 1: Failing tests** (`#[cfg(test)]` in `assets.rs`, throwaway app trees like `splash.rs`'s `app()`):
  - `generated_blocks_match_barbershop` — the four generators with `Some("assets")` equal the text in
    Task 3's block contents.
  - `fills_template_blocks_and_inserts_web_block` — after sync: gradle and project.yml blocks filled, web
    block inserted before `</head>`.
  - `missing_markers_warn_and_never_edit` — a `build.gradle.kts` without markers is byte-identical after
    sync and the report has a warning containing `mobiler upgrade --apply`.
  - `removing_section_empties_blocks_and_keeps_dir` — `assets/x.png` still exists, blocks empty.
  - `rejects_dir_outside_root` — `dir = "../x"`, `dir = "/abs"`, and a symlinked `dir` → error, nothing written.
  - `missing_dir_warns_but_writes_blocks`.
  - `second_sync_is_a_no_op` (mtimes unchanged).
  - `upgrade_never_touches_dir` — run the upgrade planner on a scaffolded app with `[assets]`; no planned
    write under `assets/`.
- [ ] **Step 2:** Run → FAIL. **Step 3:** implement. **Step 4:** `cargo test -p mobiler assets` → PASS.
- [ ] **Step 5:** Commit.

### Task 7: `icon.rs` — validation and pure generators

**Files:** Create `mobiler/src/icon.rs`; modify `mobiler/Cargo.toml` (`image` features `["png", "webp"]`; if
0.25's `webp` feature can't encode lossless, stop and pick the smallest pure-Rust encoder, noting it in the
ADR).

**Interfaces:**
- Produces: `pub struct IconSpec { foreground: String, background: String, monochrome: Option<String>,
  padding: Option<u32> }` (serde, in `[icon]`); `pub struct Icon { fg: RgbaImage, mono: Option<RgbaImage>,
  bg: [u8; 3], padding: u32 }`; `pub fn validate(root: &Path, spec: &IconSpec) -> Result<Icon, String>`;
  `pub fn layer(icon: &Icon, img: &RgbaImage, px: u32) -> RgbaImage` (Android 108 dp layer at `px`,
  transparent; content box = `px * 72/108 * (1 - 2*padding/100)` centred); `pub fn composite(icon: &Icon,
  px: u32, round: bool) -> RgbaImage` (background-filled square, content box = `px * (1 - 2*padding/100)`
  centred; `round` masks to a circle with anti-aliased edge); `pub fn ios_png(icon: &Icon) -> Vec<u8>`
  (1024 composite encoded **RGB, no alpha channel**); `pub fn android_adaptive_xml(has_mono: bool) ->
  String`; `pub fn colors_xml(bg: [u8; 3]) -> String`; `pub fn web_lines(on: bool) -> Vec<String>`;
  `fn fnv1a64_hex(bytes: &[u8]) -> String`.
- Sizes: layers `mdpi 108, hdpi 162, xhdpi 216, xxhdpi 324, xxxhdpi 432`; legacy `48, 72, 96, 144, 192`;
  web `32`, `180`. Resize Lanczos3.
- Adaptive XML: header comment, `<background android:drawable="@color/mobiler_icon_background"/>`,
  `<foreground android:drawable="@mipmap/mobiler_icon_foreground"/>`, `<monochrome
  android:drawable="@mipmap/mobiler_icon_monochrome"/>` when set else `@mipmap/mobiler_icon_foreground`.
  Same content for `ic_launcher.xml` and `ic_launcher_round.xml`.

- [ ] **Step 1: Failing tests:** `validate_rejects` (non-PNG, 1000×1000, 1024×1000, 20000 px header,
  `#12345`, `red`, padding 41, missing file — each `Err` with a reason naming the field);
  `validate_accepts_palette_gray_and_16bit` (Review Focus 3); `layer_content_box_at_default_padding`
  (432 px layer from a fully opaque 1024 foreground: opaque bbox is 184×184 centred ±1 px);
  `composite_matches_padding` (1024, padding 18 → opaque-foreground bbox 655×655 ±1 centred);
  `round_corners_are_transparent`; `ios_png_has_no_alpha` (decode → `ColorType::Rgb8`);
  `adaptive_xml_falls_back_to_foreground_for_monochrome`; `fnv_is_stable` (`fnv1a64_hex(b"") ==
  "cbf29ce484222325"`).
- [ ] **Step 2–4:** fail → implement → `cargo test -p mobiler icon` pass. **Step 5:** commit.

### Task 8: `icon.rs` — sync, ownership, undo; seeds; hooks

**Files:** Modify `mobiler/src/icon.rs`, `mobiler/src/upgrade.rs` (`SEED_PATHS` += `…/mipmap-anydpi/ic_launcher.xml`,
`…/mipmap-anydpi/ic_launcher_round.xml`), `main.rs` (`mobiler icon sync`), `build.rs`, `dev.rs`,
`watch.rs` (watch `foreground`/`monochrome` files), `doctor.rs`.

**Interfaces:**
- Produces: `pub fn sync(root: &Path) -> anyhow::Result<SyncReport>`, `pub fn sync_for_build(root: &Path)`,
  `pub fn watch_paths`, `pub fn doctor_line` (same shapes as splash).
- Files per platform (one ownership unit each; a single foreign file skips that platform with a warning):
  - Android: `mipmap-anydpi/ic_launcher{,_round}.xml`, `values/mobiler_icon.xml`,
    `mipmap-*/mobiler_icon_foreground.png`, `mipmap-*/mobiler_icon_monochrome.png` (when set),
    `mipmap-*/ic_launcher{,_round}.webp` (lossless WebP, same names as the template's).
  - iOS: `Assets.xcassets/AppIcon.appiconset/icon-1024.png`.
  - Web (only if `web/index.html` exists): `web/icon/mobiler-icon-32.png`, `web/icon/mobiler-icon-180.png`,
    block `<!-- mobiler:icon-begin/end -->` before `</head>`: `<link data-trunk rel="copy-dir"
    href="icon"/>`, `<link rel="icon" type="image/png" sizes="32x32" href="icon/mobiler-icon-32.png"/>`,
    `<link rel="apple-touch-icon" href="icon/mobiler-icon-180.png"/>`.
- Ownership: a file is writable when missing, equal to the template's stock (text CRLF-tolerant), carrying
  the header (XML), or whose `fnv1a64_hex` equals the ledger's entry. Sync-only files (`mobiler_icon_*`,
  `values/mobiler_icon.xml`, `web/icon/mobiler-icon-*`) are created with `[icon]`, removed without.
- Undo (section removed, ledger active): each file still matching the ledger → template stock (from
  `crate::upgrade::TEMPLATES`); sync-only files removed; web block emptied; `web/icon/` removed only if
  empty; ledger kept with `active: false`.
- Order: layers and PNGs → XML/plist references → ledger last; on undo, references first.

- [ ] **Step 1: Failing tests:** `writes_every_platform`; `second_sync_is_a_no_op`;
  `hand_edited_launcher_xml_skips_android_only` (Review Focus 4: warning names the file; iOS/web written);
  `broken_foreground_writes_nothing` (tree byte-identical); `removing_icon_restores_stock`;
  `removed_and_added_back_still_owns_its_files`; `no_png_beside_webp` (after sync, no resource name has
  both `.png` and `.webp` in any `mipmap-*`); `foreign_file_in_web_icon_survives`;
  `icon_source_inside_assets_dir_warns` (still syncs);
  `upgrade_reports_nothing_for_icon_files` (scaffold → `[icon]` sync → upgrade plan has no pending/conflict
  for icon paths; Review Focus 5); `no_section_touches_nothing`.
- [ ] **Step 2–4:** fail → implement → pass; whole `cargo test --workspace` green. **Step 5:** commit.

### Task 9: ADR-0070, ADR-0071, conformance with mutation proofs

**Files:** Create `docs/adr/ADR-0070-assets-sync.md`, `docs/adr/ADR-0071-icon-sync.md` (ADR-0069, the `asset:` scheme, landed in the libs PR; extend its conformance test to the template copies); modify
`docs/adr/index.md`, `xtask/tests/adr_conformance.rs`.

- [ ] **Step 1:** Write both ADRs in the README format (provenance `[recorded: the spec]`), Conformance
  naming the tests below plus the Task 6/8 unit tests that carry the invariants.
- [ ] **Step 2:** Conformance tests: `adr_0071_icon_seeds_and_marker` (both `ic_launcher*.xml` in
  `SEED_PATHS`; template `ic_launcher.xml` has a `<monochrome>`); `adr_0070_asset_markers` (resolvers: extend `adr_0069_asset_resolvers_follow_one_rule` to the template files)
  (gradle has one `mobiler:assets` pair after `android {`; project.yml has the `assets-sources` pair inside
  the target `sources` and the `assets-info` pair inside `info.properties`).
- [ ] **Step 3:** `cargo test -p xtask` green, including `adr_docs.rs`.
- [ ] **Step 4: Mutation proof** for each test: drop a marker, drop a seed path, remove the `..` check from
  one resolver, make ownership ignore the ledger, skip undo. Each must fail; record them in each ADR's §4
  `Mutation proof`; revert to green. Commit.

### Task 10: Demos and Saldo

**Files:** `demos/{saldo,coffee,todo,fullstack-todo mobile}` via `mobiler upgrade --apply` (ADR-0063);
`demos/saldo/brand/icon-foreground.png` (redraw: white DejaVu Sans Bold "S" on transparent 1024 px, glyph
height 748 px centred — matches today's `icon-1024.png` within 2 px at padding 18); `demos/saldo/mobiler.toml`
`[icon] foreground = "brand/icon-foreground.png"`, `background = "#0e9f8e"`.

- [ ] **Step 1:** Upgrade every CLI-managed demo; the ADR-0063 conformance test passes.
- [ ] **Step 2:** Saldo reset: restore `mipmap-anydpi/ic_launcher*.xml` and `drawable/ic_launcher_background.xml`
  to the template's, delete `drawable-xxxhdpi/saldo_launcher_foreground.png`, add `[icon]`, run
  `mobiler icon sync`. Then a plain `mobiler upgrade` reports nothing pending.
- [ ] **Step 3:** Compare Saldo's new `icon-1024.png` with the old one (bbox within 2 px, colour equal).
  Commit.

### Task 11: Docs

- [ ] **Step 1:** `cargo run -p xtask -- gen-readme`; hand-written sections in root `README.md`,
  `mobiler/README.md` (`[icon]`, `[assets]`, `asset:`, `.mobiler/icon.json` must be committed,
  keep icon sources outside `dir`, store size note, Android 12 splash shows the icon), site pages
  (`git grep -n 'splash\|\[fonts\]\|image(' -- site/src/content/docs`), Saldo tutorial ch.10 §4 (docs/ and
  site copy) shrinks to the `[icon]` config. Bump CLI to 0.75.0. Commit.

### Task 12: Verify and ship

- [ ] **Step 1:** Android emulator API 26 and 36 (`mobiler_verify_p7` on data2): Saldo's launcher icon
  (adaptive; themed on 36 with themed icons on); barbershop's asset card; delete `assets/demo/hello.png`,
  rebuild → placeholder (Review Focus 2). Screenshots.
- [ ] **Step 2:** iOS simulator build of Saldo + barbershop; maintainer's iPhone pass for the home-screen
  icon and `asset:` images after TestFlight.
- [ ] **Step 3:** Web: trunk build barbershop, headless check that `mobiler-assets/demo/hello.png` and the
  favicon load.
- [ ] **Step 4:** Fresh code + security review (Fable); fix; re-review. `pstree` check for leftovers.
- [ ] **Step 5:** `ship-pr`, then `release-cli` 0.75.0 with the maintainer's go, then `post-release`.
  Delete `docs/requests/2026-11-27-app-icon.md` and `2026-11-26-bundled-media.md` once released.
