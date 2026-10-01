//! `mobiler upgrade [--apply]` — bring a scaffolded app's generic native shells and its
//! `mobiler-core` dependency up to the CLI's current templates, without clobbering the user's
//! Rust app code or plugin-patched files.
//!
//! `new` and `upgrade` snapshot the pristine (substituted) shell files into `.mobiler/base/`, so
//! upgrade has the *ancestor* every managed file was generated from. With it, each file is a true
//! **3-way merge** (`base → your file → new template`, via `diffy`): framework changes apply, your
//! edits and plugin injections are preserved, and only genuinely overlapping edits become a
//! conflict (written as `<file>.mobiler-new` with `<<<<<<<`/`>>>>>>>` markers — never auto-applied).
//! A clean merge is written in place with `--apply` (saving the old file under `.mobiler/backup/`), or offered as
//! `<file>.mobiler-new` by default. Apps scaffolded before baselines existed have no ancestor, so
//! those files fall back to a conservative 2-way reconcile (anchor-aware splice / sidecar) and get
//! a baseline written so the *next* upgrade is a real 3-way merge. An Android resource's review copy
//! goes under `.mobiler/new/` instead (Gradle rejects any non-`.xml` file in a resource folder).

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
    "mobiler:plugins-stream",
    "mobiler:app-launch",
    "mobiler:permissions",
    "mobiler:manifest-application",
    "mobiler:gradle-deps",
    "mobiler:gradle-plugins",
    "mobiler:gradle-plugins-classpath",
    "mobiler:info-plist",
    "mobiler:target-extra",
    "mobiler:spm-packages",
    "mobiler:spm-dependencies",
    "mobiler:codegen-types",
];

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Class {
    /// The user's app code / per-app identity / binaries — never touched.
    Own,
    /// Carries plugin or user state at an anchor — offered as `.mobiler-new`, never overwritten.
    Merge,
    /// A generic interpreter shell file — the upgrade target.
    Shell,
    /// App-owned values the CLI ships a default for (the launch window colours): created when missing,
    /// never read, merged or overwritten after that, and never baselined (ADR-0042).
    Seed,
}

/// Seed files (ADR-0042): the launch-window values a later `[splash]` sync writes, app-owned meanwhile.
const SEED_PATHS: &[&str] = &[
    "Android/app/src/main/res/values/mobiler_splash.xml",
    "Android/app/src/main/res/values-night/mobiler_splash.xml",
    "Android/app/src/main/res/drawable/mobiler_launch.xml",
    "iOS/Sources/Assets.xcassets/MobilerSplashBackground.colorset/Contents.json",
];

