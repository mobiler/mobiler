//! `mobiler upgrade [--apply]` — bring a scaffolded app's generic native shells and its
//! `mobiler-core` dependency up to the CLI's current templates, without clobbering the user's
//! Rust app code or plugin-patched files.
//!
//! `new` and `upgrade` snapshot the pristine (substituted) shell files into `.mobiler/base/`, so
//! upgrade has the *ancestor* every managed file was generated from. With it, each file is a true
//! **3-way merge** (`base → your file → new template`, via `diffy`): framework changes apply, your
//! edits and plugin injections are preserved, and only genuinely overlapping edits become a
//! conflict (written as `<file>.mobiler-new` with `<<<<<<<`/`>>>>>>>` markers — never auto-applied).
//! A clean merge is written in place with `--apply` (saving `<file>.mobiler-bak`), or offered as
//! `<file>.mobiler-new` by default. Apps scaffolded before baselines existed have no ancestor, so
//! those files fall back to a conservative 2-way reconcile (anchor-aware splice / sidecar) and get
//! a baseline written so the *next* upgrade is a real 3-way merge.

use crate::templating::{Subs, is_binary, substitute, templated_path};
use anyhow::{Context, Result, bail};
use include_dir::{Dir, include_dir};
use std::fs;
use std::path::{Path, PathBuf};

static TEMPLATES: Dir<'_> = include_dir!("$CARGO_MANIFEST_DIR/templates");

/// Project-relative path of the version stamp written by `new` + `upgrade`.
pub(crate) const STAMP_REL: &str = ".mobiler/version";

/// Project-relative dir holding the pristine template snapshot (the 3-way merge ancestor),
/// mirroring the app layout: `.mobiler/base/<app-relative path>`.
const BASE_REL: &str = ".mobiler/base";

/// Anchor markers that mark a file as carrying plugin/user state (patched by `plugin add`).
/// A file whose template contains any of these is MERGE-class: never auto-overwritten.
const ANCHORS: &[&str] = &[
    "mobiler:plugins",
    "mobiler:permissions",
    "mobiler:manifest-application",
    "mobiler:gradle-deps",
    "mobiler:info-plist",
    "mobiler:target-extra",
];

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Class {
    /// The user's app code / per-app identity / binaries — never touched.
    Own,
    /// Carries plugin or user state at an anchor — offered as `.mobiler-new`, never overwritten.
    Merge,
    /// A generic interpreter shell file — the upgrade target.
    Shell,
}

/// Classify a template file by its app-relative path + final (substituted) contents.
fn classify(rel: &Path, desired: &[u8]) -> Class {
    let p = rel.to_string_lossy().replace('\\', "/");
    let name = rel.file_name().and_then(|n| n.to_str()).unwrap_or("");
    // OWN — the user's Rust app, the Cargo manifests (deps handled separately), per-app identity
    // files that always differ, and binaries (icons, the gradle wrapper jar).
    let own = p.starts_with("shared/src/")
        || name == "Cargo.toml"
        || p == "Android/settings.gradle.kts"
        || p == "Android/app/src/main/res/values/strings.xml"
        || p == "iOS/Sources/Info.plist"
        || p == "iOS/Sources/App.swift"
        || p.starts_with("iOS/Sources/Assets.xcassets/")
        || is_binary(rel);
    if own {
        return Class::Own;
    }
    if let Ok(text) = std::str::from_utf8(desired)
        && ANCHORS.iter().any(|a| text.contains(a))
    {
        return Class::Merge;
    }
    Class::Shell
}

/// Outcome of an upgrade run, for both the printed report and tests.
#[derive(Default)]
struct Report {
    deps: Option<(String, String)>,
    deps_note: Option<String>,
    up_to_date: usize,
    added: Vec<String>,
    changed: Vec<String>,
    updated: Vec<String>,
    merge: Vec<String>,
    /// 3-way merges with overlapping edits — written with conflict markers for manual resolution.
    conflict: Vec<String>,
    stamp: Option<(Option<String>, String)>,
}

