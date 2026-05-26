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
    Action, BoxAlign, ButtonStyle, CardStyle, Icon, ImageRatio, ImageShape, InputValue,
    ProjectColor, Spacing, Tab, TextStyle, Tone, Widget,
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

    /// Persist `data` (handed back to [`MobilerApp::restore`] on next startup).
    pub fn save(&mut self, data: impl Into<String>) {
        self.notify("storage", "save", data);
    }

    /// Perform an HTTP request via the shell's built-in `http` capability. When it
    /// completes, `then(response)` produces the typed event delivered back to
    /// `update` — `response.output` is the body, `response.ok` is success (2xx).
    /// Rides the request/response plugin mechanism, so it resolves asynchronously.
    pub fn http(
        &mut self,
        method: impl Into<String>,
        url: impl Into<String>,
        body: Option<String>,
        then: impl FnOnce(PluginResponse) -> E + Send + 'static,
    ) {
        #[derive(Serialize)]
        struct HttpReq {
            url: String,
            body: Option<String>,
        }
        let input = serde_json::to_string(&HttpReq { url: url.into(), body })
            .expect("serialize http request");
        self.plugin("http", method, input, then);
    }

    /// `GET url`, delivering the response to `then`.
    pub fn get(&mut self, url: impl Into<String>, then: impl FnOnce(PluginResponse) -> E + Send + 'static) {
        self.http("GET", url, None, then);
    }
    /// `POST url` with a JSON `body`, delivering the response to `then`.
    pub fn post(&mut self, url: impl Into<String>, body: impl Into<String>, then: impl FnOnce(PluginResponse) -> E + Send + 'static) {
        self.http("POST", url, Some(body.into()), then);
    }
    /// `PATCH url` with a JSON `body`, delivering the response to `then`.
    pub fn patch(&mut self, url: impl Into<String>, body: impl Into<String>, then: impl FnOnce(PluginResponse) -> E + Send + 'static) {
        self.http("PATCH", url, Some(body.into()), then);
    }
    /// `DELETE url`, delivering the response to `then`.
    pub fn delete(&mut self, url: impl Into<String>, then: impl FnOnce(PluginResponse) -> E + Send + 'static) {
        self.http("DELETE", url, None, then);
    }
}

// ============================ the app trait ============================

/// What a Mobiler app implements. Write typed domain events; Mobiler serializes
/// them into opaque tokens behind the scenes.
pub trait MobilerApp: Default {
    type Event: Serialize + DeserializeOwned + Send + 'static;
    type Model: Default;

    fn update(&self, event: Self::Event, model: &mut Self::Model, cx: &mut Cx<Self::Event>);

    fn input(&self, id: &str, value: InputValue, model: &mut Self::Model, cx: &mut Cx<Self::Event>) {
        let _ = (id, value, model, cx);
    }

    /// Restore persisted state on startup. `data` is whatever you last passed to
    /// `cx.save` (or empty if nothing was saved). Default: ignore.
    fn restore(&self, data: &str, model: &mut Self::Model) {
        let _ = (data, model);
    }

