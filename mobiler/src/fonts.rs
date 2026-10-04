//! `mobiler.toml` `[fonts]`: the app's display + body font files, synced into the Android, iOS and web
//! shells so `FontFamily::Custom` can render them (see `sync`).

use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::fs;
use std::path::Path;

use anyhow::Context;
use serde::Deserialize;

/// `mobiler.toml` — today only `[fonts]`.
#[derive(Deserialize, Default, Debug)]
pub struct Manifest {
    pub fonts: Option<FontsSection>,
}

/// The two font roles: `display` (titles, subtitles, bar + sheet titles) and `body` (everything else).
#[derive(Deserialize, Default, Debug)]
pub struct FontsSection {
    pub display: Option<RoleSpec>,
    pub body: Option<RoleSpec>,
}

/// One role's files (paths relative to the app root) and an optional family name (else the first
/// file's own family name).
#[derive(Deserialize, Default, Debug, Clone)]
pub struct RoleSpec {
    pub family: Option<String>,
    #[serde(default)]
    pub files: Vec<String>,
    /// The font's licence file, copied next to the web fonts (`web/fonts/mobiler-<role>-LICENSE.txt`) —
    /// e.g. the SIL OFL, which asks that the licence travel with the fonts.
    pub license: Option<String>,
}

/// What a font file says about itself.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FontInfo {
    pub family: String,
    pub weight: u16,
}

fn be16(b: &[u8], at: usize) -> Result<u16, String> {
    b.get(at..at + 2).map(|s| u16::from_be_bytes([s[0], s[1]])).ok_or_else(|| "truncated font file".to_string())
}

fn be32(b: &[u8], at: usize) -> Result<u32, String> {
    b.get(at..at + 4).map(|s| u32::from_be_bytes([s[0], s[1], s[2], s[3]])).ok_or_else(|| "truncated font file".to_string())
}

/// Read a TrueType/OpenType file's weight (OS/2 `usWeightClass`) and family (name ID 16, else 1).
/// Every read is bounds-checked: bad input is an `Err` with a human reason, never a panic.
pub fn read_font_info(bytes: &[u8]) -> Result<FontInfo, String> {
    match be32(bytes, 0) {
        Ok(0x0001_0000 | 0x7472_7565 /* true */ | 0x4F54_544F /* OTTO */) => {}
        _ => return Err("not a TrueType/OpenType file (woff/woff2 aren't supported natively)".into()),
    }
    let num_tables = usize::from(be16(bytes, 4)?);
    let (mut os2, mut name) = (None, None);
    for i in 0..num_tables {
        let rec = 12 + 16 * i;
        let tag = bytes.get(rec..rec + 4).ok_or("truncated font file")?;
        let (offset, length) = (be32(bytes, rec + 8)? as usize, be32(bytes, rec + 12)? as usize);
        let table = bytes.get(offset..offset.checked_add(length).ok_or("truncated font file")?).ok_or("truncated font file")?;
        match tag {
            b"OS/2" => os2 = Some(table),
            b"name" => name = Some(table),
            _ => {}
        }
    }
    let (Some(os2), Some(name)) = (os2, name) else { return Err("no OS/2/name table".into()) };
    let weight = be16(os2, 4)?;
    let family = family_name(name)?;
    Ok(FontInfo { family, weight })
}

/// The typographic family (name ID 16) if present, else the family (name ID 1); Windows UTF-16BE or
/// Mac Roman/ASCII records.
fn family_name(name: &[u8]) -> Result<String, String> {
    let count = usize::from(be16(name, 2)?);
    let strings = usize::from(be16(name, 4)?);
    // Per slot [id16, id1]: the best record so far, ranked Windows-English (0) < Windows/Unicode (1) <
    // Mac (2), so a localized record can't win by coming first in the table.
    let mut found: [Option<(u8, String)>; 2] = [None, None];
    for i in 0..count {
        let r = 6 + 12 * i;
        let (platform, lang, name_id) = (be16(name, r)?, be16(name, r + 4)?, be16(name, r + 6)?);
        let slot = match name_id {
            16 => 0,
            1 => 1,
            _ => continue,
        };
        let rank = match (platform, lang) {
            (3, 0x409) => 0,
            (0 | 3, _) => 1,
            _ => 2,
        };
        if found[slot].as_ref().is_some_and(|(best, _)| *best <= rank) {
            continue;
        }
        let (len, off) = (usize::from(be16(name, r + 8)?), usize::from(be16(name, r + 10)?));
        let raw = name.get(strings + off..strings + off + len).ok_or("truncated font file")?;
        let text = match platform {
            0 | 3 => String::from_utf16_lossy(&raw.chunks_exact(2).map(|c| u16::from_be_bytes([c[0], c[1]])).collect::<Vec<_>>()),
            1 => raw.iter().map(|&b| char::from(b)).collect(),
            _ => continue,
        };
        if !text.trim().is_empty() {
            found[slot] = Some((rank, text.trim().to_string()));
        }
    }
    let [id16, id1] = found;
    id16.or(id1).map(|(_, f)| f).ok_or_else(|| "no family name in the font".into())
}

