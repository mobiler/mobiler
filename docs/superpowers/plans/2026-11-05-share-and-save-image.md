# Share an image file, and save an image to the photo library: Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Extend the `sharefile` plugin (JSON input, https sources, text, Android FileProvider fix), add a `saveimage` plugin, and have the web shell answer both.

**Architecture:** No core/ABI change. Two PRs (ADR-0009): a **libs PR** (`mobiler-web` 0.50.0: web answers for both plugins, ADR-0067), then a **CLI PR** (CLI 0.74.0: the plugin sources, a template `file_paths.xml` line, the barbershop demo cards, ADR-0068 for the reply codes, docs).

**Tech Stack:** Rust/WASM (`mobiler-web`, web-sys/js-sys), Kotlin (OkHttp, MediaStore, FileProvider, ActivityResult), Swift (URLSession, UIActivityViewController, PhotoKit).

**Spec:** `docs/superpowers/specs/2026-11-05-share-and-save-image-design.md`

## Global Constraints

- Android minimum API 26 (ADR-0039), iOS minimum 17.0 (ADR-0021). Newer APIs are guarded by a version check.
- Input: bare handle string (sharefile only, unchanged behaviour) **or** JSON object when the trimmed input starts with `{`: `{source, mime?, text?, name?, headers?}` (sharefile) / `{source, name?, headers?}` (saveimage).
- Sources: `https://`, `content://`, `file://`, absolute path, sandbox-relative path (Android `filesDir`, iOS Documents). `http://` → `insecure_source`; other schemes → `unsupported_source`. Web also takes `blob:`.
- https fetch: native (OkHttp / URLSession), into Android `cacheDir/share/`, iOS `Caches/share/`. Cap **50 MB** → `too_large`. Non-2xx/network → `fetch_failed`. Files older than **24 hours** in the share dir are deleted at the start of each call.
- `ok: false` output is `<code>` or `<code>: <detail>`. Codes, exactly: `bad_input`, `insecure_source`, `unsupported_source`, `fetch_failed`, `too_large`, `not_found`, `not_an_image`, `denied`, `cancelled`, `no_activity`, `provider_missing`, `unavailable`, `failed`.
- `ok: true` outputs: `shared` (sharefile native/web share), `saved` (saveimage native), `downloaded` (web fallback for both).
- File name default: the source's last path segment; add an extension from `mime` when missing (`image/png`→`.png`, `image/jpeg`→`.jpg`, `image/webp`→`.webp`, `image/gif`→`.gif`, `application/pdf`→`.pdf`); empty → `shared` + extension.
- saveimage saves bytes unchanged (no re-encode). Android display path `Pictures/<app label>`. iOS plist key `NSPhotoLibraryAddUsageDescription = "Save images to your photo library."`. Android permission `android.permission.WRITE_EXTERNAL_STORAGE;maxSdkVersion=28`.
- Web: never `window.open`. Cross-origin source without CORS → `fetch_failed`.
- Merged ADRs are immutable; new ADRs are ADR-0067 (libs PR) and ADR-0068 (CLI PR).
- Every crates.io publish and `v*` tag needs the maintainer's confirmation. Auto mode blocks merge/publish: hand the user `!` commands.
- Build targets go on data2 with a per-demo `CARGO_TARGET_DIR` (memory: disk-build-targets, shared-target-dir-crate-collision). Subagents never poll with `pgrep -f` or marker files.

## Review Focus

1. **A sandbox-relative path from `files download`** (e.g. `"qr.png"`) shared on Android must open the chooser, not throw "Failed to find configured root". Pinned by Task 5 step 4 (emulator).
2. **Old app template without the `share/` provider path** must give `provider_missing`, not crash. Pinned by Task 5 step 4 (revert the xml line on the emulator build once).
3. **A non-image https source to saveimage** (an HTML error page served 200) must give `not_an_image`, and nothing lands in the gallery. Pinned by Task 7 step 5 / Task 8 step 3.
4. **A share sheet dismissed on iOS** must give `ok:false, cancelled` exactly once (a double resume crashes). Pinned by Task 6 step 3.
5. **Web share after a slow fetch** (`NotAllowedError`) must still download, not fail. Pinned by Task 1's `share_error_falls_back` test and Task 2 step 3.

