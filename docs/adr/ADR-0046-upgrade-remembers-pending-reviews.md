# ADR-0046: `mobiler upgrade` remembers every file it leaves for review, treats one the user has since changed (with no conflict markers left) as resolved and advances its baseline, names everything still pending on every run, and without `--apply` changes no file a build reads; ADR-0043's merge and side-file rules otherwise stand

Status:        Accepted
Date decided:  2026-10-03
Deciding PRs:  #280
Supersedes:    ADR-0043
Code anchor:   mobiler/src/upgrade.rs (sync_file, read_pending, write_pending, clear_pending, fingerprint, has_conflict_markers, list_pending, Report::pending_lines, bump_core_dep), plus ADR-0043's anchors (classify, three_way, two_way, merge_anchors, seed_baseline, write_review, review_rel, write_backup), mobiler/src/new.rs (seeds the baseline)
Conformance:   mobiler/src/upgrade.rs::resolved_conflict_stays_resolved, mobiler/src/upgrade.rs::unresolved_conflict_is_reported_on_every_run, mobiler/src/upgrade.rs::markers_left_in_file_stay_pending, mobiler/src/upgrade.rs::review_run_never_touches_build_inputs, mobiler/src/upgrade.rs::review_offer_survives_an_unrelated_edit, mobiler/src/upgrade.rs::conflict_with_an_edit_elsewhere_stays_pending, mobiler/src/upgrade.rs::clean_offer_with_an_adjacent_edit_becomes_a_conflict, mobiler/src/upgrade.rs::orphan_record_is_dropped, mobiler/src/upgrade.rs::three_way_applies_framework_change_preserves_edit_and_flags_conflict, mobiler/src/upgrade.rs::new_seeds_baseline_so_upgrade_is_idempotent, mobiler/src/upgrade.rs::bumps_dep_stamps_and_leaves_app_code_untouched, mobiler/src/upgrade.rs::review_copies_of_android_resources_go_outside_res

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

If upgrade records what it offered for each file it leaves for review, it can tell later whether the user dealt with
it. If it judges that by the app's file, not by the gitignored review copy, then:

- a resolved conflict is resolved once and never raised again;
- an ignored one is still never dropped silently, and is named on every run;
- a run without `--apply` leaves a buildable app.

The record is `.mobiler/pending/<file>.json`, committed with the app. It holds the offered template (the baseline once
resolved), a fingerprint of the app's file when the review was offered, and the kind: `conflict`, `merge`, `review`
or `new`. On each run, before merging a file with a record:

- **Changed since offered, no conflict marker lines left, and the user dealt with the offer itself:** the file counts
  as resolved. The recorded template becomes its baseline, the record is deleted, the report says `✓ resolved`, and
  the normal merge then runs. "Dealt with the offer" means one of:
  - the file equals the review copy as offered (the record keeps its fingerprint);
  - for a conflict, merging the offer onto the file still conflicts, but in different lines than the recorded
    conflict blocks (the record keeps their fingerprint). The user changed the conflict itself.
  - Without a baseline, a `new` or `merge` record resolves on any change.
- **Changed for another reason** (`plugin add`, a `git pull`, a formatter): not resolved. The normal merge runs
  against the old baseline. It applies the offer if it still merges cleanly, or reports a real conflict, such as an
  edit right next to the offered change. Either way, the offer is never dropped.
  - Marker lines are `<<<<<<< `, `||||||| `, `=======` and `>>>>>>> `. The fingerprint treats CRLF as LF, so a Windows
    checkout is not a change.
- **Unchanged:** the merge runs again against the old baseline. The review copy and the record are rewritten, which
  also recreates the review copy on a fresh clone.
- **Changed but with marker lines:** the file stays pending and is never touched.
- **Deleted:** it is offered again as a new file. It is not counted as resolved.

A record for a path the template no longer produces is deleted and reported as `dropped`, since nothing is left to
review. That covers a dropped file, a renamed package, or a file that became the app's own.

Every run ends with a warning listing each pending file and what to do with it. "Up to date" is printed only when
nothing is pending. Without `--apply`:
- a new template file is offered as a review copy (kind `new`), not written in place;
- the `mobiler-core` bump is reported (`would bump`), not made.

Everything else is ADR-0043's: the three-way merge, conflicts never applied, the app's own code never touched, review
copies of Android resources under `.mobiler/new/`, backups under `.mobiler/backup/`, and Seed files per ADR-0042.

### 2.1. Refutation Conditions

- **Condition 1: a resolved file stays resolved.**
  - **Validation Metric:** `resolved_conflict_stays_resolved` (a second run reports `✓ resolved` and no conflict,
    the baseline equals the template, and a third run is quiet).
- **Condition 2: pending files are named on every run, including a fresh clone without the review copy.**
  - **Validation Metric:** `unresolved_conflict_is_reported_on_every_run`.
- **Condition 3: conflict markers keep a file pending and untouched.**
  - **Validation Metric:** `markers_left_in_file_stay_pending`.
