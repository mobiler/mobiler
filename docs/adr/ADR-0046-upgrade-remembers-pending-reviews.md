# ADR-0046: `mobiler upgrade` remembers every file it leaves for review and names it on every run; a review copy counts as dealt with when the file equals it, a conflict only when the user runs `mobiler upgrade --resolved <file>`, and nothing else ever drops what was offered; without `--apply` it changes no file a build reads; ADR-0043's merge and side-file rules otherwise stand

Status:        Accepted
Date decided:  2026-10-03
Deciding PRs:  #280
Supersedes:    ADR-0043
Code anchor:   mobiler/src/upgrade.rs (upgrade_with, check_resolved, check_pending, sync_file, read_pending, write_pending, clear_pending, fingerprint, has_conflict_markers, list_pending, Report::pending_lines, bump_core_dep), mobiler/src/main.rs (`upgrade --resolved`), plus ADR-0043's anchors (classify, three_way, two_way, merge_anchors, seed_baseline, write_review, review_rel, write_backup), mobiler/src/new.rs (seeds the baseline)
Conformance:   mobiler/src/upgrade.rs::resolved_conflict_stays_resolved, mobiler/src/upgrade.rs::keeping_your_own_side_resolves_with_the_flag, mobiler/src/upgrade.rs::conflict_resolved_by_hand_waits_for_resolved_flag, mobiler/src/upgrade.rs::resolved_flag_refuses_markers_and_unknown_files, mobiler/src/upgrade.rs::resolved_rejects_paths_outside_the_app, mobiler/src/upgrade.rs::one_bad_resolved_argument_applies_none, mobiler/src/upgrade.rs::resolved_refuses_a_missing_file, mobiler/src/upgrade.rs::repeated_resolved_arguments_settle_once, mobiler/src/upgrade.rs::a_stub_where_a_new_file_was_offered_is_not_settled, mobiler/src/upgrade.rs::unresolved_conflict_is_reported_on_every_run, mobiler/src/upgrade.rs::markers_left_in_file_stay_pending, mobiler/src/upgrade.rs::review_run_never_touches_build_inputs, mobiler/src/upgrade.rs::review_offer_survives_an_unrelated_edit, mobiler/src/upgrade.rs::conflict_with_an_edit_elsewhere_stays_pending, mobiler/src/upgrade.rs::clean_offer_with_an_adjacent_edit_becomes_a_conflict, mobiler/src/upgrade.rs::edit_next_to_a_pending_conflict_stays_pending, mobiler/src/upgrade.rs::edit_next_to_a_clean_hunk_in_a_conflicted_file_stays_pending, mobiler/src/upgrade.rs::orphan_record_is_dropped, mobiler/src/upgrade.rs::three_way_applies_framework_change_preserves_edit_and_flags_conflict, mobiler/src/upgrade.rs::new_seeds_baseline_so_upgrade_is_idempotent, mobiler/src/upgrade.rs::bumps_dep_stamps_and_leaves_app_code_untouched, mobiler/src/upgrade.rs::review_copies_of_android_resources_go_outside_res

## 1. Context (The Problem)

ADR-0043 advances a file's baseline (`.mobiler/base/<file>`) only when the file incorporates the new template. A
conflict, or a clean merge left as a review copy without `--apply`, keeps the old baseline. That stops an ignored
review from silently dropping a framework change. The CLI 0.64.0 post-release smoke test (2026-10-03) showed the price:

- **A resolved conflict recurs.** A 0.63 app with a plugin got a `Core.kt` conflict, because `plugin add` lines sit
  next to the photo and camera lines 0.64 changed. After it was resolved as the report instructs, every later upgrade
  merged `old baseline → resolved file → template` and raised the same conflict again. Nothing could mark it resolved.
- **Nothing reminds the user.** A review copy left by an earlier run was not mentioned later. Review copies are
  gitignored, so a teammate's clone never sees them. The maintainer asked (2026-10-03) that every run warn while a
  conflict needs resolving.
- **ADR-0043's hypothesis, "an app builds after `mobiler upgrade`, with or without `--apply`", did not hold.**
  - A plain run wrote new template files straight in: `PhotoPipeline.kt`/`.swift`, which need `Photo` from a
    `codegen.rs` change that only waited as a review copy.
  - A plain run also bumped `mobiler-core` in `shared/Cargo.toml` while the shells stayed old.

## 2. Hypothesis

Suppose upgrade records what it offered for each file it leaves for review. Suppose it settles a file only on an
unambiguous signal: the file equals the review copy as offered, or the user names a conflict with `--resolved`. Then:

- a resolved conflict is resolved once and never raised again, whichever side the user kept;
- no edit the user makes for another reason drops an offered change;
- everything still pending is named on every run;
- a run without `--apply` leaves a buildable app.

**The record** is `.mobiler/pending/<file>.json`, committed with the app. It holds:
- the offered template, which becomes the baseline once settled;
- a fingerprint of the app's file when offered, and of the review copy as offered;
- the kind: `conflict`, `merge`, `review` or `new`.

