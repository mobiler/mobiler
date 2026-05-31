//! `mobiler-web` — Mobiler's web shell.
//!
//! Renders **any** Mobiler app's `Widget` tree to the DOM (Leptos / WASM), driving
//! the Rust core via crux's `Core` and fulfilling capabilities (HTTP) with the
//! browser's `fetch`. The web twin of the generic Android/SwiftUI shells: write
//! your app once as a `MobilerApp`, then
//!
//! ```ignore
//! fn main() { mobiler_web::run::<my_app::App>(); }
//! ```
//!
//! renders it on the web — fully styled, no CSS required: the shell ships its own
//! theme (`mobiler.css`) and injects it on mount, and `Scaffold.dark_mode` flips
//! the whole theme. Your crate only supplies a minimal `index.html` with the Trunk
//! entry point; an app may add its own stylesheet to override any widget class.

use std::cell::RefCell;
use std::sync::Arc;

use crux_core::{App, Core};
use leptos::prelude::*;
use mobiler_core::{
    Action, BoxAlign, ButtonStyle, CardStyle, Corner, Density, Effect, FontFamily, Icon, ImageRatio,
    ImageShape, InputValue, PluginCall, PluginNotify, PluginResponse, ProjectColor, Spacing,
    TextStyle, Theme, Tone, Widget,
};
use wasm_bindgen_futures::spawn_local;

/// The shell's own stylesheet — the web twin of the look the Android/SwiftUI shells
/// decide in code. Shipped with the crate and injected on mount, so `run::<App>()`
/// renders a fully styled, themeable app with no CSS required from the consuming
/// app (it can still override any class). Uses CSS variables so `Scaffold.dark_mode`
/// flips the whole theme by toggling one class.
const STYLE: &str = include_str!("mobiler.css");

/// Cloneable handle for sending an `Action` into the core. Leptos 0.7 view closures
/// require `Send`, so this is `Arc` + `Send + Sync` (the crux `Core` is both).
type Dispatch = Arc<dyn Fn(Action) + Send + Sync>;

/// What a Mobiler app must be to render on the web: a crux `App` speaking the fixed
/// ABI (`Action` in, `Widget` out, `Effect` for capabilities). `MobilerShell<_>`
/// satisfies this automatically.
pub trait WebApp:
    App<Event = Action, ViewModel = Widget, Effect = Effect> + Default + Send + Sync + 'static
where
    Self::Model: Default + Send + Sync,
{
}
impl<T> WebApp for T
where
    T: App<Event = Action, ViewModel = Widget, Effect = Effect> + Default + Send + Sync + 'static,
    T::Model: Default + Send + Sync,
{
}

/// Mount a Mobiler app into the document body. Call from your wasm `main`.
pub fn run<A: WebApp>()
where
    A::Model: Default + Send + Sync,
{
    console_error_panic_hook::set_once();
    inject_default_style();
    leptos::mount::mount_to_body(shell::<A>);
}

/// Inject the shell's default stylesheet at the **front** of `<head>` so it's the
/// lowest-precedence baseline: an app that ships its own CSS (later in the document)
/// overrides any of these classes, while an app with no CSS still gets a full theme.
fn inject_default_style() {
    let document = leptos::prelude::document();
    let Some(head) = document.head() else { return };
    let Ok(style) = document.create_element("style") else { return };
    let _ = style.set_attribute("data-mobiler", "shell");
    style.set_text_content(Some(STYLE));
    let _ = head.insert_before(&style, head.first_child().as_ref());
}

fn shell<A: WebApp>() -> impl IntoView
where
    A::Model: Default + Send + Sync,
{
    let core = Arc::new(Core::<A>::new());
    let (view, set_view) = signal(core.view());

    let send: Dispatch = {
        let core = core.clone();
        Arc::new(move |action: Action| {
            let effects = core.process_event(action);
            drive(&core, set_view, effects);
        })
    };

    // Restore persisted state (localStorage), then fire Start — mirrors the native
    // shells (which restore before Start so the app sees its saved Model on launch).
    let saved = local_storage().and_then(|s| s.get_item(STORAGE_KEY).ok().flatten()).unwrap_or_default();
    if !saved.is_empty() {
        send(Action::Restore { data: saved });
    }
    send(Action::Start);

    let send_for_view = send.clone();
    view! {
        <div class="app">
            {move || render(&view.get(), &send_for_view)}
        </div>
    }
}

