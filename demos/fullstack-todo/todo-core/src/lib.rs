//! The full-stack todo **app core** — a `MobilerApp` whose data lives on the Axum
//! server, reached through the HTTP capability (`cx.get`/`post`/`patch`/`delete`).
//!
//! This crate is **renderer-agnostic**: it produces a `Widget` tree and is reused
//! verbatim by the native `mobile` shell *and* the `web-widgets` WASM shell. Only
//! the API base URL differs per platform (`cfg(target_arch)`).

use domain::{NewTodo, Todo, TodoPatch, TODOS_PATH, active_count};
use mobiler_core::{
    ButtonStyle, CardStyle, Cx, Icon, InputValue, MobilerApp, MobilerShell, PluginResponse,
    Spacing, Tone, Widget, badge, button, caption, card, checkbox, column, icon_button, row,
    scaffold, spacer, text, text_field, title,
};
use serde::{Deserialize, Serialize};

// The API base differs per platform: the Android emulator reaches the host
// machine via the special alias 10.0.2.2; the browser (WASM) and the iOS
// simulator both reach it via localhost (the sim shares the host's network).
#[cfg(any(target_arch = "wasm32", target_os = "ios"))]
const BASE: &str = "http://localhost:3000";
#[cfg(not(any(target_arch = "wasm32", target_os = "ios")))]
const BASE: &str = "http://10.0.2.2:3000";

fn todos_url() -> String {
    format!("{BASE}{TODOS_PATH}")
}
fn todo_url(id: u32) -> String {
    format!("{BASE}{TODOS_PATH}/{id}")
}

#[derive(Default)]
pub struct TodoApp;

/// Typed events. `Loaded`/`Failed`/`Reload` are produced by HTTP continuations;
/// `Add`/`Delete` by the UI.
#[derive(Serialize, Deserialize, Clone, Debug)]
pub enum Msg {
    Loaded(Vec<Todo>),
    Failed(String),
    Reload,
    Add,
    Delete(u32),
}

#[derive(Default)]
pub struct Model {
    todos: Vec<Todo>,
    input: String,
    error: Option<String>,
    loading: bool,
}

/// GET the list; the continuation parses it into a typed event.
fn load(cx: &mut Cx<Msg>) {
    cx.get(todos_url(), |resp| parse_list(&resp));
}

/// Continuation for mutations (POST/PATCH/DELETE): on success, refetch the list.
fn after_mutation(resp: PluginResponse) -> Msg {
    if resp.ok { Msg::Reload } else { Msg::Failed(err_of(&resp)) }
}

fn parse_list(resp: &PluginResponse) -> Msg {
    if !resp.ok {
        return Msg::Failed(err_of(resp));
    }
    match serde_json::from_str::<Vec<Todo>>(&resp.output) {
        Ok(todos) => Msg::Loaded(todos),
        Err(e) => Msg::Failed(format!("parse error: {e}")),
    }
}

fn err_of(resp: &PluginResponse) -> String {
    if resp.output.is_empty() { "request failed".to_string() } else { resp.output.clone() }
}

impl MobilerApp for TodoApp {
    type Event = Msg;
    type Model = Model;

    fn init(&self, model: &mut Model, cx: &mut Cx<Msg>) {
        model.loading = true;
        load(cx);
    }

    fn update(&self, event: Msg, model: &mut Model, cx: &mut Cx<Msg>) {
        match event {
            Msg::Loaded(todos) => {
                model.todos = todos;
                model.loading = false;
                model.error = None;
            }
            Msg::Failed(e) => {
                model.error = Some(e);
                model.loading = false;
            }
            Msg::Reload => load(cx),
            Msg::Add => {
                let text = model.input.trim().to_string();
                if !text.is_empty() {
                    model.input.clear();
                    let body = serde_json::to_string(&NewTodo { text }).unwrap_or_default();
                    cx.post(todos_url(), body, after_mutation);
                }
            }
            Msg::Delete(id) => cx.delete(todo_url(id), after_mutation),
        }
    }

    fn input(&self, id: &str, value: InputValue, model: &mut Model, cx: &mut Cx<Msg>) {
        match value {
            InputValue::Text(v) if id == "new" => model.input = v,
            InputValue::Bool(done) => {
                if let Some(tid) = id.strip_prefix("done:").and_then(|r| r.parse::<u32>().ok()) {
                    // Optimistic local flip so the UI responds instantly; PATCH the
                    // server and refetch to reconcile.
                    if let Some(t) = model.todos.iter_mut().find(|t| t.id == tid) {
                        t.done = done;
                    }
                    let body = serde_json::to_string(&TodoPatch { done: Some(done), text: None })
                        .unwrap_or_default();
                    cx.patch(todo_url(tid), body, after_mutation);
                }
            }
            _ => {}
        }
    }

    fn view(&self, model: &Model) -> Widget {
        let mut kids = vec![
            title("Todos"),
            caption("Backed by an Axum server — the same Rust core renders this natively and on the web."),
            spacer(Spacing::Sm),
            row(vec![
                text_field("new", "Add a todo…", model.input.clone()),
                button("Add", ButtonStyle::Filled, Msg::Add),
            ]),
            spacer(Spacing::Md),
        ];

        if let Some(err) = &model.error {
            kids.push(card(
                column(vec![
                    text(format!("⚠ {err}")),
                    caption("Is the server running?  cargo run -p server"),
                ]),
                CardStyle::Outlined,
            ));
            kids.push(spacer(Spacing::Sm));
        }

        if model.todos.is_empty() {
            let msg = if model.loading { "Loading…" } else { "No todos yet — add one above." };
            kids.push(card(text(msg), CardStyle::Outlined));
        } else {
            for t in &model.todos {
                kids.push(card(
                    row(vec![
                        checkbox(format!("done:{}", t.id), t.text.clone(), t.done),
                        icon_button(Icon::Delete, Msg::Delete(t.id)),
                    ]),
                    CardStyle::Elevated,
                ));
            }
            let left = active_count(&model.todos);
            kids.push(spacer(Spacing::Sm));
            let (label, tone) = if left == 0 {
                ("All done!".to_string(), Tone::Success)
            } else {
                (format!("{left} left"), Tone::Info)
            };
            kids.push(row(vec![badge(label, tone)]));
        }

        scaffold("Todos", false, vec![], column(kids))
    }
}

/// The Crux app the FFI + WASM shells target.
pub type App = MobilerShell<TodoApp>;

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn loaded_replaces_todos_and_clears_loading() {
        let app = TodoApp;
        let mut m = Model { loading: true, ..Model::default() };
        app.update(
            Msg::Loaded(vec![Todo { id: 1, text: "a".into(), done: false }]),
            &mut m,
            &mut Cx::default(),
        );
        assert_eq!(m.todos.len(), 1);
        assert!(!m.loading);
    }

    #[test]
    fn typing_updates_input() {
        let app = TodoApp;
        let mut m = Model::default();
        app.input("new", InputValue::Text("milk".into()), &mut m, &mut Cx::default());
        assert_eq!(m.input, "milk");
    }
}