**On each run, before merging a file with a record:**

- **Changed since offered, no conflict marker lines, and equal to the review copy as offered:** it counts as settled.
  The recorded template becomes its baseline, the record is deleted, and the report says `✓ resolved`.
- **Any other change:** the normal merge runs against the old baseline. That covers `plugin add`, a `git pull`, a
  formatter, a stub of the user's own where a new file was offered, and a hand-resolved conflict. The merge applies
  the offer if it merges cleanly (the file then incorporates it), or offers it again. Nothing is dropped.
- **Unchanged:** the merge runs again. The review copy and the record are rewritten, which also recreates the review
  copy on a fresh clone.
- **Marker lines in the file:** the file stays pending and is never touched. Marker lines are `<<<<<<< `,
  `||||||| `, `=======` and `>>>>>>> `.
- **Deleted:** it is offered again as a new file.

**A conflict is settled only by `mobiler upgrade --resolved <file>`**. The same command also declines an offered change
to a file the user has, keeping their version. An offered new file that isn't wanted needs the user's own version
in place first, since the offer is never dropped.
- It is repeatable and also accepts the review copy's path.
- It refuses, before settling anything:
  - a path outside the app (`..`, or an absolute path elsewhere);
  - a file that doesn't exist;
  - a file with marker lines;
  - a file with no record.
- It makes the recorded template the file's baseline, deletes the record and the review copy, and then the run
  continues.
- It works for any resolution, keeping your own side included.

The warning names the command for each pending conflict. A record for a path the template no longer produces is
deleted and reported as `dropped`. The fingerprint reads CRLF as LF.

Every run ends with a warning listing each pending file and what to do with it. "Up to date" is printed only when
nothing is pending. Without `--apply`:
- a new template file is offered as a review copy (kind `new`), not written in place;
- the `mobiler-core` bump is reported (`would bump`), not made.

Everything else is ADR-0043's: the three-way merge, conflicts never applied, the app's own code never touched, review
copies of Android resources under `.mobiler/new/`, backups under `.mobiler/backup/`, and Seed files per ADR-0042.

### 2.1. Refutation Conditions

- **Condition 1: a conflict marked resolved stays resolved, whichever side was kept.**
  - **Validation Metric:** `resolved_conflict_stays_resolved`, `keeping_your_own_side_resolves_with_the_flag`.
- **Condition 2: a conflict is never inferred as resolved from the file, not even a correct resolution.** The one
  exception is the normal merge: when the file already incorporates the template, it is up to date.
  - **Validation Metric:** `conflict_resolved_by_hand_waits_for_resolved_flag`.
- **Condition 3: `--resolved` refuses before settling anything when an argument is wrong.** That covers a file with
  markers, without a pending review, missing, or outside the app.
  - **Validation Metric:** `resolved_flag_refuses_markers_and_unknown_files`, `resolved_refuses_a_missing_file`,
    `resolved_rejects_paths_outside_the_app`, `one_bad_resolved_argument_applies_none`.
- **Condition 4: pending files are named on every run, including a fresh clone without the review copy.**
  - **Validation Metric:** `unresolved_conflict_is_reported_on_every_run`.
- **Condition 5: conflict markers keep a file pending and untouched.**
  - **Validation Metric:** `markers_left_in_file_stay_pending`.
- **Condition 6: a run without `--apply` writes no build input.**
  - **Validation Metric:** `review_run_never_touches_build_inputs`.
- **Condition 7: an edit that isn't the offered review copy never drops the offer.** That includes an edit next to it,
  an edit next to a pending conflict, and a stub where a new file was offered.
  - **Validation Metric:** `review_offer_survives_an_unrelated_edit`, `clean_offer_with_an_adjacent_edit_becomes_a_conflict`,
    `conflict_with_an_edit_elsewhere_stays_pending`, `edit_next_to_a_pending_conflict_stays_pending`,
    `edit_next_to_a_clean_hunk_in_a_conflicted_file_stays_pending`, `a_stub_where_a_new_file_was_offered_is_not_settled`.
- **Condition 8: a record the template no longer produces doesn't warn forever.**
  - **Validation Metric:** `orphan_record_is_dropped`.
- **Condition 9: ADR-0043's merge and side-file rules hold.**
  - **Validation Metric:** `three_way_applies_framework_change_preserves_edit_and_flags_conflict`,
    `new_seeds_baseline_so_upgrade_is_idempotent`, `bumps_dep_stamps_and_leaves_app_code_untouched` (now with
    `--apply`, where the bump and the added file happen), `review_copies_of_android_resources_go_outside_res`.

## 3. Considered Options & Rationale for Refutation

- **Option A: advance the baseline even on a conflict** `[reconstructed]`
  Rejected. An ignored conflict would then look like the user deliberately kept the old code, and the framework change
  would be dropped silently: the risk ADR-0012 and ADR-0043 guard against.