/// Process effects: re-read the view on Render; fulfil HTTP via fetch and resolve.
fn drive<A: WebApp>(core: &Arc<Core<A>>, set_view: WriteSignal<Widget>, effects: Vec<Effect>)
where
    A::Model: Default + Send + Sync,
{
    for effect in effects {
        match effect {
            Effect::Render(_) => set_view.set(core.view()),
            Effect::PluginNotify(notify) => perform_notify(&notify.operation),
            Effect::Plugin(mut request) => {
                let core = core.clone();
                spawn_local(async move {
                    let response = perform(&request.operation).await;
                    if let Ok(next) = core.resolve(&mut request, response) {
                        drive(&core, set_view, next);
                    }
                });
            }
        }
    }
}

/// Fulfil a request/response capability. `http` via `fetch`; `device` via the
/// browser's user-agent string (the web analogue of a device model).
async fn perform(call: &PluginCall) -> PluginResponse {
    if call.plugin == "device" {
        let ua = web_sys::window()
            .and_then(|w| w.navigator().user_agent().ok())
            .unwrap_or_default();
        return PluginResponse { ok: true, output: ua };
    }
    if call.plugin == "photo" && call.op == "pick" {
        return take_image(false).await;
    }
    if call.plugin == "camera" && call.op == "capture" {
        return take_image(true).await;
    }
    if call.plugin == "dialog" && call.op == "confirm" {
        let v: serde_json::Value = serde_json::from_str(&call.input).unwrap_or(serde_json::Value::Null);
        let title = v.get("title").and_then(serde_json::Value::as_str).unwrap_or("");
        let message = v.get("message").and_then(serde_json::Value::as_str).unwrap_or("");
        let prompt = if title.is_empty() { message.to_string() } else { format!("{title}\n\n{message}") };
        let ok = web_sys::window()
            .and_then(|w| w.confirm_with_message(&prompt).ok())
            .unwrap_or(false);
        return PluginResponse { ok, output: if ok { "ok".into() } else { "cancel".into() } };
    }
    if call.plugin != "http" {
        return PluginResponse { ok: false, output: format!("plugin '{}' not available", call.plugin) };
    }
    let v: serde_json::Value = serde_json::from_str(&call.input).unwrap_or(serde_json::Value::Null);
    let url = v.get("url").and_then(serde_json::Value::as_str).unwrap_or("");
    let body = v.get("body").and_then(serde_json::Value::as_str);

    use gloo_net::http::Request;
    let builder = match call.op.as_str() {
        "POST" => Request::post(url),
        "PATCH" => Request::patch(url),
        "DELETE" => Request::delete(url),
        _ => Request::get(url),
    };
    let request = match body {
        Some(b) => builder.header("Content-Type", "application/json").body(b),
        None => builder.build(),
    };
    let request = match request {
        Ok(r) => r,
        Err(e) => return PluginResponse { ok: false, output: e.to_string() },
    };
    match request.send().await {
        Ok(resp) => PluginResponse { ok: resp.ok(), output: resp.text().await.unwrap_or_default() },
        Err(e) => PluginResponse { ok: false, output: e.to_string() },
    }
}

/// Pick or capture an image via a hidden `<input type=file accept=image/*>`, clicked
/// to open the browser's file dialog — or, with `capture`, to hint the device camera on
/// supporting mobile browsers (desktop falls back to the file dialog). Awaits the
/// `change` event and returns a `blob:` object URL the `<img>` renderer loads. No
/// permission needed (the picker/camera prompt is the browser's). Backs both the
/// `photo`/`pick` and `camera`/`capture` capabilities.
async fn take_image(capture: bool) -> PluginResponse {
    use wasm_bindgen::{closure::Closure, JsCast};
    let Some(doc) = web_sys::window().and_then(|w| w.document()) else {
        return PluginResponse { ok: false, output: "no document".into() };
    };
    let Some(input) = doc.create_element("input").ok().and_then(|e| e.dyn_into::<web_sys::HtmlInputElement>().ok()) else {
        return PluginResponse { ok: false, output: "no input element".into() };
    };
    input.set_type("file");
    input.set_accept("image/*");
    if capture {
        // Hints the environment-facing camera on mobile browsers that support it.
        let _ = input.set_attribute("capture", "environment");
    }

    let (tx, rx) = futures_channel::oneshot::channel::<Option<String>>();
    let tx = std::cell::RefCell::new(Some(tx));
    let input_for_cb = input.clone();
    let on_change = Closure::wrap(Box::new(move || {
        let url = input_for_cb
            .files()
            .and_then(|files| files.get(0))
            .and_then(|file| web_sys::Url::create_object_url_with_blob(&file).ok());
        if let Some(tx) = tx.borrow_mut().take() {
            let _ = tx.send(url);
        }
    }) as Box<dyn FnMut()>);
    input.set_onchange(Some(on_change.as_ref().unchecked_ref()));
    input.click();
    on_change.forget(); // keep the handler alive until `change` fires

    match rx.await {
        Ok(Some(url)) => PluginResponse { ok: true, output: url },
        _ => PluginResponse { ok: false, output: "cancelled".into() },
    }
}

