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
pub enum CardStyle { Elevated, Outlined, Filled, Brand }

/// What a [`Widget::TextField`] accepts — selects the on-screen keyboard,
/// secure (masked) entry, and single- vs multi-line layout in one axis.
///
/// `Text` is the plain default. `Secure` masks input (passwords). `Email`,
/// `Number` (integer), `Decimal`, `Phone`, and `Url` pick the matching native
/// keyboard / input mode without masking. `Multiline` is a growable multi-row
/// text area (plain keyboard).
#[derive(Facet, Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[repr(C)]
pub enum FieldKind { Text, Secure, Email, Number, Decimal, Phone, Url, Multiline }

/// Semantic status color (distinct from brand/identity color).
#[derive(Facet, Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[repr(C)]
pub enum Tone { Neutral, Success, Warning, Danger, Info }

/// How a `Chart` draws its series.
///
/// **Cartesian** styles plot every series over the shared `labels` x-axis:
/// `Bar`/`Line` (grouped bars / one polyline per series), `StackedBar` (series stack to a total
/// per x-slot), `StackedBar100` (each x-slot fills to 100% — series as proportions).
///
/// **Circular** styles ignore the x-axis and the `axis` flag: `Pie`/`Donut` turn **each series**
/// into one wedge sized by its magnitude (`Donut` leaves a center hole); `Rings` draws concentric
/// progress arcs (Apple-Watch fitness style), one per series, swept by `sum(values) / goal`;
/// `Gauge` draws a single arc for the first series' `value / goal` with the number in the center.
#[derive(Facet, Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[repr(C)]
pub enum ChartStyle { Bar, Line, StackedBar, StackedBar100, Pie, Donut, Rings, Gauge }

/// One named data series in a [`Widget::Chart`]. Cartesian styles plot `values` across the chart's
/// x-axis `labels`; circular styles (pie/donut/rings/gauge) collapse the series to a single
/// magnitude (`values` summed). `color` overrides the auto-assigned palette slot; `goal` is the
/// denominator for `Rings`/`Gauge` progress (ignored by the other styles).
#[derive(Facet, Serialize, Deserialize, Clone, Debug, PartialEq)]
#[repr(C)]
pub struct ChartSeries {
    pub name: String,
    pub values: Vec<f32>,
    pub color: Option<Rgb>,
    pub goal: Option<f32>,
}

impl ChartSeries {
    /// A named series carrying `values`. Color falls back to the chart palette; no goal.
    #[must_use]
    pub fn new(name: impl Into<String>, values: Vec<f32>) -> Self {
        Self { name: name.into(), values, color: None, goal: None }
    }
    /// Override the auto-assigned palette color for this series.
    #[must_use]
    pub fn with_color(mut self, color: Rgb) -> Self {
        self.color = Some(color);
        self
    }
    /// Set the denominator for `Rings`/`Gauge` progress (`sum(values) / goal`). Ignored by
    /// cartesian and pie/donut styles.
    #[must_use]
    pub fn with_goal(mut self, goal: f32) -> Self {
        self.goal = Some(goal);
        self
    }
}

// ----------------------------- region chart -----------------------------
//
// A [`Widget::RegionChart`] is a variable-width stacked-region ("Marimekko" / coverage-gap)
// chart: arbitrary colored rectangles placed in a 2-D `[0, x_max] × [0, y_max]` plane, each with
// an in-cell label, plus horizontal reference lines, an irregular x-axis, an optional right-side
// bracket annotation, and a legend. The app computes the geometry; the shells map domain→pixels.

/// One rectangle in a [`Widget::RegionChart`], spanning `[x0, x1]` horizontally and `[y0, y1]`
/// vertically in the chart's domain. `label` is centered inside (empty = none); `vertical` rotates
/// it 90° for narrow columns. `color` overrides the auto-assigned palette slot.
#[derive(Facet, Serialize, Deserialize, Clone, Debug, PartialEq)]
#[repr(C)]
pub struct ChartRegion {
    pub x0: f32,
    pub x1: f32,
    pub y0: f32,
    pub y1: f32,
    pub color: Option<Rgb>,
    pub label: String,
    pub vertical: bool,
}

/// A horizontal reference line across a [`Widget::RegionChart`] at `value`, with a right-edge
/// `label` chip. `dashed` draws it dashed (e.g. a "max insured" ceiling) vs solid (a target).
#[derive(Facet, Serialize, Deserialize, Clone, Debug, PartialEq)]
#[repr(C)]
pub struct ChartRefLine {
    pub value: f32,
    pub label: String,
    pub dashed: bool,
}

