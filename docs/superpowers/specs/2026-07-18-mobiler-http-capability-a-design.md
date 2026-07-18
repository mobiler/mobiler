# Mobiler HTTP Capability — Release A: request/response

**Date:** 2026-07-18
**Status:** design approved, not yet implemented
**Scope:** the request/response half of a full-REST HTTP capability
**Companion:** `2026-07-18-mobiler-http-capability-b-streaming-design.md` (Release B, large transfers)

## Motivation

The Stoa IM app (`~/working_docker/rust/stoa`) is the first real consumer of mobiler's
HTTP capability, and it cannot complete a single authenticated call. Its own
`crates/stoa-app/src/net.rs:22-28` documents the blockers as a deferred TODO.

Three concrete defects:

1. **No request headers.** The http plugin input is `{url, body}` on all three shells.
   There is no way to send `Authorization: Bearer`. Every authed Stoa endpoint —
   `/keypackages`, `/commit`, `/commits`, `/messages`, `/access-key` — is unreachable.
   `net.rs` records the bearer into `model.sent` for tests but never transmits it.

2. **PUT silently degrades to GET on web.** `mobiler-web/src/lib.rs:424-429` matches
   `POST`/`PATCH`/`DELETE` and falls through with `_ => Request::get(url)`. A
   `PUT /access-key` becomes a GET, producing a confusing 405 rather than a clear
   failure. iOS (`Core.swift:168`) and Android (`Core.kt:329`) pass the verb through
   correctly — web is the only broken shell.

3. **No status code reaches the core.** `PluginResponse { ok, output }` collapses
   "409 Conflict, go rebase", "500, server broke", and "offline, never reached the
   server" into a single `ok: false`. Stoa's commit flow is built around 409-rebase
   and cannot distinguish it from a dead network.

A fourth, latent: Android OkHttp requires a non-null body for PUT/POST/PATCH.
`.method("PUT", null)` throws `IllegalArgumentException`, so a bodyless PUT crashes
rather than failing gracefully.

## Design principles

**Keep the narrow waist.** The plugin ABI is deliberately fixed and stringly-typed:
`{plugin, op, input} -> {ok, output}`. That is what lets `mobiler plugin add` drop in
a new plugin without touching the core or regenerating shared types — the reason 30
plugins shipped, several (websocket, iap, geofence) CLI-only with no lib publish.
HTTP-specific richness goes *inside* the payload, never as new columns on the shared
struct. Notably: **`status` does NOT become a field on `PluginResponse`.**

**Model transport failure and HTTP response as different things.** "The server said
500" and "never reached the server" are categorically different. A sum type says so;
a sentinel value does not.

**No derived state.** `is_success()` is computed from `status`, not stored alongside it.

## ABI change

One field changes type:

```rust
pub struct PluginResponse {
    pub ok: bool,
    pub output: Vec<u8>,   // was: String
}
```

`PluginCall { plugin, op, input: String }` is **unchanged**.

### Why output becomes bytes

A `String` body cannot represent a PNG, a protobuf, or an encrypted media blob. This
limitation has already forced one workaround: the `files` plugin performs downloads
natively precisely because `cx.http`'s String body could not carry them.

### Why input stays a String

The asymmetry is deliberate and should be documented in the code:

- Changing `output` affects only where `PluginResponse` is *constructed*, and the
  convenience constructors below absorb nearly all of it.
- Changing `input` would affect where it is *parsed* — `JSONObject(input)` in Kotlin,
  `JSONSerialization` in Swift, `serde_json::from_str` in Rust — in every plugin on
  every shell, with no equivalent mitigation. Roughly triple the churn.
- Downloads of binary are common; uploads of binary are handled by Release B, which
  passes **file paths** (text) rather than bytes, so `input: String` remains adequate
  even for large transfers.

### Keeping `ok`

`ok` stays. It is not redundant for non-HTTP plugins, where it is the only success
signal. For HTTP it means 2xx, and the envelope carries the authoritative status.

## The HttpOutcome type

New shared type in `mobiler-core`, bincode-serialized into `PluginResponse.output`,
code-generated to Swift and Kotlin alongside `Widget`/`Action`:

```rust
pub enum HttpOutcome {
    Response {
        status: u16,
        headers: Vec<(String, String)>,
        body: Vec<u8>,
    },
    /// No HTTP response was obtained — for any reason. Chiefly network failure
    /// (offline, DNS, TLS, connection refused, timeout), but also a request that
    /// could not be attempted at all (unknown verb, malformed envelope). The
    /// defining property is that the server never answered, so no status exists.
    TransportError { message: String },
}
```

