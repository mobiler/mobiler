//! Mobiler runtime — the developer-facing API.
//!
//! Implement [`MobilerApp`] with your **typed** events, model, and view. Mobiler
//! wraps it in [`MobilerShell`], a Crux app that speaks the fixed UI ABI
//! ([`mobiler_ui`]). You never touch the wire protocol (`Action`, tokens) — the
//! native shell stays generic and is built once for every app.
//!
//! Device APIs are **capabilities**: the core emits an [`Effect`], the shell
//! fulfils it natively. The [`Effect::Plugin`] variant is an opaque envelope, so
//! adding a plugin never changes the wire ABI — only the shell's native plugin
//! registry. Use [`Cx::notify`] to fire a (result-less) plugin call.

use std::marker::PhantomData;

use crux_core::{
    App, Command,
    capability::Operation,
    macros::effect,
    render::{RenderOperation, render},
};
use facet::Facet;
use serde::{Deserialize, Serialize, de::DeserializeOwned};

pub use mobiler_ui::{Action, InputValue, Widget};

/// Built-in capabilities the generic shell can fulfil.
///
/// `Render` redraws the UI; `Plugin` is the **opaque extensibility point** —
/// `{plugin, op, input}` is dispatched by name to a native plugin in the shell's
/// registry. Because it's opaque, adding a plugin changes neither this enum nor
/// the generated bindings: only native registration differs (free generic shell
/// vs. custom build for premium plugins).
#[effect(facet_typegen)]
#[derive(Debug)]
pub enum Effect {
    Render(RenderOperation),
    Plugin(PluginOperation),
}

/// A call to a named native plugin. `input`/output are opaque (plugin-specific)
/// JSON, keeping the wire ABI stable.
#[derive(Facet, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct PluginOperation {
    pub plugin: String,
    pub op: String,
    pub input: String,
}

/// The result of a plugin call (for request/response capabilities).
#[derive(Facet, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct PluginResponse {
    pub ok: bool,
    pub output: String,
}

impl Operation for PluginOperation {
    type Output = PluginResponse;
}

/// Effects an app requests during `update`. (Prototype: fire-and-forget plugin
/// calls. Request/response — `cx.plugin(name, op, input, |resp| Msg)` returning
/// the result as a typed event — is the next increment, via `request_from_shell`.)
#[derive(Default)]
pub struct Cx {
    notifications: Vec<PluginOperation>,
}

impl Cx {
    /// Fire-and-forget call to a native plugin (no result awaited).
    pub fn notify(&mut self, plugin: impl Into<String>, op: impl Into<String>, input: impl Into<String>) {
        self.notifications.push(PluginOperation {
            plugin: plugin.into(),
            op: op.into(),
            input: input.into(),
        });
    }
}

/// What a Mobiler app implements. Write typed domain events; Mobiler serializes
/// them into opaque tokens behind the scenes.
pub trait MobilerApp: Default {
    /// Your typed domain events (serialized into opaque action tokens).
    type Event: Serialize + DeserializeOwned;
    /// Your app state.
    type Model: Default;

    /// Handle a domain event. Use `cx` to call device-API plugins.
    fn update(&self, event: Self::Event, model: &mut Self::Model, cx: &mut Cx);

    /// Handle an input-widget change (text field / switch / slider), by `id`.
    fn input(&self, id: &str, value: InputValue, model: &mut Self::Model) {
        let _ = (id, value, model);
    }

    /// Build the widget tree from the current model.
    fn view(&self, model: &Self::Model) -> Widget;
}

/// Crux adapter: turns a [`MobilerApp`] into an app that speaks the fixed ABI.
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
        let mut cx = Cx::default();
        match action {
            Action::Fired { token } => {
                if let Ok(event) = serde_json::from_str::<A::Event>(&token) {
                    app.update(event, model, &mut cx);
                }
            }
            Action::Input { id, value } => app.input(&id, value, model),
        }
        // Each requested plugin call becomes a fire-and-forget shell notification;
        // always re-render afterwards.
        let mut commands: Vec<Command<Effect, Action>> = cx
            .notifications
            .into_iter()
            .map(|op| Command::notify_shell(op).build())
            .collect();
        commands.push(render());
        Command::all(commands)
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
