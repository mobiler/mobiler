# ADR-0070: `mobiler.toml` `[assets]` ships one app folder inside every shell by pointing each build at it — a Gradle `Sync` into `build/`, an XcodeGen folder reference plus `MobilerAssetsFolder`, a trunk `copy-dir` — filled into marker blocks the CLI never inserts in template files; nothing is copied into the shells' trees, and `dir` may not start with a top-level path the CLI writes into (a template top-level entry, `web`, `.mobiler`, `target`, `mobiler.toml`), so no sync and no `upgrade` writes under it

Status:        Accepted
Date decided:  2026-11-27
Deciding PRs:  the CLI PR of the `[icon]` / `[assets]` release (CLI 0.75.0)
Supersedes:    none
Code anchor:   mobiler/src/assets.rs (clean_dir, validate, yaml_string, the generators, sync, fill, source_overlap_warnings, symlink_warnings, watch_paths, doctor_line), mobiler/src/build.rs + dev.rs + watch.rs + doctor.rs (the hooks), mobiler/templates/Android/app/build.gradle.kts and mobiler/templates/iOS/project.yml (the marker blocks), the template's MainActivity.kt and Render.swift (the ADR-0069 resolvers)
Conformance:   xtask/tests/adr_conformance.rs::adr_0070_asset_markers, xtask/tests/adr_conformance.rs::adr_0069_asset_resolvers_follow_one_rule, mobiler/src/assets.rs::generated_blocks_match_barbershop, mobiler/src/assets.rs::fills_template_blocks_and_inserts_web_block, mobiler/src/assets.rs::missing_markers_warn_and_never_edit, mobiler/src/assets.rs::missing_dir_fills_only_the_gradle_block, mobiler/src/assets.rs::removing_section_empties_blocks_and_keeps_dir, mobiler/src/assets.rs::rejects_dir_outside_root, mobiler/src/assets.rs::folder_names_yaml_would_retype_are_quoted, mobiler/src/assets.rs::symlink_inside_dir_warns, mobiler/src/assets.rs::fonts_or_splash_source_under_dir_warns, mobiler/src/assets.rs::icon_source_inside_assets_dir_warns, mobiler/src/assets.rs::upgrade_never_touches_dir

## 1. Context (The Problem)

ADR-0069 fixed what `asset:<path>` means on every shell: a file under `mobiler-assets/` (Android, web) or under the
bundle folder named by Info.plist's `MobilerAssetsFolder` (iOS). It left open how an app's files get there.

An app's images live in one folder of the app. Each platform bundles files its own way: Android's `assets/` source
set, an Xcode resource phase driven by XcodeGen's `project.yml`, trunk's asset pipeline for the web page. The app
builder should not have to learn those three, nor commit each image once per shell. And the CLI's usual tools
constrain the answer:
- `Android/app/build.gradle.kts` and `iOS/project.yml` are template files `mobiler upgrade` merges (ADR-0043,
  ADR-0046); the sync may only touch them inside markers it owns, as `[splash]` does (ADR-0048).
- `web/index.html` isn't scaffolded, and `upgrade` never touches it; `[fonts]` (ADR-0022) and `[splash]` insert
  their blocks there before `</head>`.
- The folder name reaches Kotlin source, YAML and HTML unescaped by the build tools that read them.

## 2. Hypothesis

If `[assets] dir` (default `"assets"`) names one folder inside the app, and the CLI writes into each shell only a
pointer to it, then:
- an app ships offline images by dropping files into one folder, with no sync step on an edit (the next build
  picks them up);
- nothing is committed twice, and nothing stale has to be pruned;
- no file `mobiler upgrade` merges gains content outside its markers.

The rules:
- **The pointers.**
  - **Android:** the `mobiler:assets` block in `app/build.gradle.kts` (after the `android {}` block, so
    `android.sourceSets` exists) registers a Gradle `Sync` task copying `../<dir>` into
    `build/generated/mobiler-assets/mobiler-assets/`, adds `build/generated/mobiler-assets` as a `main` assets
    source directory, and makes `preBuild` depend on the task. `Sync` prunes deleted files; the copy is build
    output, ignored by the template's `.gitignore` (`/Android/**/build/`).
  - **iOS:** two pairs in `project.yml`: `mobiler:assets-sources` inside the target's `sources`
    (`- path: ../<dir>`, `type: folder`, `buildPhase: resources`) and `mobiler:assets-info` inside
    `info.properties` (`MobilerAssetsFolder: <dir's last segment>`), because a folder reference keeps its
    directory's name in the bundle.
  - **Web:** the `mobiler:assets` block in `web/index.html`:
    `<link data-trunk rel="copy-dir" href="../<dir>" data-target-path="mobiler-assets"/>`.
