use mobiler_core::{
    ButtonStyle, CardStyle, Cx, Icon, ImageRatio, ImageShape, InputValue, MobilerApp, MobilerShell,
    Tone, Widget, badge, button, caption, card, checkbox, chip, column, divider, grid, icon_button,
    image, row, scaffold, slider, spacer, stack, stepper, subtitle, switch, tab, text, text_field,
    title,
};
use mobiler_core::{BoxAlign, Spacing};
use serde::{Deserialize, Serialize};

const HERO: &str = "https://images.unsplash.com/photo-1509042239860-f550ce710b93?w=1200&q=80";

/// Typed domain events — serialized into opaque tokens; the shell never sees them.
#[derive(Serialize, Deserialize, Clone, Debug)]
pub enum Msg {
    Bump,
    Inc,
    Dec,
    Greet,
    TryMissing,
    GetDevice,
    DeviceLoaded(String),
    SelectTab(String),
}

#[derive(Default)]
pub struct Model {
    count: i32,
    qty: i32,
    level: i32,
    name: String,
    notify: bool,
    agree: bool,
    device: String,
    tab: String,
    dark: bool,
}

#[derive(Default)]
pub struct Gallery;

impl MobilerApp for Gallery {
    type Event = Msg;
    type Model = Model;

    fn update(&self, event: Msg, model: &mut Model, cx: &mut Cx<Msg>) {
        match event {
            Msg::Bump => model.count += 1,
            Msg::Inc => model.qty += 1,
            Msg::Dec => model.qty = (model.qty - 1).max(0),
            Msg::Greet => {
                let who = if model.name.is_empty() { "there".to_string() } else { model.name.clone() };
                cx.notify("toast", "show", format!("Hello, {who}! 👋"));
            }
            Msg::TryMissing => cx.notify("confetti", "burst", ""),
            Msg::GetDevice => cx.plugin("device", "model", "", |resp| {
                Msg::DeviceLoaded(if resp.ok { resp.output } else { format!("error: {}", resp.output) })
            }),
            Msg::DeviceLoaded(name) => model.device = name,
            Msg::SelectTab(t) => model.tab = t,
        }
    }

    fn input(&self, id: &str, value: InputValue, model: &mut Model) {
        match (id, value) {
            ("name", InputValue::Text(v)) => model.name = v,
            ("notify", InputValue::Bool(v)) => model.notify = v,
            ("agree", InputValue::Bool(v)) => model.agree = v,
            ("dark", InputValue::Bool(v)) => model.dark = v,
            ("level", InputValue::Int(v)) => model.level = v as i32,
            _ => {}
        }
    }

    fn view(&self, model: &Model) -> Widget {
        let active = if model.tab.is_empty() { "gallery" } else { model.tab.as_str() };
        let body = match active {
            "settings" => settings_body(model),
            _ => gallery_body(model),
        };
        scaffold(
            "Mobiler",
            model.dark,
            vec![
                tab("Gallery", active == "gallery", Msg::SelectTab("gallery".to_string())),
                tab("Settings", active == "settings", Msg::SelectTab("settings".to_string())),
            ],
            body,
        )
    }
}

fn gallery_body(model: &Model) -> Widget {
    let greeting = if model.name.is_empty() {
        "Type your name above…".to_string()
    } else {
        format!("Hello, {}!", model.name)
    };
    column(vec![
        // Hero: image with overlaid text + scrim (Box).
        stack(
            BoxAlign::BottomStart,
            true,
            vec![
                image(HERO, ImageShape::Rounded, ImageRatio::Wide),
                column(vec![subtitle("Rust + Compose"), text("UI in Rust, native widgets.")]),
            ],
        ),
        spacer(Spacing::Sm),
        subtitle("Buttons & icons"),
        row(vec![
            button("Filled", ButtonStyle::Filled, Msg::Bump),
            button("Outlined", ButtonStyle::Outlined, Msg::Bump),
            button("Text", ButtonStyle::Text, Msg::Bump),
        ]),
        row(vec![
            icon_button(Icon::Add, Msg::Bump),
            icon_button(Icon::Edit, Msg::Bump),
            icon_button(Icon::Delete, Msg::Bump),
            badge(format!("bumped {}×", model.count), Tone::Info),
        ]),
        divider(),
        subtitle("Badges"),
        row(vec![
            badge("Neutral", Tone::Neutral),
            badge("Success", Tone::Success),
            badge("Warning", Tone::Warning),
            badge("Danger", Tone::Danger),
        ]),
        divider(),
        subtitle("Card & grid"),
        card(column(vec![subtitle("Card title"), text("Cards group content.")]), CardStyle::Elevated),
        grid(vec![
            card(column(vec![image("https://picsum.photos/seed/m1/400", ImageShape::Rounded, ImageRatio::Square), caption("One")]), CardStyle::Filled),
            card(column(vec![image("https://picsum.photos/seed/m2/400", ImageShape::Rounded, ImageRatio::Square), caption("Two")]), CardStyle::Filled),
        ]),
        divider(),
        subtitle("Inputs"),
        text_field("name", "Your name", model.name.clone()),
        text(greeting),
        switch("notify", "Notifications", model.notify),
        checkbox("agree", "I agree to the terms", model.agree),
        caption(format!("Level: {}%", model.level)),
        slider("level", model.level, 100),
        row(vec![text(format!("Quantity: {}", model.qty)), stepper(model.qty, Msg::Dec, Msg::Inc)]),
        row(vec![chip("All", true, Msg::Bump), chip("Popular", false, Msg::Bump), chip("New", false, Msg::Bump)]),
        divider(),
        subtitle("Capabilities (toast)"),
        row(vec![
            button("Toast", ButtonStyle::Outlined, Msg::Greet),
            button("Missing plugin", ButtonStyle::Outlined, Msg::TryMissing),
        ]),
        spacer(Spacing::Lg),
    ])
}

fn settings_body(model: &Model) -> Widget {
    column(vec![
        subtitle("Settings"),
        switch("dark", "Dark mode", model.dark),
        caption("Dark mode is theme-as-data — toggled in Rust, themed by the shell."),
        divider(),
        subtitle("Device (request/response capability)"),
        text(if model.device.is_empty() { "Device: (tap below)".to_string() } else { format!("Device: {}", model.device) }),
        button("Get device model", ButtonStyle::Filled, Msg::GetDevice),
        spacer(Spacing::Lg),
    ])
}

/// The Crux app the FFI + codegen target.
pub type App = MobilerShell<Gallery>;

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn events_update_model() {
        let app = Gallery;
        let mut model = Model::default();
        app.update(Msg::Bump, &mut model, &mut Cx::default());
        app.update(Msg::SelectTab("settings".into()), &mut model, &mut Cx::default());
        app.update(Msg::DeviceLoaded("Pixel".into()), &mut model, &mut Cx::default());
        assert_eq!(model.count, 1);
        assert_eq!(model.tab, "settings");
        assert_eq!(model.device, "Pixel");
    }

    #[test]
    fn inputs_route_by_id() {
        let app = Gallery;
        let mut model = Model::default();
        app.input("name", InputValue::Text("Ada".into()), &mut model);
        app.input("dark", InputValue::Bool(true), &mut model);
        app.input("level", InputValue::Int(42), &mut model);
        assert_eq!(model.name, "Ada");
        assert!(model.dark);
        assert_eq!(model.level, 42);
    }
}
