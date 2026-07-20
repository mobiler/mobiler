# Multipart/form-data Upload — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add `.multipart(field)` / `.field(k,v)` / `.filename()` / `.file_content_type()` to the streaming `cx.upload` builder, so an app can POST a file to a `multipart/form-data` endpoint with metadata fields — still streamed from disk, no bytes held in the core.

**Architecture:** A multipart config rides as an optional field on the existing transfer envelope JSON — no `PluginCall`/`PluginResponse`/`TransferEvent` struct change and no codegen change. When present, each shell builds a `multipart/form-data` body around the same streaming file source it already uses: web via `FormData`, Android via `MultipartBody` wrapping the existing streaming `RequestBody`, iOS via a temp multipart file uploaded `fromFile:`.

**Tech Stack:** Rust (mobiler-core), Leptos/WASM + `web_sys` `FormData`/XHR (web), Kotlin/OkHttp `MultipartBody` (Android), Swift/`URLSession` + `FileHandle` (iOS).

**Spec:** `docs/superpowers/specs/2026-07-20-mobiler-http-multipart-upload-design.md`

## Global Constraints

- Multipart is **upload-only** — the builder methods are no-ops when the builder is a `download`.
- `.multipart()` flips the **default method to POST**, but only if the caller has **not** called `.method(...)`. The upload constructor already sets `method: Some("PUT")`, so this needs a builder-internal "method set by caller" flag — do NOT infer it from the envelope.
- Config rides in the existing `TransferReq` JSON (`PluginCall.input` stays a `String`). **No `PluginCall`/`PluginResponse`/`TransferEvent` struct change; no `codegen.rs` / template change.**
- All three shells keep Release B semantics **unchanged**: ~10/sec progress throttle, terminal `Done` always emitted, non-2xx → `Done{Response{status}}`, only failure → `Done{TransportError}`, cancel silent + partial cleanup, the file streamed from disk (never fully in memory/core).
- Filename default = the last `/`-segment of the source handle, with `?query`/`#fragment` stripped; empty → `"file"`. Overridable via `.filename()`.
- File-part `Content-Type` default = `application/octet-stream`; overridable via `.file_content_type()` — honored on **iOS/Android only**. On **web** the browser uses the `Blob`'s own MIME type, so the override is native-only in v1 (documented, intentional — not a cross-shell bug for the review to flag).
- Text fields ordered; the **file part is emitted LAST**.
- Web: use `FormData`; do **NOT** set a `Content-Type` header in multipart mode (the browser sets `multipart/form-data; boundary=...` — a manual header would break the boundary). The existing cancelled-flag-before-send guard must still apply.
- Android: the barbershop-installed `TransferPlugin.kt` copy must stay **byte-identical** to `mobiler/plugins/transfer/android/TransferPlugin.kt` except the `package`/`import {{PACKAGE_SHARED_TYPES}}` substitution lines. Same for the iOS copy. Generated Kotlin types are `List<UByte>`, not `List<Byte>`.
- `mobiler-ui` is **NOT** touched. Two-stage release: **PR-A** core 0.33.0→**0.34.0** + web 0.33.1→**0.34.0**, then `release-libs`. **PR-C** transfer plugin + core pin + `mobiler` 0.49.0→**0.50.0**, then `release-cli`, tag **v0.50.0**.
- `main` is protected — always branch. Land each PR with `ship-pr`. This box builds/kotlinc's Android; **iOS compiles only on the macOS CI lanes**.

## File Structure

**PR-A — libs + demo**

| File | Responsibility |
|---|---|
| `mobiler-core/src/transfer.rs` | `Multipart` struct on `TransferReq`; `.multipart`/`.field`/`.filename`/`.file_content_type` + the method-set flag. |
| `mobiler-web/src/lib.rs` | `start_web_upload` multipart branch (FormData); `mobiler-web/Cargo.toml` gains the `FormData` web-sys feature. |
| `demos/barbershop/app-core/src/lib.rs` | a multipart variant of the "Send a file" flow (web-functional in PR-A). |
| `mobiler-core/Cargo.toml`, `mobiler-web/Cargo.toml`, `capabilities.json` | version bumps + capability note. |

