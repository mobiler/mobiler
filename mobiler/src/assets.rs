//! `mobiler.toml` `[assets]`: one app folder whose files ship inside every shell, shown with
//! `image("asset:<path>", …)` (the `asset:` scheme is ADR-0069; this sync is ADR-0070).
//!
//! The sync only fills marker blocks: in `Android/app/build.gradle.kts` (a Gradle `Sync` task copying the
//! folder into the APK's assets under `mobiler-assets/`), in `iOS/project.yml` (the folder as a resource
//! folder reference, and `MobilerAssetsFolder` in Info.plist naming it) and in `web/index.html` (a trunk
//! `copy-dir` to `mobiler-assets/`). The folder itself is the app's: the sync never writes, moves or
//! deletes anything in it, and removing `[assets]` only empties the blocks.

use std::fs;
use std::path::{Component, Path, PathBuf};

use anyhow::Context;
use serde::Deserialize;

/// The folder when `[assets]` has no `dir`.
pub const DEFAULT_DIR: &str = "assets";

/// `[assets]` as written in mobiler.toml.
#[derive(Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct AssetsSpec {
    #[serde(default = "default_dir")]
    pub dir: String,
}

fn default_dir() -> String {
    DEFAULT_DIR.into()
}

pub(crate) const GRADLE: &str = "Android/app/build.gradle.kts";
pub(crate) const PROJECT_YML: &str = "iOS/project.yml";
pub(crate) const WEB_INDEX: &str = "web/index.html";

const GRADLE_BEGIN: &str = "// mobiler:assets-begin";
const GRADLE_END: &str = "// mobiler:assets-end";
const SOURCES_BEGIN: &str = "# mobiler:assets-sources-begin";
const SOURCES_END: &str = "# mobiler:assets-sources-end";
const INFO_BEGIN: &str = "# mobiler:assets-info-begin";
const INFO_END: &str = "# mobiler:assets-info-end";
const WEB_BEGIN: &str = "<!-- mobiler:assets-begin -->";
const WEB_END: &str = "<!-- mobiler:assets-end -->";

#[derive(Debug, Default)]
pub struct SyncReport {
    pub written: Vec<String>,
    pub warnings: Vec<String>,
    /// The config couldn't be read or applied, so nothing was written.
    pub failed: bool,
}

/// `dir` as the generated text uses it: trailing `/` dropped, then checked to be a plain relative path
/// (no empty, `.` or `..` segment) made only of characters every block can carry unquoted (letters,
/// digits, space inside a name, `-`, `_`, `.`, `+`).
fn clean_dir(dir: &str) -> Result<&str, String> {
    let d = dir.trim_end_matches('/');
    let bad = |why: &str| Err(format!("[assets] dir {dir:?}: {why}"));
    if d.is_empty() {
        return bad("must name a folder inside the app (e.g. \"assets\")");
    }
    if d.starts_with('/') {
        return bad("must be relative to the app root, not absolute");
    }
    for seg in d.split('/') {
        if seg.is_empty() || seg == "." || seg == ".." {
            return bad("no empty, `.` or `..` segment (the folder must be inside the app)");
        }
        if seg.starts_with(' ') || seg.ends_with(' ') {
            return bad("a folder name can't start or end with a space");
        }
        if let Some(c) = seg.chars().find(|c| !(c.is_alphanumeric() || " -_.+".contains(*c))) {
            return bad(&format!("{c:?} isn't allowed (use letters, digits, space, - _ . +)"));
        }
    }
    Ok(d)
}

/// Check `[assets] dir` and return the folder's path: inside the app root, no symlink on the way (or the
/// folder itself), and a folder if it exists. A missing folder is fine here (the sync warns).
pub fn validate(root: &Path, spec: &AssetsSpec) -> Result<PathBuf, String> {
    let d = clean_dir(&spec.dir)?;
    let path = root.join(d);
    if !path.components().skip(root.components().count()).all(|c| matches!(c, Component::Normal(_))) {
        return Err(format!("[assets] dir {:?}: must be a folder inside the app", spec.dir));
    }
    crate::fsguard::check(root, &path).map_err(|e| format!("[assets] dir {:?}: {e:#}", spec.dir))?;
    if path.exists() && !path.is_dir() {
        return Err(format!("[assets] dir {:?}: is a file, not a folder", spec.dir));
    }
    Ok(path)
}

