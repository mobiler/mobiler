//! Mobiler's fixed UI wire ABI.
//!
//! These types are the **stable contract** between any Mobiler app's Rust core
//! and the native shell. Because they never change per app, a single shell can
//! be built once and render *any* Mobiler app — the shell only ever knows these
//! types, never an app's domain events or widgets.
//!
//! - The core emits a [`Widget`] tree (the `ViewModel`).
//! - The shell sends back an [`Action`] (the `Event`).
//! - App domain events ride inside actions as opaque [`ActionToken`]s that the
//!   shell round-trips without interpreting.

use facet::Facet;
use serde::{Deserialize, Serialize};

/// An opaque, serialized app event (e.g. JSON of the app's domain action).
/// The shell carries it verbatim and echoes it back on activation.
pub type ActionToken = String;

/// A value produced by an input widget at runtime.
#[derive(Facet, Serialize, Deserialize, Clone, Debug)]
#[repr(C)]
pub enum InputValue {
    Text(String),
    Bool(bool),
    Int(i64),
}

/// What the shell sends back to the core. **Fixed across all apps.**
#[derive(Facet, Serialize, Deserialize, Clone, Debug)]
#[repr(C)]
pub enum Action {
    /// An action widget (button, etc.) fired; `token` is the opaque app event.
    Fired { token: ActionToken },
    /// A value-carrying input changed; `id` names the widget.
    Input { id: String, value: InputValue },
}

/// The app-agnostic widget tree the shell renders. **Fixed across all apps.**
///
/// (Prototype subset — the production ABI carries the full widget vocabulary.)
#[derive(Facet, Serialize, Deserialize, Clone, Debug)]
#[repr(C)]
pub enum Widget {
    Text { content: String },
    Column { children: Vec<Widget> },
    Button { label: String, on_press: ActionToken },
    TextField { id: String, placeholder: String, value: String },
}
