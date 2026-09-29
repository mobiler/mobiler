# ADR-0003: `PluginResponse` stays exactly `{ ok, output: Vec<u8> }`; a capability's richer result (HTTP status, headers, transfer events) rides inside `output`, never as new fields

Status:        Accepted
Date decided:  2026-07-19
Deciding PRs:  #192
Supersedes:    none
Code anchor:   mobiler-core/src/lib.rs (PluginResponse), mobiler-core/src/http.rs (HttpOutcome), TransferEvent
Conformance:   xtask/tests/adr_conformance.rs::adr_0003_plugin_response_has_exactly_ok_and_output

## 1. Context (The Problem)

The full-REST HTTP work needed real status codes (409 vs 500 vs offline) and response headers to
reach the core, and later streaming transfers needed progress events. The obvious move was to add
`status` and `headers` fields to `PluginResponse`. But that struct is shared by every plugin (~30),
and changing it touches every construction site on every shell and breaks the
CLI-only plugin release path (ADR-0002).

## 2. Hypothesis

If `PluginResponse` keeps only `ok` and a byte payload, and each capability defines its own typed
result (`HttpOutcome`, `TransferEvent`) encoded into that payload, then:

- domain-specific data reaches the core without widening the shared ABI;
- the ~30 existing plugins compile unchanged;
- the only ABI change needed (`output: String` → `Vec<u8>`, so binary payloads fit) is absorbed by
  convenience constructors.

### 2.1. Refutation Conditions

- **Condition 1 — no new fields.** `PluginResponse` serializes to exactly the keys `ok` and
  `output`.
  - **Validation Metric:** `adr_0003_plugin_response_has_exactly_ok_and_output` in
    `xtask/tests/adr_conformance.rs`.

## 3. Considered Options & Rationale for Refutation

- **Option A — add `status` / `headers` to `PluginResponse`** `[recorded: docs/superpowers/specs/2026-07-18-mobiler-http-capability-a-design.md, "status does NOT become a field on PluginResponse"]`
  Rejected: it leaks HTTP into an ABI every plugin shares, and breaks CLI-only plugin releases.
- **Option B — a separate HTTP effect variant** `[reconstructed]`
  Rejected by ADR-0002's reasoning: a new effect variant for each rich capability.
- **Option C — typed results inside a byte `output`** `[recorded: same spec; PR #192]`
  Chosen. `output` became `Vec<u8>`, and `PluginCall.input` stayed `String` on purpose. Changing
  `input` would touch every parser on every shell, and large uploads pass file paths.

## 4. Decision & Rationale for Corroboration

Option C. `HttpOutcome` (PR #192) and `TransferEvent` (PR #195) both ride inside `output` as bincode,
and every other plugin kept compiling.

**Mutation proof:** I added a third field, `pub status: Option<u16>`, to `PluginResponse` (and set it
in `PluginResponse::text`). `adr_0003_plugin_response_has_exactly_ok_and_output` failed: the
serialized keys were `["ok", "output", "status"]`, not `["ok", "output"]`. Reverting restored green.

## 5. Consequences (Positive and Negative Predictions)

- **Positive:** capabilities can be as rich as they need to be without an ABI change.
- **Negative:** the core must decode `output` per capability, and a shell/core version skew shows up
  as a decode error, not a compile error. Mitigated by falling back to text when the payload isn't
  the expected encoding (`http.rs`).
- **Negative:** encoding discipline becomes load-bearing (ADR-0004).
