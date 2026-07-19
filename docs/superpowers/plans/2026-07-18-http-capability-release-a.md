# Mobiler HTTP Capability — Release A Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Give mobiler's HTTP capability request headers, every HTTP verb, and real response status codes, so the Stoa IM app can make authenticated calls and tell a 409 Conflict from a dead network.

**Architecture:** The fixed plugin ABI (`{plugin, op, input} -> {ok, output}`) stays. HTTP richness moves *inside* the payload as a bincode-encoded `HttpOutcome` sum type, so no HTTP-specific columns land on the struct shared by all 30 plugins. One field changes type — `PluginResponse.output` from `String` to `Vec<u8>` — which is what earns the ABI break, because a `String` body cannot carry binary at all.

**Tech Stack:** Rust (crux_core 0.18, facet, serde, bincode), Leptos/WASM + gloo-net 0.6 (web shell), Swift/URLSession (iOS shell), Kotlin/OkHttp (Android shell).

**Spec:** `docs/superpowers/specs/2026-07-18-mobiler-http-capability-a-design.md`

## Global Constraints

- `mobiler-ui` is **NOT** touched and **NOT** republished. HTTP is not a UI concern. Precedent: the C5 streaming release bumped core and web while ui stayed at 0.15.
- `PluginCall { plugin, op, input: String }` is **unchanged**. Only `PluginResponse.output` changes type.
- `status` must **NOT** become a field on `PluginResponse`. HTTP semantics live inside the payload.
- Facet enums require `#[repr(C)]` (cf. `Widget`, `mobiler-ui/src/lib.rs:434`).
- No tuples in shared types — use named structs (cf. `MapMarker`, `mobiler-ui/src/lib.rs:423`).
- Two-stage release. **PR-A** = libs + demos + version bumps, then `release-libs`. **PR-C** = template arms + core pin + CLI bump, then `release-cli`.
- Versions: `mobiler-core` 0.31.0 → **0.32.0**, `mobiler-web` 0.31.0 → **0.32.0**, `mobiler` CLI 0.47.0 → **0.48.0**, tag **v0.48.0**.
- Land every PR with the `ship-pr` skill. Run `post-release` after publishing. Run `cleanup-after-phase` when done.
- `main` is protected — always branch.

## File Structure

**PR-A — libraries and demos**

| File | Responsibility |
|---|---|
| `mobiler-core/src/http.rs` *(new)* | `HttpHeader`, `HttpOutcome`, accessors, `RequestBuilder`. Keeps `lib.rs` from growing further — it is already ~1400 lines. |
| `mobiler-core/src/lib.rs:84-88` | `PluginResponse.output` → `Vec<u8>`; add `PluginResponse::text()` helper. |
| `mobiler-core/src/lib.rs:187-222` | Replace `http`/`get`/`post`/`patch`/`delete`; add `put`, `request`. |
| `mobiler-web/src/lib.rs:416-441` | Web HTTP shell: exhaustive verbs, headers, binary body, `TransportError`. |
| 6 × `codegen.rs` | Register `HttpOutcome` explicitly. |
| 5 × `Core.swift`, 5 × `Core.kt` (demos) | Shell HTTP plugin + convenience constructor. |
| `demos/fullstack-sqlx/app-core/src/lib.rs`, `demos/fullstack-todo/todo-core/src/lib.rs` | Update ~8 call sites to `HttpOutcome`. |

**PR-C — CLI templates**

| File | Responsibility |
|---|---|
| `mobiler/templates/iOS/Sources/Core.swift` | Template iOS arm (byte-identical to the demo copies). |
| `mobiler/templates/Android/app/src/main/java/__PACKAGE_PATH__/Core.kt` | Template Android arm. |
| `mobiler/templates/shared/src/bin/codegen.rs` | Register `HttpOutcome`. |
| `mobiler/Cargo.toml`, template `Cargo.toml`s | Core pin → 0.32, CLI → 0.48.0. |

---

# PR-A — libraries and demos

### Task 1: `HttpHeader` and `HttpOutcome` types

**Files:**
- Create: `mobiler-core/src/http.rs`
- Modify: `mobiler-core/src/lib.rs` (add `pub mod http;` and re-export)
- Test: inline `#[cfg(test)]` in `mobiler-core/src/http.rs`

**Interfaces:**
- Consumes: nothing.
- Produces: `HttpHeader { name: String, value: String }`; `HttpOutcome::Response { status: u16, headers: Vec<HttpHeader>, body: Vec<u8> }`; `HttpOutcome::TransportError { message: String }`; accessors `status() -> Option<u16>`, `is_success() -> bool`, `body() -> &[u8]`, `text() -> Option<&str>`, `header(&str) -> Option<&str>`.

- [ ] **Step 1: Write the failing test**

Create `mobiler-core/src/http.rs` with only the test module:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    fn resp(status: u16, body: &str) -> HttpOutcome {
        HttpOutcome::Response {
            status,
            headers: vec![HttpHeader { name: "Content-Type".into(), value: "application/json".into() }],
            body: body.as_bytes().to_vec(),
        }
    }

    #[test]
    fn status_is_some_for_response_and_none_for_transport_error() {
        assert_eq!(resp(200, "").status(), Some(200));
        assert_eq!(HttpOutcome::TransportError { message: "offline".into() }.status(), None);
    }

    #[test]
    fn is_success_covers_exactly_2xx() {
        assert!(!resp(199, "").is_success());
        assert!(resp(200, "").is_success());
        assert!(resp(299, "").is_success());
        assert!(!resp(300, "").is_success());
        assert!(!resp(409, "").is_success());
        assert!(!HttpOutcome::TransportError { message: "x".into() }.is_success());
    }

    #[test]
    fn body_and_text_behave_on_valid_and_invalid_utf8() {
        assert_eq!(resp(200, "hi").body(), b"hi");
        assert_eq!(resp(200, "hi").text(), Some("hi"));

        let binary = HttpOutcome::Response { status: 200, headers: vec![], body: vec![0xff, 0xfe] };
        assert_eq!(binary.body(), &[0xff, 0xfe]);
        assert_eq!(binary.text(), None, "invalid UTF-8 must not panic or lossily convert");

        let err = HttpOutcome::TransportError { message: "x".into() };
        assert_eq!(err.body(), b"");
        assert_eq!(err.text(), None);
    }

    #[test]
    fn header_lookup_is_case_insensitive() {
        assert_eq!(resp(200, "").header("content-type"), Some("application/json"));
        assert_eq!(resp(200, "").header("CONTENT-TYPE"), Some("application/json"));
        assert_eq!(resp(200, "").header("missing"), None);
    }

    #[test]
    fn bincode_round_trips_both_variants() {
        for original in [
            resp(409, "conflict"),
            HttpOutcome::TransportError { message: "connection refused".into() },
        ] {
            let bytes = original.encode();
            assert_eq!(HttpOutcome::decode(&bytes).unwrap(), original);
        }
    }

    #[test]
    fn decode_rejects_garbage_without_panicking() {
        assert!(HttpOutcome::decode(&[0xff, 0xff, 0xff]).is_err());
    }
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p mobiler-core --lib http::`
Expected: FAIL — compile error, `HttpOutcome` not found.

- [ ] **Step 3: Write minimal implementation**

Prepend to `mobiler-core/src/http.rs`:

```rust
//! The HTTP capability's payload types.
//!
//! These ride *inside* `PluginResponse.output` as bincode, rather than as fields on
//! `PluginResponse` itself: that struct is shared by every plugin, and HTTP-specific
//! columns on it would be a domain leak into the fixed plugin ABI.