const STORAGE_KEY: &str = "mobiler.state";

/// `window.localStorage`, if available.
fn local_storage() -> Option<web_sys::Storage> {
    web_sys::window()?.local_storage().ok().flatten()
}

/// Fulfil a fire-and-forget capability in the browser — the web twin of the native
/// shells' notify handlers (storage/clipboard/share/browser). None block; an unknown
/// capability is a graceful no-op.
fn perform_notify(notify: &PluginNotify) {
    let win = match web_sys::window() {
        Some(w) => w,
        None => return,
    };
    match (notify.plugin.as_str(), notify.op.as_str()) {
        // Persist the state blob (paired with cx.save + restore-on-startup above).
        ("storage", "save") => {
            if let Some(s) = local_storage() {
                let _ = s.set_item(STORAGE_KEY, &notify.input);
            }
        }
        // Copy to the clipboard (write_text returns a Promise we let run).
        ("clipboard", "copy") => {
            let _ = win.navigator().clipboard().write_text(&notify.input);
        }
        // Open a URL in a new tab.
        ("browser", "open") => {
            let _ = win.open_with_url_and_target(&notify.input, "_blank");
        }
        // No reliable cross-browser share sheet (navigator.share is mobile-only and
        // gesture-gated), so degrade to copying — a sane universal fallback.
        ("share", _) => {
            let _ = win.navigator().clipboard().write_text(&notify.input);
        }
        // Transient toast: a styled div appended to <body>, auto-removed after a beat.
        ("toast", _) => show_toast(&notify.input),
        // Haptic tap. navigator.vibrate is unsupported on iOS Safari (a graceful no-op).
        ("haptics", style) => {
            let ms = match style {
                "light" => 15,
                "heavy" => 50,
                _ => 30, // medium / unknown
            };
            let _ = win.navigator().vibrate_with_duration(ms);
        }
        _ => {} // unknown capability: ignore
    }
}

/// Append a transient toast to `<body>` (styled by `.toast` in mobiler.css) and
/// remove it after ~2.6 s — the web twin of the native toast/snackbar.
fn show_toast(text: &str) {
    let Some(doc) = web_sys::window().and_then(|w| w.document()) else { return };
    let (Ok(el), Some(body)) = (doc.create_element("div"), doc.body()) else { return };
    el.set_class_name("toast");
    el.set_text_content(Some(text));
    let _ = body.append_child(&el);
    gloo_timers::callback::Timeout::new(2600, move || el.remove()).forget();
}

// ---------------- Widget → DOM ----------------

