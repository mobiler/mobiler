# Upgrade pending reviews Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:**
- A conflict or review that `mobiler upgrade` leaves is resolved once and stays resolved.
- Every run warns while anything is pending.
- A run without `--apply` never changes a file the build reads.

**Architecture:**
- All changes are in `mobiler/src/upgrade.rs`.
- A pending record per un-incorporated file, `.mobiler/pending/<rel>.json`, holds the template text and a fingerprint
  of the app's file. `sync_file` checks the record first: a changed, marker-free file adopts the recorded template as
  its baseline.
- `Report` gains `resolved` / `markers` / `new_review` / `pending` / `deps_would`, and a `pending_lines()` that
  `print` uses.
- Review mode offers new files as review copies and only reports the core bump.

**Tech Stack:** Rust (the `mobiler` CLI crate), `diffy` three-way merge, `serde_json` (already a dependency). No new
dependencies.

**Spec:** `docs/superpowers/specs/2026-10-03-upgrade-pending-reviews-design.md`

## Global Constraints

- Release: CLI 0.64.1 (`mobiler/Cargo.toml`). No library change.
- **ADR-0046** supersedes ADR-0043. Only ADR-0043's `Status:` line changes, to `Superseded by ADR-0046`; its index row
  changes the same way. Merged ADRs are otherwise immutable.
- A conflict is never applied to the app's file. The app's own code (`shared/src/` except `codegen.rs`), Own files and
  Seed files keep their ADR-0042 / ADR-0043 rules.
- Review copies keep `review_rel` placement: Android resources go under `.mobiler/new/`, everything else next to the
  file.
- Pending records live in `.mobiler/pending/` and are committed. Don't change the template `.gitignore`: it already
  doesn't ignore them, and changing it would put a review copy into every app.
- Every new conformance test must be proven by a recorded mutation in ADR-0046 §4.
- Builds use `CARGO_TARGET_DIR=/media/zmilan/data2/cargo-target-mobiler`; the root disk is tight.
- Every crates.io publish and `v*` tag needs the maintainer's approval. The user runs the merge and tag `!` commands.

## Review Focus

1. **A pending file that the template no longer changes.** The template reverted, or a newer CLI dropped the change.
   The normal merge then incorporates the file, and its record must be cleared, not left warning forever.
   Covered by `incorporated_file_clears_its_record` in Task 1.
2. **A pending new file that the user moved into place by hand.** It must count as resolved, with its baseline set to
   the template. Covered by `accepted_new_file_is_resolved` in Task 3.
3. **A fresh clone.** The record is committed but the review copy (gitignored) is absent. It must stay pending and get
   its review copy back. Covered by `unresolved_conflict_is_reported_on_every_run` in Task 2.
4. **Conflict marker detection.** It must not fire on ordinary code lines that merely contain `=======`, such as
   Markdown or comments. Only whole-line markers count. Covered by `markers_are_whole_lines` in Task 1.
5. **A corrupt or hand-edited pending JSON.** It must be ignored as "no record" without failing the upgrade. Covered by
   `corrupt_record_is_ignored` in Task 1.

---

### Task 1: pending records and resolution (spec rules 1–2)

**Files:**
- Modify: `mobiler/src/upgrade.rs`. Add a `PENDING_REL` constant next to `BASE_REL` (line ~29). Add new helpers after
  `write_baseline` (line ~412). Change `sync_file` (lines ~209–286) and the `Report` struct (line ~112).
- Test: the `mod test` in the same file.

**Interfaces:**
- Produces:
  - `const PENDING_REL: &str = ".mobiler/pending";`
  - `fn pending_path(root: &Path, rel: &Path) -> PathBuf`
  - `fn fingerprint(bytes: &[u8]) -> String`: FNV-1a 64-bit as 16 hex digits.
  - `struct PendingRecord { template: String, file_hash: String, kind: String }`
  - `fn read_pending(root: &Path, rel: &Path) -> Option<PendingRecord>`
  - `fn write_pending(root: &Path, rel: &Path, template: &str, current: &[u8], kind: &str) -> Result<()>`
  - `fn clear_pending(root: &Path, rel: &Path)`
  - `fn has_conflict_markers(text: &str) -> bool`
  - `Report` fields `resolved: Vec<String>` and `markers: Vec<String>`
  - test helpers `toolchain() -> String` and `conflicted_app() -> PathBuf`