- **Option B: "the review copy is gone" means resolved** `[recorded: design discussion, 2026-10-03]`
  Rejected. Review copies are gitignored, so on a teammate's clone every pending file would look resolved.
- **Option D: infer a conflict's resolution from the app's file** `[recorded: the approved spec, then four review rounds, 2026-10-03]`
  This was approved first and implemented. Each refinement in turn was beaten by a review: "changed since offered",
  "the offer no longer merges", "the conflict blocks changed", and "the file holds every line the template adds". Each
  time, an ordinary edit could make the file look resolved and drop the change. Examples: an edit beside the conflict,
  a line uncommented next to it, or a template line that already appears elsewhere. "Resolved, kept my own side" and
  "edited before looking" can't be told apart from the file at all, and keeping your own side looped forever.
  Rejected.
- **Option C: an explicit `mobiler upgrade --resolved <file>`** `[recorded: maintainer decision, 2026-10-03]`
  Chosen. It was first rejected as a step users might forget. The warning now names the command on every run, so
  forgetting it is visible, not silent.

## 4. Decision & Rationale for Corroboration

Option C, with records for every review and automatic settling only for the unambiguous case: the file equals the
review copy as offered.

- **The fingerprint** is FNV-1a 64-bit. It only tells "changed since offered" apart and is not a security check;
  unlike `DefaultHasher`, it is the same on every machine and Rust version.
- **A record that can't be read** counts as no record, and the next run writes a valid one.
- **The version stamp** still advances on every run (nothing reads it to decide anything) and shows the pending count.
- **The template `.gitignore`** is unchanged: it already doesn't ignore `.mobiler/pending/`.

**Mutation proof:**
- Settling any changed conflict automatically failed five tests: `conflict_resolved_by_hand_waits_for_resolved_flag`,
  `conflict_with_an_edit_elsewhere_stays_pending`, `edit_next_to_a_pending_conflict_stays_pending`,
  `edit_next_to_a_clean_hunk_in_a_conflicted_file_stays_pending` and `resolved_conflict_stays_resolved`.
- `--resolved` without advancing the baseline failed `keeping_your_own_side_resolves_with_the_flag`,
  `resolved_conflict_stays_resolved` and `conflict_resolved_by_hand_waits_for_resolved_flag`.
- `--resolved` accepting a file with markers failed `resolved_flag_refuses_markers_and_unknown_files`.
- Making `pending_lines` always return nothing failed `unresolved_conflict_is_reported_on_every_run`.
- Dropping the early return for a file with conflict markers failed `markers_left_in_file_stay_pending`.
- Writing new files in place without `--apply` failed `review_run_never_touches_build_inputs` (and
  `accepted_new_file_is_resolved`).
- Settling any changed clean offer failed `clean_offer_with_an_adjacent_edit_becomes_a_conflict` and
  `review_offer_survives_an_unrelated_edit`.
- Never dropping records for unproduced paths failed `orphan_record_is_dropped`.
- Settling a new or unsplicable file without a baseline on any change failed
  `a_stub_where_a_new_file_was_offered_is_not_settled`.
- Accepting any path component in `--resolved` failed `resolved_rejects_paths_outside_the_app`.
- Checking and settling each `--resolved` argument in turn failed `one_bad_resolved_argument_applies_none`.
- Treating a missing file as empty failed `resolved_refuses_a_missing_file`.
- Reverting restored green (122/122). The four tests carried over from ADR-0043 are not re-proven here.

## 5. Consequences (Positive and Negative Predictions)

- **Positive:** an offered framework change is never dropped by an edit. A file is settled only by the user's own
  `--resolved`, by taking the review copy as offered, or by the merge once the file already incorporates the change.
- **Positive:** a conflict is resolved once, whichever side was kept. Upgrading again later, or with a newer CLI,
  merges only newer template changes.
- **Positive:** pending reviews are visible on every run and on every clone, with the exact command to settle each
  conflict.
- **Positive:** "upgrade, build, then review" works again without `--apply`.
- **Negative:** settling a conflict takes one extra command after resolving it. Until it runs, the conflict is offered
  again on every upgrade.
- **Negative:** `--resolved` trusts the user. It records the offered template as the baseline whatever the file holds,
  so a framework change the user deleted while resolving is not offered again.
- **Negative:** a review copy taken and then edited isn't settled automatically. It goes through the merge again,
  which may report a conflict to resolve with `--resolved`.
- **Negative:** an edit right next to a pending clean offer turns it into a conflict to resolve by hand.
- **Negative:** a review run (no `--apply`) no longer bumps `mobiler-core` or adds new files. A user who relied on a
  plain run doing that must now use `--apply` or move the review copies in.
- **Negative:** `.mobiler/pending/` adds committed files, each holding a copy of a template file, until settled.
- All of ADR-0043's consequences still hold, apart from "an upgraded app builds whatever was left for review", which
  this ADR now makes true for new files and the core bump as well.
