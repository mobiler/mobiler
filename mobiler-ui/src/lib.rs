//! Mobiler's fixed UI wire ABI.
//!
//! These types are the **stable contract** between any Mobiler app's Rust core
//! and the native shell. Because they never change per app, a single shell is
//! built once and renders *any* Mobiler app — the shell only ever knows these
//! types, never an app's domain events or widgets.
//!
//! - The core emits a [`Widget`] tree (the `ViewModel`).
//! - The shell sends back an [`Action`] (the `Event`).
//! - App domain events ride inside actions as opaque [`ActionToken`]s that the
//!   shell round-trips without interpreting.
//!
//! Style is expressed as **intent tokens** (e.g. [`TextStyle`], [`Tone`]); the
//! shell maps each to a concrete look (font, color, dp), so dark mode and theme
//! come for free on the native side.

use facet::Facet;
use serde::{Deserialize, Serialize};

/// An opaque, serialized app event (e.g. JSON of the app's domain action).
pub type ActionToken = String;

/// A value produced by an input widget at runtime.
#[derive(Facet, Serialize, Deserialize, Clone, Debug)]
#[repr(C)]
pub enum InputValue {
    Text(String),
    Bool(bool),
    Int(i64),
}

/// What the shell sends back to the core. **Fixed across all apps.**
#[derive(Facet, Serialize, Deserialize, Clone, Debug)]
#[repr(C)]
pub enum Action {
    /// An action widget (button/etc.) fired; `token` is the opaque app event.
    Fired { token: ActionToken },
    /// A value-carrying input changed; `id` names the widget.
    Input { id: String, value: InputValue },
    /// Persisted state handed back to the core on startup (empty string if none).
    Restore { data: String },
    /// Fired once on startup (after `Restore`) so the app can kick off initial
    /// effects (e.g. fetching data).
    Start,
}

// ---------------------------- style tokens ----------------------------

#[derive(Facet, Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[repr(C)]
pub enum TextStyle { Body, Title, Subtitle, Caption, Emphasis }

#[derive(Facet, Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[repr(C)]
pub enum ButtonStyle { Filled, Outlined, Text }

#[derive(Facet, Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[repr(C)]
pub enum CardStyle { Elevated, Outlined, Filled }

/// Semantic status color (distinct from brand/identity color).
#[derive(Facet, Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[repr(C)]
pub enum Tone { Neutral, Success, Warning, Danger, Info }

#[derive(Facet, Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[repr(C)]
pub enum Spacing { Xs, Sm, Md, Lg, Xl }

/// A small, finite icon set (maps to Material icons in the shell).
#[derive(Facet, Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[repr(C)]
pub enum Icon { Delete, Add, Edit, Close, Settings, Check, Star }

#[derive(Facet, Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[repr(C)]
pub enum ImageShape { Square, Rounded, Circle }

#[derive(Facet, Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[repr(C)]
pub enum ImageRatio { Wide, Square, Tall }

#[derive(Facet, Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[repr(C)]
pub enum BoxAlign { TopStart, TopEnd, Center, BottomStart, BottomCenter, BottomEnd }

/// Project-identity colors (distinct from semantic `Tone`). Concrete RGB decided
/// in the render layer.
#[derive(Facet, Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[repr(C)]
pub enum ProjectColor { Indigo, Teal, Coral, Amber, Lime, Pink }

// ------------------------------- theme -------------------------------

/// A 24-bit RGB color. Used for a theme's brand/seed color — the one place an app
/// supplies an arbitrary color (everything else is intent tokens).
#[derive(Facet, Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[repr(C)]
pub struct Rgb {
    pub r: u8,
    pub g: u8,
    pub b: u8,
}

impl Rgb {
    pub const fn new(r: u8, g: u8, b: u8) -> Self {
        Self { r, g, b }
    }
}

/// Global corner-radius scale. `Medium` ≈ the current (un-themed) look.
#[derive(Facet, Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[repr(C)]
pub enum Corner { None, Small, Medium, Large }

/// Global spacing scale. `Comfortable` ≈ the current (un-themed) spacing.
#[derive(Facet, Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[repr(C)]
pub enum Density { Compact, Comfortable }

/// A finite, cross-platform font family (maps to each platform's nearest system
/// font design — no bundled font files). `System` ≈ the current look.
#[derive(Facet, Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[repr(C)]
pub enum FontFamily { System, Rounded, Serif, Monospace }

/// App branding as data — the visual twin of `dark_mode`. Set on a [`Widget::Scaffold`]
/// (`theme: None` = the framework defaults, i.e. no visual change). The shell maps these
/// to its native theming: `seed` → the brand/primary color (Android M3 scheme / iOS tint /
/// web `--primary`), plus a global corner, spacing, and font choice.
#[derive(Facet, Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[repr(C)]
pub struct Theme {
    pub seed: Rgb,
    pub corner: Corner,
    pub density: Density,
    pub font: FontFamily,
}

/// `Theme::default()` matches the framework's un-themed look as closely as a theme can
/// (medium corners, comfortable spacing, system font) with a neutral indigo seed — so an
/// app can override just the bits it cares about: `Theme { seed: brand, ..Default::default() }`.
impl Default for Theme {
    fn default() -> Self {
        Theme {
            seed: Rgb::new(0x5C, 0x6B, 0xC0), // indigo — matches the legacy default accent
            corner: Corner::Medium,
            density: Density::Comfortable,
            font: FontFamily::System,
        }
    }
}