Bincode, not JSON-with-base64: encoding the envelope as JSON would force the body
back through base64 and defeat the reason `output` became bytes. Bincode is already
the house serialization across the FFI, so the shells have both directions available.

Accessors on the core side:

```rust
impl HttpOutcome {
    pub fn status(&self) -> Option<u16>;          // None for TransportError
    pub fn is_success(&self) -> bool;             // 200..300
    pub fn body(&self) -> &[u8];                  // empty for TransportError
    pub fn text(&self) -> Option<&str>;           // Some iff body is valid UTF-8
    pub fn header(&self, name: &str) -> Option<&str>;   // case-insensitive lookup
}
```

## Rust API

A builder, so existing helpers keep their arity and future additions (timeouts, query
params) need no new functions:

```rust
cx.request("PUT", url)
    .bearer(&token)
    .header("X-Trace-Id", trace)
    .body(json)
    .send(|outcome| match outcome {
        HttpOutcome::Response { status: 409, .. }        => Event::NeedsRebase,
        HttpOutcome::Response { status, body } if (200..300).contains(&status)
                                                          => Event::Sent(body),
        HttpOutcome::Response { status, .. }             => Event::ServerError(status),
        HttpOutcome::TransportError { message }          => Event::Offline(message),
    });
```

Signatures:

```rust
impl<E> Cx<E> {
    pub fn request(&mut self, method: impl Into<String>, url: impl Into<String>)
        -> RequestBuilder<'_, E>;

    // Convenience wrappers over the builder. All deliver HttpOutcome.
    pub fn get(&mut self, url, then: impl FnOnce(HttpOutcome) -> E + Send + 'static);
    pub fn post(&mut self, url, body, then);
    pub fn put(&mut self, url, body, then);     // NEW — was missing entirely
    pub fn patch(&mut self, url, body, then);
    pub fn delete(&mut self, url, then);
}

impl<'a, E> RequestBuilder<'a, E> {
    pub fn header(self, name: impl Into<String>, value: impl Into<String>) -> Self;
    pub fn bearer(self, token: impl AsRef<str>) -> Self;   // sugar for Authorization
    pub fn body(self, body: impl Into<String>) -> Self;
    pub fn send(self, then: impl FnOnce(HttpOutcome) -> E + Send + 'static);
}
```

The callback receives a decoded `HttpOutcome`, not a raw `PluginResponse`. Apps never
parse the envelope themselves.

**Breaking change:** the five convenience helpers change their callback type from
`PluginResponse` to `HttpOutcome`. This is compile-time caught. Blast radius is small
— only two demos call them, ~8 sites total:
`demos/fullstack-sqlx/app-core/src/lib.rs:39,45,60,74` and
`demos/fullstack-todo/todo-core/src/lib.rs:55,103,106,122`.

## Request envelope

`PluginCall.input`, JSON as today, with one added field:

```json
{
  "url": "https://ds.example/access-key",
  "headers": [["Authorization", "Bearer ..."], ["X-Trace-Id", "..."]],
  "body": "{...}"
}
```

`headers` is an array of pairs, not an object, to preserve order and permit repeated
names (`Set-Cookie`, `Accept`). `body` stays nullable. Unknown fields are ignored by
older shells, so the envelope can grow without an ABI bump.

**Content-Type:** currently each shell hardcodes `application/json` whenever a body is
present. That becomes conditional — set it only if the caller has not supplied a
`Content-Type` header of their own.

## Shell changes

### iOS — `Core.swift` HttpPlugin (`mobiler/templates/iOS/Sources/Core.swift:157-182`)

- Apply `headers` to the `URLRequest`.
- Only default `Content-Type` when the caller did not set one.
- Build `HttpOutcome.Response` from `HTTPURLResponse` — status, `allHeaderFields`,
  raw `Data` body (no `String(data:encoding:)` lossy conversion).
- Map the `catch` branch to `HttpOutcome.TransportError` rather than a generic failure.
- Bincode-encode the outcome into `output`.

### Android — `Core.kt` HttpPlugin (`mobiler/templates/Android/.../Core.kt:321-337`)

- Apply `headers` to the `Request.Builder`.
- **Fix the null-body crash:** OkHttp requires a body for PUT/POST/PATCH. Use an empty
  `"".toRequestBody(null)` for those verbs when the caller supplies none.
- Build `HttpOutcome.Response` from `resp.code`, `resp.headers`, `resp.body?.bytes()`.
- Map `IOException` to `TransportError`; let other exceptions remain failures.
- Bincode-encode the outcome.

### Web — `mobiler-web/src/lib.rs:416-441`

- **Replace the `_ => Request::get(url)` fallthrough with an explicit, exhaustive verb
  mapping** including PUT, HEAD, OPTIONS. An unrecognised verb must be an error, never
  a silent GET.