/// What a sync did: one warning per skipped file / missing anchor, and the roles it wrote.
#[derive(Debug, Default)]
pub struct SyncReport {
    pub warnings: Vec<String>,
    /// (role, family, weights)
    pub synced: Vec<(String, String, Vec<u16>)>,
}

struct Face {
    weight: u16,
    ext: &'static str,
    bytes: Vec<u8>,
}

struct Role {
    name: &'static str,
    family: String,
    faces: Vec<Face>,
    license: Option<Vec<u8>>,
}

/// Sync `mobiler.toml` `[fonts]` into the shells:
/// - Android `res/font/mobiler_<role>_<weight>.<ext>` (looked up by name at runtime);
/// - iOS `Sources/Fonts/mobiler-<role>-<weight>.<ext>` + a `project.yml` block (`UIAppFonts` + the
///   `MobilerFontDisplay`/`MobilerFontBody` family names);
/// - web (if `web/index.html` exists) `web/fonts/` + `fonts.css` + an `index.html` block.
///
/// Idempotent (files are rewritten only when they differ; stale `mobiler*` copies are removed). Never
/// fails on a bad font file — it is skipped with a warning. No `mobiler.toml` → touches nothing.
pub fn sync(root: &Path) -> anyhow::Result<SyncReport> {
    let mut report = SyncReport::default();
    let manifest_path = root.join("mobiler.toml");
    let section = if manifest_path.exists() {
        let text = fs::read_to_string(&manifest_path).with_context(|| format!("reading {}", manifest_path.display()))?;
        let manifest: Manifest = toml::from_str(&text).with_context(|| format!("parsing {}", manifest_path.display()))?;
        match manifest.fonts {
            Some(fonts) => fonts,
            // [fonts] removed after a sync: clean up like an empty [fonts].
            None if previously_synced(root) => FontsSection::default(),
            // A mobiler.toml without [fonts] (e.g. only [splash]) that never used fonts: touch nothing.
            None => return Ok(report),
        }
    } else if previously_synced(root) {
        // mobiler.toml was removed after a sync: clean up like an empty [fonts].
        FontsSection::default()
    } else {
        return Ok(report); // never used fonts: touch nothing
    };
    let mut roles = Vec::new();
    // A listed file that can't be read at all (missing, permissions, a checkout without the fonts) stops
    // the sync before it changes anything, so committed copies aren't deleted by accident.
    let mut unreadable = Vec::new();
    for (name, spec) in [("display", section.display), ("body", section.body)] {
        let Some(spec) = spec else { continue };
        let mut faces: BTreeMap<u16, Face> = BTreeMap::new();
        let mut first_family = None;
        let mut families: Vec<String> = Vec::new();
        for file in &spec.files {
            let bytes = match fs::read(root.join(file)) {
                Ok(b) => b,
                Err(e) => {
                    unreadable.push(format!("{file}: can't read it ({e}) — synced fonts left unchanged"));
                    continue;
                }
            };
            let info = match read_font_info(&bytes) {
                Ok(i) => i,
                Err(e) => {
                    report.warnings.push(format!("{file}: {e} — skipped"));
                    continue;
                }
            };
            // Nearest hundred, in u32 so a corrupt usWeightClass (up to 65535) can't overflow.
            let weight = u16::try_from(((u32::from(info.weight) + 50) / 100 * 100).clamp(100, 900)).unwrap_or(900);
            if faces.contains_key(&weight) {
                report.warnings.push(format!("{file}: a second file for weight {weight} — skipped"));
                continue;
            }
            let ext = if bytes.starts_with(b"OTTO") { "otf" } else { "ttf" };
            if !families.contains(&info.family) {
                families.push(info.family.clone());
            }
            first_family.get_or_insert(info.family);
            faces.insert(weight, Face { weight, ext, bytes });
        }
        if families.len() > 1 {
            report.warnings.push(format!(
                "[fonts] {name}: the files declare several families ({}) — use one family per role (iOS registers each separately)",
                families.join(", ")
            ));
        }
        let license = match &spec.license {
            Some(path) => match fs::read(root.join(path)) {
                Ok(b) => Some(b),
                Err(e) => {
                    unreadable.push(format!("{path}: can't read it ({e}) — synced fonts left unchanged"));
                    None
                }
            },
            None => None,
        };
        if faces.is_empty() {
            if !spec.files.is_empty() {
                report.warnings.push(format!("[fonts] {name}: no usable font files — the system font is used"));
            }
            continue;
        }
        // An explicit family must be one the files declare — iOS looks the fonts up by it and would
        // silently fall back to the system font on a mismatch (e.g. a typo).
        if let Some(explicit) = spec.family.as_ref().filter(|e| !families.contains(e)) {
            report.warnings.push(format!(
                "[fonts] {name}: family \"{explicit}\" doesn't match the files ({}) — iOS won't find it; use the files' family name",
                families.join(", ")
            ));
        }
        let family = spec.family.or(first_family).unwrap_or_default();
        report.synced.push((name.to_string(), family.clone(), faces.keys().copied().collect()));
        roles.push(Role { name, family, faces: faces.into_values().collect(), license });
    }
    if !unreadable.is_empty() {
        report.warnings.extend(unreadable);
        report.synced.clear();
        return Ok(report);
    }

    sync_android(root, &roles)?;
    sync_ios(root, &roles, &mut report)?;
    sync_web(root, &roles, &mut report)?;
    Ok(report)
}

