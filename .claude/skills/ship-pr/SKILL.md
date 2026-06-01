---
name: ship-pr
description: Open a PR for the current changes and drive it to a clean squash-merge — branch, commit, push, monitor ALL CI checks, confirm mergeable fresh, merge, delete branch, sync main. Use whenever landing a change on the mobiler repo. Encodes the repo's release-hygiene rules.
---

# ship-pr — land a change as a gated PR

The standard way to merge work into `main`. Follow these steps in order; stop and report if any gate fails.

## 1. Branch
- If on `main`, create a feature branch: `git checkout -b <kebab-topic>`. Never commit directly to `main`.
- If already on a feature branch, stay on it.

## 2. Verify the changeset
- `git status -s` and confirm every change is intended. **Exclude `.claude/`** and other untracked noise — stage with `git add -u` plus explicit new paths, never a blind `git add -A` that could sweep in stray files.
- After any multi-file edit batch, grep-verify each intended change actually landed before committing (a parallel edit batch can silently no-op a file).

## 3. Commit
- Write a clear `type(scope) summary` subject + a body explaining the what/why and how it was verified.
- End the commit message with the standard `Co-Authored-By:` trailer the harness specifies for this session (use the current model's exact line — don't hardcode it here, it goes stale).

## 4. Push + open the PR
- `git push -u origin <branch>`
- `gh pr create --title ... --body ...` (body: what changed, verification, any follow-ups; end with the harness's standard PR "generated with" footer for this session).

## 5. Monitor CI — every check, every terminal state
- Get the run **after** pushing — never act on a guessed/stale run id.
- Use a Monitor that emits each check as it lands and exits when all are non-pending, e.g.:
  ```
  prev=""
  while true; do
    s=$(gh pr checks <PR#> --json name,bucket 2>/dev/null) || { sleep 30; continue; }
    cur=$(jq -r '.[] | select(.bucket!="pending") | "\(.name): \(.bucket)"' <<<"$s" | sort)
    comm -13 <(echo "$prev") <(echo "$cur"); prev=$cur
    jq -e 'all(.bucket!="pending")' <<<"$s" >/dev/null 2>&1 && { echo "=== ALL CHECKS COMPLETE ==="; break; }
    sleep 30
  done
  ```
- The native gates that actually exercise template/shell changes are: `scaffold + build (template, Android)`, the 3 `iOS build (...)` lanes, and the `Android build (...)` matrix. Wait for all of them.
- If any check is not `pass`, **stop**: read the failing job, fix, push, re-monitor. Do not merge a red PR.

## 6. Confirm mergeable — FRESH, at merge time
- `gh pr view <PR#> --json mergeable,mergeStateStatus` must be `MERGEABLE` / `CLEAN`. Re-check this immediately before merging (a stale check from earlier is not enough).

## 7. Merge + sync
- `gh pr merge <PR#> --squash --delete-branch`
- `git checkout main && git pull` and confirm the new `main` HEAD.

## Notes
- This skill does NOT publish anything. Publishing is `release-libs` / `release-cli` (separate, with their own irreversibility gates).
- A `Warning: 1 uncommitted change` from `gh pr create` is usually just the untracked `.claude/` — verify, then ignore.
