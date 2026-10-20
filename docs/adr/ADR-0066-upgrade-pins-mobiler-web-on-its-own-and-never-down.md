# ADR-0066: `mobiler upgrade` moves an app's framework dependency lines as ADR-0055 decided, but checks a web client's `mobiler-web` against mobiler-web's own version (a constant the CLI carries) instead of mobiler-core's, and never moves or reports a pin that is already at or above its target

Status:        Accepted
Date decided:  2026-10-20
Deciding PRs:  #309
Supersedes:    ADR-0055
Code anchor:   mobiler/src/upgrade.rs (move_framework_pins, set_pin, at_or_above, migrate_codegen_feature, add_codegen_deps, foreign_pins, pin_notes, FACET_PIN, MOBILER_WEB_PIN), .claude/skills/release-libs/SKILL.md (moving MOBILER_WEB_PIN)
Conformance:   xtask/tests/adr_conformance.rs::adr_0066_upgrade_points_mobiler_web_at_its_own_version, xtask/tests/adr_conformance.rs::adr_0055_template_crux_pin_matches_the_workspace, mobiler/src/upgrade.rs::mobiler_web_is_reported_against_its_own_version, mobiler/src/upgrade.rs::a_pin_above_the_target_is_never_moved_down, mobiler/src/upgrade.rs::set_pin_moves_a_string_pin_and_is_a_no_op_at_target, mobiler/src/upgrade.rs::set_pin_reports_a_table_form_pin, mobiler/src/upgrade.rs::codegen_feature_migrates_the_multiline_form_once, mobiler/src/upgrade.rs::codegen_feature_on_one_line_is_reported, mobiler/src/upgrade.rs::codegen_deps_are_added_after_uniffi_once, mobiler/src/upgrade.rs::crlf_manifests_keep_crlf, mobiler/src/upgrade.rs::upgrade_moves_a_069_app_and_rerun_is_a_no_op, mobiler/src/upgrade.rs::review_run_lists_manifest_edits_and_writes_nothing, mobiler/src/upgrade.rs::other_crates_pins_are_reported_not_edited, mobiler/src/upgrade.rs::equivalent_pins_are_not_reported, mobiler/src/upgrade.rs::path_and_git_dependencies_are_left_alone

## 1. Context (The Problem)

ADR-0055 decided how `mobiler upgrade` edits an app's manifests. It moves four framework-owned lines in the template's
own manifests (`crux_core` in the root `Cargo.toml`; `mobiler-core`, the `codegen` feature and its two dependencies
in `shared/Cargo.toml`). It reports, never edits, a framework pin in any other app manifest. It reported `mobiler-web`
against the template's mobiler-core version, "because the two publish in lockstep". Its §5 predicted the risk: "If the
two ever diverge, the note suggests the wrong version until this is revisited."

They diverged with mobiler-web 0.49.0 (#307), a web-only release; mobiler-core stayed 0.48. From CLI 0.73.0, every
`mobiler upgrade` in an app with a web client on `mobiler-web = "0.49"` printed `web/Cargo.toml: mobiler-web 0.49 ->
0.48 (edit by hand)`, a downgrade the user would be told to make on every run. Found by the site review, 2026-10-20.
`[recorded: the coordinator's report, 2026-10-20]`

Two separate faults:
- the target was wrong (the lockstep assumption);
- any target can be behind an app that took a newer library by hand, and "not compatible" was read as "must
  move to the target", including down.

## 2. Hypothesis

If mobiler-web's target is its own version, which the CLI carries as `MOBILER_WEB_PIN` (no template pins mobiler-web,
so there is no template line to read it from), and that constant is held to `mobiler-web/Cargo.toml` by a conformance
test, then the note names the version mobiler-web actually is. If a pin at or above its target is never moved or
reported, then upgrade only ever moves an app forward.

Everything else ADR-0055 decided stands unchanged: the four edits, exact line patterns, notes for unknown forms and
other crates, review runs that write nothing, kept line endings, `FACET_PIN` held to mobiler-core's facet, `path` /
`git` / `workspace` dependencies left alone, and ADR-0056's crates outside the app read and reported.

### 2.1. Refutation Conditions

