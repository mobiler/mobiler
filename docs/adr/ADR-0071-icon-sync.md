# ADR-0071: `mobiler.toml` `[icon]` (one square foreground PNG and a background colour) is synced into the Android, iOS and web launcher icons by the CLI, which writes a file only while it is missing, stock, carries its header or matches its committed ledger, skips a whole platform when one file there is the app's, and restores stock only to the files it still owns when the section goes

Status:        Accepted
Date decided:  2026-11-27
Deciding PRs:  the CLI PR of the `[icon]` / `[assets]` release (CLI 0.75.0)
Supersedes:    none
Code anchor:   mobiler/src/icon.rs (validate, check_header, decode_limits, working_copy, layer, composite, ios_png, webp, the XML and web generators, Ledger, owned, rivals, targets, input_fingerprint, up_to_date, sync, units, android_unit, undo, apply), mobiler/src/upgrade.rs (SEED_PATHS), mobiler/src/build.rs + dev.rs + watch.rs + doctor.rs (the hooks), mobiler/templates/Android/app/src/main/res/mipmap-* (the stock icon)
Conformance:   xtask/tests/adr_conformance.rs::adr_0071_icon_seeds_and_marker, mobiler/src/icon.rs::validate_rejects, mobiler/src/icon.rs::fifo_source_is_refused_without_hanging, mobiler/src/icon.rs::oversized_source_is_refused_unread, mobiler/src/icon.rs::iccp_bomb_is_bounded, mobiler/src/icon.rs::writes_every_platform, mobiler/src/icon.rs::hand_edited_launcher_xml_skips_android_only, mobiler/src/icon.rs::hand_edited_webp_is_left_alone, mobiler/src/icon.rs::png_sibling_makes_android_foreign, mobiler/src/icon.rs::anydpi_v26_icon_makes_android_foreign, mobiler/src/icon.rs::ios_contents_json_without_our_png_makes_ios_foreign, mobiler/src/icon.rs::an_icon_added_after_a_sync_is_still_seen, mobiler/src/icon.rs::foreign_file_in_web_icon_survives, mobiler/src/icon.rs::broken_foreground_writes_nothing, mobiler/src/icon.rs::removing_icon_restores_stock, mobiler/src/icon.rs::removed_and_added_back_still_owns_its_files, mobiler/src/icon.rs::undo_keeps_a_hand_edited_sync_file, mobiler/src/icon.rs::interrupted_sync_is_retried_and_owns_its_files, mobiler/src/icon.rs::a_skipped_platform_is_not_up_to_date_after_restore, mobiler/src/icon.rs::a_skipped_web_unit_is_not_up_to_date_after_repair, mobiler/src/icon.rs::unchanged_inputs_skip_decoding, mobiler/src/icon.rs::no_png_beside_webp, mobiler/src/icon.rs::upgrade_reports_nothing_for_icon_files

## 1. Context (The Problem)

Every app got the template's launcher icon. Changing it took platform knowledge: Android's density buckets, the
adaptive icon's 108 dp layers and 72 dp visible area, the legacy rasters older launchers show, the themed
(monochrome) layer of Android 13+, iOS's single opaque 1024 px PNG, and a web favicon. Another project building on
mobiler asked to declare its icon once; Saldo had made its own by hand (tutorial chapter 10) and must not lose it.

The CLI already syncs `[fonts]` (ADR-0022) and `[splash]` (ADR-0048) from `mobiler.toml`. The icon differs from the
splash in two ways that shape this decision:
- most of what it writes are **binaries** (PNG, WebP) that can't carry a header saying the sync wrote them;
- several of its files **replace template files** (the legacy `ic_launcher*.webp`, the iOS `icon-1024.png`, the
  adaptive XMLs) rather than sitting beside them, so removing the section has to put the template's back.

## 2. Hypothesis

If one `[icon]` section is synced into every shell on every `build`, `dev` and `watch`, under the ownership rules
below, then the icon follows the config with no manual step, an app's hand-made icon is never overwritten, and a
bad input never leaves half-written resources.

