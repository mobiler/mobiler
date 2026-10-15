# Copy Report and Follow-ups Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** An `ok: true` from `clipboard_with` reports which protections the shell applied (`CopyApplied`), and the
five other copy-options follow-ups land (iOS concealed marker, Android foreground-only re-check, web warning and
no-window reason, comment-aware conformance).

**Architecture:** The report is JSON in `PluginResponse.output` (ADR-0003); mobiler-core parses it into
`CopyApplied`, with `None` for anything it can't trust (an empty output from a 0.68 shell included). Barbershop's
shells change first and are ported to the templates in the same PR: the shell changes use no new core API (ADR-0009
allows it), and ADR-0052's test already requires the templates to stay byte-identical to barbershop. The CLI PR pins
core 0.46 and adds ADR-0053 with its mutation-proven conformance test.

**Tech Stack:** Rust (mobiler-core, mobiler-web wasm/Leptos, xtask tests), Kotlin (Android shell), Swift (iOS shell).

**Spec:** `docs/superpowers/specs/2026-10-14-copy-report-design.md`

## Global Constraints

- Release: core 0.46 / web 0.46 (mobiler-ui unchanged at 0.31), then CLI 0.69.
- Report keys, all always present: `{"sensitive": bool, "expires": "system"|"best_effort"|"none", "local_only": bool}`.
- On `ok: false`, `output` stays the reason text.
- `CopyApplied::from_response` → `None` for `ok: false`, empty output, malformed JSON, a missing key, or an unknown
  `expires`.
- `cx.copy`, the `clipboard_with` request and its input are unchanged (ADR-0007, ADR-0052).
- Template classes stay byte-identical to barbershop's (the ADR-0052 test enforces it).
- Build targets go on `/media/zmilan/data2/cargo-target-<name>`; never `pgrep -f` or marker-file polling; don't wipe
  `~/.gradle`; commit messages via `git commit -F <file>`.
- Every crates.io publish and the `v0.69.0` tag are the maintainer's, after confirmation.

## Review Focus

- A shell from CLI 0.68 answers `ok: true` with an **empty** output: an app must read that as unknown, never as
  protected (Task 1 test `an_empty_report_is_unknown`).
- The report must never claim more than was applied: Android `local_only` is always `false`; the web is all
  false/`"none"` even when every option was asked (Tasks 2, 3).
- A later shell sending a new `expires` value must not be misread as one of today's (Task 1 test
  `an_unknown_expiry_is_unknown`).
- Android's foreground check must not stop a clear when the app *is* in front at the deadline (Task 5 runtime:
  expiry in front still clears).
- The iOS concealed marker must sit in the **same** pasteboard item as the text, or a paste gets two items (Task 3
  code; device check by the maintainer).

---

### Task 1: Core `CopyApplied`

**Files:**
- Modify: `mobiler-core/src/clipboard.rs` (append the types and a `#[cfg(test)] mod tests`)
- Modify: `mobiler-core/src/lib.rs:21` (`pub use clipboard::{CopyApplied, CopyExpiry, CopyOptions};`) and the
  `copy_with` doc comment (~line 233)

**Interfaces:**
- Produces: `pub struct CopyApplied { pub sensitive: bool, pub expires: CopyExpiry, pub local_only: bool }`,
  `pub enum CopyExpiry { System, BestEffort, None }`, `CopyApplied::from_response(&PluginResponse) -> Option<CopyApplied>`.

