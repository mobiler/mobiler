use crux_core::{
    App, Command,
    macros::effect,
    render::{RenderOperation, render},
};
use facet::Facet;
use serde::{Deserialize, Serialize};

#[derive(Facet, Serialize, Deserialize, Clone, Debug)]
#[repr(C)]
pub enum Event {
    /// Generic demo action (wired to the starter's buttons/chips).
    Increment,
    Decrement,
    /// A text field changed; `id` identifies which one.
    TextChanged { id: String, value: String },
    /// A switch/checkbox toggled; `id` identifies which one.
    Toggled { id: String, value: bool },
    /// A slider moved; `id` identifies which one.
    SliderChanged { id: String, value: i32 },
}

#[effect(facet_typegen)]
#[derive(Debug)]
pub enum Effect {
    Render(RenderOperation),
}

#[derive(Default)]
pub struct Model {
    count: i32,
    level: i32,
    name: String,
    notify: bool,
    agree: bool,
}

// ---------- Design-system tokens (intent in Rust; concrete look in the shell) ----------

/// Text role. Maps to a Material type-scale token in the render layer.
#[derive(Facet, Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[repr(C)]
pub enum TextStyle {
    Body,
    Title,
    Subtitle,
    Caption,
    Emphasis,
}

#[derive(Facet, Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[repr(C)]
pub enum ButtonStyle {
    Filled,
    Outlined,
    Text,
}

#[derive(Facet, Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[repr(C)]
pub enum CardStyle {
    Elevated,
    Outlined,
    Filled,
}

/// Semantic status color (distinct from brand/identity color).
#[derive(Facet, Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[repr(C)]
pub enum Tone {
    Neutral,
    Success,
    Warning,
    Danger,
    Info,
}

/// Vertical/horizontal spacing token.
#[derive(Facet, Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[repr(C)]
pub enum Spacing {
    Xs,
    Sm,
    Md,
    Lg,
    Xl,
}

/// A small, finite icon set. Add variants here + a mapping in the shell.
#[derive(Facet, Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[repr(C)]
pub enum Icon {
    Delete,
    Add,
    Edit,
    Close,
    Settings,
    Check,
    Star,
}

/// How an image's corners are treated.
#[derive(Facet, Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[repr(C)]
pub enum ImageShape {
    Square,
    Rounded,
    Circle,
}

/// Image aspect ratio token.
#[derive(Facet, Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[repr(C)]
pub enum ImageRatio {
    Wide,
    Square,
    Tall,
}

/// Where a `Box`'s overlaid content sits within the stack.
#[derive(Facet, Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[repr(C)]
pub enum BoxAlign {
    TopStart,
    TopEnd,
    Center,
    BottomStart,
    BottomCenter,
    BottomEnd,
}

#[derive(Facet, Serialize, Deserialize, Clone, Debug)]
#[repr(C)]
pub enum Widget {
    // Content
    Text { content: String, style: TextStyle },
    Image { source: String, shape: ImageShape, ratio: ImageRatio },
    Badge { label: String, tone: Tone },
    Divider,
    Spacer { size: Spacing },
    // Layout
    Row { children: Vec<Widget> },
    Column { children: Vec<Widget> },
    Card { child: Box<Widget>, style: CardStyle },
    /// Z-stack: children layered back-to-front, positioned by `align`. With
    /// `scrim`, the first child is a background image, darkened for legibility,
    /// and the rest render on top with light content.
    Box {
        children: Vec<Widget>,
        align: BoxAlign,
        scrim: bool,
    },
    /// Fixed 2-column grid; children flow left-to-right, top-to-bottom.
    Grid { children: Vec<Widget> },
    // Input
    Button { label: String, style: ButtonStyle, on_press: Event },
    IconButton { icon: Icon, on_press: Event },
    Chip { label: String, selected: bool, on_press: Event },
    TextField { id: String, placeholder: String, value: String },
    Switch { id: String, label: String, value: bool },
    Checkbox { id: String, label: String, value: bool },
    /// Continuous 0..=`max` slider; emits `SliderChanged { id, value }`.
    Slider { id: String, value: i32, max: i32 },
    /// Numeric stepper with −/+ controls carrying their own events.
    Stepper { value: i32, on_decrement: Event, on_increment: Event },
}

pub type ViewModel = Widget;

#[derive(Default)]
pub struct {{NAME}}App;

impl App for {{NAME}}App {
    type Event = Event;
    type Model = Model;
    type ViewModel = ViewModel;
    type Effect = Effect;

    fn update(&self, event: Event, model: &mut Model) -> Command<Effect, Event> {
        match event {
            Event::Increment => model.count += 1,
            Event::Decrement => model.count -= 1,
            Event::TextChanged { id, value } => {
                if id == "name" {
                    model.name = value;
                }
            }
            Event::Toggled { id, value } => match id.as_str() {
                "notify" => model.notify = value,
                "agree" => model.agree = value,
                _ => {}
            },
            Event::SliderChanged { id, value } => {
                if id == "level" {
                    model.level = value.clamp(0, 100);
                }
            }
        }
        render()
    }