    /// Run once on startup, after [`restore`](Self::restore). The place to kick
    /// off initial effects — e.g. fetch data with `cx.get`. Default: nothing.
    fn init(&self, model: &mut Self::Model, cx: &mut Cx<Self::Event>) {
        let _ = (model, cx);
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
            Action::Input { id, value } => app.input(&id, value, model, &mut cx),
            Action::Restore { data } => app.restore(&data, model),
            Action::Start => app.init(model, &mut cx),
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

// ============================ navigation ============================

/// A navigation stack the app holds in its `Model`. The **core owns the stack**
/// (single source of truth); the framework reads its `route`/`depth` to drive
/// the shell's push/pop transitions and back button.
///
/// `R` is your screen-route type (typically a small enum). Hold it in the model,
/// mutate it in `update` (`push`/`pop`/`reset`), match `current()` in `view`, and
/// build the shell with [`nav_scaffold`]. Wire a `Msg::Back` (or similar) event to
/// `pop` so the back affordance works.
///
/// ```ignore
/// #[derive(Clone, Serialize)] enum Route { List, Detail(u32) }
/// // model.nav: Nav<Route> = Nav::new(Route::List);
/// // update: Msg::Open(id) => model.nav.push(Route::Detail(id)),
/// //         Msg::Back      => model.nav.pop(),
/// // view:   nav_scaffold(title, dark, tabs, body, &model.nav, Msg::Back)
/// ```
#[derive(Clone, Debug)]
pub struct Nav<R> {
    stack: Vec<R>,
}

impl<R: Clone + Serialize> Nav<R> {
    /// A stack containing a single root route.
    #[must_use]
    pub fn new(root: R) -> Self {
        Self { stack: vec![root] }
    }
    /// Push a new screen onto the stack.
    pub fn push(&mut self, route: R) {
        self.stack.push(route);
    }
    /// Pop the top screen (no-op at the root).
    pub fn pop(&mut self) {
        if self.stack.len() > 1 {
            self.stack.pop();
        }
    }
    /// Replace the whole stack with a fresh root (e.g. switching bottom-nav tabs).
    pub fn reset(&mut self, root: R) {
        self.stack = vec![root];
    }
    /// The current (top) route — what `view` should render.
    #[must_use]
    pub fn current(&self) -> &R {
        self.stack.last().expect("nav stack is never empty")
    }
    /// Stack depth (root = 1).
    #[must_use]
    pub fn depth(&self) -> u32 {
        self.stack.len() as u32
    }
    /// Whether there is a screen to pop back to.
    #[must_use]
    pub fn can_go_back(&self) -> bool {
        self.stack.len() > 1
    }
    /// Stable identity of the current route (its serialization), used by the shell
    /// to decide when to animate a transition.
    fn route_key(&self) -> String {
        serde_json::to_string(self.current()).expect("serialize route")
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
/// A small colored identity dot.
#[must_use]
pub fn color_dot(color: ProjectColor) -> Widget {
    Widget::ColorDot { color }
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
    Widget::Card { child: Box::new(child), style, on_press: None }
}
/// A tappable card carrying a typed press event.
#[must_use]
pub fn card_button<E: Serialize>(child: Widget, style: CardStyle, on_press: E) -> Widget {
    Widget::Card { child: Box::new(child), style, on_press: Some(tok(on_press)) }
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
pub fn toggle(id: impl Into<String>, label: impl Into<String>, value: bool) -> Widget {
    Widget::Toggle { id: id.into(), label: label.into(), value }
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

/// A bottom-nav tab carrying a typed selection event.
#[must_use]
pub fn tab<E: Serialize>(label: impl Into<String>, selected: bool, on_select: E) -> Tab {
    Tab { label: label.into(), selected, on_select: tok(on_select) }
}

/// App shell: top bar + bottom-nav `tabs` + scrollable `body`. `dark_mode` is
/// theme-as-data (the shell themes the whole app from it).
#[must_use]
pub fn scaffold(title: impl Into<String>, dark_mode: bool, tabs: Vec<Tab>, body: Widget) -> Widget {
    let title = title.into();
    // route defaults to the title; root depth = 1.
    Widget::Scaffold { route: title.clone(), title, body: Box::new(body), tabs, back: None, dark_mode, depth: 1 }
}

/// Like [`scaffold`], but the top bar (and the system back button) navigate back
/// via `back` — e.g. a detail screen pushed over a tab (treated as depth 2).
/// For multi-level stacks, drive navigation with [`Nav`] + [`nav_scaffold`].
#[must_use]
pub fn scaffold_back<E: Serialize>(title: impl Into<String>, dark_mode: bool, tabs: Vec<Tab>, body: Widget, back: E) -> Widget {
    let title = title.into();
    Widget::Scaffold { route: title.clone(), title, body: Box::new(body), tabs, back: Some(tok(back)), dark_mode, depth: 2 }
}

/// Scaffold driven by a [`Nav`] stack: fills `route` (from the current route's
/// serialization) and `depth` (stack depth) so the shell animates transitions,
/// and shows a back affordance (top-bar arrow + system back button) firing
/// `on_back` whenever the stack can pop.
#[must_use]
pub fn nav_scaffold<R, E>(
    title: impl Into<String>,
    dark_mode: bool,
    tabs: Vec<Tab>,
    body: Widget,
    nav: &Nav<R>,
    on_back: E,
) -> Widget
where
    R: Clone + Serialize,
    E: Serialize,
{
    Widget::Scaffold {
        title: title.into(),
        body: Box::new(body),
        tabs,
        back: if nav.can_go_back() { Some(tok(on_back)) } else { None },
        dark_mode,
        route: nav.route_key(),
        depth: nav.depth(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::Serialize;

    #[derive(Clone, Copy, Serialize, PartialEq, Debug)]
    enum Route {
        Home,
        Detail(u32),
    }

    #[derive(Serialize)]
    enum Ev {
        Tap,
        Open(u32),
    }

    // ---- Nav ----

    #[test]
    fn nav_push_pop_depth() {
        let mut nav = Nav::new(Route::Home);
        assert_eq!(nav.depth(), 1);
        assert!(!nav.can_go_back());

        nav.push(Route::Detail(7));
        assert_eq!(nav.depth(), 2);
        assert!(nav.can_go_back());
        assert!(matches!(nav.current(), Route::Detail(7)));

        nav.pop();
        assert_eq!(nav.depth(), 1);
        assert!(matches!(nav.current(), Route::Home));

        nav.pop(); // no-op at the root
        assert_eq!(nav.depth(), 1);
    }

    #[test]
    fn nav_reset_replaces_stack() {
        let mut nav = Nav::new(Route::Home);
        nav.push(Route::Detail(1));
        nav.push(Route::Detail(2));
        nav.reset(Route::Detail(9));
        assert_eq!(nav.depth(), 1);
        assert!(matches!(nav.current(), Route::Detail(9)));
    }

    #[test]
    fn nav_route_key_is_serialization() {
        let nav = Nav::new(Route::Detail(3));
        assert_eq!(nav.route_key(), serde_json::to_string(&Route::Detail(3)).unwrap());
    }

    // ---- builders ----

    #[test]
    fn scaffold_sets_route_depth_and_no_back() {
        match scaffold("Home", false, vec![], text("x")) {
            Widget::Scaffold { route, depth, back, dark_mode, .. } => {
                assert_eq!(route, "Home");
                assert_eq!(depth, 1);
                assert!(back.is_none());
                assert!(!dark_mode);
            }
            other => panic!("expected Scaffold, got {other:?}"),
        }
    }

    #[test]
    fn scaffold_back_is_depth_2_with_back() {
        match scaffold_back("Detail", true, vec![], text("x"), Ev::Tap) {
            Widget::Scaffold { depth, back, dark_mode, .. } => {
                assert_eq!(depth, 2);
                assert_eq!(back, Some(serde_json::to_string(&Ev::Tap).unwrap()));
                assert!(dark_mode);
            }
            other => panic!("expected Scaffold, got {other:?}"),
        }
    }

    #[test]
    fn nav_scaffold_shows_back_only_when_poppable() {
        let mut nav = Nav::new(Route::Home);
        // at the root: no back, depth 1, route = serialized current route
        match nav_scaffold("T", false, vec![], text("x"), &nav, Ev::Tap) {
            Widget::Scaffold { back, depth, route, .. } => {
                assert!(back.is_none());
                assert_eq!(depth, 1);
                assert_eq!(route, serde_json::to_string(&Route::Home).unwrap());
            }
            other => panic!("expected Scaffold, got {other:?}"),
        }
        // after a push: back present, depth 2
        nav.push(Route::Detail(2));
        match nav_scaffold("T", false, vec![], text("x"), &nav, Ev::Tap) {
            Widget::Scaffold { back, depth, .. } => {
                assert_eq!(back, Some(serde_json::to_string(&Ev::Tap).unwrap()));
                assert_eq!(depth, 2);
            }
            other => panic!("expected Scaffold, got {other:?}"),
        }
    }

    #[test]
    fn buttons_carry_serialized_event_tokens() {
        match button("Go", ButtonStyle::Filled, Ev::Open(5)) {
            Widget::Button { label, on_press, .. } => {
                assert_eq!(label, "Go");
                assert_eq!(on_press, serde_json::to_string(&Ev::Open(5)).unwrap());
            }
            other => panic!("expected Button, got {other:?}"),
        }
        match card_button(text("c"), CardStyle::Elevated, Ev::Tap) {
            Widget::Card { on_press, .. } => {
                assert_eq!(on_press, Some(serde_json::to_string(&Ev::Tap).unwrap()));
            }
            other => panic!("expected Card, got {other:?}"),
        }
        // a plain card is not tappable
        match card(text("c"), CardStyle::Elevated) {
            Widget::Card { on_press, .. } => assert!(on_press.is_none()),
            other => panic!("expected Card, got {other:?}"),
        }
    }

    // ---- Cx capabilities ----

    #[test]
    fn cx_notify_and_save_enqueue_notifications() {
        let mut cx = Cx::<Ev>::default();
        cx.notify("toast", "show", "hi");
        cx.save("blob");
        assert_eq!(cx.notifications.len(), 2);
        assert_eq!(cx.notifications[0], PluginNotify { plugin: "toast".into(), op: "show".into(), input: "hi".into() });
        assert_eq!(cx.notifications[1], PluginNotify { plugin: "storage".into(), op: "save".into(), input: "blob".into() });
        assert!(cx.requests.is_empty());
    }

    #[test]
    fn cx_http_helpers_build_requests() {
        let mut cx = Cx::<Ev>::default();
        cx.get("http://h/x", |_| Ev::Tap);
        cx.post("http://h/y", "hello", |_| Ev::Tap);
        cx.patch("http://h/z", "patch", |_| Ev::Tap);
        cx.delete("http://h/d", |_| Ev::Tap);

        let methods: Vec<&str> = cx.requests.iter().map(|(c, _)| c.op.as_str()).collect();
        assert_eq!(methods, ["GET", "POST", "PATCH", "DELETE"]);
        assert!(cx.requests.iter().all(|(c, _)| c.plugin == "http"));

        let get_input: serde_json::Value = serde_json::from_str(&cx.requests[0].0.input).unwrap();
        assert_eq!(get_input["url"], "http://h/x");
        assert!(get_input["body"].is_null());

        let post_input: serde_json::Value = serde_json::from_str(&cx.requests[1].0.input).unwrap();
        assert_eq!(post_input["url"], "http://h/y");
        assert_eq!(post_input["body"], "hello");
    }
}
