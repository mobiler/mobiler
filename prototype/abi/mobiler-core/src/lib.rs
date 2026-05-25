//! Mobiler runtime — the developer-facing API.
//!
//! Implement [`MobilerApp`] with your **typed** events, model, and view. Mobiler
//! wraps it in [`MobilerShell`], a Crux app that speaks the fixed UI ABI
//! ([`mobiler_ui`]). You never touch the wire protocol (`Action`, tokens).
//!
//! Device APIs are **capabilities** dispatched to native plugins. From `update`,
//! via the [`Cx`]:
//! - [`Cx::notify`] — fire-and-forget (e.g. a toast).
//! - [`Cx::plugin`] — request/response: the plugin's [`PluginResponse`] comes
//!   back to your `update` as a typed event you choose.

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

/// Built-in capabilities the generic shell fulfils. `Render` redraws the UI;
/// the two `Plugin*` variants are opaque envelopes dispatched by name to a
/// native plugin registry, so adding a plugin never changes the wire ABI.
#[effect(facet_typegen)]
#[derive(Debug)]
pub enum Effect {
    Render(RenderOperation),
    /// Fire-and-forget plugin call (shell does not resolve).
    PluginNotify(PluginNotify),
    /// Request/response plugin call (shell resolves with a [`PluginResponse`]).
    Plugin(PluginCall),
}

/// Fire-and-forget plugin call.
#[derive(Facet, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct PluginNotify {
    pub plugin: String,
    pub op: String,
    pub input: String,
}
impl Operation for PluginNotify {
    type Output = ();
}

/// Request/response plugin call.
#[derive(Facet, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct PluginCall {
    pub plugin: String,
    pub op: String,
    pub input: String,
}
impl Operation for PluginCall {
    type Output = PluginResponse;
}

/// The result of a request/response plugin call. `output` is opaque
/// (plugin-specific) — typically JSON, or an error message when `!ok`.
#[derive(Facet, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct PluginResponse {
    pub ok: bool,
    pub output: String,
}

type Continuation<E> = Box<dyn FnOnce(PluginResponse) -> E + Send>;

/// Effects an app requests during `update`, generic over the app's event type so
/// continuations stay fully typed.
pub struct Cx<E> {
    notifications: Vec<PluginNotify>,
    requests: Vec<(PluginCall, Continuation<E>)>,
}

impl<E> Default for Cx<E> {
    fn default() -> Self {
        Self { notifications: Vec::new(), requests: Vec::new() }
    }
}

impl<E> Cx<E> {
    /// Fire-and-forget call to a native plugin (no result awaited).
    pub fn notify(&mut self, plugin: impl Into<String>, op: impl Into<String>, input: impl Into<String>) {
        self.notifications.push(PluginNotify { plugin: plugin.into(), op: op.into(), input: input.into() });
    }

    /// Request/response call: when the plugin replies, `then(response)` produces
    /// the typed event delivered back to your `update`.
    pub fn plugin(
        &mut self,
        plugin: impl Into<String>,
        op: impl Into<String>,
        input: impl Into<String>,
        then: impl FnOnce(PluginResponse) -> E + Send + 'static,
    ) {
        self.requests
            .push((PluginCall { plugin: plugin.into(), op: op.into(), input: input.into() }, Box::new(then)));
    }
}

/// What a Mobiler app implements. Write typed domain events; Mobiler serializes
/// them into opaque tokens behind the scenes.
pub trait MobilerApp: Default {
    /// Your typed domain events (serialized into opaque action tokens).
    type Event: Serialize + DeserializeOwned + Send + 'static;
    /// Your app state.
    type Model: Default;

    /// Handle a domain event. Use `cx` to call device-API plugins.
    fn update(&self, event: Self::Event, model: &mut Self::Model, cx: &mut Cx<Self::Event>);

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
        let mut cx = Cx::<A::Event>::default();
        match action {
            Action::Fired { token } => {
                if let Ok(event) = serde_json::from_str::<A::Event>(&token) {
                    app.update(event, model, &mut cx);
                }
            }
            Action::Input { id, value } => app.input(&id, value, model),
        }

        let mut commands: Vec<Command<Effect, Action>> = Vec::new();
        for op in cx.notifications {
            commands.push(Command::notify_shell(op).build());
        }
        for (op, then) in cx.requests {
            // The plugin's response resumes the core as a typed event, carried
            // back as an opaque Action token (same mechanism as button presses).
            commands.push(Command::request_from_shell(op).then_send(move |response: PluginResponse| {
                Action::Fired { token: serde_json::to_string(&then(response)).expect("serialize event") }
            }));
        }
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
