//! `web-widgets` — the Widget→DOM web shell (≈ `mobiler-web`).
//!
//! Hosts the SAME `todo-core` as the native app (driven via crux's `Core`) and
//! renders its `Widget` tree to the DOM. Capabilities (HTTP) are fulfilled with
//! the browser's `fetch`. The app code is reused verbatim — only the renderer and
//! the capability host differ from the Android shell.

use std::sync::Arc;

use crux_core::Core;
use leptos::prelude::*;
use mobiler_core::{
    Action, ButtonStyle, Effect, Icon, InputValue, PluginCall, PluginResponse, TextStyle, Widget,
};
use todo_core::App;
use wasm_bindgen_futures::spawn_local;

/// A cloneable handle for sending an `Action` into the core. Leptos 0.7's view
/// closures require `Send`, so this is `Arc` + `Send + Sync` (the crux `Core` is).
type Dispatch = Arc<dyn Fn(Action) + Send + Sync>;

fn main() {
    console_error_panic_hook::set_once();
    leptos::mount::mount_to_body(shell);
}

fn shell() -> impl IntoView {
    let core = Arc::new(Core::<App>::new());
    let (view, set_view) = signal(core.view());

    let send: Dispatch = {
        let core = core.clone();
        Arc::new(move |action: Action| {
            let effects = core.process_event(action);
            drive(&core, set_view, effects);
        })
    };

    // Fire Start, exactly like the native shell, so the core fetches initial data.
    send(Action::Start);

    let send_for_view = send.clone();
    view! {
        <div class="app">
            {move || render(&view.get(), &send_for_view)}
        </div>
    }
}

/// Process effects: re-read the view on Render; fulfil HTTP via fetch and resolve.
fn drive(core: &Arc<Core<App>>, set_view: WriteSignal<Widget>, effects: Vec<Effect>) {
    for effect in effects {
        match effect {
            Effect::Render(_) => set_view.set(core.view()),
            // todo-core uses no fire-and-forget capabilities.
            Effect::PluginNotify(_) => {}
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

/// Fulfil the `http` capability with `fetch`.
async fn perform(call: &PluginCall) -> PluginResponse {
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

// ---------------- Widget → DOM ----------------

fn render(widget: &Widget, send: &Dispatch) -> AnyView {
    match widget {
        Widget::Scaffold { title, body, .. } => view! {
            <div class="scaffold">
                <div class="topbar">{title.clone()}</div>
                <div class="col">{render(body, send)}</div>
            </div>
        }
        .into_any(),

        Widget::Column { children } => {
            let kids: Vec<AnyView> = children.iter().map(|c| render(c, send)).collect();
            view! { <div class="col">{kids}</div> }.into_any()
        }
        Widget::Row { children } => {
            let kids: Vec<AnyView> = children.iter().map(|c| render(c, send)).collect();
            view! { <div class="row">{kids}</div> }.into_any()
        }
        Widget::Card { child, .. } => {
            view! { <div class="card">{render(child, send)}</div> }.into_any()
        }

        Widget::Text { content, style } => {
            let class = match style {
                TextStyle::Title => "t-title",
                TextStyle::Subtitle => "t-subtitle",
                TextStyle::Caption => "t-caption",
                _ => "t-body",
            };
            let content = content.clone();
            view! { <p class=class>{content}</p> }.into_any()
        }
        Widget::Badge { label, .. } => {
            let label = label.clone();
            view! { <span class="badge">{label}</span> }.into_any()
        }
        Widget::Spacer { .. } => view! { <div class="spacer"></div> }.into_any(),

        Widget::Button { label, on_press, style } => {
            let send = send.clone();
            let token = on_press.clone();
            let label = label.clone();
            let class = if matches!(style, ButtonStyle::Filled) { "btn" } else { "btn" };
            view! {
                <button class=class on:click=move |_| send(Action::Fired { token: token.clone() })>
                    {label}
                </button>
            }
            .into_any()
        }
        Widget::IconButton { icon, on_press } => {
            let send = send.clone();
            let token = on_press.clone();
            let glyph = match icon {
                Icon::Delete => "🗑",
                Icon::Add => "＋",
                Icon::Check => "✓",
                Icon::Star => "★",
                _ => "•",
            };
            view! {
                <button class="iconbtn" on:click=move |_| send(Action::Fired { token: token.clone() })>
                    {glyph}
                </button>
            }
            .into_any()
        }
        Widget::TextField { id, placeholder, value } => {
            let send = send.clone();
            let id = id.clone();
            let placeholder = placeholder.clone();
            let value = value.clone();
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
        Widget::Checkbox { id, label, value } => {
            let send = send.clone();
            let id = id.clone();
            let label = label.clone();
            let checked = *value;
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

        other => {
            let note = format!("[unhandled widget: {other:?}]");
            view! { <div class="unhandled">{note}</div> }.into_any()
        }
    }
}