- **`dir`** is checked before anything is written; an invalid one is a warning, `failed`, and no write. It must:
  - be non-empty and relative (no leading `/`), a trailing `/` being dropped;
  - have no empty, `.` or `..` segment, and resolve to normal path components inside the app root;
  - have no hidden segment (one starting with `.`): a hidden folder is never runtime assets, and `dir = ".git"`
    would bundle the repository into the app;
  - not start with a path the CLI writes: a top-level entry of the template (`Android`, `iOS`, `shared`,
    `Cargo.toml`, `rust-toolchain.toml`, `.gitignore`, read from the embedded template, `.tmpl` dropped), `web`,
    `.mobiler`, `target` or `mobiler.toml`, compared ignoring ASCII case (a case-insensitive file system makes
    `android` the same folder). So `web`, `web/img` or `Android/x` is refused, and `webby` or `my/web` is not;
  - have no symlinked component on the way, the folder itself included (`fsguard`, as every CLI write);
  - be a folder if it exists. A missing folder is only a warning (it says web and iOS include the folder once
    it exists, after a build or `mobiler assets sync`), and only the Gradle block is filled: trunk's `copy-dir`
    fails the web build on a missing folder (`error taking canonical path of directory`, trunk 0.21.14), and an
    Xcode folder reference to a missing path either fails XcodeGen or, with XcodeGen's `optional: true` (which
    only disables its missing-path check), leaves a reference Xcode can't copy. So both `project.yml` blocks and
    the web block stay empty (a filled one is emptied) until the folder exists; Gradle's `Sync` copies nothing
    from a missing `from`. iOS then has no `MobilerAssetsFolder`, so every `asset:` shows the failed-load
    placeholder, as for a missing file;
  - use, in each segment, only characters from an **allow-list**: Unicode letters and digits (Rust's
    `char::is_alphanumeric`), space (not first or last in a segment), `-`, `_`, `.` and `+`. The value lands in a
    Kotlin string, a YAML scalar and an HTML attribute, each unescaped by its reader; an allow-list keeps `"`,
    `\`, `$`, `<`, `&`, `#`, `:` and the rest out of all three at once, where escaping would need three escapers,
    each a place to get it wrong.
