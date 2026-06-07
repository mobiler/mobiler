// Rust app-side usage of the push plugin (drop into shared/src/app.rs).
// Two surfaces: `register` (one-shot) returns the device token to POST to your backend; the
// `events` stream (cx.subscribe) delivers each received/tapped notification + token rotations.

use mobiler_core::*;
use serde::{Deserialize, Serialize};

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
            // Each received/tapped notification, or {"type":"token_refresh","token":"…"} on rotation.
            Msg::PushEvent(r) => {
                if r.ok {
                    model.last_event = Some(r.output.clone());
                    // e.g. parse r.output, route to the relevant screen on a tap.
                }
            }
            Msg::Posted(_) => {}
        }
    }
}
