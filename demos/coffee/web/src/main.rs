//! Widget→DOM web client for the coffee demo — the SAME `shared` core that runs
//! natively on Android (Compose) and iOS (SwiftUI), rendered to the DOM by the
//! generic `mobiler-web` shell. That's the whole app: one line. No CSS required —
//! the shell ships its own theme.

fn main() {
    mobiler_web::run::<shared::App>();
}