/// `Widget` → DOM. **Exhaustive** by construction — the `match` has no catch-all,
/// so (like the Compose/SwiftUI shells) it won't compile until every `Widget`
/// variant is handled. Style *intent* (TextStyle, Tone, …) becomes a CSS class;
/// the concrete look lives in `mobiler.css`.
fn render(widget: &Widget, send: &Dispatch) -> AnyView {
    match widget {
        // ---- content ----
        Widget::Text { content, style } => {
            let (class, content) = (text_class(*style), content.clone());
            view! { <p class=class>{content}</p> }.into_any()
        }
        Widget::Image { source, shape, ratio } => {
            let (class, source) = (image_class(*shape, *ratio), source.clone());
            view! { <img class=class src=source /> }.into_any()
        }
        Widget::Badge { label, tone } => {
            let (class, label) = (format!("badge {}", tone_class(*tone)), label.clone());
            view! { <span class=class>{label}</span> }.into_any()
        }
        Widget::ColorDot { color } => {
            view! { <span class=format!("dot {}", dot_class(*color))></span> }.into_any()
        }
        Widget::Divider => view! { <hr class="divider" /> }.into_any(),
        Widget::Spacer { size } => {
            view! { <div class=format!("spacer {}", spacer_class(*size))></div> }.into_any()
        }

        // ---- layout ----
        Widget::Row { children } => {
            let kids = render_all(children, send);
            view! { <div class="row">{kids}</div> }.into_any()
        }
        Widget::Column { children } => {
            let kids = render_all(children, send);
            view! { <div class="col">{kids}</div> }.into_any()
        }
        Widget::Card { child, style, on_press } => {
            let class = format!("card {}", card_class(*style));
            let body = render(child, send);
            match on_press {
                Some(token) => {
                    let (send, token) = (send.clone(), token.clone());
                    view! {
                        <button
                            class=format!("{class} card-tappable")
                            on:click=move |_| send(Action::Fired { token: token.clone() })
                        >
                            {body}
                        </button>
                    }
                    .into_any()
                }
                None => view! { <div class=class>{body}</div> }.into_any(),
            }
        }
        // Z-stack. With `scrim`, the first child is a background image, darkened
        // by an overlay, and the rest layer on top in light content — the DOM twin
        // of the Compose `matchParentSize` scrim / SwiftUI `.overlay` on the image.
        Widget::Box { children, align, scrim } => {
            let acls = align_class(*align);
            if *scrim && children.len() > 1 {
                let bg = render(&children[0], send);
                let content = render_all(&children[1..], send);
                view! {
                    <div class=format!("box box-scrim {acls}")>
                        {bg}
                        <div class="scrim"></div>
                        <div class="box-content">{content}</div>
                    </div>
                }
                .into_any()
            } else {
                let kids = render_all(children, send);
                view! { <div class=format!("box {acls}")>{kids}</div> }.into_any()
            }
        }
        Widget::Grid { children } => {
            let kids = render_all(children, send);
            view! { <div class="grid">{kids}</div> }.into_any()
        }

        // ---- input / actions ----
        Widget::Button { label, style, on_press } => {
            let (send, token, label) = (send.clone(), on_press.clone(), label.clone());
            let class = format!("btn {}", button_class(*style));
            view! {
                <button class=class on:click=move |_| send(Action::Fired { token: token.clone() })>
                    {label}
                </button>
            }
            .into_any()
        }
        Widget::IconButton { icon, on_press } => {
            let (send, token) = (send.clone(), on_press.clone());
            let glyph = icon_glyph(*icon);
            view! {
                <button class="iconbtn" on:click=move |_| send(Action::Fired { token: token.clone() })>
                    {glyph}
                </button>
            }
            .into_any()
        }
        Widget::Chip { label, selected, on_press } => {
            let (send, token, label) = (send.clone(), on_press.clone(), label.clone());
            let class = if *selected { "chip selected" } else { "chip" };
            view! {
                <button class=class on:click=move |_| send(Action::Fired { token: token.clone() })>
                    {label}
                </button>
            }
            .into_any()
        }
        Widget::TextField { id, placeholder, value } => {
            let (send, id) = (send.clone(), id.clone());
            let (placeholder, value) = (placeholder.clone(), value.clone());
            view! {
                <input
                    class="field"
                    placeholder=placeholder
                    prop:value=value
                    on:input=move |ev| send(Action::Input {
                        id: id.clone(),
                        value: InputValue::Text(event_target_value(&ev)),
                    })
                />
            }
            .into_any()
        }
        Widget::Toggle { id, label, value } => {
            let (send, id, label, checked) = (send.clone(), id.clone(), label.clone(), *value);
            view! {
                <label class="toggle">
                    {label}
                    <input
                        type="checkbox"
                        role="switch"
                        prop:checked=checked
                        on:change=move |ev| send(Action::Input {
                            id: id.clone(),
                            value: InputValue::Bool(event_target_checked(&ev)),
                        })
                    />
                </label>
            }
            .into_any()
        }
        Widget::Checkbox { id, label, value } => {
            let (send, id, label, checked) = (send.clone(), id.clone(), label.clone(), *value);
            view! {
                <label class="check">
                    <input
                        type="checkbox"
                        prop:checked=checked
                        on:change=move |ev| send(Action::Input {
                            id: id.clone(),
                            value: InputValue::Bool(event_target_checked(&ev)),
                        })
                    />
                    {label}
                </label>
            }
            .into_any()
        }
        Widget::Slider { id, value, max } => {
            let (send, id, value, max) = (send.clone(), id.clone(), *value, *max);
            view! {
                <input
                    class="slider"
                    type="range"
                    min="0"
                    max=max
                    prop:value=value
                    on:input=move |ev| send(Action::Input {
                        id: id.clone(),
                        value: InputValue::Int(event_target_value(&ev).parse().unwrap_or(0)),
                    })
                />
            }
            .into_any()
        }
        Widget::Stepper { value, on_decrement, on_increment } => {
            let send_dec = send.clone();
            let send_inc = send.clone();
            let (dec, inc) = (on_decrement.clone(), on_increment.clone());
            view! {
                <div class="stepper">
                    <button on:click=move |_| send_dec(Action::Fired { token: dec.clone() })>"−"</button>
                    <span class="stepper-value">{*value}</span>
                    <button on:click=move |_| send_inc(Action::Fired { token: inc.clone() })>"+"</button>
                </div>
            }
            .into_any()
        }

        // ---- shell ----
        Widget::Scaffold { title, body, tabs, back, dark_mode, theme, route, depth } => {
            let back_btn = back.clone().map(|token| {
                let send = send.clone();
                view! {
                    <button class="back" on:click=move |_| send(Action::Fired { token: token.clone() })>
                        "‹"
                    </button>
                }
            });
            let tabbar = (!tabs.is_empty()).then(|| {
                let tabs: Vec<AnyView> = tabs
                    .iter()
                    .map(|tab| {
                        let (send, token) = (send.clone(), tab.on_select.clone());
                        let class = if tab.selected { "tab selected" } else { "tab" };
                        let label = tab.label.clone();
                        view! {
                            <button class=class on:click=move |_| send(Action::Fired { token: token.clone() })>
                                {label}
                            </button>
                        }
                        .into_any()
                    })
                    .collect();
                view! { <div class="tabbar">{tabs}</div> }
            });
            // `theme-dark` flips the CSS variables for the whole shell — theme-as-data,
            // the web twin of the native shells' `preferredColorScheme`/Material theme.
            let class = if *dark_mode { "scaffold theme-dark" } else { "scaffold" };
            let body_class = format!("scaffold-body {}", nav_class(route, *depth));
            // An app `Theme` overrides the CSS variables inline (brand color, corner, density,
            // font) — the web twin of the native shells' brand/tint + shape + spacing + font.
            let theme_style = theme.as_ref().map(theme_css).unwrap_or_default();
            let (title, body) = (title.clone(), render(body, send));
            view! {
                <div class=class style=theme_style>
                    <div class="topbar">
                        {back_btn}
                        <span class="title">{title}</span>
                    </div>
                    <div class=body_class data-route=route.clone()>{body}</div>
                    {tabbar}
                </div>
            }
            .into_any()
        }
    }
}

