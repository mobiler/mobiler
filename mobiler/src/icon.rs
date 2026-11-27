//! `mobiler.toml` `[icon]`: the app's launcher icon, generated for the Android, iOS and web shells
//! from one square foreground PNG and a background colour. This module validates the section and holds
//! the pure image/XML generators, and the sync that writes them into an app's shells (owning only the
//! files it wrote, undoing itself when the section is removed).

use std::path::Path;

use image::RgbaImage;
use serde::Deserialize;

/// Marks a file the sync wrote, so a later sync may rewrite it.
pub const HEADER: &str = "generated from mobiler.toml [icon]; edit mobiler.toml, not this file";
/// Android's 108 dp adaptive layer per density.
pub const LAYER_SIZES: [(&str, u32); 5] =
    [("mdpi", 108), ("hdpi", 162), ("xhdpi", 216), ("xxhdpi", 324), ("xxxhdpi", 432)];
/// The legacy (pre-adaptive launcher) icon per density.
pub const LEGACY_SIZES: [(&str, u32); 5] =
    [("mdpi", 48), ("hdpi", 72), ("xhdpi", 96), ("xxhdpi", 144), ("xxxhdpi", 192)];
/// The web favicon and apple-touch-icon.
pub const WEB_SIZES: [u32; 2] = [32, 180];
pub const DEFAULT_PADDING: u32 = 18;
const MIN_PX: u32 = 1024;
const MAX_PX: u32 = 16_384;
/// The working copy's largest side: well above the largest output content box (655 px at 1024), so the
/// ~20 resizes per sync start from this, never from a source of up to 16384 px.
const WORK_PX: u32 = 2048;

/// `[icon]` as written in mobiler.toml.
#[derive(Deserialize, Debug, Clone)]
#[serde(deny_unknown_fields)]
pub struct IconSpec {
    pub foreground: String,
    pub background: String,
    pub monochrome: Option<String>,
    pub padding: Option<u32>,
}

/// A validated `[icon]`: the sources as premultiplied RGBA8 working copies (at most `WORK_PX` per
/// side; see [`working_copy`]) and the settings. Built only through [`Icon::new`], so the generators
/// always get premultiplied copies.
#[derive(Debug, Clone)]
pub struct Icon {
    /// Premultiplied working copy of the foreground.
    fg: RgbaImage,
    /// Premultiplied working copy of the monochrome image.
    mono: Option<RgbaImage>,
    bg: [u8; 3],
    /// % of the visible icon's side left clear on each side, 0..=40.
    padding: u32,
}

impl Icon {
    /// From straight-alpha RGBA8 sources: each becomes its premultiplied working copy.
    pub fn new(fg: RgbaImage, mono: Option<RgbaImage>, bg: [u8; 3], padding: u32) -> Icon {
        Icon { fg: working_copy(fg), mono: mono.map(working_copy), bg, padding }
    }
}

/// Check `[icon]` and decode its images. Every failure names the field and the reason; nothing is
/// written by the caller unless this succeeds.
pub fn validate(root: &Path, spec: &IconSpec) -> Result<Icon, String> {
    let (bg, padding) = check_settings(spec)?;
    let fg = load(root, "foreground", &spec.foreground)?;
    let mono = spec.monochrome.as_deref().map(|rel| load(root, "monochrome", rel)).transpose()?;
    Ok(Icon::new(fg, mono, bg, padding))
}

/// `[icon]`'s settings (no image read): the background as RGB and the padding.
fn check_settings(spec: &IconSpec) -> Result<([u8; 3], u32), String> {
    let padding = spec.padding.unwrap_or(DEFAULT_PADDING);
    if padding > 40 {
        return Err(format!("[icon] padding {padding}: must be in 0..=40 (% left clear on each side)"));
    }
    let bad_bg = || format!("[icon] background {:?}: a colour is #RRGGBB", spec.background);
    if spec.background.len() != 7 {
        return Err(bad_bg());
    }
    let c = crate::splash::Rgba::parse(&spec.background).map_err(|_| bad_bg())?;
    Ok(([c.r, c.g, c.b], padding))
}

/// A square PNG, 1024..=16384 px, decoded once to straight-alpha RGBA8 (palette, grayscale and 16-bit
/// inputs included). The size is checked from the header first, so an oversized file is never decoded.
fn load(root: &Path, field: &str, rel: &str) -> Result<RgbaImage, String> {
    #[cfg(test)]
    tests::DECODES.with(|n| n.set(n.get() + 1));
    check_header(root, field, rel)?;
    let err = |why: String| format!("[icon] {field} {rel:?}: {why}");
    let mut reader = image::ImageReader::open(root.join(rel)).map_err(|e| err(format!("can't read it ({e})")))?;
    reader.set_format(image::ImageFormat::Png);
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(MAX_PX);
    limits.max_image_height = Some(MAX_PX);
    limits.max_alloc = None; // a 16384 px RGBA source is 1 GiB; the side limit bounds it
    reader.limits(limits);
    let img = reader.decode().map_err(|e| err(format!("not a readable PNG ({e})")))?;
    Ok(img.into_rgba8())
}

/// A square PNG, 1024..=16384 px, judged from its first 24 bytes (signature and IHDR) only: its side.
fn check_header(root: &Path, field: &str, rel: &str) -> Result<u32, String> {
    use std::io::Read;
    let err = |why: String| format!("[icon] {field} {rel:?}: {why}");
    let mut head = [0u8; 24];
    let mut f = std::fs::File::open(root.join(rel)).map_err(|e| err(format!("can't read it ({e})")))?;
    match f.read_exact(&mut head) {
        Ok(()) => {}
        Err(e) if e.kind() == std::io::ErrorKind::UnexpectedEof => return Err(err("not a PNG".into())),
        Err(e) => return Err(err(format!("can't read it ({e})"))),
    }
    if &head[..8] != b"\x89PNG\r\n\x1a\n" {
        return Err(err("not a PNG".into()));
    }
    if &head[12..16] != b"IHDR" {
        return Err(err("not a readable PNG (no IHDR header)".into()));
    }
    let w = u32::from_be_bytes([head[16], head[17], head[18], head[19]]);
    let h = u32::from_be_bytes([head[20], head[21], head[22], head[23]]);
    if w > MAX_PX || h > MAX_PX {
        return Err(err(format!("{w}×{h} px; at most {MAX_PX} px per side")));
    }
    if w != h {
        return Err(err(format!("{w}×{h} px; it must be square")));
    }
    if w < MIN_PX {
        return Err(err(format!("{w}×{h} px; at least {MIN_PX}×{MIN_PX} px")));
    }
    Ok(w)
}

/// `img` premultiplied (so transparent pixels never bleed their colour into an edge when resampled)
/// and, when larger than `WORK_PX`, downsized once (Lanczos3) to `WORK_PX`. Never upscaled.
fn working_copy(mut img: RgbaImage) -> RgbaImage {
    for p in img.pixels_mut() {
        let a = u16::from(p.0[3]);
        for c in &mut p.0[..3] {
            *c = premul(*c, a);
        }
    }
    let (w, h) = img.dimensions();
    if w.max(h) <= WORK_PX {
        return img;
    }
    let (nw, nh) = crate::splash::fit(w, h, WORK_PX);
    lanczos(img, nw, nh)
}

/// The content box's side: `px * visible * (1 - 2 * padding / 100)`, at least 1.
#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)] // positive and at most `px`
fn content_side(px: u32, visible: f64, padding: u32) -> u32 {
    let side = f64::from(px) * visible * f64::from(100 - 2 * padding.min(40)) / 100.0;
    (side.round() as u32).clamp(1, px)
}

/// A premultiplied working copy resized (Lanczos3) to `side`²; still premultiplied.
fn resized(work: &RgbaImage, side: u32) -> RgbaImage {
    lanczos(work.clone(), side, side)
}

/// Lanczos3 resize, through `DynamicImage` so the resampler is the one compiled inside `image` (the
/// workspace optimises that crate even in dev and test builds; a generic `imageops::resize` call would
/// be compiled here, unoptimised, and take ~1 s per icon in tests).
fn lanczos(img: RgbaImage, w: u32, h: u32) -> RgbaImage {
    image::DynamicImage::ImageRgba8(img).resize_exact(w, h, image::imageops::FilterType::Lanczos3).into_rgba8()
}

#[allow(clippy::cast_possible_truncation)] // ≤ 255
fn premul(c: u8, a: u16) -> u8 {
    ((u16::from(c) * a + 127) / 255) as u8
}

#[allow(clippy::cast_possible_truncation)] // clamped to 255
fn unpremul(c: u8, a: u8) -> u8 {
    if a == 0 {
        return 0;
    }
    ((u32::from(c) * 255 + u32::from(a) / 2) / u32::from(a)).min(255) as u8
}

/// The Android 108 dp adaptive layer at `px`: transparent, with `img` (a premultiplied working copy:
/// `icon.fg` or `icon.mono`) centred in the 72 dp visible area at the icon's padding.
pub fn layer(icon: &Icon, img: &RgbaImage, px: u32) -> RgbaImage {
    let side = content_side(px, 72.0 / 108.0, icon.padding);
    let fitted = resized(img, side);
    let off = (px - side) / 2;
    let mut out = RgbaImage::new(px, px);
    for (x, y, p) in fitted.enumerate_pixels() {
        let [r, g, b, a] = p.0;
        out.put_pixel(x + off, y + off, image::Rgba([unpremul(r, a), unpremul(g, a), unpremul(b, a), a]));
    }
    out
}