**PR-C — the native plugin**

| File | Responsibility |
|---|---|
| `mobiler/plugins/transfer/android/TransferPlugin.kt` | wrap `streamingUploadBody` in `MultipartBody`. |
| `mobiler/plugins/transfer/ios/TransferPlugin.swift` | compose a temp multipart file, upload `fromFile:`. |
| barbershop installed copies + `mobiler/Cargo.toml` + template pin | re-sync + core pin + CLI bump. |

---

# PR-A — libs + demo

### Task 1: `Multipart` envelope + builder methods (mobiler-core)

**Files:**
- Modify: `mobiler-core/src/transfer.rs`
- Test: inline `#[cfg(test)]` in `mobiler-core/src/transfer.rs`

**Interfaces:**
- Consumes: existing `TransferReq`, `TransferBuilder`, `HttpHeader`.
- Produces: `TransferBuilder::{multipart, field, filename, file_content_type}`; a `Multipart` struct serialized under `TransferReq.multipart`.

- [ ] **Step 1: Write the failing tests**

Add to `mod tests` in `mobiler-core/src/transfer.rs`:

```rust
#[test]
fn multipart_sets_config_and_flips_method_to_post() {
    let mut cx = Cx::<Ev>::default();
    cx.upload("https://h/up", "file:///tmp/a.jpg")
        .multipart("file")
        .field("title", "My Photo")
        .field("album", "vac")
        .start("m-1", |_| Ev::Tap);

    let (call, _) = &cx.streams[0];
    let v: serde_json::Value = serde_json::from_str(&call.input).unwrap();
    assert_eq!(v["method"], "POST", "multipart flips default PUT -> POST");
    assert_eq!(v["multipart"]["field"], "file");
    assert_eq!(v["multipart"]["fields"][0]["name"], "title");
    assert_eq!(v["multipart"]["fields"][0]["value"], "My Photo");
    assert_eq!(v["multipart"]["fields"][1]["name"], "album");
    // filename / file_content_type omitted when not overridden
    assert!(v["multipart"].get("filename").is_none());
    assert!(v["multipart"].get("file_content_type").is_none());
}

#[test]
fn explicit_method_survives_multipart() {
    let mut cx = Cx::<Ev>::default();
    cx.upload("https://h/up", "file:///tmp/a.jpg")
        .method("PUT")
        .multipart("file")
        .start("m-2", |_| Ev::Tap);
    let (call, _) = &cx.streams[0];
    let v: serde_json::Value = serde_json::from_str(&call.input).unwrap();
    assert_eq!(v["method"], "PUT", "an explicit .method() is not overridden by .multipart()");
}

#[test]
fn multipart_overrides_land_in_config() {
    let mut cx = Cx::<Ev>::default();
    cx.upload("https://h/up", "file:///tmp/a.bin")
        .multipart("f")
        .filename("photo.jpg")
        .file_content_type("image/jpeg")
        .start("m-3", |_| Ev::Tap);
    let (call, _) = &cx.streams[0];
    let v: serde_json::Value = serde_json::from_str(&call.input).unwrap();
    assert_eq!(v["multipart"]["filename"], "photo.jpg");
    assert_eq!(v["multipart"]["file_content_type"], "image/jpeg");
}

#[test]
fn multipart_is_a_noop_on_download() {
    let mut cx = Cx::<Ev>::default();
    cx.download("https://h/get", "/d").multipart("f").field("k", "v").start("d-1", |_| Ev::Tap);
    let (call, _) = &cx.streams[0];
    let v: serde_json::Value = serde_json::from_str(&call.input).unwrap();
    assert!(v.get("multipart").is_none(), "download ignores multipart");
    assert!(v.get("method").is_none(), "download has no method");
}
```

- [ ] **Step 2: Run to verify failure**

Run: `cargo test -p mobiler-core --lib transfer::tests::multipart`
Expected: FAIL — `.multipart`/`.field`/`.filename`/`.file_content_type` not found.

