use mobiler_core::{Cx, InputValue, MobilerApp, MobilerShell, Widget, button, column, text, text_field};
use serde::{Deserialize, Serialize};

/// The app's typed domain events. Mobiler serializes these into opaque tokens
/// behind the scenes — the shell never sees this type.
#[derive(Serialize, Deserialize, Clone, Debug)]
pub enum Msg {
    Increment,
    Greet,
    TryMissing,
}

#[derive(Default)]
pub struct Model {
    count: i32,
    name: String,
}

#[derive(Default)]
pub struct Counter;

impl MobilerApp for Counter {
    type Event = Msg;
    type Model = Model;

    fn update(&self, event: Msg, model: &mut Model, cx: &mut Cx) {
        match event {
            Msg::Increment => model.count += 1,
            // A device-API capability call. The shell's "toast" plugin shows it.
            Msg::Greet => {
                let who = if model.name.is_empty() {
                    "there".to_string()
                } else {
                    model.name.clone()
                };
                cx.notify("toast", "show", format!("Hello, {who}! 👋"));
            }
            // A plugin the generic shell doesn't bundle → graceful no-op + log.
            Msg::TryMissing => cx.notify("confetti", "burst", ""),
        }
    }

    fn input(&self, id: &str, value: InputValue, model: &mut Model) {
        if id == "name" {
            if let InputValue::Text(text) = value {
                model.name = text;
            }
        }
    }

    fn view(&self, model: &Model) -> Widget {
        let greeting = if model.name.is_empty() {
            "Type your name above…".to_string()
        } else {
            format!("Hello, {}!", model.name)
        };
        column(vec![
            text(format!("Count: {}", model.count)),
            button("Increment", Msg::Increment),
            text_field("name", "Your name", model.name.clone()),
            text(greeting),
            button("Say hello (toast plugin)", Msg::Greet),
            button("Try a missing plugin", Msg::TryMissing),
        ])
    }
}

/// The Crux app the FFI + codegen target. It's `MobilerShell` over our app, so
/// its `Event`/`ViewModel` are the fixed ABI types → the shell is generic.
pub type App = MobilerShell<Counter>;

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn increment_via_typed_event() {
        let app = Counter;
        let mut model = Model::default();
        let mut cx = Cx::default();
        app.update(Msg::Increment, &mut model, &mut cx);
        assert_eq!(model.count, 1);
    }

    #[test]
    fn input_updates_name() {
        let app = Counter;
        let mut model = Model::default();
        app.input("name", InputValue::Text("Ada".to_string()), &mut model);
        assert_eq!(model.name, "Ada");
    }
}