fn sync_android(root: &Path, roles: &[Role]) -> anyhow::Result<()> {
    if root.join("Android").is_dir() {
        let files = roles.iter().flat_map(|r| r.faces.iter().map(move |f| (format!("mobiler_{}_{}.{}", r.name, f.weight, f.ext), &f.bytes)));
        write_set(root, &root.join("Android/app/src/main/res/font"), "mobiler_", &files.collect::<Vec<_>>())?;
    }
    Ok(())
}

fn sync_ios(root: &Path, roles: &[Role], report: &mut SyncReport) -> anyhow::Result<()> {
    if root.join("iOS").is_dir() {
        let names = ios_names(roles);
        let files = roles.iter().flat_map(|r| r.faces.iter().map(move |f| (format!("mobiler-{}-{}.{}", r.name, f.weight, f.ext), &f.bytes)));
        write_set(root, &root.join("iOS/Sources/Fonts"), "mobiler-", &files.collect::<Vec<_>>())?;
        let yml_path = root.join("iOS/project.yml");
        if let Ok(yml) = fs::read_to_string(&yml_path) {
            match replace_block(&yml, "# mobiler:fonts-begin", "# mobiler:fonts-end", "# mobiler:info-plist", |indent| {
                let mut lines = Vec::new();
                if !names.is_empty() {
                    let list: Vec<String> = names.iter().map(|n| format!("\"{n}\"")).collect();
                    lines.push(format!("{indent}UIAppFonts: [{}]", list.join(", ")));
                }
                for r in roles {
                    let key = if r.name == "display" { "MobilerFontDisplay" } else { "MobilerFontBody" };
                    lines.push(format!("{indent}{key}: \"{}\"", yaml_escape(&r.family)));
                }
                lines
            }) {
                Ok(new) => write_if_changed(root, &yml_path, new.as_bytes())?,
                Err(why) => report.warnings.push(format!("iOS/project.yml: {why} — fonts block not written")),
            }
        }
    }
    Ok(())
}

fn sync_web(root: &Path, roles: &[Role], report: &mut SyncReport) -> anyhow::Result<()> {
    let index = root.join("web/index.html");
    if index.exists() {
        let css = fonts_css(roles).into_bytes();
        let mut files: Vec<(String, &Vec<u8>)> = roles
            .iter()
            .flat_map(|r| r.faces.iter().map(move |f| (format!("mobiler-{}-{}.{}", r.name, f.weight, f.ext), &f.bytes)))
            .collect();
        files.extend(roles.iter().filter_map(|r| r.license.as_ref().map(|l| (format!("mobiler-{}-LICENSE.txt", r.name), l))));
        if !roles.is_empty() {
            files.push(("mobiler-fonts.css".to_string(), &css));
        }
        // The stylesheet was `fonts.css` before it moved to the prefixed name — drop our old one only.
        let legacy = root.join("web/fonts/fonts.css");
        if fs::read_to_string(&legacy).is_ok_and(|t| t.starts_with("/* Generated by `mobiler fonts sync`")) {
            let _ = fs::remove_file(&legacy);
        }
        write_set(root, &root.join("web/fonts"), "mobiler-", &files)?;
        let html = fs::read_to_string(&index).with_context(|| format!("reading {}", index.display()))?;
        let has_fonts = !roles.is_empty();
        match replace_block(&html, "<!-- mobiler:fonts-begin -->", "<!-- mobiler:fonts-end -->", "</head>", |indent| {
            if has_fonts {
                vec![
                    format!("{indent}<link data-trunk rel=\"copy-dir\" href=\"fonts\"/>"),
                    format!("{indent}<link rel=\"stylesheet\" href=\"fonts/mobiler-fonts.css\"/>"),
                ]
            } else {
                Vec::new()
            }
        }) {
            Ok(new) => write_if_changed(root, &index, new.as_bytes())?,
            Err(why) => report.warnings.push(format!("web/index.html: {why} — fonts block not written")),
        }
    }
    Ok(())
}

/// Sorted synced file names for iOS `UIAppFonts` (bare names: `XcodeGen` copies resources flat).
fn ios_names(roles: &[Role]) -> Vec<String> {
    let mut names: Vec<String> = roles.iter().flat_map(|r| r.faces.iter().map(move |f| format!("mobiler-{}-{}.{}", r.name, f.weight, f.ext))).collect();
    names.sort();
    names
}

