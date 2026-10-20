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

pub(crate) static TEMPLATES: Dir<'_> = include_dir!("$CARGO_MANIFEST_DIR/templates");

/// Project-relative path of the version stamp written by `new` + `upgrade`.
pub(crate) const STAMP_REL: &str = ".mobiler/version";

/// Project-relative dir holding the pristine template snapshot (the 3-way merge ancestor),
/// mirroring the app layout: `.mobiler/base/<app-relative path>`.
const BASE_REL: &str = ".mobiler/base";
/// Files `upgrade` left for review (a conflict, a review copy, a new file): what was offered, so a
/// later run can tell a resolved file from an ignored one (ADR-0046). Committed with the app.
const PENDING_REL: &str = ".mobiler/pending";

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
pub(crate) const SEED_PATHS: &[&str] = &[
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
    /// Framework pin edits written this run (`shared/Cargo.toml: mobiler-core 0.46 -> 0.47`).
    deps: Vec<String>,
    /// Framework pins upgrade can't place, for the user to edit by hand (ADR-0055).
    deps_notes: Vec<String>,
    up_to_date: usize,
    added: Vec<String>,
    changed: Vec<String>,
    updated: Vec<String>,
    merge: Vec<String>,
    /// 3-way merges with overlapping edits — written with conflict markers for manual resolution.
    conflict: Vec<String>,
    /// Files left for review earlier that the user has since resolved: their baseline advanced.
    resolved: Vec<String>,
    /// Files left for review whose conflict markers are still in the file (never touched).
    markers: Vec<String>,
    /// Every file still waiting for the user's review after this run: (path, kind).
    pending: Vec<(String, String)>,
    /// New template files offered as review copies (no `--apply`): not written in place.
    new_review: Vec<String>,
    /// The framework pin edits a review run would make.
    deps_would: Vec<String>,
    /// Pending records for paths the template no longer produces: deleted, nothing left to review.
    dropped: Vec<String>,
    /// Managed paths holding a symlink (live or dangling): never read or written through, left for the user.
    skipped: Vec<String>,
    /// Every managed path this run visited (to find records for paths no longer produced).
    managed: std::collections::HashSet<String>,
    /// Installed plugins whose shell sources differ from the ones this CLI ships. Never touched
    /// automatically (they may carry user edits) — reported so the mismatch is not silent.
    plugins: Vec<String>,
    /// The app root, so `--apply` backups go to `<root>/.mobiler/backup/` (None: next to the file).
    app_root: Option<PathBuf>,
    stamp: Option<(Option<String>, String)>,
}

pub fn run(apply: bool, resolved: &[String]) -> Result<()> {
    let root = std::env::current_dir().context("reading current directory")?;
    let resolved: Vec<&str> = resolved.iter().map(String::as_str).collect();
    let report = upgrade_with(&root, apply, &resolved)
        .context("upgrade stopped — files reported before this point may already be updated")?;
    report.print(apply);
    Ok(())
}

#[cfg(test)]
fn upgrade_at(root: &Path, apply: bool) -> Result<Report> {
    upgrade_with(root, apply, &[])
}

/// `upgrade`, after settling the files the user named with `--resolved` (ADR-0046).
fn upgrade_with(root: &Path, apply: bool, resolved: &[&str]) -> Result<Report> {
    if !root.join("Android").is_dir() || !root.join("iOS").is_dir() || !root.join("shared").is_dir() {
        bail!("run `mobiler upgrade` from a Mobiler app root (the dir with Android/, iOS/, shared/)");
    }
    let subs = Subs::from_app_root(root)?;
    let mut report = Report { app_root: Some(root.to_path_buf()), ..Report::default() };
    // Check every `--resolved` argument before settling any, so one typo leaves nothing half-done.
    let settle = resolved.iter().map(|file| check_resolved(root, file)).collect::<Result<Vec<_>>>()?;
    let mut seen = std::collections::HashSet::new();
    for (rel, rec) in settle {
        if !seen.insert(rel.clone()) {
            continue; // the same file named twice
        }
        write_baseline(root, &rel, rec.template.as_bytes())?;
        clear_pending(root, &rel);
        let rel_disp = rel.to_string_lossy().to_string();
        let _ = fs::remove_file(root.join(review_rel(&rel_disp)));
        report.resolved.push(rel_disp);
    }
    move_framework_pins(root, apply, &mut report)?;
    sync_dir(&TEMPLATES, root, &subs, apply, &mut report)?;
    report.plugins = crate::plugin::drifted(root, &subs);
    // A record for a path this template no longer produces (a dropped file, a renamed package, a
    // file now app-owned) has nothing left to review.
    for (rel, _) in list_pending(root) {
        if !report.managed.contains(&rel) {
            clear_pending(root, Path::new(&rel));
            report.dropped.push(rel);
        }
    }
    report.pending = list_pending(root);
    write_stamp(root, &mut report)?;
    Ok(report)
}

// ---------------- framework pins (ADR-0055) ----------------

/// The facet version `crux_core` 0.20 and `mobiler-core` require. The template declares no facet
/// dependency, so an app that declares its own is told this rather than edited.
const FACET_PIN: &str = "=0.46.5";

/// One manifest edit: the new text and what changed, nothing to do, or a hand edit to report.
#[derive(Debug, PartialEq, Eq)]
enum Edit {
    Changed(String, String),
    Unchanged,
    Unrecognised(String),
}

/// The version an embedded template pins for `dep` (string form `dep = "X"`): the upgrade target.
fn template_pin(template: &str, dep: &str) -> Option<String> {
    extract_dep_version(TEMPLATES.get_file(template)?.contents_utf8()?, dep)
}

/// A full dependency line from an embedded template (`cargo_metadata = { … }`), for insertion.
fn template_dep_line(template: &str, dep: &str) -> Option<String> {
    let prefix = format!("{dep} = ");
    TEMPLATES.get_file(template)?.contents_utf8()?.lines().find(|l| l.starts_with(&prefix)).map(str::to_string)
}

/// Read the version of a string-form dependency: `dep = "X"` (ignores leading whitespace).
fn extract_dep_version(cargo: &str, dep: &str) -> Option<String> {
    let prefix = format!("{dep} = \"");
    cargo.lines().find_map(|l| {
        let rest = l.trim_start().strip_prefix(&prefix)?;
        rest.split('"').next().map(str::to_string)
    })
}

/// The version a manifest gives `dep` in either form: `dep = "X"` or `dep = { version = "X", … }`.
fn any_dep_version(cargo: &str, dep: &str) -> Option<String> {
    let prefix = format!("{dep} = ");
    cargo.lines().find_map(|l| {
        let rest = l.trim_start().strip_prefix(&prefix)?.trim_start();
        let quoted = if let Some(q) = rest.strip_prefix('"') {
            q
        } else {
            let after = rest.strip_prefix('{')?.split("version").nth(1)?;
            after.trim_start().strip_prefix('=')?.trim_start().strip_prefix('"')?
        };
        quoted.split('"').next().map(str::to_string)
    })
}

/// The file's line ending, so inserted lines match it.
fn eol(content: &str) -> &'static str {
    if content.contains("\r\n") { "\r\n" } else { "\n" }
}

/// Whether requirement `have` already accepts what `want` asks for under Cargo's caret rules:
/// `0.20` and `^0.20.0` match `0.20.0`, `0.47.1` matches `0.47`. An exact `=X` pin matches only `X`;
/// any other operator counts as different (so it gets reported).
fn compatible(have: &str, want: &str) -> bool {
    fn caret_key(v: &str) -> Option<Vec<u64>> {
        let parts = v.split('.').map(|p| p.parse::<u64>().ok()).collect::<Option<Vec<_>>>()?;
        let at = |i: usize| parts.get(i).copied().unwrap_or(0);
        Some(match (at(0), at(1)) {
            (0, 0) => vec![0, 0, at(2)],
            (0, minor) => vec![0, minor],
            (major, _) => vec![major],
        })
    }
    let want = want.trim().trim_start_matches(['=', '^']);
    let have = have.trim();
    if let Some(exact) = have.strip_prefix('=') {
        return exact.trim() == want;
    }
    match (caret_key(have.trim_start_matches('^')), caret_key(want)) {
        (Some(h), Some(w)) => h == w,
        _ => false,
    }
}

/// Whether the manifest takes `dep` from somewhere other than crates.io (`path`, `git`, the
/// workspace): not the framework's pin to move.
fn non_registry(content: &str, dep: &str) -> bool {
    content.lines().any(|l| {
        let t = l.trim_start();
        t.starts_with(&format!("{dep}.workspace"))
            || t.starts_with(&format!("{dep}.path"))
            || t.starts_with(&format!("{dep}.git"))
            || t.strip_prefix(&format!("{dep} = {{")).is_some_and(|rest| {
                rest.contains("path =") || rest.contains("git =") || rest.contains("workspace = true")
            })
    })
}

/// Move a string-form pin `dep = "X"` to `want`. A pin that already accepts `want` stays as it is;
/// a path, git or workspace dependency is not touched; any other form is reported.
fn set_pin(content: &str, dep: &str, want: &str) -> Edit {
    if non_registry(content, dep) {
        return Edit::Unchanged;
    }
    let prefix = format!("{dep} = \"");
    let mut out = String::with_capacity(content.len());
    let mut moved = None;
    for line in content.split_inclusive('\n') {
        if moved.is_none()
            && let Some(rest) = line.trim_start().strip_prefix(&prefix)
            && let Some((have, tail)) = rest.split_once('"')
        {
            if have == want || compatible(have, want) {
                return Edit::Unchanged;
            }
            let indent = &line[..line.len() - line.trim_start().len()];
            for piece in [indent, prefix.as_str(), want, "\"", tail] {
                out.push_str(piece);
            }
            moved = Some(have.to_string());
            continue;
        }
        out.push_str(line);
    }
    match moved {
        Some(have) => Edit::Changed(out, format!("{dep} {have} -> {want}")),
        None => Edit::Unrecognised(format!("couldn't find a `{dep} = \"…\"` line — set {dep} to \"{want}\" by hand.")),
    }
}