- `kind` values: `"conflict"`, `"merge"`, `"review"` and `"new"` (`"new"` comes in Task 3).

- [ ] **Step 1: Write the failing tests** (add inside `mod test`, after `read`):

```rust
    /// The shipped `rust-toolchain.toml` template (a Shell-class file with no tokens).
    fn toolchain() -> String {
        TEMPLATES.get_file("rust-toolchain.toml").unwrap().contents_utf8().unwrap().to_string()
    }

    /// An app whose `rust-toolchain.toml` conflicts with the template: the old baseline had a first
    /// line the template dropped, and the user changed that same line.
    fn conflicted_app() -> PathBuf {
        let root = skeleton();
        let t = toolchain();
        fs::create_dir_all(root.join(".mobiler/base")).unwrap();
        fs::write(root.join(".mobiler/base/rust-toolchain.toml"), format!("# old\n{t}")).unwrap();
        fs::write(root.join("rust-toolchain.toml"), format!("# mine\n{t}")).unwrap();
        root
    }

    #[test]
    fn resolved_conflict_stays_resolved() {
        let root = conflicted_app();
        let r1 = upgrade_at(&root, true).unwrap();
        assert!(r1.conflict.iter().any(|c| c == "rust-toolchain.toml"), "{:?}", r1.conflict);
        assert!(root.join(".mobiler/pending/rust-toolchain.toml.json").exists(), "the conflict is recorded");

        // The user resolves it as told: edits the file (no markers left), drops the review copy.
        let resolved = format!("# mine, resolved\n{}", toolchain());
        fs::write(root.join("rust-toolchain.toml"), &resolved).unwrap();
        fs::remove_file(root.join("rust-toolchain.toml.mobiler-new")).unwrap();

        let r2 = upgrade_at(&root, true).unwrap();
        assert_eq!(r2.resolved, ["rust-toolchain.toml"]);
        assert!(r2.conflict.is_empty(), "a resolved conflict never comes back: {:?}", r2.conflict);
        assert_eq!(read(&root, ".mobiler/base/rust-toolchain.toml"), toolchain(), "baseline advanced");
        assert!(!root.join(".mobiler/pending/rust-toolchain.toml.json").exists(), "record cleared");
        assert_eq!(read(&root, "rust-toolchain.toml"), resolved, "the user's resolution is kept");

        let r3 = upgrade_at(&root, true).unwrap();
        assert!(r3.conflict.is_empty() && r3.resolved.is_empty(), "third run is quiet");
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn markers_left_in_file_stay_pending() {
        let root = conflicted_app();
        upgrade_at(&root, true).unwrap();
        // The user copies the review copy over the file without resolving its markers.
        let review = read(&root, "rust-toolchain.toml.mobiler-new");
        fs::write(root.join("rust-toolchain.toml"), &review).unwrap();
        let r = upgrade_at(&root, true).unwrap();
        assert_eq!(r.markers, ["rust-toolchain.toml"]);
        assert!(r.resolved.is_empty());
        assert_eq!(read(&root, "rust-toolchain.toml"), review, "a file with markers is never touched");
        assert!(root.join(".mobiler/pending/rust-toolchain.toml.json").exists(), "still pending");
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn incorporated_file_clears_its_record() {
        let root = conflicted_app();
        upgrade_at(&root, true).unwrap();
        // The file now equals the template (e.g. the user took the template's side): incorporated.
        fs::write(root.join("rust-toolchain.toml"), toolchain()).unwrap();
        let r = upgrade_at(&root, true).unwrap();
        assert!(r.conflict.is_empty());
        assert!(!root.join(".mobiler/pending/rust-toolchain.toml.json").exists());
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn markers_are_whole_lines() {
        assert!(has_conflict_markers("a\n<<<<<<< ours\nb\n=======\nc\n>>>>>>> theirs\n"));
        assert!(!has_conflict_markers("// ======= section =======\nlet x = \"<<<<<<<\";\n"));
    }

    #[test]
    fn corrupt_record_is_ignored() {
        let root = conflicted_app();
        let p = root.join(".mobiler/pending/rust-toolchain.toml.json");
        fs::create_dir_all(p.parent().unwrap()).unwrap();
        fs::write(&p, "{ not json").unwrap();
        let r = upgrade_at(&root, true).unwrap(); // must not fail
        assert!(r.conflict.iter().any(|c| c == "rust-toolchain.toml"));
        assert!(read_pending(&root, Path::new("rust-toolchain.toml")).is_some(), "rewritten as a valid record");
        let _ = fs::remove_dir_all(&root);
    }
```

