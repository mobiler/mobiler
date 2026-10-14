# Copy with options: sensitive, expiring, device-only

**Asked by:** the appointments team (Moj Termin admin app), 2026-10-13, on CLI 0.67.0 / core 0.44. Their audit
item MOB-18 (part 2): they copy a TOTP setup secret with `cx.copy(secret)`. Android 13+'s clipboard preview shows it,
and keyboard clipboard managers keep it. They asked for a sensitive copy: `EXTRA_IS_SENSITIVE` on Android, and
`.localOnly` plus a short `.expirationDate` on iOS.
**Scope, set by the maintainer (2026-10-14):** mobiler is a general framework, so this is a configurable copy with
options. Appointments' ask is one preset, not the whole design.
**Release:** core 0.45 / web 0.45 (mobiler-ui unchanged at 0.31), then CLI 0.68.
**Constrained by:**
- ADR-0002 / ADR-0003: the call is an ordinary `{plugin, op, input}` request answered by `PluginResponse { ok, output }`.
  No new wire types, no mobiler-ui release.
- ADR-0007: `cx.copy` keeps its signature and its byte-identical notification `("clipboard", "copy", text)`. The new
  behaviour arrives as new methods.
- ADR-0013: `ok` reports what the platform answered. A shell that can't do the copy says `ok: false` with a reason.
- ADR-0009: libraries first; the template shells land in the CLI PR once core 0.45 / web 0.45 are live.
- ADR-0015: barbershop's shells first; the templates are ported from barbershop's diff.
- ADR-0010: the framework offers the general capability; one app's needs are a preset.

**New record:** ADR-0052, a copy with options is a request to a separate built-in capability, so an older shell
fails closed.

## Decisions

### 1. Core API (`mobiler-core`)

```rust
/// How `cx.copy_with` copies. `CopyOptions::new()` is a plain copy (like `cx.copy`, but with a reply).
#[derive(Clone, Debug, PartialEq)]
pub struct CopyOptions { sensitive: bool, expires_after: Option<Duration>, local_only: bool }

impl CopyOptions {
    pub fn new() -> Self;                 // not sensitive, never expires, may sync (iOS Universal Clipboard)
    pub fn sensitive() -> Self;           // preset: sensitive + expires after 2 minutes + this device only
    pub fn mark_sensitive(self, on: bool) -> Self;
    pub fn expires_after(self, d: Duration) -> Self;   // clamped to at least 1 s
    pub fn never_expires(self) -> Self;
    pub fn local_only(self, on: bool) -> Self;
}
impl Default for CopyOptions { /* = new() */ }

impl<E> Cx<E> {
    /// Copy `text` with `options`; `then` learns whether it was copied.
    pub fn copy_with(&mut self, text: impl Into<String>, options: CopyOptions,
                     then: impl FnOnce(PluginResponse) -> E + Send + 'static);
    /// `copy_with(text, CopyOptions::sensitive(), then)`: for a password, a recovery code, a TOTP secret.
    pub fn copy_sensitive(&mut self, text: impl Into<String>, then: impl FnOnce(PluginResponse) -> E + Send + 'static);
}
```

- **Presets:** `CopyOptions::sensitive()` is appointments' case, and every field can still be changed, e.g.
  `CopyOptions::sensitive().expires_after(Duration::from_secs(30))`.
- **The reply is required:** a fire-and-forget sensitive copy would fail silently on a shell that can't do it.
  `then` follows `open_url_then`'s shape (`PluginResponse`):
  - `ok: true`: copied, with every option this platform supports applied (§3);
  - `ok: false` with a reason: not copied at all.

### 2. Wire

- **Request:** plugin **`clipboard_with`**, op **`copy`**.
- **Input:** JSON, every key always present:
  `{"text":"…","sensitive":true,"expires_ms":120000,"local_only":true}`, where `expires_ms` is `null` for "never".
- **Why a new capability name and not a new op on `clipboard`:** today's Android and iOS `clipboard` handlers ignore
  the op and copy the input as-is.
  - A new op would copy a secret in plain text on an older shell and answer `ok: true`.
  - A flag inside the input would be worse: the raw JSON, secret included, would land on the clipboard.
  - A shell without `clipboard_with` already answers `ok: false` ("plugin … not available") on all three shells.
- **The skew is real:** an app can bump `mobiler-core` while `Core.kt` / `Core.swift` stay old, e.g. when
  `mobiler upgrade` leaves them for review after a conflict (ADR-0046).
