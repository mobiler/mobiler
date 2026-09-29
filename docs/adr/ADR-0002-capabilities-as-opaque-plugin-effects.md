# ADR-0002: Every platform capability is an opaque `{plugin, op, input}` effect answered by a `PluginResponse`, so adding a capability never changes the wire ABI

Status:        Accepted
Date decided:  2026-05-25
Deciding PRs:  none (pre-PR history — commit 2442688 "Prototype: request/response capabilities")
Supersedes:    none
Code anchor:   mobiler-core/src/lib.rs (PluginCall, PluginNotify, PluginResponse, Cx::plugin / notify / subscribe), shell plugin registries (Core.kt, Core.swift, mobiler-web perform)
Conformance:   mobiler-core/src/lib.rs::plugin_response_carries_bytes_and_converts_text, mobiler-core/src/lib.rs::cx_confirm_serializes_title_message_and_routes_ok, mobiler-core/src/lib.rs::cx_snackbar_sends_a_snackbar_show_request

## 1. Context (The Problem)

Apps need dozens of device capabilities: storage, HTTP, camera, push, BLE, IAP, sensors and more.
If each one were its own variant in the core↔shell effect enum, every new capability would change
the ABI, force a lockstep release of the libraries and every shell, and leave plugins that are
optional and third-party with no way to exist.

## 2. Hypothesis

If every capability travels as the same three strings (`plugin`, `op`, `input`) and comes back as
one `PluginResponse { ok, output }`, and the shells keep a registry mapping plugin names to handlers,
then:

- a capability can be added, including one shipped only by the CLI (`mobiler plugin add`), without
  touching the ABI or publishing the libraries;
- a missing plugin degrades to `ok: false`, never a crash.

### 2.1. Refutation Conditions

- **Condition 1 — the call shape stays three strings.** A capability that needs a new effect variant
  would refute the design.
  - **Validation Metric:** the typed builders (`cx.confirm`, `cx.snackbar`, …) are tested to emit
    plain `plugin/op/input` calls, e.g. `cx_snackbar_sends_a_snackbar_show_request` in
    `mobiler-core/src/lib.rs`.
- **Condition 2 — the response stays uniform.** See ADR-0003.
- **Condition 3 — a missing plugin answers `ok: false`** for a request (a stream on a missing plugin
  simply emits nothing on Android). Not tested; verified by reading the three shells' dispatch
  (Android `Core.kt`, iOS `Core.swift`, `mobiler-web`) on 2026-09-29.

## 3. Considered Options & Rationale for Refutation

- **Option A — a typed effect per capability (Crux's usual capability model)** `[reconstructed]`
  Type-safe, but every capability becomes an ABI change and a coordinated release, and
  CLI-installable plugins become impossible.
- **Option B — opaque string-keyed calls with typed Rust helpers on top** `[recorded: commit 2442688; mobiler-core README "Each is an opaque {plugin, op, input} effect, so adding one never changes the wire ABI"]`
  Chosen.

## 4. Decision & Rationale for Corroboration

Option B. The core exposes `cx.plugin(name, op, input, then)`, `cx.notify(…)` and `cx.subscribe(…)`.
Ergonomic builders (`cx.http`, `cx.confirm_with`, `cx.snackbar`, …) wrap it, so apps still get
types. It has held for 30+ bundled plugins, with plugin releases shipping as CLI-only versions (for
example CLI 0.45 and 0.51) that didn't touch the libraries.

## 5. Consequences (Positive and Negative Predictions)

- **Positive:** a capability ships with the CLI alone, and third-party plugins are possible.
- **Negative:** the payload is stringly-typed at the boundary. A shell and a core can disagree about
  an `input` JSON shape without any compile error, which is the "plugin drift" class of bug. Each
  typed builder's tests are the defence.
- **Negative:** richer results must be encoded *inside* `output`, which needs its own discipline
  (ADR-0003, ADR-0004).