pub fn run(apply: bool) -> Result<()> {
    let root = std::env::current_dir().context("reading current directory")?;
    let report = upgrade_at(&root, apply)?;
    report.print(apply);
    Ok(())
}

fn upgrade_at(root: &Path, apply: bool) -> Result<Report> {
    if !root.join("Android").is_dir() || !root.join("iOS").is_dir() || !root.join("shared").is_dir() {
        bail!("run `mobiler upgrade` from a Mobiler app root (the dir with Android/, iOS/, shared/)");
    }
    let subs = Subs::from_app_root(root)?;
    let mut report = Report::default();
    bump_core_dep(root, &mut report)?;
    sync_dir(&TEMPLATES, root, &subs, apply, &mut report)?;
    write_stamp(root, &mut report)?;
    Ok(report)
}

// ---------------- dependency bump ----------------

/// The `mobiler-core` version the CLI's templates pin (the target of the bump).
fn template_core_version() -> Option<String> {
    let f = TEMPLATES.get_file("shared/Cargo.toml.tmpl")?;
    extract_dep_version(f.contents_utf8()?, "mobiler-core")
}

/// Read the version of a string-form dependency: `dep = "X"` (ignores leading whitespace).
fn extract_dep_version(cargo: &str, dep: &str) -> Option<String> {
    let prefix = format!("{dep} = \"");
    cargo.lines().find_map(|l| {
        let rest = l.trim_start().strip_prefix(&prefix)?;
        rest.split('"').next().map(str::to_string)
    })
}

fn bump_core_dep(root: &Path, report: &mut Report) -> Result<()> {
    let Some(want) = template_core_version() else {
        return Ok(());
    };
    let path = root.join("shared/Cargo.toml");
    if !path.exists() {
        report.deps_note = Some("no shared/Cargo.toml found — couldn't bump mobiler-core.".into());
        return Ok(());
    }
    let content = fs::read_to_string(&path).with_context(|| format!("reading {}", path.display()))?;
    let Some(have) = extract_dep_version(&content, "mobiler-core") else {
        report.deps_note = Some(
            "couldn't find a `mobiler-core = \"…\"` dependency in shared/Cargo.toml — update it by hand."
                .into(),
        );
        return Ok(());
    };
    if have == want {
        return Ok(());
    }
    let updated = content.replacen(
        &format!("mobiler-core = \"{have}\""),
        &format!("mobiler-core = \"{want}\""),
        1,
    );
    fs::write(&path, updated).with_context(|| format!("writing {}", path.display()))?;
    report.deps = Some((have, want));
    Ok(())
}

// ---------------- shell sync ----------------

fn sync_dir(dir: &Dir<'_>, root: &Path, subs: &Subs, apply: bool, report: &mut Report) -> Result<()> {
    for entry in dir.entries() {
        match entry {
            include_dir::DirEntry::Dir(sub) => sync_dir(sub, root, subs, apply, report)?,
            include_dir::DirEntry::File(file) => sync_file(file, root, subs, apply, report)?,
        }
    }
    Ok(())
}