- [ ] **Step 3: Implement**

In `mobiler-core/src/transfer.rs`, add the `Multipart` struct next to `TransferReq`:

```rust
/// Multipart/form-data config for an upload. When present, the shell builds a
/// `multipart/form-data` body: the text `fields` (in order) then the file part LAST.
#[derive(Serialize)]
struct Multipart {
    /// Form field name for the file part.
    field: String,
    /// Override the file part's `filename=`; else the shell infers it from the source.
    #[serde(skip_serializing_if = "Option::is_none")]
    filename: Option<String>,
    /// Override the file part's `Content-Type`; else `application/octet-stream`.
    #[serde(skip_serializing_if = "Option::is_none")]
    file_content_type: Option<String>,
    /// Text fields (reuse HttpHeader's name/value).
    fields: Vec<HttpHeader>,
}
```

Add the field to `TransferReq` (after `method`):

```rust
    #[serde(skip_serializing_if = "Option::is_none")]
    multipart: Option<Multipart>,
```

Update BOTH constructors' `TransferReq { ... }` literals to include `multipart: None`.

Add a method-tracking flag to `TransferBuilder`:

```rust
pub struct TransferBuilder<'a, E> {
    cx: &'a mut Cx<E>,
    op: &'static str,
    req: TransferReq,
    method_set_by_caller: bool,   // NEW
}
```

Set `method_set_by_caller: false` in both `upload(...)` and `download(...)` constructors. In `method(...)`, set the flag when it takes effect:

```rust
    #[must_use]
    pub fn method(mut self, m: impl Into<String>) -> Self {
        if self.op == "upload" {
            self.req.method = Some(m.into());
            self.method_set_by_caller = true;
        }
        self
    }
```

Add the four new methods (upload-only; no-op on download):

```rust
    /// Send as `multipart/form-data`: the file becomes a part named `field`, emitted after
    /// any text `field()`s. Flips the default method to POST (an explicit `method()` wins).
    /// Upload-only.
    #[must_use]
    pub fn multipart(mut self, field: impl Into<String>) -> Self {
        if self.op == "upload" {
            if !self.method_set_by_caller {
                self.req.method = Some("POST".into());
            }
            match &mut self.req.multipart {
                Some(m) => m.field = field.into(),
                None => {
                    self.req.multipart = Some(Multipart {
                        field: field.into(),
                        filename: None,
                        file_content_type: None,
                        fields: Vec::new(),
                    })
                }
            }
        }
        self
    }

    /// Add a text field to the multipart body (order preserved). No effect unless
    /// `multipart()` was called; no effect on download.
    #[must_use]
    pub fn field(mut self, name: impl Into<String>, value: impl Into<String>) -> Self {
        if let Some(m) = &mut self.req.multipart {
            m.fields.push(HttpHeader { name: name.into(), value: value.into() });
        }
        self
    }

    /// Override the multipart file part's `filename=` (default: inferred from the source).
    #[must_use]
    pub fn filename(mut self, name: impl Into<String>) -> Self {
        if let Some(m) = &mut self.req.multipart {
            m.filename = Some(name.into());
        }
        self
    }

    /// Override the multipart file part's `Content-Type` (default: `application/octet-stream`).
    #[must_use]
    pub fn file_content_type(mut self, ct: impl Into<String>) -> Self {
        if let Some(m) = &mut self.req.multipart {
            m.file_content_type = Some(ct.into());
        }
        self
    }
```

Note `field()`/`filename()`/`file_content_type()` require `multipart()` first (they mutate the existing config). The `multipart_is_a_noop_on_download` test relies on the download builder never having a `Some(multipart)`, so `field()` is inert there.

- [ ] **Step 4: Run to verify pass**

Run: `cargo test -p mobiler-core --lib`
Expected: PASS, whole crate.

- [ ] **Step 5: Commit**

```bash
git add mobiler-core/src/transfer.rs
git commit -m "feat(core): multipart/form-data on cx.upload — .multipart/.field/.filename/.file_content_type"
```