**Inputs and validation** (all before any write; a failure is a warning naming the field and nothing is written):
- `foreground` (required) and `monochrome` (optional): PNG, square, at least 1024 and at most 16384 px per side.
  The size is read from the IHDR header before any decode, so an oversized file is never decoded. Before even
  that, from the file's metadata: it must be a regular file (a FIFO would block the open, `/dev/zero` never
  ends) of at most 256 MB. The unchanged-input fingerprint runs the same checks before it hashes a byte, so no
  build, `dev` or `watch` reads an unbounded file. The decoder's allocation is capped at the image at its widest
  (side² × 8 bytes, 16-bit RGBA) plus 64 MiB, from the checked header, so a crafted iCCP or zTXt chunk can't
  inflate without bound (image's default 512 MiB cap, which `[splash]` keeps, is too small for a 16384 px
  source). Palette,
  grayscale and 16-bit PNGs are accepted. A foreground with no transparency is accepted.
- `background` (required): exactly `#RRGGBB`.
- `padding` (optional): the % of the visible icon's side kept clear on each side, `0..=40`, default 18 (a square
  foreground at 18 fits Android's smallest, circular, mask).
- An unknown key is an error, not ignored.

**The working copy.** Each source is decoded once to straight-alpha RGBA8, premultiplied, and, when larger than
2048 px, downsized once (Lanczos3) to 2048; never upscaled. Every output is resized from it (Lanczos3, on
premultiplied pixels so transparent pixels never fringe an edge) and composited source-over.

**Generated files** (only for the platforms the app has: `Android/`, `iOS/`, `web/index.html`):
- **Android** (`app/src/main/res`):
  - `mipmap-{m,h,xh,xxh,xxxh}dpi/mobiler_icon_foreground.png` (108/162/216/324/432 px): transparent, the
    foreground centred in the 72 dp visible area at the padding; `mobiler_icon_monochrome.png` likewise when
    `monochrome` is set, removed (after the XML stops naming it) when it isn't.
  - `values/mobiler_icon.xml`: the background colour `mobiler_icon_background`.
  - `mipmap-anydpi/ic_launcher.xml` and `ic_launcher_round.xml` (same content, with the sync's header): background
    colour, foreground layer, and a `<monochrome>` layer that is the monochrome image, or else the foreground layer
    itself, so the themed icon is never lost (Android themes by the layer's alpha).
  - `mipmap-*/ic_launcher.webp` and `ic_launcher_round.webp` (48…192 px): the composite, the round one masked to an
    anti-aliased circle, **rewritten in place as lossless WebP**. The names are the template's, so no `.png` ever
    sits beside a `.webp` of the same resource (Android fails that build with a duplicate resource).
  - The template's `drawable/ic_launcher_foreground.xml` and `ic_launcher_background.xml` stay, unreferenced.
- **iOS:** `Assets.xcassets/AppIcon.appiconset/icon-1024.png`, the 1024 px composite as opaque RGB with no alpha
  channel (App Store rejects alpha). `Contents.json` is unchanged (already single-size).
- **Web:** `web/icon/mobiler-icon-32.png` (favicon) and `mobiler-icon-180.png` (apple-touch-icon), and a
  `mobiler:icon` block in `web/index.html` (a trunk `copy-dir` of `icon` and the two `<link>`s), inserted before
  `</head>` when missing, as `[fonts]` and `[splash]` do. `web/icon/` is shared with the app: only the sync's own
  files are ever removed, and the folder only when empty.

**Ownership.** The sync may write or remove a file only while it is:
- missing;
- the template's stock content (binaries byte for byte; text CRLF-tolerant, as ADR-0048);
- an XML file carrying the sync's header; or
- exactly what the ledger recorded for it (FNV-1a 64 of its bytes): the only way to own a binary it wrote.

Each platform is one unit. One file in a unit that the sync doesn't own (`web/index.html` aside, which is only
ever edited inside its block) skips that **whole platform** with a warning naming the file; a half-branded icon is
worse than none. A web page whose block can't be filled skips the web unit the same way.

A missing file is owned, but the app may have its own icon **beside** the sync's paths. These make the platform
the app's too (skipped with a warning naming the file, and undo leaves them alone):
- **Android, another file for the same resource:** in a `mipmap-*` folder, a file whose name up to the first `.`
  matches one the sync writes, with another extension (an icon generator's `ic_launcher.png` where our
  `ic_launcher.webp` would go). Writing ours beside it makes Gradle fail with "Duplicate resources".
- **Android, a versioned adaptive icon:** any `res/mipmap-anydpi-v*/ic_launcher*.xml` (Android Studio's Image
  Asset layout). It wins over the sync's `mipmap-anydpi/` on every device at minSdk 26, so our icon would
  silently not show.
- **iOS:** an `AppIcon.appiconset/Contents.json` that doesn't list `icon-1024.png` (or doesn't parse): our PNG
  would sit there unreferenced.

