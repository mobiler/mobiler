# ADR-0004: Binary payloads inside `PluginResponse.output` are encoded only through crux's `BincodeFfiFormat`; no library crate depends on `bincode` directly

Status:        Accepted
Date decided:  2026-07-19
Deciding PRs:  #192
Supersedes:    none
Code anchor:   mobiler-core/src/http.rs (HttpOutcome encode/decode), mobiler-core/Cargo.toml, mobiler-web/Cargo.toml
Conformance:   xtask/tests/adr_conformance.rs::adr_0004_no_library_crate_depends_on_bincode_directly, mobiler-core/src/http.rs::bincode_round_trips_both_variants

## 1. Context (The Problem)

ADR-0003 puts typed results inside a byte payload that the Swift and Kotlin shells decode with
serde-generated code. Crux pins bincode `=1.3` with **fixint** encoding. A direct call to bincode
2.x (`config::standard()`) produces **varint**, a different wire format. Both compile cleanly, and
the mistake only shows at runtime, as garbage decoded on a device.

## 2. Hypothesis

If every payload encode/decode goes through `crux_core::bridge::BincodeFfiFormat` (the same format
the generated decoders expect), and the library crates don't depend on `bincode` at all, then the
only way to encode is the compatible one, and the silent-garbage mistake can't be written.

### 2.1. Refutation Conditions

- **Condition 1 — no direct dependency.** Neither `mobiler-core` nor `mobiler-web` (nor
  `mobiler-ui`) declares `bincode` in its `Cargo.toml`.
  - **Validation Metric:** `adr_0004_no_library_crate_depends_on_bincode_directly` in
    `xtask/tests/adr_conformance.rs`.
- **Condition 2 — the payload round-trips.**
  - **Validation Metric:** `bincode_round_trips_both_variants` in `mobiler-core/src/http.rs`.

## 3. Considered Options & Rationale for Refutation

- **Option A — call bincode directly (2.x)** `[recorded: docs/superpowers/specs/2026-07-18-mobiler-http-capability-a-design.md]`
  Rejected: varint vs fixint makes the payload undecodable by the generated shell decoders, silently.
- **Option B — JSON inside `output`** `[recorded: docs/superpowers/specs/2026-07-18-mobiler-http-capability-a-design.md, "Bincode, not JSON-with-base64"]`
  Rejected: encoding the envelope as JSON would send the body back through base64, defeating the
  reason `output` became bytes.
- **Option C — crux's own FFI bincode format** `[recorded: same spec; PR #192]`
  Chosen.

## 4. Decision & Rationale for Corroboration

Option C. `http.rs` documents why at the call site. Payload types (`HttpOutcome`, `TransferEvent`)
are registered with typegen so the shells get matching decoders.

The test reads `cargo metadata`, so it sees a dependency however it's declared: by package name,
including renamed and target-specific declarations. It counts only normal dependencies; a test-only
`bincode` is allowed.

**Mutation proof:** each of these failed `adr_0004_no_library_crate_depends_on_bincode_directly`
(`ADR-0004: depends on bincode directly: ["mobiler-core"]`):
- `bincode = "2"` under `[dependencies]`;
- a `[dependencies.bincode]` table;
- a renamed `bc = { package = "bincode", version = "2" }`.

The same `bincode = "2"` in `mobiler-web/Cargo.toml` failed too (`["mobiler-web"]`). A
`[dev-dependencies.bincode]` passed, as intended. Reverting restored green.

## 5. Consequences (Positive and Negative Predictions)

- **Positive:** a whole class of runtime-only corruption can't be written in the libraries.
- **Negative:** we're tied to crux's bincode version and config. A crux upgrade that changes it
  would need every payload type re-verified end to end.
- **Negative:** each payload type must be registered with typegen (`codegen.rs`), or the shells
  lack a decoder. The registration also has to reach existing apps: CLI 0.48.0's `mobiler upgrade`
  failed to carry it over, and upgraded apps couldn't compile until 0.48.1 fixed it.
