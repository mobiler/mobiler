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
    let mut found: [Option<String>; 2] = [None, None]; // [id16, id1]
    for i in 0..count {
        let r = 6 + 12 * i;
        let (platform, name_id) = (be16(name, r)?, be16(name, r + 6)?);
        let slot = match name_id {
            16 => 0,
            1 => 1,
            _ => continue,
        };
        if found[slot].is_some() {
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
            found[slot] = Some(text.trim().to_string());
        }
    }
    let [id16, id1] = found;
    id16.or(id1).ok_or_else(|| "no family name in the font".into())
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
    if !manifest_path.exists() {
        return Ok(report);
    }
    let text = fs::read_to_string(&manifest_path).with_context(|| format!("reading {}", manifest_path.display()))?;
    let manifest: Manifest = toml::from_str(&text).with_context(|| format!("parsing {}", manifest_path.display()))?;
    let section = manifest.fonts.unwrap_or_default();
    let mut roles = Vec::new();
    for (name, spec) in [("display", section.display), ("body", section.body)] {
        let Some(spec) = spec else { continue };
        let mut faces: BTreeMap<u16, Face> = BTreeMap::new();
        let mut first_family = None;
        let mut families: Vec<String> = Vec::new();
        for file in &spec.files {
            let bytes = match fs::read(root.join(file)) {
                Ok(b) => b,
                Err(e) => {
                    report.warnings.push(format!("{file}: can't read it ({e}) — skipped"));
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
        roles.push(Role { name, family, faces: faces.into_values().collect() });
    }

    sync_android(root, &roles)?;
    sync_ios(root, &roles, &mut report)?;
    sync_web(root, &roles, &mut report)?;
    Ok(report)
}

fn sync_android(root: &Path, roles: &[Role]) -> anyhow::Result<()> {
    if root.join("Android").is_dir() {
        let files = roles.iter().flat_map(|r| r.faces.iter().map(move |f| (format!("mobiler_{}_{}.{}", r.name, f.weight, f.ext), &f.bytes)));
        write_set(&root.join("Android/app/src/main/res/font"), "mobiler_", &files.collect::<Vec<_>>())?;
    }
    Ok(())
}

fn sync_ios(root: &Path, roles: &[Role], report: &mut SyncReport) -> anyhow::Result<()> {
    if root.join("iOS").is_dir() {
        let names = ios_names(roles);
        let files = roles.iter().flat_map(|r| r.faces.iter().map(move |f| (format!("mobiler-{}-{}.{}", r.name, f.weight, f.ext), &f.bytes)));
        write_set(&root.join("iOS/Sources/Fonts"), "mobiler-", &files.collect::<Vec<_>>())?;
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
                Some(new) => write_if_changed(&yml_path, new.as_bytes())?,
                None => report.warnings.push("iOS/project.yml has no `# mobiler:info-plist` anchor — add the fonts block by hand".into()),
            }
        }
    }
    Ok(())
}