/// A right-side bracket annotation spanning `[y0, y1]` with a `label` note (e.g. a ceiling band).
#[derive(Facet, Serialize, Deserialize, Clone, Debug, PartialEq)]
#[repr(C)]
pub struct ChartBracket {
    pub y0: f32,
    pub y1: f32,
    pub label: String,
    /// Show an ⓘ info marker above the label (e.g. a "Ceiling max …" note). `label` may contain
    /// `\n` for multiple lines.
    pub info: bool,
}

/// An x-axis tick on a [`Widget::RegionChart`] at domain position `at`, labelled `label`. Ticks
/// are irregular (the app places them), so shells position them by fraction, not even spacing.
#[derive(Facet, Serialize, Deserialize, Clone, Debug, PartialEq)]
#[repr(C)]
pub struct ChartTick {
    pub at: f32,
    pub label: String,
}

/// One legend entry (swatch + name) for a [`Widget::RegionChart`].
#[derive(Facet, Serialize, Deserialize, Clone, Debug, PartialEq)]
#[repr(C)]
pub struct ChartLegendItem {
    pub label: String,
    pub color: Rgb,
}

impl ChartRegion {
    /// A region spanning `[x0,x1] × [y0,y1]` with a centered `label` (palette color, horizontal).
    #[must_use]
    pub fn new(x0: f32, x1: f32, y0: f32, y1: f32, label: impl Into<String>) -> Self {
        Self { x0, x1, y0, y1, color: None, label: label.into(), vertical: false }
    }
    /// Override the fill color.
    #[must_use]
    pub fn with_color(mut self, color: Rgb) -> Self {
        self.color = Some(color);
        self
    }
    /// Render the label rotated 90° (for tall, narrow regions).
    #[must_use]
    pub fn vertical(mut self) -> Self {
        self.vertical = true;
        self
    }
}

impl ChartRefLine {
    /// A solid target line at `value` with a right-edge chip.
    #[must_use]
    pub fn target(value: f32, label: impl Into<String>) -> Self {
        Self { value, label: label.into(), dashed: false }
    }
    /// A dashed "max"/ceiling line at `value`.
    #[must_use]
    pub fn max(value: f32, label: impl Into<String>) -> Self {
        Self { value, label: label.into(), dashed: true }
    }
}

impl ChartTick {
    #[must_use]
    pub fn new(at: f32, label: impl Into<String>) -> Self {
        Self { at, label: label.into() }
    }
}

impl ChartLegendItem {
    #[must_use]
    pub fn new(label: impl Into<String>, color: Rgb) -> Self {
        Self { label: label.into(), color }
    }
}

impl ChartBracket {
    #[must_use]
    pub fn new(y0: f32, y1: f32, label: impl Into<String>) -> Self {
        Self { y0, y1, label: label.into(), info: false }
    }
    /// Show an ⓘ info marker above the label.
    #[must_use]
    pub fn with_info(mut self) -> Self {
        self.info = true;
        self
    }
}

#[derive(Facet, Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[repr(C)]
pub enum Spacing { Xs, Sm, Md, Lg, Xl }

/// A finite icon set (maps to Material icons / SF Symbols / web glyphs per shell).
/// Grouped: editing, navigation/chrome, content, and domain icons.
#[derive(Facet, Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[repr(C)]
pub enum Icon {
    // editing / status
    Delete, Add, Edit, Close, Settings, Check, Star, Info,
    // navigation / chrome
    Home, Search, Menu, Filter, Back, Forward, Down, Bell, Cart, Share, Heart, HeartFilled,
    // people / contact
    Person, People, Phone, Mail, Calendar, Clock, MapPin,
    // content / media
    Camera, Photo, Play,
    // domain
    Scissors,
}

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
    /// Optional secondary brand color. `None` ⇒ derived from `seed`. Used for the
    /// gradient on `CardStyle::Brand` (seed → accent) and as a secondary accent.
    pub accent: Option<Rgb>,
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
            accent: None,
            corner: Corner::Medium,
            density: Density::Comfortable,
            font: FontFamily::System,
        }
    }
}

/// A bottom-navigation tab. `selected` marks the active one; tapping sends
/// `on_select`. `icon` (optional) renders above the label for an icon tab bar.
#[derive(Facet, Serialize, Deserialize, Clone, Debug)]
#[repr(C)]
pub struct Tab {
    pub label: String,
    pub selected: bool,
    pub on_select: ActionToken,
    /// Optional leading icon (icon tab bar). `None` = label-only (the original look).
    pub icon: Option<Icon>,
}

