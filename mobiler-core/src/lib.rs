//! Mobiler runtime — the developer-facing API.
//!
//! Implement [`MobilerApp`] with your **typed** events, model, and view (built
//! from the [builders](#functions)). Mobiler wraps it in [`MobilerShell`], a
//! Crux app speaking the fixed UI ABI ([`mobiler_ui`]); you never touch the wire
//! protocol. Device APIs are capabilities via [`Cx`].

use std::marker::PhantomData;

pub mod bunny;
pub mod format;
pub mod http;
pub mod i18n;
pub use format::{Currency, Locale};
pub use http::{HttpHeader, HttpOutcome};
pub use i18n::{Catalog, negotiate};

use crux_core::{
    App, Command,
    capability::Operation,
    macros::effect,
    render::{RenderOperation, render},
};
use facet::Facet;
use serde::{Deserialize, Serialize, de::DeserializeOwned};

pub use mobiler_ui::{
    A11yRole, Action, BoxAlign, ButtonStyle, Caption, CardStyle, ChartBracket, ChartLegendItem, ChartRefLine, ChartRegion,
    ChartSeries, ChartStyle, ChartTick, Corner, Density, Fab, FieldKind, FontFamily, Icon,
    ImageRatio, ImageShape, InputValue, MapMarker, ProjectColor, Rgb, Segment, Sheet, Spacing, SwipeButton, Tab,
    TextStyle, Theme, Tone, Widget,
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
    /// Long-lived subscription: the shell starts a native source and resolves
    /// **repeatedly** (a [`PluginResponse`] per event) until it's torn down. Powers
    /// [`Cx::subscribe`]. Stop it with [`Cx::unsubscribe`] (a `stream`/`unsubscribe`
    /// notify keyed by [`PluginStreamCall::key`]).
    PluginStream(PluginStreamCall),
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

/// A streaming plugin subscription (powers [`Effect::PluginStream`]). Like
/// [`PluginCall`] but carries a caller-chosen `key` so the subscription can be torn
/// down ([`Cx::unsubscribe`]) — the shell registers the native source under `key`.
#[derive(Facet, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct PluginStreamCall {
    pub key: String,
    pub plugin: String,
    pub op: String,
    pub input: String,
}
impl Operation for PluginStreamCall {
    type Output = PluginResponse;
}

/// A plugin's reply. `output` is raw bytes: the HTTP capability puts a bincode
/// [`HttpOutcome`](crate::HttpOutcome) here, while most plugins put UTF-8 text (use
/// [`PluginResponse::text`] to build one and [`as_text`](Self::as_text) to read it).
///
/// Note the asymmetry with [`PluginCall`], whose `input` stays a `String`: changing
/// `output` affects only where a response is *constructed*, whereas changing `input`
/// would affect where it is *parsed* — in every plugin on every shell. Large uploads
/// pass file paths (text), so `input` stays adequate.
#[derive(Facet, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct PluginResponse {
    pub ok: bool,
    pub output: Vec<u8>,
}

impl PluginResponse {
    /// Build a response whose payload is UTF-8 text — what most plugins return.
    pub fn text(ok: bool, s: impl Into<String>) -> Self {
        Self { ok, output: s.into().into_bytes() }
    }

    /// The payload as text, or `None` if it is not valid UTF-8.
    pub fn as_text(&self) -> Option<&str> {
        std::str::from_utf8(&self.output).ok()
    }
}

type Continuation<E> = Box<dyn FnOnce(PluginResponse) -> E + Send>;
/// A streaming continuation — fires once **per event** (so `Fn`, not `FnOnce`).
type StreamContinuation<E> = Box<dyn Fn(PluginResponse) -> E + Send>;

/// Effects an app requests during `update`, generic over the app event type so
/// continuations stay fully typed.
pub struct Cx<E> {
    notifications: Vec<PluginNotify>,
    requests: Vec<(PluginCall, Continuation<E>)>,
    streams: Vec<(PluginStreamCall, StreamContinuation<E>)>,
}

impl<E> Default for Cx<E> {
    fn default() -> Self {
        Self { notifications: Vec::new(), requests: Vec::new(), streams: Vec::new() }
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

    /// Subscribe to a streaming plugin: the shell starts a native source and delivers
    /// **every** event it produces to `on_event` (which fires repeatedly, once per
    /// event), each producing a typed event into your `update`. `key` is a
    /// caller-chosen id for this subscription — pass the same `key` to
    /// [`unsubscribe`](Self::unsubscribe) to stop it. Call `subscribe` **once** per
    /// key (e.g. in [`init`](MobilerApp::init) or on a connect event); calling it
    /// again with a live key starts a second source.
    pub fn subscribe(
        &mut self,
        key: impl Into<String>,
        plugin: impl Into<String>,
        op: impl Into<String>,
        input: impl Into<String>,
        on_event: impl Fn(PluginResponse) -> E + Send + 'static,
    ) {
        self.streams.push((
            PluginStreamCall { key: key.into(), plugin: plugin.into(), op: op.into(), input: input.into() },
            Box::new(on_event),
        ));
    }

    /// Stop the streaming subscription started under `key` by [`subscribe`](Self::subscribe).
    /// The shell tears down the native source registered under `key`, so it stops
    /// producing events. No-op if `key` isn't subscribed.
    pub fn unsubscribe(&mut self, key: impl Into<String>) {
        self.notify("stream", "unsubscribe", key);
    }

    /// Persist `data` (handed back to [`MobilerApp::restore`] on next startup).
    pub fn save(&mut self, data: impl Into<String>) {
        self.notify("storage", "save", data);
    }

    /// Copy `text` to the system clipboard (built-in `clipboard` capability).
    pub fn copy(&mut self, text: impl Into<String>) {
        self.notify("clipboard", "copy", text);
    }

    /// Open the system share sheet with `text` (built-in `share` capability).
    pub fn share(&mut self, text: impl Into<String>) {
        self.notify("share", "text", text);
    }

    /// Open `url` in the platform browser / default handler (built-in `browser`
    /// capability). Fire-and-forget: the app leaves the foreground.
    pub fn open_url(&mut self, url: impl Into<String>) {
        self.notify("browser", "open", url);
    }

    /// Show a transient toast / snackbar with `text` (built-in `toast` capability).
    pub fn toast(&mut self, text: impl Into<String>) {
        self.notify("toast", "show", text);
    }

    /// Fire a haptic tap (built-in `haptics` capability). `style` is `"light"`,
    /// `"medium"`, or `"heavy"`; unknown styles fall back to medium.
    pub fn haptic(&mut self, style: impl Into<String>) {
        self.notify("haptics", style, "");
    }

    /// Start an HTTP request with full control — headers, and later timeouts and
    /// query params — finished with [`RequestBuilder::send`].
    ///
    /// ```ignore
    /// cx.request("PUT", url)
    ///     .bearer(&token)
    ///     .body(json)
    ///     .send(|outcome| match outcome.status() {
    ///         Some(409) => Event::NeedsRebase,
    ///         Some(s) if outcome.is_success() => Event::Saved,
    ///         Some(s) => Event::ServerError(s),
    ///         None => Event::Offline,
    ///     });
    /// ```
    pub fn request(
        &mut self,
        method: impl Into<String>,
        url: impl Into<String>,
    ) -> crate::http::RequestBuilder<'_, E> {
        crate::http::RequestBuilder::new(self, method.into(), url.into())
    }

    /// `GET url`, delivering the outcome to `then`.
    pub fn get(&mut self, url: impl Into<String>, then: impl FnOnce(HttpOutcome) -> E + Send + 'static) {
        self.request("GET", url).send(then);
    }
    /// `POST url` with `body`, delivering the outcome to `then`.
    pub fn post(&mut self, url: impl Into<String>, body: impl Into<String>, then: impl FnOnce(HttpOutcome) -> E + Send + 'static) {
        self.request("POST", url).body(body).send(then);
    }
    /// `PUT url` with `body`, delivering the outcome to `then`.
    pub fn put(&mut self, url: impl Into<String>, body: impl Into<String>, then: impl FnOnce(HttpOutcome) -> E + Send + 'static) {
        self.request("PUT", url).body(body).send(then);
    }
    /// `PATCH url` with `body`, delivering the outcome to `then`.
    pub fn patch(&mut self, url: impl Into<String>, body: impl Into<String>, then: impl FnOnce(HttpOutcome) -> E + Send + 'static) {
        self.request("PATCH", url).body(body).send(then);
    }
    /// `DELETE url`, delivering the outcome to `then`.
    pub fn delete(&mut self, url: impl Into<String>, then: impl FnOnce(HttpOutcome) -> E + Send + 'static) {
        self.request("DELETE", url).send(then);
    }