/// Classify a template file by its app-relative path + final (substituted) contents.
fn classify(rel: &Path, desired: &[u8]) -> Class {
    let p = rel.to_string_lossy().replace('\\', "/");
    let name = rel.file_name().and_then(|n| n.to_str()).unwrap_or("");
    // SEED — checked first, so a seed path wins over the `Assets.xcassets/` OWN prefix.
    if SEED_PATHS.contains(&p.as_str()) {
        return Class::Seed;
    }
    // OWN — the user's Rust app, the Cargo manifests (deps handled separately), per-app identity
    // files that always differ, and binaries (icons, the gradle wrapper jar).
    // NOTE: iOS/Sources/App.swift is NOT own — it's generic shell infrastructure (the entry point +
    // the AppDelegate/PushBridge that remote push needs). Since it carries the `mobiler:app-launch`
    // anchor (launch-time plugin hooks), it is MERGE-class: a user's injected bootstrap() lines are
    // preserved across upgrades (offered as `.mobiler-new` / 3-way-merged), like any other anchored file.
    // NOTE: shared/src/bin/codegen.rs is NOT own either, despite living under shared/src/ — it's
    // CLI-owned scaffolding (builds the `TypeRegistry` that emits the Swift/Kotlin `SharedTypes`),
    // not the user's application code. It must stay in sync with the CLI's templates: types that
    // ride inside an opaque payload (e.g. `HttpOutcome` inside a `Vec<u8>`) can't be reached by
    // `register_app::<App>()`'s traversal and need an explicit `.register_type::<T>()?` call here,
    // or the generated `SharedTypes` are missing types the shells need. It carries the
    // `mobiler:codegen-types` anchor (a user might register their own extra types the same way), so
    // it's MERGE-class, not Shell — an upgrade never silently overwrites a customized copy.
    let own = (p.starts_with("shared/src/") && p != "shared/src/bin/codegen.rs")
        || name == "Cargo.toml"
        || p == "Android/settings.gradle.kts"
        || p == "Android/app/src/main/res/values/strings.xml"
        || p == "iOS/Sources/Info.plist"
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
    /// Installed plugins whose shell sources differ from the ones this CLI ships. Never touched
    /// automatically (they may carry user edits) — reported so the mismatch is not silent.
    plugins: Vec<String>,
    /// The app root, so `--apply` backups go to `<root>/.mobiler/backup/` (None: next to the file).
    app_root: Option<PathBuf>,
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
    let mut report = Report { app_root: Some(root.to_path_buf()), ..Report::default() };
    bump_core_dep(root, &mut report)?;
    sync_dir(&TEMPLATES, root, &subs, apply, &mut report)?;
    report.plugins = crate::plugin::drifted(root, &subs);
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
    if class == Class::Seed {
        // Missing means nothing at the path at all, not even a (dangling) symlink; `create_new` then
        // refuses to follow one created in between.
        let dst = root.join(&rel);
        if dst.symlink_metadata().is_err() {
            if let Some(parent) = dst.parent() {
                fs::create_dir_all(parent).with_context(|| format!("creating {}", parent.display()))?;
            }
            let mut f = fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&dst)
                .with_context(|| format!("creating {}", dst.display()))?;
            std::io::Write::write_all(&mut f, &desired).with_context(|| format!("writing {}", dst.display()))?;
            report.added.push(rel.to_string_lossy().to_string());
        }
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
            write_backup(report.app_root.as_deref(), dst, current.as_bytes())?;
            fs::write(dst, &merged).with_context(|| format!("writing {}", dst.display()))?;
            report.updated.push(rel_disp.to_string());
            Ok(true)
        }
        Ok(merged) => {
            write_review(report.app_root.as_deref(), dst, merged.as_bytes())?;
            report.changed.push(rel_disp.to_string());
            Ok(false)
        }
        // Overlapping edits: emit the conflict-marked merge for the user to resolve; never apply.
        Err(conflicted) => {
            write_review(report.app_root.as_deref(), dst, conflicted.as_bytes())?;
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
    // Non-UTF-8 current bytes are the one genuine "can't splice safely" case left: `merge_anchors`
    // itself always succeeds now (a template anchor absent from the user's file just lands bare).
    let spliced = if class == Class::Merge {
        std::str::from_utf8(current).ok().map(|cur| merge_anchors(pristine, cur))
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
        write_review(report.app_root.as_deref(), dst, &desired)?;
        report.merge.push(rel_disp.to_string());
        return Ok(false);
    }
    shell_write(dst, current, &desired, apply, rel_disp, report)
}

