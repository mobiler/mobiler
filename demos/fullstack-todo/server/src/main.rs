//! Axum backend for the full-stack todo demo. In-memory store (no database), one
//! plain-JSON REST surface that **both** clients use symmetrically — the Mobiler
//! core (mobile + Widget→DOM web) via its HTTP capability, and the Leptos web app
//! via fetch. Every request/response type comes from the shared `domain` crate.

use std::sync::{Arc, Mutex};

use axum::{
    Json, Router,
    extract::{Path, State},
    http::StatusCode,
    routing::{get, patch},
};
use domain::{NewTodo, Todo, TodoPatch};
use tower_http::cors::CorsLayer;

#[derive(Default)]
struct Store {
    todos: Vec<Todo>,
    next_id: u32,
}

type AppState = Arc<Mutex<Store>>;

#[tokio::main]
async fn main() {
    let mut store = Store { todos: Vec::new(), next_id: 1 };
    for text in ["Try Mobiler", "Build the Widget→DOM web shell", "Ship full-stack todo"] {
        let id = store.next_id;
        store.next_id += 1;
        store.todos.push(Todo { id, text: text.into(), done: false });
    }
    let state: AppState = Arc::new(Mutex::new(store));

    let app = Router::new()
        .route("/todos", get(list).post(create))
        .route("/todos/{id}", patch(update).delete(remove))
        // Permissive CORS so the browser-served web app can call the API.
        .layer(CorsLayer::permissive())
        .with_state(state);

    let listener = tokio::net::TcpListener::bind("0.0.0.0:3000").await.unwrap();
    println!("fullstack-todo server listening on http://0.0.0.0:3000");
    axum::serve(listener, app).await.unwrap();
}

async fn list(State(s): State<AppState>) -> Json<Vec<Todo>> {
    Json(s.lock().unwrap().todos.clone())
}

async fn create(State(s): State<AppState>, Json(new): Json<NewTodo>) -> Json<Todo> {
    let mut s = s.lock().unwrap();
    let id = s.next_id;
    s.next_id += 1;
    let todo = Todo { id, text: new.text, done: false };
    s.todos.push(todo.clone());
    Json(todo)
}

async fn update(
    State(s): State<AppState>,
    Path(id): Path<u32>,
    Json(patch): Json<TodoPatch>,
) -> Result<Json<Todo>, StatusCode> {
    let mut s = s.lock().unwrap();
    let todo = s.todos.iter_mut().find(|t| t.id == id).ok_or(StatusCode::NOT_FOUND)?;
    if let Some(done) = patch.done {
        todo.done = done;
    }
    if let Some(text) = patch.text {
        todo.text = text;
    }
    Ok(Json(todo.clone()))
}

async fn remove(State(s): State<AppState>, Path(id): Path<u32>) -> StatusCode {
    let mut s = s.lock().unwrap();
    let before = s.todos.len();
    s.todos.retain(|t| t.id != id);
    if s.todos.len() < before { StatusCode::NO_CONTENT } else { StatusCode::NOT_FOUND }
}