fn fonts_css(roles: &[Role]) -> String {
    let mut css = String::from("/* Generated by `mobiler fonts sync` from mobiler.toml [fonts] — do not edit. */\n");
    for r in roles {
        for f in &r.faces {
            let format = if f.ext == "otf" { "opentype" } else { "truetype" };
            let _ = writeln!(
                css,
                "@font-face {{ font-family: \"mobiler-{}\"; font-weight: {}; font-style: normal; font-display: swap; src: url(\"mobiler-{}-{}.{}\") format(\"{format}\"); }}",
                r.name, f.weight, r.name, f.weight, f.ext
            );
        }
    }
    css
}

fn yaml_escape(s: &str) -> String {
    s.chars().filter(|c| !c.is_control()).collect::<String>().replace('\\', "\\\\").replace('"', "\\\"")
}

/// Whether an earlier sync left copies or blocks behind (so a removed mobiler.toml still cleans up).
fn previously_synced(root: &Path) -> bool {
    let has_prefixed = |dir: &str, prefix: &str| {
        fs::read_dir(root.join(dir)).is_ok_and(|entries| entries.flatten().any(|e| e.file_name().to_string_lossy().starts_with(prefix)))
    };
    let has_marker = |file: &str, marker: &str| fs::read_to_string(root.join(file)).is_ok_and(|t| t.contains(marker));
    has_prefixed("Android/app/src/main/res/font", "mobiler_")
        || has_prefixed("iOS/Sources/Fonts", "mobiler-")
        || has_prefixed("web/fonts", "mobiler-")
        || has_marker("iOS/project.yml", "# mobiler:fonts-begin")
        || has_marker("web/index.html", "<!-- mobiler:fonts-begin -->")
}

/// Make `dir` hold exactly `files` among the entries starting with `prefix` (others untouched).
fn write_set(root: &Path, dir: &Path, prefix: &str, files: &[(String, &Vec<u8>)]) -> anyhow::Result<()> {
    // Never list, delete or write through a symlinked folder (its target may be outside the app).
    crate::fsguard::check(root, dir)?;
    if !files.is_empty() {
        fs::create_dir_all(dir).with_context(|| format!("creating {}", dir.display()))?;
    }
    let wanted: Vec<&str> = files.iter().map(|(n, _)| n.as_str()).collect();
    if let Ok(entries) = fs::read_dir(dir) {
        for e in entries.flatten() {
            let name = e.file_name().to_string_lossy().to_string();
            if name.starts_with(prefix) && !wanted.contains(&name.as_str()) {
                fs::remove_file(e.path()).with_context(|| format!("removing {}", e.path().display()))?;
            }
        }
    }
    for (name, bytes) in files {
        write_if_changed(root, &dir.join(name), bytes)?;
    }
    if files.is_empty() {
        let _ = fs::remove_dir(dir); // only succeeds when empty
    }
    Ok(())
}

fn write_if_changed(root: &Path, path: &Path, bytes: &[u8]) -> anyhow::Result<()> {
    if fs::read(path).is_ok_and(|old| old == bytes) {
        return Ok(());
    }
    crate::fsguard::write(root, path, bytes)
}

/// Replace the `begin`..`end` block (inclusive, one per file) with begin + `body(indent)` + end, or insert
/// it just above the `anchor` line (indented like it). Keeps the file's line endings (LF or CRLF).
/// `Err` (nothing written) when the markers are unbalanced or there is neither a block nor the anchor.
pub(crate) fn replace_block(text: &str, begin: &str, end: &str, anchor: &str, body: impl Fn(&str) -> Vec<String>) -> Result<String, &'static str> {
    let lines: Vec<&str> = text.lines().collect();
    let indent_of = |l: &str| l[..l.len() - l.trim_start().len()].to_string();
    let begins: Vec<usize> = lines.iter().enumerate().filter(|(_, l)| l.contains(begin)).map(|(i, _)| i).collect();
    let ends: Vec<usize> = lines.iter().enumerate().filter(|(_, l)| l.contains(end)).map(|(i, _)| i).collect();
    let (start, stop, indent) = match (begins.as_slice(), ends.as_slice()) {
        ([b], [e]) if e > b => (*b, e + 1, indent_of(lines[*b])),
        ([], []) => {
            let a = lines.iter().position(|l| l.contains(anchor)).ok_or("no anchor to insert the block at")?;
            (a, a, indent_of(lines[a]))
        }
        _ => return Err("unbalanced marker lines (fix them by hand)"),
    };
    let mut out: Vec<String> = lines[..start].iter().map(|l| (*l).to_string()).collect();
    out.push(format!("{indent}{begin}"));
    out.extend(body(&indent));
    out.push(format!("{indent}{end}"));
    out.extend(lines[stop..].iter().map(|l| (*l).to_string()));
    let nl = if text.contains("\r\n") { "\r\n" } else { "\n" };
    let mut s = out.join(nl);
    if text.ends_with('\n') {
        s.push_str(nl);
    }
    Ok(s)
}