/// Render a slice of children as sibling views.
fn render_all(children: &[Widget], send: &Dispatch) -> Vec<AnyView> {
    children.iter().map(|c| render(c, send)).collect()
}

thread_local! {
    /// (previous route key, previous depth, alternating toggle). The render is a
    /// stateless whole-tree rebuild, so nav state lives here (wasm is single-
    /// threaded). Lets the Scaffold body animate on navigation — the web twin of
    /// the native shells keying their body on `route`.
    static NAV: RefCell<(String, u32, bool)> = const { RefCell::new((String::new(), 0, false)) };
}

/// Render an app [`Theme`] as inline CSS custom properties on the scaffold root — the web
/// twin of the native brand/tint + shape + spacing + font. Overrides `mobiler.css`'s defaults
/// (its rules read these via `var(--…)`); dark mode still works (it only swaps the colors the
/// seed doesn't pin).
fn theme_css(t: &Theme) -> String {
    let (r, g, b) = (t.seed.r, t.seed.g, t.seed.b);
    let radius = match t.corner {
        Corner::None => "0px",
        Corner::Small => "8px",
        Corner::Medium => "14px",
        Corner::Large => "22px",
    };
    let (gap, pad) = match t.density {
        Density::Compact => ("8px", "10px"),
        Density::Comfortable => ("12px", "14px"),
    };
    let font = match t.font {
        FontFamily::System => "system-ui, -apple-system, \"Segoe UI\", Roboto, sans-serif",
        FontFamily::Rounded => "ui-rounded, \"SF Pro Rounded\", \"Segoe UI\", system-ui, sans-serif",
        FontFamily::Serif => "ui-serif, Georgia, \"Times New Roman\", serif",
        FontFamily::Monospace => "ui-monospace, \"SF Mono\", \"Cascadia Code\", Menlo, monospace",
    };
    format!(
        "--primary:rgb({r},{g},{b});--accent:rgb({r},{g},{b});\
         --accent-soft:rgba({r},{g},{b},0.16);--radius:{radius};\
         --gap:{gap};--pad:{pad};--font:{font};"
    )
}

