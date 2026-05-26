//! Widget→DOM web client — the SAME `todo-core` the native app uses, rendered to
//! the DOM by the generic `mobiler-web` shell. That's the whole app: one line.
//! (Styling for the widget classes lives in `index.html`.)

fn main() {
    mobiler_web::run::<todo_core::App>();
}
