// Rust app-side usage of the analytics plugin (drop into shared/src/app.rs).
// Fire-and-forget product analytics + crash reporting via Firebase. Crash CAPTURE is automatic once
// Firebase is configured; you call logEvent / setUserId / setUserProperty as the user moves through the
// app, and (optionally) log breadcrumbs / record non-fatal errors for richer crash reports.

use mobiler_core::*;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone)]
pub enum Msg {
    SignedIn(String),
    BookingConfirmed,
    Noop(PluginResponse),
}

impl MyApp {
    fn handle(&self, msg: Msg, _model: &mut Model, cx: &mut Cx<Msg>) {
        match msg {
            // Identify the user + tag a property (segments your analytics + crash reports).
            Msg::SignedIn(user_id) => {
                cx.plugin("analytics", "setUserId", &user_id, Msg::Noop);
                cx.plugin("analytics", "setUserProperty", r#"{"name":"plan","value":"pro"}"#, Msg::Noop);
            }
            // Log a custom event with parameters.
            Msg::BookingConfirmed => {
                cx.plugin(
                    "analytics",
                    "logEvent",
                    r#"{"name":"booking_confirmed","params":{"service":"fade","value":35}}"#,
                    Msg::Noop,
                );
            }
            Msg::Noop(_r) => {}
        }
    }
}

// Privacy / consent: gate collection on the user's choice (toggles both Analytics + Crashlytics):
//   cx.plugin("analytics", "setEnabled", "false", Msg::Noop);   // opt out
// Crash reporting helpers (optional — auto-capture works without them):
//   cx.plugin("analytics", "log", "entered checkout", Msg::Noop);                  // breadcrumb
//   cx.plugin("analytics", "recordError", r#"{"message":"sync failed"}"#, Msg::Noop); // non-fatal