/// crux 0.19 removed `crux_core/cli`: the codegen feature takes `UniFFI`'s bindgen and the two crates
/// the codegen helper uses instead (ADR-0054). Only the one-entry-per-line form is rewritten.
fn migrate_codegen_feature(content: &str) -> Edit {
    let mut out = String::with_capacity(content.len() + 64);
    let mut done = false;
    for line in content.split_inclusive('\n') {
        if !done && line.trim() == "\"crux_core/cli\"," {
            let indent = &line[..line.len() - line.trim_start().len()];
            let end = if line.ends_with("\r\n") { "\r\n" } else if line.ends_with('\n') { "\n" } else { "" };
            for entry in ["\"uniffi/bindgen\",", "\"dep:cargo_metadata\",", "\"dep:camino\","] {
                for piece in [indent, entry, if end.is_empty() { eol(content) } else { end }] {
                    out.push_str(piece);
                }
            }
            done = true;
            continue;
        }
        out.push_str(line);
    }
    if done {
        Edit::Changed(out, "codegen feature: crux_core/cli -> uniffi/bindgen, cargo_metadata, camino".into())
    } else if content.contains("\"crux_core/cli\"") {
        Edit::Unrecognised(
            "the codegen feature names \"crux_core/cli\" in a form upgrade doesn't rewrite — replace it with \
             \"uniffi/bindgen\", \"dep:cargo_metadata\", \"dep:camino\" by hand."
                .into(),
        )
    } else {
        Edit::Unchanged
    }
}

/// Add the codegen helper's two optional dependencies right after the `uniffi = { …, optional = true }`
/// line, each only if missing.
fn add_codegen_deps(content: &str) -> Edit {
    let wanted: Vec<(&str, String)> = ["cargo_metadata", "camino"]
        .into_iter()
        .filter_map(|dep| template_dep_line("shared/Cargo.toml.tmpl", dep).map(|l| (dep, l)))
        .collect();
    let has = |dep: &str| {
        content.lines().any(|l| {
            let t = l.trim_start();
            t.starts_with(&format!("{dep} =")) || t.starts_with(&format!("{dep}.")) || t.starts_with(&format!("{dep}="))
        })
    };
    let missing: Vec<&(&str, String)> = wanted.iter().filter(|(dep, _)| !has(dep)).collect();
    if missing.is_empty() {
        return Edit::Unchanged;
    }
    let names = missing.iter().map(|(d, _)| *d).collect::<Vec<_>>().join(" and ");
    let end = eol(content);
    let mut out = String::with_capacity(content.len() + 128);
    let mut done = false;
    for line in content.split_inclusive('\n') {
        out.push_str(line);
        if !done && line.trim_start().starts_with("uniffi = {") && line.contains("optional = true") {
            if !line.ends_with('\n') {
                out.push_str(end);
            }
            for (_, dep_line) in &missing {
                out.push_str(dep_line);
                out.push_str(end);
            }
            done = true;
        }
    }
    if done {
        Edit::Changed(out, format!("added {names} (codegen)"))
    } else {
        let lines = missing.iter().map(|(_, l)| format!("`{l}`")).collect::<Vec<_>>().join(" and ");
        Edit::Unrecognised(format!(
            "couldn't find the `uniffi = {{ …, optional = true }}` line — add {lines} to [dependencies] by hand."
        ))
    }
}

/// Record one edit in the report and fold a change into `content`.
fn take(edit: Edit, file: &str, content: &mut String, changes: &mut Vec<String>, report: &mut Report) {
    match edit {
        Edit::Changed(new, what) => {
            *content = new;
            changes.push(format!("{file}: {what}"));
        }
        Edit::Unchanged => {}
        Edit::Unrecognised(note) => report.deps_notes.push(format!("{file}: {note}")),
    }
}

/// Write the edited manifest on `--apply`; a review run only lists the edits (ADR-0046).
fn commit_edits(root: &Path, file: &str, content: String, changes: Vec<String>, apply: bool, report: &mut Report) -> Result<()> {
    if changes.is_empty() {
        return Ok(());
    }
    if apply {
        crate::fsguard::write(root, root.join(file), content)?;
        report.deps.extend(changes);
    } else {
        report.deps_would.extend(changes);
    }
    Ok(())
}

/// Move the dependency lines the framework owns in the app's two template manifests (ADR-0055), and
/// report the framework pins elsewhere in the app that need a hand edit.
fn move_framework_pins(root: &Path, apply: bool, report: &mut Report) -> Result<()> {
    let (Some(crux), Some(core)) =
        (template_pin("Cargo.toml.tmpl", "crux_core"), template_pin("shared/Cargo.toml.tmpl", "mobiler-core"))
    else {
        return Ok(());
    };
    for file in ["Cargo.toml", "shared/Cargo.toml"] {
        let path = root.join(file);
        if !path.exists() {
            report.deps_notes.push(format!("no {file} found — couldn't move the framework's pins."));
            continue;
        }
        let mut content = fs::read_to_string(&path).with_context(|| format!("reading {}", path.display()))?;
        let mut changes = Vec::new();
        if file == "Cargo.toml" {
            take(set_pin(&content, "crux_core", &crux), file, &mut content, &mut changes, report);
        } else {
            take(set_pin(&content, "mobiler-core", &core), file, &mut content, &mut changes, report);
            take(migrate_codegen_feature(&content), file, &mut content, &mut changes, report);
            take(add_codegen_deps(&content), file, &mut content, &mut changes, report);
        }
        commit_edits(root, file, content, changes, apply, report)?;
    }
    let skip = [root.join("Cargo.toml"), root.join("shared/Cargo.toml")];
    let skip: Vec<&Path> = skip.iter().map(PathBuf::as_path).collect();
    report.deps_notes.extend(foreign_pins(root, &skip));
    Ok(())
}

/// Every other app manifest's framework pin that differs from the template's, and any facet
/// dependency at all (the template declares none): one hand-edit note each, never an edit.
/// `skip` manifests are checked for facet only (their framework pins are moved above).
fn foreign_pins(root: &Path, skip: &[&Path]) -> Vec<String> {
    let Some(crux) = template_pin("Cargo.toml.tmpl", "crux_core") else { return Vec::new() };
    let Some(core) = template_pin("shared/Cargo.toml.tmpl", "mobiler-core") else { return Vec::new() };
    // mobiler-web publishes in lockstep with mobiler-core, so it moves to the same version.
    let targets = [("crux_core", crux), ("mobiler-core", core.clone()), ("mobiler-web", core)];
    let mut manifests = Vec::new();
    collect_manifests(root, &mut manifests);
    manifests.sort();
    let mut notes = Vec::new();
    for path in &manifests {
        let Ok(content) = fs::read_to_string(path) else { continue };
        let rel = path.strip_prefix(root).unwrap_or(path).to_string_lossy().replace('\\', "/");
        let targets: &[(&str, String)] = if skip.contains(&path.as_path()) { &[] } else { &targets };
        notes.extend(pin_notes(&rel, &content, targets));
    }
    // Crates outside the app folder that the app builds through `path` dependencies (an app core
    // kept in the repository's main workspace): read and reported, never written.
    for path in outside_manifests(root, &manifests) {
        let Ok(content) = fs::read_to_string(&path) else { continue };
        notes.extend(pin_notes(&relative_to(root, &path), &content, &targets));
    }
    notes
}

/// The hand-edit notes for one manifest: each framework dependency in `targets` that its version
/// doesn't accept, and a `facet` dependency off the framework's.
fn pin_notes(rel: &str, content: &str, targets: &[(&str, String)]) -> Vec<String> {
    let mut notes = Vec::new();
    for (dep, want) in targets {
        if let Some(have) = any_dep_version(content, dep)
            && !compatible(&have, want)
        {
            notes.push(format!("{rel}: {dep} {have} -> {want} (edit by hand)"));
        }
    }
    if let Some(have) = any_dep_version(content, "facet")
        && have != FACET_PIN
    {
        notes.push(format!("{rel}: declares facet {have}; it must be {FACET_PIN} to match crux_core 0.20 (edit by hand)"));
    }
    notes
}

/// Manifests outside `root` that the app reaches by following `path` dependencies from `inside`
/// (transitively), plus the workspace root of any of them that inherits a framework dependency
/// (`dep.workspace = true`), since that is where its version is written. Sorted, each once.
fn outside_manifests(root: &Path, inside: &[PathBuf]) -> Vec<PathBuf> {
    let Ok(root) = root.canonicalize() else { return Vec::new() };
    let mut seen: std::collections::BTreeSet<PathBuf> = inside.iter().filter_map(|p| p.canonicalize().ok()).collect();
    let mut queue: Vec<PathBuf> = seen.iter().cloned().collect();
    let mut outside = std::collections::BTreeSet::new();
    while let Some(manifest) = queue.pop() {
        let Ok(content) = fs::read_to_string(&manifest) else { continue };
        let Some(dir) = manifest.parent() else { continue };
        // A local checkout of the framework itself (a `[patch]`, or a path to mobiler-core) is on
        // its own versions: neither judged nor walked into.
        if package_name(&content).is_some_and(is_framework_crate) {
            continue;
        }
        let is_outside = !manifest.starts_with(&root);
        if is_outside {
            outside.insert(manifest.clone());
            let inherits = ["crux_core", "mobiler-core", "mobiler-web", "facet"]
                .iter()
                .any(|dep| inherits_from_workspace(&content, dep));
            if inherits
                && let Some(ws) = workspace_root(dir)
                && !ws.starts_with(&root)
            {
                outside.insert(ws);
            }
        }
        for rel in path_values(&content) {
            // `[lib] path = "src/lib.rs"` and friends name a file, so they have no Cargo.toml.
            let Ok(next) = dir.join(rel).join("Cargo.toml").canonicalize() else { continue };
            if next.is_file() && seen.insert(next.clone()) {
                queue.push(next);
            }
        }
    }
    outside.into_iter().collect()
}