/// The whole icon at `px`: background-filled square with the foreground centred at the padding (the
/// visible area is the whole square). `round` masks it to a circle with an anti-aliased edge.
pub fn composite(icon: &Icon, px: u32, round: bool) -> RgbaImage {
    let side = content_side(px, 1.0, icon.padding);
    let fitted = resized(&icon.fg, side);
    let off = (px - side) / 2;
    let [br, bgc, bb] = icon.bg;
    let mut out = RgbaImage::from_pixel(px, px, image::Rgba([br, bgc, bb, 255]));
    for (x, y, p) in fitted.enumerate_pixels() {
        let inv = 255 - u16::from(p.0[3]);
        let o = out.get_pixel_mut(x + off, y + off);
        for i in 0..3 {
            // Source-over, premultiplied: fg + bg × (1 − a).
            o.0[i] = p.0[i].saturating_add(premul(o.0[i], inv));
        }
    }
    if round {
        mask_circle(&mut out);
    }
    out
}

/// Scale each pixel's alpha by how much of it lies inside the inscribed circle.
#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)] // coverage × alpha is in 0..=255
fn mask_circle(img: &mut RgbaImage) {
    let r = f64::from(img.width()) / 2.0;
    for (x, y, p) in img.enumerate_pixels_mut() {
        let (dx, dy) = (f64::from(x) + 0.5 - r, f64::from(y) + 0.5 - r);
        let coverage = (r - dx.hypot(dy) + 0.5).clamp(0.0, 1.0);
        p.0[3] = (f64::from(p.0[3]) * coverage).round() as u8;
    }
}

/// iOS's `icon-1024.png`: the 1024 px composite, RGB with no alpha channel (App Store rejects alpha).
pub fn ios_png(icon: &Icon) -> Vec<u8> {
    let rgb = image::DynamicImage::ImageRgba8(composite(icon, 1024, false)).to_rgb8();
    encode(&image::DynamicImage::ImageRgb8(rgb), image::ImageFormat::Png)
}

/// `img` as PNG.
pub fn png(img: &RgbaImage) -> Vec<u8> {
    encode(&image::DynamicImage::ImageRgba8(img.clone()), image::ImageFormat::Png)
}

/// `img` as lossless WebP (Android's `ic_launcher*.webp`).
pub fn webp(img: &RgbaImage) -> Vec<u8> {
    use image::ImageEncoder;
    let mut out = Vec::new();
    image::codecs::webp::WebPEncoder::new_lossless(&mut out)
        .write_image(img.as_raw(), img.width(), img.height(), image::ExtendedColorType::Rgba8)
        .expect("encoding an in-memory RGBA8 image as WebP");
    out
}

fn encode(img: &image::DynamicImage, format: image::ImageFormat) -> Vec<u8> {
    let mut out = std::io::Cursor::new(Vec::new());
    img.write_to(&mut out, format).expect("encoding an in-memory image");
    out.into_inner()
}

/// `mipmap-anydpi/ic_launcher.xml` and `ic_launcher_round.xml` (the same content). Without a
/// monochrome image the themed icon uses the foreground layer (Android themes by its alpha).
pub fn android_adaptive_xml(has_mono: bool) -> String {
    let mono = if has_mono { "mobiler_icon_monochrome" } else { "mobiler_icon_foreground" };
    format!(
        "<?xml version=\"1.0\" encoding=\"utf-8\"?>\n<!-- {HEADER} -->\n<adaptive-icon xmlns:android=\"http://schemas.android.com/apk/res/android\">\n    <background android:drawable=\"@color/mobiler_icon_background\"/>\n    <foreground android:drawable=\"@mipmap/mobiler_icon_foreground\"/>\n    <monochrome android:drawable=\"@mipmap/{mono}\"/>\n</adaptive-icon>\n"
    )
}

/// `values/mobiler_icon.xml`: the adaptive icon's background colour.
pub fn colors_xml(bg: [u8; 3]) -> String {
    format!(
        "<?xml version=\"1.0\" encoding=\"utf-8\"?>\n<!-- {HEADER} -->\n<resources>\n    <color name=\"mobiler_icon_background\">#{:02X}{:02X}{:02X}</color>\n</resources>\n",
        bg[0], bg[1], bg[2]
    )
}

/// The `mobiler:icon` block in `web/index.html`: trunk copies `web/icon`, the favicon and the
/// apple-touch-icon. Empty without `[icon]`.
pub fn web_lines(on: bool) -> Vec<String> {
    if !on {
        return Vec::new();
    }
    vec![
        r#"<link data-trunk rel="copy-dir" href="icon"/>"#.into(),
        r#"<link rel="icon" type="image/png" sizes="32x32" href="icon/mobiler-icon-32.png"/>"#.into(),
        r#"<link rel="apple-touch-icon" href="icon/mobiler-icon-180.png"/>"#.into(),
    ]
}

/// FNV-1a 64 of the exact bytes (no CRLF folding: these are binaries), for `.mobiler/icon.json`.
pub fn fnv1a64_hex(bytes: &[u8]) -> String {
    let mut h = Fnv::default();
    h.write(bytes);
    h.hex()
}

/// FNV-1a 64, fed in pieces.
struct Fnv(u64);

impl Default for Fnv {
    fn default() -> Fnv {
        Fnv(0xcbf2_9ce4_8422_2325)
    }
}

impl Fnv {
    fn write(&mut self, bytes: &[u8]) {
        for b in bytes {
            self.0 ^= u64::from(*b);
            self.0 = self.0.wrapping_mul(0x0000_0100_0000_01b3);
        }
    }

    /// A length-prefixed piece, so two inputs can't run into each other.
    fn piece(&mut self, bytes: &[u8]) {
        self.write(&(bytes.len() as u64).to_le_bytes());
        self.write(bytes);
    }

    fn hex(&self) -> String {
        format!("{:016x}", self.0)
    }
}

// ---------------- sync ----------------

use std::collections::BTreeMap;
use std::fs;

use anyhow::Context;

const RES: &str = "Android/app/src/main/res";
/// The adaptive launcher icons: upgrade seeds (ADR-0042), so `mobiler upgrade` never merges them.
pub(crate) const ANYDPI: [&str; 2] = [
    "Android/app/src/main/res/mipmap-anydpi/ic_launcher.xml",
    "Android/app/src/main/res/mipmap-anydpi/ic_launcher_round.xml",
];
const COLORS: &str = "Android/app/src/main/res/values/mobiler_icon.xml";
const IOS_PNG: &str = "iOS/Sources/Assets.xcassets/AppIcon.appiconset/icon-1024.png";
const WEB_INDEX: &str = "web/index.html";
const WEB_DIR: &str = "web/icon";
/// What the sync last wrote to each file (FNV-1a 64 of its bytes) and the inputs it wrote them from.
pub(crate) const LEDGER: &str = ".mobiler/icon.json";
const WEB_BEGIN: &str = "<!-- mobiler:icon-begin -->";
const WEB_END: &str = "<!-- mobiler:icon-end -->";
/// Bump whenever the generated files change for the same inputs: the unchanged-input skip then
/// regenerates them once.
const GENERATOR_VERSION: u32 = 1;

fn mipmap(density: &str, file: &str) -> String {
    format!("{RES}/mipmap-{density}/{file}")
}

fn web_png(px: u32) -> String {
    format!("{WEB_DIR}/mobiler-icon-{px}.png")
}

#[derive(Debug, Default)]
pub struct SyncReport {
    pub written: Vec<String>,
    pub removed: Vec<String>,
    pub warnings: Vec<String>,
}

#[derive(Deserialize, Default)]
struct Manifest {
    icon: Option<IconSpec>,
}

/// The ledger: whether `[icon]` was active at the last sync, the inputs it synced from, and what it
/// last wrote to each file. Kept (inactive) when the section is removed.
///
/// A file normally has one hash. While a sync writes, each file has two (what it held, what it gets):
/// that ledger is written first, so a sync that stops part-way still owns every file it touched, and
/// the next one finishes the job. It has no `input`, so that next sync never skips.
#[derive(Default)]
struct Ledger {
    active: bool,
    input: Option<String>,
    files: BTreeMap<String, Vec<String>>,
}

impl Ledger {
    /// The sync wrote these exact bytes to `rel`.
    fn holds(&self, rel: &str, bytes: &[u8]) -> bool {
        self.files.get(rel).is_some_and(|hs| hs.contains(&fnv1a64_hex(bytes)))
    }

    /// This ledger's entries for the files under `prefix`.
    fn under<'a>(&'a self, prefix: &'a str) -> impl Iterator<Item = (String, Vec<String>)> + 'a {
        self.files.iter().filter(move |(k, _)| k.starts_with(prefix)).map(|(k, v)| (k.clone(), v.clone()))
    }
}

fn read_ledger(root: &Path) -> Option<Ledger> {
    let text = fs::read_to_string(root.join(LEDGER)).ok()?;
    let v: serde_json::Value = serde_json::from_str(&text).ok()?;
    let hashes = |v: &serde_json::Value| -> Vec<String> {
        match v {
            serde_json::Value::String(s) => vec![s.clone()],
            serde_json::Value::Array(a) => a.iter().filter_map(|h| h.as_str().map(str::to_string)).collect(),
            _ => Vec::new(),
        }
    };
    let files = v["files"].as_object().map(|m| m.iter().map(|(k, v)| (k.clone(), hashes(v))).collect()).unwrap_or_default();
    Some(Ledger { active: v["active"].as_bool().unwrap_or(true), input: v["input"].as_str().map(str::to_string), files })
}

