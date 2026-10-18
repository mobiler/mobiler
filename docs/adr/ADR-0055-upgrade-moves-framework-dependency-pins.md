# ADR-0055: `mobiler upgrade` edits an app's manifests only to move or add the dependency lines the framework owns — each a single line in a known form, anchored to a line the template wrote, with its target read from the CLI's templates — and reports any framework pin it cannot place instead of guessing

Status:        Accepted
Date decided:  2026-10-18
Deciding PRs:  #299
Supersedes:    none
Code anchor:   mobiler/src/upgrade.rs (move_framework_pins, set_pin, migrate_codegen_feature, add_codegen_deps, foreign_pins, FACET_PIN)
Conformance:   xtask/tests/adr_conformance.rs::adr_0055_template_crux_pin_matches_the_workspace, mobiler/src/upgrade.rs::set_pin_moves_a_string_pin_and_is_a_no_op_at_target, mobiler/src/upgrade.rs::set_pin_reports_a_table_form_pin, mobiler/src/upgrade.rs::codegen_feature_migrates_the_multiline_form_once, mobiler/src/upgrade.rs::codegen_feature_on_one_line_is_reported, mobiler/src/upgrade.rs::codegen_deps_are_added_after_uniffi_once, mobiler/src/upgrade.rs::crlf_manifests_keep_crlf, mobiler/src/upgrade.rs::upgrade_moves_a_069_app_and_rerun_is_a_no_op, mobiler/src/upgrade.rs::review_run_lists_manifest_edits_and_writes_nothing, mobiler/src/upgrade.rs::other_crates_pins_are_reported_not_edited, mobiler/src/upgrade.rs::equivalent_pins_are_not_reported, mobiler/src/upgrade.rs::path_and_git_dependencies_are_left_alone

## 1. Context (The Problem)

`Cargo.toml` files are app-owned (`Class::Own`): upgrade never merges them. Its dependency step
rewrote exactly one line, `mobiler-core = "…"` in `shared/Cargo.toml`. mobiler-core 0.47 needs crux
0.20, and ADR-0054 changes the `codegen` feature and adds two dependencies. An app upgraded with only
the core pin moved would keep `crux_core = "0.18.0"`, build two crux copies, and fail. Appointments,
the app this release is for, would hit exactly that. `[recorded: spec §3]`

## 2. Hypothesis

If upgrade makes a short, fixed list of single-line edits (each found by an exact pattern the
template wrote, each target read from the embedded templates), then an app on the template layout
moves forward with no hand edits, re-running is a no-op, and nothing else in its manifests changes.
An app whose manifests differ is told what to change, never given a guessed edit.

### 2.1. Refutation Conditions

- **Condition 1 — the four edits.**
  `crux_core = "…"` (root), `mobiler-core = "…"`, the `"crux_core/cli",` feature line, and the
  `cargo_metadata` / `camino` lines after `uniffi = { …, optional = true }` (shared). These move a
  0.69 app's manifests to exactly the template's, and a second run changes nothing.
  - **Validation Metric:** `upgrade.rs::upgrade_moves_a_069_app_and_rerun_is_a_no_op` and the
    per-edit unit tests.
- **Condition 2 — unknown forms are reported.**
  A table-form pin or a one-line feature array gets a hand-edit note and is left alone.
  - **Validation Metric:** `set_pin_reports_a_table_form_pin`, `codegen_feature_on_one_line_is_reported`.
- **Condition 3 — other crates are not edited.**
  A `crux_core` / `mobiler-core` / `mobiler-web` pin in any other app manifest, or any `facet`
  dependency off the framework's version, is reported and left alone.
  - **Validation Metric:** `other_crates_pins_are_reported_not_edited`.
- **Condition 4 — review runs write nothing** (ADR-0046), and line endings are kept.
  - **Validation Metric:** `review_run_lists_manifest_edits_and_writes_nothing`, `crlf_manifests_keep_crlf`.
- **Condition 5 — targets are the framework's own.**
  The template's crux pin equals the workspace's, and upgrade's `FACET_PIN` equals mobiler-core's
  facet pin.
  - **Validation Metric:** `xtask/tests/adr_conformance.rs::adr_0055_template_crux_pin_matches_the_workspace`.

