// Rust app-side usage of the geofence plugin (drop into shared/src/app.rs).
// Two surfaces: request/response ops (add/remove/list regions, significant-change on/off) and the
// `events` stream (cx.subscribe) that delivers enter/exit + coarse location, buffered launch-from-dead.

use mobiler_core::*;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone)]
pub enum Msg {
    EnableGeofence,
    GeoPerm(PluginResponse),
    GeoEvent(PluginResponse),
    Noop(PluginResponse),
}

#[derive(Default, Serialize, Deserialize)]
pub struct Model {
    pub last_event: Option<String>,
}

impl MyApp {
    fn handle(&self, msg: Msg, model: &mut Model, cx: &mut Cx<Msg>) {
        match msg {
            // Ask for Always location, register a region, and subscribe. Subscribe at startup in a
            // real app so an enter/exit that woke a dead process (buffered by the shell) isn't missed.
            Msg::EnableGeofence => {
                cx.plugin("geofence", "requestPermission", "", Msg::GeoPerm);
                cx.plugin(
                    "geofence",
                    "add",
                    r#"{"id":"shop","lat":47.3769,"lng":8.5417,"radius":150,"notify_title":"Nearby","notify_body":"You're at the shop"}"#,
                    Msg::Noop,
                );
                cx.subscribe("geofence", "geofence", "events", "", Msg::GeoEvent);
            }
            Msg::GeoPerm(_r) => {}
            // {"type":"geofence","id":"shop","event":"enter"|"exit"} or {"type":"location","lat":..,"lng":..}
            Msg::GeoEvent(r) => {
                if r.ok {
                    model.last_event = Some(r.output.clone());
                }
            }
            Msg::Noop(_r) => {}
        }
    }
}
