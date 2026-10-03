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

#[cfg(test)]
mod tests {
    use super::*;

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
}