- [ ] **Step 2: Run them and watch them fail**

Run: `CARGO_TARGET_DIR=/media/zmilan/data2/cargo-target-mobiler cargo test -q -p mobiler upgrade 2>&1 | tail -20`

Expected: a compile error naming `resolved`, `markers`, `has_conflict_markers` and `read_pending`. Then, once they are
stubbed in Step 3, runtime failures until the logic is in.

- [ ] **Step 3: Implement.** Add the constant beside `BASE_REL`:

```rust
/// Files `upgrade` left for review (a conflict, a review copy, a new file): what was offered, so a
/// later run can tell a resolved file from an ignored one (ADR-0046). Committed with the app.
const PENDING_REL: &str = ".mobiler/pending";
```

Add these helpers after `write_baseline`:

```rust
/// Path of `rel`'s pending record: `.mobiler/pending/<rel>.json`.
fn pending_path(root: &Path, rel: &Path) -> PathBuf {
    let mut p = root.join(PENDING_REL).join(rel).into_os_string();
    p.push(".json");
    PathBuf::from(p)
}

/// A stable fingerprint of a file's bytes (FNV-1a 64): tells "changed since offered" apart. Not a
/// security hash; the same across Rust versions and machines, unlike `DefaultHasher`.
fn fingerprint(bytes: &[u8]) -> String {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in bytes {
        h ^= u64::from(*b);
        h = h.wrapping_mul(0x0000_0100_0000_01b3);
    }
    format!("{h:016x}")
}

/// What `upgrade` offered for a file it could not incorporate.
struct PendingRecord {
    /// The new template: the file's baseline once the user has resolved it.
    template: String,
    /// `fingerprint` of the app's file when it was offered (of empty bytes for a new file).
    file_hash: String,
    /// "conflict", "merge", "review" or "new".
    kind: String,
}

/// The pending record for `rel`; a missing or unreadable one is no record.
fn read_pending(root: &Path, rel: &Path) -> Option<PendingRecord> {
    let text = fs::read_to_string(pending_path(root, rel)).ok()?;
    let v: serde_json::Value = serde_json::from_str(&text).ok()?;
    Some(PendingRecord {
        template: v["template"].as_str()?.to_string(),
        file_hash: v["file_hash"].as_str()?.to_string(),
        kind: v["kind"].as_str()?.to_string(),
    })
}

fn write_pending(root: &Path, rel: &Path, template: &str, current: &[u8], kind: &str) -> Result<()> {
    let path = pending_path(root, rel);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).with_context(|| format!("creating {}", parent.display()))?;
    }
    let json = serde_json::json!({ "template": template, "file_hash": fingerprint(current), "kind": kind });
    fs::write(&path, serde_json::to_string_pretty(&json)?).with_context(|| format!("writing {}", path.display()))
}

fn clear_pending(root: &Path, rel: &Path) {
    let _ = fs::remove_file(pending_path(root, rel));
}

/// Whether `text` still holds a merge conflict: a whole line that is a marker.
fn has_conflict_markers(text: &str) -> bool {
    text.lines().any(|l| l.starts_with("<<<<<<< ") || l == "=======" || l.starts_with(">>>>>>> "))
}
```