/// A bottom-navigation tab. `selected` marks the active one; tapping sends
/// `on_select`.
#[derive(Facet, Serialize, Deserialize, Clone, Debug)]
#[repr(C)]
pub struct Tab {
    pub label: String,
    pub selected: bool,
    pub on_select: ActionToken,
}

// ------------------------------- widgets -------------------------------

/// The app-agnostic widget tree the shell renders. **Fixed across all apps.**
#[derive(Facet, Serialize, Deserialize, Clone, Debug)]
#[repr(C)]
pub enum Widget {
    // Content
    Text { content: String, style: TextStyle },
    Image { source: String, shape: ImageShape, ratio: ImageRatio },
    Badge { label: String, tone: Tone },
    /// Small non-interactive colored dot — a project/identity hint.
    ColorDot { color: ProjectColor },
    Divider,
    Spacer { size: Spacing },
    // Layout
    Row { children: Vec<Widget> },
    Column { children: Vec<Widget> },
    /// Card; tappable when `on_press` is set.
    Card { child: Box<Widget>, style: CardStyle, on_press: Option<ActionToken> },
    /// Z-stack: children layered back-to-front, positioned by `align`. With
    /// `scrim`, the first child is a background image, darkened for legibility,
    /// and the rest render on top in light content.
    Box { children: Vec<Widget>, align: BoxAlign, scrim: bool },
    /// Fixed 2-column grid; children flow left-to-right, top-to-bottom.
    Grid { children: Vec<Widget> },
    // Input
    Button { label: String, style: ButtonStyle, on_press: ActionToken },
    IconButton { icon: Icon, on_press: ActionToken },
    Chip { label: String, selected: bool, on_press: ActionToken },
    TextField { id: String, placeholder: String, value: String },
    Toggle { id: String, label: String, value: bool },
    Checkbox { id: String, label: String, value: bool },
    /// Continuous 0..=`max` slider; emits `Input { id, Int }`.
    Slider { id: String, value: i32, max: i32 },
    /// Numeric stepper with −/+ controls carrying their own events.
    Stepper { value: i32, on_decrement: ActionToken, on_increment: ActionToken },
    /// App shell: a top bar (`title` + optional `back`), a scrollable `body`,
    /// and bottom-nav `tabs`. `dark_mode` is theme-as-data — the shell themes
    /// the whole app from it.
    ///
    /// `route` + `depth` drive navigation: the shell animates the body when
    /// `route` (the current screen's identity) changes — slide for push/pop
    /// (direction from whether `depth` grew or shrank), crossfade for a lateral
    /// move at the same depth — and wires the system back button to `back`.
    Scaffold {
        title: String,
        body: Box<Widget>,
        tabs: Vec<Tab>,
        back: Option<ActionToken>,
        dark_mode: bool,
        /// App branding (brand color, corner, density, font). `None` = framework
        /// defaults (no visual change) — theme-as-data, the visual twin of `dark_mode`.
        theme: Option<Theme>,
        route: String,
        depth: u32,
    },
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::Serialize;
    use serde::de::DeserializeOwned;

    // Round-trips the ABI without requiring `PartialEq` on the wire types:
    // serialize → deserialize → re-serialize, and compare the two encodings.
    fn round_trips<T: Serialize + DeserializeOwned>(value: &T) {
        let a = serde_json::to_string(value).expect("serialize");
        let back: T = serde_json::from_str(&a).expect("deserialize");
        let b = serde_json::to_string(&back).expect("re-serialize");
        assert_eq!(a, b);
    }

    #[test]
    fn action_round_trips() {
        round_trips(&Action::Start);
        round_trips(&Action::Fired { token: "tok".to_string() });
        round_trips(&Action::Input { id: "field".to_string(), value: InputValue::Bool(true) });
        round_trips(&Action::Restore { data: "{}".to_string() });
    }

    #[test]
    fn widget_round_trips() {
        round_trips(&Widget::Text { content: "hi".to_string(), style: TextStyle::Title });
        round_trips(&Widget::ColorDot { color: ProjectColor::Teal });
        // Un-themed scaffold (theme: None) — the default, must round-trip.
        round_trips(&Widget::Scaffold {
            title: "T".to_string(),
            body: Box::new(Widget::Divider),
            tabs: vec![Tab { label: "A".to_string(), selected: true, on_select: "t".to_string() }],
            back: Some("b".to_string()),
            dark_mode: true,
            theme: None,
            route: "r".to_string(),
            depth: 2,
        });
        // Themed scaffold — all four theme knobs must round-trip.
        round_trips(&Widget::Scaffold {
            title: "T".to_string(),
            body: Box::new(Widget::Divider),
            tabs: vec![],
            back: None,
            dark_mode: false,
            theme: Some(Theme {
                seed: Rgb::new(0xC8, 0x5A, 0x3C),
                corner: Corner::Large,
                density: Density::Compact,
                font: FontFamily::Rounded,
            }),
            route: "r".to_string(),
            depth: 1,
        });
    }
}
