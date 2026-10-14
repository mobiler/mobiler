//! `mobiler.toml` `[deep_links]`: turn the app's custom-scheme deep-link registration off or back on
//! (ADR-0051). The template ships the Android intent filter and the iOS `CFBundleURLTypes` between
//! `mobiler:deep-links` markers, switched on. `enabled = false` comments out every line between them;
//! `enabled = true` uncomments them. The CLI never writes the scheme itself, so a scheme the app set by
//! hand survives a round trip. No section: nothing is touched.

use std::fs;
use std::path::Path;

use anyhow::Context;
use serde::Deserialize;

pub(crate) const ANDROID_MANIFEST: &str = "Android/app/src/main/AndroidManifest.xml";
pub(crate) const PROJECT_YML: &str = "iOS/project.yml";

pub(crate) const XML_BEGIN: &str = "<!-- mobiler:deep-links-begin -->";
pub(crate) const XML_END: &str = "<!-- mobiler:deep-links-end -->";
pub(crate) const YML_BEGIN: &str = "# mobiler:deep-links-begin";
pub(crate) const YML_END: &str = "# mobiler:deep-links-end";

/// How a switched-off line is written. The `off:` tag tells our comments from the app's own.
const XML_OFF: (&str, &str) = ("<!-- off: ", " -->");
const YML_OFF: (&str, &str) = ("# off: ", "");

