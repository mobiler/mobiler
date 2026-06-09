// Rust app-side usage of the geolocation-fused plugin (drop into shared/src/app.rs).
// Identical to the `geolocation` plugin — it registers under the same cx name "geolocation"; only the
// Android implementation differs (Play Services FusedLocationProvider for higher accuracy).

use mobiler_core::*;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone)]
pub enum Msg {
    FindMe,
    GotLocation(PluginResponse),
}

#[derive(Default, Serialize, Deserialize)]
pub struct Model {
    pub location: String,
}

impl MyApp {
    fn handle(&self, msg: Msg, model: &mut Model, cx: &mut Cx<Msg>) {
        match msg {
            Msg::FindMe => cx.plugin("geolocation", "get", "", Msg::GotLocation),
            // r.output = "lat,lng" (e.g. "47.3769,8.5417"); r.ok = false on denial / no Play Services.
            Msg::GotLocation(r) => {
                if r.ok {
                    model.location = r.output.clone();
                }
            }
        }
    }
}
