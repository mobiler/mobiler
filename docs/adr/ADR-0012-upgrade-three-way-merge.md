# ADR-0012: `mobiler upgrade` refreshes an app's generated files by a three-way merge against the stored `.mobiler/base/` snapshot; a conflict is left as `<file>.mobiler-new` and never applied, and the app's own code is never touched

Status:        Accepted
Date decided:  2026-06-01
Deciding PRs:  #44 (the three-way merge); built on #33 (2026-05-31, the command, file classes and review-first default) and #43 (2026-06-01, anchor splice); fixed by #194 (2026-07-19, a new anchor no longer fails the fallback)
Supersedes:    none
Code anchor:   mobiler/src/upgrade.rs (classify, sync_file, three_way, two_way, merge_anchors, seed_baseline), mobiler/src/new.rs (seeds the baseline), mobiler/templates/.gitignore (commit `.mobiler/`)
Conformance:   mobiler/src/upgrade.rs::three_way_applies_framework_change_preserves_edit_and_flags_conflict, mobiler/src/upgrade.rs::new_seeds_baseline_so_upgrade_is_idempotent, mobiler/src/upgrade.rs::bumps_dep_stamps_and_leaves_app_code_untouched

## 1. Context (The Problem)

`mobiler new` copies the native shells (the Android and iOS widget interpreters, `Core.kt`,
`Core.swift`, gradle and xcodegen files) into the app. After that they belong to the app, so
framework fixes to them never reach an existing app. The web shell is a crate dependency and
upgrades by a version bump; iOS and Android did not.

The copied files are also edited after scaffolding: `mobiler plugin add` injects lines at
`mobiler:*` anchors (ADR-0011), and users change them by hand. A plain
copy of the new template would wipe both. A copy that skips every edited file strands framework
changes: PR #43 found that after the vocabulary pass "the upgraded app wouldn't compile" unless the
user hand-merged one `build.gradle.kts` line.

## 2. Hypothesis

