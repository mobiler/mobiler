# ADR-0052: A copy with options is a request to a separate built-in `clipboard_with` capability, never an op or flag on `clipboard`, so a shell that predates it fails closed; each platform applies the options it can, and the record says which it can't

Status:        Accepted
Date decided:  2026-10-14
Deciding PRs:  #293, #294
Supersedes:    none
Code anchor:   mobiler-core/src/clipboard.rs, mobiler-core/src/lib.rs (Cx::copy_with, Cx::copy_sensitive), mobiler-web/src/lib.rs (copy_with_text, write_clipboard, perform), mobiler/templates/Android/app/src/main/java/__PACKAGE_PATH__/Core.kt and demos/barbershop/Android/app/src/main/java/dev/mobiler/barbershop/Core.kt (ClipboardWithPlugin, ClipboardExpiry), mobiler/templates/iOS/Sources/Core.swift and demos/barbershop/iOS/Sources/Core.swift (ClipboardWithPlugin)
Conformance:   xtask/tests/adr_conformance.rs::adr_0052_every_shell_answers_clipboard_with, mobiler-core/src/lib.rs::plain_copy_is_unchanged_and_options_never_go_to_clipboard, mobiler-core/src/lib.rs::copy_with_and_copy_sensitive_send_the_whole_options_object

## 1. Context (The Problem)

`cx.copy(text)` is a fire-and-forget notification `("clipboard", "copy", text)` (ADR-0007). Every
shell copies it as-is. Nothing tells the platform that the text is a secret.

The Moj Termin (appointments) admin app's audit raised it as MOB-18, part 2
`[recorded: appointments request, 2026-10-13, quoted here]`: "We copy a TOTP setup secret with
`cx.copy(secret)`. On Android 13+ the clipboard preview toast shows it, and keyboard clipboard managers
keep it." Their ask: "a `cx.copy_sensitive(text)`, or a flag on `cx.copy`, that: on Android sets
`ClipDescription.EXTRA_IS_SENSITIVE` (API 33+; `"android.content.extra.IS_SENSITIVE"` before); on iOS
uses `UIPasteboard.setItems(_:options:)` with `.localOnly` and a short `.expirationDate`." Their check:
"on Android 13+ the copy preview shows dots, not the secret."

The maintainer set the scope `[recorded: maintainer, 2026-10-14, quoted here]`: "keep in mind that we
wish that mobiler is general framework, not strictly tied to appointments app... so we always wish to
have it configurable instead exactly how appointments requests..."

Two facts shaped the wire:
- Today's Android and iOS `clipboard` handlers ignore the op and copy the input as-is.
- An app can run a newer `mobiler-core` against older `Core.kt` / `Core.swift`, e.g. when
  `mobiler upgrade` leaves them for review after a conflict (ADR-0046). Whatever the core sends, an older
  shell must not copy a secret in plain text and report success.

## 2. Hypothesis

If a copy with options is the request `("clipboard_with", "copy", <json>)`, a capability name no older
shell knows, with the input `{"text","sensitive","expires_ms"(number or null),"local_only"}` (every key
always present), then:
- a shell that predates it answers `ok: false` ("plugin 'clipboard_with' not available in this build")
  and copies nothing, on all three shells;
- `cx.copy` stays byte-identical (ADR-0007), and no options ever reach the plain `clipboard` handler;
- the options are general (`CopyOptions` builders), and appointments' ask is one preset,
  `CopyOptions::sensitive()` (sensitive, expires after 2 minutes, this device only), behind
  `cx.copy_sensitive`.

The rules:
- `cx.copy_with(text, options, then)` always replies. `ok: true` means copied, with every option this
  platform supports applied. `ok: false` with a reason means not copied at all (ADR-0013). An app must
  never fall back to `cx.copy` on `ok: false`: that is the plain copy this record exists to prevent.
- `expires_after` is clamped to at least 1 s; a duration past `u64::MAX` ms saturates. Android saturates
  a value past `Long.MAX_VALUE` instead of wrapping it into an instant clear, and iOS caps the date at
  `Date.distantFuture`.
- What each platform applies:

| Option | Android | iOS | Web |
|---|---|---|---|
| `sensitive` | `"android.content.extra.IS_SENSITIVE"` in the clip's extras (the preview shows dots on 13+; keyboards read the key before) | no equivalent | ignored |
| `expires_after` | best effort (below) | `.expirationDate`; the system removes it | ignored |
| `local_only` | no effect (no system cross-device clipboard) | `.localOnly`; kept out of Universal Clipboard | ignored |

