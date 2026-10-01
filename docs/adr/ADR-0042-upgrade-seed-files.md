# ADR-0042: `mobiler upgrade` has a seed class: a listed app-owned file the template provides is created when missing and never touched after that

Status:        Accepted
Date decided:  2026-10-01
Deciding PRs:  #266
Supersedes:    none
Code anchor:   mobiler/src/upgrade.rs (`Class::Seed`, `SEED_PATHS`, `classify`, `sync_file`, `seed_dir`)
Conformance:   mobiler/src/upgrade.rs::seed_paths_classify_as_seed_and_exist_in_the_template, mobiler/src/upgrade.rs::missing_seed_files_are_created_and_existing_ones_never_touched, mobiler/src/upgrade.rs::seed_write_does_not_follow_a_symlink

## 1. Context (The Problem)

ADR-0012 (now restated by ADR-0043) sorts every template file into a class for `mobiler upgrade`: `Own` files (the app's code,
identity files, binaries) are never touched, `Shell` files are three-way merged against the stored
baseline, and `Merge` files carry anchors and are offered as `.mobiler-new`.

The dark-theme release (spec `docs/superpowers/specs/2026-10-01-dark-theme-gaps-design.md`) adds
launch-window themes that reference app-owned values: the light and dark background colours and the
splash icon (`res/values/mobiler_splash.xml`, `res/values-night/mobiler_splash.xml`,
`res/drawable/mobiler_launch.xml`) and an iOS colour set. The next release's `[splash]` section in
`mobiler.toml` will write those same files.

`upgrade` skipped `Own` files entirely, even when they were missing. A new app-owned file would therefore
never reach an existing app, and the shell theme that references it would fail to build after the
upgrade.

## 2. Hypothesis

If `upgrade` has a `Seed` class for an explicit list of paths, created when missing and never read,
merged, overwritten or baselined after that, then:

- an app upgrading from an older CLI gains the seed files and still builds, even if it declines the new
  themes (they become `.mobiler-new`, and the unused resources do no harm);
- an app's edits to a seed file, or a later `[splash]` sync's output, never meet a three-way merge or a
  `.mobiler-new` conflict;
- every listed path stays shipped by the template.

### 2.1. Refutation Conditions

- **Condition 1 — the list is classified and shipped.** Every `SEED_PATHS` entry classifies as `Seed`
  (winning over the `Assets.xcassets/` `Own` prefix) and exists in the embedded template.
  - **Validation Metric:** `seed_paths_classify_as_seed_and_exist_in_the_template`.
- **Condition 2 — created when missing, never touched after.** Upgrading an app without them creates
  every seed file; an edited seed file stays byte-identical across `upgrade --apply`, gets no
  `.mobiler-new` and no baseline.
  - **Validation Metric:** `missing_seed_files_are_created_and_existing_ones_never_touched`; also
    `seed_files_created_even_when_theme_is_offered_as_new`.
- **Condition 3 — a seed write stays in the project.** A seed path occupied by anything, even a dangling
  symlink, counts as present; the file is created with `create_new`, so a link is never followed.
  - **Validation Metric:** `seed_write_does_not_follow_a_symlink`.

## 3. Considered Options & Rationale for Refutation

- **Option A — classify them `Own`** `[reconstructed]`
  Rejected: `Own` files never reach an existing app, so the upgraded themes reference resources that
  don't exist and the build breaks.
- **Option B — classify them `Shell`** `[reconstructed]`
  Rejected: the values belong to the app (and later to the `[splash]` sync). As shell files they would
  meet three-way merges and `.mobiler-new` conflicts whenever a template default changed, and the
  first upgrade without a baseline would offer the app's own colours back as a conflict.
- **Option C — a `Seed` class: create when missing, never touch after** `[recorded: docs/superpowers/specs/2026-10-01-dark-theme-gaps-design.md, decision 1]`
  Chosen. It extends ADR-0012 / ADR-0043 with a fourth class; it supersedes nothing.

## 4. Decision & Rationale for Corroboration

Option C. `SEED_PATHS` lists the four files; `classify` checks it before the `Own` rules; `sync_file`
writes a missing seed file (`create_new`, only when nothing is at the path) and reports it as added;
`seed_dir` (the baseline writer) skips seed files.

The seed values are kept apart from theme structure. The app-facing `Theme.<Name>` lives only in
`values/themes.xml` with `parent="Base.Theme.<Name>"`; the framework's light, dark and Android 12+
variants define only `Base.Theme.<Name>`, in `mobiler_themes.xml` files of their own. So an app's items
on its theme apply in every configuration, an app that keeps its old `themes.xml` is not overridden by a
variant (Android does not merge a style across resource qualifiers), and no template file collides with
an app's own `values-night/themes.xml` (`existing_night_theme_is_not_overwritten_even_with_apply`,
`only_the_base_theme_file_defines_the_app_theme`; both failed against the first layout, where each
variant redefined `Theme.<Name>`).

**Mutation proof:**
- Removing the `SEED_PATHS` check from `classify` failed `seed_paths_classify_as_seed_and_exist_in_the_template`,
  `missing_seed_files_are_created_and_existing_ones_never_touched` and
  `seed_files_created_even_when_theme_is_offered_as_new`.
- Writing the seed file even when it exists failed `missing_seed_files_are_created_and_existing_ones_never_touched`:
  "seed file left byte-identical".
- Before the fix, a dangling symlink at a seed path made the upgrade create the link's target outside
  the project: `seed_write_does_not_follow_a_symlink` failed ("the seed write followed a symlink out of
  the project").
- Reverting each mutation restored green (96/96).

## 5. Consequences (Positive and Negative Predictions)

- **Positive:** the next release's `[splash]` sync can own these files with no merge machinery, and
  apps can hand-edit them meanwhile.
- **Negative:** a template change to a seed file's *default* never reaches existing apps; only new
  apps get it. A seed file must therefore hold values, not structure the shell depends on.
- **Negative:** like every file a new version adds, seed files are created by a plain `mobiler upgrade`
  too, not only with `--apply`.
- **Negative:** deleting a seed file makes the next upgrade restore the default, which may surprise an
  app that removed it on purpose (its themes reference it, so removal breaks the build anyway).
- **Negative:** the list is explicit; a new seed file needs a code change, and the template-shipped
  check only catches a listed path that the template lacks, not a template file that should be seed but
  isn't listed.