fn sync_file(
    file: &include_dir::File<'_>,
    root: &Path,
    subs: &Subs,
    apply: bool,
    report: &mut Report,
) -> Result<()> {
    let rel = templated_path(file.path(), subs);
    let desired: Vec<u8> = if is_binary(file.path()) {
        file.contents().to_vec()
    } else {
        let raw = std::str::from_utf8(file.contents())
            .with_context(|| format!("template {} is not UTF-8", file.path().display()))?;
        substitute(raw, subs).into_bytes()
    };

    let class = classify(&rel, &desired);
    if class == Class::Own {
        return Ok(());
    }
    let dst = root.join(&rel);
    let rel_disp = rel.to_string_lossy().to_string();

    if !dst.exists() {
        // A file the new version introduces — additive, safe to create (and baseline).
        if let Some(parent) = dst.parent() {
            fs::create_dir_all(parent).with_context(|| format!("creating {}", parent.display()))?;
        }
        fs::write(&dst, &desired).with_context(|| format!("writing {}", dst.display()))?;
        report.added.push(rel_disp);
        write_baseline(root, &rel, &desired)?;
        return Ok(());
    }

    let current = fs::read(&dst).with_context(|| format!("reading {}", dst.display()))?;

    // Managed (non-OWN) files are text; `pristine` is the new template and the next baseline.
    let Ok(pristine) = std::str::from_utf8(&desired).map(str::to_string) else {
        // Non-UTF-8 managed file (not expected) — degrade to a plain shell reconcile.
        if current == desired {
            report.up_to_date += 1;
        } else {
            shell_write(&dst, &current, &desired, apply, &rel_disp, report)?;
        }
        return Ok(());
    };
    let current_s = String::from_utf8_lossy(&current).into_owned();

    // With a recorded ancestor we do a real 3-way merge; otherwise reconcile conservatively and
    // leave behind a baseline so the next upgrade can.
    let incorporated = match read_baseline(root, &rel) {
        Some(base) => three_way(&base, &current_s, &pristine, &dst, &rel_disp, apply, report)?,
        None => two_way(class, &current, &pristine, &dst, &rel_disp, apply, report)?,
    };
    if incorporated {
        write_baseline(root, &rel, pristine.as_bytes())?;
    }
    Ok(())
}

/// True 3-way merge of a single file. Returns whether the on-disk file now incorporates the new
/// template (so the caller advances its baseline).
fn three_way(
    base: &str,
    current: &str,
    pristine: &str,
    dst: &Path,
    rel_disp: &str,
    apply: bool,
    report: &mut Report,
) -> Result<bool> {
    if current == pristine {
        report.up_to_date += 1;
        return Ok(true);
    }
    match diffy::merge(base, current, pristine) {
        // Clean merge: framework changes layered onto the user's edits with no overlap.
        Ok(merged) if merged == current => {
            report.up_to_date += 1; // user's file already reflected the new template
            Ok(true)
        }
        Ok(merged) if apply => {
            write_sidecar(dst, "mobiler-bak", current.as_bytes())?;
            fs::write(dst, &merged).with_context(|| format!("writing {}", dst.display()))?;
            report.updated.push(rel_disp.to_string());
            Ok(true)
        }
        Ok(merged) => {
            write_sidecar(dst, "mobiler-new", merged.as_bytes())?;
            report.changed.push(rel_disp.to_string());
            Ok(false)
        }
        // Overlapping edits: emit the conflict-marked merge for the user to resolve; never apply.
        Err(conflicted) => {
            write_sidecar(dst, "mobiler-new", conflicted.as_bytes())?;
            report.conflict.push(rel_disp.to_string());
            Ok(false)
        }
    }
}

/// Baseline-free fallback for apps scaffolded before `.mobiler/base/` existed. MERGE-class files
/// (plugin anchors) get an anchor-aware splice; other shell files are a plain overwrite/sidecar.
fn two_way(
    class: Class,
    current: &[u8],
    pristine: &str,
    dst: &Path,
    rel_disp: &str,
    apply: bool,
    report: &mut Report,
) -> Result<bool> {
    let mut merge_failed = false;
    let spliced = if class == Class::Merge {
        std::str::from_utf8(current).ok().and_then(|cur| merge_anchors(pristine, cur))
    } else {
        None
    };
    let desired: Vec<u8> = if let Some(m) = spliced {
        m.into_bytes()
    } else if class == Class::Merge {
        merge_failed = true; // anchor file but couldn't splice safely — keep the raw template
        pristine.as_bytes().to_vec()
    } else {
        pristine.as_bytes().to_vec()
    };

    if current == desired.as_slice() {
        report.up_to_date += 1;
        return Ok(true);
    }

    // A successfully-spliced anchor file is safe to write like a shell file; a splice failure
    // stays hands-off (offered as `.mobiler-new`).
    if class == Class::Merge && merge_failed {
        write_sidecar(dst, "mobiler-new", &desired)?;
        report.merge.push(rel_disp.to_string());
        return Ok(false);
    }
    shell_write(dst, current, &desired, apply, rel_disp, report)
}