/// `mobiler fonts sync` — run the sync from the app root and print what happened.
pub fn run_sync_cli() -> anyhow::Result<()> {
    let root = std::env::current_dir().context("reading current directory")?;
    if !root.join("mobiler.toml").exists() {
        println!("No mobiler.toml here — nothing to sync. Add a [fonts] section to use custom fonts.");
        return Ok(());
    }
    let report = sync(&root)?;
    for w in &report.warnings {
        eprintln!("warning: fonts: {w}");
    }
    if report.synced.is_empty() {
        println!("No usable fonts in [fonts] — the system font is used.");
    }
    for (role, family, weights) in &report.synced {
        let ws: Vec<String> = weights.iter().map(u16::to_string).collect();
        println!("{role}: {family} ({})", ws.join(", "));
    }
    Ok(())
}

/// Called at the start of `mobiler build` / `dev` / `watch`: sync and print warnings, never fail.
pub fn sync_for_build(root: &Path) {
    match sync(root) {
        Ok(r) => {
            for w in r.warnings {
                eprintln!("warning: fonts: {w}");
            }
        }
        Err(e) => eprintln!("warning: fonts: {e:#} — fonts not synced"),
    }
}

/// What `mobiler watch` should also watch for fonts: `mobiler.toml` and the directories holding the
/// listed font/licence files (so editing a font re-runs the build, which re-syncs).
pub fn watch_paths(root: &Path) -> (Vec<std::path::PathBuf>, Vec<std::path::PathBuf>) {
    let manifest = root.join("mobiler.toml");
    let mut dirs: Vec<std::path::PathBuf> = Vec::new();
    if let Some(section) = fs::read_to_string(&manifest).ok().and_then(|t| toml::from_str::<Manifest>(&t).ok()).and_then(|m| m.fonts) {
        for spec in [section.display, section.body].into_iter().flatten() {
            for file in spec.files.iter().chain(spec.license.iter()) {
                if let Some(parent) = root.join(file).parent().map(Path::to_path_buf).filter(|p| !dirs.contains(p)) {
                    dirs.push(parent);
                }
            }
        }
    }
    (dirs, vec![manifest])
}

