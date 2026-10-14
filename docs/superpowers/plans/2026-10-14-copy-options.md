# Copy with Options Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** `cx.copy_with(text, CopyOptions, then)` and `cx.copy_sensitive(text, then)`: a copy that can be marked
sensitive, expire, and stay on the device, sent to a new built-in `clipboard_with` capability so an older shell fails
closed.

**Architecture:** the core builds a JSON request `("clipboard_with", "copy", {"text","sensitive","expires_ms","local_only"})`.
Each shell handles it:
- Android: a sensitive-flagged clip, plus `ClipboardExpiry`, which clears only a clip it can prove is its own;
- iOS: `UIPasteboard.setItems(_:options:)` with `.localOnly` / `.expirationDate`;
- web: `navigator.clipboard.writeText`, answering from the promise.

`cx.copy` is untouched. Libraries ship first (core 0.45 / web 0.45, with barbershop's shells), then the CLI templates
(0.68.0).

**Tech Stack:** Rust (mobiler-core, mobiler-web / wasm-bindgen / web-sys), Kotlin (Android `ClipboardManager`),
Swift (`UIPasteboard`), the xtask ADR tests.

**Spec:** `docs/superpowers/specs/2026-10-14-copy-options-design.md` (approved 2026-10-14). Read it before starting.

## Global Constraints

- **Wire:**
  - plugin `clipboard_with`, op `copy`;
  - input JSON with every key always present: `{"text":"…","sensitive":<bool>,"expires_ms":<u64|null>,"local_only":<bool>}`.
- **`cx.copy` stays byte-identical:** notification `("clipboard", "copy", text)` (ADR-0007). Never add an op or a
  flag to the `clipboard` capability.
- **Presets:**
  - `CopyOptions::new()` / `Default` = not sensitive, never expires, not local-only;
  - `CopyOptions::sensitive()` = sensitive, expires after **120 s**, local-only.
- **`expires_after`** is clamped to at least **1 s** (1000 ms); a `Duration` too large for `u64` milliseconds
  saturates at `u64::MAX`.
- **Every shell** answers `ok: false` with a reason for a malformed input or an unknown op, and never crashes.
- **Android:**
  - minSdk 26;
  - the sensitive key is the string `"android.content.extra.IS_SENSITIVE"` on every API level (it equals
    `ClipDescription.EXTRA_IS_SENSITIVE` on 33+);
  - clear with `clearPrimaryClip()` on API 28+, an empty plain clip below;
  - re-check every **5 s**, give up after **1 hour**.
- **Versions:** core 0.44.0 → **0.45.0**, web 0.44.0 → **0.45.0** (its `mobiler-core` req → `0.45`), mobiler-ui
  unchanged (0.31.0), CLI 0.67.0 → **0.68.0**, template `mobiler-core = "0.45"`.
- **Release:**
  - every crates.io publish and every `v*` tag is irreversible: ask the maintainer first;
  - auto mode blocks `gh pr merge` and `cargo publish`, so give the maintainer `!` commands.
- **Build dirs:** `CARGO_TARGET_DIR` under `/media/zmilan/data2/cargo-target-<name>`, one per workspace. Never
  `find /`. Wait on long builds with `run_in_background`, never `pgrep -f` or `until grep` loops.
- **Emulator:** use the `mobiler_pixel7` AVD on port 5560. `/data` is ~91% full, so uninstall an app before
  reinstalling it, or the install fails and you test the old build. Never touch `si.malimlin.appointments`, and never
  use the physical phone `RF8R70603GD`.

## Review Focus

1. **Text with quotes, backslashes, newlines and emoji** must reach the clipboard exactly. The core's JSON must escape
   it, and every shell must parse it back. Pinned by Task 1's `copy_with_input_round_trips_tricky_text` and Task 2's
   `copy_with_text_reads_escaped_text`.
2. **Empty text** (`""`) is a valid copy, not "missing text". Pinned by Task 1's assertion on `""`, and by Task 2,
   where `{"text":""}` gives `Ok("")`.
3. **A second copy before the first expires.** The newer copy's deadline replaces the old one, and a plain `cx.copy` in
   between means the old deadline must not clear the user's new clipboard. Pinned by Task 5's runtime steps 4–5.
4. **A huge `expires_after`** (`Duration::MAX`) must not overflow or panic. Pinned by Task 1's
   `expires_after_is_clamped_and_saturates`.
5. **A shell given garbage** (`"not json"`, missing `text`, an unknown op) answers `ok: false` with a reason. Pinned by
   Task 2's `copy_with_text_rejects_bad_input`, and by the Android/iOS code in Task 3, which is checked by review and the
   Task 5 runtime step 6.

---

### Task 1: Core `CopyOptions`, `copy_with`, `copy_sensitive`

**Files:**
- Create: `mobiler-core/src/clipboard.rs`
- Modify: `mobiler-core/src/lib.rs` (the `pub mod` / `pub use` block near line 10–26; the `Cx` impl next to `copy` near
  line 226; the tests module near line 1955)

**Interfaces:**
- Produces:
  - `mobiler_core::CopyOptions` with `new()`, `sensitive()`, `mark_sensitive(bool)`, `expires_after(Duration)`,
    `never_expires()`, `local_only(bool)`, `Default`, `Clone`, `Debug`, `PartialEq`;
  - `pub(crate) fn to_input(&self, text: &str) -> String`;
  - `Cx::copy_with(&mut self, text: impl Into<String>, options: CopyOptions, then: impl FnOnce(PluginResponse) -> E + Send + 'static)`;
  - `Cx::copy_sensitive(&mut self, text: impl Into<String>, then: impl FnOnce(PluginResponse) -> E + Send + 'static)`.

- [ ] **Step 1: Write the failing tests** at the end of `mod tests` in `mobiler-core/src/lib.rs`:

```rust
    /// The copy request's input as JSON (ADR-0052 wire).
    fn copy_input(cx: &Cx<Ev>, i: usize) -> serde_json::Value {
        let (call, _) = &cx.requests[i];
        assert_eq!((call.plugin.as_str(), call.op.as_str()), ("clipboard_with", "copy"));
        serde_json::from_str(&call.input).unwrap()
    }

    #[test]
    fn copy_with_and_copy_sensitive_send_the_whole_options_object() {
        let mut cx = Cx::<Ev>::default();
        cx.copy_with("plain", CopyOptions::new(), |_| Ev::Tap);
        cx.copy_sensitive("JBSWY3DPEHPK3PXP", |_| Ev::Tap);
        cx.copy_with(
            "",
            CopyOptions::sensitive().expires_after(std::time::Duration::from_secs(30)).local_only(false),
            |_| Ev::Tap,
        );
        cx.copy_with("x", CopyOptions::sensitive().never_expires().mark_sensitive(false), |_| Ev::Tap);
        assert_eq!(copy_input(&cx, 0), serde_json::json!({"text":"plain","sensitive":false,"expires_ms":null,"local_only":false}));
        assert_eq!(copy_input(&cx, 1), serde_json::json!({"text":"JBSWY3DPEHPK3PXP","sensitive":true,"expires_ms":120000,"local_only":true}));
        assert_eq!(copy_input(&cx, 2), serde_json::json!({"text":"","sensitive":true,"expires_ms":30000,"local_only":false}));
        assert_eq!(copy_input(&cx, 3), serde_json::json!({"text":"x","sensitive":false,"expires_ms":null,"local_only":true}));
        assert!(cx.notifications.is_empty(), "copy_with is a request, never a notification");
        assert_eq!(CopyOptions::default(), CopyOptions::new());
    }

    #[test]
    fn expires_after_is_clamped_and_saturates() {
        let mut cx = Cx::<Ev>::default();
        cx.copy_with("a", CopyOptions::new().expires_after(std::time::Duration::from_millis(10)), |_| Ev::Tap);
        cx.copy_with("b", CopyOptions::new().expires_after(std::time::Duration::MAX), |_| Ev::Tap);
        assert_eq!(copy_input(&cx, 0)["expires_ms"], serde_json::json!(1000));
        assert_eq!(copy_input(&cx, 1)["expires_ms"], serde_json::json!(u64::MAX));
    }

    #[test]
    fn copy_with_input_round_trips_tricky_text() {
        let tricky = "a \"quoted\" \\ back\nslash\t🔑 ćčž";
        let mut cx = Cx::<Ev>::default();
        cx.copy_with(tricky, CopyOptions::sensitive(), |_| Ev::Tap);
        assert_eq!(copy_input(&cx, 0)["text"], serde_json::json!(tricky));
    }

    /// ADR-0007 / ADR-0052: the plain copy is unchanged, and no `Cx` call sends options to `clipboard`.
    #[test]
    fn plain_copy_is_unchanged_and_options_never_go_to_clipboard() {
        let mut cx = Cx::<Ev>::default();
        cx.copy("c");
        cx.copy_with("o", CopyOptions::sensitive(), |_| Ev::Tap);
        cx.copy_sensitive("s", |_| Ev::Tap);
        let notes: Vec<(&str, &str, &str)> =
            cx.notifications.iter().map(|n| (n.plugin.as_str(), n.op.as_str(), n.input.as_str())).collect();
        assert_eq!(notes, vec![("clipboard", "copy", "c")]);
        assert!(cx.requests.iter().all(|(c, _)| c.plugin == "clipboard_with"));
    }
```

(`Ev` is the tests module's event enum, `enum Ev { Tap, Open(u32) }`, near line 1636.)

- [ ] **Step 2: Run them and see them fail**

Run: `CARGO_TARGET_DIR=/media/zmilan/data2/cargo-target-mobiler cargo test -p mobiler-core copy_ 2>&1 | tail -15`
Expected: compile errors, `cannot find type CopyOptions` / `no method named copy_with`.

- [ ] **Step 3: Write `mobiler-core/src/clipboard.rs`**

```rust
//! A copy with options (ADR-0052): `cx.copy_with` / `cx.copy_sensitive`. The request goes to the built-in
//! `clipboard_with` capability, not `clipboard`, so a shell that predates it answers `ok: false` instead of copying a
//! secret in plain text.

use std::time::Duration;

/// How [`Cx::copy_with`](crate::Cx::copy_with) copies. [`CopyOptions::new`] is a plain copy (like `cx.copy`, but with
/// a reply); [`CopyOptions::sensitive`] is the preset for a password, a recovery code or a TOTP secret.
///
/// What each platform honours:
/// - `mark_sensitive`: Android hides the text in the clipboard preview (Android 13+) and asks keyboards not to keep it.
///   iOS has no equivalent. The web can't.
/// - `expires_after`: iOS removes the item itself. Android clears the clipboard when the time is up, but only while it
///   still holds this copy; from the background it re-checks every 5 s until the app is in front, for up to an hour.
///   The web can't.
/// - `local_only`: iOS keeps the item out of Universal Clipboard. No effect on Android or the web.
#[derive(Clone, Debug, PartialEq)]
pub struct CopyOptions {
    sensitive: bool,
    expires_after: Option<Duration>,
    local_only: bool,
}

impl Default for CopyOptions {
    fn default() -> Self {
        Self::new()
    }
}

impl CopyOptions {
    /// A plain copy: not sensitive, never expires, may sync to the user's other devices.
    pub fn new() -> Self {
        Self { sensitive: false, expires_after: None, local_only: false }
    }

    /// Sensitive, expires after 2 minutes, this device only. Every field can still be changed.
    pub fn sensitive() -> Self {
        Self { sensitive: true, expires_after: Some(Duration::from_secs(120)), local_only: true }
    }

    pub fn mark_sensitive(mut self, on: bool) -> Self {
        self.sensitive = on;
        self
    }

    /// Remove the copy after `d` (at least 1 s).
    pub fn expires_after(mut self, d: Duration) -> Self {
        self.expires_after = Some(d.max(Duration::from_secs(1)));
        self
    }

    pub fn never_expires(mut self) -> Self {
        self.expires_after = None;
        self
    }

    /// Keep the copy on this device (iOS: out of Universal Clipboard).
    pub fn local_only(mut self, on: bool) -> Self {
        self.local_only = on;
        self
    }

    /// The `clipboard_with` request's input: every key always present (ADR-0052).
    pub(crate) fn to_input(&self, text: &str) -> String {
        let expires_ms = self.expires_after.map(|d| u64::try_from(d.as_millis()).unwrap_or(u64::MAX));
        serde_json::json!({
            "text": text,
            "sensitive": self.sensitive,
            "expires_ms": expires_ms,
            "local_only": self.local_only,
        })
        .to_string()
    }
}
```

- [ ] **Step 4: Wire it into `lib.rs`.** Add `pub mod clipboard;` to the `pub mod` block (alphabetical, after
`pub mod bunny;`), and `pub use clipboard::CopyOptions;` to the `pub use` block. Then add, directly after `pub fn copy`:

```rust
    /// Copy `text` with `options` (built-in `clipboard_with` capability, ADR-0052); `then` learns whether it was
    /// copied. `ok: true` means copied, with every option this platform supports applied (see [`CopyOptions`]);
    /// `ok: false` with a reason means not copied at all, e.g. `"plugin 'clipboard_with' not available"` from a shell
    /// that predates it. On the web it is a plain copy: a browser can't mark, scope or expire one.
    pub fn copy_with(&mut self, text: impl Into<String>, options: CopyOptions, then: impl FnOnce(PluginResponse) -> E + Send + 'static) {
        let input = options.to_input(&text.into());
        self.plugin("clipboard_with", "copy", input, then);
    }

    /// [`copy_with`](Self::copy_with) with [`CopyOptions::sensitive`]: for a password, a recovery code, a TOTP secret.
    /// `cx.copy_sensitive(secret, |r| Msg::Copied(r.ok))`.
    pub fn copy_sensitive(&mut self, text: impl Into<String>, then: impl FnOnce(PluginResponse) -> E + Send + 'static) {
        self.copy_with(text, CopyOptions::sensitive(), then);
    }
```

- [ ] **Step 5: Run the tests and see them pass**

Run: `CARGO_TARGET_DIR=/media/zmilan/data2/cargo-target-mobiler cargo test -p mobiler-core 2>&1 | grep -E "^test result|FAILED|panicked"`
Expected: every result `ok`, including the 4 new tests and the existing `cx_notify_capabilities_map_to_the_right_plugin_and_op`.

- [ ] **Step 6: Commit**

```bash
git add mobiler-core/src/clipboard.rs mobiler-core/src/lib.rs
git commit -F <msgfile>   # "feat(core): cx.copy_with / CopyOptions / cx.copy_sensitive (ADR-0052)" + Co-Authored-By trailer
```

(Write commit messages to a file and use `-F`: backticks in `-m "…"` run as shell commands.)

### Task 2: Web shell answers `clipboard_with`

**Files:**
- Modify: `mobiler-web/src/lib.rs`: in `perform` (near line 980), next to the `browser`/`open` branch near line 1011;
  two new free functions near `js_err` (line 820); tests in the existing `#[cfg(test)]` module (the one asserting
  `"device info is not available on the web"`, near line 3787).

**Interfaces:**
- Consumes: the Task 1 wire (input JSON).
- Produces: `fn copy_with_text(input: &str) -> Result<String, String>` (native-testable) and
  `async fn write_clipboard(text: &str) -> PluginResponse` (wasm).

- [ ] **Step 1: Write the failing tests** in the existing test module of `mobiler-web/src/lib.rs`:

```rust
    #[test]
    fn copy_with_text_reads_escaped_text() {
        let input = r#"{"text":"a \"q\" \\ b\n🔑","sensitive":true,"expires_ms":120000,"local_only":true}"#;
        assert_eq!(copy_with_text(input), Ok("a \"q\" \\ b\n🔑".to_string()));
        assert_eq!(copy_with_text(r#"{"text":"","sensitive":false,"expires_ms":null,"local_only":false}"#), Ok(String::new()));
    }

    #[test]
    fn copy_with_text_rejects_bad_input() {
        assert_eq!(copy_with_text("not json"), Err("invalid input JSON".to_string()));
        assert_eq!(copy_with_text(r#"{"sensitive":true}"#), Err("missing text".to_string()));
        assert_eq!(copy_with_text(r#"{"text":5}"#), Err("missing text".to_string()));
    }
```

- [ ] **Step 2: Run them and see them fail**

Run: `cd mobiler-web && CARGO_TARGET_DIR=/media/zmilan/data2/cargo-target-mobiler-web cargo test copy_with_text 2>&1 | tail -8`
Expected: `cannot find function copy_with_text`.

- [ ] **Step 3: Implement.** Near `js_err`:

```rust
/// `clipboard_with` input → the text to copy. A browser can't mark, scope or expire a copy, so the options are only
/// read to reject a malformed call (ADR-0052).
fn copy_with_text(input: &str) -> Result<String, String> {
    let v: serde_json::Value = serde_json::from_str(input).map_err(|_| "invalid input JSON".to_string())?;
    v.get("text").and_then(serde_json::Value::as_str).map(str::to_string).ok_or_else(|| "missing text".to_string())
}

/// Write `text` with the async Clipboard API; `ok` follows the promise (it rejects when the page has no focus).
async fn write_clipboard(text: &str) -> PluginResponse {
    let Some(win) = web_sys::window() else { return PluginResponse::text(false, "no window") };
    match wasm_bindgen_futures::JsFuture::from(win.navigator().clipboard().write_text(text)).await {
        Ok(_) => PluginResponse::text(true, ""),
        Err(e) => PluginResponse::text(false, format!("copy failed: {}", js_err(&e))),
    }
}
```

In `perform`, directly **before** the `if call.plugin == "browser" && call.op == "open"` branch (so nothing awaits
before `writeText` starts in the tap's task):

```rust
    // cx.copy_with / cx.copy_sensitive (ADR-0052): a plain copy on the web, answered from the promise.
    if call.plugin == "clipboard_with" {
        if call.op != "copy" {
            return PluginResponse::text(false, format!("unknown clipboard_with op '{}'", call.op));
        }
        return match copy_with_text(&call.input) {
            Ok(text) => write_clipboard(&text).await,
            Err(e) => PluginResponse::text(false, e),
        };
    }
```

If `js_err` is `#[cfg(target_arch = "wasm32")]`-gated or `copy_with_text` triggers a dead-code warning on native,
mirror how the neighbouring helpers are gated (`grep -n "cfg(" mobiler-web/src/lib.rs | head`). The rule is to keep
`copy_with_text` compiled for native tests.

- [ ] **Step 4: Run the tests, then the wasm build**

Run: `cd mobiler-web && CARGO_TARGET_DIR=/media/zmilan/data2/cargo-target-mobiler-web cargo test 2>&1 | grep -E "^test result|FAILED|warning: unused"`
Expected: all `ok`, no new warnings.
Run: `cd mobiler-web && CARGO_TARGET_DIR=/media/zmilan/data2/cargo-target-mobiler-web cargo build --target wasm32-unknown-unknown 2>&1 | tail -3`
Expected: `Finished`.

- [ ] **Step 5: Commit** (`feat(web): answer clipboard_with — cx.copy_with on the web (ADR-0052)`), files `mobiler-web/src/lib.rs`.

### Task 3: Barbershop's Android and iOS shells (the reference, ADR-0015)

**Files:**
- Modify: `demos/barbershop/Android/app/src/main/java/dev/mobiler/barbershop/Core.kt`: the new classes after
  `ClipboardPlugin` (near line 190), the registration after `"clipboard" to ClipboardPlugin(application),` (line 579).
- Modify: `demos/barbershop/iOS/Sources/Core.swift`: the new enum after `ClipboardPlugin` (near line 380), the case
  after `case "clipboard":` (line 290).

**Interfaces:**
- Consumes: the Task 1 wire.
- Produces: Kotlin `class ClipboardWithPlugin(context: Context) : MobilerPlugin`, `object ClipboardExpiry`; Swift
  `@MainActor enum ClipboardWithPlugin`. Task 6 ports exactly these into the templates.

- [ ] **Step 1: Android.** Add to `Core.kt`, after `ClipboardPlugin`:

```kotlin
/** Built-in `clipboard_with` capability (cx.copy_with / cx.copy_sensitive, ADR-0052): a copy with options. */
class ClipboardWithPlugin(private val context: Context) : MobilerPlugin {
    override suspend fun handle(op: String, input: String): PluginResponse {
        if (op != "copy") return PluginResponse(false, "unknown clipboard_with op '$op'")
        val o = runCatching { JSONObject(input) }.getOrNull() ?: return PluginResponse(false, "invalid input JSON")
        val text = o.opt("text") as? String ?: return PluginResponse(false, "missing text")
        val cm = context.getSystemService(Context.CLIPBOARD_SERVICE) as ClipboardManager
        // A unique label is how ClipboardExpiry later proves the clipboard still holds this copy.
        val label = "mobiler-copy-" + java.util.UUID.randomUUID()
        val clip = ClipData.newPlainText(label, text)
        if (o.optBoolean("sensitive")) {
            // ClipDescription.EXTRA_IS_SENSITIVE on API 33+ (hides the preview); keyboards read the same key before.
            clip.description.extras = android.os.PersistableBundle().apply {
                putBoolean("android.content.extra.IS_SENSITIVE", true)
            }
        }
        return try {
            withContext(Dispatchers.Main) { cm.setPrimaryClip(clip) }
            if (o.isNull("expires_ms")) ClipboardExpiry.disarm() else ClipboardExpiry.arm(cm, label, o.optLong("expires_ms"))
            PluginResponse(true, "")
        } catch (e: Exception) {
            PluginResponse(false, "copy failed: ${e.message ?: e.javaClass.simpleName}")
        }
    }
}

/** Clears a `clipboard_with` copy when its time is up, but only while the clipboard provably still holds it: since
 *  Android 10 a backgrounded app can't read the clipboard, so it re-checks every 5 s (it clears within seconds of the
 *  user returning) and gives up after an hour. A newer copy replaces the deadline; a process death drops it. */
object ClipboardExpiry {
    private val scope = kotlinx.coroutines.CoroutineScope(kotlinx.coroutines.SupervisorJob() + Dispatchers.Main)
    private var job: Job? = null

    fun disarm() {
        job?.cancel()
        job = null
    }

    fun arm(cm: ClipboardManager, label: String, ms: Long) {
        disarm()
        job = scope.launch {
            kotlinx.coroutines.delay(ms)
            val giveUpAt = System.currentTimeMillis() + 60 * 60 * 1000L
            while (isActive && System.currentTimeMillis() < giveUpAt) {
                // Null while the app can't read the clipboard (backgrounded on Android 10+), or when it's empty.
                val desc = cm.primaryClipDescription
                if (desc != null) {
                    if (desc.label?.toString() == label) clear(cm)
                    return@launch
                }
                kotlinx.coroutines.delay(5_000)
            }
        }
    }

    private fun clear(cm: ClipboardManager) {
        if (Build.VERSION.SDK_INT >= 28) cm.clearPrimaryClip() else cm.setPrimaryClip(ClipData.newPlainText("", ""))
    }
}
```

Register it on the line after `"clipboard" to ClipboardPlugin(application),`:

```kotlin
        "clipboard_with" to ClipboardWithPlugin(application),
```

All imports it needs (`ClipData`, `ClipboardManager`, `Context`, `Build`, `Dispatchers`, `Job`, `launch`, `isActive`,
`withContext`, `JSONObject`) are already at the top of `Core.kt`; check with `grep -n "^import" … | head -50`.

- [ ] **Step 2: iOS.** Add to `Core.swift`, after `ClipboardPlugin`:

```swift
/// Built-in `clipboard_with` capability (cx.copy_with / cx.copy_sensitive, ADR-0052). iOS has no "sensitive" flag;
/// `local_only` keeps the item out of Universal Clipboard and `expires_ms` lets the system remove it.
@MainActor
enum ClipboardWithPlugin {
    static func handle(op: String, input: String) -> PluginResponse {
        guard op == "copy" else { return PluginResponse(ok: false, output: "unknown clipboard_with op '\(op)'") }
        guard let data = input.data(using: .utf8),
              let obj = try? JSONSerialization.jsonObject(with: data) as? [String: Any]
        else { return PluginResponse(ok: false, output: "invalid input JSON") }
        guard let text = obj["text"] as? String else { return PluginResponse(ok: false, output: "missing text") }
        var options: [UIPasteboard.OptionsKey: Any] = [:]
        if (obj["local_only"] as? Bool) == true { options[.localOnly] = true }
        if let ms = (obj["expires_ms"] as? NSNumber)?.doubleValue {
            options[.expirationDate] = Date().addingTimeInterval(ms / 1000)
        }
        UIPasteboard.general.setItems([[UTType.utf8PlainText.identifier: text]], options: options)
        return PluginResponse(ok: true, output: "")
    }
}
```

and the dispatch case after `case "clipboard": …`:

```swift
        case "clipboard_with": return await ClipboardWithPlugin.handle(op: op, input: input)
```

(`UniformTypeIdentifiers` is already imported in `Core.swift`. `expires_ms` is JSON `null` → `NSNull`, so the
`as? NSNumber` cast is nil and nothing expires.)

- [ ] **Step 3: Compile barbershop Android**

Run in the background (`run_in_background`; you are notified when it exits), from `demos/barbershop` with
`CARGO_TARGET_DIR=/media/zmilan/data2/cargo-target-barbershop JAVA_HOME=/home/zmilan/jdk21 ANDROID_HOME=/home/zmilan/Android/Sdk`:
`cargo build -p shared --features uniffi && cargo run -p shared --bin codegen --features codegen,facet_typegen -- --language kotlin --output-dir Android/generated && Android/gradlew -p Android --no-daemon :app:compileDebugKotlin`
Expected: `BUILD SUCCESSFUL`. iOS is compiled by CI's `iOS build (demos/barbershop)` lane.

- [ ] **Step 4: Commit** (`feat(barbershop): clipboard_with on Android and iOS (ADR-0052)`), the two files.

### Task 4: Registry and library READMEs

ADR-0052 and its conformance test land in the CLI PR (Task 7), the PR that completes the decision. That way the test
covers every shell, templates included, and its mutation proofs are recorded in the ADR before it merges (merged ADRs
are immutable).

**Files:**
- Modify: `capabilities.json` (the `Clipboard` row, line 6), the generated README sections
  (`cargo run -p xtask -- gen-readme`), and the hand-written mentions in `mobiler-core/README.md` /
  `mobiler-web/README.md` if they describe copy behaviour.

- [ ] **Step 1: Registry.** In `capabilities.json` change the Clipboard row to
  - `"api": "cx.copy(text) · cx.copy_with(text, CopyOptions, then) · cx.copy_sensitive(text, then)"`;
  - `"notes": "copy text; copy_with: sensitive (Android hides the preview), expires_after, local_only (iOS); a shell without clipboard_with answers ok:false"`.

  Run `CARGO_TARGET_DIR=/media/zmilan/data2/cargo-target-mobiler cargo run -q -p xtask -- gen-readme`.
- [ ] **Step 2: READMEs.** `git grep -n -i "clipboard\|cx.copy" -- '*README.md'`. Update any hand-written sentence that
  describes copy behaviour so it names `copy_with` / `copy_sensitive` and says the web copies plainly.
- [ ] **Step 3: Run everything**

Run: `CARGO_TARGET_DIR=/media/zmilan/data2/cargo-target-mobiler cargo test --workspace 2>&1 | grep -E "^test result|FAILED|panicked" | sort | uniq -c`
Expected: all `ok`. (This includes `readme_is_current`-style generator checks, if any.)

- [ ] **Step 4: Commit** (`docs: copy_with / copy_sensitive in capabilities.json + READMEs`).

### Task 5: Runtime verification (Android emulator, web)

No repo files change. This proves the spec's §5 runtime list.

- [ ] **Step 1: Scratch app on the local core.** Build the CLI
(`CARGO_TARGET_DIR=/media/zmilan/data2/cargo-target-mobiler cargo build -q -p mobiler`). Then:
1. `mobiler new copytest --package dev.test.copytest` in the scratchpad.
2. In `copytest/shared/Cargo.toml`, point `mobiler-core` at the repo (`{ path = "/home/zmilan/working_docker/rust/mobiler/mobiler-core" }`).
3. Copy barbershop's `ClipboardWithPlugin` + `ClipboardExpiry` + registration into `copytest/Android/.../Core.kt`.
4. In `copytest/shared/src/app.rs`, add four buttons:
   - "Sensitive": `cx.copy_sensitive("SECRET-123", …)`;
   - "Plain": `cx.copy_with("PLAIN-123", CopyOptions::new(), …)`;
   - "Expire 5s": `cx.copy_with("EXP-123", CopyOptions::sensitive().expires_after(Duration::from_secs(5)), …)`;
   - "Old copy": `cx.copy("OLD-123")`.
5. Show the last reply's `ok` / output in a caption.

Build with `CARGO_TARGET_DIR=/media/zmilan/data2/cargo-target-copytest mobiler build android` (in the background).

- [ ] **Step 2: Boot the emulator** (`emulator -avd mobiler_pixel7 -port 5560 -no-window -no-audio -no-boot-anim -no-snapshot -gpu swiftshader_indirect`).
Confirm it is API 33+: `adb -s emulator-5560 shell getprop ro.build.version.sdk`. Install with `install -d` after an
`uninstall` (storage).

- [ ] **Step 3: Preview.** Tap "Sensitive" and screenshot the clipboard toast at once: it shows dots/"Content hidden",
not `SECRET-123`. Tap "Plain" and screenshot: it shows `PLAIN-123`. The caption reads `ok: true` both times.

- [ ] **Step 4: Expiry in front.** Tap "Expire 5s", wait 7 s, and check the clipboard is empty. Read it via
`adb shell cmd clipboard get-text` if the image supports it, else paste into the app's own text field (if the scaffold
has none, add one in Step 1).

- [ ] **Step 5: Someone else's copy is left alone.** Tap "Expire 5s", then within 5 s tap "Old copy" (a plain
`cx.copy`, label `text`). Wait 7 s; the clipboard still holds `OLD-123`.

- [ ] **Step 6: Fail closed and bad input.**
- Remove the `"clipboard_with"` registration line, rebuild, reinstall, and tap "Sensitive": the caption reads
  `ok: false` with "not available", and the clipboard does **not** hold `SECRET-123`.
- Restore the line. Bad input is covered by code review: the app can't send malformed JSON through `cx`.

- [ ] **Step 7: Web.** In the scratch app's `web/`, `RUSTUP_TOOLCHAIN=stable trunk build`, serve `dist/`, open it in
headless Chrome over CDP with `Browser.grantPermissions` for `clipboardReadWrite`, click "Sensitive", and check the
caption reads `ok: true` and `navigator.clipboard.readText()` gives `SECRET-123`. If CDP permissions don't work
headless, record that and rely on Task 2's tests plus the CI wasm build.

- [ ] **Step 8: Clean up.** Shut the emulator down (`adb -s emulator-5560 emu kill`), uninstall `dev.test.copytest`,
delete the scratch app and `cargo-target-copytest`. Note the results (with screenshot paths) for the PR body.

### Task 6: Ship the libraries (core 0.45, web 0.45)

- [ ] **Step 1: Fresh reviews.** Run a fresh code + security review (opus) of the branch. Fix, then re-review the fix
pass.
- [ ] **Step 2: Bump versions** with the `release-libs` skill:
  - core 0.45.0, web 0.45.0, with web's `mobiler-core` req at `0.45`;
  - refresh the root, web and demo lockfiles as that skill says.

  Note the PR number: ADR-0052 (Task 7) cites it.
- [ ] **Step 3: `ship-pr`.** Open the PR: what, constrained-by, verification (Task 5 results), the READMEs checked.
  Monitor every check, confirm it's mergeable, and hand the maintainer the merge command.
- [ ] **Step 4: Publish.** Per `release-libs`, ask the maintainer before each irreversible publish: core first, then
  web. Hand over `!` commands.

### Task 7: CLI templates, ADR-0052, CLI 0.68.0

**Files:**
- Modify: `mobiler/templates/Android/app/src/main/java/__PACKAGE_PATH__/Core.kt` (the same class, object and
  registration as Task 3, ported from barbershop's diff, ADR-0015).
- Modify: `mobiler/templates/iOS/Sources/Core.swift` (the same enum and case).
- Modify: `mobiler/templates/shared/Cargo.toml.tmpl` (`mobiler-core = "0.45"`), `mobiler/Cargo.toml` (0.68.0),
  `Cargo.lock`, `mobiler/README.md` (the built-in capabilities list near line 329, and the generated section).
- Create: `docs/adr/ADR-0052-copy-options-fail-closed.md`. Modify: `docs/adr/index.md`, `xtask/tests/adr_conformance.rs`.

- [ ] **Step 1: Write the failing conformance test** (append to `xtask/tests/adr_conformance.rs`):

```rust
/// ADR-0052: every shell answers `clipboard_with`, the capability `cx.copy_with` targets, so a copy with options never
/// falls back to the plain `clipboard` handler (which ignores its op and would copy a secret as-is).
#[test]
fn adr_0052_every_shell_answers_clipboard_with() {
    let read = |p: &str| std::fs::read_to_string(root().join(p)).expect(p);
    let kotlin = "\"clipboard_with\" to ClipboardWithPlugin(";
    let swift = "case \"clipboard_with\": return await ClipboardWithPlugin.handle(";
    for (path, needle) in [
        ("mobiler/templates/Android/app/src/main/java/__PACKAGE_PATH__/Core.kt", kotlin),
        ("mobiler/templates/iOS/Sources/Core.swift", swift),
        ("demos/barbershop/Android/app/src/main/java/dev/mobiler/barbershop/Core.kt", kotlin),
        ("demos/barbershop/iOS/Sources/Core.swift", swift),
        ("mobiler-web/src/lib.rs", "call.plugin == \"clipboard_with\""),
    ] {
        assert!(read(path).contains(needle), "ADR-0052: {path} doesn't answer clipboard_with");
    }
    let core = read("mobiler-core/src/lib.rs");
    assert!(core.contains("self.plugin(\"clipboard_with\", \"copy\""), "ADR-0052: cx.copy_with must target clipboard_with");
    assert!(core.contains("self.notify(\"clipboard\", \"copy\", text)"), "ADR-0007: cx.copy must stay a plain notification");
}
```

Run: `CARGO_TARGET_DIR=/media/zmilan/data2/cargo-target-mobiler cargo test -p xtask --test adr_conformance adr_0052`
Expected: FAIL with "ADR-0052: mobiler/templates/Android/… doesn't answer clipboard_with" (the templates aren't
ported yet).

- [ ] **Step 2: Port** barbershop's Task 3 diff into both templates. The classes must be byte-identical apart from
  package placeholders: check with `diff` after substituting `dev.mobiler.barbershop` ↔ the placeholders. Run the test
  and see it PASS.
- [ ] **Step 3: Prove the test**, once per mutation, reverting after each. Each must FAIL with its `ADR-0052:` message:
  - (a) delete the template Kotlin registration line;
  - (b) rename barbershop's Swift case to `"clipboard_w"`;
  - (c) change the web branch's string to `"clipboard_x"`;
  - (d) change `copy_with`'s plugin to `"clipboard"`, which also fails the core test
    `plain_copy_is_unchanged_and_options_never_go_to_clipboard`.

  Write the results down for the ADR.
- [ ] **Step 4: Write ADR-0052.** Copy `docs/adr/TEMPLATE.md`. The title claim is: "A copy with options is a request to
  a separate built-in `clipboard_with` capability, never an op or flag on `clipboard`, so a shell that predates it
  fails closed; each platform applies the options it can, and the record says which it can't". Fill each section from
  the spec:
  - **§1:** the MOB-18 ask, quoted from the request doc and tagged
    `[recorded: appointments request, 2026-10-13, quoted here]`. Also the maintainer's scope, quoted verbatim and
    tagged `[recorded: maintainer, 2026-10-14, quoted here]`: "keep in mind that we wish that mobiler is general
    framework, not strictly tied to appointments app... so we always wish to have it configurable instead exactly how
    appointments requests..."
  - **§2:** the rules (spec §2–§4) and the refutation conditions with their tests.
  - **§3:** the options, each with its tag:
    - a new op on `clipboard` (fails open) `[reconstructed]`;
    - a flag in the input (the raw JSON lands on the clipboard) `[reconstructed]`;
    - a fixed `copy_sensitive` with no options, appointments' exact ask, rejected by the maintainer (quoted above);
    - blind Android clearing `[reconstructed]`;
    - the chosen design `[recorded: docs/superpowers/specs/2026-10-14-copy-options-design.md]`.
  - **§4:** the Step 3 mutation proofs.
  - **§5:** the negatives:
    - Android expiry is best effort (process death; background re-check; an empty clipboard polled until the hour);
    - iOS has no sensitive flag;
    - the web honours no option;
    - `copy_with` answers `ok: false` until an app's shells are upgraded.

  Header:
  - `Deciding PRs:` the libraries PR and this PR.
  - `Code anchor:` `mobiler-core/src/clipboard.rs`, `mobiler-core/src/lib.rs (Cx::copy_with, Cx::copy_sensitive)`,
    `mobiler-web/src/lib.rs (copy_with_text, write_clipboard, perform)`, the template and barbershop `Core.kt` /
    `Core.swift` (`ClipboardWithPlugin`, `ClipboardExpiry`).
  - `Conformance:` `xtask/tests/adr_conformance.rs::adr_0052_every_shell_answers_clipboard_with`,
    `mobiler-core/src/lib.rs::plain_copy_is_unchanged_and_options_never_go_to_clipboard`,
    `mobiler-core/src/lib.rs::copy_with_and_copy_sensitive_send_the_whole_options_object`.

  Add the index row (same claim, `Accepted`, `2026-10-14`, `—`).
- [ ] **Step 5: Pin and bump.** Once core 0.45 is live on crates.io, set the template to `mobiler-core = "0.45"`, the
  CLI to 0.68.0, run `cargo update -p mobiler`, and update `mobiler/README.md`.
- [ ] **Step 6: Verify.** Run `cargo test --workspace` (with `adr_docs`). Scaffold with the local CLI against the
  **published** core 0.45 and run `mobiler build android`. CI's `scaffold + build (template, Android)` lane must pass.
- [ ] **Step 7: Ship.** Fresh reviews (and a re-review of any fix pass), then `ship-pr`, then `release-cli` (ask before
  the `v0.68.0` tag), then `post-release`. The post-release smoke test includes a 0.67.0 app on
  `mobiler upgrade --apply`, whose `Core.kt` / `Core.swift` must gain `clipboard_with`.
- [ ] **Step 8: Tell appointments.** Draft the note (`cx.copy_sensitive`, the options, what each platform honours,
  `mobiler upgrade --apply`), give it to the maintainer to send, and once it's sent delete
  `docs/deep-link-opt-out-and-sensitive-clipboard.md`.