/// Write a shell file: overwrite in place (saving `.mobiler-bak`) with `--apply`, else offer it as
/// `.mobiler-new`. Returns whether the on-disk file now holds `desired`.
fn shell_write(
    dst: &Path,
    current: &[u8],
    desired: &[u8],
    apply: bool,
    rel_disp: &str,
    report: &mut Report,
) -> Result<bool> {
    if apply {
        write_sidecar(dst, "mobiler-bak", current)?;
        fs::write(dst, desired).with_context(|| format!("writing {}", dst.display()))?;
        report.updated.push(rel_disp.to_string());
        Ok(true)
    } else {
        write_sidecar(dst, "mobiler-new", desired)?;
        report.changed.push(rel_disp.to_string());
        Ok(false)
    }
}

/// Path of a file's recorded ancestor under `.mobiler/base/`.
fn baseline_path(root: &Path, rel: &Path) -> PathBuf {
    root.join(BASE_REL).join(rel)
}

/// The recorded ancestor for `rel`, if any (`None` for pre-baseline apps).
fn read_baseline(root: &Path, rel: &Path) -> Option<String> {
    fs::read_to_string(baseline_path(root, rel)).ok()
}

/// Record `bytes` as the ancestor for `rel` (the pristine new template).
fn write_baseline(root: &Path, rel: &Path, bytes: &[u8]) -> Result<()> {
    let path = baseline_path(root, rel);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).with_context(|| format!("creating {}", parent.display()))?;
    }
    fs::write(&path, bytes).with_context(|| format!("writing baseline {}", path.display()))
}

/// Snapshot every managed (non-OWN, text) template file into `.mobiler/base/` as the merge
/// ancestor. Called by `mobiler new` so a freshly-scaffolded app upgrades via a true 3-way merge.
pub(crate) fn seed_baseline(root: &Path, subs: &Subs) -> Result<()> {
    seed_dir(&TEMPLATES, root, subs)
}

fn seed_dir(dir: &Dir<'_>, root: &Path, subs: &Subs) -> Result<()> {
    for entry in dir.entries() {
        match entry {
            include_dir::DirEntry::Dir(sub) => seed_dir(sub, root, subs)?,
            include_dir::DirEntry::File(file) => {
                if is_binary(file.path()) {
                    continue;
                }
                let Ok(raw) = std::str::from_utf8(file.contents()) else { continue };
                let rel = templated_path(file.path(), subs);
                let content = substitute(raw, subs);
                if classify(&rel, content.as_bytes()) == Class::Own {
                    continue;
                }
                write_baseline(root, &rel, content.as_bytes())?;
            }
        }
    }
    Ok(())
}

