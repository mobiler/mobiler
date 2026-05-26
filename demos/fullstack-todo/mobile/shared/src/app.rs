//! This app's logic lives in the reusable `todo-core` crate — the same core the
//! `web-widgets` shell renders. Here the native FFI just re-exports the target.

pub use todo_core::App;