In `Report`, after `conflict`:

```rust
    /// Files left for review earlier that the user has since resolved: their baseline advanced.
    resolved: Vec<String>,
    /// Files left for review whose conflict markers are still in the file (never touched).
    markers: Vec<String>,
```

In `sync_file`, directly after `let rel_disp = rel.to_string_lossy().to_string();`:

```rust
    // A file left for review by an earlier run: changed since then and marker-free means the user
    // resolved it, so what was offered becomes its baseline (ADR-0046). Unchanged: merge again below.
    if let Some(rec) = read_pending(root, &rel) {
        let now = fs::read(&dst).unwrap_or_default();
        if fingerprint(&now) != rec.file_hash {
            if has_conflict_markers(&String::from_utf8_lossy(&now)) {
                report.markers.push(rel_disp);
                return Ok(());
            }
            write_baseline(root, &rel, rec.template.as_bytes())?;
            clear_pending(root, &rel);
            report.resolved.push(rel_disp.clone());
        }
    }
```

At the end of `sync_file`, replace the block
`let incorporated = match … ; if incorporated { write_baseline(…)?; } Ok(())` with this:

```rust
    let (conflicts, merges) = (report.conflict.len(), report.merge.len());
    let incorporated = match read_baseline(root, &rel) {
        Some(base) => three_way(&base, &current_s, &pristine, &dst, &rel_disp, apply, report)?,
        None => two_way(class, &current, &pristine, &dst, &rel_disp, apply, report)?,
    };
    if incorporated {
        write_baseline(root, &rel, pristine.as_bytes())?;
        clear_pending(root, &rel);
    } else {
        let kind = if report.conflict.len() > conflicts {
            "conflict"
        } else if report.merge.len() > merges {
            "merge"
        } else {
            "review"
        };
        write_pending(root, &rel, &pristine, &current, kind)?;
    }
    Ok(())
```

In the `!dst.exists()` branch, after `write_baseline(root, &rel, &desired)?;`, add `clear_pending(root, &rel);`.

- [ ] **Step 4: Run the tests and watch them pass**

Run: `CARGO_TARGET_DIR=/media/zmilan/data2/cargo-target-mobiler cargo test -q -p mobiler 2>&1 | grep -E "test result|panicked|FAILED"`

Expected: all pass. If `conflicted_app` doesn't conflict in `diffy`, because a delete-vs-modify of line 1 merged
cleanly, change the template side so that both sides modify the same line. Ledger that as a Ruling.

- [ ] **Step 5: Commit**

```bash
git add mobiler/src/upgrade.rs
git commit -F <msg file>   # "fix(upgrade): a resolved conflict stays resolved (pending records)"
```

### Task 2: the warning on every run (spec rule 3)

**Files:**
- Modify: `mobiler/src/upgrade.rs`: the `Report` struct, `upgrade_at` (line ~137), `Report::print` (line ~565).
- Test: same file.

**Interfaces:**
- Consumes:
  - `PENDING_REL`, `read_pending`, `review_rel`;
  - `Report.markers` from Task 1.
- Produces:
  - `fn list_pending(root: &Path) -> Vec<(String, String)>`: (rel, kind), sorted by rel;
  - `Report.pending: Vec<(String, String)>`;
  - `fn pending_lines(&self) -> Vec<String>`: the warning block's lines, empty when nothing is pending.

- [ ] **Step 1: Write the failing test**

```rust
    #[test]
    fn unresolved_conflict_is_reported_on_every_run() {
        let root = conflicted_app();
        upgrade_at(&root, true).unwrap();
        // A fresh clone: the record is committed, the review copy (gitignored) is not.
        fs::remove_file(root.join("rust-toolchain.toml.mobiler-new")).unwrap();
        let r = upgrade_at(&root, false).unwrap();
        assert!(r.pending.contains(&("rust-toolchain.toml".to_string(), "conflict".to_string())), "{:?}", r.pending);
        let lines = r.pending_lines();
        assert!(lines[0].starts_with('⚠'), "{lines:?}");
        assert!(lines.iter().any(|l| l.contains("rust-toolchain.toml") && l.contains("conflict")), "{lines:?}");
        assert!(root.join("rust-toolchain.toml.mobiler-new").exists(), "the review copy is offered again");
        let _ = fs::remove_dir_all(&root);
    }
```

