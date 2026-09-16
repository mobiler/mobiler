// Rust app-side usage of the push plugin (drop into shared/src/app.rs).
// Two surfaces: `register` (one-shot) returns the device token to POST to your backend; the
// `events` stream (cx.subscribe) delivers notification events + token rotations. Each notification
// event is its payload plus a reserved "mobiler_push" key: "opened" (the user tapped it — buffered
// until you subscribe, so a tap that launched the app arrives) or "received" (it arrived while the
// app was in the foreground — live only). Navigate on "opened"; on "received", refresh in place.

use mobiler_core::*;
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Serialize, Deserialize, Clone)]
pub enum Msg {
    EnablePush,
    PushToken(PluginResponse),
    PushEvent(PluginResponse),
    Posted(PluginResponse),
}

#[derive(Default, Serialize, Deserialize)]
pub struct Model {
    pub token: Option<String>,
    pub last_event: Option<String>,
}

impl MyApp {
    fn handle(&self, msg: Msg, model: &mut Model, cx: &mut Cx<Msg>) {
        match msg {
            // Ask the OS for the device token AND subscribe to inbound pushes. Subscribe at startup
            // in a real app so a tap that launched the app (buffered by the shell) isn't missed.
            Msg::EnablePush => {
                cx.plugin("push", "register", "", Msg::PushToken);
                cx.subscribe("push", "push", "events", "", Msg::PushEvent);
            }
            // register → {"token":"…","platform":"apns"|"fcm"}. POST it (with your tenant) to the backend.
            Msg::PushToken(r) => {
                if r.ok {
                    model.token = Some(r.output.clone());
                    cx.post("https://api.example.com/devices", r.output, Msg::Posted);
                }
            }
            // A tagged notification event, or {"type":"token_refresh","token":"…"} on rotation.
            Msg::PushEvent(r) => {
                if !r.ok {
                    return;
                }
                let text = r.as_text().unwrap_or_default().to_string();
                model.last_event = Some(text.clone());
                let Ok(event) = serde_json::from_str::<Value>(&text) else { return };
                match event["mobiler_push"].as_str() {
                    // The user tapped the notification → open the screen it is about.
                    Some("opened") => { /* e.g. navigate to event["booking_id"] */ }
                    // It arrived while the app was open → refresh in place; don't navigate uninvited.
                    Some("received") => { /* e.g. reload the list the push concerns */ }
                    // No tag: {"type":"token_refresh","token":"…"} → re-POST the token to the backend.
                    _ => {}
                }
            }
            Msg::Posted(_) => {}
        }
    }
}