If `new` records the exact generated text of every managed file under `.mobiler/base/`, and
`upgrade` merges each file three ways (that snapshot → the app's file → the new template), then:

- framework changes, the user's edits and plugin injections all land when they don't overlap;
- overlapping edits never overwrite the user's file: they are written as `<file>.mobiler-new` with
  conflict markers, even with `--apply`;
- an app already on the current template upgrades as a no-op;
- the app's own code (`shared/src/`, except the CLI-owned `shared/src/bin/codegen.rs`), the Cargo
  manifests, per-app identity files and binaries are never touched.

What the merge anchors on: the ancestor is the per-file snapshot in `.mobiler/base/<path>`, the
pristine, placeholder-substituted template text the file was last generated or merged from. The
snapshot advances to the new template only when the file on disk now incorporates it (a clean
`--apply`, or a file already equal). A file left as `.mobiler-new` keeps its old ancestor, so the
next run merges against the same base. The `mobiler:*` anchor comments play no part in the
three-way path. They matter only in the fallback for a file that has no snapshot
(`two_way` → `merge_anchors`), which rebuilds the file from the new template and splices back the
lines found directly above each anchor. That is every file of an app scaffolded before
`.mobiler/base/` existed, and also a file that only became managed in a later release: CLI 0.48
made `shared/src/bin/codegen.rs` a managed file (PR #193), so 0.47 apps had no snapshot for it.

### 2.1. Refutation Conditions

- **Condition 1 — non-overlapping changes merge, overlapping ones stay out.**
  A framework change and a user edit on separate lines both land; the same line changed on both
  sides leaves the file untouched and writes a conflict-marked `.mobiler-new`, even with `--apply`.
  - **Validation Metric:** `three_way_applies_framework_change_preserves_edit_and_flags_conflict`
    in `mobiler/src/upgrade.rs`.
- **Condition 2 — a fresh app upgrades to nothing.**
  An app materialised and baselined exactly as `new` does it reports no changed, merge or conflict
  files.
  - **Validation Metric:** `new_seeds_baseline_so_upgrade_is_idempotent` in `mobiler/src/upgrade.rs`.
- **Condition 3 — app code is never touched.**
  `shared/src/app.rs` stays byte-identical and gets no sidecar, while `mobiler-core` is bumped.
  - **Validation Metric:** `bumps_dep_stamps_and_leaves_app_code_untouched` in
    `mobiler/src/upgrade.rs`.

## 3. Considered Options & Rationale for Refutation

- **Option A — overwrite the shells with the new template** `[recorded: PR #33 body, MERGE files are "never auto-overwritten (a blind copy would wipe installed plugins)"]`
  Rejected: it destroys plugin injections and hand edits.
- **Option B — two-way, review-first: write every changed file as `.mobiler-new`, leave anchor files hands-off** `[recorded: PR #33 body, "Existing apps have no recorded baseline, so the command can't auto-tell your shell edits from framework drift — hence the review-first default. (User-approved model.)"]`
  Shipped first (CLI 0.14). Replaced because anchor files never received shell changes, which
  broke the build after an upgrade (PR #43).
- **Option C — re-emit anchor files from the new template and splice back the lines above each anchor** `[recorded: PR #43 body]`
  Shipped in CLI 0.15.1. PR #44 calls the three-way merge "the robust successor to the 0.15.1
  anchor-splice heuristic", which could not handle "a framework change on the line directly above a
  plugin anchor". It survives as the fallback for apps without a baseline.
- **Option D — three-way merge against a committed snapshot of the generated files** `[recorded: PR #44 body; mobiler/README.md "It does a **true 3-way merge**"]`
  Chosen.
- **Option E — regenerate the shells on every build, so there is nothing to merge** `[reconstructed]`
  Rejected in effect by ADR-0001's copied-shell model and by plugin injection, which needs the
  files to live in the app.

## 4. Decision & Rationale for Corroboration

Option D, in CLI 0.16.0. `new` seeds `.mobiler/base/` (`seed_baseline`) and the template
`.gitignore` tells apps to commit `.mobiler/`, so collaborators and CI merge against the same
ancestor. The merge is `diffy::merge`. By default even a clean merge is only offered as
`<file>.mobiler-new`; `--apply` writes a clean merge in place after saving `<file>.mobiler-bak`.

The fallback had one real defect: a release that added a new anchor could never find it in an older
file, so the splice failed and the file was left as a sidecar. CLI 0.48.0 shipped with it and the
upgraded app failed to build. PR #194 made a missing anchor land bare, and verified an upgrade from
the published 0.47.0 with "zero manual steps". PR #194 also records: "Upgrade on an already-current
app is a clean no-op (zero sidecars)." Release PRs point existing apps at it, for example PR
#242: "Existing apps: `mobiler upgrade --apply`."

## 5. Consequences (Positive and Negative Predictions)

- **Positive:** framework shell fixes reach existing apps without losing plugin injections or hand
  edits.
- **Positive:** a template change that touches an anchor file (`project.yml`, `Core.kt`) is safe to
  ship, because it merges instead of being skipped.
- **Negative:** the app must keep `.mobiler/base/` committed and unedited. A deleted or edited
  snapshot turns the next run into the weaker two-way fallback or a wrong merge.
- **Negative:** in the two-way fallback, `--apply` writes the rebuilt file: a plain shell file
  becomes the new template, and an anchor file keeps only the lines directly above its anchors.
  Any other hand edit survives only in `<file>.mobiler-bak`.
- **Negative:** plugin bodies copied by `plugin add` are not templates, so this merge never
  refreshes them. That gap needed its own drift warning (ADR-0011).
- **Negative:** a template edit near a common user edit produces a real conflict the user must
  resolve by hand; PR #193's upgrade check hit one in `Core.kt`.
- **Negative:** the fallback's splice depends on the template line directly above each anchor
  staying stable. That is a rule for template authors, checked by no test.
