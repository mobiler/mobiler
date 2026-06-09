// Rust app-side usage of the background-fetch plugin (drop into shared/src/app.rs).
// Schedule a periodic OS wake; the `events` stream delivers a {"type":"fetch","id":..} each time the
// OS runs it (buffered launch-from-dead). Timing is OS-controlled and best-effort.

use mobiler_core::*;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone)]
pub enum Msg {
    EnableFetch,
    Fetch(PluginResponse),
    Noop(PluginResponse),
}

#[derive(Default, Serialize, Deserialize)]
pub struct Model {
    pub fetches: u32,
}

impl MyApp {
    fn handle(&self, msg: Msg, model: &mut Model, cx: &mut Cx<Msg>) {
        match msg {
            // Schedule the wake AND subscribe. Subscribe at startup in a real app so a wake that ran
            // while the app was dead (buffered by the shell) isn't missed.
            Msg::EnableFetch => {
                cx.plugin(
                    "background-fetch",
                    "schedule",
                    r#"{"id":"refresh","min_interval_seconds":3600,"notify_title":"Updated","notify_body":"New data ready"}"#,
                    Msg::Noop,
                );
                cx.subscribe("background-fetch", "background-fetch", "events", "", Msg::Fetch);
            }
            // {"type":"fetch","id":"refresh"} — re-fetch your data here.
            Msg::Fetch(r) => {
                if r.ok {
                    model.fetches += 1;
                }
            }
            Msg::Noop(_r) => {}
        }
    }
}
