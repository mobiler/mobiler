# Mobiler HTTP Capability — Release B (streaming transfers) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add `cx.upload` / `cx.download` streaming transfers that move large files by string handle (path / URI / `blob:`) without the bytes ever crossing the FFI, with per-tick progress and free cancellation.

**Architecture:** Transfers ride the shipped `cx.subscribe` / `cx.unsubscribe` streaming primitive. A builder terminates in a subscription to a new streaming `transfer` plugin; each stream event is a `TransferEvent` (`Progress` | `Done`) bincoded inside the stream's `PluginResponse.output`, exactly as Release A put `HttpOutcome` there. No `PluginCall` / `PluginResponse` struct changes.

**Tech Stack:** Rust (crux_core 0.18, facet, serde), Leptos/WASM + `web_sys` XHR/fetch (web shell), Swift/`URLSession` upload+download tasks (iOS shell), Kotlin/OkHttp + `callbackFlow` (Android shell).

**Spec:** `docs/superpowers/specs/2026-07-18-mobiler-http-capability-b-streaming-design.md`

## Global Constraints

- Encode/decode `TransferEvent` **only** via `crux_core::bridge::{BincodeFfiFormat, FfiFormat}` — never `bincode` directly, and add no `bincode` dependency. crux pins bincode `=1.3` fixint; bincode 2.x `config::standard()` is varint and silently emits bytes the generated Swift/Kotlin decoders cannot read. (Release A lesson.)
- `TransferEvent` nests `HttpOutcome` (already a registered shared type from Release A). Registering `TransferEvent` pulls it in.
- A type hidden inside `PluginResponse.output` (a `Vec<u8>`) is **not** reached by `TypeRegistry::register_app::<App>()` — it needs an explicit `.register_type::<TransferEvent>()?` in **every** app's `codegen.rs` (5 demos in PR-A + template in PR-C).
- Facet enums/structs need `#[repr(C)]` (cf. `HttpOutcome`, `mobiler-core/src/http.rs`).
- The streaming continuation is `Fn(PluginResponse) -> E` (fires per event), **not** `FnOnce`. The builder's `on_event` must therefore be `Fn`, and the decode-and-dispatch wrapper must be `Fn`.
- No tuples in shared types — named structs only.
- `PluginCall.input` stays a `String`; the transfer envelope is JSON serialized into it. `PluginResponse` / `PluginCall` structs do not change.
- `mobiler-ui` is **NOT** touched and **NOT** republished.
- Progress is throttled in each shell to ~10/sec (or on ≥1% change). The terminal `Done` is always emitted.
- `download` overwrites `dest` by default. On cancel or transport failure the shell **deletes** the partial download file.
- Upload is **raw body only** (the file is the body). No multipart in v1.
- Web asymmetry (deliberate, comment at both sites): **upload** uses `XMLHttpRequest` (only XHR has `upload.onprogress`); **download** uses `fetch` + `ReadableStream`. Web download returns a `blob:` handle; `dest` is a filename hint on web, a real sandbox path on iOS/Android.
- Two-stage release. **PR-A** (Tasks 1–6): core + web + demos, bump `mobiler-core` 0.32.0 → **0.33.0** and `mobiler-web` 0.32.0 → **0.33.0**, then `release-libs`. **PR-C** (Tasks 7–9): the `transfer` plugin + template registration + core pin + `mobiler` 0.48.1 → **0.49.0**, then `release-cli`, tag **v0.49.0**.
- Land every PR with `ship-pr`. `main` is protected — always branch. The whole-branch review must explicitly compare the three shells' progress / cancel / terminal semantics.
- `mobiler upgrade` acceptance is a build with **zero manual steps** (0.48.1 lesson).

## File Structure

**PR-A — libraries and demos**

