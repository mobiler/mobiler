//! `mobiler.toml` `[splash]`: the launch screen's background (light/dark) and an optional logo, synced
//! into the Android, iOS and web shells (ADR-0048).

use serde::Deserialize;

/// Marks a file the sync wrote, so a later sync may rewrite it (a seed without it was edited by hand).
pub const HEADER: &str = "generated from mobiler.toml [splash]; edit mobiler.toml, not this file";
/// Android 12+ draws the splash icon in a 240dp box masked to a 160dp circle; a logo must fit the
/// circle's inscribed square to show whole.
const ANDROID12_BOX_DP: u32 = 240;
const ANDROID12_SQUARE_DP: u32 = 113;

/// `[splash]` as written in mobiler.toml.
#[derive(Deserialize, Debug, Clone)]
#[serde(deny_unknown_fields)]
pub struct SplashSpec {
    pub background: String,
    pub background_dark: Option<String>,
    pub logo: Option<String>,
    pub logo_dark: Option<String>,
    pub logo_size: Option<u32>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rgba {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: u8,
}

impl Rgba {
    /// `#RRGGBB` or `#AARRGGBB` (Android's order).
    pub fn parse(s: &str) -> Result<Rgba, String> {
        let bad = || format!("{s:?}: a colour is #RRGGBB or #AARRGGBB");
        let hex = s.strip_prefix('#').filter(|h| h.is_ascii()).ok_or_else(bad)?;
        let byte = |i: usize| u8::from_str_radix(&hex[i..i + 2], 16).map_err(|_| bad());
        match hex.len() {
            6 => Ok(Rgba { r: byte(0)?, g: byte(2)?, b: byte(4)?, a: 0xFF }),
            8 => Ok(Rgba { a: byte(0)?, r: byte(2)?, g: byte(4)?, b: byte(6)? }),
            _ => Err(bad()),
        }
    }

    /// Android's `#AARRGGBB`.
    pub fn android(&self) -> String {
        format!("#{:02X}{:02X}{:02X}{:02X}", self.a, self.r, self.g, self.b)
    }

    /// CSS `#RRGGBB`.
    pub fn css(&self) -> String {
        format!("#{:02X}{:02X}{:02X}", self.r, self.g, self.b)
    }