These are checked on every run, the unchanged-input skip included, so an icon added after a sync is still seen.
A skipped unit keeps its ledger entries. Files only the sync creates (`mobiler_icon_*`, `values/mobiler_icon.xml`,
`web/icon/mobiler-icon-*`) are created with the section and removed only while owned.

**Seeds.** `mipmap-anydpi/ic_launcher.xml` and `ic_launcher_round.xml` join `SEED_PATHS`, extending ADR-0042's
seed list the way ADR-0048 did: `mobiler upgrade` creates them when missing and never merges them, so it never
fights the sync or an app's own adaptive icon. (They were merged Shell-class files before; Saldo's hand-made one
survived only because the template hadn't changed.) The other files are binaries, `Own`-class already, or not
template files.

**The ledger** `.mobiler/icon.json`: `active`, `files` (path → one hash, or a list: while a sync is in flight, or carried over for a skipped unit) and
`input`, the fingerprint of the generator's version, the source images' bytes, the background and the padding.
- **Before the first write**, an interim ledger is written, giving each file both its old hash and its new one and
  no `input`. A sync that stops part-way still owns every file it touched, and the next one never skips.
- **The final ledger is written last**, with one hash per file it wrote (a skipped unit's carried entries may
  stay lists).
- **The unchanged-input skip:** when the ledger is active, its `input` equals the current fingerprint, every target
  file still hashes to its entry and the web block is already filled, the sync returns without decoding an image.
  The final ledger **omits `input` whenever any unit was skipped**, so a platform skipped for a foreign file (or a
  broken web block) is regenerated as soon as it is restored, never left on the old colours.

**Undo** (the section removed while the ledger is active): each adaptive XML, legacy raster and the iOS PNG still
as the sync wrote it (ledger hash, or the header for XML) goes back to the template's stock bytes; the web block
is emptied; the sync's own files are removed only while owned, and a hand-edited one is kept with a warning and its
ledger entry. If a hand-edited adaptive XML still names `mobiler_icon_*`, the layers and colour are kept and the
warning says so. The ledger stays, marked inactive, so adding `[icon]` back still recognises the files; undo
happens once.

**Write order:** every target checked for a symlink on the way first; then the interim ledger, the images, the
files that refer to them (colour XML, adaptive XMLs, the web page), the removals, the final ledger. Undo writes the
adaptive XMLs back to stock before it removes the layers they named.

Without `[icon]` and without a ledger the sync touches nothing, and writes no ledger.

### 2.1. Refutation Conditions

- **Condition 1 — the seeds, the stock monochrome layer, `.webp` only, and a committable ledger.**
  - **Validation Metric:** `adr_0071_icon_seeds_and_marker`, `upgrade_reports_nothing_for_icon_files`.
- **Condition 2 — validation before any write.**
  - **Validation Metric:** `validate_rejects`, `broken_foreground_writes_nothing`,
    `fifo_source_is_refused_without_hanging`, `oversized_source_is_refused_unread`, `iccp_bomb_is_bounded`.
- **Condition 3 — every platform's files are generated.**
  - **Validation Metric:** `writes_every_platform`, `no_png_beside_webp`.
- **Condition 4 — a hand-edited file is left alone, and its whole platform skipped.**
  - **Validation Metric:** `hand_edited_launcher_xml_skips_android_only`, `hand_edited_webp_is_left_alone`.
- **Condition 4a — the app's own icon beside the sync's paths skips its platform.**
  - **Validation Metric:** `png_sibling_makes_android_foreign`, `anydpi_v26_icon_makes_android_foreign`,
    `ios_contents_json_without_our_png_makes_ios_foreign`, `an_icon_added_after_a_sync_is_still_seen`.