    /// Query the device model/name via the built-in `device` capability; the result
    /// (`response.output`, e.g. "Google Pixel 7" / "Apple iPhone (iOS 18.0)") is
    /// delivered to `then`.
    pub fn device_model(&mut self, then: impl FnOnce(PluginResponse) -> E + Send + 'static) {
        self.plugin("device", "model", "", then);
    }

    /// Query the device's preferred locale as a BCP-47 language tag (e.g. `"de-CH"`, `"en-US"`)
    /// via the built-in `device` capability; `then` receives it in `response.output`. Pair with
    /// [`Locale::from_tag`](crate::format::Locale::from_tag) to choose the app's language /
    /// formatting locale at startup. Works on iOS, Android, and web.
    pub fn device_locale(&mut self, then: impl FnOnce(PluginResponse) -> E + Send + 'static) {
        self.plugin("device", "locale", "", then);
    }

    /// Let the user pick an image (built-in `photo` capability — the system photo
    /// picker, no permission required). `then` receives the result: on success
    /// `response.ok` is `true` and `response.output` is a local image URI you can
    /// hand straight to the `image(...)` widget; on cancel, `ok` is `false`.
    pub fn pick_photo(&mut self, then: impl FnOnce(PluginResponse) -> E + Send + 'static) {
        self.plugin("photo", "pick", "", then);
    }

    /// Capture a photo with the device camera (built-in `camera` capability — launches
    /// the system camera). `then` receives the result: on success `response.ok` is
    /// `true` and `response.output` is a local image URI you can hand straight to the
    /// `image(...)` widget; on cancel, `ok` is `false`. iOS requires an
    /// `NSCameraUsageDescription` (the template ships one, opt-in); Android captures via
    /// the system camera app, so no extra runtime permission is needed.
    pub fn capture_photo(&mut self, then: impl FnOnce(PluginResponse) -> E + Send + 'static) {
        self.plugin("camera", "capture", "", then);
    }

    /// Ask the user to confirm via a native dialog (built-in `dialog` capability).
    /// `then` receives the choice: `response.ok` is `true` if confirmed, `false` if
    /// cancelled/dismissed. Resolves asynchronously (the user replies whenever).
    pub fn confirm(
        &mut self,
        title: impl Into<String>,
        message: impl Into<String>,
        then: impl FnOnce(PluginResponse) -> E + Send + 'static,
    ) {
        #[derive(Serialize)]
        struct Confirm {
            title: String,
            message: String,
        }
        let input = serde_json::to_string(&Confirm { title: title.into(), message: message.into() })
            .expect("serialize confirm");
        self.plugin("dialog", "confirm", input, then);
    }

    /// Let the user pick a date via the native date picker (built-in `datetime`
    /// capability). On success `response.ok` is `true` and `response.output` is the
    /// chosen date as an ISO `YYYY-MM-DD` string; on cancel/dismiss, `ok` is `false`.
    /// Resolves asynchronously (the user replies whenever).
    pub fn pick_date(&mut self, then: impl FnOnce(PluginResponse) -> E + Send + 'static) {
        self.plugin("datetime", "date", "", then);
    }

    /// Let the user pick a time via the native time picker (built-in `datetime`
    /// capability). On success `response.ok` is `true` and `response.output` is the
    /// chosen time as a 24-hour `HH:MM` string; on cancel/dismiss, `ok` is `false`.
    /// Resolves asynchronously (the user replies whenever).
    pub fn pick_time(&mut self, then: impl FnOnce(PluginResponse) -> E + Send + 'static) {
        self.plugin("datetime", "time", "", then);
    }