    pub fn opaque(self) -> Rgba {
        Rgba { a: 0xFF, ..self }
    }
}

/// A validated `[splash]`: colours made opaque (a launch screen can't be translucent).
#[derive(Debug, Clone)]
pub struct Splash {
    pub light: Rgba,
    pub dark: Rgba,
    pub logo: Option<String>,
    pub logo_dark: Option<String>,
    /// The logo's box, in dp/pt.
    pub size: u32,
    pub warnings: Vec<String>,
}

pub fn validate(spec: &SplashSpec) -> Result<Splash, String> {
    let light = Rgba::parse(&spec.background)?;
    let dark = spec.background_dark.as_deref().map(Rgba::parse).transpose()?.unwrap_or(light);
    let size = spec.logo_size.unwrap_or(120);
    if !(24..=288).contains(&size) {
        return Err(format!("logo_size {size}: must be in 24..=288 (dp/pt)"));
    }
    let mut warnings = Vec::new();
    if light.a != 0xFF || dark.a != 0xFF {
        warnings.push("a translucent background is used as opaque (a launch screen can't be translucent)".into());
    }
    Ok(Splash {
        light: light.opaque(),
        dark: dark.opaque(),
        logo: spec.logo.clone(),
        logo_dark: spec.logo_dark.clone(),
        size,
        warnings,
    })
}

/// A `w`×`h` image fitted inside a `size` square, aspect kept; each side at least 1.
pub fn fit(w: u32, h: u32, size: u32) -> (u32, u32) {
    let scale = f64::from(size) / f64::from(w.max(h).max(1));
    let side = |v: u32| ((f64::from(v) * scale).round() as u32).max(1);
    (side(w), side(h))
}

fn dp(v: f64) -> String {
    if v.fract() == 0.0 { format!("{}dp", v as i64) } else { format!("{v:.1}dp") }
}

/// `values{,-night}/mobiler_splash.xml`. Only the default one carries the `mobiler_splash_icon` alias
/// the template's seed has (kept so nothing that refers to it breaks).
pub fn android_colors_xml(c: Rgba, with_icon_alias: bool) -> String {
    let alias = if with_icon_alias {
        "    <drawable name=\"mobiler_splash_icon\">@mipmap/ic_launcher</drawable>\n"
    } else {
        ""
    };
    format!(
        "<?xml version=\"1.0\" encoding=\"utf-8\"?>\n<!-- {HEADER}. The launch window and splash background. -->\n<resources>\n    <color name=\"mobiler_splash_background\">{}</color>\n{alias}</resources>\n",
        c.opaque().android()
    )
}

/// `drawable/mobiler_launch.xml`: the window behind the app until its first frame — the colour, and
/// the logo centred at `logo` dp when there is one.
pub fn android_launch_xml(logo: Option<(u32, u32)>) -> String {
    let item = logo.map_or(String::new(), |(w, h)| {
        format!("    <item android:gravity=\"center\" android:width=\"{w}dp\" android:height=\"{h}dp\" android:drawable=\"@drawable/mobiler_splash_logo\" />\n")
    });
    format!(
        "<?xml version=\"1.0\" encoding=\"utf-8\"?>\n<!-- {HEADER}. The window behind the app until its first frame. -->\n<layer-list xmlns:android=\"http://schemas.android.com/apk/res/android\">\n    <item android:drawable=\"@color/mobiler_splash_background\" />\n{item}</layer-list>\n"
    )
}

/// `drawable/mobiler_splash_logo_icon.xml`: the Android 12+ splash icon — the logo (box `w`×`h` dp)
/// fitted into the icon circle's inscribed square, centred in the 240dp icon box.
pub fn android_icon_xml(w: u32, h: u32) -> String {
    let (fw, fh) = fit(w, h, ANDROID12_SQUARE_DP);
    let box_dp = f64::from(ANDROID12_BOX_DP);
    let (x, y) = (dp((box_dp - f64::from(fw)) / 2.0), dp((box_dp - f64::from(fh)) / 2.0));
    format!(
        "<?xml version=\"1.0\" encoding=\"utf-8\"?>\n<!-- {HEADER}. The Android 12+ splash icon. -->\n<inset xmlns:android=\"http://schemas.android.com/apk/res/android\"\n    android:drawable=\"@drawable/mobiler_splash_logo\"\n    android:insetLeft=\"{x}\"\n    android:insetRight=\"{x}\"\n    android:insetTop=\"{y}\"\n    android:insetBottom=\"{y}\" />\n"
    )
}

/// The lines between the `mobiler:splash` markers in the framework's v31 themes.
pub fn android_theme_icon_lines(on: bool) -> Vec<String> {
    if on {
        vec![r#"<item name="android:windowSplashScreenAnimatedIcon">@drawable/mobiler_splash_logo_icon</item>"#.into()]
    } else {
        Vec::new()
    }
}

fn ios_color(c: Rgba) -> String {
    format!(
        "\"color\" : {{ \"color-space\" : \"srgb\", \"components\" : {{ \"alpha\" : \"1.000\", \"blue\" : \"0x{:02X}\", \"green\" : \"0x{:02X}\", \"red\" : \"0x{:02X}\" }} }}",
        c.b, c.g, c.r
    )
}

const IOS_DARK: &str = "\"appearances\" : [ { \"appearance\" : \"luminosity\", \"value\" : \"dark\" } ]";
const IOS_INFO: &str = "\"info\" : { \"author\" : \"xcode\", \"version\" : 1 }";

/// `MobilerSplashBackground.colorset/Contents.json` (the iOS launch colour; always opaque).
pub fn ios_colorset_json(light: Rgba, dark: Rgba) -> String {
    format!(
        "{{\n  \"colors\" : [\n    {{\n      {},\n      \"idiom\" : \"universal\"\n    }},\n    {{\n      {IOS_DARK},\n      {},\n      \"idiom\" : \"universal\"\n    }}\n  ],\n  {IOS_INFO}\n}}\n",
        ios_color(light),
        ios_color(dark)
    )
}

/// `MobilerSplashLogo.imageset/Contents.json`: the logo at @3x, and its dark variant if any.
pub fn ios_imageset_json(has_dark: bool) -> String {
    let dark = if has_dark {
        format!(",\n    {{\n      {IOS_DARK},\n      \"filename\" : \"mobiler-splash-logo-dark@3x.png\",\n      \"idiom\" : \"universal\",\n      \"scale\" : \"3x\"\n    }}")
    } else {
        String::new()
    };
    format!(
        "{{\n  \"images\" : [\n    {{\n      \"filename\" : \"mobiler-splash-logo@3x.png\",\n      \"idiom\" : \"universal\",\n      \"scale\" : \"3x\"\n    }}{dark}\n  ],\n  {IOS_INFO}\n}}\n"
    )
}

/// The lines between the `mobiler:splash` markers under `UILaunchScreen` in project.yml.
pub fn ios_plist_lines(on: bool) -> Vec<String> {
    if on { vec!["UIImageName: MobilerSplashLogo".into()] } else { Vec::new() }
}

/// The lines between the `mobiler:splash` markers in web/index.html: the page background (light/dark)
/// and the logo, centred on the empty body until the app's first render fills it.
pub fn web_lines(light: Rgba, dark: Rgba, logo: Option<(u32, u32)>, has_dark: bool) -> Vec<String> {
    let mut lines = Vec::new();
    if logo.is_some() {
        lines.push(r#"<link data-trunk rel="copy-dir" href="splash"/>"#.to_string());
    }
    lines.push("<style>".into());
    lines.push(format!("html{{background:{}}}", light.css()));
    if let Some((w, h)) = logo {
        lines.push(format!(
            "body:empty{{min-height:100vh;margin:0;background:url(splash/mobiler-splash-logo.png) center/{w}px {h}px no-repeat}}"
        ));
    }
    let dark_logo = if logo.is_some() && has_dark {
        "body:empty{background-image:url(splash/mobiler-splash-logo-dark.png)}"
    } else {
        ""
    };
    lines.push(format!("@media (prefers-color-scheme: dark){{html{{background:{}}}{dark_logo}}}", dark.css()));
    lines.push("</style>".into());
    lines
}

// ---------------- sync ----------------

use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

use anyhow::Context;

/// `values/` then `values-night/` `mobiler_splash.xml` (seeds).
pub(crate) const COLORS: [&str; 2] =
    ["Android/app/src/main/res/values/mobiler_splash.xml", "Android/app/src/main/res/values-night/mobiler_splash.xml"];
pub(crate) const LAUNCH: &str = "Android/app/src/main/res/drawable/mobiler_launch.xml";
pub(crate) const COLORSET: &str = "iOS/Sources/Assets.xcassets/MobilerSplashBackground.colorset/Contents.json";
pub(crate) const LOGO_PNG: &str = "Android/app/src/main/res/drawable-xxxhdpi/mobiler_splash_logo.png";
pub(crate) const LOGO_DARK_PNG: &str = "Android/app/src/main/res/drawable-night-xxxhdpi/mobiler_splash_logo.png";
pub(crate) const ICON: &str = "Android/app/src/main/res/drawable/mobiler_splash_logo_icon.xml";
pub(crate) const THEMES: [&str; 2] = [
    "Android/app/src/main/res/values-v31/mobiler_themes.xml",
    "Android/app/src/main/res/values-night-v31/mobiler_themes.xml",
];
pub(crate) const IMAGESET: &str = "iOS/Sources/Assets.xcassets/MobilerSplashLogo.imageset";
pub(crate) const PROJECT_YML: &str = "iOS/project.yml";
pub(crate) const WEB_INDEX: &str = "web/index.html";
pub(crate) const WEB_DIR: &str = "web/splash";
/// What the sync last wrote to each seed (fingerprints), so a JSON seed — which can't carry the header —
/// is still recognised as the sync's own.
pub(crate) const LEDGER: &str = ".mobiler/splash.json";

const XML_BEGIN: &str = "<!-- mobiler:splash-begin -->";
const XML_END: &str = "<!-- mobiler:splash-end -->";
const YML_BEGIN: &str = "# mobiler:splash-begin";
const YML_END: &str = "# mobiler:splash-end";

#[derive(Debug, Default)]
pub struct SyncReport {
    pub written: Vec<String>,
    pub removed: Vec<String>,
    pub warnings: Vec<String>,
}

/// The template's content for each seed path (the seeds carry no placeholders).
pub fn stock_seeds() -> Vec<(&'static str, &'static [u8])> {
    crate::upgrade::SEED_PATHS
        .iter()
        .filter_map(|p| crate::upgrade::TEMPLATES.get_file(p).map(|f| (*p, f.contents())))
        .collect()
}

#[derive(Deserialize, Default)]
struct Manifest {
    splash: Option<SplashSpec>,
}

/// A decoded logo, fitted: its dp box and the source image.
struct Logo {
    image: image::DynamicImage,
    box_dp: (u32, u32),
}

/// One change to the app, computed before anything is written (so a bad input writes nothing).
enum Change {
    Write(String, Vec<u8>),
    Remove(String),
}

/// Sync `mobiler.toml` `[splash]` into the shells (ADR-0048). No section and nothing synced before:
/// touches nothing. A bad config or logo: warnings, nothing written.
pub fn sync(root: &Path) -> anyhow::Result<SyncReport> {
    let mut report = SyncReport::default();
    let toml_path = root.join("mobiler.toml");
    let spec = if toml_path.exists() {
        let text = fs::read_to_string(&toml_path).with_context(|| format!("reading {}", toml_path.display()))?;
        match toml::from_str::<Manifest>(&text) {
            Ok(m) => m.splash,
            Err(e) => {
                report.warnings.push(format!("mobiler.toml [splash] doesn't parse: {e}"));
                return Ok(report);
            }
        }
    } else {
        None
    };
    let ledger = read_ledger(root);
    let Some(spec) = spec else {
        if ledger.is_some() {
            undo(root, &mut report)?;
        }
        return Ok(report);
    };
    let splash = match validate(&spec) {
        Ok(s) => s,
        Err(e) => {
            report.warnings.push(format!("{e} — splash not synced"));
            return Ok(report);
        }
    };
    report.warnings.extend(splash.warnings.iter().cloned());
    let load = |rel: &Option<String>, report: &mut SyncReport| -> Result<Option<Logo>, ()> {
        let Some(rel) = rel else { return Ok(None) };
        match load_logo(&root.join(rel), splash.size) {
            Ok(l) => Ok(Some(l)),
            Err(e) => {
                report.warnings.push(format!("{rel}: {e} — splash not synced"));
                Err(())
            }
        }
    };
    let Ok(logo) = load(&splash.logo, &mut report) else { return Ok(report) };
    let Ok(logo_dark) = load(&splash.logo_dark, &mut report) else { return Ok(report) };
    if logo.is_none() && logo_dark.is_some() {
        report.warnings.push("logo_dark without logo is ignored".into());
    }
    let logo_dark = logo_dark.filter(|_| logo.is_some());

    let mut changes = Vec::new();
    // Seeds: written only while they are the sync's (or the template's) own.
    let ledger = ledger.unwrap_or_default();
    let mut new_ledger = BTreeMap::new();
    let box_dp = logo.as_ref().map(|l| l.box_dp);
    let seeds: [(&str, String); 4] = [
        (COLORS[0], android_colors_xml(splash.light, true)),
        (COLORS[1], android_colors_xml(splash.dark, false)),
        (LAUNCH, android_launch_xml(box_dp)),
        (COLORSET, ios_colorset_json(splash.light, splash.dark)),
    ];
    for (rel, content) in seeds {
        if owned_seed(root, rel, &ledger) {
            new_ledger.insert(rel.to_string(), crate::upgrade::fingerprint(content.as_bytes()));
            changes.push(Change::Write(rel.into(), content.into_bytes()));
        } else {
            report.warnings.push(format!(
                "{rel} was edited by hand — remove your edits (or delete the file) to let [splash] manage it"
            ));
            if let Some(f) = ledger.get(rel) {
                new_ledger.insert(rel.to_string(), f.clone());
            }
        }
    }
    // The sync's own files.
    match &logo {
        Some(l) => {
            changes.push(Change::Write(LOGO_PNG.into(), png_at(l, 4, &mut report)?));
            changes.push(Change::Write(ICON.into(), android_icon_xml(l.box_dp.0, l.box_dp.1).into_bytes()));
            changes.push(Change::Write(format!("{IMAGESET}/Contents.json"), ios_imageset_json(logo_dark.is_some()).into_bytes()));
            changes.push(Change::Write(format!("{IMAGESET}/mobiler-splash-logo@3x.png"), png_at(l, 3, &mut report)?));
        }
        None => {
            changes.push(Change::Remove(LOGO_PNG.into()));
            changes.push(Change::Remove(ICON.into()));
            changes.push(Change::Remove(IMAGESET.into()));
        }
    }
    match &logo_dark {
        Some(l) => {
            changes.push(Change::Write(LOGO_DARK_PNG.into(), png_at(l, 4, &mut report)?));
            changes.push(Change::Write(format!("{IMAGESET}/mobiler-splash-logo-dark@3x.png"), png_at(l, 3, &mut report)?));
        }
        None => {
            changes.push(Change::Remove(LOGO_DARK_PNG.into()));
            changes.push(Change::Remove(format!("{IMAGESET}/mobiler-splash-logo-dark@3x.png")));
        }
    }
    // Marker blocks in framework files: filled, never inserted.
    for t in THEMES {
        block_change(root, t, XML_BEGIN, XML_END, android_theme_icon_lines(logo.is_some()), &mut changes, &mut report);
    }
    block_change(root, PROJECT_YML, YML_BEGIN, YML_END, ios_plist_lines(logo.is_some()), &mut changes, &mut report);
    // Web (only an app with a web shell): a block before </head> and the logo copies.
    if root.join(WEB_INDEX).exists() {
        let html = fs::read_to_string(root.join(WEB_INDEX)).with_context(|| format!("reading {WEB_INDEX}"))?;
        let lines = web_lines(splash.light, splash.dark, box_dp, logo_dark.is_some());
        match crate::fonts::replace_block(&html, XML_BEGIN, XML_END, "</head>", |indent| {
            lines.iter().map(|l| format!("{indent}{l}")).collect()
        }) {
            Ok(text) => changes.push(Change::Write(WEB_INDEX.into(), text.into_bytes())),
            Err(e) => report.warnings.push(format!("{WEB_INDEX}: {e} — web splash not synced")),
        }
        match &logo {
            Some(l) => changes.push(Change::Write(format!("{WEB_DIR}/mobiler-splash-logo.png"), png_at(l, 2, &mut report)?)),
            None => changes.push(Change::Remove(WEB_DIR.into())),
        }
        match &logo_dark {
            Some(l) => changes.push(Change::Write(format!("{WEB_DIR}/mobiler-splash-logo-dark.png"), png_at(l, 2, &mut report)?)),
            None => changes.push(Change::Remove(format!("{WEB_DIR}/mobiler-splash-logo-dark.png"))),
        }
    }
    let ledger_json = serde_json::to_string_pretty(&serde_json::json!({ "seeds": new_ledger }))? + "\n";
    changes.push(Change::Write(LEDGER.into(), ledger_json.into_bytes()));
    apply(root, changes, &mut report)?;
    report.warnings.dedup();
    Ok(report)
}

/// `[splash]` removed after a sync: the sync's own files go, blocks empty (the web block goes), the
/// launch drawable back to colour-only if the sync wrote it; the colours stay (app-owned again).
fn undo(root: &Path, report: &mut SyncReport) -> anyhow::Result<()> {
    let mut changes = vec![
        Change::Remove(LOGO_PNG.into()),
        Change::Remove(LOGO_DARK_PNG.into()),
        Change::Remove(ICON.into()),
        Change::Remove(IMAGESET.into()),
        Change::Remove(WEB_DIR.into()),
        Change::Remove(LEDGER.into()),
    ];
    if fs::read_to_string(root.join(LAUNCH)).is_ok_and(|t| t.contains(HEADER)) {
        changes.push(Change::Write(LAUNCH.into(), android_launch_xml(None).into_bytes()));
    }
    for t in THEMES {
        block_change(root, t, XML_BEGIN, XML_END, Vec::new(), &mut changes, report);
    }
    block_change(root, PROJECT_YML, YML_BEGIN, YML_END, Vec::new(), &mut changes, report);
    if let Ok(html) = fs::read_to_string(root.join(WEB_INDEX)) {
        if let Some(text) = remove_block(&html, XML_BEGIN, XML_END) {
            changes.push(Change::Write(WEB_INDEX.into(), text.into_bytes()));
        }
    }
    // An app without the markers needs no warning when there is nothing to undo there.
    report.warnings.retain(|w| !w.contains("mobiler upgrade --apply"));
    apply(root, changes, report)
}

/// Fill a framework file's marker block; a file without the markers gets a warning, never an insert.
fn block_change(root: &Path, rel: &str, begin: &str, end: &str, lines: Vec<String>, changes: &mut Vec<Change>, report: &mut SyncReport) {
    let Ok(text) = fs::read_to_string(root.join(rel)) else { return };
    if !text.contains(begin) || !text.contains(end) {
        if !lines.is_empty() {
            report.warnings.push(format!(
                "{rel} has no splash markers — run `mobiler upgrade --apply` to get them (the logo isn't shown there until then)"
            ));
        }
        return;
    }
    match crate::fonts::replace_block(&text, begin, end, begin, |indent| lines.iter().map(|l| format!("{indent}{l}")).collect()) {
        Ok(t) => changes.push(Change::Write(rel.into(), t.into_bytes())),
        Err(e) => report.warnings.push(format!("{rel}: {e}")),
    }
}

/// `text` without the `begin`..`end` block (inclusive), keeping line endings; `None` if there is none.
fn remove_block(text: &str, begin: &str, end: &str) -> Option<String> {
    let lines: Vec<&str> = text.split_inclusive('\n').collect();
    let b = lines.iter().position(|l| l.contains(begin))?;
    let e = lines.iter().skip(b).position(|l| l.contains(end))? + b;
    Some(lines[..b].iter().chain(&lines[e + 1..]).copied().collect())
}

fn owned_seed(root: &Path, rel: &str, ledger: &BTreeMap<String, String>) -> bool {
    let Ok(bytes) = fs::read(root.join(rel)) else { return true };
    stock_seeds().iter().any(|(p, stock)| *p == rel && *stock == bytes.as_slice())
        || String::from_utf8_lossy(&bytes).contains(HEADER)
        || ledger.get(rel).is_some_and(|f| *f == crate::upgrade::fingerprint(&bytes))
}

fn read_ledger(root: &Path) -> Option<BTreeMap<String, String>> {
    let text = fs::read_to_string(root.join(LEDGER)).ok()?;
    let v: serde_json::Value = serde_json::from_str(&text).ok()?;
    Some(
        v["seeds"]
            .as_object()
            .map(|m| m.iter().filter_map(|(k, v)| Some((k.clone(), v.as_str()?.to_string()))).collect())
            .unwrap_or_default(),
    )
}

fn load_logo(path: &Path, size: u32) -> Result<Logo, String> {
    let reader = image::ImageReader::open(path)
        .map_err(|e| format!("can't read it ({e})"))?
        .with_guessed_format()
        .map_err(|e| format!("can't read it ({e})"))?;
    if reader.format() != Some(image::ImageFormat::Png) {
        return Err("not a PNG".into());
    }
    let image = reader.decode().map_err(|e| format!("not a readable PNG ({e})"))?;
    let box_dp = fit(image.width(), image.height(), size);
    Ok(Logo { image, box_dp })
}

/// The logo at `scale`× its dp box, PNG-encoded; never larger than the source (a warning names the
/// size it wanted).
fn png_at(logo: &Logo, scale: u32, report: &mut SyncReport) -> anyhow::Result<Vec<u8>> {
    let (w, h) = (logo.box_dp.0 * scale, logo.box_dp.1 * scale);
    let img = if w > logo.image.width() || h > logo.image.height() {
        let warn = format!(
            "the logo is {}×{} px; {w}×{h} px would look sharper — it is used at its own size",
            logo.image.width(),
            logo.image.height()
        );
        if !report.warnings.contains(&warn) {
            report.warnings.push(warn);
        }
        logo.image.clone()
    } else {
        logo.image.resize_exact(w, h, image::imageops::FilterType::Lanczos3)
    };
    let mut out = std::io::Cursor::new(Vec::new());
    img.write_to(&mut out, image::ImageFormat::Png).context("encoding the logo")?;
    Ok(out.into_inner())
}

fn apply(root: &Path, changes: Vec<Change>, report: &mut SyncReport) -> anyhow::Result<()> {
    for change in changes {
        match change {
            Change::Write(rel, bytes) => {
                let path = root.join(&rel);
                if fs::read(&path).is_ok_and(|old| old == bytes) {
                    continue;
                }
                if let Some(parent) = path.parent() {
                    fs::create_dir_all(parent).with_context(|| format!("creating {}", parent.display()))?;
                }
                fs::write(&path, bytes).with_context(|| format!("writing {}", path.display()))?;
                if rel != LEDGER {
                    report.written.push(rel);
                }
            }
            Change::Remove(rel) => {
                let path = root.join(&rel);
                let removed = if path.is_dir() { fs::remove_dir_all(&path).is_ok() } else { fs::remove_file(&path).is_ok() };
                if removed && rel != LEDGER {
                    report.removed.push(rel);
                }
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn spec(bg: &str) -> SplashSpec {
        SplashSpec { background: bg.into(), background_dark: None, logo: None, logo_dark: None, logo_size: None }
    }

    #[test]
    fn colours_parse_rgb_and_argb() {
        assert_eq!(Rgba::parse("#FFFBFE").unwrap(), Rgba { r: 0xFF, g: 0xFB, b: 0xFE, a: 0xFF });
        assert_eq!(Rgba::parse("#801C1B1F").unwrap(), Rgba { r: 0x1C, g: 0x1B, b: 0x1F, a: 0x80 });
        assert_eq!(Rgba::parse("#fffbfe").unwrap().android(), "#FFFFFBFE");
        for bad in ["FFFBFE", "#FFF", "#GGGGGG", "", "#FFFBFE00FF", "#ÿÿÿ"] {
            assert!(Rgba::parse(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn validate_applies_defaults_and_ranges() {
        let s = validate(&spec("#FFFFFF")).unwrap();
        assert_eq!((s.dark, s.size), (s.light, 120));
        let mut bad = spec("#FFFFFF");
        bad.logo_size = Some(500);
        assert!(validate(&bad).err().unwrap().contains("24..=288"));
        assert!(validate(&spec("white")).is_err());
    }

    #[test]
    fn translucent_background_warns_and_is_opaque_where_required() {
        let s = validate(&spec("#80FFFFFF")).unwrap();
        assert!(s.warnings.iter().any(|w| w.contains("opaque")), "{:?}", s.warnings);
        assert!(ios_colorset_json(s.light, s.dark).contains("\"alpha\" : \"1.000\""));
        assert!(android_colors_xml(s.light, true).contains("#FFFFFFFF"));
    }

    #[test]
    fn fit_keeps_aspect_and_never_upscales_the_box() {
        assert_eq!(fit(1000, 500, 120), (120, 60));
        assert_eq!(fit(500, 1000, 120), (60, 120));
        assert_eq!(fit(800, 800, 120), (120, 120));
        assert_eq!(fit(1, 1000, 120), (1, 120)); // never 0
    }

    #[test]
    fn generated_android_files() {
        let c = Rgba::parse("#1C1B1F").unwrap();
        let colors = android_colors_xml(c, true);
        assert!(colors.contains(HEADER));
        assert!(colors.contains(r#"<color name="mobiler_splash_background">#FF1C1B1F</color>"#));
        assert!(colors.contains(r#"<drawable name="mobiler_splash_icon">@mipmap/ic_launcher</drawable>"#));
        assert!(!android_colors_xml(c, false).contains("mobiler_splash_icon"));

        let plain = android_launch_xml(None);
        assert!(plain.contains(HEADER));
        assert!(plain.contains("@color/mobiler_splash_background") && !plain.contains("mobiler_splash_logo"));
        let with = android_launch_xml(Some((120, 60)));
        assert!(with.contains(r#"android:width="120dp""#) && with.contains(r#"android:height="60dp""#));
        assert!(with.contains(r#"android:gravity="center""#) && with.contains("@drawable/mobiler_splash_logo"));

        // Android 12 shows the icon in a 240dp box masked to a 160dp circle: fit inside its 113dp square.
        let icon = android_icon_xml(120, 60);
        assert!(icon.contains("<inset") && icon.contains("@drawable/mobiler_splash_logo"));
        assert!(icon.contains(r#"android:insetLeft="63.5dp""#), "{icon}"); // (240 - 113) / 2
        assert!(icon.contains(r#"android:insetTop="91.5dp""#), "{icon}"); // (240 - 57) / 2
        assert_eq!(android_theme_icon_lines(false), Vec::<String>::new());
        assert_eq!(
            android_theme_icon_lines(true),
            [r#"<item name="android:windowSplashScreenAnimatedIcon">@drawable/mobiler_splash_logo_icon</item>"#]
        );
    }

    #[test]
    fn generated_ios_and_web() {
        let (l, d) = (Rgba::parse("#FFFBFE").unwrap(), Rgba::parse("#1C1B1F").unwrap());
        let cs = ios_colorset_json(l, d);
        assert!(cs.contains("\"red\" : \"0xFF\"") && cs.contains("\"luminosity\"") && cs.contains("\"red\" : \"0x1C\""));
        let is = ios_imageset_json(true);
        assert!(is.contains("mobiler-splash-logo@3x.png") && is.contains("mobiler-splash-logo-dark@3x.png"));
        assert!(!ios_imageset_json(false).contains("dark"));
        assert_eq!(ios_plist_lines(true), ["UIImageName: MobilerSplashLogo"]);
        assert!(ios_plist_lines(false).is_empty());
        let web = web_lines(l, d, Some((120, 60)), true).join("\n");
        assert!(web.contains("prefers-color-scheme: dark") && web.contains("#FFFBFE") && web.contains("#1C1B1F"));
        assert!(web.contains("splash/mobiler-splash-logo.png") && web.contains("splash/mobiler-splash-logo-dark.png"));
        assert!(web.contains("body:empty") && web.contains("120px 60px"));
        assert!(web.contains(r#"rel="copy-dir" href="splash""#));
        let no_logo = web_lines(l, d, None, false).join("\n");
        assert!(!no_logo.contains("splash/") && no_logo.contains("#1C1B1F"));
    }

    // ---------------- sync ----------------

    use std::sync::atomic::{AtomicUsize, Ordering};
    static N: AtomicUsize = AtomicUsize::new(0);

    const THEME: &str = "<resources>\n    <style name=\"Base.Theme.Demo\" parent=\"android:Theme.Material.Light.NoActionBar\">\n        <item name=\"android:windowSplashScreenBackground\">@color/mobiler_splash_background</item>\n        <!-- mobiler:splash-begin -->\n        <!-- mobiler:splash-end -->\n    </style>\n</resources>\n";
    const PROJECT: &str = "targets:\n  Demo:\n    info:\n      properties:\n        UILaunchScreen:\n          UIColorName: MobilerSplashBackground\n          # mobiler:splash-begin\n          # mobiler:splash-end\n        UIRequiresFullScreen: true\n";

    /// A scaffolded app's splash-relevant files: the stock seeds, v31 themes and project.yml with markers.
    fn app(toml: &str) -> std::path::PathBuf {
        let root = std::env::temp_dir().join(format!("mob_splash_{}_{}", std::process::id(), N.fetch_add(1, Ordering::SeqCst)));
        let _ = std::fs::remove_dir_all(&root);
        for (rel, bytes) in stock_seeds() {
            put(&root, rel, bytes);
        }
        for t in THEMES {
            put(&root, t, THEME.as_bytes());
        }
        put(&root, PROJECT_YML, PROJECT.as_bytes());
        put(&root, "mobiler.toml", toml.as_bytes());
        root
    }

    fn put(root: &Path, rel: &str, bytes: &[u8]) {
        let p = root.join(rel);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, bytes).unwrap();
    }

    fn read(root: &Path, rel: &str) -> String {
        std::fs::read_to_string(root.join(rel)).unwrap()
    }

    fn png(root: &Path, rel: &str, w: u32, h: u32) {
        let p = root.join(rel);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        image::RgbaImage::from_pixel(w, h, image::Rgba([200, 30, 30, 255])).save(p).unwrap();
    }

    fn dims(root: &Path, rel: &str) -> (u32, u32) {
        image::image_dimensions(root.join(rel)).unwrap()
    }

    const LOGO_TOML: &str = "[splash]\nbackground = \"#FFFBFE\"\nbackground_dark = \"#1C1B1F\"\nlogo = \"assets/logo.png\"\nlogo_dark = \"assets/logo-dark.png\"\nlogo_size = 120\n";

    fn logo_app() -> std::path::PathBuf {
        let root = app(LOGO_TOML);
        png(&root, "assets/logo.png", 1200, 600);
        png(&root, "assets/logo-dark.png", 1200, 600);
        root
    }

    #[test]
    fn stock_seeds_are_the_template_and_have_no_placeholders() {
        let seeds = stock_seeds();
        assert_eq!(seeds.len(), crate::upgrade::SEED_PATHS.len());
        for (rel, bytes) in seeds {
            assert!(crate::upgrade::SEED_PATHS.contains(&rel), "{rel}");
            assert!(!String::from_utf8_lossy(bytes).contains("{{"), "{rel} has a placeholder");
        }
    }

    #[test]
    fn no_mobiler_toml_or_no_section_touches_nothing() {
        let root = app("[fonts]\n");
        let before = read(&root, COLORS[0]);
        let r = sync(&root).unwrap();
        assert!(r.written.is_empty() && r.removed.is_empty() && r.warnings.is_empty(), "{r:?}");
        assert_eq!(read(&root, COLORS[0]), before);
        std::fs::remove_file(root.join("mobiler.toml")).unwrap();
        assert!(sync(&root).unwrap().written.is_empty());
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn colours_only_rewrites_stock_seeds() {
        let root = app("[splash]\nbackground = \"#FFFBFE\"\nbackground_dark = \"#1C1B1F\"\n");
        let r = sync(&root).unwrap();
        assert!(r.warnings.is_empty(), "{:?}", r.warnings);
        assert!(read(&root, COLORS[0]).contains("#FFFFFBFE") && read(&root, COLORS[0]).contains(HEADER));
        assert!(read(&root, COLORS[1]).contains("#FF1C1B1F"));
        assert!(!read(&root, LAUNCH).contains("mobiler_splash_logo"));
        assert!(read(&root, COLORSET).contains("\"red\" : \"0x1C\""));
        assert!(!root.join(LOGO_PNG).exists() && !root.join(ICON).exists() && !root.join(IMAGESET).exists());
        assert!(!read(&root, THEMES[0]).contains("windowSplashScreenAnimatedIcon"));
        assert!(!read(&root, PROJECT_YML).contains("UIImageName"));
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn logo_writes_every_platform() {
        let root = logo_app();
        put(&root, WEB_INDEX, b"<html>\n<head>\n  <title>x</title>\n</head>\n<body></body>\n</html>\n");
        let r = sync(&root).unwrap();
        assert!(r.warnings.is_empty(), "{:?}", r.warnings);
        assert_eq!(dims(&root, LOGO_PNG), (480, 240));
        assert_eq!(dims(&root, LOGO_DARK_PNG), (480, 240));
        assert!(read(&root, LAUNCH).contains(r#"android:width="120dp""#) && read(&root, LAUNCH).contains(r#"android:height="60dp""#));
        assert!(read(&root, ICON).contains("<inset"));
        for t in THEMES {
            assert!(read(&root, t).contains("@drawable/mobiler_splash_logo_icon"), "{t}");
        }
        assert_eq!(dims(&root, &format!("{IMAGESET}/mobiler-splash-logo@3x.png")), (360, 180));
        assert!(root.join(format!("{IMAGESET}/mobiler-splash-logo-dark@3x.png")).exists());
        assert!(read(&root, &format!("{IMAGESET}/Contents.json")).contains("dark"));
        assert!(read(&root, PROJECT_YML).contains("          UIImageName: MobilerSplashLogo\n"), "{}", read(&root, PROJECT_YML));
        let html = read(&root, WEB_INDEX);
        assert!(html.contains("mobiler:splash-begin") && html.find("splash-begin").unwrap() < html.find("</head>").unwrap());
        assert_eq!(dims(&root, &format!("{WEB_DIR}/mobiler-splash-logo.png")), (240, 120)); // 2× for the web
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn small_logo_is_not_upscaled_and_warns() {
        let root = app("[splash]\nbackground = \"#FFFFFF\"\nlogo = \"assets/logo.png\"\n");
        png(&root, "assets/logo.png", 100, 100);
        let r = sync(&root).unwrap();
        assert_eq!(dims(&root, LOGO_PNG), (100, 100));
        assert!(r.warnings.iter().any(|w| w.contains("480")), "{:?}", r.warnings);
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn hand_edited_seed_is_left_alone_with_a_warning() {
        let root = app("[splash]\nbackground = \"#123456\"\n");
        let mine = "<resources><color name=\"mobiler_splash_background\">#FF000000</color></resources>\n";
        put(&root, COLORS[0], mine.as_bytes());
        let r = sync(&root).unwrap();
        assert_eq!(read(&root, COLORS[0]), mine);
        assert!(r.warnings.iter().any(|w| w.contains(COLORS[0]) && w.contains("edited by hand")), "{:?}", r.warnings);
        assert!(read(&root, COLORS[1]).contains("#FF123456"), "the other seeds are still written");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn second_sync_is_a_no_op() {
        let root = logo_app();
        sync(&root).unwrap();
        let r = sync(&root).unwrap();
        assert!(r.written.is_empty() && r.removed.is_empty(), "{r:?}");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn a_json_seed_the_sync_wrote_stays_its_own() {
        // JSON can't carry the header: the ledger recognises the colorset the sync wrote last time.
        let root = app("[splash]\nbackground = \"#FFFFFF\"\n");
        sync(&root).unwrap();
        put(&root, "mobiler.toml", b"[splash]\nbackground = \"#000000\"\n");
        let r = sync(&root).unwrap();
        assert!(read(&root, COLORSET).contains("\"red\" : \"0x00\""), "{r:?}");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn broken_logo_leaves_everything_unchanged() {
        let root = logo_app();
        sync(&root).unwrap();
        let snapshot = |root: &Path| -> Vec<(String, Vec<u8>)> {
            [COLORS[0], COLORS[1], LAUNCH, ICON, LOGO_PNG, COLORSET, PROJECT_YML, THEMES[0]]
                .iter()
                .map(|r| (r.to_string(), std::fs::read(root.join(r)).unwrap()))
                .collect()
        };
        let before = snapshot(&root);
        put(&root, "assets/logo.png", b"not a png");
        put(&root, "mobiler.toml", LOGO_TOML.replace("#FFFBFE", "#000000").as_bytes());
        let r = sync(&root).unwrap();
        assert!(r.warnings.iter().any(|w| w.contains("assets/logo.png")), "{:?}", r.warnings);
        assert_eq!(snapshot(&root), before, "nothing changed");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn removing_the_logo_undoes_logo_files() {
        let root = logo_app();
        put(&root, WEB_INDEX, b"<html>\n<head>\n</head>\n<body></body>\n</html>\n");
        sync(&root).unwrap();
        put(&root, "mobiler.toml", b"[splash]\nbackground = \"#FFFBFE\"\n");
        let r = sync(&root).unwrap();
        for gone in [LOGO_PNG, LOGO_DARK_PNG, ICON, IMAGESET, WEB_DIR] {
            assert!(!root.join(gone).exists(), "{gone} still there ({r:?})");
        }
        assert!(!read(&root, LAUNCH).contains("mobiler_splash_logo"));
        assert!(!read(&root, THEMES[1]).contains("windowSplashScreenAnimatedIcon"));
        assert!(!read(&root, PROJECT_YML).contains("UIImageName"));
        assert!(read(&root, COLORS[0]).contains("#FFFFFBFE"));
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn removing_splash_undoes_the_sync_but_keeps_colours() {
        let root = logo_app();
        put(&root, WEB_INDEX, b"<html>\n<head>\n</head>\n<body></body>\n</html>\n");
        sync(&root).unwrap();
        put(&root, "mobiler.toml", b"");
        let r = sync(&root).unwrap();
        assert!(!r.removed.is_empty());
        for gone in [LOGO_PNG, ICON, IMAGESET, WEB_DIR, LEDGER] {
            assert!(!root.join(gone).exists(), "{gone}");
        }
        assert!(!read(&root, WEB_INDEX).contains("mobiler:splash"), "web block removed");
        assert!(!read(&root, LAUNCH).contains("mobiler_splash_logo"), "launch back to colour-only");
        assert!(read(&root, COLORS[1]).contains("#FF1C1B1F"), "colours stay");
        assert!(sync(&root).unwrap().removed.is_empty(), "undo happens once");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn missing_markers_warn_and_colours_still_sync() {
        let root = logo_app();
        put(&root, PROJECT_YML, b"targets:\n  Demo:\n    info:\n      properties:\n        UILaunchScreen:\n          UIColorName: MobilerSplashBackground\n");
        put(&root, THEMES[0], b"<resources>\n</resources>\n");
        let r = sync(&root).unwrap();
        assert!(r.warnings.iter().any(|w| w.contains("mobiler upgrade --apply")), "{:?}", r.warnings);
        assert!(read(&root, COLORS[0]).contains("#FFFFFBFE"));
        assert!(!read(&root, PROJECT_YML).contains("UIImageName"), "never edits a framework file without markers");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn crlf_files_keep_their_endings() {
        let root = logo_app();
        put(&root, PROJECT_YML, PROJECT.replace('\n', "\r\n").as_bytes());
        sync(&root).unwrap();
        let yml = read(&root, PROJECT_YML);
        assert!(yml.contains("UIImageName: MobilerSplashLogo\r\n"));
        assert!(!yml.replace("\r\n", "").contains('\n'), "every line still CRLF");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn huge_and_odd_named_logo() {
        let root = app("[splash]\nbackground = \"#FFFFFF\"\nlogo = \"assets/logo ć.png\"\n");
        png(&root, "assets/logo ć.png", 4000, 4000);
        let r = sync(&root).unwrap();
        assert!(r.warnings.is_empty(), "{:?}", r.warnings);
        assert_eq!(dims(&root, LOGO_PNG), (480, 480));
        let _ = std::fs::remove_dir_all(&root);
    }

}