## 3. Considered Options & Rationale for Refutation

- **Option A — keep bumping only `mobiler-core`; tell apps in the release notes** `[recorded: spec §3]`
  Every app breaks on the first build after upgrading, and the fix is three manifest edits the CLI
  already knows.
- **Option B — three-way merge the manifests like shell files** `[reconstructed]`
  Manifests are the app's own: dependencies, features and workspace members are added freely.
  Merge conflicts in `Cargo.toml` on every release would be noise, and an auto-merge could pick
  dependency versions the app chose against.
- **Option C — parse and rewrite with a TOML editor** `[reconstructed]`
  This could handle every form, but it reformats or reorders what it doesn't need to touch, and it
  makes it easy to widen what upgrade edits without anyone deciding to. Exact line patterns keep
  the edit surface listed and reviewable.
- **Option D — also edit pins in other crates (workspace members, `web/`)** `[recorded: spec §3, designer's call]`
  Their layout is the app's. A report with the target version is enough, and appointments has none.

## 4. Decision & Rationale for Corroboration

The dependency step is `move_framework_pins`: the four edits above, then `foreign_pins` for every
other manifest (skipping `target/`, `node_modules/`, `build/`, `Pods/`, dot-dirs, and symlinks). A
pin that already accepts the target under Cargo's caret rules (`0.20` for `0.20.0`, `0.47.1` for
`0.47`) is left as it is, and a `path`, `git` or `workspace` dependency is never touched or reported:
neither is the framework's pin to move. A new framework-owned line joins only by being added to this
list in the CLI, with its own test.
mobiler-web is reported against the template's mobiler-core version, because the two publish in
lockstep. Facet has no template line to read from, so its target is the `FACET_PIN` constant, held
to mobiler-core's pin by the conformance test.

**Mutation proof:** each mutation was reverted afterwards, and each failed
`adr_0055_template_crux_pin_matches_the_workspace` with the message shown:
- (a) setting `Cargo.toml.tmpl` to `crux_core = "0.18.0"` gave "ADR-0055: the template pins crux_core
  0.18.0, the workspace builds with 0.20.0";
- (b) setting `FACET_PIN` to `=0.46.4` gave "ADR-0055: upgrade tells apps facet =0.46.4, mobiler-core
  builds with =0.46.5".

The `upgrade.rs` tests were seen to fail under these mutations, each reverted after:
- (c) `set_pin` returning `Unchanged` where it reported an unknown form failed
  `set_pin_reports_a_table_form_pin` ("expected a note");
- (d) `add_codegen_deps` ending its inserted lines with `"\n"` instead of the file's ending failed
  `crlf_manifests_keep_crlf` (the output showed bare `\n` after `cargo_metadata` and `camino`);
- (e) writing the manifests on a review run failed `review_run_lists_manifest_edits_and_writes_nothing`
  (`deps_would` empty);
- (f) scanning `target/` failed `other_crates_pins_are_reported_not_edited` (a fourth note for
  `target/x/Cargo.toml`);
- (g) comparing versions as strings instead of with `compatible` failed `equivalent_pins_are_not_reported`
  ("crux_core 0.20 -> 0.20.0", "mobiler-web 0.47.1 -> 0.47");
- (h) letting `migrate_codegen_feature` rewrite an already-migrated `"uniffi/bindgen",` line failed
  `codegen_feature_migrates_the_multiline_form_once`.

## 5. Consequences (Positive and Negative Predictions)

- Positive: an app on the template layout (appointments included) upgrades across a crux bump with
  no hand edits. `mobiler upgrade` without `--apply` lists exactly which lines would change.
- Positive: the manifest edit surface is a short list in one function, and anything outside it stays
  the app's.
- Negative: any manifest that strays from the template's line forms (a table-form pin, a one-line
  feature array, a renamed dependency) falls back to a note, and the user edits it by hand.
- Negative: `FACET_PIN` is a constant that must move with each crux bump. The conformance test
  catches drift from mobiler-core, but only in this repo.
- Negative: the mobiler-web target assumes lockstep versions with mobiler-core. If the two ever
  diverge, the note suggests the wrong version until this is revisited.
