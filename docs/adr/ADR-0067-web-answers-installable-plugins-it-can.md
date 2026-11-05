# ADR-0067: The web shell answers an installable plugin's op itself when a browser can do the job (`transfer`, `sharefile`, `saveimage`), and answers `ok: false, "plugin '…' not available"` for every other installable plugin; nothing is installed on the web

Status:        Accepted
Date decided:  2026-11-05
Deciding PRs:  #315 (`sharefile` and `saveimage` on the web, mobiler-web 0.50.0); #195 (2026-07-20, the first instance: `transfer`)
Supersedes:    none
Code anchor:   mobiler-web/src/lib.rs (`perform`, the `sharefile` / `saveimage` branch; `parse_share_ask`, `web_share_file`, `web_save_image`; the `("transfer", …)` stream source)
Conformance:   xtask/tests/adr_conformance.rs::adr_0067_web_answers_sharefile_and_saveimage, mobiler-web/src/lib.rs::parse_rejects, mobiler-web/src/lib.rs::abort_does_not_fall_back, mobiler-web/src/lib.rs::file_name_rules

## 1. Context (The Problem)

A mobiler plugin is installed with `mobiler plugin add`. That command copies Kotlin and Swift sources into the
app's Android and iOS shells (ADR-0011). The web shell is a published crate, never copied into the app
(ADR-0023), so there is nothing for `plugin add` to copy into on the web. Until now `perform` answered every
installable plugin with `ok: false, "plugin '…' not available"`. The one exception was `transfer`, whose
upload/download stream the web shell has carried since #195 (ADR-0027).

The appointments app (Moj Termin admin, issue #711, 2026-11-04) asked to share its booking-page QR code as an image
and to save it to the phone's photos, and asked for both on the web too ("On the web: `share_file` uses the Web
Share API with files where available, else downloads. `save_image` downloads."). The maintainer chose two plugins
(`sharefile`, extended, and a new `saveimage`) over new core builders (spec
`docs/superpowers/specs/2026-11-05-share-and-save-image-design.md`). So the question was what the web does with an
installable plugin's call.

## 2. Hypothesis

If the web shell answers an installable plugin's op itself whenever a browser can do the job, using the same input
and the same reply codes as the native plugin, then:

- one app call (`cx.plugin("sharefile", "file", …)`) works on all three shells with no `cfg` and no web install
  step;
- an app reads the outcome from the reply word (`shared`, `downloaded`, `cancelled`, a code), so the browser's
  weaker abilities are visible, not hidden behind `ok: true`;
- a plugin the browser cannot serve keeps failing closed with `"plugin '…' not available"`.

### 2.1. Refutation Conditions

- **Condition 1 — the web routes the two plugins before its fallback.** `perform` handles `sharefile` and
  `saveimage` ahead of `"plugin '…' not available"`, and sends each to its handler.
  - **Validation Metric:** `adr_0067_web_answers_sharefile_and_saveimage` in `xtask/tests/adr_conformance.rs`.
- **Condition 2 — the web reads the same input and refuses what the native plugins refuse.** A bare handle or a
  `{source, …}` object; `http:` is `insecure_source`; an unknown scheme or a protocol-relative `//host` URL is
  `unsupported_source`; no source is `bad_input`.
  - **Validation Metric:** `parse_rejects` (and `parse_json_fields`, `parse_accepts_web_sources`) in
    `mobiler-web/src/lib.rs`.
- **Condition 3 — only a dismissed share sheet stops the fallback.** `AbortError` answers `cancelled`; any other
  share error (`NotAllowedError` when the tap's activation ran out during the fetch) falls back to a download.
  - **Validation Metric:** `abort_does_not_fall_back` in `mobiler-web/src/lib.rs`.
- **Condition 4 — a file name cannot carry a path.** The downloaded or shared file's name is its last segment.
  - **Validation Metric:** `file_name_rules` in `mobiler-web/src/lib.rs`.

## 3. Considered Options & Rationale for Refutation

- **Option A — keep "not available" for every installable plugin** `[recorded: issue #711 request ("On the web: …")]`
  The app would need its own web branch for the same button (`cx.open_url` to the PNG), which is the
  out-of-app workaround the request was written to remove.
- **Option B — a web half inside each plugin, installed with `plugin add`** `[reconstructed]`
  There is no place to install it: the web shell is a crate the app depends on (ADR-0023), and a plugin cannot
  add Rust code to it. Rejected as impossible without a new plugin mechanism.
- **Option C — new core builders (`cx.share_file`, `cx.save_image`) the web shell answers** `[recorded: issue #711 "Suggested additive contract"]`
  Rejected by the maintainer on 2026-11-05 for the native side: every platform capability is an opaque plugin
  effect (ADR-0002), and the photo-library permission must only reach apps that install it (ADR-0035).
- **Option D — the web answers what a browser can do, by plugin name** `[recorded: spec §6]` Chosen.

## 4. Decision & Rationale for Corroboration

Option D. The web shell's `perform` answers `sharefile/file` (Web Share API with the file where
`navigator.canShare({files})` allows it, else a download) and `saveimage/save` (a download; a browser has no photo
library). Both run after a `fetch`, so neither uses `window.open`, which a pop-up blocker treats as unrequested after
an `await` (the `browser/open` rule). A cross-origin source without CORS headers is `fetch_failed`. The reply words
are the native plugins' reply codes (spec §5), plus `downloaded` for the web's fallback.

Verified in headless Chrome over CDP on the barbershop web client: a same-origin share → `ok: true, "downloaded"`
(no `navigator.share` in headless Linux Chrome), a save → `downloaded` with the given name, `/index.html` →
`not_an_image`, a cross-origin URL without CORS → `fetch_failed: Failed to fetch`. With `navigator.share` stubbed:
resolve → `shared` (the file arrived with its name, `image/png` type and text), `AbortError` → `cancelled`,
`NotAllowedError` → `downloaded`.

**Mutation proof:** each mutation, reverted after, failed the named test:
- (a) narrowing the branch to `if call.plugin == "sharefile" {` failed
  `adr_0067_web_answers_sharefile_and_saveimage` ("ADR-0067: mobiler-web's perform() doesn't route
  `call.plugin == "saveimage"` before its not-available fallback");
- (b) moving the whole branch below the `if call.plugin != "http"` fallback failed it ("… doesn't route
  `call.plugin == "sharefile"` before its not-available fallback");
- (c) making `share_error_falls_back` return `true` for every name failed `abort_does_not_fall_back`;
- (d) skipping the `http://` refusal failed `parse_rejects`.
- Reverting restored green.

## 5. Consequences (Positive and Negative Predictions)

- **Positive:** a share or save button works on every shell from one call; the web needs no install step.
- **Positive:** the reply word tells the app what the browser actually did, so it can word its message
  ("downloaded" vs "shared").
- **Negative:** each web-served plugin is code in `mobiler-web`, so a change to it needs a library release, not
  just a CLI release, and the web and native halves can drift. Both halves read the same input and codes
  (spec §5), and review must compare them.
- **Negative:** the web's abilities are weaker and vary by browser: desktop Linux Chrome has no file sharing, and
  Safari may refuse `navigator.share` once the fetch has used up the tap's activation. Both become a download.
- **Negative:** a cross-origin `source` needs CORS headers on its server; without them the web answers
  `fetch_failed`, and the app must fall back itself (e.g. `cx.open_url`).
- **Negative:** the fetched file is held in memory as a `Blob` (up to the 50 MB cap) — fine for an image, not for
  large media.
- **Negative:** the list of web-served plugins is a judgement per plugin. A future plugin a browser can partly
  serve needs the same call recorded here.