- **Condition 5 — the ledger owns binaries across a removal, an interruption and a skip.**
  - **Validation Metric:** `removed_and_added_back_still_owns_its_files`,
    `interrupted_sync_is_retried_and_owns_its_files`, `a_skipped_platform_is_not_up_to_date_after_restore`,
    `a_skipped_web_unit_is_not_up_to_date_after_repair`, `unchanged_inputs_skip_decoding`.
- **Condition 6 — undo restores stock only for owned files.**
  - **Validation Metric:** `removing_icon_restores_stock`, `undo_keeps_a_hand_edited_sync_file`,
    `foreign_file_in_web_icon_survives` (an app's file in the shared `web/icon/` stays).
- **Condition 7 (runtime):** the adaptive icon, the themed icon on Android 13+ and a legacy raster on an Android
  emulator; the home-screen icon on the maintainer's iPhone; the favicon in a trunk build. Checked in the release's
  device pass.

## 3. Considered Options & Rationale for Refutation

- **Option A — sync like `[splash]`** `[recorded: docs/superpowers/specs/2026-11-27-icon-and-assets-design.md §3]`
  Chosen: "Regenerate on every run; write a file only while it is stock, carries the sync's header, or matches the
  sync's ledger; validate everything before writing anything."
- **Option B — a one-time `mobiler icon` generator** `[recorded: docs/superpowers/specs/2026-11-27-icon-and-assets-design.md §3]`
  Rejected: "a changed PNG would need a manual re-run, and the generated copy goes stale silently, against the
  request's acceptance criterion 2."
- **Option C — generate at build time, uncommitted** `[recorded: docs/superpowers/specs/2026-11-27-icon-and-assets-design.md §3]`
  Rejected "for the reason ADR-0022 gives: a plain Gradle, Xcode or trunk build would miss the files."
- **Option D — write `.png` legacy icons beside the template's `.webp`** `[recorded: docs/superpowers/specs/2026-11-27-icon-and-assets-design.md §5.2]`
  Rejected: Android fails the build on two files for one resource name; the WebPs are rewritten in place.
- **Option E — skip only the foreign file, write the rest of its platform** `[recorded: docs/superpowers/specs/2026-11-27-icon-and-assets-design.md §5.3, "a half-branded icon is worse than none"]`
  Rejected.
- **Option F — own binaries by comparing them with what the sync would generate now** `[reconstructed]`
  Rejected: a changed input changes the expected bytes, so a file the sync wrote last time would look foreign. The
  ledger records what was written.

## 4. Decision & Rationale for Corroboration

Option A with the rules in §2. The working copy bounds the cost of a 16384 px source to one decode and one resize;
the unchanged-input skip makes a build with an unchanged icon decode nothing.

**Mutation proof** (2026-11-27; each mutation reverted after it was seen to fail):
- Dropping `mipmap-anydpi/ic_launcher_round.xml` from `SEED_PATHS` failed `adr_0071_icon_seeds_and_marker`
  ("…/ic_launcher_round.xml is written by the [icon] sync but isn't a seed") and
  `upgrade_reports_nothing_for_icon_files` ("… is an upgrade seed").
- Removing `<monochrome>` from the template's `ic_launcher.xml` failed `adr_0071_icon_seeds_and_marker` ("the
  template's ic_launcher.xml lost its <monochrome> layer").
- Commenting out the template's `<monochrome>` (`<!--<monochrome … /> -->`) failed it too.
- Adding `.mobiler/` to the template's `.gitignore` failed it ("the template ignores `.mobiler/`"); so did adding
  `.mobiler/*` ("the template's .gitignore has `.mobiler/*`, which ignores .mobiler/icon.json").
- Writing the legacy icon as `ic_launcher.png` (PNG bytes) failed `adr_0071_icon_seeds_and_marker` ("icon.rs
  writes "ic_launcher.png", a twin of the template's .webp") and `no_png_beside_webp` ("mipmap-xxxhdpi:
  ic_launcher is both .png and .webp"), with `writes_every_platform` and `unchanged_inputs_skip_decoding`.
- Making `owned()` ignore the ledger failed `removed_and_added_back_still_owns_its_files`,
  `interrupted_sync_is_retried_and_owns_its_files`, `removing_icon_restores_stock`,
  `undo_keeps_a_hand_edited_sync_file`, `hand_edited_webp_is_left_alone`, both skipped-unit tests,
  `unchanged_inputs_skip_decoding` and `writes_every_platform` (the second sync called its own files hand-edited).
- Making `owned()` always true failed `hand_edited_launcher_xml_skips_android_only`,
  `hand_edited_webp_is_left_alone`, `undo_keeps_a_hand_edited_sync_file` and
  `a_skipped_platform_is_not_up_to_date_after_restore`.
- Skipping the undo when the section is gone failed `removing_icon_restores_stock` ("…/ic_launcher.xml back to the
  template"), with `undo_keeps_a_hand_edited_sync_file` and `foreign_file_in_web_icon_survives`.
- Letting undo remove every sync-only file, owned or not, failed `undo_keeps_a_hand_edited_sync_file`.
- Recording the input fingerprint after a skip failed `a_skipped_platform_is_not_up_to_date_after_restore`
  ("Android regenerated: SyncReport { written: [] … }") and `a_skipped_web_unit_is_not_up_to_date_after_repair`.
- Not writing the interim ledger before the first write failed `interrupted_sync_is_retried_and_owns_its_files`
  (the retry called the half-written files hand-edited).
- Writing the ledger when validation fails failed `broken_foreground_writes_nothing` ("the tree is
  byte-identical").
- Dropping the 1024 px minimum failed `validate_rejects` (the 1000 px source "must be rejected"); so did widening
  the padding bound to 41.
- Before `rivals` existed (a missing file always owned), `png_sibling_makes_android_foreign`,
  `anydpi_v26_icon_makes_android_foreign`, `ios_contents_json_without_our_png_makes_ios_foreign` and
  `an_icon_added_after_a_sync_is_still_seen` all failed (no warning: `[]`). Dropping only the `rivals` check from
  `up_to_date` failed `an_icon_added_after_a_sync_is_still_seen`.
- Before the metadata check (the fingerprint read the whole source first), `fifo_source_is_refused_without_hanging`
  failed ("a FIFO source hung the sync: Timeout") and `oversized_source_is_refused_unread` failed ("an oversized
  source is never fingerprinted").
- With `max_alloc = None` (the first version), `iccp_bomb_is_bounded` failed ("the iCCP chunk inflated past the
  limit": a 1.7 MB file's iCCP chunk inflated to 283 MB).
- Reverting restored green.

## 5. Consequences (Positive and Negative Predictions)

- **Positive:** one foreground PNG and a colour give the adaptive, themed, legacy, iOS and web icons, kept in step
  on every build.
- **Positive:** a hand-made icon (Saldo's, before its reset) is never overwritten; the sync names the file and
  leaves its platform alone.
- **Positive:** the Android 12+ splash falls back to `@mipmap/ic_launcher`, so the declared icon also shows at
  launch when `[splash]` has no logo.
- **Negative:** `.mobiler/icon.json` must be committed, or a teammate's sync treats the generated binaries as
  hand-made and skips every platform.
- **Negative:** while a platform is skipped for a foreign file, the ledger has no `input`, so every build decodes
  and resizes the sources again until the file is restored or deleted.
- **Negative:** iOS is checked only by compile in CI and the maintainer's device; no simulator runs in CI.
- **Negative:** no iOS 18 dark or tinted icon variants, no distinct round artwork (the round icon is the square
  masked), no web app manifest.
- **Negative:** the template's `drawable/ic_launcher_foreground.xml` and `ic_launcher_background.xml` stay in the
  app, unreferenced, while `[icon]` is set.
- **Negative:** as seeds, the adaptive XMLs no longer receive template changes through `upgrade`; a future change
  to the stock adaptive icon reaches new apps only.
- **Negative:** the CLI's `image` dependency gains the `webp` feature.
- **Negative:** `foreground` and `monochrome` aren't confined to the app, as `[splash]`'s logos aren't: an absolute
  path, a `../` path and a symlink are all followed, for reading only. The sync never writes through a source
  path, and what it reads is checked (a regular file, at most 256 MB, a PNG header) before a byte is decoded, so
  the reach is limited to the CLI user's own readable files ending up in their own app's icon. `watch` then
  watches that outside folder too.