/// Re-apply a user's anchor injections onto the new template. `plugin add` inserts its payload
/// lines immediately above a `mobiler:<anchor>` marker (and copies plugin bodies to separate
/// files), so a user's injected lines are exactly the contiguous lines above each marker in their
/// file that the stock template doesn't contain. We rebuild from `new_tmpl`, splicing those lines
/// back above each marker. Returns `None` if any marker present in the template is missing from
/// the user's file (can't merge safely → caller falls back to a `.mobiler-new` sidecar).
///
/// Invariant this relies on: the template line *directly above* each marker is stable (a base dep
/// / registration that stays in the shell, or a blank line) — true of all current anchor files —
/// so the upward walk stops at it and never mistakes an evolving shell line for a user injection.
fn merge_anchors(new_tmpl: &str, current: &str) -> Option<String> {
    let is_marker = |line: &str| ANCHORS.iter().any(|a| line.contains(*a));
    // Stock lines (trimmed, non-empty) — anything here is template structure, not a user injection.
    let stock: std::collections::HashSet<&str> =
        new_tmpl.lines().map(str::trim).filter(|l| !l.is_empty()).collect();
    let cur_lines: Vec<&str> = current.lines().collect();

    // Trailing-newline fidelity: preserve whatever the template ends with.
    let ends_with_nl = new_tmpl.ends_with('\n');
    let mut out: Vec<String> = Vec::new();
    for line in new_tmpl.lines() {
        if is_marker(line) {
            // Find the matching marker line in the user's file (by the same anchor string).
            let anchor: &str = ANCHORS.iter().copied().find(|a| line.contains(a))?;
            let cur_idx = cur_lines.iter().position(|l| l.contains(anchor))?;
            // Walk upward collecting the user's injected lines (non-blank, not in the template).
            let mut injected: Vec<&str> = Vec::new();
            let mut i = cur_idx;
            while i > 0 {
                let above = cur_lines[i - 1];
                if above.trim().is_empty() || stock.contains(above.trim()) {
                    break;
                }
                injected.push(above);
                i -= 1;
            }
            injected.reverse();
            out.extend(injected.into_iter().map(str::to_string));
        }
        out.push(line.to_string());
    }
    let mut merged = out.join("\n");
    if ends_with_nl {
        merged.push('\n');
    }
    Some(merged)
}

/// Write `<dst>.<suffix>` next to `dst` (e.g. `Render.swift.mobiler-new`).
fn write_sidecar(dst: &Path, suffix: &str, bytes: &[u8]) -> Result<()> {
    let side = PathBuf::from(format!("{}.{suffix}", dst.display()));
    fs::write(&side, bytes).with_context(|| format!("writing {}", side.display()))?;
    Ok(())
}

// ---------------- version stamp ----------------

/// Write the CLI version into `.mobiler/version`. Returns the previous stamp, if any. Shared
/// with `new` so freshly-scaffolded apps are stamped too.
pub(crate) fn write_version_stamp(root: &Path) -> Result<Option<String>> {
    let path = root.join(STAMP_REL);
    let prev = fs::read_to_string(&path)
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty());
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).with_context(|| format!("creating {}", parent.display()))?;
    }
    fs::write(&path, format!("{}\n", env!("CARGO_PKG_VERSION")))
        .with_context(|| format!("writing {}", path.display()))?;
    Ok(prev)
}

fn write_stamp(root: &Path, report: &mut Report) -> Result<()> {
    let prev = write_version_stamp(root)?;
    report.stamp = Some((prev, env!("CARGO_PKG_VERSION").to_string()));
    Ok(())
}

// ---------------- report ----------------