---

### Task 2: Web `FormData` multipart path (mobiler-web)

**Files:**
- Modify: `mobiler-web/src/lib.rs` (`start_web_upload`)
- Modify: `mobiler-web/Cargo.toml` (add `FormData` web-sys feature)
- Test: headless-Chrome integration (Step 4)

**Interfaces:**
- Consumes: the `multipart` envelope object (Task 1); the shipped `start_web_upload` (XHR, cancelled-flag guard, throttle, terminal Done).
- Produces: a web upload that, when the envelope has `multipart`, sends a `FormData` body.

- [ ] **Step 1: Add the web-sys feature**

In `mobiler-web/Cargo.toml`, add `"FormData"` to the `web-sys` `features = [...]` list (beside `XmlHttpRequest`).

- [ ] **Step 2: Branch the send on multipart**

`start_web_upload` currently fetches the `blob:` source and calls `send_with_opt_blob`. Parse the `multipart` object from the envelope (passed into `start_web_upload` — extend its signature or parse it from the same `serde_json::Value` the caller already has; follow how `headers`/`method` are threaded in). Then in the spawned send task, after `fetch_blob(&source)` resolves to a `Blob` and after the existing `if cancelled { return }` guard:

```rust
// ... inside the spawn_local, after the cancelled check, `blob` in scope:
if let Some(mp) = &multipart {
    use web_sys::FormData;
    let form = FormData::new().expect("FormData");
    // Text fields first, file part LAST (spec ordering).
    for (name, value) in &mp.fields {           // Vec<(String,String)> parsed from multipart.fields
        let _ = form.append_with_str(name, value);
    }
    let filename = mp.filename.clone().unwrap_or_else(|| infer_filename(&source));
    let _ = form.append_with_blob_and_filename(&mp.field, &blob, &filename);
    // Do NOT set Content-Type — the browser sets multipart/form-data; boundary=...
    let _ = xhr_send.send_with_opt_form_data(Some(&form));
} else {
    let _ = xhr_send.send_with_opt_blob(Some(&blob));   // existing raw path
}
```

Add the filename helper near `parse_header_block`:

```rust
/// Last `/`-segment of a handle, `?query`/`#fragment` stripped; empty -> "file".
fn infer_filename(source: &str) -> String {
    let s = source.split(['?', '#']).next().unwrap_or(source);
    let name = s.rsplit('/').next().unwrap_or("");
    if name.is_empty() { "file".to_string() } else { name.to_string() }
}
```

Thread the parsed multipart config (field, fields as `Vec<(String,String)>`, optional filename; `file_content_type` is ignored on web — the browser derives the part type from the `Blob`, and the picker/photo blob already carries a type) into `start_web_upload` the same way `headers`/`method`/`source` are. When multipart is present, the multipart-mode branch must NOT also apply a caller `Content-Type` header to the XHR (skip adding `Content-Type` from `headers` in multipart mode; other headers like `Authorization` still apply).

- [ ] **Step 3: Build + clippy**

Run: `cd mobiler-web && cargo check --target wasm32-unknown-unknown && cargo clippy --target wasm32-unknown-unknown -- -D warnings`
Expected: clean.

- [ ] **Step 4: Verify in headless Chrome**

Extend the repo's headless-Chrome/CDP transfer recipe with a local server that parses `multipart/form-data`. Drive a `cx.upload(...).multipart("file").field("title","hi")` and assert the server received: a part named `file` with the expected filename, a text field `title=hi`, progress ticks fired, and the terminal `Done` carried the server's 2xx status. If headless Chrome cannot run here, say so plainly and give the strongest alternative (clean wasm build + a careful reading that the FormData branch appends fields-then-file and never sets Content-Type) — do NOT claim a browser run you did not do.

- [ ] **Step 5: Commit**

```bash
git add mobiler-web/src/lib.rs mobiler-web/Cargo.toml
git commit -m "feat(web): multipart/form-data upload via FormData"
```

---

### Task 3: barbershop multipart demo variant

**Files:**
- Modify: `demos/barbershop/app-core/src/lib.rs`

**Interfaces:**
- Consumes: `.multipart`/`.field` (Task 1); the existing barbershop "Send a file" card + `cx.upload` wiring.
- Produces: a demo that exercises a multipart upload (web-functional now; native multipart lands when PR-C re-syncs the plugin).

- [ ] **Step 1: Add a multipart path to the card**

The existing card picks a photo then `cx.upload(UPLOAD_URL, handle).start(...)`. Add a second action (e.g. a "Send as form" button, or make the existing send use multipart) that does `cx.upload(UPLOAD_URL, handle).multipart("file").field("source", "barbershop").start("bx-up", ...)`. Keep the existing progress-bar wiring and the re-entrancy guard. Use a real multipart-accepting endpoint (e.g. `https://httpbin.org/post`) or a clearly-commented placeholder.

