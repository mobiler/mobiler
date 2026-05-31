//! `mobiler upgrade [--apply]` — bring a scaffolded app's generic native shells and its
//! `mobiler-core` dependency up to the CLI's current templates, without clobbering the user's
//! Rust app code or plugin-patched files.
//!
//! Existing apps record no baseline, so the command can't auto-distinguish a user's shell edits
//! from framework drift. The default is therefore **non-destructive**: a changed shell file is
//! written as `<file>.mobiler-new` for the user to review/merge. `--apply` overwrites in place
//! after saving `<file>.mobiler-bak`. Files carrying plugin/user state (`Core.kt`, manifests, …)
//! are only ever offered as `.mobiler-new` — never overwritten — since a blind copy would wipe
//! installed plugins.

use crate::templating::{Subs, is_binary, substitute, templated_path};
use anyhow::{Context, Result, bail};
use include_dir::{Dir, include_dir};
use std::fs;
use std::path::{Path, PathBuf};

static TEMPLATES: Dir<'_> = include_dir!("$CARGO_MANIFEST_DIR/templates");

/// Project-relative path of the version stamp written by `new` + `upgrade`.
pub(crate) const STAMP_REL: &str = ".mobiler/version";

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
        // A file the new version introduces — additive, safe to create.
        if let Some(parent) = dst.parent() {
            fs::create_dir_all(parent).with_context(|| format!("creating {}", parent.display()))?;
        }
        fs::write(&dst, &desired).with_context(|| format!("writing {}", dst.display()))?;
        report.added.push(rel_disp);
        return Ok(());
    }

    let current = fs::read(&dst).with_context(|| format!("reading {}", dst.display()))?;
    if current == desired {
        report.up_to_date += 1;
        return Ok(());
    }

    match class {
        Class::Merge => {
            write_sidecar(&dst, "mobiler-new", &desired)?;
            report.merge.push(rel_disp);
        }
        Class::Shell if apply => {
            write_sidecar(&dst, "mobiler-bak", &current)?;
            fs::write(&dst, &desired).with_context(|| format!("writing {}", dst.display()))?;
            report.updated.push(rel_disp);
        }
        Class::Shell => {
            write_sidecar(&dst, "mobiler-new", &desired)?;
            report.changed.push(rel_disp);
        }
        Class::Own => unreachable!("OWN files return early"),
    }
    Ok(())
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
        println!("  = {} file(s) up to date", self.up_to_date);
        if let Some((prev, cur)) = &self.stamp {
            match prev {
                Some(p) if p != cur => println!("  stamp: {p} -> {cur}"),
                _ => println!("  stamp: v{cur}"),
            }
        }

        println!();
        let pending = self.changed.len() + self.merge.len();
        if pending == 0 && self.updated.is_empty() {
            println!("Up to date. ✓");
            return;
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
    fn merge_file_never_overwritten_even_with_apply() {
        let root = skeleton();
        // Core.kt carries plugin state; its template has a `mobiler:plugins` anchor → MERGE.
        let core = root.join("Android/app/src/main/java/dev/mobiler/demo/Core.kt");
        fs::write(&core, "package dev.mobiler.demo\n// my installed plugins\n").unwrap();

        upgrade_at(&root, true).unwrap(); // even with --apply
        assert_eq!(
            fs::read_to_string(&core).unwrap(),
            "package dev.mobiler.demo\n// my installed plugins\n",
            "MERGE file is never overwritten"
        );
        assert!(
            root.join("Android/app/src/main/java/dev/mobiler/demo/Core.kt.mobiler-new").exists(),
            "MERGE file offered as .mobiler-new"
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
