use mobiler_core::{
    ButtonStyle, CardStyle, Cx, ImageRatio, ImageShape, InputValue, MobilerApp, MobilerShell,
    Spacing, Tone, Widget, badge, button, caption, card, column, divider, image, row, spacer,
    text, text_field, title, toggle,
};
use serde::{Deserialize, Serialize};

/// Your app's typed events. Mobiler serializes these into opaque tokens behind
/// the scenes — the native shell never sees this type.
#[derive(Serialize, Deserialize, Clone, Debug)]
pub enum Msg {
    Increment,
    Greet,
}

#[derive(Default)]
pub struct Model {
    count: i32,
    name: String,
    notify: bool,
}

#[derive(Default)]
pub struct {{NAME}}App;

impl MobilerApp for {{NAME}}App {
    type Event = Msg;
    type Model = Model;

    fn update(&self, event: Msg, model: &mut Model, cx: &mut Cx<Msg>) {
        match event {
            Msg::Increment => model.count += 1,
            // A device-API capability call (handled by the shell's "toast" plugin).
            Msg::Greet => {
                let who = if model.name.is_empty() { "there".to_string() } else { model.name.clone() };
                cx.notify("toast", "show", format!("Hello, {who}! 👋"));
            }
        }
    }

    fn input(&self, id: &str, value: InputValue, model: &mut Model, _cx: &mut Cx<Msg>) {
        match (id, value) {
            ("name", InputValue::Text(v)) => model.name = v,
            ("notify", InputValue::Bool(v)) => model.notify = v,
            _ => {}
        }
    }

    fn view(&self, model: &Model) -> Widget {
        let greeting = if model.name.is_empty() {
            "Type your name above…".to_string()
        } else {
            format!("Hello, {}!", model.name)
        };
        column(vec![
            title("Welcome to {{NAME}}"),
            caption("Built with Mobiler — your UI is Rust, rendered to native widgets. Edit shared/src/app.rs to make it yours."),
            image("https://picsum.photos/seed/{{NAME}}/1200/700", ImageShape::Rounded, ImageRatio::Wide),
            divider(),
            row(vec![
                button("Tap me", ButtonStyle::Filled, Msg::Increment),
                badge(format!("count {}", model.count), Tone::Info),
            ]),
            divider(),
            card(text("This is a card — cards group related content."), CardStyle::Elevated),
            divider(),
            text_field("name", "Your name", model.name.clone()),
            text(greeting),
            toggle("notify", "Enable notifications", model.notify),
            divider(),
            button("Say hello (toast plugin)", ButtonStyle::Outlined, Msg::Greet),
            spacer(Spacing::Lg),
        ])
    }
}

/// The Crux app the FFI + codegen target. It's `MobilerShell` over your app, so
/// its `Event`/`ViewModel` are the fixed Mobiler ABI types → the native shell
/// stays generic and is built once for every app.
pub type App = MobilerShell<{{NAME}}App>;

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn increment_counts() {
        let app = {{NAME}}App;
        let mut model = Model::default();
        app.update(Msg::Increment, &mut model, &mut Cx::default());
        assert_eq!(model.count, 1);
    }

    #[test]
    fn input_sets_name() {
        let app = {{NAME}}App;
        let mut model = Model::default();
        app.input("name", InputValue::Text("Ada".to_string()), &mut model, &mut Cx::default());
        assert_eq!(model.name, "Ada");
    }
}
