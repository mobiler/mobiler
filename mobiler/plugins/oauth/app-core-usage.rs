// Rust app-side usage of the oauth + http + securestore plugins (drop into shared/src/app.rs).
// No mobiler-core change — cx.plugin is the generic by-name escape hatch. The full login is
// composed in app logic: open the auth browser (oauth) → exchange the code for tokens (http) →
// persist the tokens (securestore). PKCE/state are built into the authorize URL here.

use mobiler_core::*;
use serde::{Deserialize, Serialize};

// Your provider + app config.
const AUTHORIZE: &str = "https://accounts.example.com/authorize";
const TOKEN_ENDPOINT: &str = "https://accounts.example.com/token";
const CLIENT_ID: &str = "your-client-id";
const SCHEME: &str = "dev.test.smoke"; // = your app's bundle/application id; redirect = "<SCHEME>://oauth"

#[derive(Serialize, Deserialize, Clone)]
pub enum Msg {
    LoginPressed,
    LoggedIn(PluginResponse), // oauth result: output = redirect URL
    Exchanged(PluginResponse), // http token-exchange result: output = token JSON
    Stored(PluginResponse),
}

#[derive(Default, Serialize, Deserialize)]
pub struct Model {
    pub signed_in: bool,
    pub status: Option<String>,
    // In a real app, hold the PKCE verifier + state you generated for the in-flight request.
    pub state: String,
    pub verifier: String,
}

impl MyApp {
    fn handle(&self, msg: Msg, model: &mut Model, cx: &mut Cx<Msg>) {
        match msg {
            // 1. Open the system auth browser. (Generate `state` + PKCE `verifier`/`challenge`
            //    first and stash them in the model; here they're assumed already set.)
            Msg::LoginPressed => {
                let redirect = format!("{SCHEME}://oauth");
                let url = format!(
                    "{AUTHORIZE}?response_type=code&client_id={CLIENT_ID}\
                     &redirect_uri={redirect}&scope=openid%20profile&state={}\
                     &code_challenge={}&code_challenge_method=S256",
                    model.state, /* challenge = */ model.verifier // (challenge = S256(verifier) in practice)
                );
                let input = serde_json::json!({ "url": url, "scheme": SCHEME }).to_string();
                cx.plugin("oauth", "login", input, Msg::LoggedIn);
            }

            // 2. The browser redirected back. Parse `code` (+ verify `state`) from the URL, then
            //    POST it to the token endpoint via http.
            Msg::LoggedIn(r) => {
                if !r.ok {
                    model.status = Some(format!("Login cancelled/failed: {}", r.output));
                    return;
                }
                let Some(code) = query_param(&r.output, "code") else {
                    model.status = Some("No authorization code in redirect.".into());
                    return;
                };
                // (Verify query_param(&r.output, "state") == model.state before continuing.)
                let body = format!(
                    "grant_type=authorization_code&code={code}&client_id={CLIENT_ID}\
                     &redirect_uri={SCHEME}://oauth&code_verifier={}",
                    model.verifier
                );
                // Form-encoded token exchange (adjust to your provider's expectations).
                cx.post(TOKEN_ENDPOINT, body, Msg::Exchanged);
            }

            // 3. Persist the tokens securely (Keychain / Keystore).
            Msg::Exchanged(r) => {
                if !r.ok {
                    model.status = Some(format!("Token exchange failed: {}", r.output));
                    return;
                }
                // r.output is the token JSON; store it (or just the access/refresh tokens).
                let input = serde_json::json!({ "key": "oauth_tokens", "value": r.output }).to_string();
                cx.plugin("securestore", "set", input, Msg::Stored);
            }
            Msg::Stored(r) => {
                model.signed_in = r.ok;
                model.status = Some(if r.ok { "Signed in ✓".into() } else { format!("Save failed: {}", r.output) });
            }
        }
    }
}

/// Pull a query parameter out of a redirect URL (tiny, dependency-free).
fn query_param(url: &str, key: &str) -> Option<String> {
    let query = url.split_once('?').map(|(_, q)| q)?;
    query.split('&').find_map(|pair| {
        let (k, v) = pair.split_once('=')?;
        (k == key).then(|| v.to_string())
    })
}
