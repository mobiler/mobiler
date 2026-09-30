# ADR-0038: Plain-HTTP (cleartext) traffic is allowed only in debug builds, and only to the local dev hosts; release builds keep Android's HTTPS-only default

Status:        Accepted
Date decided:  2026-09-30
Deciding PRs:  #256
Supersedes:    none
Code anchor:   mobiler/templates/Android/app/src/debug/AndroidManifest.xml, mobiler/templates/Android/app/src/debug/res/xml/network_security_config.xml, mobiler/templates/Android/app/src/main/AndroidManifest.xml (no cleartext attribute)
Conformance:   xtask/tests/adr_conformance.rs::adr_0038_cleartext_is_debug_only_and_local

## 1. Context (The Problem)

Android blocks plain `http://` by default. During development an app talks to a server on the
developer's machine: `http://10.0.2.2:<port>` from the emulator, or `localhost`. The scaffold
offered no way to allow that. The only worked example, the fullstack-todo demo, set
`android:usesCleartextTraffic="true"` in its *main* manifest, which allows plain HTTP to every host
in release builds too. The appointments team flagged it (malimlindoo/appointments@4a9d4972, docs/mobiler-feedback.md §2, a private repo, so quoted
here): "anyone copying the demo ships a release APK permitting plaintext HTTP."

## 2. Hypothesis

If the scaffold ships a debug-only manifest overlay (`src/debug/`) that points at a network
security config permitting cleartext only to `10.0.2.2`, `localhost` and `127.0.0.1`, and the main
manifest sets nothing, then:

- a new app reaches a local dev server over HTTP in debug builds with no edits;
- a release build keeps Android's HTTPS-only policy (enforced for the platform HTTP stack, OkHttp
  and WebView, which the shell's `http` capability uses);
- adding a LAN address for a real phone is one line in a debug-only file.

### 2.1. Refutation Conditions

- **Condition 1 — release stays HTTPS-only.** The main manifest sets neither
  `usesCleartextTraffic` nor `networkSecurityConfig`.
- **Condition 2 — debug allows only the local hosts.** The debug config has no `<base-config>`
  and permits exactly `10.0.2.2`, `localhost` and `127.0.0.1`.
  - **Validation Metric:** both, `adr_0038_cleartext_is_debug_only_and_local` in
    `xtask/tests/adr_conformance.rs`.

## 3. Considered Options & Rationale for Refutation

- **Option A — `usesCleartextTraffic="true"` in the main manifest** `[recorded: malimlindoo/appointments@4a9d4972, docs/mobiler-feedback.md §2; demos/fullstack-todo before this record]`
  Rejected: it ships to release builds and allows every host.
- **Option B — no allowance; each app works it out** `[recorded: malimlindoo/appointments@4a9d4972, docs/mobiler-feedback.md §2]`
  The scaffold's state before this record. Every app hits the block on its first local request.
- **Option C — a debug-only overlay for the local dev hosts** `[recorded: malimlindoo/appointments@4a9d4972, docs/mobiler-feedback.md §2, quoted here]`
  The team's suggested fix: "have `mobiler new` generate exactly that debug-source-set overlay
  (ideally a debug-only `network_security_config.xml` with `10.0.2.2` + `localhost` allowlisted,
  rather than a blanket allow)." Chosen. Gradle merges `src/debug/` only into debug builds.

## 4. Decision & Rationale for Corroboration

Option C. `mobiler new` writes `app/src/debug/AndroidManifest.xml` (only the
`networkSecurityConfig` attribute) and `app/src/debug/res/xml/network_security_config.xml`. The
fullstack-todo demo moved its allowance into the same overlay, since its core talks to
`http://10.0.2.2:3000` in debug.

**Mutation proof:**
- Adding `android:usesCleartextTraffic="true"` to the main manifest failed the test: "ADR-0038:
  the main manifest must not set usesCleartextTraffic".
- Adding `<base-config cleartextTrafficPermitted="true" />` to the debug config failed it:
  "ADR-0038: no app-wide cleartext, even in debug".
- Adding a non-local domain (`example.com`) failed it: "cleartext only to the local dev hosts".
- Reverting restored green.

## 5. Consequences (Positive and Negative Predictions)

- **Positive:** local development works out of the box, and a release build keeps the platform's
  HTTPS-only policy.
- **Negative:** the policy binds the platform network stacks, not raw sockets or an HTTP client
  inside the Rust core, and a dependency's manifest could still merge `usesCleartextTraffic` in.
- **Negative:** a real phone on the LAN needs its dev machine's address added by hand to the debug
  config. The file's comment says so.
- **Negative:** a debug build still talks plain HTTP to the dev hosts, so debug builds must not
  point at production data over those addresses.
- **Negative:** an existing app that already has its own `src/debug/AndroidManifest.xml` gets the
  template's as a `.mobiler-new` from `mobiler upgrade`, and must merge the one attribute by hand.
- **Negative:** iOS is not covered. App Transport Security has the same question and still has no
  scaffold allowance.
