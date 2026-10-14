# Copy report and the copy-options follow-ups

**Asked by:** the maintainer, 2026-10-14: "lets do deferred minors", for the six minors deferred from the copy-options
release (core 0.45 / web 0.45 / CLI 0.68, #293 / #294, ADR-0052). The design was presented in chat and approved
("yes, looks right").
**Release:** core 0.46 / web 0.46 (mobiler-ui unchanged at 0.31), then CLI 0.69.
**Constrained by:**
- ADR-0003: `PluginResponse` stays `{ ok, output }`; the report rides inside `output`.
- ADR-0013: `ok` reports what the platform answered; the report says what it applied, never more.
- ADR-0007: `cx.copy` is untouched (the web warning changes no wire).
- ADR-0052: the `clipboard_with` request, its input and its fail-closed rule are unchanged. This adds to its reply.
- ADR-0009 / ADR-0015: libraries first, barbershop's shells first, templates ported in the CLI PR.
- ADR-0010: the framework reports facts; whether an unprotected copy is acceptable is the app's call.

**New record:** ADR-0053, an `ok: true` from `clipboard_with` carries the protections the shell applied; an empty
output means unknown (a shell from CLI 0.68). It retires ADR-0052's negative "`ok: true` reports the copy, not the
protection" (as ADR-0049 retired two of ADR-0045's).

## Decisions

### 1. The copy report (core + every shell)

**Wire.** On `ok: true`, `output` is JSON, every key always present:

```json
{"sensitive": true, "expires": "system", "local_only": true}
```

- `sensitive`: the copy carries the platform's sensitive marker (Android's `IS_SENSITIVE` extra, iOS's
  `org.nspasteboard.ConcealedType`, §2). `false` when not asked, or not possible.
- `expires`: `"system"` (the OS removes it: iOS `.expirationDate`), `"best_effort"` (the shell clears it if it can:
  Android, ADR-0052 §2), or `"none"` (no expiry asked, or not possible).
- `local_only`: the copy is kept off other devices by the platform (iOS `.localOnly`). `false` when not asked, and
  always `false` on Android and the web, which have no such switch.
- On `ok: false`, `output` stays the reason text (unchanged).

What each shell answers for `CopyOptions::sensitive()`:

| | sensitive | expires | local_only |
|---|---|---|---|
| Android | `true` | `"best_effort"` | `false` |
| iOS | `true` | `"system"` | `true` |
| Web | `false` | `"none"` | `false` |

For `CopyOptions::new()` every shell answers `{"sensitive":false,"expires":"none","local_only":false}`.

**Core API** (`mobiler_core::clipboard`, re-exported as `mobiler_core::CopyApplied`):

```rust
/// What a `clipboard_with` shell applied to a copy it made (ADR-0053).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CopyApplied { pub sensitive: bool, pub expires: CopyExpiry, pub local_only: bool }

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CopyExpiry { System, BestEffort, None }

impl CopyApplied {
    /// The report in an `ok: true` reply, or `None`: the reply is `ok: false`, its output is empty (a shell from
    /// CLI 0.68, which reports nothing), or it doesn't parse.
    pub fn from_response(resp: &PluginResponse) -> Option<Self>;
}
```

- `None` means *unknown*, never *protected*. The doc comment says an app that needs protection treats `None` as not
  protected.
- An unknown `expires` string (a later shell) parses as `None` for the whole report rather than guessing.
- `copy_with` / `copy_sensitive` keep their signatures; `then` still gets the `PluginResponse`.

**Rejected:** a `strict` option that makes the shell refuse when it can't apply an option. A shell from CLI 0.68
ignores unknown input keys and would copy anyway, so `strict` would fail open on exactly the shells it is meant to
guard. ADR-0053 records it.

### 2. iOS: the concealed marker

When `sensitive` is set, the pasteboard item also carries `"org.nspasteboard.ConcealedType": ""` next to the text,
in the same item. It is the nspasteboard.org convention that 1Password and Bitwarden write; Mac clipboard managers
honour it when the copy reaches a Mac over Universal Clipboard (i.e. `local_only` is off). iOS has no system flag;
the doc says this is a convention, not an OS guarantee. iOS then reports `sensitive: true`.

### 3. Android: no clipboard reads while the app is away

`ClipboardExpiry`'s re-check reads `primaryClipDescription` only while an activity is resumed
(`MobilerActivity.current?.get() != null`, which MainActivity already maintains). While away it just waits 5 s.
This removes the system's "Denying clipboard access" log line every 5 s, and the clear still happens within about
5 s of the user returning. No new dependency.

### 4. Web: a plain copy or share that can't run warns

On a page without `navigator.clipboard` (no secure context), the plain `cx.copy` and the `share` clipboard fallback
log `console.warn("mobiler: the clipboard needs a secure page (https or localhost)")` instead of doing nothing
silently. Needs web-sys's `console` feature. No reply exists for them (fire-and-forget), so the warning is the only
signal.

### 5. Web: "no window" is its own reason

`write_clipboard` answers `ok: false, "no window"` when there is no `window` (a worker), and keeps the secure-page
reason for a missing `navigator.clipboard`.

### 6. Conformance: multi-line comments don't count

`adr_0052_every_shell_answers_clipboard_with` (and the new ADR-0053 check) match only live code: lines inside a
`/* … */` block that spans lines, and lines inside a Swift `#if false … #endif` region, are skipped, as are whole-line
`//` comments. A small `live_code(src) -> String` helper in `xtask/tests/adr_conformance.rs`; it's applied to the
Kotlin, Swift and Rust files the ADR-0052 test reads. `kotlin_code` (used by ADR-0041) is left as it is.

### 7. Testing

- **Core:** `CopyApplied::from_response` for each shell's sensitive answer, the all-false answer, an empty output
  (`None`), `ok: false` (`None`), malformed JSON (`None`), an unknown `expires` (`None`), a missing key (`None`).
- **Web:** the `clipboard_with` success output is the all-false report (unit test on the report builder); the
  no-window reason.
- **Conformance (ADR-0053), mutation-proven:** every shell's `clipboard_with` success path builds the report (the
  three keys appear in a live line of each shell's handler block), and `live_code` drops a `/* */`-wrapped and an
  `#if false`-wrapped registration (each proven by mutating barbershop's file).
- **Runtime:** Android emulator (API 33+): `copy_sensitive` → `{"sensitive":true,"expires":"best_effort",
  "local_only":false}`, plain `copy_with` → all-false; with the app backgrounded past an expiry, logcat shows no
  "Denying clipboard access" line from the app, and the clip clears within about 5 s of returning. Web: headless
  Chrome, the report text, and the `console.warn` on an insecure page. iOS: CI compiles it; the marker and report
  need the maintainer's iPhone.

### 8. Release

1. **Libraries PR:** core 0.46 (`CopyApplied`, `CopyExpiry`), web 0.46 (report, warning, no-window reason),
   barbershop's Android + iOS (report, concealed marker, foreground-only re-check), docs (`capabilities.json` +
   `gen-readme`, library READMEs).
2. Publish core, then web (the maintainer confirms each).
3. **CLI PR:** port barbershop to the templates (byte-identical, guarded), pin core 0.46, ADR-0053 with its
   conformance test and mutation proofs, `live_code`, CLI 0.69.0, `mobiler/README.md`.

## Not doing

- **`strict` / refuse-if-unprotected** (§1, rejected).
- **Changing the request** or `cx.copy`.
- **A Kotlin test harness.** The Android pieces are checked on the emulator, as for ADR-0052.