/// One `mobiler doctor` line about `[fonts]` in the current directory.
pub fn doctor_line(root: &Path) -> String {
    let path = root.join("mobiler.toml");
    let Ok(text) = fs::read_to_string(&path) else { return "fonts: no mobiler.toml (system fonts)".into() };
    match toml::from_str::<Manifest>(&text) {
        Ok(m) => match m.fonts {
            Some(f) => {
                let mut parts = Vec::new();
                for (name, spec) in [("display", f.display), ("body", f.body)] {
                    if let Some(s) = spec {
                        let present = s.files.iter().filter(|p| root.join(p).exists()).count();
                        parts.push(format!("{name} {present}/{} files", s.files.len()));
                    }
                }
                format!("fonts: [fonts] {}", if parts.is_empty() { "empty".to_string() } else { parts.join(", ") })
            }
            None => "fonts: no [fonts] (system fonts)".into(),
        },
        Err(e) => format!("fonts: mobiler.toml doesn't parse: {e}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A minimal sfnt with just OS/2 (weight) and name (family, nameID 1, Windows UTF-16BE).
    pub(super) fn tiny_font(weight: u16, family: &str) -> Vec<u8> {
        tiny_font_names(weight, &[(3, 0x409, 1, family)])
    }

    /// Like `tiny_font`, with explicit (platform, language, nameID, text) records (Windows UTF-16BE).
    pub(super) fn tiny_font_names(weight: u16, records: &[(u16, u16, u16, &str)]) -> Vec<u8> {
        let encoded: Vec<Vec<u8>> = records.iter().map(|(_, _, _, t)| t.encode_utf16().flat_map(u16::to_be_bytes).collect()).collect();
        let mut name = Vec::new();
        name.extend_from_slice(&0u16.to_be_bytes()); // format
        name.extend_from_slice(&(records.len() as u16).to_be_bytes()); // count
        name.extend_from_slice(&(6 + 12 * records.len() as u16).to_be_bytes()); // stringOffset
        let mut off = 0u16;
        for ((platform, lang, id, _), bytes) in records.iter().zip(&encoded) {
            for v in [*platform, 1, *lang, *id, bytes.len() as u16, off] {
                name.extend_from_slice(&v.to_be_bytes());
            }
            off += bytes.len() as u16;
        }
        for bytes in &encoded {
            name.extend_from_slice(bytes);
        }
        let mut os2 = vec![0u8; 8];
        os2[4..6].copy_from_slice(&weight.to_be_bytes());
        let tables: [(&[u8; 4], &Vec<u8>); 2] = [(b"OS/2", &os2), (b"name", &name)];
        let mut out = Vec::new();
        out.extend_from_slice(&0x0001_0000u32.to_be_bytes());
        out.extend_from_slice(&(tables.len() as u16).to_be_bytes());
        out.extend_from_slice(&[0u8; 6]);
        let mut offset = 12 + 16 * tables.len();
        let mut body = Vec::new();
        for (tag, data) in tables {
            out.extend_from_slice(tag);
            out.extend_from_slice(&0u32.to_be_bytes());
            out.extend_from_slice(&(offset as u32).to_be_bytes());
            out.extend_from_slice(&(data.len() as u32).to_be_bytes());
            body.extend_from_slice(data);
            offset += data.len();
        }
        out.extend_from_slice(&body);
        out
    }

    #[test]
    fn sfnt_reads_weight_and_family() {
        let i = read_font_info(&tiny_font(600, "Space Grotesk")).unwrap();
        assert_eq!((i.family.as_str(), i.weight), ("Space Grotesk", 600));
    }

    #[test]
    fn sfnt_rejects_garbage_without_panicking() {
        assert!(read_font_info(b"wOF2\0\0\0\0garbage").is_err());
        assert!(read_font_info(&[]).is_err());
        let mut t = tiny_font(400, "X");
        t.truncate(20);
        assert!(read_font_info(&t).is_err());
    }

    use std::fs;
    use std::path::PathBuf;

    /// A throwaway app tree: Android res/, iOS project.yml (with the info-plist anchor), web/index.html,
    /// and three tiny fonts under assets/fonts.
    fn app(name: &str, with_web: bool) -> PathBuf {
        let root = std::env::temp_dir().join(format!("mobiler-fonts-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(root.join("Android/app/src/main/res/values")).unwrap();
        fs::create_dir_all(root.join("iOS/Sources")).unwrap();
        fs::write(root.join("iOS/project.yml"), "targets:\n  App:\n    info:\n      properties:\n        CFBundleName: X\n        # mobiler:info-plist — anchor\n").unwrap();
        if with_web {
            fs::create_dir_all(root.join("web")).unwrap();
            fs::write(root.join("web/index.html"), "<html>\n<head>\n  <title>t</title>\n</head>\n<body></body>\n</html>\n").unwrap();
        }
        fs::create_dir_all(root.join("assets/fonts")).unwrap();
        fs::write(root.join("assets/fonts/d400.ttf"), tiny_font(400, "Space Grotesk")).unwrap();
        fs::write(root.join("assets/fonts/d600.ttf"), tiny_font(600, "Space Grotesk")).unwrap();
        fs::write(root.join("assets/fonts/b400.ttf"), tiny_font(400, "Roboto")).unwrap();
        fs::write(
            root.join("mobiler.toml"),
            "[fonts]\ndisplay = { files = [\"assets/fonts/d400.ttf\", \"assets/fonts/d600.ttf\"] }\nbody = { family = \"Roboto\", files = [\"assets/fonts/b400.ttf\"] }\n",
        )
        .unwrap();
        root
    }

    fn read(root: &std::path::Path, rel: &str) -> String {
        fs::read_to_string(root.join(rel)).unwrap_or_default()
    }

    #[cfg(unix)]
    #[test]
    fn a_symlinked_font_folder_is_never_listed_written_or_cleaned() {
        // web/fonts linked outside the app: its `mobiler-*` files are neither replaced nor deleted.
        let root = app("linkedfonts", true);
        let outside = root.with_extension("outside-fonts");
        let _ = fs::remove_dir_all(&outside);
        fs::create_dir_all(&outside).unwrap();
        fs::write(outside.join("mobiler-display-400.ttf"), "MINE").unwrap();
        std::os::unix::fs::symlink(&outside, root.join("web/fonts")).unwrap();
        let err = sync(&root).err().expect("refused");
        assert!(format!("{err:#}").contains("symlink"), "{err:#}");
        assert_eq!(fs::read_to_string(outside.join("mobiler-display-400.ttf")).unwrap(), "MINE");
        assert_eq!(fs::read_dir(&outside).unwrap().count(), 1, "nothing added or removed");
        let _ = fs::remove_dir_all(&outside);
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn a_mobiler_toml_without_fonts_touches_nothing() {
        // e.g. only [splash]: the fonts sync must not add empty font blocks to project.yml / index.html.
        let root = app("nofonts", true);
        fs::write(root.join("mobiler.toml"), "[splash]\nbackground = \"#FFFFFF\"\n").unwrap();
        let (yml, html) = (read(&root, "iOS/project.yml"), read(&root, "web/index.html"));
        sync(&root).unwrap();
        assert_eq!(read(&root, "iOS/project.yml"), yml);
        assert_eq!(read(&root, "web/index.html"), html);
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn sync_writes_every_shell() {
        let root = app("all", true);
        let r = sync(&root).unwrap();
        assert!(r.warnings.is_empty(), "{:?}", r.warnings);
        assert!(root.join("Android/app/src/main/res/font/mobiler_display_400.ttf").exists());
        assert!(root.join("Android/app/src/main/res/font/mobiler_display_600.ttf").exists());
        assert!(root.join("iOS/Sources/Fonts/mobiler-body-400.ttf").exists());
        let yml = read(&root, "iOS/project.yml");
        assert_eq!(yml.matches("# mobiler:fonts-begin").count(), 1);
        assert!(yml.contains(r#"UIAppFonts: ["mobiler-body-400.ttf", "mobiler-display-400.ttf", "mobiler-display-600.ttf"]"#), "{yml}");
        assert!(yml.contains(r#"MobilerFontDisplay: "Space Grotesk""#) && yml.contains(r#"MobilerFontBody: "Roboto""#));
        let css = read(&root, "web/fonts/mobiler-fonts.css");
        assert!(css.contains(r#"font-family: "mobiler-display"; font-weight: 600;"#), "{css}");
        let html = read(&root, "web/index.html");
        assert_eq!(html.matches("<!-- mobiler:fonts-begin -->").count(), 1);
        assert!(html.contains(r#"<link data-trunk rel="copy-dir" href="fonts"/>"#) && html.contains(r#"href="fonts/mobiler-fonts.css""#));
        assert!(html.find("mobiler:fonts-begin").unwrap() < html.find("</head>").unwrap());
    }

    #[test]
    fn sync_is_idempotent() {
        let root = app("idem", true);
        sync(&root).unwrap();
        let files = ["iOS/project.yml", "web/index.html", "web/fonts/mobiler-fonts.css"];
        let before: Vec<String> = files.iter().map(|f| read(&root, f)).collect();
        sync(&root).unwrap();
        let after: Vec<String> = files.iter().map(|f| read(&root, f)).collect();
        assert_eq!(before, after);
    }

    #[test]
    fn removing_a_role_cleans_up() {
        let root = app("remove", true);
        sync(&root).unwrap();
        fs::write(root.join("mobiler.toml"), "[fonts]\nbody = { files = [\"assets/fonts/b400.ttf\"] }\n").unwrap();
        sync(&root).unwrap();
        assert!(!root.join("Android/app/src/main/res/font/mobiler_display_400.ttf").exists());
        assert!(!root.join("iOS/Sources/Fonts/mobiler-display-600.ttf").exists());
        assert!(!root.join("web/fonts/mobiler-display-400.ttf").exists());
        let yml = read(&root, "iOS/project.yml");
        assert!(!yml.contains("MobilerFontDisplay") && yml.contains(r#"UIAppFonts: ["mobiler-body-400.ttf"]"#), "{yml}");
        assert!(!read(&root, "web/fonts/mobiler-fonts.css").contains("mobiler-display"));
    }

    #[test]
    fn bad_files_are_skipped_with_warnings() {
        let root = app("bad", true);
        fs::write(root.join("assets/fonts/x.woff2"), b"wOF2garbagegarbage").unwrap();
        let mut t = tiny_font(500, "Space Grotesk");
        t.truncate(30);
        fs::write(root.join("assets/fonts/trunc.ttf"), t).unwrap();
        fs::write(
            root.join("mobiler.toml"),
            "[fonts]\ndisplay = { files = [\"assets/fonts/d400.ttf\", \"assets/fonts/x.woff2\", \"assets/fonts/trunc.ttf\"] }\n",
        )
        .unwrap();
        let r = sync(&root).unwrap();
        assert_eq!(r.warnings.len(), 2, "{:?}", r.warnings);
        for f in ["x.woff2", "trunc.ttf"] {
            assert!(r.warnings.iter().any(|w| w.contains(f)), "{f}: {:?}", r.warnings);
        }
        assert!(root.join("Android/app/src/main/res/font/mobiler_display_400.ttf").exists());
    }

    #[test]
    fn no_manifest_touches_nothing() {
        let root = app("none", true);
        fs::remove_file(root.join("mobiler.toml")).unwrap();
        let (yml, html) = (read(&root, "iOS/project.yml"), read(&root, "web/index.html"));
        let r = sync(&root).unwrap();
        assert!(r.warnings.is_empty() && r.synced.is_empty());
        assert!(!root.join("iOS/Sources/Fonts").exists() && !root.join("Android/app/src/main/res/font").exists());
        assert_eq!((read(&root, "iOS/project.yml"), read(&root, "web/index.html")), (yml, html));
    }

    #[test]
    fn block_is_replaced_not_duplicated() {
        let root = app("dup", true);
        let yml = read(&root, "iOS/project.yml").replace(
            "        # mobiler:info-plist",
            "        # mobiler:fonts-begin\n        UIAppFonts: [\"old.ttf\"]\n        # mobiler:fonts-end\n        # mobiler:info-plist",
        );
        fs::write(root.join("iOS/project.yml"), yml).unwrap();
        sync(&root).unwrap();
        let yml = read(&root, "iOS/project.yml");
        assert_eq!(yml.matches("# mobiler:fonts-begin").count(), 1);
        assert!(!yml.contains("old.ttf"));
    }

    #[test]
    fn no_web_dir_is_fine() {
        let root = app("noweb", false);
        let r = sync(&root).unwrap();
        assert!(r.warnings.is_empty(), "{:?}", r.warnings);
        assert!(root.join("iOS/Sources/Fonts/mobiler-display-400.ttf").exists() && !root.join("web").exists());
    }

    #[test]
    fn extreme_weight_is_clamped_not_overflowed() {
        let root = app("weight", false);
        fs::write(root.join("assets/fonts/d400.ttf"), tiny_font(0xFFFF, "Space Grotesk")).unwrap();
        let r = sync(&root).unwrap();
        let (_, _, weights) = r.synced.iter().find(|(role, _, _)| role == "display").unwrap();
        assert!(weights.contains(&900), "{weights:?}");
    }

    #[test]
    fn explicit_family_that_matches_no_file_warns() {
        let root = app("family", false);
        fs::write(
            root.join("mobiler.toml"),
            "[fonts]\ndisplay = { family = \"Space Grotesq\", files = [\"assets/fonts/d400.ttf\"] }\n",
        )
        .unwrap();
        let r = sync(&root).unwrap();
        assert!(r.warnings.iter().any(|w| w.contains("Space Grotesq") && w.contains("Space Grotesk")), "{:?}", r.warnings);
    }

    #[test]
    fn unreadable_file_changes_nothing() {
        let root = app("unreadable", true);
        sync(&root).unwrap();
        let before = read(&root, "iOS/project.yml");
        fs::remove_file(root.join("assets/fonts/d600.ttf")).unwrap();
        let r = sync(&root).unwrap();
        assert!(r.warnings.iter().any(|w| w.contains("d600.ttf") && w.contains("unchanged")), "{:?}", r.warnings);
        assert!(root.join("Android/app/src/main/res/font/mobiler_display_600.ttf").exists());
        assert_eq!(read(&root, "iOS/project.yml"), before);
    }

    #[test]
    fn unbalanced_markers_are_left_alone_with_a_warning() {
        let root = app("unbalanced", false);
        let yml = read(&root, "iOS/project.yml").replace("        # mobiler:info-plist", "        # mobiler:fonts-begin\n        # mobiler:info-plist");
        fs::write(root.join("iOS/project.yml"), &yml).unwrap();
        let r = sync(&root).unwrap();
        assert!(r.warnings.iter().any(|w| w.contains("project.yml") && w.contains("marker")), "{:?}", r.warnings);
        assert_eq!(read(&root, "iOS/project.yml"), yml);
    }

    #[test]
    fn crlf_line_endings_are_kept() {
        let root = app("crlf", true);
        let html = read(&root, "web/index.html").replace('\n', "\r\n");
        fs::write(root.join("web/index.html"), html).unwrap();
        sync(&root).unwrap();
        let html = read(&root, "web/index.html");
        assert!(html.contains("<!-- mobiler:fonts-begin -->\r\n") && !html.replace("\r\n", "").contains('\n'), "{html:?}");
    }

    #[test]
    fn deleting_the_manifest_cleans_up() {
        let root = app("delmanifest", true);
        sync(&root).unwrap();
        fs::remove_file(root.join("mobiler.toml")).unwrap();
        sync(&root).unwrap();
        assert!(!root.join("Android/app/src/main/res/font/mobiler_display_400.ttf").exists());
        assert!(!root.join("web/fonts").exists());
        assert!(!read(&root, "iOS/project.yml").contains("UIAppFonts"));
        assert!(!read(&root, "web/index.html").contains("copy-dir"));
    }

    #[test]
    fn mixed_families_in_a_role_warn() {
        let root = app("mixed", false);
        fs::write(root.join("assets/fonts/d600.ttf"), tiny_font(600, "Other Sans")).unwrap();
        let r = sync(&root).unwrap();
        assert!(r.warnings.iter().any(|w| w.contains("Other Sans") && w.contains("Space Grotesk")), "{:?}", r.warnings);
    }

    #[test]
    fn license_is_copied_next_to_the_web_fonts() {
        let root = app("license", true);
        fs::write(root.join("assets/fonts/OFL.txt"), "SIL Open Font License").unwrap();
        fs::write(
            root.join("mobiler.toml"),
            "[fonts]\ndisplay = { files = [\"assets/fonts/d400.ttf\"], license = \"assets/fonts/OFL.txt\" }\n",
        )
        .unwrap();
        sync(&root).unwrap();
        assert_eq!(read(&root, "web/fonts/mobiler-display-LICENSE.txt"), "SIL Open Font License");
    }

    #[test]
    fn family_prefers_the_english_windows_record() {
        let bytes = tiny_font_names(400, &[(3, 0x404, 1, "思源"), (3, 0x409, 1, "Space Grotesk")]);
        assert_eq!(read_font_info(&bytes).unwrap().family, "Space Grotesk");
    }

    #[test]
    fn yaml_family_drops_control_characters() {
        let root = app("ctrl", false);
        fs::write(root.join("mobiler.toml"), "[fonts]\nbody = { family = \"Rob\\u0000oto\", files = [\"assets/fonts/b400.ttf\"] }\n").unwrap();
        sync(&root).unwrap();
        assert!(!read(&root, "iOS/project.yml").contains('\u{0}'));
    }

    #[test]
    fn manifest_parses_roles() {
        let m: Manifest = toml::from_str(
            r#"[fonts]
display = { family = "Space Grotesk", files = ["a.ttf"] }
body = { files = ["b.ttf", "c.ttf"] }"#,
        )
        .unwrap();
        let f = m.fonts.unwrap();
        assert_eq!(f.display.unwrap().family.as_deref(), Some("Space Grotesk"));
        assert_eq!(f.body.unwrap().files.len(), 2);
    }
}