/// The `name` under a manifest's `[package]`.
fn package_name(content: &str) -> Option<&str> {
    let mut in_package = false;
    for line in content.lines() {
        let t = line.trim();
        if t.starts_with('[') {
            in_package = t.split('#').next().is_some_and(|h| h.trim() == "[package]");
        } else if in_package
            && let Some(rest) = t.strip_prefix("name")
            && let Some(value) = rest.trim_start().strip_prefix('=')
        {
            return value.trim().trim_matches(['"', '\'']).split(['"', '\'']).next();
        }
    }
    None
}

/// The framework's own crates and the crates it builds on, whose versions it chooses.
fn is_framework_crate(name: &str) -> bool {
    matches!(name, "mobiler-core" | "mobiler-ui" | "mobiler-web") || name.starts_with("crux_") || name.starts_with("facet")
}

/// Every `path = "…"` value in a manifest (`dep = { path = … }`, `dep.path = …`, a
/// `[dependencies.dep]` table's `path = …`, and target paths, which the caller filters out).
fn path_values(content: &str) -> Vec<&str> {
    let mut out = Vec::new();
    for line in content.lines() {
        let line = line.split('#').next().unwrap_or("");
        let mut rest = line;
        while let Some(at) = rest.find("path") {
            let before = rest[..at].chars().next_back();
            let after = rest[at + 4..].trim_start();
            rest = &rest[at + 4..];
            if before.is_some_and(|c| c.is_alphanumeric() || c == '_' || c == '-') {
                continue;
            }
            let Some(value) = after.strip_prefix('=').map(str::trim_start).and_then(|v| v.strip_prefix('"')) else {
                continue;
            };
            if let Some(value) = value.split('"').next() {
                out.push(value);
            }
        }
    }
    out
}

/// Whether `dep` takes its version from the workspace (`dep.workspace = true` or
/// `dep = { workspace = true, … }`).
fn inherits_from_workspace(content: &str, dep: &str) -> bool {
    content.lines().any(|l| {
        let t = l.trim_start();
        t.strip_prefix(&format!("{dep}.workspace")).is_some_and(|r| r.trim_start().starts_with('='))
            || t.strip_prefix(&format!("{dep} = {{")).is_some_and(|r| r.contains("workspace = true"))
    })
}

/// The nearest manifest at or above `dir` that declares `[workspace]`.
fn workspace_root(dir: &Path) -> Option<PathBuf> {
    dir.ancestors().map(|d| d.join("Cargo.toml")).find(|m| {
        fs::read_to_string(m).is_ok_and(|c| c.lines().any(|l| l.trim() == "[workspace]"))
    })
}

/// `path` as seen from `base`, with `..` steps where it leaves `base` (both canonical when they
/// can be), `/`-separated for the report.
fn relative_to(base: &Path, path: &Path) -> String {
    let base = base.canonicalize().unwrap_or_else(|_| base.to_path_buf());
    let path = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
    let (b, p): (Vec<_>, Vec<_>) = (base.components().collect(), path.components().collect());
    let common = b.iter().zip(&p).take_while(|(x, y)| x == y).count();
    let mut parts: Vec<String> = std::iter::repeat_n("..".to_string(), b.len() - common).collect();
    parts.extend(p[common..].iter().map(|c| c.as_os_str().to_string_lossy().into_owned()));
    parts.join("/")
}

/// Every `Cargo.toml` under `dir`, skipping build output, dot-dirs (`.git`, `.mobiler`, `.gradle`) and
/// symlinks (never followed).
fn collect_manifests(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(dir) else { return };
    for entry in entries.flatten() {
        let Ok(kind) = entry.file_type() else { continue };
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if kind.is_dir() {
            if !(name.starts_with('.') || matches!(name.as_ref(), "target" | "node_modules" | "build" | "Pods")) {
                collect_manifests(&entry.path(), out);
            }
        } else if kind.is_file() && name == "Cargo.toml" {
            out.push(entry.path());
        }
    }
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
            crate::fsguard::check(root, &dst)?; // no symlinked folder on the way either
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

    report.managed.insert(rel_disp.clone());
    // A symlink (to a real file or to nothing): upgrade never writes through links — its target may be
    // outside the app — so the path is reported and left alone, with any pending review kept as is.
    if dst.symlink_metadata().is_ok_and(|m| m.file_type().is_symlink()) {
        report.skipped.push(rel_disp);
        return Ok(());
    }
    if check_pending(root, &rel, &dst, &rel_disp, report)? {
        return Ok(());
    }

    if !dst.exists() {
        // A file the new version introduces. A review run offers it instead: it may need edits that
        // still wait as review copies (e.g. a type registered in codegen.rs).
        if !apply && let Ok(text) = std::str::from_utf8(&desired) {
            write_review(report.app_root.as_deref(), &dst, &desired)?;
            let offer = Offer { template: text, current: b"", kind: "new", offered: &desired };
            write_pending(root, &rel, &offer)?;
            report.new_review.push(rel_disp);
            return Ok(());
        }
        // With --apply it is additive, safe to create (and baseline).
        crate::fsguard::write(root, &dst, &desired)?;
        report.added.push(rel_disp);
        write_baseline(root, &rel, &desired)?;
        clear_pending(root, &rel);
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
        let offered = fs::read(root.join(review_rel(&rel_disp))).unwrap_or_default();
        let offer = Offer { template: &pristine, current: &current, kind, offered: &offered };
        write_pending(root, &rel, &offer)?;
    }
    Ok(())
}

/// Settle a file an earlier run left for review (ADR-0046). Resolved: what was offered becomes its
/// baseline and the record goes. Returns true when the file still has conflict markers, so the caller
/// leaves it untouched; otherwise the normal merge follows.
fn check_pending(root: &Path, rel: &Path, dst: &Path, rel_disp: &str, report: &mut Report) -> Result<bool> {
    // A file left for review by an earlier run: changed since then and marker-free means the user
    // may have resolved it. Unchanged: the normal merge offers it again.
    if let Some(rec) = read_pending(root, rel)
        && dst.exists()
    {
        let now_bytes = fs::read(dst).with_context(|| format!("reading {}", dst.display()))?;
        if fingerprint(&now_bytes) != rec.file_hash {
            let now = String::from_utf8_lossy(&now_bytes);
            if has_conflict_markers(&now) {
                report.markers.push(rel_disp.to_string());
                return Ok(true);
            }
            // Settled without being told only when the user took the review copy as offered. Any other
            // edit (plugin add, git pull, a formatter, a stub of their own) goes through the normal merge
            // below, which applies the offer or offers it again, never drops it. Anything else is settled
            // by `--resolved` (ADR-0046): from the file alone, "dealt with it, kept my version" and
            // "edited before looking" are the same.
            let resolved = fingerprint(&now_bytes) == rec.offered_hash;
            if resolved {
                write_baseline(root, rel, rec.template.as_bytes())?;
                clear_pending(root, rel);
                report.resolved.push(rel_disp.to_string());
            }
        }
    }
    Ok(false)
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
            crate::fsguard::write(guard_root(report.app_root.as_deref(), dst), dst, &merged)?;
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
        crate::fsguard::write(guard_root(report.app_root.as_deref(), dst), dst, desired)?;
        report.updated.push(rel_disp.to_string());
        Ok(true)
    } else {
        write_review(report.app_root.as_deref(), dst, desired)?;
        report.changed.push(rel_disp.to_string());
        Ok(false)
    }
}

/// The root the write guard checks folders from: the app root, or (tests without one) the file's folder.
fn guard_root<'a>(app_root: Option<&'a Path>, dst: &'a Path) -> &'a Path {
    app_root.or_else(|| dst.parent()).unwrap_or(Path::new("."))
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
    crate::fsguard::write(root, &path, bytes)
}

/// Path of `rel`'s pending record: `.mobiler/pending/<rel>.json`.
fn pending_path(root: &Path, rel: &Path) -> PathBuf {
    let mut p = root.join(PENDING_REL).join(rel).into_os_string();
    p.push(".json");
    PathBuf::from(p)
}

/// A stable fingerprint of a file's bytes (FNV-1a 64): tells "changed since offered" apart. Not a
/// security hash; the same across Rust versions and machines, unlike `DefaultHasher`.
pub(crate) fn fingerprint(bytes: &[u8]) -> String {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for (i, b) in bytes.iter().enumerate() {
        // CRLF hashes as LF: a teammate's Windows checkout (core.autocrlf) is not a change.
        if *b == b'\r' && bytes.get(i + 1) == Some(&b'\n') {
            continue;
        }
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
    /// `fingerprint` of the review copy as offered: the file equal to it means the user took it.
    offered_hash: String,
}

/// The pending record for `rel`; a missing or unreadable one is no record.
fn read_pending(root: &Path, rel: &Path) -> Option<PendingRecord> {
    let text = fs::read_to_string(pending_path(root, rel)).ok()?;
    let v: serde_json::Value = serde_json::from_str(&text).ok()?;
    Some(PendingRecord {
        template: v["template"].as_str()?.to_string(),
        file_hash: v["file_hash"].as_str()?.to_string(),
        kind: v["kind"].as_str()?.to_string(),
        offered_hash: v["offered_hash"].as_str().unwrap_or_default().to_string(),
    })
}

/// A pending record's inputs: what was offered for a file and the file as it was then.
struct Offer<'a> {
    template: &'a str,
    current: &'a [u8],
    kind: &'a str,
    /// The review copy as written (the template itself for a new file).
    offered: &'a [u8],
}