- **Condition 1: mobiler-web is checked against its own version.**
  A web client on `MOBILER_WEB_PIN` gets no note; an older one is told `-> MOBILER_WEB_PIN`.
  - **Validation Metric:** `upgrade.rs::mobiler_web_is_reported_against_its_own_version`.
- **Condition 2: the constant is mobiler-web's version.**
  `MOBILER_WEB_PIN` equals the major.minor of `mobiler-web/Cargo.toml`.
  - **Validation Metric:** `xtask/tests/adr_conformance.rs::adr_0066_upgrade_points_mobiler_web_at_its_own_version`.
- **Condition 3: never down.**
  A pin at or above the target, compared part by part (`0.49` over `0.48`, `1.0` over `0.48`), is neither edited by
  `set_pin` nor reported by `pin_notes`. A lower pin, or a range that isn't a plain version (`>=0.1`), still is.
  - **Validation Metric:** `upgrade.rs::a_pin_above_the_target_is_never_moved_down`.
- **Conditions 1 to 5 of ADR-0055** (the four edits, unknown forms reported, other crates not edited, review runs write
  nothing, targets are the framework's own) hold as they were stated there, with the same tests.

## 3. Considered Options & Rationale for Refutation

- **Option A: keep the lockstep rule and always publish mobiler-web with mobiler-core** `[reconstructed]`
  Rejected. It would force a mobiler-core release, and a core bump for every app, for every web-only change, as #307
  was.
- **Option B: read mobiler-web's version from the repo at build time** `[recorded: the coordinator's report, 2026-10-20]`
  Rejected. The published CLI crate is packaged without `../mobiler-web`, so a build-time read works in this repo and
  fails on crates.io. A constant plus a conformance test, as `FACET_PIN` already does, gives the same guarantee.
- **Option C: stop reporting mobiler-web** `[reconstructed]`
  Rejected. An app on an old mobiler-web against new shells is the drift the report exists to catch.
- **Option D: only fix the target, keep reporting pins above it** `[reconstructed]`
  Rejected. Any target can be behind an app (a library published before the CLI that knows it), and a downgrade note
  is never what upgrade means.

## 4. Decision & Rationale for Corroboration

`foreign_pins` targets `("mobiler-web", MOBILER_WEB_PIN)`. `MOBILER_WEB_PIN` sits next to `FACET_PIN` in `upgrade.rs`.
release-libs moves it whenever mobiler-web's minor moves, and the conformance test fails until it does. `at_or_above`
compares plain versions part by part, missing parts as 0. `set_pin` leaves a pin alone and `pin_notes` stays silent when
the pin is compatible with the target or at or above it. The rest is ADR-0055's mechanism, unchanged.

**Mutation proof:** each mutation was reverted afterwards.
- (a) `MOBILER_WEB_PIN` set to `"0.48"` failed `adr_0066_upgrade_points_mobiler_web_at_its_own_version` ("upgrade
  tells web clients mobiler-web 0.48, mobiler-web is 0.49.0: move MOBILER_WEB_PIN").
- (b) mobiler-web's target set back to the template's mobiler-core pin failed
  `mobiler_web_is_reported_against_its_own_version`.
- (c) `set_pin` without the `at_or_above` check failed `a_pin_above_the_target_is_never_moved_down`.
- (d) `pin_notes` without the `at_or_above` check failed the same test.

Both new `upgrade.rs` tests also failed before the fix was written. The ADR-0055 tests listed in `Conformance` keep the
mutation proofs recorded in ADR-0055 §4. `equivalent_pins_are_not_reported` now gives its web manifest a mobiler-web
pin from `MOBILER_WEB_PIN`, where it used mobiler-core's version.

## 5. Consequences (Positive and Negative Predictions)

- **Positive:** a web client on the current mobiler-web is up to date, and a web-only release no longer forces a core
  release.
- **Positive:** an app never gets a downgrade from upgrade, whichever pin is behind.
- **Negative:** `MOBILER_WEB_PIN` is one more constant a release must move. The conformance test catches drift, but
  only in this repo. A CLI released before a later mobiler-web publish keeps naming the older version until the next
  CLI release; by Condition 3 it never tells that app to go down.
- **Negative:** a pin ahead of the target is trusted. An app that pins a mistyped high version (`0.94`) gets no note,
  and cargo's own resolution error is what tells it.