/// One hash is written as a string (the usual form), several as an array.
fn ledger_json(l: &Ledger) -> anyhow::Result<Vec<u8>> {
    let files: serde_json::Map<String, serde_json::Value> = l
        .files
        .iter()
        .map(|(k, hs)| {
            let v = match hs.as_slice() {
                [h] => serde_json::Value::String(h.clone()),
                _ => serde_json::Value::Array(hs.iter().cloned().map(serde_json::Value::String).collect()),
            };
            (k.clone(), v)
        })
        .collect();
    let mut v = serde_json::json!({ "active": l.active, "files": files });
    if let Some(input) = &l.input {
        v["input"] = serde_json::Value::String(input.clone());
    }
    Ok((serde_json::to_string_pretty(&v)? + "\n").into_bytes())
}

/// One change to the app, computed before anything is written (so a bad input writes nothing).
enum Change {
    Write(String, Vec<u8>),
    Remove(String),
    /// A directory the sync shares with the app (`web/icon`): removed only once it is empty.
    RemoveDirIfEmpty(String),
}

/// The template's bytes at `rel` (these paths carry no placeholders).
fn stock(rel: &str) -> Option<&'static [u8]> {
    crate::upgrade::TEMPLATES.get_file(rel).map(include_dir::File::contents)
}

fn pron(n: usize) -> &'static str {
    if n == 1 { "it" } else { "them" }
}

/// XML can carry the sync's header; the images can't.
fn is_xml(rel: &str) -> bool {
    Path::new(rel).extension().is_some_and(|e| e.eq_ignore_ascii_case("xml"))
}

/// The sync may write (or remove) `rel`: it is missing, the template's stock content (a text file read
/// CRLF-tolerant), an XML file carrying the sync's header, or exactly what the ledger recorded.
fn owned(root: &Path, rel: &str, ledger: &Ledger) -> bool {
    let path = root.join(rel);
    if path.symlink_metadata().is_err() {
        return true;
    }
    let Ok(bytes) = fs::read(&path) else { return false };
    let is_stock = stock(rel).is_some_and(|s| {
        if crate::templating::is_binary(Path::new(rel)) {
            s == bytes.as_slice()
        } else {
            crate::upgrade::fingerprint(s) == crate::upgrade::fingerprint(&bytes)
        }
    });
    is_stock
        || (is_xml(rel) && String::from_utf8_lossy(&bytes).contains(HEADER))
        || ledger.holds(rel, &bytes)
}

/// One platform's share of the sync: one ownership unit.
struct Unit {
    name: &'static str,
    /// Path prefix of its files (for the ledger entries kept when it is skipped).
    prefix: &'static str,
    /// Images, written first.
    assets: Vec<(String, Vec<u8>)>,
    /// Files that refer to the images, written after them.
    refs: Vec<(String, Vec<u8>)>,
    /// The sync's own files this config no longer has, removed after the references.
    removes: Vec<String>,
}

/// The paths `[icon]` writes in this app (no image decoded), per platform present.
fn targets(root: &Path, has_mono: bool) -> Vec<String> {
    let mut out = Vec::new();
    if root.join("Android").is_dir() {
        for (d, _) in LAYER_SIZES {
            out.push(mipmap(d, "mobiler_icon_foreground.png"));
            if has_mono {
                out.push(mipmap(d, "mobiler_icon_monochrome.png"));
            }
        }
        for (d, _) in LEGACY_SIZES {
            out.push(mipmap(d, "ic_launcher.webp"));
            out.push(mipmap(d, "ic_launcher_round.webp"));
        }
        out.push(COLORS.into());
        out.extend(ANYDPI.map(String::from));
    }
    if root.join("iOS").is_dir() {
        out.push(IOS_PNG.into());
    }
    if root.join(WEB_INDEX).is_file() {
        out.extend(WEB_SIZES.map(web_png));
    }
    out
}

/// The inputs' fingerprint: the source images' bytes, the settings and the generator's version. `None`
/// when a source can't be read (validation then names it).
fn input_fingerprint(root: &Path, spec: &IconSpec) -> Option<String> {
    let mut h = Fnv::default();
    h.piece(&GENERATOR_VERSION.to_le_bytes());
    h.piece(&fs::read(root.join(&spec.foreground)).ok()?);
    match &spec.monochrome {
        Some(m) => h.piece(&fs::read(root.join(m)).ok()?),
        None => h.piece(b"no monochrome"),
    }
    h.piece(spec.background.to_ascii_uppercase().as_bytes());
    h.piece(&spec.padding.unwrap_or(DEFAULT_PADDING).to_le_bytes());
    Some(h.hex())
}

/// The web page with the icon block filled (`on`) or emptied; `Err` names why it can't be.
fn web_page(html: &str, on: bool) -> Result<String, &'static str> {
    let lines = web_lines(on);
    crate::fonts::replace_block(html, WEB_BEGIN, WEB_END, "</head>", |indent| {
        lines.iter().map(|l| format!("{indent}{l}")).collect()
    })
}

/// Nothing to do: the same inputs as last time, and every file still what the sync wrote.
fn up_to_date(root: &Path, ledger: &Ledger, input: &str, has_mono: bool) -> bool {
    ledger.active
        && ledger.input.as_deref() == Some(input)
        && targets(root, has_mono).iter().all(|rel| {
            fs::read(root.join(rel)).is_ok_and(|b| ledger.holds(rel, &b))
        })
        && fs::read_to_string(root.join(WEB_INDEX)).map_or(true, |html| web_page(&html, true).is_ok_and(|t| t == html))
}

/// Sync `mobiler.toml` `[icon]` into the shells. No section and nothing synced before: touches nothing.
/// A bad config or image: a warning, nothing written. A platform with a file the sync doesn't own: a
/// warning naming it, and that platform is left alone (a half-branded icon is worse than none).
pub fn sync(root: &Path) -> anyhow::Result<SyncReport> {
    let mut report = SyncReport::default();
    let spec = match read_spec(root) {
        Ok(s) => s,
        Err(e) => {
            report.warnings.push(format!("mobiler.toml {e}"));
            return Ok(report);
        }
    };
    let ledger = read_ledger(root);
    let Some(spec) = spec else {
        if let Some(ledger) = ledger.filter(|l| l.active) {
            undo(root, &ledger, &mut report)?;
        }
        return Ok(report);
    };
    let ledger = ledger.unwrap_or_default();
    let input = input_fingerprint(root, &spec);
    if input.as_deref().is_some_and(|i| up_to_date(root, &ledger, i, spec.monochrome.is_some())) {
        return Ok(report);
    }
    let icon = match validate(root, &spec) {
        Ok(i) => i,
        Err(e) => {
            report.warnings.push(format!("{e} — icon not synced"));
            return Ok(report);
        }
    };

    let mut files = BTreeMap::new();
    let units = units(root, &icon, &ledger, &mut files, &mut report);

    // Order matters if a write fails part-way: images before the files that refer to them, those
    // before the removals, the ledger last. The ledger is also written first, naming both what each
    // file held and what it gets, so a sync that stops part-way still owns what it wrote.
    let mut pending = BTreeMap::new();
    let (mut assets, mut refs, mut removes) = (Vec::new(), Vec::new(), Vec::new());
    for unit in units {
        let foreign: Vec<&str> = unit
            .assets
            .iter()
            .chain(&unit.refs)
            .map(|(rel, _)| rel.as_str())
            .filter(|rel| *rel != WEB_INDEX)
            .chain(unit.removes.iter().map(String::as_str))
            .filter(|rel| !owned(root, rel, &ledger))
            .collect();
        if !foreign.is_empty() {
            report.warnings.push(format!(
                "{} {} edited by hand (not the template's and not [icon]'s) — the {} icon is not synced; delete or restore {} to let [icon] manage it",
                foreign.join(", "),
                if foreign.len() == 1 { "was" } else { "were" },
                unit.name,
                pron(foreign.len()),
            ));
            files.extend(ledger.under(unit.prefix));
            continue;
        }
        for (rel, bytes) in unit.assets.iter().chain(&unit.refs) {
            if rel != WEB_INDEX {
                let new = fnv1a64_hex(bytes);
                // What it holds now (if the ledger is how the sync owns it) and what it gets.
                let mut both = ledger.files.get(rel).cloned().unwrap_or_default();
                if !both.contains(&new) {
                    both.push(new.clone());
                }
                pending.insert(rel.clone(), both);
                files.insert(rel.clone(), vec![new]);
            }
        }
        for rel in &unit.removes {
            if let Some(old) = ledger.files.get(rel) {
                pending.insert(rel.clone(), old.clone());
            }
        }
        assets.extend(unit.assets.into_iter().map(|(r, b)| Change::Write(r, b)));
        refs.extend(unit.refs.into_iter().map(|(r, b)| Change::Write(r, b)));
        removes.extend(unit.removes.into_iter().map(Change::Remove));
    }
    let mut interim = Ledger { active: true, input: None, files: files.clone() };
    interim.files.extend(pending);
    let mut changes = vec![Change::Write(LEDGER.into(), ledger_json(&interim)?)];
    changes.extend(assets);
    changes.extend(refs);
    changes.extend(removes);
    changes.push(Change::Write(LEDGER.into(), ledger_json(&Ledger { active: true, input, files })?));
    apply(root, changes, &mut report)?;
    Ok(report)
}

