// Rust app-side usage of the biometric + securestore plugins (drop into shared/src/app.rs).
// No mobiler-core change — cx.plugin is the generic by-name escape hatch. The "biometric-gated
// secret read" is composed in app logic: authenticate, then (on success) read the secret.

use mobiler_core::*;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone)]
pub enum Msg {
    UnlockPressed,
    Authed(PluginResponse),   // biometric result
    GotToken(PluginResponse), // securestore get result
    SaveToken(String),
    Stored(PluginResponse),
}

#[derive(Default, Serialize, Deserialize)]
pub struct Model {
    pub unlocked: bool,
    pub token: Option<String>,
    pub status: Option<String>,
}

impl MyApp {
    fn handle(&self, msg: Msg, model: &mut Model, cx: &mut Cx<Msg>) {
        match msg {
            // Authenticate, then (only on success) read the protected token from secure storage.
            Msg::UnlockPressed => cx.plugin("biometric", "authenticate", "Unlock your account", Msg::Authed),
            Msg::Authed(resp) => {
                if resp.ok {
                    model.unlocked = true;
                    cx.plugin("securestore", "get", r#"{"key":"auth_token"}"#, Msg::GotToken);
                } else {
                    model.status = Some(format!("Auth failed: {}", resp.output));
                }
            }
            Msg::GotToken(resp) => {
                if resp.ok && !resp.output.is_empty() {
                    model.token = Some(resp.output);
                } else {
                    model.status = Some("No token stored yet — log in once.".into());
                }
            }

            // Persist a secret (e.g. after a login round-trip).
            Msg::SaveToken(value) => {
                let input = format!(r#"{{"key":"auth_token","value":"{value}"}}"#);
                cx.plugin("securestore", "set", input, Msg::Stored);
            }
            Msg::Stored(resp) => {
                model.status = Some(if resp.ok { "Saved securely ✓".into() } else { format!("Save failed: {}", resp.output) });
            }
        }
    }
}
