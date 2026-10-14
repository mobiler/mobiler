# ADR-0053: An `ok: true` from `clipboard_with` carries the protections the shell applied — `{sensitive, expires, local_only}`, never more than it did — and an app reads an empty or unknown report as unknown, never as protected; retires ADR-0052's `ok: true` Negative

Status:        Accepted
Date decided:  2026-10-14
Deciding PRs:  #295
Supersedes:    none
Code anchor:   mobiler-core/src/clipboard.rs (CopyApplied, CopyExpiry), mobiler-web/src/lib.rs (WEB_COPY_REPORT, write_clipboard), mobiler/templates/Android/app/src/main/java/__PACKAGE_PATH__/Core.kt and demos/barbershop/Android/app/src/main/java/dev/mobiler/barbershop/Core.kt (ClipboardWithPlugin, ClipboardExpiry), mobiler/templates/iOS/Sources/Core.swift and demos/barbershop/iOS/Sources/Core.swift (ClipboardWithPlugin)
Conformance:   xtask/tests/adr_conformance.rs::adr_0053_every_shell_reports_what_a_copy_got, xtask/tests/adr_conformance.rs::live_code_drops_comments_blocks_and_if_false, mobiler-core/src/clipboard.rs::each_shells_sensitive_report_parses, mobiler-core/src/clipboard.rs::an_empty_report_is_unknown, mobiler-core/src/clipboard.rs::a_failed_copy_has_no_report, mobiler-core/src/clipboard.rs::an_unknown_expiry_is_unknown, mobiler-core/src/clipboard.rs::a_malformed_or_partial_report_is_unknown, mobiler-web/src/lib.rs::the_web_reports_no_protection

## 1. Context (The Problem)

ADR-0052 made a copy with options a request to `clipboard_with`, answered with `ok: true` when the copy
was made "with every option this platform supports applied". It listed the gap as a Negative: "`ok: true`
reports the copy, not the protection. An app that must not copy a secret unprotected (on the web, or on
Android before 13, where no preview is hidden) has to decide by platform itself."

The security review of CLI 0.68 raised it as a Low: on the web, `copy_sensitive` answers `ok: true` with
nothing applied, and a security-minded app or auditor may read that as "protected". The maintainer then
asked for the deferred minors to be done `[recorded: maintainer, 2026-10-14, quoted here]`: "lets do
deferred minors".

ADR-0003 fixes `PluginResponse` as `{ ok, output }`, so anything richer rides in `output`. Today a
`clipboard_with` shell answers `ok: true` with an empty output. Shells from CLI 0.68 will keep doing that
until their app is upgraded.

## 2. Hypothesis

If every `clipboard_with` shell answers `ok: true` with
`{"sensitive": bool, "expires": "system"|"best_effort"|"none", "local_only": bool}` (every key always
present) describing what it actually did, and mobiler-core reads it with `CopyApplied::from_response`,
which returns `None` for anything it can't trust, then:
- an app can see, per copy, what this platform applied, and refuse or warn when it isn't enough;
- an older shell's empty output reads as unknown, never as protected;
- `ok: false` keeps its reason text, the request and `cx.copy` are unchanged (ADR-0007, ADR-0052).

The rules:
- **`sensitive`:** a sensitive marker was attached. It is not proof the text is hidden: on Android the
  `IS_SENSITIVE` extra hides the preview on 13+ and asks keyboards not to keep it (some, e.g. Gboard,
  honour it); on iOS the `org.nspasteboard.ConcealedType` convention, written in the **same** pasteboard
  item as the text, is honoured only by some Mac clipboard managers and only if the copy reaches a Mac
  (`local_only` off). Never on the web.
- **`expires`:** who removes the copy when its time is up (who would, not when): `"system"` (iOS
  `.expirationDate`), `"best_effort"` (Android, ADR-0052 §2), `"none"` (no expiry asked, or the web).
  A huge `expires_after` is reported like any other.
- **`local_only`:** the platform keeps the copy off the user's other devices (iOS `.localOnly`). Always
  `false` on Android and the web, which have no such switch.
- **`CopyApplied::from_response` is `None`** for `ok: false`, an empty output (a shell from CLI 0.68), a
  malformed report, a missing or mistyped key, or an `expires` this core doesn't know. Extra keys are
  ignored. `CopyExpiry` is `#[non_exhaustive]`; its "doesn't expire" variant is `Never`, so it never
  reads like `Option::None` ("unknown").

| `CopyOptions::sensitive()` | sensitive | expires | local_only |
|---|---|---|---|
| Android | `true` | `"best_effort"` | `false` |
| iOS | `true` | `"system"` | `true` |
| Web | `false` | `"none"` | `false` |

`CopyOptions::new()` gets `{"sensitive":false,"expires":"none","local_only":false}` everywhere.

### 2.1. Refutation Conditions

