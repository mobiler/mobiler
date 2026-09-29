# ADR-0013: A request's `ok` reports what the platform actually answered: a hand-off the platform refuses or can't route is `ok: false` with a reason — never a crash, and never an unconditional `ok: true`

Status:        Accepted
Date decided:  2026-06-02
Deciding PRs:  #71 (first instance: the `composer` plugin's `tel:` / `mailto:` result on iOS); #220 (Android `browser/open` without a handler, CLI 0.57.1); #240 (`cx.open_url_then`, honest `browser/open` on iOS and web)
Supersedes:    none
Code anchor:   template iOS Core.swift (BrowserPlugin), template Android Core.kt (BrowserPlugin), mobiler-web/src/lib.rs (perform, the `browser`/`open` branch), mobiler/plugins/composer/ios/ComposerPlugin.swift, mobiler/plugins/composer/android/ComposerPlugin.kt, mobiler-core/src/lib.rs (Cx::open_url_then)
Conformance:   none — the property is in the shells' native plugin code, which has no unit-test harness; checked by review and by the device checks recorded in PRs #220 and #240

## 1. Context (The Problem)

A `PluginResponse` carries `ok` and `output` (ADR-0003). An app branches on `ok`: it shows "this
device can't place calls" only when a call really failed. Two failures showed that the answer must
come from the platform:

- **iOS `composer`, June 2026.** "Email the shop" reported no mail app on a phone with Gmail
  installed. `canOpenURL` false-negatives for undeclared schemes. The fix used the result of
  `UIApplication.open(_:)` instead (#71).
- **Android `browser`, September 2026.** `tel:` on a device with no dialer threw
  `ActivityNotFoundException` and crashed the app (#220). At the same time iOS `browser/open`
  answered `ok: true` whatever happened, and web answered a `browser/open` request `ok: false`
  ("plugin 'browser' not available") whatever the link.

## 2. Hypothesis

If every plugin handler answers `ok` from the platform's own result (the `startActivity` outcome,
the `UIApplication.open` completion, `window.open`'s return value), and turns a platform failure
into `ok: false` with a short reason in `output`, then:

- an app can tell the user the truth ("this device can't place calls");
- a missing handler, app or permission never crashes the process.

`ok: true` means what the shell can observe, no more. For `browser/open` it means an app took the
link. For `share`, the sheet was presented. For `review`, the prompt was *requested*: the output
says `"requested"`, because the system may not show it. A shell that cannot observe the outcome
documents that limit. The web cannot tell whether a desktop can dial `tel:`.

### 2.1. Refutation Conditions

- **Condition 1 — no crash on a missing handler.** `browser/open` with no app for the scheme
  answers `ok: false, "no app can open this link"` on Android.
  - **Validation Metric:** review, plus the emulator check recorded in PR #220 (RED on the old code
    with `FATAL EXCEPTION … ActivityNotFoundException`, GREEN with `ok=false no app can open this link`).
- **Condition 2 — `ok` follows the platform result.** iOS awaits `UIApplication.shared.open` and
  web checks `window.open` for `null`. Neither answers `ok: true` unconditionally.
  - **Validation Metric:** review, plus the web CDP check recorded in PR #240 (a blocked
    `window.open` shows the failure message).
- **Condition 3 — the result reaches the app.** `open_url_then` is a request, so its `then` sees
  `ok`.
  - **Validation Metric:** review. The core side (that `open_url_then` is a request) is ADR-0007's
    conformance. That test would not catch a shell that lies, so this record cites none.

## 3. Considered Options & Rationale for Refutation

- **Option A — fire-and-forget: answer `ok: true` once the hand-off is attempted** `[recorded: docs/superpowers/specs/2026-09-29-open-url-result-design.md ("The other demos' iOS shells keep the always-ok plugin")]`
  This was the iOS `browser` plugin before #240. The app can't tell a failure from success. It
  still exists in the coffee, saldo, todo and fullstack-todo demo iOS shells, which #240 scoped out.
- **Option B — ask first (`canOpenURL`) and answer from that** `[recorded: commit 6bcf38a (PR #71): "drop canOpenURL entirely and use the async UIApplication.open(_:) — it needs no scheme whitelist and returns the real success result"]`
  Rejected: `canOpenURL` returns false for any scheme not declared in `LSApplicationQueriesSchemes`.
  It survives only as a tie-breaker *after* `open` has failed, to tell a declined `tel:` prompt
  (`"cancelled"`) from no app at all (`docs/superpowers/specs/2026-09-29-open-url-result-design.md`,
  "After review").
- **Option C — answer from the platform's result; failures are `ok: false` with a reason** `[recorded: PR #220 ("answers `ok: false` (`"no app can open this link"`) instead of crashing"); docs/superpowers/specs/2026-09-29-open-url-result-design.md, decision 2 "Honest results."]`
  Chosen.

## 4. Decision & Rationale for Corroboration

Option C. The template's `browser` plugin answers from the platform: Android `ok: true` with an
empty output, or `ok: false, "no app can open this link"`; iOS `ok: true, "opened"`, or `ok: false`
with `"invalid url"`, `"cancelled"` or `"no app can open this link"`; web `ok: true, "opened"` or
`ok: false, "blocked"`. Android's
`composer` plugin catches the exception from `startActivity` and answers `ok: false`. iOS
`composer` answers `ok: opened` from `UIApplication.shared.open`. `cx.open_url` stays a
fire-and-forget notification, and `cx.open_url_then` sends the same call as a request (ADR-0007).

Scope, as of 2026-09-29: checked by reading the template shells' built-in plugins (`browser`,
`share`, `clipboard`, `haptics`, `toast`, `snackbar`, `dialog`, `storage`) and the `composer` and
`review` plugins. The other bundled plugins under `mobiler/plugins/` were not audited for this
record. One exception was found and is not yet fixed: Android `review` answers
`ok: true, "unavailable"` when `requestReviewFlow` fails (`ReviewPlugin.kt`).

## 5. Consequences (Positive and Negative Predictions)

- **Positive:** an app can show an accurate message on failure, and a device without a dialer, mail
  app or permission never crashes it.
- **Negative:** `ok: true` means "the platform took it", not "the user finished it". A share sheet
  the user closes, or a review prompt the system suppresses, is still `ok: true`. Apps must not read
  more into it.
- **Negative:** the reason strings (`"no app can open this link"`, `"blocked"`, `"cancelled"`) are
  an informal contract that differs per shell. The iOS `"cancelled"` / "no app" split depends on
  `canOpenURL`, so it is only accurate for schemes declared in `LSApplicationQueriesSchemes`
  (the template declares `tel` and `sms`).
- **Negative:** four demo iOS shells still answer `browser/open` with an unconditional `ok: true`.
  Nothing mechanical stops a new plugin doing the same.