fn write_pending(root: &Path, rel: &Path, offer: &Offer<'_>) -> Result<()> {
    let path = pending_path(root, rel);
    let json = serde_json::json!({
        "template": offer.template,
        "file_hash": fingerprint(offer.current),
        "kind": offer.kind,
        "offered_hash": fingerprint(offer.offered),
    });
    crate::fsguard::write(root, &path, serde_json::to_string_pretty(&json)?)
}

fn clear_pending(root: &Path, rel: &Path) {
    let path = pending_path(root, rel);
    let _ = fs::remove_file(&path);
    // Drop the folders the record emptied, up to `.mobiler/pending/` (remove_dir fails on a non-empty one).
    let top = root.join(PENDING_REL);
    let mut dir = path.parent();
    while let Some(d) = dir.filter(|d| d.starts_with(&top) && *d != top) {
        if fs::remove_dir(d).is_err() {
            break;
        }
        dir = d.parent();
    }
}

/// Whether `text` still holds a merge conflict: a whole line that is a marker.
fn has_conflict_markers(text: &str) -> bool {
    text.lines().any(|l| {
        l.starts_with("<<<<<<< ") || l.starts_with("||||||| ") || l == "=======" || l.starts_with(">>>>>>> ")
    })
}

/// `--resolved <file>`: the user has dealt with a pending review: resolved a conflict, or decided to
/// keep their own version of any offered file. What was offered becomes the file's baseline, so the
/// next merge layers only newer template changes; the record and the review copy go. Checked here,
/// applied by the caller once every argument passed: the file is inside the app, exists, has a
/// pending review and no conflict markers.
fn check_resolved(root: &Path, arg: &str) -> Result<(PathBuf, PendingRecord)> {
    let given = Path::new(arg);
    let canon_root = fs::canonicalize(root).ok();
    let inside = given
        .strip_prefix(root)
        .ok()
        .or_else(|| canon_root.as_deref().and_then(|c| given.strip_prefix(c).ok()))
        .unwrap_or(given);
    let mut rel = inside.to_string_lossy().replace('\\', "/");
    while let Some(r) = rel.strip_prefix("./") {
        rel = r.to_string();
    }
    // The review copy's path names the same file.
    if let Some(r) = rel.strip_prefix(".mobiler/new/") {
        rel = r.to_string();
    }
    if let Some(r) = rel.strip_suffix(".mobiler-new") {
        rel = r.to_string();
    }
    let given_rel = PathBuf::from(&rel);
    let components: Vec<_> = given_rel.components().filter(|c| !matches!(c, std::path::Component::CurDir)).collect();
    if components.is_empty() || !components.iter().all(|c| matches!(c, std::path::Component::Normal(_))) {
        bail!("{arg}: give a path inside the app (relative to its root)");
    }
    // One spelling (`a/./b` → `a/b`), the one the records, review copies and report use.
    let rel_path: PathBuf = components.iter().collect();
    let rel = rel_path.to_string_lossy().to_string();
    let Some(rec) = read_pending(root, &rel_path) else {
        bail!("no pending review for {rel} — `mobiler upgrade` lists the files waiting for one");
    };
    let Ok(bytes) = fs::read(root.join(&rel_path)) else {
        bail!("{rel} doesn't exist — put your version in place first (or move its review copy in)");
    };
    if has_conflict_markers(&String::from_utf8_lossy(&bytes)) {
        bail!("{rel} still has conflict markers — finish resolving them, then run --resolved again");
    }
    Ok((rel_path, rec))
}

/// Every pending record under `.mobiler/pending/`: (the file's path, its kind), sorted.
fn list_pending(root: &Path) -> Vec<(String, String)> {
    fn walk(dir: &Path, base: &Path, root: &Path, out: &mut Vec<(String, String)>) {
        let Ok(entries) = fs::read_dir(dir) else { return };
        for e in entries.flatten() {
            let p = e.path();
            // Don't follow a symlink: a committed link must not walk the tree outside the records.
            if e.file_type().is_ok_and(|t| t.is_dir()) {
                walk(&p, base, root, out);
            } else if let Some(rel) = p.strip_prefix(base).ok().and_then(|r| r.to_str()).and_then(|r| r.strip_suffix(".json"))
                && let Some(rec) = read_pending(root, Path::new(rel))
            {
                out.push((rel.to_string(), rec.kind));
            }
        }
    }
    let base = root.join(PENDING_REL);
    let mut out = Vec::new();
    walk(&base, &base, root, &mut out);
    out.sort();
    out
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
    crate::fsguard::write(root, &backup, bytes)
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
    let rel = rel.replace('\\', "/");
    if is_android_resource(&rel) { format!(".mobiler/new/{rel}.mobiler-new") } else { format!("{rel}.mobiler-new") }
}

/// Write the review copy of `dst` (see [`review_rel`]).
fn write_review(app_root: Option<&Path>, dst: &Path, bytes: &[u8]) -> Result<()> {
    let Some((root, rel)) = app_root.and_then(|root| dst.strip_prefix(root).ok().map(|rel| (root, rel))) else {
        return write_sidecar(dst, "mobiler-new", bytes);
    };
    let side = root.join(review_rel(&rel.to_string_lossy()));
    crate::fsguard::write(root, &side, bytes)
}

/// Write `<dst>.<suffix>` next to `dst` (e.g. `Render.swift.mobiler-new`).
fn write_sidecar(dst: &Path, suffix: &str, bytes: &[u8]) -> Result<()> {
    let side = PathBuf::from(format!("{}.{suffix}", dst.display()));
    crate::fsguard::write(dst.parent().unwrap_or(Path::new(".")), &side, bytes)?;
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
    crate::fsguard::write(root, &path, format!("{}\n", env!("CARGO_PKG_VERSION")))?;
    Ok(prev)
}

fn write_stamp(root: &Path, report: &mut Report) -> Result<()> {
    let prev = write_version_stamp(root)?;
    report.stamp = Some((prev, env!("CARGO_PKG_VERSION").to_string()));
    Ok(())
}

// ---------------- report ----------------

impl Report {
    /// The end-of-run warning while anything waits for review (empty when nothing does).
    fn pending_lines(&self) -> Vec<String> {
        if self.pending.is_empty() {
            return Vec::new();
        }
        let mut lines = vec![format!("⚠ {} file(s) still need your review from an upgrade:", self.pending.len())];
        for (rel, kind) in &self.pending {
            let copy = review_rel(rel);
            let what = if self.markers.contains(rel) {
                format!("conflict markers left in the file — finish resolving, then `mobiler upgrade --resolved {rel}`")
            } else {
                match kind.as_str() {
                    "conflict" => format!(
                        "conflict — resolve {copy}, replace the file, then run `mobiler upgrade --resolved {rel}`"
                    ),
                    "merge" => format!(
                        "plugin/user state — merge {copy} by hand, then `mobiler upgrade --resolved {rel}`"
                    ),
                    "new" => format!("new file — move {copy} into place, or run with --apply"),
                    _ => format!(
                        "review {copy} and replace the file, run with --apply, or keep yours with `--resolved {rel}`"
                    ),
                }
            };
            lines.push(format!("    {rel}   {what}"));
        }
        lines.push("  Review copies are confirmed on the next run; a conflict when you run --resolved.".to_string());
        lines
    }

