// Rust app-side usage of the notifications plugin (drop into shared/src/app.rs).
// Local scheduled notifications — appointment/reminder alerts that fire when the app is closed.

use mobiler_core::*;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone)]
pub enum Msg {
    EnableReminders,
    NotifAllowed(PluginResponse),
    ScheduleReminder { id: i64, title: String, body: String, after_seconds: i64 },
    Scheduled(PluginResponse),
    CancelReminder(i64),
    Noop(PluginResponse),
}

impl MyApp {
    fn handle(&self, msg: Msg, model: &mut Model, cx: &mut Cx<Msg>) {
        match msg {
            // Ask once (e.g. when the user turns on reminders).
            Msg::EnableReminders => cx.plugin("notifications", "requestPermission", "", Msg::NotifAllowed),
            Msg::NotifAllowed(_resp) => { /* resp.ok = allowed; update UI / store the preference */ }

            // Schedule an appointment reminder. `after_seconds` from now (compute it from the
            // appointment time minus the lead time, e.g. 1h before).
            Msg::ScheduleReminder { id, title, body, after_seconds } => {
                let input = format!(
                    r#"{{"id":{id},"title":"{title}","body":"{body}","after_seconds":{after_seconds}}}"#
                );
                cx.plugin("notifications", "schedule", input, Msg::Scheduled);
            }
            Msg::Scheduled(_resp) => { /* resp.ok = scheduled */ }

            // Cancel a reminder (e.g. the appointment was moved/cancelled).
            Msg::CancelReminder(id) => {
                cx.plugin("notifications", "cancel", format!(r#"{{"id":{id}}}"#), Msg::Noop);
            }
            Msg::Noop(_) => {}
        }
    }
}
