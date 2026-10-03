# `mobiler upgrade`: pending reviews are remembered, resolved once, and warned about

**Found by:** the CLI 0.64.0 post-release smoke test (2026-10-03). A 0.63 app with a plugin got a `Core.kt` conflict.
After it was resolved as instructed, every later `mobiler upgrade` raised the same conflict again.
**Asked by the maintainer (2026-10-03):** "print on terminal warning if there were conflict so that running mobiler
upgrade will inform that there were conflict that need to be resolved".
**Release:** CLI 0.64.1. No library change.
**Constrained by:**
- ADR-0012 / ADR-0043: the three-way merge against `.mobiler/base/`. A conflict is never applied, the app's own code
  is never touched, and review copies and backups never land where a build reads them.
- ADR-0042: Own / Merge / Shell / Seed classes. Seed files are created when missing and are never touched.
- ADR-0009: template code that uses a new ABI item lands after the library release.

**New record:** ADR-0046, which supersedes ADR-0043. It keeps ADR-0043's merge and side-file rules and changes
when the baseline advances and what a review run (no `--apply`) may write.

## Problem

1. **A resolved conflict recurs.**
   - `three_way` advances `.mobiler/base/<file>` only when the file incorporates the new template. A conflict, or a
     review copy left un-applied, keeps the old baseline. That is how ADR-0043 stops an ignored review from silently
     dropping a framework change.
   - Once the user resolves the conflict by hand, the next upgrade merges `old base → resolved file → template`. The
     same region differs on both sides, so it conflicts again, on every run, forever.
   - The 0.64.0 smoke test showed it: `Core.kt`, where `plugin add` lines sit next to the photo and camera lines that
     0.64 changed.
2. **Nothing reminds the user.** A review copy left from an earlier run is not mentioned by later runs unless the merge
   happens to produce it again. Review copies are gitignored, so a teammate's clone never sees them.
3. **A review run can leave an app that doesn't compile**, which breaks ADR-0043's hypothesis ("an app builds after
   `mobiler upgrade`, with or without `--apply`"):
   - New template files are written straight in. In 0.64.0, `PhotoPipeline.kt` and `.swift` need `Photo` from
     `codegen.rs`, and that `codegen.rs` change only waits as a review copy.
   - `bump_core_dep` rewrites `shared/Cargo.toml` in place while the shells stay old.

## Decisions (approved 2026-10-03)

### 1. Pending reviews are recorded

When `upgrade` leaves a managed file un-incorporated, it records the file in `.mobiler/pending/<rel>.json`. That
covers a conflict, or a clean merge offered as a review copy without `--apply`. The record is committed: the template
`.gitignore` keeps ignoring only `.mobiler/backup/`, `.mobiler/new/` and `*.mobiler-new` / `*.mobiler-bak`.

```json
{ "template": "<the new template text: the baseline once resolved>",
  "file_hash": "<fingerprint of the app's file when the review was offered>",
  "kind": "conflict" | "review" }
```

### 2. Resolution is judged by the app's file, not the review copy

On each run, before merging a file that has a pending record:

- **Resolved:** the file's fingerprint differs from `file_hash` and the file has no conflict marker lines (`<<<<<<< `,
  `=======`, `>>>>>>> ` at the start of a line).
  - The recorded `template` becomes the baseline, and the record is deleted.
  - The report lists `✓ resolved <file>`.
  - The file then goes through the normal three-way merge against that baseline. With the same CLI it is up to date;
    with a newer CLI, only newer template changes merge.
- **Still pending:** the file is unchanged since the review was offered.
  - The normal merge runs again against the old baseline. That rewrites the review copy, recreating it on a fresh
    clone, and refreshes the record.
- **Markers left in the file:** the file has changed but still contains marker lines.
  - It stays pending and is reported as `conflict markers left in <file>`. The file itself is never touched.

**Cost (accepted):** an unrelated edit to a pending file, without resolving it, counts as resolved, and the framework
change in that region is dropped. The report names the file (`✓ resolved`), so this never happens silently.

### 3. Every run ends with a warning while anything is pending

After the per-file lines, if any record remains, every `mobiler upgrade` prints this. It does so even on a run that
otherwise changes nothing, with or without `--apply`:

```
⚠ 1 file still needs your review from an earlier upgrade:
    Android/app/src/main/java/dev/test/old/Core.kt   conflict — resolve Core.kt.mobiler-new, then replace the file
  Run `mobiler upgrade` again after resolving; it will confirm.
```

- The review-copy path comes from `review_rel`. The printed "Up to date. ✓" is replaced by this block when anything is
  pending.
- The stamp line adds `(<n> file(s) pending)`. The stamp still advances: nothing reads it to decide anything.