/// The folder's last segment: iOS's bundle folder takes it (a folder reference can't be renamed).
fn basename(dir: &str) -> &str {
    dir.rsplit('/').next().unwrap_or(dir)
}

/// A YAML scalar for `name`: plain unless YAML would read it as something other than a string (a number,
/// a boolean, null) or it has a space. `clean_dir` already kept `"` and `\` out.
fn yaml_string(name: &str) -> String {
    let lower = name.to_ascii_lowercase();
    let keyword = ["true", "false", "yes", "no", "on", "off", "y", "n", "null", "~"].contains(&lower.as_str());
    let numeric = name.chars().all(|c| c.is_ascii_digit() || "._+-".contains(c));
    if keyword || numeric || name.contains(' ') || name.starts_with(['-', '.']) {
        format!("\"{name}\"")
    } else {
        name.to_string()
    }
}

/// The lines between the `mobiler:assets` markers in `Android/app/build.gradle.kts`: copy the folder into
/// a generated assets root under `mobiler-assets/` before every build (`Sync` also prunes deleted files).
pub fn gradle_lines(dir: Option<&str>) -> Vec<String> {
    let Some(dir) = dir else { return Vec::new() };
    vec![
        r#"val mobilerAssets = tasks.register<Sync>("mobilerAssets") {"#.into(),
        format!(r#"    from(rootProject.file("../{dir}"))"#),
        r#"    into(layout.buildDirectory.dir("generated/mobiler-assets/mobiler-assets"))"#.into(),
        "}".into(),
        r#"android.sourceSets["main"].assets.srcDir(layout.buildDirectory.dir("generated/mobiler-assets"))"#.into(),
        r#"tasks.named("preBuild") { dependsOn(mobilerAssets) }"#.into(),
    ]
}

/// The lines between the `mobiler:assets-sources` markers in `iOS/project.yml`: the folder as a folder
/// reference copied into the bundle.
pub fn yml_source_lines(dir: Option<&str>) -> Vec<String> {
    let Some(dir) = dir else { return Vec::new() };
    vec![format!("- path: ../{dir}"), "  type: folder".into(), "  buildPhase: resources".into()]
}

/// The lines between the `mobiler:assets-info` markers in `iOS/project.yml`: the bundle folder's name.
pub fn yml_info_lines(dir: Option<&str>) -> Vec<String> {
    let Some(dir) = dir else { return Vec::new() };
    vec![format!("MobilerAssetsFolder: {}", yaml_string(basename(dir)))]
}

/// The lines between the `mobiler:assets` markers in `web/index.html`: trunk copies the folder to
/// `mobiler-assets/`.
pub fn web_lines(dir: Option<&str>) -> Vec<String> {
    let Some(dir) = dir else { return Vec::new() };
    vec![format!(r#"<link data-trunk rel="copy-dir" href="../{dir}" data-target-path="mobiler-assets"/>"#)]
}

#[derive(Deserialize, Default)]
struct Manifest {
    assets: Option<AssetsSpec>,
}

/// `[splash]` only, read with splash's own spec (fonts' `Manifest` is public; splash's isn't).
#[derive(Deserialize, Default)]
struct SplashOnly {
    splash: Option<crate::splash::SplashSpec>,
}

/// `Ok(None)`: no mobiler.toml or no `[assets]`.
fn read_spec(root: &Path) -> Result<Option<AssetsSpec>, String> {
    let path = root.join("mobiler.toml");
    let text = match fs::read_to_string(&path) {
        Ok(t) => t,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(format!("can't read mobiler.toml ({e})")),
    };
    toml::from_str::<Manifest>(&text).map(|m| m.assets).map_err(|e| format!("mobiler.toml [assets] doesn't parse: {}", e.message()))
}

/// Sync `[assets]` into the shells' marker blocks (ADR-0070). No section: the blocks are emptied. An
/// invalid `dir`: a warning, `failed`, nothing written. Files without the markers are never edited.
pub fn sync(root: &Path) -> anyhow::Result<SyncReport> {
    let mut report = SyncReport::default();
    let spec = match read_spec(root) {
        Ok(s) => s,
        Err(e) => {
            report.warnings.push(format!("{e} — nothing synced"));
            report.failed = true;
            return Ok(report);
        }
    };
    let dir = match &spec {
        None => None,
        Some(spec) => match validate(root, spec) {
            Ok(path) => {
                if !path.exists() {
                    report.warnings.push(format!(
                        "[assets] dir {:?} doesn't exist yet — the shells are set up for it; add your files there",
                        spec.dir
                    ));
                }
                source_overlap_warnings(root, &spec.dir, &mut report);
                Some(clean_dir(&spec.dir).expect("validated").to_string())
            }
            Err(e) => {
                report.warnings.push(format!("{e} — nothing synced"));
                report.failed = true;
                return Ok(report);
            }
        },
    };
    let dir = dir.as_deref();

    let mut writes: Vec<(&str, String)> = Vec::new();
    if let Some(text) = read_if_exists(root, GRADLE, &mut report)? {
        let pairs = [(GRADLE_BEGIN, GRADLE_END, "assets", gradle_lines(dir))];
        if let Some(t) = fill(GRADLE, &text, &pairs, &mut report) {
            writes.push((GRADLE, t));
        }
    }
    if let Some(text) = read_if_exists(root, PROJECT_YML, &mut report)? {
        let pairs = [
            (SOURCES_BEGIN, SOURCES_END, "assets-sources", yml_source_lines(dir)),
            (INFO_BEGIN, INFO_END, "assets-info", yml_info_lines(dir)),
        ];
        if let Some(t) = fill(PROJECT_YML, &text, &pairs, &mut report) {
            writes.push((PROJECT_YML, t));
        }
    }
    // web/index.html isn't a template file (no web shell is scaffolded): the block is inserted before
    // `</head>` when missing, like the fonts and splash blocks. No section and no block: left alone.
    if let Some(html) = read_if_exists(root, WEB_INDEX, &mut report)?
        && (dir.is_some() || html.contains(WEB_BEGIN))
    {
        let lines = web_lines(dir);
        match crate::fonts::replace_block(&html, WEB_BEGIN, WEB_END, "</head>", |indent| {
            lines.iter().map(|l| format!("{indent}{l}")).collect()
        }) {
            Ok(t) => writes.push((WEB_INDEX, t)),
            Err(e) => report.warnings.push(format!("{WEB_INDEX}: {e} — web assets not synced")),
        }
    }

    // All or nothing: no symlink on the way to any target, checked before the first write.
    for (rel, _) in &writes {
        crate::fsguard::check(root, &root.join(rel))?;
    }
    for (rel, text) in writes {
        let path = root.join(rel);
        if fs::read(&path).is_ok_and(|old| old == text.as_bytes()) {
            continue;
        }
        crate::fsguard::write(root, &path, text)?;
        report.written.push(rel.to_string());
    }
    Ok(report)
}

/// A shell file's text; `None` when the app has no such shell. A symlinked file is never read through.
fn read_if_exists(root: &Path, rel: &str, report: &mut SyncReport) -> anyhow::Result<Option<String>> {
    let path = root.join(rel);
    if path.symlink_metadata().is_err() {
        return Ok(None);
    }
    crate::fsguard::check(root, &path)?;
    match fs::read_to_string(&path) {
        Ok(t) => Ok(Some(t)),
        Err(e) => {
            report.warnings.push(format!("{rel}: can't read it ({e}) — not synced"));
            Ok(None)
        }
    }
}

/// Fill each marker pair in a template file. A missing pair is never inserted: it gets a warning when
/// there is something to put in it. `None` when nothing is to be written.
fn fill(rel: &str, text: &str, pairs: &[(&str, &str, &str, Vec<String>)], report: &mut SyncReport) -> Option<String> {
    let mut out = text.to_string();
    for (begin, end, name, lines) in pairs {
        if !out.contains(begin) || !out.contains(end) {
            if !lines.is_empty() {
                report.warnings.push(format!(
                    "{rel} has no mobiler:{name} markers — run `mobiler upgrade --apply` to get them (the bundled files aren't in that shell until then)"
                ));
            }
            continue;
        }
        match crate::fonts::replace_block(&out, begin, end, begin, |indent| lines.iter().map(|l| format!("{indent}{l}")).collect()) {
            Ok(t) => out = t,
            Err(e) => {
                report.warnings.push(format!("{rel}: mobiler:{name}: {e} — not synced"));
                return None;
            }
        }
    }
    (out != text).then_some(out)
}

/// Whether the relative path `file` lies inside the relative folder `dir` (compared by segment, `.` ignored).
fn under(file: &str, dir: &str) -> bool {
    let segs = |p: &str| -> Vec<String> {
        Path::new(p)
            .components()
            .filter(|c| !matches!(c, Component::CurDir))
            .map(|c| c.as_os_str().to_string_lossy().into_owned())
            .collect()
    };
    let (f, d) = (segs(file), segs(dir));
    !d.is_empty() && f.len() > d.len() && f.starts_with(&d)
}

/// A `[fonts]` or `[splash]` source inside `dir` would ship inside the app a second time: warn, naming
/// the file and its section (the sync still runs). `[icon]` joins this list with the icon sync.
fn source_overlap_warnings(root: &Path, dir: &str, report: &mut SyncReport) {
    let Ok(text) = fs::read_to_string(root.join("mobiler.toml")) else { return };
    let mut sources: Vec<(&str, String)> = Vec::new();
    if let Some(fonts) = toml::from_str::<crate::fonts::Manifest>(&text).ok().and_then(|m| m.fonts) {
        for role in [fonts.display, fonts.body].into_iter().flatten() {
            sources.extend(role.files.into_iter().chain(role.license).map(|f| ("[fonts]", f)));
        }
    }
    if let Some(splash) = toml::from_str::<SplashOnly>(&text).ok().and_then(|m| m.splash) {
        sources.extend(splash.logo.into_iter().chain(splash.logo_dark).map(|f| ("[splash]", f)));
    }
    for (section, file) in sources {
        if under(&file, dir) {
            report.warnings.push(format!(
                "{section} source {file} is inside [assets] dir {dir:?}, so it is bundled into the app as well — keep sources outside it (e.g. dir = \"assets/bundled\")"
            ));
        }
    }
}

// ---------------- CLI ----------------

/// `mobiler assets sync` — run the sync from the app root and print what happened. Fails (non-zero exit)
/// when the config couldn't be applied.
pub fn run_sync_cli() -> anyhow::Result<()> {
    let root = std::env::current_dir().context("reading current directory")?;
    let report = sync(&root)?;
    for w in &report.warnings {
        eprintln!("warning: assets: {w}");
    }
    for p in &report.written {
        println!("  wrote   {p}");
    }
    if report.failed {
        anyhow::bail!("[assets] not applied (see the warnings above)");
    }
    if report.written.is_empty() {
        match read_spec(&root) {
            Ok(Some(s)) => println!("Assets up to date ({}).", s.dir),
            _ => println!("No [assets] in mobiler.toml — no bundled files."),
        }
    }
    Ok(())
}

/// Called at the start of `mobiler build` / `dev` / `watch`: sync and print warnings, never fail.
pub fn sync_for_build(root: &Path) {
    match sync(root) {
        Ok(r) => {
            for w in r.warnings {
                eprintln!("warning: assets: {w}");
            }
        }
        Err(e) => eprintln!("warning: assets: {e:#} — assets not synced"),
    }
}

/// What `mobiler watch` should also watch for `[assets]`: the folder (a changed file triggers a rebuild,
/// which copies it) and `mobiler.toml`. An invalid `dir` is never watched.
pub fn watch_paths(root: &Path) -> (Vec<PathBuf>, Vec<PathBuf>) {
    let dirs = match read_spec(root) {
        Ok(Some(spec)) => validate(root, &spec).into_iter().collect(),
        _ => Vec::new(),
    };
    (dirs, vec![root.join("mobiler.toml")])
}

/// Regular files under `dir` (symlinks not followed).
fn count_files(dir: &Path) -> usize {
    let Ok(entries) = fs::read_dir(dir) else { return 0 };
    entries
        .flatten()
        .map(|e| match e.file_type() {
            Ok(t) if t.is_dir() => count_files(&e.path()),
            Ok(t) if t.is_file() => 1,
            _ => 0,
        })
        .sum()
}

/// One `mobiler doctor` line about `[assets]`.
pub fn doctor_line(root: &Path) -> String {
    match read_spec(root) {
        Ok(None) => "assets: not set (no bundled files)".into(),
        Ok(Some(spec)) => match validate(root, &spec) {
            Ok(path) if path.is_dir() => format!("assets: {} ({} files)", spec.dir, count_files(&path)),
            Ok(_) => format!("assets: {} (folder missing)", spec.dir),
            Err(e) => format!("assets: invalid: {e}"),
        },
        Err(e) => format!("assets: invalid: {e}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;
    use std::fs;
    use std::sync::atomic::{AtomicUsize, Ordering};

    static N: AtomicUsize = AtomicUsize::new(0);

    const GRADLE_T: &str = "android {\n}\n\n// mobiler:assets-begin\n// mobiler:assets-end\n\ndependencies {\n}\n";
    const PROJECT_T: &str = "targets:\n  Demo:\n    sources:\n      - Sources\n      # mobiler:assets-sources-begin\n      # mobiler:assets-sources-end\n    info:\n      properties:\n        # mobiler:assets-info-begin\n        # mobiler:assets-info-end\n        UIRequiresFullScreen: true\n";
    const WEB_T: &str = "<!DOCTYPE html>\n<html>\n<head>\n<title>x</title>\n</head>\n<body></body>\n</html>\n";

    fn tmp() -> PathBuf {
        let root = std::env::temp_dir().join(format!("mob_assets_{}_{}", std::process::id(), N.fetch_add(1, Ordering::SeqCst)));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();
        root
    }

    /// An app's assets-relevant files: the template's marker pairs and a web shell without a block.
    fn app(toml: &str) -> PathBuf {
        let root = tmp();
        put(&root, GRADLE, GRADLE_T.as_bytes());
        put(&root, PROJECT_YML, PROJECT_T.as_bytes());
        put(&root, WEB_INDEX, WEB_T.as_bytes());
        put(&root, "mobiler.toml", toml.as_bytes());
        root
    }

    fn put(root: &Path, rel: &str, bytes: &[u8]) {
        let p = root.join(rel);
        fs::create_dir_all(p.parent().unwrap()).unwrap();
        fs::write(p, bytes).unwrap();
    }

    fn read(root: &Path, rel: &str) -> String {
        fs::read_to_string(root.join(rel)).unwrap()
    }

    /// The lines strictly between `begin` and `end`, without the begin line's indent.
    fn block(text: &str, begin: &str, end: &str) -> Vec<String> {
        let lines: Vec<&str> = text.lines().collect();
        let b = lines.iter().position(|l| l.contains(begin)).unwrap_or_else(|| panic!("no {begin}"));
        let e = lines.iter().position(|l| l.contains(end)).unwrap_or_else(|| panic!("no {end}"));
        let indent = &lines[b][..lines[b].len() - lines[b].trim_start().len()];
        lines[b + 1..e].iter().map(|l| l.strip_prefix(indent).unwrap_or(l).to_string()).collect()
    }

    fn mtimes(root: &Path, rels: &[&str]) -> Vec<std::time::SystemTime> {
        rels.iter().map(|r| fs::metadata(root.join(r)).unwrap().modified().unwrap()).collect()
    }

    #[test]
    fn generated_blocks_match_barbershop() {
        // Barbershop's hand-filled blocks build on every shell: the generators must emit exactly them.
        let demo = Path::new(env!("CARGO_MANIFEST_DIR")).join("../demos/barbershop");
        let gradle = fs::read_to_string(demo.join(GRADLE)).unwrap();
        let yml = fs::read_to_string(demo.join(PROJECT_YML)).unwrap();
        let web = fs::read_to_string(demo.join(WEB_INDEX)).unwrap();
        let dir = Some("assets/bundled");
        assert_eq!(gradle_lines(dir), block(&gradle, GRADLE_BEGIN, GRADLE_END));
        assert_eq!(yml_source_lines(dir), block(&yml, SOURCES_BEGIN, SOURCES_END));
        assert_eq!(yml_info_lines(dir), block(&yml, INFO_BEGIN, INFO_END));
        assert_eq!(web_lines(dir), block(&web, WEB_BEGIN, WEB_END));
        for empty in [gradle_lines(None), yml_source_lines(None), yml_info_lines(None), web_lines(None)] {
            assert!(empty.is_empty(), "{empty:?}");
        }
        // The default folder: `MobilerAssetsFolder` is the dir's last segment.
        assert_eq!(yml_info_lines(Some("assets")), ["MobilerAssetsFolder: assets"]);
        assert_eq!(yml_source_lines(Some("assets"))[0], "- path: ../assets");
    }

    #[test]
    fn folder_names_yaml_would_retype_are_quoted() {
        assert_eq!(yml_info_lines(Some("img/2024")), ["MobilerAssetsFolder: \"2024\""]);
        assert_eq!(yml_info_lines(Some("yes")), ["MobilerAssetsFolder: \"yes\""]);
        assert_eq!(yml_info_lines(Some("my images")), ["MobilerAssetsFolder: \"my images\""]);
        assert_eq!(yml_info_lines(Some("slike-č")), ["MobilerAssetsFolder: slike-č"]);
    }

    #[test]
    fn fills_template_blocks_and_inserts_web_block() {
        let root = app("[assets]\n");
        fs::create_dir_all(root.join("assets")).unwrap();
        let r = sync(&root).unwrap();
        assert!(r.warnings.is_empty() && !r.failed, "{r:?}");
        let gradle = read(&root, GRADLE);
        assert_eq!(block(&gradle, GRADLE_BEGIN, GRADLE_END), gradle_lines(Some("assets")));
        assert!(gradle.contains("    from(rootProject.file(\"../assets\"))\n"), "{gradle}");
        let yml = read(&root, PROJECT_YML);
        assert!(yml.contains("      # mobiler:assets-sources-begin\n      - path: ../assets\n        type: folder\n"), "{yml}");
        assert!(yml.contains("        # mobiler:assets-info-begin\n        MobilerAssetsFolder: assets\n"), "{yml}");
        let html = read(&root, WEB_INDEX);
        assert!(html.contains("<!-- mobiler:assets-begin -->\n<link data-trunk rel=\"copy-dir\" href=\"../assets\" data-target-path=\"mobiler-assets\"/>\n<!-- mobiler:assets-end -->\n</head>"), "{html}");
        assert_eq!(r.written.len(), 3, "{r:?}");
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn missing_markers_warn_and_never_edit() {
        let root = app("[assets]\n");
        fs::create_dir_all(root.join("assets")).unwrap();
        let bare = "android {\n}\n\ndependencies {\n}\n";
        put(&root, GRADLE, bare.as_bytes());
        let r = sync(&root).unwrap();
        assert_eq!(read(&root, GRADLE), bare, "a framework file without markers is never edited");
        assert!(r.warnings.iter().any(|w| w.contains(GRADLE) && w.contains("mobiler upgrade --apply")), "{:?}", r.warnings);
        assert!(read(&root, PROJECT_YML).contains("MobilerAssetsFolder: assets"), "the other shells still sync");
        // project.yml with only one of its two pairs: that pair is filled, the other one warned about.
        let one_pair = "targets:\n  Demo:\n    sources:\n      # mobiler:assets-sources-begin\n      # mobiler:assets-sources-end\n";
        put(&root, PROJECT_YML, one_pair.as_bytes());
        let r = sync(&root).unwrap();
        assert!(read(&root, PROJECT_YML).contains("- path: ../assets"));
        assert!(r.warnings.iter().any(|w| w.contains(PROJECT_YML) && w.contains("assets-info") && w.contains("mobiler upgrade --apply")), "{:?}", r.warnings);
        // No section: no markers is fine, nothing to say.
        put(&root, "mobiler.toml", b"");
        assert!(sync(&root).unwrap().warnings.is_empty());
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn removing_section_empties_blocks_and_keeps_dir() {
        let root = app("[assets]\n");
        put(&root, "assets/x.png", b"png");
        sync(&root).unwrap();
        put(&root, "mobiler.toml", b"[fonts]\n");
        let r = sync(&root).unwrap();
        assert!(r.warnings.is_empty(), "{r:?}");
        assert_eq!(read(&root, GRADLE), GRADLE_T, "the gradle block is empty again");
        assert_eq!(read(&root, PROJECT_YML), PROJECT_T, "both project.yml blocks are empty again");
        let html = read(&root, WEB_INDEX);
        assert!(block(&html, WEB_BEGIN, WEB_END).is_empty(), "{html}");
        assert_eq!(fs::read(root.join("assets/x.png")).unwrap(), b"png", "the app's folder is never touched");
        // Without mobiler.toml at all: the same.
        fs::remove_file(root.join("mobiler.toml")).unwrap();
        assert!(sync(&root).unwrap().written.is_empty());
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn rejects_dir_outside_root() {
        let mut bad = vec!["../x", "/abs", "a/../b", "./assets", "a//b", "", "a\\b", "a\"b", "$x", "C:/x", " a"];
        let root = app("");
        #[cfg(unix)]
        {
            let outside = tmp();
            std::os::unix::fs::symlink(&outside, root.join("linked")).unwrap();
            fs::create_dir_all(root.join("real")).unwrap();
            std::os::unix::fs::symlink(&outside, root.join("real/inner")).unwrap();
            bad.extend(["linked", "linked/sub", "real/inner"]);
        }
        for dir in bad {
            assert!(validate(&root, &AssetsSpec { dir: dir.into() }).is_err(), "{dir:?} accepted");
            put(&root, "mobiler.toml", format!("[assets]\ndir = {}\n", toml::Value::String(dir.into())).as_bytes());
            let r = sync(&root).unwrap();
            assert!(r.failed && r.written.is_empty(), "{dir:?}: {r:?}");
            assert!(r.warnings.iter().any(|w| w.contains("[assets]")), "{dir:?}: {r:?}");
            assert_eq!(read(&root, GRADLE), GRADLE_T, "{dir:?}: nothing written");
            assert_eq!(read(&root, PROJECT_YML), PROJECT_T);
            assert_eq!(read(&root, WEB_INDEX), WEB_T);
        }
        fs::create_dir_all(root.join("real/ok")).unwrap();
        assert_eq!(validate(&root, &AssetsSpec { dir: "real/ok".into() }).unwrap(), root.join("real/ok"));
        assert_eq!(validate(&root, &AssetsSpec { dir: "real/ok/".into() }).unwrap(), root.join("real/ok"));
        // A file where the folder should be.
        put(&root, "afile", b"x");
        assert!(validate(&root, &AssetsSpec { dir: "afile".into() }).is_err());
        // An unknown key is a typo, not ignored.
        put(&root, "mobiler.toml", b"[assets]\ndirs = \"x\"\n");
        assert!(sync(&root).unwrap().failed);
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn missing_dir_warns_but_writes_blocks() {
        let root = app("[assets]\ndir = \"media/bundled\"\n");
        let r = sync(&root).unwrap();
        assert!(!r.failed, "{r:?}");
        assert!(r.warnings.iter().any(|w| w.contains("media/bundled") && w.contains("doesn't exist")), "{:?}", r.warnings);
        assert!(read(&root, GRADLE).contains("../media/bundled"));
        assert!(read(&root, PROJECT_YML).contains("MobilerAssetsFolder: bundled"));
        assert!(read(&root, WEB_INDEX).contains("href=\"../media/bundled\""));
        assert!(!root.join("media").exists(), "the sync never creates the folder");
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn second_sync_is_a_no_op() {
        let root = app("[assets]\n");
        put(&root, "assets/x.png", b"png");
        sync(&root).unwrap();
        let files = [GRADLE, PROJECT_YML, WEB_INDEX];
        let before = mtimes(&root, &files);
        let texts: Vec<String> = files.iter().map(|f| read(&root, f)).collect();
        std::thread::sleep(std::time::Duration::from_millis(20));
        let r = sync(&root).unwrap();
        assert!(r.written.is_empty() && r.warnings.is_empty(), "{r:?}");
        assert_eq!(mtimes(&root, &files), before, "nothing rewritten");
        assert_eq!(files.iter().map(|f| read(&root, f)).collect::<Vec<_>>(), texts);
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn crlf_files_keep_their_line_endings() {
        let root = app("[assets]\n");
        fs::create_dir_all(root.join("assets")).unwrap();
        put(&root, GRADLE, GRADLE_T.replace('\n', "\r\n").as_bytes());
        sync(&root).unwrap();
        let g = read(&root, GRADLE);
        assert!(g.contains("../assets") && !g.replace("\r\n", "").contains('\n'), "{g:?}");
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn fonts_or_splash_source_under_dir_warns() {
        let toml = "[fonts]\nbody = { files = [\"assets/fonts/b400.ttf\"], license = \"assets/fonts/OFL.txt\" }\ndisplay = { files = [\"art/d.ttf\"] }\n[splash]\nbackground = \"#FFFFFF\"\nlogo = \"assets/splash/logo.png\"\nlogo_dark = \"./assets/splash/dark.png\"\n[assets]\n";
        let root = app(toml);
        fs::create_dir_all(root.join("assets")).unwrap();
        let r = sync(&root).unwrap();
        for (file, section) in [
            ("assets/fonts/b400.ttf", "[fonts]"),
            ("assets/fonts/OFL.txt", "[fonts]"),
            ("assets/splash/logo.png", "[splash]"),
            ("assets/splash/dark.png", "[splash]"),
        ] {
            assert!(r.warnings.iter().any(|w| w.contains(file) && w.contains(section)), "{file}: {:?}", r.warnings);
        }
        assert!(!r.warnings.iter().any(|w| w.contains("art/d.ttf")), "{:?}", r.warnings);
        assert!(read(&root, GRADLE).contains("../assets"), "it still syncs");
        // A sibling folder that only shares a prefix isn't "under" dir.
        put(&root, "mobiler.toml", toml.replace("[assets]\n", "[assets]\ndir = \"asset\"\n").as_bytes());
        fs::create_dir_all(root.join("asset")).unwrap();
        assert!(sync(&root).unwrap().warnings.is_empty());
        // Barbershop's layout: sources in assets/, bundled images in assets/bundled — no warning.
        put(&root, "mobiler.toml", toml.replace("[assets]\n", "[assets]\ndir = \"assets/bundled\"\n").as_bytes());
        fs::create_dir_all(root.join("assets/bundled")).unwrap();
        assert!(sync(&root).unwrap().warnings.is_empty());
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn watch_and_doctor() {
        let root = app("[assets]\ndir = \"assets/bundled\"\n");
        put(&root, "assets/bundled/a.png", b"a");
        put(&root, "assets/bundled/sub/b.png", b"b");
        let (dirs, files) = watch_paths(&root);
        assert_eq!(dirs, [root.join("assets/bundled")]);
        assert_eq!(files, [root.join("mobiler.toml")]);
        assert!(doctor_line(&root).contains("assets/bundled") && doctor_line(&root).contains("2 files"), "{}", doctor_line(&root));
        put(&root, "mobiler.toml", b"");
        assert!(watch_paths(&root).0.is_empty());
        assert!(doctor_line(&root).contains("not set"), "{}", doctor_line(&root));
        put(&root, "mobiler.toml", b"[assets]\ndir = \"../x\"\n");
        assert!(watch_paths(&root).0.is_empty(), "never watches outside the app");
        assert!(doctor_line(&root).contains("invalid"), "{}", doctor_line(&root));
        let _ = fs::remove_dir_all(&root);
    }

    /// The upgrade planner, on a scaffolded app that uses `[assets]`, plans nothing under the folder
    /// (it isn't a template path) and leaves the folder byte-for-byte as it was.
    #[test]
    fn upgrade_never_touches_dir() {
        let root = tmp();
        crate::upgrade::test_support::scaffold(&root);
        put(&root, "mobiler.toml", b"[assets]\n");
        // Files that look like template paths, inside the folder.
        for rel in ["assets/x.png", "assets/Android/app/build.gradle.kts", "assets/iOS/project.yml", "assets/rust-toolchain.toml"] {
            put(&root, rel, b"mine");
        }
        let r = sync(&root).unwrap();
        assert!(r.warnings.is_empty() && r.written.len() == 2, "{r:?}");
        let snapshot = |root: &Path| -> BTreeMap<PathBuf, (Vec<u8>, std::time::SystemTime)> {
            fn walk(d: &Path, out: &mut BTreeMap<PathBuf, (Vec<u8>, std::time::SystemTime)>) {
                for e in fs::read_dir(d).unwrap().flatten() {
                    let p = e.path();
                    if p.is_dir() {
                        walk(&p, out);
                    } else {
                        out.insert(p.clone(), (fs::read(&p).unwrap(), fs::metadata(&p).unwrap().modified().unwrap()));
                    }
                }
            }
            let mut out = BTreeMap::new();
            walk(&root.join("assets"), &mut out);
            out
        };
        let before = snapshot(&root);
        for apply in [false, true] {
            let touched = crate::upgrade::test_support::touched(&root, apply);
            assert!(touched.iter().all(|p| !p.starts_with("assets")), "apply={apply}: {touched:?}");
            assert!(!touched.iter().any(|p| p == GRADLE || p == PROJECT_YML), "filled blocks are no pending change: {touched:?}");
            assert_eq!(snapshot(&root), before, "apply={apply}: the folder changed");
        }
        assert!(read(&root, GRADLE).contains("../assets"), "the filled block survives the upgrade");
        let _ = fs::remove_dir_all(&root);
    }
}