- **Condition 1: every shell builds the report in live code, and the core can read it.**
  - **Validation Metric:** `xtask/tests/adr_conformance.rs::adr_0053_every_shell_reports_what_a_copy_got`
    (both templates, barbershop's two shells, mobiler-web, core), using
    `live_code` (`live_code_drops_comments_blocks_and_if_false`).
- **Condition 2: unknown is never protected.** Empty, failed, malformed, partial or unknown reports parse
  to `None`; each shell's sensitive answer parses to what it applied.
  - **Validation Metric:** the five `mobiler-core/src/clipboard.rs` tests in the header.
- **Condition 3: the web claims nothing.**
  - **Validation Metric:** `mobiler-web/src/lib.rs::the_web_reports_no_protection`.
- **Condition 4 (runtime).** Checked in PR #295:
  - Android emulator API 36: `copy_sensitive` → `{"sensitive":true,"expires":"best_effort","local_only":false}`,
    a plain `copy_with` → all false/`"none"`, both parsed by the core.
  - Headless Chrome: the all-false report on a secure page; `ok: false` with the reason on an insecure one.
  - iOS is compiled by CI; the marker and the report need a device.

## 3. Considered Options & Rationale for Refutation

- **Option A: a `strict` option that makes the shell refuse when it can't apply an option**
  `[recorded: docs/superpowers/specs/2026-10-14-copy-report-design.md]`
  Rejected: a shell from CLI 0.68 ignores unknown input keys and would copy anyway, so `strict` would fail
  open on exactly the shells it is meant to guard.
- **Option B: a typed reply in place of `PluginResponse` in `then`** `[reconstructed]`
  Rejected: it changes `copy_with` / `copy_sensitive`'s signatures, which ADR-0007's keep-the-builder rule
  argues against, and an old shell would still need an "unknown" case.
- **Option C: report only the options that were asked** `[reconstructed]`
  Rejected: an app couldn't tell "not asked" from "an older shell that reports nothing". Every key is
  always present instead.
- **Option D: report `sensitive` only where the OS itself hides the text (Android 13+)**
  `[recorded: security review of PR #295]`
  Rejected: the field would hide that the marker reached keyboards on Android 8–12 and Mac clipboard
  managers from iOS. The field means "a marker was attached", and its doc says who honours it.
- **Option E: the report, read by `CopyApplied::from_response`, `None` when unknown**
  `[recorded: docs/superpowers/specs/2026-10-14-copy-report-design.md]`
  Presented in chat and approved by the maintainer ("yes, looks right"), then the spec ("yes"). Chosen.

## 4. Decision & Rationale for Corroboration

Option E. It adds to ADR-0052's reply and keeps its request, its fail-closed rule and `cx.copy`.
**Retires ADR-0052's Negative** "`ok: true` reports the copy, not the protection" (as ADR-0049 retired two of
ADR-0045's): an app now reads what was applied, and `None` tells it the shell is too old to say.

The shells change in barbershop first and are ported byte-identically to the templates in the same PR
(ADR-0015); the change uses no new core API, so ADR-0009 allows it, and ADR-0052's identity check would
fail on `main` otherwise.

**Mutation proof:** each mutation, reverted after, failed the named test with its message:
- (a) deleting the template Kotlin `.put("local_only", false)` line failed
  `adr_0053_every_shell_reports_what_a_copy_got` ("ADR-0053: mobiler/templates/…/Core.kt doesn't report
  .put("local_only", false)"), and ADR-0052's identity check;
- (b) renaming barbershop's Swift report key to `"localOnly"` failed it ("ADR-0053:
  demos/barbershop/iOS/Sources/Core.swift doesn't report what a copy got"), and ADR-0052's identity check;
- (c) changing the web success arm to `Ok(_) => PluginResponse::text(true, ""),` failed it
  ("ADR-0053: mobiler-web doesn't report");
- (f) commenting out the template Kotlin report line with `//` failed it (the same message as (a));
- `live_code` also closed two gaps in ADR-0052's test, which used a line-by-line filter: wrapping
  barbershop's Kotlin registration in `/*` … `*/` lines (d) and barbershop's Swift case in `#if false` …
  `#endif` (e) both passed `adr_0052_every_shell_answers_clipboard_with` before it used `live_code`, and
  failed it after ("ADR-0052: demos/barbershop/…/Core.kt doesn't answer clipboard_with", resp. "…Core.swift…").
- Reverting restored green.

## 5. Consequences (Positive and Negative Predictions)

- **Positive:** an app can refuse or warn when a secret's copy isn't protected enough on this platform,
  from the reply alone; an auditor can see what each platform does from one table.
- **Positive:** no wire types change; an old shell still fails closed (ADR-0052) and its success reads as
  unknown.
- **Negative:** the report is the shell's own word, not a check of the clipboard.
- **Negative:** a shell from CLI 0.68 reports nothing (`None`) until the app runs `mobiler upgrade --apply`.
- **Negative:** `sensitive: true` is a marker, not a guarantee; its strength differs per platform and
  version (§2), and on iOS with `local_only` on it has no reader.
- **Negative:** `expires: "best_effort"` on Android keeps ADR-0052's limits, sharpened here:
  - Android 10+ reads the clipboard only while an activity is resumed (`MobilerActivity.current`, set by
    MainActivity), so the clear waits for the user's return. An app showing its own other activity in
    front doesn't clear until MainActivity resumes. In split-screen without focus the read is denied and
    retried (and logged).
  - Right after the app returns, the window may not have focus yet, so the clear can take up to ~10 s.
  - Android 8–9 read from the background and clear on time.
  - A process death drops the deadline.
- **Negative:** the iOS marker is a convention, not an OS rule.
