// Rust app-side usage of the notifications plugin (drop into shared/src/app.rs).
// Local scheduled notifications that fire when the app is closed, with actions and an events stream.

use mobiler_core::*;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone)]
pub enum Msg {
    AppStarted,
    EnableReminders,
    NotifAllowed(PluginResponse),
    Replanned(PluginResponse),
    CancelReminder(i64),
    ShowPending,
    Pending(PluginResponse),
    NotifEvent(PluginResponse),
    Noop(PluginResponse),
}

/// One reminder the app wants on the phone. `at` is local wall-clock time.
struct Reminder {
    id: i64,
    title: String,
    body: String,
    at: String,
    appointment_id: String,
}

fn schedule_input(r: &Reminder) -> String {
    // serde_json escapes quotes, newlines and non-ASCII correctly; never build this by hand.
    serde_json::json!({
        "id": r.id,
        "title": r.title,
        "body": r.body,
        "at": r.at,                         // or "after_seconds": 2700 (exactly one of the two)
        "payload": r.appointment_id,        // comes back in events (up to 1024 bytes)
        "actions": [
            {"id": "snooze", "label": "Snooze 10 min", "kind": "snooze", "snooze_minutes": 10},
            {"id": "open", "label": "Open", "kind": "open"}
        ],
        "late_minutes": 30                  // a re-arm after reboot skips reminders older than 30 min
    })
    .to_string()
}

impl MyApp {
    fn handle(&self, msg: Msg, model: &mut Model, cx: &mut Cx<Msg>) {
        match msg {
            Msg::AppStarted => {
                // Listen first so a tap that launched the app is delivered (it is buffered natively).
                cx.subscribe("notif", "notifications", "events", "", Msg::NotifEvent);

                // Re-plan on every launch: replace the whole schedule with the next few days. This
                // also restores reminders after an Android force-stop. iOS holds at most 64 pending,
                // so plan ~10 a day for a few days. Calls are applied in order.
                cx.plugin("notifications", "cancelAll", "", Msg::Noop);
                for r in model.upcoming_reminders() {
                    cx.plugin("notifications", "schedule", schedule_input(&r), Msg::Replanned);
                }
            }

            // Ask once (e.g. when the user turns on reminders). `permissionStatus` never prompts.
            Msg::EnableReminders => cx.plugin("notifications", "requestPermission", "", Msg::NotifAllowed),
            Msg::NotifAllowed(resp) => {
                if !resp.ok {
                    // Denied: offer a button that calls `openSettings`.
                }
            }

            // Failures are "<code>" or "<code>: <detail>": invalid_input, past, pending_limit,
            // unsupported, failed.
            Msg::Replanned(resp) => {
                if !resp.ok && resp.output.starts_with("pending_limit") {
                    // Plan fewer days ahead.
                }
            }

            // The appointment was moved or cancelled.
            Msg::CancelReminder(id) => {
                cx.plugin("notifications", "cancel", format!(r#"{{"id":{id}}}"#), Msg::Noop);
            }

            Msg::ShowPending => cx.plugin("notifications", "listPending", "", Msg::Pending),
            Msg::Pending(resp) => {
                // [{"id":7,"fire_at":"2026-12-01 09:45:00","snoozed":false}]
                model.pending_json = resp.output;
            }

            Msg::NotifEvent(resp) => {
                // {"type":"opened"|"snoozed"|"dismissed"|"action","id":7,"action_id"?:"open",
                //  "payload"?:"...","at":"2026-12-01 09:45:12"}. At-least-once: handle repeats.
                if let Ok(ev) = serde_json::from_str::<serde_json::Value>(&resp.output) {
                    if ev["type"] == "opened" {
                        // The payload is untrusted (another app can launch yours with crafted
                        // extras): look the appointment up in your own data, never trust it blindly.
                        if let Some(p) = ev["payload"].as_str() {
                            model.open_appointment_if_exists(p);
                        }
                    }
                }
            }

            Msg::Noop(_) => {}
        }
    }
}