fn sync_web(root: &Path, roles: &[Role], report: &mut SyncReport) -> anyhow::Result<()> {
    let index = root.join("web/index.html");
    if index.exists() {
        let files = roles.iter().flat_map(|r| r.faces.iter().map(move |f| (format!("mobiler-{}-{}.{}", r.name, f.weight, f.ext), &f.bytes)));
        let fonts_dir = root.join("web/fonts");
        write_set(&fonts_dir, "mobiler-", &files.collect::<Vec<_>>())?;
        let css_path = fonts_dir.join("fonts.css");
        if roles.is_empty() {
            let _ = fs::remove_file(&css_path);
            let _ = fs::remove_dir(&fonts_dir); // only if now empty
        } else {
            write_if_changed(&css_path, fonts_css(roles).as_bytes())?;
        }
        let html = fs::read_to_string(&index).with_context(|| format!("reading {}", index.display()))?;
        let has_fonts = !roles.is_empty();
        match replace_block(&html, "<!-- mobiler:fonts-begin -->", "<!-- mobiler:fonts-end -->", "</head>", |indent| {
            if has_fonts {
                vec![
                    format!("{indent}<link data-trunk rel=\"copy-dir\" href=\"fonts\"/>"),
                    format!("{indent}<link rel=\"stylesheet\" href=\"fonts/fonts.css\"/>"),
                ]
            } else {
                Vec::new()
            }
        }) {
            Some(new) => write_if_changed(&index, new.as_bytes())?,
            None => report.warnings.push("web/index.html has no `</head>` — add the fonts links by hand".into()),
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
    s.replace('\\', "\\\\").replace('"', "\\\"")
}

/// Make `dir` hold exactly `files` among the entries starting with `prefix` (others untouched).
fn write_set(dir: &Path, prefix: &str, files: &[(String, &Vec<u8>)]) -> anyhow::Result<()> {
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
        write_if_changed(&dir.join(name), bytes)?;
    }
    if files.is_empty() {
        let _ = fs::remove_dir(dir); // only succeeds when empty
    }
    Ok(())
}

fn write_if_changed(path: &Path, bytes: &[u8]) -> anyhow::Result<()> {
    if fs::read(path).is_ok_and(|old| old == bytes) {
        return Ok(());
    }
    fs::write(path, bytes).with_context(|| format!("writing {}", path.display()))
}

/// Replace the `begin`..`end` block (inclusive, one per file) with begin + `body(indent)` + end, or insert
/// it just above the `anchor` line (indented like it). `None` when neither a block nor the anchor exists.
fn replace_block(text: &str, begin: &str, end: &str, anchor: &str, body: impl Fn(&str) -> Vec<String>) -> Option<String> {
    let lines: Vec<&str> = text.lines().collect();
    let indent_of = |l: &str| l[..l.len() - l.trim_start().len()].to_string();
    let (start, stop, indent) = match (lines.iter().position(|l| l.contains(begin)), lines.iter().position(|l| l.contains(end))) {
        (Some(b), Some(e)) if e >= b => (b, e + 1, indent_of(lines[b])),
        _ => {
            let a = lines.iter().position(|l| l.contains(anchor))?;
            (a, a, indent_of(lines[a]))
        }
    };
    let mut out: Vec<String> = lines[..start].iter().map(|l| (*l).to_string()).collect();
    out.push(format!("{indent}{begin}"));
    out.extend(body(&indent));
    out.push(format!("{indent}{end}"));
    out.extend(lines[stop..].iter().map(|l| (*l).to_string()));
    let mut s = out.join("\n");
    if text.ends_with('\n') {
        s.push('\n');
    }
    Some(s)
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
        let name_str: Vec<u8> = family.encode_utf16().flat_map(u16::to_be_bytes).collect();
        let mut name = Vec::new();
        name.extend_from_slice(&0u16.to_be_bytes()); // format
        name.extend_from_slice(&1u16.to_be_bytes()); // count
        name.extend_from_slice(&(6u16 + 12).to_be_bytes()); // stringOffset
        for v in [3u16, 1, 0x409, 1, name_str.len() as u16, 0] {
            name.extend_from_slice(&v.to_be_bytes());
        }
        name.extend_from_slice(&name_str);
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
        let css = read(&root, "web/fonts/fonts.css");
        assert!(css.contains(r#"font-family: "mobiler-display"; font-weight: 600;"#), "{css}");
        let html = read(&root, "web/index.html");
        assert_eq!(html.matches("<!-- mobiler:fonts-begin -->").count(), 1);
        assert!(html.contains(r#"<link data-trunk rel="copy-dir" href="fonts"/>"#) && html.contains(r#"href="fonts/fonts.css""#));
        assert!(html.find("mobiler:fonts-begin").unwrap() < html.find("</head>").unwrap());
    }

    #[test]
    fn sync_is_idempotent() {
        let root = app("idem", true);
        sync(&root).unwrap();
        let files = ["iOS/project.yml", "web/index.html", "web/fonts/fonts.css"];
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
        assert!(!read(&root, "web/fonts/fonts.css").contains("mobiler-display"));
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
            "[fonts]\ndisplay = { files = [\"assets/fonts/d400.ttf\", \"assets/fonts/missing.ttf\", \"assets/fonts/x.woff2\", \"assets/fonts/trunc.ttf\"] }\n",
        )
        .unwrap();
        let r = sync(&root).unwrap();
        assert_eq!(r.warnings.len(), 3, "{:?}", r.warnings);
        for f in ["missing.ttf", "x.woff2", "trunc.ttf"] {
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
