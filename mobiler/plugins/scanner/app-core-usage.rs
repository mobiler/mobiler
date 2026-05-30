// Rust app-side usage of the scanner plugin — drop into your `shared/src/app.rs`.
// No mobiler-core change: `cx.plugin` is the generic by-name escape hatch.

use mobiler_core::*;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone)]
pub enum Msg {
    // …your other events…
    ScanPressed,
    Scanned(PluginResponse),
}

#[derive(Default, Serialize, Deserialize)]
pub struct Model {
    pub last_code: Option<String>,
    pub banner: Option<String>,
}

impl MyApp {
    fn handle(&self, msg: Msg, model: &mut Model, cx: &mut Cx<Msg>) {
        match msg {
            // Open the scanner. Returns the first code as "<format>:<value>".
            Msg::ScanPressed => cx.plugin("scanner", "scan", "", Msg::Scanned),
            Msg::Scanned(resp) => {
                if resp.ok {
                    // resp.output == "qr:https://example.com" or "ean13:9781234567897", etc.
                    match resp.output.split_once(':') {
                        Some(("ean13", code)) | Some(("ean8", code)) | Some(("upca", code)) => {
                            // a product barcode → look it up
                            model.last_code = Some(code.to_string());
                        }
                        Some(("qr", payload)) => {
                            // a QR code → could be a URL, a table token, anything
                            model.last_code = Some(payload.to_string());
                        }
                        Some((_, value)) => model.last_code = Some(value.to_string()),
                        None => model.last_code = Some(resp.output.clone()),
                    }
                } else {
                    // cancelled / no camera (e.g. simulator) / permission denied
                    model.banner = Some(format!("Scan didn't complete: {}", resp.output));
                }
            }
        }
    }
}