/// Write a shell file: overwrite in place (saving a backup under `.mobiler/backup/`) with `--apply`, else offer it as
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
        write_backup(report.app_root.as_deref(), dst, current)?;
        fs::write(dst, desired).with_context(|| format!("writing {}", dst.display()))?;
        report.updated.push(rel_disp.to_string());
        Ok(true)
    } else {
        write_review(report.app_root.as_deref(), dst, desired)?;
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
                if matches!(classify(&rel, content.as_bytes()), Class::Own | Class::Seed) {
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
/// back above each marker. If a marker is present in the template but absent from the user's
/// file, the anchor is new in THIS release — the user's file predates it and cannot possibly have
/// injected anything above it yet, so that marker simply lands bare (no splice, not a failure);
/// the rest of the merge proceeds normally. This always succeeds; the caller still guards against
/// the one genuine failure mode (current file isn't valid UTF-8 at all) before calling in.
///
/// Invariant this relies on: the template line *directly above* each marker is stable (a base dep
/// / registration that stays in the shell, or a blank line) — true of all current anchor files —
/// so the upward walk stops at it and never mistakes an evolving shell line for a user injection.
fn merge_anchors(new_tmpl: &str, current: &str) -> String {
    // Stock lines (trimmed, non-empty) — anything here is template structure, not a user injection.
    let stock: std::collections::HashSet<&str> =
        new_tmpl.lines().map(str::trim).filter(|l| !l.is_empty()).collect();
    let cur_lines: Vec<&str> = current.lines().collect();

    // Trailing-newline fidelity: preserve whatever the template ends with.
    let ends_with_nl = new_tmpl.ends_with('\n');
    let mut out: Vec<String> = Vec::new();
    for line in new_tmpl.lines() {
        if let Some(anchor) = ANCHORS.iter().copied().find(|a| line.contains(a)) {
            // Find the matching marker line in the user's file (by the same anchor string). If
            // it's missing, the anchor is new in this release — nothing to splice, just fall
            // through and emit the template's marker line as-is.
            if let Some(cur_idx) = cur_lines.iter().position(|l| l.contains(anchor)) {
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
        }
        out.push(line.to_string());
    }
    let mut merged = out.join("\n");
    if ends_with_nl {
        merged.push('\n');
    }
    merged
}

/// Save the file's previous content before `--apply` overwrites it: under the app's
/// `.mobiler/backup/`, at the same relative path, so no stray file lands in the source tree (a
/// `res/xml/x.xml.mobiler-bak` fails Android's resource merge). A later `--apply` overwrites it.
fn write_backup(app_root: Option<&Path>, dst: &Path, bytes: &[u8]) -> Result<()> {
    let Some((root, rel)) = app_root.and_then(|root| dst.strip_prefix(root).ok().map(|rel| (root, rel))) else {
        return write_sidecar(dst, "mobiler-bak", bytes);
    };
    let backup = root.join(".mobiler/backup").join(rel);
    if let Some(parent) = backup.parent() {
        fs::create_dir_all(parent).with_context(|| format!("creating {}", parent.display()))?;
    }
    fs::write(&backup, bytes).with_context(|| format!("writing {}", backup.display()))
}

/// Whether `rel` is an Android resource (`…/src/<sourceSet>/res/…`). Gradle's resource merge rejects
/// any file in a resource folder that isn't `.xml`, so nothing but the real file may sit there.
fn is_android_resource(rel: &str) -> bool {
    let parts: Vec<&str> = rel.split('/').collect();
    parts.iter().enumerate().any(|(i, p)| *p == "res" && i >= 2 && parts[i - 2] == "src" && i + 1 < parts.len())
}

/// Where `upgrade` offers the new version of `rel` for review: `<file>.mobiler-new` next to it, except
/// Android resources, whose copy goes under `.mobiler/new/` (a copy in `res/` breaks the build).
fn review_rel(rel: &str) -> String {
    if is_android_resource(rel) { format!(".mobiler/new/{rel}.mobiler-new") } else { format!("{rel}.mobiler-new") }
}

/// Write the review copy of `dst` (see [`review_rel`]).
fn write_review(app_root: Option<&Path>, dst: &Path, bytes: &[u8]) -> Result<()> {
    let Some((root, rel)) = app_root.and_then(|root| dst.strip_prefix(root).ok().map(|rel| (root, rel))) else {
        return write_sidecar(dst, "mobiler-new", bytes);
    };
    let side = root.join(review_rel(&rel.to_string_lossy().replace('\\', "/")));
    if let Some(parent) = side.parent() {
        fs::create_dir_all(parent).with_context(|| format!("creating {}", parent.display()))?;
    }
    fs::write(&side, bytes).with_context(|| format!("writing {}", side.display()))
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
            println!("  ~ updated {u}  (previous version in .mobiler/backup/)");
        }
        for c in &self.changed {
            println!("  ~ changed {c}  -> {}", review_rel(c));
        }
        for m in &self.merge {
            println!("  ! merge   {m}  (plugin/user state) -> {}", review_rel(m));
        }
        for c in &self.conflict {
            println!("  ‼ conflict {c}  (overlapping edits) -> {}", review_rel(c));
        }
        for p in &self.plugins {
            println!("  ! plugin  {p}  has shell updates in this release");
        }
        println!("  = {} file(s) up to date", self.up_to_date);
        if let Some((prev, cur)) = &self.stamp {
            match prev {
                Some(p) if p != cur => println!("  stamp: {p} -> {cur}"),
                _ => println!("  stamp: v{cur}"),
            }
        }

        println!();
        // A drifted plugin is pending work too: its Rust-side API arrives with the core bump
        // while its shell half stays behind, so "Up to date. ✓" would be a lie.
        if !self.plugins.is_empty() {
            println!(
                "{} installed plugin(s) ship updated shell code in this release. Their Rust API \
                 comes with the core bump, but the native half is only updated by re-adding:",
                self.plugins.len()
            );
            for p in &self.plugins {
                println!("    mobiler plugin add {p}");
            }
            println!(
                "  This overwrites your copy of those plugins' shell sources — back up any local \
                 edits first (registrations and other files are left alone)."
            );
        }
        let pending = self.changed.len() + self.merge.len() + self.conflict.len();
        if pending == 0 && self.updated.is_empty() {
            if self.plugins.is_empty() {
                println!("Up to date. ✓");
            }
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
                     to overwrite in place (previous versions go to .mobiler/backup/).",
                    self.changed.len()
                );
            }
        }
        if !self.updated.is_empty() {
            println!(
                "Overwrote {} shell file(s); your previous versions are saved under .mobiler/backup/.",
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
        assert_eq!(
            classify(Path::new("Android/app/src/main/res/mipmap-hdpi/ic_launcher.webp"), b"\x00"),
            Class::Own
        );
        // MERGE — template carries an anchor
        assert_eq!(
            classify(Path::new("Android/app/src/main/java/dev/x/Core.kt"), b"// mobiler:plugins\n"),
            Class::Merge
        );
        // The project build.gradle.kts now carries the gradle-plugins-classpath anchor (push applies
        // the google-services Gradle plugin there) → MERGE, so an upgrade never wipes it.
        assert_eq!(
            classify(Path::new("Android/build.gradle.kts"), b"plugins {\n    // mobiler:gradle-plugins-classpath\n}\n"),
            Class::Merge
        );
        // App.swift carries the `mobiler:app-launch` anchor (launch-time plugin hooks) → MERGE, so an
        // upgrade preserves a user's injected bootstrap() lines.
        assert_eq!(
            classify(Path::new("iOS/Sources/App.swift"), b"@main struct {{NAME}}App {}\n// mobiler:app-launch\n"),
            Class::Merge
        );
        // SHELL — generic, no anchor, not own.
        assert_eq!(classify(Path::new("iOS/Sources/Render.swift"), b"func render(){}"), Class::Shell);
        assert_eq!(classify(Path::new("rust-toolchain.toml"), b"[toolchain]"), Class::Shell);
    }

    #[test]
    fn codegen_rs_is_carved_out_of_own_but_rest_of_shared_src_stays_own() {
        // shared/src/bin/codegen.rs is CLI-owned scaffolding (it builds the `TypeRegistry` that
        // emits the Swift/Kotlin `SharedTypes`), not the user's application code — despite living
        // under shared/src/, it must NOT be OWN, or `upgrade` can never deliver framework changes
        // to it (e.g. a new `.register_type::<T>()?` call a plugin's payload type needs). It carries
        // the `mobiler:codegen-types` anchor, so it's MERGE-class: a user's own extra registered
        // types are preserved, never silently overwritten.
        assert_eq!(
            classify(
                Path::new("shared/src/bin/codegen.rs"),
                b"TypeRegistry::new()\n    .register_app::<App>()?\n    .register_type::<mobiler_core::HttpOutcome>()?\n    // mobiler:codegen-types - insert above\n    .build()?;\n"
            ),
            Class::Merge,
            "codegen.rs is carved out of Own and classified via its anchor, like Core.kt/App.swift"
        );
        // The rest of shared/src/ — the user's actual app code — must remain untouched by upgrade.
        assert_eq!(classify(Path::new("shared/src/app.rs"), b"fn main(){}"), Class::Own);
        assert_eq!(classify(Path::new("shared/src/lib.rs"), b"pub mod app;"), Class::Own);
        assert_eq!(
            classify(Path::new("shared/src/bin/other_tool.rs"), b"fn main(){}"),
            Class::Own,
            "only codegen.rs is carved out — sibling bin/ tools the user might add stay Own"
        );
    }

    #[test]
    fn merge_anchors_updates_shell_and_preserves_injections() {
        // Mirrors the real anchor files: an evolving shell line (core → extended) higher up, and a
        // STABLE base line (okhttp) directly above the marker — `plugin add` always inserts its
        // payload between that stable line and the marker.
        let new_tmpl = "deps {\n    impl(\"material-icons-extended\")\n    impl(\"okhttp\")\n    // mobiler:gradle-deps — insert above\n}\n";
        // No plugins installed: adopt the new template verbatim (so the evolved base dep lands).
        let fresh = "deps {\n    impl(\"material-icons-core\")\n    impl(\"okhttp\")\n    // mobiler:gradle-deps — insert above\n}\n";
        assert_eq!(merge_anchors(new_tmpl, fresh), new_tmpl);

        // A plugin injected a line directly above the anchor: it must survive onto the new shell.
        let with_plugin =
            "deps {\n    impl(\"material-icons-core\")\n    impl(\"okhttp\")\n    impl(\"play-services-scanner\")\n    // mobiler:gradle-deps — insert above\n}\n";
        let merged = merge_anchors(new_tmpl, with_plugin);
        assert!(merged.contains("material-icons-extended"), "shell evolution applied");
        assert!(merged.contains("play-services-scanner"), "plugin injection preserved");
        assert!(!merged.contains("material-icons-core"), "stale base dep dropped");

        // Marker missing in the user's file entirely (the anchor is new in THIS release — an older
        // file can never contain it) ⇒ merge still succeeds: the template's marker line lands bare,
        // there's simply nothing to splice above it yet. This is the 0.48.0 regression this fix
        // closes — it must NOT be treated as a failure.
        assert_eq!(merge_anchors(new_tmpl, "deps {\n}\n"), new_tmpl);
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
    fn seed_paths_classify_as_seed_and_exist_in_the_template() {
        for p in SEED_PATHS {
            assert_eq!(classify(Path::new(p), b"x"), Class::Seed, "{p}");
        }
        // The rest of the asset catalog stays the app's.
        assert_eq!(classify(Path::new("iOS/Sources/Assets.xcassets/AppIcon.appiconset/Contents.json"), b"{}"), Class::Own);
        // Every seed path is shipped by the template, so the list can't rot.
        for p in SEED_PATHS {
            assert!(TEMPLATES.get_file(p).is_some(), "seed path {p} is not in the template");
        }
    }

    #[test]
    fn missing_seed_files_are_created_and_existing_ones_never_touched() {
        let root = skeleton();
        upgrade_at(&root, false).unwrap();
        for p in SEED_PATHS {
            assert!(root.join(p).exists(), "{p} created");
        }
        let edited = "<resources><color name=\"mobiler_splash_background\">#123456</color></resources>\n";
        fs::write(root.join(SEED_PATHS[0]), edited).unwrap();
        upgrade_at(&root, true).unwrap();
        assert_eq!(read(&root, SEED_PATHS[0]), edited, "seed file left byte-identical");
        assert!(!root.join(format!("{}.mobiler-new", SEED_PATHS[0])).exists());
        assert!(!root.join(".mobiler/base").join(SEED_PATHS[0]).exists(), "no baseline for a seed file");
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn seed_files_created_even_when_theme_is_offered_as_new() {
        let root = skeleton();
        let theme = "Android/app/src/main/res/values/themes.xml";
        fs::create_dir_all(root.join(theme).parent().unwrap()).unwrap();
        fs::write(root.join(theme), "<resources><!-- mine --></resources>\n").unwrap();
        upgrade_at(&root, false).unwrap();
        assert_eq!(read(&root, theme), "<resources><!-- mine --></resources>\n");
        // Gradle rejects any non-.xml file in a resource folder: the review copy goes outside `res/`.
        assert!(!root.join(format!("{theme}.mobiler-new")).exists(), "a review copy inside res/ breaks the Android build");
        assert!(root.join(format!(".mobiler/new/{theme}.mobiler-new")).exists());
        for p in SEED_PATHS {
            assert!(root.join(p).exists(), "{p} created although the theme was not applied");
        }
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn existing_night_theme_is_not_overwritten_even_with_apply() {
        let root = skeleton();
        let night = "Android/app/src/main/res/values-night/themes.xml";
        fs::create_dir_all(root.join(night).parent().unwrap()).unwrap();
        fs::write(root.join(night), "<resources><!-- my night --></resources>\n").unwrap();
        upgrade_at(&root, true).unwrap();
        assert_eq!(read(&root, night), "<resources><!-- my night --></resources>\n");
        assert!(!root.join(format!("{night}.mobiler-new")).exists());
        let _ = fs::remove_dir_all(&root);
    }

    /// The app-facing theme style lives only in `values/themes.xml`; the light/dark/v31 variants define
    /// `Base.Theme.*`, so an app's own items on its theme apply in every configuration, and an app that
    /// keeps its old `themes.xml` is not overridden by a variant (Android doesn't merge a style across
    /// resource qualifiers).
    #[test]
    fn only_the_base_theme_file_defines_the_app_theme() {
        let mut found = Vec::new();
        fn walk(dir: &Dir<'_>, found: &mut Vec<String>) {
            for e in dir.entries() {
                match e {
                    include_dir::DirEntry::Dir(d) => walk(d, found),
                    include_dir::DirEntry::File(f) => {
                        let p = f.path().to_string_lossy().replace('\\', "/");
                        if p.starts_with("Android/app/src/main/res/") && f.contents_utf8().is_some_and(|t| t.contains("<style name=\"Theme.{{NAME}}\"")) {
                            found.push(p);
                        }
                    }
                }
            }
        }
        walk(&TEMPLATES, &mut found);
        assert_eq!(found, ["Android/app/src/main/res/values/themes.xml"]);
        let base = TEMPLATES.get_file("Android/app/src/main/res/values/themes.xml").unwrap().contents_utf8().unwrap();
        assert!(base.contains("parent=\"Base.Theme.{{NAME}}\""), "the app theme must inherit the framework's Base theme");
    }

    #[cfg(unix)]
    #[test]
    fn seed_write_does_not_follow_a_symlink() {
        let root = skeleton();
        let outside = root.with_extension("outside");
        let _ = fs::remove_file(&outside);
        let seed = root.join(SEED_PATHS[0]);
        fs::create_dir_all(seed.parent().unwrap()).unwrap();
        std::os::unix::fs::symlink(&outside, &seed).unwrap(); // dangling: exists() is false
        let _ = upgrade_at(&root, true);
        assert!(!outside.exists(), "the seed write followed a symlink out of the project");
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn review_copies_of_android_resources_go_outside_res() {
        assert_eq!(review_rel("Android/app/src/main/res/values/themes.xml"), ".mobiler/new/Android/app/src/main/res/values/themes.xml.mobiler-new");
        assert_eq!(review_rel("Android/app/src/debug/res/xml/network_security_config.xml"), ".mobiler/new/Android/app/src/debug/res/xml/network_security_config.xml.mobiler-new");
        assert_eq!(review_rel("iOS/Sources/Render.swift"), "iOS/Sources/Render.swift.mobiler-new");
        assert_eq!(review_rel("Android/app/src/main/java/x/res/Core.kt"), "Android/app/src/main/java/x/res/Core.kt.mobiler-new");
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

        // --apply: overwrite + back up the old content, outside the source tree: a stray file next
        // to a resource (e.g. res/xml/x.xml.mobiler-bak) fails Android's resource merge.
        upgrade_at(&root, true).unwrap();
        assert_eq!(read(&root, "rust-toolchain.toml"), new, "apply installed the new template");
        assert_eq!(read(&root, ".mobiler/backup/rust-toolchain.toml"), "OLD\n", "old content backed up");
        assert!(!root.join("rust-toolchain.toml.mobiler-bak").exists(), "no backup next to the file");
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn merge_file_missing_anchor_still_splices_in_the_new_anchor() {
        // The real 0.48.0 regression: a MERGE-class file (Core.kt) whose on-disk copy predates a
        // just-introduced anchor (`mobiler:plugins`) has no way to contain it yet. That must NOT
        // be treated as an unmergeable file — the anchor is new in this release, so it lands bare
        // from the template and the rest of the merge proceeds. (Skeleton has no baseline recorded
        // for Core.kt, so this exercises the 2-way anchor-aware splice path via `two_way`.)
        let root = skeleton();
        let core = root.join("Android/app/src/main/java/dev/mobiler/demo/Core.kt");
        fs::write(&core, "package dev.mobiler.demo\n// my installed plugins\n").unwrap();

        upgrade_at(&root, true).unwrap(); // even with --apply
        let after = fs::read_to_string(&core).unwrap();
        assert!(
            after.contains("mobiler:plugins"),
            "the new anchor landed even though the user's file predates it"
        );
        assert!(
            !root.join("Android/app/src/main/java/dev/mobiler/demo/Core.kt.mobiler-new").exists(),
            "applied in place — not left hands-off as a sidecar"
        );
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn two_way_merge_file_with_non_utf8_current_stays_hands_off() {
        // The one genuine "can't splice safely" case left after the anchor-absent fix: the user's
        // on-disk bytes for a MERGE-class file aren't valid UTF-8 at all, so they can't be scanned
        // for anchors/injections. That must still fall back to `.mobiler-new`, never clobbered.
        let root = skeleton();
        let dst = root.join("f.txt");
        let current: &[u8] = b"not \xFF\xFEvalid utf8 \xC0\xC0\n";
        fs::write(&dst, current).unwrap();
        let pristine = "template content\n// mobiler:plugins\n";

        let mut r = Report::default();
        let inc = two_way(Class::Merge, current, pristine, &dst, "f.txt", true, &mut r).unwrap();
        assert!(!inc, "non-UTF-8 current can't be spliced — not incorporated");
        assert_eq!(fs::read(&dst).unwrap(), current, "original left untouched, even with --apply");
        assert!(root.join("f.txt.mobiler-new").exists(), "offered as .mobiler-new");
        assert_eq!(r.merge.len(), 1);
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