- [ ] **Step 2: Verify**

Run: `cd demos/barbershop && cargo build && cargo test`
Expected: clean. Note in the report: on iOS/Android the installed transfer plugin is still the Release-B (non-multipart) version until PR-C, so native ignores the `multipart` envelope field and sends raw — expected; native multipart is verified in PR-C.

- [ ] **Step 3: Commit**

```bash
git add demos/barbershop/
git commit -m "feat(barbershop): multipart upload variant on the Send-a-file card"
```

---

### Task 4: version bumps, capability note, ship PR-A

**Files:**
- Modify: `mobiler-core/Cargo.toml` (0.33.0→0.34.0), `mobiler-web/Cargo.toml` (0.33.1→0.34.0 + its `mobiler-core` dep → "0.34"), `capabilities.json`

- [ ] **Step 1: Verify every workspace** (root is `mobiler`/`mobiler-ui`/`mobiler-core`/`xtask`; web + demos are separate — build each explicitly):

```bash
cargo build --workspace && cargo test --workspace && cargo clippy -p mobiler-core -p mobiler-ui -p xtask -- -D warnings
(cd mobiler-web && cargo check --target wasm32-unknown-unknown && cargo clippy --target wasm32-unknown-unknown -- -D warnings)
for d in demos/*/ demos/*/mobile/; do [ -f "$d/Cargo.toml" ] && (cd "$d" && echo "--- $d" && cargo build && cargo test) || true; done
```

- [ ] **Step 2: Bump versions** — core 0.34.0, web 0.34.0 (+ dep pin "0.34"). Leave `mobiler-ui` 0.23.0.

- [ ] **Step 3: Capability note** — extend the `capabilities.json` HTTP row's note to mention multipart upload (`cx.upload(...).multipart(...)`). Run `cargo run -p xtask -- gen-readme` and confirm `--check` passes.

- [ ] **Step 4: Ship** — use `ship-pr`; confirm ALL CI green (iOS/Android lanes prove the barbershop demo still compiles; native multipart is a no-op until PR-C). **STOP before `release-libs` — hand back to the user for the publish go-ahead.**

---

# PR-C — the native transfer plugin

### Task 5: Android multipart (transfer plugin)

**Files:**
- Modify: `mobiler/plugins/transfer/android/TransferPlugin.kt`

**Interfaces:**
- Consumes: the `multipart` envelope object; the existing `streamingUploadBody(...)` (a streaming `RequestBody`) and the `op == "upload"` branch around line 108.

- [ ] **Step 1: Parse multipart and wrap the body**

In the `if (op == "upload")` branch (currently: reads `method`, `source`, builds `body = streamingUploadBody(...)`, then `builder.method(method, body)`), parse the optional `multipart` object. When present, override the file part's content-type with `multipart.file_content_type` (else keep the octet-stream default the streaming body already uses), then wrap the streaming body in an OkHttp `MultipartBody`:

```kotlin
import okhttp3.MultipartBody   // add if absent

// ... after `val body = streamingUploadBody(application, source, fileContentType, ::maybeProgress)`
//     where fileContentType = multipart?.optString("file_content_type")?.ifEmpty{null} ?: appContentType
val finalBody: RequestBody = if (mp != null) {   // mp = obj.optJSONObject("multipart")
    val field = mp.getString("field")
    val filename = mp.optString("filename").ifEmpty { inferFilename(source) }
    val mb = MultipartBody.Builder().setType(MultipartBody.FORM)
    val fields = mp.optJSONArray("fields")
    if (fields != null) for (i in 0 until fields.length()) {
        val f = fields.getJSONObject(i)
        mb.addFormDataPart(f.getString("name"), f.getString("value"))   // text fields first
    }
    mb.addFormDataPart(field, filename, body)                            // file part LAST
    mb.build()
} else body
val request = builder.method(method, finalBody).build()
```

Add an `inferFilename` helper mirroring the web one (last `/`-segment, strip `?`/`#`, empty → `"file"`). Progress still flows: `MultipartBody` streams the file part, so `streamingUploadBody`'s counting sink (`::maybeProgress`) still fires. Do not apply the app `Content-Type` header to the whole request in multipart mode — OkHttp sets the multipart type (the existing `appContentType` handling already filters `Content-Type` out of the header loop for uploads; confirm that still holds and the multipart type isn't double-set).

- [ ] **Step 2: Type-check the Kotlin**

Extract the plugin against the Release-B stub (or the generated types) and compile with `kotlinc` + the real OkHttp jars (the Release-B Task-7 precedent). Confirm no `List<Byte>`/`List<UByte>` regressions and that `MultipartBody`/`addFormDataPart` resolve. A full in-app gradle build happens in Task 7 (after re-sync).

- [ ] **Step 3: Commit**

```bash
git add mobiler/plugins/transfer/android/TransferPlugin.kt
git commit -m "feat(transfer/android): multipart/form-data upload via MultipartBody"
```

---

### Task 6: iOS multipart (transfer plugin)

**Files:**
- Modify: `mobiler/plugins/transfer/ios/TransferPlugin.swift`

**Interfaces:**
- Consumes: the `multipart` envelope object; the existing upload branch (`resolve(source)`, `request.httpMethod`, `uploadTask(with:fromFile:)` around line 61-83) and the temp-file/cleanup discipline of the download path.

- [ ] **Step 1: Compose a temp multipart file and upload it**

When the envelope has a `multipart` object, instead of `uploadTask(fromFile: source)`, build a temp multipart file on disk and upload THAT:

```swift
// boundary
let boundary = "Boundary-\(UUID().uuidString)"
let tmp = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
FileManager.default.createFile(atPath: tmp.path, contents: nil)
guard let h = try? FileHandle(forWritingTo: tmp) else { /* emit Done{transportError}; return */ }
func w(_ s: String) { h.write(s.data(using: .utf8)!) }
// text fields first
for f in fields {   // [(name, value)] parsed from multipart.fields
    w("--\(boundary)\r\nContent-Disposition: form-data; name=\"\(f.0)\"\r\n\r\n\(f.1)\r\n")
}
// file part LAST — stream the source bytes in, no full in-memory load
let filename = multipartFilename ?? inferFilename(source)      // multipart.filename else inferred
let ctype = multipartFileContentType ?? "application/octet-stream"
w("--\(boundary)\r\nContent-Disposition: form-data; name=\"\(field)\"; filename=\"\(filename)\"\r\nContent-Type: \(ctype)\r\n\r\n")
if let rh = try? FileHandle(forReadingFrom: resolvedSource) {
    while case let chunk = rh.readData(ofLength: 64 * 1024), !chunk.isEmpty { h.write(chunk) }
    try? rh.close()
} else { try? h.close(); try? FileManager.default.removeItem(at: tmp); /* Done{transportError}; return */ }
w("\r\n--\(boundary)--\r\n")
try? h.close()
request.setValue("multipart/form-data; boundary=\(boundary)", forHTTPHeaderField: "Content-Type")
task = session.uploadTask(with: request, fromFile: tmp)
```

