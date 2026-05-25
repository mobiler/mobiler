//! Mobiler runtime — the developer-facing API.
//!
//! Implement [`MobilerApp`] with your **typed** events, model, and view (built
//! from the [builders](#functions)). Mobiler wraps it in [`MobilerShell`], a
//! Crux app speaking the fixed UI ABI ([`mobiler_ui`]); you never touch the wire
//! protocol. Device APIs are capabilities via [`Cx`].

use std::marker::PhantomData;

use crux_core::{
    App, Command,
    capability::Operation,
    macros::effect,
    render::{RenderOperation, render},
};
use facet::Facet;
use serde::{Deserialize, Serialize, de::DeserializeOwned};

pub use mobiler_ui::{
    Action, BoxAlign, ButtonStyle, CardStyle, Icon, ImageRatio, ImageShape, InputValue, Spacing,
    TextStyle, Tone, Widget,
};

// ============================ capabilities ============================

/// Built-in capabilities the generic shell fulfils.
#[effect(facet_typegen)]
#[derive(Debug)]
pub enum Effect {
    Render(RenderOperation),
    /// Fire-and-forget plugin call (shell does not resolve).
    PluginNotify(PluginNotify),
    /// Request/response plugin call (shell resolves with a [`PluginResponse`]).
    Plugin(PluginCall),
}

#[derive(Facet, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct PluginNotify {
    pub plugin: String,
    pub op: String,
    pub input: String,
}
impl Operation for PluginNotify {
    type Output = ();
}

#[derive(Facet, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct PluginCall {
    pub plugin: String,
    pub op: String,
    pub input: String,
}
impl Operation for PluginCall {
    type Output = PluginResponse;
}

#[derive(Facet, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct PluginResponse {
    pub ok: bool,
    pub output: String,
}

type Continuation<E> = Box<dyn FnOnce(PluginResponse) -> E + Send>;

/// Effects an app requests during `update`, generic over the app event type so
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
    /// Fire-and-forget call to a native plugin.
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

// ============================ the app trait ============================

/// What a Mobiler app implements. Write typed domain events; Mobiler serializes
/// them into opaque tokens behind the scenes.
pub trait MobilerApp: Default {
    type Event: Serialize + DeserializeOwned + Send + 'static;
    type Model: Default;

    fn update(&self, event: Self::Event, model: &mut Self::Model, cx: &mut Cx<Self::Event>);

    fn input(&self, id: &str, value: InputValue, model: &mut Self::Model) {
        let _ = (id, value, model);
    }

    fn view(&self, model: &Self::Model) -> Widget;
}

/// Crux adapter: turns a [`MobilerApp`] into an app speaking the fixed ABI.
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

// ============================ widget builders ============================
// Action-carrying builders take a TYPED event and serialize it into a token.

fn tok<E: Serialize>(event: E) -> String {
    serde_json::to_string(&event).expect("serialize event")
}

#[must_use]
pub fn styled(content: impl Into<String>, style: TextStyle) -> Widget {
    Widget::Text { content: content.into(), style }
}
#[must_use]
pub fn text(content: impl Into<String>) -> Widget { styled(content, TextStyle::Body) }
#[must_use]
pub fn title(content: impl Into<String>) -> Widget { styled(content, TextStyle::Title) }
#[must_use]
pub fn subtitle(content: impl Into<String>) -> Widget { styled(content, TextStyle::Subtitle) }
#[must_use]
pub fn caption(content: impl Into<String>) -> Widget { styled(content, TextStyle::Caption) }
#[must_use]
pub fn emphasis(content: impl Into<String>) -> Widget { styled(content, TextStyle::Emphasis) }

#[must_use]
pub fn image(source: impl Into<String>, shape: ImageShape, ratio: ImageRatio) -> Widget {
    Widget::Image { source: source.into(), shape, ratio }
}
#[must_use]
pub fn badge(label: impl Into<String>, tone: Tone) -> Widget {
    Widget::Badge { label: label.into(), tone }
}
#[must_use]
pub fn divider() -> Widget { Widget::Divider }
#[must_use]
pub fn spacer(size: Spacing) -> Widget { Widget::Spacer { size } }

#[must_use]
pub fn row(children: Vec<Widget>) -> Widget { Widget::Row { children } }
#[must_use]
pub fn column(children: Vec<Widget>) -> Widget { Widget::Column { children } }
#[must_use]
pub fn card(child: Widget, style: CardStyle) -> Widget {
    Widget::Card { child: Box::new(child), style }
}
/// Z-stack/overlay (the `Box` widget). With `scrim`, the first child is a
/// darkened background and the rest render on top.
#[must_use]
pub fn stack(align: BoxAlign, scrim: bool, children: Vec<Widget>) -> Widget {
    Widget::Box { children, align, scrim }
}
#[must_use]
pub fn grid(children: Vec<Widget>) -> Widget { Widget::Grid { children } }

#[must_use]
pub fn button<E: Serialize>(label: impl Into<String>, style: ButtonStyle, on_press: E) -> Widget {
    Widget::Button { label: label.into(), style, on_press: tok(on_press) }
}
#[must_use]
pub fn icon_button<E: Serialize>(icon: Icon, on_press: E) -> Widget {
    Widget::IconButton { icon, on_press: tok(on_press) }
}
#[must_use]
pub fn chip<E: Serialize>(label: impl Into<String>, selected: bool, on_press: E) -> Widget {
    Widget::Chip { label: label.into(), selected, on_press: tok(on_press) }
}
#[must_use]
pub fn text_field(id: impl Into<String>, placeholder: impl Into<String>, value: impl Into<String>) -> Widget {
    Widget::TextField { id: id.into(), placeholder: placeholder.into(), value: value.into() }
}
#[must_use]
pub fn switch(id: impl Into<String>, label: impl Into<String>, value: bool) -> Widget {
    Widget::Switch { id: id.into(), label: label.into(), value }
}
#[must_use]
pub fn checkbox(id: impl Into<String>, label: impl Into<String>, value: bool) -> Widget {
    Widget::Checkbox { id: id.into(), label: label.into(), value }
}
#[must_use]
pub fn slider(id: impl Into<String>, value: i32, max: i32) -> Widget {
    Widget::Slider { id: id.into(), value, max }
}
#[must_use]
pub fn stepper<E: Serialize>(value: i32, on_decrement: E, on_increment: E) -> Widget {
    Widget::Stepper { value, on_decrement: tok(on_decrement), on_increment: tok(on_increment) }
}