---

## Part A: libs PR (branch `feat/web-share-save-image`)

### Task 1: Web input parsing and reply mapping (pure Rust)

**Files:**
- Modify: `mobiler-web/src/lib.rs` (new section near `copy_with_text`, ~line 829; tests in a new `#[cfg(test)] mod share_tests` at the end)

**Interfaces:**
- Produces:
  - `struct ShareAsk { source: String, mime: Option<String>, text: Option<String>, name: Option<String>, headers: Vec<(String, String)> }`
  - `fn parse_share_ask(input: &str) -> Result<ShareAsk, &'static str>` — bare string → `ShareAsk{source: input}`; `{…}` → JSON fields; errors are the code strings `bad_input`, `insecure_source`, `unsupported_source`. Accepted schemes on the web: `https:`, `blob:`, and a same-origin relative/absolute path starting with `/`. 
  - `fn share_file_name(source: &str, name: Option<&str>, mime: Option<&str>) -> String` (rule in Global Constraints; strips `?query`/`#frag`).
  - `fn share_error_falls_back(js_error_name: &str) -> bool` — `true` for everything except `"AbortError"`.

- [ ] **Step 1: Write the failing tests**

```rust
#[test] fn parse_bare_handle() { assert_eq!(parse_share_ask("blob:https://a/1").unwrap().source, "blob:https://a/1"); }
#[test] fn parse_json_fields() {
    let a = parse_share_ask(r#"{"source":"https://x.rs/q?s=1","mime":"image/png","text":"Hi","name":"qr","headers":{"Authorization":"B t"}}"#).unwrap();
    assert_eq!((a.mime.as_deref(), a.text.as_deref(), a.name.as_deref()), (Some("image/png"), Some("Hi"), Some("qr")));
    assert_eq!(a.headers, vec![("Authorization".into(), "B t".into())]);
}
#[test] fn parse_rejects() {
    assert_eq!(parse_share_ask("").err(), Some("bad_input"));
    assert_eq!(parse_share_ask("{\"mime\":\"x\"}").err(), Some("bad_input"));
    assert_eq!(parse_share_ask("{not json").err(), Some("bad_input"));
    assert_eq!(parse_share_ask("http://x.rs/a.png").err(), Some("insecure_source"));
    assert_eq!(parse_share_ask("ftp://x/a.png").err(), Some("unsupported_source"));
}
#[test] fn file_name_rules() {
    assert_eq!(share_file_name("https://x.rs/page/qr?v=2", None, Some("image/png")), "qr.png");
    assert_eq!(share_file_name("https://x.rs/a.jpg", None, Some("image/png")), "a.jpg");
    assert_eq!(share_file_name("https://x.rs/", None, Some("image/png")), "shared.png");
    assert_eq!(share_file_name("https://x.rs/a", Some("moj-termin-qr"), Some("image/jpeg")), "moj-termin-qr.jpg");
    assert_eq!(share_file_name("https://x.rs/a", None, None), "a");
}
#[test] fn abort_does_not_fall_back() { assert!(!share_error_falls_back("AbortError")); assert!(share_error_falls_back("NotAllowedError")); assert!(share_error_falls_back("TypeError")); }
```

- [ ] **Step 2: Run, expect FAIL (functions not defined)**

Run: `cd mobiler-web && CARGO_TARGET_DIR=/media/zmilan/data2/targets/mobiler-web cargo test share_tests`

- [ ] **Step 3: Implement the three functions and `ShareAsk`** (serde_json is already a dependency).

- [ ] **Step 4: Run, expect PASS** (same command).

- [ ] **Step 5: Mutation proof.** Change `share_error_falls_back` to return `true` for `"AbortError"`; rerun; `abort_does_not_fall_back` must FAIL. Change `"http://"` handling to accept it; `parse_rejects` must FAIL. Revert both. Note both mutations for ADR-0067 §4 (Task 3).

- [ ] **Step 6: Commit** `feat(web): parse sharefile/saveimage input and name rules`

### Task 2: Web shell answers `sharefile/file` and `saveimage/save`

**Files:**
- Modify: `mobiler-web/src/lib.rs` `perform()` (insert before the `if call.plugin != "http"` line, ~1085); `mobiler-web/Cargo.toml` web-sys features if needed (`HtmlAnchorElement`, `FilePropertyBag`)