- [ ] **Step 2: Run it and watch it fail**

Run: `CARGO_TARGET_DIR=/media/zmilan/data2/cargo-target-mobiler cargo test -q -p mobiler unresolved_conflict 2>&1 | tail -5`

Expected: a compile error, because there is no `pending` field and no `pending_lines`.

- [ ] **Step 3: Implement**

Add to `Report`:

```rust
    /// Every file still waiting for the user's review after this run: (path, kind).
    pending: Vec<(String, String)>,
```

Add after `has_conflict_markers`:

```rust
/// Every pending record under `.mobiler/pending/`: (the file's path, its kind), sorted.
fn list_pending(root: &Path) -> Vec<(String, String)> {
    fn walk(dir: &Path, base: &Path, root: &Path, out: &mut Vec<(String, String)>) {
        let Ok(entries) = fs::read_dir(dir) else { return };
        for e in entries.flatten() {
            let p = e.path();
            if p.is_dir() {
                walk(&p, base, root, out);
            } else if let Some(rel) = p.strip_prefix(base).ok().and_then(|r| r.to_str()).and_then(|r| r.strip_suffix(".json")) {
                if let Some(rec) = read_pending(root, Path::new(rel)) {
                    out.push((rel.to_string(), rec.kind));
                }
            }
        }
    }
    let base = root.join(PENDING_REL);
    let mut out = Vec::new();
    walk(&base, &base, root, &mut out);
    out.sort();
    out
}
```

In `upgrade_at`, before `write_stamp`, add `report.pending = list_pending(root);`.

Add to `impl Report`:

```rust
    /// The end-of-run warning while anything waits for review (empty when nothing does).
    fn pending_lines(&self) -> Vec<String> {
        if self.pending.is_empty() {
            return Vec::new();
        }
        let mut lines = vec![format!(
            "⚠ {} file(s) still need your review from an upgrade:",
            self.pending.len()
        )];
        for (rel, kind) in &self.pending {
            let copy = review_rel(rel);
            let what = if self.markers.contains(rel) {
                "conflict markers left in the file — finish resolving them".to_string()
            } else {
                match kind.as_str() {
                    "conflict" => format!("conflict — resolve {copy}, then replace the file"),
                    "merge" => format!("plugin/user state — merge {copy} by hand"),
                    "new" => format!("new file — move {copy} into place, or run with --apply"),
                    _ => format!("review {copy} and replace the file, or run with --apply"),
                }
            };
            lines.push(format!("    {rel}   {what}"));
        }
        lines.push("  Run `mobiler upgrade` again after resolving; it will confirm.".to_string());
        lines
    }
```

In `print`:
- Add, after the `conflict` loop:
  - `for r in &self.resolved { println!("  ✓ resolved {r}  (baseline advanced)"); }`
  - `for m in &self.markers { println!("  ‼ markers  {m}  (conflict markers left in the file)"); }`
- Change the stamp lines so `-> {cur}` / `v{cur}` gain the suffix
  `if self.pending.is_empty() { String::new() } else { format!(" ({} file(s) pending)", self.pending.len()) }`.
- Right after the `println!();` that follows the stamp, print each `pending_lines()` line.
- Rename the local `pending` count to `offered`.
- Print "Up to date. ✓" only when `offered == 0 && self.updated.is_empty() && self.pending.is_empty() && self.plugins.is_empty()`.

- [ ] **Step 4: Run all tests and watch them pass**

Run: `CARGO_TARGET_DIR=/media/zmilan/data2/cargo-target-mobiler cargo test -q -p mobiler 2>&1 | grep -E "test result|panicked|FAILED"`

Expected: all pass.

- [ ] **Step 5: Commit**: `feat(upgrade): warn on every run while files wait for review`.