Track the temp URL so it is **deleted on completion, cancel, and error** — extend the delegate/cleanup the same way the download path removes its partial file. Add an `inferFilename` helper (last `/`-segment, strip `?`/`#`, empty → `"file"`). `didSendBodyData` progress is unchanged (it now reports the whole envelope's byte count — fine). The header loop must NOT also set a `Content-Type` from the caller's headers in multipart mode (this multipart Content-Type is authoritative).

- [ ] **Step 2: Self-review (no local Swift compiler)**

Read the change line-by-line against the surrounding Swift and the verified generated API (`.progress`/`.done`, `[UInt8]`, `bincodeSerialize() throws`). Confirm: the temp file is deleted on every terminal path (success/cancel/error); the streaming read never loads the whole file; the boundary is unique per request; cancel semantics unchanged. State plainly that no Swift compile was done — the barbershop iOS CI lane (Task 7) is the gate.

- [ ] **Step 3: Commit**

```bash
git add mobiler/plugins/transfer/ios/TransferPlugin.swift
git commit -m "feat(transfer/ios): multipart/form-data upload via a temp multipart file"
```

---

### Task 7: re-sync barbershop, core pin, CLI bump, ship PR-C

**Files:**
- Modify: barbershop installed copies of `TransferPlugin.{kt,swift}`; `mobiler/templates/shared/Cargo.toml.tmpl` (core pin → 0.34); `mobiler/Cargo.toml` (0.49.0→0.50.0)

- [ ] **Step 1: Re-sync the installed plugin copies** — copy the updated `mobiler/plugins/transfer/{android,ios}/TransferPlugin.*` into `demos/barbershop/.../TransferPlugin.kt` and `demos/barbershop/iOS/Sources/TransferPlugin.swift`, preserving ONLY the `package`/`import {{PACKAGE_SHARED_TYPES}}` substitution differences. Verify with a diff that they differ only in those lines (Release B precedent).

- [ ] **Step 2: Build the barbershop Android APK** — `cd demos/barbershop/Android && JAVA_HOME=~/jdk21 ANDROID_HOME=~/Android/Sdk ./gradlew :app:assembleDebug` → APK. This is the first real in-app compile of the Android multipart Kotlin. Report the APK path/size.

- [ ] **Step 3: Bump pins + CLI** — template core pin → "0.34"; `mobiler` → 0.50.0. **No `codegen.rs`/template shell change** (multipart added no type). Confirm a fresh scaffold would resolve 0.34 (the pin) — `cargo run -- new` into a temp dir, check `Cargo.lock` if the published libs are up (they are, after PR-A publishes; if PR-A is not yet published, note the scaffold resolves the not-yet-published 0.34 and defer this check to post-release).

- [ ] **Step 4: Ship** — `ship-pr`; ALL CI green incl. `iOS build (demos/barbershop)` (the gate for Task 6's Swift) + the Android matrix. **STOP before `release-cli` — hand back to the user for the tag/publish go-ahead.**

---

## Self-Review Notes

**Spec coverage.** `.multipart`/`.field`/`.filename`/`.file_content_type` + POST-flip + upload-only + method-flag → Task 1. Envelope no-ABI-change → Task 1 (JSON field). Web FormData (fields-then-file, no Content-Type, cancel guard) → Task 2. Filename inference → Tasks 2/5/6 (`infer_filename`). barbershop demo → Task 3. Version/release → Tasks 4/7. Android MultipartBody + progress via counting sink → Task 5. iOS temp-file + cleanup → Task 6. Re-sync byte-identity + APK + CLI → Task 7. `mobiler-ui` untouched, no codegen change → Global Constraints + Task 7.

**Known uncertainty, flagged not guessed.** Threading the parsed `multipart` config into `start_web_upload` (Task 2) depends on that function's current parameter shape — the implementer reads it and extends the same way `headers`/`method` are threaded, rather than assuming a signature. iOS (Task 6) is not locally compilable; the barbershop iOS CI lane in Task 7 is the gate, and Task 6's self-review is the interim check.

**Out of scope (spec).** Multiple file parts per request; multipart on `cx.request`; extension-based content-type sniffing; background/resumable transfers.
