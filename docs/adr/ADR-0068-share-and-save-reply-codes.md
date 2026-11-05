# ADR-0068: The `sharefile` and `saveimage` plugins answer a failure as `ok: false` with a stable code (`<code>` or `<code>: <detail>`) from one fixed list, and success with one word (`shared`, `saved`, `downloaded`); codes may be added, never renamed, and an app treats an unknown code as `failed`

Status:        Accepted
Date decided:  2026-11-05
Deciding PRs:  #316 (the native `sharefile` extension and the new `saveimage` plugin, CLI 0.74.0); #315 (the web shell's answers, mobiler-web 0.50.0, ADR-0067)
Supersedes:    none
Code anchor:   mobiler/plugins/sharefile/**, mobiler/plugins/saveimage/**, mobiler-web/src/lib.rs (`parse_share_ask`, `web_share_file`, `web_save_image`)
Conformance:   mobiler-web/src/lib.rs::parse_rejects, mobiler-web/src/lib.rs::abort_does_not_fall_back, mobiler/src/plugin.rs::add_bundled_saveimage_registers_activity_permission_and_plist_key

## 1. Context (The Problem)

A plugin answers with a `PluginResponse` of `ok` and `output` (ADR-0003). Until now each plugin chose its own
failure text: `sharefile` answered "no file uri", "bad uri", "no foreground activity" or a raw exception
message. An app could not tell "the user refused photo access" from "the server was down" without matching
free text that changed between releases and platforms.

The appointments app (Moj Termin admin, issue #711, 2026-11-04) asked to share and save its QR code with an
acceptance check that reads: "With the iOS permission denied it reports `ok: false` with a reason, and nothing
crashes." To word its messages ("allow photo access in Settings", "check your connection") the app must branch on
the reason. The same call also runs on three shells (Android, iOS, web, ADR-0067), which must agree on what a
reason means.

## 2. Hypothesis

If both plugins answer a failure with one code from a fixed list, optionally followed by `: <detail>` for logs,
and success with one word, then:

- an app branches on the part before the first `:` and stays correct across Android, iOS and the web;
- the detail can change freely (it is for logs), and new failure kinds can be added as new codes without
  breaking an app that treats an unknown code as `failed`.

The codes are: `bad_input`, `insecure_source`, `unsupported_source`, `fetch_failed`, `too_large`, `not_found`,
`not_an_image`, `denied`, `cancelled`, `no_activity`, `provider_missing`, `unavailable`, `failed`. The success
words are `shared` (`sharefile`: the sheet was presented on Android, the user completed it on iOS or the web),
`saved` (`saveimage`: the platform confirmed the write) and `downloaded` (the web's fallback for both).

### 2.1. Refutation Conditions

- **Condition 1 — the web maps refusals to the same codes.** `http:` → `insecure_source`, an unknown scheme →
  `unsupported_source`, no source → `bad_input`; only a dismissed sheet is `cancelled`.
  - **Validation Metric:** `parse_rejects` and `abort_does_not_fall_back` in `mobiler-web/src/lib.rs`.
- **Condition 2 — the native plugins use only listed codes.** Every `ok: false` the Kotlin and Swift sources of
  both plugins can return starts with a listed code.
  - **Validation Metric:** review of `mobiler/plugins/sharefile/**` and `mobiler/plugins/saveimage/**` (the
    native code has no unit-test harness, as for ADR-0013), plus the device checks in §4.
- **Condition 3 — an app that never installs `saveimage` gets no photo permission.** The codes `denied` and
  `not_an_image` only exist where the plugin is installed (ADR-0035).
  - **Validation Metric:** `add_bundled_saveimage_registers_activity_permission_and_plist_key` in
    `mobiler/src/plugin.rs` (a fresh scaffold carries neither `NSPhotoLibraryAddUsageDescription` nor
    `WRITE_EXTERNAL_STORAGE`; installing the plugin adds both).

## 3. Considered Options & Rationale for Refutation

- **Option A — free-text reasons, as before** `[recorded: the pre-0.74 sharefile sources]`
  Rejected: the app cannot branch on them, and they differ between platforms.
- **Option B — a JSON reply `{ok, code, detail}` in `output`** `[reconstructed]`
  Rejected: every other plugin answers a word or a short text, the app would need a parser for the simplest
  case, and ADR-0003's `ok` already carries the success bit.
- **Option C — a code, optionally `: detail`** `[recorded: spec §5]` Chosen.

## 4. Decision & Rationale for Corroboration

Option C, for these two plugins. An app matches `output.split(':').next()`. Existing bare-handle `sharefile`
callers keep `shared` / `cancelled`; the old free-text errors became codes (no app matched on them).

Device checks (barbershop "Share poster" / "Save poster", Android emulators):
- API 33: an https PNG → chooser with an image preview, `shared`; a file in the app's files dir → copied into
  `cache/share/`, chooser opens; a template without the `share/` FileProvider path → `provider_missing`, no crash;
  save → `saved`, MediaStore lists `Pictures/Fade House/fadehouse.png` as `image/png`; an HTML URL →
  `not_an_image`, MediaStore unchanged.
- API 26: the first save asks for storage access and, allowed, saves in the same call (`saved`); refused →
  `denied`, no crash; share → chooser with image targets.
- Web: see ADR-0067 §4.
- iOS: compiled by CI's barbershop lane only; no Mac was available. The Photos save, the denied path and the
  messengers' handling are left to the maintainer's iPhone check after release.

**Mutation proof:** the cited web tests were seen to fail before their code existed and under the mutations
recorded in ADR-0067 §4 ((c) `share_error_falls_back` always true failed `abort_does_not_fall_back`; (d) accepting
`http://` failed `parse_rejects`). `add_bundled_saveimage_registers_activity_permission_and_plist_key` failed
before the plugin existed ("`saveimage` is neither a plugin directory nor a bundled sample").

## 5. Consequences (Positive and Negative Predictions)

- **Positive:** an app words its message from the code, the same on every shell.
- **Positive:** a new failure kind is a new code; apps written today keep working.
- **Negative:** the native half of the contract is guarded by review and device checks, not by a test; a native
  change can return an unlisted code unnoticed.
- **Negative:** `shared` means different things per platform (Android cannot see the user's choice), as
  ADR-0013 records; the code list cannot hide that.
- **Negative:** other plugins still answer free text; this record does not extend to them.