### Task 3: a review run never changes a build input (spec rule 4)

**Files:**
- Modify: `mobiler/src/upgrade.rs`:
  - `bump_core_dep` (line ~167) gains `apply: bool`;
  - `upgrade_at` passes `apply` to it;
  - `Report` gains `new_review` and `deps_would`;
  - the `!dst.exists()` branch of `sync_file` changes;
  - `print` gains lines.
- Modify tests: `bumps_dep_stamps_and_leaves_app_code_untouched` calls `upgrade_at(&root, true)`. It is ADR-0043's
  conformance test for "app code untouched"; with `--apply` it still asserts the bump and the added file.

**Interfaces:**
- Consumes: `write_pending`, `clear_pending` and `fingerprint` (Task 1), and `Report.pending` (Task 2).
- Produces:
  - `Report.new_review: Vec<String>`;
  - `Report.deps_would: Option<(String, String)>`;
  - pending kind `"new"`.

- [ ] **Step 1: Write the failing tests**

```rust
    #[test]
    fn review_run_never_touches_build_inputs() {
        let root = skeleton();
        let cargo_before = read(&root, "shared/Cargo.toml");
        let r = upgrade_at(&root, false).unwrap();
        assert_eq!(read(&root, "shared/Cargo.toml"), cargo_before, "no core bump without --apply");
        assert!(r.deps_would.is_some());
        assert!(!root.join("iOS/Sources/Render.swift").exists(), "a new file is not written in place");
        assert!(root.join("iOS/Sources/Render.swift.mobiler-new").exists(), "it is offered for review");
        assert!(r.pending.iter().any(|(f, k)| f == "iOS/Sources/Render.swift" && k == "new"), "{:?}", r.pending);

        let r2 = upgrade_at(&root, true).unwrap();
        assert!(root.join("iOS/Sources/Render.swift").exists());
        let want = template_core_version().unwrap();
        assert!(read(&root, "shared/Cargo.toml").contains(&format!("mobiler-core = \"{want}\"")));
        assert!(r2.pending.is_empty(), "--apply leaves nothing pending: {:?}", r2.pending);
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn accepted_new_file_is_resolved() {
        let root = skeleton();
        upgrade_at(&root, false).unwrap();
        // The user moves the review copy into place by hand.
        fs::rename(root.join("iOS/Sources/Render.swift.mobiler-new"), root.join("iOS/Sources/Render.swift")).unwrap();
        let r = upgrade_at(&root, false).unwrap();
        assert!(r.resolved.iter().any(|f| f == "iOS/Sources/Render.swift"), "{:?}", r.resolved);
        assert!(!r.pending.iter().any(|(f, _)| f == "iOS/Sources/Render.swift"));
        assert!(root.join(".mobiler/base/iOS/Sources/Render.swift").exists(), "baselined");
        let _ = fs::remove_dir_all(&root);
    }
```

- [ ] **Step 2: Run them and watch them fail**

Run: `CARGO_TARGET_DIR=/media/zmilan/data2/cargo-target-mobiler cargo test -q -p mobiler review_run accepted_new 2>&1 | tail -5`

(Use two runs if the filter only takes one name.)

Expected: a compile error (`deps_would`), then runtime failures.

- [ ] **Step 3: Implement**

Add to `Report`:

```rust
    /// New template files offered as review copies (no `--apply`): not written in place.
    new_review: Vec<String>,
    /// The `mobiler-core` bump a review run would make: (from, to).
    deps_would: Option<(String, String)>,
```

In `bump_core_dep(root, apply, report)`, after the `if have == want { return Ok(()); }` line:

```rust
    if !apply {
        // A review run never changes what the build reads (ADR-0046): the bump waits for --apply.
        report.deps_would = Some((have, want));
        return Ok(());
    }
```

In `sync_file`, change the `!dst.exists()` branch so it starts like this:

```rust
    if !dst.exists() {
        // A file the new version introduces. A review run offers it instead: it may need edits that
        // still wait as review copies (e.g. a type registered in codegen.rs).
        if !apply {
            if let Ok(text) = std::str::from_utf8(&desired) {
                write_review(report.app_root.as_deref(), &dst, &desired)?;
                write_pending(root, &rel, text, b"", "new")?;
                report.new_review.push(rel_disp);
                return Ok(());
            }
        }
        // (existing create-and-baseline code follows, with clear_pending added in Task 1)
```

In `print`:
- Add `(None, _) if self.deps_would.is_some()` handling first. Simplest is to start the deps `match` with
  `if let Some((from, to)) = &self.deps_would { println!("  deps: would bump mobiler-core {from} -> {to} (run with --apply)"); } else { …existing match… }`.
- Add `for n in &self.new_review { println!("  + new     {n}  -> {}", review_rel(n)); }` after the `added` loop.
- Add `self.new_review.is_empty() && self.deps_would.is_none()` to the "Up to date. ✓" condition.

In `bumps_dep_stamps_and_leaves_app_code_untouched`, change `upgrade_at(&root, false)` to `upgrade_at(&root, true)`.

- [ ] **Step 4: Run the whole suite and clippy**

Run: `CARGO_TARGET_DIR=/media/zmilan/data2/cargo-target-mobiler cargo test -q -p mobiler -p xtask 2>&1 | grep -E "test result|panicked|FAILED"; CARGO_TARGET_DIR=/media/zmilan/data2/cargo-target-mobiler cargo clippy -q -p mobiler -- -D warnings`

Expected: all pass; clippy clean.

- [ ] **Step 5: Commit**: `fix(upgrade): a review run never changes a file the build reads`.

### Task 4: ADR-0046, mutation proofs, READMEs, version

**Files:**
- Create: `docs/adr/ADR-0046-upgrade-pending-reviews.md`. Follow ADR-0043's section layout: header block,
  §1 Context, §2 Hypothesis with Refutation Conditions, §3 Options, §4 Decision with Mutation proof, §5 Consequences.
- Modify:
  - `docs/adr/ADR-0043-*.md`: the `Status:` line only, to `Superseded by ADR-0046`;
  - `docs/adr/index.md`: ADR-0043's status and "superseded by" cells, plus a new ADR-0046 row;
  - `README.md` "Upgrading an app" (lines ~348–372) and `mobiler/README.md` (lines ~80–100);
  - `mobiler/Cargo.toml`: version `0.64.1`, then refresh the lockfile.

The ADR-0046 header:
- Deciding PRs: the PR number `gh pr create` will get (check `gh pr list --state all -L1`, then add 1; fix it before
  merge if wrong).
- Code anchor: `mobiler/src/upgrade.rs (sync_file, read_pending, write_pending, has_conflict_markers, list_pending, pending_lines, bump_core_dep)`.
- Conformance:
  - `mobiler/src/upgrade.rs::resolved_conflict_stays_resolved`
  - `mobiler/src/upgrade.rs::unresolved_conflict_is_reported_on_every_run`
  - `mobiler/src/upgrade.rs::markers_left_in_file_stay_pending`
  - `mobiler/src/upgrade.rs::review_run_never_touches_build_inputs`
  - plus ADR-0043's carried-over tests: `three_way_applies_framework_change_preserves_edit_and_flags_conflict`,
    `new_seeds_baseline_so_upgrade_is_idempotent`, `bumps_dep_stamps_and_leaves_app_code_untouched`,
    `review_copies_of_android_resources_go_outside_res`.

ADR-0046 content: Context as in the spec's Problem. Conditions:
1. A resolved file stays resolved.
2. Pending files are named on every run.
3. Markers keep a file pending and untouched.
4. A review run never writes a build input.
5. ADR-0043's merge and side-file rules, carried over.

