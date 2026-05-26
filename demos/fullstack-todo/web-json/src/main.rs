//! `web-json` — an idiomatic Leptos app with its OWN UI (TodoMVC-style, filters).
//!
//! The contrast to `web-widgets`: this shares only the `domain` crate (types +
//! intents) and talks the same plain-JSON API directly with `fetch`. There is no
//! crux core and no `Widget` tree — the *data layer* is reused, the UI is its own.

use domain::{NewTodo, Todo, TodoPatch, active_count};
use gloo_net::http::Request;
use leptos::prelude::*;
use wasm_bindgen_futures::spawn_local;

const API: &str = "http://localhost:3000/todos";

// ---- the shared JSON API, reached with fetch ----

async fn fetch_list() -> Vec<Todo> {
    match Request::get(API).send().await {
        Ok(resp) => resp.json::<Vec<Todo>>().await.unwrap_or_default(),
        Err(_) => Vec::new(),
    }
}

async fn create(text: String) -> Option<Todo> {
    Request::post(API)
        .json(&NewTodo { text })
        .ok()?
        .send()
        .await
        .ok()?
        .json::<Todo>()
        .await
        .ok()
}

async fn set_done(id: u32, done: bool) {
    if let Ok(req) = Request::patch(&format!("{API}/{id}")).json(&TodoPatch { done: Some(done), text: None }) {
        let _ = req.send().await;
    }
}

async fn remove(id: u32) {
    let _ = Request::delete(&format!("{API}/{id}")).send().await;
}

#[derive(Clone, Copy, PartialEq)]
enum Filter {
    All,
    Active,
    Done,
}

fn main() {
    console_error_panic_hook::set_once();
    leptos::mount::mount_to_body(app);
}

fn app() -> impl IntoView {
    let todos = RwSignal::new(Vec::<Todo>::new());
    let input = RwSignal::new(String::new());
    let filter = RwSignal::new(Filter::All);

    // Initial load.
    spawn_local(async move { todos.set(fetch_list().await); });

    let add = move || {
        let text = input.get().trim().to_string();
        if text.is_empty() {
            return;
        }
        input.set(String::new());
        spawn_local(async move {
            if let Some(todo) = create(text).await {
                todos.update(|v| v.push(todo));
            }
        });
    };

    let toggle = move |id: u32, done: bool| {
        todos.update(|v| {
            if let Some(t) = v.iter_mut().find(|t| t.id == id) {
                t.done = done;
            }
        });
        spawn_local(async move { set_done(id, done).await; });
    };

    let delete = move |id: u32| {
        todos.update(|v| v.retain(|t| t.id != id));
        spawn_local(async move { remove(id).await; });
    };

    let visible = move || {
        let f = filter.get();
        todos
            .get()
            .into_iter()
            .filter(move |t| match f {
                Filter::All => true,
                Filter::Active => !t.done,
                Filter::Done => t.done,
            })
            .collect::<Vec<_>>()
    };

    let pill = move |label: &'static str, f: Filter| {
        view! {
            <button
                class=move || if filter.get() == f { "pill on" } else { "pill" }
                on:click=move |_| filter.set(f)
            >
                {label}
            </button>
        }
    };

    view! {
        <div class="wrap">
            <h1>"Todos"</h1>
            <p class="sub">
                "Idiomatic Leptos app — its own UI, sharing only the domain crate, talking the same JSON API."
            </p>
            <div class="panel">
                <div class="new">
                    <input
                        placeholder="What needs doing?"
                        prop:value=move || input.get()
                        on:input=move |ev| input.set(event_target_value(&ev))
                        on:keydown=move |ev| { if ev.key() == "Enter" { add(); } }
                    />
                    <button on:click=move |_| add()>"Add"</button>
                </div>

                {move || {
                    if visible().is_empty() {
                        view! { <div class="empty">"Nothing here."</div> }.into_any()
                    } else {
                        view! {
                            <ul>
                                <For each=visible key=|t| (t.id, t.done) let:t>
                                    {
                                        let id = t.id;
                                        let done = t.done;
                                        view! {
                                            <li class=if done { "done" } else { "" }>
                                                <input
                                                    type="checkbox"
                                                    prop:checked=done
                                                    on:change=move |_| toggle(id, !done)
                                                />
                                                <span class="txt">{t.text.clone()}</span>
                                                <button class="del" on:click=move |_| delete(id)>"✕"</button>
                                            </li>
                                        }
                                    }
                                </For>
                            </ul>
                        }
                        .into_any()
                    }
                }}

                <div class="foot">
                    <span>{move || format!("{} left", active_count(&todos.get()))}</span>
                    <span class="pills">
                        {pill("All", Filter::All)}
                        {pill("Active", Filter::Active)}
                        {pill("Done", Filter::Done)}
                    </span>
                </div>
            </div>
        </div>
    }
}