**Interfaces:**
- Consumes: Task 1's `parse_share_ask`, `share_file_name`, `share_error_falls_back`.
- Produces: `async fn web_share_file(ask: ShareAsk) -> PluginResponse`, `async fn web_save_image(ask: ShareAsk) -> PluginResponse`, `async fn fetch_share_blob(ask: &ShareAsk) -> Result<web_sys::Blob, String>` (sends `headers`; non-2xx → `fetch_failed: <status>`; size > 50 MB → `too_large`), `fn download_blob(blob: &web_sys::Blob, name: &str) -> Result<(), String>` (object URL on a clicked `<a download>`, URL revoked on a 60 s timer).

- [ ] **Step 1:** Implement the four functions. Call `navigator.canShare` / `navigator.share` through `js_sys::Reflect` (no web-sys `ShareData` features, which are unstable). `File` built with `File::new_with_blob_sequence_and_options` and the type from `mime` or the blob. Outcomes: shared → `ok:true "shared"`; `AbortError` → `ok:false "cancelled"`; any other error or no `canShare` → `download_blob` → `ok:true "downloaded"`. saveimage: fetch, require `blob.type_()` to start with `image/` else `not_an_image`, then `download_blob` → `"downloaded"`. Unknown op on either plugin → `ok:false "unavailable: unknown op '<op>'"`.

- [ ] **Step 2: Build** `cd mobiler-web && cargo build --target wasm32-unknown-unknown` → no errors, no new warnings. `cargo test` → all pass.

- [ ] **Step 3: Browser check (headless Chrome + CDP, recipe in memory `mobiler-web-shell`).** In the barbershop web client (path dep on `mobiler-web`), temporarily wire a button to `cx.plugin("sharefile","file", r#"{"source":"/assets/…png"}"#)` *or* drive `perform` from a tiny scratch app in the scratchpad. Expect: headless has no `navigator.share` → `ok:true "downloaded"` and Chrome's download event (`Browser.setDownloadBehavior` + `Browser.downloadWillBegin`) names the file. A cross-origin URL without CORS → `ok:false` starting `fetch_failed`. Revert temporary wiring.

- [ ] **Step 4: Commit** `feat(web): answer sharefile and saveimage in the browser`

### Task 3: ADR-0067, docs, version, ship libs

**Files:**
- Create: `docs/adr/ADR-0067-web-answers-installable-plugins-it-can.md` (from `docs/adr/TEMPLATE.md`)
- Modify: `docs/adr/index.md`, `mobiler-web/Cargo.toml` (0.49.1 → 0.50.0), `mobiler-web/README.md`, `docs/superpowers/specs/2026-11-05-share-and-save-image-design.md` (commit it here)

- [ ] **Step 1:** Write ADR-0067: *The web shell answers an installable plugin's op built-in when a browser can do the job (`transfer`, `sharefile`, `saveimage`), and answers `ok:false "plugin '…' not available"` otherwise; nothing is installed on the web.* Conformance: the Task 1 tests; §4 Mutation proof notes from Task 1 step 5. Context cites ADR-0027 (`transfer` precedent) and ADR-0023.
- [ ] **Step 2:** README: grep `git grep -n -i 'sharefile\|not available' -- '*.md' site/src/content/docs` and update each web-behaviour mention (web answers both; cross-origin needs CORS).
- [ ] **Step 3:** `cargo test -p xtask` (adr_docs + adr_conformance) → PASS.
- [ ] **Step 4:** Fresh opus code + security review of the branch (memory: fresh-reviews-before-ship); fix and re-review.
- [ ] **Step 5:** Ship with the `ship-pr` skill, then `release-libs` for `mobiler-web` 0.50.0 only (maintainer confirms the publish).

## Part B: CLI PR (branch `feat/share-and-save-image`, after mobiler-web 0.50.0 is live)