    /// One line per file this run touched, offered or resolved (`  + added …`, `  ‼ conflict …`).
    fn print_files(&self) {
        for d in &self.deps_would {
            println!("  deps: would change {d} (run with --apply)");
        }
        for d in &self.deps {
            println!("  deps: {d} (updated)");
        }
        for n in &self.deps_notes {
            println!("  deps: {n}");
        }
        if self.deps_would.is_empty() && self.deps.is_empty() && self.deps_notes.is_empty() {
            println!("  deps: up to date");
        }
        for a in &self.added {
            println!("  + added   {a}");
        }
        for n in &self.new_review {
            println!("  + new     {n}  -> {}", review_rel(n));
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
        for r in &self.resolved {
            println!("  ✓ resolved {r}  (baseline advanced; it stays resolved)");
        }
        for m in &self.markers {
            println!("  ‼ markers  {m}  (conflict markers left in the file)");
        }
        for k in &self.skipped {
            println!("  ! skipped  {k}  (a symlink at this path — upgrade doesn't write through it; replace it with a real file to get updates)");
        }
        for d in &self.dropped {
            println!("  - dropped  {d}  (no longer part of the template; its pending review was removed)");
        }
        for p in &self.plugins {
            println!("  ! plugin  {p}  has shell updates in this release");
        }
    }

    fn print(&self, apply: bool) {
        self.print_files();
        println!("  = {} file(s) up to date", self.up_to_date);
        if let Some((prev, cur)) = &self.stamp {
            let waiting = if self.pending.is_empty() {
                String::new()
            } else {
                format!(" ({} file(s) pending)", self.pending.len())
            };
            match prev {
                Some(p) if p != cur => println!("  stamp: {p} -> {cur}{waiting}"),
                _ => println!("  stamp: v{cur}{waiting}"),
            }
        }

        println!();
        for line in self.pending_lines() {
            println!("{line}");
        }
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
        let offered = self.changed.len() + self.new_review.len() + self.merge.len() + self.conflict.len();
        if offered == 0 && self.updated.is_empty() {
            if self.plugins.is_empty() && self.pending.is_empty() && self.deps_would.is_empty() && self.deps_notes.is_empty() && self.skipped.is_empty() {
                println!("Up to date. ✓");
            }
            return;
        }
        if !self.conflict.is_empty() {
            println!(
                "{} file(s) have overlapping edits — resolve the conflict markers in their review \
                 copy (path above), replace the original (never auto-applied), then run \
                 `mobiler upgrade --resolved <file>`.",
                self.conflict.len()
            );
        }
        let reviews = self.changed.len() + self.new_review.len();
        if reviews > 0 {
            if apply {
                // (in --apply mode `changed` is empty; shown only for completeness)
            } else {
                println!(
                    "Review the {reviews} .mobiler-new shell file(s) and merge, or re-run with `--apply` \
                     to overwrite in place (previous versions go to .mobiler/backup/)."
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

        // Resolving by hand is not enough: a conflict is only settled when the user says so.
        let pending = upgrade_at(&root, true).unwrap();
        assert!(pending.resolved.is_empty() && pending.conflict.iter().any(|c| c == "rust-toolchain.toml"));
        fs::remove_file(root.join("rust-toolchain.toml.mobiler-new")).unwrap();

        let r2 = upgrade_with(&root, true, &["rust-toolchain.toml"]).unwrap();
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

    #[test]
    fn review_run_never_touches_build_inputs() {
        let root = skeleton();
        let cargo_before = read(&root, "shared/Cargo.toml");
        let r = upgrade_at(&root, false).unwrap();
        assert_eq!(read(&root, "shared/Cargo.toml"), cargo_before, "no core bump without --apply");
        assert!(!r.deps_would.is_empty());
        assert!(!root.join("iOS/Sources/Render.swift").exists(), "a new file is not written in place");
        assert!(root.join("iOS/Sources/Render.swift.mobiler-new").exists(), "it is offered for review");
        assert!(r.pending.iter().any(|(f, k)| f == "iOS/Sources/Render.swift" && k == "new"), "{:?}", r.pending);

        let r2 = upgrade_at(&root, true).unwrap();
        assert!(root.join("iOS/Sources/Render.swift").exists());
        let want = template_pin("shared/Cargo.toml.tmpl", "mobiler-core").unwrap();
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

    #[test]
    fn review_offer_survives_an_unrelated_edit() {
        // A clean change offered by a plain run, then the file is touched for another reason
        // (plugin add, git pull, a formatter) before --apply: the change must still land.
        let root = skeleton();
        let t = toolchain();
        fs::create_dir_all(root.join(".mobiler/base")).unwrap();
        fs::write(root.join(".mobiler/base/rust-toolchain.toml"), format!("{t}# old tail\n")).unwrap();
        fs::write(root.join("rust-toolchain.toml"), format!("{t}# old tail\n")).unwrap();
        upgrade_at(&root, false).unwrap();
        assert!(read_pending(&root, Path::new("rust-toolchain.toml")).is_some_and(|r| r.kind == "review"));

        let edited = format!("{t}# old tail\n").replacen("[toolchain]", "[toolchain] # mine", 1);
        fs::write(root.join("rust-toolchain.toml"), &edited).unwrap();
        let r = upgrade_at(&root, true).unwrap();
        assert!(r.resolved.is_empty(), "an unrelated edit is not a resolution: {:?}", r.resolved);
        let after = read(&root, "rust-toolchain.toml");
        assert!(after.contains("[toolchain] # mine"), "the user's edit is kept");
        assert!(!after.contains("# old tail"), "the framework change landed: {after}");
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn conflict_with_an_edit_elsewhere_stays_pending() {
        // `plugin add` / a pull touches the file but not the conflict: still unresolved.
        let root = conflicted_app();
        upgrade_at(&root, true).unwrap();
        let edited = format!("# mine\n{}# added elsewhere\n", toolchain());
        fs::write(root.join("rust-toolchain.toml"), &edited).unwrap();
        let r = upgrade_at(&root, true).unwrap();
        assert!(r.resolved.is_empty(), "the conflict itself is untouched: {:?}", r.resolved);
        assert!(r.conflict.iter().any(|c| c == "rust-toolchain.toml"), "offered again: {:?}", r.conflict);
        assert_eq!(read(&root, ".mobiler/base/rust-toolchain.toml"), format!("# old\n{}", toolchain()), "baseline kept");
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn clean_offer_with_an_adjacent_edit_becomes_a_conflict() {
        // `plugin add` inserts right next to the offered change: report a conflict, never "resolved".
        let root = skeleton();
        let t = toolchain();
        fs::create_dir_all(root.join(".mobiler/base")).unwrap();
        fs::write(root.join(".mobiler/base/rust-toolchain.toml"), format!("{t}# old tail\n")).unwrap();
        fs::write(root.join("rust-toolchain.toml"), format!("{t}# old tail\n")).unwrap();
        upgrade_at(&root, false).unwrap();
        fs::write(root.join("rust-toolchain.toml"), format!("{t}# plugin line\n# old tail\n")).unwrap();
        let r = upgrade_at(&root, true).unwrap();
        assert!(r.resolved.is_empty(), "{:?}", r.resolved);
        assert!(r.conflict.iter().any(|c| c == "rust-toolchain.toml"), "{:?}", r.conflict);
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn accepted_review_copy_is_resolved() {
        let root = skeleton();
        let t = toolchain();
        fs::create_dir_all(root.join(".mobiler/base")).unwrap();
        fs::write(root.join(".mobiler/base/rust-toolchain.toml"), format!("{t}# old tail\n")).unwrap();
        fs::write(root.join("rust-toolchain.toml"), format!("{t}# old tail\n")).unwrap();
        upgrade_at(&root, false).unwrap();
        fs::rename(root.join("rust-toolchain.toml.mobiler-new"), root.join("rust-toolchain.toml")).unwrap();
        let r = upgrade_at(&root, false).unwrap();
        assert!(!r.pending.iter().any(|(f, _)| f == "rust-toolchain.toml"), "{:?}", r.pending);
        assert_eq!(read(&root, "rust-toolchain.toml"), t);
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn clearing_a_record_removes_its_empty_folders() {
        let root = skeleton();
        let offer = Offer { template: "x\n", current: b"", kind: "review", offered: b"" };
        write_pending(&root, Path::new("a/b/C.kt"), &offer).unwrap();
        write_pending(&root, Path::new("a/D.kt"), &offer).unwrap();
        clear_pending(&root, Path::new("a/b/C.kt"));
        assert!(!root.join(".mobiler/pending/a/b").exists(), "empty folder removed");
        assert!(root.join(".mobiler/pending/a/D.kt.json").exists(), "a folder still holding a record stays");
        let _ = fs::remove_dir_all(&root);
    }

    /// An app whose `channel` line conflicts (template `stable`, old baseline `beta`, user `nightly`),
    /// optionally with a second, clean template change on the `profile` line.
    fn channel_conflict_app(two_hunks: bool) -> PathBuf {
        let root = skeleton();
        let t = toolchain();
        let mut base = t.replace("channel = \"stable\"", "channel = \"beta\"");
        if two_hunks {
            base = base.replace("profile = \"minimal\"", "profile = \"default\"");
        }
        let user = base.replace("channel = \"beta\"", "channel = \"nightly\"");
        fs::create_dir_all(root.join(".mobiler/base")).unwrap();
        fs::write(root.join(".mobiler/base/rust-toolchain.toml"), &base).unwrap();
        fs::write(root.join("rust-toolchain.toml"), &user).unwrap();
        root
    }

    #[test]
    fn edit_next_to_a_pending_conflict_stays_pending() {
        let root = channel_conflict_app(false);
        upgrade_at(&root, true).unwrap();
        let now = read(&root, "rust-toolchain.toml").replace("[toolchain]", "[toolchain] # touched");
        fs::write(root.join("rust-toolchain.toml"), &now).unwrap();
        let r = upgrade_at(&root, true).unwrap();
        assert!(r.resolved.is_empty(), "the template's line is not in the file: {:?}", r.resolved);
        assert!(r.conflict.iter().any(|c| c == "rust-toolchain.toml"), "{:?}", r.conflict);
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn edit_next_to_a_clean_hunk_in_a_conflicted_file_stays_pending() {
        // plugin add inserting at an anchor beside a clean template change, while a conflict waits.
        let root = channel_conflict_app(true);
        upgrade_at(&root, true).unwrap();
        let now = read(&root, "rust-toolchain.toml").replace("profile = ", "# plugin line\nprofile = ");
        fs::write(root.join("rust-toolchain.toml"), &now).unwrap();
        let r = upgrade_at(&root, true).unwrap();
        assert!(r.resolved.is_empty(), "{:?}", r.resolved);
        assert!(read_pending(&root, Path::new("rust-toolchain.toml")).is_some(), "still pending");
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn conflict_resolved_by_hand_waits_for_resolved_flag() {
        let root = channel_conflict_app(true);
        upgrade_at(&root, true).unwrap();
        // The user resolves the review copy: the template's channel, their note, the clean change kept.
        let resolved = read(&root, "rust-toolchain.toml.mobiler-new")
            .lines()
            .filter(|l| !l.starts_with("<<<<<<<") && !l.starts_with("|||||||") && !l.starts_with(">>>>>>>") && *l != "=======")
            .filter(|l| !l.contains("beta") && !l.contains("nightly"))
            .fold(String::new(), |mut out, l| {
                out.push_str(l);
                out.push('\n');
                out
            })
            .replace("channel = \"stable\"\n", "channel = \"stable\" # wanted nightly\nchannel = \"stable\"\n");
        fs::write(root.join("rust-toolchain.toml"), &resolved).unwrap();
        fs::remove_file(root.join("rust-toolchain.toml.mobiler-new")).unwrap();
        // Every template line present, conflict lines changed: still not guessed as resolved.
        let r = upgrade_at(&root, true).unwrap();
        assert!(r.resolved.is_empty(), "never inferred: {:?}", r.resolved);
        assert!(r.pending.iter().any(|(f, k)| f == "rust-toolchain.toml" && k == "conflict"));
        let r2 = upgrade_with(&root, true, &["rust-toolchain.toml"]).unwrap();
        assert_eq!(r2.resolved, ["rust-toolchain.toml"], "file: {resolved}");
        assert_eq!(read(&root, "rust-toolchain.toml"), resolved, "the user's resolution is kept");
        assert!(upgrade_at(&root, true).unwrap().pending.is_empty(), "quiet afterwards");
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn keeping_your_own_side_resolves_with_the_flag() {
        // The user rejects the framework change: file unchanged, then --resolved. No loop afterwards.
        let root = conflicted_app();
        upgrade_at(&root, true).unwrap();
        let mine = read(&root, "rust-toolchain.toml");
        let r = upgrade_with(&root, true, &["rust-toolchain.toml"]).unwrap();
        assert_eq!(r.resolved, ["rust-toolchain.toml"]);
        assert!(r.conflict.is_empty(), "{:?}", r.conflict);
        assert_eq!(read(&root, "rust-toolchain.toml"), mine, "the user's side is kept");
        assert!(!root.join("rust-toolchain.toml.mobiler-new").exists(), "the review copy goes too");
        let again = upgrade_at(&root, true).unwrap();
        assert!(again.conflict.is_empty() && again.pending.is_empty(), "{:?}", again.pending);
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn resolved_flag_refuses_markers_and_unknown_files() {
        let root = conflicted_app();
        upgrade_at(&root, true).unwrap();
        let review = read(&root, "rust-toolchain.toml.mobiler-new");
        fs::write(root.join("rust-toolchain.toml"), &review).unwrap();
        let err = upgrade_with(&root, true, &["rust-toolchain.toml"]).err().expect("markers refused");
        assert!(format!("{err:#}").contains("conflict markers"), "{err:#}");
        let err = upgrade_with(&root, true, &["iOS/Sources/Render.swift"]).err().expect("not pending");
        assert!(format!("{err:#}").contains("no pending review"), "{err:#}");
        // The review copy's own path names the same file.
        fs::write(root.join("rust-toolchain.toml"), format!("# fixed\n{}", toolchain())).unwrap();
        let r = upgrade_with(&root, true, &["rust-toolchain.toml.mobiler-new"]).unwrap();
        assert_eq!(r.resolved, ["rust-toolchain.toml"]);
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn a_stub_where_a_new_file_was_offered_is_not_settled() {
        let root = skeleton();
        upgrade_at(&root, false).unwrap();
        fs::write(root.join("iOS/Sources/Render.swift"), "// my stub\n").unwrap();
        let r = upgrade_at(&root, false).unwrap();
        assert!(!r.resolved.iter().any(|f| f == "iOS/Sources/Render.swift"), "{:?}", r.resolved);
        assert!(r.pending.iter().any(|(f, _)| f == "iOS/Sources/Render.swift"), "still offered: {:?}", r.pending);
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn resolved_rejects_paths_outside_the_app() {
        let root = conflicted_app();
        upgrade_at(&root, true).unwrap();
        let outside = root.with_extension("outside");
        fs::create_dir_all(&outside).unwrap();
        let record = r#"{"template":"PWNED\n","file_hash":"0","kind":"conflict","offered_hash":"0"}"#;
        fs::write(outside.join("victim.json"), record).unwrap();
        let name = outside.file_name().unwrap().to_string_lossy().to_string();
        let up = format!("../../{name}/victim");
        let abs = outside.join("victim").to_string_lossy().to_string();
        for arg in [up.as_str(), abs.as_str()] {
            let err = upgrade_with(&root, true, &[arg]).err().expect("refused");
            assert!(format!("{err:#}").contains("inside the app"), "{err:#}");
        }
        assert!(!outside.join("victim").exists(), "nothing written outside the app");
        assert!(outside.join("victim.json").exists(), "nothing deleted outside the app");
        let _ = fs::remove_dir_all(&outside);
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn one_bad_resolved_argument_applies_none() {
        let root = conflicted_app();
        upgrade_at(&root, true).unwrap();
        assert!(upgrade_with(&root, true, &["rust-toolchain.toml", "typo/Nope.kt"]).is_err());
        assert!(read_pending(&root, Path::new("rust-toolchain.toml")).is_some(), "the good one is not half-applied");
        assert!(upgrade_with(&root, true, &["rust-toolchain.toml"]).is_ok(), "and a retry works");
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn repeated_resolved_arguments_settle_once() {
        let root = conflicted_app();
        upgrade_at(&root, true).unwrap();
        let r = upgrade_with(&root, true, &["rust-toolchain.toml", "./rust-toolchain.toml", "rust-toolchain.toml.mobiler-new"]).unwrap();
        assert_eq!(r.resolved, ["rust-toolchain.toml"]);
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn resolved_refuses_a_missing_file() {
        let root = conflicted_app();
        upgrade_at(&root, true).unwrap();
        fs::remove_file(root.join("rust-toolchain.toml")).unwrap();
        let err = upgrade_with(&root, true, &["rust-toolchain.toml"]).err().expect("refused");
        assert!(format!("{err:#}").contains("doesn't exist"), "{err:#}");
        let _ = fs::remove_dir_all(&root);
    }

    #[cfg(unix)]
    #[test]
    fn upgrade_never_writes_its_own_files_through_a_symlink() {
        // The version stamp (or any .mobiler/ side file) linked outside the app: refused, target untouched.
        let root = skeleton();
        let outside = root.with_extension("outside-stamp");
        fs::write(&outside, "MINE\n").unwrap();
        fs::create_dir_all(root.join(".mobiler")).unwrap();
        std::os::unix::fs::symlink(&outside, root.join(STAMP_REL)).unwrap();
        let err = upgrade_at(&root, true).err().expect("refused");
        assert!(format!("{err:#}").contains("symlink"), "{err:#}");
        assert_eq!(fs::read_to_string(&outside).unwrap(), "MINE\n");
        let _ = fs::remove_file(&outside);
        let _ = fs::remove_dir_all(&root);
    }

    #[cfg(unix)]
    #[test]
    fn upgrade_never_writes_into_a_symlinked_folder() {
        // `.mobiler/` linked outside the app: no stamp, baseline, backup or record lands there.
        let root = skeleton();
        let outside = root.with_extension("outside-mobiler");
        let _ = fs::remove_dir_all(&outside);
        fs::create_dir_all(&outside).unwrap();
        std::os::unix::fs::symlink(&outside, root.join(".mobiler")).unwrap();
        let err = upgrade_at(&root, true).err().expect("refused");
        assert!(format!("{err:#}").contains(".mobiler is a symlink"), "{err:#}");
        assert_eq!(fs::read_dir(&outside).unwrap().count(), 0, "nothing written through the link");
        let _ = fs::remove_dir_all(&outside);
        let _ = fs::remove_dir_all(&root);
    }

    #[cfg(unix)]
    #[test]
    fn a_live_symlink_at_a_managed_path_is_never_written_through() {
        // A symlink to a real file outside the app: upgrade must not read-merge it, overwrite its target,
        // or write a backup or review copy through it.
        let root = skeleton();
        let outside = root.with_extension("outside-toolchain");
        fs::write(&outside, "MINE\n").unwrap();
        let dst = root.join("rust-toolchain.toml");
        std::os::unix::fs::symlink(&outside, &dst).unwrap();
        for apply in [false, true] {
            let r = upgrade_at(&root, apply).unwrap();
            assert_eq!(fs::read_to_string(&outside).unwrap(), "MINE\n", "the link's target was changed (apply={apply})");
            assert!(r.skipped.iter().any(|s| s.contains("rust-toolchain.toml")), "{:?}", r.skipped);
            assert!(fs::symlink_metadata(&dst).unwrap().file_type().is_symlink(), "the link itself is left alone");
            assert!(!root.join("rust-toolchain.toml.mobiler-new").exists(), "no review copy");
        }
        let _ = fs::remove_file(&outside);
        let _ = fs::remove_dir_all(&root);
    }

    #[cfg(unix)]
    #[test]
    fn a_dangling_symlink_at_a_managed_path_is_skipped() {
        // A missing managed file is created — but not through a symlink that points nowhere.
        let root = skeleton();
        let outside = root.with_extension("outside-render");
        let _ = fs::remove_file(&outside);
        let dst = root.join("iOS/Sources/Render.swift");
        std::os::unix::fs::symlink(&outside, &dst).unwrap();
        let r = upgrade_at(&root, true).unwrap();
        assert!(!outside.exists(), "the write followed a symlink out of the project");
        assert!(r.skipped.iter().any(|s| s.contains("Render.swift")), "{:?}", r.skipped);
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn orphan_record_is_dropped() {
        // A record for a path the template no longer produces (dropped file, renamed package).
        let root = skeleton();
        upgrade_at(&root, true).unwrap();
        let offer = Offer { template: "x\n", current: b"", kind: "conflict", offered: b"" };
        write_pending(&root, Path::new("gone/Old.kt"), &offer).unwrap();
        let r = upgrade_at(&root, true).unwrap();
        assert_eq!(r.dropped, [Path::new("gone").join("Old.kt").to_string_lossy()]);
        assert!(r.pending.is_empty(), "{:?}", r.pending);
        assert!(!root.join(".mobiler/pending/gone/Old.kt.json").exists());
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn deleted_pending_file_is_not_resolved() {
        let root = conflicted_app();
        upgrade_at(&root, true).unwrap();
        fs::remove_file(root.join("rust-toolchain.toml")).unwrap();
        let r = upgrade_at(&root, true).unwrap();
        assert!(r.resolved.is_empty(), "{:?}", r.resolved);
        assert!(r.added.iter().any(|a| a == "rust-toolchain.toml"));
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn fingerprint_ignores_line_endings() {
        // A teammate's Windows checkout (core.autocrlf) is not a change.
        assert_eq!(fingerprint(b"a\r\nb\r\n"), fingerprint(b"a\nb\n"));
        assert_ne!(fingerprint(b"a\nb\n"), fingerprint(b"a\nc\n"));
    }

    #[test]
    fn markers_are_whole_lines() {
        assert!(has_conflict_markers("a\n<<<<<<< ours\nb\n=======\nc\n>>>>>>> theirs\n"));
        assert!(has_conflict_markers("x\n||||||| original\ny\n"), "a diff3 ancestor block is unresolved");
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
        // A Windows relative path (as the report holds it) maps to the same place the file is written.
        assert_eq!(review_rel("Android\\app\\src\\main\\res\\values\\themes.xml"), ".mobiler/new/Android/app/src/main/res/values/themes.xml.mobiler-new");
    }

    #[test]
    fn bumps_dep_stamps_and_leaves_app_code_untouched() {
        let root = skeleton();
        let want = template_pin("shared/Cargo.toml.tmpl", "mobiler-core").expect("templates pin mobiler-core");
        let report = upgrade_at(&root, true).unwrap();

        // dep bumped to the template's version, other deps preserved.
        let cargo = read(&root, "shared/Cargo.toml");
        assert!(cargo.contains(&format!("mobiler-core = \"{want}\"")), "core bumped");
        assert!(cargo.contains("serde = \"1\""), "other deps preserved");
        assert!(report.deps.contains(&format!("shared/Cargo.toml: mobiler-core 0.1.0 -> {want}")), "{:?}", report.deps);

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

    // ---------------- framework pins (ADR-0055) ----------------

    /// The manifests as the 0.69 template wrote them: crux 0.18, core 0.46, crux's bindgen feature,
    /// no codegen deps.
    fn as_069(root_cargo: &str, shared_cargo: &str) -> (String, String) {
        let crux = template_pin("Cargo.toml.tmpl", "crux_core").unwrap();
        let core = template_pin("shared/Cargo.toml.tmpl", "mobiler-core").unwrap();
        let root_old = root_cargo.replace(&format!("crux_core = \"{crux}\""), "crux_core = \"0.18.0\"");
        let shared_old = shared_cargo
            .replace(&format!("mobiler-core = \"{core}\""), "mobiler-core = \"0.46\"")
            .replace("    \"uniffi/bindgen\",\n    \"dep:cargo_metadata\",\n    \"dep:camino\",\n", "    \"crux_core/cli\",\n")
            .lines()
            .filter(|l| !l.starts_with("cargo_metadata = ") && !l.starts_with("camino = "))
            .fold(String::new(), |mut acc, l| {
                acc.push_str(l);
                acc.push('\n');
                acc
            });
        assert_ne!(root_old, root_cargo, "the template pins crux_core as a string");
        assert!(shared_old.contains("\"crux_core/cli\",") && shared_old.contains("mobiler-core = \"0.46\""));
        (root_old, shared_old)
    }

    /// An app on the current templates with its manifests rendered exactly as `mobiler new` writes them.
    fn app_with_template_manifests() -> (PathBuf, String, String) {
        let root = skeleton();
        let subs = Subs::from_app_root(&root).unwrap();
        let render = |t: &str| {
            crate::templating::substitute(TEMPLATES.get_file(t).unwrap().contents_utf8().unwrap(), &subs)
        };
        let (root_cargo, shared_cargo) = (render("Cargo.toml.tmpl"), render("shared/Cargo.toml.tmpl"));
        fs::write(root.join("Cargo.toml"), &root_cargo).unwrap();
        fs::write(root.join("shared/Cargo.toml"), &shared_cargo).unwrap();
        (root, root_cargo, shared_cargo)
    }

    fn app_069() -> (PathBuf, String, String) {
        let (root, root_cargo, shared_cargo) = app_with_template_manifests();
        let (root_old, shared_old) = as_069(&root_cargo, &shared_cargo);
        fs::write(root.join("Cargo.toml"), root_old).unwrap();
        fs::write(root.join("shared/Cargo.toml"), shared_old).unwrap();
        (root, root_cargo, shared_cargo)
    }

    const CODEGEN_069: &str = "codegen = [\n    \"crux_core/cli\",\n    \"dep:clap\",\n    \"uniffi\",\n]\n";
    const DEPS_069: &str = "[dependencies]\nclap = { version = \"4\", optional = true }\nuniffi = { version = \"=0.29.4\", optional = true }\n\n[dev-dependencies]\n";

    #[test]
    fn set_pin_moves_a_string_pin_and_is_a_no_op_at_target() {
        let c = "[workspace.dependencies]\nanyhow = \"1\"\ncrux_core = \"0.18.0\"\nserde = \"1\"\n";
        let Edit::Changed(out, what) = set_pin(c, "crux_core", "0.20.0") else { panic!("expected a change") };
        assert_eq!(out, c.replace("0.18.0", "0.20.0"));
        assert!(what.contains("0.18.0") && what.contains("0.20.0"), "{what}");
        assert_eq!(set_pin(&out, "crux_core", "0.20.0"), Edit::Unchanged);
    }

    #[test]
    fn set_pin_reports_a_table_form_pin() {
        let c = "[workspace.dependencies]\ncrux_core = { version = \"0.18.0\", features = [\"x\"] }\n";
        let Edit::Unrecognised(note) = set_pin(c, "crux_core", "0.20.0") else { panic!("expected a note") };
        assert!(note.contains("crux_core") && note.contains("0.20.0"), "{note}");
    }

    #[test]
    fn codegen_feature_migrates_the_multiline_form_once() {
        let Edit::Changed(out, _) = migrate_codegen_feature(CODEGEN_069) else { panic!("expected a change") };
        assert!(!out.contains("crux_core/cli"), "{out}");
        assert!(
            out.contains("    \"uniffi/bindgen\",\n    \"dep:cargo_metadata\",\n    \"dep:camino\",\n    \"dep:clap\",\n"),
            "{out}"
        );
        assert_eq!(migrate_codegen_feature(&out), Edit::Unchanged);
    }

    #[test]
    fn codegen_feature_on_one_line_is_reported() {
        let c = "codegen = [\"crux_core/cli\", \"uniffi\"]\n";
        assert!(matches!(migrate_codegen_feature(c), Edit::Unrecognised(_)));
    }

    #[test]
    fn codegen_deps_are_added_after_uniffi_once() {
        let Edit::Changed(out, _) = add_codegen_deps(DEPS_069) else { panic!("expected a change") };
        assert!(
            out.contains(
                "uniffi = { version = \"=0.29.4\", optional = true }\n\
                 cargo_metadata = { version = \"0.19\", optional = true }\n\
                 camino = { version = \"1\", optional = true }\n\n[dev-dependencies]"
            ),
            "{out}"
        );
        assert_eq!(add_codegen_deps(&out), Edit::Unchanged);
        assert!(matches!(add_codegen_deps("[dependencies]\nserde = \"1\"\n"), Edit::Unrecognised(_)));
    }

    #[test]
    fn crlf_manifests_keep_crlf() {
        let crlf = |s: &str| s.replace('\n', "\r\n");
        let only_crlf = |s: &str| !s.replace("\r\n", "").contains('\n');
        for edit in [
            set_pin(&crlf("[workspace.dependencies]\ncrux_core = \"0.18.0\"\n"), "crux_core", "0.20.0"),
            migrate_codegen_feature(&crlf(CODEGEN_069)),
            add_codegen_deps(&crlf(DEPS_069)),
        ] {
            let Edit::Changed(out, _) = edit else { panic!("expected a change") };
            assert!(only_crlf(&out), "{out:?}");
        }
    }

    #[test]
    fn upgrade_moves_a_069_app_and_rerun_is_a_no_op() {
        let (root, root_cargo, shared_cargo) = app_069();
        let r = upgrade_at(&root, true).unwrap();
        assert_eq!(r.deps.len(), 4, "{:?}", r.deps);
        assert_eq!(read(&root, "Cargo.toml"), root_cargo);
        assert_eq!(read(&root, "shared/Cargo.toml"), shared_cargo);
        assert!(r.deps_notes.is_empty(), "{:?}", r.deps_notes);
        let r2 = upgrade_at(&root, true).unwrap();
        assert!(r2.deps.is_empty() && r2.deps_would.is_empty(), "{:?} {:?}", r2.deps, r2.deps_would);
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn review_run_lists_manifest_edits_and_writes_nothing() {
        let (root, _, _) = app_069();
        let (root_before, shared_before) = (read(&root, "Cargo.toml"), read(&root, "shared/Cargo.toml"));
        let r = upgrade_at(&root, false).unwrap();
        assert_eq!(r.deps_would.len(), 4, "{:?}", r.deps_would);
        assert!(r.deps.is_empty());
        assert_eq!(read(&root, "Cargo.toml"), root_before);
        assert_eq!(read(&root, "shared/Cargo.toml"), shared_before);
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn other_crates_pins_are_reported_not_edited() {
        let (root, _, shared_cargo) = app_069();
        let app_core = "[package]\nname = \"app-core\"\n\n[dependencies]\ncrux_core = \"0.18.0\"\n";
        let web = "[package]\nname = \"web\"\n\n[dependencies]\nmobiler-web = \"0.46\"\n";
        fs::create_dir_all(root.join("app-core")).unwrap();
        fs::create_dir_all(root.join("web")).unwrap();
        fs::write(root.join("app-core/Cargo.toml"), app_core).unwrap();
        fs::write(root.join("web/Cargo.toml"), web).unwrap();
        let shared_old = read(&root, "shared/Cargo.toml").replace("[dependencies]\n", "[dependencies]\nfacet = \"0.44\"\n");
        fs::write(root.join("shared/Cargo.toml"), shared_old).unwrap();
        // Build output and a dot-dir are never scanned.
        fs::create_dir_all(root.join("target/x")).unwrap();
        fs::write(root.join("target/x/Cargo.toml"), app_core).unwrap();

        let r = upgrade_at(&root, true).unwrap();
        assert_eq!(r.deps_notes.len(), 3, "{:?}", r.deps_notes);
        assert!(r.deps_notes.iter().any(|n| n.starts_with("app-core/Cargo.toml: crux_core 0.18.0")), "{:?}", r.deps_notes);
        assert!(r.deps_notes.iter().any(|n| n.starts_with("web/Cargo.toml: mobiler-web 0.46")), "{:?}", r.deps_notes);
        assert!(r.deps_notes.iter().any(|n| n.starts_with("shared/Cargo.toml: declares facet 0.44")), "{:?}", r.deps_notes);
        assert_eq!(read(&root, "app-core/Cargo.toml"), app_core);
        assert_eq!(read(&root, "web/Cargo.toml"), web);
        assert_eq!(read(&root, "shared/Cargo.toml"), shared_cargo.replace("[dependencies]\n", "[dependencies]\nfacet = \"0.44\"\n"));
        let _ = fs::remove_dir_all(&root);
    }

    /// A crate outside the app folder, as the appointments repo has it: `shared` depends by path on
    /// `<outer>/crates/mobile-admin`, which depends by path on `domain`, which inherits mobiler-core
    /// from `<outer>/Cargo.toml`'s `[workspace.dependencies]`.
    fn app_with_outside_core(admin: &str, domain: &str, outer_ws: &str) -> (PathBuf, PathBuf) {
        let (root, _, _) = app_069();
        let outer = PathBuf::from(format!("{}_outer", root.display()));
        let _ = fs::remove_dir_all(&outer);
        fs::create_dir_all(outer.join("crates/mobile-admin/src")).unwrap();
        fs::create_dir_all(outer.join("crates/domain")).unwrap();
        fs::write(outer.join("Cargo.toml"), outer_ws).unwrap();
        fs::write(outer.join("crates/mobile-admin/Cargo.toml"), admin).unwrap();
        fs::write(outer.join("crates/domain/Cargo.toml"), domain).unwrap();
        let rel = format!("../../{}/crates/mobile-admin", outer.file_name().unwrap().to_string_lossy());
        let shared = read(&root, "shared/Cargo.toml")
            .replace("[dependencies]\n", &format!("[dependencies]\nmobile-admin = {{ path = \"{rel}\" }}\n"));
        fs::write(root.join("shared/Cargo.toml"), shared).unwrap();
        (root, outer)
    }

    const ADMIN_046: &str = "[package]\nname = \"mobile-admin\"\n\n[lib]\npath = \"src/lib.rs\"\n\n[dependencies]\n\
        mobiler-core = \"0.46\"\ndomain = { path = \"../domain\" }\n\n[dev-dependencies]\ncrux_core = \"0.18\"\n";
    // The dev-dependency back on mobile-admin closes a loop the walk must not follow forever.
    const DOMAIN_046: &str = "[package]\nname = \"domain\"\n\n[dependencies]\nmobiler-core.workspace = true\n\n\
        [dev-dependencies]\nmobile-admin = { path = \"../mobile-admin\" }\n";
    const OUTER_046: &str = "[workspace]\nmembers = [\"crates/*\"]\n\n[workspace.dependencies]\nmobiler-core = \"0.46\"\n";

    #[test]
    fn crates_outside_the_app_reached_by_path_are_reported_not_edited() {
        let (root, outer) = app_with_outside_core(ADMIN_046, DOMAIN_046, OUTER_046);
        let r = upgrade_at(&root, true).unwrap();
        let notes = &r.deps_notes;
        let has = |suffix: &str| notes.iter().any(|n| n.starts_with("../") && n.contains(suffix));
        // The template's current pin, so the test follows every core release.
        let core = template_pin("shared/Cargo.toml.tmpl", "mobiler-core").unwrap();
        assert!(has(&format!("/crates/mobile-admin/Cargo.toml: mobiler-core 0.46 -> {core} (edit by hand)")), "{notes:?}");
        assert!(has("/crates/mobile-admin/Cargo.toml: crux_core 0.18 -> 0.20.0 (edit by hand)"), "{notes:?}");
        assert!(has(&format!("_outer/Cargo.toml: mobiler-core 0.46 -> {core} (edit by hand)")), "{notes:?}");
        assert_eq!(notes.len(), 3, "{notes:?}");
        // Outside the app is read, never written.
        assert_eq!(fs::read_to_string(outer.join("crates/mobile-admin/Cargo.toml")).unwrap(), ADMIN_046);
        assert_eq!(fs::read_to_string(outer.join("crates/domain/Cargo.toml")).unwrap(), DOMAIN_046);
        assert_eq!(fs::read_to_string(outer.join("Cargo.toml")).unwrap(), OUTER_046);
        let _ = fs::remove_dir_all(&root);
        let _ = fs::remove_dir_all(&outer);
    }

    #[test]
    fn a_local_framework_checkout_is_not_judged() {
        // A [patch] to a local mobiler-core checkout whose workspace is a release ahead: the walk
        // must not tell the user to move the framework's own pins back to the CLI's.
        let (root, _, _) = app_with_template_manifests();
        let outer = PathBuf::from(format!("{}_fw", root.display()));
        let _ = fs::remove_dir_all(&outer);
        fs::create_dir_all(outer.join("mobiler-core")).unwrap();
        fs::write(outer.join("Cargo.toml"), "[workspace]\n\n[workspace.dependencies]\ncrux_core = \"0.21.0\"\nfacet = \"=0.47.0\"\n").unwrap();
        fs::write(
            outer.join("mobiler-core/Cargo.toml"),
            "[package]\nname = \"mobiler-core\"\n\n[dependencies]\ncrux_core.workspace = true\nfacet = \"=0.47.0\"\n",
        )
        .unwrap();
        let rel = format!("../{}/mobiler-core", outer.file_name().unwrap().to_string_lossy());
        let cargo = read(&root, "Cargo.toml") + &format!("\n[patch.crates-io]\nmobiler-core = {{ path = \"{rel}\" }}\n");
        fs::write(root.join("Cargo.toml"), cargo).unwrap();
        let r = upgrade_at(&root, true).unwrap();
        assert!(r.deps_notes.is_empty(), "{:?}", r.deps_notes);
        let _ = fs::remove_dir_all(&root);
        let _ = fs::remove_dir_all(&outer);
    }

    #[test]
    fn crates_outside_the_app_at_target_are_quiet() {
        let core = template_pin("shared/Cargo.toml.tmpl", "mobiler-core").unwrap();
        let admin = ADMIN_046.replace("\"0.46\"", &format!("\"{core}\"")).replace("\"0.18\"", "\"0.20\"");
        let (root, outer) = app_with_outside_core(&admin, DOMAIN_046, &OUTER_046.replace("0.46", &core));
        let r = upgrade_at(&root, true).unwrap();
        assert!(r.deps_notes.is_empty(), "{:?}", r.deps_notes);
        let _ = fs::remove_dir_all(&root);
        let _ = fs::remove_dir_all(&outer);
    }

    #[test]
    fn equivalent_pins_are_not_reported() {
        let (root, _, _) = app_with_template_manifests();
        fs::create_dir_all(root.join("app-core")).unwrap();
        fs::create_dir_all(root.join("web")).unwrap();
        fs::write(root.join("app-core/Cargo.toml"), "[dependencies]\ncrux_core = \"0.20\"\n").unwrap();
        let core = template_pin("shared/Cargo.toml.tmpl", "mobiler-core").unwrap();
        fs::write(root.join("web/Cargo.toml"), format!("[dependencies]\nmobiler-web = {{ version = \"{core}.1\" }}\n")).unwrap();
        let r = upgrade_at(&root, true).unwrap();
        assert!(r.deps_notes.is_empty(), "{:?}", r.deps_notes);
        assert_eq!(set_pin("crux_core = \"^0.20\"\n", "crux_core", "0.20.0"), Edit::Unchanged);
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn path_and_git_dependencies_are_left_alone() {
        let c = "[dependencies]\nmobiler-core = { path = \"../../mobiler-core\" }\n";
        assert_eq!(set_pin(c, "mobiler-core", "0.47"), Edit::Unchanged);
        let g = "[dependencies]\ncrux_core = { git = \"https://example.com/crux\" }\n";
        assert_eq!(set_pin(g, "crux_core", "0.20.0"), Edit::Unchanged);
        assert_eq!(set_pin("[dependencies]\ncrux_core.workspace = true\n", "crux_core", "0.20.0"), Edit::Unchanged);
    }
}