use facet::Facet;
use serde::{Deserialize, Serialize};

/// One HTTP header.
///
/// A named struct rather than a `(String, String)` tuple: the shared types contain no
/// tuples today, and tuple codegen into Swift/Kotlin is the least reliable corner of
/// serde-reflection.
#[derive(Facet, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[repr(C)]
pub struct HttpHeader {
    pub name: String,
    pub value: String,
}

/// The result of an HTTP request.
#[derive(Facet, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[repr(C)]
pub enum HttpOutcome {
    /// The server answered. `status` is the real HTTP status — 409 is distinguishable
    /// from 500.
    Response { status: u16, headers: Vec<HttpHeader>, body: Vec<u8> },
    /// No HTTP response was obtained, for any reason: chiefly network failure
    /// (offline, DNS, TLS, connection refused, timeout), but also a request that could
    /// not be attempted at all (unknown verb, malformed envelope). The defining
    /// property is that the server never answered, so no status exists.
    TransportError { message: String },
}

impl HttpOutcome {
    /// The HTTP status, or `None` when the server never answered.
    pub fn status(&self) -> Option<u16> {
        match self {
            Self::Response { status, .. } => Some(*status),
            Self::TransportError { .. } => None,
        }
    }

    /// True only for a 2xx response.
    pub fn is_success(&self) -> bool {
        matches!(self, Self::Response { status, .. } if (200..300).contains(status))
    }

    /// The raw response body; empty for a transport error.
    pub fn body(&self) -> &[u8] {
        match self {
            Self::Response { body, .. } => body,
            Self::TransportError { .. } => &[],
        }
    }

    /// The body as text, or `None` if it is not valid UTF-8 (or there is no body).
    pub fn text(&self) -> Option<&str> {
        std::str::from_utf8(self.body()).ok().filter(|_| matches!(self, Self::Response { .. }))
    }

    /// Look up a response header by name, case-insensitively.
    pub fn header(&self, name: &str) -> Option<&str> {
        match self {
            Self::Response { headers, .. } => headers
                .iter()
                .find(|h| h.name.eq_ignore_ascii_case(name))
                .map(|h| h.value.as_str()),
            Self::TransportError { .. } => None,
        }
    }

    /// Serialize for transport in `PluginResponse.output`.
    ///
    /// Uses crux's own FFI format rather than calling bincode directly. This is not
    /// incidental: crux pins bincode `=1.3` with `with_fixint_encoding()`, whereas
    /// bincode 2.x's `config::standard()` is *varint*. The two produce different
    /// bytes, and the Swift/Kotlin decoders generated by serde-generate expect crux's
    /// encoding — so hand-rolling this would silently yield garbage across the FFI.
    pub fn encode(&self) -> Vec<u8> {
        use crux_core::bridge::{BincodeFfiFormat, FfiFormat};
        let mut buffer = Vec::new();
        BincodeFfiFormat::serialize(&mut buffer, self).expect("encode HttpOutcome");
        buffer
    }

    /// Decode what a shell placed in `PluginResponse.output`.
    pub fn decode(bytes: &[u8]) -> Result<Self, String> {
        use crux_core::bridge::{BincodeFfiFormat, FfiFormat};
        BincodeFfiFormat::deserialize(bytes).map_err(|e| e.to_string())
    }
}
```

Add to `mobiler-core/src/lib.rs` beside the other `pub mod` lines (near line 10):

```rust
pub mod http;
pub use http::{HttpHeader, HttpOutcome};
```

**No new dependency is needed.** `mobiler-core` already depends on `crux_core`, and
`crux_core::bridge::{BincodeFfiFormat, FfiFormat}` are both public. Do **not** add
`bincode` directly — see the doc comment on `encode` above for why the version and
config matter.

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test -p mobiler-core --lib http::`
Expected: PASS, 5 tests.

- [ ] **Step 5: Commit**

```bash
git add mobiler-core/src/http.rs mobiler-core/src/lib.rs mobiler-core/Cargo.toml
git commit -m "feat(core): HttpOutcome + HttpHeader payload types for the HTTP capability"
```

---

### Task 2: `PluginResponse.output` becomes `Vec<u8>`

**Files:**
- Modify: `mobiler-core/src/lib.rs:84-88`
- Test: inline `#[cfg(test)]` in `mobiler-core/src/lib.rs`

**Interfaces:**
- Consumes: nothing.
- Produces: `PluginResponse { ok: bool, output: Vec<u8> }`; helper `PluginResponse::text(ok: bool, s: impl Into<String>) -> PluginResponse`; accessor `PluginResponse::as_text(&self) -> Option<&str>`.

This task will break compilation across the workspace. That is expected and intended — the breakage is compiler-guided, which is the whole reason this change is safe. Tasks 3–8 repair it. Do not attempt a workspace-wide `cargo build` until Task 8.

- [ ] **Step 1: Write the failing test**

Add to the existing `mod tests` in `mobiler-core/src/lib.rs`:

```rust
#[test]
fn plugin_response_carries_bytes_and_converts_text() {
    let r = PluginResponse::text(true, "hello");
    assert!(r.ok);
    assert_eq!(r.output, b"hello".to_vec());
    assert_eq!(r.as_text(), Some("hello"));

    let binary = PluginResponse { ok: true, output: vec![0xff, 0xfe] };
    assert_eq!(binary.as_text(), None, "invalid UTF-8 must not panic");
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p mobiler-core --lib plugin_response_carries_bytes`
Expected: FAIL — `PluginResponse::text` not found; `output` is a `String`.

- [ ] **Step 3: Write minimal implementation**

Replace `mobiler-core/src/lib.rs:84-88` with:

```rust
/// A plugin's reply. `output` is raw bytes: the HTTP capability puts a bincode
/// [`HttpOutcome`](crate::HttpOutcome) here, while most plugins put UTF-8 text (use
/// [`PluginResponse::text`] to build one and [`as_text`](Self::as_text) to read it).
///
/// Note the asymmetry with [`PluginCall`], whose `input` stays a `String`: changing
/// `output` affects only where a response is *constructed*, whereas changing `input`
/// would affect where it is *parsed* — in every plugin on every shell. Large uploads
/// pass file paths (text), so `input` stays adequate.
#[derive(Facet, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct PluginResponse {
    pub ok: bool,
    pub output: Vec<u8>,
}

impl PluginResponse {
    /// Build a response whose payload is UTF-8 text — what most plugins return.
    pub fn text(ok: bool, s: impl Into<String>) -> Self {
        Self { ok, output: s.into().into_bytes() }
    }

    /// The payload as text, or `None` if it is not valid UTF-8.
    pub fn as_text(&self) -> Option<&str> {
        std::str::from_utf8(&self.output).ok()
    }
}
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test -p mobiler-core --lib plugin_response_carries_bytes`
Expected: PASS. Other tests in the crate may still fail to compile — Task 3 fixes them.

- [ ] **Step 5: Commit**

```bash
git add mobiler-core/src/lib.rs
git commit -m "feat(core)!: PluginResponse.output String -> Vec<u8>

Binary bodies could not be represented at all before, which already forced
the files plugin to do downloads natively rather than through cx.http."
```

---

### Task 3: `RequestBuilder` and the verb helpers

**Files:**
- Modify: `mobiler-core/src/http.rs` (append `RequestBuilder`)
- Modify: `mobiler-core/src/lib.rs:187-222` (replace the HTTP block on `Cx`)
- Test: inline `#[cfg(test)]` in `mobiler-core/src/lib.rs`

**Interfaces:**
- Consumes: `HttpOutcome` (Task 1), `PluginResponse` (Task 2).
- Produces: `Cx::request(method, url) -> RequestBuilder<'_, E>`; `RequestBuilder::header`, `::bearer`, `::body`, `::send`; `Cx::get`, `::post`, `::put`, `::patch`, `::delete`, all with `then: impl FnOnce(HttpOutcome) -> E + Send + 'static`.

- [ ] **Step 1: Write the failing test**

Replace the existing `cx_http_helpers_build_requests` test in `mobiler-core/src/lib.rs` (currently around line 1328) with:

```rust
#[test]
fn cx_http_helpers_build_requests() {
    let mut cx = Cx::<Ev>::default();
    cx.get("http://h/x", |_| Ev::Tap);
    cx.post("http://h/y", "hello", |_| Ev::Tap);
    cx.put("http://h/p", "putbody", |_| Ev::Tap);
    cx.patch("http://h/z", "patch", |_| Ev::Tap);
    cx.delete("http://h/d", |_| Ev::Tap);

    let methods: Vec<&str> = cx.requests.iter().map(|(c, _)| c.op.as_str()).collect();
    assert_eq!(methods, ["GET", "POST", "PUT", "PATCH", "DELETE"]);
    assert!(cx.requests.iter().all(|(c, _)| c.plugin == "http"));

    let get_input: serde_json::Value = serde_json::from_str(&cx.requests[0].0.input).unwrap();
    assert_eq!(get_input["url"], "http://h/x");
    assert!(get_input["body"].is_null());

    let put_input: serde_json::Value = serde_json::from_str(&cx.requests[2].0.input).unwrap();
    assert_eq!(put_input["url"], "http://h/p");
    assert_eq!(put_input["body"], "putbody");
}

#[test]
fn request_builder_emits_headers_in_order() {
    let mut cx = Cx::<Ev>::default();
    cx.request("PUT", "http://h/access-key")
        .bearer("tok123")
        .header("X-Trace-Id", "abc")
        .body("{}")
        .send(|_| Ev::Tap);

    assert_eq!(cx.requests.len(), 1);
    let (call, _) = &cx.requests[0];
    assert_eq!(call.plugin, "http");
    assert_eq!(call.op, "PUT");

    let input: serde_json::Value = serde_json::from_str(&call.input).unwrap();
    assert_eq!(input["url"], "http://h/access-key");
    assert_eq!(input["body"], "{}");
    assert_eq!(input["headers"][0]["name"], "Authorization");
    assert_eq!(input["headers"][0]["value"], "Bearer tok123");
    assert_eq!(input["headers"][1]["name"], "X-Trace-Id");
    assert_eq!(input["headers"][1]["value"], "abc");
}

#[test]
fn helpers_emit_no_headers_field_content() {
    let mut cx = Cx::<Ev>::default();
    cx.get("http://h/x", |_| Ev::Tap);
    let input: serde_json::Value = serde_json::from_str(&cx.requests[0].0.input).unwrap();
    assert_eq!(input["headers"].as_array().unwrap().len(), 0);
}

#[test]
fn continuation_receives_decoded_outcome() {
    #[derive(Debug, PartialEq)]
    enum Got { Conflict, Offline, Other }

    let classify = |r: PluginResponse| -> Got {
        match HttpOutcome::decode(&r.output).unwrap() {
            HttpOutcome::Response { status: 409, .. } => Got::Conflict,
            HttpOutcome::TransportError { .. } => Got::Offline,
            _ => Got::Other,
        }
    };

    let conflict = HttpOutcome::Response { status: 409, headers: vec![], body: b"c".to_vec() };
    assert_eq!(classify(PluginResponse { ok: false, output: conflict.encode() }), Got::Conflict);

    let offline = HttpOutcome::TransportError { message: "refused".into() };
    assert_eq!(classify(PluginResponse { ok: false, output: offline.encode() }), Got::Offline);
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p mobiler-core --lib cx_http_helpers_build_requests request_builder_emits`
Expected: FAIL — `cx.put` and `cx.request` not found.

- [ ] **Step 3: Write minimal implementation**

Append to `mobiler-core/src/http.rs`:

```rust
use crate::{Cx, PluginResponse};

/// Wire shape of the HTTP request envelope, serialized into `PluginCall.input`.
/// Unknown fields are ignored by older shells, so this can grow without an ABI bump.
#[derive(Serialize)]
struct HttpReq {
    url: String,
    headers: Vec<HttpHeader>,
    body: Option<String>,
}

/// Builds one HTTP request. Obtained from [`Cx::request`]; finished with
/// [`send`](Self::send).
///
/// A builder rather than more arguments on `http()`: it lets later additions
/// (timeouts, query params) arrive as new links in the chain instead of bumping the
/// arity of every existing call site.
pub struct RequestBuilder<'a, E> {
    cx: &'a mut Cx<E>,
    method: String,
    url: String,
    headers: Vec<HttpHeader>,
    body: Option<String>,
}

impl<'a, E> RequestBuilder<'a, E> {
    pub(crate) fn new(cx: &'a mut Cx<E>, method: String, url: String) -> Self {
        Self { cx, method, url, headers: Vec::new(), body: None }
    }

    /// Add a request header. Order is preserved and names may repeat.
    #[must_use]
    pub fn header(mut self, name: impl Into<String>, value: impl Into<String>) -> Self {
        self.headers.push(HttpHeader { name: name.into(), value: value.into() });
        self
    }

    /// Sugar for `header("Authorization", format!("Bearer {token}"))`.
    #[must_use]
    pub fn bearer(self, token: impl AsRef<str>) -> Self {
        self.header("Authorization", format!("Bearer {}", token.as_ref()))
    }

    /// Set the request body.
    #[must_use]
    pub fn body(mut self, body: impl Into<String>) -> Self {
        self.body = Some(body.into());
        self
    }

    /// Dispatch the request. `then(outcome)` produces the typed event delivered back
    /// to `update` once the shell replies.
    pub fn send(self, then: impl FnOnce(HttpOutcome) -> E + Send + 'static) {
        let input = serde_json::to_string(&HttpReq {
            url: self.url,
            headers: self.headers,
            body: self.body,
        })
        .expect("serialize http request");

        self.cx.plugin("http", self.method, input, move |r: PluginResponse| {
            then(decode_outcome(&r))
        });
    }
}

/// Decode what the shell put in `PluginResponse.output`. A shell that returns
/// something undecodable is a bug, but it must not panic the app — surface it as a
/// transport error instead.
fn decode_outcome(r: &PluginResponse) -> HttpOutcome {
    HttpOutcome::decode(&r.output)
        .unwrap_or_else(|e| HttpOutcome::TransportError {
            message: format!("malformed http response: {e}"),
        })
}
```