### 4. A review run never changes a file a build reads

Without `--apply`:

- **New Shell/Merge-class file:** written as a review copy at `review_rel(rel)`, recorded as pending (`kind: review`)
  and listed as `+ new (review)`. It is not written in place.
- **The `mobiler-core` bump:** reported as `deps: would bump mobiler-core 0.42 -> 0.43 (run with --apply)`. Cargo.toml
  is left alone.

With `--apply`, both behave as today. Seed files keep ADR-0042's rule (created when missing), and Own files stay
untouched.

A new file accepted by hand (the review copy moved into place) is resolved by rule 2. A pending record for a file that
didn't exist has `file_hash` = the fingerprint of empty bytes, so "the file now exists" counts as changed.

## Testing

Unit tests in `mobiler/src/upgrade.rs`, using the existing fixture helpers:

- **`resolved_conflict_stays_resolved`:** conflict, then the user resolves it (markers removed, both sides kept), then
  a second upgrade.
  - Expected: no conflict; `✓ resolved`; the baseline equals the template; no record left.
  - A third upgrade is a no-op.
- **`unresolved_conflict_is_reported_on_every_run`:** conflict, then a second upgrade with nothing changed.
  - Expected: still pending, the warning is listed, and the review copy is recreated after being deleted.
- **`markers_left_in_file_stay_pending`.**
- **`review_run_never_touches_build_inputs`:** a new template file plus a core bump, without `--apply`.
  - Expected: the file is absent; the review copy is present; Cargo.toml is unchanged; a pending record exists.
  - With `--apply`, both land and nothing stays pending.
- **Existing tests stay green.** The added-file and dep-bump tests are adjusted to use `--apply` where they expected
  in-place writes.

Each new conformance test is proven by a mutation recorded in ADR-0046 §4:

- never using the pending template as the baseline (fails the first test);
- skipping the warning (fails the second);
- writing new files in review mode (fails the fourth).

**Runtime:** reproduce the 0.64.0 smoke scenario with the workspace CLI.
- Scaffold with 0.63.0, `plugin add battery`, `upgrade --apply`, then resolve `Core.kt`.
- A second `upgrade --apply` must print `✓ resolved` and no conflict, and the app must build an APK.
- Also run a plain `mobiler upgrade` on a 0.63 app: it must build unchanged.

## READMEs

The root `README.md` "Upgrading an app" section and `mobiler/README.md`'s upgrade section, around lines 82–100, gain
three things:
- the pending record (commit `.mobiler/pending/`);
- the end-of-run warning;
- the rule that a review run never changes a file the build reads.

## Amendments

- **2026-10-03 (plan):**
  - The record's fingerprint is FNV-1a 64-bit (`file_hash`), not SHA-256. It only tells "changed since offered" apart,
    is not a security check, and the CLI has no SHA dependency. It is stable across Rust versions, unlike
    `DefaultHasher`.
  - The template `.gitignore` is left unchanged. It already doesn't ignore `.mobiler/pending/`, and editing it would put
    a review copy into every app.
- **2026-10-03 (review of the implementation):**
  - **Resolution:** a changed, marker-free file counts as resolved only when the offered template no longer merges
    cleanly onto it against the old baseline. Otherwise an unrelated edit (`plugin add`, `git pull`) would drop a clean
    offer. Without a baseline, only a `new` or `merge` record resolves this way.
  - **A deleted file:** offered again as new, not resolved.
  - **Orphan records:** a record for a path the template no longer produces is deleted and reported as `dropped`.
  - **Markers and fingerprint:** marker lines include diff3's `||||||| `, and the fingerprint treats CRLF as LF.
  - **Kinds and wording:** the kinds are `conflict`, `merge`, `review` and `new`. A new file prints as
    `+ new <file> -> <copy>`. The warning header is "⚠ N file(s) still need your review from an upgrade:".
- **2026-10-03 (second review):** the resolution rule is narrowed to "the user dealt with the offer".
  - A file counts as resolved when it equals the review copy as offered.
  - A conflict also counts as resolved when its conflict lines changed. The record fingerprints the conflict blocks.
  - Any other edit goes through the normal merge, which applies the offer or reports a real conflict. This closes
    two gaps: a conflict plus an edit elsewhere was counted as resolved, and a clean offer plus an adjacent edit
    (`plugin add` at an anchor) was counted as resolved.
- **2026-10-03 (third review):** a conflict counts as resolved only when two checks hold.
  - The file contains every run of lines the offered template adds or changes relative to the baseline.
  - The conflict blocks changed.
  - Both checks read CRLF as LF.
  - This closes the case of an edit beside a pending conflict, or beside a clean change in the same file. Before, the
    block fingerprint changed, so it counted as resolved and the template's lines were dropped.