/// The platforms this app has, each with what `[icon]` writes there. A web page whose block can't be
/// filled is left out with a warning, its ledger entries carried into `files`.
fn units(root: &Path, icon: &Icon, ledger: &Ledger, files: &mut BTreeMap<String, Vec<String>>, report: &mut SyncReport) -> Vec<Unit> {
    let mut units = Vec::new();
    if root.join("Android").is_dir() {
        units.push(android_unit(icon));
    }
    if root.join("iOS").is_dir() {
        units.push(Unit {
            name: "iOS",
            prefix: "iOS/",
            assets: vec![(IOS_PNG.into(), ios_png(icon))],
            refs: Vec::new(),
            removes: Vec::new(),
        });
    }
    if let Ok(html) = fs::read_to_string(root.join(WEB_INDEX)) {
        match web_page(&html, true) {
            Ok(page) => units.push(Unit {
                name: "web",
                prefix: "web/",
                assets: WEB_SIZES.iter().map(|&px| (web_png(px), png(&composite(icon, px, false)))).collect(),
                refs: vec![(WEB_INDEX.into(), page.into_bytes())],
                removes: Vec::new(),
            }),
            Err(e) => {
                report.warnings.push(format!("{WEB_INDEX}: {e} — web icon not synced"));
                files.extend(ledger.under("web/"));
            }
        }
    }
    units
}

/// Android: the adaptive layers and the legacy rasters, then the colour and the adaptive icon XML that
/// refer to them. Without a monochrome image its layers go (after the XML stops naming them).
fn android_unit(icon: &Icon) -> Unit {
    let mut assets = Vec::new();
    let mut removes = Vec::new();
    for (d, px) in LAYER_SIZES {
        assets.push((mipmap(d, "mobiler_icon_foreground.png"), png(&layer(icon, &icon.fg, px))));
        match &icon.mono {
            Some(m) => assets.push((mipmap(d, "mobiler_icon_monochrome.png"), png(&layer(icon, m, px)))),
            None => removes.push(mipmap(d, "mobiler_icon_monochrome.png")),
        }
    }
    for (d, px) in LEGACY_SIZES {
        let square = composite(icon, px, false);
        let mut round = square.clone();
        mask_circle(&mut round);
        assets.push((mipmap(d, "ic_launcher.webp"), webp(&square)));
        assets.push((mipmap(d, "ic_launcher_round.webp"), webp(&round)));
    }
    let mut refs = vec![(COLORS.to_string(), colors_xml(icon.bg).into_bytes())];
    let xml = android_adaptive_xml(icon.mono.is_some());
    refs.extend(ANYDPI.map(|rel| (rel.to_string(), xml.clone().into_bytes())));
    Unit { name: "Android", prefix: "Android/", assets, refs, removes }
}

/// `[icon]` removed after a sync: each file still as the sync wrote it goes back to the template's (the
/// adaptive icon XML first, so nothing refers to the sync's layers), the sync's own files are removed,
/// the web block is emptied. The ledger stays, inactive.
fn undo(root: &Path, ledger: &Ledger, report: &mut SyncReport) -> anyhow::Result<()> {
    let mut files = ledger.files.clone();
    let as_written = |rel: &str| {
        fs::read(root.join(rel))
            .is_ok_and(|b| ledger.holds(rel, &b) || (is_xml(rel) && String::from_utf8_lossy(&b).contains(HEADER)))
    };
    let mut changes = Vec::new();
    let mut android_still_used = None;
    for rel in ANYDPI {
        if as_written(rel) {
            if let Some(s) = stock(rel) {
                changes.push(Change::Write(rel.into(), s.to_vec()));
                files.remove(rel);
            }
        } else if fs::read_to_string(root.join(rel)).is_ok_and(|t| t.contains("mobiler_icon_")) {
            android_still_used = Some(rel);
        }
    }
    if let Ok(html) = fs::read_to_string(root.join(WEB_INDEX))
        && html.contains(WEB_BEGIN)
    {
        match web_page(&html, false) {
            Ok(page) => changes.push(Change::Write(WEB_INDEX.into(), page.into_bytes())),
            Err(e) => report.warnings.push(format!("{WEB_INDEX}: {e} — its icon block is left as it is")),
        }
    }
    let rasters = LEGACY_SIZES
        .iter()
        .flat_map(|(d, _)| [mipmap(d, "ic_launcher.webp"), mipmap(d, "ic_launcher_round.webp")])
        .chain([IOS_PNG.to_string()]);
    for rel in rasters {
        if as_written(&rel)
            && let Some(s) = stock(&rel)
        {
            changes.push(Change::Write(rel.clone(), s.to_vec()));
            files.remove(&rel);
        }
    }
    let mut own: Vec<String> = WEB_SIZES.map(web_png).to_vec();
    if let Some(rel) = android_still_used {
        report.warnings.push(format!(
            "{rel} (edited by hand) still uses the mobiler_icon_* resources [icon] no longer provides — change it, then delete the mipmap-*/mobiler_icon_*.png files and {COLORS}"
        ));
    } else {
        own.push(COLORS.into());
        for (d, _) in LAYER_SIZES {
            own.push(mipmap(d, "mobiler_icon_foreground.png"));
            own.push(mipmap(d, "mobiler_icon_monochrome.png"));
        }
    }
    // Only what is still as the sync wrote it: a layer retouched by hand stays, and says so.
    let mut kept = Vec::new();
    for rel in own {
        if owned(root, &rel, ledger) {
            files.remove(&rel);
            changes.push(Change::Remove(rel));
        } else {
            kept.push(rel);
        }
    }
    if !kept.is_empty() {
        report.warnings.push(format!(
            "{} {} edited by hand, so [icon] leaves {} in place — delete {} once nothing uses {}",
            kept.join(", "),
            if kept.len() == 1 { "was" } else { "were" },
            pron(kept.len()),
            pron(kept.len()),
            pron(kept.len()),
        ));
    }
    changes.push(Change::RemoveDirIfEmpty(WEB_DIR.into()));
    changes.push(Change::Write(LEDGER.into(), ledger_json(&Ledger { active: false, input: None, files })?));
    apply(root, changes, report)
}

fn apply(root: &Path, changes: Vec<Change>, report: &mut SyncReport) -> anyhow::Result<()> {
    // All or nothing: a symlink on the way to any target (written or removed) stops the sync before it
    // touches anything.
    for change in &changes {
        let (Change::Write(rel, _) | Change::Remove(rel) | Change::RemoveDirIfEmpty(rel)) = change;
        crate::fsguard::check(root, &root.join(rel))?;
    }
    for change in changes {
        match change {
            Change::Write(rel, bytes) => {
                let path = root.join(&rel);
                if fs::read(&path).is_ok_and(|old| old == bytes) {
                    continue;
                }
                #[cfg(test)]
                if rel != LEDGER && tests::FAIL_AFTER.with(|n| n.get() == Some(report.written.len())) {
                    anyhow::bail!("injected failure before writing {rel}");
                }
                crate::fsguard::write(root, &path, bytes)?;
                if rel != LEDGER {
                    report.written.push(rel);
                }
            }
            Change::Remove(rel) => {
                if fs::remove_file(root.join(&rel)).is_ok() {
                    report.removed.push(rel);
                }
            }
            Change::RemoveDirIfEmpty(rel) => {
                if fs::remove_dir(root.join(&rel)).is_ok() {
                    report.removed.push(rel);
                }
            }
        }
    }
    Ok(())
}

// ---------------- CLI ----------------

/// `Ok(None)`: no mobiler.toml or no `[icon]`.
fn read_spec(root: &Path) -> Result<Option<IconSpec>, String> {
    let Ok(text) = fs::read_to_string(root.join("mobiler.toml")) else { return Ok(None) };
    toml::from_str::<Manifest>(&text).map(|m| m.icon).map_err(|e| format!("[icon] doesn't parse: {}", e.message()))
}

/// `mobiler icon sync` — run the sync from the app root and print what happened.
pub fn run_sync_cli() -> anyhow::Result<()> {
    let root = std::env::current_dir().context("reading current directory")?;
    let report = sync(&root)?;
    for w in &report.warnings {
        eprintln!("warning: icon: {w}");
    }
    for p in &report.written {
        println!("  wrote   {p}");
    }
    for p in &report.removed {
        println!("  removed {p}");
    }
    if report.written.is_empty() && report.removed.is_empty() {
        match read_spec(&root) {
            Ok(Some(_)) => println!("Icon up to date."),
            _ => println!("No [icon] in mobiler.toml — the launcher icon files are the app's own to edit."),
        }
    }
    Ok(())
}

/// Called at the start of `mobiler build` / `dev` / `watch`: sync and print warnings, never fail.
pub fn sync_for_build(root: &Path) {
    match sync(root) {
        Ok(r) => {
            for w in r.warnings {
                eprintln!("warning: icon: {w}");
            }
        }
        Err(e) => eprintln!("warning: icon: {e:#} — icon not synced"),
    }
}

/// What `mobiler watch` should also watch for the icon: the source images' directories, and
/// `mobiler.toml`.
pub fn watch_paths(root: &Path) -> (Vec<std::path::PathBuf>, Vec<std::path::PathBuf>) {
    let mut dirs: Vec<std::path::PathBuf> = Vec::new();
    if let Ok(Some(spec)) = read_spec(root) {
        for src in std::iter::once(&spec.foreground).chain(spec.monochrome.as_ref()) {
            if let Some(parent) = root.join(src).parent().map(Path::to_path_buf).filter(|p| !dirs.contains(p)) {
                dirs.push(parent);
            }
        }
    }
    (dirs, vec![root.join("mobiler.toml")])
}