- `cx.copy` is untouched.

### 3. What each shell does

| Option | Android | iOS | Web |
|---|---|---|---|
| copy | `ClipboardManager.setPrimaryClip` | `UIPasteboard.general.setItems` | `navigator.clipboard.writeText`; `ok` follows the promise |
| `sensitive` | `ClipDescription.EXTRA_IS_SENSITIVE` in the clip's extras (API 33+); on API 26–32 the same key as the string `"android.content.extra.IS_SENSITIVE"`, which keyboards read | no iOS equivalent; `local_only` and expiry are its protection | not possible; ignored |
| `expires_after` | best effort: when the time is up, clears the clipboard **only if it still holds this copy** (§4) | `.expirationDate` (the system removes it) | not possible; ignored |
| `local_only` | no effect (Android has no system cross-device clipboard) | `.localOnly` (kept out of Universal Clipboard) | not possible; ignored |

- **Web** copies plainly and answers `ok: true` once `writeText` resolves (and `ok: false` with the reason when it
  rejects, e.g. the page has no focus). A browser has no way to mark, scope or expire a copy. This is the same risk
  as `cx.copy`, which the user asked for by tapping. The doc comment and README say so.
- **iOS** never clears the pasteboard itself; `.expirationDate` does it.

### 4. Android expiry, in detail

Android can't schedule clipboard expiry. Since Android 10, an app that doesn't have focus can't read the clipboard, so
it can't tell whether the user copied something else in the meantime. The shell therefore clears only what it can
prove is its own:

- Each `clipboard_with` copy gets a unique clip label (`mobiler-copy-<random>`), and the shell remembers it with the
  deadline. A newer copy replaces it.
- At the deadline, the shell reads the clip description:
  - **ours:** clear it (`clearPrimaryClip()` on API 28+, an empty clip below);
  - **someone else's:** forget the deadline;
  - **unreadable** (the app is in the background): check again every 5 s while the process lives, so it clears
    within seconds of the user returning to the app. Give up after an hour.
- If the process dies first, nothing clears it. Android 13+ itself clears the clipboard after about an hour.
- The ADR records this as weaker than iOS.

### 5. Testing

- **Core:**
  - `copy_with` / `copy_sensitive` issue `("clipboard_with", "copy", <json>)` with exact JSON (presets, every
    builder, the 1 s clamp, `null` for never);
  - `cx.copy` still emits exactly `("clipboard", "copy", text)` (ADR-0007).
- **Web (`mobiler-web`):** the request is routed and answered: `ok: true` on a resolved write, `ok: false` with the
  reason on a rejected one (the handler's decision logic, unit-tested like the other request handlers).
- **Conformance (ADR-0052), mutation-proven:**
  - every shell (the Android and iOS templates, barbershop's Android and iOS, mobiler-web) handles `clipboard_with`;
  - no core method sends options to the plain `clipboard` capability: every `Cx` call whose op is `copy` and whose
    input isn't the bare text targets `clipboard_with` (checked on `Cx`'s recorded requests and notifications).
- **Runtime:**
  - Android emulator (API 33+), scratch scaffold on the local core:
    - `copy_sensitive` shows dots in the clipboard preview, and a plain `copy_with` shows the text;
    - with `expires_after(5 s)` and the app in front, the clipboard is empty after 5 s;
    - a copy from another app in the meantime is left alone.
  - Web: headless Chrome, `ok: true` and the text on the clipboard.
  - iOS: CI compiles it; expiry and local-only need a device (TestFlight, the maintainer).

### 6. Release

1. **Libraries PR:**
   - core 0.45: `CopyOptions`, `copy_with`, `copy_sensitive`;
   - web 0.45: the `clipboard_with` handler;
   - barbershop's Android + iOS `clipboard_with` handlers (CI compiles them), `capabilities.json` + `gen-readme`,
     the library READMEs, ADR-0052.
2. Publish core, then web (the maintainer confirms each).
3. **CLI PR:** port barbershop's handlers to the templates, pin core 0.45, CLI 0.68.0, `mobiler/README.md`. Existing
   apps get it with `mobiler upgrade --apply`. Until then their `copy_with` answers `ok: false`, never a plain copy.

## Not doing

- **Auto-clearing a plain `cx.copy`.** It stays fire-and-forget and unchanged.
- **Reading the clipboard** (`cx.paste`). It is a separate capability with its own permission prompts.
- **Blind Android clearing** (clearing without proving the clip is ours). It would wipe what the user copied next.
