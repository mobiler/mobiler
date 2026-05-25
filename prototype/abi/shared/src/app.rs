use crux_core::{
    App, Command,
    macros::effect,
    render::{RenderOperation, render},
};
use mobiler_ui::{Action, InputValue, Widget};
use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// The app's TYPED domain events (Option A). These never cross the FFI as types
// — they are serialized into opaque string tokens that the shell round-trips.
// The shell knows nothing about `Msg`.
// ---------------------------------------------------------------------------
#[derive(Serialize, Deserialize, Clone, Debug)]
enum Msg {
    Increment,
}

/// Serialize a typed domain event into an opaque action token.
fn token(msg: &Msg) -> String {
    serde_json::to_string(msg).expect("serialize Msg")
}

#[effect(facet_typegen)]
#[derive(Debug)]
pub enum Effect {
    Render(RenderOperation),
}

#[derive(Default)]
pub struct Model {
    count: i32,
    name: String,
}

/// The core speaks the fixed Mobiler ABI: it receives [`Action`] and returns a
/// [`Widget`] tree. Neither is app-specific, so the shell is generic.
pub type ViewModel = Widget;

#[derive(Default)]
pub struct AbiApp;

impl App for AbiApp {
    type Event = Action;
    type Model = Model;
    type ViewModel = ViewModel;
    type Effect = Effect;

    fn update(&self, event: Action, model: &mut Model) -> Command<Effect, Action> {
        match event {
            // An action widget fired: decode the opaque token back into a typed
            // domain event and handle it.
            Action::Fired { token } => match serde_json::from_str::<Msg>(&token) {
                Ok(Msg::Increment) => model.count += 1,
                Err(_) => {} // unknown/foreign token — ignore
            },
            // A value-carrying input changed, routed by id.
            Action::Input { id, value } => {
                if id == "name" {
                    if let InputValue::Text(text) = value {
                        model.name = text;
                    }
                }
            }
        }
        render()
    }

    fn view(&self, model: &Model) -> ViewModel {
        let greeting = if model.name.is_empty() {
            "Type your name above…".to_string()
        } else {
            format!("Hello, {}!", model.name)
        };
        Widget::Column {
            children: vec![
                Widget::Text { content: format!("Count: {}", model.count) },
                Widget::Button { label: "Increment".to_string(), on_press: token(&Msg::Increment) },
                Widget::TextField {
                    id: "name".to_string(),
                    placeholder: "Your name".to_string(),
                    value: model.name.clone(),
                },
                Widget::Text { content: greeting },
            ],
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn fired_token_increments() {
        let app = AbiApp;
        let mut model = Model::default();
        app.update(Action::Fired { token: token(&Msg::Increment) }, &mut model)
            .expect_only_render();
        assert_eq!(model.count, 1);
    }

    #[test]
    fn input_updates_name() {
        let app = AbiApp;
        let mut model = Model::default();
        app.update(
            Action::Input { id: "name".to_string(), value: InputValue::Text("Ada".to_string()) },
            &mut model,
        )
        .expect_only_render();
        assert_eq!(model.name, "Ada");
    }

    #[test]
    fn foreign_token_is_ignored() {
        let app = AbiApp;
        let mut model = Model::default();
        // A token this app doesn't understand must not panic the core.
        app.update(Action::Fired { token: "\"SomethingElse\"".to_string() }, &mut model);
        assert_eq!(model.count, 0);
    }
}
