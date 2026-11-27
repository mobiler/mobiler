//! `mobiler.toml` `[icon]`: the app's launcher icon, generated for the Android, iOS and web shells
//! from one square foreground PNG and a background colour. This module validates the section and holds
//! the pure image/XML generators; the sync that writes them into an app sits on top.

// Task 8's sync uses these; until it lands only the tests do.
#![allow(dead_code)]

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
/// side; see [`working_copy`]) and the settings.
#[derive(Debug, Clone)]
pub struct Icon {
    /// Premultiplied working copy of the foreground.
    pub fg: RgbaImage,
    /// Premultiplied working copy of the monochrome image.
    pub mono: Option<RgbaImage>,
    pub bg: [u8; 3],
    /// % of the visible icon's side left clear on each side, 0..=40.
    pub padding: u32,
}

/// Check `[icon]` and decode its images. Every failure names the field and the reason; nothing is
/// written by the caller unless this succeeds.
pub fn validate(root: &Path, spec: &IconSpec) -> Result<Icon, String> {
    let padding = spec.padding.unwrap_or(DEFAULT_PADDING);
    if padding > 40 {
        return Err(format!("[icon] padding {padding}: must be in 0..=40 (% left clear on each side)"));
    }
    let bad_bg = || format!("[icon] background {:?}: a colour is #RRGGBB", spec.background);
    if spec.background.len() != 7 {
        return Err(bad_bg());
    }
    let c = crate::splash::Rgba::parse(&spec.background).map_err(|_| bad_bg())?;
    let fg = load(root, "foreground", &spec.foreground)?;
    let mono = spec.monochrome.as_deref().map(|rel| load(root, "monochrome", rel)).transpose()?;
    Ok(Icon { fg, mono, bg: [c.r, c.g, c.b], padding })
}

/// A square PNG, 1024..=16384 px, decoded once to RGBA8 (palette, grayscale and 16-bit inputs
/// included) and turned into its working copy. The size is checked from the header first, so an
/// oversized file is never decoded.
fn load(root: &Path, field: &str, rel: &str) -> Result<RgbaImage, String> {
    use std::io::Read;
    let path = root.join(rel);
    let err = |why: String| format!("[icon] {field} {rel:?}: {why}");
    let mut head = [0u8; 24];
    let mut f = std::fs::File::open(&path).map_err(|e| err(format!("can't read it ({e})")))?;
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
    let mut reader = image::ImageReader::open(&path).map_err(|e| err(format!("can't read it ({e})")))?;
    reader.set_format(image::ImageFormat::Png);
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(MAX_PX);
    limits.max_image_height = Some(MAX_PX);
    limits.max_alloc = None; // a 16384 px RGBA source is 1 GiB; the side limit bounds it
    reader.limits(limits);
    let img = reader.decode().map_err(|e| err(format!("not a readable PNG ({e})")))?;
    Ok(working_copy(img.into_rgba8()))
}

/// `img` premultiplied (so transparent pixels never bleed their colour into an edge when resampled)
/// and, when larger than `WORK_PX`, downsized once (Lanczos3) to `WORK_PX`. Never upscaled.
pub fn working_copy(mut img: RgbaImage) -> RgbaImage {
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
    image::imageops::resize(&img, nw, nh, image::imageops::FilterType::Lanczos3)
}

/// The content box's side: `px * visible * (1 - 2 * padding / 100)`, at least 1.
#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)] // positive and at most `px`
fn content_side(px: u32, visible: f64, padding: u32) -> u32 {
    let side = f64::from(px) * visible * f64::from(100 - 2 * padding.min(40)) / 100.0;
    (side.round() as u32).clamp(1, px)
}

/// A premultiplied working copy resized (Lanczos3) to `side`²; still premultiplied.
fn resized(work: &RgbaImage, side: u32) -> RgbaImage {
    image::imageops::resize(work, side, side, image::imageops::FilterType::Lanczos3)
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
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in bytes {
        h ^= u64::from(*b);
        h = h.wrapping_mul(0x0000_0100_0000_01b3);
    }
    format!("{h:016x}")
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
        Icon { fg: working_copy(fg), mono: None, bg: [10, 20, 30], padding }
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
        let icon = icon(fg, 18);
        let out = composite(&icon, 1024, false);
        assert_eq!(out.dimensions(), (1024, 1024));
        assert_eq!(out.get_pixel(0, 0).0, [10, 20, 30, 255], "background fills the square");
        assert!(out.pixels().all(|p| p.0[3] == 255), "a square composite is opaque");
        assert_box(bbox(&out, |p| p.0 == [250, 250, 250, 255]), 184, 655);
        // Padding 0: the foreground fills the whole square.
        let full = composite(&Icon { padding: 0, ..icon.clone() }, 192, false);
        assert_box(bbox(&full, |p| p.0 == [250, 250, 250, 255]), 0, 192);
    }

    #[test]
    fn large_source_gives_the_same_composite_box() {
        let root = tmp();
        RgbaImage::from_pixel(4096, 4096, Rgba([250, 250, 250, 255])).save(root.join("big.png")).unwrap();
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
}
