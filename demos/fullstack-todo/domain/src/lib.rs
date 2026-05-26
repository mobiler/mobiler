//! Shared domain for the full-stack todo demo.
//!
//! This crate is the **single source of truth** for the data model and the API
//! contract, reused by every surface: the Axum server, the Mobiler core (mobile +
//! the Widget→DOM web shell), and the Leptos web app. It is **WASM-clean** — pure
//! types + pure logic, no IO, no `tokio`, no platform deps — so it links into the
//! browser bundle, the mobile core, and the server alike.

use serde::{Deserialize, Serialize};

/// A todo item — the resource the API exposes.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct Todo {
    pub id: u32,
    pub text: String,
    pub done: bool,
}

// ---- API intents (request bodies). The contract = intent in / `Todo` out. ----

/// `POST /todos` — create a todo.
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct NewTodo {
    pub text: String,
}

/// `PATCH /todos/{id}` — partial update (toggle done and/or edit text).
#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct TodoPatch {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub done: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
}

// ---- pure logic shared by all surfaces ----

/// How many todos are still open. Pure — usable on server and every client.
#[must_use]
pub fn active_count(todos: &[Todo]) -> usize {
    todos.iter().filter(|t| !t.done).count()
}

/// The API base path (clients build URLs from this, never hand-rolled strings).
pub const TODOS_PATH: &str = "/todos";
