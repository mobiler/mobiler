# ADR-0043: `mobiler upgrade` three-way merges an app's generated files against `.mobiler/base/`, never applies a conflict and never touches the app's own code; its review copies and backups never land where a build reads them

Status:        Accepted
Date decided:  2026-10-01
Deciding PRs:  #266; restates ADR-0012 (#33, #43, #44, #194) and the backup move of #258
Supersedes:    ADR-0012
Code anchor:   mobiler/src/upgrade.rs (classify, sync_file, three_way, two_way, merge_anchors, seed_baseline, write_review, review_rel, write_backup), mobiler/src/new.rs (seeds the baseline), mobiler/templates/.gitignore (commit `.mobiler/`, ignore `.mobiler/backup/` and `.mobiler/new/`)
Conformance:   mobiler/src/upgrade.rs::three_way_applies_framework_change_preserves_edit_and_flags_conflict, mobiler/src/upgrade.rs::new_seeds_baseline_so_upgrade_is_idempotent, mobiler/src/upgrade.rs::bumps_dep_stamps_and_leaves_app_code_untouched, mobiler/src/upgrade.rs::review_copies_of_android_resources_go_outside_res, mobiler/src/upgrade.rs::seed_files_created_even_when_theme_is_offered_as_new, mobiler/src/upgrade.rs::changed_shell_writes_new_then_apply_overwrites_with_backup

## 1. Context (The Problem)

ADR-0012 decided how `mobiler upgrade` brings framework changes into an app's copied shells: a
three-way merge against the snapshot in `.mobiler/base/`, review-first by default, conflicts never
applied, the app's own code never touched. Read it for the history and the rejected options; this
record restates that decision and changes one part of it.

ADR-0012 also fixed *where* its side files go: a review copy at `<file>.mobiler-new` and, with
`--apply`, a backup at `<file>.mobiler-bak`, both next to the file. Next to the file is inside the
build's inputs. Android's resource merge rejects any file in a resource folder whose name does not end
in `.xml`:

- PR #258 (2026-09-30) found that `res/xml/x.xml.mobiler-bak` failed the build, and moved backups to
  `.mobiler/backup/<path>`, without a new record.
- The dark-theme release (spec `docs/superpowers/specs/2026-10-01-dark-theme-gaps-design.md`) changes
  `res/values/themes.xml`. A verification upgrade of a CLI 0.60.2 app with a customised theme wrote
  `res/values/themes.xml.mobiler-new`, and the build failed: "The file name must end with .xml". Every
  app that upgraded without `--apply`, or had edited its theme, would have stopped building.

## 2. Hypothesis

If the merge stays as ADR-0012 decided, and every side file is written where no build reads it, then
everything ADR-0012 promised holds, and in addition:

- an app builds after `mobiler upgrade`, with or without `--apply`, whatever was offered for review;
- every review copy and backup is still easy to find: the report prints its path.

The merge itself: `new` records the exact generated text of every managed file under
`.mobiler/base/`; `upgrade` merges each file three ways (that snapshot → the app's file → the new
template, `diffy::merge`). Non-overlapping framework changes, user edits and plugin injections all
land; overlapping edits never overwrite the app's file, even with `--apply`; an app already on the
current template upgrades as a no-op; the app's own code (`shared/src/` except the CLI-owned
`shared/src/bin/codegen.rs`), the Cargo manifests, per-app identity files and binaries are never
touched. Files without a snapshot fall back to the anchor-aware two-way reconcile ADR-0012 describes.
App-owned seed files are created when missing and never touched (ADR-0042).

Where side files go:

- **Review copy** (a changed or conflicting file without `--apply`, or an anchor file whose splice
  failed): `<file>.mobiler-new` next to the file, except an Android resource (a path under
  `…/src/<sourceSet>/res/`), whose copy goes to `.mobiler/new/<path>.mobiler-new`.