- **Condition 4: a run without `--apply` writes no build input.**
  - **Validation Metric:** `review_run_never_touches_build_inputs`.
- **Condition 5: an edit that doesn't deal with the offer never drops it.** That covers an edit elsewhere in the
  file, an edit next to a clean offer, and an edit to a file with a pending conflict that leaves the conflict alone.
  - **Validation Metric:** `review_offer_survives_an_unrelated_edit`, `clean_offer_with_an_adjacent_edit_becomes_a_conflict`,
    `conflict_with_an_edit_elsewhere_stays_pending`.
- **Condition 6: a record the template no longer produces doesn't warn forever.**
  - **Validation Metric:** `orphan_record_is_dropped`.
- **Condition 7: ADR-0043's merge and side-file rules hold.**
  - **Validation Metric:** `three_way_applies_framework_change_preserves_edit_and_flags_conflict`,
    `new_seeds_baseline_so_upgrade_is_idempotent`, `bumps_dep_stamps_and_leaves_app_code_untouched` (now with
    `--apply`, where the bump and the added file happen), `review_copies_of_android_resources_go_outside_res`.

## 3. Considered Options & Rationale for Refutation

- **Option A: advance the baseline even on a conflict** `[reconstructed]`
  Rejected. An ignored conflict would then look like the user deliberately kept the old code, and the framework change
  would be dropped silently: the risk ADR-0012 and ADR-0043 guard against.
- **Option B: "the review copy is gone" means resolved** `[recorded: design discussion, 2026-10-03]`
  Rejected. Review copies are gitignored, so on a teammate's clone every pending file would look resolved.
- **Option C: an explicit resolve command (`mobiler upgrade --resolved <file>`)** `[reconstructed]`
  Rejected. The report already tells the user to edit the file. A second step they must remember would bring the
  recurring conflict back for anyone who forgets it.
- **Option D: a committed pending record, resolution judged by the app's file** `[recorded: docs/superpowers/specs/2026-10-03-upgrade-pending-reviews-design.md, approved 2026-10-03]`
  Chosen.

## 4. Decision & Rationale for Corroboration

Option D. The fingerprint is FNV-1a 64-bit. It only tells "changed since offered" apart and is not a security check;
unlike `DefaultHasher`, it is the same on every machine and Rust version. A record that can't be read counts as no
record, and the next run writes a valid one. The version stamp still advances on every run (nothing reads it to decide
anything) and shows the pending count. The template `.gitignore` is unchanged: it already doesn't ignore
`.mobiler/pending/`.

**Mutation proof:**
- Skipping the baseline advance and record clean-up when a pending file has changed failed
  `resolved_conflict_stays_resolved`.
- Making `pending_lines` always return nothing failed `unresolved_conflict_is_reported_on_every_run`.
- Dropping the early return for a file with conflict markers failed `markers_left_in_file_stay_pending`.
- Writing new files in place without `--apply` failed `review_run_never_touches_build_inputs` (and
  `accepted_new_file_is_resolved`).
- Treating every changed file as resolved, without the re-merge check, failed `review_offer_survives_an_unrelated_edit`.
- Treating any still-conflicting edit as a resolution (ignoring the recorded conflict blocks) failed
  `conflict_with_an_edit_elsewhere_stays_pending`.
- Treating any change to a clean offer as a resolution failed `clean_offer_with_an_adjacent_edit_becomes_a_conflict`
  and `review_offer_survives_an_unrelated_edit`.
- Never dropping records for unproduced paths failed `orphan_record_is_dropped`.
- Reverting restored green (112/112). The four tests carried over from ADR-0043 are not re-proven here.

## 5. Consequences (Positive and Negative Predictions)

- **Positive:** a conflict is resolved once. Upgrading again later, or with a newer CLI, merges only newer template
  changes.
- **Positive:** pending reviews are visible on every run and on every clone.
- **Positive:** "upgrade, build, then review" works again without `--apply`.
- **Negative:** an edit inside a pending conflict's own lines counts as resolving it, even when made for another
  reason, and the framework change there is dropped. The report names the file (`✓ resolved`), so it is visible but
  not prevented. Edits elsewhere never resolve anything.
- **Negative:** keeping only your own side of a conflict leaves the file unchanged, so it is offered again on every
  run. Delete its record to stop that.
- **Negative:** an edit right next to a pending clean offer turns it into a conflict to resolve by hand.
- **Negative:** a pending review is abandoned by deleting its record (`.mobiler/pending/<file>.json`). No command
  does it.
- **Negative:** a review run (no `--apply`) no longer bumps `mobiler-core` or adds new files. A user who relied on a
  plain run doing that must now use `--apply` or move the review copies in.
- **Negative:** `.mobiler/pending/` adds committed files, each holding a copy of a template file, until resolved.
- All of ADR-0043's consequences still hold, apart from "an upgraded app builds whatever was left for review", which
  this ADR now makes true for new files and the core bump as well.