- **Android expiry clears only what it can prove is its own.** Each copy gets a unique label
  `mobiler-copy-<uuid>`, and a newer copy replaces the deadline. At the deadline (counted on
  `SystemClock.elapsedRealtime()`, so sleep doesn't stretch it), the shell reads the clip description:
  ours → clear; someone else's → forget it; unreadable (the app is in the background, Android 10+) →
  check again every 5 s, giving up an hour after the deadline. All of it runs on the main thread.
- **Web** copies plainly with `navigator.clipboard.writeText`; `ok` follows the promise. Without a secure
  context (`navigator.clipboard` is undefined) it answers `ok: false` with the reason instead of throwing.

### 2.1. Refutation Conditions

- **Condition 1: every shell answers `clipboard_with`.** The Android and iOS templates, barbershop's
  Android and iOS shells, and `mobiler-web` all route it; `cx.copy_with` targets it.
  - **Validation Metric:** `xtask/tests/adr_conformance.rs::adr_0052_every_shell_answers_clipboard_with`.
- **Condition 2: options never reach `clipboard`, and `cx.copy` is unchanged.** A plain `cx.copy` still
  emits exactly `("clipboard", "copy", text)`; every request carrying options goes to `clipboard_with`.
  - **Validation Metric:** `mobiler-core/src/lib.rs::plain_copy_is_unchanged_and_options_never_go_to_clipboard`,
    and the `cx.copy` assertion in the conformance test.
- **Condition 3: the whole options object goes on the wire, with exact values.** The presets, each
  builder, the 1 s clamp and `null` for never.
  - **Validation Metric:** `mobiler-core/src/lib.rs::copy_with_and_copy_sensitive_send_the_whole_options_object`.
- **Condition 4 (runtime): an older shell fails closed.** With `clipboard_with` unregistered, a
  `copy_sensitive` answers `ok: false` and the clipboard keeps what it held. Checked on an Android
  emulator (API 36) in PR #293, together with: the sensitive preview shows dots; a 5 s expiry clears; a
  later plain copy is left alone; an expiry passed in the background clears after the app returns.

## 3. Considered Options & Rationale for Refutation

- **Option A: a new op on `clipboard` (`"copy_sensitive"`)** `[reconstructed]`
  Rejected: the Android and iOS `clipboard` handlers ignore the op, so an older shell would copy the
  secret in plain text and answer `ok: true`. It fails open.
- **Option B: a flag in the input (`{"text","sensitive"}` to `clipboard`)** `[reconstructed]`
  Rejected, and worse than A: an older shell copies its input as-is, so the raw JSON, secret included,
  would land on the clipboard. Appointments' "or a flag on `cx.copy`" would have had to take this path
  or change `cx.copy`'s wire (ADR-0007).
- **Option C: a fixed `cx.copy_sensitive(text)` with no options** `[recorded: maintainer, 2026-10-14, quoted here]`
  Appointments' exact ask. Rejected by the maintainer's scope (quoted in §1): the expiry, the device-only
  scope and whether to flag the copy at all belong to the app. It survives as one preset.
- **Option D: blind Android clearing at the deadline** `[reconstructed]`
  Clearing the clipboard without proving the clip is ours. Rejected: since Android 10 a backgrounded app
  can't read the clipboard, so it would wipe whatever the user copied next.
- **Option E: a separate `clipboard_with` capability, `CopyOptions` builders and a sensitive preset**
  `[recorded: docs/superpowers/specs/2026-10-14-copy-options-design.md]`
  Approved by the maintainer as the spec. Chosen.

## 4. Decision & Rationale for Corroboration

Option E. It doesn't supersede ADR-0007 (`cx.copy` is untouched) and uses only ADR-0002 / ADR-0003's
ordinary request and `PluginResponse`, so it needed no mobiler-ui release. Barbershop's shells came first
(ADR-0015), and the templates were ported from barbershop's diff in the CLI PR, after core 0.45 and web
0.45 were live (ADR-0009). The ported classes are byte-identical to barbershop's.

**Mutation proof:** each mutation, reverted after, failed
`adr_0052_every_shell_answers_clipboard_with` with its message:
- (a) deleting the template `Core.kt` registration line `"clipboard_with" to ClipboardWithPlugin(application),`
  failed it ("ADR-0052: mobiler/templates/Android/…/Core.kt doesn't answer clipboard_with");
- (b) renaming barbershop's Swift case to `"clipboard_w"` failed it ("ADR-0052:
  demos/barbershop/iOS/Sources/Core.swift doesn't answer clipboard_with");
- (c) changing the web branch to `call.plugin == "clipboard_x"` failed it ("ADR-0052: mobiler-web/src/lib.rs
  doesn't answer clipboard_with");
- (d) pointing `copy_with` at `self.plugin("clipboard", "copy", …)` failed it ("ADR-0052: cx.copy_with must
  target clipboard_with") and also failed `plain_copy_is_unchanged_and_options_never_go_to_clipboard`.
- Reverting restored green.

## 5. Consequences (Positive and Negative Predictions)

- **Positive:** an app can copy a secret so that Android 13+ hides the preview, and iOS keeps it off
  other devices and removes it after a while, with one call, and the reply says whether it happened.
- **Positive:** no wire types change, `cx.copy` is untouched, and an older shell never leaks.
- **Negative:** Android expiry is best effort. A process death before the deadline drops it (Android 13+
  itself clears the clipboard after about an hour). In the background it only clears within ~5 s of the
  user returning. An app whose clipboard is emptied by someone else keeps polling every 5 s until the hour
  is up.
- **Negative:** iOS has no sensitive flag; its protection is `local_only` and the expiry.
- **Negative:** the web honours no option: a browser can't mark, scope or expire a copy.
- **Negative:** an existing app's `copy_with` answers `ok: false` until its shells are upgraded
  (`mobiler upgrade --apply`), and the app must show that failure rather than fall back to `cx.copy`.
- **Negative:** the deadline lives only in the shell's memory, so the option cannot outlive the process
  on Android.