    /// Read the current local date-time (built-in `datetime` capability). The core is a
    /// pure state machine and can't read the clock itself, so stamping an event with "now"
    /// — a ledger entry, a log line — goes through the shell. `response.output` is the local
    /// date-time as `YYYY-MM-DD HH:MM:SS` (sortable lexicographically; the date is the first
    /// 10 chars). No UI — resolves immediately. Pair with [`pick_date`](Self::pick_date) when
    /// the user should choose a different date instead.
    pub fn now(&mut self, then: impl FnOnce(PluginResponse) -> E + Send + 'static) {
        self.plugin("datetime", "now", "", then);
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
        for (op, then) in cx.streams {
            // A long-lived shell stream: `then_send` fires `then` once per emitted
            // event (it's `Fn`), each re-entering `update` as a `Fired` action.
            commands.push(Command::stream_from_shell(op).then_send(move |response: PluginResponse| {
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
/// A progress bar (`Some(0.0..=1.0)`) or an indeterminate spinner (`None`).
#[must_use]
pub fn progress(value: Option<f32>) -> Widget { Widget::Progress { value } }
/// A shimmer placeholder shown while content loads.
#[must_use]
pub fn skeleton() -> Widget { Widget::Skeleton }
/// An in-app PDF viewer for the document at `url` (remote https URL or local file URI) — rendered
/// natively per platform (PDFKit / `PdfRenderer` / `<iframe>`). The app just supplies the URL, e.g.
/// a backend-generated report. Give it room (place in a sized container or a scroller).
#[must_use]
pub fn pdf_view(url: impl Into<String>) -> Widget { Widget::PdfView { url: url.into() } }
/// A controllable native video player for `url` (remote MP4/HLS or a local file URI), rendered with the
/// native player per platform (AVPlayer / Media3 ExoPlayer / `<video>`). `id` routes the ~once-per-second
/// position into `input(id, InputValue::Int(position_ms))`; build it fresh each render with the current
/// `playing` (play/pause) + `seek_to_ms` (the shell jumps when this CHANGES; `-1` = no seek). `on_ended`
/// fires when the clip finishes. Defaults: controls shown, not looping/muted, no poster, no resume
/// offset, no captions, rate 1.0, full volume, single clip (no playlist), PiP off — tune with
/// [`with_loop`]/[`with_muted`]/[`without_controls`]/[`with_poster`]/[`with_start_at`]/[`with_captions`]/
/// [`with_rate`]/[`with_volume`]/[`with_pip`] (or [`video_playlist`] for a queue). Give it room.
#[must_use]
pub fn video_player<E: Serialize>(id: impl Into<String>, url: impl Into<String>, playing: bool, seek_to_ms: i64, on_ended: E) -> Widget {
    Widget::Video {
        url: url.into(),
        id: id.into(),
        playing,
        seek_to_ms,
        controls: true,
        looping: false,
        muted: false,
        on_ended: Some(tok(on_ended)),
        poster: None,
        start_at_ms: -1,
        captions: Vec::new(),
        rate: 1.0,
        volume: 1.0,
        urls: Vec::new(),
        start_index: 0,
        seek_index: -1,
        allow_pip: false,
    }
}
/// A controllable native video player over a **playlist** of `urls` (auto-advances gaplessly; the
/// shell reports the current track via `input("{id}.index", InputValue::Int(i))`). `start_index` is
/// the first clip; build it fresh each render with the current `playing`. Force-jump to a track by
/// pairing this with [`with_seek_index`]. `on_ended` fires when the LAST clip finishes. Same cosmetic
/// modifiers as [`video_player`]. Empty `urls` renders nothing useful — use [`video_player`] for one clip.
#[must_use]
pub fn video_playlist<E: Serialize>(id: impl Into<String>, urls: Vec<String>, start_index: i64, playing: bool, on_ended: E) -> Widget {
    Widget::Video {
        url: urls.first().cloned().unwrap_or_default(),
        id: id.into(),
        playing,
        seek_to_ms: -1,
        controls: true,
        looping: false,
        muted: false,
        on_ended: Some(tok(on_ended)),
        poster: None,
        start_at_ms: -1,
        captions: Vec::new(),
        rate: 1.0,
        volume: 1.0,
        urls,
        start_index,
        seek_index: -1,
        allow_pip: false,
    }
}
/// Apply a mutation to a [`Widget::Video`]'s fields, passing other widgets through unchanged. Keeps
/// the `with_*` video modifiers from each having to spell out all of `Video`'s fields.
fn map_video(widget: Widget, f: impl FnOnce(&mut VideoFields)) -> Widget {
    match widget {
        Widget::Video { url, id, playing, seek_to_ms, controls, looping, muted, on_ended,
            poster, start_at_ms, captions, rate, volume, urls, start_index, seek_index, allow_pip } => {
            let mut v = VideoFields { url, id, playing, seek_to_ms, controls, looping, muted, on_ended,
                poster, start_at_ms, captions, rate, volume, urls, start_index, seek_index, allow_pip };
            f(&mut v);
            Widget::Video { url: v.url, id: v.id, playing: v.playing, seek_to_ms: v.seek_to_ms,
                controls: v.controls, looping: v.looping, muted: v.muted, on_ended: v.on_ended,
                poster: v.poster, start_at_ms: v.start_at_ms, captions: v.captions, rate: v.rate,
                volume: v.volume, urls: v.urls, start_index: v.start_index, seek_index: v.seek_index,
                allow_pip: v.allow_pip }
        }
        other => other,
    }
}
struct VideoFields {
    url: String, id: String, playing: bool, seek_to_ms: i64, controls: bool, looping: bool,
    muted: bool, on_ended: Option<String>, poster: Option<String>, start_at_ms: i64,
    captions: Vec<Caption>, rate: f32, volume: f32, urls: Vec<String>, start_index: i64,
    seek_index: i64, allow_pip: bool,
}
/// Loop a [`video_player`] (restart on end). No-op on non-Video widgets.
#[must_use]
pub fn with_loop(widget: Widget) -> Widget { map_video(widget, |v| v.looping = true) }
/// Start a [`video_player`] muted (needed for reliable autoplay). No-op on non-Video widgets.
#[must_use]
pub fn with_muted(widget: Widget) -> Widget { map_video(widget, |v| v.muted = true) }
/// Hide the native transport controls on a [`video_player`] (the app drives it). No-op otherwise.
#[must_use]
pub fn without_controls(widget: Widget) -> Widget { map_video(widget, |v| v.controls = false) }
/// Show `poster` (an image URL) before the first play / while idle. No-op on non-Video widgets.
#[must_use]
pub fn with_poster(widget: Widget, poster: impl Into<String>) -> Widget {
    let poster = poster.into();
    map_video(widget, move |v| v.poster = Some(poster))
}
/// Resume a [`video_player`] at `start_at_ms` (applied once on load). No-op on non-Video widgets.
#[must_use]
pub fn with_start_at(widget: Widget, start_at_ms: i64) -> Widget {
    map_video(widget, move |v| v.start_at_ms = start_at_ms)
}
/// Attach subtitle/caption tracks to a [`video_player`] (see [`Caption`]). No-op on non-Video widgets.
#[must_use]
pub fn with_captions(widget: Widget, captions: Vec<Caption>) -> Widget {
    map_video(widget, move |v| v.captions = captions)
}
/// Set playback speed (`1.0` = normal) on a [`video_player`]. No-op on non-Video widgets.
#[must_use]
pub fn with_rate(widget: Widget, rate: f32) -> Widget { map_video(widget, move |v| v.rate = rate) }
/// Set the volume (`0.0`–`1.0`) on a [`video_player`]. No-op on non-Video widgets.
#[must_use]
pub fn with_volume(widget: Widget, volume: f32) -> Widget {
    map_video(widget, move |v| v.volume = volume.clamp(0.0, 1.0))
}
/// Force a playlist [`video_playlist`] to jump to track `index` when this CHANGES. No-op otherwise.
#[must_use]
pub fn with_seek_index(widget: Widget, index: i64) -> Widget {
    map_video(widget, move |v| v.seek_index = index)
}
/// Enable Picture-in-Picture on a [`video_player`] (the shell adds a PiP affordance). No-op otherwise.
#[must_use]
pub fn with_pip(widget: Widget) -> Widget { map_video(widget, |v| v.allow_pip = true) }
/// A native web view showing the page / embedded player at `url` (`WKWebView` / Android `WebView` /
/// `<iframe>`). General-purpose: docs, dashboards, or a hosted player embed (e.g. a Bunny.net /
/// YouTube embed URL). NOT the default video player — use [`video_player`] for that. Give it room
/// (a sized container or a card).
#[must_use]
pub fn web_view(url: impl Into<String>) -> Widget { Widget::WebView { url: url.into() } }

/// An interactive map centered at (`center_lat`, `center_lng`) with the given `zoom` (≈ MapLibre/Google
/// zoom levels: ~2 world, ~14 city, ~17 street). iOS MapKit / Android MapLibre / web MapLibre-GL — no
/// API key. Add pins with [`with_markers`], a vector style with [`with_map_style`]. Taps arrive in
/// [`MobilerApp::input`] as `Input { id: "{id}.tap", Text("lat,lng") }` / `{ "{id}.marker", Text(id) }`.
/// Give it a height (a sized container or card).
#[must_use]
pub fn map(id: impl Into<String>, center_lat: f64, center_lng: f64, zoom: f64) -> Widget {
    Widget::Map {
        id: id.into(),
        center_lat,
        center_lng,
        zoom,
        markers: Vec::new(),
        style_url: None,
        interactive: true,
    }
}
/// Add/replace the pins on a [`map`] (no-op on any other widget).
#[must_use]
pub fn with_markers(widget: Widget, markers: Vec<MapMarker>) -> Widget {
    match widget {
        Widget::Map { id, center_lat, center_lng, zoom, style_url, interactive, .. } =>
            Widget::Map { id, center_lat, center_lng, zoom, markers, style_url, interactive },
        other => other,
    }
}
/// Set the MapLibre vector-style URL (Android + web; iOS MapKit ignores it). None → a free default.
#[must_use]
pub fn with_map_style(widget: Widget, url: impl Into<String>) -> Widget {
    match widget {
        Widget::Map { id, center_lat, center_lng, zoom, markers, interactive, .. } =>
            Widget::Map { id, center_lat, center_lng, zoom, markers, style_url: Some(url.into()), interactive },
        other => other,
    }
}
/// A map pin at (`lat`, `lng`); `id` is echoed on tap. Add a title with [`marker_titled`].
#[must_use]
pub fn marker(id: impl Into<String>, lat: f64, lng: f64) -> MapMarker {
    MapMarker { id: id.into(), lat, lng, title: None }
}
/// A titled map pin (the title shows in the marker's callout/popup).
#[must_use]
pub fn marker_titled(id: impl Into<String>, lat: f64, lng: f64, title: impl Into<String>) -> MapMarker {
    MapMarker { id: id.into(), lat, lng, title: Some(title.into()) }
}
/// A single unnamed series wrapping `values` — the back-compat shape for `bar_chart`/`line_chart`.
fn one_series(values: Vec<f32>) -> Vec<ChartSeries> {
    vec![ChartSeries { name: String::new(), values, color: None, goal: None }]
}

/// A bar chart of `values` (normalized to the max), with optional per-value `labels`.
/// Single-series, no axis or legend — for richer charts use [`chart`].
#[must_use]
pub fn bar_chart(values: Vec<f32>, labels: Vec<String>) -> Widget {
    Widget::Chart { series: one_series(values), labels, style: ChartStyle::Bar, axis: false, legend: false }
}
/// A line chart of `values` (normalized to the max), with optional per-value `labels`.
/// Single-series, no axis or legend — for richer charts use [`chart`].
#[must_use]
pub fn line_chart(values: Vec<f32>, labels: Vec<String>) -> Widget {
    Widget::Chart { series: one_series(values), labels, style: ChartStyle::Line, axis: false, legend: false }
}
/// A multi-series chart in the given `style`, with optional x-axis `labels`, y-`axis` gridlines/
/// ticks (cartesian styles), and a series `legend`. The general builder behind the convenience
/// constructors below.
#[must_use]
pub fn chart(series: Vec<ChartSeries>, labels: Vec<String>, style: ChartStyle, axis: bool, legend: bool) -> Widget {
    Widget::Chart { series, labels, style, axis, legend }
}
/// Bars stacked to a total per x-slot. Axis + legend on by default.
#[must_use]
pub fn stacked_bar_chart(series: Vec<ChartSeries>, labels: Vec<String>) -> Widget {
    chart(series, labels, ChartStyle::StackedBar, true, true)
}
/// Bars where each x-slot fills to 100% — series as proportions. Legend on, no value axis.
#[must_use]
pub fn pct_stacked_bar_chart(series: Vec<ChartSeries>, labels: Vec<String>) -> Widget {
    chart(series, labels, ChartStyle::StackedBar100, false, true)
}
/// A pie chart — each series is one wedge sized by its magnitude. Legend on.
#[must_use]
pub fn pie_chart(series: Vec<ChartSeries>) -> Widget {
    chart(series, vec![], ChartStyle::Pie, false, true)
}
/// A donut chart (pie with a center hole). Legend on.
#[must_use]
pub fn donut_chart(series: Vec<ChartSeries>) -> Widget {
    chart(series, vec![], ChartStyle::Donut, false, true)
}
/// Concentric progress rings — one per series, swept by `sum(values) / goal`. Legend on.
/// Give each series a goal via [`ChartSeries::with_goal`].
#[must_use]
pub fn rings_chart(series: Vec<ChartSeries>) -> Widget {
    chart(series, vec![], ChartStyle::Rings, false, true)
}
/// A single radial gauge — the first series' `value / goal` with the number in the center.
#[must_use]
pub fn gauge_chart(series: ChartSeries) -> Widget {
    chart(vec![series], vec![], ChartStyle::Gauge, false, false)
}

/// A variable-width stacked-region ("coverage-gap" / Marimekko) chart. `regions` are rectangles in
/// the `[0, x_max] × [0, y_max]` plane (build with [`ChartRegion::new`]); `ticks` label the
/// irregular x-axis; `ref_lines` are horizontal target/max lines ([`ChartRefLine::target`]/`::max`);
/// `legend` names the colors. Add a right-side bracket annotation with [`with_bracket`].
#[must_use]
pub fn region_chart(
    regions: Vec<ChartRegion>,
    ticks: Vec<ChartTick>,
    x_max: f32,
    y_max: f32,
    ref_lines: Vec<ChartRefLine>,
    legend: Vec<ChartLegendItem>,
) -> Widget {
    Widget::RegionChart { regions, ticks, x_max, y_max, ref_lines, bracket: None, legend }
}

/// Attach a right-side bracket annotation to a [`region_chart`] (no-op on any other widget).
#[must_use]
pub fn with_bracket(widget: Widget, bracket: ChartBracket) -> Widget {
    match widget {
        Widget::RegionChart { regions, ticks, x_max, y_max, ref_lines, legend, .. } => {
            Widget::RegionChart { regions, ticks, x_max, y_max, ref_lines, bracket: Some(bracket), legend }
        }
        other => other,
    }
}

/// Days in `month` (1–12) of `year`, leap-year aware.
fn days_in_month(year: u32, month: u8) -> u8 {
    match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 => if (year % 4 == 0 && year % 100 != 0) || year % 400 == 0 { 29 } else { 28 },
        _ => 30,
    }
}

/// Weekday of `year-month-day` as 0=Sunday..6=Saturday (Sakamoto's algorithm).
fn weekday(year: u32, month: u8, day: u8) -> u8 {
    const T: [u32; 12] = [0, 3, 2, 5, 0, 3, 5, 1, 4, 6, 2, 4];
    let y = if month < 3 { year - 1 } else { year };
    let m = month as usize - 1;
    ((y + y / 4 - y / 100 + y / 400 + T[m] + u32::from(day)) % 7) as u8
}

/// An inline month calendar for `year`/`month` (1–12). `on_day(d)` builds the tap event for each
/// day `d` in the month; `selected` highlights a day. Leading blanks + weekday header are handled
/// by the shells from the computed `first_weekday`.
#[must_use]
pub fn calendar<E: Serialize>(year: u32, month: u8, selected: Option<u8>, on_day: impl Fn(u8) -> E) -> Widget {
    let n = days_in_month(year, month);
    let on_day = (1..=n).map(|d| tok(on_day(d))).collect();
    Widget::Calendar { year, month, first_weekday: weekday(year, month, 1), selected, on_day }
}

/// A list row that reveals trailing `actions` (label, tone, event) on horizontal swipe; each is
/// tappable. On web the actions render inline (no gesture).
#[must_use]
pub fn swipe_action<S: Into<String>, E: Serialize>(child: Widget, actions: Vec<(S, Tone, E)>) -> Widget {
    Widget::SwipeAction {
        child: Box::new(child),
        actions: actions
            .into_iter()
            .map(|(label, tone, ev)| SwipeButton { label: label.into(), tone, on_tap: tok(ev) })
            .collect(),
    }
}
#[must_use]
pub fn spacer(size: Spacing) -> Widget { Widget::Spacer { size } }

#[must_use]
pub fn row(children: Vec<Widget>) -> Widget { Widget::Row { children } }
#[must_use]
pub fn column(children: Vec<Widget>) -> Widget { Widget::Column { children } }
#[must_use]
pub fn card(child: Widget, style: CardStyle) -> Widget {
    Widget::Card { child: Box::new(child), style, on_press: None, on_long_press: None }
}
/// A tappable card carrying a typed press event.
#[must_use]
pub fn card_button<E: Serialize>(child: Widget, style: CardStyle, on_press: E) -> Widget {
    Widget::Card { child: Box::new(child), style, on_press: Some(tok(on_press)), on_long_press: None }
}
/// Attach a long-press (press-and-hold) event to a `Card`. No-op on any other widget.
/// Combines with `card` / `card_button` — a card can carry both a tap and a long-press.
#[must_use]
pub fn with_long_press<E: Serialize>(widget: Widget, on_long_press: E) -> Widget {
    match widget {
        Widget::Card { child, style, on_press, .. } => Widget::Card {
            child,
            style,
            on_press,
            on_long_press: Some(tok(on_long_press)),
        },
        other => other,
    }
}
/// Z-stack/overlay (the `Box` widget). With `scrim`, the first child is a
/// darkened background and the rest render on top.
#[must_use]
pub fn stack(align: BoxAlign, scrim: bool, children: Vec<Widget>) -> Widget {
    Widget::Box { children, align, scrim }
}
#[must_use]
pub fn grid(children: Vec<Widget>) -> Widget { Widget::Grid { children } }
/// A two-pane master-detail layout ([`Widget::Split`]). Side-by-side on a wide screen (tablet /
/// landscape); one pane on a phone — `primary` until `show_detail` (the app sets it on selection),
/// then `detail` with a back chevron firing `on_back`. On wide, `detail` should show a placeholder
/// until a row is selected.
#[must_use]
pub fn split<E: Serialize>(primary: Widget, detail: Widget, show_detail: bool, on_back: E) -> Widget {
    Widget::Split { primary: Box::new(primary), detail: Box::new(detail), show_detail, on_back: Some(tok(on_back)) }
}
/// Horizontally scrolling row of children (a carousel / chip rail).
#[must_use]
pub fn scroller(children: Vec<Widget>) -> Widget { Widget::Scroller { children } }
/// A circular avatar image.
#[must_use]
pub fn avatar(source: impl Into<String>) -> Widget { Widget::Avatar { source: source.into(), status: None } }
/// A circular avatar image with a colored status dot.
#[must_use]
pub fn avatar_status(source: impl Into<String>, status: Tone) -> Widget {
    Widget::Avatar { source: source.into(), status: Some(status) }
}
/// A read-only star rating. `value` is in tenths (e.g. `48` = 4.8 of `max` stars).
#[must_use]
pub fn rating(value: u32, max: u8) -> Widget { Widget::Rating { value, max, on_rate: None } }
/// A tappable star rating — `on_rate` carries one event per star (star *i* fires `on_rate[i]`).
#[must_use]
pub fn rating_input<E: Serialize>(value: u32, max: u8, on_rate: Vec<E>) -> Widget {
    Widget::Rating { value, max, on_rate: Some(on_rate.into_iter().map(tok).collect()) }
}

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
    Widget::TextField { id: id.into(), placeholder: placeholder.into(), value: value.into(), kind: FieldKind::Text, error: None }
}
/// A text field with full control over [`FieldKind`] and an optional inline
/// validation `error`. The kind-specific helpers below ([`secure_field`],
/// [`email_field`], …) wrap this for the common cases.
#[must_use]
pub fn field(id: impl Into<String>, placeholder: impl Into<String>, value: impl Into<String>, kind: FieldKind, error: Option<String>) -> Widget {
    Widget::TextField { id: id.into(), placeholder: placeholder.into(), value: value.into(), kind, error }
}
/// A masked password field ([`FieldKind::Secure`]).
#[must_use]
pub fn secure_field(id: impl Into<String>, placeholder: impl Into<String>, value: impl Into<String>) -> Widget {
    field(id, placeholder, value, FieldKind::Secure, None)
}
/// An email-keyboard field ([`FieldKind::Email`]).
#[must_use]
pub fn email_field(id: impl Into<String>, placeholder: impl Into<String>, value: impl Into<String>) -> Widget {
    field(id, placeholder, value, FieldKind::Email, None)
}
/// A whole-number keypad field ([`FieldKind::Number`]).
#[must_use]
pub fn number_field(id: impl Into<String>, placeholder: impl Into<String>, value: impl Into<String>) -> Widget {
    field(id, placeholder, value, FieldKind::Number, None)
}
/// A decimal keypad field ([`FieldKind::Decimal`]).
#[must_use]
pub fn decimal_field(id: impl Into<String>, placeholder: impl Into<String>, value: impl Into<String>) -> Widget {
    field(id, placeholder, value, FieldKind::Decimal, None)
}
/// A phone-keypad field ([`FieldKind::Phone`]).
#[must_use]
pub fn phone_field(id: impl Into<String>, placeholder: impl Into<String>, value: impl Into<String>) -> Widget {
    field(id, placeholder, value, FieldKind::Phone, None)
}
/// A URL-keyboard field ([`FieldKind::Url`]).
#[must_use]
pub fn url_field(id: impl Into<String>, placeholder: impl Into<String>, value: impl Into<String>) -> Widget {
    field(id, placeholder, value, FieldKind::Url, None)
}
/// A growable multi-line text area ([`FieldKind::Multiline`]).
#[must_use]
pub fn multiline_field(id: impl Into<String>, placeholder: impl Into<String>, value: impl Into<String>) -> Widget {
    field(id, placeholder, value, FieldKind::Multiline, None)
}
/// Attach an inline validation message to a [`Widget::TextField`], marking it
/// invalid. No-op on any other widget.
#[must_use]
pub fn with_error(widget: Widget, message: impl Into<String>) -> Widget {
    match widget {
        Widget::TextField { id, placeholder, value, kind, .. } =>
            Widget::TextField { id, placeholder, value, kind, error: Some(message.into()) },
        other => other,
    }
}

/// Wrap `child` so a screen reader (VoiceOver / TalkBack) announces the subtree as ONE element named
/// `label` — gives an unlabeled `icon_button`/`image` a name, or groups a card's children into one
/// announced element. Add `with_a11y_hint` / `with_a11y_role` for the activation hint + control type.
#[must_use]
pub fn a11y(child: Widget, label: impl Into<String>) -> Widget {
    Widget::A11y { child: Box::new(child), label: label.into(), hint: None, role: None }
}
/// Set the accessibility activation hint (e.g. "Opens your bookings"); wraps `widget` if it isn't an
/// [`a11y`] wrapper yet.
#[must_use]
pub fn with_a11y_hint(widget: Widget, hint: impl Into<String>) -> Widget {
    match widget {
        Widget::A11y { child, label, role, .. } =>
            Widget::A11y { child, label, hint: Some(hint.into()), role },
        other => Widget::A11y { child: Box::new(other), label: String::new(), hint: Some(hint.into()), role: None },
    }
}
/// Set the accessibility role / control type; wraps `widget` if it isn't an [`a11y`] wrapper yet.
#[must_use]
pub fn with_a11y_role(widget: Widget, role: A11yRole) -> Widget {
    match widget {
        Widget::A11y { child, label, hint, .. } =>
            Widget::A11y { child, label, hint, role: Some(role) },
        other => Widget::A11y { child: Box::new(other), label: String::new(), hint: None, role: Some(role) },
    }
}
/// A search input (leading magnifier, pill); emits `Input { id, Text }` like [`text_field`].
#[must_use]
pub fn search_field(id: impl Into<String>, placeholder: impl Into<String>, value: impl Into<String>) -> Widget {
    Widget::SearchField { id: id.into(), placeholder: placeholder.into(), value: value.into() }
}
/// One option in a [`segmented`] control, carrying a typed selection event.
#[must_use]
pub fn segment<E: Serialize>(label: impl Into<String>, selected: bool, on_select: E) -> Segment {
    Segment { label: label.into(), selected, on_select: tok(on_select) }
}
/// A single-choice segmented control (exclusive options in a pill).
#[must_use]
pub fn segmented(segments: Vec<Segment>) -> Widget {
    Widget::Segmented { segments }
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

/// A bottom-nav tab carrying a typed selection event (label-only).
#[must_use]
pub fn tab<E: Serialize>(label: impl Into<String>, selected: bool, on_select: E) -> Tab {
    Tab { label: label.into(), selected, on_select: tok(on_select), icon: None }
}

/// A bottom-nav tab with a leading icon (icon tab bar).
#[must_use]
pub fn tab_icon<E: Serialize>(label: impl Into<String>, icon: Icon, selected: bool, on_select: E) -> Tab {
    Tab { label: label.into(), selected, on_select: tok(on_select), icon: Some(icon) }
}

/// App shell: top bar + bottom-nav `tabs` + scrollable `body`. `dark_mode` is
/// theme-as-data (the shell themes the whole app from it).
#[must_use]
pub fn scaffold(title: impl Into<String>, dark_mode: bool, tabs: Vec<Tab>, body: Widget) -> Widget {
    let title = title.into();
    // route defaults to the title; root depth = 1.
    Widget::Scaffold { route: title.clone(), title, body: Box::new(body), tabs, back: None, dark_mode, theme: None, fab: None, sheet: None, on_refresh: None, refreshing: false, depth: 1 }
}

/// Like [`scaffold`], but the top bar (and the system back button) navigate back
/// via `back` — e.g. a detail screen pushed over a tab (treated as depth 2).
/// For multi-level stacks, drive navigation with [`Nav`] + [`nav_scaffold`].
#[must_use]
pub fn scaffold_back<E: Serialize>(title: impl Into<String>, dark_mode: bool, tabs: Vec<Tab>, body: Widget, back: E) -> Widget {
    let title = title.into();
    Widget::Scaffold { route: title.clone(), title, body: Box::new(body), tabs, back: Some(tok(back)), dark_mode, theme: None, fab: None, sheet: None, on_refresh: None, refreshing: false, depth: 2 }
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
        theme: None,
        fab: None,
        sheet: None,
        on_refresh: None,
        refreshing: false,
        route: nav.route_key(),
        depth: nav.depth(),
    }
}

/// Apply a [`Theme`] to a scaffold (brand color, corner, density, font). No-op on any
/// other widget. Lets an app brand its UI without new scaffold builder overloads:
/// `with_theme(nav_scaffold(...), Theme { seed, ..Default::default() })`.
pub fn with_theme(widget: Widget, theme: Theme) -> Widget {
    match widget {
        Widget::Scaffold { title, body, tabs, back, dark_mode, fab, sheet, on_refresh, refreshing, route, depth, .. } => Widget::Scaffold {
            title,
            body,
            tabs,
            back,
            dark_mode,
            theme: Some(theme),
            fab,
            sheet,
            on_refresh,
            refreshing,
            route,
            depth,
        },
        other => other,
    }
}

/// Anchor a floating action button over a scaffold's body (the raised primary action).
/// No-op on any other widget: `with_fab(scaffold(...), Icon::Add, Msg::New)`.
pub fn with_fab<E: Serialize>(widget: Widget, icon: Icon, on_press: E) -> Widget {
    match widget {
        Widget::Scaffold { title, body, tabs, back, dark_mode, theme, sheet, on_refresh, refreshing, route, depth, .. } => Widget::Scaffold {
            title,
            body,
            tabs,
            back,
            dark_mode,
            theme,
            fab: Some(Fab { icon, on_press: tok(on_press) }),
            sheet,
            on_refresh,
            refreshing,
            route,
            depth,
        },
        other => other,
    }
}

/// Open a modal bottom sheet over a scaffold's body. No-op on any other widget — drive it from
/// the model: `with_sheet(scaffold(...), title, sheet_body, Msg::CloseSheet)`.
pub fn with_sheet<E: Serialize>(widget: Widget, title: impl Into<String>, child: Widget, on_dismiss: E) -> Widget {
    match widget {
        Widget::Scaffold { title: t, body, tabs, back, dark_mode, theme, fab, on_refresh, refreshing, route, depth, .. } => Widget::Scaffold {
            title: t,
            body,
            tabs,
            back,
            dark_mode,
            theme,
            fab,
            sheet: Some(Sheet { title: title.into(), child: Box::new(child), on_dismiss: tok(on_dismiss) }),
            on_refresh,
            refreshing,
            route,
            depth,
        },
        other => other,
    }
}

/// Enable pull-to-refresh on a scaffold's body: the body becomes pull-refreshable and fires
/// `on_refresh` on pull. `refreshing` is app-owned — set it true when the pull fires and clear it
/// when the async reload completes (the shell shows a spinner while true). No-op on other widgets.
pub fn with_refresh<E: Serialize>(widget: Widget, refreshing: bool, on_refresh: E) -> Widget {
    match widget {
        Widget::Scaffold { title, body, tabs, back, dark_mode, theme, fab, sheet, route, depth, .. } => Widget::Scaffold {
            title,
            body,
            tabs,
            back,
            dark_mode,
            theme,
            fab,
            sheet,
            on_refresh: Some(tok(on_refresh)),
            refreshing,
            route,
            depth,
        },
        // Pull-to-refresh on a LazyList's top — same API as on a Scaffold. Leaves the load-more
        // fields intact.
        Widget::LazyList { children, on_load_more, loading, has_more, .. } => Widget::LazyList {
            children,
            on_load_more,
            loading,
            has_more,
            on_refresh: Some(tok(on_refresh)),
            refreshing,
        },
        other => other,
    }
}

/// A scrollable list for long/paged feeds that fires `on_load_more` when the user scrolls near the
/// end. The app owns the state: append to `children` on each load-more event, set `loading` true
/// while the page loads (the shell shows a spinner and won't re-fire), and `has_more=false` when
/// the feed is exhausted. Add pull-to-refresh at the top with [`with_refresh`]. Give it room — a
/// `LazyList` nested in a scrollable body needs a bounded height to scroll on its own.
#[must_use]
pub fn lazy_list<E: Serialize>(children: Vec<Widget>, loading: bool, has_more: bool, on_load_more: E) -> Widget {
    Widget::LazyList {
        children,
        on_load_more: Some(tok(on_load_more)),
        loading,
        has_more,
        on_refresh: None,
        refreshing: false,
    }
}

/// A scrollable list with no load-more and no refresh — a plain virtualized list of `children`.
#[must_use]
pub fn lazy_list_static(children: Vec<Widget>) -> Widget {
    Widget::LazyList { children, on_load_more: None, loading: false, has_more: false, on_refresh: None, refreshing: false }
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

    // ---- PluginResponse ----

    #[test]
    fn plugin_response_carries_bytes_and_converts_text() {
        let r = PluginResponse::text(true, "hello");
        assert!(r.ok);
        assert_eq!(r.output, b"hello".to_vec());
        assert_eq!(r.as_text(), Some("hello"));

        let binary = PluginResponse { ok: true, output: vec![0xff, 0xfe] };
        assert_eq!(binary.as_text(), None, "invalid UTF-8 must not panic");
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
            Widget::Card { on_press, on_long_press, .. } => {
                assert!(on_press.is_none());
                assert!(on_long_press.is_none());
            }
            other => panic!("expected Card, got {other:?}"),
        }
        // with_long_press attaches a long-press, keeping any existing tap
        match with_long_press(card_button(text("c"), CardStyle::Filled, Ev::Tap), Ev::Open(7)) {
            Widget::Card { on_press, on_long_press, .. } => {
                assert_eq!(on_press, Some(serde_json::to_string(&Ev::Tap).unwrap()));
                assert_eq!(on_long_press, Some(serde_json::to_string(&Ev::Open(7)).unwrap()));
            }
            other => panic!("expected Card, got {other:?}"),
        }
        // with_long_press on a non-Card is a no-op
        assert!(matches!(with_long_press(text("x"), Ev::Tap), Widget::Text { .. }));
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
        cx.put("http://h/p", "putbody", |_| Ev::Tap);
        cx.patch("http://h/z", "patch", |_| Ev::Tap);
        cx.delete("http://h/d", |_| Ev::Tap);

        let methods: Vec<&str> = cx.requests.iter().map(|(c, _)| c.op.as_str()).collect();
        assert_eq!(methods, ["GET", "POST", "PUT", "PATCH", "DELETE"]);
        assert!(cx.requests.iter().all(|(c, _)| c.plugin == "http"));

        let get_input: serde_json::Value = serde_json::from_str(&cx.requests[0].0.input).unwrap();
        assert_eq!(get_input["url"], "http://h/x");
        assert!(get_input["body"].is_null());

        let put_input: serde_json::Value = serde_json::from_str(&cx.requests[2].0.input).unwrap();
        assert_eq!(put_input["url"], "http://h/p");
        assert_eq!(put_input["body"], "putbody");
    }

    #[test]
    fn request_builder_emits_headers_in_order() {
        let mut cx = Cx::<Ev>::default();
        cx.request("PUT", "http://h/access-key")
            .bearer("tok123")
            .header("X-Trace-Id", "abc")
            .body("{}")
            .send(|_| Ev::Tap);

        assert_eq!(cx.requests.len(), 1);
        let (call, _) = &cx.requests[0];
        assert_eq!(call.plugin, "http");
        assert_eq!(call.op, "PUT");

        let input: serde_json::Value = serde_json::from_str(&call.input).unwrap();
        assert_eq!(input["url"], "http://h/access-key");
        assert_eq!(input["body"], "{}");
        assert_eq!(input["headers"][0]["name"], "Authorization");
        assert_eq!(input["headers"][0]["value"], "Bearer tok123");
        assert_eq!(input["headers"][1]["name"], "X-Trace-Id");
        assert_eq!(input["headers"][1]["value"], "abc");
    }

    #[test]
    fn helpers_emit_no_headers_field_content() {
        let mut cx = Cx::<Ev>::default();
        cx.get("http://h/x", |_| Ev::Tap);
        let input: serde_json::Value = serde_json::from_str(&cx.requests[0].0.input).unwrap();
        assert_eq!(input["headers"].as_array().unwrap().len(), 0);
    }

    #[test]
    fn continuation_receives_decoded_outcome() {
        #[derive(Debug, PartialEq)]
        enum Got { Conflict, Offline, Other }

        let classify = |r: PluginResponse| -> Got {
            match HttpOutcome::decode(&r.output).unwrap() {
                HttpOutcome::Response { status: 409, .. } => Got::Conflict,
                HttpOutcome::TransportError { .. } => Got::Offline,
                _ => Got::Other,
            }
        };

        let conflict = HttpOutcome::Response { status: 409, headers: vec![], body: b"c".to_vec() };
        assert_eq!(classify(PluginResponse { ok: false, output: conflict.encode() }), Got::Conflict);

        let offline = HttpOutcome::TransportError { message: "refused".into() };
        assert_eq!(classify(PluginResponse { ok: false, output: offline.encode() }), Got::Offline);
    }

    #[test]
    fn decode_failure_in_continuation_surfaces_as_transport_error() {
        // Drives the actual `send()` callback path (not just `HttpOutcome::decode`
        // directly): stores a continuation via `cx.request(...).send(...)`, then
        // invokes it with a `PluginResponse` whose `output` is malformed bytes, the
        // way the shell would if it returned something undecodable.
        let mut cx = Cx::<Ev>::default();

        cx.request("GET", "http://h/x").send(|outcome| {
            match outcome {
                HttpOutcome::TransportError { message } => {
                    assert!(
                        message.contains("malformed http response"),
                        "unexpected message: {message}"
                    );
                }
                HttpOutcome::Response { .. } => {
                    panic!("garbage bytes must not decode as a Response")
                }
            }
            Ev::Tap
        });

        assert_eq!(cx.requests.len(), 1);
        let (_, continuation) = cx.requests.remove(0);
        // Must not panic: a malformed `output` has to surface as `TransportError`,
        // asserted inside the callback above.
        continuation(PluginResponse { ok: true, output: vec![0xff, 0xff, 0xff] });
    }

    #[test]
    fn cx_pick_and_capture_photo_request_the_right_plugin() {
        let mut cx = Cx::<Ev>::default();
        cx.pick_photo(|_| Ev::Tap);
        cx.capture_photo(|_| Ev::Tap);
        assert_eq!(cx.requests.len(), 2);
        // photo picker = `photo`/`pick`; camera capture = `camera`/`capture`. Both
        // carry empty input (the shell needs no parameters to launch picker/camera).
        assert_eq!((cx.requests[0].0.plugin.as_str(), cx.requests[0].0.op.as_str(), cx.requests[0].0.input.as_str()), ("photo", "pick", ""));
        assert_eq!((cx.requests[1].0.plugin.as_str(), cx.requests[1].0.op.as_str(), cx.requests[1].0.input.as_str()), ("camera", "capture", ""));
    }

    #[test]
    fn cx_capture_photo_routes_success_and_cancel() {
        // Happy path: ok=true delivers the URI to the success branch.
        let mut cx = Cx::<Ev>::default();
        cx.capture_photo(|r| if r.ok { Ev::Open(7) } else { Ev::Tap });
        let (_, then) = cx.requests.pop().unwrap();
        assert!(matches!(then(PluginResponse { ok: true, output: "file:///tmp/shot.jpg".into() }), Ev::Open(7)));

        // Sad path: ok=false (user cancelled / permission denied) takes the else branch.
        let mut cx = Cx::<Ev>::default();
        cx.capture_photo(|r| if r.ok { Ev::Open(7) } else { Ev::Tap });
        let (_, then) = cx.requests.pop().unwrap();
        assert!(matches!(then(PluginResponse { ok: false, output: Vec::new() }), Ev::Tap));
    }

    #[test]
    fn cx_notify_capabilities_map_to_the_right_plugin_and_op() {
        let mut cx = Cx::<Ev>::default();
        cx.copy("c");
        cx.share("s");
        cx.open_url("u");
        cx.toast("t");
        cx.haptic("heavy");
        let got: Vec<(&str, &str, &str)> = cx
            .notifications
            .iter()
            .map(|n| (n.plugin.as_str(), n.op.as_str(), n.input.as_str()))
            .collect();
        assert_eq!(
            got,
            vec![
                ("clipboard", "copy", "c"),
                ("share", "text", "s"),
                ("browser", "open", "u"),
                ("toast", "show", "t"),
                ("haptics", "heavy", ""), // haptic style is the op, input empty
            ]
        );
        assert!(cx.requests.is_empty());
    }

    #[test]
    fn cx_device_model_is_a_request_not_a_notification() {
        let mut cx = Cx::<Ev>::default();
        cx.device_model(|_| Ev::Tap);
        assert!(cx.notifications.is_empty());
        assert_eq!(cx.requests.len(), 1);
        let (call, _) = &cx.requests[0];
        assert_eq!((call.plugin.as_str(), call.op.as_str(), call.input.as_str()), ("device", "model", ""));
    }

    #[test]
    fn cx_device_locale_requests_the_device_locale_op() {
        let mut cx = Cx::<Ev>::default();
        cx.device_locale(|_| Ev::Tap);
        assert!(cx.notifications.is_empty());
        assert_eq!(cx.requests.len(), 1);
        let (call, _) = &cx.requests[0];
        assert_eq!((call.plugin.as_str(), call.op.as_str(), call.input.as_str()), ("device", "locale", ""));
    }

    #[test]
    fn cx_now_requests_the_datetime_now_op() {
        let mut cx = Cx::<Ev>::default();
        cx.now(|_| Ev::Tap);
        assert_eq!(cx.requests.len(), 1);
        let (call, _) = &cx.requests[0];
        assert_eq!((call.plugin.as_str(), call.op.as_str(), call.input.as_str()), ("datetime", "now", ""));
    }

    #[test]
    fn cx_subscribe_enqueues_a_keyed_stream_and_maps_each_event() {
        let mut cx = Cx::<Ev>::default();
        cx.subscribe("ws", "websocket", "stream", "wss://h/x", |r| if r.ok { Ev::Tap } else { Ev::Open(0) });
        // It's a stream, not a one-shot request or a notification.
        assert!(cx.notifications.is_empty());
        assert!(cx.requests.is_empty());
        assert_eq!(cx.streams.len(), 1);
        let (call, on_event) = &cx.streams[0];
        assert_eq!(
            (call.key.as_str(), call.plugin.as_str(), call.op.as_str(), call.input.as_str()),
            ("ws", "websocket", "stream", "wss://h/x")
        );
        // The continuation is `Fn` — it can map MANY events, not just one.
        assert!(matches!(on_event(PluginResponse { ok: true, output: "frame1".into() }), Ev::Tap));
        assert!(matches!(on_event(PluginResponse { ok: true, output: "frame2".into() }), Ev::Tap));
        assert!(matches!(on_event(PluginResponse { ok: false, output: "closed".into() }), Ev::Open(0)));
    }

    #[test]
    fn cx_unsubscribe_enqueues_the_teardown_notify_keyed_by_subscription() {
        let mut cx = Cx::<Ev>::default();
        cx.unsubscribe("ws");
        assert!(cx.streams.is_empty());
        assert_eq!(cx.notifications.len(), 1);
        // The shell tears down the native source registered under this key.
        assert_eq!(
            cx.notifications[0],
            PluginNotify { plugin: "stream".into(), op: "unsubscribe".into(), input: "ws".into() }
        );
    }

    #[test]
    fn cx_confirm_serializes_title_message_and_routes_ok() {
        let mut cx = Cx::<Ev>::default();
        cx.confirm("Delete?", "This cannot be undone.", |r| if r.ok { Ev::Tap } else { Ev::Open(0) });
        let (call, then) = cx.requests.pop().unwrap();
        assert_eq!((call.plugin.as_str(), call.op.as_str()), ("dialog", "confirm"));
        let v: serde_json::Value = serde_json::from_str(&call.input).unwrap();
        assert_eq!(v["title"], "Delete?");
        assert_eq!(v["message"], "This cannot be undone.");
        // ok=true → confirmed branch; ok=false would take the else branch.
        assert!(matches!(then(PluginResponse { ok: true, output: "ok".into() }), Ev::Tap));
    }

    // ---- widget builders ----

    #[test]
    fn text_builders_carry_their_style() {
        assert!(matches!(text("b"), Widget::Text { style: TextStyle::Body, .. }));
        assert!(matches!(title("t"), Widget::Text { style: TextStyle::Title, .. }));
        assert!(matches!(subtitle("s"), Widget::Text { style: TextStyle::Subtitle, .. }));
        assert!(matches!(caption("c"), Widget::Text { style: TextStyle::Caption, .. }));
        assert!(matches!(emphasis("e"), Widget::Text { style: TextStyle::Emphasis, .. }));
    }

    #[test]
    fn layout_and_content_builders_produce_their_variants() {
        assert!(matches!(row(vec![text("a")]), Widget::Row { children } if children.len() == 1));
        assert!(matches!(column(vec![]), Widget::Column { children } if children.is_empty()));
        assert!(matches!(grid(vec![text("a"), text("b")]), Widget::Grid { children } if children.len() == 2));
        assert!(matches!(divider(), Widget::Divider));
        assert!(matches!(bar_chart(vec![1.0, 2.0], vec![]), Widget::Chart { style: ChartStyle::Bar, series, .. } if series[0].values.len() == 2));
        assert!(matches!(line_chart(vec![1.0], vec![]), Widget::Chart { style: ChartStyle::Line, .. }));
        assert!(matches!(donut_chart(vec![ChartSeries::new("a", vec![1.0])]), Widget::Chart { style: ChartStyle::Donut, legend: true, .. }));
        assert!(matches!(gauge_chart(ChartSeries::new("g", vec![3.0]).with_goal(5.0)), Widget::Chart { style: ChartStyle::Gauge, series, .. } if series[0].goal == Some(5.0)));
        let rc = with_bracket(
            region_chart(
                vec![ChartRegion::new(0.0, 3.0, 0.0, 80.0, "80%").vertical()],
                vec![ChartTick::new(3.0, "3 Mt.")],
                65.0, 80.0,
                vec![ChartRefLine::target(80.0, "CHF 80'000"), ChartRefLine::max(90.0, "CHF 90'000")],
                vec![ChartLegendItem::new("Gap", Rgb::new(0x5A, 0x7D, 0x9A))],
            ),
            ChartBracket::new(60.0, 80.0, "Ceiling").with_info(),
        );
        assert!(matches!(rc, Widget::RegionChart { bracket: Some(b), regions, ref_lines, .. } if regions[0].vertical && ref_lines[1].dashed && b.info));
        // June 2026 has 30 days and starts on a Monday (weekday 1).
        assert!(matches!(
            calendar(2026, 6, Some(3), |d| Ev::Open(u32::from(d))),
            Widget::Calendar { first_weekday: 1, selected: Some(3), on_day, .. } if on_day.len() == 30
        ));
        assert!(matches!(
            swipe_action(text("row"), vec![("Delete", Tone::Danger, Ev::Tap)]),
            Widget::SwipeAction { actions, .. } if actions.len() == 1
        ));
        // lazy_list carries the load-more token + app-owned flags; no refresh by default.
        assert!(matches!(
            lazy_list(vec![text("a"), text("b")], false, true, Ev::Tap),
            Widget::LazyList { children, on_load_more: Some(t), loading: false, has_more: true, on_refresh: None, refreshing: false }
                if children.len() == 2 && t == serde_json::to_string(&Ev::Tap).unwrap()
        ));
        assert!(matches!(lazy_list_static(vec![text("a")]), Widget::LazyList { on_load_more: None, on_refresh: None, .. }));
        // with_refresh adds pull-to-refresh to a LazyList without disturbing the load-more fields.
        assert!(matches!(
            with_refresh(lazy_list(vec![text("a")], true, false, Ev::Tap), true, Ev::Open(9)),
            Widget::LazyList { on_load_more: Some(_), loading: true, has_more: false, on_refresh: Some(r), refreshing: true, .. }
                if r == serde_json::to_string(&Ev::Open(9)).unwrap()
        ));
        assert!(matches!(spacer(Spacing::Lg), Widget::Spacer { .. }));
        assert!(matches!(image("u", ImageShape::Circle, ImageRatio::Square), Widget::Image { .. }));
        assert!(matches!(badge("new", Tone::Success), Widget::Badge { .. }));
        assert!(matches!(color_dot(ProjectColor::Teal), Widget::ColorDot { .. }));
        assert!(matches!(card(text("x"), CardStyle::Filled), Widget::Card { on_press: None, .. }));
        // a scrim z-stack keeps its align + scrim flag
        assert!(matches!(stack(BoxAlign::Center, true, vec![]), Widget::Box { scrim: true, .. }));
        // split: children boxed, show_detail + on_back carried.
        assert!(matches!(split(text("list"), text("detail"), true, Ev::Tap),
            Widget::Split { show_detail: true, on_back: Some(_), .. }));
    }

    #[test]
    fn input_builders_carry_ids_values_and_event_tokens() {
        assert!(matches!(text_field("id", "ph", "v"), Widget::TextField { kind: FieldKind::Text, error: None, .. }));
        assert!(matches!(pdf_view("https://x/report.pdf"), Widget::PdfView { url } if url == "https://x/report.pdf"));
        assert!(matches!(web_view("https://iframe.mediadelivery.net/embed/1/abc"), Widget::WebView { url } if url == "https://iframe.mediadelivery.net/embed/1/abc"));
        // video_player defaults + the cosmetic modifiers (match-and-rebind like with_refresh).
        assert!(matches!(video_player("v", "https://x/c.mp4", false, -1, Ev::Tap),
            Widget::Video { id, playing: false, seek_to_ms: -1, controls: true, looping: false, muted: false, on_ended: Some(_), .. } if id == "v"));
        assert!(matches!(without_controls(with_muted(with_loop(video_player("v", "u", true, 0, Ev::Tap)))),
            Widget::Video { playing: true, controls: false, looping: true, muted: true, .. }));
        // v2 defaults + modifiers.
        assert!(matches!(video_player("v", "u", false, -1, Ev::Tap),
            Widget::Video { poster: None, start_at_ms: -1, rate, volume, allow_pip: false, .. }
                if (rate - 1.0).abs() < f32::EPSILON && (volume - 1.0).abs() < f32::EPSILON));
        let tuned = with_pip(with_volume(with_rate(with_start_at(with_poster(
            with_captions(video_player("v", "u", true, -1, Ev::Tap),
                vec![Caption { url: "e.vtt".into(), label: "EN".into(), language: "en".into(), default_on: true }]),
            "p.jpg"), 9000), 1.5), 0.5));
        assert!(matches!(tuned,
            Widget::Video { poster: Some(p), start_at_ms: 9000, rate, volume, allow_pip: true, captions, .. }
                if p == "p.jpg" && (rate - 1.5).abs() < f32::EPSILON && (volume - 0.5).abs() < f32::EPSILON && captions.len() == 1));
        // playlist builder: url defaults to the first clip; urls/start_index carried; seek_index jumps.
        assert!(matches!(with_seek_index(video_playlist("pl", vec!["a.mp4".into(), "b.mp4".into()], 1, true, Ev::Tap), 0),
            Widget::Video { url, urls, start_index: 1, seek_index: 0, .. } if url == "a.mp4" && urls.len() == 2));
        // modifiers are no-ops on non-Video widgets.
        assert!(matches!(with_pip(divider()), Widget::Divider));
        assert!(matches!(secure_field("pw", "Password", ""), Widget::TextField { kind: FieldKind::Secure, .. }));
        assert!(matches!(email_field("e", "", ""), Widget::TextField { kind: FieldKind::Email, .. }));
        assert!(matches!(multiline_field("note", "", ""), Widget::TextField { kind: FieldKind::Multiline, .. }));
        assert!(matches!(with_error(email_field("e", "", "x"), "Invalid"), Widget::TextField { error: Some(m), kind: FieldKind::Email, .. } if m == "Invalid"));
        assert!(matches!(with_error(divider(), "ignored"), Widget::Divider));
        assert!(matches!(toggle("t", "l", true), Widget::Toggle { value: true, .. }));
        assert!(matches!(checkbox("c", "l", false), Widget::Checkbox { value: false, .. }));
        assert!(matches!(slider("s", 3, 10), Widget::Slider { value: 3, max: 10, .. }));

        match chip("Latte", true, Ev::Open(2)) {
            Widget::Chip { selected, on_press, .. } => {
                assert!(selected);
                assert_eq!(on_press, serde_json::to_string(&Ev::Open(2)).unwrap());
            }
            other => panic!("expected Chip, got {other:?}"),
        }
        match stepper(5, Ev::Tap, Ev::Open(1)) {
            Widget::Stepper { value, on_decrement, on_increment } => {
                assert_eq!(value, 5);
                assert_eq!(on_decrement, serde_json::to_string(&Ev::Tap).unwrap());
                assert_eq!(on_increment, serde_json::to_string(&Ev::Open(1)).unwrap());
            }
            other => panic!("expected Stepper, got {other:?}"),
        }
        let t = tab("Home", true, Ev::Tap);
        assert_eq!(t.label, "Home");
        assert!(t.selected);
        assert_eq!(t.on_select, serde_json::to_string(&Ev::Tap).unwrap());
    }

    // ---- ABI serialization round-trips (structural stability of the wire types) ----

    #[test]
    fn widget_tree_round_trips_through_serde() {
        let tree = scaffold(
            "Home",
            true,
            vec![tab("A", true, Ev::Tap)],
            column(vec![
                title("Hi"),
                row(vec![button("Go", ButtonStyle::Filled, Ev::Open(3)), chip("x", false, Ev::Tap)]),
                image("u", ImageShape::Rounded, ImageRatio::Wide),
                slider("s", 2, 5),
            ]),
        );
        let s = serde_json::to_string(&tree).unwrap();
        let back: Widget = serde_json::from_str(&s).unwrap();
        assert_eq!(s, serde_json::to_string(&back).unwrap());
    }

    #[test]
    fn actions_and_input_values_round_trip() {
        let actions = vec![
            Action::Fired { token: serde_json::to_string(&Ev::Open(1)).unwrap() },
            Action::Input { id: "n".into(), value: InputValue::Int(7) },
            Action::Input { id: "n".into(), value: InputValue::Text("hi".into()) },
            Action::Input { id: "n".into(), value: InputValue::Bool(true) },
            Action::Restore { data: "blob".into() },
            Action::Start,
        ];
        for a in actions {
            let s = serde_json::to_string(&a).unwrap();
            let back: Action = serde_json::from_str(&s).unwrap();
            assert_eq!(s, serde_json::to_string(&back).unwrap());
        }
    }

    // ---- MobilerShell: the fixed-ABI action dispatch ----

    #[derive(Default)]
    struct CounterModel {
        count: i32,
        restored: String,
        started: bool,
        last_input: String,
    }

    #[derive(serde::Serialize, serde::Deserialize)]
    enum CounterEv {
        Inc,
        Add(i32),
    }

    #[derive(Default)]
    struct CounterApp;

    impl MobilerApp for CounterApp {
        type Event = CounterEv;
        type Model = CounterModel;
        fn update(&self, ev: CounterEv, model: &mut CounterModel, _cx: &mut Cx<CounterEv>) {
            match ev {
                CounterEv::Inc => model.count += 1,
                CounterEv::Add(n) => model.count += n,
            }
        }
        fn input(&self, id: &str, value: InputValue, model: &mut CounterModel, _cx: &mut Cx<CounterEv>) {
            if let InputValue::Text(t) = value {
                model.last_input = format!("{id}={t}");
            }
        }
        fn restore(&self, data: &str, model: &mut CounterModel) {
            model.restored = data.to_string();
        }
        fn init(&self, model: &mut CounterModel, _cx: &mut Cx<CounterEv>) {
            model.started = true;
        }
        fn view(&self, model: &CounterModel) -> Widget {
            text(format!("{}", model.count))
        }
    }

    #[test]
    fn shell_dispatches_fired_input_restore_and_start() {
        use crux_core::App as _;
        let shell = MobilerShell::<CounterApp>::default();
        let mut m = CounterModel::default();

        // Fired with a valid token → the typed event reaches app.update.
        let _ = shell.update(Action::Fired { token: serde_json::to_string(&CounterEv::Add(5)).unwrap() }, &mut m);
        assert_eq!(m.count, 5);
        // Input → app.input.
        let _ = shell.update(Action::Input { id: "name".into(), value: InputValue::Text("bob".into()) }, &mut m);
        assert_eq!(m.last_input, "name=bob");
        // Restore → app.restore.
        let _ = shell.update(Action::Restore { data: "saved".into() }, &mut m);
        assert_eq!(m.restored, "saved");
        // Start → app.init.
        let _ = shell.update(Action::Start, &mut m);
        assert!(m.started);
        // view renders the (mutated) model through the ABI.
        assert!(matches!(shell.view(&m), Widget::Text { .. }));
    }

    #[test]
    fn shell_ignores_a_malformed_fired_token() {
        use crux_core::App as _;
        let shell = MobilerShell::<CounterApp>::default();
        let mut m = CounterModel::default();
        // A token that doesn't deserialize to the app's event type is dropped — no
        // panic, model untouched (the `if let Ok(event)` guard in MobilerShell::update).
        let _ = shell.update(Action::Fired { token: "not a valid token".into() }, &mut m);
        assert_eq!(m.count, 0);
    }
}