    fn view(&self, model: &Model) -> ViewModel {
        Widget::Column {
            children: vec![
                title("Welcome to Mobiler"),
                caption("A starter showcase of the built-in widgets. Edit shared/src/app.rs to make it yours."),
                divider(),
                // Hero image with overlaid text (Box + Image + scrim)
                Widget::Box {
                    align: BoxAlign::BottomStart,
                    scrim: true,
                    children: vec![
                        Widget::Image {
                            source: "https://picsum.photos/seed/mobiler/1200/700".to_string(),
                            shape: ImageShape::Rounded,
                            ratio: ImageRatio::Wide,
                        },
                        Widget::Column {
                            children: vec![styled("Rust + Compose", TextStyle::Subtitle), body("UI written in Rust, rendered to native widgets.")],
                        },
                    ],
                },
                spacer(Spacing::Sm),
                subtitle("Buttons"),
                Widget::Row {
                    children: vec![
                        Widget::Button { label: "Filled".to_string(), style: ButtonStyle::Filled, on_press: Event::Increment },
                        Widget::Button { label: "Outlined".to_string(), style: ButtonStyle::Outlined, on_press: Event::Increment },
                        Widget::Button { label: "Text".to_string(), style: ButtonStyle::Text, on_press: Event::Increment },
                    ],
                },
                Widget::Row {
                    children: vec![
                        Widget::IconButton { icon: Icon::Add, on_press: Event::Increment },
                        Widget::IconButton { icon: Icon::Edit, on_press: Event::Increment },
                        Widget::IconButton { icon: Icon::Delete, on_press: Event::Increment },
                        Widget::Badge { label: format!("tapped {}×", model.count), tone: Tone::Info },
                    ],
                },
                divider(),
                subtitle("Badges"),
                Widget::Row {
                    children: vec![
                        Widget::Badge { label: "Neutral".to_string(), tone: Tone::Neutral },
                        Widget::Badge { label: "Success".to_string(), tone: Tone::Success },
                        Widget::Badge { label: "Warning".to_string(), tone: Tone::Warning },
                        Widget::Badge { label: "Danger".to_string(), tone: Tone::Danger },
                    ],
                },
                divider(),
                subtitle("Card & grid"),
                Widget::Card {
                    style: CardStyle::Elevated,
                    child: Box::new(Widget::Column {
                        children: vec![styled("Card title", TextStyle::Subtitle), body("Cards group related content. Styles: elevated, outlined, filled.")],
                    }),
                },
                Widget::Grid {
                    children: vec![
                        gallery_tile(1, "Sunrise"),
                        gallery_tile(2, "Forest"),
                        gallery_tile(3, "Ocean"),
                        gallery_tile(4, "Desert"),
                    ],
                },
                divider(),
                subtitle("Inputs"),
                Widget::TextField { id: "name".to_string(), placeholder: "Your name".to_string(), value: model.name.clone() },
                body(&greeting(model)),
                Widget::Switch { id: "notify".to_string(), label: "Enable notifications".to_string(), value: model.notify },
                Widget::Checkbox { id: "agree".to_string(), label: "I agree to the terms".to_string(), value: model.agree },
                Widget::Row {
                    children: vec![
                        Widget::Chip { label: "All".to_string(), selected: true, on_press: Event::Increment },
                        Widget::Chip { label: "Popular".to_string(), selected: false, on_press: Event::Increment },
                        Widget::Chip { label: "New".to_string(), selected: false, on_press: Event::Increment },
                    ],
                },
                divider(),
                subtitle("Slider & stepper"),
                caption(&format!("Level: {}%", model.level)),
                Widget::Slider { id: "level".to_string(), value: model.level, max: 100 },
                Widget::Row {
                    children: vec![
                        body("Count"),
                        Widget::Stepper { value: model.count, on_decrement: Event::Decrement, on_increment: Event::Increment },
                    ],
                },
                spacer(Spacing::Lg),
            ],
        }
    }
}

// ---------- View helpers (one-liner constructors keep view() readable) ----------

fn styled(content: &str, style: TextStyle) -> Widget {
    Widget::Text { content: content.to_string(), style }
}
fn title(s: &str) -> Widget { styled(s, TextStyle::Title) }
fn subtitle(s: &str) -> Widget { styled(s, TextStyle::Subtitle) }
fn caption(s: &str) -> Widget { styled(s, TextStyle::Caption) }
fn body(s: &str) -> Widget { styled(s, TextStyle::Body) }
fn divider() -> Widget { Widget::Divider }
fn spacer(size: Spacing) -> Widget { Widget::Spacer { size } }

fn gallery_tile(seed: u32, label: &str) -> Widget {
    Widget::Card {
        style: CardStyle::Filled,
        child: Box::new(Widget::Column {
            children: vec![
                Widget::Image {
                    source: format!("https://picsum.photos/seed/mobiler{seed}/400"),
                    shape: ImageShape::Rounded,
                    ratio: ImageRatio::Square,
                },
                styled(label, TextStyle::Caption),
            ],
        }),
    }
}

fn greeting(model: &Model) -> String {
    if model.name.is_empty() {
        "Type your name above…".to_string()
    } else {
        format!("Hello, {}!", model.name)
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn increment_counts() {
        let app = {{NAME}}App;
        let mut model = Model::default();
        app.update(Event::Increment, &mut model).expect_only_render();
        assert_eq!(model.count, 1);
    }

    #[test]
    fn inputs_update_the_model() {
        let app = {{NAME}}App;
        let mut model = Model::default();
        app.update(Event::TextChanged { id: "name".to_string(), value: "Ada".to_string() }, &mut model);
        app.update(Event::Toggled { id: "notify".to_string(), value: true }, &mut model);
        assert_eq!(model.name, "Ada");
        assert!(model.notify);
        assert!(!model.agree);
    }
}