impl Report {
    fn print(&self, apply: bool) {
        match (&self.deps, &self.deps_note) {
            (Some((from, to)), _) => println!("  deps: mobiler-core {from} -> {to} (updated)"),
            (None, Some(note)) => println!("  deps: {note}"),
            (None, None) => println!("  deps: up to date"),
        }
        for a in &self.added {
            println!("  + added   {a}");
        }
        for u in &self.updated {
            println!("  ~ updated {u}  (.mobiler-bak saved)");
        }
        for c in &self.changed {
            println!("  ~ changed {c}  -> {c}.mobiler-new");
        }
        for m in &self.merge {
            println!("  ! merge   {m}  (plugin/user state) -> {m}.mobiler-new");
        }
        for c in &self.conflict {
            println!("  ‼ conflict {c}  (overlapping edits) -> {c}.mobiler-new");
        }
        println!("  = {} file(s) up to date", self.up_to_date);
        if let Some((prev, cur)) = &self.stamp {
            match prev {
                Some(p) if p != cur => println!("  stamp: {p} -> {cur}"),
                _ => println!("  stamp: v{cur}"),
            }
        }

        println!();
        let pending = self.changed.len() + self.merge.len() + self.conflict.len();
        if pending == 0 && self.updated.is_empty() {
            println!("Up to date. ✓");
            return;
        }
        if !self.conflict.is_empty() {
            println!(
                "{} file(s) have overlapping edits — resolve the conflict markers in their \
                 .mobiler-new, then replace the original (never auto-applied).",
                self.conflict.len()
            );
        }
        if !self.changed.is_empty() {
            if apply {
                // (in --apply mode `changed` is empty; shown only for completeness)
            } else {
                println!(
                    "Review the {} .mobiler-new shell file(s) and merge, or re-run with `--apply` \
                     to overwrite in place (a .mobiler-bak is saved).",
                    self.changed.len()
                );
            }
        }
        if !self.updated.is_empty() {
            println!(
                "Overwrote {} shell file(s); your previous versions are saved as *.mobiler-bak.",
                self.updated.len()
            );
        }
        if !self.merge.is_empty() {
            println!(
                "{} file(s) carry plugin/user state — merge their .mobiler-new by hand (never auto-overwritten).",
                self.merge.len()
            );
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    static COUNTER: AtomicUsize = AtomicUsize::new(0);

    #[test]
    fn classify_buckets() {
        // OWN
        assert_eq!(classify(Path::new("shared/src/app.rs"), b"fn main(){}"), Class::Own);
        assert_eq!(classify(Path::new("shared/Cargo.toml"), b""), Class::Own);
        assert_eq!(classify(Path::new("Android/settings.gradle.kts"), b""), Class::Own);
        assert_eq!(classify(Path::new("iOS/Sources/App.swift"), b""), Class::Own);
        assert_eq!(
            classify(Path::new("Android/app/src/main/res/mipmap-hdpi/ic_launcher.webp"), b"\x00"),
            Class::Own
        );
        // MERGE — template carries an anchor
        assert_eq!(
            classify(Path::new("Android/app/src/main/java/dev/x/Core.kt"), b"// mobiler:plugins\n"),
            Class::Merge
        );
        // SHELL — generic, no anchor, not own
        assert_eq!(classify(Path::new("iOS/Sources/Render.swift"), b"func render(){}"), Class::Shell);
        assert_eq!(classify(Path::new("rust-toolchain.toml"), b"[toolchain]"), Class::Shell);
    }

    #[test]
    fn merge_anchors_updates_shell_and_preserves_injections() {
        // Mirrors the real anchor files: an evolving shell line (core → extended) higher up, and a
        // STABLE base line (okhttp) directly above the marker — `plugin add` always inserts its
        // payload between that stable line and the marker.
        let new_tmpl = "deps {\n    impl(\"material-icons-extended\")\n    impl(\"okhttp\")\n    // mobiler:gradle-deps — insert above\n}\n";
        // No plugins installed: adopt the new template verbatim (so the evolved base dep lands).
        let fresh = "deps {\n    impl(\"material-icons-core\")\n    impl(\"okhttp\")\n    // mobiler:gradle-deps — insert above\n}\n";
        assert_eq!(merge_anchors(new_tmpl, fresh).as_deref(), Some(new_tmpl));

        // A plugin injected a line directly above the anchor: it must survive onto the new shell.
        let with_plugin =
            "deps {\n    impl(\"material-icons-core\")\n    impl(\"okhttp\")\n    impl(\"play-services-scanner\")\n    // mobiler:gradle-deps — insert above\n}\n";
        let merged = merge_anchors(new_tmpl, with_plugin).unwrap();
        assert!(merged.contains("material-icons-extended"), "shell evolution applied");
        assert!(merged.contains("play-services-scanner"), "plugin injection preserved");
        assert!(!merged.contains("material-icons-core"), "stale base dep dropped");

        // Marker missing in the user's file ⇒ refuse to merge (caller keeps it hands-off).
        assert_eq!(merge_anchors(new_tmpl, "deps {\n}\n"), None);
    }

    #[test]
    fn three_way_applies_framework_change_preserves_edit_and_flags_conflict() {
        let root = skeleton();
        let dst = root.join("f.txt");
        let side = |s: &str| PathBuf::from(format!("{}.{s}", dst.display()));

        let base = "1\n2\n3\n4\n5\n6\n7\n";
        // Clean 3-way: template changed line 2, user changed line 6 (well separated) — both land.
        let user = "1\n2\n3\n4\n5\nSIX\n7\n";
        let new = "1\nTWO\n3\n4\n5\n6\n7\n";
        fs::write(&dst, user).unwrap();
        let mut r = Report::default();
        let inc = three_way(base, user, new, &dst, "f.txt", true, &mut r).unwrap();
        assert!(inc, "clean merge incorporates the new template");
        assert_eq!(fs::read_to_string(&dst).unwrap(), "1\nTWO\n3\n4\n5\nSIX\n7\n", "both edits merged");
        assert!(side("mobiler-bak").exists(), "backup saved");
        assert_eq!(r.updated.len(), 1);

        // Conflict: template and user both changed the SAME line → never applied; conflict sidecar.
        let user_c = "1\n2\n3\nUSER\n5\n6\n7\n";
        let new_c = "1\n2\n3\nTMPL\n5\n6\n7\n";
        fs::write(&dst, user_c).unwrap();
        let mut r2 = Report::default();
        let inc2 = three_way(base, user_c, new_c, &dst, "f.txt", true, &mut r2).unwrap();
        assert!(!inc2, "conflict does not advance the baseline");
        assert_eq!(fs::read_to_string(&dst).unwrap(), user_c, "original left untouched on conflict");
        assert_eq!(r2.conflict.len(), 1);
        assert!(
            fs::read_to_string(side("mobiler-new")).unwrap().contains("<<<<<<<"),
            "conflict markers offered for resolution"
        );
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn new_seeds_baseline_so_upgrade_is_idempotent() {
        // A freshly-baselined app that's already on the current template: upgrade is a clean no-op
        // (3-way sees base == new, current == new) — no sidecars, nothing to merge.
        let root = skeleton();
        let subs = Subs::from_package("dev.mobiler.demo".into(), "Demo".into(), "30.0.14904198".into());
        // Materialise the shell files + their baselines exactly as `mobiler new` would.
        sync_dir(&TEMPLATES, &root, &subs, true, &mut Report::default()).unwrap();
        seed_baseline(&root, &subs).unwrap();
        assert!(root.join(".mobiler/base/iOS/Sources/Render.swift").exists(), "baseline seeded");

        let report = upgrade_at(&root, false).unwrap();
        assert!(report.changed.is_empty() && report.merge.is_empty() && report.conflict.is_empty(),
            "a current, baselined app has nothing pending");
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn extract_dep_version_reads_string_form() {
        let cargo = "[dependencies]\ncrux_core.workspace = true\nmobiler-core = \"0.11\"\n";
        assert_eq!(extract_dep_version(cargo, "mobiler-core").as_deref(), Some("0.11"));
        assert_eq!(extract_dep_version(cargo, "nope"), None);
    }

    /// A minimal Mobiler app skeleton carrying just enough for `upgrade_at`: the dir markers,
    /// a MainActivity.kt (for `Subs::from_app_root`), and a shared/Cargo.toml.
    fn skeleton() -> PathBuf {
        let n = COUNTER.fetch_add(1, Ordering::SeqCst);
        let root = std::env::temp_dir().join(format!("mob_upgrade_test_{}_{n}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        let pkg = root.join("Android/app/src/main/java/dev/mobiler/demo");
        fs::create_dir_all(&pkg).unwrap();
        fs::create_dir_all(root.join("iOS/Sources")).unwrap();
        fs::create_dir_all(root.join("shared/src")).unwrap();
        fs::write(pkg.join("MainActivity.kt"), "package dev.mobiler.demo\nclass MainActivity\n").unwrap();
        fs::write(root.join("Android/settings.gradle.kts"), "rootProject.name = \"Demo\"\n").unwrap();
        fs::write(
            root.join("shared/Cargo.toml"),
            "[dependencies]\nserde = \"1\"\nmobiler-core = \"0.1.0\"\n",
        )
        .unwrap();
        fs::write(root.join("shared/src/app.rs"), "// MY CUSTOM APP — do not touch\n").unwrap();
        root
    }

    fn read(root: &Path, rel: &str) -> String {
        fs::read_to_string(root.join(rel)).unwrap()
    }

    #[test]
    fn bumps_dep_stamps_and_leaves_app_code_untouched() {
        let root = skeleton();
        let want = template_core_version().expect("templates pin mobiler-core");
        let report = upgrade_at(&root, false).unwrap();

        // dep bumped to the template's version, other deps preserved.
        let cargo = read(&root, "shared/Cargo.toml");
        assert!(cargo.contains(&format!("mobiler-core = \"{want}\"")), "core bumped");
        assert!(cargo.contains("serde = \"1\""), "other deps preserved");
        assert_eq!(report.deps, Some(("0.1.0".into(), want)));

        // app code never touched.
        assert_eq!(read(&root, "shared/src/app.rs"), "// MY CUSTOM APP — do not touch\n");
        assert!(!root.join("shared/src/app.rs.mobiler-new").exists());

        // version stamp written.
        assert_eq!(read(&root, ".mobiler/version").trim(), env!("CARGO_PKG_VERSION"));

        // a real shell file the skeleton lacks gets created (additive).
        assert!(root.join("iOS/Sources/Render.swift").exists(), "missing shell file added");
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn changed_shell_writes_new_then_apply_overwrites_with_backup() {
        let root = skeleton();
        // Seed a SHELL file (rust-toolchain.toml, tokenless) with custom content.
        fs::write(root.join("rust-toolchain.toml"), "OLD\n").unwrap();

        // Default: non-destructive .mobiler-new, original kept.
        upgrade_at(&root, false).unwrap();
        assert_eq!(read(&root, "rust-toolchain.toml"), "OLD\n", "original untouched by default");
        let new = read(&root, "rust-toolchain.toml.mobiler-new");
        assert!(new.contains("toolchain"), "the new template was written as .mobiler-new");

        // --apply: overwrite + back up the old content.
        upgrade_at(&root, true).unwrap();
        assert_eq!(read(&root, "rust-toolchain.toml"), new, "apply installed the new template");
        assert_eq!(read(&root, "rust-toolchain.toml.mobiler-bak"), "OLD\n", "old content backed up");
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn merge_file_without_usable_anchor_stays_hands_off() {
        let root = skeleton();
        // A MERGE-class file (Core.kt template carries a `mobiler:plugins` anchor) whose on-disk
        // copy has no usable marker → merge_anchors() can't splice safely → conservative fallback:
        // never overwritten (even with --apply), offered as .mobiler-new. (The happy path — marker
        // present, injections re-applied onto the new shell — is covered by the merge_anchors unit
        // test and verified end-to-end via `mobiler new` + `upgrade --apply`.)
        let core = root.join("Android/app/src/main/java/dev/mobiler/demo/Core.kt");
        fs::write(&core, "package dev.mobiler.demo\n// my installed plugins\n").unwrap();

        upgrade_at(&root, true).unwrap(); // even with --apply
        assert_eq!(
            fs::read_to_string(&core).unwrap(),
            "package dev.mobiler.demo\n// my installed plugins\n",
            "unmergeable MERGE file is never overwritten"
        );
        assert!(
            root.join("Android/app/src/main/java/dev/mobiler/demo/Core.kt.mobiler-new").exists(),
            "offered as .mobiler-new"
        );
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn reports_previous_stamp_on_reupgrade() {
        let root = skeleton();
        fs::create_dir_all(root.join(".mobiler")).unwrap();
        fs::write(root.join(STAMP_REL), "0.9.0\n").unwrap();
        let report = upgrade_at(&root, false).unwrap();
        assert_eq!(report.stamp, Some((Some("0.9.0".into()), env!("CARGO_PKG_VERSION").to_string())));
        let _ = fs::remove_dir_all(&root);
    }
}