/// One `mobiler doctor` line about `[icon]`. Checks the settings and the images' headers; decodes nothing.
pub fn doctor_line(root: &Path) -> String {
    let checked = read_spec(root).and_then(|spec| {
        let Some(spec) = spec else { return Ok(None) };
        let (_, padding) = check_settings(&spec)?;
        let side = check_header(root, "foreground", &spec.foreground)?;
        if let Some(m) = &spec.monochrome {
            check_header(root, "monochrome", m)?;
        }
        Ok(Some((spec, side, padding)))
    });
    match checked {
        Ok(None) => "icon: not set (template launcher icon)".into(),
        Ok(Some((spec, side, padding))) => {
            let mono = if spec.monochrome.is_some() { ", monochrome" } else { "" };
            format!("icon: {} ({side} px) on {}, padding {padding}%{mono}", spec.foreground, spec.background.to_ascii_uppercase())
        }
        Err(e) => format!("icon: mobiler.toml {e}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::{ImageBuffer, Luma, Rgba};

    fn tmp() -> std::path::PathBuf {
        use std::sync::atomic::{AtomicU32, Ordering};
        static N: AtomicU32 = AtomicU32::new(0);
        let d = std::env::temp_dir().join(format!(
            "mobiler-icon-{}-{}",
            std::process::id(),
            N.fetch_add(1, Ordering::SeqCst)
        ));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    fn opaque_png(root: &Path, rel: &str, w: u32, h: u32) {
        RgbaImage::from_pixel(w, h, Rgba([200, 30, 30, 255])).save(root.join(rel)).unwrap();
    }

    fn spec(fg: &str) -> IconSpec {
        IconSpec { foreground: fg.into(), background: "#0E9F8E".into(), monochrome: None, padding: None }
    }

    fn icon(fg: RgbaImage, padding: u32) -> Icon {
        Icon::new(fg, None, [10, 20, 30], padding)
    }

    /// A PNG whose header claims `w`×`h` but holds no image data: only the header is ever read.
    fn header_only_png(w: u32, h: u32) -> Vec<u8> {
        let mut v = b"\x89PNG\r\n\x1a\n".to_vec();
        v.extend_from_slice(&13u32.to_be_bytes());
        v.extend_from_slice(b"IHDR");
        v.extend_from_slice(&w.to_be_bytes());
        v.extend_from_slice(&h.to_be_bytes());
        v.extend_from_slice(&[8, 6, 0, 0, 0, 0, 0, 0, 0]); // depth, colour type, …, (bogus) CRC
        v
    }

    fn crc32(bytes: &[u8]) -> u32 {
        let mut c = 0xFFFF_FFFFu32;
        for &b in bytes {
            c ^= u32::from(b);
            for _ in 0..8 {
                c = if c & 1 == 1 { 0xEDB8_8320 ^ (c >> 1) } else { c >> 1 };
            }
        }
        !c
    }

    /// A `side`² 8-bit indexed PNG: palette [transparent blue, opaque white], pixels alternating 0/1.
    fn palette_png(side: u32) -> Vec<u8> {
        fn chunk(out: &mut Vec<u8>, kind: &[u8], data: &[u8]) {
            out.extend_from_slice(&u32::try_from(data.len()).unwrap().to_be_bytes());
            let start = out.len();
            out.extend_from_slice(kind);
            out.extend_from_slice(data);
            let crc = crc32(&out[start..]);
            out.extend_from_slice(&crc.to_be_bytes());
        }
        let mut raw = Vec::new();
        for _ in 0..side {
            raw.push(0); // filter: none
            raw.extend((0..side).map(|x| u8::from(x % 2 == 1)));
        }
        // zlib with stored (uncompressed) deflate blocks.
        let mut z = vec![0x78, 0x01];
        let blocks: Vec<&[u8]> = raw.chunks(65_535).collect();
        for (i, b) in blocks.iter().enumerate() {
            z.push(u8::from(i + 1 == blocks.len()));
            let len = u16::try_from(b.len()).unwrap();
            z.extend_from_slice(&len.to_le_bytes());
            z.extend_from_slice(&(!len).to_le_bytes());
            z.extend_from_slice(b);
        }
        let (mut a, mut bb) = (1u32, 0u32);
        for &x in &raw {
            a = (a + u32::from(x)) % 65_521;
            bb = (bb + a) % 65_521;
        }
        z.extend_from_slice(&((bb << 16) | a).to_be_bytes());
        let mut out = b"\x89PNG\r\n\x1a\n".to_vec();
        let mut ihdr = side.to_be_bytes().to_vec();
        ihdr.extend_from_slice(&side.to_be_bytes());
        ihdr.extend_from_slice(&[8, 3, 0, 0, 0]);
        chunk(&mut out, b"IHDR", &ihdr);
        chunk(&mut out, b"PLTE", &[0, 0, 255, 255, 255, 255]);
        chunk(&mut out, b"tRNS", &[0, 255]);
        chunk(&mut out, b"IDAT", &z);
        chunk(&mut out, b"IEND", &[]);
        out
    }

    /// The bounding box (x0, y0, x1, y1 inclusive) of the pixels for which `hit` is true.
    fn bbox(img: &RgbaImage, hit: impl Fn(&Rgba<u8>) -> bool) -> (u32, u32, u32, u32) {
        let (mut x0, mut y0, mut x1, mut y1) = (u32::MAX, u32::MAX, 0, 0);
        for (x, y, p) in img.enumerate_pixels() {
            if hit(p) {
                x0 = x0.min(x);
                y0 = y0.min(y);
                x1 = x1.max(x);
                y1 = y1.max(y);
            }
        }
        (x0, y0, x1, y1)
    }

    fn assert_box(b: (u32, u32, u32, u32), off: u32, side: u32) {
        assert_eq!(b, (off, off, off + side - 1, off + side - 1), "want {side}² at offset {off}");
    }

    #[test]
    fn validate_rejects() {
        let root = tmp();
        std::fs::write(root.join("not.png"), b"GIF89a not a png at all").unwrap();
        opaque_png(&root, "small.png", 1000, 1000);
        opaque_png(&root, "wide.png", 1024, 1000);
        opaque_png(&root, "ok.png", 1024, 1024);
        std::fs::write(root.join("huge.png"), header_only_png(20_000, 20_000)).unwrap();

        let err = |s: IconSpec| validate(&root, &s).expect_err("must be rejected");
        let e = err(spec("not.png"));
        assert!(e.contains("foreground") && e.contains("PNG"), "{e}");
        let e = err(spec("small.png"));
        assert!(e.contains("foreground") && e.contains("1024"), "{e}");
        let e = err(spec("wide.png"));
        assert!(e.contains("foreground") && e.contains("square"), "{e}");
        let e = err(spec("huge.png"));
        assert!(e.contains("foreground") && e.contains("16384"), "{e}");
        let e = err(IconSpec { background: "#12345".into(), ..spec("ok.png") });
        assert!(e.contains("background"), "{e}");
        let e = err(IconSpec { background: "red".into(), ..spec("ok.png") });
        assert!(e.contains("background"), "{e}");
        let e = err(IconSpec { background: "#FF0E9F8E".into(), ..spec("ok.png") });
        assert!(e.contains("background"), "an #AARRGGBB colour is not #RRGGBB: {e}");
        let e = err(IconSpec { padding: Some(41), ..spec("ok.png") });
        assert!(e.contains("padding"), "{e}");
        let e = err(spec("missing.png"));
        assert!(e.contains("foreground") && e.contains("missing.png"), "{e}");
        let e = err(IconSpec { monochrome: Some("small.png".into()), ..spec("ok.png") });
        assert!(e.contains("monochrome"), "{e}");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn validate_accepts_palette_gray_and_16bit() {
        let root = tmp();
        // 16-bit RGBA.
        ImageBuffer::<Rgba<u16>, Vec<u16>>::from_pixel(1024, 1024, Rgba([65_535, 0, 0, 65_535]))
            .save(root.join("deep.png"))
            .unwrap();
        // 8-bit grayscale.
        ImageBuffer::<Luma<u8>, Vec<u8>>::from_pixel(1024, 1024, Luma([128])).save(root.join("gray.png")).unwrap();
        // Indexed (palette) colour with a tRNS chunk ("image" can't write one): hand-built.
        std::fs::write(root.join("palette.png"), palette_png(1024)).unwrap();
        for (rel, want) in [("deep.png", [255, 0, 0, 255]), ("gray.png", [128, 128, 128, 255]), ("palette.png", [0, 0, 0, 0])] {
            let icon = validate(&root, &IconSpec { monochrome: Some(rel.into()), ..spec(rel) })
                .unwrap_or_else(|e| panic!("{rel}: {e}"));
            assert_eq!(icon.fg.dimensions(), (1024, 1024), "{rel}");
            assert_eq!(icon.fg.get_pixel(0, 0).0, want, "{rel} decodes to premultiplied RGBA8");
            assert!(icon.mono.is_some(), "{rel}");
            assert_eq!(icon.bg, [0x0E, 0x9F, 0x8E]);
            assert_eq!(icon.padding, DEFAULT_PADDING);
        }
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn layer_content_box_at_default_padding() {
        let fg = RgbaImage::from_pixel(1024, 1024, Rgba([255, 255, 255, 255]));
        let icon = icon(fg.clone(), DEFAULT_PADDING);
        let out = layer(&icon, &icon.fg, 432);
        assert_eq!(out.dimensions(), (432, 432));
        assert_eq!(out.get_pixel(0, 0).0[3], 0, "the layer is transparent outside the content");
        assert_box(bbox(&out, |p| p.0[3] == 255), 124, 184);
        for (_, px) in LAYER_SIZES {
            assert_eq!(layer(&icon, &icon.fg, px).dimensions(), (px, px));
        }
    }

    #[test]
    fn composite_matches_padding() {
        let fg = RgbaImage::from_pixel(1024, 1024, Rgba([250, 250, 250, 255]));
        let icon = icon(fg.clone(), 18);
        let out = composite(&icon, 1024, false);
        assert_eq!(out.dimensions(), (1024, 1024));
        assert_eq!(out.get_pixel(0, 0).0, [10, 20, 30, 255], "background fills the square");
        assert!(out.pixels().all(|p| p.0[3] == 255), "a square composite is opaque");
        assert_box(bbox(&out, |p| p.0 == [250, 250, 250, 255]), 184, 655);
        // Padding 0: the foreground fills the whole square.
        let full = composite(&Icon::new(fg, None, [10, 20, 30], 0), 192, false);
        assert_box(bbox(&full, |p| p.0 == [250, 250, 250, 255]), 0, 192);
    }

    #[test]
    fn large_source_gives_the_same_composite_box() {
        let root = tmp();
        // Just over WORK_PX: the same downscale branch as a 4096 px source, at a fraction of the cost.
        RgbaImage::from_pixel(2050, 2050, Rgba([250, 250, 250, 255])).save(root.join("big.png")).unwrap();
        RgbaImage::from_pixel(1024, 1024, Rgba([250, 250, 250, 255])).save(root.join("small.png")).unwrap();
        let big = validate(&root, &spec("big.png")).unwrap();
        let small = validate(&root, &spec("small.png")).unwrap();
        assert_eq!(big.fg.dimensions(), (WORK_PX, WORK_PX), "a large source is downsized once");
        assert_eq!(small.fg.dimensions(), (1024, 1024), "a small source is never upscaled");
        let white = |p: &Rgba<u8>| p.0 == [250, 250, 250, 255];
        assert_box(bbox(&composite(&big, 1024, false), white), 184, 655);
        assert_box(bbox(&composite(&small, 1024, false), white), 184, 655);
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn round_corners_are_transparent() {
        let fg = RgbaImage::from_pixel(1024, 1024, Rgba([250, 250, 250, 255]));
        let out = composite(&icon(fg, 18), 192, true);
        for (x, y) in [(0, 0), (191, 0), (0, 191), (191, 191), (20, 20)] {
            assert_eq!(out.get_pixel(x, y).0[3], 0, "corner ({x},{y})");
        }
        assert_eq!(out.get_pixel(96, 96).0[3], 255, "centre is opaque");
        assert_eq!(out.get_pixel(96, 3).0[3], 255, "inside the edge is opaque");
        // The edge is anti-aliased: some pixels are partly covered.
        assert!(out.pixels().any(|p| p.0[3] > 0 && p.0[3] < 255), "edge has partial coverage");
    }

    #[test]
    fn ios_png_has_no_alpha() {
        let mut fg = RgbaImage::from_pixel(1024, 1024, Rgba([0, 0, 0, 0]));
        fg.put_pixel(512, 512, Rgba([255, 255, 255, 255]));
        let bytes = ios_png(&icon(fg, 18));
        let img = image::load_from_memory_with_format(&bytes, image::ImageFormat::Png).unwrap();
        assert_eq!(img.color(), image::ColorType::Rgb8);
        assert_eq!(img.dimensions_tuple(), (1024, 1024));
        assert_eq!(img.to_rgb8().get_pixel(0, 0).0, [10, 20, 30]);
    }

    trait Dims {
        fn dimensions_tuple(&self) -> (u32, u32);
    }
    impl Dims for image::DynamicImage {
        fn dimensions_tuple(&self) -> (u32, u32) {
            (self.width(), self.height())
        }
    }

    #[test]
    fn webp_is_lossless() {
        let mut img = RgbaImage::from_pixel(48, 48, Rgba([1, 2, 3, 255]));
        img.put_pixel(7, 9, Rgba([200, 100, 50, 77]));
        let bytes = webp(&img);
        let back = image::load_from_memory_with_format(&bytes, image::ImageFormat::WebP).unwrap().to_rgba8();
        assert_eq!(back, img);
        assert_eq!(image::load_from_memory_with_format(&png(&img), image::ImageFormat::Png).unwrap().to_rgba8(), img);
    }

    #[test]
    fn adaptive_xml_falls_back_to_foreground_for_monochrome() {
        let x = android_adaptive_xml(false);
        assert!(x.starts_with("<?xml"), "{x}");
        assert!(x.contains(&format!("<!-- {HEADER} -->")), "{x}");
        assert!(x.contains("<background android:drawable=\"@color/mobiler_icon_background\"/>"), "{x}");
        assert!(x.contains("<foreground android:drawable=\"@mipmap/mobiler_icon_foreground\"/>"), "{x}");
        assert!(x.contains("<monochrome android:drawable=\"@mipmap/mobiler_icon_foreground\"/>"), "{x}");
        let m = android_adaptive_xml(true);
        assert!(m.contains("<monochrome android:drawable=\"@mipmap/mobiler_icon_monochrome\"/>"), "{m}");
        assert!(!m.contains("<monochrome android:drawable=\"@mipmap/mobiler_icon_foreground\"/>"), "{m}");
    }

    #[test]
    fn colors_xml_has_the_background() {
        let x = colors_xml([0x0E, 0x9F, 0x8E]);
        assert!(x.contains(&format!("<!-- {HEADER} -->")), "{x}");
        assert!(x.contains("<color name=\"mobiler_icon_background\">#0E9F8E</color>"), "{x}");
    }

    #[test]
    fn web_lines_are_the_links_or_nothing() {
        assert_eq!(
            web_lines(true),
            [
                r#"<link data-trunk rel="copy-dir" href="icon"/>"#,
                r#"<link rel="icon" type="image/png" sizes="32x32" href="icon/mobiler-icon-32.png"/>"#,
                r#"<link rel="apple-touch-icon" href="icon/mobiler-icon-180.png"/>"#,
            ]
        );
        assert!(web_lines(false).is_empty());
    }

    #[test]
    fn fnv_is_stable() {
        assert_eq!(fnv1a64_hex(b""), "cbf29ce484222325");
        assert_eq!(fnv1a64_hex(b"a"), "af63dc4c8601ec8c");
        assert_ne!(fnv1a64_hex(b"a\r\n"), fnv1a64_hex(b"a\n"), "binary: CRLF is not folded");
    }

    // ---------------- sync ----------------

    thread_local! {
        /// How many source images this thread decoded (the unchanged-input skip must decode none).
        pub(super) static DECODES: std::cell::Cell<u32> = const { std::cell::Cell::new(0) };
        /// `Some(n)`: the sync's writes fail after `n` files (a full disk, a killed process).
        pub(super) static FAIL_AFTER: std::cell::Cell<Option<usize>> = const { std::cell::Cell::new(None) };
    }

    const ICON_TOML: &str = "[icon]\nforeground = \"brand/fg.png\"\nbackground = \"#0E9F8E\"\n";
    const PAGE: &str = "<html>\n<head>\n  <title>x</title>\n</head>\n<body></body>\n</html>\n";

    fn put(root: &Path, rel: &str, bytes: &[u8]) {
        let p = root.join(rel);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, bytes).unwrap();
    }

    fn read(root: &Path, rel: &str) -> Vec<u8> {
        std::fs::read(root.join(rel)).unwrap_or_else(|e| panic!("{rel}: {e}"))
    }

    fn text(root: &Path, rel: &str) -> String {
        String::from_utf8(read(root, rel)).unwrap()
    }

    /// A white square on transparency, `side`² px.
    fn glyph_png(root: &Path, rel: &str, side: u32) {
        let mut img = RgbaImage::new(side, side);
        for y in side / 4..side * 3 / 4 {
            for x in side / 4..side * 3 / 4 {
                img.put_pixel(x, y, Rgba([255, 255, 255, 255]));
            }
        }
        std::fs::create_dir_all(root.join(rel).parent().unwrap()).unwrap();
        img.save(root.join(rel)).unwrap();
    }

    /// A scaffolded app (every template file, as `mobiler new` writes them) with a web page, a 1024 px
    /// foreground in `brand/` and `toml` as its mobiler.toml.
    fn app(toml: &str) -> std::path::PathBuf {
        let root = tmp();
        crate::upgrade::test_support::scaffold(&root);
        // The scaffold leaves out binaries (upgrade never touches them); `mobiler new` writes them.
        for (d, _) in LEGACY_SIZES {
            for name in ["ic_launcher.webp", "ic_launcher_round.webp"] {
                put(&root, &mipmap(d, name), template(&mipmap(d, name)));
            }
        }
        put(&root, IOS_PNG, template(IOS_PNG));
        put(&root, WEB_INDEX, PAGE.as_bytes());
        glyph_png(&root, "brand/fg.png", 1024);
        put(&root, "mobiler.toml", toml.as_bytes());
        root
    }

    /// Every file under `root` (bytes and modification time), except the ones in `skip`.
    fn snapshot(root: &Path) -> BTreeMap<std::path::PathBuf, (Vec<u8>, std::time::SystemTime)> {
        fn walk(d: &Path, out: &mut BTreeMap<std::path::PathBuf, (Vec<u8>, std::time::SystemTime)>) {
            for e in std::fs::read_dir(d).unwrap().flatten() {
                let p = e.path();
                if p.is_dir() {
                    walk(&p, out);
                } else {
                    let m = std::fs::metadata(&p).unwrap().modified().unwrap();
                    out.insert(p.clone(), (std::fs::read(&p).unwrap(), m));
                }
            }
        }
        let mut out = BTreeMap::new();
        walk(root, &mut out);
        out
    }

    fn template(rel: &str) -> &'static [u8] {
        stock(rel).unwrap_or_else(|| panic!("{rel} is not in the template"))
    }

    fn android_icon_files(mono: bool) -> Vec<String> {
        let mut v: Vec<String> = Vec::new();
        for (d, _) in LAYER_SIZES {
            v.push(mipmap(d, "mobiler_icon_foreground.png"));
            if mono {
                v.push(mipmap(d, "mobiler_icon_monochrome.png"));
            }
        }
        v.push(COLORS.into());
        v
    }

    #[test]
    fn writes_every_platform() {
        let root = app(ICON_TOML);
        let r = sync(&root).unwrap();
        assert!(r.warnings.is_empty(), "{:?}", r.warnings);
        for (d, px) in LAYER_SIZES {
            assert_eq!(image::image_dimensions(root.join(mipmap(d, "mobiler_icon_foreground.png"))).unwrap(), (px, px));
            assert!(!root.join(mipmap(d, "mobiler_icon_monochrome.png")).exists());
        }
        for (d, px) in LEGACY_SIZES {
            for name in ["ic_launcher.webp", "ic_launcher_round.webp"] {
                let rel = mipmap(d, name);
                let bytes = read(&root, &rel);
                assert_ne!(bytes, template(&rel), "{rel} rewritten");
                let img = image::load_from_memory_with_format(&bytes, image::ImageFormat::WebP).unwrap();
                assert_eq!((img.width(), img.height()), (px, px), "{rel}");
            }
        }
        assert!(text(&root, COLORS).contains("#0E9F8E"));
        for rel in ANYDPI {
            assert_eq!(text(&root, rel), android_adaptive_xml(false), "{rel}");
        }
        let ios = image::load_from_memory_with_format(&read(&root, IOS_PNG), image::ImageFormat::Png).unwrap();
        assert_eq!((ios.color(), ios.width()), (image::ColorType::Rgb8, 1024));
        for px in WEB_SIZES {
            assert_eq!(image::image_dimensions(root.join(web_png(px))).unwrap(), (px, px));
        }
        let html = text(&root, WEB_INDEX);
        let (b, e, head) = (html.find(WEB_BEGIN).unwrap(), html.find(WEB_END).unwrap(), html.find("</head>").unwrap());
        assert!(b < e && e < head, "{html}");
        for line in web_lines(true) {
            assert!(html[b..e].contains(&line), "{html}");
        }
        let ledger = read_ledger(&root).unwrap();
        assert!(ledger.active && ledger.input.is_some());
        for rel in targets(&root, false) {
            assert_eq!(ledger.files.get(&rel), Some(&vec![fnv1a64_hex(&read(&root, &rel))]), "{rel} in the ledger");
        }
        // A monochrome image adds its layers and the XML names them; dropping it removes them again.
        glyph_png(&root, "brand/mono.png", 1024);
        put(&root, "mobiler.toml", format!("{ICON_TOML}monochrome = \"brand/mono.png\"\n").as_bytes());
        assert!(sync(&root).unwrap().warnings.is_empty());
        for f in android_icon_files(true) {
            assert!(root.join(&f).exists(), "{f}");
        }
        assert_eq!(text(&root, ANYDPI[0]), android_adaptive_xml(true));
        put(&root, "mobiler.toml", ICON_TOML.as_bytes());
        let r = sync(&root).unwrap();
        assert!(r.warnings.is_empty(), "{:?}", r.warnings);
        assert!(!root.join(mipmap("mdpi", "mobiler_icon_monochrome.png")).exists(), "{r:?}");
        assert_eq!(text(&root, ANYDPI[1]), android_adaptive_xml(false));
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn second_sync_is_a_no_op() {
        let root = app(ICON_TOML);
        sync(&root).unwrap();
        let r = sync(&root).unwrap();
        assert!(r.written.is_empty() && r.removed.is_empty() && r.warnings.is_empty(), "{r:?}");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn unchanged_inputs_skip_decoding() {
        let root = app(ICON_TOML);
        sync(&root).unwrap();
        let before = snapshot(&root);
        DECODES.with(|n| n.set(0));
        let r = sync(&root).unwrap();
        assert!(r.written.is_empty() && r.warnings.is_empty(), "{r:?}");
        assert_eq!(DECODES.with(std::cell::Cell::get), 0, "the same inputs decode nothing");
        assert_eq!(snapshot(&root), before, "no file touched (bytes and mtimes)");
        // A file the sync wrote, changed (or gone): regenerated.
        std::fs::remove_file(root.join(IOS_PNG)).unwrap();
        let r = sync(&root).unwrap();
        assert_eq!(DECODES.with(std::cell::Cell::get), 1);
        assert_eq!(r.written, [IOS_PNG], "only what differs is rewritten");
        // A changed setting: decoded again.
        put(&root, "mobiler.toml", ICON_TOML.replace("#0E9F8E", "#112233").as_bytes());
        sync(&root).unwrap();
        assert_eq!(DECODES.with(std::cell::Cell::get), 2);
        assert!(text(&root, COLORS).contains("#112233"));
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn hand_edited_launcher_xml_skips_android_only() {
        let root = app(ICON_TOML);
        // Saldo's own adaptive icon, before its reset.
        let mine = "<?xml version=\"1.0\" encoding=\"utf-8\"?>\n<adaptive-icon xmlns:android=\"http://schemas.android.com/apk/res/android\">\n    <background android:drawable=\"@drawable/ic_launcher_background\" />\n    <foreground android:drawable=\"@drawable/saldo_launcher_foreground\" />\n</adaptive-icon>\n";
        put(&root, ANYDPI[0], mine.as_bytes());
        let r = sync(&root).unwrap();
        assert!(r.warnings.iter().any(|w| w.contains(ANYDPI[0]) && w.contains("Android")), "{:?}", r.warnings);
        assert!(!r.warnings.iter().any(|w| w.contains(ANYDPI[1])), "the stock round icon isn't named: {:?}", r.warnings);
        assert_eq!(text(&root, ANYDPI[0]), mine);
        assert_eq!(read(&root, ANYDPI[1]), template(ANYDPI[1]), "nothing of Android's written");
        assert_eq!(read(&root, &mipmap("mdpi", "ic_launcher.webp")), template(&mipmap("mdpi", "ic_launcher.webp")));
        for f in android_icon_files(false) {
            assert!(!root.join(&f).exists(), "{f}");
        }
        assert_ne!(read(&root, IOS_PNG), template(IOS_PNG), "iOS still written");
        assert!(root.join(web_png(180)).exists() && text(&root, WEB_INDEX).contains(WEB_BEGIN), "web still written");
        assert!(!read_ledger(&root).unwrap().files.keys().any(|k| k.starts_with("Android/")), "nothing of Android's recorded");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn broken_foreground_writes_nothing() {
        let root = app(ICON_TOML);
        sync(&root).unwrap();
        let before = snapshot(&root);
        put(&root, "brand/fg.png", b"not a png");
        put(&root, "mobiler.toml", ICON_TOML.replace("#0E9F8E", "#000000").as_bytes());
        let mut before = before;
        for rel in ["brand/fg.png", "mobiler.toml"] {
            before.remove(&root.join(rel));
        }
        let r = sync(&root).unwrap();
        assert!(r.warnings.iter().any(|w| w.contains("brand/fg.png")), "{:?}", r.warnings);
        assert!(r.written.is_empty() && r.removed.is_empty(), "{r:?}");
        let mut after = snapshot(&root);
        for rel in ["brand/fg.png", "mobiler.toml"] {
            after.remove(&root.join(rel));
        }
        assert_eq!(after, before, "the tree is byte-identical");
        // A bad image on a first sync writes nothing either.
        let fresh = app(ICON_TOML);
        put(&fresh, "brand/fg.png", b"not a png");
        let before = snapshot(&fresh);
        sync(&fresh).unwrap();
        assert_eq!(snapshot(&fresh), before);
        let _ = std::fs::remove_dir_all(&root);
        let _ = std::fs::remove_dir_all(&fresh);
    }

    #[test]
    fn removing_icon_restores_stock() {
        let root = app(&format!("{ICON_TOML}monochrome = \"brand/fg.png\"\n"));
        sync(&root).unwrap();
        put(&root, "mobiler.toml", b"");
        let r = sync(&root).unwrap();
        assert!(r.warnings.is_empty(), "{:?}", r.warnings);
        let mut stocked: Vec<String> = ANYDPI.map(String::from).to_vec();
        for (d, _) in LEGACY_SIZES {
            stocked.push(mipmap(d, "ic_launcher.webp"));
            stocked.push(mipmap(d, "ic_launcher_round.webp"));
        }
        stocked.push(IOS_PNG.into());
        for rel in &stocked {
            assert_eq!(read(&root, rel), template(rel), "{rel} back to the template");
        }
        for rel in android_icon_files(true).into_iter().chain(WEB_SIZES.map(web_png)) {
            assert!(!root.join(&rel).exists(), "{rel} removed");
        }
        assert!(!root.join(WEB_DIR).exists(), "the emptied web/icon goes");
        let html = text(&root, WEB_INDEX);
        assert!(html.contains(WEB_BEGIN) && !html.contains("mobiler-icon-"), "web block emptied: {html}");
        assert!(!read_ledger(&root).unwrap().active, "the ledger stays, inactive");
        let r = sync(&root).unwrap();
        assert!(r.written.is_empty() && r.removed.is_empty(), "undo happens once: {r:?}");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn removed_and_added_back_still_owns_its_files() {
        let root = app(ICON_TOML);
        sync(&root).unwrap();
        put(&root, "mobiler.toml", b"");
        sync(&root).unwrap();
        put(&root, "mobiler.toml", ICON_TOML.replace("#0E9F8E", "#123456").as_bytes());
        let r = sync(&root).unwrap();
        assert!(r.warnings.is_empty(), "{:?}", r.warnings);
        assert!(text(&root, COLORS).contains("#123456"));
        assert_eq!(text(&root, ANYDPI[0]), android_adaptive_xml(false));
        assert_ne!(read(&root, IOS_PNG), template(IOS_PNG));
        // A file the sync wrote, then the section changed: still the sync's by the ledger (a binary has
        // no header).
        put(&root, "mobiler.toml", ICON_TOML.replace("#0E9F8E", "#654321").as_bytes());
        let r = sync(&root).unwrap();
        assert!(r.warnings.is_empty() && r.written.contains(&IOS_PNG.to_string()), "{r:?}");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn no_png_beside_webp() {
        let root = app(&format!("{ICON_TOML}monochrome = \"brand/fg.png\"\n"));
        sync(&root).unwrap();
        for e in std::fs::read_dir(root.join(RES)).unwrap().flatten() {
            let name = e.file_name().to_string_lossy().to_string();
            if !name.starts_with("mipmap-") {
                continue;
            }
            let files: Vec<String> =
                std::fs::read_dir(e.path()).unwrap().flatten().map(|f| f.file_name().to_string_lossy().to_string()).collect();
            for f in &files {
                if let Some(stem) = f.strip_suffix(".webp") {
                    assert!(!files.contains(&format!("{stem}.png")), "{name}: {stem} is both .png and .webp");
                }
            }
        }
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn foreign_file_in_web_icon_survives() {
        let root = app(ICON_TOML);
        put(&root, "web/icon/mine.svg", b"<svg/>");
        sync(&root).unwrap();
        put(&root, "mobiler.toml", b"");
        sync(&root).unwrap();
        assert_eq!(read(&root, "web/icon/mine.svg"), b"<svg/>");
        assert!(!root.join(web_png(32)).exists());
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn upgrade_reports_nothing_for_icon_files() {
        let root = app(&format!("{ICON_TOML}monochrome = \"brand/fg.png\"\n"));
        assert!(sync(&root).unwrap().warnings.is_empty());
        let ours: Vec<String> = targets(&root, true).into_iter().chain([LEDGER.to_string()]).collect();
        for rel in ANYDPI {
            assert!(crate::upgrade::SEED_PATHS.contains(&rel), "{rel} is an upgrade seed");
        }
        let before: Vec<Vec<u8>> = ours.iter().map(|r| read(&root, r)).collect();
        for apply in [false, true] {
            let touched = crate::upgrade::test_support::touched(&root, apply);
            assert!(!touched.iter().any(|p| ours.contains(&p.replace('\\', "/"))), "apply={apply}: {touched:?}");
            let now: Vec<Vec<u8>> = ours.iter().map(|r| read(&root, r)).collect();
            assert!(now == before, "apply={apply}: an icon file changed");
        }
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn no_section_touches_nothing() {
        let root = app("[splash]\nbackground = \"#FFFFFF\"\n");
        let before = snapshot(&root);
        let r = sync(&root).unwrap();
        assert!(r.written.is_empty() && r.removed.is_empty() && r.warnings.is_empty(), "{r:?}");
        std::fs::remove_file(root.join("mobiler.toml")).unwrap();
        assert!(sync(&root).unwrap().written.is_empty());
        put(&root, "mobiler.toml", b"[splash]\nbackground = \"#FFFFFF\"\n");
        assert_eq!(snapshot(&root).len(), before.len(), "no ledger either");
        assert!(!root.join(LEDGER).exists());
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn watch_paths_and_doctor_line() {
        let root = app(&format!("{ICON_TOML}monochrome = \"art/mono.png\"\n"));
        glyph_png(&root, "art/mono.png", 1024);
        let (dirs, files) = watch_paths(&root);
        assert_eq!(dirs, [root.join("brand"), root.join("art")]);
        assert_eq!(files, [root.join("mobiler.toml")]);
        assert_eq!(doctor_line(&root), "icon: brand/fg.png (1024 px) on #0E9F8E, padding 18%, monochrome");
        put(&root, "mobiler.toml", ICON_TOML.replace("#0E9F8E", "teal").as_bytes());
        assert_eq!(doctor_line(&root), "icon: mobiler.toml [icon] background \"teal\": a colour is #RRGGBB");
        put(&root, "mobiler.toml", b"");
        assert_eq!(doctor_line(&root), "icon: not set (template launcher icon)");
        assert!(watch_paths(&root).0.is_empty());
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn undo_keeps_a_hand_edited_sync_file() {
        let root = app(ICON_TOML);
        sync(&root).unwrap();
        let layer = mipmap("xxxhdpi", "mobiler_icon_foreground.png");
        glyph_png(&root, &layer, 432); // retouched by hand
        let mine = read(&root, &layer);
        put(&root, "mobiler.toml", b"");
        let r = sync(&root).unwrap();
        assert!(r.warnings.iter().any(|w| w.contains(&layer) && w.contains("edited by hand")), "{:?}", r.warnings);
        assert_eq!(read(&root, &layer), mine, "the retouched layer survives");
        assert!(!root.join(mipmap("mdpi", "mobiler_icon_foreground.png")).exists(), "the untouched ones go");
        assert!(read_ledger(&root).unwrap().files.contains_key(&layer), "its ledger entry is kept");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn hand_edited_webp_is_left_alone() {
        let root = app(ICON_TOML);
        sync(&root).unwrap();
        let webp_rel = mipmap("hdpi", "ic_launcher.webp");
        let mine = webp(&RgbaImage::from_pixel(72, 72, Rgba([1, 2, 3, 255])));
        put(&root, &webp_rel, &mine);
        put(&root, "mobiler.toml", ICON_TOML.replace("#0E9F8E", "#123456").as_bytes());
        let r = sync(&root).unwrap();
        assert!(r.warnings.iter().any(|w| w.contains(&webp_rel) && w.contains("Android")), "{:?}", r.warnings);
        assert_eq!(read(&root, &webp_rel), mine, "byte-identical");
        assert!(text(&root, COLORS).contains("#0E9F8E"), "nothing of Android's rewritten");
        assert!(r.written.contains(&IOS_PNG.to_string()) && r.written.contains(&web_png(32)), "{r:?}");
        put(&root, "mobiler.toml", b"");
        sync(&root).unwrap();
        assert_eq!(read(&root, &webp_rel), mine, "undo leaves it too");
        assert_eq!(read(&root, IOS_PNG), template(IOS_PNG));
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn interrupted_sync_is_retried_and_owns_its_files() {
        for first in [true, false] {
            let root = app(ICON_TOML);
            if !first {
                sync(&root).unwrap();
            }
            put(&root, "mobiler.toml", ICON_TOML.replace("#0E9F8E", "#123456").as_bytes());
            FAIL_AFTER.with(|n| n.set(Some(7)));
            let err = sync(&root).expect_err("the injected failure");
            FAIL_AFTER.with(|n| n.set(None));
            assert!(format!("{err:#}").contains("injected"), "{err:#}");
            let r = sync(&root).unwrap();
            assert!(r.warnings.is_empty(), "first={first}: {:?}", r.warnings);
            assert!(text(&root, COLORS).contains("#123456"));
            let ledger = text(&root, LEDGER);
            assert!(!ledger.contains('['), "the final ledger has one hash per file: {ledger}");
            assert!(sync(&root).unwrap().written.is_empty(), "and then it is up to date");
            let _ = std::fs::remove_dir_all(&root);
        }
    }

    #[test]
    fn a_broken_web_block_keeps_its_ledger_entries() {
        let root = app(ICON_TOML);
        sync(&root).unwrap();
        let html = text(&root, WEB_INDEX).replace(WEB_END, "");
        put(&root, WEB_INDEX, html.as_bytes());
        put(&root, "mobiler.toml", ICON_TOML.replace("#0E9F8E", "#123456").as_bytes());
        let r = sync(&root).unwrap();
        assert!(r.warnings.iter().any(|w| w.contains(WEB_INDEX)), "{:?}", r.warnings);
        assert!(read_ledger(&root).unwrap().files.contains_key(&web_png(32)), "web entries carried forward");
        // Fixed again: the sync still owns its web images.
        put(&root, WEB_INDEX, PAGE.as_bytes());
        let r = sync(&root).unwrap();
        assert!(r.warnings.is_empty(), "{:?}", r.warnings);
        let _ = std::fs::remove_dir_all(&root);
    }
}
