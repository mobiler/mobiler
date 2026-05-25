//! Mobiler runtime — the developer-facing API.
//!
//! Implement [`MobilerApp`] with your **typed** events, model, and view. Mobiler
//! wraps it in [`MobilerShell`], a Crux app that speaks the fixed UI ABI
//! ([`mobiler_ui`]). You never touch the wire protocol (`Action`, tokens) — the
//! native shell stays generic and is built once for every app.
//!
//! ```ignore
//! #[derive(Serialize, Deserialize)] enum Msg { Increment }
//! #[derive(Default)] struct Model { count: i32 }
//! #[derive(Default)] struct Counter;
//! impl MobilerApp for Counter {
//!     type Event = Msg;
//!     type Model = Model;
//!     fn update(&self, e: Msg, m: &mut Model) { match e { Msg::Increment => m.count += 1 } }
//!     fn view(&self, m: &Model) -> Widget {
//!         column(vec![text(format!("Count: {}", m.count)), button("＋", Msg::Increment)])
//!     }
//! }
//! pub type App = MobilerShell<Counter>;
//! ```

use std::marker::PhantomData;

use crux_core::{
    App, Command,
    macros::effect,
    render::{RenderOperation, render},
};
use serde::{Serialize, de::DeserializeOwned};

pub use mobiler_ui::{Action, InputValue, Widget};

/// Built-in capabilities the generic shell can fulfil. (Just `Render` for now;
/// device-API plugins extend this set — and need a custom shell build.)
#[effect(facet_typegen)]
#[derive(Debug)]
pub enum Effect {
    Render(RenderOperation),
}

/// What a Mobiler app implements. Write typed domain events; Mobiler serializes
/// them into opaque tokens behind the scenes.
pub trait MobilerApp: Default {
    /// Your typed domain events (serialized into opaque action tokens).
    type Event: Serialize + DeserializeOwned;
    /// Your app state.
    type Model: Default;

    /// Handle a domain event (fired by a button, etc.).
    fn update(&self, event: Self::Event, model: &mut Self::Model);

    /// Handle an input-widget change (text field / switch / slider), routed by
    /// the widget's `id`. Defaults to ignoring it.
    fn input(&self, id: &str, value: InputValue, model: &mut Self::Model) {
        let _ = (id, value, model);
    }

    /// Build the widget tree from the current model.
    fn view(&self, model: &Self::Model) -> Widget;
}

/// Crux adapter: turns a [`MobilerApp`] into an app that speaks the fixed ABI.
/// Target `MobilerShell<YourApp>` from your FFI/codegen.
pub struct MobilerShell<A>(PhantomData<fn() -> A>);

impl<A> Default for MobilerShell<A> {
    fn default() -> Self {
        Self(PhantomData)
    }
}

impl<A: MobilerApp> App for MobilerShell<A> {
    type Event = Action;
    type Model = A::Model;
    type ViewModel = Widget;
    type Effect = Effect;

    fn update(&self, action: Action, model: &mut Self::Model) -> Command<Effect, Action> {
        let app = A::default();
        match action {
            Action::Fired { token } => {
                // Decode the opaque token back into a typed domain event.
                // Unknown/foreign tokens are ignored — that tolerance is exactly
                // what lets one shell run any app.
                if let Ok(event) = serde_json::from_str::<A::Event>(&token) {
                    app.update(event, model);
                }
            }
            Action::Input { id, value } => app.input(&id, value, model),
        }
        render()
    }

    fn view(&self, model: &Self::Model) -> Widget {
        A::default().view(model)
    }
}

// ---- Widget builders: take TYPED events, serialize them into opaque tokens ----

#[must_use]
pub fn text(content: impl Into<String>) -> Widget {
    Widget::Text { content: content.into() }
}

#[must_use]
pub fn column(children: Vec<Widget>) -> Widget {
    Widget::Column { children }
}

/// A button whose press carries a typed domain event (serialized to a token).
#[must_use]
pub fn button<E: Serialize>(label: impl Into<String>, on_press: E) -> Widget {
    Widget::Button {
        label: label.into(),
        on_press: serde_json::to_string(&on_press).expect("serialize event"),
    }
}

#[must_use]
pub fn text_field(id: impl Into<String>, placeholder: impl Into<String>, value: impl Into<String>) -> Widget {
    Widget::TextField {
        id: id.into(),
        placeholder: placeholder.into(),
        value: value.into(),
    }
}
