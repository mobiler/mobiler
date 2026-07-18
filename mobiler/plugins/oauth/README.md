# oauth — OAuth 2.0 / OIDC login (free, bundled)

```bash
mobiler plugin add oauth
```

```rust
// 1. Build the provider's authorize URL in your core (client_id, redirect_uri, scope, state, PKCE).
let url = format!(
    "https://accounts.example.com/authorize?response_type=code&client_id={CLIENT}\
     &redirect_uri={SCHEME}%3A%2F%2Foauth&scope=openid%20profile&state={state}\
     &code_challenge={challenge}&code_challenge_method=S256"
);
let input = serde_json::json!({ "url": url, "scheme": SCHEME }).to_string();
cx.plugin("oauth", "login", input, Msg::LoggedIn);

// 2. On success, r.as_text() is the redirect URL — parse `code`+`state`, then exchange via
//    cx.request/cx.post (POST the token endpoint) and store the tokens with the `securestore` plugin.
Msg::LoggedIn(r) => { if r.ok { /* parse code from r.as_text() */ } else { /* r.as_text() = "cancelled"/error */ } }
```

- `op` = `login`; `input` = JSON `{"url": "<authorize URL>", "scheme": "<callback scheme>"}`.
  `ok:true` → `output` is the full redirect URL (`"<scheme>://oauth?code=…&state=…"`); `ok:false`
  → `"cancelled"` (user dismissed) or an error message.
- **This plugin owns only the browser redirect step.** Token exchange is `cx.request`/`cx.post`; token storage
  is the `securestore` plugin; PKCE/`state` are built into the authorize URL by your core. See
  `app-core-usage.rs` for the full compose.
- **iOS:** `ASWebAuthenticationSession` (system `AuthenticationServices`) — a private, SSO-capable
  web session the OS dismisses on redirect; it intercepts `callbackURLScheme` itself, so **no
  Info.plist scheme registration is needed**. Any scheme works.
- **Android:** Chrome Custom Tabs (`androidx.browser`) launched from a self-contained `OAuthActivity`
  whose intent-filter catches the redirect on **`${applicationId}`**. So on Android the **redirect
  scheme must equal your application id** (for a different scheme, add your own `<intent-filter>`).
  Adds the `androidx.browser` Gradle dep.
- **Web:** graceful `ok:false` (`"plugin 'oauth' not available"`). Web OAuth (popup/redirect) is a
  planned `mobiler-web` enhancement.

**Redirect scheme:** Mobiler sets the **same bundle/application id on iOS and Android**, so use it as
your redirect scheme — register `"<your.app.id>://oauth"` (any path) as the redirect URI with your
provider and pass `scheme = "<your.app.id>"`. The same code then works on both platforms.

Test on **real hardware / a registered provider** (the browser session + redirect interception only
run on device; iOS via TestFlight). A quick self-contained sanity check of the redirect plumbing,
with no IdP: point `url` at `https://httpbin.org/redirect-to?url=<scheme>%3A%2F%2Foauth%3Fcode%3Dtest&status_code=302`
and confirm `output` comes back as `"<scheme>://oauth?code=test"`.