Replace `mobiler-core/src/lib.rs:187-222` (the whole block from the `http` doc comment through the `delete` fn) with:

```rust
    /// Start an HTTP request with full control — headers, and later timeouts and
    /// query params — finished with [`RequestBuilder::send`].
    ///
    /// ```ignore
    /// cx.request("PUT", url)
    ///     .bearer(&token)
    ///     .body(json)
    ///     .send(|outcome| match outcome.status() {
    ///         Some(409) => Event::NeedsRebase,
    ///         Some(s) if outcome.is_success() => Event::Saved,
    ///         Some(s) => Event::ServerError(s),
    ///         None => Event::Offline,
    ///     });
    /// ```
    pub fn request(
        &mut self,
        method: impl Into<String>,
        url: impl Into<String>,
    ) -> crate::http::RequestBuilder<'_, E> {
        crate::http::RequestBuilder::new(self, method.into(), url.into())
    }

    /// `GET url`, delivering the outcome to `then`.
    pub fn get(&mut self, url: impl Into<String>, then: impl FnOnce(HttpOutcome) -> E + Send + 'static) {
        self.request("GET", url).send(then);
    }
    /// `POST url` with `body`, delivering the outcome to `then`.
    pub fn post(&mut self, url: impl Into<String>, body: impl Into<String>, then: impl FnOnce(HttpOutcome) -> E + Send + 'static) {
        self.request("POST", url).body(body).send(then);
    }
    /// `PUT url` with `body`, delivering the outcome to `then`.
    pub fn put(&mut self, url: impl Into<String>, body: impl Into<String>, then: impl FnOnce(HttpOutcome) -> E + Send + 'static) {
        self.request("PUT", url).body(body).send(then);
    }
    /// `PATCH url` with `body`, delivering the outcome to `then`.
    pub fn patch(&mut self, url: impl Into<String>, body: impl Into<String>, then: impl FnOnce(HttpOutcome) -> E + Send + 'static) {
        self.request("PATCH", url).body(body).send(then);
    }
    /// `DELETE url`, delivering the outcome to `then`.
    pub fn delete(&mut self, url: impl Into<String>, then: impl FnOnce(HttpOutcome) -> E + Send + 'static) {
        self.request("DELETE", url).send(then);
    }
```

Note the old `pub fn http(method, url, body, then)` is **removed** — `request()` replaces it. Add `use crate::http::HttpOutcome;` to the `lib.rs` imports if the re-export from Task 1 does not already bring it into scope.

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test -p mobiler-core --lib`
Expected: PASS, all tests in the crate.

- [ ] **Step 5: Commit**

```bash
git add mobiler-core/src/http.rs mobiler-core/src/lib.rs
git commit -m "feat(core)!: RequestBuilder with headers + cx.put; helpers deliver HttpOutcome

cx.http(method,...) is replaced by cx.request(method, url), which takes headers
and leaves room for timeouts and query params without another arity bump."
```

---

### Task 4: Register `HttpOutcome` for Swift/Kotlin codegen

**Files:**
- Modify: `demos/barbershop/shared/src/bin/codegen.rs`
- Modify: `demos/coffee/shared/src/bin/codegen.rs`
- Modify: `demos/fullstack-todo/mobile/shared/src/bin/codegen.rs`
- Modify: `demos/saldo/shared/src/bin/codegen.rs`
- Modify: `demos/todo/shared/src/bin/codegen.rs`

