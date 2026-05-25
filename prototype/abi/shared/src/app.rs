use mobiler_core::{Cx, InputValue, MobilerApp, MobilerShell, Widget, button, column, text, text_field};
use serde::{Deserialize, Serialize};

/// The app's typed domain events. Mobiler serializes these into opaque tokens
/// behind the scenes — the shell never sees this type.
#[derive(Serialize, Deserialize, Clone, Debug)]
pub enum Msg {
    Increment,
    Greet,
    TryMissing,
    GetDevice,
    DeviceLoaded(String),
}

#[derive(Default)]
pub struct Model {
    count: i32,
    name: String,
    device: String,
}

#[derive(Default)]
pub struct Counter;

impl MobilerApp for Counter {
    type Event = Msg;
    type Model = Model;

    fn update(&self, event: Msg, model: &mut Model, cx: &mut Cx<Msg>) {
        match event {
            Msg::Increment => model.count += 1,
            // Fire-and-forget capability: the shell's "toast" plugin shows it.
            Msg::Greet => {
                let who = if model.name.is_empty() { "there".to_string() } else { model.name.clone() };
                cx.notify("toast", "show", format!("Hello, {who}! 👋"));
            }
            // A plugin the generic shell doesn't bundle → graceful no-op + log.
            Msg::TryMissing => cx.notify("confetti", "burst", ""),
            // Request/response capability: ask the "device" plugin for the model;
            // the reply comes back as a typed Msg::DeviceLoaded.
            Msg::GetDevice => cx.plugin("device", "model", "", |resp| {
                Msg::DeviceLoaded(if resp.ok { resp.output } else { format!("error: {}", resp.output) })
            }),
            Msg::DeviceLoaded(model_name) => model.device = model_name,
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
        let device = if model.device.is_empty() {
            "Device: (tap below)".to_string()
        } else {
            format!("Device: {}", model.device)
        };
        column(vec![
            text(format!("Count: {}", model.count)),
            button("Increment", Msg::Increment),
            text_field("name", "Your name", model.name.clone()),
            text(greeting),
            button("Say hello (toast plugin)", Msg::Greet),
            button("Try a missing plugin", Msg::TryMissing),
            text(device),
            button("Get device model (request/response)", Msg::GetDevice),
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
        app.update(Msg::Increment, &mut model, &mut Cx::default());
        assert_eq!(model.count, 1);
    }

    #[test]
    fn device_loaded_updates_model() {
        let app = Counter;
        let mut model = Model::default();
        app.update(Msg::DeviceLoaded("Pixel".to_string()), &mut model, &mut Cx::default());
        assert_eq!(model.device, "Pixel");
    }

    #[test]
    fn input_updates_name() {
        let app = Counter;
        let mut model = Model::default();
        app.input("name", InputValue::Text("Ada".to_string()), &mut model);
        assert_eq!(model.name, "Ada");
    }
}