- **YAML quoting.** `MobilerAssetsFolder`'s value is written plain only when it is an identifier
  (`[A-Za-z_][A-Za-z0-9_-]*`) that isn't a YAML keyword (`true`, `false`, `yes`, `no`, `on`, `off`, `y`, `n`,
  `null`, `~`, any case); otherwise it is double-quoted, so no YAML version reads `2024`, `0x1F`, `1e5`, `.5`,
  `yes` or `null` as anything but a string. The allow-list already keeps `"` and `\` out of the quoted form.
- **Template files are filled, never inserted into.** `build.gradle.kts` and `project.yml` get only their marker
  blocks filled. A file without a pair gets a warning naming `mobiler upgrade --apply` and is not edited; a file
  with one of its pairs gets that one filled. With no section, a missing pair says nothing.
- **`web/index.html`** follows the `[fonts]`/`[splash]` precedent: the block is inserted before `</head>` when
  missing and replaced afterwards. Without the section and without a block, the page is left alone.
- **Removing `[assets]`** empties every block and never touches the folder. The `[assets]` sync never writes,
  moves or deletes anything under `dir`. Nothing else the CLI writes lies there either: `dir` can't start with a
  CLI-written top-level path (above), so `new`, `upgrade` and the other syncs can't reach it (each writes only under
  those paths, plus single files at the app root — `new`'s `CLAUDE.md` and `upgrade`'s `<file>.mobiler-new` review
  copies — never inside another folder), and the template has no files under `assets/`, the default `dir`.
- **Warnings that don't stop the sync:**
  - a `[fonts]`, `[splash]` or `[icon]` source file under `dir` (compared by path segment), since it would also
    ship inside the app;
  - each symlink inside `dir` (file or folder; a linked folder isn't walked into), since Gradle's `Sync` and
    trunk's `copy-dir` follow it and ship its target. `mobiler doctor` repeats them.
- **Hooks:** the sync runs at the start of `mobiler build`, `dev` and `watch`, has its own `mobiler assets sync`,
  and has a `doctor` line (the folder and its file count). `watch` watches `dir`, so a changed file rebuilds; an
  invalid `dir` is never watched.
- **Write order:** every target is checked for a symlink on the way before the first write; a file whose text
  would not change is not rewritten (a second sync is a no-op); line endings are kept.

### 2.1. Refutation Conditions

- **Condition 1 — the template ships the markers, empty, where each tool reads them.** One pair in
  `build.gradle.kts` after `android {}`; `assets-sources` inside the target's `sources`; `assets-info` at
  `info.properties`' key depth; the same markers `assets.rs` fills; the Gradle copy in an ignored build folder.
  - **Validation Metric:** `adr_0070_asset_markers`.
- **Condition 2 — every template shell resolves `asset:` by ADR-0069's rule.** The template's `MainActivity.kt`
  and `Render.swift` carry the same checks as barbershop's.
  - **Validation Metric:** `adr_0069_asset_resolvers_follow_one_rule` (now over barbershop and the template).
- **Condition 3 — the generated blocks are the ones proven to build.** The generators emit exactly barbershop's
  hand-filled blocks, which build on every shell in CI.
  - **Validation Metric:** `generated_blocks_match_barbershop`.
- **Condition 4 — template files are filled, never inserted into; the web block is inserted.**
  - **Validation Metric:** `fills_template_blocks_and_inserts_web_block`, `missing_markers_warn_and_never_edit`,
    `missing_dir_fills_only_the_gradle_block` (a missing folder gets only the Gradle block, and the web and iOS
    blocks once it exists).
- **Condition 5 — removing the section empties the blocks and keeps the folder.**
  - **Validation Metric:** `removing_section_empties_blocks_and_keeps_dir`.
- **Condition 6 — nothing the CLI writes lies under `dir`.** The real guards are structural: `validate` refuses a
  `dir` starting with a CLI-written top-level path, and the template has no `assets/` folder.
  `upgrade_never_touches_dir` adds a behavioural check on one app (default `dir`, files under it named like
  template paths): an upgrade run, review or `--apply`, plans nothing under `dir`, leaves it byte for byte and
  mtime for mtime, and treats the filled marker blocks as no pending change. It does **not** guard against a
  future template file under `dir`, or a `dir` placed over a CLI path: in that test the scaffold would write the
  file too, and a three-way merge leaves the app's copy alone while the template is unchanged.
  - **Validation Metric:** `rejects_dir_outside_root` (the CLI-path cases), `adr_0070_asset_markers` (no
    `mobiler/templates/assets`), `upgrade_never_touches_dir`.
- **Condition 7 — no `dir` outside the app, through a symlink, hidden, or with a character outside the allow-list.**
  - **Validation Metric:** `rejects_dir_outside_root`, `folder_names_yaml_would_retype_are_quoted`.
- **Condition 8 — the warnings.**
  - **Validation Metric:** `symlink_inside_dir_warns`, `fonts_or_splash_source_under_dir_warns`,
    `icon_source_inside_assets_dir_warns`.
- **Condition 9 (runtime):** an `asset:` image, avatar and poster load on an Android emulator, the iOS device and
  a trunk build of the web page; a missing one shows the failed-load placeholder. Checked in the release's device
  pass.

## 3. Considered Options & Rationale for Refutation

- **Option A — copy the folder into each shell, as `[fonts]` does** `[recorded: docs/superpowers/specs/2026-11-27-icon-and-assets-design.md §3]`
  Rejected: "every image committed four times, plus pruning and ownership logic for stale copies. Fonts needed
  copies for fixed names and registration; images don't."
- **Option B — point each build at the app's folder** `[recorded: docs/superpowers/specs/2026-11-27-icon-and-assets-design.md §3, §4; maintainer's pick 2026-11-27]`
  Chosen: "One marker block per shell; no committed copies; edits are picked up by the next build with no sync
  step."
- **Option C — escape `dir` for each target language instead of an allow-list** `[reconstructed]`
  Rejected: three escapers (Kotlin string with `$` templates, YAML, HTML attribute) for a folder name; a gap in any
  one is an injection into a build file. Folder names outside the allow-list are rare and easy to rename.
- **Option D — a symlink in `dir` pointing at shared images** `[recorded: docs/superpowers/specs/2026-11-27-icon-and-assets-design.md §9, "every fallback (a symlink, a copy) changes approach A"]`
  Rejected as a mechanism; a symlink the app puts inside `dir` is allowed but warned about, because every build
  tool follows it and ships its target.
- **Option E — insert the markers into old apps' template files** `[recorded: ADR-0048, "fills marker blocks in framework files without ever inserting them"]`
  Rejected: a template file changes only through `mobiler upgrade`, which shows the change; the sync warns
  instead.

## 4. Decision & Rationale for Corroboration

Option B with the rules in §2. The default `dir` is `assets`, but an app whose `assets/` already holds font and
splash sources should name a subfolder (barbershop uses `assets/bundled`), which is why a source under `dir` is
warned about.

ADR-0069's conformance test now also reads the template's `MainActivity.kt` and `Render.swift`, as ADR-0069's
Follow-up asked; ADR-0069 itself is unchanged.

**Mutation proof** (2026-11-27; each mutation reverted after it was seen to fail):
- Deleting the template `build.gradle.kts`'s `// mobiler:assets-begin` failed `adr_0070_asset_markers`
  ("app/build.gradle.kts must have exactly one `// mobiler:assets-begin`").
- Moving the Gradle pair above `android {` failed it ("the mobiler:assets markers must sit after the `android {}`
  block").
- Moving the `assets-sources` pair from `sources` into `dependencies` failed it ("project.yml target sources must
  have exactly one `# mobiler:assets-sources-begin`").
- Moving the `assets-info` pair under `UILaunchScreen` failed it ("assets-info markers must sit at the properties'
  key depth").
- Making `gradle_lines` copy into `src/main/assets/mobiler-assets` failed it ("the Gradle copy must land in
  build/generated") and `generated_blocks_match_barbershop`.
- Deleting `|| it == ".."` from the template Kotlin `assetPath` failed `adr_0069_asset_resolvers_follow_one_rule`
  ("…/__PACKAGE_PATH__/MainActivity.kt assetPath lost `it == ".."`"); deleting `|| $0 == ".."` from the template
  Swift `assetURL` failed it too ("mobiler/templates/iOS/Sources/Render.swift assetURL lost …").
- In `assets.rs`:
  - never warning about a missing pair in `fill` failed `missing_markers_warn_and_never_edit`;
  - not inserting the web block when the page has none failed `fills_template_blocks_and_inserts_web_block` (and
    `missing_dir_fills_only_the_gradle_block`, `removing_section_empties_blocks_and_keeps_dir`);
  - filling the web and iOS blocks whether or not the folder exists (the first version) failed
    `missing_dir_fills_only_the_gradle_block` (the warning didn't name web and iOS; then "both iOS blocks stay
    empty");
  - returning early when the section is gone (blocks kept) failed `removing_section_empties_blocks_and_keeps_dir`
    ("the gradle block is empty again");
  - dropping the `fsguard` check from `validate` failed `rejects_dir_outside_root` (`"linked" accepted`);
  - dropping the character allow-list failed it (`"a\\b" accepted`);
  - dropping the hidden-segment check from `clean_dir` failed it (`".git" accepted`);
  - dropping only `clean_dir`'s `..` check did **not** fail it: `validate`'s normal-components check still refused
    `../x` and `a/../b` (two guards, by design); dropping both failed it (`"../x" accepted`);
  - never quoting the YAML value failed `folder_names_yaml_would_retype_are_quoted`;
  - not walking the folder in `symlink_warnings` failed `symlink_inside_dir_warns` ("one per link, a linked folder
    isn't followed: []");
  - never warning about a source under `dir` failed `fonts_or_splash_source_under_dir_warns` and
    `icon_source_inside_assets_dir_warns`.
- Making `upgrade` (not the scaffold) target `rust-toolchain.toml` at `assets/rust-toolchain.toml`, a template
  path under the default folder, failed `upgrade_never_touches_dir` ("apply=false: ["assets/rust-toolchain.toml",
  …]"). Adding a template file under `assets/` alone doesn't fail it: the scaffold writes it, and a three-way
  merge leaves an app's edit of an unchanged template file alone. That case is what the structural guards cover:
  - adding `mobiler/templates/assets/x.txt` failed `adr_0070_asset_markers` ("the template has files under
    assets/, the default [assets] dir");
  - dropping the CLI-path check from `validate` failed `rejects_dir_outside_root` (`"web" accepted`).
- Moving the Gradle pair inside the `dependencies {}` block failed `adr_0070_asset_markers` ("the mobiler:assets
  markers must sit at the top level of build.gradle.kts").
- Reverting restored green.

## 5. Consequences (Positive and Negative Predictions)

- **Positive:** offline images on every shell from one folder; an edit needs only a rebuild.
- **Positive:** no image is committed more than once; nothing goes stale in a shell's tree.
- **Positive:** `upgrade` merges the filled blocks like any marker block and never touches the folder.
- **Negative:** an app made before CLI 0.75 has no markers; its Android and iOS shells bundle nothing until
  `mobiler upgrade --apply` adds them (the sync says so on every run). The web block works at once.
- **Negative:** Android shows an empty box for a missing or misspelled asset, as for any failed image (ADR-0069).
- **Negative:** every file in `dir` ships in every build on every platform and grows the download; nothing is
  resized, compressed or filtered (an iOS folder reference can't exclude files).
- **Negative:** a font, splash or icon source left under `dir` ships twice; the sync warns but doesn't filter.
- **Negative:** a symlink inside `dir` ships its target; the sync warns but doesn't stop.
- **Negative:** a folder name outside the allow-list (an apostrophe, a `#`, …) is refused and must be renamed.
- **Negative:** on iOS the bundle folder is the last segment of `dir`, so two apps' folders aren't namespaced like
  Android's and the web's `mobiler-assets/`; `MobilerAssetsFolder` carries the name (ADR-0069).
- **Negative:** the Android `Sync` runs before every build; a large folder costs copy time on each one.