#[derive(Deserialize, Default)]
struct Manifest {
    deep_links: Option<Section>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Section {
    enabled: bool,
}

#[derive(Default, Debug)]
pub struct SyncReport {
    pub written: Vec<String>,
    pub warnings: Vec<String>,
    /// The config couldn't be read, or it couldn't be applied to every file, so nothing was written.
    pub failed: bool,
}

impl SyncReport {
    fn refuse(mut self, why: String) -> SyncReport {
        self.warnings.push(format!("{why} — nothing synced"));
        self.failed = true;
        self
    }
}

/// Sync `mobiler.toml` `[deep_links]` into the Android manifest and the iOS `project.yml`. All or
/// nothing: every file is checked before any is written, so a bad config, a file that can't be read, a
/// missing or broken marker block, or a line that would stay live writes nothing (`failed`).
pub fn sync(root: &Path) -> anyhow::Result<SyncReport> {
    let mut report = SyncReport::default();
    let Some(enabled) = read_enabled(root, &mut report)? else { return Ok(report) };
    let mut changes = Vec::new();
    for (rel, begin, end, off) in [(ANDROID_MANIFEST, XML_BEGIN, XML_END, XML_OFF), (PROJECT_YML, YML_BEGIN, YML_END, YML_OFF)] {
        let text = match fs::read_to_string(root.join(rel)) {
            Ok(t) => t,
            // An app without this platform has nothing to register there.
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => continue,
            Err(e) => return Ok(report.refuse(format!("{rel}: can't read it ({e})"))),
        };
        match toggle(&text, begin, end, off, enabled) {
            Ok(Some(new)) => {
                if !enabled && live_outside(&new, begin, end, off) {
                    report.warnings.push(format!(
                        "{rel} still registers a URL scheme outside the deep-link markers (the `oauth` plugin's redirect, say), \
                         so the app can still be opened by that scheme"
                    ));
                }
                if new != text {
                    changes.push((rel, new));
                }
            }
            Ok(None) if !enabled => {
                return Ok(report.refuse(format!(
                    "{rel} has no deep-link markers, so deep links can't be switched off — run `mobiler upgrade --apply` to get them"
                )));
            }
            Ok(None) => {}
            Err(e) => return Ok(report.refuse(format!("{rel}: {e}"))),
        }
    }
    for (rel, _) in &changes {
        crate::fsguard::check(root, &root.join(rel))?;
    }
    for (rel, new) in changes {
        crate::fsguard::write(root, root.join(rel), new)?;
        report.written.push(rel.to_string());
    }
    Ok(report)
}

/// Whether `text`, outside the marker block and its comments, still registers a URL scheme: any
/// `android:scheme` in the manifest (a VIEW filter resolves `am start -d x://` with or without
/// BROWSABLE), `CFBundleURLTypes` in `project.yml`.
fn live_outside(text: &str, begin: &str, end: &str, off: (&str, &str)) -> bool {
    let (Some(b), Some(e)) = (text.find(begin), text.find(end)) else { return false };
    let outside = format!("{}{}", &text[..b], &text[e..]);
    if off.1.is_empty() {
        outside.lines().any(|l| l.trim_start().starts_with("CFBundleURLTypes:"))
    } else {
        strip_xml_comments(&outside).contains("android:scheme=")
    }
}

/// `text` with every `<!-- … -->` removed (an unclosed comment runs to the end).
fn strip_xml_comments(text: &str) -> String {
    let mut out = String::new();
    let mut rest = text;
    while let Some(start) = rest.find("<!--") {
        out.push_str(&rest[..start]);
        match rest[start + 4..].find("-->") {
            Some(len) => rest = &rest[start + 4 + len + 3..],
            None => return out,
        }
    }
    out.push_str(rest);
    out
}

/// `Some(enabled)` from `[deep_links]`; `None` when there is no `mobiler.toml` or no section, or the
/// section is invalid (a warning).
fn read_enabled(root: &Path, report: &mut SyncReport) -> anyhow::Result<Option<bool>> {
    let path = root.join("mobiler.toml");
    if !path.exists() {
        return Ok(None);
    }
    let text = fs::read_to_string(&path).with_context(|| format!("reading {}", path.display()))?;
    match toml::from_str::<Manifest>(&text) {
        Ok(m) => Ok(m.deep_links.map(|s| s.enabled)),
        Err(e) => {
            report.warnings.push(format!("mobiler.toml [deep_links]: {} — nothing synced", e.message()));
            report.failed = true;
            Ok(None)
        }
    }
}

/// `text` with the lines between `begin` and `end` switched on or off; `None` if the markers are absent.
/// A switched-off line is the original line's indent, then `off.0`, the rest of the line, `off.1`. Lines
/// that are already comments (the app's own, or already off) and blank lines are left as they are.
fn toggle(text: &str, begin: &str, end: &str, off: (&str, &str), on: bool) -> Result<Option<String>, &'static str> {
    let lines: Vec<&str> = text.split_inclusive('\n').collect();
    let begins: Vec<usize> = lines.iter().enumerate().filter(|(_, l)| l.contains(begin)).map(|(i, _)| i).collect();
    let ends: Vec<usize> = lines.iter().enumerate().filter(|(_, l)| l.contains(end)).map(|(i, _)| i).collect();
    let (b, e) = match (begins.as_slice(), ends.as_slice()) {
        ([], []) => return Ok(None),
        ([b], [e]) if e > b => (*b, *e),
        _ => return Err("unbalanced deep-link marker lines (fix them by hand)"),
    };
    // Anything else on a marker line would never be switched.
    if lines[b].trim() != begin || lines[e].trim() != end {
        return Err("a deep-link marker line holds something besides the marker (move it onto its own line)");
    }
    let comment = if off.1.is_empty() { "#" } else { "<!--" };
    let mut out = String::with_capacity(text.len() + 64);
    for (i, line) in lines.iter().enumerate() {
        if i <= b || i >= e {
            out.push_str(line);
            continue;
        }
        let body = line.trim_end_matches(['\n', '\r']);
        let eol = &line[body.len()..];
        let rest = body.trim_start();
        let indent = &body[..body.len() - rest.len()];
        let switched_off = rest.strip_prefix(off.0).and_then(|r| r.strip_suffix(off.1));
        match (on, switched_off) {
            (true, Some(inner)) => out.push_str(&format!("{indent}{inner}{eol}")),
            (false, None) if !rest.is_empty() && !rest.starts_with(comment) => {
                // An XML comment can't hold `--`; such a line would end the comment early.
                if !off.1.is_empty() && rest.contains("--") {
                    return Err("a line between the deep-link markers contains `--`, which can't be commented out");
                }
                out.push_str(&format!("{indent}{}{rest}{}{eol}", off.0, off.1));
            }
            _ => out.push_str(line),
        }
    }
    if !on {
        // Off must leave nothing live between the markers, e.g. markup after a comment on one line.
        let block: String = out.split_inclusive('\n').skip(b + 1).take(e - b - 1).collect();
        let live = if off.1.is_empty() {
            block.lines().any(|l| !l.trim().is_empty() && !l.trim_start().starts_with('#'))
        } else {
            !strip_xml_comments(&block).trim().is_empty()
        };
        if live {
            return Err("a line between the deep-link markers mixes a comment with markup, so it can't be switched off (split it)");
        }
    }
    Ok(Some(out))
}