| File | Responsibility |
|---|---|
| `mobiler-core/src/transfer.rs` *(new)* | `TransferEvent`, its `encode`/`decode`, accessors, `TransferBuilder`, `Cx::upload`/`download`. Keeps `http.rs`/`lib.rs` focused. |
| `mobiler-core/src/lib.rs` | `pub mod transfer;` + re-export `TransferEvent`. |
| `mobiler-web/src/lib.rs` | `start_stream` gains `("transfer","upload")` + `("transfer","download")` arms; `StreamHandle::Transfer`; a `TransferEvent`-encoding helper. |
| 5 × demo `codegen.rs` | `.register_type::<mobiler_core::TransferEvent>()?`. |
| `demos/barbershop/…` (app-core + web render) | A "Transfer" demo card: pick a file → upload with a progress bar → download it back. Web-functional in PR-A. |
| `mobiler-core/Cargo.toml`, `mobiler-web/Cargo.toml` | 0.32.0 → 0.33.0 (and web's core dep). |

**PR-C — the transfer plugin + CLI**

| File | Responsibility |
|---|---|
| `mobiler/plugins/transfer/mobiler-plugin.toml` *(new)* | Streaming plugin manifest (register + register_stream), modeled on `websocket`. |
| `mobiler/plugins/transfer/ios/TransferPlugin.swift` *(new)* | `URLSessionUpload`/`DownloadTask` + delegate; emits `TransferEvent`. |
| `mobiler/plugins/transfer/android/TransferPlugin.kt` *(new)* | OkHttp streaming upload `RequestBody` + download sink loop; `callbackFlow`. |
| `mobiler/plugins/transfer/README.md`, `app-core-usage.rs` *(new)* | Docs + usage snippet (bundled-plugin convention). |
| `mobiler/templates/shared/src/bin/codegen.rs` | `.register_type::<mobiler_core::TransferEvent>()?`. |
| barbershop native (via `mobiler plugin add transfer`) | Wire the plugin into the demo so native lanes build it + device test. |
| `mobiler/templates/**/Cargo.toml*`, `mobiler/Cargo.toml` | core pin → 0.33, CLI → 0.49.0. |
| `mobiler/plugins/files/{android,ios}` | Point the `files` `download` op at the shared transfer path (subsume the blocking workaround). |

---

# PR-A — libraries and demos

### Task 1: `TransferEvent` type

**Files:**
- Create: `mobiler-core/src/transfer.rs`
- Modify: `mobiler-core/src/lib.rs` (add `pub mod transfer;` + re-export)
- Test: inline `#[cfg(test)]` in `mobiler-core/src/transfer.rs`

**Interfaces:**
- Consumes: `crate::http::HttpOutcome` (Release A, re-exported from crate root).
- Produces: `TransferEvent::Progress { transferred: u64, total: Option<u64> }`; `TransferEvent::Done { outcome: HttpOutcome, handle: Option<String> }`; `TransferEvent::encode(&self) -> Vec<u8>`; `TransferEvent::decode(&[u8]) -> Result<Self, String>`.

- [ ] **Step 1: Write the failing test**

Create `mobiler-core/src/transfer.rs` with only the test module:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::http::{HttpHeader, HttpOutcome};

    #[test]
    fn round_trips_progress_with_and_without_total() {
        for ev in [
            TransferEvent::Progress { transferred: 0, total: Some(1024) },
            TransferEvent::Progress { transferred: 999_999_999, total: None },
        ] {
            assert_eq!(TransferEvent::decode(&ev.encode()).unwrap(), ev);
        }
    }

    #[test]
    fn round_trips_done_upload_and_download() {
        let up = TransferEvent::Done {
            outcome: HttpOutcome::Response { status: 200, headers: vec![], body: vec![] },
            handle: None,
        };
        let down = TransferEvent::Done {
            outcome: HttpOutcome::Response {
                status: 200,
                headers: vec![HttpHeader { name: "Content-Length".into(), value: "5".into() }],
                body: vec![],
            },
            handle: Some("blob:abc".into()),
        };
        for ev in [up, down] {
            assert_eq!(TransferEvent::decode(&ev.encode()).unwrap(), ev);
        }
    }

    #[test]
    fn decode_rejects_garbage_without_panicking() {
        assert!(TransferEvent::decode(&[0xff, 0xff, 0xff]).is_err());
    }
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p mobiler-core --lib transfer::`
Expected: FAIL — compile error, `TransferEvent` not found.

- [ ] **Step 3: Write minimal implementation**

Prepend to `mobiler-core/src/transfer.rs`:

```rust
//! Streaming file transfers (`cx.upload` / `cx.download`).
//!
//! Large transfers move by string **handle** (a filesystem path, a `content://` /
//! `file://` URI, or a web `blob:` URL) — the bytes never cross the FFI. Each transfer
//! rides the streaming primitive ([`Cx::subscribe`](crate::Cx::subscribe)) and delivers
//! a [`TransferEvent`] per progress tick and once at completion.

use facet::Facet;
use serde::{Deserialize, Serialize};

use crate::http::HttpOutcome;

/// One event from an in-flight transfer, bincoded into the stream's
/// `PluginResponse.output` (the same pattern Release A uses for [`HttpOutcome`]).
#[derive(Facet, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[repr(C)]
pub enum TransferEvent {
    /// A progress tick. `total` is `None` when the size is unknown (chunked response).
    Progress { transferred: u64, total: Option<u64> },
    /// The transfer finished. `outcome` is Release A's request result; for a download
    /// its `body` is empty (bytes went to disk) and `handle` is the destination the
    /// shell wrote (sandbox path on native, `blob:` URL on web). `handle` is `None` for
    /// an upload.
    Done { outcome: HttpOutcome, handle: Option<String> },
}

impl TransferEvent {
    /// Serialize for the stream payload. Uses crux's FFI format, not bincode directly —
    /// see [`HttpOutcome::encode`](crate::http::HttpOutcome::encode) for why the version
    /// and config matter.
    pub fn encode(&self) -> Vec<u8> {
        use crux_core::bridge::{BincodeFfiFormat, FfiFormat};
        let mut buffer = Vec::new();
        BincodeFfiFormat::serialize(&mut buffer, self).expect("encode TransferEvent");
        buffer
    }

    /// Decode a stream payload produced by a shell's `transfer` plugin.
    pub fn decode(bytes: &[u8]) -> Result<Self, String> {
        use crux_core::bridge::{BincodeFfiFormat, FfiFormat};
        BincodeFfiFormat::deserialize(bytes).map_err(|e| e.to_string())
    }
}
```

Add to `mobiler-core/src/lib.rs` beside the `pub mod http;` line:

```rust
pub mod transfer;
pub use transfer::TransferEvent;
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test -p mobiler-core --lib transfer::`
Expected: PASS, 3 tests.

- [ ] **Step 5: Commit**

```bash
git add mobiler-core/src/transfer.rs mobiler-core/src/lib.rs
git commit -m "feat(core): TransferEvent — progress + done payload for streaming transfers"
```

---

### Task 2: `TransferBuilder` + `cx.upload` / `cx.download`

**Files:**
- Modify: `mobiler-core/src/transfer.rs` (append the builder)
- Modify: `mobiler-core/src/lib.rs` (add `upload`/`download` to `impl<E> Cx<E>`)
- Test: inline `#[cfg(test)]` in `mobiler-core/src/lib.rs`

**Interfaces:**
- Consumes: `TransferEvent` (Task 1); `Cx::subscribe(key, plugin, op, input, on_event)` (shipped).
- Produces: `Cx::upload(url, source) -> TransferBuilder<'_, E>`; `Cx::download(url, dest) -> TransferBuilder<'_, E>`; `TransferBuilder::{method, header, bearer, start}`; `start(key, on_event) -> String` where `on_event: Fn(TransferEvent) -> E + Send + 'static`.

- [ ] **Step 1: Write the failing test**

Add to the `mod tests` in `mobiler-core/src/lib.rs`:

```rust
#[test]
fn upload_builder_emits_transfer_stream_call() {
    let mut cx = Cx::<Ev>::default();
    let key = cx.upload("https://h/put", "file:///tmp/a.enc")
        .bearer("tok")
        .header("Content-Type", "application/octet-stream")
        .start("up-1", |_ev| Ev::Tap);

    assert_eq!(key, "up-1");
    assert_eq!(cx.streams.len(), 1);
    let (call, _) = &cx.streams[0];
    assert_eq!(call.key, "up-1");
    assert_eq!(call.plugin, "transfer");
    assert_eq!(call.op, "upload");

    let v: serde_json::Value = serde_json::from_str(&call.input).unwrap();
    assert_eq!(v["url"], "https://h/put");
    assert_eq!(v["source"], "file:///tmp/a.enc");
    assert_eq!(v["method"], "PUT");                       // default
    assert_eq!(v["headers"][0]["name"], "Authorization");
    assert_eq!(v["headers"][0]["value"], "Bearer tok");
    assert_eq!(v["headers"][1]["name"], "Content-Type");
}

#[test]
fn download_builder_uses_dest_and_no_default_method() {
    let mut cx = Cx::<Ev>::default();
    cx.download("https://h/get", "/data/att-9.enc").start("dl-1", |_| Ev::Tap);
    let (call, _) = &cx.streams[0];
    assert_eq!(call.op, "download");
    let v: serde_json::Value = serde_json::from_str(&call.input).unwrap();
    assert_eq!(v["dest"], "/data/att-9.enc");
    assert!(v.get("source").is_none());
}

#[test]
fn start_continuation_decodes_progress_and_done() {
    use crate::http::HttpOutcome;
    #[derive(Debug, PartialEq)]
    enum Got { Prog(u64), Done(u16), Bad }

    let mut cx = Cx::<GotEv>::default();
    cx.download("https://h/get", "/d").start("k", |ev| match ev {
        TransferEvent::Progress { transferred, .. } => GotEv(Got::Prog(transferred)),
        TransferEvent::Done { outcome, .. } => GotEv(match outcome.status() {
            Some(s) => Got::Done(s),
            None => Got::Bad,
        }),
    });
    let (_, cont) = &cx.streams[0];

    let prog = TransferEvent::Progress { transferred: 512, total: Some(1024) };
    assert_eq!(cont(PluginResponse { ok: true, output: prog.encode() }), GotEv(Got::Prog(512)));

    let done = TransferEvent::Done {
        outcome: HttpOutcome::Response { status: 201, headers: vec![], body: vec![] },
        handle: Some("/d".into()),
    };
    assert_eq!(cont(PluginResponse { ok: true, output: done.encode() }), GotEv(Got::Done(201)));
}
```

Add near the other test helpers in `lib.rs` (only if `Ev` alone is insufficient — a wrapper newtype so the continuation's return type is `PartialEq`):

```rust
#[derive(Debug, PartialEq)]
struct GotEv(Got);
```

If defining `Got`/`GotEv` at module scope is awkward, inline them inside the test fn (Rust allows local enums/structs in a fn body); adjust so the test compiles. The behavior asserted — `start` returns the key, and the stored continuation decodes each `TransferEvent` and dispatches — is what matters.

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p mobiler-core --lib upload_builder download_builder start_continuation`
Expected: FAIL — `cx.upload`/`download` not found.

- [ ] **Step 3: Write minimal implementation**

Append to `mobiler-core/src/transfer.rs`:

```rust
use crate::{Cx, HttpHeader, PluginResponse};

/// Wire shape of a transfer request, serialized into the stream call's `input`.
/// Exactly one of `source` (upload) / `dest` (download) is set.
#[derive(Serialize)]
struct TransferReq {
    url: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    source: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    dest: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    method: Option<String>,
    headers: Vec<HttpHeader>,
}

/// Builds a streaming transfer. Obtained from [`Cx::upload`] / [`Cx::download`];
/// finished with [`start`](Self::start), which subscribes and returns the key.
pub struct TransferBuilder<'a, E> {
    cx: &'a mut Cx<E>,
    op: &'static str, // "upload" | "download"
    req: TransferReq,
}

impl<'a, E> TransferBuilder<'a, E> {
    pub(crate) fn upload(cx: &'a mut Cx<E>, url: String, source: String) -> Self {
        Self {
            cx,
            op: "upload",
            req: TransferReq { url, source: Some(source), dest: None, method: Some("PUT".into()), headers: Vec::new() },
        }
    }

    pub(crate) fn download(cx: &'a mut Cx<E>, url: String, dest: String) -> Self {
        Self {
            cx,
            op: "download",
            req: TransferReq { url, source: None, dest: Some(dest), method: None, headers: Vec::new() },
        }
    }

    /// Override the upload method (default `PUT`). No effect on download.
    #[must_use]
    pub fn method(mut self, m: impl Into<String>) -> Self {
        if self.op == "upload" {
            self.req.method = Some(m.into());
        }
        self
    }

    /// Add a request header (order preserved, repeats allowed).
    #[must_use]
    pub fn header(mut self, name: impl Into<String>, value: impl Into<String>) -> Self {
        self.req.headers.push(HttpHeader { name: name.into(), value: value.into() });
        self
    }

    /// Sugar for `header("Authorization", format!("Bearer {token}"))`.
    #[must_use]
    pub fn bearer(self, token: impl AsRef<str>) -> Self {
        self.header("Authorization", format!("Bearer {}", token.as_ref()))
    }

    /// Subscribe under `key`. `on_event` fires per progress tick and once at `Done`.
    /// Returns `key` so the caller can [`cx.unsubscribe(key)`](crate::Cx::unsubscribe).
    pub fn start(self, key: impl Into<String>, on_event: impl Fn(TransferEvent) -> E + Send + 'static) -> String {
        let key = key.into();
        let input = serde_json::to_string(&self.req).expect("serialize transfer request");
        self.cx.subscribe(key.clone(), "transfer", self.op, input, move |r: PluginResponse| {
            on_event(decode_event(&r))
        });
        key
    }
}

/// Decode a stream payload. A shell that emits something undecodable is a bug, but it
/// must not panic the app — surface it as a completed transport error.
fn decode_event(r: &PluginResponse) -> TransferEvent {
    TransferEvent::decode(&r.output).unwrap_or_else(|e| TransferEvent::Done {
        outcome: HttpOutcome::TransportError { message: format!("malformed transfer event: {e}") },
        handle: None,
    })
}
```

Add to `impl<E> Cx<E>` in `mobiler-core/src/lib.rs` (near `subscribe`):

```rust
    /// Upload the file at `source` (a path / `content://` / `file://` / `blob:` handle)
    /// as the raw request body. Finish with [`TransferBuilder::start`].
    pub fn upload(&mut self, url: impl Into<String>, source: impl Into<String>) -> crate::transfer::TransferBuilder<'_, E> {
        crate::transfer::TransferBuilder::upload(self, url.into(), source.into())
    }

    /// Download `url` to `dest` (a sandbox path on iOS/Android; a filename hint on web,
    /// which returns a `blob:` handle). Finish with [`TransferBuilder::start`].
    pub fn download(&mut self, url: impl Into<String>, dest: impl Into<String>) -> crate::transfer::TransferBuilder<'_, E> {
        crate::transfer::TransferBuilder::download(self, url.into(), dest.into())
    }
