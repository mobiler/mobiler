//! Widget→DOM web client for the todo demo — the SAME `shared` core that runs
//! natively on Android (Compose) and iOS (SwiftUI), rendered to the DOM by the
//! generic `mobiler-web` shell. One line, no CSS (the shell ships its own theme).

fn main() {
    mobiler_web::run::<shared::App>();
}