**Order:** Task 4, then Task 9 steps 1–2 (the demo's Rust side; plugin installs after Tasks 5–8 exist), then Tasks 5–8, then Task 9 step 3, then Task 10. The device checks in Tasks 5–8 drive the Task 9 buttons.

### Task 4: FileProvider `share/` path — barbershop first, then the template and demos

**Files:**
- Modify: `demos/barbershop/Android/app/src/main/res/xml/file_paths.xml`, then `mobiler/templates/Android/app/src/main/res/xml/file_paths.xml`; demos via upgrade: `demos/saldo`, `demos/coffee`, `demos/todo`, `demos/fullstack-todo/mobile`

- [ ] **Step 1:** Add `<cache-path name="share" path="share/" />` after the `captures` line in barbershop; update the comment ("…and sharefile/saveimage stage files in share/").
- [ ] **Step 2:** Build barbershop Android (`assembleDebug`) → succeeds.
- [ ] **Step 3:** Port the identical file to the template (ADR-0015).
- [ ] **Step 4:** Run `cargo run -p mobiler -- upgrade --apply` in each of the four demo apps (ADR-0063); `cargo test --workspace` → the ADR-0063 conformance passes.
- [ ] **Step 5: Commit** `feat(template): FileProvider shares cache/share/`

### Task 5: `sharefile` Android

**Files:**
- Modify: `mobiler/plugins/sharefile/android/SharefilePlugin.kt`, `mobiler/plugins/sharefile/mobiler-plugin.toml` (usage comment block like `files`')
- Test: `mobiler/src/plugin.rs` (existing add test pattern)

**Interfaces:**
- Produces (copied verbatim into Task 7's plugin, per spec §9 duplication is fine): `ShareSource.resolve(app: Application, input: JSONObject): File | Uri` that applies the source rules, the 24 h prune of `cacheDir/share/`, the OkHttp fetch with headers and 50 MB cap, and returns codes as `ShareError(code: String, detail: String?)`.

- [ ] **Step 1: Write the failing CLI test** `add_bundled_sharefile_copies_and_registers` in `mobiler/src/plugin.rs`: after `add_at(&root,"sharefile")`, `SharefilePlugin.kt` contains `package dev.mobiler.demo` and `"share/"`; Core.kt contains `"sharefile" to SharefilePlugin(application),`. Run `cargo test -p mobiler add_bundled_sharefile` → FAIL on `"share/"`.
- [ ] **Step 2: Implement.** Bare input → today's path but every file is copied to `cacheDir/share/` before `FileProvider.getUriForFile`. JSON input → resolve; build `ACTION_SEND` with `type` = mime (or `contentResolver.getType` / extension / `*/*`), `EXTRA_STREAM`, `clipData = ClipData.newRawUri(name, uri)`, `EXTRA_TEXT` if text, `FLAG_GRANT_READ_URI_PERMISSION`. `IllegalArgumentException` from `getUriForFile` → `provider_missing: run mobiler upgrade`. No foreground activity → `no_activity`. Fetch/IO on `Dispatchers.IO`, the chooser on `Main`.
- [ ] **Step 3:** `cargo test -p mobiler add_bundled_sharefile` → PASS.
- [ ] **Step 4: Emulator check (API 33, then API 26 AVD; memory `mobiler-android-verify`)** using the barbershop cards from Task 9 (build Task 9's Rust first, or temporarily): (a) https PNG share → chooser shows the image preview; (b) a `files download` result `"qr.png"` → chooser opens; (c) revert the xml line once → `provider_missing`, no crash; restore.
- [ ] **Step 5: Commit** `feat(sharefile): JSON input, https sources, text; fix sharing app files on Android`

### Task 6: `sharefile` iOS

**Files:**
- Modify: `mobiler/plugins/sharefile/ios/SharefilePlugin.swift`

- [ ] **Step 1: Implement.** JSON or bare input; resolve the source (Documents-relative paths, `file://`, absolute; https via `URLSession.shared.download(for:)` with headers, moved to `Caches/share/<name>`; size from response length / file attributes, > 50 MB → `too_large`); prune entries older than 24 h. Activity items: for `image/*` that decodes as `UIImage`, an `NSItemProvider` registering the file URL **and** the image (`registerObject(image)` + `registerFileRepresentation`), else the file URL; plus `text` if given. Keep the existing single-resume guard (`resumed` flag) for the completion handler.
- [ ] **Step 2: Build** the barbershop iOS app for the simulator (memory `mobiler-ios-build`) → succeeds.
- [ ] **Step 3: Simulator check (iOS 17):** share sheet shows the image; dismiss → `ok:false cancelled` reaches the app once (toast shown once); "Save Image" from the sheet works.
- [ ] **Step 4: Commit** `feat(sharefile): JSON input, https sources and text on iOS`

### Task 7: `saveimage` plugin — manifest, CLI test, Android

**Files:**
- Create: `mobiler/plugins/saveimage/mobiler-plugin.toml`, `mobiler/plugins/saveimage/android/SaveImagePlugin.kt`, `mobiler/plugins/saveimage/android/SaveImagePermissionActivity.kt`
- Test: `mobiler/src/plugin.rs`

**Interfaces:**
- Consumes: Task 5's `ShareSource` code (copied into `SaveImagePlugin.kt` as a private helper).
- Produces: manifest `name = "saveimage"`, android `register = '"saveimage" to SaveImagePlugin(application)'`, `manifest_application = ['<activity android:name=".SaveImagePermissionActivity" android:exported="false" android:theme="@android:style/Theme.Translucent.NoTitleBar" />']`, `permissions = ["android.permission.WRITE_EXTERNAL_STORAGE;maxSdkVersion=28"]`; ios `register = 'case "saveimage": return await SaveImagePlugin.handle(op: op, input: input)'`, `[ios.info_plist] NSPhotoLibraryAddUsageDescription = "Save images to your photo library."`.

- [ ] **Step 1: Write the failing test** `add_bundled_saveimage_registers_activity_permission_and_plist_key`: after `add_at(&root,"saveimage")`: Core.kt has `"saveimage" to SaveImagePlugin(application),`; Core.swift has `case "saveimage": return await SaveImagePlugin.handle`; manifest has `.SaveImagePermissionActivity` and `WRITE_EXTERNAL_STORAGE` with `maxSdkVersion="28"`; project.yml has `NSPhotoLibraryAddUsageDescription`. Also assert a **fresh skeleton without the plugin** contains neither `NSPhotoLibraryAddUsageDescription` nor `WRITE_EXTERNAL_STORAGE` (ADR-0035). Run → FAIL (unknown plugin).
- [ ] **Step 2:** Write the manifest (with a usage comment block like `files`'), then `SaveImagePlugin.kt`: op `save` else `unavailable: unknown op`; resolve source; bounds-decode (`BitmapFactory.Options.inJustDecodeBounds`) → `outWidth <= 0` → `not_an_image`; mime from `outMimeType`. API ≥ 29: MediaStore insert (`DISPLAY_NAME`, `MIME_TYPE`, `RELATIVE_PATH = "Pictures/<label>"`, `IS_PENDING=1`), copy bytes, `IS_PENDING=0`. API 26–28: if not granted, `SaveImagePermissionActivity.request(application)` suspends until the answer (a `CompletableDeferred<Boolean>` relay like `FileExportRelay`) → false → `denied`; write to `Environment.getExternalStoragePublicDirectory(DIRECTORY_PICTURES)/<label>/<name>` (unique name on clash) and `MediaScannerConnection.scanFile`. Success → `ok:true "saved"`.
- [ ] **Step 3:** `SaveImagePermissionActivity.kt`: `ComponentActivity` that registers `RequestPermission`, launches it in `onCreate`, completes the relay, `finish()`es; completes `false` if destroyed without an answer.
- [ ] **Step 4:** `cargo test -p mobiler add_bundled_saveimage` → PASS; `cargo test -p mobiler` → all pass.
- [ ] **Step 5: Emulator check, API 33 and API 26:** save the https PNG → `ok:true saved`; `adb shell content query --uri content://media/external/images/media --projection _display_name:relative_path` lists it. API 26: first tap shows the permission dialog; Allow → saved in the same call; reinstall, Deny → `denied`. An https URL serving HTML → `not_an_image`, nothing new in MediaStore.
- [ ] **Step 6: Commit** `feat(saveimage): new plugin — save an image to the gallery (Android)`

### Task 8: `saveimage` iOS

**Files:**
- Create: `mobiler/plugins/saveimage/ios/SaveImagePlugin.swift`

- [ ] **Step 1: Implement** `@MainActor enum SaveImagePlugin { static func handle(op:input:) async -> PluginResponse }`: resolve source (same rules as Task 6, private copy); `CGImageSourceCreateWithURL` + `CGImageSourceGetCount > 0` else `not_an_image`; `PHPhotoLibrary.requestAuthorization(for: .addOnly)` → `.authorized`/`.limited` continue, else `denied`; `performChanges { PHAssetCreationRequest.forAsset().addResource(with: .photo, fileURL:, options: originalFilename = name) }` → `ok:true "saved"` or `failed: <error>`.
- [ ] **Step 2: Build** barbershop iOS for the simulator → succeeds.
- [ ] **Step 3: Simulator check:** grant → saved, visible in Photos; `xcrun simctl privacy booted revoke photos-add <bundle id>` → `ok:false denied`, no crash; HTML URL → `not_an_image`.
- [ ] **Step 4: Commit** `feat(saveimage): save an image to Photos (iOS)`

### Task 9: Barbershop demo cards

**Files:**
- Modify: `demos/barbershop/app-core/src/lib.rs` (Msg + update + view next to `ShareClip`, ~line 130/797), `demos/barbershop/mobiler.toml` / shells via `mobiler plugin add saveimage` and re-add `sharefile` (refreshes the body, ADR-0011)

**Interfaces:**
- Produces: `Msg::ShareQr`, `Msg::SaveQr`, `Msg::Saved(String)` (reuse `Msg::Shared(String)`). Sample source: `https://raw.githubusercontent.com/mobiler/mobiler/main/demos/barbershop/screenshots/bookings.png`, name `fadehouse.png`, mime `image/png`, text `"Book at Fade House"`.

- [ ] **Step 1: Write the failing test** in the app-core tests: `share_qr_sends_sharefile_json` — `Msg::ShareQr` emits one plugin request `sharefile`/`file` whose input JSON has `source` = the sample URL and `text` = `"Book at Fade House"`; `save_qr_sends_saveimage` — `saveimage`/`save` with `name` = `"fadehouse.png"`. Run `cargo test` in `demos/barbershop` → FAIL.
- [ ] **Step 2:** Implement the two messages, the toast on the reply (`ok` → "Shared"/"Saved to Photos"/"Downloaded"; else the code), and two buttons in the existing share card. Install the plugins with the CLI built from this branch.
- [ ] **Step 3:** `cargo test` → PASS; Android, iOS and web builds succeed. These buttons are what Tasks 5–8 verify with.
- [ ] **Step 4: Commit** `feat(barbershop): share and save an image`

### Task 10: ADR-0068, docs, versions, ship CLI

**Files:**
- Create: `docs/adr/ADR-0068-share-and-save-reply-codes.md`
- Modify: `docs/adr/index.md`, `mobiler/src/upgrade.rs:210` (`MOBILER_WEB_PIN` → `"0.50"`, plus any test fixture the bump breaks), `mobiler/Cargo.toml` (0.73.3 → 0.74.0), `README.md`, `mobiler/README.md`, `site/src/content/docs/reference/plugins.md`, `capabilities.json`, plugin counts (`git grep -n -E '3[0-9] (bundled )?plugins'`)

- [ ] **Step 1:** ADR-0068: the §5 codes are stable for `sharefile` and `saveimage`; new codes may be added; an app treats an unknown code as `failed`; `ok:true` words `shared`/`saved`/`downloaded`. Conformance: native code → review + device checks (as ADR-0013); cite the web mapping test from Task 1.
- [ ] **Step 2:** Docs: add `saveimage` to every plugin list (`git grep -n -i 'sharefile' -- '*.md' site/src/content/docs`), document the JSON input, https sources, "on the web a cross-origin source needs CORS", and the upgrade step ("run `mobiler upgrade`, then `mobiler plugin add sharefile` again"). `cargo run -p xtask -- gen-readme`. `cargo test --workspace` → PASS.
- [ ] **Step 3:** Fresh opus code + security review of the whole branch, all three shells side by side; fix and re-review.
- [ ] **Step 4:** Ship with `ship-pr`, then `release-cli` for 0.74.0 (maintainer confirms the tag), then `post-release`. Delete `docs/share-and-save-image.md` (request doc, never committed) once shipped, and write the appointments note with the call examples.