- [ ] **Step 1: Mutation proofs.** Apply each mutation, run `cargo test -q -p mobiler upgrade`, record the failing
  test, then revert:
  - (a) In `sync_file`'s pending check, skip `write_baseline(root, &rel, rec.template…)` and `clear_pending`. Expect
    `resolved_conflict_stays_resolved` to fail.
  - (b) Make `pending_lines` return `Vec::new()` always. Expect `unresolved_conflict_is_reported_on_every_run` to fail.
  - (c) Drop the `has_conflict_markers` early return. Expect `markers_left_in_file_stay_pending` to fail.
  - (d) Delete the `if !apply { … }` block in the `!dst.exists()` branch. Expect
    `review_run_never_touches_build_inputs` to fail.

  Restore and confirm the suite is green. Write the four results into ADR-0046 §4 under "Mutation proof:".

- [ ] **Step 2: Write ADR-0046** and the ADR-0043 Status / index edits.

  Run: `CARGO_TARGET_DIR=/media/zmilan/data2/cargo-target-mobiler cargo test -q -p xtask --test adr_docs`

  Expected: pass.

- [ ] **Step 3: READMEs.** In both upgrade sections, replace the sentence about clean merges being offered as
  `<file>.mobiler-new` with three statements:
  - Without `--apply`, nothing your build reads changes: merges, new files and conflicts are offered as review copies,
    and the `mobiler-core` bump is reported, not made.
  - Everything left for review is recorded in `.mobiler/pending/` (commit it). Every `mobiler upgrade` ends with a
    warning listing those files. Once you've resolved one (the file changed and no `<<<<<<<` markers are left), the
    next run confirms it with `✓ resolved` and it stays resolved.
  - `--apply` writes clean merges and new files in place and bumps `mobiler-core`. It never applies a conflict.

  Also: "Commit `.mobiler/` (the baseline, the version stamp and pending reviews)".

- [ ] **Step 4: Version.** Run
  `sed -i '0,/^version = "0.64.0"/s//version = "0.64.1"/' mobiler/Cargo.toml && CARGO_TARGET_DIR=/media/zmilan/data2/cargo-target-mobiler cargo update -q -p mobiler`.

  Then run `cargo test -q -p mobiler -p xtask`. Expected: green.

- [ ] **Step 5: Commit**: `docs(adr): ADR-0046 upgrade pending reviews (supersedes ADR-0043); CLI 0.64.1`.

### Task 5: runtime check, reviews, PR

- [ ] **Step 1: Runtime check** with a background agent; no emulator is needed. The brief must forbid `pgrep -f` and
  require data2 target dirs. Steps:
  - Install 0.63.0 into a data2 root and scaffold an app with it, then `plugin add battery`.
  - Run the workspace-built `mobiler upgrade --apply`: expect a `Core.kt` conflict and a warning block.
  - Resolve `Core.kt` (both sides) and delete the review copy, then upgrade again: expect `✓ resolved`, no conflict,
    no warning, "Up to date. ✓" or only unrelated lines.
  - Build the app with `mobiler build android`: expect an APK.
  - Second app: scaffold with 0.63.0, run a plain `mobiler upgrade`. Expect `git status`-style evidence that no build
    input changed (Cargo.toml pin still 0.42, no `PhotoPipeline.kt` in place, review copies present), and that
    `mobiler build android` still builds.
  - Clean up everything.
- [ ] **Step 2: Fresh independent code and security review** (opus) of the branch against the spec and ADR-0046.
  Fix the Critical and Important findings test-first, and re-review the fix pass.
- [ ] **Step 3: PR** (ship-pr). The body lists the READMEs checked: root `README.md` and `mobiler/README.md` updated;
  `mobiler-core/README.md` line 86 checked, no change. Fix ADR-0046 "Deciding PRs" if the number differs. Monitor CI
  until all checks pass. The user merges.

### Task 6: release CLI 0.64.1

- [ ] Packaging pre-check: `cargo package -p mobiler --allow-dirty --list` contains all tracked template files and
  plugins.
- [ ] Ask the maintainer to approve. The user runs the merge and the `v0.64.1` tag `!` commands.
- [ ] Watch the release workflow and confirm crates.io shows 0.64.1.
- [ ] Post-release: smoke test the published 0.64.1 (fresh scaffold builds; 0.63→0.64.1 upgrade resolves once). Then
  update `start.md` and memory.