/// A floating action button anchored over the scaffold body (the raised primary action).
#[derive(Facet, Serialize, Deserialize, Clone, Debug)]
#[repr(C)]
pub struct Fab {
    pub icon: Icon,
    pub on_press: ActionToken,
}

/// One option in a [`Widget::Segmented`] control (mirrors [`Tab`]). `selected` marks the
/// active segment; tapping sends `on_select`.
#[derive(Facet, Serialize, Deserialize, Clone, Debug)]
#[repr(C)]
pub struct Segment {
    pub label: String,
    pub selected: bool,
    pub on_select: ActionToken,
}

/// A modal bottom sheet anchored over the scaffold body (a scrim behind, a panel rising from
/// the bottom). Present (`Some`) ⇒ open; tapping the scrim/handle sends `on_dismiss`.
#[derive(Facet, Serialize, Deserialize, Clone, Debug)]
#[repr(C)]
pub struct Sheet {
    pub title: String,
    pub child: Box<Widget>,
    pub on_dismiss: ActionToken,
}

/// One revealed action in a `SwipeAction` row (swipe to reveal, tap to fire).
#[derive(Facet, Serialize, Deserialize, Clone, Debug)]
#[repr(C)]
pub struct SwipeButton {
    pub label: String,
    pub tone: Tone,
    pub on_tap: ActionToken,
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
    /// A circular avatar image with an optional colored status dot.
    Avatar { source: String, status: Option<Tone> },
    /// An in-app PDF viewer showing the document at `url` (a remote https URL or a local
    /// file URI). Each shell uses its native renderer — PDFKit on iOS, a paged `PdfRenderer`
    /// on Android, an `<iframe>` on web — so the app only supplies the URL (e.g. a
    /// backend-generated report). Fills its width; give it room (place in a sized container).
    PdfView { url: String },
    /// A star rating. `value` is in tenths (e.g. `48` = 4.8 of `max` stars). When `on_rate`
    /// is set (one token per star), the stars are tappable — star *i* fires `on_rate[i]`.
    Rating { value: u32, max: u8, on_rate: Option<Vec<ActionToken>> },
    /// Small non-interactive colored dot — a project/identity hint.
    ColorDot { color: ProjectColor },
    Divider,
    /// Progress indicator: `value` 0.0–1.0 for a determinate bar, `None` for an indeterminate spinner.
    Progress { value: Option<f32> },
    /// Shimmer placeholder shown while content loads.
    Skeleton,
    /// A data chart drawing one or more named `series` in the given `style` (see [`ChartStyle`]).
    /// `labels` (optional) annotate the x-axis for cartesian styles. `axis` shows y gridlines +
    /// tick values (cartesian only); `legend` shows a series swatch+name row. Non-interactive.
    Chart { series: Vec<ChartSeries>, labels: Vec<String>, style: ChartStyle, axis: bool, legend: bool },
    /// A variable-width stacked-region ("Marimekko" / coverage-gap) chart: `regions` are arbitrary
    /// colored rectangles in the `[0, x_max] × [0, y_max]` plane (each with an in-cell label),
    /// `ticks` annotate the irregular x-axis, `ref_lines` are horizontal target/max lines with
    /// right-edge chips, `bracket` is an optional right-side range annotation, and `legend` names
    /// the colors. The app supplies all geometry; shells map domain→pixels. Non-interactive.
    RegionChart {
        regions: Vec<ChartRegion>,
        ticks: Vec<ChartTick>,
        x_max: f32,
        y_max: f32,
        ref_lines: Vec<ChartRefLine>,
        bracket: Option<ChartBracket>,
        legend: Vec<ChartLegendItem>,
    },
    /// An inline month calendar. `first_weekday` is the weekday of day 1 (0=Sun..6=Sat) so the
    /// shells render leading blanks without date math; `on_day[d-1]` fires when day `d` is tapped
    /// (length = days in the month). `selected` highlights a day.
    Calendar { year: u32, month: u8, first_weekday: u8, selected: Option<u8>, on_day: Vec<ActionToken> },
    /// A list row that reveals trailing `actions` on horizontal swipe (each tappable). On web the
    /// actions render inline as a trailing button row (no gesture).
    SwipeAction { child: Box<Widget>, actions: Vec<SwipeButton> },
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
    /// Horizontally scrolling row of children (a carousel / chip rail).
    Scroller { children: Vec<Widget> },
    // Input
    Button { label: String, style: ButtonStyle, on_press: ActionToken },
    IconButton { icon: Icon, on_press: ActionToken },
    Chip { label: String, selected: bool, on_press: ActionToken },
    /// A text input. `kind` selects keyboard / secure entry / multiline
    /// (see [`FieldKind`]); `error`, when `Some`, shows an inline validation
    /// message below the field and marks it invalid. Emits `Input { id, Text }`.
    TextField { id: String, placeholder: String, value: String, kind: FieldKind, error: Option<String> },
    /// A search input (leading magnifier, pill shape); emits `Input { id, Text }` like `TextField`.
    SearchField { id: String, placeholder: String, value: String },
    /// A single-choice segmented control — exclusive options in a pill (e.g. Men/Women/Kids).
    Segmented { segments: Vec<Segment> },
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
        /// Optional floating action button (raised primary action over the body).
        fab: Option<Fab>,
        /// Optional modal bottom sheet over the body (a scrim + a panel from the bottom).
        sheet: Option<Sheet>,
        /// Pull-to-refresh: when set, the body is pull-refreshable and fires this event on pull.
        /// The app owns `refreshing` — set it true when the pull fires, clear it when the async
        /// reload completes (the shell shows a spinner while it's true).
        on_refresh: Option<ActionToken>,
        refreshing: bool,
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
        round_trips(&Widget::Chart {
            series: vec![ChartSeries { name: "s".to_string(), values: vec![1.0, 2.5, 3.0], color: None, goal: None }],
            labels: vec!["a".to_string()],
            style: ChartStyle::Bar,
            axis: true,
            legend: false,
        });
        round_trips(&Widget::RegionChart {
            regions: vec![ChartRegion::new(0.0, 3.0, 0.0, 80.0, "80%").vertical(),
                          ChartRegion::new(3.0, 21.0, 0.0, 80.0, "CHF 80'000").with_color(Rgb::new(0x8E, 0xC6, 0xBA))],
            ticks: vec![ChartTick { at: 3.0, label: "3 Mt.".to_string() }, ChartTick { at: 65.0, label: "65 J.".to_string() }],
            x_max: 65.0,
            y_max: 80.0,
            ref_lines: vec![ChartRefLine { value: 80.0, label: "CHF 80'000".to_string(), dashed: false }],
            bracket: Some(ChartBracket { y0: 60.0, y1: 80.0, label: "Ceiling".to_string(), info: true }),
            legend: vec![ChartLegendItem { label: "Gap".to_string(), color: Rgb::new(0x5A, 0x7D, 0x9A) }],
        });
        round_trips(&Widget::PdfView { url: "https://example.com/report.pdf".to_string() });
        round_trips(&Widget::TextField { id: "email".to_string(), placeholder: "you@co".to_string(), value: "".to_string(), kind: FieldKind::Email, error: None });
        round_trips(&Widget::TextField { id: "pw".to_string(), placeholder: "Password".to_string(), value: "x".to_string(), kind: FieldKind::Secure, error: Some("Too short".to_string()) });
        round_trips(&Widget::Calendar { year: 2026, month: 6, first_weekday: 1, selected: Some(15), on_day: vec!["d1".to_string(), "d2".to_string()] });
        round_trips(&Widget::SwipeAction { child: Box::new(Widget::Divider), actions: vec![SwipeButton { label: "Del".to_string(), tone: Tone::Danger, on_tap: "t".to_string() }] });
        // Un-themed scaffold (theme: None) — the default, must round-trip.
        round_trips(&Widget::Scaffold {
            title: "T".to_string(),
            body: Box::new(Widget::Divider),
            tabs: vec![Tab { label: "A".to_string(), selected: true, on_select: "t".to_string(), icon: Some(Icon::Home) }],
            back: Some("b".to_string()),
            dark_mode: true,
            theme: None,
            fab: None,
            sheet: None,
            on_refresh: None,
            refreshing: false,
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
                accent: Some(Rgb::new(0xE0, 0x6A, 0x2C)),
                corner: Corner::Large,
                density: Density::Compact,
                font: FontFamily::Rounded,
            }),
            fab: Some(Fab { icon: Icon::Calendar, on_press: "f".to_string() }),
            sheet: Some(Sheet { title: "S".to_string(), child: Box::new(Widget::Divider), on_dismiss: "d".to_string() }),
            on_refresh: Some("r".to_string()),
            refreshing: true,
            route: "r".to_string(),
            depth: 1,
        });
    }
}