- **Backup** (`--apply` overwrote a file): `.mobiler/backup/<path>`.
- The template's `.gitignore` ignores `.mobiler/backup/` and `.mobiler/new/`; the rest of `.mobiler/`
  is committed.

### 2.1. Refutation Conditions

- **Condition 1 — non-overlapping changes merge, overlapping ones stay out.**
  - **Validation Metric:** `three_way_applies_framework_change_preserves_edit_and_flags_conflict`.
- **Condition 2 — a fresh app upgrades to nothing.**
  - **Validation Metric:** `new_seeds_baseline_so_upgrade_is_idempotent`.
- **Condition 3 — app code is never touched.**
  - **Validation Metric:** `bumps_dep_stamps_and_leaves_app_code_untouched`.
- **Condition 4 — no side file in an Android resource folder.** A resource's review copy goes under
  `.mobiler/new/`; other files keep `<file>.mobiler-new`; a backup goes under `.mobiler/backup/`, never
  next to the file.
  - **Validation Metric:** `review_copies_of_android_resources_go_outside_res`,
    `seed_files_created_even_when_theme_is_offered_as_new` (a theme offered for review lands outside
    `res/`), `changed_shell_writes_new_then_apply_overwrites_with_backup`.

## 3. Considered Options & Rationale for Refutation

The merge options are ADR-0012's (A–E); Option D, the three-way merge, stands. For the side files:

- **Option F — keep every side file next to its file** `[recorded: ADR-0012 §4]`
  ADR-0012's choice. Rejected: it breaks the Android build for any resource file (PR #258; the
  dark-theme verification, 2026-10-01).
- **Option G — give a resource's review copy a `.xml` name (`themes.mobiler-new.xml`)** `[reconstructed]`
  Rejected: Gradle would compile it as a resource, so the style is defined twice (and a conflict
  copy's markers are not valid XML).
- **Option H — move every review copy under `.mobiler/new/`** `[reconstructed]`
  Rejected: it changes the documented place for every file (Kotlin, Swift, gradle, xcodegen) where
  next-to-the-file works and is what users and release notes expect.
- **Option I — only Android resources move; backups always under `.mobiler/backup/`** `[recorded: PR #258 for backups; reconstructed for review copies, decided 2026-10-01 after the dark-theme verification]`
  Chosen.

## 4. Decision & Rationale for Corroboration

Option D for the merge, Option I for side files: `write_review` and `review_rel` place a review copy,
`write_backup` places a backup, and the report prints each review copy's real path.

**Mutation proof:**
- Making `review_rel` always return `<file>.mobiler-new` failed
  `review_copies_of_android_resources_go_outside_res` and `seed_files_created_even_when_theme_is_offered_as_new`
  ("a review copy inside res/ breaks the Android build").
- Making `write_backup` always write `<file>.mobiler-bak` next to the file failed
  `changed_shell_writes_new_then_apply_overwrites_with_backup` (no `.mobiler/backup/rust-toolchain.toml`).
- Reverting restored green (97/97). The three tests carried over from ADR-0012 predate the ADR
  practice and are not re-proven here.

## 5. Consequences (Positive and Negative Predictions)

- **Positive:** an upgraded app builds whatever was left for review, so "upgrade, build, then review"
  works.
- **Negative:** review copies live in two places. A user looking next to a resource file finds
  nothing; the upgrade report and the READMEs point to `.mobiler/new/`.
- **Negative:** iOS asset catalogs and other build inputs that might reject stray files are not
  covered; only Android `res/` is known to fail.
- **Negative:** `.mobiler/new/` is never cleaned up by the CLI; the user deletes a copy after merging it.
- All of ADR-0012's consequences still hold: `.mobiler/base/` must stay committed and unedited, the
  two-way fallback's `--apply` keeps only the lines above anchors, plugin bodies are not refreshed
  (ADR-0011), overlapping edits need a hand merge, and the splice depends on stable template lines.