```

Confirm `HttpHeader` is re-exported from the crate root (Release A did this). If not, import it in `transfer.rs` via `crate::http::HttpHeader`.

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test -p mobiler-core --lib`
Expected: PASS, whole crate.

- [ ] **Step 5: Commit**

```bash
git add mobiler-core/src/transfer.rs mobiler-core/src/lib.rs
git commit -m "feat(core): cx.upload / cx.download streaming-transfer builders"
```

---

### Task 3: Register `TransferEvent` for codegen (demos)

**Files:**
- Modify: `demos/barbershop/shared/src/bin/codegen.rs`
- Modify: `demos/coffee/shared/src/bin/codegen.rs`
- Modify: `demos/fullstack-todo/mobile/shared/src/bin/codegen.rs`
- Modify: `demos/saldo/shared/src/bin/codegen.rs`
- Modify: `demos/todo/shared/src/bin/codegen.rs`

(The template's `codegen.rs` is Task 8, in PR-C.)

**Interfaces:**
- Consumes: `TransferEvent` (Task 1).
- Produces: generated `TransferEvent` Swift/Kotlin types in each demo's `SharedTypes`, which Tasks 4 (web reads the Rust type directly) and 7 (native shells) rely on.

`TransferEvent` rides inside a `Vec<u8>`, so `register_app::<App>()` cannot reach it by traversal — the same trap as `HttpOutcome`. These are per-app files.

- [ ] **Step 1: Make the change in all five files**

Each demo's `codegen.rs` already has (from Release A):

```rust
    let typegen_app = TypeRegistry::new()
        .register_app::<App>()?
        .register_type::<mobiler_core::HttpOutcome>()?
        .build()?;
```

Add the `TransferEvent` line:

```rust
    let typegen_app = TypeRegistry::new()
        .register_app::<App>()?
        .register_type::<mobiler_core::HttpOutcome>()?
        .register_type::<mobiler_core::TransferEvent>()?
        .build()?;
```

- [ ] **Step 2: Verify codegen emits the type**

```bash
cd demos/saldo/shared && cargo run --bin codegen -- --language swift --output-dir /tmp/tg-b && grep -rl "TransferEvent" /tmp/tg-b
cargo run --bin codegen -- --language kotlin --output-dir /tmp/tg-b-kt && grep -rl "TransferEvent" /tmp/tg-b-kt
```

Expected: hits in both. **Record, verbatim in the task report, the generated spelling for BOTH languages** — the enum case constructors (`TransferEvent.progress(...)` vs `.Progress(...)`), the field types (is `transferred` `UInt64`/`ULong`? is `total` an optional?), and the serialize method (`bincodeSerialize()`), because Tasks 4 and 7 write code against them and must not guess. Return to repo root when done.

- [ ] **Step 3: Commit**

```bash
git add demos/*/shared/src/bin/codegen.rs demos/*/*/shared/src/bin/codegen.rs
git commit -m "build(demos): register TransferEvent for Swift/Kotlin codegen"
```

---

### Task 4: Web shell — the `transfer` stream source

**Files:**
- Modify: `mobiler-web/src/lib.rs` (`start_stream` arms + `StreamHandle` + a helper)
- Test: the headless-Chrome/CDP integration recipe (Step 4)

**Interfaces:**
- Consumes: `TransferEvent`, `HttpOutcome`, `HttpHeader` (Tasks 1 / Release A); the `start_stream` `(plugin, op)` dispatch + `STREAMS` per-key registry + `StreamHandle` enum (shipped).
- Produces: a working `("transfer","upload")` (XHR) and `("transfer","download")` (fetch/ReadableStream) source that emits `TransferEvent`-bearing `PluginResponse`s and is cancelled by dropping its `StreamHandle`.

- [ ] **Step 1: Add the `StreamHandle` variant and the encode helper**

In `mobiler-web/src/lib.rs`, extend the `enum StreamHandle` (around line 348) with:

```rust
    /// An in-flight transfer — held so dropping it (on unsubscribe) aborts the XHR /
    /// cancels the fetch reader.
    Transfer(TransferHandle),
```

and define near it:

```rust
/// Holds a web transfer so unsubscribe can abort it. For upload we keep the
/// `XmlHttpRequest` (call `.abort()` on drop via the Drop impl); for download we keep an
/// `AbortController` whose `.abort()` cancels the fetch + reader.
struct TransferHandle {
    xhr: Option<web_sys::XmlHttpRequest>,
    abort: Option<web_sys::AbortController>,
    // Closures must outlive the request; parked here so they aren't dropped early.
    _keepalive: Vec<wasm_bindgen::JsValue>,
}
impl Drop for TransferHandle {
    fn drop(&mut self) {
        if let Some(x) = &self.xhr { let _ = x.abort(); }
        if let Some(a) = &self.abort { a.abort(); }
    }
}

/// Bincode a TransferEvent into a stream `PluginResponse` (mirrors Release A's http encode).
fn transfer_response(ev: &mobiler_core::TransferEvent) -> PluginResponse {
    PluginResponse { ok: matches!(ev, mobiler_core::TransferEvent::Done { outcome, .. } if outcome.is_success()), output: ev.encode() }
}
```

Add `use mobiler_core::{HttpHeader, HttpOutcome, TransferEvent};` to the imports (extend the Release A import line). Enable the `web_sys` features `XmlHttpRequest`, `ProgressEvent`, `AbortController`, `ReadableStream`, `Response as WebResponse` etc. in `mobiler-web/Cargo.toml` under `[dependencies.web-sys] features = [...]` — add whichever the compiler reports missing.

- [ ] **Step 2: Add the two `start_stream` arms**

Insert before the `_ => return` arm in `start_stream` (mobiler-web/src/lib.rs ~line 328). Parse the envelope, then branch:

```rust
        ("transfer", op @ ("upload" | "download")) => {
            let v: serde_json::Value = serde_json::from_str(&call.input).unwrap_or(serde_json::Value::Null);
            let url = v.get("url").and_then(|x| x.as_str()).unwrap_or("").to_string();
            let headers: Vec<(String, String)> = v.get("headers").and_then(|x| x.as_array()).map(|hs| {
                hs.iter().filter_map(|h| Some((h.get("name")?.as_str()?.to_string(), h.get("value")?.as_str()?.to_string()))).collect()
            }).unwrap_or_default();

            if op == "upload" {
                // Web upload: XHR is the ONLY API with upload progress (fetch has none).
                let method = v.get("method").and_then(|x| x.as_str()).unwrap_or("PUT").to_string();
                let source = v.get("source").and_then(|x| x.as_str()).unwrap_or("").to_string();
                start_web_upload(url, method, headers, source, emit.clone())
            } else {
                // Web download: fetch + ReadableStream reports progress; returns a blob: handle.
                start_web_download(url, headers, emit.clone())
            }
        }
```

Then implement the two helpers (below the fn), each returning a `StreamHandle::Transfer`. Full code — upload via XHR, download via fetch reader — with **~10/sec progress throttling** and a terminal `TransferEvent::Done`:

```rust
fn start_web_upload(
    url: String, method: String, headers: Vec<(String, String)>, source: String,
    emit: impl Fn(PluginResponse) + Clone + 'static,
) -> StreamHandle {
    use wasm_bindgen::{closure::Closure, JsCast};
    let xhr = web_sys::XmlHttpRequest::new().expect("xhr");
    let _ = xhr.open_with_async(&method, &url, true);
    for (n, val) in &headers { let _ = xhr.set_request_header(n, val); }

    let last = std::rc::Rc::new(std::cell::Cell::new(0.0f64));
    let on_prog = {
        let (emit, last) = (emit.clone(), last.clone());
        Closure::<dyn FnMut(web_sys::ProgressEvent)>::new(move |e: web_sys::ProgressEvent| {
            let now = js_now();
            if now - last.get() < 100.0 && e.loaded() < e.total() { return; } // ~10/sec
            last.set(now);
            let total = if e.length_computable() { Some(e.total() as u64) } else { None };
            emit(transfer_response(&TransferEvent::Progress { transferred: e.loaded() as u64, total }));
        })
    };
    xhr.upload().unwrap().set_onprogress(Some(on_prog.as_ref().unchecked_ref()));

    let on_done = {
        let (emit, xhr_c) = (emit.clone(), xhr.clone());
        Closure::<dyn FnMut()>::new(move || {
            let status = xhr_c.status().unwrap_or(0);
            let outcome = if status == 0 {
                HttpOutcome::TransportError { message: "upload failed".into() }
            } else {
                HttpOutcome::Response { status, headers: vec![], body: vec![] }
            };
            emit(transfer_response(&TransferEvent::Done { outcome, handle: None }));
        })
    };
    xhr.set_onload(Some(on_done.as_ref().unchecked_ref()));
    let on_err = {
        let emit = emit.clone();
        Closure::<dyn FnMut()>::new(move || {
            emit(transfer_response(&TransferEvent::Done {
                outcome: HttpOutcome::TransportError { message: "upload error".into() }, handle: None }));
        })
    };
    xhr.set_onerror(Some(on_err.as_ref().unchecked_ref()));

    // Fetch the blob: source back into a Blob and send it. (Spawned; the XHR send fires
    // once the blob resolves.) For a same-document blob: URL this is synchronous-ish.
    let xhr_send = xhr.clone();
    wasm_bindgen_futures::spawn_local(async move {
        if let Some(blob) = fetch_blob(&source).await {
            let _ = xhr_send.send_with_opt_blob(Some(&blob));
        } else {
            let _ = xhr_send.send(); // no body
        }
    });

    StreamHandle::Transfer(TransferHandle {
        xhr: Some(xhr),
        abort: None,
        _keepalive: vec![on_prog.into_js_value(), on_done.into_js_value(), on_err.into_js_value()],
    })
}
```

```rust
fn start_web_download(
    url: String, headers: Vec<(String, String)>,
    emit: impl Fn(PluginResponse) + Clone + 'static,
) -> StreamHandle {
    let ctrl = web_sys::AbortController::new().expect("abortcontroller");
    let signal = ctrl.signal();
    let emit2 = emit.clone();
    wasm_bindgen_futures::spawn_local(async move {
        match fetch_stream(&url, &headers, &signal).await {
            Ok((status, resp_headers, total, mut reader)) => {
                let mut got: u64 = 0;
                let mut chunks: Vec<u8> = Vec::new();
                let mut last = js_now();
                while let Some(chunk) = reader.next().await {
                    got += chunk.len() as u64;
                    chunks.extend_from_slice(&chunk);
                    let now = js_now();
                    if now - last >= 100.0 { // ~10/sec
                        last = now;
                        emit2(transfer_response(&TransferEvent::Progress { transferred: got, total }));
                    }
                }
                let handle = make_blob_url(&chunks); // blob: URL the app can render/save
                let outcome = HttpOutcome::Response { status, headers: resp_headers, body: vec![] };
                emit2(transfer_response(&TransferEvent::Done { outcome, handle: Some(handle) }));
            }
            Err(msg) => emit2(transfer_response(&TransferEvent::Done {
                outcome: HttpOutcome::TransportError { message: msg }, handle: None })),
        }
    });
    StreamHandle::Transfer(TransferHandle { xhr: None, abort: Some(ctrl), _keepalive: vec![] })
}
```

The helpers `js_now()` (a monotonic `performance.now()`), `fetch_blob(url) -> Option<Blob>`, `send_with_opt_blob`, `fetch_stream(...) -> Result<(u16, Vec<HttpHeader>, Option<u64>, Reader), String>`, `make_blob_url(&[u8]) -> String`, and a minimal async chunk `Reader` over `ReadableStreamDefaultReader` are small wrappers — implement them in the same file. `make_blob_url` mirrors `take_image`'s `Url::create_object_url_with_blob`. If `web_sys`'s `ReadableStream` reader ergonomics prove heavy, an acceptable fallback for the download body is `Response::array_buffer()` **without** mid-stream progress — but then emit a single `Progress { transferred: total, total }` before `Done`, and note the degradation in the report rather than silently shipping no progress.

- [ ] **Step 3: Fix the web unsubscribe path if needed**

Confirm the existing `notify("stream","unsubscribe", key)` handler drops the `STREAMS[key]` entry (it does for ticker/websocket/system). Since `TransferHandle: Drop` aborts on drop, unsubscribe cancels the transfer with no extra code. Verify by reading the unsubscribe handler; if it only `.remove()`s the map entry, that is exactly what triggers `Drop`. No change expected — confirm and note it.

- [ ] **Step 4: Verify end-to-end in headless Chrome**

Stand up a throwaway server that accepts a large `PUT` (echoing `Content-Length`) and serves a large `GET` with a known body, then drive the web shell against it via the repo's headless-chrome/CDP recipe (see the `mobiler-web shell` memory). Assert:
1. An upload emits ≥2 `Progress` ticks then a `Done { Response{status:200} }`.
2. A download emits `Progress` ticks then a `Done` whose `handle` is a `blob:` URL, and fetching that blob yields the known body.
3. `cx.unsubscribe` mid-download aborts (server sees a dropped connection; no `Done{Response}` arrives, or a `Done{TransportError}` does).
Build: `cd mobiler-web && cargo check --target wasm32-unknown-unknown && cargo clippy -- -D warnings`.

- [ ] **Step 5: Commit**

```bash
git add mobiler-web/src/lib.rs mobiler-web/Cargo.toml
git commit -m "feat(web): transfer stream source — XHR upload progress, fetch download to blob"
```

---

### Task 5: Demo consumer — barbershop "Transfer" card

**Files:**
- Modify: `demos/barbershop/shared/src/app.rs` (or wherever its app-core lives — confirm with `ls demos/barbershop/shared/src`)
- Modify: barbershop's view builder for the card + progress UI
- Test: web render + the Task 4 harness

**Interfaces:**
- Consumes: `cx.upload`/`download` (Task 2), the file-picker capability (existing `filepicker` plugin, or `cx`'s photo pick) for a `blob:`/URI source.
- Produces: a visible demo: pick a file → upload with a progress bar → on done, download it back and show the result. Proves the capability in a real app and gives all three shells something to compile.

- [ ] **Step 1: Add model + events**

Add to barbershop's model: `transfer_pct: Option<u8>`, `transfer_note: String`. Add events `PickForUpload`, `Picked(String)`, `UpProgress { transferred: u64, total: Option<u64> }`, `UpDone(/* status */ u16)`, `DlProgress {..}`, `DlDone { handle: Option<String>, status: u16 }`. (Crux serializes Msg opaquely, so adding variants is a 0-byte codegen diff — the shells don't reference Msg variants.)

- [ ] **Step 2: Wire the flow**

On `Picked(handle)`, call `cx.upload(UPLOAD_URL, handle).start("bx-up", |ev| match ev { Progress{transferred,total} => Msg::UpProgress{transferred,total}, Done{outcome,..} => Msg::UpDone(outcome.status().unwrap_or(0)) })`. On `UpDone(200)`, kick a `cx.download(DOWNLOAD_URL, "att.bin").start("bx-dl", ...)`. Map progress to `transfer_pct = total.map(|t| (transferred*100/t) as u8)`. Use a demo endpoint that accepts this (e.g. `https://httpbin.org/put` + `/bytes/…`) or a documented placeholder constant with a comment that a real server is needed on device.

- [ ] **Step 3: Render the card**

Add a card to a barbershop tab (Profile is already crowded — put it on Home or a new "More" area) showing a button ("Send a file"), the `transfer_note`, and a progress bar bound to `transfer_pct` (reuse the existing `Progress`/`Skeleton` widget vocabulary — check `mobiler-ui` for the progress widget name).

- [ ] **Step 4: Verify**

`cargo build`/`test` the barbershop workspace. Drive the web shell (Task 4 harness or the demo's own `web/`) and confirm the progress bar animates and the note updates. On iOS/Android the `transfer` plugin does not exist until PR-C, so the stream is a no-op there (the card renders, the button does nothing) — this is expected and must be stated in the report; native wiring + device test is Task 9.

- [ ] **Step 5: Commit**

```bash
git add demos/barbershop/
git commit -m "feat(barbershop): Transfer demo card — upload+download with a progress bar (web-functional)"
```

---

### Task 6: Version bumps, README audit, ship PR-A

**Files:**
- Modify: `mobiler-core/Cargo.toml` (0.32.0 → 0.33.0), `mobiler-web/Cargo.toml` (0.32.0 → 0.33.0 + its `mobiler-core` dep → "0.33")
- Modify: `mobiler-core/README.md`, `mobiler-web/README.md`, `capabilities.json`

**Interfaces:**
- Consumes: Tasks 1–5.
- Produces: green tree across every workspace; published `mobiler-core` 0.33.0 + `mobiler-web` 0.33.0.

- [ ] **Step 1: Verify every workspace** (root is only `mobiler`/`mobiler-ui`/`mobiler-core`/`xtask`; web + demos are separate — build each explicitly, per Release A's Task 8):

```bash
cargo build --workspace && cargo test --workspace && cargo clippy -p mobiler-core -p mobiler-ui -p xtask -- -D warnings
(cd mobiler-web && cargo check --target wasm32-unknown-unknown && cargo clippy -- -D warnings)
for d in demos/*/ demos/*/mobile/; do [ -f "$d/Cargo.toml" ] && (cd "$d" && echo "--- $d" && cargo build && cargo test) || true; done
```

- [ ] **Step 2: Bump versions** — core + web to 0.33.0; leave `mobiler-ui` at 0.23.0.

- [ ] **Step 3: Update `capabilities.json` + regen** — add a row (or extend the HTTP row's note) for streaming transfers: `cx.upload / cx.download (streaming, progress + cancel)`. Run `cargo run -p xtask -- gen-readme` and confirm `--check` passes. Audit `mobiler-core`/`mobiler-web` README prose for any HTTP/transfer vocabulary.

- [ ] **Step 4: Ship PR-A** — use `ship-pr`; confirm ALL CI green (all iOS + Android + scaffold lanes — they prove the demo card compiles even though native transfer is a no-op until PR-C).

- [ ] **Step 5: Publish** — `release-libs` (`mobiler-core` then `mobiler-web`; return to repo root between crates — the LazyList `cd` gotcha). `mobiler-ui` skipped.

---

# PR-C — the transfer plugin + CLI

### Task 7: The `transfer` plugin (iOS + Android)

**Files:**
- Create: `mobiler/plugins/transfer/mobiler-plugin.toml`
- Create: `mobiler/plugins/transfer/ios/TransferPlugin.swift`
- Create: `mobiler/plugins/transfer/android/TransferPlugin.kt`
- Create: `mobiler/plugins/transfer/README.md`, `mobiler/plugins/transfer/app-core-usage.rs`

**Interfaces:**
- Consumes: the generated `TransferEvent` Swift/Kotlin types (Task 3's recorded spelling — read it, do not guess) and its `bincodeSerialize()`.
- Produces: a streaming `transfer` plugin whose `subscribe(op, input, emit)` emits `TransferEvent`-bearing `PluginResponse`s, modeled byte-for-byte on the `websocket` plugin's structure.

- [ ] **Step 1: Write the manifest** (model on `mobiler/plugins/websocket/mobiler-plugin.toml`):

```toml
name = "transfer"
summary = "Streaming file upload/download — progress + cancel, by path/URI/blob handle (free)"

[android]
sources = ["android/TransferPlugin.kt"]
register = '"transfer" to TransferPlugin(application)'
# Streaming is the plugin's polymorphic `subscribe()` override — no separate registration.
# OkHttp is already a template dep (HttpPlugin).

[ios]
sources = ["ios/TransferPlugin.swift"]
register = 'case "transfer": return await TransferPlugin.handle(op: op, input: input)'
register_stream = 'case "transfer": await TransferPlugin.subscribe(op: op, input: input, emit: emit)'
```

- [ ] **Step 2: iOS plugin** — `URLSessionUploadTask(fromFile:)` / `URLSessionDownloadTask` with a `URLSessionTaskDelegate`. `didSendBodyData` / `didWriteData` → throttled `TransferEvent.progress`; completion → `TransferEvent.done`. On download, move the temp file to `dest` (overwriting); on cancel/error delete the partial and emit `Done{TransportError}`. Encode each event with the generated `try? ev.bincodeSerialize() ?? []` into a `PluginResponse`, matching Task 3's recorded case spelling and `UInt64` field types. Throttle to ~10/sec. `subscribe(op:input:emit:)` static signature exactly as `WebSocketPlugin.subscribe`.

- [ ] **Step 3: Android plugin** — override `subscribe(op, input): Flow<PluginResponse> = callbackFlow { ... }` (websocket precedent). Upload: a custom `RequestBody` streaming from the source `Uri`/path (`contentResolver.openInputStream(Uri.parse(source))` for `content://`, else a `File`), reporting bytes via a counting sink → throttled `TransferEvent.Progress`. Download: `client.newCall(req).execute()`, then copy `body.source()` to the `dest` file sink in a loop that calls `ensureActive()` each iteration (so `streamJobs[key].cancel()` on unsubscribe actually stops the socket read); on cancellation or `IOException`, `dest`.delete() the partial and emit `Done{TransportError}`. Use `List<UByte>` for the generated `PluginResponse.output` and `ev.bincodeSerialize()` (returns `ByteArray`) — mind the `List<UByte>` vs `List<Byte>` trap from Release A; add a `ByteArray.toUByteList()` helper if the plugin constructs `PluginResponse` directly. Throttle to ~10/sec; `awaitClose { call.cancel() }`.

- [ ] **Step 4: Compile-check what can be checked** — Kotlin: extract the plugin against a stub matching Task 3's generated `TransferEvent` and compile with `kotlinc` (Release A Task 7 precedent), since a full gradle build needs the plugin wired into a demo (Task 9). iOS cannot be compiled on this Linux box — state that plainly; the barbershop iOS lane (Task 9) is the gate. Self-review both against the recorded generated API line by line.

- [ ] **Step 5: Commit**

```bash
git add mobiler/plugins/transfer/
git commit -m "feat(cli): transfer plugin — streaming upload/download on iOS + Android"
```

---

### Task 8: Template codegen registration, core pin, CLI bump

**Files:**
- Modify: `mobiler/templates/shared/src/bin/codegen.rs`
- Modify: template `Cargo.toml.tmpl` (`mobiler-core` "0.32" → "0.33", `mobiler-web` → "0.33")
- Modify: `mobiler/Cargo.toml` (0.48.1 → 0.49.0)

- [ ] **Step 1: Register the type** — apply the identical `.register_type::<mobiler_core::TransferEvent>()?` change from Task 3 to the template's `codegen.rs`.
- [ ] **Step 2: Bump pins + CLI** — template pins to "0.33"; `mobiler` to 0.49.0.
- [ ] **Step 3: Verify a fresh scaffold builds against published 0.33** — `cargo run -- new prc-b && cd prc-b && grep -A1 'name = "mobiler-core"' Cargo.lock && cargo build`; confirm it resolves 0.33.0 from crates.io.
- [ ] **Step 4: Commit**

```bash
git add mobiler/templates/ mobiler/Cargo.toml
git commit -m "feat(cli): register TransferEvent in the template; core pin 0.33; mobiler 0.49.0"
```

---

### Task 9: Wire transfer into barbershop natively, subsume files download, ship PR-C

**Files:**
- Modify: barbershop native (via `mobiler plugin add transfer` from the barbershop dir)
- Modify: `mobiler/plugins/files/android/FilesPlugin.kt`, `mobiler/plugins/files/ios/FilesPlugin.swift`

**Interfaces:**
- Consumes: the `transfer` plugin (Task 7), the barbershop demo card (Task 5).
- Produces: a demo whose native shells actually perform the transfer; a `files` plugin whose `download` delegates to the shared path.

- [ ] **Step 1: Add the plugin to barbershop** — `mobiler plugin add transfer` in the barbershop dir; commit the injected iOS/Android sources + registration. Confirm the demo's stream registration markers received the `transfer` case.
- [ ] **Step 2: Subsume the files download** — replace `FilesPlugin`'s blocking `URL(url).openStream().copyTo(...)` (`FilesPlugin.kt:61`) and the iOS equivalent so the `download` op reports progress / respects cancellation by using the same streaming implementation, OR document it explicitly as the fire-and-forget convenience wrapper over `transfer` with a comment pointing at `cx.download`. Do not silently leave two divergent download code paths — pick one and note the choice in the report.
- [ ] **Step 3: Ship PR-C** — `ship-pr`; ALL CI green incl. `iOS build (demos/barbershop)` (the gate for Task 7's Swift) and the Android matrix.
- [ ] **Step 4: Release** — `release-cli`, tag `v0.49.0`.
- [ ] **Step 5: post-release** — the fresh-scaffold + APK smoke, AND the **zero-manual-steps** `mobiler upgrade` 0.48.1 → 0.49.0 check (scaffold on 0.48.1, custom edit + a plugin, upgrade, `mobiler build android` with no hand-applied `.mobiler-new` → APK). Device-test the barbershop transfer card (real upload + download with a visible progress bar — the first time throttle/cancel run on hardware).

---

## Self-Review Notes

**Spec coverage.** Handle-is-a-string → Tasks 2/4/7 (envelope carries `source`/`dest` strings). `TransferEvent` inside `output` via `BincodeFfiFormat` → Task 1. Rides `cx.subscribe`/`unsubscribe` → Task 2 (`start` calls `subscribe`; cancel is the shipped `unsubscribe`). `transfer` streaming plugin via `register_stream` → Task 7. iOS `URLSession` tasks → Task 7. Android OkHttp + `ensureActive` cancel → Task 7. Web XHR-upload / fetch-download / `blob:` handle → Task 4. Throttle ~10/sec → Tasks 4/7. Delete partial on cancel/fail → Tasks 4/7. Overwrite default, raw body only → Tasks 2/7. Register in 6 codegen.rs → Tasks 3 (5 demos) + 8 (template). Subsume files download → Task 9. Demo consumer with progress UI → Task 5. Two-stage release + versions → Tasks 6/8/9.

**Known uncertainty, deliberately flagged not guessed.** The exact generated Swift/Kotlin spelling of `TransferEvent` (`.progress`/`.Progress`, `UInt64`/`ULong`, optional encoding of `total`) comes out of Task 3; Tasks 4 and 7 must read the recorded spelling before writing shell code. `web_sys` `ReadableStream` reader ergonomics are the riskiest web bit — Task 4 names an explicit degraded fallback (`array_buffer()` + single progress) rather than blocking on it.

**Out of scope (spec §Explicitly out of scope).** Background transfers, resumable/chunked uploads, multipart/form-data, concurrent-transfer scheduling.