- [ ] **Step 1: Write the failing tests** (append to `mobiler-core/src/clipboard.rs`):

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::PluginResponse;

    fn ok(s: &str) -> PluginResponse {
        PluginResponse::text(true, s)
    }

    #[test]
    fn each_shells_sensitive_report_parses() {
        let android = ok(r#"{"sensitive":true,"expires":"best_effort","local_only":false}"#);
        let ios = ok(r#"{"sensitive":true,"expires":"system","local_only":true}"#);
        let web = ok(r#"{"sensitive":false,"expires":"none","local_only":false}"#);
        assert_eq!(CopyApplied::from_response(&android), Some(CopyApplied { sensitive: true, expires: CopyExpiry::BestEffort, local_only: false }));
        assert_eq!(CopyApplied::from_response(&ios), Some(CopyApplied { sensitive: true, expires: CopyExpiry::System, local_only: true }));
        assert_eq!(CopyApplied::from_response(&web), Some(CopyApplied { sensitive: false, expires: CopyExpiry::None, local_only: false }));
    }

    #[test]
    fn an_empty_report_is_unknown() {
        // A shell from CLI 0.68 answers ok:true with an empty output.
        assert_eq!(CopyApplied::from_response(&ok("")), None);
    }

    #[test]
    fn a_failed_copy_has_no_report() {
        let failed = PluginResponse::text(false, r#"{"sensitive":true,"expires":"system","local_only":true}"#);
        assert_eq!(CopyApplied::from_response(&failed), None);
    }

    #[test]
    fn an_unknown_expiry_is_unknown() {
        assert_eq!(CopyApplied::from_response(&ok(r#"{"sensitive":true,"expires":"later","local_only":true}"#)), None);
    }

    #[test]
    fn a_malformed_or_partial_report_is_unknown() {
        assert_eq!(CopyApplied::from_response(&ok("not json")), None);
        assert_eq!(CopyApplied::from_response(&ok(r#"{"sensitive":true,"expires":"system"}"#)), None);
        assert_eq!(CopyApplied::from_response(&ok(r#"{"sensitive":"yes","expires":"system","local_only":true}"#)), None);
    }
}
```

- [ ] **Step 2: Run them to see them fail.**
  Run: `CARGO_TARGET_DIR=/media/zmilan/data2/cargo-target-mobiler cargo test -p mobiler-core clipboard::tests`
  Expected: compile error, `cannot find type CopyApplied`.

- [ ] **Step 3: Implement** (in `mobiler-core/src/clipboard.rs`, above the tests):

```rust
/// What a `clipboard_with` shell applied to a copy it made (ADR-0053), read from an `ok: true` reply with
/// [`CopyApplied::from_response`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CopyApplied {
    /// The copy carries the platform's sensitive marker: Android's `IS_SENSITIVE` extra (the preview hides it on 13+),
    /// or iOS's `org.nspasteboard.ConcealedType` convention. Never on the web.
    pub sensitive: bool,
    /// Who removes the copy when its time is up.
    pub expires: CopyExpiry,
    /// The platform keeps the copy off the user's other devices (iOS `.localOnly`). Never on Android or the web.
    pub local_only: bool,
}

/// How a copy expires, as the shell reported it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CopyExpiry {
    /// The OS removes it (iOS `.expirationDate`).
    System,
    /// The shell clears it if the clipboard still holds it (Android; ADR-0052).
    BestEffort,
    /// It doesn't expire: none was asked, or the platform can't (the web).
    None,
}

impl CopyApplied {
    /// The report in an `ok: true` reply, or `None` when there is none to trust: the copy failed, the output is
    /// empty (a shell from CLI 0.68 reports nothing), it doesn't parse, or it names an expiry this core doesn't know.
    /// `None` means *unknown*: an app that needs a protected copy treats it as not protected.
    pub fn from_response(resp: &crate::PluginResponse) -> Option<Self> {
        if !resp.ok {
            return None;
        }
        let v: serde_json::Value = serde_json::from_slice(&resp.output).ok()?;
        let expires = match v.get("expires")?.as_str()? {
            "system" => CopyExpiry::System,
            "best_effort" => CopyExpiry::BestEffort,
            "none" => CopyExpiry::None,
            _ => return None,
        };
        Some(Self { sensitive: v.get("sensitive")?.as_bool()?, expires, local_only: v.get("local_only")?.as_bool()? })
    }
}
```

  In `mobiler-core/src/lib.rs` change line 21 to `pub use clipboard::{CopyApplied, CopyExpiry, CopyOptions};`, and
  append to `copy_with`'s doc comment: `/// On `ok: true`, [`CopyApplied::from_response`] says which options took
  effect on this platform (ADR-0053); `None` there means unknown, e.g. a shell from CLI 0.68.`

- [ ] **Step 4: Run the tests to see them pass**, then the crate.
  Run: `CARGO_TARGET_DIR=/media/zmilan/data2/cargo-target-mobiler cargo test -p mobiler-core` and
  `cargo clippy -p mobiler-core --all-targets -- -D warnings` (same target dir).
  Expected: all pass (116 + 5), clippy clean.

- [ ] **Step 5: Commit** (`git add mobiler-core/src/clipboard.rs mobiler-core/src/lib.rs`; message file:
  `feat(core): CopyApplied — what a clipboard_with copy got (ADR-0053)`).

### Task 2: Web — the report, the no-window reason, the warning

**Files:**
- Modify: `mobiler-web/Cargo.toml` (add `"console"` to the web-sys features list at line 31)
- Modify: `mobiler-web/src/lib.rs`: `write_clipboard` (~835), the `clipboard_with` branch (~1044), the
  `("clipboard", "copy")` and `("share", _)` notify arms (~1446, ~1457), `mod clipboard_with_tests` (~3839)

**Interfaces:**
- Produces: `const WEB_COPY_REPORT: &str`, `fn missing_clipboard_reason(has_window: bool) -> &'static str`.

- [ ] **Step 1: Write the failing tests** (add to `mod clipboard_with_tests`):

```rust
    #[test]
    fn the_web_reports_no_protection() {
        let r = PluginResponse::text(true, WEB_COPY_REPORT);
        let v: serde_json::Value = serde_json::from_slice(&r.output).unwrap();
        assert_eq!(v, serde_json::json!({"sensitive": false, "expires": "none", "local_only": false}));
    }

    #[test]
    fn a_missing_clipboard_names_its_cause() {
        assert_eq!(missing_clipboard_reason(false), "no window");
        assert_eq!(missing_clipboard_reason(true), "the clipboard needs a secure page (https or localhost)");
    }
```

- [ ] **Step 2: Run to see them fail.**
  Run: `cd mobiler-web && CARGO_TARGET_DIR=/media/zmilan/data2/cargo-target-mobiler-web cargo test clipboard_with_tests`
  Expected: compile error, `cannot find value WEB_COPY_REPORT` / `missing_clipboard_reason`.

- [ ] **Step 3: Implement.**

```rust
/// What the web applied to a `clipboard_with` copy (ADR-0053): a browser can't mark, scope or expire one.
const WEB_COPY_REPORT: &str = r#"{"sensitive":false,"expires":"none","local_only":false}"#;

/// Why there is no clipboard to write to: no `window` (a worker), or no `navigator.clipboard` (no secure context).
fn missing_clipboard_reason(has_window: bool) -> &'static str {
    if has_window { "the clipboard needs a secure page (https or localhost)" } else { "no window" }
}
```

  `write_clipboard` starts:

```rust
    let Some(win) = web_sys::window() else {
        return PluginResponse::text(false, missing_clipboard_reason(false));
    };
    let Some(clipboard) = clipboard(&win) else {
        return PluginResponse::text(false, missing_clipboard_reason(true));
    };
```

  and its success arm becomes `Ok(_) => PluginResponse::text(true, WEB_COPY_REPORT),` (its only caller is the
  `clipboard_with` branch; update its doc comment: "`ok` follows the promise; on success the output is the
  report"). In both notify arms, add an `else` to the `if let Some(c) = clipboard(&win)`:

```rust
            } else {
                web_sys::console::warn_1(&format!("mobiler: {}", missing_clipboard_reason(true)).into());
            }
```

  Add `"console",` to the web-sys feature list in `mobiler-web/Cargo.toml`.

- [ ] **Step 4: Run to see them pass**, plus the wasm build.
  Run: `cd mobiler-web && CARGO_TARGET_DIR=/media/zmilan/data2/cargo-target-mobiler-web cargo test` and
  `cargo clippy --target wasm32-unknown-unknown -- -D warnings` (same target dir).
  Expected: all pass (31 + 2), clippy clean.

- [ ] **Step 5: Commit** (`feat(web): clipboard_with reports what it applied; warn when a plain copy can't run`).

### Task 3: Barbershop's Android and iOS shells, ported to the templates

**Files:**
- Modify: `demos/barbershop/Android/app/src/main/java/dev/mobiler/barbershop/Core.kt` (`ClipboardWithPlugin`,
  `ClipboardExpiry`)
- Modify: `demos/barbershop/iOS/Sources/Core.swift` (`ClipboardWithPlugin`)
- Modify: `mobiler/templates/Android/app/src/main/java/__PACKAGE_PATH__/Core.kt`, `mobiler/templates/iOS/Sources/Core.swift`
  (the same blocks, byte-identical: ADR-0052's test checks it)

**Interfaces:**
- Consumes: the report shape from Global Constraints.
- Produces: the shell blocks Task 7's conformance test reads.

- [ ] **Step 1: Android report.** In `ClipboardWithPlugin.handle`, read `val sensitive = o.optBoolean("sensitive")`
  once (use it in the existing `if`), and replace the success `PluginResponse(true, "")` with:

```kotlin
            // What this copy got (ADR-0053): Android has no switch to keep a copy off other devices.
            val report = JSONObject()
                .put("sensitive", sensitive)
                .put("expires", if (o.isNull("expires_ms")) "none" else "best_effort")
                .put("local_only", false)
            PluginResponse(true, report.toString())
```

- [ ] **Step 2: Android foreground-only re-check.** In `ClipboardExpiry.arm`'s second loop, wrap the read:

```kotlin
            while (isActive && android.os.SystemClock.elapsedRealtime() < giveUpAt) {
                // Read only while an activity is resumed: from the background Android 10+ denies the read and logs it.
                if (MobilerActivity.current?.get() != null) {
                    // Null when the clipboard is empty (or still unreadable, e.g. before the window has focus).
                    val desc = cm.primaryClipDescription
                    if (desc != null) {
                        if (desc.label?.toString() == label) clear(cm)
                        return@launch
                    }
                }
                kotlinx.coroutines.delay(5_000)
            }
```

  and update `ClipboardExpiry`'s doc comment: "it re-checks every 5 s, reading the clipboard only while an activity
  is resumed".

- [ ] **Step 3: iOS concealed marker and report.** Replace the body after the `text` guard:

```swift
        let sensitive = (obj["sensitive"] as? Bool) == true
        let localOnly = (obj["local_only"] as? Bool) == true
        var item: [String: Any] = [UTType.utf8PlainText.identifier: text]
        // The nspasteboard.org convention (1Password, Bitwarden): a Mac's clipboard managers skip a concealed item.
        if sensitive { item["org.nspasteboard.ConcealedType"] = Data() }
        var options: [UIPasteboard.OptionsKey: Any] = [:]
        if localOnly { options[.localOnly] = true }
        var expires = "none"
        if let ms = (obj["expires_ms"] as? NSNumber)?.doubleValue {
            // A huge value (the core sends up to u64::MAX) is capped at distantFuture, i.e. never.
            options[.expirationDate] = min(Date().addingTimeInterval(ms / 1000), Date.distantFuture)
            expires = "system"
        }
        UIPasteboard.general.setItems([item], options: options)
        // What this copy got (ADR-0053).
        let report: [String: Any] = ["sensitive": sensitive, "expires": expires, "local_only": localOnly]
        let out = (try? JSONSerialization.data(withJSONObject: report)).flatMap { String(data: $0, encoding: .utf8) } ?? ""
        return PluginResponse(ok: true, output: out)
```

  Update the enum's doc comment: "iOS has no system sensitive flag; a sensitive copy carries the
  `org.nspasteboard.ConcealedType` convention, `local_only` keeps it out of Universal Clipboard and `expires_ms` lets
  the system remove it. `ok: true` carries the report (ADR-0053)."

- [ ] **Step 3b: Port to the templates.** Take the Step 1–3 diff (`git diff -- demos/barbershop/Android/…/Core.kt
  demos/barbershop/iOS/Sources/Core.swift`), rewrite the paths to the template files, and `git apply` it. Run
  `CARGO_TARGET_DIR=/media/zmilan/data2/cargo-target-mobiler cargo test -p xtask --test adr_conformance adr_0052`.
  Expected: PASS (it fails with "…handler differs from barbershop's" until both templates are ported).

- [ ] **Step 4: Compile Android.** Run barbershop's Kotlin compile for real (not a log grep):
  `cd demos/barbershop && ANDROID_HOME=~/Android/Sdk JAVA_HOME=~/jdk21 CARGO_TARGET_DIR=/media/zmilan/data2/cargo-target-barbershop mobiler build android`
  (local CLI: `/media/zmilan/data2/cargo-target-mobiler/debug/mobiler` after `cargo build -p mobiler`).
  Expected: an APK. iOS compiles in CI.

  Also scaffold with the local CLI and `mobiler build android` (the template's copy, against the published core
  0.45: the change uses no new core API). Expected: an APK.

- [ ] **Step 5: Commit** (`feat(shells): clipboard_with report, iOS concealed marker, foreground-only re-check`).

### Task 4: Docs

**Files:** `capabilities.json` (Clipboard row notes), root `README.md` (generated), `mobiler-core/README.md:71-78`,
`mobiler-web/README.md:42-45`.

- [ ] **Step 1:** Append to the Clipboard row's notes: `; ok:true carries what was applied (CopyApplied)`. Run
  `cargo run -p xtask -- gen-readme`.
- [ ] **Step 2:** `mobiler-core/README.md`, after "`cx.copy` is unchanged.": "On `ok: true`,
  `CopyApplied::from_response(&resp)` says what this platform applied (sensitive marker, who expires it, kept off
  other devices); `None` means unknown, e.g. a shell from CLI 0.68." Also replace "iOS keeps it out of Universal
  Clipboard" with "iOS marks it concealed (a convention Mac clipboard managers honour), keeps it out of Universal
  Clipboard".
- [ ] **Step 3:** `mobiler-web/README.md`: after "where browsers have no clipboard API.": "The reply's report says
  so (`sensitive: false, expires: none, local_only: false`); a plain `cx.copy` on such a page logs a console warning."
- [ ] **Step 4:** `git grep -n 'copy_with\|copy_sensitive' -- '*README.md'` and check each match still holds.
  Run the gen-readme check: `cargo test -p xtask` (it includes the README-in-sync check). Expected: pass.
- [ ] **Step 5: Commit** (`docs: the copy report in capabilities.json and the library READMEs`).

### Task 5: Runtime verification (no commits)

- [ ] **Android emulator (API 33+)**, scratch scaffold on the local core with barbershop's Kotlin (as in the
  copy-options run): a button each for `copy_sensitive`, plain `copy_with`, and `copy_with(expires_after(5 s))`, the
  reply shown on screen.
  - `copy_sensitive` → `{"sensitive":true,"expires":"best_effort","local_only":false}`; plain → all false/`"none"`.
  - Expiry with the app in front: the clipboard is empty after ~5 s (Review Focus line 4).
  - Expiry with HOME pressed for 12 s: `adb logcat -d | grep -c "Denying clipboard access"` for the app's package is
    0 while away; after returning, the clipboard is empty within ~5 s.
- [ ] **Web**, headless Chrome over CDP (the copy-options recipe): the `clipboard_with` reply text is the web report;
  on an insecure origin (`--host-resolver-rules` mapping a test host to 127.0.0.1), a plain `cx.copy` logs the
  `mobiler: the clipboard needs a secure page …` warning (`Runtime.consoleAPICalled`).
- [ ] Ledger every result; stop the emulator and uninstall the scratch app afterwards.

### Task 6: Libraries release

- [ ] **Step 1:** Final fresh code review and fresh security review (opus) of the branch; one fix pass; fresh
  re-review of the fix pass.
- [ ] **Step 2:** Bump core to 0.46.0 and web to 0.46.0 (its `mobiler-core` req `"0.46"`); `cargo update -p
  mobiler-core` in the root, each demo and each web workspace; verify no lockfile still names 0.45.0 for them.
- [ ] **Step 3:** release-libs checks (core test + clippy, web wasm clippy, `cargo publish -p mobiler-core
  --dry-run --allow-dirty`, every demo's `cargo test`).
- [ ] **Step 4:** ship-pr; every check green; the maintainer merges.
- [ ] **Step 5:** The maintainer publishes core, then web (`!` commands); confirm each on the crates.io API.

### Task 7: CLI templates, ADR-0053, CLI 0.69.0

**Files:**
- Modify: `mobiler/templates/shared/Cargo.toml.tmpl`
  (`mobiler-core = "0.46"`), `mobiler/Cargo.toml` (0.69.0), `Cargo.lock`, `mobiler/README.md` (the copy paragraph
  near line 333 and the Android-version row near line 131).
- Create: `docs/adr/ADR-0053-copy-report.md`. Modify: `docs/adr/index.md`, `xtask/tests/adr_conformance.rs`.

- [ ] **Step 1: `live_code`, test first.** Add the unit test to `xtask/tests/adr_conformance.rs`:

```rust
#[test]
fn live_code_drops_comments_blocks_and_if_false() {
    let src = "a\n// b\n/* c\n d\n*/ e\n#if false\nf\n#if X\ng\n#endif\nh\n#endif\ni\n  * j\nk // l\n\
               #if false\nm\n#else\nn\n#endif\n#if os(iOS)\no\n#else\np\n#endif\n";
    assert_eq!(live_code(src), "a\n e\ni\nk // l\nn\no\np\n");
}
```

  Run: `CARGO_TARGET_DIR=/media/zmilan/data2/cargo-target-mobiler cargo test -p xtask --test adr_conformance live_code`
  Expected: compile error, `cannot find function live_code`. Then add the helper:

```rust
/// The live code in `src`: whole-line `//` comments, `/* … */` blocks (also across lines) and the dead branch of a
/// Swift `#if false` (up to its `#else` or `#endif`, nested `#if`s included) are dropped, as are the `#if` / `#else` /
/// `#endif` lines themselves; every other line is kept whole (a `//` later in a line may sit inside a string). Any
/// other `#if` condition counts as live on both branches.
fn live_code(src: &str) -> String {
    // One frame per open `#if`: (inside a dead region already, is `#if false`, past its `#else`).
    let mut frames: Vec<(bool, bool, bool)> = Vec::new();
    let dead = |f: &[(bool, bool, bool)]| f.last().is_some_and(|&(outer, is_false, in_else)| outer || (is_false && !in_else));
    let (mut out, mut in_block) = (String::new(), false);
    for line in src.lines() {
        let t = line.trim_start();
        if t.starts_with("#if") {
            frames.push((dead(&frames), t.starts_with("#if false"), false));
            continue;
        }
        if t.starts_with("#else") {
            if let Some(top) = frames.last_mut() {
                top.2 = true;
            }
            continue;
        }
        if t.starts_with("#endif") {
            frames.pop();
            continue;
        }
        if dead(&frames) {
            continue;
        }
        if in_block {
            if let Some(end) = line.find("*/") {
                in_block = false;
                out.push_str(&line[end + 2..]);
                out.push('\n');
            }
            continue;
        }
        if t.starts_with("//") || t.starts_with('*') {
            continue;
        }
        if t.starts_with("/*") {
            match line.find("*/") {
                Some(end) => {
                    out.push_str(&line[end + 2..]);
                    out.push('\n');
                }
                None => in_block = true,
            }
            continue;
        }
        out.push_str(line);
        out.push('\n');
    }
    out
}
```

  Run it again. Expected: PASS.

- [ ] **Step 2: Switch the ADR-0052 test to `live_code`, proven RED first.** With the test still on `kotlin_code`,
  apply each mutation and see the test still PASS (the gap):
  - (d) wrap barbershop's Kotlin `"clipboard_with" to ClipboardWithPlugin(application),` line in `/*` and `*/` on
    lines of their own;
  - (e) wrap barbershop's Swift `case "clipboard_with": …` line in `#if false` / `#endif`.
  Revert, change the `answers` closure to `|path: &str, needle: &str| live_code(&read(path)).contains(needle)`, and
  re-apply each: each must now FAIL with `ADR-0052: demos/barbershop/…/Core.kt doesn't answer clipboard_with`
  (resp. `…Core.swift…`). Revert; the test passes.

- [ ] **Step 3: The ADR-0053 test.** Append:

```rust
/// ADR-0053: every shell's `clipboard_with` success path builds the report, in live code.
#[test]
fn adr_0053_every_shell_reports_what_a_copy_got() {
    for path in [
        "mobiler/templates/Android/app/src/main/java/__PACKAGE_PATH__/Core.kt",
        "demos/barbershop/Android/app/src/main/java/dev/mobiler/barbershop/Core.kt",
    ] {
        let code = live_code(&read(path));
        for needle in [".put(\"sensitive\", sensitive)", ".put(\"expires\", ", ".put(\"local_only\", false)"] {
            assert!(code.contains(needle), "ADR-0053: {path} doesn't report {needle}");
        }
    }
    for path in ["mobiler/templates/iOS/Sources/Core.swift", "demos/barbershop/iOS/Sources/Core.swift"] {
        let code = live_code(&read(path));
        assert!(
            code.contains("[\"sensitive\": sensitive, \"expires\": expires, \"local_only\": localOnly]"),
            "ADR-0053: {path} doesn't report what a copy got"
        );
    }
    let web = live_code(&read("mobiler-web/src/lib.rs"));
    assert!(web.contains("Ok(_) => PluginResponse::text(true, WEB_COPY_REPORT)"), "ADR-0053: mobiler-web doesn't report");
    let core = read("mobiler-core/src/clipboard.rs");
    assert!(core.contains("pub fn from_response(resp: &crate::PluginResponse) -> Option<Self>"), "ADR-0053: core can't read the report");
}
```

  The code it guards already exists (Tasks 1–3), so it passes at once; the proof is the mutations. Apply each,
  reverting after; each must FAIL with its message:
  - (a) delete the template Kotlin `.put("local_only", false)` line → `ADR-0053: mobiler/templates/…/Core.kt doesn't
    report .put("local_only", false)`;
  - (b) change barbershop's Swift report key `"local_only"` to `"localOnly"` → `ADR-0053:
    demos/barbershop/iOS/Sources/Core.swift doesn't report what a copy got`;
  - (c) change the web success arm to `Ok(_) => PluginResponse::text(true, ""),` → `ADR-0053: mobiler-web doesn't
    report`;
  - (f) comment out the template Kotlin report line with `//` → the same message as (a).
  Write (a)–(f) down for the ADR.

- [ ] **Step 4: Write ADR-0053** (copy `docs/adr/TEMPLATE.md`). Title claim: "An `ok: true` from `clipboard_with`
  carries the protections the shell applied — `{sensitive, expires, local_only}`, never more than it did — and an
  app reads an empty or unknown report as unknown, never as protected; retires ADR-0052's `ok: true` Negative".
  - **§1:** ADR-0052 §5's negative ("`ok: true` reports the copy, not the protection"), the security review's Low 3
    that raised it, and the maintainer's ask `[recorded: maintainer, 2026-10-14, quoted here]`: "lets do deferred
    minors".
  - **§2:** the wire (spec §1), `CopyApplied::from_response`'s `None` cases, the per-platform table, the iOS
    concealed marker; refutation conditions with their tests (the ADR-0053 conformance test, Task 1's five tests,
    Task 2's `the_web_reports_no_protection`, the Task 5 runtime result).
  - **§3:** options: a `strict` option `[recorded: docs/superpowers/specs/2026-10-14-copy-report-design.md]`
    (fails open on 0.68 shells); a typed reply replacing `PluginResponse` in `then` `[reconstructed]` (breaks
    `copy_with`'s signature, ADR-0007's spirit); report only what was asked, omitting keys `[reconstructed]` (an
    app can't tell "not asked" from "an older shell"); the chosen report `[recorded: the spec]`.
  - **§4:** the Step 2–3 mutation proofs; "Retires ADR-0052's Negative '`ok: true` reports the copy, not the
    protection'" (as ADR-0049 did for ADR-0045).
  - **§5:** negatives: a 0.68 shell still reports nothing (`None`); the iOS marker is a convention, not an OS rule;
    `sensitive: true` on Android before 13 hides no preview (it only asks keyboards); the report is the shell's own
    word, not a check of the clipboard.
  - **Header:** `Deciding PRs:` the libraries PR and this PR; `Code anchor:` `mobiler-core/src/clipboard.rs
    (CopyApplied, CopyExpiry)`, `mobiler-web/src/lib.rs (WEB_COPY_REPORT, write_clipboard)`, the template and
    barbershop `Core.kt` / `Core.swift` (`ClipboardWithPlugin`); `Conformance:`
    `xtask/tests/adr_conformance.rs::adr_0053_every_shell_reports_what_a_copy_got`, the five Task 1 tests
    (`mobiler-core/src/clipboard.rs::tests::<name>`), `mobiler-web/src/lib.rs::the_web_reports_no_protection`.
  - Index row (same claim, `Accepted`, `2026-10-14`, `—`).
- [ ] **Step 5: Pin and bump.** Once core 0.46 is live: template `mobiler-core = "0.46"`, CLI 0.69.0,
  `cargo update -p mobiler`, `mobiler/README.md` (the paragraph: "`ok: true` carries what was applied:
  `CopyApplied::from_response`"; drop "not protected: the web applies none of the options" in favour of the report).
- [ ] **Step 6: Verify.** `cargo test --workspace`; scaffold with the local CLI against the **published** core 0.46
  and `mobiler build android`; CI green.
- [ ] **Step 7: Ship.** Fresh code + security reviews, a re-review of any fix pass, ship-pr, release-cli (the
  maintainer tags `v0.69.0`), post-release (crates.io install, fresh scaffold build, a 0.68.0 app on
  `mobiler upgrade --apply` gains the report, no-op upgrade).