- Apply request headers; read response headers back off the `Response`.
- Return the raw body via `Response::binary()` rather than `.text()`.
- Map `send()` errors to `TransportError`.

Everything required already exists in `gloo-net` 0.6 — no dependency change and no
drop to `web_sys` is needed:

| Need | gloo-net 0.6 |
|---|---|
| PUT | `Request::put` — `src/http/request.rs:247` |
| arbitrary verbs (HEAD, OPTIONS) | `RequestBuilder::method(Method)` — `request.rs:141` |
| request headers | `RequestBuilder::header(k, v)` — `request.rs:78` |
| response status | `Response::status()` — `src/http/response.rs:54` |
| response headers | `Response::headers()` — `response.rs:75` |
| raw body bytes | `Response::binary()` — `response.rs:117` |

Note that **`Request::put` has existed all along**. The PUT-degrades-to-GET defect is
purely a missing match arm, not a library limitation — making this the cheapest of the
three fixes.

### Churn mitigation on the shells

Roughly 30 plugins per shell construct `PluginResponse` with a String. Rather than
editing each, add a convenience constructor in the hand-written shell code so existing
plugins compile **unchanged**:

- **Swift:** `extension PluginResponse { init(ok: Bool, output: String) }` delegating to
  the generated byte initialiser.
- **Kotlin:** a top-level pseudo-constructor `fun PluginResponse(ok: Boolean, output: String)`
  (Kotlin permits a function named like the type).
- **Rust:** `PluginResponse::text(ok, s)` helper.

With these in place only the HTTP plugin on each shell needs a substantive edit.

## Error handling

| Situation | Outcome |
|---|---|
| 2xx | `Response { status, .. }`, `is_success() == true`, `ok == true` |
| 4xx / 5xx | `Response { status, .. }`, `is_success() == false`, `ok == false` |
| Offline / DNS / TLS / refused / timeout | `TransportError { message }`, `ok == false` |
| Malformed request envelope | `TransportError { message }` — the request was never attempted |
| Unknown HTTP verb (web) | `TransportError { message }` — explicitly not a silent GET |

`TransportError.message` is diagnostic text for logging, not for control flow. Apps
branch on the variant, not the string.

## Testing

**mobiler-core (unit).**
- Builder emits the expected `PluginCall` — method, URL, headers array, body.
- Extend the existing verb test at `mobiler-core/src/lib.rs:1335-1336` to include PUT.
- `HttpOutcome` bincode round-trip, both variants.
- Accessors: `text()` returns `None` on non-UTF-8; `header()` is case-insensitive;
  `is_success()` boundaries at 199/200/299/300.

**Web (integration).** The existing headless-chrome/CDP recipe against a local test
server: assert PUT arrives as PUT, headers arrive intact, a 409 surfaces as
`Response { status: 409 }`, and a connection-refused surfaces as `TransportError`.

**iOS / Android.** The per-demo CI build lanes cover compilation. One demo gets a
device pass for a real authed request.

**Stoa integration — the acceptance test.** Point `stoa-app` at a local `stoa-ds` and
verify end to end:
1. `PUT /access-key` with a bearer succeeds (today: unreachable).
2. A deliberate epoch conflict on `POST /commit` surfaces as `status: 409`,
   distinguishable from a stopped server surfacing as `TransportError`.
3. Remove the deferred TODO at `stoa-app/src/net.rs:22-28` and transmit the bearer.

## Release process

Two-stage, per the repo's ABI convention.

`mobiler-ui` is **not** affected — HTTP is not a UI concern. Precedent: the C5
streaming release bumped core and web while ui stayed at 0.15.

**Stage 1 — PR-A (libs).** `mobiler-core` 0.31.0 → 0.32.0, `mobiler-web` 0.31.0 →
0.32.0. Includes the two demo updates. Then run the `release-libs` skill.

**Stage 2 — PR-C (CLI).** iOS and Android template arms, core pin bumped to 0.32,
`mobiler` CLI 0.47.0 → 0.48.0. Then run the `release-cli` skill, tag `v0.48.0`.

Land each PR with the `ship-pr` skill. Run `post-release` after publishing.

## Explicitly out of scope

Deferred, but the envelope accommodates each without a further ABI change:

- **Timeouts** (`timeout_ms`) — add as an envelope field and a `.timeout()` builder link.
- **Binary request bodies** — Release B handles large uploads by file path; a small
  base64 body field can be added to the envelope if a middle-band case appears.
- **Query-parameter helpers** — callers build URLs themselves for now.
- **Redirect policy, cookie jar, retries** — platform defaults apply.
- **Response streaming, progress, cancellation** — Release B.
