# ADR-0048: `mobiler.toml` `[splash]` (launch colours and a logo) is synced into the shells by the CLI, which writes app-owned seed files only while they are stock or carry its header, fills marker blocks in framework files without ever inserting them, and owns only the logo files it creates

Status:        Accepted
Date decided:  2026-10-03
Deciding PRs:  #282
Supersedes:    none
Code anchor:   mobiler/src/splash.rs (validate, fit, the generators, sync, undo, owned_seed, block_change), mobiler/src/build.rs + dev.rs + watch.rs + doctor.rs (the hooks), mobiler/templates/Android/app/src/main/res/values{,-night}-v31/mobiler_themes.xml and mobiler/templates/iOS/project.yml (the marker blocks)
Conformance:   xtask/tests/adr_conformance.rs::adr_0048_splash_markers_and_seeds, mobiler/src/splash.rs::hand_edited_seed_is_left_alone_with_a_warning, mobiler/src/splash.rs::broken_logo_leaves_everything_unchanged, mobiler/src/splash.rs::removing_splash_undoes_the_sync_but_keeps_colours

## 1. Context (The Problem)

An app's launch screen was the template's: a background colour in app-owned seed files (ADR-0042), the launcher icon
on Android 12+, and nothing branded on iOS or the web. The maintainer asked, on 2026-10-01 and scoped on 2026-10-03,
that app builders set their own colours and a logo without learning each platform's files and rules:
- Android's density buckets, layer-lists and the Android 12 icon circle;
- iOS's asset catalogue and its unscalable launch image;
- the web page before the app loads.

ADR-0042 anticipated a `[splash]` sync writing the seed files.

## 2. Hypothesis

If one `mobiler.toml` section is synced into every shell the way `[fonts]` is (ADR-0022), then:
- an app builder sets the launch screen once;
- the app's own edits are never overwritten;
- a framework file never gains content the app didn't ask for.

The rules that make that hold:
- **Seeds** (the two Android colour files, the launch layer-list and the iOS colourset) are written only while each is
  missing, equal to the template's stock content, marked with the sync's header, or (for JSON, which has no comments)
  equal to what the sync recorded in `.mobiler/splash.json`. Otherwise the sync warns and leaves the file.
- **Framework files** (the v31 themes, `project.yml`) only have their `mobiler:splash` marker blocks filled. A file
  without the markers gets a warning naming `mobiler upgrade --apply` and is never edited.
- **The sync's own files** (the logo PNGs, the Android 12 inset icon, the iOS imageset, `web/splash/`, the web block)
  are written with a logo and removed without one.
- **Nothing is written until everything validates:** a bad colour, size or logo writes nothing.
- **Removing `[splash]`** undoes the sync's own files and blocks, and keeps the colours (app-owned again).

### 2.1. Refutation Conditions

- **Condition 1: the markers ship, and every seed the sync writes is an upgrade seed.**
  - **Validation Metric:** `adr_0048_splash_markers_and_seeds`.
- **Condition 2: a hand-edited seed is never overwritten.**
  - **Validation Metric:** `hand_edited_seed_is_left_alone_with_a_warning`.
- **Condition 3: a bad logo writes nothing.**
  - **Validation Metric:** `broken_logo_leaves_everything_unchanged`.
- **Condition 4: removing the section undoes the sync, but keeps the colours.**
  - **Validation Metric:** `removing_splash_undoes_the_sync_but_keeps_colours`.
- **Condition 5 (runtime):** without a logo, the Android 12 splash is identical to CLI 0.64.2's; with one, it shows
  the logo. Checked on emulators in PR #282.

## 3. Considered Options & Rationale for Refutation

- **Option A: document the files and let apps edit them** `[recorded: CLI 0.61.0 README, "Launch window colours"]`
  Rejected as the only way. It needs platform knowledge, and the logo needs resizing per platform. It stays possible:
  without `[splash]` the files are the app's to edit.
- **Option B: the sync always owns the seed files** `[reconstructed]`
  Rejected. It would overwrite an app's hand-set colours, breaking ADR-0042's promise.
- **Option C: a theme attribute set from a separate generated resource file** `[reconstructed]`
  Rejected. Android defines a style once per resource folder, so the Android 12 icon attribute must live in the
  framework theme. A marker block there keeps the no-logo theme byte-identical to before.
- **Option D: the rules above** `[recorded: docs/superpowers/specs/2026-10-03-splash-config-design.md and its plan amendment]`
  Chosen.

## 4. Decision & Rationale for Corroboration

Option D.
- **Android:** the logo is resized with the `image` crate (PNG only, Lanczos3, never upscaled) to xxxhdpi (4×). The
  launch layer-list centres it at its dp size. The Android 12 icon is an `inset` that fits it into the 113 dp square
  inscribed in the 160 dp icon circle, inside the 240 dp box. It is named `mobiler_splash_logo_icon`, because
  `mobiler_splash_icon` already exists as an alias in the colour seed.
- **iOS:** the logo goes into a `MobilerSplashLogo` imageset at @3x, and `UIImageName` is written into the
  `UILaunchScreen` block.
- **Web:** a `<style>` block (background via `prefers-color-scheme`, the logo on `body:empty`) and copies at 2×.

**Mutation proof:**
- Removing a template theme's begin marker failed `adr_0048_splash_markers_and_seeds`.
- Dropping `mobiler_launch.xml` from `SEED_PATHS` failed it too ("written by the sync but isn't a seed").
- Treating every seed as the sync's own (ignoring the header check) failed
  `hand_edited_seed_is_left_alone_with_a_warning`.
- Writing despite an unreadable logo failed `broken_logo_leaves_everything_unchanged`.
- Skipping the undo when the section is gone failed `removing_splash_undoes_the_sync_but_keeps_colours`.
- Reverting restored green.

## 5. Consequences (Positive and Negative Predictions)

- **Positive:** a branded launch screen on Android 8+, Android 12+, iOS and the web from five lines of config,
  following light/dark.
- **Positive:** an app's hand-edited launch files are never overwritten, and a framework file changes only inside its
  markers.
- **Negative:** on Android 12+ a logo larger than about 113 dp is shown smaller than on Android 8–11, because Android
  masks the icon to a circle.
- **Negative:** an app made before CLI 0.65 shows the logo on Android 12+ and iOS only after `mobiler upgrade --apply`
  adds the markers; the colours work at once.
- **Negative:** a translucent background is used as opaque.
- **Negative:** the CLI gains the `image` dependency.
- **Negative:** `.mobiler/splash.json` must be committed, or a teammate's sync treats the iOS colourset as
  hand-edited.
- **Negative:** the iOS launch screen itself is verified only by compile in CI. Its look needs a simulator or device.