/// `mobiler deep-links sync` — run the sync from the app root and print what happened. Fails (non-zero
/// exit) when the config couldn't be applied, so a CI step can gate on it.
pub fn run_sync_cli() -> anyhow::Result<()> {
    let root = std::env::current_dir().context("reading current directory")?;
    let report = sync(&root)?;
    for w in &report.warnings {
        eprintln!("warning: deep links: {w}");
    }
    for p in &report.written {
        println!("  wrote   {p}");
    }
    // A switch that exists to close an entry point must not look applied when it wasn't.
    if report.failed {
        anyhow::bail!("[deep_links] not applied (see the warnings above)");
    }
    if report.written.is_empty() && report.warnings.is_empty() {
        let mut ignored = SyncReport::default();
        match read_enabled(&root, &mut ignored)? {
            Some(true) => println!("Deep links on, up to date."),
            Some(false) => println!("Deep links off, up to date."),
            None => println!("No [deep_links] in mobiler.toml — the deep-link registration is the app's own to edit."),
        }
    }
    Ok(())
}

/// Called at the start of `mobiler build` / `dev` / `watch`: sync and print warnings, never fail.
pub fn sync_for_build(root: &Path) {
    match sync(root) {
        Ok(r) => {
            for w in r.warnings {
                eprintln!("warning: deep links: {w}");
            }
        }
        Err(e) => eprintln!("warning: deep links: {e:#} — deep links not synced"),
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    static COUNTER: AtomicUsize = AtomicUsize::new(0);

    /// An app root holding the two template files, rendered for `dev.test.links`.
    fn app(toml: Option<&str>) -> std::path::PathBuf {
        let n = COUNTER.fetch_add(1, Ordering::SeqCst);
        let root = std::env::temp_dir().join(format!("mob_deeplinks_test_{}_{n}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        for rel in [ANDROID_MANIFEST, PROJECT_YML] {
            let path = root.join(rel);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(&path, template(rel)).unwrap();
        }
        if let Some(t) = toml {
            fs::write(root.join("mobiler.toml"), t).unwrap();
        }
        root
    }

    fn template(rel: &str) -> String {
        let f = crate::upgrade::TEMPLATES.get_file(rel).unwrap().contents_utf8().unwrap();
        f.replace("{{PACKAGE}}", "dev.test.links").replace("{{NAME}}", "Links")
    }

    fn read(root: &Path, rel: &str) -> String {
        fs::read_to_string(root.join(rel)).unwrap()
    }

    const OFF: &str = "[deep_links]\nenabled = false\n";
    const ON: &str = "[deep_links]\nenabled = true\n";

    #[test]
    fn off_comments_out_both_registrations_and_on_restores_them_exactly() {
        let root = app(Some(OFF));
        let report = sync(&root).unwrap();
        assert_eq!(report.written, vec![ANDROID_MANIFEST, PROJECT_YML], "{report:?}");
        let manifest = read(&root, ANDROID_MANIFEST);
        assert!(!manifest.lines().any(|l| l.trim_start().starts_with("<intent-filter android:autoVerify")), "{manifest}");
        assert!(manifest.contains("<!-- off: <data android:scheme=\"dev.test.links\" /> -->"), "{manifest}");
        // The launcher filter stays.
        assert!(manifest.contains("android.intent.category.LAUNCHER"));
        let yml = read(&root, PROJECT_YML);
        assert!(!yml.lines().any(|l| l.trim_start().starts_with("CFBundleURLTypes:")), "{yml}");
        assert!(yml.contains("# off: CFBundleURLTypes:"), "{yml}");

        fs::write(root.join("mobiler.toml"), ON).unwrap();
        sync(&root).unwrap();
        assert_eq!(read(&root, ANDROID_MANIFEST), template(ANDROID_MANIFEST));
        assert_eq!(read(&root, PROJECT_YML), template(PROJECT_YML));
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn a_second_sync_changes_nothing() {
        for toml in [OFF, ON] {
            let root = app(Some(toml));
            sync(&root).unwrap();
            let before = (read(&root, ANDROID_MANIFEST), read(&root, PROJECT_YML));
            let report = sync(&root).unwrap();
            assert!(report.written.is_empty() && report.warnings.is_empty(), "{report:?}");
            assert_eq!((read(&root, ANDROID_MANIFEST), read(&root, PROJECT_YML)), before);
            let _ = fs::remove_dir_all(&root);
        }
    }

    #[test]
    fn a_hand_set_scheme_survives_off_and_on() {
        let root = app(Some(OFF));
        let custom = read(&root, PROJECT_YML).replace("              - dev.test.links", "              - si.example.custom");
        fs::write(root.join(PROJECT_YML), &custom).unwrap();
        sync(&root).unwrap();
        assert!(read(&root, PROJECT_YML).contains("# off: - si.example.custom"));
        fs::write(root.join("mobiler.toml"), ON).unwrap();
        sync(&root).unwrap();
        assert_eq!(read(&root, PROJECT_YML), custom);
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn no_section_touches_nothing() {
        for toml in [None, Some("[splash]\nbackground = \"#FFFFFF\"\n")] {
            let root = app(toml);
            fs::write(root.join(ANDROID_MANIFEST), "<manifest>hand edited</manifest>\n").unwrap();
            let report = sync(&root).unwrap();
            assert!(report.written.is_empty() && report.warnings.is_empty(), "{report:?}");
            assert_eq!(read(&root, ANDROID_MANIFEST), "<manifest>hand edited</manifest>\n");
            assert_eq!(read(&root, PROJECT_YML), template(PROJECT_YML));
            let _ = fs::remove_dir_all(&root);
        }
    }

    #[test]
    fn missing_markers_refuse_and_edit_nothing() {
        let root = app(Some(OFF));
        let old: String = template(ANDROID_MANIFEST).lines().filter(|l| !l.contains("mobiler:deep-links")).map(|l| format!("{l}\n")).collect();
        fs::write(root.join(ANDROID_MANIFEST), &old).unwrap();
        let report = sync(&root).unwrap();
        assert!(report.failed, "{report:?}");
        assert!(report.warnings.iter().any(|w| w.contains(ANDROID_MANIFEST) && w.contains("mobiler upgrade --apply")), "{report:?}");
        // All or nothing: iOS, which has its markers, stays on too.
        assert!(report.written.is_empty(), "{report:?}");
        assert_eq!(read(&root, ANDROID_MANIFEST), old);
        assert_eq!(read(&root, PROJECT_YML), template(PROJECT_YML));
        // Turned on, a file without markers is simply left on.
        fs::write(root.join("mobiler.toml"), ON).unwrap();
        let report = sync(&root).unwrap();
        assert!(!report.failed && report.warnings.is_empty() && report.written.is_empty(), "{report:?}");
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn a_line_that_would_stay_live_writes_nothing() {
        let markup_after_comment = template(ANDROID_MANIFEST).replace(
            "                <category android:name=\"android.intent.category.BROWSABLE\" />",
            "                <!-- why --><category android:name=\"android.intent.category.BROWSABLE\" />",
        );
        let markup_on_marker = template(ANDROID_MANIFEST).replace(
            &format!("{XML_END}\n"),
            &format!("{XML_END}<intent-filter><data android:scheme=\"x\" /></intent-filter>\n"),
        );
        let unreadable = [0xff_u8, 0xfe, 0x00].to_vec();
        for (name, manifest) in [("markup after comment", markup_after_comment.into_bytes()), ("markup on marker", markup_on_marker.into_bytes()), ("not UTF-8", unreadable)] {
            let root = app(Some(OFF));
            fs::write(root.join(ANDROID_MANIFEST), &manifest).unwrap();
            let report = sync(&root).unwrap();
            assert!(report.failed && report.written.is_empty(), "{name}: {report:?}");
            assert_eq!(fs::read(root.join(ANDROID_MANIFEST)).unwrap(), manifest, "{name}");
            assert_eq!(read(&root, PROJECT_YML), template(PROJECT_YML), "{name}");
            let _ = fs::remove_dir_all(&root);
        }
    }

    #[test]
    fn a_scheme_registered_outside_the_markers_is_reported() {
        let root = app(Some(OFF));
        // What `mobiler plugin add oauth` inserts at the manifest-application anchor.
        let oauth = template(ANDROID_MANIFEST).replace(
            "        <!-- mobiler:manifest-application",
            "        <activity android:name=\".OAuthActivity\" android:exported=\"true\"><intent-filter><action android:name=\"android.intent.action.VIEW\" /><category android:name=\"android.intent.category.BROWSABLE\" /><data android:scheme=\"${applicationId}\" /></intent-filter></activity>\n        <!-- mobiler:manifest-application",
        );
        assert_ne!(oauth, template(ANDROID_MANIFEST), "the template lost its manifest-application anchor");
        fs::write(root.join(ANDROID_MANIFEST), &oauth).unwrap();
        let report = sync(&root).unwrap();
        assert!(!report.failed && report.written.len() == 2, "{report:?}");
        assert!(report.warnings.iter().any(|w| w.contains(ANDROID_MANIFEST) && w.contains("oauth")), "{report:?}");
        // The stock template has none outside, so no warning there.
        let clean = app(Some(OFF));
        assert!(sync(&clean).unwrap().warnings.is_empty());
        let _ = fs::remove_dir_all(&root);
        let _ = fs::remove_dir_all(&clean);
    }

    #[test]
    fn a_broken_block_or_config_writes_nothing() {
        let root = app(Some(OFF));
        let broken = template(PROJECT_YML).replace(YML_END, "# (end marker deleted)");
        fs::write(root.join(PROJECT_YML), &broken).unwrap();
        let report = sync(&root).unwrap();
        assert!(report.written.is_empty(), "{report:?}");
        assert_eq!(read(&root, ANDROID_MANIFEST), template(ANDROID_MANIFEST));
        assert!(report.failed && report.warnings.iter().any(|w| w.contains("unbalanced")), "{report:?}");

        fs::write(root.join(PROJECT_YML), template(PROJECT_YML)).unwrap();
        fs::write(root.join("mobiler.toml"), "[deep_links]\nenable = false\n").unwrap();
        let report = sync(&root).unwrap();
        assert!(report.written.is_empty(), "{report:?}");
        assert!(report.failed && report.warnings.iter().any(|w| w.contains("[deep_links]")), "{report:?}");
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn the_apps_own_comments_in_the_block_are_kept() {
        let root = app(Some(OFF));
        let xml_note = template(ANDROID_MANIFEST)
            .replace(&format!("{XML_BEGIN}\n"), &format!("{XML_BEGIN}\n            <!-- our note -->\n"));
        let yml_note = template(PROJECT_YML).replace(&format!("{YML_BEGIN}\n"), &format!("{YML_BEGIN}\n        # our note\n"));
        fs::write(root.join(ANDROID_MANIFEST), &xml_note).unwrap();
        fs::write(root.join(PROJECT_YML), &yml_note).unwrap();
        let report = sync(&root).unwrap();
        // Switched off, with each note left exactly as written.
        assert_eq!(report.written, vec![ANDROID_MANIFEST, PROJECT_YML], "{report:?}");
        assert!(read(&root, ANDROID_MANIFEST).contains("\n            <!-- our note -->\n            <!-- off: <intent-filter"));
        assert!(read(&root, PROJECT_YML).contains("\n        # our note\n        # off: CFBundleURLTypes:"));
        fs::write(root.join("mobiler.toml"), ON).unwrap();
        sync(&root).unwrap();
        assert_eq!(read(&root, ANDROID_MANIFEST), xml_note);
        assert_eq!(read(&root, PROJECT_YML), yml_note);
        let _ = fs::remove_dir_all(&root);
    }
}