(The template's `codegen.rs` is Task 10, in PR-C.)

**Interfaces:**
- Consumes: `HttpOutcome` (Task 1).
- Produces: generated `HttpOutcome` / `HttpHeader` types in each app's `SharedTypes`, which Tasks 6 and 7 rely on to encode the outcome shell-side.

`HttpOutcome` lives inside a `Vec<u8>`, so `register_app::<App>()` cannot reach it by traversal. Without an explicit registration the shells get no generated encoder and would need hand-rolled bincode in Swift and Kotlin. These are per-app files — exactly the omission class that bit the `spm_packages` rollout.

- [ ] **Step 1: Make the change in all five files**

In each file, change:

```rust
    let typegen_app = TypeRegistry::new().register_app::<App>()?.build()?;
```

to:

```rust
    // HttpOutcome rides inside PluginResponse.output as bincode, so register_app
    // cannot reach it by traversal — register it explicitly or the shells have no
    // generated encoder.
    let typegen_app = TypeRegistry::new()
        .register_app::<App>()?
        .register_type::<mobiler_core::HttpOutcome>()?
        .build()?;
```

If a demo's `shared/Cargo.toml` does not already depend on `mobiler-core`, add it — check with `grep mobiler-core demos/*/shared/Cargo.toml demos/*/*/shared/Cargo.toml`.

- [ ] **Step 2: Verify codegen runs and emits the type**

Run for one demo:

```bash
cd demos/saldo/shared && cargo run --bin codegen -- --language swift --output-dir /tmp/tg-check
grep -r "HttpOutcome" /tmp/tg-check | head
```

Expected: at least one hit showing a generated `HttpOutcome` Swift type. If `register_type` errors with a Facet complaint, confirm `#[repr(C)]` is present on both `HttpOutcome` and `HttpHeader` (Task 1).

- [ ] **Step 3: Commit**

```bash
git add demos/*/shared/src/bin/codegen.rs demos/*/*/shared/src/bin/codegen.rs demos/*/shared/Cargo.toml
git commit -m "build(demos): register HttpOutcome for Swift/Kotlin codegen"
```

---

### Task 5: Web shell — exhaustive verbs, headers, binary body

**Files:**
- Modify: `mobiler-web/src/lib.rs:416-441`

**Interfaces:**
- Consumes: `HttpOutcome`, `HttpHeader` (Task 1); `PluginResponse` (Task 2).
- Produces: a web HTTP plugin returning bincode `HttpOutcome` in `PluginResponse.output`.

This is where the PUT-degrades-to-GET defect lives. `gloo-net` 0.6 already has everything needed — `Request::put` at `request.rs:247`, `.method()` at `:141`, `.header()` at `:78`, `Response::status()`/`headers()`/`binary()`. The defect was only ever a missing match arm.

- [ ] **Step 1: Replace the HTTP branch**

Replace `mobiler-web/src/lib.rs:416-441` (from `if call.plugin != "http"` through the end of that function's HTTP handling) with:

```rust
    if call.plugin != "http" {
        return PluginResponse::text(false, format!("plugin '{}' not available", call.plugin));
    }
    let v: serde_json::Value = serde_json::from_str(&call.input).unwrap_or(serde_json::Value::Null);
    let url = v.get("url").and_then(serde_json::Value::as_str).unwrap_or("");
    let body = v.get("body").and_then(serde_json::Value::as_str);
    let req_headers: Vec<(String, String)> = v
        .get("headers")
        .and_then(serde_json::Value::as_array)
        .map(|hs| {
            hs.iter()
                .filter_map(|h| {
                    Some((
                        h.get("name")?.as_str()?.to_string(),
                        h.get("value")?.as_str()?.to_string(),
                    ))
                })
                .collect()
        })
        .unwrap_or_default();

    use gloo_net::http::{Method, Request};

    // Exhaustive: an unknown verb is an error, never a silent GET. The previous
    // `_ => Request::get(url)` fallthrough turned every PUT into a GET.
    let builder = match call.op.as_str() {
        "GET" => Request::get(url),
        "POST" => Request::post(url),
        "PUT" => Request::put(url),
        "PATCH" => Request::patch(url),
        "DELETE" => Request::delete(url),
        "HEAD" => Request::get(url).method(Method::HEAD),
        "OPTIONS" => Request::get(url).method(Method::OPTIONS),
        other => return http_transport_error(format!("unsupported HTTP method '{other}'")),
    };

    // Only default Content-Type when the caller did not set one.
    let caller_set_content_type =
        req_headers.iter().any(|(n, _)| n.eq_ignore_ascii_case("content-type"));
    let mut builder = builder;
    for (name, value) in &req_headers {
        builder = builder.header(name, value);
    }
    if body.is_some() && !caller_set_content_type {
        builder = builder.header("Content-Type", "application/json");
    }

    let request = match body {
        Some(b) => builder.body(b),
        None => builder.build(),
    };
    let request = match request {
        Ok(r) => r,
        Err(e) => return http_transport_error(e.to_string()),
    };

    match request.send().await {
        Ok(resp) => {
            let status = resp.status();
            let headers = resp
                .headers()
                .entries()
                .map(|(name, value)| HttpHeader { name, value })
                .collect();
            let bytes = resp.binary().await.unwrap_or_default();
            let outcome = HttpOutcome::Response { status, headers, body: bytes };
            PluginResponse { ok: (200..300).contains(&status), output: outcome.encode() }
        }
        Err(e) => http_transport_error(e.to_string()),
    }
}

/// A failure where no HTTP response was obtained. `ok` is false and there is no status.
fn http_transport_error(message: String) -> PluginResponse {
    PluginResponse { ok: false, output: HttpOutcome::TransportError { message }.encode() }
}
```

Add `use mobiler_core::{HttpHeader, HttpOutcome};` to the imports at the top of
`mobiler-web/src/lib.rs`. **No `bincode` dependency is needed** — encoding goes through
`HttpOutcome::encode` from Task 1, which uses crux's FFI format.

- [ ] **Step 2: Fix the remaining `PluginResponse` constructions in this file**

Run: `cargo check -p mobiler-web --target wasm32-unknown-unknown`
Expected: errors listing every other `PluginResponse { ok, output: <String> }` in the file.

For each, switch to the helper — e.g.

```rust
return PluginResponse { ok, output: if ok { "ok".into() } else { "cancel".into() } };
```

becomes

```rust
return PluginResponse::text(ok, if ok { "ok" } else { "cancel" });
```

Re-run until clean.

- [ ] **Step 3: Verify PUT actually leaves as PUT**

Start a throwaway echo server that reports the method it saw:

```bash
python3 -c "
from http.server import BaseHTTPRequestHandler, HTTPServer
class H(BaseHTTPRequestHandler):
    def _r(self):
        self.send_response(409 if self.path=='/conflict' else 200)
        self.send_header('Content-Type','text/plain'); self.end_headers()
        self.wfile.write(self.command.encode()+b' '+self.headers.get('Authorization','none').encode())
    do_GET=do_PUT=do_POST=do_PATCH=do_DELETE=_r
HTTPServer(('127.0.0.1',8777),H).serve_forever()" &
```

Build and drive the web shell against it using the repo's existing headless-chrome/CDP verify recipe (see the `mobiler-web shell` notes). Assert three things:
1. A `cx.put` request arrives at the server as `PUT`, not `GET`.
2. An `Authorization: Bearer ...` header arrives intact.
3. `/conflict` surfaces as `HttpOutcome::Response { status: 409 }`, and stopping the server surfaces as `HttpOutcome::TransportError`.

Kill the server when done: `kill %1`.

- [ ] **Step 4: Commit**

```bash
git add mobiler-web/src/lib.rs mobiler-web/Cargo.toml
git commit -m "fix(web)!: exhaustive HTTP verb mapping, request headers, binary body

PUT silently became GET via the `_ => Request::get(url)` fallthrough. gloo-net
had Request::put all along — this was only ever a missing match arm."
```

---

### Task 6: iOS demo shells — HTTP plugin and convenience initialiser

**Files:**
- Modify: `demos/barbershop/iOS/Sources/Core.swift`
- Modify: `demos/coffee/iOS/Sources/Core.swift`
- Modify: `demos/fullstack-todo/mobile/iOS/Sources/Core.swift`
- Modify: `demos/saldo/iOS/Sources/Core.swift`
- Modify: `demos/todo/iOS/Sources/Core.swift`

**Interfaces:**
- Consumes: generated `HttpOutcome`/`HttpHeader` Swift types (Task 4).
- Produces: iOS `HttpPlugin` returning a bincode `HttpOutcome`; a `PluginResponse(ok:output:)` String initialiser so the other plugins compile untouched.

These five files are byte-identical in the `HttpPlugin` region. Edit one, verify, then copy the region to the rest — the same "write once + cp" approach used for `Render.swift`.

- [ ] **Step 1: Add the convenience initialiser**

In `demos/saldo/iOS/Sources/Core.swift`, immediately after the imports, add:

```swift
// Keeps every non-HTTP plugin compiling unchanged now that PluginResponse.output is
// bytes: they all construct responses from Strings.
extension PluginResponse {
    init(ok: Bool, output: String) {
        self.init(ok: ok, output: [UInt8](output.utf8))
    }
}
```

- [ ] **Step 2: Replace `HttpPlugin`**

Replace the `enum HttpPlugin { ... }` block (in `demos/saldo/iOS/Sources/Core.swift`, the analogue of `mobiler/templates/iOS/Sources/Core.swift:157-182`) with:

```swift
/// HTTP capability (paired with `cx.request`/`get`/`post`/`put`/... in Rust). `op` is
/// the method; `input` is `{"url":..., "headers":[{"name":...,"value":...}], "body":...}`.
/// Returns a bincode `HttpOutcome` in `output`; `ok` = 2xx.
enum HttpPlugin {
    static func handle(op: String, input: String) async -> PluginResponse {
        guard
            let data = input.data(using: .utf8),
            let obj = try? JSONSerialization.jsonObject(with: data) as? [String: Any],
            let urlString = obj["url"] as? String,
            let url = URL(string: urlString)
        else {
            return transportError("invalid http request envelope")
        }

        var req = URLRequest(url: url)
        req.httpMethod = op

        var callerSetContentType = false
        if let headers = obj["headers"] as? [[String: Any]] {
            for h in headers {
                guard let name = h["name"] as? String, let value = h["value"] as? String else { continue }
                req.addValue(value, forHTTPHeaderField: name)
                if name.lowercased() == "content-type" { callerSetContentType = true }
            }
        }

        if let body = obj["body"] as? String {
            req.httpBody = body.data(using: .utf8)
            if !callerSetContentType {
                req.setValue("application/json", forHTTPHeaderField: "Content-Type")
            }
        }

        do {
            let (respData, resp) = try await URLSession.shared.data(for: req)
            guard let http = resp as? HTTPURLResponse else {
                return transportError("non-HTTP response")
            }
            let headers = http.allHeaderFields.compactMap { key, value -> HttpHeader? in
                guard let name = key as? String else { return nil }
                return HttpHeader(name: name, value: String(describing: value))
            }
            let status = UInt16(http.statusCode)
            let outcome = HttpOutcome.response(status: status, headers: headers, body: [UInt8](respData))
            return PluginResponse(ok: (200..<300).contains(http.statusCode), output: encode(outcome))
        } catch {
            // URLSession throws only when no response was obtained.
            return transportError(error.localizedDescription)
        }
    }

    private static func encode(_ outcome: HttpOutcome) -> [UInt8] {
        (try? outcome.bincodeSerialize()) ?? []
    }

    private static func transportError(_ message: String) -> PluginResponse {
        PluginResponse(ok: false, output: encode(.transportError(message: message)))
    }
}
```

**Verified generated Swift API** (from Task 4's actual codegen output — use these exact
spellings, they are not illustrative):

```swift
HttpOutcome.response(status: UInt16, headers: [HttpHeader], body: [UInt8])
HttpOutcome.transportError(message: String)     // lowercase first letter
func bincodeSerialize() throws -> [UInt8]
// PluginResponse.output is [UInt8]
```

Note `status` is `UInt16`, so `UInt16(http.statusCode)` is the correct conversion, and
`bincodeSerialize()` throws — hence the `(try? ...) ?? []` in `encode` above.

- [ ] **Step 3: Verify it builds**

Run: `cd demos/saldo/iOS && ./build-ios.sh`
Expected: build succeeds. If other plugins error on `PluginResponse`, the Step 1 extension is missing or misplaced.

- [ ] **Step 4: Propagate to the other four demos**

Copy the `extension PluginResponse` block and the `enum HttpPlugin` block into the other four `Core.swift` files, replacing their existing `HttpPlugin`. Then verify each builds.

- [ ] **Step 5: Commit**

```bash
git add demos/*/iOS/Sources/Core.swift demos/*/*/iOS/Sources/Core.swift
git commit -m "feat(ios): HTTP headers + HttpOutcome in the demo shells"
```

---

### Task 7: Android demo shells — HTTP plugin, null-body fix, pseudo-constructor

**Files:**
- Modify: `demos/barbershop/Android/app/src/main/java/dev/mobiler/barbershop/Core.kt`
- Modify: `demos/coffee/Android/app/src/main/java/dev/mobiler/coffee/Core.kt`
- Modify: `demos/fullstack-todo/mobile/Android/app/src/main/java/dev/mobiler/mobile/Core.kt`
- Modify: `demos/saldo/Android/app/src/main/java/rs/mobiler/saldo/Core.kt`
- Modify: `demos/todo/Android/app/src/main/java/dev/mobiler/todo/Core.kt`

**Interfaces:**
- Consumes: generated `HttpOutcome`/`HttpHeader` Kotlin types (Task 4).
- Produces: Android `HttpPlugin` returning a bincode `HttpOutcome`; a `PluginResponse(ok, output: String)` pseudo-constructor.

This task also fixes a latent crash: OkHttp requires a non-null body for PUT/POST/PATCH, so `.method("PUT", null)` throws `IllegalArgumentException`. A bodyless PUT currently crashes rather than failing gracefully.

- [ ] **Step 1: Add the pseudo-constructor**

In `demos/saldo/Android/app/src/main/java/rs/mobiler/saldo/Core.kt`, at file scope near the top, add:

```kotlin
// Keeps every non-HTTP plugin compiling unchanged now that PluginResponse.output is
// bytes. Kotlin allows a top-level function named like the type, so existing
// `PluginResponse(true, "text")` call sites resolve here.
fun PluginResponse(ok: Boolean, output: String): PluginResponse =
    PluginResponse(ok, output.toByteArray(Charsets.UTF_8).toList())
```

If the generated `output` is a `List<UByte>` rather than `List<Byte>`, adjust the conversion to match — check the generated type before writing.

- [ ] **Step 2: Replace `HttpPlugin`**

Replace the `class HttpPlugin : MobilerPlugin { ... }` block with:

```kotlin
/**
 * HTTP capability (paired with `cx.request`/`get`/`post`/`put`/... in Rust). `op` is
 * the method; `input` is `{"url":..., "headers":[{"name":...,"value":...}], "body":...}`.
 * Runs on the IO dispatcher; returns a bincode `HttpOutcome` with `ok` = 2xx.
 */
class HttpPlugin : MobilerPlugin {
    private val client = OkHttpClient()

    override suspend fun handle(op: String, input: String): PluginResponse = withContext(Dispatchers.IO) {
        try {
            val obj = JSONObject(input)
            val url = obj.getString("url")
            val bodyStr = if (obj.has("body") && !obj.isNull("body")) obj.getString("body") else null

            var callerSetContentType = false
            val builder = Request.Builder().url(url)
            if (obj.has("headers")) {
                val headers = obj.getJSONArray("headers")
                for (i in 0 until headers.length()) {
                    val h = headers.getJSONObject(i)
                    val name = h.getString("name")
                    builder.addHeader(name, h.getString("value"))
                    if (name.lowercase() == "content-type") callerSetContentType = true
                }
            }

            // OkHttp REQUIRES a body for these verbs — `.method("PUT", null)` throws.
            val needsBody = op in setOf("POST", "PUT", "PATCH")
            val mediaType = if (callerSetContentType) null else "application/json".toMediaType()
            val reqBody = when {
                bodyStr != null -> bodyStr.toRequestBody(mediaType)
                needsBody -> "".toRequestBody(mediaType)
                else -> null
            }

            val request = builder.method(op, reqBody).build()
            client.newCall(request).execute().use { resp ->
                val headers = resp.headers.map { (name, value) -> HttpHeader(name, value) }
                val bytes = resp.body?.bytes() ?: ByteArray(0)
                val outcome = HttpOutcome.Response(resp.code.toUShort(), headers, bytes.toUByteList())
                PluginResponse(resp.isSuccessful, outcome.bincodeSerialize().toUByteList())
            }
        } catch (e: IOException) {
            // IOException means no response was obtained.
            transportError(e.message ?: "network error")
        } catch (e: Exception) {
            transportError(e.message ?: "http error")
        }
    }

    private fun transportError(message: String): PluginResponse =
        PluginResponse(false, HttpOutcome.TransportError(message).bincodeSerialize().toUByteList())
}

/// The generated types use List<UByte>, not List<Byte> — ByteArray.toList() gives
/// the wrong element type and will not compile.
private fun ByteArray.toUByteList(): List<UByte> = this.map { it.toUByte() }
```

Add `import java.io.IOException` if absent.

**Verified generated Kotlin API** (from Task 4's actual codegen output — use these exact
spellings, they are not illustrative):

```kotlin
HttpOutcome.Response(status: UShort, headers: List<HttpHeader>, body: List<UByte>)
HttpOutcome.TransportError(message: String)     // capitalized; nested data classes
fun bincodeSerialize(): ByteArray               // NOT throwing
// PluginResponse.output is List<UByte>
```

- [ ] **Step 3: Verify it builds**

Run: `cd demos/saldo/Android && ./gradlew :app:assembleDebug`
Expected: BUILD SUCCESSFUL. If other plugins error, the Step 1 pseudo-constructor is missing or its byte type is wrong.

- [ ] **Step 4: Propagate to the other four demos**

Copy both blocks into the other four `Core.kt` files and verify each builds. Note the differing package declarations — copy the blocks, not the whole file.

- [ ] **Step 5: Commit**

```bash
git add demos/*/Android/app/src/main/java/*/*/*/Core.kt demos/*/*/Android/app/src/main/java/*/*/*/Core.kt
git commit -m "feat(android): HTTP headers + HttpOutcome; fix null-body crash on PUT

OkHttp requires a non-null body for PUT/POST/PATCH — .method(\"PUT\", null)
threw IllegalArgumentException, so a bodyless PUT crashed."
```

---

### Task 8: Update the two demo app-cores, bump versions, ship PR-A

**Files:**
- Modify: `demos/fullstack-sqlx/app-core/src/lib.rs:39,45,60,74`
- Modify: `demos/fullstack-todo/todo-core/src/lib.rs:55,103,106,122`
- Modify: `mobiler-core/Cargo.toml` (0.31.0 → 0.32.0)
- Modify: `mobiler-web/Cargo.toml` (0.31.0 → 0.32.0, and its `mobiler-core` dep)

**Interfaces:**
- Consumes: everything from Tasks 1–7.
- Produces: a green workspace, published `mobiler-core` 0.32.0 and `mobiler-web` 0.32.0.

- [ ] **Step 1: Update the call sites**

The callbacks now receive `HttpOutcome`, not `PluginResponse`. In `demos/fullstack-sqlx/app-core/src/lib.rs` change lines 39 and 45 from:

```rust
cx.get(format!("{API}/notes"), |r| Msg::GotNotes(if r.ok { r.output } else { String::new() }));
```

to:

```rust
cx.get(format!("{API}/notes"), |r| {
    Msg::GotNotes(r.text().unwrap_or_default().to_string())
});
```

Apply the same shape at line 60 (`cx.post`) and line 74 (`cx.delete`). In `demos/fullstack-todo/todo-core/src/lib.rs`, update lines 55, 103, 106 and 122 the same way — `parse_list(&resp)` and `after_mutation` now take an `HttpOutcome`, so change their signatures and read the body via `.text().unwrap_or_default()`.

- [ ] **Step 2: Verify every workspace**

`mobiler-web` and each demo are **separate workspaces** — the root workspace is only
`["mobiler", "mobiler-ui", "mobiler-core", "xtask"]`, so `--workspace` from the root
does **not** cover them and stayed green even mid-migration. Build each explicitly:

```bash
# root workspace
cargo build --workspace && cargo test --workspace && cargo clippy --workspace -- -D warnings

# standalone: web shell
(cd mobiler-web && cargo check --target wasm32-unknown-unknown && cargo clippy -- -D warnings)

# standalone: every demo
for d in demos/*/ demos/*/mobile/; do
  [ -f "$d/Cargo.toml" ] && (cd "$d" && echo "--- $d" && cargo build && cargo test) || true
done
```

Expected: all clean. This is the first point at which every workspace builds.

- [ ] **Step 3: Bump versions**

`mobiler-core/Cargo.toml`: `version = "0.32.0"`.
`mobiler-web/Cargo.toml`: `version = "0.32.0"`, and its `mobiler-core` dependency to `"0.32"`.
Leave `mobiler-ui` at 0.23.0 — untouched and not republished.

- [ ] **Step 4: Audit the READMEs before publishing**

Check `mobiler-core/README.md` and `mobiler-web/README.md` for the HTTP vocabulary — `cx.http`, `cx.get`, `response.ok`, `response.output`. Update to the builder and `HttpOutcome`. A stale-README catch before publish is exactly what the Maps release taught.

- [ ] **Step 5: Ship PR-A**

Use the `ship-pr` skill. Confirm ALL CI checks pass — including the per-demo iOS and Android lanes, which are what verify Tasks 6 and 7.

- [ ] **Step 6: Publish**

Use the `release-libs` skill: `mobiler-core` then `mobiler-web`. `mobiler-ui` is skipped.

Note the publishing gotcha from the LazyList release: a persisted `cd mobiler-web` broke `cargo publish` — return to the repo root between crates.

---

# PR-C — CLI templates

### Task 9: Template shell arms

**Files:**
- Modify: `mobiler/templates/iOS/Sources/Core.swift:157-182`
- Modify: `mobiler/templates/Android/app/src/main/java/__PACKAGE_PATH__/Core.kt:321-337`

**Interfaces:**
- Consumes: the finished demo shell code from Tasks 6 and 7.
- Produces: templates that scaffold apps with the new HTTP capability.

- [ ] **Step 1: Copy the verified blocks**

Copy the `extension PluginResponse` + `enum HttpPlugin` blocks from `demos/saldo/iOS/Sources/Core.swift` into the iOS template, and the pseudo-constructor + `class HttpPlugin` from `demos/saldo/Android/.../Core.kt` into the Android template. They must be byte-identical to the demo copies apart from package/module lines — divergence here is what left a stale copy during the Batch 4 work and cost a red CI lane.

- [ ] **Step 2: Verify with a fresh scaffold**

```bash
cd /tmp && rm -rf httptest && cargo run --manifest-path ~/working_docker/rust/mobiler/mobiler/Cargo.toml -- new httptest
cd httptest && cargo build
```

Expected: scaffolds and builds.

- [ ] **Step 3: Commit**

```bash
git add mobiler/templates/
git commit -m "feat(cli): template shell arms for the HTTP capability"
```

---

### Task 10: Template codegen registration, core pin, CLI bump, ship PR-C

**Files:**
- Modify: `mobiler/templates/shared/src/bin/codegen.rs`
- Modify: `mobiler/templates/shared/Cargo.toml` and any template `Cargo.toml` pinning `mobiler-core`
- Modify: `mobiler/Cargo.toml` (0.47.0 → 0.48.0)

- [ ] **Step 1: Register the type in the template**

Apply the identical `.register_type::<mobiler_core::HttpOutcome>()?` change from Task 4 to `mobiler/templates/shared/src/bin/codegen.rs`.

- [ ] **Step 2: Pin the published core and bump the CLI**

Update every template `Cargo.toml` pinning `mobiler-core` to `"0.32"` and `mobiler-web` to `"0.32"`. Set `mobiler/Cargo.toml` `version = "0.48.0"`.

- [ ] **Step 3: Verify a scaffold against the published crates**

```bash
cd /tmp && rm -rf httptest2 && cargo run --manifest-path ~/working_docker/rust/mobiler/mobiler/Cargo.toml -- new httptest2
cd httptest2 && cargo build && cargo test
```

Expected: resolves `mobiler-core` 0.32.0 from crates.io and builds.

- [ ] **Step 4: Verify `mobiler upgrade` merges cleanly**

Scaffold an app on CLI 0.47, hand-edit one line of its `Core.swift`, then upgrade to 0.48 and confirm the three-way merge keeps both the user edit and the new HTTP arm. `Core.swift` is SHELL-class in `upgrade.rs`, so this path matters.

- [ ] **Step 5: Ship PR-C and release**

Use `ship-pr`, confirm all CI green, then the `release-cli` skill — publish and tag `v0.48.0`. Then run `post-release`.

---

# Acceptance

### Task 11: Stoa integration — the real acceptance test

**Files:**
- Modify: `~/working_docker/rust/stoa/crates/stoa-app/src/net.rs:22-40`
- Modify: `~/working_docker/rust/stoa/crates/stoa-app/Cargo.toml` (mobiler-core → 0.32)

**Interfaces:**
- Consumes: published `mobiler-core` 0.32.0.
- Produces: a Stoa client that actually authenticates.

- [ ] **Step 1: Point Stoa at the new core and transmit the bearer**

Bump the `mobiler-core` dependency to `0.32`. Then replace the body of `request` in `net.rs`, deleting the deferred TODO comment at lines 22-28:

```rust
pub(crate) fn request(
    model: &mut Model,
    cx: &mut Cx<Event>,
    method: &str,
    url: String,
    bearer: Option<String>,
    body: Option<String>,
    then: impl FnOnce(HttpOutcome) -> Event + Send + 'static,
) {
    model.sent.push(SentIntent { method: method.to_string(), url: url.clone(), bearer: bearer.clone() });

    let mut req = cx.request(method, url);
    if let Some(token) = &bearer {
        req = req.bearer(token);
    }
    if let Some(b) = body {
        req = req.body(b);
    }
    req.send(then);
}
```

Update the call sites in `stoa-app/src/app.rs` whose closures take a `PluginResponse` to take an `HttpOutcome` — they are at lines 123, 154, 160, 174, 186, 192, 258, 284, 325, 372, 594 and 705.

- [ ] **Step 2: Run a local DS and verify the three acceptance criteria**

```bash
cd ~/working_docker/rust/stoa && cargo run -p stoa-ds &
```

Verify, via the `stoa-app` test harness or a driver binary:

1. **Authed PUT works.** `PUT /access-key` with a bearer returns 2xx. Previously unreachable — no header could be sent and PUT degraded to GET on web.
2. **409 is distinguishable.** Force an epoch conflict on `POST /commit`; assert the app sees `HttpOutcome::Response { status: 409, .. }`.
3. **Offline is distinguishable from 500.** Stop `stoa-ds`, retry, and assert the app sees `HttpOutcome::TransportError`, *not* a 409 and not a generic failure.

Criterion 3 is the one that was impossible before: all three collapsed into `ok: false`.

- [ ] **Step 3: Confirm no stale auth TODO remains**

Run: `grep -rn "TODO(deferred)" ~/working_docker/rust/stoa/crates/stoa-app/src/`
Expected: no hits about auth headers or PUT.

- [ ] **Step 4: Commit in the Stoa repo**

```bash
cd ~/working_docker/rust/stoa
git add crates/stoa-app/
git commit -m "feat(stoa-app): transmit bearer auth; handle 409 vs transport failure

Unblocked by mobiler-core 0.32's HTTP capability — headers, PUT, and real
status codes. Removes the deferred TODO in net.rs."
```

- [ ] **Step 5: Clean up**

Run the `cleanup-after-phase` skill to clear regenerable build caches — this plan builds five demos on two native toolchains and fills disk fast.

---

## Self-Review Notes

**Spec coverage.** Every spec section maps to a task: ABI change → Task 2; `HttpOutcome` → Task 1; codegen registration → Tasks 4 and 10; Rust API → Task 3; request envelope → Task 3; iOS shell → Tasks 6 and 9; Android shell (incl. null-body fix) → Tasks 7 and 9; web shell → Task 5; churn-mitigation constructors → Tasks 2, 6, 7; error-handling table → Tasks 1, 5, 6, 7; testing → Tasks 1, 3, 5, 11; release process → Tasks 8 and 10.

**Known uncertainty, deliberately flagged rather than guessed.** The exact generated Swift/Kotlin spelling of `HttpOutcome`'s cases and its `bincodeSerialize()` method comes out of Task 4. Tasks 6, 7 and 9 instruct the implementer to inspect the generated file rather than trust the illustrative spelling. Resolve this in Task 4 before starting Task 6.

**Out of scope for this plan.** Timeouts, binary request bodies, query-param helpers, redirect/cookie/retry policy, and all of Release B (streaming uploads and downloads, progress, cancellation).
