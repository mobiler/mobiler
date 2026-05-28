//! Shared domain types — the wire contract between the server and every client.

use serde::{Deserialize, Serialize};

/// A note. `id` is assigned by the server.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Note {
    pub id: i64,
    pub title: String,
    pub body: String,
}

/// Payload to create a note (the server assigns the `id`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NewNote {
    pub title: String,
    pub body: String,
}
