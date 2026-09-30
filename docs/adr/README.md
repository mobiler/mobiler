# Architecture Decision Records

Adapted from the appointments project's ADR practice. The rules are the same; the adaptations are
listed at the end.

## What an ADR is, and what it is not

**An ADR records a decision:** its context, the options rejected, and its consequences.

**ADRs are immutable from the moment a record merges.** Before that, a record is a draft, and review
may change anything in it, including the decision itself. A **merged** ADR allows only two repairs:

- its `Status:` line, including `Superseded by ADR-NNNN` when a later decision replaces it;
- a stale `Conformance:` path, when the cited test has moved or been renamed.

Neither repair changes the decision or its rationale. A decision that changes after merge gets a
**new** ADR that supersedes the old one, so the history of a decision stays readable.

**If you are tempted to edit an ADR, you are writing a new one instead.**

How something behaves *now* lives in the crate READMEs, `capabilities.json` and the specs. They are
living documents, edited with the code.

## When to write one

Write one when a choice constrains future work: someone could reasonably do it another way, and
doing it another way here would break something structural. Typical areas:

- the wire ABI between core and shells;
- compatibility promises to apps;
- capability mechanics (plugins, streams, payload encoding);
- release and publish mechanics;
- the line between the framework and one app's product rules.

Do not write one for a bug fix, a refactor that changes no contract, or a behaviour detail. Those
belong in the PR body.

**Before designing**, read `index.md` and the records your change touches, and name them in the
spec. A design that contradicts an accepted record supersedes it openly, with a new ADR in the same
PR.

## Numbering

Sequential from `0001`, no gaps, no reuse; take the next free number. File name:
`ADR-NNNN-short-slug.md`. `xtask/tests/adr_docs.rs` enforces this: with a duplicate or a hole, a
record can no longer be cited unambiguously.

## Status vocabulary

| Status | Meaning |
|---|---|
| `Accepted` | In force. |
| `Accepted (blocked)` | Decided and implemented on our side, but something external stops it taking effect. The record names the blocker. |
| `Superseded by ADR-NNNN` | Replaced. The record stays; read the successor. |
| `Deprecated` | No longer in force, and not replaced. |

## Conformance

Every ADR names, in its `Conformance:` header, the tests that would fail if the decision were
violated. There are three kinds:

- **Invariant:** asserts the property holds absolutely. These live in
  `xtask/tests/adr_conformance.rs`.
- **Behavioural:** an ordinary test that exercises the decision. Cite it by path and test name.
- **Ratchet:** asserts a counted violation never moves without a deliberate edit. Use it for a
  migration in progress, so a record can state a direction of travel honestly.

A decision with nothing to test mechanically (a process rule, say) writes
`Conformance: none — <why>`, and review enforces it.

**Format.** Each `Conformance:` entry is `path::test_name` (the lint checks that the file has a
`#[test]` fn of that name, in code, not a comment), a CI job reference
`.github/workflows/<file>.yml job "<name>" …` (the lint checks a job's `name:` line matches), or
`none — <why>`.

**Every conformance test written for an ADR must be proven to fail by a deliberate mutation before
it is trusted.** (The backfilled records also cite older behavioural tests that existed before this
directory. Those aren't re-proven; see the adaptations below.)
Record the mutation **in the record itself**, as a short `Mutation proof` note in §4, not only in
the PR that added the test. A PR lives on GitHub, not in the tree, so a reader of the repo months
later can't check it. A test that has never been seen to fail turns an open question into false
assurance, which is worse than no test. The lint that guards the ADR set as a whole
(`adr_docs.rs`) records its own proof in its module doc comment.

## Writing a record from history

Most of the first records describe decisions made before this directory existed. Code tells you
what was decided, never what was rejected. So in §3, every option carries a provenance tag:

- `[recorded: <source>]`: the rationale was written down at the time **somewhere a reader can
  check**: a tracked file (a spec or plan in `docs/superpowers/`, a README), a commit message, or a
  GitHub PR or issue body. Cite it, and quote it exactly when you quote.
- `[recorded: maintainer, YYYY-MM-DD, quoted here]`: a maintainer statement that exists nowhere
  else in the repo (the gitignored engineering notes, a chat, private memory). Quote it verbatim
  right after the tag; the record then becomes its record. Use `YYYY-MM` when only the month is
  known.
- `[reconstructed]`: inferred to be the alternative. Nobody wrote it down.

Never present an inferred alternative as recorded history. The tag tells a reader which rationale to
trust and which to check.

`Date decided:` comes from git history for the decision, not the day the record was written.

## Files

- `TEMPLATE.md`: copy this to start a record.
- `index.md`: the table of every record. Add a row when you add a record, and keep the status in
  step. The lint checks both.

## Adaptations from the appointments practice

- There is no `docs/features/` for now. The crate READMEs, `capabilities.json` (with
  `xtask gen-readme`) and the specs already describe current behaviour.
- Much of mobiler's history was written down in specs, plans, PRs and notes, so most of the
  backfilled options can be `[recorded]` rather than `[reconstructed]`.
- The lint and the invariant tests live in the unpublished `xtask` crate, so they never ship to
  crates.io.
- `Conformance: none — <why>` is allowed for a decision with nothing mechanical to test (a scoping
  or process rule); review enforces those.
- `[recorded: …]` may cite specs and plans in `docs/superpowers/` (they record rationale at the
  time). Maintainer statements that live only outside the repo are quoted in the record, tagged
  `[recorded: maintainer, date, quoted here]`.
- Pre-existing behavioural tests and CI jobs cited by the backfilled records (ADR-0001 …
  ADR-0036) are not re-proven by mutation. Every conformance test written *for* an ADR is.