/// Pick the Scaffold body's transition class for this render. Returns `""` for a
/// same-route data update (re-render in place, no transition). On a route change it
/// returns a directional class — slide-in from the right when `depth` grew (push),
/// from the left when it shrank (pop), a crossfade for a lateral move — and *alternates*
/// the `-a`/`-b` suffix each navigation so the CSS animation restarts even though
/// Leptos reuses the same DOM node.
fn nav_class(route: &str, depth: u32) -> &'static str {
    NAV.with_borrow_mut(|(prev_route, prev_depth, toggle)| {
        if route == prev_route {
            return "";
        }
        let dir = if depth > *prev_depth {
            ["nav-push-a", "nav-push-b"]
        } else if depth < *prev_depth {
            ["nav-pop-a", "nav-pop-b"]
        } else {
            ["nav-fade-a", "nav-fade-b"]
        };
        *toggle = !*toggle;
        *prev_route = route.to_string();
        *prev_depth = depth;
        dir[usize::from(*toggle)]
    })
}

// ---- style intent → CSS class / glyph (the only place that names the look) ----

fn text_class(s: TextStyle) -> &'static str {
    match s {
        TextStyle::Title => "t-title",
        TextStyle::Subtitle => "t-subtitle",
        TextStyle::Caption => "t-caption",
        TextStyle::Emphasis => "t-emphasis",
        TextStyle::Body => "t-body",
    }
}

fn button_class(s: ButtonStyle) -> &'static str {
    match s {
        ButtonStyle::Filled => "btn-filled",
        ButtonStyle::Outlined => "btn-outlined",
        ButtonStyle::Text => "btn-text",
    }
}

fn card_class(s: CardStyle) -> &'static str {
    match s {
        CardStyle::Elevated => "card-elevated",
        CardStyle::Outlined => "card-outlined",
        CardStyle::Filled => "card-filled",
    }
}

fn tone_class(t: Tone) -> &'static str {
    match t {
        Tone::Neutral => "tone-neutral",
        Tone::Success => "tone-success",
        Tone::Warning => "tone-warning",
        Tone::Danger => "tone-danger",
        Tone::Info => "tone-info",
    }
}

fn spacer_class(s: Spacing) -> &'static str {
    match s {
        Spacing::Xs => "sp-xs",
        Spacing::Sm => "sp-sm",
        Spacing::Md => "sp-md",
        Spacing::Lg => "sp-lg",
        Spacing::Xl => "sp-xl",
    }
}

fn icon_glyph(i: Icon) -> &'static str {
    match i {
        Icon::Delete => "🗑",
        Icon::Add => "＋",
        Icon::Edit => "✏️",
        Icon::Close => "✕",
        Icon::Settings => "⚙",
        Icon::Check => "✓",
        Icon::Star => "★",
    }
}

fn image_class(shape: ImageShape, ratio: ImageRatio) -> String {
    let shape = match shape {
        ImageShape::Square => "img-square",
        ImageShape::Rounded => "img-rounded",
        ImageShape::Circle => "img-circle",
    };
    let ratio = match ratio {
        ImageRatio::Wide => "ratio-wide",
        ImageRatio::Square => "ratio-square",
        ImageRatio::Tall => "ratio-tall",
    };
    format!("img {shape} {ratio}")
}

fn dot_class(c: ProjectColor) -> &'static str {
    match c {
        ProjectColor::Indigo => "dot-indigo",
        ProjectColor::Teal => "dot-teal",
        ProjectColor::Coral => "dot-coral",
        ProjectColor::Amber => "dot-amber",
        ProjectColor::Lime => "dot-lime",
        ProjectColor::Pink => "dot-pink",
    }
}

fn align_class(a: BoxAlign) -> &'static str {
    match a {
        BoxAlign::TopStart => "align-top-start",
        BoxAlign::TopEnd => "align-top-end",
        BoxAlign::Center => "align-center",
        BoxAlign::BottomStart => "align-bottom-start",
        BoxAlign::BottomCenter => "align-bottom-center",
        BoxAlign::BottomEnd => "align-bottom-end",
    }
}
